// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{Location, Position, Range, Url};

/// A token found in the text: either an anchor (`&name`) or an alias (`*name`).
#[derive(Debug, Clone)]
struct Token {
    name: String,
    line: u32,
    start_col: u32,
    end_col: u32,
    is_anchor: bool,
}

/// Go-to-definition: cursor on `*alias` returns the location of the matching `&anchor`.
///
/// Returns `None` if the cursor is not on an alias, the anchor is not found,
/// the position is out of bounds, or the document is empty.
#[must_use]
pub fn goto_definition(text: &str, uri: &Url, position: Position) -> Option<Location> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    let line = lines.get(line_idx)?;
    if col_idx > line.len() {
        return None;
    }

    let doc_range = document_range_for_line(&lines, line_idx);
    let tokens = scan_tokens(&lines, doc_range.0, doc_range.1);

    // Find the alias token at the cursor position
    let alias = tokens.iter().find(|t| {
        !t.is_anchor
            && t.line == position.line
            && col_idx >= t.start_col as usize
            && col_idx < t.end_col as usize
    })?;

    // Find the matching anchor in the same document
    let anchor = tokens
        .iter()
        .find(|t| t.is_anchor && t.name == alias.name)?;

    #[allow(clippy::cast_possible_truncation)]
    Some(Location {
        uri: uri.clone(),
        range: Range::new(
            Position::new(anchor.line, anchor.start_col),
            Position::new(anchor.line, anchor.end_col),
        ),
    })
}

