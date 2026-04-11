**Repository:** root
**Status:** InProgress
**Created:** 2026-04-11

# rlsp-yaml-parser throughput follow-up

## Goal

Close part of the remaining throughput gap between
`rlsp-yaml-parser` and `libfyaml` by removing three
internal overheads the earlier optimisation plans left in
place — all without changing any observable parser
behaviour. The user wants the gap narrowed on the
throughput-oriented fixtures in `docs/benchmarks.md` while
the full test suite, conformance suite, and clippy all
stay green. No numeric target; each task must be
demonstrably faster (or at worst no-op) on the relevant
fixture, and no fixture may regress.

## Context

### Where we are today

A fresh `cargo bench -p rlsp-yaml-parser --bench
throughput` run on 2026-04-11 reproduces the numbers
published in `rlsp-yaml-parser/docs/benchmarks.md` within
normal run-to-run noise. The same bench was then re-run
on the user's **bare-metal** host to validate the
container numbers and produce a cleaner baseline for
verification runs. Both sets of medians:

| Fixture     | rlsp/events (container) | libfyaml (container) | ratio  | rlsp/events (bare) | libfyaml (bare) | ratio  |
|-------------|------------------------:|---------------------:|-------:|-------------------:|----------------:|-------:|
| tiny_100B   |    1.50 µs              |     3.16 µs          | **0.47×** |      1.61 µs    |    2.85 µs      | **0.56×** |
| medium_10KB |  106.76 µs              |    95.56 µs          |  1.12× |    108.61 µs      |   90.31 µs      |  1.20× |
| large_100KB |    1.07 ms              |   819.0 µs           |  1.31× |      1.078 ms     |  814.5 µs       |  1.32× |
| huge_1MB    |   10.15 ms              |    7.80 ms           |  1.30× |     10.26 ms      |    7.63 ms      |  1.34× |
| block_heavy |    1.257 ms             |  930.4 µs            |  1.35× |      1.162 ms     |  904.4 µs       |  1.28× |
| block_seq   |  739.25 µs              |  394.6 µs            |  1.87× |    711.53 µs      |  403.6 µs       |  1.76× |
| flow_heavy  |  960.15 µs              |    1.117 ms          | **0.86×** |    921.10 µs    |    1.088 ms     | **0.85×** |
| scalar_hvy  |  531.34 µs              |  462.4 µs            |  1.15× |    541.86 µs      |  420.4 µs       |  1.29× |
| mixed       |    1.081 ms             |  824.8 µs            |  1.31× |      1.033 ms     |  822.9 µs       |  1.26× |
| k8s_3KB     |   38.63 µs              |   27.11 µs           |  1.42× |     35.84 µs      |   26.47 µs      |  1.35× |

Takeaways from the two-host comparison:

- **Relative gap is stable** — most fixtures move by
  ±2–8% between hosts and the ordering is preserved.
  Neither run paints a materially different picture.
- **Bare-metal confidence intervals are tighter** —
  `block_heavy` dropped from 18% outliers and a wide CI
  in-container to 2% outliers and a ±0.1% CI on bare
  metal. This matters because Task 2's expected
  per-fixture improvement is small (2–5%) and could be
  swallowed by in-container noise.
- **libfyaml relatively gained more from bare metal**
  on `scalar_heavy` (9% faster) and `block_heavy` (3%
  faster) — the C code benefits from cache locality
  more than rlsp's bounds-checked Rust. The measured
  gap on those fixtures is therefore slightly wider on
  bare metal than the container run suggested.
- **The gap is smallest on `flow_heavy`** (rlsp wins by
  ~15% on both hosts), **largest on `block_sequence`**
  (1.76× bare / 1.87× container), and sits at ~1.3× on
  the general mixed workloads.

### Measurement protocol

Each task in this plan runs its verification bench
**in-container** for iteration speed. The developer
compares medians against `docs/benchmarks.md` manually —
**do not trust criterion's "Performance has improved /
regressed" lines in the log output**, because criterion's
stored baseline (`target/criterion/<group>/<fixture>/base/`)
may contain cross-host numbers and its comparisons then
reflect host differences, not code changes.

Regression policy:

- If an in-container re-run shows a median outside
  criterion's confidence interval in the wrong direction,
  re-run once (containers are noisy).
- If the regression persists, treat it as a real
  regression and fail the task.
- After the final task, the user will re-run the bench
  on bare metal once more for the authoritative
  end-to-end delta. That run is the final verification
  of the plan's goal.

