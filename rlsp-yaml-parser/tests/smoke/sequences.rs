use rstest::rstest;

use super::*;

// -----------------------------------------------------------------------
// Spike: integration test through parse_events public entry point
// -----------------------------------------------------------------------

#[test]
fn single_entry_sequence_through_parse_events() {
    let results: Vec<_> = parse_events("- hello\n").collect();
    let has_start = results
        .iter()
        .any(|r| matches!(r, Ok((Event::SequenceStart { .. }, _))));
    let has_end = results
        .iter()
        .any(|r| matches!(r, Ok((Event::SequenceEnd, _))));
    assert!(has_start, "parse_events must emit SequenceStart");
    assert!(has_end, "parse_events must emit SequenceEnd");
}

// -----------------------------------------------------------------------
// Group A: Basic flat sequences
// -----------------------------------------------------------------------

#[test]
fn single_entry_sequence_emits_correct_event_order() {
    let events = event_variants("- hello\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::SequenceStart {
                anchor: None,
                tag: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "hello".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::SequenceEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_entry_flat_sequence() {
    let events = event_variants("- foo\n- bar\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::SequenceStart {
                anchor: None,
                tag: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "foo".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Scalar {
                value: "bar".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::SequenceEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn three_entry_flat_sequence() {
    // StreamStart + DocStart + SeqStart + 3 scalars + SeqEnd + DocEnd + StreamEnd = 9
    let events = event_variants("- a\n- b\n- c\n");
    assert_eq!(events.len(), 9, "expected 9 events total");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(seq_starts, 1, "exactly one SequenceStart");
    assert_eq!(seq_ends, 1, "exactly one SequenceEnd");
    let scalar_count = events
        .iter()
        .filter(|e| matches!(e, Event::Scalar { .. }))
        .count();
    assert_eq!(scalar_count, 3, "three scalars");
}

// -----------------------------------------------------------------------
// Group B: Empty items
// -----------------------------------------------------------------------

// Dash-only items (with or without trailing space) emit an empty plain scalar.
#[rstest]
// Bare `-\n` — no value token; empty scalar is synthesized.
#[case::dash_followed_by_newline("-\n")]
// `- \n` — trailing space stripped; same result as bare dash.
#[case::dash_space_then_newline("- \n")]
fn sequence_empty_item_emits_empty_plain_scalar(#[case] input: &str) {
    let events = event_variants(input);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            } if value.as_ref() == ""
        )),
        "empty sequence item must emit empty plain scalar for input: {input:?}"
    );
}

#[test]
fn mixed_empty_and_nonempty_items() {
    let events = event_variants("- foo\n-\n- bar\n");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
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
        })
        .collect();
    assert_eq!(scalars, ["foo", "", "bar"]);
}

// -----------------------------------------------------------------------
// Group C: Nested sequences
// -----------------------------------------------------------------------

#[test]
fn two_level_nested_sequence_inline() {
    // `- - inner\n` — outer at col 0, inner at col 2 (inline)
    let events = event_variants("- - inner\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::SequenceStart {
                anchor: None,
                tag: None,
                style: CollectionStyle::Block,
            },
            Event::SequenceStart {
                anchor: None,
                tag: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "inner".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::SequenceEnd,
            Event::SequenceEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_level_nested_sequence_multiline() {
    let input = "- - a\n  - b\n";
    let events = event_variants(input);
    // outer SequenceStart, inner SequenceStart, Scalar(a), Scalar(b),
    // inner SequenceEnd, outer SequenceEnd
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(seq_starts, 2, "two SequenceStart events");
    assert_eq!(seq_ends, 2, "two SequenceEnd events");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
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
        })
        .collect();
    assert_eq!(scalars, ["a", "b"]);
    // Verify nesting order: inner SequenceEnd before outer SequenceEnd
    let positions: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Event::SequenceEnd))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(positions.len(), 2);
    if let [first, second] = positions.as_slice() {
        assert!(first < second, "inner SequenceEnd before outer");
    }
}

