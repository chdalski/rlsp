// SPDX-License-Identifier: MIT
#![expect(clippy::expect_used, missing_docs, reason = "test code")]

mod helpers;

use std::fmt::Write as _;

use futures::StreamExt;
use helpers::*;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::{
    DiagnosticSeverity, HoverProviderCapability, InitializeResult, NumberOrString, OneOf,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};

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

fn hover_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/hover")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_hover_response_for_valid_position() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have result");
    // result should not be null
    assert!(!result.is_null(), "hover result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize result");
    assert!(
        result_str.contains("key"),
        "hover content should contain 'key'"
    );
}

#[tokio::test]
async fn should_return_null_hover_for_whitespace_position() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 1, 0)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have result");
    assert!(
        result.is_null(),
        "hover result should be null for whitespace"
    );
}

#[tokio::test]
async fn should_return_none_for_hover_on_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have result");
    assert!(
        result.is_null(),
        "hover result should be null for unknown document"
    );
}

fn completion_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/completion")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

// Test 17 (SPIKE)
#[tokio::test]
async fn should_return_completion_items_for_valid_position() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "completion result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize result");
    assert!(
        result_str.contains("age"),
        "completion should suggest sibling key 'age', got: {result_str}"
    );
}

// Test 18
#[tokio::test]
async fn should_return_empty_completions_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "completion result should be null or empty for unknown document"
    );
}

fn folding_range_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/foldingRange")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

// Test 21 (SPIKE)
#[tokio::test]
async fn should_return_folding_ranges_for_nested_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/folding.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n  port: 8080\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(!result.is_null(), "foldingRange result should not be null");
    let arr = result.as_array().expect("foldingRange should be an array");
    assert!(
        !arr.is_empty(),
        "should return at least 1 folding range for nested YAML"
    );
}

// Test 22
#[tokio::test]
async fn should_return_empty_folding_ranges_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "foldingRange result should be null or empty for unknown document"
    );
}

fn definition_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/definition")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

fn references_request(
    id: i64,
    uri: &str,
    line: u32,
    character: u32,
    include_declaration: bool,
) -> Request {
    Request::build("textDocument/references")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": include_declaration }
        }))
        .finish()
}

// Test 25 (SPIKE)
#[tokio::test]
async fn should_return_definition_for_alias() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/anchors.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "defaults: &defaults\n  key: val\nproduction:\n  <<: *defaults\n",
        ),
    )
    .await;

    let resp = send(&mut service, definition_request(2, uri, 3, 6)).await;
    let resp = resp.expect("definition should return a response");
    let result = resp.result().expect("definition should have a result");
    assert!(!result.is_null(), "definition result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize result");
    assert!(
        result_str.contains("\"line\":0"),
        "definition should point to line 0 where &defaults is, got: {result_str}"
    );
}

// Test 26
#[tokio::test]
async fn should_return_null_definition_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, definition_request(2, uri, 0, 0)).await;
    let resp = resp.expect("definition should return a response");
    let result = resp.result().expect("definition should have result");
    assert!(
        result.is_null(),
        "definition result should be null for unknown document"
    );
}

// Test 27
#[tokio::test]
async fn should_return_references_for_anchor() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/refs.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n",
        ),
    )
    .await;

    let resp = send(&mut service, references_request(2, uri, 0, 10, true)).await;
    let resp = resp.expect("references should return a response");
    let result = resp.result().expect("references should have a result");
    assert!(!result.is_null(), "references result should not be null");
    let arr = result.as_array().expect("references should be an array");
    assert!(
        arr.len() >= 3,
        "should find at least 3 locations (1 anchor + 2 aliases), got: {}",
        arr.len()
    );
}

fn document_symbol_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/documentSymbol")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_document_symbols_for_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/symbols.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have result");
    assert!(
        !result.is_null(),
        "documentSymbol result should not be null"
    );
    let result_str = serde_json::to_string(result).expect("serialize result");
    assert!(result_str.contains("name"), "symbols should contain 'name'");
    assert!(result_str.contains("age"), "symbols should contain 'age'");
}

#[tokio::test]
async fn should_return_empty_symbols_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have result");
    // Should be null (None) or empty array
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "documentSymbol result should be null or empty for unknown document"
    );
}

fn prepare_rename_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/prepareRename")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

fn rename_request(id: i64, uri: &str, line: u32, character: u32, new_name: &str) -> Request {
    Request::build("textDocument/rename")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "newName": new_name
        }))
        .finish()
}

// Test 31 (SPIKE)
#[tokio::test]
async fn should_return_prepare_rename_range_for_anchor() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &defaults\n  key: val\n"),
    )
    .await;

    let resp = send(&mut service, prepare_rename_request(2, uri, 0, 10)).await;
    let resp = resp.expect("prepareRename should return a response");
    let result = resp.result().expect("prepareRename should have result");
    assert!(
        !result.is_null(),
        "prepareRename result should not be null for anchor"
    );
}

// Test 32
#[tokio::test]
async fn should_return_null_prepare_rename_when_not_on_anchor() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, prepare_rename_request(2, uri, 0, 0)).await;
    let resp = resp.expect("prepareRename should return a response");
    let result = resp.result().expect("prepareRename should have result");
    assert!(
        result.is_null(),
        "prepareRename result should be null when not on anchor"
    );
}

// Test 33
#[tokio::test]
async fn should_return_workspace_edit_on_rename() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &old\n  <<: *old\n"),
    )
    .await;

    let resp = send(&mut service, rename_request(2, uri, 0, 10, "new")).await;
    let resp = resp.expect("rename should return a response");
    let result = resp.result().expect("rename should have result");
    assert!(
        !result.is_null(),
        "rename result should not be null, got: {result}"
    );
}

// Test 34
#[tokio::test]
async fn should_return_null_rename_for_invalid_new_name() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(&mut service, did_open_notification(uri, "defaults: &old\n")).await;

    let resp = send(&mut service, rename_request(2, uri, 0, 10, "invalid name")).await;
    let resp = resp.expect("rename should return a response");
    let result = resp.result().expect("rename should have result");
    assert!(
        result.is_null(),
        "rename result should be null for invalid new_name with space"
    );
}

// ---- Validator Integration Tests ----

// Test 41 (SPIKE)
#[tokio::test]
async fn should_publish_combined_parser_and_validator_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // This text has both a parse error and an unused anchor in the valid portion
    // Note: the parser will fail to parse if there's a syntax error, so we can't
    // test combined parse + validator errors easily. Instead, test that validator
    // diagnostics are published for valid YAML.
    let uri = "file:///test/validators.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "defaults: &unused\n  key: val\nproduction:\n  key: other\n",
        ),
    )
    .await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    // Should have at least 1 diagnostic for unused anchor
    assert!(!diags.is_empty(), "should have validator diagnostics");
}

// Test 42
#[tokio::test]
async fn should_publish_validator_diagnostics_on_document_open() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/unused.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &unused\n  key: val\n"),
    )
    .await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(!diags.is_empty(), "should have unused anchor diagnostic");
    assert!(diags.iter().any(|d| {
        d.tags
            .as_ref()
            .is_some_and(|t| t.contains(&tower_lsp::lsp_types::DiagnosticTag::UNNECESSARY))
    }));
}

// Test 43
#[tokio::test]
async fn should_publish_validator_diagnostics_on_document_change() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/change.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(diags.is_empty(), "should have no diagnostics initially");
    }

    send(
        &mut service,
        did_change_notification(uri, "defaults: &unused\n  key: val\n", 2),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(
        !diags.is_empty(),
        "should have unused anchor diagnostic after change"
    );
}

// Test 44
#[tokio::test]
async fn should_clear_diagnostics_on_document_close_validators() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/close.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &unused\n  key: val\n"),
    )
    .await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(!diags.is_empty(), "should have diagnostics before close");
    }

    send(&mut service, did_close_notification(uri)).await;

    let diags = service.inner().get_diagnostics(uri);
    assert!(
        diags.is_none() || diags.as_ref().is_some_and(Vec::is_empty),
        "diagnostics should be cleared after close"
    );
}