Per-task fixture watch list (where each optimisation is
expected to register cleanly):

| Task | Fixture to watch                | Expected direction |
|------|---------------------------------|--------------------|
| 1    | `huge_1MB`, `large_100KB`       | faster (10–25%)    |
| 2    | `scalar_heavy`, `mixed`         | faster (2–5%)      |
| 3    | `scalar_heavy`, `mixed`, `block_sequence` | faster (3–8%) |

No fixture may regress on any task — the "expected
direction" column is where the win should show up, not
where the developer is allowed to take a hit elsewhere.

### Prior optimisation plans (context, not scope)

- `2026-04-10-unicode-position-safety-and-lazy-pos.md`
  introduced lazy `Pos` tracking, dropped `char_offset`
  from `Pos`, and replaced `advance_pos_past_line` with a
  lightweight const helper. That optimisation landed in
  `src/lines.rs` but did **not** reach the duplicate
  helper that still lives in `src/lexer.rs` — Task 1 of
  this plan finishes the migration.
- `2026-04-10-byte-level-scanning-and-memchr.md` turned
  the scalar inner loops into byte-level `memchr`
  scanners. That covered the hottest loop but not the
  downstream per-event work in the dispatcher and span
  emission — Tasks 2 and 3 address those.
- `2026-04-11-code-improvements.md` split `lib.rs` into
  `event_iter/*` submodules, collapsed boolean state into
  enums, and retired dead predicates. That landed today
  and defines the current file layout this plan targets.

### Where the remaining overhead lives

Investigation identified three spots that still do work
`libfyaml` does not:

**1. Duplicate `pos_after_line` in `src/lexer.rs:501`.**
The fast, const-fn version is already present at
`src/lines.rs:376`:

```rust
const fn pos_after_line(line: &Line<'_>) -> Pos {
    Pos {
        byte_offset: line.offset + line.content.len()
            + line.break_type.byte_len(),
        line: line.pos.line + 1,
        column: 0,
    }
}
```

But `src/lexer.rs` still defines an O(n) variant that
walks every character of every consumed line:

```rust
pub fn pos_after_line(line: &Line<'_>) -> Pos {
    let mut pos = line.pos;
    for ch in line.content.chars() {
        pos = pos.advance(ch);
    }
    line.break_type.advance(pos)
}
```

The lexer version is called once per line consumed, from
19 call sites across `src/lexer.rs`, `src/lexer/block.rs`,
`src/lexer/plain.rs`, `src/lexer/quoted.rs`, and
`src/lexer/comment.rs`. For a 1 MB `mixed` fixture with
~30 k lines and ~33 chars per line, that is ~1 M `Pos`
advances of pure position-tracking overhead.

The `lines.rs` version as written is **not** a drop-in
replacement — it is wrong for `BreakType::Eof` (the final
line has no terminator, so `line` must not increment and
`column` must become the char count of the content).
The Task 1 implementation must special-case `Eof` using
`pos::column_at` (which already has an ASCII fast path)
so the only O(n) work that remains is at most one line at
end-of-input.

**2. End-of-scalar span positions walk the scalar content
again.** After each scalar is scanned, the dispatcher
computes the closing `Pos` of the span with the same
loop idiom:

```rust
for ch in key_content.chars() {
    p = p.advance(ch);
}
```

11 occurrences across `src/lexer/plain.rs`,
`src/lexer/quoted.rs`, `src/lexer/comment.rs`,
`src/lexer.rs`, `src/event_iter/block/mapping.rs`,
`src/event_iter/flow.rs`, and `src/event_iter/directives.rs`.
The scanned content is guaranteed single-line at those
sites (scalar bodies, trailing comments, mapping keys,
flow-span slices), so the advance reduces to:

```
byte_offset += content.len()
line         unchanged
column      += column_at(content, content.len())
```

`src/pos.rs:50` already provides `column_at` with an ASCII
fast path. A small `pos::advance_within_line(pos: Pos,
content: &str) -> Pos` helper lets every site become one
call instead of a loop. Most real YAML content is ASCII,
so the advance becomes O(1) via the ASCII branch.

**3. `try_consume_scalar`'s 5-way speculative try-chain.**
`src/event_iter/base.rs:114` calls, in order,
`try_consume_literal_block_scalar`,
`try_consume_folded_block_scalar`,
`try_consume_single_quoted`,
`try_consume_double_quoted`,
`try_consume_plain_scalar`. Each of the first four
re-peeks `buf.peek_next()`, re-trims leading whitespace
with `trim_start_matches([' ', '\t'])`, and re-checks the
first character before returning `None`. For a plain
scalar (the common case), that is **four negative
probes** per emitted scalar, each touching the line's
leading bytes.

