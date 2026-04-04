// SPDX-License-Identifier: MIT

/// A position within the input stream.
///
/// `line` is 1-based; `column` is 0-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub byte_offset: usize,
    pub char_offset: usize,
    pub line: usize,
    pub column: usize,
}

impl Pos {
    /// The position representing the start of a document.
    pub const ORIGIN: Self = Self {
        byte_offset: 0,
        char_offset: 0,
        line: 1,
        column: 0,
    };
}

/// A half-open span `[start, end)` within the input stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn pos_default_is_origin() {
        let pos = Pos::ORIGIN;

        assert_eq!(pos.byte_offset, 0);
        assert_eq!(pos.char_offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 0);
    }

    #[test]
    fn pos_fields_are_accessible() {
        let pos = Pos {
            byte_offset: 10,
            char_offset: 8,
            line: 3,
            column: 4,
        };

        assert_eq!(pos.byte_offset, 10);
        assert_eq!(pos.char_offset, 8);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 4);
    }

    #[test]
    fn span_start_and_end_are_accessible() {
        let start = Pos {
            byte_offset: 0,
            char_offset: 0,
            line: 1,
            column: 0,
        };
        let end = Pos {
            byte_offset: 5,
            char_offset: 5,
            line: 1,
            column: 5,
        };
        let span = Span { start, end };

        assert_eq!(span.start, start);
        assert_eq!(span.end, end);
    }

    #[test]
    fn span_single_character_has_equal_offsets_except_end_byte() {
        let start = Pos {
            byte_offset: 0,
            char_offset: 0,
            line: 1,
            column: 0,
        };
        let end = Pos {
            byte_offset: 1,
            char_offset: 1,
            line: 1,
            column: 1,
        };
        let span = Span { start, end };

        assert_eq!(span.start.byte_offset, 0);
        assert_eq!(span.end.byte_offset, 1);
    }

    #[test]
    fn pos_is_copy() {
        let pos = Pos::ORIGIN;
        let pos2 = pos;
        // Both bindings are usable — Pos is Copy
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
}
