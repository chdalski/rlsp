use std::collections::HashMap;
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

/// A token found in the text: either an anchor (`&name`) or an alias (`*name`).
#[derive(Debug, Clone)]
struct Token {
    name: String,
    line: u32,
    start_col: u32,
    end_col: u32,
    is_anchor: bool,
}

/// Prepare rename: validates cursor is on anchor/alias, returns name range.
///
/// Returns `None` if the cursor is not on an anchor or alias, the position
/// is out of bounds, or the document is empty.
#[must_use]
pub fn prepare_rename(text: &str, position: Position) -> Option<Range> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    let line = lines.get(line_idx)?;
    if col_idx > line.len() {
        return None;
    }

    let doc_range = document_range_for_line(&lines, line_idx);
    let tokens = scan_tokens(&lines, doc_range.0, doc_range.1);

    // Find the token at the cursor position (anchor or alias)
    let token = tokens.iter().find(|t| {
        t.line == position.line && col_idx >= t.start_col as usize && col_idx < t.end_col as usize
    })?;

    Some(Range::new(
        Position::new(token.line, token.start_col),
        Position::new(token.line, token.end_col),
    ))
}

/// Rename: returns edits for all occurrences of anchor and aliases.
///
/// Returns `None` if the cursor is not on an anchor or alias, the new name
/// is invalid, the position is out of bounds, or the document is empty.
#[must_use]
pub fn rename(text: &str, uri: &Url, position: Position, new_name: &str) -> Option<WorkspaceEdit> {
    // Validate new_name first
    if !is_valid_anchor_name(new_name) {
        return None;
    }

    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    let line = lines.get(line_idx)?;
    if col_idx > line.len() {
        return None;
    }

    let doc_range = document_range_for_line(&lines, line_idx);
    let tokens = scan_tokens(&lines, doc_range.0, doc_range.1);

    // Find the token at the cursor position (anchor or alias)
    let cursor_token = tokens.iter().find(|t| {
        t.line == position.line && col_idx >= t.start_col as usize && col_idx < t.end_col as usize
    })?;

    let name = &cursor_token.name;

    // Collect all edits for this name (anchor + all aliases)
    let mut edits = Vec::new();
    for token in &tokens {
        if token.name == *name {
            let prefix = if token.is_anchor { "&" } else { "*" };
            edits.push(TextEdit {
                range: Range::new(
                    Position::new(token.line, token.start_col),
                    Position::new(token.line, token.end_col),
                ),
                new_text: format!("{prefix}{new_name}"),
            });
        }
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..WorkspaceEdit::default()
    })
}

/// Determine the document boundaries for the YAML document containing the
/// given line. Returns `(start_line, end_line)` where end is exclusive.
/// Documents are separated by `---`.
fn document_range_for_line(lines: &[&str], line_idx: usize) -> (usize, usize) {
    let mut start = 0;
    let end = lines.len();

    // Walk backwards to find the start of the current document
    for i in (0..=line_idx).rev() {
        let trimmed = lines.get(i).map_or("", |l| l.trim());
        if trimmed == "---" && i < line_idx {
            start = i + 1;
            break;
        }
    }

    // Walk forward to find the end of the current document
    for i in (line_idx + 1)..end {
        let trimmed = lines.get(i).map_or("", |l| l.trim());
        if trimmed == "---" {
            return (start, i);
        }
    }

    (start, end)
}

