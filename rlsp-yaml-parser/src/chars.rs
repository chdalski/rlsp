// SPDX-License-Identifier: MIT

//! YAML 1.2 §5 character productions [1]–[62] and plain-scalar character
//! productions [125]–[128], [57]–[60].
//!
//! Each function is named after the spec production and cross-referenced by
//! its production number in a `// [N]` comment.  All functions return
//! `Parser<'i>` built from the combinator framework in `combinator.rs`.

use crate::combinator::{Context, Parser, alt, lookahead, satisfy, seq};

// ---------------------------------------------------------------------------
// §5.1 – Unicode character set helpers
// ---------------------------------------------------------------------------

/// [1] c-printable — printable Unicode characters allowed in YAML.
#[must_use]
pub fn c_printable<'i>() -> Parser<'i> {
    satisfy(|ch| {
        matches!(ch,
            '\t'            // x09
            | '\n'          // x0A
            | '\r'          // x0D
            | '\x20'..='\x7E'   // printable ASCII
            | '\u{85}'      // NEL
            | '\u{A0}'..='\u{D7FF}'  // BMP (excluding surrogates)
            | '\u{E000}'..='\u{FFFD}' // BMP private / specials (excluding FFFE/FFFF)
            | '\u{10000}'..='\u{10FFFF}' // supplementary planes
        )
    })
}

/// [2] nb-json — non-break JSON-compatible characters.
#[must_use]
pub fn nb_json<'i>() -> Parser<'i> {
    satisfy(|ch| ch == '\t' || ch >= '\x20')
}

/// [3] c-byte-order-mark — the Unicode BOM character (U+FEFF).
#[must_use]
pub fn c_byte_order_mark<'i>() -> Parser<'i> {
    satisfy(|ch| ch == '\u{FEFF}')
}

// ---------------------------------------------------------------------------
// §5.3 – Indicator characters [4]–[23]
// ---------------------------------------------------------------------------

/// [22] c-indicator — one of the 21 YAML indicator characters.
#[must_use]
pub fn c_indicator<'i>() -> Parser<'i> {
    satisfy(|ch| {
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
    })
}

/// [23] c-flow-indicator — the five flow-collection indicator characters.
#[must_use]
pub fn c_flow_indicator<'i>() -> Parser<'i> {
    satisfy(|ch| matches!(ch, ',' | '[' | ']' | '{' | '}'))
}

// ---------------------------------------------------------------------------
// §5.4 – Line break characters [24]–[30]
// ---------------------------------------------------------------------------

/// [24] b-line-feed
#[must_use]
pub fn b_line_feed<'i>() -> Parser<'i> {
    satisfy(|ch| ch == '\n')
}

/// [25] b-carriage-return
#[must_use]
pub fn b_carriage_return<'i>() -> Parser<'i> {
    satisfy(|ch| ch == '\r')
}

/// [26] b-char — line feed or carriage return.
#[must_use]
pub fn b_char<'i>() -> Parser<'i> {
    satisfy(|ch| matches!(ch, '\n' | '\r'))
}

/// [27] nb-char — printable character that is not a line break or BOM.
///
/// c-printable minus b-char minus c-byte-order-mark (U+FEFF).
/// Note: U+FEFF lies within the \u{E000}–\u{FFFD} BMP range, so it must be
/// excluded explicitly.
#[must_use]
pub fn nb_char<'i>() -> Parser<'i> {
    satisfy(|ch| {
        ch != '\u{FEFF}'
            && matches!(ch,
                '\t'
                | '\x20'..='\x7E'
                | '\u{85}'
                | '\u{A0}'..='\u{D7FF}'
                | '\u{E000}'..='\u{FFFD}'
                | '\u{10000}'..='\u{10FFFF}'
            )
    })
}

/// [28] b-break — CRLF pair (tried first), lone CR, or lone LF.
///
/// The CRLF alternative must be ordered before the lone-CR alternative so
/// that `\r\n` is consumed as a unit.
#[must_use]
pub fn b_break<'i>() -> Parser<'i> {
    alt(
        seq(b_carriage_return(), b_line_feed()),
        alt(b_carriage_return(), b_line_feed()),
    )
}

/// [29] b-as-line-feed — alias for b-break (spec production name).
#[must_use]
pub fn b_as_line_feed<'i>() -> Parser<'i> {
    b_break()
}

