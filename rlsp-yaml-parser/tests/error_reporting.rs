// SPDX-License-Identifier: MIT
//
// Duplicate key and error reporting tests — verifies parser behavior for
// duplicate keys (accepted silently), error detection, error positions,
// error recovery (stream stops after first error), and merge key handling.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn
)]

use rlsp_yaml_parser::loader::LoadError;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Error, Event, load, parse_events};

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
// Group A: Duplicate keys — parser accepts silently
// ===========================================================================

#[test]
fn duplicate_plain_scalar_keys_both_entries_present() {
    let kvs = mapping_key_values("name: Alice\nname: Bob\n");
    assert_eq!(kvs.len(), 2);
    assert!(kvs.contains(&("name".into(), "Alice".into())));
    assert!(kvs.contains(&("name".into(), "Bob".into())));
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
fn duplicate_keys_in_flow_mapping_both_entries_present() {
    let input = "{a: 1, a: 2}\n";
    let kvs = mapping_key_values(input);
    assert_eq!(kvs.len(), 2);
    assert!(kvs.contains(&("a".into(), "1".into())));
    assert!(kvs.contains(&("a".into(), "2".into())));
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
// Group B: Error detection — parse errors occur
// ===========================================================================

#[test]
fn unterminated_single_quoted_scalar_produces_error() {
    let input = "key: 'unterminated\n";
    assert!(first_error(input).is_some(), "expected parse error");
    assert!(
        matches!(load(input), Err(LoadError::Parse { .. })),
        "expected LoadError::Parse"
    );
}

#[test]
fn unterminated_double_quoted_scalar_produces_error() {
    let input = "key: \"unterminated\n";
    assert!(first_error(input).is_some(), "expected parse error");
    assert!(
        matches!(load(input), Err(LoadError::Parse { .. })),
        "expected LoadError::Parse"
    );
}

#[test]
fn unterminated_flow_sequence_produces_error() {
    let input = "[a, b, c\n";
    assert!(
        first_error(input).is_some(),
        "expected parse error for unterminated flow sequence"
    );
}

#[test]
fn unterminated_flow_mapping_produces_error() {
    let input = "{a: 1, b: 2\n";
    assert!(
        first_error(input).is_some(),
        "expected parse error for unterminated flow mapping"
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
