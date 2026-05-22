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
    use super::super::support::test_fixtures::{object_schema_with_props, string_schema};
    use crate::schema::{JsonSchema, SchemaType};
    use crate::server::YamlVersion;
    use crate::test_utils::parse_docs;

    use super::super::validate_schema;

    // ══════════════════════════════════════════════════════════════════════════
    // Composition (allOf / anyOf / oneOf)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 29
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

    // Test 30
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

    // Test 31
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

    // Test 32
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

    // Test 33
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

    // Test 34
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

    // Test 35
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
}
