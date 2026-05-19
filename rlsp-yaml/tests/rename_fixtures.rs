// SPDX-License-Identifier: MIT
//
// Fixture-driven rename tests.
//
// Each file in `tests/fixtures/rename/*.md` is a self-contained test case.
// The file format is:
//
//   ---
//   test-name: descriptive-kebab-case-name
//   category: rename
//   cursor: <line>:<character>   (zero-based)
//   new-name: <replacement-name>
//   applies-rename: true         # OR omits-rename: true
//   ---
//
//   # Test: Title
//
//   ## Test-Document
//
//   ```yaml
//   input here
//   ```
//
//   ## Expected-Document   (omit when omits-rename is used)
//
//   ```yaml
//   expected output here
//   ```
//
// Two assertion modes (mutually exclusive):
//   applies-rename — calls rename(...), applies all TextEdits from the WorkspaceEdit
//                    to Test-Document in reverse range-start order (highest position first
//                    so earlier edits don't shift later edit ranges), asserts result equals
//                    Expected-Document
//   omits-rename   — asserts rename(...) returned None;
//                    Expected-Document is not required

#![expect(missing_docs, reason = "test code")]

mod common;
use common::*;

use std::path::{Path, PathBuf};

use rlsp_yaml::navigation::rename::rename;
use rstest::rstest;
use tower_lsp::lsp_types::{Position, TextEdit};

// ---- Data model -------------------------------------------------------------

/// Assertion mode parsed from frontmatter.
#[derive(Debug, PartialEq)]
enum RenameMode {
    /// Assert rename succeeds and all edits produce Expected-Document.
    AppliesRename,
    /// Assert rename returns None.
    OmitsRename,
}

/// The parsed contents of a rename fixture file.
#[derive(Debug)]
struct RenameFixtureSpec {
    /// `test-name` from frontmatter (informational).
    test_name: String,
    /// Cursor position (zero-based line and character).
    cursor_line: u32,
    cursor_char: u32,
    /// Replacement anchor/alias name.
    new_name: String,
    /// Assertion mode.
    mode: RenameMode,
    /// Raw YAML from `## Test-Document`.
    test_document: String,
    /// Raw YAML from `## Expected-Document`. Empty for `OmitsRename` mode.
    expected_document: String,
}

// ---- Parsing ----------------------------------------------------------------

fn split_frontmatter(content: &str) -> Result<(&str, &str), String> {
    let content = content.strip_prefix("---\n").ok_or_else(|| {
        "fixture file must start with '---\\n' (frontmatter opening delimiter)".to_string()
    })?;
    let close = content.find("\n---\n").ok_or_else(|| {
        "fixture file frontmatter is not closed (no closing '---' delimiter found)".to_string()
    })?;
    let frontmatter = &content[..close];
    let body = &content[close + 5..];
    Ok((frontmatter, body))
}

fn parse_frontmatter(frontmatter: &str) -> Result<(String, u32, u32, String, RenameMode), String> {
    let mut test_name = String::new();
    let mut cursor: Option<(u32, u32)> = None;
    let mut new_name: Option<String> = None;
    let mut applies_rename = false;
    let mut omits_rename = false;

    for line in frontmatter.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(": ") {
            let trimmed_value = value.trim();
            match key.trim() {
                "test-name" => test_name = trimmed_value.to_string(),
                // new-name is intentionally not trimmed: whitespace-only names are valid test inputs.
                "new-name" => new_name = Some(value.to_string()),
                "cursor" => {
                    let (line_s, col_s) = value.split_once(':').ok_or_else(|| {
                        format!("cursor field must be 'line:character', got: {value:?}")
                    })?;
                    let line_n = line_s.trim().parse::<u32>().map_err(|_| {
                        format!("cursor line must be a non-negative integer, got: {line_s:?}")
                    })?;
                    let col_n = col_s.trim().parse::<u32>().map_err(|_| {
                        format!("cursor character must be a non-negative integer, got: {col_s:?}")
                    })?;
                    cursor = Some((line_n, col_n));
                }
                "applies-rename" => applies_rename = trimmed_value == "true",
                "omits-rename" => omits_rename = trimmed_value == "true",
                _ => {}
            }
        }
    }

    if applies_rename && omits_rename {
        return Err(
            "applies-rename and omits-rename are mutually exclusive; only one may be set"
                .to_string(),
        );
    }

    let (cursor_line, cursor_char) = cursor.ok_or_else(|| {
        "missing required frontmatter field: cursor (e.g. cursor: 0:0)".to_string()
    })?;

    let mode = if applies_rename {
        RenameMode::AppliesRename
    } else if omits_rename {
        RenameMode::OmitsRename
    } else {
        return Err(
            "missing required frontmatter field: one of applies-rename or omits-rename must be set"
                .to_string(),
        );
    };

    let resolved_new_name =
        new_name.ok_or_else(|| "missing required frontmatter field: new-name".to_string())?;

    Ok((test_name, cursor_line, cursor_char, resolved_new_name, mode))
}

