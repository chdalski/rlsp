**Repository:** root
**Status:** InProgress
**Created:** 2026-09-23

# Adopt Rust 1.98.1, Edition 2024 for Zed, and Refresh Cargo.lock

## Goal

Move the project from Rust 1.97.1 to the current stable
Rust 1.98.1 so local dev and CI build on the latest
compiler, fixing every compiler and clippy finding the new
toolchain raises rather than suppressing them. Alongside the
toolchain bump: raise the declared MSRV to 1.98 in all four
crates, move the Zed extension crate from edition 2021 to
edition 2024 (the latest stable edition — there is no
edition 2026), and refresh both lockfiles to the latest
semver-compatible dependency versions so that tests, clippy,
fmt, and build are all green on 1.98.1 with up-to-date
dependencies.

## Context

- **Toolchain pin mechanics (full-pin).** The root
  `rust-toolchain.toml` (`channel = "1.97.1"`, components
  clippy + rustfmt) pins BOTH local dev and CI: rustup
  honors it over `dtolnay/rust-toolchain`'s default. The CI
  workflows reference the matching action branch
  `dtolnay/rust-toolchain@1.97.1` at 8 sites — `ci.yml` (2),
  `coverage.yml` (1), `zed-release.yml` (1),
  `release-plz.yml` (3), `vscode-extension.yml` (1) — so
  each job's cross-compile targets are installed onto the
  same toolchain the pin selects. `stable` and `1.98.1` are
  separate rustup installs; pin and refs must move together
  or cross-compile jobs fail with `can't find crate for
  core`. The `dtolnay/rust-toolchain` repo has a `1.98.1`
  branch (verified via `git ls-remote`).
- **Declared MSRV** is `rust-version = "1.97"` in
  `rlsp-fmt/Cargo.toml`, `rlsp-yaml/Cargo.toml`,
  `rlsp-yaml-parser/Cargo.toml`, and
  `rlsp-yaml/integrations/zed/Cargo.toml`. No README or
  `docs/` file states the MSRV or Rust version, so no doc
  sync is needed.
- **Editions.** The three workspace crates are already on
  edition 2024. The Zed extension crate
  (`rlsp-yaml/integrations/zed/`, outside the workspace,
  built for `wasm32-wasip2`) is on edition 2021. `rustc
  1.98.1 --help` lists `2015|2018|2021|2024|future` and
  names 2024 the latest stable edition. A lead-side trial
  bump of the Zed crate to 2024 on 1.98.1 passed its wasm
  clippy gate; only `cargo fmt --check` reported diffs (the
  2024 style edition formats differently).
- **1.98.1 findings (measured on a clean target dir with the
  refreshed lockfile).** `cargo fmt --check` and
  `cargo build` pass. Five findings, all errors under the
  workspace's `warnings = "deny"`:
  - `unused_imports` (rustc, not clippy) —
    `rlsp-yaml/src/schema.rs` test module re-imports
    `use std::io::Read as _;` while the module file already
    imports it at the top and the tests pull it in via
    `use super::*`. This one breaks `cargo test`
    compilation outright.
  - `clippy::chunks_exact_to_as_chunks` (2 sites) and
    `clippy::missing_const_for_fn` on `detect_encoding` —
    `rlsp-yaml-parser/src/encoding.rs`, the parser's
    BOM/encoding detection and UTF-16/32 decoding, which
    runs on untrusted input.
  - `clippy::manual_is_variant_and` —
    `rlsp-yaml/src/server.rs` (`.ok().is_some_and(..)` on a
    `Result`).
  - With lints capped to warn, all 6300 workspace tests
    pass on 1.98.1 with the refreshed lockfile.
  - `as_chunks` (stable since 1.88), `is_ok_and`, and a
    `const fn` over this body all compile on 1.97, so the
    fixes can land before the pin moves.
- **Dependencies.** Every direct dependency of every crate
  (workspace + Zed) is already locked at its newest stable
  crates.io release; no `Cargo.toml` requirement changes
  are needed. `cargo update` refreshes 20 transitive
  packages in the workspace `Cargo.lock` (e.g. `rustls`
  0.23.45, `syn` 3.0.6, `synstructure` 0.14.0, `zerocopy`
  0.8.57) and 11 in the Zed crate's `Cargo.lock`.
