---
name: No completed items in memory files
description: Remove completed plans and todos from memory — completed work lives in plan files and git history
type: feedback
originSessionId: 4da03109-f825-431a-8ef9-29e249d33e7d
---
Do not keep completed items in memory files (especially the follow-up task queue). Remove items when their plan is marked Completed.

**Why:** Completed work already lives in the plan file and git history. Keeping it in memory adds noise and stale state — the memory file should only show what's still open.

**How to apply:** When marking a plan complete, remove its entry from the follow-up queue rather than striking it through or annotating it as done.
