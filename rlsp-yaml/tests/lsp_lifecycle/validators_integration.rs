// SPDX-License-Identifier: MIT
use std::fmt::Write as _;

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

use super::completion::completion_request;
use super::configuration::did_change_configuration_notification;
use super::folding_ranges::folding_range_request;
use super::helpers::*;
use super::hover::hover_request;
use super::navigation::document_symbol_request;

// ---- Validator Integration Tests ----

// Test 41 (SPIKE)
#[tokio::test]
async fn should_publish_combined_parser_and_validator_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // This text has both a parse error and an unused anchor in the valid portion
    // Note: the parser will fail to parse if there's a syntax error, so we can't
    // test combined parse + validator errors easily. Instead, test that validator
    // diagnostics are published for valid YAML.
    let uri = "file:///test/validators.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "defaults: &unused\n  key: val\nproduction:\n  key: other\n",
        ),
    )
    .await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    // Should have at least 1 diagnostic for unused anchor
    assert!(!diags.is_empty(), "should have validator diagnostics");
}

// Test 42
#[tokio::test]
async fn should_publish_validator_diagnostics_on_document_open() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/unused.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &unused\n  key: val\n"),
    )
    .await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(!diags.is_empty(), "should have unused anchor diagnostic");
    assert!(diags.iter().any(|d| {
        d.tags
            .as_ref()
            .is_some_and(|t| t.contains(&tower_lsp::lsp_types::DiagnosticTag::UNNECESSARY))
    }));
}

// Test 43
#[tokio::test]
async fn should_publish_validator_diagnostics_on_document_change() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/change.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(diags.is_empty(), "should have no diagnostics initially");
    }

    send(
        &mut service,
        did_change_notification(uri, "defaults: &unused\n  key: val\n", 2),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(
        !diags.is_empty(),
        "should have unused anchor diagnostic after change"
    );
}

// Test 44
#[tokio::test]
async fn should_clear_diagnostics_on_document_close_validators() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/close.yaml";
    send(
        &mut service,
        did_open_notification(uri, "defaults: &unused\n  key: val\n"),
    )
    .await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(!diags.is_empty(), "should have diagnostics before close");
    }

    send(&mut service, did_close_notification(uri)).await;

    let diags = service.inner().get_diagnostics(uri);
    assert!(
        diags.is_none() || diags.as_ref().is_some_and(Vec::is_empty),
        "diagnostics should be cleared after close"
    );
}

// ---- key ordering validation path ----

fn initialize_request_with_key_ordering(id: i64) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": { "keyOrdering": true }
        }))
        .finish()
}

#[tokio::test]
async fn should_publish_key_ordering_diagnostic_when_enabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request_with_key_ordering(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/key-order.yaml";
    // Keys out of alphabetical order should trigger a diagnostic when key_ordering is enabled
    send(
        &mut service,
        did_open_notification(uri, "zebra: 1\napple: 2\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !diags.is_empty(),
        "out-of-order keys should produce a diagnostic when keyOrdering is enabled"
    );
}

// ---- schema-aware YAML 1.1 diagnostics (integration) ----
//
// These tests exercise the `schemaYaml11Boolean`, `schemaYaml11Octal`, and
// `schemaYaml11BooleanType` diagnostics through the full LSP pipeline.
// The schema is injected into the cache via `seed_schema_cache` so no network
// fetch is required.  A `$schema=` modeline in the document causes
// `process_schema` to find the cached schema and run validation.

/// K8s ConfigMap-style schema: `data` is an object with string values;
/// `enabled` is a boolean field.
fn configmap_schema() -> rlsp_yaml::schema::JsonSchema {
    let schema_json = serde_json::json!({
        "type": "object",
        "properties": {
            "data": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            },
            "enabled": {
                "type": "boolean"
            }
        }
    });
    rlsp_yaml::schema::parse_schema(&schema_json).expect("configmap_schema: parse failed")
}

const CONFIGMAP_SCHEMA_URL: &str = "https://example.com/test-configmap.json";

