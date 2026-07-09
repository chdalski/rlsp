// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Document, LineIndex, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};
use crate::lsp_util::span_to_lsp;

use super::{block_to_flow::node_loc, diagnostic_code, make_action};

pub(super) fn yaml11_octal_actions(
    docs: &[Document<Span>],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
    options: &YamlFormatOptions,
) -> Vec<CodeAction> {
    let Some((scalar, loc, base_indent, idx)) = find_yaml11_octal_scalar(docs, diag) else {
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
    let mut converted = scalar.clone();
    if let Node::Scalar {
        style,
        value: v,
        tag,
        meta,
        ..
    } = &mut converted
    {
        *style = ScalarStyle::Plain;
        *v = format!("0o{}", &value[1..]);
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
    let converted_text = format_subtree(&converted, options, base_indent);
    let edit_range = span_to_lsp(*loc, idx);

    vec![
        make_action(
            "Quote as string".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: quoted_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
        make_action(
            "Convert to YAML 1.2 octal".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: converted_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
    ]
}

fn find_yaml11_octal_scalar<'a>(
    docs: &'a [Document<Span>],
    diag: &Diagnostic,
) -> Option<(&'a Node<Span>, &'a Span, usize, &'a LineIndex)> {
    let col_match = diagnostic_code(diag) == Some("yaml11Octal");
    let parser_line = diag.range.start.line as usize + 1;
    if !col_match {
        let count: usize = docs
            .iter()
            .map(|doc| count_yaml11_octal_on_line(&doc.root, parser_line, doc.line_index()))
            .sum();
        if count != 1 {
            return None;
        }
    }
    for doc in docs {
        let idx = doc.line_index();
        if let Some((node, loc, col)) =
            find_yaml11_octal_in_node(&doc.root, parser_line, diag, col_match, idx)
        {
            return Some((node, loc, col, idx));
        }
    }
    None
}

fn count_yaml11_octal_on_line(node: &Node<Span>, parser_line: usize, idx: &LineIndex) -> usize {
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
                            && crate::scalar_helpers::is_yaml11_octal(value),
                    )
                } else {
                    count_yaml11_octal_on_line(v, parser_line, idx)
                };
                count_yaml11_octal_on_line(k, parser_line, idx) + v_count
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
                            && crate::scalar_helpers::is_yaml11_octal(value),
                    )
                } else {
                    count_yaml11_octal_on_line(item, parser_line, idx)
                }
            })
            .sum(),
        Node::Scalar { .. } | Node::Alias { .. } => 0,
    }
}

fn yaml11_octal_col_matches_diag(loc: Span, diag: &Diagnostic, idx: &LineIndex) -> bool {
    let (_, start_col) = idx.line_column(loc.start);
    let (_, end_col) = idx.line_column(loc.end);
    diag.range.start.character == start_col && diag.range.end.character == end_col
}

