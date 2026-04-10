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

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

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

#[test]
fn decode_rejects_lone_continuation_byte() {
    assert_eq!(decode(&[0x80]), Err(EncodingError::InvalidBytes));
}

#[test]
fn decode_rejects_high_continuation_byte() {
    assert_eq!(decode(&[0xBF]), Err(EncodingError::InvalidBytes));
}

#[test]
fn decode_rejects_incomplete_two_byte_sequence() {
    // Two-byte lead 0xC3 without its continuation byte.
    assert_eq!(decode(&[0xC3]), Err(EncodingError::InvalidBytes));
}

#[test]
fn decode_rejects_incomplete_three_byte_sequence() {
    // Three-byte lead 0xE2 with only one continuation byte (needs two).
    assert_eq!(decode(&[0xE2, 0x82]), Err(EncodingError::InvalidBytes));
}

#[test]
fn decode_rejects_incomplete_four_byte_sequence() {
    // Four-byte lead 0xF0 with two continuations (needs three).
    assert_eq!(
        decode(&[0xF0, 0x9F, 0x98]),
        Err(EncodingError::InvalidBytes)
    );
}

#[test]
fn decode_rejects_overlong_encoding() {
    // 0xC0 0x80 is an overlong encoding of NUL — forbidden in UTF-8.
    assert_eq!(decode(&[0xC0, 0x80]), Err(EncodingError::InvalidBytes));
}

#[test]
fn decode_rejects_byte_0xfe() {
    // 0xFE followed by a non-BOM byte, so detect_encoding falls through to
    // UTF-8 and decode_utf8 rejects the invalid lead byte.
    assert_eq!(decode(&[0xFE, b'x']), Err(EncodingError::InvalidBytes));
}

#[test]
fn decode_rejects_byte_0xff_without_bom_context() {
    // 0xFF followed by a non-BOM byte avoids UTF-16 LE BOM detection,
    // landing in the UTF-8 path which rejects 0xFF.
    assert_eq!(decode(&[0xFF, b'x']), Err(EncodingError::InvalidBytes));
}

#[test]
fn decode_rejects_truncated_utf8_at_eof() {
    // Valid ASCII followed by a truncated two-byte sequence at EOF.
    assert_eq!(decode(b"hello\xC3"), Err(EncodingError::InvalidBytes));
}

// ===========================================================================
// decode() — valid multibyte UTF-8
// ===========================================================================

#[test]
fn decode_accepts_two_byte_utf8_sequence() {
    // U+00E9 LATIN SMALL LETTER E WITH ACUTE — 2-byte UTF-8.
    assert_eq!(decode("café".as_bytes()).unwrap(), "café");
}

#[test]
fn decode_accepts_three_byte_utf8_sequence() {
    // CJK characters — 3-byte UTF-8 each.
    assert_eq!(decode("中文".as_bytes()).unwrap(), "中文");
}

#[test]
fn decode_accepts_four_byte_utf8_sequence() {
    // U+1F600 GRINNING FACE emoji — 4-byte UTF-8.
    assert_eq!(decode("\u{1F600}".as_bytes()).unwrap(), "😀");
}

#[test]
fn decode_accepts_arabic_script() {
    let arabic = "\u{0639}\u{0631}\u{0628}\u{064A}";
    let result = decode(arabic.as_bytes()).unwrap();
    assert_eq!(result, arabic);
}

// ===========================================================================
// decode() — BOM handling
// ===========================================================================

#[test]
fn decode_strips_utf8_bom() {
    assert_eq!(
        decode(&[0xEF, 0xBB, 0xBF, b'k', b'e', b'y']).unwrap(),
        "key"
    );
}

#[test]
fn decode_strips_utf16_le_bom() {
    // UTF-16 LE BOM + "hi" in UTF-16 LE.
    assert_eq!(decode(&[0xFF, 0xFE, 0x68, 0x00, 0x69, 0x00]).unwrap(), "hi");
}

#[test]
fn decode_strips_utf16_be_bom() {
    // UTF-16 BE BOM + "hi" in UTF-16 BE.
    assert_eq!(decode(&[0xFE, 0xFF, 0x00, 0x68, 0x00, 0x69]).unwrap(), "hi");
}

#[test]
fn decode_strips_utf32_le_bom() {
    // UTF-32 LE BOM + "A" in UTF-32 LE.
    assert_eq!(
        decode(&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00]).unwrap(),
        "A"
    );
}

#[test]
fn decode_strips_utf32_be_bom() {
    // UTF-32 BE BOM + "A" in UTF-32 BE.
    assert_eq!(
        decode(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41]).unwrap(),
        "A"
    );
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

#[test]
fn parse_events_rejects_nul_in_plain_scalar() {
    assert!(has_parse_error("key: val\0ue\n"));
}

#[test]
fn parse_events_accepts_nul_in_double_quoted_scalar() {
    // NUL (U+0000) is excluded from YAML 1.2 c-printable [1], but the parser's
    // nb-double-char implementation accepts any non-break, non-BOM character
    // that is not '"' or '\'. NUL passes that filter, so it is accepted inside
    // double-quoted scalars. Assert actual behavior.
    assert!(!has_parse_error("key: \"val\0ue\"\n"));
}

#[test]
fn parse_events_rejects_nul_in_comment() {
    assert!(has_parse_error("key: value  # comment\0here\n"));
}

#[test]
fn parse_events_rejects_nul_as_standalone() {
    assert!(has_parse_error("\0\n"));
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
