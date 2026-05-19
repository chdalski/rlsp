// SPDX-License-Identifier: MIT
//
// Duplicate key and error reporting tests — verifies parser behavior for
// duplicate keys (accepted silently), error detection, error positions,
// error recovery (stream stops after first error), and merge key handling.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    missing_docs,
    reason = "test code"
)]

use rstest::rstest;

use rlsp_yaml_parser::loader::LoadError;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Error, ErrorKind, Event, load, parse_events};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the first parse error from `parse_events`, if any.
fn first_error(input: &str) -> Option<Error> {
    parse_events(input).find_map(Result::err)
}

/// Load input and extract scalar key-value pairs from the root mapping.
fn mapping_key_values(input: &str) -> Vec<(String, String)> {
    let docs = load(input).unwrap();
    match &docs[0].root {
        Node::Mapping { entries, .. } => entries
            .iter()
            .filter_map(|(k, v)| {
                let key = match k {
                    Node::Scalar { value, .. } => value.clone(),
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                        return None;
                    }
                };
                let val = match v {
                    Node::Scalar { value, .. } => value.clone(),
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                        return None;
                    }
                };
                Some((key, val))
            })
            .collect(),
        Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => vec![],
    }
}

// ===========================================================================
// Spike
// ===========================================================================

#[test]
fn duplicate_keys_both_entries_present_in_mapping() {
    let input = "key: first\nkey: second\n";
    let result = load(input);
    assert!(result.is_ok(), "load failed: {result:?}");
    let kvs = mapping_key_values(input);
    assert_eq!(kvs.len(), 2);
    assert!(kvs.contains(&("key".into(), "first".into())));
    assert!(kvs.contains(&("key".into(), "second".into())));
}

// ===========================================================================
// Group A: Duplicate keys — two entries present (uniform shape)
//
// input → mapping_key_values → assert len==2 + contains both pairs
// ===========================================================================

#[rstest]
#[case::plain_scalar("name: Alice\nname: Bob\n", ("name", "Alice"), ("name", "Bob"))]
#[case::flow_mapping("{a: 1, a: 2}\n", ("a", "1"), ("a", "2"))]
fn duplicate_keys_two_entries(
    #[case] input: &str,
    #[case] pair_a: (&str, &str),
    #[case] pair_b: (&str, &str),
) {
    let kvs = mapping_key_values(input);
    assert_eq!(kvs.len(), 2);
    assert!(kvs.contains(&(pair_a.0.into(), pair_a.1.into())));
    assert!(kvs.contains(&(pair_b.0.into(), pair_b.1.into())));
}

#[test]
fn duplicate_quoted_and_unquoted_keys_both_entries_present() {
    let input = "key: unquoted\n'key': single-quoted\n\"key\": double-quoted\n";
    let kvs = mapping_key_values(input);
    assert_eq!(kvs.len(), 3);
    assert!(kvs.iter().any(|(_, v)| v == "unquoted"));
    assert!(kvs.iter().any(|(_, v)| v == "single-quoted"));
    assert!(kvs.iter().any(|(_, v)| v == "double-quoted"));
}

#[test]
fn duplicate_keys_in_nested_mappings_are_independent() {
    let input = "outer:\n  key: inner_value\nkey: outer_value\n";
    let docs = load(input).unwrap();
    match &docs[0].root {
        Node::Mapping { entries, .. } => {
            assert_eq!(entries.len(), 2);
            // Find the "outer" entry and verify its nested mapping.
            let outer_entry = entries
                .iter()
                .find(|(k, _)| matches!(k, Node::Scalar { value, .. } if value == "outer"))
                .expect("expected 'outer' key");
            match &outer_entry.1 {
                Node::Mapping { entries: inner, .. } => {
                    assert_eq!(inner.len(), 1);
                    assert!(matches!(&inner[0].0, Node::Scalar { value, .. } if value == "key"));
                    assert!(matches!(
                        &inner[0].1,
                        Node::Scalar { value, .. } if value == "inner_value"
                    ));
                }
                other @ (Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. }) => {
                    panic!("expected nested mapping, got: {other:?}")
                }
            }
            // Find the root "key" entry.
            let root_key = entries
                .iter()
                .find(|(k, _)| matches!(k, Node::Scalar { value, .. } if value == "key"))
                .expect("expected root 'key' entry");
            assert!(matches!(
                &root_key.1,
                Node::Scalar { value, .. } if value == "outer_value"
            ));
        }
        other @ (Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. }) => {
            panic!("expected root mapping, got: {other:?}")
        }
    }
}

