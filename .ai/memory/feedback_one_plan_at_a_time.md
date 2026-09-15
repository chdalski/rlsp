---
name: feedback-one-plan-at-a-time
description: "User wants to approve and execute one plan at a time, never batch-present or batch-approve multi-plan programs"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 69991bad-bc0f-411d-972c-0fdb9ad7151c
---

When a single user request decomposes into multiple plan
files, do not batch-present them all for approval up front.
Approve and execute one plan at a time: present plan N,
get approval, run the developer-reviewer pipeline to plan
N's completion, then move on to plan N+1.

**Why:** the user said this directly during a six-plan
file-split program — after plan 1 was approved and
committed, they declined the offer to also approve plan 2
in the same session and said "We'll work through one plan
at a time and i'll approve one after the next. We can work
on the lsp lifecycle now. We'll do the rest tomorrow." So
the batch-of-plans framing I was using (draft all six,
review all six, present all six) is wrong for the approval
and execution phases — only drafting and reviewing can
batch.

**How to apply:**

- It is fine to draft and plan-review multiple plans in the
  same session if the user asks for them together — that
  saves time on the planning side.
- Stop at the presentation step. Present one plan, ask for
  approval via `AskUserQuestion`, execute through the
  developer-reviewer loop to completion, then present the
  next.
- If the user defers later plans to another session (as
  here), leave them in the plans directory with
  `Status: NotStarted` and stop. Don't ask whether they
  want to approve more — wait for them to bring it up next
  session.
- See [[feedback-plan-presentation]] for the
  show-full-content rule that still applies when
  presenting any single plan.
