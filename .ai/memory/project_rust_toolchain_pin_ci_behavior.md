---
name: rust-toolchain-pin-also-pins-ci
description: The repo-root rust-toolchain.toml pins CI too (it overrides dtolnay's rustup default); full-pin uses @<version> action refs matching the pin
metadata:
  type: project
---

The repo pins Rust via a root `rust-toolchain.toml`
(`channel = "1.97.0"`, `components = ["clippy","rustfmt"]`, no
`targets`). This pins BOTH local dev AND CI: rustup honors the file over
`dtolnay/rust-toolchain`'s `rustup default`, so CI runs on the pinned
toolchain (CI log: `"1.97.0 ... overridden by rust-toolchain.toml"`). It
does NOT let CI float on stable.

**Critical gotcha:** `stable` and `1.97.0` are SEPARATE rustup installs
even at the same version. `dtolnay/rust-toolchain@stable` installs each
job's cross-compile targets (wasm32-wasip2, x86_64-apple-darwin, …) onto
the `stable` install — but the pin makes cargo use the `1.97.0` install,
which lacks those targets → cross-compile jobs fail with
`error[E0463]: can't find crate for core`.

**Fix in use (full-pin):** the CI workflows pin their action refs to
`dtolnay/rust-toolchain@1.97.0` (a real per-version branch of the action)
so dtolnay installs targets onto the SAME toolchain the pin selects.
CI == local exactly; no canary. `RUSTUP_TOOLCHAIN` is NOT used; the pin
declares no `targets`.

**To bump the toolchain:** edit BOTH `rust-toolchain.toml`'s `channel`
AND all 8 `dtolnay/rust-toolchain@<ver>` refs — ci.yml (2), coverage (1),
zed-release (1), release-plz (3), vscode-extension (1) — to the new
version, in one deliberate commit.

**Verification gotcha:** the Zed / vscode / release-binary cross-compile
jobs are path/release-gated, so they do NOT run on ordinary pushes.
Verify toolchain changes affecting them with a throwaway push-triggered
scratch workflow on a branch (a `workflow_dispatch`-only workflow can't
be dispatched from a branch — GitHub requires it on the default branch;
`on: push:` to the scratch branch works, and delete the branch after).

See plan `.ai/plans/2026-07-09-adopt-rust-1-97-toolchain.md`. Related:
[[clean-build-before-lint-verification]].
