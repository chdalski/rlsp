// SPDX-License-Identifier: MIT

use std::borrow::Cow;

use crate::chars::{decode_escape, is_c_printable};
use crate::error::Error;
use crate::pos::{Pos, Span};

use super::{Lexer, is_doc_marker_line, pos_after_line};

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
pub(super) fn scan_double_quoted_line(
    body: &str,
    start_pos: Pos,
) -> Result<DoubleQuotedLine<'_>, Error> {
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
