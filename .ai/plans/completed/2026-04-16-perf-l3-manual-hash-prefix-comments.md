**Repository:** root
**Status:** Completed (2026-04-16)
**Created:** 2026-04-16

## Goal

Replace the four `format!("#{text}")` call sites in the
loader that prepend a `#` sigil to every comment event
text with a direct manual concatenation
(`String::with_capacity(text.len() + 1)` + `push('#')` +
`push_str(text)`). `format!` routes through
`fmt::Display`/`Formatter` machinery and is noticeably
slower than direct push for a fixed-prefix concat; for
comment-heavy YAML documents — CI configuration,
documented Kubernetes manifests, annotated fixture files
— every comment event pays the overhead. The current
benchmark fixtures (block_heavy, scalar_heavy, etc.) have
no comments, so this change does not move those numbers;
the win is on comment-heavy real-world workloads.

## Context

- Four identical call sites convert an
  `Event::Comment { text: &str }` payload to a
  `String` with a `#` prefix:
  - `rlsp-yaml-parser/src/loader.rs:640` —
    `self.pending_leading.push(format!("#{text}"));` in
    `parse_node`'s `Event::Comment` arm
  - `rlsp-yaml-parser/src/loader/stream.rs:37` —
    `doc_comments.push(format!("#{text}"));` in
    `consume_leading_doc_comments`
  - `rlsp-yaml-parser/src/loader/stream.rs:56` —
    `leading.push(format!("#{text}"));` in
    `consume_leading_comments`
  - `rlsp-yaml-parser/src/loader/stream.rs:80` —
    `return Ok(Some(format!("#{text}")));` in
    `peek_trailing_comment`
- `format!("#{text}")` expands to
  `alloc::fmt::format(format_args!("#{}", text))` which
  builds a `Formatter`, invokes `Display::fmt`, and
  writes into a `String`. For a pure-ASCII prefix + one
  borrowed `&str`, the direct form is roughly 2–3× faster
  (benchmarked in other Rust projects; also visible
  indirectly on a comment-heavy flame).
- The prefix (`#`) is always a single ASCII byte and the
  suffix is the pre-stripped comment text from the lexer,
  so `String::with_capacity(text.len() + 1)` sizes the
  buffer exactly — no reallocation.
- No behavior change: the output string is byte-identical
  to the `format!` output for any well-formed `&str`
  (UTF-8 preserved, no format specifiers in play).
- A small private helper in `loader/stream.rs`
  (`pub(super) fn with_hash_prefix(text: &str) -> String`)
  centralizes the pattern so all four call sites use one
  canonical implementation. The helper's single
  responsibility also lets us add `#[inline]` in one
  place.
- Related memory:
  `.ai/memory/potential-performance-optimizations.md`
  (candidate L3).
- Flame on block_heavy shows no visible `core::fmt` or
  `format_args` frames, because block_heavy has zero
  comments. This change is a correctness/efficiency
  improvement that benefits fixtures not currently in the
  benchmark set.

## Steps

- [x] Add a private helper
      `pub(super) fn with_hash_prefix(text: &str) ->
      String` in `rlsp-yaml-parser/src/loader/stream.rs`
      that builds the prefixed string directly, marked
      `#[inline]`
- [x] Replace the four `format!("#{text}")` call sites
      with `with_hash_prefix(text)` (or the path-qualified
      form from `loader.rs`, re-exported via the
      `loader::stream` module's existing `pub(super)`
      imports)
- [x] Remove the L3 subsection from
      `.ai/memory/potential-performance-optimizations.md`
      (it documents the pre-change `format!` code
      pattern and line numbers that no longer exist
      after this change; per project memory convention
      completed items do not live in the follow-up queue)
- [x] Update `.ai/memory/MEMORY.md` — remove
      "L3 format!-for-comments" from the remaining
      candidates list and add the L3 commit to the
      Applied summary (mirroring the L5/L2/L7/L1
      pattern already present)
- [x] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings
- [x] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and confirm 726 passed, 0 failed (351
      stream + 375 loader cases) — comment-text
      preservation must be unchanged

## Tasks

### Task 1: Replace `format!` with direct prefix concat (commit: `1763787`)

Extract the `#`-prefix-prepend pattern into a single
helper, apply `#[inline]`, and replace all four call
sites. The helper produces a byte-identical `String` so
no behavior changes and no test fixtures need updating.

- [x] `pub(super) fn with_hash_prefix(text: &str) ->
      String` added to
      `rlsp-yaml-parser/src/loader/stream.rs`, marked
      `#[inline]`, with a body of:
      `let mut s = String::with_capacity(text.len() + 1);
      s.push('#'); s.push_str(text); s`
- [x] Call site at `loader/stream.rs:37` uses
      `with_hash_prefix(text)`
- [x] Call site at `loader/stream.rs:56` uses
      `with_hash_prefix(text)`
- [x] Call site at `loader/stream.rs:80` uses
      `with_hash_prefix(text)`
- [x] Call site at `loader.rs:640` imports and uses
      `with_hash_prefix` (add it to the existing
      `use stream::{…}` import group)
- [x] No other behavior or signature changes
- [x] `.ai/memory/potential-performance-optimizations.md`
      no longer contains the L3 subsection (removed per
      the "No completed items in memory" convention)
- [x] `.ai/memory/MEMORY.md` description for
      `potential-performance-optimizations.md` updated:
      "L3 format!-for-comments" removed from the
      remaining-candidates list and L3 added to the
      Applied summary alongside L5/L2/L7/L1
- [x] `cargo fmt` produces zero diff
- [x] `cargo clippy --all-targets` produces zero warnings
- [x] `cargo test -p rlsp-yaml-parser` — all tests pass,
      including tests in `loader/comments.rs` and
      `loader/stream.rs` that verify comment text is
      stored with the `#` prefix
- [x] `cargo test -p rlsp-yaml-parser --test conformance`
      — 726 passed, 0 failed (351 stream + 375 loader
      cases)
- [x] `cargo test` (full workspace) — all tests pass
- [x] Bench binary builds:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`
      exits 0

## Decisions

- **Helper over inlined 3-line pattern.** Four identical
  sites justifies a single helper. Inlining the three
  lines at each site would duplicate the capacity
  calculation and obscure the intent. `#[inline]` on the
  helper keeps per-call overhead at the level of the
  inlined pattern.
- **Helper lives in `loader/stream.rs`.** Three of four
  call sites are already in that file; the fourth
  (`loader.rs`) already imports several `pub(super)`
  items from `loader::stream` via
  `use stream::{consume_leading_comments, ... }` at
  `loader.rs:38`. Adding `with_hash_prefix` to that
  import group keeps related helpers co-located.
- **Exact capacity.** `text.len() + 1` is always
  correct: `#` is one ASCII byte, `text` is a valid
  `&str`. `String::with_capacity` avoids the single
  reallocation `format!` may or may not do depending on
  internal buffer heuristics.
- **Measurement:** block_heavy and the current benchmark
  fixtures have zero comments, so bench throughput will
  not visibly change. The correctness claim is that
  comment-bearing tests continue to produce the same AST,
  which is verified by the 726-case conformance suite
  and the comment-focused unit tests already in the
  module.
