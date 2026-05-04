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

/// [2] nb-json — characters allowed anywhere inside a quoted scalar (nb-json
/// exception per §5.1: "YAML processors must allow all non-C0 characters
/// inside quoted scalars").
///
/// nb-json = x09 | [x20-x10FFFF]
///
/// Broader than `c-printable`: additionally accepts DEL (x7F), C1 controls
/// (x80-x9F), and non-characters xFFFE / xFFFF.  Only the C0 control range
/// (x00-x08, x0A-x1F) is excluded (x09 TAB is permitted, LF/CR are handled
/// by the line splitter and never appear in line content).
///
/// Production code uses the byte-level [`find_non_nb_json`] for efficiency.
/// This predicate is exposed for unit testing and documentation purposes.
#[cfg(test)]
pub const fn is_nb_json(ch: char) -> bool {
    matches!(ch, '\t' | '\x20'..='\u{10FFFF}')
}

// ---------------------------------------------------------------------------
// Byte-level character-set validation helpers
// ---------------------------------------------------------------------------

/// Find the first byte position in `bytes` whose character violates the
/// c-printable rule, returning `(byte_offset, char)`.
///
/// Non-printable byte patterns in valid UTF-8:
/// - C0 controls: single bytes `0x00-0x08`, `0x0B-0x0C`, `0x0E-0x1F`
///   (TAB 0x09 is c-printable; LF 0x0A and CR 0x0D are excluded from line
///   content by the line splitter and will not appear here)
/// - DEL: single byte `0x7F`
/// - C1 controls (except NEL U+0085): two-byte sequence `0xC2 0x80-0x84`,
///   `0xC2 0x86-0x9F`
/// - Non-characters U+FFFE / U+FFFF: three-byte sequences `0xEF 0xBF 0xBE/0xBF`
///
/// # Invariants
/// `bytes` must be valid UTF-8 (guaranteed by Rust's `&str`). Positions
/// returned are always char-boundary aligned because the scan advances by
/// `char::len_utf8()` for multi-byte codepoints.
#[expect(
    clippy::indexing_slicing,
    reason = "bounds are enforced by the while i < bytes.len() guard and explicit i + 1 / i + 2 checks"
)]
pub fn find_non_c_printable(bytes: &[u8]) -> Option<(usize, char)> {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x20 {
            // C0 control. TAB (0x09) is c-printable; LF (0x0A) and CR (0x0D)
            // never appear in line content (stripped by the line splitter).
            if b != 0x09 {
                return Some((i, b as char));
            }
            i += 1;
        } else if b == 0x7F {
            // DEL — not c-printable.
            return Some((i, '\x7F'));
        } else if b == 0xC2 && i + 1 < bytes.len() {
            let b2 = bytes[i + 1];
            if (0x80..=0x9F).contains(&b2) && b2 != 0x85 {
                // C1 control except NEL (U+0085 = 0xC2 0x85 is c-printable).
                // Decode the two-byte UTF-8 sequence manually.
                // A leading 0xC2 means the codepoint is 0x80 | (b2 & 0x3F).
                let ch = char::from_u32(u32::from(b2 & 0x3F) | 0x80).unwrap_or('\u{FFFD}');
                return Some((i, ch));
            }
            i += 2;
        } else if b == 0xEF && i + 2 < bytes.len() {
            let b2 = bytes[i + 1];
            let b3 = bytes[i + 2];
            if b2 == 0xBF && (b3 == 0xBE || b3 == 0xBF) {
                // U+FFFE (0xEF 0xBF 0xBE) or U+FFFF (0xEF 0xBF 0xBF).
                let ch = if b3 == 0xBE { '\u{FFFE}' } else { '\u{FFFF}' };
                return Some((i, ch));
            }
            i += 3;
        } else if b >= 0x80 {
            // Other multi-byte sequence: advance by char width to stay on
            // char boundaries. Safety: bytes is valid UTF-8 (`&str` guarantee).
            // SAFETY: `i` is always on a char boundary because we advance only
            // by single bytes for ASCII and by `char::len_utf8()` for multi-byte.
            let s = unsafe { std::str::from_utf8_unchecked(&bytes[i..]) };
            let ch = s.chars().next().unwrap_or('\u{FFFD}');
            i += ch.len_utf8();
        } else {
            i += 1;
        }
    }
    None
}

