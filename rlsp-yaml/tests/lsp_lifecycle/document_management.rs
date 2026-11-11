// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::DiagnosticSeverity;

use super::code_actions::code_action_request;
use super::completion::completion_request;
use super::document_color::document_color_request;
use super::document_links::document_link_request;
use super::folding_ranges::folding_range_request;
use super::formatting_integration::{
    formatting_request_with_tab_size, range_formatting_request_with_tab_size,
};
use super::helpers::*;
use super::hover::hover_request;
use super::navigation::{definition_request, document_symbol_request, references_request};
use super::on_type_formatting::on_type_formatting_request;
use super::rename::{prepare_rename_request, rename_request};
use super::selection_ranges::selection_range_request_for;
use super::semantic_tokens::semantic_tokens_request;

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

const NEVER_OPENED: &str = "file:///test/never-opened.yaml";

/// Every handler that reads an open document answers a document the client
/// never opened with no item: `null`, or the empty array where the LSP 3.17
/// result type has no `null` (`textDocument/documentColor`).
#[rstest]
#[case::hover(hover_request(2, NEVER_OPENED, 0, 0), json!(null))]
#[case::completion(completion_request(2, NEVER_OPENED, 0, 0), json!(null))]
#[case::definition(definition_request(2, NEVER_OPENED, 0, 0), json!(null))]
#[case::references(references_request(2, NEVER_OPENED, 0, 0, true), json!(null))]
#[case::folding_range(folding_range_request(2, NEVER_OPENED), json!(null))]
#[case::document_link(document_link_request(2, NEVER_OPENED), json!(null))]
#[case::selection_range(selection_range_request_for(2, NEVER_OPENED, &[(0, 0), (1, 1)]), json!(null))]
#[case::code_action(code_action_request(2, NEVER_OPENED, 0, 1), json!(null))]
#[case::formatting(formatting_request_with_tab_size(2, NEVER_OPENED, 2), json!(null))]
#[case::range_formatting(range_formatting_request_with_tab_size(2, NEVER_OPENED, 0, 1, 2), json!(null))]
#[case::on_type_formatting(on_type_formatting_request(2, NEVER_OPENED, 1, 0, "\n"), json!(null))]
#[case::semantic_tokens(semantic_tokens_request(2, NEVER_OPENED), json!(null))]
#[case::document_symbol(document_symbol_request(2, NEVER_OPENED), json!(null))]
#[case::prepare_rename(prepare_rename_request(2, NEVER_OPENED, 0, 0), json!(null))]
#[case::rename(rename_request(2, NEVER_OPENED, 0, 0, "new"), json!(null))]
#[case::document_color(document_color_request(2, NEVER_OPENED), json!([]))]
#[tokio::test]
async fn a_document_that_was_never_opened_has_no_item_for_any_handler(
    #[case] request: Request,
    #[case] expected: serde_json::Value,
) {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let method = request.method().to_string();
    let resp = send(&mut service, request).await;
    let resp = resp.expect("the request should return a response");
    let result = resp.result().expect("the request should have a result");
    assert_eq!(result, &expected, "{method}");
}