fn find_yaml11_octal_in_node<'a>(
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
                    && crate::scalar_helpers::is_yaml11_octal(value)
                    && (!col_match || yaml11_octal_col_matches_diag(*loc, diag, idx))
                {
                    let key_col = idx.line_column(node_loc(k).start).1 as usize;
                    return Some((v, loc, key_col));
                }
                if let Some(result) =
                    find_yaml11_octal_in_node(k, parser_line, diag, col_match, idx)
                {
                    return Some(result);
                }
                if let Some(result) =
                    find_yaml11_octal_in_node(v, parser_line, diag, col_match, idx)
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
                    && crate::scalar_helpers::is_yaml11_octal(value)
                    && (!col_match || yaml11_octal_col_matches_diag(*loc, diag, idx))
                {
                    return Some((item, loc, idx.line_column(loc.start).1 as usize));
                }
                if let Some(result) =
                    find_yaml11_octal_in_node(item, parser_line, diag, col_match, idx)
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
    use tower_lsp::lsp_types::NumberOrString;

    use rstest::rstest;

    use super::super::code_actions;
    use super::super::test_helpers::{
        apply_yaml11_octal_convert_edit, apply_yaml11_octal_quote_edit, docs_for, line_range,
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
    fn yaml11_octal_quote_action_new_text_does_not_duplicate_anchor() {
        let text = "mode: &myanchor 0755\n";
        let diag = make_diagnostic(0, 16, 20, "yaml11Octal");
        let (result, edit) = apply_yaml11_octal_quote_edit(text, diag);
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
    fn yaml11_octal_quote_action_new_text_does_not_duplicate_user_tag() {
        let text = "mode: !mytag 0755\n";
        let diag = make_diagnostic(0, 13, 17, "yaml11Octal");
        let (result, edit) = apply_yaml11_octal_quote_edit(text, diag);
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
    fn yaml11_octal_quote_action_new_text_does_not_duplicate_anchor_or_tag() {
        let text = "mode: &a !mytag 0755\n";
        let diag = make_diagnostic(0, 16, 20, "yaml11Octal");
        let (result, edit) = apply_yaml11_octal_quote_edit(text, diag);
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
    fn yaml11_octal_convert_action_new_text_does_not_duplicate_anchor() {
        let text = "mode: &myanchor 0755\n";
        let diag = make_diagnostic(0, 16, 20, "yaml11Octal");
        let (result, edit) = apply_yaml11_octal_convert_edit(text, diag);
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
    fn yaml11_octal_convert_action_new_text_does_not_duplicate_user_tag() {
        let text = "mode: !mytag 0755\n";
        let diag = make_diagnostic(0, 13, 17, "yaml11Octal");
        let (result, edit) = apply_yaml11_octal_convert_edit(text, diag);
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
    fn yaml11_octal_convert_action_new_text_does_not_duplicate_anchor_or_tag() {
        let text = "mode: &a !mytag 0755\n";
        let diag = make_diagnostic(0, 16, 20, "yaml11Octal");
        let (result, edit) = apply_yaml11_octal_convert_edit(text, diag);
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
    fn should_not_offer_yaml11_octal_quote_for_out_of_bounds_range() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 100, 104, "yaml11Octal");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        assert!(actions.iter().all(|a| a.title != "Quote as string"));
    }

    #[test]
    fn yaml11_octal_actions_attach_diagnostic() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
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
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
        {
            let attached = action.diagnostics.as_ref().unwrap();
            assert_eq!(attached.len(), 1);
            assert_eq!(
                attached[0].code,
                Some(NumberOrString::String("yaml11Octal".to_string()))
            );
        }
    }

    #[test]
    fn yaml11_octal_on_line_other_than_zero() {
        let text = "name: foo\nmode: 0755\n";
        let diag = make_diagnostic(1, 6, 10, "yaml11Octal");
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
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].range.start.line, 1, "edit must target line 1");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
        assert_eq!(edits[0].new_text, "\"0755\"");
    }

    #[test]
    fn yaml11_octal_diagnostic_not_triggered_by_other_codes() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 0, 10, "flowSeq");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );

        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    #[test]
    fn yaml11_octal_quote_action_new_text_is_scalar_only() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
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
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"0755\"");
        assert_eq!(edits[0].range.start.character, 6);
        assert_eq!(edits[0].range.end.character, 10);
    }

    #[test]
    fn yaml11_octal_convert_action_new_text_is_scalar_only() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
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
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o755");
        assert_eq!(edits[0].range.start.character, 6);
        assert_eq!(edits[0].range.end.character, 10);
    }

    #[test]
    fn yaml11_octal_quote_on_0777_produces_valid_double_quoted_yaml() {
        let text = "perms: 0777\n";
        let diag = make_diagnostic(0, 7, 11, "yaml11Octal");
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
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"0777\"");
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "perms: 0777";
        let result = format!("{}{}{}\n", &line[..start], edits[0].new_text, &line[end..]);
        let parse_result = crate::parser::parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "quoted octal must produce valid YAML; got: {:?}\nresult: {result:?}",
            parse_result.diagnostics
        );
    }

    #[test]
    fn yaml11_octal_convert_on_0755_produces_0o755() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
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
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o755");
    }

    #[test]
    fn yaml11_octal_convert_on_0777_produces_0o777() {
        let text = "perms: 0777\n";
        let diag = make_diagnostic(0, 7, 11, "yaml11Octal");
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
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o777");
    }

    #[test]
    fn yaml11_octal_rejects_08_no_actions() {
        let text = "val: 08\n";
        let diag = make_diagnostic(0, 5, 7, "yaml11Octal");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    #[test]
    fn yaml11_octal_rejects_09_no_actions() {
        let text = "val: 09\n";
        let diag = make_diagnostic(0, 5, 7, "yaml11Octal");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    #[test]
    fn yaml11_octal_trailing_comment_preserved_quote_action() {
        let text = "mode: 0755  # keep this\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
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
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"0755\"");
        assert!(
            edits[0].range.end.character <= 10,
            "range must not reach into comment"
        );
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "mode: 0755  # keep this";
        let result = format!("{}{}{}", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive in: {result:?}"
        );
    }

    #[test]
    fn yaml11_octal_trailing_comment_preserved_convert_action() {
        let text = "mode: 0755  # keep this\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
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
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o755");
        assert!(
            edits[0].range.end.character <= 10,
            "range must not reach into comment"
        );
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "mode: 0755  # keep this";
        let result = format!("{}{}{}", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive in: {result:?}"
        );
    }

    #[test]
    fn yaml11_octal_sequence_item_edit_starts_at_scalar_col() {
        let text = "modes:\n  - 0755\n";
        // `0755` starts at col 4 in line 1: "  - 0755"
        let diag = make_diagnostic(1, 4, 8, "yaml11Octal");
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
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].range.start.character > 0,
            "sequence item edit must not start at col 0"
        );
        assert_eq!(edits[0].new_text, "\"0755\"");
    }

    #[test]
    fn yaml11_octal_multi_octal_per_line_schema_code_offers_no_action() {
        let text = "{a: 0755, b: 0644}\n";
        // schemaYaml11Octal uses line-only matching; two octals on the same line must suppress both
        let diag = make_diagnostic(0, 4, 8, "schemaYaml11Octal");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    #[test]
    fn yaml11_octal_multi_octal_per_line_direct_code_resolves_by_col() {
        let text = "{a: 0755, b: 0644}\n";
        let first_diag = make_diagnostic(0, 4, 8, "yaml11Octal");
        let second_diag = make_diagnostic(0, 13, 17, "yaml11Octal");

        let first_actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[first_diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let first_action = first_actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let first_edits = &first_action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()[&test_uri()];
        assert_eq!(first_edits[0].new_text, "\"0755\"");
        assert_eq!(first_edits[0].range.start.character, 4);

        let second_actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[second_diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let second_action = second_actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let second_edits = &second_action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()[&test_uri()];
        assert_eq!(second_edits[0].new_text, "\"0644\"");
        assert_eq!(second_edits[0].range.start.character, 13);
    }

    // ════════════════════════════════════════════════════════════════════
    // Group F: schemaYaml11Octal code actions
    // ════════════════════════════════════════════════════════════════════

    // F4: both actions attach the triggering schemaYaml11Octal diagnostic
    #[test]
    fn schema_yaml11_octal_actions_attach_diagnostic() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "schemaYaml11Octal");
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
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
        {
            let diags = action.diagnostics.as_ref().unwrap();
            assert_eq!(diags.len(), 1);
            assert_eq!(
                super::super::diagnostic_code(&diags[0]),
                Some("schemaYaml11Octal"),
                "action '{}' should attach schemaYaml11Octal diagnostic",
                action.title
            );
        }
    }

    #[rstest]
    #[case::yaml11_octal_code("yaml11Octal")]
    #[case::schema_yaml11_octal_code("schemaYaml11Octal")]
    fn yaml11_octal_both_diag_codes_produce_two_actions(#[case] code: &str) {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, code);
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
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
            .count();
        assert_eq!(count, 2);
    }
}
