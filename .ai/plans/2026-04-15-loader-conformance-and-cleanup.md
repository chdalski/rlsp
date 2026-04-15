**Repository:** root
**Status:** NotStarted
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
2. All loader bugs listed in this plan are fixed
3. Formatter workarounds for loader bugs are removed
4. Formatter conformance KNOWN_FAILURES reaches 0
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

- [ ] Restructure conformance tests into module
- [ ] Add loader conformance test against yaml-test-suite
- [ ] Surface document marker flags in AST
- [ ] Fix loader bugs (document tags, explicit keys,
      empty keys, anchors)
- [ ] Remove formatter quote-char workaround
- [ ] Verify formatter KNOWN_FAILURES reduction
- [ ] Add interacting-settings fixture combinations
- [ ] Final verification — 0 KNOWN_FAILURES

## Tasks

### Task 1: Restructure conformance tests into module

Move the conformance test from a standalone file into a
module structure that supports both stream and loader
testing.

- [ ] Create `rlsp-yaml-parser/tests/conformance/` directory
- [ ] Move `conformance.rs` → `conformance/stream.rs`
- [ ] Add `conformance/mod.rs` with shared helpers
      (`visual_to_raw`, case parsing, etc.)
- [ ] Verify existing stream conformance still passes
      (351/351)
- [ ] Check if `smoke/conformance.rs` has cases that should
      be in the conformance module
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 2: Add loader conformance test

Add a loader conformance test that runs `load()` against
the yaml-test-suite and verifies AST correctness by
comparing the AST structure against the expected event tree
from each test case.

- [ ] Create `conformance/loader.rs`
- [ ] For each non-fail case: `load(input)` must succeed
- [ ] Verify AST structure matches expected event tree
      (correct number of documents, correct node types,
      correct scalar values, correct styles, correct
      anchors/tags)
- [ ] Populate KNOWN_FAILURES allowlist for loader
- [ ] Measure baseline: how many cases pass vs fail
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 3: Surface document marker flags in AST

Add `explicit_start: bool` and `explicit_end: bool` to
the `Document` struct and populate from events.

- [ ] Add fields to `Document` in `node.rs`
- [ ] Populate from `DocumentStart { explicit, .. }` in
      loader
- [ ] Populate from `DocumentEnd { explicit, .. }` in
      loader
- [ ] Update formatter to use flags: emit `---` only when
      `explicit_start` is true, emit `...` when
      `explicit_end` is true
- [ ] Update `Document<Span>` struct diagram in
      `rlsp-yaml-parser/docs/architecture.md` to include
      `explicit_start` and `explicit_end`
- [ ] `cargo test`, `cargo clippy --all-targets` pass
- [ ] Parser conformance remains 351/351

### Task 4: Fix loader bugs — document tags and inline scalars

Fix loader handling of document-level tags and inline
quoted scalars on `---` lines.

- [ ] `--- !!set` / `--- !!map` — document tag must go on
      the mapping's tag field, not become a scalar key
- [ ] `--- !!omap` — same pattern
- [ ] `--- "quoted"` inline scalars — loader must produce
      correct DoubleQuoted scalar (not garbled Plain)
- [ ] Remove KNOWN_FAILURES entries for fixed cases
- [ ] `cargo test`, `cargo clippy --all-targets` pass
- [ ] Loader conformance: 2XXW, 35KP, J7PZ now pass

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

### Task 7: Remove formatter quote-char workaround

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

### Task 8: Verify formatter KNOWN_FAILURES reduction

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

### Task 9: Add interacting-settings fixture combinations

Test formatter behavior when multiple settings interact.

- [ ] `single_quote` + `yaml_version`
- [ ] `single_quote` + `format_enforce_block_style`
- [ ] `bracket_spacing` + `print_width`
- [ ] `bracket_spacing` + `format_enforce_block_style`
- [ ] `format_enforce_block_style` + `print_width`
- [ ] `use_tabs` + `tab_width`
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 10: Final verification

- [ ] Formatter KNOWN_FAILURES is 0
- [ ] Loader conformance passes
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
- **Acceptance criterion is 0 KNOWN_FAILURES** — but only
  after the loader is conformant, so the metric measures
  what it claims to measure
