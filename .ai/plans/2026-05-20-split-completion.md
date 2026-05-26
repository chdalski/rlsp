**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-20

# Split `src/completion.rs` into per-stage submodules

## Goal

The 3302-line `rlsp-yaml/src/completion.rs` implements the
LSP `textDocument/completion` handler end-to-end: cursor
location detection, per-context dispatch (key vs. value
vs. sequence-item), AST navigation, schema-driven item
generation, label/description formatting, and ~150 unit
tests in a single `mod tests` block. Reorganize the file
using the project's `foo.rs` + adjacent `foo/` directory
convention so each pipeline stage lives in its own
submodule alongside its dedicated unit tests, while
`completion.rs` keeps the `complete_at` public entry
point as the orchestrator that wires the stages together.

## Context

- **Module-layout convention in this repo:** `foo.rs` plus
  adjacent `foo/` directory (Rust 2018+ style, no
  `mod.rs`).
- **Source-of-truth file:**
  `rlsp-yaml/src/completion.rs` (3302 lines).
- **Public surface (must remain reachable at the existing
  paths):**
  - `pub fn complete_at` (line 34) — only public symbol
- **External callers (paths that must keep working):**
  - `src/server.rs` line 865 — uses
    `crate::completion::complete_at`
  - `tests/corpus_invariants.rs` line 40 — uses
    `rlsp_yaml::completion::complete_at`
- **Pipeline stages and their items:**
  - Cursor location detection (lines 603–987):
    `enum CursorLocation<'a>` (line 603, 6 variants),
    `fn span_contains_cursor` (line 658),
    `const fn node_span` (line 672),
    `const fn scalar_key` (line 683),
    `const fn lsp_position_to_pos` (line 692),
    `fn deepest_mapping_at_column` (line 708),
    `fn cursor_line_has_mapping_content` (line 765,
    with nested `fn node_has_content_on_line`),
    `fn locate_cursor` (line 804),
    `fn locate_in_node` (line 877).
  - Navigation helpers (lines 989–1066):
    `fn find_node_at_path`,
    `fn present_keys`,
    `fn collect_sibling_keys_ast`,
    `fn collect_sequence_sibling_keys`.
  - Per-context completion drivers (lines 102–214):
    `fn complete_on_key`,
    `fn complete_on_value`,
    `fn complete_in_sequence_item`.
  - Completion-item building (lines 216–314):
    `fn keys_to_items`,
    `fn collect_values_for_key_ast`,
    `fn collect_values_in_node`,
    `fn merge_completions`.
  - Schema-driven completion (lines 315–544):
    `fn resolve_schema_path`,
    `fn schema_has_properties`,
    `fn schema_key_completions`,
    `fn snippet_default`,
    `fn collect_schema_properties`,
    `fn collect_schema_properties_keys`,
    `fn collect_schema_properties_keys_inner`,
    `fn schema_value_completions`.
  - Label/description formatting (lines 545–602):
    `fn json_value_to_yaml_label`,
    `fn type_label`,
    `fn truncate_description`,
    `fn truncate_enum_label`.
  - Constants (lines 17–23): `MAX_COMPLETION_ITEMS`,
    `MAX_BRANCH_COUNT`, `MAX_DESCRIPTION_LEN`,
    `MAX_ENUM_LABEL_LEN`.
- **Unit tests (`mod tests` block, lines 1067–3302,
  ~150 tests using rstest parametrization):** the block
  is organized by completion feature. The dossier
  identifies these contiguous test groupings:
  - Structural completion (AST-based) tests
    (lines 1143–1251, ~3 tests) → exercise `complete_at`
    end-to-end with no schema → stay with the public
    entry in `completion.rs`
  - Edge cases — empty docs / empty AST (lines 1259–1275,
    ~2 tests) → stay with `complete_at`
  - Schema property completion at key position
    (lines 1275–1360, ~5 tests) → schema_completions
  - Enum completion at value position
    (lines 1452–1532, ~5 tests) → schema_completions
  - Type-aware completion (lines 1547–1595, ~2 tests) →
    schema_completions
  - Schema composition (allOf, anyOf)
    (lines 1595–1660, ~3 tests) → schema_completions
  - Path and blank-line handling
    (lines 1711–1794, ~3 tests) → cursor_location
  - Truncation tests (lines 1792–1875, ~3 tests) →
    formatting
  - Performance bounds (item cap, branch cap, enum
    truncation) (lines 1844–1875, ~2 tests) →
    schema_completions
  - Document separation (lines 1983–2040, ~3 tests) →
    stay with `complete_at`
  - Edge cases on unusual cursor lines
    (lines 2040–2089, ~4 tests) → cursor_location
  - Deprecated property tagging
    (lines 2089–2194, ~3 tests) → schema_completions
  - Snippet completion for required properties
    (lines 2205–2381, ~5 tests) → schema_completions
  - Multiline / indentation
    (lines 2396–2424, ~2 tests) → completion_drivers
