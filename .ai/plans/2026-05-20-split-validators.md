**Repository:** root
**Status:** Completed (2026-05-21)
**Created:** 2026-05-20

# Split `src/validation/validators.rs` into per-validator submodules

## Goal

The 2514-line `rlsp-yaml/src/validation/validators.rs`
contains six structurally independent validators
(`validate_unused_anchors`, `validate_flow_style`,
`validate_custom_tags`, `validate_key_ordering`,
`validate_duplicate_keys`, `validate_yaml11_compat`) plus
the `CustomTag` parsing utilities (`TagNodeType`,
`CustomTag`, `parse_custom_tag`), with all 162 unit tests
in a single `mod tests` block. Reorganize the file using
the project's `foo.rs` + adjacent `foo/` directory
convention so each validator lives in its own submodule
with its private helpers and its dedicated unit tests in
the same file. The parent `validators.rs` becomes a thin
orchestrator that declares the submodules and re-exports
each validator's public surface, preserving the existing
`crate::validation::validators::X` and
`rlsp_yaml::validation::validators::X` paths used by
`server.rs`, `editing/code_actions.rs`, and three
integration test binaries.

## Context

- **Module-layout convention in this repo:** `foo.rs` plus
  adjacent `foo/` directory (Rust 2018+ style, no
  `mod.rs`). Confirmed by `src/editing/code_actions.rs`
  with its `src/editing/code_actions/` directory, and by
  `src/schema_validation.rs` with its
  `src/schema_validation/formats.rs` submodule.
- **Source-of-truth file:**
  `rlsp-yaml/src/validation/validators.rs` (2514 lines).
- **Public surface (must remain reachable at the existing
  paths):**
  - `pub enum TagNodeType` (line 16)
  - `pub struct CustomTag` (line 27)
  - `pub fn parse_custom_tag` (line 46)
  - `pub fn validate_unused_anchors` (line 89)
  - `pub fn validate_flow_style` (line 198)
  - `pub fn validate_custom_tags` (line 315)
  - `pub fn validate_key_ordering` (line 428)
  - `pub fn validate_duplicate_keys` (line 510)
  - `pub fn validate_yaml11_compat` (line 584)
- **External callers (paths that must keep working):**
  - `src/server.rs` lines 481, 492, 496, 506, 515, 520,
    525, 528, 530, 541 — uses
    `crate::validation::validators::{CustomTag,
    parse_custom_tag, validate_unused_anchors,
    validate_flow_style, validate_duplicate_keys,
    validate_key_ordering, validate_custom_tags,
    validate_yaml11_compat}`
  - `src/editing/code_actions.rs` line 154 — uses
    `crate::validation::validators::validate_flow_style`
  - `tests/corpus_invariants.rs` line 49 — uses
    `rlsp_yaml::validation::validators::{...}`
  - `tests/ecosystem_fixtures.rs` line 15 — uses
    `rlsp_yaml::validation::validators::{validate_duplicate_keys,
    validate_flow_style}`
  - `tests/code_action_property_preservation.rs` line 21 —
    uses `rlsp_yaml::validation::validators::validate_flow_style`
- **Private helpers and their owners:**
  - `parse_custom_tag` is paired with `TagNodeType` and
    `CustomTag`
  - `validate_unused_anchors` is paired with
    `struct AnchorEntry` (line 75) and
    `fn collect_anchors_and_aliases` (line 144)
  - `validate_flow_style` is paired with
    `fn collect_flow_style_diagnostics` (line 214) and
    `fn flow_diagnostic` (line 277)
  - `validate_custom_tags` is paired with
    `fn collect_tag_diagnostics` (line 336)
  - `validate_key_ordering` is paired with
    `fn check_yaml_ordering` (line 440)
  - `validate_duplicate_keys` is paired with
    `fn check_node_for_duplicate_keys` (line 527) and
    `fn push_duplicate_diagnostic` (line 670)
  - `validate_yaml11_compat` is paired with
    `fn collect_yaml11_diagnostics` (line 594)
- **Shared concern:** `validate_flow_style` and
  `validate_duplicate_keys` both accept a
  `&ValidationSettings` parameter. `ValidationSettings`
  lives in `src/validation/settings.rs`; it is unaffected
  by this reorganization.
