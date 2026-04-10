// SPDX-License-Identifier: MIT
// Functions defined here will be used by scanner/lexer in later tasks.
#![allow(dead_code)]

//! YAML 1.2 §5 character productions [1]–[62] and selected character
//! predicates from §6–§8.
//!
//! Each function is named after the spec production and cross-referenced by
//! its production number in a `// [N]` comment.  All functions are pure
//! `fn(char) -> bool` predicates; sequence-level productions (e.g. percent-
//! encoded URI chars, b-break CRLF pairing) live in the scanner.

// ---------------------------------------------------------------------------
// §5.1 – Unicode character set [1]–[3]
// ---------------------------------------------------------------------------

/// [1] c-printable — printable Unicode characters allowed in YAML.
pub const fn is_c_printable(ch: char) -> bool {
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
}

/// [2] nb-json — non-break JSON-compatible characters.
pub const fn is_nb_json(ch: char) -> bool {
    ch == '\t' || ch >= '\x20'
}

/// [3] c-byte-order-mark — the Unicode BOM character (U+FEFF).
pub const fn is_c_byte_order_mark(ch: char) -> bool {
    ch == '\u{FEFF}'
}

// ---------------------------------------------------------------------------
// §5.3 – Indicator characters [22]–[23]
// ---------------------------------------------------------------------------

