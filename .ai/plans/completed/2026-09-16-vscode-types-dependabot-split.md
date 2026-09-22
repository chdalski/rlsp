**Repository:** root
**Status:** Completed (2026-09-17)
**Created:** 2026-09-16

## Goal

Dependabot PR #70 fails because it bumps `@types/vscode` past `engines.vscode`,
and the lockstep guard test fails that on purpose. But the VS Code extension's
Dependabot group takes every npm dependency (`patterns: ["*"]`), so that one
intentional failure also blocks the unrelated updates in the same PR, and it
will happen again almost every week. Take `@types/vscode` out of that group so
its bump arrives in a PR of its own, which stays red until the minimum VS Code
version is changed on purpose, while every other extension dependency update
can pass CI and merge again.

## Context

- **Why PR #70 fails (investigated 2026-09-16).** Dependabot opened #70 on
  2026-09-10 for the `vscode-extension-dependencies` group: `@types/node`
  26.4.1→26.5.0, `@types/vscode` 1.125.0→1.136.0, `typescript-eslint`
  8.69.0→8.70.0. The only failing check is `VS Code Extension Coverage`, and
  inside it only one test: `rlsp-yaml/integrations/vscode/src/engine-compat.test.ts`
  › "the two real values agree on major.minor". The other 86 of 87 unit tests
  pass, and `VS Code Extension Static Checks`, the Rust CI jobs, and Codecov
  are green. So `@types/node` and `typescript-eslint` cause no failures; the
  whole failure comes from `@types/vscode` 1.136.0 against `engines.vscode`
  `^1.125.0`.
- **The guard is intentional.** Commit `abc8675c` (2026-09-07) pinned
  `@types/vscode` to an exact `1.125.0` and added `engine-compat.test.ts`,
  because `vsce package` refuses to build when `@types/vscode`'s major.minor is
  higher than `engines.vscode`'s. That had broken all five `Build VSIX` jobs
  after an earlier Dependabot PR. The user decided then to keep
  `engines.vscode` at `^1.125.0`, since the source uses no API newer than 1.125,
  and to add no Dependabot ignore rule for `@types/vscode`. Instead the guard
  turns the bump red, so the minimum VS Code version is changed on purpose. This
  plan keeps both decisions.
- **Why this recurs.** `.github/dependabot.yml` has one npm entry for
  `/rlsp-yaml/integrations/vscode`, with the single group
  `vscode-extension-dependencies` matching `"*"`. `@types/vscode` now ships
  about weekly (1.134.0 on 2026-08-19, 1.136.0 on 2026-09-02, 1.137.0 on
  2026-09-09), so nearly every weekly group PR will include a newer version and
  fail as a whole.
- **Dependabot grouping semantics** (GitHub docs, "Dependabot options
  reference" and "Optimizing the creation of pull requests for Dependabot
  version updates"): if a dependency matches both a group's `patterns` and its
  `exclude-patterns`, it is left out of that group. Dependabot "continues to
  raise single pull requests" for dependencies left out of every group. The
  docs say nothing about what happens to an already-open group PR when the
  group configuration changes.
- **A change to `dependabot.yml` on `main` starts a Dependabot run right
  away.** Observed: pushing `31d2c92a` at 2026-09-08 13:27:16 UTC started all
  four ecosystem update jobs at 13:27:59 UTC. The jobs show up in
  `gh run list --workflow "Dependabot Updates"`.
- **Known Dependabot noise.** The npm update job sometimes fails with
  `ERR_PNPM_NO_MATURE_MATCHING_VERSION`. That comes from Dependabot's own 72-hour
  `minimumReleaseAge`, not from repo configuration, and it clears on the next
  run once the blocking package is older than 72 hours.
- **Workflow conventions.** The work lands directly on `main` (trunk-based, no
  feature branch). For a Dependabot PR that the new state of `main` makes
  obsolete, the user prefers commenting `@dependabot rebase`, so Dependabot
  re-checks the PR against `main`, over closing it by hand.
- **Specification.** The Dependabot configuration schema
  (`https://json.schemastore.org/dependabot-2.0.json`) describes valid
  `dependabot.yml` content, including `groups.*.exclude-patterns`.

## Steps

- [x] Investigate why PR #70 fails
- [x] Clarify the fix direction with the user (separate `@types/vscode` from the group)
- [x] Exclude `@types/vscode` from the extension's catch-all Dependabot group
- [x] Push to `main` and let Dependabot's immediate run pick up the new configuration
- [x] Comment `@dependabot rebase` on PR #70 if it still carries `@types/vscode` after that run
      — not needed: the push-triggered run closed #70 itself ("no longer needed")
      and replaced it with #72 (`@types/node`, `typescript-eslint`, `vite`; no
      `@types/vscode`), which passes every check
