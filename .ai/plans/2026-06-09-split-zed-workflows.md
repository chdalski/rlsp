**Repository:** root
**Status:** NotStarted
**Created:** 2026-06-09

## Goal

Split `zed-extension.yml` into two independently retriggerable workflows and
rename it to clarify its role, so that a failure in the registry PR step does
not require re-bumping the version. Currently, `commit-and-tag` and
`open-registry-pr` are jobs in a single workflow — if `open-registry-pr` fails
after `commit-and-tag` succeeds, re-triggering the workflow bumps the version
again unnecessarily, leaving a skipped version and requiring manual recovery.
After the split, each half can be retried independently: re-run the version
bump, or re-run only the registry PR for an existing tag.

## Context

- The `zed-v*` tag pushed by `commit-and-tag` is a durable artifact. Once
  the tag exists, the registry PR workflow only needs the tag name — it can
  resolve the commit SHA and version from the tag itself.
- GitHub Actions events triggered by `GITHUB_TOKEN` do not trigger other
  workflows (push, tag create, etc.). However, `gh workflow run` (REST API
  dispatch) does work with `GITHUB_TOKEN`. So the automatic trigger from the
  release workflow to the registry PR workflow uses `gh workflow run`, not a
  `push: tags:` event.
- `release-plz.yml`'s `trigger-zed` job currently triggers `zed-extension.yml`
  via `workflow_dispatch` — this reference must be updated to the new filename.
- `ZED_REGISTRY_PAT` is a classic PAT with `public_repo` and `workflow`
  scopes, used for cross-repo operations against `zed-industries/extensions`.
- Concurrency group `zed-release` currently serializes both jobs. After the
  split, each workflow gets its own concurrency group.
- Existing workflow fixes (submodule advance, `gh auth setup-git`, sentence
  case PR title) must be preserved in the new workflow.

## Steps

- [ ] Create `zed-registry-pr.yml` with registry PR logic extracted from
      `zed-extension.yml`
- [ ] Rename `zed-extension.yml` → `zed-release.yml`, replace
      `open-registry-pr` with `trigger-registry-pr`, update
      `release-plz.yml` reference

## Tasks

### Task 1: Create the registry PR workflow

Create `.github/workflows/zed-registry-pr.yml` that encapsulates all registry
PR logic currently in the `open-registry-pr` job of `zed-extension.yml`.

**Trigger:** `workflow_dispatch` with a required `tag` input (string,
description: e.g. "Tag to create registry PR for (e.g. zed-v0.1.3)").

**Job structure — single job `open-registry-pr`:**

1. **Checkout at tag** — `actions/checkout@v6` with `ref: ${{ inputs.tag }}`
   and `fetch-depth: 1`. This gives the source tree at the tagged commit.

2. **Resolve version and SHA** — extract version by stripping the `zed-v`
   prefix from the tag input. Validate format (`X.Y.Z`). Get SHA via
   `git rev-parse HEAD` (checkout is at the tag). Output both as step outputs.

3. **Fork and clone registry** — identical to the current "Fork and clone
   zed-industries/extensions" step (fork, sync, clone to `/tmp/extensions`).
   Uses `ZED_REGISTRY_PAT`.

4. **Advance submodule** — identical to current "Advance submodule to new
   release" step, consuming SHA and VERSION from step 2 outputs.

5. **Bump version in extensions.toml** — identical to current "Bump version
   in fork's extensions.toml" step.

6. **Push branch and open PR** — identical to current "Push branch and open
   PR" step (includes `gh auth setup-git`, sentence case title).

**Job metadata:**
- `permissions: contents: read` (checkout needs read; cross-repo ops use PAT)
- `concurrency: { group: zed-registry-pr, cancel-in-progress: false }`
- Workflow-level `name: Zed Registry PR`

**Acceptance criteria:**
- [ ] New workflow file exists at `.github/workflows/zed-registry-pr.yml`
- [ ] `workflow_dispatch` trigger with required `tag` input of type string
- [ ] Version extracted from tag name (strip `zed-v` prefix), validated as
      semver `X.Y.Z`