/// Find references: cursor on `&anchor` or `*alias` returns all `*alias` usage locations.
///
/// When `include_declaration` is true, the `&anchor` definition is also included.
/// Returns an empty list if the cursor is not on an anchor or alias, the position
/// is out of bounds, or the document is empty.
#[must_use]
pub fn find_references(
    text: &str,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Vec<Location> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    let Some(line) = lines.get(line_idx) else {
        return Vec::new();
    };
    if col_idx > line.len() {
        return Vec::new();
    }

    let doc_range = document_range_for_line(&lines, line_idx);
    let tokens = scan_tokens(&lines, doc_range.0, doc_range.1);

    // Find the token at the cursor position (anchor or alias)
    let cursor_token = tokens.iter().find(|t| {
        t.line == position.line && col_idx >= t.start_col as usize && col_idx < t.end_col as usize
    });

    let Some(cursor_token) = cursor_token else {
        return Vec::new();
    };

    let name = &cursor_token.name;

    // Optionally include the anchor declaration
    let declaration = if include_declaration {
        tokens
            .iter()
            .find(|t| t.is_anchor && t.name == *name)
            .map(|anchor| Location {
                uri: uri.clone(),
                range: Range::new(
                    Position::new(anchor.line, anchor.start_col),
                    Position::new(anchor.line, anchor.end_col),
                ),
            })
    } else {
        None
    };

    // Include all alias references
    let aliases = tokens
        .iter()
        .filter(|t| !t.is_anchor && t.name == *name)
        .map(|t| Location {
            uri: uri.clone(),
            range: Range::new(
                Position::new(t.line, t.start_col),
                Position::new(t.line, t.end_col),
            ),
        });

    declaration.into_iter().chain(aliases).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn test_uri() -> Url {
        Url::parse("file:///test/doc.yaml").expect("valid test URI")
    }

    // ---- Go-to-Definition: Happy Path ----

    // Test 1
    #[test]
    fn should_jump_from_alias_to_anchor_definition() {
        let text = "defaults: &defaults\n  adapter: postgres\nproduction:\n  <<: *defaults\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(3, 6));

        let loc = result.expect("should return a location");
        assert_eq!(loc.uri, uri);
        assert_eq!(loc.range.start.line, 0, "anchor should be on line 0");
    }

    // Test 2
    #[test]
    fn should_return_correct_range_for_anchor_definition() {
        let text = "defaults: &defaults\n  adapter: postgres\nproduction:\n  <<: *defaults\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(3, 6));

        let loc = result.expect("should return a location");
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(
            loc.range.start.character, 10,
            "anchor '&defaults' starts at column 10"
        );
        assert_eq!(
            loc.range.end.character, 19,
            "anchor '&defaults' ends at column 19"
        );
    }

    // Test 3
    #[test]
    fn should_handle_multiple_anchors_and_jump_to_correct_one() {
        let text = "a: &first\n  key: val\nb: &second\n  key: val\nc:\n  ref: *second\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(5, 7));

        let loc = result.expect("should return a location");
        assert_eq!(loc.range.start.line, 2, "should jump to &second on line 2");
    }

    // ---- Go-to-Definition: Edge Cases ----

    // Test 4
    #[test]
    fn should_return_none_when_cursor_not_on_alias() {
        let text = "key: value\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(0, 0));

        assert!(
            result.is_none(),
            "should return None when cursor is not on an alias"
        );
    }

    // Test 5
    #[test]
    fn should_return_none_when_cursor_on_anchor_not_alias() {
        let text = "defaults: &defaults\n  key: value\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(0, 10));

        assert!(
            result.is_none(),
            "should return None when cursor is on anchor definition, not alias"
        );
    }

    // Test 6
    #[test]
    fn should_return_none_when_alias_has_no_matching_anchor() {
        let text = "production:\n  <<: *undefined\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(1, 6));

        assert!(
            result.is_none(),
            "should return None when no matching anchor exists"
        );
    }

    // Test 7
    #[test]
    fn should_return_none_for_empty_document() {
        let text = "";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(0, 0));

        assert!(result.is_none(), "should return None for empty document");
    }

    // Test 8
    #[test]
    fn should_return_none_for_position_beyond_document_lines() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(10, 0));

        assert!(
            result.is_none(),
            "should return None for position beyond document lines"
        );
    }

    // Test 9
    #[test]
    fn should_return_none_for_position_beyond_line_length() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(0, 100));

        assert!(
            result.is_none(),
            "should return None for position beyond line length"
        );
    }

    // ---- Go-to-Definition: Multi-Document Scoping ----

    // Test 10
    #[test]
    fn should_not_jump_across_document_boundaries() {
        let text = "doc1: &shared\n  key: val\n---\ndoc2:\n  ref: *shared\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(4, 7));

        assert!(
            result.is_none(),
            "should return None when anchor is in a different document"
        );
    }

    // Test 11
    #[test]
    fn should_jump_to_anchor_within_same_document() {
        let text = "---\ndefaults: &defaults\n  key: val\nproduction:\n  <<: *defaults\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(4, 6));

        let loc = result.expect("should return a location");
        assert_eq!(
            loc.range.start.line, 1,
            "anchor should be on line 1 within the same document"
        );
    }

    // ---- Find References: Happy Path ----

    // Test 12
    #[test]
    fn should_find_all_alias_references_for_anchor() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 10), false);

        assert_eq!(result.len(), 2, "should find 2 alias references");
        let lines: Vec<u32> = result.iter().map(|l| l.range.start.line).collect();
        assert!(lines.contains(&3), "should include *shared on line 3");
        assert!(lines.contains(&5), "should include *shared on line 5");
    }

    // Test 13
    #[test]
    fn should_find_references_when_cursor_on_alias() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(3, 6), false);

        assert_eq!(result.len(), 2, "should find 2 alias references");
        let lines: Vec<u32> = result.iter().map(|l| l.range.start.line).collect();
        assert!(lines.contains(&3), "should include *shared on line 3");
        assert!(lines.contains(&5), "should include *shared on line 5");
    }

    // Test 14
    #[test]
    fn should_include_declaration_when_flag_is_true() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 10), true);

        assert_eq!(
            result.len(),
            3,
            "should find 3 locations (1 anchor + 2 aliases)"
        );
        let lines: Vec<u32> = result.iter().map(|l| l.range.start.line).collect();
        assert!(lines.contains(&0), "should include &shared on line 0");
        assert!(lines.contains(&3), "should include *shared on line 3");
        assert!(lines.contains(&5), "should include *shared on line 5");
    }

    // Test 15
    #[test]
    fn should_exclude_declaration_when_flag_is_false() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 10), false);

        assert_eq!(result.len(), 2, "should find 2 alias references only");
        let lines: Vec<u32> = result.iter().map(|l| l.range.start.line).collect();
        assert!(
            !lines.contains(&0),
            "should NOT include &shared anchor on line 0"
        );
    }

    // ---- Find References: Edge Cases ----

    // Test 16
    #[test]
    fn should_return_empty_when_cursor_not_on_anchor_or_alias() {
        let text = "key: value\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 0), false);

        assert!(
            result.is_empty(),
            "should return empty when cursor is not on an anchor or alias"
        );
    }

    // Test 17
    #[test]
    fn should_return_empty_when_anchor_has_no_alias_usages() {
        let text = "defaults: &lonely\n  key: val\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 10), false);

        assert!(
            result.is_empty(),
            "should return empty when anchor has no alias usages"
        );
    }

    // Test 18
    #[test]
    fn should_return_only_declaration_when_anchor_has_no_usages_and_include_declaration_true() {
        let text = "defaults: &lonely\n  key: val\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 10), true);

        assert_eq!(
            result.len(),
            1,
            "should return exactly 1 location (the anchor itself)"
        );
        assert_eq!(result[0].range.start.line, 0);
    }

    // Test 19
    #[test]
    fn should_return_empty_refs_for_empty_document() {
        let text = "";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 0), false);

        assert!(result.is_empty(), "should return empty for empty document");
    }

    // Test 20
    #[test]
    fn should_return_empty_refs_for_position_beyond_document_lines() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(10, 0), false);

        assert!(
            result.is_empty(),
            "should return empty for position beyond document lines"
        );
    }

    // Test 21
    #[test]
    fn should_return_empty_refs_for_position_beyond_line_length() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 100), false);

        assert!(
            result.is_empty(),
            "should return empty for position beyond line length"
        );
    }

    // ---- Find References: Multi-Document Scoping ----

    // Test 22
    #[test]
    fn should_scope_references_to_same_document() {
        let text = "doc1: &name\n  ref: *name\n---\ndoc2: &name\n  ref: *name\n";
        let uri = test_uri();
        let result = find_references(text, &uri, pos(0, 6), false);

        assert_eq!(
            result.len(),
            1,
            "should find only 1 alias reference in document 1"
        );
        assert_eq!(
            result[0].range.start.line, 1,
            "the reference should be on line 1 (document 1)"
        );
    }

    // ---- Additional Tests ----

    // Test 22a
    #[test]
    fn should_not_match_ampersand_in_non_anchor_context() {
        let text = "formula: a & b\nref: *undefined\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(0, 11));

        assert!(
            result.is_none(),
            "should return None for '&' followed by space (not a valid anchor)"
        );
    }

    // Test 22b
    #[test]
    fn should_still_find_anchors_in_unparseable_yaml() {
        let text = "defaults: &defaults\n  key: [bad\nproduction:\n  <<: *defaults\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(3, 6));

        let loc = result.expect("should find anchor even in unparseable YAML");
        assert_eq!(
            loc.range.start.line, 0,
            "anchor should be on line 0 even with syntax errors"
        );
    }

    // Test 22c
    #[test]
    fn should_not_treat_anchor_in_comment_as_definition() {
        let text = "# &fake\nreal: &real val\nref: *real\n";
        let uri = test_uri();
        let result = goto_definition(text, &uri, pos(2, 5));

        let loc = result.expect("should find &real, not &fake");
        assert_eq!(
            loc.range.start.line, 1,
            "should jump to &real on line 1, not &fake in comment on line 0"
        );
    }
}
