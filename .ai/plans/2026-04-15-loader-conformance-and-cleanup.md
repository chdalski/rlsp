**Repository:** root
**Status:** InProgress
**Created:** 2026-04-15

## Goal

Achieve 100% yaml-test-suite conformance for both the
loader (`load()` API) and the formatter's round-trip test
(`parse → format → reparse`), with every bug fixed in the
crate that owns it. The loader has untested bugs that
produce incorrect ASTs from correct event streams — the
formatter cannot produce correct output from a wrong AST.
This plan establishes loader conformance testing, fixes
loader bugs in the parser crate, removes formatter
workarounds that compensated for loader bugs, and drives
formatter KNOWN_FAILURES to 0.

Acceptance criteria:
1. Loader conformance test exists and runs against the
   full yaml-test-suite
2. All loader bugs are fixed — 0 loader KNOWN_FAILURES,
   constant deleted
3. Formatter workarounds for loader bugs are removed
4. All formatter bugs are fixed — 0 formatter
   KNOWN_FAILURES, constant deleted
5. Document marker flags (`explicit_start`, `explicit_end`)
   surfaced in AST

## Context

### Current state

The formatter's round-trip conformance test has 40
KNOWN_FAILURES remaining (down from 79). The formatter
correctly handles: escape sequences, block scalar
indentation indicators, multiline quoted scalars, explicit
key syntax, anchor/tag placement on block collections,
empty-key mappings, and multiline plain scalars.

### The root cause

The formatter conformance test (`formatter_conformance.rs`)
checks round-trip correctness: `parse → format → reparse`.
When this fails, it could mean either:
- The formatter emits invalid YAML (formatter bug)
- The loader produces an incorrect AST (loader bug)

Without a loader conformance test, these were conflated.
~13 of the remaining 40 KNOWN_FAILURES are confirmed loader
bugs where the AST doesn't match the event tree. The
formatter can't produce correct output from a wrong AST.

### Known loader bugs

Document tags and inline scalars:
- 2XXW, 35KP: document tags (`--- !!set`, `--- !!map`)
  loaded as scalar keys instead of mapping tag field
- J7PZ: `!!omap` tag garbled into scalar value

Explicit keys and empty keys:
- M2N8[0]: nested explicit key produces garbled mapping
- 6PBE: sequence-as-key produces wrong entry count
- KK5P: nested explicit keys produce spurious entries
- S3PD: empty key with comment absorbs next entry
- NKF9: empty-key entries have key/value swapped

Anchors and aliases:
- E76Z: alias-as-key mapping misloaded
- PW8X: explicit key + anchor combination
- FTA2: document start marker + anchor
- FH7J: value-tag `!!str` handling
- 6M2F: explicit block mapping with aliases

### Missing AST info (not a bug — info not surfaced)

- `DocumentStart.explicit` — whether `---` was present
  (event carries it, loader discards it)
- `DocumentEnd.explicit` — whether `...` was present
  (event carries it, loader discards it)

These block formatter handling of comment-only documents,
multi-document streams, and bare `...` markers.

### Formatter workaround to remove

Commit `25b1130` added a quote-char guard in the Plain
scalar branch of `node_to_doc`: `value.starts_with('"') ||
value.starts_with('\'')`. A conformant loader never
produces a Plain scalar starting with a quote character.
This guard works around a loader bug where `--- "quoted"`
inline scalars are garbled. Remove after loader is fixed.

### Current KNOWN_FAILURES (40 entries)

26DV, 2G84[2], 2G84[3], 2XXW, 35KP, 6CA3, 6M2F, 6PBE,
6WLZ, 6XDY, 8KB6, 98YD, 9BXH, C4HZ, DK95[7], E76Z,
FH7J, FTA2, HWV9, J7PZ, JHB9, KK5P, L383, M2N8[0],
MUS6[2-6], NKF9, PW8X, Q5MG, QT73, RZP5, S3PD, T26H,
UGM3, UKK6[2], W4TN, XW4D

### Key files

- `rlsp-yaml-parser/tests/conformance.rs` — current stream
  conformance test (to be restructured)
