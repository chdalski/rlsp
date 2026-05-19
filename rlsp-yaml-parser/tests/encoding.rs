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

#![expect(clippy::wildcard_enum_match_arm, missing_docs, reason = "test code")]

use proptest::prelude::*;
use rstest::rstest;

use rlsp_yaml_parser::encoding::{Encoding, EncodingError, decode, detect_encoding};
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
fn parse_events_rejects_nul_in_double_quoted_scalar() {
    // NUL (U+0000) is a C0 control character excluded by nb-json per YAML §5.1.
    // nb-json = x09 | [x20-x10FFFF]; NUL (x00) is excluded.
    // Parser enforces nb-json on literal content in double-quoted scalars.
    assert!(has_parse_error("key: \"val\0ue\"\n"));
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
// parse_events() — BOM at document-prefix positions (YAML 1.2 §5.2)
// ===========================================================================
//
// YAML 1.2 §5.2 / production [202] l-document-prefix = c-byte-order-mark? l-comment*
// A BOM is valid at the start of any document prefix — not only at stream start.

#[test]
fn parse_events_accepts_bom_immediately_after_document_end_marker() {
    // BOM at the start of the second document's prefix (no blank lines between).
    let input = "key: a\n...\n\u{FEFF}key: b\n";
    assert!(
        !has_parse_error(input),
        "BOM immediately after '...' must be accepted"
    );
    let values = scalar_values(input);
    assert!(
        values.contains(&"a".to_string()),
        "first doc scalar 'a' present"
    );
    assert!(
        values.contains(&"b".to_string()),
        "second doc scalar 'b' present"
    );
}

#[test]
fn parse_events_accepts_bom_after_doc_end_then_blank_lines() {
    // BOM after a blank line that follows the `...` marker.
    let input = "key: a\n...\n\n\u{FEFF}key: b\n";
    assert!(
        !has_parse_error(input),
        "BOM after blank line after '...' must be accepted"
    );
    let values = scalar_values(input);
    assert!(values.contains(&"a".to_string()));
    assert!(values.contains(&"b".to_string()));
}

#[test]
fn parse_events_accepts_bom_after_doc_end_then_comment() {
    // BOM after a trail comment that follows the `...` marker.
    let input = "key: a\n...\n# comment\n\u{FEFF}key: b\n";
    assert!(
        !has_parse_error(input),
        "BOM after comment after '...' must be accepted"
    );
    let values = scalar_values(input);
    assert!(values.contains(&"a".to_string()));
    assert!(values.contains(&"b".to_string()));
    // The comment event must also be present.
    let has_comment = parse_events(input)
        .filter_map(Result::ok)
        .any(|(event, _)| matches!(event, Event::Comment { text } if text.trim() == "comment"));
    assert!(has_comment, "expected Comment event with text 'comment'");
}

#[test]
fn parse_events_accepts_multiple_docs_each_with_bom() {
    // Each document in the stream starts with a BOM at its prefix position.
    let input = "\u{FEFF}a: 1\n...\n\u{FEFF}b: 2\n...\n\u{FEFF}c: 3\n";
    assert!(
        !has_parse_error(input),
        "multiple docs each with BOM must be accepted"
    );
    let values = scalar_values(input);
    assert!(values.contains(&"1".to_string()));
    assert!(values.contains(&"2".to_string()));
    assert!(values.contains(&"3".to_string()));
}

#[test]
fn parse_events_bom_at_stream_start_still_accepted() {
    // Regression: stream-start BOM handling must not be broken by the fix.
    let input = "\u{FEFF}key: value\n";
    assert!(!has_parse_error(input));
    let values = scalar_values(input);
    assert!(values.contains(&"value".to_string()));
}

#[test]
fn parse_events_rejects_bom_mid_scalar_regression() {
    // Regression: BOM embedded mid-scalar must still produce a parse error.
    assert!(has_parse_error("key: val\u{FEFF}ue\n"));
}

#[test]
fn load_multidoc_with_bom_between_docs_produces_correct_ast() {
    // The `load()` API (loader layer) must independently handle inter-document BOM.
    let input = "key: a\n...\n\u{FEFF}key: b\n";
    let docs = rlsp_yaml_parser::load(input).expect("load must succeed");
    assert_eq!(docs.len(), 2, "expected two documents");
    // First document: root is a mapping with key→"a".
    match &docs[0].root {
        rlsp_yaml_parser::Node::Mapping { entries, .. } => {
            assert_eq!(entries.len(), 1);
            let (k, v) = &entries[0];
            assert!(matches!(k, rlsp_yaml_parser::Node::Scalar { value, .. } if value == "key"));
            assert!(matches!(v, rlsp_yaml_parser::Node::Scalar { value, .. } if value == "a"));
        }
        other => panic!("expected mapping, got {other:?}"),
    }
    // Second document: root is a mapping with key→"b".
    match &docs[1].root {
        rlsp_yaml_parser::Node::Mapping { entries, .. } => {
            assert_eq!(entries.len(), 1);
            let (k, v) = &entries[0];
            assert!(matches!(k, rlsp_yaml_parser::Node::Scalar { value, .. } if value == "key"));
            assert!(matches!(v, rlsp_yaml_parser::Node::Scalar { value, .. } if value == "b"));
        }
        other => panic!("expected mapping, got {other:?}"),
    }
}

#[test]
fn parse_events_bom_after_directives_end_marker_is_error() {
    // After `---`, the document body begins immediately — there is no
    // `l-document-prefix` between `---` and content.  A BOM here is
    // inside the document body and is not a valid character.
    let input = "key: a\n...\n---\n\u{FEFF}key: b\n";
    assert!(
        has_parse_error(input),
        "BOM after '---' is inside the document body and must produce a parse error"
    );
}

#[test]
fn parse_events_rejects_double_bom_at_document_prefix() {
    // Only one BOM is stripped at a document prefix position.
    // A second consecutive BOM is illegal content.
    let input = "key: a\n...\n\u{FEFF}\u{FEFF}key: b\n";
    assert!(
        has_parse_error(input),
        "double BOM at document prefix must produce a parse error"
    );
}

#[test]
fn parse_events_rejects_double_bom_at_stream_start() {
    // Two BOMs at the very start of the stream — only one is stripped at the
    // document-prefix position; the second is illegal content.
    let input = "\u{FEFF}\u{FEFF}key: v\n";
    assert!(
        has_parse_error(input),
        "double BOM at stream start must produce a parse error"
    );
}

#[test]
fn parse_events_accepts_single_bom_at_stream_start_regression() {
    // Regression guard: a single BOM at stream start must still be accepted.
    let input = "\u{FEFF}key: v\n";
    assert!(
        !has_parse_error(input),
        "single BOM at stream start must be accepted"
    );
    let values = scalar_values(input);
    assert!(
        values.contains(&"v".to_string()),
        "expected scalar 'v', got: {values:?}"
    );
}

#[test]
fn parse_events_rejects_double_bom_at_inter_doc_regression() {
    // Regression guard: double BOM at inter-document prefix still produces an error.
    let input = "key: a\n...\n\u{FEFF}\u{FEFF}key: b\n";
    assert!(
        has_parse_error(input),
        "double BOM at inter-document prefix must produce a parse error"
    );
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

// ===========================================================================
// decode() — BOM priority ordering (GAP-E2)
// ===========================================================================
//
// The 4-byte UTF-32-BE BOM [00 00 FE FF] contains the 2-byte UTF-16-BE BOM
// [FE FF] at offset 2.  Detection must check 4-byte BOMs before 2-byte BOMs;
// otherwise a UTF-32-BE stream would be misidentified as UTF-16-BE.

#[test]
fn utf32_be_bom_takes_priority_over_utf16_be_prefix() {
    // UTF-32-BE BOM [00 00 FE FF] followed by a single LF (U+000A) encoded as
    // the 4-byte UTF-32-BE codepoint [00 00 00 0A].  Total: 8 bytes.
    // If detection incorrectly matched [FE FF] (at offset 2) as UTF-16-BE,
    // the leading [00 00] would become the first code unit (U+0000), causing a
    // truncated or invalid decode instead of a clean newline string.
    let input: &[u8] = &[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x0A];
    assert_eq!(detect_encoding(input), Encoding::Utf32Be);
    assert_eq!(decode(input).unwrap(), "\n");
}

// ===========================================================================
// detect_encoding() — YAML 1.2 §5.2 detection-table spec fixture
// ===========================================================================
//
// One case per row of the §5.2 encoding detection table — both BOM and
// BOM-less heuristic rows. This is the authoritative regression baseline for
// dispatch completeness: future readers can see which spec-table row each case
// covers.
//
// Note: the utf16_le_with_bom case uses [0xFF, 0xFE, 0x41, 0x00] (non-zero
// at byte 2) rather than [0xFF, 0xFE, 0x00, 0x00] (which is the UTF-32-LE
// BOM) to represent an unambiguous UTF-16-LE BOM input.

#[rstest]
#[case::utf32_be_with_bom(&[0x00u8, 0x00, 0xFE, 0xFF], Encoding::Utf32Be)]
#[case::utf32_le_with_bom(&[0xFFu8, 0xFE, 0x00, 0x00], Encoding::Utf32Le)]
#[case::utf16_be_with_bom(&[0xFEu8, 0xFF, 0x00, 0x41], Encoding::Utf16Be)]
#[case::utf16_le_with_bom(&[0xFFu8, 0xFE, 0x41, 0x00], Encoding::Utf16Le)]
#[case::utf8_with_bom(&[0xEFu8, 0xBB, 0xBF, 0x41], Encoding::Utf8)]
#[case::utf32_be_no_bom(&[0x00u8, 0x00, 0x00, 0x41], Encoding::Utf32Be)]
#[case::utf32_le_no_bom(&[0x41u8, 0x00, 0x00, 0x00], Encoding::Utf32Le)]
#[case::utf16_be_no_bom(&[0x00u8, 0x41, 0x00, 0x42], Encoding::Utf16Be)]
#[case::utf16_le_no_bom(&[0x41u8, 0x00, 0x42, 0x00], Encoding::Utf16Le)]
#[case::utf8_default(&[0x41u8, 0x42, 0x43, 0x44], Encoding::Utf8)]
fn detect_encoding_covers_all_spec_rows(#[case] bytes: &[u8], #[case] expected: Encoding) {
    assert_eq!(detect_encoding(bytes), expected);
}

// ===========================================================================
// detect_encoding() + decode() — encoding dispatch completeness proptest
// ===========================================================================
//
// Property: for any ASCII-only YAML string, encoding it in any of the four
// non-UTF-8 encodings × {with BOM, without BOM} and decoding it must produce
// the same event sequence as the UTF-8 baseline. This is a permanent
// dispatch-completeness guardrail — adding a new encoding variant without
// correct dispatch coverage will cause this property to fail automatically.

fn encode_ascii_as_utf32(bytes: &[u8], big_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() * 4);
    for &b in bytes {
        let cp = u32::from(b);
        if big_endian {
            out.extend_from_slice(&cp.to_be_bytes());
        } else {
            out.extend_from_slice(&cp.to_le_bytes());
        }
    }
    out
}

fn encode_ascii_as_utf16(bytes: &[u8], big_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        if big_endian {
            out.push(0x00);
            out.push(b);
        } else {
            out.push(b);
            out.push(0x00);
        }
    }
    out
}

