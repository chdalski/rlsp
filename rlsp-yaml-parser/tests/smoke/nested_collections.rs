use std::fmt::Write as _;

use super::*;

// -----------------------------------------------------------------------
// Span helper (mirrors mappings::find_span, scoped to this module)
// -----------------------------------------------------------------------

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

// Exhaustive non-scalar arm used by filter_map helpers below.
fn scalar_value<'a>(e: &'a Event<'a>) -> Option<&'a str> {
    match e {
        Event::Scalar { value, .. } => Some(value.as_ref()),
        Event::StreamStart
        | Event::StreamEnd
        | Event::DocumentStart { .. }
        | Event::DocumentEnd { .. }
        | Event::SequenceStart { .. }
        | Event::SequenceEnd
        | Event::MappingStart { .. }
        | Event::MappingEnd
        | Event::Alias { .. }
        | Event::Comment { .. } => None,
    }
}

// -----------------------------------------------------------------------
// Group A: Basic two-level cross-type combinations
// -----------------------------------------------------------------------

#[test]
fn seq_of_mappings_event_order() {
    // `- key: val\n- other: thing\n`
    // Expected: SeqStart, MapStart, "key", "val", MapEnd, MapStart, "other", "thing", MapEnd, SeqEnd
    let events = event_variants("- key: val\n- other: thing\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let map_ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    assert_eq!(seq_starts, 1, "one SequenceStart");
    assert_eq!(map_starts, 2, "two MappingStart (one per entry)");
    assert_eq!(map_ends, 2, "two MappingEnd");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["key", "val", "other", "thing"]);
}

#[test]
fn map_of_sequences_event_order() {
    // `key:\n  - a\n  - b\n`
    // Expected: MapStart, "key", SeqStart, "a", "b", SeqEnd, MapEnd
    let events = event_variants("key:\n  - a\n  - b\n");
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    assert_eq!(map_starts, 1, "one MappingStart");
    assert_eq!(seq_starts, 1, "one SequenceStart");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["key", "a", "b"]);
    // SequenceStart after "key" scalar, SequenceEnd before MappingEnd
    let key_pos = events
        .iter()
        .position(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"))
        .unwrap_or_else(|| unreachable!("key scalar must exist"));
    let seq_start_pos = events
        .iter()
        .position(|e| matches!(e, Event::SequenceStart { .. }))
        .unwrap_or_else(|| unreachable!("SequenceStart must exist"));
    let seq_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::SequenceEnd))
        .unwrap_or_else(|| unreachable!("SequenceEnd must exist"));
    let map_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::MappingEnd))
        .unwrap_or_else(|| unreachable!("MappingEnd must exist"));
    assert!(key_pos < seq_start_pos, "key scalar before SequenceStart");
    assert!(seq_end_pos < map_end_pos, "SequenceEnd before MappingEnd");
}

#[test]
fn map_of_sequences_two_keys() {
    // Two map entries each with a sequence value
    let events = event_variants("a:\n  - 1\n  - 2\nb:\n  - 3\n  - 4\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    assert_eq!(seq_starts, 2, "two sequences (one per key)");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["a", "1", "2", "b", "3", "4"]);
}

// -----------------------------------------------------------------------
// Group B: Three-level combinations
// -----------------------------------------------------------------------

#[test]
fn seq_of_map_of_seq() {
    // `- key:\n    - a\n    - b\n`
    let events = event_variants("- key:\n    - a\n    - b\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    assert_eq!(seq_starts, 2, "outer seq + inner seq");
    assert_eq!(map_starts, 1, "one mapping");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["key", "a", "b"]);
}

#[test]
fn map_of_seq_of_map() {
    // `key:\n  - a: 1\n  - b: 2\n`
    let events = event_variants("key:\n  - a: 1\n  - b: 2\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    assert_eq!(seq_starts, 1, "one sequence");
    assert_eq!(map_starts, 3, "outer map + two inner maps");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["key", "a", "1", "b", "2"]);
}

#[test]
fn seq_of_seq_of_map() {
    // `- - key: val\n`
    let events = event_variants("- - key: val\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    assert_eq!(seq_starts, 2, "two sequences");
    assert_eq!(map_starts, 1, "one mapping");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["key", "val"]);
}

#[test]
fn map_of_map_of_seq() {
    // `a:\n  b:\n    - x\n    - y\n`
    let events = event_variants("a:\n  b:\n    - x\n    - y\n");
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    assert_eq!(map_starts, 2, "two mappings");
    assert_eq!(seq_starts, 1, "one sequence");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["a", "b", "x", "y"]);
}

