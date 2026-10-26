// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::lsp_util::span_to_lsp;
use crate::schema::{JsonSchema, ParsedSchema};
use crate::server::YamlVersion;

mod array_constraints;
#[cfg(test)]
mod budget_tests;
mod composition;
mod context;
mod formats;
mod mapping_constraints;
mod scalar_constraints;
mod support;
mod type_validation;

use context::{Budget, Ctx, MAX_DIAGNOSTICS, Mode, step_budget};
use support::{
    MAX_ENUM_DISPLAY, MAX_VALIDATION_DEPTH, format_path, make_diagnostic, node_loc, yaml_to_json,
};

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Whether a validation run examined everything it was asked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    /// Every document was validated in full.
    Complete,
    /// The run ran out of steps, or reached the diagnostic limit, and stopped
    /// early. Diagnostics found before that point stand; checks that depended on
    /// unfinished work report nothing.
    StoppedEarly,
}

/// The outcome of [`validate_schema`].
#[derive(Debug)]
pub struct Validation {
    /// Everything found before the run finished or stopped.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether the run covered all of its input.
    pub completeness: Completeness,
}

/// Validate `docs` (parsed YAML ASTs) against `schema`.
///
/// Each element of `docs` is one YAML document (separated by `---`).
/// `format_validation` controls whether the `format` keyword is validated.
/// `yaml_version` is used to suppress YAML 1.1 compatibility warnings when the
/// user has explicitly opted into 1.1 semantics.
///
/// All documents share one step budget sized from the documents' node count, so
/// a hostile schema cannot make the run cost more than a fixed multiple of the
/// input, and the run returns at most `MAX_DIAGNOSTICS` diagnostics. The
/// returned [`Completeness`] says whether either limit stopped it.
#[must_use]
pub fn validate_schema(
    docs: &[Document<Span>],
    schema: &ParsedSchema,
    format_validation: bool,
    yaml_version: YamlVersion,
) -> Validation {
    let budget = Budget::new(step_budget(docs));
    validate_documents(docs, &schema.root, format_validation, yaml_version, &budget)
}

/// [`validate_schema`] with an explicit `budget`, so tests can pick its size and
/// read back what the run spent.
fn validate_documents(
    docs: &[Document<Span>],
    schema: &JsonSchema,
    format_validation: bool,
    yaml_version: YamlVersion,
    budget: &Budget,
) -> Validation {
    let mut diagnostics = Vec::new();

    for doc in docs {
        let mut ctx = Ctx::new(
            &mut diagnostics,
            format_validation,
            yaml_version,
            doc.line_index(),
            budget,
        );
        validate_node(&doc.root, schema, &[], &mut ctx, 0);
    }

    // A node can record many diagnostics at once, so the entry check in
    // `validate_node` alone can overshoot the limit.
    if diagnostics.len() > MAX_DIAGNOSTICS {
        diagnostics.truncate(MAX_DIAGNOSTICS);
        budget.exhaust();
    }

    let completeness = if budget.is_exhausted() {
        Completeness::StoppedEarly
    } else {
        Completeness::Complete
    };
    Validation {
        diagnostics,
        completeness,
    }
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
        ctx.undecided = true;
        return;
    }
    if ctx.mode == Mode::Report && ctx.diagnostics.len() >= MAX_DIAGNOSTICS {
        ctx.budget.exhaust();
        return;
    }
    if ctx.probe_decided() || !ctx.budget.spend(1) {
        return;
    }

    // Type check — returns false when a type mismatch error was emitted and
    // further validation of this node should be skipped.
    if !type_validation::validate_type(node, schema, path, ctx) {
        return;
    }

    // Enum check. The value is compared with each listed one.
    if let Some(enum_values) = &schema.enum_values
        && ctx.budget.spend_work(enum_values.len())
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

    // A probe that has recorded a diagnostic is decided; each guard below
    // keeps the remaining phases — and the nested probes they start — from
    // running.
    if ctx.probe_decided() {
        return;
    }

    // Scalar constraints
    scalar_constraints::validate_scalar_constraints(node, schema, path, ctx);
    if ctx.probe_decided() {
        return;
    }

    // Mapping-specific checks
    if let Node::Mapping { entries, loc, .. } = node {
        mapping_constraints::validate_mapping(entries, *loc, schema, path, ctx, depth);
    }

    // Sequence-specific checks
    if let Node::Sequence { items, loc, .. } = node {
        array_constraints::validate_sequence(items, *loc, schema, path, ctx, depth);
    }
    if ctx.probe_decided() {
        return;
    }

    // Composition
    composition::validate_composition(node, schema, path, ctx, depth);
    if ctx.probe_decided() {
        return;
    }

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

