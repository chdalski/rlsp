// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::CompletionItem;

use crate::schema::JsonSchema;

use super::completion_items::{collect_values_for_key_ast, keys_to_items, merge_completions};
use super::cursor_location::{node_span, scalar_key};
use super::navigation::{
    collect_sequence_sibling_keys, collect_sibling_keys_ast, find_node_at_path, present_keys,
};

pub(super) fn complete_on_key<'a>(
    docs: &'a [Document<Span>],
    cursor_line: usize,
    key: String,
    enclosing_path: &[String],
    mapping: &'a Node<Span>,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    let present = docs.first().map_or_else(HashSet::new, |d| {
        present_keys(mapping, cursor_line, d.line_index())
    });
    let seq_len = enclosing_path.len().saturating_sub(1);
    let structural_keys: HashSet<String> = if enclosing_path.last().is_some_and(|s| s == "[]") {
        let seq_path = enclosing_path.get(..seq_len).unwrap_or(&[]);
        match find_node_at_path(docs, seq_path) {
            Some(seq @ Node::Sequence { .. }) => collect_sequence_sibling_keys(seq),
            _ => collect_sibling_keys_ast(mapping).into_iter().collect(),
        }
    } else {
        collect_sibling_keys_ast(mapping).into_iter().collect()
    };
    let structural = keys_to_items(structural_keys.into_iter().filter(|k| k != &key).collect());
    if let Some(s) = schema {
        if let Some(resolved_schema) = super::resolve_schema_path(s, enclosing_path)
            && super::schema_has_properties(resolved_schema)
        {
            let schema_properties = super::collect_schema_properties_keys(resolved_schema);
            let schema_exclude: HashSet<String> = if schema_properties.contains(&key) {
                let mut ex = present;
                ex.insert(key);
                ex
            } else {
                HashSet::from([key])
            };
            let schema_items = super::schema_key_completions(resolved_schema, &schema_exclude);
            let filtered_structural: Vec<CompletionItem> = structural
                .into_iter()
                .filter(|i| !schema_exclude.contains(i.label.as_str()))
                .collect();
            return merge_completions(filtered_structural, schema_items);
        }
    }
    structural
}

pub(super) fn complete_on_value(
    docs: &[Document<Span>],
    cursor_line: usize,
    key: &str,
    enclosing_path: Vec<String>,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    if let Some(s) = schema {
        let mut value_path = enclosing_path;
        value_path.push(key.to_string());
        if let Some(prop_schema) = super::resolve_schema_path(s, &value_path) {
            let schema_items = super::schema_value_completions(prop_schema);
            if !schema_items.is_empty() {
                return schema_items;
            }
        }
    }
    let cursor_parser_line = cursor_line + 1;
    let cursor_doc = docs.first().map_or(docs, |first_doc| {
        let idx = first_doc.line_index();
        docs.iter()
            .position(|d| {
                let span = node_span(&d.root);
                idx.line_column(span.start).0 as usize <= cursor_parser_line
                    && cursor_parser_line <= idx.line_column(span.end).0 as usize
            })
            .and_then(|i| docs.get(i))
            .map_or(docs, std::slice::from_ref)
    });
    collect_values_for_key_ast(cursor_doc, cursor_line, key)
}

