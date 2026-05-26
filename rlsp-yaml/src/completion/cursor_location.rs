// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::LineIndex;
use rlsp_yaml_parser::Pos;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::Position;

/// Where the cursor sits in the YAML AST.
///
/// Used by `locate_cursor` and consumed by `complete_at`.
/// Every variant carries the `enclosing_path` — ancestor mapping keys from the
/// document root down to the immediately enclosing structure, with `"[]"`
/// sentinels for sequence descents.
#[derive(Debug)]
pub(super) enum CursorLocation<'a> {
    /// Cursor is inside a mapping key token.
    ///
    /// `key` is the key being typed; `enclosing_path` is the path to the
    /// containing mapping; `mapping` is the containing `Node::Mapping`.
    OnKey {
        key: String,
        enclosing_path: Vec<String>,
        mapping: &'a Node<Span>,
    },
    /// Cursor is in the value position of a `key: <value>` pair.
    ///
    /// `key` names the key whose value is under the cursor; `enclosing_path`
    /// is the path to the containing mapping (does **not** include `key`).
    OnValue {
        key: String,
        enclosing_path: Vec<String>,
    },
    /// Cursor is on a blank/whitespace-only line inside a mapping.
    ///
    /// No AST node's span contains the cursor, but a Mapping's span covers
    /// `cursor.line` and its entries sit at a column ≤ the cursor column.
    /// `mapping` is the deepest such `Node::Mapping`.
    InBlankMapping {
        enclosing_path: Vec<String>,
        mapping: &'a Node<Span>,
    },
    /// Cursor is inside a specific sequence item.
    ///
    /// `sequence` is the containing `Node::Sequence`; `current_item` is the
    /// item node the cursor sits in.
    InSequenceItem {
        enclosing_path: Vec<String>,
        sequence: &'a Node<Span>,
        current_item: &'a Node<Span>,
    },
    /// Cursor is on a blank/whitespace-only line directly inside a sequence.
    ///
    /// No item's span contains the cursor, but the sequence's own span covers
    /// the cursor line.
    InBlankSequence {
        enclosing_path: Vec<String>,
        sequence: &'a Node<Span>,
    },
    /// Cursor cannot be located in any AST structure.
    ///
    /// Covers: empty document, position past EOF, cursor on `---`/`...`,
    /// cursor on a comment line.
    OutsideAny,
}

/// Returns `true` when `cursor` is within `span` using half-open `[start, end)`.
///
/// Comparison is lexicographic on `(line, column)`, matching the semantics of
/// `hover.rs::span_contains` and `navigation/references.rs::span_contains`.
pub(super) fn span_contains_cursor(span: Span, cursor: Pos, idx: &LineIndex) -> bool {
    let start = (
        idx.line_column(span.start).0 as usize,
        idx.line_column(span.start).1 as usize,
    );
    let end = (
        idx.line_column(span.end).0 as usize,
        idx.line_column(span.end).1 as usize,
    );
    let pos = (cursor.line, cursor.column);
    start <= pos && pos < end
}

