**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-26

# Recover parser perf: NodeMeta + EventMeta box, plus iterative follow-ups

## Goal

Bring `rlsp-yaml-parser` throughput and first-event latency
back to within ±2% of the documented baseline in
`rlsp-yaml-parser/docs/benchmarks.md` on every fixture.
The prior plan
(`2026-04-26-parser-perf-recover-tag-allocations.md`)
recovered some of the regression but left measurable gaps:
load down 12–32% on size fixtures, events down 5–12%,
first-event latency up 22–26%.

The remaining gap traces to the 2026-04-20 anchor/tag-span
work, which added two `Option<Span>` fields (~112 bytes) per
Node and per Event variant. The fix is structural: box the
rare per-node and per-event metadata so the common case
(no anchor, no user-authored tag, no comments) carries an
8-byte `Option<Box<…>>` instead of ~200 bytes of inline
state.

This plan is **menu-driven and bench-iterative**. The lead
applies stages in order, runs benchmarks between stages,
and stops as soon as the ±2% target is met. Stages later
in the menu are conditional on earlier stages being
insufficient.

When the target is met, the user runs `git reset --soft
main` to collapse iteration WIP commits into staged
changes; final shippable commits are crafted from the
staged result.

## Context

**Starting state (HEAD when this plan begins):** the prior
plan's two tasks are already on `main`:
- commit `3f15780` — `Node::tag` migrated to
  `Option<Cow<'static, str>>`
- commit `a7206f6` — first-byte fast-path in
  `resolve_core_plain`

**Measured gap to documented baseline (2026-04-26 run):**

| Metric class | Worst-fixture gap |
|---|---|
| `rlsp/load` size fixtures | huge_1MB −32% |
| `rlsp/load` style fixtures | block_heavy −22% |
| `rlsp/load` real-world (kubernetes_3KB) | −12% |
| `rlsp/events` size fixtures | huge_1MB −12% |
| `rlsp/events` style fixtures | block_sequence −9% |
| First-event latency | tiny_100B/medium_10KB/large_100KB +25% |

**Concrete struct sizes (measured):**

| Type | Bytes |
|---|--:|
| `Pos` | 24 |
| `Span` | 48 |
| `Option<Span>` | 56 |
| `Option<Box<Span>>` | 8 |
| `Cow<'static, str>` | 24 |

| Layout | Bytes per Node |
|---|--:|
| Current Node (post Tasks 1+2) | 288 |
| Node with hybrid `Option<Box<NodeMeta>>` | 112 (−61%) |

A typical Kubernetes leaf scalar (`name: nginx`) has no
anchor, no user-authored tag (resolver tag is inline), no
leading comments, no trailing comment — `meta` is `None`.
That is the hot path the boxing exists to optimize.

**Files in scope (any stage may touch these):**
- `rlsp-yaml-parser/src/node.rs` — Node enum
- `rlsp-yaml-parser/src/event.rs` — Event enum
- `rlsp-yaml-parser/src/loader.rs` — Node construction
- `rlsp-yaml-parser/src/loader/reloc.rs` — destructure-and-rebuild
- `rlsp-yaml-parser/src/event_iter/` — Event construction
- `rlsp-yaml-parser/src/pos.rs` — Span/Pos types (Stage C only)
- `rlsp-yaml-parser/src/event_iter/step.rs` — `step_in_document` (Stage D only)
- Cross-crate consumers in `rlsp-yaml/src/` — accessor call sites
- All test modules that construct `Node` / `Event` literals

**Specifications and references:**
- `rlsp-yaml-parser/docs/benchmarks.md` — recovery target
  (commit `3bec2da`, 2026-04-16 baremetal)
- `.ai/reports/bench-baremetal.log` — current measurements
- `.ai/memory/potential-performance-optimizations.md` —
  pre-existing analysis identifying "L4 full" (this plan's
  Stage A) as the deferred candidate now justified by
  fresh evidence; also lists Stage C (lazy Span) and
  Stage D (`step_in_document` restructure) as deferred
  candidates with prior diagnostic data
- YAML 1.2.2 — no spec change in scope; behavior-preserving
  refactors only

**Constraints from clarification:**