fn extract_section(body: &str, section: &str) -> Result<String, String> {
    let heading = format!("## {section}");
    let section_start = body
        .find(&heading)
        .ok_or_else(|| format!("missing '## {section}' section"))?;
    let after_heading = &body[section_start + heading.len()..];
    let fence_open = after_heading
        .find("```")
        .ok_or_else(|| format!("missing opening '```' fence in '## {section}' section"))?;
    let after_fence = &after_heading[fence_open + 3..];
    let content_start = after_fence
        .find('\n')
        .ok_or_else(|| format!("no newline after opening fence in '## {section}'"))?;
    let content = &after_fence[content_start + 1..];
    let fence_close = content
        .find("```")
        .ok_or_else(|| format!("missing closing '```' fence in '## {section}' section"))?;
    Ok(content[..fence_close].to_string())
}

fn parse_fixture(content: &str, path: &Path) -> RenameFixtureSpec {
    let path_str = path.display().to_string();

    let (frontmatter, body) =
        split_frontmatter(content).unwrap_or_else(|e| panic_fixture(&path_str, &e));

    let (test_name, cursor_line, cursor_char, new_name, mode) =
        parse_frontmatter(frontmatter).unwrap_or_else(|e| panic_fixture(&path_str, &e));

    let test_document =
        extract_section(body, "Test-Document").unwrap_or_else(|e| panic_fixture(&path_str, &e));

    let expected_document = match &mode {
        RenameMode::OmitsRename => String::new(),
        RenameMode::AppliesRename => extract_section(body, "Expected-Document")
            .unwrap_or_else(|e| panic_fixture(&path_str, &e)),
    };

    RenameFixtureSpec {
        test_name,
        cursor_line,
        cursor_char,
        new_name,
        mode,
        test_document,
        expected_document,
    }
}

#[expect(
    clippy::panic,
    reason = "test harness reports fixture errors via panic"
)]
fn panic_fixture(path_str: &str, msg: &str) -> ! {
    panic!("fixture {path_str}: {msg}")
}

// ---- Harness helpers --------------------------------------------------------

/// Apply a list of `TextEdit`s to `source` in reverse range-start order.
///
/// Reverse order (highest position first) ensures each edit's byte offsets remain
/// valid after previous edits are applied.
fn apply_edits_reverse(source: &str, edits: &[TextEdit]) -> String {
    let mut sorted = edits.to_vec();
    sorted.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then(b.range.start.character.cmp(&a.range.start.character))
    });
    let mut result = source.to_string();
    for edit in &sorted {
        result = apply_text_edit(&result, edit);
    }
    result
}

// ---- rstest harness ---------------------------------------------------------

#[rstest]
fn rename_fixture(#[files("tests/fixtures/rename/rename-*.md")] path: PathBuf) {
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic_fixture(&path.display().to_string(), &e.to_string()));

    let fixture = parse_fixture(&content, &path);
    let uri = test_uri();
    let docs = docs_for(&fixture.test_document);
    let position = Position::new(fixture.cursor_line, fixture.cursor_char);
    let result = rename(&docs, &uri, position, &fixture.new_name);

    match fixture.mode {
        RenameMode::AppliesRename => {
            let workspace_edit = result.unwrap_or_else(|| {
                panic_fixture(
                    &path.display().to_string(),
                    &format!(
                        "rename returned None for cursor {}:{} new-name {:?}; expected Some(WorkspaceEdit)",
                        fixture.cursor_line, fixture.cursor_char, fixture.new_name
                    ),
                )
            });
            let changes = workspace_edit.changes.unwrap_or_else(|| {
                panic_fixture(
                    &path.display().to_string(),
                    "WorkspaceEdit has no changes map",
                )
            });
            let edits = changes.get(&uri).unwrap_or_else(|| {
                panic_fixture(
                    &path.display().to_string(),
                    "no edits for test URI in WorkspaceEdit changes",
                )
            });
            let actual = apply_edits_reverse(&fixture.test_document, edits);

            let actual_norm = actual.strip_suffix('\n').unwrap_or(&actual);
            let expected_norm = fixture
                .expected_document
                .strip_suffix('\n')
                .unwrap_or(&fixture.expected_document);

            assert_eq!(
                actual_norm,
                expected_norm,
                "fixture {}: output mismatch\ntest-name: {}\ninput:    {:?}\nexpected: {:?}\ngot:      {:?}",
                path.display(),
                fixture.test_name,
                fixture.test_document,
                fixture.expected_document,
                actual,
            );
        }
        RenameMode::OmitsRename => {
            assert!(
                result.is_none(),
                "fixture {}: expected rename to return None for cursor {}:{} new-name {:?}, but got Some(WorkspaceEdit)",
                path.display(),
                fixture.cursor_line,
                fixture.cursor_char,
                fixture.new_name,
            );
        }
    }
}

