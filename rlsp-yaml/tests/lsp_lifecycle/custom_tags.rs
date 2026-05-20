// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::NumberOrString;

use super::helpers::*;

pub fn initialize_request_with_custom_tags(id: i64, tags: &[&str]) -> Request {
    let tag_array: Vec<serde_json::Value> = tags.iter().map(|t| json!(t)).collect();
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": { "customTags": tag_array }
        }))
        .finish()
}

#[tokio::test]
async fn should_emit_unknown_tag_for_tag_not_in_custom_tags_list() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(
        &mut service,
        initialize_request_with_custom_tags(1, &["!include"]),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/tags.yaml";
    // !unknown is not in the allowed list → unknownTag diagnostic
    send(
        &mut service,
        did_open_notification(uri, "value: !unknown foo\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let unknown_tag_count = diags
        .iter()
        .filter(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag"))
        .count();
    assert_eq!(
        unknown_tag_count, 1,
        "expected 1 unknownTag diagnostic, got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_tag_type_mismatch_when_tag_type_annotation_does_not_match_node() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    // Configure !include to expect a scalar, but the YAML has !include on a mapping
    send(
        &mut service,
        initialize_request_with_custom_tags(1, &["!include scalar", "!ref"]),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/tag-type.yaml";
    // !include expects scalar but gets a mapping → tagTypeMismatch
    // !ref has no type annotation → no diagnostic (it's in the allowed list)
    send(
        &mut service,
        did_open_notification(uri, "a: !include {key: val}\nb: !ref bar\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let mismatch_count = diags
        .iter()
        .filter(|d| {
            matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "tagTypeMismatch")
        })
        .count();
    let has_unknown = diags
        .iter()
        .any(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag"));
    assert_eq!(
        mismatch_count, 1,
        "expected 1 tagTypeMismatch diagnostic for !include on mapping, got: {diags:?}"
    );
    assert!(
        !has_unknown,
        "!ref is in allowed list and has no type annotation — no unknownTag expected, got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_no_diagnostic_when_tag_type_annotation_matches_node() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    // Configure !include to expect a scalar; YAML has a scalar → no diagnostic
    send(
        &mut service,
        initialize_request_with_custom_tags(1, &["!include scalar"]),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/tag-match.yaml";
    send(
        &mut service,
        did_open_notification(uri, "value: !include path/to/file.yaml\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let has_tag_diag = diags.iter().any(|d| {
        matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag" || s == "tagTypeMismatch")
    });
    assert!(
        !has_tag_diag,
        "scalar !include matches expected type scalar — no tag diagnostics expected, got: {diags:?}"
    );
}
