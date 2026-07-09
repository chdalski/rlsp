// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Document, LineIndex, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::{block_to_flow::node_loc, diagnostic_code, make_action};

pub(super) fn yaml11_bool_actions(
    docs: &[Document<Span>],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
    options: &YamlFormatOptions,
) -> Vec<CodeAction> {
    let Some((scalar, loc, base_indent, idx)) = find_yaml11_bool_scalar(docs, diag) else {
        return vec![];
    };
    let Node::Scalar { value, .. } = scalar else {
        return vec![];
    };

    // The edit range covers only the scalar token (not the preceding anchor/tag prefix).
    // Clear properties from both clones so format_subtree does not re-emit them in new_text,
    // which would double them — the source buffer already preserves the single occurrence.
    let mut quoted = scalar.clone();
    if let Node::Scalar {
        style, tag, meta, ..
    } = &mut quoted
    {
        *style = ScalarStyle::DoubleQuoted;
        *tag = None;
        if let Some(m) = meta.as_mut() {
            m.anchor = None;
            m.anchor_loc = None;
            m.tag_loc = None;
        }
    }
    let mut plain = scalar.clone();
    if let Node::Scalar {
        style,
        value: v,
        tag,
        meta,
        ..
    } = &mut plain
    {
        *style = ScalarStyle::Plain;
        *v = crate::scalar_helpers::yaml11_bool_canonical(value).to_string();
        *tag = None;
        if let Some(m) = meta.as_mut() {
            m.anchor = None;
            m.anchor_loc = None;
            m.tag_loc = None;
        }
    }

    let quote_opts = YamlFormatOptions {
        preserve_quotes: true,
        ..options.clone()
    };
    let quoted_text = format_subtree(&quoted, &quote_opts, base_indent);
    let plain_text = format_subtree(&plain, options, base_indent);
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

    vec![
        make_action(
            "Quote value".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: quoted_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
        make_action(
            "Convert to boolean".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: plain_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
    ]
}

pub(super) fn schema_yaml11_bool_type_actions(
    docs: &[Document<Span>],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
    options: &YamlFormatOptions,
) -> Vec<CodeAction> {
    let Some((scalar, loc, base_indent, idx)) = find_yaml11_bool_scalar(docs, diag) else {
        return vec![];
    };
    let Node::Scalar { value, .. } = scalar else {
        return vec![];
    };

    let mut plain = scalar.clone();
    // The edit range covers only the scalar token; clear properties from the clone so
    // format_subtree does not re-emit them and double the single source occurrence.
    if let Node::Scalar {
        style,
        value: v,
        tag,
        meta,
        ..
    } = &mut plain
    {
        *style = ScalarStyle::Plain;
        *v = crate::scalar_helpers::yaml11_bool_canonical(value).to_string();
        *tag = None;
        if let Some(m) = meta.as_mut() {
            m.anchor = None;
            m.anchor_loc = None;
            m.tag_loc = None;
        }
    }

    let plain_text = format_subtree(&plain, options, base_indent);
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

    vec![make_action(
        "Convert to boolean".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text: plain_text,
        }],
        CodeActionKind::QUICKFIX,
        Some(vec![diag.clone()]),
    )]
}

/// Walk the AST to find a plain YAML 1.1 boolean scalar whose span matches the diagnostic.
fn find_yaml11_bool_scalar<'a>(
    docs: &'a [Document<Span>],
    diag: &Diagnostic,
) -> Option<(&'a Node<Span>, &'a Span, usize, &'a LineIndex)> {
    let col_match = diagnostic_code(diag) == Some("yaml11Boolean");
    let parser_line = diag.range.start.line as usize + 1;
    if !col_match {
        let count: usize = docs
            .iter()
            .map(|doc| count_yaml11_bool_on_line(&doc.root, parser_line, doc.line_index()))
            .sum();
        if count != 1 {
            return None;
        }
    }
    for doc in docs {
        let idx = doc.line_index();
        if let Some((node, loc, col)) =
            find_yaml11_bool_in_node(&doc.root, parser_line, diag, col_match, idx)
        {
            return Some((node, loc, col, idx));
        }
    }
    None
}

