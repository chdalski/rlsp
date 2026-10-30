// SPDX-License-Identifier: MIT
//
// Integration tests verifying that container nodes (Mapping, Sequence) carry
// full spans — `loc.start` from the opening token, `loc.end` from the closing
// token. These tests exercise the public `load()` API.

#![expect(missing_docs, reason = "test code")]

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

/// A flow or alias key after a finished entry is an entry of the same mapping:
/// the root mapping's span covers it, and the key starts on its own line.
#[rstest]
#[case::flow_seq_key("a: 1\n[x]: 2\n")]
#[case::flow_map_key("a: 1\n{x: 1}: 2\n")]
#[case::alias_key("a: &y 1\n*y : 2\n")]
fn flow_or_alias_key_after_entry_is_covered_by_the_root_mapping_span(#[case] input: &str) {
    let docs = load(input).unwrap();
    let Node::Mapping { entries, loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    assert_eq!(entries.len(), 2, "no nested mapping");
    let line_index = docs[0].line_index();
    let key_loc = match &entries[1].0 {
        Node::Sequence { loc, .. } | Node::Mapping { loc, .. } | Node::Alias { loc, .. } => *loc,
        Node::Scalar { .. } => panic!("expected a collection or alias key"),
    };
    assert_eq!(line_index.line_column(key_loc.start), (2, 0));
    assert_eq!(line_index.line_column(loc.start), (1, 0));
    assert!(line_index.line_column(loc.end).0 >= 2);
    assert!(loc.end >= key_loc.end, "root span ends before the last key");
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

// ---------------------------------------------------------------------------
// Group: Multi-line plain scalar spans
// ---------------------------------------------------------------------------

/// The first mapping value or sequence item of the root collection, with the
/// root collection's own span; `None` when the root is not a non-empty collection.
fn first_child_and_container_loc(
    root: &Node<rlsp_yaml_parser::Span>,
) -> Option<(&Node<rlsp_yaml_parser::Span>, rlsp_yaml_parser::Span)> {
    match root {
        Node::Mapping { entries, loc, .. } => entries.first().map(|(_, v)| (v, *loc)),
        Node::Sequence { items, loc, .. } => items.first().map(|item| (item, *loc)),
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

/// A multi-line plain scalar's `loc` ends at its last content byte while the
/// enclosing collection's `loc.end` still includes the trailing blank lines.
#[rstest]
#[case::mapping_value("a:\n  v\n  w\n", 5, 10, 11)]
#[case::mapping_value_eof_without_newline("a:\n  v\n  w", 5, 10, 10)]
#[case::blank_lines_before_next_key("a:\n  v\n  w\n\n\nb: 1\n", 5, 10, 18)]
#[case::sequence_item("- v\n  w\n\n- z\n", 2, 7, 13)]
#[case::comment_line_after_value("a:\n  v\n  w\n# d\nb: 1\n", 5, 10, 20)]
#[case::trailing_comment_on_last_line("a:\n  v\n  w # c\nb: 1\n", 5, 10, 20)]
fn loader_multiline_plain_scalar_loc_ends_at_last_content_byte(
    #[case] input: &str,
    #[case] scalar_start: u32,
    #[case] scalar_end: u32,
    #[case] container_end: u32,
) {
    let docs = load(input).unwrap();
    let Some((child, container)) = first_child_and_container_loc(&docs[0].root) else {
        panic!("expected a non-empty collection");
    };
    let Node::Scalar { loc, .. } = child else {
        panic!("expected Scalar, got {child:?}");
    };
    assert_eq!(
        (loc.start, loc.end),
        (scalar_start, scalar_end),
        "scalar loc"
    );
    assert_eq!(container.end, container_end, "collection loc.end");
}

/// A comment line after a multi-line plain scalar belongs to the next key, not
/// to the scalar as a trailing comment.
#[test]
fn loader_multiline_plain_own_line_comment_is_leading_of_next_key() {
    let docs = load("a:\n  v\n  w\n# d\nb: 1\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    assert_eq!(entries[0].1.trailing_comment(), None);
    assert_eq!(entries[1].0.leading_comments(), ["# d"]);
}

/// Same for a sequence item: the comment line leads the next item.
#[test]
fn loader_multiline_plain_own_line_comment_is_leading_of_next_item() {
    let docs = load("- v\n  w\n# d\n- z\n").unwrap();
    let Node::Sequence { items, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    assert_eq!(items[0].trailing_comment(), None);
    assert_eq!(items[1].leading_comments(), ["# d"]);
}

/// Regression guard for the single-line path: the comment line still leads the
/// next key.
#[test]
fn loader_single_line_plain_own_line_comment_is_leading_of_next_key() {
    let docs = load("a: v\n# d\nb: 1\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    assert_eq!(entries[1].0.leading_comments(), ["# d"]);
}

/// An indented comment line after a plain scalar ends it and belongs to the
/// next key or item, exactly like one at the left margin.
#[rstest]
#[case::next_key_single_line("a: v\n  # d\nb: 1\n")]
#[case::next_key_after_blank_line("a: v\n\n  # d\nb: 1\n")]
#[case::next_key_multiline("a:\n  v\n  w\n  # d\nb: 1\n")]
fn loader_indented_comment_line_is_leading_of_next_key(#[case] input: &str) {
    let docs = load(input).unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    assert_eq!(entries[0].1.trailing_comment(), None);
    assert_eq!(entries[1].0.leading_comments(), ["# d"]);
}

#[test]
fn loader_indented_comment_line_is_leading_of_next_item() {
    let docs = load("- v\n  # d\n- z\n").unwrap();
    let Node::Sequence { items, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    assert_eq!(items[0].trailing_comment(), None);
    assert_eq!(items[1].leading_comments(), ["# d"]);
}

/// The same holds for the `loc` of a multi-line plain scalar in a flow
/// collection.
#[rstest]
#[case::sequence_item("[a\n  b\n]\n", 1, 6)]
#[case::sequence_item_three_lines("[a\n  b\n  c\n]\n", 1, 10)]
#[case::sequence_item_blank_line_between("[a\n\n  b\n]\n", 1, 7)]
#[case::sequence_item_multibyte("[é\n  ü\n]\n", 1, 8)]
fn loader_flow_multiline_plain_scalar_loc_ends_at_last_content_byte(
    #[case] input: &str,
    #[case] scalar_start: u32,
    #[case] scalar_end: u32,
) {
    let docs = load(input).unwrap();
    let Some((child, _)) = first_child_and_container_loc(&docs[0].root) else {
        panic!("expected a non-empty collection");
    };
    let Node::Scalar { loc, .. } = child else {
        panic!("expected Scalar, got {child:?}");
    };
    assert_eq!((loc.start, loc.end), (scalar_start, scalar_end));
}

/// A multi-line plain key of a flow mapping, and the entry after a multi-line
/// plain item of a flow sequence, keep their own spans.
#[test]
fn loader_flow_multiline_plain_key_and_following_entry_spans() {
    let docs = load("{a b\n  c: d}\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let (Node::Scalar { loc: key, .. }, Node::Scalar { loc: value, .. }) =
        (&entries[0].0, &entries[0].1)
    else {
        panic!("expected scalar key and value");
    };
    assert_eq!((key.start, key.end), (1, 8));
    assert_eq!((value.start, value.end), (10, 11));

    let docs = load("[a\n  b, c]\n").unwrap();
    let Node::Sequence { items, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    let Node::Scalar { loc, .. } = &items[1] else {
        panic!("expected Scalar");
    };
    assert_eq!((loc.start, loc.end), (8, 9));
}
