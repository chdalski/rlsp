use super::*;

// Module-local helpers (same pattern as `mod flow_collections`).

const fn seq_start_flow() -> Event<'static> {
    Event::SequenceStart {
        anchor: None,
        tag: None,
        style: CollectionStyle::Flow,
    }
}

// -----------------------------------------------------------------------
// Legal combinations — flow-inside-flow (multi-item variants beyond Task 14)
// -----------------------------------------------------------------------

#[test]
fn multi_nested_flow_sequence_inside_sequence() {
    // `[[a, b], [c, d]]` — outer flow seq contains two inner flow seqs.
    // Task 14 covers `[[a, b], c]` (single inner); this covers two inner seqs.
    let events = evs("[[a, b], [c, d]]\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::SequenceStart { .. })),
        3,
        "three SequenceStarts: outer and two inner"
    );
    assert_eq!(
        count(&events, |e| matches!(e, Event::SequenceEnd)),
        3,
        "three SequenceEnds"
    );
    assert_eq!(scalar_values(&events), ["a", "b", "c", "d"]);
}

#[test]
fn multi_nested_flow_mapping_inside_sequence() {
    // `[{a: b}, {c: d}]` — outer flow seq contains two inner flow maps.
    // Task 14 covers `[{a: b}]` (single inner); this covers two inner maps.
    let events = evs("[{a: b}, {c: d}]\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::SequenceStart { .. })),
        1,
        "one outer SequenceStart"
    );
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        2,
        "two inner MappingStarts"
    );
    assert_eq!(count(&events, |e| matches!(e, Event::MappingEnd)), 2);
    assert_eq!(scalar_values(&events), ["a", "b", "c", "d"]);
}

#[test]
fn multi_pair_flow_mapping_with_sequence_values() {
    // `{x: [a, b], y: [c, d]}` — flow map with two flow-sequence values.
    // Task 14 covers `{key: [a, b]}` (one pair); this covers two pairs.
    let events = evs("{x: [a, b], y: [c, d]}\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        1,
        "one outer flow MappingStart"
    );
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        2,
        "two inner flow SequenceStarts"
    );
    assert_eq!(scalar_values(&events), ["x", "a", "b", "y", "c", "d"]);
}

#[test]
fn multi_pair_flow_mapping_with_mapping_values() {
    // `{x: {a: b}, y: {c: d}}` — flow map with two flow-mapping values.
    let events = evs("{x: {a: b}, y: {c: d}}\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        3,
        "three MappingStarts: one outer + two inner"
    );
    assert_eq!(count(&events, |e| matches!(e, Event::MappingEnd)), 3);
    assert_eq!(scalar_values(&events), ["x", "a", "b", "y", "c", "d"]);
}

// -----------------------------------------------------------------------
// Legal combinations — flow-inside-block (multi-item variants)
// -----------------------------------------------------------------------

#[test]
fn multiple_block_sequence_items_with_flow_sequence() {
    // `- [a, b]\n- [c, d]\n` — two block seq items, each a flow seq.
    // Task 14 covers `- [a, b]\n` (single item); this covers two items.
    let events = evs("- [a, b]\n- [c, d]\n");
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        1,
        "one outer block SequenceStart"
    );
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        2,
        "two inner flow SequenceStarts"
    );
    assert_eq!(scalar_values(&events), ["a", "b", "c", "d"]);
}

#[test]
fn multiple_block_sequence_items_with_flow_mapping() {
    // `- {a: b}\n- {c: d}\n` — two block seq items, each a flow map.
    let events = evs("- {a: b}\n- {c: d}\n");
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        1,
        "one outer block SequenceStart"
    );
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        2,
        "two inner flow MappingStarts"
    );
    assert_eq!(scalar_values(&events), ["a", "b", "c", "d"]);
}

#[test]
fn multiple_block_mapping_values_as_flow_sequence() {
    // `x: [a, b]\ny: [c, d]\n` — two-key block map, each value a flow seq.
    // Task 14 covers `key: [a, b]\n` (single key); this covers two keys.
    let events = evs("x: [a, b]\ny: [c, d]\n");
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        1,
        "one outer block MappingStart"
    );
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        2,
        "two inner flow SequenceStarts"
    );
    assert_eq!(scalar_values(&events), ["x", "a", "b", "y", "c", "d"]);
}

