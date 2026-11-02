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

// F-3: A trailing comment over MAX_COMMENT_LEN bytes is an error, as a
//      standalone comment is; one at the limit is accepted.
#[rstest]
#[case::plain_scalar("scalar #{}\n")]
#[case::quoted_scalar("a: \"q\" #{}\n")]
#[case::empty_value("k: #{}\n")]
fn trailing_comment_over_limit_returns_error(#[case] template: &str) {
    let at_limit = template.replace("{}", &"x".repeat(MAX_COMMENT_LEN));
    assert!(parse_events(&at_limit).all(|r| r.is_ok()), "at the limit");

    let over_limit = template.replace("{}", &"x".repeat(MAX_COMMENT_LEN + 1));
    assert!(
        parse_events(&over_limit).any(|r| r.is_err()),
        "expected an error for a trailing comment over MAX_COMMENT_LEN"
    );
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

// -----------------------------------------------------------------------
// Group H — A comment is an event at every position it can follow
// (YAML 1.2.2 [75] c-nb-comment-text, [77] s-b-comment, [79] s-l-comments)
// -----------------------------------------------------------------------

/// The events of `input` as short strings, in stream order: `+MAP`/`-MAP`,
/// `+SEQ`/`-SEQ`, `+DOC`/`-DOC` (with `---`/`...` when explicit), `=value`
/// for a scalar, `*name` for an alias and `#text` for a comment.
fn event_shape(input: &str) -> Vec<String> {
    event_variants(input)
        .into_iter()
        .filter_map(|event| match event {
            Event::StreamStart | Event::StreamEnd => None,
            Event::DocumentStart { explicit, .. } => {
                Some(format!("+DOC{}", if explicit { " ---" } else { "" }))
            }
            Event::DocumentEnd { explicit } => {
                Some(format!("-DOC{}", if explicit { " ..." } else { "" }))
            }
            Event::MappingStart { .. } => Some("+MAP".to_owned()),
            Event::MappingEnd => Some("-MAP".to_owned()),
            Event::SequenceStart { .. } => Some("+SEQ".to_owned()),
            Event::SequenceEnd => Some("-SEQ".to_owned()),
            Event::Scalar { value, .. } => Some(format!("={value}")),
            Event::Alias { name } => Some(format!("*{name}")),
            Event::Comment { text } => Some(format!("#{text}")),
        })
        .collect()
}

#[rstest]
#[case::double_quoted_value("a: \"q\" # c\n", &["+DOC", "+MAP", "=a", "=q", "# c", "-MAP", "-DOC"])]
#[case::single_quoted_value("a: 'q' # c\n", &["+DOC", "+MAP", "=a", "=q", "# c", "-MAP", "-DOC"])]
#[case::quoted_sequence_item("- \"q\" # c\n", &["+DOC", "+SEQ", "=q", "# c", "-SEQ", "-DOC"])]
#[case::key_without_value("k: # c\n", &["+DOC", "+MAP", "=k", "# c", "=", "-MAP", "-DOC"])]
#[case::bare_dash_before_mapping(
    "- # c\n  a: 1\n",
    &["+DOC", "+SEQ", "# c", "+MAP", "=a", "=1", "-MAP", "-SEQ", "-DOC"]
)]
#[case::bare_question_mark("? # c\n: v\n", &["+DOC", "+MAP", "# c", "=", "=v", "-MAP", "-DOC"])]
#[case::bare_colon_of_explicit_key(
    "? k\n: # c\n  v\n",
    &["+DOC", "+MAP", "=k", "# c", "=v", "-MAP", "-DOC"]
)]
#[case::anchor_before_mapping(
    "a: &x # c\n  b: 1\n",
    &["+DOC", "+MAP", "=a", "# c", "+MAP", "=b", "=1", "-MAP", "-MAP", "-DOC"]
)]
#[case::tag_before_mapping(
    "a: !t # c\n  b: 1\n",
    &["+DOC", "+MAP", "=a", "# c", "+MAP", "=b", "=1", "-MAP", "-MAP", "-DOC"]
)]
#[case::literal_header("a: | # c\n  t\n", &["+DOC", "+MAP", "=a", "=t\n", "# c", "-MAP", "-DOC"])]
#[case::folded_header("a: > # c\n  t\n", &["+DOC", "+MAP", "=a", "=t\n", "# c", "-MAP", "-DOC"])]
#[case::last_line_of_multi_line_plain_scalar(
    "a:\n  v\n  w # c\n",
    &["+DOC", "+MAP", "=a", "=v w", "# c", "-MAP", "-DOC"]
)]
#[case::scalar_after_document_start("--- v # c\n", &["+DOC ---", "=v", "# c", "-DOC"])]
#[case::yaml_directive(
    "%YAML 1.2 # c\n---\na: 1\n",
    &["# c", "+DOC ---", "+MAP", "=a", "=1", "-MAP", "-DOC"]
)]
#[case::tag_directive(
    "%TAG ! tag:x: # c\n---\na: 1\n",
    &["# c", "+DOC ---", "+MAP", "=a", "=1", "-MAP", "-DOC"]
)]
#[case::reserved_directive(
    "%FOO bar # c\n---\na: 1\n",
    &["# c", "+DOC ---", "+MAP", "=a", "=1", "-MAP", "-DOC"]
)]
#[case::document_end_marker(
    "a: 1\n... # c\n",
    &["+DOC", "+MAP", "=a", "=1", "-MAP", "-DOC ...", "# c"]
)]
#[case::orphan_document_end_marker("... # c\n", &["# c"])]
#[case::orphan_document_end_marker_between_documents(
    "a: 1\n...\n... # c\n--- # d\nb\n",
    &["+DOC", "+MAP", "=a", "=1", "-MAP", "-DOC ...", "# c", "+DOC ---", "# d", "=b", "-DOC"]
)]
#[case::quoted_value_of_quoted_key("\"k\": \"v\" # c\n", &["+DOC", "+MAP", "=k", "=v", "# c", "-MAP", "-DOC"])]
#[case::quoted_scalar_after_document_start("--- \"q\" # c\n", &["+DOC ---", "=q", "# c", "-DOC"])]
#[case::block_scalar_after_document_start("--- | # c\n t\n", &["+DOC ---", "=t\n", "# c", "-DOC"])]
#[case::tag_after_document_start(
    "--- !t # c\n v\n",
    &["+DOC ---", "# c", "=v", "-DOC"]
)]
#[case::property_line_before_scalar("&a # c\nb\n", &["+DOC", "# c", "=b", "-DOC"])]
#[case::alias_item("- *a # c\n", &["+DOC", "+SEQ", "*a", "# c", "-SEQ", "-DOC"])]
#[case::nested_bare_dashes(
    "- - # c\n    x\n",
    &["+DOC", "+SEQ", "+SEQ", "# c", "=x", "-SEQ", "-SEQ", "-DOC"]
)]
#[case::property_of_explicit_key("? !t # c\n: v\n", &["+DOC", "+MAP", "# c", "=", "=v", "-MAP", "-DOC"])]
#[case::empty_comment_body("k: #\n", &["+DOC", "+MAP", "=k", "#", "=", "-MAP", "-DOC"])]
#[case::two_comments_in_source_order(
    "a: \"q\" # c\nb: # d\n  v # e\n",
    &["+DOC", "+MAP", "=a", "=q", "# c", "=b", "# d", "=v", "# e", "-MAP", "-DOC"]
)]
#[case::flow_sequence_inside_block_value(
    "a: [b, # c\n  d] # e\n",
    &["+DOC", "+MAP", "=a", "+SEQ", "=b", "# c", "=d", "-SEQ", "# e", "-MAP", "-DOC"]
)]
fn comment_after_a_node_or_indicator_is_one_event_in_source_order(
    #[case] input: &str,
    #[case] expected: &[&str],
) {
    assert_eq!(event_shape(input), expected, "input: {input:?}");
}

