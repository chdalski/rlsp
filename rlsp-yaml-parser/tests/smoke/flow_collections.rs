use super::*;
use rstest::rstest;

// -----------------------------------------------------------------------
// Module-local event constructors (reduce repetition in exact-event assertions)
// -----------------------------------------------------------------------

const fn seq_start_flow() -> Event<'static> {
    Event::SequenceStart {
        anchor: None,
        tag: None,
        style: CollectionStyle::Flow,
    }
}

const fn map_start_flow() -> Event<'static> {
    Event::MappingStart {
        anchor: None,
        tag: None,
        style: CollectionStyle::Flow,
    }
}

const fn plain(v: &'static str) -> Event<'static> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(v),
        style: ScalarStyle::Plain,
        anchor: None,
        tag: None,
    }
}

const fn single_quoted(v: &'static str) -> Event<'static> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(v),
        style: ScalarStyle::SingleQuoted,
        anchor: None,
        tag: None,
    }
}

const fn double_quoted(v: &'static str) -> Event<'static> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(v),
        style: ScalarStyle::DoubleQuoted,
        anchor: None,
        tag: None,
    }
}

// -----------------------------------------------------------------------
// Group A: Spike — validates the test harness reaches the flow parser
// -----------------------------------------------------------------------

#[test]
fn spike_flow_sequence_through_parse_events() {
    // "[a]\n" must produce: StreamStart, DocumentStart, SequenceStart(Flow),
    // Scalar("a"), SequenceEnd, DocumentEnd, StreamEnd.
    let events = evs("[a]\n");
    assert!(
        matches!(events.first(), Some(Event::StreamStart)),
        "first event is StreamStart"
    );
    assert!(
        matches!(events.last(), Some(Event::StreamEnd)),
        "last event is StreamEnd"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "must contain a Flow SequenceStart"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::SequenceEnd)),
        "must contain a SequenceEnd"
    );
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a"], "one scalar 'a'");
}

// -----------------------------------------------------------------------
// Group B: Empty flow collections
// -----------------------------------------------------------------------

#[test]
fn empty_flow_sequence_emits_start_end_pair() {
    // Full event sequence: StreamStart, DocumentStart, SequenceStart(Flow),
    // SequenceEnd, DocumentEnd, StreamEnd.
    let events = evs("[]\n");
    // Find a window where SequenceStart(Flow) is immediately followed by SequenceEnd.
    let pair = events
        .windows(2)
        .find(|w| matches!(w, [a, _] if *a == seq_start_flow()));
    assert!(
        matches!(pair, Some([_, Event::SequenceEnd])),
        "SequenceStart(Flow) immediately followed by SequenceEnd; events: {events:?}"
    );
    assert_eq!(
        scalar_values(&events).len(),
        0,
        "no scalars in empty sequence"
    );
}

#[test]
fn empty_flow_mapping_emits_start_end_pair() {
    // Full event sequence: StreamStart, DocumentStart, MappingStart(Flow),
    // MappingEnd, DocumentEnd, StreamEnd.
    let events = evs("{}\n");
    let pair = events
        .windows(2)
        .find(|w| matches!(w, [a, _] if *a == map_start_flow()));
    assert!(
        matches!(pair, Some([_, Event::MappingEnd])),
        "MappingStart(Flow) immediately followed by MappingEnd; events: {events:?}"
    );
    assert_eq!(
        scalar_values(&events).len(),
        0,
        "no scalars in empty mapping"
    );
}

// -----------------------------------------------------------------------
// Group C: Single-item flow sequences and mappings
// -----------------------------------------------------------------------

#[test]
fn single_item_flow_sequence() {
    // SequenceStart(Flow) immediately followed by plain("hello"), then SequenceEnd.
    let events = evs("[hello]\n");
    let triple = events
        .windows(3)
        .find(|w| matches!(w, [a, b, _] if *a == seq_start_flow() && *b == plain("hello")));
    assert!(
        matches!(triple, Some([_, _, Event::SequenceEnd])),
        "SequenceStart(Flow), plain(hello), SequenceEnd in sequence; events: {events:?}"
    );
}

