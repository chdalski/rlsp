---
name: feedback_verify_ci_pipeline_after_push
description: "After pushing, check the full CI pipeline (gh run list), not just local gates and Dependabot alerts"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 6e598ae3-52c4-47ec-8d26-7e18f55dc4a5
  modified: 2026-08-10T07:12:10.373Z
---

After pushing changes that CI covers, verify the actual CI pipeline result (`gh run list`, `gh run view --log-failed`) before reporting the work green — local gates and alert checks do not cover the full matrix.

Concretely (2026-08-07): the VS Code extension CI (`vscode-extension.yml`) was red the entire session and I didn't notice until the user told me to check pipeline outputs. Two gaps: (1) the local gate runs `pnpm run test` (vitest, transpile-only), which does NOT typecheck — `tsc` type errors in test code passed vitest but failed CI's `test:integration` (`tsc && vscode-test`); run `pnpm exec tsc --noEmit` as part of the gate. (2) CI runs a Windows matrix job; a CRLF line-ending bug in a lockfile-parsing test failed only on Windows, invisible to the Linux-only local gate.

**Why:** "Dependabot alerts closed" + "local build/lint/test pass on Linux" is NOT "CI green." Marking a plan Completed while its CI workflow is red overstates the outcome.

**How to apply:** after the post-approval push for any task touching CI-covered code, run `gh run list --limit ~10` and confirm the relevant workflow(s) succeeded across the whole matrix (incl. Windows/macOS). If a workflow is path-filtered, confirm it actually ran. Treat a standalone `tsc --noEmit` typecheck as part of the definition of done for TypeScript, distinct from vitest. Relates to [[feedback_clean_build_before_lint_verification]].
