// SPDX-License-Identifier: MIT

//! The boolean schema `false`: no value satisfies it, wherever it stands.

use rstest::rstest;
use serde_json::{Value, json};

use super::support::test_fixtures::code_of;
use super::*;
use crate::schema::parse_schema;
use crate::schema::test_support::behind_a_reference;
use crate::test_utils::parse_docs;

fn parsed(schema: &Value) -> ParsedSchema {
    parse_schema(schema).expect("the schema parses")
}

fn diagnostics_of(schema: &Value, text: &str) -> Vec<Diagnostic> {
    validate_schema(&parse_docs(text), &parsed(schema), true, YamlVersion::V1_2).diagnostics
}

/// Where a diagnostic is, what it is, and a part of its text.
type Found = (u32, u32, &'static str, &'static str);

fn not_allowed(path: &str) -> String {
    format!("Value at {path} is not allowed by the schema")
}

/// The diagnostics of `text`, ordered by place and code, as `(line, character,
/// code, message)`.
fn found(schema: &Value, text: &str) -> Vec<(u32, u32, String, String)> {
    let mut found: Vec<_> = diagnostics_of(schema, text)
        .into_iter()
        .map(|d| {
            (
                d.range.start.line,
                d.range.start.character,
                code_of(&d).to_string(),
                d.message,
            )
        })
        .collect();
    found.sort();
    found
}

fn assert_found(schema: &Value, text: &str, expected: &[Found]) {
    let actual = found(schema, text);
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            (actual.0, actual.1, actual.2.as_str()),
            (expected.0, expected.1, expected.2),
            "{actual:?}"
        );
        assert!(
            actual.3.contains(expected.3),
            "'{}' does not contain '{}'",
            actual.3,
            expected.3
        );
    }
}

const NA: &str = "schemaNotAllowed";

#[test]
fn a_false_schema_is_reported_as_a_warning_with_the_documented_message() {
    let schema = json!({ "properties": { "x": false } });

    let diagnostics = diagnostics_of(&schema, "x: 1");

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(code_of(&diagnostics[0]), NA);
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(diagnostics[0].message, not_allowed("x"));
    assert_eq!(diagnostics[0].source.as_deref(), Some("rlsp-yaml"));
}

// ── Where a value meets `false` ──────────────────────────────────────────────

#[rstest]
#[case::items_beyond_the_prefix(
    json!({ "properties": { "xs": { "prefixItems": [{ "type": "string" }], "items": false } } }),
    "xs:\n  - a\n  - b\n  - c\n",
    &[(2, 4, NA, "Value at xs[1] is not allowed by the schema"),
      (3, 4, NA, "Value at xs[2] is not allowed by the schema")]
)]
#[case::items_without_a_prefix(
    json!({ "properties": { "xs": { "items": false } } }),
    "xs:\n  - a\n",
    &[(1, 4, NA, "Value at xs[0] is not allowed by the schema")]
)]
#[case::items_with_an_empty_array(
    json!({ "properties": { "xs": { "items": false } } }), "xs: []", &[]
)]
#[case::items_with_an_array_inside_the_prefix(
    json!({ "properties": { "xs": { "prefixItems": [true], "items": false } } }),
    "xs: [a]",
    &[]
)]
#[case::prefix_entry_and_the_entry_after_it(
    json!({ "prefixItems": [{ "type": "string" }, false, { "type": "integer" }] }),
    "- a\n- b\n- x\n",
    &[(1, 2, NA, "Value at [1] is not allowed by the schema"),
      (2, 2, "schemaType", "[2]")]
)]
#[case::tuple_entry_and_the_entry_after_it(
    json!({ "items": [{ "type": "string" }, false, { "type": "integer" }] }),
    "- a\n- b\n- x\n",
    &[(1, 2, NA, "Value at [1] is not allowed by the schema"),
      (2, 2, "schemaType", "[2]")]
)]
#[case::prefix_entry_beyond_a_short_array(
    json!({ "prefixItems": [{ "type": "string" }, false] }), "- a\n", &[]
)]
#[case::unevaluated_items(
    json!({ "prefixItems": [{ "type": "string" }], "unevaluatedItems": false }),
    "- a\n- b\n- c\n",
    &[(1, 2, NA, "Value at [1] is not allowed by the schema"),
      (2, 2, NA, "Value at [2] is not allowed by the schema")]
)]
#[case::unevaluated_items_beside_a_prefix_of_a_branch(
    json!({ "allOf": [{ "prefixItems": [{}] }], "unevaluatedItems": false }),
    "- a\n- b\n",
    &[(1, 2, NA, "Value at [1] is not allowed by the schema")]
)]
#[case::unevaluated_items_beside_items_of_a_branch(
    json!({ "allOf": [{ "items": {} }], "unevaluatedItems": false }),
    "- a\n- b\n",
    &[]
)]
#[case::items_false_evaluates_every_item(
    json!({ "items": false, "unevaluatedItems": false }),
    "- a\n",
    &[(0, 2, NA, "Value at [0] is not allowed by the schema")]
)]
#[case::a_false_prefix_entry_evaluates_its_item(
    json!({ "prefixItems": [false], "unevaluatedItems": false }),
    "- a\n- b\n",
    &[(0, 2, NA, "Value at [0] is not allowed by the schema"),
      (1, 2, NA, "Value at [1] is not allowed by the schema")]
)]
fn a_false_schema_for_items_is_reported_at_the_item(
    #[case] schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    assert_found(&schema, text, expected);
}

