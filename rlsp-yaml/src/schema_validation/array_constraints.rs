// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::lsp_util::span_to_lsp;
use crate::schema::{AdditionalProperties, JsonSchema};

use super::context::Ctx;
use super::support::{
    collect_evaluated_item_count, format_path, make_diagnostic, node_loc, yaml_to_json,
};
use super::validate_node;

pub(super) fn validate_unevaluated_items(
    seq: &[Node<Span>],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let evaluated_count = collect_evaluated_item_count(schema);
    let Some(unevaluated_schema) = &schema.unevaluated_items else {
        return;
    };
    for (i, item) in seq.iter().enumerate() {
        if evaluated_count == usize::MAX || i < evaluated_count {
            continue;
        }
        let mut item_path = path.to_vec();
        item_path.push(format!("[{i}]"));
        validate_node(item, unevaluated_schema, &item_path, ctx, depth + 1);
    }
}

pub(super) fn validate_sequence(
    seq: &[Node<Span>],
    seq_loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    // prefixItems — validate each element at its positional schema
    let prefix_len = schema.prefix_items.as_ref().map_or(0, Vec::len);
    if let Some(prefix_schemas) = &schema.prefix_items {
        for (i, (item, item_schema)) in seq.iter().zip(prefix_schemas.iter()).enumerate() {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            validate_node(item, item_schema, &item_path, ctx, depth + 1);
        }
    }
    // items — applies to elements beyond prefixItems
    if let Some(items_schema) = &schema.items {
        for (i, item) in seq.iter().enumerate().skip(prefix_len) {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            validate_node(item, items_schema, &item_path, ctx, depth + 1);
        }
    } else if let Some(additional_items) = &schema.additional_items {
        // additionalItems (Draft-04/07) — applies to elements beyond the tuple prefix
        for (i, item) in seq.iter().enumerate().skip(prefix_len) {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            match additional_items {
                AdditionalProperties::Denied => {
                    let range = span_to_lsp(node_loc(item), ctx.idx);
                    ctx.diagnostics.push(make_diagnostic(
                        range,
                        DiagnosticSeverity::WARNING,
                        "schemaAdditionalProperties",
                        format!(
                            "Additional item at {}[{i}] is not allowed",
                            format_path(path)
                        ),
                    ));
                }
                AdditionalProperties::Schema(extra_schema) => {
                    validate_node(item, extra_schema, &item_path, ctx, depth + 1);
                }
            }
        }
    }
    validate_array_constraints(seq, seq_loc, schema, path, ctx, depth);
}

pub(super) fn validate_array_constraints(
    seq: &[Node<Span>],
    seq_loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let len = seq.len() as u64;

    if let Some(min) = schema.min_items
        && len < min
    {
        let range = span_to_lsp(seq_loc, ctx.idx);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaMinItems",
            format!(
                "Array at {} has {} items, minimum is {}",
                format_path(path),
                len,
                min
            ),
        ));
    }

    if let Some(max) = schema.max_items
        && len > max
    {
        let range = span_to_lsp(seq_loc, ctx.idx);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaMaxItems",
            format!(
                "Array at {} has {} items, maximum is {}",
                format_path(path),
                len,
                max
            ),
        ));
    }

    if schema.unique_items == Some(true) {
        let json_items: Vec<serde_json::Value> = seq.iter().filter_map(yaml_to_json).collect();
        let has_duplicate = json_items.iter().enumerate().any(|(i, a)| {
            json_items
                .get(..i)
                .is_some_and(|prev| prev.iter().any(|b| a == b))
        });
        if has_duplicate {
            let range = span_to_lsp(seq_loc, ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaUniqueItems",
                format!("Array at {} contains duplicate items", format_path(path)),
            ));
        }
    }

    if let Some(contains_schema) = &schema.contains {
        validate_contains(seq, seq_loc, contains_schema, schema, path, ctx, depth);
    }
}

