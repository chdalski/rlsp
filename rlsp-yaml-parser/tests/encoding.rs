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
// BOM-only lines in a document prefix (YAML 1.2.2 [202], [211])
// ===========================================================================
//
// [202] l-document-prefix = c-byte-order-mark? l-comment*, and a line of
// optional blanks and a line break is an l-comment, so a line that holds only
// a BOM (with or without blanks) is a complete prefix.  [211] l-yaml-stream
// allows several prefixes in a row.

/// One line per event: its kind, its content where it matters, and its span.
fn events_with_spans(input: &str) -> Vec<String> {
    parse_events(input)
        .map(|item| {
            let (event, span) = match item {
                Ok(parsed) => parsed,
                Err(error) => unreachable!("unexpected parse error: {error}"),
            };
            let text = match event {
                Event::StreamStart => "StreamStart".to_owned(),
                Event::StreamEnd => "StreamEnd".to_owned(),
                Event::DocumentStart {
                    explicit, version, ..
                } => format!("DocumentStart explicit={explicit} version={version:?}"),
                Event::DocumentEnd { explicit } => format!("DocumentEnd explicit={explicit}"),
                Event::MappingStart { .. } => "MappingStart".to_owned(),
                Event::MappingEnd => "MappingEnd".to_owned(),
                Event::SequenceStart { .. } => "SequenceStart".to_owned(),
                Event::SequenceEnd => "SequenceEnd".to_owned(),
                Event::Scalar { value, .. } => format!("Scalar {value}"),
                Event::Alias { name } => format!("Alias {name}"),
                Event::Comment { text } => format!("Comment {text}"),
            };
            format!("{text} {}..{}", span.start, span.end)
        })
        .collect()
}

