// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Document, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::{block_to_flow::node_loc, make_action};

pub(super) fn delete_unused_anchor(
    docs: &[Document<Span>],
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let diag_line = diag.range.start.line as usize;
    let anchor_start_col = diag.range.start.character as usize;
    let anchor_end_col = diag.range.end.character as usize;

    let line = text.lines().nth(diag_line)?;

    if anchor_start_col >= line.len() || anchor_end_col > line.len() {
        return None;
    }
    // Diagnostic range starts at `&` — the name follows.
    let anchor_name = &line[anchor_start_col + 1..anchor_end_col];

    let node = find_anchored_node(docs, diag_line, anchor_name)?;
    let loc = node_loc(node);

    if matches!(node, Node::Alias { .. }) {
        return None;
    }
    let mut deanchored = node.clone();
    deanchored.clear_anchor();

    let base_indent = loc.start.column;
    let new_text = format_subtree(&deanchored, &YamlFormatOptions::default(), base_indent);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(diag_line as u32, anchor_start_col as u32),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
    );

    Some(make_action(
        "Delete unused anchor".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::QUICKFIX,
        Some(vec![diag.clone()]),
    ))
}

fn find_anchored_node<'a>(
    docs: &'a [Document<Span>],
    diag_line: usize,
    anchor_name: &str,
) -> Option<&'a Node<Span>> {
    let parser_line = diag_line + 1;
    for doc in docs {
        if let Some(node) = find_anchored_node_in(&doc.root, parser_line, anchor_name) {
            return Some(node);
        }
    }
    None
}

fn find_anchored_node_in<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    anchor_name: &str,
) -> Option<&'a Node<Span>> {
    if node.anchor() == Some(anchor_name) {
        let loc_line = node_loc(node).start.line;
        if loc_line == parser_line || loc_line == parser_line + 1 {
            return Some(node);
        }
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_anchored_node_in(k, parser_line, anchor_name) {
                    return Some(found);
                }
                if let Some(found) = find_anchored_node_in(v, parser_line, anchor_name) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_anchored_node_in(item, parser_line, anchor_name) {
                    return Some(found);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::super::code_actions;
    use super::super::test_helpers::{docs_for, line_range, make_diagnostic};
    use crate::test_utils::test_uri;

    // UA-1: plain scalar — anchor removed, surrounding structure preserved
    #[test]
    fn delete_anchor_plain_scalar_value() {
        let text = "defaults: &unused value\n";
        let diag = make_diagnostic(0, 10, 17, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "value");
        assert!(!edits[0].new_text.contains("&unused"));
        assert_eq!(edits[0].range.start.character, 10);
    }

    // UA-2: anchor is the sole value (empty scalar after removal)
    #[test]
    fn delete_anchor_sole_value_empty_scalar() {
        let text = "data: &unused\n";
        let diag = make_diagnostic(0, 6, 13, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&unused"));
    }

    // UA-3: quoted scalar — anchor removed, quotes preserved
    #[test]
    fn delete_anchor_quoted_scalar() {
        let text = "key: &a \"hello\"\n";
        let diag = make_diagnostic(0, 5, 7, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&a"));
        assert!(edits[0].new_text.contains("hello"));
    }

    // UA-4: anchor with user-defined tag — tag preserved after anchor removal
    #[test]
    fn delete_anchor_user_tag_preserved() {
        let text = "key: &a !custom \"hello\"\n";
        let diag = make_diagnostic(0, 5, 7, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&a"));
        assert!(
            edits[0].new_text.contains("!custom"),
            "user tag must be preserved: {:?}",
            edits[0].new_text
        );
        assert!(edits[0].new_text.contains("hello"));
    }

    // UA-5: flow sequence — anchor removed, collection style preserved
    #[test]
    fn delete_anchor_flow_sequence() {
        let text = "list: &nums [1, 2, 3]\n";
        let diag = make_diagnostic(0, 6, 11, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&nums"));
        assert!(
            edits[0].new_text.contains('['),
            "flow sequence bracket must be preserved: {:?}",
            edits[0].new_text
        );
        assert!(edits[0].new_text.contains('1'));
    }

    // UA-6: block mapping value with anchor (multi-line)
    #[test]
    fn delete_anchor_block_mapping_value() {
        let text = "base: &defaults\n  x: 1\n  y: 2\n";
        let diag = make_diagnostic(0, 6, 15, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            !edits[0].new_text.contains("&defaults"),
            "anchor must be removed: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("x: 1"),
            "x entry must be preserved: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("y: 2"),
            "y entry must be preserved: {:?}",
            edits[0].new_text
        );
    }

    // Trailing comment preservation
    #[test]
    fn delete_anchor_trailing_comment_preserved() {
        let text = "key: &a value  # keep me\n";
        let diag = make_diagnostic(0, 5, 7, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].range.end.character as usize <= "key: &a value".len(),
            "edit end must not reach into trailing comment: {:?}",
            edits[0].range
        );
        assert!(
            !edits[0].new_text.contains('#'),
            "new_text must not contain the trailing comment: {:?}",
            edits[0].new_text
        );
        let mut result = text.to_string();
        let start = "key: ".len();
        let end = "key: &a value".len();
        result.replace_range(start..end, &edits[0].new_text);
        assert!(
            result.contains("# keep me"),
            "trailing comment must survive: {result:?}"
        );
    }

    // UA-8: stale diagnostic — anchor already absent from text
    #[test]
    fn delete_anchor_stale_diagnostic_returns_no_action() {
        let text = "data: value\n";
        let diag = make_diagnostic(0, 6, 13, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("unused anchor")),
            "stale diagnostic must not produce an action"
        );
    }
}
