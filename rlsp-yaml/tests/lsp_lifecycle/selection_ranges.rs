// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn selection_range_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    selection_range_request_for(id, uri, &[(line, character)])
}

pub fn selection_range_request_for(id: i64, uri: &str, positions: &[(u32, u32)]) -> Request {
    let positions: Vec<_> = positions
        .iter()
        .map(|&(line, character)| json!({ "line": line, "character": character }))
        .collect();
    Request::build("textDocument/selectionRange")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "positions": positions
        }))
        .finish()
}

#[tokio::test]
async fn should_return_selection_ranges_for_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n"),
    )
    .await;

    let resp = send(&mut service, selection_range_request(2, uri, 0, 0)).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert!(
        !result.is_null(),
        "selectionRange result should not be null for valid YAML"
    );
}

#[tokio::test]
async fn selection_range_after_multiline_value_comment_returns_only_the_root() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection_comment.yaml";
    send(
        &mut service,
        did_open_notification(uri, "k: \"aaa\n  bbb\" # c\n"),
    )
    .await;

    let resp = send(&mut service, selection_range_request(2, uri, 1, 9)).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert_eq!(
        result,
        &json!([{
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 2, "character": 0 }
            }
        }]),
        "a cursor on the comment must get the document root only"
    );
}

#[tokio::test]
async fn selection_range_inside_multiline_value_keeps_the_full_chain() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection_inside.yaml";
    send(
        &mut service,
        did_open_notification(uri, "k: \"aaa\n  bbb\" # c\n"),
    )
    .await;

    let resp = send(&mut service, selection_range_request(2, uri, 1, 5)).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert_eq!(
        result,
        &json!([{
            "range": {
                "start": { "line": 0, "character": 3 },
                "end": { "line": 1, "character": 6 }
            },
            "parent": {
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 1, "character": 6 }
                },
                "parent": {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 2, "character": 0 }
                    }
                }
            }
        }]),
    );
}

fn range_json(start: (u32, u32), end: (u32, u32)) -> serde_json::Value {
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 }
    })
}

#[tokio::test]
async fn selection_range_on_a_later_sibling_after_a_multiline_value_selects_that_sibling() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection_later_sibling.yaml";
    send(
        &mut service,
        did_open_notification(uri, "- a: 1\n  b: \"p\n    q\"\n- c: 2\n"),
    )
    .await;

    let resp = send(&mut service, selection_range_request(2, uri, 3, 2)).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert_eq!(
        result,
        &json!([{
            "range": range_json((3, 2), (3, 3)),
            "parent": {
                "range": range_json((3, 2), (3, 6)),
                "parent": {
                    "range": range_json((3, 2), (4, 0)),
                    "parent": { "range": range_json((0, 0), (4, 0)) }
                }
            }
        }]),
    );
}

fn empty_range_entry(line: u32, character: u32) -> serde_json::Value {
    json!({
        "range": {
            "start": { "line": line, "character": character },
            "end": { "line": line, "character": character }
        }
    })
}

#[tokio::test]
async fn selection_range_outside_every_range_returns_the_empty_range() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection_outside.yaml";
    send(&mut service, did_open_notification(uri, "key: value")).await;

    let resp = send(&mut service, selection_range_request(2, uri, 0, 11)).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert_eq!(result, &json!([empty_range_entry(0, 11)]));
}

#[tokio::test]
async fn selection_range_returns_one_entry_per_position_in_order() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection_order.yaml";
    send(&mut service, did_open_notification(uri, "k: v\n# c\n")).await;

    let resp = send(
        &mut service,
        selection_range_request_for(2, uri, &[(1, 1), (0, 0)]),
    )
    .await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert_eq!(
        result,
        &json!([
            empty_range_entry(1, 1),
            {
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 1 }
                },
                "parent": {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 4 }
                    },
                    "parent": {
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 2, "character": 0 }
                        }
                    }
                }
            }
        ])
    );
}

#[tokio::test]
async fn selection_range_for_document_without_ast_returns_an_empty_range_per_position() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection_empty.yaml";
    send(&mut service, did_open_notification(uri, "")).await;

    let resp = send(
        &mut service,
        selection_range_request_for(2, uri, &[(0, 0), (2, 5)]),
    )
    .await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert_eq!(
        result,
        &json!([empty_range_entry(0, 0), empty_range_entry(2, 5)])
    );
}

#[tokio::test]
async fn selection_range_with_no_positions_returns_null() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/selection_no_positions.yaml";
    send(&mut service, did_open_notification(uri, "k: v\n")).await;

    let resp = send(&mut service, selection_range_request_for(2, uri, &[])).await;
    let resp = resp.expect("selectionRange should return a response");
    let result = resp.result().expect("selectionRange should have a result");
    assert!(result.is_null(), "got {result}");
}