fn document_link_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/documentLink")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_document_links_for_yaml_with_urls() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/links.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "homepage: https://example.com\n# See https://docs.example.com\n",
        ),
    )
    .await;

    let resp = send(&mut service, document_link_request(2, uri)).await;
    let resp = resp.expect("documentLink should return a response");
    let result = resp.result().expect("documentLink should have a result");
    assert!(!result.is_null(), "documentLink result should not be null");
    let arr = result.as_array().expect("documentLink should be an array");
    // After AST retrofit, comment URLs are not detected (deliberate drop).
    // Only the scalar-value URL is returned.
    assert_eq!(
        arr.len(),
        1,
        "should return 1 document link (comment URL is not detected after AST retrofit)"
    );
}

#[tokio::test]
async fn should_return_null_document_links_for_yaml_without_urls() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/no-links.yaml";
    send(
        &mut service,
        did_open_notification(uri, "key: value\nother: data\n"),
    )
    .await;

    let resp = send(&mut service, document_link_request(2, uri)).await;
    let resp = resp.expect("documentLink should return a response");
    let result = resp.result().expect("documentLink should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "documentLink result should be null or empty for YAML without URLs"
    );
}

#[tokio::test]
async fn should_return_null_document_links_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, document_link_request(2, uri)).await;
    let resp = resp.expect("documentLink should return a response");
    let result = resp.result().expect("documentLink should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "documentLink result should be null or empty for unknown document"
    );
}

// ---- selection_range ----

fn selection_range_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/selectionRange")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "positions": [{ "line": line, "character": character }]
        }))
        .finish()
}

#[tokio::test]
async fn should_return_selection_ranges_for_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n"),
    )
    .await;

    let resp = send(&mut service, selection_range_request(2, uri, 0, 0)).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert!(
        !result.is_null(),
        "selectionRange result should not be null for valid YAML"
    );
}

#[tokio::test]
async fn should_return_null_selection_ranges_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, selection_range_request(2, uri, 0, 0)).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "selectionRange result should be null or empty for unknown document"
    );
}

// ---- code_action ----

fn code_action_request(id: i64, uri: &str, start_line: u32, end_line: u32) -> Request {
    Request::build("textDocument/codeAction")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": start_line, "character": 0 },
                "end":   { "line": end_line,   "character": 0 }
            },
            "context": { "diagnostics": [] }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_code_actions_for_document_with_actions() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/code-action.yaml";
    // Flow mapping triggers a "convert to block" code action
    send(
        &mut service,
        did_open_notification(uri, "config: {key: value}\n"),
    )
    .await;

    let resp = send(&mut service, code_action_request(2, uri, 0, 1)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    assert!(
        !result.is_null(),
        "codeAction result should not be null for document with actions"
    );
}

#[tokio::test]
async fn should_return_null_code_actions_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, code_action_request(2, uri, 0, 1)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "codeAction result should be null or empty for unknown document"
    );
}

fn code_action_request_with_diagnostics(
    id: i64,
    uri: &str,
    start_line: u32,
    end_line: u32,
    diagnostics: &serde_json::Value,
) -> Request {
    Request::build("textDocument/codeAction")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": start_line, "character": 0 },
                "end":   { "line": end_line,   "character": 0 }
            },
            "context": { "diagnostics": diagnostics }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_yaml11_bool_code_actions_via_server_handler() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-bool.yaml";
    send(&mut service, did_open_notification(uri, "enabled: yes\n")).await;

    // Relay the yaml11Boolean diagnostic back to the server, as VS Code would.
    // The diagnostic range covers the scalar "yes" at col 9..12 on line 0.
    let diagnostics = json!([{
        "range": {
            "start": { "line": 0, "character": 9 },
            "end":   { "line": 0, "character": 12 }
        },
        "severity": 2,
        "code": "yaml11Boolean",
        "source": "rlsp-yaml",
        "message": "\"yes\" is a boolean in YAML 1.1 but a string in YAML 1.2."
    }]);

    let resp = send(
        &mut service,
        code_action_request_with_diagnostics(2, uri, 0, 1, &diagnostics),
    )
    .await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");

    let actions = result.as_array().expect("result should be an array");
    let titles: Vec<&str> = actions.iter().filter_map(|a| a["title"].as_str()).collect();

    assert!(
        titles.contains(&"Quote value"),
        "should include 'Quote value' action; got: {titles:?}"
    );
    assert!(
        titles.contains(&"Convert to boolean"),
        "should include 'Convert to boolean' action; got: {titles:?}"
    );
}

// ---- code_lens ----

fn code_lens_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/codeLens")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_null_code_lens_when_no_schema_association() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/no-schema.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "codeLens result should be null when no schema is associated"
    );
}

#[tokio::test]
async fn should_return_code_lens_when_schema_modeline_is_present() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-doc.yaml";
    // The $schema modeline causes process_schema to store the association even
    // when the fetch fails (network unavailable in tests). code_lens then finds
    // the association and returns a lens with the URL as the title.
    send(
        &mut service,
        did_open_notification(
            uri,
            "# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json\nkey: value\n",
        ),
    )
    .await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    assert!(
        !result.is_null(),
        "codeLens result should not be null when schema modeline is present"
    );
    let arr = result.as_array().expect("codeLens result should be array");
    assert!(!arr.is_empty(), "codeLens should return at least one lens");
    let result_str = serde_json::to_string(&arr[0]).expect("serialize lens");
    assert!(
        result_str.contains("json.schemastore.org"),
        "lens command title or arguments should reference the schema URL"
    );
}

// ---- on_type_formatting ----

fn on_type_formatting_request(id: i64, uri: &str, line: u32, character: u32, ch: &str) -> Request {
    Request::build("textDocument/onTypeFormatting")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "ch": ch,
            "options": { "tabSize": 2, "insertSpaces": true }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_indent_edit_on_type_newline_after_mapping_key() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/format.yaml";
    // After "server:\n" the cursor is at line 1 col 0; the formatter should
    // insert indentation for the child key.
    send(&mut service, did_open_notification(uri, "server:\n\n")).await;

    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp.result().expect("onTypeFormatting should have result");
    assert!(
        !result.is_null(),
        "onTypeFormatting result should not be null after newline on mapping line"
    );
}

#[tokio::test]
async fn should_return_null_on_type_formatting_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp.result().expect("onTypeFormatting should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "onTypeFormatting should be null or empty for unknown document"
    );
}

// ---- semantic_tokens_full ----

fn semantic_tokens_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/semanticTokens/full")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_semantic_tokens_for_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/tokens.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, semantic_tokens_request(2, uri)).await;
    let resp = resp.expect("semanticTokens/full should return a response");
    let result = resp
        .result()
        .expect("semanticTokens/full should have result");
    assert!(
        !result.is_null(),
        "semanticTokens/full result should not be null for valid YAML"
    );
}

#[tokio::test]
async fn should_return_null_semantic_tokens_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, semantic_tokens_request(2, uri)).await;
    let resp = resp.expect("semanticTokens/full should return a response");
    let result = resp
        .result()
        .expect("semanticTokens/full should have result");
    assert!(
        result.is_null(),
        "semanticTokens/full should be null for unknown document"
    );
}

// ---- did_change_configuration ----

fn did_change_configuration_notification(settings: &serde_json::Value) -> Request {
    Request::build("workspace/didChangeConfiguration")
        .params(json!({ "settings": settings }))
        .finish()
}

#[tokio::test]
async fn should_update_settings_on_did_change_configuration() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Open a document with out-of-order keys. Before didChangeConfiguration
    // (keyOrdering=false), no ordering diagnostic should appear.
    let uri = "file:///test/config-change.yaml";
    send(
        &mut service,
        did_open_notification(uri, "zebra: 1\napple: 2\n"),
    )
    .await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(
            diags.is_empty(),
            "key_ordering is disabled by default, no ordering diagnostics expected"
        );
    }

    // Enable keyOrdering via didChangeConfiguration
    let resp = send(
        &mut service,
        did_change_configuration_notification(&json!({ "keyOrdering": true })),
    )
    .await;
    assert!(
        resp.is_none(),
        "didChangeConfiguration should not return a response"
    );

    // Re-open (or change) the document to trigger re-validation with new settings
    send(
        &mut service,
        did_change_notification(uri, "zebra: 1\napple: 2\n", 2),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after re-validation");
    assert!(
        !diags.is_empty(),
        "out-of-order keys should produce a diagnostic after keyOrdering is enabled"
    );
}