#[test]
fn three_level_nested_sequence() {
    let events = event_variants("- - - deep\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(seq_starts, 3, "three SequenceStart events");
    assert_eq!(seq_ends, 3, "three SequenceEnd events");
    // Scalar "deep" must appear between the innermost SequenceStart and SequenceEnd.
    let scalar_pos = events
        .iter()
        .position(|e| matches!(e, Event::Scalar { .. }));
    let last_start_pos = events
        .iter()
        .rposition(|e| matches!(e, Event::SequenceStart { .. }));
    let first_end_pos = events.iter().position(|e| matches!(e, Event::SequenceEnd));
    assert!(
        scalar_pos > last_start_pos,
        "scalar must follow the innermost SequenceStart"
    );
    assert!(
        scalar_pos < first_end_pos,
        "scalar must precede the innermost SequenceEnd"
    );
}

#[test]
fn sibling_sequences_at_same_indent() {
    // outer seq: [a, [b, c], d]
    let input = "- a\n- - b\n  - c\n- d\n";
    let events = event_variants(input);
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
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
        })
        .collect();
    assert_eq!(scalars, ["a", "b", "c", "d"]);
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    // outer seq opened once; inner seq opened once for the `- - b` entry.
    assert_eq!(seq_starts, 2, "outer and inner SequenceStart");
}

// -----------------------------------------------------------------------
// Group D: Sequence termination
// -----------------------------------------------------------------------

#[test]
fn sequence_ends_on_dedent() {
    // Two-space-indented sequence followed by zero-indent scalar.
    let input = "  - foo\n  - bar\nbaz\n";
    let events = event_variants(input);
    // SequenceStart, foo, bar, SequenceEnd must all come before Scalar(baz).
    assert!(
        events.iter().any(|e| matches!(e, Event::SequenceEnd)),
        "SequenceEnd must exist"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "baz")),
        "Scalar(baz) must exist"
    );
    let seq_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::SequenceEnd))
        .unwrap_or(usize::MAX);
    let baz_pos = events
        .iter()
        .position(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "baz"))
        .unwrap_or(usize::MAX);
    assert!(
        seq_end_pos < baz_pos,
        "SequenceEnd must precede Scalar(baz)"
    );
}

#[test]
fn sequence_ends_on_eof_no_trailing_newline() {
    let events = event_variants("- foo");
    let has_start = events
        .iter()
        .any(|e| matches!(e, Event::SequenceStart { .. }));
    let has_end = events.iter().any(|e| matches!(e, Event::SequenceEnd));
    let has_scalar = events
        .iter()
        .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "foo"));
    let has_stream_end = events.iter().any(|e| matches!(e, Event::StreamEnd));
    assert!(has_start, "SequenceStart must be emitted");
    assert!(has_scalar, "Scalar(foo) must be emitted");
    assert!(has_end, "SequenceEnd must be emitted");
    assert!(has_stream_end, "StreamEnd must be emitted");
}

#[test]
fn sequence_ends_on_explicit_document_end_marker() {
    let events = event_variants("- foo\n...\n");
    assert!(
        events.iter().any(|e| matches!(e, Event::SequenceEnd)),
        "SequenceEnd must exist"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::DocumentEnd { explicit: true })),
        "explicit DocumentEnd must exist"
    );
    let seq_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::SequenceEnd))
        .unwrap_or(usize::MAX);
    let doc_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::DocumentEnd { explicit: true }))
        .unwrap_or(usize::MAX);
    assert!(
        seq_end_pos < doc_end_pos,
        "SequenceEnd must precede explicit DocumentEnd"
    );
}