#[rstest]
#[case::bom_only_no_line_break("\u{FEFF}", &["StreamStart 0..0", "StreamEnd 3..3"])]
#[case::bom_only_line("\u{FEFF}\n", &["StreamStart 0..0", "StreamEnd 4..4"])]
#[case::bom_and_spaces("\u{FEFF}  \n", &["StreamStart 0..0", "StreamEnd 6..6"])]
#[case::bom_and_tab("\u{FEFF}\t\n", &["StreamStart 0..0", "StreamEnd 5..5"])]
#[case::bom_line_crlf("\u{FEFF}\r\n", &["StreamStart 0..0", "StreamEnd 5..5"])]
#[case::two_bom_lines("\u{FEFF}\n\u{FEFF}\n", &["StreamStart 0..0", "StreamEnd 8..8"])]
#[case::bom_lines_around_a_blank_line("\u{FEFF}\n\n\u{FEFF}  \n", &["StreamStart 0..0", "StreamEnd 11..11"])]
#[case::bom_line_then_comment_line("\u{FEFF}\n# only\n", &["StreamStart 0..0", "Comment  only 4..10", "StreamEnd 11..11"])]
#[case::bom_line_after_document_end_marker_crlf("a\n...\n\u{FEFF}\r\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 0..0",
    "Scalar a 0..1",
    "DocumentEnd explicit=true 2..5",
    "StreamEnd 11..11",
])]
#[case::bom_line_after_document_end_marker_at_end_of_input("a\n...\n\u{FEFF}", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 0..0",
    "Scalar a 0..1",
    "DocumentEnd explicit=true 2..5",
    "StreamEnd 9..9",
])]
#[case::bom_line_before_a_bare_document("\u{FEFF}\nk: v\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 4..4",
    "MappingStart 4..4",
    "Scalar k 4..5",
    "Scalar v 7..8",
    "MappingEnd 9..9",
    "DocumentEnd explicit=false 9..9",
    "StreamEnd 9..9",
])]
#[case::bom_line_crlf_before_a_bare_document("\u{FEFF}\r\nk: v\r\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 5..5",
    "MappingStart 5..5",
    "Scalar k 5..6",
    "Scalar v 8..9",
    "MappingEnd 11..11",
    "DocumentEnd explicit=false 11..11",
    "StreamEnd 11..11",
])]
#[case::bom_and_spaces_line_before_a_bare_document("\u{FEFF}  \nk: v\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 6..6",
    "MappingStart 6..6",
    "Scalar k 6..7",
    "Scalar v 9..10",
    "MappingEnd 11..11",
    "DocumentEnd explicit=false 11..11",
    "StreamEnd 11..11",
])]
#[case::two_bom_lines_before_a_bare_document("\u{FEFF}\n\u{FEFF}\nk: v\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 8..8",
    "MappingStart 8..8",
    "Scalar k 8..9",
    "Scalar v 11..12",
    "MappingEnd 13..13",
    "DocumentEnd explicit=false 13..13",
    "StreamEnd 13..13",
])]
#[case::bom_line_then_comment_before_a_bare_document("\u{FEFF}\n# c\nk: v\n", &[
    "StreamStart 0..0",
    "Comment  c 4..7",
    "DocumentStart explicit=false version=None 8..8",
    "MappingStart 8..8",
    "Scalar k 8..9",
    "Scalar v 11..12",
    "MappingEnd 13..13",
    "DocumentEnd explicit=false 13..13",
    "StreamEnd 13..13",
])]
#[case::comment_then_bom_line_before_a_bare_document("# c\n\u{FEFF}\nk: v\n", &[
    "StreamStart 0..0",
    "Comment  c 0..3",
    "DocumentStart explicit=false version=None 8..8",
    "MappingStart 8..8",
    "Scalar k 8..9",
    "Scalar v 11..12",
    "MappingEnd 13..13",
    "DocumentEnd explicit=false 13..13",
    "StreamEnd 13..13",
])]
#[case::bom_line_before_directives_end_marker("\u{FEFF}\n---\nk: v\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=true version=None 4..7",
    "MappingStart 8..8",
    "Scalar k 8..9",
    "Scalar v 11..12",
    "MappingEnd 13..13",
    "DocumentEnd explicit=false 13..13",
    "StreamEnd 13..13",
])]
#[case::bom_line_before_a_directive("\u{FEFF}\n%YAML 1.2\n---\nk\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=true version=Some((1, 2)) 14..17",
    "Scalar k 18..19",
    "DocumentEnd explicit=false 20..20",
    "StreamEnd 20..20",
])]
#[case::bom_line_between_documents("a: 1\n...\n\u{FEFF}\nb: 2\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 0..0",
    "MappingStart 0..0",
    "Scalar a 0..1",
    "Scalar 1 3..4",
    "MappingEnd 5..5",
    "DocumentEnd explicit=true 5..8",
    "DocumentStart explicit=false version=None 13..13",
    "MappingStart 13..13",
    "Scalar b 13..14",
    "Scalar 2 16..17",
    "MappingEnd 18..18",
    "DocumentEnd explicit=false 18..18",
    "StreamEnd 18..18",
])]
#[case::bom_line_between_documents_before_directives_end_marker("a\n...\n\u{FEFF}\n---\nb\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 0..0",
    "Scalar a 0..1",
    "DocumentEnd explicit=true 2..5",
    "DocumentStart explicit=true version=None 10..13",
    "Scalar b 14..15",
    "DocumentEnd explicit=false 16..16",
    "StreamEnd 16..16",
])]
#[case::two_bom_lines_between_documents("a\n...\n\u{FEFF}\n\u{FEFF}\nb\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 0..0",
    "Scalar a 0..1",
    "DocumentEnd explicit=true 2..5",
    "DocumentStart explicit=false version=None 14..14",
    "Scalar b 14..15",
    "DocumentEnd explicit=false 16..16",
    "StreamEnd 16..16",
])]
#[case::comment_then_bom_line_between_documents("a\n...\n# c\n\u{FEFF}\nb\n", &[
    "StreamStart 0..0",
    "DocumentStart explicit=false version=None 0..0",
    "Scalar a 0..1",
    "DocumentEnd explicit=true 2..5",
    "Comment  c 6..9",
    "DocumentStart explicit=false version=None 14..14",
    "Scalar b 14..15",
    "DocumentEnd explicit=false 16..16",
    "StreamEnd 16..16",
])]
fn parse_events_skips_a_line_that_holds_only_a_bom(#[case] input: &str, #[case] expected: &[&str]) {
    assert_eq!(events_with_spans(input), expected, "input {input:?}");
}

