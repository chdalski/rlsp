**Repository:** root
**Status:** Completed (2026-06-08)
**Created:** 2026-06-08

## Goal

Fix the `git push` authentication failure in the
`open-registry-pr` job of `.github/workflows/zed-extension.yml`
so that the registry PR can be pushed to the fork and
opened against `zed-industries/extensions`. Document the
root cause and why it went undetected through prior CI
runs, committed alongside the code fix in this plan file.

## Context

- **The file:** `.github/workflows/zed-extension.yml`
- **Failure:** CI run
  [27143183159/80122147678](https://github.com/chdalski/rlsp/actions/runs/27143183159/job/80122147678)
  — the "Push branch and open PR" step fails with:
  ```
  fatal: could not read Username for 'https://github.com':
  No such device or address
  ```
- **Root cause:** `gh repo clone` (in the "Fork and clone"
  step) sets an HTTPS remote on `/tmp/extensions` without
  embedding credentials. The `GH_TOKEN` env var is set on
  the "Push branch and open PR" step, which `gh` CLI reads
  automatically — but raw `git push` does not. `git push`
  attempts unauthenticated HTTPS and fails because the
  fork requires write access.
- **Why it wasn't caught earlier:** This bug is
  pre-existing — the original workflow (before commit
  `a8facf13`) already had the same `git push` without
  credential setup. It was never reached in prior CI runs
  because earlier steps failed first:
  - May 13 run: the `[rlsp-yaml]` registry entry didn't
    exist yet, so "Bump version in fork's extensions.toml"
    failed (regex found 0 matches)
  - June 8 first run: `gh repo sync` required `workflow`
    scope on the PAT, which was missing at the time
  The push step was reached for the first time in the
  June 8 second run (after PAT scope was updated), and
  the auth bug surfaced.
- **Audit of all steps:** every step in the workflow was
  audited for git/auth issues:
  - `commit-and-tag` push: OK — `actions/checkout@v6`
    configures HTTPS credentials via `GITHUB_TOKEN`
  - `open-registry-pr` fork/sync/clone: OK — all use
    `gh` which reads `GH_TOKEN` directly
  - `open-registry-pr` submodule fetch: OK — fetches
    from `chdalski/rlsp` which is public; no auth needed
  - `open-registry-pr` push: **broken** — raw `git push`
    has no credential helper configured
- **Fix:** add `gh auth setup-git` before `git push`.
  This configures git's credential helper to use the
  `GH_TOKEN` env var for HTTPS authentication. The `gh`
  CLI is pre-installed on `ubuntu-latest` runners and
  already used in earlier steps.

## Steps

- [x] Add `gh auth setup-git` to the "Push branch and
      open PR" step
- [x] Run actionlint to verify no syntax errors

## Tasks

### Task 1: Add git credential setup before push ✅ `04d49adb`

In `.github/workflows/zed-extension.yml`, in the
"Push branch and open PR" step (line 197), add
`gh auth setup-git` after the `cd /tmp/extensions` line
and before `git checkout -b "$BRANCH"`. This configures
git's credential helper to use the `GH_TOKEN` env var
(already declared on this step) for HTTPS push
authentication.

- [x] `gh auth setup-git` is added before any `git` command
      that requires write access
- [x] `actionlint .github/workflows/zed-extension.yml`
      exits 0 with no errors
- [x] The step's `GH_TOKEN` env var (already present) is
      the only auth mechanism — no new secrets or env vars
      introduced
- [x] The plan file (with root-cause explanation in Context)
      is staged and committed alongside the workflow change

## Non-Goals

- Hardening the submodule fetch step with auth (public
  repo fetch works without credentials)
- Changing the `commit-and-tag` push mechanism
  (`actions/checkout` already handles credentials there)

## Decisions

- **`gh auth setup-git` over manual credential helper:**
  `gh auth setup-git` is the idiomatic way to wire
  `GH_TOKEN` into git on GitHub-hosted runners. The
  alternative — injecting an `extraheader` via `git -c` —
  works but is verbose, fragile (base64 encoding), and
  less readable. `gh` is already used throughout this job.
- **Single task:** one line in one file, no decomposition
  needed.