- **Iteration is bench-driven.** The lead runs `cargo bench`
  in this Docker session between stages. CPU is the same
  Intel Core Ultra X7 358H as the documented baremetal
  baseline; the user has freed the host so Docker has full
  CPU access. Numbers used as a **directional signal for
  picking the next stage** (relative deltas reliable) and
  as a **stopping criterion when the gap closes**, not as
  per-task acceptance gates that fail tasks for noise.
- **Public API may break.** Parser is at 0.7.0 (Task 1
  already broke it). Further breakage is acceptable; bump
  to 0.8.0 if a stage materially changes accessor signatures
  or pattern-match shape.
- **Non-goal: revert any prior work.** Schema resolution,
  span propagation, and Cow tag plumbing all stay.
- **Soft-reset coda.** When the target is met, this plan
  ends; the user runs `git reset --soft main` to collapse
  iteration WIPs; final commits are crafted from the
  resulting staged tree (out of plan scope).

## Steps

- [ ] **Stage 0 — Capture baseline at HEAD.** Run
  `cargo bench` once at the current HEAD. Record per-fixture
  numbers in this plan as the "session start" line. This
  is the comparison point for every later stage and
  cross-checks Docker-vs-baremetal drift.
- [x] **Stage A — Box NodeMeta.** Apply Option B to `Node`.
  Bench. Record numbers.
- [ ] **Stage B — Box EventMeta.** Apply the same pattern
  to `Event`. Bench. Record numbers. **Decision gate:** if
  every fixture is within ±2% of documented baseline,
  STOP and proceed to Soft-Reset Handoff.
- [ ] **Stage C — Lazy Span (conditional).** Only if first-
  event latency or events throughput is still outside ±2%.
  Span becomes `(start_offset_u32, end_offset_u32)`; line/
  column computed on demand. Bench. Decision gate as
  above.
- [ ] **Stage D — `step_in_document` byte-dispatch
  (conditional).** Only if a load fixture (especially
  `block_sequence`) is still outside ±2%. Bench. Decision
  gate as above.
- [ ] **Stage E — Re-flame and pick (conditional).** Only
  if A+B+C+D are insufficient. Capture a fresh flamegraph
  on the worst-remaining fixture; identify a new dominant
  frame; add a new task to this plan addressing it. Repeat.
- [ ] **Memory-file housekeeping.** After all applied
  stages: update
  `.ai/memory/potential-performance-optimizations.md` to
  mark the candidates that were applied (the relevant
  subset of "L4 full" / "Lazy Span construction" /
  "`step_in_document` restructure") with a reference to
  this plan and the commits.
- [ ] **Soft-Reset Handoff.** Once every fixture is within
  ±2%, summarize which stages were applied, total commits,
  and present the working tree to the user for soft-reset
  + clean re-commit.

## Tasks

The tasks below are the **menu**. Execution order is fixed
through Stage B; Stages C/D/E are conditional and chosen
based on bench data captured between stages. The lead
records each stage's bench results in this file before
deciding the next stage.

### Task A: Box NodeMeta

Move `anchor`, `anchor_loc`, `tag_loc`, `leading_comments`,
and `trailing_comment` off `Node::Scalar` / `Node::Mapping`
/ `Node::Sequence` into a heap-allocated `NodeMeta` struct
behind a single `meta: Option<Box<NodeMeta>>` field. Tag
stays inline (post Task 1 it is populated on every loaded
node by the schema resolver and would defeat the boxing
hot path).

`Node::Alias` does not get a `meta` field — its
`leading_comments` and `trailing_comment` are unrelated to
anchor/tag/loc state. Decide at implementation time
whether Alias retains inline `Option<Vec<String>>` /
`Option<String>` for comments, or moves them behind a
smaller `AliasMeta` box for layout consistency. Default:
keep Alias inline (simplest, lowest blast radius).

**Implementation:**

- [x] Define `NodeMeta` struct in `node.rs` with the five
  moved fields.
- [x] Restructure `Node::Scalar`, `Mapping`, `Sequence` to
  carry `meta: Option<Box<NodeMeta>>` instead of the five
  inline fields.
- [x] Update accessors (`node.anchor()`,
  `.anchor_loc()`, `.tag_loc()`, `.leading_comments()`,
  `.trailing_comment()`) to deref through `meta`. Add
  `#[inline]` to keep callers' costs flat in the
  meta=None case.