// The span of each comment event starts at its `#` and ends after its text,
// whichever position the comment follows (byte offsets).
#[rstest]
#[case::double_quoted_value("a: \"q\" # c\n")]
#[case::single_quoted_value("a: 'q' # c\n")]
#[case::quoted_sequence_item("- \"q\" # c\n")]
#[case::key_without_value("k: # c\n")]
#[case::bare_dash("- # c\n  a: 1\n")]
#[case::bare_question_mark("? # c\n: v\n")]
#[case::bare_colon("? k\n: # c\n  v\n")]
#[case::anchor("a: &x # c\n  b: 1\n")]
#[case::tag("a: !t # c\n  b: 1\n")]
#[case::literal_header("a: | # c\n  t\n")]
#[case::folded_header("a: > # c\n  t\n")]
#[case::multi_line_plain("a:\n  v\n  w # c\n")]
#[case::document_start("--- v # c\n")]
#[case::yaml_directive("%YAML 1.2 # c\n---\na: 1\n")]
#[case::tag_directive("%TAG ! tag:x: # c\n---\na: 1\n")]
#[case::document_end("a: 1\n... # c\n")]
#[case::orphan_document_end("... # c\n")]
#[case::multi_line_double_quoted("a: \"x\n  y\" # c\nb: 1\n")]
#[case::multi_line_double_quoted_multibyte("a: \"x\n  é\" # c\nb: 1\n")]
#[case::multi_line_single_quoted("a: 'x\n  y' # c\nb: 1\n")]
#[case::multibyte_scalar_before("a: \"é日\" # c\n")]
#[case::tab_before_hash("a: \"q\"\t# c\n")]
#[case::crlf("a: \"q\" # c\r\nb: 1\r\n")]
#[case::empty_value_at_eof("k: # c")]
#[case::document_start_at_eof("--- v # c")]
#[case::header_at_eof("a: | # c")]
#[case::empty_value_crlf("k: # c\r\nb: 1\r\n")]
#[case::header_crlf("a: | # c\r\n  t\r\n")]
#[case::directive_crlf("%YAML 1.2 # c\r\n---\r\n")]
#[case::tab_after_dash("-\t# c\n  a: 1\n")]
fn comment_event_span_covers_exactly_hash_and_text(#[case] input: &str) {
    let comments: Vec<_> = parse_to_vec(input)
        .into_iter()
        .filter_map(|r| match r {
            Ok((Event::Comment { text }, span)) => Some((text, span)),
            _ => None,
        })
        .collect();
    let [(text, span)] = comments.as_slice() else {
        unreachable!("expected exactly one comment event in {input:?}: {comments:?}")
    };
    assert_eq!(text, &" c", "input: {input:?}");
    assert_eq!(
        input.get(span.start as usize..span.end as usize),
        Some("# c"),
        "input: {input:?}"
    );
}

