// SPDX-License-Identifier: MIT

use std::borrow::Cow;

use memchr::{memchr, memchr2};

use crate::chars::{decode_escape, find_non_nb_json, is_c_printable, non_printable_error_message};
use crate::error::Error;
use crate::limits::MAX_SCALAR_LEN;
use crate::pos::{Pos, Span};

use super::{Lexer, is_doc_marker_line};
use crate::lines::pos_after_line;

impl<'input> Lexer<'input> {
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
    #[expect(
        clippy::too_many_lines,
        reason = "length cap checks added to all single-quoted paths increase line count beyond 100"
    )]
    pub fn try_consume_single_quoted(
        &mut self,
        parent_indent: usize,
    ) -> Result<Option<(Cow<'input, str>, Span)>, Error> {
        let Some(first_line) = self.buf.peek_next() else {
            return Ok(None);
        };
        let content = first_line.content.trim_start_matches([' ', '\t']);
        if !content.starts_with('\'') {
            return Ok(None);
        }

        let leading_bytes = first_line.content.len() - content.len();
        let leading_chars = crate::pos::column_at(first_line.content, leading_bytes);
        let open_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
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
            if value.quoted_len > MAX_SCALAR_LEN {
                return Err(Error::syntax(
                    open_pos,
                    "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                ));
            }
            // Validate nb-json on the literal source bytes (single-quoted scalars
            // allow all non-C0 characters per YAML §5.1 JSON-compatibility clause).
            if !self.input_all_printable {
                let body_slice = body_start.get(..value.quoted_len).unwrap_or_default();
                if let Some((bad_i, bad_ch)) = find_non_nb_json(body_slice.as_bytes()) {
                    let bad_pos = crate::pos::advance_within_line(
                        open_pos.advance('\''),
                        &body_slice[..bad_i],
                    );
                    return Err(Error::invalid_character(
                        bad_pos,
                        non_printable_error_message(bad_ch, "single-quoted scalar"),
                    ));
                }
            }
            // Span: from open `'` through closing `'`.
            let end_pos = crate::pos::advance_within_line(
                open_pos.advance('\''),
                &body_start[..value.quoted_len],
            )
            .advance('\'');
            return Ok(Some((
                value.into_cow(body_start),
                Span::from_pos(open_pos, end_pos),
            )));
        }

        // Multi-line: YAML §7.3.3 trims trailing whitespace from each line.
        // Validate nb-json on the first line's source bytes before building owned.
        if !self.input_all_printable {
            let body_slice = body_start.get(..value.quoted_len).unwrap_or_default();
            if let Some((bad_i, bad_ch)) = find_non_nb_json(body_slice.as_bytes()) {
                let bad_pos =
                    crate::pos::advance_within_line(open_pos.advance('\''), &body_slice[..bad_i]);
                return Err(Error::invalid_character(
                    bad_pos,
                    non_printable_error_message(bad_ch, "single-quoted scalar"),
                ));
            }
        }
        let mut owned = value.as_owned_string(body_start);
        owned.truncate(owned.trim_end_matches([' ', '\t']).len());

        loop {
            let Some(next) = self.buf.peek_next() else {
                // EOF without closing quote — point to the opening `'`, not EOF.
                return Err(Error::syntax(
                    open_pos,
                    "unterminated single-quoted scalar".to_owned(),
                ));
            };

            // Document markers at column 0 terminate the document even inside
            // quoted scalars (YAML spec §6.5 / test suite RXY3).
            if is_doc_marker_line(next.content) {
                return Err(Error::syntax(
                    next.pos,
                    "document marker '...' or '---' is not allowed inside a quoted scalar"
                        .to_owned(),
                ));
            }

            // SAFETY: peek succeeded in the let-else above; LineBuffer invariant.
            let Some(consumed) = self.buf.consume_next() else {
                unreachable!("peek returned Some but consume returned None")
            };
            let line_start_pos = consumed.pos;
            self.current_pos = pos_after_line(&consumed);
            let line_content = consumed.content;

            // Determine how this line participates in folding.
            // For blank lines (all whitespace), bypass the indent check: per spec
            // l-empty(n,c) lines are allowed regardless of indentation.
            let trimmed_blank_check = line_content.trim_matches([' ', '\t']);
            let trimmed = if trimmed_blank_check.is_empty() {
                // Blank line: trim all leading whitespace.
                trimmed_blank_check
            } else {
                // s-indent(n) enforcement — §6.3 production [66].
                // Non-blank continuation lines must have at least `parent_indent`
                // leading spaces when in block context; blank lines are exempt.
                if parent_indent > 0 {
                    let found = consumed.indent;
                    if found < parent_indent {
                        return Err(Error::syntax(
                            line_start_pos,
                            format!(
                                "continuation line does not have enough indentation \
                                 (expected at least {parent_indent} spaces, found {found})"
                            ),
                        ));
                    }
                }
                // Strip indent (n spaces) + separation (remaining whitespace including tabs).
                line_content.trim_start_matches([' ', '\t'])
            };

            if trimmed.is_empty() {
                // Blank continuation line: counts as a literal newline.
                owned.push('\n');
                if owned.len() > MAX_SCALAR_LEN {
                    return Err(Error::syntax(
                        self.current_pos,
                        "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                    ));
                }
                continue;
            }

            // Fold: if the preceding content ends with a newline (from blank
            // lines), no extra space; otherwise add a fold space.
            if !owned.ends_with('\n') {
                owned.push(' ');
            }

            let (cont_value, cont_closed) = scan_single_quoted_line(trimmed);

            // Validate nb-json on the source bytes of this continuation line.
            if !self.input_all_printable {
                let cont_slice = trimmed.get(..cont_value.quoted_len).unwrap_or_default();
                if let Some((bad_i, bad_ch)) = find_non_nb_json(cont_slice.as_bytes()) {
                    let leading_len = line_content.len() - trimmed.len();
                    let bad_pos = crate::pos::advance_within_line(
                        line_start_pos,
                        &line_content[..leading_len + bad_i],
                    );
                    return Err(Error::invalid_character(
                        bad_pos,
                        non_printable_error_message(bad_ch, "single-quoted scalar"),
                    ));
                }
            }

            if cont_closed {
                if cont_value.has_escape {
                    owned.push_str(&unescape_single_quoted(trimmed, cont_value.quoted_len));
                } else {
                    owned.push_str(&trimmed[..cont_value.quoted_len]);
                }
                if owned.len() > MAX_SCALAR_LEN {
                    return Err(Error::syntax(
                        self.current_pos,
                        "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                    ));
                }
                // Compute position right after the closing `'` by advancing from
                // the line start over leading whitespace + content + closing `'`.
                let leading_len = line_content.len() - trimmed.len();
                let close_pos = crate::pos::advance_within_line(
                    line_start_pos,
                    &line_content[..leading_len + cont_value.quoted_len],
                )
                .advance('\'');
                // If there is content after the closing `'`, store it so the
                // flow parser can continue parsing `,`, `]`, `}`, etc.
                let tail = trimmed.get(cont_value.quoted_len + 1..).unwrap_or("");
                if !tail.is_empty() {
                    self.pending_multiline_tail = Some((tail, close_pos));
                }
                let end_pos = self.current_pos;
                return Ok(Some((Cow::Owned(owned), Span::from_pos(open_pos, end_pos))));
            }
            // Non-closing continuation: trim trailing whitespace per YAML §7.3.3.
            let line_str = if cont_value.has_escape {
                unescape_single_quoted(trimmed, cont_value.quoted_len)
            } else {
                trimmed[..cont_value.quoted_len].to_owned()
            };
            owned.push_str(line_str.trim_end_matches([' ', '\t']));
            if owned.len() > MAX_SCALAR_LEN {
                return Err(Error::syntax(
                    self.current_pos,
                    "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                ));
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
        let leading_chars = crate::pos::column_at(first_line.content, leading_bytes);
        let open_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
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
        let (value, span) = match scan_double_quoted_line(
            body_start,
            open_pos.advance('"'),
            self.input_all_printable,
        )? {
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
                    Span::from_pos(open_pos, end_pos),
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
                (Cow::Owned(owned), Span::from_pos(open_pos, end_pos))
            }
        };
        Ok(Some((value, span)))
    }

    /// Collect continuation lines for a multi-line double-quoted scalar.
    ///
    /// `owned` is the accumulated content so far (from the first line).
    /// `line_continuation` indicates whether the first line ended with `\<LF>`
    /// (which suppresses the fold space).
    pub(super) fn collect_double_quoted_continuations(
        &mut self,
        owned: &mut String,
        mut line_continuation: bool,
        open_pos: Pos,
        block_context_indent: Option<usize>,
    ) -> Result<(), Error> {
        let mut pending_blanks: usize = 0;

        loop {
            let Some(next) = self.buf.peek_next() else {
                return Err(Error::syntax(
                    self.current_pos,
                    "unterminated double-quoted scalar".to_owned(),
                ));
            };

            // Document markers at column 0 terminate the document even inside
            // quoted scalars (YAML spec §6.5 / test suite 5TRB).
            if is_doc_marker_line(next.content) {
                return Err(Error::syntax(
                    next.pos,
                    "document marker '...' or '---' is not allowed inside a quoted scalar"
                        .to_owned(),
                ));
            }

            // Determine if this line is blank (all whitespace).
            let is_blank = next.content.trim_matches([' ', '\t']).is_empty();

            // s-indent(n+1) enforcement — YAML 1.2.2 §6.3 / production [66].
            // In block context (Some(n)), non-blank continuation lines must have
            // at least n+1 leading SPACE characters (strictly more than the
            // enclosing block's indent n).  Tabs do not count toward indent.
            // Blank lines bypass the check: l-empty (§6.5) allows any indentation.
            // Rationale: without this check a quoted scalar can "escape" its block
            // by dedenting, silently absorbing lines that belong to a sibling node.
            if let Some(n) = block_context_indent {
                if !is_blank {
                    let found = next.indent;
                    if found <= n {
                        return Err(Error::syntax(
                            next.pos,
                            format!(
                                "continuation line does not have enough indentation \
                                 (expected at least {} spaces, found {found})",
                                n + 1
                            ),
                        ));
                    }
                }
            }

            let trimmed = next.content.trim_start_matches([' ', '\t']);

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
                    owned.extend(std::iter::repeat_n('\n', pending_blanks));
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
            match scan_double_quoted_line(trimmed, line_start_pos, self.input_all_printable)? {
                DoubleQuotedLine::Closed {
                    value,
                    close_pos,
                    tail,
                } => {
                    value.push_into(owned);
                    if owned.len() > MAX_SCALAR_LEN {
                        return Err(Error::syntax(
                            line_start_pos,
                            "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                        ));
                    }
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
                    if owned.len() > MAX_SCALAR_LEN {
                        return Err(Error::syntax(
                            line_start_pos,
                            "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                        ));
                    }
                    line_continuation = next_cont;
                    // continue loop
                    let _ = open_pos;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Single-quoted scalar scanning helpers
// ---------------------------------------------------------------------------

/// Result of scanning one line of a single-quoted scalar body.
pub(super) struct SingleQuotedScan {
    /// Byte length of the accepted content inside the line (after `''` unescape).
    /// For borrowed case this is a slice length; for owned it's the source chars
    /// counted up to and including the `''` / closing `'`.
    ///
    /// This is the length of the *source* content that was consumed, used to
    /// compute the span end position.
    pub(super) quoted_len: usize,
    /// Whether the scanning found the closing `'` on this line.
    pub(super) has_escape: bool,
}

impl SingleQuotedScan {
    /// Convert to a `Cow` borrowing from `body` (the line slice starting after
    /// the opening `'`).
    ///
    /// `body` is the full line content after the opening quote.  If the scalar
    /// closed on this line, the slice up to the closing `'` is used.
    pub(super) fn into_cow(self, body: &str) -> Cow<'_, str> {
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
    pub(super) fn as_owned_string(&self, body: &str) -> String {
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
pub(super) fn scan_single_quoted_line(body: &str) -> (SingleQuotedScan, bool) {
    let mut i = 0;
    let bytes = body.as_bytes();
    let mut has_escape = false;

    while let Some(rel) = memchr(b'\'', bytes.get(i..).unwrap_or_default()) {
        let quote_pos = i + rel;

        // Everything before quote_pos is plain content — skip it.
        // But we must ensure we haven't landed mid-multibyte. Since `'` is
        // ASCII (0x27), it can never be a continuation byte (0x80–0xBF), so
        // any byte equal to 0x27 is the start of a new character.

        match bytes.get(quote_pos + 1) {
            Some(&b'\'') => {
                // `''` escape: consume both quotes and continue.
                has_escape = true;
                i = quote_pos + 2;
            }
            _ => {
                // Closing `'`.
                return (
                    SingleQuotedScan {
                        quoted_len: quote_pos,
                        has_escape,
                    },
                    true,
                );
            }
        }
    }

    // No more `'` in the remaining bytes — end of line without closing quote.
    (
        SingleQuotedScan {
            quoted_len: bytes.len(),
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
pub(super) enum DoubleQuotedLine<'a> {
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
pub(super) enum DoubleQuotedValue<'a> {
    /// No escapes and no transformation — can borrow.
    Borrowed(&'a str),
    /// Escapes or other transformation occurred — must own.
    Owned(String),
}

impl<'a> DoubleQuotedValue<'a> {
    pub(super) fn into_cow(self, _body: &'a str) -> Cow<'a, str> {
        match self {
            Self::Borrowed(s) => Cow::Borrowed(s),
            Self::Owned(s) => Cow::Owned(s),
        }
    }

    /// Push this value into `out`.
    pub(super) fn push_into(self, out: &mut String) {
        match self {
            Self::Borrowed(s) => out.push_str(s),
            Self::Owned(s) => out.push_str(&s),
        }
    }

    pub(super) fn into_string(self) -> String {
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
        return Err(Error::syntax(
            escape_pos,
            format!(
                "invalid escape sequence '\\{}'",
                after_backslash
                    .chars()
                    .next()
                    .map_or_else(|| "EOF".to_owned(), |c| c.to_string())
            ),
        ));
    };

    // Security: for hex escapes (\x, \u, \U), the decoded character must
    // be a YAML c-printable character.  Named escapes (\0, \a, \b, …)
    // produce well-known control chars and are exempt from this check.
    let escape_prefix = after_backslash.chars().next().unwrap_or('\0');
    if matches!(escape_prefix, 'x' | 'u' | 'U') && !is_c_printable(decoded_ch) {
        return Err(Error::invalid_character(
            escape_pos,
            format!(
                "escape produces non-printable character U+{:04X}",
                u32::from(decoded_ch)
            ),
        ));
    }

    // Security: reject bidi override characters produced by numeric
    // escapes (\u and \U can reach the bidi range; \x max is U+00FF).
    if is_bidi_control(decoded_ch) {
        return Err(Error::syntax(
            escape_pos,
            format!(
                "escape produces bidirectional control character U+{:04X}",
                u32::from(decoded_ch)
            ),
        ));
    }

    let buf = ensure_owned(owned, prefix);
    buf.push(decoded_ch);

    // Maximum scalar length cap.
    if buf.len() > MAX_SCALAR_LEN {
        return Err(Error::syntax(
            escape_pos,
            "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
        ));
    }

    Ok(consumed)
}

/// `start_pos` is the position of the first character of `body` (i.e. the byte
/// after the opening `"`), used only for error reporting.
///
/// `skip_char_validation` suppresses the two `find_non_nb_json` passes when the
/// caller's pre-scan has already confirmed the input contains no non-printable bytes.
#[expect(
    clippy::too_many_lines,
    reason = "length cap checks added to borrow paths increase line count beyond 100"
)]
pub(super) fn scan_double_quoted_line(
    body: &str,
    start_pos: Pos,
    skip_char_validation: bool,
) -> Result<DoubleQuotedLine<'_>, Error> {
    let bytes = body.as_bytes();
    let mut i = 0;
    // We delay allocation until the first escape or discovery of multi-line.
    let mut owned: Option<String> = None;
    // Byte length of the borrow-safe prefix (updated only while owned is None).
    let mut borrow_end: usize = 0;
    // Length of `owned` after the last non-literal-whitespace source character.
    // Used to trim only literal trailing spaces/tabs, not escape-decoded chars.
    // `None` means no escape sequences have been seen (borrow path still valid).
    let mut owned_non_ws_len: Option<usize> = None;

    while let Some(rel) = memchr2(b'"', b'\\', bytes.get(i..).unwrap_or_default()) {
        let hit = i + rel;

        // Accumulate the plain span [i..hit] that memchr skipped over.
        // For the borrow case we simply extend borrow_end; for owned we push.
        {
            let span = body.get(i..hit).unwrap_or_default();
            // Validate nb-json on the literal span (quoted scalars allow all
            // non-C0 characters per YAML §5.1 JSON-compatibility clause).
            if !skip_char_validation {
                if let Some((bad_i, bad_ch)) = find_non_nb_json(span.as_bytes()) {
                    let bad_pos = crate::pos::advance_within_line(
                        start_pos,
                        body.get(..i + bad_i).unwrap_or_default(),
                    );
                    return Err(Error::invalid_character(
                        bad_pos,
                        non_printable_error_message(bad_ch, "double-quoted scalar"),
                    ));
                }
            }
            if let Some(buf) = owned.as_mut() {
                buf.push_str(span);
                if buf.len() > MAX_SCALAR_LEN {
                    return Err(Error::syntax(
                        start_pos,
                        "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                    ));
                }
                // owned_non_ws_len is updated after each escape decode; see below.
                // If the next hit is `"` (return) or `\` (escape decode updates it),
                // we do not need to update the checkpoint here.
            } else {
                borrow_end = hit;
                if borrow_end > MAX_SCALAR_LEN {
                    return Err(Error::syntax(
                        start_pos,
                        "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                    ));
                }
            }
        }

        if bytes.get(hit) == Some(&b'"') {
            // Closing quote.
            let content_end_pos =
                crate::pos::advance_within_line(start_pos, body.get(..hit).unwrap_or_default());
            let close_pos = content_end_pos.advance('"');
            let value = owned.map_or_else(
                || DoubleQuotedValue::Borrowed(body.get(..hit).unwrap_or_default()),
                DoubleQuotedValue::Owned,
            );
            let tail = body.get(hit + 1..).unwrap_or_default();
            return Ok(DoubleQuotedLine::Closed {
                value,
                close_pos,
                tail,
            });
        }
        // b'\\' — escape sequence.
        {
            let escape_pos =
                crate::pos::advance_within_line(start_pos, body.get(..hit).unwrap_or_default());
            let after_backslash = body.get(hit + 1..).unwrap_or_default();

            if after_backslash.is_empty() {
                // `\` at end of line — line continuation.
                // Per YAML 1.2 §7.3.1, the `\` just before EOL is a line-
                // continuation escape that suppresses the following line break and
                // strips leading whitespace from the next line.  Content preceding
                // the `\` (including literal tabs and spaces) is NOT trimmed — the
                // author explicitly wrote `\<newline>` to signal continuation.
                let prefix =
                    owned.unwrap_or_else(|| body.get(..borrow_end).unwrap_or_default().to_owned());
                return Ok(DoubleQuotedLine::Incomplete {
                    value: DoubleQuotedValue::Owned(prefix),
                    line_continuation: true,
                });
            }

            let consumed = decode_and_push_escape(
                after_backslash,
                escape_pos,
                &mut owned,
                body.get(..borrow_end).unwrap_or_default(),
            )?;
            // After decoding an escape, `owned` has new non-literal-whitespace
            // content (the decoded character is not a trailing literal space/tab).
            owned_non_ws_len = Some(owned.as_ref().map_or(0, String::len));
            i = hit + 1 + consumed; // skip `\` + escape body
        }
    }

    // No more `"` or `\` — consume the rest of the line as plain content.
    let rest = body.get(i..).unwrap_or_default();
    // Validate nb-json on the trailing literal span.
    if !skip_char_validation {
        if let Some((bad_i, bad_ch)) = find_non_nb_json(rest.as_bytes()) {
            let bad_pos = crate::pos::advance_within_line(
                start_pos,
                body.get(..i + bad_i).unwrap_or_default(),
            );
            return Err(Error::invalid_character(
                bad_pos,
                non_printable_error_message(bad_ch, "double-quoted scalar"),
            ));
        }
    }
    if let Some(buf) = owned.as_mut() {
        if !rest.is_empty() {
            buf.push_str(rest);
            if buf.len() > MAX_SCALAR_LEN {
                return Err(Error::syntax(
                    start_pos,
                    "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                ));
            }
            // Update non-ws checkpoint for the remaining literal span.
            if let Some(last_non_ws) = rest.rfind(|c: char| c != ' ' && c != '\t') {
                owned_non_ws_len = Some(
                    buf.len() - rest.len()
                        + last_non_ws
                        + rest[last_non_ws..].chars().next().map_or(0, char::len_utf8),
                );
            }
            // else: rest is all-whitespace, checkpoint unchanged.
        }
    } else {
        borrow_end = body.len();
        if borrow_end > MAX_SCALAR_LEN {
            return Err(Error::syntax(
                start_pos,
                "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
            ));
        }
    }

    // End of line without closing `"` — trim trailing LITERAL whitespace before fold.
    // For the borrow path (no escape sequences), trim the source slice directly.
    // For the owned path, truncate to the non-ws checkpoint to avoid trimming
    // characters that were produced by escape sequences (e.g. `\t` → tab).
    let value = owned.map_or_else(
        || {
            let s = body
                .get(..borrow_end)
                .unwrap_or_default()
                .trim_end_matches([' ', '\t']);
            DoubleQuotedValue::Borrowed(s)
        },
        |buf| {
            let trim_to = owned_non_ws_len.unwrap_or(0);
            DoubleQuotedValue::Owned(buf[..trim_to].to_owned())
        },
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

#[cfg(test)]
#[expect(clippy::expect_used, reason = "test code")]
mod tests {
    use std::borrow::Cow;

    use rstest::rstest;

    use super::{
        DoubleQuotedLine, DoubleQuotedValue, SingleQuotedScan, scan_double_quoted_line,
        scan_single_quoted_line,
    };
    use crate::error::Error;
    use crate::pos::{Pos, Span};

    fn make_lexer(input: &str) -> super::super::Lexer<'_> {
        super::super::Lexer::new(input)
    }

    fn sq(input: &str) -> (Cow<'_, str>, Span) {
        make_lexer(input)
            .try_consume_single_quoted(0)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"))
            .unwrap_or_else(|| unreachable!("expected Some, got None"))
    }

    fn sq_err(input: &str) -> Error {
        match make_lexer(input).try_consume_single_quoted(0) {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    fn sq_none(input: &str) {
        let result = make_lexer(input)
            .try_consume_single_quoted(0)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"));
        assert!(result.is_none(), "expected None for input {input:?}");
    }

    fn dq(input: &str) -> (Cow<'_, str>, Span) {
        make_lexer(input)
            .try_consume_double_quoted(None)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"))
            .unwrap_or_else(|| unreachable!("expected Some, got None"))
    }

    fn dq_err(input: &str) -> Error {
        match make_lexer(input).try_consume_double_quoted(None) {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    fn dq_none(input: &str) {
        let result = make_lexer(input)
            .try_consume_double_quoted(None)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"));
        assert!(result.is_none(), "expected None for input {input:?}");
    }

    const START: Pos = Pos {
        byte_offset: 0,
        line: 1,
        column: 0,
    };

    // ── scan_single_quoted_line ──────────────────────────────────────────────

    #[test]
    fn sq_empty_body_returns_not_closed_zero_len_no_escape() {
        // Empty body means no closing quote was found on this line.
        // The caller is responsible for finding the closing quote.
        let (scan, closed) = scan_single_quoted_line("");
        assert_eq!(scan.quoted_len, 0);
        assert!(!scan.has_escape);
        assert!(!closed);
    }

    #[test]
    fn sq_just_closing_quote_returns_closed_zero_len() {
        // "''" body is "'", the closing quote only.
        let (scan, closed) = scan_single_quoted_line("'");
        assert_eq!(scan.quoted_len, 0);
        assert!(!scan.has_escape);
        assert!(closed);
    }

    #[test]
    fn sq_plain_ascii_closes_at_single_quote() {
        let (scan, closed) = scan_single_quoted_line("hello'");
        assert_eq!(scan.quoted_len, 5);
        assert!(!scan.has_escape);
        assert!(closed);
    }

    #[test]
    fn sq_double_quote_escape_at_start() {
        // "''rest'" — escape at 0-1, then "rest", then closing quote at 6.
        // quoted_len is 6 (bytes 0-5 consumed: ''rest), closing ' is at 6.
        let (scan, closed) = scan_single_quoted_line("''rest'");
        assert_eq!(scan.quoted_len, 6);
        assert!(scan.has_escape);
        assert!(closed);
    }

    #[test]
    fn sq_double_quote_escape_at_end_no_close() {
        // `content''` — the `''` is an escape, no closing quote follows.
        let (scan, closed) = scan_single_quoted_line("content''");
        assert_eq!(scan.quoted_len, 9);
        assert!(scan.has_escape);
        assert!(!closed);
    }

    #[test]
    fn sq_no_quote_returns_full_len_not_closed() {
        let (scan, closed) = scan_single_quoted_line("no quote here");
        assert_eq!(scan.quoted_len, 13);
        assert!(!scan.has_escape);
        assert!(!closed);
    }

    #[test]
    fn sq_multibyte_no_quote_returns_byte_len() {
        // "café" is 5 bytes in UTF-8.
        let (scan, closed) = scan_single_quoted_line("café");
        assert_eq!(scan.quoted_len, "café".len()); // 5
        assert!(!scan.has_escape);
        assert!(!closed);
    }

    #[test]
    fn sq_multibyte_before_closing_quote() {
        let (scan, closed) = scan_single_quoted_line("café'");
        assert_eq!(scan.quoted_len, "café".len()); // 5
        assert!(!scan.has_escape);
        assert!(closed);
    }

    #[test]
    fn sq_double_quote_escape_adjacent_to_multibyte() {
        // "café''latte'" — escape between non-ASCII and ASCII
        let body = "café''latte'";
        let (scan, closed) = scan_single_quoted_line(body);
        assert_eq!(scan.quoted_len, "café''latte".len()); // 12
        assert!(scan.has_escape);
        assert!(closed);
    }

    #[test]
    fn sq_all_double_quote_escapes_no_close() {
        // "''''" = two `''` escapes with no closing quote after them.
        let (scan, closed) = scan_single_quoted_line("''''");
        assert_eq!(scan.quoted_len, 4);
        assert!(scan.has_escape);
        assert!(!closed);
    }

    #[test]
    fn sq_all_double_quote_escapes_then_close() {
        // "'''''" = two `''` escapes followed by a closing `'` (5 quotes total).
        // 0-1: escape, i=2. 2-3: escape, i=4. 4: closing quote.
        let (scan, closed) = scan_single_quoted_line("'''''");
        assert_eq!(scan.quoted_len, 4);
        assert!(scan.has_escape);
        assert!(closed);
    }

    #[test]
    fn sq_three_quotes_escape_then_close() {
        // "text'''" — first two `'` are escape, third is close
        let (scan, closed) = scan_single_quoted_line("text'''");
        assert_eq!(scan.quoted_len, 6); // "text''"
        assert!(scan.has_escape);
        assert!(closed);
    }

    // ── SingleQuotedScan::into_cow (tests unescape_single_quoted) ───────────

    #[test]
    fn us_no_escape_borrows_directly() {
        let result = SingleQuotedScan {
            quoted_len: 5,
            has_escape: false,
        }
        .into_cow("hello'");
        assert!(matches!(result, Cow::Borrowed("hello")));
    }

    #[test]
    fn us_double_quote_escape_produces_single_quote() {
        // Body "it''s'" — quoted_len 5 covers "it''s" (the escape + surrounding content).
        let result = SingleQuotedScan {
            quoted_len: 5,
            has_escape: true,
        }
        .into_cow("it''s'");
        assert_eq!(result, "it's");
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn us_escape_adjacent_to_multibyte() {
        let body = "café''latte'";
        let result = SingleQuotedScan {
            quoted_len: "café''latte".len(),
            has_escape: true,
        }
        .into_cow(body);
        assert_eq!(result, "café'latte");
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn us_all_double_quote_escapes_produces_two_quotes() {
        let result = SingleQuotedScan {
            quoted_len: 4,
            has_escape: true,
        }
        .into_cow("''''");
        assert_eq!(result, "''");
        assert!(matches!(result, Cow::Owned(_)));
    }

    // ── scan_double_quoted_line ──────────────────────────────────────────────

    fn dq_ok(input: &str) -> DoubleQuotedLine<'_> {
        // Pass skip_char_validation=false in tests so validation is always active.
        scan_double_quoted_line(input, START, false)
            .unwrap_or_else(|e| unreachable!("unexpected error: {}", e.message))
    }

    #[rstest]
    #[case::empty_body_borrows_empty("\"", "", "")]
    #[case::plain_ascii_borrows_and_closes("hello\"", "hello", "")]
    #[case::multibyte_no_escape_borrows("café\"", "café", "")]
    #[case::tail_after_closing_quote_is_captured("hello\" world", "hello", " world")]
    fn dq_line_closed_borrowed_cases(
        #[case] input: &str,
        #[case] expected_val: &str,
        #[case] expected_tail: &str,
    ) {
        match dq_ok(input) {
            DoubleQuotedLine::Closed { value, tail, .. } => {
                assert!(
                    matches!(value, DoubleQuotedValue::Borrowed(_)),
                    "expected Borrowed for {input:?}"
                );
                assert_eq!(value.into_string(), expected_val);
                assert_eq!(tail, expected_tail);
            }
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed for {input:?}"),
        }
    }

    #[rstest]
    #[case::newline_escape_forces_owned("a\\nb\"", "a\nb")]
    #[case::unicode_escape_u4("\\u00E9\"", "é")]
    #[case::unicode_escape_u8_supplementary("\\U0001F600\"", "😀")]
    #[case::hex_escape_xff("\\xFF\"", "\u{FF}")]
    #[case::multibyte_then_escape_accumulates_correctly("café\\n\"", "café\n")]
    #[case::escape_then_multibyte_accumulates_correctly("\\ncafé\"", "\ncafé")]
    #[case::null_byte_escape_is_allowed("\\0\"", "\0")]
    fn dq_line_closed_escaped_value_cases(#[case] input: &str, #[case] expected: &str) {
        match dq_ok(input) {
            DoubleQuotedLine::Closed { value, .. } => assert_eq!(value.into_string(), expected),
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed for {input:?}"),
        }
    }

    #[rstest]
    #[case::backslash_at_end_is_line_continuation("text\\", "text", true)]
    #[case::no_delimiter_trims_trailing_whitespace("hello   ", "hello", false)]
    fn dq_line_incomplete_cases(
        #[case] input: &str,
        #[case] expected_val: &str,
        #[case] expected_continuation: bool,
    ) {
        match dq_ok(input) {
            DoubleQuotedLine::Incomplete {
                value,
                line_continuation,
            } => {
                assert_eq!(value.into_string(), expected_val);
                assert_eq!(line_continuation, expected_continuation);
            }
            DoubleQuotedLine::Closed { .. } => unreachable!("expected Incomplete for {input:?}"),
        }
    }

    #[test]
    fn dq_bidi_escape_is_rejected() {
        match scan_double_quoted_line("\\u202A\"", START, false) {
            Err(e) => assert!(
                e.message.contains("bidirectional"),
                "message: {}",
                e.message
            ),
            Ok(_) => unreachable!("expected Err for bidi escape"),
        }
    }

    #[test]
    fn dq_non_printable_hex_escape_is_rejected() {
        match scan_double_quoted_line("\\x01\"", START, false) {
            Err(e) => assert!(
                e.message.contains("non-printable"),
                "message: {}",
                e.message
            ),
            Ok(_) => unreachable!("expected Err for non-printable escape"),
        }
    }

    #[test]
    fn dq_unknown_escape_is_rejected() {
        match scan_double_quoted_line("\\q\"", START, false) {
            Err(e) => assert!(
                e.message.contains("invalid escape"),
                "message: {}",
                e.message
            ),
            Ok(_) => unreachable!("expected Err for unknown escape"),
        }
    }

    // =======================================================================
    // Group H — try_consume_single_quoted (Task 7)
    // =======================================================================

    // -----------------------------------------------------------------------
    // Group H-A — happy path
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::simple_word("'hello'", "hello")]
    #[case::empty_string("''", "")]
    #[case::escaped_quote_in_middle("'it''s'", "it's")]
    #[case::escaped_quote_at_start("'''leading'", "'leading")]
    #[case::escaped_quote_at_end("'trailing'''", "trailing'")]
    #[case::multiple_escaped_quotes("'a''b''c'", "a'b'c")]
    #[case::multi_word("'hello world'", "hello world")]
    #[case::multibyte_utf8("'日本語'", "日本語")]
    #[case::backslash_not_special("'foo\\nbar'", "foo\\nbar")]
    #[case::double_quote_inside("'say \"hello\"'", "say \"hello\"")]
    fn single_quoted_happy_path_cases(#[case] input: &str, #[case] expected: &str) {
        let (val, _) = sq(input);
        assert_eq!(val, expected);
    }

    // -----------------------------------------------------------------------
    // Group H-B — Cow allocation
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::single_line_no_escape_is_borrowed("'hello'", true)]
    #[case::with_escaped_quote_is_owned("'it''s'", false)]
    #[case::multiline_is_owned("'foo\n  bar'", false)]
    fn single_quoted_cow_allocation_cases(#[case] input: &str, #[case] expect_borrowed: bool) {
        let (val, _) = sq(input);
        if expect_borrowed {
            assert!(matches!(val, Cow::Borrowed(_)), "must be Borrowed");
        } else {
            assert!(matches!(val, Cow::Owned(_)), "must be Owned");
        }
    }

    // -----------------------------------------------------------------------
    // Group H-C — multi-line folding
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::single_break_folds_to_space("'foo\nbar'", "foo bar")]
    #[case::leading_whitespace_stripped_on_continuation("'foo\n  bar'", "foo bar")]
    #[case::blank_line_produces_newline("'foo\n\nbar'", "foo\nbar")]
    #[case::two_blank_lines_produce_two_newlines("'foo\n\n\nbar'", "foo\n\nbar")]
    fn single_quoted_multiline_folding_cases(#[case] input: &str, #[case] expected: &str) {
        let (val, _) = sq(input);
        assert_eq!(val, expected);
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

    // -----------------------------------------------------------------------
    // Group I-E — happy path
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::simple_word("\"hello\"", "hello")]
    #[case::empty_string("\"\"", "")]
    #[case::escape_newline("\"foo\\nbar\"", "foo\nbar")]
    #[case::escape_tab("\"foo\\tbar\"", "foo\tbar")]
    #[case::escape_backslash("\"foo\\\\bar\"", "foo\\bar")]
    #[case::escape_double_quote("\"say \\\"hi\\\"\"", "say \"hi\"")]
    #[case::escape_slash("\"foo\\/bar\"", "foo/bar")]
    #[case::escape_space("\"foo\\ bar\"", "foo bar")]
    fn double_quoted_happy_path_cases(#[case] input: &str, #[case] expected: &str) {
        let (val, _) = dq(input);
        assert_eq!(val, expected);
    }

    #[test]
    fn double_quoted_escape_null() {
        let (val, _) = dq("\"\\0\"");
        assert_eq!(val.as_bytes(), b"\x00");
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

    #[rstest]
    #[case::hex_escape_2digit_correct("\"\\x41\"", "A")]
    #[case::hex_escape_2digit_lowercase("\"\\x61\"", "a")]
    #[case::unicode_4digit_correct("\"\\u0041\"", "A")]
    #[case::unicode_4digit_non_ascii("\"\\u00E9\"", "é")]
    #[case::unicode_8digit_basic("\"\\U00000041\"", "A")]
    #[case::unicode_8digit_supplementary("\"\\U0001F600\"", "😀")]
    fn double_quoted_hex_unicode_success_cases(#[case] input: &str, #[case] expected: &str) {
        let (val, _) = dq(input);
        assert_eq!(val, expected);
    }

    #[rstest]
    #[case::hex_invalid_digits("\"\\xGG\"")]
    #[case::hex_truncated("\"\\xA\"")]
    #[case::unicode_4digit_truncated("\"\\u004\"")]
    #[case::unicode_surrogate_low("\"\\uD800\"")]
    #[case::unicode_surrogate_high("\"\\uDFFF\"")]
    #[case::unicode_8digit_out_of_range("\"\\U00110000\"")]
    #[case::unknown_escape_code("\"\\q\"")]
    fn double_quoted_hex_unicode_error_cases(#[case] input: &str) {
        let _ = dq_err(input);
    }

    // -----------------------------------------------------------------------
    // Group I-G — line continuation and folding
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::backslash_newline_suppresses_break("\"foo\\\nbar\"", "foobar")]
    #[case::backslash_newline_strips_leading_whitespace_on_next_line("\"foo\\\n   bar\"", "foobar")]
    #[case::real_newline_folds_to_space("\"foo\nbar\"", "foo bar")]
    #[case::real_newline_with_leading_whitespace_on_continuation("\"foo\n  bar\"", "foo bar")]
    #[case::blank_line_in_multiline_produces_newline("\"foo\n\nbar\"", "foo\nbar")]
    #[case::two_blank_lines_produce_two_newlines("\"foo\n\n\nbar\"", "foo\n\nbar")]
    fn double_quoted_line_folding_cases(#[case] input: &str, #[case] expected: &str) {
        let (val, _) = dq(input);
        assert_eq!(val, expected);
    }

    // -----------------------------------------------------------------------
    // Group I-H — Cow allocation
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::single_line_no_escape_is_borrowed("\"hello\"", true)]
    #[case::with_escape_is_owned("\"\\n\"", false)]
    #[case::multiline_is_owned("\"foo\nbar\"", false)]
    fn double_quoted_cow_allocation_cases(#[case] input: &str, #[case] expect_borrowed: bool) {
        let (val, _) = dq(input);
        if expect_borrowed {
            assert!(matches!(val, Cow::Borrowed(_)), "must be Borrowed");
        } else {
            assert!(matches!(val, Cow::Owned(_)), "must be Owned");
        }
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
    // Group I-I continued — borrow-path length cap (I-26, I-27, I-28)
    // -----------------------------------------------------------------------

    // I-26: Double-quoted borrow path (no escape) — over 1 MiB raises error.
    // Exercises the end-of-line borrow path (borrow_end = body.len()).
    #[test]
    fn double_quoted_borrow_path_length_cap_exceeded_raises_error() {
        let mut big = String::with_capacity(1_048_579);
        big.push('"');
        big.extend(std::iter::repeat_n('a', 1_048_577));
        big.push('"');
        let e = dq_err(&big);
        assert!(
            e.message.contains("maximum allowed length"),
            "expected length cap error, got: {}",
            e.message
        );
    }

    // I-27: Double-quoted borrow path — exactly at the 1 MiB limit succeeds.
    #[test]
    fn double_quoted_borrow_path_exactly_at_limit_succeeds() {
        let mut big = String::with_capacity(1_048_578);
        big.push('"');
        big.extend(std::iter::repeat_n('a', 1_048_576));
        big.push('"');
        let (val, _) = dq(&big);
        assert_eq!(val.len(), 1_048_576);
    }

    // I-28: Double-quoted borrow path — plain span hits closing `"` after > 1 MiB.
    // Exercises the borrow_end = hit site (line in the memchr2 loop else branch).
    #[test]
    fn double_quoted_borrow_path_mid_scan_cap_exceeded_raises_error() {
        // Build: 1_048_577 plain 'a' bytes then closing `"` — memchr2 finds the
        // closing `"` at position 1_048_577 and sets borrow_end = hit.
        let mut big = String::with_capacity(1_048_579);
        big.push('"');
        big.extend(std::iter::repeat_n('a', 1_048_577));
        big.push('"'); // closing quote found by memchr2 at hit = 1_048_577
        let e = dq_err(&big);
        assert!(
            e.message.contains("maximum allowed length"),
            "expected length cap error, got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group H-E — single-quoted length cap
    // -----------------------------------------------------------------------

    // H-E1: Single-quoted single-line borrow path — over 1 MiB raises error.
    #[test]
    fn single_quoted_single_line_borrow_path_length_cap_exceeded_raises_error() {
        let mut big = String::with_capacity(1_048_579);
        big.push('\'');
        big.extend(std::iter::repeat_n('a', 1_048_577));
        big.push('\'');
        let e = sq_err(&big);
        assert!(
            e.message.contains("maximum allowed length"),
            "expected length cap error, got: {}",
            e.message
        );
    }

    // H-E2: Single-quoted single-line borrow path — exactly at 1 MiB succeeds.
    #[test]
    fn single_quoted_single_line_borrow_path_exactly_at_limit_succeeds() {
        let mut big = String::with_capacity(1_048_578);
        big.push('\'');
        big.extend(std::iter::repeat_n('a', 1_048_576));
        big.push('\'');
        let (val, _) = sq(&big);
        assert_eq!(val.len(), 1_048_576);
        assert!(matches!(val, Cow::Borrowed(_)), "no escape → Borrowed");
    }

    // H-E3: Single-quoted multi-line owned path — accumulated total over 1 MiB raises error.
    #[test]
    fn single_quoted_multiline_owned_path_length_cap_exceeded_raises_error() {
        // Two lines of 600_000 'a' chars each; fold space between them makes
        // total 1_200_001 > 1_048_576.
        let mut big = String::with_capacity(1_200_010);
        big.push('\'');
        big.extend(std::iter::repeat_n('a', 600_000));
        big.push('\n');
        big.extend(std::iter::repeat_n('a', 600_000));
        big.push('\'');
        let e = sq_err(&big);
        assert!(
            e.message.contains("maximum allowed length"),
            "expected length cap error, got: {}",
            e.message
        );
    }

    // H-E4: Single-quoted multi-line with escape — cap fires on push_str after unescape.
    #[test]
    fn single_quoted_multiline_with_escape_owned_path_length_cap_exceeded_raises_error() {
        // First line: 600_000 'a' + '' escape (produces one literal ').
        // Second line: 600_000 'a' + closing '.
        // Total accumulated > 1_048_576.
        let mut big = String::with_capacity(1_200_015);
        big.push('\'');
        big.extend(std::iter::repeat_n('a', 600_000));
        big.push_str("''"); // '' escape forces owned path from the start
        big.push('\n');
        big.extend(std::iter::repeat_n('a', 600_000));
        big.push('\'');
        let e = sq_err(&big);
        assert!(
            e.message.contains("maximum allowed length"),
            "expected length cap error, got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group SQ-NP: single-quoted nb-json rejection (C0 controls)
    // -----------------------------------------------------------------------

    #[test]
    fn single_quoted_rejects_nul_in_body() {
        let e = sq_err("'val\x00ue'");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0000"),
            "expected non-printable error for NUL, got: {}",
            e.message
        );
    }

    #[test]
    fn single_quoted_rejects_0x01_soh_in_body() {
        let e = sq_err("'val\x01ue'");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0001"),
            "expected non-printable error for SOH, got: {}",
            e.message
        );
    }

    #[test]
    fn single_quoted_rejects_0x08_bs_in_body() {
        let e = sq_err("'val\x08ue'");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0008"),
            "expected non-printable error for BS, got: {}",
            e.message
        );
    }

    #[test]
    fn single_quoted_rejects_lf_literal_in_body() {
        // LF (U+000A) as a literal byte inside the single-quoted body is a line
        // break; nb-json excludes it (nb-json = x09 | [x20..x10FFFF]; LF = x0A).
        // The parser sees it as a multi-line scalar and processes it as a fold.
        // Verify no crash occurs and the character is not in the resulting value
        // as a raw C0 byte (it gets converted to a fold/newline by the spec).
        let result = make_lexer("'val\nue'").try_consume_single_quoted(0);
        // LF is a line break — the multi-line path produces a folded value;
        // it should not be rejected as non-printable (it's a structural break).
        if let Err(e) = result {
            assert!(
                !e.message.contains("non-printable"),
                "LF in single-quoted must not be rejected as non-printable: {}",
                e.message
            );
        }
    }

    #[test]
    fn single_quoted_rejects_0x0b_vt_in_body() {
        let e = sq_err("'val\x0bue'");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+000B"),
            "expected non-printable error for VT, got: {}",
            e.message
        );
    }

    #[test]
    fn single_quoted_rejects_0x0c_ff_in_body() {
        let e = sq_err("'val\x0cue'");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+000C"),
            "expected non-printable error for FF, got: {}",
            e.message
        );
    }

    #[test]
    fn single_quoted_rejects_cr_literal_in_body() {
        // CR (U+000D) as a literal byte — structural line break, not nb-json C0 rejection.
        // Same reasoning as LF: CR is a line break character, not rejected as non-printable.
        if let Err(e) = make_lexer("'val\rue'").try_consume_single_quoted(0) {
            assert!(
                !e.message.contains("non-printable"),
                "CR in single-quoted must not be rejected as non-printable: {}",
                e.message
            );
        }
    }

    #[test]
    fn single_quoted_rejects_0x1f_in_body() {
        let e = sq_err("'val\x1fue'");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+001F"),
            "expected non-printable error for US (U+001F), got: {}",
            e.message
        );
    }

    #[test]
    fn single_quoted_error_message_contains_uplus_hex() {
        let e = sq_err("'val\x07ue'");
        assert!(
            e.message.contains("U+0007"),
            "error message must contain U+0007, got: {}",
            e.message
        );
        assert!(
            e.message.contains("single-quoted scalar"),
            "error message must mention 'single-quoted scalar', got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group SQ-OK: single-quoted nb-json acceptance (DEL, C1, U+FFFE/FFFF)
    // -----------------------------------------------------------------------

    #[test]
    fn single_quoted_accepts_del_0x7f() {
        // DEL (0x7F) is ≥ 0x20 so it passes nb-json.
        let (val, _) = sq("'val\x7fue'");
        assert!(
            val.contains('\x7f'),
            "DEL must be accepted in single-quoted scalar"
        );
    }

    #[test]
    fn single_quoted_accepts_c1_0x80() {
        // U+0080 is a C1 control; nb-json allows it (≥ 0x20 in the [x20..x10FFFF] range
        // — wait, C1 = 0x80..0x9F which is > 0x20, so nb-json allows them).
        let (val, _) = sq("'val\u{0080}ue'");
        assert!(
            val.contains('\u{0080}'),
            "U+0080 must be accepted in single-quoted scalar"
        );
    }

    #[test]
    fn single_quoted_accepts_c1_0x9f() {
        let (val, _) = sq("'val\u{009F}ue'");
        assert!(
            val.contains('\u{009F}'),
            "U+009F must be accepted in single-quoted scalar"
        );
    }

    #[test]
    fn single_quoted_accepts_0xfffe() {
        let (val, _) = sq("'val\u{FFFE}ue'");
        assert!(
            val.contains('\u{FFFE}'),
            "U+FFFE must be accepted in single-quoted scalar"
        );
    }

    #[test]
    fn single_quoted_accepts_0xffff() {
        let (val, _) = sq("'val\u{FFFF}ue'");
        assert!(
            val.contains('\u{FFFF}'),
            "U+FFFF must be accepted in single-quoted scalar"
        );
    }

    #[test]
    fn single_quoted_accepts_tab() {
        // TAB is also allowed by nb-json (x09).
        let (val, _) = sq("'col1\tcol2'");
        assert!(
            val.contains('\t'),
            "TAB must be accepted in single-quoted scalar"
        );
    }

    // -----------------------------------------------------------------------
    // Group DQ-NP: double-quoted nb-json rejection (C0 literal content)
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_rejects_nul_literal_in_body() {
        let e = dq_err("\"val\x00ue\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0000"),
            "expected non-printable error for NUL, got: {}",
            e.message
        );
    }

    #[test]
    fn double_quoted_rejects_0x01_literal_in_body() {
        let e = dq_err("\"val\x01ue\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0001"),
            "expected non-printable error for SOH, got: {}",
            e.message
        );
    }

    #[test]
    fn double_quoted_rejects_0x08_bs_literal() {
        let e = dq_err("\"val\x08ue\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0008"),
            "expected non-printable error for BS, got: {}",
            e.message
        );
    }

    #[test]
    fn double_quoted_rejects_0x0b_vt_literal() {
        let e = dq_err("\"val\x0bue\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+000B"),
            "expected non-printable error for VT, got: {}",
            e.message
        );
    }

    #[test]
    fn double_quoted_rejects_0x1f_literal() {
        let e = dq_err("\"val\x1fue\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+001F"),
            "expected non-printable error for US (U+001F), got: {}",
            e.message
        );
    }

    #[test]
    fn double_quoted_error_message_contains_uplus_hex() {
        let e = dq_err("\"val\x07ue\"");
        assert!(
            e.message.contains("U+0007"),
            "error message must contain U+0007, got: {}",
            e.message
        );
        assert!(
            e.message.contains("double-quoted scalar"),
            "error message must mention 'double-quoted scalar', got: {}",
            e.message
        );
    }

    #[test]
    fn double_quoted_rejects_non_nb_json_adjacent_to_escape() {
        // BEL immediately before an escape sequence must be caught.
        let e = dq_err("\"val\x07\\nue\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0007"),
            "expected non-printable error for BEL before escape, got: {}",
            e.message
        );
    }

    #[test]
    fn double_quoted_rejects_non_nb_json_between_two_escapes() {
        // BEL between two escape sequences must be caught.
        let e = dq_err("\"\\n\x07\\n\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0007"),
            "expected non-printable error for BEL between escapes, got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group DQ-OK: double-quoted nb-json acceptance (DEL, C1, U+FFFE/FFFF)
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_accepts_del_0x7f_literal() {
        // DEL (0x7F) is ≥ 0x20 → accepted by nb-json.
        let (val, _) = dq("\"val\x7fue\"");
        assert!(
            val.contains('\x7f'),
            "DEL must be accepted in double-quoted scalar"
        );
    }

    #[test]
    fn double_quoted_accepts_c1_0x80_literal() {
        let (val, _) = dq("\"val\u{0080}ue\"");
        assert!(
            val.contains('\u{0080}'),
            "U+0080 must be accepted in double-quoted scalar"
        );
    }

    #[test]
    fn double_quoted_accepts_0xfffe_literal() {
        let (val, _) = dq("\"val\u{FFFE}ue\"");
        assert!(
            val.contains('\u{FFFE}'),
            "U+FFFE must be accepted in double-quoted scalar"
        );
    }

    #[test]
    fn double_quoted_accepts_0xffff_literal() {
        let (val, _) = dq("\"val\u{FFFF}ue\"");
        assert!(
            val.contains('\u{FFFF}'),
            "U+FFFF must be accepted in double-quoted scalar"
        );
    }

    // -----------------------------------------------------------------------
    // Group DQ-REG1: regression guard — named escape for C0 still accepted
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_escape_non_printable_still_rejected() {
        // A *literal* BEL byte in the source (not via escape) must be rejected.
        // This is the regression guard: make sure the nb-json check is on the
        // raw source span, not on the decoded value.
        let e = dq_err("\"literal\x07bel\"");
        assert!(
            e.message.contains("non-printable") || e.message.contains("U+0007"),
            "literal BEL must be rejected even in double-quoted, got: {}",
            e.message
        );
    }

    // =======================================================================
    // Group J — s-indent(n) enforcement on quoted scalar continuation lines
    // =======================================================================

    fn sq_with_indent(input: &str, n: usize) -> (Cow<'_, str>, Span) {
        make_lexer(input)
            .try_consume_single_quoted(n)
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"))
            .unwrap_or_else(|| unreachable!("expected Some, got None"))
    }

    fn sq_err_with_indent(input: &str, n: usize) -> Error {
        match make_lexer(input).try_consume_single_quoted(n) {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    fn dq_with_indent(input: &str, n: usize) -> (Cow<'_, str>, Span) {
        make_lexer(input)
            .try_consume_double_quoted(Some(n))
            .unwrap_or_else(|e| unreachable!("unexpected error: {e}"))
            .unwrap_or_else(|| unreachable!("expected Some, got None"))
    }

    fn dq_err_with_indent(input: &str, n: usize) -> Error {
        match make_lexer(input).try_consume_double_quoted(Some(n)) {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    // -----------------------------------------------------------------------
    // Group J-A: Single-quoted, block context (n > 0), happy path
    // -----------------------------------------------------------------------

    // J-A1: continuation with exactly n spaces is accepted.
    #[test]
    fn sq_indent_continuation_exactly_n_spaces_accepted() {
        let (val, _) = sq_with_indent("'foo\n  bar'", 2);
        assert_eq!(val, "foo bar");
    }

    // J-A2: continuation with more than n spaces is accepted.
    #[test]
    fn sq_indent_continuation_more_than_n_spaces_accepted() {
        let (val, _) = sq_with_indent("'foo\n    bar'", 2);
        assert_eq!(val, "foo bar");
    }

    // J-A3: continuation with exactly n spaces then a tab (tab in separation
    // position) is accepted.
    #[test]
    fn sq_indent_continuation_n_spaces_then_tab_accepted() {
        let (val, _) = sq_with_indent("'foo\n  \tbar'", 2);
        assert_eq!(val, "foo bar");
    }

    // J-A4: continuation at n=1 with one space is accepted.
    #[test]
    fn sq_indent_continuation_n1_one_space_accepted() {
        let (val, _) = sq_with_indent("'foo\n bar'", 1);
        assert_eq!(val, "foo bar");
    }

    // J-A5: blank continuation line with zero spaces is accepted (blank-line bypass).
    // The blank line itself is bypassed. The subsequent non-blank line must still
    // satisfy s-indent(n). Use n=1 so the final "bar" line (0 spaces) would fail
    // if checked — but it's the blank line we're testing. Use a properly-indented
    // final line so the test isolates the blank-line bypass behavior.
    #[test]
    fn sq_indent_blank_continuation_zero_spaces_accepted() {
        // n=2: blank line (0 spaces) is bypassed; " bar" (1 space + content) has
        // indent=1 which is less than n=2 and would fail if the blank-bypass
        // didn't fire. Instead test with blank between two well-indented lines.
        let (val, _) = sq_with_indent("'foo\n\n  bar'", 2);
        assert_eq!(val, "foo\nbar");
    }

    // J-A6: blank continuation line with fewer spaces than n is accepted (blank-line bypass).
    #[test]
    fn sq_indent_blank_continuation_fewer_spaces_accepted() {
        // Second line is " " (one space), trims to blank — should not trigger indent check.
        // Third line "  bar" has 2 spaces, meeting n=2 requirement.
        let (val, _) = sq_with_indent("'foo\n \n  bar'", 2);
        assert_eq!(val, "foo\nbar");
    }

    // J-A7: n=0 (flow context) skips indent check: tab-only indent is accepted.
    #[test]
    fn sq_indent_n0_tab_only_continuation_accepted() {
        let (val, _) = sq_with_indent("'foo\n\tbar'", 0);
        assert_eq!(val, "foo bar");
    }

    // -----------------------------------------------------------------------
    // Group J-B: Single-quoted, block context (n > 0), error path
    // -----------------------------------------------------------------------

    // J-B1: continuation with zero spaces when n=2 → error.
    #[test]
    fn sq_indent_continuation_zero_spaces_n2_err() {
        let e = sq_err_with_indent("'foo\nbar'", 2);
        assert!(
            e.message.contains("expected at least 2 spaces"),
            "expected indent error, got: {}",
            e.message
        );
    }

    // J-B2: continuation with n-1 spaces when n=3 → error with found count.
    #[test]
    fn sq_indent_continuation_n_minus_1_spaces_n3_err() {
        let e = sq_err_with_indent("'foo\n  bar'", 3);
        assert!(
            e.message.contains("expected at least 3 spaces"),
            "expected indent error, got: {}",
            e.message
        );
        assert!(
            e.message.contains("found 2"),
            "expected 'found 2' in error, got: {}",
            e.message
        );
    }

    // J-B3: continuation starting with a tab (tab in indent position) when n=2 → error.
    #[test]
    fn sq_indent_continuation_tab_in_indent_position_n2_err() {
        let e = sq_err_with_indent("'foo\n\tbar'", 2);
        assert!(
            e.message.contains("expected at least 2 spaces"),
            "expected indent error, got: {}",
            e.message
        );
        assert!(
            e.message.contains("found 0"),
            "expected 'found 0' in error, got: {}",
            e.message
        );
    }

    // J-B4: error message includes both expected and found counts.
    #[test]
    fn sq_indent_error_message_includes_expected_and_found() {
        let e = sq_err_with_indent("'foo\n  bar'", 4);
        assert!(
            e.message.contains('4'),
            "expected '4' in error message, got: {}",
            e.message
        );
        assert!(
            e.message.contains('2'),
            "expected '2' (found count) in error message, got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group J-C: Double-quoted, block context (Some(n)), happy path
    //
    // For double-quoted in block context, the parameter n is the ENCLOSING
    // block's indent. Continuation lines must have strictly more than n spaces
    // (i.e. at least n+1) — matching the block-context `s-flow-line-prefix`
    // requirement. `None` signals flow context where no constraint applies.
    // -----------------------------------------------------------------------

    // J-C1: continuation with more than n spaces (n=0, 2 spaces) is accepted.
    // Enclosing block at indent 0, continuation at 2 spaces → 2 > 0.
    #[test]
    fn dq_indent_continuation_more_than_n_spaces_accepted() {
        let (val, _) = dq_with_indent("\"foo\n  bar\"", 0);
        assert_eq!(val, "foo bar");
    }

    // J-C2: continuation with many more than n spaces (n=2, 4 spaces) is accepted.
    #[test]
    fn dq_indent_continuation_many_more_than_n_spaces_accepted() {
        let (val, _) = dq_with_indent("\"foo\n    bar\"", 2);
        assert_eq!(val, "foo bar");
    }

    // J-C3: continuation with n+1 spaces then a tab (tab in separation
    // position after sufficient spaces) is accepted.
    // n=2, 3 spaces + tab: indent=3 > 2 → accepted, tab is in separation phase.
    #[test]
    fn dq_indent_continuation_np1_spaces_then_tab_accepted() {
        let (val, _) = dq_with_indent("\"foo\n   \tbar\"", 2);
        assert_eq!(val, "foo bar");
    }

    // J-C4: blank continuation line with fewer spaces than n+1 is accepted (blank-line bypass).
    #[test]
    fn dq_indent_blank_continuation_fewer_spaces_accepted() {
        // Second line is " " (one space), trims to blank — bypassed by blank-line rule.
        // Third line "   bar" has 3 spaces, meeting n=2 requirement (3 > 2).
        let (val, _) = dq_with_indent("\"foo\n \n   bar\"", 2);
        assert_eq!(val, "foo\nbar");
    }

    // J-C5: None (flow context) skips indent check — tab-only is accepted.
    #[test]
    fn dq_indent_none_flow_context_tab_only_accepted() {
        let (val, _) = dq("\"foo\n\tbar\"");
        assert_eq!(val, "foo bar");
    }

    // J-C6: continuation after `\<LF>` line continuation escape with sufficient
    // spaces is accepted. n=0, continuation has 1 space (1 > 0).
    #[test]
    fn dq_indent_line_continuation_escape_with_sufficient_spaces_accepted() {
        let (val, _) = dq_with_indent("\"foo\\\n bar\"", 0);
        assert_eq!(val, "foobar");
    }

    // -----------------------------------------------------------------------
    // Group J-D: Double-quoted, block context (Some(n)), error path
    //
    // Continuation lines must have more than n spaces. If found <= n, error.
    // -----------------------------------------------------------------------

    // J-D1: continuation with zero spaces when n=0 → error (0 not > 0).
    #[test]
    fn dq_indent_continuation_zero_spaces_n0_err() {
        let e = dq_err_with_indent("\"foo\nbar\"", 0);
        assert!(
            e.message.contains("expected at least 1 spaces"),
            "expected indent error, got: {}",
            e.message
        );
        assert!(
            e.message.contains("found 0"),
            "expected 'found 0' in error, got: {}",
            e.message
        );
    }

    // J-D2: continuation with n spaces when n=2 → error (2 not > 2).
    #[test]
    fn dq_indent_continuation_exactly_n_spaces_n2_err() {
        let e = dq_err_with_indent("\"foo\n  bar\"", 2);
        assert!(
            e.message.contains("expected at least 3 spaces"),
            "expected indent error, got: {}",
            e.message
        );
        assert!(
            e.message.contains("found 2"),
            "expected 'found 2' in error, got: {}",
            e.message
        );
    }

    // J-D3: continuation starting with a tab (tab in indent position) when n=0 → error.
    // Tab contributes 0 to indent, so found=0, required>0.
    #[test]
    fn dq_indent_continuation_tab_in_indent_position_n0_err() {
        let e = dq_err_with_indent("\"foo\n\tbar\"", 0);
        assert!(
            e.message.contains("expected at least 1 spaces"),
            "expected indent error, got: {}",
            e.message
        );
        assert!(
            e.message.contains("found 0"),
            "expected 'found 0' in error, got: {}",
            e.message
        );
    }

    // J-D4: error message includes both expected and found counts.
    // n=2, continuation has 1 space → error "expected at least 3, found 1".
    #[test]
    fn dq_indent_error_message_includes_expected_and_found() {
        let e = dq_err_with_indent("\"foo\n bar\"", 2);
        assert!(
            e.message.contains('3'),
            "expected '3' (n+1) in error message, got: {}",
            e.message
        );
        assert!(
            e.message.contains('1'),
            "expected '1' (found count) in error message, got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Class 3: unterminated single-quoted scalar — error pos is opening `'`
    // -----------------------------------------------------------------------

    // 3a. Single-line input, EOF immediately after the body (harness spike).
    // `'hello world` — opening `'` at byte 0, line 1, col 0.
    #[test]
    fn single_quoted_unterminated_eof_pos_is_open_quote_col_0() {
        let e = sq_err("'hello world");
        assert_eq!(e.pos.byte_offset, 0, "byte_offset");
        assert_eq!(e.pos.line, 1, "line");
        assert_eq!(e.pos.column, 0, "column");
        assert!(e.message.contains("unterminated"), "message");
    }

    // 3b. Multi-line: quote opens on line 1, continuation line exists, then EOF.
    // Input: `'hello\n  world` — `'` at byte 0, line 1, col 0.
    #[test]
    fn single_quoted_unterminated_multiline_pos_is_open_quote() {
        let e = sq_err("'hello\n  world");
        assert_eq!(e.pos.byte_offset, 0, "byte_offset");
        assert_eq!(e.pos.line, 1, "line");
        assert_eq!(e.pos.column, 0, "column");
        assert!(e.message.contains("unterminated"), "message");
    }

    // 3c. Opening quote not at column 0 — verify open_pos tracks leading content.
    // `key: 'value\n  continuation` — `'` is at byte 5, line 1, col 5.
    // Drive through parse_events since the lexer is called by the event iterator
    // with context that establishes leading bytes.
    #[test]
    fn single_quoted_unterminated_non_zero_col_pos_is_open_quote() {
        // Use parse_events to get the full context where `'` is past leading content.
        let e = crate::parse_events("key: 'value\n  continuation")
            .find_map(Result::err)
            .expect("expected an error");
        assert_eq!(e.pos.byte_offset, 5, "byte_offset");
        assert_eq!(e.pos.line, 1, "line");
        assert_eq!(e.pos.column, 5, "column");
        assert!(e.message.contains("unterminated"), "message");
    }
}