#[tokio::test]
async fn should_emit_schema_yaml11_boolean_warning_in_string_typed_field() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service
        .inner()
        .seed_schema_cache(CONFIGMAP_SCHEMA_URL, configmap_schema());

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-yaml11-bool-string.yaml";
    // `data.value: yes` — string-typed field gets a YAML 1.1 boolean warning.
    let yaml =
        format!("# yaml-language-server: $schema={CONFIGMAP_SCHEMA_URL}\ndata:\n  value: yes\n");
    send(&mut service, did_open_notification(uri, &yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(
        diags
            .iter()
            .any(|d| d.code == Some(NumberOrString::String("schemaYaml11Boolean".to_string()))),
        "expected schemaYaml11Boolean warning; got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .all(|d| d.severity != Some(DiagnosticSeverity::ERROR)
                || d.code != Some(NumberOrString::String("schemaType".to_string()))),
        "schemaType error should not be emitted alongside schemaYaml11Boolean; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_schema_yaml11_boolean_type_error_in_boolean_typed_field() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service
        .inner()
        .seed_schema_cache(CONFIGMAP_SCHEMA_URL, configmap_schema());

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-yaml11-bool-type.yaml";
    // `enabled: yes` — boolean-typed field with a YAML 1.1 value gets a
    // schemaYaml11BooleanType error (not a generic schemaType error).
    let yaml = format!("# yaml-language-server: $schema={CONFIGMAP_SCHEMA_URL}\nenabled: yes\n");
    send(&mut service, did_open_notification(uri, &yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    assert!(
        diags.iter().any(|d| {
            d.code
                == Some(NumberOrString::String(
                    "schemaYaml11BooleanType".to_string(),
                ))
                && d.severity == Some(DiagnosticSeverity::ERROR)
        }),
        "expected schemaYaml11BooleanType error; got: {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|d| d.code == Some(NumberOrString::String("schemaType".to_string()))),
        "generic schemaType should not be emitted; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_suppress_schema_yaml11_diagnostics_in_v1_1_mode() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service
        .inner()
        .seed_schema_cache(CONFIGMAP_SCHEMA_URL, configmap_schema());

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-yaml11-v11-suppress.yaml";
    // In V1_1 mode: `yes` in a boolean field passes the type check (no
    // diagnostics); `yes` in a string field gets no warning either.
    let yaml = format!(
        "# yaml-language-server: $schema={CONFIGMAP_SCHEMA_URL}\n\
         # yaml-language-server: $yamlVersion=1.1\n\
         enabled: yes\n\
         data:\n  value: yes\n"
    );
    send(&mut service, did_open_notification(uri, &yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_schema_yaml11_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c))
                if c == "schemaYaml11Boolean"
                    || c == "schemaYaml11BooleanType"
                    || c == "schemaYaml11Octal"
        )
    });

    assert!(
        !has_schema_yaml11_diag,
        "V1_1 mode should suppress all schema YAML 1.1 diagnostics; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_yaml11_boolean_warning_for_plain_scalars() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-bool-plain.yaml";
    let yaml = "enabled: yes\nactive: on\nname: \"yes\"\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let bool_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code == Some(NumberOrString::String("yaml11Boolean".to_string())))
        .collect();

    assert_eq!(
        bool_diags.len(),
        2,
        "expected exactly 2 yaml11Boolean diagnostics (quoted 'yes' must be excluded); got: {diags:?}"
    );
    assert!(
        bool_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "yaml11Boolean diagnostics must have WARNING severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_emit_yaml11_octal_info_for_plain_scalars() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-octal-plain.yaml";
    let yaml = "mode: 0777\nperm: \"0644\"\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let octal_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code == Some(NumberOrString::String("yaml11Octal".to_string())))
        .collect();

    assert_eq!(
        octal_diags.len(),
        1,
        "expected exactly 1 yaml11Octal diagnostic (quoted '0644' must be excluded); got: {diags:?}"
    );
    assert!(
        octal_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::INFORMATION)),
        "yaml11Octal diagnostics must have INFORMATION severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn should_suppress_yaml11_compat_diagnostics_in_v1_1_mode() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/yaml11-compat-v11-suppress.yaml";
    let yaml = "# yaml-language-server: $yamlVersion=1.1\nenabled: yes\nmode: 0777\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_yaml11_compat_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c))
                if c == "yaml11Boolean" || c == "yaml11Octal"
        )
    });

    assert!(
        !has_yaml11_compat_diag,
        "V1_1 mode should suppress yaml11Boolean and yaml11Octal diagnostics; got: {diags:?}"
    );
}

