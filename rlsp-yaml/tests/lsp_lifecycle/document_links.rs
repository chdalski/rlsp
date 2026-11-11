// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::{Value, json};
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

/// The ranges of the document links of `text`.
pub async fn link_ranges(text: &str) -> Vec<Value> {
    let uri = "file:///test/link-ranges.yaml";
    let mut service = open(text, uri).await;
    let resp = send(&mut service, document_link_request(2, uri)).await;
    let resp = resp.expect("documentLink should return a response");
    let result = resp.result().expect("documentLink should have a result");
    result
        .as_array()
        .expect("documentLink should be an array")
        .iter()
        .map(|link| link["range"].clone())
        .collect()
}

fn range(start: (u32, u32), end: (u32, u32)) -> Value {
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 }
    })
}

// LSP 3.17 "Document Link Request" makes `DocumentLink.range` the range this
// link applies to, so a link covers the source text of its URL: the columns an
// escape takes count, a URL on a continuation line is on that line, and a URL
// across an escaped line break reaches over both lines. In
// `key: "\x41 https://example.com"` the URL occupies columns 11 to 30.
#[rstest]
#[case::after_an_escape_sequence("key: \"\\x41 https://example.com\"\n", &[range((0, 11), (0, 30))])]
#[case::on_a_continuation_line("k: \"a\n  https://x.com\"\n", &[range((1, 2), (1, 15))])]
#[case::across_an_escaped_line_break("k: \"https://x.com/a\\\n  b\"\n", &[range((0, 4), (1, 3))])]
#[tokio::test]
async fn a_document_link_covers_the_source_text_of_its_url(
    #[case] text: &str,
    #[case] expected: &[Value],
) {
    assert_eq!(link_ranges(text).await, expected);
}
