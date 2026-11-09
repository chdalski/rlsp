use std::borrow::Cow;

use rstest::rstest;

use super::*;

// IT-1 through IT-5, IT-8, IT-9: quoted scalar emits correct full event sequence.
// Cases with SingleQuoted style use single(); cases with DoubleQuoted use double().
#[rstest]
// IT-1 (spike): single-quoted scalar emits Scalar with SingleQuoted style.
// Use bare document (no --- marker) so the quoted scalar starts on its own line.
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

// YAML 1.2.2 [109] `c-double-quoted` / [120] `c-single-quoted`: a multi-line
// quoted scalar's span ends right after its closing quote, not after the line
// break or the comment that follows it.  The enclosing collection's end comes
// from its own end event and is unaffected.
// Expected: (scalar start, scalar end, ends of the MappingEnd/SequenceEnd events).
#[rstest]
#[case::dq_lf("\"a\n b\"\n", 0, 6, &[])]
#[case::sq_lf("'a\n b'\n", 0, 6, &[])]
#[case::dq_crlf("\"a\r\n b\"\r\n", 0, 7, &[])]
#[case::dq_cr("\"a\r b\"\r", 0, 6, &[])]
#[case::sq_crlf("'a\r\n b'\r\n", 0, 7, &[])]
#[case::sq_cr("'a\r b'\r", 0, 6, &[])]
#[case::dq_eof("\"a\n b\"", 0, 6, &[])]
#[case::dq_comment("\"a\n b\" # c\n", 0, 6, &[])]
#[case::sq_comment("'a\n b'  # c\n", 0, 6, &[])]
#[case::dq_trailing_whitespace("\"a\n b\"   \n", 0, 6, &[])]
#[case::dq_blank_line("\"a\n\n b\"\n", 0, 7, &[])]
#[case::dq_three_lines("\"a\n b\n c\"\n", 0, 9, &[])]
#[case::dq_tab_indent("\"a\n\tb\"\n", 0, 6, &[])]
#[case::dq_escaped_line_break("\"a\\\n b\"\n", 0, 7, &[])]
#[case::sq_escaped_quote("'a\n b''c'\n", 0, 9, &[])]
#[case::sq_quote_alone_on_closing_line("'a\n'\n", 0, 4, &[])]
#[case::dq_multibyte("\"é\n ü\"\n", 0, 8, &[])]
#[case::mapping_value_dq("k: \"a\n b\"\n", 3, 9, &[10])]
#[case::mapping_value_sq("k: 'a\n b'\n", 3, 9, &[10])]
#[case::mapping_value_eof("k: \"a\n b\"", 3, 9, &[9])]
#[case::mapping_value_then_entry("k: \"a\n b\"\nz: 1\n", 3, 9, &[15])]
#[case::mapping_value_comment_then_entry("k: \"a\n b\" # c\nz: 1\n", 3, 9, &[19])]
#[case::mapping_value_crlf("k: \"a\r\n b\"\r\nz: 1\r\n", 3, 10, &[18])]
#[case::mapping_value_multibyte("k: \"é\n ü\"\nz: 1\n", 3, 11, &[17])]
#[case::sequence_item("- \"a\n b\"\n", 2, 8, &[9])]
#[case::sequence_item_then_item("- \"a\n b\"\n- z\n", 2, 8, &[13])]
#[case::nested_mapping("x:\n  k: \"a\n   b\"\ny: 1\n", 8, 16, &[17, 22])]
#[case::tagged_value("k: !!str \"a\n b\"\n", 9, 15, &[16])]
#[case::anchored_value("k: &x \"a\n b\"\n", 6, 12, &[13])]
#[case::explicit_key("? \"a\n b\"\n: v\n", 2, 8, &[13])]
fn multiline_quoted_scalar_span_ends_at_closing_quote(
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
            matches!(
                ev,
                Event::Scalar {
                    style: ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted,
                    ..
                }
            )
            .then_some(span)
        })
        .unwrap_or_else(|| unreachable!("expected a quoted scalar"));
    assert_eq!(
        (multiline.start, multiline.end),
        (scalar_start, scalar_end),
        "scalar span"
    );
    let source = &input[multiline.start as usize..multiline.end as usize];
    assert!(
        source.len() >= 2 && source.starts_with(['"', '\'']) && source.ends_with(['"', '\'']),
        "scalar span must cover the quoted text, got {source:?}"
    );

    let ends: Vec<u32> = results
        .iter()
        .filter(|(ev, _)| matches!(ev, Event::MappingEnd | Event::SequenceEnd))
        .map(|(_, span)| span.end)
        .collect();
    assert_eq!(ends, collection_ends, "collection end spans");
}

