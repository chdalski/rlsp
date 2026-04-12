use super::*;
use rstest::rstest;

// -----------------------------------------------------------------------
// Spike: verify MappingStart / MappingEnd reach parse_events
// -----------------------------------------------------------------------

#[test]
fn single_entry_mapping_through_parse_events() {
    let results: Vec<_> = parse_events("key: value\n").collect();
    let has_mapping_start = results
        .iter()
        .any(|r| matches!(r, Ok((Event::MappingStart { .. }, _))));
    let has_mapping_end = results
        .iter()
        .any(|r| matches!(r, Ok((Event::MappingEnd, _))));
    assert!(has_mapping_start, "expected MappingStart event");
    assert!(has_mapping_end, "expected MappingEnd event");
}

// -----------------------------------------------------------------------
// Group A: Flat mappings (event order)
// -----------------------------------------------------------------------

#[test]
fn single_key_value_pair_emits_correct_event_order() {
    let events = event_variants("key: value\n");
    assert!(
        matches!(events.as_slice(), [
                Event::StreamStart,
                Event::DocumentStart { explicit: false, .. },
                Event::MappingStart { anchor: None, tag: None, style: CollectionStyle::Block },
                Event::Scalar { value: k, style: ScalarStyle::Plain, .. },
                Event::Scalar { value: v, style: ScalarStyle::Plain, .. },
                Event::MappingEnd,
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ] if k.as_ref() == "key" && v.as_ref() == "value"
        ),
        "got: {events:?}"
    );
}

#[test]
fn two_entry_flat_mapping() {
    let events = event_variants("a: 1\nb: 2\n");
    assert!(
        matches!(events.as_slice(), [
                Event::StreamStart,
                Event::DocumentStart { .. },
                Event::MappingStart { .. },
                Event::Scalar { value: a, .. },
                Event::Scalar { value: one, .. },
                Event::Scalar { value: b, .. },
                Event::Scalar { value: two, .. },
                Event::MappingEnd,
                Event::DocumentEnd { .. },
                Event::StreamEnd,
            ] if a.as_ref() == "a"
              && one.as_ref() == "1"
              && b.as_ref() == "b"
              && two.as_ref() == "2"
        ),
        "got: {events:?}"
    );
}

#[test]
fn three_entry_flat_mapping_counts() {
    let events = event_variants("x: 1\ny: 2\nz: 3\n");
    let mapping_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let mapping_ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    let scalars = events
        .iter()
        .filter(|e| matches!(e, Event::Scalar { .. }))
        .count();
    assert_eq!(mapping_starts, 1, "exactly 1 MappingStart");
    assert_eq!(mapping_ends, 1, "exactly 1 MappingEnd");
    assert_eq!(scalars, 6, "exactly 6 Scalar events");
}

// -----------------------------------------------------------------------
// Group B: Empty values
// -----------------------------------------------------------------------

#[rstest]
#[case::key_colon_newline("key:\n")]
#[case::key_colon_space_newline("key: \n")]
fn mapping_missing_value_emits_empty_plain_scalar(#[case] input: &str) {
    let events = event_variants(input);
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars.len(), 2, "expected 2 scalars (key + empty value)");
    if let [key, val] = scalars.as_slice() {
        assert_eq!(key, "key");
        assert_eq!(val, "", "missing value must be empty string");
    }
}

#[test]
fn mixed_empty_and_nonempty_values() {
    let events = event_variants("a: 1\nb:\nc: 3\n");
    let values: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        values,
        vec!["a", "1", "b", "", "c", "3"],
        "scalar values in order"
    );
}

// -----------------------------------------------------------------------
// Group C: Explicit key form (`?` indicator)
// -----------------------------------------------------------------------

#[test]
fn explicit_key_simple_form() {
    let events = event_variants("? key\n: value\n");
    let has_mapping_start = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    let has_mapping_end = events.iter().any(|e| matches!(e, Event::MappingEnd));
    assert!(has_mapping_start, "expected MappingStart");
    assert!(has_mapping_end, "expected MappingEnd");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars, vec!["key", "value"], "scalars: key then value");
}