#[test]
fn single_pair_flow_mapping() {
    // MappingStart(Flow), plain("a"), plain("b"), MappingEnd.
    let events = evs("{a: b}\n");
    let quad = events.windows(4).find(|w| {
        matches!(w, [a, b, c, _] if *a == map_start_flow() && *b == plain("a") && *c == plain("b"))
    });
    assert!(
        matches!(quad, Some([_, _, _, Event::MappingEnd])),
        "MappingStart(Flow), plain(a), plain(b), MappingEnd in sequence; events: {events:?}"
    );
}

// -----------------------------------------------------------------------
// Group D: Multi-item flow collections
// -----------------------------------------------------------------------

#[rstest]
#[case::multi_item_flow_sequence("[a, b, c]\n", vec!["a", "b", "c"])]
#[case::multi_pair_flow_mapping("{a: 1, b: 2}\n", vec!["a", "1", "b", "2"])]
#[case::trailing_comma_in_flow_sequence("[a, b,]\n", vec!["a", "b"])]
#[case::trailing_comma_in_flow_mapping("{a: b,}\n", vec!["a", "b"])]
fn flow_collection_scalar_values_are_correct(#[case] input: &str, #[case] expected: Vec<&str>) {
    let events = evs(input);
    let scalars = scalar_values(&events);
    assert_eq!(scalars, expected);
}

// -----------------------------------------------------------------------
// Group E: Trailing commas allowed
// (covered by flow_collection_scalar_values_are_correct above)
// -----------------------------------------------------------------------

// -----------------------------------------------------------------------
// Group F: Quoted scalars inside flow collections
// -----------------------------------------------------------------------

#[test]
fn single_quoted_scalar_in_flow_sequence() {
    let events = evs("['hello world']\n");
    let triple = events.windows(3).find(
        |w| matches!(w, [a, b, _] if *a == seq_start_flow() && *b == single_quoted("hello world")),
    );
    assert!(
        matches!(triple, Some([_, _, Event::SequenceEnd])),
        "SequenceStart(Flow), single_quoted(hello world), SequenceEnd; events: {events:?}"
    );
}

#[test]
fn double_quoted_scalar_in_flow_sequence() {
    let events = evs("[\"hello\"]\n");
    let triple = events
        .windows(3)
        .find(|w| matches!(w, [a, b, _] if *a == seq_start_flow() && *b == double_quoted("hello")));
    assert!(
        matches!(triple, Some([_, _, Event::SequenceEnd])),
        "SequenceStart(Flow), double_quoted(hello), SequenceEnd; events: {events:?}"
    );
}

#[test]
fn single_quoted_key_in_flow_mapping() {
    let events = evs("{'key': value}\n");
    let triple = events.windows(3).find(|w| {
        matches!(w, [a, b, c] if *a == map_start_flow() && *b == single_quoted("key") && *c == plain("value"))
    });
    assert!(
        triple.is_some(),
        "MappingStart(Flow), single_quoted(key), plain(value) in sequence; events: {events:?}"
    );
}

// -----------------------------------------------------------------------
// Group G: Plain scalar rules in flow context
// -----------------------------------------------------------------------

#[test]
fn flow_indicator_terminates_plain_scalar() {
    // Plain scalar `ab` is terminated by `]`.
    let events = evs("[ab]\n");
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["ab"], "flow indicator terminates scalar");
}

#[test]
fn comma_terminates_plain_scalar_in_flow() {
    // In `[a,b]` the comma terminates `a`; `b` is a separate scalar.
    let events = evs("[a,b]\n");
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a", "b"]);
}