- **Unit tests:** `mod tests` at line 700–2514, ~162
  `#[test]` functions using `rstest` parametrization,
  organized in contiguous blocks per validator:
  - unused-anchor tests: lines 729–1026 (~26 tests)
  - flow-style tests: lines 1055–1365 (~33 tests)
  - key-ordering tests: lines 1381–1434 (~7 tests)
  - custom-tag parsing + validation tests: lines 1460–1828
    (~36 tests)
  - duplicate-keys tests: lines 1911–2188 (~32 tests)
  - yaml11-compat tests: lines 2224–2487 (~28 tests)
  - Two `mod tests` helpers used across groups:
    `parse_anchors` (line 709) and `parse_duplicate` (line
    713) — each is used by only the validator named in its
    name and travels with that validator.
- **Test colocation rule (from the user):** every `mod
  tests` unit-test block must live in the same file as the
  function(s) it exercises. The single monolithic `mod
  tests` block is split per-validator during extraction.
- **Test routing rule (from the lsp_lifecycle split
  retrospective):** when extracting tests, decide each
  test's destination by what its body asserts — not by
  its name and not by its position in the file. The
  rstest case names and section-header line ranges are
  strong signals but not authoritative; if a test
  doesn't obviously belong to any validator by name, read
  its body and route by which `validate_*` function (or
  helper) it exercises. Do not leave a stray test
  stranded in the parent file.
- **Build/test commands (from CLAUDE.md):** `cargo build`,
  `cargo test`, `cargo clippy --all-targets`, `cargo fmt`.

## Steps

- [x] Extract `custom_tag` (types + parser) and `anchors`
- [x] Extract `flow_style` and `key_ordering`
- [x] Extract `duplicate_keys` and `yaml11_compat`
- [x] Extract `custom_tags_validation`
- [x] Verify `validators.rs` is orchestration only and the
      external call sites continue to compile unchanged

## Tasks

### Task 1: Extract `custom_tag` and `anchors` submodules

Create `src/validation/validators/` and add the first two
submodules. `custom_tag` contains the type definitions and
the string parser (no dependency on any validator).
`anchors` contains the anchor-unused validator and its
internal data structures.

- [x] `src/validation/validators/custom_tag.rs` exists and
      contains:
  - `pub enum TagNodeType`
  - `pub struct CustomTag`
  - `pub fn parse_custom_tag`
  - a `#[cfg(test)] mod tests` block holding every
    existing unit test that calls `parse_custom_tag`
    (sourced from the custom-tag parsing block in the
    original `mod tests`)
- [x] `src/validation/validators/anchors.rs` exists and
      contains:
  - `struct AnchorEntry`
  - `pub fn validate_unused_anchors`
  - `fn collect_anchors_and_aliases`
  - a `#[cfg(test)] mod tests` block holding the
    unused-anchor / unresolved-alias tests (lines 729–1026
    of the original `mod tests` block) and the
    `parse_anchors` test helper
- [x] `src/validation/validators.rs` declares
      `pub mod custom_tag;` and `pub mod anchors;`
- [x] `src/validation/validators.rs` re-exports
      `pub use custom_tag::{TagNodeType, CustomTag,
      parse_custom_tag};` and
      `pub use anchors::validate_unused_anchors;`
- [x] `src/validation/validators.rs` no longer defines the
      moved items or contains the moved unit tests
- [x] `cargo build` succeeds without any new compiler
      warnings
- [x] `cargo test` reports the same total test count as
      the pre-task baseline; record both numbers in the
      commit message
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes
- [x] `grep -rn "validation::validators::\\(TagNodeType\\|CustomTag\\|parse_custom_tag\\|validate_unused_anchors\\)"
      rlsp-yaml/src rlsp-yaml/tests` returns the same set
      of call sites as before this task — no caller paths
      were rewritten

Commit: `06ff9a3` (amended; see `git log --follow rlsp-yaml/src/validation/validators/anchors.rs`)

### Task 2: Extract `flow_style` and `key_ordering`

- [x] `src/validation/validators/flow_style.rs` exists and
      contains:
  - `pub fn validate_flow_style`
  - `fn collect_flow_style_diagnostics`
  - `fn flow_diagnostic`
  - a `#[cfg(test)] mod tests` block holding the
    flow-style tests (lines 1055–1365 of the original
    `mod tests` block)
