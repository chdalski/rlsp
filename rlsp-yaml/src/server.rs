// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CodeLens,
    CodeLensOptions, CodeLensParams, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DidChangeConfigurationParams, DidChangeTextDocumentParams,
    DidChangeWatchedFilesParams, DidChangeWatchedFilesRegistrationOptions,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentLink, DocumentLinkOptions,
    DocumentLinkParams, DocumentOnTypeFormattingOptions, DocumentOnTypeFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, FileSystemWatcher, FoldingRange,
    FoldingRangeParams, GlobPattern, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverParams, InitializeParams, InitializeResult, InitializedParams, Location, OneOf,
    PrepareRenameResponse, ReferenceParams, Registration, RenameOptions, RenameParams,
    SelectionRange, SelectionRangeParams, SelectionRangeProviderCapability, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Url, WatchKind, WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

use crate::document_store::DocumentStore;
use crate::parser;
use crate::schema::SchemaCache;

/// Workspace settings received via LSP initialization options or
/// `workspace/didChangeConfiguration`.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub custom_tags: Vec<String>,
    pub key_ordering: bool,
    /// Maps schema URL → glob pattern (upstream yaml-language-server convention).
    pub schemas: HashMap<String, String>,
}

// Lock ordering (must be acquired in this order to prevent deadlock):
//   document_store → schema_associations → schema_cache → diagnostics → settings
pub struct Backend {
    client: Client,
    document_store: Mutex<DocumentStore>,
    /// Maps document URI to the schema URL associated with that document.
    schema_associations: Mutex<HashMap<Url, String>>,
    /// In-memory schema cache shared across all documents.
    schema_cache: Mutex<SchemaCache>,
    diagnostics: Mutex<HashMap<Url, Vec<Diagnostic>>>,
    settings: Mutex<Settings>,
}

