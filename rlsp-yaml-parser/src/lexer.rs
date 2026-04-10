// SPDX-License-Identifier: MIT

//! Line-level lexer: wraps [`LineBuffer`] and provides marker-detection and
//! line-consumption primitives consumed by the [`EventIter`] state machine.
//!
//! The lexer is lazy — it never buffers the whole input.  It advances through
//! the [`LineBuffer`] one line at a time, driven by the state machine.

use std::borrow::Cow;

use crate::chars::{decode_escape, is_c_printable};
use crate::error::Error;
use crate::event::Chomp;
use crate::lines::{BreakType, Line, LineBuffer};
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

    /// Try to consume the next line as a comment.
    ///
    /// Returns `Some((text, span))` if the next line is a comment (starts with
    /// `#` after optional leading whitespace), or `None` if the next line is
    /// blank, a directive, or content.
    ///
    /// `text` is the comment body: the slice after the `#`, excluding the
    /// newline.  Leading whitespace after `#` is preserved.
    ///
    /// Returns `Err` when the comment body exceeds `max_comment_len` bytes.
    pub fn try_consume_comment(
        &mut self,
        max_comment_len: usize,
    ) -> Result<Option<(&'input str, Span)>, crate::error::Error> {
        let Some(line) = self.buf.peek_next() else {
            return Ok(None);
        };

        let trimmed = line.content.trim_start_matches([' ', '\t']);
        if !trimmed.starts_with('#') {
            return Ok(None);
        }

        // The `#` is the first non-whitespace character.
        // Compute byte offset of `#` within line.content using char_indices —
        // security: byte-index from char_indices, never character-count arithmetic.
        let hash_byte_offset = line
            .content
            .char_indices()
            .find(|&(_, ch)| ch == '#')
            .map_or(0, |(i, _)| i);

        let hash_char_offset = line.content[..hash_byte_offset].chars().count();
        let hash_pos = Pos {
            byte_offset: line.pos.byte_offset + hash_byte_offset,
            char_offset: line.pos.char_offset + hash_char_offset,
            line: line.pos.line,
            column: line.pos.column + hash_char_offset,
        };

        // Comment text: everything after the `#`.
        // text_start is always ≤ line.content.len(): hash_byte_offset is from
        // char_indices() (so < len) and `#` is 1 byte, giving text_start ≤ len.
        // The slice is always on a char boundary because `#` is ASCII.
        let text_start = hash_byte_offset + 1; // byte after `#`
        let text: &'input str = &line.content[text_start..];

        if text.len() > max_comment_len {
            return Err(crate::error::Error {
                pos: hash_pos,
                message: format!(
                    "comment exceeds maximum allowed length ({max_comment_len} bytes)"
                ),
            });
        }

        // Span: from `#` through end of text (not the newline).
        let mut span_end = hash_pos.advance('#');
        for ch in text.chars() {
            span_end = span_end.advance(ch);
        }
        let span = Span {
            start: hash_pos,
            end: span_end,
        };

        // SAFETY: peek succeeded above; LineBuffer invariant.
        let Some(consumed) = self.buf.consume_next() else {
            unreachable!("try_consume_comment: peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&consumed);

        Ok(Some((text, span)))
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
            let prefix_chars = line.content[..prefix_bytes].chars().count();
            let inline_start = Pos {
                byte_offset: marker_pos.byte_offset + prefix_bytes,
                char_offset: marker_pos.char_offset + prefix_chars,
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
    #[allow(clippy::too_many_lines)]
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

        // SAFETY: LineBuffer guarantees consume returns Some when peek returned
        // Some on the same instance (single-threaded, no interleaving).
        let Some(consumed_first) = self.buf.consume_next() else {
            unreachable!("peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&consumed_first);

        // SAFETY: leading_spaces and first_value_len are computed by
        // peek_plain_scalar_first_line from the same line content via
        // char_indices(), guaranteeing char-boundary alignment and bounds.
        let Some(first_value_ref): Option<&'input str> = consumed_first
            .content
            .get(leading_spaces..leading_spaces + first_value_len)
        else {
            unreachable!("scalar slice out of bounds")
        };

        // Detect trailing comment on the same line as the scalar.
        // `scan_plain_line_block` already stopped at `# ` (whitespace-preceded
        // `#`), so the content after `leading_spaces + first_value_len` is
        // either empty, whitespace-only, or `  # comment`.
        // We use char_indices on the suffix — security: byte offsets only.
        let after_scalar_start = leading_spaces + first_value_len;
        if let Some(suffix) = consumed_first.content.get(after_scalar_start..) {
            if let Some(comment_text) = extract_trailing_comment(suffix) {
                // Compute the byte offset of `#` within the line.
                // It is `after_scalar_start + (suffix.len() - comment_text.len() - 1)`
                // where -1 accounts for the `#` itself.
                let hash_byte_in_line = after_scalar_start + suffix.len() - comment_text.len() - 1;
                let hash_char_in_line = consumed_first.content[..hash_byte_in_line].chars().count();
                let hash_pos = Pos {
                    byte_offset: consumed_first.pos.byte_offset + hash_byte_in_line,
                    char_offset: consumed_first.pos.char_offset + hash_char_in_line,
                    line: consumed_first.pos.line,
                    column: consumed_first.pos.column + hash_char_in_line,
                };
                let mut span_end = hash_pos.advance('#');
                for ch in comment_text.chars() {
                    span_end = span_end.advance(ch);
                }
                // Validate comment text: YAML 1.2 §8.1.1 — comment lines must
                // not contain NUL (U+0000) since it is not a c-printable char.
                if let Some((bad_i, bad_ch)) = comment_text.char_indices().find(|(_, c)| *c == '\0')
                {
                    let bad_char_i = comment_text[..bad_i].chars().count();
                    let bad_pos = Pos {
                        byte_offset: hash_pos.byte_offset + 1 + bad_i,
                        char_offset: hash_pos.char_offset + 1 + bad_char_i,
                        line: hash_pos.line,
                        column: hash_pos.column + 1 + bad_char_i,
                    };
                    self.plain_scalar_suffix_error = Some(Error {
                        pos: bad_pos,
                        message: format!("invalid character U+{:04X} in comment", bad_ch as u32),
                    });
                } else {
                    self.trailing_comment = Some((
                        comment_text,
                        Span {
                            start: hash_pos,
                            end: span_end,
                        },
                    ));
                }
            } else if let Some((bad_i, bad_ch)) = suffix
                .char_indices()
                .find(|(_, c)| matches!(*c, '\0' | '\u{FEFF}'))
            {
                // Suffix contains a character that stopped plain-scalar
                // scanning (NUL U+0000 or mid-stream BOM U+FEFF) and is not
                // valid at this position.  Other non-whitespace characters
                // (e.g. `: value`) may be valid YAML content that the mapping
                // detector missed and are not flagged here.
                let bad_char_offset = consumed_first.content[..after_scalar_start + bad_i]
                    .chars()
                    .count();
                let bad_pos = Pos {
                    byte_offset: consumed_first.pos.byte_offset + after_scalar_start + bad_i,
                    char_offset: consumed_first.pos.char_offset + bad_char_offset,
                    line: consumed_first.pos.line,
                    column: consumed_first.pos.column + bad_char_offset,
                };
                self.plain_scalar_suffix_error = Some(Error {
                    pos: bad_pos,
                    message: format!("invalid character U+{:04X} in plain scalar", bad_ch as u32),
                });
            }
        }

        // A trailing comment on the first line terminates the plain scalar —
        // continuation lines after a comment are not part of the scalar.
        let extra = if self.trailing_comment.is_some() || self.plain_scalar_suffix_error.is_some() {
            None
        } else {
            self.collect_plain_continuations(first_value_ref, parent_indent, consumed_first.indent)
        };

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
        scalar_indent: usize,
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
                // SAFETY: peek succeeded on this iteration; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume blank line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                continue;
            }

            if is_marker(next.content, b'-') || is_marker(next.content, b'.') {
                break;
            }

            // A continuation line is valid when it is strictly more indented
            // than the enclosing block (`indent > parent_indent`).
            //
            // Special case: when `parent_indent == 0` AND the scalar itself
            // started at column 0 (`scalar_indent == 0`), a continuation at
            // column 0 is also valid — `s-flow-folded(0)` allows any indentation ≥ 0
            // for scalars in the n=0 document-root context.
            // (YAML 1.2 spec example 7.12 / tests HS5T.)
            // A tab at the start of a continuation also satisfies `s-separate-in-line`
            // even when parent_indent=0 and scalar_indent>0.
            let n0_exception = parent_indent == 0 && scalar_indent == 0;
            let tab_exception = parent_indent == 0 && next.content.starts_with('\t');
            if next.indent <= parent_indent && !n0_exception && !tab_exception {
                break;
            }

            let cont_value = scan_plain_line_block(trimmed);
            if cont_value.is_empty() {
                break;
            }

            // If the plain scan stops short (not at end of content) and the
            // remaining content starts with `: ` (value indicator), this line
            // is an implicit mapping entry — the plain scalar terminates here.
            let after_cont = trimmed[cont_value.len()..].trim_start_matches([' ', '\t']);
            if after_cont.starts_with(": ") || after_cont == ":" {
                break;
            }

            // If the remainder after the scanned value is a comment (`# …`),
            // this line has a trailing comment that terminates the plain scalar.
            // Include the current line's content but do NOT continue after this.
            let has_trailing_comment = after_cont.starts_with('#');

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

            // SAFETY: peek succeeded on this iteration; LineBuffer invariant.
            let Some(consumed) = self.buf.consume_next() else {
                unreachable!("consume cont line failed")
            };
            self.current_pos = pos_after_line(&consumed);

            if has_trailing_comment {
                break;
            }
        }

        result
    }

    /// Try to tokenize a single-quoted scalar starting at the current line.
    ///
    /// Implements YAML 1.2 §7.3.2 `c-single-quoted` in block context.
    ///
    /// Returns:
    /// - `Ok(None)` — current line does not start with `'` (not a single-quoted scalar).
    /// - `Ok(Some((value, span)))` — successfully tokenized.
    /// - `Err(Error)` — started parsing (opening `'` seen) but hit a hard error
    ///   (e.g. unterminated string).
    ///
    /// **Borrow contract:** Single-line with no `''` escapes → `Cow::Borrowed`.
    /// Anything else (escapes or multi-line) → `Cow::Owned`.
    pub fn try_consume_single_quoted(
        &mut self,
        _parent_indent: usize,
    ) -> Result<Option<(Cow<'input, str>, Span)>, Error> {
        let Some(first_line) = self.buf.peek_next() else {
            return Ok(None);
        };
        let content = first_line.content.trim_start_matches([' ', '\t']);
        if !content.starts_with('\'') {
            return Ok(None);
        }

        let leading_bytes = first_line.content.len() - content.len();
        let leading_chars = first_line.content[..leading_bytes].chars().count();
        let open_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
            char_offset: first_line.pos.char_offset + leading_chars,
            line: first_line.pos.line,
            column: first_line.pos.column + leading_chars,
        };

        // Consume the first line.
        // SAFETY: LineBuffer guarantees consume returns Some when peek returned
        // Some on the same instance (single-threaded, no interleaving).
        let Some(consumed_first) = self.buf.consume_next() else {
            unreachable!("peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&consumed_first);

        // The body starts after the opening `'`.
        let body_start = &consumed_first.content[leading_bytes + 1..];

        // Scan within this line for the closing `'`, handling `''` escapes.
        let (value, closed) = scan_single_quoted_line(body_start);

        if closed {
            // Entire scalar on one line.
            // Span: from open `'` through closing `'`.
            let mut end_pos = open_pos.advance('\''); // past opening `'`
            for ch in body_start[..value.quoted_len].chars() {
                end_pos = end_pos.advance(ch);
            }
            end_pos = end_pos.advance('\''); // past closing `'`
            return Ok(Some((
                value.into_cow(body_start),
                Span {
                    start: open_pos,
                    end: end_pos,
                },
            )));
        }

        // Multi-line: must collect continuation lines.
        let mut owned = value.as_owned_string(body_start);

        loop {
            let Some(next) = self.buf.peek_next() else {
                // EOF without closing quote.
                return Err(Error {
                    pos: self.current_pos,
                    message: "unterminated single-quoted scalar".to_owned(),
                });
            };

            // Document markers at column 0 terminate the document even inside
            // quoted scalars (YAML spec §6.5 / test suite RXY3).
            if is_doc_marker_line(next.content) {
                return Err(Error {
                    pos: next.pos,
                    message: "document marker '...' or '---' is not allowed inside a quoted scalar"
                        .to_owned(),
                });
            }

            // SAFETY: peek succeeded in the let-else above; LineBuffer invariant.
            let Some(consumed) = self.buf.consume_next() else {
                unreachable!("peek returned Some but consume returned None")
            };
            let line_start_pos = consumed.pos;
            self.current_pos = pos_after_line(&consumed);
            let line_content = consumed.content;

            // Determine how this line participates in folding.
            let trimmed = line_content.trim_start_matches([' ', '\t']);

            if trimmed.is_empty() {
                // Blank continuation line: counts as a literal newline.
                owned.push('\n');
                continue;
            }

            // Non-blank continuation line: fold the preceding content.
            // The fold already has any newlines from blank lines above.
            // If last char is '\n' (blank lines were counted), no extra space.
            // If last char is something else, add a space.
            let last = owned.chars().next_back();
            if last != Some('\n') {
                // Remove trailing space/newline we may have appended for a
                // previous non-blank fold, then add the single-fold space.
                // Actually: the owned string ends with real content; just add space.
                owned.push(' ');
            }

            let (cont_value, cont_closed) = scan_single_quoted_line(trimmed);

            if cont_closed {
                if cont_value.has_escape {
                    owned.push_str(&unescape_single_quoted(trimmed, cont_value.quoted_len));
                } else {
                    owned.push_str(&trimmed[..cont_value.quoted_len]);
                }
                // Compute position right after the closing `'` by advancing from
                // the line start over leading whitespace + content + closing `'`.
                let leading_len = line_content.len() - trimmed.len();
                let mut close_pos = line_start_pos;
                for ch in line_content[..leading_len + cont_value.quoted_len].chars() {
                    close_pos = close_pos.advance(ch);
                }
                close_pos = close_pos.advance('\''); // past closing `'`
                // If there is content after the closing `'`, store it so the
                // flow parser can continue parsing `,`, `]`, `}`, etc.
                let tail = trimmed.get(cont_value.quoted_len + 1..).unwrap_or("");
                if !tail.is_empty() {
                    self.pending_multiline_tail = Some((tail, close_pos));
                }
                let end_pos = self.current_pos;
                return Ok(Some((
                    Cow::Owned(owned),
                    Span {
                        start: open_pos,
                        end: end_pos,
                    },
                )));
            }
            if cont_value.has_escape {
                owned.push_str(&unescape_single_quoted(trimmed, cont_value.quoted_len));
            } else {
                owned.push_str(&trimmed[..cont_value.quoted_len]);
            }
        }
    }

    /// Try to tokenize a double-quoted scalar starting at the current line.
    ///
    /// Implements YAML 1.2 §7.3.1 `c-double-quoted` in block context.
    ///
    /// Returns:
    /// - `Ok(None)` — current line does not start with `"`.
    /// - `Ok(Some((value, span)))` — successfully tokenized.
    /// - `Err(Error)` — started parsing but hit a hard error (invalid/truncated
    ///   escape sequence, unterminated string, or invalid codepoint).
    ///
    /// **Security:** Numeric escape sequences (`\xHH`, `\uHHHH`, `\UHHHHHHHH`)
    /// are validated via `chars::decode_escape` which rejects surrogates and
    /// codepoints > U+10FFFF.  Additionally, escaped bidi override characters
    /// (U+200E, U+200F, U+202A–U+202E, U+2066–U+2069) are rejected at the
    /// caller level.  Literal (unescaped) bidi characters in source are out of
    /// scope for this task.
    ///
    /// **Note:** `\0` produces a null byte (U+0000) in the output.  Rust
    /// `String` can hold null bytes.  C-FFI callers must handle embedded nulls.
    ///
    /// **Borrow contract:** Single-line with no escapes → `Cow::Borrowed`.
    /// Multi-line or any escape → `Cow::Owned`.
    pub fn try_consume_double_quoted(
        &mut self,
        block_context_indent: Option<usize>,
    ) -> Result<Option<(Cow<'input, str>, Span)>, Error> {
        let Some(first_line) = self.buf.peek_next() else {
            return Ok(None);
        };
        let content = first_line.content.trim_start_matches([' ', '\t']);
        if !content.starts_with('"') {
            return Ok(None);
        }

        let leading_bytes = first_line.content.len() - content.len();
        let leading_chars = first_line.content[..leading_bytes].chars().count();
        let open_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
            char_offset: first_line.pos.char_offset + leading_chars,
            line: first_line.pos.line,
            column: first_line.pos.column + leading_chars,
        };

        // Consume the first line.
        // SAFETY: LineBuffer guarantees consume returns Some when peek returned
        // Some on the same instance (single-threaded, no interleaving).
        let Some(consumed_first) = self.buf.consume_next() else {
            unreachable!("peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&consumed_first);

        // Body starts after the opening `"`.
        let body_start = &consumed_first.content[leading_bytes + 1..];

        // Try to scan on a single line (fast path / borrow path).
        let (value, span) = match scan_double_quoted_line(body_start, open_pos.advance('"'))? {
            DoubleQuotedLine::Closed {
                value,
                close_pos: end_pos,
                tail,
            } => {
                // Store any non-empty tail so the caller can validate or process it.
                if !tail.is_empty() {
                    self.pending_multiline_tail = Some((tail, end_pos));
                }
                (
                    value.into_cow(body_start),
                    Span {
                        start: open_pos,
                        end: end_pos,
                    },
                )
            }
            DoubleQuotedLine::Incomplete {
                value,
                line_continuation,
            } => {
                // Multi-line: accumulate.
                let mut owned = value.into_string();
                self.collect_double_quoted_continuations(
                    &mut owned,
                    line_continuation,
                    open_pos,
                    block_context_indent,
                )?;
                let end_pos = self.current_pos;
                (
                    Cow::Owned(owned),
                    Span {
                        start: open_pos,
                        end: end_pos,
                    },
                )
            }
        };
        Ok(Some((value, span)))
    }

    /// Try to tokenize a literal block scalar (`|`) starting at the current line.
    ///
    /// Implements YAML 1.2 §8.1.2 `c-l+literal` in block context.  The caller
    /// supplies `parent_indent` — the indentation level of the enclosing block
    /// node (`n` in the spec).
    ///
    /// Returns:
    /// - `None` — the current line does not start with `|` (not a literal block scalar).
    /// - `Some(Ok((value, chomp, span)))` — successfully tokenized.
    /// - `Some(Err(e))` — started parsing (opening `|` seen) but hit a hard error
    ///   (e.g. invalid indicator character, tab in indentation).
    ///
    /// **Borrow contract:** Always returns `Cow::Owned` — the content is
    /// assembled from stripped lines and does not exist contiguously in input.
    ///
    /// **Span:** Covers from the `|` through the last consumed line terminator.
    #[allow(clippy::too_many_lines)]
    pub fn try_consume_literal_block_scalar(
        &mut self,
        parent_indent: usize,
    ) -> LiteralBlockResult<'input> {
        // Check the current line starts with `|`.
        let first_line = self.buf.peek_next()?;
        let content = first_line.content.trim_start_matches([' ', '\t']);
        if !content.starts_with('|') {
            return None;
        }

        // Record the position of the `|` for the span start.
        let leading_bytes = first_line.content.len() - content.len();
        let leading_chars = first_line.content[..leading_bytes].chars().count();
        let pipe_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
            char_offset: first_line.pos.char_offset + leading_chars,
            line: first_line.pos.line,
            column: first_line.pos.column + leading_chars,
        };

        // Consume the header line.
        // SAFETY: LineBuffer guarantees consume returns Some when peek returned
        // Some on the same instance (single-threaded, no interleaving).
        let Some(header_line) = self.buf.consume_next() else {
            unreachable!("peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&header_line);

        // Parse the header: `|` [indent-indicator] [chomp-indicator] [comment]
        // Indicators can appear in either order: `|+2` or `|2+`.
        let after_pipe = &content[1..]; // everything after `|`
        let (chomp, explicit_indent, header_err) = parse_block_header(after_pipe, pipe_pos);
        if let Some(e) = header_err {
            return Some(Err(e));
        }

        // Determine content indent.
        // If explicit indicator given: content_indent = parent_indent + explicit_indent.
        // Otherwise: auto-detect by scanning forward to the first non-blank content line.
        // `content_indent_known` is true when content_indent is derived from an actual
        // non-blank line (or an explicit indicator); false when it falls back to the
        // default (parent_indent + 1) because no content lines exist.  The spec rule
        // "leading empty lines must not exceed the first non-empty line's indent" only
        // applies when there IS a first non-empty line (content_indent_known = true).
        // `effective_min` is `parent_indent + 1` when parent_indent is a real
        // value, or `1` when parent_indent == usize::MAX (root-level sentinel,
        // meaning the scalar has no enclosing collection and body lines may
        // start at column 0 per YAML spec §8.1: body indent > -1).
        let effective_min = if parent_indent == usize::MAX {
            1_usize
        } else {
            parent_indent + 1
        };

        let (content_indent, content_indent_known): (usize, bool) =
            if let Some(explicit) = explicit_indent {
                // Explicit indent indicator: content_indent = parent_indent + indicator.
                // For root level (usize::MAX), treat parent as 0 for arithmetic.
                let base = if parent_indent == usize::MAX {
                    0
                } else {
                    parent_indent
                };
                (base + explicit, true)
            } else {
                // Use peek_until_dedent to scan for the first non-blank line and
                // read its indent.  base_indent = parent_indent so we stop at
                // any line with indent <= parent_indent (usize::MAX means no limit).
                let lookahead = self.buf.peek_until_dedent(parent_indent);
                // Find the first non-blank line's indent.
                lookahead
                    .iter()
                    .find(|l| !l.content.trim_matches([' ', '\t']).is_empty())
                    .map_or((effective_min, false), |l| (l.indent, true))
            };

        // Collect content lines.
        let mut out = String::new();
        // Count pending transparent blank lines (not yet pushed, for chomping).
        // These are lines with indent < content_indent (or truly empty lines).
        let mut trailing_newlines: usize = 0;
        // Track whether we've seen the first line with actual non-whitespace
        // characters.  Per YAML 1.2 §8.1.1.1: "It is an error for any of the
        // leading empty lines to contain more spaces than the first non-empty
        // line."  A spaces-only line with indent > content_indent is therefore
        // an error if it precedes any real content.
        let mut before_first_real_content = true;

        loop {
            let Some(next) = self.buf.peek_next() else {
                break;
            };

            let line_content = next.content;

            // Tab at the very start of a line means the line uses a tab as
            // indentation, which is a YAML error.
            if line_content.starts_with('\t') {
                let tab_pos = next.pos;
                // SAFETY: peek succeeded above; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume failed")
                };
                self.current_pos = pos_after_line(&consumed);
                return Some(Err(Error {
                    pos: tab_pos,
                    message: "tab character is not valid indentation in a block scalar".to_owned(),
                }));
            }

            // Classify this line:
            // - If indent >= content_indent: content line (may be spaces-only
            //   after the indent prefix, but that's still content).
            // - Otherwise (indent < content_indent): transparent blank — counts
            //   as a newline but does not terminate if the line is whitespace-only
            //   (per spec `l-empty(n,c)`). If it has non-whitespace characters,
            //   it's a dedent terminator.
            // A line is a content line if:
            // 1. Its indent >= content_indent, AND
            // 2. After stripping content_indent leading spaces, at least one
            //    nb-char (non-break char, including spaces) remains.
            //
            // A line is blank (l-empty) if it has indent < content_indent, or
            // if after stripping content_indent spaces the remaining content is
            // completely empty. In the blank case we check for non-whitespace
            // to decide between transparent (blank → newline) vs terminator.
            let after_indent = if next.indent >= content_indent {
                line_content.get(content_indent..).unwrap_or("")
            } else {
                ""
            };

            let is_content_line = next.indent >= content_indent && !after_indent.is_empty();

            if is_content_line {
                // Leading all-whitespace lines with indent > content_indent are
                // errors (spec §8.1.1.1: leading empty lines must not exceed the
                // first non-empty line's indentation).  Only applies when
                // content_indent was derived from an actual non-blank line or an
                // explicit indicator (content_indent_known = true).
                let has_real_content = !after_indent.trim_end_matches([' ', '\t']).is_empty();
                if content_indent_known
                    && before_first_real_content
                    && !has_real_content
                    && next.indent > content_indent
                {
                    let blank_pos = next.pos;
                    let Some(consumed) = self.buf.consume_next() else {
                        unreachable!("consume over-indented blank failed")
                    };
                    self.current_pos = pos_after_line(&consumed);
                    return Some(Err(Error {
                        pos: blank_pos,
                        message: "block scalar blank line has more indentation than the content"
                            .to_owned(),
                    }));
                }
                if has_real_content {
                    before_first_real_content = false;
                }

                // Content line. Flush any pending blank lines first.
                for _ in 0..trailing_newlines {
                    out.push('\n');
                }
                trailing_newlines = 0;

                // SAFETY: peek succeeded on this loop iteration; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume content line failed")
                };
                self.current_pos = pos_after_line(&consumed);

                out.push_str(after_indent);
                // Only push a newline if the physical line had one.
                // A line ending with BreakType::Eof means the input ended
                // without a trailing newline — no b-as-line-feed is emitted.
                if consumed.break_type != BreakType::Eof {
                    out.push('\n');
                }
            } else {
                // Blank line (indent < content_indent, or content after indent
                // is empty). Check whether it terminates the scalar.
                let trimmed = line_content.trim_matches([' ', '\t']);
                if !trimmed.is_empty() {
                    // Non-whitespace at dedented position: terminates the scalar.
                    break;
                }
                // Whitespace-only line: transparent (l-empty). Count as newline.
                // SAFETY: peek succeeded on this loop iteration; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume blank line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                trailing_newlines += 1;
            }
        }

        // Apply chomping to the trailing newlines.
        // At this point `out` ends with `\n` from the last content line (if any),
        // and `trailing_newlines` counts blank lines following that last content line.
        let value = apply_chomping(out, trailing_newlines, chomp);

        let span = Span {
            start: pipe_pos,
            end: self.current_pos,
        };

        Some(Ok((Cow::Owned(value), chomp, span)))
    }

    /// Try to tokenize a folded block scalar (`>`) starting at the current line.
    ///
    /// Implements YAML 1.2 §8.1.3 `c-l+folded` in block context.  The caller
    /// supplies `parent_indent` — the indentation level of the enclosing block
    /// node (`n` in the spec).
    ///
    /// **Borrow contract:** Always returns `Cow::Owned` — folding assembles a
    /// transformed string that does not exist contiguously in the input.
    ///
    /// **Security:** Collection is O(n) in input size; no amplification.
    /// `peek_until_dedent` may scan O(n) ahead — pre-existing constraint shared
    /// with literal scalars.
    pub fn try_consume_folded_block_scalar(
        &mut self,
        parent_indent: usize,
    ) -> LiteralBlockResult<'input> {
        let first_line = self.buf.peek_next()?;
        let content = first_line.content.trim_start_matches([' ', '\t']);
        if !content.starts_with('>') {
            return None;
        }

        let leading_bytes = first_line.content.len() - content.len();
        let leading_chars = first_line.content[..leading_bytes].chars().count();
        let gt_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
            char_offset: first_line.pos.char_offset + leading_chars,
            line: first_line.pos.line,
            column: first_line.pos.column + leading_chars,
        };

        // SAFETY: LineBuffer guarantees consume returns Some when peek returned
        // Some on the same instance (single-threaded, no interleaving).
        let Some(header_line) = self.buf.consume_next() else {
            unreachable!("peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&header_line);

        // Parse the header — reuse `parse_block_header`; works identically for `>` and `|`.
        let after_gt = &content[1..];
        let (chomp, explicit_indent, header_err) = parse_block_header(after_gt, gt_pos);
        if let Some(e) = header_err {
            return Some(Err(e));
        }

        let effective_min = if parent_indent == usize::MAX {
            1_usize
        } else {
            parent_indent + 1
        };
        let content_indent: usize = if let Some(explicit) = explicit_indent {
            let base = if parent_indent == usize::MAX {
                0
            } else {
                parent_indent
            };
            base + explicit
        } else {
            let lookahead = self.buf.peek_until_dedent(parent_indent);
            lookahead
                .iter()
                .find(|l| !l.content.trim_matches([' ', '\t']).is_empty())
                .map_or(effective_min, |l| l.indent)
        };

        let (content_result, trailing_newlines) = self.collect_folded_lines(content_indent);
        let folded = match content_result {
            Ok(s) => s,
            Err(e) => return Some(Err(e)),
        };
        let value = apply_chomping(folded, trailing_newlines, chomp);
        let span = Span {
            start: gt_pos,
            end: self.current_pos,
        };
        Some(Ok((Cow::Owned(value), chomp, span)))
    }

    /// Collect and fold content lines for a folded block scalar.
    ///
    /// Returns `(content, trailing_blank_count)`.
    ///
    /// The physical line break after each content line is deferred — it becomes
    /// the inter-line separator (space, `\n`, or N newlines) when the next line
    /// is classified, per YAML 1.2 §8.1.3 folding rules:
    ///
    /// - Single break, both lines equally indented → space.
    /// - Single break surrounding a more-indented line → `\n` (preserved).
    /// - N blank lines between non-blank lines → N `\n`s.
    fn collect_folded_lines(&mut self, content_indent: usize) -> (Result<String, Error>, usize) {
        let mut out = String::new();
        let mut trailing_newlines: usize = 0;
        let mut last_had_break = false;
        let mut prev_more_indented = false;
        let mut has_content = false;

        loop {
            let Some(next) = self.buf.peek_next() else {
                break;
            };

            let line_content = next.content;

            if line_content.starts_with('\t') {
                let tab_pos = next.pos;
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume failed")
                };
                self.current_pos = pos_after_line(&consumed);
                return (
                    Err(Error {
                        pos: tab_pos,
                        message: "tab character is not valid indentation in a block scalar"
                            .to_owned(),
                    }),
                    0,
                );
            }

            let after_indent = if next.indent >= content_indent {
                line_content.get(content_indent..).unwrap_or("")
            } else {
                ""
            };
            // A content line must have a non-whitespace character after the indent.
            let is_content_line = next.indent >= content_indent
                && !after_indent.trim_end_matches([' ', '\t']).is_empty();

            if is_content_line {
                let is_more_indented = next.indent > content_indent;
                if has_content {
                    if trailing_newlines > 0 {
                        // N blank lines → N newlines (first break discarded).
                        for _ in 0..trailing_newlines {
                            out.push('\n');
                        }
                    } else if prev_more_indented || is_more_indented {
                        // Break surrounding a more-indented line is preserved.
                        out.push('\n');
                    } else {
                        // Single break between equally-indented lines → space.
                        out.push(' ');
                    }
                } else {
                    // Leading blank lines before first content line.
                    for _ in 0..trailing_newlines {
                        out.push('\n');
                    }
                }
                trailing_newlines = 0;

                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume content line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                out.push_str(after_indent);
                // Defer the physical break — decided as separator by the next line.
                last_had_break = consumed.break_type != BreakType::Eof;
                prev_more_indented = is_more_indented;
                has_content = true;
            } else {
                let trimmed = line_content.trim_matches([' ', '\t']);
                if !trimmed.is_empty() {
                    break; // Dedented non-whitespace terminates the scalar.
                }
                // Whitespace-only blank line: validate l-empty indentation constraint.
                // Per YAML 1.2 §8.1.1 rule 175, a blank line must have at most
                // content_indent leading spaces.  More spaces is a parse error.
                if next.indent > content_indent {
                    let blank_pos = next.pos;
                    let Some(consumed) = self.buf.consume_next() else {
                        unreachable!("consume over-indented blank failed")
                    };
                    self.current_pos = pos_after_line(&consumed);
                    return (
                        Err(Error {
                            pos: blank_pos,
                            message:
                                "block scalar blank line has more indentation than the content"
                                    .to_owned(),
                        }),
                        0,
                    );
                }
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume blank line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                trailing_newlines += 1;
            }
        }

        // Append the final content line's physical break so `apply_chomping`
        // sees the canonical `\n`-terminated content.
        if has_content && last_had_break {
            out.push('\n');
        }

        (Ok(out), trailing_newlines)
    }

    /// Collect continuation lines for a multi-line double-quoted scalar.
    ///
    /// `owned` is the accumulated content so far (from the first line).
    /// `line_continuation` indicates whether the first line ended with `\<LF>`
    /// (which suppresses the fold space).
    fn collect_double_quoted_continuations(
        &mut self,
        owned: &mut String,
        mut line_continuation: bool,
        open_pos: Pos,
        block_context_indent: Option<usize>,
    ) -> Result<(), Error> {
        let mut pending_blanks: usize = 0;

        loop {
            let Some(next) = self.buf.peek_next() else {
                return Err(Error {
                    pos: self.current_pos,
                    message: "unterminated double-quoted scalar".to_owned(),
                });
            };

            // Document markers at column 0 terminate the document even inside
            // quoted scalars (YAML spec §6.5 / test suite 5TRB).
            if is_doc_marker_line(next.content) {
                return Err(Error {
                    pos: next.pos,
                    message: "document marker '...' or '---' is not allowed inside a quoted scalar"
                        .to_owned(),
                });
            }

            let trimmed = next.content.trim_start_matches([' ', '\t']);

            // In block context, continuation lines of a double-quoted scalar
            // must be indented more than the enclosing block (YAML 1.2 §7.3.1).
            // A non-blank continuation line at indent <= n is invalid.
            // At document root (no enclosing block), there is no constraint.
            if let Some(n) = block_context_indent {
                if !trimmed.is_empty() && next.indent <= n {
                    return Err(Error {
                        pos: next.pos,
                        message: format!(
                            "double-quoted scalar continuation line must be indented more than {n}"
                        ),
                    });
                }
            }

            if trimmed.is_empty() {
                // Blank continuation line.
                pending_blanks += 1;
                // SAFETY: peek succeeded above; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume blank line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                continue;
            }

            // Non-blank continuation line: apply fold separator.
            // If line_continuation is true (`\<LF>` ended the prior line),
            // the break is suppressed — no separator and leading whitespace
            // on this line (already stripped into `trimmed`) is discarded.
            if !line_continuation {
                if pending_blanks > 0 {
                    // Blank lines → literal newlines (N blank lines → N newlines).
                    for _ in 0..pending_blanks {
                        owned.push('\n');
                    }
                } else {
                    // Normal fold: single newline between non-blank lines → space.
                    owned.push(' ');
                }
            }
            pending_blanks = 0;

            // SAFETY: peek succeeded above; LineBuffer invariant.
            let Some(consumed) = self.buf.consume_next() else {
                unreachable!("consume cont line failed")
            };
            self.current_pos = pos_after_line(&consumed);

            let line_start_pos = consumed.pos;
            match scan_double_quoted_line(trimmed, line_start_pos)? {
                DoubleQuotedLine::Closed {
                    value,
                    close_pos,
                    tail,
                } => {
                    value.push_into(owned);
                    // Store the tail (content after closing `"` on the closing
                    // line) so the flow parser can prepend it as a synthetic
                    // line to continue processing `,`, `]`, `}`, etc.
                    if !tail.is_empty() {
                        self.pending_multiline_tail = Some((tail, close_pos));
                    }
                    return Ok(());
                }
                DoubleQuotedLine::Incomplete {
                    value,
                    line_continuation: next_cont,
                } => {
                    value.push_into(owned);
                    line_continuation = next_cont;
                    // continue loop
                    let _ = open_pos;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Literal block scalar helpers
// ---------------------------------------------------------------------------

/// Return type of [`Lexer::try_consume_literal_block_scalar`].
///
/// `None` — not a literal block scalar.
/// `Some(Ok(...))` — successfully tokenized `(value, chomp, span)`.
/// `Some(Err(...))` — parse error.
type LiteralBlockResult<'a> = Option<Result<(Cow<'a, str>, Chomp, Span), Error>>;

/// Parse the block scalar header following the `|` character.
///
/// `after_pipe` is the slice starting immediately after `|`.
/// Returns `(chomp, explicit_indent, error)`.
///
/// - `explicit_indent` is `Some(n)` for `|n` or `None` for auto-detect.
/// - Error is `Some(Error)` for invalid indicator characters.
#[allow(clippy::too_many_lines)]
fn parse_block_header(after_pipe: &str, pipe_pos: Pos) -> (Chomp, Option<usize>, Option<Error>) {
    // Try to parse indicator characters. They can appear in either order:
    // indent-then-chomp or chomp-then-indent.
    let mut chomp: Option<Chomp> = None;
    let mut explicit_indent: Option<usize> = None;
    let mut pos = pipe_pos.advance('|');
    let mut byte_offset: usize = 0;

    // We track how many indicator chars we consumed to detect `|99` (two digits).
    loop {
        let remaining = &after_pipe[byte_offset..];
        match remaining.chars().next() {
            None | Some(' ' | '\t' | '\n' | '\r') => {
                // End of indicators: whitespace or line end.
                break;
            }
            Some('#') => {
                // `#` immediately after indicator (no whitespace) is an error.
                return (
                    Chomp::Clip,
                    None,
                    Some(Error {
                        pos,
                        message:
                            "comment after block scalar indicator requires at least one space before '#'"
                                .to_owned(),
                    }),
                );
            }
            Some(ch) => {
                if ch == '+' {
                    if chomp.is_some() {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "duplicate chomp indicator in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    chomp = Some(Chomp::Keep);
                    byte_offset += ch.len_utf8();
                    pos = pos.advance(ch);
                } else if ch == '-' {
                    if chomp.is_some() {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "duplicate chomp indicator in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    chomp = Some(Chomp::Strip);
                    byte_offset += ch.len_utf8();
                    pos = pos.advance(ch);
                } else if ch.is_ascii_digit() {
                    if ch == '0' {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "indent indicator '0' is not valid in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    if explicit_indent.is_some() {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "duplicate indent indicator in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    explicit_indent = Some(ch as usize - '0' as usize);
                    byte_offset += ch.len_utf8();
                    pos = pos.advance(ch);
                } else {
                    // Invalid indicator character.
                    return (
                        Chomp::Clip,
                        None,
                        Some(Error {
                            pos,
                            message: format!("invalid block scalar indicator character '{ch}'"),
                        }),
                    );
                }
            }
        }
    }

    // After indicators, only optional whitespace followed by optional comment
    // (or end of line) is allowed.  Non-whitespace, non-comment content is invalid.
    let remaining = &after_pipe[byte_offset..];
    let after_ws = remaining.trim_start_matches([' ', '\t']);
    if !after_ws.is_empty() && !after_ws.starts_with('#') {
        // Non-comment, non-whitespace content after indicators.
        return (
            Chomp::Clip,
            None,
            Some(Error {
                pos,
                message: "invalid content after block scalar indicator".to_owned(),
            }),
        );
    }

    (chomp.unwrap_or(Chomp::Clip), explicit_indent, None)
}

/// Apply chomping rules to the assembled scalar content.
///
/// `content` is the raw assembled content (ends with `\n` from the last
/// content line, if any content exists).
/// `trailing_blank_count` is the number of blank lines that followed the last
/// content line.
///
/// Chomping rules (spec §8.1.1.2):
/// - Strip: remove all trailing newlines (the `\n` from the last content line
///   and any blank lines).
/// - Clip: keep exactly one trailing newline (the `\n` from the last content
///   line).  If content is empty, result is "".
/// - Keep: keep the `\n` from the last content line plus all blank lines.
fn apply_chomping(mut content: String, trailing_blank_count: usize, chomp: Chomp) -> String {
    match chomp {
        Chomp::Strip => {
            // Remove the trailing `\n` added after the last content line,
            // plus any blank lines.
            if content.ends_with('\n') {
                content.pop();
            }
            // content already has no trailing blanks (they were counted separately).
        }
        Chomp::Clip => {
            // Keep exactly one trailing `\n`.  The content already ends with `\n`
            // (if non-empty) from the last content line — that's the one to keep.
            // Blank lines are discarded.
        }
        Chomp::Keep => {
            // Keep the trailing `\n` from the last content line plus all blank lines.
            for _ in 0..trailing_blank_count {
                content.push('\n');
            }
        }
    }
    content
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

    let leading_bytes = first.content.len() - content_trimmed.len();
    let leading_chars = first.content[..leading_bytes].chars().count();
    let scalar_start_pos = Pos {
        byte_offset: first.offset + leading_bytes,
        char_offset: first.pos.char_offset + leading_chars,
        line: first.pos.line,
        column: first.pos.column + leading_chars,
    };

    Some((leading_bytes, scalar_start_pos, first_value.len()))
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

/// Scan a plain scalar from `content` (flow context, after leading whitespace
/// has been stripped).
///
/// Flow plain scalars (YAML 1.2 §7.3.3) cannot contain flow indicators
/// (`,`, `[`, `]`, `{`, `}`) or a `:` that is followed by a space, tab, or
/// flow indicator.  This function returns the longest prefix of `content` that
/// is a valid flow plain scalar, trimmed of trailing whitespace.
///
/// This is `pub(crate)` so the flow parser in `lib.rs` can call it without
/// routing through the Lexer struct — the input slice is already available at
/// the call site.  Callers must not pass flow-context content to
/// [`scan_plain_line_block`] — the block scanner does not stop at flow
/// indicators.
pub fn scan_plain_line_flow(content: &str) -> &str {
    let mut chars = content.char_indices().peekable();
    let mut committed_end: usize = 0;
    let mut prev_was_ws = false;

    while let Some((i, ch)) = chars.next() {
        if matches!(ch, '\n' | '\r') {
            break;
        }

        // Flow indicators always terminate a plain scalar.
        if matches!(ch, ',' | '[' | ']' | '{' | '}') {
            break;
        }

        if is_s_white(ch) {
            prev_was_ws = true;
            continue;
        }

        // `#` preceded by whitespace (or at position 0) starts a comment.
        if ch == '#' && (i == 0 || prev_was_ws) {
            break;
        }

        // `:` in flow context: terminates when followed by a space, tab,
        // flow indicator, or end-of-content.  (YAML 1.2 §7.3.3)
        if ch == ':' {
            let next = chars.peek().map(|(_, c)| *c);
            match next {
                None | Some(' ' | '\t' | ',' | '[' | ']' | '{' | '}') => break,
                _ => {}
            }
        }

        if !ns_plain_safe_block(ch) {
            break;
        }

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
pub const fn is_ns_char(ch: char) -> bool {
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

/// True when `line` is blank (empty or whitespace-only) but NOT a comment.
///
/// Used by [`Lexer::skip_empty_lines`] which stops at comment lines so the
/// state machine can emit `Event::Comment` for them.
fn is_blank_not_comment(line: &Line<'_>) -> bool {
    line.content.trim_start_matches([' ', '\t']).is_empty()
}

/// Extract a trailing comment from the content that follows a scalar value.
///
/// `suffix` is the slice of the line after the scalar ends (may be empty,
/// whitespace-only, or `"  # comment"`).
///
/// Returns the comment body (everything after the `#`) if a comment is
/// present (i.e. `#` is preceded by at least one whitespace), or `None`
/// if there is no comment in this suffix.
///
/// Safety: uses `char_indices` for byte offsets — never character-count
/// arithmetic — to guarantee char-boundary slicing.
pub fn extract_trailing_comment(suffix: &str) -> Option<&str> {
    let mut prev_was_ws = true; // position before suffix content = boundary
    for (i, ch) in suffix.char_indices() {
        if ch == '#' && prev_was_ws {
            // `#` preceded by whitespace (or at the very start): comment start.
            // Return everything after `#`.
            // SAFETY: i + 1 is a valid char boundary because `#` is ASCII (1 byte).
            return Some(&suffix[i + 1..]);
        }
        prev_was_ws = matches!(ch, ' ' | '\t');
    }
    None
}

/// True when `line` is blank (empty or whitespace-only) or comment-only.
///
/// Does **not** treat `%`-prefixed lines as skippable — inside a document body
/// a `%`-prefixed line is regular content (e.g. `%complete: 50`).
///
/// Used by [`Lexer::has_content`] which must return `false` for both blank
/// and comment-only lines.
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

/// Return `true` if `content` is a document-start (`---`) or document-end
/// (`...`) marker at column 0.
///
/// Used to detect forbidden markers inside multi-line quoted scalars.
fn is_doc_marker_line(content: &str) -> bool {
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
// Single-quoted scalar scanning helpers
// ---------------------------------------------------------------------------

/// Result of scanning one line of a single-quoted scalar body.
struct SingleQuotedScan {
    /// Byte length of the accepted content inside the line (after `''` unescape).
    /// For borrowed case this is a slice length; for owned it's the source chars
    /// counted up to and including the `''` / closing `'`.
    ///
    /// This is the length of the *source* content that was consumed, used to
    /// compute the span end position.
    quoted_len: usize,
    /// Whether the scanning found the closing `'` on this line.
    has_escape: bool,
}

impl SingleQuotedScan {
    /// Convert to a `Cow` borrowing from `body` (the line slice starting after
    /// the opening `'`).
    ///
    /// `body` is the full line content after the opening quote.  If the scalar
    /// closed on this line, the slice up to the closing `'` is used.
    fn into_cow(self, body: &str) -> Cow<'_, str> {
        if self.has_escape {
            Cow::Owned(unescape_single_quoted(body, self.quoted_len))
        } else {
            // No escapes: borrow directly.
            // SAFETY: quoted_len is computed by scan_single_quoted_line which
            // advances via char::len_utf8(), guaranteeing char-boundary alignment
            // and that quoted_len <= body.len().
            let Some(slice) = body.get(..self.quoted_len) else {
                unreachable!("quoted_len out of bounds")
            };
            Cow::Borrowed(slice)
        }
    }

    /// Convert to an owned `String` from `body` (used for multi-line start).
    fn as_owned_string(&self, body: &str) -> String {
        if self.has_escape {
            unescape_single_quoted(body, self.quoted_len)
        } else {
            // SAFETY: same invariant as into_cow — quoted_len is char-boundary aligned.
            let Some(slice) = body.get(..self.quoted_len) else {
                unreachable!("quoted_len out of bounds")
            };
            slice.to_owned()
        }
    }
}

/// Scan one line of single-quoted content (after the opening `'` has been
/// stripped from `body`).
///
/// Returns `(scan, closed)`:
/// - `closed` is `true` when the closing `'` was found on this line.
/// - `scan.quoted_len` is the byte length of content consumed (not counting the
///   closing `'` itself).
/// - `scan.has_escape` is `true` when any `''` was present.
fn scan_single_quoted_line(body: &str) -> (SingleQuotedScan, bool) {
    let mut i = 0;
    let bytes = body.as_bytes();
    let mut has_escape = false;

    while i < bytes.len() {
        if bytes.get(i) == Some(&b'\'') {
            // Either closing `'` or `''` escape.
            if bytes.get(i + 1) == Some(&b'\'') {
                // `''` escape: consume both, continue.
                has_escape = true;
                i += 2;
            } else {
                // Closing `'`.
                return (
                    SingleQuotedScan {
                        quoted_len: i,
                        has_escape,
                    },
                    true,
                );
            }
        } else {
            // Advance by one character (handle multibyte).
            let ch_len = body
                .get(i..)
                .and_then(|s| s.chars().next())
                .map_or(1, char::len_utf8);
            i += ch_len;
        }
    }

    // Reached end of line without closing `'`.
    (
        SingleQuotedScan {
            quoted_len: i,
            has_escape,
        },
        false,
    )
}

/// Produce the unescaped value of a single-quoted line, replacing `''` with `'`.
///
/// `body` is the line after the opening `'`.
/// `content_len` is the byte length of the content (not counting closing `'`).
fn unescape_single_quoted(body: &str, content_len: usize) -> String {
    let mut out = String::with_capacity(content_len);
    // SAFETY: content_len equals quoted_len from scan_single_quoted_line, which
    // advances via char::len_utf8() — always char-boundary aligned and <= body.len().
    let Some(src) = body.get(..content_len) else {
        unreachable!("content_len out of bounds")
    };
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) == Some(&b'\'') && bytes.get(i + 1) == Some(&b'\'') {
            out.push('\'');
            i += 2;
        } else {
            let ch = src.get(i..).and_then(|s| s.chars().next()).unwrap_or('\0');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Double-quoted scalar scanning helpers
// ---------------------------------------------------------------------------

/// Result of scanning one line of a double-quoted scalar body.
enum DoubleQuotedLine<'a> {
    /// The closing `"` was found.
    Closed {
        value: DoubleQuotedValue<'a>,
        close_pos: Pos,
        /// Content that follows the closing `"` on the same line (may be empty).
        tail: &'a str,
    },
    /// End of line without closing `"`.
    Incomplete {
        value: DoubleQuotedValue<'a>,
        /// Whether the line ended with `\<LF>` (line continuation escape).
        line_continuation: bool,
    },
}

/// Content accumulated during double-quoted scanning.
enum DoubleQuotedValue<'a> {
    /// No escapes and no transformation — can borrow.
    Borrowed(&'a str),
    /// Escapes or other transformation occurred — must own.
    Owned(String),
}

impl<'a> DoubleQuotedValue<'a> {
    fn into_cow(self, _body: &'a str) -> Cow<'a, str> {
        match self {
            Self::Borrowed(s) => Cow::Borrowed(s),
            Self::Owned(s) => Cow::Owned(s),
        }
    }

    /// Push this value into `out`.
    fn push_into(self, out: &mut String) {
        match self {
            Self::Borrowed(s) => out.push_str(s),
            Self::Owned(s) => out.push_str(&s),
        }
    }

    fn into_string(self) -> String {
        match self {
            Self::Borrowed(s) => s.to_owned(),
            Self::Owned(s) => s,
        }
    }
}

/// True if `ch` is a bidirectional control character that should not be
/// introduced silently via an escape sequence.
const fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{200E}'
            | '\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

/// Scan one line of double-quoted content (after the opening `"` has been
/// stripped from `body`).
///
/// Decode one backslash escape sequence from `after_backslash`, apply
/// security checks, push the decoded character into `owned`, and return the
/// number of bytes consumed (not counting the leading `\`).
///
/// Returns `Err` for invalid escapes, non-printable hex results, or bidi
/// characters.  Also enforces the 1 MiB scalar length cap on `owned`.
fn decode_and_push_escape(
    after_backslash: &str,
    escape_pos: Pos,
    owned: &mut Option<String>,
    prefix: &str,
) -> Result<usize, Error> {
    let Some((decoded_ch, consumed)) = decode_escape(after_backslash) else {
        return Err(Error {
            pos: escape_pos,
            message: format!(
                "invalid escape sequence '\\{}'",
                after_backslash
                    .chars()
                    .next()
                    .map_or_else(|| "EOF".to_owned(), |c| c.to_string())
            ),
        });
    };

    // Security: for hex escapes (\x, \u, \U), the decoded character must
    // be a YAML c-printable character.  Named escapes (\0, \a, \b, …)
    // produce well-known control chars and are exempt from this check.
    let escape_prefix = after_backslash.chars().next().unwrap_or('\0');
    if matches!(escape_prefix, 'x' | 'u' | 'U') && !is_c_printable(decoded_ch) {
        return Err(Error {
            pos: escape_pos,
            message: format!(
                "escape produces non-printable character U+{:04X}",
                u32::from(decoded_ch)
            ),
        });
    }

    // Security: reject bidi override characters produced by numeric
    // escapes (\u and \U can reach the bidi range; \x max is U+00FF).
    if is_bidi_control(decoded_ch) {
        return Err(Error {
            pos: escape_pos,
            message: format!(
                "escape produces bidirectional control character U+{:04X}",
                u32::from(decoded_ch)
            ),
        });
    }

    let buf = ensure_owned(owned, prefix);
    buf.push(decoded_ch);

    // Maximum scalar length cap: 1 MiB.
    if buf.len() > 1_048_576 {
        return Err(Error {
            pos: escape_pos,
            message: "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
        });
    }

    Ok(consumed)
}

/// `start_pos` is the position of the first character of `body` (i.e. the byte
/// after the opening `"`), used only for error reporting.
fn scan_double_quoted_line(body: &str, start_pos: Pos) -> Result<DoubleQuotedLine<'_>, Error> {
    let mut i = 0;
    // We delay allocation until the first escape or discovery of multi-line.
    let mut owned: Option<String> = None;
    // Borrow end (used only while `owned` is `None`).
    let mut borrow_end: usize = 0;

    while i < body.len() {
        let ch = body[i..].chars().next().unwrap_or('\0');

        match ch {
            '"' => {
                // Closing quote.
                let content_end_pos = {
                    let mut p = start_pos;
                    for c in body[..i].chars() {
                        p = p.advance(c);
                    }
                    p
                };
                let close_pos = content_end_pos.advance('"');
                let value = owned.map_or_else(
                    || DoubleQuotedValue::Borrowed(body.get(..i).unwrap_or("")),
                    DoubleQuotedValue::Owned,
                );
                // `tail` is whatever follows the closing `"` on this line.
                let tail = body.get(i + 1..).unwrap_or("");
                return Ok(DoubleQuotedLine::Closed {
                    value,
                    close_pos,
                    tail,
                });
            }
            '\\' => {
                // Escape sequence.
                let escape_pos = {
                    let mut p = start_pos;
                    for c in body[..i].chars() {
                        p = p.advance(c);
                    }
                    p
                };
                let after_backslash = &body[i + 1..];

                // Check for `\<newline>` (line continuation) — the backslash
                // is the last character on the line (nothing follows).
                if after_backslash.is_empty() {
                    // Line continuation: `\` at end of line.  Force Owned so
                    // the continuation accumulator starts with the prefix seen
                    // so far.  Do not push anything — the newline and leading
                    // whitespace on the next line are stripped by the caller.
                    let prefix = owned.unwrap_or_else(|| body[..borrow_end].to_owned());
                    return Ok(DoubleQuotedLine::Incomplete {
                        value: DoubleQuotedValue::Owned(prefix),
                        line_continuation: true,
                    });
                }

                let consumed = decode_and_push_escape(
                    after_backslash,
                    escape_pos,
                    &mut owned,
                    &body[..borrow_end],
                )?;
                i += 1 + consumed; // skip `\` + escape body
            }
            _ => {
                if let Some(buf) = owned.as_mut() {
                    buf.push(ch);
                    if buf.len() > 1_048_576 {
                        return Err(Error {
                            pos: start_pos,
                            message: "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                        });
                    }
                } else {
                    borrow_end = i + ch.len_utf8();
                }
                i += ch.len_utf8();
            }
        }
    }

    // End of line without closing `"`.
    // Trim trailing whitespace before fold.
    let value = owned.map_or_else(
        || {
            let s = body
                .get(..borrow_end)
                .unwrap_or("")
                .trim_end_matches([' ', '\t']);
            DoubleQuotedValue::Borrowed(s)
        },
        |buf| DoubleQuotedValue::Owned(buf.trim_end_matches([' ', '\t']).to_owned()),
    );

    Ok(DoubleQuotedLine::Incomplete {
        value,
        line_continuation: false,
    })
}

/// Ensure `owned` is populated (allocating from `prefix` if needed), and
/// return a mutable reference to it.
fn ensure_owned<'s>(owned: &'s mut Option<String>, prefix: &str) -> &'s mut String {
    owned.get_or_insert_with(|| prefix.to_owned())
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
