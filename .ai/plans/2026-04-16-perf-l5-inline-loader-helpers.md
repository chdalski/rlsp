**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-16

## Goal

Add `#[inline]` to the loader helper functions
`node_end_line` and `is_block_scalar` so that the compiler
inlines them at every call site. A baremetal flamegraph of
`throughput_style/rlsp/load/block_heavy` on commit `5ebca0a`
showed `node_end_line` accounting for 3.0% of total CPU
time as its own stack frame — evidence that rustc is not
inlining either helper despite both being `const fn`.
Inlining should collapse the per-entry call overhead on
every mapping value and sequence item.

## Context

- `rlsp-yaml-parser/src/loader.rs:799` defines
  `node_end_line(node: &Node<Span>) -> usize` — a match
  over `Node` variants returning `loc.end.line`.
- `rlsp-yaml-parser/src/loader.rs:816` defines
  `is_block_scalar(node: &Node<Span>) -> bool` — a
  `matches!` check on `ScalarStyle::Literal | Folded`.
- Both functions are called on every mapping value and
  every sequence item in `parse_node`
  (`loader.rs:499–504` and `:594–599`), in the guarded
  block that decides whether to peek for a trailing
  comment.
- `const fn` does not imply `#[inline]`. In a release
  build across crate boundaries (none apply here, these
  are private), rustc usually inlines small functions but
  does not guarantee it — the flamegraph is authoritative.
- Profile source: `.ai/reports/flame-block_heavy-load.svg`.
  Frame `rlsp_yaml_parser::loader::node_end_line`
  recorded 3,185,241 samples (3.00% of 106,224,381 total).
  No frame for `is_block_scalar` is visible, so it may
  already be inlined — adding `#[inline]` to it is
  belt-and-braces at zero cost.
- Measured regression: `block_heavy` rlsp/load throughput
  dropped from 60.8 MiB/s (container, `05d21fa`) to
  51.4 MiB/s baremetal HEAD, a 15–16% slowdown relative
  to the recorded baseline. This plan does not claim to
  close that entire gap — it targets the specific 3.0%
  self-time visible on this one helper.
- Related memory:
  `.ai/memory/potential-performance-optimizations.md`
  (candidates L2, L5).

## Steps

- [ ] Add `#[inline]` to `node_end_line` in `loader.rs`
- [ ] Add `#[inline]` to `is_block_scalar` in `loader.rs`
- [ ] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings
- [ ] Confirm debug symbols build cleanly for a subsequent
      flamegraph run: `CARGO_PROFILE_BENCH_DEBUG=true
      cargo bench -p rlsp-yaml-parser --bench throughput
      --no-run`

## Tasks

### Task 1: Inline loader helpers

Add `#[inline]` attributes to both helper functions so the
compiler stops emitting out-of-line calls for them at
their two mapping and two sequence call sites in
`parse_node`.

- [ ] `#[inline]` on `node_end_line` at
      `rlsp-yaml-parser/src/loader.rs:799`
- [ ] `#[inline]` on `is_block_scalar` at
      `rlsp-yaml-parser/src/loader.rs:816`
- [ ] `cargo fmt` produces zero diff
- [ ] `cargo clippy --all-targets` produces zero warnings
- [ ] `cargo test -p rlsp-yaml-parser` — all tests pass
- [ ] `cargo test` — all workspace tests pass
- [ ] Bench binary builds:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`
      exits 0

## Decisions

- **Attribute choice:** `#[inline]` (not `#[inline(always)]`).
  The functions are small enough that rustc is virtually
  guaranteed to inline them once hinted. Forcing
  `inline(always)` adds noise without additional benefit
  and removes the compiler's freedom to decline inlining
  in cold paths.
- **Scope:** only the two helpers named above. Other
  `const fn` helpers in `loader.rs` are not visible in
  the flame and are out of scope.
- **Measurement:** post-merge throughput and flamegraph
  verification are run baremetal by the user, outside the
  pipeline. The reviewer gates on code + tests + clippy,
  not on a throughput number — the throughput check is a
  follow-up activity recorded in the memory file once
  results are in.
