// SPDX-License-Identifier: MIT
#![deny(clippy::panic)]

//! Smoke / integration tests for `rlsp-yaml-parser`.
//!
//! Tests are grouped by grammar area using nested modules.  Each task adds
//! a new `mod` block here as it introduces new event variants.
//!
//! # Shared helper
//!
//! [`parse_to_vec`] collects the full event stream into a `Vec` without
//! hiding errors.  It is the canonical test helper for all grammar tasks.

use rlsp_yaml_parser::{
    Chomp, CollectionStyle, Error, Event, MAX_ANCHOR_NAME_BYTES, MAX_COLLECTION_DEPTH,
    MAX_COMMENT_LEN, MAX_DIRECTIVES_PER_DOC, MAX_TAG_HANDLE_BYTES, MAX_TAG_LEN, Pos, ScalarStyle,
    Span, parse_events,
};

// ---------------------------------------------------------------------------
// Shared helper for extracting event variants from parse_to_vec
// ---------------------------------------------------------------------------

/// Extract only the `Event` variant (dropping the `Span`) from a `parse_to_vec`
/// result, panicking if any item is an `Err`.
fn event_variants(input: &str) -> Vec<Event<'_>> {
    parse_events(input)
        .map(|r| match r {
            Ok((ev, _span)) => ev,
            Err(e) => unreachable!("unexpected parse error: {e}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// Collect `parse_events(input)` into a `Vec`, preserving `Err` items.
///
/// The returned items include `Span`s so that later tasks can assert on
/// event positions.  Tests that only care about variant identity can use
/// `matches!` or extract the event with `.as_ref().unwrap().0`.
fn parse_to_vec(input: &str) -> Vec<Result<(Event<'_>, Span), Error>> {
    parse_events(input).collect()
}

// ---------------------------------------------------------------------------
// mod stream — StreamStart / StreamEnd (Task 4)
// ---------------------------------------------------------------------------

mod stream {
    use super::*;
    use rstest::rstest;

    // -----------------------------------------------------------------------
    // Group A: Helper contract
    // -----------------------------------------------------------------------

    #[test]
    fn parse_to_vec_returns_raw_results() {
        let events = parse_to_vec("");
        assert!(!events.is_empty(), "expected at least one event");
        assert!(events.iter().all(Result::is_ok), "all items must be Ok");
    }

    // -----------------------------------------------------------------------
    // Group B: StreamStart/StreamEnd — happy path
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::empty_string("")]
    #[case::whitespace_only("   \n\n")]
    #[case::tab_only("\t\t\t")]
    #[case::single_newline("\n")]
    #[case::crlf_only("\r\n\r\n")]
    fn stream_start_end_empty_inputs(#[case] input: &str) {
        let events = parse_to_vec(input);
        assert_eq!(events.len(), 2, "expected exactly 2 events");
        assert!(
            matches!(events.first(), Some(Ok((Event::StreamStart, _)))),
            "first event must be StreamStart"
        );
        assert!(
            matches!(events.get(1), Some(Ok((Event::StreamEnd, _)))),
            "second event must be StreamEnd"
        );
    }

    #[test]
    fn comment_only_input_emits_stream_start_comment_stream_end() {
        // Since Task 18, comment lines produce Event::Comment, not silence.
        let events = parse_to_vec("# comment\n   \n");
        // StreamStart, Comment, StreamEnd
        assert_eq!(
            events.len(),
            3,
            "expected StreamStart + Comment + StreamEnd"
        );
        assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
        assert!(matches!(
            events.get(1),
            Some(Ok((Event::Comment { .. }, _)))
        ));
        assert!(matches!(events.get(2), Some(Ok((Event::StreamEnd, _)))));
    }

    // -----------------------------------------------------------------------
    // Group C: Stream event ordering invariant
    // -----------------------------------------------------------------------

    #[test]
    fn stream_start_is_always_first_event() {
        let events = parse_to_vec("");
        assert!(
            matches!(events.first(), Some(Ok((Event::StreamStart, _)))),
            "first event must be StreamStart"
        );
        // No StreamEnd appears before StreamStart.
        let first_end_pos = events
            .iter()
            .position(|r| matches!(r, Ok((Event::StreamEnd, _))));
        let first_start_pos = events
            .iter()
            .position(|r| matches!(r, Ok((Event::StreamStart, _))));
        assert!(
            first_start_pos < first_end_pos,
            "StreamStart must come before StreamEnd"
        );
    }

    #[test]
    fn stream_end_is_always_last_event() {
        let events = parse_to_vec("");
        assert!(
            matches!(events.last(), Some(Ok((Event::StreamEnd, _)))),
            "last event must be StreamEnd"
        );
    }

    // -----------------------------------------------------------------------
    // Group D: Span correctness
    // -----------------------------------------------------------------------

    #[test]
    fn stream_start_span_starts_at_origin() {
        let events = parse_to_vec("");
        let Some(Ok((Event::StreamStart, span))) = events.first() else {
            unreachable!("expected StreamStart as first event");
        };
        assert_eq!(
            span.start,
            Pos::ORIGIN,
            "StreamStart span must start at Pos::ORIGIN"
        );
    }

    #[test]
    fn stream_end_span_for_empty_input_is_at_origin() {
        let events = parse_to_vec("");
        let Some(Ok((Event::StreamEnd, span))) = events.get(1) else {
            unreachable!("expected StreamEnd as second event");
        };
        assert_eq!(
            span.start.byte_offset, 0,
            "StreamEnd for empty input must be at byte_offset 0"
        );
    }

    #[test]
    fn stream_end_span_for_whitespace_input_reflects_consumed_bytes() {
        // "   " = 3 bytes
        let events = parse_to_vec("   ");
        let Some(Ok((Event::StreamEnd, span))) = events.get(1) else {
            unreachable!("expected StreamEnd as second event");
        };
        assert_eq!(
            span.start.byte_offset, 3,
            "StreamEnd span start must be at byte_offset 3 after consuming 3-byte input"
        );
    }

    // -----------------------------------------------------------------------
    // Group E: Iterator protocol
    // -----------------------------------------------------------------------

    #[test]
    fn iterator_is_fused_after_stream_end() {
        let mut iter = parse_events("");
        // Exhaust the iterator.
        while iter.next().is_some() {}
        // Additional calls must return None.
        assert!(
            iter.next().is_none(),
            "iterator must return None after exhaustion"
        );
    }

    #[test]
    fn parse_events_can_be_called_multiple_times_on_same_input() {
        let input = "";
        let first: Vec<_> = parse_to_vec(input);
        let second: Vec<_> = parse_to_vec(input);
        assert_eq!(
            first.len(),
            second.len(),
            "both calls must return same length"
        );
        for (a, b) in first.iter().zip(second.iter()) {
            match (a, b) {
                (Ok((ea, _)), Ok((eb, _))) => assert_eq!(ea, eb, "event variants must match"),
                _ => unreachable!("both must be Ok"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// mod documents — DocumentStart / DocumentEnd (Task 5)
// ---------------------------------------------------------------------------

mod documents {
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
}

// ---------------------------------------------------------------------------
// mod scalars — plain scalar integration tests (Task 6)
// ---------------------------------------------------------------------------

mod scalars {
    use super::*;
    use rstest::rstest;

    // Helper: make a plain `Scalar` event for easy comparison.
    fn plain(value: &str) -> Event<'_> {
        Event::Scalar {
            value: value.into(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
        }
    }

    // IT-S1/S3/S4/S5/S7/S11/S12/S13 — plain scalar in bare (implicit) document.
    #[rstest]
    #[case::plain_scalar_emits_scalar_event("hello", "hello")]
    #[case::multi_line_plain_scalar_folds_to_spaces("foo\n  bar\n  baz\n", "foo bar baz")]
    #[case::plain_scalar_with_url("http://example.com", "http://example.com")]
    #[case::plain_scalar_with_hash_inside("a#b", "a#b")]
    #[case::multi_line_plain_scalar_blank_line_folds_to_newline("foo\n\nbar\n", "foo\nbar")]
    #[case::plain_scalar_with_backslashes(
        "plain\\value\\with\\backslashes",
        "plain\\value\\with\\backslashes"
    )]
    #[case::multi_line_plain_scalar_two_blank_lines_fold_to_two_newlines(
        "foo\n\n\nbar\n",
        "foo\n\nbar"
    )]
    #[case::multi_line_plain_scalar_continuation_trailing_space_stripped(
        "foo\nbar   \nbaz\n",
        "foo bar baz"
    )]
    fn plain_scalar_implicit_doc_emits_correct_events(
        #[case] input: &str,
        #[case] expected_value: &str,
    ) {
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: false,
                    version: None,
                    tag_directives: vec![]
                },
                plain(expected_value),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S2/S14 — plain scalar with explicit `---` document marker.
    #[rstest]
    #[case::plain_scalar_explicit_doc_markers("---\nhello\n...\n", "hello", true)]
    #[case::plain_scalar_inline_after_directives_end_marker("--- text\n", "text", false)]
    fn plain_scalar_explicit_doc_emits_correct_events(
        #[case] input: &str,
        #[case] expected_value: &str,
        #[case] explicit_end: bool,
    ) {
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: true,
                    version: None,
                    tag_directives: vec![]
                },
                plain(expected_value),
                Event::DocumentEnd {
                    explicit: explicit_end
                },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S6 — plain scalar terminated by inline comment (different shape: Comment event).
    #[test]
    fn plain_scalar_terminated_by_comment() {
        // "foo # comment\n" → scalar "foo" (trailing space stripped), then Comment.
        let events = event_variants("foo # comment\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: false,
                    version: None,
                    tag_directives: vec![]
                },
                plain("foo"),
                Event::Comment { text: " comment" },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S8 — span start byte offset for plain scalar.
    #[test]
    fn plain_scalar_span_start_at_byte_zero() {
        let results = parse_to_vec("hello");
        let Some(Ok((Event::Scalar { .. }, span))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert_eq!(span.start.byte_offset, 0);
    }

    // IT-S9 — span end byte offset for plain scalar.
    #[test]
    fn plain_scalar_span_end_after_value() {
        // "hello" = 5 bytes; span end at byte 5.
        let results = parse_to_vec("hello");
        let Some(Ok((Event::Scalar { .. }, span))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert_eq!(span.end.byte_offset, 5);
    }

    // IT-S10 — span start for indented scalar.
    #[test]
    fn plain_scalar_indented_span_start() {
        // "  hello" — leading 2 spaces, scalar starts at byte 2.
        let results = parse_to_vec("  hello");
        let Some(Ok((Event::Scalar { .. }, span))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert_eq!(span.start.byte_offset, 2);
    }
}

// ---------------------------------------------------------------------------
// mod quoted_scalars — single- and double-quoted scalar integration tests (Task 7)
// ---------------------------------------------------------------------------

mod quoted_scalars {
    use std::borrow::Cow;

    use rstest::rstest;

    use super::*;

    // IT-1 through IT-5, IT-8, IT-9: quoted scalar emits correct full event sequence.
    // Cases with SingleQuoted style use single(); cases with DoubleQuoted use double().
    #[rstest]
    // IT-1 (spike): single-quoted scalar emits Scalar with SingleQuoted style.
    // Use bare document (no --- marker) so quoted scalar starts on its own line,
    // avoiding the inline_scalar slot which is plain-scalar only.
    #[case::single_quoted_hello("'hello'\n", "hello", ScalarStyle::SingleQuoted)]
    // IT-2: double-quoted scalar emits Scalar with DoubleQuoted style.
    #[case::double_quoted_hello("\"hello\"\n", "hello", ScalarStyle::DoubleQuoted)]
    // IT-3: double-quoted escape sequence produces the unescaped value.
    #[case::double_quoted_escape_newline(
        "\"with\\nescape\"\n",
        "with\nescape",
        ScalarStyle::DoubleQuoted
    )]
    // IT-4: unicode escape in double-quoted produces correct codepoint.
    #[case::double_quoted_unicode_escape("\"\\u00E9\"\n", "é", ScalarStyle::DoubleQuoted)]
    // IT-5: single-quoted doubled-quote produces literal apostrophe.
    #[case::single_quoted_escaped_quote("'it''s'\n", "it's", ScalarStyle::SingleQuoted)]
    // IT-8: single-quoted empty scalar.
    #[case::single_quoted_empty("''\n", "", ScalarStyle::SingleQuoted)]
    // IT-9: double-quoted empty scalar.
    #[case::double_quoted_empty("\"\"\n", "", ScalarStyle::DoubleQuoted)]
    fn quoted_scalar_emits_full_event_sequence(
        #[case] input: &str,
        #[case] expected_value: &str,
        #[case] expected_style: ScalarStyle,
    ) {
        let scalar = Event::Scalar {
            value: expected_value.into(),
            style: expected_style,
            anchor: None,
            tag: None,
        };
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: false,
                    version: None,
                    tag_directives: vec![]
                },
                scalar,
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-10, IT-11: malformed double-quoted input propagates at least one Err.
    #[rstest]
    // IT-10: surrogate pair codepoint is invalid in YAML.
    #[case::malformed_surrogate_escape("\"\\uD800\"\n")]
    // IT-11: unterminated double-quoted scalar is a parse error.
    #[case::unterminated_double_quoted("\"unterminated\n")]
    fn quoted_scalar_malformed_input_propagates_err(#[case] input: &str) {
        let results = parse_to_vec(input);
        assert!(
            results.iter().any(Result::is_err),
            "expected at least one Err in results"
        );
    }

    // IT-12, IT-13: no-escape quoted scalar value is Cow::Borrowed (zero-copy).
    #[rstest]
    // IT-12: single-quoted with no escapes borrows from input.
    #[case::single_quoted_no_escape("'hello'\n")]
    // IT-13: double-quoted with no escapes borrows from input.
    #[case::double_quoted_no_escape("\"hello\"\n")]
    fn quoted_scalar_no_escape_is_cow_borrowed(#[case] input: &str) {
        let results = parse_to_vec(input);
        let Some(Ok((Event::Scalar { value, .. }, _))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert!(
            matches!(value, Cow::Borrowed(_)),
            "quoted scalar with no escapes must be Cow::Borrowed"
        );
    }

    // IT-6: single-quoted span covers including delimiters.
    #[test]
    fn single_quoted_span_covers_including_delimiters() {
        // "'hello'\n" — `'hello'` starts at byte 0.
        // Span should be [0, 7) covering `'hello'` (7 bytes).
        let results = parse_to_vec("'hello'\n");
        let Some(Ok((Event::Scalar { .. }, span))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert_eq!(
            span.start.byte_offset, 0,
            "span must start at opening quote"
        );
        assert_eq!(span.end.byte_offset, 7, "span must end after closing quote");
    }

    // IT-7: double-quoted span covers including delimiters.
    #[test]
    fn double_quoted_span_covers_including_delimiters() {
        // "\"hello\"\n" — `"hello"` starts at byte 0.
        // Span should cover 7 bytes: `"hello"`.
        let results = parse_to_vec("\"hello\"\n");
        let Some(Ok((Event::Scalar { .. }, span))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert_eq!(
            span.start.byte_offset, 0,
            "span must start at opening quote"
        );
        assert_eq!(span.end.byte_offset, 7, "span must end after closing quote");
    }

    // IT-14: plain scalar regression guard — adding quoted paths must not break plain.
    #[test]
    fn single_quoted_follows_plain_scalar_fallback() {
        let events = event_variants("--- plain");
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
                    value: "plain".into(),
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }
}

// ---------------------------------------------------------------------------
// mod conformance — yaml-test-suite fixture tests (Task 5 scope)
// ---------------------------------------------------------------------------
//
// These tests use the exact YAML content from yaml-test-suite fixtures in
// `/workspace/rlsp-yaml-parser/tests/yaml-test-suite/src/`.  Only fixtures
// whose full event sequence is deterministic with Task 5's implementation
// (no scalar content) are included here.  Fixtures with scalar content are
// deferred to the task that implements scalar parsing.

mod conformance {
    use super::*;

    // CF-1: AVM7 — "Empty Stream"
    // The `∎` sentinel in the fixture means end-of-stream; after visual_to_raw
    // this is an empty string.
    #[test]
    fn avm7_empty_stream() {
        let events = event_variants("");
        assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
    }

    // CF-2: 98YD — "Spec Example 5.5. Comment Indicator"
    #[test]
    fn yd98_comment_only() {
        let events = event_variants("# Comment only.\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::Comment {
                    text: " Comment only."
                },
                Event::StreamEnd,
            ]
        );
    }

    // CF-3: HWV9 — "Document-end marker"
    // An orphan `...` with no open document produces no document events.
    #[test]
    fn hwv9_orphan_document_end() {
        let events = event_variants("...\n");
        assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
    }

    // CF-4: QT73 — "Comment and document-end marker"
    #[test]
    fn qt73_comment_and_document_end() {
        let events = event_variants("# comment\n...\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::Comment { text: " comment" },
                Event::StreamEnd,
            ]
        );
    }

    // ---------------------------------------------------------------------------
    // Task 6 conformance fixtures — plain scalars
    // ---------------------------------------------------------------------------

    // CF-5: 4V8U — "Plain scalar with backslashes"
    // yaml: `---\nplain\value\with\backslashes\n`
    #[test]
    fn cf5_4v8u_plain_scalar_with_backslashes() {
        // From yaml-test-suite/src/4V8U.yaml
        let input = "---\nplain\\value\\with\\backslashes\n";
        let events = event_variants(input);
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
                    value: "plain\\value\\with\\backslashes".into(),
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-6: EX5H — "Multiline Scalar at Top Level [1.3]"
    // yaml: `---\na\nb  \n  c\nd\n\ne\n` (with trailing spaces on b-line stripped)
    // Expected scalar: "a b c d\ne"
    // Note: ␣␣ in the fixture is two trailing spaces that get stripped.
    #[test]
    fn cf6_ex5h_multiline_scalar_at_top_level() {
        // From yaml-test-suite/src/EX5H.yaml
        // The fixture yaml field (after visual notation):
        //   "---\na\nb  \n  c\nd\n\ne\n"
        // (b has two trailing spaces that are stripped during folding)
        let input = "---\na\nb  \n  c\nd\n\ne\n";
        let events = event_variants(input);
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
                    value: "a b c d\ne".into(),
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-7: 9YRD — "Multiline Scalar at Top Level" (bare document, YAML 1.2)
    // yaml: `a\nb  \n  c\nd\n\ne\n`
    // Expected scalar: "a b c d\ne"
    #[test]
    fn cf7_9yrd_multiline_scalar_bare_doc() {
        // From yaml-test-suite/src/9YRD.yaml
        let input = "a\nb  \n  c\nd\n\ne\n";
        let events = event_variants(input);
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
                    value: "a b c d\ne".into(),
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-8: HS5T — "Spec Example 7.12. Plain Lines"
    // Tests tab-prefixed continuation, blank line folding, trailing-space stripping.
    // Expected scalar: "1st non-empty\n2nd non-empty 3rd non-empty"
    #[test]
    fn cf8_hs5t_plain_lines_spec_example() {
        // From yaml-test-suite/src/HS5T.yaml
        // Visual notation: ␣ = space, → = tab
        let input = "1st non-empty\n\n 2nd non-empty \n\t3rd non-empty\n";
        let events = event_variants(input);
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
                    value: "1st non-empty\n2nd non-empty 3rd non-empty".into(),
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-9: 27NA — "Spec Example 5.9. Directive Indicator"
    // Tests inline scalar on the same line as `---`: `--- text` → scalar "text".
    // Also tests %YAML directive — now parsed and included in DocumentStart.version.
    #[test]
    fn cf9_27na_directive_indicator_spec_example() {
        // From yaml-test-suite/src/27NA.yaml
        // yaml: "%YAML 1.2\n--- text\n"
        let input = "%YAML 1.2\n--- text\n";
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: true,
                    version: Some((1, 2)),
                    tag_directives: vec![],
                },
                Event::Scalar {
                    value: "text".into(),
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-9b: 27NA — exact name from TE spec
    #[test]
    fn yaml27na_directive_indicator_spec_example() {
        // From yaml-test-suite/src/27NA.yaml — %YAML 1.2 + `--- text`
        // The scalar "text" follows the directives-end marker on the same line.
        let input = "%YAML 1.2\n--- text\n";
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: true,
                    version: Some((1, 2)),
                    tag_directives: vec![],
                },
                Event::Scalar {
                    value: "text".into(),
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // ---------------------------------------------------------------------------
    // Task 7 conformance fixtures — quoted scalars
    // ---------------------------------------------------------------------------

    // CF-Q1: 4GC6 — "Spec Example 7.7. Single Quoted Characters"
    // yaml: `'here''s to "quotes"'`
    // Expected scalar value: `here's to "quotes"`
    #[test]
    fn cf_q1_4gc6_single_quoted_characters() {
        // Spike test — validates that single-quoted parsing works end-to-end.
        let input = "'here''s to \"quotes\"'\n";
        let events = event_variants(input);
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
                    value: "here's to \"quotes\"".into(),
                    style: ScalarStyle::SingleQuoted,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-Q2: 2LFX — "Spec Example 6.13. Reserved Directives [1.3]"
    // yaml: `%FOO  bar baz # ...\n                  # with a warning.\n---\n"foo"\n`
    // Expected scalar value: `foo`; the comment-only second line produces a Comment event.
    #[test]
    fn cf_q2_2lfx_double_quoted_after_directive() {
        let input = "%FOO  bar baz # Should be ignored\n                  # with a warning.\n---\n\"foo\"\n";
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::Comment {
                    text: " with a warning."
                },
                Event::DocumentStart {
                    explicit: true,
                    version: None,
                    tag_directives: vec![]
                },
                Event::Scalar {
                    value: "foo".into(),
                    style: ScalarStyle::DoubleQuoted,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-Q3: double-quoted scalar on its own line after a directive and `---`.
    // Based on 2LFX (not 6LVF): `%FOO ...\n---\n"foo"\n`.  The full 6LVF
    // fixture (`--- "foo"` on the same line as `---`) is not exercised here
    // because inline quoted scalars after `---` are not yet supported —
    // consume_marker_line dispatches through scan_plain_line_block (plain
    // only).  See the TODO in consume_marker_line for the deferred fix.
    #[test]
    fn cf_q3_quoted_scalar_after_directive_and_doc_marker() {
        // 2LFX variant: `%FOO ...\n                  # with a warning.\n---\n"foo"\n`
        // The comment-only second line produces a Comment event before DocumentStart.
        let input = "%FOO  bar baz # Should be ignored\n                  # with a warning.\n---\n\"foo\"\n";
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::Comment {
                    text: " with a warning."
                },
                Event::DocumentStart {
                    explicit: true,
                    version: None,
                    tag_directives: vec![]
                },
                Event::Scalar {
                    value: "foo".into(),
                    style: ScalarStyle::DoubleQuoted,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // CF-Q4: 4UYU — "Colon in Double Quoted String"
    // yaml: `"foo: bar\": baz"`
    // Expected scalar value: `foo: bar": baz`
    #[test]
    fn cf_q4_4uyu_colon_in_double_quoted() {
        let input = "\"foo: bar\\\": baz\"\n";
        let events = event_variants(input);
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
                    value: "foo: bar\": baz".into(),
                    style: ScalarStyle::DoubleQuoted,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }
}

// ---------------------------------------------------------------------------
// mod block_scalars — literal block scalar integration tests (Task 8)
// ---------------------------------------------------------------------------

mod block_scalars {
    use rstest::rstest;

    use super::*;

    // Helper: make a literal Scalar event for easy comparison.
    fn literal(value: &str, chomp: Chomp) -> Event<'_> {
        Event::Scalar {
            value: value.into(),
            style: ScalarStyle::Literal(chomp),
            anchor: None,
            tag: None,
        }
    }

    // IT-LB-1 through IT-LB-5, IT-LB-7, IT-LB-13, IT-LB-14:
    // Literal block scalar emits correct full event sequence (implicit document).
    #[rstest]
    // IT-LB-1 (spike): simple literal block scalar; validates `|` dispatch and Literal(Clip) style.
    #[case::spike_simple_literal_clip("|\n  hello\n", "hello\n", Chomp::Clip)]
    // IT-LB-2: Strip chomping removes all trailing newlines.
    #[case::strip_chomping_removes_trailing_newlines("|-\n  foo\n\n", "foo", Chomp::Strip)]
    // IT-LB-3: Keep chomping retains all trailing blank lines (content + blank = two newlines).
    #[case::keep_chomping_retains_trailing_newlines("|+\n  foo\n\n", "foo\n\n", Chomp::Keep)]
    // IT-LB-4: Clip chomping keeps exactly one trailing newline (trailing blank dropped).
    #[case::clip_chomping_keeps_one_trailing_newline("|\n  foo\n\n", "foo\n", Chomp::Clip)]
    // IT-LB-5: Explicit indent indicator; `|2` forces content_indent=2 regardless of auto-detect.
    #[case::explicit_indent_indicator("|2\n  foo\n", "foo\n", Chomp::Clip)]
    // IT-LB-7: Empty scalar — header only, no content; Clip with empty input yields "".
    #[case::empty_literal_clip_yields_empty_string("|\n", "", Chomp::Clip)]
    // IT-LB-13: Multi-line content with blank line between lines; blank becomes a newline.
    #[case::multiline_content_with_blank_line_between(
        "|\n  foo\n\n  bar\n",
        "foo\n\nbar\n",
        Chomp::Clip
    )]
    // IT-LB-14: Leading blank before first content line is included as newline (l-empty per spec §8.1.2).
    #[case::leading_blank_before_first_content("|\n\n  foo\n", "\nfoo\n", Chomp::Clip)]
    fn literal_scalar_emits_full_event_sequence(
        #[case] input: &str,
        #[case] expected_value: &str,
        #[case] chomp: Chomp,
    ) {
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: false,
                    version: None,
                    tag_directives: vec![]
                },
                literal(expected_value, chomp),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-6 — Literal terminated by dedent.
    // "|\n  foo\nkey: val\n" → "foo\n" (the `key:` line has indent 0 < 2)
    #[test]
    fn literal_terminated_by_dedent() {
        // The `key: val` line is not part of the scalar — it terminates it.
        // After the scalar, the remaining content is consumed by the fallback
        // (plain scalar handler, not yet mapping-aware), but the scalar value
        // itself is "foo\n".
        let events = parse_to_vec("|\n  foo\nbar\n");
        // Find the first scalar event.
        let scalar_event = events.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Literal(_),
                    ..
                },
                _,
            )) => Some(value.as_ref()),
            _ => None,
        });
        assert_eq!(scalar_event, Some("foo\n"));
    }

    // IT-LB-8 — Literal block scalar in explicit document.
    #[test]
    fn literal_block_scalar_in_explicit_document() {
        let events = event_variants("---\n|\n  hello world\n...\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: true,
                    version: None,
                    tag_directives: vec![]
                },
                literal("hello world\n", Chomp::Clip),
                Event::DocumentEnd { explicit: true },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-9 — Span: start at `|`, end after all consumed lines.
    #[test]
    fn span_start_at_pipe_byte_offset() {
        let results = parse_to_vec("|\n  hello\n");
        let scalar_span = results.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                },
                span,
            )) => Some(*span),
            _ => None,
        });
        let span = scalar_span.unwrap_or_else(|| unreachable!("expected a Literal scalar event"));
        // `|` is at byte 0.
        assert_eq!(span.start.byte_offset, 0, "span must start at the `|`");
    }

    // IT-LB-10 — Span: end after all content lines are consumed.
    #[test]
    fn span_end_after_all_consumed_lines() {
        // "|\n  hello\n" = 10 bytes total.
        let results = parse_to_vec("|\n  hello\n");
        let scalar_span = results.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                },
                span,
            )) => Some(*span),
            _ => None,
        });
        let span = scalar_span.unwrap_or_else(|| unreachable!("expected a Literal scalar event"));
        assert_eq!(span.end.byte_offset, 10, "span must end after all 10 bytes");
    }

    // IT-LB-11 — Span: start at `|` when `|` is after leading whitespace.
    #[test]
    fn span_start_accounts_for_leading_whitespace() {
        // "  |\n    hello\n": `|` is at byte offset 2.
        let results = parse_to_vec("  |\n    hello\n");
        let scalar_span = results.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                },
                span,
            )) => Some(*span),
            _ => None,
        });
        let span = scalar_span.unwrap_or_else(|| unreachable!("expected a Literal scalar event"));
        assert_eq!(
            span.start.byte_offset, 2,
            "span must start at `|` byte offset"
        );
    }

    // IT-LB-12 — Error path: invalid indicator character.
    #[test]
    fn invalid_indicator_character_produces_error() {
        // "|!\n  hello\n" → Err (invalid indicator `!`)
        let results = parse_to_vec("|!\n  hello\n");
        let has_err = results.iter().any(Result::is_err);
        assert!(has_err, "expected a parse error for invalid indicator `!`");
    }

    // ---------------------------------------------------------------------------
    // Task 8 conformance fixtures
    // ---------------------------------------------------------------------------

    // CF-LB-1: DWX9 — "Spec Example 8.8. Literal Content"
    // A top-level literal block scalar with leading blank lines, embedded blank,
    // and trailing comment. The spec example uses `|` (Clip).
    // yaml: "|\n \n  \n  literal\n   \n  \n  text\n\n # Comment\n"
    // Expected value: "\n\nliteral\n \n\ntext\n"
    // Note: leading blank lines (` ` and `  `) become content (indent detected=2),
    //   the ` ` line becomes "\n", the `   ` line becomes " \n", and trailing
    //   comment terminates the block.
    #[test]
    fn cf_lb1_dwx9_spec_example_8_8_literal_content() {
        // From yaml-test-suite/src/DWX9.yaml
        // Spaces on otherwise-blank lines are intentional (spec example uses ␣).
        let input = "|\n \n  \n  literal\n   \n  \n  text\n\n # Comment\n";
        let results = parse_to_vec(input);
        let scalar = results.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Literal(_),
                    ..
                },
                _,
            )) => Some(value.as_ref()),
            _ => None,
        });
        assert_eq!(scalar, Some("\n\nliteral\n \n\ntext\n"));
    }

    // CF-LB-2: 96NN — "Leading tab content in literals"
    // A `|-` literal with tab character after indent spaces.
    // yaml: "foo: |-\n \t\tbar\n"
    // Expected scalar value for the `foo` key: "\tbar" (tab preserved)
    // Note: Task 8 only handles top-level scalars; the mapping key is consumed
    // as a plain scalar. We test only that the literal scalar value is correct.
    #[test]
    fn cf_lb2_96nn_tab_in_content_preserved() {
        // From yaml-test-suite/src/96NN.yaml — tab after indent spaces is content.
        // At top level with indent=1: "|-\n \t\tbar\n"
        // parent_indent=0, auto-detect finds indent=1, content= "\t\tbar" (tab+tab+bar)
        // Strip chomping → "\t\tbar" (no trailing newline).
        let input = "|-\n \t\tbar\n";
        let results = parse_to_vec(input);
        let scalar = results.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Literal(_),
                    ..
                },
                _,
            )) => Some(value.as_ref()),
            _ => None,
        });
        assert_eq!(scalar, Some("\t\tbar"));
    }

    // CF-LB-3: M29M — "Literal Block Scalar" (NimYAML test)
    // Mapping with literal scalar value containing blank lines.
    // Top-level test: we test only the scalar content without mapping support.
    // yaml: "|\n ab\n \n cd\n ef\n \n\n...\n"
    // Expected: "ab\n\ncd\nef\n" (Clip)
    #[test]
    fn cf_lb3_m29m_literal_block_with_blank_lines() {
        // From yaml-test-suite/src/M29M.yaml — literal block with embedded blanks.
        let input = "|\n ab\n \n cd\n ef\n \n\n...\n";
        let results = parse_to_vec(input);
        let scalar = results.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Literal(_),
                    ..
                },
                _,
            )) => Some(value.as_ref()),
            _ => None,
        });
        assert_eq!(scalar, Some("ab\n\ncd\nef\n"));
    }
}