// ---- Self-tests for harness helpers -----------------------------------------

#[cfg(test)]
mod self_tests {
    use tower_lsp::lsp_types::{Position, Range};

    use super::*;

    fn make_edit(
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
        text: &str,
    ) -> TextEdit {
        TextEdit {
            range: Range::new(
                Position::new(start_line, start_char),
                Position::new(end_line, end_char),
            ),
            new_text: text.to_string(),
        }
    }

    // ---- Group A: Frontmatter parsing ----------------------------------------

    // A1. test-name is parsed from frontmatter.
    #[test]
    fn frontmatter_parses_test_name() {
        let fm = "test-name: my-test\ncursor: 0:0\nnew-name: foo\napplies-rename: true\n";
        let (name, _, _, _, _) = parse_frontmatter(fm).unwrap();
        assert_eq!(name, "my-test");
    }

    // A2. cursor field parses to correct line and character.
    #[test]
    fn frontmatter_cursor_parses_line_and_char() {
        let fm = "test-name: foo\ncursor: 3:7\nnew-name: x\napplies-rename: true\n";
        let (_, line, ch, _, _) = parse_frontmatter(fm).unwrap();
        assert_eq!(line, 3);
        assert_eq!(ch, 7);
    }

    // A3. applies-rename is parsed.
    #[test]
    fn frontmatter_parses_applies_rename() {
        let fm = "test-name: foo\ncursor: 0:0\nnew-name: x\napplies-rename: true\n";
        let (_, _, _, _, mode) = parse_frontmatter(fm).unwrap();
        assert_eq!(mode, RenameMode::AppliesRename);
    }

    // A4. omits-rename is parsed.
    #[test]
    fn frontmatter_parses_omits_rename() {
        let fm = "test-name: foo\ncursor: 0:0\nnew-name: x\nomits-rename: true\n";
        let (_, _, _, _, mode) = parse_frontmatter(fm).unwrap();
        assert_eq!(mode, RenameMode::OmitsRename);
    }

