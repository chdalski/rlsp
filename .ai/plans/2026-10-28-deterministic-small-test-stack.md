**Repository:** root
**Status:** Completed (2026-10-28)
**Created:** 2026-10-28

# Deterministic Small Test Stack

## Goal

The test helper `on_a_small_stack` requests a 512 KiB thread so its
tests prove that code under test does not grow the call stack with
schema, document, or reference-chain depth, but glibc can hand that
thread a finished libtest thread's cached 2 MiB stack, so these tests
pass or fail by scheduling. Give
the helper a size glibc cannot replace with a larger cached stack,
prove with a test that the stack is really that small, name the
small-stack thread after its test, and fix the cause of every test
that overflows once the stack is truly small.

## Context

- `rlsp-yaml/src/schema.rs`, `test_support::on_a_small_stack`: spawns
  `std::thread::Builder::new().stack_size(512 * 1024)` and joins it.
  Its tests are in `src/schema.rs`, `src/schema/path.rs`,
  `src/schema_validation/reference_tests.rs`, and
  `src/completion/reference_tests.rs`.
- glibc reuses a cached thread stack when the cached block, guard
  page included, is at least the requested size plus guard and at
  most four times that. libtest runs every test on a 2 MiB thread
  (`RUST_MIN_STACK` default), so a request is safe from reuse only
  below 512 KiB − ¾ × guard: below 509 KiB with x86_64's 4 KiB guard,
  below 464 KiB with aarch64 Linux's 64 KiB guard.
- A stack overflow aborts the whole test binary with
  `thread '<name>' … has overflowed its stack`; it cannot be caught
  in-process.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: the small test stack is truly small and proven so
- [x] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Make the small test stack truly small and prove it

- [x] `on_a_small_stack` requests a stack of exactly 448 KiB, and its
      doc comment tells a maintainer why the size is 448 KiB and not
      512 KiB, which `RUST_MIN_STACK` values would defeat it, and what
      happens on an overflow
- [x] The helper's failure message no longer claims to catch a stack
      overflow
- [x] The small-stack thread carries the name of the test thread that
      calls the helper, so an overflow message names the test
- [x] A canary test, compiled only for Linux with glibc, runs its own
      test binary as a child process that first spawns and joins a
      2 MiB thread and then recurses about 1 MiB deep through
      `on_a_small_stack`; it passes only when the child fails and its
      stderr contains `has overflowed its stack`; the handoff shows
      the canary failing with the helper at 512 KiB and passing at
      448 KiB
- [x] All tests pass at 448 KiB, each of these run twice:
      `cargo test -p rlsp-yaml --lib`, the same with
      `--test-threads=1`, and
      `cargo llvm-cov --no-report -p rlsp-yaml --lib` with
      `--test-threads=64` and with `--test-threads=1`
- [x] Every test that overflows at 448 KiB keeps its depth and its
      assertions, the handoff names its cause, and the cause is
      removed in the code under test, or in the test's own setup when
      the overflow lies in setup code and not in what the test pins;
      for a setup-side fix the handoff names the setup code that
      recursed
- [x] Every other existing test passes with its assertions unchanged
- [x] `cargo llvm-cov --workspace --lcov --output-path lcov.info`
      passes locally on the pinned toolchain
- [x] Build, tests, clippy, and fmt pass

## Non-Goals

- Reading nested collections in `rlsp-yaml-parser`'s loader without
  recursion
- The 2 MiB thread in `every_subschema_keyword_parses_to_the_json_depth_limit`
  (no test spawns a cached stack between 2 and 8 MiB) and the two
  64 MiB threads in `src/parser.rs` (intentionally large for the
  recursive loader)

## Decisions

- **448 KiB:** it stays below the reuse bound for any guard up to
  85 KiB and is a multiple of 64 KiB, so neither macOS arm64 (16 KiB
  pages) nor Windows (64 KiB allocation granularity) rounds it.
  Declined: 511 KiB (reused with a 4 KiB guard) and 480 KiB (reused
  with aarch64's 64 KiB guard).
- **A re-exec canary proves the size:** a stack overflow cannot be
  caught in-process, and reading stack bounds needs `unsafe` or a new
  dependency; the canary seeds a free 2 MiB stack deterministically,
  since a joined thread's stack is back in glibc's cache. Declined: a
  `stacker` dev-dependency that checks every small-stack body (a new
  dependency wrapping `unsafe` FFI) and `GLIBC_TUNABLES` to disable
  the stack cache (depends on how the tests are launched).
- **The size stays a literal in the helper:** a named constant would
  repeat what the doc comment states; the canary checks it.
- **A cause in `rlsp-yaml-parser` goes to the user:** the parser is a
  separately published crate with its own change rules; the user
  decides before any change there.