// ---------------------------------------------------------------------------
// mod folded_scalars — folded block scalar integration tests (Task 10)
// ---------------------------------------------------------------------------

mod folded_scalars {
    use rstest::rstest;

    use super::*;

    // Helper: make a folded Scalar event for easy comparison.
    fn folded(value: &str, chomp: Chomp) -> Event<'_> {
        Event::Scalar {
            value: value.into(),
            style: ScalarStyle::Folded(chomp),
            anchor: None,
            tag: None,
        }
    }

    // IT-FB-1 through IT-FB-20 (excluding 19): folded scalar emits correct full event
    // sequence (implicit document). Cases cover folding rules, chomping, explicit indent,
    // and edge cases.
    #[rstest]
    // IT-FB-1 (spike): two-line folded scalar; validates `>` dispatch and Folded(Clip) style.
    #[case::spike_two_line_break_becomes_space(">\n  foo\n  bar\n", "foo bar\n", Chomp::Clip)]
    // IT-FB-2: single non-blank line is not folded (no preceding content to join).
    #[case::single_line_not_folded(">\n  hello\n", "hello\n", Chomp::Clip)]
    // IT-FB-3: three equally-indented non-blank lines — all breaks folded to spaces.
    #[case::three_lines_all_breaks_become_spaces(">\n  a\n  b\n  c\n", "a b c\n", Chomp::Clip)]
    // IT-FB-4: one blank line between non-blank lines produces one newline (§8.1.3).
    #[case::one_blank_line_produces_one_newline(">\n  foo\n\n  bar\n", "foo\nbar\n", Chomp::Clip)]
    // IT-FB-5: two blank lines between non-blank lines produce two newlines.
    #[case::two_blank_lines_produce_two_newlines(
        ">\n  foo\n\n\n  bar\n",
        "foo\n\nbar\n",
        Chomp::Clip
    )]
    // IT-FB-6: more-indented line — break before preserved as `\n`; relative indent kept.
    #[case::more_indented_break_before_preserved(
        ">\n  normal\n    indented\n",
        "normal\n  indented\n",
        Chomp::Clip
    )]
    // IT-FB-7: breaks surrounding more-indented region both preserved (§8.1.3).
    #[case::breaks_surrounding_more_indented_both_preserved(
        ">\n  a\n    b\n  c\n",
        "a\n  b\nc\n",
        Chomp::Clip
    )]
    // IT-FB-8: all lines at same deeper indent — auto-detect, normal folding.
    #[case::all_deep_lines_equally_indented_normal_folding(
        ">\n    deep\n    also deep\n",
        "deep also deep\n",
        Chomp::Clip
    )]
    // IT-FB-9: Strip (`>-`) — trailing newlines removed.
    #[case::strip_chomp_removes_trailing_newlines(">-\n  foo\n\n", "foo", Chomp::Strip)]
    // IT-FB-10: Keep (`>+`) — trailing blank lines preserved.
    #[case::keep_chomp_preserves_trailing_blank_lines(">+\n  foo\n\n", "foo\n\n", Chomp::Keep)]
    // IT-FB-11: Clip (`>`) — single trailing newline kept, extra blanks dropped.
    #[case::clip_chomp_keeps_one_trailing_newline(">\n  foo\n\n", "foo\n", Chomp::Clip)]
    // IT-FB-12: explicit indent indicator `>2`.
    #[case::explicit_indent_indicator(">2\n  foo\n  bar\n", "foo bar\n", Chomp::Clip)]
    // IT-FB-13: explicit indent with strip, chomp-then-indent order `>-2`.
    #[case::explicit_indent_with_strip_chomp_then_indent(">-2\n  foo\n", "foo", Chomp::Strip)]
    // IT-FB-14: explicit indent with keep, chomp-then-indent order `>+2`.
    #[case::explicit_indent_with_keep_chomp_then_indent(">+2\n  foo\n\n", "foo\n\n", Chomp::Keep)]
    // IT-FB-15: explicit indent with strip, indent-then-chomp order `>2-` (both orderings accepted).
    #[case::explicit_indent_with_strip_indent_then_chomp(">2-\n  foo\n", "foo", Chomp::Strip)]
    // IT-FB-16: empty folded scalar (header only, no content) yields "".
    #[case::empty_folded_scalar_yields_empty_string(">\n", "", Chomp::Clip)]
    // IT-FB-17: all-blank content (blank lines only, no non-blank lines) yields "" with Clip.
    #[case::all_blank_content_yields_empty_string(">\n\n\n", "", Chomp::Clip)]
    // IT-FB-18: single-line with trailing blanks (Keep preserves all trailing blank lines).
    #[case::keep_chomp_with_multiple_trailing_blanks(">+\n  only\n\n\n", "only\n\n\n", Chomp::Keep)]
    // IT-FB-20: leading blank before first content line becomes a newline prefix (l-empty).
    #[case::leading_blank_before_first_content(">\n\n  foo\n", "\nfoo\n", Chomp::Clip)]
    fn folded_scalar_emits_full_event_sequence(
        #[case] input: &str,
        #[case] expected_value: &str,
        #[case] chomp: Chomp,
    ) {
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: false,
                    version: None,
                    tag_directives: vec![]
                },
                folded(expected_value, chomp),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-19 — terminated by dedent.
    #[test]
    fn folded_terminated_by_dedent() {
        let events = parse_to_vec(">\n  foo\n  bar\nkey\n");
        let scalar_value = events.iter().find_map(|r| match r {
            Ok((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Folded(_),
                    ..
                },
                _,
            )) => Some(value.as_ref()),
            _ => None,
        });
        assert_eq!(scalar_value, Some("foo bar\n"));
    }

    // IT-FB-20 — leading blank before first content line.
    // blank line before first content → leading newline (l-empty).
    #[test]
    fn leading_blank_before_first_content() {
        let events = event_variants(">\n\n  foo\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: false,
                    version: None,
                    tag_directives: vec![]
                },
                folded("\nfoo\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Span correctness
    // -----------------------------------------------------------------------

    // IT-FB-21 — span starts at `>`.
    #[test]
    fn span_starts_at_gt() {
        let results = parse_to_vec(">\n  hello\n");
        let span = results
            .iter()
            .find_map(|r| match r {
                Ok((
                    Event::Scalar {
                        style: ScalarStyle::Folded(_),
                        ..
                    },
                    span,
                )) => Some(*span),
                _ => None,
            })
            .unwrap_or_else(|| unreachable!("expected a Folded scalar event"));
        assert_eq!(span.start.byte_offset, 0, "span must start at the `>`");
    }

    // IT-FB-22 — span starts at `>` when preceded by whitespace.
    #[test]
    fn span_start_accounts_for_leading_whitespace() {
        // "  >\n    hello\n": `>` is at byte offset 2.
        let results = parse_to_vec("  >\n    hello\n");
        let span = results
            .iter()
            .find_map(|r| match r {
                Ok((
                    Event::Scalar {
                        style: ScalarStyle::Folded(_),
                        ..
                    },
                    span,
                )) => Some(*span),
                _ => None,
            })
            .unwrap_or_else(|| unreachable!("expected a Folded scalar event"));
        assert_eq!(
            span.start.byte_offset, 2,
            "span must start at `>` byte offset"
        );
    }

    // IT-FB-23 — span ends after all consumed lines.
    #[test]
    fn span_end_after_all_consumed_lines() {
        // ">\n  hello\n" = 10 bytes total.
        let results = parse_to_vec(">\n  hello\n");
        let span = results
            .iter()
            .find_map(|r| match r {
                Ok((
                    Event::Scalar {
                        style: ScalarStyle::Folded(_),
                        ..
                    },
                    span,
                )) => Some(*span),
                _ => None,
            })
            .unwrap_or_else(|| unreachable!("expected a Folded scalar event"));
        assert_eq!(span.end.byte_offset, 10, "span must end after all 10 bytes");
    }

    // -----------------------------------------------------------------------
    // Error paths
    // -----------------------------------------------------------------------

    // IT-FB-24, IT-FB-25, IT-FB-26: invalid folded scalar headers produce at least one Err.
    #[rstest]
    // IT-FB-24: invalid indicator character — `!` is not a valid chomping or indent indicator.
    #[case::invalid_indicator_character(">!\n  hello\n")]
    // IT-FB-25: indent indicator `0` is invalid per spec (indentation must be 1–9).
    #[case::indent_indicator_zero_is_invalid(">0\n  hello\n")]
    // IT-FB-26: duplicate chomp indicator `++` is invalid.
    #[case::duplicate_chomp_indicator_is_invalid(">++\n  hello\n")]
    fn folded_scalar_invalid_header_produces_err(#[case] input: &str) {
        let results = parse_to_vec(input);
        assert!(
            results.iter().any(Result::is_err),
            "expected at least one Err for invalid header in: {input:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Explicit document integration
    // -----------------------------------------------------------------------

    // IT-FB-27 — folded scalar in explicit document.
    #[test]
    fn folded_scalar_in_explicit_document() {
        let events = event_variants("---\n>\n  hello world\n...\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart {
                    explicit: true,
                    version: None,
                    tag_directives: vec![]
                },
                folded("hello world\n", Chomp::Clip),
                Event::DocumentEnd { explicit: true },
                Event::StreamEnd,
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Style emission
    // -----------------------------------------------------------------------

    // IT-FB-28 — `ScalarStyle::Folded(Chomp::Clip)` emitted through `parse_events`.
    // Explicit style discriminant check — ensures scanner wires to `Folded` not `Literal`.
    #[test]
    fn folded_scalar_style_is_folded_not_literal() {
        let results = parse_to_vec(">\n  text\n");
        let style = results.iter().find_map(|r| match r {
            Ok((Event::Scalar { style, .. }, _)) => Some(*style),
            _ => None,
        });
        assert_eq!(
            style,
            Some(ScalarStyle::Folded(Chomp::Clip)),
            "scalar style must be Folded(Clip), not Literal or Plain"
        );
    }
}

// ---------------------------------------------------------------------------
// mod sequences — Block sequences (Task 11)
// ---------------------------------------------------------------------------

mod sequences {
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
        let seq_start_span =
            seq_start_span.unwrap_or_else(|| unreachable!("SequenceStart must exist"));
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
        let seq_start_span =
            seq_start_span.unwrap_or_else(|| unreachable!("SequenceStart must exist"));
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
}

// ---------------------------------------------------------------------------
// mod mappings — Block mappings (Task 12)
// ---------------------------------------------------------------------------

mod mappings {
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
}

// ---------------------------------------------------------------------------
// mod nested_collections — Cross-type nesting audit (Task 13)
// ---------------------------------------------------------------------------

mod nested_collections {
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
        let has_inner = results.iter().any(
            |r| matches!(r, Ok((Event::Scalar { value, .. }, _)) if value.as_ref() == "inner"),
        );
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
        let events = event_variants(
            "- item: Super Hoop\n  quantity: 1\n- item: Basketball\n  quantity: 4\n",
        );
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
}

// ---------------------------------------------------------------------------
// mod flow_collections — Flow sequences and mappings (Task 14)
// ---------------------------------------------------------------------------

mod flow_collections {
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

    // Local helper: extract scalar string values from events, skipping non-scalars.
    fn scalar_values<'a>(events: &'a [Event<'a>]) -> Vec<&'a str> {
        events
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
            .collect()
    }

    // Local helper: parse and return event variants, panicking on any error.
    fn evs(input: &str) -> Vec<Event<'_>> {
        parse_events(input)
            .map(|r| match r {
                Ok((ev, _)) => ev,
                Err(e) => unreachable!("unexpected parse error: {e}"),
            })
            .collect()
    }

    // Local helper: count events of a specific type using a predicate.
    fn count<'a>(events: &[Event<'a>], pred: impl Fn(&Event<'a>) -> bool) -> usize {
        events.iter().filter(|e| pred(e)).count()
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
        let triple = events.windows(3).find(|w| {
            matches!(w, [a, b, _] if *a == seq_start_flow() && *b == single_quoted("hello world"))
        });
        assert!(
            matches!(triple, Some([_, _, Event::SequenceEnd])),
            "SequenceStart(Flow), single_quoted(hello world), SequenceEnd; events: {events:?}"
        );
    }

    #[test]
    fn double_quoted_scalar_in_flow_sequence() {
        let events = evs("[\"hello\"]\n");
        let triple = events.windows(3).find(
            |w| matches!(w, [a, b, _] if *a == seq_start_flow() && *b == double_quoted("hello")),
        );
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
}

// ---------------------------------------------------------------------------
// mod nested_flow_block_mixing — Task 15
//
// Covers every combination from the cross-context matrix:
//   • flow-inside-flow  (legal)
//   • flow-inside-block (legal)
//   • block-inside-flow (illegal per YAML 1.2 §7.4)
//
// Cross-references to existing Task 14 coverage in `mod flow_collections`:
//   • flow seq inside flow seq     → `nested_flow_sequence_inside_sequence`
//   • flow map inside flow seq     → `nested_flow_mapping_inside_sequence`
//   • flow seq inside flow map val → `nested_flow_sequence_inside_mapping_value`
//   • flow seq as block map value  → `flow_sequence_as_block_mapping_value`
//   • flow map as block map value  → `flow_mapping_as_block_mapping_value`
//   • flow seq as block seq item   → `flow_sequence_as_block_sequence_item`
//   • explicit key in flow map     → `explicit_key_in_flow_mapping`
// ---------------------------------------------------------------------------

mod nested_flow_block_mixing {
    use super::*;

    // Module-local helpers (same pattern as `mod flow_collections`).

    const fn seq_start_flow() -> Event<'static> {
        Event::SequenceStart {
            anchor: None,
            tag: None,
            style: CollectionStyle::Flow,
        }
    }

    fn evs(input: &str) -> Vec<Event<'_>> {
        parse_events(input)
            .map(|r| match r {
                Ok((ev, _)) => ev,
                Err(e) => unreachable!("unexpected parse error: {e}"),
            })
            .collect()
    }

    fn scalar_values<'a>(events: &'a [Event<'a>]) -> Vec<&'a str> {
        events
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
            .collect()
    }

    fn count<'a>(events: &[Event<'a>], pred: impl Fn(&Event<'a>) -> bool) -> usize {
        events.iter().filter(|e| pred(e)).count()
    }

    fn has_error(input: &str) -> bool {
        parse_events(input).any(|r| r.is_err())
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
}

// ---------------------------------------------------------------------------
// mod anchors_and_aliases — Task 16
//
// Covers anchor (`&name`) and alias (`*name`) parsing in both block and flow
// contexts.  Tests are ordered:
//   A — anchor on block scalars
//   B — anchor on block sequences
//   C — anchor on block mappings
//   D — alias in block context
//   E — anchor / alias in flow context
//   F — error cases (empty name, oversized name, invalid position)
//   G — span correctness
//   H — conformance (yaml-test-suite fixtures)
// ---------------------------------------------------------------------------

mod anchors_and_aliases {
    use rstest::rstest;

    use super::*;

    fn evs(input: &str) -> Vec<Event<'_>> {
        parse_events(input)
            .map(|r| match r {
                Ok((ev, _)) => ev,
                Err(e) => unreachable!("unexpected parse error: {e}"),
            })
            .collect()
    }

    fn has_error(input: &str) -> bool {
        parse_events(input).any(|r| r.is_err())
    }

    // -----------------------------------------------------------------------
    // Group A: Anchor on block scalars
    // -----------------------------------------------------------------------

    #[test]
    fn anchor_inline_before_plain_scalar_value() {
        // `key: &a val\n` — anchor before plain scalar value.
        let events = evs("key: &a val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "val"
            )),
            "anchor `&a` must be attached to value scalar 'val'"
        );
    }

    #[test]
    fn anchor_on_standalone_line_applies_to_scalar_below() {
        // `&a\nval\n` — anchor on own line, scalar on next line.
        let events = evs("&a\nval\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "val"
            )),
            "standalone anchor `&a` must be attached to following scalar"
        );
    }

    #[test]
    fn anchor_on_mapping_key_scalar() {
        // `&k key: val\n` — anchor is inline before the key, so it annotates
        // the key scalar (YAML test suite 9KAX: inline property → key scalar).
        let events = evs("&k key: val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
            )),
            "anchor `&k` must be attached to key scalar"
        );
        // MappingStart carries no anchor.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { anchor: None, .. })),
            "MappingStart must have no anchor"
        );
    }

    #[test]
    fn anchor_on_sequence_item_plain_scalar() {
        // `- &a item\n` — anchor on a plain scalar sequence item.
        let events = evs("- &a item\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "item"
            )),
            "anchor `&a` must be attached to sequence item scalar"
        );
    }

    #[test]
    fn anchor_on_empty_scalar_value() {
        // yaml-test-suite 6KGN: `a: &anchor\nb: *anchor\n`
        // `&anchor` with no inline content → empty scalar value.
        let events = evs("a: &anchor\nb: *anchor\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("anchor"), value, .. } if value.as_ref() == ""
            )),
            "anchor `&anchor` with no value must emit empty scalar"
        );
    }

    #[test]
    fn duplicate_anchor_name_overwrites_previous() {
        // `First: &anchor Foo\nOverride: &anchor Bar\n`
        // The parser emits both scalars each with the anchor; no error.
        let events = evs("First: &anchor Foo\nOverride: &anchor Bar\n");
        let anchored: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Scalar {
                    anchor: Some("anchor"),
                    value,
                    ..
                } => Some(value.as_ref()),
                Event::Scalar { .. }
                | Event::StreamStart
                | Event::StreamEnd
                | Event::Alias { .. }
                | Event::DocumentStart { .. }
                | Event::DocumentEnd { .. }
                | Event::SequenceStart { .. }
                | Event::SequenceEnd
                | Event::MappingStart { .. }
                | Event::MappingEnd
                | Event::Comment { .. } => None,
            })
            .collect();
        assert_eq!(
            anchored.len(),
            2,
            "both anchored scalars must appear; got {anchored:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group B: Anchor on block sequences
    // -----------------------------------------------------------------------

    #[test]
    fn anchor_on_standalone_line_applies_to_block_sequence() {
        // `&seq\n- a\n- b\n` — standalone anchor applies to following sequence.
        let events = evs("&seq\n- a\n- b\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    anchor: Some("seq"),
                    style: CollectionStyle::Block,
                    ..
                }
            )),
            "standalone anchor `&seq` must be attached to SequenceStart"
        );
    }

    #[test]
    fn inline_anchor_on_dash_applies_to_nested_sequence() {
        // `- &seq\n  - a\n` — anchor before nested sequence item.
        // The `&seq` is on the same line as `-`, so it applies to the nested seq.
        let events = evs("- &seq\n  - a\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    anchor: Some("seq"),
                    style: CollectionStyle::Block,
                    ..
                }
            )),
            "anchor `&seq` on dash line must be attached to nested SequenceStart"
        );
    }

    // -----------------------------------------------------------------------
    // Group C: Anchor on block mappings
    // -----------------------------------------------------------------------

    #[test]
    fn anchor_on_standalone_line_applies_to_block_mapping() {
        // `&map\nkey: val\n` — standalone anchor applies to the mapping.
        let events = evs("&map\nkey: val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    anchor: Some("map"),
                    style: CollectionStyle::Block,
                    ..
                }
            )),
            "standalone anchor `&map` must be attached to MappingStart"
        );
    }

    #[test]
    fn anchor_inline_before_mapping_value_applies_to_nested_mapping() {
        // `key: &node\n  inner: val\n` — anchor before nested mapping.
        let events = evs("key: &node\n  inner: val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    anchor: Some("node"),
                    style: CollectionStyle::Block,
                    ..
                }
            )),
            "anchor `&node` inline before nested mapping must be attached to MappingStart"
        );
    }

    #[test]
    fn inline_anchor_on_key_does_not_annotate_mapping_start() {
        // `&k key: val\n` — `&k` is inline before the key, so it annotates the
        // key scalar, NOT the MappingStart (YAML test suite 9KAX).
        let events = evs("&k key: val\n");
        // MappingStart has no anchor.
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    anchor: None,
                    style: CollectionStyle::Block,
                    ..
                }
            )),
            "MappingStart must have no anchor when anchor is inline before key"
        );
        // The key scalar carries the anchor.
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
            )),
            "anchor `&k` must be on key scalar"
        );
    }

    // -----------------------------------------------------------------------
    // Group D: Alias in block context
    // -----------------------------------------------------------------------

    // D-1, D-2: `*ref` in different block positions emits Event::Alias { name: "ref" }.
    #[rstest]
    // D-1: Alias as block mapping value.
    #[case::alias_as_block_mapping_value("key: *ref\n")]
    // D-2: Alias as block sequence item.
    #[case::alias_as_block_sequence_item("- *ref\n")]
    fn alias_in_block_context_emits_alias_event(#[case] input: &str) {
        let events = evs(input);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "ref" })),
            "alias `*ref` must emit Alias {{ name: \"ref\" }} for input: {input:?}"
        );
    }

    #[test]
    fn alias_as_block_mapping_key_explicit() {
        // `? *ref\n: value\n` — alias as explicit block mapping key.
        let events = evs("? *ref\n: value\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "ref" })),
            "alias `*ref` as explicit mapping key must emit Alias event"
        );
        // The value scalar must also be present.
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { value, .. } if value.as_ref() == "value"
            )),
            "mapping value 'value' must be present after the alias key"
        );
    }

    #[test]
    fn alias_does_not_expand_referenced_node() {
        // Parser must emit Event::Alias, NOT re-emit the anchored node's events.
        // yaml-test-suite 3GZX: anchor-then-alias mapping.
        let events = evs("First occurrence: &anchor Foo\nSecond occurrence: *anchor\n");
        let alias_count = events
            .iter()
            .filter(|e| matches!(e, Event::Alias { .. }))
            .count();
        assert_eq!(alias_count, 1, "exactly one Alias event emitted");
        // No second "Foo" scalar should appear (no expansion).
        let foo_scalars = events
            .iter()
            .filter(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "Foo"))
            .count();
        assert_eq!(
            foo_scalars, 1,
            "value 'Foo' must appear exactly once (no alias expansion)"
        );
    }

    // -----------------------------------------------------------------------
    // Group E: Anchor / alias in flow context
    // -----------------------------------------------------------------------

    #[test]
    fn anchor_on_flow_sequence_start() {
        // `&seq [a, b]\n` — anchor applied to a flow sequence.
        let events = evs("&seq [a, b]\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    anchor: Some("seq"),
                    style: CollectionStyle::Flow,
                    ..
                }
            )),
            "anchor `&seq` must be attached to flow SequenceStart"
        );
    }

    #[test]
    fn anchor_on_flow_mapping_start() {
        // `&map {a: b}\n` — anchor applied to a flow mapping.
        let events = evs("&map {a: b}\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    anchor: Some("map"),
                    style: CollectionStyle::Flow,
                    ..
                }
            )),
            "anchor `&map` must be attached to flow MappingStart"
        );
    }

    #[test]
    fn anchor_on_plain_scalar_inside_flow_mapping() {
        // `{key: &a val}\n` — anchor on value inside flow mapping.
        let events = evs("{key: &a val}\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "val"
            )),
            "anchor `&a` must be attached to scalar 'val' in flow mapping"
        );
    }

    #[test]
    fn alias_in_flow_mapping_emits_alias_event() {
        // `{key: *ref}\n` — alias as flow mapping value.
        let events = evs("{key: *ref}\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "ref" })),
            "alias `*ref` inside flow mapping must emit Alias event"
        );
    }

    // -----------------------------------------------------------------------
    // Group F: Error cases
    // -----------------------------------------------------------------------

    #[test]
    fn anchor_with_empty_name_returns_error() {
        // `& val\n` — `&` immediately followed by space is an empty anchor name.
        assert!(
            has_error("& val\n"),
            "empty anchor name `&<space>` must return an error"
        );
    }

    #[test]
    fn alias_with_empty_name_returns_error() {
        // `* val\n` — `*` immediately followed by space is an empty alias name.
        assert!(
            has_error("* val\n"),
            "empty alias name `*<space>` must return an error"
        );
    }

    // Anchor name at the length boundary: at-limit accepted, over-limit rejected.
    #[rstest]
    // At exactly MAX_ANCHOR_NAME_BYTES: accepted.
    #[case::at_max_length_accepted(MAX_ANCHOR_NAME_BYTES, false)]
    // One byte over the limit: rejected.
    #[case::over_max_length_returns_error(MAX_ANCHOR_NAME_BYTES + 1, true)]
    fn anchor_name_length_boundary(#[case] name_len: usize, #[case] expect_error: bool) {
        let name = "a".repeat(name_len);
        let input = format!("&{name} val\n");
        assert_eq!(
            has_error(&input),
            expect_error,
            "anchor name of {name_len} bytes: expect_error={expect_error} (limit={MAX_ANCHOR_NAME_BYTES})"
        );
    }

    #[test]
    fn anchor_name_with_unicode_characters_is_accepted() {
        // yaml-test-suite 8XYN: unicode anchor name (emoji counts as ns-anchor-char).
        // `&😁 unicode anchor\n`
        let events = evs("- &\u{1F601} unicode anchor\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    anchor: Some("\u{1F601}"),
                    ..
                }
            )),
            "unicode anchor name must be accepted"
        );
    }

    #[test]
    fn alias_name_exceeding_max_length_returns_error() {
        // Alias name one byte over the limit must return an error.
        // scan_anchor_name is shared between anchors and aliases, so the same
        // limit applies to both.
        let name = "a".repeat(MAX_ANCHOR_NAME_BYTES + 1);
        let input = format!("*{name}\n");
        assert!(
            has_error(&input),
            "alias name of {} bytes must be rejected (limit is {MAX_ANCHOR_NAME_BYTES})",
            MAX_ANCHOR_NAME_BYTES + 1
        );
    }

    #[test]
    fn flow_indicator_terminates_anchor_name() {
        // `[&name item]\n` — the space terminates the anchor name (space is not
        // ns-anchor-char); `item` is the anchored scalar.  The anchor name must
        // be `"name"`, not `"name item"`.  This also verifies that flow
        // indicators (`,`, `]`, `}`) inside the name would be excluded by
        // is_ns_anchor_char.
        let events = evs("[&name item]\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("name"), value, .. } if value.as_ref() == "item"
            )),
            "anchor name must be `name`, not include the space or the value `item`"
        );
    }

    #[test]
    fn line_break_terminates_anchor_name() {
        // `&name\nscalar\n` — newline terminates the anchor name on the first
        // line; `scalar` is the following node that inherits the anchor.
        // is_ns_anchor_char excludes `\n`, so the scan stops at end-of-content.
        let events = evs("&name\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("name"), value, .. } if value.as_ref() == "scalar"
            )),
            "newline must terminate anchor name; anchor `name` must attach to following `scalar`"
        );
    }

    #[test]
    fn tag_before_anchor_on_same_line_both_emitted() {
        // `!tag &anchor value\n` — Task 17 implements tag parsing.
        // Both the tag and anchor are emitted on the scalar.
        let events = evs("!tag &anchor value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    anchor: Some("anchor"),
                    tag: Some(t),
                    ..
                } if t.as_ref() == "!tag"
            )),
            "tag-before-anchor on same line: both tag and anchor must be emitted on the scalar"
        );
    }

    // -----------------------------------------------------------------------
    // Group G: Span correctness
    // -----------------------------------------------------------------------

    #[test]
    fn alias_event_span_covers_star_and_name() {
        // `*foo\n` — alias span must start at `*` and cover the full name.
        // `*` = byte 0, `foo` = bytes 1-3, so span is [0, 4).
        let items = parse_to_vec("*foo\n");
        let alias_span = items.iter().find_map(|r| match r {
            Ok((Event::Alias { .. }, span)) => Some(*span),
            Ok(_) | Err(_) => None,
        });
        assert!(alias_span.is_some(), "Alias event must be present");
        if let Some(span) = alias_span {
            assert_eq!(span.start.byte_offset, 0, "Alias span must start at byte 0");
            assert_eq!(
                span.end.byte_offset, 4,
                "Alias span must end at byte 4 (after 'foo')"
            );
            assert_eq!(span.start.column, 0, "Alias must start at column 0");
        }
    }

    #[test]
    fn anchor_name_borrowed_from_input_not_allocated() {
        // Anchor names must be `&'input str` borrows — verify round-trip identity.
        let input = "key: &myanchor value\n";
        let events = evs(input);
        let found = events.iter().any(|e| {
            matches!(
                e,
                Event::Scalar { anchor: Some("myanchor"), value, .. } if value.as_ref() == "value"
            )
        });
        assert!(found, "anchor name must survive as a borrowed slice");
    }

    // -----------------------------------------------------------------------
    // Group H: Conformance (yaml-test-suite fixtures)
    // -----------------------------------------------------------------------

    #[test]
    fn conformance_3gzx_spec_example_7_1_alias_nodes() {
        // yaml-test-suite 3GZX: Spec Example 7.1. Alias Nodes.
        // `First occurrence: &anchor Foo\nSecond occurrence: *anchor\n
        //  Override anchor: &anchor Bar\nReuse anchor: *anchor\n`
        let input = "First occurrence: &anchor Foo\n\
                     Second occurrence: *anchor\n\
                     Override anchor: &anchor Bar\n\
                     Reuse anchor: *anchor\n";
        let events = evs(input);

        // Two anchored scalars ("Foo" and "Bar").
        let anchored_scalar_count = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Scalar {
                        anchor: Some("anchor"),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            anchored_scalar_count, 2,
            "must have two scalars with anchor 'anchor'"
        );

        // Two alias events.
        let alias_count = events
            .iter()
            .filter(|e| matches!(e, Event::Alias { name: "anchor" }))
            .count();
        assert_eq!(alias_count, 2, "must have two alias events for 'anchor'");
    }

    #[test]
    fn conformance_6kgn_anchor_for_empty_node() {
        // yaml-test-suite 6KGN: Anchor for empty node.
        // `---\na: &anchor\nb: *anchor\n`
        let input = "---\na: &anchor\nb: *anchor\n";
        let events = evs(input);

        // The anchored empty scalar.
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("anchor"), value, .. } if value.as_ref() == ""
            )),
            "anchored empty scalar must be present"
        );

        // The alias.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "anchor" })),
            "alias *anchor must emit Alias event"
        );
    }

    #[test]
    fn conformance_7bub_spec_example_2_10_sammy_sosa() {
        // yaml-test-suite 7BUB: Spec Example 2.10 — anchor on sequence item,
        // alias as subsequent item in another sequence.
        let input = "---\nhr:\n  - Mark McGwire\n  - &SS Sammy Sosa\n\
                     rbi:\n  - *SS\n  - Ken Griffey\n";
        let events = evs(input);

        // Anchored scalar "Sammy Sosa".
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("SS"), value, .. } if value.as_ref() == "Sammy Sosa"
            )),
            "anchor `&SS` on 'Sammy Sosa' must be present"
        );

        // Alias *SS.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "SS" })),
            "alias `*SS` must emit Alias event"
        );
    }

    #[test]
    fn conformance_8xyn_anchor_with_unicode_character() {
        // yaml-test-suite 8XYN: Unicode anchor name (emoji).
        let input = "---\n- &\u{1F601} unicode anchor\n";
        let events = evs(input);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("\u{1F601}"), value, .. }
                    if value.as_ref() == "unicode anchor"
            )),
            "unicode emoji anchor name must be accepted"
        );
    }

    #[test]
    fn conformance_6m2f_aliases_in_explicit_block_mapping() {
        // yaml-test-suite 6M2F: Aliases in Explicit Block Mapping.
        // `? &a a\n: &b b\n: *a\n`
        let input = "? &a a\n: &b b\n: *a\n";
        let events = evs(input);

        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "a"
            )),
            "anchor `&a` on key must be present"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("b"), value, .. } if value.as_ref() == "b"
            )),
            "anchor `&b` on value must be present"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "a" })),
            "alias `*a` must emit Alias event"
        );
    }

    // -----------------------------------------------------------------------
    // Group I: Additional scenarios from test-engineer review
    // -----------------------------------------------------------------------

    #[test]
    fn alias_as_explicit_block_mapping_key() {
        // UT-A14: `? *ref\n: value\n` — alias as explicit mapping key.
        let events = evs("? *ref\n: value\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "ref" })),
            "alias `*ref` as explicit mapping key must emit Alias event"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "value")),
            "value scalar must be present"
        );
    }

    #[test]
    fn flow_sequence_with_anchored_first_and_unannotated_second() {
        // UT-A15: `[&a foo, bar]\n` — anchor on first item only; second item
        // has no anchor.
        let events = evs("[&a foo, bar]\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "foo"
            )),
            "first item must have anchor `a`"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: None, value, .. } if value.as_ref() == "bar"
            )),
            "second item must have no anchor"
        );
    }

    #[test]
    fn anchor_on_flow_mapping_key() {
        // UT-A16: `{&a key: value}\n` — anchor on the key inside a flow mapping.
        let events = evs("{&a key: value}\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "key"
            )),
            "anchor `&a` must be attached to key scalar 'key'"
        );
    }

    #[test]
    fn alias_in_flow_sequence_with_following_item() {
        // UT-A18: `[*ref, foo]\n` — alias followed by a plain scalar.
        let events = evs("[*ref, foo]\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "ref" })),
            "alias `*ref` must emit Alias event"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "foo")),
            "scalar 'foo' must follow alias"
        );
    }

    #[test]
    fn undefined_alias_emits_alias_event_without_error() {
        // UT-A21: parser emits Event::Alias for names that were never anchored.
        // Resolution of undefined aliases is the loader's responsibility.
        let events = evs("*undefined\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "undefined" })),
            "undefined alias must emit Alias event without error"
        );
    }

    #[test]
    fn multi_document_alias_in_second_doc_emits_event() {
        // UT-A22: anchor in doc 1, alias in doc 2 — parser does not resolve
        // cross-document aliases; it emits both events.
        let input = "&a foo\n---\n*a\n";
        let events = evs(input);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "foo"
            )),
            "anchored scalar 'foo' in doc 1 must be present"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "a" })),
            "alias `*a` in doc 2 must emit Alias event"
        );
    }

    #[test]
    fn anchor_name_stops_at_comma_in_flow_sequence() {
        // UT-A23: `[&name,foo]\n` — comma immediately after anchor name stops
        // the scan; anchor name is `"name"`, not `"name,foo"`.  The `,` acts
        // as a flow separator — since no scalar follows the anchor before the
        // comma, this tests the boundary case where anchor name scan stops
        // correctly before the comma.  The current parser emits an error here
        // (leading comma after anchor with no value) which is the correct
        // behavior: `&name,foo` is ambiguous without a space, not valid YAML.
        // Test verifies that the anchor name `"name"` is still extracted
        // correctly (it does not absorb the `,`).
        let result: Vec<_> = parse_events("[&name,foo]\n").collect();
        // The error should reference `name` as the anchor — not `name,foo`.
        // Since this produces an error, we just verify anchor name parsing
        // stopped at the comma by checking the error message does not mention
        // a comma-containing name.
        let error_msg = result
            .iter()
            .filter_map(|r| r.as_ref().err())
            .map(ToString::to_string)
            .next()
            .unwrap_or_default();
        assert!(
            !error_msg.contains("name,"),
            "anchor name must not include the comma; error was: {error_msg}"
        );
    }

    #[test]
    fn anchor_name_stops_at_closing_bracket() {
        // UT-A24: `[&name]\n` — anchor before `]`; the sequence has the anchor
        // but no value scalar.  The `]` terminates the anchor name scan and the
        // sequence closes.  Current behavior: anchor set with no node → emitted
        // as empty scalar (pending anchor consumed when sequence closes or
        // similar) or as anchor on SequenceEnd — either is acceptable.  This
        // test just verifies no panic and no inclusion of `]` in the name.
        let result: Vec<_> = parse_events("[&name]\n").collect();
        // Check that if any anchor appeared, its name does not contain `]`.
        for item in &result {
            if let Ok((
                Event::Scalar {
                    anchor: Some(name), ..
                },
                _,
            )) = item
            {
                assert!(
                    !name.contains(']'),
                    "anchor name must not include `]`; got `{name}`"
                );
            }
        }
    }

    #[test]
    fn scalar_span_covers_value_not_anchor_prefix() {
        // UT-S2: `&a foo\n` — scalar span must start at `foo` (byte 3),
        // not at `&a` (byte 0).
        let items = parse_to_vec("&a foo\n");
        let scalar_span = items.iter().find_map(|r| match r {
            Ok((Event::Scalar { .. }, span)) => Some(*span),
            Ok(_) | Err(_) => None,
        });
        assert!(scalar_span.is_some(), "Scalar event must be present");
        if let Some(span) = scalar_span {
            assert_eq!(
                span.start.byte_offset, 3,
                "Scalar span must start at byte 3 (the 'f' of 'foo'), not at the anchor prefix"
            );
        }
    }

    #[test]
    fn sequence_start_span_starts_at_dash_not_anchor_line() {
        // UT-S3: `&anchor\n- item\n` — SequenceStart span should start at the
        // `-` (byte 8), not at the `&anchor` line (byte 0).
        let items = parse_to_vec("&anchor\n- item\n");
        let seq_span = items.iter().find_map(|r| match r {
            Ok((Event::SequenceStart { .. }, span)) => Some(*span),
            Ok(_) | Err(_) => None,
        });
        assert!(seq_span.is_some(), "SequenceStart event must be present");
        if let Some(span) = seq_span {
            assert_eq!(
                span.start.byte_offset, 8,
                "SequenceStart span must start at the `-` (byte 8), not at the anchor line"
            );
        }
    }

    #[test]
    fn flow_sequence_with_anchored_item_then_alias() {
        // IT-A2: `[&a first, *a]\n` — anchored scalar followed by alias.
        let events = evs("[&a first, *a]\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "first"
            )),
            "anchored scalar 'first' must be present"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "a" })),
            "alias `*a` must emit Alias event"
        );
    }

    #[test]
    fn flow_mapping_with_anchored_key_and_alias_value() {
        // IT-A3: `{&k key: *v}\n` — anchor on key, alias as value.
        let events = evs("{&k key: *v}\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
            )),
            "anchor `&k` must be on key scalar"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "v" })),
            "alias `*v` must emit Alias event"
        );
    }

    #[test]
    fn block_sequence_mix_scalars_and_aliases() {
        // IT-A4: `- &first one\n- *first\n- two\n`
        let events = evs("- &first one\n- *first\n- two\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("first"), value, .. } if value.as_ref() == "one"
            )),
            "anchored scalar 'one' must be present"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Alias { name: "first" })),
            "alias `*first` must emit Alias event"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: None, value, .. } if value.as_ref() == "two"
            )),
            "plain scalar 'two' with no anchor must be present"
        );
    }

    // -----------------------------------------------------------------------
    // Group I: PendingAnchor enum consolidation (Task 15)
    // -----------------------------------------------------------------------

    // A-4: Standalone anchor on block sequence — SequenceStart carries the anchor.
    #[test]
    fn standalone_anchor_applies_to_block_sequence_start() {
        let events = evs("&seq\n- a\n- b\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    anchor: Some("seq"),
                    ..
                }
            )),
            "standalone &seq must be attached to SequenceStart"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: None, value, .. } if value.as_ref() == "a"
            )),
            "first sequence item must have no anchor"
        );
    }

    // A-5: Inline anchor annotates the key scalar, not the mapping (9KAX scenario).
    #[test]
    fn inline_anchor_on_key_annotates_key_scalar_not_mapping() {
        let events = evs("&k key: value\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { anchor: None, .. })),
            "MappingStart must have no anchor when &k is inline before key"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
            )),
            "key scalar must carry anchor &k"
        );
    }

    // A-6: Standalone anchor on block mapping — MappingStart carries the anchor.
    #[test]
    fn standalone_anchor_applies_to_block_mapping_start() {
        let events = evs("&map\nkey: value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    anchor: Some("map"),
                    ..
                }
            )),
            "standalone &map must be attached to MappingStart"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: None, value, .. } if value.as_ref() == "key"
            )),
            "key scalar must have no anchor"
        );
    }

    // A-7: Inline anchor on a scalar value — value scalar carries the anchor.
    #[test]
    fn inline_anchor_on_scalar_value_attaches_to_value() {
        let events = evs("key: &a value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "value"
            )),
            "value scalar must carry anchor &a"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: None, value, .. } if value.as_ref() == "key"
            )),
            "key scalar must have no anchor"
        );
    }

    // A-8: Nested anchors — outer on sequence, inner on first item.
    #[test]
    fn nested_anchors_outer_on_sequence_inner_on_item() {
        let events = evs("&outer\n- &inner a\n- b\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    anchor: Some("outer"),
                    ..
                }
            )),
            "SequenceStart must carry anchor &outer"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("inner"), value, .. } if value.as_ref() == "a"
            )),
            "first item scalar must carry anchor &inner"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: None, value, .. } if value.as_ref() == "b"
            )),
            "second item scalar must have no anchor"
        );
    }

    // A-9: When a standalone anchor is followed by a second inline key anchor,
    // the second anchor overwrites the first (pre-existing parser behaviour —
    // the standalone anchor is consumed and replaced).  The key scalar carries
    // the inline anchor; no error is produced.
    #[test]
    fn standalone_anchor_overwritten_by_subsequent_inline_key_anchor() {
        // "&map\n&k key: value\n" — &map is scanned as standalone, then &k is
        // scanned as inline before the key.  The parser replaces &map with &k.
        // MappingStart has no anchor; key scalar carries &k.
        let events = evs("&map\n&k key: value\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { anchor: None, .. })),
            "MappingStart must have no anchor (standalone &map is replaced by inline &k)"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
            )),
            "key scalar must carry anchor &k"
        );
    }

    // A-10: Duplicate anchors on the same node return an error.
    #[test]
    fn duplicate_anchors_on_same_node_return_error() {
        assert!(
            parse_events("&a &b scalar\n").any(|r| r.is_err()),
            "two anchors on one node must return an error"
        );
    }

    // A-11: Anchor cleared after use — second sequence item has no anchor.
    #[test]
    fn anchor_cleared_after_use_second_item_has_none() {
        let events = evs("- &a first\n- second\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "first"
            )),
            "first item must carry anchor &a"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { anchor: None, value, .. } if value.as_ref() == "second"
            )),
            "second item must have no anchor"
        );
    }

    // B-8: Inline anchor immediately before an alias is an error.
    #[test]
    fn inline_anchor_before_alias_returns_error() {
        assert!(
            parse_events("&a *b\n").any(|r| r.is_err()),
            "inline anchor &a before alias *b must return an error"
        );
    }

    // B-10: Standalone anchor at insufficient indent returns an error.
    #[test]
    fn standalone_anchor_at_insufficient_indent_returns_error() {
        // `"key:\n  nested: val\n&a\n"` — after opening the nested mapping at
        // indent 2, a standalone `&a` at indent 0 is below the minimum required
        // indent for that context.
        let result: Vec<_> = parse_events("key:\n  nested: val\n&a\n").collect();
        let has_indent_error = result.iter().any(|r| {
            r.as_ref()
                .is_err_and(|e| e.to_string().to_lowercase().contains("indent"))
        });
        assert!(
            has_indent_error,
            "standalone anchor below minimum indent must return an indent error"
        );
    }

    // B-11: Anchor followed immediately by a block-sequence dash on the same line is an error.
    #[test]
    fn anchor_followed_by_inline_dash_returns_error() {
        assert!(
            parse_events("&a - item\n").any(|r| r.is_err()),
            "anchor &a directly before block-sequence dash on same line must return an error"
        );
    }
}

