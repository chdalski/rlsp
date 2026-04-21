**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-21

## Goal

Repository-wide cleanup: reduce file sizes, eliminate
cross-module test helper duplication, parameterize
repetitive test groups, and split oversized files. Every
source file in `rlsp-yaml/src/` must be ≤1500 lines
(production code only — extracted test submodules are
exempt), the test/production ratio must be ≤2:1 per
module, and zero cross-module test-helper duplication
must remain. `lsp_lifecycle.rs` must drop by ≥450 lines
via rstest parameterization.

## Context

The "one parser, one AST" retrofit program completed on
2026-04-21 (commit `a1d88d2`). Twelve feature-level
retrofits across six plans rewrote every LSP feature to
consume the parser AST instead of scanning raw text. Each
plan was scoped to preserve behavior and pass review — none
was scoped to address the accumulated test cruft, duplicated
helpers, or file-size growth across the program. This plan
is the post-program consolidation.

**Current file sizes (lines) and test/production ratios
(retrofitted + non-retrofitted):**

| File | Total | Prod | Test | Ratio |
|------|-------|------|------|-------|
| `schema_validation.rs` | 6162 | 1679 | 4483 | 2.7:1 |
| `code_actions.rs` | 5076 | 1382 | 3694 | 2.7:1 |
| `lsp_lifecycle.rs` (tests/) | 3655 | — | 3655 | test file |
| `schema.rs` | 3282 | 1390 | 1892 | 1.4:1 |
| `completion.rs` | 3279 | 1033 | 2246 | 2.2:1 |
| `formatter.rs` | 2525 | 1677 | 848 | 0.5:1 |
| `server.rs` | 2482 | 1409 | 1073 | 0.8:1 |
| `validators.rs` | 2073 | 637 | 1436 | 2.3:1 |
| `hover.rs` | 1829 | 397 | 1432 | 3.6:1 |
| `document_links.rs` | 1057 | 197 | 860 | 4.4:1 |
| `selection.rs` | 781 | 212 | 569 | 2.7:1 |
| `symbols.rs` | 642 | 179 | 463 | 2.6:1 |
| `rename.rs` | 608 | 185 | 423 | 2.3:1 |

**Cross-module test helper duplication:** 14 source files
each define their own `parse_docs` / `docs_for` / `parse`
helper (identical 2-3 line function calling
`rlsp_yaml_parser::load()`). Three files duplicate
`test_uri()`.

**`lsp_lifecycle.rs` repetitive groups:** ~110 tests with
4 parameterizable groups — "unknown doc returns null" (13
tests), diagnostic severity toggles (~7), max-items-computed
(3), feature-disable toggles (~8).

**Dead production code:** clippy `--all-targets` is clean
(zero warnings). No orphaned public functions found — all
`pub fn` / `pub(crate) fn` in production code have non-test
call sites.

**References:**
- Module style: `lang-rust.md` — use `<module>.rs` files,
  not `mod.rs` in `src/`. Exception: `mod.rs` acceptable
  in `tests/`.
- Test parameterization: `lang-rust-testing.md` — use
  `#[case::name]` syntax for named rstest cases.
- `CLAUDE.md` Crate Boundaries — settings sync table for
  formatter changes.

## Steps

- [x] Extract shared test helpers into a crate-wide
      `#[cfg(test)]` module
- [ ] Split `code_actions.rs` into per-action submodules
- [ ] Parameterize and reduce tests in `schema_validation.rs`
- [ ] Parameterize `lsp_lifecycle.rs` test groups with rstest
- [ ] Reduce file sizes and test ratios across remaining
      files

## Tasks

### Task 1: Shared test helper consolidation

Eliminate cross-module test helper duplication by extracting
common helpers into a single `#[cfg(test)]` module.

Create `rlsp-yaml/src/test_utils.rs` with `#[cfg(test)]`
gating:

```rust
#[cfg(test)]
pub(crate) mod test_utils {
    pub fn parse_docs(text: &str) -> Vec<Document<Span>> { ... }
    pub fn test_uri() -> Url { ... }
}
```

Declare it in `lib.rs` with `#[cfg(test)] mod test_utils;`.

Update all 14 files that define their own `parse_docs` /
`docs_for` / `parse` variant to import from `test_utils`
instead:
- `completion.rs` (`parse`)
- `hover.rs` (`parse_docs`)
- `validators.rs` (`parse_docs`)
- `schema_validation.rs` (`parse_docs`)
- `schema/association.rs` (`parse_docs`)
- `schema_validation/formats.rs` (`parse_docs`)
- `decorators/color.rs` (`parse_docs`)
- `decorators/document_links.rs` (`parse_docs`)
- `analysis/semantic_tokens.rs` (`parse_docs`)
- `analysis/selection.rs` (`parse_docs`)
- `analysis/symbols.rs` (`parse_docs`)
- `navigation/references.rs` (`parse`)
- `navigation/rename.rs` (`parse`)
- `editing/on_type_formatting.rs` (`parse_docs`)
- `editing/code_actions.rs` (`docs_for`)

