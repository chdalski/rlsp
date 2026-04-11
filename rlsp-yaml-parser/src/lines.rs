// SPDX-License-Identifier: MIT

//! Line-at-a-time buffer with one-line lookahead for the streaming parser.
//!
//! `LineBuffer` wraps an `&'input str` and yields one [`Line`] at a time,
//! always keeping the *next* line primed in an internal slot so callers can
//! peek at the next line's indent without consuming it.  It never scans the
//! full input up front, giving O(1) first-event latency.

use std::collections::VecDeque;

use crate::pos::Pos;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The type of line terminator that ends a [`Line`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakType {
    /// `\n` (line feed)
    Lf,
    /// `\r` (bare carriage return — no following `\n`)
    Cr,
    /// `\r\n` (CRLF pair)
    CrLf,
    /// End of input — the line has no terminator.
    Eof,
}

impl BreakType {
    /// Byte length of this line terminator (0 for Eof).
    #[must_use]
    pub const fn byte_len(self) -> usize {
        match self {
            Self::Lf | Self::Cr => 1,
            Self::CrLf => 2,
            Self::Eof => 0,
        }
    }

    /// Advance `pos` past this line break.
    ///
    /// Each break type requires distinct logic because `Pos::advance(char)`
    /// operates on individual characters and cannot distinguish bare `\r`
    /// from `\r\n`.
    #[must_use]
    pub const fn advance(self, mut pos: Pos) -> Pos {
        match self {
            Self::Lf => pos.advance('\n'),
            Self::CrLf => {
                pos.byte_offset += '\r'.len_utf8();
                pos.advance('\n')
            }
            Self::Cr => {
                pos.byte_offset += '\r'.len_utf8();
                pos.line += 1;
                pos.column = 0;
                pos
            }
            Self::Eof => pos,
        }
    }
}

/// A single logical line extracted from the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line<'input> {
    /// The line content slice, **excluding** the terminator.
    pub content: &'input str,
    /// Byte offset of `content` within the original input string.
    pub offset: usize,
    /// Number of leading `SPACE` (`\x20`) characters.  Leading tabs do not
    /// contribute to indent — they are a YAML syntax error in indentation
    /// context and are reported by the lexer, not here.
    pub indent: usize,
    /// The terminator that ends this line.
    pub break_type: BreakType,
    /// Position of the first byte of this line (after BOM stripping on line 1).
    pub pos: Pos,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Detect the line break at the start of `s` and return `(BreakType, rest)`.
///
/// CRLF is checked first so that `\r\n` is consumed as a unit rather than
/// treating `\r` as a bare CR.
fn detect_break(s: &str) -> (BreakType, &str) {
    if let Some(rest) = s.strip_prefix("\r\n") {
        return (BreakType::CrLf, rest);
    }
    if let Some(rest) = s.strip_prefix('\r') {
        return (BreakType::Cr, rest);
    }
    if let Some(rest) = s.strip_prefix('\n') {
        return (BreakType::Lf, rest);
    }
    (BreakType::Eof, s)
}

/// Scan one line from `remaining`, starting at `pos`.
///
/// `is_first` controls BOM stripping: if `true` and the slice starts with
/// U+FEFF (UTF-8 BOM, 3 bytes), the BOM is skipped before content begins.
///
/// Returns `Some((line, rest))` or `None` if `remaining` is empty.
fn scan_line(remaining: &str, pos: Pos, is_first: bool) -> Option<(Line<'_>, &str)> {
    if remaining.is_empty() {
        return None;
    }

    // Strip BOM on first line only.
    let (content_start, pos) = if is_first && remaining.starts_with('\u{FEFF}') {
        let bom_len = '\u{FEFF}'.len_utf8(); // 3 bytes
        (
            &remaining[bom_len..],
            Pos {
                byte_offset: pos.byte_offset + bom_len,
                ..pos
            },
        )
    } else {
        (remaining, pos)
    };

    // Find the end of line content (position of the first \n or \r).
    let line_end = content_start
        .find(['\n', '\r'])
        .unwrap_or(content_start.len());

    let content = &content_start[..line_end];
    let after_content = &content_start[line_end..];

    // Determine break type and advance past the terminator.
    // Try CRLF first (must be checked before bare CR).
    let (break_type, after_break) = detect_break(after_content);

    // Count leading SPACE characters only (tabs do not count).
    let indent = content.chars().take_while(|&ch| ch == ' ').count();

    // `offset` is the byte offset of `content` within the *original* input.
    // `pos` already reflects the position after any BOM skip.
    let offset = pos.byte_offset;

    let line = Line {
        content,
        offset,
        indent,
        break_type,
        pos,
    };

    Some((line, after_break))
}

// ---------------------------------------------------------------------------
// LineBuffer
// ---------------------------------------------------------------------------

/// A one-line-lookahead buffer over a `&'input str`.
///
/// Always holds the *next* line pre-parsed.  Callers use [`Self::peek_next`]
/// to inspect without consuming and [`Self::consume_next`] to advance.
pub struct LineBuffer<'input> {
    /// Remaining unparsed input (past the next line's terminator).
    remaining: &'input str,
    /// Synthetic lines prepended by the caller (e.g. inline content extracted
    /// from a sequence- or mapping-entry line).  Drained front-first before
    /// `next`.  A `VecDeque` supports multiple pending prepends when parsing
    /// implicit mapping entries that need to inject both key and value lines.
    prepend: VecDeque<Line<'input>>,
    /// The pre-parsed next line, if any.
    next: Option<Line<'input>>,
    /// Position at the start of `remaining`.
    remaining_pos: Pos,
    /// Whether the next line to be parsed from `remaining` is the first line
    /// of input (used for BOM detection after the initial prime).
    remaining_is_first: bool,
    /// Lookahead buffer for [`Self::peek_until_dedent`].
    lookahead: Vec<Line<'input>>,
}

