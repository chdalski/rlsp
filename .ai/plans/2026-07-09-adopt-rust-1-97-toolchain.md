**Repository:** root
**Status:** NotStarted
**Created:** 2026-07-09

# Adopt Rust 1.97 and Fix Clippy Regressions

## Goal

CI's "Format + Clippy" job went red because CI floats on
`dtolnay/rust-toolchain@stable` (currently Rust 1.97.0)
while local dev lagged on 1.94.1 — so 13 new clippy lints
introduced/tightened in 1.97 were invisible locally and
only failed on push. Get CI green again under 1.97 and
adopt 1.97 as the project's Rust version: fix the 13
clippy findings, pin local dev to 1.97.0 via
`rust-toolchain.toml` so contributors and AI agents build
against the same compiler CI uses (catching such lints
before push), and bump the declared MSRV to 1.97 across
all crates.

## Context

- **Trigger:** failed run
  https://github.com/chdalski/rlsp/actions/runs/29021942363/job/86131172450
  — `cargo clippy --workspace --all-targets -- -D warnings`
  exited 101. `cargo fmt --check`, the full test suite,
  `cargo build --all-targets`, and the Zed wasm clippy
  gate are all clean under 1.97; the *only* breakage is
  the 13 clippy findings below.
- **The 13 clippy findings (all in test code):**
  - `useless_borrows_in_formatting` — redundant `&` in
    `assert!`/`assert_eq!` format arguments:
    - `rlsp-yaml-parser/tests/loader.rs` lines 60, 86,
      117, 186, 191, 205, 210, 243, 420 (9)
    - `rlsp-yaml-parser/tests/conformance/loader.rs`
      lines 766, 771 (2)
  - `manual_assert_eq` — `assert!(a == b)` should be
    `assert_eq!(a, b)`:
    - `rlsp-yaml/src/schema_validation/scalar_constraints.rs`
      lines 1105, 1115 (2) — inside the `#[cfg(test)]`
      module.
  - Both are fixed exactly as clippy suggests (remove the
    `&`; convert to `assert_eq!`). Both changes are
    semantics-preserving and compile on the old MSRV — no
    newer-compiler feature is required.
- **Why warnings fail even without `-D warnings`:** the
  workspace lint config sets `warnings = "deny"`, so any
  clippy warning is an error regardless of the CLI flag.
- **Toolchain strategy (user-chosen, "Option 1"):** pin
  *local* dev to 1.97.0 via a root `rust-toolchain.toml`;
  leave *CI* on floating `@stable` so it keeps acting as a
  canary that fails when a new stable Rust ships new lints
  — the user's intended "hint to update the toolchain."
  CI workflow toolchain refs are deliberately NOT changed
  (see Non-Goals).
- **Why CI keeps floating with the pin present:**
  `dtolnay/rust-toolchain` exports `RUSTUP_TOOLCHAIN` into
  `$GITHUB_ENV`, and that env var has higher precedence
  than a `rust-toolchain.toml` file in rustup's override
  order — so CI resolves to `@stable`, ignoring the pin,
  while local dev (no such env var) honors the file. Note:
  today stable == 1.97.0 == pin, so CI behaves identically
  whether or not it honors the file; the float-vs-pin
  distinction only becomes observable when stable advances
  past 1.97.0.
- **MSRV is deliberate policy, not a technical need.** The
  fixes compile on 1.87; setting `rust-version = "1.97"`
  aligns the declared MSRV with the pinned/CI toolchain
  and drops support for Rust 1.87–1.96. MSRV is currently
  `"1.87"` in the three workspace crates only; it appears
  in no README or docs, so no doc sync is needed. The Zed
  integration crate (outside the workspace) has no
  `rust-version` today.
- **Release relevance:** an MSRV bump can be
  release-significant; the reviewer selects the
  conventional-commit type for the Task 2 commit
  accordingly (release-plz derives version progression
  from commit type — agents must not edit `version =`
  fields directly).
- **Devcontainer (unchanged, per user):** the `rust:1`
  devcontainer feature installs rustup, which auto-installs
  the pinned 1.97.0 (+ listed components) on the first
  cargo command in the repo. A *fresh* toolchain install
  works in this container's overlay filesystem (an
  in-place `rustup update` does not — cross-device link
  error — but the pin performs a fresh install, so it is
  unaffected).
- **Key files:**
  - New: `rust-toolchain.toml` (repo root)
  - Edited (fixes): the 3 test files above
  - Edited (MSRV): `rlsp-fmt/Cargo.toml`,
    `rlsp-yaml/Cargo.toml`, `rlsp-yaml-parser/Cargo.toml`,
    `rlsp-yaml/integrations/zed/Cargo.toml`
- **Verification toolchain:** the sandbox default is now
  1.97.0. Task 1 lands *before* the pin file exists, so
  the implementer confirms `rustc --version` is 1.97.0
  (or runs clippy via `cargo +1.97.0 …`) when verifying
  Task 1.

## Steps

- [x] Reproduce the CI failure locally on 1.97.0
- [x] Enumerate all 1.97 breakage (clippy only; 13 findings)
- [x] Confirm strategy with user (pin local, CI canary; MSRV 1.97; all 4 crates; devcontainer unchanged)
- [x] Task 1: Fix the 13 clippy findings
- [ ] Task 2: Add `rust-toolchain.toml` pin and bump MSRV to 1.97 in all 4 crates
- [ ] Confirm the full gate set passes under 1.97

## Tasks

### Task 1: Fix the 13 clippy findings

