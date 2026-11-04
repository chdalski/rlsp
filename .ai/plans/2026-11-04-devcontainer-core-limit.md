**Repository:** root
**Status:** NotStarted
**Created:** 2026-11-04

# The Dev Container Dumps No Core

## Goal

Every crash of a process in the dev container runs the host's `systemd-coredump`, which on the
user's SELinux host logs a denial and stores a core file each time. No process in the dev
container reaches the host's core-dump handler anymore, whichever process crashes, and the
stack-cache canary's doc comment says that its core-dump check is blind inside the dev
container.

## Context

- The host kernel's `core_pattern` pipes every core dump to the host's `systemd-coredump`.
  `core_pattern` is not namespaced, so a crash of any process in the dev container runs the
  host's handler. Inside the container the core limit is unlimited today.

## Steps

- [x] Clarify the approach
- [x] Settle the design with the design-advisor
- [x] Task 1: The dev container limits core dumps to 1
- [x] Task 2: The canary's doc comment names the dev container's blind spot

## Tasks

### Task 1: The dev container limits core dumps to 1

The `sandbox` service gets a core limit of 1, so the kernel never starts the host's core-dump
handler for a crash in the container, and the dev container's README tells a developer why no
core file appears. `.devcontainer/` is a template meant to be copied into other projects.

- [x] The `sandbox` service in `.devcontainer/docker-compose.yml` sets the core limit to the
      fixed value 1; the value is not read from `.env`, and its comment is separate from the
      comment on the personal CPU and memory limits
- [x] That comment tells a reader why the container keeps every process from dumping core and
      why the value is 1 and not 0
- [x] `.devcontainer/README.md` has a `## Core Dumps` section from which a reader learns why no
      process in the container dumps core, how to confirm inside a rebuilt container that the
      limit is active, and that a program whose crash they want to debug from a core file has
      to run outside the dev container
- [x] The new comment and the new README section name nothing specific to this project
- [x] `.devcontainer/docker-compose.yml` parses as YAML and `services.sandbox.ulimits.core` is
      the integer 1; the handoff names the parser used
- [x] In this container, a process that runs under a soft and hard core limit of 1 and is
      killed by SIGSEGV is reported as not having dumped core; the handoff shows the run

### Task 2: The canary's doc comment names the dev container's blind spot

The stack-cache canary `small_stack_is_not_served_from_a_cached_2_mib_stack` in
`rlsp-yaml/src/schema.rs` asserts that its deliberately aborting child probe, which marks
itself non-dumpable, left no core dump. Its doc comment says the check cannot detect a missing
flag on a host where the kernel writes no dump anyway; with a core limit of 1, the dev
container is such a host, so a green run there proves nothing about the flag.

- [x] A reader of that doc comment learns that the core-dump check cannot detect a missing flag
      inside this repository's dev container, and that the container's core limit is the reason
- [x] The change is limited to that doc comment
- [x] `cargo fmt --check` and `cargo clippy --all-targets` pass

## Decisions

- **Core limit 1, not 0:** at 1 the kernel does not start a pipe `core_pattern` handler and
  logs a warning instead; at 0 it still starts the handler.
- **One container-wide limit, not per-process flags or a host SELinux module:** most crashes
  come from throwaway probe programs that never enter the repository, and a host module would
  change the host's policy outside the repository while still leaving a core file per crash on
  the host.
- **A fixed value in `docker-compose.yml`, not a `.env` setting:** the limit holds for every
  developer and nobody needs to change it; the `.env` limits are personal settings.
- **A `## Core Dumps` README section, not under "Resource limits" or "Troubleshooting":**
  "Resource limits" documents the personal `.env` settings and Troubleshooting holds failure
  recipes; the core limit is a standing behavior of the container.
- **The canary probe keeps its non-dumpable flag:** it keeps the probe away from the core-dump
  handler on CI and on hosts without this dev container.
- **The user confirms the effect after "Rebuild Container":** the container has no Docker CLI,
  and the rebuild ends the session that runs this plan, so no task criterion can observe it.
- **No push in this plan:** local `main` carries unpushed work that is not ready to land, and
  these commits go out with it; the compose change takes effect from the local checkout.
