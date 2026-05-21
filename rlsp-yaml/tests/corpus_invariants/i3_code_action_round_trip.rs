use std::path::Path;

use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rlsp_yaml::parser::parse_yaml;
use tower_lsp::lsp_types::{Position, Range, TextEdit};

use super::i2_range_validity::utf16_len;
use super::shared::{
    collect_all_diagnostics, collect_error_diagnostics, error_key, error_key_set, fmt_range,
};

pub fn check_i3_code_action_round_trip(path: &Path, text: &str) -> Result<(), String> {
    let parse_result = parse_yaml(text);
    let docs = parse_result.documents;
    let all_diagnostics = collect_all_diagnostics(&docs);

    // Build pre-edit error set: only DiagnosticSeverity::Error entries.
    let pre_edit_errors = error_key_set(&collect_error_diagnostics(text));

    let lines: Vec<&str> = text.lines().collect();
    let last_line = lines.len().saturating_sub(1) as u32;
    let last_char = lines.last().map_or(0, |l| utf16_len(l) as u32);
    let whole_file = Range::new(Position::new(0, 0), Position::new(last_line, last_char));

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let uri = tower_lsp::lsp_types::Url::parse(&format!("file:///corpus/{file_name}"))
        .expect("valid URI");

    let actions = code_actions(
        &docs,
        text,
        whole_file,
        &all_diagnostics,
        &uri,
        &YamlFormatOptions::default(),
    );

    for action in &actions {
        let Some(edit) = &action.edit else {
            continue;
        };
        let Some(changes) = &edit.changes else {
            continue;
        };
        let Some(text_edits) = changes.get(&uri) else {
            continue;
        };
        if text_edits.is_empty() {
            continue;
        }

        let edited = apply_text_edits(text, text_edits);
        let post_edit_diagnostics = collect_error_diagnostics(&edited);
        let post_edit_errors = error_key_set(&post_edit_diagnostics);
        let new_error_keys: Vec<_> = post_edit_errors.difference(&pre_edit_errors).collect();

        if !new_error_keys.is_empty() {
            // Find the triggering diagnostic for the action (first associated diag, if any)
            let (diag_code, diag_range) = action
                .diagnostics
                .as_ref()
                .and_then(|v| v.first())
                .map_or_else(
                    || ("<no-code>".to_string(), "unknown".to_string()),
                    |d| {
                        let code = d
                            .code
                            .as_ref()
                            .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
                        let range = fmt_range(d.range);
                        (code, range)
                    },
                );

            // Find the full diagnostic for the first new error key.
            let new_key = new_error_keys[0];
            let new_diag = post_edit_diagnostics
                .iter()
                .find(|d| &error_key(d) == new_key)
                .expect("key came from this collection");
            let new_code = new_diag
                .code
                .as_ref()
                .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
            let new_range = fmt_range(new_diag.range);
            return Err(format!(
                r#"action "{}": edit for diagnostic {} at {} introduced new error [{}] "{}" at {}"#,
                action.title, diag_code, diag_range, new_code, new_diag.message, new_range
            ));
        }
    }

    Ok(())
}

/// Apply a list of `TextEdit`s to `text`, working in reverse start-position order so
/// that applying one edit does not shift byte offsets for earlier (lower-position) edits.
///
/// # Panics / undefined behaviour
/// Overlapping edits are the caller's responsibility (LSP spec §3.16.2 forbids them).
/// This function does not detect or guard against overlapping ranges.
pub fn apply_text_edits(text: &str, edits: &[TextEdit]) -> String {
    // Sort edits by start position descending.
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then_with(|| b.range.start.character.cmp(&a.range.start.character))
    });

    let mut result = text.to_string();
    for edit in sorted {
        let start_byte = lsp_pos_to_byte_offset(&result, edit.range.start);
        let end_byte = lsp_pos_to_byte_offset(&result, edit.range.end);
        result.replace_range(start_byte..end_byte, &edit.new_text);
    }
    result
}

