// SPDX-License-Identifier: MIT

use memchr::memchr;

use crate::chars::{find_non_c_printable, non_printable_error_message};
use crate::error::Error;
use crate::pos::{Pos, Span, advance_within_line};

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
            return Err(Error::syntax(
                hash_pos,
                format!("comment exceeds maximum allowed length ({max_comment_len} bytes)"),
            ));
        }

        // Validate c-printable on the comment body.
        if !self.input_all_printable
            && let Some((bad_i, bad_ch)) = find_non_c_printable(text.as_bytes())
        {
            let bad_char_count = text[..bad_i].chars().count();
            let bad_pos = Pos {
                byte_offset: hash_pos.byte_offset + 1 + bad_i,
                line: hash_pos.line,
                column: hash_pos.column + 1 + bad_char_count,
            };
            return Err(Error::invalid_character(
                bad_pos,
                non_printable_error_message(bad_ch, "comment"),
            ));
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

    /// Read the comment, if any, from the rest of a line after a node.
    ///
    /// `suffix` is the text that follows the node on its line and
    /// `suffix_start` is the position of its first character.  A comment
    /// starts at the first `#` that opens the suffix or follows a space or
    /// tab (YAML 1.2.2 [75] `c-nb-comment-text`, [77] `s-b-comment`).
    ///
    /// Returns `Ok(Some((text, span)))` with the body after the `#` and the
    /// span from the `#` through the end of the text, `Ok(None)` when the
    /// suffix holds no comment, and `Err` when the body holds a character
    /// that is not `c-printable`.  The caller decides what to do with the
    /// result; this function changes no lexer state.
    pub fn comment_in_suffix(
        &self,
        suffix: &'input str,
        suffix_start: Pos,
    ) -> Result<Option<(&'input str, Span)>, Error> {
        let Some(hash_index) = comment_hash_index(suffix) else {
            return Ok(None);
        };
        // `#` is ASCII, so both slice boundaries are on a char boundary.
        let text = &suffix[hash_index + 1..];
        let hash_pos = advance_within_line(suffix_start, &suffix[..hash_index]);

        // NUL keeps its specific message; the general c-printable check is
        // skipped when the whole input is known to be printable.
        let bad = memchr(b'\0', text.as_bytes())
            .map(|i| (i, '\0'))
            .or_else(|| {
                if self.input_all_printable {
                    None
                } else {
                    find_non_c_printable(text.as_bytes())
                }
            });
        if let Some((bad_i, bad_ch)) = bad {
            let bad_char_count = text[..bad_i].chars().count();
            let bad_pos = Pos {
                byte_offset: hash_pos.byte_offset + 1 + bad_i,
                line: hash_pos.line,
                column: hash_pos.column + 1 + bad_char_count,
            };
            let message = if bad_ch == '\0' {
                "invalid character U+0000 in comment".to_owned()
            } else {
                non_printable_error_message(bad_ch, "comment")
            };
            return Err(Error::invalid_character(bad_pos, message));
        }

        let span_end = advance_within_line(hash_pos.advance('#'), text);
        Ok(Some((text, Span::from_pos(hash_pos, span_end))))
    }
}

