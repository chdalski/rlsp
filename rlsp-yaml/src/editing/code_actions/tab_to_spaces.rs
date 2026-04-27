// SPDX-License-Identifier: MIT

//! Tab-to-spaces code action — operates on raw text, not the AST.
//!
//! Tabs are a pre-parse lexical concern (YAML 1.2 §6.1 forbids them for
//! indentation); the parser normalises or rejects them, so they are not
//! represented in the AST. This action is whitespace-cleanup that runs
//! before any structural editing applies — same carve-out category as
//! modeline handling and BOM stripping. Not an AST-retrofit candidate.

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use super::make_action;

pub(super) fn tab_to_spaces(
    lines: &[&str],
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let line = lines.get(line_idx)?;
    if !line.contains('\t') {
        return None;
    }

    let new_text = line.replace('\t', "  ");

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(line_idx as u32, 0),
        Position::new(line_idx as u32, line.len() as u32),
    );

    Some(make_action(
        "Convert tabs to spaces".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::QUICKFIX,
        None,
    ))
}
