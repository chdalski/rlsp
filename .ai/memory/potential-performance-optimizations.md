---
name: potential-performance-optimizations
description: Deferred parser and loader throughput optimizations — Option D (step_in_document restructure), loader regression candidates (2026-04-16), and other ideas catalogued during the 2026-04-11/12 perf work
type: project
---

## Current state (2026-04-12)

Two perf plans completed across 5 code commits:

| Commit | Change | Bare-metal impact |
|--------|--------|-------------------|
| `32a2809` | O(1) `pos_after_line` (Eof-safe) | −15 to −22% across all fixtures |
| `5966502` | `advance_within_line` helper (12 sites) | −19% `flow_heavy` |
| `8650780` | Scalar try-chain first-byte dispatch | No-op (compiler optimized it) |
| `ba11228` | Cached trim + marker indent guard | −6-8% `block_seq`/`scalar_heavy` |
| `4728ea3` | Probe reorder by frequency | −5% `block_seq` |

**Why:** Prior to these changes, rlsp was 1.15–1.76× slower
than libfyaml on event-drain throughput. After: rlsp is
faster on 4/10 fixtures, at parity on 3, and only
meaningfully behind on `block_sequence` (1.42×).

**How to apply:** Bare-metal ratios (rlsp/events ÷ libfyaml)
as of the final verification run:

| Fixture | ratio | notes |
|---------|------:|-------|
| tiny_100B | 0.45× | rlsp 2.2× faster (FFI overhead) |
| medium_10KB | 1.03× | parity |
| large_100KB | 0.97× | rlsp faster |
| huge_1MB | 1.05× | near parity |
| block_heavy | 1.04× | near parity |
| block_seq | **1.42×** | **only remaining meaningful gap** |
| flow_heavy | 0.68× | rlsp 1.5× faster |
| scalar_heavy | 0.96× | rlsp faster |
| mixed | 1.10× | close |
| k8s_3KB | 1.10× | close |

## Deferred: Option D — full `step_in_document` restructure

### What it is

Replace the linear if-else probe cascade in
`src/event_iter/step.rs:step_in_document` (~760 lines)
with a single-peek, single-trim, byte-dispatch structure.
Today the function runs 10–15 sequential probes per step;
a restructure would do one peek + one `match` on the
first non-whitespace byte + one handler call.

### Why it was deferred

The two diagnostic tasks (cached trim + probe reorder)
confirmed the probe cascade IS a measurable overhead —
neither was a no-op. But the remaining gap after those
changes (1.42× on `block_sequence`, ~1.05–1.10× elsewhere)
is likely dominated by fundamentals rather than dispatch
costs:

- Rust bounds-checking vs C unchecked access
- `VecDeque` queue overhead for multi-event steps
- `Span` struct construction (24 bytes per event)
- Comment event emission (libfyaml skips comments entirely)

The user assessed that the risk/reward of a 760-line
function rewrite was not justified at this time.

### What the restructure would entail

**Target shape** — after the comment/blank skip and
tab/EOF/marker checks (which are order-sensitive and stay
at the top), dispatch on `first_byte`:

```
match first_byte {
    Some(b'-') if next is space/tab/EOL → sequence entry
    Some(b'?') if next is space/tab/EOL → explicit key
    Some(b'*') → alias
    Some(b'!') → tag
    Some(b'&') → anchor
    Some(b'[' | b'{') → flow collection start
    Some(b']' | b'}') → stray flow closer error
    Some(b'|' | b'>' | b'\'' | b'"') → scalar (already dispatched by Task 3 of prior plan)
    _ → mapping-or-plain detection
}
```

**Hard parts:**
1. **Mapping entries can start with ANY character.** A line
   like `key: value` starts with `k`. You still need
   `find_value_indicator_offset()` to detect them — that
   scans the whole line for `: `. This scan can't be
   eliminated by first-byte dispatch; it must remain in
   the fallthrough path.
2. **`-` is ambiguous.** Could be a sequence entry
   (`- item`), a document marker (`---`), or a plain
   scalar (`-value`). Needs second-byte context. The
   marker check is already handled before the dispatch
   (indent guard from `ba11228`); the seq-vs-plain
   distinction needs `after_dash` inspection.