#[test]
fn parse_events_pins_bom_between_directive_and_directives_end_marker_as_kept_deviation() {
    // [209] l-directive-document allows no l-document-prefix between the
    // directive and `---`, so a BOM there is not valid YAML 1.2.2.  The parser
    // accepts it today; the plan's Decisions entry "Kept deviations" keeps that
    // behavior for a later plan of the parser's conformance bugs.
    assert_eq!(
        events_with_spans("%YAML 1.2\n\u{FEFF}---\nk\n"),
        [
            "StreamStart 0..0",
            "DocumentStart explicit=true version=Some((1, 2)) 13..16",
            "Scalar k 17..18",
            "DocumentEnd explicit=false 19..19",
            "StreamEnd 19..19",
        ]
    );
}

#[test]
fn parse_events_pins_bom_only_line_after_directive_and_comment_as_kept_deviation() {
    // Same deviation from [209] as above, reached through a comment line that
    // follows the directive: the parser has always accepted it, and the plan's
    // Decisions entry "Kept deviations" keeps that behavior.
    assert_eq!(
        events_with_spans("%YAML 1.2\n# c\n\u{FEFF}\n---\nk\n"),
        [
            "StreamStart 0..0",
            "Comment  c 10..13",
            "DocumentStart explicit=true version=Some((1, 2)) 18..21",
            "Scalar k 22..23",
            "DocumentEnd explicit=false 24..24",
            "StreamEnd 24..24",
        ]
    );
}

#[test]
fn parse_events_pins_bom_between_documents_without_end_marker_as_kept_deviation() {
    // [211] l-yaml-stream allows a BOM there, but the plan's Decisions entry
    // "Kept deviations" keeps the error for a later plan of the parser's
    // conformance bugs.
    assert!(has_parse_error("a\n\u{FEFF}\n---\nb\n"));
}

#[rstest]
#[case::in_the_document_body("k: v\n\u{FEFF}\nz: 1\n")]
#[case::after_the_directives_end_marker("---\n\u{FEFF}\nk: v\n")]
#[case::two_boms_on_the_line_before_a_bare_document("a\n...\n\u{FEFF}\u{FEFF}\nb\n")]
// [209] l-directive-document allows no l-document-prefix between a directive
// and `---`, so a BOM-only line there is an error, as it was before the
// BOM-only line was skipped in a prefix.
#[case::after_a_yaml_directive("%YAML 1.2\n\u{FEFF}\n---\nk\n")]
#[case::after_a_yaml_directive_and_blanks("%YAML 1.2\n\u{FEFF}  \n---\nk\n")]
#[case::after_a_yaml_directive_and_a_blank_line("%YAML 1.2\n\n\u{FEFF}\n---\nk\n")]
#[case::after_a_tag_directive("%TAG !e! tag:x,1:\n\u{FEFF}\n---\nk\n")]
fn bom_only_line_that_is_not_in_a_document_prefix_stays_an_error(#[case] input: &str) {
    assert!(has_parse_error(input), "events of {input:?}");
    assert!(rlsp_yaml_parser::load(input).is_err(), "load of {input:?}");
}

#[test]
fn bom_after_blanks_on_a_line_is_never_silently_dropped() {
    // Only the start of a line is a document-prefix position, so the skip of
    // a BOM-only line must not strip a BOM that follows blanks.  The BOM is no
    // ns-char ([27] nb-char excludes c-byte-order-mark), so an error is the
    // spec's answer; the test accepts an error and rejects a dropped BOM.
    assert!(!scalar_values("\u{FEFF}\n  \u{FEFF}k: v\n").contains(&"k".to_owned()));
}

#[rstest]
#[case::bom_only_no_line_break("\u{FEFF}")]
#[case::bom_and_spaces("\u{FEFF}  \n")]
#[case::bom_and_tab("\u{FEFF}\t\n")]
#[case::two_bom_lines("\u{FEFF}\n\u{FEFF}\n")]
fn load_returns_no_document_for_bom_only_lines(#[case] input: &str) {
    let docs = rlsp_yaml_parser::load(input).expect("load must succeed");
    assert!(docs.is_empty(), "documents of {input:?}: {docs:?}");
}

/// The spans of the single key and the single value of a one-entry mapping.
fn key_and_value_spans(node: &rlsp_yaml_parser::Node) -> ((u32, u32), (u32, u32)) {
    let rlsp_yaml_parser::Node::Mapping { entries, .. } = node else {
        unreachable!("expected a mapping, got {node:?}");
    };
    let [(key, value)] = entries.as_slice() else {
        unreachable!("expected one entry, got {entries:?}");
    };
    let span = |node: &rlsp_yaml_parser::Node| match node {
        rlsp_yaml_parser::Node::Scalar { loc, .. } => (loc.start, loc.end),
        other => unreachable!("expected a scalar, got {other:?}"),
    };
    (span(key), span(value))
}

