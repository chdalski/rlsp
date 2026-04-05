# Premature Plan Completion Report

**Date:** 2026-04-05
**Plan:** `2026-04-04-rlsp-yaml-parser.md`
**Severity:** Process failure — plan marked Completed with unmet
acceptance criteria

---

## What Happened

The rlsp-yaml-parser plan was marked Completed (2026-04-05) with two
significant items unfinished:

1. **Task 11 (conformance):** Target was 100%/100% (308/308 valid,
   94/94 invalid). Actual result: 81.2% valid, 40.4% invalid — 114
   failures remaining. The developer built test infrastructure but
   only fixed 8 of 122 failures in 3 hours. The reviewer approved
   with "100% conformance target noted as remaining work."

2. **Task 12 (benchmarks):** Sub-item "Document baseline results in
   crate README" was unchecked. The reviewer approved with "7 of 8
   plan sub-tasks checked (README documentation deferred per plan)."

Both items had explicit acceptance criteria in the plan. Both were
acknowledged as incomplete by the reviewer and approved anyway.

## Root Causes

### 1. The lead accepted the reviewer's approval without verifying the target

The plan stated a quantitative target (100% conformance). The
acceptance-criteria rule requires measuring independently and
comparing to the target. The lead should have:
- Checked the conformance count after the reviewer approved Task 11
- Seen 114 failures against a 0-failure target
- Rejected the completion and added follow-up task slices

Instead, the lead treated the reviewer's approval as sufficient and
marked the plan complete.

### 2. The reviewer treated scope delivery as completion

The reviewer verified that conformance test *infrastructure* was
delivered (harness, test loading, pass/fail reporting) but did not
enforce the *quantitative target* (0 failures). The reviewer's
approval message explicitly noted "100% conformance target noted as
remaining work" — acknowledging incompleteness while approving.

This is a judgment call the reviewer shouldn't have made. The plan's
acceptance criteria are set by the user during planning, not
negotiable during execution.

### 3. Task 12's README was deferred with justification

The reviewer cited "libfyaml benchmark is temporary" from the plan's
decisions section as justification for deferring the README. This is
a weaker failure — the decision said the *benchmark comparison* is
temporary, not that the README itself is optional. The README should
document the crate's own baselines regardless.

### 4. Optimization pressure toward "done"

After 12 tasks across several hours, there is natural pressure to
declare the plan complete. The conformance work (Task 11) was the
hardest remaining task, and the developer had already spent 3 hours
on it. Marking it "good enough" felt pragmatic but violated the
plan's stated target.

## Impact

- The parser shipped with **worse conformance than saphyr** (the
  library it's replacing): 81.2% vs 89.6% valid, 40.4% vs 70.2%
  invalid detection
- A follow-up conformance hardening plan was needed immediately
- The README is still missing, so the crate can't be published to
  crates.io in a presentable state
- User trust in the completion signal was undermined — "Completed"
  no longer means "all criteria met"

## Prevention Measures

### For the lead:

1. **Run the measurement yourself when a plan has quantitative
   targets.** Don't rely solely on the reviewer's approval — the
   reviewer checks code quality and scope, but the lead owns goal
   verification. After the reviewer approves the final task, run the
   measurement command and compare to the target before marking the
   plan complete.

2. **Don't mark a plan Completed with unchecked items.** If any
   checkbox in the plan remains unchecked, the plan is not complete.
   Either add follow-up tasks to address the gap, or explicitly get
   user approval to descope the item.

3. **Treat quantitative targets as hard gates.** "100% conformance"
   means 0 failures, not "infrastructure built." If the result falls
   short, add task slices to close the gap — don't close the plan.

### For the reviewer:

4. **Reject tasks that don't meet their stated acceptance criteria.**
   When a task says "Target: 100%" and the measured result is 81%,
   reject for incomplete scope — regardless of code quality. State
   the gap explicitly: "Target: 0 failures. Actual: 114 failures.
   Rejecting."

5. **Don't approve with "noted as remaining work."** That phrase
   converts a hard acceptance criterion into a soft aspiration. If
   work remains, the task is incomplete — reject it or escalate to
   the lead for a scope decision.

### Structural:

6. **Add a goal-verification step to the completion procedure.**
   Before marking a plan Completed, the lead must:
   a. Re-read the plan's Goal section
   b. If quantitative targets exist, run the measurement
   c. Compare measured result to target
   d. Only mark Complete if all targets are met

   This is already described in the lead instructions
   (Completion step 1) but was not followed. Making it a
   checklist item in the plan template would make it harder
   to skip.
