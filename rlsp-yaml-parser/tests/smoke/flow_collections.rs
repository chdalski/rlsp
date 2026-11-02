use super::*;
use rstest::rstest;

// -----------------------------------------------------------------------
// Module-local event constructors (reduce repetition in exact-event assertions)
// -----------------------------------------------------------------------

const fn seq_start_flow() -> Event<'static> {
    Event::SequenceStart {
        meta: None,
        style: CollectionStyle::Flow,
    }
}

const fn map_start_flow() -> Event<'static> {
    Event::MappingStart {
        meta: None,
        style: CollectionStyle::Flow,
    }
}

const fn plain(v: &'static str) -> Event<'static> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(v),
        style: ScalarStyle::Plain,
        meta: None,
    }
}

const fn single_quoted(v: &'static str) -> Event<'static> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(v),
        style: ScalarStyle::SingleQuoted,
        meta: None,
    }
}

const fn double_quoted(v: &'static str) -> Event<'static> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(v),
        style: ScalarStyle::DoubleQuoted,
        meta: None,
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
    let input = "[a]\n";
    let items = parse_to_vec(input);
    let seq_span = items.iter().find_map(|r| match r {
        Ok((Event::SequenceStart { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(seq_span.is_some(), "SequenceStart event must be present");
    if let Some(span) = seq_span {
        assert_eq!(span.start, 0, "SequenceStart byte_offset must be 0");
        assert_eq!(
            line_col(input, span.start).1,
            0,
            "SequenceStart column must be 0"
        );
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
            span.start, 2,
            "SequenceEnd byte_offset must be 2 (position of `]`)"
        );
    }
}

#[test]
fn flow_mapping_start_span_anchors_at_opening_brace() {
    // "{a: b}\n" — MappingStart span.start at byte 0, column 0.
    let input = "{a: b}\n";
    let items = parse_to_vec(input);
    let map_span = items.iter().find_map(|r| match r {
        Ok((Event::MappingStart { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(map_span.is_some(), "MappingStart event must be present");
    if let Some(span) = map_span {
        assert_eq!(span.start, 0, "MappingStart byte_offset must be 0");
        assert_eq!(
            line_col(input, span.start).1,
            0,
            "MappingStart column must be 0"
        );
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
            span.start, 5,
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
        assert_eq!(span.start, 1, "scalar start byte_offset must be 1");
        assert_eq!(span.end, 3, "scalar end byte_offset must be 3");
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
        events.iter().any(|e| {
            matches!(e, Event::Scalar { value, .. } if value.as_ref() == "foo")
                && e.anchor() == Some("x")
        }),
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
        events
            .iter()
            .any(|e| { matches!(e, Event::Scalar { .. }) && e.tag() == Some("!t") }),
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
// Group Q: Coverage gap tests — error paths and edge cases
// -----------------------------------------------------------------------

// Test 48: tab indentation in flow continuation
#[test]
fn tab_indent_in_flow_continuation_returns_error() {
    // `[\n\ta\n]\n` — tab at start of non-blank continuation line inside flow
    // collection: YAML 1.2 §6.2 — indentation uses spaces only.
    let result: Vec<_> = parse_events("[\n\ta\n]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "tab as indentation in flow continuation must return an error"
    );
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message.contains("tab") || err.message.contains("indentation"),
            "error message should mention tab or indentation; got: {}",
            err.message
        );
    }
}

// Test 49: flow continuation line not indented enough when inside block context
#[test]
fn flow_continuation_not_indented_enough_returns_error() {
    // Flow sequence as value of a block mapping key; continuation line at column 0
    // regresses to the block indent level, which is forbidden.
    // "key:\n  [\n  a,\nb\n  ]\n" — `b` is at column 0, below the block indent of 2.
    let input = "key:\n  [\n  a,\nb\n  ]\n";
    let result: Vec<_> = parse_events(input).collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "flow continuation line at block indent level must return an error"
    );
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message.contains("not indented") || err.message.contains("continuation"),
            "error message should mention indent or continuation; got: {}",
            err.message
        );
    }
}

// Test 50: `]` used to close a flow mapping
#[test]
fn closing_bracket_on_flow_mapping_returns_error() {
    let result: Vec<_> = parse_events("{a: b]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(has_error, "']' closing a flow mapping must return an error");
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message.contains("'}'"),
            "error message should mention expected '}}'; got: {}",
            err.message
        );
    }
}

// Test 51: `}` used to close a flow sequence
#[test]
fn closing_brace_on_flow_sequence_returns_error() {
    let result: Vec<_> = parse_events("[a, b}\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "'}}' closing a flow sequence must return an error"
    );
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message.contains("']'"),
            "error message should mention expected ']'; got: {}",
            err.message
        );
    }
}

// Test 52: `#` immediately after closing bracket without preceding space
#[test]
fn hash_immediately_after_close_bracket_returns_error() {
    // `[a]#comment` — no space between `]` and `#`; not a valid comment.
    let result: Vec<_> = parse_events("[a]#comment\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "'#' immediately after ']' without space must return an error"
    );
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message.contains("space") && err.message.contains('#'),
            "error message should mention space and '#'; got: {}",
            err.message
        );
    }
}

// Test 53: comment on its own line after flow collection is valid
#[test]
fn comment_on_own_line_after_flow_collection_is_valid() {
    // `[a]\n# comment\n` — the comment on a separate line after the flow
    // collection closes is processed by the block-context comment handler,
    // not the flow-collection hash check.
    let events = evs("[a]\n# a comment\n");
    let scalars = scalar_values(&events);
    assert_eq!(
        scalars,
        ["a"],
        "scalar must be 'a'; comment on next line must not affect it"
    );
}

// Test 54: block sequence entry `- ` (dash-space) inside flow sequence
#[test]
fn block_sequence_entry_dash_space_in_flow_sequence_returns_error() {
    let result: Vec<_> = parse_events("[- a]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "block sequence entry '-' followed by space inside flow must return an error"
    );
}

// Test 55: block sequence entry `- ` (dash-tab) inside flow sequence
#[test]
fn block_sequence_entry_dash_tab_in_flow_sequence_returns_error() {
    let result: Vec<_> = parse_events("[-\ta]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "block sequence entry '-' followed by tab inside flow must return an error"
    );
}

// Test 56: `-` at end of content (none after) inside flow sequence
#[test]
fn dash_at_end_of_flow_line_returns_error() {
    // `[- ]` — dash followed by space, no value: treated as block sequence entry.
    let result: Vec<_> = parse_events("[- ]\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "bare '-' followed by space inside flow must return an error"
    );
}

// Test 57: `-` followed by digit is a valid plain scalar in flow context
#[test]
fn dash_followed_by_non_separator_is_plain_scalar() {
    // `-1` is a legal plain scalar in flow context.
    let events = evs("[-1]\n");
    let scalars = scalar_values(&events);
    assert_eq!(scalars, ["-1"], "'-1' is a plain scalar in flow context");
}

// Test 58: anchor before comma in mapping value position emits empty scalar with anchor
#[test]
fn anchor_before_comma_emits_empty_scalar_with_anchor() {
    // `{a: &x , b: c}` — after emitting key `a`, parser is in Value phase
    // (has_value=true).  `&x` sets pending_anchor.  When `,` appears with
    // pending_anchor but no value scalar yet, the parser emits an empty scalar
    // carrying the anchor, then resets the frame.
    let events = evs("{a: &x , b: c}\n");
    let has_anchored_empty = events.iter().any(|e| {
        matches!(e, Event::Scalar { value, .. } if value.as_ref().is_empty())
            && e.anchor() == Some("x")
    });
    assert!(
        has_anchored_empty,
        "anchor before comma in value position must produce an empty scalar with anchor 'x'; events: {events:?}"
    );
    let scalars = scalar_values(&events);
    assert!(scalars.contains(&"b"), "scalar 'b' must be present");
    assert!(scalars.contains(&"c"), "scalar 'c' must be present");
}

// Test 59: tag before comma in mapping value position emits empty scalar with tag
#[test]
fn tag_before_comma_emits_empty_scalar_with_tag() {
    // `{a: !!str , b: c}` — after emitting key `a`, parser is in Value phase
    // (has_value=true).  `!!str` sets pending_tag.  When `,` appears with
    // pending_tag but no value scalar yet, the parser emits an empty scalar
    // carrying the tag.
    let events = evs("{a: !!str , b: c}\n");
    let has_tagged_empty = events.iter().any(|e| {
        matches!(e, Event::Scalar { value, .. } if value.as_ref().is_empty()) && e.tag().is_some()
    });
    assert!(
        has_tagged_empty,
        "tag before comma in value position must produce an empty scalar with a tag; events: {events:?}"
    );
    let scalars = scalar_values(&events);
    assert!(scalars.contains(&"b"), "scalar 'b' must be present");
    assert!(scalars.contains(&"c"), "scalar 'c' must be present");
}

// Test 60: flow collection spanning multiple lines used as implicit mapping key
#[test]
fn multiline_flow_collection_as_implicit_key_returns_error() {
    // `[\na\n]: value` — flow sequence closes on a different line than it opened,
    // then is used as an implicit block-mapping key (followed by `:`). Illegal.
    let input = "[\na\n]: value\n";
    let result: Vec<_> = parse_events(input).collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "multiline flow collection used as implicit mapping key must return an error"
    );
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message.contains("multi-line") || err.message.contains("implicit mapping key"),
            "error message should mention multi-line or implicit mapping key; got: {}",
            err.message
        );
    }
}