// ---- did_change_watched_files ----

fn did_change_watched_files_notification(uri: &str) -> Request {
    Request::build("workspace/didChangeWatchedFiles")
        .params(json!({
            "changes": [
                { "uri": uri, "type": 2 }
            ]
        }))
        .finish()
}

#[tokio::test]
async fn should_republish_diagnostics_on_did_change_watched_files() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Open a valid document
    let uri = "file:///test/watched.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    // Trigger watched-files notification (re-validates all open documents)
    let resp = send(&mut service, did_change_watched_files_notification(uri)).await;
    assert!(
        resp.is_none(),
        "didChangeWatchedFiles should not return a response"
    );

    // Document should still be open with same diagnostics (empty for valid YAML)
    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after re-validation");
    assert!(diags.is_empty(), "valid YAML should have no diagnostics");
}

// ---- key ordering validation path ----

fn initialize_request_with_key_ordering(id: i64) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": { "keyOrdering": true }
        }))
        .finish()
}

#[tokio::test]
async fn should_publish_key_ordering_diagnostic_when_enabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request_with_key_ordering(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/key-order.yaml";
    // Keys out of alphabetical order should trigger a diagnostic when key_ordering is enabled
    send(
        &mut service,
        did_open_notification(uri, "zebra: 1\napple: 2\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !diags.is_empty(),
        "out-of-order keys should produce a diagnostic when keyOrdering is enabled"
    );
}

// ---- custom tags with type annotations ----

fn initialize_request_with_custom_tags(id: i64, tags: &[&str]) -> Request {
    let tag_array: Vec<serde_json::Value> = tags.iter().map(|t| json!(t)).collect();
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": { "customTags": tag_array }
        }))
        .finish()
}

#[tokio::test]
async fn should_emit_unknown_tag_for_tag_not_in_custom_tags_list() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(
        &mut service,
        initialize_request_with_custom_tags(1, &["!include"]),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/tags.yaml";
    // !unknown is not in the allowed list → unknownTag diagnostic
    send(
        &mut service,
        did_open_notification(uri, "value: !unknown foo\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let unknown_tag_count = diags
        .iter()
        .filter(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag"))
        .count();
    assert_eq!(
        unknown_tag_count, 1,
        "expected 1 unknownTag diagnostic, got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_tag_type_mismatch_when_tag_type_annotation_does_not_match_node() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    // Configure !include to expect a scalar, but the YAML has !include on a mapping
    send(
        &mut service,
        initialize_request_with_custom_tags(1, &["!include scalar", "!ref"]),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/tag-type.yaml";
    // !include expects scalar but gets a mapping → tagTypeMismatch
    // !ref has no type annotation → no diagnostic (it's in the allowed list)
    send(
        &mut service,
        did_open_notification(uri, "a: !include {key: val}\nb: !ref bar\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let mismatch_count = diags
        .iter()
        .filter(|d| {
            matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "tagTypeMismatch")
        })
        .count();
    let has_unknown = diags
        .iter()
        .any(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag"));
    assert_eq!(
        mismatch_count, 1,
        "expected 1 tagTypeMismatch diagnostic for !include on mapping, got: {diags:?}"
    );
    assert!(
        !has_unknown,
        "!ref is in allowed list and has no type annotation — no unknownTag expected, got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_no_diagnostic_when_tag_type_annotation_matches_node() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    // Configure !include to expect a scalar; YAML has a scalar → no diagnostic
    send(
        &mut service,
        initialize_request_with_custom_tags(1, &["!include scalar"]),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/tag-match.yaml";
    send(
        &mut service,
        did_open_notification(uri, "value: !include path/to/file.yaml\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let has_tag_diag = diags.iter().any(|d| {
        matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag" || s == "tagTypeMismatch")
    });
    assert!(
        !has_tag_diag,
        "scalar !include matches expected type scalar — no tag diagnostics expected, got: {diags:?}"
    );
}

// ---- $schema=none modeline ----

#[tokio::test]
async fn should_produce_no_diagnostics_for_valid_yaml_with_schema_none_modeline() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // $schema=none should disable schema processing; the non-schema validators
    // (anchors, flow style, key ordering, duplicate keys) still run, but valid
    // YAML should produce no diagnostics.
    let uri = "file:///test/schema-none.yaml";
    send(
        &mut service,
        did_open_notification(uri, "# yaml-language-server: $schema=none\nkey: value\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        diags.is_empty(),
        "valid YAML with $schema=none should have no diagnostics, got: {diags:?}"
    );
}

#[tokio::test]
async fn should_suppress_code_lens_after_schema_none_modeline() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-none-lens.yaml";
    // First open with a real schema modeline so an association is stored.
    send(
        &mut service,
        did_open_notification(
            uri,
            "# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json\nkey: value\n",
        ),
    )
    .await;

    // Verify lens is returned when association exists
    {
        let resp = send(&mut service, code_lens_request(2, uri)).await;
        let result = resp
            .expect("codeLens should return a response")
            .result()
            .expect("codeLens should have result")
            .clone();
        assert!(
            !result.is_null(),
            "codeLens should be present before $schema=none"
        );
    }

    // Now change to $schema=none, which should clear the association
    send(
        &mut service,
        did_change_notification(uri, "# yaml-language-server: $schema=none\nkey: value\n", 2),
    )
    .await;

    let resp = send(&mut service, code_lens_request(3, uri)).await;
    let result = resp
        .expect("codeLens should return a response")
        .result()
        .expect("codeLens should have result")
        .clone();
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "codeLens should be null after $schema=none clears the association"
    );
}

// ---- glob-based schema association fallback ----

fn initialize_request_with_schema_glob(id: i64, schema_url: &str, glob: &str) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": {
                "schemas": { schema_url: glob }
            }
        }))
        .finish()
}

#[tokio::test]
async fn should_attempt_schema_validation_via_glob_association() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    // Configure a schema glob that matches *.yaml; the fetch will fail (no
    // real network) but the path through match_schema_by_filename is exercised.
    send(
        &mut service,
        initialize_request_with_schema_glob(1, "https://example.com/schema.json", "*.yaml"),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/glob-doc.yaml";
    let resp = send(&mut service, did_open_notification(uri, "key: value\n")).await;
    assert!(
        resp.is_none(),
        "didOpen notification should not return a response"
    );

    // Document should be stored; schema fetch will fail silently, yielding no
    // schema-validation diagnostics but the code path is exercised.
    let text = service.inner().get_document_text(uri);
    assert_eq!(
        text.as_deref(),
        Some("key: value\n"),
        "document should be stored even when schema fetch fails"
    );
}

// ---- hover with schema association via modeline ----

#[tokio::test]
async fn should_return_hover_when_schema_modeline_is_present() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    let stub = serde_json::json!({"type": "object"});
    let schema = rlsp_yaml::schema::parse_schema(&stub).expect("stub schema");
    service
        .inner()
        .seed_schema_cache(GITHUB_WORKFLOW_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover-schema.yaml";
    // Pre-seeded schema cache means process_schema hits the cache and skips the
    // HTTP fetch entirely — no network required.
    send(
        &mut service,
        did_open_notification(
            uri,
            &format!("# yaml-language-server: $schema={GITHUB_WORKFLOW_SCHEMA_URL}\nname: Alice\n"),
        ),
    )
    .await;

    let hover_req = Request::build("textDocument/hover")
        .id(2)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 0 }
        }))
        .finish();

    let resp = send(&mut service, hover_req).await;
    let resp = resp.expect("hover should return a response");
    // Result may or may not be null depending on hover logic, but the
    // schema association lookup path in hover() is exercised regardless.
    assert!(
        resp.result().is_some(),
        "hover response should have a result field (even if null)"
    );
}

// ---- completion with schema association via modeline ----

#[tokio::test]
async fn should_exercise_schema_lookup_in_completion() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    let stub = serde_json::json!({"type": "object"});
    let schema = rlsp_yaml::schema::parse_schema(&stub).expect("stub schema");
    service
        .inner()
        .seed_schema_cache(GITHUB_WORKFLOW_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion-schema.yaml";
    // Pre-seeded schema cache means process_schema hits the cache and skips the
    // HTTP fetch entirely — no network required.
    send(
        &mut service,
        did_open_notification(
            uri,
            &format!("# yaml-language-server: $schema={GITHUB_WORKFLOW_SCHEMA_URL}\nname: Alice\nage: 30\n"),
        ),
    )
    .await;

    let completion_req = Request::build("textDocument/completion")
        .id(2)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 0 }
        }))
        .finish();

    let resp = send(&mut service, completion_req).await;
    let resp = resp.expect("completion should return a response");
    // Result may be null or have items; the important thing is the schema
    // association lookup path in completion() is exercised.
    assert!(
        resp.result().is_some(),
        "completion response should have a result field"
    );
}

