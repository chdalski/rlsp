// SPDX-License-Identifier: MIT

/// A position within the input stream.
///
/// `line` is 1-based; `column` is 0-based (codepoints from the start of the line).
///
/// Used internally by the lexer and for error reporting. Consumers that need
/// line/column from a `Span` should use [`LineIndex`] instead.
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

// ---------------------------------------------------------------------------
// LineIndex
// ---------------------------------------------------------------------------

/// Bytes between two entries of the character-count table of a [`LineIndex`].
///
/// The table costs `4 / CHECKPOINT_STEP` bytes per source byte, and a column
/// lookup counts at most this many bytes per end of the range.
const CHECKPOINT_STEP: usize = 128;

/// An index over a document's byte offsets that resolves byte offsets to
/// `(line, column)` pairs on demand.
///
/// Constructed once per document from the source `&str`. Stores the byte
/// offset of each line terminator (`\n`, `\r`, or `\r\n`) in sorted order, and
/// the number of characters before every [`CHECKPOINT_STEP`]-th byte.
/// The line is found by binary search; the column is the difference of two
/// character counts, each taken from the nearest checkpoint plus a count over
/// fewer than [`CHECKPOINT_STEP`] bytes. A lookup therefore takes
/// O(log n + [`CHECKPOINT_STEP`]) time however long the line is.
///
/// Line numbers are 1-based; column numbers are 0-based codepoint counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    /// Sorted byte offsets of newline characters in the source.
    /// For `\r\n` pairs, stores the offset of the `\r`.
    newlines: Vec<u32>,
    /// `checkpoints[i]` is the number of characters that start before byte
    /// `i * step`. Holds `source.len() / step + 1` entries.
    checkpoints: Vec<u32>,
    /// Bytes between two checkpoints; at least 1.
    step: usize,
    /// Full source kept for codepoint-column computation.
    source: String,
}

/// Number of characters that start within `bytes` (every byte that is not a
/// UTF-8 continuation byte starts one). Defined for any byte range, also one
/// that starts or ends inside a character.
fn count_char_starts(bytes: &[u8]) -> u32 {
    let count = bytes.iter().filter(|&&b| b & 0xC0 != 0x80).count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

impl LineIndex {
    /// Build a `LineIndex` from the document source string.
    #[must_use]
    pub fn new(source: &str) -> Self {
        Self::with_step(source, CHECKPOINT_STEP)
    }

    /// Build a `LineIndex` with a checkpoint every `step` bytes (at least 1).
    /// Tests use a small step to reach the checkpoint logic with short texts.
    fn with_step(source: &str, step: usize) -> Self {
        let step = step.max(1);
        let mut newlines = Vec::new();
        let mut chars = source.char_indices().peekable();
        while let Some((i, ch)) = chars.next() {
            match ch {
                '\r' => {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "YAML files <= 4 GB; u32 offset is sufficient"
                    )]
                    newlines.push(i as u32);
                    // Consume the following `\n` of a CRLF pair.
                    if chars.peek().is_some_and(|(_, next)| *next == '\n') {
                        let _ = chars.next();
                    }
                }
                '\n' => {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "YAML files <= 4 GB; u32 offset is sufficient"
                    )]
                    newlines.push(i as u32);
                }
                _ => {}
            }
        }
        let mut checkpoints = Vec::with_capacity(source.len() / step + 1);
        checkpoints.push(0u32);
        let mut total = 0u32;
        for chunk in source.as_bytes().chunks_exact(step) {
            total = total.saturating_add(count_char_starts(chunk));
            checkpoints.push(total);
        }
        Self {
            newlines,
            checkpoints,
            step,
            source: source.to_owned(),
        }
    }

    /// Number of characters that start before `byte`, which must not exceed
    /// the source length.
    fn chars_before(&self, byte: usize) -> u32 {
        let slot = byte / self.step;
        // `slot <= source.len() / step`, so the table holds it, and
        // `slot * step <= byte <= source.len()`, so the range is in bounds;
        // the fallbacks are unreachable.
        let base = self.checkpoints.get(slot).copied().unwrap_or(0);
        let tail = self
            .source
            .as_bytes()
            .get(slot * self.step..byte)
            .unwrap_or_default();
        base.saturating_add(count_char_starts(tail))
    }

    /// Resolve a byte offset to a `(line, column)` pair.
    ///
    /// `line` is 1-based; `column` is the 0-based codepoint count from the
    /// start of the line.
    ///
    /// The offset must be a valid byte boundary within the source string; for
    /// any other offset (inside a character, past the end, between the `\r` and
    /// the `\n` of a line break) the column is 0.
    #[must_use]
    pub fn line_column(&self, offset: u32) -> (u32, u32) {
        // Binary-search for the number of newlines before `offset`.
        // `partition_point` returns the index of the first element >= offset,
        // so all elements before that index are < offset (i.e., preceding newlines).
        let newline_idx = self.newlines.partition_point(|&nl| nl < offset);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "line count fits u32 for any realistic document"
        )]
        let line = (newline_idx as u32) + 1;

        // Byte offset of the start of this line.
        let line_start_byte = if newline_idx == 0 {
            0usize
        } else {
            // Safety: newline_idx > 0 here, so newline_idx - 1 is valid.
            #[expect(clippy::indexing_slicing, reason = "newline_idx > 0 is checked above")]
            let nl_byte = self.newlines[newline_idx - 1] as usize;
            // The stored byte is the \r or \n character itself.
            // Advance past it (and a following \n for CRLF).
            let nl_char = self
                .source
                .get(nl_byte..)
                .and_then(|s| s.chars().next())
                .unwrap_or('\n');
            if nl_char == '\r'
                && self
                    .source
                    .get(nl_byte + 1..nl_byte + 2)
                    .is_some_and(|s| s == "\n")
            {
                nl_byte + 2
            } else {
                nl_byte + 1
            }
        };

        // Count codepoints from line start to the requested offset.
        let offset = offset as usize;
        let column = if line_start_byte <= offset && self.source.is_char_boundary(offset) {
            self.chars_before(offset)
                .saturating_sub(self.chars_before(line_start_byte))
        } else {
            0
        };

        (line, column)
    }
}

// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

/// A half-open span `[start, end)` within the input stream, stored as byte
/// offsets.
///
/// Use [`LineIndex`] (available from [`crate::node::Document::line_index`])
/// to convert offsets to `(line, column)` pairs on demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Inclusive start byte offset of the span.
    pub start: u32,
    /// Exclusive end byte offset of the span.
    pub end: u32,
}

impl Span {
    /// Construct a `Span` from a `Pos` pair.
    ///
    /// Used internally during parsing to convert eager `Pos` values to compact
    /// byte-offset spans.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "YAML files <= 4 GB; u32 offset is sufficient"
    )]
    pub(crate) const fn from_pos(start: Pos, end: Pos) -> Self {
        Self {
            start: start.byte_offset as u32,
            end: end.byte_offset as u32,
        }
    }

    /// Return this span with its end moved to `end`.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "YAML files <= 4 GB; u32 offset is sufficient"
    )]
    pub(crate) const fn with_end(self, end: Pos) -> Self {
        Self {
            start: self.start,
            end: end.byte_offset as u32,
        }
    }

    /// Return `(line, column)` for the start of this span.
    ///
    /// Line is 1-based; column is 0-based codepoint count from line start.
    #[must_use]
    pub fn start_line_column(&self, index: &LineIndex) -> (u32, u32) {
        index.line_column(self.start)
    }

    /// Return `(line, column)` for the end of this span.
    ///
    /// Line is 1-based; column is 0-based codepoint count from line start.
    #[must_use]
    pub fn end_line_column(&self, index: &LineIndex) -> (u32, u32) {
        index.line_column(self.end)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    // -----------------------------------------------------------------------
    // Pos tests (kept because Pos is still a public type)
    // -----------------------------------------------------------------------

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
    // Span tests
    // -----------------------------------------------------------------------

    // SZ-1
    #[test]
    fn span_size_is_eight_bytes() {
        assert_eq!(std::mem::size_of::<Span>(), 8);
    }

    const _: () = assert!(
        std::mem::size_of::<Span>() == 8,
        "Span must be exactly 8 bytes"
    );

    #[test]
    fn span_is_copy() {
        let span = Span { start: 0, end: 0 };
        let span2 = span;
        let _ = span.start;
        let _ = span2.start;
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

    // -----------------------------------------------------------------------
    // LI-*: LineIndex unit tests
    // -----------------------------------------------------------------------

    // LI-1
    #[test]
    fn line_index_empty_string_produces_no_newlines() {
        let idx = LineIndex::new("");
        assert!(idx.newlines.is_empty());
        assert_eq!(idx.line_column(0), (1, 0));
    }

    // LI-2
    #[test]
    fn line_index_single_line_no_newline() {
        let idx = LineIndex::new("hello");
        assert_eq!(idx.line_column(0), (1, 0));
        assert_eq!(idx.line_column(4), (1, 4));
        assert_eq!(idx.line_column(5), (1, 5));
    }

    // LI-3
    #[test]
    fn line_index_single_newline_at_end() {
        let idx = LineIndex::new("hello\n");
        assert_eq!(idx.line_column(0), (1, 0));
        assert_eq!(idx.line_column(4), (1, 4));
        assert_eq!(idx.line_column(5), (1, 5)); // the \n byte itself
        assert_eq!(idx.line_column(6), (2, 0)); // byte past the newline
    }

    // LI-4
    #[test]
    fn line_index_multiple_lines_line_numbers_correct() {
        let idx = LineIndex::new("a\nb\nc");
        assert_eq!(idx.line_column(0), (1, 0));
        assert_eq!(idx.line_column(2), (2, 0));
        assert_eq!(idx.line_column(4), (3, 0));
    }

    // LI-5
    #[test]
    fn line_index_column_is_codepoint_count_not_byte_count() {
        // "日本語\nfoo" — 日本語 is 9 bytes / 3 codepoints
        let idx = LineIndex::new("日本語\nfoo");
        assert_eq!(idx.line_column(9), (1, 3)); // the \n byte
        assert_eq!(idx.line_column(10), (2, 0));
        assert_eq!(idx.line_column(11), (2, 1));
    }

    // LI-6
    #[test]
    fn line_index_ascii_fast_path_matches_general_path() {
        // "abc\nxyz": x=col0, y=col1, z=col2
        let idx = LineIndex::new("abc\nxyz");
        assert_eq!(idx.line_column(5), (2, 1)); // 'y'
    }

    // LI-7
    #[test]
    fn line_index_multibyte_mid_line() {
        // "ab日xyz\nok" — ab=2B, 日=3B, xyz starts at byte 5
        let idx = LineIndex::new("ab日xyz\nok");
        assert_eq!(idx.line_column(2), (1, 2));
        assert_eq!(idx.line_column(5), (1, 3)); // byte after 日 = col 3
        assert_eq!(idx.line_column(6), (1, 4)); // x
        assert_eq!(idx.line_column(8), (1, 6)); // z
        assert_eq!(idx.line_column(9), (2, 0)); // after \n
    }

    // LI-8
    #[test]
    fn line_index_crlf_line_endings() {
        // "a\r\nb" — \r at byte 1, \n at byte 2; 'b' at byte 3
        let idx = LineIndex::new("a\r\nb");
        assert_eq!(idx.line_column(0), (1, 0));
        assert_eq!(idx.line_column(3), (2, 0));
    }

    // LI-9
    #[test]
    fn line_index_bare_cr_line_endings() {
        // "a\rb" — \r at byte 1; 'b' at byte 2
        let idx = LineIndex::new("a\rb");
        assert_eq!(idx.line_column(0), (1, 0));
        assert_eq!(idx.line_column(2), (2, 0));
    }

    // -----------------------------------------------------------------------
    // RT-*: Property tests — round-trip correctness
    // -----------------------------------------------------------------------

    /// Oracle: drive `Pos::advance` through each char up to `offset`, return (line, col).
    fn eager_line_column(source: &str, offset: usize) -> (u32, u32) {
        let mut pos = Pos::ORIGIN;
        for ch in source.chars() {
            if pos.byte_offset >= offset {
                break;
            }
            pos = pos.advance(ch);
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "test oracle: values fit u32 in realistic inputs"
        )]
        (pos.line as u32, pos.column as u32)
    }

    proptest! {
        // RT-1: ASCII inputs
        #[test]
        #[expect(
            clippy::cast_possible_truncation,
            reason = "proptest: offset fits u32 for any string of length <= 50"
        )]
        fn line_index_line_column_matches_advance_loop_ascii(
            input in proptest::string::string_regex("[a-z\n]{0,50}").unwrap()
        ) {
            let idx = LineIndex::new(&input);
            let mut offset = 0usize;
            for ch in input.chars() {
                let expected = eager_line_column(&input, offset);
                let got = idx.line_column(offset as u32);
                prop_assert_eq!(
                    got, expected,
                    "mismatch at offset {} in {:?}", offset, input
                );
                offset += ch.len_utf8();
            }
            // Also test at end-of-string offset.
            if offset <= input.len() {
                let expected = eager_line_column(&input, offset);
                let got = idx.line_column(offset as u32);
                prop_assert_eq!(got, expected, "mismatch at end offset {}", offset);
            }
        }

        // RT-2: Unicode inputs including multibyte characters
        #[test]
        #[expect(
            clippy::cast_possible_truncation,
            reason = "proptest: offset fits u32 for any string of length <= 30"
        )]
        fn line_index_line_column_matches_advance_loop_multibyte(
            input in proptest::string::string_regex("[a-z\n\u{4E00}-\u{4E10}]{0,30}").unwrap()
        ) {
            let idx = LineIndex::new(&input);
            let mut offset = 0usize;
            for ch in input.chars() {
                let expected = eager_line_column(&input, offset);
                let got = idx.line_column(offset as u32);
                prop_assert_eq!(
                    got, expected,
                    "mismatch at offset {} in {:?}", offset, input
                );
                offset += ch.len_utf8();
            }
        }
    }

    // -----------------------------------------------------------------------
    // CP-*: checkpoint table — results equal the per-call counting it replaced
    // -----------------------------------------------------------------------

    /// Reference: the implementation before the checkpoint table, which counts
    /// the code points from the line start on every call. Kept as the oracle
    /// for the results the table must preserve, edge cases included.
    fn old_line_column(idx: &LineIndex, offset: u32) -> (u32, u32) {
        let source = &idx.source;
        let newline_idx = idx.newlines.partition_point(|&nl| nl < offset);
        let line = u32::try_from(newline_idx).unwrap() + 1;
        let line_start_byte = if newline_idx == 0 {
            0usize
        } else {
            let nl_byte = idx.newlines[newline_idx - 1] as usize;
            let nl_char = source
                .get(nl_byte..)
                .and_then(|s| s.chars().next())
                .unwrap_or('\n');
            if nl_char == '\r' && source.get(nl_byte + 1..nl_byte + 2) == Some("\n") {
                nl_byte + 2
            } else {
                nl_byte + 1
            }
        };
        let col_prefix = source.get(line_start_byte..offset as usize).unwrap_or("");
        (line, u32::try_from(col_prefix.chars().count()).unwrap())
    }

    /// Assert that `idx` answers like the oracle at every offset from 0 to one
    /// past the end, and at `u32::MAX`.
    fn assert_matches_old(idx: &LineIndex) {
        let len = u32::try_from(idx.source.len()).unwrap();
        for offset in (0..=len + 1).chain([u32::MAX]) {
            assert_eq!(
                idx.line_column(offset),
                old_line_column(idx, offset),
                "offset {offset} in {:?} with step {}",
                idx.source,
                idx.step
            );
        }
    }

    #[rstest]
    #[case::empty_text("")]
    #[case::ascii_line("abcdefghijklmnop")]
    #[case::two_byte_char_over_a_checkpoint("abc\u{e9}def")]
    #[case::three_byte_char_over_a_checkpoint("ab\u{20ac}cdef")]
    #[case::four_byte_char_over_a_checkpoint("a\u{1f600}bcdef")]
    #[case::mixed_widths("a\u{e9}\u{20ac}\u{1f600}a\u{e9}\u{20ac}\u{1f600}")]
    #[case::crlf_across_a_checkpoint("abc\r\ndef\r\nghi")]
    #[case::cr_on_a_checkpoint("abcd\r\nefgh")]
    #[case::lf_on_a_checkpoint("abcd\nefgh")]
    #[case::line_start_inside_the_previous_char("ab\u{e9}\nxy\u{e9}z")]
    #[case::text_ending_in_cr("abc\r")]
    #[case::text_ending_in_crlf("abc\r\n")]
    #[case::lone_crs_and_empty_lines("a\rb\r\rc\n\n\r\n\r\nd")]
    #[case::long_line_then_short_line("abcdefghijklmnopqrstuvwxyz\nab")]
    #[case::bom("\u{feff}a\nb")]
    fn line_column_matches_the_per_call_count_at_every_offset(
        #[case] text: &str,
        #[values(1, 2, 3, 4, 5, 8, 64)] step: usize,
    ) {
        assert_matches_old(&LineIndex::with_step(text, step));
        assert_matches_old(&LineIndex::new(text));
    }

    #[rstest]
    #[case::before_the_end("abc", 2, (1, 2))]
    #[case::at_the_end("abc", 3, (1, 3))]
    #[case::past_the_end("abc", 4, (1, 0))]
    #[case::at_u32_max("abc", u32::MAX, (1, 0))]
    #[case::past_the_end_on_a_later_line("a\nbc", 100, (2, 0))]
    #[case::inside_a_two_byte_char("a\u{e9}", 2, (1, 0))]
    #[case::inside_a_four_byte_char_on_a_later_line("\u{e9}\n\u{1f600}", 5, (2, 0))]
    #[case::between_cr_and_lf("a\r\nb", 2, (2, 0))]
    #[case::after_crlf("a\r\nb", 3, (2, 0))]
    #[case::after_cr_at_the_end("a\r", 2, (2, 0))]
    #[case::mixed_breaks_second_line("ab\ncd\r\nef\rgh", 4, (2, 1))]
    #[case::mixed_breaks_third_line("ab\ncd\r\nef\rgh", 8, (3, 1))]
    #[case::mixed_breaks_fourth_line("ab\ncd\r\nef\rgh", 11, (4, 1))]
    fn line_column_of_edge_offsets(
        #[case] text: &str,
        #[case] offset: u32,
        #[case] expected: (u32, u32),
    ) {
        assert_eq!(LineIndex::new(text).line_column(offset), expected);
        assert_eq!(LineIndex::with_step(text, 2).line_column(offset), expected);
    }

    fn text_from(parts: &[&str]) -> String {
        parts.concat()
    }

    proptest! {
        // CP-1: small steps put checkpoints inside short texts.
        #[test]
        fn line_column_matches_the_per_call_count_for_small_steps(
            parts in proptest::collection::vec(
                prop_oneof![
                    3 => Just("a"),
                    1 => Just(" "),
                    2 => Just("\n"),
                    2 => Just("\r"),
                    2 => Just("\r\n"),
                    2 => Just("\u{e9}"),
                    1 => Just("\u{20ac}"),
                    1 => Just("\u{1f600}"),
                ],
                0..40,
            ),
            step in prop_oneof![Just(1usize), Just(2), Just(3), Just(4), Just(5), Just(8), Just(64)],
        ) {
            assert_matches_old(&LineIndex::with_step(&text_from(&parts), step));
        }

        // CP-2: texts several checkpoints long with the real step.
        #[test]
        fn line_column_matches_the_per_call_count_for_long_texts(
            parts in proptest::collection::vec(
                prop_oneof![
                    8 => Just("abcdefgh"),
                    1 => Just("\n"),
                    1 => Just("\r\n"),
                    3 => Just("\u{e9}"),
                    2 => Just("\u{20ac}"),
                    2 => Just("\u{1f600}"),
                ],
                0..300,
            ),
        ) {
            assert_matches_old(&LineIndex::new(&text_from(&parts)));
        }
    }

    #[rstest]
    #[case::ascii("a")]
    #[case::two_byte("\u{e9}")]
    #[case::four_byte("\u{1f600}")]
    fn checkpoint_table_of_one_mib_text_takes_at_most_64_kib(#[case] unit: &str) {
        let text = unit.repeat(1024 * 1024 / unit.len());
        let idx = LineIndex::new(&text);
        let bytes = idx.checkpoints.capacity() * size_of::<u32>();
        assert!(bytes <= 64 * 1024, "table takes {bytes} bytes");
    }

    #[test]
    fn checkpoint_table_of_equal_texts_is_equal() {
        assert_eq!(LineIndex::new("a\u{e9}\nb"), LineIndex::new("a\u{e9}\nb"));
        assert_ne!(LineIndex::new("a\u{e9}\nb"), LineIndex::new("a\u{e9}\nc"));
    }
}
