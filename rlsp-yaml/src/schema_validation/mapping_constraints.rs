// SPDX-License-Identifier: MIT

use std::borrow::Cow;

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::lsp_util::span_to_lsp;
use crate::schema::{AdditionalProperties, JsonSchema};

use super::context::Ctx;
use super::support::{
    MAX_ENUM_DISPLAY, MAX_PATTERN_LEN, collect_evaluated_properties, entries_contains_key,
    format_path, get_regex, make_diagnostic, node_key_str, node_loc,
};
use super::validate_node;

pub(super) fn validate_unevaluated_properties(
    entries: &[(Node<Span>, Node<Span>)],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    for (k, v) in entries {
        let Some(key_str) = node_key_str(k) else {
            continue;
        };
        if collect_evaluated_properties(schema, &key_str) {
            continue;
        }
        match &schema.unevaluated_properties {
            Some(AdditionalProperties::Denied) => {
                let range = span_to_lsp(node_loc(k), ctx.idx);
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::WARNING,
                    "schemaUnevaluatedProperty",
                    format!(
                        "Unevaluated property '{}' is not allowed at {}",
                        key_str,
                        format_path(path)
                    ),
                ));
            }
            Some(AdditionalProperties::Schema(extra_schema)) => {
                let mut child_path = path.to_vec();
                child_path.push(key_str.clone());
                validate_node(v, extra_schema, &child_path, ctx, depth + 1);
            }
            None => {}
        }
    }
}

pub(super) fn validate_mapping_constraints(
    entries: &[(Node<Span>, Node<Span>)],
    mapping_loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    let len = entries.len() as u64;

    if let Some(min) = schema.min_properties {
        if len < min {
            let range = span_to_lsp(mapping_loc, ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinProperties",
                format!(
                    "Object at {} has {} properties, minimum is {}",
                    format_path(path),
                    len,
                    min
                ),
            ));
        }
    }

    if let Some(max) = schema.max_properties {
        if len > max {
            let range = span_to_lsp(mapping_loc, ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaxProperties",
                format!(
                    "Object at {} has {} properties, maximum is {}",
                    format_path(path),
                    len,
                    max
                ),
            ));
        }
    }
}