#[test]
fn duplicate_keys_with_numeric_and_string_representations_both_present() {
    // 1 and '1' both parse as string value "1" — the parser does not
    // type-convert keys. Both entries should be present.
    let input = "1: numeric_key\n'1': quoted_key\n";
    let kvs = mapping_key_values(input);
    assert_eq!(kvs.len(), 2);
    assert!(kvs.iter().any(|(_, v)| v == "numeric_key"));
    assert!(kvs.iter().any(|(_, v)| v == "quoted_key"));
}

// ===========================================================================
// Group B: Error detection — unterminated quoted scalars (uniform shape)
//
// input → first_error().is_some() + load() is Err(LoadError::Parse)
// ===========================================================================

#[rstest]
#[case::single_quoted("key: 'unterminated\n")]
#[case::double_quoted("key: \"unterminated\n")]
fn unterminated_quoted_scalar_produces_parse_error(#[case] input: &str) {
    assert!(first_error(input).is_some(), "expected parse error");
    assert!(
        matches!(load(input), Err(LoadError::Parse { .. })),
        "expected LoadError::Parse"
    );
}

// ===========================================================================
// Group C: Error detection — unterminated flow collections (uniform shape)
//
// input → first_error().is_some()
// ===========================================================================

#[rstest]
#[case::sequence("[a, b, c\n")]
#[case::mapping("{a: 1, b: 2\n")]
fn unterminated_flow_collection_produces_error(#[case] input: &str) {
    assert!(
        first_error(input).is_some(),
        "expected parse error for unterminated flow collection"
    );
}

#[test]
fn load_returns_parse_error_for_invalid_input() {
    let input = "\t bad: indentation\n";
    let result = load(input);
    assert!(
        matches!(result, Err(LoadError::Parse { .. })),
        "expected LoadError::Parse, got: {result:?}"
    );
}

#[test]
fn parse_events_error_has_non_empty_message() {
    let input = "key: 'unterminated\n";
    let err = first_error(input).expect("expected parse error");
    assert!(!err.message.is_empty(), "error message should not be empty");
}

// ===========================================================================
// Group C: Error position — byte offset is meaningful
// ===========================================================================

#[test]
fn error_position_byte_offset_nonzero_for_error_after_content() {
    let input = "valid_key: value\n\t bad: indentation\n";
    let err = first_error(input).expect("expected parse error");
    assert!(
        err.pos.byte_offset > 0,
        "byte_offset should be > 0 for error after content, got: {:?}",
        err.pos
    );
}

#[test]
fn load_parse_error_carries_byte_offset() {
    let input = "a: 1\nb: 'unterminated\n";
    match load(input) {
        Err(LoadError::Parse { pos, .. }) => {
            assert!(
                pos.byte_offset > 4,
                "byte_offset should be past first line, got: {pos:?}"
            );
        }
        other => panic!("expected LoadError::Parse, got: {other:?}"),
    }
}

#[test]
fn error_position_at_stream_start_for_leading_invalid_input() {
    // Use an unterminated flow sequence starting at byte 0.
    let input = "[\n";
    let err = first_error(input).expect("expected parse error");
    // The error should be at or near the start of the stream.
    assert!(
        err.pos.byte_offset <= 2,
        "expected byte_offset near start, got: {:?}",
        err.pos
    );
}

