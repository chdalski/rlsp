// SPDX-License-Identifier: MIT
//
// Security and robustness stress tests — adversarial and pathological input
// that exercises security limits, panic safety, and completion guarantees.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn
)]

use std::fmt::Write;
use std::time::{Duration, Instant};

use rlsp_yaml_parser::encoding::decode;
use rlsp_yaml_parser::loader::{LoadError, LoaderBuilder};
use rlsp_yaml_parser::{load, parse_events};

// ===========================================================================
// Group A: Security limits — alias expansion
// ===========================================================================

#[test]
fn alias_bomb_triggers_expansion_limit() {
    // "Billion laughs" pattern with 3 levels of 10x fan-out.
    // Theoretical expansion: 10 * 10 * 10 = 1000 nodes.
    // Limit set to 500 so it triggers before full expansion.
    let input = "\
a: &a x
b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a, *a]
c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b, *b]
d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c, *c]
";
    let result = LoaderBuilder::new()
        .resolved()
        .max_expanded_nodes(500)
        .build()
        .load(input);
    assert!(
        matches!(
            result,
            Err(LoadError::AliasExpansionLimitExceeded { limit: 500 })
        ),
        "expected AliasExpansionLimitExceeded, got: {result:?}"
    );
}

#[test]
fn alias_bomb_lossless_mode_no_expansion() {
    // Same input as the alias bomb, but lossless mode never expands aliases.
    let input = "\
a: &a x
b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a, *a]
c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b, *b]
d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c, *c]
";
    let result = load(input);
    assert!(
        result.is_ok(),
        "lossless mode should not trigger expansion limit: {result:?}"
    );
}

#[test]
fn circular_alias_cross_reference_returns_undefined() {
    // Depth-2 cross-reference: anchor a contains *b, anchor b contains *a.
    // The loader registers mapping/sequence anchors AFTER parsing their
    // content (loader.rs:399). When expanding anchor a's value, *b is
    // encountered before b is registered, producing UndefinedAlias.
    // CircularAlias requires both anchors to be registered before expansion
    // begins, which sequential event processing prevents.
    let input = "\
a: &a
  ref: *b
b: &b
  ref: *a
";
    let result = LoaderBuilder::new().resolved().build().load(input);
    assert!(
        matches!(&result, Err(LoadError::UndefinedAlias { .. })),
        "expected UndefinedAlias (forward reference), got: {result:?}"
    );
}

#[test]
fn circular_alias_self_reference_returns_error() {
    // Self-referencing anchor: &a mapping contains *a in its value.
    // Anchor is registered post-content, so *a finds no registered anchor.
    let input = "a: &a\n  b: *a\n";
    let result = LoaderBuilder::new().resolved().build().load(input);
    assert!(
        result.is_err(),
        "expected error for self-referencing alias, got: {result:?}"
    );
}

#[test]
fn circular_alias_via_scalar_redefine_returns_error() {
    // Define &a as a scalar first, then &b as a sequence containing *a and *b.
    // *a resolves to the scalar (no cycle). *b is encountered while b's
    // content is being parsed — b is not yet registered, producing UndefinedAlias.
    // The CircularAlias code path (loader.rs:528) is defensive — it guards
    // against future loader changes that might enable pre-registration. The
    // expansion limit (AliasExpansionLimitExceeded) is the primary defense.
    let input = "\
- &a scalar_value
- &b
  - *a
  - *b
";
    let result = LoaderBuilder::new().resolved().build().load(input);
    assert!(
        result.is_err(),
        "expected error for forward/self alias reference, got: {result:?}"
    );
}

// ===========================================================================
// Group B: Security limits — anchor count
// ===========================================================================

#[test]
fn anchor_count_limit_triggers_at_configured_threshold() {
    let mut input = String::new();
    for i in 0..11 {
        writeln!(input, "- &a{i} x").unwrap();
    }
    let result = LoaderBuilder::new().max_anchors(10).build().load(&input);
    assert!(
        matches!(
            result,
            Err(LoadError::AnchorCountLimitExceeded { limit: 10 })
        ),
        "expected AnchorCountLimitExceeded, got: {result:?}"
    );
}

#[test]
fn default_anchor_limit_allows_large_anchor_count() {
    let mut input = String::new();
    for i in 0..1000 {
        writeln!(input, "- &a{i} x").unwrap();
    }
    let result = load(&input);
    assert!(
        result.is_ok(),
        "1000 anchors should be well below default 10,000 limit: {result:?}"
    );
}

// ===========================================================================
// Group C: Security limits — nesting depth
// ===========================================================================

#[test]
fn nesting_depth_limit_triggers_at_configured_threshold() {
    // 6-level deep mapping with max_nesting_depth(5).
    let input = "a:\n  b:\n    c:\n      d:\n        e:\n          f: leaf\n";
    let result = LoaderBuilder::new()
        .max_nesting_depth(5)
        .build()
        .load(input);
    assert!(
        matches!(
            result,
            Err(LoadError::NestingDepthLimitExceeded { limit: 5 })
        ),
        "expected NestingDepthLimitExceeded, got: {result:?}"
    );
}

fn build_nested_mapping(depth: usize) -> String {
    // Build a depth-level nested mapping iteratively.
    // Each level adds "kN:\n" with increasing indentation, ending with "leaf\n".
    let mut input = String::new();
    for i in 0..depth {
        let indent = "  ".repeat(i);
        writeln!(input, "{indent}k{i}:").unwrap();
    }
    let indent = "  ".repeat(depth);
    writeln!(input, "{indent}leaf").unwrap();
    input
}

