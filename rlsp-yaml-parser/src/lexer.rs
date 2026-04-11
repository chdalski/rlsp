// SPDX-License-Identifier: MIT

//! Line-level lexer: wraps [`LineBuffer`] and provides marker-detection and
//! line-consumption primitives consumed by the [`EventIter`] state machine.
//!
//! The lexer is lazy — it never buffers the whole input.  It advances through
//! the [`LineBuffer`] one line at a time, driven by the state machine.

use std::borrow::Cow;

use crate::error::Error;
use crate::lines::{Line, LineBuffer, pos_after_line};
use crate::pos::{Pos, Span};

mod block;
mod comment;
mod plain;
mod quoted;

pub use crate::chars::is_ns_char;
pub use plain::scan_plain_line_flow;

use block::parse_block_header;
use plain::scan_plain_line_block;

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// Line-level lexer over a `&'input str`.
///
/// Wraps a [`LineBuffer`] and exposes line-classification and consumption
/// primitives.  The `EventIter` state machine calls into this rather than
/// operating on the `LineBuffer` directly, keeping the grammar logic clean.
pub struct Lexer<'input> {
    pub(super) buf: LineBuffer<'input>,
    /// Position after the last consumed line (or `Pos::ORIGIN` at start).
    pub(super) current_pos: Pos,
    /// Inline scalar content following a `---` or `...` marker on the same
    /// line (e.g. `--- text`).  Populated by [`Self::consume_marker_line`]
    /// when the marker line has trailing content; drained by
    /// [`Self::try_consume_plain_scalar`] on the next call.
    pub(super) inline_scalar: Option<(Cow<'input, str>, Span)>,
    /// Trailing comment found on the same line as a plain scalar.
    ///
    /// Populated by [`Self::try_consume_plain_scalar`] and by the mapping/
    /// sequence entry consumers when a `# comment` follows the scalar content
    /// on the same line.  Drained by the state machine after emitting the
    /// scalar event.
    pub trailing_comment: Option<(&'input str, Span)>,
    /// Content that follows the closing quote of a multiline double- or
    /// single-quoted scalar (on the same line as the closing quote).
    ///
    /// Set by [`Self::try_consume_double_quoted`] and
    /// [`Self::try_consume_single_quoted`] when the scalar spans multiple
    /// lines.  Drained by the flow parser after calling those methods, which
    /// prepends the tail as a synthetic line so the flow parser can continue
    /// processing `,`, `]`, `}`, etc. that follow the closing quote.
    pub pending_multiline_tail: Option<(&'input str, Pos)>,
    /// Error detected in the suffix of a plain scalar (content after the
    /// scalar value that is neither whitespace nor a valid trailing comment).
    ///
    /// Set by [`Self::try_consume_plain_scalar`] when an invalid suffix is
    /// found (e.g. a NUL byte or mid-stream BOM).  Drained by the caller
    /// after the scalar event is emitted, so the error is reported after the
    /// scalar rather than instead of it.
    pub plain_scalar_suffix_error: Option<Error>,
    /// Error produced by [`Self::consume_marker_line`] when the marker line
    /// carries inline content that cannot be parsed (e.g. `--- !tag` where
    /// `!` starts a tag indicator that cannot start a plain scalar, or
    /// `--- key: value` where `: ` occurs after the plain-scanned prefix,
    /// or any inline on a `...` marker which never permits inline content).
    ///
    /// Drained by the callers in `lib.rs` immediately after calling
    /// `consume_marker_line`.
    pub marker_inline_error: Option<Error>,
}

impl<'input> Lexer<'input> {
    /// Create a new `Lexer` over the given input.
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        Self {
            buf: LineBuffer::new(input),
            current_pos: Pos::ORIGIN,
            inline_scalar: None,
            trailing_comment: None,
            pending_multiline_tail: None,
            plain_scalar_suffix_error: None,
            marker_inline_error: None,
        }
    }

    /// Skip blank lines (empty or whitespace-only), stopping at comment lines.
    ///
    /// Returns the position after the last consumed line.
    ///
    /// Unlike the previous behaviour, this does **not** consume comment lines —
    /// they are yielded as `Event::Comment` by the state machine.
    ///
    /// Use this inside a document body (`InDocument`), where `%`-prefixed lines
    /// are regular content, not directives.
    pub fn skip_empty_lines(&mut self) -> Pos {
        loop {
            let skip = self
                .buf
                .peek_next()
                .is_some_and(|line| is_blank_not_comment(line));
            if skip {
                if let Some(line) = self.buf.consume_next() {
                    self.current_pos = pos_after_line(&line);
                }
            } else {
                return self.current_pos;
            }
        }
    }

    /// Skip blank lines between documents, stopping at directive (`%`), comment
    /// (`#`), content, or marker lines.
    ///
    /// Use this between documents (`BetweenDocs`) after directive and comment
    /// lines have already been consumed by the caller.
    pub fn skip_blank_lines_between_docs(&mut self) -> Pos {
        loop {
            let skip = self
                .buf
                .peek_next()
                .is_some_and(|line| is_blank_not_comment(line));
            if skip {
                if let Some(line) = self.buf.consume_next() {
                    self.current_pos = pos_after_line(&line);
                }
            } else {
                return self.current_pos;
            }
        }
    }

    /// True when the next line is a directive (`%`-prefixed).
    #[must_use]
    pub fn is_directive_line(&self) -> bool {
        self.buf
            .peek_next()
            .is_some_and(|line| line.content.starts_with('%'))
    }

    /// Try to consume the next line as a directive.
    ///
    /// Returns `Some((directive_content, start_pos))` when the next line starts
    /// with `%`, where `directive_content` is the full line content (including
    /// the `%`).  Returns `None` when the next line is not a directive.
    pub fn try_consume_directive_line(&mut self) -> Option<(&'input str, Pos)> {
        let line = self.buf.peek_next()?;
        if !line.content.starts_with('%') {
            return None;
        }
        let start_pos = line.pos;
        let content: &'input str = line.content;
        // SAFETY: peek succeeded above; LineBuffer invariant.
        let Some(consumed) = self.buf.consume_next() else {
            unreachable!("try_consume_directive_line: peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&consumed);
        Some((content, start_pos))
    }

    /// True when the next line is a comment (starts with `#` after optional
    /// leading whitespace).
    #[must_use]
    pub fn is_comment_line(&self) -> bool {
        self.buf.peek_next().is_some_and(|line| {
            let trimmed = line.content.trim_start_matches([' ', '\t']);
            trimmed.starts_with('#')
        })
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
    /// Consume the currently-primed marker line (`---` or `...`).
    ///
    /// `reject_all_inline` — when `true` (used for `...` markers), any
    /// non-comment inline content after the marker is flagged as an error in
    /// [`Self::marker_inline_error`].  When `false` (used for `---` markers),
    /// a valid plain scalar may appear inline; invalid inline content (an
    /// indicator character that cannot start a plain scalar, or residual
    /// content after the scanned scalar) is still flagged.
    pub fn consume_marker_line(&mut self, reject_all_inline: bool) -> (Pos, Pos) {
        // SAFETY: caller must verify via is_directives_end() or is_document_end()
        // before calling — the state machine enforces this precondition.
        let Some(line) = self.buf.consume_next() else {
            unreachable!("consume_marker_line called at EOF")
        };
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
            let prefix_bytes = line.content.len() - inline.len();
            let prefix_chars = crate::pos::column_at(line.content, prefix_bytes);
            let inline_start = Pos {
                byte_offset: marker_pos.byte_offset + prefix_bytes,
                line: marker_pos.line,
                column: marker_pos.column + prefix_chars,
            };

            // If the inline content is a comment (`# ...`), store it as a
            // trailing comment on the marker line rather than as a scalar.
            if let Some(comment_text) = inline.strip_prefix('#') {
                let comment_end =
                    crate::pos::advance_within_line(inline_start.advance('#'), comment_text);
                self.trailing_comment = Some((
                    comment_text,
                    Span {
                        start: inline_start,
                        end: comment_end,
                    },
                ));
            } else if reject_all_inline {
                // `...` markers must not have non-comment inline content.
                self.marker_inline_error = Some(Error {
                    pos: inline_start,
                    message: "invalid content after document-end marker '...'".into(),
                });
            } else {
                // Detect block scalar indicators (`|` / `>`) in inline position.
                // `scan_plain_line_block` below would mis-scan `|0` or `|10` as
                // plain scalars; instead validate the block header eagerly so that
                // invalid indicators (indent 0, double-digit, duplicate markers)
                // produce the correct parse error.
                if let Some(after_pipe) = inline
                    .strip_prefix('|')
                    .or_else(|| inline.strip_prefix('>'))
                {
                    let (_, _, header_err) = parse_block_header(after_pipe, inline_start);
                    if let Some(e) = header_err {
                        self.marker_inline_error = Some(e);
                        return (marker_pos, after);
                    }
                    // Valid block scalar header — fall through to the plain-scalar path
                    // so it is stashed as inline_scalar for the event emitter.
                    // (The body on subsequent lines is handled by the normal scalar
                    // dispatch path after DocumentStart is emitted.)
                }
                // TODO(architecture): scan_plain_line_block only tokenizes plain scalars.
                // Inline content after `---` that starts with `'` or `"` (Task 7) is
                // currently emitted as a Plain scalar with the quotes as literal chars.
                // Same gap exists for `|` and `>` (Tasks 8/9) and flow collections
                // (Task 13). Fix candidate: restructure to dispatch via the normal
                // scalar try-chain instead of pre-extracting. Deferred because it
                // requires re-running the security review for escape handling.
                let scanned = scan_plain_line_block(inline);
                if scanned.is_empty() {
                    // First character cannot start a plain scalar (e.g. `&`, `!`,
                    // `*`, `%`, `{`, `[`) — invalid inline content after `---`.
                    self.marker_inline_error = Some(Error {
                        pos: inline_start,
                        message: "invalid content after document-start marker '---'".into(),
                    });
                } else {
                    // Check for residual content after the plain scalar (e.g.
                    // `--- key: value` where `: value` is left over).  Any
                    // non-whitespace residual that is not a comment is invalid.
                    let residual = inline[scanned.len()..].trim_start_matches([' ', '\t']);
                    if !residual.is_empty() && !residual.starts_with('#') {
                        self.marker_inline_error = Some(Error {
                            pos: inline_start,
                            message: "invalid content after document-start marker '---'".into(),
                        });
                    } else {
                        let inline_end = crate::pos::advance_within_line(inline_start, scanned);
                        self.inline_scalar = Some((
                            Cow::Borrowed(scanned),
                            Span {
                                start: inline_start,
                                end: inline_end,
                            },
                        ));
                    }
                }
            }
        }

        (marker_pos, after)
    }

    /// Peek at the next line without consuming it.
    ///
    /// Returns the prepended synthetic line first (if any), then the normally
    /// buffered next line.  The returned reference is valid until the next
    /// call to any method that consumes or modifies the buffer.
    #[must_use]
    pub fn peek_next_line(&self) -> Option<&Line<'input>> {
        self.buf.peek_next()
    }

    /// Peek at the second upcoming line without consuming either.
    ///
    /// Returns `None` if fewer than two lines remain.
    #[must_use]
    pub fn peek_second_line(&self) -> Option<Line<'input>> {
        self.buf.peek_second()
    }

    /// Prepend a synthetic line to the front of the buffer.
    ///
    /// Used to re-present inline content extracted from a sequence-entry line
    /// (e.g. `- item` from `- - item\n`) as if it were a separate next line.
    pub fn prepend_inline_line(&mut self, line: Line<'input>) {
        self.buf.prepend_line(line);
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
    pub fn at_eof(&self) -> bool {
        self.buf.at_eof()
    }

    /// True when an inline scalar extracted from a preceding `---` or `...`
    /// marker line is waiting to be consumed by [`Self::try_consume_plain_scalar`].
    #[must_use]
    pub const fn has_inline_scalar(&self) -> bool {
        self.inline_scalar.is_some()
    }

    /// Return a reference to the pending inline scalar (value, start position)
    /// without consuming it, or `None` if there is no pending inline scalar.
    #[must_use]
    pub fn peek_inline_scalar(&self) -> Option<(&str, Pos)> {
        self.inline_scalar
            .as_ref()
            .map(|(v, span)| (v.as_ref(), span.start))
    }

    /// Discard the pending inline scalar without emitting it.
    pub fn drain_inline_scalar(&mut self) {
        self.inline_scalar = None;
    }

    /// Inject an inline scalar for testing — simulates the state left by
    /// `consume_marker_line` after parsing `--- text`.
    #[cfg(test)]
    pub fn set_inline_scalar_for_test(&mut self, value: Cow<'input, str>, span: crate::pos::Span) {
        self.inline_scalar = Some((value, span));
    }

    /// True when the next line in the buffer is a synthetic line prepended by
    /// the parser (e.g. inline key content from `? key` or `- item`), rather
    /// than a raw line from the original input stream.
    #[must_use]
    pub fn is_next_line_synthetic(&self) -> bool {
        self.buf.is_next_synthetic()
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
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// True when `line` is blank (empty or whitespace-only) but NOT a comment.
///
/// Used by [`Lexer::skip_empty_lines`] which stops at comment lines so the
/// state machine can emit `Event::Comment` for them.
fn is_blank_not_comment(line: &Line<'_>) -> bool {
    line.content.trim_start_matches([' ', '\t']).is_empty()
}

/// True when `line` is blank (empty or whitespace-only) or comment-only.
///
/// Does **not** treat `%`-prefixed lines as skippable — inside a document body
/// a `%`-prefixed line is regular content (e.g. `%complete: 50`).
///
/// Used by [`Lexer::has_content`] which must return `false` for both blank
/// and comment-only lines.
pub fn is_blank_or_comment(line: &Line<'_>) -> bool {
    let trimmed = line.content.trim_start_matches([' ', '\t']);
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// True when `content` is a YAML document marker for the given byte `ch`
/// (`b'-'` for `---`, `b'.'` for `...`).
///
/// Rules (YAML 1.2 §9.1 / c-forbidden):
/// - Must start with exactly three occurrences of `ch`
/// - The 4th byte, if present, must be space (0x20) or tab (0x09)
/// - `"---word"` is NOT a marker; `"--- word"` IS a marker
pub fn is_marker(content: &str, ch: u8) -> bool {
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

/// Return `true` if `content` is a document-start (`---`) or document-end
/// (`...`) marker at column 0.
///
/// Used to detect forbidden markers inside multi-line quoted scalars.
pub fn is_doc_marker_line(content: &str) -> bool {
    is_marker(content, b'-') || is_marker(content, b'.')
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// True when `line` is blank, comment-only, or a directive (`%`-prefixed).
    ///
    /// Directive lines (`%YAML`, `%TAG`, and unknown `%` directives) are
    /// stream-level metadata that precede `---`.  This predicate is only correct
    /// to use in the between-documents context; inside a document body `%`-prefixed
    /// lines are content and must be handled by [`is_blank_or_comment`] instead.
    fn is_directive_or_blank_or_comment(line: &Line<'_>) -> bool {
        if is_blank_or_comment(line) {
            return true;
        }
        let trimmed = line.content.trim_start_matches([' ', '\t']);
        trimmed.starts_with('%')
    }

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
    fn skip_empty_lines_stops_at_comment_lines() {
        // skip_empty_lines now stops at comment lines so the state machine can
        // emit Event::Comment for them.  A comment line is NOT consumed.
        let mut lex = make_lexer("# comment\n---");
        lex.skip_empty_lines();
        assert!(lex.is_comment_line(), "expected to stop at comment line");
        assert!(!lex.is_directives_end());
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
        let (marker_pos, after_pos) = lex.consume_marker_line(false);
        assert_eq!(marker_pos.byte_offset, 0);
        assert_eq!(after_pos.byte_offset, 4);
    }

    #[test]
    fn consume_marker_line_advances_lexer_past_line() {
        let mut lex = make_lexer("---\nnext");
        lex.consume_marker_line(false);
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
            unreachable!("LineBuffer produced no line for non-empty input")
        };
        assert!(!is_blank_or_comment(&line));
    }

    #[test]
    fn is_directive_or_blank_or_comment_skips_directive_lines() {
        // UT-23: The BetweenDocs predicate must skip `%`-prefixed lines.
        // Full directive grammar (Task 18) will distinguish valid directives
        // from bare-doc content; until then, all `%`-lines are skipped here.
        let Some(line) = LineBuffer::new("%YAML 1.2").consume_next() else {
            unreachable!("LineBuffer produced no line for non-empty input")
        };
        assert!(is_directive_or_blank_or_comment(&line));
    }
}
