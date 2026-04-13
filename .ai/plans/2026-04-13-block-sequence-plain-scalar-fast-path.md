**Repository:** root
**Status:** InProgress
**Created:** 2026-04-13

## Goal

Close the remaining `block_sequence` throughput gap against
libfyaml (currently 1.42×) by short-circuiting the
"prepend synthetic line → re-enter `step_in_document` →
scalar dispatch" round-trip for the overwhelmingly common
case: `- plain_scalar` on a single line with no anchor, no
tag, and no mapping key indicator.

**Target:** ≥10% throughput improvement on the
`block_sequence` benchmark fixture, with no regression on
other fixtures and zero behaviour change (full conformance
suite + existing tests green).

## Context

- **Why `block_sequence` is the worst fixture.** Each
  `- item` line currently takes two full trips through
  the dispatch loop:
  1. `step_in_document` → `handle_sequence_entry` →
     `consume_sequence_dash` → prepends inline content as
     a synthetic `Line` → returns `StepResult::Continue`
  2. `Iterator::next` loops → `step_in_document` again →
     comment/blank skip → tab check → peek/trim → all
     probes (sequence, explicit key, mapping, alias, tag,
     anchor, flow) → `try_consume_scalar` → plain scalar
     path → `scan_plain_line_block` → emit `Scalar`

  The second trip repeats peek, trim, and 10+ probes that
  are unnecessary when the inline content is a bare plain
  scalar. Libfyaml handles `- value` as one token-level
  operation.

- **Prior optimisation work** (memory file
  `potential-performance-optimizations.md`):
  - `32a2809` O(1) `pos_after_line` — −15 to −22%
  - `5966502` `advance_within_line` — −19% `flow_heavy`
  - `ba11228` cached trim + marker indent guard — −6-8%
  - `4728ea3` probe reorder by frequency — −5%
  - After these, `block_sequence` at 1.42× is the only
    fixture significantly behind libfyaml.

- **Key functions and files:**

  | Function | File | Lines |
  |----------|------|-------|
  | `handle_sequence_entry` | `event_iter/block/sequence.rs` | 102–261 |
  | `consume_sequence_dash` | `event_iter/block/sequence.rs` | 54–95 |
  | `try_consume_scalar` | `event_iter/base.rs` | 118–261 |
  | `try_consume_plain_scalar` | `lexer/plain.rs` | 31–154 |
  | `scan_plain_line_block` | `lexer/plain.rs` | 354–430 |
  | `peek_plain_scalar_first_line` | `lexer/plain.rs` | 256–287 |
  | `find_value_indicator_offset` | `event_iter/line_mapping.rs` | 68–172 |
  | `tick_mapping_phase_after_scalar` | `event_iter/block/mapping.rs` | 739–770 |
  | `drain_trailing_comment` | `event_iter/base.rs` | 263–278 |

- **Borrow contract:** `try_consume_plain_scalar` returns
  `Cow::Borrowed` for single-line plain scalars (zero
  allocation). The fast path must preserve this.