#[test]
fn sequence_ends_on_document_start_marker() {
    let events = event_variants("- foo\n---\n");
    assert!(
        events.iter().any(|e| matches!(e, Event::SequenceEnd)),
        "SequenceEnd must exist"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::DocumentEnd { .. })),
        "DocumentEnd must exist"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::DocumentStart { explicit: true, .. })),
        "second explicit DocumentStart must exist"
    );
    let seq_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::SequenceEnd))
        .unwrap_or(usize::MAX);
    let doc_end_pos = events
        .iter()
        .position(|e| matches!(e, Event::DocumentEnd { .. }))
        .unwrap_or(usize::MAX);
    let doc_start_2_pos = events
        .iter()
        .rposition(|e| matches!(e, Event::DocumentStart { explicit: true, .. }))
        .unwrap_or(0);
    assert!(seq_end_pos < doc_end_pos, "SequenceEnd before DocumentEnd");
    assert!(
        doc_end_pos < doc_start_2_pos,
        "DocumentEnd before second DocumentStart"
    );
}

// -----------------------------------------------------------------------
// Group E: Compact indent / inline rules
// -----------------------------------------------------------------------

#[test]
fn compact_item_content_at_column_two() {
    let events = event_variants("- item\n");
    let scalar = events.iter().find_map(|e| match e {
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
    });
    assert_eq!(
        scalar,
        Some("item"),
        "scalar value must be 'item' without leading space"
    );
}

#[test]
fn inline_nested_sequence_on_same_line() {
    // `- - item\n` — inline nested sequence
    let events = event_variants("- - item\n");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(seq_starts, 2, "two SequenceStart for inline nesting");
    assert_eq!(seq_ends, 2, "two SequenceEnd for inline nesting");
    let scalar = events.iter().find_map(|e| match e {
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
    });
    assert_eq!(scalar, Some("item"));
}

// -----------------------------------------------------------------------
// Group F: Explicit document context
// -----------------------------------------------------------------------

#[test]
fn sequence_in_explicit_document() {
    let events = event_variants("---\n- foo\n- bar\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::SequenceStart {
                anchor: None,
                tag: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "foo".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Scalar {
                value: "bar".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::SequenceEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn sequence_in_explicit_document_with_explicit_end() {
    let events = event_variants("---\n- foo\n...\n");
    let doc_end_explicit = events
        .iter()
        .any(|e| matches!(e, Event::DocumentEnd { explicit: true }));
    let has_seq = events
        .iter()
        .any(|e| matches!(e, Event::SequenceStart { .. }));
    assert!(has_seq, "SequenceStart must be present");
    assert!(doc_end_explicit, "explicit DocumentEnd must be present");
}

// -----------------------------------------------------------------------
// Group G: Scalar style variety
// -----------------------------------------------------------------------

// Quoted sequence items carry the correct ScalarStyle discriminant.
#[rstest]
// Single-quoted item must emit SingleQuoted style.
#[case::single_quoted_item("- 'hello'\n", "hello", ScalarStyle::SingleQuoted)]
// Double-quoted item must emit DoubleQuoted style.
#[case::double_quoted_item("- \"world\"\n", "world", ScalarStyle::DoubleQuoted)]
fn sequence_quoted_item_emits_correct_scalar_style(
    #[case] input: &str,
    #[case] expected_value: &str,
    #[case] expected_style: ScalarStyle,
) {
    let events = event_variants(input);
    let scalar = events.iter().find_map(|e| match e {
        Event::Scalar { value, style, .. } => Some((value.as_ref(), *style)),
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
    });
    assert_eq!(
        scalar,
        Some((expected_value, expected_style)),
        "sequence item must carry expected scalar style for input: {input:?}"
    );
}

#[test]
fn sequence_with_mixed_scalar_styles() {
    let input = "- plain\n- 'single'\n- \"double\"\n";
    let events = event_variants(input);
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Scalar { value, style, .. } => Some((value.as_ref(), *style)),
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
        })
        .collect();
    assert_eq!(
        scalars,
        [
            ("plain", ScalarStyle::Plain),
            ("single", ScalarStyle::SingleQuoted),
            ("double", ScalarStyle::DoubleQuoted),
        ]
    );
}

// -----------------------------------------------------------------------
// Group H: Non-sequence disambiguation
// -----------------------------------------------------------------------

#[test]
fn dash_without_space_is_plain_scalar_not_sequence() {
    let events = event_variants("-foo\n");
    let has_seq = events
        .iter()
        .any(|e| matches!(e, Event::SequenceStart { .. }));
    assert!(!has_seq, "'-foo' must not be parsed as a sequence");
    let has_scalar = events.iter().any(|e| matches!(e, Event::Scalar { .. }));
    assert!(has_scalar, "'-foo' must be parsed as a plain scalar");
}

#[test]
fn double_dash_is_plain_scalar_not_sequence() {
    let events = event_variants("--foo\n");
    let has_seq = events
        .iter()
        .any(|e| matches!(e, Event::SequenceStart { .. }));
    assert!(!has_seq, "'--foo' must not be parsed as a sequence");
}

// -----------------------------------------------------------------------
// Group I: Depth and stack safety
// -----------------------------------------------------------------------

#[test]
fn ten_level_nested_sequence() {
    // 10 dashes inline: `- - - - - - - - - - deep\n`
    let input = "- - - - - - - - - - deep\n";
    let events = event_variants(input);
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    let seq_ends = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceEnd))
        .count();
    assert_eq!(seq_starts, 10, "10 SequenceStart events for 10 levels");
    assert_eq!(seq_ends, 10, "10 SequenceEnd events");
    let has_scalar = events
        .iter()
        .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "deep"));
    assert!(has_scalar, "Scalar('deep') must be present");
}

