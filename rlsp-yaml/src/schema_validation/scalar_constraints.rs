// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::LineIndex;
use rlsp_yaml_parser::ScalarStyle;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::lsp_util::span_to_lsp;
use crate::scalar_helpers;
use crate::schema::JsonSchema;
use crate::server::YamlVersion;

use super::context::Ctx;
use super::formats;
use super::support::{
    MAX_PATTERN_LEN, format_path, get_regex, make_diagnostic, node_loc, yaml_to_json,
};
use super::validate_node;

pub(super) fn validate_scalar_constraints(
    node: &Node<Span>,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    if let Node::Scalar {
        value,
        style,
        tag,
        loc,
        ..
    } = node
    {
        let is_plain = matches!(style, ScalarStyle::Plain);

        // String constraints apply to scalars that resolve to string type.
        // A scalar is a string when its resolved tag is str (or unrecognised).
        if tag.as_deref() == Some("tag:yaml.org,2002:str") {
            validate_string_constraints(value, *loc, schema, path, ctx);
        }

        // Numeric constraints only apply to plain scalars.
        if is_plain {
            let numeric_val = scalar_helpers::parse_integer(value)
                .map(|i| {
                    #[expect(clippy::cast_precision_loss, reason = "integer-to-f64 for numeric comparison; precision loss acceptable here")]
                    {
                        i as f64
                    }
                })
                .or_else(|| scalar_helpers::parse_float(value));
            if let Some(val) = numeric_val {
                validate_numeric_constraints(val, *loc, schema, path, ctx);
            }
        }
    }

    // const — compare any scalar node via yaml_to_json
    if let Some(const_val) = &schema.const_value {
        if let Some(yaml_val) = yaml_to_json(node) {
            if yaml_val != *const_val {
                let range = span_to_lsp(node_loc(node), ctx.idx);
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::ERROR,
                    "schemaConst",
                    format!("Value at {} must equal {}", format_path(path), const_val),
                ));
            }
        }
        // If yaml_to_json returns None (object/array), skip the check
    }
}

pub(super) fn validate_string_constraints(
    s: &str,
    loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    let range = span_to_lsp(loc, ctx.idx);
    if let Some(pattern) = &schema.pattern {
        if pattern.len() > MAX_PATTERN_LEN {
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaPatternLimit",
                format!(
                    "Pattern at {} exceeds maximum length ({MAX_PATTERN_LEN} chars) and was not validated",
                    format_path(path),
                ),
            ));
        } else if let Some(re) = get_regex(pattern) {
            if !re.is_match(s) {
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::ERROR,
                    "schemaPattern",
                    format!(
                        "Value at {} does not match pattern: {}",
                        format_path(path),
                        pattern
                    ),
                ));
            }
        } else {
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

    let char_count = s.chars().count() as u64;

    if let Some(min_len) = schema.min_length {
        if char_count < min_len {
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinLength",
                format!(
                    "Value at {} is too short: {} chars (minimum {})",
                    format_path(path),
                    char_count,
                    min_len
                ),
            ));
        }
    }

    if let Some(max_len) = schema.max_length {
        if char_count > max_len {
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaxLength",
                format!(
                    "Value at {} is too long: {} chars (maximum {})",
                    format_path(path),
                    char_count,
                    max_len
                ),
            ));
        }
    }

    if ctx.format_validation {
        if let Some(format) = &schema.format {
            validate_format(s, format, loc, path, ctx.diagnostics, ctx.idx);
        }
        if schema.content_encoding.is_some()
            || schema.content_media_type.is_some()
            || schema.content_schema.is_some()
        {
            validate_content(s, schema, loc, path, ctx.diagnostics, ctx.idx);
        }
    }
}

