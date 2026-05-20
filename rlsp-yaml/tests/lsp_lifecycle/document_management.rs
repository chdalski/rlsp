// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use tower_lsp::LspService;
use tower_lsp::lsp_types::DiagnosticSeverity;

use super::helpers::*;

#[tokio::test]
async fn should_store_document_text_on_did_open() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    let text = "key: value";
    let resp = send(&mut service, did_open_notification(uri, text)).await;
    assert!(
        resp.is_none(),
        "didOpen notification should not return a response"
    );

    let backend = service.inner();
    let stored = backend.get_document_text(uri);
    assert_eq!(stored.as_deref(), Some(text));
}

#[tokio::test]
async fn should_remove_document_on_did_close() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    send(&mut service, did_open_notification(uri, "content")).await;
    send(&mut service, did_close_notification(uri)).await;

    let backend = service.inner();
    let stored = backend.get_document_text(uri);
    assert_eq!(stored, None);
}

#[tokio::test]
async fn should_publish_diagnostics_on_did_open_with_invalid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/bad.yaml";
    send(&mut service, did_open_notification(uri, "key: [bad\n")).await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[tokio::test]
async fn should_publish_empty_diagnostics_on_did_open_with_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/good.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(diags.is_empty());
}

#[tokio::test]
async fn should_update_diagnostics_on_did_change() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(diags.is_empty());
    }

    send(&mut service, did_change_notification(uri, "key: [bad\n", 2)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[tokio::test]
async fn should_clear_diagnostics_on_did_close() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    send(&mut service, did_open_notification(uri, "key: [bad\n")).await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(!diags.is_empty());
    }

    send(&mut service, did_close_notification(uri)).await;

    let diags = service.inner().get_diagnostics(uri);
    assert!(diags.is_none() || diags.as_ref().is_some_and(Vec::is_empty));
}