- **Test helpers inside `mod tests`
  (lines 1076–1143):**
  `fn pos(line: u32, character: u32) -> Position`,
  `fn labels(items: &[CompletionItem]) -> Vec<&str>`,
  `string_schema`, `integer_schema`, `boolean_schema`,
  `object_schema`, `sibling_key_suggests_and_excludes`.
  These fixture builders are reused across nearly every
  test group; in the new layout they go into a
  `support::test_fixtures` module reachable as `use
  super::support::test_fixtures::*;` from each
  submodule's `mod tests` block.
- **Test colocation rule (from the user):** every `mod
  tests` unit-test block must live in the same file as
  the function(s) it exercises. The single monolithic
  `mod tests` block is split per-stage during extraction.
- **Test routing rule (from the lsp_lifecycle split
  retrospective):** when extracting tests, decide each
  test's destination by what its body asserts — not by
  its name and not by its position in the file. The
  Context section's mapping of test groupings to stages
  is a starting point; if a test exercises a specific
  stage helper directly (e.g. calls `locate_cursor`,
  `schema_key_completions`, or `merge_completions`),
  route it to that stage's submodule even if its name or
  position suggests another. Do not leave a stray test
  stranded in `completion.rs` just because its name
  doesn't match a stage.
- **Cross-module visibility:** `complete_at` calls
  `locate_cursor`, then dispatches to `complete_on_key`,
  `complete_on_value`, or `complete_in_sequence_item`,
  which in turn call into the schema and item-building
  helpers. Helpers move with `pub(super)` visibility so
  siblings can reach them; the only public symbol
  remains `complete_at`.
- **Build/test commands (from CLAUDE.md):** `cargo build`,
  `cargo test`, `cargo clippy --all-targets`, `cargo fmt`.

## Steps

- [x] Extract `formatting` and `support` (constants +
      test fixtures)
- [x] Extract `cursor_location`
- [x] Extract `navigation`
- [x] Extract `completion_items` and `completion_drivers`
- [ ] Extract `schema_completions`
- [ ] Verify `completion.rs` is orchestration only and
      every external caller continues to compile
      unchanged

## Tasks

### Task 1: Extract `formatting` and `support`

Create `src/completion/` and add two leaf submodules.
`formatting` holds the label/description helpers (no
internal dependencies beyond `serde_json` and
`JsonSchema`). `support` holds the four crate-internal
constants and the test fixtures that every later test
module needs.

- [x] `src/completion/formatting.rs` exists and contains:
  - `pub(super) fn json_value_to_yaml_label`
  - `pub(super) fn type_label`
  - `pub(super) fn truncate_description`
  - `pub(super) fn truncate_enum_label`
  - a `#[cfg(test)] mod tests` block holding the
    truncation tests (lines 1792–1875 of the original
    `mod tests` block, ~3 tests)
- [x] `src/completion/support.rs` exists and contains:
  - `pub(super) const MAX_COMPLETION_ITEMS`,
    `MAX_BRANCH_COUNT`, `MAX_DESCRIPTION_LEN`,
    `MAX_ENUM_LABEL_LEN`
  - a nested `pub(super) mod test_fixtures` exposing
    `pos`, `labels`, `string_schema`, `integer_schema`,
    `boolean_schema`, `object_schema`,
    `sibling_key_suggests_and_excludes`
- [x] `src/completion.rs` declares `mod formatting;` and
      `mod support;`
