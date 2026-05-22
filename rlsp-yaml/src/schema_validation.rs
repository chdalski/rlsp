// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::lsp_util::span_to_lsp;
use crate::schema::JsonSchema;
use crate::server::YamlVersion;

mod array_constraints;
mod composition;
mod context;
mod formats;
mod mapping_constraints;
mod scalar_constraints;
mod support;
mod type_validation;

use context::Ctx;
use support::{
    MAX_ENUM_DISPLAY, MAX_VALIDATION_DEPTH, format_path, make_diagnostic, node_loc, yaml_to_json,
};

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Validate `docs` (parsed YAML ASTs) against `schema` and return diagnostics.
///
/// Each element of `docs` is one YAML document (separated by `---`).
/// `format_validation` controls whether the `format` keyword is validated.
/// `yaml_version` is used to suppress YAML 1.1 compatibility warnings when the
/// user has explicitly opted into 1.1 semantics.
#[must_use]
pub fn validate_schema(
    docs: &[Document<Span>],
    schema: &JsonSchema,
    format_validation: bool,
    yaml_version: YamlVersion,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for doc in docs {
        let mut ctx = Ctx::new(
            &mut diagnostics,
            format_validation,
            yaml_version,
            doc.line_index(),
        );
        validate_node(&doc.root, schema, &[], &mut ctx, 0);
    }

    diagnostics
}

// ──────────────────────────────────────────────────────────────────────────────
// Core recursive validation
// ──────────────────────────────────────────────────────────────────────────────

