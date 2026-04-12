// SPDX-License-Identifier: MIT

use memchr::{memchr, memchr2};
use std::borrow::Cow;

use crate::error::Error;
use crate::lines::LineBuffer;
use crate::pos::{Pos, Span};

use super::{Lexer, is_blank_or_comment, is_marker};
use crate::chars::{is_c_indicator, is_ns_char};
use crate::lines::pos_after_line;

impl<'input> Lexer<'input> {
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
                let hash_col_in_line =
                    crate::pos::column_at(consumed_first.content, hash_byte_in_line);
                let hash_pos = Pos {
                    byte_offset: consumed_first.pos.byte_offset + hash_byte_in_line,
                    line: consumed_first.pos.line,
                    column: consumed_first.pos.column + hash_col_in_line,
                };
                let span_end = crate::pos::advance_within_line(hash_pos.advance('#'), comment_text);
                // Validate comment text: YAML 1.2 §8.1.1 — comment lines must
                // not contain NUL (U+0000) since it is not a c-printable char.
                if let Some(bad_i) = memchr(b'\0', comment_text.as_bytes()) {
                    let bad_char_i = comment_text[..bad_i].chars().count();
                    let bad_pos = Pos {
                        byte_offset: hash_pos.byte_offset + 1 + bad_i,
                        line: hash_pos.line,
                        column: hash_pos.column + 1 + bad_char_i,
                    };
                    self.plain_scalar_suffix_error = Some(Error {
                        pos: bad_pos,
                        message: "invalid character U+0000 in comment".to_owned(),
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
                let bad_col_offset =
                    crate::pos::column_at(consumed_first.content, after_scalar_start + bad_i);
                let bad_pos = Pos {
                    byte_offset: consumed_first.pos.byte_offset + after_scalar_start + bad_i,
                    line: consumed_first.pos.line,
                    column: consumed_first.pos.column + bad_col_offset,
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
                let end_pos = crate::pos::advance_within_line(scalar_start_pos, first_value_ref);
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
                buf.extend(std::iter::repeat_n('\n', pending_blanks));
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
}

// ---------------------------------------------------------------------------
// Plain scalar first-line inspection
// ---------------------------------------------------------------------------

/// Peek at the next line in `buf` and determine whether it can start a plain
/// scalar in block context.
///
/// Returns `(leading_spaces, scalar_start_pos, first_value_len)` on success, or
/// `None` if the line cannot start a plain scalar.
pub(super) fn peek_plain_scalar_first_line(buf: &LineBuffer<'_>) -> Option<(usize, Pos, usize)> {
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
    let leading_chars = crate::pos::column_at(first.content, leading_bytes);
    let scalar_start_pos = Pos {
        byte_offset: first.offset + leading_bytes,
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
pub(super) fn scan_plain_line_block(content: &str) -> &str {
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut pos = 0;
    let mut committed_end: usize = 0;
    let mut prev_was_ws = false;

    while pos < len {
        // Use memchr2 to jump ahead to the next `:` or `#` — the only
        // context-sensitive ASCII terminators. Everything between `pos` and
        // the hit is processed with a fast byte-level scan.
        let candidate =
            memchr2(b':', b'#', bytes.get(pos..).unwrap_or_default()).map(|off| pos + off);

        // Process bytes before the candidate (or the rest of the string).
        let end = candidate.unwrap_or(len);
        while pos < end {
            let Some(&b) = bytes.get(pos) else { break };
            if b >= 0x80 {
                // Non-ASCII: decode one char and apply the char predicate.
                // pos is on a char boundary: ASCII advances by 1, non-ASCII
                // advances by ch.len_utf8(), so boundaries are always respected.
                let Some(ch) = content.get(pos..).and_then(|s| s.chars().next()) else {
                    break;
                };
                let ch_len = ch.len_utf8();
                let next_ch = content.get(pos + ch_len..).and_then(|s| s.chars().next());
                if !ns_plain_char_block(prev_was_ws, ch, next_ch) {
                    return &content[..committed_end];
                }
                committed_end = pos + ch_len;
                prev_was_ws = false;
                pos += ch_len;
            } else {
                match b {
                    b' ' | b'\t' => {
                        prev_was_ws = true;
                        pos += 1;
                    }
                    // NUL, control bytes, DEL, and line breaks are not ns-char.
                    // Note: 0x00..=0x1F covers \t (0x09) but \t is matched above.
                    0x00..=0x1F | 0x7F => {
                        return &content[..committed_end];
                    }
                    _ => {
                        // Safe printable ASCII (0x21–0x7E excluding `:` and `#`,
                        // which are caught by memchr2 above).
                        committed_end = pos + 1;
                        prev_was_ws = false;
                        pos += 1;
                    }
                }
            }
        }

        // Handle the candidate byte (`:` or `#`), if any.
        let Some(hit) = candidate else { break };
        let Some(&b) = bytes.get(hit) else { break };
        if b == b'#' {
            if prev_was_ws {
                break;
            }
        } else {
            // b == b':'
            // `:` is content only when followed by an ns_plain_safe_block char.
            let next_ch = content.get(hit + 1..).and_then(|s| s.chars().next());
            if !next_ch.is_some_and(ns_plain_safe_block) {
                break;
            }
        }
        committed_end = hit + 1;
        prev_was_ws = false;
        pos = hit + 1;
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
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut pos = 0;
    let mut committed_end: usize = 0;
    let mut prev_was_ws = false;

    while pos < len {
        // Use memchr2 to jump ahead to the next `:` or `#`. Flow indicators
        // (`,`, `[`, `]`, `{`, `}`) are checked byte-by-byte in the segment
        // loop since they're simple ASCII terminators.
        let candidate =
            memchr2(b':', b'#', bytes.get(pos..).unwrap_or_default()).map(|off| pos + off);

        let end = candidate.unwrap_or(len);
        while pos < end {
            let Some(&b) = bytes.get(pos) else { break };
            if b >= 0x80 {
                let Some(ch) = content.get(pos..).and_then(|s| s.chars().next()) else {
                    break;
                };
                let ch_len = ch.len_utf8();
                if !ns_plain_safe_block(ch) {
                    return &content[..committed_end];
                }
                committed_end = pos + ch_len;
                prev_was_ws = false;
                pos += ch_len;
            } else {
                match b {
                    b' ' | b'\t' => {
                        prev_was_ws = true;
                        pos += 1;
                    }
                    // Flow indicators, line breaks, NUL, control bytes, and DEL
                    // all terminate the plain scalar.
                    b',' | b'[' | b']' | b'{' | b'}' | 0x00..=0x1F | 0x7F => {
                        return &content[..committed_end];
                    }
                    _ => {
                        committed_end = pos + 1;
                        prev_was_ws = false;
                        pos += 1;
                    }
                }
            }
        }

        let Some(hit) = candidate else { break };
        let Some(&b) = bytes.get(hit) else { break };
        if b == b'#' {
            if prev_was_ws {
                break;
            }
        } else {
            // b == b':'
            // In flow context, `:` terminates when followed by space, tab,
            // flow indicator, or end-of-content.
            match bytes.get(hit + 1).copied() {
                None | Some(b' ' | b'\t' | b',' | b'[' | b']' | b'{' | b'}') => break,
                // Non-ASCII next char: ns_plain_safe_block is true for all
                // valid non-ASCII ns-chars, so `:` is content — fall through.
                Some(_) => {}
            }
        }
        committed_end = hit + 1;
        prev_was_ws = false;
        pos = hit + 1;
    }

    &content[..committed_end]
}

// ---------------------------------------------------------------------------
// Trailing comment extraction
// ---------------------------------------------------------------------------

/// Extract a trailing comment from the content that follows a scalar value.
///
/// `suffix` is the slice of the line after the scalar ends (may be empty,
/// whitespace-only, or `"  # comment"`).
///
/// Returns the comment body (everything after the `#`) if a comment is
/// present (i.e. `#` is preceded by at least one whitespace), or `None`
/// if there is no comment in this suffix.
pub fn extract_trailing_comment(suffix: &str) -> Option<&str> {
    let bytes = suffix.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = memchr(b'#', bytes.get(search_from..).unwrap_or_default()) {
        let i = search_from + rel;
        // `#` must be preceded by whitespace (or be at position 0, which was
        // preceded by the scalar end — treated as a boundary).
        let preceded_by_ws = i == 0 || matches!(bytes.get(i - 1), Some(b' ' | b'\t'));
        if preceded_by_ws {
            // SAFETY: i + 1 is a valid char boundary because `#` is ASCII (1 byte).
            return Some(&suffix[i + 1..]);
        }
        search_from = i + 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use std::borrow::Cow;

    fn make_lexer(input: &str) -> super::super::Lexer<'_> {
        super::super::Lexer::new(input)
    }

    // -----------------------------------------------------------------------
    // Group A — scan_plain_line_block: ASCII baseline
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::empty_input_returns_empty("", "")]
    #[case::single_ascii_safe_char("a", "a")]
    #[case::plain_word_no_terminators("hello", "hello")]
    #[case::trailing_whitespace_excluded("abc   ", "abc")]
    #[case::colon_followed_by_space_terminates("key: rest", "key")]
    #[case::colon_at_eol_terminates("key:", "key")]
    #[case::colon_followed_by_alnum_is_content("a:b", "a:b")]
    #[case::hash_after_space_terminates("foo # comment", "foo")]
    #[case::hash_without_preceding_space_is_content("a#b", "a#b")]
    #[case::url_with_colon_slash_slash_is_content("http://example.com", "http://example.com")]
    fn scan_plain_line_block_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(scan_plain_line_block(input), expected);
    }

    // -----------------------------------------------------------------------
    // Group B — scan_plain_line_block: memchr candidate bytes in multi-byte positions
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::pure_cjk_no_terminators("日本語", "日本語")]
    #[case::mixed_ascii_then_multibyte_no_terminator("hello日本語", "hello日本語")]
    #[case::mixed_multibyte_then_ascii_no_terminator("日本語hello", "日本語hello")]
    #[case::colon_followed_by_multibyte_is_content("key:値", "key:値")]
    #[case::colon_followed_by_two_byte_char_is_content("key:ñ", "key:ñ")]
    #[case::colon_followed_by_four_byte_char_is_content("key:😀", "key:😀")]
    #[case::hash_after_multibyte_not_whitespace_preceded_is_content("日#本", "日#本")]
    #[case::hash_after_space_in_multibyte_context_terminates("日本 #note", "日本")]
    fn scan_plain_line_block_multibyte_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(scan_plain_line_block(input), expected);
    }

    // -----------------------------------------------------------------------
    // Group C — scan_plain_line_block: NUL and BOM as terminators
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::nul_byte_mid_scalar_terminates_before_nul("hello\0world", "hello")]
    #[case::nul_byte_at_start_returns_empty("\0abc", "")]
    #[case::bom_mid_scalar_terminates_before_bom("hello\u{FEFF}world", "hello")]
    #[case::bom_at_start_returns_empty("\u{FEFF}abc", "")]
    fn scan_plain_line_block_terminator_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(scan_plain_line_block(input), expected);
    }

    // -----------------------------------------------------------------------
    // Group D — scan_plain_line_block: whitespace edge cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::tab_between_words_included("abc\tdef", "abc\tdef")]
    #[case::multiple_spaces_between_words_included("foo  bar", "foo  bar")]
    #[case::trailing_tab_excluded("abc\t", "abc")]
    fn scan_plain_line_block_whitespace_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(scan_plain_line_block(input), expected);
    }

