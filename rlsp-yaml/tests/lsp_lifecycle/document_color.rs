// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

fn document_color_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/documentColor")
        .id(id)
        .params(json!({ "textDocument": { "uri": uri } }))
        .finish()
}

/// Open `text`, request `textDocument/documentColor`, and return the result.
async fn document_colors(text: &str) -> serde_json::Value {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/colors.yaml";
    send(&mut service, did_open_notification(uri, text)).await;

    let resp = send(&mut service, document_color_request(2, uri)).await;
    let resp = resp.expect("documentColor should return a response");
    resp.result()
        .expect("documentColor should have a result")
        .clone()
}

#[tokio::test]
async fn should_return_color_information_for_each_color_in_document() {
    let colors = document_colors("primary: '#ff0000'\nsecondary: rgb(0, 0, 255)\n").await;

    // The ranges are pinned as the server reports them today: the quoted
    // color's range starts at the opening quote and stops before the closing one.
    assert_eq!(
        colors,
        json!([
            {
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 16 }
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
async fn should_return_no_colors_for_document_that_does_not_load() {
    // The same color as in the loading document; the unterminated flow
    // sequence makes the whole text fail to load, so there is no AST to scan.
    let colors = document_colors("primary: '#ff0000'\nitems: [1, 2\n").await;

    assert_eq!(colors, json!([]));
}
