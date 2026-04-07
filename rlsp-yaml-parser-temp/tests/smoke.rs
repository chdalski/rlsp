// SPDX-License-Identifier: MIT

//! Smoke / integration tests for `rlsp-yaml-parser-temp`.
//!
//! Tests are grouped by grammar area using nested modules.  Each task adds
//! a new `mod` block here as it introduces new event variants.
//!
//! # Shared helper
//!
//! [`parse_to_vec`] collects the full event stream into a `Vec` without
//! hiding errors.  It is the canonical test helper for all grammar tasks.

use rlsp_yaml_parser_temp::{Error, Event, Pos, Span, parse_events};

// ---------------------------------------------------------------------------
// Shared helper for extracting event variants from parse_to_vec
// ---------------------------------------------------------------------------

/// Extract only the `Event` variant (dropping the `Span`) from a `parse_to_vec`
/// result, panicking if any item is an `Err`.
fn event_variants(input: &str) -> Vec<Event<'_>> {
    parse_events(input)
        .map(|r| match r {
            Ok((ev, _span)) => ev,
            Err(e) => panic!("unexpected parse error: {e}"),
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
            panic!("expected StreamStart as first event");
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
            panic!("expected StreamEnd as second event");
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
            panic!("expected StreamEnd as second event");
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
                _ => panic!("both must be Ok"),
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
        // The `value` on the same line is consumed by InDocument (scalar
        // handling deferred).  For Task 5, the document is started and
        // implicitly ended.
        let events = event_variants("--- value\n");
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
    // Group I — Indented `---` is NOT a marker
    // -----------------------------------------------------------------------

    #[test]
    fn indented_dash_is_not_a_directives_end_marker() {
        // "  ---" has indent=2; it is bare-document content (not a marker).
        // BetweenDocs fires the bare-doc path: DocumentStart{explicit:false},
        // then InDocument consumes the line, then EOF → DocumentEnd{false}.
        let events = event_variants("  ---\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
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
            panic!("expected DocumentStart as second event");
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
            panic!("expected DocumentStart as second event");
        };
        assert_eq!(span.start.byte_offset, 0);
    }

    #[test]
    fn doc_end_explicit_span_covers_three_bytes() {
        let results = parse_to_vec("---\n...\n");
        let Some(Ok((Event::DocumentEnd { explicit: true }, span))) = results.get(2) else {
            panic!("expected explicit DocumentEnd as third event");
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
            panic!("expected explicit DocumentEnd as third event");
        };
        assert_eq!(span.start.byte_offset, 4);
    }

    #[test]
    fn doc_end_implicit_span_is_zero_width() {
        let results = parse_to_vec("---\n");
        let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(2) else {
            panic!("expected implicit DocumentEnd as third event");
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
            panic!("expected DocumentStart as second event");
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
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    #[test]
    fn multi_line_content_yields_single_bare_doc() {
        // Both lines consumed inside InDocument; only one DocumentStart/End.
        let events = event_variants("foo\nbar\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
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
                Event::DocumentEnd { explicit: false },
                Event::StreamEnd,
            ]
        );
    }

    #[test]
    fn bare_doc_with_explicit_end_marker() {
        // InDocument sees `...` → DocumentEnd{explicit:true}.
        let events = event_variants("foo\n...\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
                Event::DocumentEnd { explicit: true },
                Event::StreamEnd,
            ]
        );
    }

    #[test]
    fn bare_doc_followed_by_explicit_doc() {
        // InDocument sees `---` → implicit DocumentEnd for the bare doc, then
        // DocumentStart{explicit:true} for the new one.
        let events = event_variants("foo\n---\n");
        assert_eq!(
            events,
            [
                Event::StreamStart,
                Event::DocumentStart { explicit: false },
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
                Event::DocumentEnd { explicit: true },
                Event::DocumentStart { explicit: false },
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
            panic!("expected bare DocumentStart as second event");
        };
        assert_eq!(
            span.start, span.end,
            "bare DocumentStart span must be zero-width"
        );
        assert_eq!(span.start.byte_offset, 0);
    }

    #[test]
    fn bare_doc_end_at_eof_span_is_zero_width_after_last_content() {
        // "foo\n" = 4 bytes; DocumentEnd{false} at position after last byte.
        let results = parse_to_vec("foo\n");
        let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(2) else {
            panic!("expected bare DocumentEnd as third event");
        };
        assert_eq!(
            span.start, span.end,
            "bare DocumentEnd span must be zero-width"
        );
        assert_eq!(span.start.byte_offset, 4);
    }

    #[test]
    fn bare_doc_end_before_explicit_doc_span_is_zero_width_at_marker_pos() {
        // "foo\n---\n": implicit DocumentEnd at byte 4 (position of `---`).
        let results = parse_to_vec("foo\n---\n");
        let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(2) else {
            panic!("expected implicit DocumentEnd as third event");
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
            panic!("expected bare DocumentStart as second event");
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
}