// `#` that does not follow whitespace is not a comment at these positions: it is
// an error after a quoted scalar and content inside a plain scalar.
#[rstest]
#[case::after_quoted_scalar("a: \"q\"#c\n")]
#[case::after_single_quoted_scalar("a: 'q'#c\n")]
#[case::after_block_scalar_indicator("a: |#c\n  t\n")]
#[case::no_break_space_is_no_separator("a: \"q\"\u{a0}# c\n")]
fn hash_without_space_after_a_quoted_scalar_or_indicator_is_an_error(#[case] input: &str) {
    assert!(
        parse_events(input).any(|r| r.is_err()),
        "expected an error for {input:?}"
    );
}

// A `#` inside a scalar or a tag is content: no comment event.
#[rstest]
#[case::quoted_scalar("a: \"x # y\"\n")]
#[case::single_quoted_scalar("a: 'x # y'\n")]
#[case::plain_scalar_hash_after_char("a: x#y\n")]
#[case::block_scalar_body("a: |\n  x # y\n")]
#[case::tag_with_hash("a: !t#x v\n")]
#[case::flow_quoted("[\"x # y\", 'z # w']\n")]
fn hash_inside_a_scalar_is_not_a_comment(#[case] input: &str) {
    assert!(
        !event_shape(input).iter().any(|e| e.starts_with('#')),
        "input: {input:?}"
    );
}