// Test 61: explicit key `?` in flow sequence suppresses multiline key error
#[test]
fn explicit_key_in_flow_sequence_suppresses_multiline_key_error() {
    // `[? a\n: b]` — `?` explicit-key indicator allows the key to span lines
    // inside a flow sequence (YAML 1.2 §7.4.2).
    let input = "[? a\n: b]\n";
    let events = evs(input);
    let scalars = scalar_values(&events);
    assert!(
        scalars.contains(&"a"),
        "key scalar 'a' must be present; got {scalars:?}"
    );
    assert!(
        scalars.contains(&"b"),
        "value scalar 'b' must be present; got {scalars:?}"
    );
}

// Test 62: `after_colon` flag — implicit single-pair mapping value on next line
#[test]
fn implicit_single_pair_mapping_value_after_colon_on_next_line() {
    // `[a:\n  b]` — key `a`, colon separator, value `b` on the next line.
    // The `after_colon` flag prevents the missing-comma check from firing.
    let input = "[a:\n  b]\n";
    let events = evs(input);
    let scalars = scalar_values(&events);
    assert!(
        scalars.contains(&"a"),
        "key scalar 'a' must be present; got {scalars:?}"
    );
    assert!(
        scalars.contains(&"b"),
        "value scalar 'b' must be present; got {scalars:?}"
    );
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
    let input = "[\n  a,\n  b\n]\n";
    let items = parse_to_vec(input);
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
        assert_eq!(b_span.start, 9, "scalar `b` must start at byte_offset 9");
        assert_eq!(
            line_col(input, b_span.start).0,
            3,
            "scalar `b` must be on line 3 (1-based)"
        );
        assert_eq!(
            line_col(input, b_span.start).1,
            2,
            "scalar `b` must be at column 2 (0-based)"
        );
    } else {
        unreachable!("expected exactly 2 scalar spans");
    }
}

