// SPDX-License-Identifier: MIT

//! Integration tests for YAML §5.1 c-printable / nb-json character-set enforcement.
//!
//! All tests exercise the feature through `parse_events()` (the production entry
//! point) and are grouped as follows:
//!
//! - **Group IV-A**: Plain scalar context — c-printable enforced.
//! - **Group IV-B**: Block scalar context (literal + folded) — c-printable enforced.
//! - **Group IV-C**: Comment body context — c-printable enforced.
//! - **Group IV-D**: Quoted scalar context (single- and double-quoted) — nb-json
//!   enforced (broader: rejects only C0 controls except TAB).

use rstest::rstest;

use super::*;

// -----------------------------------------------------------------------
// Group IV-A: Plain scalar — c-printable enforcement
// -----------------------------------------------------------------------

// IV-A1: C0 control (BEL U+0007) in plain scalar → error.
// Spike test: validates the test harness can detect non-printable errors.
#[test]
fn plain_scalar_c0_bel_produces_error() {
    assert!(has_error("key: val\x07ue\n"));
}

// IV-A2: DEL (U+007F) in plain scalar → error.
#[test]
fn plain_scalar_del_produces_error() {
    assert!(has_error("key: val\x7fue\n"));
}

// IV-A3: C1 control (U+0080, first C1) in plain scalar → error.
// C1 controls in UTF-8 encode as 0xC2 0x80-0x9F.
#[test]
fn plain_scalar_c1_0x80_produces_error() {
    assert!(has_error("key: val\u{0080}ue\n"));
}

// IV-A4: U+FFFE (non-character) in plain scalar → error.
#[test]
fn plain_scalar_fffe_produces_error() {
    assert!(has_error("key: val\u{FFFE}ue\n"));
}

// IV-A5: U+FFFF (non-character) in plain scalar → error.
#[test]
fn plain_scalar_ffff_produces_error() {
    assert!(has_error("key: val\u{FFFF}ue\n"));
}

// IV-A6: TAB (U+0009) in plain scalar is accepted (c-printable).
#[test]
fn plain_scalar_tab_accepted() {
    assert!(!has_error("key: value\n"));
}

// IV-A7: Printable ASCII in plain scalar — accepted.
#[test]
fn plain_scalar_printable_ascii_accepted() {
    assert!(!has_error("key: hello-world_123\n"));
}

// IV-A8: Multibyte BMP character (U+263A WHITE SMILING FACE) in plain scalar — accepted.
#[test]
fn plain_scalar_bmp_codepoint_accepted() {
    assert!(!has_error("key: \u{263A}\n"));
}

// IV-A9: Error message format — must contain "U+XXXX" for the offending codepoint.
#[test]
fn plain_scalar_error_message_contains_codepoint_format() {
    let events: Vec<_> = parse_events("key: val\x07ue\n").collect();
    let err_msg = events
        .iter()
        .find_map(|r| r.as_ref().err().map(|e| e.message.clone()))
        .unwrap_or_else(|| unreachable!("expected an error event"));
    assert!(
        err_msg.contains("U+0007"),
        "error message should contain U+0007, got: {err_msg}"
    );
    assert!(
        err_msg.contains("non-printable"),
        "error message should contain 'non-printable', got: {err_msg}"
    );
}

// IV-A10: NUL (U+0000) in plain scalar → error (existing NUL check preserved).
#[test]
fn plain_scalar_nul_produces_error() {
    assert!(has_error("key: val\0ue\n"));
}

// IV-A11: C0 control in plain scalar continuation line → error.
#[test]
fn plain_scalar_multiline_c0_in_continuation_produces_error() {
    // Plain scalar spans two lines; BEL is in the continuation line.
    let input = "key: first\n  second\x07line\n";
    assert!(has_error(input));
}

// IV-A12: Valid plain scalar with multibyte characters — no error.
#[rstest]
#[case::arabic("مرحبا")]
#[case::cjk("こんにちは")]
#[case::emoji("\u{1F600}")]
fn plain_scalar_valid_multibyte_accepted(#[case] text: &str) {
    let input = format!("key: {text}\n");
    assert!(!has_error(&input));
}

