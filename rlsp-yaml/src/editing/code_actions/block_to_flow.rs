// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{CollectionStyle, Document, LineIndex, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::make_action;

pub(super) fn block_to_flow(
    docs: &[Document<Span>],
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
    options: &YamlFormatOptions,
) -> Option<CodeAction> {
    if options.format_enforce_block_style {
        return None;
    }
    let (node, key_loc, idx) = find_innermost_block_collection(docs, line_idx)?;
    let loc = node_loc(node);

    let loc_start_line = idx.line_column(loc.start).0 as usize;
    let key_start_line = idx.line_column(key_loc.start).0 as usize;
    if loc_start_line <= key_start_line {
        return None;
    }

    let mut flow_node = node.clone();
    match &mut flow_node {
        Node::Mapping { .. } | Node::Sequence { .. } => flip_to_flow(&mut flow_node),
        Node::Scalar { .. } | Node::Alias { .. } => return None,
    }

    let (key_start_line_1based, key_start_col) = idx.line_column(key_loc.start);
    let base_indent = key_start_col as usize + 2;
    let key_line = key_start_line_1based.saturating_sub(1); // 0-based
    let formatted = format_subtree(&flow_node, options, base_indent);
    let new_text = format!(" {formatted}\n");

    if new_text.trim().is_empty() {
        return None;
    }

    let title = "Convert block to flow style".to_string();

    let (_, key_end_col) = idx.line_column(key_loc.end);
    // The `+ 1` is load-bearing for property preservation: starting the edit immediately
    // after the key's colon (rather than at the collection node's `loc.start`) means the
    // edit range covers the source `&anchor`/`!tag` prefix that precedes the block value.
    // When `format_subtree` re-emits those properties in `new_text`, the source occurrence
    // is inside the replaced range and is erased — net count stays at 1. Simplifying this
    // to `loc.start` would exclude the property prefix from the edit, leaving the source
    // occurrence in place while `format_subtree` re-emits a second copy in `new_text`.
    let edit_start_col = key_end_col as usize + 1;
    let (loc_end_line, loc_end_col) = idx.line_column(loc.end);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "edit_start_col is a usize byte offset that always fits u32"
    )]
    let edit_range = Range::new(
        Position::new(key_line, edit_start_col as u32),
        Position::new(loc_end_line.saturating_sub(1), loc_end_col),
    );

    Some(make_action(
        title,
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        None,
    ))
}

/// Format a block node and compute the replacement text and edit start column.
///
/// When the node is a mapping value (`key: {…}` or `key: […]`), inserting block
/// items inline after `key: ` produces invalid YAML. In that case this function
/// produces `"\n<indent><first-item>\n<indent><second-item>..."` so the edit
/// replaces from the `{`/`[` with a newline-plus-properly-indented block form.
pub(super) fn block_text_and_start_col(
    node: &Node<Span>,
    loc: Span,
    text: &str,
    idx: &LineIndex,
    options: &YamlFormatOptions,
) -> (String, usize) {
    let start_col = idx.line_column(loc.start).1 as usize;
    let line_idx = idx.line_column(loc.start).0.saturating_sub(1) as usize;
    let lines: Vec<&str> = text.lines().collect();

    let is_mapping_value = lines.get(line_idx).is_some_and(|line| {
        let prefix = if start_col <= line.len() {
            &line[..start_col]
        } else {
            line
        };
        prefix.trim_end().ends_with(':')
    });

    if is_mapping_value {
        let key_indent = lines
            .get(line_idx)
            .map_or(0, |line| line.len() - line.trim_start().len());
        let base_indent = key_indent + 2;
        let indent_str = " ".repeat(base_indent);
        let formatted = format_subtree(node, options, base_indent);
        (format!("\n{indent_str}{formatted}"), start_col)
    } else {
        let formatted = format_subtree(node, options, start_col);
        (formatted, start_col)
    }
}

