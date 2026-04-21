// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Document, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::make_action;

pub(super) fn quoted_bool_to_unquoted(
    docs: &[Document<Span>],
    line_idx: usize,
    col: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let parser_line = line_idx + 1;
    let scalar = find_quoted_bool_scalar(docs, parser_line, col)?;

    let Node::Scalar { value, loc, .. } = scalar else {
        return None;
    };

    let mut plain = scalar.clone();
    if let Node::Scalar { style, .. } = &mut plain {
        *style = ScalarStyle::Plain;
    }

    let base_indent = loc.start.column;
    let new_text = format_subtree(&plain, &YamlFormatOptions::default(), base_indent);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            loc.start.column as u32,
        ),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
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
) -> Option<&Node<Span>> {
    for doc in docs {
        if let Some(node) = find_quoted_bool_in_node(&doc.root, parser_line, col) {
            return Some(node);
        }
    }
    None
}

fn find_quoted_bool_in_node(
    node: &Node<Span>,
    parser_line: usize,
    col: usize,
) -> Option<&Node<Span>> {
    match node {
        Node::Scalar {
            style: ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted,
            value,
            loc,
            ..
        } if loc.start.line == parser_line
            && col >= loc.start.column
            && col <= loc.end.column
            && (value == "true" || value == "false") =>
        {
            Some(node)
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_quoted_bool_in_node(k, parser_line, col) {
                    return Some(found);
                }
                if let Some(found) = find_quoted_bool_in_node(v, parser_line, col) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_quoted_bool_in_node(item, parser_line, col) {
                    return Some(found);
                }
            }
            None
        }
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
    use tower_lsp::lsp_types::CodeActionKind;

    use super::super::code_actions;
    use super::super::test_helpers::{cursor_range, docs_for};
    use crate::test_utils::test_uri;

    #[test]
    fn should_convert_double_quoted_true_to_unquoted() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("true")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn should_convert_single_quoted_false_to_unquoted() {
        let text = "enabled: 'false'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("false")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn should_not_offer_bool_conversion_for_non_bool_string() {
        let text = "name: \"hello\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_convert_double_quoted_false_to_unquoted() {
        let text = "flag: \"false\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("false")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn should_convert_single_quoted_true_to_unquoted() {
        let text = "active: 'true'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("true")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
    }

    #[test]
    fn should_not_offer_bool_conversion_for_plain_true() {
        let text = "enabled: true\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_not_offer_bool_conversion_for_case_variant_true() {
        let text = "flag: \"True\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_not_offer_bool_conversion_for_uppercase_false() {
        let text = "flag: \"FALSE\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_not_offer_bool_conversion_when_cursor_before_scalar() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 8), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_offer_bool_conversion_when_cursor_at_scalar_start_column() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        assert!(actions.iter().any(|a| a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_edit_range_is_scalar_span_not_full_line() {
        let text = "enabled: \"true\"  # keep this comment\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

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
    fn quoted_bool_action_offered_for_second_document() {
        let text = "key: value\n---\nflag: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(2, 7), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool in second document"
        );
    }

    #[test]
    fn quoted_bool_action_offered_inside_flow_sequence() {
        let text = "items: [\"true\", \"false\"]\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool inside flow sequence"
        );
    }

    #[test]
    fn quoted_bool_action_not_offered_when_cursor_on_different_line() {
        let text = "a: true\nb: \"false\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 3), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_offer_bool_conversion_when_cursor_at_scalar_end_column() {
        let text = "enabled: \"true\"\n";
        let docs = docs_for(text);
        let actions = code_actions(&docs, text, cursor_range(0, 15), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "cursor at loc.end.column (exclusive end) must still trigger action"
        );
        let actions_past = code_actions(&docs, text, cursor_range(0, 16), &[], &test_uri());
        assert!(
            actions_past
                .iter()
                .all(|a| !a.title.contains("Convert quoted")),
            "cursor past loc.end.column must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_action_offered_for_sequence_item() {
        let text = "items:\n  - \"true\"\n  - \"false\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 5), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool as a sequence item"
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
        );
        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_action_offered_for_unicode_escaped_true() {
        let text = "flag: \"\\u0074rue\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 8), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "decoded value 'true' via unicode escape must trigger action"
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].new_text, "true",
            "new_text must be plain 'true', not the unicode-escaped form"
        );
    }

    #[test]
    fn quoted_bool_action_kind_is_quickfix() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

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

    #[test]
    fn quoted_bool_action_title_uses_plain_value() {
        let text = "flag: 'false'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 8), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        assert_eq!(action.title, "Convert quoted string to false");
    }

    #[test]
    fn quoted_bool_cursor_at_closing_quote_column_offers_action() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 14), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "cursor at closing-quote column must still trigger action"
        );
    }

    #[test]
    fn quoted_bool_cursor_after_scalar_end_offers_no_action() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 16), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "cursor past scalar exclusive end must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_mixed_case_offers_no_action() {
        let text = "enabled: \"tRuE\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_literal_block_scalar_offers_no_action() {
        let text = "enabled: |\n  true\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 3), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "literal block scalar must not trigger bool conversion"
        );
    }

    #[test]
    fn quoted_bool_inside_flow_mapping_value_offers_action() {
        let text = "config: {enabled: \"true\"}\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 19), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool as a flow mapping value"
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
    }

    #[test]
    fn quoted_bool_pattern_inside_longer_scalar_not_offered() {
        let text = "msg: 'status ''true'' reported'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "longer scalar containing 'true' substring must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_multiple_bools_same_line_cursor_on_first() {
        let text = "x: { a: \"true\", b: \"false\" }\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .expect("must offer action for first bool");
        assert_eq!(
            action.title, "Convert quoted string to true",
            "cursor on first bool must offer 'true', not 'false'"
        );
    }

    #[test]
    fn quoted_bool_multiple_bools_same_line_cursor_on_second() {
        let text = "x: { a: \"true\", b: \"false\" }\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 20), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .expect("must offer action for second bool");
        assert_eq!(
            action.title, "Convert quoted string to false",
            "cursor on second bool must offer 'false', not 'true'"
        );
    }

    #[test]
    fn quoted_bool_flow_context_rest_of_mapping_preserved() {
        let text = "config: { a: \"true\", b: 1 }\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 14), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .expect("must offer action inside flow mapping");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_ne!(
            edits[0].range.start.character, 0,
            "edit must not replace from col 0 — that would destroy surrounding flow content"
        );
        assert!(
            edits[0].range.end.character <= 19,
            "edit end must not extend past the closing quote of the scalar"
        );
    }

    #[test]
    fn quoted_bool_value_with_leading_whitespace_not_offered() {
        let text = "key: \" true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "scalar with leading whitespace in decoded value must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_non_bool_string_not_offered() {
        let text = "key: \"hello\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 6), &[], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_already_plain_not_offered() {
        let text = "key: true\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 6), &[], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_cursor_on_trailing_comment_not_offered() {
        let text = "key: \"true\"  # comment\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 14), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "cursor on trailing comment must not trigger action"
        );
    }
}