#[test]
fn multiple_block_mapping_values_as_flow_mapping() {
    // `x: {a: b}\ny: {c: d}\n` — two-key block map, each value a flow map.
    // Task 14 covers `key: {a: b}\n` (single key); this covers two keys.
    let events = evs("x: {a: b}\ny: {c: d}\n");
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        1,
        "one outer block MappingStart"
    );
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        2,
        "two inner flow MappingStarts"
    );
    assert_eq!(scalar_values(&events), ["x", "a", "b", "y", "c", "d"]);
}

// -----------------------------------------------------------------------
// Legal combinations — three-level nesting (block-block-flow)
// -----------------------------------------------------------------------

#[test]
fn three_level_block_map_block_seq_flow_seq() {
    // `outer:\n  - [a, b]\n  - [c, d]\n` — block map → block seq → flow seq.
    let events = evs("outer:\n  - [a, b]\n  - [c, d]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        "outer block MappingStart present"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        "inner block SequenceStart present"
    );
    assert_eq!(
        count(&events, |e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        2,
        "two innermost flow SequenceStarts"
    );
    let scalars = scalar_values(&events);
    assert!(scalars.contains(&"outer"), "outer key present");
    assert!(scalars.contains(&"a"), "a present");
    assert!(scalars.contains(&"b"), "b present");
    assert!(scalars.contains(&"c"), "c present");
    assert!(scalars.contains(&"d"), "d present");
}

#[test]
fn three_level_block_seq_block_map_flow_map() {
    // `- x:\n    {a: b}\n` — block seq → block map → flow map (continuation line).
    let events = evs("- x:\n    {a: b}\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        "outer block SequenceStart present"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Block,
                ..
            }
        )),
        "inner block MappingStart present"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "innermost flow MappingStart present"
    );
    let scalars = scalar_values(&events);
    assert!(scalars.contains(&"x"), "key x present");
    assert!(scalars.contains(&"a"), "a present");
    assert!(scalars.contains(&"b"), "b present");
}

// -----------------------------------------------------------------------
// Legal combinations — deeply nested all-flow
// -----------------------------------------------------------------------

#[test]
fn deeply_nested_flow_seq_map_seq_map() {
    // `[{a: [b, {c: d}]}]` — flow seq → flow map → flow seq → flow map (4 levels).
    let events = evs("[{a: [b, {c: d}]}]\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::SequenceStart { .. })),
        2,
        "two SequenceStarts (outer and inner)"
    );
    assert_eq!(count(&events, |e| matches!(e, Event::SequenceEnd)), 2);
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        2,
        "two MappingStarts (outer and inner)"
    );
    assert_eq!(count(&events, |e| matches!(e, Event::MappingEnd)), 2);
    assert_eq!(scalar_values(&events), ["a", "b", "c", "d"]);
}

// -----------------------------------------------------------------------
// Legal combination — flow mapping as flow mapping key via `?`
// -----------------------------------------------------------------------

#[test]
fn flow_mapping_as_flow_mapping_key_via_explicit_indicator() {
    // `{? {a: b} : value}` — `?` introduces a flow mapping as the outer
    // mapping's key.  Two MappingStart events, scalars ["a", "b", "value"].
    let events = evs("{? {a: b} : value}\n");
    assert_eq!(
        count(&events, |e| matches!(e, Event::MappingStart { .. })),
        2,
        "outer and inner MappingStart"
    );
    assert_eq!(count(&events, |e| matches!(e, Event::MappingEnd)), 2);
    assert_eq!(scalar_values(&events), ["a", "b", "value"]);
}

// -----------------------------------------------------------------------
// Illegal combinations — block-inside-flow (YAML 1.2 §7.4)
// -----------------------------------------------------------------------

#[test]
fn block_sequence_dash_inside_flow_sequence_returns_error() {
    // `[- a]` — `-` followed by space is the block-sequence entry indicator;
    // it is not allowed inside a flow collection.
    assert!(
        has_error("[- a]\n"),
        "block sequence dash inside flow sequence must return an error"
    );
}

