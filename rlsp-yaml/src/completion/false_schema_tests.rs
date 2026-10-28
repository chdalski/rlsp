// SPDX-License-Identifier: MIT

//! The boolean schema `false` allows nothing, so completion offers nothing
//! for it.

use rstest::rstest;
use serde_json::{Value, json};
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use super::support::test_fixtures::{labels, pos};
use super::*;
use crate::schema::parse_schema;
use crate::schema::test_support::behind_a_reference;
use crate::test_utils::parse_docs;

fn complete(schema: &Value, text: &str, line: u32, character: u32) -> Vec<CompletionItem> {
    let parsed = parse_schema(schema).expect("the schema parses");
    complete_at(&parse_docs(text), pos(line, character), Some(&parsed))
}

fn sorted_labels(items: &[CompletionItem]) -> Vec<&str> {
    let mut labels = labels(items);
    labels.sort_unstable();
    labels
}

#[rstest]
#[case::property(json!({ "properties": { "a": {}, "b": {}, "c": false } }))]
#[case::in_a_branch(json!({ "allOf": [{ "properties": { "a": {}, "b": {}, "c": false } }] }))]
#[case::beside_valid_properties_of_a_branch(json!({
    "properties": { "a": {}, "c": false }, "anyOf": [{ "properties": { "b": {} } }] }))]
fn a_property_that_is_false_is_not_offered(#[case] schema: Value) {
    for schema in [schema.clone(), behind_a_reference(&schema)] {
        let items = complete(&schema, "a: 1\n", 0, 0);

        assert_eq!(sorted_labels(&items), ["b"], "{items:?}");
    }
}

#[test]
fn a_key_that_is_false_in_one_branch_and_valid_in_another_is_offered() {
    let schema = json!({
        "properties": { "a": {}, "c": false },
        "anyOf": [{ "properties": { "c": { "type": "string" } } }]
    });

    let items = complete(&schema, "a: 1\n", 0, 0);

    assert_eq!(sorted_labels(&items), ["c"]);
}

#[rstest]
#[case::inline(false)]
#[case::behind_a_reference(true)]
fn properties_that_are_all_false_leave_the_structural_suggestions(#[case] by_reference: bool) {
    let mut schema = json!({ "properties": { "p": false, "q": false } });
    if by_reference {
        schema = behind_a_reference(&schema);
    }

    let items = complete(&schema, "a: 1\nz: 2\n", 0, 0);

    assert_eq!(sorted_labels(&items), ["z"], "{items:?}");
}

fn required_snippet(items: &[CompletionItem]) -> Option<&CompletionItem> {
    items
        .iter()
        .find(|item| item.kind == Some(CompletionItemKind::SNIPPET))
}

#[rstest]
#[case::inline(false)]
#[case::behind_a_reference(true)]
fn the_required_keys_snippet_leaves_out_a_property_that_is_false(#[case] by_reference: bool) {
    let mut schema = json!({
        "properties": { "a": { "type": "string" }, "b": false, "c": { "type": "integer" } },
        "required": ["a", "b", "c"]
    });
    if by_reference {
        schema = behind_a_reference(&schema);
    }

    let items = complete(&schema, "x: 1\n", 0, 0);

    let snippet = required_snippet(&items).expect("a snippet");
    assert_eq!(
        snippet.insert_text.as_deref(),
        Some("a: ${1:\"\"}\nc: ${2:0}")
    );
    assert_eq!(snippet.detail.as_deref(), Some("2 required properties"));
}

#[test]
fn one_required_key_that_is_not_false_is_no_snippet() {
    let schema = json!({
        "properties": { "a": {}, "b": false },
        "required": ["a", "b"]
    });

    let items = complete(&schema, "x: 1\n", 0, 0);

    assert!(required_snippet(&items).is_none(), "{items:?}");
}

#[test]
fn a_value_is_not_completed_from_a_branch_when_the_property_is_false() {
    let declares_values = json!({ "properties": { "x": { "enum": ["a", "b"] } } });
    let allowed = json!({ "allOf": [declares_values] });
    let forbidden = json!({ "properties": { "x": false }, "allOf": [declares_values] });

    assert_eq!(
        sorted_labels(&complete(&allowed, "x: \n", 0, 3)),
        ["a", "b"]
    );
    assert!(complete(&forbidden, "x: \n", 0, 3).is_empty());
}

#[test]
fn nothing_is_completed_below_an_item_that_is_false() {
    let schema = json!({ "items": false, "allOf": [{ "items": { "properties": { "k": {} } } }] });

    let items = complete(&schema, "- a: 1\n  \n", 1, 2);

    assert!(!labels(&items).contains(&"k"), "{items:?}");
}
