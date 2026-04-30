// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Document, LineIndex, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::make_action;

pub(super) fn quoted_bool_to_unquoted(
    docs: &[Document<Span>],
    line_idx: usize,
    col: usize,
    uri: &tower_lsp::lsp_types::Url,
    options: &YamlFormatOptions,
) -> Option<CodeAction> {
    let parser_line = line_idx + 1;
    let (scalar, idx) = find_quoted_bool_scalar(docs, parser_line, col)?;

    let Node::Scalar { value, loc, .. } = scalar else {
        return None;
    };

    let mut plain = scalar.clone();
    // The edit range covers only the scalar token (not the preceding anchor/tag prefix).
    // Clear properties from the clone so format_subtree does not re-emit them in new_text,
    // which would double them — the source buffer already preserves the single occurrence.
    if let Node::Scalar {
        style, tag, meta, ..
    } = &mut plain
    {
        *style = ScalarStyle::Plain;
        *tag = None;
        if let Some(m) = meta.as_mut() {
            m.anchor = None;
            m.anchor_loc = None;
            m.tag_loc = None;
        }
    }

    let base_indent = idx.line_column(loc.start).1 as usize;
    let new_text = format_subtree(&plain, options, base_indent);

    let edit_range = Range::new(
        Position::new(
            idx.line_column(loc.start).0.saturating_sub(1),
            idx.line_column(loc.start).1,
        ),
        Position::new(
            idx.line_column(loc.end).0.saturating_sub(1),
            idx.line_column(loc.end).1,
        ),
    );

    Some(make_action(
        format!("Convert quoted string to {value}"),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::QUICKFIX,
        None,
    ))
}

fn find_quoted_bool_scalar(
    docs: &[Document<Span>],
    parser_line: usize,
    col: usize,
) -> Option<(&Node<Span>, &LineIndex)> {
    for doc in docs {
        let idx = doc.line_index();
        if let Some(node) = find_quoted_bool_in_node(&doc.root, parser_line, col, idx) {
            return Some((node, idx));
        }
    }
    None
}

fn find_quoted_bool_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    col: usize,
    idx: &LineIndex,
) -> Option<&'a Node<Span>> {
    match node {
        Node::Scalar {
            style: ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted,
            value,
            loc,
            ..
        } if idx.line_column(loc.start).0 as usize == parser_line
            && col >= idx.line_column(loc.start).1 as usize
            && col <= idx.line_column(loc.end).1 as usize
            && (value == "true" || value == "false") =>
        {
            Some(node)
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_quoted_bool_in_node(k, parser_line, col, idx) {
                    return Some(found);
                }
                if let Some(found) = find_quoted_bool_in_node(v, parser_line, col, idx) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_quoted_bool_in_node(item, parser_line, col, idx) {
                    return Some(found);
                }
            }
            None
        }
    }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::unwrap_used, reason = "test code")]
mod tests {
    use tower_lsp::lsp_types::CodeActionKind;

    use super::super::code_actions;
    use super::super::test_helpers::{apply_quoted_bool_edit, cursor_range, docs_for};
    use crate::editing::formatter::YamlFormatOptions;
    use crate::test_utils::test_uri;

    fn count(haystack: &str, needle: &str) -> usize {
        let mut count = 0;
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(needle) {
            count += 1;
            start += pos + needle.len();
        }
        count
    }

    // The edit range covers only the scalar token (not the preceding anchor/tag prefix).
    // The fix clears properties from the cloned node before formatting, so new_text
    // contains zero occurrences — the source buffer preserves the single occurrence.
    // The final document therefore contains exactly one occurrence.

    #[test]
    fn quoted_bool_new_text_does_not_duplicate_anchor() {
        let text = "enabled: &myanchor \"true\"\n";
        let (result, edit) = apply_quoted_bool_edit(text, 19);
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
    }

    #[test]
    fn quoted_bool_new_text_does_not_duplicate_user_tag() {
        let text = "enabled: !mytag \"true\"\n";
        let (result, edit) = apply_quoted_bool_edit(text, 16);
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
    }

    #[test]
    fn quoted_bool_new_text_does_not_duplicate_anchor_or_tag() {
        let text = "enabled: &a !mytag \"true\"\n";
        let (result, edit) = apply_quoted_bool_edit(text, 19);
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
    }

    #[test]
    fn quoted_bool_edit_range_is_scalar_span_not_full_line() {
        let text = "enabled: \"true\"  # keep this comment\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(0, 10),
            &[],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(
            edits[0].range.start.character, 9,
            "edit range must start at the opening-quote column: {:?}",
            edits[0].range
        );
        assert_eq!(
            edits[0].range.end.character, 15,
            "edit range end must be the exclusive end of the scalar, not the full line: {:?}",
            edits[0].range
        );
    }

    #[test]
    fn quoted_bool_action_not_offered_for_empty_docs() {
        let actions = code_actions(
            &[],
            "enabled: \"true\"\n",
            cursor_range(0, 10),
            &[],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_action_kind_is_quickfix() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(0, 10),
            &[],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        assert_eq!(
            action.kind,
            Some(CodeActionKind::QUICKFIX),
            "action kind must be QUICKFIX"
        );
    }
}
