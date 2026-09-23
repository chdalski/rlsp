---
name: no-pausing-between-tasks
description: "Once a plan is approved, run it end-to-end without stopping to report or wait for the user between tasks"
metadata:
  node_type: memory
  type: feedback
  originSessionId: acd45279-f5b8-401b-b238-387855cfd3c6
  modified: 2026-09-23T13:30:48.789Z
---

After the user approves a plan, execute every task, the push, CI verification, and plan closure continuously. Do not end turns with per-task status summaries that read as waiting for the user, and do not treat task boundaries as checkpoints.

**Why:** during the Rust 1.98.1 adoption (2026-09-23) the user had to ask "all done?" twice and then said "Stop using tasks as gate to pause the work..." — per-task stops made it look like work had stalled.

**How to apply:** approval covers the whole plan including its push/CI steps. Only surface to the user for genuine blockers (failed gate needing a user-owned decision, unresolvable rejection loop) or at plan completion with one final report. Keep intermediate turn-ending text to a single line or none. Related: [[wait-for-reviewer-direct-approval]], [[one-plan-at-a-time]].
