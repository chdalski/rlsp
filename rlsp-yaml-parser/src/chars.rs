// SPDX-License-Identifier: MIT

//! YAML 1.2 §5 character productions used by the parser.
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
// §5.5 – Non-space characters [34]
// ---------------------------------------------------------------------------

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
// §5.6 – Miscellaneous character classes [39]–[40], [102]
// ---------------------------------------------------------------------------

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
    use rstest::rstest;

    use super::*;

    // -----------------------------------------------------------------------
    // c_printable [1]
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::tab('\t')]
    #[case::lf('\n')]
    #[case::cr('\r')]
    #[case::space(' ')]
    #[case::tilde('~')]
    #[case::ascii_letter('M')]
    #[case::nel('\u{85}')]
    #[case::non_breaking_space('\u{A0}')]
    fn c_printable_accepts(#[case] ch: char) {
        assert!(is_c_printable(ch));
    }

    #[rstest]
    #[case::null('\x00')]
    #[case::del('\x7F')]
    #[case::soh('\x01')]
    #[case::bs('\x08')]
    #[case::vt('\x0B')]
    #[case::ff('\x0C')]
    #[case::so('\x0E')]
    #[case::us('\x1F')]
    #[case::fffe('\u{FFFE}')]
    #[case::ffff('\u{FFFF}')]
    fn c_printable_rejects(#[case] ch: char) {
        assert!(!is_c_printable(ch));
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

    #[rstest]
    #[case::lowercase_letter('a')]
    #[case::digit('0')]
    #[case::space(' ')]
    fn c_indicator_rejects(#[case] ch: char) {
        assert!(!is_c_indicator(ch));
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
    // ns_char [34] — whitespace exclusion
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::lowercase_letter('a')]
    #[case::exclamation('!')]
    #[case::cjk_ideograph('\u{4E2D}')]
    fn ns_char_accepts(#[case] ch: char) {
        assert!(is_ns_char(ch));
    }

    #[rstest]
    #[case::space(' ')]
    #[case::tab('\t')]
    #[case::lf('\n')]
    #[case::cr('\r')]
    fn ns_char_rejects(#[case] ch: char) {
        assert!(!is_ns_char(ch));
    }

    // -----------------------------------------------------------------------
    // ns_anchor_char [102] — flow-indicator exclusion from ns-char
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::lowercase_letter('a')]
    #[case::hyphen('-')]
    #[case::colon(':')]
    fn ns_anchor_char_accepts(#[case] ch: char) {
        assert!(is_ns_anchor_char(ch));
    }

    #[test]
    fn ns_anchor_char_rejects_flow_indicators() {
        for ch in [',', '[', ']', '{', '}'] {
            assert!(!is_ns_anchor_char(ch), "should reject {ch:?}");
        }
    }

    #[rstest]
    #[case::space(' ')]
    #[case::tab('\t')]
    #[case::bom('\u{FEFF}')]
    fn ns_anchor_char_rejects(#[case] ch: char) {
        assert!(!is_ns_anchor_char(ch));
    }

    // -----------------------------------------------------------------------
    // ns_tag_char [40] — excludes '!' and flow indicators vs ns_uri_char
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::comma(',')]
    #[case::open_bracket('[')]
    #[case::close_bracket(']')]
    #[case::open_brace('{')]
    #[case::close_brace('}')]
    fn ns_tag_char_rejects_flow_indicators(#[case] ch: char) {
        assert!(!is_ns_tag_char_single(ch));
    }

    #[rstest]
    #[case::lowercase_letter('a')]
    #[case::hyphen('-')]
    #[case::digit('9')]
    #[case::colon(':')]
    fn ns_tag_char_accepts(#[case] ch: char) {
        assert!(is_ns_tag_char_single(ch));
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

    #[rstest]
    #[case::null_escape("0", '\x00', 1)]
    #[case::newline_escape("n", '\n', 1)]
    #[case::tab_escape("t", '\t', 1)]
    #[case::backslash_escape("\\", '\\', 1)]
    #[case::nel_escape("N", '\u{85}', 1)]
    #[case::nbsp_escape("_", '\u{A0}', 1)]
    #[case::line_sep_escape("L", '\u{2028}', 1)]
    #[case::para_sep_escape("P", '\u{2029}', 1)]
    #[case::hex_2digit("x41", 'A', 3)]
    #[case::hex_4digit("u0041", 'A', 5)]
    #[case::hex_8digit("U00000041", 'A', 9)]
    #[case::high_plane_codepoint("U0001F600", '\u{1F600}', 9)]
    fn decode_escape_success(
        #[case] input: &str,
        #[case] expected_char: char,
        #[case] expected_len: usize,
    ) {
        assert_eq!(decode_escape(input), Some((expected_char, expected_len)));
    }

    #[rstest]
    #[case::unknown_code("q")]
    #[case::truncated_hex("x4")]
    #[case::non_hex_digits("xGG")]
    #[case::surrogate_codepoint("uD800")]
    #[case::out_of_range_codepoint("U00110000")]
    fn decode_escape_rejects(#[case] input: &str) {
        assert_eq!(decode_escape(input), None);
    }
}