/// Walk the AST to find the block `Mapping` or `Sequence` to convert when the
/// cursor is on `line_idx` (LSP 0-based).
fn find_innermost_block_collection<'a>(
    docs: &'a [Document<Span>],
    line_idx: usize,
) -> Option<(&'a Node<Span>, &'a Span, &'a LineIndex)> {
    let parser_line = line_idx + 1;
    let mut best: Option<(&'a Node<Span>, &'a Span, &'a LineIndex)> = None;
    for doc in docs {
        let idx = doc.line_index();
        find_innermost_block_in_node(&doc.root, parser_line, &mut best, idx);
    }
    best
}

fn find_innermost_block_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    best: &mut Option<(&'a Node<Span>, &'a Span, &'a LineIndex)>,
    idx: &'a LineIndex,
) {
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if idx.line_column(node_loc(k).start).0 as usize == parser_line
                    && is_block_collection(v)
                {
                    *best = Some((v, node_loc(k), idx));
                }
                find_innermost_block_in_node(k, parser_line, best, idx);
                find_innermost_block_in_node(v, parser_line, best, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                find_innermost_block_in_node(item, parser_line, best, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

const fn is_block_collection(node: &Node<Span>) -> bool {
    matches!(
        node,
        Node::Mapping {
            style: CollectionStyle::Block,
            ..
        } | Node::Sequence {
            style: CollectionStyle::Block,
            ..
        }
    )
}

/// Recursively set `CollectionStyle::Flow` on every `Mapping` and `Sequence` in the subtree.
fn flip_to_flow(node: &mut Node<Span>) {
    match node {
        Node::Mapping { style, entries, .. } => {
            *style = CollectionStyle::Flow;
            for (k, v) in entries {
                flip_to_flow(k);
                flip_to_flow(v);
            }
        }
        Node::Sequence { style, items, .. } => {
            *style = CollectionStyle::Flow;
            for item in items {
                flip_to_flow(item);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

pub(super) const fn node_loc(node: &Node<Span>) -> &Span {
    match node {
        Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Scalar { loc, .. }
        | Node::Alias { loc, .. } => loc,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::apply_block_to_flow_edit;
    use crate::parser::parse_yaml;

    // Pattern C: re-parseability assertion on the applied edit — fixture format verifies
    // text equality only; the round-trip parse check requires an explicit `parse_yaml` call.
    #[test]
    fn should_produce_reparseable_yaml_when_long_sequence_wraps() {
        let text = "items:\n  - long_item_aaa\n  - long_item_bbb\n  - long_item_ccc\n  - long_item_ddd\n  - long_item_eee\n  - long_item_fff\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "edited YAML must reparse without diagnostics; got: {:?}\nresult text:\n{result}",
            parse_result.diagnostics
        );
        assert_eq!(
            parse_result.documents.len(),
            1,
            "edited YAML must produce exactly one document; result text:\n{result}"
        );
    }

    // Pattern C: re-parseability assertion on the applied edit.
    #[test]
    fn should_produce_reparseable_yaml_when_long_nested_mapping_wraps() {
        let text = "outer:\n  inner:\n    key_aaa: val_aaa\n    key_bbb: val_bbb\n    key_ccc: val_ccc\n    key_ddd: val_ddd\n    key_eee: val_eee\n";
        let result = apply_block_to_flow_edit(text, 1);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "edited YAML must reparse without diagnostics; got: {:?}\nresult text:\n{result}",
            parse_result.diagnostics
        );
        assert_eq!(
            parse_result.documents.len(),
            1,
            "edited YAML must produce exactly one document; result text:\n{result}"
        );
    }

    // Pattern C: re-parseability assertion — nested mapping converts recursively.
    #[test]
    fn should_produce_reparseable_yaml_when_nested_mapping_converts() {
        let text = "outer:\n  inner:\n    a: 1\n    b: 2\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "edited YAML must reparse without diagnostics; got: {:?}\nresult text:\n{result}",
            parse_result.diagnostics
        );
        assert_eq!(
            parse_result.documents.len(),
            1,
            "edited YAML must produce exactly one document; result text:\n{result}"
        );
    }

    // Pattern C: re-parseability assertion — nested sequence converts recursively.
    #[test]
    fn should_produce_reparseable_yaml_when_nested_sequence_converts() {
        let text = "items:\n  - - a\n    - b\n  - - c\n    - d\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "edited YAML must reparse without diagnostics; got: {:?}\nresult text:\n{result}",
            parse_result.diagnostics
        );
        assert_eq!(
            parse_result.documents.len(),
            1,
            "edited YAML must produce exactly one document; result text:\n{result}"
        );
    }
}
