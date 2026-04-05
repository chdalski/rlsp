# Claim Verification

When an agent reports that work is infeasible, blocked, or
requires major architectural changes, verify the claim
before accepting it. Unverified technical claims are the
most common source of premature plan closure in this
blueprint — four documented instances show the same
pattern: work framed as impossible turns out to be a
moderate change in the project's own codebase.

## The Pattern

The implementor optimizes for scope reduction — fewer
things to implement means faster completion. This creates
a consistent bias: remaining work is framed as harder than
it is. Category labels like "needs X enhancement" or
"requires architectural changes" inflate perceived effort
without describing the actual work. These labels are
unfalsifiable without reading the code, so they pass
through review unchallenged.

## The Rule

### Require specifics, not categories

When reporting work as infeasible or requiring major
changes, the report must include:

- **Which file and function** would need to change
- **Whether the change is in the project's own codebase**
  or an external dependency — these are fundamentally
  different. A change in our own code is work to be
  scoped; a change in a dependency may be a genuine
  blocker.
- **Estimated scope** — approximate line count or
  complexity
- **What specifically is missing or insufficient** — not
  "the parser doesn't support X" but "the parser emits
  events A and B but the loader in `file.rs:fn_name()`
  doesn't consume event B"

A category label ("needs parser enhancements") without
these specifics is not sufficient justification for
closing or deferring work.

### Verify against the codebase

When you receive a claim that work is infeasible, read
the relevant source before accepting it. Check:

- Does the infrastructure the implementor says is missing
  actually not exist? Often it does — the implementor
  looked in the wrong place or at the wrong abstraction
  level.
- Is the change in an external dependency, or in the
  project's own code? The implementor may frame an
  internal change as if it requires upstream work.
- Is the scope as large as claimed? A "parser enhancement"
  might be a 10-line loader fix.

This verification takes minutes. Skipping it has
repeatedly allowed moderate tasks to be closed as
impossible.

### Distinguish N/A from blocked

- **Not applicable (N/A)** — the item does not apply to
  this project. Close it.
- **Blocked** — the item applies but cannot be completed
  now. Keep it open with a description of what unblocks
  it.

Do not use N/A for blocked items — N/A closes the item
permanently, while blocked preserves it for future work.
A plan with blocked items is not Completed; it stays
InProgress or gets follow-up tasks that address the
blocker.

## Why This Matters

Each instance of unverified infeasibility compounds:
the plan closes, the work disappears from the queue,
and the underlying issue remains. Future sessions see a
Completed plan and move on — they don't know the work
was deferred. The gap is only discovered when a user
hits the missing functionality and asks why it wasn't
done.
