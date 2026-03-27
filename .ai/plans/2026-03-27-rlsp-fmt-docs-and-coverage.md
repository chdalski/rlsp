**Repository:** root
**Status:** InProgress
**Created:** 2026-03-27

## Goal

Improve docs.rs quality for `rlsp-fmt` (crate-level docs, doc
examples on public API) and add a brief crate description for
`rlsp-yaml`. Close the 7-line coverage gap in `rlsp-fmt`'s
`fits()` function by removing dead `Mode::Break` branches that
are unreachable.

## Context

- **docs.rs gap:** Neither crate has `//!` crate-level doc
  comments in `lib.rs`, so docs.rs landing pages are empty.
  `rlsp-fmt` is a library crate that external users may consume,
  so comprehensive docs matter. `rlsp-yaml` is primarily a
  binary, so a brief description suffices.
- **Coverage gap:** Codecov shows `printer.rs` at 96.87% — 7
  missed lines (135-140, 157) are all `Mode::Break` arms inside
  the `fits()` function. `fits()` is always called in `Flat`
  mode and nested groups push `Flat` (line 152), so `Break` mode
  is unreachable. Removing these dead branches eliminates the gap
  and simplifies the function.
- **Key files:**
  - `rlsp-fmt/src/lib.rs` — needs `//!` crate docs
  - `rlsp-fmt/src/ir.rs` — builder functions need doc examples
  - `rlsp-fmt/src/printer.rs` — `format()` needs doc example;
    `fits()` has dead code to remove
  - `rlsp-yaml/src/lib.rs` — needs brief `//!` crate description

## Steps

- [x] Analyze codecov data and identify uncovered lines
- [x] Read all relevant source files
- [x] Add crate-level `//!` docs to `rlsp-fmt/src/lib.rs` (a3b9bfa)
- [x] Add doc examples to builder functions in `rlsp-fmt/src/ir.rs` (a3b9bfa)
- [x] Add doc example to `format()` in `rlsp-fmt/src/printer.rs` (a3b9bfa)
- [ ] Remove dead `Mode::Break` branches from `fits()` in `printer.rs`
- [ ] Add brief `//!` crate description to `rlsp-yaml/src/lib.rs`
- [ ] Run `cargo test`, `cargo clippy`, `cargo doc`

## Tasks

### Task 1: Add crate-level docs and doc examples to rlsp-fmt

Add `//!` crate-level documentation to `rlsp-fmt/src/lib.rs`
with an overview of the Wadler-Lindig algorithm, a quick usage
example, and re-export documentation. Add `///` doc examples to
builder functions in `ir.rs` and to `format()` in `printer.rs`.

- [x] `lib.rs`: crate-level `//!` docs with overview and example (a3b9bfa)
- [x] `ir.rs`: doc examples on all 8 builder functions (a3b9bfa)
- [x] `printer.rs`: doc example on `format()` (a3b9bfa)
- [x] All 10 doc examples compile and pass as doc tests (a3b9bfa)

### Task 2: Remove dead code in fits() and add rlsp-yaml crate docs

Remove the unreachable `Mode::Break` branches from `fits()` in
`printer.rs` — the function only operates in `Flat` mode, so the
`mode` parameter and break-mode handling are unnecessary. Add a
brief `//!` crate description to `rlsp-yaml/src/lib.rs`.

- [ ] Remove `mode` from `fits()` internal stack — always `Flat`
- [ ] Remove `Mode::Break` arm from `Doc::Line` match (lines 135-140)
- [ ] Remove `Mode::Break` arm from `Doc::FlatAlt` match (line 157)
- [ ] Add `//!` crate description to `rlsp-yaml/src/lib.rs`
- [ ] `cargo test`, `cargo clippy`, `cargo doc` all pass

## Decisions

- **Remove dead code rather than add tests:** The `Mode::Break`
  branches in `fits()` are structurally unreachable — no caller
  passes `Break` mode and nested groups always push `Flat`.
  Adding tests would require artificially exposing the internal
  function. Removing the dead code is simpler, improves clarity,
  and eliminates the coverage gap.
- **Doc examples over standalone tests:** Doc examples on public
  API serve double duty — they test the API and provide
  documentation on docs.rs. Preferred over adding more unit tests
  for already-covered functions.