/// Extract the `loc` span from any AST node.
pub(super) const fn node_span(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

/// Extract the scalar key string from a key node, returning `None` for
/// non-scalar keys (complex mappings, sequences, aliases).
pub(super) const fn scalar_key(node: &Node<Span>) -> Option<&str> {
    match node {
        Node::Scalar { value, .. } => Some(value.as_str()),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
    }
}

/// Convert an LSP `Position` (0-based line, 0-based character) to a parser
/// `Pos` (1-based line, 0-based column).
pub(super) const fn lsp_position_to_pos(position: Position) -> Pos {
    Pos {
        byte_offset: 0,
        line: position.line as usize + 1,
        column: position.character as usize,
    }
}

/// Walk `node` (which must be a `Node::Mapping`) looking for the deepest
/// nested mapping whose entries have `key.(idx.line_column(loc.start).1 as usize) <= cursor.column`
/// and whose span covers `cursor.line`.
///
/// `path` accumulates ancestor mapping keys in root-to-leaf order as the
/// function descends. Returns `None` if `node` is not a mapping, its span
/// doesn't cover `cursor.line`, or none of its entries' key columns ≤
/// `cursor.column`.
pub(super) fn deepest_mapping_at_column<'a>(
    node: &'a Node<Span>,
    cursor: Pos,
    path: &mut Vec<String>,
    idx: &LineIndex,
) -> Option<&'a Node<Span>> {
    let Node::Mapping { entries, loc, .. } = node else {
        return None;
    };

    // The mapping span must cover the cursor line.
    if !(idx.line_column(loc.start).0 as usize <= cursor.line
        && cursor.line <= idx.line_column(loc.end).0 as usize)
    {
        return None;
    }

    // Find an entry whose key column satisfies key.col <= cursor.col and
    // whose value is a nested mapping that covers the cursor line. Descend
    // into the deepest such mapping. Stop as soon as we find one that
    // admits descent.
    for (key_node, value_node) in entries {
        let Some(key_str) = scalar_key(key_node) else {
            continue;
        };
        let key_span = node_span(key_node);
        if idx.line_column(key_span.start).1 as usize > cursor.column {
            continue;
        }

        // This key's column satisfies the condition. Try to descend into its
        // value if it is also a Mapping whose keys satisfy the condition.
        if let Node::Mapping { .. } = value_node {
            let saved_len = path.len();
            path.push(key_str.to_string());
            if let Some(deeper) = deepest_mapping_at_column(value_node, cursor, path, idx) {
                return Some(deeper);
            }
            // Descent failed (value's entries too deep or span mismatch) — undo.
            path.truncate(saved_len);
        }
    }

    // No deeper mapping admitted descent. Check whether at least one entry's
    // key column satisfies the condition — if so, this mapping is the result.
    let has_eligible_entry = entries.iter().any(|(k, _)| {
        let key_span = node_span(k);
        idx.line_column(key_span.start).1 as usize <= cursor.column
    });
    if has_eligible_entry { Some(node) } else { None }
}

/// Return `true` if any mapping entry in `docs` has its key or value starting
/// on `cursor_parser_line` (1-based parser line number).
///
/// Used to prevent the blank-line extension from firing on non-blank lines
/// where the cursor is positioned past the end of content.
pub(super) fn cursor_line_has_mapping_content(
    docs: &[Document<Span>],
    cursor_parser_line: usize,
) -> bool {
    fn node_has_content_on_line(node: &Node<Span>, line: usize, idx: &LineIndex) -> bool {
        match node {
            Node::Mapping { entries, .. } => {
                for (key_node, value_node) in entries {
                    let key_span = node_span(key_node);
                    let value_span = node_span(value_node);
                    if idx.line_column(key_span.start).0 as usize == line
                        || (idx.line_column(value_span.start).0 as usize == line
                            && value_span.start != value_span.end)
                    {
                        return true;
                    }
                    if node_has_content_on_line(value_node, line, idx) {
                        return true;
                    }
                }
                false
            }
            Node::Sequence { items, .. } => items.iter().any(|item| {
                let span = node_span(item);
                idx.line_column(span.start).0 as usize == line
                    || node_has_content_on_line(item, line, idx)
            }),
            Node::Scalar { loc, .. } => {
                idx.line_column(loc.start).0 as usize == line && loc.start != loc.end
            }
            Node::Alias { .. } => false,
        }
    }
    docs.iter()
        .any(|doc| node_has_content_on_line(&doc.root, cursor_parser_line, doc.line_index()))
}

