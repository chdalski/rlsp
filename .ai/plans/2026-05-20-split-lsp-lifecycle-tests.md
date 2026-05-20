**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-20

# Split `tests/lsp_lifecycle.rs` into per-LSP-capability modules

## Goal

The 4103-line `rlsp-yaml/tests/lsp_lifecycle.rs` integration
test bundles 118 `#[tokio::test]` async tests covering every
LSP method the server exposes (lifecycle, hover, completion,
folding ranges, navigation, rename, code actions, validators,
formatting settings, schema modelines, Kubernetes detection,
etc.). Reorganize the file into a `tests/lsp_lifecycle/`
directory using the `main.rs` + sibling-modules layout already
adopted by `rlsp-yaml-parser/tests/smoke/` and
`rlsp-yaml-parser/tests/conformance/`, so each LSP capability
has its own focused test file while the suite continues to be
built and run as a single Cargo integration test binary named
`lsp_lifecycle`.

## Context

- **Reference layout:** `rlsp-yaml-parser/tests/smoke/main.rs`
  declares `mod anchors_and_aliases; mod block_scalars; ...`
  and each sibling file owns one grammar topic. Cargo
  auto-detects `tests/<name>/main.rs` as a test binary named
  `<name>`; neither `rlsp-yaml-parser/Cargo.toml` nor
  `rlsp-yaml/Cargo.toml` has any `[[test]]` entry, so no
  Cargo manifest changes are needed.
- **Source-of-truth file:**
  `rlsp-yaml/tests/lsp_lifecycle.rs` (4103 lines, 118
  `#[tokio::test]` async functions).
- **Shared LSP helpers (lines 17–77):** `initialize_request`,
  `initialized_notification`, `shutdown_request`,
  `did_open_notification`, `did_change_notification`,
  `did_close_notification`, `send`. Every test uses `send`;
  every lifecycle test uses the request builders. There is
  no nested `mod tests` block — every test is top-level.
- **Section markers already present:** the file has 21
  explicit `// ---- <capability> ----` comment headers that
  identify natural groupings: Validator Integration Tests,
  selection_range, code_action, code_lens,
  on_type_formatting, semantic_tokens_full,
  did_change_configuration, did_change_watched_files,
  key ordering validation path, custom tags with type
  annotations, $schema=none modeline, glob-based schema
  association fallback, hover with schema association via
  modeline, completion with schema association via modeline,
  Kubernetes auto-detection, schema-aware YAML 1.1
  diagnostics (integration), flowStyle setting,
  duplicateKeys setting, formatEnforceBlockStyle setting,
  formatPreserveQuotes setting, formatEnable setting.
- **Layout constraint:** Cargo refuses to compile two test
  binaries with the same name. If `tests/lsp_lifecycle.rs`
  AND `tests/lsp_lifecycle/main.rs` both exist, the build
  fails. The rename must happen in a single commit; partial
  intermediate states with both paths present are not
  allowed.
- **Public API impact:** none — integration tests have no
  external consumers.
- **Build/test commands (from CLAUDE.md):** `cargo build`,
  `cargo test`, `cargo clippy --all-targets`, `cargo fmt`.

## Steps

- [ ] Migrate the file into the new directory layout
- [ ] Extract the shared LSP request/response helpers into a
      `helpers` module
- [ ] Extract per-LSP-capability tests (small modules)
- [ ] Extract configuration, watched-files, custom-tags, and
      schema-routing modules
- [ ] Extract the validators integration test group
- [ ] Extract the formatting integration test group
- [ ] Verify `main.rs` is orchestration only

## Tasks

### Task 1: Migrate file to directory layout

Rename `rlsp-yaml/tests/lsp_lifecycle.rs` to
`rlsp-yaml/tests/lsp_lifecycle/main.rs` via `git mv`, keeping
the file content byte-identical. No tests change. This task
exists as a separate slice so the rename is visible in
`git log --follow` independently of any later content edits,
and so subsequent tasks start from a known-green baseline in
the new layout.

- [ ] `git mv rlsp-yaml/tests/lsp_lifecycle.rs
      rlsp-yaml/tests/lsp_lifecycle/main.rs`
