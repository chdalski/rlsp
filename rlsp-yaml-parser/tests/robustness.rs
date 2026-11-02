// SPDX-License-Identifier: MIT
//
// Security and robustness stress tests — adversarial and pathological input
// that exercises security limits, panic safety, and completion guarantees.

#![expect(clippy::unwrap_used, missing_docs, reason = "test code")]

use std::fmt::Write;
use std::time::{Duration, Instant};

use rstest::rstest;

use rlsp_yaml_parser::encoding::decode;
use rlsp_yaml_parser::loader::{LoadError, LoaderBuilder};
use rlsp_yaml_parser::node::Node;
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
            Err(LoadError::AliasExpansionLimitExceeded { limit: 500, .. })
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
    // The loader calls `register_anchor` for mappings and sequences only
    // after all their content has been parsed (post-MappingEnd /
    // SequenceEnd). When expanding anchor a's value, *b is encountered
    // before b is registered, producing UndefinedAlias.
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
    // The `CircularAlias` guard inside `expand_node` is defensive — it guards
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
            Err(LoadError::AnchorCountLimitExceeded { limit: 10, .. })
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
            Err(LoadError::NestingDepthLimitExceeded { limit: 5, .. })
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
            Err(LoadError::NestingDepthLimitExceeded { limit: 100, .. })
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

// ===========================================================================
// Group H: A plain scalar on the `---` line
// ===========================================================================

/// Events of `input`, up to a fixed budget so that a parser that stops
/// advancing fails the test instead of hanging it.
fn bounded_events(input: &str) -> Vec<Result<(), ()>> {
    parse_events(input)
        .take(10_000)
        .map(|item| item.map(|_| ()).map_err(|_| ()))
        .collect()
}

#[test]
fn marker_line_scalar_inputs_terminate_within_the_event_budget() {
    for input in [
        "--- x\n%y",
        "--- x\n---",
        "--- x\n...",
        "--- x\n... y",
        "--- x\n--- y",
        "--- x\n---y",
        "--- x",
        "--- x\n",
        "--- x\n\n\n",
        "--- x\n\t",
        "--- x\n  \t y",
        "--- x\n\u{FEFF}",
        "--- x\n\0",
        "--- x # \u{1}",
        "--- x\n  y # \u{7f}",
        "--- #c",
        "--- #c\n  x",
        "---\t#c",
        "--- x\r",
    ] {
        let events = bounded_events(input);
        assert!(events.len() < 10_000, "no end in sight for {input:?}");
        if let Some(position) = events.iter().position(Result::is_err) {
            assert_eq!(
                position + 1,
                events.len(),
                "events after an error: {input:?}"
            );
        }
    }
}

#[test]
fn marker_line_scalar_never_panics_on_marker_alphabet_strings() {
    const ALPHABET: [&str; 14] = [
        "-", " ", "\t", "x", "日", "\r", "\n", "#", ":", "%", ".", "\"", "\0", "\u{FEFF}",
    ];
    // A small deterministic generator keeps the test reproducible.
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..20_000 {
        let length = usize::try_from(next() % 24).unwrap();
        let mut input = String::from("--- ");
        for _ in 0..length {
            let pick = usize::try_from(next() % 14).unwrap();
            input.push_str(ALPHABET[pick]);
        }
        let events = bounded_events(&input);
        assert!(events.len() < 10_000, "no end in sight for {input:?}");
        for item in parse_events(&input).take(10_000).flatten() {
            let (_, span) = item;
            assert!(span.start <= span.end, "inverted span in {input:?}");
            assert!(
                span.end as usize <= input.len(),
                "span past the end in {input:?}"
            );
            assert!(input.is_char_boundary(span.start as usize), "{input:?}");
            assert!(input.is_char_boundary(span.end as usize), "{input:?}");
        }
        let _ = load(&input);
    }
}

#[test]
fn marker_line_scalar_with_many_continuation_lines_is_linear() {
    let input = format!("--- x\n{}", "  y\n".repeat(200_000));
    let start = Instant::now();
    let docs = load(&input).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(docs.len(), 1);
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
}

