// SPDX-License-Identifier: MIT
//
// Integration tests verifying that container nodes (Mapping, Sequence) carry
// full spans — `loc.start` from the opening token, `loc.end` from the closing
// token. These tests exercise the public `load()` API.
//
// Ported from rlsp-yaml-parser/tests/loader_spans.rs with import paths updated.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
#![allow(clippy::panic)]

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
    assert_eq!(
        loc.start.line, 1,
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
    assert_eq!(loc.start.line, 1);
}

/// Test 3 — block mapping span end reaches the last value's line.
#[test]
fn block_mapping_span_end_is_last_value_line() {
    let docs = load("a: 1\nb: 2\n").unwrap();
    let Node::Mapping { loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    assert!(
        loc.end.line >= 2,
        "mapping end should reach at least line 2, got {}",
        loc.end.line
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
    assert!(
        loc.end.line >= 2,
        "inner mapping end should reach at least line 2, got {}",
        loc.end.line
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
    assert_eq!(loc.start.line, 1);
}

/// Test 9 — block sequence span end reaches the last item's line.
#[test]
fn block_sequence_span_end_is_last_item_line() {
    let docs = load("- a\n- b\n").unwrap();
    let Node::Sequence { loc, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    assert!(
        loc.end.line >= 2,
        "sequence end should reach at least line 2, got {}",
        loc.end.line
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
    assert!(
        loc.end.line >= 4,
        "sequence end should reach at least line 4, got {}",
        loc.end.line
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
    assert_eq!(loc.start.line, 1, "scalar should start on line 1 (1-based)");
    assert_ne!(
        loc.start, loc.end,
        "scalar span must be non-zero (regression: loader fix must not break scalar spans)"
    );
}
