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
use rlsp_yaml_parser::{Error, ErrorKind, Event, LoaderBuilder, load, parse_events};

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

// ===========================================================================
// Group H: Content after a complete node
//
// A sequence item, mapping value or explicit key holds exactly one node.
// A line indented past the collection after that node is complete is a
// syntax error, not a second node and not part of the first.
// ===========================================================================

/// Scalar values of every `Event::Scalar` in document order; panics on a parse error.
fn scalar_values(input: &str) -> Vec<String> {
    parse_events(input)
        .filter_map(|item| match item.unwrap().0 {
            Event::Scalar { value, .. } => Some(value.into_owned()),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::Alias { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd => None,
        })
        .collect()
}

// input → syntax error on `stray_line` through both `parse_events` and `load`,
// and the event stream ends at that error.
#[rstest]
#[case::seq_item_quoted_then_plain("- \"q\"\n  w\n", 2)]
#[case::seq_item_plain_comment_then_plain("- v # c\n  w\n", 2)]
#[case::seq_item_plain_then_key("- v\n  k: 1\n", 2)]
#[case::map_value_quoted_then_flow("a: \"q\"\n  [x]\n", 2)]
#[case::map_value_quoted_then_alias("a: \"q\"\n  *y\n", 2)]
#[case::map_value_plain_comment_then_flow("a: v # c\n  [x]\n", 2)]
#[case::explicit_key_plain_comment_then_plain("? k # c\n  w\n: v\n", 2)]
#[case::seq_item_after_comment_line("- \"q\"\n  # c\n  w\n", 3)]
#[case::seq_item_single_quoted("- 'q'\n  w\n", 2)]
#[case::seq_item_two_quoted("- \"q\"\n  \"r\"\n", 2)]
#[case::seq_item_multiline_quoted("- \"a\n  b\"\n  c\n", 3)]
#[case::seq_item_tag_line_then_quoted("- !!str\n  \"q\"\n  w\n", 3)]
#[case::seq_item_anchor_line_then_quoted("- &a\n  \"q\"\n  w\n", 3)]
#[case::seq_item_flow_seq_then_plain("- [x]\n  y\n", 2)]
#[case::seq_item_anchored_flow_seq_then_plain("- &a [x]\n  y\n", 2)]
#[case::seq_item_flow_map_then_plain("- {a: 1}\n  b\n", 2)]
#[case::seq_item_flow_map_then_flow_seq("- {a: 1}\n  [x]\n", 2)]
#[case::seq_item_flow_seq_comment_then_plain("- [x] # c\n  w\n", 2)]
#[case::seq_item_alias_then_plain("- *a\n  w\n", 2)]
#[case::seq_item_then_explicit_key("- \"q\"\n  ? w\n", 2)]
#[case::seq_item_then_block_scalar_header("- \"q\"\n  |\n", 2)]
#[case::map_value_then_anchored_plain("a: \"q\"\n  &x w\n", 2)]
#[case::map_value_then_tagged_plain("a: \"q\"\n  !!str w\n", 2)]
#[case::map_value_single_quoted("a: 'q'\n  r\n", 2)]
#[case::map_value_flow_seq_then_plain("a: [x]\n  y\n", 2)]
#[case::map_value_flow_map_then_plain("a: {x: 1}\n  y\n", 2)]
#[case::map_value_anchored_flow_seq_then_plain("a: &x [1]\n  z\n", 2)]
#[case::explicit_key_double_quoted("? \"k\"\n  w\n: v\n", 2)]
#[case::explicit_key_flow_seq("? [k]\n  w\n: v\n", 2)]
#[case::explicit_key_alias("? *a\n  w\n: v\n", 2)]
#[case::explicit_value_double_quoted("? k\n: \"v\"\n  w\n", 3)]
#[case::explicit_value_flow_seq("? k\n: [v]\n  w\n", 3)]
#[case::nested_mapping_in_seq_item("- a: \"q\"\n    [x]\n", 2)]
#[case::nested_seq_in_seq_item("- - \"q\"\n    w\n", 2)]
#[case::seq_under_mapping_indented("a:\n  - \"q\"\n    w\n", 3)]
#[case::seq_under_mapping_same_column("a:\n- \"q\"\n  w\n", 3)]
#[case::nested_mapping_value("a:\n  b: \"q\"\n    w\n", 3)]
#[case::blank_line_before_stray_seq_item("- \"q\"\n\n  w\n", 3)]
#[case::blank_line_before_stray_map_value("a: \"q\"\n\n  [x]\n", 3)]
#[case::crlf_seq_item("- \"q\"\r\n  w\r\n", 2)]
#[case::crlf_map_value("a: \"q\"\r\n  [x]\r\n", 2)]
#[case::crlf_comment_seq_item("- v # c\r\n  w\r\n", 2)]
#[case::seq_item_plain_comment_line_after_blank_then_plain("- a\n\n  # c\n  b\n", 4)]
#[case::seq_item_plain_after_blank_then_key("- a\n\n  b: 1\n", 3)]
#[case::seq_item_plain_comment_blank_then_plain("- a # c\n\n  b\n", 3)]
#[case::seq_item_plain_blank_then_sibling_key_at_dash_column("- a\n\nb: 1\n", 3)]
#[case::seq_item_then_flow_seq_at_dash_column("- \"q\"\n[x]\n", 2)]
#[case::seq_item_then_alias_at_dash_column("- \"q\"\n*y\n", 2)]
#[case::seq_item_then_anchored_plain_at_dash_column("- \"q\"\n&x w\n", 2)]
#[case::seq_item_then_tagged_plain_at_dash_column("- \"q\"\n!!str w\n", 2)]
#[case::nested_seq_item_then_flow_seq_indented("- - \"a\"\n  [x]\n", 2)]
#[case::nested_seq_item_then_flow_seq_between_levels("- - \"a\"\n [x]\n", 2)]
#[case::nested_seq_item_then_alias_between_levels("- - \"a\"\n *y\n", 2)]
#[case::nested_seq_item_then_anchored_plain_between_levels("- - \"a\"\n &x w\n", 2)]
#[case::seq_under_mapping_then_flow_seq_between_levels("a:\n  - \"q\"\n [x]\n", 3)]
#[case::seq_under_mapping_then_flow_seq_dedented_one_level("a:\n    - \"q\"\n  [x]\n", 3)]
#[case::mapping_in_seq_item_then_flow_seq_between_levels("- a: \"q\"\n [x]\n", 2)]
#[case::mapping_in_seq_item_then_flow_seq_indented("- a: \"q\"\n  [x]\n", 2)]
#[case::mapping_in_seq_item_then_alias_between_levels("- a: \"q\"\n *y\n", 2)]
#[case::mapping_in_seq_item_then_anchored_plain_between_levels("- a: \"q\"\n &x w\n", 2)]
#[case::nested_mapping_then_alias_between_levels("a:\n    b: \"q\"\n  *y\n", 3)]
#[case::nested_mapping_then_flow_map_between_levels("a:\n    b: \"q\"\n  {x: 1}\n", 3)]
#[case::map_value_then_flow_seq_at_key_column("a: \"q\"\n[x]\nb: 1\n", 2)]
#[case::map_value_then_alias_at_key_column("a: \"q\"\n*y\n", 2)]
#[case::map_value_then_anchored_plain_at_key_column("a: \"q\"\n&x w\n", 2)]
#[case::explicit_key_then_flow_seq_at_key_column("? \"a\"\n[x]\n: v\n", 2)]
#[case::explicit_key_then_alias_at_key_column("? \"a\"\n*y\n: v\n", 2)]
#[case::plain_explicit_key_then_flow_seq_at_key_column("? a\n[x]\n", 2)]
#[case::explicit_key_then_scalar_at_key_column("? \"a\"\nw\n: v\n", 2)]
#[case::explicit_entry_in_seq_item_then_flow_seq("- ? \"a\"\n [x]\n", 2)]
#[case::complex_explicit_key_mapping_then_flow_seq("? a: \"q\"\n [x]\n: v\n", 2)]
#[case::complex_explicit_key_seq_then_flow_seq("? - \"q\"\n [x]\n: v\n", 2)]
#[case::flow_key_between_levels("- a: 1\n [x]: 2\n", 2)]
#[case::alias_key_between_levels("- a: 1\n *y : 2\n", 2)]
#[case::seq_item_then_anchored_key_at_dash_column("- \"q\"\n&x c: 1\n", 2)]
#[case::seq_item_then_flow_key_at_dash_column("- \"q\"\n[x]: 1\n", 2)]
#[case::nested_seq_item_then_anchored_key_at_dash_column("- - \"q\"\n  &x c: 1\n", 2)]
#[case::dash_after_plain_explicit_key("? k\n- b\n: v\n", 2)]
#[case::dash_after_quoted_explicit_key("? \"k\"\n- b\n", 2)]
#[case::dash_after_flow_explicit_key("? [k]\n- b\n: v\n", 2)]
#[case::dash_after_alias_explicit_key("? *k\n- b\n", 2)]
#[case::dash_after_block_seq_explicit_key("? - x\n- b\n: v\n", 2)]
#[case::dash_after_block_scalar_explicit_key("? |\n  t\n- b\n: v\n", 3)]
#[case::dash_after_nested_explicit_key("a:\n  ? k\n  - b\n", 3)]
#[case::dash_after_explicit_key_in_seq_item("- ? k\n  - b\n", 2)]
#[case::dash_after_later_explicit_key("? \"k\"\n? a\n- b\n", 3)]
#[case::dash_after_explicit_value("? k\n: \"q\"\n- b\n", 3)]
#[case::dash_after_quoted_map_value("a: \"q\"\n- b\nc: 1\n", 2)]
#[case::dash_after_plain_map_value("a: 1\n- b\n", 2)]
#[case::dash_after_nested_map_value("a:\n  b: \"q\"\n  - c\n", 3)]
#[case::dash_after_map_value_in_seq_item("- a: \"q\"\n  - c\n", 2)]
#[case::dash_after_flow_map_value("a: [x]\n- b\n", 2)]
#[case::dash_after_alias_map_value("a: *y\n- b\n", 2)]
#[case::flow_seq_then_flow_seq_in_seq_item("- [x] [y]\n", 1)]
#[case::flow_seq_then_plain_in_seq_item("- [x] w\n", 1)]
#[case::flow_map_then_flow_map_in_seq_item("- {a: 1} {b: 2}\n", 1)]
#[case::alias_then_alias_in_seq_item("- *a *b\n", 1)]
#[case::alias_then_plain_in_seq_item("- *a w\n", 1)]
#[case::alias_then_flow_seq_in_seq_item("- *a [x]\n", 1)]
#[case::anchored_flow_seq_then_flow_seq_in_seq_item("- &x [a] [b]\n", 1)]
#[case::tagged_flow_seq_then_plain_in_seq_item("- !!seq [a] w\n", 1)]
#[case::flow_seq_then_flow_seq_map_value("a: [x] [y]\n", 1)]
#[case::flow_seq_then_plain_map_value("a: [x] w\n", 1)]
#[case::flow_seq_then_second_plain_word_map_value("a: [x] y\n", 1)]
#[case::flow_map_then_plain_map_value("a: {b: 1} w\n", 1)]
#[case::alias_then_alias_map_value("a: *b *c\n", 1)]
#[case::alias_then_plain_map_value("a: *b w\n", 1)]
#[case::flow_seq_then_flow_seq_explicit_key("? [x] [y]\n: v\n", 1)]
#[case::alias_then_alias_explicit_key("? *a *b\n: v\n", 1)]
#[case::alias_then_plain_explicit_key("? *a w\n: v\n", 1)]
#[case::bare_anchor_after_quoted_map_value("a: \"q\"\n&x\n", 2)]
#[case::bare_tag_after_plain_map_value("a: 1\n!!str\n", 2)]
#[case::bare_anchor_after_nested_map_value("a:\n  b: \"q\"\n  &x\n", 3)]
#[case::single_quoted_then_plain_in_seq_item("- 'q' w\n", 1)]
#[case::single_quoted_then_flow_seq_in_seq_item("- 'q' [x]\n", 1)]
#[case::single_quoted_then_single_quoted_in_seq_item("- 'q' 'r'\n", 1)]
#[case::single_quoted_then_plain_map_value("a: 'q' w\n", 1)]
#[case::single_quoted_then_single_quoted_map_value("a: 'q' 'r'\n", 1)]
#[case::single_quoted_then_plain_explicit_key("? 'q' w\n: v\n", 1)]
#[case::multiline_single_quoted_then_plain_in_seq_item("- 'a\n  b' w\n", 2)]
#[case::single_quoted_then_hash_without_space("- 'q'# c\n", 1)]
#[case::bare_anchor_in_seq_item_mapping_then_dash("- a: \"q\"\n  &x\n- b\n", 2)]
#[case::bare_anchor_then_comment_at_end("a: \"q\"\n&x\n# c\n", 2)]
#[case::bare_anchor_then_document_end("a:\n  b: \"q\"\n  &x\n---\n", 3)]
#[case::single_quoted_escaped_quote_then_plain_in_seq_item("- 'it''s' w\n", 1)]
#[case::single_quoted_then_plain_explicit_key_k("? 'k' w\n: v\n", 1)]
#[case::anchored_single_quoted_then_plain("- &a 'q' w\n", 1)]
#[case::tagged_single_quoted_then_plain("- !!str 'q' w\n", 1)]
#[case::single_quoted_then_plain_in_nested_seq_item("- - 'q' w\n", 1)]
#[case::single_quoted_then_plain_at_root("'q' w\n", 1)]
#[case::flow_seq_tab_plain_in_seq_item("- [x]\tw\n", 1)]
#[case::flow_seq_tab_flow_seq_in_seq_item("- [x]\t[y]\n", 1)]
#[case::flow_seq_tab_plain_map_value("a: [x]\tw\n", 1)]
#[case::alias_tab_plain_map_value("a: *b\tw\n", 1)]
#[case::crlf_flow_seq_then_flow_seq("- [x] [y]\r\n", 1)]
#[case::flow_seq_then_key_like_text("- [x] w: 1\n", 1)]
#[case::flow_map_then_key_like_text("- {a: 1} w: 2\n", 1)]
#[case::nested_flow_then_plain("- {a: [x]} w\n", 1)]
#[case::flow_seq_then_comma("- [x],\n", 1)]
#[case::flow_seq_then_space_comma("- [x] ,\n", 1)]
#[case::flow_seq_then_extra_close_bracket("- [x]]\n", 1)]
#[case::flow_seq_then_close_brace("- [x]}\n", 1)]
#[case::flow_seq_map_value_then_comma("a: [x],\n", 1)]
#[case::flow_seq_map_value_then_extra_close_bracket("a: [x]]\n", 1)]
#[case::flow_seq_map_value_then_adjacent_plain("a: [x]w\n", 1)]
#[case::two_item_flow_seq_then_adjacent_plain("a: [x, y]w\n", 1)]
#[case::explicit_key_flow_seq_then_adjacent_plain("? [x]w\n: v\n", 1)]
#[case::flow_seq_then_quoted("a: [x] \"q\"\n", 1)]
#[case::flow_seq_then_dash_text("a: [x] - y\n", 1)]
#[case::flow_seq_then_question_text("a: [x] ? y\n", 1)]
#[case::flow_seq_then_anchor("a: [x] &z\n", 1)]
#[case::flow_seq_then_tag("a: [x] !!str\n", 1)]
#[case::alias_then_anchor("a: *b &z\n", 1)]
#[case::alias_name_with_colon_then_value("- *a: 1\n", 1)]
#[case::alias_name_with_colon_and_text_then_plain("- *a:b w\n", 1)]
#[case::alias_name_with_inner_colon_then_plain("a: *b:c w\n", 1)]
#[case::multiline_flow_seq_then_plain_map_value("a: [x,\n  y] w\n", 2)]
#[case::multiline_flow_seq_then_flow_seq_in_seq_item("- [a,\n  b] [c]\n", 2)]
#[case::multiline_flow_seq_then_plain_at_root("[x,\ny] z\n", 2)]
#[case::flow_seq_then_flow_seq_at_root("[x] [y]\n", 1)]
#[case::flow_seq_then_plain_at_root("[x] w\n", 1)]
#[case::alias_then_plain_at_root("*a w\n", 1)]
#[case::flow_map_then_flow_map_at_root("{a: 1} {b: 2}\n", 1)]
#[case::dash_after_block_scalar_map_value("a: |\n  t\n- b\n", 3)]
#[case::bare_anchor_above_key_after_map_value("a: 1\n&x\nb: 2\n", 2)]
#[case::bare_anchor_above_key_in_nested_mapping("a:\n  b: \"q\"\n  &x\n  c: 1\n", 3)]
#[case::single_quoted_empty_then_plain("- '' w\n", 1)]
#[case::single_quoted_escaped_quote_only_then_plain("- '''' w\n", 1)]
#[case::single_quoted_then_dash_text("- 'q' - w\n", 1)]
#[case::single_quoted_tab_then_plain("- 'q'\tw\n", 1)]
#[case::single_quoted_crlf_then_plain("- 'q' w\r\n", 1)]
#[case::single_quoted_anchored_map_value_then_plain("a: &x 'q' w\n", 1)]
#[case::single_quoted_in_seq_item_mapping_then_plain("- a: 'q' w\n", 1)]
#[case::single_quoted_in_seq_space_sequence_then_plain("k:\n- 'r' w\n", 2)]
#[case::single_quoted_then_indented_plain_line("- 'q'\n  w\n", 2)]
#[case::single_quoted_explicit_key_then_indented_plain_line("? 'q'\n  w\n: v\n", 2)]
#[case::single_quoted_blank_line_then_indented_plain("- 'q'\n\n  w\n", 3)]
#[case::multiline_single_quoted_as_implicit_key("- 'a\n  b': v\n", 2)]
#[case::single_quoted_map_value_then_hash_without_space("a: 'q'#c\n", 1)]
#[case::single_quoted_after_document_start_then_plain("--- 'q' w\n", 1)]
#[case::indented_root_seq_then_bare_anchor("  - a\n&x\n", 2)]
#[case::indented_root_map_then_bare_anchor("  a: 1\n&x\n", 2)]
#[case::indented_root_seq_flow_item_then_bare_anchor("  - [x]\n&x\n", 2)]
#[case::indented_root_seq_then_bare_tag(" - a\n!!str\n", 2)]
#[case::indented_root_map_then_flow_seq("  a:\n[x]\n", 2)]
#[case::indented_root_seq_then_alias("  - a\n*y\n", 2)]
#[case::indented_root_explicit_key_then_flow_seq(" ? k\n[x]\n", 2)]
#[case::indented_root_map_then_plain("  key: val\nbaz\n", 2)]
#[case::indented_root_seq_then_plain("  - foo\n  - bar\nbaz\n", 3)]
#[case::indented_root_map_then_anchored_plain("  a: 1\n&x b\n", 2)]
#[case::indented_root_seq_then_flow_map("  - a\n{x: 1}\n", 2)]
#[case::indented_root_map_then_bare_anchor_and_key(" a: 1\n&x\nb: 2\n", 2)]
#[case::indented_root_seq_quoted_then_flow_seq("  - \"q\"\n[x]\n", 2)]
#[case::indented_root_map_then_alias("  a: 1\n*y\n", 2)]
#[case::indented_root_map_then_flow_map("  a: 1\n{x: 1}\n", 2)]
#[case::indented_root_seq_then_anchored_plain_word("  - a\n&x w\n", 2)]
#[case::indented_root_map_then_key_at_column_zero("  a: 1\nb: 2\n", 2)]
#[case::indented_root_map_then_plain_word_at_column_zero("  a: 1\nw\n", 2)]
#[case::indented_root_seq_quoted_then_less_indented_plain("  - \"q\"\n w\n", 2)]
#[case::indented_root_seq_quoted_then_less_indented_item("  - \"q\"\n - r\n", 2)]
#[case::indented_root_map_then_anchored_key_at_column_zero("  a: 1\n&x b: 2\n", 2)]
#[case::indented_root_map_then_less_indented_anchored_key("    a: 1\n  &x b: 2\n", 2)]
#[case::single_quoted_key_with_tail("'k' x: v\n", 1)]
#[case::double_quoted_key_with_tail("\"k\" x: v\n", 1)]
#[case::single_quoted_key_with_tail_in_seq_item("- 'k' x: v\n", 1)]
#[case::double_quoted_key_with_tail_in_seq_item("- \"k\" x: v\n", 1)]
#[case::double_quoted_key_with_tail_value_in_seq_item("- \"q\" w: v\n", 1)]
#[case::tab_led_flow_after_map_value("a: \"q\"\n\t[x]\n", 2)]
#[case::tab_led_flow_after_seq_item("- \"q\"\n\t[x]\n", 2)]
#[case::tab_led_flow_after_nested_seq_item_map_value("- a: \"q\"\n\t[x]\n", 2)]
#[case::tab_led_flow_after_nested_map_value("a:\n  b: \"q\"\n\t[x]\n", 3)]
#[case::tab_led_flow_map_after_explicit_key("? \"k\"\n\t{x: 1}\n: v\n", 2)]
#[case::flow_key_indented_past_entry("a: 1\n  [x]: 2\n", 2)]
#[case::flow_key_indented_four_past_entry("a: 1\n    [x]: 2\n", 2)]
#[case::flow_key_indented_past_nested_entry("a:\n  b: 1\n    [x]: 2\n", 3)]
#[case::alias_key_without_colon_separator("a: 1\n*y: 2\n", 2)]
#[case::anchor_before_alias_key("a: 1\n&z *y : 2\n", 2)]
#[case::two_aliases_before_colon("a: 1\n*y *z : 2\n", 2)]
#[case::tag_before_alias_key("a: 1\n!!str *y : 2\n", 2)]
#[case::standalone_anchor_before_flow_key("a: 1\n&z\n[x]: 2\n", 2)]
#[case::standalone_anchor_before_alias_key("a: 1\n&z\n*y : 2\n", 2)]
#[case::scalar_after_flow_key_before_colon("a: 1\n[x] y: 2\n", 2)]
#[case::flow_seq_after_flow_key_before_colon("a: 1\n[x] [y]: 2\n", 2)]
#[case::multiline_flow_seq_key("a: 1\n[x,\n y]: 2\n", 2)]
#[case::multiline_flow_map_key("a: 1\n{x: 1,\n y}: 2\n", 2)]
#[case::flow_seq_without_key_colon_after_entry("a: 1\n[x]\n", 2)]
#[case::flow_map_without_key_colon_after_entry("a: 1\n{x: 1}\n", 2)]
#[case::alias_without_key_colon_after_entry("a: 1\n*y\n", 2)]
#[case::alias_without_key_colon_after_anchored_entry("a: &y 1\n*y\n", 2)]
#[case::flow_seq_after_flow_key_entry("a: 1\n[x]: 2\n[y]\n", 3)]
#[case::dash_after_flow_key_entry("a: 1\n[x]: 2\n- b\n", 3)]
#[case::seq_item_flow_key_after_entry("a: 1\n- [x]: 2\n", 2)]
#[case::second_root_node_after_root_alias("*a\nb: 1\n", 2)]
#[case::alias_key_after_root_alias("*y\n!!str\n*y : 1\n", 2)]
#[case::tag_line_after_bare_explicit_key_before_alias_key("?\n!!str\n*y : 2\n", 3)]
#[case::tag_line_after_bare_explicit_key_after_entry_before_alias_key(
    "x: 1\n?\n!!str\n*y : 2\n",
    4
)]
#[case::tag_line_at_explicit_key_column_in_seq_item_before_alias_key("- ?\n  !!str\n  *y : 2\n", 3)]
#[case::second_anchor_line_before_plain_key("&y\n&k\nb: 2\n", 2)]
#[case::second_anchor_line_without_node("&y\n&k\n", 2)]
#[case::second_anchor_line_after_tag_line_before_alias_key("&y\n!!str\n&k\n*y : 2\n", 3)]
#[case::second_anchor_line_after_displaced_anchor_before_alias_key(
    "a: &y !!str\n &k\n *y : 2\n",
    2
)]
#[case::second_anchor_line_in_nested_value("a:\n  &y\n  &k\n  b: 1\n", 3)]
#[case::second_anchor_line_in_seq_item("- &y\n  &k\n  x\n", 2)]
#[case::tag_line_after_bare_explicit_key_with_cr_line_breaks("?\r!!str\r*y : 2\r", 3)]
#[case::mapping_entry_after_flow_key_value("a: 1\n[x]: 2\n  c: 3\n", 3)]
#[case::quoted_continuation_after_flow_key_value("a: 1\n[x]: \"q\"\n  w\n", 3)]
#[case::scalar_after_flow_value_of_flow_key("a: 1\n[x]: [y] z\n", 2)]
#[case::scalar_after_flow_value_of_alias_key("a: 1\n*y : [z] w\n", 2)]
#[case::flow_key_unterminated_at_end("a: 1\n[x", 2)]
#[case::alias_without_name_at_end("a: 1\n*", 2)]
#[case::flow_key_after_plain_seq_item("- a\n[x]: 1\n", 2)]
#[case::flow_key_between_nested_levels("a:\n  b: 1\n [x]: 2\n", 3)]
#[case::flow_key_past_explicit_key_column("? a\n  [x]: 1\n", 2)]
#[case::alias_without_key_colon_after_explicit_entry("? a\n: b\n*y\n", 3)]
#[case::flow_without_key_colon_after_explicit_entry("? a\n: b\n[x]\n", 3)]
#[case::alias_key_after_inline_root_value("--- a: &y 1\n*y : 3\n", 1)]
#[case::flow_key_past_quoted_value("a: \"q\"\n  {x: 1}: 2\n", 2)]
#[case::flow_key_past_quoted_seq_item("- \"q\"\n  [x]: 1\n", 2)]
#[case::alias_key_past_entry("a: 1\n  *y : 2\n", 2)]
#[case::flow_key_unterminated_with_quote("a: 1\n['a]': 1\n", 2)]
fn content_after_complete_node_is_a_syntax_error(#[case] input: &str, #[case] stray_line: usize) {
    let err = first_error(input).expect("expected a parse error");
    assert_eq!(err.kind, ErrorKind::Syntax);
    assert_eq!(
        err.pos.line, stray_line,
        "error must point at the stray line"
    );
    assert!(
        matches!(load(input), Err(LoadError::Parse { pos, .. }) if pos.line == stray_line),
        "load() must report the same error"
    );
    let items: Vec<_> = parse_events(input).collect();
    assert_eq!(items.iter().filter(|r| r.is_err()).count(), 1);
    assert!(
        items.last().is_some_and(Result::is_err),
        "no event after the error"
    );
}

