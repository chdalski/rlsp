// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::schema::parse_schema;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn hover_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
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

// ── Hover schema docs through the shared resolver ────────────────────────────

const HOVER_SCHEMA_URL: &str = "https://example.com/test-hover-docs.json";

/// Hover at (`line`, `character`) in `document` against a schema that documents
/// an array item's key, a key behind nested composition, a key reached only
/// through a later branch, and a key with dots.
/// Returns the serialized hover result.
async fn hover_with_docs_schema(document: &str, line: u32, character: u32) -> String {
    let schema = parse_schema(&json!({
        "type": "object",
        "allOf": [
            { "properties": { "server": { "properties": {
                "host": { "description": "Host docs" }
            } } } },
            { "properties": { "server": { "properties": {
                "port": { "description": "Port docs" }
            } } } }
        ],
        "properties": {
            "containers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "image": { "description": "Container image docs" } }
                }
            },
            "spec": {
                "allOf": [{
                    "anyOf": [{
                        "properties": { "replicas": { "description": "Replica count docs" } }
                    }]
                }]
            },
            "labels": {
                "properties": {
                    "app.kubernetes.io/name": { "description": "App name label docs" }
                }
            }
        }
    }))
    .expect("hover docs schema parse failed");

    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    service.inner().seed_schema_cache(HOVER_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover-docs.yaml";
    let text = format!("# yaml-language-server: $schema={HOVER_SCHEMA_URL}\n{document}");
    send(&mut service, did_open_notification(uri, &text)).await;

    // The modeline occupies line 0, so the document's lines start at 1.
    let resp = send(&mut service, hover_request(2, uri, line + 1, character)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have a result");
    serde_json::to_string(result).expect("serialize")
}

#[tokio::test]
async fn should_show_schema_docs_for_a_key_below_an_array_item() {
    let result_str = hover_with_docs_schema("containers:\n  - image: nginx\n", 1, 4).await;

    assert!(
        result_str.contains("Container image docs"),
        "hover should show the docs of 'image', got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_for_a_key_reached_through_nested_composition() {
    let result_str = hover_with_docs_schema("spec:\n  replicas: 3\n", 1, 2).await;

    assert!(
        result_str.contains("Replica count docs"),
        "hover should show the docs of 'replicas', got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_for_a_key_containing_dots() {
    let result_str = hover_with_docs_schema("labels:\n  app.kubernetes.io/name: web\n", 1, 2).await;

    assert!(
        result_str.contains("App name label docs"),
        "hover should show the docs of 'app.kubernetes.io/name', got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_when_the_first_matching_branch_dead_ends() {
    let result_str = hover_with_docs_schema("server:\n  port: 1\n", 1, 2).await;

    assert!(
        result_str.contains("Port docs"),
        "hover should show the docs of 'port' from the later branch, got: {result_str}"
    );
}
