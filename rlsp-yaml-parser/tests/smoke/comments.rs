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
                meta: None,
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
                meta: None,
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
                meta: None,
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
                meta: None,
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
                meta: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "key".into(),
                style: ScalarStyle::Plain,
                meta: None,
            },
            Event::Scalar {
                value: "value".into(),
                style: ScalarStyle::Plain,
                meta: None,
            },
            Event::Comment {
                text: " inline comment"
            },
            Event::Scalar {
                value: "key2".into(),
                style: ScalarStyle::Plain,
                meta: None,
            },
            Event::Scalar {
                value: "val2".into(),
                style: ScalarStyle::Plain,
                meta: None,
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
                meta: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "a".into(),
                style: ScalarStyle::Plain,
                meta: None,
            },
            Event::Comment {
                text: " between items"
            },
            Event::Scalar {
                value: "b".into(),
                style: ScalarStyle::Plain,
                meta: None,
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
                meta: None,
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
                meta: None,
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
    let input = "# hello\n";
    let results: Vec<_> = parse_events(input).collect();
    let comment_span = results.iter().find_map(|r| {
        if let Ok((Event::Comment { .. }, span)) = r {
            Some(*span)
        } else {
            None
        }
    });
    assert!(comment_span.is_some(), "expected Comment span");
    if let Some(span) = comment_span {
        assert_eq!(span.start, 0, "span start at byte 0 (the `#`)");
        assert_eq!(line_col(input, span.start).0, 1, "on line 1");
        assert_eq!(line_col(input, span.start).1, 0, "at column 0 (0-based)");
        // end should be at the last byte of text " hello" (6 bytes after `#`)
        assert_eq!(
            span.end, 7,
            "span end covers `# hello` (7 bytes, newline excluded)"
        );
    }
}

// G-2: Trailing comment span starts at `#` on the same line as the scalar.
#[test]
fn trailing_comment_span_starts_at_hash_on_scalar_line() {
    // "foo # bar\n" — `#` is at byte 4.
    let input = "foo # bar\n";
    let results: Vec<_> = parse_events(input).collect();
    let comment_span = results.iter().find_map(|r| {
        if let Ok((Event::Comment { .. }, span)) = r {
            Some(*span)
        } else {
            None
        }
    });
    assert!(comment_span.is_some(), "expected trailing Comment span");
    if let Some(span) = comment_span {
        assert_eq!(span.start, 4, "span start at byte 4 (the `#`)");
        assert_eq!(line_col(input, span.start).0, 1, "on line 1");
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
                meta: None,
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
                meta: None,
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
                meta: None,
                style: CollectionStyle::Block,
            },
            Event::Scalar {
                value: "item".into(),
                style: ScalarStyle::Plain,
                meta: None,
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
                meta: None,
                style: CollectionStyle::Flow,
            },
            Event::Scalar {
                value: "k".into(),
                style: ScalarStyle::Plain,
                meta: None,
            },
            Event::Scalar {
                value: "v".into(),
                style: ScalarStyle::Plain,
                meta: None,
            },
            Event::Comment { text: " remark" },
            Event::Scalar {
                value: "w".into(),
                style: ScalarStyle::Plain,
                meta: None,
            },
            Event::Scalar {
                value: "x".into(),
                style: ScalarStyle::Plain,
                meta: None,
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
                meta: None,
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
                meta: None,
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
                meta: None,
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
                meta: None,
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
                meta: None,
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
    let input = "key: val\n# second\n";
    let results: Vec<_> = parse_events(input).collect();
    let comment_span = results.iter().find_map(|r| {
        if let Ok((Event::Comment { .. }, span)) = r {
            Some(*span)
        } else {
            None
        }
    });
    assert!(comment_span.is_some(), "expected a Comment span on line 2");
    if let Some(span) = comment_span {
        assert_eq!(
            line_col(input, span.start).0,
            2,
            "comment must be on line 2"
        );
        assert_eq!(
            line_col(input, span.start).1,
            0,
            "comment starts at column 0"
        );
    }
}

// -----------------------------------------------------------------------
// Group C — A comment line ends a plain scalar (YAML 1.2.2 [130], [134])
// -----------------------------------------------------------------------

fn comment_texts<'a>(events: &[Event<'a>]) -> Vec<&'a str> {
    events
        .iter()
        .filter_map(|e| {
            if let Event::Comment { text } = e {
                Some(*text)
            } else {
                None
            }
        })
        .collect()
}