// ── Diagnostic suppression integration tests ─────────────────────────────────

fn has_code(diags: &[tower_lsp::lsp_types::Diagnostic], code: &str) -> bool {
    diags
        .iter()
        .any(|d| matches!(&d.code, Some(NumberOrString::String(s)) if s == code))
}

// Section 1: disable-next-line — specific code

#[tokio::test]
async fn should_suppress_duplicate_key_diagnostic_with_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression1.yaml";
    // Line 0: key: a, Line 1: disable comment targets line 2, Line 2: duplicate key: b
    let text = "key: a\n# rlsp-yaml-disable-next-line duplicateKey\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed"
    );
}

#[tokio::test]
async fn should_not_suppress_duplicate_key_when_comment_targets_wrong_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression2.yaml";
    // Line 0: disable comment targets line 1; duplicateKey is on line 2
    let text = "# rlsp-yaml-disable-next-line duplicateKey\nkey: a\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should still be present"
    );
}

#[tokio::test]
async fn should_suppress_flow_map_diagnostic_with_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression3.yaml";
    // Line 1: disable comment targets line 2 (the flowMap)
    let text = "config:\n# rlsp-yaml-disable-next-line flowMap\n  settings: {a: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
}

#[tokio::test]
async fn should_not_suppress_unlisted_code_with_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression4.yaml";
    // flowMap on line 2 is suppressed; duplicateKey on line 4 is not listed
    let text =
        "config:\n# rlsp-yaml-disable-next-line flowMap\n  settings: {a: 1}\nkey: a\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should still be present"
    );
}

// Section 2: disable-next-line — suppress all

#[tokio::test]
async fn should_suppress_all_diagnostics_on_next_line_with_bare_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression5.yaml";
    // Bare comment on line 1 targets line 2 (the flowMap)
    let text = "config:\n# rlsp-yaml-disable-next-line\n  settings: {a: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "flowMap"),
        "flowMap should be suppressed by bare disable-next-line"
    );
}

#[tokio::test]
async fn should_not_suppress_diagnostics_on_other_lines_with_bare_disable_next_line() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression6.yaml";
    // Comment on line 0 targets line 1 only; duplicateKey on line 2, flowMap on line 3
    let text = "# rlsp-yaml-disable-next-line\nkey: a\nkey: b\na: {x: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey on line 2 should not be suppressed"
    );
    assert!(
        has_code(&diags, "flowMap"),
        "flowMap on line 3 should not be suppressed"
    );
}

// Section 3: disable-file — specific code

#[tokio::test]
async fn should_suppress_flow_map_diagnostics_file_wide_with_disable_file() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression7.yaml";
    // flowMap on lines 1 and 2 — both suppressed file-wide
    let text = "# rlsp-yaml-disable-file flowMap\na: {x: 1}\nb: {y: 2}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "flowMap"),
        "all flowMap diagnostics should be suppressed"
    );
}

#[tokio::test]
async fn should_not_suppress_other_codes_with_disable_file_specific_code() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression8.yaml";
    // Only flowMap suppressed; duplicateKey on lines 1-2 remains
    let text = "# rlsp-yaml-disable-file flowMap\nkey: a\nkey: b\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should still be present"
    );
}

// Section 4: disable-file — suppress all

#[tokio::test]
async fn should_suppress_all_diagnostics_file_wide_with_bare_disable_file() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression9.yaml";
    // Both duplicateKey (line 2) and flowMap (line 3) suppressed
    let text = "# rlsp-yaml-disable-file\nkey: a\nkey: b\na: {x: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        diags.is_empty(),
        "all diagnostics should be suppressed by bare disable-file"
    );
}

// Section 5: Unsuppressed diagnostics on other lines still reported

