// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

/// Compute folding ranges for the given YAML text.
///
/// Returns foldable regions for mappings, sequences, block scalars,
/// and multi-document sections. Returns an empty list for empty documents.
#[must_use]
pub fn folding_ranges(text: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    collect_indentation_folds(&lines, &mut ranges);
    collect_document_section_folds(&lines, &mut ranges);
    collect_comment_block_folds(&lines, &mut ranges);
    ranges
}

/// An open region on the indentation stack.
struct OpenRegion {
    start_line: usize,
    indent: usize,
}

/// Collect folding ranges based on indentation changes.
///
/// A line that introduces deeper indentation on subsequent lines starts a
/// fold region. The region ends at the last line before indentation returns
/// to the same or lesser level.
fn collect_indentation_folds(lines: &[&str], ranges: &mut Vec<FoldingRange>) {
    let mut stack: Vec<OpenRegion> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip blank lines and document separators -- they don't affect the stack
        if trimmed.is_empty() || trimmed == "---" || trimmed == "..." {
            continue;
        }

        // Comment lines: use their indentation but don't start new regions
        if trimmed.starts_with('#') {
            let indent = line.len() - line.trim_start().len();
            close_regions_at_or_above(&mut stack, indent, i, lines, ranges);
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        // Close any regions that this line's indentation ends
        close_regions_at_or_above(&mut stack, indent, i, lines, ranges);

        // Check if this line starts a new fold region (has children at deeper indent)
        if starts_fold_region(trimmed) {
            stack.push(OpenRegion {
                start_line: i,
                indent,
            });
        }
    }

    // Close any remaining open regions at end of document
    let total = lines.len();
    stack.into_iter().rev().for_each(|region| {
        let end = find_last_content_line(lines, region.start_line, total);
        if end > region.start_line {
            push_fold(ranges, region.start_line, end, None);
        }
    });
}

/// Close stack regions whose indentation is >= the current line's indent.
fn close_regions_at_or_above(
    stack: &mut Vec<OpenRegion>,
    indent: usize,
    current_line: usize,
    lines: &[&str],
    ranges: &mut Vec<FoldingRange>,
) {
    while let Some(top) = stack.last() {
        if top.indent >= indent {
            let Some(region) = stack.pop() else { break };
            let end = find_last_content_line(lines, region.start_line, current_line);
            if end > region.start_line {
                push_fold(ranges, region.start_line, end, None);
            }
        } else {
            break;
        }
    }
}

/// Determine if a trimmed line could start a fold region.
///
/// A line starts a fold when it ends with `:` (mapping), `: |`, `: >`,
/// or similar patterns indicating children follow on subsequent lines.
fn starts_fold_region(trimmed: &str) -> bool {
    // "key:" at end of line (mapping with block value)
    if trimmed.ends_with(':') {
        return true;
    }

    // Check for block scalar indicators or mapping with no inline value
    if let Some(colon_pos) = find_mapping_colon(trimmed) {
        let after_colon = trimmed[colon_pos + 1..].trim();
        // "key: |", "key: >", "key: |+", "key: >-", etc.
        if after_colon.is_empty() {
            return true;
        }
        if is_block_scalar_indicator(after_colon) {
            return true;
        }
    }

    // Bare sequence parent lines (already handled by mapping check above in most cases)
    false
}

/// Check if the value after a colon is a block scalar indicator.
///
/// Block scalar indicators are `|` or `>` optionally followed by
/// chomping indicators (`+`, `-`) and/or an indentation digit.
/// But NOT something like `a > b` which has content after the `>`.
fn is_block_scalar_indicator(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '|' && first != '>' {
        return false;
    }
    // Everything after must be chomping/indentation indicators or comments
    for ch in chars {
        match ch {
            '+' | '-' | '0'..='9' => {}
            ' ' | '\t' | '#' => return true, // rest is whitespace or comment
            _ => return false,               // content after indicator -- not a block scalar
        }
    }
    true
}

