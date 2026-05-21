**Repository:** root
**Status:** InProgress
**Created:** 2026-05-20

# Split `tests/corpus_invariants.rs` into per-invariant modules

## Goal

The 2435-line `rlsp-yaml/tests/corpus_invariants.rs`
integration test bundles 11 independent invariant checks
(I1 through I11, no I7) that run across a corpus of YAML
files, plus a `mod tests` block with roughly 85 unit tests
covering each invariant's helpers. Reorganize the file into
a `tests/corpus_invariants/` directory using the `main.rs`
+ sibling-modules layout already adopted by
`rlsp-yaml-parser/tests/smoke/` and
`rlsp-yaml-parser/tests/conformance/`, so each invariant
owns one focused module containing its check function, its
private helpers, and its unit tests, while the
corpus-runner orchestration stays in `main.rs`.

## Context

- **Reference layout:** `rlsp-yaml-parser/tests/smoke/main.rs`
  declares `mod anchors_and_aliases; mod block_scalars; ...`
  and each sibling file owns one topic. Cargo auto-detects
  `tests/<name>/main.rs` as a test binary named `<name>`;
  no `[[test]]` entry exists in `rlsp-yaml/Cargo.toml`, so
  no Cargo manifest changes are needed.
- **Source-of-truth file:**
  `rlsp-yaml/tests/corpus_invariants.rs` (2435 lines).
- **Top-level structure:** a single `#[test] fn
  corpus_invariants()` (line 1155) iterates an
  `INVARIANTS: &[Invariant]` array of 11 entries (line 75)
  over every file in the corpus, calling each check
  function and reporting failures.
- **Eleven invariant check functions:**
  - `check_i1_no_panics` (line 137)
  - `check_i2_range_validity` (line 200) + helpers
    `check_diagnostic_ranges`, `utf16_len`,
    `check_utf8_boundary`
  - `check_i3_code_action_round_trip` (line 339) + helpers
    `apply_text_edits`, `lsp_pos_to_byte_offset`
  - `check_i4_scalar_preservation` (line 430) + helpers
    `collect_scalar_values`, `collect_node_scalars`,
    `missing_scalars`
  - `check_i5_anchor_loc_invariant` (line 507) + helper
    `check_i5_node`
  - `check_i6_tag_loc_invariant` (line 551) + helpers
    `check_i6_node`, `check_i6_references_no_panics`
  - `check_i8_selection_no_panic` (line 637)
  - `check_i9_complete_at_no_panics` (line 667) + helper
    `safe_utf16_midpoint`
  - `check_i10_formatter_round_trip` (line 733)
  - `check_i11_validator_stability_under_reemit` (line 787)
    + helpers `i11_build_schema`, `i11_collect_diagnostics`,
    `diagnostic_identity_multiset`
- **Cross-invariant shared utilities (lines 831–1153):**
  `documents_equivalent`, `nodes_equivalent`,
  `node_kind_name`, `node_tag_str`,
  `collect_error_diagnostics`, `error_key_set`,
  `error_key`, `fmt_range`, `collect_all_diagnostics`,
  `panic_message`, `collect_corpus_files`, `is_skipped`,
  `CheckOutcome`, `run_check`, plus the `SKIP_LIST: &[(&str,
  &str, &str)] = &[]` constant.
- **Unit tests (`mod tests` block, lines 1198–2435):**
  roughly 85 `#[test]` functions grouped by invariant:
  I2 range/UTF tests, I3 text-edit tests, I4 scalar-value
  tests, I6 tag-loc tests, I9 cursor tests, I10 round-trip
  tests, I11 multiset tests, plus shared test helpers
  `with_temp_dir`, `make_diag`, `collect_from`,
  `skip_list_contains`, `load_docs`, `run_i9`, `run_i10`,
  `run_i11`, `make_i11_diag`, `zero_span`, `make_scalar`,
  `make_mapping`, `make_sequence`, `make_doc`.
- **Layout constraint:** Cargo refuses to build two test
  binaries with the same name; `tests/corpus_invariants.rs`
  and `tests/corpus_invariants/main.rs` cannot coexist. The
  rename must happen in a single commit.
- **Test colocation rule (from the user):** every `mod
  tests` unit-test block must live in the same file as the
  function(s) it exercises. The current monolithic `mod
  tests` block is split per-invariant during extraction.
