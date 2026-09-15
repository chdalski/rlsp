---
name: No agent-verifiable perf thresholds
description: Don't put hard throughput/latency numbers in agent acceptance criteria when agents run in a different environment than the perf target
type: feedback
originSessionId: 00c09f36-ddb7-414d-95d8-24766bd6d134
---
When a plan involves performance work, do NOT include hard
throughput, latency, or memory-allocation numbers as
acceptance criteria the agent must hit. The agent runs in
Docker; the perf target is baremetal. Numbers measured in
Docker do not match baremetal numbers, and forcing the
agent to hit a baremetal-derived number in Docker either
fails the task for environmental reasons unrelated to the
code, or trains agents to ignore the criterion when
measurements drift.

**Why:** the user explicitly called this out when reviewing
the 2026-04-26 parser perf-recovery plan: "throughput goals
make no sense because of the docker environment anyway and
because they're hard numbers that might not be reachable as
well — what should the agents do if they just can't reach
that numbers for some reason... Let's make the changes to
the parser and i'll run baremetal benchmarks afterwards."

**How to apply:**

- Per-task acceptance for perf-related plans: build passes,
  clippy clean, tests pass, code change is structurally
  what the plan describes. These are agent-verifiable.
- Skip "throughput X ≥ N MiB/s" and "latency ≤ M ns"
  acceptance criteria entirely when the agent cannot
  reproduce the user's measurement environment.
- Plan-level perf verification is the user's responsibility
  out-of-band on baremetal. Reflect this in the Goal and
  Decisions sections — the plan delivers the change; the
  user verifies the recovery.
- If the plan needs a measurement step at all, frame it as
  "report whatever the in-Docker bench produced for the
  user's reference" rather than as a pass/fail gate.
- Same logic applies to any other env-sensitive metric
  (e.g., GPU benchmarks when the agent has no GPU, network
  latency tests, sound/video timings).
