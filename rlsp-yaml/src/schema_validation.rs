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
    if schema.unevaluated_properties.is_some()
        && let Node::Mapping { entries, .. } = node
    {
        mapping_constraints::validate_unevaluated_properties(entries, schema, path, ctx, depth);
    }

    // unevaluatedItems (Draft 2019-09)
    if schema.unevaluated_items.is_some()
        && let Node::Sequence { items, .. } = node
    {
        array_constraints::validate_unevaluated_items(items, schema, path, ctx, depth);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

    use super::*;
    use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};
    use crate::test_utils::parse_docs;
    use serde_json::json;

    use super::support::test_fixtures::{
        code_of, integer_schema, object_schema_with_props, string_schema,
    };

    // ══════════════════════════════════════════════════════════════════════════
    // Nested validation
    // ══════════════════════════════════════════════════════════════════════════

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

    #[test]
    fn should_return_empty_when_docs_is_empty() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let result = validate_schema(&[], &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_return_empty_for_schema_with_no_constraints() {
        let schema = JsonSchema::default();
        let docs = parse_docs("anything: value\nnested:\n  key: 123");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

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
}