// -----------------------------------------------------------------------
// Group C: Compact inline forms
// -----------------------------------------------------------------------

#[test]
fn compact_single_mapping_entry_in_sequence() {
    // `- key: value\n` — mapping starts on same line as sequence dash
    let events = event_variants("- key: value\n");
    let has_seq = events
        .iter()
        .any(|e| matches!(e, Event::SequenceStart { .. }));
    let has_map = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(has_seq, "SequenceStart must be emitted");
    assert!(has_map, "MappingStart must be emitted");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["key", "value"]);
}

#[test]
fn compact_chained_mapping_in_sequence() {
    // `- key: value\n  other: thing\n` — two keys in one compact mapping
    // YAML §8.2.3: the mapping started at column 2 continues as long as
    // subsequent lines have the same indent.
    let events = event_variants("- key: value\n  other: thing\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    assert_eq!(seq_starts, 1, "one SequenceStart");
    assert_eq!(map_starts, 1, "one MappingStart for both keys");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["key", "value", "other", "thing"]);
}

#[test]
fn compact_seq_of_seq_inline() {
    // `- - nested\n` — sequence of sequence, compact form
    let events = event_variants("- - nested\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    assert_eq!(seq_starts, 2, "two SequenceStart for inline nesting");
    let scalar = events.iter().find_map(scalar_value);
    assert_eq!(scalar, Some("nested"));
}

#[test]
fn explicit_key_form_in_sequence() {
    // `- ? key\n  : value\n` — explicit-key form inside a sequence
    let events = event_variants("- ? key\n  : value\n");
    let has_seq = events
        .iter()
        .any(|e| matches!(e, Event::SequenceStart { .. }));
    let has_map = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(has_seq, "SequenceStart must be emitted");
    assert!(has_map, "MappingStart must be emitted");
    // "key" and "value" must both appear as scalars
    let has_key = events
        .iter()
        .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"));
    let has_val = events
        .iter()
        .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "value"));
    assert!(has_key, "scalar 'key' must be emitted");
    assert!(has_val, "scalar 'value' must be emitted");
}

// -----------------------------------------------------------------------
// Group D: Indent edge cases
// -----------------------------------------------------------------------

#[test]
fn strict_indent_map_seq_map() {
    // Zero-indent mapping → 2-space sequence → 4-space mapping
    // `outer:\n  - a: 1\n    b: 2\n`
    let events = event_variants("outer:\n  - a: 1\n    b: 2\n");
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    assert_eq!(map_starts, 2, "outer mapping + inner compact mapping");
    assert_eq!(seq_starts, 1, "one sequence");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["outer", "a", "1", "b", "2"]);
}

#[test]
fn multi_level_dedent_closes_nested_collections() {
    // Updated in Task 21: reference parser rejects this input per YAML 1.2
    // §8.2.1 (block sequence structure); conformance fix makes streaming parser match.
    // `- key:\n    - inner\nother: val\n`
    // After `inner`, dedent back to col 0 closes: inner seq, mapping, outer seq.
    // `other: val` at col 0 is an error — the document's root sequence already
    // ended and a bare mapping key cannot follow at the same indent level
    // (YAML 1.2 spec; confirmed by rlsp-yaml-parser reference impl).
    let results = parse_to_vec("- key:\n    - inner\nother: val\n");
    let has_inner = results
        .iter()
        .any(|r| matches!(r, Ok((Event::Scalar { value, .. }, _)) if value.as_ref() == "inner"));
    assert!(has_inner, "scalar 'inner' must be emitted before the error");
    let has_error = results.iter().any(Result::is_err);
    assert!(
        has_error,
        "parse error expected after 'other: val' at root sequence indent"
    );
}