The dispatch decision is a single-byte check:

| First non-ws byte | Style to attempt |
|-------------------|------------------|
| `\|`              | Literal block    |
| `>`               | Folded block     |
| `'`               | Single-quoted    |
| `"`               | Double-quoted    |
| anything else     | Plain            |

Task 3 hoists the whitespace-trim and first-byte peek to
`try_consume_scalar` and dispatches to exactly one
`try_consume_*` call. No grammar change — the individual
scanners keep their existing entry checks as cheap
assertions. A pending `inline_scalar` (from a `--- text`
marker line) continues to short-circuit to the plain
branch, matching today's behaviour.

### Acceptance criteria (applies to every task)

1. **Zero behaviour change.** All existing tests pass —
   unit tests, integration tests, the YAML 1.2 conformance
   suite (`tests/conformance.rs`), round-trip tests, and
   any snapshot assertions. Emitted `Span` values and
   event contents must be byte-for-byte identical to
   before the change. The developer verifies this by
   running the full suite before and after and diffing
   nothing new.
2. **`cargo clippy --all-targets` clean**, zero warnings.
3. **`cargo fmt` clean.**
4. **Demonstrable throughput delta or no-op.** Each task
   re-runs `cargo bench -p rlsp-yaml-parser --bench
   throughput`, diffs the medians against the baseline in
   `docs/benchmarks.md`, and:
   - Updates `docs/benchmarks.md` with the new numbers.
   - Fails the task if any fixture regresses beyond
     Criterion's reported noise bands (treat a median that
     moves outside the `[low, high]` confidence interval
     in the regressing direction as a regression; re-run
     once before accepting).
5. **No new public API surface.** These are internal
   refactors; no changes to the crate's `pub` items
   except adding one small `pub(crate)` helper in
   `src/pos.rs` for Task 2.

### Specifications and reference implementations

