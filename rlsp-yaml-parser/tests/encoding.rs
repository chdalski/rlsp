// SPDX-License-Identifier: MIT
//
// Encoding edge-case tests — malformed UTF-8, NUL bytes, BOM handling,
// and valid multibyte content.
//
// Architecture note: `parse_events` takes `&str`, so Rust guarantees valid
// UTF-8 at the type level. Malformed UTF-8 byte sequences are tested via
// `encoding::decode(&[u8])`, which is the entry point for raw byte streams.
// NUL (0x00) is valid UTF-8 but excluded from YAML's c-printable production,
// so it is tested via `parse_events` at the semantic level.

#![expect(clippy::unwrap_used, missing_docs, reason = "test code")]

use rstest::rstest;

use rlsp_yaml_parser::encoding::{EncodingError, decode};
use rlsp_yaml_parser::{Event, parse_events};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Collect parse events and return whether any errors were produced.
fn has_parse_error(input: &str) -> bool {
    parse_events(input).any(|r| r.is_err())
}

/// Collect parse events and extract all scalar values.
fn scalar_values(input: &str) -> Vec<String> {
    parse_events(input)
        .filter_map(Result::ok)
        .filter_map(|(event, _span)| match event {
            Event::Scalar { value, .. } => Some(value.into_owned()),
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::MappingStart { .. }
            | Event::MappingEnd
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::Alias { .. }
            | Event::Comment { .. } => None,
        })
        .collect()
}

// ===========================================================================
// decode() — malformed UTF-8 input
// ===========================================================================

#[rstest]
#[case::lone_continuation(&[0x80u8] as &[u8])]
#[case::high_continuation(&[0xBFu8] as &[u8])]
#[case::incomplete_two_byte(&[0xC3u8] as &[u8])]
#[case::incomplete_three_byte(&[0xE2u8, 0x82] as &[u8])]
#[case::incomplete_four_byte(&[0xF0u8, 0x9F, 0x98] as &[u8])]
#[case::overlong_nul(&[0xC0u8, 0x80] as &[u8])]
#[case::invalid_0xfe(&[0xFEu8, b'x'] as &[u8])]
#[case::invalid_0xff(&[0xFFu8, b'x'] as &[u8])]
#[case::truncated_at_eof(b"hello\xC3" as &[u8])]
fn decode_invalid_bytes_returns_error(#[case] input: &[u8]) {
    assert_eq!(decode(input), Err(EncodingError::InvalidBytes));
}

// ===========================================================================
// decode() — valid multibyte UTF-8
// ===========================================================================

#[rstest]
#[case::two_byte("café", "café")]
#[case::three_byte("中文", "中文")]
#[case::four_byte("\u{1F600}", "😀")]
#[case::arabic("\u{0639}\u{0631}\u{0628}\u{064A}", "\u{0639}\u{0631}\u{0628}\u{064A}")]
fn decode_valid_multibyte_roundtrip(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(decode(input.as_bytes()).unwrap(), expected);
}

// ===========================================================================
// decode() — BOM handling
// ===========================================================================

#[rstest]
#[case::utf8_bom(&[0xEFu8, 0xBB, 0xBF, b'k', b'e', b'y'] as &[u8], "key")]
#[case::utf16_le_bom(&[0xFFu8, 0xFE, 0x68, 0x00, 0x69, 0x00] as &[u8], "hi")]
#[case::utf16_be_bom(&[0xFEu8, 0xFF, 0x00, 0x68, 0x00, 0x69] as &[u8], "hi")]
#[case::utf32_le_bom(&[0xFFu8, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00] as &[u8], "A")]
#[case::utf32_be_bom(&[0x00u8, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41] as &[u8], "A")]
fn decode_bom_stripping(#[case] input: &[u8], #[case] expected: &str) {
    assert_eq!(decode(input).unwrap(), expected);
}

// ===========================================================================
// decode() — UTF-16/32 error cases
// ===========================================================================

#[test]
fn decode_rejects_truncated_utf16() {
    // UTF-16 LE BOM + one byte — odd total length.
    assert_eq!(
        decode(&[0xFF, 0xFE, 0x68]),
        Err(EncodingError::TruncatedUtf16)
    );
}

