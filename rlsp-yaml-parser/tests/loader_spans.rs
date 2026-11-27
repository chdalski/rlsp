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
#[case::sequence_item_continued_by_an_anchor("[a\n &x b\n]\n", 1, 8)]
#[case::sequence_item_continued_by_a_quote("[a\n \"b\"\n]\n", 1, 7)]
#[case::sequence_item_continued_by_an_alias_and_space("[a\n *x ]\n", 1, 6)]
#[case::sequence_item_continued_by_an_anchor_multibyte("[é\n &x ü\n]\n", 1, 10)]
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

/// A flow mapping key that goes on with a property keeps the span of its text,
/// and the value after it its own.
#[test]
fn loader_flow_key_continued_by_an_anchor_spans() {
    let docs = load("{a\n &x b: 1}\n").unwrap();
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
}

/// YAML 1.2.2 [109] `c-double-quoted` / [120] `c-single-quoted`: the `loc` of a
/// multi-line quoted scalar ends right after its closing quote.  The
/// enclosing collection's `loc.end` is unaffected.
#[rstest]
#[case::mapping_value_double("k: \"a\n b\"\n", 3, 9, 10)]
#[case::mapping_value_single("k: 'a\n b'\n", 3, 9, 10)]
#[case::mapping_value_comment_then_entry("k: \"a\n b\" # c\nz: 1\n", 3, 9, 19)]
#[case::mapping_value_crlf("k: \"a\r\n b\"\r\nz: 1\r\n", 3, 10, 18)]
#[case::mapping_value_cr("k: \"a\r b\"\rz: 1\r", 3, 9, 15)]
#[case::mapping_value_multibyte("k: \"é\n ü\"\nz: 1\n", 3, 11, 17)]
#[case::sequence_item("- \"a\n b\"\n- z\n", 2, 8, 13)]
#[case::flow_sequence("[\"a\n b\"\n]", 1, 7, 8)]
#[case::flow_sequence_item("[\"a\n b\", c]", 1, 7, 10)]
#[case::flow_sequence_single_quoted("['a\n b', c]", 1, 7, 10)]
#[case::flow_mapping_value("{k: \"a\n b\"}", 4, 10, 10)]
fn loader_multiline_quoted_scalar_loc_ends_at_closing_quote(
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
    assert!(
        loc.end <= container.end,
        "a child's loc must lie inside its parent's"
    );
}

/// A multi-line quoted scalar as the root of a document ends at its closing
/// quote, so the next document's marker is not part of it.
#[test]
fn loader_multiline_quoted_root_loc_ends_at_closing_quote() {
    let docs = load("\"a\n b\"\n---\nx\n").unwrap();
    let Node::Scalar { loc, .. } = &docs[0].root else {
        panic!("expected Scalar");
    };
    assert_eq!((loc.start, loc.end), (0, 6));
}

/// The scalars next to a multi-line quoted scalar keep their `loc`.
#[rstest]
#[case::flow_sequence_sibling("[\"a\n b\", c]", 9, 10)]
#[case::flow_mapping_value("{\"a\n b\": 1}", 9, 10)]
fn loader_multiline_quoted_scalar_leaves_sibling_loc_unchanged(
    #[case] input: &str,
    #[case] sibling_start: u32,
    #[case] sibling_end: u32,
) {
    let docs = load(input).unwrap();
    let sibling = match &docs[0].root {
        Node::Sequence { items, .. } => &items[1],
        Node::Mapping { entries, .. } => &entries[0].1,
        other @ (Node::Scalar { .. } | Node::Alias { .. }) => {
            panic!("expected a collection, got {other:?}")
        }
    };
    let Node::Scalar { loc, .. } = sibling else {
        panic!("expected Scalar, got {sibling:?}");
    };
    assert_eq!((loc.start, loc.end), (sibling_start, sibling_end));
}

/// Apart from `loc`, the loaded nodes of a multi-line quoted scalar are those
/// of its one-line form: value, style, and the comments around it.
#[test]
fn loader_multiline_quoted_scalar_keeps_comments_and_value() {
    let docs = load("k: \"a\n b\" # c\n# d\nz: 1\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    let Node::Scalar { value, style, .. } = &entries[0].1 else {
        panic!("expected Scalar");
    };
    assert_eq!(value, "a b");
    assert_eq!(*style, rlsp_yaml_parser::ScalarStyle::DoubleQuoted);
    assert_eq!(entries[0].1.trailing_comment(), Some("# c"));
    assert_eq!(entries[1].0.leading_comments(), ["# d"]);
}

// ---------------------------------------------------------------------------
// Group: the single pair of a flow sequence
// ---------------------------------------------------------------------------

