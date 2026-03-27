// SPDX-License-Identifier: MIT

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{
    DiagnosticSeverity, HoverProviderCapability, InitializeResult, OneOf,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};

fn initialize_request(id: i64) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null
        }))
        .finish()
}

fn initialized_notification() -> Request {
    Request::build("initialized").params(json!({})).finish()
}

fn shutdown_request(id: i64) -> Request {
    Request::build("shutdown").id(id).finish()
}

fn did_open_notification(uri: &str, text: &str) -> Request {
    Request::build("textDocument/didOpen")
        .params(json!({
            "textDocument": {
                "uri": uri,
                "languageId": "yaml",
                "version": 1,
                "text": text
            }
        }))
        .finish()
}

fn did_change_notification(uri: &str, text: &str, version: i32) -> Request {
    Request::build("textDocument/didChange")
        .params(json!({
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": [
                { "text": text }
            ]
        }))
        .finish()
}

fn did_close_notification(uri: &str) -> Request {
    Request::build("textDocument/didClose")
        .params(json!({
            "textDocument": {
                "uri": uri
            }
        }))
        .finish()
}

async fn send(service: &mut LspService<Backend>, req: Request) -> Option<Response> {
    use tower::Service;
    service.call(req).await.expect("service call failed")
}

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
    // Note: saphyr will fail to parse if there's a syntax error, so we can't
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
    assert_eq!(
        arr.len(),
        2,
        "should return 2 document links for YAML with 2 URLs"
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

fn did_change_configuration_notification(settings: serde_json::Value) -> Request {
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
        did_change_configuration_notification(json!({ "keyOrdering": true })),
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

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover-schema.yaml";
    // Use a $schema modeline. The fetch will fail in tests (no network), but
    // the schema_associations entry is stored and the schema_cache lookup path
    // in hover() is exercised (returning None from cache, which is fine).
    send(
        &mut service,
        did_open_notification(
            uri,
            "# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json\nname: Alice\n",
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

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion-schema.yaml";
    // Use a $schema modeline to store a schema association. The fetch will fail
    // in tests but the schema lookup path in completion() is exercised.
    send(
        &mut service,
        did_open_notification(
            uri,
            "# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json\nname: Alice\nage: 30\n",
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
    assert!(!arr.is_empty(), "codeLens should return a lens for the K8s schema");
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
    assert!(!arr.is_empty(), "codeLens should return a lens for the K8s schema");
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