#[test]
fn explicit_key_without_value() {
    // `? key\n` with no `: value` — value should be empty scalar
    let events = event_variants("? key\n");
    let has_mapping = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(has_mapping, "expected MappingStart");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars.len(), 2, "expected key scalar + empty value scalar");
    if let [key, val] = scalars.as_slice() {
        assert_eq!(key, "key");
        assert_eq!(val, "", "missing value must be empty plain scalar");
    }
}

#[test]
fn explicit_key_complex_multiline() {
    // `? |` introduces a literal-block key
    let events = event_variants("? |\n  multiline\n  key\n: value\n");
    let has_mapping = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(has_mapping, "expected MappingStart");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars.len(), 2, "expected key scalar + value scalar");
    if let [key, val] = scalars.as_slice() {
        assert_eq!(key, "multiline\nkey\n", "literal block key content");
        assert_eq!(val, "value");
    }
}

// -----------------------------------------------------------------------
// Group D: Nested mappings
// -----------------------------------------------------------------------

#[test]
fn mapping_as_value_of_another_mapping() {
    let events = event_variants("outer:\n  inner: val\n");
    assert!(
        matches!(events.as_slice(), [
                Event::StreamStart,
                Event::DocumentStart { .. },
                Event::MappingStart { .. },
                Event::Scalar { value: outer, .. },
                Event::MappingStart { .. },
                Event::Scalar { value: inner, .. },
                Event::Scalar { value: val, .. },
                Event::MappingEnd,
                Event::MappingEnd,
                Event::DocumentEnd { .. },
                Event::StreamEnd,
            ] if outer.as_ref() == "outer"
              && inner.as_ref() == "inner"
              && val.as_ref() == "val"
        ),
        "got: {events:?}"
    );
}

#[test]
fn three_level_nested_mapping() {
    let events = event_variants("a:\n  b:\n    c: d\n");
    let mapping_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let mapping_ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    assert_eq!(mapping_starts, 3, "exactly 3 MappingStart");
    assert_eq!(mapping_ends, 3, "exactly 3 MappingEnd");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars, vec!["a", "b", "c", "d"]);
}

#[test]
fn sibling_nested_mappings() {
    let events = event_variants("x:\n  a: 1\ny:\n  b: 2\n");
    let mapping_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let mapping_ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    assert_eq!(
        mapping_starts, 3,
        "exactly 3 MappingStart (outer + 2 inner)"
    );
    assert_eq!(mapping_ends, 3, "exactly 3 MappingEnd");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars, vec!["x", "a", "1", "y", "b", "2"]);
}

// -----------------------------------------------------------------------
// Group E: Mapping termination
// -----------------------------------------------------------------------

#[test]
fn mapping_ends_on_dedent() {
    // `  key: val` (indented) followed by `baz` at col 0 — MappingEnd before Scalar("baz")
    let events = event_variants("  key: val\nbaz\n");
    let mapping_end_idx = events.iter().position(|e| matches!(e, Event::MappingEnd));
    let baz_idx = events
        .iter()
        .position(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "baz"));
    assert!(mapping_end_idx.is_some(), "expected MappingEnd");
    assert!(baz_idx.is_some(), "expected Scalar(baz)");
    if let (Some(m), Some(b)) = (mapping_end_idx, baz_idx) {
        assert!(m < b, "MappingEnd must come before Scalar(baz)");
    }
}

#[test]
fn mapping_ends_on_eof_no_trailing_newline() {
    let events = event_variants("key: val");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MappingStart { .. }))
    );
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars, vec!["key", "val"]);
    assert!(events.iter().any(|e| matches!(e, Event::MappingEnd)));
    assert!(matches!(events.last(), Some(Event::StreamEnd)));
}