#[test]
fn pathologically_deep_sequence_returns_error_not_panic() {
    // Construct input exceeding MAX_COLLECTION_DEPTH levels.
    // Each level is `- ` (2 bytes) followed by `val\n`.
    let depth = MAX_COLLECTION_DEPTH + 1;
    let input = "- ".repeat(depth) + "val\n";
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        has_error,
        "input with depth {depth} must produce an Err (limit is {MAX_COLLECTION_DEPTH})",
    );
}

#[test]
fn depth_limit_boundary_succeeds() {
    // Exactly MAX_COLLECTION_DEPTH levels of multiline nesting must succeed.
    // Build: each level on its own line with increasing indent.
    // Level 0: `- ` at col 0 → item at col 2
    // Level 1: `  - ` at col 2 → item at col 4
    // ...
    // Level n: `  `*n `- ` at col 2n → item at col 2n+2
    // Final level: `  `*MAX_COLLECTION_DEPTH + `val`
    // Build MAX_COLLECTION_DEPTH lines with increasing indent.
    // Line i has 2*i leading spaces then `-\n` (empty item).
    // The last line has `- val` instead.
    let mut input = String::new();
    for i in 0..MAX_COLLECTION_DEPTH - 1 {
        input.push_str(&"  ".repeat(i));
        input.push_str("-\n");
    }
    input.push_str(&"  ".repeat(MAX_COLLECTION_DEPTH - 1));
    input.push_str("- val\n");
    let results = parse_to_vec(&input);
    let has_error = results.iter().any(Result::is_err);
    assert!(
        !has_error,
        "exactly {MAX_COLLECTION_DEPTH} levels must succeed (no error)",
    );
}

// -----------------------------------------------------------------------
// Group J: Span correctness
// -----------------------------------------------------------------------

#[test]
fn zero_indent_sequence_scalar_span_points_at_value() {
    // Input: "- foo\n"
    // byte 0: '-'  byte 1: ' '  byte 2-4: "foo"  byte 5: '\n'
    // Scalar("foo") must start at byte 2, column 2.
    let results = parse_to_vec("- foo\n");
    let foo_span = results.iter().find_map(|r| match r {
        Ok((Event::Scalar { value, .. }, span)) if value.as_ref() == "foo" => Some(*span),
        _ => None,
    });
    let foo_span = foo_span.unwrap_or_else(|| unreachable!("foo scalar must exist"));
    assert_eq!(foo_span.start.byte_offset, 2, "scalar must start at byte 2");
    assert_eq!(foo_span.start.column, 2, "scalar must start at column 2");
}

