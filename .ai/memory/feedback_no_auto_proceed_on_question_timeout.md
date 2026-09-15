---
name: feedback-no-auto-proceed-on-question-timeout
description: "On AskUserQuestion \"no response after 60s / proceed with best judgment\" notices, do not guess user-owned decisions — hold and wait for the real answer"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b6ff04e3-b575-489e-838c-bb64974f3620
---

When `AskUserQuestion` returns the harness notice "No response after 60s —
the user may be away from keyboard. Proceed using your best judgment," do
NOT auto-answer or act on a decision that is the user's to make. Treat the
notice as "user is temporarily away," not "user delegated this decision."
Hold, and re-surface the question when they return.

**Why:** the user explicitly objected to answers being "automagically
guessed." The 60s timeout is a harness affordance (not user-configurable,
not something the assistant sets); its "best judgment" wording is a nudge,
not the user's instruction.

**How to apply:** on a timeout, either wait or continue only with clearly
reversible, non-decisional prep (e.g. read-only investigation) — never
commit the user to a choice among presented options. Genuine decision
points stay open until the user answers. Relatedly, prefer waiting over
proceeding even when a fix looks low-risk. See [[feedback-wait-for-reviewer-direct-approval]].
