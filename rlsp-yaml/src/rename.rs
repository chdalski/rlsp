// SPDX-License-Identifier: MIT

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
    let edits: Vec<TextEdit> = tokens
        .iter()
        .filter(|t| t.name == *name)
        .map(|t| {
            let prefix = if t.is_anchor { "&" } else { "*" };
            TextEdit {
                range: Range::new(
                    Position::new(t.line, t.start_col),
                    Position::new(t.line, t.end_col),
                ),
                new_text: format!("{prefix}{new_name}"),
            }
        })
        .collect();

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
    !name.is_empty() && name.len() <= 256 && name.chars().all(is_anchor_name_char)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use rstest::rstest;

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

    #[rstest]
    #[case::not_on_anchor_or_alias("key: value\n", 0, 0)]
    #[case::empty_document("", 0, 0)]
    #[case::beyond_document_lines("key: &anchor value\n", 10, 0)]
    #[case::beyond_line_length("key: &anchor value\n", 0, 100)]
    #[case::anchor_in_comment("# &fake\nkey: value\n", 0, 2)]
    // ---- Additional Position Edge Cases ----
    #[case::cursor_at_exact_end_of_line("key: &anchor\n", 0, 12)]
    #[case::cursor_at_document_end("key: &anchor", 0, 12)]
    fn prepare_rename_returns_none(#[case] text: &str, #[case] line: u32, #[case] character: u32) {
        let result = prepare_rename(text, pos(line, character));
        assert!(result.is_none());
    }

    // ---- rename: Happy Path ----

    #[rstest]
    #[case::anchor_and_single_alias(
        "defaults: &old\n  key: val\nproduction:\n  <<: *old\n",
        0,
        10,
        "new",
        2
    )]
    #[case::anchor_and_multiple_aliases(
        "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n",
        0,
        10,
        "common",
        3
    )]
    #[case::cursor_on_alias(
        "defaults: &old\n  key: val\nproduction:\n  <<: *old\n",
        3,
        7,
        "new",
        2
    )]
    #[case::anchor_with_no_aliases("key: &lonely value\n", 0, 5, "orphan", 1)]
    // ---- rename: Multi-Document Boundaries ----
    #[case::not_across_document_boundaries(
        "doc1: &name\n  ref: *name\n---\ndoc2: &name\n  ref: *name\n",
        0,
        6,
        "renamed",
        2
    )]
    #[case::within_second_document(
        "doc1: &name\n---\ndoc2: &name\n  ref: *name\n",
        2,
        6,
        "other",
        2
    )]
    fn rename_returns_edits_len(
        #[case] text: &str,
        #[case] line: u32,
        #[case] character: u32,
        #[case] new_name: &str,
        #[case] expected_len: usize,
    ) {
        let uri = test_uri();
        let result = rename(text, &uri, pos(line, character), new_name);
        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), expected_len);
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

    // ---- rename: Invalid Position Cases ----

    #[rstest]
    #[case::cursor_not_on_anchor_or_alias("key: value\n", 0, 0, "anything")]
    #[case::empty_document("", 0, 0, "anything")]
    #[case::beyond_document_lines("key: &anchor value\n", 10, 0, "anything")]
    #[case::beyond_line_length("key: &anchor value\n", 0, 100, "anything")]
    fn rename_returns_none_invalid_position(
        #[case] text: &str,
        #[case] line: u32,
        #[case] character: u32,
        #[case] new_name: &str,
    ) {
        let uri = test_uri();
        let result = rename(text, &uri, pos(line, character), new_name);
        assert!(result.is_none());
    }

    // ---- rename: Invalid new_name Validation (Security Cases) ----

    #[rstest]
    #[case::empty_name("")]
    #[case::spaces("has space")]
    #[case::open_bracket("bad[name")]
    #[case::close_bracket("bad]name")]
    #[case::open_brace("bad{name")]
    #[case::close_brace("bad}name")]
    #[case::colon("bad:name")]
    #[case::comma("bad,name")]
    // ---- Additional Invalid new_name Tests (Security) ----
    #[case::whitespace_only("   ")]
    #[case::hash("name#comment")]
    #[case::newline("name\n")]
    #[case::tab("name\t")]
    #[case::carriage_return("name\r")]
    #[case::ampersand("name&other")]
    #[case::asterisk("name*other")]
    #[case::exclamation("name!tag")]
    fn rename_rejects_invalid_new_name(#[case] new_name: &str) {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), new_name);
        assert!(result.is_none());
    }

    // ---- rename: Valid new_name Validation ----

    #[rstest]
    #[case::hyphen("valid-name")]
    #[case::underscore("valid_name")]
    #[case::dot("valid.name")]
    #[case::starts_with_digit("123abc")]
    fn rename_accepts_valid_new_name(#[case] new_name: &str) {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = rename(text, &uri, pos(0, 5), new_name);
        assert!(result.is_some());
    }

    #[test]
    fn should_reject_new_name_exceeding_max_length() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let long_name = "a".repeat(257);
        let result = rename(text, &uri, pos(0, 5), &long_name);
        assert!(
            result.is_none(),
            "name longer than 256 chars must be rejected"
        );
    }

    #[test]
    fn should_accept_new_name_at_exactly_max_length() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let max_name = "a".repeat(256);
        let result = rename(text, &uri, pos(0, 5), &max_name);
        assert!(
            result.is_some(),
            "name of exactly 256 chars must be accepted"
        );
    }
}
