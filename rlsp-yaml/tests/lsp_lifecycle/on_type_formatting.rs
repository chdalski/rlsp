// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn on_type_formatting_request(
    id: i64,
    uri: &str,
    line: u32,
    character: u32,
    ch: &str,
) -> Request {
    Request::build("textDocument/onTypeFormatting")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "ch": ch,
            "options": { "tabSize": 2, "insertSpaces": true }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_indent_edit_on_type_newline_after_mapping_key() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/format.yaml";
    // After "server:\n" the cursor is at line 1 col 0; the formatter should
    // insert indentation for the child key.
    send(&mut service, did_open_notification(uri, "server:\n\n")).await;

    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp.result().expect("onTypeFormatting should have result");
    assert!(
        !result.is_null(),
        "onTypeFormatting result should not be null after newline on mapping line"
    );
}

#[tokio::test]
async fn should_return_null_on_type_formatting_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp.result().expect("onTypeFormatting should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "onTypeFormatting should be null or empty for unknown document"
    );
}
