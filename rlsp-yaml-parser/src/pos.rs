// SPDX-License-Identifier: MIT

/// A position within the input stream.
///
/// `line` is 1-based; `column` is 0-based (codepoints from the start of the line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub byte_offset: usize,
    pub line: usize,
    pub column: usize,
}

impl Pos {
    /// The position representing the start of a document.
    pub const ORIGIN: Self = Self {
        byte_offset: 0,
        line: 1,
        column: 0,
    };

    /// Advance the position by one character.
    ///
    /// If `ch` is a line feed (`\n`) the line counter is incremented and the
    /// column is reset to 0.  For all other characters the column advances by
    /// one.  `byte_offset` advances by `ch.len_utf8()`.
    #[must_use]
    pub const fn advance(self, ch: char) -> Self {
        let byte_offset = self.byte_offset + ch.len_utf8();
        if ch == '\n' {
            Self {
                byte_offset,
                line: self.line + 1,
                column: 0,
            }
        } else {
            Self {
                byte_offset,
                line: self.line,
                column: self.column + 1,
            }
        }
    }
}

/// Compute the 0-based column (codepoint count) for a position within a line.
///
/// `byte_offset_in_line` must be a valid byte-boundary index into `line_content`.
/// Uses an ASCII fast path: if the prefix is pure ASCII, the column equals the
/// byte offset (1 byte = 1 codepoint).
pub fn column_at(line_content: &str, byte_offset_in_line: usize) -> usize {
    let prefix = &line_content[..byte_offset_in_line];
    if prefix.is_ascii() {
        byte_offset_in_line
    } else {
        prefix.chars().count()
    }
}

/// Advance `pos` past `content`, assuming `content` contains no line break.
/// Uses the ASCII fast path in [`column_at`].
pub fn advance_within_line(pos: Pos, content: &str) -> Pos {
    Pos {
        byte_offset: pos.byte_offset + content.len(),
        line: pos.line,
        column: pos.column + column_at(content, content.len()),
    }
}

