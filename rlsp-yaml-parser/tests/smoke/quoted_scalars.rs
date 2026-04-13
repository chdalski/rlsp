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

// IT-15: double-quoted mapping key emits DoubleQuoted style at event layer.
#[test]
fn quoted_key_parse_events_style_double() {
    let events = event_variants("\"key\": value\n");
    let key_event = events.iter().find_map(|ev| {
        if let Event::Scalar { value, style, .. } = ev {
            if value == "key" {
                return Some(*style);
            }
        }
        None
    });
    assert_eq!(
        key_event,
        Some(ScalarStyle::DoubleQuoted),
        "key scalar must have DoubleQuoted style at event layer"
    );
}

// IT-16: single-quoted mapping key emits SingleQuoted style at event layer.
#[test]
fn quoted_key_parse_events_style_single() {
    let events = event_variants("'key': value\n");
    let key_event = events.iter().find_map(|ev| {
        if let Event::Scalar { value, style, .. } = ev {
            if value == "key" {
                return Some(*style);
            }
        }
        None
    });
    assert_eq!(
        key_event,
        Some(ScalarStyle::SingleQuoted),
        "key scalar must have SingleQuoted style at event layer"
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