#[test]
fn mapping_ends_on_explicit_document_end_marker() {
    let events = event_variants("key: val\n...\n");
    let mapping_end_idx = events.iter().position(|e| matches!(e, Event::MappingEnd));
    let doc_end_idx = events
        .iter()
        .position(|e| matches!(e, Event::DocumentEnd { explicit: true }));
    assert!(mapping_end_idx.is_some(), "expected MappingEnd");
    assert!(doc_end_idx.is_some(), "expected DocumentEnd explicit=true");
    if let (Some(m), Some(d)) = (mapping_end_idx, doc_end_idx) {
        assert!(m < d, "MappingEnd must come before DocumentEnd");
    }
}

#[test]
fn mapping_ends_on_document_start_marker() {
    let events = event_variants("key: val\n---\n");
    let mapping_end_idx = events.iter().position(|e| matches!(e, Event::MappingEnd));
    // Second DocumentStart (the one from `---`)
    let second_doc_start_idx = events
        .iter()
        .position(|e| matches!(e, Event::DocumentStart { explicit: true, .. }));
    assert!(mapping_end_idx.is_some(), "expected MappingEnd");
    assert!(
        second_doc_start_idx.is_some(),
        "expected second DocumentStart"
    );
    if let (Some(m), Some(d)) = (mapping_end_idx, second_doc_start_idx) {
        assert!(m < d, "MappingEnd must come before second DocumentStart");
    }
}

// -----------------------------------------------------------------------
// Group F: Mapping in explicit document
// -----------------------------------------------------------------------

#[test]
fn mapping_in_explicit_document() {
    let events = event_variants("---\nkey: value\n");
    assert!(
        matches!(events.as_slice(), [
                Event::StreamStart,
                Event::DocumentStart { explicit: true, .. },
                Event::MappingStart { .. },
                Event::Scalar { value: k, .. },
                Event::Scalar { value: v, .. },
                Event::MappingEnd,
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ] if k.as_ref() == "key" && v.as_ref() == "value"
        ),
        "got: {events:?}"
    );
}

// -----------------------------------------------------------------------
// Group G: Non-mapping disambiguation
// -----------------------------------------------------------------------

#[rstest]
#[case::colon_without_space_is_plain_scalar("key:value\n", "key:value")]
#[case::url_colon_slash_slash_is_plain_scalar("http://example.com\n", "http://example.com")]
fn non_mapping_colon_produces_plain_scalar_not_mapping(
    #[case] input: &str,
    #[case] expected_value: &str,
) {
    let events = event_variants(input);
    let has_mapping = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(
        !has_mapping,
        "colon pattern must not create a mapping; got: {events:?}"
    );
    let has_scalar = events
        .iter()
        .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == expected_value));
    assert!(
        has_scalar,
        "expected Scalar({expected_value}), got: {events:?}"
    );
}

#[test]
fn hash_after_space_in_key_terminates_at_comment() {
    // "key # comment: value\n" — `#` after space starts a comment (YAML 1.2
    // §6.6); the `:` inside the comment is not a value indicator.
    // The whole line is a plain scalar "key", not a mapping.
    let events = event_variants("key # comment: value\n");
    let has_mapping = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(!has_mapping, "comment must hide the colon; got: {events:?}");
    let has_scalar = events
        .iter()
        .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"));
    assert!(has_scalar, "expected Scalar(\"key\"), got: {events:?}");
}

#[test]
fn hash_immediately_after_word_is_part_of_plain_scalar() {
    // "a#b: 1\n" — no space before `#`, so `#` is plain scalar content;
    // the `: ` after it is the real value indicator.
    let events = event_variants("a#b: 1\n");
    let has_mapping = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(
        has_mapping,
        "no preceding space → `#` is not a comment → this IS a mapping; got: {events:?}"
    );
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars, vec!["a#b", "1"], "got: {events:?}");
}

// -----------------------------------------------------------------------
// Group H: Depth limit
// -----------------------------------------------------------------------

