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
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn indent_of(edit: &TextEdit) -> usize {
        edit.new_text.len()
    }

    // Test 1: After `key:` — indents by tab_size
    #[test]
    fn should_indent_after_bare_mapping_key() {
        let text = "key:\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 2);
    }

    // Test 2: After `key: value` — maintains same indent as previous line
    #[test]
    fn should_maintain_indent_after_complete_key_value_pair() {
        let text = "key: value\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 0);
    }

    // Test 3: After `  key:` (indented key) — indents to previous indent + tab_size
    #[test]
    fn should_add_extra_indent_after_indented_bare_key() {
        let text = "  key:\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 4);
    }

    // Test 4: After `- item` — maintains same indent
    #[test]
    fn should_maintain_indent_after_sequence_item() {
        let text = "- item\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 0);
    }

    // Test 5: After `key: |` — indents by tab_size
    #[test]
    fn should_indent_after_literal_block_scalar() {
        let text = "key: |\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 2);
    }

    // Test 6: After `key: >` — indents by tab_size
    #[test]
    fn should_indent_after_folded_block_scalar() {
        let text = "key: >\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 2);
    }

    // Test 7: After `key: |-` — indents by tab_size
    #[test]
    fn should_indent_after_block_scalar_with_strip_chomping() {
        let text = "key: |-\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 2);
    }

    // Test 7b: After `key: >-` — indents by tab_size
    #[test]
    fn should_indent_after_folded_block_scalar_with_strip_chomping() {
        let text = "key: >-\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 2);
    }

    // Test 7c: After `key: |+` — indents by tab_size
    #[test]
    fn should_indent_after_block_scalar_with_keep_chomping() {
        let text = "key: |+\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 2);
    }

    // Test 8: Non-newline character — empty vec
    #[test]
    fn should_return_empty_for_non_newline_character() {
        let text = "key:\n";
        let edits = format_on_type(text, pos(1, 0), "a", 2);

        assert!(edits.is_empty());
    }

    // Test 9: Position at line 0 — empty vec
    #[test]
    fn should_return_empty_when_position_is_line_zero() {
        let text = "key: value\n";
        let edits = format_on_type(text, pos(0, 0), "\n", 2);

        assert!(edits.is_empty());
    }

    // Test 10: Empty text — empty vec (line 1 exists but prev line is empty)
    #[test]
    fn should_return_empty_for_empty_text() {
        let edits = format_on_type("", pos(0, 0), "\n", 2);

        assert!(edits.is_empty());
    }

    // Test 11: Different tab_size values work correctly
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

    // Test 12: TextEdit range covers col 0 to cursor on the new line
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

    // Test 13: Empty previous line falls back to nearest non-empty line above
    #[test]
    fn should_use_nearest_non_empty_line_when_prev_is_empty() {
        // line 0: "key:" (indent 0, ends with :)
        // line 1: "" (empty)
        // line 2: new line (position)
        let text = "key:\n\n";
        let edits = format_on_type(text, pos(2, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        // Falls back to "key:" → indent 0 + 2 = 2
        assert_eq!(indent_of(&edits[0]), 2);
    }

    // Test 14: Comment line — maintain same indent
    #[test]
    fn should_maintain_indent_after_comment_line() {
        let text = "  # a comment\n";
        let edits = format_on_type(text, pos(1, 0), "\n", 2);

        assert_eq!(edits.len(), 1);
        assert_eq!(indent_of(&edits[0]), 2);
    }
}
