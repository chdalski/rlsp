// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, NumberOrString, TextEdit, WorkspaceEdit,
};

use std::collections::HashMap;

use rlsp_yaml_parser::{Document, LineIndex, Span};

use crate::editing::formatter::YamlFormatOptions;

use block_scalar::string_to_block_scalar;
use block_to_flow::block_to_flow;
use delete_anchor::delete_unused_anchor;
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
    let lines: Vec<&str> = text.lines().collect();

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
            Some("unusedAnchor") => delete_unused_anchor(docs, text, diag, uri, options)
                .into_iter()
                .collect::<Vec<_>>(),
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
    let context_actions: Vec<CodeAction> = lines.get(line_idx).map_or(vec![], |line| {
        [
            if line.contains('\t') {
                tab_to_spaces(&lines, line_idx, uri, options)
            } else {
                None
            },
            quoted_bool_to_unquoted(docs, line_idx, col, uri, options),
            string_to_block_scalar(docs, text, line_idx, uri, options),
            block_to_flow(docs, line_idx, uri, options),
        ]
        .into_iter()
        .flatten()
        .collect()
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
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helper code"
)]
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
        let edit = &edits[0];
        let start_line = edit.range.start.line as usize;
        let start_col = edit.range.start.character as usize;
        let end_line = edit.range.end.line as usize;
        let end_col = edit.range.end.character as usize;
        let source_lines: Vec<&str> = text.lines().collect();
        let mut result = String::new();
        for (i, src_line) in source_lines.iter().enumerate() {
            if i < start_line || i > end_line {
                result.push_str(src_line);
                result.push('\n');
            } else if i == start_line && i == end_line {
                result.push_str(&src_line[..start_col]);
                result.push_str(&edit.new_text);
                result.push_str(&src_line[end_col..]);
                result.push('\n');
            } else if i == start_line {
                result.push_str(&src_line[..start_col]);
                result.push_str(&edit.new_text);
                result.push('\n');
            } else if i == end_line {
                result.push_str(&src_line[end_col..]);
                result.push('\n');
            }
            // lines strictly between start and end are absorbed into the edit
        }
        result
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

    /// Apply the "Convert to block scalar" edit to `text` at the given line.
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
            .find(|a| a.title.contains("block scalar"))
            .expect("expected block-scalar action");
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
