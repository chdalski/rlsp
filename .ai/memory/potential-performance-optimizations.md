---
name: potential-performance-optimizations
description: Deferred parser/loader throughput optimizations — Option D (step_in_document restructure), full Option<Box<NodeMeta>>, arena allocation for event queue, lazy Span construction. Applied work lives in plans + git log.
type: project
---

## Current state (2026-04-16)

rlsp-yaml-parser is post-campaign. The 8-plan loader
performance push (commits `9370579` L5, `d9afbdf` L2,
`3f493a8` L7, `a506589` L1, `d586012` L3, `8097aa5` L6,
`e812232` L4 scoped, `3bec2da` L7b) closed the
container-vs-baremetal regression the user surfaced on
2026-04-16 and brought rlsp to parity-or-faster vs
libfyaml on ~7/10 fixtures while keeping the ~19× latency
advantage. Detail for each applied change lives in its
plan file under `.ai/plans/2026-04-16-perf-*.md` and the
matching git commit.

The benchmarks.md doc refresh (reflecting the post-L7b
state) is the separate follow-up plan at
`.ai/plans/2026-04-16-docs-benchmarks-refresh.md`.

## Deferred candidates

Four architectural candidates remain open. None has a
committed plan yet — each needs fresh evidence before it
is worth the refactor cost, since the 8-plan campaign
already exhausted the clearly-measured gains.

### 1. Option D — full `step_in_document` restructure

Replace the linear if-else probe cascade in
`src/event_iter/step.rs:step_in_document` (~760 lines)
with a single-peek, single-trim, byte-dispatch structure.
Today the function runs 10–15 sequential probes per step;
a restructure would do one peek + one `match` on the
first non-whitespace byte + one handler call.

**Why deferred:** the cached-trim + probe-reorder
diagnostics (commits `ba11228`, `4728ea3`) confirmed the
cascade is a measurable cost, but the remaining gap after
the 2026-04-16 campaign is likely dominated by
fundamentals (Rust bounds-checking vs C unchecked,
`VecDeque` overhead, `Span` construction, Comment event
emission that libfyaml skips) rather than dispatch.

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
    Some(b'|' | b'>' | b'\'' | b'"') → scalar
    _ → mapping-or-plain detection
}
```

**Hard parts:**
1. Mapping entries can start with ANY character
   (`key: value` starts with `k`) —
   `find_value_indicator_offset` still needed in the
   fallthrough path.
2. `-` is ambiguous (sequence entry / `---` marker /
   `-value` plain scalar) — needs second-byte context.
3. Dedent detection depends on indent + collection stack,
   not first byte — stays in post-dispatch fallthrough.
4. 760-line function rewrite touches the entry point for
   every parse event. Full conformance suite +
   `unicode_positions` + `smoke` tests must stay green.

**Estimated impact:** 5–15% on `block_sequence`, 2–8%
elsewhere. Based on the cached-trim diagnostic (~6–8%)
plus the probe-reorder diagnostic (~5%).

**Advisor needs:** test-engineer + security-engineer
(both gates each). The dispatch touches the scalar entry
point for untrusted input.

### 2. L4 full — `Option<Box<NodeMeta>>` variant

The broader boxing of all four rarely-populated Node
fields (`anchor`, `tag`, `leading_comments` post-scoped-L4,
`trailing_comment`) into a single `Option<Box<NodeMeta>>`.
Would shrink the common Node to fit 4+ per cache line,
improving tree-traversal locality for any AST consumer
(formatter, LSP).

**Why deferred:** the scoped L4 variant that landed in
`e812232` measured flat on block_heavy throughput even
though it eliminated the specific `drop_in_place<Vec<String>>`
frame. That is evidence the per-field drop costs are
amortized below measurable throughput threshold on current
fixtures. The full variant's win comes from cache
locality, which is even harder to measure and more at risk
of being cancelled by the Box indirection cost on every
accessor call. Do not pursue without new evidence of a
cache-bound bottleneck.

**Cost:** API shape preserved (`node.leading_comments()`,
`node.anchor()`, etc. stay the same), but internal field
access moves through a Box indirection. Touches ~251
consumer occurrences across `rlsp-yaml` + ~103 parser
occurrences.

### 3. Arena allocation for `Event` queue

The `VecDeque<(Event, Span)>` used for multi-event parser
steps allocates on the heap. An arena or small-vec
optimization could reduce allocation pressure for steps
that emit 2–4 events (common for collection open/close
pairs).

**Why deferred:** low expected impact — Rust's allocator
is already fast for small allocations, and the 2026-04-16
flamegraph shows no allocation-related frame in the event
iterator hot path. Worth revisiting only if a future
profile surfaces allocation overhead.

### 4. Lazy `Span` construction

Store `Span` as `(start_byte_offset, end_byte_offset)`
and compute `(line, column)` lazily when the consumer
actually reads them. Would eliminate `column_at` calls
that `advance_within_line` still makes. Significant API
change to `Span`/`Pos`.

**Why deferred:** `column_at` no longer appears in the
post-campaign flamegraph as a measurable self-time frame
(it was 3.0% pre-campaign). The motivation weakened.
Still a valid architectural cleanup if the `Span` API
itself is being revisited for other reasons.

## Methodology for verification

Before opening a new perf plan, confirm the opportunity
is real (vs environment-only artifact or prior-run noise):

1. Capture a baseline baremetal flamegraph on HEAD using
   the pre-built-binary approach:
   ```
   CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p rlsp-yaml-parser --bench throughput --no-run
   flamegraph -o .ai/reports/flame-<fixture>-load.svg -- target/release/deps/throughput-<hash> --bench --profile-time 10 '<filter>'
   ```
   Running `cargo flamegraph` directly mis-profiles cargo
   itself.
2. Record median MiB/s from
   `cargo bench -p rlsp-yaml-parser --bench throughput`
   against the target fixture. Criterion reports
   run-to-run change — use that to distinguish signal
   from noise (typically ±2%, up to ±8% on this
   machine's thermal range).
3. For any claim about rlsp-vs-libfyaml ratio, include
   libfyaml's absolute change in the same run —
   libfyaml can swing ±20% from thermal alone on some
   fixtures (block_sequence).
4. Verify inlining hints took effect with LLVM IR:
   ```
   cargo rustc -p rlsp-yaml-parser --release --lib -- --emit=llvm-ir -O
   grep -c "define.*<fn_name>" target/release/deps/rlsp_yaml_parser-*.ll
   ```
   A return of 0 confirms no standalone definition
   survives (either inlined or the symbol is not
   referenced). `#[inline]` is a hint; `#[inline(always)]`
   is stronger but widens caller code.