3. **Dedent detection** depends on indent vs collection
   stack, not on first byte. Must remain as a post-dispatch
   fallthrough.
4. **The function is 760 lines.** Restructuring it is a
   high-risk diff that touches the entry point for every
   parse event. The zero-behaviour-change constraint
   requires the full conformance suite + `unicode_positions`
   + `smoke` tests to stay green.

**Estimated impact:** 5–15% on `block_sequence` (where the
dispatch overhead is largest relative to per-event work),
2–8% on other fixtures. Based on Task 1 of the dispatcher
plan producing ~6-8% from just caching + guards, and
Task 2 producing ~5% from reordering.

**Prerequisites:** none — the codebase is ready. The cached
`(peeked_indent, trimmed, first_byte)` tuple from
`ba11228` is already in place at the top of
`step_in_document` and would serve as the dispatch key.

**Advisor needs:** test-engineer (both gates) +
security-engineer (both gates). The dispatch touches the
scalar entry point for untrusted input — same risk profile
as Task 3 of the throughput follow-up plan (`8650780`),
which required both advisors.

### Plans for reference

- `2026-04-11-parser-throughput-follow-up.md` — Completed.
  3 tasks: pos_after_line, span walks, scalar dispatch.
- `2026-04-12-step-dispatcher-micro-optimizations.md` —
  Completed. 2 tasks: cached trim + marker guards, probe
  reorder.

## Loader throughput regression (2026-04-16)

### Signal

Baremetal benchmark comparison vs documented (containerized)
numbers at `05d21fa` shows:

- **rlsp/load**: consistent −5 to −16% across fixtures
  (worst: `block_heavy` −16%, median −6%)
- **rlsp/events**: essentially flat (−1% to +5%, within noise)
- **libfyaml**: +5 to +21% (baremetal uncovered real speed
  that container noise hid — not comparable to rlsp delta)

Since `rlsp/load = parse_events + tree build`, and events is
flat, the slowdown is in **tree construction**. Ten parser
commits landed since `05d21fa`; three touch loader hot paths:

- **`168c0e3`** — +400 lines in loader.rs for nested
  comment preservation. Added `pending_leading` accumulator
  and per-entry combine logic. **Biggest suspect** given
  diff size.
- **`728d182`** — added `CollectionStyle` field to
  Mapping/Sequence nodes (+10 lines in loader).
- **`4740d10`** — DocumentStart/DocumentEnd flag capture
  (per-document, not per-node; minor).

**Caveat:** container-vs-baremetal mixes two variables.
Attribution requires baremetal A/B at pre-`05d21fa` vs HEAD.

### Flamegraph measurement (2026-04-16, baremetal)

Profile: `throughput_style/rlsp/load/block_heavy` on HEAD,
`--profile-time 10` with debug symbols. SVG at
`.ai/reports/flame-block_heavy-load.svg`. Sample totals as
percent of total CPU time:

| Frame | % | Category |
|-------|--:|----------|
| `LoadState::parse_node` (cumulative) | ~34% | Recursive tree build |
| `handle_mapping_entry` / `consume_mapping_entry` | ~16% | Block mapping handler |
| `step_in_document` | ~13% | Event dispatcher |
| `scan_line` (line buffer) | ~11% | Input splitting |
| `find_value_indicator_offset` | **7.3%** | Per-line `:` scan |
| `Vec::push` | 5.2% | Tree node push |
| `drop_in_place<Node>` | **8.8%** | Tree destruction |
| `drop_in_place<Vec<(Node,Node)>>` | 3.0% | Mapping entries drop |
| `drop_in_place<String>` | 3.0% | Scalar value drops |
| `drop_in_place<Vec<String>>` | **2.9%** | `leading_comments` Vec drop (empty on block_heavy!) |
| `node_end_line` | **3.0%** | Per-entry line query |
| `consume_leading_comments` | **3.0%** | Empty-stream comment drain |
| `column_at` | 3.0% | Pos math |
| `current_pos` | 3.0% | Pos construction |

**Total drop_in_place**: ~17.7% of bench time is tree
destruction (build/drop per criterion iteration). This is
entirely proportional to `sizeof(Node)` and number of nodes.

