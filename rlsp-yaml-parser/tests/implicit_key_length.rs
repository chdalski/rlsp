// SPDX-License-Identifier: MIT
//
// Tests for the YAML 1.2 §8.2.2 / §7.4.3 limit: implicit mapping keys must
// not exceed 1024 Unicode characters.  Groups A–G cover block context;
// groups H–N cover flow context (both §7.4.3 [154] and [155]).

#![expect(missing_docs, reason = "test code")]

use rlsp_yaml_parser::{load, parse_events};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn has_parse_error(input: &str) -> bool {
    parse_events(input).any(|r| r.is_err())
}

fn parses_clean(input: &str) -> bool {
    parse_events(input).all(|r| r.is_ok())
}

fn first_error_message(input: &str) -> Option<String> {
    parse_events(input)
        .find_map(std::result::Result::err)
        .map(|e| e.message)
}

// ===========================================================================
// Group A: Boundary acceptance — plain ASCII keys
// ===========================================================================

#[test]
fn a1_1024_ascii_plain_key_parses_successfully() {
    let key = "a".repeat(1024);
    let input = format!("{key}: value\n");
    assert!(
        parses_clean(&input),
        "1024-char ASCII key should parse without error"
    );
}

#[test]
fn a2_1025_ascii_plain_key_produces_error() {
    let key = "a".repeat(1025);
    let input = format!("{key}: value\n");
    assert!(
        has_parse_error(&input),
        "1025-char ASCII key should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
    assert!(
        msg.contains("§8.2.2"),
        "error message should cite §8.2.2, got: {msg}"
    );
}

#[test]
fn a3_short_key_parses_successfully() {
    let input = "k: v\n";
    assert!(parses_clean(input), "short key should parse without error");
}

#[test]
fn a4_empty_key_parses_successfully() {
    // Bare `: value` — zero-char key.
    let input = ": value\n";
    assert!(
        parses_clean(input),
        "empty (zero-char) key should parse without error"
    );
}

// ===========================================================================
// Group B: Unicode / multibyte boundary
// ===========================================================================

#[test]
fn b1_1024_two_byte_chars_parse_successfully() {
    // 1024 × 'é' (U+00E9, 2 bytes each) = 2048 bytes but only 1024 chars.
    let key = "é".repeat(1024);
    let input = format!("{key}: value\n");
    assert!(
        parses_clean(&input),
        "1024 two-byte chars (2048 bytes) should parse successfully — limit is chars not bytes"
    );
}

#[test]
fn b2_1025_two_byte_chars_produce_error() {
    // 1025 × 'é' = 1025 chars → over limit.
    let key = "é".repeat(1025);
    let input = format!("{key}: value\n");
    assert!(
        has_parse_error(&input),
        "1025 two-byte chars should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
}

#[test]
fn b3_1024_three_byte_chars_parse_successfully() {
    // 1024 × '中' (U+4E2D, 3 bytes each) = 3072 bytes but only 1024 chars.
    let key = "中".repeat(1024);
    let input = format!("{key}: value\n");
    assert!(
        parses_clean(&input),
        "1024 three-byte chars (3072 bytes) should parse successfully"
    );
}

#[test]
fn b4_1025_four_byte_chars_produce_error() {
    // 1025 × '𝄞' (U+1D11E, 4 bytes each) = 4100 bytes but 1025 chars → over limit.
    let key = "\u{1D11E}".repeat(1025);
    let input = format!("{key}: value\n");
    assert!(
        has_parse_error(&input),
        "1025 four-byte chars should produce a parse error"
    );
}

// ===========================================================================
// Group C: Quoted implicit keys
//
// The check measures trimmed[..colon_offset].chars().count(), which for a
// double-quoted key like `"<content>": value` includes the opening `"`,
// the content, and the closing `"` — so a 1024-content-char quoted key has
// colon_offset at position 1026 (quote + 1024 chars + quote), making
// trimmed[..colon_offset].chars().count() == 1026.
//
// A key whose content is 1022 chars will produce a slice of 1024 chars
// (1022 content + 2 quotes) — right at the limit and accepted.
// A key whose content is 1023 chars produces a slice of 1025 chars — over.
//
// All assertions below are written in terms of the *slice* char count,
// not the content char count.
// ===========================================================================

#[test]
fn c1_double_quoted_key_at_limit_parses_successfully() {
    // Content: 1022 × 'a' → slice = `"` + 1022 + `"` = 1024 chars → at limit, accepted.
    let content = "a".repeat(1022);
    let input = format!("\"{content}\": value\n");
    assert!(
        parses_clean(&input),
        "double-quoted key with 1024-char slice should parse successfully"
    );
}

#[test]
fn c2_double_quoted_key_over_limit_produces_error() {
    // Content: 1023 × 'a' → slice = `"` + 1023 + `"` = 1025 chars → over limit.
    let content = "a".repeat(1023);
    let input = format!("\"{content}\": value\n");
    assert!(
        has_parse_error(&input),
        "double-quoted key with 1025-char slice should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
}

#[test]
fn c3_single_quoted_key_at_limit_parses_successfully() {
    // Content: 1022 × 'a' → slice = `'` + 1022 + `'` = 1024 chars → at limit, accepted.
    let content = "a".repeat(1022);
    let input = format!("'{content}': value\n");
    assert!(
        parses_clean(&input),
        "single-quoted key with 1024-char slice should parse successfully"
    );
}

#[test]
fn c4_single_quoted_key_over_limit_produces_error() {
    // Content: 1023 × 'a' → slice = `'` + 1023 + `'` = 1025 chars → over limit.
    let content = "a".repeat(1023);
    let input = format!("'{content}': value\n");
    assert!(
        has_parse_error(&input),
        "single-quoted key with 1025-char slice should produce a parse error"
    );
}

// ===========================================================================
// Group D: Explicit key exemption
// ===========================================================================

#[test]
fn d1_explicit_key_indicator_with_long_key_parses_successfully() {
    // `? <1025-char key>` — explicit `?` form is not subject to the limit.
    let key = "a".repeat(1025);
    let input = format!("? {key}\n: value\n");
    assert!(
        parses_clean(&input),
        "explicit '?' key with >1024 chars should not be limited"
    );
}

// ===========================================================================
// Group E: Error position and message
// ===========================================================================

#[test]
fn e1_error_position_points_to_colon_indicator() {
    // 1025-char key at column 0; `:` is at column 1025, byte offset 1025.
    let key = "a".repeat(1025);
    let input = format!("{key}: value\n");
    let err = parse_events(&input)
        .find_map(std::result::Result::err)
        .expect("expected a parse error");
    assert_eq!(
        err.pos.byte_offset, 1025,
        "error byte_offset should point to the ':' at byte 1025"
    );
    assert_eq!(
        err.pos.column, 1025,
        "error column should point to the ':' at column 1025"
    );
}

#[test]
fn e2_error_message_contains_expected_substrings() {
    let key = "a".repeat(1025);
    let input = format!("{key}: value\n");
    let msg = first_error_message(&input).expect("expected a parse error");
    assert!(
        msg.contains("1024 Unicode characters"),
        "error message should contain '1024 Unicode characters', got: {msg}"
    );
    assert!(
        msg.contains("§8.2.2"),
        "error message should cite §8.2.2, got: {msg}"
    );
}

// ===========================================================================
// Group F: Integration via load()
// ===========================================================================

#[test]
fn f1_load_with_overlong_key_returns_err() {
    let key = "a".repeat(1025);
    let input = format!("{key}: value\n");
    let result = load(&input);
    assert!(
        result.is_err(),
        "load() should return Err for overlong implicit key"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("1024"),
        "load() error should mention '1024', got: {err_msg}"
    );
}

#[test]
fn f2_load_nested_overlong_key_returns_err_at_correct_position() {
    // Nested mapping — the overlong key is on line 2 (1-indexed).
    let key = "a".repeat(1025);
    let input = format!("outer:\n  {key}: value\n");
    let result = load(&input);
    assert!(
        result.is_err(),
        "load() should return Err for overlong implicit key in nested mapping"
    );
    // Confirm the error is not on line 1 (the outer key is fine).
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("1024"),
        "nested overlong key error should mention '1024', got: {err_str}"
    );
}

// ===========================================================================
// Group G: Interaction with single-line restriction
// ===========================================================================

#[test]
fn g1_multiline_flow_collection_key_fires_multiline_error_not_length_error() {
    // A block mapping whose nested key is a multi-line flow collection.
    // `key:\n  [a\n  b]: value` — the `[a\n  b]` spans two lines, which is
    // illegal as an implicit mapping key (YAML 1.2 §7.4.2).  The multi-line
    // flow collection error must fire, not the new block key length error.
    let input = "key:\n  [a\n  b]: value\n";
    assert!(
        has_parse_error(input),
        "multi-line flow collection as implicit key should produce a parse error"
    );
    let msg = first_error_message(input).expect("expected an error");
    assert!(
        msg.contains("multi-line flow collection"),
        "error should mention 'multi-line flow collection', not the length limit; got: {msg}"
    );
    assert!(
        !msg.contains("1024"),
        "length-limit error must not fire for a short multi-line key; got: {msg}"
    );
}

// ===========================================================================
// Group H: Flow-mapping boundary acceptance — plain ASCII keys
//          `{key: value}` form (YAML 1.2 §7.4.3 [154])
// ===========================================================================

#[test]
fn h1_flow_map_1024_ascii_plain_key_parses_successfully() {
    let key = "a".repeat(1024);
    let input = format!("{{{key}: value}}");
    assert!(
        parses_clean(&input),
        "1024-char ASCII key in flow mapping should parse without error"
    );
}

#[test]
fn h2_flow_map_1025_ascii_plain_key_produces_error() {
    let key = "a".repeat(1025);
    let input = format!("{{{key}: value}}");
    assert!(
        has_parse_error(&input),
        "1025-char ASCII key in flow mapping should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
    assert!(
        msg.contains("§7.4.3"),
        "error message should cite §7.4.3, got: {msg}"
    );
}

#[test]
fn h3_flow_map_short_key_parses_successfully() {
    assert!(
        parses_clean("{k: v}"),
        "short key in flow mapping should parse"
    );
}

#[test]
fn h4_flow_map_empty_key_parses_successfully() {
    assert!(
        parses_clean("{: v}"),
        "empty (zero-char) key in flow mapping should parse"
    );
}

// ===========================================================================
// Group I: Flow-sequence single-pair boundary — plain ASCII keys
//          `[key: value]` form (YAML 1.2 §7.4.3 [154])
// ===========================================================================

#[test]
fn i1_flow_seq_1024_ascii_plain_key_parses_successfully() {
    let key = "a".repeat(1024);
    let input = format!("[{key}: value]");
    assert!(
        parses_clean(&input),
        "1024-char ASCII key in flow sequence should parse without error"
    );
}

#[test]
fn i2_flow_seq_1025_ascii_plain_key_produces_error() {
    let key = "a".repeat(1025);
    let input = format!("[{key}: value]");
    assert!(
        has_parse_error(&input),
        "1025-char ASCII key in flow sequence should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
    assert!(
        msg.contains("§7.4.3"),
        "error message should cite §7.4.3, got: {msg}"
    );
}

#[test]
fn i3_flow_seq_short_key_parses_successfully() {
    assert!(
        parses_clean("[k: v]"),
        "short key in flow sequence should parse"
    );
}

// ===========================================================================
// Group J: Flow context Unicode / multibyte boundary
// ===========================================================================

#[test]
fn j1_flow_map_1024_two_byte_chars_parse_successfully() {
    let key = "é".repeat(1024);
    let input = format!("{{{key}: value}}");
    assert!(
        parses_clean(&input),
        "1024 two-byte chars in flow mapping should parse — limit is chars not bytes"
    );
}

#[test]
fn j2_flow_map_1025_two_byte_chars_produce_error() {
    let key = "é".repeat(1025);
    let input = format!("{{{key}: value}}");
    assert!(
        has_parse_error(&input),
        "1025 two-byte chars in flow mapping should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
}

#[test]
fn j3_flow_seq_1024_three_byte_chars_parse_successfully() {
    let key = "中".repeat(1024);
    let input = format!("[{key}: value]");
    assert!(
        parses_clean(&input),
        "1024 three-byte chars in flow sequence should parse"
    );
}

#[test]
fn j4_flow_seq_1025_four_byte_chars_produce_error() {
    let key = "\u{1D11E}".repeat(1025);
    let input = format!("[{key}: value]");
    assert!(
        has_parse_error(&input),
        "1025 four-byte chars in flow sequence should produce a parse error"
    );
}

// ===========================================================================
// Group K: Quoted (JSON-key) implicit keys in flow context
//          Both `{"key": value}` (double-quoted) and `{'key': value}` (single-quoted)
//          forms (YAML 1.2 §7.4.3 [155]).
//
// Quote-inclusive measurement: `"content"` counts as len(content)+2 chars.
// A 1022-content-char quoted key has a 1024-char slice — at the limit.
// A 1023-content-char quoted key has a 1025-char slice — over the limit.
// ===========================================================================

#[test]
fn k1_flow_map_double_quoted_key_at_limit_parses_successfully() {
    let content = "a".repeat(1022);
    let input = format!("{{\"{content}\": value}}");
    assert!(
        parses_clean(&input),
        "double-quoted key with 1024-char slice in flow mapping should parse"
    );
}

#[test]
fn k2_flow_map_double_quoted_key_over_limit_produces_error() {
    let content = "a".repeat(1023);
    let input = format!("{{\"{content}\": value}}");
    assert!(
        has_parse_error(&input),
        "double-quoted key with 1025-char slice in flow mapping should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
}

#[test]
fn k3_flow_map_single_quoted_key_at_limit_parses_successfully() {
    let content = "a".repeat(1022);
    let input = format!("{{'{content}': value}}");
    assert!(
        parses_clean(&input),
        "single-quoted key with 1024-char slice in flow mapping should parse"
    );
}

#[test]
fn k4_flow_map_single_quoted_key_over_limit_produces_error() {
    let content = "a".repeat(1023);
    let input = format!("{{'{content}': value}}");
    assert!(
        has_parse_error(&input),
        "single-quoted key with 1025-char slice in flow mapping should produce a parse error"
    );
}

#[test]
fn k5_flow_seq_double_quoted_key_at_limit_parses_successfully() {
    let content = "a".repeat(1022);
    let input = format!("[\"{content}\": value]");
    assert!(
        parses_clean(&input),
        "double-quoted key with 1024-char slice in flow sequence should parse"
    );
}

#[test]
fn k6_flow_seq_double_quoted_key_over_limit_produces_error() {
    let content = "a".repeat(1023);
    let input = format!("[\"{content}\": value]");
    assert!(
        has_parse_error(&input),
        "double-quoted key with 1025-char slice in flow sequence should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
}

#[test]
fn k7_flow_seq_single_quoted_key_at_limit_parses_successfully() {
    let content = "a".repeat(1022);
    let input = format!("['{content}': value]");
    assert!(
        parses_clean(&input),
        "single-quoted key with 1024-char slice in flow sequence should parse"
    );
}

#[test]
fn k8_flow_seq_single_quoted_key_over_limit_produces_error() {
    let content = "a".repeat(1023);
    let input = format!("['{content}': value]");
    assert!(
        has_parse_error(&input),
        "single-quoted key with 1025-char slice in flow sequence should produce a parse error"
    );
}

// ===========================================================================
// Group L: Explicit key exemption in flow context
// ===========================================================================

#[test]
fn l1_explicit_key_in_flow_mapping_with_long_key_parses_successfully() {
    // `{? <1025-char key>: value}` — explicit `?` form is not subject to the limit.
    let key = "a".repeat(1025);
    let input = format!("{{? {key}: value}}");
    assert!(
        parses_clean(&input),
        "explicit '?' key in flow mapping with >1024 chars should not be limited"
    );
}

#[test]
fn l2_explicit_key_in_flow_sequence_with_long_key_parses_successfully() {
    // `[? <1025-char key>: value]` — explicit `?` form is not subject to the limit.
    let key = "a".repeat(1025);
    let input = format!("[? {key}: value]");
    assert!(
        parses_clean(&input),
        "explicit '?' key in flow sequence with >1024 chars should not be limited"
    );
}

// ===========================================================================
// Group M: Error position and message in flow context
// ===========================================================================

#[test]
fn m1_flow_map_error_position_points_to_colon_indicator() {
    // `{` at byte 0, 1025-char key at bytes 1–1025, `:` at byte 1026.
    let key = "a".repeat(1025);
    let input = format!("{{{key}: value}}");
    let err = parse_events(&input)
        .find_map(std::result::Result::err)
        .expect("expected a parse error");
    assert_eq!(
        err.pos.byte_offset, 1026,
        "error byte_offset should point to the ':' at byte 1026"
    );
    assert_eq!(
        err.pos.column, 1026,
        "error column should point to the ':' at column 1026"
    );
}

#[test]
fn m2_flow_seq_error_position_points_to_colon_indicator() {
    // `[` at byte 0, 1025-char key at bytes 1–1025, `:` at byte 1026.
    let key = "a".repeat(1025);
    let input = format!("[{key}: value]");
    let err = parse_events(&input)
        .find_map(std::result::Result::err)
        .expect("expected a parse error");
    assert_eq!(
        err.pos.byte_offset, 1026,
        "error byte_offset should point to the ':' at byte 1026"
    );
}

#[test]
fn m3_flow_error_message_contains_expected_substrings() {
    let key = "a".repeat(1025);
    let input = format!("{{{key}: value}}");
    let msg = first_error_message(&input).expect("expected a parse error");
    assert!(
        msg.contains("1024 Unicode characters"),
        "error message should contain '1024 Unicode characters', got: {msg}"
    );
    assert!(
        msg.contains("§7.4.3"),
        "error message should cite §7.4.3, got: {msg}"
    );
}

// ===========================================================================
// Group N: Integration via load() in flow context
// ===========================================================================

#[test]
fn n1_load_flow_map_overlong_key_returns_err() {
    let key = "a".repeat(1025);
    let input = format!("{{{key}: value}}");
    let result = load(&input);
    assert!(
        result.is_err(),
        "load() should return Err for overlong implicit key in flow mapping"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("1024"),
        "load() error should mention '1024', got: {err_msg}"
    );
}

#[test]
fn n2_load_flow_seq_overlong_key_returns_err() {
    let key = "a".repeat(1025);
    let input = format!("[{key}: value]");
    let result = load(&input);
    assert!(
        result.is_err(),
        "load() should return Err for overlong implicit key in flow sequence"
    );
}

// ===========================================================================
// Group H (continued): Interaction with other flow-context restrictions
// ===========================================================================

#[test]
fn h5_flow_seq_single_line_restriction_fires_before_length_check() {
    // Key `aaa` ends on line 1; `:` is on line 2 with no continuation on line 2.
    // The single-line restriction must fire, not the length-limit error.
    let input = "[aaa\n: v]";
    assert!(
        has_parse_error(input),
        "flow-seq key with colon on a different line should produce a parse error"
    );
    let msg = first_error_message(input).expect("expected an error");
    assert!(
        msg.contains("single line"),
        "error should mention 'single line', got: {msg}"
    );
    assert!(
        !msg.contains("1024"),
        "length-limit error must not fire for a short key; got: {msg}"
    );
}

#[test]
fn h6_second_key_in_multi_entry_flow_map_also_checked() {
    // First key is short and valid; second key is overlong — the error must fire.
    let long_key = "a".repeat(1025);
    let input = format!("{{ok: v, {long_key}: v}}");
    assert!(
        has_parse_error(&input),
        "overlong second key in multi-entry flow mapping should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
}

#[test]
fn h7_nested_flow_map_inner_overlong_key_caught() {
    // Outer key is short; inner key is overlong — the inner error must fire.
    let long_key = "a".repeat(1025);
    let input = format!("{{outer: {{{long_key}: v}}}}");
    assert!(
        has_parse_error(&input),
        "overlong key in nested flow mapping should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("1024"),
        "error message should mention '1024', got: {msg}"
    );
}

#[test]
fn h8_flow_seq_overlong_multiline_key_fires_single_line_error_not_length_error() {
    // A 1025-char key ends on line 1; `:` is on line 2.
    // The single-line restriction must fire before the length check so the error
    // message cites the line restriction, not the character limit.
    let key = "a".repeat(1025);
    let input = format!("[{key}\n: v]");
    assert!(
        has_parse_error(&input),
        "overlong flow-seq key with colon on next line should produce a parse error"
    );
    let msg = first_error_message(&input).expect("expected an error");
    assert!(
        msg.contains("single line"),
        "error should cite the single-line restriction, got: {msg}"
    );
    assert!(
        !msg.contains("1024"),
        "length-limit error must not fire when single-line restriction fires first; got: {msg}"
    );
}