- `rlsp-yaml-parser/src/loader.rs` — loader implementation
- `rlsp-yaml-parser/src/node.rs` — AST types
- `rlsp-yaml-parser/src/event.rs` — event types
- `rlsp-yaml/src/editing/formatter.rs` — formatter
- `rlsp-yaml/tests/formatter_conformance.rs` — formatter
  conformance test (KNOWN_FAILURES)

### References

- YAML 1.2 §3.1 — Processes (serialization, presentation)
- YAML 1.2 §9.1 — Document markers
- Crate Boundaries in root `CLAUDE.md`

## Steps

- [x] Restructure conformance tests into module
- [x] Add loader conformance test against yaml-test-suite
- [x] Surface document marker flags in AST
- [x] Fix identified loader bugs (document tags, explicit
      keys, empty keys, anchors)
- [ ] Fix all remaining loader KNOWN_FAILURES
- [ ] Remove formatter quote-char workaround
- [ ] Fix all remaining formatter KNOWN_FAILURES
- [ ] Add interacting-settings fixture combinations
- [ ] Final verification — 0 KNOWN_FAILURES in both
      loader and formatter

## Tasks

### Task 1: Restructure conformance tests into module — 3ca38ea

Move the conformance test from a standalone file into a
module structure that supports both stream and loader
testing.

- [x] Create `rlsp-yaml-parser/tests/conformance/` directory
- [x] Move `conformance.rs` → `conformance/stream.rs`
- [x] Add `conformance/main.rs` with shared helpers
      (`visual_to_raw`, case parsing, etc.)
- [x] Verify existing stream conformance still passes
      (351/351)
- [x] Check if `smoke/conformance.rs` has cases that should
      be in the conformance module
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 2: Add loader conformance test — e66ddad

Add a loader conformance test that runs `load()` against
the yaml-test-suite and verifies AST correctness by
comparing the AST structure against the expected event tree
from each test case.

- [x] Create `conformance/loader.rs`
- [x] For each non-fail case: `load(input)` must succeed
- [x] Verify AST structure matches expected event tree
      (correct number of documents, correct node types,
      correct scalar values, correct styles, correct
      anchors/tags)
- [x] Populate KNOWN_FAILURES allowlist for loader
- [x] Measure baseline: 351 cases, 155 known failures
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 3: Surface document marker flags in AST — c3216d7

Add `explicit_start: bool` and `explicit_end: bool` to
the `Document` struct and populate from events.

- [x] Add fields to `Document` in `node.rs`
- [x] Populate from `DocumentStart { explicit, .. }` in
      loader
- [x] Populate from `DocumentEnd { explicit, .. }` in
      loader
- [x] Update formatter to use flags: emit `---` only when
      `explicit_start` is true, emit `...` when
      `explicit_end` is true
- [x] Update `Document<Span>` struct diagram in
      `rlsp-yaml-parser/docs/architecture.md` to include
      `explicit_start` and `explicit_end`
- [x] `cargo test`, `cargo clippy --all-targets` pass
- [x] Parser conformance remains 351/351

### Task 4: Fix document tags and inline scalars — ac07843

Fix lexer handling of inline content after `---` starting
with tags or quotes — these were mis-scanned as plain
scalars by `scan_plain_line_block`.

- [x] `--- !!set` / `--- !!map` — fixed (2XXW, 35KP)
- [x] `--- !!omap` — fixed (J7PZ)
- [x] `--- "quoted"` inline scalars — fixed (C4HZ)
- [x] Remove KNOWN_FAILURES entries for fixed cases
- [x] `cargo test`, `cargo clippy --all-targets` pass
- [x] Loader conformance: 2XXW, 35KP, J7PZ + 6LVF, 9MQT
      now pass (155 → 150)

### Task 5: Fix loader bugs — explicit keys and empty keys

Fix loader handling of complex mapping structures.

- [ ] Nested explicit keys (`? ? :`) producing spurious
      entries
- [ ] Sequence-as-key producing wrong entry count
- [ ] Empty key with comment absorbing next entry
- [ ] Empty key key/value swap
- [ ] Remove KNOWN_FAILURES entries for fixed cases
- [ ] `cargo test`, `cargo clippy --all-targets` pass
- [ ] Loader conformance: M2N8[0], 6PBE, KK5P, S3PD, NKF9
      now pass