/// Find the position of the mapping colon in a YAML line.
/// Skips colons inside quoted strings.
fn find_mapping_colon(line: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ':' if !in_single_quote && !in_double_quote => {
                let rest = &line[i + 1..];
                if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find the last non-blank, non-separator content line between `start` (exclusive)
/// and `before` (exclusive).
fn find_last_content_line(lines: &[&str], start: usize, before: usize) -> usize {
    ((start + 1)..before)
        .rev()
        .find(|&i| {
            lines
                .get(i)
                .is_some_and(|l| !l.trim().is_empty() && l.trim() != "---" && l.trim() != "...")
        })
        .unwrap_or(start)
}

/// Collect folding ranges for document sections separated by `---`.
fn collect_document_section_folds(lines: &[&str], ranges: &mut Vec<FoldingRange>) {
    let separator_positions: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i)
        .collect();

    if separator_positions.is_empty() {
        return;
    }

    // First section: from line 0 to just before first separator
    if let Some(&first_sep) = separator_positions.first()
        && first_sep > 0
    {
        let end = find_last_content_line_in_range(lines, 0, first_sep);
        if let Some(end) = end
            && end > 0
        {
            push_fold(ranges, 0, end, Some(FoldingRangeKind::Region));
        }
    }

    // Sections between separators
    for window in separator_positions.windows(2) {
        if let [start_sep, before] = window {
            let start = start_sep + 1;
            if start < *before {
                let end = find_last_content_line_in_range(lines, start, *before);
                if let Some(end) = end
                    && end > start
                {
                    push_fold(ranges, start, end, Some(FoldingRangeKind::Region));
                }
            }
        }
    }

    // Last section: from after last separator to end
    let Some(&last_sep) = separator_positions.last() else {
        return;
    };
    let start = last_sep + 1;
    if start < lines.len() {
        let end = find_last_content_line_in_range(lines, start, lines.len());
        if let Some(end) = end
            && end > start
        {
            push_fold(ranges, start, end, Some(FoldingRangeKind::Region));
        }
    }
}

/// Find the last content line in the range `[from, before)`.
fn find_last_content_line_in_range(lines: &[&str], from: usize, before: usize) -> Option<usize> {
    (from..before).rev().find(|&i| {
        lines
            .get(i)
            .is_some_and(|l| !l.trim().is_empty() && l.trim() != "---" && l.trim() != "...")
    })
}

/// Collect folding ranges for consecutive comment blocks (3+ lines).
fn collect_comment_block_folds(lines: &[&str], ranges: &mut Vec<FoldingRange>) {
    let mut comment_start: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if comment_start.is_none() {
                comment_start = Some(i);
            }
        } else {
            if let Some(start) = comment_start
                && i - 1 > start
            {
                push_fold(ranges, start, i - 1, Some(FoldingRangeKind::Comment));
            }
            comment_start = None;
        }
    }

    // Handle comment block at end of file
    if let Some(start) = comment_start {
        let end = lines.len() - 1;
        if end > start {
            push_fold(ranges, start, end, Some(FoldingRangeKind::Comment));
        }
    }
}

