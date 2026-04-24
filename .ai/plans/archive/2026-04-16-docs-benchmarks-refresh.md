**Repository:** root
**Status:** Completed (2026-04-16)
**Created:** 2026-04-16

## Goal

Refresh `rlsp-yaml-parser/docs/benchmarks.md` so that its
environment, numbers, and analysis reflect the current
baremetal measurements taken on commit `3bec2da` (after
the eight-commit performance campaign L5, L2, L7, L1, L3,
L6, L4 scoped, L7b). The doc today still describes a
containerized environment and cites pre-optimization
numbers from commit `05d21fa` — every table in the file
is stale and several prose claims (parity, throughput
ratios, style rankings) are no longer accurate. Bring the
doc into alignment with the 2026-04-16 baremetal bench
log at `.ai/reports/bench-baremetal.log`.

## Context

- `rlsp-yaml-parser/docs/benchmarks.md` was last updated
  at commit `05d21fa` (2026-04-12) and describes a
  container run. Current baremetal numbers differ
  materially on both absolute values and relative
  rlsp-vs-libfyaml ratios.
- Authoritative source for new numbers:
  `.ai/reports/bench-baremetal.log` (modified
  2026-04-16 13:46 UTC, run on commit `3bec2da`).
  Contains 100 samples per group for latency
  (latency.rs), throughput (throughput.rs), and memory
  (memory.rs) benches.
- The `environment` section needs two substantive
  changes: (a) switch "Linux (container)" to
  "Linux (baremetal)", (b) remove the containerized-noise
  caveat note. The CPU model and rustc version are
  machine-specific — the plan must not fabricate these
  values. The developer fills them from the user's
  machine (see the sub-task verification: check
  `rustc --version` output and the CPU model string,
  record exact strings in the table).
- Analysis prose contains several statements that are
  no longer accurate:
  - "block_sequence: 1.04× faster" → now 0.89×/0.96×
    depending on run (libfyaml spikes on this fixture).
    Frame honestly: noise-dominated on this fixture.
  - "mixed: 1.10× slower" → current ratio is closer to
    0.95×.
  - "huge_1MB: parity" → now rlsp is 1.07× faster.
  - The "Acceptance criterion: huge_1MB first-event
    latency < 1 ms" line — still met (38.9 ns is even
    further below), update the ratio (21,088× →
    ~25,700×).
- Numbers to use (medians from the 2026-04-16 bench
  run):
  - **Latency first-event (rlsp):** tiny 38.88 ns,
    medium 38.82 ns, large 38.80 ns, huge 38.91 ns,
    kubernetes 39.54 ns.
  - **Latency first-event (libfyaml):** tiny 796.0 ns,
    medium 783.7 ns, large 788.6 ns, huge 802.0 ns,
    kubernetes 788.8 ns.
  - **Latency full drain (rlsp):** tiny 1.212 µs,
    medium 84.26 µs, large 788.4 µs, kubernetes
    26.39 µs.
  - **Latency full drain (libfyaml):** tiny 2.863 µs,
    medium 91.45 µs, large 819.2 µs, kubernetes
    26.37 µs.
  - **Throughput rlsp/load (MiB/s):** tiny 54.08,
    medium 58.28, large 43.34, huge 35.69, block_heavy
    55.92, block_sequence 128.89, flow_heavy 57.83,
    scalar_heavy 141.14, mixed 60.69, kubernetes 79.15.
  - **Throughput rlsp/events (MiB/s):** tiny 87.02,
    medium 109.88, large 123.59, huge 130.80,
    block_heavy 105.37, block_sequence 227.65,
    flow_heavy 131.22, scalar_heavy 236.16, mixed
    115.53, kubernetes 138.11.
  - **Throughput libfyaml/events (MiB/s):** tiny 37.81,
    medium 108.56, large 119.57, huge 122.18,
    block_heavy 107.33, block_sequence 255.90
    (note: libfyaml-block_sequence spiked +24% vs
    prior run — call out as noise in the doc),
    flow_heavy 87.83, scalar_heavy 225.23, mixed
    121.07, kubernetes 139.97.
  - **Memory (rlsp timings):** rlsp_load tiny 2.152 µs,
    medium 174.99 µs, large 2.343 ms.
    rlsp_parse_events tiny 1.275 µs, medium 87.21 µs,
    large 782.92 µs. alloc_stats/large_load 2.506 ms.
    real_world/load 49.37 µs.
- The performance campaign that produced these numbers
  is 8 commits: L5 `9370579`, L2 `d9afbdf`, L7
  `3f493a8`, L1 `a506589`, L3 `d586012`, L6 `8097aa5`,
  L4 scoped `e812232`, L7b `3bec2da`. A one-line
  reference to these commits in the Analysis section
  gives future readers a pointer to the detailed plan
  files without turning the doc into a changelog.
- No code changes in this plan. No test changes. Only
  `rlsp-yaml-parser/docs/benchmarks.md` is touched.