/// A half-open span `[start, end)` within the input stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos_origin_is_start_of_document() {
        let pos = Pos::ORIGIN;
        assert_eq!(pos.byte_offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 0);
    }

    #[test]
    fn pos_fields_are_accessible() {
        let pos = Pos {
            byte_offset: 10,
            line: 3,
            column: 4,
        };
        assert_eq!(pos.byte_offset, 10);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 4);
    }

    #[test]
    fn pos_is_copy() {
        let pos = Pos::ORIGIN;
        let pos2 = pos;
        let _ = pos.byte_offset;
        let _ = pos2.byte_offset;
    }

    #[test]
    fn span_is_copy() {
        let span = Span {
            start: Pos::ORIGIN,
            end: Pos::ORIGIN,
        };
        let span2 = span;
        let _ = span.start;
        let _ = span2.start;
    }

    #[test]
    fn advance_ascii_increments_byte_and_column() {
        let pos = Pos::ORIGIN.advance('a');
        assert_eq!(pos.byte_offset, 1);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn advance_newline_increments_line_and_resets_column() {
        let pos = Pos::ORIGIN.advance('a').advance('\n');
        assert_eq!(pos.byte_offset, 2);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 0);
    }

    #[test]
    fn advance_multibyte_char_increments_byte_offset_by_utf8_len() {
        // '中' is 3 bytes in UTF-8
        let pos = Pos::ORIGIN.advance('中');
        assert_eq!(pos.byte_offset, 3);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn advance_multiple_lines() {
        let pos = Pos::ORIGIN
            .advance('a')
            .advance('\n')
            .advance('b')
            .advance('\n')
            .advance('c');
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    // -----------------------------------------------------------------------
    // column_at
    // -----------------------------------------------------------------------

    #[test]
    fn column_at_empty_prefix_is_zero() {
        assert_eq!(column_at("hello", 0), 0);
    }

    #[test]
    fn column_at_ascii_only_line_returns_byte_offset() {
        assert_eq!(column_at("hello world", 5), 5);
    }

    #[test]
    fn column_at_ascii_full_line_returns_byte_len() {
        assert_eq!(column_at("abc", 3), 3);
    }

    #[test]
    fn column_at_multibyte_prefix_counts_chars() {
        // "日本語xyz": 日本語 = 9 bytes / 3 chars
        assert_eq!(column_at("日本語xyz", 9), 3);
    }

    #[test]
    fn column_at_mixed_prefix_ascii_then_multibyte() {
        // "ab日本": ab = 2 bytes, 日本 = 6 bytes; prefix = 8 bytes = 4 chars
        assert_eq!(column_at("ab日本", 8), 4);
    }

    #[test]
    fn column_at_multibyte_then_ascii() {
        // "日ab": 日 = 3 bytes, ab = 2 bytes; prefix = first 5 bytes = "日ab" = 3 chars
        assert_eq!(column_at("日ab", 5), 3);
    }

    #[test]
    fn column_at_full_multibyte_line() {
        // "日本語" = 9 bytes / 3 chars; prefix = entire string
        assert_eq!(column_at("日本語", 9), 3);
    }

    // -----------------------------------------------------------------------
    // advance_within_line
    // -----------------------------------------------------------------------

    #[test]
    fn advance_within_line_empty_content_returns_pos_unchanged() {
        let pos = Pos {
            byte_offset: 5,
            line: 2,
            column: 3,
        };
        assert_eq!(advance_within_line(pos, ""), pos);
    }

    #[test]
    fn advance_within_line_ascii_only_advances_byte_and_column() {
        let result = advance_within_line(Pos::ORIGIN, "hello");
        assert_eq!(result.byte_offset, 5);
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 5);
    }

    #[test]
    fn advance_within_line_ascii_mid_line_accumulates_correctly() {
        let pos = Pos {
            byte_offset: 10,
            line: 3,
            column: 4,
        };
        let result = advance_within_line(pos, "abc");
        assert_eq!(result.byte_offset, 13);
        assert_eq!(result.line, 3);
        assert_eq!(result.column, 7);
    }

    #[test]
    fn advance_within_line_multibyte_utf8_column_counts_codepoints() {
        // "日本語" = 9 bytes / 3 codepoints
        let result = advance_within_line(Pos::ORIGIN, "日本語");
        assert_eq!(result.byte_offset, 9);
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 3);
    }

    #[test]
    fn advance_within_line_multibyte_mid_line_accumulates_correctly() {
        let pos = Pos {
            byte_offset: 4,
            line: 1,
            column: 2,
        };
        let result = advance_within_line(pos, "日本語");
        assert_eq!(result.byte_offset, 13);
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 5);
    }

    #[test]
    fn advance_within_line_mixed_ascii_then_multibyte() {
        // "ab日" = 2 + 3 = 5 bytes / 3 codepoints
        let result = advance_within_line(Pos::ORIGIN, "ab日");
        assert_eq!(result.byte_offset, 5);
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 3);
    }

    #[test]
    fn advance_within_line_line_field_is_preserved() {
        let pos = Pos {
            byte_offset: 0,
            line: 7,
            column: 0,
        };
        let result = advance_within_line(pos, "xyz");
        assert_eq!(result.line, 7);
    }

    #[test]
    fn advance_within_line_matches_advance_loop_ascii() {
        let pos = Pos {
            byte_offset: 2,
            line: 1,
            column: 2,
        };
        let content = "abc";
        let expected = content.chars().fold(pos, super::Pos::advance);
        assert_eq!(advance_within_line(pos, content), expected);
    }

    #[test]
    fn advance_within_line_matches_advance_loop_multibyte() {
        let pos = Pos {
            byte_offset: 0,
            line: 1,
            column: 0,
        };
        let content = "日本語xyz";
        let expected = content.chars().fold(pos, super::Pos::advance);
        assert_eq!(advance_within_line(pos, content), expected);
    }
}