// -----------------------------------------------------------------------
// Group Q: Coverage gap tests — verbatim tag separation inside flow
// collections (YAML 1.2 §6.9.1: s-separate(n,c) required before node
// content)
// -----------------------------------------------------------------------

// Test 63: verbatim tag not separated from content by whitespace, inside a
// flow collection.
#[test]
fn verbatim_tag_in_flow_collection_missing_separator_errors() {
    // `[!<tag:example.com,2000:t>value]` — no space after the closing `>`
    // of the verbatim tag; `value` is unseparated node content.
    let input = "[!<tag:example.com,2000:t>value]\n";
    let result: Vec<_> = parse_events(input).collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "verbatim tag not separated from content by whitespace must return an error"
    );
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message
                .contains("tag must be separated from node content by whitespace"),
            "error message should mention tag/content separation; got: {}",
            err.message
        );
    }
}

// -----------------------------------------------------------------------
// A single pair of a flow sequence has one `:`
// -----------------------------------------------------------------------

/// A second `:` in one entry of a flow sequence needs a comma before it (YAML
/// 1.2.2 [138], [150]).  The parser once wrapped the key in a second mapping
/// and emitted a `MappingStart` without its `MappingEnd`, which made `load`
/// run until memory was exhausted.
#[rstest]
#[case::empty_key_and_value("[: :]\n", 3)]
#[case::spaced("[ : : ]\n", 4)]
#[case::explicit_key("[? : :]\n", 5)]
#[case::nested("[[: :]]\n", 4)]
#[case::plain_key("[a: b: c]\n", 5)]
#[case::collection_value("[a: [b]: c]\n", 7)]
#[case::space_before_the_second_colon("[a: b : c]\n", 6)]
fn second_colon_in_a_flow_sequence_entry_is_an_error(
    #[case] input: &str,
    #[case] error_offset: usize,
) {
    let error = parse_events(input).find_map(Result::err);
    assert!(error.is_some(), "{input:?}");
    if let Some(error) = error {
        assert_eq!(error.pos.byte_offset, error_offset, "{input:?}");
        assert!(
            error.message.contains("missing comma in flow sequence"),
            "{}",
            error.message
        );
    }
    assert!(rlsp_yaml_parser::load(input).is_err(), "{input:?}");
}

/// Entries that each have one `:` stay single pairs.
#[rstest]
#[case::two_pairs("[a: b, c: d]\n", 2)]
#[case::empty_keys("[: a, : b]\n", 2)]
#[case::explicit_key("[? a : b, c]\n", 1)]
#[case::pair_in_a_pair("[a: [b: c]]\n", 2)]
fn flow_sequence_entries_with_one_colon_are_single_pairs(
    #[case] input: &str,
    #[case] mappings: usize,
) {
    let events = event_variants(input);
    let starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    assert_eq!((starts, ends), (mappings, mappings), "{input:?}");
}

