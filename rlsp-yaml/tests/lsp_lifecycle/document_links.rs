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
async fn a_document_link_after_an_escape_sequence_starts_early_by_the_characters_the_escape_saves()
{
    // In `key: "\x41 https://example.com"` the URL occupies columns 11 to 30 of
    // the source line, and LSP 3.17 "Document Link Request" makes
    // `DocumentLink.range` the range this link applies to, so that is the range
    // the specification asks for. The server measures the URL's offset in the
    // scalar's value, where `\x41` is the one character `A`, so the range
    // starts and ends three columns early (8 to 27): one for each character the
    // escape saves. It stays that way because the fix is a change to the
    // document-link feature, which would map value offsets back through the
    // escapes, and it has nothing to do with the unit the columns count in.
    let uri = "file:///test/escaped-link.yaml";
    let mut service = open("key: \"\\x41 https://example.com\"\n", uri).await;

    let resp = send(&mut service, document_link_request(2, uri)).await;
    let resp = resp.expect("documentLink should return a response");
    let result = resp.result().expect("documentLink should have a result");

    let ranges: Vec<_> = result
        .as_array()
        .expect("documentLink should be an array")
        .iter()
        .map(|link| link["range"].clone())
        .collect();
    assert_eq!(
        ranges,
        [json!({
            "start": { "line": 0, "character": 8 },
            "end": { "line": 0, "character": 27 }
        })]
    );
}