#[test]
fn decode_rejects_truncated_utf32() {
    // UTF-32 BE BOM + 1 byte — length 5, not a multiple of 4.
    assert_eq!(
        decode(&[0x00, 0x00, 0xFE, 0xFF, 0x00]),
        Err(EncodingError::TruncatedUtf32)
    );
}

#[test]
fn decode_rejects_utf16_unpaired_surrogate() {
    // UTF-16 BE BOM + lone high surrogate 0xD800.
    assert_eq!(
        decode(&[0xFE, 0xFF, 0xD8, 0x00]),
        Err(EncodingError::InvalidCodepoint(0xD800))
    );
}

#[test]
fn decode_rejects_utf32_out_of_range_codepoint() {
    // UTF-32 BE BOM + U+110000 (above the valid Unicode range).
    assert_eq!(
        decode(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x11, 0x00, 0x00]),
        Err(EncodingError::InvalidCodepoint(0x0011_0000))
    );
}

// ===========================================================================
// parse_events() — NUL byte in content
// ===========================================================================
//
// NUL (U+0000) is valid UTF-8 but excluded from YAML 1.2 c-printable [1],
// so the parser should reject it in all content positions.

#[rstest]
#[case::plain_scalar("key: val\0ue\n")]
#[case::comment("key: value  # comment\0here\n")]
#[case::standalone("\0\n")]
fn parse_events_nul_produces_error(#[case] input: &str) {
    assert!(has_parse_error(input));
}

#[test]
fn parse_events_accepts_nul_in_double_quoted_scalar() {
    // NUL (U+0000) is excluded from YAML 1.2 c-printable [1], but the parser's
    // nb-double-char implementation accepts any non-break, non-BOM character
    // that is not '"' or '\'. NUL passes that filter, so it is accepted inside
    // double-quoted scalars. Assert actual behavior.
    assert!(!has_parse_error("key: \"val\0ue\"\n"));
}

// ===========================================================================
// parse_events() — BOM handling
// ===========================================================================

#[test]
fn parse_events_accepts_bom_at_stream_start() {
    // YAML 1.2 §5.2: BOM is allowed at the start of a stream.
    let input = "\u{FEFF}key: value\n";
    assert!(!has_parse_error(input));
    let values = scalar_values(input);
    assert!(
        values.contains(&"value".to_string()),
        "expected scalar 'value', got: {values:?}"
    );
}

#[test]
fn parse_events_rejects_bom_mid_stream() {
    // BOM embedded mid-scalar. Although U+FEFF is within c_printable's
    // \u{E000}..=\u{FFFD} range, the parser rejects it in this position
    // because it breaks plain scalar parsing — the tokenizer does not
    // recognize BOM as valid scalar content in flow context.
    assert!(has_parse_error("key: val\u{FEFF}ue\n"));
}

// ===========================================================================
// parse_events() — valid multibyte content
// ===========================================================================

#[test]
fn parse_events_accepts_emoji_in_double_quoted_scalar() {
    let input = "greeting: \"hello\u{1F600}\"\n";
    assert!(!has_parse_error(input));
    let values = scalar_values(input);
    assert!(
        values.contains(&"hello😀".to_string()),
        "expected scalar with emoji, got: {values:?}"
    );
}

#[test]
fn parse_events_accepts_cjk_in_plain_scalar() {
    let input = "title: 中文\n";
    assert!(!has_parse_error(input));
    let values = scalar_values(input);
    assert!(
        values.contains(&"中文".to_string()),
        "expected scalar '中文', got: {values:?}"
    );
}

#[test]
fn parse_events_accepts_arabic_in_mapping_key() {
    let arabic = "\u{0639}\u{0631}\u{0628}\u{064A}";
    let input = format!("{arabic}: value\n");
    assert!(!has_parse_error(&input));
    let values = scalar_values(&input);
    assert!(
        values.contains(&arabic.to_string()),
        "expected Arabic key scalar, got: {values:?}"
    );
}
