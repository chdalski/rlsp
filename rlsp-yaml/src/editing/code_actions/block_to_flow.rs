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
) -> Option<CodeAction> {
    let (node, key_loc, idx) = find_innermost_block_collection(docs, line_idx)?;
    let loc = node_loc(node);

    let loc_start_line = idx.line_column(loc.start).0 as usize;
    let key_start_line = idx.line_column(key_loc.start).0 as usize;
    if loc_start_line <= key_start_line {
        return None;
    }

    if has_nested_collection_child(node) {
        return None;
    }

    let mut flow_node = node.clone();
    match &mut flow_node {
        Node::Mapping { style, .. } | Node::Sequence { style, .. } => {
            *style = CollectionStyle::Flow;
        }
        Node::Scalar { .. } | Node::Alias { .. } => return None,
    }

    let (key_start_line_1based, key_start_col) = idx.line_column(key_loc.start);
    let base_indent = key_start_col as usize + 2;
    let key_line = key_start_line_1based.saturating_sub(1); // 0-based
    let formatted = format_subtree(&flow_node, &YamlFormatOptions::default(), base_indent);
    let new_text = format!(" {formatted}");

    if new_text.trim().is_empty() {
        return None;
    }

    let title = if new_text.len() > 80 {
        "Convert block to flow style (long line)".to_string()
    } else {
        "Convert block to flow style".to_string()
    };

    let (_, key_end_col) = idx.line_column(key_loc.end);
    let edit_start_col = key_end_col as usize + 1;
    let (loc_end_line, loc_end_col) = idx.line_column(loc.end);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "edit_start_col is a usize byte offset that always fits u32"
    )]
    let edit_range = Range::new(
        Position::new(key_line, edit_start_col as u32),
        Position::new(loc_end_line.saturating_sub(1), loc_end_col + 1),
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
        let formatted = format_subtree(node, &YamlFormatOptions::default(), base_indent);
        (format!("\n{indent_str}{formatted}"), start_col)
    } else {
        let formatted = format_subtree(node, &YamlFormatOptions::default(), start_col);
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

/// Return true if any direct child of the given block collection node is
/// itself a block-style `Mapping` or `Sequence`.
pub(super) fn has_nested_collection_child(node: &Node<Span>) -> bool {
    match node {
        Node::Mapping { entries, .. } => entries.iter().any(|(_, v)| is_block_collection(v)),
        Node::Sequence { items, .. } => items.iter().any(is_block_collection),
        Node::Scalar { .. } | Node::Alias { .. } => false,
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
#[expect(clippy::indexing_slicing, clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::super::code_actions;
    use super::super::test_helpers::{
        apply_block_to_flow_edit, cursor_range, docs_for, line_range,
    };
    use crate::parser::parse_yaml;
    use crate::test_utils::test_uri;

    // ---- Block to flow ----

    #[test]
    fn should_convert_block_mapping_to_flow() {
        let text = "config:\n  a: 1\n  b: 2\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("block to flow"));
        assert!(action.is_some());
        let edit = action.unwrap().edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("{ a: 1, b: 2 }"),
            "expected flow mapping with bracket spacing: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_convert_block_sequence_to_flow() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("block to flow"));
        assert!(action.is_some());
        let edit = action.unwrap().edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("[one, two]"));
    }

    #[test]
    fn should_not_offer_block_to_flow_for_inline_value() {
        let text = "key: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block to flow")));
    }

    #[test]
    fn should_not_offer_block_to_flow_for_nested_structures() {
        let text = "config:\n  a:\n    nested: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block to flow")));
    }

    #[test]
    fn should_handle_flow_sequence_item_when_converting_block_to_flow() {
        let text = "args:\n  - [nested]\n  - safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[nested]"),
            "nested flow sequence must appear in output: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("safe"),
            "safe item should be present: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_quote_item_containing_comma_when_converting_block_to_flow() {
        let text = "args:\n  - a, b\n  - c\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"a, b\""),
            "comma-containing item must be quoted: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains('c'),
            "safe item should be present: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_not_quote_safe_items_when_converting_block_to_flow() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[one, two]"),
            "safe items should not be quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_append_long_line_warning_when_result_exceeds_80_chars() {
        let text = "items:\n  - long_item_aaa\n  - long_item_bbb\n  - long_item_ccc\n  - long_item_ddd\n  - long_item_eee\n  - long_item_fff\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        assert!(
            action.title.contains("(long line)"),
            "long result should include warning in title: {:?}",
            action.title
        );
    }

    #[test]
    fn should_not_append_long_line_warning_for_short_result() {
        let text = "items:\n  - a\n  - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        assert_eq!(
            action.title, "Convert block to flow style",
            "short result must not include long-line warning: {:?}",
            action.title
        );
    }

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

    // B-1: cursor on block collection start line → action offered
    #[test]
    fn should_offer_block_to_flow_when_cursor_is_on_block_collection_line() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action on block collection start line"
        );
    }

    // B-3: cursor on child line (not collection start) → no action
    #[test]
    fn should_not_offer_block_to_flow_when_cursor_is_on_child_line_not_collection_start() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when cursor is on a child line, not the collection start"
        );
    }

    // B-4: innermost block on cursor line is targeted
    #[test]
    fn should_offer_block_to_flow_for_nested_block_when_cursor_is_on_inner_collection_line() {
        let text = "outer:\n  inner:\n    - a\n    - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action targeting the inner sequence at cursor line 1"
        );
    }

    // B-5: cursor on comment line → no action
    #[test]
    fn should_not_offer_block_to_flow_when_no_block_collection_on_cursor_line() {
        let text = "# comment\nkey: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow on a comment-only line"
        );
    }

    // C-2: sequence items are mappings → no action
    #[test]
    fn should_not_offer_block_to_flow_when_sequence_item_is_nested_mapping() {
        let text = "items:\n  - key: val\n  - other: val\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when sequence items are mappings"
        );
    }

    // C-3: sequence item is a nested block sequence → no action
    #[test]
    fn should_not_offer_block_to_flow_when_sequence_item_is_nested_sequence() {
        let text = "matrix:\n  -\n    - a\n    - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when sequence items are nested sequences"
        );
    }

    // C-4: all sequence items are scalars → action offered
    #[test]
    fn should_offer_block_to_flow_when_all_sequence_items_are_scalars() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action when all sequence items are scalars"
        );
    }

    // C-5: all mapping values are scalars → action offered
    #[test]
    fn should_offer_block_to_flow_when_all_mapping_values_are_scalars() {
        let text = "point:\n  x: 1\n  y: 2\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action when all mapping values are scalars"
        );
    }

    // D-1: top-level block sequence value → flow inline (no leading newline)
    #[test]
    fn should_emit_flow_inline_when_block_sequence_is_top_level_value() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            !edits[0].new_text.starts_with('\n'),
            "top-level value edit must not start with newline: {:?}",
            edits[0].new_text
        );
    }

    // D-2: block mapping that is itself a mapping value → flow emitted inline after key colon
    #[test]
    fn should_emit_flow_inline_when_block_is_mapping_value() {
        let text = "outer:\n  inner:\n    x: 1\n    y: 2\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.starts_with(' '),
            "mapping-value block edit must start with space (inline placement): {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("x: 1") && edits[0].new_text.contains("y: 2"),
            "mapping keys must appear in output: {:?}",
            edits[0].new_text
        );
    }

    // G-1: empty document → no action
    #[test]
    fn should_not_offer_block_to_flow_for_empty_document() {
        let text = "\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow for empty document"
        );
    }

    // G-2: multi-document YAML — cursor on block collection in second document → action offered
    #[test]
    fn should_offer_block_to_flow_for_second_document_block_collection() {
        let text = "key: value\n---\nother: stuff\nitems:\n  - a\n  - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(3, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "must offer block-to-flow for block collection in second document"
        );
    }

    // G-3: flow collection on cursor line → no action
    #[test]
    fn should_not_offer_block_to_flow_when_cursor_is_on_flow_collection_line() {
        let text = "items: [a, b]\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when collection is already flow style"
        );
    }

    // D-3: top-level block sequence (key at column 0)
    #[test]
    fn should_use_loc_start_column_as_base_indent_for_top_level_block() {
        let text = "items:\n  - a\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert_eq!(
            new_text.trim_start(),
            "[a]",
            "top-level block must produce single-line flow text: {new_text:?}"
        );
    }

    // D-4: nested mapping value block (key at column 2)
    #[test]
    fn should_use_key_indent_plus_2_as_base_indent_for_mapping_value_block() {
        let text = "outer:\n  inner:\n    x: 1\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.starts_with(' ') && new_text.contains("x: 1"),
            "nested mapping value must produce inline flow with correct content: {new_text:?}"
        );
    }

    // ---- block_to_flow: mapping-value quoting and anchor regression tests ----

    #[test]
    fn should_produce_valid_yaml_for_mapping_value_containing_colon() {
        let text = "endpoint:\n  url: http://example.com\n  method: GET\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "colon-in-value mapping must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_result.diagnostics
        );
        assert!(
            result.contains("http://example.com"),
            "URL value must be preserved: {result:?}"
        );
    }

    #[test]
    fn should_quote_mapping_value_containing_comma_when_converting_block_to_flow() {
        let text = "info:\n  tags: foo, bar\n  name: safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"foo, bar\""),
            "comma-containing mapping value must be quoted: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "result must reparse cleanly: {result:?}"
        );
    }

    #[test]
    fn should_quote_mapping_value_containing_brace_when_converting_block_to_flow() {
        let text = "template:\n  expr: ${VAR}\n  name: safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"${VAR}\""),
            "brace-containing mapping value must be quoted: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "result must reparse cleanly: {result:?}"
        );
    }

    #[test]
    fn should_quote_mapping_value_containing_bracket_when_converting_block_to_flow() {
        let text = "filter:\n  pattern: a[0]\n  name: safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"a[0]\""),
            "bracket-containing mapping value must be quoted: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "result must reparse cleanly: {result:?}"
        );
    }

    #[test]
    fn should_preserve_quoted_key_with_colon_when_converting_block_to_flow() {
        let text = "labels:\n  \"foo:bar\": value\n  safe: ok\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "quoted-key-with-colon mapping must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_result.diagnostics
        );
        assert!(
            result.contains("foo:bar"),
            "key value must be preserved in output: {result:?}"
        );
    }

    #[test]
    fn should_preserve_anchor_when_converting_anchored_block_mapping_to_flow() {
        let text = "defaults: &base\n  timeout: 30\n  retries: 3\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("&base"),
            "anchor must appear in flow output: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "anchored mapping result must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_yaml(&result).diagnostics
        );
    }

    #[test]
    fn should_preserve_anchor_when_converting_anchored_block_sequence_to_flow() {
        let text = "items: &mylist\n  - a\n  - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("&mylist"),
            "anchor must appear in flow output: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "anchored sequence result must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_yaml(&result).diagnostics
        );
    }

    #[test]
    fn should_not_offer_block_to_flow_for_line_range() {
        let text = "key: value\n";
        let actions = code_actions(&docs_for(text), text, line_range(0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block to flow")));
    }
}
