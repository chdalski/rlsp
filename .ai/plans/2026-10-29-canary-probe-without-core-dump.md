**Repository:** root
**Status:** NotStarted
**Created:** 2026-10-29

# Stop the stack-cache canary's child probe from dumping core

## Goal

Every `cargo test` of `rlsp-yaml` on Linux with glibc aborts one child
process on purpose — the stack-cache canary's probe — and the kernel hands
each abort to the host's core-dump handler, which leaves a core dump and, on
SELinux hosts, an audit denial per test run. The deliberate abort must stop
producing a core dump, the canary must keep verifying everything it verifies
today, and the canary must fail if the probe dumps core again.

## Context

- The canary is `test_support::tests::stack_cache_canary` in
  `rlsp-yaml/src/schema.rs`, compiled only under
  `cfg(all(target_os = "linux", target_env = "gnu"))`. Its test
  `small_stack_is_not_served_from_a_cached_2_mib_stack` re-runs the test
  binary filtered to `stack_cache_canary_child_probe` with
  `RLSP_YAML_STACK_CACHE_CANARY_PROBE` set. The probe overflows the stack of
  `on_a_small_stack`'s thread, and Rust's overflow handler aborts the child
  with SIGABRT. The canary asserts that the filter matched, that the child
  died by signal 6, and that stderr carries the overflow line naming the
  probe's thread.
- This dev container's `core_pattern` pipes to a core-dump handler.
- A process marked non-dumpable (`PR_SET_DUMPABLE` = 0) gets no core dump and
  no handler invocation. `execve` resets the flag, so only the child process
  itself can set it.
- rustix exposes the flag safely as
  `rustix::process::set_dumpable_behavior(DumpableBehavior::NotDumpable)`
  behind its `process` feature. rustix 1.1.5 is already in `Cargo.lock` as a
  transitive dev-dependency through `tempfile`.
- `std::os::unix::process::ExitStatusExt::core_dumped()` reports whether the
  kernel wrote a core dump for the exited child.

## Steps

- [x] Clarify the approach
- [x] Settle the design with the design-advisor
- [ ] Task 1: make the canary's child probe non-dumpable and require it

## Tasks

### Task 1: Make the canary's child probe non-dumpable

The probe marks its own process non-dumpable before it overflows, so the
deliberate abort leaves no core dump, and the canary additionally requires
that no core was dumped, so losing the call fails the test.

- [ ] With `RLSP_YAML_STACK_CACHE_CANARY_PROBE` set,
      `stack_cache_canary_child_probe` marks its process non-dumpable through
      `rustix::process::set_dumpable_behavior` before any other work on the
      probe path; without the variable it still returns at once
- [ ] `small_stack_is_not_served_from_a_cached_2_mib_stack` asserts, after its
      signal assertion and with its own failure message that carries the
      child's report, that the child did not dump core; its existing
      assertions are unchanged
- [ ] `rustix` with feature `process` is a dev-dependency of `rlsp-yaml` only
      under `cfg(all(target_os = "linux", target_env = "gnu"))`, with a
      one-line manifest comment naming the canary as its user; `Cargo.lock`
      gains no new package
- [ ] A reader of the probe's doc comment learns why the probe disables core
      dumps and why the child, not the canary, has to do it; a reader of the
      canary's doc comment learns that it requires the overflow without a core
      dump
- [ ] In this dev container, the canary fails on its core-dump assertion when
      the probe does not set the flag, shown by a run reported in the handoff;
      the committed change sets the flag
- [ ] Build, `cargo clippy --all-targets` and the full `rlsp-yaml` test suite
      pass

## Decisions

- **The child sets the flag, after its environment gate:** setting it in the
  parent would stop genuine crashes of the whole test binary from dumping
  core, and handing it to the child through `CommandExt::pre_exec` is
  `unsafe`.
- **rustix, not `libc::prctl`:** `libc::prctl` would bring the first `unsafe`
  block into the repository.
- **A failed call fails the canary:** if marking the process non-dumpable
  fails, the probe panics and the canary fails on its signal assertion. A
  silent fallback would bring the core dumps back unnoticed.
- **Not `RLIMIT_CORE = 1`, `ulimit -c 0`, or a host SELinux `dontaudit`
  module:** the rlimit makes the kernel log a warning per abort; `ulimit -c 0`
  does not stop a pipe `core_pattern` handler from running; a host-side module
  hides the denial for every process and lives outside this repository.
- **The dependency's gate matches the canary's cfg:** the dependency exists
  exactly where its only user compiles.
