// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::LineIndex;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use super::cursor_location::{node_span, scalar_key};
use super::support::MAX_COMPLETION_ITEMS;

pub(super) fn keys_to_items(keys: Vec<String>) -> Vec<CompletionItem> {
    keys.into_iter()
        .map(|k| CompletionItem {
            label: k,
            kind: Some(CompletionItemKind::FIELD),
            ..CompletionItem::default()
        })
        .collect()
}

/// Scan `docs` for all distinct scalar values associated with `key_name` in any
/// mapping, excluding the cursor line itself (which is still being typed).
pub(super) fn collect_values_for_key_ast(
    docs: &[Document<Span>],
    cursor_line: usize,
    key_name: &str,
) -> Vec<CompletionItem> {
    let parser_cursor_line = cursor_line + 1;
    let mut seen: HashSet<String> = HashSet::new();
    let mut items = Vec::new();

    for doc in docs {
        let idx = doc.line_index();
        collect_values_in_node(
            &doc.root,
            key_name,
            parser_cursor_line,
            &mut seen,
            &mut items,
            idx,
        );
    }
    items
}

pub(super) fn collect_values_in_node(
    node: &Node<Span>,
    key_name: &str,
    parser_cursor_line: usize,
    seen: &mut HashSet<String>,
    items: &mut Vec<CompletionItem>,
    idx: &LineIndex,
) {
    match node {
        Node::Mapping { entries, .. } => {
            for (key_node, value_node) in entries {
                if let Some(k) = scalar_key(key_node)
                    && k == key_name
                {
                    let key_span = node_span(key_node);
                    if idx.line_column(key_span.start).0 as usize != parser_cursor_line
                        && let Node::Scalar { value, .. } = value_node
                        && !value.is_empty()
                        && seen.insert(value.clone())
                    {
                        items.push(CompletionItem {
                            label: value.clone(),
                            kind: Some(CompletionItemKind::VALUE),
                            ..CompletionItem::default()
                        });
                    }
                }
                collect_values_in_node(value_node, key_name, parser_cursor_line, seen, items, idx);
            }
        }
        Node::Sequence {
            items: seq_items, ..
        } => {
            for item in seq_items {
                collect_values_in_node(item, key_name, parser_cursor_line, seen, items, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Merge structural and schema-sourced key completion items, deduplicating by
/// label and capping at `MAX_COMPLETION_ITEMS`.
pub(super) fn merge_completions(
    structural: Vec<CompletionItem>,
    schema_items: Vec<CompletionItem>,
) -> Vec<CompletionItem> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result: Vec<CompletionItem> = Vec::new();

    // Schema items first (richer metadata), then structural fallback.
    for item in schema_items.into_iter().chain(structural) {
        if seen.insert(item.label.clone()) {
            result.push(item);
            if result.len() >= MAX_COMPLETION_ITEMS {
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::CompletionItemKind;

    use super::super::support::test_fixtures::labels;
    use super::*;
    use crate::test_utils::parse_docs;

    // ── keys_to_items ─────────────────────────────────────────────────────────

    #[test]
    fn keys_to_items_produces_field_items() {
        let result = keys_to_items(vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].label, "alpha");
        assert_eq!(result[1].label, "beta");
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD))
        );
    }

    #[test]
    fn keys_to_items_empty_input_returns_empty() {
        let result = keys_to_items(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn keys_to_items_preserves_order() {
        let result = keys_to_items(vec!["z".to_string(), "a".to_string()]);
        assert_eq!(result[0].label, "z");
        assert_eq!(result[1].label, "a");
    }

    // ── merge_completions ─────────────────────────────────────────────────────

    fn field_item(label: &str) -> CompletionItem {
        CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            ..CompletionItem::default()
        }
    }

    #[test]
    fn merge_completions_schema_items_first() {
        let structural = vec![field_item("foo")];
        let schema = vec![field_item("bar"), field_item("baz")];
        let result = merge_completions(structural, schema);
        assert_eq!(
            result.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            vec!["bar", "baz", "foo"]
        );
    }

    #[test]
    fn merge_completions_deduplicates_by_label() {
        let structural = vec![field_item("foo")];
        let schema = vec![field_item("foo"), field_item("bar")];
        let result = merge_completions(structural, schema);
        assert_eq!(result.len(), 2);
        assert_eq!(result.iter().filter(|i| i.label == "foo").count(), 1);
        assert!(result.iter().any(|i| i.label == "bar"));
    }

    #[test]
    fn merge_completions_empty_structural() {
        let result = merge_completions(vec![], vec![field_item("x")]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "x");
    }

    #[test]
    fn merge_completions_empty_schema() {
        let result = merge_completions(vec![field_item("a")], vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "a");
    }

    #[test]
    fn merge_completions_caps_at_max_completion_items() {
        let structural: Vec<CompletionItem> =
            (0..60).map(|i| field_item(&format!("s{i}"))).collect();
        let schema: Vec<CompletionItem> = (0..60).map(|i| field_item(&format!("c{i}"))).collect();
        let result = merge_completions(structural, schema);
        assert_eq!(result.len(), super::MAX_COMPLETION_ITEMS);
        // schema items come first
        assert!(result[0].label.starts_with('c'));
        assert!(result[59].label.starts_with('c'));
        assert!(result[60].label.starts_with('s'));
    }

    // ── collect_values_for_key_ast ────────────────────────────────────────────

    #[test]
    fn collect_values_for_key_ast_collects_distinct_scalar_values() {
        let docs = parse_docs("env: prod\nenv: staging\nenv: prod\n");
        let result = collect_values_for_key_ast(&docs, 3, "env");
        let ls = labels(&result);
        assert_eq!(
            ls.iter().filter(|&&l| l == "prod").count(),
            1,
            "no duplicate 'prod'"
        );
        assert!(ls.contains(&"staging"));
    }

    #[test]
    fn collect_values_for_key_ast_excludes_cursor_line() {
        let docs = parse_docs("env: prod\nenv: \n");
        // cursor_line=1 → parser line 2, which is the blank-value line
        let result = collect_values_for_key_ast(&docs, 1, "env");
        let ls = labels(&result);
        assert!(ls.contains(&"prod"), "non-cursor line value should appear");
        assert_eq!(ls.len(), 1, "cursor line should be excluded");
    }

    #[test]
    fn collect_values_for_key_ast_skips_non_scalar_values() {
        let docs = parse_docs("env: prod\nenv:\n  nested: val\n");
        let result = collect_values_for_key_ast(&docs, 2, "env");
        let ls = labels(&result);
        assert!(ls.contains(&"prod"));
        assert!(!ls.contains(&"nested"));
    }

    #[test]
    fn collect_values_for_key_ast_empty_when_no_match() {
        let docs = parse_docs("name: Alice\n");
        let result = collect_values_for_key_ast(&docs, 1, "age");
        assert!(result.is_empty());
    }

    #[test]
    fn collect_values_for_key_ast_items_have_value_kind() {
        let docs = parse_docs("kind: app\nkind: lib\nkind: \n");
        let result = collect_values_for_key_ast(&docs, 2, "kind");
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE))
        );
        assert_eq!(result.len(), 2);
    }
}
