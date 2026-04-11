// SPDX-License-Identifier: MIT

use memchr::memchr;

use crate::error::Error;
use crate::pos::{Pos, Span};

use super::{Lexer, pos_after_line};

impl<'input> Lexer<'input> {
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
    ) -> Result<Option<(&'input str, Span)>, Error> {
        let Some(line) = self.buf.peek_next() else {
            return Ok(None);
        };

        let trimmed = line.content.trim_start_matches([' ', '\t']);
        if !trimmed.starts_with('#') {
            return Ok(None);
        }

        // The `#` is the first non-whitespace character.
        // `#` is ASCII so memchr finds its byte offset directly.
        let hash_byte_offset = memchr(b'#', line.content.as_bytes()).unwrap_or(0);

        let hash_col = crate::pos::column_at(line.content, hash_byte_offset);
        let hash_pos = Pos {
            byte_offset: line.pos.byte_offset + hash_byte_offset,
            line: line.pos.line,
            column: line.pos.column + hash_col,
        };

        // Comment text: everything after the `#`.
        // text_start is always ≤ line.content.len(): memchr returns a valid
        // byte offset (< len) when Some, and `#` is 1 byte, giving text_start ≤ len.
        // The slice is always on a char boundary because `#` is ASCII.
        let text_start = hash_byte_offset + 1; // byte after `#`
        let text: &'input str = &line.content[text_start..];

        if text.len() > max_comment_len {
            return Err(Error {
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
}
