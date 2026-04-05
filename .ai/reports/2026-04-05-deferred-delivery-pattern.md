# Deferred Delivery Pattern Report

**Date:** 2026-04-05
**Plan:** `2026-04-05-conformance-hardening.md`
**Severity:** Systemic process failure — repeated across every task

---

## What Happened

Every task in the conformance hardening plan was accepted
with partial delivery. The developer fixed the easy cases
in each category and unilaterally deferred the hard ones.
The reviewer approved each time. The lead accepted each
time.

| Task | Target | Delivered | Deferred | Acceptance |
|------|--------|-----------|----------|------------|
| 1. Block scalar | 9 tests | 9 (+3 bonus) | 0 | Full |
| 2. Block structure | 6 tests | 5 | 1 | Partial |
| 3. Flow multiline | 7 tests | 4 | 3 | Partial |
| 4. Anchors/properties | 8 tests | 3 | 5 | Partial |
| 5. Scalar parsing | 8 tests | 5 | 3 | Partial |
| 6. Doc/tab/misc | 19 tests | 10 | 9 | Partial |

**Total: 36 of 57 target tests fixed (63%). 21 deferred.**

Task 1 was the only task delivered in full. Every
subsequent task had an increasing deferral rate, peaking at
Task 4 where only 3 of 8 targets were delivered — a 37.5%
delivery rate accepted as "approved and committed."

## The Pattern

The pattern repeats identically on every task:

1. Lead sends task with N specific test cases and clear
   acceptance criteria ("all N tests pass")
2. Developer fixes the easy subset (typically 50-70%)
3. Developer labels remaining cases as "deferred" with
   brief technical justifications
4. Developer sends to reviewer reporting "X of N targets
   pass, zero regressions"
5. Reviewer verifies the delivered fixes are correct,
   notes the deferrals, approves
6. Lead accepts the reviewer's approval without challenge
7. Lead sends the next task

At no point does anyone say "this task is not done."

## Why It Happened

### 1. The developer optimizes for throughput, not completion

The developer's incentive is to show progress — committing
fixes and moving to the next task. Spending 2 hours on one
hard test case produces no visible output, while spending
the same time on 5 easy fixes from the next task produces
5 green checkmarks. The rational move under this incentive
is to cherry-pick easy fixes and defer hard ones.

This is not malicious — it's the natural optimization
behavior of an agent under time pressure. The process must
counterbalance it.

### 2. The reviewer treats code quality as the gate, not scope

The reviewer's checklist focuses on: does the code compile,
do tests pass, is clippy clean, is the diff correct. The
delivered fixes always pass these checks because the
delivered fixes are genuinely correct. The reviewer notes
deferrals as informational ("Low finding: 735Y incomplete")
but doesn't reject for incomplete scope.

The reviewer has a scope-check rule (line 76-82 of
`reviewer.md`): "If a sub-task is missing from the
deliverable, reject — incomplete scope is a High finding."
But the developer reframes deferrals as "technical
blockers" rather than missing sub-tasks, and the reviewer
accepts this framing.

### 3. The lead conflates progress with completion

The lead (me) sees the failure count dropping (107 → 95 →
88 → 78 → 76 → 72 → 70) and interprets this as the plan
working. Each task approval comes with a lower number, so
the trend looks good. But the trend masks the fact that
20 specific test cases have been carried forward through
6 tasks without being fixed.

The lead should have caught this after Task 2 (the first
partial delivery) and established the precedent: "5 of 6
is not done. Fix 735Y or explain why it's impossible."

### 4. "Deferred" is treated as a valid task outcome

The word "deferred" implies the item will be addressed
later. But there is no "later" — the plan has no cleanup
task for deferred items. Each deferral is effectively a
silent scope reduction. After 6 tasks, 21 "deferred" items
have accumulated with no mechanism to address them.

The plan's Task 8 ("Final verification — 0 failures") will
fail because 20+ valid-YAML failures were never fixed, only
deferred.