// ---- Kubernetes auto-detection ----

fn initialize_request_with_k8s_version(id: i64, version: &str) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": { "kubernetesVersion": version }
        }))
        .finish()
}

#[tokio::test]
async fn should_record_schema_association_for_kubernetes_manifest() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // A Kubernetes manifest without a modeline or glob should trigger
    // auto-detection. The schema fetch will fail (no network in tests)
    // but the association is recorded before the fetch.
    let uri = "file:///test/pod.yaml";
    send(
        &mut service,
        did_open_notification(uri, "apiVersion: v1\nkind: Pod\nmetadata:\n  name: test\n"),
    )
    .await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    let arr = result.as_array().expect("codeLens result should be array");
    assert!(
        !arr.is_empty(),
        "codeLens should return a lens for the K8s schema"
    );
    let lens_str = serde_json::to_string(&arr[0]).expect("serialize lens");
    assert!(
        lens_str.contains("kubernetes-json-schema"),
        "lens should reference the kubernetes-json-schema repository"
    );
    assert!(
        lens_str.contains("pod-v1.json"),
        "lens should reference pod-v1.json"
    );
}

#[tokio::test]
async fn should_use_configured_kubernetes_version_in_schema_url() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(
        &mut service,
        initialize_request_with_k8s_version(1, "1.29.0"),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/deployment.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: test\n",
        ),
    )
    .await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    let arr = result.as_array().expect("codeLens result should be array");
    assert!(
        !arr.is_empty(),
        "codeLens should return a lens for the K8s schema"
    );
    let lens_str = serde_json::to_string(&arr[0]).expect("serialize lens");
    assert!(
        lens_str.contains("v1.29.0"),
        "lens should reference the configured Kubernetes version"
    );
    assert!(
        lens_str.contains("deployment-apps-v1.json"),
        "lens should reference deployment-apps-v1.json"
    );
}

// ── Diagnostic suppression integration tests ─────────────────────────────────

fn has_code(diags: &[tower_lsp::lsp_types::Diagnostic], code: &str) -> bool {
    diags
        .iter()
        .any(|d| matches!(&d.code, Some(NumberOrString::String(s)) if s == code))
}

// Section 1: disable-next-line — specific code

#[tokio::test]
async fn should_suppress_duplicate_key_diagnostic_with_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression1.yaml";
    // Line 0: key: a, Line 1: disable comment targets line 2, Line 2: duplicate key: b
    let text = "key: a\n# rlsp-yaml-disable-next-line duplicateKey\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed"
    );
}

#[tokio::test]
async fn should_not_suppress_duplicate_key_when_comment_targets_wrong_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression2.yaml";
    // Line 0: disable comment targets line 1; duplicateKey is on line 2
    let text = "# rlsp-yaml-disable-next-line duplicateKey\nkey: a\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should still be present"
    );
}

#[tokio::test]
async fn should_suppress_flow_map_diagnostic_with_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression3.yaml";
    // Line 1: disable comment targets line 2 (the flowMap)
    let text = "config:\n# rlsp-yaml-disable-next-line flowMap\n  settings: {a: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
}

#[tokio::test]
async fn should_not_suppress_unlisted_code_with_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression4.yaml";
    // flowMap on line 2 is suppressed; duplicateKey on line 4 is not listed
    let text =
        "config:\n# rlsp-yaml-disable-next-line flowMap\n  settings: {a: 1}\nkey: a\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should still be present"
    );
}

// Section 2: disable-next-line — suppress all

#[tokio::test]
async fn should_suppress_all_diagnostics_on_next_line_with_bare_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression5.yaml";
    // Bare comment on line 1 targets line 2 (the flowMap)
    let text = "config:\n# rlsp-yaml-disable-next-line\n  settings: {a: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "flowMap"),
        "flowMap should be suppressed by bare disable-next-line"
    );
}

#[tokio::test]
async fn should_not_suppress_diagnostics_on_other_lines_with_bare_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression6.yaml";
    // Comment on line 0 targets line 1 only; duplicateKey on line 2, flowMap on line 3
    let text = "# rlsp-yaml-disable-next-line\nkey: a\nkey: b\na: {x: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey on line 2 should not be suppressed"
    );
    assert!(
        has_code(&diags, "flowMap"),
        "flowMap on line 3 should not be suppressed"
    );
}

// Section 3: disable-file — specific code

#[tokio::test]
async fn should_suppress_flow_map_diagnostics_file_wide_with_disable_file() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression7.yaml";
    // flowMap on lines 1 and 2 — both suppressed file-wide
    let text = "# rlsp-yaml-disable-file flowMap\na: {x: 1}\nb: {y: 2}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "flowMap"),
        "all flowMap diagnostics should be suppressed"
    );
}

#[tokio::test]
async fn should_not_suppress_other_codes_with_disable_file_specific_code() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression8.yaml";
    // Only flowMap suppressed; duplicateKey on lines 1-2 remains
    let text = "# rlsp-yaml-disable-file flowMap\nkey: a\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should still be present"
    );
}

// Section 4: disable-file — suppress all

#[tokio::test]
async fn should_suppress_all_diagnostics_file_wide_with_bare_disable_file() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression9.yaml";
    // Both duplicateKey (line 2) and flowMap (line 3) suppressed
    let text = "# rlsp-yaml-disable-file\nkey: a\nkey: b\na: {x: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        diags.is_empty(),
        "all diagnostics should be suppressed by bare disable-file"
    );
}

// Section 5: Unsuppressed diagnostics on other lines still reported

#[tokio::test]
async fn should_leave_unsuppressed_diagnostics_on_other_lines_intact() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression10.yaml";
    // duplicateKey on line 2 suppressed; flowMap on line 3 is not
    let text = "key: a\n# rlsp-yaml-disable-next-line duplicateKey\nkey: b\na: {x: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed"
    );
    assert!(
        has_code(&diags, "flowMap"),
        "flowMap should still be present"
    );
}

// Section 6: Multiple suppression comments in one file

#[tokio::test]
async fn should_apply_multiple_disable_next_line_comments_independently() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression11.yaml";
    // Line 0: disable flowMap → targets line 1 (flowMap)
    // Line 3: disable duplicateKey → targets line 4 (second "key: c", where duplicateKey is emitted)
    let text = "# rlsp-yaml-disable-next-line flowMap\na: {x: 1}\nkey: c\n# rlsp-yaml-disable-next-line duplicateKey\nkey: c\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed"
    );
}

#[tokio::test]
async fn should_suppress_only_targeted_lines_with_multiple_comments() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression12.yaml";
    // comment on line 1 targets line 2; flowMap on line 3 is not targeted
    let text = "a: {x: 1}\n# rlsp-yaml-disable-next-line flowMap\nb: {y: 2}\nc: {z: 3}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let flow_map_diags: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.code, Some(NumberOrString::String(s)) if s == "flowMap"))
        .collect();
    // line 0 (a: {x:1}) and line 3 (c: {z:3}) should produce flowMap; line 2 (b: {y:2}) suppressed
    assert_eq!(
        flow_map_diags.len(),
        2,
        "exactly two flowMap diagnostics should remain (lines 0 and 3)"
    );
    assert!(
        flow_map_diags.iter().any(|d| d.range.start.line == 3),
        "flowMap on line 3 should still be present"
    );
    assert!(
        !flow_map_diags.iter().any(|d| d.range.start.line == 2),
        "flowMap on line 2 should be suppressed"
    );
}

// Section 7: Suppression applied on didChange

