**Repository:** root
**Status:** InProgress
**Created:** 2026-05-20

# Split `src/schema_validation.rs` into per-constraint submodules

## Goal

The 6331-line `rlsp-yaml/src/schema_validation.rs` is the
largest file in the workspace. It implements the entire
JSON-Schema-against-YAML validation pipeline — type
checking, scalar/string/numeric constraints, mapping
constraints (required, additional, pattern, dependencies),
array constraints (items, contains, unevaluated),
composition (allOf/anyOf/oneOf), the validation context,
the regex cache, the YAML→JSON shim, and ~100 unit tests
in a single `mod tests` block. Reorganize the file using
the project's `foo.rs` + adjacent `foo/` directory
convention so each constraint family lives in its own
submodule alongside its dedicated unit tests, while
`schema_validation.rs` keeps the `validate_schema` public
entry point and the `validate_node` recursive dispatcher
that wires the submodules together. The existing
`src/schema_validation/formats.rs` submodule is left
unchanged.

## Context

- **Module-layout convention in this repo:** `foo.rs` plus
  adjacent `foo/` directory. The `src/schema_validation/`
  directory already exists and already contains
  `formats.rs`; this plan adds further submodules
  alongside it.
- **Source-of-truth file:**
  `rlsp-yaml/src/schema_validation.rs` (6331 lines).
- **Public surface (must remain reachable at the existing
  paths):**
  - `pub fn validate_schema` (line 225) — only public
    symbol in the file
- **External callers (paths that must keep working):**
  - `src/server.rs` line 397 — uses
    `crate::schema_validation::validate_schema`
  - `tests/corpus_invariants.rs` line 46 — uses
    `rlsp_yaml::schema_validation::validate_schema`
- **Internal validation context and shared state:**
  - `struct Ctx<'a>` (line 191) and its `new` constructor
    (line 199) — passed as `&mut Ctx<'_>` to every
    validator
  - `REGEX_CACHE` thread-local (line 56),
    `fn get_regex` (line 65), and the constants
    `MAX_PATTERN_LEN`, `REGEX_SIZE_LIMIT`,
    `MAX_VALIDATION_DEPTH` (line 85),
    `MAX_BRANCH_COUNT` (line 88), `MAX_DESCRIPTION_LEN`
    (line 91), `MAX_ENUM_DISPLAY` (line 94)
  - Generic helpers (lines 22–183): `node_loc`,
    `entries_contains_key`, `node_key_str`,
    `collect_evaluated_properties`,
    `collect_evaluated_item_count`
  - Type-matching helpers (lines 1520–1571):
    `yaml_type_name`, `type_matches`,
    `single_type_or_contains`, `single_type_matches`,
    `display_schema_type`
  - Diagnostic builders (lines 1573–1650): `yaml_to_json`,
    `make_diagnostic`, `truncate_message`, `format_path`
- **Constraint families and their items:**
  - Type checking: `effective_yaml_type` (line 255),
    `validate_type` (line 281), `type_mismatch_diagnostic`
    (line 335), `emit_yaml11_string_warnings` (line 377).
  - Scalar / string / numeric: `validate_scalar_constraints`
    (line 770), `validate_string_constraints` (line 825),
    `validate_format` (line 920), `validate_content`
    (line 970), `validate_content_schema` (line 1046),
    `validate_numeric_constraints` (line 1089).
  - Mapping: `validate_unevaluated_properties` (line 492),
    `validate_mapping_constraints` (line 669),
    `validate_dependencies` (line 1304),
    `validate_pattern_properties` (line 1356),
    `validate_mapping` (line 1198).
  - Array: `validate_unevaluated_items` (line 530),
    `validate_sequence` (line 551),
    `validate_array_constraints` (line 602),
    `validate_contains` (line 713).
  - Composition: `validate_composition` (line 1406,
    handles allOf/anyOf/oneOf).
  - Dispatcher: `validate_node` (line 416, recursive
    entry into every constraint family),
    `pub fn validate_schema` (line 225, public entry that
    sets up `Ctx` and walks each document).
