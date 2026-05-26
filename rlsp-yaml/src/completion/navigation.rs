// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::LineIndex;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};

use super::cursor_location::node_span;
use super::cursor_location::scalar_key;

/// Walk `docs` following `path` (a sequence of mapping key strings) and return
/// the node at that path, or `None` if any step fails.
pub(super) fn find_node_at_path<'a>(
    docs: &'a [Document<Span>],
    path: &[String],
) -> Option<&'a Node<Span>> {
    let root = docs.first().map(|d| &d.root)?;
    let mut current = root;
    for key in path {
        match current {
            Node::Mapping { entries, .. } => {
                let entry = entries
                    .iter()
                    .find(|(k, _)| scalar_key(k) == Some(key.as_str()));
                current = entry.map(|(_, v)| v)?;
            }
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => return None,
        }
    }
    Some(current)
}

/// Return all keys present in `mapping`, excluding the entry whose key token
/// starts on `cursor_line` (the line currently being edited).
///
/// `cursor_line` is the 0-based LSP line number. The exclusion prevents the
/// key under the cursor from appearing in "already present" sets when
/// computing schema suggestions.
pub(super) fn present_keys(
    mapping: &Node<Span>,
    cursor_line: usize,
    idx: &LineIndex,
) -> HashSet<String> {
    let Node::Mapping { entries, .. } = mapping else {
        return HashSet::new();
    };
    // Parser lines are 1-based; LSP lines are 0-based.
    let parser_cursor_line = cursor_line + 1;
    entries
        .iter()
        .filter_map(|(key_node, _)| {
            let key_span = node_span(key_node);
            if idx.line_column(key_span.start).0 as usize == parser_cursor_line {
                return None;
            }
            scalar_key(key_node).map(ToString::to_string)
        })
        .collect()
}

/// Return all keys in `mapping` in declaration order.
///
/// Non-scalar keys (complex mapping-as-key, sequence-as-key) are skipped.
pub(super) fn collect_sibling_keys_ast(mapping: &Node<Span>) -> Vec<String> {
    let Node::Mapping { entries, .. } = mapping else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(key_node, _)| scalar_key(key_node).map(ToString::to_string))
        .collect()
}