#[test]
fn pathologically_deep_mapping_returns_error_not_panic() {
    // Build MAX_COLLECTION_DEPTH + 1 levels of nested mappings.
    // Level 0: `k:\n`, Level 1: `  k:\n`, etc.
    let depth = MAX_COLLECTION_DEPTH + 1;
    let mut input = String::new();
    for i in 0..depth {
        input.push_str(&"  ".repeat(i));
        input.push_str("k:\n");
    }
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        has_error,
        "depth {depth} must produce an Err (limit is {MAX_COLLECTION_DEPTH})"
    );
}

#[test]
fn depth_limit_boundary_mapping_succeeds() {
    // Exactly MAX_COLLECTION_DEPTH levels of nested mappings — all at distinct
    // indents.  The last level has a value `v`.
    let mut input = String::new();
    for i in 0..MAX_COLLECTION_DEPTH - 1 {
        input.push_str(&"  ".repeat(i));
        input.push_str("k:\n");
    }
    input.push_str(&"  ".repeat(MAX_COLLECTION_DEPTH - 1));
    input.push_str("k: v\n");
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        !has_error,
        "exactly {MAX_COLLECTION_DEPTH} levels must succeed"
    );
}

// -----------------------------------------------------------------------
// Group I: Span correctness
// -----------------------------------------------------------------------

/// Find the first event matching the predicate and return its span.
/// Returns `None` if no matching event is found.
fn find_span<'a, F>(results: &'a [Result<(Event<'a>, Span), Error>], pred: F) -> Option<Span>
where
    F: Fn(&Event<'a>) -> bool,
{
    results.iter().find_map(|r| {
        r.as_ref()
            .ok()
            .and_then(|(ev, span)| if pred(ev) { Some(*span) } else { None })
    })
}

#[test]
fn zero_indent_mapping_key_span() {
    // "key: value\n" — k=0,e=1,y=2,:=3,' '=4,v=5,a=6,l=7,u=8,e=9,\n=10
    let results = parse_to_vec("key: value\n");
    let span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"),
    );
    assert!(span_opt.is_some(), "expected key scalar span");
    if let Some(span) = span_opt {
        assert_eq!(span.start.byte_offset, 0, "key must start at byte 0");
        assert_eq!(span.start.column, 0, "key must be at column 0");
        assert_eq!(span.end.byte_offset, 3, "key ends at byte 3 (past 'y')");
    }
}

#[test]
fn zero_indent_mapping_value_span() {
    let results = parse_to_vec("key: value\n");
    let span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "value"),
    );
    assert!(span_opt.is_some(), "expected value scalar span");
    if let Some(span) = span_opt {
        assert_eq!(span.start.byte_offset, 5, "value must start at byte 5");
        assert_eq!(span.start.column, 5, "value at column 5");
        assert_eq!(span.end.byte_offset, 10, "value ends at byte 10");
    }
}

#[test]
fn indented_mapping_key_span() {
    // "  key: value\n" — ' '=0,' '=1,k=2,e=3,y=4,:=5,' '=6,v=7...
    // This is the Task 11 bug class: dropping leading_spaces would give byte_offset=0.
    let results = parse_to_vec("  key: value\n");
    let span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"),
    );
    assert!(span_opt.is_some(), "expected key scalar span");
    if let Some(span) = span_opt {
        assert_eq!(
            span.start.byte_offset, 2,
            "key must start at byte 2 (after 2 leading spaces)"
        );
        assert_eq!(span.start.column, 2, "key at column 2");
    }
}

#[test]
fn indented_mapping_value_span() {
    let results = parse_to_vec("  key: value\n");
    let span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "value"),
    );
    assert!(span_opt.is_some(), "expected value scalar span");
    if let Some(span) = span_opt {
        assert_eq!(
            span.start.byte_offset, 7,
            "value must start at byte 7 (2 spaces + 'key' + ': ')"
        );
        assert_eq!(span.start.column, 7, "value at column 7");
    }
}

#[test]
fn mapping_start_span_points_at_first_key() {
    let results = parse_to_vec("key: value\n");
    let span_opt = find_span(&results, |e| matches!(e, Event::MappingStart { .. }));
    assert!(span_opt.is_some(), "expected MappingStart span");
    if let Some(span) = span_opt {
        assert_eq!(
            span.start.byte_offset, 0,
            "MappingStart span must point at the first key (byte 0)"
        );
        assert_eq!(span.start.column, 0);
    }
}

