// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::ScalarStyle;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::lsp_util::span_to_lsp;
use crate::scalar_helpers;
use crate::schema::{JsonSchema, SchemaType};
use crate::server::YamlVersion;

use super::context::Ctx;
use super::support::{
    display_schema_type, format_path, make_diagnostic, node_loc, single_type_or_contains,
    type_matches, yaml_type_name,
};

/// Compute the effective YAML type for a plain scalar under the given schema,
/// taking YAML 1.1 boolean promotion into account.
///
/// In `V1_1` mode a plain scalar that matches `is_yaml11_bool` and the schema
/// expects `boolean` is treated as `"boolean"` so the type check passes.
pub(super) fn effective_yaml_type<'a>(
    node: &Node<Span>,
    schema_type: &SchemaType,
    yaml_type: &'a str,
    is_plain: bool,
    yaml_version: YamlVersion,
) -> &'a str {
    if yaml_version == YamlVersion::V1_1
        && is_plain
        && yaml_type == "string"
        && single_type_or_contains(schema_type, "boolean")
    {
        if let Node::Scalar { value, .. } = node {
            if scalar_helpers::is_yaml11_bool(value) {
                return "boolean";
            }
        }
    }
    yaml_type
}

/// Check the schema type constraint for `node`.
///
/// Returns `true` when validation should continue (type matched or no type
/// constraint).  Returns `false` when a type mismatch diagnostic was emitted
/// and the caller should stop further validation of this node.
pub(super) fn validate_type(
    node: &Node<Span>,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) -> bool {
    let Some(schema_type) = &schema.schema_type else {
        return true;
    };

    let yaml_type = yaml_type_name(node);
    let is_plain =
        matches!(node, Node::Scalar { style, .. } if matches!(style, ScalarStyle::Plain));
    let effective = effective_yaml_type(node, schema_type, yaml_type, is_plain, ctx.yaml_version);

    if !type_matches(effective, schema_type) {
        let range = span_to_lsp(node_loc(node), ctx.idx);
        let (code, message) = type_mismatch_diagnostic(
            node,
            schema_type,
            path,
            effective,
            is_plain,
            ctx.yaml_version,
        );
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            code,
            message,
        ));
        return false;
    }

    // Scenario A: string-typed field with a YAML 1.1 boolean or octal value.
    // The value IS valid in YAML 1.2 (it's a string), but downstream 1.1
    // parsers will interpret it differently.
    if ctx.yaml_version == YamlVersion::V1_2
        && is_plain
        && effective == "string"
        && single_type_or_contains(schema_type, "string")
    {
        emit_yaml11_string_warnings(node, path, ctx);
    }

    true
}

/// Build the diagnostic code and message for a type mismatch.
///
/// For Scenario B (boolean-typed field with a YAML 1.1 bool value in `V1_2`
/// mode), the message explains the 1.1/1.2 difference and uses the
/// `schemaYaml11BooleanType` code so code-action dispatch can offer a
/// "Convert to boolean" fix.
pub(super) fn type_mismatch_diagnostic(
    node: &Node<Span>,
    schema_type: &SchemaType,
    path: &[String],
    effective_type: &str,
    is_plain: bool,
    yaml_version: YamlVersion,
) -> (&'static str, String) {
    if yaml_version == YamlVersion::V1_2
        && is_plain
        && effective_type == "string"
        && single_type_or_contains(schema_type, "boolean")
    {
        if let Node::Scalar { value, .. } = node {
            if scalar_helpers::is_yaml11_bool(value) {
                let canonical = scalar_helpers::yaml11_bool_canonical(value);
                return (
                    "schemaYaml11BooleanType",
                    format!(
                        "Value at {} does not match type: expected boolean, got string. \
                         \"{value}\" is not a boolean in YAML 1.2 — use {canonical} instead. \
                         (In YAML 1.1, \"{value}\" was a boolean.)",
                        format_path(path),
                    ),
                );
            }
        }
    }
    (
        "schemaType",
        format!(
            "Value at {} does not match type: expected {}, got {}",
            format_path(path),
            display_schema_type(schema_type),
            effective_type,
        ),
    )
}