// A comment line after a plain scalar is a comment, never scalar content:
// the stream carries the scalar values, then the comments, in order.
#[rstest]
#[case::mapping_value("a: v\n  # d\nb: 1\n", &["a", "v", "b", "1"], &[" d"])]
#[case::mapping_value_after_continuation("a:\n  v\n  w\n  # d\nb: 1\n", &["a", "v w", "b", "1"], &[" d"])]
#[case::mapping_value_blank_line_before_comment("a: v\n\n  # d\nb: 1\n", &["a", "v", "b", "1"], &[" d"])]
#[case::mapping_value_tab_before_comment("a: v\n\t# d\nb: 1\n", &["a", "v", "b", "1"], &[" d"])]
#[case::sequence_item("- v\n  # d\n- z\n", &["v", "z"], &[" d"])]
#[case::root_scalar("v\n# d\n", &["v"], &[" d"])]
#[case::root_scalar_indented_comment("v\n  # d\n", &["v"], &[" d"])]
#[case::explicit_root_scalar("---\nv\n  # d\n", &["v"], &[" d"])]
#[case::explicit_root_scalar_comment_at_column_zero("---\nv\n# d\n", &["v"], &[" d"])]
#[case::comment_without_space("a: v\n  #d\nb: 1\n", &["a", "v", "b", "1"], &["d"])]
#[case::comment_without_text("a: v\n  #\nb: 1\n", &["a", "v", "b", "1"], &[""])]
#[case::comment_at_eof_without_newline("a: v\n  # d", &["a", "v"], &[" d"])]
#[case::empty_comment_at_eof_without_newline("a: v\n  #", &["a", "v"], &[""])]
#[case::crlf_mapping_value("a: v\r\n  # d\r\nb: 1\r\n", &["a", "v", "b", "1"], &[" d"])]
#[case::crlf_sequence_item("- v\r\n  # d\r\n- z\r\n", &["v", "z"], &[" d"])]
#[case::tab_after_indent("x:\n  a: v\n  \t# d\n  b: 1\n", &["x", "a", "v", "b", "1"], &[" d"])]
#[case::two_comment_lines("a: v\n  # d1\n  # d2\nb: 1\n", &["a", "v", "b", "1"], &[" d1", " d2"])]
#[case::several_blank_lines("a: v\n\n\n  # d\n\nb: 1\n", &["a", "v", "b", "1"], &[" d"])]
#[case::comment_indented_past_scalar("- v\n      # d\n- z\n", &["v", "z"], &[" d"])]
#[case::nested_sequence("- - v\n    # d\n  - w\n", &["v", "w"], &[" d"])]
#[case::mapping_in_sequence_item("- a: v\n    # d\n  b: 1\n", &["a", "v", "b", "1"], &[" d"])]
#[case::sequence_under_mapping("k:\n  - v\n    # d\n  - z\n", &["k", "v", "z"], &[" d"])]
#[case::nested_mapping("a:\n  b: v\n    # d\n  c: 1\n", &["a", "b", "v", "c", "1"], &[" d"])]
#[case::comment_at_column_zero_inside_nested_mapping("a:\n  b: v\n# d\n  c: 1\n", &["a", "b", "v", "c", "1"], &[" d"])]
#[case::document_end_marker("v\n  # d\n...\n", &["v"], &[" d"])]
#[case::document_start_marker("a: v\n  # d\n---\nb: 1\n", &["a", "v", "b", "1"], &[" d"])]
#[case::comment_mentions_marker("a: v\n  # ---\nb: 1\n", &["a", "v", "b", "1"], &[" ---"])]
#[case::hash_without_space_in_continuation_line("a:\n  v\n  x#y\nb: 1\n", &["a", "v x#y", "b", "1"], &[])]
#[case::nbsp_before_hash_is_content("a:\n  v\n  \u{a0}#y\nb: 1\n", &["a", "v \u{a0}#y", "b", "1"], &[])]
fn comment_line_after_plain_scalar_is_a_comment(
    #[case] input: &str,
    #[case] scalars: &[&str],
    #[case] comments: &[&str],
) {
    let events = event_variants(input);
    assert_eq!(scalar_values(&events), scalars);
    assert_eq!(comment_texts(&events), comments);
}