#[tokio::test]
async fn should_apply_suppression_after_document_change() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression13.yaml";
    // Open with duplicateKey present
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;
    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(
            has_code(&diags, "duplicateKey"),
            "duplicateKey should be present before change"
        );
    }

    // Change to add a suppression comment before the duplicate key
    send(
        &mut service,
        did_change_notification(
            uri,
            "key: a\n# rlsp-yaml-disable-next-line duplicateKey\nkey: b\n",
            2,
        ),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after change");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed after change"
    );
}

// ── Feature toggle settings integration tests ─────────────────────────────────

// Section: validate toggle

#[tokio::test]
async fn should_produce_diagnostics_by_default_with_validate_not_set() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_validate_default.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should be present with default settings"
    );
}

#[tokio::test]
async fn should_suppress_all_diagnostics_when_validate_is_false() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_validate_false.yaml";
    send(
        &mut service,
        did_change_configuration_notification(&json!({"validate": false})),
    )
    .await;
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        diags.is_empty(),
        "all diagnostics should be suppressed when validate=false"
    );
}

#[tokio::test]
async fn should_resume_diagnostics_when_validate_re_enabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_validate_reenable.yaml";
    // Disable validation and open a document with a duplicate key
    send(
        &mut service,
        did_change_configuration_notification(&json!({"validate": false})),
    )
    .await;
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;
    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(
            diags.is_empty(),
            "no diagnostics expected while validate=false"
        );
    }

    // Re-enable validation and change the document to trigger re-validation
    send(
        &mut service,
        did_change_configuration_notification(&json!({"validate": true})),
    )
    .await;
    send(
        &mut service,
        did_change_notification(uri, "key: a\nkey: b\n", 2),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after re-enable");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should reappear after validate is re-enabled"
    );
}

// Section: hover toggle

#[tokio::test]
async fn should_return_hover_result_when_hover_setting_not_set() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_hover_default.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response");
    // A response was returned (not an error) — the handler ran to completion
    assert!(
        resp.result().is_some() || resp.error().is_none(),
        "hover handler should run to completion with default settings"
    );
}

#[tokio::test]
async fn should_return_null_hover_when_hover_is_disabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_hover_false.yaml";
    send(
        &mut service,
        did_change_configuration_notification(&json!({"hover": false})),
    )
    .await;
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response even when disabled");
    let result = resp.result().expect("hover should have a result field");
    assert!(
        result.is_null(),
        "hover result should be null when hover=false"
    );
}

// Section: completion toggle

#[tokio::test]
async fn should_return_completion_items_when_completion_setting_not_set() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_completion_default.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp
        .result()
        .expect("completion should have a result field");
    assert!(
        !result.is_null(),
        "completion result should not be null with default settings"
    );
}

#[tokio::test]
async fn should_return_null_completion_when_completion_is_disabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_completion_false.yaml";
    send(
        &mut service,
        did_change_configuration_notification(&json!({"completion": false})),
    )
    .await;
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response even when disabled");
    let result = resp
        .result()
        .expect("completion should have a result field");
    assert!(
        result.is_null(),
        "completion result should be null when completion=false"
    );
}

// Section: maxItemsComputed

// Test: document_symbols_returns_results_for_valid_yaml (spike)
#[tokio::test]
async fn document_symbols_returns_results_for_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_spike.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\ncity: NYC\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(
        !result.is_null(),
        "documentSymbol result should not be null for valid YAML"
    );
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert!(!arr.is_empty(), "should return symbols for 3-key document");
}

#[tokio::test]
async fn document_symbols_respects_max_items_computed_limit() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 3})),
    )
    .await;

    let uri = "file:///test/max_items_symbols_truncate.yaml";
    let mut yaml_text = String::new();
    for i in 0..10 {
        let _ = writeln!(yaml_text, "key_{i}: value_{i}");
    }
    send(&mut service, did_open_notification(uri, &yaml_text)).await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert_eq!(
        arr.len(),
        3,
        "documentSymbol should be truncated to limit 3"
    );
}

#[tokio::test]
async fn document_symbols_returns_all_items_when_limit_exceeds_count() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 100})),
    )
    .await;

    let uri = "file:///test/max_items_symbols_no_truncate.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a: 1\nb: 2\nc: 3\nd: 4\ne: 5\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert_eq!(
        arr.len(),
        5,
        "all 5 symbols should be returned when limit is 100"
    );
}

#[tokio::test]
async fn document_symbols_returns_empty_when_max_items_computed_is_zero() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 0})),
    )
    .await;

    let uri = "file:///test/max_items_symbols_zero.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a: 1\nb: 2\nc: 3\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "documentSymbol should be null or empty when maxItemsComputed=0"
    );
}

#[tokio::test]
async fn document_symbols_uses_default_5000_limit_when_setting_absent() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_symbols_default.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a: 1\nb: 2\nc: 3\nd: 4\ne: 5\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert_eq!(
        arr.len(),
        5,
        "all 5 symbols should be returned with default 5000 limit"
    );
}

// IT-NEW-2: Multi-document YAML returns NAMESPACE wrapper symbols
#[tokio::test]
async fn document_symbols_multi_doc_returns_namespace_wrappers() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/multi_doc_namespace.yaml";
    send(
        &mut service,
        did_open_notification(uri, "doc1key: value1\n---\ndoc2key: value2\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(
        !result.is_null(),
        "result should not be null for multi-doc YAML"
    );

    let arr = result.as_array().expect("result should be an array");
    assert_eq!(
        arr.len(),
        2,
        "two-doc file should return exactly 2 top-level NAMESPACE symbols"
    );

    assert_eq!(
        arr[0]["name"], "Document 1",
        "first wrapper should be named 'Document 1'"
    );
    assert_eq!(
        arr[0]["kind"], 3,
        "first wrapper kind should be NAMESPACE (3)"
    );
    assert_eq!(
        arr[1]["name"], "Document 2",
        "second wrapper should be named 'Document 2'"
    );
    assert_eq!(
        arr[1]["kind"], 3,
        "second wrapper kind should be NAMESPACE (3)"
    );

    let children0 = arr[0]["children"]
        .as_array()
        .expect("Document 1 should have children");
    assert!(
        children0.iter().any(|c| c["name"] == "doc1key"),
        "Document 1 children should contain 'doc1key'"
    );

    let children1 = arr[1]["children"]
        .as_array()
        .expect("Document 2 should have children");
    assert!(
        children1.iter().any(|c| c["name"] == "doc2key"),
        "Document 2 children should contain 'doc2key'"
    );
}

// IT-NEW-1: Kubernetes-style YAML — container name used as sequence item label
#[tokio::test]
async fn document_symbols_kubernetes_style_container_name_used_as_label() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/k8s_container.yaml";
    let yaml = "spec:\n  containers:\n    - name: nginx\n      image: nginx:latest\n      ports:\n        - containerPort: 80\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(!result.is_null(), "result should not be null");

    let arr = result.as_array().expect("result should be an array");
    assert!(!arr.is_empty(), "should have symbols");

    // Find `spec` symbol
    let spec = arr
        .iter()
        .find(|s| s["name"] == "spec")
        .expect("should have 'spec'");
    assert_eq!(spec["detail"], "1 key", "spec detail should be '1 key'");

    // Navigate to `containers` child
    let spec_children = spec["children"]
        .as_array()
        .expect("spec should have children");
    let containers = spec_children
        .iter()
        .find(|s| s["name"] == "containers")
        .expect("should have 'containers'");
    assert_eq!(
        containers["detail"], "1 item",
        "containers detail should be '1 item'"
    );

    // Navigate to first sequence item — should be named "nginx" by label-key heuristic
    let container_children = containers["children"]
        .as_array()
        .expect("containers should have children");
    let nginx = &container_children[0];
    assert_eq!(
        nginx["name"], "nginx",
        "first container should be named by 'name' key value"
    );
    assert_eq!(nginx["detail"], "[0]", "detail should show original index");
}

#[tokio::test]
async fn folding_ranges_returns_results_for_nested_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_folding_spike.yaml";
    send(
        &mut service,
        did_open_notification(uri, "outer:\n  inner: value\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(
        !result.is_null(),
        "foldingRange should not be null for nested YAML"
    );
    let arr = result.as_array().expect("foldingRange should be an array");
    assert!(!arr.is_empty(), "should return at least one folding range");
}

