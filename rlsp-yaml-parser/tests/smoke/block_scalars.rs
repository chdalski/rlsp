use rstest::rstest;

use super::*;

// Helper: make a literal Scalar event for easy comparison.
fn literal(value: &str, chomp: Chomp) -> Event<'_> {
    Event::Scalar {
        value: value.into(),
        style: ScalarStyle::Literal(chomp),
        anchor: None,
        tag: None,
    }
}

// IT-LB-1 through IT-LB-5, IT-LB-7, IT-LB-13, IT-LB-14:
// Literal block scalar emits correct full event sequence (implicit document).
#[rstest]
// IT-LB-1 (spike): simple literal block scalar; validates `|` dispatch and Literal(Clip) style.
#[case::spike_simple_literal_clip("|\n  hello\n", "hello\n", Chomp::Clip)]
// IT-LB-2: Strip chomping removes all trailing newlines.
#[case::strip_chomping_removes_trailing_newlines("|-\n  foo\n\n", "foo", Chomp::Strip)]
// IT-LB-3: Keep chomping retains all trailing blank lines (content + blank = two newlines).
#[case::keep_chomping_retains_trailing_newlines("|+\n  foo\n\n", "foo\n\n", Chomp::Keep)]
// IT-LB-4: Clip chomping keeps exactly one trailing newline (trailing blank dropped).
#[case::clip_chomping_keeps_one_trailing_newline("|\n  foo\n\n", "foo\n", Chomp::Clip)]
// IT-LB-5: Explicit indent indicator; `|2` forces content_indent=2 regardless of auto-detect.
#[case::explicit_indent_indicator("|2\n  foo\n", "foo\n", Chomp::Clip)]
// IT-LB-7: Empty scalar — header only, no content; Clip with empty input yields "".
#[case::empty_literal_clip_yields_empty_string("|\n", "", Chomp::Clip)]
// IT-LB-13: Multi-line content with blank line between lines; blank becomes a newline.
#[case::multiline_content_with_blank_line_between(
    "|\n  foo\n\n  bar\n",
    "foo\n\nbar\n",
    Chomp::Clip
)]
// IT-LB-14: Leading blank before first content line is included as newline (l-empty per spec §8.1.2).
#[case::leading_blank_before_first_content("|\n\n  foo\n", "\nfoo\n", Chomp::Clip)]
fn literal_scalar_emits_full_event_sequence(
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
            literal(expected_value, chomp),
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// IT-LB-6 — Literal terminated by dedent.
// "|\n  foo\nkey: val\n" → "foo\n" (the `key:` line has indent 0 < 2)
#[test]
fn literal_terminated_by_dedent() {
    // The `key: val` line is not part of the scalar — it terminates it.
    // After the scalar, the remaining content is consumed by the fallback
    // (plain scalar handler, not yet mapping-aware), but the scalar value
    // itself is "foo\n".
    let events = parse_to_vec("|\n  foo\nbar\n");
    // Find the first scalar event.
    let scalar_event = events.iter().find_map(|r| match r {
        Ok((
            Event::Scalar {
                value,
                style: ScalarStyle::Literal(_),
                ..
            },
            _,
        )) => Some(value.as_ref()),
        _ => None,
    });
    assert_eq!(scalar_event, Some("foo\n"));
}

// IT-LB-8 — Literal block scalar in explicit document.
#[test]
fn literal_block_scalar_in_explicit_document() {
    let events = event_variants("---\n|\n  hello world\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            literal("hello world\n", Chomp::Clip),
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

// IT-LB-9 — Span: start at `|`, end after all consumed lines.
#[test]
fn span_start_at_pipe_byte_offset() {
    let results = parse_to_vec("|\n  hello\n");
    let scalar_span = results.iter().find_map(|r| match r {
        Ok((
            Event::Scalar {
                style: ScalarStyle::Literal(_),
                ..
            },
            span,
        )) => Some(*span),
        _ => None,
    });
    let span = scalar_span.unwrap_or_else(|| unreachable!("expected a Literal scalar event"));
    // `|` is at byte 0.
    assert_eq!(span.start.byte_offset, 0, "span must start at the `|`");
}

// IT-LB-10 — Span: end after all content lines are consumed.
#[test]
fn span_end_after_all_consumed_lines() {
    // "|\n  hello\n" = 10 bytes total.
    let results = parse_to_vec("|\n  hello\n");
    let scalar_span = results.iter().find_map(|r| match r {
        Ok((
            Event::Scalar {
                style: ScalarStyle::Literal(_),
                ..
            },
            span,
        )) => Some(*span),
        _ => None,
    });
    let span = scalar_span.unwrap_or_else(|| unreachable!("expected a Literal scalar event"));
    assert_eq!(span.end.byte_offset, 10, "span must end after all 10 bytes");
}

// IT-LB-11 — Span: start at `|` when `|` is after leading whitespace.
#[test]
fn span_start_accounts_for_leading_whitespace() {
    // "  |\n    hello\n": `|` is at byte offset 2.
    let results = parse_to_vec("  |\n    hello\n");
    let scalar_span = results.iter().find_map(|r| match r {
        Ok((
            Event::Scalar {
                style: ScalarStyle::Literal(_),
                ..
            },
            span,
        )) => Some(*span),
        _ => None,
    });
    let span = scalar_span.unwrap_or_else(|| unreachable!("expected a Literal scalar event"));
    assert_eq!(
        span.start.byte_offset, 2,
        "span must start at `|` byte offset"
    );
}

// IT-LB-12 — Error path: invalid indicator character.
#[test]
fn invalid_indicator_character_produces_error() {
    // "|!\n  hello\n" → Err (invalid indicator `!`)
    let results = parse_to_vec("|!\n  hello\n");
    let has_err = results.iter().any(Result::is_err);
    assert!(has_err, "expected a parse error for invalid indicator `!`");
}

// ---------------------------------------------------------------------------
// Task 8 conformance fixtures
// ---------------------------------------------------------------------------

// CF-LB-1: DWX9 — "Spec Example 8.8. Literal Content"
// A top-level literal block scalar with leading blank lines, embedded blank,
// and trailing comment. The spec example uses `|` (Clip).
// yaml: "|\n \n  \n  literal\n   \n  \n  text\n\n # Comment\n"
// Expected value: "\n\nliteral\n \n\ntext\n"
// Note: leading blank lines (` ` and `  `) become content (indent detected=2),
//   the ` ` line becomes "\n", the `   ` line becomes " \n", and trailing
//   comment terminates the block.
#[test]
fn cf_lb1_dwx9_spec_example_8_8_literal_content() {
    // From yaml-test-suite/src/DWX9.yaml
    // Spaces on otherwise-blank lines are intentional (spec example uses ␣).
    let input = "|\n \n  \n  literal\n   \n  \n  text\n\n # Comment\n";
    let results = parse_to_vec(input);
    let scalar = results.iter().find_map(|r| match r {
        Ok((
            Event::Scalar {
                value,
                style: ScalarStyle::Literal(_),
                ..
            },
            _,
        )) => Some(value.as_ref()),
        _ => None,
    });
    assert_eq!(scalar, Some("\n\nliteral\n \n\ntext\n"));
}

// CF-LB-2: 96NN — "Leading tab content in literals"
// A `|-` literal with tab character after indent spaces.
// yaml: "foo: |-\n \t\tbar\n"
// Expected scalar value for the `foo` key: "\tbar" (tab preserved)
// Note: Task 8 only handles top-level scalars; the mapping key is consumed
// as a plain scalar. We test only that the literal scalar value is correct.
#[test]
fn cf_lb2_96nn_tab_in_content_preserved() {
    // From yaml-test-suite/src/96NN.yaml — tab after indent spaces is content.
    // At top level with indent=1: "|-\n \t\tbar\n"
    // parent_indent=0, auto-detect finds indent=1, content= "\t\tbar" (tab+tab+bar)
    // Strip chomping → "\t\tbar" (no trailing newline).
    let input = "|-\n \t\tbar\n";
    let results = parse_to_vec(input);
    let scalar = results.iter().find_map(|r| match r {
        Ok((
            Event::Scalar {
                value,
                style: ScalarStyle::Literal(_),
                ..
            },
            _,
        )) => Some(value.as_ref()),
        _ => None,
    });
    assert_eq!(scalar, Some("\t\tbar"));
}

// CF-LB-3: M29M — "Literal Block Scalar" (NimYAML test)
// Mapping with literal scalar value containing blank lines.
// Top-level test: we test only the scalar content without mapping support.
// yaml: "|\n ab\n \n cd\n ef\n \n\n...\n"
// Expected: "ab\n\ncd\nef\n" (Clip)
#[test]
fn cf_lb3_m29m_literal_block_with_blank_lines() {
    // From yaml-test-suite/src/M29M.yaml — literal block with embedded blanks.
    let input = "|\n ab\n \n cd\n ef\n \n\n...\n";
    let results = parse_to_vec(input);
    let scalar = results.iter().find_map(|r| match r {
        Ok((
            Event::Scalar {
                value,
                style: ScalarStyle::Literal(_),
                ..
            },
            _,
        )) => Some(value.as_ref()),
        _ => None,
    });
    assert_eq!(scalar, Some("ab\n\ncd\nef\n"));
}
