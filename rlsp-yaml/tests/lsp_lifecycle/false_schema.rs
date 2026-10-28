// SPDX-License-Identifier: MIT
//! A `false` sub-schema of a schema takes effect: the value it covers is
//! reported.
use serde_json::json;
use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

use super::helpers::*;

const SCHEMA_URL: &str = "https://example.com/false-sub-schema.json";
const DOC_URI: &str = "file:///test/false-sub-schema.yaml";

#[tokio::test]
async fn should_publish_schema_not_allowed_at_the_value_of_a_property_whose_schema_is_false() {
    let schema = json!({
        "type": "object",
        "properties": { "ok": { "type": "string" }, "forbidden": false }
    });
    let (mut service, mut log) = start_server(json!({})).await;
    service.inner().seed_schema_cache(
        SCHEMA_URL,
        rlsp_yaml::schema::parse_schema(&schema).expect("test schema"),
    );

    let text = format!("# yaml-language-server: $schema={SCHEMA_URL}\nok: a\nforbidden: 1\n");
    send(&mut service, did_open_notification(DOC_URI, &text)).await;
    warnings_before_sentinel(&mut service, &mut log).await;

    let diagnostics = service
        .inner()
        .get_diagnostics(DOC_URI)
        .expect("diagnostics should exist");
    let not_allowed: Vec<_> = diagnostics
        .iter()
        .filter(
            |d| matches!(&d.code, Some(NumberOrString::String(code)) if code == "schemaNotAllowed"),
        )
        .collect();
    assert_eq!(not_allowed.len(), 1, "{diagnostics:?}");
    let diagnostic = not_allowed[0];
    assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(
        diagnostic.message,
        "Value at forbidden is not allowed by the schema"
    );
    // The value `1` of `forbidden: 1` on the third line of the document.
    assert_eq!(diagnostic.range.start.line, 2);
    assert_eq!(diagnostic.range.start.character, 11);
    assert_eq!(diagnostic.range.end.character, 12);
}