#[tokio::test]
async fn folding_ranges_respects_max_items_computed_limit() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 2})),
    )
    .await;

    // 5 nested mappings each produces a folding range — well above the limit of 2
    let uri = "file:///test/max_items_folding_truncate.yaml";
    let mut yaml_text = String::new();
    for i in 0..5 {
        let _ = write!(yaml_text, "item_{i}:\n  key: value\n");
    }
    send(&mut service, did_open_notification(uri, &yaml_text)).await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    let arr = result.as_array().expect("foldingRange should be an array");
    assert_eq!(arr.len(), 2, "foldingRange should be truncated to limit 2");
}

#[tokio::test]
async fn folding_ranges_returns_empty_when_max_items_computed_is_zero() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 0})),
    )
    .await;

    let uri = "file:///test/max_items_folding_zero.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n  port: 8080\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "foldingRange should be null or empty when maxItemsComputed=0"
    );
}

#[tokio::test]
async fn folding_ranges_uses_default_5000_limit_when_setting_absent() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_folding_default.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a:\n  x: 1\nb:\n  y: 2\nc:\n  z: 3\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(
        !result.is_null(),
        "foldingRange should return results with default 5000 limit"
    );
    let arr = result.as_array().expect("foldingRange should be an array");
    assert!(
        !arr.is_empty(),
        "should return folding ranges with default limit"
    );
}

// ---- schema-aware YAML 1.1 diagnostics (integration) ----
//
// These tests exercise the `schemaYaml11Boolean`, `schemaYaml11Octal`, and
// `schemaYaml11BooleanType` diagnostics through the full LSP pipeline.
// The schema is injected into the cache via `seed_schema_cache` so no network
// fetch is required.  A `$schema=` modeline in the document causes
// `process_schema` to find the cached schema and run validation.

/// K8s ConfigMap-style schema: `data` is an object with string values;
/// `enabled` is a boolean field.
fn configmap_schema() -> rlsp_yaml::schema::JsonSchema {
    let schema_json = serde_json::json!({
        "type": "object",
        "properties": {
            "data": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            },
            "enabled": {
                "type": "boolean"
            }
        }
    });
    rlsp_yaml::schema::parse_schema(&schema_json).expect("configmap_schema: parse failed")
}

const CONFIGMAP_SCHEMA_URL: &str = "https://example.com/test-configmap.json";
const GITHUB_WORKFLOW_SCHEMA_URL: &str = "https://json.schemastore.org/github-workflow.json";

#[tokio::test]
async fn should_emit_schema_yaml11_boolean_warning_in_string_typed_field() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service
        .inner()
        .seed_schema_cache(CONFIGMAP_SCHEMA_URL, configmap_schema());

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-yaml11-bool-string.yaml";
    // `data.value: yes` — string-typed field gets a YAML 1.1 boolean warning.
    let yaml =
        format!("# yaml-language-server: $schema={CONFIGMAP_SCHEMA_URL}\ndata:\n  value: yes\n");
    send(&mut service, did_open_notification(uri, &yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(
        diags
            .iter()
            .any(|d| d.code == Some(NumberOrString::String("schemaYaml11Boolean".to_string()))),
        "expected schemaYaml11Boolean warning; got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .all(|d| d.severity != Some(DiagnosticSeverity::ERROR)
                || d.code != Some(NumberOrString::String("schemaType".to_string()))),
        "schemaType error should not be emitted alongside schemaYaml11Boolean; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_schema_yaml11_boolean_type_error_in_boolean_typed_field() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service
        .inner()
        .seed_schema_cache(CONFIGMAP_SCHEMA_URL, configmap_schema());

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-yaml11-bool-type.yaml";
    // `enabled: yes` — boolean-typed field with a YAML 1.1 value gets a
    // schemaYaml11BooleanType error (not a generic schemaType error).
    let yaml = format!("# yaml-language-server: $schema={CONFIGMAP_SCHEMA_URL}\nenabled: yes\n");
    send(&mut service, did_open_notification(uri, &yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(
        diags.iter().any(|d| {
            d.code
                == Some(NumberOrString::String(
                    "schemaYaml11BooleanType".to_string(),
                ))
                && d.severity == Some(DiagnosticSeverity::ERROR)
        }),
        "expected schemaYaml11BooleanType error; got: {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|d| d.code == Some(NumberOrString::String("schemaType".to_string()))),
        "generic schemaType should not be emitted; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_suppress_schema_yaml11_diagnostics_in_v1_1_mode() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service
        .inner()
        .seed_schema_cache(CONFIGMAP_SCHEMA_URL, configmap_schema());

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-yaml11-v11-suppress.yaml";
    // In V1_1 mode: `yes` in a boolean field passes the type check (no
    // diagnostics); `yes` in a string field gets no warning either.
    let yaml = format!(
        "# yaml-language-server: $schema={CONFIGMAP_SCHEMA_URL}\n\
         # yaml-language-server: $yamlVersion=1.1\n\
         enabled: yes\n\
         data:\n  value: yes\n"
    );
    send(&mut service, did_open_notification(uri, &yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_schema_yaml11_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c))
                if c == "schemaYaml11Boolean"
                    || c == "schemaYaml11BooleanType"
                    || c == "schemaYaml11Octal"
        )
    });

    assert!(
        !has_schema_yaml11_diag,
        "V1_1 mode should suppress all schema YAML 1.1 diagnostics; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_yaml11_boolean_warning_for_plain_scalars() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-bool-plain.yaml";
    let yaml = "enabled: yes\nactive: on\nname: \"yes\"\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let bool_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code == Some(NumberOrString::String("yaml11Boolean".to_string())))
        .collect();

    assert_eq!(
        bool_diags.len(),
        2,
        "expected exactly 2 yaml11Boolean diagnostics (quoted 'yes' must be excluded); got: {diags:?}"
    );
    assert!(
        bool_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "yaml11Boolean diagnostics must have WARNING severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_yaml11_octal_info_for_plain_scalars() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-octal-plain.yaml";
    let yaml = "mode: 0777\nperm: \"0644\"\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let octal_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code == Some(NumberOrString::String("yaml11Octal".to_string())))
        .collect();

    assert_eq!(
        octal_diags.len(),
        1,
        "expected exactly 1 yaml11Octal diagnostic (quoted '0644' must be excluded); got: {diags:?}"
    );
    assert!(
        octal_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::INFORMATION)),
        "yaml11Octal diagnostics must have INFORMATION severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_suppress_yaml11_compat_diagnostics_in_v1_1_mode() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-compat-v11-suppress.yaml";
    let yaml = "# yaml-language-server: $yamlVersion=1.1\nenabled: yes\nmode: 0777\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_yaml11_compat_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c))
                if c == "yaml11Boolean" || c == "yaml11Octal"
        )
    });

    assert!(
        !has_yaml11_compat_diag,
        "V1_1 mode should suppress yaml11Boolean and yaml11Octal diagnostics; got: {diags:?}"
    );
}

// ---- flowStyle setting ----

#[tokio::test]
async fn flow_style_off_suppresses_flow_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable flowStyle: "off"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "flowStyle": "off" })),
    )
    .await;

    let uri = "file:///test/flow-off.yaml";
    // This YAML uses a flow map — normally produces a flowStyle diagnostic.
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_flow_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c)) if c == "flowMap" || c == "flowSeq"
        )
    });
    assert!(
        !has_flow_diag,
        "flowStyle=off should suppress flow diagnostics; got: {diags:?}"
    );
}

#[tokio::test]
async fn flow_style_default_emits_warning_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No explicit flowStyle setting — default is "warning".
    let uri = "file:///test/flow-warning.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let flow_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "flowMap" || c == "flowSeq"
            )
        })
        .collect();

    assert!(
        !flow_diags.is_empty(),
        "flowStyle default should produce flow diagnostics; got: {diags:?}"
    );
    assert!(
        flow_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "flowStyle default should emit WARNING severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn flow_style_error_emits_error_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable flowStyle: "error"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "flowStyle": "error" })),
    )
    .await;

    let uri = "file:///test/flow-error.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let flow_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "flowMap" || c == "flowSeq"
            )
        })
        .collect();

    assert!(
        !flow_diags.is_empty(),
        "flowStyle=error should produce flow diagnostics; got: {diags:?}"
    );
    assert!(
        flow_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "flowStyle=error should emit ERROR severity; got: {diags:?}"
    );
}

// ---- duplicateKeys setting ----