#[test]
fn block_sequence_dash_as_flow_mapping_value_returns_error() {
    // `{k: - a}` — same reason: `-` + space inside flow mapping.
    assert!(
        has_error("{k: - a}\n"),
        "block sequence dash as flow mapping value must return an error"
    );
}

#[test]
fn block_sequence_dash_space_before_close_returns_error() {
    // `[- ]` — `-` followed by space then `]`; same rejection rule.
    assert!(
        has_error("[- ]\n"),
        "block sequence dash before close bracket must return an error"
    );
}

#[test]
fn plain_scalar_dash_non_separator_is_legal_in_flow() {
    // `-x`, `-1`, `-abc` are valid plain scalars in flow context.
    // This guards against over-broad rejection of the `-` character.
    let events = evs("[-x]\n");
    assert_eq!(
        scalar_values(&events),
        ["-x"],
        "'-x' is a valid plain scalar"
    );

    let events2 = evs("[-1]\n");
    assert_eq!(
        scalar_values(&events2),
        ["-1"],
        "'-1' is a valid plain scalar"
    );

    let events3 = evs("[-abc]\n");
    assert_eq!(
        scalar_values(&events3),
        ["-abc"],
        "'-abc' is a valid plain scalar"
    );
}

// -----------------------------------------------------------------------
// Additional edge cases
// -----------------------------------------------------------------------

#[test]
fn flow_mapping_value_is_empty_nested_flow_sequence() {
    // `{a: []}` — flow map value is an empty flow sequence.
    let events = evs("{a: []}\n");
    // Expected window: MappingStart(Flow), Scalar("a"), SequenceStart(Flow),
    // SequenceEnd, MappingEnd.
    let pair = events
        .windows(2)
        .find(|w| matches!(w, [a, _] if *a == seq_start_flow()));
    assert!(
        matches!(pair, Some([_, Event::SequenceEnd])),
        "SequenceStart(Flow) immediately followed by SequenceEnd; events: {events:?}"
    );
    assert_eq!(scalar_values(&events), ["a"]);
}

#[test]
fn deeply_nested_flow_missing_inner_close_returns_error() {
    // `[[a, b]\n` — inner closed but outer unterminated at EOF.
    assert!(
        has_error("[[a, b]\n"),
        "unterminated outer flow sequence must return an error"
    );
}

#[test]
fn explicit_key_on_continuation_line_in_flow_sequence_is_legal() {
    // `[\n? key\n: value\n]\n` — `?` on a new line is consumed as explicit-key
    // indicator inside the flow sequence; `:` is consumed as value separator.
    // The result is a flow sequence with two scalars ["key", "value"].
    let events = evs("[\n? key\n: value\n]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "flow SequenceStart must be present"
    );
    let scalars = scalar_values(&events);
    assert!(
        scalars.contains(&"key"),
        "key scalar present; got {scalars:?}"
    );
    assert!(
        scalars.contains(&"value"),
        "value scalar present; got {scalars:?}"
    );
}

// -----------------------------------------------------------------------
// Span correctness — nested flow collections
// -----------------------------------------------------------------------

#[test]
fn inner_flow_sequence_start_span_in_nested_context() {
    // `[[a]]\n` — inner `[` is at byte offset 1, column 1.
    let items = parse_to_vec("[[a]]\n");
    let seq_spans: Vec<_> = items
        .iter()
        .filter_map(|r| match r {
            Ok((Event::SequenceStart { .. }, span)) => Some(*span),
            Ok(_) | Err(_) => None,
        })
        .collect();
    assert_eq!(seq_spans.len(), 2, "two SequenceStart spans");
    if let [outer, inner] = seq_spans.as_slice() {
        assert_eq!(outer.start.byte_offset, 0, "outer SequenceStart at byte 0");
        assert_eq!(inner.start.byte_offset, 1, "inner SequenceStart at byte 1");
    } else {
        unreachable!("expected exactly two SequenceStart spans");
    }
}
