// SPDX-License-Identifier: MIT

#![allow(clippy::expect_used)]

mod fixtures;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rlsp_yaml::hover::hover_at;
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::references::find_references;
use rlsp_yaml::selection::selection_ranges;
use rlsp_yaml::validators::{
    validate_custom_tags, validate_duplicate_keys, validate_flow_style, validate_key_ordering,
    validate_unused_anchors,
};
use saphyr::{LoadableYamlNode, MarkedYamlOwned};
use tower_lsp::lsp_types::{Position, Url};

// ──────────────────────────────────────────────────────────────────────────────
// Tier 3 — architectural insight benchmarks
// ──────────────────────────────────────────────────────────────────────────────

/// Each of the 5 validators in isolation on `large()`.
///
/// Identifies which validator dominates total validation time.
fn bench_validators_individual(c: &mut Criterion) {
    let text = fixtures::large();
    let parse_result = parse_yaml(&text);
    let docs = parse_result.documents;
    let allowed_tags = std::collections::HashSet::new();

    let mut group = c.benchmark_group("validators_individual");

    group.bench_function("validate_unused_anchors", |b| {
        b.iter(|| validate_unused_anchors(&text));
    });

    group.bench_function("validate_flow_style", |b| {
        b.iter(|| validate_flow_style(&text));
    });

    group.bench_function("validate_custom_tags", |b| {
        b.iter(|| validate_custom_tags(&text, &docs, &allowed_tags));
    });

    group.bench_function("validate_key_ordering", |b| {
        b.iter(|| validate_key_ordering(&text, &docs));
    });

    group.bench_function("validate_duplicate_keys", |b| {
        b.iter(|| validate_duplicate_keys(&text));
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
    let parse_result = parse_yaml(&text);
    let docs = parse_result.documents;

    let uri: Url = "file:///bench/anchors.yaml"
        .parse()
        .expect("valid URI in benchmark setup");

    // Cursor on the key at line 0 col 0 — hover over a mapping key.
    let hover_pos = Position::new(0, 0);
    // Cursor on the anchor token `&anchor0` at line 0 col 9.
    let ref_pos = Position::new(0, 9);

    let mut group = c.benchmark_group("hover_and_references");

    group.bench_function("hover_at", |b| {
        b.iter(|| hover_at(&text, Some(&docs), hover_pos, Some(&schema)));
    });

    group.bench_function("find_references", |b| {
        b.iter(|| find_references(&text, &uri, ref_pos, true));
    });

    group.finish();
}

/// `selection_ranges` on nested YAML at varying cursor depths.
///
/// Uses `generate_nested_yaml(20, 3)` — each depth level takes 3 lines.
/// Measures AST traversal cost as selection expands outward from cursor.
fn bench_selection_ranges(c: &mut Criterion) {
    let text = fixtures::generate_nested_yaml(20, 3);
    let marked_docs =
        MarkedYamlOwned::load_from_str(&text).expect("fixture YAML parses without error");

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
                b.iter(|| selection_ranges(&text, Some(&marked_docs), positions));
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_validators_individual,
    bench_hover_and_references,
    bench_selection_ranges
);
criterion_main!(benches);
