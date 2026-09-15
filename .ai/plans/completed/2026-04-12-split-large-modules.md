**Repository:** root
**Status:** Completed (2026-04-12)
**Created:** 2026-04-12

## Goal

Split three oversized files into module directories to
improve navigability and reduce per-file cognitive load.
Pure mechanical refactoring — no logic changes, no new
features, no behavioral changes. All existing tests must
pass identically after each task.

| File | Lines | Crate |
|------|-------|-------|
| `rlsp-yaml-parser/tests/smoke.rs` | 10,133 | rlsp-yaml-parser |
| `rlsp-yaml/src/schema_validation.rs` | 5,840 | rlsp-yaml |
| `rlsp-yaml/src/schema.rs` | 3,733 | rlsp-yaml |

## Context

### smoke.rs (rlsp-yaml-parser)

Integration test file with 18 nested `mod` blocks and 463
test functions. Two shared helpers at file scope
(`parse_to_vec`, `event_variants`). Six modules duplicate
an `evs()` helper; five duplicate `has_error()`; two
duplicate `scalar_values()` and `count()`.

Rust convention allows `mod.rs` inside `tests/` (the
`<module>.rs` preference applies to `src/`, not `tests/`).
An existing `tests/conformance.rs` coexists without
conflict.

Module inventory:

| Module | Lines | Local helpers |
|--------|-------|---------------|
| `stream` | 165 | — |
| `documents` | 836 | — |
| `scalars` | 136 | `plain()` |
| `quoted_scalars` | 146 | — |
| `conformance` | 360 | — |
| `block_scalars` | 257 | `literal()` |
| `folded_scalars` | 267 | `folded()` |
| `sequences` | 877 | — |
| `mappings` | 706 | — |
| `nested_collections` | 916 | `find_span()`, `scalar_value()` |
| `flow_collections` | 893 | `scalar_values()`, `evs()`, `count()` |
| `nested_flow_block_mixing` | 516 | `evs()`, `scalar_values()`, `count()`, `has_error()` |
| `anchors_and_aliases` | 1120 | `evs()`, `has_error()` |
| `tags` | 813 | `evs()`, `has_error()` |
| `comments` | 869 | — |
| `directives` | 834 | `evs()`, `has_error()` |
| `scalar_dispatch` | 152 | `first_scalar()`, `has_parse_error()` |
| `probe_dispatch` | 85 | `evs()`, `has_error()` |

### schema_validation.rs (rlsp-yaml)

5,840 lines: ~2,008 production code + ~3,832 inline tests.

Production code breakdown:

| Section | Lines | Description |
|---------|-------|-------------|
| Helpers & constants | 1–61 | `entries_contains_key`, `node_key_str`, `get_regex`, constants, thread-local regex cache |
| Ctx struct | 174–194 | Validation context |
| `validate_schema` (entry) | 206–227 | Public entry point |
| `build_key_index` | 235–266 | Pre-builds key→Range index |
| Core validation | 276–955 | `validate_node` and its recursive callees: `validate_unevaluated_*`, `validate_sequence`, `validate_array_constraints`, `validate_contains`, `validate_scalar_constraints`, `validate_string_constraints`, `validate_format`, `validate_content`, `validate_content_schema`, `validate_mapping_constraints` |
| **Format validators** | **957–1387** | **23 pure `is_valid_*` functions (~430 lines) — no dependency on Ctx or JsonSchema traversal** |
| Numeric constraints | 1388–1493 | `validate_numeric_constraints` |
| Mapping validation | 1495–1707 | `validate_mapping`, `validate_dependencies`, `validate_pattern_properties` |
| Composition | 1709–1825 | `validate_composition` (allOf/anyOf/oneOf/not/if-then-else) |
| Type helpers | 1827–1913 | `yaml_type_name`, `type_matches`, `yaml_to_json` |
| Diagnostic helpers | 1915–2003 | `make_diagnostic`, `node_range`, `format_path` |
| Tests | 2009–5840 | 26 test groups, ~231 test cases |

The core validation functions form a tight recursive call
graph (validate_node → validate_mapping → validate_node,
etc.) and cannot be split. The format validators are pure
functions with only internal cross-calls (e.g.,
`is_valid_date_time` calls `is_valid_date` +
`is_valid_time`) — they are the clean extraction target.

The only caller of format validators from outside their
group is `validate_format` (line 789), which dispatches to
them by format name. After extraction, `validate_format`
calls them via `formats::is_valid_*`.

### schema.rs (rlsp-yaml)

3,733 lines: ~1,545 production code + ~2,188 inline tests.

Production code breakdown:

