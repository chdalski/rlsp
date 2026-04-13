// SPDX-License-Identifier: MIT
//
// Fixture-driven formatter tests.
//
// Each file in `tests/fixtures/formatter/*.md` is a self-contained test case.
// The file format is:
//
//   ---
//   test-name: descriptive-kebab-case-name
//   category: quoting | flow-style | ...
//   settings:
//     single_quote: true
//     print_width: 40
//   idempotent: true
//   ---
//
//   # Test: Title
//
//   Optional prose description.
//
//   ## Test-Document
//
//   ```yaml
//   input here
//   ```
//
//   ## Expected-Document   (omit when idempotent: true)
//
//   ```yaml
//   expected output here
//   ```
//
// rstest `#[files]` generates one independent test per matched file,
// giving per-file pass/fail visibility in test output.

#![expect(missing_docs, reason = "test code")]

use std::path::{Path, PathBuf};

use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::server::YamlVersion;
use rstest::rstest;

// ---- Data model -------------------------------------------------------------

/// The parsed contents of a fixture file.
#[derive(Debug)]
struct FixtureSpec {
    /// `test-name` from frontmatter (informational).
    test_name: String,
    /// Formatting options derived from frontmatter `settings:`.
    options: YamlFormatOptions,
    /// When `true`, assert `format(format(input)) == format(input)`.
    /// No `Expected-Document` section is required.
    idempotent: bool,
    /// Raw YAML from the `## Test-Document` fenced block.
    test_document: String,
    /// Raw YAML from `## Expected-Document`. Empty string when `idempotent: true`.
    expected_document: String,
}

// ---- Frontmatter parsing ----------------------------------------------------

/// Split the leading YAML frontmatter from the rest of the file.
///
/// Returns `(frontmatter_text, body_text)` where `frontmatter_text` is the raw
/// content between the two `---` delimiters (excluding the delimiters themselves)
/// and `body_text` is everything after the closing `---`.
fn split_frontmatter(content: &str) -> Result<(&str, &str), String> {
    let content = content.strip_prefix("---\n").ok_or_else(|| {
        "fixture file must start with '---\\n' (frontmatter opening delimiter)".to_string()
    })?;

    let close = content.find("\n---\n").ok_or_else(|| {
        "fixture file frontmatter is not closed (no closing '---' delimiter found)".to_string()
    })?;

    let frontmatter = &content[..close];
    let body = &content[close + 5..]; // skip "\n---\n"
    Ok((frontmatter, body))
}

/// Parse the frontmatter and body into a [`FixtureSpec`].
///
/// `path` is used only for error messages.
fn parse_fixture(content: &str, path: &Path) -> FixtureSpec {
    let path_str = path.display().to_string();

    let (frontmatter, body) = split_frontmatter(content)
        .unwrap_or_else(|e| panic_with_message(&format!("fixture {path_str}: {e}")));

    let (test_name, idempotent, options) = parse_frontmatter(frontmatter, &path_str);

    let test_document = extract_section(body, "Test-Document", &path_str)
        .unwrap_or_else(|e| panic_with_message(&format!("fixture {path_str}: {e}")));

    let expected_document = if idempotent {
        String::new()
    } else {
        extract_section(body, "Expected-Document", &path_str)
            .unwrap_or_else(|e| panic_with_message(&format!("fixture {path_str}: {e}")))
    };

    FixtureSpec {
        test_name,
        options,
        idempotent,
        test_document,
        expected_document,
    }
}

/// Terminate the test with a clear message.
///
/// This helper exists so that `#[expect(clippy::panic)]` can be placed
/// here rather than scattered across every call site.
#[expect(
    clippy::panic,
    reason = "test harness reports fixture errors via panic"
)]
fn panic_with_message(msg: &str) -> ! {
    panic!("{msg}")
}

