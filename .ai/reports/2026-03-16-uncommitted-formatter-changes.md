# Uncommitted Formatter Changes — Reviewer Staging Gap

**Date:** 2026-03-16
**Context:** duplicate-key-detection team, Task 1
**Commit:** `551fc7b` — `feat(validators): add duplicate key detection validator`

## Problem Statement

After the reviewer approved and committed the duplicate key
detection work, `git status` revealed uncommitted changes in
`rlsp-yaml/src/completion.rs`. These were pure formatting
changes (line-wrapping of long expressions) produced by
`cargo fmt` during the developer's build cycle. The reviewer
committed only the task-scoped files (`validators.rs`,
`server.rs`) and missed the incidental formatting diff in an
unrelated file.

## Impact

- **Dirty working tree after a "clean" commit** — the user
  discovered unexpected modifications that should not exist
  after a reviewed-and-committed task.
- **Erosion of trust** — if the review process claims "clean
  build, zero warnings" but leaves uncommitted changes, the
  quality gate feels unreliable.
- **Potential cascading diffs** — uncommitted formatting
  changes accumulate and pollute future diffs, making
  subsequent reviews harder to read.

## Root Cause

The reviewer staged files by name (`git add validators.rs
server.rs`) rather than checking `git status` for all
modified files before committing. The task description listed
two files to modify, so the reviewer scoped the commit to
those two files without verifying whether the build process
(specifically `cargo fmt`) had touched anything else.

This is a **selective staging error** — the reviewer verified
correctness of the code but not completeness of the commit.

## Contributing Factors

1. **Task description listed explicit files.** The lead's
   task message said "Files to modify: `validators.rs`,
   `server.rs`". This framing encouraged the reviewer to
   scope the commit strictly to those files rather than
   checking for all changes.

2. **`cargo fmt` has global reach.** Running `cargo fmt`
   reformats the entire crate, not just modified files. Any
   pre-existing formatting drift in other files gets swept
   into the working tree.

3. **No post-commit verification step.** Neither the
   developer nor the reviewer ran `git status` after the
   commit to confirm a clean working tree.

## Mitigation Strategies

### Strategy 1: Post-commit `git status` check (Recommended)

Add an explicit step to the reviewer's workflow: after
committing, run `git status` and verify the working tree is
clean. If unexpected changes remain, either include them in
the commit (if they're formatter/linter artifacts) or flag
them to the lead.

**Pros:** Simple, catches all cases, no tooling changes.
**Cons:** Relies on the reviewer remembering the step.

### Strategy 2: Pre-commit `git diff --stat` in reviewer instructions

Before staging, the reviewer should run `git diff --stat` to
see all modified files — not just the ones listed in the
task. This surfaces incidental changes before the commit
rather than after.

**Pros:** Catches the issue earlier in the flow.
**Cons:** Same reliance on procedural discipline.

### Strategy 3: Reviewer stages with `git add -A` scoped to crate

Instead of adding files by name, the reviewer could stage
all changes within the relevant crate directory:
`git add rlsp-yaml/`. This captures formatter artifacts
automatically.

**Pros:** Eliminates selective staging errors within the
crate.
**Cons:** Risk of staging unrelated work-in-progress if
the working tree isn't clean at task start. Should be
combined with a clean-tree precondition check.

### Recommended Combination

Apply Strategy 1 (post-commit `git status`) as a mandatory
reviewer step, and Strategy 2 (pre-commit `git diff --stat`)
as a best practice. Together they provide defense in depth —
the pre-commit check catches most issues, and the post-commit
check catches anything that slips through.