/// Scan lines for anchor (`&name`) and alias (`*name`) tokens within the
/// given line range. Skips comment lines.
fn scan_tokens(lines: &[&str], start_line: usize, end_line: usize) -> Vec<Token> {
    let mut tokens = Vec::new();

    for line_idx in start_line..end_line {
        let Some(line) = lines.get(line_idx) else {
            continue;
        };

        let trimmed = line.trim();

        // Skip comment lines
        if trimmed.starts_with('#') {
            continue;
        }

        #[allow(clippy::cast_possible_truncation)]
        let line_num = line_idx as u32;

        let mut chars = line.char_indices().peekable();
        while let Some((i, ch)) = chars.next() {
            if ch == '&' || ch == '*' {
                let is_anchor = ch == '&';

                // Check if followed by a valid anchor name character
                let name_start = i + 1;
                let mut name_end = name_start;

                while let Some(&(j, next_ch)) = chars.peek() {
                    if is_anchor_name_char(next_ch) {
                        name_end = j + next_ch.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }

                // Must have at least one name character
                if name_end > name_start {
                    #[allow(clippy::cast_possible_truncation)]
                    tokens.push(Token {
                        name: line[name_start..name_end].to_string(),
                        line: line_num,
                        start_col: i as u32,
                        end_col: name_end as u32,
                        is_anchor,
                    });
                }
            }
        }
    }

    tokens
}

/// Check if a character is valid in a YAML anchor/alias name.
/// Valid characters: alphanumeric, `-`, `_`, `.`
const fn is_anchor_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.'
}

