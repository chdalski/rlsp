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