// The error position is the first byte of the stray content, past the
// indentation and past multi-byte characters earlier in the input.
#[rstest]
#[case::after_ascii("- \"q\"\n  w\n", 8, 2)]
#[case::after_multibyte_scalar("- \"ä\"\n  w\n", 9, 2)]
#[case::deeper_indent("- a: \"q\"\n    [x]\n", 13, 4)]
fn content_after_complete_node_error_points_at_first_content_byte(
    #[case] input: &str,
    #[case] byte_offset: usize,
    #[case] column: usize,
) {
    let err = first_error(input).expect("expected a parse error");
    assert_eq!(err.pos.byte_offset, byte_offset);
    assert_eq!(err.pos.column, column);
    assert!(input.is_char_boundary(err.pos.byte_offset));
}

// Same collection shapes with the node still open or properly separated: no
// error, and the values are the ones the document always had.
#[rstest]
#[case::plain_continuation("- v\n  w\n", &["v w"])]
#[case::compact_mapping_in_seq_item("- a: 1\n  b: 2\n", &["a", "1", "b", "2"])]
#[case::compact_seq_in_seq_item("- - x\n  - y\n", &["x", "y"])]
#[case::tag_line_then_value("- !!str\n  v\n", &["v"])]
#[case::anchor_line_then_value("- &a\n  v\n", &["v"])]
#[case::bare_dash_then_value("-\n  v\n", &["v"])]
#[case::explicit_key_plain_continuation("? k\n  w\n: v\n", &["k w", "v"])]
#[case::explicit_key_then_value("? k\n: v\n", &["k", "v"])]
#[case::explicit_key_block_scalar("? |\n  t\n: v\n", &["t\n", "v"])]
#[case::seq_item_comment_line_between_items("- \"q\"\n  # c\n- w\n", &["q", "w"])]
#[case::map_value_quoted_comment_lines("a: \"q\" # c\n  # d\nb: 1\n", &["a", "q", "b", "1"])]
#[case::seq_item_flow_seq_comment_line("- [x]\n  # c\n- z\n", &["x", "z"])]
#[case::map_value_flow_seq_comment_line("a: [x]\n  # c\nb: 1\n", &["a", "x", "b", "1"])]
#[case::map_value_alias_comment_line("a: &y 1\nb: *y\n  # c\nc: 2\n", &["a", "1", "b", "c", "2"])]
#[case::explicit_key_quoted_comment_line("? \"k\"\n  # c\n: v\n", &["k", "v"])]
#[case::comment_line_between_tag_and_value("- !!str\n  # c\n  v\n", &["v"])]
#[case::comment_line_between_anchor_and_value("- &a\n  # c\n  v\n", &["v"])]
#[case::comment_line_after_bare_dash("-\n  # c\n  v\n", &["v"])]
#[case::comment_line_after_empty_map_value("a:\n  # c\n  v\n", &["a", "v"])]
#[case::map_value_tag_line_then_value("a: !!str\n  v\nb: 1\n", &["a", "v", "b", "1"])]
#[case::map_value_anchor_line_then_seq("a: &x\n  - 1\n", &["a", "1"])]
#[case::tag_line_then_mapping_in_seq_item("- !!map\n  a: 1\n", &["a", "1"])]
#[case::anchor_line_then_mapping_in_seq_item("- &x\n  a: 1\n", &["a", "1"])]
#[case::tag_line_then_seq_in_seq_item("- !!seq\n  - 1\n", &["1"])]
#[case::multiline_plain_then_item("- v\n  w\n- x\n", &["v w", "x"])]
#[case::multiline_plain_map_value("a: b\n  c\n", &["a", "b c"])]
#[case::multiline_double_quoted_seq_item("- \"a\n  b\"\n- w\n", &["a b", "w"])]
#[case::multiline_single_quoted_seq_item("- 'a\n  b'\n- w\n", &["a b", "w"])]
#[case::multiline_double_quoted_map_value("a: \"a\n  b\"\nc: 1\n", &["a", "a b", "c", "1"])]
#[case::explicit_value_plain_continuation("? k\n: v\n  w\n", &["k", "v w"])]
#[case::block_scalar_seq_item("- |\n  t\n  u\n- w\n", &["t\nu\n", "w"])]
#[case::block_scalar_map_value("a: |\n  t\nb: 1\n", &["a", "t\n", "b", "1"])]
#[case::block_scalar_hash_line_is_content("- |\n  t\n  # not a comment\n- w\n", &["t\n# not a comment\n", "w"])]
#[case::block_scalar_explicit_key_multiline("? |\n  t\n  u\n: v\n", &["t\nu\n", "v"])]
#[case::multiline_flow_seq_map_value("a: [x,\n  y]\nb: 1\n", &["a", "x", "y", "b", "1"])]
#[case::multiline_flow_seq_seq_item("- [x,\n  y]\n- z\n", &["x", "y", "z"])]
#[case::explicit_key_block_seq("? - a\n  - b\n: v\n", &["a", "b", "v"])]
#[case::explicit_key_block_mapping("? a: 1\n  b: 2\n: v\n", &["a", "1", "b", "2", "v"])]
#[case::explicit_entry_in_seq_item("- ? a\n  : b\n", &["a", "b"])]
#[case::explicit_entry_then_key_in_seq_item("- ? a\n  : b\n  c: d\n", &["a", "b", "c", "d"])]
#[case::explicit_key_without_value("? k\n  w\n", &["k w", ""])]
#[case::sibling_mapping_entry_in_seq_item("- a: \"q\"\n  b: 1\n", &["a", "q", "b", "1"])]
#[case::sibling_mapping_entry_nested("a:\n  b: \"q\"\n  c: 1\n", &["a", "b", "q", "c", "1"])]
#[case::blank_line_between_quoted_items("- \"q\"\n\n- w\n", &["q", "w"])]
#[case::blank_line_between_entries("a: \"q\"\n\nb: 1\n", &["a", "q", "b", "1"])]
#[case::crlf_plain_continuation("- v\r\n  w\r\n", &["v w"])]
#[case::blank_line_inside_seq_item_plain("- a\n\n  b\n", &["a\nb"])]
#[case::blank_line_inside_nested_seq_item_plain("a:\n  - b\n\n    c\n", &["a", "b\nc"])]
#[case::whitespace_only_line_inside_seq_item_plain("- a\n  \n  b\n", &["a\nb"])]
#[case::crlf_blank_line_inside_seq_item_plain("- a\r\n\r\n  b\r\n", &["a\nb"])]
#[case::several_blank_lines_inside_seq_item_plain("- a\n\n\n  b\n", &["a\n\nb"])]
#[case::tab_only_line_inside_seq_item_plain("- a\n\t\n  b\n", &["a\nb"])]
#[case::blank_lines_between_three_plain_lines("- a\n\n  b\n\n  c\n", &["a\nb\nc"])]
#[case::plain_continuation_then_blank_then_plain("- a\n  b\n\n  c\n", &["a b\nc"])]
#[case::two_spaces_after_dash_blank_line_inside("-  a\n\n   b\n", &["a\nb"])]
#[case::continuation_indented_one_past_dash("- a\n\n b\n", &["a\nb"])]
#[case::continuation_indented_three("- a\n\n   b\n", &["a\nb"])]
#[case::seq_at_key_column_blank_line_inside("a:\n- b\n\n  c\n", &["a", "b\nc"])]
#[case::blank_line_inside_item_followed_by_item("a:\n  - b\n\n    c\n  - d\n", &["a", "b\nc", "d"])]
#[case::blank_line_inside_compact_nested_seq_item("- - a\n\n    b\n", &["a\nb"])]
#[case::root_plain_blank_line_inside("a\n\n  b\n", &["a\nb"])]
#[case::tagged_seq_item_blank_line_inside("- !!str a\n\n  b\n", &["a\nb"])]
#[case::anchored_seq_item_blank_line_inside("- &x a\n\n  b\n", &["a\nb"])]
#[case::explicit_key_blank_line_inside("? a\n\n  b\n: v\n", &["a\nb", "v"])]
#[case::explicit_value_blank_line_inside("? k\n: a\n\n  b\n", &["k", "a\nb"])]
#[case::blank_line_continuation_starting_with_flow_indicator("- a\n\n  [x]\n", &["a\n[x]"])]
#[case::blank_line_continuation_starting_with_dash("- a\n\n  - b\n", &["a\n- b"])]
#[case::blank_line_continuation_starting_with_quote("- a\n\n  \"q\"\n", &["a\n\"q\""])]
#[case::blank_line_continuation_starting_with_star("- a\n\n  *x\n", &["a\n*x"])]
#[case::blank_line_inside_item_then_more_lines("- a\n\n  b\n  c\n", &["a\nb c"])]
#[case::blank_line_inside_item_then_next_item("- a\n\n  b\n- c\n", &["a\nb", "c"])]
#[case::blank_line_between_plain_items("- a\n\n- b\n", &["a", "b"])]
#[case::blank_line_before_dedent_to_key("a:\n  - x\n\nb: 1\n", &["a", "x", "b", "1"])]
#[case::blank_line_between_items_of_nested_seq("a:\n  - x\n\n  - y\n", &["a", "x", "y"])]
#[case::blank_line_between_items_of_inner_seq("- - a\n\n  - b\n", &["a", "b"])]
#[case::trailing_blank_line_at_eof("- a\n\n", &["a"])]
#[case::trailing_blank_lines_at_eof("- a\n\n\n", &["a"])]
#[case::trailing_whitespace_only_line_at_eof("- a\n\n  \n", &["a"])]
#[case::no_line_after_item("- a", &["a"])]
#[case::blank_line_then_document_start_marker("- a\n\n---\n- b\n", &["a", "b"])]
#[case::blank_line_then_document_end_marker("- a\n\n...\n", &["a"])]
#[case::trailing_comment_then_blank_then_item("- a # c\n\n- b\n", &["a", "b"])]
#[case::continuation_trailing_comment("- a\n\n  b # c\n- c\n", &["a\nb", "c"])]
#[case::comment_line_after_blank_at_column_zero("- a\n\n# c\n- b\n", &["a", "b"])]
#[case::indented_comment_line_after_blank_then_item("- a\n\n  # c\n- b\n", &["a", "b"])]
#[case::quoted_key_after_value("a: 1\n\"k\": 2\n", &["a", "1", "k", "2"])]
#[case::anchored_key_after_value("a: 1\n&x b: 2\n", &["a", "1", "b", "2"])]
#[case::tagged_key_after_value("a: 1\n!!str b: 2\n", &["a", "1", "b", "2"])]
#[case::explicit_entry_after_value("a: 1\n? b\n: c\n", &["a", "1", "b", "c"])]
#[case::explicit_block_seq_key_after_value("a: 1\n? - b\n: c\n", &["a", "1", "b", "c"])]
#[case::block_seq_as_explicit_key_content_at_question_mark_column("?\n- x\n: v\n", &["x", "v"])]
#[case::indented_flow_seq_as_explicit_key_content("? \n  [x]\n: v\n", &["x", "v"])]
#[case::seq_space_anchored_key_after_seq("a:\n- b\n&x c: 1\n", &["a", "b", "c", "1"])]
#[case::seq_space_tagged_key_after_seq("a:\n- b\n!!str c: 1\n", &["a", "b", "c", "1"])]
#[case::seq_space_anchored_quoted_key_after_seq("k:\n- a\n&x \"c d\": 1\n", &["k", "a", "c d", "1"])]
#[case::seq_space_anchored_key_after_two_items_then_key("k:\n- a\n- b\n&x c: 1\nd: 2\n", &["k", "a", "b", "c", "1", "d", "2"])]
#[case::seq_space_anchored_key_after_flow_item("k:\n- [a]\n&x b: 1\n", &["k", "a", "b", "1"])]
#[case::seq_space_anchored_key_in_seq_item_mapping("- a:\n  - b\n  &x c: 1\n", &["a", "b", "c", "1"])]
#[case::seq_space_anchored_key_nested_then_key("k:\n  a:\n  - b\n  &x c: 1\nd: 2\n", &["k", "a", "b", "c", "1", "d", "2"])]
#[case::seq_space_anchored_key_after_explicit_value_seq("? k\n:\n- a\n&x c: 1\n", &["k", "a", "c", "1"])]
#[case::seq_value_after_explicit_value_indicator("? k\n:\n- b\n", &["k", "b"])]
#[case::indented_seq_value_after_explicit_value_indicator("? k\n:\n  - b\n", &["k", "b"])]
#[case::block_seq_explicit_key_with_two_items("?\n- x\n- y\n: v\n", &["x", "y", "v"])]
#[case::seq_value_with_two_items("k:\n- a\n- b\n", &["k", "a", "b"])]
#[case::flow_seq_key_in_seq_item("- [x]: 1\n", &["x", "1"])]
#[case::flow_seq_with_comment_in_seq_item("- [x] # c\n", &["x"])]
#[case::flow_seq_in_seq_item("- [x]\n", &["x"])]
#[case::flow_map_in_seq_item("- {a: 1}\n", &["a", "1"])]
#[case::flow_seq_with_comment_map_value("a: [x] # c\n", &["a", "x"])]
#[case::flow_seq_key_explicit_key("? [x]: 1\n: v\n", &["x", "1", "v"])]
#[case::nested_flow_seq_in_flow_seq("[[a], b]\n", &["a", "b"])]
#[case::nested_flow_in_flow_map("{a: [x], b: *c}\n", &["a", "x", "b"])]
#[case::alias_in_flow_seq_before_comma("[*a, b]\n", &["b"])]
#[case::nested_flow_seq_in_map_value("a: [x, [y]]\n", &["a", "x", "y"])]
#[case::multi_line_nested_flow_seq_in_map_value("a: [x,\n  [y]\n  ]\n", &["a", "x", "y"])]
#[case::multi_line_flow_seq_with_trailing_comment("- [x,\n  y] # c\n", &["x", "y"])]
#[case::flow_seq_with_tab_before_comment("a: [x]\t# c\n", &["a", "x"])]
#[case::flow_seq_with_trailing_spaces("a: [x]  \n", &["a", "x"])]
#[case::alias_with_tab_before_comment("- *a\t# c\n", &[])]
#[case::flow_seq_key_with_nested_flow_in_seq_item("- [a, [b], c]: 1\n", &["a", "b", "c", "1"])]
#[case::single_quoted_with_comment_in_seq_item("- 'q' # c\n", &["q"])]
#[case::single_quoted_key_in_seq_item("- 'q': v\n", &["q", "v"])]
#[case::single_quoted_with_escaped_quote("- 'it''s'\n", &["it's"])]
#[case::multiline_single_quoted_with_comment("- 'a\n  b' # c\n", &["a b"])]
#[case::single_quoted_scalars_in_flow_seq("['q' , 'r']\n", &["q", "r"])]
#[case::single_quoted_scalars_in_flow_map("{'a': 'b', c: 'd'}\n", &["a", "b", "c", "d"])]
#[case::single_quoted_in_flow_seq_map_value("a: ['q']\n", &["a", "q"])]
#[case::single_quoted_explicit_entry_with_comment("? 'q'\n: 'v' # c\n", &["q", "v"])]
#[case::single_quoted_key_with_space_before_colon("- 'q' : 1\n", &["q", "1"])]
#[case::single_quoted_key_in_seq_item_value("- 'q': 1\n", &["q", "1"])]
#[case::single_quoted_escaped_quote_with_comment("- 'it''s' # c\n", &["it's"])]
#[case::single_quoted_key_and_value_at_root("'a': 'q'\n", &["a", "q"])]
#[case::single_quoted_explicit_key_with_space_before_colon("? 'k' : 1\n: v\n", &["k", "1", "v"])]
#[case::flow_seq_of_plain_and_single_quoted("- ['q', r]\n  # c\n- 's'\n", &["q", "r", "s"])]
#[case::flow_seq_with_comment_at_root("['q'] # c\n", &["q"])]
#[case::single_quoted_key_with_flow_seq_value_then_key("'k': ['q']\nl: 'r'\n", &["k", "q", "l", "r"])]
#[case::explicit_flow_seq_key_with_single_quoted_value("? ['q']\n: 'r'\n", &["q", "r"])]
#[case::flow_map_key_with_single_quoted_value("{'a': 1}: 'r'\n", &["a", "1", "r"])]
#[case::flow_seq_of_single_quoted_scalars_at_root("['a', 'b']\n", &["a", "b"])]
#[case::flow_map_of_single_quoted_scalars_at_root("{'a': 'b'}\n", &["a", "b"])]
#[case::flow_seq_tab_before_comment("- [x]\t# c\n", &["x"])]
#[case::flow_seq_two_spaces_before_comment("- [x]  # c\n", &["x"])]
#[case::flow_seq_comment_without_space_after_hash("- [x] #c\n", &["x"])]
#[case::flow_seq_map_value_comment_without_space_after_hash("a: [x] #c\n", &["a", "x"])]
#[case::alias_map_value_tab_before_comment("a: *b\t# c\n", &["a"])]
#[case::crlf_flow_seq_then_item("- [x]\r\n- b\r\n", &["x", "b"])]
#[case::flow_seq_key_with_plain_value("- [x]: y\n", &["x", "y"])]
#[case::flow_seq_key_without_value("- [x]:\n", &["x", ""])]
#[case::flow_seq_key_space_before_colon("- [x] : 1\n", &["x", "1"])]
#[case::flow_seq_key_multiword_value("- [x] : 1 2\n", &["x", "1 2"])]
#[case::flow_seq_key_value_with_flow_text("- [x]: 1 [y]\n", &["x", "1 [y]"])]
#[case::anchored_flow_seq_in_seq_item("- &x [a]\n", &["a"])]
#[case::anchored_flow_seq_with_comment("- &x [a] # c\n", &["a"])]
#[case::multiline_flow_seq_map_value_with_comment("a: [x,\n  y] # c\n", &["a", "x", "y"])]
#[case::flow_map_with_nested_seq_and_comment("- {a: [x]} # c\n", &["a", "x"])]
#[case::bare_anchor_above_first_key_at_root("&x\na: 1\n", &["a", "1"])]
#[case::bare_anchor_above_key_of_value_mapping("k:\n  &x\n  a: 1\n", &["k", "a", "1"])]
#[case::anchor_after_key_colon_above_nested_mapping("k: &x\n  a: 1\n", &["k", "a", "1"])]
#[case::anchored_key_after_map_value("a: 1\n&x b: 2\n", &["a", "1", "b", "2"])]
#[case::single_quoted_empty("- ''\n", &[""])]
#[case::single_quoted_escaped_quote_only("- ''''\n", &["'"])]
#[case::single_quoted_tab_before_comment("- 'q'\t# c\n", &["q"])]
#[case::single_quoted_trailing_spaces("- 'q'  \n", &["q"])]
#[case::crlf_single_quoted_items("- 'q'\r\n- 'r'\r\n", &["q", "r"])]
#[case::single_quoted_escaped_key_space_before_colon("- 'a''b' : c\n", &["a'b", "c"])]
#[case::multiline_single_quoted_with_blank_line("- 'a\n\n  b'\n", &["a\nb"])]
#[case::single_quoted_at_root_with_comment("'q' # c\n", &["q"])]
#[case::single_quoted_after_document_start_with_comment("--- 'q' # c\n", &["q"])]
#[case::anchored_single_quoted("- &x 'q'\n", &["q"])]
#[case::tagged_single_quoted_with_comment("- !!str 'q' # c\n", &["q"])]
#[case::single_quoted_flow_map_adjacent_colon("{'a':'b'}\n", &["a", "b"])]
#[case::single_quoted_flow_seq_pair_adjacent_colon("['a':'b']\n", &["a", "b"])]
#[case::indented_root_seq("  - a\n  - b\n", &["a", "b"])]
#[case::indented_root_map("  a: 1\n  b: 2\n", &["a", "1", "b", "2"])]
#[case::indented_root_seq_then_comment_line("  - a\n# c\n", &["a"])]
#[case::indented_root_map_then_blank_and_comment_lines("  a: 1\n\n# c\n", &["a", "1"])]
#[case::indented_root_seq_then_next_document("  - a\n---\n- b\n", &["a", "b"])]
#[case::indented_root_map_then_document_end("  a: 1\n...\n", &["a", "1"])]
#[case::tag_line_then_indented_mapping("!!map\n  a: 1\n", &["a", "1"])]
#[case::indented_anchored_root_mapping("  &x\n  a: 1\n", &["a", "1"])]
#[case::indented_map_two_entries(" a: 1\n b: 2\n", &["a", "1", "b", "2"])]
#[case::indented_root_map_with_nested_seq_then_key("  a:\n    - b\n  c: 1\n", &["a", "b", "c", "1"])]
#[case::indented_root_map_with_anchored_second_key("  a: 1\n  &x b: 2\n", &["a", "1", "b", "2"])]
#[case::anchored_first_key_at_root_then_key("&x a: 1\nb: 2\n", &["a", "1", "b", "2"])]
#[case::tagged_first_key_at_root_then_key("!!map a: 1\nb: 2\n", &["a", "1", "b", "2"])]
#[case::tagged_str_first_key_at_root_then_key("!!str a: 1\nb: 2\n", &["a", "1", "b", "2"])]
#[case::two_anchored_keys_at_root("&x a: 1\n&y b: 2\n", &["a", "1", "b", "2"])]
#[case::indented_anchored_first_key("  &x a: 1\n  b: 2\n", &["a", "1", "b", "2"])]
#[case::indented_tagged_first_key("  !!str a: 1\n  b: 2\n", &["a", "1", "b", "2"])]
#[case::indented_root_seq_then_document_with_scalar("  - a\n---\nb\n", &["a", "b"])]
#[case::indented_root_map_then_second_document(" a: 1\n b: 2\n---\nc: 3\n", &["a", "1", "b", "2", "c", "3"])]
#[case::indented_root_map_then_comment_and_deeper_key("  a: 1\n# c\n  b: 2\n", &["a", "1", "b", "2"])]
#[case::indented_root_map_then_marker_and_map("  a: 1\n---\nb: 2\n", &["a", "1", "b", "2"])]
#[case::single_quoted_key_at_root("'k': v\n", &["k", "v"])]
#[case::double_quoted_key_with_space_before_colon("\"k\" : v\n", &["k", "v"])]
#[case::tab_led_comment_line_after_seq_item("- \"q\"\n\t# c\n- w\n", &["q", "w"])]
#[case::space_then_tab_comment_line_after_seq_item("- \"q\"\n  \t# c\n- w\n", &["q", "w"])]
#[case::tab_led_plain_continuation("a: v\n\tw\n", &["a", "v w"])]
#[case::explicit_key_empty_value_then_key("? a\nb: 1\n", &["a", "", "b", "1"])]
#[case::explicit_key_then_deeper_value_mapping("? a\n:\n  b: 1\n", &["a", "b", "1"])]
#[case::explicit_key_then_value_seq_same_column("? a\n:\n- b\n", &["a", "b"])]
fn content_before_node_is_complete_still_loads(#[case] input: &str, #[case] expected: &[&str]) {
    assert_eq!(scalar_values(input), expected);
    assert!(load(input).is_ok(), "load() must accept {input:?}");
}

