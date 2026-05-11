// SPDX-License-Identifier: MIT
//
// Fixture-driven code-action tests.
//
// Each file in `tests/fixtures/code_actions/*.md` is a self-contained test case.
// The file format is:
//
//   ---
//   test-name: descriptive-kebab-case-name
//   category: tab-to-spaces | ...
//   cursor: <line>:<character>   (zero-based)
//   applies-action: <title-substring>
//   format-options:
//     print_width: 120
//     single_quote: true
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
//   ## Expected-Document   (omit when omits-action is used)
//
//   ```yaml
//   expected output here
//   ```
//
// Two assertion modes (mutually exclusive):
//   applies-action — finds the action by title substring, applies its first
//                    TextEdit to Test-Document, asserts result equals Expected-Document
//   omits-action   — asserts no action with that title substring is returned;
//                    Expected-Document is not required
//
// Optional frontmatter block:
//   format-options — indented key-value pairs that override YamlFormatOptions
//                    fields; unspecified fields remain at their default value;
//                    unknown keys are silently ignored

#![expect(missing_docs, reason = "test code")]

mod common;
use common::*;

use std::path::{Path, PathBuf};

use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rstest::rstest;
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};

// ---- Data model -------------------------------------------------------------

/// Assertion mode parsed from frontmatter.
#[derive(Debug, PartialEq)]
enum ActionMode {
    /// Assert the named action is offered and its first edit produces Expected-Document.
    AppliesAction(String),
    /// Assert no action with this title substring is offered.
    OmitsAction(String),
}

