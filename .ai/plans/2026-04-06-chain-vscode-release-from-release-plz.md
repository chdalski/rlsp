**Repository:** root
**Status:** InProgress
**Created:** 2026-04-06

## Goal

Chain the VS Code extension release from the release-plz
workflow so that publishing `rlsp-yaml` to crates.io
automatically triggers a VS Code extension build and
marketplace publish. Currently this never happens because
GitHub Actions suppresses push events (including tags)
created by `GITHUB_TOKEN`, making the `on.push.tags`
trigger in `vscode-extension.yml` dead code.

## Context

- **Root cause:** release-plz uses `GITHUB_TOKEN` to create
  tags. GitHub's anti-recursion rule prevents
  `GITHUB_TOKEN`-created push events from triggering other
  workflows. The `rlsp-yaml-v*` tag trigger has never fired.
- **Exception:** `workflow_dispatch` events initiated via
  `GITHUB_TOKEN` *do* create new workflow runs (explicitly
  exempted by GitHub).
- **Current state:** `rlsp-yaml 0.6.0` is published to
  crates.io with all platform binaries on GitHub Releases,
  but the VS Code extension was never built or published
  for this release.
- **Immediate action:** After the pipeline fix, manually
  trigger the VS Code workflow to release 0.6.0.

### Key files

- `.github/workflows/release-plz.yml` — release pipeline,
  owns crate publishing and binary builds
- `.github/workflows/vscode-extension.yml` — VS Code
  extension build and marketplace publish

### Constraints

- The VS Code workflow's `workflow_dispatch` path already
  works correctly (auto-generates CalVer version, builds
  all platforms, publishes to marketplace).
- The `on.push.branches.main` + `paths` trigger in the
  VS Code workflow is used for CI checks on extension code
  changes — this must be preserved.
- `gh workflow run` with `GITHUB_TOKEN` + `actions: write`
  permission is the mechanism for dispatching.
- Per `github-workflows.md` rules: explicit permissions,
  latest action versions, pin to major version tags.

## Steps

- [x] Add `trigger-vscode` job to `release-plz.yml`
- [x] Remove dead `tags` trigger from `vscode-extension.yml`
- [ ] Manually trigger VS Code workflow for 0.6.0 release

## Tasks

### Task 1: Chain VS Code release from release-plz and remove dead trigger

**In `release-plz.yml`:** Add a `trigger-vscode` job after
`release-plz-release` that:
1. Runs only when `releases_created == 'true'`
2. Checks if `rlsp-yaml` is among the released packages
   (using the `releases` output JSON)
3. Triggers the VS Code extension workflow via
   `gh workflow run vscode-extension.yml`
4. Declares explicit `permissions: actions: write` (needed
   for workflow dispatch) and `contents: read` (needed for
   checkout to use `gh`)

The job needs `actions/checkout` so that `gh` can resolve
the workflow file. Use `GH_TOKEN` env var with
`${{ secrets.GITHUB_TOKEN }}`.

**In `vscode-extension.yml`:** Remove the `tags` block
from `on.push` — it is dead code (never fires due to
`GITHUB_TOKEN` limitation). Keep the `branches` + `paths`
trigger for CI and the `workflow_dispatch` trigger for
releases.

- [x] Add `trigger-vscode` job to `release-plz.yml` (6442554)
- [x] Remove dead `tags` trigger from `vscode-extension.yml` (6442554)
- [x] Verify workflow YAML is valid (6442554)

### Task 2: Trigger VS Code 0.6.0 release

After Task 1 is committed and pushed, manually trigger the
VS Code extension workflow to release the 0.6.0 extension
that was missed.

- [ ] Run `gh workflow run vscode-extension.yml`
- [ ] Verify the workflow starts successfully

## Decisions

- **Mechanism:** `gh workflow run` (workflow_dispatch via
  API) rather than a PAT or `repository_dispatch`. This
  uses the existing `GITHUB_TOKEN` with no additional
  secrets, and `workflow_dispatch` is explicitly exempted
  from GitHub's anti-recursion rule.
- **Dead trigger removal:** The `on.push.tags: rlsp-yaml-v*`
  trigger is removed because it is misleading — it suggests
  the workflow responds to tags when it structurally cannot.
  Keeping dead triggers invites future debugging sessions
  that trace a non-existent code path.
- **No checkout ref pinning:** The `workflow_dispatch`
  trigger checks out `main` at HEAD, which is the same
  commit the tag points to (release-plz tags on main). A
  race condition is theoretically possible if someone pushes
  between tag creation and dispatch, but this is acceptable
  for this project's single-contributor model.
