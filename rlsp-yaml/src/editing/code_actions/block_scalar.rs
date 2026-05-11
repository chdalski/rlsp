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
) -> Option<CodeAction> {
    let parser_line = line_idx + 1;
    let (scalar, key_col, scalar_loc, idx) = find_block_scalar_candidate(docs, parser_line)?;

    let base_indent = key_col;
    let mut block_scalar = scalar.clone();
    // Clear anchor, anchor_loc, tag, and tag_loc from the clone before formatting.
    // The edit range covers only the scalar token (not the anchor/tag prefix), so
    // the source buffer already preserves those properties. If the clone retains
    // them, format_subtree re-emits them, doubling the properties in the output.
    if let Node::Scalar {
        style, tag, meta, ..
    } = &mut block_scalar
    {
        *style = ScalarStyle::Literal(Chomp::Clip);
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

    Some(make_action(
        "Convert to block scalar".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        None,
    ))
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
    use super::super::test_helpers::apply_block_scalar_edit;

    fn count(haystack: &str, needle: &str) -> usize {
        let mut count = 0;
        let mut start = 0;
        while let Some(pos) = haystack[start..].find(needle) {
            count += 1;
            start += pos + needle.len();
        }
        count
    }

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
    }
}