- [ ] `cargo test --test lsp_lifecycle` runs successfully
- [ ] The test count reported by `cargo test --test
      lsp_lifecycle -- --list 2>&1 | grep -c '^test '`
      equals 118 (the baseline before the rename); record
      both numbers in the commit message
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes

### Task 2: Extract shared LSP test helpers

The seven helpers at lines 17–77 of the original file
(`initialize_request`, `initialized_notification`,
`shutdown_request`, `did_open_notification`,
`did_change_notification`, `did_close_notification`, `send`)
are used by every test group. Move them into
`rlsp-yaml/tests/lsp_lifecycle/helpers.rs` and declare
`mod helpers;` plus `use helpers::*;` in `main.rs`.

- [ ] `tests/lsp_lifecycle/helpers.rs` exists and contains
      exactly the seven helpers listed above and no other
      functions
- [ ] `tests/lsp_lifecycle/main.rs` declares `mod helpers;`
      and references the helpers via `use helpers::*;`
- [ ] All 118 tests still compile and pass
- [ ] `cargo clippy --all-targets -- -D warnings` passes

### Task 3: Extract per-capability test modules

Move tests for each LSP method family into one sibling
module per family. Each capability's request-builder
helper(s) (e.g. `hover_request`, `completion_request`) move
into the module that uses it. Each sibling module declares
`use super::helpers::*;` to reach the shared helpers.

Sibling modules to create under `tests/lsp_lifecycle/`:

- [ ] `lifecycle.rs` — initialize/shutdown tests
- [ ] `document_management.rs` — didOpen / didChange /
      didClose tests
- [ ] `hover.rs` — hover tests + `hover_request` helper
- [ ] `completion.rs` — completion tests +
      `completion_request` helper
- [ ] `folding_ranges.rs` — folding range tests +
      `folding_range_request` helper
- [ ] `navigation.rs` — definition, references, and
      document_symbols tests + their request helpers
- [ ] `rename.rs` — prepare-rename and rename tests + their
      request helpers
- [ ] `document_links.rs` — document link tests +
      `document_link_request` helper
- [ ] `selection_ranges.rs` — selection range tests +
      `selection_range_request` helper
- [ ] `code_actions.rs` — code action tests + the two
      code-action request helpers
- [ ] `code_lens.rs` — code lens tests +
      `code_lens_request` helper
- [ ] `on_type_formatting.rs` — on-type formatting tests +
      its request helper
- [ ] `semantic_tokens.rs` — semantic tokens tests +
      `semantic_tokens_request` helper

Acceptance:

- [ ] Each module above exists in
      `rlsp-yaml/tests/lsp_lifecycle/` with the tests and
      helper(s) listed
- [ ] `tests/lsp_lifecycle/main.rs` declares each module via
      `mod <name>;` and no longer contains any of the moved
      tests or helpers
- [ ] `cargo test --test lsp_lifecycle` reports the same
      118-test total as the Task 1 baseline
- [ ] `cargo clippy --all-targets -- -D warnings` passes

### Task 4: Extract configuration and schema-routing modules

Move tests for configuration changes, file watching, custom
tag annotations, and schema modeline / glob / Kubernetes
auto-detection.

- [ ] `configuration.rs` — `did_change_configuration` tests
      + `did_change_configuration_notification` helper
- [ ] `watched_files.rs` — `did_change_watched_files` tests
      + `did_change_watched_files_notification` helper
- [ ] `custom_tags.rs` — `custom tags with type annotations`
      tests + `initialize_request_with_custom_tags` helper
- [ ] `schema_modelines.rs` — `$schema=none modeline`,
      `glob-based schema association fallback`,
      `hover with schema association via modeline`, and
      `completion with schema association via modeline`
      tests + `initialize_request_with_schema_glob` helper
- [ ] `kubernetes_detection.rs` — Kubernetes auto-detection
      tests + `initialize_request_with_k8s_version` helper

Acceptance:

- [ ] Each module above exists with the tests and helpers
      listed
- [ ] `tests/lsp_lifecycle/main.rs` declares each module and
      no longer contains any of the moved tests or helpers
- [ ] `cargo test --test lsp_lifecycle` test count remains
      118
