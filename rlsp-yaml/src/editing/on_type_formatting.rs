// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{ScalarStyle, Span};
use tower_lsp::lsp_types::{Position, Range, TextEdit};

/// Compute text edits for on-type formatting when a newline is typed.
///
/// Returns indentation edits for the new line based on the AST context of the
/// previous line. Only handles `ch == "\n"` — returns an empty vec for
/// anything else.
#[must_use]
pub fn format_on_type(
    docs: &[Document<Span>],
    position: Position,
    ch: &str,
    tab_size: u32,
) -> Vec<TextEdit> {
    if ch != "\n" {
        return Vec::new();
    }

    if position.line == 0 {
        return Vec::new();
    }

    let tab_size = if tab_size == 0 { 2 } else { tab_size as usize };

    // LSP line is 0-based; parser line is 1-based.
    // The trigger was typed at the end of the previous LSP line = position.line - 1.
    let prev_ast_line = position.line as usize; // (position.line - 1) + 1

    let indent_level = indent_for_prev_line(docs, prev_ast_line, tab_size).unwrap_or(0);

    vec![TextEdit {
        range: Range::new(
            Position::new(position.line, 0),
            Position::new(position.line, position.character),
        ),
        new_text: " ".repeat(indent_level),
    }]
}

/// Walk all documents to determine the indentation for the line following
/// `prev_ast_line` (1-based parser line number).
fn indent_for_prev_line(
    docs: &[Document<Span>],
    prev_ast_line: usize,
    tab_size: usize,
) -> Option<usize> {
    for doc in docs {
        if let Some(indent) = node_indent(&doc.root, prev_ast_line, tab_size) {
            return Some(indent);
        }
    }
    None
}