    // -----------------------------------------------------------------------
    // Group E — scan_plain_line_flow: multi-byte parity with block
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::pure_cjk_returns_full("日本語", "日本語")]
    #[case::colon_followed_by_multibyte_is_content("key:値", "key:値")]
    #[case::colon_followed_by_flow_indicator_terminates("key:]rest", "key")]
    #[case::nul_mid_scalar_terminates("hello\0world", "hello")]
    #[case::bom_mid_scalar_terminates("hello\u{FEFF}world", "hello")]
    #[case::mixed_ascii_cjk_no_terminator("abc中文", "abc中文")]
    fn scan_plain_line_flow_multibyte_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(scan_plain_line_flow(input), expected);
    }

    // -----------------------------------------------------------------------
    // Group F — slice validity (UTF-8 boundary regression guard)
    // -----------------------------------------------------------------------

    // Tests where result is asserted equal and chars().count() must not panic.
    // Merges Group B last 3 tests + Group F first 2 tests.
    #[rstest]
    #[case::block_two_byte_char_correct_slice(scan_plain_line_block as fn(&str) -> &str, "café", "café")]
    #[case::block_three_byte_char_correct_slice(scan_plain_line_block as fn(&str) -> &str, "中文abc", "中文abc")]
    #[case::block_four_byte_char_correct_slice(scan_plain_line_block as fn(&str) -> &str, "😀abc", "😀abc")]
    #[case::block_colon_then_multibyte(scan_plain_line_block as fn(&str) -> &str, "a:b中文", "a:b中文")]
    #[case::block_termination_before_multibyte(scan_plain_line_block as fn(&str) -> &str, "foo: 日本語", "foo")]
    fn scan_plain_line_slice_no_panic_cases(
        #[case] scan_fn: fn(&str) -> &str,
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let result = scan_fn(input);
        assert_eq!(result, expected);
        let _ = result.chars().count(); // must not panic (valid UTF-8 boundary)
    }

    #[test]
    fn flow_slice_valid_utf8_after_flow_indicator() {
        let result = scan_plain_line_flow("日本語]rest");
        assert_eq!(result, "日本語");
        assert_eq!(result.chars().count(), 3);
    }

    // -----------------------------------------------------------------------
    // Group G — try_consume_plain_scalar unit tests
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::single_word("hello", "hello")]
    #[case::multi_word("hello world", "hello world")]
    #[case::with_url("http://x.com", "http://x.com")]
    #[case::with_hash_no_preceding_space("a#b", "a#b")]
    #[case::terminated_by_colon_space("key: value", "key")]
    #[case::terminated_by_hash_with_space("foo # comment", "foo")]
    #[case::trailing_whitespace_stripped("foo   ", "foo")]
    #[case::multiline_folds_single_break_to_space("foo\n  bar\n  baz", "foo bar baz")]
    #[case::multiline_blank_line_folds_to_newline("foo\n\nbar", "foo\nbar")]
    #[case::minus_followed_by_safe_char_is_valid("-a", "-a")]
    #[case::colon_followed_by_safe_char_is_valid(":a", ":a")]
    #[case::forbidden_continuation_stops_at_marker("foo\n---\nbar", "foo")]
    #[case::with_multibyte_utf8("中文", "中文")]
    #[case::with_backslashes("plain\\value\\with\\backslashes", "plain\\value\\with\\backslashes")]
    fn try_consume_plain_scalar_value_cases(#[case] input: &str, #[case] expected: &str) {
        let mut lex = make_lexer(input);
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, expected);
    }

    #[rstest]
    #[case::cow_borrowed_for_single_line("hello", true)]
    #[case::cow_owned_for_multiline("foo\n  bar", false)]
    fn try_consume_plain_scalar_cow_cases(#[case] input: &str, #[case] expect_borrowed: bool) {
        let mut lex = make_lexer(input);
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        if expect_borrowed {
            assert!(matches!(val, Cow::Borrowed(_)), "expected Cow::Borrowed");
        } else {
            assert!(matches!(val, Cow::Owned(_)), "expected Cow::Owned");
        }
    }

    #[rstest]
    #[case::empty_input_returns_none("")]
    #[case::blank_line_returns_none("   ")]
    #[case::comment_line_returns_none("# comment")]
    #[case::dash_space_returns_none("- ")]
    #[case::question_space_returns_none("? ")]
    #[case::colon_space_returns_none(": ")]
    fn try_consume_plain_scalar_returns_none_cases(#[case] input: &str) {
        let mut lex = make_lexer(input);
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

    // Span cases: (input, parent_indent, expected_val, expected_start, expected_end)
    // expected_val is checked when Some; None means skip val assertion.
    #[rstest]
    #[case::ascii_word_span("hello", 0, Some("hello"), 0, 5)]
    #[case::indented_start_span("  hello", 0, Some("hello"), 2, 7)]
    #[case::multibyte_utf8_span("中文", 0, Some("中文"), 0, 6)]
    #[case::multibyte_with_leading_space_span("  中", 0, Some("中"), 2, 5)]
    fn try_consume_plain_scalar_span_cases(
        #[case] input: &str,
        #[case] parent_indent: usize,
        #[case] expected_val: Option<&str>,
        #[case] expected_start: usize,
        #[case] expected_end: usize,
    ) {
        let mut lex = make_lexer(input);
        let (val, span) = lex
            .try_consume_plain_scalar(parent_indent)
            .unwrap_or_else(|| unreachable!("should parse"));
        if let Some(ev) = expected_val {
            assert_eq!(val, ev);
        }
        assert_eq!(span.start.byte_offset, expected_start);
        assert_eq!(span.end.byte_offset, expected_end);
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

    // -----------------------------------------------------------------------
    // Group B (TE additions) — colon termination edge cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::colon_tab_terminates("key:\tvalue", "key")]
    #[case::colon_eof_terminates("key:", "key")]
    fn try_consume_plain_scalar_colon_edge_cases(#[case] input: &str, #[case] expected: &str) {
        let mut lex = make_lexer(input);
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, expected);
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

    #[rstest]
    #[case::two_blank_lines_fold_to_two_newlines("foo\n\n\nbar", "foo\n\nbar")]
    #[case::continuation_trailing_space_stripped("foo\nbar   \nbaz", "foo bar baz")]
    fn try_consume_plain_scalar_multiline_folding_cases(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let mut lex = make_lexer(input);
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, expected);
    }

    // -----------------------------------------------------------------------
    // Group D (TE required) — two blank lines with indented continuation
    // -----------------------------------------------------------------------

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
    // Group F (TE additions) — c-forbidden disambiguation
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::dots_terminated_by_document_end_marker("foo\n...\nbar", "foo")]
    #[case::dash_dash_dash_word_attached_is_not_forbidden("foo\n---word", "foo ---word")]
    fn try_consume_plain_scalar_forbidden_marker_cases(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let mut lex = make_lexer(input);
        let (val, _) = lex
            .try_consume_plain_scalar(0)
            .unwrap_or_else(|| unreachable!("should parse"));
        assert_eq!(val, expected);
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

    // -----------------------------------------------------------------------
    // Group SPF: scan_plain_line_flow
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::terminates_at_close_bracket("abc]rest", "abc")]
    #[case::terminates_at_close_brace("abc}rest", "abc")]
    #[case::terminates_at_comma("abc,rest", "abc")]
    #[case::terminates_at_open_bracket("abc[rest", "abc")]
    #[case::terminates_at_open_brace("abc{rest", "abc")]
    #[case::returns_full_when_no_terminator("hello", "hello")]
    #[case::empty_input_returns_empty("", "")]
    #[case::hash_after_space_starts_comment("abc # comment", "abc")]
    #[case::hash_without_preceding_space_is_content("abc#def", "abc#def")]
    #[case::colon_space_terminates("key: rest", "key")]
    #[case::colon_flow_indicator_terminates("key:}rest", "key")]
    #[case::colon_at_eol_terminates("key:", "key")]
    #[case::colon_followed_by_alnum_is_content("a:b", "a:b")]
    #[case::trailing_whitespace_excluded("abc   ", "abc")]
    fn scan_plain_line_flow_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(scan_plain_line_flow(input), expected);
    }
}