- [x] `src/validation/validators/key_ordering.rs` exists
      and contains:
  - `pub fn validate_key_ordering`
  - `fn check_yaml_ordering`
  - a `#[cfg(test)] mod tests` block holding the
    key-ordering tests (lines 1381–1434 of the original
    `mod tests` block)
- [x] `src/validation/validators.rs` declares both
      submodules and re-exports
      `pub use flow_style::validate_flow_style;` and
      `pub use key_ordering::validate_key_ordering;`
- [x] `src/validation/validators.rs` no longer defines the
      moved items or contains the moved unit tests
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes
- [x] All five external call sites listed in Context still
      compile without source changes

Commit: `a46b142` (amended; see `git log --follow rlsp-yaml/src/validation/validators/flow_style.rs`)

### Task 3: Extract `duplicate_keys` and `yaml11_compat`

- [x] `src/validation/validators/duplicate_keys.rs` exists
      and contains:
  - `pub fn validate_duplicate_keys`
  - `fn check_node_for_duplicate_keys`
  - `fn push_duplicate_diagnostic`
  - a `#[cfg(test)] mod tests` block holding the
    duplicate-key tests (lines 1911–2188 of the original
    `mod tests` block) and the `parse_duplicate` test
    helper
- [x] `src/validation/validators/yaml11_compat.rs` exists
      and contains:
  - `pub fn validate_yaml11_compat`
  - `fn collect_yaml11_diagnostics`
  - a `#[cfg(test)] mod tests` block holding the
    yaml11-compat tests (lines 2224–2487 of the original
    `mod tests` block)
- [x] `src/validation/validators.rs` declares both
      submodules and re-exports
      `pub use duplicate_keys::validate_duplicate_keys;`
      and `pub use yaml11_compat::validate_yaml11_compat;`
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `ace8172` (amended; see `git log --follow rlsp-yaml/src/validation/validators/duplicate_keys.rs`)

### Task 4: Extract `custom_tags_validation`

`validate_custom_tags` is the diagnostic-emitting validator
that checks whether parsed tags match the configured
`CustomTag` allowlist. It depends on the types defined in
`custom_tag` (Task 1) but is separate from the parser. Keep
it in its own submodule so the parser-only path and the
validator-only path can be reasoned about independently.

- [x] `src/validation/validators/custom_tags_validation.rs`
      exists and contains:
  - `pub fn validate_custom_tags`
  - `fn collect_tag_diagnostics`
  - a `#[cfg(test)] mod tests` block holding every test
    in the custom-tag section (lines 1460–1828 of the
    original `mod tests` block) whose body asserts on
    `validate_custom_tags` or `collect_tag_diagnostics`
    behavior — determined by reading the test body, not
    by name or position. This includes tests that call
    `parse_custom_tag` as setup if their assertion
    target is the tag validator. Tests whose assertion
    target is `parse_custom_tag` itself (e.g. parser
    round-trip, parse-error formatting) move with Task 1
    into `custom_tag.rs`.
  - any imports needed from `super::custom_tag` for
    `CustomTag` / `TagNodeType`
- [x] `src/validation/validators.rs` declares
      `pub mod custom_tags_validation;` and re-exports
      `pub use custom_tags_validation::validate_custom_tags;`
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `f9217e5` (amended; see `git log --follow rlsp-yaml/src/validation/validators/custom_tags_validation.rs`)

### Task 5: Verify orchestration-only `validators.rs`

This task is primarily a verification of the post-Task-4
state. If Tasks 1–4 already produced an orchestration-only
parent file, Task 5 will have no source diff — submit a
verification-only handoff documenting the measured criteria
(grep/ls/cargo command outputs) and the plan-progress
update only. If verification reveals any leftover `fn` /
`struct` / `enum` items, missing re-exports, or stranded
unit tests in `validators.rs`, fix them and report what
changed.

After all extractions, `src/validation/validators.rs`
contains only:

- a module-level doc comment
- `pub mod custom_tag; pub mod anchors; pub mod
  flow_style; pub mod key_ordering; pub mod
  duplicate_keys; pub mod yaml11_compat; pub mod
  custom_tags_validation;`
- `pub use` re-exports that preserve every existing public
  symbol path under `crate::validation::validators::`

No `fn` items, no `struct`/`enum` items, no `mod tests`
block.

- [x] `src/validation/validators.rs` contains zero `fn`
      definitions, zero `struct`/`enum`/`type`
      definitions, and zero `#[cfg(test)]` blocks