- **Test routing rule (from the lsp_lifecycle split
  retrospective):** when extracting tests, decide each
  test's destination by what its body asserts — not by
  its name and not by its position in the file. The
  `iN_*` naming convention is a strong signal but not
  authoritative; a test named `iN_…` that exercises a
  shared utility may belong with that utility, and a
  test with a generic name that calls `check_iN_…` belongs
  with invariant N. If a test in the original `mod tests`
  block doesn't obviously belong to any invariant by name,
  read its body and route by which check function or
  helper it exercises.
- **Public API impact:** none — integration tests have no
  external consumers.
- **Build/test commands (from CLAUDE.md):** `cargo build`,
  `cargo test`, `cargo clippy --all-targets`, `cargo fmt`.

## Steps

- [x] Migrate the file into the new directory layout
- [x] Extract the cross-invariant shared utilities into a
      `shared` module
- [x] Extract invariants I1–I4 (each as its own module with
      its own `mod tests`)
- [x] Extract invariants I5, I6, I8, I9
- [ ] Extract invariants I10 and I11
- [ ] Verify `main.rs` contains only the corpus runner and
      module declarations

## Tasks

### Task 1: Migrate file to directory layout

Rename `rlsp-yaml/tests/corpus_invariants.rs` to
`rlsp-yaml/tests/corpus_invariants/main.rs` via `git mv`,
keeping the content byte-identical.

- [x] `git mv rlsp-yaml/tests/corpus_invariants.rs
      rlsp-yaml/tests/corpus_invariants/main.rs`
- [x] `cargo test --test corpus_invariants` runs
      successfully
- [x] The test count reported by `cargo test --test
      corpus_invariants -- --list 2>&1 | grep -c '^test '`
      matches the count produced from the pre-rename file;
      record both numbers in the commit message
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `7982ff5` (amended; see `git log --follow rlsp-yaml/tests/corpus_invariants/main.rs`)

### Task 2: Extract cross-invariant shared utilities

Move every utility that is used by two or more invariants,
the corpus runner itself, or by the shared test helpers
into `rlsp-yaml/tests/corpus_invariants/shared.rs`. The
following items belong in `shared.rs`:

- `collect_corpus_files`, `is_skipped`, `SKIP_LIST`,
  `CheckOutcome`, `run_check`, `panic_message` (corpus
  runner)
- `documents_equivalent`, `nodes_equivalent`,
  `node_kind_name`, `node_tag_str` (used by I4 and I10)
- `collect_error_diagnostics`, `error_key_set`,
  `error_key`, `fmt_range`, `collect_all_diagnostics`
  (used by I11 and the I2 diagnostic helpers)
- The unit-test helpers `with_temp_dir`, `make_diag`,
  `collect_from`, `skip_list_contains`, `load_docs`,
  `zero_span`, `make_scalar`, `make_mapping`,
  `make_sequence`, `make_doc` go into a
  `pub(super)`-visible `pub mod helpers` inside
  `shared.rs` so per-invariant test modules can reach them
  via `use super::shared::helpers::*;`.

The `INVARIANTS: &[Invariant]` array stays in `main.rs`
because it is the corpus runner's input and references
every per-invariant check function (which become `use`
imports as those modules are introduced in later tasks).

Also update the sentence in
`rlsp-yaml/tests/corpus/WORKLIST.md` that names the
`SKIP_LIST` constant's source file so it points at
`rlsp-yaml/tests/corpus_invariants/shared.rs` (the new
home of `SKIP_LIST`) instead of the pre-split path. The
old path appears in a single sentence that wraps across
the file's first paragraph — every occurrence of
`rlsp-yaml/tests/corpus_invariants.rs` must be replaced.
This is the only live-source documentation file that
names the moved path.

Acceptance:

- [x] `tests/corpus_invariants/shared.rs` exists and
      contains exactly the symbols listed above
- [x] `tests/corpus_invariants/main.rs` declares `mod
      shared;` and references the moved items via `use
      shared::*;` (or qualified paths) — no duplicate
      definitions remain
- [x] `rlsp-yaml/tests/corpus/WORKLIST.md` names
      `rlsp-yaml/tests/corpus_invariants/shared.rs` as the
      location of `SKIP_LIST`; the pre-split path
      `rlsp-yaml/tests/corpus_invariants.rs` no longer
      appears anywhere in that file
- [x] `cargo test --test corpus_invariants` test count is
      unchanged from the Task 1 baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `a4f5757` (amended; see `git log --follow rlsp-yaml/tests/corpus_invariants/shared.rs`)

### Task 3: Extract invariant modules I1, I2, I3, I4

