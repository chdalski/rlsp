// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Document, ScalarStyle, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

use super::{block_to_flow::node_loc, diagnostic_code, make_action};

pub(super) fn yaml11_octal_actions(
    docs: &[Document<Span>],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let Some((scalar, loc, base_indent)) = find_yaml11_octal_scalar(docs, diag) else {
        return vec![];
    };
    let Node::Scalar { value, .. } = scalar else {
        return vec![];
    };

    let mut quoted = scalar.clone();
    if let Node::Scalar { style, .. } = &mut quoted {
        *style = ScalarStyle::DoubleQuoted;
    }
    let mut converted = scalar.clone();
    if let Node::Scalar {
        style, value: v, ..
    } = &mut converted
    {
        *style = ScalarStyle::Plain;
        *v = format!("0o{}", &value[1..]);
    }

    let quote_opts = YamlFormatOptions {
        preserve_quotes: true,
        ..YamlFormatOptions::default()
    };
    let quoted_text = format_subtree(&quoted, &quote_opts, base_indent);
    let converted_text = format_subtree(&converted, &YamlFormatOptions::default(), base_indent);

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
) -> Option<(&'a Node<Span>, &'a Span, usize)> {
    let col_match = diagnostic_code(diag) == Some("yaml11Octal");
    let parser_line = diag.range.start.line as usize + 1;
    if !col_match {
        let count: usize = docs
            .iter()
            .map(|doc| count_yaml11_octal_on_line(&doc.root, parser_line))
            .sum();
        if count != 1 {
            return None;
        }
    }
    for doc in docs {
        if let Some(result) = find_yaml11_octal_in_node(&doc.root, parser_line, diag, col_match) {
            return Some(result);
        }
    }
    None
}

fn count_yaml11_octal_on_line(node: &Node<Span>, parser_line: usize) -> usize {
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
                        loc.start.line == parser_line
                            && crate::scalar_helpers::is_yaml11_octal(value),
                    )
                } else {
                    count_yaml11_octal_on_line(v, parser_line)
                };
                count_yaml11_octal_on_line(k, parser_line) + v_count
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
                        loc.start.line == parser_line
                            && crate::scalar_helpers::is_yaml11_octal(value),
                    )
                } else {
                    count_yaml11_octal_on_line(item, parser_line)
                }
            })
            .sum(),
        Node::Scalar { .. } | Node::Alias { .. } => 0,
    }
}

const fn yaml11_octal_col_matches_diag(loc: &Span, diag: &Diagnostic) -> bool {
    diag.range.start.character as usize == loc.start.column
        && diag.range.end.character as usize == loc.end.column
}

fn find_yaml11_octal_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    diag: &Diagnostic,
    col_match: bool,
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
                {
                    if loc.start.line == parser_line
                        && crate::scalar_helpers::is_yaml11_octal(value)
                        && (!col_match || yaml11_octal_col_matches_diag(loc, diag))
                    {
                        let key_col = node_loc(k).start.column;
                        return Some((v, loc, key_col));
                    }
                }
                if let Some(result) = find_yaml11_octal_in_node(k, parser_line, diag, col_match) {
                    return Some(result);
                }
                if let Some(result) = find_yaml11_octal_in_node(v, parser_line, diag, col_match) {
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
                {
                    if loc.start.line == parser_line
                        && crate::scalar_helpers::is_yaml11_octal(value)
                        && (!col_match || yaml11_octal_col_matches_diag(loc, diag))
                    {
                        return Some((item, loc, loc.start.column));
                    }
                }
                if let Some(result) = find_yaml11_octal_in_node(item, parser_line, diag, col_match)
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
#[expect(clippy::indexing_slicing, clippy::unwrap_used, reason = "test code")]
mod tests {
    use tower_lsp::lsp_types::NumberOrString;

    use rstest::rstest;

    use super::super::code_actions;
    use super::super::test_helpers::{docs_for, line_range, make_diagnostic};
    use crate::test_utils::test_uri;

    #[test]
    fn should_not_offer_yaml11_octal_quote_for_out_of_bounds_range() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 100, 104, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());

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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
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
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let count = actions
            .iter()
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
            .count();
        assert_eq!(count, 2);
    }
}
