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

### Applied optimizations

- **L5** (commit `9370579`) — `#[inline]` on
  `node_end_line` / `is_block_scalar`.
- **L2** (commit `d9afbdf`) — peek-first guard for
  trailing-comment detection.
- **L7** (commit `3f493a8`) — `#[inline]` on
  `consume_leading_comments` and `next_from`.
- **L1** (commit `a506589`) — skip anchor-subtree clone
  in Lossless mode.

Combined measured effect on
`throughput_style/rlsp/load/block_heavy` (2026-04-16
baremetal pre-L5/L2 vs post-L5/L2): 51.4 → 54.7 MiB/s
(+6.4%). L1 and L7 not re-measured individually.

### Remaining candidates, reprioritized by flamegraph

**Ranking for block_heavy:**

1. **L6** — merge `find_value_indicator_offset` with
   plain-scalar scan. **Target: 7.3% self-time.** Highest
   measured payoff remaining.
2. **L4** — shrink `Node` variant (box rarely-populated
   fields). **Target: up to ~6% of the 17.7% drop cost +
   cache-locality wins.** Architectural, needs a plan.
3. **L3** — replace `format!("#{text}")` with direct
   `push`. No impact on block_heavy (zero comments);
   targets comment-heavy workloads.

#### L6 — Merge `find_value_indicator_offset` with plain scan

**Flame: 7.3% of total time.** Every candidate mapping-key
line is scanned twice — once by
`find_value_indicator_offset` (looking for `: `), once by
`scan_plain_line_block` (looking for `:` and `#` via
`memchr2`). A merged single-pass scanner could eliminate the
redundant walk.

**Prereqs:** touches two hot modules (`event_iter/
line_mapping.rs` + `lexer/plain.rs`). Moderate complexity.

**Advisor needs:** test-engineer (scanner behavior covers
many YAML grammar edge cases); no security gate needed for
pure refactor.

#### L3 — Avoid `format!("#{text}")` for comments

`loader.rs:640`, `loader/stream.rs:37,56,80` — every
Comment event allocates via `format!`. Replace with:

```
let mut s = String::with_capacity(text.len() + 1);
s.push('#');
s.push_str(text);
```

**Why:** `format!` goes through the `fmt::Write` machinery
— more overhead than direct push. Minor per comment,
adds up on comment-heavy documents.

Better alternative: have the lexer/event iter emit
`Event::Comment { text }` with `#` already included so the
loader can just clone the `Cow` into a `String`. This
changes the event-stream contract, so it's a bigger
decision.

#### L4 — Shrink `Node` variant size by boxing rare fields

Each `Node::Scalar` carries:

- `anchor: Option<String>` — 24 B (usually None)
- `tag: Option<String>` — 24 B (usually None)
- `leading_comments: Vec<String>` — 24 B (usually empty)
- `trailing_comment: Option<String>` — 24 B (usually None)
- plus value + style + loc

The four rarely-populated fields account for ~96 B per Node
on no-comment/no-anchor/no-tag input. Collection variants
are larger. Cache lines hold 2–3 Nodes max.

Boxing rarely-populated fields into a single
`Option<Box<NodeMeta>>` would shrink the common Node to
fit 4+ per cache line, improving tree-traversal locality
for anything that reads the AST (formatter, LSP).

**Why:** most documents have zero comments and zero
anchors/tags. The fields exist in the layout regardless.

**Cost:** API change — `node.leading_comments()` stays the
same shape, but internal field access moves through a Box
indirection. Not a drop-in.

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

1. **Merge `find_value_indicator_offset` with
   `scan_plain_line_block`.** Both scan the same line
   content — one looks for `: `, the other for `:` and `#`
   via `memchr2`. A merged single-pass scanner could
   eliminate one redundant scan per mapping-entry line.
   Moderate complexity; touches two hot paths in different
   modules (`event_iter/line_mapping.rs` and
   `lexer/plain.rs`).

2. **Arena allocation for `Event` queue.** The `VecDeque`
   used for multi-event steps allocates on the heap. An
   arena or small-vec optimization could reduce allocation
   pressure for steps that emit 2–4 events (common for
   collection open/close pairs). Low expected impact since
   Rust's allocator is already fast for small allocations.

3. **Lazy `Span` construction.** Instead of computing
   `Span { start, end }` eagerly for every event, store
   only `(start_byte_offset, end_byte_offset)` and compute
   `(line, column)` lazily when the consumer actually reads
   them. This would eliminate the `column_at` calls that
   `advance_within_line` still does. Significant API change
   to `Span`/`Pos` — would need a new plan.