/// Determine the `CursorLocation` for a cursor position within `docs`.
///
/// Returns `OutsideAny` when the cursor cannot be placed inside any node
/// (empty document, past EOF, on a `---`/`...` separator, on a comment).
/// Otherwise returns the most-specific variant describing the cursor context.
pub(super) fn locate_cursor(docs: &[Document<Span>], position: Position) -> CursorLocation<'_> {
    if docs.is_empty() {
        return CursorLocation::OutsideAny;
    }

    let cursor = lsp_position_to_pos(position);

    // If cursor sits on a `---` or `...` separator line, return OutsideAny.
    // When a document has an explicit start marker, the marker is on the line
    // immediately before the root node's start line.
    for doc in docs {
        let idx = doc.line_index();
        let root_start = idx.line_column(node_span(&doc.root).start).0 as usize;
        if doc.explicit_start && root_start > 0 && cursor.line == root_start - 1 {
            return CursorLocation::OutsideAny;
        }
        if doc.explicit_end {
            let root_end = idx.line_column(node_span(&doc.root).end).0 as usize;
            if cursor.line == root_end {
                return CursorLocation::OutsideAny;
            }
        }
    }

    for doc in docs {
        let idx = doc.line_index();
        let result = locate_in_node(&doc.root, cursor, &mut Vec::new(), idx);
        if !matches!(result, CursorLocation::OutsideAny) {
            return result;
        }
    }

    // No node contained the cursor. Try the blank-line extension: walk
    // mappings whose span covers the cursor line and descend by column.
    // Skip this extension if the cursor line has actual mapping content —
    // positions past the end of a content-bearing line should return OutsideAny.
    if !cursor_line_has_mapping_content(docs, cursor.line) {
        for doc in docs {
            let idx = doc.line_index();
            let path: Vec<String> = Vec::new();
            if let Node::Mapping { loc, .. } = &doc.root {
                if idx.line_column(loc.start).0 as usize <= cursor.line
                    && cursor.line <= idx.line_column(loc.end).0 as usize
                {
                    let mut descent_path: Vec<String> = Vec::new();
                    if let Some(mapping) =
                        deepest_mapping_at_column(&doc.root, cursor, &mut descent_path, idx)
                    {
                        return CursorLocation::InBlankMapping {
                            enclosing_path: descent_path,
                            mapping,
                        };
                    }
                }
            } else if let Node::Sequence { loc, .. } = &doc.root {
                if idx.line_column(loc.start).0 as usize <= cursor.line
                    && cursor.line <= idx.line_column(loc.end).0 as usize
                {
                    return CursorLocation::InBlankSequence {
                        enclosing_path: path,
                        sequence: &doc.root,
                    };
                }
            }
        }
    }

    CursorLocation::OutsideAny
}

/// Recursively walk `node`, building `enclosing_path` as keys are descended.
/// Returns the most-specific `CursorLocation` for `cursor`, or `OutsideAny`
/// if the cursor is not inside `node`.
pub(super) fn locate_in_node<'a>(
    node: &'a Node<Span>,
    cursor: Pos,
    enclosing_path: &mut Vec<String>,
    idx: &LineIndex,
) -> CursorLocation<'a> {
    match node {
        Node::Mapping { entries, .. } => {
            for (key_node, value_node) in entries {
                let key_span = node_span(key_node);
                let value_span = node_span(value_node);

                if span_contains_cursor(key_span, cursor, idx) {
                    let key = scalar_key(key_node).unwrap_or("").to_string();
                    return CursorLocation::OnKey {
                        key,
                        enclosing_path: enclosing_path.clone(),
                        mapping: node,
                    };
                }

                if span_contains_cursor(value_span, cursor, idx) {
                    let key = scalar_key(key_node).unwrap_or("").to_string();
                    enclosing_path.push(key.clone());

                    // Recurse into the value.
                    let inner = locate_in_node(value_node, cursor, enclosing_path, idx);
                    if !matches!(inner, CursorLocation::OutsideAny) {
                        return inner;
                    }

                    // Value span contains cursor but no child matched.
                    // If the value is a Mapping, the cursor is on a blank/whitespace
                    // line inside that mapping — not on the scalar value.
                    if matches!(value_node, Node::Mapping { .. }) {
                        return CursorLocation::InBlankMapping {
                            enclosing_path: enclosing_path.clone(),
                            mapping: value_node,
                        };
                    }
                    // Similarly for Sequence.
                    if matches!(value_node, Node::Sequence { .. }) {
                        return CursorLocation::InBlankSequence {
                            enclosing_path: enclosing_path.clone(),
                            sequence: value_node,
                        };
                    }

                    enclosing_path.pop();

                    // Cursor is on the scalar value directly.
                    return CursorLocation::OnValue {
                        key,
                        enclosing_path: enclosing_path.clone(),
                    };
                }

                // Fallback A: cursor is on the same line as the key, past the key
                // span, and the value node's span starts on a DIFFERENT line.
                // This happens for null/empty values where the parser places the
                // value span at the start of the following line. Treat as OnValue.
                if cursor.line == idx.line_column(key_span.start).0 as usize
                    && cursor.column >= idx.line_column(key_span.end).1 as usize
                    && idx.line_column(value_span.start).0 as usize != cursor.line
                {
                    if let Some(key) = scalar_key(key_node) {
                        return CursorLocation::OnValue {
                            key: key.to_string(),
                            enclosing_path: enclosing_path.clone(),
                        };
                    }
                }
            }
            CursorLocation::OutsideAny
        }
        Node::Sequence { items, .. } => {
            for item in items {
                let item_span = node_span(item);
                if span_contains_cursor(item_span, cursor, idx) {
                    // Push "[]" so that inner mapping keys carry the sequence
                    // sentinel in their enclosing_path.
                    enclosing_path.push("[]".to_string());
                    let inner = locate_in_node(item, cursor, enclosing_path, idx);
                    if matches!(inner, CursorLocation::OutsideAny) {
                        enclosing_path.pop();
                        return CursorLocation::InSequenceItem {
                            enclosing_path: enclosing_path.clone(),
                            sequence: node,
                            current_item: item,
                        };
                    }
                    // inner already has the "[]" in path via enclosing_path
                    return inner;
                }
            }

            // Cursor in sequence span but not in any item — blank sequence line.
            if span_contains_cursor(node_span(node), cursor, idx) {
                return CursorLocation::InBlankSequence {
                    enclosing_path: enclosing_path.clone(),
                    sequence: node,
                };
            }

            CursorLocation::OutsideAny
        }
        Node::Scalar { .. } | Node::Alias { .. } => CursorLocation::OutsideAny,
    }
}