#[test]
fn zero_indent_sequence_start_span_points_at_dash() {
    // Input: "- foo\n"
    // SequenceStart must anchor at the '-' indicator: byte 0, column 0.
    let results = parse_to_vec("- foo\n");
    let seq_start_span = results.iter().find_map(|r| match r {
        Ok((Event::SequenceStart { .. }, span)) => Some(*span),
        _ => None,
    });
    let seq_start_span = seq_start_span.unwrap_or_else(|| unreachable!("SequenceStart must exist"));
    assert_eq!(
        seq_start_span.start.byte_offset, 0,
        "SequenceStart must anchor at byte 0"
    );
    assert_eq!(
        seq_start_span.start.column, 0,
        "SequenceStart must anchor at column 0"
    );
}

#[test]
fn indented_sequence_scalar_span_points_at_value() {
    // Input: "  - foo\n"
    // byte 0-1: ' '  byte 2: '-'  byte 3: ' '  byte 4-6: "foo"  byte 7: '\n'
    // Scalar("foo") must start at byte 4, column 4.
    let results = parse_to_vec("  - foo\n");
    let foo_span = results.iter().find_map(|r| match r {
        Ok((Event::Scalar { value, .. }, span)) if value.as_ref() == "foo" => Some(*span),
        _ => None,
    });
    let foo_span = foo_span.unwrap_or_else(|| unreachable!("foo scalar must exist"));
    assert_eq!(
        foo_span.start.byte_offset, 4,
        "indented scalar must start at byte 4"
    );
    assert_eq!(
        foo_span.start.column, 4,
        "indented scalar must start at column 4"
    );
}

#[test]
fn indented_sequence_start_span_points_at_dash() {
    // Input: "  - foo\n"
    // SequenceStart must anchor at the '-' indicator: byte 2, column 2.
    let results = parse_to_vec("  - foo\n");
    let seq_start_span = results.iter().find_map(|r| match r {
        Ok((Event::SequenceStart { .. }, span)) => Some(*span),
        _ => None,
    });
    let seq_start_span = seq_start_span.unwrap_or_else(|| unreachable!("SequenceStart must exist"));
    assert_eq!(
        seq_start_span.start.byte_offset, 2,
        "SequenceStart must anchor at the dash (byte 2)"
    );
    assert_eq!(
        seq_start_span.start.column, 2,
        "SequenceStart must anchor at the dash (column 2)"
    );
}

#[test]
fn nested_indented_sequence_scalar_span_points_at_value() {
    // Input: "  - - inner\n"
    // byte 0-1: ' '  byte 2: '-'  byte 3: ' '  byte 4: '-'  byte 5: ' '
    // byte 6-10: "inner"  byte 11: '\n'
    // Scalar("inner") must start at byte 6, column 6.
    let results = parse_to_vec("  - - inner\n");
    let inner_span = results.iter().find_map(|r| match r {
        Ok((Event::Scalar { value, .. }, span)) if value.as_ref() == "inner" => Some(*span),
        _ => None,
    });
    let inner_span = inner_span.unwrap_or_else(|| unreachable!("inner scalar must exist"));
    assert_eq!(
        inner_span.start.byte_offset, 6,
        "nested indented scalar must start at byte 6"
    );
    assert_eq!(
        inner_span.start.column, 6,
        "nested indented scalar must start at column 6"
    );
}

#[test]
fn nested_indented_sequence_inner_start_span_points_at_dash() {
    // Input: "  - - inner\n"
    // Outer SequenceStart: byte 2 (the first `-`), column 2.
    // Inner SequenceStart: byte 4 (the second `-`), column 4.
    let results = parse_to_vec("  - - inner\n");
    let seq_starts: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            Ok((Event::SequenceStart { .. }, span)) => Some(*span),
            _ => None,
        })
        .collect();
    assert_eq!(seq_starts.len(), 2, "exactly 2 SequenceStart events");
    if let [outer, inner] = seq_starts.as_slice() {
        assert_eq!(
            outer.start.byte_offset, 2,
            "outer SequenceStart must be at byte 2"
        );
        assert_eq!(
            outer.start.column, 2,
            "outer SequenceStart must be at column 2"
        );
        assert_eq!(
            inner.start.byte_offset, 4,
            "inner SequenceStart must be at byte 4"
        );
        assert_eq!(
            inner.start.column, 4,
            "inner SequenceStart must be at column 4"
        );
    }
}

