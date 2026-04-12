// SPDX-License-Identifier: MIT
//
// Tests verifying Span positions (byte_offset, line, column) for events
// produced from YAML input containing multi-byte UTF-8 characters.
//
// Tag names are ASCII-only per YAML 1.2 (URI characters), so no multi-byte
// tag content is possible in conforming input — tag tests are not needed.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rstest::rstest;

use rlsp_yaml_parser::{Event, Pos, ScalarStyle, Span, parse_events};

fn collect_events(input: &str) -> Vec<(Event<'_>, Span)> {
    parse_events(input)
        .map(|r| r.expect("parse error"))
        .collect()
}

fn assert_pos(label: &str, got: &Pos, byte: usize, line: usize, col: usize) {
    assert_eq!(got.byte_offset, byte, "{label}: byte_offset");
    assert_eq!(got.line, line, "{label}: line");
    assert_eq!(got.column, col, "{label}: column");
}

// ---------------------------------------------------------------------------
// Group 1: Baseline — ASCII scalars (all pass immediately)
// ---------------------------------------------------------------------------

/// Spike: plain scalar with ASCII content has correct `byte_offset`,
/// `line`, and `column` on both start and end.
#[test]
fn plain_scalar_ascii_byte_offset_correct() {
    let events = collect_events("hello\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == "hello"))
        .expect("Scalar(hello) not found");
    assert_pos("start", &span.start, 0, 1, 0);
    assert_pos("end", &span.end, 5, 1, 5);
}

/// Mapping key with ASCII content has correct positions.
#[test]
fn mapping_key_ascii_byte_offset_correct() {
    let events = collect_events("abc: xyz\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == "abc"))
        .expect("Scalar(abc) not found");
    assert_pos("start", &span.start, 0, 1, 0);
    assert_pos("end", &span.end, 3, 1, 3);
}

// ---------------------------------------------------------------------------
// Groups 2–9: Uniform plain-find start+end assertions
//
// All cases: collect_events(input), find Scalar by plain value match,
// assert_pos on both start and end.
// ---------------------------------------------------------------------------

// (start_byte, start_line, start_col), (end_byte, end_line, end_col)
#[rstest]
#[case::multibyte_key("日本語: value\n", "日本語", (0, 1, 0), (9, 1, 3))]
#[case::multibyte_value("日本語: val\n", "val", (11, 1, 5), (14, 1, 8))]
#[case::two_byte_char_value("ñ: ok\n", "ok", (4, 1, 3), (6, 1, 5))]
#[case::anchor_ascii_scalar("&foo bar\n", "bar", (5, 1, 5), (8, 1, 8))]
#[case::anchor_multibyte_scalar("&名前 hello\n", "hello", (8, 1, 4), (13, 1, 9))]
#[case::sequence_second_item("- 日本\n- 語文\n", "語文", (11, 2, 2), (17, 2, 4))]
#[case::doc_marker_inline("--- 中文\n", "中文", (4, 1, 4), (10, 1, 6))]
fn scalar_span_start_and_end(
    #[case] input: &str,
    #[case] find_value: &str,
    #[case] start: (usize, usize, usize),
    #[case] end: (usize, usize, usize),
) {
    let events = collect_events(input);
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == find_value))
        .unwrap_or_else(|| panic!("Scalar({find_value}) not found"));
    assert_pos("start", &span.start, start.0, start.1, start.2);
    assert_pos("end", &span.end, end.0, end.1, end.2);
}

// ---------------------------------------------------------------------------
// Group 2: Mapping with multi-byte key — byte_offset only assertions
// ---------------------------------------------------------------------------

