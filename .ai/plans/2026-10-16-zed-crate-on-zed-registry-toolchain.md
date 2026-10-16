**Repository:** root
**Status:** NotStarted
**Created:** 2026-10-16

# Build the Zed Extension on Zed's Registry Toolchain

## Goal

Zed's registry rejects rlsp-yaml 0.1.3 (zed-industries/extensions#8014)
because the Zed crate declares `rust-version = "1.98"` while Zed's registry
CI builds extensions with Rust 1.90. Lower the crate's declared floor to
Zed's toolchain, make our CI build and lint the crate with that toolchain so
a crate Zed cannot build fails here first, and keep the next root toolchain
bump from raising either. The work is done when the fix is on `main` with
every triggered CI run green, version 0.1.4 is released through
`zed-release.yml`, its registry PR passes Zed's `package` and
`verification/cla-signed` checks, and #8014 is closed with a pointer to its
successor.

## Context

- Zed's registry CI
  (https://github.com/zed-industries/extensions/blob/main/.github/workflows/ci.yml)
  builds every extension with Rust 1.90, pinned in that file, through its
  `zed-extension` CLI, which runs a release build for `wasm32-wasip2`.
- `rlsp-yaml/integrations/zed/` is a standalone crate (its own empty
  `[workspace]`). Its `rust-version` followed the root toolchain pin up to
  1.98, which is what Zed rejects. With `rust-version = "1.90"`, Zed's CLI
  packages the crate on Rust 1.90, and a release build plus clippy with
  `-D warnings` pass on 1.90 against the committed lockfile.
- The root `rust-toolchain.toml` pins 1.98.1 for local dev and CI; rustup
  gives `RUSTUP_TOOLCHAIN` and `cargo +<toolchain>` precedence over that file.
  Every CI job installs its toolchain through
  `dtolnay/rust-toolchain@1.98.1`; the `check` job in `zed-release.yml` is
  currently one of them.
- The `check` job in `zed-release.yml` runs on pushes and pull requests to
  `main` that touch `rlsp-yaml/integrations/zed/**`; dispatch runs skip it.

## Steps

- [x] Clarify requirements with user
- [x] Design review of the approach
- [ ] Task 1: Build the Zed crate on Zed's registry toolchain
- [ ] Push to `main`; every triggered CI run succeeds, and the Zed `check`
      run's log shows rustc 1.90
- [ ] Release 0.1.4 through `zed-release.yml`; its registry PR passes
      Zed's `package` and `verification/cla-signed` checks; #8014 is closed
      with a pointer to that PR

## Tasks

### Task 1: Build the Zed crate on Zed's registry toolchain

The Zed crate's declared floor and our CI's Zed check use Zed's registry
toolchain, 1.90, instead of the root pin, so a crate Zed cannot build fails
in our CI first and a root toolchain bump leaves both alone.

- [ ] The Zed crate declares `rust-version = "1.90"`, with a comment beside
      it stating that it must not exceed the toolchain Zed's registry CI
      builds with
- [ ] The `check` job selects Rust 1.90 through a job-level
      `RUSTUP_TOOLCHAIN` value, installs that toolchain with the
      `wasm32-wasip2` target and clippy without `dtolnay/rust-toolchain`,
      and a comment on the job links Zed's registry CI as the source of the
      version and states that root toolchain bumps do not touch it
- [ ] The `check` job runs a release build of the crate for
      `wasm32-wasip2` and clippy with `-D warnings`, both on Rust 1.90
- [ ] A change to `.github/workflows/zed-release.yml` triggers the `check`
      job on push and on pull request
- [ ] A reader of `rust-toolchain.toml` learns that the Zed `check` job is
      excluded from a toolchain bump; `.github/dependabot.yml` refers to the
      toolchain refs without a fixed count
- [ ] Root `CLAUDE.md` lets an agent install Rust 1.90 and run the same Zed
      build and clippy commands the `check` job runs, and its Conventions
      record that the Zed crate follows Zed's registry toolchain, not the
      root pin
- [ ] A local run on Rust 1.90 shows that the crate declaring
      `rust-version = "1.91"` fails the build with cargo's "requires rustc"
      error; the probe leaves no change in the repository
- [ ] `actionlint` reports no findings for
      `.github/workflows/zed-release.yml`

## Decisions

- **Fixed guard version, separate from `rust-version`:** deriving the
  guard from `rust-version` would raise it whenever the floor is raised,
  which is how 1.98 reached Zed; cargo itself rejects a floor above the
  guard.
- **No automatic detection of Zed's toolchain:** a fixed guard can only
  become stricter than Zed after Zed moves up, never wrong; raising it is a
  deliberate step after checking Zed's registry CI.
- **`RUSTUP_TOOLCHAIN` with a plain `rustup toolchain install` instead of
  `dtolnay/rust-toolchain@1.90`:** every remaining `dtolnay` ref matches
  the root pin, so a toolchain bump never touches the Zed job, and the
  job-level variable keeps `Swatinem/rust-cache`'s key on 1.90.
- **No nested `rust-toolchain.toml` in the Zed crate:** rustup reads it by
  working directory only, so the documented `--manifest-path` commands run
  from the repository root would keep using the root pin.
- **Release-path gap left open:** dispatch runs skip the `check` job; Zed
  would have to lower its toolchain between our last Zed change and a
  release, and Zed's own PR check remains the backstop.
- **Release 0.1.4 instead of reusing 0.1.3:** the `zed-v0.1.3` tag points at
  the commit that declares 1.98.

## Non-Goals

- Running Zed's `zed-extension` CLI in our CI.
- Changing the root toolchain pin or the other crates' `rust-version`.
- Linting the Zed crate on the root pin as well.