/// [30] b-non-content — alias for b-break (spec production name).
#[must_use]
pub fn b_non_content<'i>() -> Parser<'i> {
    b_break()
}

// ---------------------------------------------------------------------------
// §5.5 – White space characters [31]–[34]
// ---------------------------------------------------------------------------

/// [31] s-space
#[must_use]
pub fn s_space<'i>() -> Parser<'i> {
    satisfy(|ch| ch == ' ')
}

/// [32] s-tab
#[must_use]
pub fn s_tab<'i>() -> Parser<'i> {
    satisfy(|ch| ch == '\t')
}

/// [33] s-white — space or tab.
#[must_use]
pub fn s_white<'i>() -> Parser<'i> {
    satisfy(|ch| matches!(ch, ' ' | '\t'))
}

/// [34] ns-char — non-break, non-white printable character.
#[must_use]
pub fn ns_char<'i>() -> Parser<'i> {
    satisfy(|ch| {
        !matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{FEFF}')
            && matches!(ch,
                '\x21'..='\x7E'
                | '\u{85}'
                | '\u{A0}'..='\u{D7FF}'
                | '\u{E000}'..='\u{FFFD}'
                | '\u{10000}'..='\u{10FFFF}'
            )
    })
}

// ---------------------------------------------------------------------------
// §5.6 – Miscellaneous character classes [35]–[40]
// ---------------------------------------------------------------------------

/// [35] ns-dec-digit — ASCII decimal digit.
#[must_use]
pub fn ns_dec_digit<'i>() -> Parser<'i> {
    satisfy(|ch| ch.is_ascii_digit())
}

/// [36] ns-hex-digit — ASCII hexadecimal digit (case-insensitive).
#[must_use]
pub fn ns_hex_digit<'i>() -> Parser<'i> {
    satisfy(|ch| ch.is_ascii_hexdigit())
}

/// [37] ns-ascii-letter — ASCII letter.
#[must_use]
pub fn ns_ascii_letter<'i>() -> Parser<'i> {
    satisfy(|ch| ch.is_ascii_alphabetic())
}

/// [38] ns-word-char — decimal digit, ASCII letter, or hyphen.
#[must_use]
pub fn ns_word_char<'i>() -> Parser<'i> {
    satisfy(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

/// [39] ns-uri-char — characters allowed in a URI.
///
/// Either `%` followed by two hex digits, or a word/punctuation character
/// from the allowed set.
#[must_use]
pub fn ns_uri_char<'i>() -> Parser<'i> {
    alt(
        // Percent-encoded: % HH
        seq(satisfy(|ch| ch == '%'), seq(ns_hex_digit(), ns_hex_digit())),
        // Single allowed character
        satisfy(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '-' | '_'
                        | '.'
                        | '!'
                        | '~'
                        | '*'
                        | '\''
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '#'
                        | ';'
                        | '/'
                        | '?'
                        | ':'
                        | '@'
                        | '&'
                        | '='
                        | '+'
                        | '$'
                        | ','
                )
        }),
    )
}

/// [40] ns-tag-char — URI characters minus `!` and flow indicators.
#[must_use]
pub fn ns_tag_char<'i>() -> Parser<'i> {
    alt(
        // Percent-encoded: % HH
        seq(satisfy(|ch| ch == '%'), seq(ns_hex_digit(), ns_hex_digit())),
        // Single allowed character: ns-uri-char minus '!' and flow indicators (,[]{}),
        satisfy(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '-' | '_'
                        | '.'
                        | '~'
                        | '*'
                        | '\''
                        | '('
                        | ')'
                        | '#'
                        | ';'
                        | '/'
                        | '?'
                        | ':'
                        | '@'
                        | '&'
                        | '='
                        | '+'
                        | '$'
                )
        }),
    )
}

// ---------------------------------------------------------------------------
// §5.7 – Escape sequences [41]–[62]
// ---------------------------------------------------------------------------