/// Parse a flat YAML-ish frontmatter block.
///
/// Returns `(test_name, idempotent, options)`.
///
/// Supports:
/// - `test-name: <value>`
/// - `category: <value>` (informational, ignored)
/// - `idempotent: true|false`
/// - `settings:` block with indented key-value pairs
///
/// Unknown keys and unknown settings keys are silently ignored.
fn parse_frontmatter(frontmatter: &str, path_str: &str) -> (String, bool, YamlFormatOptions) {
    let mut test_name = String::new();
    let mut idempotent = false;
    let mut options = YamlFormatOptions::default();

    let mut in_settings = false;

    for line in frontmatter.lines() {
        if line.is_empty() {
            continue;
        }

        // Detect the `settings:` section header.
        if line == "settings:" {
            in_settings = true;
            continue;
        }

        // A non-indented, non-empty line ends the settings block.
        if in_settings && !line.starts_with(' ') && !line.starts_with('\t') {
            in_settings = false;
        }

        if in_settings {
            // Setting line: `  key: value`
            let trimmed = line.trim();
            if let Some((key, value)) = trimmed.split_once(": ") {
                let value = value.trim().trim_matches('"');
                apply_setting(&mut options, key.trim(), value, path_str);
            }
        } else if let Some((key, value)) = line.split_once(": ") {
            let value = value.trim();
            match key.trim() {
                "test-name" => test_name = value.to_string(),
                "idempotent" => idempotent = value == "true",
                // category and other top-level keys are ignored
                _ => {}
            }
        }
    }

    (test_name, idempotent, options)
}

/// Apply a single setting key-value pair to `options`.
///
/// Unknown keys are silently ignored — forward-compatible with future options
/// that do not yet exist in `YamlFormatOptions`.
fn apply_setting(options: &mut YamlFormatOptions, key: &str, value: &str, path_str: &str) {
    match key {
        "print_width" => {
            options.print_width = value.parse().unwrap_or_else(|_| {
                panic_with_message(&format!(
                    "fixture {path_str}: invalid print_width: {value:?}"
                ))
            });
        }
        "tab_width" => {
            options.tab_width = value.parse().unwrap_or_else(|_| {
                panic_with_message(&format!("fixture {path_str}: invalid tab_width: {value:?}"))
            });
        }
        "use_tabs" => {
            options.use_tabs = value == "true";
        }
        "single_quote" => {
            options.single_quote = value == "true";
        }
        "bracket_spacing" => {
            options.bracket_spacing = value == "true";
        }
        "yaml_version" => {
            options.yaml_version = match value {
                "1.1" => YamlVersion::V1_1,
                "1.2" => YamlVersion::V1_2,
                other => panic_with_message(&format!(
                    "fixture {path_str}: unknown yaml_version: {other:?} (expected \"1.1\" or \"1.2\")"
                )),
            };
        }
        "format_enforce_block_style" => {
            options.format_enforce_block_style = value == "true";
        }
        "format_remove_duplicate_keys" => {
            options.format_remove_duplicate_keys = value == "true";
        }
        // Unknown settings keys are silently ignored.
        _ => {}
    }
}

// ---- Section extraction -----------------------------------------------------

/// Extract the YAML content from a fenced code block under a given `## Section` heading.
///
/// Returns `Err` with a clear message when the section or fenced block is not found.
fn extract_section(body: &str, section: &str, path_str: &str) -> Result<String, String> {
    // Find the section heading.
    let heading = format!("## {section}");
    let section_start = body
        .find(&heading)
        .ok_or_else(|| format!("missing '## {section}' section (required for {path_str})"))?;

    let after_heading = &body[section_start + heading.len()..];

    // Find the opening fence.
    let fence_open = after_heading.find("```").ok_or_else(|| {
        format!("missing opening '```' fence in '## {section}' section ({path_str})")
    })?;

    let after_fence = &after_heading[fence_open + 3..];

    // Skip an optional language tag on the same line as the fence (e.g., "yaml\n").
    let content_start = after_fence
        .find('\n')
        .ok_or_else(|| format!("no newline after opening fence in '## {section}' ({path_str})"))?;
    let content = &after_fence[content_start + 1..];

    // Find the closing fence.
    let fence_close = content.find("```").ok_or_else(|| {
        format!("missing closing '```' fence in '## {section}' section ({path_str})")
    })?;

    Ok(content[..fence_close].to_string())
}

// ---- rstest harness ---------------------------------------------------------