- [x] Verify the outcomes stated in the Goal on GitHub — verified 2026-09-17
      after a manual "Check for updates" (the push-triggered run on 2026-09-16
      had skipped `@types/vscode`; see Actions run 35096393013). Both npm update
      jobs succeeded. #73 bumps only `@types/vscode` (1.125.0→1.137.0), and its
      only failure is the `engine-compat.test.ts` lockstep guard (1 failed, 86
      passed). The grouped PR #72 (`@types/node`, `typescript-eslint`, `vite`)
      passes every check

## Tasks

### Task 1: Take `@types/vscode` out of the extension's catch-all Dependabot group

Change the extension's npm Dependabot entry so `@types/vscode` is no longer
bundled with the other dependencies. Leave a comment explaining why, so no one
later folds it back in as a tidy-up.

- [x] In the npm entry for `/rlsp-yaml/integrations/vscode`, the
      `vscode-extension-dependencies` group no longer matches `@types/vscode`;
      every other dependency still matches it
- [x] No Dependabot `ignore` rule for `@types/vscode` exists; its updates are
      still proposed
- [x] A comment beside the change says: `@types/vscode` must move in lockstep
      with `engines.vscode`; its bump PR is expected to stay red until the
      minimum VS Code version is raised on purpose; keeping it in the group would
      block every other extension dependency update
- [x] The existing `typescript` major-version ignore rule and its comment are
      unchanged, and no other ecosystem entry in `.github/dependabot.yml` changes
- [x] `.github/dependabot.yml` passes validation against the SchemaStore
      `dependabot-2.0` JSON schema; the handoff names the command and its output
- [x] `.github/dependabot.yml` is the only file changed

## Decisions

- **Exclude from the catch-all group instead of defining a second group.** One
  `exclude-patterns` entry is the fewest moving parts and does not depend on the
  order groups are listed in. Dependabot then opens a separate PR just for
  `@types/vscode`, which is what the user asked for: the types bump in its own
  PR.
- **`engines.vscode` stays `^1.125.0` and the guard test stays unchanged.**
  Both are earlier user decisions (commit `abc8675c`). The separate
  `@types/vscode` PR failing that guard is the intended signal, not a defect to
  fix here.
- **PR #70 is handled with `@dependabot rebase`, not closed by hand.** This
  follows the user's standing preference. If #70 still bumps `@types/vscode`
  after the rebase, that is a blocker to report to the user, not grounds to
  close it or edit it by hand.
- **Plan-level outcomes, checked after the push:** Dependabot's npm update job
  for `/rlsp-yaml/integrations/vscode` succeeds on the new configuration; no
  open Dependabot PR bumps `@types/vscode` together with another dependency; a
  separate open Dependabot PR bumps `@types/vscode`, and its only failing test
  is the `engine-compat.test.ts` lockstep guard; the grouped extension PR (#70
  after rebase, or its successor) passes every check. If the npm job fails with
  `ERR_PNPM_NO_MATURE_MATCHING_VERSION`, the plan stays open until a later run
  succeeds; the failure is not treated as a pass.

## Non-Goals

- Raising `engines.vscode` or changing `@types/vscode` in `package.json`. That
  choice is the user's to make, through the separate PR this plan creates.
- Merging PR #70 or the separate `@types/vscode` PR.
- Changing `engine-compat.test.ts` or its comments.
- Regrouping other Dependabot ecosystems (cargo, github-actions, the Zed crate).