Update the 3 files that duplicate `test_uri()`:
- `editing/code_actions.rs`
- `navigation/references.rs`
- `navigation/rename.rs`

**Note:** `selection.rs` and `symbols.rs` return
`Option<Vec<Document<Span>>>` — their helpers wrap `load()`
differently. If the wrapper differs from the shared version,
keep a local adapter that calls the shared helper, do not
force all callers into the same return type.

- [x] Verify starting-point duplication count: grep the
      codebase for all definitions of `parse_docs`,
      `docs_for`, `parse`, and `test_uri` in `#[cfg(test)]`
      blocks and confirm the count matches the 14+3 list
      above. Update the list if it differs.
- [x] Create `rlsp-yaml/src/test_utils.rs` with
      `parse_docs` and `test_uri`
- [x] Declare `#[cfg(test)] mod test_utils` in `lib.rs`
- [x] Replace all 14 local `parse_docs` variants with
      imports from `test_utils`
- [x] Replace all 3 local `test_uri` definitions with
      imports from `test_utils`
- [x] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

Acceptance criteria:
- Zero duplicate `parse_docs` / `docs_for` / `parse`
  definitions across the crate (grep confirms single
  definition in `test_utils.rs`)
- Zero duplicate `test_uri` definitions (grep confirms
  single definition)
- All tests pass, zero clippy warnings

### Task 2: Split `code_actions.rs` into per-action modules

`code_actions.rs` is 5076 lines — the largest file touched
by the retrofit program. The production code has clear
per-action boundaries. Split into submodules under
`editing/code_actions/`.

Create directory `rlsp-yaml/src/editing/code_actions/` with
the following structure. Per `lang-rust.md`, use
`<module>.rs` not `mod.rs` in `src/` — but for `code_actions`
this means converting from `code_actions.rs` to
`code_actions.rs` + `code_actions/` directory (Rust 2018
path attribute style):

```
editing/
  code_actions.rs          — pub fn code_actions() entry point +
                             shared helpers (diagnostic_code,
                             ranges_overlap, make_action)
  code_actions/
    flow_to_block.rs       — flow_map_to_block, flow_seq_to_block + tests
    block_to_flow.rs       — block_to_flow, block_text_and_start_col,
                             has_nested_collection_child + tests
    tab_to_spaces.rs       — tab_to_spaces + tests
    delete_anchor.rs       — delete_unused_anchor + tests
    quoted_bool.rs         — quoted_bool_to_unquoted, find_quoted_bool_* + tests
    block_scalar.rs        — string_to_block_scalar, find_block_scalar_* + tests
    yaml11_bool.rs         — yaml11_bool_actions, schema_yaml11_bool_type_actions + tests
    yaml11_octal.rs        — yaml11_octal_actions + tests
```

Each submodule uses `pub(super)` visibility for functions
called from the parent entry point. Tests move with their
production code.

Shared test helpers (`cursor_range`, `line_range`,
`make_flow_diag`, `make_diagnostic`) stay in the parent
module's test block or move to `test_utils.rs` if used
across submodules. `docs_for` is already handled by Task 1.

- [ ] Create `editing/code_actions/` directory
- [ ] Extract `flow_to_block.rs` (flow_map_to_block +
      flow_seq_to_block + tests)
- [ ] Extract `block_to_flow.rs` (block_to_flow +
      helpers + tests)
- [ ] Extract `tab_to_spaces.rs` + tests
- [ ] Extract `delete_anchor.rs` + tests
- [ ] Extract `quoted_bool.rs` + tests
- [ ] Extract `block_scalar.rs` + tests
- [ ] Extract `yaml11_bool.rs` (both yaml11 + schema
      variant) + tests
- [ ] Extract `yaml11_octal.rs` + tests
- [ ] Parent `code_actions.rs` retains entry point +
      shared helpers only
- [ ] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

Acceptance criteria:
- Parent `code_actions.rs` ≤ 200 lines
- No submodule file exceeds 800 lines
- All tests pass, zero clippy warnings
- `code_actions()` public API unchanged (same function
  signature, same behavior)

### Task 3: Parameterize `schema_validation.rs` tests

`schema_validation.rs` is 6162 lines with a 2.7:1
test/production ratio. The explore analysis found 199
standalone `#[test]` functions alongside 21 existing rstest
groups — 50+ type-checking test cases follow repetitive
patterns that can collapse into parameterized groups.

- [ ] Identify standalone `#[test]` functions that follow
      repetitive patterns (same assertion shape, differing
      only in input schema + YAML + expected diagnostic)
