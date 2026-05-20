// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn did_change_configuration_notification(settings: &serde_json::Value) -> Request {
    Request::build("workspace/didChangeConfiguration")
        .params(json!({ "settings": settings }))
        .finish()
}

#[tokio::test]
async fn should_update_settings_on_did_change_configuration() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Open a document with out-of-order keys. Before didChangeConfiguration
    // (keyOrdering=false), no ordering diagnostic should appear.
    let uri = "file:///test/config-change.yaml";
    send(
        &mut service,
        did_open_notification(uri, "zebra: 1\napple: 2\n"),
    )
    .await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(
            diags.is_empty(),
            "key_ordering is disabled by default, no ordering diagnostics expected"
        );
    }

    // Enable keyOrdering via didChangeConfiguration
    let resp = send(
        &mut service,
        did_change_configuration_notification(&json!({ "keyOrdering": true })),
    )
    .await;
    assert!(
        resp.is_none(),
        "didChangeConfiguration should not return a response"
    );

    // Re-open (or change) the document to trigger re-validation with new settings
    send(
        &mut service,
        did_change_notification(uri, "zebra: 1\napple: 2\n", 2),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after re-validation");
    assert!(
        !diags.is_empty(),
        "out-of-order keys should produce a diagnostic after keyOrdering is enabled"
    );
}
