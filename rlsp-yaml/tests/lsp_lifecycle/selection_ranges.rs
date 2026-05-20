// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn selection_range_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
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