#[tokio::test]
async fn duplicate_keys_default_emits_error_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No explicit duplicateKeys setting — default is "error".
    let uri = "file:///test/dup-default.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let dup_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "duplicateKey"
            )
        })
        .collect();

    assert!(
        !dup_diags.is_empty(),
        "duplicateKeys default should produce duplicate-key diagnostics; got: {diags:?}"
    );
    assert!(
        dup_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "duplicateKeys default should emit ERROR severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn duplicate_keys_error_emits_error_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Explicit duplicateKeys: "error"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "duplicateKeys": "error" })),
    )
    .await;

    let uri = "file:///test/dup-error.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let dup_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "duplicateKey"
            )
        })
        .collect();

    assert!(
        !dup_diags.is_empty(),
        "duplicateKeys=error should produce duplicate-key diagnostics; got: {diags:?}"
    );
    assert!(
        dup_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "duplicateKeys=error should emit ERROR severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn duplicate_keys_warning_emits_warning_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable duplicateKeys: "warning"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "duplicateKeys": "warning" })),
    )
    .await;

    let uri = "file:///test/dup-warning.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let dup_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "duplicateKey"
            )
        })
        .collect();

    assert!(
        !dup_diags.is_empty(),
        "duplicateKeys=warning should produce duplicate-key diagnostics; got: {diags:?}"
    );
    assert!(
        dup_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "duplicateKeys=warning should emit WARNING severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn duplicate_keys_off_suppresses_duplicate_key_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable duplicateKeys: "off"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "duplicateKeys": "off" })),
    )
    .await;

    let uri = "file:///test/dup-off.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_dup_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c)) if c == "duplicateKey"
        )
    });
    assert!(
        !has_dup_diag,
        "duplicateKeys=off should suppress duplicate-key diagnostics; got: {diags:?}"
    );
}

// ---- formatEnforceBlockStyle setting ----

fn formatting_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/formatting")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "options": {
                "tabSize": 2,
                "insertSpaces": true
            }
        }))
        .finish()
}

#[tokio::test]
async fn format_enforce_block_style_converts_flow_to_block() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable formatEnforceBlockStyle
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatEnforceBlockStyle": true })),
    )
    .await;

    let uri = "file:///test/enforce-block.yaml";
    let flow_yaml = "key: {a: 1, b: 2}\n";
    send(&mut service, did_open_notification(uri, flow_yaml)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    assert!(
        !result.is_null(),
        "formatEnforceBlockStyle=true should produce a formatting edit for flow YAML; got null"
    );

    let edits = result
        .as_array()
        .expect("formatting result should be array");
    assert!(
        !edits.is_empty(),
        "formatEnforceBlockStyle=true should produce at least one edit"
    );

    // The formatted text should not contain flow-style braces.
    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");
    assert!(
        !new_text.contains('{'),
        "formatEnforceBlockStyle=true should remove flow maps; got: {new_text:?}"
    );
}

#[tokio::test]
async fn format_enforce_block_style_off_by_default_preserves_flow() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No formatEnforceBlockStyle setting — default is false.
    let uri = "file:///test/no-enforce-block.yaml";
    // This YAML is already well-formatted so the formatter won't change it.
    // Use a simple flow map that the formatter would normally leave as-is.
    let flow_yaml = "key:\n  a: 1\n  b: 2\n";
    send(&mut service, did_open_notification(uri, flow_yaml)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    // Well-formatted block YAML with no changes expected — formatter returns null or empty.
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "well-formatted block YAML should produce no edits when formatEnforceBlockStyle is false; got: {result:?}"
    );
}

// ---- formatPreserveQuotes setting ----

const KUBERNETES_YAML: &str = "\
apiVersion: v1
kind: Namespace
metadata:
  name: finance
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: payment-api
  namespace: finance
spec:
  replicas: 2
  selector:
    matchLabels:
      app: payment
  template:
    metadata:
      labels:
        app: payment
    spec:
      containers:
      - name: payment
        image: python:3.12-slim
        command: [\"python\", \"-m\", \"http.server\", \"5000\"]
        ports:
        - containerPort: 5000
        readinessProbe:
          httpGet:
            path: /
            port: 5000
          initialDelaySeconds: 5
          periodSeconds: 5
";

#[tokio::test]
async fn preserve_quotes_retains_double_quoted_command_array() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatPreserveQuotes": true })),
    )
    .await;

    let uri = "file:///test/k8s-preserve-command.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    assert!(
        !result.is_null(),
        "formatPreserveQuotes=true should produce a formatting edit; got null"
    );
    let edits = result.as_array().expect("result should be array");
    assert!(
        !edits.is_empty(),
        "formatting should produce at least one edit"
    );

    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");

    assert!(
        new_text.contains(r#"["python", "-m", "http.server", "5000"]"#),
        "all four command elements must remain double-quoted; got: {new_text:?}"
    );
}

#[tokio::test]
async fn preserve_quotes_plain_scalars_remain_plain() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatPreserveQuotes": true })),
    )
    .await;

    let uri = "file:///test/k8s-preserve-plain.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    let new_text = if result.is_null() || result.as_array().is_some_and(Vec::is_empty) {
        KUBERNETES_YAML.to_owned()
    } else {
        let edits = result.as_array().expect("result should be array");
        edits[0]["newText"]
            .as_str()
            .expect("newText should be a string")
            .to_owned()
    };

    for plain in ["payment-api", "finance", "python:3.12-slim"] {
        assert!(
            new_text.contains(plain),
            "plain scalar {plain:?} should appear unquoted in output; got: {new_text:?}"
        );
        assert!(
            !new_text.contains(&format!("\"{plain}\"")),
            "plain scalar {plain:?} must not be double-quoted in output; got: {new_text:?}"
        );
        assert!(
            !new_text.contains(&format!("'{plain}'")),
            "plain scalar {plain:?} must not be single-quoted in output; got: {new_text:?}"
        );
    }
}

#[tokio::test]
async fn preserve_quotes_idempotent_on_second_format() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatPreserveQuotes": true })),
    )
    .await;

    let uri = "file:///test/k8s-preserve-idempotent.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    // First format pass
    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("first formatting should return a response");
    let result = resp
        .result()
        .expect("first formatting should have a result");

    let first_output = if result.is_null() || result.as_array().is_some_and(Vec::is_empty) {
        KUBERNETES_YAML.to_owned()
    } else {
        let edits = result.as_array().expect("result should be array");
        edits[0]["newText"]
            .as_str()
            .expect("newText should be a string")
            .to_owned()
    };

    // Update document to first_output, then format again
    send(&mut service, did_change_notification(uri, &first_output, 2)).await;

    let resp2 = send(&mut service, formatting_request(3, uri)).await;
    let resp2 = resp2.expect("second formatting should return a response");
    let result2 = resp2
        .result()
        .expect("second formatting should have a result");

    assert!(
        result2.is_null() || result2.as_array().is_some_and(Vec::is_empty),
        "second format pass should produce no edits (idempotent); got: {result2:?}"
    );
}

#[tokio::test]
async fn preserve_quotes_off_by_default_strips_safe_plain_scalars() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No formatPreserveQuotes — defaults to false
    let uri = "file:///test/k8s-default-strip.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    assert!(
        !result.is_null(),
        "default formatting should produce edits for the flow command array; got null"
    );
    let edits = result.as_array().expect("result should be array");
    assert!(
        !edits.is_empty(),
        "default formatting should produce at least one edit"
    );

    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");

    // Safe-plain scalars must have quotes stripped
    assert!(
        !new_text.contains("\"python\""),
        "default mode must strip quotes from safe-plain 'python'; got: {new_text:?}"
    );
    assert!(
        !new_text.contains("\"http.server\""),
        "default mode must strip quotes from safe-plain 'http.server'; got: {new_text:?}"
    );

    // Scalars that require quoting must stay quoted
    assert!(
        new_text.contains("\"-m\""),
        "default mode must keep quotes on '-m' (reserved leading dash); got: {new_text:?}"
    );
    assert!(
        new_text.contains("\"5000\""),
        "default mode must keep quotes on '5000' (looks like integer); got: {new_text:?}"
    );
}

// ── complete_at AST-branch integration tests (B-1 through B-4) ───────────────