#[test]
fn three_level_inline_seq_item_at_col_six() {
    // `- - - item\n` — 3 levels of inline nesting, item at column 6
    let events = event_variants("- - - item\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    assert_eq!(seq_starts, 3, "three SequenceStart events");
    let scalar = events.iter().find_map(scalar_value);
    assert_eq!(scalar, Some("item"));
}

// -----------------------------------------------------------------------
// Group E: Span correctness
// -----------------------------------------------------------------------

#[test]
fn scalar_span_inside_nested_compact_mapping_key() {
    // `- key: value\n`
    // '-'=0, ' '=1, k=2, e=3, y=4, ':'=5, ' '=6, v=7...
    // key must start at byte 2, column 2
    let results = parse_to_vec("- key: value\n");
    let key_span = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"),
    );
    assert!(key_span.is_some(), "key scalar must have a span");
    if let Some(span) = key_span {
        assert_eq!(span.start.byte_offset, 2, "key at byte 2");
        assert_eq!(span.start.column, 2, "key at column 2");
    }
}

#[test]
fn scalar_span_inside_nested_compact_mapping_value() {
    // `- key: value\n`
    // value starts at byte 7, column 7
    let results = parse_to_vec("- key: value\n");
    let val_span = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "value"),
    );
    assert!(val_span.is_some(), "value scalar must have a span");
    if let Some(span) = val_span {
        assert_eq!(span.start.byte_offset, 7, "value at byte 7");
        assert_eq!(span.start.column, 7, "value at column 7");
    }
}

#[test]
fn mapping_start_span_inside_nested_sequence() {
    // `- key: value\n` — MappingStart inside sequence item
    // MappingStart must point at 'k' (the key), byte 2, column 2
    let results = parse_to_vec("- key: value\n");
    let span = find_span(&results, |e| matches!(e, Event::MappingStart { .. }));
    assert!(span.is_some(), "MappingStart must have a span");
    if let Some(span) = span {
        assert_eq!(
            span.start.byte_offset, 2,
            "MappingStart inside seq item must point at byte 2"
        );
        assert_eq!(span.start.column, 2);
    }
}

#[test]
fn sequence_start_span_inside_mapping_value() {
    // `key:\n  - a\n`
    // k=0,e=1,y=2,:=3,\n=4 → line 2: ' '=5,' '=6,'-'=7,' '=8,'a'=9,\n=10
    // SequenceStart must point at '-' indicator: byte 7, column 2
    let results = parse_to_vec("key:\n  - a\n");
    let seq_span = find_span(&results, |e| matches!(e, Event::SequenceStart { .. }));
    assert!(seq_span.is_some(), "SequenceStart must have a span");
    if let Some(span) = seq_span {
        assert_eq!(
            span.start.byte_offset, 7,
            "SequenceStart must point at '-' (byte 7)"
        );
        assert_eq!(span.start.column, 2, "SequenceStart at column 2");
    }
}

#[test]
fn nested_mapping_value_span_indented_key() {
    // `outer:\n  inner: val\n`
    // outer=0..5, :=5, \n=6 → line 2: ' '=7,' '=8,i=9...r=13,:=14,' '=15,v=16..l=18,\n=19
    // "val" starts at byte 16, column 9
    let results = parse_to_vec("outer:\n  inner: val\n");
    let val_span = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "val"),
    );
    assert!(val_span.is_some(), "val scalar must have a span");
    if let Some(span) = val_span {
        assert_eq!(span.start.byte_offset, 16, "val at byte 16");
        assert_eq!(span.start.column, 9, "val at column 9");
    }
}

#[test]
fn sequence_start_span_in_compact_seq_map() {
    // `- key: value\n` — SequenceStart must anchor at byte 0, column 0
    let results = parse_to_vec("- key: value\n");
    let seq_span = find_span(&results, |e| matches!(e, Event::SequenceStart { .. }));
    assert!(seq_span.is_some(), "SequenceStart must have a span");
    if let Some(span) = seq_span {
        assert_eq!(
            span.start.byte_offset, 0,
            "SequenceStart must anchor at byte 0"
        );
        assert_eq!(span.start.column, 0, "SequenceStart at column 0");
        assert_eq!(span.start.line, 1, "SequenceStart at line 1");
    }
}