#[test]
fn content_after_complete_node_in_second_document_reports_its_own_line() {
    // The stray line is in the second document; the first one is untouched.
    let input = "- a\n---\n- \"q\"\n  w\n";
    let items: Vec<_> = parse_events(input).collect();
    let err = items
        .iter()
        .find_map(|r| r.as_ref().err())
        .expect("expected a parse error");
    assert_eq!(err.pos.line, 4);
}

#[test]
fn documents_separated_by_markers_after_quoted_item_load() {
    assert_eq!(scalar_values("- \"q\"\n---\n- w\n...\n"), ["q", "w"]);
}

#[test]
fn content_after_complete_node_in_deeply_nested_sequence_is_a_syntax_error() {
    let depth = 100;
    let input = format!("{}\"q\"\n{}w\n", "- ".repeat(depth), " ".repeat(2 * depth));
    let err = first_error(&input).expect("expected a parse error");
    assert_eq!(err.kind, ErrorKind::Syntax);
    assert_eq!(err.pos.line, 2);
    assert!(matches!(load(&input), Err(LoadError::Parse { .. })));
}

// A sequence item and a mapping value take the same plain-scalar body to the
// same value: the sequence fast path and the general path must agree.
#[rstest]
#[case::blank_line("a\n\n  b")]
#[case::two_blank_lines("a\n\n\n  b")]
#[case::whitespace_only_line("a\n  \n  b")]
#[case::crlf_blank_line("a\r\n\r\n  b")]
#[case::tab_only_line("a\n\t\n  b")]
#[case::blank_lines_between_three_lines("a\n\n  b\n\n  c")]
#[case::flow_indicator_line_start("a\n\n  [x]")]
#[case::dash_line_start("a\n\n  - b")]
#[case::tag_property("!!str a\n\n  b")]
#[case::anchor_property("&x a\n\n  b")]
#[case::no_blank_line("a\n  b")]
fn seq_item_and_map_value_plain_scalar_agree(#[case] body: &str) {
    let in_seq = format!("- {body}\n");
    let in_map = format!("k: {body}\n");
    let seq_values = scalar_values(&in_seq);
    let map_values = scalar_values(&in_map);
    assert_eq!(
        seq_values.first(),
        map_values.get(1),
        "{in_seq:?} vs {in_map:?}"
    );
    assert_eq!(seq_values.len(), 1);
}