- [x] Every sibling `.rs` file under
      `src/validation/validators/` corresponds to a `pub
      mod <name>;` declaration in `validators.rs`, and
      every declaration corresponds to an existing sibling
      file
- [x] The set of public symbols re-exported by
      `validators.rs` is exactly:
      `TagNodeType, CustomTag, parse_custom_tag,
      validate_unused_anchors, validate_flow_style,
      validate_custom_tags, validate_key_ordering,
      validate_duplicate_keys, validate_yaml11_compat`
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` reports the same total test count as
      the pre-Task-1 baseline; record both numbers in the
      commit message
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes
- [x] None of the five external call sites listed in
      Context were modified (`git diff --stat` shows
      `validators.rs` plus new submodule files only)

Commit: `dd5a6e5` (Task 5 verification + plan completion)

## Decisions

- **Module-layout convention:** `validators.rs` becomes the
  module-entry file alongside a new
  `src/validation/validators/` directory containing the
  submodules. This matches `src/editing/code_actions.rs` +
  `src/editing/code_actions/` and `src/schema_validation.rs`
  + `src/schema_validation/formats.rs`.
- **Public API preservation via `pub use` re-exports:**
  every existing public symbol stays reachable at its
  current path
  (`crate::validation::validators::validate_flow_style`,
  etc.) so the 10 call sites in `server.rs`, the one in
  `editing/code_actions.rs`, and the three integration
  test binaries compile without source changes.
- **One submodule per validator + one submodule for the
  custom-tag types:** the six validators are
  structurally independent (no shared state, no shared
  helpers); each becomes a sibling module containing its
  public function, its private helpers, and its unit
  tests. `parse_custom_tag` plus the `TagNodeType` and
  `CustomTag` types live together in `custom_tag` because
  they are tightly coupled to each other but used by both
  `validate_custom_tags` and external callers.
- **Test colocation:** each per-validator `mod tests`
  block sits at the bottom of its own submodule, holding
  exactly the tests for that validator's public fn and
  private helpers. The `parse_anchors` and
  `parse_duplicate` test-only helpers travel with the
  validator they support.
- **No incremental shim file:** the parent file
  `validators.rs` stays present throughout — only new
  files are added and old contents are removed in-place,
  so no rename or temporary forwarding module is needed.
- **Caller-path reference in Context may shift:** the
  Context section lists `tests/corpus_invariants.rs line
  49` as an external caller. The sibling plan
  `2026-05-20-split-corpus-invariants-tests.md` renames
  that file into `tests/corpus_invariants/main.rs` plus
  sibling modules; if it runs before this plan, the `use
  rlsp_yaml::validation::validators::{...}` statement
  will have moved to `shared.rs` or a per-invariant
  submodule by execution time. The acceptance criterion
  "All five external call sites listed in Context still
  compile without source changes" still holds because
  the public API path
  (`rlsp_yaml::validation::validators::*`) does not
  change.
- **`rlsp-yaml/README.md` is not updated:** the README's
  "Architecture" section (lines 186–208) is a conceptual
  module map, not a literal file tree — it already lists
  `validators.rs`, `code_actions.rs`, `hover.rs`,
  `folding.rs`, etc. as if they sat directly under `src/`
  even though several already live in subdirectories
  (`src/validation/`, `src/editing/`, `src/analysis/`,
  `src/decorators/`). The description "Non-schema
  diagnostics (anchors, flow, keys)" documents what the
  `validators` module does for callers, not what the
  literal `validators.rs` file contains. After the split,
  the module continues to deliver exactly those
  diagnostics through its re-exports, so the
  behavior-level description remains accurate and no
  README update is in scope.

## Non-Goals

- Changing validator behavior, diagnostic codes, or error
  messages.
- Modifying `ValidationSettings` or moving it.
- Consolidating the duplicated `const MAX_DEPTH: usize =
  100;` constants that appear inside several validator
  helpers — each is intentionally function-local and
  preserved as-is.
- Renaming any public symbol.
- Splitting any other source file
  (`src/schema_validation.rs`, `src/editing/formatter.rs`,
  `src/completion.rs`, etc.) — covered by separate plans.
- Removing or modifying `SKIP_LIST`, suppression rules, or
  any other validation infrastructure not listed in this
  plan.