- [ ] Collapse each group into `#[rstest]` with
      `#[case::name]` named cases
- [ ] Extract the test module to
      `schema_validation/tests.rs` submodule to bring the
      parent file under 1500 lines
- [ ] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

Acceptance criteria:
- `schema_validation.rs` (production code) ≤ 1700 lines
  (production code is 1679 — no production changes needed)
- `schema_validation/tests.rs` test/production ratio
  ≤ 2:1 (≤ 3358 test lines, down from 4483 — reduce
  by ≥ 1125 lines through parameterization)
- All tests pass, zero clippy warnings

### Task 4: Parameterize `lsp_lifecycle.rs` with rstest

`lsp_lifecycle.rs` is 3655 lines with 4 repetitive test
groups that can collapse into rstest parameterized
functions.

**Group 1: "Unknown doc returns null" (13 tests)**
Tests: `should_return_null_hover_for_unknown_document`,
`should_return_empty_completion_for_unknown_document`, etc.
All follow the same pattern: send request for unknown URI,
assert null/empty response. Collapse into 1 `#[rstest]`
with 13 `#[case::name]` entries.

**Group 2: Diagnostic severity toggles (~7 tests)**
Tests: `flow_style_*`, `duplicate_keys_*`. Each varies a
settings value and asserts diagnostic severity. Collapse
into 1-2 `#[rstest]` functions.

**Group 3: Max items computed (3 tests)**
Tests: `document_symbols_respects_max_items_computed`,
`folding_ranges_respects_max_items_computed`, etc. Same
pattern with different LSP methods. Collapse into 1
`#[rstest]`.

**Group 4: Feature disable toggles (~8 tests)**
Tests toggling features on/off via settings. Same
assertion pattern. Collapse into 1-2 `#[rstest]` functions.

- [ ] Add `rstest` to `[dev-dependencies]` if not already
      present
- [ ] Parameterize Group 1 (unknown doc → null)
- [ ] Parameterize Group 2 (severity toggles)
- [ ] Parameterize Group 3 (max items computed)
- [ ] Parameterize Group 4 (feature disable toggles)
- [ ] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

Acceptance criteria:
- `lsp_lifecycle.rs` ≤ 3200 lines (down from 3655 —
  reduce by ≥ 455 lines)
- All named cases use `#[case::descriptive_name]` syntax
- All tests pass, zero clippy warnings

### Task 5: Batch file size and test ratio reduction

Bring all remaining source files under 1500 lines and
test/production ratio ≤ 2:1. For files where production
code is already ≤1500 lines, the fix is extracting the
inline `#[cfg(test)] mod tests` to a submodule file
(`<module>/tests.rs`). For files where test ratios
exceed 2:1, parameterize repetitive test patterns before
extracting.

**Files requiring test submodule extraction (prod ≤ 1500,
total > 1500):**

| File | Action |
|------|--------|
| `completion.rs` (3279) | Extract cursor helpers (lines 646-1031) to `completion/cursor.rs`. Extract tests to `completion/tests.rs`. |
| `schema.rs` (3282) | Consolidate 8+ type-mismatch test variants into rstest. Extract tests to `schema/tests.rs`. |
| `server.rs` (2482) | Extract tests to `server/tests.rs`. |
| `validators.rs` (2073) | Parameterize repetitive validator test patterns. Extract tests to `validation/validators/tests.rs`. |
| `hover.rs` (1829) | Parameterize similar hover-content tests (ratio 3.6:1 → ≤ 2:1). Extract tests to `hover/tests.rs`. |

**Files requiring test ratio reduction only (total ≤ 1500
but ratio > 2:1):**

| File | Current ratio | Action |
|------|--------------|--------|
| `document_links.rs` | 4.4:1 | Parameterize URL/include test groups |
| `selection.rs` | 2.7:1 | Parameterize range assertion patterns |
| `symbols.rs` | 2.6:1 | Parameterize symbol assertion patterns |
| `rename.rs` | 2.3:1 | Parameterize rename scenario variants |

**File requiring production code split (prod > 1500):**

| File | Action |
|------|--------|
| `formatter.rs` (2525, prod 1677) | Extract a natural submodule (e.g., format-options types or a helper group) to bring prod ≤ 1500. Extract tests to `editing/formatter/tests.rs`. |

- [ ] Extract `completion/cursor.rs` submodule and
      `completion/tests.rs` test submodule
- [ ] Extract `schema/tests.rs` with rstest consolidation
- [ ] Extract `server/tests.rs` test submodule
- [ ] Parameterize + extract `validators/tests.rs`
- [ ] Parameterize + extract `hover/tests.rs`
- [ ] Parameterize tests in `document_links.rs`,
      `selection.rs`, `symbols.rs`, `rename.rs`
