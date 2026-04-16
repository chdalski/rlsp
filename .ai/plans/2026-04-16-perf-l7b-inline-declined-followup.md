**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-16

## Goal

Complete the L7 inline work for the two
`rlsp-yaml-parser/src/loader/stream.rs` helpers that
rustc's inlining cost model declined despite the
`#[inline]` hint applied in commit `3f493a8` (L7) and
`d586012` (L3). LLVM IR emitted from the release build on
commit `704d5f8` confirms both functions are still
defined as `internal fastcc` symbols with live `invoke`
calls:

- `consume_leading_comments` — 1 definition, 2 runtime
  call sites in `parse_node` (mapping and sequence
  loops). Present as its own 2.96% stack frame in the
  post-L4 block_heavy flamegraph.
- `with_hash_prefix` — 1 definition, 4 runtime call
  sites (3 in `stream.rs` + 1 in `loader.rs`). Not
  visible in block_heavy (which has zero comments) but
  pays a call-site cost per comment event on any
  comment-heavy fixture.

Split `consume_leading_comments` into a tiny inlinable
fast-path wrapper plus an out-of-line slow-path function
so the hot no-comment case becomes a single peek-and-return
that rustc will inline. Change `with_hash_prefix` to
`#[inline(always)]` to force inlining of the trivial
3-line allocating helper that rustc's cost model is too
conservative to inline.

## Context

- **Non-inlined status confirmed by LLVM IR.** Release
  build artifact at
  `target/release/deps/rlsp_yaml_parser-*.ll` emitted
  with `cargo rustc -p rlsp-yaml-parser --release --lib
  -- --emit=llvm-ir -O` shows:
  - `define internal fastcc void
    @_ZN16rlsp_yaml_parser6loader6stream24consume_leading_comments...`
    and two `invoke fastcc` call sites at the mapping
    and sequence loops inside `parse_node`.
  - `define internal fastcc void
    @_ZN16rlsp_yaml_parser6loader6stream16with_hash_prefix...`
    and four `invoke fastcc` call sites across
    `parse_node`'s Comment arm,
    `consume_leading_doc_comments`,
    `consume_leading_comments`'s inner body, and
    `peek_trailing_comment`.
- **Why rustc declined:**
  - `consume_leading_comments` — the `while matches!`
    loop makes the cost model treat the function as
    "big" to inline at every caller; the `Vec<String>`
    return type further raises the estimate.
  - `with_hash_prefix` — functions that heap-allocate
    (create a `String`) are held out-of-line more
    conservatively than non-allocating ones.
- **Fast-path / slow-path split idiom.** A widely used
  Rust pattern: a small inlinable wrapper handles the
  common case (here: the stream's next event is not a
  Comment, return an empty `Vec` immediately) and
  delegates to an out-of-line helper only for the rare
  loop case. The caller ends up with one inlined peek
  plus an early return on the hot path — no function-call
  prologue, no argument shuffling, no return-value
  construction.
- **`#[inline(always)]` is appropriate here.** The
  helper is three lines
  (`String::with_capacity(text.len() + 1)`, `push('#')`,
  `push_str(text)`). Forcing inline at 4 call sites
  grows caller code minimally — a few bytes per site —
  with no hot loop getting its body duplicated. It is
  the right level of aggressiveness for a tiny
  allocation helper, consistent with the project's
  existing use of `#[inline(always)]` in performance-
  critical paths only when `#[inline]` has measurably
  failed.
- **Block_heavy bench impact expected small.** On a
  no-comment fixture, `consume_leading_comments` is
  called on every block-collection iteration but takes
  the cheapest path; the 2.96% flame cost is almost
  entirely function-call overhead. Eliminating that
  overhead should shift those cycles into the caller's
  inlined fast path — likely +1–2% throughput on
  block_heavy, possibly within bench noise.
  `with_hash_prefix` changes will not move block_heavy
  at all (zero calls); they target comment-heavy
  fixtures that are not in the current benchmark set.
- **Related memory:** the L7 follow-up note in
  `.ai/memory/potential-performance-optimizations.md`
  records this investigation; this plan resolves that
  note.

## Steps

- [ ] In `rlsp-yaml-parser/src/loader/stream.rs`, split
      `consume_leading_comments` into two functions: a
      `#[inline]`-marked wrapper that peeks the next
      event and returns `Ok(Vec::new())` immediately when
      it is not a `Comment`, and a private
      `consume_leading_comments_slow` helper that
      contains the existing `while` loop body. The
      wrapper delegates to the slow helper only when a
      Comment is confirmed present.
- [ ] Change `with_hash_prefix` in
      `rlsp-yaml-parser/src/loader/stream.rs` from
      `#[inline]` to `#[inline(always)]`.
- [ ] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings.
- [ ] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and confirm 726 passed, 0 failed (351
      stream + 375 loader cases) — comment handling must
      be unchanged.
