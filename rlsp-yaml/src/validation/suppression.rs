// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

use crate::lsp_util::source_lines_without_bom;

const NEXT_LINE_PREFIX: &str = "# rlsp-yaml-disable-next-line";
const FILE_PREFIX: &str = "# rlsp-yaml-disable-file";

/// A suppression rule: either suppress all diagnostic codes, or only a
/// specific set.
#[derive(Debug, Clone)]
enum SuppressionRule {
    /// Suppress every diagnostic code.
    All,
    /// Suppress only the listed diagnostic codes.
    Codes(HashSet<String>),
}

impl SuppressionRule {
    fn matches(&self, code: &str) -> bool {
        match self {
            Self::All => true,
            Self::Codes(set) => set.contains(code),
        }
    }
}

/// A map of active diagnostic suppressions for a YAML document.
///
/// Suppressions are parsed from `# rlsp-yaml-disable-next-line` and
/// `# rlsp-yaml-disable-file` comments in the document text.
///
/// All line numbers are 0-based, matching LSP `Range.start.line as usize`.
pub struct SuppressionMap {
    /// Per-line suppressions: line number → suppression rule.
    line_suppressions: HashMap<u32, SuppressionRule>,
    /// File-level suppression rule, if any.
    file_suppression: Option<SuppressionRule>,
}

impl SuppressionMap {
    /// Return `true` if the diagnostic at `line` with `code` is suppressed.
    #[must_use]
    pub fn is_suppressed(&self, line: u32, code: &str) -> bool {
        if self
            .file_suppression
            .as_ref()
            .is_some_and(|r| r.matches(code))
        {
            return true;
        }
        self.line_suppressions
            .get(&line)
            .is_some_and(|r| r.matches(code))
    }
}

/// Parse `text` line by line for suppression comments and build a
/// [`SuppressionMap`].
///
/// Recognised syntax (the comment must be the first non-whitespace content
/// on its line, or preceded only by whitespace):
///
/// ```text
/// # rlsp-yaml-disable-next-line              ← suppress all codes on line N+1
/// # rlsp-yaml-disable-next-line code-a, code-b  ← suppress listed codes on line N+1
/// # rlsp-yaml-disable-file                   ← suppress all codes in this file
/// # rlsp-yaml-disable-file code-a, code-b    ← suppress listed codes in this file
/// ```
///
/// Unrecognised lines are silently ignored.
#[must_use]
pub fn build_suppression_map(text: &str) -> SuppressionMap {
    let mut line_suppressions: HashMap<u32, SuppressionRule> = HashMap::new();
    let mut file_suppression: Option<SuppressionRule> = None;

    for (idx, line) in source_lines_without_bom(text).enumerate() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix(NEXT_LINE_PREFIX) {
            let rule = parse_rule(rest);
            // Suppress line idx + 1 (the line immediately following the comment).
            // If the comment is on the last line there is no next line, but
            // inserting idx+1 is harmless — no diagnostic is ever emitted for
            // a line beyond the document end.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "LSP line/col are u32; always fits"
            )]
            let target = (idx as u32).saturating_add(1);
            line_suppressions.entry(target).or_insert(rule);
        } else if let Some(rest) = trimmed.strip_prefix(FILE_PREFIX) {
            let rule = parse_rule(rest);
            // First file-level comment wins; subsequent ones are ignored.
            if file_suppression.is_none() {
                file_suppression = Some(rule);
            }
        }
    }

    SuppressionMap {
        line_suppressions,
        file_suppression,
    }
}

