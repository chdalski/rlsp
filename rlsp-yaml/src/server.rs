use std::collections::HashMap;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbolParams, DocumentSymbolResponse, Hover, HoverParams, InitializeParams,
    InitializeResult, InitializedParams, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
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
        let diagnostics = result.diagnostics.clone();

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
            document_symbol_provider: Some(tower_lsp::lsp_types::OneOf::Left(true)),
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

    async fn document_symbol(
        &self,
        _: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        Ok(None)
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
}