- **References:**
  - [YAML 1.2 §7.3.3 — Plain Scalars](https://yaml.org/spec/1.2.2/#733-plain-style)
  - [YAML 1.2 §8.2.1 — Block Sequences](https://yaml.org/spec/1.2.2/#821-block-sequences)
  - Memory file: `.ai/memory/potential-performance-optimizations.md`
  - Prior plans: `2026-04-11-parser-throughput-follow-up.md`,
    `2026-04-12-step-dispatcher-micro-optimizations.md`

## Steps

- [x] Clarify requirements with user
- [x] Analyse code paths and assess scope
- [x] Implement fast path in `handle_sequence_entry`
- [x] Add targeted unit tests for the fast path
- [x] Run benchmarks and verify ≥10% improvement on
      `block_sequence`
- [x] Verify full conformance suite + existing tests pass

## Tasks

### Task 1: Implement block-sequence plain scalar fast path

Add an early-exit optimisation in `handle_sequence_entry`
that detects "simple plain scalar after `-`" and emits the
`Scalar` event directly — bypassing the synthetic line
prepend and full `step_in_document` re-entry.

**Insertion point:** `sequence.rs:239`, immediately after
`consume_sequence_dash` returns `had_inline = true`.

**Fast-path guard conditions** (all must hold):

1. `self.pending_anchor.is_none()` — no anchor to attach
2. `self.pending_tag.is_none()` — no tag to attach
3. The synthetic line's first byte is not a special char
   (`|`, `>`, `'`, `"`, `[`, `{`, `&`, `*`, `!`, `?`, `-`,
   `#`, `%`, `@`, `` ` ``)
4. `find_value_indicator_offset(content).is_none()` — not
   a mapping key line (no `: `)
5. Content passes `ns_plain_first_block` — valid plain
   scalar start

**Fast-path body** (when all guards pass):

1. Peek the synthetic line from the lexer
2. Call `scan_plain_line_block(trimmed_content)` to get the
   scalar value slice
3. Check for trailing comment in the suffix (reuse the
   trailing comment extraction pattern from
   `try_consume_plain_scalar`)
4. Check for suffix errors (NUL/BOM — same as
   `try_consume_plain_scalar` lines 101–121)
5. Consume the synthetic line from the lexer
6. Compute `Span` for the scalar (start from content
   position, end via `advance_within_line`)
7. Push `Event::Scalar { value: Cow::Borrowed(value),
   style: Plain, anchor: None, tag: None }` + span to
   `self.queue`
8. Call `self.tick_mapping_phase_after_scalar()`
9. Call `self.drain_trailing_comment()`
10. If `self.lexer.plain_scalar_suffix_error` is set,
    return it as a `StepResult::Yield(Err(...))`

**Fallthrough:** If any guard fails, proceed with the
existing `StepResult::Continue` which re-enters
`step_in_document` via the synthetic line — zero behaviour
change for non-fast-path cases.

**What this does NOT do:**

- Does not handle multi-line plain scalars — those require
  `collect_plain_continuations` which reads subsequent
  lines. The fast path only handles single-line. This is
  fine because `block_sequence` entries are overwhelmingly
  single-line.
- Does not handle anchors/tags — those need the full
  dispatch path. Falls through cleanly.
- Does not restructure `step_in_document` (Option D) —
  this is a targeted optimisation in one function.

**Visibility:** `scan_plain_line_block` and
`ns_plain_first_block` are currently `pub(super)` in
`lexer/plain.rs`. They need to be made `pub(crate)` for
access from `event_iter/block/sequence.rs`. Similarly,
`extract_trailing_comment` (used for trailing comment
detection) is `fn` in `lexer/plain.rs` — needs
`pub(crate)`. No public API change.

**Acceptance criteria:**

- [x] Fast path emits identical events and spans as the
      existing two-trip path for `- plain_scalar` lines
- [x] Trailing comments handled (`- value # comment`)
- [x] Suffix errors detected (`- value\0more`)
- [x] Falls through cleanly for: anchors, tags, block
      scalars, quoted scalars, mapping keys, flow
      collections, multi-byte first chars that are
      indicators
- [x] `cargo test` passes (all existing tests)
- [x] `cargo clippy --all-targets` zero warnings
- [x] Benchmark: ≥10% improvement on `block_sequence`
- [x] Benchmark: no regression (>2%) on other fixtures

**Commit:** `05d21fa`

## Decisions

- **Single task, not multiple.** The change is ~30-50
  lines in one function with a clear fallthrough. Splitting
  it would create artificial boundaries.
- **Guard-heavy, body-light.** The fast path has many
  guards (5 conditions) but a simple body. This is correct
  — the guards ensure we only take the fast path when
  behaviour is guaranteed identical; any doubt → fallthrough.
- **Skip multi-line scalars.** Adding continuation-line
  handling to the fast path would require duplicating
  `collect_plain_continuations` logic for marginal gain.
  Multi-line values in block sequences (e.g. long
  descriptions) are uncommon enough that the two-trip
  overhead is negligible.
- **10% target rationale.** The two-trip overhead includes:
  peek + trim (~2ns), probe cascade (~8-15ns per
  step_in_document entry), comment/blank skip, tab check.
  For `block_sequence` where every line is `- value`, this
  overhead applies to ~50% of all events (every scalar).
  Eliminating it should yield 10-20% improvement on that
  fixture.
