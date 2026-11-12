// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, NumberOrString, TextEdit, WorkspaceEdit,
};

use std::cell::OnceCell;
use std::collections::HashMap;

use rlsp_yaml_parser::{Document, LineIndex, Span};

use crate::editing::formatter::YamlFormatOptions;
use crate::lsp_util::source_lines;

use block_scalar::string_to_block_scalar;
use block_to_flow::block_to_flow;
use delete_anchor::{AnchorIndex, delete_unused_anchor};
use flow_to_block::{flow_map_to_block, flow_seq_to_block};
use quoted_bool::quoted_bool_to_unquoted;
use tab_to_spaces::tab_to_spaces;
use yaml11_bool::{schema_yaml11_bool_type_actions, yaml11_bool_actions};
use yaml11_octal::yaml11_octal_actions;

mod block_scalar;
mod block_to_flow;
mod delete_anchor;
mod flow_to_block;
mod quoted_bool;
mod tab_to_spaces;
mod yaml11_bool;
mod yaml11_octal;

/// Compute code actions available for the given text, range, and diagnostics.
///
/// Returns actions for:
/// - Converting flow mappings to block style (when cursor is on a `flowMap` diagnostic)
/// - Converting flow sequences to block style (when cursor is on a `flowSeq` diagnostic)
/// - Converting block mappings to flow style (when cursor is on a block mapping key)
/// - Replacing tabs with spaces (when the line contains tabs)
/// - Deleting unused anchors (when cursor is on an `unusedAnchor` diagnostic)
/// - Converting quoted booleans to unquoted (`"true"` -> `true`)
/// - Converting long strings to block scalars (`|` style)
#[must_use]
pub fn code_actions(
    docs: &[Document<Span>],
    text: &str,
    range: tower_lsp::lsp_types::Range,
    diagnostics: &[Diagnostic],
    uri: &tower_lsp::lsp_types::Url,
    options: &YamlFormatOptions,
) -> Vec<CodeAction> {
    // The anchors are indexed for the first unused-anchor diagnostic and read by
    // all the others.
    let anchor_index = OnceCell::new();

    // Diagnostic-driven actions
    let diag_actions = diagnostics
        .iter()
        .filter(|diag| ranges_overlap(&diag.range, &range))
        .flat_map(|diag| match diagnostic_code(diag) {
            Some("flowMap") => flow_map_to_block(docs, text, diag, uri, options)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("flowSeq") => flow_seq_to_block(docs, text, diag, uri, options)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("unusedAnchor") => {
                let index = anchor_index.get_or_init(|| AnchorIndex::new(docs));
                delete_unused_anchor(index, text, diag, uri)
                    .into_iter()
                    .collect::<Vec<_>>()
            }
            Some("yaml11Boolean" | "schemaYaml11Boolean") => {
                yaml11_bool_actions(docs, diag, uri, options)
            }
            Some("yaml11Octal" | "schemaYaml11Octal") => {
                yaml11_octal_actions(docs, diag, uri, options)
            }
            Some("schemaYaml11BooleanType") => {
                schema_yaml11_bool_type_actions(docs, diag, uri, options)
            }
            _ => vec![],
        });

    // Context-driven actions (not tied to diagnostics)
    let line_idx = range.start.line as usize;
    let col = range.start.character as usize;
    let context_actions: Vec<CodeAction> =
        source_lines(text).nth(line_idx).map_or(vec![], |line| {
            let mut actions: Vec<CodeAction> = [
                tab_to_spaces(line, line_idx, uri, options),
                quoted_bool_to_unquoted(docs, line_idx, col, uri, options),
                block_to_flow(docs, line_idx, uri, options),
            ]
            .into_iter()
            .flatten()
            .collect();
            actions.extend(string_to_block_scalar(docs, text, line_idx, uri, options));
            actions
        });

    diag_actions.chain(context_actions).collect()
}