// A bare dash looks past the comment and blank lines below it to the first
// line that is neither; every run is walked once, so the input stays linear.
#[rstest]
#[case::comments_until_end(format!("-\n{}", "# c\n".repeat(200_000)), 1)]
#[case::comments_before_value(format!("-\n{}  b\n", "# c\n".repeat(200_000)), 1)]
#[case::comments_before_next_item(format!("-\n{}- x\n", "# c\n".repeat(200_000)), 2)]
#[case::blank_lines_before_value(format!("-\n{}  b\n", "\n".repeat(200_000)), 1)]
#[case::space_lines_before_value(format!("-\n{}  b\n", "  \n".repeat(200_000)), 1)]
#[case::one_very_long_blank_line(format!("-\n{}\n  b\n", " \t".repeat(100_000)), 1)]
#[case::many_dashes_with_a_comment_each("-\n  # c\n".repeat(100_000), 100_000)]
#[case::many_dashes_with_blank_lines_each("-\n\n".repeat(200_000), 200_000)]
#[case::many_bare_dashes("-\n".repeat(300_000), 300_000)]
#[case::tag_line_comments_before_value(format!("- !!str\n{}  b\n", "# c\n".repeat(200_000)), 1)]
fn bare_dash_with_long_comment_or_blank_runs_is_linear(
    #[case] input: String,
    #[case] items: usize,
) {
    let start = Instant::now();
    let docs = load(&input).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(docs.len(), 1);
    assert!(
        matches!(&docs[0].root, Node::Sequence { items: found, .. } if found.len() == items),
        "expected {items} items"
    );
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
}

#[test]
fn many_marker_line_scalar_documents_do_not_hang() {
    let input = "--- x\n".repeat(200_000);
    let start = Instant::now();
    let docs = load(&input).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(docs.len(), 200_000);
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
}

#[test]
fn long_marker_line_scalar_does_not_hang() {
    let input = format!("--- {}\n", "a ".repeat(2_500_000));
    let start = Instant::now();
    let docs = load(&input).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(docs.len(), 1);
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
}

#[test]
fn many_blank_lines_after_a_marker_line_scalar_do_not_hang() {
    let input = format!("--- x\n{}y\n", "\n".repeat(100_000));
    let start = Instant::now();
    let docs = load(&input).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(docs.len(), 1);
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
}

// ===========================================================================
// Group I: A `:` line after a key with an empty value
// ===========================================================================

#[test]
fn many_colon_lines_after_empty_values_are_linear() {
    let input = "a:\n: c\n".repeat(50_000);
    let start = Instant::now();
    let events = bounded_events(&input);
    let elapsed = start.elapsed();
    assert!(events.iter().all(Result::is_ok));
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
    assert!(load(&input).is_ok());
}

/// The scalar values of `input`; the test fails on the first parse error.
fn scalar_values_of(input: &str) -> Vec<String> {
    parse_events(input)
        .filter_map(|item| {
            let (event, _) = item.unwrap();
            if let rlsp_yaml_parser::Event::Scalar { value, .. } = event {
                Some(value.into_owned())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn many_flow_plain_continuation_lines_are_linear() {
    let input = format!("[a{}]\n", "\n\n b".repeat(50_000));
    let start = Instant::now();
    let values = scalar_values_of(&input);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(30), "took {elapsed:?}");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].len(), 1 + 50_000 * 2);
}

#[test]
fn many_blank_lines_in_a_flow_plain_scalar_fold_to_as_many_line_feeds() {
    // The first line break ends the line of `a`; the rest are blank lines.
    let input = format!("[a{} b]\n", "\n".repeat(200_001));
    let values = scalar_values_of(&input);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].len(), 1 + 200_000 + 1);
    assert!(values[0].starts_with("a\n\n"));
    assert!(values[0].ends_with("\n\nb"));
}

#[test]
fn many_colon_lines_after_a_flow_key_terminate_within_the_event_budget() {
    let input = format!("[x]:\n{}", ":\n".repeat(2_000));
    let events = bounded_events(&input);
    assert!(events.len() < 10_000, "no end in sight");
    assert!(events.iter().all(Result::is_ok));
}

#[test]
fn nested_colon_lines_beyond_the_depth_limit_end_in_one_error() {
    let mut input = String::new();
    for level in 0..600 {
        writeln!(input, "{}:", " ".repeat(level)).unwrap();
    }
    let events = bounded_events(&input);
    assert!(events.len() < 10_000, "no end in sight");
    let position = events
        .iter()
        .position(Result::is_err)
        .expect("depth limit must be reported");
    assert_eq!(position + 1, events.len(), "events after the error");
}

#[test]
fn colon_lines_at_the_end_of_input_leave_no_collection_open() {
    for input in [
        "a:\n:",
        "a:\n:\n",
        "[x]:\n:",
        "*a :\n:\n",
        "a:\n---\n: c",
        "a:\n...\n: c\n",
    ] {
        let events = bounded_events(input);
        assert!(events.iter().all(Result::is_ok), "{input:?}");
        assert!(load(input).is_ok(), "{input:?}");
    }
}
