**Repository:** root
**Status:** InProgress
**Created:** 2026-03-30

## Goal

Add a comprehensive Criterion benchmark suite for rlsp-yaml
to establish performance baselines across all hot paths.
The codebase currently has zero benchmarks — every measurement
is infinitely more valuable than having none. Benchmarks
cover the keystroke hot path (parse + validate), schema
validation, formatting, completion, semantic tokens, document
store, individual validators, hover/references, and selection
ranges. Programmatic fixtures match real-world schema
complexity (Kubernetes-scale: ~50 properties, nested objects,
allOf/anyOf branching, required fields, enum values, pattern
constraints).

## Context

- No existing benchmarks, no `benches/` directory, no
  Criterion dependency
- Every keystroke triggers: full saphyr re-parse → 5
  sequential validators → schema validation (if attached)
  → semantic tokens. That's ~6 full document traversals per
  keystroke.
- Schema validation (`schema_validation.rs`, 5339 LOC) is
  the largest module and runs O(n × schema_complexity ×
  branch_count) on every change when a schema is attached
- `document_store.change()` does two independent full parses
  (YamlOwned + MarkedYamlOwned) on every change
- All validators are O(n) and run sequentially
- Key public APIs to benchmark:
  - `parser::parse_yaml(text)` → `ParseResult`
  - `validators::validate_unused_anchors(text)` → `Vec<Diagnostic>`
  - `validators::validate_flow_style(text)` → `Vec<Diagnostic>`
  - `validators::validate_custom_tags(text, docs, allowed)` → `Vec<Diagnostic>`
  - `validators::validate_key_ordering(text, docs)` → `Vec<Diagnostic>`
  - `validators::validate_duplicate_keys(text)` → `Vec<Diagnostic>`
  - `schema_validation::validate_schema(text, docs, schema, fmt)` → `Vec<Diagnostic>`
  - `formatter::format_yaml(text, options)` → `String`
  - `completion::complete_at(text, docs, pos, schema)` → `Vec<CompletionItem>`
  - `semantic_tokens::semantic_tokens(text)` → `Vec<SemanticToken>`
  - `hover::hover_at(text, docs, pos, schema)` → `Option<Hover>`
  - `references::goto_definition(text, uri, pos)` → `Option<Location>`
  - `references::find_references(text, uri, pos, incl)` → `Vec<Location>`
  - `selection::selection_ranges(text, marked, positions)` → `Vec<SelectionRange>`
- Criterion is the recommended benchmarking crate per
  project rules (`lang-rust.md`)
- Benchmarks are local-only (no CI regression check yet)

## Steps

- [x] Clarify requirements with user
- [x] Set up benchmark infrastructure and fixture generators (78a03cd)
- [x] Implement Tier 1 benchmarks (hot path) (79e74e6)
- [x] Implement Tier 2 benchmarks (user-perceivable latency) (279b156)
- [ ] Implement Tier 3 benchmarks (architectural insight)

## Tasks

### Task 1: Benchmark infrastructure and fixture generators

Set up the Criterion harness and build reusable YAML/schema
fixture generators that all benchmark files will share.

- [ ] Add `criterion` to `[dev-dependencies]` in
      `rlsp-yaml/Cargo.toml`
- [ ] Add `[[bench]]` entries for three benchmark binaries:
      `hot_path`, `latency`, `insight`
- [ ] Create `rlsp-yaml/benches/fixtures/mod.rs` with:
  - `generate_yaml(lines: usize)` — flat key-value YAML
  - `generate_nested_yaml(depth: usize, width: usize)` —
    nested mapping YAML
  - `generate_anchor_yaml(anchor_count: usize)` — YAML with
    anchors and aliases
  - `generate_schema(properties: usize, depth: usize)` —
    JsonSchema with Kubernetes-like complexity: nested object
    properties, allOf branches, required fields, enum values,
    pattern constraints, description strings
  - Size presets: `tiny()` (20 lines), `medium()` (500),
    `large()` (2000), `huge()` (10000)
- [ ] Create minimal `rlsp-yaml/benches/hot_path.rs` with a
      single smoke-test benchmark (`parse_yaml` on `tiny()`)
      to verify the harness compiles and runs
- [ ] Verify `cargo bench --bench hot_path` runs successfully

### Task 2: Tier 1 — hot-path benchmarks

Benchmarks for the three paths that dominate keystroke latency.

- [ ] `bench_parse_and_validate` — simulates the full
      `parse_and_publish` path: parse_yaml + all 5 validators,
      benchmarked across tiny/medium/large/huge fixtures.
      Uses `criterion::BenchmarkGroup` with `BenchmarkId` for
      each size.
- [ ] `bench_schema_validation` — `validate_schema()` with the
      programmatic Kubernetes-complexity schema, across
      tiny/medium/large/huge YAML fixtures. Parses YAML
      outside the benchmark loop (measures validation only).
- [ ] `bench_formatter` — `format_yaml()` across
      tiny/medium/large/huge fixtures. Also benchmark with
      `deeply_nested` to test Wadler-Lindig depth sensitivity.

### Task 3: Tier 2 — user-perceivable latency benchmarks

Benchmarks for operations triggered by user actions (completion,
semantic highlighting, document store update).

- [ ] `bench_completion` — `complete_at()` with schema at
      varying cursor depths (root, 3 levels, 8 levels deep)
      using nested YAML + matching schema. Measures schema
      path resolution cost.
- [ ] `bench_semantic_tokens` — `semantic_tokens()` across
      tiny/medium/large/huge. Pure text scan — measures O(n)
      scaling.
- [ ] `bench_document_store_change` — `DocumentStore::change()`
      measuring the double-parse cost (YamlOwned + MarkedYamlOwned)
      across sizes.

### Task 4: Tier 3 — architectural insight benchmarks

Benchmarks to isolate individual component costs and identify
optimization targets.

- [ ] `bench_validators_individual` — benchmark each of the 5
      validators independently on `large()` to identify which
      dominates total validation time:
      `validate_unused_anchors`, `validate_flow_style`,
      `validate_custom_tags`, `validate_key_ordering`,
      `validate_duplicate_keys`
- [ ] `bench_hover_and_references` — `hover_at()` and
      `find_references()` on medium YAML with anchors, cursor
      at known positions. Measures AST traversal + schema
      resolution cost.
- [ ] `bench_selection_ranges` — `selection_ranges()` with
      MarkedYamlOwned on nested YAML at varying cursor depths.

## Decisions

- **Criterion over built-in `#[bench]`** — Criterion provides
  statistical analysis, warmup, outlier detection, and HTML
  reports. The built-in bench is nightly-only and minimal.
- **Programmatic fixtures over embedded JSON** — avoids
  checking in large schema files that go stale. Schema
  generators build Kubernetes-complexity structs (50+
  properties, 3-level nesting, allOf branches, enums,
  patterns, required fields) directly in Rust.
- **Three benchmark binaries** — `hot_path`, `latency`,
  `insight` map to the three tiers. Separate binaries allow
  running just the tier you care about without recompiling
  everything.
- **No CI regression check** — starting with local
  measurement only. CI benchmarks require baseline storage
  strategy and can be added as a follow-up.
- **Shared fixtures module** — `benches/fixtures/mod.rs` is
  imported by all three binaries via `mod fixtures;`. Keeps
  fixture generation DRY.
