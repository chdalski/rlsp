**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-12

## Goal

Harden workspace lint configuration: (1) promote
`clippy::panic` from a crate-level deny in `rlsp-yaml-parser`
to a workspace-wide deny, (2) enforce `#[expect(..., reason)]`
over `#[allow(...)]` across the entire workspace via
`allow_attributes` and `allow_attributes_without_reason`,
(3) remove dead code behind `#[allow(dead_code)]` blankets,
and (4) update the project-init skill template to match.

## Context

- `clippy::panic` is currently `#![deny(clippy::panic)]` in
  `rlsp-yaml-parser/src/lib.rs` only. The `rust-init.md`
  skill template already includes `panic = "deny"` but the
  workspace `Cargo.toml` was never updated because the lint
  was added after code was already written.
- All `panic!()` calls in `rlsp-yaml/src/` and `rlsp-fmt/src/`
  are inside `#[cfg(test)]` modules — no production-code
  violations exist.
- `rlsp-yaml-parser` test files (`tests/`) already
  `#![allow(clippy::panic)]` because the crate-level deny
  was in place.
- `#[allow(dead_code)]` appears in three locations:
  `rlsp-yaml-parser/src/loader.rs:726` (on genuinely dead
  `load_one` test helper), `rlsp-yaml-parser/benches/fixtures.rs:8`
  (blanket — no actual dead code), `rlsp-yaml/benches/fixtures/mod.rs:7`
  (blanket — no actual dead code).
- There are ~80-100 `#[allow(...)]` annotations across the
  workspace (production code, inline test modules, integration
  tests, benches). All must convert to
  `#[expect(..., reason = "...")]`.
- Workspace lints apply to all crates uniformly. Adding
  `allow_attributes = "deny"` breaks the build until all
  `#[allow]` annotations are converted.

### Key files

- `/workspace/Cargo.toml` — workspace lint configuration
- `/workspace/rlsp-yaml-parser/src/lib.rs` — crate-level
  `#![deny(clippy::panic)]` to remove
- `/workspace/.claude/skills/project-init/rust-init.md` —
  skill template to update
- All `.rs` files with `#[allow(...)]` annotations

## Steps

- [ ] Add `panic = "deny"` to workspace Cargo.toml, remove
      crate-level deny, fix test modules
- [ ] Remove dead code and `#[allow(dead_code)]` blankets
- [ ] Convert all `#[allow]` → `#[expect(..., reason)]`
      across all crates
- [ ] Add `allow_attributes` + `allow_attributes_without_reason`
      to workspace Cargo.toml
- [ ] Verify `cargo clippy --all-targets` passes clean

## Tasks

### Task 1: Promote `clippy::panic` to workspace

Add `panic = "deny"` to `[workspace.lints.clippy]` in the
root `Cargo.toml`. Remove `#![deny(clippy::panic)]` from
`rlsp-yaml-parser/src/lib.rs`. Fix any new violations —
all known `panic!()` calls in `rlsp-yaml` and `rlsp-fmt`
are in `#[cfg(test)]` modules, so add `clippy::panic` to
their existing `#[allow(...)]` lists. Verify with
`cargo clippy --all-targets`.

- [ ] Add `panic = "deny"` to `[workspace.lints.clippy]`
- [ ] Remove `#![deny(clippy::panic)]` from
      `rlsp-yaml-parser/src/lib.rs`
- [ ] Add `clippy::panic` to test module allow-lists in
      `rlsp-yaml` where `panic!()` is used
- [ ] Verify `cargo clippy --all-targets` passes

### Task 2: Dead code cleanup

Remove `#[allow(dead_code)]` annotations and clean up:

- `rlsp-yaml-parser/src/loader.rs:726` — delete the unused
  `load_one` test helper function and its `#[allow]`
- `rlsp-yaml-parser/benches/fixtures.rs:8` — remove blanket
  `#![allow(dead_code)]` (all functions are used; no dead
  code exists)
- `rlsp-yaml/benches/fixtures/mod.rs:7` — same: remove
  blanket (no dead code)

Verify with `cargo clippy --all-targets`.

- [ ] Delete `load_one` from loader.rs test module
- [ ] Remove `#![allow(dead_code)]` from both bench fixtures
- [ ] Verify no dead-code warnings

### Task 3: Convert `#[allow]` → `#[expect(..., reason)]` and enable enforcement lints

This task is atomic — the `#[allow]` → `#[expect]`
conversion and the Cargo.toml lint additions must be in
the same commit because `allow_attributes = "deny"` would
break the build with any remaining `#[allow]`.

a) Convert every `#[allow(...)]` and `#![allow(...)]`
   annotation in all three crates to
   `#[expect(..., reason = "...")]` /
   `#![expect(..., reason = "...")]`. This includes:
   - Production source code (`src/`)
   - Inline `#[cfg(test)]` modules
   - Integration tests (`tests/`)
   - Benchmarks (`benches/`)

   Reason strings should be concise and explain *why* the
   suppression is needed (e.g., "test code uses unwrap for
   brevity", "LSP protocol field is deprecated but required",
   "u32 line numbers always fit in usize").

b) Add to `[workspace.lints.clippy]` in root `Cargo.toml`:
   ```
   allow_attributes = "deny"
   allow_attributes_without_reason = "deny"
   ```

c) Verify `cargo clippy --all-targets` passes clean.

Note: `rust-init.md` was already updated and committed
(`10757da`) as a skill-output commit before plan execution.

- [ ] Convert all `#[allow]` in `rlsp-fmt`
- [ ] Convert all `#[allow]` in `rlsp-yaml-parser`
- [ ] Convert all `#[allow]` in `rlsp-yaml`
- [ ] Add `allow_attributes` + `allow_attributes_without_reason`
      to workspace Cargo.toml
- [ ] Verify clean clippy pass

## Decisions

- **`#[expect]` over `#[allow]`** — `#[expect]` is
  self-cleaning: it warns when the suppressed lint stops
  firing, preventing stale suppressions from accumulating.
  Combined with mandatory reasons, every suppression is
  documented and automatically cleaned up when no longer
  needed.
- **Same rules for test and production code** — uniform
  policy, no exceptions. Test code uses `#[expect]` with
  reasons too.
- **Task 3 is atomic** — converting `#[allow]` and adding
  the deny lints must be in one commit. The `#[expect]`
  attribute works independently of `allow_attributes`, but
  the build would break if the deny is added before all
  `#[allow]` annotations are converted.
- **Bench fixture `dead_code` blankets removed, not
  converted** — all functions in both fixture files are
  actually used. The blankets were precautionary and
  suppress nothing.