#[cfg(test)]
#[expect(clippy::wildcard_enum_match_arm, reason = "test code")]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::Position;

    use super::super::support::test_fixtures::pos;
    use super::{
        CursorLocation, cursor_line_has_mapping_content, locate_cursor, locate_in_node, node_span,
        scalar_key, span_contains_cursor,
    };
    use crate::test_utils::parse_docs;
    use rlsp_yaml_parser::node::Node;

    // ── locate_cursor: OnKey ─────────────────────────────────────────────────

    #[rstest]
    #[case::on_key_top_level("name: Alice\nage: 30\n", pos(0, 0), "name", vec![])]
    #[case::on_key_nested("server:\n  host: localhost\n", pos(1, 2), "host", vec!["server".to_string()])]
    #[case::on_key_in_sequence_item(
        "items:\n  - name: foo\n    age: 1\n",
        pos(1, 4),
        "name",
        vec!["items".to_string(), "[]".to_string()]
    )]
    #[case::on_key_utf8("café: latte\n", pos(0, 0), "café", vec![])]
    #[case::on_key_three_levels(
        "a:\n  b:\n    c: v\n",
        pos(2, 4),
        "c",
        vec!["a".to_string(), "b".to_string()]
    )]
    fn locate_cursor_on_key(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_key: &str,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::OnKey {
                key,
                enclosing_path,
                mapping,
            } => {
                assert_eq!(key, expected_key, "key mismatch");
                assert_eq!(enclosing_path, expected_path, "path mismatch");
                assert!(
                    matches!(mapping, Node::Mapping { .. }),
                    "mapping should be a Mapping node"
                );
            }
            other => panic!("expected OnKey, got different variant for yaml={yaml:?}: {other:?}"),
        }
    }

    // ── locate_cursor: OnValue ───────────────────────────────────────────────

    #[rstest]
    #[case::on_value_scalar("name: Alice\n", pos(0, 6), "name", vec![])]
    #[case::on_value_nested("server:\n  host: localhost\n", pos(1, 8), "host", vec!["server".to_string()])]
    fn locate_cursor_on_value(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_key: &str,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::OnValue {
                key,
                enclosing_path,
            } => {
                assert_eq!(key, expected_key, "key mismatch");
                assert_eq!(enclosing_path, expected_path, "path mismatch");
            }
            other => panic!("expected OnValue, got different variant for yaml={yaml:?}: {other:?}"),
        }
    }

    // ── locate_cursor: InBlankMapping ────────────────────────────────────────

    #[rstest]
    #[case::blank_mapping_root("name: Alice\n\nage: 30\n", pos(1, 0), vec![])]
    #[case::blank_mapping_nested("server:\n  host: localhost\n  \nport: 80\n", pos(2, 2), vec!["server".to_string()])]
    #[case::blank_mapping_eof("server:\n  host: localhost\n", pos(2, 2), vec!["server".to_string()])]
    #[case::blank_mapping_column_boundary(
        "outer:\n  inner:\n    key: val\n",
        pos(3, 2),
        vec!["outer".to_string()]
    )]
    #[case::blank_mapping_column_descent_deeper(
        "outer:\n  inner:\n    key: val\n",
        pos(3, 4),
        vec!["outer".to_string(), "inner".to_string()]
    )]
    fn locate_cursor_in_blank_mapping(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::InBlankMapping {
                enclosing_path,
                mapping,
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(mapping, Node::Mapping { .. }),
                    "mapping should be a Mapping node"
                );
            }
            other => panic!(
                "expected InBlankMapping, got different variant for yaml={yaml:?}: {other:?}"
            ),
        }
    }

    // ── locate_cursor: InBlankSequence ───────────────────────────────────────

    #[rstest]
    #[case::blank_sequence_after_scalar("items:\n  - foo\n  \n", pos(2, 2), vec!["items".to_string()])]
    fn locate_cursor_in_blank_sequence(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::InBlankSequence {
                enclosing_path,
                sequence,
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(sequence, Node::Sequence { .. }),
                    "sequence should be a Sequence node"
                );
            }
            other => panic!(
                "expected InBlankSequence, got different variant for yaml={yaml:?}: {other:?}"
            ),
        }
    }

    // ── locate_cursor: InSequenceItem ────────────────────────────────────────

    #[rstest]
    #[case::in_sequence_item_mapping_second_key(
        "items:\n  - name: foo\n    age: 1\n",
        pos(2, 4),
        vec!["items".to_string(), "[]".to_string()]
    )]
    fn locate_cursor_in_sequence_item(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::InSequenceItem {
                enclosing_path,
                sequence,
                ..
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(sequence, Node::Sequence { .. }),
                    "sequence should be a Sequence node"
                );
            }
            CursorLocation::OnKey {
                enclosing_path,
                mapping,
                ..
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(mapping, Node::Mapping { .. }),
                    "mapping should be a Mapping node"
                );
            }
            other => panic!(
                "expected InSequenceItem or OnKey, got different variant for yaml={yaml:?}: {other:?}"
            ),
        }
    }

    #[test]
    fn locate_cursor_in_sequence_item_scalar() {
        let yaml = "tags:\n  - rust\n  - yaml\n";
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, pos(1, 4));
        match loc {
            CursorLocation::InSequenceItem {
                enclosing_path,
                sequence,
                current_item,
            } => {
                assert_eq!(enclosing_path, vec!["tags".to_string()]);
                assert!(
                    matches!(sequence, Node::Sequence { .. }),
                    "sequence should be a Sequence node"
                );
                assert!(
                    matches!(current_item, Node::Scalar { .. }),
                    "current_item should be scalar"
                );
            }
            other => panic!("expected InSequenceItem, got: {other:?}"),
        }
    }

    // ── locate_cursor: OutsideAny ────────────────────────────────────────────

    #[rstest]
    #[case::empty_doc("", pos(0, 0))]
    #[case::past_eof("name: Alice\n", pos(5, 0))]
    #[case::on_separator("key1: v1\n---\nkey2: v2\n", pos(1, 0))]
    #[case::on_comment("# comment\nkey: val\n", pos(0, 2))]
    fn locate_cursor_outside_any(#[case] yaml: &str, #[case] position: Position) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        assert!(
            matches!(loc, CursorLocation::OutsideAny),
            "expected OutsideAny for yaml={yaml:?} position={position:?}"
        );
    }

    // ── locate_cursor: span_contains boundary cases ──────────────────────────

    #[test]
    fn locate_cursor_span_boundary_at_end_is_outside() {
        // The scalar "Alice" ends at some position; cursor exactly at span.end
        // should NOT be contained. We use a position clearly past any node.
        let yaml = "name: Alice\n";
        let docs = parse_docs(yaml);
        // Parser line 1, column 11 is one past "Alice" (col 6 start + 5 chars).
        // Use LSP pos(0, 11) to hit the boundary.
        let loc = locate_cursor(&docs, pos(0, 11));
        // Should be OutsideAny or InBlankMapping (not OnValue)
        assert!(
            !matches!(loc, CursorLocation::OnValue { .. }),
            "cursor at span.end should not be OnValue"
        );
    }

    #[test]
    fn locate_cursor_span_boundary_at_start_is_contained() {
        // Cursor at span.start should be contained.
        let yaml = "name: Alice\n";
        let docs = parse_docs(yaml);
        // "name" key starts at parser line=1, col=0 → LSP pos(0, 0)
        let loc = locate_cursor(&docs, pos(0, 0));
        assert!(
            matches!(loc, CursorLocation::OnKey { .. }),
            "cursor at span.start should be OnKey"
        );
    }

    // ── locate_cursor: additional cases ─────────────────────────────────────

    #[test]
    fn locate_cursor_on_key_at_end_of_key_token() {
        // LC-2: cursor at last char of "name" key token
        let yaml = "name: Alice\n";
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, pos(0, 3));
        assert!(
            matches!(loc, CursorLocation::OnKey { ref key, .. } if key == "name"),
            "cursor at end of key token should still be OnKey"
        );
    }

    // ── node_span and scalar_key ─────────────────────────────────────────────

    #[test]
    fn node_span_returns_scalar_loc() {
        let yaml = "key: value\n";
        let docs = parse_docs(yaml);
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping");
        };
        let (key_node, _) = &entries[0];
        let span = node_span(key_node);
        // span is non-zero (key has content)
        assert!(span.start < span.end, "scalar span should be non-empty");
    }

    #[test]
    fn scalar_key_returns_string_for_scalar() {
        let yaml = "key: value\n";
        let docs = parse_docs(yaml);
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping");
        };
        let (key_node, _) = &entries[0];
        assert_eq!(scalar_key(key_node), Some("key"));
    }

    #[test]
    fn scalar_key_returns_none_for_mapping_node() {
        // A Mapping used as root node — scalar_key returns None for it
        let yaml = "key: value\n";
        let docs = parse_docs(yaml);
        // The root is a Mapping; passing it returns None
        assert!(scalar_key(&docs[0].root).is_none());
    }

    // ── span_contains_cursor ─────────────────────────────────────────────────

    #[test]
    fn span_contains_cursor_includes_start_excludes_end() {
        let yaml = "key: value\n";
        let docs = parse_docs(yaml);
        let idx = docs[0].line_index();
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping");
        };
        let (key_node, _) = &entries[0];
        let span = node_span(key_node);
        let start_pos = {
            let (l, c) = idx.line_column(span.start);
            rlsp_yaml_parser::Pos {
                byte_offset: span.start as usize,
                line: l as usize,
                column: c as usize,
            }
        };
        let end_pos = {
            let (l, c) = idx.line_column(span.end);
            rlsp_yaml_parser::Pos {
                byte_offset: span.end as usize,
                line: l as usize,
                column: c as usize,
            }
        };
        assert!(
            span_contains_cursor(span, start_pos, idx),
            "start pos should be contained"
        );
        assert!(
            !span_contains_cursor(span, end_pos, idx),
            "end pos should NOT be contained"
        );
    }

    // ── cursor_line_has_mapping_content ──────────────────────────────────────

    #[test]
    fn cursor_line_has_content_returns_true_for_content_line() {
        let yaml = "key: value\n";
        let docs = parse_docs(yaml);
        // Parser line 1 (1-based) has the mapping entry
        assert!(
            cursor_line_has_mapping_content(&docs, 1),
            "line 1 should have content"
        );
    }

    #[test]
    fn cursor_line_has_content_returns_false_for_blank_line() {
        let yaml = "key: value\n\n";
        let docs = parse_docs(yaml);
        // Parser line 2 (1-based) is blank
        assert!(
            !cursor_line_has_mapping_content(&docs, 2),
            "blank line should not have content"
        );
    }

    // ── locate_in_node ───────────────────────────────────────────────────────

    #[test]
    fn locate_in_node_returns_outside_any_for_scalar_node() {
        let yaml = "key: value\n";
        let docs = parse_docs(yaml);
        let idx = docs[0].line_index();
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping");
        };
        let (_, value_node) = &entries[0];
        // value_node is a Scalar; locate_in_node should return OutsideAny
        let cursor = rlsp_yaml_parser::Pos {
            byte_offset: 0,
            line: 5,
            column: 0,
        };
        let result = locate_in_node(value_node, cursor, &mut Vec::new(), idx);
        assert!(
            matches!(result, CursorLocation::OutsideAny),
            "scalar node with non-contained cursor should return OutsideAny"
        );
    }
}