pub(super) fn validate_mapping(
    entries: &[(Node<Span>, Node<Span>)],
    mapping_loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    validate_mapping_constraints(entries, mapping_loc, schema, path, ctx);

    let properties = schema.properties.as_ref();

    // Required properties
    if let Some(required) = &schema.required {
        let listed: Vec<&str> = required
            .iter()
            .take(MAX_ENUM_DISPLAY)
            .map(String::as_str)
            .collect();
        let props_list = if required.len() > MAX_ENUM_DISPLAY {
            format!("{}, ... ({} total)", listed.join(", "), required.len())
        } else {
            listed.join(", ")
        };
        ctx.diagnostics.extend(
            required
                .iter()
                .filter(|req_key| !entries_contains_key(entries, req_key))
                .map(|req_key| {
                    let range = span_to_lsp(mapping_loc, ctx.idx);
                    make_diagnostic(
                        range,
                        DiagnosticSeverity::ERROR,
                        "schemaRequired",
                        format!(
                            "Object at {} is missing required property '{}'. Expected: {}.",
                            format_path(path),
                            req_key,
                            props_list
                        ),
                    )
                }),
        );
    }

    // Validate known properties and check for additional properties
    for (k, v) in entries {
        let Some(key_str) = node_key_str(k) else {
            continue;
        };

        let is_known = properties.is_some_and(|p| p.contains_key(&key_str));

        if let Some(prop_schema) = properties.and_then(|p| p.get(&key_str)) {
            let mut child_path = path.to_vec();
            child_path.push(key_str.clone());
            validate_node(v, prop_schema, &child_path, ctx, depth + 1);
        } else {
            // Check patternProperties for keys not in properties
            let matched_by_pattern =
                validate_pattern_properties(v, &key_str, schema, path, ctx, depth);

            if !is_known && !matched_by_pattern {
                // Check additionalProperties
                match &schema.additional_properties {
                    Some(AdditionalProperties::Denied) => {
                        let range = span_to_lsp(node_loc(k), ctx.idx);
                        ctx.diagnostics.push(make_diagnostic(
                            range,
                            DiagnosticSeverity::WARNING,
                            "schemaAdditionalProperty",
                            format!(
                                "Additional property '{}' is not allowed at {}",
                                key_str,
                                format_path(path)
                            ),
                        ));
                    }
                    Some(AdditionalProperties::Schema(extra_schema)) => {
                        let mut child_path = path.to_vec();
                        child_path.push(key_str.clone());
                        validate_node(v, extra_schema, &child_path, ctx, depth + 1);
                    }
                    None => {}
                }
            }
        }

        // propertyNames — validate each key as a string node against the schema
        if let Some(pn_schema) = &schema.property_names {
            // Create a temporary scalar node for the key string.
            // Keys are always strings, so tag must be set accordingly.
            let key_node = Node::Scalar {
                value: key_str.clone(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                tag: Some(Cow::Borrowed("tag:yaml.org,2002:str")),
                loc: rlsp_yaml_parser::Span { start: 0, end: 0 },
                meta: None,
            };
            validate_node(&key_node, pn_schema, path, ctx, depth + 1);
        }
    }

    validate_dependencies(entries, mapping_loc, schema, path, ctx, depth);
}

pub(super) fn validate_dependencies(
    entries: &[(Node<Span>, Node<Span>)],
    mapping_loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    // dependentRequired: if trigger key is present, listed keys must also be present
    if let Some(dep_req) = &schema.dependent_required {
        for (trigger, required_keys) in dep_req {
            if entries_contains_key(entries, trigger) {
                for missing in required_keys {
                    if !entries_contains_key(entries, missing) {
                        let range = span_to_lsp(mapping_loc, ctx.idx);
                        ctx.diagnostics.push(make_diagnostic(
                            range,
                            DiagnosticSeverity::ERROR,
                            "schemaDependentRequired",
                            format!(
                                "Property '{}' is required when '{}' is present at {}",
                                missing,
                                trigger,
                                format_path(path)
                            ),
                        ));
                    }
                }
            }
        }
    }

    // dependentSchemas: if trigger key is present, validate the whole mapping
    if let Some(dep_sch) = &schema.dependent_schemas {
        for (trigger, dep_schema) in dep_sch {
            if entries_contains_key(entries, trigger) {
                // Reconstruct a mapping node to validate against the dep schema.
                let mapping_node = Node::Mapping {
                    entries: entries.to_vec(),
                    style: rlsp_yaml_parser::CollectionStyle::Block,
                    tag: None,
                    loc: rlsp_yaml_parser::Span { start: 0, end: 0 },
                    meta: None,
                };
                validate_node(&mapping_node, dep_schema, path, ctx, depth + 1);
            }
        }
    }
}

/// Validate `value` against any `patternProperties` patterns that match `key`.
/// Returns `true` if the key was matched by at least one pattern.
pub(super) fn validate_pattern_properties(
    value: &Node<Span>,
    key: &str,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) -> bool {
    let Some(pattern_props) = &schema.pattern_properties else {
        return false;
    };

    let mut matched = false;
    for (pattern, pat_schema) in pattern_props {
        if pattern.len() > MAX_PATTERN_LEN {
            let range = span_to_lsp(node_loc(value), ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaPatternLimit",
                format!(
                    "Pattern at {} exceeds maximum length ({MAX_PATTERN_LEN} chars) and was not validated",
                    format_path(path),
                ),
            ));
            continue;
        }
        if let Some(re) = get_regex(pattern) {
            if re.is_match(key) {
                matched = true;
                let mut child_path = path.to_vec();
                child_path.push(key.to_string());
                validate_node(value, pat_schema, &child_path, ctx, depth + 1);
            }
        } else {
            let range = span_to_lsp(node_loc(value), ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaPatternLimit",
                format!(
                    "Pattern at {} could not be compiled and was not validated",
                    format_path(path),
                ),
            ));
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{DiagnosticSeverity, Position};

    use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};
    use crate::server::YamlVersion;
    use crate::test_utils::parse_docs;
    use serde_json::json;

    use super::super::support::test_fixtures::{
        code_of, integer_schema, object_schema_with_props, string_schema,
    };
    use super::super::validate_schema;

    // ══════════════════════════════════════════════════════════════════════════
    // Required properties
    // ══════════════════════════════════════════════════════════════════════════

    // Test 1
    #[test]
    fn should_produce_no_diagnostics_when_required_property_present() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            properties: Some([("name".to_string(), string_schema())].into()),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 2
    #[test]
    fn should_produce_error_for_missing_required_property() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaRequired");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("name"));
    }

    // Test 3
    #[test]
    fn should_produce_one_diagnostic_per_missing_required_property() {
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
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|d| code_of(d) == "schemaRequired"));
    }

    // Test 4
    #[test]
    fn should_produce_no_diagnostics_when_all_required_present() {
        let schema = JsonSchema {
            required: Some(vec!["a".to_string(), "b".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("a: 1\nb: 2");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 5
    #[test]
    fn should_produce_no_diagnostics_for_empty_required_array() {
        let schema = JsonSchema {
            required: Some(vec![]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("key: value");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 6
    #[test]
    fn should_validate_required_in_nested_mapping() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            required: Some(vec!["name".to_string()]),
            properties: Some([("name".to_string(), string_schema())].into()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  other: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaRequired");
        assert!(result[0].message.contains("name"));
        assert!(result[0].message.contains("spec"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Enum violations
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 19, 21, 23: enum match → no diagnostics
    #[rstest]
    #[case::string_value_in_string_enum(
        object_schema_with_props(vec![("env", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            enum_values: Some(vec![json!("prod"), json!("staging"), json!("dev")]),
            ..JsonSchema::default()
        })]),
        "env: staging"
    )]
    #[case::integer_value_in_integer_enum(
        object_schema_with_props(vec![("level", JsonSchema {
            enum_values: Some(vec![json!(1), json!(2), json!(3)]),
            ..JsonSchema::default()
        })]),
        "level: 2"
    )]
    #[case::string_value_in_mixed_type_enum(
        object_schema_with_props(vec![("value", JsonSchema {
            enum_values: Some(vec![json!("auto"), json!(0), serde_json::Value::Null]),
            ..JsonSchema::default()
        })]),
        "value: auto"
    )]
    fn enum_match_produces_no_diagnostics(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 20: enum value missing — message lists valid values (unique assertion shape)
    #[test]
    fn should_produce_error_when_value_not_in_enum() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging"), json!("dev")]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: testing");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaEnum");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        // Message should list valid values
        assert!(
            result[0].message.contains("prod"),
            "message should contain 'prod'"
        );
        assert!(
            result[0].message.contains("staging"),
            "message should contain 'staging'"
        );
        assert!(
            result[0].message.contains("dev"),
            "message should contain 'dev'"
        );
    }

    // Test 22: integer enum mismatch → schemaEnum error
    #[rstest]
    #[case::integer_value_not_in_enum(
        object_schema_with_props(vec![("level", JsonSchema {
            enum_values: Some(vec![json!(1), json!(2), json!(3)]),
            ..JsonSchema::default()
        })]),
        "level: 5"
    )]
    fn enum_mismatch_produces_schemaenum_error(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaEnum");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // additionalProperties
    // ══════════════════════════════════════════════════════════════════════════

    // Test 24
    #[test]
    fn should_produce_no_diagnostics_when_additional_properties_absent() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let text = "name: Alice\nextra: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 25
    #[test]
    fn should_produce_warning_for_extra_key_when_additional_properties_false() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nextra: value";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaAdditionalProperty");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(result[0].message.contains("extra"));
    }

    // Test 26
    #[test]
    fn should_produce_no_diagnostics_for_known_keys_when_additional_properties_false() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Test 27
    #[test]
    fn should_produce_one_warning_per_extra_key() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nextra1: a\nextra2: b";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|d| code_of(d) == "schemaAdditionalProperty")
        );
    }

    // Test 28
    #[test]
    fn should_validate_extra_properties_against_additional_properties_schema() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nextra: not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
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
    // Regex security hardening (patternProperties)
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
    // Required properties — message format (Tests 66–67, 229–231)
    // ══════════════════════════════════════════════════════════════════════════

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

    // ── Diagnostic range — mapping constraints ──────────────────────────────

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
        let root_loc = super::super::support::node_loc(&docs[0].root);
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

    // ── unevaluatedProperties ────────────────────────────────────────────────

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
}