/// Convert an LSP `Position` (UTF-16 column) to a UTF-8 byte offset in `text`.
pub fn lsp_pos_to_byte_offset(text: &str, pos: Position) -> usize {
    let mut line_start = 0;
    for (i, line) in text.split('\n').enumerate() {
        if i == pos.line as usize {
            // Walk UTF-16 units to find the byte offset within the line.
            let mut utf16_col = 0u32;
            for (byte_pos, ch) in line.char_indices() {
                if utf16_col == pos.character {
                    return line_start + byte_pos;
                }
                utf16_col += ch.len_utf16() as u32;
            }
            // Column is at or past end of line (e.g., pointing to the newline).
            return line_start + line.len();
        }
        line_start += line.len() + 1; // +1 for '\n'
    }
    // Position past end of text.
    text.len()
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::{Position, Range, TextEdit};

    use super::*;

    // UT-1: single edit replacing a range
    #[test]
    fn i3_at1_single_edit_replaces_range() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 6), Position::new(0, 11)),
            new_text: "Rust".to_string(),
        }];
        assert_eq!(apply_text_edits("hello world", &edits), "hello Rust");
    }

    // UT-2: two non-overlapping edits given in forward order; function must re-sort to reverse
    #[test]
    fn i3_at2_two_non_overlapping_edits_applied_in_reverse_order() {
        let edits = vec![
            TextEdit {
                range: Range::new(Position::new(0, 0), Position::new(0, 3)),
                new_text: "X".to_string(),
            },
            TextEdit {
                range: Range::new(Position::new(0, 8), Position::new(0, 11)),
                new_text: "Z".to_string(),
            },
        ];
        assert_eq!(apply_text_edits("abc def ghi", &edits), "X def Z");
    }

    // UT-3: edit at start of text
    #[test]
    fn i3_at3_edit_at_start_of_text() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 3)),
            new_text: "NEW".to_string(),
        }];
        assert_eq!(apply_text_edits("old value", &edits), "NEW value");
    }

    // UT-4: edit at end of text
    #[test]
    fn i3_at4_edit_at_end_of_text() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(0, 8)),
            new_text: "new".to_string(),
        }];
        assert_eq!(apply_text_edits("key: val", &edits), "key: new");
    }

    // UT-5: edit spanning multiple lines
    #[test]
    fn i3_at5_edit_spanning_multiple_lines() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(1, 5)),
            new_text: " MIDDLE ".to_string(),
        }];
        assert_eq!(
            apply_text_edits("line0\nline1\nline2", &edits),
            "line0 MIDDLE \nline2"
        );
    }

    // UT-6: empty new_text deletes range
    #[test]
    fn i3_at6_empty_new_text_deletes_range() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(0, 6)),
            new_text: String::new(),
        }];
        assert_eq!(apply_text_edits("hello  world", &edits), "hello world");
    }

    // UT-7: zero-width range inserts text
    #[test]
    fn i3_at7_zero_width_range_inserts_text() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 3), Position::new(0, 3)),
            new_text: "l".to_string(),
        }];
        assert_eq!(apply_text_edits("helo", &edits), "hello");
    }

    // UT-8: empty edits slice returns text unchanged
    #[test]
    fn i3_at8_empty_edits_returns_text_unchanged() {
        assert_eq!(apply_text_edits("unchanged", &[]), "unchanged");
    }

    // UT-9: edit after multi-byte char uses UTF-16 columns
    #[test]
    fn i3_at9_edit_after_multibyte_char_uses_utf16_columns() {
        // "a😀b" — emoji is 2 UTF-16 code units; 'b' is at UTF-16 col 3
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 3), Position::new(0, 4)),
            new_text: "X".to_string(),
        }];
        assert_eq!(apply_text_edits("a\u{1F600}b", &edits), "a\u{1F600}X");
    }
}
