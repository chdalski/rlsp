// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn code_action_request(id: i64, uri: &str, start_line: u32, end_line: u32) -> Request {
    Request::build("textDocument/codeAction")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": start_line, "character": 0 },
                "end":   { "line": end_line,   "character": 0 }
            },
            "context": { "diagnostics": [] }
        }))
        .finish()
}

pub fn code_action_request_with_diagnostics(
    id: i64,
    uri: &str,
    start_line: u32,
    end_line: u32,
    diagnostics: &serde_json::Value,
) -> Request {
    Request::build("textDocument/codeAction")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": start_line, "character": 0 },
                "end":   { "line": end_line,   "character": 0 }
            },
            "context": { "diagnostics": diagnostics }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_code_actions_for_document_with_actions() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/code-action.yaml";
    // Flow mapping triggers a "convert to block" code action
    send(
        &mut service,
        did_open_notification(uri, "config: {key: value}\n"),
    )
    .await;

    let resp = send(&mut service, code_action_request(2, uri, 0, 1)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    assert!(
        !result.is_null(),
        "codeAction result should not be null for document with actions"
    );
}

#[tokio::test]
async fn should_return_null_code_actions_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, code_action_request(2, uri, 0, 1)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "codeAction result should be null or empty for unknown document"
    );
}

#[tokio::test]
async fn should_return_yaml11_bool_code_actions_via_server_handler() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-bool.yaml";
    send(&mut service, did_open_notification(uri, "enabled: yes\n")).await;

    // Relay the yaml11Boolean diagnostic back to the server, as VS Code would.
    // The diagnostic range covers the scalar "yes" at col 9..12 on line 0.
    let diagnostics = json!([{
        "range": {
            "start": { "line": 0, "character": 9 },
            "end":   { "line": 0, "character": 12 }
        },
        "severity": 2,
        "code": "yaml11Boolean",
        "source": "rlsp-yaml",
        "message": "\"yes\" is a boolean in YAML 1.1 but a string in YAML 1.2."
    }]);

    let resp = send(
        &mut service,
        code_action_request_with_diagnostics(2, uri, 0, 1, &diagnostics),
    )
    .await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");

    let actions = result.as_array().expect("result should be an array");
    let titles: Vec<&str> = actions.iter().filter_map(|a| a["title"].as_str()).collect();

    assert!(
        titles.contains(&"Quote value"),
        "should include 'Quote value' action; got: {titles:?}"
    );
    assert!(
        titles.contains(&"Convert to boolean"),
        "should include 'Convert to boolean' action; got: {titles:?}"
    );
}