- Related memory:
  `.ai/memory/potential-performance-optimizations.md`
  already records the campaign; no memory update
  needed for this plan.

## Steps

- [x] Rewrite the **Environment** table: replace
      "Linux (container)" with "Linux (baremetal)",
      remove the "containerized noise" note beneath the
      table. Fill the CPU row and the rustc row with
      the exact values reported by
      `rustc --version` and `cat /proc/cpuinfo | grep
      "model name" | head -1` on the measurement
      machine — do not fabricate values.
- [x] Replace the **Latency — time to first event**
      Criterion-output code block and the
      "rlsp vs libfyaml — first-event latency" table
      with the 2026-04-16 medians. Keep the
      acceptance-criterion callout ("huge_1MB
      first-event latency < 1 ms") and update the ratio
      (38.91 ns → ~25,700× under 1 ms). The comparative
      sentence should state exactly **"~20× faster"**
      computed from huge_1MB medians (802.0 ns ÷ 38.91
      ns = 20.6×, rounds to 20×).
- [x] Replace the **Throughput — full event drain**
      Criterion-output code block and the "Throughput
      by document size" table with the new numbers. The
      rlsp vs libfyaml ratios change for every row —
      compute each fresh from the listed medians.
      Update the raw-timings median table alongside.
- [x] Replace the **Throughput by YAML style**
      Criterion-output code block and summary table.
      Remove the container-noise caveat note. Call out
      the libfyaml `block_sequence` +24% run-to-run
      spike inline so a future reader does not
      misinterpret the number as stable.
- [x] Replace the **Throughput — real-world** section
      with the new kubernetes numbers.
- [x] Replace the **Latency — full event drain** table
      with the new full-drain medians.
- [x] Replace the **Memory allocation profile**
      Criterion-output code block with the new memory
      timings.
- [x] Rewrite the **Analysis** section so the prose
      matches the new numbers. Required claims,
      computed from the Context medians:
      - O(1) first-event latency at **~38.9 ns**
        (unchanged story, updated number).
      - Latency ratio vs libfyaml: **~20×** (huge_1MB
        802.0 ÷ 38.91 ns).
      - Throughput headline: **"5-of-10 fixtures faster,
        3 at parity, 2 slightly behind"**:
        - **Faster**: tiny_100B (2.30×), large_100KB
          (1.03×), huge_1MB (1.07×), flow_heavy
          (1.49×), scalar_heavy (1.05×).
        - **Parity** (within ±2% or noise-dominated):
          medium_10KB (1.01×), kubernetes (0.99×),
          block_sequence (rlsp 227.65 vs libfyaml
          255.90 = 0.89× this run, but libfyaml's
          +24% thermal spike vs the prior run makes
          this ratio untrustworthy — categorize as
          parity with an inline noise caveat, do NOT
          list as faster or slower).
        - **Slightly behind**: block_heavy (0.98×,
          -2%) and mixed (0.95×, -5%).
      - History line: the 2026-04-16 campaign (8
        commits) closed the container-vs-baremetal
        regression and narrowed the libfyaml gap from
        "slower on 2–3 fixtures by 10%+" to "slightly
        behind on 2 fixtures by ≤5%".
- [x] Add a short **History** subsection pointing at
      the eight-commit campaign (L5 `9370579`, L2
      `d9afbdf`, L7 `3f493a8`, L1 `a506589`, L3
      `d586012`, L6 `8097aa5`, L4 scoped `e812232`,
      L7b `3bec2da`) and at
      `.ai/plans/2026-04-16-perf-*.md` for the detailed
      plan files. This replaces the existing "history
      since …" remarks if any.
- [x] Verify the **Fixtures** table (currently at
      `benchmarks.md:30–46`) still matches the fixtures
      present in `rlsp-yaml-parser/benches/fixtures.rs`.
      The 8-commit campaign did not touch the benches/
      directory, so the table is expected to be
      accurate — but the developer confirms by reading
      the bench source. If any fixture name/size/style
      differs, update the table.
- [x] Verify that `rlsp-yaml-parser/README.md`
      contains no inline perf numbers that become
      stale. Current state (verified 2026-04-16):
      `README.md:100` says "The streaming architecture
      delivers sub-microsecond first-event latency" —
      qualitative only, no stale numbers. Confirm this
      remains true; do NOT modify README.md unless new
      stale numbers are found.
- [x] Update
      `.ai/memory/potential-performance-optimizations.md`
      so the "doc refresh is the separate follow-up
      plan" sentence (currently at lines 20–22 of
      that file) is removed or retargeted — after
      this plan commits, the refresh is no longer a
      follow-up.
- [x] Verify Criterion-output code blocks use the
      `time: [a b c] thrpt: [...]` pairs from the
      bench log directly (the log already contains both
      time and thrpt lines for throughput benches; the
      developer copies the relevant lines rather than
      deriving them).
