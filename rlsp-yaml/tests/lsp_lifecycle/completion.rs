// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn completion_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
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