pub(super) fn validate_contains(
    seq: &[Node<Span>],
    seq_loc: Span,
    contains_schema: &JsonSchema,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let format_validation = ctx.format_validation;
    let yaml_version = ctx.yaml_version;
    let match_count = seq
        .iter()
        .filter(|item| {
            let mut scratch = Vec::new();
            let mut probe = Ctx::new(&mut scratch, format_validation, yaml_version, ctx.idx);
            validate_node(item, contains_schema, path, &mut probe, depth + 1);
            scratch.is_empty()
        })
        .count() as u64;

    // Default min is 1 when `contains` is present without `minContains`
    let effective_min = schema.min_contains.unwrap_or(1);

    if match_count < effective_min {
        let range = span_to_lsp(seq_loc, ctx.idx);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaContains",
            format!(
                "Array at {} must contain at least {} item(s) matching the schema, found {}",
                format_path(path),
                effective_min,
                match_count
            ),
        ));
    }

    if let Some(max) = schema.max_contains
        && match_count > max
    {
        let range = span_to_lsp(seq_loc, ctx.idx);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaContains",
            format!(
                "Array at {} must contain at most {} item(s) matching the schema, found {}",
                format_path(path),
                max,
                match_count
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{DiagnosticSeverity, Position};

    use crate::schema::parse_schema;
    use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};
    use crate::server::YamlVersion;
    use crate::test_utils::parse_docs;
    use rlsp_yaml_parser::node::Node;
    use serde_json::json;

    use super::super::support::test_fixtures::{
        code_of, integer_schema, object_schema_with_props, string_schema,
    };
    use super::super::validate_schema;

    #[test]
    fn should_validate_array_items_against_items_schema() {
        let schema = object_schema_with_props(vec![(
            "ports",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(integer_schema())),
                ..JsonSchema::default()
            },
        )]);
        let text = "ports:\n  - 8080\n  - not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    #[test]
    fn should_produce_no_diagnostics_for_valid_array_items() {
        let schema = object_schema_with_props(vec![(
            "ports",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(integer_schema())),
                ..JsonSchema::default()
            },
        )]);
        let text = "ports:\n  - 8080\n  - 9090";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
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

    #[test]
    fn should_produce_no_diagnostics_when_array_has_one_matching_item_no_min_max() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, None))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_diagnostic_when_no_items_match_contains_schema() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, None))]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at least 1"));
    }

    #[test]
    fn should_produce_diagnostic_when_min_contains_not_met() {
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(2), None))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at least 2"));
    }

    #[test]
    fn should_produce_no_diagnostics_when_min_contains_met() {
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(2), None))]);
        let docs = parse_docs("items:\n  - 1\n  - 2");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_diagnostic_when_max_contains_exceeded() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, Some(1)))]);
        let docs = parse_docs("items:\n  - 1\n  - 2");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at most 1"));
    }

    #[test]
    fn should_produce_no_diagnostics_when_max_contains_not_exceeded() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, Some(1)))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    #[test]
    fn should_produce_no_diagnostics_when_min_contains_zero() {
        // minContains: 0 disables the "at least one" requirement
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(0), None))]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

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

    // Draft-04 array-form items parsed as prefixItems
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

    // prefixItems takes precedence over array-form items
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

    // ── unevaluatedItems ─────────────────────────────────────────────────────

    // unevaluatedItems with prefixItems — prefix items are evaluated (pass)
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

    // unevaluatedItems — item beyond prefix not evaluated (diagnostic)
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
        assert_eq!(code_of(&result[0]), "schemaAdditionalProperties");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        let msg = &result[0].message;
        assert!(
            msg.contains("[1]"),
            "message should reference [1], got: {msg}"
        );
    }

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
        assert!(
            result
                .iter()
                .all(|d| code_of(d) == "schemaAdditionalProperties")
        );
    }

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

    #[test]
    fn should_produce_no_diagnostics_when_additional_items_absent_and_extra_items_present() {
        let schema = tuple_schema_with_additional_items(vec![string_schema()], None);
        let text = "- hello\n- 42\n- extra";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }

    // ── Diagnostic range — array constraints ────────────────────────────────

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
            super::super::support::node_loc(v)
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
}
