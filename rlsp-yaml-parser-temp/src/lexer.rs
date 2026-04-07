// SPDX-License-Identifier: MIT

//! Line-level lexer: wraps [`LineBuffer`] and provides marker-detection and
//! line-consumption primitives consumed by the [`EventIter`] state machine.
//!
//! The lexer is lazy — it never buffers the whole input.  It advances through
//! the [`LineBuffer`] one line at a time, driven by the state machine.

use std::borrow::Cow;

use crate::lines::{Line, LineBuffer};
use crate::pos::{Pos, Span};

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// Line-level lexer over a `&'input str`.
///
/// Wraps a [`LineBuffer`] and exposes line-classification and consumption
/// primitives.  The `EventIter` state machine calls into this rather than
/// operating on the `LineBuffer` directly, keeping the grammar logic clean.
pub struct Lexer<'input> {
    buf: LineBuffer<'input>,
    /// Position after the last consumed line (or `Pos::ORIGIN` at start).
    current_pos: Pos,
    /// Inline scalar content following a `---` or `...` marker on the same
    /// line (e.g. `--- text`).  Populated by [`Self::consume_marker_line`]
    /// when the marker line has trailing content; drained by
    /// [`Self::try_consume_plain_scalar`] on the next call.
    inline_scalar: Option<(Cow<'input, str>, Span)>,
}