/// Validate that a proposed new anchor name is valid.
fn is_valid_anchor_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_anchor_name_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn test_uri() -> Url {
        Url::parse("file:///test/doc.yaml").expect("valid test URI")
    }

    // ---- prepare_rename: Happy Path ----

    // Test 1
    #[test]
    fn should_return_range_when_cursor_on_anchor() {
        let text = "key: &myanchor value\n";
        let result = prepare_rename(text, pos(0, 6));

        let range = result.expect("should return a range");
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 5, "&myanchor starts at column 5");
        assert_eq!(range.end.character, 14, "&myanchor ends at column 14");
    }

    // Test 2
    #[test]
    fn should_return_range_when_cursor_on_alias() {
        let text = "defaults: &defaults\n  key: val\nproduction:\n  <<: *defaults\n";
        let result = prepare_rename(text, pos(3, 7));

        let range = result.expect("should return a range");
        assert_eq!(range.start.line, 3);
        assert!(range.start.character <= 7);
        assert!(range.end.character > 7);
    }

    // Test 3
    #[test]
    fn should_return_range_when_cursor_at_end_of_anchor_name() {
        let text = "key: &anchor value\n";
        let result = prepare_rename(text, pos(0, 11));

        let range = result.expect("should return a range");
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 5);
    }

    // ---- prepare_rename: Edge Cases ----

    // Test 4
    #[test]
    fn should_return_none_when_cursor_not_on_anchor_or_alias() {
        let text = "key: value\n";
        let result = prepare_rename(text, pos(0, 0));

        assert!(
            result.is_none(),
            "should return None when cursor is not on an anchor or alias"
        );
    }

    // Test 5
    #[test]
    fn should_return_none_for_empty_document() {
        let text = "";
        let result = prepare_rename(text, pos(0, 0));

        assert!(result.is_none(), "should return None for empty document");
    }

    // Test 6
    #[test]
    fn should_return_none_for_position_beyond_document_lines() {
        let text = "key: &anchor value\n";
        let result = prepare_rename(text, pos(10, 0));

        assert!(
            result.is_none(),
            "should return None for position beyond document lines"
        );
    }

    // Test 7
    #[test]
    fn should_return_none_for_position_beyond_line_length() {
        let text = "key: &anchor value\n";
        let result = prepare_rename(text, pos(0, 100));

        assert!(
            result.is_none(),
            "should return None for position beyond line length"
        );
    }

    // Test 8
    #[test]
    fn should_return_none_for_anchor_in_comment() {
        let text = "# &fake\nkey: value\n";
        let result = prepare_rename(text, pos(0, 2));

        assert!(result.is_none(), "should return None for anchor in comment");
    }

    // ---- rename: Happy Path ----

    // Test 9
    #[test]
    fn should_rename_anchor_and_single_alias() {
        let text = "defaults: &old\n  key: val\nproduction:\n  <<: *old\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 10), "new");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2, "should have 2 edits (anchor + alias)");
    }

    // Test 10
    #[test]
    fn should_rename_anchor_and_multiple_aliases() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 10), "common");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 3, "should have 3 edits (1 anchor + 2 aliases)");
    }

    // Test 11
    #[test]
    fn should_rename_when_cursor_on_alias() {
        let text = "defaults: &old\n  key: val\nproduction:\n  <<: *old\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(3, 7), "new");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2, "should have 2 edits (anchor + alias)");
    }

    // Test 12
    #[test]
    fn should_rename_anchor_with_no_aliases() {
        let text = "key: &lonely value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "orphan");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 1, "should have 1 edit (just the anchor)");
    }

    // ---- rename: Multi-Document Boundaries ----

    // Test 13
    #[test]
    fn should_not_rename_across_document_boundaries() {
        let text = "doc1: &name\n  ref: *name\n---\ndoc2: &name\n  ref: *name\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 6), "renamed");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(
            edits.len(),
            2,
            "should have only 2 edits (anchor and alias in doc1 only)"
        );
    }

    // Test 14
    #[test]
    fn should_rename_within_second_document() {
        let text = "doc1: &name\n---\ndoc2: &name\n  ref: *name\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(2, 6), "other");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(
            edits.len(),
            2,
            "should have 2 edits (only doc2's anchor and alias)"
        );
    }

    // ---- rename: Invalid Position Cases ----

    // Test 15
    #[test]
    fn rename_should_return_none_when_cursor_not_on_anchor_or_alias() {
        let text = "key: value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 0), "anything");

        assert!(
            result.is_none(),
            "should return None when cursor is not on an anchor or alias"
        );
    }

    // Test 16
    #[test]
    fn rename_should_return_none_for_empty_document() {
        let text = "";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 0), "anything");

        assert!(result.is_none(), "should return None for empty document");
    }

    // Test 17
    #[test]
    fn rename_should_return_none_for_position_beyond_document_lines() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(10, 0), "anything");

        assert!(
            result.is_none(),
            "should return None for position beyond document lines"
        );
    }

    // Test 18
    #[test]
    fn rename_should_return_none_for_position_beyond_line_length() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 100), "anything");

        assert!(
            result.is_none(),
            "should return None for position beyond line length"
        );
    }

    // ---- rename: Invalid new_name Validation (Security Cases) ----

    // Test 19
    #[test]
    fn should_reject_empty_new_name() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "");

        assert!(result.is_none(), "should return None for empty new_name");
    }

    // Test 20
    #[test]
    fn should_reject_new_name_with_spaces() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "has space");

        assert!(
            result.is_none(),
            "should return None for new_name with spaces"
        );
    }

    // Test 21
    #[test]
    fn should_reject_new_name_with_open_bracket() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "bad[name");

        assert!(result.is_none(), "should return None for new_name with [");
    }

    // Test 22
    #[test]
    fn should_reject_new_name_with_close_bracket() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "bad]name");

        assert!(result.is_none(), "should return None for new_name with ]");
    }

    // Test 23
    #[test]
    fn should_reject_new_name_with_open_brace() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "bad{name");

        assert!(result.is_none(), "should return None for new_name with {{");
    }

    // Test 24
    #[test]
    fn should_reject_new_name_with_close_brace() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "bad}name");

        assert!(result.is_none(), "should return None for new_name with }}");
    }

    // Test 25
    #[test]
    fn should_reject_new_name_with_colon() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "bad:name");

        assert!(result.is_none(), "should return None for new_name with :");
    }

    // Test 26
    #[test]
    fn should_reject_new_name_with_comma() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "bad,name");

        assert!(result.is_none(), "should return None for new_name with ,");
    }

    // Test 27
    #[test]
    fn should_accept_new_name_with_hyphen() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "valid-name");

        assert!(result.is_some(), "should accept new_name with hyphen");
    }

    // Test 28
    #[test]
    fn should_accept_new_name_with_underscore() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "valid_name");

        assert!(result.is_some(), "should accept new_name with underscore");
    }

    // Test 29
    #[test]
    fn should_accept_new_name_with_dot() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "valid.name");

        assert!(result.is_some(), "should accept new_name with dot");
    }

    // ---- rename: Edit Content Verification ----

    // Test 30
    #[test]
    fn should_produce_correct_edit_ranges() {
        let text = "key: &old value\nref: *old\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "new");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2);

        // Check first edit (anchor)
        assert_eq!(edits[0].range.start.line, 0);
        assert_eq!(edits[0].range.start.character, 5);
        assert_eq!(edits[0].range.end.line, 0);
        assert_eq!(edits[0].range.end.character, 9);
        assert_eq!(edits[0].new_text, "&new");

        // Check second edit (alias)
        assert_eq!(edits[1].range.start.line, 1);
        assert_eq!(edits[1].range.start.character, 5);
        assert_eq!(edits[1].range.end.line, 1);
        assert_eq!(edits[1].range.end.character, 9);
        assert_eq!(edits[1].new_text, "*new");
    }

    // ---- Additional Invalid new_name Tests (Security) ----

    // Test 36
    #[test]
    fn should_reject_new_name_with_whitespace_only() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "   ");

        assert!(
            result.is_none(),
            "should return None for whitespace-only new_name"
        );
    }

    // Test 37
    #[test]
    fn should_reject_new_name_with_hash() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "name#comment");

        assert!(result.is_none(), "should return None for new_name with #");
    }

    // Test 38
    #[test]
    fn should_reject_new_name_with_newline() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "name\n");

        assert!(
            result.is_none(),
            "should return None for new_name with newline"
        );
    }

    // Test 39
    #[test]
    fn should_reject_new_name_with_tab() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "name\t");

        assert!(result.is_none(), "should return None for new_name with tab");
    }

    // Test 40
    #[test]
    fn should_reject_new_name_with_carriage_return() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "name\r");

        assert!(
            result.is_none(),
            "should return None for new_name with carriage return"
        );
    }

    // Test 41
    #[test]
    fn should_reject_new_name_with_ampersand() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "name&other");

        assert!(result.is_none(), "should return None for new_name with &");
    }

    // Test 42
    #[test]
    fn should_reject_new_name_with_asterisk() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "name*other");

        assert!(result.is_none(), "should return None for new_name with *");
    }

    // Test 43
    #[test]
    fn should_reject_new_name_with_exclamation() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "name!tag");

        assert!(result.is_none(), "should return None for new_name with !");
    }

    // Test 44
    #[test]
    fn should_accept_new_name_starting_with_digit() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), "123abc");

        assert!(
            result.is_some(),
            "should accept new_name starting with digit"
        );
    }

    // Test 45
    #[test]
    fn should_handle_very_long_new_name_without_panic() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let long_name = "a".repeat(10000);
        let result = rename(text, &uri, pos(0, 5), &long_name);

        // Should either accept or reject, but not panic
        assert!(
            result.is_some() || result.is_none(),
            "should not panic with very long new_name"
        );
    }

    // ---- Additional Position Edge Cases ----

    // Test 46
    #[test]
    fn should_return_none_for_cursor_at_exact_end_of_line() {
        let text = "key: &anchor\n";
        let result = prepare_rename(text, pos(0, 12));

        assert!(
            result.is_none(),
            "should return None for cursor one past last char"
        );
    }

    // Test 47
    #[test]
    fn should_handle_cursor_at_document_end() {
        let text = "key: &anchor";
        let result = prepare_rename(text, pos(0, 12));

        assert!(
            result.is_none(),
            "should return None for cursor at/past end"
        );
    }
}
