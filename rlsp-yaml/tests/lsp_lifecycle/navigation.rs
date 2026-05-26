// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn definition_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/definition")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

pub fn references_request(
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

pub fn document_symbol_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/documentSymbol")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

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
