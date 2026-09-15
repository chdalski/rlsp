---
name: feedback_trunk_based_land_on_main
description: "User develops trunk-based — land work directly on main, no feature-branch/PR ceremony"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 19da60e8-20d1-4660-9f0d-1e1f4cbcc1c4
  modified: 2026-08-10T09:50:58.434Z
---

The user develops trunk-based: land work directly on `main` rather than
creating feature branches or opening PRs for internal work. For PR #58
(a Dependabot bump) the user chose "land it on main - trunk based" over
both reusing the Dependabot branch and creating our own branch.

**Why:** the user maintains this repo solo (AI-authored project) and
prefers a linear trunk history over PR-branch mechanics.

**How to apply:** the blueprint pipeline already commits to the current
branch (`main`) via the After-Reviewer-Approval squash+commit sequence —
keep doing that; do not spin up a separate feature branch for the team's
work. For a Dependabot PR superseded by landing equivalent work on `main`,
the user prefers commenting `@dependabot rebase` (after the fixes land,
with any un-landable group members added to `dependabot.yml` `ignore`) so
Dependabot re-evaluates the group against the new `main` and self-closes
its now-empty PR — rather than closing it by hand. See
[[feedback_verify_ci_pipeline_after_push]] for the post-push CI check.