/// Recursively inspect `node` to determine the appropriate indentation for
/// the line following `prev_ast_line`.
///
/// Returns `Some(column)` when this subtree provides context for the target
/// line, or `None` when the line falls outside this subtree.
fn node_indent(node: &Node<Span>, prev_ast_line: usize, tab_size: usize) -> Option<usize> {
    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                let key_line = node_start_line(key);
                let val_line = node_start_line(value);

                // Case 1: key and value on the same line as prev_ast_line.
                if key_line == prev_ast_line && val_line == prev_ast_line {
                    // Block scalar on the same line as the key → extra indent.
                    if is_block_scalar_node(value) {
                        return Some(key_line_column(key) + tab_size);
                    }
                    // Recurse into the value in case it is a nested collection
                    // on the same line.
                    if let Some(inner) = node_indent(value, prev_ast_line, tab_size) {
                        return Some(inner);
                    }
                    // Plain inline value → indent to the key's column (no extra).
                    return Some(key_line_column(key));
                }

                // Case 2: key is on prev_ast_line but value starts later → bare key.
                if key_line == prev_ast_line && val_line > prev_ast_line {
                    return Some(key_line_column(key) + tab_size);
                }

                // Case 3: key is before prev_ast_line, value starts on or after →
                // cursor is in the "gap" between a key and its deferred block value,
                // or within the value's subtree.
                if key_line < prev_ast_line && val_line > prev_ast_line {
                    return Some(key_line_column(key) + tab_size);
                }

                // Case 4: value starts at or before prev_ast_line → recurse into value.
                if val_line <= prev_ast_line {
                    if let Some(inner) = node_indent(value, prev_ast_line, tab_size) {
                        return Some(inner);
                    }
                }
            }
            None
        }
        Node::Sequence { items, loc, .. } => {
            let seq_col = loc.start.column;
            for item in items {
                let item_line = node_start_line(item);
                if item_line == prev_ast_line {
                    return Some(seq_col);
                }
                // Recurse into nested items.
                if let Some(inner) = node_indent(item, prev_ast_line, tab_size) {
                    return Some(inner);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

/// Return the 1-based parser line on which `node` starts.
const fn node_start_line(node: &Node<Span>) -> usize {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => loc.start.line,
    }
}

/// Return the 0-based column of the key node's start position.
const fn key_line_column(key: &Node<Span>) -> usize {
    match key {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => loc.start.column,
    }
}

/// Return `true` when the node is a block scalar (literal `|` or folded `>`).
const fn is_block_scalar_node(node: &Node<Span>) -> bool {
    matches!(
        node,
        Node::Scalar {
            style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
            ..
        }
    )
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn indent_of(edit: &TextEdit) -> usize {
        edit.new_text.len()
    }

    fn parse_docs(yaml: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(yaml).unwrap_or_default()
    }

    // Group: format_on_type_produces_indent — single edit, assert indent size
    #[rstest]
    #[case::bare_mapping_key("key:\n", pos(1, 0), "\n", 2, 2)]
    #[case::complete_key_value_pair("key: value\n", pos(1, 0), "\n", 2, 0)]
    #[case::indented_bare_key("  key:\n", pos(1, 0), "\n", 2, 4)]
    #[case::sequence_item("- item\n", pos(1, 0), "\n", 2, 0)]
    #[case::literal_block_scalar("key: |\n", pos(1, 0), "\n", 2, 2)]
    #[case::folded_block_scalar("key: >\n", pos(1, 0), "\n", 2, 2)]
    #[case::block_scalar_strip_chomping("key: |-\n", pos(1, 0), "\n", 2, 2)]
    #[case::folded_block_scalar_strip_chomping("key: >-\n", pos(1, 0), "\n", 2, 2)]
    #[case::block_scalar_keep_chomping("key: |+\n", pos(1, 0), "\n", 2, 2)]
    // Comment-only document has no structural AST nodes; falls back to 0.
    #[case::comment_line_maintains_indent("  # a comment\n", pos(1, 0), "\n", 2, 0)]
    #[case::empty_prev_line_fallback("key:\n\n", pos(2, 0), "\n", 2, 2)]
    // New regression cases (a), (b), (c)
    #[case::mapping_value_indent_from_parent_column("parent:\n  child:\n", pos(2, 0), "\n", 2, 4)]
    #[case::sequence_item_indent_from_parent_column("items:\n  - first\n", pos(2, 0), "\n", 2, 2)]
    #[case::block_scalar_indicator_extra_indent_from_node_column("key: |\n", pos(1, 0), "\n", 2, 2)]
    fn format_on_type_produces_indent(
        #[case] text: &str,
        #[case] position: Position,
        #[case] ch: &str,
        #[case] tab_size: u32,
        #[case] expected_indent: usize,
    ) {
        let docs = parse_docs(text);
        let edits = format_on_type(&docs, position, ch, tab_size);
        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), expected_indent);
    }

    // Group: format_on_type_returns_empty — assert edits.is_empty()
    #[rstest]
    #[case::non_newline_character("key:\n", pos(1, 0), "a", 2)]
    #[case::position_at_line_zero("key: value\n", pos(0, 0), "\n", 2)]
    #[case::empty_text("", pos(0, 0), "\n", 2)]
    fn format_on_type_returns_empty(
        #[case] text: &str,
        #[case] position: Position,
        #[case] ch: &str,
        #[case] tab_size: u32,
    ) {
        let docs = parse_docs(text);
        let edits = format_on_type(&docs, position, ch, tab_size);
        assert!(edits.is_empty());
    }

    // Different tab_size values work correctly
    #[test]
    fn should_respect_tab_size_parameter() {
        let docs = parse_docs("key:\n");

        let edits_4 = format_on_type(&docs, pos(1, 0), "\n", 4);
        assert_eq!(edits_4.len(), 1);
        assert_eq!(indent_of(&edits_4[0]), 4);

        let edits_0 = format_on_type(&docs, pos(1, 0), "\n", 0);
        assert_eq!(edits_0.len(), 1);
        // tab_size 0 treated as 2
        assert_eq!(indent_of(&edits_0[0]), 2);
    }

    // TextEdit range covers col 0 to cursor on the new line
    #[test]
    fn edit_range_replaces_existing_characters_on_new_line() {
        // Simulate the cursor at column 3 (some pre-existing chars on the line)
        let docs = parse_docs("key:\n   \n");
        let edits = format_on_type(&docs, pos(1, 3), "\n", 2);

        assert_eq!(edits.len(), 1);
        let edit = &edits[0];
        assert_eq!(edit.range.start, Position::new(1, 0));
        assert_eq!(edit.range.end, Position::new(1, 3));
    }

    // Edge case: empty docs returns empty vec
    #[test]
    fn empty_docs_returns_empty_vec() {
        let edits = format_on_type(&[], pos(0, 0), "\n", 2);
        assert!(edits.is_empty());
    }

    // Edge case: position beyond document end falls back gracefully (no panic)
    #[test]
    fn position_outside_all_node_spans_falls_back_gracefully() {
        let docs = parse_docs("key: value\n");
        let edits = format_on_type(&docs, pos(5, 0), "\n", 2);
        assert!(
            edits.len() <= 1,
            "expected 0 or 1 edit, got {}",
            edits.len()
        );
    }

    // Edge case: non-newline trigger returns empty with new signature
    #[test]
    fn non_newline_trigger_returns_empty_with_ast_signature() {
        let docs = parse_docs("key: value\n");
        let edits = format_on_type(&docs, pos(1, 0), "a", 2);
        assert!(edits.is_empty());
    }
}