#[rstest]
#[case::scalar(json!({ "properties": { "ok": { "type": "string" }, "forbidden": false } }),
    "ok: a\nforbidden: 1\n", &[(1, 11, NA, "Value at forbidden is not allowed by the schema")])]
#[case::mapping(json!({ "properties": { "forbidden": false } }),
    "forbidden:\n  a: 1\n", &[(1, 2, NA, "Value at forbidden is not allowed by the schema")])]
#[case::sequence(json!({ "properties": { "forbidden": false } }),
    "forbidden:\n  - 1\n", &[(1, 2, NA, "Value at forbidden is not allowed by the schema")])]
#[case::null(json!({ "properties": { "forbidden": false } }),
    "forbidden: ~\n", &[(0, 11, NA, "Value at forbidden is not allowed by the schema")])]
#[case::absent(json!({ "properties": { "forbidden": false } }), "other: 1\n", &[])]
#[case::beside_additional_properties_false(
    json!({ "properties": { "x": false }, "additionalProperties": false }),
    "x: 1\ny: 2\n",
    &[(0, 3, NA, "Value at x is not allowed by the schema"),
      (1, 0, "schemaAdditionalProperty", "Additional property 'y' is not allowed at <root>")]
)]
#[case::not_checked_against_the_additional_properties_schema(
    json!({ "properties": { "x": false }, "additionalProperties": { "type": "string" } }),
    "x: 1\n",
    &[(0, 3, NA, "Value at x is not allowed by the schema")]
)]
#[case::not_unevaluated(
    json!({ "properties": { "x": false }, "unevaluatedProperties": false }),
    "x: 1\ny: 2\n",
    &[(0, 3, NA, "Value at x is not allowed by the schema"),
      (1, 0, "schemaUnevaluatedProperty", "'y'")]
)]
#[case::declared_in_a_branch_is_evaluated(
    json!({ "allOf": [{ "properties": { "x": false } }], "unevaluatedProperties": false }),
    "x: 1\n",
    &[(0, 3, NA, "Value at x is not allowed by the schema")]
)]
#[case::required_and_absent(
    json!({ "properties": { "x": false }, "required": ["x"] }),
    "y: 1\n",
    &[(0, 0, "schemaRequired", "'x'")]
)]
fn a_false_property_is_reported_at_its_value(
    #[case] schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    assert_found(&schema, text, expected);
}

#[rstest]
#[case::matching_key(json!({ "patternProperties": { "^a": false } }),
    "a1: 1\nb: 2\n", &[(0, 4, NA, "Value at a1 is not allowed by the schema")])]