**Observations:**
- `drop_in_place<Vec<String>>` at 2.9% is the
  `leading_comments` Vec drop on EVERY node, even though
  block_heavy has zero comments. Pure carried cost.
- `consume_leading_comments` at 3.0% is all peek + empty
  Vec return — validates the 168c0e3 bookkeeping cost
  (now mitigated by L7, applied).

### Flamegraph measurement (2026-04-16, post-L3)

Same fixture, same command, after applying L5+L2+L7+L1+L3.
SVG at `.ai/reports/flame-block_heavy-load.svg`. Selected
frames:

| Frame | % | Note |
|-------|--:|------|
| `LoadState::parse_node` (cumulative) | ~40% | Up from ~34% pre-L5/L2; likely attribution shift from inlined helpers, not a real regression |
| `find_value_indicator_offset` (cumulative across 2 sites) | **~7.2%** | **Still the top remaining self-time frame; L6 target** |
| `consume_leading_comments` | **7.19%** | **Anomaly: L7 added `#[inline]` but the frame is still visible. Likely rustc declined inlining (function body includes a loop + `format!`-shaped work pre-L3 and a conditional peek loop post-L3) or the flame is attributing drop costs from the returned `Vec<String>` to this frame. Not a regression — absolute time per call is unchanged; the percentage rose because other frames shrank. Worth investigating later: examine the `.ll`/LLVM-IR output to confirm whether `#[inline]` took effect, and if not whether `#[inline(always)]` or a manual call-site inline is warranted.** |
| `drop_in_place<Node>` | 6.99% | Still dominant tree-destruction cost |
| `drop_in_place<String>` | 3.53% | Scalar value drops |
| `drop_in_place<Vec<(Node,Node)>>` | 3.46% | Mapping entries drop |
| `handle_mapping_entry` | ~5.8% | Block mapping event path |
| `step_in_document` (cumulative) | ~10% | Event dispatcher |

**Cumulative drop cost** on Node-related types in the
post-L3 flame: ~17.5% (essentially unchanged from
pre-L5/L2). This confirms L4 (Node shrink) is still the
biggest architectural lever.

### Applied optimizations

- **L5** (commit `9370579`) — `#[inline]` on
  `node_end_line` / `is_block_scalar`.
- **L2** (commit `d9afbdf`) — peek-first guard for
  trailing-comment detection.
- **L7** (commit `3f493a8`) — `#[inline]` on
  `consume_leading_comments` and `next_from`.
- **L1** (commit `a506589`) — skip anchor-subtree clone
  in Lossless mode.
- **L3** — replace `format!("#{text}")` with direct
  `with_hash_prefix` helper (4 call sites in loader +
  loader/stream). Targets comment-heavy workloads.

Combined measured effect on
`throughput_style/rlsp/load/block_heavy` (2026-04-16
baremetal):
- Pre-any-change baseline: 51.4 MiB/s
- Post L5+L2: 54.7 MiB/s (+6.4%)
- Post L5+L2+L7+L1: 55.2 MiB/s (+7.4% cumulative; +0.9%
  from L7+L1, consistent with expectation — L7 targeted
  the 3% `consume_leading_comments` cost but the inline
  hint appears to have been declined by rustc, see
  Follow-ups below)
- Post-L3: not yet re-measured (L3 targets
  comment-heavy fixtures; no block_heavy impact
  expected)

### Follow-ups surfaced during execution

- **`consume_leading_comments` and `with_hash_prefix` L7b
  inlining — RESOLVED.** Both functions confirmed
  non-inlined via LLVM IR at baseline (`define internal
  fastcc` symbols with live `invoke` call sites). Applied
  L7b (commit SHA recorded below):
  - `consume_leading_comments` split into
    `#[inline(always)]` wrapper (peek + early return on
    non-Comment) + private `consume_leading_comments_slow`
    helper (original while-loop, NOT `#[inline]`). The
    slow helper is the intentional out-of-line cold path.
  - `with_hash_prefix` promoted from `#[inline]` to
    `#[inline(always)]`.
  - LLVM IR verification after L7b:
    `grep -c "define.*consume_leading_comments[^_]"` → 0
    (wrapper inlined; slow path correctly has 1
    standalone definition).
    `grep -c "define.*with_hash_prefix"` → 0 (inlined).
  - Both clippy `inline_always` lints suppressed with
    `#[expect(..., reason = "...")]` per project rules.

