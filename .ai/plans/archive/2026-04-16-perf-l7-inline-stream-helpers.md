**Repository:** root
**Status:** Completed (2026-04-16)
**Created:** 2026-04-16

## Goal

Add `#[inline]` to the loader stream helpers
`consume_leading_comments` and `next_from` in
`rlsp-yaml-parser/src/loader/stream.rs` so the compiler
folds them into their call sites. A baremetal flamegraph
of `throughput_style/rlsp/load/block_heavy` on commit
`5ebca0a` showed `consume_leading_comments` accounting for
3.0% of total CPU time as its own stack frame — on a
fixture with zero comments, meaning the entire 3.0% is
per-call overhead around a peek that immediately returns.
`next_from` is the internal error-conversion helper
called from every stream-pulling site in the loader; it
is small enough that inlining yields consistent
call-site-folded code paths.

## Context

- `rlsp-yaml-parser/src/loader/stream.rs:52–60` defines
  `consume_leading_comments(stream) -> Result<Vec<String>>`.
  The body is:
  - `let mut leading = Vec::new();` (empty `Vec::new()`
    does not allocate on the heap)
  - `while matches!(stream.peek(), Some(Ok((Event::Comment { .. }, _))))` — single cheap branch
  - `Ok(leading)`
  On a no-comment document the while loop never enters
  and the function returns an empty `Vec`. The cost
  is entirely the non-inlined call itself: prologue,
  peek, branch, epilogue.
- `rlsp-yaml-parser/src/loader/stream.rs:15–24` defines
  `next_from`, a 9-line pattern-match that lifts
  `Option<Result<T, Error>>` into `Result<Option<T>>`. It
  is called from every stream-pulling site — notably
  inside `parse_node`, `consume_leading_doc_comments`,
  `consume_leading_comments`, and `peek_trailing_comment`.
- `consume_leading_comments` is invoked from two sites in
  `parse_node`: the top of every mapping-loop iteration
  (`loader.rs:453`) and every sequence-loop iteration
  (`loader.rs:552`). On `block_heavy` with thousands of
  nested collection entries, this is a per-entry call.
- Profile source: `.ai/reports/flame-block_heavy-load.svg`.
  Frame
  `rlsp_yaml_parser::loader::stream::consume_leading_comments`
  recorded 3,177,241 samples (2.99% of 106,224,381 total).
- L5 already inlined `node_end_line` and
  `is_block_scalar`. This plan applies the same treatment
  to two hot stream helpers. `consume_leading_doc_comments`
  and `peek_trailing_comment` are out of scope — they are
  either not on the hot path (`consume_leading_doc_comments`
  fires once per document) or already short-circuited at
  their callers by L2 (`peek_trailing_comment`).
- Related memory:
  `.ai/memory/potential-performance-optimizations.md`
  (candidate L7).

## Steps

- [x] Add `#[inline]` to `consume_leading_comments` in
      `loader/stream.rs`
- [x] Add `#[inline]` to `next_from` in `loader/stream.rs`
- [x] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings
- [x] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and confirm 726 passed, 0 failed (351
      stream + 375 loader cases) — comment handling must
      be unchanged
- [x] Confirm debug symbols build cleanly:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`

## Tasks

### Task 1: Inline stream helpers (commit: `faa4f92`)

Add `#[inline]` attributes to both helper functions so
the compiler stops emitting out-of-line calls for them at
the per-entry and per-event call sites in `parse_node`,
`consume_leading_doc_comments`, and
`peek_trailing_comment`.

- [x] `#[inline]` on `consume_leading_comments` at
      `rlsp-yaml-parser/src/loader/stream.rs:52`
- [x] `#[inline]` on `next_from` at
      `rlsp-yaml-parser/src/loader/stream.rs:15`
- [x] No other signature, body, or caller changes
- [x] `cargo fmt` produces zero diff
- [x] `cargo clippy --all-targets` produces zero warnings
- [x] `cargo test -p rlsp-yaml-parser` — all tests pass
- [x] `cargo test -p rlsp-yaml-parser --test conformance`
      — 726 passed, 0 failed (351 stream + 375 loader
      cases)
- [x] `cargo test` (full workspace) — all tests pass
- [x] Bench binary builds:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`
      exits 0

## Decisions

- **Attribute choice:** `#[inline]` (not `#[inline(always)]`).
  Matches the L5 decision: both helpers are small; rustc
  will inline once hinted. Forcing `inline(always)`
  removes the compiler's freedom to decline on cold paths
  without measurable benefit.
- **Scope:** only the two helpers named above.
  `consume_leading_doc_comments` is per-document (cold);
  `peek_trailing_comment` is already skipped at its
  callers after L2, so inlining it would only help the
  rare path where a trailing comment is actually present.
- **Measurement:** post-merge throughput and flamegraph
  verification are run baremetal by the user, outside the
  pipeline. The reviewer gates on code + tests + clippy.