#[test]
fn inline_nested_sequence_scalar_span_points_at_value() {
    // Input: "- - inner\n"
    // byte 0: '-'  byte 1: ' '  byte 2: '-'  byte 3: ' '  byte 4-8: "inner"
    // Scalar("inner") must start at byte 4, column 4.
    let results = parse_to_vec("- - inner\n");
    let inner_span = results.iter().find_map(|r| match r {
        Ok((Event::Scalar { value, .. }, span)) if value.as_ref() == "inner" => Some(*span),
        _ => None,
    });
    let inner_span = inner_span.unwrap_or_else(|| unreachable!("inner scalar must exist"));
    assert_eq!(
        inner_span.start.byte_offset, 4,
        "inline nested scalar must start at byte 4"
    );
    assert_eq!(
        inner_span.start.column, 4,
        "inline nested scalar must start at column 4"
    );
}

#[test]
fn multiline_indented_sequence_second_entry_scalar_span() {
    // Input: "  - foo\n  - bar\n"
    // Line 1 bytes 0-7:  "  - foo\n"  → "foo" at byte 4, col 4, line 1
    // Line 2 bytes 8-15: "  - bar\n"  → "bar" at byte 12, col 4, line 2
    let results = parse_to_vec("  - foo\n  - bar\n");
    let scalars: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            Ok((Event::Scalar { value, .. }, span)) => Some((value.as_ref().to_owned(), *span)),
            _ => None,
        })
        .collect();
    assert_eq!(scalars.len(), 2, "exactly 2 scalars");
    if let [(foo_val, foo_span), (bar_val, bar_span)] = scalars.as_slice() {
        assert_eq!(foo_val, "foo");
        assert_eq!(foo_span.start.byte_offset, 4, "foo at byte 4");
        assert_eq!(foo_span.start.column, 4, "foo at column 4");
        assert_eq!(foo_span.start.line, 1, "foo on line 1");
        assert_eq!(bar_val, "bar");
        assert_eq!(bar_span.start.byte_offset, 12, "bar at byte 12");
        assert_eq!(bar_span.start.column, 4, "bar at column 4");
        assert_eq!(bar_span.start.line, 2, "bar on line 2");
    }
}

// -----------------------------------------------------------------------
// Group K: Plain scalar fast path
// -----------------------------------------------------------------------

#[test]
fn fast_path_plain_scalar_emits_scalar_event() {
    // Canonical fast-path input: single plain scalar inline after dash.
    let events = event_variants("- hello\n");
    let has_scalar = events.iter().any(|e| {
        matches!(
            e,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            } if value.as_ref() == "hello"
        )
    });
    assert!(has_scalar, "expected Scalar(hello, Plain, None, None)");
}

#[test]
fn fast_path_plain_scalar_value_is_borrowed() {
    // Fast path must return Cow::Borrowed — zero allocation.
    let results = parse_to_vec("- hello\n");
    let value = results.iter().find_map(|r| match r {
        Ok((Event::Scalar { value, .. }, _)) if value.as_ref() == "hello" => Some(value.clone()),
        _ => None,
    });
    let value = value.unwrap_or_else(|| unreachable!("scalar must exist"));
    assert!(
        matches!(value, std::borrow::Cow::Borrowed(_)),
        "fast path must return Cow::Borrowed"
    );
}

#[test]
fn fast_path_plain_scalar_with_trailing_comment() {
    // Comment after scalar: the fast path must strip the comment from the value.
    let events = event_variants("- value # comment\n");
    let scalar = events.iter().find(|e| matches!(e, Event::Scalar { .. }));
    assert!(scalar.is_some(), "scalar must be emitted");
    if let Some(Event::Scalar {
        value,
        style,
        anchor,
        tag,
    }) = scalar
    {
        assert_eq!(value.as_ref(), "value");
        assert!(matches!(style, ScalarStyle::Plain));
        assert!(anchor.is_none());
        assert!(tag.is_none());
    }
}

