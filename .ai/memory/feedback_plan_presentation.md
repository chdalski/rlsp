---
name: Present full plan before approval
description: Always show the complete plan file content to the user before asking for approval — never summarize and rush past the review gate
type: feedback
originSessionId: 7e50f19a-7cbf-4b38-81bd-268ef8f9b01f
---
Present the full plan file content to the user before asking for approval. Do not summarize the plan and treat a quick yes/no on the summary as approval.

**Why:** The user explicitly called out that showing a summary, getting a quick confirmation, committing the plan, and starting execution all in one flow bypasses the review gate. The instructions (step 6: "present the plan to the user for approval", step 7: "commit after user approval") are clear — the user needs to read the actual plan, not an outline.

**How to apply:** After writing the plan file, use Read to display its full contents to the user. Wait for them to read it and confirm. Only then commit and proceed to team creation and execution. The AskUserQuestion for plan approval should come after the user has seen the complete plan content, not before.
