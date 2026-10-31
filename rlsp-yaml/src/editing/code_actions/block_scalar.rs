// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Chomp, CollectionStyle, Document, LineIndex, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree, prints_block_header};

use super::{block_to_flow::node_loc, make_action};

pub(super) fn string_to_block_scalar(
    docs: &[Document<Span>],
    text: &str,
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
    let comment_end = trailing_comment_line_end(text, scalar, scalar_loc.end);

    let make_block_action = |style: ScalarStyle, title: &str| -> Option<CodeAction> {
        let mut block_scalar = scalar.clone();
        // Clear anchor, anchor_loc, tag, and tag_loc from the clone before formatting.
        // The edit range covers only the scalar token (not the anchor/tag prefix), so
        // the source buffer already preserves those properties. If the clone retains
        // them, format_subtree re-emits them, doubling the properties in the output.
        // The leading comments sit above the scalar's line, outside the edit.
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
                m.leading_comments = None;
            }
        }
        // The trailing comment follows the scalar on its line.  After the block
        // scalar it would be part of the value, so the edit replaces it too and
        // the header carries it.  When the header cannot carry it, the comment
        // stays where it is: after the replaced text, at the end of the line.
        let range_end = comment_end.filter(|_| prints_block_header(&block_scalar));
        if range_end.is_none()
            && let Node::Scalar {
                meta: Some(meta), ..
            } = &mut block_scalar
        {
            meta.trailing_comment = None;
        }
        let new_text = format_subtree(&block_scalar, options, base_indent);
        if new_text.trim().is_empty() {
            return None;
        }
        Some(make_action(
            title.to_string(),
            uri,
            vec![TextEdit {
                range: Range::new(
                    lsp_position(text, idx, scalar_loc.start),
                    lsp_position(text, idx, range_end.unwrap_or(scalar_loc.end)),
                ),
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

/// The LSP position of a byte offset of `text`: the 0-based line and the column
/// in UTF-16 code units.
fn lsp_position(text: &str, index: &LineIndex, offset: u32) -> Position {
    let line = index.line_column(offset).0.saturating_sub(1);
    let before = text.get(..offset as usize).unwrap_or_default();
    let line_start = before.rfind(['\n', '\r']).map_or(0, |i| i + 1);
    let column: usize = before
        .get(line_start..)
        .unwrap_or_default()
        .chars()
        .map(char::len_utf16)
        .sum();
    Position::new(line, u32::try_from(column).unwrap_or(u32::MAX))
}

/// The byte offset of the end of the line `scalar` ends on, when the rest of
/// that line is the scalar's trailing comment (after spaces or tabs) and
/// nothing else.  `None` when the scalar has no trailing comment or the line
/// holds anything else, so that no neighbouring text is ever replaced.
fn trailing_comment_line_end(text: &str, scalar: &Node<Span>, scalar_end: u32) -> Option<u32> {
    let comment = scalar.trailing_comment()?;
    let end = scalar_end as usize;
    let rest_of_line = text.get(end..)?.split(['\n', '\r']).next()?;
    let blanks = [' ', '\t'];
    let matches = rest_of_line.starts_with(blanks)
        && rest_of_line.trim_matches(blanks) == comment.trim_end_matches(blanks);
    if matches {
        u32::try_from(end + rest_of_line.len()).ok()
    } else {
        None
    }
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
                if is_block
                    && let Node::Scalar {
                        style: scalar_style,
                        value,
                        loc,
                        ..
                    } = v
                    && idx.line_column(loc.start).0 as usize == parser_line
                    && matches!(
                        scalar_style,
                        ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted
                    )
                    && value.chars().count() >= 40
                {
                    let key_col = idx.line_column(node_loc(k).start).1 as usize;
                    return Some((v, key_col, loc));
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
                if is_block
                    && let Node::Scalar {
                        style: scalar_style,
                        value,
                        loc,
                        ..
                    } = item
                    && idx.line_column(loc.start).0 as usize == parser_line
                    && matches!(
                        scalar_style,
                        ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted
                    )
                    && value.chars().count() >= 40
                {
                    let scalar_col = idx.line_column(loc.start).1 as usize;
                    let base_indent = scalar_col.saturating_sub(2);
                    return Some((item, base_indent, loc));
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
    use rlsp_yaml_parser::Span;
    use rlsp_yaml_parser::node::Node;
    use rstest::rstest;
    use tower_lsp::lsp_types::{Position, TextEdit};

    use crate::editing::formatter::YamlFormatOptions;
    use crate::test_utils::{parse_docs, test_uri};

    use super::super::test_helpers::{apply_block_scalar_edit, apply_text_edit};
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
    // the end of the full line, because the edit replaces the trailing comment together with the
    // scalar and the header carries the comment. The fixture format has no range-structure field.
    #[test]
    fn should_preserve_trailing_comment_when_converting_to_block_scalar() {
        let text = "description: \"this is a long string that exceeds forty chars\"  # keep me\n";
        let (result, edit) = apply_block_scalar_edit(text, 0);
        let line_end_col = text.trim_end().len();
        assert_eq!(
            edit.range.end.character as usize, line_end_col,
            "edit end column must equal the end of the line: range={:?}",
            edit.range
        );
        assert_eq!(
            count(&edit.new_text, "# keep me"),
            1,
            "new_text must carry the trailing comment on the header: {:?}",
            edit.new_text
        );
        assert!(
            edit.new_text.starts_with("|  # keep me\n"),
            "the comment must be on the header line: {:?}",
            edit.new_text
        );
        assert_eq!(
            count(&result, "# keep me"),
            1,
            "trailing comment must survive exactly once in the final edited text: {result:?}"
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
        let line_end_col = text.trim_end().len();
        assert_eq!(
            edit.range.end.character as usize, line_end_col,
            "folded edit end column must equal the end of the line, which holds the trailing comment: range={:?}",
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

    #[rstest]
    #[case::next_key_follows(
        "description: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n  bbbb\nother: 1\n"
    )]
    #[case::blank_line_follows(
        "description: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n  bbbb\n\nother: 1\n"
    )]
    #[case::end_of_input_without_newline(
        "description: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n  bbbb"
    )]
    fn multiline_plain_edit_range_ends_at_last_content_byte(#[case] text: &str) {
        let actions = actions_for(text);
        let edit = &actions[0].edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0];
        assert_eq!(edit.range.end, Position::new(1, 6));
    }

    // ---- Comments around the converted scalar ----

    const LONG_TEXT: &str = "this is a long string that exceeds forty chars";

    fn scalar_values(yaml: &str) -> Vec<String> {
        fn collect(node: &Node<Span>, out: &mut Vec<String>) {
            match node {
                Node::Scalar { value, .. } => out.push(value.clone()),
                Node::Alias { .. } => {}
                Node::Mapping { entries, .. } => {
                    for (key, value) in entries {
                        collect(key, out);
                        collect(value, out);
                    }
                }
                Node::Sequence { items, .. } => {
                    for item in items {
                        collect(item, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for doc in rlsp_yaml_parser::load(yaml).expect("test input must parse") {
            collect(&doc.root, &mut out);
        }
        out
    }

    /// The text after applying the action named `kind` ("literal" or "folded")
    /// for the scalar on `line`, and the action's edit.
    fn apply_action(yaml: &str, line: usize, kind: &str) -> (String, TextEdit) {
        let docs = parse_docs(yaml);
        let actions = string_to_block_scalar(
            &docs,
            yaml,
            line,
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains(kind))
            .expect("expected the block scalar action");
        let edit = action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0].clone();
        (apply_text_edit(yaml, &edit), edit)
    }

    // The comment after the scalar moves to the header line of the block scalar:
    // left behind, it would be part of the value.  A comment above the scalar's
    // line is not part of the edit.
    #[rstest]
    #[case::double_quoted(format!("a: \"{LONG_TEXT}\" # c\n"), 0, LONG_TEXT)]
    #[case::plain(format!("a: {LONG_TEXT} # c\n"), 0, LONG_TEXT)]
    #[case::sequence_item(format!("- \"{LONG_TEXT}\" # c\n"), 0, LONG_TEXT)]
    #[case::with_anchor(format!("a: &x \"{LONG_TEXT}\" # c\n"), 0, LONG_TEXT)]
    #[case::nested_key(format!("x:\n  a: \"{LONG_TEXT}\" # c\n"), 1, LONG_TEXT)]
    #[case::tab_before_comment(format!("a: \"{LONG_TEXT}\"\t# c\n"), 0, LONG_TEXT)]
    #[case::trailing_spaces_after_comment(format!("a: \"{LONG_TEXT}\" # c   \n"), 0, LONG_TEXT)]
    #[case::leading_comment_above(format!("# lead\na: \"{LONG_TEXT}\" # c\n"), 1, LONG_TEXT)]
    #[case::crlf_buffer(format!("a: \"{LONG_TEXT}\" # c\r\nb: 1\r\n"), 0, LONG_TEXT)]
    #[case::multiline_plain(
        "a: this is a long string that\n  exceeds forty chars # c\nb: 1\n".to_string(),
        0,
        LONG_TEXT
    )]
    #[case::emoji_in_key(format!("\u{1F600}: \"{LONG_TEXT}\" # c\n"), 0, LONG_TEXT)]
    #[case::emoji_in_scalar(
        "a: \"this is a long \u{1F600} string that exceeds forty chars\" # c\n".to_string(),
        0,
        "this is a long \u{1F600} string that exceeds forty chars"
    )]
    #[case::emoji_before_the_scalar_on_the_line(
        format!("a: \"\u{1F600}{LONG_TEXT}\" # c\n"),
        0,
        &format!("\u{1F600}{LONG_TEXT}")
    )]
    #[case::emoji_everywhere(
        "\u{1F600}: \"this is a long \u{1F600} string that exceeds forty chars\" # c\n".to_string(),
        0,
        "this is a long \u{1F600} string that exceeds forty chars"
    )]
    fn converting_moves_the_trailing_comment_to_the_header(
        #[case] yaml: String,
        #[case] line: usize,
        #[case] text: &str,
        #[values("literal", "folded")] kind: &str,
    ) {
        let (result, edit) = apply_action(&yaml, line, kind);
        assert_eq!(count(&result, "# c"), 1, "one comment: {result:?}");
        let comment_line = result
            .lines()
            .find(|l| l.contains("# c"))
            .expect("the comment is in the result");
        assert!(
            comment_line.contains("|  # c") || comment_line.contains(">  # c"),
            "the comment must be on the header line: {result:?}"
        );
        assert!(
            !edit.new_text.contains("# lead"),
            "new_text must not carry the leading comment: {:?}",
            edit.new_text
        );
        let expected: Vec<String> = scalar_values(&yaml)
            .into_iter()
            .map(|v| if v == text { format!("{v}\n") } else { v })
            .collect();
        assert_eq!(
            scalar_values(&result),
            expected,
            "the value must not change: {result:?}"
        );
    }

    // A value that cannot be a block scalar is printed double-quoted; the
    // comment stays after it, at the end of the line.
    #[test]
    fn converting_a_value_without_a_header_leaves_the_comment_in_place() {
        let yaml = "a: \"this is a long string that\\n \\nexceeds forty chars\" # c\n";
        let (result, edit) = apply_action(yaml, 0, "literal");
        assert_eq!(count(&result, "# c"), 1, "one comment: {result:?}");
        assert!(
            !edit.new_text.contains("# c"),
            "new_text must not carry the comment: {:?}",
            edit.new_text
        );
        assert_eq!(scalar_values(&result), scalar_values(yaml));
    }

    // The edit replaces only the line of the scalar: the lines around it, and the
    // text after a multi-line quoted scalar's last line, stay as they are.
    #[test]
    fn converting_replaces_nothing_beyond_the_scalar_and_its_comment() {
        let yaml = format!("before: 1 # keep\na: \"{LONG_TEXT}\" # c\nafter: 2 # keep\n");
        let (result, _) = apply_action(&yaml, 1, "literal");
        assert!(
            result.starts_with("before: 1 # keep\na: |  # c\n"),
            "{result:?}"
        );
        assert!(result.ends_with("\nafter: 2 # keep\n"), "{result:?}");
    }
}
