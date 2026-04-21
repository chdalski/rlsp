**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-21

## Goal

Repository-wide cleanup: reduce file sizes, eliminate
cross-module test helper duplication, parameterize
repetitive test groups, and split oversized files where
natural module boundaries exist. Zero cross-module
test-helper duplication must remain. Parameterize tests
where patterns genuinely repeat; split files where there
is a structural reason (separable concerns, readability,
independent testability).

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
- [x] Split `code_actions.rs` into per-action submodules
- [x] Parameterize and reduce tests in `schema_validation.rs`
- [x] Parameterize `lsp_lifecycle.rs` test groups with rstest
      (investigated — no groups are genuinely parameterizable)
- [x] Reduce file sizes and test ratios across remaining
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

**Commit:** `30df41b`

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

- [x] Create `editing/code_actions/` directory
- [x] Extract `flow_to_block.rs` (flow_map_to_block +
      flow_seq_to_block + tests)
- [x] Extract `block_to_flow.rs` (block_to_flow +
      helpers + tests)
- [x] Extract `tab_to_spaces.rs` + tests
- [x] Extract `delete_anchor.rs` + tests
- [x] Extract `quoted_bool.rs` + tests
- [x] Extract `block_scalar.rs` + tests
- [x] Extract `yaml11_bool.rs` (both yaml11 + schema
      variant) + tests
- [x] Extract `yaml11_octal.rs` + tests
- [x] Parent `code_actions.rs` retains entry point +
      shared helpers only
- [x] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

Acceptance criteria:
- Parent `code_actions.rs` ≤ 200 lines
- No submodule file exceeds 800 lines
- All tests pass, zero clippy warnings
- `code_actions()` public API unchanged (same function
  signature, same behavior)

**Commit:** `c9e02db`

### Task 3: Parameterize `schema_validation.rs` tests

`schema_validation.rs` is 6162 lines with a 2.7:1
test/production ratio. The explore analysis found 199
standalone `#[test]` functions alongside 21 existing rstest
groups — 50+ type-checking test cases follow repetitive
patterns that can collapse into parameterized groups.

- [x] Identify standalone `#[test]` functions that follow
      repetitive patterns (same assertion shape, differing
      only in input schema + YAML + expected diagnostic)
- [x] Collapse each group into `#[rstest]` with
      `#[case::name]` named cases
- [x] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

**Commit:** `3219493`

Acceptance criteria:
- Repetitive test patterns consolidated into rstest groups
  where the pattern genuinely repeats (same assertion
  shape, differing only in inputs)
- All parameterized groups use `#[case::descriptive_name]`
- All tests pass, zero clippy warnings
- No production code changes

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

- [x] Investigate all 4 groups for parameterization
- [x] Result: no changes — all 4 groups have heterogeneous
      request signatures, assertion shapes, or setup
      sequences that prevent clean rstest parameterization

**No commit — no code changes.**

### Task 5: Batch file size and test ratio reduction

Parameterize repetitive test patterns across remaining
files where patterns genuinely repeat. Split files only
where a natural module boundary exists (separable
concerns, readability, independent testability) — not to
hit a line count.

**Files with repetitive test patterns to parameterize:**

| File | Current ratio | Action |
|------|--------------|--------|
| `completion.rs` (3279, 2.2:1) | Parameterize where patterns repeat. Extract cursor helpers to `completion/cursor.rs` if a natural module boundary exists. |
| `schema.rs` (3282, 1.4:1) | Consolidate 8+ type-mismatch test variants into rstest. |
| `validators.rs` (2073, 2.3:1) | Parameterize repetitive validator test patterns. |
| `hover.rs` (1829, 3.6:1) | Parameterize similar hover-content tests. |
| `document_links.rs` (1057, 4.4:1) | Parameterize URL/include test groups. |
| `selection.rs` (781, 2.7:1) | Parameterize range assertion patterns. |
| `symbols.rs` (642, 2.6:1) | Parameterize symbol assertion patterns. |
| `rename.rs` (608, 2.3:1) | Parameterize rename scenario variants. |
| `server.rs` (2482, 0.8:1) | No parameterization needed — ratio is healthy. |
| `formatter.rs` (2525, 0.5:1) | No parameterization needed — ratio is healthy. |

- [x] Parameterize repetitive tests in each file listed
      above where patterns genuinely repeat
      (schema.rs: 19 URL tests → 2 rstest groups + 4
      duplicates removed. Other 7 files: no genuine
      repetition found.)
- [x] `cargo test` passes, `cargo clippy --all-targets`
      zero warnings

Acceptance criteria:
- Repetitive test patterns consolidated into rstest groups
  where the pattern genuinely repeats
- All parameterized groups use `#[case::descriptive_name]`
- Splits only where structurally justified (natural module
  boundary, separable concerns, readability)
- All tests pass, zero clippy warnings
- No production code changes, no public API changes

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
- **No mechanical splits** — fixed line counts and ratios
  are diagnostic signals ("this file is large, investigate")
  not prescriptive rules ("this file is large, split it").
  Splits must have a structural reason: natural module
  boundary, separable concerns, readability. Tests belong
  with the code they test — do not extract test modules
  into submodule files solely to reduce a parent file's
  line count.
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
