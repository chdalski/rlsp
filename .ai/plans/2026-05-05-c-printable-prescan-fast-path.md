**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-05

## Goal

Eliminate redundant per-scanner c-printable/nb-json validation passes on valid YAML input by adding a single pre-scan flag to `Lexer`. When the input contains no non-printable bytes (the common case for all valid YAML), all 11 per-scanner `find_non_c_printable` / `find_non_nb_json` calls are skipped — replacing N per-scalar O(m) scans with one O(n) pre-scan at construction time.

Expected user-measurable outcome: recovery of the 10-12% throughput regression on `scalar_heavy` and `block_sequence` benchmarks introduced by commit `666e2f2` (c-printable enforcement). Benchmark verification is the user's responsibility on baremetal.

## Context

- Commit `666e2f2` added YAML §5.1 character-set enforcement across all 4 scanner modules. Each scanner now calls `find_non_c_printable()` or `find_non_nb_json()` on every scalar/comment slice it processes. On valid YAML (which by definition contains no non-printables), every call scans the slice and finds nothing — pure overhead proportional to total content length.
- Baremetal benchmarks show `scalar_heavy` dropped from 256.61 → 224.68 MiB/s (-12.4%) and `block_sequence` from 268.75 → 241.31 MiB/s (-10.2%) relative to the documented 2026-04-27 baseline. These regressions exceed the ~5-6% system-level noise (libfyaml regressed ~6% on the same run).
- The `.ai/memory/potential-performance-optimizations.md` file documents this optimization as "c-printable enforcement pre-scan fast-path" with a note "when to pursue: after conformance work is complete." The conformance enforcement commits have all landed.
- The `Lexer` struct (`lexer.rs:36`) holds all scanner state. Scanner methods (`try_consume_plain_scalar`, `try_consume_single_quoted`, `try_consume_double_quoted`, block scalar consumption, comment validation) are all `impl Lexer` methods with `&mut self` access.
- One exception: `scan_double_quoted_line` (`quoted.rs:737`) is a `pub(super)` free function — it doesn't have `self`. Its 2 `find_non_nb_json` calls need the flag passed as a parameter.
- Call sites: 3 in `plain.rs`, 2 in `block.rs`, 1 in `comment.rs`, 5 in `quoted.rs` (3 in methods, 2 in the free function). Total: 11 call sites to guard.

### Specifications

- YAML 1.2.2 §5.1: c-printable character set definition
- `find_non_c_printable` in `chars.rs:76`: byte-level scanner that detects C0 controls (except TAB), DEL, C1 controls (except NEL), and U+FFFE/FFFF
- `find_non_nb_json` in `chars.rs:140`: byte-level scanner that detects only C0 controls (except TAB) — the JSON-compatibility subset

## Steps

- [ ] Add `input_all_printable: bool` field to `Lexer` and set it in `Lexer::new()`
- [ ] Guard all 11 `find_non_c_printable` / `find_non_nb_json` call sites with the flag
- [ ] Pass the flag to `scan_double_quoted_line` as a parameter
- [ ] Verify all existing tests pass (correctness preserved)
- [ ] Update `.ai/memory/potential-performance-optimizations.md` to reflect applied state

## Tasks

### Task 1: Add pre-scan flag and guard all validation call sites

Add an `input_all_printable: bool` field to the `Lexer` struct that is computed once in `Lexer::new()` by calling `find_non_c_printable(input.as_bytes()).is_none()`. Then guard every per-scanner validation call site with `if !self.input_all_printable { ... }` (or pass the flag as a parameter for the free function case).

**Files:**
- `rlsp-yaml-parser/src/lexer.rs` — add field to struct (line 36) and initialize in `Lexer::new()` (line 83)
- `rlsp-yaml-parser/src/lexer/plain.rs` — guard 3 `find_non_c_printable` calls (lines 98, 131, 236)
- `rlsp-yaml-parser/src/lexer/block.rs` — guard 2 `find_non_c_printable` calls (lines 236, 465)
- `rlsp-yaml-parser/src/lexer/comment.rs` — guard 1 `find_non_c_printable` call (line 64)
- `rlsp-yaml-parser/src/lexer/quoted.rs` — guard 3 `find_non_nb_json` calls in methods (lines 77, 101, 190); add `skip_char_validation: bool` parameter to `scan_double_quoted_line` (line 737) and guard its 2 `find_non_nb_json` calls (lines 761, 846); update all callers of `scan_double_quoted_line` to pass `self.input_all_printable`

**Rationale for single flag:** c-printable is strictly more restrictive than nb-json. If `find_non_c_printable` returns `None` on the full input, `find_non_nb_json` will also return `None` on any substring. One flag covers both check types.

**Acceptance criteria:**
- `cargo test -p rlsp-yaml-parser` passes (all existing tests including the 44 char_validation integration tests)
- `cargo clippy --all-targets` produces zero warnings
- The flag is `false` for inputs containing non-printable characters (existing tests exercise this path)
- The flag is `true` for the benchmark fixtures (all valid YAML)
- `.ai/memory/potential-performance-optimizations.md` updated: "c-printable enforcement pre-scan fast-path" moved from "Still deferred" to "Applied candidates" with commit SHA and approach description (single `input_all_printable` flag, 11 guarded call sites)

## Non-Goals

- SIMD-accelerated pre-scan — the existing `find_non_c_printable` byte loop is sufficient; SIMD is a separate future optimization if the pre-scan itself shows up in profiles
- Separate `input_is_nb_json` flag — the two-flag variant provides marginal benefit only for YAML containing DEL/C1 characters (extremely rare in practice)
- Updating `benchmarks.md` — performance verification is the user's responsibility on baremetal; agents run in Docker where perf numbers are unreliable

## Decisions

- **Single flag, not two:** c-printable ⊂ nb-json in rejection criteria, so one pre-scan covers both. Simpler, and the edge case (YAML with C1/DEL where only quoted scalars appear) is too narrow to optimize for.
- **Pre-scan in `Lexer::new()`, not `parse_events()` or `load()`:** The Lexer is the natural owner — it already holds the input reference and all scanner state. Both `parse_events` and `load` go through `Lexer::new()`.
- **Guard pattern, not early-return refactor:** Each call site gets a simple `if !self.input_all_printable` wrapper around the existing validation block. This preserves the validation logic unchanged for the `false` path and makes the optimization trivially reversible.
- **Parameter on free function, not refactoring to method:** `scan_double_quoted_line` is a complex function (100+ lines) that's called from `try_consume_double_quoted`. Adding a bool parameter is a 1-line signature change; converting to a method would be a larger refactor with no benefit.