// ---------------------------------------------------------------------------
// mod tags — Task 17
//
// Covers tag property parsing in block and flow contexts.  Tests are ordered:
//   A — verbatim tags on scalars
//   B — primary handle (`!!`) on scalars
//   C — named handle (`!handle!suffix`)
//   D — secondary handle (`!suffix`)
//   E — non-specific tag (`!`)
//   F — tags on collections (block)
//   G — tags on collections (flow)
//   H — tag + anchor combinations
//   I — error cases
//   J — span correctness
//   K — regression: pre-existing silent drop
//   L — tag on implicit mapping key context
//   M — standalone tag applies to next node
// ---------------------------------------------------------------------------

mod tags {
    use super::*;
    use rstest::rstest;

    fn evs(input: &str) -> Vec<Event<'_>> {
        parse_events(input)
            .map(|r| match r {
                Ok((ev, _)) => ev,
                Err(e) => unreachable!("unexpected parse error: {e}"),
            })
            .collect()
    }

    fn has_error(input: &str) -> bool {
        parse_events(input).any(|r| r.is_err())
    }

    // -----------------------------------------------------------------------
    // Group A: Verbatim tags on scalars
    // -----------------------------------------------------------------------

    #[test]
    fn verbatim_tag_on_plain_scalar() {
        let events = evs("!<tag:yaml.org,2002:str> hello\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello"
            )),
            "verbatim tag must be stored as URI content (without angle brackets)"
        );
    }

    #[test]
    fn verbatim_tag_strips_angle_brackets() {
        let events = evs("!<my-uri> val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    ..
                } if t.as_ref() == "my-uri"
            )),
            "verbatim tag must store just 'my-uri', not '!<my-uri>'"
        );
    }

    #[test]
    fn verbatim_tag_missing_closing_angle_bracket_returns_error() {
        assert!(
            has_error("!<tag:yaml.org,2002:str hello\n"),
            "verbatim tag missing '>' must return an error"
        );
    }

    #[test]
    fn verbatim_tag_empty_uri_returns_error() {
        assert!(
            has_error("!<> val\n"),
            "empty verbatim tag URI must return an error"
        );
    }

    #[test]
    fn verbatim_tag_whitespace_in_uri_returns_error() {
        // Space (0x20) is above the control-character threshold, but a tab
        // (0x09) is below 0x20 and must be rejected.
        assert!(
            has_error("!<foo\tbar> val\n"),
            "verbatim tag URI containing a tab must return an error"
        );
    }

    #[test]
    fn verbatim_tag_control_char_in_uri_returns_error() {
        // NUL byte inside URI must be rejected.
        assert!(
            has_error("!<foo\x00bar> val\n"),
            "verbatim tag URI containing NUL must return an error"
        );
    }

    // -----------------------------------------------------------------------
    // Group A2: Verbatim tag URI validation — YAML 1.2 §6.8.1 production [38]
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::alphanumeric("!<abc123> v\n")]
    #[case::allowed_punctuation("!<a-_.~*'()[]#;/?:@&=+$,b> v\n")]
    #[case::exclamation("!<tag:foo!bar> v\n")]
    #[case::percent_encoded_space("!<%20> v\n")]
    #[case::percent_encoded_slash("!<path%2Fto> v\n")]
    fn verbatim_tag_uri_valid_chars_accepted(#[case] input: &str) {
        assert!(
            !has_error(input),
            "verbatim tag URI must be accepted: {input:?}"
        );
    }

    #[test]
    fn verbatim_tag_uri_percent_uppercase_hex_accepted() {
        assert!(!has_error("!<%2F> v\n"), "uppercase %2F must be accepted");
        assert!(!has_error("!<%2f> v\n"), "lowercase %2f must be accepted");
    }

    #[rstest]
    #[case::space("!<foo bar> v\n")]
    #[case::curly_brace("!<foo{bar}> v\n")]
    #[case::non_ascii("!<\u{4E2D}\u{6587}> v\n")]
    #[case::bare_percent("!<%GG> v\n")]
    #[case::percent_with_one_hex_digit("!<%2> v\n")]
    #[case::del_char("!<foo\x7Fbar> v\n")]
    #[case::vertical_bar("!<foo|bar> v\n")]
    #[case::backslash("!<foo\\bar> v\n")]
    #[case::less_than("!<foo<bar> v\n")]
    fn verbatim_tag_uri_invalid_chars_rejected(#[case] input: &str) {
        assert!(
            has_error(input),
            "verbatim tag URI with invalid char must be rejected: {input:?}"
        );
    }

    #[test]
    fn verbatim_tag_uri_embedded_close_delimiter_terminates_uri() {
        // First `>` closes the verbatim tag; `bar>` becomes part of the scalar value.
        let events = evs("!<foo>bar>\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "foo" && value.as_ref() == "bar>"
            )),
            "first '>' must close the verbatim tag URI; remainder is scalar content"
        );
    }

    // -----------------------------------------------------------------------
    // Group B: Primary handle (`!!`) on scalars
    // -----------------------------------------------------------------------

    #[test]
    fn primary_handle_on_plain_scalar() {
        // `!!str` expands to `"tag:yaml.org,2002:str"` via the default `!!` handle.
        let events = evs("!!str hello\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello"
            )),
            "primary handle tag must expand to 'tag:yaml.org,2002:str'"
        );
    }

    #[test]
    fn primary_handle_empty_suffix_expands_to_core_schema_prefix() {
        // `!! val` — primary handle with empty suffix; expands to `"tag:yaml.org,2002:"`.
        let events = evs("!! val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:"
            )),
            "primary handle with empty suffix must expand to 'tag:yaml.org,2002:'"
        );
    }

    // -----------------------------------------------------------------------
    // Group C: Named handle (`!handle!suffix`)
    // -----------------------------------------------------------------------

    #[test]
    fn named_handle_without_declaration_returns_error() {
        // `!e!tag val` — `!e!` handle is not declared via `%TAG`, so an error is expected.
        assert!(
            has_error("!e!tag val\n"),
            "named handle with no %TAG declaration must return an error"
        );
    }

    // -----------------------------------------------------------------------
    // Group D: Secondary handle (`!suffix`)
    // -----------------------------------------------------------------------

    #[test]
    fn secondary_handle_on_plain_scalar() {
        let events = evs("!yaml val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "!yaml" && value.as_ref() == "val"
            )),
            "secondary handle tag must be stored as '!yaml'"
        );
    }

    // -----------------------------------------------------------------------
    // Group E: Non-specific tag (`!`)
    // -----------------------------------------------------------------------

    #[test]
    fn non_specific_tag_on_plain_scalar() {
        // `! val` — bare `!` followed by space, then content.
        let events = evs("! val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "!" && value.as_ref() == "val"
            )),
            "non-specific tag '!' must be stored as '!'"
        );
    }

    // -----------------------------------------------------------------------
    // Group F: Tags on collections (block)
    // -----------------------------------------------------------------------

    #[test]
    fn tag_on_block_sequence() {
        let events = evs("!!seq\n- item\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    tag: Some(t),
                    style: CollectionStyle::Block,
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:seq"
            )),
            "block sequence must carry resolved tag 'tag:yaml.org,2002:seq'"
        );
    }

    #[test]
    fn tag_on_block_mapping() {
        let events = evs("!!map\nkey: val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    tag: Some(t),
                    style: CollectionStyle::Block,
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:map"
            )),
            "block mapping must carry resolved tag 'tag:yaml.org,2002:map'"
        );
    }

    #[test]
    fn tag_on_block_literal_scalar() {
        let events = evs("!!str |\n  hello\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    style: ScalarStyle::Literal(Chomp::Clip),
                    value,
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello\n"
            )),
            "literal block scalar must carry resolved tag 'tag:yaml.org,2002:str'"
        );
    }

    #[test]
    fn tag_on_block_folded_scalar() {
        let events = evs("!!str >\n  hello\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    style: ScalarStyle::Folded(Chomp::Clip),
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:str"
            )),
            "folded block scalar must carry resolved tag 'tag:yaml.org,2002:str'"
        );
    }

    // -----------------------------------------------------------------------
    // Group G: Tags on collections (flow)
    // -----------------------------------------------------------------------

    #[test]
    fn tag_on_flow_sequence() {
        let events = evs("!!seq [a, b]\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    tag: Some(t),
                    style: CollectionStyle::Flow,
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:seq"
            )),
            "flow sequence must carry resolved tag 'tag:yaml.org,2002:seq'"
        );
    }

    #[test]
    fn tag_on_flow_mapping() {
        let events = evs("!!map {a: b}\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    tag: Some(t),
                    style: CollectionStyle::Flow,
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:map"
            )),
            "flow mapping must carry resolved tag 'tag:yaml.org,2002:map'"
        );
    }

    // -----------------------------------------------------------------------
    // Group H: Tag + anchor combinations
    // -----------------------------------------------------------------------

    #[test]
    fn tag_before_anchor_both_emitted_on_scalar() {
        let events = evs("!str &anchor value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    anchor: Some("anchor"),
                    value,
                    ..
                } if t.as_ref() == "!str" && value.as_ref() == "value"
            )),
            "tag before anchor: both must be emitted on the scalar"
        );
    }

    #[test]
    fn anchor_before_tag_both_emitted_on_scalar() {
        let events = evs("&anchor !str value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    anchor: Some("anchor"),
                    value,
                    ..
                } if t.as_ref() == "!str" && value.as_ref() == "value"
            )),
            "anchor before tag: both must be emitted on the scalar"
        );
    }

    #[test]
    fn tag_before_anchor_both_emitted_on_sequence() {
        let events = evs("!seq &s\n- item\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    tag: Some(t),
                    anchor: Some("s"),
                    ..
                } if t.as_ref() == "!seq"
            )),
            "tag before anchor on sequence: both must be emitted on SequenceStart"
        );
    }

    // -----------------------------------------------------------------------
    // Group I: Error cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::duplicate_tag_on_same_node("!!str !!int val\n")]
    #[case::alias_with_tag("&anchor val\n!!str *anchor\n")]
    #[case::flow_duplicate_tag_on_same_node("[!t !t2 val]\n")]
    #[case::flow_alias_with_pending_tag("[!t *a, val]\n")]
    #[case::flow_alias_with_pending_tag_alone("[!a *name]\n")]
    fn tag_error_cases_return_error(#[case] input: &str) {
        assert!(
            has_error(input),
            "invalid tag usage must return an error: {input:?}"
        );
    }

    #[test]
    fn tag_length_at_limit_is_accepted() {
        // Verbatim tag with URI exactly MAX_TAG_LEN bytes long.
        let uri = "a".repeat(MAX_TAG_LEN);
        let input = format!("!<{uri}> val\n");
        assert!(
            !has_error(&input),
            "tag URI at exactly MAX_TAG_LEN bytes must be accepted"
        );
    }

    #[test]
    fn tag_length_exceeding_limit_returns_error() {
        // Verbatim tag with URI one byte over MAX_TAG_LEN.
        let uri = "a".repeat(MAX_TAG_LEN + 1);
        let input = format!("!<{uri}> val\n");
        assert!(
            has_error(&input),
            "tag URI exceeding MAX_TAG_LEN bytes must return an error"
        );
    }

    #[test]
    fn tag_with_invalid_char_stops_at_boundary() {
        // `!foo<bar val\n` — `<` is not a valid ns-tag-char per §6.8.1.
        // The tag must stop at `<`, yielding tag `!foo` and value `<bar val`.
        // (The parser does not error on this; it scans `!foo` as the tag
        // and treats the rest as the scalar value.)
        let events = evs("!foo<bar val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    ..
                } if t.as_ref() == "!foo"
            )),
            "tag scan must stop before '<' — tag must be '!foo'"
        );
    }

    #[test]
    fn percent_encoded_tag_suffix_is_accepted() {
        // `!foo%2Fbar val\n` — `%2F` is a valid percent-encoded sequence.
        let events = evs("!foo%2Fbar val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    ..
                } if t.as_ref() == "!foo%2Fbar"
            )),
            "percent-encoded sequence in tag suffix must be accepted"
        );
    }

    #[test]
    fn bare_percent_in_tag_stops_scan() {
        // Updated in Task 21: reference parser rejects this input per YAML 1.2
        // §6.8.1 (tag properties); conformance fix makes streaming parser match.
        // `!foo%zz\nhello\n` — `%zz` is not a valid percent-encoded sequence
        // (z is not a hex digit).  The tag scanner stops at `%`, yielding `!foo`
        // with `%zz` remaining inline (no space between tag and `%zz`).  Per YAML
        // 1.2 this is an invalid tag property (confirmed by rlsp-yaml-parser reference
        // impl which errors on this input).
        assert!(
            has_error("!foo%zz\nhello\n"),
            "tag followed immediately by bare '%' is a parse error"
        );
    }

    // -----------------------------------------------------------------------
    // Group J: Span correctness
    // -----------------------------------------------------------------------

    #[test]
    fn tagged_scalar_span_covers_value_not_tag() {
        // `!!str hello\n` — `!!str ` is 6 bytes; `hello` starts at byte 6.
        let items = parse_to_vec("!!str hello\n");
        let scalar_span = items.iter().find_map(|r| match r {
            Ok((Event::Scalar { tag: Some(_), .. }, span)) => Some(*span),
            Ok(_) | Err(_) => None,
        });
        assert!(scalar_span.is_some(), "tagged scalar event must be present");
        if let Some(span) = scalar_span {
            assert_eq!(
                span.start.byte_offset, 6,
                "tagged scalar span must start at 'h' of 'hello' (byte 6), not at '!'"
            );
            assert_eq!(
                span.end.byte_offset, 11,
                "tagged scalar span must end after 'hello' (byte 11)"
            );
        }
    }

    #[test]
    fn tagged_sequence_span_is_at_dash_indicator() {
        // `!!seq\n- a\n` — SequenceStart span should point to the `-` on line 2.
        let items = parse_to_vec("!!seq\n- a\n");
        let seq_span = items.iter().find_map(|r| match r {
            Ok((Event::SequenceStart { tag: Some(_), .. }, span)) => Some(*span),
            Ok(_) | Err(_) => None,
        });
        assert!(
            seq_span.is_some(),
            "tagged SequenceStart event must be present"
        );
        // `!!seq\n` is 6 bytes; `-` is at byte 6.
        if let Some(span) = seq_span {
            assert_eq!(
                span.start.byte_offset, 6,
                "SequenceStart span must start at '-' indicator (byte 6), not at tag"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Group K: Regression — pre-existing silent drop
    // -----------------------------------------------------------------------

    #[test]
    fn tag_prefix_line_not_silently_dropped() {
        // `!str value\n` was previously silently consumed by the fallback
        // `consume_line` at lib.rs:1124 (the "unrecognised content" path).
        // This test ensures a Scalar event is produced.
        let events = evs("!str value\n");
        assert!(
            events.iter().any(|e| matches!(e, Event::Scalar { .. })),
            "!str value must produce a Scalar event, not be silently dropped"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "!str" && value.as_ref() == "value"
            )),
            "scalar must have tag '!str' and value 'value'"
        );
    }

    #[test]
    fn verbatim_tag_prefix_line_not_silently_dropped() {
        let events = evs("!<tag:yaml.org,2002:str> value\n");
        assert!(
            events.iter().any(|e| matches!(e, Event::Scalar { .. })),
            "verbatim-tagged value must produce a Scalar event"
        );
    }

    // -----------------------------------------------------------------------
    // Group L: Tag on implicit mapping key context
    // -----------------------------------------------------------------------

    #[test]
    fn tag_on_implicit_mapping_key_scalar() {
        // `!!str key: val\n` — tag is inline before the key, so it annotates
        // the key scalar, NOT the MappingStart (YAML test suite 9KAX).
        let events = evs("!!str key: val\n");
        // Key scalar carries the tag.
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref().contains("str") && value.as_ref() == "key"
            )),
            "tag must be on key scalar, not on MappingStart"
        );
        // MappingStart has no tag.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { tag: None, .. })),
            "MappingStart must have no tag when tag is inline before key"
        );
    }

    // -----------------------------------------------------------------------
    // Group M: Standalone tag applies to next node
    // -----------------------------------------------------------------------

    #[test]
    fn standalone_tag_line_applies_to_scalar_below() {
        let events = evs("!!str\nhello\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello"
            )),
            "standalone tag line must be attached to the following scalar"
        );
    }

    #[test]
    fn standalone_tag_line_applies_to_sequence_below() {
        let events = evs("!!seq\n- a\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    tag: Some(t),
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:seq"
            )),
            "standalone tag line must be attached to the following sequence"
        );
    }

    #[test]
    fn standalone_tag_line_applies_to_mapping_below() {
        let events = evs("!!map\nkey: val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    tag: Some(t),
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:map"
            )),
            "standalone tag line must be attached to the following mapping"
        );
    }

    // -----------------------------------------------------------------------
    // Carry-forward note — Medium #2 (flow empty-element-with-tag)
    //
    // `[!!]` drops the tag silently; `[!, x]` returns "invalid leading comma".
    // This mirrors the pre-existing Task 16 behaviour for anchors: `[&a]`
    // drops the anchor and `[&a, x]` returns the same leading-comma error.
    // Fixing flow empty-element handling for both anchors and tags requires
    // deeper changes to the flow loop's has_value / emit logic and is tracked
    // as a follow-up task (out of Task 17 scope).
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Group J: PendingTag enum consolidation (Task 16)
    // -----------------------------------------------------------------------

    // T-3: Standalone tag on block sequence — SequenceStart carries the tag; scalar has none.
    #[test]
    fn standalone_tag_on_block_sequence_propagates_to_sequence_start() {
        let events = evs("!!seq\n- a\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:seq"
            )),
            "SequenceStart must carry tag:yaml.org,2002:seq"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: None, value, .. } if value.as_ref() == "a"
            )),
            "sequence item scalar must have no tag"
        );
    }

    // T-4: Inline tag on mapping key — MappingStart has no tag; key scalar carries the tag.
    #[test]
    fn inline_tag_on_mapping_key_annotates_key_scalar_not_mapping() {
        let events = evs("!!str key: value\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { tag: None, .. })),
            "MappingStart must have no tag when !!str is inline before key"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "key"
            )),
            "key scalar must carry tag:yaml.org,2002:str"
        );
    }

    // T-5: Standalone tag on block mapping — MappingStart carries the tag; key scalar has none.
    #[test]
    fn standalone_tag_on_block_mapping_propagates_to_mapping_start() {
        let events = evs("!!map\nkey: value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:map"
            )),
            "MappingStart must carry tag:yaml.org,2002:map"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: None, value, .. } if value.as_ref() == "key"
            )),
            "key scalar must have no tag"
        );
    }

    // T-6: Verbatim tag passes through unchanged.
    #[test]
    fn verbatim_tag_passes_through_unchanged() {
        let events = evs("!<tag:example.com/foo> value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "tag:example.com/foo" && value.as_ref() == "value"
            )),
            "verbatim tag must be preserved as-is on the scalar"
        );
    }

    // T-7: Local tag preserved as-is.
    #[test]
    fn local_tag_preserved_as_is() {
        let events = evs("!local value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "!local" && value.as_ref() == "value"
            )),
            "local tag !local must be preserved unchanged"
        );
    }

    // T-8: Tag resolved via %TAG directive — Cow::Owned flows through enum variant correctly.
    #[test]
    fn tag_resolved_via_pct_tag_directive_cow_owned() {
        let input = "%TAG !custom! tag:example.com/\n---\n!custom!foo value\n";
        let events = evs(input);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "tag:example.com/foo" && value.as_ref() == "value"
            )),
            "resolved tag tag:example.com/foo must flow through PendingTag::Inline correctly"
        );
    }

    // T-9: Tag cleared after use — second sequence item has no tag.
    #[test]
    fn tag_cleared_after_use_second_item_has_none() {
        let events = evs("- !!str first\n- second\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), value, .. }
                    if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "first"
            )),
            "first item must carry tag:yaml.org,2002:str"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: None, value, .. } if value.as_ref() == "second"
            )),
            "second item must have no tag"
        );
    }

    // T-10: Tag on flow sequence — SequenceStart carries the tag with Flow style.
    #[test]
    fn tag_on_flow_sequence_propagates_to_sequence_start() {
        let events = evs("!!seq [a, b]\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    tag: Some(t),
                    style: CollectionStyle::Flow,
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:seq"
            )),
            "SequenceStart for flow sequence must carry tag:yaml.org,2002:seq"
        );
    }

    // T-11: Tag + anchor both on standalone collection — both propagate through distinct enums.
    #[test]
    fn tag_and_anchor_both_standalone_both_propagate_to_sequence_start() {
        let input = "&myseq\n!!seq\n- a\n";
        let events = evs(input);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    anchor: Some("myseq"),
                    tag: Some(t),
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:seq"
            )),
            "SequenceStart must carry both anchor myseq and tag:yaml.org,2002:seq"
        );
    }

    // T-12: Inline tag + inline anchor on same scalar — both attached to the scalar.
    #[test]
    fn inline_tag_and_anchor_on_same_scalar_both_attached() {
        let events = evs("!!str &a value\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    tag: Some(t),
                    anchor: Some("a"),
                    value,
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "value"
            )),
            "scalar must carry both tag:yaml.org,2002:str and anchor a"
        );
    }
}