impl<'input> Lexer<'input> {
    /// Create a new `Lexer` over the given input.
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        Self {
            buf: LineBuffer::new(input),
            current_pos: Pos::ORIGIN,
            inline_scalar: None,
        }
    }

    /// Skip blank and comment-only lines, returning the position after the
    /// last consumed line (i.e. the position at the start of the first
    /// non-blank/non-comment line, or the end of input if all remaining lines
    /// are blank/comments).
    ///
    /// A line is blank-or-comment if its content is empty, whitespace-only,
    /// or begins (after optional leading whitespace) with `#`.
    ///
    /// Use this inside a document body (`InDocument`), where `%`-prefixed lines
    /// are regular content, not directives.
    pub fn skip_empty_lines(&mut self) -> Pos {
        loop {
            let skip = self
                .buf
                .peek_next()
                .is_some_and(|line| is_blank_or_comment(line));
            if skip {
                if let Some(line) = self.buf.consume_next() {
                    self.current_pos = pos_after_line(&line);
                }
            } else {
                return self.current_pos;
            }
        }
    }

    /// Skip blank, comment-only, and directive (`%`-prefixed) lines, returning
    /// the position after the last consumed line.
    ///
    /// Use this between documents (`BetweenDocs`), where `%YAML` / `%TAG` /
    /// unknown directives are stream-level metadata.  Full directive parsing is
    /// deferred to Task 18.
    pub fn skip_directives_and_blank_lines(&mut self) -> Pos {
        loop {
            let skip = self
                .buf
                .peek_next()
                .is_some_and(|line| is_directive_or_blank_or_comment(line));
            if skip {
                if let Some(line) = self.buf.consume_next() {
                    self.current_pos = pos_after_line(&line);
                }
            } else {
                return self.current_pos;
            }
        }
    }

    /// True if the currently-primed next line is a `---` (directives-end)
    /// marker.
    ///
    /// A line is a `---` marker when its content starts with `"---"` and the
    /// 4th byte (if any) is space or tab.  Column 0 is implicit: every line
    /// produced by [`LineBuffer`] starts at the beginning of a physical line.
    #[must_use]
    pub fn is_directives_end(&self) -> bool {
        self.buf
            .peek_next()
            .is_some_and(|line| is_marker(line.content, b'-'))
    }

    /// True if the currently-primed next line is a `...` (document-end)
    /// marker.
    ///
    /// Same rules as [`Self::is_directives_end`] but for `'.'`.
    #[must_use]
    pub fn is_document_end(&self) -> bool {
        self.buf
            .peek_next()
            .is_some_and(|line| is_marker(line.content, b'.'))
    }

    /// True if there is any remaining non-blank, non-comment line in the
    /// buffer (including the currently-primed line).
    #[must_use]
    pub fn has_content(&self) -> bool {
        self.buf
            .peek_next()
            .is_some_and(|line| !is_blank_or_comment(line))
    }

    /// Consume the currently-primed line as a marker line.
    ///
    /// Returns `(marker_pos, after_pos)` where:
    /// - `marker_pos` is the start position of the marker line
    /// - `after_pos` is the position immediately after the line terminator
    ///
    /// If the marker line carries inline content (e.g. `--- text`), that
    /// content is extracted as a plain scalar and stored in
    /// [`Self::inline_scalar`] for retrieval by the next call to
    /// [`Self::try_consume_plain_scalar`].
    ///
    /// The caller must ensure the current line is a marker (via
    /// [`Self::is_directives_end`] or [`Self::is_document_end`]) before
    /// calling this.
    pub fn consume_marker_line(&mut self) -> (Pos, Pos) {
        let line = self
            .buf
            .consume_next()
            .unwrap_or_else(|| panic!("consume_marker_line called at EOF"));
        let marker_pos = line.pos;
        let after = pos_after_line(&line);
        self.current_pos = after;

        // Extract inline content: `--- <content>` — the 4th byte is space/tab,
        // so content starts at offset 4 in the line.
        let inline = line
            .content
            .get(4..)
            .unwrap_or("")
            .trim_start_matches([' ', '\t']);
        if !inline.is_empty() {
            // Compute the start position of the inline content.
            // marker_pos is at column 0 of the line; inline content starts at
            // byte_offset = marker_pos.byte_offset + (content.len() - inline.len()).
            let prefix_len = line.content.len() - inline.len();
            let inline_start = Pos {
                byte_offset: marker_pos.byte_offset + prefix_len,
                char_offset: marker_pos.char_offset + prefix_len,
                line: marker_pos.line,
                column: marker_pos.column + prefix_len,
            };
            let scanned = scan_plain_line_block(inline);
            if !scanned.is_empty() {
                let mut inline_end = inline_start;
                for ch in scanned.chars() {
                    inline_end = inline_end.advance(ch);
                }
                self.inline_scalar = Some((
                    Cow::Borrowed(scanned),
                    Span {
                        start: inline_start,
                        end: inline_end,
                    },
                ));
            }
        }

        (marker_pos, after)
    }

    /// Consume the currently-primed line (any content) and advance position.
    pub fn consume_line(&mut self) {
        if let Some(line) = self.buf.consume_next() {
            self.current_pos = pos_after_line(&line);
        }
    }

    /// True when no more lines remain.
    ///
    /// Note: a true result here does **not** mean there is no remaining scalar
    /// content — an inline scalar from a preceding `--- text` marker may still
    /// be pending in [`Self::inline_scalar`].  Check
    /// [`Self::has_inline_scalar`] separately when needed.
    #[must_use]
    pub const fn at_eof(&self) -> bool {
        self.buf.at_eof()
    }

    /// True when an inline scalar extracted from a preceding `---` or `...`
    /// marker line is waiting to be consumed by [`Self::try_consume_plain_scalar`].
    #[must_use]
    pub const fn has_inline_scalar(&self) -> bool {
        self.inline_scalar.is_some()
    }

    /// Position after the last consumed line.
    ///
    /// Before any lines are consumed this is `Pos::ORIGIN`.  At EOF this is
    /// the position after the last byte of input.
    #[must_use]
    pub const fn current_pos(&self) -> Pos {
        self.current_pos
    }

    /// Drain all remaining lines and return the position after the last byte.
    pub fn drain_to_end(&mut self) -> Pos {
        while let Some(line) = self.buf.consume_next() {
            self.current_pos = pos_after_line(&line);
        }
        self.current_pos
    }

    /// Try to tokenize a plain scalar starting at the current line.
    ///
    /// Implements YAML 1.2 §7.3.3 `ns-plain` in block context.  The caller
    /// supplies `parent_indent` — the indentation level of the enclosing
    /// block node (`n` in the spec); continuation lines must have
    /// `indent >= parent_indent`.
    ///
    /// Returns `(value, span)` on success or `None` if the current line cannot
    /// start a plain scalar (EOF, blank/comment, or forbidden first character).
    ///
    /// **Borrow contract:** Single-line → `Cow::Borrowed` (zero allocation).
    /// Multi-line → `Cow::Owned` (one allocation for the folded value).
    ///
    /// If [`Self::inline_scalar`] is set (populated by a preceding
    /// [`Self::consume_marker_line`] call for a `--- text` line), it is
    /// drained and returned immediately without consuming any new lines.
    pub fn try_consume_plain_scalar(
        &mut self,
        parent_indent: usize,
    ) -> Option<(Cow<'input, str>, Span)> {
        // Drain any inline scalar stashed by consume_marker_line (e.g. `--- text`).
        if let Some(inline) = self.inline_scalar.take() {
            return Some(inline);
        }
        let (leading_spaces, scalar_start_pos, first_value_len) =
            peek_plain_scalar_first_line(&self.buf)?;

        let consumed_first = self
            .buf
            .consume_next()
            .unwrap_or_else(|| panic!("peek returned Some but consume returned None"));
        self.current_pos = pos_after_line(&consumed_first);

        let first_value_ref: &'input str = consumed_first
            .content
            .get(leading_spaces..leading_spaces + first_value_len)
            .unwrap_or_else(|| panic!("scalar slice out of bounds"));

        let extra = self.collect_plain_continuations(first_value_ref, parent_indent);

        let span_end = self.current_pos;
        Some(extra.map_or_else(
            || {
                let mut end_pos = scalar_start_pos;
                for ch in first_value_ref.chars() {
                    end_pos = end_pos.advance(ch);
                }
                (
                    Cow::Borrowed(first_value_ref),
                    Span {
                        start: scalar_start_pos,
                        end: end_pos,
                    },
                )
            },
            |owned| {
                (
                    Cow::Owned(owned),
                    Span {
                        start: scalar_start_pos,
                        end: span_end,
                    },
                )
            },
        ))
    }

    /// Collect continuation lines after the first line of a plain scalar.
    ///
    /// Returns `Some(String)` if any continuation lines were found (multi-line),
    /// or `None` if the scalar ends after the first line (single-line).
    fn collect_plain_continuations(
        &mut self,
        first_value_ref: &str,
        parent_indent: usize,
    ) -> Option<String> {
        let mut pending_blanks: usize = 0;
        let mut result: Option<String> = None;

        loop {
            let Some(next) = self.buf.peek_next() else {
                break;
            };
            let trimmed = next.content.trim_start_matches([' ', '\t']);

            if trimmed.is_empty() {
                pending_blanks += 1;
                let consumed = self
                    .buf
                    .consume_next()
                    .unwrap_or_else(|| panic!("consume blank line failed"));
                self.current_pos = pos_after_line(&consumed);
                continue;
            }

            if is_marker(next.content, b'-') || is_marker(next.content, b'.') {
                break;
            }

            if next.indent < parent_indent {
                break;
            }

            let cont_value = scan_plain_line_block(trimmed);
            if cont_value.is_empty() {
                break;
            }

            let buf = result.get_or_insert_with(|| String::from(first_value_ref));
            if pending_blanks > 0 {
                for _ in 0..pending_blanks {
                    buf.push('\n');
                }
                pending_blanks = 0;
            } else {
                buf.push(' ');
            }
            buf.push_str(cont_value);

            let consumed = self
                .buf
                .consume_next()
                .unwrap_or_else(|| panic!("consume cont line failed"));
            self.current_pos = pos_after_line(&consumed);
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Plain scalar first-line inspection
// ---------------------------------------------------------------------------

/// Peek at the next line in `buf` and determine whether it can start a plain
/// scalar in block context.
///
/// Returns `(leading_spaces, scalar_start_pos, first_value_len)` on success, or
/// `None` if the line cannot start a plain scalar.
fn peek_plain_scalar_first_line(buf: &LineBuffer<'_>) -> Option<(usize, Pos, usize)> {
    let first = buf.peek_next()?;

    if is_blank_or_comment(first) {
        return None;
    }

    let content_trimmed = first.content.trim_start_matches([' ', '\t']);
    if content_trimmed.is_empty() {
        return None;
    }

    let first_char = content_trimmed.chars().next()?;
    if !ns_plain_first_block(first_char, content_trimmed) {
        return None;
    }

    let first_value = scan_plain_line_block(content_trimmed);
    if first_value.is_empty() {
        return None;
    }

    let leading_spaces = first.content.len() - content_trimmed.len();
    let scalar_start_pos = Pos {
        byte_offset: first.offset + leading_spaces,
        char_offset: first.pos.char_offset + leading_spaces,
        line: first.pos.line,
        column: first.pos.column + leading_spaces,
    };

    Some((leading_spaces, scalar_start_pos, first_value.len()))
}

// ---------------------------------------------------------------------------
// Plain scalar character predicates (YAML 1.2 §7.3.3)
// ---------------------------------------------------------------------------

/// `ns-plain-first(c)` for block context: the first character of a plain scalar.
///
/// A character can start a plain scalar if:
/// - It is a non-indicator `ns-char`, OR
/// - It is `?`, `:`, or `-` AND the next character is a safe plain char.
///
/// YAML 1.2 spec [126]: `ns-plain-first(c) ::= (ns-char – c-indicator) |
///   ((? | : | -) Followed by ns-plain-safe(c))`
fn ns_plain_first_block(ch: char, rest: &str) -> bool {
    if is_c_indicator(ch) {
        // Special case: `?`, `:`, `-` are allowed if followed by a safe char.
        if matches!(ch, '?' | ':' | '-') {
            // Look at the character after `ch`.
            let after = &rest[ch.len_utf8()..];
            if let Some(next) = after.chars().next() {
                return ns_plain_safe_block(next);
            }
        }
        // Other indicators or indicator not followed by safe char.
        return false;
    }
    // Non-indicator ns-char.
    is_ns_char(ch)
}

/// `ns-plain-safe(c)` for block context: any `ns-char`.
///
/// In flow context this would additionally exclude flow indicators (Task 13).
const fn ns_plain_safe_block(ch: char) -> bool {
    is_ns_char(ch)
}

/// `ns-plain-char(c)` for block context: characters allowed in the body of a plain scalar.
///
/// Rules (YAML 1.2 [130]):
/// - Any `ns-plain-safe(c)` that is not `:` or `#`.
/// - `#` when the preceding character was not whitespace (i.e., `#` here means
///   a `:` or `#` character encountered in the middle of a run, which cannot
///   be whitespace-preceded since we only arrive here after consuming a
///   non-whitespace run).
/// - `:` when followed by an `ns-plain-safe(c)` character.
fn ns_plain_char_block(prev_was_ws: bool, ch: char, next: Option<char>) -> bool {
    if ch == '#' {
        // `#` is allowed only when NOT preceded by whitespace.
        return !prev_was_ws;
    }
    if ch == ':' {
        // `:` is allowed only when followed by a safe plain char.
        return next.is_some_and(ns_plain_safe_block);
    }
    ns_plain_safe_block(ch)
}

/// Scan a plain scalar from `content` (block context, after leading whitespace
/// has been stripped).
///
/// Returns the trimmed value slice (trailing whitespace stripped, comment
/// stripped if preceded by whitespace).
///
/// This implements `nb-ns-plain-in-line(c)` applied to the full line content
/// starting at the first non-space character position.
fn scan_plain_line_block(content: &str) -> &str {
    // We track: the end of the last committed non-whitespace run.
    // Whitespace is tentatively included but stripped if the line ends with it
    // or if `#` follows whitespace.
    let mut chars = content.char_indices().peekable();
    // Last committed byte offset (exclusive): the scalar ends here.
    let mut committed_end: usize = 0;
    // Whether the previous character was whitespace.
    let mut prev_was_ws = false;

    while let Some((i, ch)) = chars.next() {
        // Check for break characters (should never appear in line content, but
        // guard anyway).
        if matches!(ch, '\n' | '\r') {
            break;
        }

        if is_s_white(ch) {
            // Whitespace is tentative — don't advance committed_end yet.
            prev_was_ws = true;
            continue;
        }

        // Non-whitespace character: check if it terminates the scalar.
        let next_ch = chars.peek().map(|(_, c)| *c);

        if !ns_plain_char_block(prev_was_ws, ch, next_ch) {
            // This character cannot be part of the plain scalar.
            // The scalar ends at committed_end (before any pending whitespace).
            break;
        }

        // Character is valid — commit through this character.
        committed_end = i + ch.len_utf8();
        prev_was_ws = false;
    }

    &content[..committed_end]
}

// ---------------------------------------------------------------------------
// Character class predicates (YAML 1.2 §5)
// ---------------------------------------------------------------------------

/// `c-indicator` — all 21 YAML indicator characters.
const fn is_c_indicator(ch: char) -> bool {
    matches!(
        ch,
        '-' | '?'
            | ':'
            | ','
            | '['
            | ']'
            | '{'
            | '}'
            | '#'
            | '&'
            | '*'
            | '!'
            | '|'
            | '>'
            | '\''
            | '"'
            | '%'
            | '@'
            | '`'
    )
}

/// `ns-char` — printable non-whitespace non-BOM character.
const fn is_ns_char(ch: char) -> bool {
    !matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{FEFF}')
        && matches!(ch,
            '\x21'..='\x7E'
            | '\u{85}'
            | '\u{A0}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
        )
}

/// `s-white` — space or tab.
const fn is_s_white(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// True when `line` is blank (empty or whitespace-only) or comment-only.
///
/// Does **not** treat `%`-prefixed lines as skippable — inside a document body
/// a `%`-prefixed line is regular content (e.g. `%complete: 50`).
fn is_blank_or_comment(line: &Line<'_>) -> bool {
    let trimmed = line.content.trim_start_matches([' ', '\t']);
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// True when `line` is blank, comment-only, or a directive (`%`-prefixed).
///
/// Directive lines (`%YAML`, `%TAG`, and unknown `%` directives) are
/// stream-level metadata that precede `---`.  This predicate is only correct
/// to use in the between-documents context; inside a document body `%`-prefixed
/// lines are content and must be handled by [`is_blank_or_comment`] instead.
///
/// TODO(Task 18): This predicate currently skips ALL `%`-prefixed lines in
/// `BetweenDocs`.  Task 18 will add full directive grammar parsing per YAML §6.8,
/// which will distinguish valid directives (`%YAML`, `%TAG`, etc.) from
/// malformed `%`-prefixed lines that should error or be treated as bare-doc
/// content.  Until then, any `%`-prefixed line in `BetweenDocs` is silently
/// treated as a directive.
fn is_directive_or_blank_or_comment(line: &Line<'_>) -> bool {
    if is_blank_or_comment(line) {
        return true;
    }
    let trimmed = line.content.trim_start_matches([' ', '\t']);
    trimmed.starts_with('%')
}

/// True when `content` is a YAML document marker for the given byte `ch`
/// (`b'-'` for `---`, `b'.'` for `...`).
///
/// Rules (YAML 1.2 §9.1 / c-forbidden):
/// - Must start with exactly three occurrences of `ch`
/// - The 4th byte, if present, must be space (0x20) or tab (0x09)
/// - `"---word"` is NOT a marker; `"--- word"` IS a marker
fn is_marker(content: &str, ch: u8) -> bool {
    let bytes = content.as_bytes();
    // Need at least three bytes for the marker.
    if bytes.len() < 3 {
        return false;
    }
    // All three bytes must match `ch`.  Length is checked above so .get() is
    // used to satisfy the indexing_slicing lint.
    let Some((&b0, &b1, &b2)) = bytes
        .first()
        .zip(bytes.get(1))
        .zip(bytes.get(2))
        .map(|((a, b), c)| (a, b, c))
    else {
        return false;
    };
    if b0 != ch || b1 != ch || b2 != ch {
        return false;
    }
    // The 4th byte, if present, must be space or tab.
    matches!(bytes.get(3), None | Some(&b' ' | &b'\t'))
}

/// Compute the `Pos` immediately after the terminator of `line`.
pub fn pos_after_line(line: &Line<'_>) -> Pos {
    let mut pos = line.pos;
    for ch in line.content.chars() {
        pos = pos.advance(ch);
    }
    line.break_type.advance(pos)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_lexer(input: &str) -> Lexer<'_> {
        Lexer::new(input)
    }

    // -----------------------------------------------------------------------
    // Group A — is_directives_end
    // -----------------------------------------------------------------------

    #[test]
    fn directives_end_exact_three_dashes() {
        let lex = make_lexer("---");
        assert!(lex.is_directives_end());
    }

    #[test]
    fn directives_end_followed_by_space() {
        let lex = make_lexer("--- ");
        assert!(lex.is_directives_end());
    }

    #[test]
    fn directives_end_followed_by_tab() {
        let lex = make_lexer("---\t");
        assert!(lex.is_directives_end());
    }

    #[test]
    fn directives_end_false_for_word_attached() {
        let lex = make_lexer("---word");
        assert!(!lex.is_directives_end());
    }

    #[test]
    fn directives_end_false_for_partial_dashes() {
        let lex = make_lexer("--");
        assert!(!lex.is_directives_end());
    }

    #[test]
    fn directives_end_false_for_empty_line() {
        let lex = make_lexer("");
        assert!(!lex.is_directives_end());
    }

    // -----------------------------------------------------------------------
    // Group B — is_document_end
    // -----------------------------------------------------------------------

    #[test]
    fn document_end_exact_three_dots() {
        let lex = make_lexer("...");
        assert!(lex.is_document_end());
    }

    #[test]
    fn document_end_followed_by_space() {
        let lex = make_lexer("... ");
        assert!(lex.is_document_end());
    }

    #[test]
    fn document_end_false_for_word_attached() {
        let lex = make_lexer("...word");
        assert!(!lex.is_document_end());
    }

    #[test]
    fn document_end_false_for_partial_dots() {
        let lex = make_lexer("..");
        assert!(!lex.is_document_end());
    }

    // -----------------------------------------------------------------------
    // Group C — skip_empty_lines
    // -----------------------------------------------------------------------

    #[test]
    fn skip_empty_lines_advances_past_blank_line() {
        let mut lex = make_lexer("\n---");
        lex.skip_empty_lines();
        assert!(lex.is_directives_end());
    }

    #[test]
    fn skip_empty_lines_returns_pos_after_consumed_lines() {
        let mut lex = make_lexer("\n\n---");
        let pos = lex.skip_empty_lines();
        assert_eq!(pos.byte_offset, 2);
    }

    #[test]
    fn skip_empty_lines_skips_comment_lines() {
        let mut lex = make_lexer("# comment\n---");
        lex.skip_empty_lines();
        assert!(lex.is_directives_end());
    }

    #[test]
    fn skip_empty_lines_on_empty_input_returns_origin_pos() {
        let mut lex = make_lexer("");
        let pos = lex.skip_empty_lines();
        assert_eq!(pos, Pos::ORIGIN);
    }

    #[test]
    fn skip_empty_lines_leaves_content_line_untouched() {
        let mut lex = make_lexer("content");
        lex.skip_empty_lines();
        assert!(lex.has_content());
    }

    // -----------------------------------------------------------------------
    // Group D — consume_marker_line
    // -----------------------------------------------------------------------

    #[test]
    fn consume_marker_line_returns_marker_pos_and_after_pos() {
        let mut lex = make_lexer("---\n");
        let (marker_pos, after_pos) = lex.consume_marker_line();
        assert_eq!(marker_pos.byte_offset, 0);
        assert_eq!(after_pos.byte_offset, 4);
    }

    #[test]
    fn consume_marker_line_advances_lexer_past_line() {
        let mut lex = make_lexer("---\nnext");
        lex.consume_marker_line();
        assert!(lex.buf.peek_next().is_some_and(|l| l.content == "next"));
    }

    // -----------------------------------------------------------------------
    // Group E — has_content / drain_to_end
    // -----------------------------------------------------------------------

    #[test]
    fn has_content_false_for_empty_input() {
        let lex = make_lexer("");
        assert!(!lex.has_content());
    }

    #[test]
    fn has_content_false_for_blank_and_comment_lines_only() {
        let lex = make_lexer("\n# comment\n   \n");
        assert!(!lex.has_content());
    }

    #[test]
    fn has_content_true_when_non_blank_line_present() {
        let lex = make_lexer("foo");
        assert!(lex.has_content());
    }

    #[test]
    fn drain_to_end_returns_pos_after_last_byte() {
        let mut lex = make_lexer("abc\n");
        let pos = lex.drain_to_end();
        assert_eq!(pos.byte_offset, 4);
    }

    // -----------------------------------------------------------------------
    // Group F — predicate unit tests (UT-22, UT-23)
    // -----------------------------------------------------------------------
    // Lock in the directive-context split: is_blank_or_comment must NOT skip
    // `%`-prefixed lines (they are content inside a document body), while
    // is_directive_or_blank_or_comment must skip them (BetweenDocs context).

    #[test]
    fn is_blank_or_comment_does_not_skip_directive_lines() {
        // UT-22: Regression — `%`-prefixed lines are content in InDocument.
        // If this predicate ever starts returning true for `%`-lines, Task 6
        // scalar events for `%foo: bar` inside a document will be silently
        // dropped.
        let Some(line) = LineBuffer::new("%foo: bar").consume_next() else {
            panic!("LineBuffer produced no line for non-empty input");
        };
        assert!(!is_blank_or_comment(&line));
    }

    #[test]
    fn is_directive_or_blank_or_comment_skips_directive_lines() {
        // UT-23: The BetweenDocs predicate must skip `%`-prefixed lines.
        // Full directive grammar (Task 18) will distinguish valid directives
        // from bare-doc content; until then, all `%`-lines are skipped here.
        let Some(line) = LineBuffer::new("%YAML 1.2").consume_next() else {
            panic!("LineBuffer produced no line for non-empty input");
        };
        assert!(is_directive_or_blank_or_comment(&line));
    }

    // -----------------------------------------------------------------------
    // Group G — try_consume_plain_scalar unit tests (Task 6)
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_single_word() {
        let mut lex = make_lexer("hello");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "hello");
    }

    #[test]
    fn plain_scalar_multi_word() {
        let mut lex = make_lexer("hello world");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "hello world");
    }

    #[test]
    fn plain_scalar_cow_borrowed_for_single_line() {
        let mut lex = make_lexer("hello");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert!(
            matches!(val, Cow::Borrowed(_)),
            "single-line must be Borrowed"
        );
    }

    #[test]
    fn plain_scalar_cow_owned_for_multiline() {
        let mut lex = make_lexer("foo\n  bar");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert!(matches!(val, Cow::Owned(_)), "multi-line must be Owned");
        assert_eq!(val, "foo bar");
    }

    #[test]
    fn plain_scalar_with_url() {
        // `:` not followed by space → allowed inside plain scalar.
        let mut lex = make_lexer("http://x.com");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "http://x.com");
    }

    #[test]
    fn plain_scalar_with_hash_no_preceding_space() {
        // `#` not preceded by whitespace → allowed inside plain scalar.
        let mut lex = make_lexer("a#b");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "a#b");
    }

    #[test]
    fn plain_scalar_terminated_by_colon_space() {
        // `: ` (colon + space) terminates the scalar — the colon is not safe.
        let mut lex = make_lexer("key: value");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "key");
    }

    #[test]
    fn plain_scalar_terminated_by_hash_with_space() {
        // ` #` (space + hash) terminates the scalar — `#` preceded by whitespace.
        let mut lex = make_lexer("foo # comment");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_trailing_whitespace_stripped() {
        // Trailing spaces on a line are not part of the scalar value.
        let mut lex = make_lexer("foo   ");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_multiline_folds_single_break_to_space() {
        let mut lex = make_lexer("foo\n  bar\n  baz");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo bar baz");
    }

    #[test]
    fn plain_scalar_multiline_blank_line_folds_to_newline() {
        // A blank line in the middle of a multi-line scalar becomes a newline.
        let mut lex = make_lexer("foo\n\nbar");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo\nbar");
    }

    #[test]
    fn plain_scalar_empty_input_returns_none() {
        let mut lex = make_lexer("");
        assert!(lex.try_consume_plain_scalar(0).is_none());
    }

    #[test]
    fn plain_scalar_blank_line_returns_none() {
        let mut lex = make_lexer("   ");
        assert!(lex.try_consume_plain_scalar(0).is_none());
    }

    #[test]
    fn plain_scalar_comment_line_returns_none() {
        let mut lex = make_lexer("# comment");
        assert!(lex.try_consume_plain_scalar(0).is_none());
    }

    #[test]
    fn plain_scalar_indicator_chars_return_none() {
        // These characters cannot start a plain scalar when not followed by safe chars.
        // Standalone indicators at the start of a line.
        for indicator in &[
            "[", "{", "&", "!", "*", ":", "?", "-", "|", ">", "'", "\"", "#", "%", ",", "]", "}",
        ] {
            let mut lex = make_lexer(indicator);
            let result = lex.try_consume_plain_scalar(0);
            assert!(
                result.is_none(),
                "indicator '{indicator}' should not start a plain scalar"
            );
        }
    }

    #[test]
    fn plain_scalar_minus_followed_by_safe_char_is_valid() {
        // `-a` starts a plain scalar (ns-plain-first allows `-` + ns-plain-safe).
        let mut lex = make_lexer("-a");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "-a");
    }

    #[test]
    fn plain_scalar_colon_followed_by_safe_char_is_valid() {
        // `:a` starts a plain scalar.
        let mut lex = make_lexer(":a");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, ":a");
    }

    #[test]
    fn plain_scalar_forbidden_continuation_stops_at_marker() {
        // A `---` marker at column 0 terminates multi-line continuation.
        let mut lex = make_lexer("foo\n---\nbar");
        // Only "foo" should be collected (the --- terminates the scalar).
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_span_start_byte_offset() {
        let mut lex = make_lexer("hello");
        let (_, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(span.start.byte_offset, 0);
    }

    #[test]
    fn plain_scalar_span_end_byte_offset() {
        // "hello" = 5 bytes; span.end should be at byte offset 5.
        let mut lex = make_lexer("hello");
        let (_, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(span.end.byte_offset, 5);
    }

    #[test]
    fn plain_scalar_indented_start_span_byte_offset() {
        // "  hello" — leading 2 spaces, scalar starts at byte 2.
        let mut lex = make_lexer("  hello");
        let (val, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "hello");
        assert_eq!(span.start.byte_offset, 2);
    }

    #[test]
    fn plain_scalar_with_multibyte_utf8() {
        // '中' (3 bytes) should be consumed as a valid plain scalar.
        let mut lex = make_lexer("中文");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "中文");
    }

    #[test]
    fn plain_scalar_dedented_continuation_stops() {
        // A line at indent < parent_indent stops continuation.
        // For parent_indent=2: "  foo\nbar" — bar at indent 0 < 2, terminates.
        let mut lex = make_lexer("  foo\nbar");
        let (val, _) = lex
            .try_consume_plain_scalar(2)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_with_backslashes() {
        // Backslashes are not special in plain scalars.
        let mut lex = make_lexer("plain\\value\\with\\backslashes");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "plain\\value\\with\\backslashes");
    }

    // -----------------------------------------------------------------------
    // Group B (TE additions) — colon termination edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_colon_tab_terminates() {
        // `:`+tab is not ns-plain-safe (tab is s-white, not ns-char) → terminates.
        let mut lex = make_lexer("key:\tvalue");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "key");
    }

    #[test]
    fn plain_scalar_colon_eof_terminates() {
        // `:`+EOF: next char is None → ns_plain_char_block returns false → `:` not included.
        let mut lex = make_lexer("key:");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "key");
    }

    // -----------------------------------------------------------------------
    // Group C (TE additions) — hash with tab preceding
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_hash_preceded_by_tab_terminates() {
        // tab before `#` — tab is s-white, so `#` is whitespace-preceded → terminates.
        let mut lex = make_lexer("foo\t# comment");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo");
    }

    // -----------------------------------------------------------------------
    // Group D (TE additions) — multi-line folding edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_multiline_two_blank_lines_fold_to_two_newlines() {
        // Two blank lines in the middle: N blank lines → N newlines.
        let mut lex = make_lexer("foo\n\n\nbar");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo\n\nbar");
    }

    #[test]
    fn plain_scalar_multiline_continuation_trailing_space_stripped() {
        // Trailing space on a continuation line is stripped before folding.
        let mut lex = make_lexer("foo\nbar   \nbaz");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo bar baz");
    }

    // -----------------------------------------------------------------------
    // Group F (TE additions) — c-forbidden disambiguation
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_dots_terminated_by_document_end_marker() {
        // `...` at column 0 terminates the plain scalar.
        let mut lex = make_lexer("foo\n...\nbar");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_dash_dash_dash_word_attached_is_not_forbidden() {
        // `---word` at column 0 is NOT a c-forbidden marker — it's a valid continuation.
        let mut lex = make_lexer("foo\n---word");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo ---word");
    }

    // -----------------------------------------------------------------------
    // Group H (TE additions) — indicator chars that need safe-char context
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_dash_space_returns_none() {
        // `- ` is a block sequence entry indicator, not a plain scalar start.
        let mut lex = make_lexer("- ");
        assert!(lex.try_consume_plain_scalar(0).is_none());
    }

    #[test]
    fn plain_scalar_question_space_returns_none() {
        // `? ` is a mapping key indicator.
        let mut lex = make_lexer("? ");
        assert!(lex.try_consume_plain_scalar(0).is_none());
    }

    #[test]
    fn plain_scalar_colon_space_returns_none() {
        // `: ` is a mapping value indicator.
        let mut lex = make_lexer(": ");
        assert!(lex.try_consume_plain_scalar(0).is_none());
    }

    // -----------------------------------------------------------------------
    // Group I (TE additions) — span byte offsets with multi-byte UTF-8
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_multibyte_utf8_span_byte_offset() {
        // '中' = U+4E2D = 3 UTF-8 bytes; '文' = U+6587 = 3 UTF-8 bytes.
        // "中文" = 6 bytes; span should be [0, 6).
        let mut lex = make_lexer("中文");
        let (val, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "中文");
        assert_eq!(span.start.byte_offset, 0);
        assert_eq!(span.end.byte_offset, 6);
    }

    #[test]
    fn plain_scalar_multibyte_utf8_with_leading_space_span() {
        // "  中" — 2-byte prefix, then 3-byte char; scalar starts at byte 2.
        let mut lex = make_lexer("  中");
        let (val, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "中");
        assert_eq!(span.start.byte_offset, 2);
        assert_eq!(span.end.byte_offset, 5);
    }

    // -----------------------------------------------------------------------
    // Group F (TE required) — exact name from TE spec
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_forbidden_dot_dot_dot_at_col_0_terminates() {
        // `...` at column 0 terminates multi-line plain scalar continuation.
        // Covers the b'.' arm of `is_marker` in collect_plain_continuations.
        let mut lex = make_lexer("foo\n...\nbar");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(val, "foo");
    }

    // -----------------------------------------------------------------------
    // Group D (TE required) — exact name and input from TE spec
    // -----------------------------------------------------------------------

    // Note: plain_scalar_multiline_two_blank_lines_fold_to_two_newlines
    // exists above with input "foo\n\n\nbar". The TE spec input is
    // "foo\n\n\n  bar" (indented continuation). Adding the TE's exact variant:
    #[test]
    fn plain_scalar_multiline_two_blank_lines_fold_to_two_newlines_indented() {
        // Two blank lines + indented continuation: "foo\n\n\n  bar" → "foo\n\nbar"
        let mut lex = make_lexer("foo\n\n\n  bar");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert!(matches!(val, Cow::Owned(_)), "multi-line must be Owned");
        assert_eq!(val, "foo\n\nbar");
    }

    // -----------------------------------------------------------------------
    // Group I (TE required) — exact name from TE spec
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_multibyte_span_byte_offset() {
        // "中文" = 6 UTF-8 bytes, 2 chars. Span width must equal byte count.
        let mut lex = make_lexer("中文");
        let (_, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse"));
        assert_eq!(span.end.byte_offset - span.start.byte_offset, 6);
    }

    // -----------------------------------------------------------------------
    // Group G extension — inline scalar after --- marker
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_inline_after_marker_is_extracted() {
        // `--- text` — after consuming the marker line, try_consume_plain_scalar
        // returns the inline content "text".
        let mut lex = make_lexer("--- text");
        lex.consume_marker_line();
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse inline scalar"));
        assert_eq!(val, "text");
    }

    #[test]
    fn plain_scalar_inline_after_marker_is_cow_borrowed() {
        // Inline content from `---` line is a zero-copy borrowed slice.
        let mut lex = make_lexer("--- text");
        lex.consume_marker_line();
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| panic!("should parse inline scalar"));
        assert!(
            matches!(val, Cow::Borrowed(_)),
            "inline scalar from marker line must be Cow::Borrowed"
        );
    }
}