// The invalid character is reported at its own position (line, column) whether
// or not a blank line follows, by the sequence fast path and the general path.
#[rstest]
#[case::nul_after_item_then_blank_then_item("- a\0\n\n- b\n", 1, 3)]
#[case::nul_in_comment_then_blank_then_item("- a # c\0\n\n- b\n", 1, 7)]
#[case::bom_after_item_then_blank_then_item("- a\u{FEFF}\n\n- b\n", 1, 3)]
#[case::nul_in_continuation_line("- a\n\n  b\0\n", 3, 3)]
#[case::nul_after_item_then_blank_then_continuation("- a\0\n\n  b\n", 1, 3)]
#[case::nul_after_item_without_blank_line("- a\0\n- b\n", 1, 3)]
fn invalid_character_around_blank_line_is_one_error_at_its_position(
    #[case] input: &str,
    #[case] line: usize,
    #[case] column: usize,
) {
    let items: Vec<_> = parse_events(input).collect();
    assert_eq!(items.iter().filter(|r| r.is_err()).count(), 1, "{items:?}");
    let err = first_error(input).expect("expected a parse error");
    assert_eq!((err.pos.line, err.pos.column), (line, column));
    assert!(matches!(load(input), Err(LoadError::Parse { .. })));
}

#[test]
fn many_blank_lines_between_sequence_items_load_in_one_pass() {
    let input = format!("- a\n{}- b\n", "\n".repeat(100_000));
    assert_eq!(scalar_values(&input), ["a", "b"]);
}

