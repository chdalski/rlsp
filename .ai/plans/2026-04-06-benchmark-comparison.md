**Repository:** root
**Status:** InProgress
**Created:** 2026-04-06

## Goal

Compare rlsp-yaml-parser performance against libfyaml (C)
across throughput, latency, and memory. Fix the existing
apples-to-oranges throughput comparison, add a real-world
YAML fixture, run all benchmarks, and write a comparison
document.

## Context

- Benchmark infrastructure exists in
  `rlsp-yaml-parser/benches/` — three Criterion bench
  targets (throughput, latency, memory) with libfyaml FFI
- libfyaml 0.8 installed in dev environment; compiles and
  links successfully
- **Throughput gap:** compares rlsp `load()` (builds full
  node tree) against libfyaml events (stream drain) — not
  apples-to-apples. Need rlsp `parse_events` drain for
  fair event-level comparison.
- **Latency gap:** first-event comparison exists for both.
  Full drain exists only for rlsp — need libfyaml full
  drain for parity.
- **Memory:** counting allocator intercepts Rust's global
  allocator only. libfyaml uses C malloc — cannot be
  captured. Keep memory as rlsp-only profiling.
- **Fixtures:** synthetic only (tiny/medium/large/huge +
  5 styles). A real Kubernetes manifest would better
  represent the LSP use case.
- Prior work: workaround removal plan completed 2026-04-06

## Steps

- [x] Add rlsp `parse_events` to throughput benchmarks
- [x] Add libfyaml full-drain to latency benchmarks
- [ ] Add real-world YAML fixture (Kubernetes deployment)
- [ ] Run all benchmarks and collect results
- [ ] Write benchmark comparison document

## Tasks

### Task 1: Add event-level parity to benchmarks ✓ ec00e1d

Fix the apples-to-oranges comparison so both parsers are
measured at the same abstraction level.

**Throughput (`throughput.rs`):**
- Add `throughput/rlsp_events` group benchmarking
  `parse_events().count()` by size (tiny through huge)
- Add `throughput_style/rlsp_events` group by style
- Keep existing `throughput/rlsp` (`load()`) as "full
  pipeline" comparison
- Keep existing `throughput/libfyaml` groups unchanged

**Latency (`latency.rs`):**
- Add `latency/libfyaml_full` group benchmarking libfyaml
  full event drain by size (tiny through large)
- Reuse the existing `libfyaml_parse_all()` FFI function
  from throughput — extract to shared fixture or duplicate
  the minimal FFI block

Files: `benches/throughput.rs`, `benches/latency.rs`

- [x] `throughput/rlsp_events` group (by size)
- [x] `throughput_style/rlsp_events` group (by style)
- [x] `latency/libfyaml_full` group (by size)
- [x] All benchmarks compile and run

### Task 2: Add real-world YAML fixture

Add a representative Kubernetes Deployment manifest to the
fixture module and benchmark it across all three targets.

**Fixture (`fixtures.rs`):**
- Add `kubernetes_deployment() -> String` returning a
  realistic ~3KB Deployment manifest (metadata, labels,
  spec with containers, env vars, volume mounts, probes,
  resource limits)
- Hardcoded string, not generated — deterministic and
  representative of real LSP input

**Benchmarks:**
- `throughput.rs`: add real-world group comparing all three
  APIs (rlsp load, rlsp events, libfyaml events)
- `latency.rs`: add real-world first-event + full-drain
  comparison
- `memory.rs`: add real-world allocation profiling (rlsp
  only, per existing pattern)

Files: `benches/fixtures.rs`, `benches/throughput.rs`,
`benches/latency.rs`, `benches/memory.rs`

- [ ] `kubernetes_deployment()` fixture function
- [ ] Real-world benchmark groups in all three bench files
- [ ] All benchmarks compile and run

### Task 3: Run benchmarks and document results

Run all benchmarks, collect Criterion output, and write the
comparison document.

- Run `cargo bench` in `rlsp-yaml-parser/`
- Create `rlsp-yaml-parser/docs/benchmarks.md` with:
  - **Methodology:** what's measured, fixture descriptions,
    Criterion parameters, environment (rustc version, CPU)
  - **Throughput:** MB/s tables for event-level and full
    pipeline, by size and style
  - **Latency:** time-to-first-event and full-parse tables
  - **Memory:** allocation bytes/count by size (rlsp only,
    with note on why libfyaml is excluded)
  - **Real-world:** Kubernetes fixture results across all
    dimensions
  - **Summary:** where rlsp-yaml-parser wins, loses, and
    why — focusing on the trade-offs (Rust safety +
    lossless spans + comments vs C performance)

Files: `rlsp-yaml-parser/docs/benchmarks.md`

- [ ] All benchmarks run successfully
- [ ] Results document with tables and analysis
- [ ] Methodology section documents limitations

## Decisions

- **Memory: rlsp-only** — libfyaml uses C malloc which the
  Rust counting allocator cannot intercept. C-level
  allocation tracking (LD_PRELOAD, jemalloc stats) is out
  of scope. Document this limitation in the results.
- **Three comparison levels** — event-level (fair), full
  pipeline (shows tree construction cost), first-event
  (LSP latency). Covers the full picture without
  overcomplicating.
- **Kubernetes Deployment as real-world fixture** — most
  representative for the LSP use case, which is the primary
  consumer of this parser.
- **FFI duplication** — the libfyaml FFI block is duplicated
  across throughput.rs and latency.rs rather than shared
  via a common module. Criterion bench targets are separate
  binaries; sharing FFI declarations would require a
  build-script or separate crate. The duplication is ~30
  lines of C struct declarations and is acceptable for
  benchmark infrastructure.