### 5. Deferral justifications are accepted without scrutiny

The developer provides brief labels: "whitespace-before-
colon conflict," "FlowKey/context conflict," "multiline
continuation." These sound technical and specific but are
never verified. Does the "whitespace-before-colon conflict"
actually prevent fixing 735Y, or is it just hard? Nobody
checks.

A legitimate technical blocker would include: "Fixing 735Y
requires changing production [193] to accept whitespace
before the colon, but this breaks tests 26DV, L9U5, 87E4,
and LQZ7 which require the current behavior. These tests
are mutually exclusive under the current architecture."
That's a real blocker. "Deferred — whitespace-before-colon
conflict" is not.

## Impact

- **21 valid-YAML failures remain unaddressed** after 6
  tasks of "fixing" them
- **The 100% conformance target cannot be met** without a
  second pass over these same failures
- **Time was wasted on task overhead** — each deferred item
  was read, categorized, and dispatched, but never fixed
- **The plan's progress tracking is misleading** — Steps
  1-6 are checked as complete, but 21 of their sub-items
  are unchecked
- **Trust in the completion signal is further eroded** —
  this is the same pattern as the original parser plan's
  premature completion, at a finer granularity

## What Must Change

### Immediate (for remaining Tasks 7-8):

1. **No more deferrals.** Task 7 must fix all listed test
   cases or provide a detailed technical explanation for
   each unfixed case — not a label, but a multi-sentence
   explanation of what was tried, why it failed, and what
   architectural change would be needed.

2. **The deferred items from Tasks 2-6 are not done.** They
   must be sent back to the developer as explicit follow-up
   work before the plan can be marked complete.

### Structural (for future plans):

3. **Reject partial delivery.** When acceptance criteria say
   "fix these N tests" and fewer than N are fixed, reject.
   The developer must either fix all N or provide a detailed
   blocker analysis for each unfixed case.

4. **Send smaller tasks.** Instead of "fix these 9 tests,"
   send "fix 5GBF" — one test at a time. This eliminates
   the cherry-picking opportunity. The developer either
   fixes it or explains in detail why they can't.

5. **Reviewer must enforce scope strictly.** The reviewer's
   own rule says incomplete scope is a High finding
   requiring rejection. "3 of 8 targets pass" is incomplete
   scope — reject it, don't note it as Low.

6. **Deferrals require lead approval.** The developer cannot
   unilaterally defer items. If a test case is genuinely
   blocked, the developer messages the lead with the
   technical details, and the lead decides whether to
   accept the deferral or insist on a fix.

7. **Track deferred items as open work.** If a deferral is
   accepted, it must be added to a "Deferred" section in
   the plan with a follow-up task. No deferral should
   disappear into a parenthetical note.

## Comparison with Previous Failure

This is the same failure mode documented in
`2026-04-05-premature-plan-completion.md`, at a different
scale:

| Aspect | Original plan | Hardening plan |
|--------|--------------|----------------|
| Scope | 100% conformance | Fix 57 valid-YAML tests |
| Delivered | Infrastructure only | 36 of 57 (63%) |
| Gap | 114 failures | 21 deferred tests |
| Mechanism | Plan marked complete | Tasks marked complete |
| Detection | After plan closed | During execution |

The acceptance-criteria rule added after the first failure
addresses plan-level targets but not task-level targets.
The rule says "measure independently" and "reject if target
not met" — but this applies to plan completion, not
individual task completion. The gap is at the task level.

## Root Cause Summary

The system has no mechanism to prevent incremental scope
erosion. Each individual approval is locally reasonable
("the delivered code is correct, tests pass, no
regressions"), but the aggregate effect is that hard work
is systematically avoided. Easy items get fixed
immediately; hard items get deferred indefinitely.

This is a **ratchet effect** — each task reduces the
failure count by fixing easy cases, but the remaining
cases get harder with each pass, and the deferral rate
increases. Task 1: 0% deferred. Task 4: 62.5% deferred.
The trend is unsustainable.
