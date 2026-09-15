**Repository:** root
**Status:** Completed (2026-04-16)
**Created:** 2026-04-16

## Goal

Rewrite the inner loop of `find_value_indicator_offset`
— the scanner that classifies whether a line is an
implicit mapping key — to use the same
`memchr2(b':', b'#')` fast-path pattern already present
in `scan_plain_line_block`. Today the function walks
every byte of a candidate mapping-key line in a `while`
loop doing a multi-condition match on each byte. A
baremetal flamegraph of
`throughput_style/rlsp/load/block_heavy` (post-L3) shows
the function holding ~7.2% of total CPU as its own stack
frame across two call sites. The memchr-based rewrite
produces byte-identical output while scanning only the
context-sensitive bytes; all intermediate ASCII-safe
content is skipped by memchr in bulk. **The pipeline's
acceptance criterion is behavioral preservation — same
accept/reject decisions, same returned offsets — not a
throughput number; any throughput gain is verified by the
user in a subsequent baremetal bench run and is not a
gate here.**

## Context

- `rlsp-yaml-parser/src/event_iter/line_mapping.rs:68–172`
  defines `find_value_indicator_offset(trimmed: &str) ->
  Option<usize>`. It:
  1. Rejects a handful of indicator-prefix starts
     (`\t`, `%`, `@`, `` ` ``, `,`, `[`, `]`, `{`, `}`,
     `#`, `&`, `*`, `!`, `|`, `>`) up-front.
  2. Handles a leading quoted span at byte 0 (`"…"` or
     `'…'`) by skipping past the closing quote.
  3. Walks the remaining bytes one at a time, tracking
     `prev_was_space` and stopping at an unquoted `#`
     preceded by whitespace (comment start).
  4. Returns `Some(i)` the first time a `:` is followed
     by space/tab/newline/CR/EOL.
- `rlsp-yaml-parser/src/lexer/plain.rs:354–430` defines
  `scan_plain_line_block(content: &str) -> &str`. It uses
  `memchr2(b':', b'#')` from the `memchr` crate (already
  a dependency; it appears in `Cargo.toml`) to jump to
  the next context-sensitive byte, then processes bytes
  in between with a fast byte-level scan.
- The two scanners are **not merged** in this plan. Their
  contracts differ (one returns an offset, the other a
  content slice; they have different acceptance rules for
  `:` followed by `ns-plain-safe-char` vs.
  space/tab/newline). The scope is strictly: make
  `find_value_indicator_offset` faster without changing
  its external behavior.
- Scanner self-time in the flame reflects per-mapping-key
  byte-walking cost; on a 100KB block-heavy fixture with
  thousands of mapping entries each ~10–30 bytes of key,
  the loop iterates tens of thousands of times.
- `find_value_indicator_offset` already has an
  invariant-verification test at
  `event_iter/line_mapping.rs:189–230` that keeps
  `is_implicit_mapping_line` and the offset helper in
  lock-step on a set of accepted and rejected lines. The
  726-case conformance suite (351 stream + 375 loader)
  is the broader safety net.
- The function is hot but trivial to test in isolation;
  YAML 1.2 §7.4 (block mapping keys) plus §6.6 (comment
  boundaries) define the acceptance rules. The existing
  unit table must keep passing without modification.
- Related memory:
  `.ai/memory/potential-performance-optimizations.md`
  (remaining candidate L6). `memchr` is already a direct
  dependency of the parser — see `Cargo.toml` and the
  existing import in `lexer/plain.rs`.

## Steps

- [x] Rewrite
      `find_value_indicator_offset` in
      `rlsp-yaml-parser/src/event_iter/line_mapping.rs`
      to use `memchr2(b':', b'#', …)` for jumping to the
      next context-sensitive byte, matching the pattern
      in `scan_plain_line_block`
- [x] Preserve all three behavior cases exactly:
      (a) indicator-prefix rejection at byte 0;
      (b) leading quoted-span skip at byte 0 for `"` and
      `'`;
      (c) `:` followed by space/tab/newline/CR/EOL
      returns its offset; `:` followed by a non-space
      `ns-char` is skipped (not a value indicator);
      and stop at an unquoted `#` preceded by whitespace
      (comment boundary)
- [x] Keep the existing unit test
      `find_value_indicator_agrees_with_is_implicit_mapping_line`
      at
      `event_iter/line_mapping.rs:189–230` unchanged —
      pass/fail unchanged is the primary behavior gate
- [x] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings
- [x] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and confirm 726 passed, 0 failed (351
      stream + 375 loader cases) — mapping classification
      must be unchanged
- [x] Remove the L6 subsection from
      `.ai/memory/potential-performance-optimizations.md`
      "Remaining candidates" section, **and** delete the
      duplicate "Merge `find_value_indicator_offset` with
      `scan_plain_line_block`" entry from the "Other
      potential optimizations (not investigated)" list
      (per the "No completed items in memory" convention)
- [x] Update `.ai/memory/MEMORY.md` — remove the "L6
      merge-scan 7.3%" mention from the
      remaining-candidates list and add L6 to the
      Applied summary with a description matching the
      actual change (memchr fast-path in
      `find_value_indicator_offset`, not a merge of two
      scanners)

## Tasks

### Task 1: Rewrite `find_value_indicator_offset` with memchr (commit: `b57e344`)

Replace the per-byte `while let Some(&ch) = bytes.get(i)`
loop with a memchr-jumping loop that scans only the
context-sensitive bytes (`:` and `#`) and treats
everything between hits as safe content. Preserve the
existing behavior exactly — same return offsets, same
rejections, same quoted-span handling.

- [x] Function signature unchanged:
      `pub(in crate::event_iter) fn
      find_value_indicator_offset(trimmed: &str) ->
      Option<usize>`
- [x] Early rejection of indicator-prefix starts
      (`\t`, `%`, `@`, `` ` ``, `,`, `[`, `]`, `{`, `}`,
      `#`, `&`, `*`, `!`, `|`, `>`) kept as the first
      check
- [x] Leading `"` / `'` quoted-span skip at byte 0 kept;
      both escape rules preserved (`\"` in double-quoted
      span; `''` in single-quoted span)
- [x] Main scan uses `memchr::memchr2(b':', b'#', &bytes[pos..])`
      to locate the next candidate byte; returns `None`
      when memchr returns `None` and no earlier
      candidate has satisfied the value-indicator check
- [x] At each hit, check the byte immediately before
      (`bytes[hit - 1]`, or treat position 0 as
      non-whitespace) to determine comment-boundary
      context for `#`; UTF-8 continuation bytes are not
      whitespace so this check is byte-level safe
- [x] For `:` hits, check `bytes.get(hit + 1)` for
      `None | Some(b' ' | b'\t' | b'\n' | b'\r')` and
      return `Some(hit)`; otherwise advance past and
      continue
- [x] For `#` hits, return `None` when
      `hit == 0 || bytes[hit - 1] == b' ' || bytes[hit - 1] == b'\t'`;
      otherwise advance past and continue
- [x] The existing in-file unit test
      `find_value_indicator_agrees_with_is_implicit_mapping_line`
      continues to pass unmodified (verifies accepted
      and rejected line tables, including multi-byte
      `unicode_\u{00e9}: v`, quoted-key forms, and
      comment-looking-not-a-key lines)
- [x] `.ai/memory/potential-performance-optimizations.md`
      no longer contains the L6 "Remaining candidates"
      subsection **and** no longer contains the
      duplicate "Merge `find_value_indicator_offset`
      with `scan_plain_line_block`" entry in the "Other
      potential optimizations (not investigated)" list
- [x] `.ai/memory/MEMORY.md` description updated: L6
      removed from remaining-candidates list; Applied
      summary line includes L6 with a description that
      reflects the actual change (memchr fast-path in
      `find_value_indicator_offset`, not a merge of two
      scanners)
- [x] `cargo fmt` produces zero diff
- [x] `cargo clippy --all-targets` produces zero
      warnings
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

- **Scope: speed up only, not merge.** The memory entry
  described L6 as "merge `find_value_indicator_offset`
  with plain-scalar scan." The two scanners have
  different contracts (offset-of-indicator vs.
  content-slice) and different acceptance rules for `:`
  after a non-space char, so a true merge would force
  both callers through a single function that makes
  intermediate decisions. That is a bigger change than
  the target payoff justifies. Matching the memchr
  pattern in the existing scanner captures the same
  algorithmic win (skip ASCII-safe bytes in bulk) without
  touching the caller contracts.
- **Behavior preservation is the bar.** The existing
  `find_value_indicator_agrees_with_is_implicit_mapping_line`
  test locks the accepted/rejected contract. The 726-case
  conformance suite exercises real-world mapping
  classification. No new tests are authored in this plan
  — the existing tests are the gate.
- **Test advisor consultation.** Scanner refactors have
  many edge cases (quoted spans, multi-byte chars,
  comment boundaries, trailing colon, colon-tab). The
  developer consults the test advisor before implementing
  to confirm coverage is adequate, and again at the
  output gate to verify the completed code against the
  advisor's test list. Per
  `risk-assessment.md`, "complex interactions between
  components" and "the modified code has no existing test
  coverage beyond one unit test" both trigger test-engineer
  consultation.
- **No security consultation.** The function operates on
  a `&str` slice with bounded length. The memchr scanner
  is a pure refactor — no new code paths for untrusted
  input, no change to acceptance rules, no new
  deserialization. Conformance suite is the authoritative
  check that nothing changes observably.
- **Measurement:** this plan's reviewer acceptance
  criterion is **behavioral preservation only** —
  conformance 726/0 + zero clippy warnings + zero `fmt`
  diff + bench binary builds. Throughput and flamegraph
  verification are run baremetal by the user in a
  separate step. The reviewer does not gate on a
  throughput number; no minimum speedup is asserted
  here. This matches the convention used in plans L5,
  L2, L7, L1, and L3 (all behavioral-acceptance plans
  whose perf impact was measured after commit).
