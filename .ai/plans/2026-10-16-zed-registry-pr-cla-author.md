**Repository:** root
**Status:** InProgress
**Created:** 2026-10-16

# Author Zed Registry Commits as the PAT Owner and Make Re-Runs Work

## Goal

Zed closed the registry PR for rlsp-yaml 0.1.3
(zed-industries/extensions#7955) because its commit was authored by
`github-actions[bot]`, which has no Zed CLA on file. Make
`zed-registry-pr.yml` author its registry commit as the account that owns
`ZED_REGISTRY_PAT`, and make the workflow re-runnable for a version whose
earlier registry branch is still in the fork, so that re-dispatching it for
`zed-v0.1.3` opens a registry PR that passes Zed's CLA check. The work is done
when both changes are on `main` and every CI run the push triggers succeeds.

## Context

- `.github/workflows/zed-registry-pr.yml` (workflow_dispatch, input `tag`
  such as `zed-v0.1.3`) syncs the fork `<PAT owner>/zed-editor-extensions`
  with `zed-industries/extensions`, shallow-clones it, advances the
  `extensions/rlsp-yaml` submodule to the tag's commit, bumps
  `[rlsp-yaml] version` in `extensions.toml`, commits on branch
  `update-rlsp-yaml-<version>`, pushes it to the fork, and opens the PR
  against `zed-industries/extensions`. All fork and PR operations use
  `ZED_REGISTRY_PAT`, a classic PAT with `public_repo` and `workflow` scopes
  owned by `chdalski`, who has a Zed CLA on file.
- Zed's cla-bot checks the GitHub account behind each commit in a registry
  PR. A PR opened by `chdalski` whose commit is authored by another identity
  fails the check.
- Workflows have no automated tests; `actionlint` is the static check.

## Steps

- [x] Clarify requirements with user
- [x] Design review of the approach
- [x] Task 1: Author the registry commit as the PAT owner
- [ ] Task 2: Re-run the workflow for a version with a leftover branch
- [ ] Push to `main` and confirm the triggered CI runs succeed

## Tasks

### Task 1: Author the registry commit as the PAT owner

The registry commit must come from the account that signed Zed's CLA.

- [x] The registry commit's author and committer are both the PAT owner:
      the name is the account's profile name, or its login when the profile
      name is unset or empty; the email is
      `<id>+<login>@users.noreply.github.com`
- [x] The workflow fetches the PAT owner's account details once, in a
      dedicated step, and every later use of the fork owner or the commit
      identity reads that step's outputs
- [x] No `github-actions[bot]` identity remains in `zed-registry-pr.yml`
- [x] `actionlint` reports no findings for
      `.github/workflows/zed-registry-pr.yml`

### Task 2: Re-run the workflow for a version with a leftover branch

A re-run for a version whose branch already exists in the fork must not
need manual cleanup. The fork currently holds `update-rlsp-yaml-0.1.3` with
the rejected bot-authored commit, and its PR (#7955) is closed. A re-run
builds the branch from the freshly synced fork `main`, so a plain push is
rejected as non-fast-forward.

- [ ] A run whose release branch already exists in the fork, with no open
      registry PR from it, replaces that branch and opens a new registry PR
- [ ] A run for which an open registry PR from the fork's release branch
      already exists fails before pushing, with an error naming that PR; an
      open PR from a same-named branch in another fork does not trigger
      this error
- [ ] A run whose commit would contain no changes fails before pushing,
      with an error stating that rlsp-yaml is already at that version in the
      registry
- [ ] A run with no existing branch in the fork pushes the branch and opens
      the registry PR, as before
- [ ] `actionlint` reports no findings for
      `.github/workflows/zed-registry-pr.yml`

## Decisions

- **Identity source:** derived at runtime from the PAT owner, not
  hardcoded — the commit follows whoever owns the secret.
- **No-reply email over the account's email:** needs no extra PAT scope
  (`user:email`), and GitHub maps it to the account by numeric id.
- **`git config` sets the identity:** it sets author and committer
  together, so the CLA check sees the PAT owner whichever field it reads.
- **`zed-release.yml` bump commit stays bot-authored:** that commit lives
  in this repo as the submodule target; the cla-bot checks only the
  registry PR's own commits.
- **Open registry PR → stop with an error:** the workflow never overwrites
  a branch that backs an open PR, so commits a Zed maintainer pushed to it
  survive; the user closes or updates that PR by hand.
- **Force-push over delete-then-push:** one operation; deleting an existing
  branch first needs a tolerated "not found" case.
- **No `--force-with-lease`:** the shallow clone has no tracking ref for
  the release branch, and the `zed-registry-pr` concurrency group already
  serializes runs.
- **Commit through the GitHub API declined:** GraphQL
  `createCommitOnBranch` cannot write the submodule pointer, and the REST
  tree route takes several API calls in place of two config lines.
- **`huacnlee/zed-extension-action` declined:** it would hand
  `ZED_REGISTRY_PAT` to a third-party action and replace a working
  workflow.

## Non-Goals

- Dispatching `zed-registry-pr.yml` for `zed-v0.1.3` — the user runs it
  after this plan lands on `main`.
- Changing the commit identity used by `zed-release.yml`.
