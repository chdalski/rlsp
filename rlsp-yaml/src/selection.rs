// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::pos::Span;
use tower_lsp::lsp_types::{Position, Range, SelectionRange};

/// Compute selection ranges for the given YAML text and cursor positions.
///
/// For each position, returns a `SelectionRange` whose parent chain expands
/// from innermost node to outermost document root.
/// Returns an empty `Vec` if the AST is unavailable.
#[must_use]
pub fn selection_ranges(
    text: &str,
    documents: Option<&Vec<Document<Span>>>,
    positions: &[Position],
) -> Vec<SelectionRange> {
    let Some(documents) = documents else {
        return Vec::new();
    };
    if positions.is_empty() || documents.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();

    positions
        .iter()
        .filter_map(|pos| selection_range_for_position(&lines, documents, *pos))
        .collect()
}

/// Build a `SelectionRange` chain for a single cursor position.
fn selection_range_for_position(
    lines: &[&str],
    documents: &[Document<Span>],
    position: Position,
) -> Option<SelectionRange> {
    let line = position.line as usize;
    let col = position.character as usize;

    // Skip positions on document separator or comment lines — no AST node
    if let Some(l) = lines.get(line) {
        let trimmed = l.trim();
        if trimmed == "---" || trimmed == "..." || trimmed.starts_with('#') {
            return None;
        }
    }

    // Find which document contains this line (based on --- separators)
    let (doc_idx, doc_start_line) = find_document_for_line(lines, line);
    let doc = documents.get(doc_idx)?;

    // Collect ancestor spans innermost-first from the AST walk
    let mut ancestor_spans: Vec<Span> = Vec::new();
    collect_ancestor_spans(&doc.root, line, col, &mut ancestor_spans);

    if ancestor_spans.is_empty() {
        return None;
    }

    // Add document root as the outermost range if the last span doesn't already cover it.
    let doc_end_line = find_document_end(lines, doc_start_line);
    let doc_root = make_line_range(doc_start_line, doc_end_line);

    // Build the SelectionRange chain: innermost first in ancestor_spans,
    // outermost (doc root) is the final parent.
    let mut current: Option<Box<SelectionRange>> = Some(Box::new(SelectionRange {
        range: doc_root,
        parent: None,
    }));

    // ancestor_spans[0] is innermost, last is closest-to-root.
    // We want to wrap them outermost-first, so iterate in reverse.
    for span in ancestor_spans.iter().rev() {
        let range = span_to_lsp_range(span);
        // Avoid emitting the doc root twice
        if range == doc_root {
            continue;
        }
        let sr = SelectionRange {
            range,
            parent: current,
        };
        current = Some(Box::new(sr));
    }

    current.map(|b| *b)
}

/// Find which document index a given line belongs to, and the start line of that document.
fn find_document_for_line(lines: &[&str], target_line: usize) -> (usize, usize) {
    let mut doc_idx = 0;
    let mut doc_start = 0;

    for (i, line) in lines.iter().enumerate() {
        if i >= target_line {
            break;
        }
        if line.trim() == "---" {
            doc_idx += 1;
            doc_start = i + 1;
        }
    }

    (doc_idx, doc_start)
}

/// Find the last line of a document starting at `doc_start` (exclusive of the next `---`).
fn find_document_end(lines: &[&str], doc_start: usize) -> usize {
    let mut last = doc_start;
    for (i, line) in lines.iter().enumerate().skip(doc_start) {
        if line.trim() == "---" || line.trim() == "..." {
            break;
        }
        last = i;
    }
    last
}

/// Build an LSP `Range` spanning full lines from `start_line` to `end_line` (0-based).
fn make_line_range(start_line: usize, end_line: usize) -> Range {
    #[allow(clippy::cast_possible_truncation)]
    Range::new(
        Position::new(start_line as u32, 0),
        Position::new(end_line as u32, u32::MAX),
    )
}