// -----------------------------------------------------------------------
// Properties before a comma or a closing delimiter are an empty node
// -----------------------------------------------------------------------

const STR: &str = "tag:yaml.org,2002:str";

/// The scalars and aliases of `input` in order, as `(value, anchor, tag)`; an
/// alias is `("*name", None, None)`.
fn nodes_of(input: &str) -> Vec<(String, Option<String>, Option<String>)> {
    event_variants(input)
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some((
                    value.to_string(),
                    e.anchor().map(str::to_string),
                    e.tag().map(str::to_string),
                ))
            } else if let Event::Alias { name } = e {
                Some((format!("*{name}"), None, None))
            } else {
                None
            }
        })
        .collect()
}

fn node(
    value: &str,
    anchor: Option<&str>,
    tag: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    (
        value.to_string(),
        anchor.map(str::to_string),
        tag.map(str::to_string),
    )
}

/// Properties that a `,`, `]` or `}` follows stand on an empty node of their own
/// (YAML 1.2.2 [96] `c-ns-properties` on an empty node, [105] `e-scalar`).  The
/// parser dropped those before a closing delimiter and rejected those before a
/// comma in a sequence; a property that stays pending is also the anchor of the
/// next node (`[{ b: &x }, *x]` gave a third item).  The expected events are
/// those of libfyaml (`fy-tool --testsuite`, 0.9.6).
#[rstest]
#[case::tag_in_a_sequence("[ !!str ]\n", vec![node("", None, Some(STR))])]
#[case::anchor_in_a_sequence("[ &x ]\n", vec![node("", Some("x"), None)])]
#[case::tag_before_a_comma("[a, !!str , b]\n", vec![node("a", None, None), node("", None, Some(STR)), node("b", None, None)])]
#[case::anchor_before_a_comma_first("[&a , b]\n", vec![node("", Some("a"), None), node("b", None, None)])]
#[case::tag_as_a_key_in_a_mapping("{ !!str }\n", vec![node("", None, Some(STR)), node("", None, None)])]
#[case::tag_as_a_key_before_a_comma("{ !t , b: 2 }\n", vec![node("", None, Some("!t")), node("", None, None), node("b", None, None), node("2", None, None)])]
#[case::tag_as_a_value("{ b: !!str }\n", vec![node("b", None, None), node("", None, Some(STR))])]
#[case::anchor_as_a_value("{ b: &x }\n", vec![node("b", None, None), node("", Some("x"), None)])]
#[case::tag_as_a_value_before_a_comma("{ b: !!str , c }\n", vec![node("b", None, None), node("", None, Some(STR)), node("c", None, None), node("", None, None)])]
#[case::tag_on_a_single_pair_value("[ a: !!str ]\n", vec![node("a", None, None), node("", None, Some(STR))])]
#[case::anchor_on_a_single_pair_value("[ a: &x ]\n", vec![node("a", None, None), node("", Some("x"), None)])]
#[case::tag_on_an_explicit_key("{ ? !!str }\n", vec![node("", None, Some(STR)), node("", None, None)])]
#[case::anchor_does_not_reach_the_next_item("[{ b: &x }, *x]\n", vec![node("b", None, None), node("", Some("x"), None), node("*x", None, None)])]
#[case::anchor_and_tag("[ &a !t ]\n", vec![node("", Some("a"), Some("!t"))])]
fn properties_before_a_comma_or_a_closing_delimiter_are_an_empty_node(
    #[case] input: &str,
    #[case] expected: Vec<(String, Option<String>, Option<String>)>,
) {
    assert_eq!(nodes_of(input), expected, "{input:?}");
}

// -----------------------------------------------------------------------
// A collection or a second `:` after a complete entry needs a comma
// -----------------------------------------------------------------------