- [ ] Split `formatter.rs` production code and extract
      `formatter/tests.rs`. If `YamlFormatOptions` moves
      to a submodule, update its path reference in
      `rlsp-yaml/tests/fixtures/CLAUDE.md` and in the
      root `CLAUDE.md` Settings Sync table.
- [ ] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

Acceptance criteria:
- Every source file in `rlsp-yaml/src/` ≤ 1500 lines
  of production code (extracted `<module>/tests.rs`
  submodules are exempt from the line-count target)
- Test/production ratio ≤ 2:1 per module, measured as
  (test lines in `<module>/tests.rs` or inline test
  block) / (production lines in the parent module file)
- `formatter.rs` production code ≤ 1700 lines (see
  Decisions for rationale)
- Any moved type's references in `.md` files are updated
  (specifically `YamlFormatOptions` path in
  `rlsp-yaml/tests/fixtures/CLAUDE.md` and root
  `CLAUDE.md` Settings Sync table)
- All tests pass, zero clippy warnings
- No public API changes (same function signatures, same
  behavior)

## Non-Goals

- Changing production behavior — this is pure refactoring,
  test restructuring, and file organization
- Adding new tests — parameterization consolidates
  existing tests, it does not expand coverage
- Feature-log cleanup — tracked as a separate follow-up
  item

## Decisions

- **Repository-wide scope assessment:** all three crates
  were assessed for the cleanup targets (file size, test
  ratio, helper duplication, dead code):
  - `rlsp-yaml-parser` — largest file is `flow.rs` at
    1663 lines, all production code (no inline tests). No
    files exceed 1500 lines of production code. No
    cross-module test helper duplication (grep for
    `parse_docs`/`docs_for`/`parse` found zero matches
    in the parser crate). No cleanup action needed.
  - `rlsp-fmt` — largest file is `printer.rs` at 371
    lines. All files under 400 lines. No cleanup action
    needed.
  - `rlsp-yaml` — all targets violated. This plan's tasks
    address all `rlsp-yaml` findings.
- **Dead code removal investigated and found clean:**
  `cargo clippy --all-targets` across all three crates
  produces zero warnings. Systematic search for `pub fn`
  and `pub(crate) fn` definitions outside `#[cfg(test)]`
  confirmed every public function has non-test call sites.
  No dead production code exists — no removal needed.
- **Shared test helpers in `src/test_utils.rs`** rather
  than `tests/common/mod.rs` — the duplicated helpers are
  used inside `#[cfg(test)]` inline modules in source
  files, which need crate-internal access. A `tests/`
  helper would only be accessible to integration tests.
- **Per-action modules for code_actions** — each code
  action is an independent unit with its own tests. The
  dispatch entry point is thin (75 lines) and naturally
  belongs in the parent module. This matches the existing
  `validation/` and `schema_validation/` directory patterns.
- **Test submodule extraction pattern** — for files where
  production code is ≤1500 but total exceeds 1500, extract
  `#[cfg(test)] mod tests` to `<module>/tests.rs`. This
  is mechanical, preserves private access via `use
  super::*`, and follows Rust 2018 module conventions.
- **Test ratio measured per module** — the 2:1 ratio
  target is measured per original module: production lines
  in the parent file, test lines in the inline block or
  extracted `tests.rs`. Extracting tests to a submodule
  does not satisfy the ratio target — parameterization
  and pruning must reduce the test volume first if the
  ratio exceeds 2:1.
- **formatter.rs production tolerance** — at 1677 lines of
  production code, `formatter.rs` is 177 lines over the
  1500 guideline. Extract a natural submodule if a clean
  boundary exists; accept ≤1700 if no natural boundary is
  found. The ≤1700 limit is an explicit carve-out, not an
  escape hatch — the acceptance criterion for formatter.rs
  in Task 5 is ≤1700 (not ≤1500).
- **`lsp_lifecycle.rs` targeted parameterization** — the
  completed `2026-04-12-yaml-rstest-parameterization.md`
  plan excluded `lsp_lifecycle.rs` as a whole, citing
  "async/fixture-specific setup that dominates over
  assertion pattern." That exclusion applies to the
  majority of the file's 110 tests, which have
  heterogeneous setup and assertion shapes. Task 4
  targets only 4 structurally uniform groups (Groups 1–4
  named in the task description) where the tests share
  identical setup, assertion pattern, and return type —
  differing only in the LSP method called or the setting
  value. The remaining tests are not candidates for
  parameterization in this plan.
- **Baseline measurements** taken at commit `a1d88d2`
  (2026-04-21). File sizes verified via `wc -l`; test
  helper duplication verified via `grep` across
  `rlsp-yaml/src/`. All completed prior plans
  (split-large-modules, rstest-parameterization,
  module-grouping) are reflected in the current state —
  `schema_validation.rs` is still 6162 lines (the
  split-large-modules plan created `formats.rs` as a
  submodule but did not further split the main file).