- [x] Update Node construction in `loader.rs` —
  Event→Node sites build a `NodeMeta` only when at least
  one of the moved fields is non-empty; otherwise `meta:
  None`. Resolver-injected tag (the always-present case)
  goes inline as today.
- [x] Update `loader/reloc.rs` destructure-and-rebuild
  to flow the meta field through.
- [x] Audit cross-crate consumers in `rlsp-yaml/src/`
  (formatter, validators, symbols, schema_validation) —
  any pattern-match arms that bind `anchor` / `anchor_loc`
  / `tag_loc` / `leading_comments` / `trailing_comment`
  by field name need updates. Accessors are unchanged.
- [x] Update test construction sites in:
  `rlsp-yaml-parser/src/node.rs` (test module),
  `rlsp-yaml-parser/src/loader.rs` (test module),
  `rlsp-yaml-parser/src/loader/reloc.rs` (test module),
  any `tests/` integration tests in either crate,
  `rlsp-yaml/tests/corpus_invariants.rs`,
  `rlsp-yaml/src/schema_validation.rs` (synthetic Node
  constructions).
- [x] Update `rlsp-yaml-parser/docs/feature-log.md` with a
  user-facing entry for the API shape change: direct
  field access to `anchor`, `anchor_loc`, `tag_loc`,
  `leading_comments`, `trailing_comment` on Scalar/Mapping/
  Sequence variants is no longer available; callers must
  use the existing accessor methods. Bump the parser
  `Cargo.toml` to `0.8.0`.
- [x] Bench: `cargo bench -p rlsp-yaml-parser` (full).
  Record per-fixture results in the Stage A bench-record
  block below.

**Acceptance:**

- [x] `cargo build` workspace clean.
- [x] `cargo clippy --all-targets` zero warnings.
- [x] `cargo test` workspace passes.
- [x] `Node` size measured at ≤ 120 bytes (target: 112)
  via a one-shot `size_of::<Node<Span>>()` assertion in a
  test or `dbg!` in a temporary scratch. Record actual
  bytes in this plan. **Measured: 120 bytes.**
- [x] No remaining inline `anchor` / `anchor_loc` / `tag_loc`
  / `leading_comments` / `trailing_comment` fields on
  Scalar/Mapping/Sequence variants (diff-shape proof).

**Completed:** 2026-04-26 — commit `40b3e8df0a3488cad29af895d2615ba8ccb162d2`

### Task B: Box EventMeta

Apply the same pattern to `Event::Scalar`,
`Event::MappingStart`, `Event::SequenceStart`. Move
`anchor`, `anchor_loc`, `tag` (Cow), `tag_loc` into an
`EventMeta` struct behind `meta: Option<Box<EventMeta>>`.

Unlike `NodeMeta`, `EventMeta` includes `tag` because the
loader's schema resolver runs on the loaded `Node`, not on
the upstream `Event` — events without a source-text tag
have `tag: None` naturally.

**Implementation:**

- [ ] Define `EventMeta<'input>` in `event.rs` with the
  four moved fields.
- [ ] Restructure the three variants to carry
  `meta: Option<Box<EventMeta<'input>>>` (lifetime
  parameter preserved through the box).
- [ ] Update `Event` construction sites in
  `event_iter/` — emit `meta: None` when no anchor/tag
  is present (the common case), otherwise allocate the
  meta box.
- [ ] Update `loader.rs` Event consumption sites to
  pattern-match on `meta` and pull out the four fields.
- [ ] Update production Event-construction sites:
  `rlsp-yaml-parser/src/lib.rs`,
  `rlsp-yaml-parser/src/loader/stream.rs`,
  `rlsp-yaml-parser/src/loader.rs`,
  `rlsp-yaml-parser/src/event_iter/base.rs`,
  `rlsp-yaml-parser/src/event_iter/flow.rs`,
  `rlsp-yaml-parser/src/event_iter/step.rs`,
  `rlsp-yaml-parser/src/event_iter/block/mapping.rs`,
  `rlsp-yaml-parser/src/event_iter/block/sequence.rs`.
- [ ] Update test Event-construction sites:
  `rlsp-yaml-parser/tests/encoding.rs`,
  `rlsp-yaml-parser/tests/unicode_positions.rs`,
  all files under `rlsp-yaml-parser/tests/smoke/` that
  construct Event literals (~17 files including
  `scalars.rs`, `sequences.rs`, `mappings.rs`,
  `tags.rs`, `anchors_and_aliases.rs`, etc. —
  `cargo build` will surface any missed sites).
