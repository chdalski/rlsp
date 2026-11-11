// SPDX-License-Identifier: MIT

#![expect(
    clippy::expect_used,
    missing_docs,
    reason = "bench setup panics are acceptable"
)]

mod fixtures;

use std::fmt::Write as _;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rlsp_yaml::analysis::selection::selection_ranges;
use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rlsp_yaml::hover::hover_at;
use rlsp_yaml::navigation::references::find_references;
use rlsp_yaml::validation::ValidationSettings;
use rlsp_yaml::validation::validators::{
    validate_custom_tags, validate_duplicate_keys, validate_flow_style, validate_key_ordering,
    validate_unused_anchors,
};
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Document;
use tower_lsp::lsp_types::{Position, Range, Url};

// ──────────────────────────────────────────────────────────────────────────────
// Tier 3 — architectural insight benchmarks
// ──────────────────────────────────────────────────────────────────────────────

/// Each of the 5 validators in isolation on `large()`.
///
/// Identifies which validator dominates total validation time.
fn bench_validators_individual(c: &mut Criterion) {
    let text = fixtures::large();
    // Validators now use rlsp-yaml-parser types.
    let docs: Vec<Document<Span>> = rlsp_yaml_parser::load(&text).unwrap_or_default();
    let mut group = c.benchmark_group("validators_individual");

    group.bench_function("validate_unused_anchors", |b| {
        b.iter(|| validate_unused_anchors(&docs));
    });

    group.bench_function("validate_flow_style", |b| {
        b.iter(|| validate_flow_style(&docs, &ValidationSettings::default()));
    });

    group.bench_function("validate_custom_tags", |b| {
        b.iter(|| validate_custom_tags(&docs, &[]));
    });

    group.bench_function("validate_key_ordering", |b| {
        b.iter(|| validate_key_ordering(&docs));
    });

    group.bench_function("validate_duplicate_keys", |b| {
        b.iter(|| validate_duplicate_keys(&docs, &ValidationSettings::default()));
    });

    group.finish();
}

/// `hover_at` and `find_references` on medium anchor-heavy YAML.
///
/// Measures AST traversal + schema resolution cost.
///
/// `generate_anchor_yaml(100)` produces lines of the form:
/// - Even lines: `anchorN: &anchorN valueN`  (anchor token at col 9)
/// - Odd lines:  `aliasN: *anchorN`           (alias token at col 8)
fn bench_hover_and_references(c: &mut Criterion) {
    let schema = fixtures::generate_schema(20, 2);
    let text = fixtures::generate_anchor_yaml(100);
    // Hover now uses rlsp-yaml-parser types.
    let docs: Vec<Document<Span>> = rlsp_yaml_parser::load(&text).unwrap_or_default();

    let uri: Url = "file:///bench/anchors.yaml"
        .parse()
        .expect("valid URI in benchmark setup");

    // Cursor on the key at line 0 col 0 — hover over a mapping key.
    let hover_pos = Position::new(0, 0);
    // Cursor on the anchor token `&anchor0` at line 0 col 9.
    let ref_pos = Position::new(0, 9);

    let mut group = c.benchmark_group("hover_and_references");

    group.bench_function("hover_at", |b| {
        b.iter(|| hover_at(&docs, hover_pos, Some(&schema)));
    });

    group.bench_function("find_references", |b| {
        b.iter(|| find_references(&docs, &uri, ref_pos, true));
    });

    group.finish();
}

/// `selection_ranges` on nested YAML at varying cursor depths.
///
/// Uses `generate_nested_yaml(20, 3)` — each depth level takes 3 lines.
/// Measures AST traversal cost as selection expands outward from cursor.
fn bench_selection_ranges(c: &mut Criterion) {
    let text = fixtures::generate_nested_yaml(20, 3);
    let docs: Vec<Document<Span>> =
        rlsp_yaml_parser::load(&text).expect("fixture YAML parses without error");

    // Cursor positions at known nesting depths (depth * 3 lines, depth * 2 cols).
    let positions_by_depth: &[(&str, &[Position])] = &[
        ("depth_0", &[Position::new(0, 0)]),
        ("depth_3", &[Position::new(9, 6)]),
        ("depth_8", &[Position::new(24, 16)]),
    ];

    let mut group = c.benchmark_group("selection_ranges");
    for (label, positions) in positions_by_depth {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            positions,
            |b, positions| {
                b.iter(|| selection_ranges(&docs, positions));
            },
        );
    }
    group.finish();
}

/// A shape of document: the name and the text of the entry `i`, appended.
type Shape = (&'static str, fn(&mut String, usize));

/// `code_actions` over the whole range of documents with 1 000 to 8 000 unused
/// anchors: each doubling of the anchors must raise the time by at most 2.5
/// times.  Three shapes, since a cost per alias or per document hides in the
/// first one:
/// - `plain`: only unused anchors;
/// - `with_aliases`: a used anchor with its alias for every eighth unused one
///   (a document holds at most 10 000 anchors);
/// - `documents`: one unused anchor in each of as many documents.
fn bench_code_actions_unused_anchors(c: &mut Criterion) {
    let uri: Url = "file:///bench/unused-anchors.yaml"
        .parse()
        .expect("valid URI in benchmark setup");
    let options = YamlFormatOptions::default();
    let whole = Range::new(Position::new(0, 0), Position::new(u32::MAX, 0));
    let shapes: [Shape; 3] = [
        ("plain", |text, i| {
            writeln!(text, "k{i}: &a{i} v").expect("writing to a String");
        }),
        ("with_aliases", |text, i| {
            writeln!(text, "k{i}: &a{i} v").expect("writing to a String");
            if i % 8 == 0 {
                write!(text, "u{i}: &b{i} v\nr{i}: *b{i}\n").expect("writing to a String");
            }
        }),
        ("documents", |text, i| {
            write!(text, "---\nk: &a{i} v\n").expect("writing to a String");
        }),
    ];

    let mut group = c.benchmark_group("code_actions_unused_anchors");
    for (shape, write_entry) in shapes {
        for n in [1_000, 2_000, 4_000, 8_000] {
            let mut text = String::new();
            for i in 0..n {
                write_entry(&mut text, i);
            }
            let docs: Vec<Document<Span>> =
                rlsp_yaml_parser::load(&text).expect("fixture YAML parses without error");
            let diagnostics = validate_unused_anchors(&docs);
            let offered = |actions: &[tower_lsp::lsp_types::CodeAction]| {
                actions
                    .iter()
                    .filter(|a| a.title == "Delete unused anchor")
                    .count()
            };
            assert_eq!(
                offered(&code_actions(
                    &docs,
                    &text,
                    whole,
                    &diagnostics,
                    &uri,
                    &options
                )),
                n,
                "{shape}/{n}: one action per unused anchor"
            );
            group.bench_with_input(BenchmarkId::new(shape, n), &n, |b, _| {
                // The thousands of actions are freed outside the timed section:
                // freeing them costs time with the size of the answer, which
                // this group does not measure.
                b.iter_with_large_drop(|| {
                    code_actions(&docs, &text, whole, &diagnostics, &uri, &options)
                });
            });
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_validators_individual,
    bench_hover_and_references,
    bench_selection_ranges,
    bench_code_actions_unused_anchors
);
criterion_main!(benches);
