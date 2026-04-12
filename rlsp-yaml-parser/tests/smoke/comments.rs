use rstest::rstest;

use super::*;

// -----------------------------------------------------------------------
// Group A — Standalone comment lines (stream level)
// -----------------------------------------------------------------------

// A-1, A-3: Single standalone comment → [StreamStart, Comment { text }, StreamEnd].
#[rstest]
// A-1: Single comment line with leading space — body includes the space.
#[case::single_standalone_comment_with_space("# hello world\n", " hello world")]
// A-3: Comment body with no leading space is preserved as-is.
#[case::comment_body_no_leading_space_preserved("#nospace\n", "nospace")]
fn single_standalone_comment_emits_stream_comment_stream(
    #[case] input: &str,
    #[case] expected_text: &str,
) {
    let events = event_variants(input);
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment {
                text: expected_text
            },
            Event::StreamEnd,
        ]
    );
}

// A-2: Multiple standalone comment lines each produce one Comment event.
#[test]
fn multiple_standalone_comments_each_emit_one_event() {
    let events = event_variants("# first\n# second\n# third\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " first" },
            Event::Comment { text: " second" },
            Event::Comment { text: " third" },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group B — Comments around document markers
// -----------------------------------------------------------------------

// B-1: Comment before `---` is emitted before DocumentStart.
#[test]
fn comment_before_explicit_doc_start_emitted_before_document_start() {
    let events = event_variants("# preamble\n---\nvalue\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " preamble" },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "value".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// B-2: Comment after `---` (same line) — trailing comment on marker line.
#[test]
fn comment_after_doc_start_marker_on_same_line() {
    let events = event_variants("--- # marker comment\nvalue\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Comment {
                text: " marker comment"
            },
            Event::Scalar {
                value: "value".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// B-3: Comment after the second `---` marker (same document boundary).
//      The comment appears inside the second document — after DocumentStart.
#[test]
fn comment_after_second_doc_start_marker_is_inside_second_document() {
    let events = event_variants("---\na\n---\n# between\nb\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "a".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Comment { text: " between" },
            Event::Scalar {
                value: "b".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// B-4: Comment inside an otherwise-empty explicit document.
#[test]
fn comment_inside_empty_explicit_document() {
    let events = event_variants("---\n# inside\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Comment { text: " inside" },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group C — Comments inside documents
// -----------------------------------------------------------------------

// C-1: Comment on its own line inside a block mapping.
#[test]
fn comment_line_inside_block_mapping() {
    let events = event_variants("key: value\n# inline comment\nkey2: val2\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::MappingStart {
                anchor: None,
                tag: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "key".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Scalar {
                value: "value".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Comment {
                text: " inline comment"
            },
            Event::Scalar {
                value: "key2".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Scalar {
                value: "val2".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::MappingEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// C-2: Comment on its own line inside a block sequence.
#[test]
fn comment_line_inside_block_sequence() {
    let events = event_variants("- a\n# between items\n- b\n");
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
                value: "a".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Comment {
                text: " between items"
            },
            Event::Scalar {
                value: "b".into(),
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

// C-3: Blank line followed by comment inside block mapping — blank is
//      skipped, comment is emitted.
#[test]
fn blank_line_then_comment_inside_block_mapping() {
    let events = event_variants("k: v\n\n# after blank\nk2: v2\n");
    let comment_count = events
        .iter()
        .filter(|e| matches!(e, Event::Comment { .. }))
        .count();
    assert_eq!(comment_count, 1, "exactly one Comment event expected");
    let texts: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Comment { text } = e {
                Some(*text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(texts, [" after blank"]);
}

// -----------------------------------------------------------------------
// Group D — Trailing (inline) comments after scalars
// -----------------------------------------------------------------------

// D-1: Plain scalar followed by inline comment — scalar value trimmed,
//      Comment event emitted after the scalar.
#[test]
fn trailing_comment_after_plain_scalar() {
    let events = event_variants("foo # trailing\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Comment { text: " trailing" },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// D-2: `#` immediately after non-whitespace is NOT a comment (part of
//      the value), so no Comment event.
#[test]
fn hash_without_preceding_space_is_not_a_comment() {
    let events = event_variants("foo#bar\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo#bar".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
    // No Comment events.
    assert!(
        events.iter().all(|e| !matches!(e, Event::Comment { .. })),
        "unexpected Comment event when # has no preceding space"
    );
}

// D-3: Trailing comment on a mapping value.
#[test]
fn trailing_comment_after_mapping_value() {
    let events = event_variants("key: val # note\n");
    let comment_idx = events
        .iter()
        .position(|e| matches!(e, Event::Comment { .. }));
    let val_idx = events
        .iter()
        .position(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "val"));
    assert!(comment_idx.is_some(), "expected a Comment event");
    assert!(val_idx.is_some(), "value scalar expected");
    if let (Some(ci), Some(vi)) = (comment_idx, val_idx) {
        if let Some(Event::Comment { text }) = events.get(ci) {
            assert_eq!(*text, " note");
        }
        assert!(vi < ci, "Comment must follow the value scalar");
    }
}

// D-4: Trailing comment text includes everything after `#` to end-of-line
//      (leading space preserved).
#[test]
fn trailing_comment_leading_space_preserved() {
    let events = event_variants("x #  two spaces\n");
    let text = events.iter().find_map(|e| {
        if let Event::Comment { text } = e {
            Some(*text)
        } else {
            None
        }
    });
    assert_eq!(
        text,
        Some("  two spaces"),
        "leading spaces after # preserved"
    );
}

// -----------------------------------------------------------------------
// Group E — Comments inside flow collections
// -----------------------------------------------------------------------

// E-1: Comment inside a flow sequence terminates the current item and
//      consumes the rest of the line.
#[test]
fn comment_inside_flow_sequence() {
    let events = event_variants("[a, # comment\nb]\n");
    // Must contain a Comment event.
    let has_comment = events.iter().any(|e| matches!(e, Event::Comment { .. }));
    assert!(has_comment, "expected Comment event inside flow sequence");
    // Both scalar items must still be present.
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scalars, ["a", "b"], "both scalars must be present");
}

// E-2: Comment inside a flow mapping.
#[test]
fn comment_inside_flow_mapping() {
    let events = event_variants("{k: v # comment\n}\n");
    let has_comment = events.iter().any(|e| matches!(e, Event::Comment { .. }));
    assert!(has_comment, "expected Comment event inside flow mapping");
    // Key and value scalars intact.
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
    assert!(scalars.contains(&"k".to_owned()), "key scalar expected");
    assert!(scalars.contains(&"v".to_owned()), "value scalar expected");
}

// -----------------------------------------------------------------------
// Group F — Length limits
// -----------------------------------------------------------------------

// F-1: Standalone comment at exactly MAX_COMMENT_LEN bytes is accepted.
#[test]
fn standalone_comment_at_limit_accepted() {
    // `#` + MAX_COMMENT_LEN bytes of text
    let body = "x".repeat(MAX_COMMENT_LEN);
    let input = format!("#{body}\n");
    let events = event_variants(&input);
    let texts: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Comment { text } = e {
                Some(*text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(texts.len(), 1, "expected exactly one Comment event");
    if let [text] = texts.as_slice() {
        assert_eq!(text.len(), MAX_COMMENT_LEN);
    }
}

// F-2: Standalone comment exceeding MAX_COMMENT_LEN bytes returns an error.
#[test]
fn standalone_comment_over_limit_returns_error() {
    let body = "x".repeat(MAX_COMMENT_LEN + 1);
    let input = format!("#{body}\n");
    let results: Vec<_> = parse_events(&input).collect();
    let has_error = results.iter().any(Result::is_err);
    assert!(
        has_error,
        "expected an error for comment exceeding MAX_COMMENT_LEN"
    );
}

// F-3: Trailing inline comment has no separate length cap (bounded by
//      line length); it is accepted even when body exceeds MAX_COMMENT_LEN.
#[test]
fn trailing_comment_has_no_separate_length_limit() {
    // Build a line where the trailing comment body is MAX_COMMENT_LEN + 1,
    // but since it's trailing (not standalone), no error is expected.
    let body = "x".repeat(MAX_COMMENT_LEN + 1);
    let input = format!("scalar # {body}\n");
    let results: Vec<_> = parse_events(&input).collect();
    let has_error = results.iter().any(Result::is_err);
    assert!(!has_error, "trailing comment should not be length-limited");
}

// -----------------------------------------------------------------------
// Group G — Span correctness
// -----------------------------------------------------------------------

// G-1: Span of a standalone comment starts at `#` and ends at last text byte.
#[test]
fn standalone_comment_span_starts_at_hash() {
    // "# hello\n" — `#` is at byte 0.
    let results: Vec<_> = parse_events("# hello\n").collect();
    let comment_span = results.iter().find_map(|r| {
        if let Ok((Event::Comment { .. }, span)) = r {
            Some(*span)
        } else {
            None
        }
    });
    assert!(comment_span.is_some(), "expected Comment span");
    if let Some(span) = comment_span {
        assert_eq!(span.start.byte_offset, 0, "span start at byte 0 (the `#`)");
        assert_eq!(span.start.line, 1, "on line 1");
        assert_eq!(span.start.column, 0, "at column 0 (0-based)");
        // end should be at the last byte of text " hello" (6 bytes after `#`)
        assert_eq!(
            span.end.byte_offset, 7,
            "span end covers `# hello` (7 bytes, newline excluded)"
        );
    }
}

// G-2: Trailing comment span starts at `#` on the same line as the scalar.
#[test]
fn trailing_comment_span_starts_at_hash_on_scalar_line() {
    // "foo # bar\n" — `#` is at byte 4.
    let results: Vec<_> = parse_events("foo # bar\n").collect();
    let comment_span = results.iter().find_map(|r| {
        if let Ok((Event::Comment { .. }, span)) = r {
            Some(*span)
        } else {
            None
        }
    });
    assert!(comment_span.is_some(), "expected trailing Comment span");
    if let Some(span) = comment_span {
        assert_eq!(span.start.byte_offset, 4, "span start at byte 4 (the `#`)");
        assert_eq!(span.start.line, 1, "on line 1");
    }
}

#[rstest]
#[case::empty_body("#\n", "")]
#[case::leading_whitespace_preserved("#   triple space\n", "   triple space")]
fn comment_body_text_is_preserved_verbatim(#[case] input: &str, #[case] expected_text: &str) {
    let events = event_variants(input);
    let text = events.iter().find_map(|e| {
        if let Event::Comment { text } = e {
            Some(*text)
        } else {
            None
        }
    });
    assert_eq!(
        text,
        Some(expected_text),
        "comment body text must be preserved verbatim"
    );
}

// -----------------------------------------------------------------------
// Spec #7 — standalone comment line inside explicit document before scalar
// -----------------------------------------------------------------------

#[test]
fn comment_after_doc_start_marker_before_scalar() {
    // "---\n# top comment\nvalue\n"
    // Comment is on its own line inside the explicit document, scalar follows.
    let events = event_variants("---\n# top comment\nvalue\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Comment {
                text: " top comment"
            },
            Event::Scalar {
                value: "value".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #8 — multiple consecutive comments then scalar content
// -----------------------------------------------------------------------

#[test]
fn multiple_comments_then_scalar_all_emitted() {
    // Three comment lines followed by a scalar — all three comments emitted,
    // scalar still parsed correctly.
    let events = event_variants("# first\n# second\n# third\nval\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " first" },
            Event::Comment { text: " second" },
            Event::Comment { text: " third" },
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "val".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #14 — trailing comment after a sequence entry scalar
// -----------------------------------------------------------------------

#[test]
fn trailing_comment_after_sequence_entry() {
    // "- item # note\n" → SequenceStart, Scalar "item", Comment " note", SequenceEnd
    let events = event_variants("- item # note\n");
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
                value: "item".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Comment { text: " note" },
            Event::SequenceEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #16 — flow mapping with entry following comment line
// -----------------------------------------------------------------------

#[test]
fn comment_mid_flow_mapping_second_entry_still_parsed() {
    // "{k: v, # remark\nw: x}\n" — comment in flow mapping; second pair follows.
    let events = event_variants("{k: v, # remark\nw: x}\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::MappingStart {
                anchor: None,
                tag: None,
                style: CollectionStyle::Flow,
            },
            Event::Scalar {
                value: "k".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Scalar {
                value: "v".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Comment { text: " remark" },
            Event::Scalar {
                value: "w".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Scalar {
                value: "x".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::MappingEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #17 — comment between `...` and `---` (BetweenDocs state)
// -----------------------------------------------------------------------

#[test]
fn comment_between_docs_via_dot_dot_dot_marker() {
    // "doc1\n...\n# between docs\n---\ndoc2\n"
    // Comment appears after `...` DocumentEnd and before `---` DocumentStart.
    let events = event_variants("doc1\n...\n# between docs\n---\ndoc2\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "doc1".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: true },
            Event::Comment {
                text: " between docs"
            },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "doc2".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #18 — comment between two fully explicit documents
// -----------------------------------------------------------------------

#[test]
fn comment_between_two_explicit_documents() {
    // "---\na\n...\n# inter-doc comment\n---\nb\n"
    let events = event_variants("---\na\n...\n# inter-doc comment\n---\nb\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "a".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: true },
            Event::Comment {
                text: " inter-doc comment"
            },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "b".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #19 — comment-only input with no trailing newline
// -----------------------------------------------------------------------

#[test]
fn comment_only_no_trailing_newline_emits_comment() {
    // "# no newline" — no `\n` at end; comment must not be silently dropped.
    let events = event_variants("# no newline");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment {
                text: " no newline"
            },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #20 — trailing comment with no trailing newline
// -----------------------------------------------------------------------

#[test]
fn trailing_comment_no_trailing_newline() {
    // "foo # trailing" — no `\n`; both scalar and comment must be emitted.
    let events = event_variants("foo # trailing");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::Comment { text: " trailing" },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Spec #26 — comment span on a later line has correct `start.line`
// -----------------------------------------------------------------------

#[test]
fn comment_on_second_line_span_has_correct_line_number() {
    // "key: val\n# second\n" — comment is on line 2.
    let results: Vec<_> = parse_events("key: val\n# second\n").collect();
    let comment_span = results.iter().find_map(|r| {
        if let Ok((Event::Comment { .. }, span)) = r {
            Some(*span)
        } else {
            None
        }
    });
    assert!(comment_span.is_some(), "expected a Comment span on line 2");
    if let Some(span) = comment_span {
        assert_eq!(span.start.line, 2, "comment must be on line 2");
        assert_eq!(span.start.column, 0, "comment starts at column 0");
    }
}