pub(super) const fn diagnostic_code(diag: &Diagnostic) -> Option<&str> {
    match &diag.code {
        Some(NumberOrString::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

const fn ranges_overlap(a: &tower_lsp::lsp_types::Range, b: &tower_lsp::lsp_types::Range) -> bool {
    a.start.line <= b.end.line && b.start.line <= a.end.line
}
pub(super) fn span_matches_diag(loc: Span, diag: &Diagnostic, idx: &LineIndex) -> bool {
    let start_line = idx.line_column(loc.start).0.saturating_sub(1);
    let start_col = idx.line_column(loc.start).1;
    let end_line = idx.line_column(loc.end).0.saturating_sub(1);
    let end_col = idx.line_column(loc.end).1 + 1;

    diag.range.start.line == start_line
        && diag.range.start.character == start_col
        && diag.range.end.line == end_line
        && diag.range.end.character == end_col
}

pub(super) fn make_action(
    title: String,
    uri: &tower_lsp::lsp_types::Url,
    edits: Vec<TextEdit>,
    kind: CodeActionKind,
    diagnostics: Option<Vec<Diagnostic>>,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    CodeAction {
        title,
        kind: Some(kind),
        diagnostics,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        }),
        ..CodeAction::default()
    }
}

#[cfg(test)]
mod test_helpers {
    use tower_lsp::lsp_types::{CodeAction, Diagnostic, NumberOrString, Position, Range, TextEdit};

    use rlsp_yaml_parser::Span;
    use rlsp_yaml_parser::node::Document;

    use crate::editing::formatter::YamlFormatOptions;
    use crate::test_utils::{parse_docs, test_uri};
    use crate::validation::ValidationSettings;
    use crate::validation::validators::validate_flow_style;

    use super::code_actions;

    pub(super) fn cursor_range(line: u32, col: u32) -> Range {
        Range::new(Position::new(line, col), Position::new(line, col))
    }

    pub(super) fn line_range(line: u32) -> Range {
        Range::new(Position::new(line, 0), Position::new(line, 999))
    }

    pub(super) fn make_flow_diag(
        code: &str,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> Diagnostic {
        Diagnostic {
            range: Range::new(
                Position::new(start_line, start_char),
                Position::new(end_line, end_char),
            ),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some("rlsp-yaml".to_string()),
            ..Diagnostic::default()
        }
    }

    /// The byte offset of a position of `buffer`: the column counts code points,
    /// as the columns of the features do, and lines end at `\n`, `\r\n` or a
    /// lone `\r`.
    fn byte_offset(buffer: &str, position: Position) -> usize {
        let mut line_start = 0;
        let mut line = 0;
        let bytes = buffer.as_bytes();
        let mut i = 0;
        while line < position.line {
            match bytes.get(i) {
                Some(b'\r') if bytes.get(i + 1) == Some(&b'\n') => i += 2,
                Some(b'\r' | b'\n') => i += 1,
                Some(_) => {
                    i += 1;
                    continue;
                }
                None => break,
            }
            line += 1;
            line_start = i;
        }
        buffer[line_start..]
            .char_indices()
            .nth(position.character as usize)
            .map_or(buffer.len(), |(offset, _)| line_start + offset)
    }

    /// `buffer` with `edit` applied.
    pub(super) fn apply_text_edit(buffer: &str, edit: &TextEdit) -> String {
        let (start, end) = (
            byte_offset(buffer, edit.range.start),
            byte_offset(buffer, edit.range.end),
        );
        format!("{}{}{}", &buffer[..start], edit.new_text, &buffer[end..])
    }

    pub(super) fn make_diagnostic(line: u32, start: u32, end: u32, code: &str) -> Diagnostic {
        make_flow_diag(code, line, start, line, end)
    }

    pub(super) fn docs_for(text: &str) -> Vec<Document<Span>> {
        parse_docs(text)
    }

    pub(super) fn flow_diags_for(text: &str) -> Vec<Diagnostic> {
        let docs = docs_for(text);
        validate_flow_style(&docs, &ValidationSettings::default())
    }

    pub(super) fn flow_map_action(text: &str) -> Option<CodeAction> {
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let diag = diags
            .iter()
            .find(|d| d.code == Some(NumberOrString::String("flowMap".to_string())))?;
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        code_actions(
            &docs,
            text,
            whole,
            std::slice::from_ref(diag),
            &test_uri(),
            &YamlFormatOptions::default(),
        )
        .into_iter()
        .find(|a| a.title.contains("flow mapping"))
    }

    pub(super) fn flow_seq_action(text: &str) -> Option<CodeAction> {
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let diag = diags
            .iter()
            .find(|d| d.code == Some(NumberOrString::String("flowSeq".to_string())))?;
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        code_actions(
            &docs,
            text,
            whole,
            std::slice::from_ref(diag),
            &test_uri(),
            &YamlFormatOptions::default(),
        )
        .into_iter()
        .find(|a| a.title.contains("flow sequence"))
    }

    pub(super) fn new_text_for(action: &CodeAction) -> String {
        action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()
            .get(&test_uri())
            .unwrap()[0]
            .new_text
            .clone()
    }

    /// Apply the first block-to-flow edit to `text` and return the resulting string.
    pub(super) fn apply_block_to_flow_edit(text: &str, line: u32) -> String {
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(line, 0),
            &[],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .expect("expected block-to-flow action");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        apply_text_edit(text, &edits[0])
    }

    /// Apply the "Convert quoted string" (`quoted_bool`) edit to `yaml` at the given cursor column.
    /// Returns the full edited text and the raw `TextEdit` (for range assertions).
    pub(super) fn apply_quoted_bool_edit(yaml: &str, col: u32) -> (String, TextEdit) {
        let actions = code_actions(
            &docs_for(yaml),
            yaml,
            cursor_range(0, col),
            &[],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .expect("expected quoted-bool action");
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

    fn apply_first_action_edit(yaml: &str, diag: Diagnostic, title: &str) -> (String, TextEdit) {
        let actions = code_actions(
            &docs_for(yaml),
            yaml,
            line_range(0),
            &[diag],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == title)
            .expect("expected action with matching title");
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

    pub(super) fn apply_yaml11_bool_quote_edit(yaml: &str, diag: Diagnostic) -> (String, TextEdit) {
        apply_first_action_edit(yaml, diag, "Quote value")
    }

    pub(super) fn apply_yaml11_bool_convert_edit(
        yaml: &str,
        diag: Diagnostic,
    ) -> (String, TextEdit) {
        apply_first_action_edit(yaml, diag, "Convert to boolean")
    }

    pub(super) fn apply_yaml11_octal_quote_edit(
        yaml: &str,
        diag: Diagnostic,
    ) -> (String, TextEdit) {
        apply_first_action_edit(yaml, diag, "Quote as string")
    }

    pub(super) fn apply_yaml11_octal_convert_edit(
        yaml: &str,
        diag: Diagnostic,
    ) -> (String, TextEdit) {
        apply_first_action_edit(yaml, diag, "Convert to YAML 1.2 octal")
    }

    /// Apply the "Convert to block scalar (literal)" edit to `text` at the given line.
    /// Returns the full edited text and the raw `TextEdit` (for range assertions).
    pub(super) fn apply_block_scalar_edit(text: &str, line: u32) -> (String, TextEdit) {
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(line, 0),
            &[],
            &test_uri(),
            &YamlFormatOptions::default(),
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("literal"))
            .expect("expected block-scalar literal action");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let edit = edits[0].clone();
        let source_lines: Vec<&str> = text.lines().collect();
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
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{CodeAction, Position, Range, TextEdit};

    use crate::editing::formatter::YamlFormatOptions;
    use crate::parser::parse_yaml;
    use crate::test_utils::test_uri;

    use super::code_actions;
    use super::test_helpers::{apply_text_edit, cursor_range, docs_for, flow_diags_for};

    /// What a document offers: the diagnostics are the flow-style ones of the
    /// document itself, as the server computes them, when `with_flow_diags`.
    fn actions_of(text: &str, range: Range, with_flow_diags: bool) -> Vec<CodeAction> {
        let diagnostics = if with_flow_diags {
            flow_diags_for(text)
        } else {
            vec![]
        };
        code_actions(
            &docs_for(text),
            text,
            range,
            &diagnostics,
            &test_uri(),
            &YamlFormatOptions::default(),
        )
    }

    /// From the start of `line` to far beyond the document: the diagnostics of
    /// the lines from `line` on apply.
    fn from_line(line: u32) -> Range {
        Range::new(Position::new(line, 0), Position::new(999, 0))
    }

    fn edit_of(action: &CodeAction) -> TextEdit {
        let changes = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .expect("the action carries edits");
        changes[&test_uri()][0].clone()
    }

    fn titles(actions: &[CodeAction]) -> Vec<&str> {
        actions.iter().map(|a| a.title.as_str()).collect()
    }

    /// The edits write `\n` whatever the break of the document is, so compare
    /// documents by their lines.
    fn normalised(text: &str) -> String {
        text.replace("\r\n", "\n").replace('\r', "\n")
    }

    /// Offer and apply the action titled `title` for `template` and for
    /// `template` with every `\n` replaced by `line_break`, and assert that the
    /// two agree. Returns the applied text of the broken document.
    fn assert_parity(
        template: &str,
        line_break: &str,
        range: Range,
        with_flow_diags: bool,
        expected_titles: &[&str],
        applied_title: &str,
    ) -> String {
        let broken = template.replace('\n', line_break);
        let lf_actions = actions_of(template, range, with_flow_diags);
        let broken_actions = actions_of(&broken, range, with_flow_diags);

        assert_eq!(titles(&lf_actions), expected_titles, "the \\n document");
        assert_eq!(broken_actions, lf_actions, "offered with {line_break:?}");

        let apply = |text: &str, actions: &[CodeAction]| {
            let action = actions
                .iter()
                .find(|a| a.title == applied_title)
                .expect("the action to apply is offered");
            apply_text_edit(text, &edit_of(action))
        };
        let applied = apply(&broken, &broken_actions);
        assert_eq!(
            normalised(&applied),
            apply(template, &lf_actions),
            "applied with {line_break:?}"
        );
        applied
    }

    const TABS: &str = "Convert tabs to spaces";
    const BLOCK_TO_FLOW: &str = "Convert block to flow style";
    const FLOW_MAP: &str = "Convert flow mapping to block style";
    const FLOW_SEQ: &str = "Convert flow sequence to block style";

    #[rstest]
    fn should_offer_tabs_to_spaces_on_a_later_line_when_lines_end_with_a_break(
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let applied = assert_parity(
            "a: 1\nb:\t2\nc: 3\n",
            line_break,
            cursor_range(1, 0),
            false,
            &[TABS],
            TABS,
        );

        assert_eq!(
            applied,
            format!("a: 1{line_break}b:  2{line_break}c: 3{line_break}")
        );
    }

    #[rstest]
    fn should_change_only_the_cursor_line_when_tabs_are_on_two_lines(
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let applied = assert_parity(
            "a:\t1\nb:\t2\n",
            line_break,
            cursor_range(0, 0),
            false,
            &[TABS],
            TABS,
        );

        assert_eq!(applied, format!("a:  1{line_break}b:\t2{line_break}"));
    }

    #[rstest]
    fn should_offer_nothing_when_the_tab_is_on_another_line(
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let broken = "a:\t1\nb: 2\nc: 3\n".replace('\n', line_break);

        assert!(actions_of(&broken, cursor_range(2, 0), false).is_empty());
    }

    #[rstest]
    fn should_count_an_empty_line_when_the_tab_follows_it(
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let applied = assert_parity(
            "a: 1\n\nb:\t2\n",
            line_break,
            cursor_range(2, 0),
            false,
            &[TABS],
            TABS,
        );

        assert_eq!(
            applied,
            format!("a: 1{line_break}{line_break}b:  2{line_break}")
        );
    }

    #[test]
    fn should_count_each_break_once_when_breaks_are_mixed() {
        let text = "a: 1\r\nb:\t2\rc: 3\n";

        let actions = actions_of(text, cursor_range(1, 0), false);

        assert_eq!(titles(&actions), [TABS]);
        let edit = edit_of(&actions[0]);
        assert_eq!(
            edit.range,
            Range::new(Position::new(1, 0), Position::new(1, 4))
        );
        assert_eq!(edit.new_text, "b:  2");
    }

    #[rstest]
    fn should_offer_the_quoted_bool_on_a_later_line(#[values("\r\n", "\r")] line_break: &str) {
        let applied = assert_parity(
            "a: 1\nb: \"true\"\n",
            line_break,
            cursor_range(1, 3),
            false,
            &["Convert quoted string to true"],
            "Convert quoted string to true",
        );

        assert_eq!(applied, format!("a: 1{line_break}b: true{line_break}"));
    }

    #[rstest]
    fn should_convert_block_to_flow_on_a_later_line(#[values("\r\n", "\r")] line_break: &str) {
        let applied = assert_parity(
            "x: 1\nouter:\n  a: 1\n  b: 2\ny: 3\n",
            line_break,
            cursor_range(1, 0),
            false,
            &[BLOCK_TO_FLOW],
            BLOCK_TO_FLOW,
        );

        assert!(
            applied.starts_with(&format!("x: 1{line_break}")),
            "{applied:?}"
        );
        assert!(
            applied.ends_with(&format!("y: 3{line_break}")),
            "{applied:?}"
        );
        let reparsed = parse_yaml(&applied);
        assert!(reparsed.diagnostics.is_empty(), "{applied:?}");
    }

    #[rstest]
    fn should_not_convert_block_to_flow_on_a_child_line(#[values("\r\n", "\r")] line_break: &str) {
        let broken = "x: 1\nouter:\n  a: 1\n  b: 2\ny: 3\n".replace('\n', line_break);

        let actions = actions_of(&broken, cursor_range(2, 0), false);

        assert!(titles(&actions).iter().all(|t| *t != BLOCK_TO_FLOW));
    }

    #[rstest]
    fn should_offer_block_scalar_conversion_on_a_later_line(
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let template = format!("x: 1\nkey: \"{}\"\n", "a".repeat(50));

        assert_parity(
            &template,
            line_break,
            cursor_range(1, 0),
            false,
            &[
                "Convert to block scalar (literal)",
                "Convert to block scalar (folded)",
            ],
            "Convert to block scalar (literal)",
        );
    }

    #[rstest]
    #[case::after_the_last_break("a:\t1\n", 1, &[])]
    #[case::without_a_final_break("a:\t1", 0, &[TABS])]
    #[case::empty_text("", 0, &[])]
    fn should_see_no_line_after_the_last_break(
        #[case] template: &str,
        #[case] line: u32,
        #[case] expected: &[&str],
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let broken = template.replace('\n', line_break);

        let actions = actions_of(&broken, cursor_range(line, 0), false);

        assert_eq!(titles(&actions), expected);
    }

    #[rstest]
    fn should_offer_nothing_when_the_line_is_beyond_the_document(
        #[values("\r\n", "\r")] line_break: &str,
        #[values(3, u32::MAX)] line: u32,
    ) {
        let broken = "a:\t1\nb: 2\n".replace('\n', line_break);

        assert!(actions_of(&broken, cursor_range(line, 0), false).is_empty());
    }

    #[rstest]
    #[case::mapping_value_map("outer:\n  inner: {a: 1, b: 2}\n", 1, FLOW_MAP)]
    #[case::mapping_value_seq("outer:\n  items: [x, y]\n", 1, FLOW_SEQ)]
    #[case::deeper_indent("a:\n    deep: {x: 1}\n", 1, FLOW_MAP)]
    #[case::spanning_two_lines("top:\n  k: {a: 1,\n    b: 2}\n", 1, FLOW_MAP)]
    #[case::sequence_item("list:\n  - {a: 1}\n", 1, FLOW_MAP)]
    #[case::first_line("k: {a: 1}\nz: 2\n", 0, FLOW_MAP)]
    fn should_convert_a_flow_collection_when_lines_end_with_a_break(
        #[case] template: &str,
        #[case] line: u32,
        #[case] title: &str,
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let applied = assert_parity(template, line_break, from_line(line), true, &[title], title);

        let reparsed = parse_yaml(&applied);
        assert!(reparsed.diagnostics.is_empty(), "{applied:?}");
    }

    #[rstest]
    fn should_open_the_block_form_on_a_new_line_when_the_flow_value_follows_a_key(
        #[values("\r\n", "\r")] line_break: &str,
    ) {
        let broken = "outer:\n  inner: {a: 1, b: 2}\n".replace('\n', line_break);

        let actions = actions_of(&broken, from_line(1), true);

        assert_eq!(edit_of(&actions[0]).new_text, "\n    a: 1\n    b: 2");
    }

    #[rstest]
    #[case::cjk_key("日本語: [a, b]\n", 0, FLOW_SEQ)]
    #[case::accented_key("é: {a: 1}\n", 0, FLOW_MAP)]
    #[case::emoji_key("😀: [a]\n", 0, FLOW_SEQ)]
    #[case::on_a_later_line("x: 1\n日本語: {a: 1}\n", 1, FLOW_MAP)]
    fn should_convert_a_flow_collection_after_multibyte_text(
        #[case] template: &str,
        #[case] line: u32,
        #[case] title: &str,
        #[values("\n", "\r")] line_break: &str,
    ) {
        let applied = assert_parity(template, line_break, from_line(line), true, &[title], title);

        let reparsed = parse_yaml(&applied);
        assert!(reparsed.diagnostics.is_empty(), "{applied:?}");
    }
}
