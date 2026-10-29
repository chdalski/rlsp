// SPDX-License-Identifier: MIT

#![expect(missing_docs, reason = "bench code")]

mod fixtures;

use std::collections::BTreeMap;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::schema::{
    FilePatterns, match_schema_by_filename, match_schemastore, parse_schemastore_catalog,
};
use rlsp_yaml::schema_validation::validate_schema;
use rlsp_yaml::server::YamlVersion;
use rlsp_yaml::validation::ValidationSettings;
use rlsp_yaml::validation::validators::{
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

    let mut group = c.benchmark_group("parse_and_validate");
    for (name, text) in &sizes {
        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, text| {
            b.iter(|| {
                let result = parse_yaml(text);
                let _ = validate_unused_anchors(&result.documents);
                let _ = validate_flow_style(&result.documents, &ValidationSettings::default());
                let _ =
                    rlsp_yaml::validation::validators::validate_custom_tags(&result.documents, &[]);
                let _ = validate_key_ordering(&result.documents);
                let _ = validate_duplicate_keys(&result.documents, &ValidationSettings::default());
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
        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, _text| {
            b.iter(|| validate_schema(docs, &schema, false, YamlVersion::V1_2).diagnostics);
        });
    }
    group.finish();
}

/// Schema validation against a wide `anyOf` / `oneOf`.
///
/// Each of the 100 branches requires a field the document lacks, so validation
/// probes every branch. The `exhausted` case adds `unevaluatedProperties` under
/// nested compositions that outgrow the step budget, measuring the cost of
/// stopping at it.
fn bench_composition_validation(c: &mut Criterion) {
    let branches: Vec<serde_json::Value> = (0..100)
        .map(|i| serde_json::json!({ "required": [format!("field_{i}")] }))
        .collect();
    let nested = (0..4).fold(
        serde_json::json!({ "required": ["missing"] }),
        |inner, _| {
            serde_json::json!({
                "anyOf": vec![inner; 8],
                "unevaluatedProperties": false
            })
        },
    );
    let cases = [
        (
            "any_of_100_branches",
            serde_json::json!({ "anyOf": branches }),
        ),
        (
            "one_of_100_branches",
            serde_json::json!({ "oneOf": branches }),
        ),
        ("budget_exhausted", nested),
    ];
    let docs: Vec<Document<Span>> =
        rlsp_yaml_parser::load("name: demo\nreplicas: 3\n").unwrap_or_default();

    let mut group = c.benchmark_group("composition_validation");
    for (name, raw) in &cases {
        let Ok(schema) = rlsp_yaml::schema::parse_schema(raw) else {
            continue;
        };
        group.bench_with_input(BenchmarkId::from_parameter(name), &schema, |b, schema| {
            b.iter(|| validate_schema(&docs, schema, false, YamlVersion::V1_2).diagnostics);
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

/// Schema association: matching a document path against the `SchemaStore`
/// catalog and the workspace `schemas` setting.
///
/// This is the cost paid on every validation of a document that no workspace
/// `schemas` entry matches. The IDs stay fixed so results compare across
/// changes to the matcher.
#[expect(
    clippy::expect_used,
    reason = "bench setup; a malformed fixture must stop the run"
)]
fn bench_schema_glob_matching(c: &mut Criterion) {
    let catalog_json = fixtures::schemastore_catalog_json();
    assert_eq!(
        catalog_json,
        fixtures::schemastore_catalog_json(),
        "the catalog generator must be deterministic"
    );
    let catalog = parse_schemastore_catalog(&catalog_json).expect("the catalog parses");

    let (late_path, late_url) = fixtures::schemastore_late_match();
    let no_match_path = fixtures::schemastore_no_match_path();
    let deep_path = fixtures::schemastore_deep_path();
    let yml_no_match_path = fixtures::schemastore_yml_no_match_path();
    assert_eq!(deep_path.len(), 200);

    // Each path must measure what its ID says; a generator change that breaks
    // one of these would otherwise go unnoticed.
    assert_eq!(match_schemastore(&late_path, &catalog), Some(late_url));
    assert_eq!(match_schemastore(&no_match_path, &catalog), None);
    assert_eq!(match_schemastore(&deep_path, &catalog), None);
    assert_eq!(match_schemastore(&yml_no_match_path, &catalog), None);
    assert_eq!(
        match_schemastore(&fixtures::schemastore_json_only_path(), &catalog),
        None,
        "JSON-family patterns must be dropped when the catalog loads"
    );

    let schemas: BTreeMap<String, FilePatterns> = fixtures::workspace_schemas()
        .into_iter()
        .map(|(url, patterns)| {
            let patterns = FilePatterns::new(&patterns).expect("the workspace patterns are valid");
            (url, patterns)
        })
        .collect();
    let (workspace_path, workspace_url) = fixtures::workspace_match();
    let workspace_no_match_path = fixtures::workspace_no_match_path();
    assert_eq!(
        match_schema_by_filename(&workspace_path, &schemas),
        Some(workspace_url)
    );
    assert_eq!(
        match_schema_by_filename(&workspace_no_match_path, &schemas),
        None
    );

    let mut group = c.benchmark_group("schema_glob_matching");
    group.bench_function("schemastore_load", |b| {
        b.iter(|| parse_schemastore_catalog(black_box(&catalog_json)));
    });
    for (name, path) in [
        ("schemastore_no_match", &no_match_path),
        ("schemastore_late_match", &late_path),
        ("schemastore_deep_path_200_bytes", &deep_path),
        ("schemastore_yml_no_match", &yml_no_match_path),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| match_schemastore(black_box(path), black_box(&catalog)));
        });
    }
    for (name, path) in [
        ("workspace_match", &workspace_path),
        ("workspace_no_match", &workspace_no_match_path),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| match_schema_by_filename(black_box(path), black_box(&schemas)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_and_validate,
    bench_schema_validation,
    bench_composition_validation,
    bench_formatter,
    bench_schema_glob_matching
);
criterion_main!(benches);