- **`formats.rs` submodule (line 19 — `mod formats;`):**
  already a separate file at `src/schema_validation/
  formats.rs`. Used by `validate_format`. Unchanged by
  this plan.
- **Unit tests (`mod tests` block, lines 1651–6331,
  ~100 tests using rstest parametrization):** the block
  is organized by constraint family with `// ══════`
  divider comments. The dossier identifies these
  contiguous test sections:
  - Required properties (lines 1703–1772, ~6 tests) →
    mapping_constraints
  - Type mismatch (lines 1823–1879, ~9 tests) →
    type_validation
  - Enum validation (lines 1913–1958, ~7 tests) →
    mapping_constraints
  - Additional properties (lines 1971–2033, ~6 tests) →
    mapping_constraints
  - Composition (lines 2053–2201, ~6 tests) → composition
  - Recursive validation (lines 2201–2283, ~5 tests) →
    array_constraints (covers nested array items)
  - Stack overflow protection (line 2282–2309, 1 test) →
    dispatcher (tests `MAX_VALIDATION_DEPTH`)
  - Diagnostic properties (source, code, severity)
    (lines 2309–2430, ~7 tests) → dispatcher
  - Message content (lines 2430–2815, ~4 tests) →
    dispatcher
  - Edge cases (empty docs, no constraints, parse errors)
    (lines 2472–2536, ~4 tests) → dispatcher
  - Performance bounds (lines 2551–2688, ~5 tests) →
    dispatcher
  - Cache poison handling (line 2723–2749, 1 test) →
    support (regex cache)
  - Pattern validation (lines 2814–2888, ~3 tests) →
    scalar_constraints (validate_string_constraints)
  - String constraints (lines 2888–2951, ~2 tests) →
    scalar_constraints
  - Numeric constraints (lines 2951–3159, ~8 tests) →
    scalar_constraints (validate_numeric_constraints)
  - Const validation (lines 3159–3184, ~2 tests) →
    scalar_constraints
- **Test helpers inside `mod tests` (lines 1662–1701):**
  `string_schema`, `integer_schema`, `boolean_schema`,
  `object_schema_with_props`, `code_of`. These fixture
  builders are reused across nearly every test group; in
  the new layout they go into a `support::test_fixtures`
  module reachable as `use super::support::test_fixtures::*;`
  from each submodule's `mod tests` block.
- **Test colocation rule (from the user):** every `mod
  tests` unit-test block must live in the same file as
  the function(s) it exercises. The single monolithic
  `mod tests` block is split per-constraint-family during
  extraction.
- **Test routing rule (from the lsp_lifecycle split
  retrospective):** when extracting tests, decide each
  test's destination by what its body asserts — not by
  its name, not by its position in the file, and not by
  the `// ══════` divider comments alone. The Context
  section's mapping of test sections to submodules is a
  starting point; if a test exercises a specific
  validator (e.g. `validate_string_constraints`), route
  it to that validator's submodule even if it sits in a
  section labeled differently. Do not leave a stray test
  stranded in `schema_validation.rs` just because its
  name or position doesn't match a constraint family.
- **Cross-module visibility:** sibling modules call each
  other through the dispatcher (`validate_node`) and
  occasionally directly (e.g.
  `validate_array_constraints` calls
  `validate_contains`). Helpers move with `pub(super)`
  visibility so siblings can reach them; `Ctx<'_>` and
  the type-matching helpers become `pub(super)` exports
  from the support modules.
- **Build/test commands (from CLAUDE.md):** `cargo build`,
  `cargo test`, `cargo clippy --all-targets`, `cargo fmt`.

## Steps

- [x] Extract `context` and `support`
- [x] Extract `type_validation`
- [x] Extract `composition`
- [ ] Extract `array_constraints`
- [ ] Extract `scalar_constraints`
- [ ] Extract `mapping_constraints`
- [ ] Verify `schema_validation.rs` is dispatcher-only and
      every external caller continues to compile unchanged

## Tasks

### Task 1: Extract `context` and `support`

Create two foundational submodules used by every
constraint family. `context` holds the validation state;
`support` holds the regex cache, generic helpers,
type-matching utilities, diagnostic builders, and the
test fixtures shared by every constraint-family test
block.