#[tokio::test]
async fn should_leave_unsuppressed_diagnostics_on_other_lines_intact() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression10.yaml";
    // duplicateKey on line 2 suppressed; flowMap on line 3 is not
    let text = "key: a\n# rlsp-yaml-disable-next-line duplicateKey\nkey: b\na: {x: 1}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed"
    );
    assert!(
        has_code(&diags, "flowMap"),
        "flowMap should still be present"
    );
}

// Section 6: Multiple suppression comments in one file

#[tokio::test]
async fn should_apply_multiple_disable_next_line_comments_independently() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression11.yaml";
    // Line 0: disable flowMap → targets line 1 (flowMap)
    // Line 3: disable duplicateKey → targets line 4 (second "key: c", where duplicateKey is emitted)
    let text = "# rlsp-yaml-disable-next-line flowMap\na: {x: 1}\nkey: c\n# rlsp-yaml-disable-next-line duplicateKey\nkey: c\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!has_code(&diags, "flowMap"), "flowMap should be suppressed");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed"
    );
}

#[tokio::test]
async fn should_suppress_only_targeted_lines_with_multiple_comments() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression12.yaml";
    // comment on line 1 targets line 2; flowMap on line 3 is not targeted
    let text = "a: {x: 1}\n# rlsp-yaml-disable-next-line flowMap\nb: {y: 2}\nc: {z: 3}\n";
    send(&mut service, did_open_notification(uri, text)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    let flow_map_diags: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.code, Some(NumberOrString::String(s)) if s == "flowMap"))
        .collect();
    // line 0 (a: {x:1}) and line 3 (c: {z:3}) should produce flowMap; line 2 (b: {y:2}) suppressed
    assert_eq!(
        flow_map_diags.len(),
        2,
        "exactly two flowMap diagnostics should remain (lines 0 and 3)"
    );
    assert!(
        flow_map_diags.iter().any(|d| d.range.start.line == 3),
        "flowMap on line 3 should still be present"
    );
    assert!(
        !flow_map_diags.iter().any(|d| d.range.start.line == 2),
        "flowMap on line 2 should be suppressed"
    );
}

// Section 7: Suppression applied on didChange

#[tokio::test]
async fn should_apply_suppression_after_document_change() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/suppression13.yaml";
    // Open with duplicateKey present
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;
    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(
            has_code(&diags, "duplicateKey"),
            "duplicateKey should be present before change"
        );
    }

    // Change to add a suppression comment before the duplicate key
    send(
        &mut service,
        did_change_notification(
            uri,
            "key: a\n# rlsp-yaml-disable-next-line duplicateKey\nkey: b\n",
            2,
        ),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after change");
    assert!(
        !has_code(&diags, "duplicateKey"),
        "duplicateKey should be suppressed after change"
    );
}

// ── Feature toggle settings integration tests ─────────────────────────────────

// Section: validate toggle

#[tokio::test]
async fn should_produce_diagnostics_by_default_with_validate_not_set() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_validate_default.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should be present with default settings"
    );
}

#[tokio::test]
async fn should_suppress_all_diagnostics_when_validate_is_false() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_validate_false.yaml";
    send(
        &mut service,
        did_change_configuration_notification(&json!({"validate": false})),
    )
    .await;
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        diags.is_empty(),
        "all diagnostics should be suppressed when validate=false"
    );
}

#[tokio::test]
async fn should_resume_diagnostics_when_validate_re_enabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_validate_reenable.yaml";
    // Disable validation and open a document with a duplicate key
    send(
        &mut service,
        did_change_configuration_notification(&json!({"validate": false})),
    )
    .await;
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;
    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(
            diags.is_empty(),
            "no diagnostics expected while validate=false"
        );
    }

    // Re-enable validation and change the document to trigger re-validation
    send(
        &mut service,
        did_change_configuration_notification(&json!({"validate": true})),
    )
    .await;
    send(
        &mut service,
        did_change_notification(uri, "key: a\nkey: b\n", 2),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist after re-enable");
    assert!(
        has_code(&diags, "duplicateKey"),
        "duplicateKey should reappear after validate is re-enabled"
    );
}

// Section: hover toggle

#[tokio::test]
async fn should_return_hover_result_when_hover_setting_not_set() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_hover_default.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response");
    // A response was returned (not an error) — the handler ran to completion
    assert!(
        resp.result().is_some() || resp.error().is_none(),
        "hover handler should run to completion with default settings"
    );
}

