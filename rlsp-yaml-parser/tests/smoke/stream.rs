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