fn prepend_bom(encoding: Encoding, payload: &[u8]) -> Vec<u8> {
    let bom: &[u8] = match encoding {
        Encoding::Utf16Be => &[0xFE, 0xFF],
        Encoding::Utf16Le => &[0xFF, 0xFE],
        Encoding::Utf32Be => &[0x00, 0x00, 0xFE, 0xFF],
        Encoding::Utf32Le => &[0xFF, 0xFE, 0x00, 0x00],
        Encoding::Utf8 => &[],
    };
    let mut out = Vec::with_capacity(bom.len() + payload.len());
    out.extend_from_slice(bom);
    out.extend_from_slice(payload);
    out
}

// ===========================================================================
// GAP-E1: UTF-8 BOM minimum 3-byte case (no content byte)
// ===========================================================================

#[test]
fn decode_utf8_bom_only_three_bytes_detected_as_utf8() {
    // The UTF-8 BOM [EF BB BF] with no following content. Detection must
    // recognise this as UTF-8 and strip the BOM, yielding an empty string.
    let input: &[u8] = &[0xEF, 0xBB, 0xBF];
    assert_eq!(detect_encoding(input), Encoding::Utf8);
    assert_eq!(decode(input).unwrap(), "");
}

// ===========================================================================
// GAP-E3: BOM-less UTF-16 LE odd-length input → TruncatedUtf16
// ===========================================================================

