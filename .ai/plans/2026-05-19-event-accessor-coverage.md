**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-19

# event.rs accessor coverage

## Goal

Close the coverage gap on `Event` accessor methods in
`rlsp-yaml-parser/src/event.rs`. Codecov reports the file at
79.8% (21 missed lines) — the lowest-coverage parser file.
Every missed line is in the "no anchor/tag for this variant"
arms of `Event::anchor()`, `Event::anchor_loc()`,
`Event::tag()`, and `Event::tag_loc()`. Direct unit tests
covering every accessor against every variant bring the file
to 100% line coverage.

## Context

### Public API under test

`rlsp-yaml-parser/src/event.rs` defines `Event<'input>` with
11 variants:

- **Node-typed (carry `meta: Option<Box<EventMeta>>`):**
  `Scalar`, `SequenceStart`, `MappingStart`
- **Non-Node-typed (no meta):** `StreamStart`, `StreamEnd`,
  `Comment`, `Alias`, `DocumentStart`, `DocumentEnd`,
  `SequenceEnd`, `MappingEnd`

The four accessors return `Option<&str>` (anchor, tag) or
`Option<Span>` (anchor_loc, tag_loc). Each inspects `meta` for
Node-typed variants and returns `None` for non-Node variants.

### Current test coverage

The existing `#[cfg(test)] mod tests` block in event.rs covers
only `make_meta()` (4 tests, EM-1 through EM-4) and a size
assertion (EM-5). The four public accessors are exercised only
indirectly by other tests when an event happens to carry meta —
the `None` arms (all non-Node variants and Node-typed with
`meta = None`) are never asserted directly.

### Missed lines (from `cargo llvm-cov` on commit 519b916)

- `anchor()`: lines 206, 207, 211 — `Comment`, `Alias`,
  `MappingEnd` arms
- `anchor_loc()`: lines 223–230 — all 8 non-Node variants
- `tag()`: lines 242–249 — all 8 non-Node variants
- `tag_loc()`: lines 263, 264, 267 — `Comment`, `Alias`,
  `SequenceEnd` arms

Total: 21 missed lines, all inside accessor `None` arms.

### Conventions

- `rstest = "0.26"` is already a dev-dep in
  `rlsp-yaml-parser/Cargo.toml`; other test modules use it for
  parameterized cases.
- Tests live in the inline `#[cfg(test)] mod tests` block in
  event.rs alongside the existing `make_meta` tests.
- Module-scope `#[expect(clippy::unwrap_used, reason = "test
  code")]` is already in place.
- Named `#[case::name(...)]` syntax is required for rstest
  parameterized cases per `lang-rust-testing.md`.

## Steps

- [ ] Add parameterized unit tests for the four accessors
      covering every `Event` variant in every meta state
- [ ] Verify `cargo llvm-cov` reports 100% line coverage on
      event.rs

## Tasks

### Task 1: Add accessor coverage tests

Add inline unit tests to `rlsp-yaml-parser/src/event.rs` that
directly invoke `Event::anchor()`, `Event::anchor_loc()`,
`Event::tag()`, and `Event::tag_loc()` against every variant
of `Event` in every meta state. Tests go into the existing
`#[cfg(test)] mod tests` block; named `#[case::name(...)]`
rstest cases preserve scenario intent in failure messages.

Required coverage matrix per accessor:

- **Non-Node variants** (8 variants — `StreamStart`,
  `StreamEnd`, `Comment`, `Alias`, `DocumentStart`,
  `DocumentEnd`, `SequenceEnd`, `MappingEnd`): each accessor
  returns `None`.
- **Node-typed variants** (3 variants — `Scalar`,
  `SequenceStart`, `MappingStart`):
  - With `meta = None`: each accessor returns `None`.
  - With `meta = Some` carrying anchor only: `anchor()` and
    `anchor_loc()` return the constructed values; `tag()` and
    `tag_loc()` return `None`.
  - With `meta = Some` carrying tag only: `tag()` and
    `tag_loc()` return the constructed values; `anchor()` and
    `anchor_loc()` return `None`.
  - With `meta = Some` carrying both: all four accessors
    return the constructed values.

Acceptance criteria:

- [ ] All 11 `Event` variants appear in at least one test case
- [ ] All four accessors are called against every variant — no
      variant skipped for any accessor
- [ ] Both `meta = None` and `meta = Some` paths are exercised
      for each of the three Node-typed variants
- [ ] All four meta states (None, anchor-only, tag-only, both)
      are exercised for at least one Node-typed variant per
      accessor
- [ ] `cargo fmt --check` passes
- [ ] `cargo test -p rlsp-yaml-parser` passes (no regressions
      and all new tests pass)
- [ ] `cargo clippy -p rlsp-yaml-parser --all-targets -- -D
      warnings` passes
- [ ] `cargo llvm-cov -p rlsp-yaml-parser --summary-only`
      reports `event.rs` at 100.00% line coverage

## Decisions

- **Test location:** Inline `#[cfg(test)] mod tests` block in
  event.rs. Matches the existing pattern for the `make_meta`
  tests; module already has the `unwrap_used` allowance for
  test code. (User chose this over a separate `tests/`
  integration crate.)
- **Test framework:** `rstest` parameterized cases with named
  `#[case::name(...)]` syntax. The enum-variant matrix is
  uniform — one parameterized test per accessor reads better
  than 11 hand-written tests per accessor.
- **Advisor consultation:** None — this is a test-only change
  with no production-code modification, no I/O, no trust
  boundaries, and an obvious test design (Cartesian product of
  variants × meta states). Per `risk-assessment.md`, test-only
  changes skip both advisors.
- **Scope limited to event.rs:** Other parser files have
  uncovered lines, but separate analysis showed
  `lexer/comment.rs`'s 19 missed lines are all `unreachable!()`
  defensive branches in test code (not real coverage gaps).
  Files with real gaps (`event_iter/flow.rs`,
  `event_iter/block/mapping.rs`, etc.) need their own analysis
  and are deferred to follow-up plans if the user requests
  them.