impl<'input> LineBuffer<'input> {
    /// Construct a new `LineBuffer` and prime the next-line slot.
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        let mut buf = Self {
            remaining: input,
            prepend: VecDeque::new(),
            next: None,
            remaining_pos: Pos::ORIGIN,
            remaining_is_first: true,
            lookahead: Vec::new(),
        };
        buf.prime();
        buf
    }

    /// Prepend a synthetic line that will be returned by the next call to
    /// [`Self::peek_next`] / [`Self::consume_next`], ahead of any real lines.
    ///
    /// Used to re-present inline content extracted from a sequence- or
    /// mapping-entry line as if it were a separate line.  Multiple prepends
    /// are supported: each call pushes to the front of the queue, so the last
    /// prepended line is returned first (LIFO order).  Callers that need FIFO
    /// order (key before value) should prepend value first, then key.
    pub fn prepend_line(&mut self, line: Line<'input>) {
        self.lookahead.clear();
        self.prepend.push_front(line);
    }

    /// Look at the next line without consuming it.
    ///
    /// Returns the frontmost prepended synthetic line first (if any), then the
    /// normally buffered next line.
    #[must_use]
    pub fn peek_next(&self) -> Option<&Line<'input>> {
        self.prepend.front().or(self.next.as_ref())
    }

    /// Returns `true` if the next line comes from the prepend queue (synthetic),
    /// rather than from the original input stream.
    #[must_use]
    pub fn is_next_synthetic(&self) -> bool {
        !self.prepend.is_empty()
    }

    /// Convenience: the indent of the next line, without consuming it.
    #[must_use]
    pub fn peek_next_indent(&self) -> Option<usize> {
        self.peek_next().map(|l| l.indent)
    }

    /// Peek at the second upcoming line without consuming either.
    ///
    /// Handles the prepend queue: the second line may come from the prepend
    /// queue or from the primed `next` slot or from `remaining`.
    #[must_use]
    pub fn peek_second(&self) -> Option<Line<'input>> {
        // Determine where the "first" line comes from, then find the "second".
        if !self.prepend.is_empty() {
            // First line is prepend[0]. Second is prepend[1] if it exists,
            // else self.next.
            if self.prepend.len() >= 2 {
                return self.prepend.get(1).cloned();
            }
            return self.next.clone();
        }
        // First line is self.next. Second is the first line from `remaining`.
        self.next.as_ref()?; // ensure first exists
        scan_line(self.remaining, self.remaining_pos, self.remaining_is_first).map(|(line, _)| line)
    }

    /// Advance: return the currently primed next line and prime the following
    /// one from the remaining input.  Returns `None` when no lines remain.
    ///
    /// Drains prepended synthetic lines (front-first) before the real buffer.
    pub fn consume_next(&mut self) -> Option<Line<'input>> {
        // Drain prepend queue front-first.
        if let Some(line) = self.prepend.pop_front() {
            return Some(line);
        }
        // Clear any cached lookahead — it was based on the old position.
        self.lookahead.clear();
        let line = self.next.take()?;
        self.prime();
        Some(line)
    }

    /// True when no more lines are available (buffer is empty, no prepend, and
    /// input is exhausted).
    #[must_use]
    pub fn at_eof(&self) -> bool {
        self.prepend.is_empty() && self.next.is_none()
    }

    /// Scan forward without consuming to collect all lines with
    /// `indent > base_indent`, stopping at the first line with
    /// `indent <= base_indent`.  Blank lines (empty content) are transparent
    /// to the scan and are included in the result regardless of their indent.
    ///
    /// Returns a slice of the buffered lookahead lines.  Calling this method
    /// repeatedly (without consuming) returns the same slice.
    ///
    /// Note: trailing blank lines in the returned slice are **not** part of
    /// the block scalar content — per YAML chomping rules, trailing blank
    /// lines are stripped, clipped, or kept based on the chomping indicator.
    /// The consumer (lexer, Task 8) is responsible for trimming them.
    pub fn peek_until_dedent(&mut self, base_indent: usize) -> &[Line<'input>] {
        // Rebuild the lookahead starting from the next line.
        self.lookahead.clear();

        // We need to scan from the next primed line plus additional lines
        // from `remaining`.  Use a local cursor.
        let mut cursor_remaining = self.remaining;
        let mut cursor_pos = self.remaining_pos;
        let mut cursor_is_first = self.remaining_is_first;

        // The first line in the lookahead is `self.next` (if any).
        // We include it if it is blank or its indent > base_indent.
        let start_line = match self.next.as_ref() {
            None => return &self.lookahead,
            Some(l) => l.clone(),
        };

        // Process lines in order: start with `self.next`, then scan from
        // `remaining`.
        let mut scanning_next = Some(start_line);

        loop {
            let line = match scanning_next.take() {
                Some(l) => l,
                None => {
                    // Fetch from remaining input.
                    match scan_line(cursor_remaining, cursor_pos, cursor_is_first) {
                        None => break,
                        Some((l, rest)) => {
                            cursor_pos = pos_after_line(&l);
                            cursor_remaining = rest;
                            cursor_is_first = false;
                            l
                        }
                    }
                }
            };

            // Blank lines (empty content) are transparent: include them and
            // keep scanning.
            if line.content.is_empty() {
                self.lookahead.push(line);
                continue;
            }

            // Stop before the first non-blank line that is dedented.
            // base_indent == usize::MAX is the "root level" sentinel meaning
            // no indent threshold — include all non-blank lines.
            if base_indent != usize::MAX && line.indent <= base_indent {
                break;
            }

            self.lookahead.push(line);
        }

        &self.lookahead
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Parse one more line from `remaining` into `self.next`.
    fn prime(&mut self) {
        match scan_line(self.remaining, self.remaining_pos, self.remaining_is_first) {
            None => {
                self.next = None;
            }
            Some((line, rest)) => {
                // Advance `remaining_pos` past the line we just parsed.
                let new_pos = pos_after_line(&line);
                self.remaining_pos = new_pos;
                self.remaining = rest;
                self.remaining_is_first = false;
                self.next = Some(line);
            }
        }
    }
}