/// A flow collection that starts right after an entry, and a second `:` after
/// the one that follows a key, begin another entry without the `,` before it
/// (YAML 1.2.2 [138] `ns-s-flow-seq-entries`, [141] `ns-s-flow-map-entries`).
/// The parser accepted them, and `{ a: 2 !u2 { } }` loaded to data that no
/// output of the formatter has.  Positions equal those of libfyaml
/// (`fy-tool --testsuite`, 0.9.6).
/// So does a plain scalar on the line after a comment: a comment ends the plain
/// scalar before it ([135] `ns-plain-multi-line` has no place for one), and the
/// parser dropped that line.
#[rstest]
#[case::mapping_after_a_plain_value("{ a: b { } }\n", 7, "mapping")]
#[case::mapping_after_a_quoted_value("{ a: \"q\" { } }\n", 9, "mapping")]
#[case::mapping_after_an_alias_value("{ a: *x { } }\n", 8, "mapping")]
#[case::sequence_after_a_plain_value("{ a: b [ ] }\n", 7, "mapping")]
#[case::sequence_after_a_collection_value("{ a: [x] [y] }\n", 9, "mapping")]
#[case::second_colon_after_an_empty_key("{: :}\n", 3, "mapping")]
#[case::collection_after_a_plain_item("[ a [ ] ]\n", 4, "sequence")]
#[case::collection_after_a_quoted_item("[ \"q\" [ ] ]\n", 6, "sequence")]
#[case::collection_after_an_alias_item("[ *x [ ] ]\n", 5, "sequence")]
#[case::collection_after_a_pair("[ a: b [ ] ]\n", 7, "sequence")]
#[case::the_fuzz_input(
    "!u { null: \n#c\n &z-\n2 !u2 { }\n#c\n , #c\n true::\n }\n",
    26,
    "mapping"
)]
#[case::plain_item_after_a_comment("[a # c\n  b]\n", 9, "sequence")]
#[case::plain_item_after_a_comment_line("[a\n # c\n b]\n", 9, "sequence")]
#[case::plain_item_after_a_comment_and_a_blank_line("[a # c\n\n b]\n", 9, "sequence")]
#[case::plain_item_after_a_comment_and_two_blank_lines("[a # c\n\n\n b]\n", 10, "sequence")]
#[case::plain_item_after_a_blank_line_and_a_comment_line("[a\n\n # c\n b]\n", 10, "sequence")]
#[case::plain_key_after_a_comment("{a # c\n b: 1}\n", 8, "mapping")]
#[case::plain_item_after_a_pair_and_a_comment("[a: b # c\n c]\n", 11, "sequence")]
#[case::plain_item_after_an_item_and_a_comment("[a, b # c\n c]\n", 11, "sequence")]
#[case::plain_key_after_a_value_and_a_comment("{a: b # c\n c: d}\n", 11, "mapping")]
#[case::plain_item_after_a_comment_in_a_block_sequence("- [a # c\n  b]\n", 11, "sequence")]
#[case::plain_item_after_a_comment_in_a_block_mapping("x: [a # c\n b]\n", 11, "sequence")]
#[case::plain_item_after_a_comment_in_a_nested_sequence("[[a # c\n b]]\n", 9, "sequence")]
#[case::plain_item_after_a_comment_on_a_continuation("[a\n\n  b #c\n  d]\n", 13, "sequence")]
fn a_flow_collection_after_a_complete_entry_needs_a_comma(
    #[case] input: &str,
    #[case] error_offset: usize,
    #[case] kind: &str,
) {
    let error = parse_events(input).find_map(Result::err);
    assert!(error.is_some(), "{input:?}");
    if let Some(error) = error {
        assert_eq!(error.pos.byte_offset, error_offset, "{input:?}");
        assert!(
            error
                .message
                .contains(&format!("missing comma in flow {kind}")),
            "{}",
            error.message
        );
    }
    assert!(rlsp_yaml_parser::load(input).is_err(), "{input:?}");
}

/// The same positions with the comma are fine, and so are values that start
/// with a collection or a colon.
#[rstest]
#[case::comma_in_a_mapping("{ a: b, { } }\n")]
#[case::comma_in_a_sequence("[ a, [ ] ]\n")]
#[case::collection_as_a_value("{ a: [x] }\n")]
#[case::collection_as_a_pair_value("[ a: [x] ]\n")]
#[case::collection_key("{ [x]: 1, {y}: 2 }\n")]
#[case::value_that_starts_with_a_colon("{ [x]: :c }\n")]
#[case::collection_with_properties("[ !t [a], &x {b} ]\n")]
fn a_flow_collection_after_a_comma_or_a_colon_is_accepted(#[case] input: &str) {
    assert!(!has_error(input), "{input:?}");
}

// -----------------------------------------------------------------------
// Single pairs with properties and `?` in flow collections
// -----------------------------------------------------------------------

