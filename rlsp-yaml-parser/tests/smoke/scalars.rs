use super::*;
use rstest::rstest;

// Helper: make a plain `Scalar` event for easy comparison.
fn plain(value: &str) -> Event<'_> {
    Event::Scalar {
        value: value.into(),
        style: ScalarStyle::Plain,
        meta: None,
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
    assert_eq!(span.start, 0);
}

// IT-S9 — span end byte offset for plain scalar.
#[test]
fn plain_scalar_span_end_after_value() {
    // "hello" = 5 bytes; span end at byte 5.
    let results = parse_to_vec("hello");
    let Some(Ok((Event::Scalar { .. }, span))) = results.get(2) else {
        unreachable!("expected Scalar as third event");
    };
    assert_eq!(span.end, 5);
}

// IT-S10 — span start for indented scalar.
#[test]
fn plain_scalar_indented_span_start() {
    // "  hello" — leading 2 spaces, scalar starts at byte 2.
    let results = parse_to_vec("  hello");
    let Some(Ok((Event::Scalar { .. }, span))) = results.get(2) else {
        unreachable!("expected Scalar as third event");
    };
    assert_eq!(span.start, 2);
}

// A multi-line plain scalar's span ends at its last content byte; the spans of
// the enclosing mapping/sequence end events (in event order) stay where the
// consumed trailing blank lines put them.
#[rstest]
#[case::mapping_value("a:\n  v\n  w\n", 5, 10, &[11])]
#[case::mapping_value_eof_without_newline("a:\n  v\n  w", 5, 10, &[10])]
#[case::blank_lines_before_next_key("a:\n  v\n  w\n\n\nb: 1\n", 5, 10, &[18])]
#[case::whitespace_only_lines_before_next_key("a:\n  v\n  w\n  \n  \nb: 1\n", 5, 10, &[22])]
#[case::nested_mapping("x:\n  a:\n    v\n    w\n\ny: 1\n", 12, 19, &[21, 26])]
#[case::sequence_item("- v\n  w\n\n- z\n", 2, 7, &[13])]
#[case::sequence_item_before_comment_line("- v\n  w\n# d\n- z\n", 2, 7, &[16])]
#[case::comment_line_after_value("a:\n  v\n  w\n# d\nb: 1\n", 5, 10, &[20])]
#[case::trailing_comment_on_last_line("a:\n  v\n  w # c\nb: 1\n", 5, 10, &[20])]
#[case::trailing_spaces_on_last_line("a:\n  v\n  w  \nb: 1\n", 5, 10, &[18])]
#[case::crlf_line_breaks("a:\n  v\r\n  w\r\n\r\nb: 1\r\n", 5, 11, &[21])]
#[case::multibyte_content("a:\n  é\n  ü\n", 5, 12, &[13])]
#[case::blank_line_between_lines("a:\n  v\n\n  w\n\nb: 1\n", 5, 11, &[18])]
#[case::sequence_item_blank_line_inside("- a\n\n  b\n", 2, 8, &[9])]
#[case::nested_sequence_item_blank_line_inside("a:\n  - b\n\n    c\n", 7, 15, &[16, 16])]
#[case::sequence_item_whitespace_only_line_inside("- a\n  \n  b\n", 2, 10, &[11])]
#[case::sequence_item_crlf_blank_line_inside("- a\r\n\r\n  b\r\n", 2, 10, &[12])]
#[case::sequence_item_two_blank_lines_inside("- a\n\n\n  b\n", 2, 9, &[10])]
#[case::sequence_item_blank_line_inside_then_next_item("- a\n\n  b\n\n- z\n", 2, 8, &[14])]
#[case::sequence_item_multibyte_blank_line_inside("- ä\n\n  ö\n", 2, 10, &[11])]
#[case::sequence_item_trailing_blank_lines_consumed("- a\n\n  b\n\n\n", 2, 8, &[11])]
fn multiline_plain_scalar_span_ends_at_last_content_byte(
    #[case] input: &str,
    #[case] scalar_start: u32,
    #[case] scalar_end: u32,
    #[case] collection_ends: &[u32],
) {
    let results: Vec<(Event<'_>, Span)> = parse_to_vec(input)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| unreachable!("unexpected parse error: {e}")))
        .collect();

    let multiline = results
        .iter()
        .find_map(|(ev, span)| {
            if let Event::Scalar { value, .. } = ev
                && value.contains([' ', '\n'])
            {
                Some(span)
            } else {
                None
            }
        })
        .unwrap_or_else(|| unreachable!("expected a multi-line plain scalar"));
    assert_eq!(
        (multiline.start, multiline.end),
        (scalar_start, scalar_end),
        "scalar span"
    );

    let ends: Vec<u32> = results
        .iter()
        .filter(|(ev, _)| matches!(ev, Event::MappingEnd | Event::SequenceEnd))
        .map(|(_, span)| span.end)
        .collect();
    assert_eq!(ends, collection_ends, "collection end spans");
}

// A single-line plain scalar keeps its span regardless of blank lines after it.
#[test]
fn single_line_plain_scalar_span_ignores_trailing_blank_lines() {
    let results = parse_to_vec("a: v\n\n\nb: 1\n");
    let Some(Ok((Event::Scalar { .. }, span))) = results.get(4) else {
        unreachable!("expected the value of `a` as fifth event");
    };
    assert_eq!((span.start, span.end), (3, 4));
}

// A multi-line plain scalar inside a flow collection: the span covers the
// first line through the last content byte of the last continuation line.
// Expected: (start, end) of every scalar that is not a lone single-line word.
#[rstest]
#[case::two_lines_in_sequence("[a\n  b\n]\n", &[(1, 6)])]
#[case::three_lines_in_sequence("[a\n  b\n  c\n]\n", &[(1, 10)])]
#[case::blank_line_between_lines("[a\n\n  b\n]\n", &[(1, 7)])]
#[case::multiline_mapping_key_with_value("{a b\n  c: d}\n", &[(1, 8), (10, 11)])]
#[case::next_entry_span_unchanged("[a\n  b, c]\n", &[(1, 6), (8, 9)])]
#[case::multibyte_continuation("[é\n  ü\n]\n", &[(1, 8)])]
#[case::continuation_on_later_line("[a\n  b, c\n  d]\n", &[(1, 6), (8, 13)])]
#[case::scalar_after_comma_then_continuation("[a, b\n  c\n]", &[(1, 2), (4, 9)])]
#[case::crlf_line_breaks("[a\r\n  b\r\n]\r\n", &[(1, 7)])]
#[case::trailing_whitespace_on_continuation("[a\n  b  \n]\n", &[(1, 6)])]
#[case::comment_on_continuation_line("[a\n  b # c\n]\n", &[(1, 6)])]
#[case::flow_in_block_mapping("k: [a\n  b]\nm: 1\n", &[(0, 1), (4, 9), (11, 12), (14, 15)])]
fn flow_multiline_plain_scalar_span_ends_at_last_content_byte(
    #[case] input: &str,
    #[case] scalar_spans: &[(u32, u32)],
) {
    let spans: Vec<(u32, u32)> = parse_to_vec(input)
        .into_iter()
        .filter_map(|r| match r {
            Ok((Event::Scalar { .. }, span)) => Some((span.start, span.end)),
            _ => None,
        })
        .collect();
    assert_eq!(spans, scalar_spans);
}

// A single-quoted scalar followed by a comment has the spans (scalar and any
// comment event) the double-quoted form has.
#[test]
fn single_quoted_scalar_with_trailing_comment_matches_double_quoted_spans() {
    let spans = |input: &str| -> Vec<(u32, u32)> {
        parse_to_vec(input)
            .into_iter()
            .filter_map(|r| match r {
                Ok((Event::Scalar { .. } | Event::Comment { .. }, span)) => {
                    Some((span.start, span.end))
                }
                _ => None,
            })
            .collect()
    };
    let single = spans("- 'q' # c\n");
    assert_eq!(single, spans("- \"q\" # c\n"));
    assert_eq!(single.first(), Some(&(2, 5)));
}

// A plain scalar cannot start with an indicator (YAML 1.2.2 [126]
// `ns-plain-first`).  `,`, `@`, the backtick and an indented `%` are errors, not
// an empty node that drops their text.
#[rstest]
#[case::comma_as_a_mapping_value("v: ,\n")]
#[case::comma_then_text_as_a_mapping_value("v: ,a\n")]
#[case::comma_as_a_sequence_item("- ,\n")]
#[case::comma_then_text_as_a_sequence_item("- ,a\n")]
#[case::comma_as_the_root("a\n---\n,\n")]
#[case::comma_as_a_bare_root(",a\n")]
#[case::comma_as_a_key(",: v\n")]
#[case::at_sign_as_a_mapping_value("v: @a\n")]
#[case::at_sign_as_a_sequence_item("- @a\n")]
#[case::at_sign_as_a_key("@a: b\n")]
#[case::at_sign_as_the_root("@a\n")]
#[case::backtick_as_a_mapping_value("v: `a\n")]
#[case::backtick_as_a_sequence_item("- `a\n")]
#[case::backtick_as_a_key("`a: b\n")]
#[case::backtick_as_the_root("`a\n")]
#[case::percent_as_a_mapping_value("v: %a\n")]
#[case::percent_as_a_sequence_item("- %a\n")]
#[case::after_an_anchor("v: &x ,\n")]
#[case::after_a_tag("v: !t @a\n")]
#[case::on_its_own_line_below_a_key("a:\n  ,\n")]
#[case::in_an_explicit_key("? ,\n")]
#[case::on_the_marker_line_comma("--- ,\n")]
#[case::on_the_marker_line_comma_then_text("--- ,x\n")]
#[case::on_the_marker_line_percent("--- %a\n")]
#[case::on_the_marker_line_at_sign("--- @a\n")]
#[case::on_the_marker_line_backtick("--- `a\n")]
fn reserved_indicator_cannot_start_a_plain_scalar(#[case] input: &str) {
    assert!(has_error(input), "input: {input:?}");
}

// The same characters later in a scalar, in a continuation line, in a flow
// collection, in a quoted scalar or in a comment are content, and a directive
// still starts with `%` at the start of a line.
#[rstest]
#[case::comma_inside("v: a,b\n")]
#[case::at_sign_inside("v: a@b\n")]
#[case::backtick_inside("v: a`b\n")]
#[case::percent_inside("v: a%b\n")]
#[case::comma_in_a_continuation_line("a: b\n  ,c\n")]
#[case::at_sign_in_a_continuation_line("a: b\n  @c\n")]
#[case::percent_in_a_continuation_line("- a\n  %c\n")]
#[case::in_a_flow_item("v: [a@b, c`d, e%f]\n")]
#[case::quoted("v: \"@a\"\nw: ',a'\n")]
#[case::block_scalar_content("v: |\n  ,a\n  @b\n")]
#[case::in_a_comment("k: v # , @ `\n")]
#[case::directive("%YAML 1.2\n---\na: 1\n")]
#[case::directive_after_a_document_end("a: 1\n...\n%YAML 1.2\n---\nb: 2\n")]
fn reserved_indicator_elsewhere_is_content(#[case] input: &str) {
    assert!(!has_error(input), "input: {input:?}");
}

#[test]
fn reserved_indicator_error_points_at_the_indicator() {
    let error = parse_to_vec("a:\n  - b\n  - ,x\n")
        .into_iter()
        .find_map(Result::err)
        .expect("an error");
    assert_eq!((error.pos.line, error.pos.column), (3, 4));
}

// -----------------------------------------------------------------------
// Plain scalar continuation: the root versus a collection at column 0
// -----------------------------------------------------------------------
// YAML 1.2.2 [207]/[197]: the root node is at indentation -1, so its
// continuation lines ([134]/[135]) may start at column 0.  The value of a
// block collection at column 0 is at indentation 0, so its continuation lines
// must be indented past the collection.  libfyaml 0.9.6 reports an error for
// `a:\nw\nx\n`.

#[rstest]
#[case::root_at_column_zero("x\ny\n", "x y")]
#[case::indented_root_with_column_zero_continuation("  x\ny\n", "x y")]
#[case::root_with_tab_continuation("x\n\ty\n", "x y")]
#[case::root_with_percent_line("x\n%y\n", "x %y")]
#[case::root_with_at_sign_line("x\n@y\n", "x @y")]
fn root_plain_scalar_continues_at_any_column(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(scalar_values(&evs(input)), [expected], "input: {input:?}");
}

#[rstest]
#[case::mapping_value_at_column_zero("a:\nw\nx\n")]
#[case::mapping_value_with_blank_line_between("a:\nw\n\nx\n")]
#[case::explicit_value_at_column_zero("? a\n:\nw\nx\n")]
#[case::mapping_value_after_a_line("a:\n  w\nx\n")]
#[case::mapping_entry_then_text("a: w\nx\n")]
#[case::sequence_entry_then_text("- w\nx\n")]
#[case::bare_dash_at_column_zero("-\nw\n")]
#[case::explicit_key_at_column_zero("?\nw\n")]
#[case::bare_dash_after_an_entry("- a\n-\nw\n")]
fn plain_text_at_column_zero_under_a_collection_is_an_error(#[case] input: &str) {
    let items = parse_to_vec(input);
    assert!(items.iter().any(Result::is_err), "input: {input:?}");
    let merged = items.iter().any(|item| {
        matches!(item, Ok((Event::Scalar { value, .. }, _)) if value == "w x" || value == "w\nx")
    });
    assert!(!merged, "the lines are merged: {input:?}");
}

#[rstest]
#[case::value_on_the_next_line("a:\nw\n", &["a", "w"])]
#[case::value_with_indented_continuation("a:\nw\n x\n", &["a", "w x"])]
#[case::value_on_the_key_line("a: w\n x\n", &["a", "w x"])]
#[case::value_with_tab_continuation("a: w\n\tx\n", &["a", "w x"])]
#[case::indented_value_with_shallower_continuation("a:\n  w\n x\n", &["a", "w x"])]
#[case::indented_value_with_continuation("a:\n  w\n  x\n", &["a", "w x"])]
#[case::sequence_item_with_continuation("- w\n x\n", &["w x"])]
fn plain_scalar_in_a_collection_at_column_zero_keeps_valid_continuations(
    #[case] input: &str,
    #[case] expected: &[&str],
) {
    assert_eq!(scalar_values(&evs(input)), expected, "input: {input:?}");
}