Apply clippy's suggested fixes to the 13 findings in test
code so `cargo clippy --workspace --all-targets -- -D warnings`
passes under Rust 1.97. Lands first so `main` returns to
green immediately.

- [x] `useless_borrows_in_formatting`: remove the redundant
      `&` at the 9 sites in
      `rlsp-yaml-parser/tests/loader.rs` (60, 86, 117, 186,
      191, 205, 210, 243, 420)
- [x] `useless_borrows_in_formatting`: remove the redundant
      `&` at the 2 sites in
      `rlsp-yaml-parser/tests/conformance/loader.rs`
      (766, 771)
- [x] `manual_assert_eq`: convert `assert!(a == b)` to
      `assert_eq!(a, b)` at the 2 sites in
      `rlsp-yaml/src/schema_validation/scalar_constraints.rs`
      (1105, 1115)
- [x] Confirm the running toolchain is 1.97.0
      (`rustc --version`)
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
      exits 0 (0 warnings)
- [x] `cargo test --workspace` passes with 0 failures
- [x] `cargo fmt --all -- --check` exits 0
- [x] No `version = "..."` field changed in any Cargo.toml

Acceptance: all four commands above pass; the diff touches
only the three named test files.

### Task 2: Adopt Rust 1.97 (pin toolchain + bump MSRV)

Add a root `rust-toolchain.toml` pinning local dev to
1.97.0, and bump the declared MSRV to `"1.97"` in all four
crates. CI toolchain refs are intentionally left on
`@stable` (Non-Goals).

- [ ] Create `rust-toolchain.toml` at the repo root with
      `channel = "1.97.0"` and
      `components = ["clippy", "rustfmt"]`
- [ ] Add a comment in `rust-toolchain.toml` stating that
      it pins local dev while CI floats on `@stable` as a
      new-stable canary (reveals intent for future readers)
- [ ] Change `rust-version = "1.87"` to
      `rust-version = "1.97"` in `rlsp-fmt/Cargo.toml`,
      `rlsp-yaml/Cargo.toml`, `rlsp-yaml-parser/Cargo.toml`
- [ ] Add `rust-version = "1.97"` to the `[package]` table
      of `rlsp-yaml/integrations/zed/Cargo.toml` (leave its
      `version` and `edition` untouched)
- [ ] Do NOT modify any `version = "..."` field in any
      Cargo.toml
- [ ] With the pin file present, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace`,
      and `cargo fmt --all -- --check` still pass
- [ ] The Zed wasm gate still passes:
      `cargo clippy --manifest-path
      rlsp-yaml/integrations/zed/Cargo.toml --all-targets
      --target wasm32-wasip2 -- -D warnings`
- [ ] `cargo metadata` / `cargo build` accept the new
      `rust-version` without an MSRV error

Acceptance: `rust-toolchain.toml` exists and pins 1.97.0;
all four crates declare `rust-version = "1.97"`; no
`version =` field changed; the workspace and Zed gates
listed above all pass under the pin.

## Decisions

- **Toolchain: pin local, CI floats (Option 1).** Fixes the
  local/CI drift that hid the lints, while preserving CI's
  canary behavior the user values. Minimal surface: one new
  file, no workflow edits. A clean stepping stone if the
  user later adopts Renovate/automation.
- **MSRV = "1.97" (major.minor).** Matches the existing
  field style (`"1.87"`) and aligns the declared MSRV with
  the pinned toolchain. Deliberate policy choice, not a
  compiler-feature requirement — the reviewer should not
  reject it as "unnecessary."
- **Pin channel = "1.97.0" (exact patch).** Maximizes local
  reproducibility; CI's floating stable may advance to
  1.97.x — patch drift rarely adds lints, so the canary
  still works at minor granularity.
- **Two tasks, clippy first.** Task 1 restores green `main`
  immediately (all 13 clippy errors must land in one commit
  because `cargo clippy --workspace` fails wholesale on any
  single warning). Task 2 is pure metadata/infra and keeps
  `main` green.
- **No advisors.** Task 1 is test-only, mechanical, and
  semantics-preserving; Task 2 is config/metadata. Neither
  touches a trust boundary, untrusted input, or test
  coverage — both are low-risk, low-uncertainty per
  `risk-assessment.md`.

## Non-Goals

- **Aligning CI workflow toolchain refs to the pin.** CI
  intentionally stays on floating `@stable` as the canary
  (Option 1). The 8 `dtolnay/rust-toolchain@stable` refs
  across `ci.yml` (2), `coverage.yml` (1),
  `zed-release.yml` (1), `release-plz.yml` (3), and
  `vscode-extension.yml` (1) are left unchanged.
- **Renovate or a scheduled toolchain-bump workflow.**
  Deferred; Dependabot does not manage toolchains, and the
  user chose the lighter Option 1 for now.
- **Any `.devcontainer/` change.** rustup auto-installs the
  pinned toolchain from `rust-toolchain.toml`; the pin file
  stays the single source of truth (no version baked into
  the `rust:1` feature).
- **Adding `wasm32-wasip2` to `rust-toolchain.toml`
  `targets`.** Workspace crates don't need it; Zed devs add
  the target per existing `CLAUDE.md` instructions.
- **Editing any `version = "..."` field.** release-plz owns
  version progression; only `rust-version` changes here.
- **A `docs/feature-log.md` entry.** This is a
  toolchain/packaging change, not a user-facing YAML LSP
  feature; the conventional-commit history + this plan
  carry the record.