#[test]
fn seq_of_seq_of_map_spans() {
    // `- - key: val\n`
    // byte layout: '-'=0,' '=1,'-'=2,' '=3,k=4,e=5,y=6,':'=7,' '=8,v=9,a=10,l=11,\n=12
    // Outer SequenceStart: byte 0, col 0
    // Inner SequenceStart: byte 2, col 2
    // MappingStart: byte 4, col 4
    // key scalar: byte 4, col 4
    // value scalar: byte 9, col 9
    let results = parse_to_vec("- - key: val\n");
    let seq_start_spans: Vec<_> = results
        .iter()
        .filter_map(|r| {
            r.as_ref()
                .ok()
                .filter(|(e, _)| matches!(e, Event::SequenceStart { .. }))
                .map(|(_, span)| *span)
        })
        .collect();
    let [outer, inner] = seq_start_spans.as_slice() else {
        unreachable!(
            "expected exactly 2 SequenceStart events, got {}",
            seq_start_spans.len()
        )
    };
    assert_eq!(outer.start.byte_offset, 0, "outer SeqStart at byte 0");
    assert_eq!(outer.start.column, 0, "outer SeqStart at col 0");
    assert_eq!(outer.start.line, 1, "outer SeqStart at line 1");
    assert_eq!(inner.start.byte_offset, 2, "inner SeqStart at byte 2");
    assert_eq!(inner.start.column, 2, "inner SeqStart at col 2");
    assert_eq!(inner.start.line, 1, "inner SeqStart at line 1");
    let map_span = find_span(&results, |e| matches!(e, Event::MappingStart { .. }));
    assert!(map_span.is_some(), "MappingStart must have a span");
    if let Some(span) = map_span {
        assert_eq!(span.start.byte_offset, 4, "MappingStart at byte 4");
        assert_eq!(span.start.column, 4, "MappingStart at col 4");
    }
    let key_span = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key"),
    );
    assert!(key_span.is_some(), "key scalar must have a span");
    if let Some(span) = key_span {
        assert_eq!(span.start.byte_offset, 4, "key at byte 4");
        assert_eq!(span.start.column, 4, "key at col 4");
    }
    let val_span = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "val"),
    );
    assert!(val_span.is_some(), "val scalar must have a span");
    if let Some(span) = val_span {
        assert_eq!(span.start.byte_offset, 9, "val at byte 9");
        assert_eq!(span.start.column, 9, "val at col 9");
    }
}

#[test]
fn mapping_and_scalar_spans_with_seq_value() {
    // `key:\n  - item\n`
    // byte layout: k=0,e=1,y=2,:=3,\n=4 → line 2: ' '=5,' '=6,'-'=7,' '=8,i=9..m=12,\n=13
    // MappingStart: byte 0, col 0, line 1
    // key scalar "key": byte 0, col 0, line 1
    // SequenceStart: byte 7, col 2, line 2  (already asserted by sequence_start_span_inside_mapping_value)
    // item scalar: byte 9, col 4, line 2
    let results = parse_to_vec("key:\n  - item\n");
    let map_span = find_span(&results, |e| matches!(e, Event::MappingStart { .. }));
    assert!(map_span.is_some(), "MappingStart must have a span");
    if let Some(span) = map_span {
        assert_eq!(span.start.byte_offset, 0, "outer MappingStart at byte 0");
        assert_eq!(span.start.column, 0, "outer MappingStart at col 0");
        assert_eq!(span.start.line, 1, "outer MappingStart at line 1");
    }
    let item_span = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "item"),
    );
    assert!(item_span.is_some(), "item scalar must have a span");
    if let Some(span) = item_span {
        assert_eq!(span.start.byte_offset, 9, "item at byte 9");
        assert_eq!(span.start.column, 4, "item at col 4");
        assert_eq!(span.start.line, 2, "item at line 2");
    }
}

// -----------------------------------------------------------------------
// Group F: Depth limit with alternating seq/map patterns
// -----------------------------------------------------------------------