#[case::excludes_additional_properties_false(
    json!({ "patternProperties": { "^a": false }, "additionalProperties": false }),
    "a1: 1\nb: 2\n",
    &[(0, 4, NA, "Value at a1 is not allowed by the schema"),
      (1, 0, "schemaAdditionalProperty", "'b'")]
)]
#[case::excludes_the_additional_properties_schema(
    json!({ "patternProperties": { "^a": false }, "additionalProperties": { "type": "string" } }),
    "a1: 1\n",
    &[(0, 4, NA, "Value at a1 is not allowed by the schema")]
)]
#[case::beside_a_pattern_that_accepts(
    json!({ "patternProperties": { "^a": false, "^ab": { "type": "string" } } }),
    "abc: 1\n",
    &[(0, 5, NA, "Value at abc is not allowed by the schema"),
      (0, 5, "schemaType", "abc")]
)]
#[case::two_false_patterns(
    json!({ "patternProperties": { "^a": false, "^ab": false } }),
    "abc: 1\n",
    &[(0, 5, NA, "Value at abc is not allowed by the schema"),
      (0, 5, NA, "Value at abc is not allowed by the schema")]
)]
#[case::a_declared_property_is_not_asked(
    json!({ "properties": { "a1": true }, "patternProperties": { "^a": false } }),
    "a1: 1\n",
    &[]
)]
#[case::an_invalid_pattern_is_reported_as_such(
    json!({ "patternProperties": { "[invalid": false } }), "a: 1\n",
    &[(0, 3, "schemaPatternLimit", "could not be compiled")]
)]
#[case::evaluated_for_unevaluated_properties(
    json!({ "patternProperties": { "^a": false }, "unevaluatedProperties": false }),
    "a1: 1\nb: 2\n",
    &[(0, 4, NA, "Value at a1 is not allowed by the schema"),
      (1, 0, "schemaUnevaluatedProperty", "'b'")]
)]
fn a_false_pattern_schema_is_reported_at_the_value_of_each_key_it_matches(
    #[case] schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    assert_found(&schema, text, expected);
}

#[test]
fn a_pattern_that_exceeds_the_length_limit_is_reported_as_such_not_as_a_false_schema() {
    let long = "a".repeat(crate::schema::pattern::MAX_PATTERN_LEN + 1);
    let schema = json!({ "patternProperties": { long: false } });

    let found = found(&schema, "a: 1\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].2, "schemaPatternLimit");
}

#[rstest]
#[case::scalar(json!({ "allOf": [false] }), "x", &[(0, 0, NA, "Value at <root> is not allowed by the schema")])]
#[case::mapping(json!({ "allOf": [false] }), "a: 1", &[(0, 0, NA, "Value at <root> is not allowed by the schema")])]
#[case::sequence(json!({ "allOf": [false] }), "- 1", &[(0, 0, NA, "Value at <root> is not allowed by the schema")])]
#[case::next_to_a_failing_branch(
    json!({ "allOf": [{ "type": "integer" }, false] }),
    "x",
    &[(0, 0, NA, "Value at <root> is not allowed by the schema"), (0, 0, "schemaType", "<root>")]
)]
#[case::below_a_property(
    json!({ "properties": { "p": { "allOf": [false] } } }),
    "p: 1",
    &[(0, 3, NA, "Value at p is not allowed by the schema")]
)]
#[case::then_selected(
    json!({ "if": { "type": "string" }, "then": false }), "x",
    &[(0, 0, NA, "Value at <root> is not allowed by the schema")]
)]
#[case::then_not_selected(json!({ "if": { "type": "string" }, "then": false }), "1", &[])]
#[case::else_selected(
    json!({ "if": { "type": "string" }, "else": false }), "1",
    &[(0, 0, NA, "Value at <root> is not allowed by the schema")]
)]
#[case::else_not_selected(json!({ "if": { "type": "string" }, "else": false }), "x", &[])]
#[case::then_without_an_if(json!({ "then": false, "else": false }), "x", &[])]
#[case::if_false_applies_else(
    json!({ "if": false, "then": false, "else": { "type": "integer" } }), "1", &[]
)]
#[case::if_false_applies_else_false(
    json!({ "if": false, "else": false }), "1",
    &[(0, 0, NA, "Value at <root> is not allowed by the schema")]
)]
#[case::if_false_does_not_apply_then(json!({ "if": false, "then": false }), "1", &[])]
fn a_false_schema_for_a_node_is_reported_at_the_node(
    #[case] schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    assert_found(&schema, text, expected);
}

