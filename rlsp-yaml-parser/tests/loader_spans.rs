// SPDX-License-Identifier: MIT
//
// Integration tests verifying that container nodes (Mapping, Sequence) carry
// full spans — `loc.start` from the opening token, `loc.end` from the closing
// token. These tests exercise the public `load()` API.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    missing_docs,
    reason = "test code"
)]

use rstest::rstest;

use rlsp_yaml_parser::loader::load;
use rlsp_yaml_parser::node::Node;

// ---------------------------------------------------------------------------
// Group: Mapping container spans
// ---------------------------------------------------------------------------

/// Test 1 — block mapping span is non-zero (covers its full extent).
#[test]
fn block_mapping_span_covers_full_extent() {
    let docs = load("key: value\n").unwrap();
    let Node::Mapping { loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let line_index = docs[0].line_index();
    assert_eq!(
        line_index.line_column(loc.start).0,
        1,
        "mapping should start on line 1 (1-based)"
    );
    assert_ne!(loc.start, loc.end, "mapping span must not be zero");
}

/// Test 2 — block mapping span start is on the first key's line.
#[test]
fn block_mapping_span_start_is_first_key_line() {
    let docs = load("key: value\n").unwrap();
    let Node::Mapping { loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let line_index = docs[0].line_index();
    assert_eq!(line_index.line_column(loc.start).0, 1);
}

/// Test 3 — block mapping span end reaches the last value's line.
#[test]
fn block_mapping_span_end_is_last_value_line() {
    let docs = load("a: 1\nb: 2\n").unwrap();
    let Node::Mapping { loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let line_index = docs[0].line_index();
    let end_line = line_index.line_column(loc.end).0;
    assert!(
        end_line >= 2,
        "mapping end should reach at least line 2, got {end_line}"
    );
}

/// Test 4 — nested mapping's inner span end reaches the inner value's line.
#[test]
fn nested_mapping_outer_span_covers_inner() {
    let docs = load("outer:\n  inner: value\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let (_, val) = entries.first().expect("expected at least one entry");
    let Node::Mapping { loc, .. } = val else {
        panic!("expected nested Mapping value");
    };
    let line_index = docs[0].line_index();
    let end_line = line_index.line_column(loc.end).0;
    assert!(
        end_line >= 2,
        "inner mapping end should reach at least line 2, got {end_line}"
    );
}

// ---------------------------------------------------------------------------
// Mapping span is non-zero — flow and empty variants
// ---------------------------------------------------------------------------

#[rstest]
#[case::flow_mapping("{a: 1, b: 2}\n")]
#[case::empty_flow_mapping("{}\n")]
fn mapping_span_is_non_zero(#[case] input: &str) {
    let docs = load(input).unwrap();
    let Node::Mapping { loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    assert_ne!(loc.start, loc.end, "mapping span must not be zero");
}

// ---------------------------------------------------------------------------
// Group: Sequence container spans
// ---------------------------------------------------------------------------

/// Test 8 — block sequence span start is on the first item's line.
#[test]
fn block_sequence_span_start_is_first_item_line() {
    let docs = load("- a\n- b\n").unwrap();
    let Node::Sequence { loc, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    let line_index = docs[0].line_index();
    assert_eq!(line_index.line_column(loc.start).0, 1);
}

/// Test 9 — block sequence span end reaches the last item's line.
#[test]
fn block_sequence_span_end_is_last_item_line() {
    let docs = load("- a\n- b\n").unwrap();
    let Node::Sequence { loc, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    let line_index = docs[0].line_index();
    let end_line = line_index.line_column(loc.end).0;
    assert!(
        end_line >= 2,
        "sequence end should reach at least line 2, got {end_line}"
    );
}

// ---------------------------------------------------------------------------
// Sequence span is non-zero — block, flow and empty variants
// ---------------------------------------------------------------------------

#[rstest]
#[case::block_sequence("- a\n- b\n")]
#[case::flow_sequence("[a, b, c]\n")]
#[case::empty_flow_sequence("[]\n")]
fn sequence_span_is_non_zero(#[case] input: &str) {
    let docs = load(input).unwrap();
    let Node::Sequence { loc, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    assert_ne!(loc.start, loc.end, "sequence span must not be zero");
}

/// Test 12 — sequence-of-mappings outer span covers all items.
#[test]
fn sequence_of_mappings_outer_span_covers_all_items() {
    let docs = load("- name: Alice\n  age: 30\n- name: Bob\n  age: 25\n").unwrap();
    let Node::Sequence { loc, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    let line_index = docs[0].line_index();
    let end_line = line_index.line_column(loc.end).0;
    assert!(
        end_line >= 4,
        "sequence end should reach at least line 4, got {end_line}"
    );
}

// ---------------------------------------------------------------------------
// Group: Scalar spans are unchanged (regression guard)
// ---------------------------------------------------------------------------

/// Test 13 — scalar spans are not affected by the container span fix.
///
/// Scalars already carry correct spans (start != end). This test confirms
/// the loader fix did not accidentally alter scalar span handling.
#[test]
fn scalar_span_unchanged_after_container_fix() {
    let docs = load("hello\n").unwrap();
    let Node::Scalar { loc, .. } = &docs[0].root else {
        panic!("expected Scalar");
    };
    let line_index = docs[0].line_index();
    assert_eq!(
        line_index.line_column(loc.start).0,
        1,
        "scalar should start on line 1 (1-based)"
    );
    assert_ne!(
        loc.start, loc.end,
        "scalar span must be non-zero (regression: loader fix must not break scalar spans)"
    );
}

// ---------------------------------------------------------------------------
// DLI-*: Document::line_index() accessor tests (Task C)
// ---------------------------------------------------------------------------

/// DLI-1: `line_index` accessible after load.
#[test]
fn document_line_index_accessible_after_load() {
    let docs = load("key: value\n").unwrap();
    let _ = docs[0].line_index(); // must not panic
}

/// DLI-2: origin maps to line 1.
#[test]
fn document_line_index_origin_is_line_one() {
    let docs = load("hello\n").unwrap();
    assert_eq!(docs[0].line_index().line_column(0), (1, 0));
}

/// DLI-3: span start offset consistent with expected line/col.
#[test]
fn document_line_index_consistent_with_span_start_offset() {
    let docs = load("key: value\n").unwrap();
    let Node::Mapping { loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let (line, col) = docs[0].line_index().line_column(loc.start);
    assert_eq!(line, 1, "key starts at line 1");
    assert_eq!(col, 0, "key starts at column 0");
}

/// DLI-4: span end offset on last line for multiline mapping.
#[test]
fn document_line_index_consistent_with_span_end_offset_multiline() {
    let docs = load("a: 1\nb: 2\n").unwrap();
    let Node::Mapping { loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let (end_line, _) = docs[0].line_index().line_column(loc.end);
    assert!(end_line >= 2, "mapping end should be on line 2 or later");
}

// ---------------------------------------------------------------------------
// SA-*: Span accessor methods (Task C)
// ---------------------------------------------------------------------------

/// SA-1: start/end `line_column` for ASCII scalar.
#[test]
fn span_start_line_column_ascii_scalar() {
    let docs = load("hello\n").unwrap();
    let Node::Scalar { loc, .. } = &docs[0].root else {
        panic!("expected Scalar");
    };
    let idx = docs[0].line_index();
    assert_eq!(loc.start_line_column(idx), (1, 0));
    assert_eq!(loc.end_line_column(idx), (1, 5));
}

/// SA-2: multibyte key column count is in codepoints.
#[test]
fn span_start_line_column_multibyte_key() {
    let docs = load("日本語: val\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let (key, _) = &entries[0];
    let Node::Scalar { loc, .. } = key else {
        panic!("expected Scalar key");
    };
    let idx = docs[0].line_index();
    assert_eq!(loc.start_line_column(idx), (1, 0));
    assert_eq!(loc.end_line_column(idx), (1, 3)); // 3 codepoints
}

/// SA-3: value after multibyte key has correct column.
#[test]
fn span_start_line_column_value_after_multibyte_key() {
    let docs = load("日本語: val\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let (_, val) = &entries[0];
    let Node::Scalar { loc, .. } = val else {
        panic!("expected Scalar value");
    };
    let idx = docs[0].line_index();
    // "日本語" = 3 cp, ": " = 2 cp → val starts at col 5
    assert_eq!(loc.start_line_column(idx).1, 5);
}

/// SA-4: second line key has line=2, col=0.
#[test]
fn span_accessor_second_line_column_zero() {
    let docs = load("a: 1\nb: 2\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let (key2, _) = &entries[1];
    let Node::Scalar { loc, .. } = key2 else {
        panic!("expected Scalar key");
    };
    let idx = docs[0].line_index();
    let (line, col) = loc.start_line_column(idx);
    assert_eq!(line, 2, "second key should be on line 2");
    assert_eq!(col, 0, "second key should be at column 0");
}
