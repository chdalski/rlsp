// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
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