/// `byte_offset` for the mapping value is correct — regression guard.
#[test]
fn multibyte_mapping_value_byte_offset_always_correct() {
    let events = collect_events("日本語: val\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == "val"))
        .expect("Scalar(val) not found");
    assert_eq!(span.start.byte_offset, 11, "byte_offset must be correct");
}

// ---------------------------------------------------------------------------
// Group 3: Trailing comment after multi-byte scalar (lexer.rs Sites 8-9)
// ---------------------------------------------------------------------------

/// Trailing comment after ASCII scalar: all position fields correct.
#[test]
fn trailing_comment_ascii_all_fields_correct() {
    // "hello # remark\n": `#` at byte6/col6
    let events = collect_events("hello # remark\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Comment { .. }))
        .expect("Comment not found");
    assert_pos("start", &span.start, 6, 1, 6);
}

/// `byte_offset` of trailing comment after multi-byte scalar is correct.
#[test]
fn trailing_comment_after_multibyte_scalar_byte_offset_correct() {
    // "日本語 # note\n": 日本語=9B/3C, ` `=1B → `#` at byte10
    let events = collect_events("日本語 # note\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Comment { .. }))
        .expect("Comment not found");
    assert_eq!(span.start.byte_offset, 10, "byte_offset of `#` must be 10");
}

/// Comment position after multi-byte scalar, and the scalar's own span.
///
/// Input: `"日本語 # note\n"` — `#` at byte10/col4; comment text `" note"`.
#[test]
fn trailing_comment_after_multibyte_scalar_column_correct() {
    let events = collect_events("日本語 # note\n");

    // Scalar span is built via Pos::advance — correct.
    let (_, scalar_span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == "日本語"))
        .expect("Scalar(日本語) not found");
    assert_pos("scalar.start", &scalar_span.start, 0, 1, 0);
    assert_pos("scalar.end", &scalar_span.end, 9, 1, 3);

    // Comment span.
    let (_, comment_span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Comment { .. }))
        .expect("Comment not found");
    assert_pos("comment.start", &comment_span.start, 10, 1, 4);
    assert_pos("comment.end", &comment_span.end, 16, 1, 10);
}

// ---------------------------------------------------------------------------
// Group 4: Quoted scalars as mapping values after multi-byte key (lib.rs Site 6)
// ---------------------------------------------------------------------------

/// Single-quoted value after a 3-byte key `日`.
///
/// Input: `"日: 'val'\n"` — `日`=3B/1C; `': ` = 2B; `'` at byte5/col3.
#[test]
fn single_quoted_value_after_multibyte_key_column() {
    let events = collect_events("日: 'val'\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| {
            matches!(ev, Event::Scalar { value, style, .. }
            if value == "val" && *style == ScalarStyle::SingleQuoted)
        })
        .expect("SingleQuoted Scalar(val) not found");
    assert_pos("start", &span.start, 5, 1, 3);
    assert_pos("end", &span.end, 10, 1, 8);
}

/// Double-quoted value after a 3-byte key `日`.
///
/// Input: `"日: \"val\"\n"` — same layout as single-quoted; `"` at byte5/col3.
#[test]
fn double_quoted_value_after_multibyte_key_column() {
    let events = collect_events("日: \"val\"\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| {
            matches!(ev, Event::Scalar { value, style, .. }
            if value == "val" && *style == ScalarStyle::DoubleQuoted)
        })
        .expect("DoubleQuoted Scalar(val) not found");
    assert_pos("start", &span.start, 5, 1, 3);
    assert_pos("end", &span.end, 10, 1, 8);
}

// ---------------------------------------------------------------------------
// Group 5: Block scalars after multi-byte key (lib.rs Site 6 cascade)
// ---------------------------------------------------------------------------

/// `byte_offset` of block literal indicator after multi-byte key is correct.
#[test]
fn block_literal_byte_offset_always_correct() {
    // "日: |\n  text\n": `日`=3B, `: `=2B → `|` at byte5
    let events = collect_events("日: |\n  text\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| {
            matches!(
                ev,
                Event::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                }
            )
        })
        .expect("Literal Scalar not found");
    assert_eq!(span.start.byte_offset, 5, "byte_offset of `|` must be 5");
}

