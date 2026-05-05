---
name: potential-performance-optimizations
description: Parser/loader throughput optimization candidates — four applied (2026-04-27/05-05), one still deferred (arena allocation). Applied work lives in plans + git log.
type: project
---

## Current state (2026-04-27)

A two-plan performance campaign recovered and exceeded the
2026-04-16 baremetal baseline:

- Plan 1 (`2026-04-26-parser-perf-recover-tag-allocations.md`):
  Cow tag URIs (`3f15780`) + first-byte schema dispatch
  (`a7206f6`). Partially recovered load throughput.
- Plan 2 (`2026-04-26-parser-perf-recover-node-event-meta-box.md`):
  5 stages (A–E) that fully recovered all 24 fixtures to
  within ±2% of baseline or better.

Final struct sizes: Node<Span> 120 bytes (was 288),
Event 40 bytes (was ~112), Span 8 bytes (was 48).
First-event latency 36.7 ns (was 38.9 ns baseline — now
5–9% faster). Real-world kubernetes events throughput
143.6 MiB/s (libfyaml 123.1 — rlsp 17% faster).

## Applied candidates

### 1. `step_in_document` byte-dispatch (was Option D)

**Applied:** Stage D of Plan 2, commit `9bd368e`.
Single-peek byte-dispatch replaced the 10–15 sequential
probe cascade. Measured 2–8% improvement on load fixtures.
Stage E (`ccdfc1a`) added `#[inline]` hints to the schema-
resolution chain for an additional ~2% on small fixtures.

### 2. `Option<Box<NodeMeta>>` (was L4 full)

**Applied:** Stage A of Plan 2, commit `d853605`.
Hybrid variant: `anchor`, `anchor_loc`, `tag_loc`,
`leading_comments`, `trailing_comment` boxed behind
`Option<Box<NodeMeta>>`. Tag stays inline (schema resolver
populates it on every node). Node<Span> 288 → 120 bytes.
Stage B (`76904a9`) applied the same pattern to Event
(`Option<Box<EventMeta>>`, 40 bytes).

The earlier scoped-L4 variant (commit `e812232`) measured
flat because it only boxed comments. The full variant
measured a dramatic win because the 2026-04-20 anchor/tag
span work had widened Node by ~112 bytes since the scoped
test — the evidence the memory file said to wait for.

### 3. Lazy `Span` construction

**Applied:** Stage C of Plan 2, commit `716771f`.
Span = `(start: u32, end: u32)` = 8 bytes. Line/column
resolved on demand via `LineIndex` (built once per parse,
shared via `Arc<LineIndex>`). `Pos` retained at 24 bytes
for internal lexer use and error reporting.

### 4. c-printable enforcement pre-scan fast-path

**Applied:** 2026-05-05 (plan `2026-05-05-prescan-flag-implementation.md`).
Single O(n) pre-scan of the full input in `Lexer::new()` via
`find_non_c_printable(input.as_bytes()).is_none()`. Result stored in
`Lexer::input_all_printable: bool`. When `true`, all 11 per-scanner
`find_non_c_printable` / `find_non_nb_json` calls are skipped — one flag
check per call site replaces an O(m) scan. `scan_double_quoted_line` received
a `skip_char_validation: bool` parameter; callers pass `self.input_all_printable`.
On valid YAML (the common case), zero per-scalar validation overhead.

## Still deferred

### Arena allocation for `Event` queue

The `VecDeque<(Event, Span)>` used for multi-event parser
steps allocates on the heap. An arena or small-vec
optimization could reduce allocation pressure for steps
that emit 2–4 events.

**Why still deferred:** Event is now 40 bytes (post
Stage B boxing), so the per-event allocation cost is much
lower than when this candidate was first identified. No
flamegraph evidence of allocation overhead in the current
code. Worth revisiting only if a future profile surfaces
allocation overhead.


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