#[rstest]
#[case::each_key(json!({ "propertyNames": false }), "a: 1\nb: 2\n",
    &[(0, 0, NA, "Value at <root> is not allowed by the schema"),
      (1, 0, NA, "Value at <root> is not allowed by the schema")])]
#[case::below_a_property(
    json!({ "properties": { "outer": { "propertyNames": false } } }), "outer:\n  k: 1\n",
    &[(1, 2, NA, "Value at outer is not allowed by the schema")]
)]
#[case::an_empty_mapping(json!({ "propertyNames": false }), "{}", &[])]
#[case::in_a_later_document(json!({ "propertyNames": false }), "{}\n---\nk: 1\n",
    &[(2, 0, NA, "Value at <root> is not allowed by the schema")])]
fn a_false_schema_for_property_names_is_reported_at_each_key(
    #[case] schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    assert_found(&schema, text, expected);
}

#[rstest]
#[case::dependent_schemas(json!({ "dependentSchemas": { "a": false } }))]
#[case::dependencies(json!({ "dependencies": { "a": false } }))]
fn a_false_dependency_schema_is_reported_at_the_mapping_when_its_key_is_present(
    #[case] schema: Value,
) {
    assert_found(
        &schema,
        "a: 1\nb: 2\n",
        &[(0, 0, NA, "Value at <root> is not allowed by the schema")],
    );
    assert_found(&schema, "b: 2\n", &[]);
    assert_found(
        &schema,
        "b: 1\n---\nc: 2\na: 1\n",
        &[(2, 0, NA, "Value at <root> is not allowed by the schema")],
    );
    assert_found(
        &json!({ "properties": { "m": schema } }),
        "m:\n  a: 1\n",
        &[(1, 2, NA, "Value at m is not allowed by the schema")],
    );
}

#[test]
fn each_present_key_with_a_false_dependency_schema_is_reported() {
    let schema = json!({ "dependentSchemas": { "a": false, "b": false, "c": false } });

    assert_eq!(found(&schema, "a: 1\nb: 2\n").len(), 2);
}

#[test]
fn a_false_content_schema_is_reported_for_the_decoded_content() {
    let schema = json!({ "properties": { "x": { "contentSchema": false } } });

    assert_found(
        &schema,
        "x: \"hello\"\n",
        &[(
            0,
            0,
            NA,
            "Value at x.(content) is not allowed by the schema",
        )],
    );
}

#[test]
fn content_that_cannot_be_parsed_is_not_reported_as_a_false_schema() {
    let schema = json!({ "properties": { "x": { "contentSchema": false } } });

    let found = found(&schema, "x: \"a: [\"\n");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].2, "schemaContentSchema");
}

// ── Keywords that probe a sub-schema ─────────────────────────────────────────

#[rstest]
#[case::empty(json!({ "contains": false }), "[]")]
#[case::one_item(json!({ "contains": false }), "[1]")]
#[case::many_items(json!({ "contains": false }), "[1, 2, 3]")]
fn contains_false_reports_the_array_once(#[case] schema: Value, #[case] text: &str) {
    let found = found(&schema, text);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].2, "schemaContains");
    assert!(found[0].3.contains("found 0"), "{found:?}");
}

#[rstest]
#[case::min_contains_zero(json!({ "contains": false, "minContains": 0 }), "[1]")]
#[case::min_and_max_zero(json!({ "contains": false, "minContains": 0, "maxContains": 0 }), "[1]")]
#[case::not_an_array(json!({ "contains": false }), "x")]
fn contains_false_reports_nothing_where_no_match_is_needed(
    #[case] schema: Value,
    #[case] text: &str,
) {
    assert_found(&schema, text, &[]);
}

