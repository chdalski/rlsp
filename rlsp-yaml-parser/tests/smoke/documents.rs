use super::*;

// -----------------------------------------------------------------------
// Group A — Basic explicit-start documents
// -----------------------------------------------------------------------

#[test]
fn bare_dash_no_newline_yields_doc_start_and_implicit_end() {
    let events = event_variants("---");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_dash_with_lf_yields_same_sequence_as_no_newline() {
    let events = event_variants("---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn explicit_end_marker_yields_explicit_doc_end() {
    let events = event_variants("---\n...");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn explicit_end_with_lf_yields_explicit_doc_end() {
    let events = event_variants("---\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group B — Multiple documents
// -----------------------------------------------------------------------

#[test]
fn two_docs_adjacent_markers_both_have_implicit_end() {
    let events = event_variants("---\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_docs_explicit_ends() {
    let events = event_variants("---\n...\n---\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_docs_blank_lines_between_markers() {
    let events = event_variants("---\n\n\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group C — Blank/comment-only stream (regressions from Task 4)
// -----------------------------------------------------------------------

#[test]
fn empty_input_produces_stream_only() {
    let events = event_variants("");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

#[test]
fn whitespace_only_produces_stream_only() {
    let events = event_variants("   \n");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

#[test]
fn comment_only_produces_stream_only() {
    let events = event_variants("# comment\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " comment" },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group D — Comments and blank lines around markers
// -----------------------------------------------------------------------

#[test]
fn comment_before_marker_is_emitted() {
    let events = event_variants("# comment\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " comment" },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn blank_lines_before_marker_are_skipped() {
    let events = event_variants("\n\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn comment_between_start_and_end_marker() {
    let events = event_variants("---\n# comment\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Comment { text: " comment" },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn blank_lines_between_start_and_end_marker() {
    let events = event_variants("---\n\n\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group E — Orphan `...` (document-end before any document start)
// -----------------------------------------------------------------------

#[test]
fn orphan_document_end_before_any_start_is_skipped() {
    let events = event_variants("...\n");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

#[test]
fn orphan_document_end_then_real_document() {
    let events = event_variants("...\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group F — Line ending variants
// -----------------------------------------------------------------------

#[test]
fn crlf_terminated_marker_is_recognised() {
    let events = event_variants("---\r\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_cr_terminated_marker_is_recognised() {
    let events = event_variants("---\r");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group G — BOM handling
// -----------------------------------------------------------------------

#[test]
fn bom_before_directives_end_marker_is_stripped_correctly() {
    let events = event_variants("\u{FEFF}---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group H — Content after marker on same line
// -----------------------------------------------------------------------

#[test]
fn content_after_dash_marker_space_separated_starts_document() {
    // Space after `---` qualifies as a marker (4th byte is space).
    // The inline content "value" is extracted as a plain scalar by Task 6.
    let events = event_variants("--- value\n");
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
                value: "value".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group I — Indented `---` is NOT a marker
// -----------------------------------------------------------------------

#[test]
fn indented_dash_is_not_a_directives_end_marker() {
    // "  ---" has indent=2; it is a plain scalar (not a marker).
    // `---` is allowed as a plain scalar when it is indented — ns-plain-first
    // allows `-` when followed by a safe ns-char, and the next two `-` chars
    // are ns-chars.
    let events = event_variants("  ---\n");
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
                value: "---".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn indented_dots_is_not_a_document_end_marker() {
    // "  ..." has indent=2; it is a plain scalar (not a doc-end marker).
    // Verifies that `peeked_indent == 0` guard in Change B does not suppress
    // the line — it reaches `is_document_end()` (which also returns false for
    // indented content) and then falls through to scalar parsing.
    let events = event_variants("---\n  ...\n");
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
                value: "...".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group J — Span assertions
// -----------------------------------------------------------------------

#[test]
fn doc_start_explicit_span_covers_three_bytes() {
    let results = parse_to_vec("---\n");
    let Some(Ok((Event::DocumentStart { .. }, span))) = results.get(1) else {
        unreachable!("expected DocumentStart as second event");
    };
    assert_eq!(
        span.end.byte_offset - span.start.byte_offset,
        3,
        "DocumentStart span must cover exactly 3 bytes"
    );
}

#[test]
fn doc_start_explicit_span_start_byte_offset_is_zero() {
    let results = parse_to_vec("---\n");
    let Some(Ok((Event::DocumentStart { .. }, span))) = results.get(1) else {
        unreachable!("expected DocumentStart as second event");
    };
    assert_eq!(span.start.byte_offset, 0);
}

#[test]
fn doc_end_explicit_span_covers_three_bytes() {
    let results = parse_to_vec("---\n...\n");
    let Some(Ok((Event::DocumentEnd { explicit: true }, span))) = results.get(2) else {
        unreachable!("expected explicit DocumentEnd as third event");
    };
    assert_eq!(
        span.end.byte_offset - span.start.byte_offset,
        3,
        "DocumentEnd span must cover exactly 3 bytes"
    );
}

#[test]
fn doc_end_explicit_span_start_byte_offset_is_four() {
    // "---\n" = 4 bytes, so `...` starts at byte offset 4.
    let results = parse_to_vec("---\n...\n");
    let Some(Ok((Event::DocumentEnd { explicit: true }, span))) = results.get(2) else {
        unreachable!("expected explicit DocumentEnd as third event");
    };
    assert_eq!(span.start.byte_offset, 4);
}

#[test]
fn doc_end_implicit_span_is_zero_width() {
    let results = parse_to_vec("---\n");
    let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(2) else {
        unreachable!("expected implicit DocumentEnd as third event");
    };
    assert_eq!(
        span.start, span.end,
        "implicit DocumentEnd span must be zero-width"
    );
}

#[test]
fn doc_start_explicit_span_start_after_blank_lines() {
    // "\n\n---\n": two newlines (2 bytes) then `---` at byte offset 2.
    let results = parse_to_vec("\n\n---\n");
    let Some(Ok((Event::DocumentStart { .. }, span))) = results.get(1) else {
        unreachable!("expected DocumentStart as second event");
    };
    assert_eq!(span.start.byte_offset, 2);
}

// -----------------------------------------------------------------------
// Group K — Bare document boundaries (IT-28 through IT-37)
// -----------------------------------------------------------------------

#[test]
fn single_content_line_yields_bare_doc() {
    let events = event_variants("foo\n");
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
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn multi_line_content_yields_single_bare_doc() {
    // Both lines fold into a single plain scalar ("foo bar").
    let events = event_variants("foo\nbar\n");
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
                value: "foo bar".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn leading_blank_before_bare_content() {
    // Blank line skipped in BetweenDocs; `foo` triggers the bare-doc path.
    let events = event_variants("\nfoo\n");
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
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_doc_with_explicit_end_marker() {
    // InDocument sees scalar, then `...` → DocumentEnd{explicit:true}.
    let events = event_variants("foo\n...\n");
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
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_doc_followed_by_explicit_doc() {
    // InDocument emits scalar, sees `---` → implicit DocumentEnd, then
    // DocumentStart{explicit:true} for the new one.
    let events = event_variants("foo\n---\n");
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
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_bare_docs_separated_by_explicit_end() {
    let events = event_variants("foo\n...\nbar\n");
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
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: true },
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "bar".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_doc_start_span_is_zero_width_at_first_content_byte() {
    // "foo\n": DocumentStart{false} span at byte 0, zero-width.
    let results = parse_to_vec("foo\n");
    let Some(Ok((
        Event::DocumentStart {
            explicit: false, ..
        },
        span,
    ))) = results.get(1)
    else {
        unreachable!("expected bare DocumentStart as second event");
    };
    assert_eq!(
        span.start, span.end,
        "bare DocumentStart span must be zero-width"
    );
    assert_eq!(span.start.byte_offset, 0);
}

#[test]
fn bare_doc_end_at_eof_span_is_zero_width_after_last_content() {
    // "foo\n" = 4 bytes; sequence: StreamStart, DocStart, Scalar, DocEnd.
    // DocEnd is at index 3 now (Scalar is at index 2).
    let results = parse_to_vec("foo\n");
    let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(3) else {
        unreachable!("expected bare DocumentEnd as fourth event");
    };
    assert_eq!(
        span.start, span.end,
        "bare DocumentEnd span must be zero-width"
    );
    assert_eq!(span.start.byte_offset, 4);
}

#[test]
fn bare_doc_end_before_explicit_doc_span_is_zero_width_at_marker_pos() {
    // "foo\n---\n": StreamStart, DocStart, Scalar, DocEnd(implicit), DocStart, DocEnd.
    // Implicit DocEnd is at index 3.
    let results = parse_to_vec("foo\n---\n");
    let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(3) else {
        unreachable!("expected implicit DocumentEnd at index 3");
    };
    assert_eq!(
        span.start, span.end,
        "implicit DocumentEnd span must be zero-width"
    );
    assert_eq!(span.start.byte_offset, 4);
}

#[test]
fn bare_doc_start_span_zero_width_after_leading_blank() {
    // "\nfoo\n": `f` is at byte offset 1 (after the leading `\n`).
    let results = parse_to_vec("\nfoo\n");
    let Some(Ok((
        Event::DocumentStart {
            explicit: false, ..
        },
        span,
    ))) = results.get(1)
    else {
        unreachable!("expected bare DocumentStart as second event");
    };
    assert_eq!(span.start.byte_offset, 1);
}

// -----------------------------------------------------------------------
// Group L — Directive line skipping (IT-38 through IT-40)
// -----------------------------------------------------------------------

#[test]
fn yaml_directive_before_explicit_doc_carries_version() {
    let events = event_variants("%YAML 1.2\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: Some((1, 2)),
                tag_directives: vec![],
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn yaml_directive_with_explicit_end_carries_version() {
    let events = event_variants("%YAML 1.2\n---\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: Some((1, 2)),
                tag_directives: vec![],
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn unknown_directive_before_explicit_doc_is_skipped() {
    // Any `%`-prefixed line is silently skipped (not limited to %YAML).
    let events = event_variants("%FOO bar\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group M — directive-split regression test (IT-41)
// -----------------------------------------------------------------------
// Verify that `%`-prefixed lines are treated as content inside a document
// (InDocument context) and not silently dropped.

#[test]
fn percent_prefixed_line_inside_explicit_doc_is_treated_as_content() {
    // IT-41: A `%`-prefixed line inside an open document (after `---`) is
    // regular content, not a directive.  It should be consumed normally
    // rather than silently dropped.
    let events = event_variants("---\n%foo: bar\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}