#[test]
fn colon_space_terminates_plain_scalar_in_flow_key() {
    // `{a: b}` — `a` is a key terminated by `: `.
    let events = evs("{a: b}\n");
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a", "b"]);
}

// -----------------------------------------------------------------------
// Group H: Block scalar indicators forbidden in flow context
// -----------------------------------------------------------------------

#[rstest]
#[case::literal_block_scalar_in_flow("[|]\n")]
#[case::folded_block_scalar_in_flow("[>]\n")]
fn block_scalar_indicator_in_flow_returns_error(#[case] input: &str) {
    let result: Vec<_> = parse_events(input).collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "block scalar indicator inside flow must return an error"
    );
}

// -----------------------------------------------------------------------
// Group I: Nested flow collections
// -----------------------------------------------------------------------

#[test]
fn nested_flow_sequence_inside_sequence() {
    // `[[a, b], c]` — inner sequence then scalar.
    let events = evs("[[a, b], c]\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::SequenceStart { .. })),
        2,
        "two SequenceStarts: outer and inner"
    );
    assert_eq!(count(&events, |e| matches!(e, Event::SequenceEnd)), 2);
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a", "b", "c"]);
}

#[test]
fn nested_flow_mapping_inside_sequence() {
    // `[{a: b}]` — mapping as a sequence item.
    let events = evs("[{a: b}]\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::SequenceStart { .. })),
        1
    );
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        1
    );
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a", "b"]);
}

#[test]
fn nested_flow_sequence_inside_mapping_value() {
    // `{key: [a, b]}` — sequence as a mapping value.
    let events = evs("{key: [a, b]}\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        1
    );
    assert_eq!(
        count(&events, |e| matches!(e, Event::SequenceStart { .. })),
        1
    );
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["key", "a", "b"]);
}

// -----------------------------------------------------------------------
// Group J: Multi-line flow collections
// -----------------------------------------------------------------------

#[test]
fn multiline_flow_sequence() {
    // Flow collection spanning multiple lines.
    let input = "[\n  a,\n  b\n]\n";
    let events = evs(input);
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a", "b"]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "SequenceStart(Flow) present"
    );
}

#[test]
fn multiline_flow_mapping() {
    let input = "{\n  a: 1,\n  b: 2\n}\n";
    let events = evs(input);
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a", "1", "b", "2"]);
}

// -----------------------------------------------------------------------
// Group K: Flow collection as block mapping value (inline flow)
// -----------------------------------------------------------------------

#[test]
fn flow_sequence_as_block_mapping_value() {
    // "key: [a, b]\n"
    let events = evs("key: [a, b]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        "outer block mapping"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "inner flow sequence"
    );
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["key", "a", "b"]);
}

#[test]
fn flow_mapping_as_block_mapping_value() {
    // "key: {a: b}\n"
    let events = evs("key: {a: b}\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        "outer block mapping"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "inner flow mapping"
    );
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["key", "a", "b"]);
}

// -----------------------------------------------------------------------
// Group L: CollectionStyle::Flow is emitted (not Block)
// -----------------------------------------------------------------------

#[test]
fn flow_sequence_style_is_flow_not_block() {
    let events = evs("[a]\n");
    let seq_start = events
        .iter()
        .find(|e| matches!(e, Event::SequenceStart { .. }));
    assert!(
        matches!(
            seq_start,
            Some(Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            })
        ),
        "SequenceStart style must be Flow, not Block"
    );
}

#[test]
fn flow_mapping_style_is_flow_not_block() {
    let events = evs("{a: b}\n");
    let map_start = events
        .iter()
        .find(|e| matches!(e, Event::MappingStart { .. }));
    assert!(
        matches!(
            map_start,
            Some(Event::MappingStart {
                style: CollectionStyle::Flow,
                ..
            })
        ),
        "MappingStart style must be Flow, not Block"
    );
}

// -----------------------------------------------------------------------
// Group M: Security — depth limit and unterminated input
// -----------------------------------------------------------------------