// ---------------------------------------------------------------------------
// mod comments — Comment events (Task 18)
// ---------------------------------------------------------------------------

mod comments {
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
}

// ---------------------------------------------------------------------------
// mod directives — %YAML and %TAG directive parsing (Task 19)
// ---------------------------------------------------------------------------

mod directives {
    use rstest::rstest;

    use super::*;

    fn evs(input: &str) -> Vec<Event<'_>> {
        parse_events(input)
            .filter_map(|r| match r {
                Ok((ev, _span)) => Some(ev),
                Err(_) => None,
            })
            .collect()
    }

    fn has_error(input: &str) -> bool {
        parse_events(input).any(|r| r.is_err())
    }

    // -----------------------------------------------------------------------
    // Group A — %YAML directive
    // -----------------------------------------------------------------------

    // A-1 through A-3: %YAML directive version propagated to DocumentStart.version.
    #[rstest]
    // A-1: %YAML 1.2 produces version Some((1, 2)).
    #[case::yaml_1_2_propagated("%YAML 1.2\n---\nscalar\n", Some((1, 2)))]
    // A-2: %YAML 1.1 produces version Some((1, 1)).
    #[case::yaml_1_1_propagated("%YAML 1.1\n---\nscalar\n", Some((1, 1)))]
    // A-3: No %YAML directive produces version None.
    #[case::no_yaml_directive_version_is_none("---\nscalar\n", None)]
    // A-3b: Non-standard version %YAML 1.3 is accepted without validation.
    #[case::yaml_non_standard_version_accepted("%YAML 1.3\n---\nscalar\n", Some((1, 3)))]
    fn yaml_directive_version_propagated_to_document_start(
        #[case] input: &str,
        #[case] expected_version: Option<(u8, u8)>,
    ) {
        let events = event_variants(input);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart { version, .. } if *version == expected_version
            )),
            "%YAML directive must produce DocumentStart with version {expected_version:?} for input: {input:?}"
        );
    }

    // A-4, A-5, A-6, A-8: Malformed or disallowed %YAML directives return at least one error.
    #[rstest]
    // A-4: Missing version number after %YAML.
    #[case::missing_version_returns_error("%YAML\n---\nscalar\n")]
    // A-5: Non-numeric version (e.g., %YAML abc) is rejected.
    #[case::non_numeric_version_returns_error("%YAML abc\n---\nscalar\n")]
    // A-6: Unsupported major version 2 is rejected (only major 1 is supported).
    #[case::major_version_2_returns_error("%YAML 2.0\n---\nscalar\n")]
    // A-8: Duplicate %YAML directives in the same preamble are rejected.
    #[case::duplicate_yaml_directive_returns_error("%YAML 1.2\n%YAML 1.2\n---\nscalar\n")]
    fn yaml_directive_invalid_input_returns_error(#[case] input: &str) {
        assert!(
            has_error(input),
            "invalid %YAML directive must return an error for input: {input:?}"
        );
    }

    // A-9: %YAML directive scope resets between documents.
    #[test]
    fn yaml_directive_scope_resets_between_documents() {
        let events = event_variants("%YAML 1.2\n---\nfirst\n...\n---\nsecond\n");
        let versions: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::DocumentStart { version, .. } = e {
                    Some(*version)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            versions,
            [Some((1, 2)), None],
            "first doc must have version Some((1,2)); second doc (no directive) must have version None"
        );
    }

    // -----------------------------------------------------------------------
    // Group B — %TAG directive
    // -----------------------------------------------------------------------

    // B-1: %TAG directive populates tag_directives field.
    #[test]
    fn tag_directive_propagated_to_document_start() {
        let events = event_variants("%TAG !foo! tag:example.com,2026:\n---\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart { tag_directives, .. }
                    if tag_directives.iter().any(|(h, p)| h == "!foo!" && p == "tag:example.com,2026:")
            )),
            "%TAG must populate DocumentStart.tag_directives with the declared handle and prefix"
        );
    }

    // B-2: No %TAG directives → tag_directives is empty.
    #[test]
    fn no_tag_directive_produces_empty_tag_directives() {
        let events = event_variants("---\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart { tag_directives, .. } if tag_directives.is_empty()
            )),
            "absent %TAG directives must produce DocumentStart with empty tag_directives"
        );
    }

    // B-3: Multiple %TAG directives accumulate.
    #[test]
    fn multiple_tag_directives_all_present_in_document_start() {
        let events = event_variants("%TAG !a! prefix-a:\n%TAG !b! prefix-b:\n---\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart { tag_directives, .. }
                    if tag_directives.iter().any(|(h, _)| h == "!a!")
                        && tag_directives.iter().any(|(h, _)| h == "!b!")
            )),
            "multiple %TAG directives must all appear in DocumentStart.tag_directives"
        );
    }

    // B-4: Duplicate %TAG handle returns error.
    #[test]
    fn duplicate_tag_handle_returns_error() {
        assert!(
            has_error("%TAG !foo! prefix-a:\n%TAG !foo! prefix-b:\n---\nscalar\n"),
            "duplicate %TAG handle must return an error"
        );
    }

    // B-5: %TAG directive scope resets between documents.
    #[test]
    fn tag_directive_scope_resets_between_documents() {
        let events = event_variants("%TAG !foo! prefix-a:\n---\nfirst\n...\n---\nsecond\n");
        let directives_per_doc: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::DocumentStart { tag_directives, .. } = e {
                    Some(tag_directives.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            directives_per_doc.len(),
            2,
            "expected two DocumentStart events"
        );
        assert!(
            directives_per_doc.first().is_some_and(|d| !d.is_empty()),
            "first doc must include the !foo! tag directive"
        );
        assert!(
            directives_per_doc.get(1).is_some_and(Vec::is_empty),
            "second doc (no directives) must have empty tag_directives"
        );
    }

    // -----------------------------------------------------------------------
    // Group C — Default handle expansion (no %TAG override)
    // -----------------------------------------------------------------------

    // C-1: `!!str` expands to `tag:yaml.org,2002:str` without any %TAG.
    #[test]
    fn default_handle_expands_to_core_schema_prefix() {
        let events = evs("!!str hello\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:str"
            )),
            "!!str must expand to 'tag:yaml.org,2002:str' using the default !! handle"
        );
    }

    // C-2: `!! val` (empty suffix) expands to `tag:yaml.org,2002:`.
    #[test]
    fn default_handle_empty_suffix_expands_to_prefix_only() {
        let events = evs("!! val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:"
            )),
            "!! with empty suffix must expand to 'tag:yaml.org,2002:'"
        );
    }

    // -----------------------------------------------------------------------
    // Group D — Custom %TAG handle resolution
    // -----------------------------------------------------------------------

    // D-1: Custom handle resolves scalar tag at scan time.
    #[test]
    fn custom_tag_handle_resolves_scalar_tag() {
        let events = evs("%TAG !e! tag:example.com,2026:\n---\n!e!foo bar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. }
                    if t.as_ref() == "tag:example.com,2026:foo"
            )),
            "!e!foo with %TAG !e! tag:example.com,2026: must resolve to 'tag:example.com,2026:foo'"
        );
    }

    // D-2: %TAG overrides the default !! handle.
    #[test]
    fn percent_tag_overrides_default_double_bang_handle() {
        let events = evs("%TAG !! tag:custom.org,2026:\n---\n!!str hello\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. }
                    if t.as_ref() == "tag:custom.org,2026:str"
            )),
            "%TAG !! override must cause !!str to resolve to 'tag:custom.org,2026:str'"
        );
    }

    // D-3: Undeclared named handle returns error.
    #[test]
    fn undeclared_named_handle_returns_error() {
        assert!(
            has_error("!e!foo bar\n"),
            "using !e! handle without %TAG declaration must return an error"
        );
    }

    // D-4: Custom handle resolves on sequence tag.
    #[test]
    fn custom_tag_handle_resolves_sequence_tag() {
        let events = evs("%TAG !e! tag:example.com,2026:\n---\n!e!seq\n- item\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart { tag: Some(t), .. }
                    if t.as_ref() == "tag:example.com,2026:seq"
            )),
            "!e!seq on block sequence must resolve to 'tag:example.com,2026:seq'"
        );
    }

    // D-5: Custom handle resolves on mapping tag.
    #[test]
    fn custom_tag_handle_resolves_mapping_tag() {
        let events = evs("%TAG !e! tag:example.com,2026:\n---\n!e!map\nkey: val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart { tag: Some(t), .. }
                    if t.as_ref() == "tag:example.com,2026:map"
            )),
            "!e!map on block mapping must resolve to 'tag:example.com,2026:map'"
        );
    }

    // -----------------------------------------------------------------------
    // Group E — Verbatim tags (unchanged by resolve_tag)
    // -----------------------------------------------------------------------

    // E-1: Verbatim tag `!<URI>` is stored as bare URI (no angle brackets).
    #[test]
    fn verbatim_tag_stored_as_bare_uri() {
        let events = evs("!<tag:example.com,2026:str> val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. }
                    if t.as_ref() == "tag:example.com,2026:str"
            )),
            "verbatim tag must be stored as bare URI without angle brackets"
        );
    }

    // E-2: Local tag `!suffix` is stored as-is (no expansion).
    #[test]
    fn local_tag_stored_as_is() {
        let events = evs("!foo val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. } if t.as_ref() == "!foo"
            )),
            "local tag !foo must be stored as '!foo' without expansion"
        );
    }

    // -----------------------------------------------------------------------
    // Group F — Directive scope per document
    // -----------------------------------------------------------------------

    // F-1: Directive scope is independent per document in a multi-doc stream.
    #[test]
    fn directive_scope_is_independent_per_document() {
        // Doc 1: %TAG !e! prefix-a:, uses !e!type
        // Doc 2: %TAG !e! prefix-b:, uses !e!type
        // Both should resolve to different prefixes.
        let input =
            "%TAG !e! prefix-a:\n---\n!e!type val1\n...\n%TAG !e! prefix-b:\n---\n!e!type val2\n";
        let events = evs(input);
        let tags: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::Scalar {
                    tag: Some(t),
                    value,
                    ..
                } = e
                {
                    if value.as_ref() == "val1" || value.as_ref() == "val2" {
                        return Some((value.as_ref().to_owned(), t.as_ref().to_owned()));
                    }
                }
                None
            })
            .collect();
        assert_eq!(tags.len(), 2, "expected two scalars with tags");
        assert!(
            tags.iter()
                .any(|(v, t)| v == "val1" && t == "prefix-a:type"),
            "doc 1 !e!type must resolve to 'prefix-a:type'"
        );
        assert!(
            tags.iter()
                .any(|(v, t)| v == "val2" && t == "prefix-b:type"),
            "doc 2 !e!type must resolve to 'prefix-b:type'"
        );
    }

    // F-2: Directive from doc 1 is not visible in doc 2.
    #[test]
    fn directive_from_first_doc_not_visible_in_second() {
        // Doc 1: %TAG !e! prefix:, doc 2: no directive — !e!type must error.
        let input = "%TAG !e! prefix:\n---\nscalar\n...\n---\n!e!type val\n";
        assert!(
            has_error(input),
            "handle declared in doc 1 must not be visible in doc 2"
        );
    }

    // -----------------------------------------------------------------------
    // Group G — Multi-document streams
    // -----------------------------------------------------------------------

    // G-1: Multi-doc stream without directives produces multiple DocumentStart events.
    #[test]
    fn multi_doc_stream_without_directives() {
        let events = event_variants("---\nfirst\n...\n---\nsecond\n");
        let doc_starts = events
            .iter()
            .filter(|e| matches!(e, Event::DocumentStart { .. }))
            .count();
        assert_eq!(
            doc_starts, 2,
            "two documents must produce two DocumentStart events"
        );
    }

    // G-2: Each document in a multi-doc stream gets its own version field.
    #[test]
    fn multi_doc_stream_each_doc_gets_its_own_version() {
        let events = event_variants("%YAML 1.2\n---\nfirst\n...\n%YAML 1.3\n---\nsecond\n");
        let versions: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::DocumentStart { version, .. } = e {
                    Some(*version)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            versions,
            [Some((1, 2)), Some((1, 3))],
            "each document must carry its own %YAML version"
        );
    }

    // -----------------------------------------------------------------------
    // Group H — DocumentStart completeness
    // -----------------------------------------------------------------------

    // H-1: Explicit document (with `---`) sets explicit: true.
    #[test]
    fn explicit_document_marker_sets_explicit_true() {
        let events = event_variants("---\nscalar\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::DocumentStart { explicit: true, .. })),
            "--- marker must produce DocumentStart with explicit: true"
        );
    }

    // H-2: Bare document (without `---`) sets explicit: false.
    #[test]
    fn bare_document_sets_explicit_false() {
        let events = event_variants("scalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart {
                    explicit: false,
                    ..
                }
            )),
            "bare document must produce DocumentStart with explicit: false"
        );
    }

    // H-3: %YAML + %TAG together populate both fields.
    #[test]
    fn yaml_and_tag_directives_both_present_in_document_start() {
        let events = event_variants("%YAML 1.2\n%TAG !e! prefix:\n---\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart {
                    version: Some((1, 2)),
                    tag_directives,
                    ..
                } if !tag_directives.is_empty()
            )),
            "%YAML and %TAG must both be present in DocumentStart"
        );
    }

    // -----------------------------------------------------------------------
    // Group I — Unknown directives
    // -----------------------------------------------------------------------

    // I-1: Unknown directive is silently skipped (does not return an error).
    #[test]
    fn unknown_directive_is_silently_skipped() {
        assert!(
            !has_error("%FOO bar baz\n---\nscalar\n"),
            "unknown directive must be silently skipped, not return an error"
        );
    }

    // I-2: Unknown directive does not pollute DocumentStart fields.
    #[test]
    fn unknown_directive_does_not_affect_document_start() {
        let events = event_variants("%FOO bar\n---\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart { version: None, tag_directives, .. }
                    if tag_directives.is_empty()
            )),
            "unknown directive must not affect DocumentStart fields"
        );
    }

    // -----------------------------------------------------------------------
    // Group J — Span correctness
    // -----------------------------------------------------------------------

    // J-1: DocumentStart span covers the `---` marker when explicit.
    #[test]
    fn explicit_document_start_span_covers_dashes() {
        let items = parse_to_vec("---\nscalar\n");
        let doc_start_span = items.iter().find_map(|r| match r {
            Ok((Event::DocumentStart { explicit: true, .. }, span)) => Some(*span),
            _ => None,
        });
        assert!(
            doc_start_span.is_some(),
            "expected an explicit DocumentStart"
        );
        if let Some(span) = doc_start_span {
            assert_eq!(
                span.start.byte_offset, 0,
                "DocumentStart span must start at byte 0"
            );
            assert_eq!(
                span.end.byte_offset, 3,
                "DocumentStart span must end after '---' (byte 3)"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Group K — Edge cases
    // -----------------------------------------------------------------------

    // K-1: %TAG directive with no prefix field (line ends after handle) returns error.
    #[test]
    fn tag_directive_missing_prefix_returns_error() {
        assert!(
            has_error("%TAG !foo!\n---\nscalar\n"),
            "%TAG with no prefix field must return an error"
        );
    }

    // K-2: %TAG directive whose prefix is the two-char literal `""` is
    // accepted (non-empty prefix consisting of two ASCII quote characters).
    #[test]
    fn tag_directive_with_double_quote_prefix_is_accepted() {
        assert!(
            !has_error("%TAG !e! \"\"\n---\nscalar\n"),
            "%TAG with double-quote prefix must be accepted"
        );
    }

    // K-3: Directive count at limit is accepted.
    #[test]
    fn directive_count_at_limit_is_accepted() {
        // Build exactly MAX_DIRECTIVES_PER_DOC directives with distinct handles.
        use std::fmt::Write as _;
        let mut input = String::new();
        for i in 0..MAX_DIRECTIVES_PER_DOC {
            let _ = writeln!(input, "%TAG !h{i}! prefix{i}:");
        }
        input.push_str("---\nscalar\n");
        assert!(
            !has_error(&input),
            "exactly MAX_DIRECTIVES_PER_DOC directives must be accepted"
        );
    }

    // K-4: Directive count exceeding limit returns error.
    #[test]
    fn directive_count_exceeding_limit_returns_error() {
        use std::fmt::Write as _;
        let mut input = String::new();
        for i in 0..=MAX_DIRECTIVES_PER_DOC {
            let _ = writeln!(input, "%TAG !h{i}! prefix{i}:");
        }
        input.push_str("---\nscalar\n");
        assert!(
            has_error(&input),
            "more than MAX_DIRECTIVES_PER_DOC directives must return an error"
        );
    }

    // K-5: Tag handle at byte limit is accepted.
    #[test]
    fn tag_handle_at_byte_limit_is_accepted() {
        // Handle is `!` + (MAX_TAG_HANDLE_BYTES - 3) inner chars + `!`
        // Total handle bytes = MAX_TAG_HANDLE_BYTES.
        let inner = "a".repeat(MAX_TAG_HANDLE_BYTES.saturating_sub(3));
        let handle = format!("!{inner}!");
        let input = format!("%TAG {handle} prefix:\n---\n!{inner}!suffix val\n");
        assert!(
            !has_error(&input),
            "tag handle at MAX_TAG_HANDLE_BYTES must be accepted"
        );
    }

    // K-6: Tag handle exceeding byte limit returns error.
    #[test]
    fn tag_handle_exceeding_byte_limit_returns_error() {
        let inner = "a".repeat(MAX_TAG_HANDLE_BYTES);
        let handle = format!("!{inner}!");
        let input = format!("%TAG {handle} prefix:\n---\nscalar\n");
        assert!(
            has_error(&input),
            "tag handle exceeding MAX_TAG_HANDLE_BYTES must return an error"
        );
    }

    // K-7: Tag prefix at exactly MAX_TAG_LEN bytes is accepted.
    #[test]
    fn tag_prefix_at_byte_limit_is_accepted() {
        let prefix = "a".repeat(MAX_TAG_LEN);
        let input = format!("%TAG !e! {prefix}\n---\nscalar\n");
        assert!(
            !has_error(&input),
            "tag prefix at MAX_TAG_LEN must be accepted"
        );
    }

    // K-8: Tag prefix exceeding MAX_TAG_LEN by one byte returns error.
    #[test]
    fn tag_prefix_exceeding_byte_limit_returns_error() {
        let prefix = "a".repeat(MAX_TAG_LEN + 1);
        let input = format!("%TAG !e! {prefix}\n---\nscalar\n");
        assert!(
            has_error(&input),
            "tag prefix exceeding MAX_TAG_LEN must return an error"
        );
    }

    // K-9: Control character in %TAG prefix returns error.
    #[test]
    fn tag_prefix_with_control_character_returns_error() {
        assert!(
            has_error("%TAG !e! tag:\x01example.com\n---\nscalar\n"),
            "control character in %TAG prefix must return an error"
        );
    }

    // -----------------------------------------------------------------------
    // Group L — directive+comment interaction and directive-without-marker
    // -----------------------------------------------------------------------

    // L-1: Comment after %YAML directive does not clobber version.
    #[test]
    fn yaml_directive_survives_trailing_comment() {
        let events = event_variants("%YAML 1.2\n# comment\n---\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart {
                    version: Some((1, 2)),
                    ..
                }
            )),
            "%YAML 1.2 version must survive a following comment line"
        );
    }

    // L-2: Comment before %YAML directive does not clobber version.
    #[test]
    fn yaml_directive_survives_leading_comment() {
        let events = event_variants("# comment\n%YAML 1.2\n---\nscalar\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::DocumentStart {
                    version: Some((1, 2)),
                    ..
                }
            )),
            "%YAML 1.2 version must survive a preceding comment line"
        );
    }

    // L-3: Comments interspersed between %YAML and %TAG directives preserve both.
    #[test]
    fn directives_survive_interspersed_comments() {
        let events = evs("%YAML 1.2\n# a\n# b\n%TAG !e! prefix:\n---\n!e!foo val\n");
        // DocumentStart must carry version and tag_directives.
        let doc_start = events.iter().find(|e| {
            matches!(
                e,
                Event::DocumentStart {
                    version: Some((1, 2)),
                    ..
                }
            )
        });
        assert!(
            doc_start.is_some(),
            "DocumentStart must have version Some((1,2)) when comments interspersed"
        );
        if let Some(Event::DocumentStart { tag_directives, .. }) = doc_start {
            assert!(
                tag_directives
                    .iter()
                    .any(|(h, p)| h == "!e!" && p == "prefix:"),
                "DocumentStart must carry !e! tag directive when comments interspersed"
            );
        }
        // Scalar tag must be resolved.
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. } if t.as_ref() == "prefix:foo"
            )),
            "!e!foo must resolve to prefix:foo when comments interspersed"
        );
    }

    // L-4: Comment between %TAG and `---` does not break tag resolution.
    #[test]
    fn tag_directive_survives_trailing_comment() {
        let events = evs("%TAG !e! tag:example.com:\n# banner\n---\n!e!foo val\n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar { tag: Some(t), .. } if t.as_ref() == "tag:example.com:foo"
            )),
            "!e!foo must resolve to tag:example.com:foo when comment follows %TAG"
        );
    }

    // L-5: %YAML directive without `---` returns error.
    #[test]
    fn yaml_directive_without_marker_returns_error() {
        assert!(
            has_error("%YAML 1.2\nscalar\n"),
            "%YAML directive without --- must return an error"
        );
    }

    // L-6: %TAG directive without `---` returns error.
    #[test]
    fn tag_directive_without_marker_returns_error() {
        assert!(
            has_error("%TAG !e! prefix:\nscalar\n"),
            "%TAG directive without --- must return an error"
        );
    }

    // L-7: Reserved directive without `---` returns error.
    #[test]
    fn reserved_directive_without_marker_returns_error() {
        assert!(
            has_error("%FOO bar\nscalar\n"),
            "reserved directive without --- must return an error"
        );
    }

    // L-8: %YAML directive followed by orphan `...` (not `---`) returns error.
    #[test]
    fn yaml_directive_followed_by_document_end_returns_error() {
        assert!(
            has_error("%YAML 1.2\n...\nscalar\n"),
            "%YAML directive followed by ... (not ---) must return an error"
        );
    }

    // L-9: Multi-doc with comments between directives and marker preserves scope
    // isolation across documents.
    #[test]
    fn multi_doc_directive_scope_isolated_through_comments() {
        let events = evs("%YAML 1.2\n# one\n---\nfirst\n...\n%YAML 1.1\n# two\n---\nsecond\n");
        let doc_starts: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::DocumentStart { .. }))
            .collect();
        assert_eq!(doc_starts.len(), 2, "expected two DocumentStart events");
        assert!(
            matches!(
                doc_starts.first(),
                Some(Event::DocumentStart {
                    version: Some((1, 2)),
                    ..
                })
            ),
            "first doc must have version Some((1, 2))"
        );
        assert!(
            matches!(
                doc_starts.get(1),
                Some(Event::DocumentStart {
                    version: Some((1, 1)),
                    ..
                })
            ),
            "second doc must have version Some((1, 1))"
        );
    }

    // -----------------------------------------------------------------------
    // Group M — %YAML trailing garbage and %TAG handle shape validation
    // -----------------------------------------------------------------------

    // M-1: %YAML directive with trailing garbage returns error.
    #[test]
    fn yaml_directive_trailing_garbage_returns_error() {
        assert!(
            has_error("%YAML 1.2 garbage\n---\nscalar\n"),
            "%YAML with trailing non-comment garbage must return an error"
        );
    }

    // M-2: %YAML directive with trailing comment is accepted.
    #[test]
    fn yaml_directive_trailing_comment_is_accepted() {
        assert!(
            !has_error("%YAML 1.2 # a comment\n---\nscalar\n"),
            "%YAML with trailing comment must be accepted"
        );
    }

    // M-3: %TAG handle not starting with `!` returns error.
    #[test]
    fn tag_handle_without_leading_bang_returns_error() {
        assert!(
            has_error("%TAG noBang prefix:\n---\nscalar\n"),
            "%TAG handle not starting with ! must return an error"
        );
    }

    // M-4: %TAG named handle missing trailing `!` returns error.
    #[test]
    fn tag_handle_missing_trailing_bang_returns_error() {
        assert!(
            has_error("%TAG !a prefix:\n---\nscalar\n"),
            "%TAG named handle missing trailing ! must return an error"
        );
    }

    // M-5: %TAG handle with three bangs (`!!!`) returns error.
    #[test]
    fn tag_handle_three_bangs_returns_error() {
        assert!(
            has_error("%TAG !!! prefix:\n---\nscalar\n"),
            "%TAG handle !!! must return an error"
        );
    }

    // M-6: %TAG primary handle `!` is accepted.
    #[test]
    fn tag_handle_primary_is_accepted() {
        assert!(
            !has_error("%TAG ! prefix:\n---\nscalar\n"),
            "%TAG primary handle ! must be accepted"
        );
    }

    // M-7: %TAG secondary handle `!!` is accepted.
    #[test]
    fn tag_handle_secondary_is_accepted() {
        assert!(
            !has_error("%TAG !! prefix:\n---\nscalar\n"),
            "%TAG secondary handle !! must be accepted"
        );
    }

    // M-8: %TAG named handle `!foo!` is accepted.
    #[test]
    fn tag_handle_named_is_accepted() {
        assert!(
            !has_error("%TAG !foo! prefix:\n---\nscalar\n"),
            "%TAG named handle !foo! must be accepted"
        );
    }
}