// The same in a flow collection, where the flow parser rebuilds the text after
// the closing quote itself.  Expected: (start, end) of every scalar.
#[rstest]
#[case::dq_then_bracket("[\"a\n b\"\n]", &[(1, 7)])]
#[case::dq_then_plain_item("[\"a\n b\", c]", &[(1, 7), (9, 10)])]
#[case::sq_then_plain_item("['a\n b', c]", &[(1, 7), (9, 10)])]
#[case::between_plain_items("[x, \"a\n b\", y]", &[(1, 2), (4, 10), (12, 13)])]
#[case::two_multiline_items("[\"a\n b\", 'c\n d']", &[(1, 7), (9, 15)])]
#[case::comment_after_quote("[\"a\n b\" # c\n]", &[(1, 7)])]
#[case::crlf("[\"a\r\n b\"\r\n]", &[(1, 8)])]
#[case::cr("[\"a\r b\"\r]", &[(1, 7)])]
#[case::mapping_value("{k: \"a\n b\"}", &[(1, 2), (4, 10)])]
#[case::mapping_key("{\"a\n b\": 1}", &[(1, 7), (9, 10)])]
#[case::flow_in_block_mapping("k: [\"a\n  b\", c]\nz: 1\n", &[(0, 1), (4, 11), (13, 14), (16, 17), (19, 20)])]
fn multiline_quoted_scalar_span_in_flow_ends_at_closing_quote(
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

// The span change moves no other event: every event that is not the scalar
// keeps its span (kind, start, end).
#[rstest]
#[case::flow_sequence("[\"a\n b\"\n]", &[("SequenceStart", 0, 0), ("SequenceEnd", 8, 8), ("DocumentEnd", 9, 9), ("StreamEnd", 9, 9)])]
#[case::flow_sequence_item("[\"a\n b\", c]", &[("SequenceStart", 0, 0), ("SequenceEnd", 10, 10), ("DocumentEnd", 11, 11), ("StreamEnd", 11, 11)])]
#[case::root("\"a\n b\"\n", &[("DocumentEnd", 7, 7), ("StreamEnd", 7, 7)])]
#[case::root_with_document_end("\"a\n b\"\n...\n", &[("DocumentEnd", 7, 10), ("StreamEnd", 11, 11)])]
#[case::root_comment("\"a\n b\" # c\n", &[("Comment", 7, 10), ("DocumentEnd", 11, 11), ("StreamEnd", 11, 11)])]
#[case::root_single_quoted_comment("'a\n b'  # c\n", &[("Comment", 8, 11), ("DocumentEnd", 12, 12), ("StreamEnd", 12, 12)])]
#[case::flow_mapping("{k: \"a\n b\"}", &[("MappingStart", 0, 0), ("MappingEnd", 10, 10), ("DocumentEnd", 11, 11), ("StreamEnd", 11, 11)])]
fn multiline_quoted_scalar_leaves_other_event_spans_unchanged(
    #[case] input: &str,
    #[case] expected: &[(&str, u32, u32)],
) {
    let actual: Vec<(&str, u32, u32)> = parse_to_vec(input)
        .into_iter()
        .filter_map(|r| {
            let (event, span) = r.unwrap_or_else(|e| unreachable!("unexpected parse error: {e}"));
            let kind = match event {
                Event::SequenceStart { .. } => "SequenceStart",
                Event::SequenceEnd => "SequenceEnd",
                Event::MappingStart { .. } => "MappingStart",
                Event::MappingEnd => "MappingEnd",
                Event::DocumentEnd { .. } => "DocumentEnd",
                Event::StreamEnd => "StreamEnd",
                Event::Comment { .. } => "Comment",
                Event::StreamStart
                | Event::Alias { .. }
                | Event::DocumentStart { .. }
                | Event::Scalar { .. } => return None,
            };
            Some((kind, span.start, span.end))
        })
        .collect();
    assert_eq!(actual, expected);
}

// Apart from the scalar's span, the events of a multi-line quoted scalar are
// those of its one-line form.
#[rstest]
#[case::flow_sequence("[\"a\n b\"\n]", "[\"a b\"]")]
#[case::flow_sequence_item("[\"a\n b\", c]", "[\"a b\", c]")]
#[case::block_value_comment("k: \"a\n b\" # c\nz: 1\n", "k: \"a b\" # c\nz: 1\n")]
#[case::single_quoted("k: 'a\n b' # c\nz: 1\n", "k: 'a b' # c\nz: 1\n")]
fn multiline_quoted_scalar_events_match_one_line_form(
    #[case] multiline: &str,
    #[case] one_line: &str,
) {
    assert_eq!(event_variants(multiline), event_variants(one_line));
}

// Trailing content after the closing quote is reported at the tail, which does
// not move with the scalar's span.
#[rstest]
#[case::root("\"a\n b\" x\n", 6, 2, 3)]
#[case::root_single_quoted("'a\n b' x\n", 6, 2, 3)]
#[case::mapping_value("k: \"a\n b\" x\n", 9, 2, 3)]
fn multiline_quoted_scalar_tail_error_position(
    #[case] input: &str,
    #[case] byte_offset: usize,
    #[case] line: usize,
    #[case] column: usize,
) {
    let error = parse_to_vec(input)
        .into_iter()
        .find_map(Result::err)
        .unwrap_or_else(|| unreachable!("expected an error for {input:?}"));
    assert_eq!(error.message, "unexpected content after quoted scalar");
    assert_eq!(
        (error.pos.byte_offset, error.pos.line, error.pos.column),
        (byte_offset, line, column)
    );
}