#[test]
fn many_blank_lines_inside_a_sequence_item_fold_into_one_scalar() {
    let blanks = 1000;
    let input = format!("- a\n{}  b\n", "\n".repeat(blanks));
    assert_eq!(
        scalar_values(&input),
        [format!("a{}b", "\n".repeat(blanks))]
    );
}

/// Whether a shape lists tags.  `load()` resolves a tag onto every node from the
/// schema, which a source tag cannot be told from, so its shape leaves them out.
#[derive(Clone, Copy)]
enum Tags {
    Listed,
    Omitted,
}

/// `&anchor` and `!!tag` of a node, in that order; a tag in the YAML core
/// namespace is written in its `!!` shorthand.
fn property_tokens(anchor: Option<&str>, tag: Option<&str>, tags: Tags) -> Vec<String> {
    let tag = tag.filter(|_| matches!(tags, Tags::Listed)).map(|tag| {
        tag.strip_prefix("tag:yaml.org,2002:")
            .map_or_else(|| format!("<{tag}>"), |name| format!("!!{name}"))
    });
    anchor
        .map(|name| format!("&{name}"))
        .into_iter()
        .chain(tag)
        .collect()
}

/// A scalar is `:value` (backslashes and newlines escaped) preceded by each
/// property and a comma; a collection start is its kind followed by each
/// property and a comma.
fn scalar_token(props: &[String], value: &str) -> String {
    let value = value.replace('\\', "\\\\").replace('\n', "\\n");
    let props: String = props.iter().flat_map(|prop| [prop.as_str(), ","]).collect();
    format!("{props}:{value}")
}

