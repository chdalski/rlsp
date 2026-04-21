// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{CollectionStyle, Document, Span};

use super::{block_to_flow::block_text_and_start_col, make_action, span_matches_diag};

pub(super) fn flow_map_to_block(
    docs: &[Document<Span>],
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let node = find_flow_mapping(docs, diag)?;
    let Node::Mapping { loc, .. } = node else {
        return None;
    };

    let mut block_node = node.clone();
    if let Node::Mapping { style, .. } = &mut block_node {
        *style = CollectionStyle::Block;
    }

    let (new_text, edit_start_col) = block_text_and_start_col(&block_node, loc, text);
    if new_text.trim().is_empty() {
        return None;
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            edit_start_col as u32,
        ),
        Position::new(
            loc.end.line.saturating_sub(1) as u32,
            (loc.end.column + 1) as u32,
        ),
    );

    Some(make_action(
        "Convert flow mapping to block style".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        Some(vec![diag.clone()]),
    ))
}

/// Walk the AST to find a flow mapping node whose span matches the diagnostic range.
fn find_flow_mapping<'a>(docs: &'a [Document<Span>], diag: &Diagnostic) -> Option<&'a Node<Span>> {
    for doc in docs {
        if let Some(node) = find_flow_mapping_in_node(&doc.root, diag) {
            return Some(node);
        }
    }
    None
}

