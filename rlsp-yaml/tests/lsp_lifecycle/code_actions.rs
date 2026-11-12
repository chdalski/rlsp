// SPDX-License-Identifier: MIT
use std::fmt::Write as _;

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn code_action_request(id: i64, uri: &str, start_line: u32, end_line: u32) -> Request {
    code_action_request_at(id, uri, (start_line, 0), (end_line, 0))
}

/// A code-action request for the range `start..end`, given as `(line, character)`.
pub fn code_action_request_at(id: i64, uri: &str, start: (u32, u32), end: (u32, u32)) -> Request {
    Request::build("textDocument/codeAction")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": start.0, "character": start.1 },
                "end":   { "line": end.0,   "character": end.1 }
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

/// The answer to a code-action request for line 0 of `text`, from a server
/// whose client announced `capabilities`.
async fn code_action_result(capabilities: &serde_json::Value, text: &str) -> serde_json::Value {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(
        &mut service,
        initialize_request_with_capabilities(1, capabilities),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/disabled-action.yaml";
    send(&mut service, did_open_notification(uri, text)).await;

    let resp = send(&mut service, code_action_request(2, uri, 0, 0)).await;
    let resp = resp.expect("codeAction should return a response");
    resp.result()
        .expect("codeAction should have a result")
        .clone()
}

const COMMENT_IN_COLLECTION: &str = "a: # note\n  b: 1\n";

const BLOCK_SCALAR_IN_COLLECTION: &str = "a:\n  b: |\n    t\n";

#[tokio::test]
async fn should_offer_block_to_flow_disabled_to_a_client_that_supports_disabled_actions() {
    let capabilities = json!({ "textDocument": { "codeAction": { "disabledSupport": true } } });

    for (text, reason) in [
        (
            COMMENT_IN_COLLECTION,
            "Flow style would lose the comments in this collection",
        ),
        (
            BLOCK_SCALAR_IN_COLLECTION,
            "Flow style cannot hold the block scalar (| or >) in this collection",
        ),
    ] {
        let result = code_action_result(&capabilities, text).await;

        let actions = result.as_array().expect("result should be an array");
        let action = actions
            .iter()
            .find(|a| a["title"] == "Convert block to flow style")
            .unwrap_or_else(|| panic!("block-to-flow action expected; got: {result}"));
        assert_eq!(action["disabled"], json!({ "reason": reason }));
        assert_eq!(action["kind"], "refactor.rewrite");
        assert!(
            action.get("edit").is_none(),
            "no edit expected; got: {action}"
        );
    }
}

#[tokio::test]
async fn should_offer_no_disabled_action_to_a_client_without_disabled_support() {
    for capabilities in [
        json!({}),
        json!({ "textDocument": {} }),
        json!({ "textDocument": { "codeAction": {} } }),
        json!({ "textDocument": { "codeAction": { "disabledSupport": false } } }),
    ] {
        for text in [COMMENT_IN_COLLECTION, BLOCK_SCALAR_IN_COLLECTION] {
            let result = code_action_result(&capabilities, text).await;

            // The collection has no other action, so the answer is empty.
            assert!(
                result.is_null(),
                "capabilities {capabilities}, text {text:?}: expected null; got: {result}"
            );
        }
    }
}

#[tokio::test]
async fn should_keep_the_enabled_actions_for_a_client_without_disabled_support() {
    let result = code_action_result(&json!({}), "a:\n  b: 1\n").await;

    let actions = result.as_array().expect("result should be an array");
    let action = actions
        .iter()
        .find(|a| a["title"] == "Convert block to flow style")
        .unwrap_or_else(|| panic!("block-to-flow action expected; got: {result}"));
    assert!(action.get("disabled").is_none(), "{action}");
    assert!(action.get("edit").is_some(), "{action}");
}

fn disabled_support() -> serde_json::Value {
    json!({ "textDocument": { "codeAction": { "disabledSupport": true } } })
}

fn titles(result: &serde_json::Value) -> Vec<&str> {
    result
        .as_array()
        .expect("result should be an array")
        .iter()
        .filter_map(|a| a["title"].as_str())
        .collect()
}

#[tokio::test]
async fn should_track_the_capability_per_server_instance() {
    let with = code_action_result(&disabled_support(), COMMENT_IN_COLLECTION).await;
    let without = code_action_result(&json!({}), COMMENT_IN_COLLECTION).await;
    let with_again = code_action_result(&disabled_support(), COMMENT_IN_COLLECTION).await;

    assert_eq!(titles(&with), ["Convert block to flow style"]);
    assert!(without.is_null(), "{without}");
    assert_eq!(with_again, with);
}

#[tokio::test]
async fn should_follow_the_edits_of_the_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(
        &mut service,
        initialize_request_with_capabilities(1, &disabled_support()),
    )
    .await;
    send(&mut service, initialized_notification()).await;
    let uri = "file:///test/edits.yaml";
    send(
        &mut service,
        did_open_notification(uri, COMMENT_IN_COLLECTION),
    )
    .await;

    let mut answers = Vec::new();
    for (id, version, text) in [
        (2, None, COMMENT_IN_COLLECTION),
        (3, Some(2), "a:\n  b: 1\n"),
        (4, Some(3), COMMENT_IN_COLLECTION),
    ] {
        if let Some(version) = version {
            send(&mut service, did_change_notification(uri, text, version)).await;
        }
        let resp = send(&mut service, code_action_request(id, uri, 0, 0)).await;
        let result = resp.unwrap().result().unwrap().clone();
        answers.push((
            result[0]["disabled"].is_object(),
            result[0]["edit"].is_object(),
        ));
    }

    assert_eq!(answers, [(true, false), (false, true), (true, false)]);
}

#[tokio::test]
async fn should_keep_the_enabled_action_next_to_the_dropped_disabled_one() {
    // The tab after the comment gives an enabled action on the same line.
    let text = "a: # note\t\n  b: 1\n";

    let without = code_action_result(&json!({}), text).await;
    let with = code_action_result(&disabled_support(), text).await;

    assert!(!titles(&without).contains(&"Convert block to flow style"));
    assert!(titles(&without).contains(&"Convert tabs to spaces"));
    assert!(titles(&with).contains(&"Convert tabs to spaces"));
    assert_eq!(titles(&with).len(), titles(&without).len() + 1);
    assert!(titles(&with).contains(&"Convert block to flow style"));
}

#[tokio::test]
async fn should_answer_the_disabled_action_for_text_beyond_the_basic_plane() {
    for text in ["\u{1F600}: # c\n  b: 1\n", "ключ: # コメント\n  b: 1\n"] {
        let result = code_action_result(&disabled_support(), text).await;

        assert_eq!(titles(&result), ["Convert block to flow style"], "{text:?}");
        assert!(result[0]["disabled"].is_object(), "{result}");
    }
}

#[tokio::test]
async fn should_offer_no_action_when_block_style_is_enforced_even_with_disabled_support() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(
        &mut service,
        Request::build("initialize")
            .id(1)
            .params(json!({
                "capabilities": disabled_support(),
                "processId": null,
                "rootUri": null,
                "initializationOptions": { "formatEnforceBlockStyle": true }
            }))
            .finish(),
    )
    .await;
    send(&mut service, initialized_notification()).await;
    let uri = "file:///test/enforce.yaml";
    send(
        &mut service,
        did_open_notification(uri, COMMENT_IN_COLLECTION),
    )
    .await;

    let resp = send(&mut service, code_action_request(2, uri, 0, 0)).await;

    assert!(resp.unwrap().result().unwrap().is_null());
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

// The edit replaces the converted range only: the comment on the last line of
// the collection is outside it, and the new text ends with the flow collection.
#[tokio::test]
async fn should_answer_an_edit_that_leaves_the_comment_after_the_collection() {
    let uri = "file:///test/block-to-flow-keeps-comment.yaml";
    let text = "a:\n  b: 1\n  c: 2 # note\nd: 1\n";
    let mut service = open(text, uri).await;

    let resp = send(&mut service, code_action_request(2, uri, 0, 1)).await;
    let result = resp
        .expect("a response")
        .result()
        .expect("a result")
        .clone();

    let actions = result.as_array().expect("an array");
    assert_eq!(actions.len(), 1, "{result}");
    assert_eq!(
        actions[0]["edit"]["changes"][uri],
        json!([{
            "range": {
                "start": { "line": 0, "character": 2 },
                "end": { "line": 2, "character": 6 }
            },
            "newText": " { b: 1, c: 2 }"
        }])
    );
    assert_eq!(
        apply_action(text, uri, &actions[0]),
        "a: { b: 1, c: 2 } # note\nd: 1\n"
    );
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

/// `leaf` inside `depth` nested flow sequences.
fn nested_flow(depth: usize, leaf: &str) -> String {
    format!("{}{leaf}{}", "[".repeat(depth), "]".repeat(depth))
}

#[tokio::test]
async fn should_delete_an_unused_anchor_nested_deeper_than_a_hundred_levels() {
    let uri = "file:///test/delete-anchor-deep.yaml";
    for depth in [101, 255] {
        let text = format!("a: {}\n", nested_flow(depth, "&x 1"));
        let mut service = open(&text, uri).await;
        let actions = unused_anchor_actions_of(&mut service, uri, 0, 0).await;

        assert_eq!(actions.len(), 1, "depth {depth}: {actions:?}");
        assert_eq!(actions[0]["kind"], "quickfix", "depth {depth}");
        assert_eq!(
            apply_action(&text, uri, &actions[0]),
            format!("a: {}\n", nested_flow(depth, "1")),
            "depth {depth}"
        );
    }
}

#[tokio::test]
async fn should_not_report_an_anchor_whose_alias_is_nested_deeper_than_a_hundred_levels() {
    let uri = "file:///test/delete-anchor-deep-alias.yaml";
    for depth in [101, 255] {
        let text = format!("a: &x 1\nb: {}\n", nested_flow(depth, "*x"));
        let mut service = open(&text, uri).await;

        assert!(
            unused_anchor_actions_of(&mut service, uri, 0, 99)
                .await
                .is_empty(),
            "depth {depth}"
        );
        let diagnostics = format!("{:?}", service.inner().get_diagnostics(uri));
        assert!(!diagnostics.contains("yamlSyntax"), "depth {depth}");
        assert!(!diagnostics.contains("unusedAnchor"), "depth {depth}");
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

/// The `(line, start column, end column)` of the edit of a "Delete unused
/// anchor" action.
#[expect(clippy::indexing_slicing, reason = "JSON access in a test helper")]
fn deleted_span(action: &serde_json::Value, uri: &str) -> (u64, u64, u64) {
    let edit = &action["edit"]["changes"][uri][0];
    assert_eq!(edit["newText"], "", "the fix only deletes: {action:?}");
    let range = &edit["range"];
    assert_eq!(range["start"]["line"], range["end"]["line"], "{action:?}");
    (
        range["start"]["line"].as_u64().expect("a line"),
        range["start"]["character"].as_u64().expect("a column"),
        range["end"]["character"].as_u64().expect("a column"),
    )
}

#[tokio::test]
async fn should_offer_one_delete_action_per_unused_anchor_in_line_order_for_a_large_document() {
    let uri = "file:///test/delete-anchor-many.yaml";
    let text = (0..2000).fold(String::new(), |mut text, i| {
        writeln!(text, "k{i}: &a{i} v").expect("writing to a String");
        text
    });
    let mut service = open(&text, uri).await;

    let actions = unused_anchor_actions_of(&mut service, uri, 0, 1999).await;

    assert_eq!(actions.len(), 2000);
    for (i, action) in actions.iter().enumerate() {
        let start = format!("k{i}: ").len() as u64;
        let token = format!("&a{i} ").len() as u64;
        assert_eq!(
            deleted_span(action, uri),
            (i as u64, start, start + token),
            "action {i}"
        );
        assert_eq!(action["kind"], "quickfix", "action {i}");
        assert_eq!(
            action["diagnostics"][0]["code"], "unusedAnchor",
            "action {i}"
        );
    }
}

#[tokio::test]
async fn should_resolve_each_anchor_of_many_documents_through_the_server() {
    let uri = "file:///test/delete-anchor-documents.yaml";
    let text = (0..300).fold(String::new(), |mut text, i| {
        write!(text, "---\nk: &a{i} v\n").expect("writing to a String");
        text
    });
    let mut service = open(&text, uri).await;

    let actions = unused_anchor_actions_of(&mut service, uri, 0, 599).await;

    assert_eq!(actions.len(), 300);
    for (i, action) in actions.iter().enumerate() {
        assert_eq!(
            deleted_span(action, uri),
            (2 * i as u64 + 1, 3, 3 + format!("&a{i} ").len() as u64),
            "action {i}"
        );
    }
}

/// Open `text` and return the code actions the server offers for `line`.
async fn actions_on_line(text: &str, uri: &str, line: u32) -> Vec<serde_json::Value> {
    let mut service = open(text, uri).await;
    let resp = send(&mut service, code_action_request(2, uri, line, line)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    result.as_array().cloned().unwrap_or_default()
}

fn titles_of(actions: &[serde_json::Value]) -> Vec<&str> {
    actions
        .iter()
        .map(|a| a["title"].as_str().expect("a title"))
        .collect()
}

#[tokio::test]
async fn should_offer_the_context_actions_of_a_lone_cr_document_through_the_server() {
    let uri = "file:///test/context-actions-cr.yaml";
    let lf = "x: 1\nb:\t2\nouter:\n  a: 1\n  b: 2\n";
    let cr = lf.replace('\n', "\r");

    let tab_lf = actions_on_line(lf, uri, 1).await;
    let tab_cr = actions_on_line(&cr, uri, 1).await;
    let flow_lf = actions_on_line(lf, uri, 2).await;
    let flow_cr = actions_on_line(&cr, uri, 2).await;

    assert_eq!(titles_of(&tab_lf), ["Convert tabs to spaces"]);
    assert_eq!(tab_cr, tab_lf);
    assert_eq!(titles_of(&flow_lf), ["Convert block to flow style"]);
    assert_eq!(flow_cr, flow_lf);
    assert_eq!(
        apply_action(&cr, uri, &tab_cr[0]),
        "x: 1\rb:  2\router:\r  a: 1\r  b: 2\r"
    );
    assert_eq!(
        apply_action(&cr, uri, &flow_cr[0]).replace('\r', "\n"),
        apply_action(lf, uri, &flow_lf[0])
    );
}
