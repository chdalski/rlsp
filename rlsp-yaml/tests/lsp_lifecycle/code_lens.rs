// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn code_lens_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/codeLens")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_null_code_lens_when_no_schema_association() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/no-schema.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "codeLens result should be null when no schema is associated"
    );
}

#[tokio::test]
async fn should_return_code_lens_when_schema_modeline_is_present() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-doc.yaml";
    // The $schema modeline causes process_schema to store the association even
    // when the fetch fails (network unavailable in tests). code_lens then finds
    // the association and returns a lens with the URL as the title.
    send(
        &mut service,
        did_open_notification(
            uri,
            "# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json\nkey: value\n",
        ),
    )
    .await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    assert!(
        !result.is_null(),
        "codeLens result should not be null when schema modeline is present"
    );
    let arr = result.as_array().expect("codeLens result should be array");
    assert!(!arr.is_empty(), "codeLens should return at least one lens");
    let result_str = serde_json::to_string(&arr[0]).expect("serialize lens");
    assert!(
        result_str.contains("json.schemastore.org"),
        "lens command title or arguments should reference the schema URL"
    );
}