/// How a probe ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Probe {
    /// Validated without a single diagnostic.
    Passes,
    /// Recorded a diagnostic.
    Fails,
    /// Recorded no diagnostic, but skipped checks it could not finish: its
    /// silence is not a pass.
    Undecided,
}

/// Passing probes counted over a run of branches or items.
struct Tally {
    passing: usize,
    /// Some probe was undecided, so `passing` may undercount.
    undecided: bool,
}

fn tally(outcomes: impl Iterator<Item = Probe>) -> Tally {
    outcomes.fold(
        Tally {
            passing: 0,
            undecided: false,
        },
        |tally, outcome| Tally {
            passing: tally.passing + usize::from(outcome == Probe::Passes),
            undecided: tally.undecided || outcome == Probe::Undecided,
        },
    )
}

/// Probe whether `node` validates against `schema`.
///
/// Validates into a scratch context that inherits `ctx`'s options and budget and
/// stops at the first diagnostic, so nothing reaches `ctx.diagnostics`. A caller
/// that gets [`Probe::Undecided`] must set `ctx.undecided`, so an enclosing probe
/// does not read the missing diagnostic as a pass.
fn probe_passes(
    node: &Node<Span>,
    schema: &JsonSchema,
    path: &[String],
    ctx: &Ctx<'_>,
    depth: usize,
) -> Probe {
    let mut scratch = Vec::new();
    let mut probe = Ctx::new(
        &mut scratch,
        ctx.format_validation,
        ctx.yaml_version,
        ctx.idx,
        ctx.budget,
    )
    .with_mode(Mode::Probe);
    validate_node(node, schema, path, &mut probe, depth);
    let undecided = probe.undecided;
    if !scratch.is_empty() {
        Probe::Fails
    } else if undecided || ctx.budget.is_exhausted() {
        Probe::Undecided
    } else {
        Probe::Passes
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
    use crate::schema::{AdditionalProperties, JsonSchema, ParsedSchema, SchemaType};
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(l1), true, YamlVersion::V1_2).diagnostics;
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
        let _ = validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2);
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
        assert!(result.is_empty());
    }

    #[test]
    fn should_return_empty_when_docs_is_empty() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let result =
            validate_schema(&[], &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
        assert!(result.is_empty());
    }

    #[test]
    fn should_return_empty_for_schema_with_no_constraints() {
        let schema = JsonSchema::default();
        let docs = parse_docs("anything: value\nnested:\n  key: 123");
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
        assert!(result.is_empty());
    }

    #[test]
    fn should_return_empty_for_yaml_with_parse_errors() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        // Simulate parse failure by passing empty docs slice
        let result =
            validate_schema(&[], &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Recursion / Depth Guard
    // ══════════════════════════════════════════════════════════════════════════

    // Nesting beyond MAX_VALIDATION_DEPTH is cut off without a panic. The cut is
    // a depth limit, not budget exhaustion, so the run still reports Complete.
    #[rstest]
    #[case::just_past_depth_limit(70)]
    #[case::far_past_depth_limit(100)]
    fn nesting_past_depth_limit_completes_without_panic(#[case] levels: usize) {
        let mut schema = string_schema();
        for _ in 0..levels {
            schema = JsonSchema {
                schema_type: Some(SchemaType::Single("object".to_string())),
                properties: Some([("child".to_string(), schema)].into()),
                ..JsonSchema::default()
            };
        }
        // Matching YAML whose leaf value is an integer (type mismatch)
        let mut text = String::new();
        for i in 0..levels {
            text.push_str(&"  ".repeat(i));
            text.push_str("child:\n");
        }
        text.push_str(&"  ".repeat(levels));
        text.push_str("42\n");
        let docs = parse_docs(&text);
        let result = validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2);
        assert_eq!(result.completeness, Completeness::Complete);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Composition branches are all evaluated
    // ══════════════════════════════════════════════════════════════════════════

    // Each branch requires a field the document lacks.
    fn required_field_branches(count: usize) -> Vec<JsonSchema> {
        (0..count)
            .map(|i| JsonSchema {
                required: Some(vec![format!("field_{i}")]),
                ..JsonSchema::default()
            })
            .collect()
    }

    #[test]
    fn fifty_failing_one_of_branches_report_one_diagnostic_naming_all_fifty() {
        let schema = JsonSchema {
            one_of: Some(required_field_branches(50)),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2);
        assert_eq!(result.completeness, Completeness::Complete);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(
            result.diagnostics[0].message.contains("50 oneOf"),
            "message: {}",
            result.diagnostics[0].message
        );
    }

    #[test]
    fn all_of_reports_a_diagnostic_for_every_unsatisfied_branch() {
        let schema = JsonSchema {
            all_of: Some(required_field_branches(25)),
            ..JsonSchema::default()
        };
        // Only field_0 is present — the other 24 branches are unsatisfied
        let docs = parse_docs("field_0: value");
        let result = validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2);
        assert_eq!(result.completeness, Completeness::Complete);
        let missing: Vec<&str> = result
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(missing.len(), 24, "diagnostics: {missing:?}");
        for i in 1..25 {
            assert!(
                missing
                    .iter()
                    .any(|m| m.contains(&format!("required property 'field_{i}'"))),
                "no diagnostic for field_{i}: {missing:?}"
            );
        }
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result =
            validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2).diagnostics;
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
        let result = validate_schema(&docs, &ParsedSchema::new(schema), false, YamlVersion::V1_2)
            .diagnostics;
        let diag = result
            .iter()
            .find(|d| code_of(d) == "schemaEnum")
            .expect("expected a schemaEnum diagnostic");
        // "testing" value: line 0, col 5..12
        assert_eq!(diag.range.start.line, 0, "start line");
        assert_eq!(diag.range.start.character, 5, "start column");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Probe mode — stops at the first diagnostic
    // ══════════════════════════════════════════════════════════════════════════

    /// Validate `text` against `raw_schema` directly in `mode` and count the
    /// diagnostics the walk recorded.
    fn recorded_diagnostics(raw_schema: &serde_json::Value, text: &str, mode: Mode) -> usize {
        let schema = crate::schema::parse_schema(raw_schema).expect("valid schema");
        let docs = parse_docs(text);
        let mut diagnostics = Vec::new();
        let budget = Budget::new(usize::MAX);
        let mut ctx = Ctx::new(
            &mut diagnostics,
            true,
            YamlVersion::V1_2,
            docs[0].line_index(),
            &budget,
        )
        .with_mode(mode);
        validate_node(&docs[0].root, &schema.root, &[], &mut ctx, 0);
        diagnostics.len()
    }

    // Every case records `report_count` diagnostics in report mode and exactly
    // one in probe mode. The cases where the failing node fails a second time
    // after its first diagnostic pin the guards between validation phases.
    #[rstest]
    #[case::mapping_children(
        json!({"properties": {
            "a": {"type": "integer"}, "b": {"type": "integer"}, "c": {"type": "integer"}
        }}),
        "a: p\nb: q\nc: r",
        3
    )]
    #[case::sequence_items(json!({"items": {"type": "integer"}}), "[p, q, r]", 3)]
    // base64("a: 1\nb: 2\n"): both content values violate the string schema.
    #[case::content_schema(
        json!({
            "type": "string",
            "contentEncoding": "base64",
            "contentSchema": {"properties": {"a": {"type": "string"}, "b": {"type": "string"}}}
        }),
        "\"YTogMQpiOiAyCg==\"",
        2
    )]
    #[case::scalar_constraint_after_enum(json!({"enum": ["x"], "minLength": 5}), "ab", 2)]
    #[case::contains_after_item_error(
        json!({"items": {"type": "integer"}, "contains": {"type": "integer"}}),
        "[a]",
        2
    )]
    #[case::required_before_children(
        json!({"required": ["z"], "properties": {"a": {"type": "integer"}}}),
        "a: p",
        2
    )]
    #[case::any_of_after_child_error(
        json!({"properties": {"a": {"type": "integer"}}, "anyOf": [{"required": ["z"]}]}),
        "a: p",
        2
    )]
    #[case::not_after_child_error(
        json!({"properties": {"a": {"type": "integer"}}, "not": {"type": "object"}}),
        "a: p",
        2
    )]
    #[case::one_of_after_any_of(
        json!({"anyOf": [{"required": ["z"]}], "oneOf": [{"required": ["z"]}]}),
        "a: p",
        2
    )]
    #[case::if_after_any_of(
        json!({
            "anyOf": [{"required": ["z"]}],
            "if": {"required": ["z"]},
            "else": {"required": ["w"]}
        }),
        "a: p",
        2
    )]
    fn walk_records_every_diagnostic_in_report_mode_and_the_first_in_probe_mode(
        #[case] raw_schema: serde_json::Value,
        #[case] text: &str,
        #[case] report_count: usize,
    ) {
        assert_eq!(
            recorded_diagnostics(&raw_schema, text, Mode::Report),
            report_count,
            "report mode"
        );
        assert_eq!(
            recorded_diagnostics(&raw_schema, text, Mode::Probe),
            1,
            "probe mode"
        );
    }
}
