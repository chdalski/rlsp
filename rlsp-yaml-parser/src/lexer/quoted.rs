// SPDX-License-Identifier: MIT

use std::borrow::Cow;

use memchr::{memchr, memchr2};

use crate::chars::{decode_escape, is_c_printable};
use crate::error::Error;
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
    let bytes = body.as_bytes();
    let mut i = 0;
    // We delay allocation until the first escape or discovery of multi-line.
    let mut owned: Option<String> = None;
    // Byte length of the borrow-safe prefix (updated only while owned is None).
    let mut borrow_end: usize = 0;

    while let Some(rel) = memchr2(b'"', b'\\', bytes.get(i..).unwrap_or_default()) {
        let hit = i + rel;

        // Accumulate the plain span [i..hit] that memchr skipped over.
        // For the borrow case we simply extend borrow_end; for owned we push.
        if let Some(buf) = owned.as_mut() {
            let span = body.get(i..hit).unwrap_or_default();
            buf.push_str(span);
            if buf.len() > 1_048_576 {
                return Err(Error {
                    pos: start_pos,
                    message: "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
                });
            }
        } else {
            borrow_end = hit;
        }

        if bytes.get(hit) == Some(&b'"') {
            // Closing quote.
            let content_end_pos = pos_after_str(start_pos, body.get(..hit).unwrap_or_default());
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
            let escape_pos = pos_after_str(start_pos, body.get(..hit).unwrap_or_default());
            let after_backslash = body.get(hit + 1..).unwrap_or_default();

            if after_backslash.is_empty() {
                // `\` at end of line — line continuation.
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
            i = hit + 1 + consumed; // skip `\` + escape body
        }
    }

    // No more `"` or `\` — consume the rest of the line as plain content.
    let rest = body.get(i..).unwrap_or_default();
    if let Some(buf) = owned.as_mut() {
        buf.push_str(rest);
        if buf.len() > 1_048_576 {
            return Err(Error {
                pos: start_pos,
                message: "scalar exceeds maximum allowed length (1 MiB)".to_owned(),
            });
        }
    } else {
        borrow_end = body.len();
    }

    // End of line without closing `"` — trim trailing whitespace before fold.
    let value = owned.map_or_else(
        || {
            let s = body
                .get(..borrow_end)
                .unwrap_or_default()
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

/// Advance `pos` over all characters in `s`, returning the resulting position.
fn pos_after_str(pos: Pos, s: &str) -> Pos {
    let mut p = pos;
    for c in s.chars() {
        p = p.advance(c);
    }
    p
}

/// Ensure `owned` is populated (allocating from `prefix` if needed), and
/// return a mutable reference to it.
fn ensure_owned<'s>(owned: &'s mut Option<String>, prefix: &str) -> &'s mut String {
    owned.get_or_insert_with(|| prefix.to_owned())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

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
        scan_double_quoted_line(input, START)
            .unwrap_or_else(|e| unreachable!("unexpected error: {}", e.message))
    }

    #[test]
    fn dq_empty_body_closes_immediately() {
        let result = dq_ok("\"");
        assert!(matches!(
            result,
            DoubleQuotedLine::Closed {
                value: DoubleQuotedValue::Borrowed(""),
                tail: "",
                ..
            }
        ));
    }

    #[test]
    fn dq_plain_ascii_borrows_and_closes() {
        let result = dq_ok("hello\"");
        assert!(matches!(
            result,
            DoubleQuotedLine::Closed {
                value: DoubleQuotedValue::Borrowed("hello"),
                tail: "",
                ..
            }
        ));
    }

    #[test]
    fn dq_newline_escape_forces_owned() {
        match dq_ok("a\\nb\"") {
            DoubleQuotedLine::Closed { value, tail, .. } => {
                assert_eq!(value.into_string(), "a\nb");
                assert_eq!(tail, "");
            }
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed"),
        }
    }

    #[test]
    fn dq_unicode_escape_u4() {
        match dq_ok("\\u00E9\"") {
            DoubleQuotedLine::Closed { value, .. } => assert_eq!(value.into_string(), "é"),
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed"),
        }
    }

    #[test]
    fn dq_unicode_escape_u8_supplementary() {
        match dq_ok("\\U0001F600\"") {
            DoubleQuotedLine::Closed { value, .. } => assert_eq!(value.into_string(), "😀"),
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed"),
        }
    }

    #[test]
    fn dq_hex_escape_xff() {
        match dq_ok("\\xFF\"") {
            DoubleQuotedLine::Closed { value, .. } => assert_eq!(value.into_string(), "\u{FF}"),
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed"),
        }
    }

    #[test]
    fn dq_backslash_at_end_is_line_continuation() {
        match dq_ok("text\\") {
            DoubleQuotedLine::Incomplete {
                value,
                line_continuation,
            } => {
                assert_eq!(value.into_string(), "text");
                assert!(line_continuation);
            }
            DoubleQuotedLine::Closed { .. } => unreachable!("expected Incomplete"),
        }
    }

    #[test]
    fn dq_no_delimiter_trims_trailing_whitespace() {
        match dq_ok("hello   ") {
            DoubleQuotedLine::Incomplete {
                value,
                line_continuation,
            } => {
                assert_eq!(value.into_string(), "hello");
                assert!(!line_continuation);
            }
            DoubleQuotedLine::Closed { .. } => unreachable!("expected Incomplete"),
        }
    }

    #[test]
    fn dq_multibyte_no_escape_borrows() {
        let result = dq_ok("café\"");
        assert!(matches!(
            result,
            DoubleQuotedLine::Closed {
                value: DoubleQuotedValue::Borrowed("café"),
                tail: "",
                ..
            }
        ));
    }

    #[test]
    fn dq_multibyte_then_escape_accumulates_correctly() {
        match dq_ok("café\\n\"") {
            DoubleQuotedLine::Closed { value, .. } => assert_eq!(value.into_string(), "café\n"),
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed"),
        }
    }

    #[test]
    fn dq_escape_then_multibyte_accumulates_correctly() {
        match dq_ok("\\ncafé\"") {
            DoubleQuotedLine::Closed { value, .. } => assert_eq!(value.into_string(), "\ncafé"),
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed"),
        }
    }

    #[test]
    fn dq_bidi_escape_is_rejected() {
        match scan_double_quoted_line("\\u202A\"", START) {
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
        match scan_double_quoted_line("\\x01\"", START) {
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
        match scan_double_quoted_line("\\q\"", START) {
            Err(e) => assert!(
                e.message.contains("invalid escape"),
                "message: {}",
                e.message
            ),
            Ok(_) => unreachable!("expected Err for unknown escape"),
        }
    }

    #[test]
    fn dq_tail_after_closing_quote_is_captured() {
        let result = dq_ok("hello\" world");
        assert!(matches!(
            result,
            DoubleQuotedLine::Closed {
                value: DoubleQuotedValue::Borrowed("hello"),
                tail: " world",
                ..
            }
        ));
    }

    #[test]
    fn dq_null_byte_escape_is_allowed() {
        match dq_ok("\\0\"") {
            DoubleQuotedLine::Closed { value, .. } => assert_eq!(value.into_string(), "\0"),
            DoubleQuotedLine::Incomplete { .. } => unreachable!("expected Closed"),
        }
    }

    // =======================================================================
    // Group H — try_consume_single_quoted (Task 7)
    // =======================================================================

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
}
