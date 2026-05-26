// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn folding_range_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/foldingRange")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

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