// -----------------------------------------------------------------------
// Group IV-B: Block scalar — c-printable enforcement
// -----------------------------------------------------------------------

// IV-B1: C0 control (BEL U+0007) in literal block scalar content → error.
#[test]
fn literal_block_scalar_c0_bel_produces_error() {
    let input = "key: |\n  val\x07ue\n";
    assert!(has_error(input));
}

// IV-B2: DEL (U+007F) in literal block scalar content → error.
#[test]
fn literal_block_scalar_del_produces_error() {
    let input = "key: |\n  val\x7fue\n";
    assert!(has_error(input));
}

// IV-B3: C1 control (U+0080) in literal block scalar content → error.
#[test]
fn literal_block_scalar_c1_produces_error() {
    // U+0080 is a C1 control (PAD); c-printable excludes C1 controls.
    let input = "key: |\n  val\u{0080}ue\n";
    assert!(has_error(input));
}

// IV-B4: C0 control (BEL) in folded block scalar content → error.
#[test]
fn folded_block_scalar_c0_bel_produces_error() {
    let input = "key: >\n  val\x07ue\n";
    assert!(has_error(input));
}

// IV-B5: DEL in folded block scalar content → error.
#[test]
fn folded_block_scalar_del_produces_error() {
    let input = "key: >\n  val\x7fue\n";
    assert!(has_error(input));
}

// IV-B6: Valid literal block scalar with printable ASCII — no error.
#[test]
fn literal_block_scalar_printable_content_accepted() {
    assert!(!has_error("key: |\n  hello world\n"));
}

// IV-B7: Error message for block scalar identifies context.
#[test]
fn literal_block_scalar_error_message_identifies_context() {
    let events: Vec<_> = parse_events("key: |\n  val\x07ue\n").collect();
    let err_msg = events
        .iter()
        .find_map(|r| r.as_ref().err().map(|e| e.message.clone()))
        .unwrap_or_else(|| unreachable!("expected an error event"));
    assert!(
        err_msg.contains("block scalar"),
        "error should mention 'block scalar', got: {err_msg}"
    );
}

// -----------------------------------------------------------------------
// Group IV-C: Comment body — c-printable enforcement
// -----------------------------------------------------------------------

// IV-C1: C0 control (BEL U+0007) in standalone comment → error.
#[test]
fn standalone_comment_c0_bel_produces_error() {
    let input = "# hello\x07world\n";
    assert!(has_error(input));
}

// IV-C2: DEL (U+007F) in comment body → error.
#[test]
fn standalone_comment_del_produces_error() {
    let input = "# hello\x7fworld\n";
    assert!(has_error(input));
}

// IV-C3: C1 control (U+0080) in comment body → error.
#[test]
fn standalone_comment_c1_produces_error() {
    // U+0080 is a C1 control excluded by c-printable.
    let input = "# hello\u{0080}world\n";
    assert!(has_error(input));
}

// IV-C4: Printable comment body is accepted — no error.
#[test]
fn standalone_comment_printable_accepted() {
    assert!(!has_error("# This is a valid comment\n"));
}

// IV-C5: Error message for comment identifies context.
#[test]
fn comment_error_message_identifies_context() {
    let events: Vec<_> = parse_events("# hello\x07world\n").collect();
    let err_msg = events
        .iter()
        .find_map(|r| r.as_ref().err().map(|e| e.message.clone()))
        .unwrap_or_else(|| unreachable!("expected an error event"));
    assert!(
        err_msg.contains("comment"),
        "error should mention 'comment', got: {err_msg}"
    );
}

// IV-C6: NUL in trailing comment → error (existing check preserved).
#[test]
fn trailing_comment_nul_produces_error() {
    assert!(has_error("key: value # comment\0 here\n"));
}

// -----------------------------------------------------------------------
// Group IV-D: Quoted scalar — nb-json enforcement
// -----------------------------------------------------------------------

// IV-D1: DEL (U+007F) in double-quoted scalar — accepted per nb-json.
// nb-json = x09 | [x20-x10FFFF]; DEL (0x7F) is ≥ 0x20 so it is accepted.
#[test]
fn double_quoted_del_accepted_per_nb_json() {
    assert!(!has_error("key: \"val\x7fue\"\n"));
}