| Section | Lines | Description |
|---------|-------|-------------|
| Types & constants | 1–179 | `SchemaError`, `SchemaType`, `AdditionalProperties`, `JsonSchema` (45 fields), `SchemaAssociation`, `SchemaStoreEntry`, `SchemaStoreCatalog`, `SchemaCache`, constants |
| SchemaCache impl | 184–244 | Cache operations (`new`, `get`, `insert`, `get_or_fetch`) |
| URL validation | 250–349 | `validate_and_normalize_url`, `is_ssrf_blocked_host` |
| Fetching | 354–461 | `build_agent`, `sanitize_content_type`, `fetch_schema_raw` |
| SchemaStore | 467–565 | `fetch_schemastore_catalog`, `parse_schemastore_catalog`, `match_schemastore` |
| Depth check | 574–592 | `check_json_depth` |
| **Parsing core** | **600–1313** | **`ParseContext`, `parse_schema`, `parse_schema_with_root`, 11 field parsers, `resolve_ref`, `find_anchor_in_value` (~714 lines)** |
| **Association** | **1327–1539** | **`extract_schema_url`, `extract_custom_tags`, `extract_yaml_version`, `detect_kubernetes_resource`, `kubernetes_schema_url`, `match_schema_by_filename`, `glob_matches` (~213 lines)** |
| Tests | 1545–3733 | 15 test groups, ~120 test cases |

Dependency analysis reveals two tightly coupled clusters
and one independent cluster:

**Cluster 1 — Parsing + fetching (circular dependency):**
`fetch_schema_raw` calls `parse_schema`/
`parse_schema_with_root`. `resolve_ref` (inside parsing)
calls `fetch_schema_raw` for remote `$ref`s. These must
stay together.

**Cluster 2 — Types + cache + URL validation:** Used by
Cluster 1 and by external callers. Small and cohesive.

**Cluster 3 — Association (independent):** Modeline
extractors, Kubernetes detection, and file-pattern matching
have zero dependencies on parsing, fetching, or the cache.
They operate on raw text and YAML documents. This is the
clean extraction target.

External consumers import from `crate::schema::*`:
- `server.rs` uses types, cache, fetching, association,
  and schemastore functions
- `schema_validation.rs` uses `AdditionalProperties`,
  `JsonSchema`, `SchemaType`
- `hover.rs`, `completion.rs`, `code_lens.rs` use
  `JsonSchema` (and `SchemaType` in some)

Converting `schema.rs` → `schema/mod.rs` preserves the
`crate::schema::*` import path. Re-exports in `mod.rs`
maintain the public API.

## Steps

- [x] Split `smoke.rs` into `tests/smoke/` directory (8809c48)
- [x] Split `schema_validation.rs` into
      `src/schema_validation/` directory (dcdd239)
- [x] Split `schema.rs` into `src/schema/` directory (f45b206)
- [x] Verify full test suite and clippy pass after each

## Tasks

### Task 1: Split smoke.rs into test module directory

Create `rlsp-yaml-parser/tests/smoke/` directory. Write
`mod.rs` with:

- The SPDX header and `#![deny(clippy::panic)]`
- The full `use rlsp_yaml_parser::{...}` import block
- The four promoted helpers: `evs`, `has_error`,
  `scalar_values`, `count`
- The two existing shared helpers: `parse_to_vec`,
  `event_variants`
- All 18 `mod` declarations

Extract each module's body into its own `.rs` file. Each
file starts with `use super::*;` (plus `use rstest::rstest;`
where needed). Remove the duplicated `evs`, `has_error`,
`scalar_values`, and `count` from submodule files — they
now come from `mod.rs` via `use super::*`. Keep
module-specific helpers in their respective files.

Delete the original `smoke.rs`.

- [x] `mod.rs` with shared imports and 18 mod declarations
- [x] 18 submodule `.rs` files
- [x] Duplicate helpers removed from submodules
- [x] Original `smoke.rs` deleted
- [x] `cargo test --test smoke` passes (547 tests)
- [x] `cargo clippy --all-targets` — zero warnings
- [x] `cargo fmt --check` — clean

### Task 2: Extract format validators from schema_validation.rs

Convert `rlsp-yaml/src/schema_validation.rs` into
`rlsp-yaml/src/schema_validation/` directory:

**`schema_validation/mod.rs`** — everything except the
format validators: constants, `Ctx`, `validate_schema`
entry point, core recursive validation, sequence/mapping/
scalar/numeric/composition validators, type helpers,
diagnostic helpers, and all inline tests that exercise the
core validators.

Add `mod formats;` declaration and update `validate_format`
(line 789) to call `formats::is_valid_*` instead of the
bare function names.

**`schema_validation/formats.rs`** — the 23 `is_valid_*`
functions and their internal helpers (`days_in_month`,
`is_leap_year`, `is_valid_duration_designators`,
`is_json_pointer_tokens_valid`). These are pure functions
with:
- No dependency on `Ctx`, `JsonSchema`, or any validation
  infrastructure
- Only internal cross-calls (e.g., `is_valid_date_time`
  calls `is_valid_date` + `is_valid_time`)
- External crate dependencies: `regex::RegexBuilder`,
  `idna`, `iri_string`