- [x] `src/schema_validation/context.rs` exists and
      contains:
  - `pub(super) struct Ctx<'a>` and `impl Ctx<'_>` (with
    `pub(super)` constructor)
- [x] `src/schema_validation/support.rs` exists and
      contains:
  - `pub(super) const MAX_PATTERN_LEN`,
    `REGEX_SIZE_LIMIT`, `MAX_VALIDATION_DEPTH`,
    `MAX_BRANCH_COUNT`, `MAX_DESCRIPTION_LEN`,
    `MAX_ENUM_DISPLAY`
  - `REGEX_CACHE` thread-local (kept private to this
    module)
  - `pub(super) fn get_regex`,
    `pub(super) fn node_loc`,
    `pub(super) fn entries_contains_key`,
    `pub(super) fn node_key_str`,
    `pub(super) fn collect_evaluated_properties`,
    `pub(super) fn collect_evaluated_item_count`,
    `pub(super) fn yaml_type_name`,
    `pub(super) fn type_matches`,
    `pub(super) fn single_type_or_contains`,
    `pub(super) fn single_type_matches`,
    `pub(super) fn display_schema_type`,
    `pub(super) fn yaml_to_json`,
    `pub(super) fn make_diagnostic`,
    `pub(super) fn truncate_message`,
    `pub(super) fn format_path`
  - a nested `pub(super) mod test_fixtures` exposing
    `string_schema`, `integer_schema`, `boolean_schema`,
    `object_schema_with_props`, `code_of` (these are
    only callable from sibling test modules)
  - a `#[cfg(test)] mod tests` block holding the
    cache-poison-handling unit test (lines 2723–2749 of
    the original `mod tests` block)
- [x] `src/schema_validation.rs` declares `mod context;`
      and `mod support;`
- [x] `src/schema_validation.rs` no longer defines the
      moved constants, statics, structs, functions, or
      unit tests
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` reports the same total test count as
      the pre-task baseline; record both numbers in the
      commit message
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `89b9d40` (amended; see `git log --follow rlsp-yaml/src/schema_validation/context.rs`)

### Task 2: Extract `type_validation`

- [x] `src/schema_validation/type_validation.rs` exists
      and contains:
  - `pub(super) fn effective_yaml_type`
  - `pub(super) fn validate_type`
  - `pub(super) fn type_mismatch_diagnostic`
  - `pub(super) fn emit_yaml11_string_warnings`
  - imports from sibling modules (`use super::context::Ctx;`,
    `use super::support::*;`)
  - a `#[cfg(test)] mod tests` block holding the
    type-mismatch tests (lines 1823–1879 of the original
    `mod tests` block, ~9 tests)
- [x] `src/schema_validation.rs` declares `mod
      type_validation;` and routes its existing calls
      through the submodule (e.g., `validate_node` now
      calls `type_validation::validate_type`)
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `40a0faa` (amended; see `git log --follow rlsp-yaml/src/schema_validation/type_validation.rs`)

### Task 3: Extract `composition`

- [x] `src/schema_validation/composition.rs` exists and
      contains:
  - `pub(super) fn validate_composition` (the
    allOf/anyOf/oneOf implementation)
  - imports from sibling modules as needed
  - a `#[cfg(test)] mod tests` block holding the
    composition tests (lines 2053–2201 of the original
    `mod tests` block, ~6 tests)
- [x] `src/schema_validation.rs` declares `mod
      composition;` and `validate_node` now calls
      `composition::validate_composition`
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `c9f9981` (amended; see `git log --follow rlsp-yaml/src/schema_validation/composition.rs`)

### Task 4: Extract `array_constraints`

- [ ] `src/schema_validation/array_constraints.rs` exists
      and contains:
  - `pub(super) fn validate_sequence`
  - `pub(super) fn validate_array_constraints`
  - `pub(super) fn validate_unevaluated_items`
  - `pub(super) fn validate_contains`
  - imports from sibling modules as needed
  - a `#[cfg(test)] mod tests` block holding the
    recursive-validation tests (lines 2201–2283 of the
    original `mod tests` block, ~5 tests covering nested
    array items)
