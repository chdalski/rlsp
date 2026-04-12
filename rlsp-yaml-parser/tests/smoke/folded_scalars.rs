use rstest::rstest;

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

// IT-FB-1 through IT-FB-20 (excluding 19): folded scalar emits correct full event
// sequence (implicit document). Cases cover folding rules, chomping, explicit indent,
// and edge cases.
#[rstest]
// IT-FB-1 (spike): two-line folded scalar; validates `>` dispatch and Folded(Clip) style.
#[case::spike_two_line_break_becomes_space(">\n  foo\n  bar\n", "foo bar\n", Chomp::Clip)]
// IT-FB-2: single non-blank line is not folded (no preceding content to join).
#[case::single_line_not_folded(">\n  hello\n", "hello\n", Chomp::Clip)]
// IT-FB-3: three equally-indented non-blank lines — all breaks folded to spaces.
#[case::three_lines_all_breaks_become_spaces(">\n  a\n  b\n  c\n", "a b c\n", Chomp::Clip)]
// IT-FB-4: one blank line between non-blank lines produces one newline (§8.1.3).
#[case::one_blank_line_produces_one_newline(">\n  foo\n\n  bar\n", "foo\nbar\n", Chomp::Clip)]
// IT-FB-5: two blank lines between non-blank lines produce two newlines.
#[case::two_blank_lines_produce_two_newlines(">\n  foo\n\n\n  bar\n", "foo\n\nbar\n", Chomp::Clip)]
// IT-FB-6: more-indented line — break before preserved as `\n`; relative indent kept.
#[case::more_indented_break_before_preserved(
    ">\n  normal\n    indented\n",
    "normal\n  indented\n",
    Chomp::Clip
)]
// IT-FB-7: breaks surrounding more-indented region both preserved (§8.1.3).
#[case::breaks_surrounding_more_indented_both_preserved(
    ">\n  a\n    b\n  c\n",
    "a\n  b\nc\n",
    Chomp::Clip
)]
// IT-FB-8: all lines at same deeper indent — auto-detect, normal folding.
#[case::all_deep_lines_equally_indented_normal_folding(
    ">\n    deep\n    also deep\n",
    "deep also deep\n",
    Chomp::Clip
)]
// IT-FB-9: Strip (`>-`) — trailing newlines removed.
#[case::strip_chomp_removes_trailing_newlines(">-\n  foo\n\n", "foo", Chomp::Strip)]
// IT-FB-10: Keep (`>+`) — trailing blank lines preserved.
#[case::keep_chomp_preserves_trailing_blank_lines(">+\n  foo\n\n", "foo\n\n", Chomp::Keep)]
// IT-FB-11: Clip (`>`) — single trailing newline kept, extra blanks dropped.
#[case::clip_chomp_keeps_one_trailing_newline(">\n  foo\n\n", "foo\n", Chomp::Clip)]
// IT-FB-12: explicit indent indicator `>2`.
#[case::explicit_indent_indicator(">2\n  foo\n  bar\n", "foo bar\n", Chomp::Clip)]
// IT-FB-13: explicit indent with strip, chomp-then-indent order `>-2`.
#[case::explicit_indent_with_strip_chomp_then_indent(">-2\n  foo\n", "foo", Chomp::Strip)]
// IT-FB-14: explicit indent with keep, chomp-then-indent order `>+2`.
#[case::explicit_indent_with_keep_chomp_then_indent(">+2\n  foo\n\n", "foo\n\n", Chomp::Keep)]
// IT-FB-15: explicit indent with strip, indent-then-chomp order `>2-` (both orderings accepted).
#[case::explicit_indent_with_strip_indent_then_chomp(">2-\n  foo\n", "foo", Chomp::Strip)]
// IT-FB-16: empty folded scalar (header only, no content) yields "".
#[case::empty_folded_scalar_yields_empty_string(">\n", "", Chomp::Clip)]
// IT-FB-17: all-blank content (blank lines only, no non-blank lines) yields "" with Clip.
#[case::all_blank_content_yields_empty_string(">\n\n\n", "", Chomp::Clip)]
// IT-FB-18: single-line with trailing blanks (Keep preserves all trailing blank lines).
#[case::keep_chomp_with_multiple_trailing_blanks(">+\n  only\n\n\n", "only\n\n\n", Chomp::Keep)]
// IT-FB-20: leading blank before first content line becomes a newline prefix (l-empty).
#[case::leading_blank_before_first_content(">\n\n  foo\n", "\nfoo\n", Chomp::Clip)]
fn folded_scalar_emits_full_event_sequence(
    #[case] input: &str,
    #[case] expected_value: &str,
    #[case] chomp: Chomp,
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
            folded(expected_value, chomp),
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
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
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

// IT-FB-24, IT-FB-25, IT-FB-26: invalid folded scalar headers produce at least one Err.
#[rstest]
// IT-FB-24: invalid indicator character — `!` is not a valid chomping or indent indicator.
#[case::invalid_indicator_character(">!\n  hello\n")]
// IT-FB-25: indent indicator `0` is invalid per spec (indentation must be 1–9).
#[case::indent_indicator_zero_is_invalid(">0\n  hello\n")]
// IT-FB-26: duplicate chomp indicator `++` is invalid.
#[case::duplicate_chomp_indicator_is_invalid(">++\n  hello\n")]
fn folded_scalar_invalid_header_produces_err(#[case] input: &str) {
    let results = parse_to_vec(input);
    assert!(
        results.iter().any(Result::is_err),
        "expected at least one Err for invalid header in: {input:?}"
    );
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
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
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