#[test]
fn decode_bomless_utf16_le_odd_length_returns_truncated_utf16() {
    // [0x41, 0x00, 0x42] — detected as UTF-16 LE by the null-byte heuristic
    // (byte 1 is 0x00), but has 3 bytes (odd), so decode must return
    // TruncatedUtf16.
    use rlsp_yaml_parser::encoding::EncodingError;
    let input: &[u8] = &[0x41, 0x00, 0x42];
    assert_eq!(detect_encoding(input), Encoding::Utf16Le);
    assert_eq!(decode(input), Err(EncodingError::TruncatedUtf16));
}

// ===========================================================================
// GAP-P5: encoding proptest extended to non-ASCII unicode scalars
// ===========================================================================

proptest! {
    #[test]
    fn encoding_choice_invariant_for_nonascii_utf8_scalars(
        ch in proptest::char::range('\u{0080}', '\u{07FF}')
    ) {
        // 2-byte UTF-8 codepoints (U+0080..=U+07FF). Parse as a double-quoted
        // scalar value so the parser sees the character as nb-json content.
        let yaml = format!("key: \"{ch}\"\n");
        // Must parse without error and produce a scalar containing the character.
        let events: Vec<_> = parse_events(yaml.as_str())
            .map(|r| r.map(|(e, _)| e))
            .collect();
        prop_assert!(
            events.iter().all(Result::is_ok),
            "parse error for char U+{:04X}: {:?}",
            u32::from(ch),
            events
        );
        let has_scalar = events.iter().any(|r| {
            matches!(r, Ok(Event::Scalar { value, .. }) if value.contains(ch))
        });
        prop_assert!(
            has_scalar,
            "expected scalar containing U+{:04X}",
            u32::from(ch)
        );
    }
}

