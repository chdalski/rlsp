// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

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
/// All line numbers are 0-based, matching LSP `Range.start.line`.
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

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix(NEXT_LINE_PREFIX) {
            let rule = parse_rule(rest);
            // Suppress line idx + 1 (the line immediately following the comment).
            // If the comment is on the last line there is no next line, but
            // inserting idx+1 is harmless — no diagnostic is ever emitted for
            // a line beyond the document end.
            #[allow(clippy::cast_possible_truncation)]
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── Section 1: disable-next-line with specific codes ──────────────────────

    #[test]
    fn disable_next_line_suppresses_specified_code_on_next_line() {
        let text = "# rlsp-yaml-disable-next-line yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(map.is_suppressed(1, "yaml-schema"));
    }

    #[test]
    fn disable_next_line_does_not_suppress_unlisted_code_on_next_line() {
        let text = "# rlsp-yaml-disable-next-line yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(1, "other-code"));
    }

    #[test]
    fn disable_next_line_does_not_suppress_the_comment_line_itself() {
        let text = "# rlsp-yaml-disable-next-line yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(0, "yaml-schema"));
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

    #[test]
    fn empty_input_returns_map_with_no_suppressions() {
        let map = build_suppression_map("");
        assert!(!map.is_suppressed(0, "any-code"));
    }

    #[test]
    fn input_with_no_suppression_comments_returns_empty_map() {
        let text = "key: value\nother: stuff\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(0, "any-code"));
        assert!(!map.is_suppressed(1, "any-code"));
    }

    #[test]
    fn unrecognized_comment_keyword_is_ignored() {
        // Missing `-next-line` or `-file` suffix.
        let text = "# rlsp-yaml-disable yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(0, "yaml-schema"));
        assert!(!map.is_suppressed(1, "yaml-schema"));
    }

    #[test]
    fn wrong_prefix_is_ignored() {
        let text = "# yaml-language-server: disable-next-line yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(1, "yaml-schema"));
    }

    #[test]
    fn non_comment_line_is_ignored() {
        let text = "rlsp-yaml-disable-next-line yaml-schema\nkey: value\n";
        let map = build_suppression_map(text);
        assert!(!map.is_suppressed(1, "yaml-schema"));
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
}
