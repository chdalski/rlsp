// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn did_change_watched_files_notification(uri: &str) -> Request {
    Request::build("workspace/didChangeWatchedFiles")
        .params(json!({
            "changes": [
                { "uri": uri, "type": 2 }
            ]
        }))
        .finish()
}

#[tokio::test]
async fn should_republish_diagnostics_on_did_change_watched_files() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Open a valid document
    let uri = "file:///test/watched.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    // Trigger watched-files notification (re-validates all open documents)
    let resp = send(&mut service, did_change_watched_files_notification(uri)).await;
    assert!(
        resp.is_none(),
        "didChangeWatchedFiles should not return a response"
    );

    // Document should still be open with same diagnostics (empty for valid YAML)
    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after re-validation");
    assert!(diags.is_empty(), "valid YAML should have no diagnostics");
}