- [ ] `src/schema_validation.rs` declares `mod
      array_constraints;` and routes its existing calls
      through the submodule
- [ ] `cargo build` succeeds without new warnings
- [ ] `cargo test` total test count matches the previous
      task's baseline
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes

### Task 5: Extract `scalar_constraints`

- [ ] `src/schema_validation/scalar_constraints.rs` exists
      and contains:
  - `pub(super) fn validate_scalar_constraints`
  - `pub(super) fn validate_string_constraints`
  - `pub(super) fn validate_format` (which delegates to
    the existing `formats` submodule)
  - `pub(super) fn validate_content`
  - `pub(super) fn validate_content_schema`
  - `pub(super) fn validate_numeric_constraints`
  - imports from sibling modules as needed
  - a `#[cfg(test)] mod tests` block holding the
    pattern-validation tests (lines 2814–2888, ~3),
    string-constraints tests (lines 2888–2951, ~2),
    numeric-constraints tests (lines 2951–3159, ~8), and
    const-validation tests (lines 3159–3184, ~2) — all
    from the original `mod tests` block
- [ ] `src/schema_validation.rs` declares `mod
      scalar_constraints;` and routes its existing calls
      through the submodule
- [ ] `cargo build` succeeds without new warnings
- [ ] `cargo test` total test count matches the previous
      task's baseline
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes

### Task 6: Extract `mapping_constraints`

- [ ] `src/schema_validation/mapping_constraints.rs`
      exists and contains:
  - `pub(super) fn validate_mapping`
  - `pub(super) fn validate_mapping_constraints`
  - `pub(super) fn validate_dependencies`
  - `pub(super) fn validate_pattern_properties`
  - `pub(super) fn validate_unevaluated_properties`
  - imports from sibling modules as needed
  - a `#[cfg(test)] mod tests` block holding the
    required-properties tests (lines 1703–1772, ~6),
    enum-validation tests (lines 1913–1958, ~7), and
    additional-properties tests (lines 1971–2033, ~6) —
    all from the original `mod tests` block
- [ ] `src/schema_validation.rs` declares `mod
      mapping_constraints;` and routes its existing calls
      through the submodule
- [ ] `cargo build` succeeds without new warnings
- [ ] `cargo test` total test count matches the previous
      task's baseline
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes

### Task 7: Verify dispatcher-only `schema_validation.rs`

This task is primarily a verification of the post-Task-6
state. If Tasks 1–6 already produced a dispatcher-only
parent file, Task 7 will have no source diff — submit a
verification-only handoff documenting the measured criteria
(grep/ls/cargo command outputs) and the plan-progress
update only. If verification reveals any leftover internal
helpers, missing module declarations, or stranded unit
tests in `schema_validation.rs` beyond the dispatcher
itself, fix them and report what changed.

After all extractions, `src/schema_validation.rs` contains
only:

- a module-level doc comment
- `mod formats;` (pre-existing) and the seven new module
  declarations: `mod context; mod support; mod
  type_validation; mod composition; mod array_constraints;
  mod scalar_constraints; mod mapping_constraints;`
- `pub fn validate_schema` (the public entry that sets up
  `Ctx` and walks each document — calls into
  `validate_node`)
- `fn validate_node` (the recursive dispatcher — calls
  into the submodules)
- a `#[cfg(test)] mod tests` block holding only the
  dispatcher-level tests: stack-overflow protection
  (line 2282–2309), diagnostic properties
  (lines 2309–2430), message content (lines 2430–2815),
  edge cases (lines 2472–2536), and performance bounds
  (lines 2551–2688) — all from the original `mod tests`
  block

No constraint-family helpers, no per-constraint unit
tests, no thread-locals, no `struct`/`enum` items remain
in `schema_validation.rs`.

- [ ] `src/schema_validation.rs` contains exactly one
      `pub fn` item (`validate_schema`), one private `fn`
      item (`validate_node`), eight `mod` declarations,
      and one `#[cfg(test)] mod tests` block; nothing
      else at the item level
- [ ] Every sibling `.rs` file under
      `src/schema_validation/` corresponds to a `mod
      <name>;` declaration in `schema_validation.rs`, and
      every declaration corresponds to an existing
      sibling file (including the pre-existing
      `formats.rs`)
