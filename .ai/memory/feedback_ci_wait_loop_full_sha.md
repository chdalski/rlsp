---
name: feedback_ci_wait_loop_full_sha
description: gh run list --commit needs the full 40-char SHA; a short SHA returns zero runs and silently breaks CI wait loops
metadata:
  node_type: memory
  type: feedback
  originSessionId: 3b76f14d-c522-46da-878e-68c9890a64c1
  modified: 2026-10-11T09:06:56.175Z
---

`gh run list --commit <sha>` matches only the full 40-character SHA. A short SHA such as `3a66a743` returns an empty list with no error.

On 2026-10-11 a background wait loop (`while gh run list --commit <short> ... | grep -qv completed`) exited at once on that empty list. CI then finished green unnoticed, and the user had to point out it had been done "long ago".

**Why:** an empty result looks the same as "all runs completed" to a naive loop. The post-push CI check then never actually happens.

**How to apply:**
- Pass `$(git rev-parse <ref>)` to `--commit`, or filter `gh run list --json headSha` with `startswith`.
- Before backgrounding a wait loop, run its query once and confirm it returns the expected runs. An empty first result means the query is wrong, not that CI is done.

Related: [[feedback_verify_ci_pipeline_after_push]].