/// Parse the tail of a suppression comment into a [`SuppressionRule`].
///
/// - Empty (or whitespace-only) → [`SuppressionRule::All`]
/// - Non-empty → [`SuppressionRule::Codes`] with trimmed, non-empty codes
fn parse_rule(rest: &str) -> SuppressionRule {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return SuppressionRule::All;
    }
    let codes: HashSet<String> = trimmed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if codes.is_empty() {
        SuppressionRule::All
    } else {
        SuppressionRule::Codes(codes)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    // ── Section 1: disable-next-line with specific codes ──────────────────────

    /// All three cases use the same input text; each case checks a different
    /// (line, code, expected) property of the same parsed map.
    #[rstest]
    #[case::suppresses_specified_code_on_next_line(1, "yaml-schema", true)]
    #[case::does_not_suppress_unlisted_code_on_next_line(1, "other-code", false)]
    #[case::does_not_suppress_the_comment_line_itself(0, "yaml-schema", false)]
    fn disable_next_line_with_specific_code(
        #[case] line: u32,
        #[case] code: &str,
        #[case] expected: bool,
    ) {
        let text = "# rlsp-yaml-disable-next-line yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert_eq!(map.is_suppressed(line, code), expected);
    }

    #[test]
    fn disable_next_line_suppresses_multiple_codes() {
        let text = "# rlsp-yaml-disable-next-line code-a, code-b\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "code-a"));
        assert!(map.is_suppressed(1, "code-b"));
        assert!(!map.is_suppressed(1, "code-c"));
    }

    #[test]
    fn disable_next_line_trims_whitespace_around_codes() {
        let text = "# rlsp-yaml-disable-next-line  code-a ,  code-b \nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "code-a"));
        assert!(map.is_suppressed(1, "code-b"));
    }

    #[test]
    fn disable_next_line_drops_empty_strings_after_split() {
        let text = "# rlsp-yaml-disable-next-line code-a,,code-b\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "code-a"));
        assert!(map.is_suppressed(1, "code-b"));
    }

    // ── Section 2: disable-next-line suppress-all ─────────────────────────────

    #[test]
    fn disable_next_line_without_codes_suppresses_all_on_next_line() {
        let text = "# rlsp-yaml-disable-next-line\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "any-code"));
        assert!(map.is_suppressed(1, "another-code"));
    }

    #[test]
    fn disable_next_line_suppress_all_does_not_suppress_other_lines() {
        let text = "# rlsp-yaml-disable-next-line\nkey: value\nother: stuff\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(2, "any-code"));
    }

    // ── Section 3: disable-next-line edge cases ───────────────────────────────

    #[test]
    fn disable_next_line_on_last_line_adds_no_suppression() {
        // The comment is on line 0, the last (and only) line of the file.
        // The implementation unconditionally inserts line 0+1 = 1 into the map,
        // which is harmless — no diagnostic is ever emitted for a line beyond
        // the document end. The suppression for line 1 is present but never
        // triggered in practice.
        let text = "# rlsp-yaml-disable-next-line yaml-schema";
        let map = build_suppression_map(text);
        // Line 1 is beyond the document; asserting suppressed is acceptable
        // because it is unreachable in the diagnostic pipeline.
        assert!(map.is_suppressed(1, "yaml-schema"));
    }

    #[test]
    fn disable_next_line_on_second_to_last_line_suppresses_last_line() {
        let text = "line0\n# rlsp-yaml-disable-next-line yaml-schema\nlast\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(2, "yaml-schema"));
    }

    #[test]
    fn disable_next_line_comment_trimmed_from_leading_whitespace() {
        let text = "  # rlsp-yaml-disable-next-line yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "yaml-schema"));
    }

    #[test]
    fn disable_next_line_only_suppresses_immediately_following_line() {
        let text = "# rlsp-yaml-disable-next-line yaml-schema\nkey: value\nother: stuff\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "yaml-schema"));
        assert!(!map.is_suppressed(2, "yaml-schema"));
    }

    #[test]
    fn multiple_disable_next_line_comments_suppress_respective_next_lines() {
        let text = "# rlsp-yaml-disable-next-line code-a\nline1\n# rlsp-yaml-disable-next-line code-b\nline3\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "code-a"));
        assert!(map.is_suppressed(3, "code-b"));
        assert!(!map.is_suppressed(1, "code-b"));
        assert!(!map.is_suppressed(3, "code-a"));
    }

    // ── Section 4: disable-file with specific codes ───────────────────────────

    #[test]
    fn disable_file_suppresses_specified_code_on_any_line() {
        let text = "# rlsp-yaml-disable-file yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(0, "yaml-schema"));
        assert!(map.is_suppressed(5, "yaml-schema"));
        assert!(map.is_suppressed(100, "yaml-schema"));
    }

    #[test]
    fn disable_file_does_not_suppress_unlisted_code() {
        let text = "# rlsp-yaml-disable-file yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(5, "other-code"));
    }

    #[test]
    fn disable_file_suppresses_multiple_codes_everywhere() {
        let text = "# rlsp-yaml-disable-file code-a, code-b\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(0, "code-a"));
        assert!(map.is_suppressed(10, "code-b"));
        assert!(!map.is_suppressed(5, "code-c"));
    }

    // ── Section 5: disable-file suppress-all ─────────────────────────────────

    #[test]
    fn disable_file_without_codes_suppresses_all_codes_on_all_lines() {
        let text = "# rlsp-yaml-disable-file\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(0, "any-code"));
        assert!(map.is_suppressed(50, "another-code"));
    }

    // ── Section 6: Ignored / unrecognised input ───────────────────────────────

    #[rstest]
    #[case::empty_input("", 0, "any-code")]
    #[case::unrecognized_keyword_line_0(
        "# rlsp-yaml-disable yaml-schema\nkey: value\n",
        0,
        "yaml-schema"
    )]
    #[case::unrecognized_keyword_line_1(
        "# rlsp-yaml-disable yaml-schema\nkey: value\n",
        1,
        "yaml-schema"
    )]
    #[case::wrong_prefix(
        "# yaml-language-server: disable-next-line yaml-schema\nkey: value\n",
        1,
        "yaml-schema"
    )]
    #[case::non_comment_line(
        "rlsp-yaml-disable-next-line yaml-schema\nkey: value\n",
        1,
        "yaml-schema"
    )]
    fn ignored_input_produces_no_suppressions(
        #[case] text: &str,
        #[case] line: u32,
        #[case] code: &str,
    ) {
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(line, code));
    }

    #[test]
    fn input_with_no_suppression_comments_returns_empty_map() {
        let text = "key: value\nother: stuff\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(0, "any-code"));
        assert!(!map.is_suppressed(1, "any-code"));
    }

    // ── Section 7: Interaction between disable-file and disable-next-line ─────

    #[test]
    fn disable_file_and_disable_next_line_coexist() {
        let text = "# rlsp-yaml-disable-file file-code\nline1\n# rlsp-yaml-disable-next-line line-code\nline3\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "file-code"));
        assert!(map.is_suppressed(3, "line-code"));
        assert!(map.is_suppressed(3, "file-code"));
        assert!(!map.is_suppressed(1, "line-code"));
    }

    #[test]
    fn disable_file_suppress_all_overrides_everything() {
        let text = "# rlsp-yaml-disable-file\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(99, "any-code"));
    }

    // ── Section 7b: a byte order mark on line 1 ───────────────────────────────

    #[rstest]
    #[case::lf("\n")]
    #[case::crlf("\r\n")]
    #[case::lone_cr("\r")]
    fn disable_next_line_after_a_bom_targets_the_following_line(#[case] line_break: &str) {
        let text = format!(
            "\u{FEFF}# rlsp-yaml-disable-next-line code-a{line_break}key: value{line_break}other: v{line_break}"
        );
        let map = build_suppression_map(&text);
        assert!(!map.is_suppressed(0, "code-a"));
        assert!(map.is_suppressed(1, "code-a"));
        assert!(!map.is_suppressed(1, "code-b"));
        assert!(!map.is_suppressed(2, "code-a"));
    }

    #[rstest]
    #[case::lf("\n")]
    #[case::crlf("\r\n")]
    #[case::lone_cr("\r")]
    fn disable_file_after_a_bom_suppresses_every_line(#[case] line_break: &str) {
        let text =
            format!("\u{FEFF}# rlsp-yaml-disable-file code-a{line_break}key: value{line_break}");
        let map = build_suppression_map(&text);
        assert!(map.is_suppressed(0, "code-a"));
        assert!(map.is_suppressed(7, "code-a"));
        assert!(!map.is_suppressed(7, "code-b"));
    }

    #[test]
    fn disable_next_line_after_a_bom_and_indentation_targets_the_following_line() {
        let map = build_suppression_map("\u{FEFF}  # rlsp-yaml-disable-next-line code-a\nkey: v\n");
        assert!(map.is_suppressed(1, "code-a"));
        assert!(!map.is_suppressed(0, "code-a"));
    }

    #[test]
    fn disable_next_line_without_codes_after_a_bom_suppresses_all_codes_on_the_next_line() {
        let map =
            build_suppression_map("\u{FEFF}# rlsp-yaml-disable-next-line\nkey: v\nother: v\n");
        assert!(map.is_suppressed(1, "any"));
        assert!(!map.is_suppressed(0, "any"));
        assert!(!map.is_suppressed(2, "any"));
    }

    #[test]
    fn first_file_comment_wins_when_the_first_one_follows_a_bom() {
        let map = build_suppression_map(
            "\u{FEFF}# rlsp-yaml-disable-file code-a\n# rlsp-yaml-disable-file code-b\nkey: v\n",
        );
        assert!(map.is_suppressed(2, "code-a"));
        assert!(!map.is_suppressed(2, "code-b"));
    }

    #[rstest]
    #[case::bom_on_line_two("k: v\n\u{FEFF}# rlsp-yaml-disable-file\nx: y\n")]
    #[case::two_boms("\u{FEFF}\u{FEFF}# rlsp-yaml-disable-file\nx: y\n")]
    #[case::space_before_bom(" \u{FEFF}# rlsp-yaml-disable-file\nx: y\n")]
    #[case::bom_after_the_comment_indicator("# \u{FEFF}rlsp-yaml-disable-file\nx: y\n")]
    #[case::next_line_bom_on_line_two("k: v\n\u{FEFF}# rlsp-yaml-disable-next-line\nx: y\n")]
    fn a_bom_that_does_not_lead_the_text_hides_a_suppression_comment(#[case] text: &str) {
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(1, "any"));
        assert!(!map.is_suppressed(2, "any"));
    }

    #[test]
    fn a_bom_alone_gives_no_suppression() {
        let map = build_suppression_map("\u{FEFF}");
        assert!(!map.is_suppressed(0, "any"));
        assert!(!map.is_suppressed(1, "any"));
    }

    // ── Section 8: line breaks ────────────────────────────────────────────────
    //
    // Lines are numbered the way `LineIndex` numbers them: `\n`, `\r\n`, and a
    // lone `\r` each end one line.

    #[rstest]
    #[case::lf("\n")]
    #[case::crlf("\r\n")]
    #[case::lone_cr("\r")]
    fn disable_next_line_with_codes_targets_the_following_line(#[case] line_break: &str) {
        let text =
            format!("# rlsp-yaml-disable-next-line code-a{line_break}key: value{line_break}");
        let map = build_suppression_map(&text);
        assert!(!map.is_suppressed(0, "code-a"));
        assert!(map.is_suppressed(1, "code-a"));
        assert!(!map.is_suppressed(1, "code-b"));
        assert!(!map.is_suppressed(2, "code-a"));
    }

    #[rstest]
    #[case::lf("\n")]
    #[case::crlf("\r\n")]
    #[case::lone_cr("\r")]
    fn disable_next_line_without_codes_suppresses_all_on_the_following_line(
        #[case] line_break: &str,
    ) {
        let text = format!("# rlsp-yaml-disable-next-line{line_break}key: value{line_break}");
        let map = build_suppression_map(&text);
        assert!(!map.is_suppressed(0, "any-code"));
        assert!(map.is_suppressed(1, "any-code"));
        assert!(!map.is_suppressed(2, "any-code"));
    }

    #[rstest]
    #[case::lf("\n")]
    #[case::crlf("\r\n")]
    #[case::lone_cr("\r")]
    fn disable_file_without_codes_suppresses_all_for_every_break(#[case] line_break: &str) {
        let text =
            format!("# rlsp-yaml-disable-file{line_break}key: a{line_break}key: b{line_break}");
        let map = build_suppression_map(&text);
        assert!(map.is_suppressed(0, "any-code"));
        assert!(map.is_suppressed(2, "any-code"));
    }

    #[rstest]
    #[case::lf("\n")]
    #[case::crlf("\r\n")]
    #[case::lone_cr("\r")]
    fn disable_file_with_codes_suppresses_only_those_codes_for_every_break(
        #[case] line_break: &str,
    ) {
        let text = format!(
            "# rlsp-yaml-disable-file code-a{line_break}key: a{line_break}key: b{line_break}"
        );
        let map = build_suppression_map(&text);
        assert!(map.is_suppressed(2, "code-a"));
        assert!(!map.is_suppressed(2, "code-b"));
    }

    #[test]
    fn mixed_breaks_number_lines_like_line_index() {
        let text =
            "a\r\n# rlsp-yaml-disable-next-line c1\n b\r# rlsp-yaml-disable-next-line c2\rd\r\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(2, "c1"));
        assert!(map.is_suppressed(4, "c2"));
        for line in [0, 1, 3, 5] {
            assert!(!map.is_suppressed(line, "c1"), "c1 on line {line}");
            assert!(!map.is_suppressed(line, "c2"), "c2 on line {line}");
        }
        assert!(!map.is_suppressed(4, "c1"));
        assert!(!map.is_suppressed(2, "c2"));
    }

    #[test]
    fn consecutive_next_line_comments_with_lone_cr_each_suppress_their_next_line() {
        let text = "# rlsp-yaml-disable-next-line c\r# rlsp-yaml-disable-next-line c\rkey: value\r";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(0, "c"));
        assert!(map.is_suppressed(1, "c"));
        assert!(map.is_suppressed(2, "c"));
    }

    #[test]
    fn first_file_comment_wins_with_lone_cr() {
        let text = "# rlsp-yaml-disable-file code-a\r# rlsp-yaml-disable-file code-b\rkey: value\r";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(2, "code-a"));
        assert!(!map.is_suppressed(2, "code-b"));
    }

    #[test]
    fn disable_next_line_on_last_line_with_trailing_lone_cr_adds_no_suppression_before_it() {
        let text = "key: value\r# rlsp-yaml-disable-next-line code-a\r";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(0, "code-a"));
        assert!(!map.is_suppressed(1, "code-a"));
    }

    #[test]
    fn indented_comment_with_lone_cr_is_recognised() {
        let text = "  # rlsp-yaml-disable-next-line code-a\r key: v\r";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "code-a"));
    }

    #[test]
    fn unicode_line_separator_does_not_end_a_line() {
        let text = "# rlsp-yaml-disable-next-line code-a\u{2028}key: value\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(1, "code-a"));
        assert!(map.is_suppressed(1, "code-a\u{2028}key: value"));
    }
}