/// Find the first byte position in `bytes` whose character violates the
/// nb-json rule, returning `(byte_offset, char)`.
///
/// nb-json rejects only the C0 control range excluding TAB: bytes `0x00-0x08`
/// and `0x0A-0x1F` (all single-byte). DEL (0x7F), C1 controls (U+0080-U+009F),
/// and non-characters U+FFFE/U+FFFF are all accepted by nb-json.
///
/// Note: C1 controls (except NEL) are accepted here even though they are not
/// c-printable. This is the mandatory JSON-compatibility exception from §5.1:
/// "YAML processors must allow all non-C0 characters inside quoted scalars."
/// Downstream consumers displaying or logging values containing C1/DEL should
/// apply their own sanitization.
#[expect(
    clippy::indexing_slicing,
    reason = "bounds are enforced by the while i < bytes.len() guard"
)]
pub fn find_non_nb_json(bytes: &[u8]) -> Option<(usize, char)> {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x20 && b != 0x09 {
            // C0 control excluding TAB — the only bytes nb-json rejects.
            return Some((i, b as char));
        }
        // All other bytes (including DEL 0x7F, C1 sequences starting with 0xC2,
        // and U+FFFE/FFFF starting with 0xEF) are accepted by nb-json.
        // Advance by char width to stay on char boundaries.
        if b >= 0x80 {
            // SAFETY: bytes is valid UTF-8; `i` is always on a char boundary.
            let s = unsafe { std::str::from_utf8_unchecked(&bytes[i..]) };
            let ch = s.chars().next().unwrap_or('\u{FFFD}');
            i += ch.len_utf8();
        } else {
            i += 1;
        }
    }
    None
}