fn collection_token(kind: &str, props: &[String]) -> String {
    std::iter::once(kind)
        .chain(props.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(",")
}

/// One token per event in document order, in the notation of `fy-tool
/// --testsuite` with the styles dropped: `+M`/`-M` and `+S`/`-S` for
/// collections, `:value` for a scalar, `*name` for an alias.  Comments and
/// stream and document events are left out.
fn event_shape(input: &str, tags: Tags) -> String {
    parse_events(input)
        .filter_map(|item| {
            let event = item.unwrap().0;
            let props = property_tokens(event.anchor(), event.tag(), tags);
            match &event {
                Event::Scalar { value, .. } => Some(scalar_token(&props, value)),
                Event::Alias { name } => Some(format!("*{name}")),
                Event::SequenceStart { .. } => Some(collection_token("+S", &props)),
                Event::MappingStart { .. } => Some(collection_token("+M", &props)),
                Event::SequenceEnd => Some("-S".to_owned()),
                Event::MappingEnd => Some("-M".to_owned()),
                Event::StreamStart
                | Event::StreamEnd
                | Event::Comment { .. }
                | Event::DocumentStart { .. }
                | Event::DocumentEnd { .. } => None,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The tokens of `event_shape` for the tree `load()` builds.
fn node_shape(node: &Node) -> Vec<String> {
    match node {
        Node::Scalar { value, tag, .. } => {
            vec![scalar_token(
                &property_tokens(node.anchor(), tag.as_deref(), Tags::Omitted),
                value,
            )]
        }
        Node::Alias { name, .. } => vec![format!("*{name}")],
        Node::Sequence { items, tag, .. } => std::iter::once(collection_token(
            "+S",
            &property_tokens(node.anchor(), tag.as_deref(), Tags::Omitted),
        ))
        .chain(items.iter().flat_map(node_shape))
        .chain(["-S".to_owned()])
        .collect(),
        Node::Mapping { entries, tag, .. } => std::iter::once(collection_token(
            "+M",
            &property_tokens(node.anchor(), tag.as_deref(), Tags::Omitted),
        ))
        .chain(
            entries
                .iter()
                .flat_map(|(key, value)| node_shape(key).into_iter().chain(node_shape(value))),
        )
        .chain(["-M".to_owned()])
        .collect(),
    }
}

fn load_shape(input: &str) -> String {
    load(input)
        .unwrap()
        .iter()
        .flat_map(|document| node_shape(&document.root))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Panics unless the events of `input` nest properly and every mapping holds
/// an even number of nodes, i.e. each key has a value.
fn assert_well_formed(input: &str) {
    // One counter per open collection: (is a mapping, nodes seen so far).
    let mut open: Vec<(bool, usize)> = Vec::new();
    let complete_node = |open: &mut Vec<(bool, usize)>| {
        if let Some((_, nodes)) = open.last_mut() {
            *nodes += 1;
        }
    };
    for item in parse_events(input) {
        match item.unwrap().0 {
            Event::MappingStart { .. } => open.push((true, 0)),
            Event::SequenceStart { .. } => open.push((false, 0)),
            Event::MappingEnd => {
                let (is_mapping, nodes) = open.pop().unwrap();
                assert!(
                    is_mapping && nodes % 2 == 0,
                    "{input:?}: mapping with {nodes} nodes"
                );
                complete_node(&mut open);
            }
            Event::SequenceEnd => {
                let (is_mapping, _) = open.pop().unwrap();
                assert!(!is_mapping, "{input:?}: sequence closes a mapping");
                complete_node(&mut open);
            }
            Event::Scalar { .. } | Event::Alias { .. } => complete_node(&mut open),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. } => {}
        }
    }
    assert!(open.is_empty(), "{input:?}: collections left open");
}

// A key that starts with a flow collection or an alias is the next entry of the
// mapping that holds the preceding entry: no extra nested mapping, an empty
// scalar for a key left without a value, a seq-spaces sequence closed first.
// Expected values are the events of libfyaml 0.9.6 (`fy-tool --testsuite`).
#[rstest]
#[case::flow_seq_key("a: 1\n[x]: 2\n", "+M :a :1 +S :x -S :2 -M")]
#[case::flow_map_key("a: 1\n{x: 1}: 2\n", "+M :a :1 +M :x :1 -M :2 -M")]
#[case::alias_key_after_flow_seq_key(
    "x: &y 1\n[x]: 2\n*y : 3\n",
    "+M :x &y,:1 +S :x -S :2 *y :3 -M"
)]
#[case::flow_key_after_explicit_flow_key("? [x]\n[y]: 1\n", "+M +S :x -S : +S :y -S :1 -M")]
#[case::alias_key_after_explicit_key_without_value(
    "x: &y 1\n? a\n*y : b\n",
    "+M :x &y,:1 :a : *y :b -M"
)]
#[case::alias_key_after_tagged_explicit_key(
    "x: &y 1\n?\n  !!str k\n*y :\n  !!str v\n",
    "+M :x &y,:1 !!str,:k : *y !!str,:v -M"
)]
#[case::alias_key_with_flow_key_value_after_empty_value(
    "x: &y 1\na:\n*y :\n  {x: 1}: v\n",
    "+M :x &y,:1 :a : *y +M +M :x :1 -M :v -M -M"
)]
#[case::flow_seq_key_after_seq_space_sequence("a:\n- b\n[x]: 1\n", "+M :a +S :b -S +S :x -S :1 -M")]
#[case::flow_map_key_after_seq_space_sequence(
    "a:\n- b\n{x: 1}: 1\n",
    "+M :a +S :b -S +M :x :1 -M :1 -M"
)]
#[case::alias_key_after_seq_space_sequence("a:\n- b\n*y : 1\n", "+M :a +S :b -S *y :1 -M")]
#[case::alias_key_after_seq_space_sequence_in_seq_item(
    "- a:\n  - b\n  *y : 1\n",
    "+S +M :a +S :b -S *y :1 -M -S"
)]
#[case::flow_key_in_seq_item_mapping("- a: 1\n  [x]: 2\n", "+S +M :a :1 +S :x -S :2 -M -S")]
#[case::flow_key_in_nested_mapping("a:\n  b: 1\n  [x]: 2\n", "+M :a +M :b :1 +S :x -S :2 -M -M")]
#[case::flow_key_after_nested_mapping("a:\n  b: 1\n[x]: 2\n", "+M :a +M :b :1 -M +S :x -S :2 -M")]
#[case::alias_key_after_nested_mapping("a:\n  b: 1\n*y : 2\n", "+M :a +M :b :1 -M *y :2 -M")]
#[case::flow_key_after_nested_mapping_with_empty_value(
    "a:\n  b:\n[x]: 2\n",
    "+M :a +M :b : -M +S :x -S :2 -M"
)]
#[case::flow_key_after_explicit_entry("? k\n: v\n[x]: 2\n", "+M :k :v +S :x -S :2 -M")]
#[case::alias_key_after_explicit_entry("? k\n: v\n*y : 2\n", "+M :k :v *y :2 -M")]
#[case::alias_key_after_two_explicit_keys("? a\n? b\n*y : 3\n", "+M :a : :b : *y :3 -M")]
#[case::alias_key_after_explicit_key("? a\n*y : 1\n", "+M :a : *y :1 -M")]
#[case::alias_key_after_explicit_flow_key("? [x]\n*y : 1\n", "+M +S :x -S : *y :1 -M")]
#[case::flow_key_after_explicit_key_and_empty_value("? a\n:\n[x]: 3\n", "+M :a : +S :x -S :3 -M")]
#[case::flow_key_after_explicit_key("? a\n[x]: 1\n", "+M :a : +S :x -S :1 -M")]
#[case::flow_key_after_bare_explicit_key("?\n[x]: 1\n", "+M : : +S :x -S :1 -M")]
#[case::alias_key_after_bare_explicit_key("?\n*y : 1\n", "+M : : *y :1 -M")]
#[case::flow_key_in_seq_item_after_explicit_key(
    "- ? a\n  [x]: 1\n",
    "+S +M :a : +S :x -S :1 -M -S"
)]
#[case::alias_key_in_seq_item_after_explicit_key("- ? a\n  *y : 1\n", "+S +M :a : *y :1 -M -S")]
#[case::flow_key_after_empty_value("a:\n[x]: 2\n", "+M :a : +S :x -S :2 -M")]
#[case::alias_key_after_empty_value("a:\n*y : 2\n", "+M :a : *y :2 -M")]
#[case::alias_key_after_empty_value_in_seq_item("- a:\n  *y : 2\n", "+S +M :a : *y :2 -M -S")]
#[case::flow_key_after_anchored_empty_value("a: &y\n[x]: 2\n", "+M :a &y,: +S :x -S :2 -M")]
#[case::flow_key_after_tagged_empty_value("a: !!str\n[x]: 2\n", "+M :a !!str,: +S :x -S :2 -M")]
#[case::alias_key_after_anchored_empty_value("a: &y\n*z : 2\n", "+M :a &y,: *z :2 -M")]
#[case::flow_key_after_flow_value("a: [q]\n[x]: 2\n", "+M :a +S :q -S +S :x -S :2 -M")]
#[case::flow_key_after_alias_value("a: *z\n[x]: 2\n", "+M :a *z +S :x -S :2 -M")]
#[case::alias_key_after_alias_value("a: *z\n*y : 2\n", "+M :a *z *y :2 -M")]
#[case::flow_key_after_double_quoted_value("a: \"q\"\n[x]: 2\n", "+M :a :q +S :x -S :2 -M")]
#[case::flow_key_after_block_scalar_value("a: |\n  t\n[x]: 2\n", "+M :a :t\\n +S :x -S :2 -M")]
#[case::alias_key_after_block_scalar_value("a: |\n  t\n*y : 2\n", "+M :a :t\\n *y :2 -M")]
#[case::flow_and_alias_keys_in_succession(
    "a: 1\n[x]: 2\n[y]: 3\n*z : 4\n",
    "+M :a :1 +S :x -S :2 +S :y -S :3 *z :4 -M"
)]
#[case::flow_keys_as_first_keys("[x]: 1\n[y]: 2\n", "+M +S :x -S :1 +S :y -S :2 -M")]
#[case::alias_key_then_flow_key("*y : 1\n[y]: 2\n", "+M *y :1 +S :y -S :2 -M")]
#[case::flow_keys_after_explicit_entry(
    "? a\n: b\n[x]: 1\n[y]: 2\n",
    "+M :a :b +S :x -S :1 +S :y -S :2 -M"
)]
#[case::flow_key_between_plain_keys(
    "[x]: 1\na: 2\n[y]: 3\n",
    "+M +S :x -S :1 :a :2 +S :y -S :3 -M"
)]
#[case::nested_flow_seq_key("a: 1\n[[x]]: 2\n", "+M :a :1 +S +S :x -S -S :2 -M")]
#[case::empty_flow_seq_key("a: 1\n[]: 2\n", "+M :a :1 +S -S :2 -M")]
#[case::empty_flow_map_key("a: 1\n{}: 2\n", "+M :a :1 +M -M :2 -M")]
#[case::multibyte_flow_key("a: 1\n[\"é\"]: 2\n", "+M :a :1 +S :é -S :2 -M")]
#[case::multibyte_alias_key("a: &ü 1\n*ü : 2\n", "+M :a &ü,:1 *ü :2 -M")]
#[case::flow_key_space_before_colon("a: 1\n[x] : 2\n", "+M :a :1 +S :x -S :2 -M")]
#[case::flow_key_two_spaces_after_colon("a: 1\n[x]:  2\n", "+M :a :1 +S :x -S :2 -M")]
#[case::alias_key_without_value("a: 1\n*y :\n", "+M :a :1 *y : -M")]
#[case::flow_key_without_value("a: 1\n[x]:\n", "+M :a :1 +S :x -S : -M")]
#[case::flow_key_without_value_then_key("a: 1\n[x]:\nb: 2\n", "+M :a :1 +S :x -S : :b :2 -M")]
#[case::flow_key_with_flow_value("a: 1\n[x]: [y]\n", "+M :a :1 +S :x -S +S :y -S -M")]
#[case::flow_key_with_alias_value("a: 1\n[x]: *y\n", "+M :a :1 +S :x -S *y -M")]
#[case::alias_key_with_alias_value("a: 1\n*y : *z\n", "+M :a :1 *y *z -M")]
#[case::alias_key_with_flow_value("a: 1\n*y : [z]\n", "+M :a :1 *y +S :z -S -M")]
#[case::flow_key_with_block_scalar_value("a: 1\n[x]: |\n  t\n", "+M :a :1 +S :x -S :t\\n -M")]
#[case::flow_key_with_nested_sequence_value("a: 1\n[x]:\n  - b\n", "+M :a :1 +S :x -S +S :b -S -M")]
#[case::flow_key_with_seq_space_sequence_value(
    "a: 1\n[x]:\n- b\n",
    "+M :a :1 +S :x -S +S :b -S -M"
)]
#[case::alias_key_with_nested_mapping_value("a: 1\n*y :\n  c: 3\n", "+M :a :1 *y +M :c :3 -M -M")]
#[case::flow_map_key_with_nested_mapping_value(
    "a: 1\n{x: 1}:\n  b: 2\n",
    "+M :a :1 +M :x :1 -M +M :b :2 -M -M"
)]
#[case::anchored_flow_key("a: 1\n&z [x]: 2\n", "+M :a :1 +S,&z :x -S :2 -M")]
#[case::tagged_flow_key("a: 1\n!!seq [x]: 2\n", "+M :a :1 +S,!!seq :x -S :2 -M")]
#[case::anchored_flow_key_after_empty_value("a:\n&k [x]: 2\n", "+M :a : +S,&k :x -S :2 -M")]
#[case::tagged_flow_key_after_empty_value("a:\n!!str [x]: 2\n", "+M :a : +S,!!str :x -S :2 -M")]
#[case::anchored_tagged_flow_map_key_after_empty_value(
    "a:\n&k !!map {x: 1}: 2\n",
    "+M :a : +M,&k,!!map :x :1 -M :2 -M"
)]
#[case::alias_key_after_anchored_value("a: &p 1\n*p : 2\n", "+M :a &p,:1 *p :2 -M")]
#[case::flow_key_after_comment_line("a: 1\n# c\n[x]: 2\n", "+M :a :1 +S :x -S :2 -M")]
#[case::flow_key_after_trailing_comment("a: 1 # c\n[x]: 2\n", "+M :a :1 +S :x -S :2 -M")]
#[case::flow_key_after_blank_line("a: 1\n\n[x]: 2\n", "+M :a :1 +S :x -S :2 -M")]
#[case::flow_key_with_crlf("a: 1\r\n[x]: 2\r\n", "+M :a :1 +S :x -S :2 -M")]
#[case::alias_keys_with_crlf("a: 1\r\n*y : 2\r\n*z : 3\r\n", "+M :a :1 *y :2 *z :3 -M")]
#[case::flow_key_without_final_newline("a: 1\n[x]: 2", "+M :a :1 +S :x -S :2 -M")]
#[case::flow_key_without_value_at_end_of_input("a: 1\n[x]:", "+M :a :1 +S :x -S : -M")]
#[case::alias_key_without_value_at_end_of_input("a:\n*y :", "+M :a : *y : -M")]
#[case::flow_key_in_indented_root("  a: 1\n  [x]: 2\n", "+M :a :1 +S :x -S :2 -M")]
#[case::alias_key_in_indented_root("  a: 1\n  *y : 2\n", "+M :a :1 *y :2 -M")]
#[case::flow_key_before_document_end("a: 1\n[x]: 2\n...\n", "+M :a :1 +S :x -S :2 -M")]
#[case::flow_key_before_next_document(
    "a: 1\n[x]: 2\n---\nb: 1\n",
    "+M :a :1 +S :x -S :2 -M +M :b :1 -M"
)]
#[case::flow_and_alias_keys_in_seq_items(
    "- *y : 2\n- [x]: 3\n",
    "+S +M *y :2 -M +M +S :x -S :3 -M -S"
)]
#[case::double_quoted_key_after_entry("a: 1\n\"q\": 2\n", "+M :a :1 :q :2 -M")]
#[case::single_quoted_key_after_entry("a: 1\n'q': 2\n", "+M :a :1 :q :2 -M")]
#[case::anchored_quoted_key_after_entry("a: 1\n&z \"q\": 2\n", "+M :a :1 &z,:q :2 -M")]
#[case::alias_key_after_entry("a: 1\n*y : 2\n", "+M :a :1 *y :2 -M")]
#[case::alias_key_after_value_anchor("a: &y 1\n*y : 3\n", "+M :a &y,:1 *y :3 -M")]
#[case::alias_keys_with_following_plain_key("a: 1\n*y : 2\nb: 3\n", "+M :a :1 *y :2 :b :3 -M")]
#[case::alias_key_after_second_explicit_entry("? a\n: b\n*y : 3\n", "+M :a :b *y :3 -M")]
#[case::alias_key_in_explicit_key("? *a : 1\n: v\n", "+M +M *a :1 -M :v -M")]
#[case::alias_in_seq_item("- *a : 1\n", "+S +M *a :1 -M -S")]
#[case::alias_with_comment_in_seq_item("- *a # c\n", "+S *a -S")]
#[case::alias_named_with_colon_in_seq_item("- *a:\n", "+S *a: -S")]
#[case::alias_named_with_colon_in_map_value("a: *b:c\n", "+M :a *b:c -M")]
#[case::flow_key_in_seq_item_mapping_first_key_after_entry(
    "- a: 1\n  {x: 1}: 2\n",
    "+S +M :a :1 +M :x :1 -M :2 -M -S"
)]
#[case::flow_value_at_mapping_column_after_empty_value("a:\n[x]\n", "+M :a +S :x -S -M")]
#[case::alias_value_at_mapping_column("a:\n*y\n", "+M :a *y -M")]
#[case::deep_flow_key_after_entry(
    "a: 1\n[[[[x]]]]: 1\n",
    "+M :a :1 +S +S +S +S :x -S -S -S -S :1 -M"
)]
#[case::flow_key_tab_before_value("a: 1\n[x]:\t1\n", "+M :a :1 +S :x -S :1 -M")]
#[case::alias_key_tab_value("a: 1\n*y :\t1\n", "+M :a :1 *y :1 -M")]
#[case::quoted_bracket_in_flow_key("a: 1\n['a]']: 1\n", "+M :a :1 +S :a] -S :1 -M")]
#[case::plain_key_after_tagged_empty_value("a: !!str\nb: 1\n", "+M :a !!str,: :b :1 -M")]
#[case::alias_key_after_tagged_empty_value("a: !!str\n*z : 2\n", "+M :a !!str,: *z :2 -M")]
#[case::flow_value_with_anchor_at_mapping_column("a: &y\n[x]\n", "+M :a +S,&y :x -S -M")]
#[case::flow_value_with_tag_at_mapping_column("a: !!str\n[x]\n", "+M :a +S,!!str :x -S -M")]
#[case::anchored_tagged_flow_seq_key_after_entry(
    "a: 1\n&k !!seq [x]: 2\n",
    "+M :a :1 +S,&k,!!seq :x -S :2 -M"
)]
#[case::tagged_anchored_flow_map_key_after_entry(
    "a: 1\n!!map &k {x: 1}: 2\n",
    "+M :a :1 +M,&k,!!map :x :1 -M :2 -M"
)]
#[case::anchored_tagged_flow_map_key_in_seq_item(
    "- a: 1\n  &k !!map {x: 1}: 2\n",
    "+S +M :a :1 +M,&k,!!map :x :1 -M :2 -M -S"
)]
#[case::tag_and_anchor_on_empty_value_before_flow_key(
    "a: &y !!str\n[x]: 1\nz:\n  c: 1\n",
    "+M :a &y,!!str,: +S :x -S :1 :z +M :c :1 -M -M"
)]
#[case::tag_then_anchor_on_empty_value_before_flow_key(
    "a: !!str &y\n[x]: 1\nz:\n  c: 1\n",
    "+M :a &y,!!str,: +S :x -S :1 :z +M :c :1 -M -M"
)]
#[case::tag_then_anchor_on_empty_value_before_alias_key(
    "a: !!str &y\n*k : 1\nz:\n  c: 1\n",
    "+M :a &y,!!str,: *k :1 :z +M :c :1 -M -M"
)]
#[case::anchor_and_tag_on_empty_value_before_alias_key(
    "a: &y !!str\n*z : 1\n",
    "+M :a &y,!!str,: *z :1 -M"
)]
#[case::anchor_and_tag_on_empty_value_before_plain_key(
    "a: &y !!str\nb: 1\nz:\n  c: 1\n",
    "+M :a &y,!!str,: :b :1 :z +M :c :1 -M -M"
)]
#[case::anchor_on_empty_value_before_anchored_plain_key(
    "a: &y\n&k b: 1\nz:\n  c: 1\n",
    "+M :a &y,: &k,:b :1 :z +M :c :1 -M -M"
)]
#[case::anchor_on_empty_value_before_anchored_flow_key(
    "a: &y\n&k [x]: 2\nz:\n  c: 1\n",
    "+M :a &y,: +S,&k :x -S :2 :z +M :c :1 -M -M"
)]
#[case::anchored_empty_explicit_key_before_flow_key("? &y\n[x]: 1\n", "+M &y,: : +S :x -S :1 -M")]
#[case::tagged_empty_explicit_key_before_flow_map_key(
    "? !!str\n{x: 1}: 1\n",
    "+M !!str,: : +M :x :1 -M :1 -M"
)]
#[case::anchored_empty_explicit_key_in_seq_item_before_flow_key(
    "- ? &y\n  [x]: 1\n",
    "+S +M &y,: : +S :x -S :1 -M -S"
)]
#[case::anchored_empty_explicit_key_on_own_line_before_flow_key(
    "?\n  &k\n[x]: 2\n",
    "+M &k,: : +S :x -S :2 -M"
)]
#[case::anchored_empty_explicit_key_before_alias_key("? &y\n*y : 1\n", "+M &y,: : *y :1 -M")]
#[case::tagged_empty_value_before_anchored_flow_key(
    "a: !!str\n&k [x]: 2\n",
    "+M :a !!str,: +S,&k :x -S :2 -M"
)]
#[case::anchored_empty_value_before_tagged_flow_key(
    "a: &y\n!!seq [x]: 2\n",
    "+M :a &y,: +S,!!seq :x -S :2 -M"
)]
#[case::anchor_then_tag_on_flow_value_at_mapping_column(
    "a: &y\n!!seq [x]\n",
    "+M :a +S,&y,!!seq :x -S -M"
)]
#[case::tag_then_anchor_on_flow_value_at_mapping_column(
    "a: !!str\n&k [x]\n",
    "+M :a +S,&k,!!str :x -S -M"
)]
#[case::tag_before_alias_key_opens_root_mapping("!!str\n*y : 2\n", "+M,!!str *y :2 -M")]
#[case::anchor_and_tag_before_alias_key_open_root_mapping(
    "&m !!map\n*y : 2\n",
    "+M,&m,!!map *y :2 -M"
)]
#[case::tag_before_alias_key_opens_nested_mapping_on_value_line(
    "k: !!map\n  *y : 2\n",
    "+M :k +M,!!map *y :2 -M -M"
)]
#[case::tag_before_alias_key_opens_nested_mapping(
    "k:\n  !!map\n  *y : 2\n",
    "+M :k +M,!!map *y :2 -M -M"
)]
#[case::anchor_and_tag_on_empty_explicit_key_before_flow_key(
    "? &y !!str\n[x]: 1\n",
    "+M &y,!!str,: : +S :x -S :1 -M"
)]
#[case::tag_and_anchor_on_empty_explicit_key_before_flow_key(
    "? !!str &y\n[x]: 1\n",
    "+M &y,!!str,: : +S :x -S :1 -M"
)]
#[case::anchor_and_tag_on_own_line_of_empty_explicit_key(
    "?\n  &y !!str\n[x]: 1\n",
    "+M &y,!!str,: : +S :x -S :1 -M"
)]
#[case::anchor_and_tag_on_empty_explicit_key_in_seq_item(
    "- ? &y !!str\n  [x]: 1\n",
    "+S +M &y,!!str,: : +S :x -S :1 -M -S"
)]
#[case::anchor_and_tag_on_empty_explicit_key_before_plain_key(
    "? &y !!str\nb: 1\n",
    "+M &y,!!str,: : :b :1 -M"
)]
#[case::anchor_on_empty_explicit_key_before_anchored_flow_key(
    "? &y\n&k [x]: 1\n",
    "+M &y,: : +S,&k :x -S :1 -M"
)]
#[case::anchor_on_empty_explicit_key_before_anchored_plain_key(
    "? &y\n&k b: 1\n",
    "+M &y,: : &k,:b :1 -M"
)]
#[case::anchor_and_tag_on_empty_explicit_key_with_value(
    "? &y !!str\n: v\nz:\n  c: 1\n",
    "+M &y,!!str,: :v :z +M :c :1 -M -M"
)]
#[case::tag_on_empty_explicit_key_before_alias_key("? !!str\n*y : 1\n", "+M !!str,: : *y :1 -M")]
#[case::tag_line_inside_explicit_key_before_alias_key(
    "?\n  !!str\n*y : 2\n",
    "+M !!str,: : *y :2 -M"
)]
#[case::tag_on_empty_explicit_key_in_seq_item_before_alias_key(
    "- ? !!str\n  *y : 1\n",
    "+S +M !!str,: : *y :1 -M -S"
)]
#[case::tag_on_empty_explicit_key_after_entry_before_alias_key(
    "x: 1\n? !!str\n*y : 1\n",
    "+M :x :1 !!str,: : *y :1 -M"
)]
#[case::anchor_and_tag_on_empty_explicit_key_before_alias_key(
    "? &y !!str\n*y : 1\n",
    "+M &y,!!str,: : *y :1 -M"
)]
#[case::anchor_line_then_tag_line_before_plain_key("&y\n!!str\nb: 2\n", "+M,&y,!!str :b :2 -M")]
#[case::anchor_line_then_inline_anchor_on_key("&a\n&b k: v\n", "+M,&a &b,:k :v -M")]
#[case::anchor_and_tag_line_then_inline_anchor_on_key(
    "&a !!map\n&b k: v\n",
    "+M,&a,!!map &b,:k :v -M"
)]
#[case::anchor_line_before_block_sequence("&a\n- x\n", "+S,&a :x -S")]
#[case::anchored_empty_values_in_succession("a: &y\nb: &k\nc: 1\n", "+M :a &y,: :b &k,: :c :1 -M")]
#[case::displaced_anchor_on_quoted_value_then_anchored_mapping(
    "a: &y !!str\n  \"q\"\nb: &k\nc: 1\n",
    "+M :a &y,!!str,:q :b &k,: :c :1 -M"
)]
#[case::displaced_anchor_on_quoted_value_then_anchored_nested_mapping(
    "a: &y !!str\n  \"q\"\nb:\n  &k\n  c: 1\n",
    "+M :a &y,!!str,:q :b +M,&k :c :1 -M -M"
)]
#[case::displaced_anchor_on_plain_value_then_anchored_nested_mapping(
    "a: &y !!str\n  w\nb:\n  &k\n  c: 1\n",
    "+M :a &y,!!str,:w :b +M,&k :c :1 -M -M"
)]
#[case::displaced_anchor_on_flow_value_then_anchored_nested_mapping(
    "a: &y !!str\n  [x]\nb:\n  &k\n  c: 1\n",
    "+M :a +S,&y,!!str :x -S :b +M,&k :c :1 -M -M"
)]
#[case::displaced_anchor_on_quoted_value_then_anchored_sequence(
    "a: &y !!str\n  \"q\"\nb:\n  &k\n  - c\n",
    "+M :a &y,!!str,:q :b +S,&k :c -S -M"
)]
#[case::displaced_anchor_on_quoted_value_then_anchored_key(
    "a: &y !!str\n  \"q\"\n&k b: 1\n",
    "+M :a &y,!!str,:q &k,:b :1 -M"
)]
#[case::displaced_anchor_on_empty_item_then_anchored_mapping_item(
    "- &y !!str\n- b\n- &k\n  c: 1\n",
    "+S &y,!!str,: :b +M,&k :c :1 -M -S"
)]
#[case::displaced_anchor_on_empty_item_then_anchored_empty_item(
    "- &y !!str\n- &k\n",
    "+S &y,!!str,: &k,: -S"
)]
#[case::displaced_anchor_on_empty_item_then_anchored_mapping(
    "- &y !!str\n- &k\n  c: 1\n",
    "+S &y,!!str,: +M,&k :c :1 -M -S"
)]
#[case::displaced_anchor_on_quoted_item_then_anchored_quoted_item(
    "- &y !!str\n  \"q\"\n- &k\n  \"r\"\n",
    "+S &y,!!str,:q &k,:r -S"
)]
#[case::displaced_anchor_on_explicit_key_then_anchored_value(
    "? &y !!str\n  \"q\"\n: &k\n  \"r\"\n",
    "+M &y,!!str,:q &k,:r -M"
)]
#[case::displaced_anchor_on_explicit_key_then_tagged_anchored_value(
    "? &y !!str\n  \"q\"\n: !!str &k\n  \"r\"\n",
    "+M &y,!!str,:q &k,!!str,:r -M"
)]
#[case::displaced_anchor_on_quoted_value_then_z_mapping(
    "a: &y !!str\n  \"q\"\nz:\n  c: 1\n",
    "+M :a &y,!!str,:q :z +M :c :1 -M -M"
)]
#[case::displaced_anchor_on_flow_value_then_z_mapping(
    "a: &y !!str\n  [x]\nz:\n  c: 1\n",
    "+M :a +S,&y,!!str :x -S :z +M :c :1 -M -M"
)]
#[case::tag_then_anchor_on_quoted_value_then_anchored_mapping(
    "a: !!str &y\n  \"q\"\nb: &k\n  c: 1\n",
    "+M :a &y,!!str,:q :b +M,&k :c :1 -M -M"
)]
#[case::displaced_anchor_on_flow_item("- &y !!str\n- [b]\n", "+S &y,!!str,: +S :b -S -S")]
#[case::tag_then_anchor_on_root_quoted_scalar("!!str &y\n\"q\"\n", "&y,!!str,:q")]
#[case::anchor_then_tag_on_root_quoted_scalar("&y !!str\n\"q\"\n", "&y,!!str,:q")]
#[case::anchor_then_tag_on_root_plain_scalar("&y !!str\nw\n", "&y,!!str,:w")]
#[case::anchor_then_tag_on_root_flow_sequence("&y !!str\n[x]\n", "+S,&y,!!str :x -S")]
#[case::anchored_collection_values_after_tagged_anchor_lines(
    "x: 1\ny: &y !!str\n  - a\nz: &k\n  - b\n",
    "+M :x :1 :y +S,&y,!!str :a -S :z +S,&k :b -S -M"
)]
#[case::anchor_and_map_tag_on_value_mapping(
    "a: &y !!map\n  b: 1\nc: 1\n",
    "+M :a +M,&y,!!map :b :1 -M :c :1 -M"
)]
#[case::displaced_anchor_in_nested_empty_value_before_flow_key(
    "a:\n  b: &y !!str\n[x]: 1\nz:\n  c: 1\n",
    "+M :a +M :b &y,!!str,: -M +S :x -S :1 :z +M :c :1 -M -M"
)]
#[case::displaced_anchor_in_nested_empty_value_before_alias_key(
    "a:\n  b: &y !!str\n*x : 1\nz:\n  c: 1\n",
    "+M :a +M :b &y,!!str,: -M *x :1 :z +M :c :1 -M -M"
)]
#[case::displaced_anchor_in_nested_empty_value_before_mapping(
    "a:\n  b: &y !!str\nc:\n  d: 1\n",
    "+M :a +M :b &y,!!str,: -M :c +M :d :1 -M -M"
)]
#[case::displaced_tag_then_anchor_in_nested_empty_value_before_mapping(
    "a:\n  b: !!str &y\nc:\n  d: 1\n",
    "+M :a +M :b &y,!!str,: -M :c +M :d :1 -M -M"
)]
#[case::displaced_anchor_in_nested_sequence_mapping_before_mapping(
    "a:\n  - b: &y !!str\nc:\n  d: 1\n",
    "+M :a +S +M :b &y,!!str,: -M -S :c +M :d :1 -M -M"
)]
#[case::displaced_anchor_in_seq_item_mapping_before_mapping_item(
    "- a: &y !!str\n- b:\n    c: 1\n",
    "+S +M :a &y,!!str,: -M +M :b +M :c :1 -M -M -S"
)]
#[case::displaced_anchor_in_seq_item_mapping_before_anchored_mapping_item(
    "- a: &y !!str\n- &k\n  c: 1\n",
    "+S +M :a &y,!!str,: -M +M,&k :c :1 -M -S"
)]
#[case::displaced_anchor_on_value_at_end_of_input("a: &y !!str\n", "+M :a &y,!!str,: -M")]
#[case::displaced_anchor_on_value_before_document_end("a: &y !!str\n...\n", "+M :a &y,!!str,: -M")]
#[case::displaced_anchor_on_value_before_document_start(
    "a: &y !!str\n---\nb: 1\n",
    "+M :a &y,!!str,: -M +M :b :1 -M"
)]
#[case::displaced_anchor_on_empty_item_at_end_of_input("- &y !!str\n", "+S &y,!!str,: -S")]
#[case::displaced_anchor_on_empty_item_before_document_end("- &y !!str\n...\n", "+S &y,!!str,: -S")]
#[case::displaced_anchor_on_empty_explicit_key_at_end_of_input(
    "? &y !!str\n",
    "+M &y,!!str,: : -M"
)]
#[case::displaced_anchor_on_empty_explicit_key_before_document_end(
    "? &y !!str\n...\n",
    "+M &y,!!str,: : -M"
)]
fn flow_and_alias_keys_after_finished_entry_emit_libfyaml_events(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(event_shape(input, Tags::Listed), expected, "{input:?}");
    assert_well_formed(input);
    assert_eq!(
        load_shape(input),
        event_shape(input, Tags::Omitted),
        "load() of {input:?}"
    );
}