Move the format-specific tests from the `#[cfg(test)]`
block into a `#[cfg(test)] mod tests` block inside
`formats.rs`. The format test section starts at the
"Format validation" group header (~line 4731) and runs
through the format-related assertions.

- [x] `schema_validation/mod.rs` with `mod formats;`
- [x] `schema_validation/formats.rs` with 23 validators
- [x] `validate_format` updated to call `formats::*`
- [x] Format-related tests moved to `formats.rs`
- [x] Original `schema_validation.rs` deleted
- [x] `cargo test -p rlsp-yaml` passes
- [x] `cargo clippy --all-targets` — zero warnings
- [x] `cargo fmt --check` — clean

### Task 3: Extract association functions from schema.rs

Convert `rlsp-yaml/src/schema.rs` into
`rlsp-yaml/src/schema/` directory:

**`schema/mod.rs`** — types, constants, `SchemaCache` impl,
URL validation, fetching, SchemaStore, depth check, parsing
core, and all inline tests for those sections. Add
`mod association;` and re-export its public functions so
`crate::schema::extract_schema_url` etc. continue to work
without changing any external import paths.

**`schema/association.rs`** — the independent association
cluster:
- `extract_schema_url` (modeline `$schema=`)
- `extract_custom_tags` (modeline `$tags=`)
- `extract_yaml_version` (modeline `$yamlVersion=`)
- `detect_kubernetes_resource` (API version/kind detection)
- `kubernetes_schema_url` (URL construction)
- `match_schema_by_filename` (glob matching)
- `glob_matches` + `glob_matches_inner`

These functions have zero dependencies on the parsing,
fetching, or caching infrastructure. Their only imports:
- `rlsp_yaml_parser::{Document, Node, Span}` (Kubernetes
  detection)
- `tower_lsp::lsp_types::Url` (URL normalization in tests)

Move the corresponding test groups into a `#[cfg(test)]
mod tests` block inside `association.rs`:
- "extract_schema_url" tests
- "extract_custom_tags" tests
- "extract_yaml_version" tests
- "match_schema_by_filename" tests
- "detect_kubernetes_resource + kubernetes_schema_url" tests

- [x] `schema/mod.rs` with `mod association;` + re-exports
- [x] `schema/association.rs` with 7 functions
- [x] Association tests moved to `association.rs`
- [x] External import paths unchanged (`crate::schema::*`)
- [x] Original `schema.rs` deleted
- [x] `cargo test -p rlsp-yaml` passes
- [x] `cargo clippy --all-targets` — zero warnings
- [x] `cargo fmt --check` — clean

## Decisions

- **One task per file.** Each split is independently
  committable and reviewable. Task order:
  smoke.rs → schema_validation.rs → schema.rs (no
  dependencies between them; this order goes from largest
  to smallest, building confidence).

- **smoke.rs: single task for all 18 modules.** Mechanical
  extraction with no design decisions per module. One
  commit for the entire split is the right granularity.

- **smoke.rs: promote 4 duplicate helpers to mod.rs.**
  `evs`, `has_error`, `scalar_values`, `count` are
  duplicated identically across multiple modules.
  Module-specific helpers stay in their respective files.

- **smoke.rs: keep `anchors_and_aliases` as one file.**
  At 1,120 lines it is the largest module, but the content
  is cohesive — anchors and aliases are tested together
  throughout.

- **schema_validation.rs: extract only format validators.**
  The core validation functions form a tight recursive call
  graph and cannot be split without introducing awkward
  cross-module calls. The format validators are the one
  group with zero coupling to the validation context — 23
  pure functions, ~430 lines. Type helpers and diagnostic
  helpers are small (~90 lines each) and called heavily
  from the core — extracting them would add module
  boundaries for negligible size reduction.

- **schema.rs: extract only association functions.** The
  parsing and fetching clusters have a circular dependency
  (`fetch_schema_raw` ↔ `resolve_ref`) and must stay
  together. The association functions are completely
  independent — they don't touch parsing, fetching, or
  the cache. SchemaStore functions (`fetch_schemastore_
  catalog`, `match_schemastore`) depend on `fetch_schema_
  raw` and stay in `mod.rs`.

- **Re-export extracted public functions from mod.rs.**
  Both `schema` and `schema_validation` are `pub mod` in
  `lib.rs`. External callers use paths like
  `crate::schema::extract_schema_url`. Re-exporting from
  `mod.rs` (`pub use association::*;`) preserves these
  paths — no changes needed in `server.rs`, `hover.rs`,
  `completion.rs`, etc.

- **Move tests with extracted code.** Format-related tests
  move to `formats.rs`; association-related tests move to
  `association.rs`. This keeps tests next to the code they
  exercise and reduces the remaining `mod.rs` test module
  size.

- **No import narrowing for smoke submodules.** Each uses
  `use super::*`. The import set is small and narrowing
  would add maintenance burden for no benefit.