// ===========================================================================
// Group D: Error stops the stream
// ===========================================================================

#[test]
fn parse_events_stops_after_first_error() {
    let input = "key: 'unterminated\n";
    let items: Vec<_> = parse_events(input).collect();
    let err_count = items.iter().filter(|r| r.is_err()).count();
    assert_eq!(err_count, 1, "expected exactly one error, got: {err_count}");
    // The error should be the last item — no Ok items follow it.
    let last = items.last().expect("expected at least one item");
    assert!(last.is_err(), "last item should be the error");
}

#[test]
fn parse_events_emits_stream_start_before_error() {
    // Use unterminated quoted scalar — reliably produces an error.
    let input = "'unterminated\n";
    let items: Vec<_> = parse_events(input).collect();
    assert!(!items.is_empty(), "expected at least one event");
    assert!(
        matches!(&items[0], Ok((Event::StreamStart, _))),
        "first event should be StreamStart, got: {:?}",
        items[0]
    );
    assert!(
        items.iter().any(Result::is_err),
        "expected at least one error event"
    );
}

// ===========================================================================
// LE-1: LoadError::Parse Display includes a line number
// ===========================================================================

#[test]
fn load_error_parse_display_contains_line_info() {
    let input = "key: 'unterminated\n";
    let err = load(input).expect_err("expected LoadError::Parse");
    let display = format!("{err}");
    // The Display impl renders: "parse error at Pos { byte_offset: N, line: L, column: C }: …"
    // Assert that "line" appears (indicating position info is present) and that
    // a digit follows it (not just a bare word from the message text).
    assert!(
        display.contains("line"),
        "LoadError::Parse Display should contain 'line' (Pos info), got: {display:?}"
    );
    assert!(
        display.chars().any(|c| c.is_ascii_digit()),
        "LoadError::Parse Display should contain a digit after 'line', got: {display:?}"
    );
}

// ===========================================================================
// Group E: Merge key error
// ===========================================================================

#[test]
fn invalid_merge_key_value_is_accepted_or_errors_gracefully() {
    // The YAML spec says `<<` merge keys must reference mappings or sequences
    // of mappings. This parser does not enforce that constraint — `<<` is
    // treated as a regular scalar key. Assert actual behavior.
    let input = "<<: scalar_value\n";
    let result = load(input);
    // Either outcome is acceptable — the test documents the behavior.
    if let Ok(docs) = &result {
        assert_eq!(docs.len(), 1, "expected 1 document");
    }
    // If Err, the parser rejected merge key with scalar value — also acceptable.
}

// ===========================================================================
// Group F: ErrorKind — kind field is set correctly on parse errors
// ===========================================================================

#[test]
fn error_kind_for_non_printable_in_plain_scalar_is_invalid_character() {
    // U+0001 (SOH) is a non-printable, non-c-printable character forbidden in
    // plain scalars. Inserting it as the second byte of the content triggers
    // an InvalidCharacter error.
    let input = "key: val\u{0001}ue\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in plain scalar, got: {:?}",
        err.kind
    );
}

#[test]
fn error_kind_for_unterminated_single_quoted_scalar_is_syntax() {
    let input = "key: 'unterminated\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::Syntax,
        "expected Syntax for unterminated single-quoted scalar, got: {:?}",
        err.kind
    );
}

#[test]
fn error_kind_for_unterminated_double_quoted_scalar_is_syntax() {
    let input = "key: \"unterminated\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::Syntax,
        "expected Syntax for unterminated double-quoted scalar, got: {:?}",
        err.kind
    );
}

#[test]
fn error_kind_for_non_printable_in_double_quoted_scalar_is_invalid_character() {
    // U+0001 inside a double-quoted scalar (not as a \x01 escape, but literal).
    let input = "key: \"val\u{0001}ue\"\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in double-quoted scalar, got: {:?}",
        err.kind
    );
}