#[test]
fn contains_false_with_only_a_maximum_reports_the_minimum() {
    let found = found(&json!({ "contains": false, "maxContains": 0 }), "[1]");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].2, "schemaContains");
}

#[rstest]
#[case::any_of_accepted_by_the_other_branch(json!({ "anyOf": [false, { "type": "string" }] }), "x", &[])]
#[case::any_of_counts_the_false_branch(
    json!({ "anyOf": [false, { "type": "string" }] }), "1",
    &[(0, 0, "schemaType", "does not match any of the 2 allowed schemas (anyOf)")]
)]
#[case::any_of_only_false(
    json!({ "anyOf": [false] }), "1",
    &[(0, 0, "schemaType", "does not match any of the 1 allowed schemas (anyOf)")]
)]
#[case::one_of_accepted_by_the_other_branch(json!({ "oneOf": [false, { "type": "string" }] }), "x", &[])]
#[case::one_of_only_false(
    json!({ "oneOf": [false, false] }), "x",
    &[(0, 0, "schemaType", "does not match any of the 2 oneOf schemas")]
)]
#[case::one_of_counts_the_false_branch(
    json!({ "oneOf": [{ "type": "string" }, { "type": "string" }, false] }), "x",
    &[(0, 0, "schemaType", "matches 2 of the 3 oneOf schemas")]
)]
#[case::not_false_reports_nothing_for_a_scalar(json!({ "not": false }), "x", &[])]
#[case::not_false_reports_nothing_for_a_mapping(json!({ "not": false }), "a: 1", &[])]
#[case::not_false_reports_nothing_for_a_sequence(json!({ "not": false }), "- 1", &[])]
#[case::not_not_false(
    json!({ "not": { "not": false } }), "x",
    &[(0, 0, "schemaNot", "must not match")]
)]
#[case::a_branch_that_holds_a_false_property_fails_the_probe(
    json!({ "anyOf": [{ "properties": { "a": false } }, { "properties": { "b": false } }] }),
    "a: 1\nb: 2\n",
    &[(0, 0, "schemaType", "(anyOf)")]
)]
#[case::not_any_of_false(json!({ "not": { "anyOf": [false] } }), "x", &[])]
fn a_false_schema_in_a_probe_makes_it_fail_without_a_diagnostic_of_its_own(
    #[case] schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    let found = found(&schema, text);
    assert!(
        found.iter().all(|d| d.2 != NA),
        "a probe leaked the diagnostic: {found:?}"
    );
    assert_found(&schema, text, expected);
}

// ── The same wording and places as before ────────────────────────────────────

#[rstest]
#[case::additional_properties(
    json!({ "additionalProperties": false }), "z: 1\n",
    &[(0, 0, "schemaAdditionalProperty", "Additional property 'z' is not allowed at <root>")]
)]
#[case::additional_items(
    json!({ "items": [{}], "additionalItems": false }), "- 1\n- 2\n",
    &[(1, 2, "schemaAdditionalProperties", "Additional item at <root>[1] is not allowed")]
)]
#[case::unevaluated_properties(
    json!({ "unevaluatedProperties": false }), "z: 1\n",
    &[(0, 0, "schemaUnevaluatedProperty", "Unevaluated property 'z' is not allowed at <root>")]
)]
fn the_inline_denials_keep_their_diagnostic_and_wording(
    #[case] schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    assert_found(&schema, text, expected);
}

// ── References ───────────────────────────────────────────────────────────────