/// The events of `input` the way `fy-tool --testsuite` (libfyaml 0.9.6) prints
/// them, without the stream and document lines, joined by spaces.
fn fy_shape(input: &str) -> String {
    let properties = |e: &Event<'_>| {
        let anchor = e.anchor().map_or(String::new(), |a| format!(" &{a}"));
        let tag = e.tag().map_or(String::new(), |t| format!(" <{t}>"));
        format!("{anchor}{tag}")
    };
    event_variants(input)
        .iter()
        .filter_map(|e| match e {
            Event::SequenceStart { style, .. } => Some(format!(
                "+SEQ{}{}",
                if *style == CollectionStyle::Flow {
                    " []"
                } else {
                    ""
                },
                properties(e)
            )),
            Event::MappingStart { style, .. } => Some(format!(
                "+MAP{}{}",
                if *style == CollectionStyle::Flow {
                    " {}"
                } else {
                    ""
                },
                properties(e)
            )),
            Event::SequenceEnd => Some("-SEQ".to_string()),
            Event::MappingEnd => Some("-MAP".to_string()),
            Event::Alias { name } => Some(format!("=ALI *{name}")),
            Event::Scalar { value, .. } => Some(format!("=VAL{} :{value}", properties(e))),
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::Comment { .. } => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Properties before the `:` of a pair in a flow sequence belong to its key, and
/// a `?` with nothing after it, or only a key, is a single pair too (YAML 1.2.2
/// [150] `ns-flow-pair`, [151]).  Before, the key took none of the properties
/// (the value did, or a core tag was dropped) and the entry `[ ? a ]` became the
/// scalar `a`.  Expected events are those of libfyaml.
#[rstest]
#[case::core_tag_on_the_key(
    "[ !!str : a ]\n",
    "+SEQ [] +MAP {} =VAL <tag:yaml.org,2002:str> : =VAL :a -MAP -SEQ"
)]
#[case::tag_on_the_key("[ !t : a ]\n", "+SEQ [] +MAP {} =VAL <!t> : =VAL :a -MAP -SEQ")]
#[case::anchor_on_the_key("[ &x : a ]\n", "+SEQ [] +MAP {} =VAL &x : =VAL :a -MAP -SEQ")]
#[case::anchor_and_tag_on_the_key(
    "[ !t &x : b ]\n",
    "+SEQ [] +MAP {} =VAL &x <!t> : =VAL :b -MAP -SEQ"
)]
#[case::explicit_key_with_a_tag("[ ? !t : a ]\n", "+SEQ [] +MAP {} =VAL <!t> : =VAL :a -MAP -SEQ")]
#[case::explicit_key_with_an_anchor(
    "[ ? &x : a ]\n",
    "+SEQ [] +MAP {} =VAL &x : =VAL :a -MAP -SEQ"
)]
#[case::item_before_a_pair(
    "[ a, !t : b ]\n",
    "+SEQ [] =VAL :a +MAP {} =VAL <!t> : =VAL :b -MAP -SEQ"
)]
#[case::pair_in_a_block_value(
    "k: [ &x : a ]\n",
    "+MAP =VAL :k +SEQ [] +MAP {} =VAL &x : =VAL :a -MAP -SEQ -MAP"
)]
#[case::empty_value_after_a_tagged_key(
    "[ !t : ]\n",
    "+SEQ [] +MAP {} =VAL <!t> : =VAL : -MAP -SEQ"
)]
#[case::tagged_key_and_value(
    "[ ? !!str : !!str ]\n",
    "+SEQ [] +MAP {} =VAL <tag:yaml.org,2002:str> : =VAL <tag:yaml.org,2002:str> : -MAP -SEQ"
)]
#[case::bare_question_mark("[ ? ]\n", "+SEQ [] +MAP {} =VAL : =VAL : -MAP -SEQ")]
#[case::question_mark_and_key("[ ? a ]\n", "+SEQ [] +MAP {} =VAL :a =VAL : -MAP -SEQ")]
#[case::question_mark_and_tag(
    "[ ? !!str ]\n",
    "+SEQ [] +MAP {} =VAL <tag:yaml.org,2002:str> : =VAL : -MAP -SEQ"
)]
#[case::question_mark_and_tag_before_a_comma(
    "[ ? !!str , a ]\n",
    "+SEQ [] +MAP {} =VAL <tag:yaml.org,2002:str> : =VAL : -MAP =VAL :a -SEQ"
)]
#[case::bare_question_mark_before_a_comma(
    "[ ? , a ]\n",
    "+SEQ [] +MAP {} =VAL : =VAL : -MAP =VAL :a -SEQ"
)]
#[case::colon_without_a_value("[ a: ]\n", "+SEQ [] +MAP {} =VAL :a =VAL : -MAP -SEQ")]
#[case::explicit_key_colon_without_a_value(
    "[ ? a : ]\n",
    "+SEQ [] +MAP {} =VAL :a =VAL : -MAP -SEQ"
)]
#[case::bare_question_mark_in_a_mapping("{ ? , a }\n", "+MAP {} =VAL : =VAL : =VAL :a =VAL : -MAP")]
#[case::collection_key_without_a_value("{ [b] }\n", "+MAP {} +SEQ [] =VAL :b -SEQ =VAL : -MAP")]
#[case::collection_key_without_a_value_before_a_comma(
    "{ [b] , c: d }\n",
    "+MAP {} +SEQ [] =VAL :b -SEQ =VAL : =VAL :c =VAL :d -MAP"
)]
fn pairs_with_properties_and_question_marks_in_flow_collections_equal_libfyaml(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(fy_shape(input), expected, "{input:?}");
}

