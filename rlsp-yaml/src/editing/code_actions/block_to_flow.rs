// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionDisabled, CodeActionKind, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{CollectionStyle, Document, Event, LineIndex, Span, parse_events};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};
use crate::lsp_util::{offset_to_lsp, source_lines};

use super::{BLANKS, make_action};

const TITLE: &str = "Convert block to flow style";

/// Why the action is offered disabled when the converted range holds a comment.
const COMMENT_DISABLED_REASON: &str = "Flow style would lose the comments in this collection";

pub(super) fn block_to_flow(
    docs: &[Document<Span>],
    text: &str,
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

    // The edit replaces the converted range: from right after the key's colon to
    // the end of the collection's content. The anchor or tag of the collection
    // lies inside it and `format_subtree` prints it again, so it appears once. The
    // trivia after the content, such as a comment on the last line or the blank
    // lines before the next sibling, is outside it and stays.
    // A `#` inside a scalar is no comment, so only the parser's comment events count.
    let comments: Vec<Span> = parse_events(text)
        .filter_map(|event| match event {
            Ok((Event::Comment { .. }, span)) => Some(span),
            _ => None,
        })
        .collect();
    // A mapping's span starts at its first key, after the key's own properties.
    let first_key = match node {
        Node::Mapping { entries, .. } => entries.first().map(|(key, _)| key),
        Node::Sequence { .. } | Node::Scalar { .. } | Node::Alias { .. } => None,
    };
    let first_token = [
        Some(loc.start),
        anchor_start(node),
        tag_start(node),
        first_key.and_then(anchor_start),
        first_key.and_then(tag_start),
    ]
    .into_iter()
    .flatten()
    .min()?;
    let start = step_back_over_trivia(text, &comments, first_token, key_loc.end);
    let end = step_back_over_trivia(text, &comments, loc.end, start);
    if end <= start {
        return None;
    }

    // The flow collection the action prints holds no comments, so the edit would
    // delete every comment in the converted range.
    let first_comment_at_or_after_start = comments.partition_point(|span| span.start < start);
    if comments
        .get(first_comment_at_or_after_start)
        .is_some_and(|span| span.start < end)
    {
        return Some(CodeAction {
            title: TITLE.to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            disabled: Some(CodeActionDisabled {
                reason: COMMENT_DISABLED_REASON.to_string(),
            }),
            ..CodeAction::default()
        });
    }

    let mut flow_node = node.clone();
    match &mut flow_node {
        Node::Mapping { .. } | Node::Sequence { .. } => flip_to_flow(&mut flow_node),
        Node::Scalar { .. } | Node::Alias { .. } => return None,
    }

    let base_indent = idx.line_column(key_loc.start).1 as usize + 2;
    let formatted = format_subtree(&flow_node, options, base_indent);
    let new_text = format!(" {formatted}");

    if new_text.trim().is_empty() {
        return None;
    }

    Some(make_action(
        TITLE.to_string(),
        uri,
        vec![TextEdit {
            range: Range::new(offset_to_lsp(start, idx), offset_to_lsp(end, idx)),
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        None,
    ))
}

fn anchor_start(node: &Node<Span>) -> Option<u32> {
    node.anchor_loc().map(|span| span.start)
}

fn tag_start(node: &Node<Span>) -> Option<u32> {
    node.tag_loc().map(|span| span.start)
}

/// The offset `from` moved back over blanks, line breaks and comments, and not
/// below `floor`: the end of the last token before `from`, or `floor`.
///
/// A comment span takes in the blanks after the comment, so an offset inside
/// the span or at its end moves to the span's start. Blanks and line breaks are
/// ASCII, so every offset the function returns lies on a character boundary
/// when `from` and `floor` do. This is byte-range arithmetic on parser spans
/// and reads no YAML structure.
fn step_back_over_trivia(text: &str, comments: &[Span], from: u32, floor: u32) -> u32 {
    let mut offset = from.min(u32::try_from(text.len()).unwrap_or(u32::MAX));
    while offset > floor {
        let comment = comments
            .partition_point(|span| span.start < offset)
            .checked_sub(1)
            .and_then(|i| comments.get(i))
            .filter(|span| offset <= span.end);
        if let Some(span) = comment {
            offset = span.start;
            continue;
        }
        let is_trivia = text
            .as_bytes()
            .get(offset as usize - 1)
            .is_some_and(|byte| {
                BLANKS.contains(&char::from(*byte)) || matches!(byte, b'\n' | b'\r')
            });
        if !is_trivia {
            break;
        }
        offset -= 1;
    }
    offset.max(floor)
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
    // The column counts code points, so the prefix is cut at a char boundary.
    let mapping_value_line = source_lines(text).nth(line_idx).filter(|line| {
        let prefix = line
            .char_indices()
            .nth(start_col)
            .map_or(*line, |(byte, _)| &line[..byte]);
        prefix.trim_end().ends_with(':')
    });

    let Some(line) = mapping_value_line else {
        return (format_subtree(node, options, start_col), start_col);
    };
    let key_indent = line.len() - line.trim_start().len();
    let base_indent = key_indent + 2;
    let indent_str = " ".repeat(base_indent);
    let formatted = format_subtree(node, options, base_indent);
    (format!("\n{indent_str}{formatted}"), start_col)
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
    use rstest::rstest;

    use tower_lsp::lsp_types::{CodeAction, CodeActionDisabled, CodeActionKind, Position, Range};

    use super::super::code_actions;
    use super::super::test_helpers::{
        apply_block_to_flow_edit, apply_text_edit, block_to_flow_action, block_to_flow_edit,
        cursor_range, docs_for,
    };
    use crate::editing::formatter::YamlFormatOptions;
    use crate::parser::parse_yaml;
    use crate::test_utils::test_uri;
    use rlsp_yaml_parser::{Event, parse_events};

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

    fn assert_loads_as_one_document(result: &str) {
        let parse_result = parse_yaml(result);
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

    // The edited text of each case loads as one document.
    #[rstest]
    #[case::comment_after("a:\n  b: 1\n  c: 2 # note\nd: 1\n", 0)]
    #[case::empty_last_value("a:\n  b: 1\n  c:\n# n\nd: 1\n", 0)]
    #[case::empty_last_item("a:\n  - x\n  -\n# n\nd: 1\n", 0)]
    #[case::last_value_with_only_a_property("a:\n  b: 1\n  c: &x\n# n\nd: 1\n", 0)]
    #[case::multiline_flow_as_last_value("a:\n  b: 1\n  c: [\n    1\n    ]\n# n\nd: 1\n", 0)]
    #[case::space_before_the_colon("a :\n  b: 1\nd: 1\n", 0)]
    #[case::explicit_key("? a\n:\n  b: 1\n", 0)]
    #[case::anchor_on_its_own_line("a:\n  &x\n  b: 1\nd: 1\n", 0)]
    #[case::crlf("a:\r\n  b: 1\r\n  c: 2 # note\r\nd: 1\r\n", 0)]
    fn should_produce_reparseable_yaml(#[case] text: &str, #[case] line: u32) {
        assert_loads_as_one_document(&apply_block_to_flow_edit(text, line));
    }

    #[test]
    fn should_print_an_anchor_and_a_tag_once() {
        let result = apply_block_to_flow_edit("a:\n  &x !mytag\n  b: 1\nd: 1\n", 0);

        assert_eq!(count(&result, "&x"), 1, "{result:?}");
        assert_eq!(count(&result, "!mytag"), 1, "{result:?}");
        assert_loads_as_one_document(&result);
    }

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    // The edit replaces exactly the converted range and ends the new text with the
    // flow collection: the line break after it is the document's own.
    #[test]
    fn should_replace_exactly_the_converted_range() {
        let edit = block_to_flow_edit("a:\n  b: 1\n  c: 2 # note\nd: 1\n", 0).unwrap();

        assert_eq!(
            edit.range,
            Range::new(Position::new(0, 2), Position::new(2, 6))
        );
        assert_eq!(edit.new_text, " { b: 1, c: 2 }");
    }

    // Where the edit starts: right after the colon of the key, whatever lies
    // between the colon and the collection.
    #[rstest]
    #[case::plain("a:\n  b: 1\n", 0, (0, 2))]
    #[case::space_before_the_colon("a :\n  b: 1\n", 0, (0, 3))]
    #[case::blanks_after_the_colon("a:   \n  b: 1\n", 0, (0, 2))]
    #[case::tab_after_the_colon("a:\t\n  b: 1\n", 0, (0, 2))]
    #[case::blank_lines_between("a:\n \n\n  b: 1\n", 0, (0, 2))]
    #[case::anchor_on_the_key_line("a: &x\n  b: 1\n", 0, (0, 2))]
    #[case::anchor_on_its_own_line("a:\n  &x\n  b: 1\n", 0, (0, 2))]
    #[case::explicit_key("? a\n:\n  b: 1\n", 0, (1, 1))]
    #[case::explicit_key_with_comment("? a # c\n:\n  b: 1\n", 0, (1, 1))]
    #[case::quoted_key("\"k\":\n  b: 1\n", 0, (0, 4))]
    #[case::multibyte_key("ключ:\n  b: 1\n", 0, (0, 5))]
    #[case::item_of_a_sequence("- k:\n    b: 1\n", 0, (0, 4))]
    fn should_start_the_edit_right_after_the_colon(
        #[case] text: &str,
        #[case] line: u32,
        #[case] start: (u32, u32),
    ) {
        let edit = block_to_flow_edit(text, line).unwrap();

        assert_eq!(edit.range.start, Position::new(start.0, start.1));
    }

    // Where the edit ends: after the last character of the collection's content.
    // The text is `prefix + replaced + suffix`; the edit replaces `replaced`.
    #[rstest]
    #[case::comment_with_special_characters("a:", "\n  b: 1\n  c: 2", " # done: yes ] }\nd: 1\n")]
    #[case::hash_in_double_quotes("a:", "\n  c: \"x # y\"", "\nd: 1\n")]
    #[case::hash_in_single_quotes("a:", "\n  c: 'a #b'", "\nd: 1\n")]
    #[case::comment_after_a_quoted_hash("a:", "\n  c: \"x # y\"", " # real\nd: 1\n")]
    #[case::multiline_plain_scalar("a:", "\n  b: 1\n  c: first\n    second", "\n# n\nd: 1\n")]
    #[case::multiline_double_quoted_scalar("a:", "\n  c: \"first\n    second\"", "\n# n\nd: 1\n")]
    #[case::multibyte("a:", "\n  b: 日本語", " # コメント\n")]
    #[case::tab_after_the_content("a:", "\n  b: 1\n  c: 2", "\t\nd: 1\n")]
    #[case::blanks_and_tab_before_the_comment("a:", "\n  b: 1\n  c: 2", " \t# note\nd: 1\n")]
    #[case::blank_lines_with_blanks("a:", "\n  c: 2", "\n   \n\t\nd: 1\n")]
    #[case::comment_with_trailing_blanks("a:", "\n  b: 1", " # keep   \t\nd: 1\n")]
    #[case::own_line_comment_at_entry_indent("a:", "\n  b: 1", "\n  # tail\nd: 1\n")]
    #[case::empty_last_value("a:", "\n  b: 1\n  c:", "\n# n\nd: 1\n")]
    #[case::empty_last_item("a:", "\n  - x\n  -", "\n# n\nd: 1\n")]
    #[case::last_value_with_only_a_property("a:", "\n  b: 1\n  c: &x", "\n# n\nd: 1\n")]
    #[case::last_value_with_only_a_tag("a:", "\n  b: 1\n  c: !mytag", "\n# n\nd: 1\n")]
    #[case::multiline_flow_as_last_value("a:", "\n  b: 1\n  c: [\n    1\n    ]", "\n# n\nd: 1\n")]
    #[case::comment_before_the_document_start("a:", "\n  b: 1", "\n# end\n---\nc: 1\n")]
    #[case::hash_in_a_literal_block_scalar(
        "a:",
        "\n  c: |\n    x # y\n    # z\n  d: 1",
        "\nz: 1\n"
    )]
    #[case::hash_in_a_folded_block_scalar("a:", "\n  c: >\n    x # y\n    # z", "\nz: 1\n")]
    #[case::hash_in_a_multiline_double_quoted_scalar(
        "a:",
        "\n  c: \"first # x\n    second # y\"",
        "\nd: 1\n"
    )]
    #[case::hash_in_a_quoted_key("a:", "\n  \"k #1\": 2", "\nd: 1\n")]
    #[case::hash_in_a_url_fragment("a:", "\n  u: http://x/#frag", "\nd: 1\n")]
    fn should_end_the_edit_after_the_content_of_the_collection(
        #[case] prefix: &str,
        #[case] replaced: &str,
        #[case] suffix: &str,
    ) {
        let text = format!("{prefix}{replaced}{suffix}");
        let edit = block_to_flow_edit(&text, 0).unwrap();

        assert_eq!(
            apply_text_edit(&text, &edit),
            format!("{prefix}{}{suffix}", edit.new_text),
            "the edit must replace {replaced:?}"
        );
    }

    #[test]
    fn should_end_the_edit_after_a_non_breaking_space_that_ends_a_scalar() {
        let prefix_and_content = "a:\n  b: x\u{a0}";
        let text = format!("{prefix_and_content}\nd: 1\n");
        let edit = block_to_flow_edit(&text, 0).unwrap();

        assert_eq!(edit.range.end, Position::new(1, 7));
    }

    // The end of the file is the end of the document: nothing is added after the
    // collection, not even a line break.
    #[rstest]
    #[case::no_final_break("a:\n  b: 1", "a: { b: 1 }")]
    #[case::comment_without_final_break("a:\n  b: 1 # c", "a: { b: 1 } # c")]
    #[case::blanks_without_final_break("a:\n  b: 1   ", "a: { b: 1 }   ")]
    #[case::blank_lines_at_the_end("a:\n  b: 1\n\n\n", "a: { b: 1 }\n\n\n")]
    #[case::own_line_comment_without_final_break("a:\n  b: 1\n# end", "a: { b: 1 }\n# end")]
    fn should_keep_the_end_of_the_file_as_it_is(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(apply_block_to_flow_edit(text, 0), expected);
    }

    #[test]
    fn should_keep_the_line_break_after_the_collection_when_breaks_are_mixed() {
        let text = "a:\r\n  b: 1\r  c: 2 # n\nd: 1\n";

        let edit = block_to_flow_edit(text, 0).unwrap();

        assert_eq!(edit.range.end, Position::new(2, 6));
        assert_eq!(
            apply_text_edit(text, &edit),
            "a: { b: 1, c: 2 } # n\nd: 1\n"
        );
    }

    #[test]
    fn should_keep_a_byte_order_mark() {
        assert_eq!(
            apply_block_to_flow_edit("\u{feff}a:\n  b: 1\nd: 1\n", 0),
            "\u{feff}a: { b: 1 }\nd: 1\n"
        );
    }

    // Offsets come from spans of untrusted text and index the text: no prefix of
    // a document, cut at any character, may panic, and the edit of the action
    // has to be a range of the text.
    #[test]
    fn should_not_panic_or_answer_a_backwards_range_for_any_prefix() {
        let text = "# c\nk:\r\n  a: ключ # コメント\r  \"日本語\": [\n    x,\n    y ] # \u{1F600}\n  e:\n  - &x !t\n  -\n# n\n---\nz:\n  - 1\n...\n# end";
        let line_count = text.matches(['\n', '\r']).count() + 1;
        for (end, _) in text.char_indices().chain([(text.len(), ' ')]) {
            let prefix = &text[..end];
            for line in 0..line_count {
                let line = u32::try_from(line).unwrap();
                if let Some(edit) = block_to_flow_edit(prefix, line) {
                    assert!(
                        (edit.range.start.line, edit.range.start.character)
                            < (edit.range.end.line, edit.range.end.character),
                        "{prefix:?} line {line}: {:?}",
                        edit.range
                    );
                    let applied = apply_text_edit(prefix, &edit);
                    assert!(applied.contains(&edit.new_text), "{prefix:?} line {line}");
                }
            }
        }
    }

    // The flow style has no place for the comments inside the collection, so the
    // action is offered disabled instead of deleting them.
    #[test]
    fn should_print_no_comment_inside_the_flow_collection() {
        let text = "a:\n  b: 1 # c\n  d: # e\n    f: 1\n  g: x\n";

        assert_disabled(&block_to_flow_action(text, 0).expect("the action is offered"));
    }

    // A flow collection that holds a comment is printed in block style by the
    // formatter and would be invalid inside the flow collection the action
    // builds, so these comments disable the action too.
    #[rstest]
    #[case::flow_sequence_with_comment_inside_a_mapping("a:\n  b: [ # c\n    1]\n  d: 2\n")]
    #[case::flow_sequence_with_comment_inside_a_sequence("a:\n  - [ # c\n    1]\n  - 2\n")]
    #[case::flow_mapping_with_comment_inside_a_mapping("a:\n  b: { # c\n    x: 1}\n  d: 2\n")]
    #[case::flow_collection_with_comment_on_itself("a:\n  - [1] # c\n  - 2\n")]
    #[case::flow_collection_deeper_down("a:\n  b:\n    - { x: [ # c\n        1] }\n")]
    fn should_print_no_comment_of_a_flow_collection_inside_the_result(#[case] text: &str) {
        assert_disabled(&block_to_flow_action(text, 0).expect("the action is offered"));
    }

    /// The disabled action has the title and the kind of the enabled one, a
    /// reason and no edit.
    fn assert_disabled(action: &CodeAction) {
        assert_eq!(action.title, "Convert block to flow style");
        assert_eq!(action.kind, Some(CodeActionKind::REFACTOR_REWRITE));
        assert_eq!(
            action.disabled,
            Some(CodeActionDisabled {
                reason: "Flow style would lose the comments in this collection".to_string()
            })
        );
        assert_eq!(action.edit, None);
        assert_eq!(action.command, None);
        assert_eq!(action.diagnostics, None);
        assert_eq!(action.is_preferred, None);
        assert_eq!(action.data, None);
    }

    // Only comments inside the converted range disable the action: a comment
    // around it stays where it is, and a `#` in a scalar is no comment.
    #[rstest]
    #[case::behind_the_colon_of_a_scalar_value("x: 1 # c\na:\n  b: 1\n")]
    #[case::behind_the_key_of_the_last_entry_without_a_value("a:\n  b: 1\n  c: # n\nd: 1\n")]
    #[case::below_the_last_entry_without_a_value("a:\n  b: 1\n  c:\n    # n\nd: 1\n")]
    #[case::hash_in_a_plain_scalar("a:\n  c: a#b\n")]
    #[case::hash_in_a_multiline_quoted_scalar("a:\n  c: \"x\n    # y\"\n")]
    fn should_offer_the_action_enabled_when_no_comment_is_in_the_converted_range(
        #[case] text: &str,
    ) {
        let line = u32::try_from(
            text.lines()
                .position(|line| line.starts_with("a:"))
                .unwrap(),
        )
        .unwrap();
        let action = block_to_flow_action(text, line).expect("the action is offered");

        assert_eq!(action.disabled, None);
        assert!(action.edit.is_some());
    }

    // The innermost collection decides: a comment outside it, even inside the
    // outer collection, does not disable the inner action.
    #[test]
    fn should_look_only_at_the_collection_the_cursor_selects() {
        let text = "a:\n  # lead\n  b:\n    c: 1\n";

        let inner = block_to_flow_action(text, 2).expect("the action is offered");
        let outer = block_to_flow_action(text, 0).expect("the action is offered");

        assert_eq!(inner.disabled, None);
        assert!(inner.edit.is_some());
        assert!(outer.disabled.is_some());
        assert_eq!(outer.edit, None);
    }

    #[test]
    fn should_offer_no_disabled_action_when_block_style_is_enforced() {
        let text = "a: # note\n  b: 1\n";
        let options = YamlFormatOptions {
            format_enforce_block_style: true,
            ..YamlFormatOptions::default()
        };

        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
            &options,
        );

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "{actions:?}"
        );
    }

    // A comment in the converted range disables the action, whatever surrounds
    // it; the fixtures pin the position of the comment, these cases pin the rest.
    #[rstest]
    #[case::crlf_on_the_key_line("a: # note\r\n  b: 1\r\n", 0)]
    #[case::lone_cr_on_the_key_line("a: # note\r  b: 1\r", 0)]
    #[case::crlf_between_entries("a:\r\n  b: 1 # c\r\n  d: 2\r\n", 0)]
    #[case::mixed_breaks("a:\r\n  b: 1\r  # c\n  d: 2\n", 0)]
    #[case::byte_order_mark("\u{feff}a: # c\n  b: 1\n", 0)]
    #[case::multibyte_key("ключ: # コメント\n  b: 1\n", 0)]
    #[case::multibyte_inner_entry("ключ:\n  b: 日本語 # コメント\n  c: 1\n", 0)]
    #[case::astral_key("😀: # c\n  b: 1\n", 0)]
    #[case::behind_an_inner_sequence_item("a:\n  - x # c\n  - y\n", 0)]
    #[case::before_the_last_entry("a:\n  b: 1\n# c\n  d: 2\n", 0)]
    fn should_disable_whatever_surrounds_the_comment(#[case] text: &str, #[case] line: u32) {
        assert_disabled(&block_to_flow_action(text, line).expect("the action is offered"));
    }

    // A comment that lies in the converted range of another collection does not
    // disable the action of this one.
    #[rstest]
    #[case::in_a_sibling_before("x:\n  # c\n  y: 1\na:\n  b: 1\n", 3)]
    #[case::in_a_sibling_after("a:\n  b: 1\nz:\n  # c\n  y: 1\n", 0)]
    #[case::in_an_earlier_document("x: # c\n  y: 1\n---\na:\n  b: 1\n", 3)]
    #[case::after_the_document_start_marker("--- # c\na:\n  b: 1\n", 1)]
    #[case::in_the_collection_around_it("x:\n  a:\n    b: 1 # c\n", 1)]
    fn should_stay_enabled_for_a_comment_in_another_collection(
        #[case] text: &str,
        #[case] line: u32,
    ) {
        let action = block_to_flow_action(text, line).expect("the action is offered");

        assert_eq!(action.disabled, None);
        assert!(action.edit.is_some());
    }

    // Each collection answers for its own range.
    #[test]
    fn should_decide_per_key_for_nested_collections() {
        let text = "a:\n  b:\n    c: 1 # t\n  d: 2\n";

        let outer = block_to_flow_action(text, 0).expect("the action is offered");
        let inner = block_to_flow_action(text, 1).expect("the action is offered");

        assert!(outer.disabled.is_some());
        assert_eq!(inner.disabled, None);
        assert!(inner.edit.is_some());

        let text = "a: # c\n  b:\n    c: 1\n";
        let inner = block_to_flow_action(text, 1).expect("the action is offered");
        assert_eq!(inner.disabled, None);
    }

    // The action is not offered where there is no block collection to convert,
    // and a comment does not make up one.
    #[rstest]
    #[case::scalar_value("a: 1 # c\nb:\n  x: 1\n", 0)]
    #[case::empty_value("a:\n  # only a comment\nb: 1\n", 0)]
    #[case::flow_sequence("a: [1, 2] # c\n", 0)]
    #[case::flow_mapping("a: { x: 1 } # c\n", 0)]
    #[case::cursor_on_a_child_line("a:\n  b: 1 # c\n", 1)]
    #[case::document_that_does_not_parse("a: # c\n  b: [\n", 0)]
    fn should_offer_no_action_where_there_is_no_block_collection(
        #[case] text: &str,
        #[case] line: u32,
    ) {
        assert_eq!(block_to_flow_action(text, line), None);
    }

    // The action offers either an edit or a reason, and an action with an edit
    // keeps every comment of the document: for each prefix of a document and
    // each line, the comments the parser finds before and after applying the
    // edit are the same.
    #[test]
    fn should_keep_every_comment_when_the_action_has_an_edit() {
        let text = "# a\nk: # b\n  a: 1 # c\n  # d\n  e:\n    - x # f\n    # g\n    - [ # h\n      y]\n  i: |\n    # not a comment\n    z\nj:\n  k: 1\n# l\n";
        let comments = |text: &str| {
            parse_events(text)
                .filter(|event| matches!(event, Ok((Event::Comment { .. }, _))))
                .count()
        };
        let line_count = text.matches('\n').count() + 1;
        for (end, _) in text.char_indices().chain([(text.len(), ' ')]) {
            let prefix = &text[..end];
            for line in 0..line_count {
                let line = u32::try_from(line).unwrap();
                let Some(action) = block_to_flow_action(prefix, line) else {
                    continue;
                };
                assert_eq!(
                    action.disabled.is_some(),
                    action.edit.is_none(),
                    "{prefix:?} line {line}: {action:?}"
                );
                if let Some(edit) = block_to_flow_edit(prefix, line) {
                    assert_eq!(
                        comments(&apply_text_edit(prefix, &edit)),
                        comments(prefix),
                        "{prefix:?} line {line}"
                    );
                }
            }
        }
    }
}
