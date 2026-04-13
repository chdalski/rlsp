---
name: potential-performance-optimizations
description: Deferred parser throughput optimizations — Option D (step_in_document restructure) and other ideas catalogued during the 2026-04-11/12 perf work
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
