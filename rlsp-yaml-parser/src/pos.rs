// SPDX-License-Identifier: MIT

/// A position within the input stream.
///
/// `line` is 1-based; `column` is 0-based (codepoints from the start of the line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    /// Byte offset from the start of the input (0-based).
    pub byte_offset: usize,
    /// Line number (1-based).
    pub line: usize,
    /// Codepoint column within the current line (0-based).
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
    /// Inclusive start position of the span.
    pub start: Pos,
    /// Exclusive end position of the span.
    pub end: Pos,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

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

    #[rstest]
    #[case::ascii_char('a', 1, 1, 1)]
    #[case::newline('\n', 1, 2, 0)]
    #[case::multibyte_cjk('中', 3, 1, 1)]
    fn advance_basic(
        #[case] ch: char,
        #[case] expected_byte_offset: usize,
        #[case] expected_line: usize,
        #[case] expected_column: usize,
    ) {
        let pos = Pos::ORIGIN.advance(ch);
        assert_eq!(pos.byte_offset, expected_byte_offset);
        assert_eq!(pos.line, expected_line);
        assert_eq!(pos.column, expected_column);
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

    #[rstest]
    #[case::empty_prefix("hello", 0, 0)]
    #[case::ascii_mid_line("hello world", 5, 5)]
    #[case::ascii_full_line("abc", 3, 3)]
    #[case::multibyte_only_prefix("日本語xyz", 9, 3)]
    #[case::ascii_then_multibyte("ab日本", 8, 4)]
    #[case::multibyte_then_ascii("日ab", 5, 3)]
    #[case::full_multibyte_line("日本語", 9, 3)]
    fn column_at_cases(
        #[case] line_content: &str,
        #[case] byte_offset: usize,
        #[case] expected: usize,
    ) {
        assert_eq!(column_at(line_content, byte_offset), expected);
    }

    // -----------------------------------------------------------------------
    // advance_within_line
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::empty_content(Pos { byte_offset: 5, line: 2, column: 3 }, "", 5, 2, 3)]
    #[case::ascii_from_origin(Pos::ORIGIN, "hello", 5, 1, 5)]
    #[case::ascii_mid_line(Pos { byte_offset: 10, line: 3, column: 4 }, "abc", 13, 3, 7)]
    #[case::multibyte_from_origin(Pos::ORIGIN, "日本語", 9, 1, 3)]
    #[case::multibyte_mid_line(Pos { byte_offset: 4, line: 1, column: 2 }, "日本語", 13, 1, 5)]
    #[case::mixed_ascii_then_multibyte(Pos::ORIGIN, "ab日", 5, 1, 3)]
    fn advance_within_line_fields(
        #[case] start: Pos,
        #[case] content: &str,
        #[case] expected_byte_offset: usize,
        #[case] expected_line: usize,
        #[case] expected_column: usize,
    ) {
        let result = advance_within_line(start, content);
        assert_eq!(result.byte_offset, expected_byte_offset);
        assert_eq!(result.line, expected_line);
        assert_eq!(result.column, expected_column);
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