    // A5. Both applies-rename and omits-rename present returns error.
    #[test]
    fn frontmatter_both_modes_returns_error() {
        let fm =
            "test-name: foo\ncursor: 0:0\nnew-name: x\napplies-rename: true\nomits-rename: true\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            err.contains("mutually exclusive"),
            "error should mention 'mutually exclusive': {err}"
        );
    }

    // A6. Missing cursor field returns error that mentions 'cursor'.
    #[test]
    fn frontmatter_missing_cursor_returns_error() {
        let fm = "test-name: foo\nnew-name: x\napplies-rename: true\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            err.contains("cursor"),
            "error should mention 'cursor': {err}"
        );
    }

    // A7. Neither applies-rename nor omits-rename returns error.
    #[test]
    fn frontmatter_missing_mode_returns_error() {
        let fm = "test-name: foo\ncursor: 0:0\nnew-name: x\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            err.contains("applies-rename") || err.contains("omits-rename"),
            "error should mention the missing mode fields: {err}"
        );
    }

    // A8. Missing new-name returns error.
    #[test]
    fn frontmatter_missing_new_name_returns_error() {
        let fm = "test-name: foo\ncursor: 0:0\napplies-rename: true\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            err.contains("new-name"),
            "error should mention 'new-name': {err}"
        );
    }

    // A9. cursor field with missing character component returns error.
    #[test]
    fn frontmatter_cursor_missing_colon_returns_error() {
        let fm = "test-name: foo\ncursor: 3\nnew-name: x\napplies-rename: true\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            !err.is_empty(),
            "malformed cursor should return error: {err}"
        );
    }

    // A10. cursor field with non-numeric values returns error.
    #[test]
    fn frontmatter_cursor_non_numeric_returns_error() {
        let fm = "test-name: foo\ncursor: abc:def\nnew-name: x\napplies-rename: true\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            !err.is_empty(),
            "non-numeric cursor should return error: {err}"
        );
    }

    // ---- Group B: Multi-edit application order --------------------------------

    // B1. Single edit applied correctly.
    #[test]
    fn apply_edits_reverse_single_edit() {
        let source = "key: &old value\n";
        let edits = vec![make_edit(0, 5, 0, 9, "&new")];
        let result = apply_edits_reverse(source, &edits);
        assert_eq!(result, "key: &new value\n");
    }

    // B2. Two edits on different lines applied in reverse line order.
    #[test]
    fn apply_edits_reverse_two_edits_different_lines() {
        let source = "key: &old value\nref: *old\n";
        // anchor on line 0, alias on line 1
        let edits = vec![make_edit(0, 5, 0, 9, "&new"), make_edit(1, 5, 1, 9, "*new")];
        let result = apply_edits_reverse(source, &edits);
        assert_eq!(result, "key: &new value\nref: *new\n");
    }

    // B3. Two edits on same line applied in reverse character order.
    #[test]
    fn apply_edits_reverse_two_edits_same_line_reverse_char_order() {
        // contrived: two replacements on one line, later char first
        // "a: &x value &x extra\n"
        //  0123456789012345678901
        // First &x: cols [3,5), second &x: cols [12,14)
        let source = "a: &x value &x extra\n";
        let edits = vec![make_edit(0, 3, 0, 5, "&y"), make_edit(0, 12, 0, 14, "&y")];
        let result = apply_edits_reverse(source, &edits);
        assert_eq!(result, "a: &y value &y extra\n");
    }

    // B4. apply_edits_reverse sorts descending before applying.
    #[test]
    fn apply_edits_reverse_sorts_before_applying() {
        // If applied in wrong order (line 0 first), the line-1 byte offset shifts.
        // Correct reverse order (line 1 first) keeps offsets stable.
        let source = "anchor: &old\nalias: *old\n";
        // Give edits in forward order; harness must sort them.
        let edits = vec![
            make_edit(0, 8, 0, 12, "&new"), // anchor
            make_edit(1, 7, 1, 11, "*new"), // alias
        ];
        let result = apply_edits_reverse(source, &edits);
        assert_eq!(result, "anchor: &new\nalias: *new\n");
    }

    // ---- Group C: omits-rename round-trip ------------------------------------

    // C1. omits-rename passes when cursor is not on anchor.
    // cursor_range is imported from common (used by code_action harnesses); call it here
    // so it is not dead in this binary.
    #[test]
    fn omits_rename_passes_when_no_anchor_at_cursor() {
        let text = "key: value\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let range = cursor_range(0, 0);
        let result = rename(&docs, &uri, range.start, "anything");
        assert!(result.is_none(), "expected None for cursor not on anchor");
    }

    // C2. omits-rename fails (panics) when anchor IS renamed.
    #[test]
    #[should_panic(expected = "expected rename to return None")]
    fn omits_rename_fails_when_rename_succeeds() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let position = Position::new(0, 5);
        let result = rename(&docs, &uri, position, "new");

        // Simulate the harness OmitsRename assertion — should panic because rename succeeds.
        assert!(
            result.is_none(),
            "fixture test.md: expected rename to return None for cursor 0:5 new-name \"new\", but got Some(WorkspaceEdit)",
        );
    }

    // ---- Group D: applies-rename round-trip ----------------------------------

    // D1. applies-rename: single anchor + single alias.
    #[test]
    fn applies_rename_single_alias() {
        let text = "defaults: &old\n  key: val\nproduction:\n  <<: *old\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let position = Position::new(0, 10);
        let result = rename(&docs, &uri, position, "new");

        let workspace_edit = result.expect("should return WorkspaceEdit");
        let changes = workspace_edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        let actual = apply_edits_reverse(text, edits);
        let expected = "defaults: &new\n  key: val\nproduction:\n  <<: *new\n";
        assert_eq!(actual, expected);
    }

    // D2. applies-rename: anchor + multiple aliases produces correct output.
    #[test]
    fn applies_rename_multiple_aliases() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let position = Position::new(0, 10);
        let result = rename(&docs, &uri, position, "common");

        let workspace_edit = result.expect("should return WorkspaceEdit");
        let changes = workspace_edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        let actual = apply_edits_reverse(text, edits);
        let expected = "defaults: &common\n  key: val\ndev:\n  <<: *common\nprod:\n  <<: *common\n";
        assert_eq!(actual, expected);
    }
}