#[test]
fn alternating_seq_map_exceeding_depth_returns_error() {
    // Build alternating `- k:\n` pairs, each one level deeper.
    // Level 0: `- k:\n`, Level 1: `  - k:\n`, Level 2: `    - k:\n` ...
    // The first line is at col 0, each subsequent pair adds 2 more spaces.
    // With MAX_COLLECTION_DEPTH+1 seq/map pairs total we should hit the limit.
    let depth = MAX_COLLECTION_DEPTH / 2 + 1;
    let mut input = String::new();
    for i in 0..depth {
        input.push_str(&"  ".repeat(i * 2));
        input.push_str("- k:\n");
        input.push_str(&"  ".repeat(i * 2 + 1));
        // This value line is empty — next iteration provides the nested seq
    }
    // Terminate with a final scalar value at the deepest indent.
    input.push_str(&"  ".repeat(depth * 2));
    input.push_str("v\n");
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        has_error,
        "alternating seq/map at depth {depth} (>{MAX_COLLECTION_DEPTH}) must produce an Err"
    );
}

#[test]
fn alternating_seq_map_at_depth_boundary_succeeds() {
    // Exactly MAX_COLLECTION_DEPTH/4 alternating pairs — well within limit.
    let depth = MAX_COLLECTION_DEPTH / 4;
    let mut input = String::new();
    for i in 0..depth {
        input.push_str(&"  ".repeat(i * 2));
        input.push_str("- k:\n");
    }
    // Final level: scalar value
    input.push_str(&"  ".repeat(depth * 2));
    input.push_str("v\n");
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        !has_error,
        "alternating seq/map at depth {depth} (within limit) must succeed"
    );
}

#[test]
fn alternating_seq_map_depth_increments_for_each_level() {
    // Verify that each alternating seq/map level increments the depth counter
    // (not just one or the other). Build pairs of `- k:\n` at increasing
    // indents to approach the limit; the test asserts no panic regardless of
    // whether the limit fires.
    let depth_pairs = MAX_COLLECTION_DEPTH / 2;
    let mut input = String::new();
    for i in 0..depth_pairs {
        let _ = write!(
            input,
            "{}- k:\n{}  ",
            "  ".repeat(i * 2),
            "  ".repeat(i * 2 + 1)
        );
    }
    let results = parse_to_vec(&input);
    // Either within or over the limit — either way no panic.
    let _had_error = results.iter().any(Result::is_err);
}

// -----------------------------------------------------------------------
// Group G: Security — depth-limit, error termination, value-phase safety
// -----------------------------------------------------------------------

#[test]
fn alternating_seq_map_at_exactly_max_depth_succeeds() {
    // Exactly MAX_COLLECTION_DEPTH alternating levels: each `- k:\n` pair
    // opens one Sequence + one Mapping = 2 depth entries.
    // We use MAX_COLLECTION_DEPTH / 2 pairs (each pair = 2 entries).
    let pairs = MAX_COLLECTION_DEPTH / 2;
    let mut input = String::new();
    for i in 0..pairs {
        input.push_str(&"  ".repeat(i * 2));
        input.push_str("- k:\n");
    }
    // Final level: scalar value at the innermost indent.
    input.push_str(&"  ".repeat(pairs * 2));
    input.push_str("v\n");
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        !has_error,
        "exactly {pairs} alternating seq/map pairs ({} depth) must succeed",
        pairs * 2
    );
}

#[test]
fn compact_inline_chain_at_depth_limit_returns_error() {
    // `- - - ... item` with MAX_COLLECTION_DEPTH + 1 dashes.
    // Each `-` opens a new Sequence, so depth = number of dashes.
    let depth = MAX_COLLECTION_DEPTH + 1;
    let input = "- ".repeat(depth) + "item\n";
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        has_error,
        "inline chain with {depth} dashes must produce an Err (limit is {MAX_COLLECTION_DEPTH})"
    );
}

#[test]
fn iterator_returns_none_after_depth_limit_error() {
    // After an Err is yielded, the iterator must return None on all
    // subsequent calls (no infinite error loops).
    let depth = MAX_COLLECTION_DEPTH + 1;
    let input = "- ".repeat(depth) + "item\n";
    let mut iter = parse_events(&input);
    // Consume until we get an Err.
    let mut found_error = false;
    for result in iter.by_ref() {
        if result.is_err() {
            found_error = true;
            break;
        }
    }
    assert!(found_error, "expected an Err from depth-limit input");
    // After the Err, every subsequent call must return None.
    assert!(
        iter.next().is_none(),
        "iterator must return None after an Err"
    );
    assert!(
        iter.next().is_none(),
        "iterator must return None on repeated calls after Err"
    );
}