#[test]
fn indented_mapping_start_span_points_at_first_key() {
    let results = parse_to_vec("  key: value\n");
    let span_opt = find_span(&results, |e| matches!(e, Event::MappingStart { .. }));
    assert!(span_opt.is_some(), "expected MappingStart span");
    if let Some(span) = span_opt {
        assert_eq!(
            span.start.byte_offset, 2,
            "MappingStart must point at byte 2"
        );
        assert_eq!(span.start.column, 2);
    }
}

#[test]
fn mapping_inside_sequence_item_key_span() {
    // "- key: value\n" — '-'=0,' '=1,k=2,e=3,y=4,':'=5,' '=6,v=7...
    let results = parse_to_vec("- key: value\n");
    let key_span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"),
    );
    assert!(key_span_opt.is_some(), "expected key scalar span");
    if let Some(key_span) = key_span_opt {
        assert_eq!(key_span.start.byte_offset, 2, "key at byte 2");
        assert_eq!(key_span.start.column, 2, "key at column 2");
    }
    let val_span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "value"),
    );
    assert!(val_span_opt.is_some(), "expected value scalar span");
    if let Some(val_span) = val_span_opt {
        assert_eq!(val_span.start.byte_offset, 7, "value at byte 7");
        assert_eq!(val_span.start.column, 7, "value at column 7");
    }
}

#[test]
fn mapping_inside_sequence_item_mapping_start_span() {
    let results = parse_to_vec("- key: value\n");
    let span_opt = find_span(&results, |e| matches!(e, Event::MappingStart { .. }));
    assert!(span_opt.is_some(), "expected MappingStart span");
    if let Some(span) = span_opt {
        assert_eq!(
            span.start.byte_offset, 2,
            "MappingStart inside seq item must point at byte 2"
        );
        assert_eq!(span.start.column, 2);
    }
}

#[test]
fn nested_mapping_value_span() {
    // "outer:\n  inner: val\n"
    // outer=0..5, :=5, \n=6 → line 2 starts at byte 7:
    // ' '=7,' '=8,i=9,n=10,n=11,e=12,r=13,:=14,' '=15,v=16,a=17,l=18,\n=19
    let results = parse_to_vec("outer:\n  inner: val\n");
    let inner_span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "inner"),
    );
    assert!(inner_span_opt.is_some(), "expected inner scalar span");
    if let Some(inner_span) = inner_span_opt {
        assert_eq!(inner_span.start.byte_offset, 9, "inner at byte 9");
        assert_eq!(inner_span.start.column, 2, "inner at column 2");
    }
    let val_span_opt = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "val"),
    );
    assert!(val_span_opt.is_some(), "expected val scalar span");
    if let Some(val_span) = val_span_opt {
        assert_eq!(val_span.start.byte_offset, 16, "val at byte 16");
        assert_eq!(val_span.start.column, 9, "val at column 9");
    }
}

#[test]
fn empty_value_span_is_zero_width() {
    // "key:\n" — k=0,e=1,y=2,:=3,\n=4
    // The empty value span must be zero-width (start == end).
    let results = parse_to_vec("key:\n");
    // The second Scalar is the empty value.
    let empty_spans: Vec<_> = results
        .iter()
        .filter_map(|r| {
            r.as_ref().ok().and_then(|(ev, span)| {
                if matches!(ev, Event::Scalar { value, .. } if value.as_ref().is_empty()) {
                    Some(*span)
                } else {
                    None
                }
            })
        })
        .collect();
    assert!(
        !empty_spans.is_empty(),
        "expected at least one empty scalar"
    );
    if let Some(&span) = empty_spans.first() {
        assert_eq!(
            span.start.byte_offset, span.end.byte_offset,
            "empty value span must be zero-width"
        );
    }
}