// Moving the comment to column 0 must not change the stream: an indented
// comment line ends the scalar exactly like one at the left margin.
#[rstest]
#[case::mapping_value("a: v\n  # d\nb: 1\n", "a: v\n# d\nb: 1\n")]
#[case::continuation("a:\n  v\n  w\n  # d\nb: 1\n", "a:\n  v\n  w\n# d\nb: 1\n")]
#[case::blank_line("a: v\n\n  # d\nb: 1\n", "a: v\n\n# d\nb: 1\n")]
#[case::sequence_item("- v\n  # d\n- z\n", "- v\n# d\n- z\n")]
#[case::nested_sequence("k:\n  - v\n    # d\n  - z\n", "k:\n  - v\n# d\n  - z\n")]
#[case::end_of_input("a: v\n  # d\n", "a: v\n# d\n")]
fn indented_comment_line_yields_the_same_events_as_one_at_column_zero(
    #[case] indented: &str,
    #[case] at_margin: &str,
) {
    assert_eq!(event_variants(indented), event_variants(at_margin));
}

// The comment line is not consumed twice, and the scalar ends before the
// blank lines that precede the comment.
#[test]
fn comment_line_after_blank_line_keeps_scalar_span_and_comment_position() {
    let input = "a: v\n\n  # d\nb: 1\n";
    let results: Vec<_> = parse_events(input).map(Result::unwrap).collect();
    let scalar_span = results
        .iter()
        .find_map(|(e, s)| matches!(e, Event::Scalar { value, .. } if value == "v").then_some(*s));
    let comment_span = results
        .iter()
        .find_map(|(e, s)| matches!(e, Event::Comment { .. }).then_some(*s));
    let scalar_span = scalar_span.expect("scalar v");
    let comment_span = comment_span.expect("comment");
    assert_eq!((scalar_span.start, scalar_span.end), (3, 4));
    assert_eq!(line_col(input, comment_span.start), (3, 2));
}

// An indented comment line after a plain scalar is validated like one at the
// left margin: same error and message, same line, column shifted by the
// indentation.
#[rstest]
#[case::nul("a: v\n  # x\0y\nb: 1\n", "a: v\n# x\0y\nb: 1\n")]
#[case::bel("a: v\n  # x\x07y\nb: 1\n", "a: v\n# x\x07y\nb: 1\n")]
#[case::del("a: v\n  # x\x7fy\nb: 1\n", "a: v\n# x\x7fy\nb: 1\n")]
#[case::c1_control("a: v\n  # x\u{80}y\nb: 1\n", "a: v\n# x\u{80}y\nb: 1\n")]
#[case::fffe("a: v\n  # x\u{fffe}y\nb: 1\n", "a: v\n# x\u{fffe}y\nb: 1\n")]
fn invalid_character_in_indented_comment_line_is_reported_like_at_margin(
    #[case] indented: &str,
    #[case] at_margin: &str,
) {
    let indented_err = parse_events(indented).find_map(Result::err).unwrap();
    let margin_err = parse_events(at_margin).find_map(Result::err).unwrap();
    assert_eq!(indented_err.kind, margin_err.kind);
    assert_eq!(indented_err.message, margin_err.message);
    assert_eq!(indented_err.pos.line, margin_err.pos.line);
    assert_eq!(indented_err.pos.column, margin_err.pos.column + 2);
    assert_eq!(indented_err.pos.byte_offset, margin_err.pos.byte_offset + 2);
}

// The length limit of a standalone comment applies to an indented comment
// line after a plain scalar, which used to be folded into the value.
#[test]
fn indented_comment_line_over_limit_after_plain_scalar_returns_error() {
    let body = "x".repeat(MAX_COMMENT_LEN + 1);
    assert!(has_error(&format!("a: v\n  #{body}\nb: 1\n")));
    assert!(has_error(&format!("a: v\n#{body}\nb: 1\n")));
}

#[test]
fn indented_comment_line_at_limit_after_plain_scalar_is_accepted() {
    let body = "x".repeat(MAX_COMMENT_LEN);
    assert!(!has_error(&format!("a: v\n  #{body}\nb: 1\n")));
}
