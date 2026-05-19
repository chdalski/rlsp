// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Chomp, CollectionStyle, Document, LineIndex, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::{block_to_flow::node_loc, make_action};

pub(super) fn string_to_block_scalar(
    docs: &[Document<Span>],
    _text: &str,
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
    options: &YamlFormatOptions,
) -> Vec<CodeAction> {
    let parser_line = line_idx + 1;
    let Some((scalar, key_col, scalar_loc, idx)) = find_block_scalar_candidate(docs, parser_line)
    else {
        return vec![];
    };

    let base_indent = key_col;

    let edit_range = Range::new(
        Position::new(
            idx.line_column(scalar_loc.start).0.saturating_sub(1),
            idx.line_column(scalar_loc.start).1,
        ),
        Position::new(
            idx.line_column(scalar_loc.end).0.saturating_sub(1),
            idx.line_column(scalar_loc.end).1,
        ),
    );

    let make_block_action = |style: ScalarStyle, title: &str| -> Option<CodeAction> {
        let mut block_scalar = scalar.clone();
        // Clear anchor, anchor_loc, tag, and tag_loc from the clone before formatting.
        // The edit range covers only the scalar token (not the anchor/tag prefix), so
        // the source buffer already preserves those properties. If the clone retains
        // them, format_subtree re-emits them, doubling the properties in the output.
        if let Node::Scalar {
            style: s,
            tag,
            meta,
            ..
        } = &mut block_scalar
        {
            *s = style;
            *tag = None;
            if let Some(m) = meta.as_mut() {
                m.anchor = None;
                m.anchor_loc = None;
                m.tag_loc = None;
            }
        }
        let new_text = format_subtree(&block_scalar, options, base_indent);
        if new_text.trim().is_empty() {
            return None;
        }
        Some(make_action(
            title.to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text,
            }],
            CodeActionKind::REFACTOR_REWRITE,
            None,
        ))
    };

    [
        make_block_action(
            ScalarStyle::Literal(Chomp::Clip),
            "Convert to block scalar (literal)",
        ),
        make_block_action(
            ScalarStyle::Folded(Chomp::Clip),
            "Convert to block scalar (folded)",
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Walk the AST to find a qualifying scalar mapping value on the given parser line.
///
/// Returns `(scalar_node, key_start_column, scalar_loc, line_index)` when found.
fn find_block_scalar_candidate(
    docs: &[Document<Span>],
    parser_line: usize,
) -> Option<(&Node<Span>, usize, &Span, &LineIndex)> {
    for doc in docs {
        let idx = doc.line_index();
        if let Some((node, col, loc)) = find_block_scalar_in_node(&doc.root, parser_line, idx) {
            return Some((node, col, loc, idx));
        }
    }
    None
}

fn find_block_scalar_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    idx: &LineIndex,
) -> Option<(&'a Node<Span>, usize, &'a Span)> {
    match node {
        Node::Mapping { entries, style, .. } => {
            let is_block = matches!(style, CollectionStyle::Block);
            for (k, v) in entries {
                if is_block {
                    if let Node::Scalar {
                        style: scalar_style,
                        value,
                        loc,
                        ..
                    } = v
                    {
                        if idx.line_column(loc.start).0 as usize == parser_line
                            && matches!(
                                scalar_style,
                                ScalarStyle::Plain
                                    | ScalarStyle::SingleQuoted
                                    | ScalarStyle::DoubleQuoted
                            )
                            && value.chars().count() >= 40
                        {
                            let key_col = idx.line_column(node_loc(k).start).1 as usize;
                            return Some((v, key_col, loc));
                        }
                    }
                }
                if let Some(result) = find_block_scalar_in_node(k, parser_line, idx) {
                    return Some(result);
                }
                if let Some(result) = find_block_scalar_in_node(v, parser_line, idx) {
                    return Some(result);
                }
            }
            None
        }
        Node::Sequence { items, style, .. } => {
            let is_block = matches!(style, CollectionStyle::Block);
            for item in items {
                if is_block {
                    if let Node::Scalar {
                        style: scalar_style,
                        value,
                        loc,
                        ..
                    } = item
                    {
                        if idx.line_column(loc.start).0 as usize == parser_line
                            && matches!(
                                scalar_style,
                                ScalarStyle::Plain
                                    | ScalarStyle::SingleQuoted
                                    | ScalarStyle::DoubleQuoted
                            )
                            && value.chars().count() >= 40
                        {
                            let scalar_col = idx.line_column(loc.start).1 as usize;
                            let base_indent = scalar_col.saturating_sub(2);
                            return Some((item, base_indent, loc));
                        }
                    }
                }
                if let Some(result) = find_block_scalar_in_node(item, parser_line, idx) {
                    return Some(result);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::Position;

    use crate::editing::formatter::YamlFormatOptions;
    use crate::test_utils::{parse_docs, test_uri};

    use super::super::test_helpers::apply_block_scalar_edit;
    use super::string_to_block_scalar;

    fn count(haystack: &str, needle: &str) -> usize {
        let mut count = 0;
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(needle) {
            count += 1;
            start += pos + needle.len();
        }
        count
    }

    fn actions_for(yaml: &str) -> Vec<super::super::CodeAction> {
        let docs = parse_docs(yaml);
        string_to_block_scalar(&docs, yaml, 0, &test_uri(), &YamlFormatOptions::default())
    }

    fn apply_folded_edit(yaml: &str) -> (String, tower_lsp::lsp_types::TextEdit) {
        use crate::editing::code_actions::code_actions;
        use tower_lsp::lsp_types::Range;
        let docs = parse_docs(yaml);
        let cursor = Range::new(Position::new(0, 0), Position::new(0, 0));
        let all_actions = code_actions(
            &docs,
            yaml,
            cursor,
            &[],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = all_actions
            .iter()
            .find(|a| a.title.contains("folded"))
            .expect("expected folded action");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let edit = edits[0].clone();
        let source_lines: Vec<&str> = yaml.lines().collect();
        let line_idx = edit.range.start.line as usize;
        let start_col = edit.range.start.character as usize;
        let end_col = edit.range.end.character as usize;
        let src_line = source_lines[line_idx];
        let new_line = format!(
            "{}{}{}",
            &src_line[..start_col],
            edit.new_text,
            &src_line[end_col..]
        );
        let mut result = String::new();
        for (i, l) in source_lines.iter().enumerate() {
            if i == line_idx {
                result.push_str(&new_line);
            } else {
                result.push_str(l);
            }
            result.push('\n');
        }
        (result, edit)
    }

    // Group 1 — Vec return: two actions are produced

    #[test]
    fn returns_two_actions_for_qualifying_scalar() {
        let text = "key: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n";
        let actions = actions_for(text);
        assert_eq!(
            actions.len(),
            2,
            "expected two actions for qualifying scalar: {actions:?}"
        );
    }

    #[test]
    fn first_action_title_is_literal() {
        let text = "key: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n";
        let actions = actions_for(text);
        assert_eq!(
            actions[0].title, "Convert to block scalar (literal)",
            "first action must be literal"
        );
    }

    #[test]
    fn second_action_title_is_folded() {
        let text = "key: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n";
        let actions = actions_for(text);
        assert_eq!(
            actions[1].title, "Convert to block scalar (folded)",
            "second action must be folded"
        );
    }

    #[test]
    fn literal_action_new_text_starts_with_pipe() {
        let text = "key: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n";
        let actions = actions_for(text);
        let new_text = actions[0].edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0]
            .new_text
            .clone();
        assert!(
            new_text.starts_with('|'),
            "literal action new_text must start with '|': {new_text:?}"
        );
    }

    #[test]
    fn folded_action_new_text_starts_with_gt() {
        let text = "key: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n";
        let actions = actions_for(text);
        let new_text = actions[1].edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0]
            .new_text
            .clone();
        assert!(
            new_text.starts_with('>'),
            "folded action new_text must start with '>': {new_text:?}"
        );
    }

    // Group 2 — Existing inline tests updated to also assert folded sibling exists

    // Pattern C (kept inline): column-exact range assertion — edit.range.end.character must equal
    // the exclusive byte end of the scalar span, not the end of the full line. The fixture format
    // has no range-structure field.
    #[test]
    fn should_preserve_trailing_comment_when_converting_to_block_scalar() {
        let text = "description: \"this is a long string that exceeds forty chars\"  # keep me\n";
        let (result, edit) = apply_block_scalar_edit(text, 0);
        let scalar_end_col =
            "description: \"this is a long string that exceeds forty chars\"".len();
        assert_eq!(
            edit.range.end.character as usize, scalar_end_col,
            "edit end column must equal the exclusive end of the scalar span: range={:?}",
            edit.range
        );
        assert!(
            !edit.new_text.contains("# keep me"),
            "new_text must not contain the trailing comment: {:?}",
            edit.new_text
        );
        assert!(
            result.contains("# keep me"),
            "trailing comment must survive in the final edited text: {result:?}"
        );
        // folded sibling must also exist
        let actions = actions_for(text);
        assert!(
            actions.iter().any(|a| a.title.contains("folded")),
            "folded action must also be present: {actions:?}"
        );
    }

    // The edit range covers only the scalar token (not the preceding anchor/tag prefix).
    // The fix clears properties from the cloned node before formatting, so new_text
    // contains zero occurrences — the source buffer preserves the single occurrence.
    // The final document (source + new_text splice) therefore contains exactly one occurrence.

    #[test]
    fn new_text_does_not_duplicate_anchor() {
        let text = "description: &myanchor \"this is a long string that exceeds forty chars\"\n";
        let (result, edit) = apply_block_scalar_edit(text, 0);
        assert_eq!(
            count(&edit.new_text, "&myanchor"),
            0,
            "new_text must not contain the anchor (source buffer preserves it): {:?}",
            edit.new_text
        );
        assert_eq!(
            count(&result, "&myanchor"),
            1,
            "final document must contain the anchor exactly once: {result:?}"
        );
        let actions = actions_for(text);
        assert!(
            actions.iter().any(|a| a.title.contains("folded")),
            "folded action must also be present: {actions:?}"
        );
    }

    #[test]
    fn new_text_does_not_duplicate_user_tag() {
        let text = "description: !mytag \"this is a long string that exceeds forty chars\"\n";
        let (result, edit) = apply_block_scalar_edit(text, 0);
        assert_eq!(
            count(&edit.new_text, "!mytag"),
            0,
            "new_text must not contain the user tag (source buffer preserves it): {:?}",
            edit.new_text
        );
        assert_eq!(
            count(&result, "!mytag"),
            1,
            "final document must contain the user tag exactly once: {result:?}"
        );
        let actions = actions_for(text);
        assert!(
            actions.iter().any(|a| a.title.contains("folded")),
            "folded action must also be present: {actions:?}"
        );
    }

    #[test]
    fn new_text_does_not_duplicate_anchor_or_tag_when_both_present() {
        let text = "description: &a !mytag \"this is a long string that exceeds forty chars\"\n";
        let (result, edit) = apply_block_scalar_edit(text, 0);
        assert_eq!(
            count(&edit.new_text, "&a"),
            0,
            "new_text must not contain the anchor (source buffer preserves it): {:?}",
            edit.new_text
        );
        assert_eq!(
            count(&edit.new_text, "!mytag"),
            0,
            "new_text must not contain the user tag (source buffer preserves it): {:?}",
            edit.new_text
        );
        assert_eq!(
            count(&result, "&a"),
            1,
            "final document must contain the anchor exactly once: {result:?}"
        );
        assert_eq!(
            count(&result, "!mytag"),
            1,
            "final document must contain the user tag exactly once: {result:?}"
        );
        let actions = actions_for(text);
        assert!(
            actions.iter().any(|a| a.title.contains("folded")),
            "folded action must also be present: {actions:?}"
        );
    }

    // Group 3 — Folded action edit correctness

    #[test]
    fn folded_action_does_not_duplicate_anchor() {
        let text = "description: &myanchor \"this is a long string that exceeds forty chars\"\n";
        let (result, edit) = apply_folded_edit(text);
        assert_eq!(
            count(&edit.new_text, "&myanchor"),
            0,
            "folded new_text must not contain the anchor (source buffer preserves it): {:?}",
            edit.new_text
        );
        assert_eq!(
            count(&result, "&myanchor"),
            1,
            "final document must contain the anchor exactly once: {result:?}"
        );
    }

    #[test]
    fn folded_action_does_not_duplicate_user_tag() {
        let text = "description: !mytag \"this is a long string that exceeds forty chars\"\n";
        let (result, edit) = apply_folded_edit(text);
        assert_eq!(
            count(&edit.new_text, "!mytag"),
            0,
            "folded new_text must not contain the user tag (source buffer preserves it): {:?}",
            edit.new_text
        );
        assert_eq!(
            count(&result, "!mytag"),
            1,
            "final document must contain the user tag exactly once: {result:?}"
        );
    }

    #[test]
    fn folded_action_edit_range_end_column_is_exact() {
        let text = "description: \"this is a long string that exceeds forty chars\"  # keep me\n";
        let (_, edit) = apply_folded_edit(text);
        let scalar_end_col =
            "description: \"this is a long string that exceeds forty chars\"".len();
        assert_eq!(
            edit.range.end.character as usize, scalar_end_col,
            "folded edit end column must equal the exclusive end of the scalar span: range={:?}",
            edit.range
        );
    }

    // Group 4 — Empty Vec when no candidate (regression guard)

    #[test]
    fn returns_empty_vec_for_short_scalar() {
        let text = "key: \"short\"\n";
        let actions = actions_for(text);
        assert!(
            actions.is_empty(),
            "expected empty Vec for short scalar, got: {actions:?}"
        );
    }

    #[test]
    fn returns_empty_vec_for_already_literal_scalar() {
        let text = "key: |\n  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n";
        let actions = actions_for(text);
        assert!(
            actions.is_empty(),
            "expected empty Vec for already-literal scalar, got: {actions:?}"
        );
    }
}