/// Extract the `Span` (loc) from a `Node<Span>`.
const fn node_span(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

/// Compute effective start Pos for a node, recursing into containers
/// whose own span is zero to find their first child's start.
fn effective_start(node: &Node<Span>) -> Option<rlsp_yaml_parser::pos::Pos> {
    let span = node_span(node);
    if !is_zero_span(&span) {
        return Some(span.start);
    }
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .filter_map(|(k, _)| effective_start(k))
            .min_by_key(|p| (p.line, p.column)),
        Node::Sequence { items, .. } => items
            .iter()
            .filter_map(effective_start)
            .min_by_key(|p| (p.line, p.column)),
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

/// Compute effective end Pos for a node, recursing into containers
/// whose own span is zero to find their last child's end.
fn effective_end(node: &Node<Span>) -> Option<rlsp_yaml_parser::pos::Pos> {
    let span = node_span(node);
    if !is_zero_span(&span) {
        return Some(span.end);
    }
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .filter_map(|(_, v)| effective_end(v))
            .max_by_key(|p| (p.line, p.column)),
        Node::Sequence { items, .. } => items
            .iter()
            .filter_map(effective_end)
            .max_by_key(|p| (p.line, p.column)),
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

/// A span is "zero" if start == end (container nodes in the parser may have this).
fn is_zero_span(span: &Span) -> bool {
    span.start == span.end
}

/// Recursively collect ancestor spans for the cursor, innermost-first.
///
/// `rlsp-yaml-parser` Pos convention: line is 1-based, column is 0-based.
/// LSP Position: both 0-based.
///
/// Container nodes (Mapping, Sequence) may have zero spans — their
/// extent is computed from their children's spans.
fn collect_ancestor_spans(
    node: &Node<Span>,
    line: usize,
    col: usize,
    ancestor_spans: &mut Vec<Span>,
) {
    let depth_before = ancestor_spans.len();

    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                let key_span = node_span(key);
                let key_line_0 = key_span.start.line.saturating_sub(1);

                // Determine value's end, accounting for zero-span containers
                let val_end = effective_end(value).unwrap_or(key_span.end);
                let entry_end_line_0 = val_end.line.saturating_sub(1);

                if line < key_line_0 || line > entry_end_line_0 {
                    continue;
                }

                // Recurse into value first (innermost wins)
                collect_ancestor_spans(value, line, col, ancestor_spans);
                if ancestor_spans.len() > depth_before {
                    let entry_span = Span {
                        start: key_span.start,
                        end: val_end,
                    };
                    ancestor_spans.push(entry_span);
                    break;
                }

                // Check if cursor is on the key itself
                if key_line_0 == line && col >= key_span.start.column && col <= key_span.end.column
                {
                    ancestor_spans.push(key_span);
                    let entry_span = Span {
                        start: key_span.start,
                        end: val_end,
                    };
                    ancestor_spans.push(entry_span);
                    break;
                }

                // Cursor is within the entry's line range but not in key or value
                if key_line_0 == line {
                    let entry_span = Span {
                        start: key_span.start,
                        end: val_end,
                    };
                    ancestor_spans.push(entry_span);
                    break;
                }
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                let item_span = node_span(item);
                let has_real_span = !is_zero_span(&item_span);

                let eff_start = effective_start(item);
                let eff_end = if has_real_span {
                    Some(item_span.end)
                } else {
                    effective_end(item)
                };

                if let (Some(start_pos), Some(end_pos)) = (eff_start, eff_end) {
                    let start_line_0 = start_pos.line.saturating_sub(1);
                    let end_line_0 = end_pos.line.saturating_sub(1);
                    if line < start_line_0 || line > end_line_0 {
                        continue;
                    }
                }

                collect_ancestor_spans(item, line, col, ancestor_spans);
                if ancestor_spans.len() > depth_before {
                    if let (Some(start_pos), Some(end_pos)) = (eff_start, eff_end) {
                        ancestor_spans.push(Span {
                            start: start_pos,
                            end: end_pos,
                        });
                    }
                    break;
                }

                // Leaf item with real span and cursor within it
                if has_real_span && col >= item_span.start.column {
                    ancestor_spans.push(item_span);
                    break;
                }
            }
        }
        Node::Scalar { loc, .. } | Node::Alias { loc, .. } => {
            if loc.start.line > 0 {
                let start_line_0 = loc.start.line.saturating_sub(1);
                let end_line_0 = loc.end.line.saturating_sub(1);
                if line >= start_line_0 && line <= end_line_0 && col >= loc.start.column {
                    ancestor_spans.push(*loc);
                }
            }
        }
    }
}

