// SPDX-License-Identifier: MIT
//
// End-to-end tests for the 1 MiB quoted-scalar length cap.
// Covers double-quoted (borrow path) and single-quoted (borrow and owned paths)
// via both `parse_events()` and `load()`.

#![expect(clippy::unwrap_used, missing_docs, reason = "test code")]

use rlsp_yaml_parser::{MAX_SCALAR_LEN, load, parse_events};

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
// Group A — double-quoted, borrow path, end-to-end
// ===========================================================================

#[test]
fn a1_dq_borrow_path_over_limit_parse_events_returns_error() {
    // `key: "` + 1_048_577 × 'a' + `"` — no escape, borrow path only.
    let scalar = "a".repeat(MAX_SCALAR_LEN + 1);
    let input = format!("key: \"{scalar}\"");
    assert!(
        has_parse_error(&input),
        "double-quoted borrow path over 1 MiB should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("maximum allowed length"),
        "error message should contain 'maximum allowed length', got: {msg}"
    );
}

#[test]
fn a2_dq_borrow_path_at_limit_parse_events_succeeds() {
    // Exactly 1 MiB — must succeed.
    let scalar = "a".repeat(MAX_SCALAR_LEN);
    let input = format!("key: \"{scalar}\"");
    assert!(
        parses_clean(&input),
        "double-quoted borrow path at exactly 1 MiB should parse without error"
    );
}

#[test]
fn a3_dq_borrow_path_over_limit_load_returns_err() {
    let scalar = "a".repeat(MAX_SCALAR_LEN + 1);
    let input = format!("key: \"{scalar}\"");
    assert!(
        load(&input).is_err(),
        "load() should return Err for double-quoted borrow path over 1 MiB"
    );
}

// ===========================================================================
// Group B — single-quoted, single-line borrow path, end-to-end
// ===========================================================================

#[test]
fn b1_sq_single_line_borrow_path_over_limit_parse_events_returns_error() {
    let scalar = "a".repeat(MAX_SCALAR_LEN + 1);
    let input = format!("key: '{scalar}'");
    assert!(
        has_parse_error(&input),
        "single-quoted single-line borrow path over 1 MiB should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("maximum allowed length"),
        "error message should contain 'maximum allowed length', got: {msg}"
    );
}

#[test]
fn b2_sq_single_line_borrow_path_at_limit_parse_events_succeeds() {
    let scalar = "a".repeat(MAX_SCALAR_LEN);
    let input = format!("key: '{scalar}'");
    assert!(
        parses_clean(&input),
        "single-quoted single-line borrow path at exactly 1 MiB should parse without error"
    );
}

#[test]
fn b3_sq_single_line_borrow_path_over_limit_load_returns_err() {
    let scalar = "a".repeat(MAX_SCALAR_LEN + 1);
    let input = format!("key: '{scalar}'");
    assert!(
        load(&input).is_err(),
        "load() should return Err for single-quoted single-line borrow path over 1 MiB"
    );
}

// ===========================================================================
// Group C — single-quoted, multi-line owned path, end-to-end
// ===========================================================================

#[test]
fn c1_sq_multiline_owned_path_over_limit_parse_events_returns_error() {
    // Two lines of 600_000 'a' chars each; fold space between makes total 1_200_001.
    let line = "a".repeat(600_000);
    let input = format!("key: '{line}\n{line}'");
    assert!(
        has_parse_error(&input),
        "single-quoted multi-line owned path over 1 MiB should produce a parse error"
    );
    let msg = first_error_message(&input).unwrap();
    assert!(
        msg.contains("maximum allowed length"),
        "error message should contain 'maximum allowed length', got: {msg}"
    );
}

#[test]
fn c2_sq_multiline_owned_path_over_limit_load_returns_err() {
    let line = "a".repeat(600_000);
    let input = format!("key: '{line}\n{line}'");
    assert!(
        load(&input).is_err(),
        "load() should return Err for single-quoted multi-line owned path over 1 MiB"
    );
}

// ===========================================================================
// Group D — constant centralisation smoke test
// ===========================================================================

#[test]
fn d1_max_scalar_len_constant_is_one_mib() {
    assert_eq!(
        MAX_SCALAR_LEN, 1_048_576,
        "MAX_SCALAR_LEN must be exactly 1 MiB (1_048_576 bytes)"
    );
}
