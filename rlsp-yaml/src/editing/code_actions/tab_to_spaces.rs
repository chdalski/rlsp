// SPDX-License-Identifier: MIT

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

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::super::code_actions;
    use super::super::test_helpers::{cursor_range, docs_for};
    use crate::test_utils::test_uri;

    #[test]
    fn should_convert_tabs_to_spaces() {
        let text = "\tkey: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("tabs to spaces"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "  key: value");
        assert!(!edits[0].new_text.contains('\t'));
    }

    #[test]
    fn should_not_offer_tab_conversion_without_tabs() {
        let text = "  key: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("tabs")));
    }
}