/// Check `s` against the JSON Schema `format` keyword and push a WARNING
/// diagnostic if the value does not conform.  Unknown formats are silently
/// ignored (per the spec, format validation is advisory).
pub(super) fn validate_format(
    s: &str,
    format: &str,
    loc: Span,
    path: &[String],
    diagnostics: &mut Vec<Diagnostic>,
    idx: &LineIndex,
) {
    let valid = match format {
        "date-time" => formats::is_valid_date_time(s),
        "date" => formats::is_valid_date(s),
        "time" => formats::is_valid_time(s),
        "duration" => formats::is_valid_duration(s),
        "email" => formats::is_valid_email(s),
        "ipv4" => formats::is_valid_ipv4(s),
        "ipv6" => formats::is_valid_ipv6(s),
        "hostname" => formats::is_valid_hostname(s),
        "uri" => formats::is_valid_uri(s),
        "uri-reference" => formats::is_valid_uri_reference(s),
        "uri-template" => formats::is_valid_uri_template(s),
        "uuid" => formats::is_valid_uuid(s),
        "regex" => formats::is_valid_regex(s),
        "json-pointer" => formats::is_valid_json_pointer(s),
        "relative-json-pointer" => formats::is_valid_relative_json_pointer(s),
        "idn-hostname" => formats::is_valid_idn_hostname(s),
        "idn-email" => formats::is_valid_idn_email(s),
        "iri" => formats::is_valid_iri(s),
        "iri-reference" => formats::is_valid_iri_reference(s),
        // Unknown formats are intentionally ignored
        _ => return,
    };
    if !valid {
        diagnostics.push(make_diagnostic(
            span_to_lsp(loc, idx),
            DiagnosticSeverity::WARNING,
            "schemaFormat",
            format!(
                "String at {} does not match format '{format}'",
                format_path(path)
            ),
        ));
    }
}

/// Validates `contentEncoding`, `contentMediaType`, and `contentSchema` keywords.
///
/// Decodes the string using the declared encoding, then (if set) checks the
/// decoded bytes against the declared media type. Finally, if `contentSchema`
/// is present, parses the (possibly decoded) content as YAML and validates
/// the result against the sub-schema.
pub(super) fn validate_content(
    s: &str,
    schema: &JsonSchema,
    loc: Span,
    path: &[String],
    diagnostics: &mut Vec<Diagnostic>,
    idx: &LineIndex,
) {
    // Step 1: decode if contentEncoding is set
    let decoded_bytes: Option<Vec<u8>> = if let Some(enc) = &schema.content_encoding {
        let result = match enc.as_str() {
            "base64" => data_encoding::BASE64.decode(s.as_bytes()),
            "base64url" => data_encoding::BASE64URL.decode(s.as_bytes()),
            "base32" => data_encoding::BASE32.decode(s.as_bytes()),
            "base16" => data_encoding::HEXUPPER_PERMISSIVE.decode(s.as_bytes()),
            // Unknown encoding — skip all checks
            _ => return,
        };
        if let Ok(bytes) = result {
            Some(bytes)
        } else {
            diagnostics.push(make_diagnostic(
                span_to_lsp(loc, idx),
                DiagnosticSeverity::WARNING,
                "schemaContentEncoding",
                format!(
                    "String at {} is not valid {enc} encoded data",
                    format_path(path)
                ),
            ));
            // Encoding failed — skip media type and schema checks
            return;
        }
    } else {
        // No encoding set — use raw string bytes for media type check
        None
    };

    // Step 2: check media type if set
    if let Some(media_type) = &schema.content_media_type {
        if media_type == "application/json" {
            let text = decoded_bytes
                .as_ref()
                .map_or(Some(s), |bytes| std::str::from_utf8(bytes).ok());
            let valid = text.is_some_and(|t| serde_json::from_str::<serde_json::Value>(t).is_ok());
            if !valid {
                diagnostics.push(make_diagnostic(
                    span_to_lsp(loc, idx),
                    DiagnosticSeverity::WARNING,
                    "schemaContentMediaType",
                    format!(
                        "String at {} does not contain valid {media_type} content",
                        format_path(path)
                    ),
                ));
                // Media type check failed — skip contentSchema validation
                return;
            }
        }
        // Unknown media type — fall through to contentSchema if present
    }

    // Step 3: validate decoded content against contentSchema if present
    validate_content_schema(
        s,
        decoded_bytes.as_deref(),
        schema,
        loc,
        path,
        diagnostics,
        idx,
    );
}