- [ ] `cargo build` succeeds without new warnings
- [ ] `cargo test` reports the same total test count as
      the pre-Task-1 baseline; record both numbers in the
      commit message
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] Neither external caller listed in Context was
      modified (`git diff --stat` shows only
      `schema_validation.rs` and new submodule files
      under `schema_validation/`)

## Decisions

- **Module-layout convention:** `schema_validation.rs`
  stays as the module-entry file. The existing
  `src/schema_validation/` directory (already holding
  `formats.rs`) grows additional sibling modules. No
  Cargo manifest changes are needed.
- **Single public symbol preserved:** `validate_schema`
  stays defined in `schema_validation.rs` and remains
  reachable at `crate::schema_validation::validate_schema`
  and `rlsp_yaml::schema_validation::validate_schema`.
  Only two external call sites exist, both at fully
  qualified paths.
- **Constraint-family slicing:** the file's helpers
  cluster by JSON Schema constraint family
  (type / scalar+string+numeric / mapping / array /
  composition). Each family becomes one submodule; the
  family's tests live with the family's code.
- **Dispatcher stays in the parent:** `validate_node` is
  the recursive entry point that dispatches by AST node
  kind into the appropriate constraint family. It is the
  natural orchestrator and stays alongside
  `validate_schema`. Dispatcher-level tests
  (depth limit, branch limit, diagnostic source/code,
  empty-doc edge cases) stay with the dispatcher because
  they exercise the orchestration itself.
- **Shared state and helpers in `context` and `support`:**
  `Ctx<'_>` lives in its own small module (it is the
  state thread, used by every validator). The wide
  collection of generic helpers (regex cache, constants,
  type-matching, diagnostic builders, YAML→JSON shim,
  evaluated-property tracking) lives in `support` so each
  constraint-family submodule reaches them via a single
  `use super::support::*;`. The test fixtures
  (`string_schema`, etc.) live in a nested
  `pub(super) mod test_fixtures` inside `support` so
  every constraint-family `mod tests` reaches them the
  same way.
- **`pub(super)` visibility for internal helpers:** all
  internal helpers move with `pub(super)` so siblings
  (notably `validate_node` in the parent) can call them,
  but they are not part of the crate-public API. The
  public surface stays exactly one symbol:
  `validate_schema`.
- **Test colocation:** every per-constraint `mod tests`
  block sits at the bottom of its own submodule, holding
  exactly the tests for that constraint family.
- **`formats.rs` left as-is:** the existing format
  submodule is unaffected.
- **Caller-path reference in Context may shift:** the
  Context section names `tests/corpus_invariants.rs line
  46` as an external caller. The sibling plan
  `2026-05-20-split-corpus-invariants-tests.md` renames
  that file into a directory with submodules; if it runs
  before this plan, the `use
  rlsp_yaml::schema_validation::validate_schema`
  statement will have moved to a submodule under
  `tests/corpus_invariants/` by execution time. The
  acceptance criterion that neither external caller was
  modified still holds because the public API path
  (`rlsp_yaml::schema_validation::validate_schema`) does
  not change.
- **`rlsp-yaml/README.md` is not updated:** the README's
  "Architecture" section is a conceptual module map, not
  a literal file tree. The `schema_validation` module
  continues to deliver schema-driven diagnostics through
  its re-exported public symbol; the README description
  describes that behavior, not the file's literal
  contents, and remains accurate.

## Non-Goals

- Changing validator behavior, diagnostic codes,
  diagnostic severity, or error messages.
- Modifying `JsonSchema`, `SchemaType`, or any schema
  representation in `src/schema.rs`.
- Modifying `src/schema_validation/formats.rs` or its
  format predicates.
- Adding new constraint validations, changing the regex
  cache implementation, or tuning constants
  (`MAX_VALIDATION_DEPTH` etc.).
- Splitting any other source file
  (`src/validation/validators.rs`,
  `src/editing/formatter.rs`, `src/completion.rs`, etc.)
  — covered by separate plans.
- Modifying external callers in `src/server.rs` or
  `tests/corpus_invariants.rs`.
