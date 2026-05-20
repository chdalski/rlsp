// SPDX-License-Identifier: MIT
use futures::StreamExt;
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
