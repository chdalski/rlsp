**Repository:** root
**Status:** Completed (2026-04-16)
**Created:** 2026-04-16

## Goal

Short-circuit the trailing-comment detection in
`LoadState::parse_node` by peeking the event stream for an
`Event::Comment` before computing `node_end_line(&value)`.
On the common case — a document with no comments near a
given value — the current code unconditionally computes
`node_end_line` for every mapping value and sequence item
even though `peek_trailing_comment` will immediately return
`None`. A baremetal flamegraph showed `node_end_line` and
`consume_leading_comments` each accounting for roughly
3.0% of total CPU time on
`throughput_style/rlsp/load/block_heavy`, a fixture with
zero comments. This plan eliminates the unconditional
`node_end_line` call on the no-comment path.

## Context

- `rlsp-yaml-parser/src/loader.rs:499–504` (mapping value
  path) and `:594–599` (sequence item path) both run:

  ```
  if !is_block_scalar(&value) {
      let value_end_line = node_end_line(&value);
      if let Some(trail) = peek_trailing_comment(stream, value_end_line)? {
          attach_trailing_comment(&mut value, trail);
      }
  }
  ```

  `node_end_line` is called before the stream is peeked
  for a `Comment` event, so the call fires on every
  iteration — including the vast majority of iterations
  on no-comment documents where no trailing comment
  exists.
- `rlsp-yaml-parser/src/loader/stream.rs:71–84` defines
  `peek_trailing_comment`, which already peeks for a
  Comment event (with a line match). The peek inside
  `peek_trailing_comment` cannot be bypassed by the
  caller, but adding an outer peek in the caller makes
  the `node_end_line` call conditional. `Peekable::peek`
  is effectively free (returns a cached `Option<&Item>`),
  so the second peek inside `peek_trailing_comment` is
  not a new cost.
- Profile source:
  `.ai/reports/flame-block_heavy-load.svg` on commit
  `5ebca0a`. Frame
  `rlsp_yaml_parser::loader::node_end_line` recorded
  3,185,241 samples (3.00% of 106,224,381 total). L5 added
  `#[inline]` to the helper — measurement of the combined
  effect of L5 + L2 is left to a post-merge baremetal
  bench run.
- This change is independent of L5 and can be reviewed in
  isolation. Combined with L5, both sources of per-entry
  overhead on the no-comment path are addressed.
- Related memory:
  `.ai/memory/potential-performance-optimizations.md`
  (candidate L2).

## Steps

- [x] Wrap the mapping value trailing-comment block at
      `loader.rs:499–504` with a `stream.peek()` Comment
      guard so `node_end_line` and
      `peek_trailing_comment` are only called when the
      next event is a `Comment`
- [x] Apply the same wrap to the sequence item block at
      `loader.rs:594–599`
- [x] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings
- [x] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and confirm 726 passed, 0 failed (351
      stream + 375 loader cases) — comment-attachment
      behavior must be unchanged

## Tasks

### Task 1: Guard trailing-comment detection with a stream peek (commit: `f53e5a0`)

Add an outer `stream.peek()` check for
`Event::Comment { .. }` so the `node_end_line` + inner
`peek_trailing_comment` chain is skipped entirely when no
Comment is next in the stream. The behavior of
`peek_trailing_comment` — specifically its line-match
check — is preserved: the outer peek only guards entry,
the line match still happens inside the helper.

- [x] In `rlsp-yaml-parser/src/loader.rs` mapping path
      (`:499–504`), wrap the existing body with a
      `matches!(stream.peek(), Some(Ok((Event::Comment {
      .. }, _))))` guard before computing
      `node_end_line`
- [x] In the sequence path (`:594–599`), apply the same
      wrapping guard
- [x] No change to `peek_trailing_comment` itself; no
      change to `node_end_line`, `is_block_scalar`, or
      `attach_trailing_comment`
- [x] `cargo fmt` produces zero diff
- [x] `cargo clippy --all-targets` produces zero warnings
- [x] `cargo test -p rlsp-yaml-parser` — all tests pass,
      including the loader smoke tests that cover
      trailing-comment attachment
- [x] `cargo test -p rlsp-yaml-parser --test conformance`
      — 726 passed, 0 failed (351 stream + 375 loader
      cases)
- [x] `cargo test` (full workspace) — all tests pass
- [x] Bench binary builds:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`
      exits 0

## Decisions

- **Change location:** the caller, not the callee. Moving
  the early-exit into `peek_trailing_comment` would
  require changing its signature (it currently takes
  `preceding_end_line`, which would become unnecessary on
  the no-comment path). Keeping the helper's signature
  stable and adding one `matches!` branch in the caller
  is the smaller, safer change.
- **Duplicate peek inside `peek_trailing_comment`:**
  accepted. `Peekable::peek` caches the next item and is
  effectively free on the second call — no extra event
  iteration occurs.
- **Behavior preservation:** the loader-conformance suite
  is the authoritative check that trailing-comment
  attachment is unchanged. Comment-heavy fixtures in the
  conformance suite exercise the trailing-comment path
  directly; any regression in attachment would fail those
  cases.
- **Measurement:** post-merge throughput and flamegraph
  verification are run baremetal by the user, outside the
  pipeline. The reviewer gates on code + tests + clippy.
