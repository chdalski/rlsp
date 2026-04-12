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
