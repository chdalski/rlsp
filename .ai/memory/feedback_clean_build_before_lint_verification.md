---
name: clean-build-before-lint-verification
description: Always `cargo clean` before verifying clippy/lints after a Rust toolchain change — incremental cache hides new-toolchain findings
metadata:
  type: feedback
---

After the Rust toolchain changes this session (e.g. `rustup` bump, or
local lagging CI's floating `@stable`), `cargo clippy` reuses the stale
incremental cache and does NOT re-lint unchanged code — so it silently
under-reports findings introduced/tightened by the new toolchain.

**Why:** during the Rust 1.97 adoption work, an incremental `cargo
clippy` after a mid-session 1.94→1.97 switch reported "only 13 test-code
findings," while a full `cargo clean` build revealed **91
`collapsible_if` + 1 `missing_const_for_fn`**, mostly in production
parser/LSP code. A task shipped a "green" clippy that wasn't; the
reviewer's mandatory clean-build re-check caught it.

**How to apply:** whenever the task is "make the clippy/lint gate green"
(or any lint verification) and the toolchain changed this session, run
`cargo clean && cargo clippy --workspace --all-targets -- -D warnings`
before trusting the result. CI does clean builds, so an incremental
local pass is not equivalent — treat an incremental clippy "0 warnings"
as unverified. See plan
`.ai/plans/2026-07-09-adopt-rust-1-97-toolchain.md`. Related:
[[potential-performance-optimizations]] documents a similar
verification-methodology discipline.
