---
name: feedback_no_non_decision_questions
description: "Don't ask the user to decide low-stakes items that don't change the outcome; resolve as lead"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 6e598ae3-52c4-47ec-8d26-7e18f55dc4a5
  modified: 2026-08-07T12:51:51.020Z
---

Do not surface a low-stakes, non-blocking item as an `AskUserQuestion` choice when the answer does not change the outcome. When a reviewer or process step flags something that doesn't block the user's goal and every option leads to an acceptable result, resolve it yourself as lead and just proceed.

Concretely: the user pushed back ("there is nothing to think about, is it?") when asked to choose how to handle deferred `pnpm.overrides` consolidation — cleanup that closes no open issue and was fine to keep as a backlog note.

**Why:** asking the user to pick between options that all lead to the same acceptable outcome wastes their attention and reads as indecision. Consulting is for decisions where the answer actually changes what gets done.

**How to apply:** before any `AskUserQuestion`, ask "does the answer change the outcome, or carry real stakes the user owns?" If not, pick the sensible default, state it in one line, and continue. Reserve questions for genuine forks — scope, outward-facing actions (push/publish), and priority tradeoffs the user cares about. Relates to [[feedback_plan_presentation]] and [[feedback_one_plan_at_a_time]].