#[rstest]
#[case::items(json!({ "items": false }), "- a\n- b\n")]
#[case::prefix_items(json!({ "prefixItems": [{}, false] }), "- a\n- b\n")]
#[case::tuple_items(json!({ "items": [{}, false] }), "- a\n- b\n")]
#[case::unevaluated_items(json!({ "unevaluatedItems": false }), "- a\n- b\n")]
#[case::contains(json!({ "contains": false }), "- a\n- b\n")]
#[case::properties(json!({ "properties": { "a": false } }), "a: 1\n")]
#[case::pattern_properties(json!({ "patternProperties": { "^a": false } }), "a: 1\n")]
#[case::property_names(json!({ "propertyNames": false }), "a: 1\n")]
#[case::dependent_schemas(json!({ "dependentSchemas": { "a": false } }), "a: 1\n")]
#[case::dependencies(json!({ "dependencies": { "a": false } }), "a: 1\n")]
#[case::all_of(json!({ "allOf": [false] }), "a: 1\n")]
#[case::any_of(json!({ "anyOf": [false, { "type": "string" }] }), "a: 1\n")]
#[case::one_of(json!({ "oneOf": [false, false] }), "a: 1\n")]
#[case::not(json!({ "not": false }), "a: 1\n")]
#[case::if_schema(json!({ "if": false, "else": false }), "a: 1\n")]
#[case::then_schema(json!({ "if": {}, "then": false }), "a: 1\n")]
#[case::else_schema(json!({ "if": false, "else": false }), "a: 1\n")]
#[case::content_schema(
    json!({ "properties": { "a": { "contentSchema": false } } }), "a: \"hello\"\n"
)]
fn false_behind_a_reference_validates_like_false_written_inline(
    #[case] inline: Value,
    #[case] text: &str,
) {
    let behind = behind_a_reference(&inline);

    let expected = found(&inline, text);

    assert_eq!(found(&behind, text), expected);
}

// A reference is a schema, not the denial that the keyword itself spells.
#[rstest]
#[case::additional_properties(json!({ "additionalProperties": { "$ref": "#/definitions/No" } }), "a: 1\n",
    &[(0, 3, NA, "Value at a is not allowed by the schema")])]
#[case::unevaluated_properties(json!({ "unevaluatedProperties": { "$ref": "#/definitions/No" } }), "a: 1\n",
    &[(0, 3, NA, "Value at a is not allowed by the schema")])]
#[case::additional_items(json!({ "items": [], "additionalItems": { "$ref": "#/definitions/No" } }), "- a\n",
    &[(0, 2, NA, "Value at [0] is not allowed by the schema")])]
fn a_reference_to_false_for_the_denying_keywords_fails_the_value(
    #[case] mut schema: Value,
    #[case] text: &str,
    #[case] expected: &[Found],
) {
    schema["definitions"] = json!({ "No": false });

    assert_found(&schema, text, expected);
}

#[test]
fn a_root_reference_to_false_reports_every_document_at_its_root() {
    let schema = json!({ "$ref": "#/definitions/No", "definitions": { "No": false } });

    assert_found(
        &schema,
        "a: 1\n---\n- x\n",
        &[
            (0, 0, NA, "Value at <root> is not allowed by the schema"),
            (2, 0, NA, "Value at <root> is not allowed by the schema"),
        ],
    );
}

#[test]
fn a_false_target_shared_by_two_positions_is_reported_for_each() {
    let schema = json!({
        "properties": {
            "a": { "$ref": "#/definitions/No" },
            "b": { "$ref": "#/definitions/No" }
        },
        "definitions": { "No": false }
    });

    assert_eq!(found(&schema, "a: 1\nb: 2\n").len(), 2);
}

#[test]
fn a_long_chain_of_references_that_ends_in_false_is_followed() {
    let mut definitions = serde_json::Map::new();
    for hop in 0..100 {
        definitions.insert(
            format!("R{hop}"),
            json!({ "$ref": format!("#/definitions/R{}", hop + 1) }),
        );
    }
    definitions.insert("R100".to_string(), json!(false));
    let schema = json!({ "properties": { "a": { "$ref": "#/definitions/R0" } },
        "definitions": definitions });

    assert_eq!(found(&schema, "a: 1\n").len(), 1);
}

