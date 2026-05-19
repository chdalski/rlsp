**Repository:** root
**Status:** InProgress
**Created:** 2026-05-19

## Goal

Reduce boilerplate `#![expect(...)]` blocks in test and
bench files by moving four test-specific lint suppressions
to a workspace-level `clippy.toml`. The four lints —
`unwrap_used`, `expect_used`, `panic`, `indexing_slicing` —
appear across ~30 test/bench files and are universally
appropriate to suppress in test code. Moving them to
`clippy.toml` eliminates repetitive per-file attributes
while keeping the lints enforced in production code.

## Context

- **Current state:** Each test/bench file carries an
  `#![expect(..., reason = "test code")]` block listing
  every lint to suppress. The four most common —
  `clippy::panic`, `clippy::unwrap_used`,
  `clippy::expect_used`, `clippy::indexing_slicing` —
  appear in 10–15 files each. These blocks are pure
  boilerplate: every test file needs them, and their
  presence is never a design decision.

- **Clippy's test-aware options:** Clippy supports
  `allow-*-in-tests` configuration in `clippy.toml` that
  tells the lint "do not fire inside `#[cfg(test)]` or
  integration test crates." The lint simply does not
  trigger, so there is nothing to `expect` or `allow`.
  This is scope narrowing of the lint, not suppression.

- **Available options and their coverage:**

  | clippy.toml option | Lint | Files affected |
  |---|---|---|
  | `allow-unwrap-in-tests = true` | `unwrap_used` | ~14 |
  | `allow-expect-in-tests = true` | `expect_used` | ~15 |
  | `allow-panic-in-tests = true` | `panic` | ~11 |
  | `allow-indexing-slicing-in-tests = true` | `indexing_slicing` | ~10 |

- **Lints that cannot move to clippy.toml** (no
  `allow-*-in-tests` option exists):
  `missing_docs` (rustc lint, 28 files),
  `clippy::wildcard_enum_match_arm` (2 files),
  `clippy::too_many_lines` (1 file),
  `clippy::cast_possible_truncation` (2 files),
  `clippy::significant_drop_tightening` (2 files),
  `clippy::missing_panics_doc` (1 file),
  `dead_code` (rustc lint, 3 files),
  `unsafe_code` (rustc lint, 3 files).
  These remain as `#![expect(...)]` attributes.

- **Workspace compatibility:** `clippy.toml` placed at the
  workspace root applies to all member crates. Clippy
  walks up the directory tree to find it. No per-crate
  configuration is needed.

- **Atomicity constraint:** When clippy.toml suppresses a
  lint in tests, any `#[expect(that_lint)]` in test code
  becomes an unfulfilled expectation, which triggers a
  warning. With `warnings = "deny"` in workspace lints,
  this becomes a build error. Therefore, creating
  `clippy.toml` and removing the redundant `#[expect]`
  entries must happen in the same commit.

- **No contradiction with existing conventions:** The
  CLAUDE.md convention "Use `#[expect(lint, reason)]`
  instead of `#[allow(lint)]`" governs the *mechanism* of
  suppression — when you do suppress a lint, use `expect`
  with a reason. `clippy.toml`'s test options work at a
  different level: the lint never fires, so there is
  nothing to suppress. The two mechanisms are
  complementary.

- **Key files:**
  - `Cargo.toml` — workspace lint config (unchanged)
  - `clippy.toml` — new file at workspace root
  - `CLAUDE.md` — conventions section needs update
  - `.claude/skills/project-init/rust-init.md` — template
    for new projects needs clippy.toml section
  - ~30 test/bench `.rs` files across `rlsp-yaml-parser/`
    and `rlsp-yaml/`

- **Reference:** [Clippy Lint Configuration](https://doc.rust-lang.org/clippy/lint_configuration.html)

## Steps

- [x] Create `clippy.toml` at workspace root
- [x] Remove redundant `#[expect]` entries from all
      test/bench files
- [x] Simplify or remove `#[expect]` blocks that become
      empty or single-lint after removal
- [ ] Update CLAUDE.md conventions section
- [ ] Update project-init skill template
- [x] Verify `cargo clippy --all-targets` passes with zero
      warnings
- [x] Verify `cargo test` passes

## Tasks

### Task 1: Create clippy.toml and remove redundant test lint attributes

**Commit:** 8bde12a

Create the workspace-level `clippy.toml` and remove all
now-redundant `#[expect]` entries from test and bench files
in a single atomic change.

- [x] Create `/workspace/clippy.toml` with:
  ```toml
  allow-unwrap-in-tests = true
  allow-expect-in-tests = true
  allow-panic-in-tests = true
  allow-indexing-slicing-in-tests = true
  ```
- [x] For every location in `rlsp-yaml-parser/` and
  `rlsp-yaml/` — integration test files (`tests/**/*.rs`),
  bench files (`benches/**/*.rs`), and inline
  `#[cfg(test)] mod tests` blocks inside source files —
  remove `clippy::unwrap_used`, `clippy::expect_used`,
  `clippy::panic`, and `clippy::indexing_slicing` from
  `#[expect(...)]` attributes
- [x] No `#![expect(...)]` block in any modified file
  contains only lints from the four removable lints —
  blocks that become empty after removal are deleted
  entirely
- [x] No multi-line `#![expect(...)]` block remains with
  exactly one lint — single-lint blocks use single-line
  form
- [x] Preserve all remaining lints in their `#![expect]`
  blocks unchanged (`missing_docs`,
  `wildcard_enum_match_arm`, `too_many_lines`,
  `cast_possible_truncation`, `significant_drop_tightening`,
  `missing_panics_doc`, `dead_code`, `unsafe_code`)
- [x] Preserve `reason = "..."` strings on remaining
  `#![expect]` attributes
- [x] `cargo clippy --all-targets` passes with zero warnings
- [x] `cargo test` passes

### Task 2: Update conventions documentation

Update project documentation to reflect the new clippy.toml
configuration so future agents and contributors know the
convention.

- [ ] Update `CLAUDE.md` conventions section: add a bullet
  explaining that `clippy.toml` at the workspace root
  configures test-specific lint allowances, and that the
  four lints (`unwrap_used`, `expect_used`, `panic`,
  `indexing_slicing`) do not need `#[expect]` in test code
- [ ] Update `.claude/skills/project-init/rust-init.md`:
  add a section for `clippy.toml` generation with the four
  test-specific options, following the same pattern as the
  existing `Cargo.toml` lint configuration sections
- [ ] The CLAUDE.md convention about `#[expect]` over
  `#[allow]` remains unchanged — it still applies to any
  lint that *does* fire and needs suppression
- [ ] No changes to `Cargo.toml` workspace lint config

## Decisions

- **Only four lints move to clippy.toml** — these are the
  only ones with `allow-*-in-tests` options in Clippy.
  Other test-file lints (`missing_docs`,
  `wildcard_enum_match_arm`, etc.) remain as `#[expect]`
  attributes because Clippy has no test-specific
  configuration for them.
- **Single clippy.toml at workspace root** — Clippy does
  not merge per-crate configs with workspace configs; it
  uses the first one found walking up the tree. A single
  workspace-root file is the correct placement for
  workspace-wide policy.
- **Two tasks, not one** — the code change (Task 1) and
  the documentation change (Task 2) are independently
  committable. Task 1 is the functional change; Task 2 is
  the convention update. Separating them keeps commits
  focused.
