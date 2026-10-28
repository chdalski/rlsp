// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::{Diagnostic, NumberOrString};

use super::fake_proxy::{FakeProxy, Reply, SCHEMA_HOST};
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

// ---- Rejected settings ----

const OUT_OF_ORDER: &str = "zebra: 1\napple: 2\n";

fn has_code(diagnostics: &[Diagnostic], code: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == code))
}

/// Open `text` as `uri` and return its diagnostics.
async fn diagnostics_after_opening(
    service: &mut LspService<Backend>,
    uri: &str,
    text: &str,
) -> Vec<Diagnostic> {
    send(service, did_open_notification(uri, text)).await;
    service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist")
}

/// Re-validate `uri` with `text` as its new content and return the diagnostics.
async fn diagnostics_after_changing(
    service: &mut LspService<Backend>,
    uri: &str,
    text: &str,
    version: i32,
) -> Vec<Diagnostic> {
    send(service, did_change_notification(uri, text, version)).await;
    service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist")
}

/// Settings with a schema association that loads and one that is rejected.
fn schemas_settings(proxy: &FakeProxy) -> serde_json::Value {
    json!({
        "httpProxy": proxy.url(),
        "schemaStore": false,
        "schemas": {
            format!("http://{SCHEMA_HOST}/s.json"): "**/*.yaml",
            "http://bad.invalid/s.json": 5,
        },
    })
}

async fn assert_schema_association_applied_and_one_warning_for_the_rejected_entry(
    service: &mut LspService<Backend>,
    log: &mut tokio::sync::mpsc::UnboundedReceiver<tower_lsp::lsp_types::LogMessageParams>,
    proxy: &FakeProxy,
) {
    send(
        service,
        did_open_notification("file:///test/schemas.yaml", "key: value\n"),
    )
    .await;
    let warnings = warnings_before_sentinel(service, log).await;

    let seen = proxy.seen();
    assert!(
        seen.iter()
            .any(|line| line.starts_with(&format!("CONNECT {SCHEMA_HOST}"))),
        "the valid association must apply: {seen:?}"
    );
    assert!(
        !seen.iter().any(|line| line.contains("bad.invalid")),
        "the rejected association must not apply: {seen:?}"
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings
            .first()
            .is_some_and(|w| w.starts_with("Setting schemas, entry http://bad.invalid/s.json")),
        "{warnings:?}"
    );
}

#[tokio::test]
async fn should_warn_for_a_wrong_custom_tags_entry_and_keep_the_other_settings_at_initialization() {
    let (mut service, mut log) =
        start_server(json!({ "customTags": ["!ok", 5], "keyOrdering": true })).await;

    let tags = diagnostics_after_opening(
        &mut service,
        "file:///test/tags.yaml",
        "a: !ok x\nb: !other y\n",
    )
    .await;
    let ordering =
        diagnostics_after_opening(&mut service, "file:///test/order.yaml", OUT_OF_ORDER).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("Setting customTags, entry 1 "),
        "{warnings:?}"
    );
    assert_eq!(
        tags.iter()
            .filter(
                |d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag")
            )
            .count(),
        1,
        "only !other is unknown: {tags:?}"
    );
    assert!(!ordering.is_empty(), "keyOrdering must apply: {ordering:?}");
}

#[tokio::test]
async fn should_warn_once_and_use_the_default_for_a_wrong_typed_field_at_initialization() {
    let (mut service, mut log) =
        start_server(json!({ "keyOrdering": "yes", "customTags": ["!ok"] })).await;

    let ordering =
        diagnostics_after_opening(&mut service, "file:///test/order.yaml", OUT_OF_ORDER).await;
    let tags =
        diagnostics_after_opening(&mut service, "file:///test/tags.yaml", "a: !ok x\n").await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("Setting keyOrdering "),
        "{warnings:?}"
    );
    assert!(
        ordering.is_empty(),
        "keyOrdering falls back to false: {ordering:?}"
    );
    assert!(
        !has_code(&tags, "unknownTag"),
        "customTags must apply: {tags:?}"
    );
}

#[tokio::test]
async fn should_warn_for_a_wrong_schemas_entry_and_apply_the_valid_one_at_initialization() {
    let proxy = FakeProxy::start(Reply::ok(Some("application/json"), "{}".to_string()));
    let (mut service, mut log) = start_server(schemas_settings(&proxy)).await;

    assert_schema_association_applied_and_one_warning_for_the_rejected_entry(
        &mut service,
        &mut log,
        &proxy,
    )
    .await;
}

#[tokio::test]
async fn should_send_one_warning_per_rejection_in_field_order_at_initialization() {
    let (mut service, mut log) = start_server(json!({
        "httpProxy": 1,
        "customTags": [1, "!a", null],
        "keyOrdering": "x",
    }))
    .await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    let subjects: Vec<_> = warnings
        .iter()
        .map(|w| w.split(" must be").next().unwrap_or_default())
        .collect();
    assert_eq!(
        subjects,
        [
            "Setting customTags, entry 0",
            "Setting customTags, entry 2",
            "Setting keyOrdering",
            "Setting httpProxy"
        ]
    );
}

