// SPDX-License-Identifier: MIT

//! Tab-to-spaces code action — operates on raw text, not the AST.
//!
//! Tabs are a pre-parse lexical concern (YAML 1.2 §6.1 forbids them for
//! indentation); the parser normalises or rejects them, so they are not
//! represented in the AST. This action is whitespace-cleanup that runs
//! before any structural editing applies — same carve-out category as
//! modeline handling and BOM stripping. Not an AST-retrofit candidate.

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit};

use crate::editing::formatter::YamlFormatOptions;

use super::make_action;

pub(super) fn tab_to_spaces(
    line: &str,
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
    _options: &YamlFormatOptions,
) -> Option<CodeAction> {
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
        Position::new(line_idx as u32, line.chars().count() as u32),
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

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::Position;

    use crate::editing::formatter::YamlFormatOptions;
    use crate::test_utils::test_uri;

    use super::tab_to_spaces;

    // The edit ends at the end of the line, a column in code points: not in
    // bytes, and not past the line.
    #[test]
    fn the_edit_ends_at_the_end_of_the_line_in_code_points() {
        let action = tab_to_spaces(
            "\u{1F600}\u{e9}:\tv",
            3,
            &test_uri(),
            &YamlFormatOptions::default(),
        )
        .expect("the line holds a tab");
        let edits = &action.edit.unwrap().changes.unwrap()[&test_uri()];
        assert_eq!(edits[0].range.start, Position::new(3, 0));
        assert_eq!(edits[0].range.end, Position::new(3, 5));
        assert_eq!(edits[0].new_text, "\u{1F600}\u{e9}:  v");
    }
}
