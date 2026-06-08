**Repository:** root
**Status:** Completed (2026-06-08)
**Created:** 2026-06-08

## Goal

Fix the Zed extension auto-update workflow so the registry
PR it opens against `zed-industries/extensions` includes
both the version bump in `extensions.toml` and the
advanced git submodule pointer. Without the submodule
update, Zed's CI rejects the PR because the version in
`extensions.toml` doesn't match the `extension.toml` at
the submodule's pinned commit. Additionally, sync the
fork to upstream before branching to prevent stale diffs,
and bring all used GitHub Actions to their latest stable
major versions per project convention.

## Context

- **The file:** `.github/workflows/zed-extension.yml`
- **Current behavior:** The `open-registry-pr` job
  (lines 122–191) forks `zed-industries/extensions`,
  bumps the `version` string in `extensions.toml` via a
  Python regex, then opens a PR. It never touches the
  `extensions/rlsp-yaml` git submodule.
- **Zed's requirement:** updating an extension requires
  both advancing the submodule to the new version's
  commit and updating the `version` field to match the
  `extension.toml` at that commit (per Zed's official
  docs).
- **Upstream state:** `extensions/rlsp-yaml` submodule
  points at commit `098da7f` (tag `zed-v0.1.1`) in
  `https://github.com/chdalski/rlsp`. Upstream
  `extensions.toml` has `version = "0.1.1"` — both in
  sync currently.
- **The SHA source:** The `commit-and-tag` job
  (lines 66–120) already creates the commit on our
  `main` that bumps `extension.toml` to the new version
  and tags it. This commit is what the submodule must
  point to. Currently, `commit-and-tag` only outputs
  `version`; it does not output the commit SHA.
- **Fork staleness:** `gh repo fork` is a no-op if the
  fork already exists and does not sync it to upstream.
  Over time the fork's `main` drifts behind
  `zed-industries/extensions`, causing the PR to include
  unintended diffs (reverts of other extensions' updates).
- **Method choice:** user chose Method B (submodule init +
  fetch + checkout) over Method A (`git update-index
  --cacheinfo`) for its self-validation: the fetch fails
  fast if the SHA is unreachable, and checking out the
  submodule allows a local version cross-check before
  opening the PR.
- **Action versions:** per project convention
  (`github-workflows.md` rule), touching the workflow
  requires checking that all actions used are at their
  latest stable major versions. The workflow currently
  uses `actions/checkout@v6`, `dtolnay/rust-toolchain@stable`,
  `Swatinem/rust-cache@v2` — verify these are current.

## Steps

- [x] Add SHA output to `commit-and-tag` job
- [x] Add fork-sync step to `open-registry-pr` job
- [x] Add submodule advance and version cross-check
- [x] Update `git add` and commit to include submodule
- [x] Verify action versions are current
- [x] Test: workflow YAML is valid (actionlint or
      equivalent)

## Tasks

### Task 1: Fix the `open-registry-pr` job to advance the submodule ✅ `b3cc7511`

All changes are in `.github/workflows/zed-extension.yml`.

**1a. Add SHA output to `commit-and-tag`**

In the `commit-and-tag` job:
- Add `sha: ${{ steps.commit.outputs.sha }}` to the
  job's `outputs:` block (alongside the existing
  `version` output).
- Give the "Commit, tag, and push" step an `id: commit`.
- After `git push`, add
  `echo "sha=$(git rev-parse HEAD)" >> "$GITHUB_OUTPUT"`.

**1b. Add fork-sync step to `open-registry-pr`**

Add the fork sync to the existing "Fork and clone" step,
between the `gh repo fork` call and the `gh repo clone`
call. `FORK_OWNER` is a shell-local variable set earlier
in the same step, so the sync must be in the same step
to access it:
```bash
gh repo sync "${FORK_OWNER}/zed-editor-extensions" \
  --source zed-industries/extensions --branch main
```
This uses the same `GH_TOKEN: ${{ secrets.ZED_REGISTRY_PAT }}`
env already declared on the step. The clone then gets the
synced state.

**1c. Add submodule advance with version cross-check**

Add a new step "Advance submodule to new release" between
the clone and the version bump. Using the SHA from
`commit-and-tag` outputs:

```bash
cd /tmp/extensions
git submodule update --init --depth=1 extensions/rlsp-yaml
cd extensions/rlsp-yaml
git fetch origin ${SHA}
git checkout ${SHA}

# Cross-check: extension.toml at this commit must declare
# the expected version
ACTUAL=$(grep '^version' \
  rlsp-yaml/integrations/zed/extension.toml \
  | head -1 | sed 's/.*"\(.*\)"/\1/')
if [ "$ACTUAL" != "$VERSION" ]; then
  echo "::error::extension.toml at ${SHA} says ${ACTUAL}, expected ${VERSION}"
  exit 1
fi
cd ../..
```

Pass both `SHA` and `VERSION` as env vars from the
`commit-and-tag` outputs.

**1d. Update the commit step**

In the existing "Push branch and open PR" step, add the
submodule to the staging area:
```bash
git add extensions/rlsp-yaml extensions.toml
```
(Currently only `git add extensions.toml`.)

The commit message, PR title, and PR body remain
unchanged — they already describe the version bump, and
the submodule advance is an implicit part of that.

**1e. Verify action versions**

Check that `actions/checkout`, `dtolnay/rust-toolchain`,
and `Swatinem/rust-cache` are at their latest stable
major versions. Update any that aren't.

Acceptance criteria:
- [x] `commit-and-tag` outputs both `version` and `sha`
- [x] Fork is synced to upstream before cloning
- [x] Submodule `extensions/rlsp-yaml` is initialized,
      fetched at the new SHA, and checked out
- [x] Version cross-check asserts `extension.toml` at the
      checked-out commit matches the version being written
      to `extensions.toml`; step fails with a clear error
      annotation if mismatched
- [x] `git add` stages both `extensions/rlsp-yaml` and
      `extensions.toml`
- [x] All actions at latest stable major versions
- [x] `actionlint .github/workflows/zed-extension.yml`
      exits 0 with no errors

## Non-Goals

- Changing the direct-push-to-main approach for version
  bumps on our repo (no PR-based flow — direct push is
  correct; see discussion)
- Version-pinning the binary download in the extension
  code (the `latest_github_release` approach is correct
  for now; binary-interface breaks are a separate concern)
- Merging release-plz PR #39 (that's a user action, not
  a workflow fix)

## Decisions

- **Method B for submodule:** user chose the init + fetch +
  checkout approach over `git update-index --cacheinfo`
  for its self-validation — fetch fails fast if SHA is
  unreachable, and checkout enables local version
  cross-check before opening the PR.
- **Single task:** the changes are all in one file, tightly
  coupled (SHA output feeds submodule step), and not
  independently useful — splitting would add commit churn
  with no reviewability benefit.
- **Fork sync via `gh repo sync`:** keeps the fork's main
  current with upstream before branching, preventing stale
  diffs that could revert other extensions' updates.
