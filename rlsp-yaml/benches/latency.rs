// SPDX-License-Identifier: MIT

#![allow(clippy::expect_used)]

mod fixtures;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rlsp_yaml::completion::complete_at;
use rlsp_yaml::document_store::DocumentStore;
use rlsp_yaml::semantic_tokens::semantic_tokens;
use saphyr::{LoadableYamlNode, YamlOwned};
use tower_lsp::lsp_types::{Position, Url};

// ──────────────────────────────────────────────────────────────────────────────
// Tier 2 — user-perceivable latency benchmarks
// ──────────────────────────────────────────────────────────────────────────────

/// Completion at varying cursor depths with schema.
///
/// Measures schema path resolution cost as cursor depth increases.
/// Uses `generate_nested_yaml(20, 3)` — each nesting level adds 3 lines
/// (2 leaf properties + 1 nested mapping key).
fn bench_completion(c: &mut Criterion) {
    let schema = fixtures::generate_schema(50, 3);
    let text = fixtures::generate_nested_yaml(20, 3);
    // Completion still uses saphyr types during migration.
    let docs: Vec<YamlOwned> = YamlOwned::load_from_str(&text).unwrap_or_default();

    // Cursor positions at known nesting depths.
    // Each depth level occupies 3 lines: 2 leaf props + 1 nested key.
    // Line = depth * 3, col = depth * 2 (indentation of leaf key).
    let positions = [
        ("root", Position::new(0, 0)),
        ("depth_3", Position::new(9, 6)),
        ("depth_8", Position::new(24, 16)),
    ];

    let mut group = c.benchmark_group("completion");
    for (label, pos) in positions {
        group.bench_with_input(BenchmarkId::from_parameter(label), &pos, |b, pos| {
            b.iter(|| complete_at(&text, Some(&docs), *pos, Some(&schema)));
        });
    }
    group.finish();
}

/// Semantic token scan across document sizes.
///
/// Pure text scan — measures O(n) scaling with document size.
fn bench_semantic_tokens(c: &mut Criterion) {
    let sizes = [
        ("tiny", fixtures::tiny()),
        ("medium", fixtures::medium()),
        ("large", fixtures::large()),
        ("huge", fixtures::huge()),
    ];

    let mut group = c.benchmark_group("semantic_tokens");
    for (name, text) in &sizes {
        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, text| {
            b.iter(|| semantic_tokens(text));
        });
    }
    group.finish();
}

/// `DocumentStore::change()` — measures the parse cost.
///
/// Each call re-parses via both rlsp-yaml-parser and saphyr (legacy).
/// The document is pre-opened so only the change path is measured.
fn bench_document_store_change(c: &mut Criterion) {
    let sizes = [
        ("tiny", fixtures::tiny()),
        ("medium", fixtures::medium()),
        ("large", fixtures::large()),
        ("huge", fixtures::huge()),
    ];

    let uri: Url = "file:///bench/doc.yaml"
        .parse()
        .expect("valid URI in benchmark setup");

    let mut group = c.benchmark_group("document_store_change");
    for (name, text) in &sizes {
        // Pre-open the document so `change` only measures the update path.
        let mut store = DocumentStore::new();
        store.open(uri.clone(), text.clone());

        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, text| {
            b.iter(|| store.change(&uri, text.clone()));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_completion,
    bench_semantic_tokens,
    bench_document_store_change
);
criterion_main!(benches);
