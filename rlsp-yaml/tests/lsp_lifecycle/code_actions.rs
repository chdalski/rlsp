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

/// Open `text`, request code actions for lines `start_line..end_line`, and
/// return the action titles (empty for a `null` result).
async fn code_action_titles(text: &str, start_line: u32, end_line: u32) -> Vec<String> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/code-action-titles.yaml";
    send(&mut service, did_open_notification(uri, text)).await;

    let resp = send(
        &mut service,
        code_action_request(2, uri, start_line, end_line),
    )
    .await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    result.as_array().map_or_else(Vec::new, |actions| {
        actions
            .iter()
            .filter_map(|a| a["title"].as_str().map(str::to_string))
            .collect()
    })
}

#[tokio::test]
async fn should_offer_block_to_flow_action_for_loading_document() {
    let titles = code_action_titles("config:\n  key: value\nother: [1, 2]\n", 0, 1).await;

    assert_eq!(titles, vec!["Convert block to flow style"]);
}

#[tokio::test]
async fn should_offer_no_structural_actions_for_document_that_does_not_load() {
    // Same block mapping on line 0 as the loading document above; the
    // unterminated flow sequence on line 2 makes the whole text fail to load,
    // so the structural actions see no AST.
    let titles = code_action_titles("config:\n  key: value\nother: [1, 2\n", 0, 1).await;

    assert!(
        titles.is_empty(),
        "no structural action without an AST; got: {titles:?}"
    );
}

#[tokio::test]
async fn should_offer_text_based_tab_action_for_document_that_does_not_load() {
    // The tab action reads the raw text, so it survives an empty AST.
    let titles = code_action_titles("config:\n  key: value\n\tother: [1, 2\n", 2, 3).await;

    assert_eq!(titles, vec!["Convert tabs to spaces"]);
}

// ---- Delete unused anchor, through the server --------------------------------

/// The byte offset of an LSP position of `text`: lines end at `\n`, `\r\n` or a
/// lone `\r`, columns count characters (the test texts are in the Basic
/// Multilingual Plane).
fn offset_of(text: &str, position: &serde_json::Value) -> usize {
    let line = position["line"].as_u64().expect("a line");
    let character = position["character"].as_u64().expect("a character");
    let bytes = text.as_bytes();
    let (mut at, mut current) = (0, 0);
    while current < line {
        match bytes.get(at) {
            Some(b'\r') if bytes.get(at + 1) == Some(&b'\n') => at += 2,
            Some(b'\r' | b'\n') => at += 1,
            Some(_) => {
                at += 1;
                continue;
            }
            None => break,
        }
        current += 1;
    }
    let skipped = usize::try_from(character).expect("fits");
    text[at..]
        .char_indices()
        .nth(skipped)
        .map_or(text.len(), |(offset, _)| at + offset)
}

/// Open `text`, request code actions for the lines `start_line..end_line` and
/// return the "Delete unused anchor" actions as JSON.
async fn unused_anchor_actions_of(
    service: &mut LspService<Backend>,
    uri: &str,
    start_line: u32,
    end_line: u32,
) -> Vec<serde_json::Value> {
    let resp = send(service, code_action_request(2, uri, start_line, end_line)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    result.as_array().map_or_else(Vec::new, |actions| {
        actions
            .iter()
            .filter(|a| a["title"] == "Delete unused anchor")
            .cloned()
            .collect()
    })
}

/// `text` with the edits of `action` applied.
fn apply_action(text: &str, uri: &str, action: &serde_json::Value) -> String {
    let edits = action
        .pointer("/edit/changes")
        .and_then(|changes| changes.get(uri))
        .and_then(serde_json::Value::as_array)
        .expect("edits for the document");
    assert_eq!(edits.len(), 1, "one edit: {action}");
    let edit = edits.first().expect("one edit");
    let position = |pointer: &str| edit.pointer(pointer).expect("a position");
    let start = offset_of(text, position("/range/start"));
    let end = offset_of(text, position("/range/end"));
    format!(
        "{}{}{}",
        &text[..start],
        edit.get("newText")
            .and_then(serde_json::Value::as_str)
            .expect("new text"),
        &text[end..]
    )
}

async fn open(text: &str, uri: &str) -> LspService<Backend> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;
    send(&mut service, did_open_notification(uri, text)).await;
    service
}

#[tokio::test]
async fn should_delete_an_unused_anchor_through_the_server() {
    let uri = "file:///test/delete-anchor.yaml";
    let cases = [
        ("scalar", "k: &a v\n", "k: v\n"),
        (
            "mapping",
            "defaults: &unused\n  key: val\n",
            "defaults:\n  key: val\n",
        ),
        ("sequence", "k: &s\n  - 1\n", "k:\n  - 1\n"),
        ("empty node", "a: &y\nb: 1\n", "a:\nb: 1\n"),
        (
            "block scalar",
            "a: &x |\n  line one\nb: 1\n",
            "a: |\n  line one\nb: 1\n",
        ),
        ("CRLF", "k: &a\r\n  x: 1\r\n", "k:\r\n  x: 1\r\n"),
        (
            "second document",
            "a: 1\n---\nb: &y 2\n",
            "a: 1\n---\nb: 2\n",
        ),
    ];
    for (name, text, expected) in cases {
        let mut service = open(text, uri).await;
        let actions = unused_anchor_actions_of(&mut service, uri, 0, 99).await;

        assert_eq!(actions.len(), 1, "{name}: {actions:?}");
        assert_eq!(actions[0]["kind"], "quickfix", "{name}");
        assert_eq!(
            actions[0]["diagnostics"][0]["code"], "unusedAnchor",
            "{name}"
        );
        assert_eq!(apply_action(text, uri, &actions[0]), expected, "{name}");
    }
}

#[tokio::test]
async fn should_offer_delete_unused_anchor_on_the_anchor_line_only() {
    let uri = "file:///test/delete-anchor-line.yaml";
    let mut service = open("defaults: &d\n  key: val\n", uri).await;

    let on_the_anchor = unused_anchor_actions_of(&mut service, uri, 0, 0).await;
    let on_the_body = unused_anchor_actions_of(&mut service, uri, 1, 1).await;

    assert_eq!(on_the_anchor.len(), 1);
    assert!(on_the_body.is_empty(), "{on_the_body:?}");
}

#[tokio::test]
async fn should_not_offer_delete_unused_anchor_once_the_anchor_is_gone() {
    let uri = "file:///test/delete-anchor-change.yaml";
    let mut service = open("k: &a v\n", uri).await;
    assert_eq!(
        unused_anchor_actions_of(&mut service, uri, 0, 99)
            .await
            .len(),
        1
    );

    send(&mut service, did_change_notification(uri, "k: v\n", 2)).await;

    assert!(
        unused_anchor_actions_of(&mut service, uri, 0, 99)
            .await
            .is_empty()
    );
}
