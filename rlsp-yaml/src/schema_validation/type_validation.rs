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
        boolean_schema, code_of, integer_schema, object_schema_with_props, string_schema,
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

    // message names the expected type
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

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — type mismatch format
    // ══════════════════════════════════════════════════════════════════════════

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

    // ── Diagnostic range — type validation ──────────────────────────────────

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

    // ══════════════════════════════════════════════════════════════════════════
    // Group 1: yaml_type_name — tag-URI-driven classification
    // ══════════════════════════════════════════════════════════════════════════

    // plain null tagged as null → "null"
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

    // plain bool tagged as bool → "boolean"
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

    // plain integer tagged as int → "integer"
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

    // plain float tagged as float → "number"
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

    // plain string tagged as str → "string"
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

    // double-quoted scalar → always "string" regardless of content
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

    // explicit !!bool on an otherwise-string value → "boolean"
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

    // explicit !!str on a value that looks like a number → "string"
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