// Properties still pending at `---` or `...` belong to the document they end:
// the next document starts without them.  (libfyaml also gives the property a
// null node of its own in the first document; this parser emits none, a known
// deviation.)
#[rstest]
#[case::anchor_line_before_marker_then_anchored_key("&y\n---\n&k\nb: 1\n", "+M,&k :b :1 -M")]
#[case::anchor_line_before_end_marker_then_anchored_key("&y\n...\n&k\nb: 1\n", "+M,&k :b :1 -M")]
#[case::anchor_line_before_marker_then_inline_anchored_key("&y\n--- &k\nb: 1\n", "+M,&k :b :1 -M")]
#[case::anchor_line_before_marker_then_anchored_scalar("&y\n---\n&k\n", "&k,:")]
#[case::anchor_line_before_marker_then_plain_key("&y\n---\nb: 1\n", "+M :b :1 -M")]
#[case::tag_line_before_marker_then_tagged_key("!!str\n---\n!!map\nb: 1\n", "+M,!!map :b :1 -M")]
#[case::tag_line_before_marker_then_inline_tagged_key(
    "!!str\n--- !!map\nb: 1\n",
    "+M,!!map :b :1 -M"
)]
#[case::anchor_line_before_marker_then_alias("&y\n---\n*y\n", "*y")]
fn pending_properties_do_not_carry_into_the_next_document(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(event_shape(input, Tags::Listed), expected, "{input:?}");
}