#[test]
fn mapping_in_value_phase_emits_empty_scalar_on_close() {
    // Updated in Task 21: reference parser rejects this input per YAML 1.2
    // §8.2 (block mapping structure); conformance fix makes streaming parser match.
    // `outer:\n  key:\nbaz\n` — "outer" maps to an inner mapping, "key" has
    // no value.  After closing both mappings, "baz" at indent 0 is outside
    // the document root and is an error per YAML 1.2 (confirmed by
    // rlsp-yaml-parser reference impl: closes both mappings, then Err on
    // "baz").  The key scalars "outer" and "key" must be emitted.
    let results = parse_to_vec("outer:\n  key:\nbaz\n");
    let scalars: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            Ok((Event::Scalar { value, .. }, _)) => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(
        scalars.contains(&"outer"),
        "scalar 'outer' must be emitted; got {scalars:?}"
    );
    assert!(
        scalars.contains(&"key"),
        "scalar 'key' must be emitted; got {scalars:?}"
    );
    let has_error = results.iter().any(Result::is_err);
    assert!(
        has_error,
        "parse error expected: 'baz' at col 0 is outside the document root"
    );
}

// -----------------------------------------------------------------------
// Group H: seq-spaces rule (YAML §8.2.1) and additional coverage
// -----------------------------------------------------------------------

// NC-03: bare `-` with mapping body on next line (fixture 229Q)
#[test]
fn seq_of_mappings_bare_dash_multiline_body() {
    // `"-\n  name: Alice\n  age: 30\n-\n  name: Bob\n  age: 25\n"`
    // Bare `-` on its own line (no trailing space), mapping body on next line.
    let events = event_variants("-\n  name: Alice\n  age: 30\n-\n  name: Bob\n  age: 25\n");
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let map_ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    assert_eq!(map_starts, 2, "two MappingStart (one per bare-dash entry)");
    assert_eq!(map_ends, 2, "two MappingEnd");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(
        scalars,
        ["name", "Alice", "age", "30", "name", "Bob", "age", "25"]
    );
}

// NC-05: two sequence items each with multiple mapping keys
#[test]
fn seq_of_mappings_multi_item_multi_key() {
    // Two sequence items, each with two keys:
    // `- a: 1\n  b: 2\n- c: 3\n  d: 4\n`
    let events = event_variants("- a: 1\n  b: 2\n- c: 3\n  d: 4\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    assert_eq!(seq_starts, 1, "one SequenceStart");
    assert_eq!(map_starts, 2, "two MappingStart (one per item)");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["a", "1", "b", "2", "c", "3", "d", "4"]);
}

// NC-11: seq-spaces rule — sequence at same indent as parent mapping key
// YAML §8.2.1: seq-spaces(n, block-out) = n (not n+1).
// Fixture AZ63. This is the most commonly mishandled indent rule.
#[test]
fn zero_indent_sequence_as_mapping_value() {
    // "one:\n- 2\n- 3\nfour: 5\n"
    // Sequence at col 0 is the value of "one"; "four" continues the mapping.
    let events = event_variants("one:\n- 2\n- 3\nfour: 5\n");
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(map_starts, 1, "one outer MappingStart");
    assert_eq!(seq_starts, 1, "one SequenceStart");
    assert_eq!(seq_ends, 1, "one SequenceEnd");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["one", "2", "3", "four", "5"]);
    // SequenceEnd must appear before "four" scalar
    let seq_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::SequenceEnd))
        .unwrap_or_else(|| unreachable!("SequenceEnd must exist"));
    let four_pos = events
        .iter()
        .position(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "four"))
        .unwrap_or_else(|| unreachable!("four scalar must exist"));
    assert!(
        seq_end_pos < four_pos,
        "SequenceEnd must precede scalar 'four'"
    );
}

// NC-12: seq-spaces rule with sibling mapping key after zero-indent sequence
#[test]
fn zero_indent_sequence_as_mapping_value_sibling_keys() {
    // "alpha:\n- x\n- y\nbeta: z\n"
    // Same seq-spaces pattern with a second mapping key. Verifies the
    // sequence closes correctly and "beta" is parsed as a sibling key in
    // the same outer mapping, not as a new document.
    let events = event_variants("alpha:\n- x\n- y\nbeta: z\n");
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let map_ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(map_starts, 1, "one outer mapping");
    assert_eq!(map_ends, 1, "one MappingEnd");
    assert_eq!(seq_starts, 1, "one sequence");
    assert_eq!(seq_ends, 1, "one SequenceEnd");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["alpha", "x", "y", "beta", "z"]);
}