#[test]
fn fast_path_plain_scalar_colon_no_space() {
    // Colon without following space must not trip guard 4 and must be in value.
    let events = event_variants("- value_with:colon_no_space\n");
    let scalar = events.iter().find(|e| matches!(e, Event::Scalar { .. }));
    assert!(scalar.is_some(), "scalar must be emitted");
    if let Some(Event::Scalar { value, style, .. }) = scalar {
        assert_eq!(value.as_ref(), "value_with:colon_no_space");
        assert!(matches!(style, ScalarStyle::Plain));
    }
}

#[test]
fn fast_path_plain_scalar_span_points_at_value() {
    // "- foo\n": scalar starts at byte 2, column 2 (same as slow path).
    let results = parse_to_vec("- foo\n");
    let span = results.iter().find_map(|r| match r {
        Ok((Event::Scalar { value, .. }, span)) if value.as_ref() == "foo" => Some(*span),
        _ => None,
    });
    let span = span.unwrap_or_else(|| unreachable!("scalar must exist"));
    assert_eq!(span.start.byte_offset, 2, "scalar must start at byte 2");
    assert_eq!(span.start.column, 2, "scalar must start at column 2");
}

#[test]
fn fast_path_plain_scalar_indented_span() {
    // "  - bar\n": scalar starts at byte 4, column 4.
    let results = parse_to_vec("  - bar\n");
    let span = results.iter().find_map(|r| match r {
        Ok((Event::Scalar { value, .. }, span)) if value.as_ref() == "bar" => Some(*span),
        _ => None,
    });
    let span = span.unwrap_or_else(|| unreachable!("scalar must exist"));
    assert_eq!(span.start.byte_offset, 4, "scalar must start at byte 4");
    assert_eq!(span.start.column, 4, "scalar must start at column 4");
}

#[test]
fn fast_path_multiple_entries_in_sequence() {
    // Three consecutive plain scalar entries — verify state resets between entries.
    let events = event_variants("- a\n- b\n- c\n");
    let values: Vec<&str> = scalar_values(&events);
    assert_eq!(values, ["a", "b", "c"]);
    for e in events.iter().filter(|e| matches!(e, Event::Scalar { .. })) {
        if let Event::Scalar {
            style, anchor, tag, ..
        } = e
        {
            assert!(matches!(style, ScalarStyle::Plain));
            assert!(anchor.is_none());
            assert!(tag.is_none());
        }
    }
}

#[test]
fn fast_path_skipped_anchor_prefix() {
    // Guard 1: pending_anchor triggers fallthrough — anchor must appear on scalar.
    let events = event_variants("- &anchor value\n");
    let scalar = events.iter().find(|e| matches!(e, Event::Scalar { .. }));
    assert!(scalar.is_some(), "scalar must be emitted");
    if let Some(Event::Scalar { value, anchor, .. }) = scalar {
        assert_eq!(value.as_ref(), "value");
        assert_eq!(anchor.as_deref(), Some("anchor"));
    }
}

#[test]
fn fast_path_skipped_tag_prefix() {
    // Guard 2: pending_tag triggers fallthrough — tag must appear on scalar.
    let events = event_variants("- !!str value\n");
    let scalar = events.iter().find(|e| matches!(e, Event::Scalar { .. }));
    assert!(scalar.is_some(), "scalar must be emitted");
    if let Some(Event::Scalar { value, tag, .. }) = scalar {
        assert_eq!(value.as_ref(), "value");
        assert!(tag.is_some(), "tag must be present");
    }
}

