// SPDX-License-Identifier: MIT

//! Line-level lexer: wraps [`LineBuffer`] and provides marker-detection and
//! line-consumption primitives consumed by the [`EventIter`] state machine.
//!
//! The lexer is lazy — it never buffers the whole input.  It advances through
//! the [`LineBuffer`] one line at a time, driven by the state machine.

use std::borrow::Cow;

use crate::error::Error;
use crate::lines::{Line, LineBuffer};
use crate::pos::{Pos, Span};

mod block;
mod comment;
mod plain;
mod quoted;

pub use plain::{is_ns_char, scan_plain_line_flow};

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
                let mut comment_end = inline_start.advance('#');
                for ch in comment_text.chars() {
                    comment_end = comment_end.advance(ch);
                }
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

/// True when `line` is blank, comment-only, or a directive (`%`-prefixed).
///
/// Directive lines (`%YAML`, `%TAG`, and unknown `%` directives) are
/// stream-level metadata that precede `---`.  This predicate is only correct
/// to use in the between-documents context; inside a document body `%`-prefixed
/// lines are content and must be handled by [`is_blank_or_comment`] instead.
///
/// Used only in tests to verify the `BetweenDocs` predicate.
#[cfg(test)]
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
    use crate::event::Chomp;
    use std::borrow::Cow;

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

    // -----------------------------------------------------------------------
    // Group G — try_consume_plain_scalar unit tests (Task 6)
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_single_word() {
        let mut lex = make_lexer("hello");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "hello");
    }

    #[test]
    fn plain_scalar_multi_word() {
        let mut lex = make_lexer("hello world");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "hello world");
    }

    #[test]
    fn plain_scalar_cow_borrowed_for_single_line() {
        let mut lex = make_lexer("hello");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
        assert!(matches!(val, Cow::Owned(_)), "multi-line must be Owned");
        assert_eq!(val, "foo bar");
    }

    #[test]
    fn plain_scalar_with_url() {
        // `:` not followed by space → allowed inside plain scalar.
        let mut lex = make_lexer("http://x.com");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "http://x.com");
    }

    #[test]
    fn plain_scalar_with_hash_no_preceding_space() {
        // `#` not preceded by whitespace → allowed inside plain scalar.
        let mut lex = make_lexer("a#b");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "a#b");
    }

    #[test]
    fn plain_scalar_terminated_by_colon_space() {
        // `: ` (colon + space) terminates the scalar — the colon is not safe.
        let mut lex = make_lexer("key: value");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "key");
    }

    #[test]
    fn plain_scalar_terminated_by_hash_with_space() {
        // ` #` (space + hash) terminates the scalar — `#` preceded by whitespace.
        let mut lex = make_lexer("foo # comment");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_trailing_whitespace_stripped() {
        // Trailing spaces on a line are not part of the scalar value.
        let mut lex = make_lexer("foo   ");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_multiline_folds_single_break_to_space() {
        let mut lex = make_lexer("foo\n  bar\n  baz");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "foo bar baz");
    }

    #[test]
    fn plain_scalar_multiline_blank_line_folds_to_newline() {
        // A blank line in the middle of a multi-line scalar becomes a newline.
        let mut lex = make_lexer("foo\n\nbar");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "-a");
    }

    #[test]
    fn plain_scalar_colon_followed_by_safe_char_is_valid() {
        // `:a` starts a plain scalar.
        let mut lex = make_lexer(":a");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, ":a");
    }

    #[test]
    fn plain_scalar_forbidden_continuation_stops_at_marker() {
        // A `---` marker at column 0 terminates multi-line continuation.
        let mut lex = make_lexer("foo\n---\nbar");
        // Only "foo" should be collected (the --- terminates the scalar).
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_span_start_byte_offset() {
        let mut lex = make_lexer("hello");
        let (_, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(span.start.byte_offset, 0);
    }

    #[test]
    fn plain_scalar_span_end_byte_offset() {
        // "hello" = 5 bytes; span.end should be at byte offset 5.
        let mut lex = make_lexer("hello");
        let (_, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(span.end.byte_offset, 5);
    }

    #[test]
    fn plain_scalar_indented_start_span_byte_offset() {
        // "  hello" — leading 2 spaces, scalar starts at byte 2.
        let mut lex = make_lexer("  hello");
        let (val, span) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "hello");
        assert_eq!(span.start.byte_offset, 2);
    }

    #[test]
    fn plain_scalar_with_multibyte_utf8() {
        // '中' (3 bytes) should be consumed as a valid plain scalar.
        let mut lex = make_lexer("中文");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "中文");
    }

    #[test]
    fn plain_scalar_dedented_continuation_stops() {
        // A line at indent < parent_indent stops continuation.
        // For parent_indent=2: "  foo\nbar" — bar at indent 0 < 2, terminates.
        let mut lex = make_lexer("  foo\nbar");
        let (val, _) = lex
            .try_consume_plain_scalar(2)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_with_backslashes() {
        // Backslashes are not special in plain scalars.
        let mut lex = make_lexer("plain\\value\\with\\backslashes");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "key");
    }

    #[test]
    fn plain_scalar_colon_eof_terminates() {
        // `:`+EOF: next char is None → ns_plain_char_block returns false → `:` not included.
        let mut lex = make_lexer("key:");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "foo\n\nbar");
    }

    #[test]
    fn plain_scalar_multiline_continuation_trailing_space_stripped() {
        // Trailing space on a continuation line is stripped before folding.
        let mut lex = make_lexer("foo\nbar   \nbaz");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, "foo");
    }

    #[test]
    fn plain_scalar_dash_dash_dash_word_attached_is_not_forbidden() {
        // `---word` at column 0 is NOT a c-forbidden marker — it's a valid continuation.
        let mut lex = make_lexer("foo\n---word");
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
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
            .unwrap_or_else(|| unreachable!("should parse"));
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
        lex.consume_marker_line(false);
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse inline scalar"));
        assert_eq!(val, "text");
    }

    #[test]
    fn plain_scalar_inline_after_marker_is_cow_borrowed() {
        // Inline content from `---` line is a zero-copy borrowed slice.
        let mut lex = make_lexer("--- text");
        lex.consume_marker_line(false);
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse inline scalar"));
        assert!(
            matches!(val, Cow::Borrowed(_)),
            "inline scalar from marker line must be Cow::Borrowed"
        );
    }

    // =======================================================================
    // Group H — try_consume_single_quoted (Task 7)
    // =======================================================================

    fn sq(input: &str) -> (Cow<'_, str>, Span) {
        Lexer::new(input)
            .try_consume_single_quoted(0)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"))
            .unwrap_or_else(|| unreachable!("expected Some, got None"))
    }

    fn sq_err(input: &str) -> Error {
        match Lexer::new(input).try_consume_single_quoted(0) {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    fn sq_none(input: &str) {
        let result = Lexer::new(input)
            .try_consume_single_quoted(0)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"));
        assert!(result.is_none(), "expected None for input {input:?}");
    }

    // -----------------------------------------------------------------------
    // Group H-A — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn single_quoted_simple_word_returns_value() {
        let (val, _) = sq("'hello'");
        assert_eq!(val, "hello");
    }

    #[test]
    fn single_quoted_empty_string_returns_empty() {
        let (val, _) = sq("''");
        assert_eq!(val, "");
    }

    #[test]
    fn single_quoted_escaped_quote_in_middle() {
        let (val, _) = sq("'it''s'");
        assert_eq!(val, "it's");
    }

    #[test]
    fn single_quoted_escaped_quote_at_start() {
        let (val, _) = sq("'''leading'");
        assert_eq!(val, "'leading");
    }

    #[test]
    fn single_quoted_escaped_quote_at_end() {
        let (val, _) = sq("'trailing'''");
        assert_eq!(val, "trailing'");
    }

    #[test]
    fn single_quoted_multiple_escaped_quotes() {
        let (val, _) = sq("'a''b''c'");
        assert_eq!(val, "a'b'c");
    }

    #[test]
    fn single_quoted_multi_word() {
        let (val, _) = sq("'hello world'");
        assert_eq!(val, "hello world");
    }

    #[test]
    fn single_quoted_multibyte_utf8() {
        let (val, _) = sq("'日本語'");
        assert_eq!(val, "日本語");
    }

    #[test]
    fn single_quoted_special_chars_not_escaped() {
        // Backslash is not special in single-quoted scalars.
        let (val, _) = sq(r"'foo\nbar'");
        assert_eq!(val, r"foo\nbar");
    }

    #[test]
    fn single_quoted_double_quote_inside() {
        let (val, _) = sq(r#"'say "hello"'"#);
        assert_eq!(val, r#"say "hello""#);
    }

    // -----------------------------------------------------------------------
    // Group H-B — Cow allocation
    // -----------------------------------------------------------------------

    #[test]
    fn single_quoted_single_line_no_escape_is_borrowed() {
        let (val, _) = sq("'hello'");
        assert!(matches!(val, Cow::Borrowed(_)), "must be Borrowed");
    }

    #[test]
    fn single_quoted_with_escaped_quote_is_owned() {
        let (val, _) = sq("'it''s'");
        assert!(matches!(val, Cow::Owned(_)), "must be Owned");
    }

    #[test]
    fn single_quoted_multiline_is_owned() {
        let (val, _) = sq("'foo\n  bar'");
        assert!(matches!(val, Cow::Owned(_)), "must be Owned");
    }

    // -----------------------------------------------------------------------
    // Group H-C — multi-line folding
    // -----------------------------------------------------------------------

    #[test]
    fn single_quoted_multiline_single_break_folds_to_space() {
        let (val, _) = sq("'foo\nbar'");
        assert_eq!(val, "foo bar");
    }

    #[test]
    fn single_quoted_multiline_leading_whitespace_stripped_on_continuation() {
        let (val, _) = sq("'foo\n  bar'");
        assert_eq!(val, "foo bar");
    }

    #[test]
    fn single_quoted_multiline_blank_line_produces_newline() {
        let (val, _) = sq("'foo\n\nbar'");
        assert_eq!(val, "foo\nbar");
    }

    #[test]
    fn single_quoted_multiline_two_blank_lines_produce_two_newlines() {
        let (val, _) = sq("'foo\n\n\nbar'");
        assert_eq!(val, "foo\n\nbar");
    }

    // -----------------------------------------------------------------------
    // Group H-D — error cases
    // -----------------------------------------------------------------------

    #[test]
    fn single_quoted_unterminated_returns_err() {
        let _ = sq_err("'hello");
    }

    #[test]
    fn single_quoted_no_opening_quote_returns_none() {
        sq_none("hello");
    }

    #[test]
    fn single_quoted_blank_line_returns_none() {
        sq_none("   ");
    }

    // =======================================================================
    // Group I — try_consume_double_quoted (Task 7)
    // =======================================================================

    fn dq(input: &str) -> (Cow<'_, str>, Span) {
        Lexer::new(input)
            .try_consume_double_quoted(None)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"))
            .unwrap_or_else(|| unreachable!("expected Some, got None"))
    }

    fn dq_err(input: &str) -> Error {
        match Lexer::new(input).try_consume_double_quoted(None) {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    fn dq_none(input: &str) {
        let result = Lexer::new(input)
            .try_consume_double_quoted(None)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"));
        assert!(result.is_none(), "expected None for input {input:?}");
    }

    // -----------------------------------------------------------------------
    // Group I-E — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_simple_word_returns_value() {
        let (val, _) = dq("\"hello\"");
        assert_eq!(val, "hello");
    }

    #[test]
    fn double_quoted_empty_string_returns_empty() {
        let (val, _) = dq("\"\"");
        assert_eq!(val, "");
    }

    #[test]
    fn double_quoted_escape_newline() {
        let (val, _) = dq("\"foo\\nbar\"");
        assert_eq!(val, "foo\nbar");
    }

    #[test]
    fn double_quoted_escape_tab() {
        let (val, _) = dq("\"foo\\tbar\"");
        assert_eq!(val, "foo\tbar");
    }

    #[test]
    fn double_quoted_escape_backslash() {
        let (val, _) = dq("\"foo\\\\bar\"");
        assert_eq!(val, "foo\\bar");
    }

    #[test]
    fn double_quoted_escape_double_quote() {
        let (val, _) = dq("\"say \\\"hi\\\"\"");
        assert_eq!(val, "say \"hi\"");
    }

    #[test]
    fn double_quoted_escape_null() {
        let (val, _) = dq("\"\\0\"");
        assert_eq!(val.as_bytes(), b"\x00");
    }

    #[test]
    fn double_quoted_escape_slash() {
        let (val, _) = dq("\"foo\\/bar\"");
        assert_eq!(val, "foo/bar");
    }

    #[test]
    fn double_quoted_escape_space() {
        let (val, _) = dq("\"foo\\ bar\"");
        assert_eq!(val, "foo bar");
    }

    #[test]
    fn double_quoted_all_single_char_escapes() {
        let cases: &[(&str, &str)] = &[
            ("\"\\a\"", "\x07"),
            ("\"\\b\"", "\x08"),
            ("\"\\v\"", "\x0B"),
            ("\"\\f\"", "\x0C"),
            ("\"\\r\"", "\r"),
            ("\"\\e\"", "\x1B"),
            ("\"\\N\"", "\u{85}"),
            ("\"\\_\"", "\u{A0}"),
            ("\"\\L\"", "\u{2028}"),
            ("\"\\P\"", "\u{2029}"),
        ];
        for (input, expected) in cases {
            let (val, _) = dq(input);
            assert_eq!(val.as_ref(), *expected, "input: {input:?}");
        }
    }

    #[test]
    fn double_quoted_multibyte_utf8_literal() {
        let (val, _) = dq("\"日本語\"");
        assert_eq!(val, "日本語");
        assert!(matches!(val, Cow::Borrowed(_)), "no escapes → Borrowed");
    }

    // -----------------------------------------------------------------------
    // Group I-F — hex/unicode escapes
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_hex_escape_2digit_correct() {
        let (val, _) = dq("\"\\x41\"");
        assert_eq!(val, "A");
    }

    #[test]
    fn double_quoted_hex_escape_2digit_lowercase() {
        let (val, _) = dq("\"\\x61\"");
        assert_eq!(val, "a");
    }

    #[test]
    fn double_quoted_unicode_4digit_correct() {
        let (val, _) = dq("\"\\u0041\"");
        assert_eq!(val, "A");
    }

    #[test]
    fn double_quoted_unicode_4digit_non_ascii() {
        let (val, _) = dq("\"\\u00E9\"");
        assert_eq!(val, "é");
    }

    #[test]
    fn double_quoted_unicode_8digit_basic() {
        let (val, _) = dq("\"\\U00000041\"");
        assert_eq!(val, "A");
    }

    #[test]
    fn double_quoted_unicode_8digit_supplementary() {
        let (val, _) = dq("\"\\U0001F600\"");
        assert_eq!(val, "😀");
    }

    #[test]
    fn double_quoted_hex_invalid_digits_returns_err() {
        let _ = dq_err("\"\\xGG\"");
    }

    #[test]
    fn double_quoted_hex_truncated_returns_err() {
        // Only one hex digit before closing quote.
        let _ = dq_err("\"\\xA\"");
    }

    #[test]
    fn double_quoted_unicode_4digit_truncated_returns_err() {
        let _ = dq_err("\"\\u004\"");
    }

    #[test]
    fn double_quoted_unicode_surrogate_returns_err() {
        let _ = dq_err("\"\\uD800\"");
    }

    #[test]
    fn double_quoted_unicode_surrogate_range_high_returns_err() {
        let _ = dq_err("\"\\uDFFF\"");
    }

    #[test]
    fn double_quoted_unicode_8digit_out_of_range_returns_err() {
        let _ = dq_err("\"\\U00110000\"");
    }

    #[test]
    fn double_quoted_unknown_escape_code_returns_err() {
        let _ = dq_err("\"\\q\"");
    }

    // -----------------------------------------------------------------------
    // Group I-G — line continuation and folding
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_backslash_newline_suppresses_break() {
        // `\` as last char of line → line continuation, no separator.
        let (val, _) = dq("\"foo\\\nbar\"");
        assert_eq!(val, "foobar");
    }

    #[test]
    fn double_quoted_backslash_newline_strips_leading_whitespace_on_next_line() {
        let (val, _) = dq("\"foo\\\n   bar\"");
        assert_eq!(val, "foobar");
    }

    #[test]
    fn double_quoted_real_newline_folds_to_space() {
        let (val, _) = dq("\"foo\nbar\"");
        assert_eq!(val, "foo bar");
    }

    #[test]
    fn double_quoted_real_newline_with_leading_whitespace_on_continuation() {
        let (val, _) = dq("\"foo\n  bar\"");
        assert_eq!(val, "foo bar");
    }

    #[test]
    fn double_quoted_blank_line_in_multiline_produces_newline() {
        let (val, _) = dq("\"foo\n\nbar\"");
        assert_eq!(val, "foo\nbar");
    }

    #[test]
    fn double_quoted_two_blank_lines_produce_two_newlines() {
        let (val, _) = dq("\"foo\n\n\nbar\"");
        assert_eq!(val, "foo\n\nbar");
    }

    // -----------------------------------------------------------------------
    // Group I-H — Cow allocation
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_single_line_no_escape_is_borrowed() {
        let (val, _) = dq("\"hello\"");
        assert!(matches!(val, Cow::Borrowed(_)), "must be Borrowed");
    }

    #[test]
    fn double_quoted_with_escape_is_owned() {
        let (val, _) = dq("\"\\n\"");
        assert!(matches!(val, Cow::Owned(_)), "must be Owned");
    }

    #[test]
    fn double_quoted_multiline_is_owned() {
        let (val, _) = dq("\"foo\nbar\"");
        assert!(matches!(val, Cow::Owned(_)), "must be Owned");
    }

    // -----------------------------------------------------------------------
    // Group I-I — error cases
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_unterminated_returns_err() {
        let _ = dq_err("\"hello");
    }

    #[test]
    fn double_quoted_no_opening_quote_returns_none() {
        dq_none("hello");
    }

    // -----------------------------------------------------------------------
    // Group I-I — security controls (I-22 through I-25)
    // -----------------------------------------------------------------------

    // I-22: \u hex escape producing a bidi control character is rejected.
    #[test]
    fn double_quoted_bidi_escape_rejected() {
        let e = dq_err("\"\\u202E\""); // RIGHT-TO-LEFT OVERRIDE
        assert!(
            e.message.contains("bidirectional"),
            "expected bidi error, got: {}",
            e.message
        );
    }

    // I-23: \x hex escape producing a non-printable character is rejected.
    // \x01 is a control character (SOH) — not c-printable.
    #[test]
    fn double_quoted_non_printable_hex_escape_rejected() {
        let e = dq_err("\"\\x01\"");
        assert!(
            e.message.contains("non-printable"),
            "expected non-printable error, got: {}",
            e.message
        );
    }

    // I-23b: Named escape \0 (null byte) is NOT subject to the printability
    // check — only hex escapes (\x, \u, \U) are gated.
    #[test]
    fn double_quoted_named_null_escape_is_ok() {
        let (val, _) = dq("\"\\0\"");
        assert_eq!(val.as_ref(), "\x00");
    }

    // I-24: A scalar accumulation that exceeds 1 MiB raises an error.
    #[test]
    fn double_quoted_length_cap_exceeded_raises_error() {
        // Build a double-quoted scalar whose decoded length exceeds 1 MiB.
        // One \n escape forces Owned allocation, then 1_048_577 plain 'a'
        // bytes are appended through the _ arm, triggering the length cap.
        // Using plain chars instead of more escapes keeps source size small
        // (~1 MiB) and avoids a 5 MiB source string from escape repetition.
        let mut big = String::with_capacity(1_048_582);
        big.push('"');
        big.push('\\');
        big.push('n'); // \n → force Owned
        big.extend(std::iter::repeat_n('a', 1_048_577));
        big.push('"');
        let e = dq_err(&big);
        assert!(
            e.message.contains("maximum allowed length"),
            "expected length cap error, got: {}",
            e.message
        );
    }

    // I-25: A truncated hex escape (fewer hex digits than required) returns
    // an error rather than panicking.
    #[test]
    fn double_quoted_truncated_hex_escape_returns_error() {
        // \uXX is only 2 hex digits but \u requires 4 — decode_escape returns
        // None, which becomes an invalid-escape error.
        let e = dq_err("\"\\u00\"");
        assert!(
            e.message.contains("invalid escape"),
            "expected invalid escape error, got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group H — try_consume_literal_block_scalar unit tests (Task 8)
    // -----------------------------------------------------------------------
    //
    // Helpers for literal block scalar tests.

    /// Parse a literal block scalar from `input`, returning Ok((value, chomp)).
    /// Panics if the result is None or Err.
    fn lit_ok(input: &str) -> (String, Chomp) {
        let mut lex = make_lexer(input);
        let result = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some, got None"));
        let (cow, chomp, _span) =
            result.unwrap_or_else(|e| unreachable!("expected Ok, got Err: {e}"));
        (cow.into_owned(), chomp)
    }

    /// Parse a literal block scalar from `input`, expecting an error.
    fn lit_err(input: &str) -> Error {
        let mut lex = make_lexer(input);
        let result = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some, got None"));
        match result {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    /// Try a literal block scalar; returns None if not a block scalar.
    fn lit_none(input: &str) -> bool {
        let mut lex = make_lexer(input);
        lex.try_consume_literal_block_scalar(0).is_none()
    }

    // -----------------------------------------------------------------------
    // Group H-A: Header parsing — happy path
    // -----------------------------------------------------------------------

    // UT-LB-A1: `|` (no indicators) → Clip, auto-detect indent
    #[test]
    fn literal_header_no_indicators_yields_clip() {
        let (_, chomp) = lit_ok("|\n  hello\n");
        assert_eq!(chomp, Chomp::Clip);
    }

    // UT-LB-A2: `|-` → Strip
    #[test]
    fn literal_header_minus_yields_strip() {
        let (_, chomp) = lit_ok("|-\n  hello\n");
        assert_eq!(chomp, Chomp::Strip);
    }

    // UT-LB-A3: `|+` → Keep
    #[test]
    fn literal_header_plus_yields_keep() {
        let (_, chomp) = lit_ok("|+\n  hello\n");
        assert_eq!(chomp, Chomp::Keep);
    }

    // UT-LB-A4: `|2` → explicit indent 2 (relative to parent=0)
    #[test]
    fn literal_header_explicit_indent_2() {
        let (val, _) = lit_ok("|2\n  hello\n");
        assert_eq!(val, "hello\n");
    }

    // UT-LB-A5: `|-2` → Strip + indent 2
    #[test]
    fn literal_header_minus_indent_2() {
        let (val, chomp) = lit_ok("|-2\n  hello\n");
        assert_eq!(chomp, Chomp::Strip);
        assert_eq!(val, "hello");
    }

    // UT-LB-A6: `|2-` → same as |-2 (either order)
    #[test]
    fn literal_header_indent_2_then_minus() {
        let (val, chomp) = lit_ok("|2-\n  hello\n");
        assert_eq!(chomp, Chomp::Strip);
        assert_eq!(val, "hello");
    }

    // UT-LB-A7: `|+2` → Keep + indent 2
    #[test]
    fn literal_header_plus_indent_2() {
        let (val, chomp) = lit_ok("|+2\n  hello\n\n");
        assert_eq!(chomp, Chomp::Keep);
        assert_eq!(val, "hello\n\n");
    }

    // UT-LB-A8: `|2+` → same (either order)
    #[test]
    fn literal_header_indent_2_then_plus() {
        let (val, chomp) = lit_ok("|2+\n  hello\n\n");
        assert_eq!(chomp, Chomp::Keep);
        assert_eq!(val, "hello\n\n");
    }

    // UT-LB-A9: `| # comment` → Clip (comment ignored)
    #[test]
    fn literal_header_with_comment_yields_clip() {
        let (val, chomp) = lit_ok("| # this is a comment\n  hello\n");
        assert_eq!(chomp, Chomp::Clip);
        assert_eq!(val, "hello\n");
    }

    // UT-LB-A10: returns None for non-`|` input
    #[test]
    fn literal_block_returns_none_for_non_pipe() {
        assert!(lit_none("hello\n"));
    }

    // UT-LB-A11: returns None for empty input
    #[test]
    fn literal_block_returns_none_for_empty_input() {
        assert!(lit_none(""));
    }

    // UT-LB-A12: `|` at leading whitespace — leading spaces before `|` are allowed
    #[test]
    fn literal_block_with_leading_spaces_before_pipe() {
        let (val, chomp) = lit_ok("  |\n    hello\n");
        assert_eq!(chomp, Chomp::Clip);
        assert_eq!(val, "hello\n");
    }

    // UT-LB-A13: `|  # comment` (spaces then comment) → Clip
    #[test]
    fn header_space_then_comment_gives_clip() {
        let (val, chomp) = lit_ok("|  # comment\n  hello\n");
        assert_eq!(chomp, Chomp::Clip);
        assert_eq!(val, "hello\n");
    }

    // UT-LB-A14: `|9` → explicit indent 9
    #[test]
    fn header_explicit_indent_nine() {
        let (val, chomp) = lit_ok("|9\n         foo\n");
        assert_eq!(chomp, Chomp::Clip);
        assert_eq!(val, "foo\n");
    }

    // -----------------------------------------------------------------------
    // Group H-B: Header parsing — errors
    // -----------------------------------------------------------------------

    // UT-LB-B1: `|!` → error (invalid indicator)
    #[test]
    fn literal_header_invalid_indicator_exclamation_is_error() {
        let e = lit_err("|!\n  hello\n");
        assert!(
            e.message.contains("invalid") || e.message.contains("indicator"),
            "unexpected error: {}",
            e.message
        );
    }

    // UT-LB-B2: `|0` → error (zero is forbidden as indent digit)
    #[test]
    fn literal_header_zero_indent_is_error() {
        let e = lit_err("|0\n  hello\n");
        assert!(
            e.message.contains("indent") || e.message.contains('0'),
            "unexpected error: {}",
            e.message
        );
    }

    // UT-LB-B3: `|99` → error (duplicate indent digit)
    #[test]
    fn literal_header_duplicate_indent_digit_is_error() {
        let e = lit_err("|99\n  hello\n");
        assert!(
            e.message.contains("duplicate") || e.message.contains("indent"),
            "unexpected error: {}",
            e.message
        );
    }

    // UT-LB-B4: `|++` → error (duplicate chomp indicator)
    #[test]
    fn literal_header_duplicate_chomp_indicator_is_error() {
        let e = lit_err("|++\n  hello\n");
        assert!(
            e.message.contains("duplicate") || e.message.contains("chomp"),
            "unexpected error: {}",
            e.message
        );
    }

    // UT-LB-B5: `|--` → error (duplicate chomp indicator)
    #[test]
    fn literal_header_duplicate_strip_indicator_is_error() {
        let e = lit_err("|--\n  hello\n");
        assert!(
            e.message.contains("duplicate") || e.message.contains("chomp"),
            "unexpected error: {}",
            e.message
        );
    }

    // UT-LB-B6: `|+-` → error (two different chomp indicators)
    #[test]
    fn header_two_chomp_indicators_mixed_is_error() {
        let e = lit_err("|+-\n  hello\n");
        assert!(
            e.message.contains("duplicate") || e.message.contains("chomp"),
            "unexpected error: {}",
            e.message
        );
    }

    // UT-LB-B7: `|2!` → error (invalid char after digit)
    #[test]
    fn header_invalid_char_after_digit_is_error() {
        let e = lit_err("|2!\n  hello\n");
        assert!(
            e.message.contains("invalid") || e.message.contains("indicator"),
            "unexpected error: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group H-C: Clip content collection
    // -----------------------------------------------------------------------

    // UT-LB-C1: single-line content
    #[test]
    fn literal_single_line_content() {
        let (val, _) = lit_ok("|\n  hello\n");
        assert_eq!(val, "hello\n");
    }

    // UT-LB-C2: multi-line content
    #[test]
    fn literal_multi_line_content() {
        let (val, _) = lit_ok("|\n  foo\n  bar\n");
        assert_eq!(val, "foo\nbar\n");
    }

    // UT-LB-C3: blank line between content lines
    #[test]
    fn literal_blank_line_in_content() {
        let (val, _) = lit_ok("|\n  foo\n\n  bar\n");
        assert_eq!(val, "foo\n\nbar\n");
    }

    // UT-LB-C4: leading blank before first content (blank becomes \n per spec)
    #[test]
    fn leading_blank_before_first_content_is_included_clip() {
        // Per YAML 1.2 §8.1.2, blank lines before the first content line
        // are included as newlines via l-empty.  A completely empty line
        // has s-indent(0) which satisfies l-empty(n,BLOCK-IN) for any n>0.
        let (val, _) = lit_ok("|\n\n  foo\n");
        assert_eq!(val, "\nfoo\n");
    }

    // UT-LB-C5: empty scalar (header only, no content)
    #[test]
    fn literal_empty_scalar_clip_yields_empty_string() {
        let (val, chomp) = lit_ok("|\n");
        assert_eq!(chomp, Chomp::Clip);
        assert_eq!(val, "");
    }

    // UT-LB-C4b: two interior blank lines preserved
    #[test]
    fn two_interior_blank_lines_preserved() {
        let (val, _) = lit_ok("|\n  foo\n\n\n  bar\n");
        assert_eq!(val, "foo\n\n\nbar\n");
    }

    // UT-LB-C5b: empty scalar with trailing blank still yields empty string
    #[test]
    fn empty_scalar_with_trailing_blank_still_empty() {
        let (val, _) = lit_ok("|\n\n");
        assert_eq!(val, "");
    }

    // UT-LB-C6: trailing blank line with Clip → single newline kept
    #[test]
    fn literal_trailing_blank_with_clip_keeps_single_newline() {
        let (val, _) = lit_ok("|\n  foo\n\n");
        assert_eq!(val, "foo\n");
    }

    // UT-LB-C6b: two trailing blanks with Clip → still single newline
    #[test]
    fn two_trailing_blanks_dropped_clip() {
        let (val, _) = lit_ok("|\n  foo\n\n\n");
        assert_eq!(val, "foo\n");
    }

    // UT-LB-C7: content at higher indent → extra spaces in value
    #[test]
    fn literal_content_with_extra_indent_preserves_spaces() {
        // "|\n   foo\n" with content_indent=3: value is "foo\n"
        // "|\n  foo\n   bar\n" with content_indent=2: bar has 1 extra space
        let (val, _) = lit_ok("|\n  foo\n   bar\n");
        assert_eq!(val, "foo\n bar\n");
    }

    // UT-LB-C8: content terminated by dedent
    #[test]
    fn literal_content_terminated_by_dedent() {
        let mut lex = make_lexer("|\n  foo\nkey: val\n");
        let result = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(result.0.as_ref(), "foo\n");
        // `key: val` should still be in the buffer.
        let remaining = lex.buf.peek_next().map(|l| l.content);
        assert_eq!(remaining, Some("key: val"));
    }

    // UT-LB-C9: EOF without trailing newline (no physical newline on last line)
    #[test]
    fn literal_eof_without_trailing_newline() {
        // "|\n  foo" — no final newline; no b-as-line-feed, so value is "foo".
        let (val, _) = lit_ok("|\n  foo");
        assert_eq!(val, "foo");
    }

    // -----------------------------------------------------------------------
    // Group H-D: Strip and Keep chomping
    // -----------------------------------------------------------------------

    // UT-LB-D1: Strip — no trailing newline
    #[test]
    fn literal_strip_no_trailing_newline() {
        let (val, chomp) = lit_ok("|-\n  foo\n");
        assert_eq!(chomp, Chomp::Strip);
        assert_eq!(val, "foo");
    }

    // UT-LB-D2: Strip — trailing blank lines removed
    #[test]
    fn literal_strip_with_trailing_blanks_removes_all() {
        let (val, _) = lit_ok("|-\n  foo\n\n\n");
        assert_eq!(val, "foo");
    }

    // UT-LB-D3: Strip — empty scalar
    #[test]
    fn literal_strip_empty_scalar_yields_empty_string() {
        let (val, chomp) = lit_ok("|-\n");
        assert_eq!(chomp, Chomp::Strip);
        assert_eq!(val, "");
    }

    // UT-LB-D4: Keep — all trailing newlines kept
    #[test]
    fn literal_keep_all_trailing_newlines() {
        let (val, chomp) = lit_ok("|+\n  foo\n\n\n");
        assert_eq!(chomp, Chomp::Keep);
        assert_eq!(val, "foo\n\n\n");
    }

    // UT-LB-D5: Keep — single trailing newline
    #[test]
    fn literal_keep_single_trailing_newline() {
        let (val, _) = lit_ok("|+\n  foo\n");
        assert_eq!(val, "foo\n");
    }

    // UT-LB-D6: Keep — empty scalar
    #[test]
    fn literal_keep_empty_scalar_yields_empty_string() {
        let (val, chomp) = lit_ok("|+\n");
        assert_eq!(chomp, Chomp::Keep);
        assert_eq!(val, "");
    }

    // UT-LB-D7: Clip — single content line, no trailing blank
    #[test]
    fn literal_clip_no_trailing_blank_yields_one_newline() {
        let (val, _) = lit_ok("|\n  foo\n");
        assert_eq!(val, "foo\n");
    }

    // UT-LB-D8: Clip — multiple trailing blanks → only one newline kept
    #[test]
    fn literal_clip_multiple_trailing_blanks_clips_to_one() {
        let (val, _) = lit_ok("|\n  foo\n\n\n\n");
        assert_eq!(val, "foo\n");
    }

    // UT-LB-D9: Strip with multi-line content
    #[test]
    fn literal_strip_multiline_removes_last_newline() {
        let (val, _) = lit_ok("|-\n  foo\n  bar\n");
        assert_eq!(val, "foo\nbar");
    }

    // UT-LB-D10: Keep with multi-line content and multiple trailing blanks
    #[test]
    fn literal_keep_multiline_preserves_all_trailing() {
        let (val, _) = lit_ok("|+\n  foo\n  bar\n\n");
        assert_eq!(val, "foo\nbar\n\n");
    }

    // UT-LB-D11: Keep — only blank lines (no content) → newlines from blanks
    #[test]
    fn keep_only_blanks_produces_newlines() {
        let (val, _) = lit_ok("|+\n\n\n");
        assert_eq!(val, "\n\n");
    }

    // UT-LB-D12: Strip — only blank lines → empty string
    #[test]
    fn strip_only_blanks_produces_empty_string() {
        let (val, _) = lit_ok("|-\n\n\n");
        assert_eq!(val, "");
    }

    // UT-LB-D13: Clip — only blank lines → empty string
    #[test]
    fn clip_only_blanks_produces_empty_string() {
        let (val, _) = lit_ok("|\n\n\n");
        assert_eq!(val, "");
    }

    // -----------------------------------------------------------------------
    // Group H-E: Explicit indent indicator
    // -----------------------------------------------------------------------

    // UT-LB-E1: explicit indent 2 with parent=0
    #[test]
    fn literal_explicit_indent_2_parent_0() {
        let (val, _) = lit_ok("|2\n  foo\n");
        assert_eq!(val, "foo\n");
    }

    // UT-LB-E2: explicit indent with more indented line → extra spaces preserved
    #[test]
    fn literal_explicit_indent_2_extra_spaces_preserved() {
        let (val, _) = lit_ok("|2\n   foo\n");
        assert_eq!(val, " foo\n");
    }

    // UT-LB-E3: explicit indent 2 with parent=0, content less indented → no content
    // (foo has 0 spaces < 2: content_indent=0+2=2, but foo only has 0 spaces)
    // Actually "foo" without leading spaces is indent=0 < 2 — scalar is empty.
    #[test]
    fn literal_explicit_indent_content_insufficient_indent_yields_empty() {
        let (val, _) = lit_ok("|4\n  foo\n");
        // content_indent=4, foo has indent=2 < 4 → empty scalar
        assert_eq!(val, "");
    }

    // UT-LB-E4: explicit indent 1 with parent=0
    #[test]
    fn literal_explicit_indent_1() {
        let (val, _) = lit_ok("|1\n foo\n");
        assert_eq!(val, "foo\n");
    }

    // UT-LB-E5: explicit indent 2 relative to parent_indent=2 → content_indent=4
    #[test]
    fn explicit_indent_relative_to_parent() {
        let mut lex = make_lexer("|2\n    foo\n");
        let result = lex
            .try_consume_literal_block_scalar(2)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        // parent_indent=2 + explicit=2 = content_indent=4; "    foo" has indent=4
        assert_eq!(result.0.as_ref(), "foo\n");
    }

    // -----------------------------------------------------------------------
    // Group H-F: Termination/boundary conditions
    // -----------------------------------------------------------------------

    // UT-LB-F1: block scalar followed by non-blank content at col 0
    #[test]
    fn literal_stops_at_dedented_non_blank() {
        let mut lex = make_lexer("|\n  foo\nnext line\n");
        let (val, _, _) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(val.as_ref(), "foo\n");
        let remaining = lex.buf.peek_next().map(|l| l.content);
        assert_eq!(remaining, Some("next line"));
    }

    // UT-LB-F2: block scalar at EOF (no lines at all after header)
    #[test]
    fn literal_at_eof_after_header_yields_empty() {
        let (val, _) = lit_ok("|\n");
        assert_eq!(val, "");
    }

    // UT-LB-F3: span start is at position of `|`
    #[test]
    fn literal_span_start_at_pipe() {
        let mut lex = make_lexer("|\n  hello\n");
        let (_, _, span) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(span.start.byte_offset, 0);
        assert_eq!(span.start.column, 0);
    }

    // UT-LB-F4: span end after all consumed lines
    #[test]
    fn literal_span_end_after_content_lines() {
        // "|\n  hello\n" = 10 bytes
        let mut lex = make_lexer("|\n  hello\n");
        let (_, _, span) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(span.end.byte_offset, 10);
    }

    // UT-LB-F5: span end covers trailing blanks that are consumed
    #[test]
    fn literal_span_end_covers_trailing_blanks() {
        // "|\n  foo\n\n" = 9 bytes (|=1, \n=1, space=1, space=1, foo=3, \n=1, \n=1)
        // trailing blank is consumed even under Clip
        let mut lex = make_lexer("|\n  foo\n\n");
        let (_, _, span) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(span.end.byte_offset, 9);
    }

    // -----------------------------------------------------------------------
    // Group H-G: Tab handling
    // -----------------------------------------------------------------------

    // UT-LB-G1: tab as first char of content line → error
    #[test]
    fn literal_tab_as_indentation_is_error() {
        let e = lit_err("|\n\tfoo\n");
        assert!(
            e.message.contains("tab"),
            "expected tab error, got: {}",
            e.message
        );
    }

    // UT-LB-G2: tab after content-indent spaces → preserved in value
    #[test]
    fn literal_tab_after_spaces_is_preserved() {
        // "|\n  \tfoo\n": content_indent=2 (from `  \t`), after stripping 2 spaces: "\tfoo"
        // The tab is after the indent spaces — it's content.
        let (val, _) = lit_ok("|\n  \tfoo\n");
        assert_eq!(val, "\tfoo\n");
    }

    // -----------------------------------------------------------------------
    // Group H-H: UTF-8 and special content
    // -----------------------------------------------------------------------

    // UT-LB-H1: multi-byte UTF-8 content
    #[test]
    fn literal_multibyte_utf8_content() {
        let (val, _) = lit_ok("|\n  héllo\n");
        assert_eq!(val, "héllo\n");
    }

    // UT-LB-H2: content with embedded null byte (valid in Rust strings)
    #[test]
    fn literal_content_with_backslash_is_preserved_verbatim() {
        // Backslashes are not escape sequences in literal block scalars.
        let (val, _) = lit_ok("|\n  foo\\bar\n");
        assert_eq!(val, "foo\\bar\n");
    }

    // UT-LB-H3: result is always Cow::Owned
    #[test]
    fn literal_result_is_always_cow_owned() {
        let mut lex = make_lexer("|\n  hello\n");
        let (cow, _, _) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert!(
            matches!(cow, Cow::Owned(_)),
            "literal block scalars must always produce Cow::Owned"
        );
    }

    // -----------------------------------------------------------------------
    // Group SPF: scan_plain_line_flow (Task 14)
    // -----------------------------------------------------------------------

    // SPF-1: plain word terminates at `]`
    #[test]
    fn flow_plain_terminates_at_close_bracket() {
        assert_eq!(scan_plain_line_flow("abc]rest"), "abc");
    }

    // SPF-2: plain word terminates at `}`
    #[test]
    fn flow_plain_terminates_at_close_brace() {
        assert_eq!(scan_plain_line_flow("abc}rest"), "abc");
    }

    // SPF-3: plain word terminates at `,`
    #[test]
    fn flow_plain_terminates_at_comma() {
        assert_eq!(scan_plain_line_flow("abc,rest"), "abc");
    }

    // SPF-4: plain word terminates at `[`
    #[test]
    fn flow_plain_terminates_at_open_bracket() {
        assert_eq!(scan_plain_line_flow("abc[rest"), "abc");
    }

    // SPF-5: plain word terminates at `{`
    #[test]
    fn flow_plain_terminates_at_open_brace() {
        assert_eq!(scan_plain_line_flow("abc{rest"), "abc");
    }

    // SPF-6: plain word is returned in full when no terminator
    #[test]
    fn flow_plain_returns_full_when_no_terminator() {
        assert_eq!(scan_plain_line_flow("hello"), "hello");
    }

    // SPF-7: empty input returns empty
    #[test]
    fn flow_plain_empty_input_returns_empty() {
        assert_eq!(scan_plain_line_flow(""), "");
    }

    // SPF-8: `#` preceded by whitespace starts a comment (terminates scalar)
    #[test]
    fn flow_plain_hash_after_space_starts_comment() {
        assert_eq!(scan_plain_line_flow("abc # comment"), "abc");
    }

    // SPF-9: `#` not preceded by whitespace is part of the scalar
    #[test]
    fn flow_plain_hash_without_preceding_space_is_content() {
        assert_eq!(scan_plain_line_flow("abc#def"), "abc#def");
    }

    // SPF-10: `:` followed by space terminates plain scalar
    #[test]
    fn flow_plain_colon_space_terminates() {
        assert_eq!(scan_plain_line_flow("key: rest"), "key");
    }

    // SPF-11: `:` followed by flow indicator terminates plain scalar
    #[test]
    fn flow_plain_colon_flow_indicator_terminates() {
        assert_eq!(scan_plain_line_flow("key:}rest"), "key");
    }

    // SPF-12: `:` at EOL terminates plain scalar (None next)
    #[test]
    fn flow_plain_colon_at_eol_terminates() {
        assert_eq!(scan_plain_line_flow("key:"), "key");
    }

    // SPF-13: `:` in the middle not followed by separator is part of scalar
    #[test]
    fn flow_plain_colon_followed_by_alnum_is_content() {
        assert_eq!(scan_plain_line_flow("a:b"), "a:b");
    }

    // SPF-14: trailing whitespace is not included in the result
    #[test]
    fn flow_plain_trailing_whitespace_excluded() {
        assert_eq!(scan_plain_line_flow("abc   "), "abc");
    }
}