/// libfyaml rejects each of these, and so does the parser: a property before
/// `?`, a second `?` or a `?` after a complete entry, a node, property or `:`
/// that starts another entry without the comma.
#[rstest]
#[case::property_before_a_question_mark("[ !t ? ]\n")]
#[case::second_question_mark("[ ? ? ]\n")]
#[case::question_mark_after_a_colon("[ : ? ]\n")]
#[case::quoted_scalar_after_a_quoted_scalar("[ \"q\" \"q\" ]\n")]
#[case::alias_after_a_collection("[ [b] *x ]\n")]
#[case::property_after_a_collection("[ [b] !t ]\n")]
#[case::plain_key_after_a_quoted_key("{ \"q\" a }\n")]
#[case::second_colon_after_a_value("{ : a : }\n")]
#[case::second_question_mark_in_a_mapping("{ ? ? }\n")]
#[case::question_mark_after_a_colon_in_a_mapping("{ : ? }\n")]
fn a_flow_entry_that_libfyaml_rejects_is_an_error(#[case] input: &str) {
    assert!(has_error(input), "{input:?}");
    assert!(rlsp_yaml_parser::load(input).is_err(), "{input:?}");
}

// -----------------------------------------------------------------------
// A plain scalar over blank lines and comments in flow collections
// -----------------------------------------------------------------------

/// A plain scalar that continues after blank lines folds each of them to a line
/// feed, and a line break with none folds to a space (YAML 1.2.2 [71]
/// `b-l-trimmed`, [72] `b-as-space`, [74] `s-flow-folded`).  Before, every
/// continuation folded to a space and `[a\n\n  b]` loaded `a b`.  Values equal
/// those of libfyaml (`fy-tool --testsuite`, 0.9.6).
#[rstest]
#[case::one_blank_line("[a\n\n  b]\n", &["a\nb"])]
#[case::two_blank_lines("[a\n\n\n b]\n", &["a\n\nb"])]
#[case::four_blank_lines("[a\n\n\n\n\n b]\n", &["a\n\n\n\nb"])]
#[case::blank_line_of_space_and_tab("[a\n \t\n  b]\n", &["a\nb"])]
#[case::blank_line_of_a_tab("[a\n\t\n  b]\n", &["a\nb"])]
#[case::blank_line_with_crlf("[a\r\n\r\n  b]\r\n", &["a\nb"])]
#[case::two_blank_lines_with_crlf("[a\r\n\r\n\r\n b]\r\n", &["a\n\nb"])]
#[case::trailing_space_before_the_break("[a \n\n b]\n", &["a\nb"])]
#[case::trailing_spaces_before_a_single_break("[a  \n  b]\n", &["a b"])]
#[case::four_byte_characters("[😀\n\n 😀]\n", &["😀\n😀"])]
#[case::inner_spaces_stay("[a b\n\n c d]\n", &["a b\nc d"])]
#[case::chained_continuations("[a\n\n b\n c\n\n\n d]\n", &["a\nb c\n\nd"])]
#[case::blank_lines_before_a_new_entry_after_a_comma("[a,\n\n b\n c]\n", &["a", "b c"])]
#[case::continuation_then_a_new_entry("[a\n\n b, c\n d]\n", &["a\nb", "c d"])]
#[case::multibyte("[é\n\n ü]\n", &["é\nü"])]
#[case::hash_inside_the_scalar("[a#b\n\n c]\n", &["a#b\nc"])]
#[case::comment_after_the_continuation("[a\n\n b # c\n, d]\n", &["a\nb", "d"])]
#[case::blank_lines_before_the_end("[a\n\n]\n", &["a"])]
#[case::blank_lines_before_a_comma("[a\n\n, b]\n", &["a", "b"])]
#[case::single_pair_value("[a: b\n\n c]\n", &["a", "b\nc"])]
#[case::value_in_a_nested_sequence("{a: [b\n\n c]}\n", &["a", "b\nc"])]
#[case::comma_ends_the_scalar("[a,\n\n b]\n", &["a", "b"])]
#[case::comma_ends_the_scalar_in_a_mapping("{a: b,\n\n c: d}\n", &["a", "b", "c", "d"])]
fn a_flow_plain_scalar_folds_blank_lines_to_line_feeds(
    #[case] input: &str,
    #[case] expected: &[&str],
) {
    let events = evs(input);
    assert_eq!(scalar_values(&events), expected, "{input:?}");
}

/// The folded scalar is still a plain scalar.
#[test]
fn a_flow_plain_scalar_over_a_blank_line_keeps_the_plain_style() {
    assert!(evs("[a\n\n  b]\n").iter().any(|e| matches!(
        e,
        Event::Scalar {
            style: ScalarStyle::Plain,
            ..
        }
    )));
}