/// Recursively validate a YAML node against a schema.
///
/// `path` is the property path to the current node (for diagnostic messages).
/// `depth` guards against stack overflow on deeply nested structures.
fn validate_node(
    node: &Node<Span>,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    if depth > MAX_VALIDATION_DEPTH {
        return;
    }

    // Type check — returns false when a type mismatch error was emitted and
    // further validation of this node should be skipped.
    if !type_validation::validate_type(node, schema, path, ctx) {
        return;
    }

    // Enum check
    if let Some(enum_values) = &schema.enum_values
        && let Some(yaml_val) = yaml_to_json(node)
        && !enum_values.contains(&yaml_val)
    {
        let range = span_to_lsp(node_loc(node), ctx.idx);
        let listed: Vec<String> = enum_values
            .iter()
            .take(MAX_ENUM_DISPLAY)
            .map(ToString::to_string)
            .collect();
        let valid = if enum_values.len() > MAX_ENUM_DISPLAY {
            format!(
                "{}, ... and {} more",
                listed.join(", "),
                enum_values.len() - MAX_ENUM_DISPLAY
            )
        } else {
            listed.join(", ")
        };
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaEnum",
            format!("Value at {} must be one of: {}", format_path(path), valid),
        ));
    }

    // Scalar constraints
    scalar_constraints::validate_scalar_constraints(node, schema, path, ctx);

    // Mapping-specific checks
    if let Node::Mapping { entries, loc, .. } = node {
        mapping_constraints::validate_mapping(entries, *loc, schema, path, ctx, depth);
    }

    // Sequence-specific checks
    if let Node::Sequence { items, loc, .. } = node {
        array_constraints::validate_sequence(items, *loc, schema, path, ctx, depth);
    }

    // Composition
    composition::validate_composition(node, schema, path, ctx, depth);

    // unevaluatedProperties (Draft 2019-09)
    if schema.unevaluated_properties.is_some() {
        if let Node::Mapping { entries, .. } = node {
            mapping_constraints::validate_unevaluated_properties(entries, schema, path, ctx, depth);
        }
    }

    // unevaluatedItems (Draft 2019-09)
    if schema.unevaluated_items.is_some() {
        if let Node::Sequence { items, .. } = node {
            array_constraints::validate_unevaluated_items(items, schema, path, ctx, depth);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{NumberOrString, Position};

    use super::*;
    use crate::schema::{AdditionalProperties, JsonSchema, SchemaType, parse_schema};
    use crate::test_utils::parse_docs;
    use serde_json::json;

    use super::support::test_fixtures::{
        boolean_schema, code_of, integer_schema, object_schema_with_props, string_schema,
    };

    // ══════════════════════════════════════════════════════════════════════════
    // Nested validation
    // ══════════════════════════════════════════════════════════════════════════

    // Test 36
    #[test]
    fn should_validate_properties_recursively() {
        let server_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("port".to_string(), integer_schema())].into()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("server", server_schema)]);
        let text = "server:\n  port: not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 39
    #[test]
    fn should_validate_deeply_nested_schema_five_levels() {
        // Build 5 levels: root → a → b → c → d → {type: string}
        let leaf = string_schema();
        let l4 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("d".to_string(), leaf)].into()),
            ..JsonSchema::default()
        };
        let l3 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("c".to_string(), l4)].into()),
            ..JsonSchema::default()
        };
        let l2 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("b".to_string(), l3)].into()),
            ..JsonSchema::default()
        };
        let l1 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("a".to_string(), l2)].into()),
            ..JsonSchema::default()
        };
        let text = "a:\n  b:\n    c:\n      d: hello";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &l1, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 40
    #[test]
    fn should_not_stack_overflow_on_deep_nesting() {
        // Build 25 levels of nested object schemas programmatically
        let mut schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        };
        for _ in 0..25 {
            schema = JsonSchema {
                schema_type: Some(SchemaType::Single("object".to_string())),
                properties: Some([("x".to_string(), schema)].into()),
                ..JsonSchema::default()
            };
        }
        // Build matching YAML: x:\n  x:\n  ...
        let text = "x:\n".repeat(25) + "  value: leaf";
        let docs = parse_docs(&text);
        // Must complete without panic
        let _ = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Diagnostic format
    // ══════════════════════════════════════════════════════════════════════════

    // Test 41
    #[test]
    fn should_set_source_to_rlsp_yaml() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        assert!(
            result
                .iter()
                .all(|d| d.source == Some("rlsp-yaml".to_string()))
        );
    }

    // Tests 42–44: correct diagnostic code for each violation type
    #[rstest]
    #[case::required_violation(
        JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        },
        "age: 30",
        "schemaRequired"
    )]
    #[case::type_violation(
        object_schema_with_props(vec![("count", integer_schema())]),
        "count: hello",
        "schemaType"
    )]
    #[case::enum_violation(
        object_schema_with_props(vec![("env", JsonSchema {
            enum_values: Some(vec![json!("prod"), json!("staging")]),
            ..JsonSchema::default()
        })]),
        "env: testing",
        "schemaEnum"
    )]
    fn violation_produces_correct_code(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String(expected_code.to_string()))
        );
    }

    // Test 45
    #[test]
    fn should_set_correct_code_for_additional_property_violation() {
        let schema = JsonSchema {
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice\nextra: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        let ap_diags: Vec<_> = result
            .iter()
            .filter(|d| code_of(d) == "schemaAdditionalProperty")
            .collect();
        assert!(!ap_diags.is_empty());
        assert_eq!(
            ap_diags[0].code,
            Some(NumberOrString::String(
                "schemaAdditionalProperty".to_string()
            ))
        );
    }

    // Tests 46–48: ERROR severity for core violation types
    #[rstest]
    #[case::required_violation(
        JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        },
        "age: 30"
    )]
    #[case::type_violation(
        object_schema_with_props(vec![("count", integer_schema())]),
        "count: hello"
    )]
    #[case::enum_violation(
        object_schema_with_props(vec![("env", JsonSchema {
            enum_values: Some(vec![json!("prod")]),
            ..JsonSchema::default()
        })]),
        "env: testing"
    )]
    fn violation_produces_error_severity(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 49
    #[test]
    fn should_set_warning_severity_for_additional_property_violation() {
        let schema = JsonSchema {
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice\nextra: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        let ap = result
            .iter()
            .find(|d| code_of(d) == "schemaAdditionalProperty")
            .expect("should have additionalProperty diagnostic");
        assert_eq!(ap.severity, Some(DiagnosticSeverity::WARNING));
    }

    // Test 50
    #[test]
    fn should_include_property_path_in_required_diagnostic_message() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  other: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("spec"),
            "message should reference parent path 'spec', got: {msg}"
        );
    }

    // Test 51
    #[test]
    fn should_include_valid_values_in_enum_diagnostic_message() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: testing");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(msg.contains("prod"), "message should contain 'prod'");
        assert!(msg.contains("staging"), "message should contain 'staging'");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Edge cases
    // ══════════════════════════════════════════════════════════════════════════

    // Test 52
    #[test]
    fn should_return_empty_for_empty_yaml_document() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 53
    #[test]
    fn should_return_empty_when_docs_is_empty() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let result = validate_schema(&[], &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 54
    #[test]
    fn should_return_empty_for_schema_with_no_constraints() {
        let schema = JsonSchema::default();
        let docs = parse_docs("anything: value\nnested:\n  key: 123");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 55
    #[test]
    fn should_return_empty_for_yaml_with_parse_errors() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        // Simulate parse failure by passing empty docs slice
        let result = validate_schema(&[], &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 56
    #[test]
    fn should_validate_each_document_in_multi_document_yaml() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let text = "name: Alice\n---\nage: 30";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // Second document is missing "name"
        assert_eq!(
            result
                .iter()
                .filter(|d| code_of(d) == "schemaRequired")
                .count(),
            1
        );
    }

    // Test 57
    #[test]
    fn should_produce_no_diagnostics_for_unknown_property_when_no_properties_in_schema() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        let docs = parse_docs("anything: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Recursion / Depth Guard
    // ══════════════════════════════════════════════════════════════════════════

    // Test 58
    #[test]
    fn should_complete_without_panic_for_deeply_nested_yaml_and_schema() {
        // Build 100 levels of nested object schemas
        let mut schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        for _ in 0..100 {
            schema = JsonSchema {
                schema_type: Some(SchemaType::Single("object".to_string())),
                properties: Some([("child".to_string(), schema)].into()),
                ..JsonSchema::default()
            };
        }
        // Build matching YAML with 100 levels of nesting
        let mut text = String::new();
        for i in 0..100 {
            for _ in 0..i {
                text.push_str("  ");
            }
            text.push_str("child:\n");
        }
        let docs = parse_docs(&text);
        // Must return without panicking — result content doesn't matter
        let _result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
    }

    // Test 59
    #[test]
    fn should_not_recurse_past_depth_limit() {
        // Build 70 levels of nested properties with a string type leaf
        let mut schema = string_schema();
        for _ in 0..70 {
            schema = JsonSchema {
                schema_type: Some(SchemaType::Single("object".to_string())),
                properties: Some([("child".to_string(), schema)].into()),
                ..JsonSchema::default()
            };
        }
        // Build YAML where the leaf value is an integer (type mismatch)
        let mut text = String::new();
        for i in 0..70 {
            for _ in 0..i {
                text.push_str("  ");
            }
            text.push_str("child:\n");
        }
        for _ in 0..70 {
            text.push_str("  ");
        }
        text.push_str("42\n");
        let docs = parse_docs(&text);
        // Must return without panicking — depth guard may suppress leaf-level error
        let _result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Composition Branch Limits
    // ══════════════════════════════════════════════════════════════════════════

    // Test 60
    #[test]
    fn should_complete_in_bounded_time_for_one_of_with_many_alternatives() {
        // 50 branches, each requiring a unique field not present in the YAML
        let branches: Vec<JsonSchema> = (0..50)
            .map(|i| JsonSchema {
                required: Some(vec![format!("field_{i}")]),
                ..JsonSchema::default()
            })
            .collect();
        let schema = JsonSchema {
            one_of: Some(branches),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        // Must return (not hang) — result content doesn't matter
        let _result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
    }

    // Test 61
    #[test]
    fn should_complete_for_all_of_with_many_branches() {
        // 25 branches, each requiring a unique field
        let branches: Vec<JsonSchema> = (0..25)
            .map(|i| JsonSchema {
                required: Some(vec![format!("field_{i}")]),
                ..JsonSchema::default()
            })
            .collect();
        let schema = JsonSchema {
            all_of: Some(branches),
            ..JsonSchema::default()
        };
        // Only field_0 is present — all other branches unsatisfied
        let docs = parse_docs("field_0: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // Non-empty: at least one diagnostic for unsatisfied branches
        assert!(!result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Diagnostic Message Truncation
    // ══════════════════════════════════════════════════════════════════════════

    // Test 62
    #[test]
    fn should_truncate_long_description_in_diagnostic_message() {
        // The implementation does not embed property descriptions in violation
        // messages (descriptions appear in hover, not diagnostics). This test
        // verifies that a required-property violation message stays within a
        // reasonable length even when the schema has a very long description.
        let long_desc = "x".repeat(1000);
        let prop_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            description: Some(long_desc),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            properties: Some([("name".to_string(), prop_schema)].into()),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        // Message must not contain the full 1000-char description
        for d in &result {
            assert!(
                d.message.len() <= 300,
                "diagnostic message too long: {} chars",
                d.message.len()
            );
        }
    }

    // Test 63
    #[test]
    fn should_truncate_long_enum_value_list_in_diagnostic_message() {
        // 50 enum values — the message must not list all 50 verbatim
        let enum_values: Vec<serde_json::Value> =
            (0..50).map(|i| json!(format!("opt{i}"))).collect();
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(enum_values),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: invalid");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        assert_eq!(code_of(&result[0]), "schemaEnum");
        // Message must be bounded — 50 values * ~6 chars each would be ~300+
        assert!(
            result[0].message.len() <= 500,
            "enum diagnostic message too long: {} chars",
            result[0].message.len()
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Mutex / Async Safety (Structural)
    // ══════════════════════════════════════════════════════════════════════════

    // Mutex-across-await safety is enforced at compile time by
    // clippy::await_holding_lock (pedantic group, deny-on-warnings).

    // Test 66
    #[test]
    fn should_include_expected_properties_in_required_diagnostic_message() {
        let schema = JsonSchema {
            required: Some(vec![
                "name".to_string(),
                "age".to_string(),
                "email".to_string(),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("Expected:"),
            "message should contain 'Expected:', got: {msg}"
        );
        assert!(
            msg.contains("name"),
            "message should contain 'name', got: {msg}"
        );
        assert!(
            msg.contains("age"),
            "message should contain 'age', got: {msg}"
        );
        assert!(
            msg.contains("email"),
            "message should contain 'email', got: {msg}"
        );
    }

    // Test 67
    #[test]
    fn should_truncate_expected_properties_list_when_more_than_max() {
        let schema = JsonSchema {
            required: Some(vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
                "delta".to_string(),
                "epsilon".to_string(),
                "zeta".to_string(),
                "eta".to_string(),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("(7 total)"),
            "message should contain total count, got: {msg}"
        );
        assert!(
            msg.contains("..."),
            "message should contain ellipsis for truncation, got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // not keyword
    // ══════════════════════════════════════════════════════════════════════════

    // Test 93
    #[test]
    fn should_produce_error_when_value_matches_not_schema() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaNot");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 94
    #[test]
    fn should_produce_no_diagnostics_when_value_does_not_match_not_schema() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 95
    #[test]
    fn should_reject_string_when_not_type_string() {
        let schema = JsonSchema {
            not: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let docs = parse_docs("hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaNot");
    }

    // Test 96
    #[test]
    fn should_allow_integer_when_not_type_string() {
        let schema = JsonSchema {
            not: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let docs = parse_docs("42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 97
    #[test]
    fn should_produce_error_when_value_matches_not_enum() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    enum_values: Some(vec![json!("prod"), json!("staging")]),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: prod");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaNot");
    }

    // Test 98
    #[test]
    fn should_produce_no_diagnostics_when_value_outside_not_enum() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    enum_values: Some(vec![json!("prod"), json!("staging")]),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: dev");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // patternProperties
    // ══════════════════════════════════════════════════════════════════════════

    // Test 99
    #[test]
    fn should_validate_value_against_pattern_properties_schema() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        // str_name should be validated as string — integer is a type violation
        let text = "str_name: 42";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 100
    #[test]
    fn should_produce_no_diagnostics_when_pattern_property_value_is_valid() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        let text = "str_name: hello";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 101
    #[test]
    fn should_not_trigger_additional_properties_for_key_matched_by_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let text = "str_name: hello";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // str_name matches the pattern — no additionalProperty diagnostic
        assert!(
            result
                .iter()
                .all(|d| code_of(d) != "schemaAdditionalProperty")
        );
    }

    // Test 102
    #[test]
    fn should_trigger_additional_properties_for_key_not_matched_by_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        // "other" doesn't match "^str_" and there are no properties
        let text = "other: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaAdditionalProperty");
    }

    // Test 103
    #[test]
    fn should_prefer_properties_over_pattern_properties_for_known_key() {
        // "name" is in properties (integer), and also matches pattern (string).
        // properties takes precedence — integer schema applies.
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), integer_schema())].into()),
            pattern_properties: Some(vec![("^name$".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        // "name" is an integer — valid against properties schema (integer), but
        // patternProperties schema (string) is not applied for known properties.
        let text = "name: 42";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 104
    #[test]
    fn should_match_key_against_multiple_patterns_and_validate_all() {
        // "x_num" matches both "^x_" (string) and ".*num.*" (integer).
        // Both schemas validate the value — integer value fails string check.
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![
                ("^x_".to_string(), string_schema()),
                ("num".to_string(), integer_schema()),
            ]),
            ..JsonSchema::default()
        };
        // value is integer — fails string pattern, passes integer pattern
        let text = "x_num: 42";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // One type violation from the string pattern
        assert_eq!(
            result.iter().filter(|d| code_of(d) == "schemaType").count(),
            1
        );
    }

    // Test 105
    #[test]
    fn should_emit_warning_for_over_length_pattern_and_fall_through_to_additional_properties() {
        let long_pattern = "a".repeat(1025);
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![(long_pattern, string_schema())]),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        // The pattern is too long — emits a PatternLimit warning.
        // "key" also falls through to additionalProperties (not matched by any pattern).
        let text = "key: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.iter().any(|d| code_of(d) == "schemaPatternLimit"
            && d.severity == Some(DiagnosticSeverity::WARNING)));
        assert!(
            result
                .iter()
                .any(|d| code_of(d) == "schemaAdditionalProperty")
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Array constraints — minItems / maxItems / uniqueItems
    // ══════════════════════════════════════════════════════════════════════════

    fn array_schema(min: Option<u64>, max: Option<u64>, unique: Option<bool>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            min_items: min,
            max_items: max,
            unique_items: unique,
            ..JsonSchema::default()
        }
    }

    // Tests 106, 108, 110: array constraint violation → error with specific code
    #[rstest]
    #[case::fewer_than_min_items(
        object_schema_with_props(vec![("tags", array_schema(Some(2), None, None))]),
        "tags:\n  - a",
        "schemaMinItems"
    )]
    #[case::exceeds_max_items(
        object_schema_with_props(vec![("tags", array_schema(None, Some(2), None))]),
        "tags:\n  - a\n  - b\n  - c",
        "schemaMaxItems"
    )]
    #[case::duplicate_items_when_unique_required(
        object_schema_with_props(vec![("tags", array_schema(None, None, Some(true)))]),
        "tags:\n  - foo\n  - bar\n  - foo",
        "schemaUniqueItems"
    )]
    fn array_constraint_violated_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Tests 107, 109, 111, 112: array constraint satisfied → no diagnostics
    #[rstest]
    #[case::meets_min_items(
        object_schema_with_props(vec![("tags", array_schema(Some(2), None, None))]),
        "tags:\n  - a\n  - b"
    )]
    #[case::meets_max_items(
        object_schema_with_props(vec![("tags", array_schema(None, Some(2), None))]),
        "tags:\n  - a\n  - b"
    )]
    #[case::all_unique_with_unique_items_true(
        object_schema_with_props(vec![("tags", array_schema(None, None, Some(true)))]),
        "tags:\n  - foo\n  - bar\n  - baz"
    )]
    #[case::duplicates_allowed_when_unique_items_false(
        object_schema_with_props(vec![("tags", array_schema(None, None, Some(false)))]),
        "tags:\n  - foo\n  - foo"
    )]
    fn array_constraint_satisfied_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Regex security hardening
    // ══════════════════════════════════════════════════════════════════════════

    // Test 113
    #[test]
    fn should_emit_warning_when_pattern_limit_exceeded_in_pattern_properties() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("[invalid".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        // Invalid regex in patternProperties — warning emitted
        let text = "key: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.iter().any(|d| code_of(d) == "schemaPatternLimit"
            && d.severity == Some(DiagnosticSeverity::WARNING)));
    }

    // Test 114
    #[test]
    fn should_still_match_valid_string_against_pattern_after_hardening() {
        // Regression: hardening must not break valid pattern matching
        let schema = object_schema_with_props(vec![(
            "code",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                pattern: Some("^[A-Z]{3}$".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("code: abc");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPattern");
    }

    // Test 115
    #[test]
    fn should_still_match_valid_pattern_property_after_hardening() {
        // Regression: hardening must not break valid patternProperties matching
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        let text = "str_name: 42";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // propertyNames
    // ══════════════════════════════════════════════════════════════════════════

    // Test 116
    #[test]
    fn should_produce_no_diagnostics_when_all_keys_match_property_names_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z_]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "foo: 1\nbar_baz: 2";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 117
    #[test]
    fn should_produce_diagnostic_when_key_violates_property_names_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z_]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        // "BadKey" contains uppercase — violates pattern
        let text = "BadKey: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPattern");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 118
    #[test]
    fn should_produce_diagnostic_when_key_violates_property_names_min_length() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                min_length: Some(3),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "ab: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinLength");
    }

    // Test 119
    #[test]
    fn should_produce_diagnostic_when_key_not_in_property_names_enum() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                enum_values: Some(vec![json!("foo"), json!("bar")]),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "baz: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaEnum");
    }

    // Test 120
    #[test]
    fn should_apply_property_names_to_all_keys_regardless_of_properties() {
        // "name" is in properties, "extra" is not — propertyNames applies to both
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        // Both keys are lowercase — no violations
        let text = "name: Alice\nextra: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 121
    #[test]
    fn should_produce_diagnostics_for_all_violating_keys_with_property_names() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "UPPER: 1\nAlso_Bad: 2\ngood: 3";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(
            result
                .iter()
                .filter(|d| code_of(d) == "schemaPattern")
                .count(),
            2
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // dependentRequired / dependentSchemas
    // ══════════════════════════════════════════════════════════════════════════

    // Test 122
    #[test]
    fn should_produce_error_when_trigger_present_and_dependent_required_missing() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_required: Some(
                [(
                    "credit_card".to_string(),
                    vec!["billing_address".to_string()],
                )]
                .into(),
            ),
            ..JsonSchema::default()
        };
        // credit_card present but billing_address absent
        let text = "credit_card: 1234";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaDependentRequired");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("billing_address"));
        assert!(result[0].message.contains("credit_card"));
    }

    // Test 123
    #[test]
    fn should_produce_no_diagnostics_when_trigger_and_dependency_both_present() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_required: Some(
                [(
                    "credit_card".to_string(),
                    vec!["billing_address".to_string()],
                )]
                .into(),
            ),
            ..JsonSchema::default()
        };
        let text = "credit_card: 1234\nbilling_address: 123 Main St";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 124
    #[test]
    fn should_produce_no_diagnostics_when_trigger_absent_in_dependent_required() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_required: Some(
                [(
                    "credit_card".to_string(),
                    vec!["billing_address".to_string()],
                )]
                .into(),
            ),
            ..JsonSchema::default()
        };
        // trigger absent — no check performed
        let text = "name: Alice";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 125
    #[test]
    fn should_produce_diagnostic_when_trigger_present_and_dependent_schema_fails() {
        // When "name" is present, the mapping must also have "age" (required by dep schema)
        let dep_schema = JsonSchema {
            required: Some(vec!["age".to_string()]),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_schemas: Some([("name".to_string(), dep_schema)].into()),
            ..JsonSchema::default()
        };
        let text = "name: Alice";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        assert_eq!(code_of(&result[0]), "schemaRequired");
    }

    // Test 126
    #[test]
    fn should_produce_no_diagnostics_when_dependent_schema_passes() {
        let dep_schema = JsonSchema {
            required: Some(vec!["age".to_string()]),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_schemas: Some([("name".to_string(), dep_schema)].into()),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nage: 30";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 127
    #[test]
    fn should_produce_no_diagnostics_when_dependent_schema_trigger_absent() {
        let dep_schema = JsonSchema {
            required: Some(vec!["age".to_string()]),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_schemas: Some([("name".to_string(), dep_schema)].into()),
            ..JsonSchema::default()
        };
        // trigger "name" absent — dep schema not checked
        let text = "other: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // if / then / else
    // ══════════════════════════════════════════════════════════════════════════

    // Test 128
    #[test]
    fn should_apply_then_and_pass_when_if_matches_and_then_passes() {
        // if: type string → then: minLength 3
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                then_schema: Some(Box::new(JsonSchema {
                    min_length: Some(3),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is a string of length 5 — if matches, then passes
        let docs = parse_docs("val: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 129
    #[test]
    fn should_apply_then_and_fail_when_if_matches_and_then_fails() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                then_schema: Some(Box::new(JsonSchema {
                    min_length: Some(10),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is a short string — if matches, then fails
        let docs = parse_docs("val: hi");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinLength");
    }

    // Test 130
    #[test]
    fn should_apply_else_and_pass_when_if_does_not_match_and_else_passes() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    minimum: Some(0.0),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is integer 5 — if doesn't match (not string), else passes (>= 0)
        let docs = parse_docs("val: 5");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 131
    #[test]
    fn should_apply_else_and_fail_when_if_does_not_match_and_else_fails() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    minimum: Some(10.0),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is integer 3 — if doesn't match, else fails (< 10)
        let docs = parse_docs("val: 3");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinimum");
    }

    // Test 132
    #[test]
    fn should_produce_no_diagnostics_when_if_matches_but_no_then() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    minimum: Some(0.0),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is string — if matches, no then → no diagnostic
        let docs = parse_docs("val: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 133
    #[test]
    fn should_produce_no_diagnostics_when_if_does_not_match_and_no_else() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                then_schema: Some(Box::new(JsonSchema {
                    min_length: Some(10),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is integer — if doesn't match, no else → no diagnostic
        let docs = parse_docs("val: 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 134
    #[test]
    fn should_ignore_then_and_else_when_no_if() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                then_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("integer".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("integer".to_string())),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // Without if, then/else are ignored — string value produces no diagnostic
        let docs = parse_docs("val: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ── contains / minContains / maxContains ────────────────────────────────

    fn contains_schema(min_contains: Option<u64>, max_contains: Option<u64>) -> JsonSchema {
        JsonSchema {
            contains: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                ..JsonSchema::default()
            })),
            min_contains,
            max_contains,
            ..JsonSchema::default()
        }
    }

    // Test 135
    #[test]
    fn should_produce_no_diagnostics_when_array_has_one_matching_item_no_min_max() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, None))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 136
    #[test]
    fn should_produce_diagnostic_when_no_items_match_contains_schema() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, None))]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at least 1"));
    }

    // Test 137
    #[test]
    fn should_produce_diagnostic_when_min_contains_not_met() {
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(2), None))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at least 2"));
    }

    // Test 138
    #[test]
    fn should_produce_no_diagnostics_when_min_contains_met() {
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(2), None))]);
        let docs = parse_docs("items:\n  - 1\n  - 2");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 139
    #[test]
    fn should_produce_diagnostic_when_max_contains_exceeded() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, Some(1)))]);
        let docs = parse_docs("items:\n  - 1\n  - 2");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at most 1"));
    }

    // Test 140
    #[test]
    fn should_produce_no_diagnostics_when_max_contains_not_exceeded() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, Some(1)))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 141
    #[test]
    fn should_produce_no_diagnostics_when_min_contains_zero() {
        // minContains: 0 disables the "at least one" requirement
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(0), None))]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 142
    #[test]
    fn should_ignore_min_contains_and_max_contains_when_contains_absent() {
        // minContains/maxContains without contains are ignored per spec
        let schema = object_schema_with_props(vec![(
            "items",
            JsonSchema {
                min_contains: Some(5),
                max_contains: Some(0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ── prefixItems ─────────────────────────────────────────────────────────

    fn tuple_schema(prefix: Vec<JsonSchema>, items: Option<JsonSchema>) -> JsonSchema {
        JsonSchema {
            prefix_items: Some(prefix),
            items: items.map(Box::new),
            ..JsonSchema::default()
        }
    }

    // Test 143
    #[test]
    fn should_produce_diagnostic_when_second_item_fails_prefix_schema() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema(), integer_schema()], None),
        )]);
        // [0] is a string (ok), [1] is a string but expected integer (fail)
        let docs = parse_docs("arr:\n  - hello\n  - world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 144
    #[test]
    fn should_produce_no_diagnostics_when_all_items_match_prefix_schemas() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema(), integer_schema()], None),
        )]);
        let docs = parse_docs("arr:\n  - hello\n  - 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 145
    #[test]
    fn should_validate_extra_items_against_items_schema_when_prefix_items_set() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema()], Some(integer_schema())),
        )]);
        // [0] string ok, [1] integer ok (matches items schema)
        let docs = parse_docs("arr:\n  - hello\n  - 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 146
    #[test]
    fn should_produce_diagnostic_when_extra_item_fails_items_schema() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema()], Some(integer_schema())),
        )]);
        // [0] string ok, [1] string fails items schema (expected integer)
        let docs = parse_docs("arr:\n  - hello\n  - world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 147
    #[test]
    fn should_produce_no_diagnostics_when_array_shorter_than_prefix_items() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(
                vec![string_schema(), integer_schema(), string_schema()],
                None,
            ),
        )]);
        // Only one item — only [0] is validated, [1] and [2] positions absent
        let docs = parse_docs("arr:\n  - hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 148 — Draft-04 array-form items parsed as prefixItems
    #[test]
    fn should_parse_draft04_array_items_as_prefix_items() {
        let raw = json!({
            "type": "object",
            "properties": {
                "arr": {
                    "type": "array",
                    "items": [
                        { "type": "string" },
                        { "type": "integer" }
                    ]
                }
            }
        });
        let schema = parse_schema(&raw).expect("valid schema");
        let arr_schema = schema
            .properties
            .as_ref()
            .and_then(|p| p.get("arr"))
            .expect("arr property");
        assert!(arr_schema.prefix_items.is_some());
        assert_eq!(arr_schema.prefix_items.as_ref().unwrap().len(), 2);
        assert!(arr_schema.items.is_none());
    }

    // Test 149 — prefixItems takes precedence over array-form items
    #[test]
    fn should_prefer_prefix_items_over_draft04_array_items() {
        let raw = json!({
            "prefixItems": [{ "type": "string" }],
            "items": [{ "type": "integer" }, { "type": "boolean" }]
        });
        let schema = parse_schema(&raw).expect("valid schema");
        // prefixItems was set first — array-form items is ignored
        assert!(schema.prefix_items.is_some());
        assert_eq!(schema.prefix_items.as_ref().unwrap().len(), 1);
    }

    // ── unevaluatedProperties / unevaluatedItems ─────────────────────────────

    // Test 150 — unevaluatedProperties: false with allOf — properties from allOf evaluated (pass)
    #[test]
    fn should_produce_no_diagnostics_when_allof_evaluates_all_properties() {
        let schema = JsonSchema {
            all_of: Some(vec![object_schema_with_props(vec![(
                "name",
                string_schema(),
            )])]),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 151 — unevaluatedProperties: false — property not in any sub-schema (diagnostic)
    #[test]
    fn should_produce_diagnostic_for_unevaluated_property() {
        let schema = JsonSchema {
            properties: Some(
                vec![("name".to_string(), string_schema())]
                    .into_iter()
                    .collect(),
            ),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("extra"));
    }

    // Test 152 — unevaluatedProperties with sub-schema — unevaluated validated against it
    #[test]
    fn should_validate_unevaluated_property_against_schema() {
        let schema = JsonSchema {
            properties: Some(
                vec![("name".to_string(), string_schema())]
                    .into_iter()
                    .collect(),
            ),
            unevaluated_properties: Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
            ..JsonSchema::default()
        };
        // "extra" is unevaluated and not an integer — diagnostic
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 153 — unevaluatedItems with prefixItems — prefix items are evaluated (pass)
    #[test]
    fn should_produce_no_diagnostics_when_prefix_items_cover_all_items() {
        let schema = JsonSchema {
            prefix_items: Some(vec![string_schema(), integer_schema()]),
            unevaluated_items: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("boolean".to_string())),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let docs = parse_docs("- hello\n- 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 154 — unevaluatedItems — item beyond prefix not evaluated (diagnostic)
    #[test]
    fn should_produce_diagnostic_for_unevaluated_item_beyond_prefix() {
        let schema = JsonSchema {
            prefix_items: Some(vec![string_schema()]),
            unevaluated_items: Some(Box::new(integer_schema())),
            ..JsonSchema::default()
        };
        // [0] is string (evaluated by prefix), [1] is string (unevaluated, fails integer)
        let docs = parse_docs("- hello\n- world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 155 — unevaluatedProperties with if/then — then branch properties evaluated
    #[test]
    fn should_evaluate_properties_from_then_branch() {
        let schema = JsonSchema {
            if_schema: Some(Box::new(JsonSchema {
                required: Some(vec!["name".to_string()]),
                ..JsonSchema::default()
            })),
            then_schema: Some(Box::new(object_schema_with_props(vec![(
                "extra",
                string_schema(),
            )]))),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        // "extra" is in then_schema — evaluated, no diagnostic
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // "name" is not in then properties — it's unevaluated → diagnostic
        // "extra" IS in then properties → no diagnostic for extra
        assert!(
            result.iter().all(|d| !d.message.contains("extra")),
            "extra should be evaluated by then"
        );
    }

    // Test 156 — no unevaluated keywords — existing behavior unchanged (regression)
    #[test]
    fn should_not_change_behavior_when_no_unevaluated_keywords() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // Without unevaluated keywords, extra property is allowed
        assert!(result.is_empty());
    }

    fn content_schema(encoding: Option<&str>, media_type: Option<&str>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            content_encoding: encoding.map(str::to_string),
            content_media_type: media_type.map(str::to_string),
            ..JsonSchema::default()
        }
    }

    fn run_content(
        text: &str,
        encoding: Option<&str>,
        media_type: Option<&str>,
    ) -> Vec<Diagnostic> {
        let schema = content_schema(encoding, media_type);
        let docs = parse_docs(text);
        validate_schema(&docs, &schema, true, YamlVersion::V1_2)
    }

    // Tests 187, 189, 191, 193: valid contentEncoding → no diagnostics
    #[rstest]
    #[case::base64_valid("aGVsbG8=", "base64")]
    #[case::base64_empty_valid("", "base64")]
    #[case::base64url_valid("aGVsbG8=", "base64url")]
    #[case::base32_valid("NBSWY3DPEB3W64TMMQ======", "base32")]
    #[case::base16_uppercase_valid("48656C6C6F", "base16")]
    #[case::base16_lowercase_valid("48656c6c6f", "base16")]
    fn content_encoding_valid_produces_no_diagnostics(#[case] value: &str, #[case] encoding: &str) {
        assert!(run_content(value, Some(encoding), None).is_empty());
    }

    // Tests 188, 190, 192, 194: invalid contentEncoding → one diagnostic
    #[rstest]
    #[case::base64_invalid("not-valid-base64!!!", "base64")]
    #[case::base64url_invalid("not+valid/base64url!!!", "base64url")]
    #[case::base32_invalid("not-valid-base32!!!", "base32")]
    #[case::base16_invalid("ZZZZ", "base16")]
    fn content_encoding_invalid_produces_error(#[case] value: &str, #[case] encoding: &str) {
        assert_eq!(run_content(value, Some(encoding), None).len(), 1);
    }

    // Test 195 — contentEncoding unknown: silently ignored
    #[test]
    fn content_encoding_unknown_ignored() {
        assert!(run_content("anything", Some("base58"), None).is_empty());
    }

    // Test 196 — contentMediaType application/json: valid (no encoding)
    // The value must be a YAML string (quoted) so it reaches validate_string_constraints.
    // Values starting with { or [ are YAML flow collections; use quoted YAML.
    #[test]
    fn content_media_type_json_valid_no_encoding() {
        // "\"42\"" parses as YAML string "42", which is valid JSON
        let schema = content_schema(None, Some("application/json"));
        let docs = parse_docs("\"42\"");
        assert!(validate_schema(&docs, &schema, true, YamlVersion::V1_2).is_empty());
    }

    // Test 197 — contentMediaType application/json: invalid (no encoding)
    #[test]
    fn content_media_type_json_invalid_no_encoding() {
        assert_eq!(
            run_content("not json", None, Some("application/json")).len(),
            1
        );
    }

    // Test 198 — contentEncoding + contentMediaType: valid base64-encoded JSON
    #[test]
    fn content_encoding_and_media_type_valid() {
        // base64("{"key":"value"}") = "eyJrZXkiOiJ2YWx1ZSJ9"
        assert!(
            run_content(
                "eyJrZXkiOiJ2YWx1ZSJ9",
                Some("base64"),
                Some("application/json")
            )
            .is_empty()
        );
    }

    // Test 199 — contentEncoding + contentMediaType: encoding fails → only encoding diagnostic
    #[test]
    fn content_encoding_fails_skips_media_type_check() {
        let diags = run_content(
            "not-valid-base64!!!",
            Some("base64"),
            Some("application/json"),
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].code == Some(NumberOrString::String("schemaContentEncoding".to_string())));
    }

    // Test 200 — contentEncoding + contentMediaType: valid encoding but invalid JSON
    #[test]
    fn content_encoding_valid_media_type_invalid() {
        // base64("not json") = "bm90IGpzb24="
        let diags = run_content("bm90IGpzb24=", Some("base64"), Some("application/json"));
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].code == Some(NumberOrString::String("schemaContentMediaType".to_string()))
        );
    }

    // Test 201 — contentMediaType unknown: silently ignored
    #[test]
    fn content_media_type_unknown_ignored() {
        assert!(run_content("anything", None, Some("text/plain")).is_empty());
    }

    // Test 202 — format_validation disabled: content checks also skipped
    #[test]
    fn content_validation_disabled_when_format_validation_off() {
        let schema = content_schema(Some("base64"), Some("application/json"));
        let docs = parse_docs("not-valid-base64!!!");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // contentSchema
    // ══════════════════════════════════════════════════════════════════════════

    fn content_schema_with_sub(
        encoding: Option<&str>,
        media_type: Option<&str>,
        sub_schema: JsonSchema,
    ) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            content_encoding: encoding.map(str::to_string),
            content_media_type: media_type.map(str::to_string),
            content_schema: Some(Box::new(sub_schema)),
            ..JsonSchema::default()
        }
    }

    // contentSchema with base64-encoded JSON that matches the sub-schema
    #[test]
    fn content_schema_base64_json_valid() {
        // base64("42") = "NDI="
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let docs = parse_docs("\"NDI=\"");
        assert!(
            validate_schema(&docs, &schema, true, YamlVersion::V1_2).is_empty(),
            "valid base64-encoded integer should pass contentSchema validation"
        );
    }

    // contentSchema with base64-encoded JSON that fails the sub-schema
    #[test]
    fn content_schema_base64_json_type_mismatch() {
        // base64("\"hello\"") = "ImhlbGxvIg=="
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let docs = parse_docs("\"ImhlbGxvIg==\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaType"),
            "string decoded where integer expected should produce schemaType error: {result:?}"
        );
    }

    // contentSchema without encoding — validate raw string as YAML
    #[test]
    fn content_schema_no_encoding_validates_raw_string() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        // The raw YAML value "42" (as a quoted string) should be parsed as YAML integer
        let docs = parse_docs("\"42\"");
        assert!(
            validate_schema(&docs, &schema, true, YamlVersion::V1_2).is_empty(),
            "raw string '42' should validate as integer against contentSchema"
        );
    }

    // contentSchema without encoding — validation failure
    #[test]
    fn content_schema_no_encoding_type_mismatch() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        let docs = parse_docs("\"hello\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaType"),
            "string 'hello' should fail integer contentSchema: {result:?}"
        );
    }

    // contentSchema with encoding failure — contentSchema not checked
    #[test]
    fn content_schema_skipped_when_encoding_fails() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let docs = parse_docs("\"not-valid-base64!!!\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // Should get encoding error but NOT contentSchema type error
        assert!(
            result.iter().any(|d| code_of(d) == "schemaContentEncoding"),
            "should report encoding error: {result:?}"
        );
        assert!(
            !result.iter().any(|d| code_of(d) == "schemaType"),
            "should NOT check contentSchema when encoding fails: {result:?}"
        );
    }

    // contentSchema with media type failure — contentSchema not checked
    #[test]
    fn content_schema_skipped_when_media_type_fails() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, Some("application/json"), sub);
        let docs = parse_docs("\"not json at all\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result
                .iter()
                .any(|d| code_of(d) == "schemaContentMediaType"),
            "should report media type error: {result:?}"
        );
        assert!(
            !result.iter().any(|d| code_of(d) == "schemaType"),
            "should NOT check contentSchema when media type fails: {result:?}"
        );
    }

    // TE test 5: contentSchema validates embedded YAML mapping via base64
    // (using base64 encoding to avoid the known parser limitation with
    // colon-space inside double-quoted strings)
    #[test]
    fn content_schema_validates_embedded_yaml_mapping() {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            },
        );
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        // base64("name: alice\n") = "bmFtZTogYWxpY2UK"
        let text = "\"bmFtZTogYWxpY2UK\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "embedded YAML mapping should validate: {result:?}"
        );
    }

    // TE test 6: contentSchema embedded mapping with type mismatch via base64
    #[test]
    fn content_schema_validates_embedded_yaml_mapping_invalid() {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            },
        );
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        // base64("name: 42\n") = "bmFtZTogNDIK"
        let text = "\"bmFtZTogNDIK\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            !result.is_empty(),
            "embedded mapping with integer name should fail string check: {result:?}"
        );
    }

    // TE test 7: contentSchema skipped when format_validation is off
    #[test]
    fn content_schema_skipped_when_format_validation_off() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        let docs = parse_docs("\"hello\"");
        // format_validation = false → content keywords not checked
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "contentSchema should not be checked when format_validation is off: {result:?}"
        );
    }

    // TE test 11: all three checks pass (encoding + media type + contentSchema)
    #[test]
    fn content_schema_with_encoding_and_media_type_all_pass() {
        // base64({"key": "value"}) = eyJrZXkiOiAidmFsdWUifQ==
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let text = "\"eyJrZXkiOiAidmFsdWUifQ==\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "all three checks should pass: {result:?}"
        );
    }

    // TE test 13: valid base64 but decoded YAML is unparseable
    #[test]
    fn content_schema_decoded_yaml_invalid() {
        // base64(": bad: [") = OiBiYWQ6IFs=
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        let text = "\"OiBiYWQ6IFs=\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaContentSchema"),
            "unparseable decoded YAML should produce schemaContentSchema: {result:?}"
        );
    }

    // TE test 14: empty content with contentSchema
    #[test]
    fn content_schema_with_empty_decoded_content() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        let text = "\"\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // load("") returns 0 documents — nothing to validate, no diagnostics.
        assert!(
            result.is_empty(),
            "empty content should produce no diagnostics: {result:?}"
        );
    }

    // TE test 15: nested sub-schema runs full validation (via base64
    // to avoid known parser limitation with colon-space in double-quoted strings)
    #[test]
    fn content_schema_nested_sub_schema_uses_full_validation() {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "value".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            },
        );
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        // base64("value: 42\n") = "dmFsdWU6IDQyCg=="
        // Embedded YAML: value is 42 (integer), but schema expects string
        let text = "\"dmFsdWU6IDQyCg==\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            !result.is_empty(),
            "nested schema should catch type mismatch: {result:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Object cardinality — minProperties / maxProperties
    // ══════════════════════════════════════════════════════════════════════════

    fn object_schema_with_cardinality(min: Option<u64>, max: Option<u64>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            min_properties: min,
            max_properties: max,
            ..JsonSchema::default()
        }
    }

    // Tests 203, 205: object property count violation → error with specific code
    #[rstest]
    #[case::fewer_than_min_properties(
        object_schema_with_cardinality(Some(2), None),
        "name: Alice",
        "schemaMinProperties"
    )]
    #[case::exceeds_max_properties(
        object_schema_with_cardinality(None, Some(1)),
        "name: Alice\nage: 30",
        "schemaMaxProperties"
    )]
    fn object_property_count_violated_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Tests 204, 206: object property count within bounds → no diagnostics
    #[rstest]
    #[case::meets_min_properties(
        object_schema_with_cardinality(Some(2), None),
        "name: Alice\nage: 30"
    )]
    #[case::meets_max_properties(
        object_schema_with_cardinality(None, Some(2)),
        "name: Alice\nage: 30"
    )]
    fn object_property_count_satisfied_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // additionalItems (Draft-04/07 tuple arrays)
    // ══════════════════════════════════════════════════════════════════════════

    fn tuple_schema_with_additional_items(
        prefix: Vec<JsonSchema>,
        additional_items: Option<AdditionalProperties>,
    ) -> JsonSchema {
        JsonSchema {
            prefix_items: Some(prefix),
            additional_items,
            ..JsonSchema::default()
        }
    }

    // Test 207
    #[test]
    fn should_produce_warning_for_extra_items_when_additional_items_false() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello\n- extra";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaAdditionalItems");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        let msg = &result[0].message;
        assert!(
            msg.contains("[1]"),
            "message should reference [1], got: {msg}"
        );
    }

    // Test 208
    #[test]
    fn should_produce_no_diagnostics_when_array_exactly_matches_prefix_length_with_additional_items_false()
     {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema(), integer_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello\n- 42";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 209
    #[test]
    fn should_produce_no_diagnostics_for_items_within_prefix_with_additional_items_false() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema(), integer_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 210
    #[test]
    fn should_produce_one_warning_per_extra_item_when_additional_items_false() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello\n- extra1\n- extra2";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|d| code_of(d) == "schemaAdditionalItems"));
    }

    // Test 211
    #[test]
    fn should_validate_extra_items_against_additional_items_schema_when_valid() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
        );
        let text = "- hello\n- 42";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 212
    #[test]
    fn should_produce_type_diagnostic_when_extra_item_fails_additional_items_schema() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
        );
        let text = "- hello\n- world";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 213
    #[test]
    fn should_produce_no_diagnostics_when_additional_items_false_and_prefix_items_set_from_prefix_items_key()
     {
        // Simulates parsed outcome of Draft 2020-12 `prefixItems` + ignored `additionalItems`:
        // additional_items is None because the parser suppresses it for prefixItems keyword.
        let schema = JsonSchema {
            prefix_items: Some(vec![string_schema()]),
            additional_items: None,
            ..JsonSchema::default()
        };
        let text = "- hello\n- extra";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 214
    #[test]
    fn should_produce_no_diagnostics_when_additional_items_absent_and_extra_items_present() {
        let schema = tuple_schema_with_additional_items(vec![string_schema()], None);
        let text = "- hello\n- 42\n- extra";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — type mismatch format
    // ══════════════════════════════════════════════════════════════════════════

    // Test 215
    #[test]
    fn type_mismatch_message_uses_value_at_path_subject() {
        let schema = object_schema_with_props(vec![("replicas", integer_schema())]);
        let text = "replicas: \"hello\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.starts_with("Value at"),
            "message should start with 'Value at', got: {msg}"
        );
        assert!(
            msg.contains("does not match type"),
            "message should contain 'does not match type', got: {msg}"
        );
        assert!(
            msg.contains("integer"),
            "message should contain expected type 'integer', got: {msg}"
        );
        assert!(
            msg.contains("string"),
            "message should contain actual type 'string', got: {msg}"
        );
    }

    // Test 216
    #[test]
    fn type_mismatch_message_includes_property_path() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("replicas".to_string(), integer_schema())].into()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  replicas: not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("spec.replicas"),
            "message should contain nested path 'spec.replicas', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — numeric min/max unified format
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::inclusive_minimum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            minimum: Some(5.0),
            exclusive_minimum_draft04: Some(false),
            ..JsonSchema::default()
        })]),
        "val: 4",
        "is below minimum 5",
        "(inclusive)"
    )]
    #[case::exclusive_minimum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            minimum: Some(5.0),
            exclusive_minimum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "is below exclusive minimum 5",
        "(exclusive)"
    )]
    #[case::inclusive_maximum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            maximum: Some(10.0),
            exclusive_maximum_draft04: Some(false),
            ..JsonSchema::default()
        })]),
        "val: 11",
        "is above maximum 10",
        "(inclusive)"
    )]
    #[case::exclusive_maximum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            maximum: Some(10.0),
            exclusive_maximum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "is above exclusive maximum 10",
        "(exclusive)"
    )]
    #[case::exclusive_minimum_draft06(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_minimum: Some(5.0),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "is below exclusive minimum 5",
        "must be greater than"
    )]
    #[case::exclusive_maximum_draft06(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_maximum: Some(10.0),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "is above exclusive maximum 10",
        "must be less than"
    )]
    fn numeric_bound_message_uses_correct_phrase(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_phrase: &str,
        #[case] excluded_phrase: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains(expected_phrase),
            "message should contain '{expected_phrase}', got: {msg}"
        );
        assert!(
            !msg.contains(excluded_phrase),
            "message should not contain '{excluded_phrase}', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — anyOf branch count
    // ══════════════════════════════════════════════════════════════════════════

    // Test 223
    #[test]
    fn any_of_message_includes_branch_count() {
        let schema = JsonSchema {
            any_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let any_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &any_of_diag.message;
        assert!(
            msg.contains('2'),
            "message should contain branch count '2', got: {msg}"
        );
        assert!(
            msg.contains("(anyOf)"),
            "message should contain '(anyOf)', got: {msg}"
        );
    }

    // Test 224
    #[test]
    fn any_of_message_branch_count_capped_at_max_branch_count() {
        // 25 branches (> MAX_BRANCH_COUNT=20) — message should show 20
        let branches: Vec<JsonSchema> = (0..25)
            .map(|i| JsonSchema {
                required: Some(vec![format!("field_{i}")]),
                ..JsonSchema::default()
            })
            .collect();
        let schema = JsonSchema {
            any_of: Some(branches),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let any_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &any_of_diag.message;
        assert!(
            msg.contains("20"),
            "message should contain capped count '20', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — oneOf branch count
    // ══════════════════════════════════════════════════════════════════════════

    // Test 225
    #[test]
    fn one_of_zero_match_message_includes_branch_count() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let one_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &one_of_diag.message;
        assert!(
            msg.contains('2'),
            "message should contain branch count '2', got: {msg}"
        );
        assert!(
            msg.contains("oneOf schemas"),
            "message should contain 'oneOf schemas', got: {msg}"
        );
    }

    // Test 226
    #[test]
    fn one_of_multi_match_message_includes_passing_count() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                object_schema_with_props(vec![("a", string_schema())]),
            ]),
            ..JsonSchema::default()
        };
        // "a: hello" matches both branches
        let docs = parse_docs("a: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let one_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &one_of_diag.message;
        assert!(
            msg.contains("expected exactly 1"),
            "message should contain 'expected exactly 1', got: {msg}"
        );
    }

    // Test 227
    #[test]
    fn one_of_multi_match_message_includes_total_count() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                object_schema_with_props(vec![("a", string_schema())]),
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        // "a: hello" matches the first two branches (2 of 3)
        let docs = parse_docs("a: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let one_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &one_of_diag.message;
        assert!(
            msg.contains('3'),
            "message should contain total count '3', got: {msg}"
        );
        assert!(
            msg.contains('2'),
            "message should contain passing count '2', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — not schema wording
    // ══════════════════════════════════════════════════════════════════════════

    // Test 228
    #[test]
    fn not_schema_message_references_not_keyword() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("schema defined in 'not'"),
            "message should contain \"schema defined in 'not'\", got: {msg}"
        );
        assert!(
            !msg.contains("excluded schema"),
            "message should not contain old phrasing 'excluded schema', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — required property format
    // ══════════════════════════════════════════════════════════════════════════

    // Test 229
    #[test]
    fn required_property_message_uses_object_at_subject() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("Object at"),
            "message should contain 'Object at', got: {msg}"
        );
        assert!(
            msg.contains("is missing required property"),
            "message should contain 'is missing required property', got: {msg}"
        );
        assert!(
            !msg.contains("Missing required property"),
            "message should not use old phrasing 'Missing required property', got: {msg}"
        );
    }

    // Test 230
    #[test]
    fn required_property_message_uses_expected_label() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string(), "age".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("Expected:"),
            "message should contain 'Expected:', got: {msg}"
        );
        assert!(
            !msg.contains("Expected properties:"),
            "message should not contain old label 'Expected properties:', got: {msg}"
        );
    }

    // Test 231
    #[test]
    fn required_property_message_includes_nested_path() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            required: Some(vec!["replicas".to_string()]),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  other: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("Object at spec"),
            "message should contain 'Object at spec', got: {msg}"
        );
        assert!(
            msg.contains("replicas"),
            "message should contain the missing property name 'replicas', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group A: schemaYaml11Boolean — string-typed field with YAML 1.1 bool value
    // ══════════════════════════════════════════════════════════════════════════

    // A1: warning emitted for each 1.1 bool form in a string-typed field
    #[rstest]
    #[case::yes_lowercase("yes")]
    #[case::yes_titlecase("Yes")]
    #[case::yes_uppercase("YES")]
    #[case::no_lowercase("no")]
    #[case::no_titlecase("No")]
    #[case::no_uppercase("NO")]
    #[case::on_lowercase("on")]
    #[case::on_titlecase("On")]
    #[case::on_uppercase("ON")]
    #[case::off_lowercase("off")]
    #[case::off_titlecase("Off")]
    #[case::off_uppercase("OFF")]
    #[case::y_lowercase("y")]
    #[case::y_uppercase("Y")]
    #[case::n_lowercase("n")]
    #[case::n_uppercase("N")]
    fn schema_yaml11_boolean_warning_for_string_field(#[case] value: &str) {
        let schema = object_schema_with_props(vec![("flag", string_schema())]);
        let text = format!("flag: {value}");
        let docs = parse_docs(&text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(
            result.len(),
            1,
            "expected one diagnostic for '{value}', got: {result:?}"
        );
        assert_eq!(code_of(&result[0]), "schemaYaml11Boolean");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    // A2: warning message contains the value, "true"/"false", and string indication
    #[test]
    fn schema_yaml11_boolean_message_contains_value_and_canonical() {
        let schema = object_schema_with_props(vec![("flag", string_schema())]);
        let docs = parse_docs("flag: yes");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(msg.contains("yes"), "message should contain 'yes': {msg}");
        assert!(
            msg.contains("true"),
            "message should contain 'true' (canonical): {msg}"
        );
        assert!(
            msg.contains("string") || msg.contains("1.2"),
            "message should mention string/1.2: {msg}"
        );
    }

    // A3: no diagnostic for a quoted string that matches a 1.1 bool form
    #[test]
    fn schema_yaml11_boolean_no_warning_for_quoted_scalar() {
        let schema = object_schema_with_props(vec![("flag", string_schema())]);
        let text = "flag: \"yes\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "quoted scalar should not trigger warning: {result:?}"
        );
    }

    // A4: no diagnostic for a plain scalar that is NOT a 1.1 bool
    #[test]
    fn schema_yaml11_boolean_no_warning_for_ordinary_string() {
        let schema = object_schema_with_props(vec![("flag", string_schema())]);
        let text = "flag: hello";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "ordinary string should not trigger warning: {result:?}"
        );
    }

    // A5: suppressed when yaml_version is V1_1
    #[test]
    fn schema_yaml11_boolean_suppressed_in_v1_1_mode() {
        let schema = object_schema_with_props(vec![("flag", string_schema())]);
        let text = "flag: yes";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_1);
        assert!(
            result.is_empty(),
            "1.1 mode should suppress schema yaml11 boolean warning: {result:?}"
        );
    }

    // A6: no diagnostic when field has no matching schema property
    #[test]
    fn schema_yaml11_boolean_no_warning_when_field_not_in_schema() {
        let schema = object_schema_with_props(vec![("other", string_schema())]);
        let text = "flag: yes";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "field without schema should not trigger warning: {result:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group B: schemaYaml11Octal — string-typed field with YAML 1.1 octal value
    // ══════════════════════════════════════════════════════════════════════════

    // B1: warning emitted for representative 1.1 octal values
    #[rstest]
    #[case::mode_0755("0755")]
    #[case::mode_007("007")]
    #[case::mode_01("01")]
    #[case::mode_077("077")]
    fn schema_yaml11_octal_warning_for_string_field(#[case] value: &str) {
        let schema = object_schema_with_props(vec![("mode", string_schema())]);
        let text = format!("mode: {value}");
        let docs = parse_docs(&text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(
            result.len(),
            1,
            "expected one diagnostic for '{value}', got: {result:?}"
        );
        assert_eq!(code_of(&result[0]), "schemaYaml11Octal");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    // B2: warning message contains the value, decimal equivalent, and 0o prefix hint
    #[test]
    fn schema_yaml11_octal_message_contains_value_decimal_and_hint() {
        let schema = object_schema_with_props(vec![("mode", string_schema())]);
        let text = "mode: 0755";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(msg.contains("0755"), "message should contain '0755': {msg}");
        assert!(
            msg.contains("493"),
            "message should contain decimal '493': {msg}"
        );
        assert!(
            msg.contains("0o755"),
            "message should contain '0o755': {msg}"
        );
    }

    // B3: no diagnostic for a quoted octal-looking string
    #[test]
    fn schema_yaml11_octal_no_warning_for_quoted_scalar() {
        let schema = object_schema_with_props(vec![("mode", string_schema())]);
        let text = "mode: \"0755\"";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "quoted scalar should not trigger warning: {result:?}"
        );
    }

    // B4: no diagnostic for non-octal plain scalars
    #[rstest]
    #[case::decimal("42")]
    #[case::zero("0")]
    #[case::yaml12_octal("0o755")]
    fn schema_yaml11_octal_no_warning_for_non_octal(#[case] value: &str) {
        let schema = object_schema_with_props(vec![("mode", string_schema())]);
        let text = format!("mode: {value}");
        let docs = parse_docs(&text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // 42 is an integer (schemaType error); 0 is an integer (schemaType error);
        // 0o755 is an integer (schemaType error) — none should be schemaYaml11Octal
        assert!(
            result.iter().all(|d| code_of(d) != "schemaYaml11Octal"),
            "should not emit schemaYaml11Octal for '{value}': {result:?}"
        );
    }

    // B5: suppressed when yaml_version is V1_1
    #[test]
    fn schema_yaml11_octal_suppressed_in_v1_1_mode() {
        let schema = object_schema_with_props(vec![("mode", string_schema())]);
        let text = "mode: 0755";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_1);
        assert!(
            result.is_empty(),
            "1.1 mode should suppress schema yaml11 octal warning: {result:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group C: enhanced schemaType — boolean-typed field with YAML 1.1 bool value
    // ══════════════════════════════════════════════════════════════════════════

    // C1: enhanced error emitted for all 16 YAML 1.1 bool forms in boolean-typed field
    #[rstest]
    #[case::yes_lowercase("yes")]
    #[case::yes_titlecase("Yes")]
    #[case::yes_uppercase("YES")]
    #[case::no_lowercase("no")]
    #[case::no_titlecase("No")]
    #[case::no_uppercase("NO")]
    #[case::on_lowercase("on")]
    #[case::on_titlecase("On")]
    #[case::on_uppercase("ON")]
    #[case::off_lowercase("off")]
    #[case::off_titlecase("Off")]
    #[case::off_uppercase("OFF")]
    #[case::y_lowercase("y")]
    #[case::y_uppercase("Y")]
    #[case::n_lowercase("n")]
    #[case::n_uppercase("N")]
    fn schema_yaml11_boolean_type_error_for_boolean_field(#[case] value: &str) {
        let schema = object_schema_with_props(vec![("enabled", boolean_schema())]);
        let text = format!("enabled: {value}");
        let docs = parse_docs(&text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(
            result.len(),
            1,
            "expected one diagnostic for '{value}', got: {result:?}"
        );
        assert_eq!(code_of(&result[0]), "schemaYaml11BooleanType");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // C2: enhanced error message explains the YAML 1.1 context
    #[test]
    fn schema_yaml11_boolean_type_message_explains_1_1_context() {
        let schema = object_schema_with_props(vec![("enabled", boolean_schema())]);
        let text = "enabled: yes";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(msg.contains("yes"), "message should contain 'yes': {msg}");
        assert!(
            msg.contains("boolean"),
            "message should mention boolean: {msg}"
        );
        assert!(
            msg.contains("1.1"),
            "message should reference YAML 1.1: {msg}"
        );
        assert!(
            msg.contains("true") || msg.contains("false"),
            "message should suggest true/false: {msg}"
        );
    }

    // C3: plain YAML 1.2 boolean values accepted without error (no regression)
    #[rstest]
    #[case::true_value("true")]
    #[case::false_value("false")]
    fn schema_yaml11_boolean_type_no_error_for_yaml12_booleans(#[case] value: &str) {
        let schema = object_schema_with_props(vec![("enabled", boolean_schema())]);
        let text = format!("enabled: {value}");
        let docs = parse_docs(&text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.is_empty(),
            "YAML 1.2 boolean should not produce error: {result:?}"
        );
    }

    // C4: non-1.1-bool mismatch still produces generic schemaType error
    #[test]
    fn schema_yaml11_boolean_type_generic_error_for_non_1_1_mismatch() {
        let schema = object_schema_with_props(vec![("enabled", boolean_schema())]);
        let text = "enabled: hello";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            !result[0].message.contains("1.1"),
            "generic mismatch should not mention YAML 1.1: {}",
            result[0].message
        );
    }

    // C5: enhanced error suppressed when yaml_version is V1_1 (yes is a valid boolean)
    #[test]
    fn schema_yaml11_boolean_type_suppressed_in_v1_1_mode() {
        let schema = object_schema_with_props(vec![("enabled", boolean_schema())]);
        let text = "enabled: yes";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_1);
        assert!(
            result.is_empty(),
            "in 1.1 mode 'yes' is a valid boolean — no error expected: {result:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group D: no duplicate diagnostics
    // ══════════════════════════════════════════════════════════════════════════

    // D1: string-typed field with 1.1 bool emits only schemaYaml11Boolean, not schemaType
    #[test]
    fn schema_yaml11_boolean_no_schema_type_for_string_field() {
        let schema = object_schema_with_props(vec![("flag", string_schema())]);
        let text = "flag: yes";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().all(|d| code_of(d) != "schemaType"),
            "should not emit schemaType for string field with 1.1 bool: {result:?}"
        );
    }

    // D2: boolean-typed field with 1.1 bool emits exactly one diagnostic
    #[test]
    fn schema_yaml11_boolean_type_emits_exactly_one_diagnostic() {
        let schema = object_schema_with_props(vec![("enabled", boolean_schema())]);
        let text = "enabled: yes";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(
            result.len(),
            1,
            "exactly one diagnostic expected: {result:?}"
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // Diagnostic range (span-based) regression tests
    // ──────────────────────────────────────────────────────────────────────

    #[test]
    fn diagnostic_range_type_mismatch_points_at_value_node() {
        // "age" expects integer; "hello" is a string scalar at col 5–10.
        let schema = object_schema_with_props(vec![("age", integer_schema())]);
        let docs = parse_docs("age: hello");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        // "hello" value: line 0 (0-based), col 5..10
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 5, "start column");
        assert_eq!(diag.range.end.line, 0, "end line");
        assert_eq!(diag.range.end.character, 10, "end column");
    }

    #[test]
    fn diagnostic_range_missing_required_points_at_mapping() {
        // Required "name" is absent; diagnostic should span the mapping root.
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaRequired")
            .expect("expected a schemaRequired diagnostic");
        // Entire mapping starts at line 0, col 0
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 0, "start column");
    }

    #[test]
    fn diagnostic_range_additional_property_points_at_key_node() {
        // "extra" key is not allowed; range should span the "extra" key node.
        let schema = JsonSchema {
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice\nextra: value");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaAdditionalProperty")
            .expect("expected a schemaAdditionalProperty diagnostic");
        // "extra" key: line 1 (0-based), col 0..5
        assert_eq!(diag.range.start.line, 1, "start line");
        assert_eq!(diag.range.start.character, 0, "start column");
        assert_eq!(diag.range.end.line, 1, "end line");
        assert_eq!(diag.range.end.character, 5, "end column");
    }

    #[test]
    fn diagnostic_range_format_validation_points_at_value_node() {
        // "date" field has an invalid date value "not-a-date" at col 6..16.
        let date_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            format: Some("date".to_string()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("date", date_schema)]);
        let docs = parse_docs("date: not-a-date");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaFormat")
            .expect("expected a schemaFormat diagnostic");
        // "not-a-date" value: line 0, col 6..16
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 6, "start column");
        assert_eq!(diag.range.end.line, 0, "end line");
        assert_eq!(diag.range.end.character, 16, "end column");
    }

    #[test]
    fn diagnostic_range_yaml11_string_warning_points_at_value_node() {
        // "yes" is a YAML 1.1 boolean but schema expects string — warning at value.
        let schema = object_schema_with_props(vec![("flag", string_schema())]);
        let docs = parse_docs("flag: yes");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaYaml11Boolean")
            .expect("expected a schemaYaml11Boolean diagnostic");
        // "yes" value: line 0, col 6..9
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 6, "start column");
        assert_eq!(diag.range.end.line, 0, "end line");
        assert_eq!(diag.range.end.character, 9, "end column");
    }

    #[test]
    fn diagnostic_range_composition_error_points_at_node() {
        // anyOf with two string-type branches; integer value matches neither.
        let schema = JsonSchema {
            any_of: Some(vec![string_schema(), string_schema()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("42");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        // Scalar "42" at line 0, col 0..2
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 0, "start column");
        assert_eq!(diag.range.end.line, 0, "end line");
        assert_eq!(diag.range.end.character, 2, "end column");
    }

    #[test]
    fn diagnostic_range_deeply_nested_violation_points_at_correct_node() {
        // Deeply nested: root > items > [0] > value is a string; expects integer.
        let inner = integer_schema();
        let arr = JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            items: Some(Box::new(inner)),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("items", arr)]);
        let docs = parse_docs("items:\n  - hello");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        // "hello" item: line 1 (0-based), col 4..9
        assert_eq!(diag.range.start.line, 1, "start line");
        assert_eq!(diag.range.start.character, 4, "start column");
        assert_eq!(diag.range.end.line, 1, "end line");
        assert_eq!(diag.range.end.character, 9, "end column");
    }

    // T-R2: type mismatch on nested scalar
    #[test]
    fn diagnostic_range_type_mismatch_nested_scalar() {
        let port_schema = integer_schema();
        let spec_schema = object_schema_with_props(vec![("port", port_schema)]);
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let docs = parse_docs("spec:\n  port: hello");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        // "hello" on line 2, col 8 (0-indexed line 1, col 8)
        assert_eq!(diag.range.start.line, 1, "start line");
        assert_eq!(diag.range.start.character, 8, "start column");
    }

    // T-R3: missing required property range derived from AST root
    #[test]
    fn diagnostic_range_missing_required_uses_ast_mapping_loc() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let text = "age: 30";
        let docs = parse_docs(text);
        let idx = docs[0].line_index();
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaRequired")
            .expect("expected a schemaRequired diagnostic");
        // Derive expected range from the AST directly
        let root_loc = node_loc(&docs[0].root);
        let expected_start = Position::new(
            idx.line_column(root_loc.start).0.saturating_sub(1),
            idx.line_column(root_loc.start).1,
        );
        assert_eq!(
            diag.range.start, expected_start,
            "range must match mapping loc"
        );
    }

    // T-R4: missing required at nested path points to nested mapping
    #[test]
    fn diagnostic_range_missing_required_nested_mapping() {
        let spec_schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let docs = parse_docs("spec:\n  other: value");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaRequired")
            .expect("expected a schemaRequired diagnostic");
        // The nested mapping `spec` starts at line 2 (0-indexed line 1)
        assert_eq!(
            diag.range.start.line, 1,
            "nested mapping is on 0-indexed line 1"
        );
    }

    // T-R6: additional property on indented key
    #[test]
    fn diagnostic_range_additional_property_indented_key() {
        let inner_schema = JsonSchema {
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", inner_schema)]);
        let docs = parse_docs("spec:\n  name: Alice\n  bad: x");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaAdditionalProperty")
            .expect("expected a schemaAdditionalProperty diagnostic");
        // "bad" key: line 3 (0-indexed line 2), col 2
        assert_eq!(diag.range.start.line, 2, "start line");
        assert_eq!(diag.range.start.character, 2, "start column");
    }

    // T-R10: oneOf zero-match range points to offending node
    #[test]
    fn diagnostic_range_oneof_zero_match() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("val: hello");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        assert_eq!(diag.range.start.line, 0, "start line");
    }

    // T-R11: deeply nested three-level violation
    #[test]
    fn diagnostic_range_three_level_nested_violation() {
        let c_schema = integer_schema();
        let b_schema = object_schema_with_props(vec![("c", c_schema)]);
        let a_schema = object_schema_with_props(vec![("b", b_schema)]);
        let schema = object_schema_with_props(vec![("a", a_schema)]);
        let docs = parse_docs("a:\n  b:\n    c: hello");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        // "hello" on line 3 (0-indexed 2), col 7
        assert_eq!(diag.range.start.line, 2, "start line");
        assert_eq!(diag.range.start.character, 7, "start column");
    }

    // T-R12: contentSchema diagnostic range is on outer scalar
    #[test]
    fn diagnostic_range_content_schema_uses_outer_scalar_loc() {
        // base64("\"hello\"") = "ImhlbGxvIg==" — type mismatch (expects integer)
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let docs = parse_docs("\"ImhlbGxvIg==\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        // Outer scalar is on line 0 — not inside the decoded content
        assert_eq!(
            diag.range.start.line, 0,
            "must point at outer scalar, not inner content"
        );
    }

    // T-R13: enum violation range points to scalar node
    #[test]
    fn diagnostic_range_enum_violation_points_at_scalar() {
        let env_schema = JsonSchema {
            enum_values: Some(vec![
                serde_json::Value::String("prod".to_string()),
                serde_json::Value::String("staging".to_string()),
            ]),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("env", env_schema)]);
        let docs = parse_docs("env: testing");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaEnum")
            .expect("expected a schemaEnum diagnostic");
        // "testing" value: line 0, col 5..12
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 5, "start column");
    }

    // T-R14: minLength violation range points to scalar node
    #[test]
    fn diagnostic_range_min_length_violation_points_at_scalar() {
        let code_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            min_length: Some(5),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("code", code_schema)]);
        let docs = parse_docs("code: hi");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaMinLength")
            .expect("expected a schemaMinLength diagnostic");
        // "hi" value: line 0, col 6
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 6, "start column");
    }

    // T-R15: minItems violation range derived from sequence AST node
    #[test]
    fn diagnostic_range_min_items_uses_sequence_loc() {
        let tags_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            min_items: Some(2),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("tags", tags_schema)]);
        let text = "tags:\n  - a";
        let docs = parse_docs(text);
        let idx = docs[0].line_index();
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaMinItems")
            .expect("expected a schemaMinItems diagnostic");
        // Derive expected range from the sequence node in the AST
        let seq_loc = if let Node::Mapping { entries, .. } = &docs[0].root {
            let (_, v) = entries
                .iter()
                .find(|(k, _)| matches!(k, Node::Scalar { value, .. } if value == "tags"))
                .expect("tags key");
            node_loc(v)
        } else {
            panic!("expected mapping root");
        };
        let expected_start = Position::new(
            idx.line_column(seq_loc.start).0.saturating_sub(1),
            idx.line_column(seq_loc.start).1,
        );
        assert_eq!(
            diag.range.start, expected_start,
            "range must match sequence loc"
        );
    }

    // T-R16: range uses 0-based line numbers (off-by-one regression guard)
    #[test]
    fn diagnostic_range_uses_zero_based_lines() {
        let schema = object_schema_with_props(vec![("x", integer_schema())]);
        let docs = parse_docs("x: bad");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        assert_eq!(diag.range.start.line, 0, "must be 0-based (not 1-based)");
    }

    // T-R17: range on second line is correct
    #[test]
    fn diagnostic_range_second_line_is_correct() {
        let schema =
            object_schema_with_props(vec![("ok", integer_schema()), ("bad", integer_schema())]);
        let docs = parse_docs("ok: 1\nbad: hello");
        let result = validate_schema(&docs, &schema, false, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("expected a schemaType diagnostic");
        // "hello" is on 0-indexed line 1
        assert_eq!(diag.range.start.line, 1, "second line is 0-indexed 1");
    }

    // T-R18: format violation on third line has correct line number
    #[test]
    fn diagnostic_range_format_violation_third_line() {
        let date_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            format: Some("date".to_string()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![
            ("a", string_schema()),
            ("b", string_schema()),
            ("c", date_schema),
        ]);
        let docs = parse_docs("a: foo\nb: bar\nc: not-a-date");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaFormat")
            .expect("expected a schemaFormat diagnostic");
        // "not-a-date" is on 0-indexed line 2
        assert_eq!(diag.range.start.line, 2, "third line is 0-indexed 2");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group 1: yaml_type_name — tag-URI-driven classification
    // ══════════════════════════════════════════════════════════════════════════

    // T1.1 — plain null tagged as null → "null"
    #[test]
    fn tag_driven_null_classified_as_null_type() {
        let schema = object_schema_with_props(vec![("value", string_schema())]);
        let docs = parse_docs("value: ~");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("null"),
            "expected 'null' in message"
        );
    }

    // T1.2 — plain bool tagged as bool → "boolean"
    #[test]
    fn tag_driven_bool_classified_as_boolean_type() {
        let schema = object_schema_with_props(vec![("value", integer_schema())]);
        let docs = parse_docs("value: true");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("boolean"),
            "expected 'boolean' in message"
        );
    }

    // T1.3 — plain integer tagged as int → "integer"
    #[test]
    fn tag_driven_integer_classified_as_integer_type() {
        let schema = object_schema_with_props(vec![("value", string_schema())]);
        let docs = parse_docs("value: 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("integer"),
            "expected 'integer' in message"
        );
    }

    // T1.4 — plain float tagged as float → "number"
    #[test]
    fn tag_driven_float_classified_as_number_type() {
        let schema = object_schema_with_props(vec![("value", integer_schema())]);
        let docs = parse_docs("value: 3.14");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("number"),
            "expected 'number' in message"
        );
    }

    // T1.5 — plain string tagged as str → "string"
    #[test]
    fn tag_driven_string_classified_as_string_type() {
        let schema = object_schema_with_props(vec![("value", integer_schema())]);
        let docs = parse_docs("value: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("string"),
            "expected 'string' in message"
        );
    }

    // T1.6 — double-quoted scalar → always "string" regardless of content
    #[test]
    fn tag_driven_quoted_integer_looking_value_classified_as_string() {
        let schema = object_schema_with_props(vec![("value", integer_schema())]);
        let docs = parse_docs("value: \"42\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("string"),
            "quoted scalar must resolve as string"
        );
    }

    // T1.7 — explicit !!bool on an otherwise-string value → "boolean"
    #[test]
    fn tag_driven_explicit_bool_tag_overrides_value_content() {
        let schema = object_schema_with_props(vec![("value", integer_schema())]);
        let docs = parse_docs("value: !!bool \"yes\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("boolean"),
            "!!bool tag must drive classification"
        );
    }

    // T1.8 — explicit !!str on a value that looks like a number → "string"
    #[test]
    fn tag_driven_explicit_str_tag_on_integer_looking_value() {
        let schema = object_schema_with_props(vec![("value", integer_schema())]);
        let docs = parse_docs("value: !!str 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert!(
            result[0].message.contains("string"),
            "!!str tag must override to string"
        );
    }
}