#[rstest]
fn formatter_fixture(#[files("tests/fixtures/formatter/*.md")] path: PathBuf) {
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic_with_message(&format!("failed to read {}: {e}", path.display())));

    let fixture = parse_fixture(&content, &path);

    let first = format_yaml(&fixture.test_document, &fixture.options);

    if fixture.idempotent {
        // Idempotency mode: assert format(format(input)) == format(input).
        // Also assert that a non-empty input produces non-empty output (guards
        // against a formatter that silently returns empty string for all inputs).
        if !fixture.test_document.trim().is_empty() {
            assert!(
                !first.trim().is_empty(),
                "fixture {}: formatter returned empty output for non-empty input\ntest-name: {}\ninput: {:?}",
                path.display(),
                fixture.test_name,
                fixture.test_document,
            );
        }
        let second = format_yaml(&first, &fixture.options);
        assert_eq!(
            first,
            second,
            "fixture {}: formatter is not idempotent\ntest-name: {}\nfirst:  {:?}\nsecond: {:?}",
            path.display(),
            fixture.test_name,
            first,
            second,
        );
    } else {
        assert_eq!(
            first,
            fixture.expected_document,
            "fixture {}: output mismatch\ntest-name: {}\ninput:    {:?}\nexpected: {:?}\ngot:      {:?}",
            path.display(),
            fixture.test_name,
            fixture.test_document,
            fixture.expected_document,
            first,
        );
    }
}