/// Block literal scalar span start `column` after a 3-byte key.
///
/// Input: `"日: |\n  text\n"` — `|` at byte5/col3.
#[test]
fn block_literal_after_multibyte_key_span_start_column() {
    let events = collect_events("日: |\n  text\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| {
            matches!(
                ev,
                Event::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                }
            )
        })
        .expect("Literal Scalar not found");
    assert_pos("start", &span.start, 5, 1, 3);
}

/// Block folded scalar span start `column` after a 3-byte key.
///
/// Input: `"日: >\n  text\n"` — `>` at byte5/col3.
#[test]
fn block_folded_after_multibyte_key_span_start_column() {
    let events = collect_events("日: >\n  text\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| {
            matches!(
                ev,
                Event::Scalar {
                    style: ScalarStyle::Folded(_),
                    ..
                }
            )
        })
        .expect("Folded Scalar not found");
    assert_pos("start", &span.start, 5, 1, 3);
}

// ---------------------------------------------------------------------------
// Group 6: Anchor with multi-byte name (lib.rs Sites 15-17)
// ---------------------------------------------------------------------------

/// `byte_offset` of scalar after multi-byte anchor name is correct.
///
/// Input: `"&名前 hello\n"` — `&`=1B, `名前`=6B/2C, ` `=1B → `hello` at byte8/col4.
#[test]
fn anchor_multibyte_name_scalar_byte_offset_correct() {
    let events = collect_events("&名前 hello\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == "hello"))
        .expect("Scalar(hello) not found");
    assert_eq!(
        span.start.byte_offset, 8,
        "byte_offset after multi-byte anchor name must be 8"
    );
}

// ---------------------------------------------------------------------------
// Group 7: Alias with multi-byte name (lib.rs Site 10 / alias span)
// ---------------------------------------------------------------------------

/// Alias span for `*名前` — positions are correct.
///
/// Input: `"&名前 val\n---\n*名前\n"`:
/// - Line 1 `"&名前 val\n"` = 12 bytes
/// - Line 2 `"---\n"` = 4 bytes
/// - `*名前` starts at byte16, line3, column0
/// - end: byte23/col3 (skip `*`=1B/1C + `名前`=6B/2C)
#[test]
fn alias_span_with_multibyte_name_correct() {
    let events = collect_events("&名前 val\n---\n*名前\n");
    let (_, span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Alias { name } if *name == "名前"))
        .expect("Alias(名前) not found");
    assert_pos("start", &span.start, 16, 3, 0);
    assert_pos("end", &span.end, 23, 3, 3);
}

// ---------------------------------------------------------------------------
// Group 8: Combined — multi-byte key with value and trailing comment
// ---------------------------------------------------------------------------

/// Key, value, and trailing comment positions in one document.
///
/// Input: `"日本語: val # note\n"`:
/// - `日本語`=9B/3C; `:`=byte9/col3; ` `=byte10/col4; `val`=byte11-13/col5-7
/// - ` `=byte14/col8; `#`=byte15/col9; ` note`=byte16-20/col10-14
#[test]
fn multibyte_key_value_and_comment_positions() {
    let events = collect_events("日本語: val # note\n");

    // Key scalar — correct.
    let (_, key_span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == "日本語"))
        .expect("Scalar(日本語) not found");
    assert_pos("key.start", &key_span.start, 0, 1, 0);
    assert_pos("key.end", &key_span.end, 9, 1, 3);

    // Value scalar.
    let (_, val_span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Scalar { value, .. } if value == "val"))
        .expect("Scalar(val) not found");
    assert_pos("val.start", &val_span.start, 11, 1, 5);

    // Comment.
    let (_, comment_span) = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::Comment { text } if *text == " note"))
        .expect("Comment( note) not found");
    assert_pos("comment.start", &comment_span.start, 15, 1, 9);
    assert_pos("comment.end", &comment_span.end, 21, 1, 15);
}