#[rstest]
#[case::bom_line("\u{FEFF}\nk: v\n", ((4, 5), (7, 8)))]
#[case::bom_line_crlf("\u{FEFF}\r\nk: v\r\n", ((5, 6), (8, 9)))]
#[case::two_bom_lines("\u{FEFF}\n\u{FEFF}\nk: v\n", ((8, 9), (11, 12)))]
#[case::bom_line_then_comment("\u{FEFF}\n# c\nk: v\n", ((8, 9), (11, 12)))]
#[case::bom_line_before_directives_end_marker("\u{FEFF}\n---\nk: v\n", ((8, 9), (11, 12)))]
fn load_places_the_document_after_a_bom_only_line(
    #[case] input: &str,
    #[case] expected: ((u32, u32), (u32, u32)),
) {
    let docs = rlsp_yaml_parser::load(input).expect("load must succeed");
    assert_eq!(docs.len(), 1, "documents of {input:?}");
    assert_eq!(key_and_value_spans(&docs[0].root), expected, "{input:?}");
}

#[test]
fn load_reads_the_directive_after_a_bom_only_line() {
    let docs = rlsp_yaml_parser::load("\u{FEFF}\n%YAML 1.2\n---\nk\n").expect("load must succeed");
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].version, Some((1, 2)));
}

#[test]
fn load_reads_the_document_after_a_bom_only_line_between_documents() {
    let docs = rlsp_yaml_parser::load("a: 1\n...\n\u{FEFF}\nb: 2\n").expect("load must succeed");
    assert_eq!(docs.len(), 2);
    assert_eq!(key_and_value_spans(&docs[0].root), ((0, 1), (3, 4)));
    assert_eq!(key_and_value_spans(&docs[1].root), ((13, 14), (16, 17)));
}

#[test]
fn load_reads_the_directive_of_the_next_document_after_a_bom_only_line() {
    // The directive count of the first document must not leak into the prefix
    // of the second one.
    let docs = rlsp_yaml_parser::load("%YAML 1.2\n---\nk\n...\n\u{FEFF}\n%YAML 1.2\n---\nj\n")
        .expect("load must succeed");
    assert_eq!(docs.len(), 2);
}

#[test]
fn load_ends_after_a_bom_only_line_that_follows_the_last_document_end_marker() {
    let docs = rlsp_yaml_parser::load("a\n...\n\u{FEFF}").expect("load must succeed");
    assert_eq!(docs.len(), 1);
}

#[rstest]
#[case::bom_line_then_comment("\u{FEFF}\n# c\nk: v\n")]
#[case::bom_line_before_directives_end_marker("\u{FEFF}\n---\nk: v\n")]
#[case::bom_line_before_a_directive("\u{FEFF}\n%YAML 1.2\n---\nk\n")]
fn load_returns_one_document_after_a_bom_only_line(#[case] input: &str) {
    let docs = rlsp_yaml_parser::load(input).expect("load must succeed");
    assert_eq!(docs.len(), 1, "documents of {input:?}");
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
// decode() — BOM-less inputs at exactly one code-unit's length
// ===========================================================================
//
// Complements detect_encoding_null_byte_heuristic / _exactly_four_bytes_*
// (which only assert detection) by also asserting the decoded value through
// the public decode() entry point.

#[rstest]
#[case::le(&[0x41u8, 0x00] as &[u8], "A")]
#[case::be(&[0x00u8, 0x41] as &[u8], "A")]
fn decode_bomless_utf16_exactly_two_bytes_single_unit(
    #[case] input: &[u8],
    #[case] expected: &str,
) {
    assert_eq!(decode(input).unwrap(), expected);
}

#[rstest]
#[case::le(&[0x41u8, 0x00, 0x00, 0x00] as &[u8], "A")]
#[case::be(&[0x00u8, 0x00, 0x00, 0x41] as &[u8], "A")]
fn decode_bomless_utf32_exactly_four_bytes_single_codepoint(
    #[case] input: &[u8],
    #[case] expected: &str,
) {
    assert_eq!(decode(input).unwrap(), expected);
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