For each invariant listed below, create a sibling module
under `tests/corpus_invariants/` containing the check
function, its private helpers, and a `#[cfg(test)] mod
tests` block holding every existing unit test that
exercises that invariant's check function or any of its
private helpers — identified by reading the test body's
assertions, not by pattern-matching on test names alone.
Each module declares `use super::shared::*;` (and
`use super::shared::helpers::*;` inside its `mod tests`)
to reach the shared utilities. After extracting all named
tests, scan the parent file for any remaining tests in
the original `mod tests` block that exercise this
invariant's functions or helpers and move them as well —
do not leave a test stranded in `shared.rs` or in
`main.rs` just because its name doesn't follow the
`iN_*` convention.

- [ ] `i1_no_panics.rs` — `check_i1_no_panics`. The
      original `mod tests` block contains no `#[test]`
      function that exercises `check_i1_no_panics`,
      `i1_*`, or any I1-specific helper; the invariant is
      verified exclusively through the corpus runner. The
      new file therefore contains no `#[cfg(test)] mod
      tests` block.
- [ ] `i2_range_validity.rs` — `check_i2_range_validity`,
      `check_diagnostic_ranges`, `utf16_len`,
      `check_utf8_boundary`, plus the `i2_ut1` … `i2_ut12`
      unit tests and the `collect_corpus_files_*`
      filesystem-bounded tests that exercise the I2 helpers
- [ ] `i3_code_action_round_trip.rs` —
      `check_i3_code_action_round_trip`, `apply_text_edits`,
      `lsp_pos_to_byte_offset`, plus the `i3_at1` …
      `i3_at9` unit tests
- [ ] `i4_scalar_preservation.rs` —
      `check_i4_scalar_preservation`,
      `collect_scalar_values`, `collect_node_scalars`,
      `missing_scalars`, plus the `i4_csv1` … `i4_csv10`,
      `i4_ms1` … `i4_ms7`, and `i4_int1` unit tests

Acceptance:

- [x] Each module listed above exists with the check
      function, helpers, and unit tests specified
- [x] `tests/corpus_invariants/main.rs` declares `mod
      i1_no_panics; mod i2_range_validity; mod
      i3_code_action_round_trip; mod i4_scalar_preservation;`
      and imports each check function for the `INVARIANTS`
      array (e.g. `use i1_no_panics::check_i1_no_panics;`)
- [x] `main.rs` no longer contains the moved check
      functions, helpers, or unit tests
- [x] `cargo test --test corpus_invariants` test count
      remains the Task 1 baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes

Commit: `a2425d9` (amended; see `git log --follow rlsp-yaml/tests/corpus_invariants/i2_range_validity.rs`)

### Task 4: Extract invariant modules I5, I6, I8, I9

- [ ] `i5_anchor_loc_invariant.rs` —
      `check_i5_anchor_loc_invariant` and
      `check_i5_node`. The original `mod tests` block
      contains no `#[test]` function that exercises
      `check_i5_*` or any I5-specific helper; anchor-loc
      coverage is provided through the corpus runner and
      through the I6 unit tests in
      `i6_tag_loc_invariant.rs`. The new file therefore
      contains no `#[cfg(test)] mod tests` block.
- [ ] `i6_tag_loc_invariant.rs` —
      `check_i6_tag_loc_invariant`, `check_i6_node`,
      `check_i6_references_no_panics`, plus the
      `i6_resolver_injected_tag_no_tag_loc_passes` and
      `i6_explicit_user_tag_with_tag_loc_passes` unit
      tests
- [ ] `i8_selection_no_panic.rs` —
      `check_i8_selection_no_panic`
- [ ] `i9_complete_at_no_panics.rs` —
      `check_i9_complete_at_no_panics`,
      `safe_utf16_midpoint`, plus the I9-specific unit
      tests and the `run_i9` test helper

Acceptance:

- [x] Each module listed above exists with the check
      function, helpers, and unit tests specified
- [x] `main.rs` declares each module and imports each
      check function for the `INVARIANTS` array
- [x] `main.rs` no longer contains the moved content
- [x] `cargo test --test corpus_invariants` test count
      remains the Task 1 baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes

Commit: `044cc90` (amended; see `git log --follow rlsp-yaml/tests/corpus_invariants/i6_tag_loc_invariant.rs`)

### Task 5: Extract invariant modules I10 and I11

- [ ] `i10_formatter_round_trip.rs` —
      `check_i10_formatter_round_trip`, plus the `i10_ut1`
      … `i10_ut5` unit tests and the `run_i10` test helper
- [ ] `i11_validator_stability.rs` —
      `i11_build_schema`, `i11_collect_diagnostics`,
      `diagnostic_identity_multiset`,
      `check_i11_validator_stability_under_reemit`, plus
      the `i11_ut1` … `i11_ut15` unit tests and the
      `make_i11_diag` and `run_i11` test helpers