/// Compute the `Pos` immediately after the terminator of `line`.
///
/// O(1) for `Lf`/`Cr`/`CrLf` — the next line is at `line+1, column=0`.
/// O(content) for `Eof` — the final line has no terminator, so position stays
/// on the same line; column advances by the char count of the content via the
/// ASCII fast path in [`crate::pos::column_at`].
pub fn pos_after_line(line: &Line<'_>) -> Pos {
    let byte_offset = line.offset + line.content.len() + line.break_type.byte_len();
    match line.break_type {
        BreakType::Eof => Pos {
            byte_offset,
            line: line.pos.line,
            column: line.pos.column + crate::pos::column_at(line.content, line.content.len()),
        },
        BreakType::Lf | BreakType::Cr | BreakType::CrLf => Pos {
            byte_offset,
            line: line.pos.line + 1,
            column: 0,
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // BreakType::advance
    // -----------------------------------------------------------------------

    #[test]
    fn break_type_advance_lf() {
        let pos = Pos::ORIGIN;
        let after = BreakType::Lf.advance(pos);
        assert_eq!(after.byte_offset, 1);
        assert_eq!(after.line, 2);
        assert_eq!(after.column, 0);
    }

    #[test]
    fn break_type_advance_crlf() {
        let pos = Pos::ORIGIN;
        let after = BreakType::CrLf.advance(pos);
        // \r = 1 byte, \n = 1 byte → 2 bytes total
        assert_eq!(after.byte_offset, 2);
        assert_eq!(after.line, 2);
        assert_eq!(after.column, 0);
    }

    #[test]
    fn break_type_advance_cr_increments_line() {
        let pos = Pos::ORIGIN;
        let after = BreakType::Cr.advance(pos);
        assert_eq!(after.line, 2);
    }

    #[test]
    fn break_type_advance_cr_resets_column() {
        let pos = Pos {
            byte_offset: 3,
            line: 1,
            column: 3,
        };
        let after = BreakType::Cr.advance(pos);
        assert_eq!(after.column, 0);
        assert_eq!(after.byte_offset, 4); // \r = 1 byte
        assert_eq!(after.line, 2);
    }

    #[test]
    fn break_type_advance_lf_at_non_origin_pos() {
        let pos = Pos {
            byte_offset: 5,
            line: 2,
            column: 3,
        };
        let after = BreakType::Lf.advance(pos);
        assert_eq!(after.byte_offset, 6);
        assert_eq!(after.line, 3);
        assert_eq!(after.column, 0);
    }

    #[test]
    fn break_type_advance_crlf_at_non_origin_pos() {
        let pos = Pos {
            byte_offset: 5,
            line: 2,
            column: 3,
        };
        let after = BreakType::CrLf.advance(pos);
        assert_eq!(after.byte_offset, 7); // \r (1) + \n (1) = +2
        assert_eq!(after.line, 3);
        assert_eq!(after.column, 0);
    }

    #[test]
    fn break_type_advance_eof_is_noop() {
        let pos = Pos {
            byte_offset: 5,
            line: 3,
            column: 2,
        };
        let after = BreakType::Eof.advance(pos);
        assert_eq!(after, pos);
    }

    // -----------------------------------------------------------------------
    // new and initial state
    // -----------------------------------------------------------------------

    #[test]
    fn new_empty_input_at_eof_immediately() {
        let buf = LineBuffer::new("");
        assert!(buf.peek_next().is_none());
        assert!(buf.at_eof());
    }

    #[test]
    fn new_single_line_no_newline_primes_eof_line() {
        let buf = LineBuffer::new("foo");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "foo");
        assert_eq!(line.break_type, BreakType::Eof);
        assert_eq!(line.offset, 0);
    }

    #[test]
    fn new_single_line_with_lf_primes_first_line() {
        let buf = LineBuffer::new("foo\n");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "foo");
        assert_eq!(line.break_type, BreakType::Lf);
    }

    #[test]
    fn new_input_with_only_lf_primes_empty_line() {
        let buf = LineBuffer::new("\n");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "");
        assert_eq!(line.break_type, BreakType::Lf);
    }

    // -----------------------------------------------------------------------
    // consume_next sequencing
    // -----------------------------------------------------------------------

    #[test]
    fn consume_returns_primed_line_and_advances() {
        let mut buf = LineBuffer::new("a\nb\n");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first line");
        };
        assert_eq!(first.content, "a");
        assert_eq!(first.break_type, BreakType::Lf);
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second line");
        };
        assert_eq!(second.content, "b");
        assert_eq!(second.break_type, BreakType::Lf);
    }

    #[test]
    fn consume_after_last_line_returns_none() {
        let mut buf = LineBuffer::new("foo");
        assert!(buf.consume_next().is_some());
        assert!(buf.consume_next().is_none());
    }

    #[test]
    fn at_eof_false_before_consuming_last_and_true_after() {
        let mut buf = LineBuffer::new("foo");
        assert!(!buf.at_eof());
        buf.consume_next();
        assert!(buf.at_eof());
    }

    #[test]
    fn consume_all_lines_then_peek_returns_none() {
        let mut buf = LineBuffer::new("a\nb");
        buf.consume_next();
        buf.consume_next();
        assert!(buf.peek_next().is_none());
    }

    // -----------------------------------------------------------------------
    // line terminator types
    // -----------------------------------------------------------------------

    #[test]
    fn lf_terminator_produces_lf_break_type() {
        let mut buf = LineBuffer::new("a\n");
        let Some(line) = buf.consume_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.break_type, BreakType::Lf);
    }

    #[test]
    fn crlf_terminator_produces_crlf_break_type_not_two_lines() {
        let mut buf = LineBuffer::new("a\r\nb");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.content, "a");
        assert_eq!(first.break_type, BreakType::CrLf);
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.content, "b");
        assert_eq!(second.break_type, BreakType::Eof);
        assert!(buf.consume_next().is_none());
    }

    #[test]
    fn bare_cr_terminator_produces_cr_break_type() {
        let mut buf = LineBuffer::new("a\rb");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.content, "a");
        assert_eq!(first.break_type, BreakType::Cr);
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.content, "b");
        assert_eq!(second.break_type, BreakType::Eof);
    }

    #[test]
    fn no_terminator_on_last_line_produces_eof_break_type() {
        let mut buf = LineBuffer::new("a\nb");
        buf.consume_next();
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.content, "b");
        assert_eq!(second.break_type, BreakType::Eof);
    }

    #[test]
    fn mixed_line_endings_each_line_has_correct_break_type() {
        let mut buf = LineBuffer::new("a\nb\r\nc\rd");
        let types: Vec<BreakType> = (0..4)
            .filter_map(|_| buf.consume_next().map(|l| l.break_type))
            .collect();
        assert_eq!(
            types,
            [
                BreakType::Lf,
                BreakType::CrLf,
                BreakType::Cr,
                BreakType::Eof
            ]
        );
    }

    #[test]
    fn only_crlf_produces_one_empty_line_not_two() {
        let mut buf = LineBuffer::new("\r\n");
        let Some(line) = buf.consume_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "");
        assert_eq!(line.break_type, BreakType::CrLf);
        assert!(buf.consume_next().is_none());
    }

    #[test]
    fn only_cr_produces_one_empty_line() {
        let mut buf = LineBuffer::new("\r");
        let Some(line) = buf.consume_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "");
        assert_eq!(line.break_type, BreakType::Cr);
        assert!(buf.consume_next().is_none());
    }

    #[test]
    fn only_lf_produces_one_empty_line() {
        let mut buf = LineBuffer::new("\n");
        let Some(line) = buf.consume_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "");
        assert_eq!(line.break_type, BreakType::Lf);
        assert!(buf.consume_next().is_none());
    }

    #[test]
    fn two_consecutive_lf_produce_two_empty_lines() {
        let mut buf = LineBuffer::new("\n\n");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.content, "");
        assert_eq!(first.break_type, BreakType::Lf);
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.content, "");
        assert_eq!(second.break_type, BreakType::Lf);
        assert!(buf.consume_next().is_none());
    }

    #[test]
    fn trailing_lf_does_not_produce_extra_empty_line() {
        // A trailing newline terminates the last line; it does not introduce
        // a new empty line.
        let mut buf = LineBuffer::new("foo\n");
        let Some(line) = buf.consume_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "foo");
        assert!(buf.consume_next().is_none());
    }

    // -----------------------------------------------------------------------
    // offset and Pos tracking
    // -----------------------------------------------------------------------

    #[test]
    fn offset_is_byte_offset_of_content_start() {
        let mut buf = LineBuffer::new("foo\nbar\n");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.offset, 0);
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.offset, 4); // "foo\n" = 4 bytes
    }

    #[test]
    fn offset_and_pos_byte_offset_agree() {
        let mut buf = LineBuffer::new("foo\nbar");
        while let Some(line) = buf.consume_next() {
            assert_eq!(line.offset, line.pos.byte_offset);
        }
    }

    #[test]
    fn pos_line_number_increments_per_line() {
        let mut buf = LineBuffer::new("a\nb\nc");
        let lines: Vec<Line<'_>> = (0..3).filter_map(|_| buf.consume_next()).collect();
        assert_eq!(lines.len(), 3, "expected 3 lines");
        assert_eq!(lines.first().map(|l| l.pos.line), Some(1));
        assert_eq!(lines.get(1).map(|l| l.pos.line), Some(2));
        assert_eq!(lines.get(2).map(|l| l.pos.line), Some(3));
    }

    #[test]
    fn pos_column_is_zero_at_start_of_each_line() {
        let mut buf = LineBuffer::new("a\nb");
        while let Some(line) = buf.consume_next() {
            assert_eq!(line.pos.column, 0);
        }
    }

    #[test]
    fn pos_line_increments_after_bare_cr() {
        // Bare \r is a line terminator: the next line must start on line 2.
        let mut buf = LineBuffer::new("a\rb");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.pos.line, 1);
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.pos.line, 2);
        assert_eq!(second.pos.column, 0);
    }

    #[test]
    fn pos_column_resets_after_bare_cr() {
        // After consuming a line that ends with bare \r, the next line's
        // column must be 0, not the column that followed the last content char.
        let mut buf = LineBuffer::new("abc\rd");
        buf.consume_next(); // consume "abc"
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.pos.column, 0);
    }

    #[test]
    fn pos_line_increments_after_crlf() {
        // CRLF is a line terminator: the next line must start on line 2.
        let mut buf = LineBuffer::new("a\r\nb");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.pos.line, 1);
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.pos.line, 2);
        assert_eq!(second.pos.column, 0);
    }

    #[test]
    fn pos_after_mixed_endings_tracks_lines_correctly() {
        // Input has four lines with three different terminator types.
        let mut buf = LineBuffer::new("a\nb\r\nc\rd");
        let lines: Vec<Line<'_>> = (0..4).filter_map(|_| buf.consume_next()).collect();
        assert_eq!(lines.len(), 4, "expected 4 lines");
        let line_nums: Vec<usize> = lines.iter().map(|l| l.pos.line).collect();
        assert_eq!(line_nums, [1, 2, 3, 4]);
        for line in &lines {
            assert_eq!(
                line.pos.column, 0,
                "line {} should start at column 0",
                line.pos.line
            );
        }
    }

    #[test]
    fn multibyte_content_byte_offset_is_byte_based_not_char_based() {
        // '中' is 3 UTF-8 bytes
        let mut buf = LineBuffer::new("中\nfoo");
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.offset, 0);
        assert_eq!(first.content, "中");
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        // 3 bytes for '中' + 1 byte for '\n' = 4
        assert_eq!(second.offset, 4);
    }

    // -----------------------------------------------------------------------
    // BOM handling
    // -----------------------------------------------------------------------

    #[test]
    fn bom_is_stripped_from_content_of_first_line() {
        let input = "\u{FEFF}foo\n";
        let buf = LineBuffer::new(input);
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "foo");
    }

    #[test]
    fn bom_stripped_line_offset_starts_after_bom_bytes() {
        let input = "\u{FEFF}foo\n";
        let buf = LineBuffer::new(input);
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        // BOM is U+FEFF = 3 bytes in UTF-8
        assert_eq!(line.offset, 3);
        assert_eq!(line.pos.byte_offset, 3);
    }

    #[test]
    fn bom_only_stripped_from_first_line() {
        // A BOM in a non-first line is preserved as data (the lexer will
        // report it as an error).
        let input = "foo\n\u{FEFF}bar\n";
        let mut buf = LineBuffer::new(input);
        buf.consume_next(); // consume "foo"
        let Some(second) = buf.consume_next() else {
            unreachable!("expected second");
        };
        assert_eq!(second.content, "\u{FEFF}bar");
    }

    // -----------------------------------------------------------------------
    // indent counting
    // -----------------------------------------------------------------------

    #[test]
    fn indent_counts_only_leading_spaces() {
        let buf = LineBuffer::new("   foo");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.indent, 3);
    }

    #[test]
    fn indent_is_zero_for_no_leading_spaces() {
        let buf = LineBuffer::new("foo");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.indent, 0);
    }

    #[test]
    fn leading_tab_does_not_count_toward_indent() {
        let buf = LineBuffer::new("\tfoo");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.indent, 0);
    }

    #[test]
    fn tab_after_spaces_does_not_count() {
        let buf = LineBuffer::new("  \tfoo");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.indent, 2);
    }

    #[test]
    fn indent_of_blank_line_is_zero() {
        let buf = LineBuffer::new("\n");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.indent, 0);
    }

    #[test]
    fn indent_of_spaces_only_line_equals_space_count() {
        let buf = LineBuffer::new("   \n");
        let Some(line) = buf.peek_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.indent, 3);
        assert_eq!(line.content, "   ");
    }

    // -----------------------------------------------------------------------
    // peek_next_indent
    // -----------------------------------------------------------------------

    #[test]
    fn peek_next_indent_returns_indent_of_next_line() {
        let buf = LineBuffer::new("   foo");
        assert_eq!(buf.peek_next_indent(), Some(3));
    }

    #[test]
    fn peek_next_indent_returns_none_at_eof() {
        let buf = LineBuffer::new("");
        assert_eq!(buf.peek_next_indent(), None);
    }

    #[test]
    fn peek_next_indent_does_not_consume() {
        let mut buf = LineBuffer::new("  foo");
        assert_eq!(buf.peek_next_indent(), Some(2));
        assert_eq!(buf.peek_next_indent(), Some(2));
        let Some(line) = buf.consume_next() else {
            unreachable!("expected a line");
        };
        assert_eq!(line.content, "  foo");
    }

    // -----------------------------------------------------------------------
    // peek_until_dedent
    // -----------------------------------------------------------------------

    #[test]
    fn peek_until_dedent_empty_input_returns_empty_slice() {
        let mut buf = LineBuffer::new("");
        assert!(buf.peek_until_dedent(0).is_empty());
    }

    #[test]
    fn peek_until_dedent_returns_lines_until_indent_le_base() {
        let mut buf = LineBuffer::new("  a\n  b\nc\n");
        let lines = buf.peek_until_dedent(1);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines.first().map(|l| l.content), Some("  a"));
        assert_eq!(lines.get(1).map(|l| l.content), Some("  b"));
    }

    #[test]
    fn peek_until_dedent_does_not_consume_lines() {
        let mut buf = LineBuffer::new("  a\n  b\nc\n");
        let _ = buf.peek_until_dedent(1);
        let Some(first) = buf.consume_next() else {
            unreachable!("expected first");
        };
        assert_eq!(first.content, "  a");
    }

    #[test]
    fn peek_until_dedent_includes_all_lines_when_no_dedent_occurs() {
        let mut buf = LineBuffer::new("  a\n  b\n  c");
        let lines = buf.peek_until_dedent(1);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn peek_until_dedent_returns_empty_slice_when_first_line_already_dedented() {
        let mut buf = LineBuffer::new("a\n  b\n");
        let lines = buf.peek_until_dedent(1);
        // "a" has indent 0 <= 1, so stop immediately
        assert!(lines.is_empty());
    }

    #[test]
    fn peek_until_dedent_second_call_returns_same_slice() {
        let mut buf = LineBuffer::new("  a\n  b\nc");
        let first_call: Vec<String> = buf
            .peek_until_dedent(1)
            .iter()
            .map(|l| l.content.to_owned())
            .collect();
        let second_call: Vec<String> = buf
            .peek_until_dedent(1)
            .iter()
            .map(|l| l.content.to_owned())
            .collect();
        assert_eq!(first_call, second_call);
        assert_eq!(first_call, ["  a", "  b"]);
    }

    #[test]
    fn peek_until_dedent_base_zero_stops_at_non_indented_lines() {
        // base_indent=0: stop at lines with indent <= 0 (i.e., indent == 0).
        // Both lines here have indent > 0, so all are included.
        let mut buf = LineBuffer::new("  a\n  b\n");
        let lines = buf.peek_until_dedent(0);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn peek_until_dedent_blank_lines_are_transparent() {
        // Blank lines (empty content) are transparent: they are included in
        // the result and do not halt the scan.
        // "  a" (indent 2 > 1) -> included
        // ""    (blank)         -> transparent, included
        // "  b" (indent 2 > 1) -> included
        // "c"   (indent 0 <= 1) -> stop
        let mut buf = LineBuffer::new("  a\n\n  b\nc");
        let lines = buf.peek_until_dedent(1);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines.first().map(|l| l.content), Some("  a"));
        assert_eq!(lines.get(1).map(|l| l.content), Some(""));
        assert_eq!(lines.get(2).map(|l| l.content), Some("  b"));
    }

    // -----------------------------------------------------------------------
    // pos_after_line
    // -----------------------------------------------------------------------

    #[test]
    fn pos_after_line_lf_ascii() {
        let line = Line {
            content: "hello",
            offset: 0,
            indent: 0,
            break_type: BreakType::Lf,
            pos: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 6);
        assert_eq!(result.line, 2);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_lf_empty_content() {
        let line = Line {
            content: "",
            offset: 10,
            indent: 0,
            break_type: BreakType::Lf,
            pos: Pos {
                byte_offset: 10,
                line: 3,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 11);
        assert_eq!(result.line, 4);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_lf_multibyte() {
        let line = Line {
            content: "日本",
            offset: 0,
            indent: 0,
            break_type: BreakType::Lf,
            pos: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 7); // 6 bytes + 1 for \n
        assert_eq!(result.line, 2);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_cr_ascii() {
        let line = Line {
            content: "abc",
            offset: 0,
            indent: 0,
            break_type: BreakType::Cr,
            pos: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 4);
        assert_eq!(result.line, 2);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_cr_empty_content() {
        let line = Line {
            content: "",
            offset: 5,
            indent: 0,
            break_type: BreakType::Cr,
            pos: Pos {
                byte_offset: 5,
                line: 2,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 6);
        assert_eq!(result.line, 3);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_crlf_ascii() {
        let line = Line {
            content: "key: val",
            offset: 0,
            indent: 0,
            break_type: BreakType::CrLf,
            pos: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 10);
        assert_eq!(result.line, 2);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_crlf_empty_content() {
        let line = Line {
            content: "",
            offset: 0,
            indent: 0,
            break_type: BreakType::CrLf,
            pos: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 2);
        assert_eq!(result.line, 2);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_eof_empty_content() {
        let line = Line {
            content: "",
            offset: 20,
            indent: 0,
            break_type: BreakType::Eof,
            pos: Pos {
                byte_offset: 20,
                line: 5,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 20);
        assert_eq!(result.line, 5);
        assert_eq!(result.column, 0);
    }

    #[test]
    fn pos_after_line_eof_ascii() {
        let line = Line {
            content: "last",
            offset: 10,
            indent: 0,
            break_type: BreakType::Eof,
            pos: Pos {
                byte_offset: 10,
                line: 3,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 14);
        assert_eq!(result.line, 3);
        assert_eq!(result.column, 4);
    }

    #[test]
    fn pos_after_line_eof_ascii_nonzero_start_column() {
        let line = Line {
            content: "end",
            offset: 7,
            indent: 0,
            break_type: BreakType::Eof,
            pos: Pos {
                byte_offset: 7,
                line: 2,
                column: 5,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 10);
        assert_eq!(result.line, 2);
        assert_eq!(result.column, 8);
    }

    #[test]
    fn pos_after_line_eof_multibyte() {
        let line = Line {
            content: "日本語",
            offset: 0,
            indent: 0,
            break_type: BreakType::Eof,
            pos: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 9);
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 3);
    }

    #[test]
    fn pos_after_line_eof_mixed_content() {
        let line = Line {
            content: "ab日",
            offset: 0,
            indent: 0,
            break_type: BreakType::Eof,
            pos: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
        };
        let result = pos_after_line(&line);
        assert_eq!(result.byte_offset, 5);
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 3);
    }
}
