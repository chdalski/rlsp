---
name: project_dependabot_group_change_skips_excluded_dep
description: "After a dependabot.yml group change, the push-triggered run rebuilds the open group PR but skips a newly excluded dependency until the next scheduled run"
metadata: 
  node_type: memory
  type: project
  originSessionId: 8b23f9b2-69ac-4d57-9bd4-69ba0e149629
  modified: 2026-09-16T12:44:37.667Z
---

Observed 2026-09-16 when `@types/vscode` was taken out of the
`vscode-extension-dependencies` group with `exclude-patterns` (commit `e8ec75aa`):

- Pushing a `dependabot.yml` change starts every update job within about a minute.
- The npm version job saw the open group PR #70 (which still bumped
  `@types/vscode`), left it for a separate refresh job, and marked **all of
  #70's dependencies as handled, the excluded one included**. So no separate
  PR was opened for `@types/vscode` in that run (Actions run 35096393013).
- The refresh job closed #70 itself ("no longer needed") and opened #72 without
  the excluded dependency. No `@dependabot rebase` comment was needed.

**Why:** the dependency you exclude does not get its own PR on the push-triggered
run. It is expected on the next scheduled weekly run (Thursdays, about 23:03 UTC).

**How to apply:** after changing Dependabot groups, don't treat a missing
single-dependency PR as a config error. Check the next scheduled run first. Plan
`.ai/plans/2026-09-16-vscode-types-dependabot-split.md` is still open until that
PR is verified. Related: [[project_dependabot_npm_cooldown]],
[[feedback_trunk_based_land_on_main]].