/// If `contentSchema` is present, parse the (possibly decoded) content as YAML
/// and validate the parsed result against the sub-schema.
pub(super) fn validate_content_schema(
    raw: &str,
    decoded_bytes: Option<&[u8]>,
    schema: &JsonSchema,
    loc: Span,
    path: &[String],
    diagnostics: &mut Vec<Diagnostic>,
    idx: &LineIndex,
) {
    let Some(content_schema) = &schema.content_schema else {
        return;
    };

    // Determine the text to parse: decoded bytes (as UTF-8) or raw string.
    let content_text = decoded_bytes
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or(raw);

    // Parse the content as YAML.
    let Ok(docs) = rlsp_yaml_parser::load(content_text) else {
        diagnostics.push(make_diagnostic(
            span_to_lsp(loc, idx),
            DiagnosticSeverity::WARNING,
            "schemaContentSchema",
            format!(
                "Decoded content at {} could not be parsed as YAML",
                format_path(path)
            ),
        ));
        return;
    };

    // Validate each parsed document against the content schema.
    // Content schemas validate embedded content — 1.1 compat warnings are
    // not applicable here, so always use V1_2.
    for doc in &docs {
        let mut content_path = path.to_vec();
        content_path.push("(content)".to_string());
        let mut ctx = Ctx::new(diagnostics, true, YamlVersion::V1_2, doc.line_index());
        validate_node(&doc.root, content_schema, &content_path, &mut ctx, 0);
    }
}