#[test]
fn a_schema_that_composes_itself_with_false_ends() {
    let schema = json!({
        "allOf": [{ "$ref": "#/definitions/A" }],
        "definitions": { "A": { "allOf": [false, { "$ref": "#" }] } }
    });

    let found = found(&schema, "a: 1\n");

    assert!(found.iter().all(|d| d.2 == NA), "{found:?}");
    assert!(!found.is_empty());
}

// ── Limits ───────────────────────────────────────────────────────────────────

#[test]
fn the_diagnostics_of_a_false_schema_stop_at_the_limit() {
    let schema = json!({ "items": false });
    let text: String = (0..MAX_DIAGNOSTICS + 100)
        .map(|i| format!("- {i}\n"))
        .collect::<Vec<_>>()
        .concat();

    let result = validate_schema(
        &parse_docs(&text),
        &parsed(&schema),
        true,
        YamlVersion::V1_2,
    );

    assert_eq!(result.diagnostics.len(), MAX_DIAGNOSTICS);
    assert_eq!(result.completeness, Completeness::StoppedEarly);
}

#[test]
fn the_keys_of_a_mapping_under_property_names_false_stop_at_the_limit() {
    let schema = json!({ "propertyNames": false });
    let text: String = (0..MAX_DIAGNOSTICS + 100)
        .map(|i| format!("k{i}: 1\n"))
        .collect::<Vec<_>>()
        .concat();

    let result = validate_schema(
        &parse_docs(&text),
        &parsed(&schema),
        true,
        YamlVersion::V1_2,
    );

    assert_eq!(result.diagnostics.len(), MAX_DIAGNOSTICS);
    assert_eq!(result.completeness, Completeness::StoppedEarly);
}

#[test]
fn a_probe_that_runs_out_of_steps_does_not_read_false_as_a_pass_or_a_fail() {
    let schema = parsed(&json!({ "not": false, "anyOf": [false, { "type": "string" }] }));
    let docs = parse_docs("x");
    // One step for the document: the probes find none left.
    let budget = Budget::new(1);

    let result = validate_documents(&docs, &schema, true, YamlVersion::V1_2, &budget);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.completeness, Completeness::StoppedEarly);
}

#[test]
fn a_run_without_steps_left_reports_no_false_schema() {
    let schema = parsed(&json!({ "items": false }));
    let docs = parse_docs("- a\n- b\n");
    let budget = Budget::new(0);

    let result = validate_documents(&docs, &schema, true, YamlVersion::V1_2, &budget);

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.completeness, Completeness::StoppedEarly);
}

/// `false` below `levels` properties named `a`, and the document that goes as
/// deep.
fn nested_false(levels: usize) -> (ParsedSchema, String) {
    let mut schema = JsonSchema::always_failing();
    let mut text = String::from("1");
    for _ in 0..levels {
        schema = JsonSchema {
            properties: Some([("a".to_string(), schema)].into()),
            ..JsonSchema::default()
        };
        text = format!("{{a: {text}}}");
    }
    (ParsedSchema::new(schema), text)
}

#[test]
fn a_false_schema_beyond_the_depth_limit_is_undecided_not_reported() {
    let (within, text) = nested_false(5);
    assert_eq!(diagnostics_of_parsed(&within, &text).len(), 1);

    let (beyond, text) = nested_false(support::MAX_VALIDATION_DEPTH + 10);

    assert!(!parse_docs(&text).is_empty(), "the document parses");
    assert!(diagnostics_of_parsed(&beyond, &text).is_empty());
}

#[test]
fn the_always_failing_schema_decides_before_the_type() {
    let schema = ParsedSchema::new(JsonSchema {
        always_fails: true,
        schema_type: Some(crate::schema::SchemaType::Single("string".to_string())),
        ..JsonSchema::default()
    });

    let diagnostics = diagnostics_of_parsed(&schema, "1");

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(code_of(&diagnostics[0]), NA);
}

fn diagnostics_of_parsed(schema: &ParsedSchema, text: &str) -> Vec<Diagnostic> {
    validate_schema(&parse_docs(text), schema, true, YamlVersion::V1_2).diagnostics
}
