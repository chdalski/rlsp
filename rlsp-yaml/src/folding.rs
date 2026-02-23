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
    for region in stack.into_iter().rev() {
        let end = find_last_content_line(lines, region.start_line, total);
        if end > region.start_line {
            push_fold(ranges, region.start_line, end, None);
        }
    }
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
    let mut last = start;
    for i in (start + 1)..before {
        if let Some(line) = lines.get(i) {
            let trimmed = line.trim();
            if !trimmed.is_empty() && trimmed != "---" && trimmed != "..." {
                last = i;
            }
        }
    }
    last
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
    if separator_positions[0] > 0 {
        let end = find_last_content_line_in_range(lines, 0, separator_positions[0]);
        if let Some(end) = end
            && end > 0
        {
            push_fold(ranges, 0, end, Some(FoldingRangeKind::Region));
        }
    }

    // Sections between separators
    for window in separator_positions.windows(2) {
        let start = window[0] + 1;
        let before = window[1];
        if start < before {
            let end = find_last_content_line_in_range(lines, start, before);
            if let Some(end) = end
                && end > start
            {
                push_fold(ranges, start, end, Some(FoldingRangeKind::Region));
            }
        }
    }

    // Last section: from after last separator to end
    let last_sep = separator_positions[separator_positions.len() - 1];
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
    let mut last = None;
    for i in from..before {
        if let Some(line) = lines.get(i) {
            let trimmed = line.trim();
            if !trimmed.is_empty() && trimmed != "---" && trimmed != "..." {
                last = Some(i);
            }
        }
    }
    last
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
mod tests {
    use super::*;
    use tower_lsp::lsp_types::FoldingRangeKind;

    fn ranges_as_tuples(ranges: &[FoldingRange]) -> Vec<(u32, u32)> {
        ranges.iter().map(|r| (r.start_line, r.end_line)).collect()
    }

    // ---- Mappings ----

    // Test 1
    #[test]
    fn should_fold_mapping_with_nested_content() {
        let text = "server:\n  host: localhost\n  port: 8080\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 2)),
            "should fold server mapping from line 0 to 2, got: {tuples:?}"
        );
    }

    // Test 2
    #[test]
    fn should_not_fold_single_line_mapping() {
        let text = "key: value\n";
        let result = folding_ranges(text);

        assert!(result.is_empty(), "should not fold single-line mapping");
    }

    // Test 3
    #[test]
    fn should_fold_multiple_top_level_mappings() {
        let text =
            "server:\n  host: localhost\n  port: 8080\ndatabase:\n  name: mydb\n  port: 5432\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 2)),
            "should fold server mapping (lines 0-2), got: {tuples:?}"
        );
        assert!(
            tuples.contains(&(3, 5)),
            "should fold database mapping (lines 3-5), got: {tuples:?}"
        );
    }

    // Test 4
    #[test]
    fn should_fold_deeply_nested_mappings() {
        let text = "a:\n  b:\n    c:\n      d: value\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 3)),
            "should fold 'a' (lines 0-3), got: {tuples:?}"
        );
        assert!(
            tuples.contains(&(1, 3)),
            "should fold 'b' (lines 1-3), got: {tuples:?}"
        );
        assert!(
            tuples.contains(&(2, 3)),
            "should fold 'c' (lines 2-3), got: {tuples:?}"
        );
    }

    // Test 5
    #[test]
    fn should_not_fold_mapping_with_inline_value_only() {
        let text = "name: Alice\nage: 30\n";
        let result = folding_ranges(text);

        assert!(
            result.is_empty(),
            "should not fold flat key-value pairs with no nesting"
        );
    }

    // ---- Sequences ----

    // Test 6
    #[test]
    fn should_fold_sequence() {
        let text = "items:\n  - one\n  - two\n  - three\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 3)),
            "should fold items sequence (lines 0-3), got: {tuples:?}"
        );
    }

    // Test 7
    #[test]
    fn should_fold_sequence_of_mappings() {
        let text = "users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 4)),
            "should fold users sequence (lines 0-4), got: {tuples:?}"
        );
    }

    // ---- Block Scalars ----

    // Test 8
    #[test]
    fn should_fold_literal_block_scalar() {
        let text = "description: |\n  This is a\n  multi-line\n  description\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 3)),
            "should fold literal block scalar (lines 0-3), got: {tuples:?}"
        );
    }

    // Test 9
    #[test]
    fn should_fold_folded_block_scalar() {
        let text = "summary: >\n  This is a\n  folded\n  paragraph\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 3)),
            "should fold folded block scalar (lines 0-3), got: {tuples:?}"
        );
    }

    // Test 10
    #[test]
    fn should_not_treat_gt_or_pipe_in_value_as_block_scalar() {
        let text = "condition: a > b\nresult: true\n";
        let result = folding_ranges(text);

        assert!(
            result.is_empty(),
            "should not fold -- '>' in 'a > b' is not a block scalar indicator"
        );
    }

    // ---- Multi-Document Sections ----

    // Test 11
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

    // Test 12
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

    // Test 13
    #[test]
    fn should_not_break_fold_region_for_comment_lines() {
        let text = "server:\n  # This is a comment\n  host: localhost\n  port: 8080\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 3)),
            "should fold server mapping (lines 0-3) including comment, got: {tuples:?}"
        );
    }

    // Test 14
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

    // Test 15
    #[test]
    fn should_return_empty_for_empty_document() {
        let text = "";
        let result = folding_ranges(text);

        assert!(result.is_empty(), "should return empty for empty document");
    }

    // Test 16
    #[test]
    fn should_return_empty_for_single_line_document() {
        let text = "key: value";
        let result = folding_ranges(text);

        assert!(
            result.is_empty(),
            "should return empty for single-line document"
        );
    }

    // Test 17
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

    // Test 18
    #[test]
    fn should_handle_blank_lines_within_fold_region() {
        let text = "server:\n  host: localhost\n\n  port: 8080\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 3)),
            "should fold server mapping (lines 0-3) across blank line, got: {tuples:?}"
        );
    }

    // Test 19
    #[test]
    fn should_handle_mixed_content_types() {
        let text = "config:\n  name: app\n  ports:\n    - 80\n    - 443\n  description: |\n    A multi-line\n    description\n";
        let result = folding_ranges(text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 7)),
            "should fold config mapping (lines 0-7), got: {tuples:?}"
        );
    }
}
