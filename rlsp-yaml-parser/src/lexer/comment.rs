// SPDX-License-Identifier: MIT

use memchr::memchr;

use crate::chars::{find_non_c_printable, non_printable_error_message};
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

        // Validate c-printable on the comment body.
        if let Some((bad_i, bad_ch)) = find_non_c_printable(text.as_bytes()) {
            let bad_char_count = text[..bad_i].chars().count();
            let bad_pos = Pos {
                byte_offset: hash_pos.byte_offset + 1 + bad_i,
                line: hash_pos.line,
                column: hash_pos.column + 1 + bad_char_count,
            };
            return Err(Error {
                pos: bad_pos,
                message: non_printable_error_message(bad_ch, "comment"),
            });
        }

        // Span: from `#` through end of text (not the newline).
        let span_end = crate::pos::advance_within_line(hash_pos.advance('#'), text);
        let span = Span::from_pos(hash_pos, span_end);

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
    use rstest::rstest;

    use super::*;

    fn make_lexer(input: &str) -> Lexer<'_> {
        Lexer::new(input)
    }

    // -----------------------------------------------------------------------
    // Group A — returns None (non-comment lines)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::returns_none_at_eof("")]
    #[case::returns_none_for_blank_line("\n")]
    #[case::returns_none_for_whitespace_only_line("   \n")]
    #[case::returns_none_for_content_line("key: value\n")]
    #[case::returns_none_for_directive_line("%YAML 1.2\n")]
    fn returns_none(#[case] input: &str) {
        let mut lex = make_lexer(input);
        assert_eq!(lex.try_consume_comment(1024), Ok(None));
    }

    // -----------------------------------------------------------------------
    // Group B — happy path (+ Group F text case)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::plain_comment_returns_text_after_hash("# hello\n", " hello")]
    #[case::indented_comment_returns_text_after_hash("  # indented\n", " indented")]
    #[case::tab_indented_comment_returns_text("\t# tabbed\n", " tabbed")]
    #[case::empty_comment_body_returns_empty_text("#\n", "")]
    #[case::comment_with_hash_in_body_preserves_inner_hash("# foo # bar\n", " foo # bar")]
    #[case::unicode_body_text_is_slice_of_input("# 日本語\n", " 日本語")]
    fn happy_path_text(#[case] input: &str, #[case] expected: &str) {
        let mut lex = make_lexer(input);
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, expected);
    }

    // -----------------------------------------------------------------------
    // Group C — span correctness
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::span_start_byte_offset_at_hash("# comment\n", 0, 0, 1)]
    #[case::span_start_column_at_hash_after_leading_spaces("   # comment\n", 3, 3, 1)]
    fn span_start(
        #[case] input: &str,
        #[case] expected_byte_offset: u32,
        #[case] expected_column: u32,
        #[case] expected_line: u32,
    ) {
        let mut lex = make_lexer(input);
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        let idx = crate::pos::LineIndex::new(input);
        assert_eq!(span.start, expected_byte_offset);
        let (line, col) = idx.line_column(span.start);
        assert_eq!(col, expected_column);
        assert_eq!(line, expected_line);
    }

    #[rstest]
    // "# abc\n": # (1) + space (1) + abc (3) = 5 bytes before newline; columns: # = 0, space = 1, a = 2, b = 3, c = 4; end = 5
    #[case::span_end_byte_offset_past_last_char("# abc\n", 5, 5)]
    // "# 日\n": # (1) + space (1) + 日 (3) = 5 bytes; columns: # = 0, space = 1, 日 = 2, end = 3
    #[case::span_end_byte_offset_for_multibyte_body("# 日\n", 5, 3)]
    fn span_end(
        #[case] input: &str,
        #[case] expected_byte_offset: u32,
        #[case] expected_column: u32,
    ) {
        let mut lex = make_lexer(input);
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        let idx = crate::pos::LineIndex::new(input);
        assert_eq!(span.end, expected_byte_offset);
        let (_, col) = idx.line_column(span.end);
        assert_eq!(col, expected_column);
    }

    #[test]
    fn span_for_empty_comment_body() {
        let mut lex = make_lexer("#\n");
        let Ok(Some((_, span))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 1);
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

    #[rstest]
    // "# ab\n" -> text " ab" = 3 bytes; limit = 3
    #[case::comment_within_limit_returns_ok("# ab\n", 3, " ab")]
    // "# abc\n" -> text " abc" = 4 bytes; limit = 4 (boundary inclusive)
    #[case::comment_exactly_at_limit_returns_ok("# abc\n", 4, " abc")]
    fn comment_len_ok(#[case] input: &str, #[case] limit: usize, #[case] expected_text: &str) {
        let mut lex = make_lexer(input);
        let Ok(Some((text, _))) = lex.try_consume_comment(limit) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, expected_text);
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
    // Group CM-NP: comment body c-printable rejection
    // -----------------------------------------------------------------------

    #[test]
    fn comment_rejects_nul_in_body() {
        let mut lex = make_lexer("# hello\x00world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("non-printable") || err.message.contains("U+0000"),
            "expected non-printable error for NUL, got: {}",
            err.message
        );
    }

    #[test]
    fn comment_rejects_0x01_in_body() {
        let mut lex = make_lexer("# hello\x01world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("non-printable") || err.message.contains("U+0001"),
            "expected non-printable error for SOH, got: {}",
            err.message
        );
    }

    #[test]
    fn comment_rejects_del_0x7f_in_body() {
        let mut lex = make_lexer("# hello\x7fworld\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("non-printable") || err.message.contains("U+007F"),
            "expected non-printable error for DEL, got: {}",
            err.message
        );
    }

    #[test]
    fn comment_rejects_c1_control_0x80_in_body() {
        let mut lex = make_lexer("# hello\u{0080}world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("non-printable") || err.message.contains("U+0080"),
            "expected non-printable error for U+0080, got: {}",
            err.message
        );
    }

    #[test]
    fn comment_rejects_0xfffe_in_body() {
        let mut lex = make_lexer("# hello\u{FFFE}world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("non-printable") || err.message.contains("U+FFFE"),
            "expected non-printable error for U+FFFE, got: {}",
            err.message
        );
    }

    #[test]
    fn comment_rejects_0xffff_in_body() {
        let mut lex = make_lexer("# hello\u{FFFF}world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("non-printable") || err.message.contains("U+FFFF"),
            "expected non-printable error for U+FFFF, got: {}",
            err.message
        );
    }

    #[test]
    fn comment_non_printable_as_first_body_char() {
        // Non-printable as the very first character of the comment body.
        let mut lex = make_lexer("#\x07hello\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("non-printable") || err.message.contains("U+0007"),
            "expected non-printable error for BEL as first body char, got: {}",
            err.message
        );
    }

    #[test]
    fn comment_error_message_contains_uplus_hex() {
        let mut lex = make_lexer("# hello\x07world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert!(
            err.message.contains("U+0007"),
            "error message must contain U+0007, got: {}",
            err.message
        );
    }

    #[test]
    fn trailing_comment_non_printable_in_body_produces_error() {
        // Trailing comment on a content line: the non-printable must be caught
        // when the comment is parsed (exercise through parse_events).
        let events: Vec<_> = crate::parse_events("key: value # comment\x07here\n").collect();
        let has_non_printable_error = events
            .iter()
            .any(|r| r.as_ref().err().is_some_and(|e| e.message.contains("non-printable")));
        assert!(has_non_printable_error, "expected non-printable error for BEL in trailing comment");
    }

    // -----------------------------------------------------------------------
    // Group CM-OK: comment body c-printable acceptance
    // -----------------------------------------------------------------------

    #[test]
    fn comment_accepts_tab_in_body() {
        // TAB (U+0009) is allowed by c-printable.
        let mut lex = make_lexer("# col1\tcol2\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert!(text.contains('\t'), "TAB must be accepted in comment body");
    }

    #[test]
    fn comment_accepts_nel_0x85() {
        // NEL (U+0085) is allowed by c-printable; in a single-line comment it
        // acts as a line terminator, so the body is truncated before it. No
        // non-printable error should be emitted.
        let mut lex = make_lexer("# val\u{0085}ue\n");
        if let Err(e) = lex.try_consume_comment(1024) {
            assert!(
                !e.message.contains("non-printable"),
                "NEL must not be rejected as non-printable in comment, got: {}",
                e.message
            );
        }
    }

    #[test]
    fn comment_body_empty_no_error() {
        // Empty comment body (just `#` with nothing after it) is always valid.
        let mut lex = make_lexer("#\n");
        let Ok(Some((text, _))) = lex.try_consume_comment(1024) else {
            unreachable!("expected Ok(Some(...))")
        };
        assert_eq!(text, "", "empty comment body must produce empty text slice");
    }
}
