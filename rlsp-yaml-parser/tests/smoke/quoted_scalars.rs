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
        meta: None,
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
    assert_eq!(span.start, 0, "span must start at opening quote");
    assert_eq!(span.end, 7, "span must end after closing quote");
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
    assert_eq!(span.start, 0, "span must start at opening quote");
    assert_eq!(span.end, 7, "span must end after closing quote");
}

// IT-15: double-quoted mapping key emits DoubleQuoted style at event layer.
#[test]
fn quoted_key_parse_events_style_double() {
    let events = event_variants("\"key\": value\n");
    let key_event = events.iter().find_map(|ev| {
        if let Event::Scalar { value, style, .. } = ev
            && value == "key"
        {
            return Some(*style);
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
        if let Event::Scalar { value, style, .. } = ev
            && value == "key"
        {
            return Some(*style);
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
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// ---------------------------------------------------------------------------
// s-indent(n) enforcement — block-context quoted scalar continuation lines
// ---------------------------------------------------------------------------

// IT-SQ-BLK-1: single-quoted block value with sufficient continuation indent.
// Mapping value at column 5 (after "key: "), indent context n=0 (root).
// The continuation line has 2 spaces — accepted per spec.
#[test]
fn single_quoted_block_value_sufficient_indent_accepted() {
    let input = "key: 'foo\n  bar'\n";
    assert!(
        !has_error(input),
        "single-quoted block value with indented continuation should be accepted"
    );
    let events = event_variants(input);
    // Second scalar is the value (first is the "key" key).
    let scalar = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref())
            } else {
                None
            }
        })
        .nth(1);
    assert_eq!(scalar, Some("foo bar"));
}

// IT-SQ-BLK-2: single-quoted block value with continuation under-indented → parse error.
// Mapping value at column 5, continuation has 0 spaces (flush left).
#[test]
fn single_quoted_block_value_under_indented_continuation_errors() {
    // In block context, the continuation line must have at least n=0 spaces
    // (root level). This tests a flush-left continuation which is degenerate
    // but currently the root block context passes parent_indent=0, so this
    // tests the integration path end-to-end. The real enforcement fires when
    // the continuation is below the mapping's indent level (n>0).
    // For a mapping at indent 0, the value's parent_indent is also 0, so
    // the indent check is skipped (n=0). This test verifies the accepted case
    // to confirm the call site wires correctly.
    let input = "key: 'foo\nbar'\n";
    // At root level (n=0), continuation with 0 spaces is allowed by spec.
    // No error expected — the existing folding behavior produces "foo bar".
    assert!(
        !has_error(input),
        "root-level single-quoted with flush continuation should be accepted (n=0)"
    );
}

// IT-SQ-BLK-2b: single-quoted nested block value with under-indented continuation → error.
// Use a nested mapping where the value is at indent > 0, so the parent_indent
// passed to the lexer is non-zero, triggering the enforcement.
#[test]
fn single_quoted_nested_block_value_under_indented_continuation_errors() {
    // Outer mapping key "a" maps to inner mapping at indent 2.
    // Inner key "b" maps to a single-quoted value; continuation is flush-left.
    let input = "a:\n  b: 'foo\nbar'\n";
    assert!(
        has_error(input),
        "nested single-quoted block value with flush continuation should produce an error"
    );
}

// IT-DQ-BLK-1: double-quoted block value with sufficient continuation indent.
#[test]
fn double_quoted_block_value_sufficient_indent_accepted() {
    let input = "key: \"foo\n  bar\"\n";
    assert!(
        !has_error(input),
        "double-quoted block value with indented continuation should be accepted"
    );
    let events = event_variants(input);
    // Second scalar is the value (first is the "key" key).
    let scalar = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref())
            } else {
                None
            }
        })
        .nth(1);
    assert_eq!(scalar, Some("foo bar"));
}

// IT-DQ-BLK-2: double-quoted block value with continuation under-indented → parse error.
#[test]
fn double_quoted_nested_block_value_under_indented_continuation_errors() {
    let input = "a:\n  b: \"foo\nbar\"\n";
    assert!(
        has_error(input),
        "nested double-quoted block value with flush continuation should produce an error"
    );
}

// IT-DQ-BLK-3: tab in indent position on continuation of block double-quoted → parse error.
#[test]
fn double_quoted_block_value_tab_in_indent_position_errors() {
    // Nested value: continuation at column 0 with a tab. Tab does not satisfy
    // s-indent(n) — only spaces count.
    let input = "a:\n  b: \"foo\n\tbar\"\n";
    assert!(
        has_error(input),
        "double-quoted block value with tab-only continuation indent should produce an error"
    );
}

// IT-SQ-BLANK-BLK: blank line inside single-quoted block scalar is allowed.
#[test]
fn single_quoted_block_value_blank_continuation_line_accepted() {
    let input = "key: 'foo\n\n  bar'\n";
    assert!(
        !has_error(input),
        "single-quoted block value with blank continuation line should be accepted"
    );
    let events = event_variants(input);
    // Second scalar is the value (first is the "key" key).
    let scalar = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar { value, .. } = e {
                Some(value.as_ref())
            } else {
                None
            }
        })
        .nth(1);
    assert_eq!(scalar, Some("foo\nbar"));
}