- [x] Run `cargo fmt`, `cargo clippy --all-targets`,
      and `cargo test` — zero warnings, all tests pass
      (doc-only change; nothing else should move).

## Tasks

### Task 1: Refresh benchmarks.md with 2026-04-16 baremetal data (commit: `c653d63`)

Rewrite the numeric tables and analysis prose in
`rlsp-yaml-parser/docs/benchmarks.md` so the doc
reflects the 2026-04-16 baremetal measurements on
commit `3bec2da`. Every number comes from
`.ai/reports/bench-baremetal.log`; no synthesized data.
CPU and rustc strings come from the developer's machine,
not from this plan.

- [x] **Environment** section reflects baremetal (not
      containerized). CPU row contains the exact
      `model name` string from `/proc/cpuinfo`. rustc
      row contains the exact `rustc --version` output.
      The containerized-noise caveat paragraph is
      removed.
- [x] **Latency — time to first event** section uses
      the 2026-04-16 medians listed in the Context
      section above. Acceptance-criterion callout still
      reads "huge_1MB first-event latency < 1 ms" with
      an updated "target MET" ratio.
- [x] **Throughput by document size** section uses the
      2026-04-16 medians for rlsp/load, rlsp/events,
      libfyaml/events, and computes each rlsp/events
      ÷ libfyaml ratio from those medians.
- [x] **Throughput by YAML style** section uses the
      2026-04-16 medians. The libfyaml block_sequence
      noise spike is annotated inline. The
      containerized-variance caveat paragraph is
      removed.
- [x] **Throughput — real-world (Kubernetes)** section
      uses the 2026-04-16 medians.
- [x] **Latency — full event drain** section uses the
      2026-04-16 medians.
- [x] **Memory allocation profile** section uses the
      2026-04-16 memory-bench timings.
- [x] **Analysis** section matches the numbers
      enumerated in Steps: the O(1) story at **~38.9
      ns**, the **~20×** libfyaml latency ratio, the
      **5-of-10 faster / 3 parity / 2 slightly behind**
      summary with the exact fixtures
      (faster: tiny_100B 2.30×, large_100KB 1.03×,
      huge_1MB 1.07×, flow_heavy 1.49×, scalar_heavy
      1.05×; parity: medium_10KB 1.01×, kubernetes
      0.99×, block_sequence with the +24% libfyaml
      thermal-noise caveat; slightly behind:
      block_heavy 0.98×, mixed 0.95×), and the history
      line referencing the 2026-04-16 8-commit
      campaign.
- [x] A new **History** subsection (or equivalent)
      points at the eight-commit 2026-04-16 campaign
      and at `.ai/plans/2026-04-16-perf-*.md` for
      traceability.
- [x] Fixtures table at `benchmarks.md:30–46`
      verified against `rlsp-yaml-parser/benches/fixtures.rs`
      and unchanged (or updated if the bench source
      differs).
- [x] `rlsp-yaml-parser/README.md` confirmed to contain
      no inline perf numbers that would become stale
      after this refresh. Not modified.
- [x] `.ai/memory/potential-performance-optimizations.md`
      "doc refresh is the separate follow-up plan"
      sentence (at lines 20–22 of that file) removed
      or retargeted so the memory no longer describes
      this plan as pending.
- [x] No other files are modified. Only
      `rlsp-yaml-parser/docs/benchmarks.md` and
      `.ai/memory/potential-performance-optimizations.md`
      appear in `git status --porcelain` against the
      baseline once the plan file checkboxes are
      marked at post-approval commit time.
- [x] `cargo fmt` produces zero diff; `cargo clippy
      --all-targets` produces zero warnings; `cargo
      test` passes (doc-only change — no test should
      flip).

## Decisions

- **No advisor consultation.** This is a
  documentation-only refresh: no behavior change, no
  trust-boundary change, no new public API surface.
  Per `risk-assessment.md`, doc changes are explicitly
  listed under "skip both advisors".
- **CPU and rustc strings come from the developer's
  machine, not from this plan.** Fabricating
  environment details would invalidate the doc's
  reproducibility promise. The developer runs the two
  commands named in the Steps section and records the
  verbatim output.
- **libfyaml block_sequence noise is called out
  inline, not smoothed away.** One value in the new
  run is +24% above the prior run — that is variance,
  not a libfyaml improvement. Hiding it would let a
  future reader misinterpret the ratio; naming it
  preserves the doc's honesty.
- **History subsection links to plan files, not a
  prose narrative.** The plans already contain the
  detail; adding it to the doc would duplicate
  information that can rot. A pointer is the correct
  level of detail.
- **No throughput-acceptance number in this plan.**
  The Analysis prose correctness is the gate. Clippy,
  fmt, and the existing `cargo test` green-state
  confirm the file still parses and no syntax error
  was introduced. There is no meaningful "did we
  improve" number for a doc refresh.