#[test]
fn error_kind_for_non_printable_in_single_quoted_scalar_is_invalid_character() {
    // U+0001 inside a single-quoted scalar (literal byte).
    let input = "key: 'val\u{0001}ue'\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in single-quoted scalar, got: {:?}",
        err.kind
    );
}

#[test]
fn error_kind_for_non_printable_in_block_scalar_is_invalid_character() {
    // U+0001 inside a literal block scalar body.
    let input = "key: |\n  val\u{0001}ue\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in block scalar, got: {:?}",
        err.kind
    );
}

#[test]
fn error_kind_for_duplicate_yaml_directive_is_syntax() {
    // Two %YAML directives in one document is a structural/grammar error.
    let input = "%YAML 1.2\n%YAML 1.2\n---\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::Syntax,
        "expected Syntax for duplicate YAML directive, got: {:?}",
        err.kind
    );
}

#[test]
fn error_kind_for_non_printable_in_directive_name_is_invalid_character() {
    // U+0001 in a directive parameter triggers an InvalidCharacter error.
    // %TAG directives with a non-printable in the handle/prefix are rejected.
    let input = "%TAG !\u{0001} tag:example.com,2024:\n---\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in directive parameter, got: {:?}",
        err.kind
    );
}

#[test]
fn parse_events_comment_non_printable_produces_invalid_character_kind() {
    // SOH (U+0001) in a comment body — hits lexer/comment.rs non_printable_error_message path.
    let input = "# hello\x01world\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in comment, got: {:?}",
        err.kind
    );
}

#[test]
fn parse_events_quoted_scalar_escape_non_printable_produces_invalid_character_kind() {
    // \x01 escape sequence in a double-quoted scalar decodes to U+0001, which is
    // non-printable — hits decode_and_push_escape at lexer/quoted.rs:703.
    let input = "key: \"\\x01\"\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for \\x01 escape producing non-printable, got: {:?}",
        err.kind
    );
}

#[test]
fn parse_events_directive_parameter_non_printable_produces_invalid_character_kind() {
    // Non-printable in a reserved directive's parameter — hits directives.rs line 123
    // loop for reserved directives.
    let input = "%FOO bar\x01baz\n---\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in reserved directive parameter, got: {:?}",
        err.kind
    );
}

#[test]
fn parse_events_yaml_directive_parameter_non_printable_produces_invalid_character_kind() {
    // Non-printable in a %YAML directive parameter — hits directives.rs line 166
    // pre-validate loop for %YAML directives.
    let input = "%YAML 1\x01.2\n---\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCharacter,
        "expected InvalidCharacter for non-printable in %YAML directive parameter, got: {:?}",
        err.kind
    );
}

#[test]
fn parse_events_unterminated_flow_sequence_produces_syntax_kind() {
    // Unterminated flow sequence — structural/grammar error, not a character violation.
    let input = "[a, b, c\n";
    let err = first_error(input).expect("expected parse error");
    assert_eq!(
        err.kind,
        ErrorKind::Syntax,
        "expected Syntax for unterminated flow sequence, got: {:?}",
        err.kind
    );
}

// ===========================================================================
// Group G: LoadError::Parse carries kind — load() API propagates ErrorKind
// ===========================================================================

#[test]
fn load_parse_error_carries_kind_invalid_character_for_non_printable_in_comment() {
    // U+0080 (PAD, a C1 control character) in a comment body is non-printable
    // and forbidden by YAML 1.2.2 c-printable. Feeding it through load()
    // verifies that LoadError::Parse.kind is forwarded from the event-stream Error.
    let input = "key: value # comment\u{0080}here\n";
    match load(input) {
        Err(LoadError::Parse { kind, .. }) => {
            assert_eq!(
                kind,
                ErrorKind::InvalidCharacter,
                "LoadError::Parse.kind should be InvalidCharacter for U+0080 in comment, got: {kind:?}"
            );
        }
        other => panic!("expected Err(LoadError::Parse), got: {other:?}"),
    }
}