pub(super) fn validate_numeric_constraints(
    val: f64,
    loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    let range = span_to_lsp(loc, ctx.idx);
    // minimum (inclusive by default; strict if Draft-04 exclusiveMinimum is true)
    if let Some(minimum) = schema.minimum {
        let exclusive = schema.exclusive_minimum_draft04.unwrap_or(false);
        let violation = if exclusive {
            val <= minimum
        } else {
            val < minimum
        };
        if violation {
            let msg = if exclusive {
                format!(
                    "Value at {} is below exclusive minimum {minimum}",
                    format_path(path),
                )
            } else {
                format!("Value at {} is below minimum {minimum}", format_path(path))
            };
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinimum",
                msg,
            ));
        }
    }

    // maximum (inclusive by default; strict if Draft-04 exclusiveMaximum is true)
    if let Some(maximum) = schema.maximum {
        let exclusive = schema.exclusive_maximum_draft04.unwrap_or(false);
        let violation = if exclusive {
            val >= maximum
        } else {
            val > maximum
        };
        if violation {
            let msg = if exclusive {
                format!(
                    "Value at {} is above exclusive maximum {maximum}",
                    format_path(path),
                )
            } else {
                format!("Value at {} is above maximum {maximum}", format_path(path))
            };
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaximum",
                msg,
            ));
        }
    }

    // exclusiveMinimum (Draft-06+ number form)
    if let Some(excl_min) = schema.exclusive_minimum {
        if val <= excl_min {
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinimum",
                format!(
                    "Value at {} is below exclusive minimum {excl_min}",
                    format_path(path),
                ),
            ));
        }
    }

    // exclusiveMaximum (Draft-06+ number form)
    if let Some(excl_max) = schema.exclusive_maximum {
        if val >= excl_max {
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaximum",
                format!(
                    "Value at {} is above exclusive maximum {excl_max}",
                    format_path(path),
                ),
            ));
        }
    }

    // multipleOf
    if let Some(multiple_of) = schema.multiple_of {
        if multiple_of > 0.0 {
            let quotient = val / multiple_of;
            if (quotient - quotient.round()).abs() >= f64::EPSILON {
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::ERROR,
                    "schemaMultipleOf",
                    format!(
                        "Value at {} must be a multiple of {multiple_of}",
                        format_path(path),
                    ),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    use crate::schema::{JsonSchema, SchemaType};
    use crate::server::YamlVersion;
    use crate::test_utils::parse_docs;
    use serde_json::json;

    use crate::schema_validation::support::test_fixtures::{
        code_of, object_schema_with_props, string_schema,
    };
    use crate::schema_validation::validate_schema;

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — pattern
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_produce_no_diagnostics_when_string_matches_pattern() {
        let schema = object_schema_with_props(vec![(
            "code",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                pattern: Some("^[A-Z]{3}$".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("code: ABC");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_error_when_string_does_not_match_pattern() {
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
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

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

    // Tests 70–71: rejected pattern → schemaPatternLimit WARNING
    #[rstest]
    #[case::pattern_exceeds_max_length("a".repeat(1025))]
    #[case::pattern_fails_to_compile("[invalid".to_string())]
    fn pattern_rejected_produces_schemapatternlimit_warning(#[case] pattern: String) {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                pattern: Some(pattern),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: anything");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPatternLimit");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — minLength / maxLength
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 72, 74: string meets length constraint → no diagnostics
    #[rstest]
    #[case::string_meets_min_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            min_length: Some(3),
            ..JsonSchema::default()
        })]),
        "name: abc"
    )]
    #[case::string_meets_max_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            max_length: Some(10),
            ..JsonSchema::default()
        })]),
        "name: hello"
    )]
    fn string_length_constraint_valid_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Tests 73, 75: string violates length constraint → error with specific code
    #[rstest]
    #[case::string_shorter_than_min_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            min_length: Some(5),
            ..JsonSchema::default()
        })]),
        "name: hi",
        "schemaMinLength"
    )]
    #[case::string_exceeds_max_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            max_length: Some(3),
            ..JsonSchema::default()
        })]),
        "name: toolong",
        "schemaMaxLength"
    )]
    fn string_length_constraint_violated_produces_error(
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

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — minimum / maximum (inclusive)
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 76, 78: integer meets inclusive bound → no diagnostics
    #[rstest]
    #[case::integer_meets_minimum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            minimum: Some(1.0),
            ..JsonSchema::default()
        })]),
        "port: 80"
    )]
    #[case::integer_meets_maximum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            maximum: Some(65535.0),
            ..JsonSchema::default()
        })]),
        "port: 8080"
    )]
    fn numeric_inclusive_bound_valid_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // Tests 77, 79: integer violates inclusive bound → error with specific code
    #[rstest]
    #[case::integer_below_minimum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            minimum: Some(1.0),
            ..JsonSchema::default()
        })]),
        "port: 0",
        "schemaMinimum"
    )]
    #[case::integer_exceeds_maximum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            maximum: Some(65535.0),
            ..JsonSchema::default()
        })]),
        "port: 99999",
        "schemaMaximum"
    )]
    fn numeric_inclusive_bound_violated_produces_error(
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

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — Draft-04 exclusiveMinimum / exclusiveMaximum (bool)
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 80, 82: Draft-04 exclusiveMinimum/Maximum=true at boundary → error
    #[rstest]
    #[case::value_equals_exclusive_minimum(
        object_schema_with_props(vec![("val", JsonSchema {
            minimum: Some(5.0),
            exclusive_minimum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "schemaMinimum"
    )]
    #[case::value_equals_exclusive_maximum(
        object_schema_with_props(vec![("val", JsonSchema {
            maximum: Some(10.0),
            exclusive_maximum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "schemaMaximum"
    )]
    fn draft04_exclusive_bound_at_boundary_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
    }

    // exclusive=false at boundary → no error (unique schema combination)
    #[test]
    fn should_produce_no_diagnostics_when_value_equals_minimum_and_not_exclusive_draft04() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                minimum: Some(5.0),
                exclusive_minimum_draft04: Some(false),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 5");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — Draft-06+ exclusiveMinimum / exclusiveMaximum (f64)
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 83, 85: Draft-06 exclusive bound at boundary → error
    #[rstest]
    #[case::value_equals_exclusive_minimum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_minimum: Some(5.0),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "schemaMinimum"
    )]
    #[case::value_equals_exclusive_maximum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_maximum: Some(10.0),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "schemaMaximum"
    )]
    fn draft06_exclusive_bound_at_boundary_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
    }

    // Tests 84, 86: Draft-06 exclusive bound past boundary → no diagnostics
    #[rstest]
    #[case::value_exceeds_exclusive_minimum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_minimum: Some(5.0),
            ..JsonSchema::default()
        })]),
        "val: 6"
    )]
    #[case::value_below_exclusive_maximum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_maximum: Some(10.0),
            ..JsonSchema::default()
        })]),
        "val: 9"
    )]
    fn draft06_exclusive_bound_past_boundary_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — multipleOf
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_produce_no_diagnostics_when_value_is_multiple_of() {
        let schema = object_schema_with_props(vec![(
            "count",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                multiple_of: Some(5.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("count: 15");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_error_when_value_is_not_multiple_of() {
        let schema = object_schema_with_props(vec![(
            "count",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                multiple_of: Some(5.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("count: 7");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMultipleOf");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — const
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 89, 91: value equals const → no diagnostics
    #[rstest]
    #[case::string_value_equals_const(
        object_schema_with_props(vec![("version", JsonSchema {
            const_value: Some(json!("v1")),
            ..JsonSchema::default()
        })]),
        "version: v1"
    )]
    #[case::integer_value_equals_const(
        object_schema_with_props(vec![("level", JsonSchema {
            const_value: Some(json!(42)),
            ..JsonSchema::default()
        })]),
        "level: 42"
    )]
    fn const_match_produces_no_diagnostics(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // const mismatch → schemaConst ERROR (standalone)
    #[test]
    fn should_produce_error_when_value_does_not_equal_const() {
        let schema = object_schema_with_props(vec![(
            "version",
            JsonSchema {
                const_value: Some(json!("v1")),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("version: v2");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaConst");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn should_skip_const_check_for_mapping_node() {
        let schema = object_schema_with_props(vec![(
            "obj",
            JsonSchema {
                const_value: Some(json!({"key": "val"})),
                ..JsonSchema::default()
            },
        )]);
        let text = "obj:\n  key: other";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        // yaml_to_json returns None for mappings — const check skipped
        assert!(result.iter().all(|d| code_of(d) != "schemaConst"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group 2: validate_scalar_constraints — string constraints gate
    // ══════════════════════════════════════════════════════════════════════════

    fn min_length_schema(min: u64) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            min_length: Some(min),
            ..JsonSchema::default()
        }
    }

    // plain string scalar applies string constraints
    #[test]
    fn tag_driven_string_scalar_applies_min_length() {
        let schema = object_schema_with_props(vec![("value", min_length_schema(10))]);
        let docs = parse_docs("value: hi");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaMinLength"),
            "string scalar must have minLength applied"
        );
    }

    // plain null scalar skips string constraints
    #[test]
    fn tag_driven_null_scalar_skips_min_length() {
        let schema = object_schema_with_props(vec![("value", min_length_schema(10))]);
        let docs = parse_docs("value: ~");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().all(|d| code_of(d) != "schemaMinLength"),
            "null scalar must not have string constraints applied"
        );
    }

    // plain bool scalar skips string constraints
    #[test]
    fn tag_driven_bool_scalar_skips_min_length() {
        let schema = object_schema_with_props(vec![("value", min_length_schema(10))]);
        let docs = parse_docs("value: true");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().all(|d| code_of(d) != "schemaMinLength"),
            "bool scalar must not have string constraints applied"
        );
    }

    // plain integer scalar skips string constraints
    #[test]
    fn tag_driven_integer_scalar_skips_min_length() {
        let schema = object_schema_with_props(vec![("value", min_length_schema(10))]);
        let docs = parse_docs("value: 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().all(|d| code_of(d) != "schemaMinLength"),
            "integer scalar must not have string constraints applied"
        );
    }

    // double-quoted scalar applies string constraints
    #[test]
    fn tag_driven_quoted_scalar_applies_min_length() {
        let schema = object_schema_with_props(vec![("value", min_length_schema(10))]);
        let docs = parse_docs("value: \"hi\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaMinLength"),
            "quoted scalar is always a string — minLength must apply"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group 3: yaml_to_json — tag-URI-driven JSON conversion for const/enum
    // ══════════════════════════════════════════════════════════════════════════

    fn const_schema(val: serde_json::Value) -> JsonSchema {
        JsonSchema {
            const_value: Some(val),
            ..JsonSchema::default()
        }
    }

    // null-tagged scalar converts to JSON null
    #[test]
    fn tag_driven_null_tagged_scalar_matches_const_null() {
        let schema = object_schema_with_props(vec![("value", const_schema(json!(null)))]);
        let docs = parse_docs("value: ~");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty(), "null scalar must match const: null");
    }

    // bool-tagged scalar converts to correct JSON bool (true)
    #[test]
    fn tag_driven_true_bool_matches_const_true() {
        let schema = object_schema_with_props(vec![("value", const_schema(json!(true)))]);
        let docs = parse_docs("value: true");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty(), "true scalar must match const: true");
    }

    // bool-tagged scalar (false) does not match const: true
    #[test]
    fn tag_driven_false_bool_does_not_match_const_true() {
        let schema = object_schema_with_props(vec![("value", const_schema(json!(true)))]);
        let docs = parse_docs("value: false");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaConst"),
            "false scalar must not match const: true"
        );
    }

    // integer-tagged scalar converts to JSON number
    #[test]
    fn tag_driven_integer_scalar_matches_const_number() {
        let schema = object_schema_with_props(vec![("value", const_schema(json!(42)))]);
        let docs = parse_docs("value: 42");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty(), "integer 42 must match const: 42");
    }

    // quoted scalar whose content looks like null is a JSON string, not null
    #[test]
    fn tag_driven_quoted_null_looking_scalar_is_string_not_null() {
        let schema = object_schema_with_props(vec![("value", const_schema(json!(null)))]);
        let docs = parse_docs("value: \"~\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaConst"),
            "quoted '~' is a string, not null — must not match const: null"
        );
    }

    // quoted scalar whose content looks like bool is a JSON string
    #[test]
    fn tag_driven_quoted_bool_looking_scalar_is_string_not_bool() {
        let schema = object_schema_with_props(vec![("value", const_schema(json!(true)))]);
        let docs = parse_docs("value: \"true\"");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaConst"),
            "quoted 'true' is a string, not bool — must not match const: true"
        );
    }

    // ── contentEncoding / contentMediaType ──────────────────────────────────

    fn content_schema_helper(encoding: Option<&str>, media_type: Option<&str>) -> JsonSchema {
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
    ) -> Vec<tower_lsp::lsp_types::Diagnostic> {
        let schema = content_schema_helper(encoding, media_type);
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

    // contentEncoding unknown: silently ignored
    #[test]
    fn content_encoding_unknown_ignored() {
        assert!(run_content("anything", Some("base58"), None).is_empty());
    }

    // contentMediaType application/json: valid (no encoding)
    // The value must be a YAML string (quoted) so it reaches validate_string_constraints.
    // Values starting with { or [ are YAML flow collections; use quoted YAML.
    #[test]
    fn content_media_type_json_valid_no_encoding() {
        // "\"42\"" parses as YAML string "42", which is valid JSON
        let schema = content_schema_helper(None, Some("application/json"));
        let docs = parse_docs("\"42\"");
        assert!(validate_schema(&docs, &schema, true, YamlVersion::V1_2).is_empty());
    }

    // contentMediaType application/json: invalid (no encoding)
    #[test]
    fn content_media_type_json_invalid_no_encoding() {
        assert_eq!(
            run_content("not json", None, Some("application/json")).len(),
            1
        );
    }

    // contentEncoding + contentMediaType: valid base64-encoded JSON
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

    // contentEncoding + contentMediaType: encoding fails → only encoding diagnostic
    #[test]
    fn content_encoding_fails_skips_media_type_check() {
        use tower_lsp::lsp_types::NumberOrString;
        let diags = run_content(
            "not-valid-base64!!!",
            Some("base64"),
            Some("application/json"),
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].code == Some(NumberOrString::String("schemaContentEncoding".to_string())));
    }

    // contentEncoding + contentMediaType: valid encoding but invalid JSON
    #[test]
    fn content_encoding_valid_media_type_invalid() {
        use tower_lsp::lsp_types::NumberOrString;
        // base64("not json") = "bm90IGpzb24="
        let diags = run_content("bm90IGpzb24=", Some("base64"), Some("application/json"));
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].code == Some(NumberOrString::String("schemaContentMediaType".to_string()))
        );
    }

    // contentMediaType unknown: silently ignored
    #[test]
    fn content_media_type_unknown_ignored() {
        assert!(run_content("anything", None, Some("text/plain")).is_empty());
    }

    // format_validation disabled: content checks also skipped
    #[test]
    fn content_validation_disabled_when_format_validation_off() {
        let schema = content_schema_helper(Some("base64"), Some("application/json"));
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

    // ── Diagnostic range — scalar constraints ────────────────────────────────

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
}
