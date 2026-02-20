use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbolParams, DocumentSymbolResponse, Hover, HoverParams, InitializeParams,
    InitializeResult, InitializedParams, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};
use tower_lsp::{Client, LanguageServer};

use crate::document_store::DocumentStore;

pub struct Backend {
    client: Client,
    document_store: Mutex<DocumentStore>,
}

impl Backend {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            document_store: Mutex::new(DocumentStore::new()),
        }
    }

    pub fn get_document_text(&self, uri: &str) -> Option<String> {
        let parsed = tower_lsp::lsp_types::Url::parse(uri).ok()?;
        let store = self.document_store.lock().ok()?;
        store.get(&parsed).map(str::to_string)
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
        if let Ok(mut store) = self.document_store.lock() {
            store.open(params.text_document.uri, params.text_document.text);
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Ok(mut store) = self.document_store.lock()
            && let Some(change) = params.content_changes.into_iter().last()
        {
            store.change(&params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Ok(mut store) = self.document_store.lock() {
            store.close(&params.text_document.uri);
        }
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(None)
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
