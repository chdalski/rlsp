// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn folding_range_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/foldingRange")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_folding_ranges_for_nested_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/folding.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n  port: 8080\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(!result.is_null(), "foldingRange result should not be null");
    let folds: Vec<_> = result
        .as_array()
        .expect("foldingRange should be an array")
        .iter()
        .map(|range| (range["startLine"].as_u64(), range["endLine"].as_u64()))
        .collect();
    assert_eq!(folds, [(Some(0), Some(2)), (Some(1), Some(2))]);
}

async fn comment_fold_lines(text: &str) -> Vec<(u64, u64)> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/comment-folding.yaml";
    send(&mut service, did_open_notification(uri, text)).await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    result
        .as_array()
        .map(|ranges| {
            ranges
                .iter()
                .filter(|range| range["kind"] == "comment")
                .map(|range| {
                    (
                        range["startLine"].as_u64().expect("startLine is a number"),
                        range["endLine"].as_u64().expect("endLine is a number"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn should_not_return_comment_fold_for_trailing_comments() {
    let folds = comment_fold_lines("a: 1 # x\nb: 2 # y\n").await;

    assert!(
        folds.is_empty(),
        "trailing comments must not form a comment fold, got: {folds:?}"
    );
}

#[tokio::test]
async fn should_return_comment_fold_for_own_line_comment_run() {
    let folds = comment_fold_lines("# a\n# b\nk: v\n").await;

    assert_eq!(folds, [(0, 1)]);
}

#[tokio::test]
async fn should_return_comment_fold_for_own_line_run_after_trailing_comment() {
    let folds = comment_fold_lines("a: 1 # x\n# y\n# z\nb: 2\n").await;

    assert_eq!(folds, [(1, 2)]);
}