#[test]
fn nesting_depth_within_limit_succeeds() {
    // The check is `depth > limit`, so depth == limit should succeed.
    // Use a smaller limit to keep the test fast and avoid stack issues.
    let result = LoaderBuilder::new()
        .max_nesting_depth(100)
        .build()
        .load(&build_nested_mapping(100));
    assert!(
        result.is_ok(),
        "depth at limit (100) should succeed: {result:?}"
    );
}

#[test]
fn nesting_depth_one_over_limit_is_rejected() {
    // One over the configured limit.
    let result = LoaderBuilder::new()
        .max_nesting_depth(100)
        .build()
        .load(&build_nested_mapping(101));
    assert!(
        matches!(
            result,
            Err(LoadError::NestingDepthLimitExceeded { limit: 100 })
        ),
        "expected NestingDepthLimitExceeded, got: {result:?}"
    );
}

// ===========================================================================
// Group D: Large inputs — no panic, reasonable completion
// ===========================================================================

#[test]
fn very_long_scalar_does_not_panic() {
    // 1MB scalar — reduced from 10MB to avoid debug-build slowness.
    let input = "x".repeat(1024 * 1024);
    let start = Instant::now();
    let _result = load(&input);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "took {elapsed:?}, expected < 30s"
    );
}

#[test]
fn very_long_line_does_not_panic() {
    let input = "key: ".to_string() + &"x".repeat(100_000) + "\n";
    let start = Instant::now();
    let _result = load(&input);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "took {elapsed:?}, expected < 30s"
    );
}

#[test]
fn many_documents_does_not_hang() {
    let mut input = String::new();
    for i in 0..10_000 {
        writeln!(input, "---\n{i}").unwrap();
    }
    let start = Instant::now();
    let result = load(&input);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "took {elapsed:?}, expected < 30s"
    );
    let docs = result.unwrap();
    assert_eq!(docs.len(), 10_000);
}

#[test]
fn many_mapping_entries_does_not_hang() {
    // 10,000 entries — reduced from 100,000 to avoid debug-build slowness.
    let mut input = String::new();
    for i in 0..10_000 {
        writeln!(input, "k{i}: v{i}").unwrap();
    }
    let start = Instant::now();
    let result = load(&input);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "took {elapsed:?}, expected < 30s"
    );
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

#[test]
fn pathological_deeply_repeated_scalar_completes_quickly() {
    // 50,000 plain scalars on one line — tests tokenizer linear scan.
    let mut input = String::new();
    for _ in 0..50_000 {
        input.push_str("x ");
    }
    input.push('\n');
    let start = Instant::now();
    let _count = parse_events(&input).count();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "took {elapsed:?}, expected < 30s"
    );
}

// ===========================================================================
// Group E: Boundary inputs — empty and whitespace
// ===========================================================================

#[test]
fn empty_input_succeeds_with_no_documents() {
    assert_eq!(load("").unwrap().len(), 0);
}

#[test]
fn whitespace_only_input_produces_no_error() {
    let result = load("   \n\t\n  \n");
    assert!(
        result.is_ok(),
        "whitespace-only input should not error: {result:?}"
    );
}

#[test]
fn comment_only_input_produces_no_error() {
    let result = load("# just a comment\n# another comment\n");
    assert!(
        result.is_ok(),
        "comment-only input should not error: {result:?}"
    );
}

#[test]
fn single_newline_input_produces_no_error() {
    let result = load("\n");
    assert!(
        result.is_ok(),
        "single newline should not error: {result:?}"
    );
}

// ===========================================================================
// Group F: Byte-level invalid input — no panic
// ===========================================================================

#[test]
fn binary_garbage_does_not_panic() {
    // Deterministic pseudo-random bytes via simple LCG.
    let mut v = Vec::with_capacity(1000);
    let mut x: u8 = 42;
    for _ in 0..1000 {
        x = x.wrapping_mul(6).wrapping_add(1);
        v.push(x);
    }
    // The test passing without panic is the assertion.
    let _result = decode(&v);
}

#[test]
fn all_byte_values_zero_to_255_do_not_panic() {
    let bytes: Vec<u8> = (0u8..=255).collect();
    // The test passing without panic is the assertion.
    let _result = decode(&bytes);
}

#[test]
fn all_byte_values_fed_to_parse_events_do_not_panic() {
    // ASCII range (0x00–0x7F) is valid UTF-8; feed through parse_events.
    let s: String = String::from_utf8_lossy(&(0u8..=127).collect::<Vec<u8>>()).into_owned();
    // Consume the full iterator — the test passing without panic is the assertion.
    let _events: Vec<_> = parse_events(&s).collect();
}

// ===========================================================================
// Group G: Maximum indentation depth
// ===========================================================================

#[test]
fn maximum_indentation_on_valid_yaml_parses_correctly() {
    let input = " ".repeat(255) + "value\n";
    let events: Vec<_> = parse_events(&input).collect();
    assert!(
        events.iter().any(Result::is_ok),
        "expected at least one Ok event"
    );
}

#[test]
fn extreme_indentation_variation_does_not_panic() {
    // Alternate between deep and shallow indentation.
    let mut input = String::new();
    for i in 0..50 {
        writeln!(input, "key{i}:").unwrap();
        let indent = " ".repeat(100);
        writeln!(input, "{indent}value{i}").unwrap();
    }
    let start = Instant::now();
    let _result = load(&input);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "took {elapsed:?}, expected < 30s"
    );
}