Acceptance:

- [ ] Both modules exist with the check function, helpers,
      and unit tests specified
- [ ] `main.rs` declares both modules and imports each
      check function for the `INVARIANTS` array
- [ ] `main.rs` no longer contains the moved content
- [ ] `cargo test --test corpus_invariants` test count
      remains the Task 1 baseline
- [ ] `cargo clippy --all-targets -- -D warnings` passes

### Task 6: Verify orchestration-only `main.rs`

This task is primarily a verification of the post-Task-5
state. If Tasks 2–5 already produced an orchestration-only
`main.rs`, Task 6 will have no source diff — submit a
verification-only handoff documenting the measured criteria
(grep/ls/cargo command outputs) and the plan-progress
update only. If verification reveals leftover `fn` items,
`#[test]` attributes, or empty `mod tests` blocks in any
sibling, fix them and report what changed.

After all extractions, `tests/corpus_invariants/main.rs`
contains only:

- crate-level attributes
- a module-level doc comment
- `mod shared; mod i1_no_panics; mod i2_range_validity; …`
  declarations (one per sibling module)
- `use` imports of each check function from its module
- the `INVARIANTS` array
- the `corpus_invariants()` `#[test]` function (the corpus
  runner)
- the `Invariant` struct definition (only used by
  `INVARIANTS` and the runner)

No other `fn` items, no leftover helper functions, no
leftover `mod tests` block.

- [ ] `tests/corpus_invariants/main.rs` contains exactly
      one `#[test]` attribute (the `corpus_invariants`
      function); no other test attributes appear
- [ ] `tests/corpus_invariants/main.rs` contains no
      `#[cfg(test)]` block
- [ ] Every sibling `.rs` file in
      `tests/corpus_invariants/` corresponds to a `mod
      <name>;` declaration in `main.rs`, and every `mod
      <name>;` declaration corresponds to an existing
      sibling file
- [ ] No sibling `.rs` file contains an empty
      `#[cfg(test)] mod tests {}` block — `i1_no_panics.rs`
      and `i5_anchor_loc_invariant.rs` deliberately have
      no `mod tests` block at all, and every other sibling
      module has a `mod tests` block with at least one
      `#[test]` function inside
- [ ] `cargo test --test corpus_invariants` reports the
      same total test count as the Task 1 baseline; record
      the final count and the baseline count in the commit
      message
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Decisions

- **Layout pattern:** mirror
  `rlsp-yaml-parser/tests/smoke/main.rs` —
  `tests/corpus_invariants/main.rs` is the Cargo-detected
  test binary entry, and sibling `.rs` files declared via
  `mod` own one invariant each.
- **One module per invariant:** the eleven invariants are
  thematically independent (they check different
  properties, share no internal state, and were already
  named I1–I11). Per-invariant modules keep each
  invariant's check function, helpers, and unit tests in
  one file the developer can navigate without
  cross-referencing.
- **Shared utilities live in `shared.rs`:** utilities used
  by more than one invariant, by the corpus runner, or by
  multiple test groups go in a single `shared.rs` module.
  This avoids forcing one invariant module to expose
  internals just because another invariant happens to
  reuse a helper.
- **Test helpers nested in `shared::helpers`:** unit-test
  fixtures (`with_temp_dir`, `make_diag`, `make_scalar`,
  etc.) are reachable via `use super::shared::helpers::*;`
  from each per-invariant `mod tests` block.
- **`INVARIANTS` array stays in `main.rs`:** it is the
  corpus runner's input and references every check
  function. Keeping it next to the runner avoids
  introducing an additional layer of indirection.
- **Atomic file rename:** Task 1 is a pure `git mv` so the
  rename is recorded distinctly. Cargo cannot tolerate
  both `tests/corpus_invariants.rs` and
  `tests/corpus_invariants/main.rs` coexisting.
- **Preserve `SKIP_LIST`:** the existing empty `SKIP_LIST`
  constant is preserved in `shared.rs` as-is. Removing or
  modifying it is out of scope for this reorganization.

## Non-Goals

- Modifying the corpus, the invariant logic, or the
  diagnostic-generation code being tested. This plan only
  reorganizes test file layout.
- Adding new invariants, new unit tests, or new corpus
  files.
- Consolidating duplicated `MAX_DEPTH` or similar
  per-helper constants.
- Changing `SKIP_LIST`, adding skip entries, or removing
  the skip-list infrastructure.
- Changing any other test file under `rlsp-yaml/tests/` —
  covered by separate plans or out of scope.