// Resolved mode expands the alias key into the scalar it points at.
#[test]
fn alias_key_after_flow_key_resolves_to_the_anchored_scalar() {
    let docs = LoaderBuilder::new()
        .resolved()
        .build()
        .load("x: &y 1\n[x]: 2\n*y : 3\n")
        .unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("root must be a mapping");
    };
    assert_eq!(entries.len(), 3);
    assert!(matches!(&entries[2].0, Node::Scalar { value, .. } if value == "1"));
}

// A flow or alias key after a nested block must close every level first and
// then join the outer mapping, however deep the nesting is.
#[rstest]
#[case::flow_key("[x]: 1", "+S :x -S :1")]
#[case::alias_key("*y : 1", "*y :1")]
fn key_after_deeply_nested_mapping_closes_every_level(#[case] key: &str, #[case] entry: &str) {
    let depth: usize = 100;
    let nested: String = (0..depth)
        .flat_map(|level| ["  ".repeat(level), format!("k{level}:\n")])
        .collect();
    let input = format!("a: &y 0\n{nested}{key}\n");
    assert_well_formed(&input);
    let closers = vec!["-M"; depth - 1].join(" ");
    assert!(
        event_shape(&input, Tags::Omitted)
            .ends_with(&format!(":k{} : {closers} {entry} -M", depth - 1)),
        "{key}"
    );
}

// The error of a stray token is reported at its first byte: column counts
// characters, byte offset counts bytes.
#[rstest]
#[case::flow_seq_then_flow_seq("- [x] [y]\n", 1, 6, 6)]
#[case::alias_then_plain_map_value("a: *b w\n", 1, 6, 6)]
#[case::tab_after_flow_seq_is_skipped("- [x]\tw\n", 1, 6, 6)]
#[case::after_multiline_flow_seq("a: [x,\n  y] w\n", 2, 5, 12)]
#[case::explicit_key_flow_seq_then_flow_seq("? [x] [y]\n: v\n", 1, 6, 6)]
#[case::multibyte_inside_flow_seq("- [é] w\n", 1, 6, 7)]
#[case::multibyte_inside_double_quoted("- \"é\" w\n", 1, 5, 6)]
#[case::multibyte_inside_single_quoted("- 'é' w\n", 1, 5, 6)]
#[case::single_quoted_in_seq_item("- 'q' w\n", 1, 5, 5)]
#[case::single_quoted_map_value("a: 'q' w\n", 1, 6, 6)]
#[case::multiline_single_quoted_in_seq_item("- 'a\n  b' w\n", 2, 4, 9)]
#[case::multiline_single_quoted_map_value("a: 'a\n  b' w\n", 2, 4, 10)]
#[case::single_quoted_after_document_start("--- 'q' w\n", 1, 7, 7)]
#[case::single_quoted_with_escaped_quote("- 'it''s' w\n", 1, 9, 9)]
#[case::dash_after_map_value("a: 1\n- b\n", 2, 0, 5)]
#[case::dash_after_explicit_key("? k\n- b\n: v\n", 2, 0, 4)]
#[case::bare_property_after_map_value("a: \"q\"\n&x\n", 2, 0, 7)]
#[case::crlf_bare_property_after_map_value("a: \"q\"\r\n&x\r\n", 2, 0, 8)]
fn stray_token_error_points_at_its_first_byte(
    #[case] input: &str,
    #[case] line: usize,
    #[case] column: usize,
    #[case] byte_offset: usize,
) {
    let err = first_error(input).expect("expected a parse error");
    assert_eq!(
        (err.pos.line, err.pos.column, err.pos.byte_offset),
        (line, column, byte_offset)
    );
    assert!(input.is_char_boundary(err.pos.byte_offset));
}