/// Build the standard error message for a non-printable character found in the
/// given `context`.
///
/// Format: `"non-printable character U+XXXX is not allowed in <context>"`
#[must_use]
pub fn non_printable_error_message(ch: char, context: &str) -> String {
    format!(
        "non-printable character U+{:04X} is not allowed in {context}",
        u32::from(ch)
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

    // -----------------------------------------------------------------------
    // is_nb_json [2]
    // -----------------------------------------------------------------------

    // Group NJ-A: accepts
    #[rstest]
    #[case::tab('\t')] // 0x09 — explicitly included
    #[case::space(' ')] // 0x20
    #[case::printable_ascii('A')] // 0x41
    #[case::del('\x7F')] // DEL — nb-json extension vs c-printable
    #[case::c1_control_0x80('\u{80}')] // C1 — nb-json extension
    #[case::c1_control_0x9f('\u{9F}')] // C1 boundary
    #[case::fffe('\u{FFFE}')] // non-character — nb-json accepts it
    #[case::ffff('\u{FFFF}')] // non-character — nb-json accepts it
    #[case::supplementary('\u{1F600}')] // emoji
    fn nb_json_accepts(#[case] ch: char) {
        assert!(is_nb_json(ch));
    }

    // Group NJ-B: rejects (C0 controls excluding TAB)
    #[rstest]
    #[case::nul('\x00')]
    #[case::soh('\x01')]
    #[case::stx('\x02')]
    #[case::etx('\x03')]
    #[case::eot('\x04')]
    #[case::enq('\x05')]
    #[case::ack('\x06')]
    #[case::bel('\x07')]
    #[case::bs('\x08')]
    #[case::lf('\n')] // 0x0A
    #[case::vt('\x0B')]
    #[case::ff('\x0C')]
    #[case::cr('\r')] // 0x0D
    #[case::so('\x0E')]
    #[case::us('\x1F')]
    fn nb_json_rejects(#[case] ch: char) {
        assert!(!is_nb_json(ch));
    }

    // -----------------------------------------------------------------------
    // find_non_c_printable
    // -----------------------------------------------------------------------

    #[test]
    fn find_non_c_printable_returns_none_for_clean_ascii() {
        assert_eq!(find_non_c_printable(b"hello world"), None);
    }

    #[test]
    fn find_non_c_printable_returns_none_for_tab() {
        assert_eq!(find_non_c_printable(b"foo\tbar"), None);
    }

    #[test]
    fn find_non_c_printable_detects_c0_control() {
        let result = find_non_c_printable(b"foo\x01bar");
        assert_eq!(result, Some((3, '\x01')));
    }

    #[test]
    fn find_non_c_printable_detects_nul() {
        let result = find_non_c_printable(b"foo\x00bar");
        assert_eq!(result, Some((3, '\x00')));
    }

    #[test]
    fn find_non_c_printable_detects_del() {
        let result = find_non_c_printable(b"foo\x7Fbar");
        assert_eq!(result, Some((3, '\x7F')));
    }

    #[test]
    fn find_non_c_printable_detects_c1_control() {
        // U+0080 encodes as [0xC2, 0x80]
        let input = "foo\u{80}bar";
        let result = find_non_c_printable(input.as_bytes());
        assert_eq!(result, Some((3, '\u{80}')));
    }

    #[test]
    fn find_non_c_printable_accepts_nel() {
        // U+0085 (NEL) is c-printable; encodes as [0xC2, 0x85]
        let input = "foo\u{85}bar";
        assert_eq!(find_non_c_printable(input.as_bytes()), None);
    }

    #[test]
    fn find_non_c_printable_detects_fffe() {
        // U+FFFE encodes as [0xEF, 0xBF, 0xBE]
        let input = "foo\u{FFFE}bar";
        let result = find_non_c_printable(input.as_bytes());
        assert_eq!(result, Some((3, '\u{FFFE}')));
    }

    #[test]
    fn find_non_c_printable_detects_ffff() {
        // U+FFFF encodes as [0xEF, 0xBF, 0xBF]
        let input = "foo\u{FFFF}bar";
        let result = find_non_c_printable(input.as_bytes());
        assert_eq!(result, Some((3, '\u{FFFF}')));
    }

    #[test]
    fn find_non_c_printable_accepts_valid_bmp_multibyte() {
        // U+4E2D (CJK) is c-printable
        let input = "foo\u{4E2D}bar";
        assert_eq!(find_non_c_printable(input.as_bytes()), None);
    }

    // -----------------------------------------------------------------------
    // find_non_nb_json
    // -----------------------------------------------------------------------

    #[test]
    fn find_non_nb_json_returns_none_for_clean_ascii() {
        assert_eq!(find_non_nb_json(b"hello world"), None);
    }

    #[test]
    fn find_non_nb_json_returns_none_for_tab() {
        assert_eq!(find_non_nb_json(b"foo\tbar"), None);
    }

    #[test]
    fn find_non_nb_json_returns_none_for_del() {
        // DEL is accepted by nb-json
        assert_eq!(find_non_nb_json(b"foo\x7Fbar"), None);
    }

    #[test]
    fn find_non_nb_json_returns_none_for_c1_control() {
        // C1 controls are accepted by nb-json
        let input = "foo\u{80}bar";
        assert_eq!(find_non_nb_json(input.as_bytes()), None);
    }

    #[test]
    fn find_non_nb_json_returns_none_for_fffe() {
        let input = "foo\u{FFFE}bar";
        assert_eq!(find_non_nb_json(input.as_bytes()), None);
    }

    #[test]
    fn find_non_nb_json_detects_c0_control() {
        let result = find_non_nb_json(b"foo\x01bar");
        assert_eq!(result, Some((3, '\x01')));
    }

    #[test]
    fn find_non_nb_json_detects_nul() {
        let result = find_non_nb_json(b"foo\x00bar");
        assert_eq!(result, Some((3, '\x00')));
    }
}