// IV-D2: C1 control (U+0080) in double-quoted scalar — accepted per nb-json.
#[test]
fn double_quoted_c1_accepted_per_nb_json() {
    assert!(!has_error("key: \"val\u{0080}ue\"\n"));
}

// IV-D3: U+FFFE in double-quoted scalar — accepted per nb-json.
#[test]
fn double_quoted_fffe_accepted_per_nb_json() {
    assert!(!has_error("key: \"val\u{FFFE}ue\"\n"));
}

// IV-D4: U+FFFF in double-quoted scalar — accepted per nb-json.
#[test]
fn double_quoted_ffff_accepted_per_nb_json() {
    assert!(!has_error("key: \"val\u{FFFF}ue\"\n"));
}

// IV-D5: C0 control (BEL U+0007) in double-quoted scalar — rejected (excluded by nb-json).
#[test]
fn double_quoted_c0_bel_produces_error() {
    assert!(has_error("key: \"val\x07ue\"\n"));
}

// IV-D6: NUL (U+0000) in double-quoted literal content — rejected per nb-json.
#[test]
fn double_quoted_nul_produces_error() {
    assert!(has_error("key: \"val\0ue\"\n"));
}

// IV-D7: DEL in single-quoted scalar — accepted per nb-json.
#[test]
fn single_quoted_del_accepted_per_nb_json() {
    assert!(!has_error("key: 'val\x7fue'\n"));
}

// IV-D8: C1 control (U+0080) in single-quoted scalar — accepted per nb-json.
#[test]
fn single_quoted_c1_accepted_per_nb_json() {
    assert!(!has_error("key: 'val\u{0080}ue'\n"));
}

// IV-D9: U+FFFE in single-quoted scalar — accepted per nb-json.
#[test]
fn single_quoted_fffe_accepted_per_nb_json() {
    assert!(!has_error("key: 'val\u{FFFE}ue'\n"));
}

// IV-D10: C0 control (BEL) in single-quoted scalar — rejected per nb-json.
#[test]
fn single_quoted_c0_bel_produces_error() {
    assert!(has_error("key: 'val\x07ue'\n"));
}

// IV-D11: Valid printable content in both quoted styles — no error.
#[rstest]
#[case::double_quoted_ascii("key: \"hello world\"\n")]
#[case::single_quoted_ascii("key: 'hello world'\n")]
#[case::double_quoted_with_tab("key: \"col1\tcol2\"\n")]
#[case::single_quoted_with_tab("key: 'col1\tcol2'\n")]
fn quoted_scalar_printable_content_accepted(#[case] input: &str) {
    assert!(!has_error(input));
}

// IV-D12: Error message for double-quoted scalar identifies context.
#[test]
fn double_quoted_error_message_identifies_context() {
    let events: Vec<_> = parse_events("key: \"val\x07ue\"\n").collect();
    let err_msg = events
        .iter()
        .find_map(|r| r.as_ref().err().map(|e| e.message.clone()))
        .unwrap_or_else(|| unreachable!("expected an error event"));
    assert!(
        err_msg.contains("double-quoted scalar"),
        "error should mention 'double-quoted scalar', got: {err_msg}"
    );
}

// IV-D13: Error message for single-quoted scalar identifies context.
#[test]
fn single_quoted_error_message_identifies_context() {
    let events: Vec<_> = parse_events("key: 'val\x07ue'\n").collect();
    let err_msg = events
        .iter()
        .find_map(|r| r.as_ref().err().map(|e| e.message.clone()))
        .unwrap_or_else(|| unreachable!("expected an error event"));
    assert!(
        err_msg.contains("single-quoted scalar"),
        "error should mention 'single-quoted scalar', got: {err_msg}"
    );
}

// IV-D14: Double-quoted escape for BEL (\a) — accepted (named escape produces C0 but
// YAML spec allows named escapes; security engineer accepted this risk).
#[test]
fn double_quoted_named_escape_bel_accepted() {
    assert!(!has_error("key: \"val\\aue\"\n"));
}
