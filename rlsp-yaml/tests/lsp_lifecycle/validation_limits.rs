// SPDX-License-Identifier: MIT
//! The client log tells the user when schema validation stopped early.
use rstest::rstest;
use serde_json::json;
use tower_lsp::lsp_types::{Diagnostic, NumberOrString};

use super::helpers::*;

const SCHEMA_URL: &str = "https://example.com/hostile.json";
const DOC_URI: &str = "file:///test/validation-limits.yaml";

/// The per-run diagnostic limit (`MAX_DIAGNOSTICS` in `schema_validation`).
const DIAGNOSTIC_LIMIT: usize = 10_000;

/// `levels` nested `anyOf`s of `width` branches each, every leaf failing, with
/// `unevaluatedProperties` and `unevaluatedItems` at every level. Unbounded,
/// evaluating it takes `width.pow(levels)` leaf probes, far beyond the budget.
fn hostile_composition(levels: usize, width: usize) -> serde_json::Value {
    if levels == 0 {
        return json!({"required": ["z"]});
    }
    json!({
        "anyOf": vec![hostile_composition(levels - 1, width); width],
        "unevaluatedProperties": false,
        "unevaluatedItems": {"not": {}}
    })
}

/// A schema that rejects every key, so each key of a document yields a
/// diagnostic.
fn no_properties_schema() -> serde_json::Value {
    json!({"additionalProperties": false})
}

/// A document of `count` scalar keys.
fn keys_document(count: usize) -> String {
    (0..count)
        .map(|i| format!("k{i}: 1\n"))
        .collect::<Vec<_>>()
        .concat()
}

/// Everything a server sent and published after opening a document.
struct Outcome {
    /// Warnings that report incomplete validation.
    incomplete_warnings: Vec<String>,
    diagnostics: Vec<Diagnostic>,
}

/// Seed `schema` under `schema_url`, open `text` (preceded by a modeline for
/// `schema_url` when `modeline` is set), and collect the outcome.
async fn open_with_seeded_schema(
    schema_url: &str,
    schema: &serde_json::Value,
    modeline: bool,
    text: &str,
) -> Outcome {
    let (mut service, mut log) = start_server(json!({})).await;
    service.inner().seed_schema_cache(
        schema_url,
        rlsp_yaml::schema::parse_schema(schema).expect("test schema"),
    );

    let text = if modeline {
        format!("# yaml-language-server: $schema={schema_url}\n{text}")
    } else {
        text.to_string()
    };
    send(&mut service, did_open_notification(DOC_URI, &text)).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;
    Outcome {
        incomplete_warnings: warnings
            .into_iter()
            .filter(|w| w.contains("incomplete"))
            .collect(),
        diagnostics: service
            .inner()
            .get_diagnostics(DOC_URI)
            .expect("diagnostics should exist"),
    }
}

fn schema_diagnostic_count(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c.starts_with("schema")))
        .count()
}

#[rstest]
#[case::step_budget(hostile_composition(5, 8), "a: 1\nb: [1, 2]\n".to_string())]
#[case::diagnostic_limit(no_properties_schema(), keys_document(DIAGNOSTIC_LIMIT + 1))]
#[tokio::test]
async fn should_warn_when_validation_stops_early(
    #[case] schema: serde_json::Value,
    #[case] text: String,
) {
    let outcome = open_with_seeded_schema(SCHEMA_URL, &schema, true, &text).await;

    assert_eq!(outcome.incomplete_warnings.len(), 1);
    assert!(
        outcome.incomplete_warnings[0].contains(SCHEMA_URL),
        "{:?}",
        outcome.incomplete_warnings
    );
}

#[tokio::test]
async fn should_cap_published_diagnostics_at_the_limit_when_the_diagnostic_limit_stops_the_run() {
    let text = keys_document(DIAGNOSTIC_LIMIT + 1);

    let outcome = open_with_seeded_schema(SCHEMA_URL, &no_properties_schema(), true, &text).await;

    let schema_diagnostics = schema_diagnostic_count(&outcome.diagnostics);
    assert_eq!(schema_diagnostics, DIAGNOSTIC_LIMIT);
}

#[tokio::test]
async fn should_warn_for_auto_detected_kubernetes_schema_when_validation_stops_early() {
    let pod = rlsp_yaml::schema::KubernetesResource::new("v1", "Pod").expect("valid resource");
    let url = rlsp_yaml::schema::kubernetes_schema_url(
        &pod,
        &rlsp_yaml::schema::KubernetesVersion::master(),
    )
    .expect("core resource has a URL");

    let outcome = open_with_seeded_schema(
        &url,
        &hostile_composition(5, 8),
        false,
        "apiVersion: v1\nkind: Pod\na: 1\nb: [1, 2]\n",
    )
    .await;

    assert_eq!(outcome.incomplete_warnings.len(), 1);
    assert!(
        outcome.incomplete_warnings[0].contains(&url),
        "{:?}",
        outcome.incomplete_warnings
    );
}

#[tokio::test]
async fn should_warn_again_on_every_incomplete_run() {
    let (mut service, mut log) = start_server(json!({})).await;
    service.inner().seed_schema_cache(
        SCHEMA_URL,
        rlsp_yaml::schema::parse_schema(&hostile_composition(5, 8)).expect("test schema"),
    );
    let text = format!("# yaml-language-server: $schema={SCHEMA_URL}\na: 1\nb: [1, 2]\n");

    send(&mut service, did_open_notification(DOC_URI, &text)).await;
    send(&mut service, did_change_notification(DOC_URI, &text, 2)).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;
    let incomplete = warnings.iter().filter(|w| w.contains("incomplete")).count();
    assert_eq!(incomplete, 2, "{warnings:?}");
}

#[tokio::test]
async fn should_not_leak_credentials_in_incomplete_warning() {
    let raw_url = "https://user:secret@example.com/hostile.json?token=abc#frag";

    let outcome = open_with_seeded_schema(
        raw_url,
        &hostile_composition(5, 8),
        true,
        "a: 1\nb: [1, 2]\n",
    )
    .await;

    assert_eq!(outcome.incomplete_warnings.len(), 1);
    let warning = &outcome.incomplete_warnings[0];
    assert!(
        warning.contains("https://example.com/hostile.json"),
        "{warning}"
    );
    for leaked in ["user", "secret", "token", "abc", "frag"] {
        assert!(!warning.contains(leaked), "{leaked} leaked: {warning}");
    }
}

#[rstest]
#[case::complete_with_diagnostics(
    json!({"properties": {"a": {"type": "integer"}}}),
    "a: not-a-number\n".to_string(),
    1
)]
#[case::exactly_at_the_diagnostic_limit(
    no_properties_schema(),
    keys_document(DIAGNOSTIC_LIMIT),
    DIAGNOSTIC_LIMIT
)]
#[tokio::test]
async fn should_not_warn_when_validation_completes(
    #[case] schema: serde_json::Value,
    #[case] text: String,
    #[case] expected_schema_diagnostics: usize,
) {
    let outcome = open_with_seeded_schema(SCHEMA_URL, &schema, true, &text).await;

    assert!(
        outcome.incomplete_warnings.is_empty(),
        "{:?}",
        outcome.incomplete_warnings
    );
    let schema_diagnostics = schema_diagnostic_count(&outcome.diagnostics);
    assert_eq!(schema_diagnostics, expected_schema_diagnostics);
}