### Task 6: Fix loader bugs — anchors and aliases

Fix loader handling of anchors in complex contexts.

- [ ] Alias-as-key mapping structure (E76Z)
- [ ] Explicit key + anchor combination (PW8X)
- [ ] Document start + anchor (FTA2)
- [ ] Value-tag handling (FH7J)
- [ ] Explicit block mapping with aliases (6M2F)
- [ ] Remove KNOWN_FAILURES entries for fixed cases
- [ ] `cargo test`, `cargo clippy --all-targets` pass
- [ ] Loader conformance: E76Z, PW8X, FTA2, FH7J, 6M2F
      now pass

### Task 7: Fix all remaining loader KNOWN_FAILURES

Tasks 4-6 fix the 13 loader bugs identified during the
previous plan. The loader conformance test (Task 2) found
155 total failures. Fix every remaining failure and delete
the KNOWN_FAILURES constant.

- [ ] Run loader conformance, record remaining failures
      after Tasks 4-6
- [ ] Categorize remaining failures by root cause
- [ ] Fix all remaining loader bugs
- [ ] Loader KNOWN_FAILURES list is empty after this task
- [ ] Delete the KNOWN_FAILURES constant and its skip logic
      from `conformance/loader.rs` — every test-suite case
      must pass unconditionally
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 8: Remove formatter quote-char workaround

With the loader fixed, remove the Plain branch quote-char
guard from `formatter.rs` (commit `25b1130`).

- [ ] Remove the `value.starts_with('"') ||
      value.starts_with('\'')` guard in the Plain scalar
      branch of `node_to_doc`
- [ ] Verify no regressions — the loader now produces
      correct ASTs, so the guard is dead code
- [ ] Update fixture descriptions in
      `quoting-value-starts-with-double-quote.md` and
      `quoting-value-starts-with-single-quote.md` — remove
      loader-bug rationale, describe the general case
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 9: Fix all remaining formatter KNOWN_FAILURES

After loader fixes, re-run formatter conformance and fix
all remaining failures. The formatter already has
infrastructure for explicit keys, anchors, and empty-key
mappings — failures that were blocked by loader bugs
should now pass or need only formatter-side adjustments.

- [ ] Run formatter conformance, record remaining failures
- [ ] For each remaining failure: identify root cause
      (formatter bug, additional loader bug, or missing
      AST info) and fix it
- [ ] Remove fixed entries from KNOWN_FAILURES
- [ ] KNOWN_FAILURES list is empty after this task
- [ ] Delete the KNOWN_FAILURES constant and its skip logic
      from `formatter_conformance.rs` — every test-suite
      case must pass unconditionally

### Task 10: Add interacting-settings fixture combinations

Test formatter behavior when multiple settings interact.

- [ ] `single_quote` + `yaml_version`
- [ ] `single_quote` + `format_enforce_block_style`
- [ ] `bracket_spacing` + `print_width`
- [ ] `bracket_spacing` + `format_enforce_block_style`
- [ ] `format_enforce_block_style` + `print_width`
- [ ] `use_tabs` + `tab_width`
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 11: Final verification

- [ ] Formatter KNOWN_FAILURES constant deleted
- [ ] Loader KNOWN_FAILURES constant deleted
- [ ] Stream conformance remains 351/351
- [ ] Update VS Code extension `package.json` and
      `config.ts` if any settings changed
- [ ] `cargo test` passes
- [ ] `pnpm run build && pnpm run test && pnpm run lint`
      passes in `rlsp-yaml/integrations/vscode/`

## Decisions

- **Supersedes `2026-04-14-formatter-conformance-100.md`** —
  that plan's Tasks 1–8 are committed and valid; remaining
  work is restructured here with loader conformance as the
  foundation
- **Loader first, formatter second** — fix the foundation
  before chasing formatter conformance numbers
- **No parser changes for formatter round-trips** — per
  Crate Boundaries in root `CLAUDE.md`
- **Loader changes ARE valid** — the loader has genuine
  bugs producing incorrect ASTs from correct event streams.
  These are not formatter accommodations.
- **Acceptance criterion is 0 KNOWN_FAILURES in BOTH
  loader and formatter** — both constants deleted after
  reaching 0, so future agents cannot use them as escape
  hatches
