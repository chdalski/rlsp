---
name: feedback-wait-for-reviewer-direct-approval
description: "Lead must wait for the reviewer's direct approval message before committing; developer-relayed approvals are informational only and do not satisfy the post-approval trigger"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 69991bad-bc0f-411d-972c-0fdb9ad7151c
---

In the developer-reviewer-lead pipeline, the trigger for
the lead's post-approval squash-and-commit sequence is the
**reviewer's direct message to the lead**, not a developer
relay describing what the reviewer said.

**Why:** During the validators split execution, the
developer's idle notification included a relayed claim of
reviewer approval with the baseline SHA, WIP SHA, file
list, and a commit-message subject. The lead committed
immediately based on that relay; the reviewer's actual
direct message arrived ~90 seconds later with a slightly
different (and more precise) commit message. The substance
matched, but the lead bypassed the blueprint's intended
flow. The user flagged this directly: "Why did you commit
without waiting for the reviewer?"

**How to apply:**

- A developer's idle notification that mentions reviewer
  approval is informational only. Do not act on it.
- Wait for the reviewer's own message to the lead. The
  reviewer's message contains the canonical composed
  commit message, baseline SHA, verified file list, and
  review summary — those are the post-approval inputs.
- If the developer relays an approval but the reviewer
  hasn't messaged the lead yet, the only correct action is
  to wait. The reviewer typically sends within seconds to
  a couple of minutes after the developer's handoff.
- This also protects against the developer mis-relaying:
  if the developer reports "approved" but the reviewer
  actually rejected with conditions, acting on the relay
  would skip work the reviewer wanted done.

**Cost of waiting:** roughly seconds to minutes per task.
**Cost of skipping the wait:** committing with the wrong
message, missing reviewer-flagged conditions, or acting on
an inaccurate relay. The trade-off is not close.

Related rules:
- The blueprint's "Developer-Reviewer Loop" section: "The
  reviewer messages you on approval with: composed commit
  message, baseline commit SHA, verified file list, review
  summary."
- [[feedback-one-plan-at-a-time]] — the broader pattern is
  patience in the pipeline; don't optimize for cycle speed
  at the cost of correctness.