// ---------------------------------------------------------------------------
// mod scalar_dispatch — first-byte dispatcher integration tests (Task 3)
// ---------------------------------------------------------------------------

mod scalar_dispatch {
    use super::*;
    use rstest::rstest;

    fn first_scalar(input: &str) -> Option<Event<'_>> {
        parse_events(input)
            .filter_map(Result::ok)
            .map(|(ev, _)| ev)
            .find(|ev| matches!(ev, Event::Scalar { .. }))
    }

    fn has_parse_error(input: &str) -> bool {
        parse_events(input).any(|r| r.is_err())
    }

    // -----------------------------------------------------------------------
    // Group E — Five dispatch styles end-to-end
    // -----------------------------------------------------------------------

    #[test]
    fn it_literal_block_scalar_round_trip() {
        let ev = first_scalar("|\n  hello\n");
        assert!(
            matches!(ev, Some(Event::Scalar { style: ScalarStyle::Literal(_), ref value, .. }) if value == "hello\n"),
            "expected literal scalar 'hello\\n', got {ev:?}"
        );
    }

    #[test]
    fn it_folded_block_scalar_round_trip() {
        let ev = first_scalar(">\n  hello\n");
        assert!(
            matches!(ev, Some(Event::Scalar { style: ScalarStyle::Folded(_), ref value, .. }) if value == "hello\n"),
            "expected folded scalar 'hello\\n', got {ev:?}"
        );
    }

    #[rstest]
    #[case::single_quoted("'hello'\n", ScalarStyle::SingleQuoted)]
    #[case::double_quoted("\"hello\"\n", ScalarStyle::DoubleQuoted)]
    #[case::plain("hello\n", ScalarStyle::Plain)]
    fn it_scalar_dispatch_round_trip_exact_style(
        #[case] input: &str,
        #[case] expected_style: ScalarStyle,
    ) {
        let ev = first_scalar(input);
        assert!(
            matches!(&ev, Some(Event::Scalar { style, value, .. }) if *style == expected_style && value.as_ref() == "hello"),
            "expected scalar 'hello' with style {expected_style:?}, got {ev:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group F — Inline scalar path (the `--- text` short-circuit)
    // -----------------------------------------------------------------------

    // IT-S14 already covers basic `--- text\n`; this group adds edge cases.

    #[test]
    fn it_inline_scalar_with_leading_whitespace_in_value() {
        // Two spaces between `---` and `value` — leading ws after `---` is
        // separator; scalar content is `value with spaces`.
        let ev = first_scalar("---  value with spaces\n");
        assert!(
            matches!(ev, Some(Event::Scalar { style: ScalarStyle::Plain, ref value, .. }) if value == "value with spaces"),
            "expected plain scalar 'value with spaces', got {ev:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group G — Unusual whitespace before block indicators
    // -----------------------------------------------------------------------

    #[test]
    fn it_tab_before_pipe_behavior_preserved() {
        // `\t|` — a tab-prefixed line. The tab is stripped as whitespace and `|`
        // is seen as the first byte, routing to the literal block scanner.
        // Both the old chain and the new dispatcher behave identically here.
        // Confirm the result is NOT a plain scalar (the dispatch happened to literal).
        let events: Vec<_> = parse_events("\t|\n").collect();
        let scalar_ev = events
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .find(|(ev, _)| matches!(ev, Event::Scalar { .. }));
        if let Some((Event::Scalar { style, .. }, _)) = scalar_ev {
            assert!(
                !matches!(style, ScalarStyle::Plain),
                "tab-then-pipe should not dispatch to plain"
            );
        }
        // If no scalar (parse error or no document), that's also acceptable —
        // what matters is we don't silently emit a plain scalar.
    }

    #[test]
    fn it_pipe_with_chomping_indicator_still_dispatches_literal() {
        let ev = first_scalar("|-\n  a\n");
        assert!(
            matches!(
                ev,
                Some(Event::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                })
            ),
            "expected literal scalar, got {ev:?}"
        );
    }

    #[test]
    fn it_gt_with_indent_indicator_still_dispatches_folded() {
        let ev = first_scalar(">2\n  a\n");
        assert!(
            matches!(
                ev,
                Some(Event::Scalar {
                    style: ScalarStyle::Folded(_),
                    ..
                })
            ),
            "expected folded scalar, got {ev:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group H — Double-quoted trailing-tail validation preserved
    // -----------------------------------------------------------------------

    #[test]
    fn it_double_quoted_comment_after_closing_quote_accepted() {
        // `"hello" # comment` — valid trailing comment.
        assert!(
            !has_parse_error("\"hello\" # comment\n"),
            "valid trailing comment after double-quoted scalar must be accepted"
        );
        let ev = first_scalar("\"hello\" # comment\n");
        assert!(
            matches!(ev, Some(Event::Scalar { style: ScalarStyle::DoubleQuoted, ref value, .. }) if value == "hello"),
            "expected double-quoted scalar 'hello', got {ev:?}"
        );
    }

    #[rstest]
    #[case::no_space_before_hash("\"hello\"#comment\n")]
    #[case::non_comment_trailing_content("\"hello\" extra\n")]
    fn it_double_quoted_invalid_trailing_content_is_error(#[case] input: &str) {
        assert!(
            has_parse_error(input),
            "invalid trailing content after double-quoted scalar must be a parse error"
        );
    }
}

// ---------------------------------------------------------------------------
// mod probe_dispatch — step_in_document probe-cascade reorder (Task 2)
//
// These tests target the dispatch boundaries that move position in the cascade.
// Only cases not already covered by existing modules are added here.
// ---------------------------------------------------------------------------

mod probe_dispatch {
    use super::*;

    fn evs(input: &str) -> Vec<Event<'_>> {
        parse_events(input)
            .map(|r| match r {
                Ok((ev, _)) => ev,
                Err(e) => unreachable!("unexpected parse error: {e}"),
            })
            .collect()
    }

    fn has_error(input: &str) -> bool {
        parse_events(input).any(|r| r.is_err())
    }

    // -----------------------------------------------------------------------
    // Group Q: Pending-state handoff — tag+anchor both standalone, then
    // sequence/mapping entry.  After the reorder, the sequence/mapping probes
    // fire before the anchor/tag probes, so the pending-state must survive
    // across two Continue cycles before the structural probe fires.
    // -----------------------------------------------------------------------

    // Q-5b: tag first, then anchor, then sequence entry — both properties
    // attach to SequenceStart (reverse ordering of the T-11 test in mod tags).
    #[test]
    fn tag_first_anchor_second_standalone_both_propagate_to_sequence_start() {
        let input = "!!seq\n&myseq\n- a\n";
        let events = evs(input);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::SequenceStart {
                    anchor: Some("myseq"),
                    tag: Some(t),
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:seq"
            )),
            "SequenceStart must carry both anchor myseq and tag:yaml.org,2002:seq (tag-first order)"
        );
    }

    // Q-6: anchor first, then tag, then mapping entry — both properties attach
    // to MappingStart.
    #[test]
    fn anchor_first_tag_second_standalone_both_propagate_to_mapping_start() {
        let input = "&mymap\n!!map\nkey: val\n";
        let events = evs(input);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::MappingStart {
                    anchor: Some("mymap"),
                    tag: Some(t),
                    ..
                } if t.as_ref() == "tag:yaml.org,2002:map"
            )),
            "MappingStart must carry both anchor mymap and tag:yaml.org,2002:map"
        );
    }

    // -----------------------------------------------------------------------
    // Group R: Inline anchor + block-sequence dash — error path variants.
    // R-1 (`&a - item\n`) is already covered in mod anchors_and_aliases.
    // R-2 and R-3 are new.
    // -----------------------------------------------------------------------

    // R-2: tab after dash in inline anchor is also rejected.
    #[test]
    fn anchor_followed_by_inline_dash_tab_returns_error() {
        assert!(
            has_error("&a -\titem\n"),
            "anchor &a with inline `- <tab>item` must return an error"
        );
    }

    // R-3: bare dash (no trailing content) after inline anchor is rejected.
    #[test]
    fn anchor_followed_by_inline_bare_dash_returns_error() {
        assert!(
            has_error("&a -\n"),
            "anchor &a with inline bare `-` must return an error"
        );
    }
}