/// Count plain yaml11 bool scalars on `parser_line` (1-based) in the AST subtree.
fn count_yaml11_bool_on_line(node: &Node<Span>, parser_line: usize, idx: &LineIndex) -> usize {
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .map(|(k, v)| {
                let v_count = if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = v
                {
                    usize::from(
                        idx.line_column(loc.start).0 as usize == parser_line
                            && crate::scalar_helpers::is_yaml11_bool(value),
                    )
                } else {
                    count_yaml11_bool_on_line(v, parser_line, idx)
                };
                count_yaml11_bool_on_line(k, parser_line, idx) + v_count
            })
            .sum(),
        Node::Sequence { items, .. } => items
            .iter()
            .map(|item| {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = item
                {
                    usize::from(
                        idx.line_column(loc.start).0 as usize == parser_line
                            && crate::scalar_helpers::is_yaml11_bool(value),
                    )
                } else {
                    count_yaml11_bool_on_line(item, parser_line, idx)
                }
            })
            .sum(),
        Node::Scalar { .. } | Node::Alias { .. } => 0,
    }
}

fn yaml11_bool_col_matches_diag(loc: Span, diag: &Diagnostic, idx: &LineIndex) -> bool {
    diag.range.start.character as usize == idx.line_column(loc.start).1 as usize
        && diag.range.end.character as usize == idx.line_column(loc.end).1 as usize
}