/// Byte offset of the `#` that starts a comment in `suffix`: the first `#`
/// at offset 0 (the node ended just before it) or after a space or tab.
fn comment_hash_index(suffix: &str) -> Option<usize> {
    let bytes = suffix.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = memchr(b'#', bytes.get(search_from..).unwrap_or_default()) {
        let i = search_from + rel;
        if i == 0 || matches!(bytes.get(i - 1), Some(b' ' | b'\t')) {
            return Some(i);
        }
        search_from = i + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::error::ErrorKind;

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
        assert_eq!(
            err.kind,
            ErrorKind::InvalidCharacter,
            "expected InvalidCharacter for NUL, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn comment_rejects_0x01_in_body() {
        let mut lex = make_lexer("# hello\x01world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert_eq!(
            err.kind,
            ErrorKind::InvalidCharacter,
            "expected InvalidCharacter for SOH, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn comment_rejects_del_0x7f_in_body() {
        let mut lex = make_lexer("# hello\x7fworld\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert_eq!(
            err.kind,
            ErrorKind::InvalidCharacter,
            "expected InvalidCharacter for DEL, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn comment_rejects_c1_control_0x80_in_body() {
        let mut lex = make_lexer("# hello\u{0080}world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert_eq!(
            err.kind,
            ErrorKind::InvalidCharacter,
            "expected InvalidCharacter for U+0080, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn comment_rejects_0xfffe_in_body() {
        let mut lex = make_lexer("# hello\u{FFFE}world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert_eq!(
            err.kind,
            ErrorKind::InvalidCharacter,
            "expected InvalidCharacter for U+FFFE, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn comment_rejects_0xffff_in_body() {
        let mut lex = make_lexer("# hello\u{FFFF}world\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert_eq!(
            err.kind,
            ErrorKind::InvalidCharacter,
            "expected InvalidCharacter for U+FFFF, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn comment_non_printable_as_first_body_char() {
        // Non-printable as the very first character of the comment body.
        let mut lex = make_lexer("#\x07hello\n");
        let Err(err) = lex.try_consume_comment(1024) else {
            unreachable!("expected Err, got Ok")
        };
        assert_eq!(
            err.kind,
            ErrorKind::InvalidCharacter,
            "expected InvalidCharacter for BEL as first body char, got: {:?}",
            err.kind
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
        let has_invalid_character_error = events.iter().any(|r| {
            r.as_ref()
                .err()
                .is_some_and(|e| e.kind == ErrorKind::InvalidCharacter)
        });
        assert!(
            has_invalid_character_error,
            "expected InvalidCharacter error for BEL in trailing comment"
        );
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
        // InvalidCharacter error should be emitted.
        let mut lex = make_lexer("# val\u{0085}ue\n");
        if let Err(e) = lex.try_consume_comment(1024) {
            assert_ne!(
                e.kind,
                ErrorKind::InvalidCharacter,
                "NEL must not be rejected as InvalidCharacter in comment, got: {:?}",
                e.kind
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

    // -----------------------------------------------------------------------
    // Group S — comment_in_suffix
    // -----------------------------------------------------------------------

    /// Position of the first character of a suffix that starts mid-line.
    const SUFFIX_START: Pos = Pos {
        byte_offset: 10,
        line: 3,
        column: 7,
    };

    fn pos(byte_offset: usize, column: usize) -> Pos {
        Pos {
            byte_offset,
            line: 3,
            column,
        }
    }

    #[rstest]
    #[case::empty("")]
    #[case::spaces("   ")]
    #[case::tab("\t")]
    #[case::word("abc")]
    #[case::indented_word("  abc")]
    #[case::hash_after_non_whitespace("a#b")]
    #[case::trailing_hash_after_non_whitespace("x#")]
    #[case::hash_after_quote("'#")]
    #[case::hash_after_multibyte_char("é#x")]
    fn suffix_without_comment_returns_none(#[case] suffix: &str) {
        let lex = make_lexer(suffix);
        assert_eq!(lex.comment_in_suffix(suffix, SUFFIX_START), Ok(None));
    }

    // Each case: suffix, expected body, expected `#` byte offset in the line,
    // `#` column, end byte offset, end column.  Columns count characters,
    // byte offsets count bytes; the suffix starts at byte 10, column 7.
    #[rstest]
    #[case::hash_at_offset_zero("#x", "x", 10, 7, 12, 9)]
    #[case::after_spaces("  # c", " c", 12, 9, 15, 12)]
    #[case::after_tab("\t# c", " c", 11, 8, 14, 11)]
    #[case::empty_body(" #", "", 11, 8, 12, 9)]
    #[case::hash_inside_body_is_text("  # a # b", " a # b", 12, 9, 19, 16)]
    #[case::unspaced_hash_is_skipped("a#b # c", " c", 14, 11, 17, 14)]
    #[case::multibyte_prefix_counts_chars("  日本 # c", " c", 19, 12, 22, 15)]
    #[case::multibyte_after_hash_counts_chars("# 日本語", " 日本語", 10, 7, 21, 12)]
    #[case::multibyte_before_hash_without_space_is_skipped("é#x #y", "y", 15, 11, 17, 13)]
    #[case::tab_and_nel_are_printable("# a\tb\u{85}c", " a\tb\u{85}c", 10, 7, 18, 14)]
    #[case::bom_in_body_is_printable("# a\u{FEFF}b", " a\u{FEFF}b", 10, 7, 17, 13)]
    fn suffix_with_comment_returns_body_and_span(
        #[case] suffix: &str,
        #[case] text: &str,
        #[case] start_byte: usize,
        #[case] start_column: usize,
        #[case] end_byte: usize,
        #[case] end_column: usize,
    ) {
        let lex = make_lexer(suffix);
        let Ok(Some((got_text, span))) = lex.comment_in_suffix(suffix, SUFFIX_START) else {
            unreachable!("expected Ok(Some(...)) for {suffix:?}")
        };
        assert_eq!(got_text, text);
        assert_eq!(
            span,
            Span::from_pos(pos(start_byte, start_column), pos(end_byte, end_column))
        );
    }

    // Each case: suffix, expected error byte offset, error column.  Character
    // classes match `find_non_c_printable`; NUL has its own message.
    #[rstest]
    #[case::nul_in_body(" # a\0b", "invalid character U+0000 in comment", 14, 11)]
    #[case::nul_first_in_body("#\0", "invalid character U+0000 in comment", 11, 8)]
    #[case::nul_after_multibyte(" # 日本\0", "invalid character U+0000 in comment", 19, 12)]
    #[case::nul_wins_over_earlier_control_char(
        "# \x01\0",
        "invalid character U+0000 in comment",
        13,
        10
    )]
    #[case::c0_control(
        "# a\x01",
        "non-printable character U+0001 is not allowed in comment",
        13,
        10
    )]
    #[case::bel(
        "# a\x07",
        "non-printable character U+0007 is not allowed in comment",
        13,
        10
    )]
    #[case::del(
        "# a\x7f",
        "non-printable character U+007F is not allowed in comment",
        13,
        10
    )]
    #[case::c1_control(
        "# a\u{80}",
        "non-printable character U+0080 is not allowed in comment",
        13,
        10
    )]
    #[case::fffe(
        "# a\u{FFFE}",
        "non-printable character U+FFFE is not allowed in comment",
        13,
        10
    )]
    #[case::ffff(
        "# a\u{FFFF}",
        "non-printable character U+FFFF is not allowed in comment",
        13,
        10
    )]
    #[case::control_after_multibyte(
        "  # 日本\x01",
        "non-printable character U+0001 is not allowed in comment",
        20,
        13
    )]
    fn suffix_comment_with_invalid_character_is_an_error(
        #[case] suffix: &str,
        #[case] message: &str,
        #[case] byte_offset: usize,
        #[case] column: usize,
    ) {
        let lex = make_lexer(suffix);
        let Err(err) = lex.comment_in_suffix(suffix, SUFFIX_START) else {
            unreachable!("expected Err for {suffix:?}")
        };
        assert_eq!(err.kind, ErrorKind::InvalidCharacter);
        assert_eq!(err.message, message);
        assert_eq!(err.pos, pos(byte_offset, column));
    }

    // `input_all_printable` is the lexer's fast-path proof that no character
    // needs checking; it skips the c-printable scan but never the NUL check.
    #[rstest]
    #[case::c0_control("# a\x01")]
    #[case::del("# a\x7f")]
    #[case::fffe("# a\u{FFFE}")]
    fn suffix_comment_skips_c_printable_scan_when_input_is_all_printable(#[case] suffix: &str) {
        let mut lex = make_lexer(suffix);
        lex.input_all_printable = true;
        assert!(matches!(
            lex.comment_in_suffix(suffix, SUFFIX_START),
            Ok(Some(_))
        ));
    }

    #[test]
    fn suffix_comment_rejects_nul_even_when_input_is_all_printable() {
        let mut lex = make_lexer("# a\0");
        lex.input_all_printable = true;
        assert!(lex.comment_in_suffix("# a\0", SUFFIX_START).is_err());
    }
}
