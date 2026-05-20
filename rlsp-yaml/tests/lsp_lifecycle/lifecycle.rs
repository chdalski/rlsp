// SPDX-License-Identifier: MIT
use rlsp_yaml::server::Backend;
use tower_lsp::LspService;
use tower_lsp::lsp_types::{
    HoverProviderCapability, InitializeResult, OneOf, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};

use super::helpers::*;

#[tokio::test]
async fn should_complete_initialize_shutdown_lifecycle() {
    let (mut service, _socket) = LspService::new(Backend::new);

    let resp = send(&mut service, initialize_request(1)).await;
    let resp = resp.expect("initialize should return a response");
    let result: InitializeResult =
        serde_json::from_value(resp.result().expect("result missing").clone())
            .expect("failed to deserialize InitializeResult");

    assert_eq!(
        result.capabilities.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
    );
    assert_eq!(
        result.capabilities.hover_provider,
        Some(HoverProviderCapability::Simple(true))
    );
    assert!(matches!(
        result.capabilities.document_symbol_provider,
        Some(OneOf::Left(true))
    ));

    let notif_resp = send(&mut service, initialized_notification()).await;
    assert!(
        notif_resp.is_none(),
        "notifications should not return a response"
    );

    let shutdown_resp = send(&mut service, shutdown_request(2)).await;
    assert!(shutdown_resp.is_some(), "shutdown should return a response");
}