proptest! {
    #[test]
    fn encoding_choice_invariant_under_parse(
        yaml_str in "[a-z]{1,6}: [0-9]{1,6}\n"
    ) {
        prop_assume!(!yaml_str.is_empty());

        let utf8_events: Vec<_> = parse_events(yaml_str.as_str())
            .map(|r| r.map(|(e, _)| e))
            .collect();

        let cases: &[(Encoding, Vec<u8>, bool)] = &[
            (Encoding::Utf16Be, encode_ascii_as_utf16(yaml_str.as_bytes(), true), false),
            (Encoding::Utf16Be, encode_ascii_as_utf16(yaml_str.as_bytes(), true), true),
            (Encoding::Utf16Le, encode_ascii_as_utf16(yaml_str.as_bytes(), false), false),
            (Encoding::Utf16Le, encode_ascii_as_utf16(yaml_str.as_bytes(), false), true),
            (Encoding::Utf32Be, encode_ascii_as_utf32(yaml_str.as_bytes(), true), false),
            (Encoding::Utf32Be, encode_ascii_as_utf32(yaml_str.as_bytes(), true), true),
            (Encoding::Utf32Le, encode_ascii_as_utf32(yaml_str.as_bytes(), false), false),
            (Encoding::Utf32Le, encode_ascii_as_utf32(yaml_str.as_bytes(), false), true),
        ];

        for (encoding, payload, include_bom) in cases {
            let bytes = if *include_bom {
                prepend_bom(*encoding, payload)
            } else {
                payload.clone()
            };
            let decoded = decode(&bytes).unwrap_or_else(|e| {
                panic!("decode failed for {encoding:?} bom={include_bom}: {e}");
            });
            let events: Vec<_> = parse_events(&decoded)
                .map(|r| r.map(|(e, _)| e))
                .collect();
            prop_assert_eq!(
                &events,
                &utf8_events,
                "encoding {:?} bom={} parse events differ from UTF-8",
                encoding,
                include_bom
            );
        }
    }
}