#[test]
fn unterminated_flow_sequence_returns_error() {
    let result: Vec<_> = parse_events("[a, b\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(has_error, "unterminated flow sequence must return an error");
}

#[test]
fn unterminated_flow_mapping_returns_error() {
    let result: Vec<_> = parse_events("{a: b\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(has_error, "unterminated flow mapping must return an error");
}

#[test]
fn mismatched_close_delimiter_returns_error() {
    let result: Vec<_> = parse_events("[a}").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "mismatched closing delimiter must return an error"
    );
}

#[test]
fn flow_depth_limit_is_enforced() {
    // Build a deeply nested flow sequence that exceeds MAX_COLLECTION_DEPTH.
    // MAX_COLLECTION_DEPTH is 512; build 513 levels of `[` without closing.
    let depth = MAX_COLLECTION_DEPTH + 1;
    let input = "[".repeat(depth) + &"]".repeat(depth) + "\n";
    let result: Vec<_> = parse_events(&input).collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "depth exceeding MAX_COLLECTION_DEPTH must return error"
    );
}

#[test]
fn mixed_block_and_flow_depth_limit_is_enforced() {
    // Half the limit in block nesting, just over half in flow nesting —
    // combined depth exceeds MAX_COLLECTION_DEPTH.
    let block_depth = MAX_COLLECTION_DEPTH / 2;
    let flow_depth = MAX_COLLECTION_DEPTH / 2 + 1;
    // Build `block_depth` indented block-sequence lines, then append the
    // flow collection on the last line.  Constructing the input as a
    // String avoids indexing into the Vec.
    let block_lines: String = (0..block_depth).fold(String::new(), |mut s, i| {
        use std::fmt::Write as _;
        let _ = writeln!(s, "{}- ", "  ".repeat(i));
        s
    });
    // Strip the trailing newline of the last line so we can append inline.
    let block_prefix = block_lines.trim_end_matches('\n');
    let flow_part = "[".repeat(flow_depth) + &"]".repeat(flow_depth);
    let input = format!("{block_prefix}{flow_part}\n");
    let result: Vec<_> = parse_events(&input).collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "combined block+flow depth exceeding MAX_COLLECTION_DEPTH must return error"
    );
}

#[test]
fn iterator_returns_none_after_flow_error() {
    // After an error, the iterator must stop (return None), not loop forever.
    let mut iter = parse_events("[|\n");
    // Consume until error or end.
    let mut found_error = false;
    let mut count = 0usize;
    for item in &mut iter {
        count += 1;
        if item.is_err() {
            found_error = true;
            break;
        }
        // Safety: prevent infinite loops in test.
        if count > 100 {
            break;
        }
    }
    assert!(found_error, "must have produced an error");
    // After error, iterator must yield None.
    assert!(iter.next().is_none(), "iterator must stop after error");
}