fn find_flow_mapping_in_node<'a>(
    node: &'a Node<Span>,
    diag: &Diagnostic,
) -> Option<&'a Node<Span>> {
    match node {
        Node::Mapping {
            style: CollectionStyle::Flow,
            loc,
            entries,
            ..
        } => {
            if span_matches_diag(loc, diag) {
                return Some(node);
            }
            // Search nested nodes
            for (k, v) in entries {
                if let Some(found) = find_flow_mapping_in_node(k, diag) {
                    return Some(found);
                }
                if let Some(found) = find_flow_mapping_in_node(v, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_flow_mapping_in_node(k, diag) {
                    return Some(found);
                }
                if let Some(found) = find_flow_mapping_in_node(v, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_flow_mapping_in_node(item, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

pub(super) fn flow_seq_to_block(
    docs: &[Document<Span>],
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let node = find_flow_sequence(docs, diag)?;
    let Node::Sequence { loc, .. } = node else {
        return None;
    };

    let mut block_node = node.clone();
    if let Node::Sequence { style, .. } = &mut block_node {
        *style = CollectionStyle::Block;
    }

    let (new_text, edit_start_col) = block_text_and_start_col(&block_node, loc, text);
    if new_text.trim().is_empty() {
        return None;
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            edit_start_col as u32,
        ),
        Position::new(
            loc.end.line.saturating_sub(1) as u32,
            (loc.end.column + 1) as u32,
        ),
    );

    Some(make_action(
        "Convert flow sequence to block style".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        Some(vec![diag.clone()]),
    ))
}

/// Walk the AST to find a flow sequence node whose span matches the diagnostic range.
fn find_flow_sequence<'a>(docs: &'a [Document<Span>], diag: &Diagnostic) -> Option<&'a Node<Span>> {
    for doc in docs {
        if let Some(node) = find_flow_sequence_in_node(&doc.root, diag) {
            return Some(node);
        }
    }
    None
}

fn find_flow_sequence_in_node<'a>(
    node: &'a Node<Span>,
    diag: &Diagnostic,
) -> Option<&'a Node<Span>> {
    match node {
        Node::Sequence {
            style: CollectionStyle::Flow,
            loc,
            items,
            ..
        } => {
            if span_matches_diag(loc, diag) {
                return Some(node);
            }
            for item in items {
                if let Some(found) = find_flow_sequence_in_node(item, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_flow_sequence_in_node(item, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_flow_sequence_in_node(k, diag) {
                    return Some(found);
                }
                if let Some(found) = find_flow_sequence_in_node(v, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::items_after_statements,
    reason = "test code"
)]
mod tests {
    use rstest::rstest;

    use tower_lsp::lsp_types::{CodeActionKind, Position, Range};

    use super::super::code_actions;
    use super::super::test_helpers::{
        cursor_range, docs_for, flow_diags_for, flow_map_action, flow_seq_action, line_range,
        make_diagnostic, make_flow_diag, new_text_for,
    };
    use crate::test_utils::test_uri;

    // ---- Flow map to block ----

    #[test]
    fn should_convert_simple_flow_map_to_block() {
        let text = "config: {a: 1, b: 2}\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow mapping"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("a: 1"));
        assert!(edits[0].new_text.contains("b: 2"));
        assert!(!edits[0].new_text.contains('{'));
    }

    #[rstest]
    #[case::flow_map_invalid_range("config: {a: 1}\n", "flowMap", "flow mapping")]
    #[case::flow_seq_invalid_range("items: [a]\n", "flowSeq", "flow sequence")]
    #[case::unused_anchor_invalid_range("data: &unused\n", "unusedAnchor", "unused anchor")]
    fn invalid_range_produces_no_action(
        #[case] text: &str,
        #[case] code: &str,
        #[case] title_fragment: &str,
    ) {
        let diag = make_diagnostic(0, 100, 200, code);
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains(title_fragment)));
    }

    // ---- Flow seq to block ----

    #[test]
    fn should_convert_simple_flow_seq_to_block() {
        let text = "items: [one, two, three]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("- one"));
        assert!(edits[0].new_text.contains("- two"));
        assert!(edits[0].new_text.contains("- three"));
        assert!(!edits[0].new_text.contains('['));
    }

    #[test]
    fn should_indent_block_items_under_key_when_nested() {
        let text = "      command: [\"python\", \"-m\"]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("python"),
            "python must be present: {new_text:?}"
        );
        assert!(
            new_text.contains("\"-m\"") || new_text.contains("'-m'") || new_text.contains("- -m"),
            "second item must be present: {new_text:?}"
        );
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 (key is preserved by caller): {:?}",
            edits[0].range
        );
    }

    #[test]
    fn should_indent_block_items_at_top_level_key() {
        let text = "items: [one, two]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("- one"),
            "one must be present: {new_text:?}"
        );
        assert!(
            new_text.contains("- two"),
            "two must be present: {new_text:?}"
        );
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 (key is preserved by caller): {:?}",
            edits[0].range
        );
    }

    #[test]
    fn should_indent_block_items_under_key_at_indent_2() {
        let text = "  command: [\"a\", \"b\"]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("- a") || new_text.contains("- \"a\""),
            "a must be present: {new_text:?}"
        );
        assert!(
            new_text.contains("- b") || new_text.contains("- \"b\""),
            "b must be present: {new_text:?}"
        );
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 (key is preserved by caller): {:?}",
            edits[0].range
        );
    }

    // ---- Diagnostic overlap ----

    #[test]
    fn should_not_produce_actions_for_non_overlapping_diagnostics() {
        let text = "config: {a: 1}\nother: value\n";
        let diag = make_diagnostic(0, 8, 14, "flowMap");
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(1, 0),
            &[diag],
            &test_uri(),
        );

        assert!(actions.iter().all(|a| !a.title.contains("flow mapping")));
    }

    // ---- Empty diagnostics ----

    #[test]
    fn should_return_empty_for_plain_yaml_no_diagnostics() {
        let text = "key: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.is_empty());
    }

    #[test]
    fn should_preserve_double_quoted_item_when_converting_block_seq_to_flow() {
        let text = "items:\n  - \"true\"\n  - \"false\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[\"true\", \"false\"]"),
            "pre-quoted items must not be double-quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_preserve_single_quoted_item_when_converting_block_seq_to_flow() {
        let text = "items:\n  - 'hello'\n  - 'world'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("hello") && edits[0].new_text.contains("world"),
            "single-quoted items must appear in flow output: {:?}",
            edits[0].new_text
        );
        assert!(
            !edits[0].new_text.contains("\"hello\"") && !edits[0].new_text.contains("\"world\""),
            "safe items must not be double-quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_quote_unsafe_item_alongside_pre_quoted_item() {
        let text = "args:\n  - \"true\"\n  - value, with comma\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0]
                .new_text
                .contains("[\"true\", \"value, with comma\"]"),
            "pre-quoted item preserved and unsafe item quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_not_quote_plain_item_alongside_pre_quoted_item() {
        let text = "args:\n  - \"true\"\n  - plain\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[\"true\", plain]"),
            "pre-quoted item preserved and plain item unquoted: {:?}",
            edits[0].new_text
        );
    }

    // ---- AST-based flow_map_to_block (FM-* tests) ----

    // FM-1: sequence-item flow mapping produces clean block output, no data loss
    #[test]
    fn flow_map_sequence_item_no_data_loss() {
        let text = "- {target: linux, os: ubuntu}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("target: linux"), "new_text: {new_text:?}");
        assert!(new_text.contains("os: ubuntu"), "new_text: {new_text:?}");
        assert!(!new_text.contains('{'), "new_text: {new_text:?}");
        assert!(!new_text.contains('}'), "new_text: {new_text:?}");
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0];
        assert!(
            edit.range.start.character > 0,
            "range must not start at col 0: {:?}",
            edit.range
        );
    }

    // FM-2: multi-line flow mapping spans all input lines
    #[test]
    fn flow_map_multiline_spans_all_lines() {
        let text = "key:\n  {a: 1,\n  b: 2}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("a: 1"), "new_text: {new_text:?}");
        assert!(new_text.contains("b: 2"), "new_text: {new_text:?}");
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0];
        assert_eq!(
            edit.range.start.line, 1,
            "range must start on line 1: {:?}",
            edit.range
        );
        assert_eq!(
            edit.range.end.line, 2,
            "range must end on line 2: {:?}",
            edit.range
        );
    }

    // FM-3: flow mapping as mapping value — edit covers only the node
    #[test]
    fn flow_map_as_mapping_value_edit_covers_node_only() {
        let text = "key: {a: 1, b: 2}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("a: 1"), "new_text: {new_text:?}");
        assert!(new_text.contains("b: 2"), "new_text: {new_text:?}");
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0];
        assert!(
            edit.range.start.character > 0,
            "edit should start past col 0: {:?}",
            edit.range
        );
    }

    // FM-4: nested flow mapping — no scalar loss
    #[test]
    fn flow_map_nested_no_data_loss() {
        let text = "outer: {inner: {x: 1}}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(
            new_text.contains('1'),
            "scalar value 1 must survive: {new_text:?}"
        );
        assert!(
            new_text.contains('x'),
            "scalar key x must survive: {new_text:?}"
        );
    }

    // FM-5: empty flow mapping — no action offered
    #[test]
    fn flow_map_empty_no_action() {
        let text = "key: {}\n";
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());
        let has_map_action = actions.iter().any(|a| a.title.contains("flow mapping"));
        if has_map_action {
            let action = actions
                .iter()
                .find(|a| a.title.contains("flow mapping"))
                .unwrap();
            let new_text = new_text_for(action);
            assert!(
                !new_text.is_empty(),
                "action for empty map must produce non-empty text"
            );
        }
    }

    // FM-6: diagnostic range matches no AST node — no action offered
    #[test]
    fn flow_map_no_matching_node_no_action() {
        let text = "key: value\n";
        let docs = docs_for(text);
        let fake_diag = make_flow_diag("flowMap", 0, 5, 0, 10);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &[fake_diag], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("flow mapping")),
            "no matching node should yield no action"
        );
    }

    // FM-7: flow mapping with nested flow sequence as value — no data loss
    #[test]
    fn flow_map_with_nested_flow_seq_no_data_loss() {
        let text = "root: {a: [1, 2]}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains('a'), "key a must survive: {new_text:?}");
        assert!(
            new_text.contains('1'),
            "scalar 1 must survive: {new_text:?}"
        );
        assert!(
            new_text.contains('2'),
            "scalar 2 must survive: {new_text:?}"
        );
    }

    // FM-4b: flow mapping nested inside a flow sequence
    #[test]
    fn flow_map_to_block_inside_flow_seq_no_data_loss() {
        let text = "list: [{a: 1, b: 2}]\n";
        let action = flow_map_action(text).expect("flow-map action must be offered for inner map");
        let new_text = new_text_for(&action);
        assert!(new_text.contains('a'), "key a must survive: {new_text:?}");
        assert!(
            new_text.contains('1'),
            "scalar 1 must survive: {new_text:?}"
        );
        assert!(new_text.contains('b'), "key b must survive: {new_text:?}");
        assert!(
            new_text.contains('2'),
            "scalar 2 must survive: {new_text:?}"
        );
        assert!(
            !new_text.contains('{'),
            "block output must not contain '{{': {new_text:?}"
        );
    }

    // ---- AST-based flow_seq_to_block (FS-* tests) ----

    // FS-1: all-scalars flow sequence
    #[test]
    fn flow_seq_all_scalars_to_block() {
        let text = "list: [a, b, c]\n";
        let action = flow_seq_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("- a"), "new_text: {new_text:?}");
        assert!(new_text.contains("- b"), "new_text: {new_text:?}");
        assert!(new_text.contains("- c"), "new_text: {new_text:?}");
        assert!(!new_text.contains('['), "new_text: {new_text:?}");
        assert!(!new_text.contains(']'), "new_text: {new_text:?}");
    }

    // FS-2: flow sequence of flow mappings — no data loss
    #[test]
    fn flow_seq_of_flow_maps_no_data_loss() {
        let text = "items: [{x: 1}, {x: 2}]\n";
        let action = flow_seq_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(
            new_text.contains('1'),
            "scalar 1 must survive: {new_text:?}"
        );
        assert!(
            new_text.contains('2'),
            "scalar 2 must survive: {new_text:?}"
        );
    }

    // FS-3: empty flow sequence — no destructive action
    #[test]
    fn flow_seq_empty_no_destructive_action() {
        let text = "list: []\n";
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());
        let has_seq_action = actions.iter().any(|a| a.title.contains("flow sequence"));
        if has_seq_action {
            let action = actions
                .iter()
                .find(|a| a.title.contains("flow sequence"))
                .unwrap();
            let new_text = new_text_for(action);
            assert!(
                !new_text.is_empty(),
                "action for empty seq must produce non-empty text"
            );
        }
    }

    // ---- SIG-* signature compatibility tests ----

    // SIG-1: code_actions accepts empty docs slice
    #[test]
    fn sig_accepts_empty_docs() {
        let actions = code_actions(&[], "key: value\n", cursor_range(0, 0), &[], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains("flow")));
    }

    // SIG-2: parsed docs + matching flowMap diagnostic returns action
    #[test]
    fn sig_parsed_docs_with_flow_map_returns_action() {
        let text = "- {k: v}\n";
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("flow mapping")),
            "should return flow map action: {actions:?}"
        );
    }

    // SIG-3: non-flow diagnostics still produce actions when docs are provided
    #[test]
    fn sig_tab_action_still_works_with_docs() {
        let text = "\tkey: value\n";
        let docs = docs_for(text);
        let actions = code_actions(&docs, text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("tabs to spaces")),
            "tab action should still be offered: {actions:?}"
        );
    }

    // ---- INT-* integration tests ----

    // INT-1: sequence-item flow map preserves all scalars end-to-end
    #[test]
    fn int_sequence_item_flow_map_preserves_all_scalars() {
        use rlsp_yaml_parser::Span;
        use rlsp_yaml_parser::node::Node;

        let text = "- {target: linux, os: ubuntu}\n";
        let docs = docs_for(text);
        let pre_scalars: Vec<String> = {
            let mut out = Vec::new();
            fn collect(node: &Node<Span>, out: &mut Vec<String>) {
                match node {
                    Node::Scalar { value, .. } => out.push(value.clone()),
                    Node::Mapping { entries, .. } => {
                        for (k, v) in entries {
                            collect(k, out);
                            collect(v, out);
                        }
                    }
                    Node::Sequence { items, .. } => {
                        for item in items {
                            collect(item, out);
                        }
                    }
                    Node::Alias { .. } => {}
                }
            }
            for doc in &docs {
                collect(&doc.root, &mut out);
            }
            out
        };

        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());

        for action in &actions {
            if action.kind.as_ref() != Some(&CodeActionKind::REFACTOR_REWRITE) {
                continue;
            }
            let Some(edits) = action
                .edit
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .and_then(|c| c.get(&test_uri()))
            else {
                continue;
            };
            if edits.is_empty() {
                continue;
            }

            let edit = &edits[0];
            let lines: Vec<&str> = text.lines().collect();
            let start_line = edit.range.start.line as usize;
            let end_line = edit.range.end.line as usize;
            let start_char = edit.range.start.character as usize;
            let end_char = edit.range.end.character as usize;

            let mut result = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i < start_line || i > end_line {
                    result.push_str(line);
                    result.push('\n');
                } else if i == start_line && i == end_line {
                    result.push_str(&line[..start_char]);
                    result.push_str(&edit.new_text);
                    result.push_str(&line[end_char..]);
                    result.push('\n');
                } else if i == start_line {
                    result.push_str(&line[..start_char]);
                    result.push_str(&edit.new_text);
                } else if i == end_line {
                    result.push_str(&line[end_char..]);
                    result.push('\n');
                }
            }

            let post_docs = docs_for(&result);
            let mut post_scalars = Vec::new();
            fn collect2(node: &Node<Span>, out: &mut Vec<String>) {
                match node {
                    Node::Scalar { value, .. } => out.push(value.clone()),
                    Node::Mapping { entries, .. } => {
                        for (k, v) in entries {
                            collect2(k, out);
                            collect2(v, out);
                        }
                    }
                    Node::Sequence { items, .. } => {
                        for item in items {
                            collect2(item, out);
                        }
                    }
                    Node::Alias { .. } => {}
                }
            }
            for doc in &post_docs {
                collect2(&doc.root, &mut post_scalars);
            }

            for scalar in &pre_scalars {
                assert!(
                    post_scalars.contains(scalar),
                    "scalar {scalar:?} was lost after applying action {:?}\npre: {pre_scalars:?}\npost: {post_scalars:?}\nresult: {result:?}",
                    action.title
                );
            }
        }
    }

    // INT-2: github-token expression value preserved
    #[test]
    fn int_github_token_expression_preserved() {
        let text = "env:\n  - {GITHUB_TOKEN: \"${{ secrets.GITHUB_TOKEN }}\"}\n";
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());

        for action in &actions {
            if action.kind.as_ref() != Some(&CodeActionKind::REFACTOR_REWRITE) {
                continue;
            }
            let Some(edits) = action
                .edit
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .and_then(|c| c.get(&test_uri()))
            else {
                continue;
            };
            if edits.is_empty() {
                continue;
            }
            let new_text = &edits[0].new_text;
            assert!(
                new_text.contains("GITHUB_TOKEN"),
                "GITHUB_TOKEN key must be preserved: {new_text:?}"
            );
            assert!(
                new_text.contains("secrets.GITHUB_TOKEN"),
                "token value must be preserved: {new_text:?}"
            );
        }
    }
}
