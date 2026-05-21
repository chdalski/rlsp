// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

use crate::lsp_util::span_to_lsp;

/// Validate map key ordering in YAML documents.
///
/// Returns warning diagnostics for map keys that are not in alphabetical order.
/// Uses case-sensitive lexicographic comparison. Diagnostic ranges use the key
/// node's `loc` span directly.
#[must_use]
pub fn validate_key_ordering(docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for doc in docs {
        let idx = doc.line_index();
        check_yaml_ordering(&doc.root, &mut diagnostics, 0, idx);
    }

    diagnostics
}

/// Recursively check YAML nodes for key ordering, with depth limit.
fn check_yaml_ordering(
    node: &Node<Span>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
    idx: &LineIndex,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Mapping { entries, .. } => {
            // Collect (key_string, key_loc) pairs, skipping null keys.
            let keys: Vec<(&str, &Span)> = entries
                .iter()
                .filter_map(|(k, _)| match k {
                    Node::Scalar {
                        tag, value, loc, ..
                    } if tag.as_deref() != Some("tag:yaml.org,2002:null") => {
                        Some((value.as_str(), loc))
                    }
                    Node::Scalar { .. }
                    | Node::Mapping { .. }
                    | Node::Sequence { .. }
                    | Node::Alias { .. } => None,
                })
                .collect();

            // Track the maximum key seen so far to catch all out-of-order keys.
            let mut max_key: &str = keys.first().map_or("", |&(k, _)| k);

            for &(key, loc) in keys.iter().skip(1) {
                if key < max_key {
                    let range = span_to_lsp(*loc, idx);
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("mapKeyOrder".to_string())),
                        message: format!("Key '{key}' is out of alphabetical order"),
                        source: Some("rlsp-yaml".to_string()),
                        ..Diagnostic::default()
                    });
                } else if key > max_key {
                    max_key = key;
                }
            }

            // Recursively check nested structures.
            for (_, value) in entries {
                check_yaml_ordering(value, diagnostics, depth + 1, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_yaml_ordering(item, diagnostics, depth + 1, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    use super::*;

    // ---- Map Key Order Validator: Happy Paths / Nested Structures / Edge Cases ----

    #[rstest]
    #[case::ordered_keys("apple: 1\nbanana: 2\ncherry: 3\n")]
    #[case::empty_document("")]
    #[case::single_key("only: value\n")]
    #[case::sequence_items_ignored("items:\n  - zebra\n  - alpha\n")]
    #[case::multi_document_single_keys("z: 1\n---\na: 2\n")]
    #[case::case_sensitive_uppercase_first("Apple: 1\napple: 2\n")]
    fn key_ordering_returns_empty(#[case] input: &str) {
        let docs = rlsp_yaml_parser::load(input).unwrap();
        let result = validate_key_ordering(&docs);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::single_ooo("banana: 2\napple: 1\n", 1)]
    #[case::multiple_ooo("charlie: 3\nalpha: 1\nbravo: 2\n", 2)]
    #[case::nested_ooo("outer:\n  zebra: 1\n  alpha: 2\n", 1)]
    #[case::top_level_ooo_only("b_parent:\n  a_child: 1\na_parent:\n  key: val\n", 1)]
    #[case::numeric_string_lexicographic("2: two\n10: ten\n", 1)]
    fn key_ordering_count(#[case] input: &str, #[case] expected: usize) {
        let docs = rlsp_yaml_parser::load(input).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), expected);
    }

    // ---- Map Key Order Validator: standalone ----

    #[test]
    fn should_detect_out_of_order_keys() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "mapKeyOrder")
        );
    }

    #[test]
    fn should_return_correct_range_for_out_of_order_key() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "apple is on line 1");
        assert_eq!(result[0].range.start.character, 0, "apple starts at col 0");
        assert_eq!(result[0].range.end.character, 5, "apple is 5 chars long");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group 7: check_yaml_ordering — tag-based null key filtering
    // ══════════════════════════════════════════════════════════════════════════

    // T7.1 — null key is excluded from ordering check with tag-based null detection
    #[test]
    fn tag_driven_null_key_excluded_from_ordering_check() {
        // ~ is null; zebra and alpha are out of order — only 1 ordering diagnostic expected
        let text = "~: value\nzebra: 1\nalpha: 2\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);
        assert_eq!(
            result.len(),
            1,
            "null key must be excluded; only alpha is out of order"
        );
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "mapKeyOrder")
        );
    }

    // T7.2 — non-null plain scalar key is included in ordering check (baseline)
    #[test]
    fn tag_driven_non_null_key_included_in_ordering_check() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);
        assert_eq!(result.len(), 1, "apple is out of order and must be flagged");
    }

    // ---- Key Ordering Validator: AST-range regression tests ----

    #[test]
    fn out_of_order_key_range_covers_key_span() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "apple on line 1");
        assert_eq!(result[0].range.start.character, 0, "apple at col 0");
        assert_eq!(result[0].range.end.character, 5, "apple is 5 chars");
    }

    #[test]
    fn out_of_order_key_in_indented_block_has_correct_column() {
        let text = "outer:\n  zebra: 1\n  alpha: 2\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 2, "alpha on line 2");
        assert_eq!(result[0].range.start.character, 2, "alpha indented by 2");
        assert_eq!(
            result[0].range.end.character, 7,
            "col 2 + len('alpha') == 7"
        );
    }

    #[test]
    fn out_of_order_keys_in_flow_mapping() {
        let text = "{zebra: 1, alpha: 2}";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert!(
            result[0].range.start.character > 0,
            "alpha's column in flow mapping is non-zero"
        );
    }
}