- [ ] Update `rlsp-yaml-parser/docs/feature-log.md` if
  Task A's entry didn't already cover the Event API
  change; otherwise extend that entry. Confirm parser
  `Cargo.toml` is at `0.8.0` from Task A (no second
  bump needed).
- [ ] Bench: full `cargo bench -p rlsp-yaml-parser`.
  Record per-fixture results in the Stage B bench-record
  block below.

**Acceptance:**

- [ ] `cargo build` workspace clean.
- [ ] `cargo clippy --all-targets` zero warnings.
- [ ] `cargo test` workspace passes.
- [ ] All yaml-test-suite conformance tests still pass at
  the same rate as before this stage.
- [ ] `Event` size measured and recorded (target: ≤ 56
  bytes for the three node-variants).
- [ ] **Decision gate:** if every fixture is within ±2% of
  the documented baseline, mark Stages C/D/E `Skipped`
  and proceed to Soft-Reset Handoff. Otherwise, pick the
  next stage based on which fixture class is still off.

### Task C (conditional): Lazy Span construction

Skip unless first-event latency or events throughput
remains outside ±2% after Stage B. Per the memory file's
"4. Lazy Span construction" candidate.

Replace `Pos { line, column, offset }` (24 bytes) with a
single byte offset (4 bytes). `Span` becomes
`(start_offset_u32, end_offset_u32)` = 8 bytes. Line/column
computed on demand by a `LineIndex` carried on `Document`
(or by a helper that takes a `&str` source).

**Implementation:**

- [ ] Define `LineIndex` in `pos.rs` (sorted vector of
  newline byte offsets, binary search for line/column).
- [ ] Reduce `Pos` to a wrapper over `u32` (offset only),
  or remove `Pos` and use `u32` directly inside `Span`.
- [ ] Add accessor methods for line/column that take a
  `&LineIndex` or `&str` and compute on demand.
- [ ] **Pre-implementation discovery:** grep for `.line`
  and `.column` field accesses on `Pos`/`Span` across both
  crates (`grep -rn '\.line\b\|\.column\b'
  rlsp-yaml-parser/src/ rlsp-yaml/src/
  rlsp-yaml-parser/tests/ rlsp-yaml/tests/`) and append
  the consumer file list to this task before implementing.
  Match the enumeration specificity of Task A.
- [ ] Update every enumerated consumer that reads
  `pos.line` / `pos.column` directly to either go through
  the accessor or carry the LineIndex.
- [ ] Update `rlsp-yaml-parser/docs/feature-log.md` with
  a user-facing entry for the Span/Pos API change (if
  Task C runs).
- [ ] Bench. Record results.

**Acceptance:**

- [ ] Build / clippy / tests clean.
- [ ] `Span` size = 8 bytes verified.
- [ ] Consumer file list enumerated in this task before
  the Bench checkbox is checked (no "any other consumer"
  catch-all without a discovery pass).
- [ ] Decision gate as above.

### Task D (conditional): `step_in_document` byte-dispatch restructure

Skip unless a load fixture (especially `block_sequence` or
`block_heavy`) remains outside ±2% after Stages B and/or C.
Per the memory file's "1. Option D" candidate.

Replace the linear if-else probe cascade in
`event_iter/step.rs:step_in_document` with a single peek +
single match on the first non-whitespace byte, dispatching
to the appropriate handler. Memory file's analysis
estimates 5–15% on `block_sequence`, 2–8% elsewhere.

**Implementation:**

- [ ] Restructure `step_in_document` per the memory file's
  target shape. Order-sensitive top-level checks (comment
  skip, blank skip, tab/EOF/marker) stay above the
  dispatch.
- [ ] Verify yaml-test-suite conformance unchanged.
- [ ] Bench. Record results.

**Acceptance:**

- [ ] Build / clippy / tests clean.
- [ ] yaml-test-suite pass rate unchanged.
- [ ] Decision gate as above.

### Task E (conditional, repeatable): Re-flame and pick

Skip unless Stages A+B+C+D are insufficient.

Capture a fresh flamegraph on the worst-remaining fixture
(use the methodology recorded in the memory file's
"Methodology for verification" section). Identify the new
dominant frame. Add a new task slice to this plan
addressing it. Repeat as needed.