### Applied: L4 (scoped variant)

After L5, L2, L7, L1, L3, and L6 were applied, one loader
candidate remained:

- **L4 (scoped)** — wrap `leading_comments: Vec<String>` →
  `Option<Vec<String>>` on all four `Node` variants.
  **Applied** (commit recorded in plan
  `2026-04-16-perf-l4-option-leading-comments.md`).
  This eliminates the per-node empty-Vec drop cost
  (~2.9% of bench time on `block_heavy`). `None` drops
  at zero cost; `Some` drops only when comments are
  actually present. The accessor signature is unchanged:
  `node.leading_comments() -> &[String]` returns `&[]`
  for `None` via `.as_deref().unwrap_or(&[])`.

  **Scoped variant landed first** to measure the
  drop-cost win without Box indirection cost. Throughput
  and flamegraph verification are run baremetal by the
  user in a follow-up step. Expected outcome on
  block_heavy rlsp/load: +1 to +4% (best estimate +2%).

### Deferred: L4 (full `Option<Box<NodeMeta>>` variant)

The broader boxing of all four rarely-populated fields into
a single `Option<Box<NodeMeta>>` remains deferred:

#### What it is

Each `Node::Scalar` currently carries:

- `anchor: Option<String>` — 24 B (usually None)
- `tag: Option<String>` — 24 B (usually None)
- `leading_comments: Option<Vec<String>>` — 24 B (usually None, after scoped L4)
- `trailing_comment: Option<String>` — 24 B (usually None)
- plus value + style + loc

The four rarely-populated fields account for ~96 B per Node
on no-comment/no-anchor/no-tag input. Collection variants
are larger. Cache lines hold 2–3 Nodes max.

Boxing them into a single `Option<Box<NodeMeta>>` would
shrink the common Node to fit 4+ per cache line, improving
tree-traversal locality for anything that reads the AST
(formatter, LSP).

**Why deferred:** the scoped L4 variant (above) was landed
first to isolate the measured drop-cost win from the
indirection cost that Box would introduce. If the scoped
variant shows no improvement on baremetal, the full
`Option<Box<NodeMeta>>` refactor should not be pursued
without new evidence. If it does show improvement, the
full variant becomes the next architectural candidate.

**Cost:** API change — `node.leading_comments()`,
`node.anchor()`, etc. stay the same shape, but internal
field access moves through a Box indirection. Not a
drop-in.

**Impact unclear without measurement** — could be a win
for throughput (smaller Node, better cache) and memory,
or a wash if the indirection cost dominates.

### Methodology for verification

Before fixing, confirm the regression is real (vs
environment-only artifact):

1. Check out `05d21fa` (last benchmarks.md update).
2. Run `cargo bench --bench throughput` baremetal,
   filtering to `throughput_style/rlsp/load/block_heavy`
   and `throughput/rlsp/load/medium_10KB`.
3. Compare to current HEAD baremetal run.
4. If ≥5% slower, bisect between `05d21fa` and HEAD.
5. If within ±2%, the "regression" is environment-only
   and the candidates above are still valid
   opportunities but not urgent.

## Other potential optimizations (not investigated)

These came up during the analysis but were not pursued:

1. **Arena allocation for `Event` queue.** The `VecDeque`
   used for multi-event steps allocates on the heap. An
   arena or small-vec optimization could reduce allocation
   pressure for steps that emit 2–4 events (common for
   collection open/close pairs). Low expected impact since
   Rust's allocator is already fast for small allocations.

2. **Lazy `Span` construction.** Instead of computing
   `Span { start, end }` eagerly for every event, store
   only `(start_byte_offset, end_byte_offset)` and compute
   `(line, column)` lazily when the consumer actually reads
   them. This would eliminate the `column_at` calls that
   `advance_within_line` still does. Significant API change
   to `Span`/`Pos` — would need a new plan.