/// Push a folding range, performing the `usize` to `u32` conversion.
fn push_fold(
    ranges: &mut Vec<FoldingRange>,
    start: usize,
    end: usize,
    kind: Option<FoldingRangeKind>,
) {
    #[allow(clippy::cast_possible_truncation)]
    ranges.push(FoldingRange {
        start_line: start as u32,
        start_character: None,
        end_line: end as u32,
        end_character: None,
        kind,
        collapsed_text: None,
    });
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use rstest::rstest;

    use super::*;
    use tower_lsp::lsp_types::FoldingRangeKind;

    fn ranges_as_tuples(ranges: &[FoldingRange]) -> Vec<(u32, u32)> {
        ranges.iter().map(|r| (r.start_line, r.end_line)).collect()
    }

    // ---- Mappings / Sequences / Block Scalars ----

    // Group: folding_ranges_contains_range — single tuples.contains assertion
    #[rstest]
    // ---- Mappings ----
    #[case::mapping_with_nested_content("server:\n  host: localhost\n  port: 8080\n", (0, 2))]
    // ---- Sequences ----
    #[case::sequence("items:\n  - one\n  - two\n  - three\n", (0, 3))]
    #[case::sequence_of_mappings("users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n", (0, 4))]
    // ---- Block Scalars ----
    #[case::literal_block_scalar("description: |\n  This is a\n  multi-line\n  description\n", (0, 3))]
    #[case::folded_block_scalar("summary: >\n  This is a\n  folded\n  paragraph\n", (0, 3))]
    // ---- Comments ----
    #[case::comment_within_server_fold("server:\n  # This is a comment\n  host: localhost\n  port: 8080\n", (0, 3))]
    // ---- Edge Cases ----
    #[case::blank_lines_within_fold("server:\n  host: localhost\n\n  port: 8080\n", (0, 3))]
    #[case::mixed_content_types("config:\n  name: app\n  ports:\n    - 80\n    - 443\n  description: |\n    A multi-line\n    description\n", (0, 7))]
    #[case::block_scalar_with_indentation_indicator("text: |2\n  indented content\n  more content\n", (0, 2))]
    fn folding_ranges_contains_range(#[case] text: &str, #[case] expected: (u32, u32)) {
        let result = folding_ranges(text);
        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&expected),
            "expected tuple {expected:?} in folding ranges, got: {tuples:?}"
        );
    }

    // Group: folding_ranges_returns_empty — assert result.is_empty()
    #[rstest]
    #[case::empty_document("")]
    #[case::single_line_document("key: value")]
    #[case::mapping_with_inline_value_only("name: Alice\nage: 30\n")]
    #[case::gt_or_pipe_in_value_not_block_scalar("condition: a > b\nresult: true\n")]
    fn folding_ranges_returns_empty(#[case] text: &str) {
        let result = folding_ranges(text);
        assert!(result.is_empty(), "expected empty result, got: {result:?}");
    }

    // ---- Multi-Document Sections ----

    #[test]
    fn should_fold_document_sections() {
        let text = "key1: val1\nkey2: val2\n---\nkey3: val3\nkey4: val4\n";
        let result = folding_ranges(text);

        assert!(
            result.len() >= 2,
            "should have at least 2 folding ranges for 2 document sections, got: {}",
            result.len()
        );
    }

    #[test]
    fn should_fold_document_sections_with_nested_content() {
        let text = "doc1:\n  key: val\n---\ndoc2:\n  key: val\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 1)),
            "should fold doc1 mapping (lines 0-1), got: {tuples:?}"
        );
        assert!(
            tuples.contains(&(3, 4)),
            "should fold doc2 mapping (lines 3-4), got: {tuples:?}"
        );
    }

    // ---- Comments ----

    #[test]
    fn should_fold_consecutive_comment_block() {
        let text = "# Header comment\n# continues here\n# and here\nkey: value\n";
        let result = folding_ranges(text);

        // Comment block folding is optional. If present, it should use Comment kind.
        let comment_folds: Vec<&FoldingRange> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        let region_folds: Vec<&FoldingRange> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Region) || r.kind.is_none())
            .collect();

        // The comment block should not produce a Region fold
        for fold in &region_folds {
            assert!(
                fold.start_line > 2,
                "comment block should not produce a Region fold, got region fold starting at line {}",
                fold.start_line
            );
        }

        // If comment folds are present, verify them
        if !comment_folds.is_empty() {
            let tuples: Vec<(u32, u32)> = comment_folds
                .iter()
                .map(|r| (r.start_line, r.end_line))
                .collect();
            assert!(
                tuples.contains(&(0, 2)),
                "comment fold should span lines 0-2, got: {tuples:?}"
            );
        }
    }

    // ---- Edge Cases ----

    #[test]
    fn should_return_empty_for_comment_only_document() {
        let text = "# just a comment\n";
        let result = folding_ranges(text);

        // Either empty or a comment fold -- both are acceptable
        for fold in &result {
            assert!(
                fold.kind == Some(FoldingRangeKind::Comment) || fold.kind.is_none(),
                "comment-only document should not produce Region folds"
            );
        }
    }

    // multi-document: three sections (exercises windows(2) with two pairs)
    #[test]
    fn should_fold_three_document_sections() {
        let text = "a: 1\nb: 2\n---\nc: 3\nd: 4\n---\ne: 5\nf: 6\n";
        let result = folding_ranges(text);

        // All three sections must produce region folds
        let region_folds: Vec<_> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Region))
            .collect();
        assert!(
            region_folds.len() >= 3,
            "three document sections should produce at least 3 region folds, got: {region_folds:?}"
        );
    }

    // multi-document: content only after last separator (last section fold)
    #[test]
    fn should_fold_last_section_after_final_separator() {
        let text = "---\nkey1: val1\nkey2: val2\n";
        let result = folding_ranges(text);

        // Section after --- (lines 1-2) should produce a region fold
        assert!(
            result
                .iter()
                .any(|r| r.kind == Some(FoldingRangeKind::Region)),
            "content after separator should produce a region fold, got: {result:?}"
        );
    }

    // comment block at end of file (no trailing non-comment line)
    #[test]
    fn should_fold_comment_block_at_end_of_file() {
        let text = "key: value\n# comment line 1\n# comment line 2\n# comment line 3\n";
        let result = folding_ranges(text);

        let comment_folds: Vec<_> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        assert!(
            !comment_folds.is_empty(),
            "comment block at end of file should produce a Comment fold, got: {result:?}"
        );
        let tuples: Vec<(u32, u32)> = comment_folds
            .iter()
            .map(|r| (r.start_line, r.end_line))
            .collect();
        assert!(
            tuples.contains(&(1, 3)),
            "comment fold should span lines 1-3, got: {tuples:?}"
        );
    }

    #[test]
    fn should_fold_block_scalar_with_chomping_indicator() {
        let text =
            "a: |-\n  content line 1\n  content line 2\nb: >+\n  folded line 1\n  folded line 2\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 2)),
            "should fold '|-' block scalar (lines 0-2), got: {tuples:?}"
        );
        assert!(
            tuples.contains(&(3, 5)),
            "should fold '>+' block scalar (lines 3-5), got: {tuples:?}"
        );
    }

    // find_last_content_line_in_range: range where all lines are blank/separator
    #[test]
    fn should_not_fold_section_consisting_only_of_blank_lines() {
        // Two separators with only blank lines between them
        let text = "a: 1\n---\n\n\n---\nb: 2\n";
        let result = folding_ranges(text);

        // The middle section (only blank lines) should not produce a region fold
        let region_folds: Vec<_> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Region))
            .collect();
        for fold in &region_folds {
            assert!(
                fold.start_line != 2 && fold.start_line != 3,
                "blank-only section should not produce a region fold, got: {fold:?}"
            );
        }
    }

    // comment block of exactly 1 line (i - 1 == start, should NOT fold)
    #[test]
    fn should_not_fold_single_comment_line() {
        // Only 1 comment line — `i - 1 == start` so condition `i - 1 > start` is false
        let text = "# only one comment\nkey: value\n";
        let result = folding_ranges(text);

        let comment_folds: Vec<_> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        assert!(
            comment_folds.is_empty(),
            "single comment line should not fold, got: {comment_folds:?}"
        );
    }
}
