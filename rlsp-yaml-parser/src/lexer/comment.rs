// SPDX-License-Identifier: MIT

use memchr::memchr;

use crate::error::Error;
use crate::pos::{Pos, Span};

use super::Lexer;
use crate::lines::pos_after_line;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_lexer(input: &str) -> Lexer<'_> {
        Lexer::new(input)
    }

    // -----------------------------------------------------------------------
    // Group A — returns None (non-comment lines)
    // -----------------------------------------------------------------------

    #[test]
    fn returns_none_at_eof() {
        let mut lex = make_lexer("");
        assert_eq!(lex.try_consume_comment(1024), Ok(None));
    }

    #[test]
    fn returns_none_for_blank_line() {
        let mut lex = make_lexer("\n");
        assert_eq!(lex.try_consume_comment(1024), Ok(None));
    }

    #[test]
    fn returns_none_for_whitespace_only_line() {
        let mut lex = make_lexer("   \n");
        assert_eq!(lex.try_consume_comment(1024), Ok(None));
    }

    #[test]
    fn returns_none_for_content_line() {
        let mut lex = make_lexer("key: value\n");
        assert_eq!(lex.try_consume_comment(1024), Ok(None));
    }

    #[test]
    fn returns_none_for_directive_line() {
        let mut lex = make_lexer("%YAML 1.2\n");
        assert_eq!(lex.try_consume_comment(1024), Ok(None));
    }

    // -----------------------------------------------------------------------
    // Group B — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn plain_comment_returns_text_after_hash() {
        let mut lex = make_lexer("# hello\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, " hello");
    }

    #[test]
    fn indented_comment_returns_text_after_hash() {
        let mut lex = make_lexer("  # indented\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, " indented");
    }

    #[test]
    fn tab_indented_comment_returns_text() {
        let mut lex = make_lexer("\t# tabbed\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, " tabbed");
    }

    #[test]
    fn empty_comment_body_returns_empty_text() {
        let mut lex = make_lexer("#\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, "");
    }

    #[test]
    fn comment_with_hash_in_body_preserves_inner_hash() {
        let mut lex = make_lexer("# foo # bar\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, " foo # bar");
    }

    // -----------------------------------------------------------------------
    // Group C — span correctness
    // -----------------------------------------------------------------------

    #[test]
    fn span_start_byte_offset_at_hash() {
        let mut lex = make_lexer("# comment\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.start.byte_offset, 0);
    }

    #[test]
    fn span_start_column_at_hash_after_leading_spaces() {
        let mut lex = make_lexer("   # comment\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.start.column, 3);
    }

    #[test]
    fn span_start_line_is_one() {
        let mut lex = make_lexer("# comment\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.start.line, 1);
    }

    #[test]
    fn span_end_byte_offset_past_last_char() {
        // "# abc\n": # (1) + space (1) + abc (3) = 5 bytes before newline
        let mut lex = make_lexer("# abc\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.end.byte_offset, 5);
    }

    #[test]
    fn span_end_column_past_last_char() {
        // columns: # = 0, space = 1, a = 2, b = 3, c = 4; end = 5
        let mut lex = make_lexer("# abc\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.end.column, 5);
    }

    #[test]
    fn span_for_empty_comment_body() {
        let mut lex = make_lexer("#\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.start.byte_offset, 0);
        assert_eq!(span.end.byte_offset, 1);
    }

    // -----------------------------------------------------------------------
    // Group D — state effects after consume
    // -----------------------------------------------------------------------

    #[test]
    fn lexer_position_advances_past_consumed_comment_line() {
        // "# c\n" = 4 bytes: #, space, c, newline
        let mut lex = make_lexer("# c\nnext\n");
        let Ok(_) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok")
        };
        assert_eq!(lex.current_pos().byte_offset, 4);
    }

    #[test]
    fn next_line_is_available_after_comment_consumed() {
        let mut lex = make_lexer("# comment\nnext\n");
        let Ok(_) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok")
        };
        assert_eq!(lex.peek_next_line().map(|l| l.content), Some("next"));
    }

    #[test]
    fn comment_not_consumed_on_none_return() {
        let mut lex = make_lexer("content\n# comment\n");
        let Ok(result) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok")
        };
        assert_eq!(result, None);
        assert_eq!(lex.peek_next_line().map(|l| l.content), Some("content"));
    }

    // -----------------------------------------------------------------------
    // Group E — max_comment_len enforcement
    // -----------------------------------------------------------------------

    #[test]
    fn comment_within_limit_returns_ok() {
        // "# ab\n" -> text " ab" = 3 bytes; limit = 3
        let mut lex = make_lexer("# ab\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(3) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, " ab");
    }

    #[test]
    fn comment_exactly_at_limit_returns_ok() {
        // "# abc\n" -> text " abc" = 4 bytes; limit = 4 (boundary inclusive)
        let mut lex = make_lexer("# abc\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(4) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, " abc");
    }

    #[test]
    fn comment_exceeding_limit_returns_err() {
        // "# abc\n" -> text " abc" = 4 bytes; limit = 3 -> error
        let mut lex = make_lexer("# abc\n");
        let Err(err) = lex.try_consume_comment(3) else {
            unreachable!("expected Err")
        };
        assert!(
            err.message
                .contains("comment exceeds maximum allowed length")
        );
    }

    #[test]
    fn error_pos_points_to_hash() {
        // "  # toolong\n": hash is at byte 2, column 2; limit 0 forces error
        let mut lex = make_lexer("  # toolong\n");
        let Err(err) = lex.try_consume_comment(0) else {
            unreachable!("expected Err")
        };
        assert_eq!(err.pos.byte_offset, 2);
        assert_eq!(err.pos.column, 2);
    }

    // -----------------------------------------------------------------------
    // Group F — multibyte / Unicode in comment body
    // -----------------------------------------------------------------------

    #[test]
    fn unicode_body_text_is_slice_of_input() {
        let mut lex = make_lexer("# 日本語\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, " 日本語");
    }

    #[test]
    fn span_end_byte_offset_for_multibyte_body() {
        // "# 日\n": # (1) + space (1) + 日 (3) = 5 bytes
        let mut lex = make_lexer("# 日\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.end.byte_offset, 5);
    }

    #[test]
    fn span_end_column_for_multibyte_body() {
        // columns: # = 0, space = 1, 日 = 2, end = 3 (column counts codepoints)
        let mut lex = make_lexer("# 日\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.end.column, 3);
    }
}