/// The `loc` of every mapping that is an item of a flow sequence, in document
/// order, with the `loc` of every node inside it checked to lie within the
/// pair's: a pair spans its key and its value, the empty ones included.
fn flow_pair_locs(node: &Node, pairs: &mut Vec<(u32, u32)>) {
    fn within(node: &Node, start: u32, end: u32) {
        let (Node::Scalar { loc, .. }
        | Node::Alias { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }) = node;
        assert!(
            start <= loc.start && loc.end <= end,
            "{start}..{end} must hold {}..{}",
            loc.start,
            loc.end
        );
        match node {
            Node::Mapping { entries, .. } => {
                for (key, value) in entries {
                    within(key, start, end);
                    within(value, start, end);
                }
            }
            Node::Sequence { items, .. } => {
                for item in items {
                    within(item, start, end);
                }
            }
            Node::Scalar { .. } | Node::Alias { .. } => {}
        }
    }
    match node {
        Node::Sequence { items, style, .. } => {
            for item in items {
                if *style == rlsp_yaml_parser::CollectionStyle::Flow
                    && let Node::Mapping { loc, .. } = item
                {
                    pairs.push((loc.start, loc.end));
                    within(item, loc.start, loc.end);
                }
                flow_pair_locs(item, pairs);
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                flow_pair_locs(key, pairs);
                flow_pair_locs(value, pairs);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// A single pair spans from its key, or its `?`, to the `,` or `]` that ends it
/// (YAML 1.2.2 [150] `ns-flow-pair`; the specification defines no spans).  The
/// pairs are listed in document order.
#[rstest]
#[case::two_pairs("[a: 1, b: 2]", &[(1, 5), (7, 11)])]
#[case::blank_before_the_comma("[a: 1 , b]", &[(1, 6)])]
#[case::anchored_key("[&x a: 1]", &[(4, 8)])]
#[case::empty_key("[: v, b]", &[(1, 4)])]
#[case::flow_sequence_key("[[x]: 1, b]", &[(1, 7)])]
#[case::double_quoted_key("[\"a\": 1]", &[(1, 7)])]
#[case::value_on_the_next_line("[a:\n  1,\n b]", &[(1, 7)])]
#[case::explicit_key_without_value("[? a, b]", &[(1, 4)])]
#[case::explicit_pair("[? a : 1, b]", &[(1, 8)])]
#[case::pair_in_a_pair("[a: [b: 1]]", &[(1, 10), (5, 9)])]
#[case::pair_in_a_block_key("[a: 1]: v\n", &[(1, 5)])]
#[case::multibyte_keys_and_values("[é: ü, 日本: 語]", &[(1, 7), (9, 20)])]
#[case::explicit_key_over_two_lines("[? a\n b : c]", &[(1, 11)])]
#[case::empty_value_before_a_comma("[a: , b]", &[(1, 4)])]
#[case::empty_value_without_a_blank("[a:, b]", &[(1, 3)])]
#[case::explicit_pair_with_an_empty_value("[? a : , b]", &[(1, 7)])]
#[case::explicit_pair_without_a_key("[? , b]", &[(1, 3)])]
#[case::properties_as_the_value("[a: &x , b]", &[(1, 7)])]
fn loader_flow_single_pair_spans_its_key_and_value(
    #[case] input: &str,
    #[case] expected: &[(u32, u32)],
) {
    let docs = load(input).unwrap();
    let mut pairs = Vec::new();
    flow_pair_locs(&docs[0].root, &mut pairs);
    assert_eq!(pairs, expected, "{input:?}");
}

/// The nodes around a single pair keep their `loc`.
#[test]
fn loader_flow_single_pair_leaves_the_other_locs_unchanged() {
    let docs = load("[a: 1, b: 2]").unwrap();
    let Node::Sequence { items, loc, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    assert_eq!((loc.start, loc.end), (0, 11));
    let locs: Vec<(u32, u32)> = items
        .iter()
        .flat_map(|item| {
            let Node::Mapping { entries, .. } = item else {
                panic!("expected Mapping, got {item:?}");
            };
            entries.iter().flat_map(|(key, value)| [key, value])
        })
        .map(|node| match node {
            Node::Scalar { loc, .. } => (loc.start, loc.end),
            other @ (Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. }) => {
                panic!("expected Scalar, got {other:?}")
            }
        })
        .collect();
    assert_eq!(locs, [(1, 2), (4, 5), (7, 8), (10, 11)]);
}

/// A flow sequence that holds a pair, inside a block collection: the block
/// mapping and the sequence keep their `loc`.
#[rstest]
#[case::block_key("[a: 1]: v\n", (0, 9), None)]
#[case::block_value("k: [a: 1, b]\n", (0, 13), Some((3, 11)))]
fn loader_flow_single_pair_leaves_the_block_locs_unchanged(
    #[case] input: &str,
    #[case] mapping: (u32, u32),
    #[case] sequence: Option<(u32, u32)>,
) {
    let docs = load(input).unwrap();
    let Node::Mapping { entries, loc, .. } = &docs[0].root else {
        panic!("expected Mapping");
    };
    assert_eq!((loc.start, loc.end), mapping);
    if let Some(expected) = sequence {
        let Node::Sequence { loc, .. } = &entries[0].1 else {
            panic!("expected Sequence");
        };
        assert_eq!((loc.start, loc.end), expected);
    }
}

/// The line index counts columns in characters at the start of a pair after
/// multibyte text.
#[test]
fn loader_flow_single_pair_start_has_a_character_column() {
    let docs = load("[é: ü, 日本: 語]").unwrap();
    let Node::Sequence { items, .. } = &docs[0].root else {
        panic!("expected Sequence");
    };
    let line_index = docs[0].line_index();
    let columns: Vec<(u32, u32)> = items
        .iter()
        .map(|item| {
            let Node::Mapping { loc, .. } = item else {
                panic!("expected Mapping, got {item:?}");
            };
            line_index.line_column(loc.start)
        })
        .collect();
    assert_eq!(columns, [(1, 1), (1, 7)]);
}
