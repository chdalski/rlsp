// SPDX-License-Identifier: MIT
#![deny(clippy::panic)]

//! Smoke / integration tests for `rlsp-yaml-parser-temp`.
//!
//! Tests are grouped by grammar area using nested modules.  Each task adds
//! a new `mod` block here as it introduces new event variants.
//!
//! # Shared helper
//!
//! [`parse_to_vec`] collects the full event stream into a `Vec` without
//! hiding errors.  It is the canonical test helper for all grammar tasks.

use rlsp_yaml_parser_temp::{
    Chomp, CollectionStyle, Error, Event, MAX_COLLECTION_DEPTH, Pos, ScalarStyle, Span,
    parse_events,
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

    #[test]
    fn empty_input_emits_stream_start_then_stream_end() {
        let events = parse_to_vec("");
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
    fn whitespace_only_input_emits_stream_start_then_stream_end() {
        let events = parse_to_vec("   \n\n");
        assert_eq!(events.len(), 2, "expected exactly 2 events");
        assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
        assert!(matches!(events.get(1), Some(Ok((Event::StreamEnd, _)))));
    }

    #[test]
    fn tab_only_input_emits_stream_start_then_stream_end() {
        let events = parse_to_vec("\t\t\t");
        assert_eq!(events.len(), 2, "expected exactly 2 events");
        assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
        assert!(matches!(events.get(1), Some(Ok((Event::StreamEnd, _)))));
    }

    #[test]
    fn single_newline_emits_stream_start_then_stream_end() {
        let events = parse_to_vec("\n");
        assert_eq!(events.len(), 2, "expected exactly 2 events");
        assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
        assert!(matches!(events.get(1), Some(Ok((Event::StreamEnd, _)))));
    }

    #[test]
    fn crlf_only_input_emits_stream_start_then_stream_end() {
        let events = parse_to_vec("\r\n\r\n");
        assert_eq!(events.len(), 2, "expected exactly 2 events");
        assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
        assert!(matches!(events.get(1), Some(Ok((Event::StreamEnd, _)))));
    }

    #[test]
    fn comment_only_input_emits_stream_start_then_stream_end() {
        let events = parse_to_vec("# comment\n   \n");
        assert_eq!(events.len(), 2, "expected exactly 2 events");
        assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
        assert!(matches!(events.get(1), Some(Ok((Event::StreamEnd, _)))));
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
                Event::DocumentEnd { explicit: false },
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
                Event::DocumentEnd { explicit: true },
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
                Event::DocumentEnd { explicit: false },
                Event::DocumentStart { explicit: true },
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
        assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
    }

    // -----------------------------------------------------------------------
    // Group D — Comments and blank lines around markers
    // -----------------------------------------------------------------------

    #[test]
    fn comment_before_marker_is_skipped() {
        let events = event_variants("# comment\n---\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
                Event::Scalar {
                    value: "value".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
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
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "---".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
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
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "foo".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
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
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "foo bar".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
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
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "foo".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
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
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "foo".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
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
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "foo".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: false },
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "foo".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
                },
                Event::DocumentEnd { explicit: true },
                Event::DocumentStart { explicit: false },
                Event::Scalar {
                    value: "bar".into(),
                    style: rlsp_yaml_parser_temp::ScalarStyle::Plain,
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
        let Some(Ok((Event::DocumentStart { explicit: false }, span))) = results.get(1) else {
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
        let Some(Ok((Event::DocumentStart { explicit: false }, span))) = results.get(1) else {
            unreachable!("expected bare DocumentStart as second event");
        };
        assert_eq!(span.start.byte_offset, 1);
    }

    // -----------------------------------------------------------------------
    // Group L — Directive line skipping (IT-38 through IT-40)
    // -----------------------------------------------------------------------

    #[test]
    fn yaml_directive_before_explicit_doc_is_skipped() {
        let events = event_variants("%YAML 1.2\n---\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    #[test]
    fn yaml_directive_with_explicit_end() {
        let events = event_variants("%YAML 1.2\n---\n...\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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

    // Helper: make a plain `Scalar` event for easy comparison.
    fn plain(value: &str) -> Event<'_> {
        Event::Scalar {
            value: value.into(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
        }
    }

    // IT-S1 — single plain scalar in bare document.
    #[test]
    fn plain_scalar_emits_scalar_event() {
        let events = event_variants("hello");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("hello"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S2 — plain scalar with explicit `---` and `...` markers.
    #[test]
    fn plain_scalar_explicit_doc_markers() {
        let events = event_variants("---\nhello\n...\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
                plain("hello"),
                Event::DocumentEnd { explicit: true },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S3 — multi-line plain scalar folds to spaces.
    #[test]
    fn multi_line_plain_scalar_folds_to_spaces() {
        // "foo\n  bar\n  baz\n" → "foo bar baz"
        let events = event_variants("foo\n  bar\n  baz\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("foo bar baz"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S4 — plain scalar with embedded URL (`:` disambiguation).
    #[test]
    fn plain_scalar_with_url() {
        let events = event_variants("http://example.com");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("http://example.com"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S5 — plain scalar with `#` not preceded by whitespace.
    #[test]
    fn plain_scalar_with_hash_inside() {
        let events = event_variants("a#b");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("a#b"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S6 — plain scalar terminated by inline comment.
    #[test]
    fn plain_scalar_terminated_by_comment() {
        // "foo # comment\n" → scalar "foo" (trailing space stripped, comment excluded).
        let events = event_variants("foo # comment\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("foo"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S7 — blank line in multi-line plain scalar folds to newline.
    #[test]
    fn multi_line_plain_scalar_blank_line_folds_to_newline() {
        // "foo\n\nbar\n" → "foo\nbar"
        let events = event_variants("foo\n\nbar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("foo\nbar"),
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

    // IT-S11 — plain scalar with backslashes (no escaping in plain scalars).
    #[test]
    fn plain_scalar_with_backslashes() {
        let events = event_variants("plain\\value\\with\\backslashes");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("plain\\value\\with\\backslashes"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S12 — two blank lines in multi-line scalar fold to two newlines.
    #[test]
    fn multi_line_plain_scalar_two_blank_lines_fold_to_two_newlines() {
        // "foo\n\n\nbar\n" — two blank lines → "foo\n\nbar"
        let events = event_variants("foo\n\n\nbar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("foo\n\nbar"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S13 — trailing space on continuation lines is stripped before folding.
    #[test]
    fn multi_line_plain_scalar_continuation_trailing_space_stripped() {
        // "foo\nbar   \nbaz\n" — trailing spaces on "bar" stripped; → "foo bar baz"
        let events = event_variants("foo\nbar   \nbaz\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                plain("foo bar baz"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-S14 — inline scalar on same line as `---` marker.
    #[test]
    fn plain_scalar_inline_after_directives_end_marker() {
        // "--- text\n" — "text" follows the marker on the same line.
        let events = event_variants("--- text\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
                plain("text"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }
}

// ---------------------------------------------------------------------------
// mod quoted_scalars — single- and double-quoted scalar integration tests (Task 7)
// ---------------------------------------------------------------------------

mod quoted_scalars {
    use std::borrow::Cow;

    use super::*;

    fn single(value: &str) -> Event<'_> {
        Event::Scalar {
            value: value.into(),
            style: ScalarStyle::SingleQuoted,
            anchor: None,
            tag: None,
        }
    }

    fn double(value: &str) -> Event<'_> {
        Event::Scalar {
            value: value.into(),
            style: ScalarStyle::DoubleQuoted,
            anchor: None,
            tag: None,
        }
    }

    // IT-1 (spike): single-quoted scalar emits Scalar with SingleQuoted style.
    // Use bare document (no --- marker) so quoted scalar starts on its own line,
    // avoiding the inline_scalar slot which is plain-scalar only.
    #[test]
    fn single_quoted_scalar_emits_scalar_event_with_single_quoted_style() {
        let events = event_variants("'hello'\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                single("hello"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-2: double-quoted scalar emits Scalar with DoubleQuoted style.
    #[test]
    fn double_quoted_scalar_emits_scalar_event_with_double_quoted_style() {
        let events = event_variants("\"hello\"\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                double("hello"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-3: double-quoted escape produces correct value.
    #[test]
    fn double_quoted_escape_produces_correct_value() {
        // Input: `"with\nescape"` — `\n` is an escape sequence → literal newline.
        let events = event_variants("\"with\\nescape\"\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                double("with\nescape"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-4: unicode escape in double-quoted produces correct codepoint.
    #[test]
    fn unicode_escape_in_double_quoted_produces_correct_codepoint() {
        let events = event_variants("\"\\u00E9\"\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                double("é"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-5: single-quoted with escaped quote.
    #[test]
    fn single_quoted_with_escaped_quote() {
        let events = event_variants("'it''s'\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                single("it's"),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
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

    // IT-8: single-quoted empty scalar.
    #[test]
    fn single_quoted_empty_scalar() {
        let events = event_variants("''\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                single(""),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-9: double-quoted empty scalar.
    #[test]
    fn double_quoted_empty_scalar() {
        let events = event_variants("\"\"\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                double(""),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-10: double-quoted malformed escape propagates Err.
    #[test]
    fn double_quoted_malformed_escape_propagates_err() {
        let results = parse_to_vec("\"\\uD800\"\n");
        assert!(
            results.iter().any(Result::is_err),
            "expected at least one Err in results"
        );
    }

    // IT-11: double-quoted unterminated propagates Err.
    #[test]
    fn double_quoted_unterminated_propagates_err() {
        let results = parse_to_vec("\"unterminated\n");
        assert!(
            results.iter().any(Result::is_err),
            "expected at least one Err in results"
        );
    }

    // IT-12: single-quoted Cow borrow for no-escape content.
    #[test]
    fn single_quoted_cow_borrow_for_no_escape() {
        let results = parse_to_vec("'hello'\n");
        let Some(Ok((Event::Scalar { value, .. }, _))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert!(
            matches!(value, Cow::Borrowed(_)),
            "single-quoted with no escapes must be Cow::Borrowed"
        );
    }

    // IT-13: double-quoted Cow borrow for no-escape content.
    #[test]
    fn double_quoted_cow_borrow_for_no_escape() {
        let results = parse_to_vec("\"hello\"\n");
        let Some(Ok((Event::Scalar { value, .. }, _))) = results.get(2) else {
            unreachable!("expected Scalar as third event");
        };
        assert!(
            matches!(value, Cow::Borrowed(_)),
            "double-quoted with no escapes must be Cow::Borrowed"
        );
    }

    // IT-14: plain scalar regression guard — adding quoted paths must not break plain.
    #[test]
    fn single_quoted_follows_plain_scalar_fallback() {
        let events = event_variants("--- plain");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
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
        assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
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
        assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: false },
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
                Event::DocumentStart { explicit: false },
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
    // Also tests %YAML directive (skipped in BetweenDocs).
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: false },
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
    // yaml: `%FOO  bar baz # ...\n---\n"foo"\n`
    // Expected scalar value: `foo`
    #[test]
    fn cf_q2_2lfx_double_quoted_after_directive() {
        let input = "%FOO  bar baz # Should be ignored\n                  # with a warning.\n---\n\"foo\"\n";
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
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
        // 2LFX variant: `%FOO ...\n---\n"foo"\n` — quoted scalar on its own line.
        let input = "%FOO  bar baz # Should be ignored\n                  # with a warning.\n---\n\"foo\"\n";
        let events = event_variants(input);
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: false },
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

    // IT-LB-1 (spike) — simple literal block scalar in bare document.
    // Validates type wiring: `|` dispatch, Literal(Clip) style, basic content.
    #[test]
    fn spike_simple_literal_block_scalar() {
        // "|\n  hello\n" → scalar "hello\n" (Clip)
        let events = event_variants("|\n  hello\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("hello\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-2 — Strip chomping removes all trailing newlines.
    #[test]
    fn strip_chomping_removes_trailing_newlines() {
        // "|-\n  foo\n\n" → "foo" (no trailing newline)
        let events = event_variants("|-\n  foo\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("foo", Chomp::Strip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-3 — Keep chomping retains all trailing blank lines.
    #[test]
    fn keep_chomping_retains_all_trailing_newlines() {
        // "|+\n  foo\n\n" → "foo\n\n" (content newline + 1 blank line)
        let events = event_variants("|+\n  foo\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("foo\n\n", Chomp::Keep),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-4 — Clip chomping keeps exactly one trailing newline.
    #[test]
    fn clip_chomping_keeps_exactly_one_trailing_newline() {
        // "|\n  foo\n\n" → "foo\n" (one newline, trailing blank dropped)
        let events = event_variants("|\n  foo\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("foo\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-5 — Explicit indent indicator.
    // parent_indent=0, explicit=2 → content_indent=2; "  foo" → "foo\n"
    #[test]
    fn explicit_indent_indicator() {
        let events = event_variants("|2\n  foo\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("foo\n", Chomp::Clip),
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

    // IT-LB-7 — Empty scalar (just `|` header, no content).
    // "|\n" → "" (Clip: empty content → empty string)
    #[test]
    fn empty_literal_scalar_clip_yields_empty_string() {
        let events = event_variants("|\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-8 — Literal block scalar in explicit document.
    #[test]
    fn literal_block_scalar_in_explicit_document() {
        let events = event_variants("---\n|\n  hello world\n...\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: true },
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

    // IT-LB-13 — Multi-line content with blank lines between content lines.
    #[test]
    fn multiline_content_with_blank_line_between() {
        // "|\n  foo\n\n  bar\n" → "foo\n\nbar\n"
        let events = event_variants("|\n  foo\n\n  bar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("foo\n\nbar\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-LB-14 — Leading blank before first content line.
    // Per YAML 1.2 spec §8.1.2, blank lines between the header and the first
    // content line are included in the scalar value as newlines (via l-empty).
    #[test]
    fn leading_blank_before_first_content() {
        // "|\n\n  foo\n" → "\nfoo\n" (leading blank produces a newline)
        let events = event_variants("|\n\n  foo\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                literal("\nfoo\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
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

    // IT-FB-1 (spike) — simple two-line folded scalar, single break becomes space.
    // Validates `>` dispatch, `Folded(Clip)` style, and basic folding.
    #[test]
    fn spike_two_line_folded_break_becomes_space() {
        // ">\n  foo\n  bar\n" → scalar "foo bar\n" (Clip)
        let events = event_variants(">\n  foo\n  bar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo bar\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Core folding rules
    // -----------------------------------------------------------------------

    // IT-FB-2 — single non-blank line is not folded (no preceding content to join).
    #[test]
    fn single_line_not_folded() {
        let events = event_variants(">\n  hello\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("hello\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-3 — three equally-indented non-blank lines, all breaks folded to spaces.
    #[test]
    fn three_lines_all_breaks_become_spaces() {
        let events = event_variants(">\n  a\n  b\n  c\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("a b c\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-4 — one blank line between non-blank lines produces one newline.
    // Per §8.1.3: N blank lines → N newlines (first break discarded, blanks' breaks kept).
    #[test]
    fn one_blank_line_produces_one_newline() {
        let events = event_variants(">\n  foo\n\n  bar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo\nbar\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-5 — two blank lines between non-blank lines produce two newlines.
    #[test]
    fn two_blank_lines_produce_two_newlines() {
        let events = event_variants(">\n  foo\n\n\n  bar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo\n\nbar\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-6 — more-indented line: break before is preserved as `\n`, relative indent kept.
    // content_indent=2; "indented" line has indent 4 (more-indented by 2 spaces).
    // Break before → `\n`; content after stripping content_indent=2 spaces: "  indented".
    #[test]
    fn more_indented_break_before_preserved_relative_indent_kept() {
        // ">\n  normal\n    indented\n"
        // → "normal\n  indented\n"
        let events = event_variants(">\n  normal\n    indented\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("normal\n  indented\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-7 — breaks surrounding a more-indented region are both preserved as `\n`.
    // YAML 1.2 §8.1.3: "folding does not apply to line breaks *surrounding* text
    // lines that contain leading white space." Both the break BEFORE and the break
    // AFTER a more-indented line are preserved (neither is folded to a space).
    // content_indent=2; `b` at indent 4 (more-indented).
    // Break before `b` → `\n`; relative content of `b` → "  b"; break after `b` → `\n`.
    #[test]
    fn breaks_surrounding_more_indented_region_both_preserved() {
        // ">\n  a\n    b\n  c\n"
        // → "a\n  b\nc\n"
        let events = event_variants(">\n  a\n    b\n  c\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("a\n  b\nc\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-8 — all content lines at same (deeper) indent → auto-detect, normal folding.
    // Auto-detect gives content_indent=4; both lines at indent 4 (equally indented).
    #[test]
    fn all_deep_lines_equally_indented_normal_folding() {
        let events = event_variants(">\n    deep\n    also deep\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("deep also deep\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Chomping
    // -----------------------------------------------------------------------

    // IT-FB-9 — Strip (`>-`): trailing newlines removed.
    #[test]
    fn strip_chomp_removes_trailing_newlines() {
        let events = event_variants(">-\n  foo\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo", Chomp::Strip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-10 — Keep (`>+`): trailing blank lines preserved.
    #[test]
    fn keep_chomp_preserves_trailing_blank_lines() {
        let events = event_variants(">+\n  foo\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo\n\n", Chomp::Keep),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-11 — Clip (`>`): single trailing newline kept, extra blanks dropped.
    #[test]
    fn clip_chomp_keeps_one_trailing_newline() {
        let events = event_variants(">\n  foo\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Explicit indent indicator
    // -----------------------------------------------------------------------

    // IT-FB-12 — explicit indent indicator `>2`.
    #[test]
    fn explicit_indent_indicator() {
        let events = event_variants(">2\n  foo\n  bar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo bar\n", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-13 — explicit indent with strip, chomp-then-indent order: `>-2`.
    #[test]
    fn explicit_indent_with_strip_chomp_then_indent_order() {
        let events = event_variants(">-2\n  foo\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo", Chomp::Strip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-14 — explicit indent with keep, chomp-then-indent order: `>+2`.
    #[test]
    fn explicit_indent_with_keep_chomp_then_indent_order() {
        let events = event_variants(">+2\n  foo\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo\n\n", Chomp::Keep),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-15 — explicit indent with strip, indent-then-chomp order: `>2-`.
    // `parse_block_header` accepts either order.
    #[test]
    fn explicit_indent_with_strip_indent_then_chomp_order() {
        let events = event_variants(">2-\n  foo\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("foo", Chomp::Strip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    // IT-FB-16 — empty folded scalar (header only, no content).
    #[test]
    fn empty_folded_scalar_yields_empty_string() {
        let events = event_variants(">\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-17 — all-blank content (blank lines only, no non-blank lines).
    #[test]
    fn all_blank_content_yields_empty_string_with_clip() {
        let events = event_variants(">\n\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("", Chomp::Clip),
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    // IT-FB-18 — single-line with trailing blanks (Keep).
    #[test]
    fn keep_chomp_with_multiple_trailing_blanks() {
        let events = event_variants(">+\n  only\n\n\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                folded("only\n\n\n", Chomp::Keep),
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
                Event::DocumentStart { explicit: false },
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

    // IT-FB-24 — invalid indicator character produces an error.
    #[test]
    fn invalid_indicator_character_produces_error() {
        let results = parse_to_vec(">!\n  hello\n");
        let has_err = results.iter().any(Result::is_err);
        assert!(has_err, "expected a parse error for invalid indicator `!`");
    }

    // IT-FB-25 — indent indicator `0` is invalid.
    #[test]
    fn indent_indicator_zero_is_invalid() {
        let results = parse_to_vec(">0\n  hello\n");
        let has_err = results.iter().any(Result::is_err);
        assert!(has_err, "expected a parse error for indent indicator `0`");
    }

    // IT-FB-26 — duplicate chomp indicator is invalid.
    #[test]
    fn duplicate_chomp_indicator_is_invalid() {
        let results = parse_to_vec(">++\n  hello\n");
        let has_err = results.iter().any(Result::is_err);
        assert!(has_err, "expected a parse error for duplicate chomp `++`");
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
                Event::DocumentStart { explicit: true },
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
                Event::DocumentStart { explicit: false },
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
                Event::DocumentStart { explicit: false },
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

    #[test]
    fn dash_followed_by_newline_emits_empty_plain_scalar() {
        let events = event_variants("-\n");
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
            "empty item must emit empty plain scalar"
        );
    }

    #[test]
    fn dash_space_then_newline_emits_empty_plain_scalar() {
        let events = event_variants("- \n");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Scalar {
                    value,
                    style: ScalarStyle::Plain,
                    ..
                } if value.as_ref() == ""
            )),
            "dash+space+newline must emit empty plain scalar"
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
                | Event::MappingEnd => None,
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
                Event::DocumentStart { explicit: false },
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
                | Event::MappingEnd => None,
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
                | Event::MappingEnd => None,
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
                .any(|e| matches!(e, Event::DocumentStart { explicit: true })),
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
            .rposition(|e| matches!(e, Event::DocumentStart { explicit: true }))
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
            | Event::MappingEnd => None,
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
            | Event::MappingEnd => None,
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
                Event::DocumentStart { explicit: true },
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

    #[test]
    fn sequence_with_single_quoted_item() {
        let events = event_variants("- 'hello'\n");
        let scalar = events.iter().find_map(|e| match e {
            Event::Scalar { value, style, .. } => Some((value.as_ref(), *style)),
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd => None,
        });
        assert_eq!(
            scalar,
            Some(("hello", ScalarStyle::SingleQuoted)),
            "single-quoted item must have SingleQuoted style"
        );
    }

    #[test]
    fn sequence_with_double_quoted_item() {
        let events = event_variants("- \"world\"\n");
        let scalar = events.iter().find_map(|e| match e {
            Event::Scalar { value, style, .. } => Some((value.as_ref(), *style)),
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd => None,
        });
        assert_eq!(
            scalar,
            Some(("world", ScalarStyle::DoubleQuoted)),
            "double-quoted item must have DoubleQuoted style"
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
                | Event::MappingEnd => None,
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
                    Event::DocumentStart { explicit: false },
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

    #[test]
    fn key_colon_newline_emits_empty_plain_scalar_for_value() {
        let events = event_variants("key:\n");
        // Find the Scalar("key") then the next Scalar should be empty.
        let scalars: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::Scalar { value, style, .. } = e {
                    Some((value.as_ref(), *style))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(scalars.len(), 2, "expected 2 scalars (key + empty value)");
        if let [(key, _), (val, val_style)] = scalars.as_slice() {
            assert_eq!(*key, "key");
            assert_eq!(*val, "", "value must be empty string");
            assert!(
                matches!(val_style, ScalarStyle::Plain),
                "empty value must be Plain style"
            );
        }
    }

    #[test]
    fn key_colon_space_newline_emits_empty_plain_scalar_for_value() {
        // `key: \n` — trailing space after `:` before EOL
        let events = event_variants("key: \n");
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
        assert_eq!(scalars.len(), 2, "expected 2 scalars");
        if let [key, val] = scalars.as_slice() {
            assert_eq!(key, "key");
            assert_eq!(val, "", "trailing-space value must be empty");
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
            .position(|e| matches!(e, Event::DocumentStart { explicit: true }));
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
                    Event::DocumentStart { explicit: true },
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

    #[test]
    fn colon_without_space_is_plain_scalar_not_mapping() {
        // `key:value` — colon not followed by space, so it's a plain scalar
        let events = event_variants("key:value\n");
        let has_mapping = events
            .iter()
            .any(|e| matches!(e, Event::MappingStart { .. }));
        assert!(
            !has_mapping,
            "colon without space must not create a mapping"
        );
        let has_scalar = events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "key:value"));
        assert!(has_scalar, "expected Scalar(key:value), got: {events:?}");
    }

    #[test]
    fn url_colon_slash_slash_is_plain_scalar_not_mapping() {
        let events = event_variants("http://example.com\n");
        let has_mapping = events
            .iter()
            .any(|e| matches!(e, Event::MappingStart { .. }));
        assert!(!has_mapping, "URL must not create a mapping");
        let has_scalar = events.iter().any(
            |e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "http://example.com"),
        );
        assert!(
            has_scalar,
            "expected Scalar(http://example.com), got: {events:?}"
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
            | Event::MappingEnd => None,
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
        // `- key:\n    - inner\nother: val\n`
        // After `inner`, dedent back to col 0 closes: inner seq, outer seq (via the mapping value).
        // `other: val` at col 0 should parse as a new mapping.
        let events = event_variants("- key:\n    - inner\nother: val\n");
        let has_inner = events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "inner"));
        let has_other = events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "other"));
        let has_val = events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "val"));
        assert!(has_inner, "scalar 'inner' must be emitted");
        assert!(has_other, "scalar 'other' must be emitted");
        assert!(has_val, "scalar 'val' must be emitted");
        // inner must appear before other
        let inner_pos = events
            .iter()
            .position(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "inner"))
            .unwrap_or_else(|| unreachable!("inner scalar must exist"));
        let other_pos = events
            .iter()
            .position(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "other"))
            .unwrap_or_else(|| unreachable!("other scalar must exist"));
        assert!(inner_pos < other_pos, "inner before other");
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
        // A mapping key with no value followed by a dedent must emit an
        // empty plain scalar before MappingEnd (value-phase close).
        // `key:\nother: v\n` — "key" has no inline value; the next key
        // "other" at the same indent continues the mapping. But if we
        // put "key:\n" and then dedent immediately, the mapping closes
        // while in Value phase.
        // `key:\nv\n` — the standalone scalar `v` is not a mapping key,
        // so the mapping closes first.
        //
        // Use: `outer:\n  key:\nbaz\n`
        // outer mapping: key="outer", value=mapping(key="key", value=empty)
        // At "baz": indent 0 closes inner mapping (Value phase → empty scalar)
        // then closes outer mapping.
        let events = event_variants("outer:\n  key:\nbaz\n");
        // Must have at least one empty-string scalar (the missing value).
        let has_empty_scalar = events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref().is_empty()));
        assert!(
            has_empty_scalar,
            "a key with no value must produce an empty scalar before MappingEnd"
        );
        // "baz" must also appear.
        let has_baz = events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "baz"));
        assert!(has_baz, "scalar 'baz' must be emitted after the close");
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
