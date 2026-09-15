---
name: Always consider performance
description: When planning conformance fixes, proactively assess and report performance implications without being asked
type: feedback
---

When planning conformance or correctness fixes, always proactively assess performance implications as part of the planning analysis — don't wait to be asked.

**Why:** The user had to ask "Did you consider the performance implications?" multiple times across consecutive plans. Performance analysis should be a standard part of the planning process for parser changes, not an afterthought.

**How to apply:** For every parser fix plan, include a performance analysis in the Context section that covers: which code path is affected (hot vs cold), what the change does computationally (new iteration, comparison, allocation), and whether it's a regression, neutral, or improvement. Present this proactively during clarification or plan presentation.