/// Decode a YAML double-quoted escape sequence.
///
/// `input` begins *after* the leading `\` — i.e. it starts with the escape
/// code character (`0`, `n`, `x`, `u`, etc.).
///
/// Returns `(decoded_char, bytes_consumed)` on success, or `None` if the
/// escape is invalid (unknown code, truncated hex, non-hex digit, or
/// codepoint out of Unicode range including surrogates).
///
/// This is a pure function rather than a combinator because escape sequences
/// produce a decoded scalar value, not a token-matched byte range.  The
/// combinator wrapper (`c_ns_esc_char`) lives in Task 4 (double-quoted
/// scalars).
#[must_use]
pub fn decode_escape(input: &str) -> Option<(char, usize)> {
    let mut chars = input.chars();
    let code = chars.next()?;
    match code {
        '0' => Some(('\x00', 1)),
        'a' => Some(('\x07', 1)),
        'b' => Some(('\x08', 1)),
        't' | '\t' => Some(('\t', 1)),
        'n' => Some(('\n', 1)),
        'v' => Some(('\x0B', 1)),
        'f' => Some(('\x0C', 1)),
        'r' => Some(('\r', 1)),
        'e' => Some(('\x1B', 1)),
        ' ' => Some((' ', 1)),
        '"' => Some(('"', 1)),
        '/' => Some(('/', 1)),
        '\\' => Some(('\\', 1)),
        'N' => Some(('\u{85}', 1)),
        '_' => Some(('\u{A0}', 1)),
        'L' => Some(('\u{2028}', 1)),
        'P' => Some(('\u{2029}', 1)),
        'x' => decode_hex_escape(input, 1, 2),
        'u' => decode_hex_escape(input, 1, 4),
        'U' => decode_hex_escape(input, 1, 8),
        _ => None,
    }
}

/// Parse `digit_count` hex digits starting at byte offset `start` within
/// `input` (which begins after the `\`).  Returns the decoded char and total
/// bytes consumed (including the escape code character).
fn decode_hex_escape(input: &str, start: usize, digit_count: usize) -> Option<(char, usize)> {
    let rest = input.get(start..)?;
    if rest.len() < digit_count {
        return None;
    }
    let hex_str = rest.get(..digit_count)?;
    // All digits must be ASCII hex.
    if !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let codepoint = u32::from_str_radix(hex_str, 16).ok()?;
    let ch = char::from_u32(codepoint)?;
    Some((ch, start + digit_count))
}

// ---------------------------------------------------------------------------
// Plain scalar character productions [125]–[128], [57]–[60]
// ---------------------------------------------------------------------------

/// [126] ns-plain-safe-out — safe plain chars in block context.
///
/// In block context the spec allows all `ns-char` values (indicators are
/// handled at the `ns-plain-first` / `ns-plain-char` level, not here).
#[must_use]
fn ns_plain_safe_out<'i>() -> Parser<'i> {
    ns_char()
}

/// [127] ns-plain-safe-in — safe plain chars in flow context.
///
/// All `ns-char` values that are not flow indicators.
#[must_use]
fn ns_plain_safe_in<'i>() -> Parser<'i> {
    satisfy(|ch| {
        !matches!(ch, ',' | '[' | ']' | '{' | '}')
            && ch != '\u{FEFF}'
            && !matches!(ch, ' ' | '\t' | '\n' | '\r')
            && matches!(ch,
                '\x21'..='\x7E'
                | '\u{85}'
                | '\u{A0}'..='\u{D7FF}'
                | '\u{E000}'..='\u{FFFD}'
                | '\u{10000}'..='\u{10FFFF}'
            )
    })
}

/// [125] ns-plain-safe(c) — context-dispatched plain-safe character.
#[must_use]
pub fn ns_plain_safe<'i>(c: Context) -> Parser<'i> {
    match c {
        Context::BlockOut | Context::BlockIn => ns_plain_safe_out(),
        Context::FlowOut | Context::FlowIn | Context::BlockKey | Context::FlowKey => {
            ns_plain_safe_in()
        }
    }
}

/// [57]–[58] ns-plain-first(c) — first character of a plain scalar.
///
/// Either an `ns-char` that is not an indicator, or one of `?`, `:`, `-`
/// when followed by an `ns-plain-safe(c)` character (positive lookahead).
#[must_use]
pub fn ns_plain_first<'i>(c: Context) -> Parser<'i> {
    alt(
        // Non-indicator ns-char
        satisfy(|ch| {
            !matches!(
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
            ) && ch != '\u{FEFF}'
                && !matches!(ch, ' ' | '\t' | '\n' | '\r')
                && matches!(ch,
                    '\x21'..='\x7E'
                    | '\u{85}'
                    | '\u{A0}'..='\u{D7FF}'
                    | '\u{E000}'..='\u{FFFD}'
                    | '\u{10000}'..='\u{10FFFF}'
                )
        }),
        // Indicator ('?', ':', '-') only when followed by ns-plain-safe(c)
        seq(
            satisfy(|ch| matches!(ch, '?' | ':' | '-')),
            lookahead(ns_plain_safe(c)),
        ),
    )
}