/// The parsed contents of a code-action fixture file.
#[derive(Debug)]
struct FixtureSpec {
    /// `test-name` from frontmatter (informational).
    test_name: String,
    /// Cursor position (zero-based line and character).
    cursor_line: u32,
    cursor_char: u32,
    /// Assertion mode.
    mode: ActionMode,
    /// Formatter options from `format-options:` frontmatter block, or default.
    format_options: YamlFormatOptions,
    /// Raw YAML from `## Test-Document`.
    test_document: String,
    /// Raw YAML from `## Expected-Document`. Empty for `OmitsAction` mode.
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

fn parse_frontmatter(
    frontmatter: &str,
) -> Result<(String, u32, u32, ActionMode, YamlFormatOptions), String> {
    let mut test_name = String::new();
    let mut cursor: Option<(u32, u32)> = None;
    let mut applies_action: Option<String> = None;
    let mut omits_action: Option<String> = None;
    let mut options = YamlFormatOptions::default();
    let mut in_format_options = false;

    for line in frontmatter.lines() {
        if line.is_empty() {
            continue;
        }

        // Detect the `format-options:` section header.
        if line == "format-options:" {
            in_format_options = true;
            continue;
        }

        // A non-indented, non-empty line ends the format-options block.
        if in_format_options && !line.starts_with(' ') && !line.starts_with('\t') {
            in_format_options = false;
        }

        if in_format_options {
            let trimmed = line.trim();
            if let Some((key, value)) = trimmed.split_once(": ") {
                apply_format_option(&mut options, key.trim(), value.trim());
            }
        } else if let Some((key, value)) = line.split_once(": ") {
            let value = value.trim();
            match key.trim() {
                "test-name" => test_name = value.to_string(),
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
                "applies-action" => applies_action = Some(value.to_string()),
                "omits-action" => omits_action = Some(value.to_string()),
                _ => {}
            }
        }
    }

    if applies_action.is_some() && omits_action.is_some() {
        return Err(
            "applies-action and omits-action are mutually exclusive; only one may be set"
                .to_string(),
        );
    }

    let (cursor_line, cursor_char) = cursor.ok_or_else(|| {
        "missing required frontmatter field: cursor (e.g. cursor: 0:0)".to_string()
    })?;

    let mode = if let Some(title) = applies_action {
        ActionMode::AppliesAction(title)
    } else if let Some(title) = omits_action {
        ActionMode::OmitsAction(title)
    } else {
        return Err(
            "missing required frontmatter field: one of applies-action or omits-action must be set"
                .to_string(),
        );
    };

    Ok((test_name, cursor_line, cursor_char, mode, options))
}

/// Apply a single `format-options` key-value pair to `options`.
///
/// Panics on invalid numeric values (mirrors `formatter_fixtures.rs` behavior).
/// Unknown keys are silently ignored — forward-compatible with future options.
#[expect(
    clippy::panic,
    reason = "test harness reports fixture errors via panic"
)]
fn apply_format_option(options: &mut YamlFormatOptions, key: &str, value: &str) {
    match key {
        "print_width" => {
            options.print_width = value.parse().unwrap_or_else(|_| {
                panic!("fixture format-options: invalid print_width: {value:?}")
            });
        }
        "tab_width" => {
            options.tab_width = value
                .parse()
                .unwrap_or_else(|_| panic!("fixture format-options: invalid tab_width: {value:?}"));
        }
        "single_quote" => {
            options.single_quote = value == "true";
        }
        "preserve_quotes" => {
            options.preserve_quotes = value == "true";
        }
        "bracket_spacing" => {
            options.bracket_spacing = value == "true";
        }
        "format_enforce_block_style" => {
            options.format_enforce_block_style = value == "true";
        }
        _ => {}
    }
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

fn parse_fixture(content: &str, path: &Path) -> FixtureSpec {
    let path_str = path.display().to_string();

    let (frontmatter, body) =
        split_frontmatter(content).unwrap_or_else(|e| panic_fixture(&path_str, &e));

    let (test_name, cursor_line, cursor_char, mode, format_options) =
        parse_frontmatter(frontmatter).unwrap_or_else(|e| panic_fixture(&path_str, &e));

    let test_document =
        extract_section(body, "Test-Document").unwrap_or_else(|e| panic_fixture(&path_str, &e));

    let expected_document = match &mode {
        ActionMode::OmitsAction(_) => String::new(),
        ActionMode::AppliesAction(_) => extract_section(body, "Expected-Document")
            .unwrap_or_else(|e| panic_fixture(&path_str, &e)),
    };

    FixtureSpec {
        test_name,
        cursor_line,
        cursor_char,
        mode,
        format_options,
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

/// Extract the first `TextEdit` from a `CodeAction`'s workspace edit.
///
/// Panics with a clear message if the action has no edit, no changes map, or
/// no edits for the test URI. This covers the normal `applies-action` path where
/// fixture files are version-controlled and the action is always well-formed.
fn extract_first_edit(
    action: &tower_lsp::lsp_types::CodeAction,
    uri: &Url,
    path_str: &str,
) -> TextEdit {
    let edit = action
        .edit
        .as_ref()
        .unwrap_or_else(|| panic_fixture(path_str, "action has no WorkspaceEdit"));
    let changes = edit
        .changes
        .as_ref()
        .unwrap_or_else(|| panic_fixture(path_str, "action WorkspaceEdit has no changes map"));
    let edits = changes
        .get(uri)
        .unwrap_or_else(|| panic_fixture(path_str, "no edits for test URI in action changes"));
    edits.first().cloned().unwrap_or_else(|| {
        panic_fixture(path_str, "action has an empty TextEdit list for test URI")
    })
}

// ---- rstest harness ---------------------------------------------------------

#[rstest]
fn code_action_fixture(#[files("tests/fixtures/code_actions/*.md")] path: PathBuf) {
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic_fixture(&path.display().to_string(), &e.to_string()));

    let fixture = parse_fixture(&content, &path);
    let uri = test_uri();
    let docs = docs_for(&fixture.test_document);
    let range = cursor_range(fixture.cursor_line, fixture.cursor_char);
    let actions = code_actions(
        &docs,
        &fixture.test_document,
        range,
        &[],
        &uri,
        &fixture.format_options,
    );

    match &fixture.mode {
        ActionMode::AppliesAction(title_sub) => {
            let action = actions
                .iter()
                .find(|a| a.title.contains(title_sub.as_str()))
                .unwrap_or_else(|| {
                    panic_fixture(
                        &path.display().to_string(),
                        &format!(
                            "no action with title containing {title_sub:?}; available: {:?}",
                            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
                        ),
                    )
                });

            let edit = extract_first_edit(action, &uri, &path.display().to_string());
            let actual = apply_text_edit(&fixture.test_document, &edit);

            // The fixture format requires a newline before the closing ``` fence, so
            // expected_document always ends with \n. Normalize both sides so that
            // edits producing output without a trailing newline still compare equal.
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
        ActionMode::OmitsAction(title_sub) => {
            let found = actions
                .iter()
                .find(|a| a.title.contains(title_sub.as_str()));
            assert!(
                found.is_none(),
                "fixture {}: expected no action with title containing {title_sub:?}, but found: {:?}",
                path.display(),
                found.map(|a| &a.title),
            );
        }
    }
}

// ---- Self-tests for harness helpers -----------------------------------------

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]
mod self_tests {
    use super::*;

    // ---- Group A: Frontmatter parsing ----------------------------------------

    // A1. test-name is parsed from frontmatter.
    #[test]
    fn frontmatter_parses_test_name() {
        let fm = "test-name: tab-convert\ncategory: editing\ncursor: 0:0\napplies-action: Foo\n";
        let (name, _, _, _, _) = parse_frontmatter(fm).unwrap();
        assert_eq!(name, "tab-convert");
    }

    // A2. cursor field parses to correct line and character.
    #[test]
    fn frontmatter_cursor_parses_line_and_char() {
        let fm = "test-name: foo\ncursor: 2:5\napplies-action: Foo\n";
        let (_, line, ch, _, _) = parse_frontmatter(fm).unwrap();
        assert_eq!(line, 2);
        assert_eq!(ch, 5);
    }

    // A3. applies-action is parsed.
    #[test]
    fn frontmatter_parses_applies_action() {
        let fm = "test-name: foo\ncursor: 0:0\napplies-action: Convert tabs\n";
        let (_, _, _, mode, _) = parse_frontmatter(fm).unwrap();
        assert_eq!(mode, ActionMode::AppliesAction("Convert tabs".to_string()));
    }

    // A4. omits-action is parsed.
    #[test]
    fn frontmatter_parses_omits_action() {
        let fm = "test-name: foo\ncursor: 0:0\nomits-action: Convert tabs\n";
        let (_, _, _, mode, _) = parse_frontmatter(fm).unwrap();
        assert_eq!(mode, ActionMode::OmitsAction("Convert tabs".to_string()));
    }

    // A5. Both applies-action and omits-action present returns error.
    #[test]
    fn frontmatter_both_action_modes_returns_error() {
        let fm = "test-name: foo\ncursor: 0:0\napplies-action: X\nomits-action: Y\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            err.contains("mutually exclusive"),
            "error should mention 'mutually exclusive': {err}"
        );
    }

    // A6. Missing cursor field returns error that mentions 'cursor'.
    #[test]
    fn frontmatter_missing_cursor_returns_error() {
        let fm = "test-name: foo\napplies-action: X\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            err.contains("cursor"),
            "error should mention 'cursor': {err}"
        );
    }

    // A7. Neither applies-action nor omits-action returns error.
    #[test]
    fn frontmatter_missing_mode_returns_error() {
        let fm = "test-name: foo\ncursor: 0:0\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            err.contains("applies-action") || err.contains("omits-action"),
            "error should mention the missing mode fields: {err}"
        );
    }

    // A8. cursor field with missing character component returns error.
    #[test]
    fn frontmatter_cursor_missing_colon_returns_error() {
        let fm = "test-name: foo\ncursor: 3\napplies-action: X\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            !err.is_empty(),
            "malformed cursor should return error: {err}"
        );
    }

    // A9. cursor field with non-numeric values returns error.
    #[test]
    fn frontmatter_cursor_non_numeric_returns_error() {
        let fm = "test-name: foo\ncursor: abc:def\napplies-action: X\n";
        let err = parse_frontmatter(fm).unwrap_err();
        assert!(
            !err.is_empty(),
            "non-numeric cursor should return error: {err}"
        );
    }

    // ---- Group B: Section extraction ----------------------------------------

    // B1. Test-Document section is extracted.
    #[test]
    fn extract_test_document_returns_fenced_content() {
        let body = "## Test-Document\n\n```yaml\nkey: value\n```\n";
        let result = extract_section(body, "Test-Document").unwrap();
        assert_eq!(result, "key: value\n");
    }

    // B2. Expected-Document section is extracted.
    #[test]
    fn extract_expected_document_returns_fenced_content() {
        let body = "## Test-Document\n\n```yaml\nfoo: bar\n```\n\n## Expected-Document\n\n```yaml\nbaz: qux\n```\n";
        let result = extract_section(body, "Expected-Document").unwrap();
        assert_eq!(result, "baz: qux\n");
    }

    // B3. Missing Test-Document section returns error naming the section.
    #[test]
    fn extract_test_document_missing_returns_error_naming_section() {
        let body = "## Expected-Document\n\n```yaml\nfoo: bar\n```\n";
        let err = extract_section(body, "Test-Document").unwrap_err();
        assert!(
            err.contains("Test-Document"),
            "error should name the missing section: {err}"
        );
    }

    // B4. Missing Expected-Document section returns error naming the section.
    #[test]
    fn extract_expected_document_missing_returns_error_naming_section() {
        let body = "## Test-Document\n\n```yaml\nfoo: bar\n```\n";
        let err = extract_section(body, "Expected-Document").unwrap_err();
        assert!(
            err.contains("Expected-Document"),
            "error should name the missing section: {err}"
        );
    }

    // B5. Language tag on fence opening is ignored.
    #[test]
    fn extract_section_ignores_fence_lang_tag() {
        let body = "## Test-Document\n\n```yaml\nkey: value\n```\n";
        let result = extract_section(body, "Test-Document").unwrap();
        assert_eq!(result, "key: value\n");
        assert!(
            !result.contains("yaml"),
            "lang tag should not appear in extracted content"
        );
    }

    // B6. omits-action mode does not require Expected-Document.
    #[test]
    fn parse_fixture_omits_action_no_expected_document_succeeds() {
        let content = "---\ntest-name: foo\ncursor: 0:0\nomits-action: tabs to spaces\n---\n\n## Test-Document\n\n```yaml\n  key: value\n```\n";
        let path = PathBuf::from("test.md");
        let fixture = parse_fixture(content, &path);
        assert!(matches!(fixture.mode, ActionMode::OmitsAction(_)));
        assert_eq!(fixture.expected_document, "");
    }

    // ---- Group C: TextEdit application ----------------------------------------

    // C1. Single-line replace edit produces correct output.
    #[test]
    fn apply_text_edit_single_line_replace() {
        let source = "\tkey: value\n";
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 11)),
            new_text: "  key: value".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "  key: value\n");
    }

    // C2. Edit range respects line boundaries — only target line is modified.
    #[test]
    fn apply_text_edit_preserves_other_lines() {
        let source = "line0\n\tline1\nline2\n";
        let edit = TextEdit {
            range: Range::new(Position::new(1, 0), Position::new(1, 6)),
            new_text: "  line1".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[0], "line0");
        assert_eq!(lines[1], "  line1");
        assert_eq!(lines[2], "line2");
    }

    // C3. Multibyte single-line edit: codepoint indices on a line with 2-byte chars.
    #[test]
    fn apply_text_edit_multibyte_single_line() {
        let source = "key: \"αα\"\n";
        // α is 2 bytes but 1 codepoint. Codepoint 6 = opening quote, codepoint 8 = before closing quote.
        let edit = TextEdit {
            range: Range::new(Position::new(0, 6), Position::new(0, 8)),
            new_text: "|".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "key: \"|\"\n");
    }

    // C4. Multibyte on end line only: end_col uses codepoint-to-byte on the end line.
    #[test]
    fn apply_text_edit_multibyte_on_end_line() {
        let source = "start\nkey: \"αα\"\n";
        // Edit spans lines 0–1: tail starts at codepoint 6 of line 1 ("αα\""),
        // codepoint 6 = after the opening quote = "αα\"" remaining.
        let edit = TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(1, 6)),
            new_text: " ".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "start αα\"\n");
    }

    // C5. Two-line collapse: no spurious empty line between start and end.
    #[test]
    fn apply_text_edit_multiline_two_lines() {
        let source = "[a,\nb]\n";
        // Replace from col 3 on line 0 to col 0 on line 1 with a space.
        let edit = TextEdit {
            range: Range::new(Position::new(0, 3), Position::new(1, 0)),
            new_text: " ".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "[a, b]\n");
    }

    // C6. Three-plus source lines absorbed: in-between lines emit nothing.
    #[test]
    fn apply_text_edit_multiline_three_source_lines_absorbed() {
        let source = "a\nb\nc\nd\n";
        // Edit from col 1 of line 0 to col 0 of line 3; lines 1 and 2 are absorbed.
        let edit = TextEdit {
            range: Range::new(Position::new(0, 1), Position::new(3, 0)),
            new_text: "-".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "a-d\n");
    }

    // C7. Multibyte characters in new_text across a multi-line edit.
    #[test]
    fn apply_text_edit_multiline_new_text_multibyte() {
        let source = "[a,\nb]\n";
        let edit = TextEdit {
            range: Range::new(Position::new(0, 3), Position::new(1, 0)),
            new_text: " α".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "[a, αb]\n");
    }

    // C8. start_col == 0: empty prefix slice does not panic.
    #[test]
    fn apply_text_edit_start_col_zero() {
        let source = "  key: value\n";
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 2)),
            new_text: "\t".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "\tkey: value\n");
    }

    // C9. end_col == line.len() (codepoints): unwrap_or(s.len()) fallback is exercised.
    #[test]
    fn apply_text_edit_end_col_at_eol() {
        let source = "key: value\nline2\n";
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 10)),
            new_text: "replaced".to_string(),
        };
        let result = apply_text_edit(source, &edit);
        assert_eq!(result, "replaced\nline2\n");
    }

    // ---- Group D: Full round-trip -----------------------------------------------

    // D1. applies-action happy path.
    #[test]
    fn applies_action_happy_path() {
        let text = "\tkey: value\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let range = cursor_range(0, 0);
        let actions = code_actions(&docs, text, range, &[], &uri, &YamlFormatOptions::default());

        let action = actions
            .iter()
            .find(|a| a.title.contains("tabs to spaces"))
            .unwrap_or_else(|| panic!("expected 'tabs to spaces' action"));
        let edit = extract_first_edit(action, &uri, "self_tests::applies_action_happy_path");
        let result = apply_text_edit(text, &edit);
        assert_eq!(result, "  key: value\n");
    }

    // D2. applies-action with non-matching title substring returns no action.
    #[test]
    fn applies_action_nonexistent_title_not_found() {
        let text = "  key: value\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let range = cursor_range(0, 0);
        let actions = code_actions(&docs, text, range, &[], &uri, &YamlFormatOptions::default());

        let found = actions
            .iter()
            .find(|a| a.title.contains("nonexistent action"));
        assert!(found.is_none(), "should not find a non-existent action");
    }

    // D3. omits-action happy path: no tab action when no tabs present.
    #[test]
    fn omits_action_happy_path() {
        let text = "  key: value\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let range = cursor_range(0, 0);
        let actions = code_actions(&docs, text, range, &[], &uri, &YamlFormatOptions::default());

        let found = actions.iter().find(|a| a.title.contains("tabs to spaces"));
        assert!(
            found.is_none(),
            "expected no 'tabs to spaces' action when no tabs present"
        );
    }

    // D4. omits-action fails when action IS present.
    #[test]
    #[should_panic(expected = "expected no action with title containing")]
    fn omits_action_fails_when_action_present() {
        let text = "\tkey: value\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let range = cursor_range(0, 0);
        let actions = code_actions(&docs, text, range, &[], &uri, &YamlFormatOptions::default());

        // Simulate the harness OmitsAction assertion — this should panic because
        // the "tabs to spaces" action IS present when the document has a tab.
        let title_sub = "tabs to spaces";
        let found = actions.iter().find(|a| a.title.contains(title_sub));
        assert!(
            found.is_none(),
            "fixture test.md: expected no action with title containing {title_sub:?}, but found: {:?}",
            found.map(|a| &a.title),
        );
    }

    // ---- Group E: format-options frontmatter parsing -------------------------

    // E1. format-options block with all supported keys parses to correct options.
    #[test]
    fn format_options_all_keys_parsed() {
        let fm = "test-name: foo\ncursor: 0:0\napplies-action: X\nformat-options:\n  print_width: 60\n  single_quote: true\n  bracket_spacing: false\n  preserve_quotes: true\n  tab_width: 4\n";
        let (_, _, _, _, opts) = parse_frontmatter(fm).unwrap();
        assert_eq!(opts.print_width, 60);
        assert!(opts.single_quote);
        assert!(!opts.bracket_spacing);
        assert!(opts.preserve_quotes);
        assert_eq!(opts.tab_width, 4);
    }

    // E2. Partial format-options block: specified fields set, unspecified remain default.
    #[test]
    fn format_options_partial_keys_others_remain_default() {
        let fm =
            "test-name: foo\ncursor: 0:0\napplies-action: X\nformat-options:\n  print_width: 40\n";
        let (_, _, _, _, opts) = parse_frontmatter(fm).unwrap();
        let defaults = YamlFormatOptions::default();
        assert_eq!(opts.print_width, 40);
        assert_eq!(opts.single_quote, defaults.single_quote);
        assert_eq!(opts.bracket_spacing, defaults.bracket_spacing);
        assert_eq!(opts.preserve_quotes, defaults.preserve_quotes);
        assert_eq!(opts.tab_width, defaults.tab_width);
    }

    // E3. Empty format-options block (header present, no indented keys) yields defaults.
    #[test]
    fn format_options_empty_block_yields_defaults() {
        let fm = "test-name: foo\ncursor: 0:0\napplies-action: X\nformat-options:\n";
        let (_, _, _, _, opts) = parse_frontmatter(fm).unwrap();
        let defaults = YamlFormatOptions::default();
        assert_eq!(opts.print_width, defaults.print_width);
        assert_eq!(opts.single_quote, defaults.single_quote);
    }

    // E4. Unknown key in format-options block is silently ignored.
    #[test]
    fn format_options_unknown_key_silently_ignored() {
        let fm = "test-name: foo\ncursor: 0:0\napplies-action: X\nformat-options:\n  print_width: 50\n  unknown_setting: xyz\n";
        let (_, _, _, _, opts) = parse_frontmatter(fm).unwrap();
        assert_eq!(opts.print_width, 50);
    }

    // E5. Frontmatter without format-options block yields default options.
    #[test]
    fn format_options_absent_yields_defaults() {
        let fm = "test-name: foo\ncursor: 0:0\napplies-action: X\n";
        let (_, _, _, _, opts) = parse_frontmatter(fm).unwrap();
        let defaults = YamlFormatOptions::default();
        assert_eq!(opts.print_width, defaults.print_width);
        assert_eq!(opts.single_quote, defaults.single_quote);
        assert_eq!(opts.bracket_spacing, defaults.bracket_spacing);
        assert_eq!(opts.preserve_quotes, defaults.preserve_quotes);
        assert_eq!(opts.tab_width, defaults.tab_width);
    }

    // E6. Malformed print_width value panics with a message naming the field.
    #[test]
    #[should_panic(expected = "print_width")]
    fn format_options_invalid_print_width_panics() {
        let content = "---\ntest-name: foo\ncursor: 0:0\napplies-action: X\nformat-options:\n  print_width: notanumber\n---\n\n## Test-Document\n\n```yaml\nkey: value\n```\n\n## Expected-Document\n\n```yaml\nkey: value\n```\n";
        let path = PathBuf::from("test.md");
        let _ = parse_fixture(content, &path);
    }

    // E7. Malformed tab_width value panics with a message naming the field.
    #[test]
    #[should_panic(expected = "tab_width")]
    fn format_options_invalid_tab_width_panics() {
        let content = "---\ntest-name: foo\ncursor: 0:0\napplies-action: X\nformat-options:\n  tab_width: notanumber\n---\n\n## Test-Document\n\n```yaml\nkey: value\n```\n\n## Expected-Document\n\n```yaml\nkey: value\n```\n";
        let path = PathBuf::from("test.md");
        let _ = parse_fixture(content, &path);
    }

    // ---- Group F: format-options applied to code_actions ---------------------

    // F1. Custom print_width in format-options is passed to code_actions and affects output.
    #[test]
    fn format_options_print_width_reaches_code_actions() {
        // Input whose single-line flow form is ~92 chars — exceeds default 80, fits within 120.
        let text = "items:\n  - alpha_item_one\n  - bravo_item_two\n  - charlie_item_three\n  - delta_item_four\n  - echo_item_five\n";
        let uri = test_uri();
        let docs = docs_for(text);
        let range = cursor_range(0, 0);

        // With default print_width (80): the single-line form doesn't fit, so the
        // formatter breaks the flow sequence across multiple lines.
        let narrow_actions =
            code_actions(&docs, text, range, &[], &uri, &YamlFormatOptions::default());
        let narrow_action = narrow_actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap_or_else(|| panic!("expected block-to-flow action with default options"));
        let narrow_edit = extract_first_edit(
            narrow_action,
            &uri,
            "self_tests::format_options_print_width_reaches_code_actions",
        );
        let narrow_result = apply_text_edit(text, &narrow_edit);
        // Under default width the sequence wraps — result is multi-line.
        assert!(
            narrow_result.lines().count() > 1,
            "expected multi-line output under default print_width=80, got: {narrow_result:?}"
        );

        // With print_width 120: the single-line form fits, so the formatter keeps it inline.
        let wide_opts = YamlFormatOptions {
            print_width: 120,
            ..YamlFormatOptions::default()
        };
        let wide_actions = code_actions(&docs, text, range, &[], &uri, &wide_opts);
        let wide_action = wide_actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap_or_else(|| panic!("expected block-to-flow action with wide options"));
        let wide_edit = extract_first_edit(
            wide_action,
            &uri,
            "self_tests::format_options_print_width_reaches_code_actions",
        );
        let wide_result = apply_text_edit(text, &wide_edit);
        // Under print_width=120 the sequence fits on one line.
        assert_eq!(
            wide_result.lines().count(),
            1,
            "expected single-line output under print_width=120, got: {wide_result:?}"
        );
    }

    // F2. parse_fixture with no format-options returns options equal to default.
    #[test]
    fn format_options_absent_in_fixture_uses_defaults() {
        let content = "---\ntest-name: foo\ncursor: 0:0\napplies-action: X\n---\n\n## Test-Document\n\n```yaml\nkey: value\n```\n\n## Expected-Document\n\n```yaml\nkey: value\n```\n";
        let path = PathBuf::from("test.md");
        let fixture = parse_fixture(content, &path);
        let defaults = YamlFormatOptions::default();
        assert_eq!(fixture.format_options.print_width, defaults.print_width);
        assert_eq!(fixture.format_options.single_quote, defaults.single_quote);
        assert_eq!(
            fixture.format_options.bracket_spacing,
            defaults.bracket_spacing
        );
        assert_eq!(
            fixture.format_options.preserve_quotes,
            defaults.preserve_quotes
        );
        assert_eq!(fixture.format_options.tab_width, defaults.tab_width);
    }
}