/// Return the union of all keys across every item in `sequence`.
///
/// Items that are not mappings (e.g. scalar items) contribute no keys.
pub(super) fn collect_sequence_sibling_keys(sequence: &Node<Span>) -> HashSet<String> {
    let Node::Sequence { items, .. } = sequence else {
        return HashSet::new();
    };
    items
        .iter()
        .flat_map(|item| {
            if let Node::Mapping { entries, .. } = item {
                entries
                    .iter()
                    .filter_map(|(k, _)| scalar_key(k).map(ToString::to_string))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rstest::rstest;

    use super::{
        collect_sequence_sibling_keys, collect_sibling_keys_ast, find_node_at_path, present_keys,
    };
    use crate::test_utils::parse_docs;
    use rlsp_yaml_parser::node::Node;

    // ── find_node_at_path: None cases ─────────────────────────────────────────

    #[rstest]
    #[case::empty_docs(&[], &["any"], "empty docs slice")]
    #[case::missing_key(&["name: Alice\n"], &["missing"], "key not in mapping")]
    #[case::scalar_mid_path(&["name: Alice\n"], &["name", "deeper"], "scalar terminates traversal")]
    #[case::sequence_mid_path(&["items:\n  - foo\n"], &["items", "deeper"], "sequence terminates traversal")]
    fn find_node_at_path_returns_none(
        #[case] yamls: &[&str],
        #[case] path: &[&str],
        #[case] desc: &str,
    ) {
        let docs = yamls.first().map_or_else(Vec::new, |y| parse_docs(y));
        let path: Vec<String> = path.iter().map(|s| (*s).to_string()).collect();
        assert!(
            find_node_at_path(&docs, &path).is_none(),
            "expected None for {desc}"
        );
    }

    enum NodeKind {
        Mapping,
        Scalar,
    }

    #[rstest]
    #[case::empty_path_returns_root("key: val\n", &[], NodeKind::Mapping)]
    #[case::single_key_found("name: Alice\n", &["name"], NodeKind::Scalar)]
    #[case::nested_path("outer:\n  inner: value\n", &["outer", "inner"], NodeKind::Scalar)]
    fn find_node_at_path_returns_node(
        #[case] yaml: &str,
        #[case] path: &[&str],
        #[case] expected_kind: NodeKind,
    ) {
        let docs = parse_docs(yaml);
        let path: Vec<String> = path.iter().map(|s| (*s).to_string()).collect();
        let node = find_node_at_path(&docs, &path).expect("expected Some node, got None");
        match expected_kind {
            NodeKind::Mapping => assert!(
                matches!(node, Node::Mapping { .. }),
                "expected Mapping, got non-Mapping"
            ),
            NodeKind::Scalar => assert!(
                matches!(node, Node::Scalar { .. }),
                "expected Scalar, got non-Scalar"
            ),
        }
    }

    // ── present_keys ──────────────────────────────────────────────────────────

    #[rstest]
    #[case::excludes_cursor_line(
        "name: Alice\nage: 30\ncity: NY\n",
        1,
        &["name", "city"],
        &["age"]
    )]
    #[case::only_entry_excluded("name: Alice\n", 0, &[], &["name"])]
    #[case::utf8("café: latte\nname: Alice\n", 0, &["name"], &["café"])]
    fn present_keys_test(
        #[case] yaml: &str,
        #[case] cursor_line: usize,
        #[case] expected_present: &[&str],
        #[case] expected_absent: &[&str],
    ) {
        let docs = parse_docs(yaml);
        let Node::Mapping { .. } = &docs[0].root else {
            panic!("expected mapping root");
        };
        let keys = present_keys(&docs[0].root, cursor_line, docs[0].line_index());
        for k in expected_present {
            assert!(
                keys.contains(*k),
                "expected '{k}' in present_keys, got: {keys:?}"
            );
        }
        for k in expected_absent {
            assert!(
                !keys.contains(*k),
                "expected '{k}' absent from present_keys, got: {keys:?}"
            );
        }
    }

    #[test]
    fn present_keys_sequence_item() {
        let yaml = "items:\n  - name: foo\n    age: 1\n";
        let docs = parse_docs(yaml);
        // Navigate to the sequence item mapping
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping root");
        };
        let (_, seq_value) = &entries[0];
        let Node::Sequence { items, .. } = seq_value else {
            panic!("expected sequence");
        };
        let item_mapping = &items[0];
        // cursor_line=1 corresponds to "- name: foo" (0-based LSP line 1)
        let keys = present_keys(item_mapping, 1, docs[0].line_index());
        assert!(keys.contains("age"), "age should be present");
        assert!(
            !keys.contains("name"),
            "name should be excluded (on cursor_line 1)"
        );
    }

    // ── collect_sibling_keys_ast ──────────────────────────────────────────────

    #[rstest]
    #[case::declaration_order("a: 1\nb: 2\nc: 3\n", vec!["a", "b", "c"])]
    #[case::single_key("only: val\n", vec!["only"])]
    #[case::utf8("café: 1\ntea: 2\n", vec!["café", "tea"])]
    fn collect_sibling_keys_ast_test(#[case] yaml: &str, #[case] expected: Vec<&str>) {
        let docs = parse_docs(yaml);
        let keys = collect_sibling_keys_ast(&docs[0].root);
        assert_eq!(
            keys, expected,
            "declaration order mismatch for yaml={yaml:?}"
        );
    }

    #[test]
    fn collect_sibling_keys_ast_skips_non_scalar_keys() {
        // Verify collect_sibling_keys_ast only returns string keys.
        let yaml = "x: 1\ny: 2\n";
        let docs = parse_docs(yaml);
        let keys = collect_sibling_keys_ast(&docs[0].root);
        assert_eq!(keys, vec!["x", "y"]);
    }

    // ── collect_sequence_sibling_keys ─────────────────────────────────────────

    #[rstest]
    #[case::union("- name: foo\n  age: 1\n- name: bar\n  city: NY\n", &["name", "age", "city"])]
    #[case::scalar_items_no_keys("- foo\n- bar\n", &[])]
    #[case::utf8("- café: latte\n- tea: matcha\n", &["café", "tea"])]
    #[case::dedup("- name: foo\n- name: bar\n", &["name"])]
    #[case::single_item("- x: 1\n  y: 2\n", &["x", "y"])]
    fn collect_sequence_sibling_keys_test(#[case] yaml: &str, #[case] expected: &[&str]) {
        let docs = parse_docs(yaml);
        // The root is a sequence in these test cases.
        let keys = collect_sequence_sibling_keys(&docs[0].root);
        let expected_set: HashSet<&str> = expected.iter().copied().collect();
        let actual_set: HashSet<&str> = keys.iter().map(String::as_str).collect();
        assert_eq!(
            actual_set, expected_set,
            "key set mismatch for yaml={yaml:?}"
        );
    }

    #[test]
    fn collect_sequence_sibling_keys_empty_sequence() {
        // Use an inline flow empty sequence to get an actual Sequence node.
        let docs2 = parse_docs("[]\n");
        let keys = collect_sequence_sibling_keys(&docs2[0].root);
        assert!(keys.is_empty(), "empty sequence should return empty set");
    }
}