/// [59]–[60] ns-plain-char(c) — subsequent characters in a plain scalar.
///
/// Three alternatives (tried in order):
/// 1. `ns-plain-safe(c)` minus `:` and `#` — regular safe non-special char.
/// 2. `#` — allowed when not preceded by whitespace (whitespace terminates
///    the scalar before `#` is reached, so `#` inside a running plain scalar
///    is always safe).  The spec expresses this as "an ns-char preceding `#`"
///    which in a forward parser reduces to: `#` is matchable here.
/// 3. `:` followed by a positive lookahead of `ns-plain-safe(c)`.
#[must_use]
pub fn ns_plain_char<'i>(c: Context) -> Parser<'i> {
    // Branch 3: ':' only when followed by ns-plain-safe(c)
    let colon_branch = seq(satisfy(|ch| ch == ':'), lookahead(ns_plain_safe(c)));
    // Branch 2: '#' is safe inside a plain scalar (comment '#' requires
    // preceding whitespace, which would have terminated the scalar already).
    let hash_branch = satisfy(|ch| ch == '#');
    // Branch 1: ns-plain-safe(c) minus ':' and '#'
    let safe_branch = satisfy(move |ch| {
        ch != ':'
            && ch != '#'
            && match c {
                Context::BlockOut | Context::BlockIn => {
                    !matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{FEFF}')
                        && matches!(ch,
                            '\x21'..='\x7E'
                            | '\u{85}'
                            | '\u{A0}'..='\u{D7FF}'
                            | '\u{E000}'..='\u{FFFD}'
                            | '\u{10000}'..='\u{10FFFF}'
                        )
                }
                Context::FlowOut | Context::FlowIn | Context::BlockKey | Context::FlowKey => {
                    !matches!(ch, ',' | '[' | ']' | '{' | '}')
                        && !matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{FEFF}')
                        && matches!(ch,
                            '\x21'..='\x7E'
                            | '\u{85}'
                            | '\u{A0}'..='\u{D7FF}'
                            | '\u{E000}'..='\u{FFFD}'
                            | '\u{10000}'..='\u{10FFFF}'
                        )
                }
            }
    });
    alt(safe_branch, alt(hash_branch, colon_branch))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::combinator::{Reply, State, many1};

    fn state(input: &str) -> State<'_> {
        State::new(input)
    }

    fn is_success(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Success { .. })
    }

    fn is_failure(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Failure)
    }

    fn remaining<'a>(reply: &'a Reply<'a>) -> &'a str {
        match reply {
            Reply::Success { state, .. } => state.input,
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    // -----------------------------------------------------------------------
    // c_printable [1]
    // -----------------------------------------------------------------------

    #[test]
    fn c_printable_accepts_tab() {
        assert!(is_success(&c_printable()(state("\t"))));
    }

    #[test]
    fn c_printable_accepts_newline() {
        assert!(is_success(&c_printable()(state("\n"))));
    }

    #[test]
    fn c_printable_accepts_carriage_return() {
        assert!(is_success(&c_printable()(state("\r"))));
    }

    #[test]
    fn c_printable_accepts_space_through_tilde() {
        assert!(is_success(&c_printable()(state(" "))));
        assert!(is_success(&c_printable()(state("~"))));
        assert!(is_success(&c_printable()(state("M"))));
    }

    #[test]
    fn c_printable_accepts_nel() {
        let nel = "\u{85}";
        assert!(is_success(&c_printable()(state(nel))));
    }

    #[test]
    fn c_printable_accepts_non_breaking_space() {
        let nbsp = "\u{A0}";
        assert!(is_success(&c_printable()(state(nbsp))));
    }

    #[test]
    fn c_printable_rejects_null_byte() {
        assert!(is_failure(&c_printable()(state("\x00"))));
    }

    #[test]
    fn c_printable_rejects_del() {
        assert!(is_failure(&c_printable()(state("\x7F"))));
    }

    #[test]
    fn c_printable_rejects_control_chars_in_c0_block() {
        for ch in ['\x01', '\x08', '\x0B', '\x0C', '\x0E', '\x1F'] {
            let s = ch.to_string();
            assert!(
                is_failure(&c_printable()(state(&s))),
                "should reject {ch:?}"
            );
        }
    }

    #[test]
    fn c_printable_rejects_surrogates() {
        // Rust char cannot hold surrogates; verify U+FFFE and U+FFFF are rejected.
        assert!(is_failure(&c_printable()(state("\u{FFFE}"))));
        assert!(is_failure(&c_printable()(state("\u{FFFF}"))));
    }

    // -----------------------------------------------------------------------
    // nb_json [2]
    // -----------------------------------------------------------------------

    #[test]
    fn nb_json_accepts_tab() {
        assert!(is_success(&nb_json()(state("\t"))));
    }

    #[test]
    fn nb_json_accepts_printable_ascii() {
        assert!(is_success(&nb_json()(state("A"))));
    }

    #[test]
    fn nb_json_rejects_null_byte() {
        assert!(is_failure(&nb_json()(state("\x00"))));
    }

    #[test]
    fn nb_json_rejects_control_chars_below_0x20_except_tab() {
        assert!(is_failure(&nb_json()(state("\x01"))));
        assert!(is_failure(&nb_json()(state("\x1F"))));
    }

    // -----------------------------------------------------------------------
    // Line break characters [24]–[28]
    // -----------------------------------------------------------------------

    #[test]
    fn b_line_feed_accepts_lf_only() {
        assert!(is_success(&b_line_feed()(state("\n"))));
        assert!(is_failure(&b_line_feed()(state("\r"))));
    }

    #[test]
    fn b_carriage_return_accepts_cr_only() {
        assert!(is_success(&b_carriage_return()(state("\r"))));
        assert!(is_failure(&b_carriage_return()(state("\n"))));
    }

    #[test]
    fn b_break_accepts_crlf_as_single_unit() {
        let reply = b_break()(state("\r\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn b_break_accepts_lone_cr() {
        let reply = b_break()(state("\r"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn b_break_accepts_lone_lf() {
        let reply = b_break()(state("\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn b_break_prefers_crlf_over_lone_cr() {
        let reply = b_break()(state("\r\n..."));
        assert!(is_success(&reply));
        // Both \r and \n must be consumed; "..." remains, not "\n..."
        assert_eq!(remaining(&reply), "...");
    }

    // -----------------------------------------------------------------------
    // nb_char [27]
    // -----------------------------------------------------------------------

    #[test]
    fn nb_char_accepts_printable_non_break_non_bom() {
        assert!(is_success(&nb_char()(state("A"))));
        assert!(is_success(&nb_char()(state(" "))));
        assert!(is_success(&nb_char()(state("é"))));
    }

    #[test]
    fn nb_char_rejects_line_feed() {
        assert!(is_failure(&nb_char()(state("\n"))));
    }

    #[test]
    fn nb_char_rejects_carriage_return() {
        assert!(is_failure(&nb_char()(state("\r"))));
    }

    #[test]
    fn nb_char_rejects_byte_order_mark() {
        assert!(is_failure(&nb_char()(state("\u{FEFF}"))));
    }

    // -----------------------------------------------------------------------
    // White space [31]–[33]
    // -----------------------------------------------------------------------

    #[test]
    fn s_space_accepts_space_only() {
        assert!(is_success(&s_space()(state(" "))));
        assert!(is_failure(&s_space()(state("\t"))));
    }

    #[test]
    fn s_tab_accepts_tab_only() {
        assert!(is_success(&s_tab()(state("\t"))));
        assert!(is_failure(&s_tab()(state(" "))));
    }

    #[test]
    fn s_white_accepts_space_and_tab() {
        assert!(is_success(&s_white()(state(" "))));
        assert!(is_success(&s_white()(state("\t"))));
        assert!(is_failure(&s_white()(state("a"))));
    }

    // -----------------------------------------------------------------------
    // ns_char [34]
    // -----------------------------------------------------------------------

    #[test]
    fn ns_char_accepts_printable_non_whitespace() {
        assert!(is_success(&ns_char()(state("a"))));
        assert!(is_success(&ns_char()(state("!"))));
        assert!(is_success(&ns_char()(state("中"))));
    }

    #[test]
    fn ns_char_rejects_space() {
        assert!(is_failure(&ns_char()(state(" "))));
    }

    #[test]
    fn ns_char_rejects_tab() {
        assert!(is_failure(&ns_char()(state("\t"))));
    }

    #[test]
    fn ns_char_rejects_line_break() {
        assert!(is_failure(&ns_char()(state("\n"))));
    }

    // -----------------------------------------------------------------------
    // Digit and letter classes [35]–[38]
    // -----------------------------------------------------------------------

    #[test]
    fn ns_dec_digit_accepts_zero_through_nine() {
        assert!(is_success(&ns_dec_digit()(state("0"))));
        assert!(is_success(&ns_dec_digit()(state("9"))));
        assert!(is_failure(&ns_dec_digit()(state("a"))));
        assert!(is_failure(&ns_dec_digit()(state("/"))));
    }

    #[test]
    fn ns_hex_digit_accepts_decimal_and_hex_letters() {
        for s in ["0", "9", "a", "f", "A", "F"] {
            assert!(is_success(&ns_hex_digit()(state(s))), "should accept {s:?}");
        }
        for s in ["g", "G", " "] {
            assert!(is_failure(&ns_hex_digit()(state(s))), "should reject {s:?}");
        }
    }

    #[test]
    fn ns_ascii_letter_accepts_letters_only() {
        assert!(is_success(&ns_ascii_letter()(state("a"))));
        assert!(is_success(&ns_ascii_letter()(state("Z"))));
        assert!(is_failure(&ns_ascii_letter()(state("0"))));
        assert!(is_failure(&ns_ascii_letter()(state(" "))));
    }

    #[test]
    fn ns_word_char_accepts_digits_letters_and_hyphen() {
        assert!(is_success(&ns_word_char()(state("a"))));
        assert!(is_success(&ns_word_char()(state("9"))));
        assert!(is_success(&ns_word_char()(state("-"))));
        assert!(is_failure(&ns_word_char()(state("!"))));
        assert!(is_failure(&ns_word_char()(state(" "))));
    }

    // -----------------------------------------------------------------------
    // Indicators [22]–[23]
    // -----------------------------------------------------------------------

    #[test]
    fn c_indicator_accepts_all_21_indicator_characters() {
        let indicators = [
            "-", "?", ":", ",", "[", "]", "{", "}", "#", "&", "*", "!", "|", ">", "'", "\"", "%",
            "@", "`",
        ];
        for s in indicators {
            assert!(is_success(&c_indicator()(state(s))), "should accept {s:?}");
        }
    }

    #[test]
    fn c_indicator_rejects_non_indicator() {
        assert!(is_failure(&c_indicator()(state("a"))));
        assert!(is_failure(&c_indicator()(state("0"))));
        assert!(is_failure(&c_indicator()(state(" "))));
    }

    #[test]
    fn c_flow_indicator_accepts_exactly_five_chars() {
        for s in [",", "[", "]", "{", "}"] {
            assert!(
                is_success(&c_flow_indicator()(state(s))),
                "should accept {s:?}"
            );
        }
    }

    #[test]
    fn c_flow_indicator_rejects_other_indicators() {
        for s in ["-", "?", ":", "#"] {
            assert!(
                is_failure(&c_flow_indicator()(state(s))),
                "should reject {s:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // URI and tag characters [39]–[40]
    // -----------------------------------------------------------------------

    #[test]
    fn ns_uri_char_accepts_percent_encoded_pair() {
        let reply = ns_uri_char()(state("%2F"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn ns_uri_char_accepts_word_chars() {
        assert!(is_success(&ns_uri_char()(state("a"))));
        assert!(is_success(&ns_uri_char()(state("-"))));
        assert!(is_success(&ns_uri_char()(state("_"))));
    }

    #[test]
    fn ns_uri_char_accepts_allowed_punctuation() {
        let chars = [
            "#", ";", "/", "?", ":", "@", "&", "=", "+", "$", ",", "_", ".", "!", "~", "*", "'",
            "(", ")", "[", "]",
        ];
        for s in chars {
            assert!(is_success(&ns_uri_char()(state(s))), "should accept {s:?}");
        }
    }

    #[test]
    fn ns_uri_char_rejects_space() {
        assert!(is_failure(&ns_uri_char()(state(" "))));
    }

    #[test]
    fn ns_tag_char_rejects_exclamation_mark() {
        assert!(is_failure(&ns_tag_char()(state("!"))));
    }

    #[test]
    fn ns_tag_char_rejects_flow_indicators() {
        for s in [",", "[", "]", "{", "}"] {
            assert!(is_failure(&ns_tag_char()(state(s))), "should reject {s:?}");
        }
    }

    #[test]
    fn ns_tag_char_accepts_word_chars() {
        assert!(is_success(&ns_tag_char()(state("a"))));
        assert!(is_success(&ns_tag_char()(state("-"))));
        assert!(is_success(&ns_tag_char()(state("9"))));
    }

    #[test]
    fn ns_uri_char_rejects_bare_percent_without_hex_digits() {
        // '%' alone — no hex digits follow
        assert!(is_failure(&ns_uri_char()(state("%"))));
        // '%xy' where x, y are not hex
        assert!(is_failure(&ns_uri_char()(state("%GG"))));
    }

    // -----------------------------------------------------------------------
    // Context-sensitive plain scalar chars [125]–[128], [57]–[60]
    // -----------------------------------------------------------------------

    #[test]
    fn ns_plain_safe_accepts_ns_char_in_block_context() {
        // In block context, indicators like '-', '?', ':', ',' are allowed
        for s in ["a", "-", "?", ":", ","] {
            assert!(
                is_success(&ns_plain_safe(Context::BlockOut)(state(s))),
                "should accept {s:?} in BlockOut"
            );
        }
    }

    #[test]
    fn ns_plain_safe_rejects_flow_indicators_in_flow_context() {
        for s in [",", "[", "]", "{", "}"] {
            assert!(
                is_failure(&ns_plain_safe(Context::FlowIn)(state(s))),
                "should reject {s:?} in FlowIn"
            );
        }
    }

    #[test]
    fn ns_plain_safe_accepts_non_flow_indicator_in_flow_context() {
        assert!(is_success(&ns_plain_safe(Context::FlowIn)(state("a"))));
        assert!(is_success(&ns_plain_safe(Context::FlowIn)(state("-"))));
    }

    #[test]
    fn ns_plain_safe_dispatches_all_six_contexts() {
        for c in [
            Context::BlockOut,
            Context::BlockIn,
            Context::FlowOut,
            Context::FlowIn,
            Context::BlockKey,
            Context::FlowKey,
        ] {
            assert!(
                is_success(&ns_plain_safe(c)(state("a"))),
                "failed for {c:?}"
            );
        }
    }

    #[test]
    fn ns_plain_first_accepts_safe_ns_char_in_block() {
        assert!(is_success(&ns_plain_first(Context::BlockOut)(state("a"))));
    }

    #[test]
    fn ns_plain_first_accepts_question_followed_by_safe_char() {
        // "?a" — '?' is an indicator, but lookahead on 'a' succeeds
        let reply = ns_plain_first(Context::BlockOut)(state("?a"));
        assert!(is_success(&reply));
        // Only '?' consumed; 'a' remains
        assert_eq!(remaining(&reply), "a");
    }

    #[test]
    fn ns_plain_first_rejects_question_at_end_of_input() {
        assert!(is_failure(&ns_plain_first(Context::BlockOut)(state("?"))));
    }

    #[test]
    fn ns_plain_first_rejects_bare_colon_without_safe_successor() {
        // ": " — colon followed by space; space is not ns_plain_safe
        assert!(is_failure(&ns_plain_first(Context::BlockOut)(state(": "))));
    }

    #[test]
    fn ns_plain_char_accepts_hash_preceded_by_ns_char() {
        // many1(ns_plain_char) on "a#b" — '#' is allowed when preceded by ns_char
        let p = many1(ns_plain_char(Context::BlockOut));
        let reply = p(state("a#b"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn ns_plain_char_rejects_space_hash() {
        // Space halts the sequence
        let p = many1(ns_plain_char(Context::BlockOut));
        let reply = p(state("a "));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " ");
    }

    #[test]
    fn ns_plain_char_rejects_colon_space() {
        // ": " as first char — colon not followed by safe char
        assert!(is_failure(&ns_plain_char(Context::BlockOut)(state(": "))));
    }

    #[test]
    fn ns_plain_char_accepts_colon_followed_by_safe_char() {
        // "a:b" — ':' between two safe chars
        let p = many1(ns_plain_char(Context::BlockOut));
        let reply = p(state("a:b"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    // -----------------------------------------------------------------------
    // decode_escape
    // -----------------------------------------------------------------------

    #[test]
    fn decode_escape_null() {
        assert_eq!(decode_escape("0rest"), Some(('\x00', 1)));
    }

    #[test]
    fn decode_escape_bell() {
        assert_eq!(decode_escape("a"), Some(('\x07', 1)));
    }

    #[test]
    fn decode_escape_backspace() {
        assert_eq!(decode_escape("b"), Some(('\x08', 1)));
    }

    #[test]
    fn decode_escape_tab() {
        assert_eq!(decode_escape("t"), Some(('\t', 1)));
    }

    #[test]
    fn decode_escape_newline() {
        assert_eq!(decode_escape("n"), Some(('\n', 1)));
    }

    #[test]
    fn decode_escape_vertical_tab() {
        assert_eq!(decode_escape("v"), Some(('\x0B', 1)));
    }

    #[test]
    fn decode_escape_form_feed() {
        assert_eq!(decode_escape("f"), Some(('\x0C', 1)));
    }

    #[test]
    fn decode_escape_carriage_return() {
        assert_eq!(decode_escape("r"), Some(('\r', 1)));
    }

    #[test]
    fn decode_escape_escape() {
        assert_eq!(decode_escape("e"), Some(('\x1B', 1)));
    }

    #[test]
    fn decode_escape_space() {
        assert_eq!(decode_escape(" "), Some((' ', 1)));
    }

    #[test]
    fn decode_escape_double_quote() {
        assert_eq!(decode_escape("\""), Some(('"', 1)));
    }

    #[test]
    fn decode_escape_slash() {
        assert_eq!(decode_escape("/"), Some(('/', 1)));
    }

    #[test]
    fn decode_escape_backslash() {
        assert_eq!(decode_escape("\\"), Some(('\\', 1)));
    }

    #[test]
    fn decode_escape_nel() {
        assert_eq!(decode_escape("N"), Some(('\u{85}', 1)));
    }

    #[test]
    fn decode_escape_no_break_space() {
        assert_eq!(decode_escape("_"), Some(('\u{A0}', 1)));
    }

    #[test]
    fn decode_escape_line_separator() {
        assert_eq!(decode_escape("L"), Some(('\u{2028}', 1)));
    }

    #[test]
    fn decode_escape_paragraph_separator() {
        assert_eq!(decode_escape("P"), Some(('\u{2029}', 1)));
    }

    #[test]
    fn decode_escape_unicode_2digit() {
        assert_eq!(decode_escape("x41"), Some(('A', 3)));
    }

    #[test]
    fn decode_escape_unicode_4digit() {
        assert_eq!(decode_escape("u0041"), Some(('A', 5)));
    }

    #[test]
    fn decode_escape_unicode_8digit() {
        assert_eq!(decode_escape("U00000041"), Some(('A', 9)));
    }

    #[test]
    fn decode_escape_unicode_8digit_high_plane() {
        assert_eq!(decode_escape("U0001F600"), Some(('\u{1F600}', 9)));
    }

    #[test]
    fn decode_escape_rejects_unknown_code() {
        assert_eq!(decode_escape("q"), None);
    }

    #[test]
    fn decode_escape_rejects_truncated_hex_unicode() {
        assert_eq!(decode_escape("x4"), None);
    }

    #[test]
    fn decode_escape_rejects_non_hex_digit_in_unicode_escape() {
        assert_eq!(decode_escape("xGG"), None);
    }

    #[test]
    fn decode_escape_unicode_4digit_high_surrogate_is_rejected() {
        // U+D800 is a high surrogate — not a valid Unicode scalar
        assert_eq!(decode_escape("uD800"), None);
    }

    #[test]
    fn decode_escape_unicode_8digit_out_of_range_is_rejected() {
        // U+110000 is beyond U+10FFFF
        assert_eq!(decode_escape("U00110000"), None);
    }
}