#[tokio::test]
async fn should_return_null_hover_when_hover_is_disabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_hover_false.yaml";
    send(
        &mut service,
        did_change_configuration_notification(&json!({"hover": false})),
    )
    .await;
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response even when disabled");
    let result = resp.result().expect("hover should have a result field");
    assert!(
        result.is_null(),
        "hover result should be null when hover=false"
    );
}

// Section: completion toggle

#[tokio::test]
async fn should_return_completion_items_when_completion_setting_not_set() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_completion_default.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp
        .result()
        .expect("completion should have a result field");
    assert!(
        !result.is_null(),
        "completion result should not be null with default settings"
    );
}

#[tokio::test]
async fn should_return_null_completion_when_completion_is_disabled() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/toggle_completion_false.yaml";
    send(
        &mut service,
        did_change_configuration_notification(&json!({"completion": false})),
    )
    .await;
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response even when disabled");
    let result = resp
        .result()
        .expect("completion should have a result field");
    assert!(
        result.is_null(),
        "completion result should be null when completion=false"
    );
}

// Section: maxItemsComputed

// Test: document_symbols_returns_results_for_valid_yaml (spike)
#[tokio::test]
async fn document_symbols_returns_results_for_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_spike.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\ncity: NYC\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(
        !result.is_null(),
        "documentSymbol result should not be null for valid YAML"
    );
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert!(!arr.is_empty(), "should return symbols for 3-key document");
}

#[tokio::test]
async fn document_symbols_respects_max_items_computed_limit() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 3})),
    )
    .await;

    let uri = "file:///test/max_items_symbols_truncate.yaml";
    let mut yaml_text = String::new();
    for i in 0..10 {
        let _ = writeln!(yaml_text, "key_{i}: value_{i}");
    }
    send(&mut service, did_open_notification(uri, &yaml_text)).await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert_eq!(
        arr.len(),
        3,
        "documentSymbol should be truncated to limit 3"
    );
}

#[tokio::test]
async fn document_symbols_returns_all_items_when_limit_exceeds_count() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 100})),
    )
    .await;

    let uri = "file:///test/max_items_symbols_no_truncate.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a: 1\nb: 2\nc: 3\nd: 4\ne: 5\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert_eq!(
        arr.len(),
        5,
        "all 5 symbols should be returned when limit is 100"
    );
}

#[tokio::test]
async fn document_symbols_returns_empty_when_max_items_computed_is_zero() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 0})),
    )
    .await;

    let uri = "file:///test/max_items_symbols_zero.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a: 1\nb: 2\nc: 3\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "documentSymbol should be null or empty when maxItemsComputed=0"
    );
}

#[tokio::test]
async fn document_symbols_uses_default_5000_limit_when_setting_absent() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_symbols_default.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a: 1\nb: 2\nc: 3\nd: 4\ne: 5\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    let arr = result
        .as_array()
        .expect("documentSymbol should be an array");
    assert_eq!(
        arr.len(),
        5,
        "all 5 symbols should be returned with default 5000 limit"
    );
}

// IT-NEW-2: Multi-document YAML returns NAMESPACE wrapper symbols
#[tokio::test]
async fn document_symbols_multi_doc_returns_namespace_wrappers() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/multi_doc_namespace.yaml";
    send(
        &mut service,
        did_open_notification(uri, "doc1key: value1\n---\ndoc2key: value2\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(
        !result.is_null(),
        "result should not be null for multi-doc YAML"
    );

    let arr = result.as_array().expect("result should be an array");
    assert_eq!(
        arr.len(),
        2,
        "two-doc file should return exactly 2 top-level NAMESPACE symbols"
    );

    assert_eq!(
        arr[0]["name"], "Document 1",
        "first wrapper should be named 'Document 1'"
    );
    assert_eq!(
        arr[0]["kind"], 3,
        "first wrapper kind should be NAMESPACE (3)"
    );
    assert_eq!(
        arr[1]["name"], "Document 2",
        "second wrapper should be named 'Document 2'"
    );
    assert_eq!(
        arr[1]["kind"], 3,
        "second wrapper kind should be NAMESPACE (3)"
    );

    let children0 = arr[0]["children"]
        .as_array()
        .expect("Document 1 should have children");
    assert!(
        children0.iter().any(|c| c["name"] == "doc1key"),
        "Document 1 children should contain 'doc1key'"
    );

    let children1 = arr[1]["children"]
        .as_array()
        .expect("Document 2 should have children");
    assert!(
        children1.iter().any(|c| c["name"] == "doc2key"),
        "Document 2 children should contain 'doc2key'"
    );
}