#[rstest]
#[case::unknown_key(json!({ "fooBar": 1, "keyOrdering": true }))]
#[case::null_values(json!({ "keyOrdering": null, "customTags": null, "schemas": null, "formatPrintWidth": null }))]
#[tokio::test]
async fn should_not_warn_for_unknown_keys_or_null_values_at_initialization(
    #[case] options: serde_json::Value,
) {
    let (mut service, mut log) = start_server(options).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[case::null(json!(null))]
#[case::string(json!("str"))]
#[case::array(json!([1]))]
#[case::number(json!(5))]
#[tokio::test]
async fn should_apply_defaults_without_warning_for_non_object_initialization_options(
    #[case] options: serde_json::Value,
) {
    let (mut service, mut log) = start_server(options).await;

    let ordering =
        diagnostics_after_opening(&mut service, "file:///test/order.yaml", OUT_OF_ORDER).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(ordering.is_empty(), "{ordering:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[tokio::test]
async fn should_apply_defaults_without_warning_for_missing_initialization_options() {
    let (mut service, socket) = LspService::new(Backend::new);
    let mut log = collect_log_messages(socket);
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(warnings.is_empty(), "{warnings:?}");
}

#[tokio::test]
async fn should_warn_and_apply_valid_fields_when_a_configuration_change_has_a_wrong_typed_field() {
    let (mut service, mut log) = start_server(json!({})).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "keyOrdering": true, "validate": "off" })),
    )
    .await;
    let ordering =
        diagnostics_after_changing(&mut service, "file:///test/order.yaml", OUT_OF_ORDER, 1).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].starts_with("Setting validate "), "{warnings:?}");
    assert!(!ordering.is_empty(), "keyOrdering must apply: {ordering:?}");
}

#[tokio::test]
async fn should_use_the_default_not_the_previous_value_for_a_rejected_field_on_configuration_change()
 {
    let (mut service, mut log) = start_server(json!({ "keyOrdering": true })).await;
    let uri = "file:///test/order.yaml";
    let before = diagnostics_after_opening(&mut service, uri, OUT_OF_ORDER).await;
    assert!(
        !before.is_empty(),
        "keyOrdering applies at initialization: {before:?}"
    );

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "keyOrdering": "yes" })),
    )
    .await;
    let after = diagnostics_after_changing(&mut service, uri, OUT_OF_ORDER, 2).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(
        after.is_empty(),
        "the rejected field takes its default: {after:?}"
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[tokio::test]
async fn should_warn_for_a_wrong_schemas_entry_and_apply_the_valid_one_on_configuration_change() {
    let proxy = FakeProxy::start(Reply::ok(Some("application/json"), "{}".to_string()));
    let (mut service, mut log) = start_server(json!({})).await;

    send(
        &mut service,
        did_change_configuration_notification(&schemas_settings(&proxy)),
    )
    .await;

    assert_schema_association_applied_and_one_warning_for_the_rejected_entry(
        &mut service,
        &mut log,
        &proxy,
    )
    .await;
}

#[rstest]
#[case::null(json!(null))]
#[case::array(json!([]))]
#[case::string(json!("str"))]
#[case::number(json!(5))]
#[case::boolean(json!(true))]
#[tokio::test]
async fn should_ignore_a_non_object_configuration_change_without_warning(
    #[case] payload: serde_json::Value,
) {
    let (mut service, mut log) = start_server(json!({ "keyOrdering": true })).await;
    let uri = "file:///test/order.yaml";

    send(
        &mut service,
        did_change_configuration_notification(&payload),
    )
    .await;
    let ordering = diagnostics_after_opening(&mut service, uri, OUT_OF_ORDER).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(!ordering.is_empty(), "the settings must stay: {ordering:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[case::control_and_bidi_value(json!({ "keyOrdering": "a\n[ERROR] forged\r\u{1b}[31m\u{202e}\u{2028}" }))]
#[case::long_value(json!({ "formatPrintWidth": "9".repeat(10_000) }))]
#[case::long_array(json!({ "keyOrdering": vec![0; 100_000] }))]
#[case::schema_key_with_credentials(
    json!({ "schemas": { "https://user:secret@h.invalid/s.json?token=abc#frag": 5 } })
)]
#[case::url_setting_value(json!({ "httpProxy": { "u": "http://user:secret@proxy.invalid" } }))]
#[tokio::test]
async fn should_sanitize_cap_and_withhold_secrets_in_a_rejection_warning(
    #[case] options: serde_json::Value,
) {
    let (mut service, mut log) = start_server(options).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let message = &warnings[0];
    assert!(message.chars().count() <= 512, "{message}");
    assert!(
        !message
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{202e}' | '\u{2028}')),
        "{message:?}"
    );
    for secret in ["secret", "token", "frag"] {
        assert!(!message.contains(secret), "{message}");
    }
}