impl Backend {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            document_store: Mutex::new(DocumentStore::new()),
            schema_associations: Mutex::new(HashMap::new()),
            schema_cache: Mutex::new(SchemaCache::new()),
            diagnostics: Mutex::new(HashMap::new()),
            settings: Mutex::new(Settings::default()),
        }
    }

    pub(crate) fn get_custom_tags(&self) -> Vec<String> {
        self.settings
            .lock()
            .ok()
            .map(|s| s.custom_tags.clone())
            .unwrap_or_default()
    }

    pub(crate) fn get_key_ordering(&self) -> bool {
        self.settings.lock().ok().is_some_and(|s| s.key_ordering)
    }

    pub(crate) fn get_schema_associations(&self) -> Vec<crate::schema::SchemaAssociation> {
        self.settings
            .lock()
            .ok()
            .map(|s| {
                s.schemas
                    .iter()
                    .map(|(url, pattern)| crate::schema::SchemaAssociation {
                        pattern: pattern.clone(),
                        url: url.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_document_text(&self, uri: &str) -> Option<String> {
        let parsed = Url::parse(uri).ok()?;
        let store = self.document_store.lock().ok()?;
        store.get(&parsed).map(str::to_string)
    }

    pub fn get_diagnostics(&self, uri: &str) -> Option<Vec<Diagnostic>> {
        let parsed = Url::parse(uri).ok()?;
        let diags = self.diagnostics.lock().ok()?;
        diags.get(&parsed).cloned()
    }

    /// Normalize `schema_url`, record the association, fetch/cache the schema,
    /// and append schema-validation diagnostics to `diagnostics`.
    ///
    /// No Mutex guard is held across any `.await` point.
    async fn process_schema(
        &self,
        uri: &Url,
        schema_url: &str,
        diagnostics: &mut Vec<Diagnostic>,
        documents: &[saphyr::YamlOwned],
        text: &str,
    ) {
        let normalised = crate::schema::validate_and_normalize_url(schema_url).ok();

        if let Some(url) = normalised {
            // Record the association (lock → insert → drop).
            if let Ok(mut assoc) = self.schema_associations.lock() {
                assoc.insert(uri.clone(), url.clone());
            }

            // Check cache without holding the lock across spawn_blocking.
            let cached = self
                .schema_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&url).cloned());

            let schema = if let Some(s) = cached {
                Some(s)
            } else {
                let url_clone = url.clone();
                let join_result =
                    tokio::task::spawn_blocking(move || crate::schema::fetch_schema(&url_clone))
                        .await;
                let fetched: Option<crate::schema::JsonSchema> =
                    join_result.ok().and_then(std::result::Result::ok);

                if let Some(ref s) = fetched
                    && let Ok(mut cache) = self.schema_cache.lock()
                {
                    cache.insert(url, s.clone());
                }
                fetched
            };

            if let Some(s) = schema {
                diagnostics.extend(crate::schema_validation::validate_schema(
                    text, documents, &s,
                ));
            }
        }
    }

    async fn parse_and_publish(&self, uri: Url, text: &str) {
        let result = parser::parse_yaml(text);
        let mut diagnostics = result.diagnostics.clone();

        // Run validators and combine diagnostics
        diagnostics.extend(crate::validators::validate_unused_anchors(text));
        diagnostics.extend(crate::validators::validate_flow_style(text));
        diagnostics.extend(crate::validators::validate_duplicate_keys(text));

        // Custom tag validation: merge workspace settings tags with per-document modeline tags.
        // get_custom_tags() and get_key_ordering() acquire and release the settings lock before
        // any other lock below.
        let key_ordering = self.get_key_ordering();
        if key_ordering {
            diagnostics.extend(crate::validators::validate_key_ordering(
                text,
                &result.documents,
            ));
        }

        let mut allowed_tags: HashSet<String> = self.get_custom_tags().into_iter().collect();
        allowed_tags.extend(crate::schema::extract_custom_tags(text));
        diagnostics.extend(crate::validators::validate_custom_tags(
            text,
            &result.documents,
            &allowed_tags,
        ));

        // Schema validation: extract URL from modeline, fetch/cache schema,
        // then run schema validation against the parsed documents.
        //
        // Lock ordering: schema_associations → schema_cache (document_store
        // is not held here; diagnostics is acquired last, below).
        // No Mutex guard is held across any .await point.
        if let Some(schema_url) = crate::schema::extract_schema_url(text) {
            if schema_url.eq_ignore_ascii_case("none") {
                // $schema=none disables schema processing for this document.
                // Clear any previous association so stale schema info is not
                // carried over from a prior save.
                if let Ok(mut assoc) = self.schema_associations.lock() {
                    assoc.remove(&uri);
                }
                // Fall through: non-schema validators (anchors, flow style,
                // key ordering, duplicate keys) already ran above and their
                // diagnostics are retained.
            } else {
                self.process_schema(&uri, &schema_url, &mut diagnostics, &result.documents, text)
                    .await;
            }
        } else {
            // No modeline — check workspace associations.
            // get_schema_associations() acquires and releases the settings lock
            // before any schema_associations or schema_cache lock is taken below.
            let associations = self.get_schema_associations();
            let filename = uri.path();
            if let Some(schema_url) =
                crate::schema::match_schema_by_filename(filename, &associations)
            {
                self.process_schema(&uri, &schema_url, &mut diagnostics, &result.documents, text)
                    .await;
            }
        }

        if let Ok(mut diags) = self.diagnostics.lock() {
            diags.insert(uri.clone(), diagnostics.clone());
        }

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    #[must_use]
    pub fn capabilities() -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                ..CompletionOptions::default()
            }),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(
                tower_lsp::lsp_types::FoldingRangeProviderCapability::Simple(true),
            ),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            })),
            document_link_provider: Some(DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            code_lens_provider: Some(CodeLensOptions {
                resolve_provider: Some(false),
            }),
            document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                first_trigger_character: "\n".to_string(),
                more_trigger_character: None,
            }),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    legend: crate::semantic_tokens::legend(),
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    ..SemanticTokensOptions::default()
                }),
            ),
            ..ServerCapabilities::default()
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let settings = params
            .initialization_options
            .and_then(|v| serde_json::from_value::<Settings>(v).ok())
            .unwrap_or_default();
        if let Ok(mut s) = self.settings.lock() {
            *s = settings;
        }
        Ok(InitializeResult {
            capabilities: Self::capabilities(),
            ..InitializeResult::default()
        })
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        if let Ok(settings) = serde_json::from_value::<Settings>(params.settings)
            && let Ok(mut s) = self.settings.lock()
        {
            *s = settings;
        }
    }

    async fn initialized(&self, _: InitializedParams) {
        let watchers = vec![
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.yaml".to_string()),
                kind: Some(WatchKind::all()),
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.yml".to_string()),
                kind: Some(WatchKind::all()),
            },
        ];
        let registration = Registration {
            id: "yaml-file-watcher".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                watchers,
            })
            .ok(),
        };
        let client = self.client.clone();
        tokio::spawn(async move {
            let _ = client.register_capability(vec![registration]).await;
        });
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_watched_files(&self, _params: DidChangeWatchedFilesParams) {
        // Collect open documents, releasing the lock before any async work.
        let docs: Vec<(Url, String)> = if let Ok(store) = self.document_store.lock() {
            store.all_documents()
        } else {
            return;
        };
        for (uri, text) in docs {
            self.parse_and_publish(uri, &text).await;
        }
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        if let Ok(mut store) = self.document_store.lock() {
            store.open(uri.clone(), text.clone());
        }

        self.parse_and_publish(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            if let Ok(mut store) = self.document_store.lock() {
                store.change(&uri, change.text.clone());
            }

            self.parse_and_publish(uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        if let Ok(mut store) = self.document_store.lock() {
            store.close(&uri);
        }

        if let Ok(mut diags) = self.diagnostics.lock() {
            diags.remove(&uri);
        }

        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let (text, yaml) = if let Ok(store) = self.document_store.lock() {
            let text = store.get(&uri).map(str::to_string);
            let yaml = store.get_yaml(&uri).cloned();
            (text, yaml)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        // Lock ordering: document_store → schema_associations → schema_cache
        let schema_url = self
            .schema_associations
            .lock()
            .ok()
            .and_then(|assoc| assoc.get(&uri).cloned());

        let schema = schema_url.and_then(|url| {
            self.schema_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&url).cloned())
        });

        Ok(crate::hover::hover_at(
            &text,
            yaml.as_ref(),
            position,
            schema.as_ref(),
        ))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Lock ordering: document_store → schema_associations → schema_cache
        let (text, yaml) = if let Ok(store) = self.document_store.lock() {
            let text = store.get(&uri).map(str::to_string);
            let yaml = store.get_yaml(&uri).cloned();
            (text, yaml)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        // Retrieve the schema URL for this document (if any).
        let schema_url = self
            .schema_associations
            .lock()
            .ok()
            .and_then(|assoc| assoc.get(&uri).cloned());

        // Retrieve the cached schema (if any) — no lock held across await.
        let schema = schema_url.and_then(|url| {
            self.schema_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&url).cloned())
        });

        let items = crate::completion::complete_at(&text, yaml.as_ref(), position, schema.as_ref());
        if items.is_empty() {
            return Ok(None);
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let location = crate::references::goto_definition(&text, &uri, position);
        Ok(location.map(GotoDefinitionResponse::Scalar))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let locations =
            crate::references::find_references(&text, &uri, position, include_declaration);
        if locations.is_empty() {
            return Ok(None);
        }

        Ok(Some(locations))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let ranges = crate::folding::folding_ranges(&text);
        if ranges.is_empty() {
            return Ok(None);
        }

        Ok(Some(ranges))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let links = crate::document_links::find_document_links(&text, Some(&uri));
        if links.is_empty() {
            return Ok(None);
        }

        Ok(Some(links))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;

        let (text, marked_yaml) = if let Ok(store) = self.document_store.lock() {
            let text = store.get(&uri).map(str::to_string);
            let marked_yaml = store.get_marked_yaml(&uri).cloned();
            (text, marked_yaml)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let result =
            crate::selection::selection_ranges(&text, marked_yaml.as_ref(), &params.positions);
        if result.is_empty() {
            return Ok(None);
        }

        Ok(Some(result))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let diagnostics = self.get_diagnostics(uri.as_str()).unwrap_or_default();

        let actions = crate::code_actions::code_actions(&text, range, &diagnostics, &uri);
        if actions.is_empty() {
            return Ok(None);
        }

        Ok(Some(actions.into_iter().map(CodeAction::into).collect()))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;

        // Lock ordering: schema_associations → schema_cache
        let schema_url = self
            .schema_associations
            .lock()
            .ok()
            .and_then(|assoc| assoc.get(&uri).cloned());

        let Some(url) = schema_url else {
            return Ok(None);
        };

        let schema = self
            .schema_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&url).cloned());

        let lenses = crate::code_lens::code_lenses(&url, schema.as_ref());
        if lenses.is_empty() {
            return Ok(None);
        }

        Ok(Some(lenses))
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let ch = &params.ch;
        let tab_size = params.options.tab_size;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let edits = crate::on_type_formatting::format_on_type(&text, position, ch, tab_size);
        if edits.is_empty() {
            return Ok(None);
        }

        Ok(Some(edits))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };
        let Some(text) = text else {
            return Ok(None);
        };
        let tokens = crate::semantic_tokens::semantic_tokens(&text);
        if tokens.is_empty() {
            return Ok(None);
        }
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let (text, yaml) = if let Ok(store) = self.document_store.lock() {
            let text = store.get(&uri).map(str::to_string);
            let yaml = store.get_yaml(&uri).cloned();
            (text, yaml)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let symbols = crate::symbols::document_symbols(&text, yaml.as_ref());
        if symbols.is_empty() {
            return Ok(None);
        }

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn prepare_rename(
        &self,
        params: tower_lsp::lsp_types::TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let range = crate::rename::prepare_rename(&text, position);
        Ok(range.map(PrepareRenameResponse::Range))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let text = if let Ok(store) = self.document_store.lock() {
            store.get(&uri).map(str::to_string)
        } else {
            return Ok(None);
        };

        let Some(text) = text else {
            return Ok(None);
        };

        Ok(crate::rename::rename(&text, &uri, position, &new_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{
        HoverProviderCapability, OneOf, TextDocumentSyncCapability, TextDocumentSyncKind,
    };

    #[test]
    fn should_advertise_full_text_document_sync() {
        let caps = Backend::capabilities();

        assert_eq!(
            caps.text_document_sync,
            Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
        );
    }

    #[test]
    fn should_advertise_hover_provider() {
        let caps = Backend::capabilities();

        assert_eq!(
            caps.hover_provider,
            Some(HoverProviderCapability::Simple(true))
        );
    }

    #[test]
    fn should_advertise_document_symbol_provider() {
        let caps = Backend::capabilities();

        assert!(matches!(
            caps.document_symbol_provider,
            Some(OneOf::Left(true))
        ));
    }

    #[test]
    fn should_advertise_completion_provider() {
        let caps = Backend::capabilities();

        assert!(
            caps.completion_provider.is_some(),
            "capabilities should include completion_provider"
        );
    }

    #[test]
    fn should_advertise_definition_provider() {
        let caps = Backend::capabilities();

        assert!(
            caps.definition_provider.is_some(),
            "capabilities should include definition_provider"
        );
    }

    #[test]
    fn should_advertise_references_provider() {
        let caps = Backend::capabilities();

        assert!(
            caps.references_provider.is_some(),
            "capabilities should include references_provider"
        );
    }

    #[test]
    fn should_advertise_folding_range_provider() {
        let caps = Backend::capabilities();

        assert!(
            caps.folding_range_provider.is_some(),
            "capabilities should include folding_range_provider"
        );
    }

    #[test]
    fn should_advertise_selection_range_provider() {
        let caps = Backend::capabilities();

        assert!(
            caps.selection_range_provider.is_some(),
            "capabilities should include selection_range_provider"
        );
    }

    // Test 35
    #[test]
    fn should_advertise_rename_provider_with_prepare_support() {
        let caps = Backend::capabilities();

        assert!(
            caps.rename_provider.is_some(),
            "capabilities should include rename_provider"
        );

        if let Some(OneOf::Right(rename_opts)) = caps.rename_provider {
            assert_eq!(
                rename_opts.prepare_provider,
                Some(true),
                "rename_provider should have prepare_provider set to true"
            );
        } else {
            panic!("rename_provider should be RenameOptions with prepare_provider");
        }
    }

    #[test]
    fn should_advertise_document_link_provider() {
        let caps = Backend::capabilities();

        assert!(
            caps.document_link_provider.is_some(),
            "capabilities should include document_link_provider"
        );
    }

    // ---- Settings deserialization ----

    #[test]
    fn settings_deserializes_custom_tags_from_json() {
        let json = serde_json::json!({"customTags": ["!include", "!ref"]});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.custom_tags, vec!["!include", "!ref"]);
    }

    #[test]
    fn settings_defaults_to_empty_custom_tags_when_field_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.custom_tags.is_empty());
    }

    #[test]
    fn settings_accepts_empty_custom_tags_array() {
        let json = serde_json::json!({"customTags": []});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.custom_tags.is_empty());
    }

    #[test]
    fn settings_deserializes_key_ordering_true() {
        let json = serde_json::json!({"keyOrdering": true});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.key_ordering);
    }

    #[test]
    fn settings_defaults_key_ordering_to_false_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(!settings.key_ordering);
    }

    // ---- Schemas deserialization ----

    #[test]
    fn settings_deserializes_schemas_from_json() {
        let json = serde_json::json!({"schemas": {"https://example.com/schema.json": "*.yaml"}});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(
            settings
                .schemas
                .get("https://example.com/schema.json")
                .map(String::as_str),
            Some("*.yaml")
        );
    }

    #[test]
    fn settings_defaults_to_empty_schemas_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.schemas.is_empty());
    }

    // ---- Schema associations wiring ----

    #[test]
    fn get_schema_associations_returns_empty_by_default() {
        let (service, _) = tower_lsp::LspService::new(|client| Backend::new(client));
        let backend = service.inner();
        assert!(backend.get_schema_associations().is_empty());
    }

    #[test]
    fn get_schema_associations_converts_settings_to_vec() {
        let (service, _) = tower_lsp::LspService::new(|client| Backend::new(client));
        let backend = service.inner();

        let json = serde_json::json!({"schemas": {"https://example.com/schema.json": "*.yaml"}});
        let new_settings: Settings = serde_json::from_value(json).unwrap();
        if let Ok(mut s) = backend.settings.lock() {
            *s = new_settings;
        }

        let associations = backend.get_schema_associations();
        assert_eq!(associations.len(), 1);
        assert_eq!(associations[0].url, "https://example.com/schema.json");
        assert_eq!(associations[0].pattern, "*.yaml");
    }

    // ---- Custom tags wiring ----

    #[test]
    fn get_custom_tags_returns_empty_vec_by_default() {
        let (service, _) = tower_lsp::LspService::new(|client| Backend::new(client));
        let backend = service.inner();
        assert!(backend.get_custom_tags().is_empty());
    }

    #[test]
    fn should_advertise_code_action_provider() {
        let caps = Backend::capabilities();

        assert!(
            caps.code_action_provider.is_some(),
            "capabilities should include code_action_provider"
        );
    }

    #[test]
    fn should_advertise_code_lens_provider() {
        let caps = Backend::capabilities();
        assert!(caps.code_lens_provider.is_some());
    }

    #[test]
    fn should_advertise_on_type_formatting_provider() {
        let caps = Backend::capabilities();
        assert!(caps.document_on_type_formatting_provider.is_some());
    }

    #[test]
    fn should_advertise_semantic_tokens_provider() {
        let caps = Backend::capabilities();
        assert!(
            caps.semantic_tokens_provider.is_some(),
            "capabilities should include semantic_tokens_provider"
        );
    }
}