- [ ] Verify via LLVM IR that both functions are now
      fully inlined: rebuild with `cargo rustc -p
      rlsp-yaml-parser --release --lib -- --emit=llvm-ir
      -O` and confirm `grep -c "define.*consume_leading_comments"`
      and `grep -c "define.*with_hash_prefix"` both
      return `0` (no standalone definitions remain).
      `consume_leading_comments_slow` is allowed to
      appear as a standalone definition — that is the
      intended cold path.
- [ ] Update the L7 follow-up note in
      `.ai/memory/potential-performance-optimizations.md`
      to record this resolution (link both functions to
      the L7b commit SHA and note the LLVM IR
      verification).
- [ ] Update `.ai/memory/MEMORY.md` index to reflect the
      follow-up resolution.

## Tasks

### Task 1: Complete inline work for stream helpers

Split the hot fast path out of `consume_leading_comments`
and promote `with_hash_prefix` to `#[inline(always)]`.
Behavior is byte-identical to the current release
binary; only the compiler's inlining choices change.

- [ ] `consume_leading_comments` is a short
      `#[inline]`-marked wrapper containing exactly:
      peek check, early `Ok(Vec::new())` return on
      non-Comment, delegation to
      `consume_leading_comments_slow` on Comment
- [ ] `consume_leading_comments_slow` is a private
      (non-`pub(super)`) helper carrying the original
      `while matches!` loop body. It is NOT marked
      `#[inline]` — the intent is that it stays out of
      line as the cold path
- [ ] `with_hash_prefix` is now marked
      `#[inline(always)]`; signature and body are
      otherwise unchanged
- [ ] No other signature changes anywhere
- [ ] LLVM IR verification: after
      `cargo rustc -p rlsp-yaml-parser --release --lib
      -- --emit=llvm-ir -O`, running
      `grep -c "define.*consume_leading_comments[^_]"`
      on the emitted `.ll` file returns `0` (no
      standalone wrapper definition survives), and
      `grep -c "define.*with_hash_prefix"` returns `0`
      (no standalone definition survives)
- [ ] `.ai/memory/potential-performance-optimizations.md`
      "Follow-ups surfaced during execution" section
      updated to record the resolution, including the
      new commit SHA, the LLVM IR verification, and the
      fact that `consume_leading_comments_slow` is the
      intentional out-of-line cold path
- [ ] `.ai/memory/MEMORY.md` index line updated to
      reflect L7b as applied
- [ ] `cargo fmt` produces zero diff
- [ ] `cargo clippy --all-targets` produces zero
      warnings
- [ ] `cargo test -p rlsp-yaml-parser` — all tests pass
- [ ] `cargo test -p rlsp-yaml-parser --test conformance`
      — 726 passed, 0 failed (351 stream + 375 loader
      cases)
- [ ] `cargo test` (full workspace) — all tests pass
- [ ] Bench binary builds:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`
      exits 0

## Decisions

- **Fast/slow split over `#[inline(always)]` for
  `consume_leading_comments`.** Forcing inlining of the
  full function (loop body included) at every caller
  would duplicate the `while matches!` scan and its
  Vec-growing push into `parse_node`'s mapping and
  sequence loops — bloating the caller and potentially
  harming code-size / i-cache for the hot path that
  never takes the loop. The wrapper/slow-path split
  keeps the common case free while leaving the rare
  comment-loop path's code size exactly where it was.
- **`#[inline(always)]` for `with_hash_prefix`.** The
  helper is three lines with no control flow; code-size
  impact at 4 call sites is a handful of bytes. The
  trade-off that justified the fast/slow split does
  not apply — there is no loop body to duplicate.
- **`consume_leading_comments_slow` is deliberately not
  `pub(super)`.** It is a local implementation detail
  of `consume_leading_comments`. Keeping its visibility
  tight prevents accidental direct calls that would
  bypass the peek-first contract.
- **Test advisor consultation.** Same refactor pattern
  as the L6 memchr rewrite — small code change, many
  surrounding tests cover the behavior, but the hot
  path is now split. Consult the test-engineer at both
  the input gate (test list covering empty-stream,
  Comment-present, multi-Comment, and
  non-Comment-non-Event-end cases for both functions)
  and the output gate (sign-off on the completed
  implementation).
- **No security consultation.** Internal refactor of
  already-reviewed code; no trust-boundary change, no
  new untrusted-input handling.
- **Measurement.** Pipeline acceptance is behavioral
  (726/0 conformance, clippy zero, fmt zero, bench
  binary builds) plus the LLVM IR verification in the
  task sub-task list. Throughput verification is run
  baremetal by the user in a follow-up step. Expected
  outcome on block_heavy rlsp/load: +1–2% (likely
  within noise). No throughput number is gated.
