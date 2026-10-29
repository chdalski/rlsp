// SPDX-License-Identifier: MIT
//! A `SchemaStore` `fileMatch` pattern applies to a document by the rules of
//! the `schemas` setting, from a catalog seeded without a network fetch.
use rstest::rstest;
use serde_json::json;
use tower_lsp::lsp_types::{Diagnostic, NumberOrString};

use super::helpers::*;

const SCHEMA_URL: &str = "https://example.com/compose.json";

/// Open `uri` with a document the schema rejects, in a server whose seeded
/// catalog holds one entry for `SCHEMA_URL` with the `file_match` patterns,
/// and return the published diagnostics.
async fn open_with_seeded_catalog(
    settings: serde_json::Value,
    file_match: &[&str],
    uri: &str,
) -> Vec<Diagnostic> {
    let (mut service, mut log) = start_server(settings).await;
    let catalog = rlsp_yaml::schema::parse_schemastore_catalog(&json!({
        "schemas": [{"name": "Compose", "url": SCHEMA_URL, "fileMatch": file_match}]
    }))
    .expect("test catalog");
    service.inner().seed_schemastore_catalog(catalog);
    service.inner().seed_schema_cache(
        SCHEMA_URL,
        rlsp_yaml::schema::parse_schema(&json!({"additionalProperties": false}))
            .expect("test schema"),
    );

    send(&mut service, did_open_notification(uri, "bogus: 1\n")).await;
    let _ = warnings_before_sentinel(&mut service, &mut log).await;

    service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist")
}

fn schema_diagnostic_count(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c.starts_with("schema")))
        .count()
}

#[rstest]
#[case::bare_name_applies_below_a_directory("file:///p/docker-compose.yml", 1)]
#[case::exclusion_keeps_the_schema_off_an_excluded_file("file:///p/legacy/docker-compose.yml", 0)]
#[case::unrelated_file_is_left_alone("file:///p/other.yml", 0)]
#[tokio::test]
async fn schemastore_pattern_applies_by_the_upstream_rules(
    #[case] uri: &str,
    #[case] expected: usize,
) {
    let diagnostics = open_with_seeded_catalog(
        json!({}),
        &["docker-compose.yml", "!legacy/docker-compose.yml"],
        uri,
    )
    .await;

    assert_eq!(schema_diagnostic_count(&diagnostics), expected);
}

#[tokio::test]
async fn schemastore_catalog_is_skipped_when_schema_store_is_disabled() {
    let diagnostics = open_with_seeded_catalog(
        json!({"schemaStore": false}),
        &["docker-compose.yml"],
        "file:///p/docker-compose.yml",
    )
    .await;

    assert_eq!(schema_diagnostic_count(&diagnostics), 0);
}