#[test]
fn consecutive_commas_in_flow_sequence_returns_error() {
    let result: Vec<_> = parse_events("[a,,b]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(has_error, "consecutive commas must return an error");
}

#[test]
fn leading_comma_in_flow_sequence_is_an_error() {
    let result: Vec<_> = parse_events("[,]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(has_error, "leading comma must return an error");
    // Error-recovery invariant: no Ok item may appear after the first Err.
    let mut saw_err = false;
    for item in &result {
        if saw_err {
            assert!(
                item.is_err(),
                "Ok item appeared after Err — iterator must stop after first error"
            );
        }
        if item.is_err() {
            saw_err = true;
        }
    }
}

// -----------------------------------------------------------------------
// Group B (extra): Whitespace inside flow collections is stripped
// -----------------------------------------------------------------------

#[test]
fn single_item_flow_sequence_leading_trailing_spaces() {
    // Interior whitespace around the scalar is stripped; value is "a" not " a ".
    let events = evs("[ a ]\n");
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a"], "interior whitespace must be stripped");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "SequenceStart(Flow) present"
    );
}

// -----------------------------------------------------------------------
// Group N: Span correctness
// -----------------------------------------------------------------------

#[test]
fn flow_sequence_start_span_anchors_at_opening_bracket() {
    // "[a]\n" — SequenceStart span.start must be at byte 0, column 0.
    let items = parse_to_vec("[a]\n");
    let seq_span = items.iter().find_map(|r| match r {
        Ok((Event::SequenceStart { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(seq_span.is_some(), "SequenceStart event must be present");
    if let Some(span) = seq_span {
        assert_eq!(
            span.start.byte_offset, 0,
            "SequenceStart byte_offset must be 0"
        );
        assert_eq!(span.start.column, 0, "SequenceStart column must be 0");
    }
}

#[test]
fn flow_sequence_end_span_anchors_at_closing_bracket() {
    // "[a]\n" — SequenceEnd span.start must be at byte 2 (the `]`).
    let items = parse_to_vec("[a]\n");
    let end_span = items.iter().find_map(|r| match r {
        Ok((Event::SequenceEnd, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(end_span.is_some(), "SequenceEnd event must be present");
    if let Some(span) = end_span {
        assert_eq!(
            span.start.byte_offset, 2,
            "SequenceEnd byte_offset must be 2 (position of `]`)"
        );
    }
}

#[test]
fn flow_mapping_start_span_anchors_at_opening_brace() {
    // "{a: b}\n" — MappingStart span.start at byte 0, column 0.
    let items = parse_to_vec("{a: b}\n");
    let map_span = items.iter().find_map(|r| match r {
        Ok((Event::MappingStart { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(map_span.is_some(), "MappingStart event must be present");
    if let Some(span) = map_span {
        assert_eq!(
            span.start.byte_offset, 0,
            "MappingStart byte_offset must be 0"
        );
        assert_eq!(span.start.column, 0, "MappingStart column must be 0");
    }
}

#[test]
fn flow_mapping_end_span_anchors_at_closing_brace() {
    // "{a: b}\n" — MappingEnd span.start at byte 5 (the `}`).
    let items = parse_to_vec("{a: b}\n");
    let end_span = items.iter().find_map(|r| match r {
        Ok((Event::MappingEnd, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(end_span.is_some(), "MappingEnd event must be present");
    if let Some(span) = end_span {
        assert_eq!(
            span.start.byte_offset, 5,
            "MappingEnd byte_offset must be 5 (position of `}}`)"
        );
    }
}

#[test]
fn scalar_span_inside_flow_sequence_is_correct() {
    // "[ab]\n" — Scalar "ab" starts at byte 1, ends at byte 3.
    let items = parse_to_vec("[ab]\n");
    let scalar_span = items.iter().find_map(|r| match r {
        Ok((Event::Scalar { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(scalar_span.is_some(), "Scalar event must be present");
    if let Some(span) = scalar_span {
        assert_eq!(
            span.start.byte_offset, 1,
            "scalar start byte_offset must be 1"
        );
        assert_eq!(span.end.byte_offset, 3, "scalar end byte_offset must be 3");
    }
}

// -----------------------------------------------------------------------
// Group J: Explicit key indicator `?` inside flow mappings
// -----------------------------------------------------------------------

#[test]
fn explicit_key_in_flow_mapping() {
    // `{? key: value}` — explicit key indicator inside a flow mapping.
    let events = evs("{? key: value}\n");
    let scalars = scalar_values(&events);
    // The `?` is consumed as an explicit-key indicator; key and value scalars follow.
    assert!(
        scalars.contains(&"key"),
        "key scalar must be present; got {scalars:?}"
    );
    assert!(
        scalars.contains(&"value"),
        "value scalar must be present; got {scalars:?}"
    );
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        1
    );
    assert_eq!(count(&events, |e| matches!(e, Event::MappingEnd)), 1);
}

// -----------------------------------------------------------------------
// Group K: Flow collection as block sequence item
// -----------------------------------------------------------------------

#[test]
fn flow_sequence_as_block_sequence_item() {
    // `"- [a, b]\n"` — flow sequence is an item in a block sequence.
    let events = evs("- [a, b]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        "outer block sequence present"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "inner flow sequence present"
    );
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["a", "b"]);
}

// -----------------------------------------------------------------------
// Group O: Anchors and aliases inside flow collections (Task 16)
// -----------------------------------------------------------------------

#[test]
fn anchor_in_flow_sequence_emits_scalar_with_anchor() {
    // `[&x foo]\n` — anchor on plain scalar inside flow sequence.
    let events = evs("[&x foo]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                anchor: Some("x"),
                value,
                ..
            } if value.as_ref() == "foo"
        )),
        "anchor `&x` must be attached to the scalar 'foo' inside the flow sequence"
    );
}

#[test]
fn alias_in_flow_sequence_emits_alias_event() {
    // `[*x]\n` — alias inside flow sequence.
    let events = evs("[*x]\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "x" })),
        "alias `*x` must emit an Alias event inside flow sequence"
    );
}

#[test]
fn tag_indicator_in_flow_sequence_is_parsed() {
    // `[!t x]\n` — Task 17 implements tag parsing in flow context.
    // `!t` is a secondary-handle tag; `x` is the scalar value.
    let events = evs("[!t x]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                ..
            } if t.as_ref() == "!t"
        )),
        "tag `!t` inside flow sequence must produce a tagged scalar event"
    );
}

#[test]
fn control_character_in_flow_collection_returns_error() {
    // `[\x01]\n` — C0 control characters are not valid `ns-char`s; the
    // parser must return an error rather than panicking (the previous
    // `unreachable!` fallback would crash on these).
    let result: Vec<_> = parse_events("[\x01]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "control character inside flow collection must return an error"
    );
    // Error-recovery invariant: no Ok item after the first Err.
    let mut saw_err = false;
    for item in &result {
        if saw_err {
            assert!(
                item.is_err(),
                "Ok item appeared after Err — iterator must stop after first error"
            );
        }
        if item.is_err() {
            saw_err = true;
        }
    }
}

// -----------------------------------------------------------------------
// Group P: Multi-line flow collection span correctness
// -----------------------------------------------------------------------

#[test]
fn scalar_on_continuation_line_has_correct_span() {
    // "[\n  a,\n  b\n]\n" — scalar `b` is on line 3 (1-based), column 2,
    // byte_offset 9.
    //
    // Byte layout:
    //   0: `[`
    //   1: `\n`
    //   2-3: `  `  (indent)
    //   4: `a`
    //   5: `,`
    //   6: `\n`
    //   7-8: `  `  (indent)
    //   9: `b`      ← this scalar's start
    //  10: `\n`
    //  11: `]`
    //  12: `\n`
    let items = parse_to_vec("[\n  a,\n  b\n]\n");
    // Find the *second* Scalar — `b` (index 1 in scalar events).
    let scalar_spans: Vec<_> = items
        .iter()
        .filter_map(|r| match r {
            Ok((Event::Scalar { .. }, span)) => Some(*span),
            Ok(_) | Err(_) => None,
        })
        .collect();
    assert_eq!(scalar_spans.len(), 2, "expected exactly 2 scalar events");
    if let [_a_span, b_span] = scalar_spans.as_slice() {
        assert_eq!(
            b_span.start.byte_offset, 9,
            "scalar `b` must start at byte_offset 9"
        );
        assert_eq!(
            b_span.start.line, 3,
            "scalar `b` must be on line 3 (1-based)"
        );
        assert_eq!(
            b_span.start.column, 2,
            "scalar `b` must be at column 2 (0-based)"
        );
    } else {
        unreachable!("expected exactly 2 scalar spans");
    }
}