/// Guard 3: leading indicator characters — cases that still emit a Scalar with correct value/style.
#[rstest]
#[case::single_quoted("- 'quoted'\n", "quoted", ScalarStyle::SingleQuoted)]
#[case::double_quoted("- \"quoted\"\n", "quoted", ScalarStyle::DoubleQuoted)]
#[case::anchor_sigil("- &anc val\n", "val", ScalarStyle::Plain)]
#[case::tag_sigil("- !tag val\n", "val", ScalarStyle::Plain)]
fn fast_path_skipped_indicator_emits_correct_scalar(
    #[case] input: &str,
    #[case] expected_value: &str,
    #[case] expected_style: ScalarStyle,
) {
    let events = event_variants(input);
    let scalar = events
        .iter()
        .find(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == expected_value));
    assert!(
        scalar.is_some(),
        "expected Scalar({expected_value}) in {input:?}"
    );
    if let Some(Event::Scalar { style, .. }) = scalar {
        assert_eq!(*style, expected_style);
    }
}

/// Guard 3: leading indicator characters — cases that do NOT emit a plain scalar at the item position.
#[rstest]
#[case::pipe_block_scalar("- |\n  hello\n")]
#[case::gt_folded_scalar("- >\n  hello\n")]
#[case::flow_sequence("- [a, b]\n")]
#[case::flow_mapping("- {a: b}\n")]
#[case::alias_sigil("- *ref\n")]
#[case::explicit_dash_seq("- - item\n")]
fn fast_path_skipped_indicator_emits_noscalar(#[case] input: &str) {
    // Parsing must not fail for these inputs (fallthrough must work correctly).
    let results = parse_to_vec(input);
    let has_err = results.iter().any(Result::is_err);
    assert!(!has_err, "no parse error expected for {input:?}");
}

#[test]
fn fast_path_skipped_value_indicator_colon_space() {
    // Guard 4: colon-space triggers fallthrough — inline mapping must be parsed.
    let events = event_variants("- key: value\n");
    let has_mapping_start = events
        .iter()
        .any(|e| matches!(e, Event::MappingStart { .. }));
    assert!(
        has_mapping_start,
        "MappingStart must be emitted for inline mapping"
    );
    let scalars = scalar_values(&events);
    assert!(scalars.contains(&"key"), "key scalar must be present");
    assert!(scalars.contains(&"value"), "value scalar must be present");
}

#[test]
fn fast_path_skipped_bare_dash() {
    // No inline content: consume_sequence_dash returns had_inline=false,
    // the fast path is not entered, and an empty plain scalar is emitted.
    let events = event_variants("-\n");
    let scalar = events.iter().find(|e| matches!(e, Event::Scalar { .. }));
    assert!(
        scalar.is_some(),
        "empty scalar must be emitted for bare dash"
    );
    if let Some(Event::Scalar { value, style, .. }) = scalar {
        assert_eq!(value.as_ref(), "", "value must be empty");
        assert!(matches!(style, ScalarStyle::Plain));
    }
}

#[test]
fn fast_path_suffix_nul_error_propagated() {
    // NUL in trailing comment must produce an error (not silently accepted).
    let input = "- value # comment\x00\n";
    assert!(
        has_error(input),
        "NUL in trailing comment must produce an error"
    );
}

#[test]
fn fast_path_span_matches_slow_path_for_all_plain_cases() {
    // Regression guard: fast-path span must equal slow-path span.
    // For "- X..." the scalar always starts at byte (leading_spaces + 2), col same.
    for (input, expected_byte_offset, expected_col) in [
        ("- a\n", 2usize, 2usize),
        ("- foo\n", 2, 2),
        ("  - bar\n", 4, 4),
        ("- hello world\n", 2, 2),
    ] {
        let results = parse_to_vec(input);
        let span = results.iter().find_map(|r| match r {
            Ok((Event::Scalar { .. }, span)) => Some(*span),
            _ => None,
        });
        let span = span.unwrap_or_else(|| unreachable!("scalar must exist in {input:?}"));
        assert_eq!(
            span.start.byte_offset, expected_byte_offset,
            "byte_offset mismatch for {input:?}"
        );
        assert_eq!(
            span.start.column, expected_col,
            "column mismatch for {input:?}"
        );
    }
}
