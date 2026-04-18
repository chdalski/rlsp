**Repository:** root
**Status:** Completed (2026-03-29)
**Created:** 2026-03-29

## Goal

Make `cargo clippy --all-targets` pass with zero warnings.
Currently clippy is only run against library targets (both
in CI and the reviewer's build step), hiding 451+ lint
violations in test code. After this work, clippy covers all
targets — library, tests, and integration tests — and both
CI and the reviewer enforce it.

## Context

- All 451 violations are in test code; production code is
  clean except for 2 `expect()` calls that the new
  `expect_used = "deny"` lint now catches.
- User approved: allow `indexing_slicing`, `expect_used`,
  and `unwrap_used` in test modules (these are acceptable
  in tests — panics produce clear failures). Fix all other
  violations.
- 21 `#[cfg(test)]` modules across both crates plus 1
  integration test file (`rlsp-yaml/tests/lsp_lifecycle.rs`).
- Non-indexing/expect/unwrap violations (~88): needless_collect,
  redundant_closure, uninlined_format_args, redundant_clone,
  format! appended to String, Default::default field assignment,
  needless_raw_string_hashes, cast truncation, wildcard enum
  match, and assorted minor lints.
- CI runs `cargo clippy --workspace -- -D warnings` (no
  `--all-targets`).
- Root `CLAUDE.md` documents `cargo clippy` without
  `--all-targets`.

## Steps

- [x] Clarify requirements with user
- [x] Add `expect_used` and `unwrap_used` to Cargo.toml lints
- [x] Fix production `expect()`/`unwrap()` calls (9 total — 7092dcd)
- [x] Add lint allows to all 21 test modules + integration test (7092dcd)
- [x] Fix remaining 88 non-allowed clippy violations in tests (ac8b43f)
- [x] Update CI to use `--all-targets` (48ec1cc)
- [x] Update root CLAUDE.md build commands (48ec1cc)
- [x] Verify `cargo clippy --all-targets` passes clean (48ec1cc)

## Tasks

### Task 1: Fix production expect() and add test module allows

Fix the 2 production `expect()` calls:
- `rlsp-fmt/src/ir.rs:171` — replace `iter.next().expect(...)`
  with a safe alternative (the empty check on line 167
  guarantees this succeeds)
- `rlsp-yaml/src/schema_validation.rs:341` — replace
  `map.get(k).expect(...)` with a safe alternative (key
  comes from the map being iterated)

Add `#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]`
to all 21 `#[cfg(test)]` modules and the integration test
file `rlsp-yaml/tests/lsp_lifecycle.rs`.

Files:
- `rlsp-fmt/src/ir.rs`
- `rlsp-yaml/src/schema_validation.rs`
- All 21 files with `#[cfg(test)]` modules (listed in context)
- `rlsp-yaml/tests/lsp_lifecycle.rs`

### Task 2: Fix remaining clippy violations in test code

After Task 1 unblocks compilation, run
`cargo clippy --all-targets` and fix all remaining
violations. These are ~88 non-allowed lints including:
needless_collect, redundant_closure, uninlined_format_args,
redundant_clone, write!+format! inefficiency,
Default::default field assignment, needless_raw_string_hashes,
cast truncation allows, wildcard enum match, and minor lints.

Files: all test modules across `rlsp-yaml/src/` and
`rlsp-yaml/tests/`.

### Task 3: Update CI and CLAUDE.md

- `.github/workflows/ci.yml`: change clippy command to
  `cargo clippy --workspace --all-targets -- -D warnings`
- `CLAUDE.md`: update build commands to show
  `cargo clippy --all-targets`

Files:
- `.github/workflows/ci.yml`
- `CLAUDE.md`

## Decisions

- **Allow indexing/expect/unwrap in tests:** User approved.
  Tests that panic on unexpected data produce clear failure
  messages — enforcing `.get()` everywhere would reduce
  readability with no safety benefit.
- **Module-level allows, not file-level:** Place the allow
  on the `#[cfg(test)] mod tests` block so production code
  in the same file remains strict.
- **expect_used + unwrap_used added to Cargo.toml:** Already
  committed (57f2169). Enforces the existing rule "Never use
  unwrap/expect in production code" at the lint level.
