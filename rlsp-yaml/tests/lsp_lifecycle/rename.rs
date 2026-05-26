// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn prepare_rename_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/prepareRename")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

pub fn rename_request(id: i64, uri: &str, line: u32, character: u32, new_name: &str) -> Request {
    Request::build("textDocument/rename")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "newName": new_name
        }))
        .finish()
}

#[tokio::test]
async fn should_return_prepare_rename_range_for_anchor() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &defaults\n  key: val\n"),
    )
    .await;

    let resp = send(&mut service, prepare_rename_request(2, uri, 0, 10)).await;
    let resp = resp.expect("prepareRename should return a response");
    let result = resp.result().expect("prepareRename should have result");
    assert!(
        !result.is_null(),
        "prepareRename result should not be null for anchor"
    );
}

#[tokio::test]
async fn should_return_null_prepare_rename_when_not_on_anchor() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, prepare_rename_request(2, uri, 0, 0)).await;
    let resp = resp.expect("prepareRename should return a response");
    let result = resp.result().expect("prepareRename should have result");
    assert!(
        result.is_null(),
        "prepareRename result should be null when not on anchor"
    );
}

#[tokio::test]
async fn should_return_workspace_edit_on_rename() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &old\n  <<: *old\n"),
    )
    .await;

    let resp = send(&mut service, rename_request(2, uri, 0, 10, "new")).await;
    let resp = resp.expect("rename should return a response");
    let result = resp.result().expect("rename should have result");
    assert!(
        !result.is_null(),
        "rename result should not be null, got: {result}"
    );
}

#[tokio::test]
async fn should_return_null_rename_for_invalid_new_name() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/rename.yaml";
    send(&mut service, did_open_notification(uri, "defaults: &old\n")).await;

    let resp = send(&mut service, rename_request(2, uri, 0, 10, "invalid name")).await;
    let resp = resp.expect("rename should return a response");
    let result = resp.result().expect("rename should have result");
    assert!(
        result.is_null(),
        "rename result should be null for invalid new_name with space"
    );
}
