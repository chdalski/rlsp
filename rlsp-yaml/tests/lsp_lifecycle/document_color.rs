// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn document_color_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/documentColor")
        .id(id)
        .params(json!({ "textDocument": { "uri": uri } }))
        .finish()
}

/// Open `text`, request `textDocument/documentColor`, and return the result.
async fn document_colors(text: &str) -> serde_json::Value {
    document_colors_after_changes(text, &[]).await
}

/// Open `text`, replace it by each of `changes` in turn, request
/// `textDocument/documentColor`, and return the result.
async fn document_colors_after_changes(text: &str, changes: &[&str]) -> serde_json::Value {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/colors.yaml";
    send(&mut service, did_open_notification(uri, text)).await;
    for (version, change) in (2..).zip(changes) {
        send(&mut service, did_change_notification(uri, change, version)).await;
    }

    let resp = send(&mut service, document_color_request(2, uri)).await;
    let resp = resp.expect("documentColor should return a response");
    resp.result()
        .expect("documentColor should have a result")
        .clone()
}

#[tokio::test]
async fn should_return_color_information_for_each_color_in_document() {
    let colors = document_colors("primary: '#ff0000'\nsecondary: rgb(0, 0, 255)\n").await;

    // Each range covers the color as written in the text: the quoted color
    // starts after the opening quote and ends before the closing one.
    assert_eq!(
        colors,
        json!([
            {
                "range": {
                    "start": { "line": 0, "character": 10 },
                    "end": { "line": 0, "character": 17 }
                },
                "color": { "red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0 }
            },
            {
                "range": {
                    "start": { "line": 1, "character": 11 },
                    "end": { "line": 1, "character": 25 }
                },
                "color": { "red": 0.0, "green": 0.0, "blue": 1.0, "alpha": 1.0 }
            }
        ])
    );
}

#[tokio::test]
async fn should_report_colors_at_source_positions_for_every_scalar_style() {
    let colors = document_colors("a: \"red\"\nb: 'blue'\nc: |\n  lime\nd: >\n  white\n").await;

    assert_eq!(
        colors,
        json!([
            {
                "range": {
                    "start": { "line": 0, "character": 4 },
                    "end": { "line": 0, "character": 7 }
                },
                "color": { "red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0 }
            },
            {
                "range": {
                    "start": { "line": 1, "character": 4 },
                    "end": { "line": 1, "character": 8 }
                },
                "color": { "red": 0.0, "green": 0.0, "blue": 1.0, "alpha": 1.0 }
            },
            {
                "range": {
                    "start": { "line": 3, "character": 2 },
                    "end": { "line": 3, "character": 6 }
                },
                "color": { "red": 0.0, "green": 1.0, "blue": 0.0, "alpha": 1.0 }
            },
            {
                "range": {
                    "start": { "line": 5, "character": 2 },
                    "end": { "line": 5, "character": 7 }
                },
                "color": { "red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0 }
            }
        ])
    );
}

#[tokio::test]
async fn should_not_report_color_in_block_scalar_header_comment() {
    let colors = document_colors("a: | # red\n  x\n").await;

    assert_eq!(colors, json!([]));
}

#[tokio::test]
async fn should_report_ranges_of_the_latest_text_after_a_change() {
    let colors = document_colors_after_changes("a: red\n", &["bb: 'blue'\n"]).await;

    assert_eq!(
        colors,
        json!([
            {
                "range": {
                    "start": { "line": 0, "character": 5 },
                    "end": { "line": 0, "character": 9 }
                },
                "color": { "red": 0.0, "green": 0.0, "blue": 1.0, "alpha": 1.0 }
            }
        ])
    );
}

#[tokio::test]
async fn should_return_no_colors_for_document_that_does_not_load() {
    // The same color as in the loading document; the unterminated flow
    // sequence makes the whole text fail to load, so there is no AST to scan.
    let colors = document_colors("primary: '#ff0000'\nitems: [1, 2\n").await;

    assert_eq!(colors, json!([]));
}