- [ ] `cargo clippy --all-targets -- -D warnings` passes

### Task 5: Extract validators integration module

Move every test under the `Validator Integration Tests`,
`key ordering validation path`, and `schema-aware YAML 1.1
diagnostics (integration)` sections (lines 701 through 3043
in the original file) into
`tests/lsp_lifecycle/validators_integration.rs`. These tests
exercise the validator pipeline through the LSP diagnostics
flow and are large enough to warrant a dedicated module.

- [ ] `validators_integration.rs` exists and contains every
      test originally located under the three section
      headers listed above
- [ ] `tests/lsp_lifecycle/main.rs` declares
      `mod validators_integration;` and no longer contains
      any of those tests
- [ ] `cargo test --test lsp_lifecycle` test count remains
      118
- [ ] `cargo clippy --all-targets -- -D warnings` passes

### Task 6: Extract formatting integration module

Move the formatting-settings integration tests (`flowStyle
setting`, `duplicateKeys setting`, `formatEnforceBlockStyle
setting`, `formatPreserveQuotes setting`, `formatEnable
setting`) into `tests/lsp_lifecycle/formatting_integration.rs`.

- [ ] `formatting_integration.rs` exists and contains every
      test under the five formatting-settings section
      headers listed above
- [ ] `tests/lsp_lifecycle/main.rs` declares
      `mod formatting_integration;` and no longer contains
      any of those tests
- [ ] `cargo test --test lsp_lifecycle` test count remains
      118
- [ ] `cargo clippy --all-targets -- -D warnings` passes

### Task 7: Verify orchestration-only `main.rs`

After all extractions, `tests/lsp_lifecycle/main.rs` should
contain only crate-level attributes, a module-level doc
comment, and `mod <name>;` declarations — no
`#[tokio::test]` attributes and no `fn` items other than
the module declarations.

- [ ] `tests/lsp_lifecycle/main.rs` contains zero
      `#[tokio::test]` or `#[test]` attributes
- [ ] `tests/lsp_lifecycle/main.rs` contains zero `fn`
      definitions other than module declarations
- [ ] Every sibling `.rs` file in `tests/lsp_lifecycle/`
      corresponds to a `mod <name>;` declaration in
      `main.rs`, and every `mod <name>;` declaration
      corresponds to an existing sibling file
- [ ] `cargo test --test lsp_lifecycle` reports 118 tests,
      matching the Task 1 baseline; record the final count
      and the baseline count in the commit message
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Decisions

- **Layout pattern:** mirror
  `rlsp-yaml-parser/tests/smoke/main.rs` —
  `tests/lsp_lifecycle/main.rs` is the Cargo-detected test
  binary entry, and sibling `.rs` files declared via `mod`
  provide the tests.
- **Helper sharing:** the seven LSP request helpers live in
  `helpers.rs`. Each sibling module reaches them via
  `use super::helpers::*;`. Per-capability request helpers
  travel with the module that uses them, since each is
  referenced by only one test group.
- **Grouping granularity:** one sibling module per LSP
  method family. The validators-integration and
  formatting-integration sections each get their own module
  because each contains substantially more tests than the
  per-method modules and serves a distinct cross-cutting
  concern (diagnostics pipeline vs. formatting settings).
- **Atomic file rename:** Task 1 is a pure `git mv` so the
  rename is recorded distinctly in history before any
  content is moved. Cargo cannot tolerate both
  `tests/lsp_lifecycle.rs` and `tests/lsp_lifecycle/main.rs`
  simultaneously, so every later task starts from the
  already-migrated path.

## Non-Goals

- Modifying test assertions, request payloads, or LSP
  server behavior. This plan only reorganizes test file
  layout.
- Refactoring shared helpers beyond moving them into
  `helpers.rs`.
- Adding new tests, parametrizing existing ones, or
  consolidating duplicate setup logic.
- Changing any other test file
  (`tests/corpus_invariants.rs`,
  `tests/parser_boundary_audit.rs`,
  `tests/code_action_fixtures.rs`,
  `tests/formatter_fixtures.rs`,
  `tests/rename_fixtures.rs`) — those are covered by
  separate plans or out of scope.
