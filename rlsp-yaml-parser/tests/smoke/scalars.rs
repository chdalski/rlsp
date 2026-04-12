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