/// The events of these inputs equal those of libfyaml: the folded scalar is a
/// key, a value, or an item, at every nesting.
#[rstest]
#[case::item("[a\n\n  b]\n", "+SEQ [] =VAL :a\nb -SEQ")]
#[case::mapping_key("{a\n\n b: 1}\n", "+MAP {} =VAL :a\nb =VAL :1 -MAP")]
#[case::pair_key("[a\n\n b: 1]\n", "+SEQ [] +MAP {} =VAL :a\nb =VAL :1 -MAP -SEQ")]
#[case::explicit_key("{? a\n\n b: c}\n", "+MAP {} =VAL :a\nb =VAL :c -MAP")]
#[case::pair_key_and_value_on_a_later_line(
    "[a\n\n  b:\n  c]\n",
    "+SEQ [] +MAP {} =VAL :a\nb =VAL :c -MAP -SEQ"
)]
#[case::pair_key_and_value(
    "[a\n\n  b: c\n\n  d]\n",
    "+SEQ [] +MAP {} =VAL :a\nb =VAL :c\nd -MAP -SEQ"
)]
#[case::in_a_block_mapping_value("x: [a\n\n  b]\n", "+MAP =VAL :x +SEQ [] =VAL :a\nb -SEQ -MAP")]
#[case::in_a_block_sequence("- [a\n\n b]\n", "+SEQ +SEQ [] =VAL :a\nb -SEQ -SEQ")]
#[case::in_an_explicit_block_key(
    "? [a\n\n  b]\n: v\n",
    "+MAP +SEQ [] =VAL :a\nb -SEQ =VAL :v -MAP"
)]
#[case::after_an_earlier_entry(
    "{a: 1, b\n\n c: 2}\n",
    "+MAP {} =VAL :a =VAL :1 =VAL :b\nc =VAL :2 -MAP"
)]
fn a_flow_plain_scalar_over_blank_lines_equals_libfyaml(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(fy_shape(input), expected, "{input:?}");
}

/// A comment before a `,`, a `:` or the end leaves the scalar before it whole.
#[rstest]
#[case::comment_before_a_comma("[a # c\n, b]\n", &["a", "b"])]
#[case::comment_before_the_end("[a # c\n]\n", &["a"])]
#[case::comment_line_before_the_end("[a\n# c\n]\n", &["a"])]
#[case::comment_after_the_last_item("[a, b # c\n]\n", &["a", "b"])]
#[case::comment_before_the_colon_of_a_mapping_key("{a # c\n : 1}\n", &["a", "1"])]
#[case::comment_after_a_key_before_the_end("{a # c\n}\n", &["a", ""])]
#[case::comment_after_a_value_before_a_comma("{a: b # c\n , c}\n", &["a", "b", "c", ""])]
#[case::comment_after_an_explicit_key("{? a # c\n : b}\n", &["a", "b"])]
#[case::comment_after_the_colon("{a: # c\n b}\n", &["a", "b"])]
#[case::comment_after_a_continuation("[a\n b # c\n]\n", &["a b"])]
fn a_comment_leaves_the_flow_plain_scalar_before_it_whole(
    #[case] input: &str,
    #[case] expected: &[&str],
) {
    let events = evs(input);
    assert_eq!(scalar_values(&events), expected, "{input:?}");
}

/// A comment between a key and its `:` is an error in a sequence, as in
/// libfyaml: the key of a single pair is on one line.
#[test]
fn a_comment_between_a_pair_key_and_its_colon_in_a_sequence_is_an_error() {
    let input = "[a # c\n : b]\n";
    let error = parse_events(input).find_map(Result::err);
    assert_eq!(error.map(|e| e.pos.byte_offset), Some(8), "{input:?}");
}

/// Quoted scalars fold blank lines as before.
#[rstest]
#[case::double_quoted("[\"a\n\n b\"]\n", "a\nb")]
#[case::single_quoted("['a\n\n b']\n", "a\nb")]
fn a_quoted_flow_scalar_is_unchanged_by_the_plain_scalar_fold(
    #[case] input: &str,
    #[case] expected: &str,
) {
    let events = evs(input);
    assert_eq!(scalar_values(&events), [expected], "{input:?}");
}

/// A plain scalar does not continue a quoted one: it is another entry, and that
/// needs its `,`.
#[rstest]
#[case::after_a_double_quoted_scalar("[\"a\"\n\n b]\n")]
#[case::after_a_single_quoted_scalar("['a'\n\n b]\n")]
fn a_plain_scalar_after_a_quoted_flow_scalar_needs_a_comma(#[case] input: &str) {
    let error = parse_events(input).find_map(Result::err);
    assert_eq!(error.map(|e| e.pos.byte_offset), Some(7), "{input:?}");
}