// -----------------------------------------------------------------------
// Group I — Positions after a multi-line double-quoted scalar
// (the closing line's indentation is part of every position after the quote)
// -----------------------------------------------------------------------

// The span of the token after the closing quote is its real offset, in flow
// context (the value after the key) and for a following comment.
#[rstest]
#[case::flow_value_after_indented_closing_line("[ \"multi\n  line\": value]\n", "value", 18, 23)]
#[case::flow_value_after_deeper_indent("{ \"multi\n      line\": value}\n", "value", 22, 27)]
#[case::flow_value_after_multibyte_indent_text("{ \"multi\n  é\": value}\n", "value", 16, 21)]
fn token_after_a_multi_line_double_quoted_scalar_has_its_real_span(
    #[case] input: &str,
    #[case] value: &str,
    #[case] start: u32,
    #[case] end: u32,
) {
    let span = parse_to_vec(input).into_iter().find_map(|r| match r {
        Ok((Event::Scalar { value: v, .. }, span)) if v == value => Some(span),
        _ => None,
    });
    assert_eq!(
        span.map(|s| (s.start, s.end)),
        Some((start, end)),
        "input: {input:?}"
    );
    assert_eq!(input.get(start as usize..end as usize), Some(value));
}

// The error for content after the closing quote points at that content.
#[test]
fn content_after_a_multi_line_double_quoted_scalar_is_reported_at_its_offset() {
    let input = "a: \"x\n  y\" junk\n";
    let error = parse_to_vec(input)
        .into_iter()
        .find_map(Result::err)
        .map(|e| e.pos.byte_offset);
    // The offset right after the closing quote.
    assert_eq!(error, Some(10));
}

// -----------------------------------------------------------------------
// Group J — `#` that is not a comment, characters a comment may hold
// -----------------------------------------------------------------------

#[rstest]
#[case::anchor_name_holds_hash("a: &x#c v\n")]
#[case::tag_holds_hash("!<tag:x#y> v\n")]
#[case::tag_prefix_holds_hash("%TAG !e! tag:x.com,2000:app/#frag\n---\na: !e!t 1\n")]
#[case::alias_name_holds_hash("a: &x v\nb: *x#c\n")]
#[case::multi_line_double_quoted_hash_lines("a: \"x\n  # not\n  y\"\n")]
#[case::multi_line_double_quoted_hash_inside("a: \"x\n  y # in\n  z\"\n")]
#[case::folded_scalar_hash_lines("a: >\n  t # x\n  # y\n")]
fn hash_in_a_name_or_a_scalar_line_is_not_a_comment(#[case] input: &str) {
    assert!(
        !event_shape(input).iter().any(|e| e.starts_with('#')),
        "input: {input:?}"
    );
}

#[rstest]
#[case::prefix_with_hash_then_comment("%TAG ! tag:x#y # c\n---\na: 1\n", &[" c"])]
#[case::root_quoted_with_colon_in_comment("\"q\" # a: b\n", &[" a: b"])]
#[case::escaped_backslash_before_closing_quote("a: \"q\\\\\" # c\n", &[" c"])]
#[case::colon_in_comment_after_item("- \"q\" # k: v\n", &[" k: v"])]
#[case::empty_comment("a: \"q\" #\n", &[""])]
#[case::empty_comment_at_eof("a: \"q\" #", &[""])]
#[case::hash_without_space_inside_comment("a: \"q\" # b#c\n", &[" b#c"])]
fn comment_text_is_the_rest_of_the_line(#[case] input: &str, #[case] expected: &[&str]) {
    let texts = comment_texts(&event_variants(input));
    assert_eq!(texts, expected, "input: {input:?}");
}