fn find_yaml11_bool_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    diag: &Diagnostic,
    col_match: bool,
    idx: &LineIndex,
) -> Option<(&'a Node<Span>, &'a Span, usize)> {
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = v
                    && idx.line_column(loc.start).0 as usize == parser_line
                    && crate::scalar_helpers::is_yaml11_bool(value)
                    && (!col_match || yaml11_bool_col_matches_diag(*loc, diag, idx))
                {
                    let key_col = idx.line_column(node_loc(k).start).1 as usize;
                    return Some((v, loc, key_col));
                }
                if let Some(result) = find_yaml11_bool_in_node(k, parser_line, diag, col_match, idx)
                {
                    return Some(result);
                }
                if let Some(result) = find_yaml11_bool_in_node(v, parser_line, diag, col_match, idx)
                {
                    return Some(result);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = item
                    && idx.line_column(loc.start).0 as usize == parser_line
                    && crate::scalar_helpers::is_yaml11_bool(value)
                    && (!col_match || yaml11_bool_col_matches_diag(*loc, diag, idx))
                {
                    return Some((item, loc, idx.line_column(loc.start).1 as usize));
                }
                if let Some(result) =
                    find_yaml11_bool_in_node(item, parser_line, diag, col_match, idx)
                {
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
    use rstest::rstest;

    use tower_lsp::lsp_types::NumberOrString;

    use super::super::code_actions;
    use super::super::diagnostic_code;
    use super::super::test_helpers::{
        apply_yaml11_bool_convert_edit, apply_yaml11_bool_quote_edit, docs_for, line_range,
        make_diagnostic,
    };
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
    // The fix clears properties from both cloned nodes before formatting, so new_text
    // contains zero occurrences — the source buffer preserves the single occurrence.
    // The final document therefore contains exactly one occurrence.

    #[test]
    fn yaml11_bool_quote_action_new_text_does_not_duplicate_anchor() {
        let text = "enabled: &myanchor yes\n";
        let diag = make_diagnostic(0, 19, 22, "yaml11Boolean");
        let (result, edit) = apply_yaml11_bool_quote_edit(text, diag);
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
    fn yaml11_bool_quote_action_new_text_does_not_duplicate_user_tag() {
        let text = "enabled: !mytag yes\n";
        let diag = make_diagnostic(0, 16, 19, "yaml11Boolean");
        let (result, edit) = apply_yaml11_bool_quote_edit(text, diag);
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
    fn yaml11_bool_quote_action_new_text_does_not_duplicate_anchor_or_tag() {
        let text = "enabled: &a !mytag yes\n";
        let diag = make_diagnostic(0, 19, 22, "yaml11Boolean");
        let (result, edit) = apply_yaml11_bool_quote_edit(text, diag);
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
    fn yaml11_bool_convert_action_new_text_does_not_duplicate_anchor() {
        let text = "enabled: &myanchor yes\n";
        let diag = make_diagnostic(0, 19, 22, "yaml11Boolean");
        let (result, edit) = apply_yaml11_bool_convert_edit(text, diag);
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
    fn yaml11_bool_convert_action_new_text_does_not_duplicate_user_tag() {
        let text = "enabled: !mytag yes\n";
        let diag = make_diagnostic(0, 16, 19, "yaml11Boolean");
        let (result, edit) = apply_yaml11_bool_convert_edit(text, diag);
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
    fn yaml11_bool_convert_action_new_text_does_not_duplicate_anchor_or_tag() {
        let text = "enabled: &a !mytag yes\n";
        let diag = make_diagnostic(0, 19, 22, "yaml11Boolean");
        let (result, edit) = apply_yaml11_bool_convert_edit(text, diag);
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
    fn should_quote_yaml11_bool_yes_lowercase() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(
            edits[0].range.start.character, 9,
            "edit must start at scalar col"
        );
    }

    #[test]
    fn should_quote_yaml11_bool_uppercase_on() {
        let text = "flag: ON\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"ON\"");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
    }

    #[test]
    fn should_quote_yaml11_bool_with_indentation() {
        let text = "  enabled: yes\n";
        let diag = make_diagnostic(0, 11, 14, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(
            edits[0].range.start.character, 11,
            "edit must start at scalar col"
        );
    }

    #[test]
    fn yaml11_bool_quote_wrong_diagnostic_code_no_action() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "flowMap");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        assert!(actions.iter().all(|a| a.title != "Quote value"));
    }

    #[test]
    fn should_convert_yaml11_bool_yes_to_true() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn should_convert_yaml11_bool_no_to_false() {
        let text = "enabled: No\n";
        let diag = make_diagnostic(0, 9, 11, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn should_convert_yaml11_bool_on_to_true() {
        let text = "flag: ON\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn should_convert_yaml11_bool_off_to_false() {
        let text = "flag: OFF\n";
        let diag = make_diagnostic(0, 6, 9, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn should_convert_yaml11_bool_y_to_true() {
        let text = "active: Y\n";
        let diag = make_diagnostic(0, 8, 9, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 8);
    }

    #[test]
    fn should_convert_yaml11_bool_n_to_false() {
        let text = "active: N\n";
        let diag = make_diagnostic(0, 8, 9, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 8);
    }

    #[test]
    fn should_convert_yaml11_bool_preserving_indentation() {
        let text = "  active: yes\n";
        let diag = make_diagnostic(0, 10, 13, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(
            edits[0].range.start.character, 10,
            "edit must start at scalar col"
        );
    }

    #[test]
    fn yaml11_bool_produces_exactly_two_actions() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        assert_eq!(
            actions
                .iter()
                .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
                .count(),
            2
        );
    }

    #[test]
    fn yaml11_bool_actions_attach_diagnostic() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            std::slice::from_ref(&diag),
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        for action in actions
            .iter()
            .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
        {
            let attached = action.diagnostics.as_ref().unwrap();
            assert_eq!(attached.len(), 1);
            assert_eq!(
                attached[0].code,
                Some(NumberOrString::String("yaml11Boolean".to_string()))
            );
        }
    }

    // Quote action produces valid double-quoted YAML (round-trip check).
    #[test]
    fn yaml11_bool_quote_value_produces_valid_double_quoted_yaml() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "enabled: yes";
        let result = format!("{}{}{}\n", &line[..start], new_text, &line[end..]);
        let parse_result = crate::parser::parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "quoted bool must produce valid YAML; got: {:?}\nresult: {result:?}",
            parse_result.diagnostics
        );
        assert_eq!(
            new_text, "\"yes\"",
            "quote action must wrap scalar in double quotes"
        );
    }

    #[test]
    fn yaml11_bool_convert_action_edit_range_targets_scalar_span() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].range.start.character, 9,
            "convert edit must start at scalar col"
        );
        assert_eq!(
            edits[0].range.end.character, 12,
            "convert edit must end at scalar end"
        );
    }

    #[rstest]
    #[case::yes_lowercase("yes", "true")]
    #[case::yes_titlecase("Yes", "true")]
    #[case::yes_uppercase("YES", "true")]
    #[case::on_lowercase("on", "true")]
    #[case::on_titlecase("On", "true")]
    #[case::on_uppercase("ON", "true")]
    #[case::y_lowercase("y", "true")]
    #[case::y_uppercase("Y", "true")]
    #[case::no_lowercase("no", "false")]
    #[case::no_titlecase("No", "false")]
    #[case::no_uppercase("NO", "false")]
    #[case::off_lowercase("off", "false")]
    #[case::off_titlecase("Off", "false")]
    #[case::off_uppercase("OFF", "false")]
    #[case::n_lowercase("n", "false")]
    #[case::n_uppercase("N", "false")]
    fn yaml11_bool_convert_normalizes_all_16_tokens(#[case] token: &str, #[case] expected: &str) {
        let text = format!("flag: {token}\n");
        let col = 6u32;
        let end = col + u32::try_from(token.len()).unwrap();
        let diag = make_diagnostic(0, col, end, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].new_text, expected,
            "token {token:?} must convert to {expected:?}"
        );
    }

    #[rstest]
    #[case::yaml11_bool_code("yaml11Boolean")]
    #[case::schema_yaml11_bool_code("schemaYaml11Boolean")]
    fn yaml11_bool_actions_both_diag_codes_produce_two_actions(#[case] code: &str) {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, code);
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
                .count(),
            2,
            "diag code {code:?} must produce two actions"
        );
    }

    #[test]
    fn yaml11_bool_actions_out_of_range_diag_returns_empty() {
        let text = "enabled: yes\nother: string\n";
        let diag = make_diagnostic(1, 7, 13, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(1),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Quote value" && a.title != "Convert to boolean"),
            "diag on non-yaml11-bool line must produce no yaml11-bool actions"
        );
    }

    #[test]
    fn yaml11_bool_trailing_comment_preserved_quote_action() {
        let text = "enabled: yes  # keep this\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].new_text, "\"yes\"",
            "new_text must be just the quoted scalar"
        );
        assert!(
            edits[0].range.end.character <= 12,
            "edit end must not reach into the trailing comment: {:?}",
            edits[0].range
        );
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "enabled: yes  # keep this";
        let result = format!("{}{}{}\n", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive in result: {result:?}"
        );
    }

    #[test]
    fn yaml11_bool_trailing_comment_preserved_convert_action() {
        let text = "flag: ON  # keep this\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert!(
            edits[0].range.end.character <= 8,
            "edit end must not reach into the trailing comment: {:?}",
            edits[0].range
        );
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "flag: ON  # keep this";
        let result = format!("{}{}{}\n", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive: {result:?}"
        );
    }

    #[test]
    fn yaml11_bool_sequence_item_edit_starts_at_scalar_col() {
        let text = "items:\n  - yes\n";
        let diag = make_diagnostic(1, 4, 7, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(1),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 for sequence-item value: {:?}",
            edits[0].range
        );
        assert_eq!(edits[0].new_text, "\"yes\"");
    }

    // schema_yaml11_bool_type_actions returns exactly ONE action (no "Quote value").
    #[test]
    fn schema_yaml11_bool_type_returns_exactly_one_action() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let count = actions
            .iter()
            .filter(|a| a.title == "Convert to boolean" || a.title == "Quote value")
            .count();
        assert_eq!(
            count, 1,
            "schemaYaml11BooleanType must offer exactly one action"
        );
        assert!(
            actions.iter().any(|a| a.title == "Convert to boolean"),
            "the single action must be 'Convert to boolean'"
        );
        assert!(
            actions.iter().all(|a| a.title != "Quote value"),
            "schemaYaml11BooleanType must not offer 'Quote value'"
        );
    }

    #[test]
    fn schema_yaml11_bool_type_gated_on_yaml11_bool() {
        let text = "enabled: hello\n";
        let diag = make_diagnostic(0, 9, 14, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "non-yaml11-bool input must not produce 'Convert to boolean' for schemaYaml11BooleanType"
        );
    }

    #[test]
    fn schema_yaml11_bool_type_actions_edit_range_targets_scalar_span() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
        assert_eq!(
            edits[0].range.end.character, 9,
            "edit must end at scalar end"
        );
        assert_eq!(edits[0].new_text, "true");
    }

    #[test]
    fn schema_yaml11_bool_type_actions_out_of_range_diag_returns_empty() {
        let text = "flag: yes\nother: string\n";
        let diag = make_diagnostic(1, 7, 13, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(1),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "diag on non-yaml11-bool line must produce no schema-yaml11-bool actions"
        );
    }

    // ---- Schema-code line-ambiguity guard regression tests ----

    #[test]
    fn schema_yaml11_bool_two_bools_on_line_offers_no_action() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 9, 10, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Quote value" && a.title != "Convert to boolean"),
            "ambiguous line must suppress schema bool actions; got: {actions:?}"
        );
    }

    #[test]
    fn schema_yaml11_bool_two_bools_on_line_first_key_also_suppressed() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 1, 2, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Quote value" && a.title != "Convert to boolean"),
            "first key on ambiguous line must also be suppressed; got: {actions:?}"
        );
    }

    #[test]
    fn schema_yaml11_bool_single_bool_on_line_offers_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 0, 4, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must target `yes` at col 6"
        );
    }

    #[test]
    fn schema_yaml11_bool_single_bool_on_line_two_actions_count() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 0, 4, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
                .count(),
            2,
            "single-bool line must still offer two actions for schemaYaml11Boolean"
        );
    }

    #[test]
    fn schema_yaml11_bool_type_two_bools_on_line_offers_no_action() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 9, 10, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "ambiguous line must suppress schemaYaml11BooleanType action; got: {actions:?}"
        );
    }

    #[test]
    fn schema_yaml11_bool_type_single_bool_on_line_offers_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 0, 4, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn schema_yaml11_bool_two_bools_nested_offers_no_action() {
        let text = "x:\n  a: yes\n  b: no\n";
        let diag = make_diagnostic(1, 2, 3, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(1),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(
            edits[0].range.start.character, 5,
            "edit must target `yes` at col 5"
        );
    }

    #[test]
    fn schema_yaml11_bool_flow_map_value_two_bools_same_line_suppressed() {
        let text = "x: {a: yes, b: no}\n";
        let diag = make_diagnostic(0, 4, 5, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to boolean" && a.title != "Quote value"),
            "nested flow map two-bool line must be suppressed; got: {actions:?}"
        );
    }

    // ---- Multi-bool-per-line column-awareness regression tests ----

    #[test]
    fn yaml11_bool_flow_seq_second_bool_quote_action_targets_correct_scalar() {
        let text = "items: [yes, no]\n";
        let diag = make_diagnostic(0, 13, 15, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"no\"", "must quote `no`, not `yes`");
        assert_eq!(
            edits[0].range.start.character, 13,
            "edit must start at col 13 (`no`)"
        );
    }

    #[test]
    fn yaml11_bool_flow_seq_second_bool_convert_action_targets_correct_scalar() {
        let text = "items: [yes, no]\n";
        let diag = make_diagnostic(0, 13, 15, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false", "must convert `no` → `false`");
        assert_eq!(edits[0].range.start.character, 13);
    }

    #[test]
    fn yaml11_bool_flow_seq_first_bool_not_displaced_when_second_is_targeted() {
        let text = "items: [yes, no]\n";
        let diag = make_diagnostic(0, 8, 11, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"", "must quote `yes`, not `no`");
        assert_eq!(edits[0].range.start.character, 8);
    }

    #[test]
    fn yaml11_bool_flow_seq_second_of_three_bools_targeted_correctly() {
        let text = "flags: [yes, no, on]\n";
        let diag = make_diagnostic(0, 13, 15, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false", "must convert `no` → `false`");
        assert_eq!(edits[0].range.start.character, 13);
    }

    #[test]
    fn yaml11_bool_flow_map_second_bool_quote_action_targets_correct_scalar() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 12, 14, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"no\"", "must quote `no`, not `yes`");
        assert_eq!(edits[0].range.start.character, 12);
    }

    #[test]
    fn yaml11_bool_flow_map_second_bool_convert_action_targets_correct_scalar() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 12, 14, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 12);
    }

    #[test]
    fn yaml11_bool_flow_map_first_bool_not_displaced_when_second_is_targeted() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 4, 7, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(edits[0].range.start.character, 4);
    }

    #[test]
    fn yaml11_bool_nested_flow_seq_second_bool_targeted_correctly() {
        let text = "x:\n  flags: [yes, no]\n";
        let diag = make_diagnostic(1, 15, 17, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(1),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false", "must convert `no` → `false`");
        assert_eq!(edits[0].range.start.character, 15);
    }

    #[test]
    fn yaml11_bool_on_line_other_than_zero() {
        let text = "key: value\nflag: yes\n";
        let diag = make_diagnostic(1, 6, 9, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(1),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].range.start.line, 1, "edit must target line 1");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
        assert_eq!(edits[0].new_text, "\"yes\"");
    }

    #[test]
    fn yaml11_bool_diagnostic_not_triggered_by_other_codes() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 0, 12, "flowMap");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        assert!(actions.iter().all(|a| a.title != "Quote value"));
        assert!(actions.iter().all(|a| a.title != "Convert to boolean"));
    }

    // ════════════════════════════════════════════════════════════════════
    // Group E: schemaYaml11Boolean code actions
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn schema_yaml11_boolean_quote_value_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn schema_yaml11_boolean_convert_to_boolean_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn schema_yaml11_boolean_offers_exactly_two_actions() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let count = actions
            .iter()
            .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
            .count();
        assert_eq!(count, 2);
    }

    #[test]
    fn schema_yaml11_boolean_actions_attach_diagnostic() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            std::slice::from_ref(&diag),
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        for action in actions
            .iter()
            .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
        {
            let diags = action.diagnostics.as_ref().unwrap();
            assert_eq!(diags.len(), 1);
            assert_eq!(
                diagnostic_code(&diags[0]),
                Some("schemaYaml11Boolean"),
                "action '{}' should attach schemaYaml11Boolean diagnostic",
                action.title
            );
        }
    }

    #[test]
    fn schema_yaml11_boolean_converts_false_family_to_false() {
        let text = "flag: NO\n";
        let diag = make_diagnostic(0, 6, 8, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 6);
    }

    // ════════════════════════════════════════════════════════════════════
    // Group G: enhanced schemaYaml11BooleanType code action
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn schema_yaml11_boolean_type_convert_to_boolean_action() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn schema_yaml11_boolean_type_converts_false_family_correctly() {
        let text = "enabled: OFF\n";
        let diag = make_diagnostic(0, 9, 12, "schemaYaml11BooleanType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn schema_type_generic_no_convert_to_boolean_action() {
        let text = "enabled: hello\n";
        let diag = make_diagnostic(0, 9, 14, "schemaType");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "generic schemaType should not offer 'Convert to boolean': {actions:?}"
        );
    }
}
