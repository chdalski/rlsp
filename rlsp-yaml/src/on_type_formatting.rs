// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{Position, Range, TextEdit};

/// Compute text edits for on-type formatting when a newline is typed.
///
/// Returns indentation edits for the new line based on the context of the
/// previous line. Only handles `ch == "\n"` — returns an empty vec for
/// anything else.
#[must_use]
pub fn format_on_type(text: &str, position: Position, ch: &str, tab_size: u32) -> Vec<TextEdit> {
    if ch != "\n" {
        return Vec::new();
    }

    if position.line == 0 {
        return Vec::new();
    }

    let tab_size = if tab_size == 0 { 2 } else { tab_size };

    let lines: Vec<&str> = text.lines().collect();

    let prev_line_idx = (position.line - 1) as usize;
    let prev_line = find_prev_non_empty_line(&lines, prev_line_idx);

    let prev_indent = leading_spaces(prev_line);
    let prev_trimmed = prev_line.trim_end();

    let indent_level = if needs_extra_indent(prev_trimmed) {
        prev_indent + tab_size as usize
    } else {
        prev_indent
    };

    vec![TextEdit {
        range: Range::new(
            Position::new(position.line, 0),
            Position::new(position.line, position.character),
        ),
        new_text: " ".repeat(indent_level),
    }]
}

/// Find the most recent non-empty line at or before `idx`.
///
/// Falls back to the empty string if all lines above are empty.
fn find_prev_non_empty_line<'a>(lines: &[&'a str], idx: usize) -> &'a str {
    lines
        .get(..=idx)
        .unwrap_or(lines)
        .iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .copied()
        .unwrap_or("")
}

/// Count leading spaces on a line.
fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Determine whether a line (already right-trimmed) should cause the next
/// line to be indented one level deeper.
///
/// Returns `true` for:
/// - Lines ending with `:` (mapping key expecting a block value)
/// - Lines ending with `|`, `>`, `|-`, `>-`, `|+`, `>+` (block scalars)
fn needs_extra_indent(trimmed: &str) -> bool {
    if trimmed.ends_with(':') {
        return true;
    }

    // Check the value portion after a mapping colon (e.g., `key: |`)
    if let Some(colon_pos) = find_mapping_colon(trimmed) {
        let after_colon = trimmed[colon_pos + 1..].trim();
        if is_block_scalar_indicator(after_colon) {
            return true;
        }
    }

    false
}

/// Return `true` when the string is a block scalar indicator:
/// `|` or `>` optionally followed by chomping (`+`, `-`) only.
fn is_block_scalar_indicator(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some('|' | '>') => {}
        _ => return false,
    }
    // Only chomping modifiers are allowed after the indicator character.
    match chars.next() {
        None => true,
        Some('+' | '-') => chars.next().is_none(),
        _ => false,
    }
}

/// Find the byte position of the mapping colon in a YAML line.
///
/// A mapping colon is `:` followed by a space, tab, or end-of-string,
/// and not inside a quoted string.
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

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn indent_of(edit: &TextEdit) -> usize {
        edit.new_text.len()
    }

    // Group: format_on_type_produces_indent — single edit, assert indent size
    #[rstest]
    #[case::bare_mapping_key("key:\n", pos(1, 0), "\n", 2, 2)]
    #[case::complete_key_value_pair("key: value\n", pos(1, 0), "\n", 2, 0)]
    #[case::indented_bare_key("  key:\n", pos(1, 0), "\n", 2, 4)]
    #[case::sequence_item("- item\n", pos(1, 0), "\n", 2, 0)]
    #[case::literal_block_scalar("key: |\n", pos(1, 0), "\n", 2, 2)]
    #[case::folded_block_scalar("key: >\n", pos(1, 0), "\n", 2, 2)]
    #[case::block_scalar_strip_chomping("key: |-\n", pos(1, 0), "\n", 2, 2)]
    #[case::folded_block_scalar_strip_chomping("key: >-\n", pos(1, 0), "\n", 2, 2)]
    #[case::block_scalar_keep_chomping("key: |+\n", pos(1, 0), "\n", 2, 2)]
    #[case::comment_line_maintains_indent("  # a comment\n", pos(1, 0), "\n", 2, 2)]
    #[case::empty_prev_line_fallback("key:\n\n", pos(2, 0), "\n", 2, 2)]
    fn format_on_type_produces_indent(
        #[case] text: &str,
        #[case] position: Position,
        #[case] ch: &str,
        #[case] tab_size: u32,
        #[case] expected_indent: usize,
    ) {
        let edits = format_on_type(text, position, ch, tab_size);
        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), expected_indent);
    }

    // Group: format_on_type_returns_empty — assert edits.is_empty()
    #[rstest]
    #[case::non_newline_character("key:\n", pos(1, 0), "a", 2)]
    #[case::position_at_line_zero("key: value\n", pos(0, 0), "\n", 2)]
    #[case::empty_text("", pos(0, 0), "\n", 2)]
    fn format_on_type_returns_empty(
        #[case] text: &str,
        #[case] position: Position,
        #[case] ch: &str,
        #[case] tab_size: u32,
    ) {
        let edits = format_on_type(text, position, ch, tab_size);
        assert!(edits.is_empty());
    }

    // Different tab_size values work correctly
    #[test]
    fn should_respect_tab_size_parameter() {
        let text = "key:\n";

        let edits_4 = format_on_type(text, pos(1, 0), "\n", 4);
        assert_eq!(edits_4.len(), 1);
        assert_eq!(indent_of(&edits_4[0]), 4);

        let edits_0 = format_on_type(text, pos(1, 0), "\n", 0);
        assert_eq!(edits_0.len(), 1);
        // tab_size 0 treated as 2
        assert_eq!(indent_of(&edits_0[0]), 2);
    }

    // TextEdit range covers col 0 to cursor on the new line
    #[test]
    fn edit_range_replaces_existing_characters_on_new_line() {
        // Simulate the cursor at column 3 (some pre-existing chars on the line)
        let text = "key:\n   \n";
        let edits = format_on_type(text, pos(1, 3), "\n", 2);

        assert_eq!(edits.len(), 1);
        let edit = &edits[0];
        assert_eq!(edit.range.start, Position::new(1, 0));
        assert_eq!(edit.range.end, Position::new(1, 3));
    }
}
