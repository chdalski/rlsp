// SPDX-License-Identifier: MIT

mod fixtures;

use std::collections::HashSet;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rlsp_yaml::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::schema_validation::validate_schema;
use rlsp_yaml::validators::{
    validate_duplicate_keys, validate_flow_style, validate_key_ordering, validate_unused_anchors,
};
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Document;

// ──────────────────────────────────────────────────────────────────────────────
// Tier 1 — hot-path benchmarks
// ──────────────────────────────────────────────────────────────────────────────

/// Simulates the full `parse_and_publish` path: `parse_yaml` + all 5 validators.
///
/// This is the critical path hit on every keystroke.
fn bench_parse_and_validate(c: &mut Criterion) {
    let sizes = [
        ("tiny", fixtures::tiny()),
        ("medium", fixtures::medium()),
        ("large", fixtures::large()),
        ("huge", fixtures::huge()),
    ];

    let allowed_tags: HashSet<String> = HashSet::new();

    let mut group = c.benchmark_group("parse_and_validate");
    for (name, text) in &sizes {
        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, text| {
            b.iter(|| {
                let result = parse_yaml(text);
                let _ = validate_unused_anchors(text);
                let _ = validate_flow_style(text);
                let _ = rlsp_yaml::validators::validate_custom_tags(
                    text,
                    &result.documents,
                    &allowed_tags,
                );
                let _ = validate_key_ordering(text, &result.documents);
                let _ = validate_duplicate_keys(&result.documents);
            });
        });
    }
    group.finish();
}

/// Schema validation only — YAML is parsed outside the loop.
///
/// Measures the cost of `validate_schema()` against a Kubernetes-complexity
/// schema across document sizes.
fn bench_schema_validation(c: &mut Criterion) {
    let schema = fixtures::generate_schema(50, 3);

    let sizes = [
        ("tiny", fixtures::tiny()),
        ("medium", fixtures::medium()),
        ("large", fixtures::large()),
        ("huge", fixtures::huge()),
    ];

    // Parse YAML outside the benchmark loop — measures validation cost only.
    let parsed: Vec<(&str, String, Vec<_>)> = sizes
        .iter()
        .map(|(name, text)| {
            let docs: Vec<Document<Span>> = rlsp_yaml_parser::load(text).unwrap_or_default();
            (*name, text.clone(), docs)
        })
        .collect();

    let mut group = c.benchmark_group("schema_validation");
    for (name, text, docs) in &parsed {
        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, text| {
            b.iter(|| validate_schema(text, docs, &schema, false));
        });
    }
    group.finish();
}

/// Full-document formatter across sizes and a deeply-nested fixture.
///
/// The deeply-nested case tests Wadler-Lindig printer depth sensitivity.
fn bench_formatter(c: &mut Criterion) {
    let options = YamlFormatOptions::default();

    let sizes = [
        ("tiny", fixtures::tiny()),
        ("medium", fixtures::medium()),
        ("large", fixtures::large()),
        ("huge", fixtures::huge()),
        ("deeply_nested", fixtures::deeply_nested()),
    ];

    let mut group = c.benchmark_group("formatter");
    for (name, text) in &sizes {
        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, text| {
            b.iter(|| format_yaml(text, &options));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_and_validate,
    bench_schema_validation,
    bench_formatter
);
criterion_main!(benches);
