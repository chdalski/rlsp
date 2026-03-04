use std::collections::HashMap;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentLink, DocumentLinkOptions, DocumentLinkParams, DocumentSymbolParams,
    DocumentSymbolResponse, FoldingRange, FoldingRangeParams, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, InitializeParams, InitializeResult,
    InitializedParams, Location, OneOf, PrepareRenameResponse, ReferenceParams, RenameOptions,
    RenameParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
    WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

use crate::document_store::DocumentStore;
use crate::parser;

pub struct Backend {
    client: Client,
    document_store: Mutex<DocumentStore>,
    diagnostics: Mutex<HashMap<Url, Vec<Diagnostic>>>,
}

impl Backend {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            document_store: Mutex::new(DocumentStore::new()),
            diagnostics: Mutex::new(HashMap::new()),
        }
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

    async fn parse_and_publish(&self, uri: Url, text: &str) {
        let result = parser::parse_yaml(text);
        let mut diagnostics = result.diagnostics.clone();

        // Run validators and combine diagnostics
        diagnostics.extend(crate::validators::validate_unused_anchors(text));
        diagnostics.extend(crate::validators::validate_flow_style(text));
        diagnostics.extend(crate::validators::validate_key_ordering(
            text,
            &result.documents,
        ));

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
            ..ServerCapabilities::default()
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: Self::capabilities(),
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let _ = &self.client;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
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

        Ok(crate::hover::hover_at(&text, yaml.as_ref(), position))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

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

        let items = crate::completion::complete_at(&text, yaml.as_ref(), position);
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

        let links = crate::document_links::find_document_links(&text);
        if links.is_empty() {
            return Ok(None);
        }

        Ok(Some(links))
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
}