/// [22] c-indicator — one of the 21 YAML indicator characters.
pub const fn is_c_indicator(ch: char) -> bool {
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

/// [23] c-flow-indicator — the five flow-collection indicator characters.
pub const fn is_c_flow_indicator(ch: char) -> bool {
    matches!(ch, ',' | '[' | ']' | '{' | '}')
}

// ---------------------------------------------------------------------------
// §5.4 – Line break characters [24]–[27]
// ---------------------------------------------------------------------------

/// [24] b-line-feed
pub const fn is_b_line_feed(ch: char) -> bool {
    ch == '\n'
}

/// [25] b-carriage-return
pub const fn is_b_carriage_return(ch: char) -> bool {
    ch == '\r'
}

/// [26] b-char — line feed or carriage return.
pub const fn is_b_char(ch: char) -> bool {
    matches!(ch, '\n' | '\r')
}

/// [27] nb-char — printable character that is not a line break or BOM.
///
/// c-printable minus b-char minus c-byte-order-mark (U+FEFF).
/// Note: U+FEFF lies within the \u{E000}–\u{FFFD} BMP range, so it must be
/// excluded explicitly.
pub const fn is_nb_char(ch: char) -> bool {
    ch != '\u{FEFF}'
        && matches!(ch,
            '\t'
            | '\x20'..='\x7E'
            | '\u{85}'
            | '\u{A0}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
        )
}

// ---------------------------------------------------------------------------
// §5.5 – White space characters [31]–[34]
// ---------------------------------------------------------------------------

/// [31] s-space
pub const fn is_s_space(ch: char) -> bool {
    ch == ' '
}

/// [32] s-tab
pub const fn is_s_tab(ch: char) -> bool {
    ch == '\t'
}

/// [33] s-white — space or tab.
pub const fn is_s_white(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

/// [34] ns-char — non-break, non-white printable character.
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

// ---------------------------------------------------------------------------
// §5.6 – Miscellaneous character classes [35]–[40]
// ---------------------------------------------------------------------------

/// [35] ns-dec-digit — ASCII decimal digit.
pub const fn is_ns_dec_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

/// [36] ns-hex-digit — ASCII hexadecimal digit (case-insensitive).
pub const fn is_ns_hex_digit(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

/// [37] ns-ascii-letter — ASCII letter.
pub const fn is_ns_ascii_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

/// [38] ns-word-char — decimal digit, ASCII letter, or hyphen.
pub const fn is_ns_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-'
}

/// [39] ns-uri-char (single-char form) — characters allowed in a URI
/// that are not percent-sign.
///
/// Note: the percent-encoded form (`%HH`) is a two-character sequence and
/// must be handled at the scanner level.  This predicate covers all
/// single-character URI members.
pub const fn is_ns_uri_char_single(ch: char) -> bool {
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
}

/// [40] ns-tag-char (single-char form) — URI characters minus `!` and
/// flow indicators.
///
/// Same note as [`is_ns_uri_char_single`]: percent-encoded form handled in
/// the scanner.
pub const fn is_ns_tag_char_single(ch: char) -> bool {
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
}

/// [102] ns-anchor-char — ns-char minus flow indicators.
///
/// Used to form anchor names: any non-space, non-break character that is not
/// a flow indicator (`[`, `]`, `{`, `}`, `,`).
pub const fn is_ns_anchor_char(ch: char) -> bool {
    !matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{FEFF}')
        && !is_c_flow_indicator(ch)
        && matches!(ch,
            '\x21'..='\x7E'
            | '\u{85}'
            | '\u{A0}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
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
    if !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let codepoint = u32::from_str_radix(hex_str, 16).ok()?;
    let ch = char::from_u32(codepoint)?;
    Some((ch, start + digit_count))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // c_printable [1]
    // -----------------------------------------------------------------------

    #[test]
    fn c_printable_accepts_tab_lf_cr_and_printable_ascii() {
        assert!(is_c_printable('\t'));
        assert!(is_c_printable('\n'));
        assert!(is_c_printable('\r'));
        assert!(is_c_printable(' '));
        assert!(is_c_printable('~'));
        assert!(is_c_printable('M'));
    }

    #[test]
    fn c_printable_accepts_nel_and_non_breaking_space() {
        assert!(is_c_printable('\u{85}'));
        assert!(is_c_printable('\u{A0}'));
    }

    #[test]
    fn c_printable_rejects_null_del_and_c0_control_chars() {
        assert!(!is_c_printable('\x00'));
        assert!(!is_c_printable('\x7F'));
        for ch in ['\x01', '\x08', '\x0B', '\x0C', '\x0E', '\x1F'] {
            assert!(!is_c_printable(ch), "should reject {ch:?}");
        }
    }

    #[test]
    fn c_printable_rejects_fffe_and_ffff() {
        assert!(!is_c_printable('\u{FFFE}'));
        assert!(!is_c_printable('\u{FFFF}'));
    }

    // -----------------------------------------------------------------------
    // nb_json [2]
    // -----------------------------------------------------------------------

    #[test]
    fn nb_json_accepts_tab_and_printable_ascii() {
        assert!(is_nb_json('\t'));
        assert!(is_nb_json('A'));
    }

    #[test]
    fn nb_json_rejects_null_and_c0_below_0x20_except_tab() {
        assert!(!is_nb_json('\x00'));
        assert!(!is_nb_json('\x01'));
        assert!(!is_nb_json('\x1F'));
    }

    // -----------------------------------------------------------------------
    // c_indicator [22] — indicator/flow-indicator distinctions are critical
    // -----------------------------------------------------------------------

    #[test]
    fn c_indicator_accepts_all_21_indicator_chars() {
        let indicators = [
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ];
        for ch in indicators {
            assert!(is_c_indicator(ch), "should accept {ch:?}");
        }
    }

    #[test]
    fn c_indicator_rejects_plain_alphanum_and_whitespace() {
        assert!(!is_c_indicator('a'));
        assert!(!is_c_indicator('0'));
        assert!(!is_c_indicator(' '));
    }

    #[test]
    fn c_flow_indicator_accepts_exactly_five_chars() {
        for ch in [',', '[', ']', '{', '}'] {
            assert!(is_c_flow_indicator(ch), "should accept {ch:?}");
        }
    }

    #[test]
    fn c_flow_indicator_rejects_non_flow_indicators() {
        // These are c-indicator but NOT c-flow-indicator
        for ch in [
            '-', '?', ':', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`',
        ] {
            assert!(!is_c_flow_indicator(ch), "should reject {ch:?}");
        }
    }

    // -----------------------------------------------------------------------
    // nb_char [27] — BOM exclusion is a subtle edge case
    // -----------------------------------------------------------------------

    #[test]
    fn nb_char_accepts_printable_non_break_non_bom() {
        assert!(is_nb_char('A'));
        assert!(is_nb_char(' '));
        assert!(is_nb_char('\u{00E9}')); // é
    }

    #[test]
    fn nb_char_rejects_lf_cr_and_bom() {
        assert!(!is_nb_char('\n'));
        assert!(!is_nb_char('\r'));
        assert!(!is_nb_char('\u{FEFF}'));
    }

    // -----------------------------------------------------------------------
    // ns_char [34] — whitespace exclusion
    // -----------------------------------------------------------------------

    #[test]
    fn ns_char_accepts_printable_non_whitespace() {
        assert!(is_ns_char('a'));
        assert!(is_ns_char('!'));
        assert!(is_ns_char('\u{4E2D}')); // 中
    }

    #[test]
    fn ns_char_rejects_space_tab_and_line_breaks() {
        assert!(!is_ns_char(' '));
        assert!(!is_ns_char('\t'));
        assert!(!is_ns_char('\n'));
        assert!(!is_ns_char('\r'));
    }

    // -----------------------------------------------------------------------
    // ns_anchor_char [102] — flow-indicator exclusion from ns-char
    // -----------------------------------------------------------------------

    #[test]
    fn ns_anchor_char_accepts_non_flow_ns_chars() {
        assert!(is_ns_anchor_char('a'));
        assert!(is_ns_anchor_char('-'));
        assert!(is_ns_anchor_char(':'));
    }

    #[test]
    fn ns_anchor_char_rejects_flow_indicators() {
        for ch in [',', '[', ']', '{', '}'] {
            assert!(!is_ns_anchor_char(ch), "should reject {ch:?}");
        }
    }

    #[test]
    fn ns_anchor_char_rejects_whitespace_and_bom() {
        assert!(!is_ns_anchor_char(' '));
        assert!(!is_ns_anchor_char('\t'));
        assert!(!is_ns_anchor_char('\u{FEFF}'));
    }

    // -----------------------------------------------------------------------
    // ns_tag_char [40] — excludes '!' and flow indicators vs ns_uri_char
    // -----------------------------------------------------------------------

    #[test]
    fn ns_tag_char_rejects_exclamation_and_flow_indicators() {
        assert!(!is_ns_tag_char_single('!'));
        for ch in [',', '[', ']', '{', '}'] {
            assert!(!is_ns_tag_char_single(ch), "should reject {ch:?}");
        }
    }

    #[test]
    fn ns_tag_char_accepts_word_chars_and_uri_punctuation() {
        assert!(is_ns_tag_char_single('a'));
        assert!(is_ns_tag_char_single('-'));
        assert!(is_ns_tag_char_single('9'));
        assert!(is_ns_tag_char_single(':'));
    }

    #[test]
    fn ns_uri_char_accepts_exclamation_but_tag_char_does_not() {
        // The key distinction between [39] and [40]
        assert!(is_ns_uri_char_single('!'));
        assert!(!is_ns_tag_char_single('!'));
    }

    // -----------------------------------------------------------------------
    // decode_escape — non-trivial escape sequences
    // -----------------------------------------------------------------------

    #[test]
    fn decode_escape_single_char_codes() {
        assert_eq!(decode_escape("0"), Some(('\x00', 1)));
        assert_eq!(decode_escape("n"), Some(('\n', 1)));
        assert_eq!(decode_escape("t"), Some(('\t', 1)));
        assert_eq!(decode_escape("\\"), Some(('\\', 1)));
        assert_eq!(decode_escape("N"), Some(('\u{85}', 1)));
        assert_eq!(decode_escape("_"), Some(('\u{A0}', 1)));
        assert_eq!(decode_escape("L"), Some(('\u{2028}', 1)));
        assert_eq!(decode_escape("P"), Some(('\u{2029}', 1)));
    }

    #[test]
    fn decode_escape_hex_2digit() {
        assert_eq!(decode_escape("x41"), Some(('A', 3)));
    }

    #[test]
    fn decode_escape_hex_4digit() {
        assert_eq!(decode_escape("u0041"), Some(('A', 5)));
    }

    #[test]
    fn decode_escape_hex_8digit() {
        assert_eq!(decode_escape("U00000041"), Some(('A', 9)));
    }

    #[test]
    fn decode_escape_high_plane_codepoint() {
        assert_eq!(decode_escape("U0001F600"), Some(('\u{1F600}', 9)));
    }

    #[test]
    fn decode_escape_rejects_unknown_code() {
        assert_eq!(decode_escape("q"), None);
    }

    #[test]
    fn decode_escape_rejects_truncated_hex() {
        assert_eq!(decode_escape("x4"), None);
    }

    #[test]
    fn decode_escape_rejects_non_hex_digits() {
        assert_eq!(decode_escape("xGG"), None);
    }

    #[test]
    fn decode_escape_rejects_surrogate_codepoint() {
        // U+D800 is a high surrogate — not a valid Unicode scalar
        assert_eq!(decode_escape("uD800"), None);
    }

    #[test]
    fn decode_escape_rejects_out_of_range_codepoint() {
        // U+110000 is beyond U+10FFFF
        assert_eq!(decode_escape("U00110000"), None);
    }
}
