// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn document_link_request(id: i64, uri: &str) -> Request {
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