- [x] `src/completion.rs` no longer defines the moved
      constants, functions, or unit tests
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` reports the same total test count as
      the pre-task baseline; record both numbers in the
      commit message
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Notes (from the developer/test-engineer collaboration):
- `sibling_key_suggests_and_excludes` is an rstest-
  parametrized **test function** (not a fixture builder).
  Per the plan's test-routing rule it routes to the
  orchestrator-level tests and stays in `completion.rs`.
- The truncation-tests group at lines 1792–1875 calls
  `complete_at` end-to-end and asserts on its output,
  not on the helpers directly — those tests stay with
  `complete_at` for now (Task 5 will route the
  performance-bounds subset to `schema_completions`).
  The developer added 19 new direct unit tests for the
  four formatting helpers in `formatting.rs`, raising
  the workspace test count from **6219** to **6238**.

Commit: `6211a2c` (amended; see `git log --follow rlsp-yaml/src/completion/support.rs`)

### Task 2: Extract `cursor_location`

The cursor-location subsystem is the largest single
concern (~450 lines) and detects where the cursor sits
in the AST. It is invoked once at the top of
`complete_at`.

- [x] `src/completion/cursor_location.rs` exists and
      contains:
  - `pub(super) enum CursorLocation<'a>`
  - `pub(super) fn span_contains_cursor`
  - `pub(super) const fn node_span`
  - `pub(super) const fn scalar_key`
  - `pub(super) const fn lsp_position_to_pos`
  - `pub(super) fn deepest_mapping_at_column`
  - `pub(super) fn cursor_line_has_mapping_content`
    (with its nested helper)
  - `pub(super) fn locate_cursor`
  - `pub(super) fn locate_in_node`
  - a `#[cfg(test)] mod tests` block holding the
    path-and-blank-line tests (lines 1711–1794, ~3 tests)
    and the unusual-cursor-line tests (lines 2040–2089,
    ~4 tests) from the original `mod tests` block
- [x] `src/completion.rs` declares `mod cursor_location;`
      and calls `cursor_location::locate_cursor` from
      `complete_at`
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Notes (test-engineer / reviewer observations):
- 10 baseline `locate_cursor*` tests + 7 new direct unit
  tests for `node_span`, `scalar_key`, `span_contains_cursor`,
  `cursor_line_has_mapping_content`, and `locate_in_node`
  routed to `cursor_location.rs`. Workspace test count
  rises from **6238** to **6245**.
- Navigation tests (`present_keys_*`,
  `collect_sibling_keys_ast_*`,
  `collect_sequence_sibling_keys_*` — 6 tests) discovered
  in the original `mod tests` block during routing. They
  stay in `completion.rs` for now, colocated with their
  helpers, and will follow the helpers to `navigation.rs`
  in Task 3. This invalidates Task 3's plan-text claim
  that navigation has no `mod tests` block — see Task 3
  notes.

Commit: `f129efc` (amended; see `git log --follow rlsp-yaml/src/completion/cursor_location.rs`)

### Task 3: Extract `navigation`

The original `mod tests` block contains no `#[test]`
function that invokes `find_node_at_path`, `present_keys`,
`collect_sibling_keys_ast`, or `collect_sequence_sibling_keys`
directly. Coverage of these helpers is provided end-to-end
through the structural-completion and document-separation
tests that exercise `complete_at`. The new file therefore
contains no `#[cfg(test)] mod tests` block.

- [x] `src/completion/navigation.rs` exists and contains
      exactly:
  - `pub(super) fn find_node_at_path`
  - `pub(super) fn present_keys`
  - `pub(super) fn collect_sibling_keys_ast`
  - `pub(super) fn collect_sequence_sibling_keys`
  - **Plan-text override (Task 2 finding):**
    `navigation.rs` DOES have a `#[cfg(test)] mod tests`
    block — Task 2's routing discovered 6 navigation
    tests in the original `mod tests` that exercise
    these helpers directly. They move with the helpers.
- [x] `src/completion.rs` declares `mod navigation;` and
      routes its existing calls through the submodule
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Notes (test-engineer / reviewer observations):
- 6 baseline navigation tests moved + 7 new direct unit
  cases for `find_node_at_path` (4 None-returning + 3
  Some-returning, using a NodeKind enum per the
  enums-over-booleans guideline). Workspace test count
  rises from **6245** to **6252**.

Commit: `d82a552` (amended; see `git log --follow rlsp-yaml/src/completion/navigation.rs`)

### Task 4: Extract `completion_items` and `completion_drivers`

The per-context dispatchers (`complete_on_key`,
`complete_on_value`, `complete_in_sequence_item`) and the
item-building helpers move together; the drivers are the
sole callers of the item helpers.

