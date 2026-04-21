// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Chomp, CollectionStyle, Document, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::{block_to_flow::node_loc, make_action};

pub(super) fn string_to_block_scalar(
    docs: &[Document<Span>],
    _text: &str,
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let parser_line = line_idx + 1;
    let (scalar, key_col, scalar_loc) = find_block_scalar_candidate(docs, parser_line)?;

    let base_indent = key_col;
    let mut block_scalar = scalar.clone();
    if let Node::Scalar { style, .. } = &mut block_scalar {
        *style = ScalarStyle::Literal(Chomp::Clip);
    }

    let new_text = format_subtree(&block_scalar, &YamlFormatOptions::default(), base_indent);

    if new_text.trim().is_empty() {
        return None;
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            scalar_loc.start.line.saturating_sub(1) as u32,
            scalar_loc.start.column as u32,
        ),
        Position::new(
            scalar_loc.end.line.saturating_sub(1) as u32,
            scalar_loc.end.column as u32,
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
/// Returns `(scalar_node, key_start_column, scalar_loc)` when found.
fn find_block_scalar_candidate(
    docs: &[Document<Span>],
    parser_line: usize,
) -> Option<(&Node<Span>, usize, &Span)> {
    for doc in docs {
        if let Some(result) = find_block_scalar_in_node(&doc.root, parser_line) {
            return Some(result);
        }
    }
    None
}

fn find_block_scalar_in_node(
    node: &Node<Span>,
    parser_line: usize,
) -> Option<(&Node<Span>, usize, &Span)> {
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
                        if loc.start.line == parser_line
                            && matches!(
                                scalar_style,
                                ScalarStyle::Plain
                                    | ScalarStyle::SingleQuoted
                                    | ScalarStyle::DoubleQuoted
                            )
                            && value.chars().count() >= 40
                        {
                            let key_col = node_loc(k).start.column;
                            return Some((v, key_col, loc));
                        }
                    }
                }
                if let Some(result) = find_block_scalar_in_node(k, parser_line) {
                    return Some(result);
                }
                if let Some(result) = find_block_scalar_in_node(v, parser_line) {
                    return Some(result);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(result) = find_block_scalar_in_node(item, parser_line) {
                    return Some(result);
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
    reason = "test code"
)]
mod tests {
    use super::super::code_actions;
    use super::super::test_helpers::{apply_block_scalar_edit, cursor_range, docs_for};
    use crate::test_utils::test_uri;

    #[test]
    fn should_convert_long_string_to_block_scalar() {
        let long_value = "a".repeat(50);
        let text = format!("description: \"{long_value}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("block scalar"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].range.start.character > 0,
            "edit range must start at the scalar, not at column 0"
        );
        assert!(edits[0].new_text.contains("|\n"));
        assert!(edits[0].new_text.contains(&long_value));
    }

    #[test]
    fn should_not_offer_block_scalar_for_short_string() {
        let text = "key: \"short\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block scalar")));
    }

    #[test]
    fn should_not_offer_block_scalar_for_flow_collection() {
        let long_value = format!("{{{}:1}}", "a".repeat(50));
        let text = format!("key: {long_value}\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );

        assert!(actions.iter().all(|a| !a.title.contains("block scalar")));
    }

    #[test]
    fn should_not_offer_block_scalar_for_scalar_in_flow_mapping_value() {
        let long = "a".repeat(50);
        let text = format!("{{key: \"{long}\"}}\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for a value inside a flow mapping"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_scalar_in_nested_flow_mapping() {
        let long = "a".repeat(50);
        let text = format!("outer: {{key: \"{long}\"}}\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for a value inside a flow mapping nested in a block mapping"
        );
    }

    // ---- qualifying criteria ----

    #[test]
    fn should_offer_block_scalar_for_plain_scalar_mapping_value() {
        let text = "description: this is a very long plain scalar value that exceeds forty chars\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("block scalar")),
            "expected block-scalar action for plain mapping value"
        );
    }

    #[test]
    fn should_offer_block_scalar_for_single_quoted_mapping_value() {
        let text =
            "description: 'this is a very long single-quoted scalar that exceeds forty chars'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("block scalar")),
            "expected block-scalar action for single-quoted mapping value"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_already_literal_block_scalar() {
        let text =
            "description: |\n  this is a very long literal block scalar that exceeds forty chars\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar action for already-literal value"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_already_folded_block_scalar() {
        let text =
            "description: >\n  this is a very long folded block scalar that exceeds forty chars\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar action for already-folded value"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_value_below_char_threshold() {
        let text = "key: \"short string under forty\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar action for value below 40 char threshold"
        );
    }

    #[test]
    fn should_use_char_count_not_byte_length_for_threshold() {
        let text = "key: \"αβγδεζηθ\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for value with 8 chars (< 40), even if byte length > 8"
        );
    }

    #[test]
    fn should_offer_block_scalar_when_char_count_meets_threshold_with_multibyte() {
        let value = "α".repeat(40);
        let text = format!("key: \"{value}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().any(|a| a.title.contains("block scalar")),
            "must offer block-scalar when char count meets 40-char threshold"
        );
    }

    #[test]
    fn block_scalar_not_offered_when_cursor_on_different_line() {
        let long = "x".repeat(50);
        let text = format!("key: short\nother: \"{long}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar when cursor is on a different line from the long scalar"
        );
    }

    #[test]
    fn block_scalar_indentation_follows_key_column() {
        let long = "a".repeat(50);
        let text = format!("  key: \"{long}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to block scalar")
            .expect("expected block-scalar action");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        let body_line = new_text
            .lines()
            .nth(1)
            .expect("new_text must have a second line (the scalar body)");
        assert!(
            body_line.starts_with("    ") && !body_line.starts_with("     "),
            "scalar body must be indented by exactly 4 spaces (key_col 2 + tab_width 2), got: {body_line:?}"
        );
    }

    #[test]
    fn block_scalar_title_is_convert_to_block_scalar() {
        let long = "a".repeat(50);
        let text = format!("key: \"{long}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to block scalar")
            .expect("action with exact title 'Convert to block scalar' must be present");
        assert_eq!(
            action.title, "Convert to block scalar",
            "title must be exactly 'Convert to block scalar'"
        );
    }

    // ---- defect class regressions ----

    #[test]
    fn should_resolve_escape_sequences_in_double_quoted_value() {
        let text = "summary: \"line one\\nline two\\ttabbed\\\\backslash\\\"quote and more padding here\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            !result.contains("\\n"),
            "block scalar must not contain literal \\n escape: {result:?}"
        );
        assert!(
            !result.contains("\\t"),
            "block scalar must not contain literal \\t escape: {result:?}"
        );
        assert!(
            result.contains('\n'),
            "block scalar must contain actual newline: {result:?}"
        );
        assert!(
            result.contains('\t'),
            "block scalar must contain actual tab: {result:?}"
        );
        assert!(
            result.contains('\\'),
            "block scalar must contain literal backslash: {result:?}"
        );
        assert!(
            result.contains('"'),
            "block scalar must contain literal double-quote: {result:?}"
        );
    }

    #[test]
    fn block_scalar_double_quoted_backslash_and_quote_resolved() {
        let text = "key: \"contains \\\\backslash and \\\"quote\\\" here plus some extra padding chars\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains('\\'),
            "block scalar must contain a literal backslash: {result:?}"
        );
        assert!(
            result.contains('"'),
            "block scalar must contain a literal double-quote: {result:?}"
        );
    }

    #[test]
    fn block_scalar_uses_char_count_not_byte_count_for_threshold() {
        let val: String = "é".repeat(39);
        let text = format!("key: \"{val}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for 39 chars (< threshold), even though byte len = 78"
        );
    }

    #[test]
    fn should_resolve_single_quoted_escape_to_literal_apostrophe() {
        let text = "note: 'it''s a long string that should exceed the forty character threshold'\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("it's"),
            "block scalar must contain resolved apostrophe: {result:?}"
        );
        assert!(
            !result.contains("it''s"),
            "block scalar must not contain the '' escape sequence: {result:?}"
        );
    }

    #[test]
    fn should_not_be_fooled_by_colon_in_url_value() {
        let text = "homepage: \"https://example.com/very-long-path-that-exceeds-forty-chars\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("https://example.com/very-long-path-that-exceeds-forty-chars"),
            "full URL must be preserved in block scalar output: {result:?}"
        );
        assert!(
            result.contains("|\n"),
            "output must be a literal block scalar: {result:?}"
        );
    }

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

    #[test]
    fn should_not_be_fooled_by_colon_in_quoted_key() {
        let text = "\"foo:bar\": \"this is a long mapping value that exceeds forty characters\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("this is a long mapping value that exceeds forty characters"),
            "actual value must be preserved: {result:?}"
        );
        assert!(
            result.contains("|\n"),
            "output must be literal block scalar: {result:?}"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_sequence_item() {
        let text = "- \"this is a very long sequence item value that exceeds forty characters\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for sequence item (only mapping values qualify)"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_flow_sequence_value() {
        let text = "key: [one, two, three, four, five, six, seven, eight, nine, ten]\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar when mapping value is a flow sequence"
        );
    }

    #[test]
    fn should_preserve_anchor_when_converting_to_block_scalar() {
        let text =
            "description: &myanchor \"this is a long string that exceeds forty characters\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("&myanchor"),
            "anchor must be preserved in block scalar output: {result:?}"
        );
    }
}