- **Clean builds are mandatory for lint verification after a
  toolchain change** — the incremental clippy cache does not
  re-lint unchanged code and under-reports new-toolchain
  findings (see root `CLAUDE.md`, Build and Test).
- **CI cross-compile jobs are path/release-gated.** The Zed,
  vscode, and release-binary jobs do not run on ordinary
  pushes to `main`, so pushing proves only `ci.yml` and
  `coverage.yml`.
- **References:**
  [Rust 1.98.0 release notes](https://github.com/rust-lang/rust/releases),
  [Rust Edition Guide — 2024](https://doc.rust-lang.org/edition-guide/rust-2024/index.html),
  [clippy lint list](https://rust-lang.github.io/rust-clippy/rust-1.98.0/index.html).

## Steps

- [x] Check direct dependencies of all crates against
      crates.io latest (all current)
- [x] Trial 1.98.1 + `cargo update` in a scratch worktree;
      enumerate findings on a clean target dir
- [x] Confirm editions available in 1.98.1 (2024 is latest)
- [x] Confirm scope with user (Zed edition 2024; MSRV 1.98;
      include lockfile refresh)
- [x] Task 1: Fix the five 1.98.1 findings
- [x] Task 2: Pin Rust 1.98.1 locally and in CI; MSRV 1.98
- [ ] Task 3: Move the Zed extension crate to edition 2024
- [ ] Task 4: Refresh both lockfiles
- [ ] Push to `main` and confirm `ci.yml` and `coverage.yml`
      runs are green (`gh run list`)
- [ ] Verify the Zed wasm build on CI with the new pin via a
      throwaway push-triggered scratch workflow on a scratch
      branch; delete the branch afterward
- [ ] Mark plan Completed

## Tasks

### Task 1: Fix the five Rust 1.98.1 findings

Resolve the one rustc and four clippy findings that 1.98.1
raises, so the codebase is lint-clean on both 1.97.1 (the
current pin) and 1.98.1. Fixes apply the lint's
recommendation; no lint is allowed, expected, or suppressed.

- [x] A clean-target `cargo +1.98.1 clippy --workspace
      --all-targets -- -D warnings` exits 0
- [x] A clean-target `cargo +1.98.1 test --workspace`
      compiles and passes with 0 failures
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
      and `cargo test --workspace` on the pinned 1.97.1 also
      exit 0 with 0 failures
- [x] `cargo fmt --all -- --check` exits 0
- [x] No `#[allow]`, `#[expect]`, or lint-config entry was
      added for any of the five findings
- [x] UTF-16 and UTF-32 decoding behavior in
      `rlsp-yaml-parser/src/encoding.rs` is unchanged,
      including input whose length is not a multiple of the
      code-unit size, as shown by tests that exercise those
      inputs
- [x] The diff touches only `.rs` files

### Task 2: Pin Rust 1.98.1 locally and in CI; declare MSRV 1.98

Move the full-pin from 1.97.1 to 1.98.1 so local dev and
every CI job build on the same toolchain, and raise the
declared MSRV to match.

- [x] `rust-toolchain.toml` pins `channel = "1.98.1"` with
      the same components, and its comment names 1.98.1
      everywhere it previously named 1.97.1
- [x] All 8 CI `dtolnay/rust-toolchain` refs are `@1.98.1`;
      no `@1.97.1` reference remains anywhere under
      `.github/`
- [x] No per-job `targets:` / `components:` input and no
      workflow `permissions` block changed
- [x] All four crates declare `rust-version = "1.98"`
- [x] No `version = "..."` field changed in any Cargo.toml
- [x] `rustc --version` in the repo reports 1.98.1
- [x] After `cargo clean`: `cargo clippy --workspace
      --all-targets -- -D warnings` exits 0,
      `cargo test --workspace` passes with 0 failures,
      `cargo fmt --all -- --check` exits 0
- [x] Zed gates pass on 1.98.1: `cargo check` and
      `cargo clippy --all-targets -- -D warnings` with
      `--manifest-path rlsp-yaml/integrations/zed/Cargo.toml
      --target wasm32-wasip2`
- [x] Every `.github/workflows/*.yml` file touched parses as
      valid YAML

### Task 3: Move the Zed extension crate to edition 2024

Bring the Zed extension crate onto the latest stable
edition so all four crates share edition 2024.

- [ ] `rlsp-yaml/integrations/zed/Cargo.toml` declares
      `edition = "2024"`; its `version` field is unchanged
- [ ] Zed gates pass on 1.98.1: `cargo check` and
      `cargo clippy --all-targets -- -D warnings` with
      `--manifest-path rlsp-yaml/integrations/zed/Cargo.toml
      --target wasm32-wasip2`
- [ ] `cargo fmt --manifest-path
      rlsp-yaml/integrations/zed/Cargo.toml -- --check`
      exits 0
- [ ] Any edition-migration semantic change (e.g. from
      `cargo fix --edition`) is named in the review handoff
      with the reason it is behavior-preserving
- [ ] No lint was allowed, expected, or suppressed to make
      the edition change pass

### Task 4: Refresh both lockfiles to the latest compatible versions

Update the workspace `Cargo.lock` and the Zed crate's
`Cargo.lock` so every dependency resolves to its newest
semver-compatible release, with the full gate set green.

- [ ] `cargo update --dry-run` reports no pending updates
      for the workspace or for the Zed crate
- [ ] No `Cargo.toml` changed in this task
- [ ] After `cargo clean`: `cargo clippy --workspace
      --all-targets -- -D warnings` exits 0,
      `cargo test --workspace` passes with 0 failures,
      `cargo build --workspace` exits 0
- [ ] Zed gates pass: `cargo check` and
      `cargo clippy --all-targets -- -D warnings` with
      `--manifest-path rlsp-yaml/integrations/zed/Cargo.toml
      --target wasm32-wasip2`
- [ ] `cargo test -p rlsp-yaml --test
      claude_code_stdio_smoke` passes
- [ ] The diff touches only the two `Cargo.lock` files

## Decisions

- **Fix, don't suppress.** Same posture as the 1.97
  adoption: every new finding gets its lint's fix; no
  `#[expect]` or lint-config relaxation.
- **Fixes land before the pin (Task 1 before Task 2).** The
  fixes compile on 1.97.1, so every commit on `main` stays
  green under the pin it ships with.
- **MSRV = "1.98".** User-chosen policy: the declared MSRV
  tracks the pinned toolchain. Not a compiler-feature
  requirement — the reviewer does not reject it as
  unnecessary.
- **Pin = exact patch "1.98.1".** Matches the existing
  full-pin convention for reproducibility.
- **Edition 2024 for Zed, not "2026".** No edition 2026
  exists; 2024 is the latest stable edition rustc 1.98.1
  accepts. The workspace crates are already on 2024.
- **Lockfile refresh in this plan.** User-chosen; the
  refresh is `cargo update` only because no direct
  dependency has a release outside its declared range.
- **Advisors.** Task 1: test-engineer (input + output
  gates) — it changes production parser code that decodes
  untrusted input, and chunking semantics on
  non-multiple-length input are exactly where a mechanical
  rewrite can drift; no security-engineer, since no new
  trust-boundary logic is introduced. Task 2: none
  (toolchain/CI ref/MSRV config). Task 3: none (edition +
  formatting on a small crate with a green compile gate).
  Task 4: security-engineer (input + output gates) — new
  versions of third-party crates, including TLS (`rustls`)
  used for fetching remote schemas; test-engineer not
  needed (no code change, full suite is the verification).
- **Release/vscode cross-compile matrices verified by
  mechanism.** They run only at release/dispatch; the
  scratch-workflow Zed wasm check plus the unchanged
  targets/components inputs cover the mechanism.

## Non-Goals

- Changing any `Cargo.toml` dependency requirement — all
  direct dependencies are already at their latest release.
- VS Code extension npm dependencies (`pnpm` lockfile).
- Changing per-job CI `targets:` / `components:` inputs,
  adding `targets` to `rust-toolchain.toml`, or any
  `.devcontainer/` change.
- Editing any `version = "..."` field — release-plz owns
  versions.
- A `docs/feature-log.md` entry — this is a toolchain and
  dependency change, not a user-facing feature.