The original `mod tests` block contains no `#[test]`
function that invokes `keys_to_items`,
`collect_values_for_key_ast`, `collect_values_in_node`, or
`merge_completions` directly — these are exercised through
`complete_at` tests. `completion_items.rs` therefore
contains no `#[cfg(test)] mod tests` block. The
multiline/indentation tests at lines 2396–2424 of the
original `mod tests` block do exercise the per-context
drivers directly and move into `completion_drivers.rs`.

- [x] `src/completion/completion_items.rs` exists and
      contains exactly:
  - `pub(super) fn keys_to_items`
  - `pub(super) fn collect_values_for_key_ast`
  - `pub(super) fn collect_values_in_node`
  - `pub(super) fn merge_completions`
  - **Plan-text override (test-engineer recommendation):**
    `completion_items.rs` ships with 13 new direct unit
    tests covering `keys_to_items`, `merge_completions`,
    and `collect_values_for_key_ast`. The original plan
    said "no `mod tests`" because no baseline tests
    target these directly; the test-engineer recommended
    adding direct coverage anyway (same lens as Task 1).
- [x] `src/completion/completion_drivers.rs` exists and
      contains:
  - `pub(super) fn complete_on_key`
  - `pub(super) fn complete_on_value`
  - `pub(super) fn complete_in_sequence_item`
  - a `#[cfg(test)] mod tests` block holding 6 moved
    sequence-context tests + 10 new direct unit tests
    covering each driver with schema/no-schema paths.
- [x] `src/completion.rs` declares `mod completion_items;`
      and `mod completion_drivers;` and routes its
      existing calls through these submodules
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Notes (test-engineer / reviewer observations):
- Workspace test count rises from **6252** to **6275**
  (+23: 13 in `completion_items`, 10 in `completion_drivers`).
- 5 schema helpers in `completion.rs` widened to
  `pub(super)` so `completion_drivers` can reach them
  ahead of Task 5 (Task 5 will move them out entirely).
- Unused `LineIndex` and `Node` imports cleaned up in
  `completion.rs`.

Commit: `145d77d` (amended; see `git log --follow rlsp-yaml/src/completion/completion_drivers.rs`)

### Task 5: Extract `schema_completions`

- [ ] `src/completion/schema_completions.rs` exists and
      contains:
  - `pub(super) fn resolve_schema_path`
  - `pub(super) fn schema_has_properties`
  - `pub(super) fn schema_key_completions`
  - `pub(super) fn snippet_default`
  - `pub(super) fn collect_schema_properties`
  - `pub(super) fn collect_schema_properties_keys`
  - `fn collect_schema_properties_keys_inner` (private
    to this submodule)
  - `pub(super) fn schema_value_completions`
  - a `#[cfg(test)] mod tests` block holding the
    schema-property-at-key-position tests
    (lines 1275–1360, ~5), enum-completion-at-value-position
    tests (lines 1452–1532, ~5), type-aware tests
    (lines 1547–1595, ~2), composition tests
    (lines 1595–1660, ~3), deprecated-property-tagging
    tests (lines 2089–2194, ~3), snippet-completion
    tests (lines 2205–2381, ~5), and performance-bounds
    tests (lines 1844–1875, ~2) — all from the original
    `mod tests` block
- [ ] `src/completion.rs` declares `mod
      schema_completions;` and routes its existing calls
      through the submodule
- [ ] `cargo build` succeeds without new warnings
- [ ] `cargo test` total test count matches the previous
      task's baseline
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes

### Task 6: Verify orchestration-only `completion.rs`

This task is primarily a verification of the post-Task-5
state. If Tasks 1–5 already produced an orchestration-only
parent file, Task 6 will have no source diff — submit a
verification-only handoff documenting the measured criteria
(grep/ls/cargo command outputs) and the plan-progress
update only. If verification reveals any leftover stage
helpers, missing module declarations, or stranded unit
tests in `completion.rs` beyond the public orchestrator,
fix them and report what changed.

After all extractions, `src/completion.rs` contains only:

- a module-level doc comment
- the seven module declarations: `mod formatting; mod
  support; mod cursor_location; mod navigation; mod
  completion_items; mod completion_drivers; mod
  schema_completions;`
- `pub fn complete_at` (the public entry that calls
  `cursor_location::locate_cursor` and dispatches into
  `completion_drivers::complete_on_*`)
- a `#[cfg(test)] mod tests` block holding only the
  orchestrator-level tests: structural completion
  (lines 1143–1251, ~3), empty-document edge cases
  (lines 1259–1275, ~2), and document-separation tests
  (lines 1983–2040, ~3) — all from the original `mod
  tests` block