#[rstest]
#[case::tab("\t")]
#[case::nel("\u{85}")]
#[case::line_separator("\u{2028}")]
#[case::accented("é")]
#[case::emoji("😀")]
#[case::bom("\u{FEFF}")]
fn printable_characters_are_accepted_in_a_comment_after_a_node(
    #[case] ch: &str,
    #[values(
        "a: \"q\" # {}\n",
        "k: # {}\n",
        "--- v # {}\n",
        "%YAML 1.2 # {}\n---\n",
        "a: | # {}\n  t\n"
    )]
    template: &str,
) {
    let input = template.replace("{}", ch);
    let texts = comment_texts(&event_variants(&input));
    assert_eq!(texts, [format!(" {ch}")], "input: {input:?}");
}

// A comment is only a comment: long things that are not comments are no error.
#[rstest]
#[case::quoted_scalar_body(format!("a: \"#{}\"\n", "x".repeat(MAX_COMMENT_LEN + 100)))]
#[case::plain_scalar_without_space(format!("a: x#{}\n", "y".repeat(MAX_COMMENT_LEN + 100)))]
#[case::block_scalar_content(format!("a: |\n  # {}\n", "x".repeat(MAX_COMMENT_LEN + 100)))]
fn long_text_that_is_not_a_comment_is_not_limited(#[case] input: String) {
    assert!(parse_events(&input).all(|r| r.is_ok()));
}

// A comment that is too long and holds a bad character reports the length.
#[rstest]
#[case::quoted("a: \"q\" #{}\n")]
#[case::empty_value("k: #{}\n")]
#[case::header("a: | #{}\n  t\n")]
fn length_error_comes_before_character_error(#[case] template: &str) {
    let input = template.replace("{}", &format!("\x01{}", "x".repeat(MAX_COMMENT_LEN)));
    let error = parse_to_vec(&input).into_iter().find_map(Result::err);
    assert!(
        error.is_some_and(|e| e.message.contains("comment exceeds maximum allowed length")),
        "input: {input:?}"
    );
}

// Linear time: a long line with one comment and many `#` stays fast.
#[rstest]
#[timeout(std::time::Duration::from_secs(5))]
#[case::many_hash_after_quoted_scalar(format!("a: \"q\"{}\n", " #".repeat(100_000)))]
#[case::huge_comment_after_quoted_scalar(format!("a: \"q\" #{}\n", "x".repeat(1_000_000)))]
#[case::many_hash_without_space(format!("a: \"q\" #{}\n", "a#".repeat(1_000)))]
fn long_lines_with_hashes_parse_in_linear_time(#[case] input: String) {
    let _ = parse_events(&input).count();
}

// -----------------------------------------------------------------------
// Group H — Comment lines below a bare dash
// -----------------------------------------------------------------------

// The comments and scalars of `input` in document order, each with its span.
fn scalars_and_comments_with_spans(input: &str) -> Vec<String> {
    parse_events(input)
        .filter_map(|item| {
            match item.unwrap_or_else(|e| unreachable!("unexpected parse error: {e}")) {
                (Event::Comment { text }, span) => {
                    Some(format!("#{text}@{}..{}", span.start, span.end))
                }
                (Event::Scalar { value, .. }, span) => {
                    Some(format!(":{value}@{}..{}", span.start, span.end))
                }
                _ => None,
            }
        })
        .collect()
}

// The comment lines under a bare dash come in source order next to the item's
// events: before the value they precede, after an empty item they follow it,
// and a comment above the next entry stays with that entry, not the empty item.
#[rstest]
#[case::comment_before_value("-\n  # c\n  b\n", &["# c@4..7", ":b@10..11"])]
#[case::comment_at_column_zero_before_value("-\n# c\n  b\n", &["# c@2..5", ":b@8..9"])]
#[case::comment_after_empty_item("-\n  # d\n", &[":@2..2", "# d@4..7"])]
#[case::comment_above_the_next_entry("-\n# about x\n- x\n", &[":@2..2", "# about x@2..11", ":x@14..15"])]
#[case::comment_indented_below_empty_item_before_next("-\n  # c\n- x\n", &[":@2..2", "# c@4..7", ":x@10..11"])]
fn comment_lines_below_a_bare_dash_keep_source_order(
    #[case] input: &str,
    #[case] expected: &[&str],
) {
    assert_eq!(
        scalars_and_comments_with_spans(input),
        expected,
        "{input:?}"
    );
}