// NC-16: scalar span for key two levels deep in compact mapping
#[test]
fn key_scalar_span_in_two_level_deep_compact_mapping() {
    // `outer:\n  - inner_key: inner_val\n`
    // outer=0..4,:=5,\n=6 → line 2: ' '=7,' '=8,'-'=9,' '=10
    // inner_key starts at byte 11, column 4
    let results = parse_to_vec("outer:\n  - inner_key: inner_val\n");
    let span = find_span(
        &results,
        |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "inner_key"),
    );
    assert!(span.is_some(), "inner_key scalar must have a span");
    if let Some(s) = span {
        assert_eq!(s.start.byte_offset, 11, "inner_key at byte 11");
        assert_eq!(s.start.column, 4, "inner_key at column 4");
    }
}

// NC-17: MappingStart span in the second sequence item
#[test]
fn mapping_start_span_in_seq_of_mappings_second_item() {
    // "- a: 1\n- b: 2\n"
    // First MappingStart: byte 2, col 2 (at 'a')
    // Second MappingStart: byte 9, col 2 (at 'b')
    let results = parse_to_vec("- a: 1\n- b: 2\n");
    let map_start_spans: Vec<_> = results
        .iter()
        .filter_map(|r| {
            r.as_ref().ok().and_then(|(ev, span)| {
                if matches!(ev, Event::MappingStart { .. }) {
                    Some(*span)
                } else {
                    None
                }
            })
        })
        .collect();
    assert_eq!(map_start_spans.len(), 2, "exactly 2 MappingStart events");
    let [first, second] = map_start_spans.as_slice() else {
        unreachable!("expected exactly two MappingStart spans");
    };
    assert_eq!(first.start.byte_offset, 2, "first MappingStart at byte 2");
    assert_eq!(first.start.column, 2, "first MappingStart at column 2");
    assert_eq!(second.start.byte_offset, 9, "second MappingStart at byte 9");
    assert_eq!(second.start.column, 2, "second MappingStart at column 2");
}

// NC-18: compact sequence items each with multiple mapping keys (fixture 9U5K)
#[test]
fn seq_of_mappings_compact_item_content_scalar_ordering() {
    // `- item: Super Hoop\n  quantity: 1\n- item: Basketball\n  quantity: 4\n`
    let events =
        event_variants("- item: Super Hoop\n  quantity: 1\n- item: Basketball\n  quantity: 4\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    assert_eq!(seq_starts, 1, "one SequenceStart");
    assert_eq!(map_starts, 2, "two MappingStart (one per item)");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(
        scalars,
        [
            "item",
            "Super Hoop",
            "quantity",
            "1",
            "item",
            "Basketball",
            "quantity",
            "4"
        ]
    );
}

// NC-19: seq-spaces rule — sequence value terminates when sibling mapping key appears
#[test]
fn mapping_value_sequence_closes_on_sibling_key_same_indent() {
    // "a:\n- 1\nb: 2\n"
    // Sequence closes when sibling key "b" appears at col 0.
    // Must be: 1 MappingStart, 1 SequenceStart, 1 SequenceEnd, 1 MappingEnd.
    let events = event_variants("a:\n- 1\nb: 2\n");
    let map_starts = events
        .iter()
        .filter(|e| matches!(e, Event::MappingStart { .. }))
        .count();
    let map_ends = events
        .iter()
        .filter(|e| matches!(e, Event::MappingEnd))
        .count();
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(
        map_starts, 1,
        "one outer mapping (b is a sibling key, not a new mapping)"
    );
    assert_eq!(map_ends, 1, "one MappingEnd");
    assert_eq!(seq_starts, 1, "one SequenceStart");
    assert_eq!(seq_ends, 1, "one SequenceEnd");
    let scalars: Vec<_> = events.iter().filter_map(scalar_value).collect();
    assert_eq!(scalars, ["a", "1", "b", "2"]);
}