No stage helpers, no per-stage unit tests, no `enum` or
`struct` items remain in `completion.rs`.

- [ ] `src/completion.rs` contains exactly one `pub fn`
      item (`complete_at`), seven `mod` declarations, and
      one `#[cfg(test)] mod tests` block; nothing else at
      the item level
- [ ] Every sibling `.rs` file under `src/completion/`
      corresponds to a `mod <name>;` declaration in
      `completion.rs`, and every declaration corresponds
      to an existing sibling file
- [ ] `cargo build` succeeds without new warnings
- [ ] `cargo test` reports the same total test count as
      the pre-Task-1 baseline; record both numbers in the
      commit message
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] Neither external caller listed in Context was
      modified (`git diff --stat` shows only
      `completion.rs` and new submodule files under
      `completion/`)

## Decisions

- **Module-layout convention:** `completion.rs` becomes
  the module-entry file alongside a new `src/completion/`
  directory containing the submodules. Matches
  `src/schema_validation.rs` + `src/schema_validation/`.
- **Single public symbol preserved:** `complete_at` stays
  defined in `completion.rs` and remains reachable at
  `crate::completion::complete_at` and
  `rlsp_yaml::completion::complete_at`. Only two external
  call sites exist.
- **Stage-based slicing:** the completion handler is a
  pipeline (cursor detection → context dispatch →
  item generation → label formatting). Each stage becomes
  one submodule; the stage's tests live with the stage's
  code.
- **`complete_at` stays in the parent:** it is the
  pipeline orchestrator. Orchestrator-level tests
  (structural completion end-to-end, empty-AST edge
  cases, document-separation guarantees) stay with the
  orchestrator because they exercise the wire-up rather
  than any single stage.
- **`support` for constants and shared test fixtures:**
  the four crate-internal limit constants
  (`MAX_COMPLETION_ITEMS`, etc.) live in `support`. The
  test fixtures (`pos`, `labels`, `*_schema`,
  `sibling_key_suggests_and_excludes`) live in a nested
  `pub(super) mod test_fixtures` inside `support` so
  every per-stage `mod tests` reaches them via
  `use super::support::test_fixtures::*;`.
- **`pub(super)` visibility for internal helpers:** all
  internal helpers move with `pub(super)` so the parent
  (`complete_at`) and siblings can call them, but they
  are not part of the crate-public API.
- **Test colocation:** every per-stage `mod tests` block
  sits at the bottom of its own submodule, holding the
  tests for that stage's behavior. Snippet/composition/
  deprecated-property tests cluster with
  `schema_completions` because they all exercise
  schema-driven item generation. Three submodules
  (`support`, `navigation`, `completion_items`) have no
  dedicated `mod tests` block because no test in the
  original `mod tests` block targets their functions
  directly; coverage flows through `complete_at`
  end-to-end tests in the parent file.
- **Caller-path reference in Context may shift:** the
  Context section names `tests/corpus_invariants.rs line
  40` as an external caller. The sibling plan
  `2026-05-20-split-corpus-invariants-tests.md` renames
  that file into a directory with submodules; if it runs
  before this plan, the `use
  rlsp_yaml::completion::complete_at` statement will have
  moved to a submodule under `tests/corpus_invariants/`
  by execution time. The acceptance criterion that
  neither external caller was modified still holds
  because the public API path
  (`rlsp_yaml::completion::complete_at`) does not change.
- **`rlsp-yaml/README.md` is not updated:** the README's
  "Architecture" section is a conceptual module map, not
  a literal file tree. The `completion` module continues
  to deliver the completion provider through its
  re-exported `complete_at`; the README description
  describes that behavior, not the file's literal
  contents, and remains accurate.

## Non-Goals

- Changing completion behavior, item ranking, label
  text, or schema-driven generation logic.
- Modifying `JsonSchema` or any schema representation in
  `src/schema.rs`.
- Adding new completion features (cross-document
  references, snippet improvements, etc.).
- Tuning the limit constants (`MAX_COMPLETION_ITEMS`
  etc.).
- Splitting any other source file
  (`src/validation/validators.rs`,
  `src/editing/formatter.rs`,
  `src/schema_validation.rs`, etc.) — covered by separate
  plans.
- Modifying external callers in `src/server.rs` or
  `tests/corpus_invariants.rs`.
