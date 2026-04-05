**Repository:** root
**Status:** Completed (2026-04-05)
**Created:** 2026-04-05

## Goal

Remove saphyr from `rlsp-yaml-parser`'s dev-dependencies by
replacing it with a simple hand-written parser for the test
metadata format. Then fix any conformance regressions that
surface from the change (saphyr may have been silently
corrupting test data). Finally, write the crate README.

## Context

- saphyr is used only in `tests/conformance.rs` to parse
  the yaml-test-suite metadata files (`.yaml` files
  containing test case sequences)
- saphyr scores 89.6% valid / 70.2% invalid on the YAML
  test matrix — it may silently misparse metadata, dropping
  test cases or mangling YAML content
- The test metadata format is simple and regular: a YAML
  sequence of mappings with string keys (`name`, `yaml`,
  `fail`, `skip`, `tags`, `from`, `tree`, `json`, `dump`)
  and block scalar values. Field inheritance across entries
  (except `fail`).
- Current conformance: 351/351 (100%)
- Current unit tests: 922
- `rstest` is used for per-file parameterization
- `rlsp-yaml-parser/README.md` does not exist — needed
  for crates.io publishing
- Key files:
  - `rlsp-yaml-parser/tests/conformance.rs` — the test
    file using saphyr
  - `rlsp-yaml-parser/Cargo.toml` — saphyr in
    `[dev-dependencies]`
  - `rlsp-yaml-parser/tests/yaml-test-suite/src/` — 351
    test files

## Steps

- [x] Write a simple line-based test metadata parser to
      replace saphyr
- [x] Replace saphyr usage in conformance.rs with the new
      parser
- [x] Remove saphyr from dev-dependencies
- [x] Verify conformance results match (351/351) or fix
      any regressions from corrected test data
- [x] Write rlsp-yaml-parser/README.md

## Tasks

### Task 1: Replace saphyr with hand-written metadata parser

Write a simple parser for the yaml-test-suite metadata
format and replace saphyr in `conformance.rs`.

The metadata format is:
- Document starts with `---`
- Each entry starts with `- ` at indent 0
- Fields are `  key: value` at indent 2
- Block scalars use `  key: |` followed by indented lines
- String values can be inline: `  key: value text`
- Boolean values: `  fail: true`
- Field inheritance: fields carry from one entry to the
  next except `fail` which resets
- Entries with a `skip` field are omitted

**What to implement:**
- [x] A `parse_test_metadata(content: &str) -> Vec<ConformanceCase>` function
- [x] Handles `- ` entry boundaries
- [x] Parses `name`, `yaml`, `fail`, `skip` fields (ignore
      `tags`, `from`, `tree`, `json`, `dump` — not used)
- [x] Handles block scalar values (`yaml: |` with indented
      continuation lines)
- [x] Handles inline string values (`name: Some Name`)
- [x] Handles field inheritance (fields persist across
      entries except `fail`)
- [x] Applies `visual_to_raw` conversion to `yaml` field
- [x] Replace `load_cases_from_file` in conformance.rs to
      use the new parser instead of saphyr
- [x] Remove `saphyr` from `[dev-dependencies]` in
      Cargo.toml
- [x] Remove `use saphyr::*` imports from conformance.rs
- [x] Verify: 351/351 conformance, 922 unit tests, clippy
      clean, fmt clean
- [x] If conformance count changes (more or fewer tests
      discovered, or different pass/fail results), document
      what changed and fix any new failures

**Commit:** `979ed81`

**Acceptance criteria:** saphyr completely removed from
`rlsp-yaml-parser`. Same or better conformance results.
Zero unit test regressions.

**Files:**
- `rlsp-yaml-parser/tests/conformance.rs` — rewrite
  `load_cases_from_file`
- `rlsp-yaml-parser/Cargo.toml` — remove saphyr

### Task 2: Write crate README

Write `rlsp-yaml-parser/README.md` for crates.io.

- [x] Crate description and purpose
- [x] Key features: spec-faithful, 100% conformance,
      first-class comments, lossless spans, alias
      preservation, security controls
- [x] Quick usage example (parse events, load AST)
- [x] API overview (tokenize, parse_events, load, emit,
      schema resolution)
- [x] Conformance status (351/351 YAML test suite)
- [x] Benchmark summary (vs libfyaml baseline)
- [x] License (MIT)
- [x] Link to rlsp project

**Acceptance criteria:** README exists, is accurate, and
follows the convention from root CLAUDE.md ("crate README
is self-contained for users").

**Files:**
- `rlsp-yaml-parser/README.md` — new
- `rlsp-yaml-parser/src/loader.rs` — stale comment fix

**Commit:** `7a32bac`

## Decisions

- **Hand-written metadata parser, not our own crate.** We
  can't use `rlsp-yaml-parser` to parse its own test
  metadata (circular dependency). The metadata format is
  simple enough for a ~100 line parser — no need for a
  general YAML parser.
- **Parse only the fields we need.** The test files have
  many fields (`tree`, `json`, `dump`, `tags`, `from`)
  that conformance.rs doesn't use. Parse only `name`,
  `yaml`, `fail`, and `skip`.
