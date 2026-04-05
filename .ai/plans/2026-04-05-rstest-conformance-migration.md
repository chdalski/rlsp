**Repository:** root
**Status:** InProgress
**Created:** 2026-04-05

## Goal

Migrate `rlsp-yaml-parser/tests/conformance.rs` from a
single monolithic test function to rstest parameterized
tests using `#[files]` for per-file test generation and
`#[timeout]` for per-test timeout protection. This gives
per-file pass/fail visibility in test output and prevents
infinite loops from hanging CI.

## Context

- The conformance test is in `rlsp-yaml-parser` (the
  spec-faithful YAML 1.2 parser crate), NOT `rlsp-yaml`
  (the language server crate).
- The test loads 351 `.yaml` files from
  `tests/yaml-test-suite/src/`, iterates all cases in a
  single `#[test]` function, and asserts zero failures.
- All 402 cases currently pass — there are no known
  failures, so no `#[exclude]` list is needed.
- Test logic: for `fail: true` cases, verify `parse_events`
  produces at least one `Err`; for valid cases, verify
  `parse_events` produces no `Err` items.
- rstest `#[files("glob")]` generates one independent test
  per matched file at compile time.
- rstest `#[timeout(Duration)]` fails a test if it exceeds
  the specified duration.
- Glob resolution happens at compile time relative to
  `CARGO_MANIFEST_DIR`. A `build.rs` is needed to trigger
  recompilation when test files change.
- User decisions: one test per file, hard fail (already the
  case), 5-second timeout.

## Steps

- [x] Clarify requirements with user
- [x] Add rstest dev-dependency and build.rs
- [x] Rewrite conformance.rs using rstest
- [x] Verify all tests pass

## Tasks

### Task 1: Add rstest dependency and build.rs — 74c40b9

Add `rstest` as a dev-dependency to
`rlsp-yaml-parser/Cargo.toml`. Create
`rlsp-yaml-parser/build.rs` to trigger recompilation when
the test data directory changes.

- [x] Add `rstest = "0.26"` to `[dev-dependencies]` in
      `rlsp-yaml-parser/Cargo.toml`
- [x] Create `rlsp-yaml-parser/build.rs` with
      `cargo::rerun-if-changed=tests/yaml-test-suite/src`
- [x] Verify `cargo check -p rlsp-yaml-parser --tests`
      compiles

### Task 2: Rewrite conformance.rs with rstest — 74c40b9

Replace the monolithic test function with an rstest
parameterized test. Preserve the existing helper functions
(`visual_to_raw`, `load_cases_from_file`, `ConformanceCase`
struct, `has_parse_error`, `parses_clean`).

- [x] Add `#[rstest]` test function with
      `#[files("tests/yaml-test-suite/src/*.yaml")]`
- [x] Add `#[timeout(Duration::from_secs(5))]`
- [x] Exclude the `.commit` file via `#[exclude]` or glob
- [x] Test body: load cases from the `PathBuf`, iterate
      cases within the file, assert on failures (both
      fail-expected and valid-parse categories)
- [x] Remove the old `yaml_test_suite_conformance` function
      and summary reporting logic
- [x] Run `cargo test -p rlsp-yaml-parser` — all tests pass
- [x] Run `cargo clippy -p rlsp-yaml-parser --all-targets`
      — zero warnings

## Decisions

- **Target crate: rlsp-yaml-parser** — the spec-faithful
  parser crate, not the language server crate.
- **One test per file** — rstest `#[files]` generates one
  test per `.yaml` file. Cases within a file are iterated
  internally.
- **Hard fail (already the case)** — the existing test
  already asserts zero failures. All cases currently pass,
  so no exclude list is needed.
- **5-second timeout** — guards against infinite loops.
- **build.rs for recompilation** — rstest resolves globs at
  compile time. Without `rerun-if-changed`, adding or
  removing test files would not trigger recompilation.