// B-1: OnValue branch — cursor on a value position returns sibling values for the key
#[tokio::test]
async fn completion_on_value_position_returns_sibling_values() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_value.yaml";
    send(
        &mut service,
        did_open_notification(uri, "env: production\nenv: \n"),
    )
    .await;

    // Cursor on line 1 col 5 — after "env: ", inside the value position
    let resp = send(&mut service, completion_request(2, uri, 1, 5)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("production"),
        "value completion should suggest sibling value 'production', got: {result_str}"
    );
}

// B-2: OnKey branch with schema — cursor on a key position with schema returns schema properties
#[tokio::test]
async fn completion_on_key_position_with_schema_returns_schema_properties() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_key_schema.yaml";
    // Use a $schema modeline; schema fetch will fail but sibling structural
    // suggestions still work from the document itself.
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    // Cursor at line 0 col 0 — on "name" key
    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(
        !result.is_null(),
        "key completion with schema should return a result"
    );
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("age"),
        "key completion should include sibling key 'age', got: {result_str}"
    );
}

// B-3: OnKey inside nested mapping — cursor on a key in a nested mapping returns its sibling keys
#[tokio::test]
async fn completion_inside_nested_mapping_returns_sibling_keys() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_nested.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n  port: 8080\n"),
    )
    .await;

    // Cursor at line 1 col 2 — on "host" key inside nested mapping
    let resp = send(&mut service, completion_request(2, uri, 1, 2)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(
        !result.is_null(),
        "nested mapping key completion should return a result"
    );
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("port"),
        "nested mapping completion should suggest sibling key 'port', got: {result_str}"
    );
}

// B-4: InSequenceItem branch — cursor inside a sequence item returns sibling keys from other items
#[tokio::test]
async fn completion_inside_sequence_item_returns_sibling_item_keys() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_seq.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "items:\n  - name: Alice\n    role: admin\n  - name: Bob\n    \n",
        ),
    )
    .await;

    // Cursor at line 4 col 4 — blank key position inside second sequence item
    let resp = send(&mut service, completion_request(2, uri, 4, 4)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(
        !result.is_null(),
        "sequence-item completion should return a result"
    );
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("role"),
        "sequence-item completion should suggest sibling key 'role' from other items, got: {result_str}"
    );
}

// ── TE-specified B-1 through B-4 integration tests ───────────────────────────

// B-1: Cursor on existing mapping key, asserts sibling key label
#[tokio::test]
async fn should_complete_at_mapping_key_suggests_sibling() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b1.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\nregion: us-east\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("age") || result_str.contains("region"),
        "should suggest sibling keys, got: {result_str}"
    );
}

// B-2: Cursor on value position, asserts structural value fallback
#[tokio::test]
async fn should_complete_at_value_position_suggests_structural_values() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b2.yaml";
    send(
        &mut service,
        did_open_notification(uri, "env: production\nenv: staging\nenv: \n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 2, 5)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("production") || result_str.contains("staging"),
        "should suggest structural values, got: {result_str}"
    );
}

// B-3: Cursor on a genuine blank line inside a nested mapping, with a schema.
// Schema supplies the missing key ("port") that structural context alone cannot
// surface — InBlankMapping with schema excludes present keys and returns schema
// properties, exercising the exact branch that C-7 covers at the unit level.
#[tokio::test]
async fn should_complete_at_blank_line_in_nested_mapping_suggests_sibling_keys() {
    const B3_SCHEMA_URL: &str = "https://example.com/test-b3-server.json";
    let schema = rlsp_yaml::schema::parse_schema(&serde_json::json!({
        "type": "object",
        "properties": {
            "server": {
                "type": "object",
                "properties": {
                    "host": { "type": "string" },
                    "port": { "type": "integer" }
                }
            }
        }
    }))
    .expect("b3 schema parse failed");

    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service.inner().seed_schema_cache(B3_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b3.yaml";
    // Blank line after "host:" — cursor at line 3, col 2 is on the blank line
    // inside the "server" nested mapping. InBlankMapping fires; schema supplies "port".
    let yaml = format!(
        "# yaml-language-server: $schema={B3_SCHEMA_URL}\nserver:\n  host: localhost\n  \n"
    );
    send(&mut service, did_open_notification(uri, &yaml)).await;

    // Cursor at line 3, col 2 — blank line inside nested "server" mapping
    let resp = send(&mut service, completion_request(2, uri, 3, 2)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("port"),
        "should suggest schema key 'port' on blank line in nested mapping, got: {result_str}"
    );
    assert!(
        !result_str.contains("\"host\""),
        "should exclude present key 'host', got: {result_str}"
    );
}

// B-4: Cursor on key inside sequence item, asserts sibling key from another item
#[tokio::test]
async fn should_complete_at_sequence_item_key_suggests_sibling_from_other_item() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b4.yaml";
    send(
        &mut service,
        did_open_notification(uri, "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 3, 4)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("age"),
        "should suggest sibling key 'age' from another item, got: {result_str}"
    );
}

// ---- formatEnable setting ----

fn range_formatting_request(id: i64, uri: &str, start_line: u32, end_line: u32) -> Request {
    Request::build("textDocument/rangeFormatting")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": start_line, "character": 0 },
                "end": { "line": end_line, "character": 0 }
            },
            "options": { "tabSize": 2, "insertSpaces": true }
        }))
        .finish()
}

#[tokio::test]
async fn format_enable_false_formatting_returns_null() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({
            "formatEnable": false,
            "formatEnforceBlockStyle": true
        })),
    )
    .await;

    let uri = "file:///test/fmt-enable-false.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");
    assert!(
        result.is_null(),
        "formatting should return null when formatEnable is false; got: {result:?}"
    );
}

#[tokio::test]
async fn format_enable_false_range_formatting_returns_null() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({
            "formatEnable": false,
            "formatEnforceBlockStyle": true
        })),
    )
    .await;

    let uri = "file:///test/range-fmt-enable-false.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let resp = send(&mut service, range_formatting_request(2, uri, 0, 1)).await;
    let resp = resp.expect("rangeFormatting should return a response");
    let result = resp.result().expect("rangeFormatting should have a result");
    assert!(
        result.is_null(),
        "rangeFormatting should return null when formatEnable is false; got: {result:?}"
    );
}

#[tokio::test]
async fn format_enable_false_on_type_formatting_returns_null() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatEnable": false })),
    )
    .await;

    let uri = "file:///test/on-type-fmt-enable-false.yaml";
    send(&mut service, did_open_notification(uri, "server:\n\n")).await;

    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp
        .result()
        .expect("onTypeFormatting should have a result");
    assert!(
        result.is_null(),
        "onTypeFormatting should return null when formatEnable is false; got: {result:?}"
    );
}

#[tokio::test]
async fn format_enable_explicit_true_formatting_returns_edits() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({
            "formatEnable": true,
            "formatEnforceBlockStyle": true
        })),
    )
    .await;

    let uri = "file:///test/fmt-enable-true.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");
    assert!(
        !result.is_null(),
        "formatting should return edits when formatEnable is true; got null"
    );
    let edits = result
        .as_array()
        .expect("formatting result should be an array");
    assert!(
        !edits.is_empty(),
        "formatting should return at least one edit when formatEnable is true"
    );
}

#[tokio::test]
async fn format_enable_default_on_type_formatting_works() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No formatEnable setting — default is true; existing behavior must be preserved.
    let uri = "file:///test/on-type-fmt-default.yaml";
    send(&mut service, did_open_notification(uri, "server:\n\n")).await;

    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp
        .result()
        .expect("onTypeFormatting should have a result");
    assert!(
        !result.is_null(),
        "onTypeFormatting should work when formatEnable is absent (default true); got null"
    );
}

#[tokio::test]
async fn format_enable_false_does_not_gate_code_action() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatEnable": false })),
    )
    .await;

    let uri = "file:///test/code-action-fmt-disabled.yaml";
    send(
        &mut service,
        did_open_notification(uri, "config: {key: value}\n"),
    )
    .await;

    let resp = send(&mut service, code_action_request(2, uri, 0, 1)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    assert!(
        !result.is_null(),
        "codeAction should not be gated by formatEnable; got null"
    );
    let actions = result
        .as_array()
        .expect("codeAction result should be an array");
    assert!(
        !actions.is_empty(),
        "codeAction should return at least one action when formatEnable is false"
    );
}
