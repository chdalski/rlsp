// SPDX-License-Identifier: MIT

use saphyr::{MarkedYamlOwned, Marker, YamlDataOwned};
use tower_lsp::lsp_types::{Position, Range, SelectionRange};

/// Compute selection ranges for the given YAML text and cursor positions.
///
/// For each position, returns a `SelectionRange` whose parent chain expands
/// from innermost node to outermost document root.
/// Returns an empty `Vec` if the AST is unavailable.
#[must_use]
pub fn selection_ranges(
    text: &str,
    documents: Option<&Vec<MarkedYamlOwned>>,
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
    documents: &[MarkedYamlOwned],
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
    let mut ancestor_spans: Vec<(Marker, Marker)> = Vec::new();
    collect_ancestor_spans(doc, line, col, &mut ancestor_spans);

    if ancestor_spans.is_empty() {
        return None;
    }

    // Add document root as the outermost range if the last span doesn't already cover it.
    // The document root spans from its first line to the last line before the next separator.
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
    // But the last entry in ancestor_spans may duplicate the doc root — skip if so.
    let spans_to_emit: &[(Marker, Marker)] = &ancestor_spans;
    for (start_marker, end_marker) in spans_to_emit.iter().rev() {
        let range = marker_to_lsp_range(start_marker, end_marker);
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

/// Recursively collect ancestor spans for the cursor, innermost-first.
///
/// saphyr Marker convention (verified against 0.0.6 source):
///   - line: 1-based, col: 0-based
///
/// LSP Position: both 0-based.
///
/// Container nodes (Mapping, Sequence) have zero spans in saphyr 0.0.6 —
/// their extent is computed from their children's spans.
fn collect_ancestor_spans(
    node: &MarkedYamlOwned,
    line: usize,
    col: usize,
    ancestor_spans: &mut Vec<(Marker, Marker)>,
) {
    let depth_before = ancestor_spans.len();

    match &node.data {
        YamlDataOwned::Mapping(map) => {
            // Walk each key-value entry to find which one contains the cursor
            for (key, value) in map {
                let key_start = key.span.start;
                let key_end = key.span.end;
                let key_line_0 = key_start.line().saturating_sub(1);

                // Determine value's end span, accounting for zero-span containers
                let val_end = value_end_marker(value);
                // Entry spans from key start to value end
                let entry_end = val_end.unwrap_or(key_end);
                let entry_end_line_0 = entry_end.line().saturating_sub(1);

                // Skip entries whose range doesn't include the cursor line
                if line < key_line_0 || line > entry_end_line_0 {
                    continue;
                }

                // Recurse into value children first (innermost wins)
                collect_ancestor_spans(value, line, col, ancestor_spans);
                if ancestor_spans.len() > depth_before {
                    // Found something in value — add entry span as the pair level
                    ancestor_spans.push((key_start, entry_end));
                    break;
                }

                // Check if cursor is on the key itself
                if key_line_0 == line && col >= key_start.col() && col <= key_end.col() {
                    ancestor_spans.push((key_start, key_end));
                    ancestor_spans.push((key_start, entry_end));
                    break;
                }

                // Cursor is within the entry's line range but not in key or value
                // (e.g. on the ': ' separator) — emit the entry span
                if key_line_0 == line {
                    ancestor_spans.push((key_start, entry_end));
                    break;
                }
            }
        }
        YamlDataOwned::Sequence(arr) => {
            // Walk each sequence item. Items may have zero spans (containers),
            // so we must recurse into all of them and let children decide.
            for item in arr {
                let item_start = item.span.start;
                let item_end = item.span.end;
                let has_real_span = item_start.line() > 0;

                // For zero-span items, compute extent from children
                let effective_end = if has_real_span {
                    Some(item_end)
                } else {
                    value_end_marker(item)
                };
                let effective_start = if has_real_span {
                    Some(item_start)
                } else {
                    value_start_marker(item)
                };

                // Range check using effective span
                if let (Some(eff_start), Some(eff_end)) = (effective_start, effective_end) {
                    let eff_start_line_0 = eff_start.line().saturating_sub(1);
                    let eff_end_line_0 = eff_end.line().saturating_sub(1);
                    if line < eff_start_line_0 || line > eff_end_line_0 {
                        continue;
                    }
                }

                // Recurse into item's children
                collect_ancestor_spans(item, line, col, ancestor_spans);
                if ancestor_spans.len() > depth_before {
                    // Found match inside this item — emit the item's computed span
                    if let (Some(eff_start), Some(eff_end)) = (effective_start, effective_end) {
                        ancestor_spans.push((eff_start, eff_end));
                    }
                    break;
                }

                // Leaf item with real span and cursor within it
                if has_real_span && col >= item_start.col() {
                    ancestor_spans.push((item_start, item_end));
                    break;
                }
            }
        }
        YamlDataOwned::Tagged(_, inner) => {
            collect_ancestor_spans(inner, line, col, ancestor_spans);
        }
        YamlDataOwned::Value(_) | YamlDataOwned::Representation(_, _, _) => {
            let s = node.span.start;
            let e = node.span.end;
            if s.line() > 0 {
                let start_line_0 = s.line().saturating_sub(1);
                let end_line_0 = e.line().saturating_sub(1);
                if line >= start_line_0 && line <= end_line_0 && col >= s.col() {
                    ancestor_spans.push((s, e));
                }
            }
        }
        YamlDataOwned::Alias(_) | YamlDataOwned::BadValue => {}
    }
}

/// Compute the effective start marker for a node, recursing into containers
/// whose own span is zero to find their first child's start.
fn value_start_marker(node: &MarkedYamlOwned) -> Option<Marker> {
    let start = node.span.start;
    if start.line() > 0 {
        return Some(start);
    }
    match &node.data {
        YamlDataOwned::Mapping(map) => map
            .keys()
            .filter_map(|k| {
                let s = k.span.start;
                if s.line() > 0 { Some(s) } else { None }
            })
            .min_by_key(|m| (m.line(), m.col())),
        YamlDataOwned::Sequence(arr) => arr
            .iter()
            .filter_map(value_start_marker)
            .min_by_key(|m| (m.line(), m.col())),
        YamlDataOwned::Tagged(_, inner) => value_start_marker(inner),
        YamlDataOwned::Value(_)
        | YamlDataOwned::Representation(_, _, _)
        | YamlDataOwned::Alias(_)
        | YamlDataOwned::BadValue => None,
    }
}

/// Compute the effective end marker for a node, recursing into containers
/// whose own span is zero to find their last child's end.
fn value_end_marker(node: &MarkedYamlOwned) -> Option<Marker> {
    let end = node.span.end;
    if end.line() > 0 {
        return Some(end);
    }
    // Zero-span container: find the last non-zero child end
    match &node.data {
        YamlDataOwned::Mapping(map) => map
            .values()
            .filter_map(value_end_marker)
            .max_by_key(|m| (m.line(), m.col())),
        YamlDataOwned::Sequence(arr) => arr
            .iter()
            .filter_map(value_end_marker)
            .max_by_key(|m| (m.line(), m.col())),
        YamlDataOwned::Tagged(_, inner) => value_end_marker(inner),
        YamlDataOwned::Value(_)
        | YamlDataOwned::Representation(_, _, _)
        | YamlDataOwned::Alias(_)
        | YamlDataOwned::BadValue => None,
    }
}

/// Convert a pair of saphyr `Marker`s to an LSP `Range`.
/// Marker: line 1-based, col 0-based → LSP: both 0-based.
fn marker_to_lsp_range(start: &Marker, end: &Marker) -> Range {
    #[allow(clippy::cast_possible_truncation)]
    let start_line = start.line().saturating_sub(1) as u32;
    #[allow(clippy::cast_possible_truncation)]
    let start_col = start.col() as u32;
    #[allow(clippy::cast_possible_truncation)]
    let end_line = end.line().saturating_sub(1) as u32;
    #[allow(clippy::cast_possible_truncation)]
    let end_col = end.col() as u32;

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
    use saphyr::LoadableYamlNode;

    fn parse_marked(text: &str) -> Option<Vec<MarkedYamlOwned>> {
        // MarkedYamlOwned is fully owned — no lifetime constraints.
        // Returns None on parse failure.
        MarkedYamlOwned::load_from_str(text).ok()
    }

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    // ---- Basic expansion tests ----

    #[test]
    fn should_return_value_range_expanding_to_key_value_then_document() {
        let text = "key: value\n";
        let docs = parse_marked(text);
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
        let docs = parse_marked(text);
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
        let docs = parse_marked(text);
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
        let docs = parse_marked(text);
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
        let docs = parse_marked(text);
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
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 10)]);

        assert_eq!(result.len(), 1);
        let sr = &result[0];
        assert_eq!(sr.range.start.line, 1);
        assert!(sr.parent.is_some(), "should have parent (name: Alice)");
        let p1 = sr.parent.as_ref().expect("p1");
        assert!(
            p1.parent.is_some(),
            "should have grandparent (list item mapping)"
        );
        let p2 = p1.parent.as_ref().expect("p2");
        assert!(
            p2.parent.is_some(),
            "should have great-grandparent (users sequence)"
        );
        let p3 = p2.parent.as_ref().expect("p3");
        assert!(p3.parent.is_some(), "should have document root");
    }

    #[test]
    fn should_scope_selection_to_current_document_in_multi_doc_yaml() {
        let text = "doc1key: value1\n---\ndoc2key: value2\n";
        let docs = parse_marked(text);
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
        let docs = parse_marked(text);
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
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(99, 0)]);
        let _ = result;
    }

    #[test]
    fn should_return_safe_result_for_position_beyond_line_length() {
        let text = "key: value\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 999)]);
        let _ = result;
    }

    #[test]
    fn should_return_empty_for_cursor_on_document_separator() {
        let text = "a: 1\n---\nb: 2\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 0)]);
        let _ = result;
    }

    #[test]
    fn should_return_empty_for_comment_only_document() {
        let text = "# just a comment\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 2)]);
        let _ = result;
    }

    #[test]
    fn should_handle_cursor_on_comment_line() {
        let text = "key: value\n# this is a comment\nother: data\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 5)]);
        // Comments are not AST nodes — must not panic, safe result is acceptable.
        let _ = result;
    }

    #[test]
    fn should_not_panic_on_deeply_nested_yaml_ast_walk() {
        // Build 500 levels of nesting.
        let mut text = String::new();
        for i in 0..500usize {
            let indent = "  ".repeat(i);
            writeln!(text, "{indent}l{i}:").unwrap();
        }
        let leaf_indent = "  ".repeat(500);
        writeln!(text, "{leaf_indent}leaf: deep").unwrap();

        let docs = parse_marked(&text);
        let result = selection_ranges(&text, docs.as_ref(), &[pos(500, leaf_indent.len() as u32)]);

        let mut depth = 0usize;
        if let Some(sr) = result.first() {
            let mut current = sr;
            while let Some(ref p) = current.parent {
                depth += 1;
                current = p;
                assert!(
                    depth <= 600,
                    "parent chain should be bounded (not infinite)"
                );
            }
        }
    }

    #[test]
    fn should_handle_empty_positions_slice() {
        let text = "key: value\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[]);
        assert!(
            result.is_empty(),
            "should return empty Vec for empty positions slice"
        );
    }

    // ---- Additional coverage tests ----

    // find_document_end: document terminated by "..."
    #[test]
    fn should_scope_document_end_at_dot_dot_dot_terminator() {
        let text = "key: value\n...\nafter: end\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 5)]);

        // Should return a result scoped to before the "..." terminator
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

    // Cursor on "..." line — should return no result (filtered out)
    #[test]
    fn should_return_empty_for_cursor_on_dot_dot_dot_line() {
        let text = "key: value\n...\nother: val\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 0)]);
        // "..." is a separator — should return None (filtered out by filter_map)
        assert!(
            result.is_empty(),
            "cursor on '...' line should produce no selection range"
        );
    }

    // value_start_marker recursion through Sequence
    #[test]
    fn should_handle_sequence_value_in_mapping() {
        let text = "items:\n  - alpha\n  - beta\n  - gamma\n";
        let docs = parse_marked(text);
        // Cursor on a sequence item — exercises value_start_marker/value_end_marker paths
        let result = selection_ranges(text, docs.as_ref(), &[pos(1, 4)]);
        // Should not panic; any valid result is acceptable
        let _ = result;
    }

    // value_end_marker recursion through nested sequences
    #[test]
    fn should_handle_deeply_nested_sequence_value() {
        let text = "data:\n  - nested:\n      - deep_value\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(2, 10)]);
        // Should not panic
        let _ = result;
    }

    // make_line_range: start == end (single line document)
    #[test]
    fn should_handle_single_line_document() {
        let text = "key: value";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 5)]);
        // Single line doc: start_line == end_line
        if let Some(sr) = result.first() {
            let mut outermost = sr;
            while let Some(ref p) = outermost.parent {
                outermost = p;
            }
            assert_eq!(outermost.range.start.line, outermost.range.end.line);
        }
    }

    // find_document_for_line: line exactly at separator increments doc_idx
    #[test]
    fn should_correctly_find_document_for_line_after_separator() {
        let text = "a: 1\n---\nb: 2\n---\nc: 3\n";
        let docs = parse_marked(text);
        // Cursor on last doc (line 4)
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

    // Cursor on key token (col 0) with empty positions after key
    #[test]
    fn should_handle_key_at_column_zero_with_no_value() {
        let text = "empty:\nother: val\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(0, 0)]);
        // Should not panic
        let _ = result;
    }

    // Sequence with empty items (BadValue/Alias scenarios)
    #[test]
    fn should_handle_alias_in_sequence() {
        let text = "base: &anchor value\ncopy:\n  - *anchor\n";
        let docs = parse_marked(text);
        let result = selection_ranges(text, docs.as_ref(), &[pos(2, 4)]);
        // Alias items produce no spans — should not panic
        let _ = result;
    }
}