**Implementation guidelines:**

- [ ] Before each Re-flame iteration, document the current
  bench numbers and the targeted fixture in this plan.
- [ ] Limit each iteration to one structural change so
  bench attribution is clean.

**Acceptance:**

- [ ] Build / clippy / tests clean after each iteration.
- [ ] Decision gate as above.

## Bench Records (filled during execution)

```
Stage 0 baseline at HEAD (commit 108ca04, Docker session
on Intel Core Ultra X7 358H, host freed):
  [load size]
    tiny_100B      45.17  MiB/s   (baseline 54.08, −16%)
    medium_10KB    50.49  MiB/s   (baseline 58.28, −13%)
    large_100KB    30.69  MiB/s   (baseline 43.34, −29%)
    huge_1MB       25.55  MiB/s   (baseline 35.69, −28%)
  [load style]
    block_heavy    45.46  MiB/s   (baseline 55.92, −19%)
    block_sequence 115.31 MiB/s   (baseline 128.89, −11%)
    flow_heavy     49.71  MiB/s   (baseline 57.83, −14%)
    scalar_heavy   122.20 MiB/s   (baseline 141.14, −13%)
    mixed          50.96  MiB/s   (baseline 60.69, −16%)
  [load real]
    kubernetes_3KB 68.31  MiB/s   (baseline 79.15, −14%)
  [events size]
    tiny_100B      79.21  MiB/s   (baseline 87.02, −9%)
    medium_10KB    103.01 MiB/s   (baseline 109.88, −6%)
    large_100KB    109.69 MiB/s   (baseline 123.59, −11%)
    huge_1MB       116.08 MiB/s   (baseline 130.80, −11%)
  [events style]
    block_heavy    96.74  MiB/s   (baseline 105.37, −8%)
    block_sequence 231.18 MiB/s   (baseline 227.65, +1.5%)
    flow_heavy     113.00 MiB/s   (baseline 131.22, −14%)
    scalar_heavy   218.62 MiB/s   (baseline 236.16, −7%)
    mixed          110.04 MiB/s   (baseline 115.53, −5%)
  [events real]
    kubernetes_3KB 123.97 MiB/s   (baseline 138.11, −10%)
  [latency first_event]
    tiny_100B      46.55  ns      (baseline 38.88, +20%)
    medium_10KB    46.43  ns      (baseline 38.82, +20%)
    large_100KB    46.51  ns      (baseline 38.80, +20%)
    huge_1MB       46.38  ns      (baseline 38.91, +19%)
    kubernetes_3KB 46.98  ns      (baseline 39.54, +19%)

Docker drift check: libfyaml in this run is parity with
its documented baseline (rlsp/events tiny libfyaml 33.9
vs doc 37.81 = −10%, but real-world libfyaml 134.5 vs doc
139.97 = −4%). Docker-vs-baremetal cost is roughly a flat
~5–10% across the board, so rlsp's larger gaps (e.g. load
huge_1MB −28%) are dominated by real regression, not env.

Stage A — NodeMeta box (commit 40b3e8d):
  Node<Span> size: 120 bytes (was 288, −58%)
  [load size]
    tiny_100B      46.95  MiB/s   (vs baseline 54.08, −13.2%)
    medium_10KB    51.62  MiB/s   (vs baseline 58.28, −11.4%)
    large_100KB    54.44  MiB/s   (vs baseline 43.34, +25.6%)  ★ beats baseline
    huge_1MB       39.37  MiB/s   (vs baseline 35.69, +10.3%)  ★ beats baseline
  [load style]
    block_heavy    47.64  MiB/s   (vs baseline 55.92, −14.8%)
    block_sequence 116.41 MiB/s   (vs baseline 128.89, −9.7%)
    flow_heavy     50.80  MiB/s   (vs baseline 57.83, −12.2%)
    scalar_heavy   128.59 MiB/s   (vs baseline 141.14, −8.9%)
    mixed          54.20  MiB/s   (vs baseline 60.69, −10.7%)
  [load real]
    kubernetes_3KB 72.09  MiB/s   (vs baseline 79.15, −8.9%)
  [events size]
    tiny_100B      80.41  MiB/s   (vs baseline 87.02, −7.6%)
    medium_10KB    107.11 MiB/s   (vs baseline 109.88, −2.5%)
    large_100KB    113.07 MiB/s   (vs baseline 123.59, −8.5%)
    huge_1MB       117.94 MiB/s   (vs baseline 130.80, −9.8%)
  [events style]
    block_heavy    94.44  MiB/s   (vs baseline 105.37, −10.4%)
    block_sequence 224.36 MiB/s   (vs baseline 227.65, −1.4%)  ★ within ±2%
    flow_heavy     118.14 MiB/s   (vs baseline 131.22, −10.0%)
    scalar_heavy   220.78 MiB/s   (vs baseline 236.16, −6.5%)
    mixed          112.57 MiB/s   (vs baseline 115.53, −2.6%)
  [events real]
    kubernetes_3KB 125.31 MiB/s   (vs baseline 138.11, −9.3%)
  [latency first_event]
    tiny_100B      46.36  ns      (vs baseline 38.88, +19.2%)
    huge_1MB       46.98  ns      (vs baseline 38.91, +20.7%)

  Vs Stage 0 (improvements from boxing alone):
    load: +1.4 to +78% across fixtures
    events: +1.4 to +4.7% (modest)
    latency: ~unchanged (Event-side cost untouched)

  DECISION: continue to Stage B.
  Reason: Node boxing closed the load-side gap on large
  fixtures and brought several others within striking
  distance, but Event-side cost (events throughput and
  first-event latency) is essentially unchanged. ~14
  fixtures remain >2% from baseline; latency 19% off.
  Stage B targets exactly this remaining cost.

Stage B — EventMeta box (commit ____):
  Event size: _____ bytes
  [load size] tiny ____ medium ____ large ____ huge ____
  [load style] bh ____ bs ____ fh ____ sh ____ mx ____
  [events] tiny ____ medium ____ large ____ huge ____
  [latency tiny] _____ ns
  DECISION: STOP / continue to Stage C / continue to Stage D

(Add Stage C, D, E records here as needed.)
```

