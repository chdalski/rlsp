---
name: potential-performance-optimizations
description: Deferred parser perf candidate (arena allocation for the Event queue) plus the baremetal verification methodology to use before opening any perf plan
metadata:
  type: project
---

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
