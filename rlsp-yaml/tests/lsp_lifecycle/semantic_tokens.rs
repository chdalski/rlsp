// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn semantic_tokens_request(id: i64, uri: &str) -> Request {
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

/// A block scalar's indicator is an operator token, for a value and for a key.
#[rstest]
#[case::value("k: |\n  text\n", json!([0, 0, 1, 0, 0, 0, 3, 1, 7, 0]))]
#[case::key("? |\n  k\n: v\n", json!([0, 2, 1, 7, 0, 2, 2, 1, 1, 0]))]
#[tokio::test]
async fn block_scalar_header_has_an_operator_token_over_the_server(
    #[case] text: &str,
    #[case] expected: serde_json::Value,
) {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/block-scalar-tokens.yaml";
    send(&mut service, did_open_notification(uri, text)).await;

    let resp = send(&mut service, semantic_tokens_request(2, uri)).await;
    let resp = resp.expect("semanticTokens/full should return a response");
    let result = resp
        .result()
        .expect("semanticTokens/full should have result");

    // Token types: property = 0, string = 1, operator = 7 (the legend's order).
    assert_eq!(result["data"], expected);
}
