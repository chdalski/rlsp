// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::lsp_util::span_to_lsp;
use crate::schema::JsonSchema;

use super::context::Ctx;
use super::support::{MAX_BRANCH_COUNT, format_path, make_diagnostic, node_loc};

pub(super) fn validate_composition(
    node: &Node<Span>,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let format_validation = ctx.format_validation;
    let yaml_version = ctx.yaml_version;
    let node_range = span_to_lsp(node_loc(node), ctx.idx);

    // allOf: all branches must pass
    if let Some(all_of) = &schema.all_of {
        for branch in all_of.iter().take(MAX_BRANCH_COUNT) {
            super::validate_node(node, branch, path, ctx, depth + 1);
        }
    }

    // anyOf: at least one branch must pass; if none do, emit a diagnostic
    if let Some(any_of) = &schema.any_of {
        let branch_count = any_of.iter().take(MAX_BRANCH_COUNT).count();
        let any_passes = any_of.iter().take(MAX_BRANCH_COUNT).any(|branch| {
            let mut scratch = Vec::new();
            let mut probe = Ctx::new(&mut scratch, format_validation, yaml_version, ctx.idx);
            super::validate_node(node, branch, path, &mut probe, depth + 1);
            scratch.is_empty()
        });
        if !any_passes {
            ctx.diagnostics.push(make_diagnostic(
                node_range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match any of the {branch_count} allowed schemas (anyOf)",
                    format_path(path)
                ),
            ));
        }
    }

    // oneOf: exactly one branch must pass
    if let Some(one_of) = &schema.one_of {
        let total = one_of.iter().take(MAX_BRANCH_COUNT).count();
        let passing = one_of
            .iter()
            .take(MAX_BRANCH_COUNT)
            .filter(|branch| {
                let mut scratch = Vec::new();
                let mut probe = Ctx::new(&mut scratch, format_validation, yaml_version, ctx.idx);
                super::validate_node(node, branch, path, &mut probe, depth + 1);
                scratch.is_empty()
            })
            .count();

        if passing == 0 {
            ctx.diagnostics.push(make_diagnostic(
                node_range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match any of the {total} oneOf schemas",
                    format_path(path)
                ),
            ));
        } else if passing > 1 {
            ctx.diagnostics.push(make_diagnostic(
                node_range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} matches {passing} of the {total} oneOf schemas (expected exactly 1)",
                    format_path(path)
                ),
            ));
        }
    }

    // not: the value must NOT match the sub-schema
    if let Some(not_schema) = &schema.not {
        let mut scratch = Vec::new();
        let mut probe = Ctx::new(&mut scratch, format_validation, yaml_version, ctx.idx);
        super::validate_node(node, not_schema, path, &mut probe, depth + 1);
        if scratch.is_empty() {
            ctx.diagnostics.push(make_diagnostic(
                node_range,
                DiagnosticSeverity::ERROR,
                "schemaNot",
                format!(
                    "Value at {} must not match the schema defined in 'not'",
                    format_path(path)
                ),
            ));
        }
    }

    // if / then / else (Draft-07)
    if let Some(if_schema) = &schema.if_schema {
        let mut scratch = Vec::new();
        let mut probe = Ctx::new(&mut scratch, format_validation, yaml_version, ctx.idx);
        super::validate_node(node, if_schema, path, &mut probe, depth + 1);
        if scratch.is_empty() {
            if let Some(then_schema) = &schema.then_schema {
                super::validate_node(node, then_schema, path, ctx, depth + 1);
            }
        } else if let Some(else_schema) = &schema.else_schema {
            super::validate_node(node, else_schema, path, ctx, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::DiagnosticSeverity;

    use super::super::support::test_fixtures::{code_of, object_schema_with_props, string_schema};
    use crate::schema::{JsonSchema, SchemaType};
    use crate::server::YamlVersion;
    use crate::test_utils::parse_docs;
    use serde_json::json;

    use super::super::validate_schema;

    // ══════════════════════════════════════════════════════════════════════════
    // Composition (allOf / anyOf / oneOf)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_produce_no_diagnostics_when_all_of_all_pass() {
        let schema = JsonSchema {
            all_of: Some(vec![
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
        let docs = parse_docs("a: 1\nb: 2");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_diagnostics_when_any_all_of_fails() {
        let schema = JsonSchema {
            all_of: Some(vec![
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
        let docs = parse_docs("a: 1");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
    }

    #[test]
    fn should_produce_no_diagnostics_when_any_of_one_passes() {
        let schema = JsonSchema {
            any_of: Some(vec![
                object_schema_with_props(vec![("name", string_schema())]),
                object_schema_with_props(vec![(
                    "name",
                    JsonSchema {
                        schema_type: Some(SchemaType::Single("integer".to_string())),
                        ..JsonSchema::default()
                    },
                )]),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_diagnostic_when_none_of_any_of_pass() {
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
    }

    #[test]
    fn should_produce_no_diagnostics_when_exactly_one_of_passes() {
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
        let docs = parse_docs("a: 1");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_diagnostic_when_zero_of_one_of_pass() {
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
    }

    #[test]
    fn should_produce_diagnostic_when_multiple_of_one_of_pass() {
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
        let docs = parse_docs("a: hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(!result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // if / then / else
    // ══════════════════════════════════════════════════════════════════════════

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

    // ══════════════════════════════════════════════════════════════════════════
    // not keyword
    // ══════════════════════════════════════════════════════════════════════════

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
    // Message consistency — anyOf branch count
    // ══════════════════════════════════════════════════════════════════════════

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

    // ── Diagnostic range — composition ──────────────────────────────────────

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
}