- [ ] SHA resolved from `git rev-parse HEAD` after checkout at tag
- [ ] All four registry PR steps (fork/sync/clone, submodule advance,
      version bump, push/PR) present and functionally identical to current
- [ ] `gh auth setup-git` present before `git push`
- [ ] PR title uses sentence case: `"Bump extension rlsp-yaml to ${VERSION}"`
- [ ] Explicit `permissions` block on the job
- [ ] Concurrency group set

### Task 2: Rename and restructure the release workflow

Rename `zed-extension.yml` → `zed-release.yml` and restructure it to dispatch
the new registry PR workflow instead of running the registry PR logic inline.

1. **Rename the file** — `git mv .github/workflows/zed-extension.yml .github/workflows/zed-release.yml`.

2. **Update workflow name** — change `name: Zed Extension` to
   `name: Zed Release`.

3. **Remove the `open-registry-pr` job** entirely.

4. **Clean up `commit-and-tag` outputs** — remove the `sha` output
   declaration and the `id: commit` / `echo "sha=..."` line from the
   "Commit, tag, and push" step. The `version` output stays (consumed by the
   new trigger job).

5. **Add `trigger-registry-pr` job:**
   - `needs: commit-and-tag`
   - `runs-on: ubuntu-latest`
   - `permissions: actions: write` (needed for `gh workflow run`)
   - Single step: `gh workflow run zed-registry-pr.yml --repo "${GITHUB_REPOSITORY}" --field "tag=zed-v${VERSION}"` where `VERSION` comes from `needs.commit-and-tag.outputs.version`, `GH_TOKEN` is `${{ secrets.GITHUB_TOKEN }}`

6. **Update `release-plz.yml`** — change `gh workflow run zed-extension.yml`
   to `gh workflow run zed-release.yml` in the `trigger-zed` job (line 93).

**Acceptance criteria:**
- [ ] File renamed from `zed-extension.yml` to `zed-release.yml`
- [ ] Workflow `name:` updated to `Zed Release`
- [ ] `open-registry-pr` job removed
- [ ] `sha` output and related lines removed from `commit-and-tag`
- [ ] `trigger-registry-pr` job added with correct `needs`, `permissions`,
      and dispatch command
- [ ] `version` output still present in `commit-and-tag`
- [ ] `check`, `preflight`, and `commit-and-tag` jobs unchanged (aside from
      the output cleanup)
- [ ] `release-plz.yml` updated to reference `zed-release.yml`

## Decisions

- **Rename `zed-extension.yml` → `zed-release.yml`** — after extracting the
  registry PR logic, the remaining workflow handles version bumping, tagging,
  and triggering the downstream PR. "Zed Release" describes this role; "Zed
  Extension" is vague when there are two Zed-related workflows.
- **`workflow_dispatch` only, no `push: tags:` trigger** — GITHUB_TOKEN
  tag pushes don't trigger other workflows, so `push: tags:` would only
  fire from manual pushes or PAT-based pushes. Adding it risks duplicate
  runs if the token type changes later. The explicit `gh workflow run`
  dispatch is the sole automatic trigger; `workflow_dispatch` also serves
  manual retrigger.
- **Checkout our repo at the tag in the new workflow** — simplest way to
  resolve both SHA (via `git rev-parse HEAD`) and version (from tag name).
  Avoids complex GitHub API tag-object dereferencing for annotated tags.
- **Separate concurrency groups** — `zed-release` for the version-bump
  workflow, `zed-registry-pr` for the registry PR workflow. They run in
  separate workflows now and don't need cross-workflow serialization — the
  dispatch ordering already ensures sequencing.
- **Preflight stays in `zed-release.yml` only** — the preflight checks
  upstream registration before creating a tag. Once a tag exists, registration
  is assumed — the registry PR's Python script would fail with "Expected
  exactly 1 replacement, got 0" if the section were missing, providing its
  own error signal.

## Non-Goals

- Adding tag-push triggers to the new workflow
- Changing `ZED_REGISTRY_PAT` scopes or type
- Modifying the Zed extension source code or `extension.toml`