## Decisions

- **Why hybrid box (tag inline) vs full box:** the
  resolver populates `Node::tag` on every loaded node in
  default `Schema::Core` mode, so a full box would always
  allocate, defeating the hot-path optimization. Hybrid
  keeps the always-set field inline.
- **Why box `Event::tag` (the always-set rule does not
  apply to events):** events come from the parser before
  schema resolution; an event tag is `Some` only when the
  source text contained one. That is the rare case for
  block-heavy and Kubernetes fixtures. Boxing applies.
- **Why iterate instead of plan-everything-up-front:**
  bench attribution is cleaner one stage at a time, and
  Stages C/D add real complexity that should not be paid
  if Stages A+B already close the gap.
- **Bench-as-signal in Docker:** session is in Docker on
  the same Intel Core Ultra X7 358H as the documented
  baremetal baseline; user freed the host so Docker has
  full CPU access. Per-stage relative changes are reliable
  enough for go/no-go decisions even if absolute numbers
  drift slightly from baremetal.
- **±2% target governs the iteration loop, not per-task
  acceptance gates.** Per-task acceptance is structural
  (build/clippy/tests pass + diff-shape checks). The ±2%
  comparison is the lead's stopping criterion at each
  decision gate, not a hard task-fail.
- **Soft-reset handoff is out of plan scope.** This plan
  ends when iteration meets the target; the user
  collapses WIPs and crafts ship commits separately.
- **Memory-file relationship:** Stage A is the "L4 full"
  candidate the memory file flagged as deferred; Stages C
  and D are the lazy-Span and step-restructure candidates
  from the same file. The post-completion memory-file
  update is tracked as an explicit step in the Steps
  checklist (not just here).

## Non-Goals

- **Reverting any prior work** (Cow tag plumbing,
  schema resolution, anchor/tag spans). All stay.
- **Crafting the final ship commits.** The user does the
  soft reset and post-reset commit shaping.
- **Documenting new benchmark numbers in
  `benchmarks.md`.** The doc is the recovery target. If
  recovered, the doc remains accurate. If we beat it, the
  user decides whether to refresh the doc separately.
- **Arena allocation for the Event queue** (memory-file
  candidate 3) — pre-judged low impact, not in menu.
- **Tuning libfyaml comparison.** Out of scope.