- [YAML 1.2 specification](https://yaml.org/spec/1.2.2/) —
  authoritative grammar. The behaviour-preserving rule in
  Task 1 hinges on §5.4 (`l-line-break`) and the
  `BreakType::Eof` edge case: the final line has no
  terminator, so position tracking must stay on the same
  line.
- [libfyaml](https://github.com/pantoniou/libfyaml) — the
  performance baseline. Task 3's single-byte dispatch
  mirrors libfyaml's scanner, which does exactly one
  first-byte switch before entering a scalar rule.
- Prior plans listed above.

### Files involved

**Task 1 — duplicate pos_after_line**
- `rlsp-yaml-parser/src/lexer.rs` (delete the O(n) version
  and every `use super::pos_after_line` import)
- `rlsp-yaml-parser/src/lines.rs` (promote the helper,
  add `BreakType::Eof` branch that uses `column_at`)
- `rlsp-yaml-parser/src/lexer/block.rs`,
  `rlsp-yaml-parser/src/lexer/comment.rs`,
  `rlsp-yaml-parser/src/lexer/plain.rs`,
  `rlsp-yaml-parser/src/lexer/quoted.rs` (update imports)
- `rlsp-yaml-parser/src/pos.rs` (no change; `column_at`
  already exists)

**Task 2 — end-of-span char walks**
- `rlsp-yaml-parser/src/pos.rs` (add
  `pub(crate) fn advance_within_line(pos: Pos, content:
  &str) -> Pos`)
- `rlsp-yaml-parser/src/lexer.rs` (2 sites: marker-line
  comment, inline scalar end)
- `rlsp-yaml-parser/src/lexer/plain.rs` (2 sites: trailing
  comment span, plain scalar end)
- `rlsp-yaml-parser/src/lexer/quoted.rs` (2 sites: single
  and double closing)
- `rlsp-yaml-parser/src/lexer/comment.rs` (1 site)
- `rlsp-yaml-parser/src/event_iter/block/mapping.rs` (1
  site: implicit mapping key end)
- `rlsp-yaml-parser/src/event_iter/flow.rs` (1 site)
- `rlsp-yaml-parser/src/event_iter/directives.rs` (1 site:
  `%TAG` prefix — cold path, included for consistency)

**Task 3 — scalar try-chain dispatch**
- `rlsp-yaml-parser/src/event_iter/base.rs` (rewrite
  `try_consume_scalar` as a first-byte dispatcher)
- `rlsp-yaml-parser/src/event_iter/step.rs` (no logic
  change; verify the call site still compiles)

**Bench + docs (every task)**
- `rlsp-yaml-parser/docs/benchmarks.md` (refresh the
  affected throughput tables with the new medians)
- `rlsp-yaml-parser/benches/throughput.rs` (no change
  expected; listed so the reviewer knows it is in scope
  for verification re-runs)

## Steps

- [x] Task 1 — deduplicate `pos_after_line` with an Eof-safe
      fast path — `32a2809`
- [x] Task 2 — eliminate end-of-span char walks via
      `pos::advance_within_line` — `5966502`
- [x] Task 3 — collapse the scalar try-chain into a
      first-byte dispatcher — `8650780`

## Tasks

### Task 1: Deduplicate `pos_after_line` with an Eof-safe fast path

Delete the O(n) `pos_after_line` in `src/lexer.rs:501`
and route every call site to a single promoted helper in
`src/lines.rs` that is O(1) for all non-Eof lines and
O(one-line-len) for the final Eof line (via the existing
`column_at` ASCII fast path).

Target implementation in `src/lines.rs`:

```rust
pub fn pos_after_line(line: &Line<'_>) -> Pos {
    let byte_offset = line.offset
        + line.content.len()
        + line.break_type.byte_len();
    match line.break_type {
        BreakType::Eof => Pos {
            byte_offset,
            line: line.pos.line,
            column: line.pos.column
                + crate::pos::column_at(line.content, line.content.len()),
        },
        BreakType::Lf | BreakType::Cr | BreakType::CrLf => Pos {
            byte_offset,
            line: line.pos.line + 1,
            column: 0,
        },
    }
}
```

- [x] Promote `pos_after_line` in `src/lines.rs` from
  `const fn` private helper to `pub(crate) fn` with the
  Eof-aware implementation above.
- [x] Delete the O(n) `pos_after_line` and its `pub` export
  from `src/lexer.rs`.
- [x] Update imports in `src/lexer.rs`,
  `src/lexer/block.rs`, `src/lexer/comment.rs`,
  `src/lexer/plain.rs`, `src/lexer/quoted.rs` to use
  `crate::lines::pos_after_line`.
- [x] Verify the internal `LineBuffer::prime` and
  `peek_until_dedent` call sites in `src/lines.rs` still
  use the promoted helper (they should — one helper, one
  definition).
- [x] Add unit tests in `src/lines.rs` pinning
  `pos_after_line` output for each `BreakType` variant
  (Lf, Cr, CrLf, Eof) across ASCII-only, multi-byte, and
  empty-content lines. These are regression guards for
  the Eof branch — the only case whose behaviour
  materially differs from the previous lexer helper.
- [x] `cargo fmt`, `cargo clippy --all-targets`,
  `cargo test -p rlsp-yaml-parser` (including the
  conformance suite) all green.
- [x] `cargo bench -p rlsp-yaml-parser --bench throughput`;
  compare medians against the baseline in
  `docs/benchmarks.md`; confirm no fixture regresses.
- [x] Update `docs/benchmarks.md` with the new numbers.
- [x] Commit: `perf(parser): unify pos_after_line with
  Eof-safe O(1) fast path` — `32a2809`.

**Reference impl consultation:**
1. `src/lines.rs:376` — the pre-existing const fn that
   served LineBuffer internally.
2. libfyaml `fy_reader_advance_lb_mode()` — how column
   tracking handles line-break vs non-line-break transitions.

**Advisors:** test-engineer — the change touches a helper
called from every line-consuming path and has a single
subtle edge case (Eof). The test advisor should validate
the unit-test list *before* implementation (input gate)
and sign off on the completed test set *before* the work
goes to the reviewer (output gate). No security-engineer
consultation: this is a pure refactor with no trust
boundary involvement.

### Task 2: Eliminate end-of-span char walks via `pos::advance_within_line`

Add a small helper in `src/pos.rs`:

```rust
/// Advance `pos` past `content`, assuming `content` contains no
/// line break. Uses the ASCII fast path in [`column_at`].
pub(crate) fn advance_within_line(pos: Pos, content: &str) -> Pos {
    Pos {
        byte_offset: pos.byte_offset + content.len(),
        line: pos.line,
        column: pos.column + column_at(content, content.len()),
    }
}
```

Replace every `for ch in X.chars() { p = p.advance(ch); }`
pattern at the 11 sites below with one call to
`crate::pos::advance_within_line(start, slice)`. At each
site the caller already knows the slice contains no line
break, so the helper's assumption holds.

- [x] Add `advance_within_line` to `src/pos.rs`; include a
  unit test pinning its output for ASCII, multi-byte, and
  empty inputs. (9 unit tests added, including two
  equivalence proofs vs `chars().fold(pos, Pos::advance)`.)
- [x] `src/lexer.rs`: replace the walks at the marker-line
  comment span (~line 266) and the inline-scalar end
  (~line 329).
- [x] `src/lexer/plain.rs`: replace the trailing-comment
  span walk (~line 79) and the plain-scalar end walk
  (~line 139).
- [x] `src/lexer/quoted.rs`: replace the single-quoted
  closing walk (~line 64) and the double-quoted closing
  walk (~line 140). (All four quoted-scalar walk sites
  replaced — 2 single-quoted + 2 double-quoted inside
  `scan_double_quoted_line` — and the private
  `pos_after_str` helper was deleted as no longer needed.)
- [x] `src/lexer/comment.rs`: replace the comment-span walk
  (~line 63).
- [x] `src/event_iter/block/mapping.rs`: replace the
  implicit-key end walk (~line 170).
- [x] `src/event_iter/flow.rs`: replace the flow-span walk
  (~line 115). (Additionally replaced the flow-context
  comment walk at ~line 263, which was not originally
  enumerated but matched the same pattern.)
- [x] `src/event_iter/directives.rs`: replace the `%TAG`
  prefix walk (~line 208). (Verified non-applicable: the
  loop at `directives.rs:208` is a `prefix.chars()`
  control-character validation loop with no `p.advance(c)`
  call, not a span walk. The plan's enumeration was
  imprecise about this site; no replacement exists to
  make.)
- [x] `cargo fmt`, `cargo clippy --all-targets`,
  `cargo test -p rlsp-yaml-parser` all green.
- [x] `cargo bench -p rlsp-yaml-parser --bench throughput`;
  verify no regressions; update `docs/benchmarks.md`.
  (Same-session in-container comparison vs baseline SHA
  `47f6f7e`: `flow_heavy` -21.3% — the big win, driven by
  the `abs_pos` closure replacement; `scalar_heavy` and
  `mixed` flat within noise; `block_sequence` showed ±1–2%
  fluctuation across five re-runs with individual samples
  both above and below baseline, dominated by container
  scheduling noise per the plan's stated ±5% band.)
- [x] Commit: `perf(parser): replace end-of-span char
  walks with ASCII-fast-path helper` — `5966502`.

**Reference impl consultation:**
1. `src/pos.rs:50` `column_at` — existing ASCII fast-path
   helper that this task reuses.

**Advisors:** test-engineer — span correctness is the only
failure mode, and this change touches every scalar and
mapping-key span in the parser. The unicode position tests
from `tests/unicode_positions.rs` (from the 2026-04-10
unicode plan) are the primary safety net; the test advisor
should confirm coverage is adequate *before* implementation
(input gate) and verify the completed change against the
unicode test list *before* the work goes to the reviewer
(output gate). No security consultation needed — no new
trust boundaries.

### Task 3: Collapse the scalar try-chain into a first-byte dispatcher

Rewrite `try_consume_scalar` in
`src/event_iter/base.rs:114` so it performs exactly one
whitespace trim and first-byte peek on the next line,
then dispatches to exactly one `try_consume_*` method.
Today's chain does up to five speculative probes per
scalar; after this task, plain scalars hit the plain path
directly with no negative probes.

Dispatch table:

| First non-ws byte after trim | Call                                 |
|------------------------------|--------------------------------------|
| `\|`                         | `try_consume_literal_block_scalar`   |
| `>`                          | `try_consume_folded_block_scalar`    |
| `'`                          | `try_consume_single_quoted`          |
| `"`                          | `try_consume_double_quoted`          |
| anything else (incl. EOF)    | `try_consume_plain_scalar`           |

A pending `self.lexer.has_inline_scalar()` (from a
`--- text` marker line) must short-circuit to the plain
branch *before* the peek, because the inline scalar does
not live on the currently-primed line.

Rules the implementation must preserve:

- Every `try_consume_*` still owns its internal entry
  validation — the dispatcher's byte check is an
  optimisation, not a substitute for the scanner's own
  preconditions. If the scanner returns `None` (e.g. a
  malformed indicator), the dispatcher must fall through
  to `Ok(None)`, not retry a different style.
- Block-scalar and quoted scanners keep the exact
  `parent_indent` / `block_context_indent` arguments they
  receive today.
- The post-double-quoted trailing-tail validation block
  (currently at `src/event_iter/base.rs:174-195`) moves
  with the double-quoted branch unchanged.
- Error propagation via `?` is preserved.
- The function signature and return type stay identical.

- [x] Rewrite `try_consume_scalar` with the dispatch
  structure above.
- [x] Add a targeted unit test in `src/event_iter/base.rs`
  (or a sibling test file) asserting that each first-byte
  case dispatches to the expected scalar style on minimal
  inputs (`|\n  a`, `>\n  a`, `'a'`, `"a"`, `a`). This
  pins the dispatch decision so future refactors can't
  silently re-order it. (12 unit tests in Groups A–D
  added in `src/event_iter/base.rs`; 13 integration
  tests in Groups E–H added in `tests/smoke.rs`.)
- [x] `cargo fmt`, `cargo clippy --all-targets`,
  `cargo test -p rlsp-yaml-parser` all green, including
  the full YAML 1.2 conformance suite. Pay specific
  attention to any test that exercises a line starting
  with a block indicator followed by unusual whitespace
  patterns, and to the `---` inline-scalar paths.
- [x] `cargo bench -p rlsp-yaml-parser --bench throughput`;
  verify no regression anywhere; update `docs/benchmarks.md`.
  (Reviewer ran a four-point in-session back-to-back
  protocol — BASE×2 and HEAD×2 — on the style group.
  Container noise floor on the style fixtures is ±5–10%;
  the dispatcher shows no consistent signal above that
  floor and no regression in either direction. Net result
  on the plan's watch list [`scalar_heavy`, `mixed`,
  `block_sequence`]: no-op within noise. Acceptance
  criterion satisfied as "no-op, no regression". The
  handoff's single-shot deltas [−1.3% / −2.3% / −10.8%]
  were measured across sessions with different host load
  and did not survive the reviewer's in-session
  verification.)
- [x] Commit: `perf(parser): dispatch scalar try-chain on
  first-byte peek` — `8650780`.

**Reference impl consultation:**
1. libfyaml scanner (`fy_scan_scalar`-adjacent code in
   `fy-scan.c`) — it performs exactly one dispatch step
   before entering a style-specific scanner; use it as the
   structural model.
2. YAML 1.2 §7.3 and §8.1 — the scalar style productions,
   to confirm the first-byte dispatch is lossless.

**Advisors:**
- **test-engineer** — dispatch-order changes are the kind
  of thing unit and conformance tests catch immediately
  or not at all. Input gate: advisor provides the test
  list, paying specific attention to `---` + block-scalar
  inline content, empty-line-between-entries cases, and
  tab-prefixed indicators. Output gate: advisor signs off
  on the completed tests before the work goes to the
  reviewer.
- **security-engineer** — the parser sits at a trust
  boundary (untrusted YAML input becomes structured
  events). Changing dispatch order touches the scalar
  entry point for every user input. Input gate: advisor
  reviews the dispatch table for any style-confusion
  hazard (e.g. a byte that could be interpreted as more
  than one style). Output gate: advisor signs off on the
  implementation against the identified risks before the
  work goes to the reviewer.

## Decisions

- **Defer the `step_in_document` restructure (candidate
  4).** The user wants to discuss it after this plan
  lands. Listed for completeness only, not in scope here.
- **No numeric throughput target.** Each task is
  evaluated on "faster or no-op, no regression
  anywhere." A numeric target would require picking a
  number (a product decision) before any of these
  optimisations have measured deltas — we will know more
  after Task 1 lands.
- **One commit per task, sequential.** Preserves
  bisectability and lets us stop after any task without
  leaving a half-applied change.
- **`docs/benchmarks.md` refreshes after every task.**
  Keeps the doc honest with HEAD; each task includes the
  doc update in its commit.
- **The `pos_after_line` Eof branch uses `column_at`
  instead of character walking.** `column_at` already
  exists (`src/pos.rs:50`), already has the ASCII fast
  path, and handles the exact edge case we need. No new
  helper required for Task 1.
- **`advance_within_line` is the right abstraction for
  Task 2**, not an end-position return from the scanner.
  Returning end positions from every scanner would ripple
  through the scanner API surface; adding a small
  caller-side helper stays scoped to this plan.
- **Task 3 does not change individual scanner internals.**
  Each `try_consume_*` keeps its first-byte validation as
  a cheap assertion. This means the dispatcher can be
  reverted cleanly if a regression surfaces — the work
  cost of rollback is exactly one `git revert`.