pub(super) fn complete_in_sequence_item<'a>(
    enclosing_path: Vec<String>,
    sequence: &'a Node<Span>,
    current_item: &'a Node<Span>,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    let current_keys: HashSet<String> = if let Node::Mapping { entries, .. } = current_item {
        entries
            .iter()
            .filter_map(|(k, _)| scalar_key(k).map(ToString::to_string))
            .collect()
    } else {
        HashSet::new()
    };
    let structural = keys_to_items(
        collect_sequence_sibling_keys(sequence)
            .into_iter()
            .filter(|k| !current_keys.contains(k.as_str()))
            .collect(),
    );
    if let Some(s) = schema {
        let mut items_path = enclosing_path;
        items_path.push("[]".to_string());
        if let Some(items_schema) = super::resolve_schema_path(s, &items_path)
            && super::schema_has_properties(items_schema)
        {
            let schema_items = super::schema_key_completions(items_schema, &current_keys);
            let filtered_structural: Vec<CompletionItem> = structural
                .into_iter()
                .filter(|i| !current_keys.contains(i.label.as_str()))
                .collect();
            return merge_completions(filtered_structural, schema_items);
        }
    }
    structural
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::CompletionItemKind;

    use super::super::support::test_fixtures::{
        integer_schema, labels, object_schema, pos, string_schema,
    };
    use crate::completion::complete_at;
    use crate::schema::{JsonSchema, SchemaType};
    use crate::test_utils::parse_docs;
    use serde_json::json;

    // ── complete_on_key ───────────────────────────────────────────────────────

    #[test]
    fn complete_on_key_returns_sibling_keys_when_no_schema() {
        let docs = parse_docs("name: Alice\nage: 30\n");
        let result = complete_at(&docs, pos(0, 0), None);
        let ls = labels(&result);
        assert!(ls.contains(&"age"), "should suggest sibling 'age'");
        assert!(!ls.contains(&"name"), "should exclude cursor key 'name'");
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD))
        );
    }

    #[test]
    fn complete_on_key_excludes_cursor_key_from_schema_results() {
        let docs = parse_docs("name: Alice\n");
        let schema = object_schema(vec![("name", string_schema()), ("city", string_schema())]);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));
        let ls = labels(&result);
        assert!(!ls.contains(&"name"), "cursor key should be excluded");
        assert!(ls.contains(&"city"), "schema property 'city' should appear");
    }

    #[test]
    fn complete_on_key_schema_excludes_all_present_keys() {
        let docs = parse_docs("name: Alice\nage: 30\n");
        let schema = object_schema(vec![
            ("name", string_schema()),
            ("age", integer_schema()),
            ("city", string_schema()),
        ]);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"city"), "should suggest 'city'");
        assert!(!ls.contains(&"name"), "cursor key excluded");
        assert!(!ls.contains(&"age"), "present key excluded");
    }

    #[test]
    fn complete_on_key_falls_back_to_structural_when_schema_has_no_properties() {
        let docs = parse_docs("name: Alice\nage: 30\n");
        let schema = string_schema();
        let result = complete_at(&docs, pos(0, 0), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"age"), "structural fallback: sibling 'age'");
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD))
        );
    }

    // ── complete_on_value ─────────────────────────────────────────────────────

    #[test]
    fn complete_on_value_returns_schema_enum_values() {
        let docs = parse_docs("env: \n");
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let result = complete_at(&docs, pos(0, 5), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"prod"));
        assert!(ls.contains(&"staging"));
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE))
        );
    }

    #[test]
    fn complete_on_value_falls_back_to_structural_when_schema_is_none() {
        let docs = parse_docs("kind: app\nkind: \n");
        let result = complete_at(&docs, pos(1, 6), None);
        let ls = labels(&result);
        assert!(ls.contains(&"app"), "should suggest existing value 'app'");
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE))
        );
    }

    #[test]
    fn complete_on_value_falls_back_to_structural_when_no_enum_in_schema() {
        let docs = parse_docs("kind: app\nkind: \n");
        let schema = object_schema(vec![("kind", string_schema())]);
        let result = complete_at(&docs, pos(1, 6), Some(&schema));
        let ls = labels(&result);
        assert!(
            ls.contains(&"app"),
            "structural fallback when schema has no enum"
        );
    }

    // ── complete_in_sequence_item ─────────────────────────────────────────────

    #[test]
    fn complete_in_sequence_item_suggests_missing_sibling_keys() {
        let docs = parse_docs("items:\n  - name: Alice\n    age: 30\n  - name: Bob\n");
        let result = complete_at(&docs, pos(3, 4), None);
        let ls = labels(&result);
        assert!(ls.contains(&"age"), "should suggest sibling key 'age'");
        assert!(!ls.contains(&"name"), "current item key excluded");
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD))
        );
    }

    #[test]
    fn complete_in_sequence_item_returns_empty_when_no_sibling_keys_missing() {
        let docs = parse_docs("items:\n  - name: Alice\n  - name: Bob\n");
        let result = complete_at(&docs, pos(2, 4), None);
        let ls = labels(&result);
        assert!(!ls.contains(&"name"), "name already in current item");
    }

    #[test]
    fn complete_in_sequence_item_uses_schema_when_present() {
        let docs = parse_docs("items:\n  - name: Alice\n  - name: Bob\n");
        let schema = object_schema(vec![(
            "items",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(object_schema(vec![
                    ("name", string_schema()),
                    ("id", integer_schema()),
                ]))),
                ..JsonSchema::default()
            },
        )]);
        let result = complete_at(&docs, pos(2, 4), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"id"), "schema-sourced 'id' should appear");
        assert!(!ls.contains(&"name"), "present key excluded");
    }

    // ── sequence-context detection (moved from completion.rs) ─────────────────

    #[test]
    fn should_not_detect_sequence_context_across_document_separator() {
        let text = "items:\n  - name: Alice\n---\nhost: local\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 0), None);
        let ls = labels(&result);
        assert!(
            !ls.contains(&"name"),
            "should not suggest sequence key 'name' from doc1, got: {ls:?}"
        );
    }

    #[test]
    fn should_not_detect_sequence_context_when_parent_is_plain_mapping() {
        let text = "server:\n  host: localhost\n  port: 8080\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 2), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest sibling 'port', not sequence keys, got: {ls:?}"
        );
    }

    #[test]
    fn should_detect_sequence_context_when_same_indent_sibling_is_sequence_item() {
        let text = "people:\n  - name: Alice\n    age: 30\n  - name: Bob\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"age"),
            "should suggest 'age' from sibling sequence item, got: {ls:?}"
        );
    }

    #[test]
    fn should_suggest_sibling_sequence_item_keys_for_multiline_sequence_item() {
        let text = "items:\n  - name: Alice\n    age: 30\n    city: NY\n  - name: Bob\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(4, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"age") || ls.contains(&"city"),
            "should suggest keys from sibling sequence item, got: {ls:?}"
        );
    }

    #[test]
    fn should_find_sequence_indent_when_cursor_is_not_on_sequence_line() {
        let text = "list:\n  - id: 1\n    label: a\n  - id: 2\n    score: 99\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(2, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"score"),
            "should suggest 'score' from sibling sequence item, got: {ls:?}"
        );
    }

    #[test]
    fn should_collect_keys_from_all_sequence_items_including_those_before_cursor() {
        let text = "- kind: A\n  color: red\n- kind: B\n  size: large\n- kind: C\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(4, 2), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"color") || ls.contains(&"size"),
            "should collect keys from all prior sequence items, got: {ls:?}"
        );
    }
}