// ---- Unit tests for harness helpers -----------------------------------------

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    fn make_path(name: &str) -> PathBuf {
        PathBuf::from(format!("tests/fixtures/formatter/{name}.md"))
    }

    // Helper: build a minimal fixture file string.
    fn fixture_file(frontmatter: &str, body: &str) -> String {
        format!("---\n{frontmatter}\n---\n\n{body}")
    }

    // ---- Frontmatter parsing ------------------------------------------------

    // 1. test-name is parsed correctly.
    #[test]
    fn frontmatter_parses_test_name() {
        let fm = "test-name: my-fixture\ncategory: structure\n";
        let (name, _, _) = parse_frontmatter(fm, "test.md");
        assert_eq!(name, "my-fixture");
    }

    // 2. Omitted settings block → default YamlFormatOptions.
    #[test]
    fn frontmatter_parses_all_default_settings() {
        let fm = "test-name: foo\ncategory: structure\n";
        let (_, _, opts) = parse_frontmatter(fm, "test.md");
        let default = YamlFormatOptions::default();
        // Compare field by field (YamlFormatOptions does not implement PartialEq).
        assert_eq!(opts.print_width, default.print_width);
        assert_eq!(opts.tab_width, default.tab_width);
        assert_eq!(opts.use_tabs, default.use_tabs);
        assert_eq!(opts.single_quote, default.single_quote);
        assert_eq!(opts.bracket_spacing, default.bracket_spacing);
        assert_eq!(opts.yaml_version, default.yaml_version);
        assert_eq!(
            opts.format_enforce_block_style,
            default.format_enforce_block_style
        );
        assert_eq!(
            opts.format_remove_duplicate_keys,
            default.format_remove_duplicate_keys
        );
    }

    // 3. single_quote: true is parsed.
    #[test]
    fn frontmatter_parses_single_quote_setting() {
        let fm = "test-name: foo\nsettings:\n  single_quote: true\n";
        let (_, _, opts) = parse_frontmatter(fm, "test.md");
        assert!(opts.single_quote);
        // Other fields stay at default.
        assert_eq!(opts.print_width, YamlFormatOptions::default().print_width);
    }

    // 4. print_width is parsed.
    #[test]
    fn frontmatter_parses_print_width_setting() {
        let fm = "test-name: foo\nsettings:\n  print_width: 40\n";
        let (_, _, opts) = parse_frontmatter(fm, "test.md");
        assert_eq!(opts.print_width, 40);
    }

    // 5. yaml_version "1.1" → YamlVersion::V1_1.
    #[test]
    fn frontmatter_parses_yaml_version_1_1() {
        let fm = "test-name: foo\nsettings:\n  yaml_version: \"1.1\"\n";
        let (_, _, opts) = parse_frontmatter(fm, "test.md");
        assert_eq!(opts.yaml_version, YamlVersion::V1_1);
    }

    // 6. yaml_version "1.2" → YamlVersion::V1_2.
    #[test]
    fn frontmatter_parses_yaml_version_1_2() {
        let fm = "test-name: foo\nsettings:\n  yaml_version: \"1.2\"\n";
        let (_, _, opts) = parse_frontmatter(fm, "test.md");
        assert_eq!(opts.yaml_version, YamlVersion::V1_2);
    }

    // 7. idempotent: true is parsed.
    #[test]
    fn frontmatter_parses_idempotent_true() {
        let fm = "test-name: foo\nidempotent: true\n";
        let (_, idempotent, _) = parse_frontmatter(fm, "test.md");
        assert!(idempotent);
    }

    // 8. Missing idempotent → defaults to false.
    #[test]
    fn frontmatter_idempotent_defaults_to_false() {
        let fm = "test-name: foo\n";
        let (_, idempotent, _) = parse_frontmatter(fm, "test.md");
        assert!(!idempotent);
    }

    // 9. Unknown settings key is silently ignored.
    #[test]
    fn frontmatter_unknown_setting_key_is_ignored() {
        let fm = "test-name: foo\nsettings:\n  nonexistent_field: 999\n";
        let (_, _, opts) = parse_frontmatter(fm, "test.md");
        // Must not panic, and options must equal defaults.
        let default = YamlFormatOptions::default();
        assert_eq!(opts.print_width, default.print_width);
        assert_eq!(opts.single_quote, default.single_quote);
    }

    // ---- Section extraction -------------------------------------------------

    // 10. Test-Document section content is extracted.
    #[test]
    fn extract_test_document_returns_fenced_content() {
        let body = "## Test-Document\n\n```yaml\nfoo: bar\n```\n";
        let result = extract_section(body, "Test-Document", "test.md").unwrap();
        assert_eq!(result, "foo: bar\n");
    }

    // 11. Expected-Document section content is extracted.
    #[test]
    fn extract_expected_document_returns_fenced_content() {
        let body = "## Expected-Document\n\n```yaml\nfoo: bar\n```\n";
        let result = extract_section(body, "Expected-Document", "test.md").unwrap();
        assert_eq!(result, "foo: bar\n");
    }

    // 12. Missing Test-Document section returns Err with a clear message.
    #[test]
    fn extract_test_document_missing_section_returns_err() {
        let body = "## Expected-Document\n\n```yaml\nfoo: bar\n```\n";
        let err = extract_section(body, "Test-Document", "test.md").unwrap_err();
        assert!(
            err.contains("Test-Document"),
            "error message should name the missing section: {err}"
        );
    }

    // 13. Missing Expected-Document returns Err with a clear message.
    #[test]
    fn extract_expected_document_missing_returns_err() {
        let body = "## Test-Document\n\n```yaml\nfoo: bar\n```\n";
        let err = extract_section(body, "Expected-Document", "test.md").unwrap_err();
        assert!(
            err.contains("Expected-Document"),
            "error message should name the missing section: {err}"
        );
    }

    // 14. Prose between heading and fence is not included in output.
    #[test]
    fn extract_test_document_ignores_prose_before_fence() {
        let body = "## Test-Document\n\nThis is a description.\n\n```yaml\nkey: value\n```\n";
        let result = extract_section(body, "Test-Document", "test.md").unwrap();
        assert_eq!(result, "key: value\n");
        assert!(
            !result.contains("description"),
            "prose should not appear in extracted content"
        );
    }

    // 15. Interior blank lines inside the fence are preserved.
    #[test]
    fn extract_section_preserves_interior_blank_lines() {
        let body = "## Test-Document\n\n```yaml\n\nfoo: bar\n\n```\n";
        let result = extract_section(body, "Test-Document", "test.md").unwrap();
        assert_eq!(result, "\nfoo: bar\n\n");
    }

    // ---- Frontmatter error handling -----------------------------------------

    // 16. File without leading `---` delimiter returns Err.
    #[test]
    fn missing_frontmatter_delimiters_errors() {
        let content = "test-name: foo\n\n## Test-Document\n```yaml\nfoo: bar\n```\n";
        let err = split_frontmatter(content).unwrap_err();
        assert!(
            err.contains("---"),
            "error should mention the missing delimiter: {err}"
        );
    }

    // 17. Unclosed frontmatter (no closing `---`) returns Err.
    #[test]
    fn unclosed_frontmatter_errors() {
        let content = "---\ntest-name: foo\n";
        let err = split_frontmatter(content).unwrap_err();
        assert!(
            !err.is_empty(),
            "unclosed frontmatter should return an error"
        );
    }

    // 18. Empty frontmatter block → options equal defaults.
    #[test]
    fn empty_frontmatter_block_uses_defaults() {
        let body = "## Test-Document\n\n```yaml\nfoo: bar\n```\n## Expected-Document\n\n```yaml\nfoo: bar\n```\n";
        let content = fixture_file("", body);
        let path = make_path("empty-fm");
        let fixture = parse_fixture(&content, &path);
        let default = YamlFormatOptions::default();
        assert_eq!(fixture.options.print_width, default.print_width);
        assert_eq!(fixture.options.single_quote, default.single_quote);
        assert!(!fixture.idempotent);
    }
}