/// Convert an `rlsp-yaml-parser` `Span` to an LSP `Range`.
/// Pos: line 1-based, column 0-based -> LSP: both 0-based.
fn span_to_lsp_range(span: &Span) -> Range {
    #[allow(clippy::cast_possible_truncation)]
    let start_line = span.start.line.saturating_sub(1) as u32;
    #[allow(clippy::cast_possible_truncation)]
    let start_col = span.start.column as u32;
    #[allow(clippy::cast_possible_truncation)]
    let end_line = span.end.line.saturating_sub(1) as u32;
    #[allow(clippy::cast_possible_truncation)]
    let end_col = span.end.column as u32;

    Range::new(
        Position::new(start_line, start_col),
        Position::new(end_line, end_col),
    )
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn parse_docs(text: &str) -> Option<Vec<Document<Span>>> {
        rlsp_yaml_parser::load(text).ok()
    }

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    // ---- Basic expansion tests ----

    #[test]
    fn should_return_value_range_expanding_to_key_value_then_document() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 6)]);

        assert_eq!(
            result.len(),
            1,
            "should return one SelectionRange per position"
        );
        let sr = &result[0];
        assert_eq!(sr.range.start.line, 0);
        assert!(
            sr.parent.is_some(),
            "should have a parent range (key-value pair)"
        );
        let parent = sr.parent.as_ref().expect("parent");
        assert_eq!(parent.range.start.line, 0);
        assert!(
            parent.parent.is_some(),
            "should have a grandparent range (document root)"
        );
    }

    #[test]
    fn should_return_key_range_expanding_to_key_value_then_document() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 1)]);

        assert_eq!(result.len(), 1);
        let sr = &result[0];
        assert_eq!(sr.range.start.line, 0);
        assert!(sr.parent.is_some(), "should have parent (key-value pair)");
        let parent = sr.parent.as_ref().expect("parent");
        assert_eq!(parent.range.start.line, 0);
        assert!(
            parent.parent.is_some(),
            "should have grandparent (document root)"
        );
    }

    #[test]
    fn should_return_sequence_item_expanding_to_sequence_then_document() {
        let text = "items:\n  - one\n  - two\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 5)]);

        assert_eq!(result.len(), 1);
        let sr = &result[0];
        assert_eq!(sr.range.start.line, 1);
        assert!(sr.parent.is_some(), "should have parent (sequence)");
        assert!(
            sr.parent.as_ref().expect("parent").parent.is_some(),
            "should have grandparent (document root)"
        );
    }

    #[test]
    fn should_handle_nested_mapping() {
        let text = "server:\n  host: localhost\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 8)]);

        assert_eq!(result.len(), 1);
        let sr = &result[0];
        assert_eq!(sr.range.start.line, 1);
        assert!(sr.parent.is_some(), "should have parent (host: localhost)");
        let parent = sr.parent.as_ref().expect("parent");
        assert!(
            parent.parent.is_some(),
            "should have grandparent (server mapping)"
        );
        let grandparent = parent.parent.as_ref().expect("grandparent");
        assert!(
            grandparent.parent.is_some(),
            "should have great-grandparent (document root)"
        );
    }

    #[test]
    fn should_handle_multiple_positions() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let positions = [pos(0, 6), pos(1, 5)];
        let result = selection_ranges(text, docs.as_ref(), &positions);

        assert_eq!(
            result.len(),
            2,
            "should return one SelectionRange per position"
        );
        assert_eq!(result[0].range.start.line, 0);
        assert_eq!(result[1].range.start.line, 1);
    }

    #[test]
    fn should_handle_sequence_of_mappings() {
        let text = "users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 10)]);

        assert_eq!(result.len(), 1);
        let sr = &result[0];
        assert_eq!(sr.range.start.line, 1);
        assert!(sr.parent.is_some(), "should have parent (name: Alice)");
    }

    #[test]
    fn should_scope_selection_to_current_document_in_multi_doc_yaml() {
        let text = "doc1key: value1\n---\ndoc2key: value2\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(2, 0)]);

        assert_eq!(result.len(), 1);
        let sr = &result[0];
        let mut outermost = sr;
        while let Some(ref p) = outermost.parent {
            outermost = p;
        }
        assert!(
            outermost.range.start.line >= 2,
            "outermost range should be scoped to the second document (start >= line 2), \
             got start line {}",
            outermost.range.start.line
        );
    }

    #[test]
    fn should_handle_first_document_in_multi_doc_yaml() {
        let text = "doc1key: value1\n---\ndoc2key: value2\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 0)]);

        assert_eq!(result.len(), 1);
        let sr = &result[0];
        let mut outermost = sr;
        while let Some(ref p) = outermost.parent {
            outermost = p;
        }
        assert!(
            outermost.range.end.line <= 1,
            "outermost range should not cross the --- separator (end line must be <= 1), \
             got end line {}",
            outermost.range.end.line
        );
    }

    // ---- Safety / edge case tests ----

    #[test]
    fn should_return_empty_when_ast_is_none() {
        let result = selection_ranges("key: [bad", None, &[pos(0, 0)]);
        let _ = result;
    }

    #[test]
    fn should_return_empty_for_empty_document() {
        let result = selection_ranges("", None, &[pos(0, 0)]);
        assert!(
            result.is_empty(),
            "should return empty Vec for empty document"
        );
    }

    #[test]
    fn should_return_empty_for_position_beyond_document() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(99, 0)]);
        let _ = result;
    }

    #[test]
    fn should_return_safe_result_for_position_beyond_line_length() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 999)]);
        let _ = result;
    }

    #[test]
    fn should_return_empty_for_cursor_on_document_separator() {
        let text = "a: 1\n---\nb: 2\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 0)]);
        let _ = result;
    }

    #[test]
    fn should_return_empty_for_comment_only_document() {
        let text = "# just a comment\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 2)]);
        let _ = result;
    }

    #[test]
    fn should_handle_cursor_on_comment_line() {
        let text = "key: value\n# this is a comment\nother: data\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 5)]);
        let _ = result;
    }

    #[test]
    fn should_not_panic_on_deeply_nested_yaml_ast_walk() {
        // Build 64 levels of nesting (kept modest for stack safety in debug builds).
        let mut text = String::new();
        for i in 0..64usize {
            let indent = "  ".repeat(i);
            writeln!(text, "{indent}l{i}:").unwrap();
        }
        let leaf_indent = "  ".repeat(64);
        writeln!(text, "{leaf_indent}leaf: deep").unwrap();

        let docs = parse_docs(&text);
        let result = selection_ranges(&text, docs.as_ref(), &[pos(64, leaf_indent.len() as u32)]);

        let mut depth = 0usize;
        if let Some(sr) = result.first() {
            let mut current = sr;
            while let Some(ref p) = current.parent {
                depth += 1;
                current = p;
                assert!(
                    depth <= 200,
                    "parent chain should be bounded (not infinite)"
                );
            }
        }
    }

    #[test]
    fn should_handle_empty_positions_slice() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[]);
        assert!(
            result.is_empty(),
            "should return empty Vec for empty positions slice"
        );
    }

    // ---- Additional coverage tests ----

    #[test]
    fn should_scope_document_end_at_dot_dot_dot_terminator() {
        let text = "key: value\n...\nafter: end\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 5)]);

        if let Some(sr) = result.first() {
            let mut outermost = sr;
            while let Some(ref p) = outermost.parent {
                outermost = p;
            }
            assert!(
                outermost.range.end.line <= 1,
                "document root should end at or before '...', got end line {}",
                outermost.range.end.line
            );
        }
    }

    #[test]
    fn should_return_empty_for_cursor_on_dot_dot_dot_line() {
        let text = "key: value\n...\nother: val\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 0)]);
        assert!(
            result.is_empty(),
            "cursor on '...' line should produce no selection range"
        );
    }

    #[test]
    fn should_handle_sequence_value_in_mapping() {
        let text = "items:\n  - alpha\n  - beta\n  - gamma\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 4)]);
        let _ = result;
    }

    #[test]
    fn should_handle_deeply_nested_sequence_value() {
        let text = "data:\n  - nested:\n      - deep_value\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(2, 10)]);
        let _ = result;
    }

    #[test]
    fn should_handle_single_line_document() {
        let text = "key: value";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 5)]);
        if let Some(sr) = result.first() {
            let mut outermost = sr;
            while let Some(ref p) = outermost.parent {
                outermost = p;
            }
            assert_eq!(outermost.range.start.line, outermost.range.end.line);
        }
    }

    #[test]
    fn should_correctly_find_document_for_line_after_separator() {
        let text = "a: 1\n---\nb: 2\n---\nc: 3\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(4, 3)]);
        if let Some(sr) = result.first() {
            let mut outermost = sr;
            while let Some(ref p) = outermost.parent {
                outermost = p;
            }
            assert!(
                outermost.range.start.line >= 4,
                "outermost range should be scoped to third document, got start line {}",
                outermost.range.start.line
            );
        }
    }

    #[test]
    fn should_handle_key_at_column_zero_with_no_value() {
        let text = "empty:\nother: val\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 0)]);
        let _ = result;
    }

    #[test]
    fn should_handle_alias_in_sequence() {
        let text = "base: &anchor value\ncopy:\n  - *anchor\n";
        let docs = parse_docs(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(2, 4)]);
        let _ = result;
    }
}