/// Emit `schemaYaml11Boolean` or `schemaYaml11Octal` warnings when a plain
/// scalar that passes the `string` type check would be interpreted differently
/// by a YAML 1.1 parser.
pub(super) fn emit_yaml11_string_warnings(node: &Node<Span>, path: &[String], ctx: &mut Ctx<'_>) {
    let Node::Scalar { value, .. } = node else {
        return;
    };
    if scalar_helpers::is_yaml11_bool(value) {
        let canonical = scalar_helpers::yaml11_bool_canonical(value);
        let range = span_to_lsp(node_loc(node), ctx.idx);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::WARNING,
            "schemaYaml11Boolean",
            format!(
                "Value at {} is a string in YAML 1.2 but a boolean in YAML 1.1. \
                 Most tools use 1.1 parsers and will interpret \"{value}\" as {canonical}. \
                 Quote it (\"{value}\") or use {canonical}.",
                format_path(path),
            ),
        ));
    } else if scalar_helpers::is_yaml11_octal(value) {
        let decimal = i64::from_str_radix(&value[1..], 8).unwrap_or(0);
        let yaml12 = format!("0o{}", &value[1..]);
        let range = span_to_lsp(node_loc(node), ctx.idx);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::WARNING,
            "schemaYaml11Octal",
            format!(
                "Value at {} is a string in YAML 1.2 but octal {decimal} in YAML 1.1. \
                 Quote it (\"{value}\") or use {yaml12} (YAML 1.2 only).",
                format_path(path),
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    use crate::schema::{JsonSchema, SchemaType};
    use crate::server::YamlVersion;
    use crate::test_utils::parse_docs;

    use super::super::support::test_fixtures::{
        code_of, integer_schema, object_schema_with_props, string_schema,
    };
    use super::super::validate_schema;

    // ══════════════════════════════════════════════════════════════════════════
    // Type mismatches
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 8–13: type mismatch → schemaType ERROR
    #[rstest]
    #[case::string_where_integer_expected(
        object_schema_with_props(vec![("count", integer_schema())]),
        "count: \"hello\""
    )]
    #[case::integer_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: 42"
    )]
    #[case::boolean_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: true"
    )]
    #[case::mapping_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name:\n  nested: value"
    )]
    #[case::sequence_where_object_expected(
        object_schema_with_props(vec![("config", JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        })]),
        "config:\n  - item"
    )]
    #[case::null_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: ~"
    )]
    fn type_mismatch_produces_schematype_error(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 8: message names the expected type
    #[test]
    fn type_mismatch_message_names_expected_type() {
        let schema = object_schema_with_props(vec![("count", integer_schema())]);
        let docs = parse_docs("count: \"hello\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Tests 7, 14–18: correct type → no diagnostics
    #[rstest]
    #[case::string_value_matches_string_type(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: Alice"
    )]
    #[case::null_in_type_array_accepts_null(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Multiple(vec![
                "string".to_string(),
                "null".to_string(),
            ])),
            ..JsonSchema::default()
        })]),
        "name: ~"
    )]
    #[case::no_type_specified_accepts_any(
        object_schema_with_props(vec![("name", JsonSchema::default())]),
        "name: 42"
    )]
    #[case::integer_value_matches_integer_type(
        object_schema_with_props(vec![("port", integer_schema())]),
        "port: 8080"
    )]
    #[case::boolean_value_matches_boolean_type(
        object_schema_with_props(vec![("enabled", JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            ..JsonSchema::default()
        })]),
        "enabled: true"
    )]
    #[case::sequence_value_matches_array_type(
        object_schema_with_props(vec![("items", JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            ..JsonSchema::default()
        })]),
        "items:\n  - one\n  - two"
    )]
    fn type_match_produces_no_diagnostics(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }
}