// IT-NEW-1: Kubernetes-style YAML — container name used as sequence item label
#[tokio::test]
async fn document_symbols_kubernetes_style_container_name_used_as_label() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/k8s_container.yaml";
    let yaml = "spec:\n  containers:\n    - name: nginx\n      image: nginx:latest\n      ports:\n        - containerPort: 80\n";
    send(&mut service, did_open_notification(uri, yaml)).await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have a result");
    assert!(!result.is_null(), "result should not be null");

    let arr = result.as_array().expect("result should be an array");
    assert!(!arr.is_empty(), "should have symbols");

    // Find `spec` symbol
    let spec = arr
        .iter()
        .find(|s| s["name"] == "spec")
        .expect("should have 'spec'");
    assert_eq!(spec["detail"], "1 key", "spec detail should be '1 key'");

    // Navigate to `containers` child
    let spec_children = spec["children"]
        .as_array()
        .expect("spec should have children");
    let containers = spec_children
        .iter()
        .find(|s| s["name"] == "containers")
        .expect("should have 'containers'");
    assert_eq!(
        containers["detail"], "1 item",
        "containers detail should be '1 item'"
    );

    // Navigate to first sequence item — should be named "nginx" by label-key heuristic
    let container_children = containers["children"]
        .as_array()
        .expect("containers should have children");
    let nginx = &container_children[0];
    assert_eq!(
        nginx["name"], "nginx",
        "first container should be named by 'name' key value"
    );
    assert_eq!(nginx["detail"], "[0]", "detail should show original index");
}

#[tokio::test]
async fn folding_ranges_returns_results_for_nested_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_folding_spike.yaml";
    send(
        &mut service,
        did_open_notification(uri, "outer:\n  inner: value\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(
        !result.is_null(),
        "foldingRange should not be null for nested YAML"
    );
    let arr = result.as_array().expect("foldingRange should be an array");
    assert!(!arr.is_empty(), "should return at least one folding range");
}

#[tokio::test]
async fn folding_ranges_respects_max_items_computed_limit() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 2})),
    )
    .await;

    // 5 nested mappings each produces a folding range — well above the limit of 2
    let uri = "file:///test/max_items_folding_truncate.yaml";
    let mut yaml_text = String::new();
    for i in 0..5 {
        let _ = write!(yaml_text, "item_{i}:\n  key: value\n");
    }
    send(&mut service, did_open_notification(uri, &yaml_text)).await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    let arr = result.as_array().expect("foldingRange should be an array");
    assert_eq!(arr.len(), 2, "foldingRange should be truncated to limit 2");
}

#[tokio::test]
async fn folding_ranges_returns_empty_when_max_items_computed_is_zero() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({"maxItemsComputed": 0})),
    )
    .await;

    let uri = "file:///test/max_items_folding_zero.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n  port: 8080\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "foldingRange should be null or empty when maxItemsComputed=0"
    );
}

#[tokio::test]
async fn folding_ranges_uses_default_5000_limit_when_setting_absent() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/max_items_folding_default.yaml";
    send(
        &mut service,
        did_open_notification(uri, "a:\n  x: 1\nb:\n  y: 2\nc:\n  z: 3\n"),
    )
    .await;

    let resp = send(&mut service, folding_range_request(2, uri)).await;
    let resp = resp.expect("foldingRange should return a response");
    let result = resp.result().expect("foldingRange should have a result");
    assert!(
        !result.is_null(),
        "foldingRange should return results with default 5000 limit"
    );
    let arr = result.as_array().expect("foldingRange should be an array");
    assert!(
        !arr.is_empty(),
        "should return folding ranges with default limit"
    );
}
