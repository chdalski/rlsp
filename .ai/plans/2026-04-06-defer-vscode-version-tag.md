**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-06

## Goal

Move the VS Code extension version tag creation from before
the build to after marketplace publish. Currently
`resolve-version` creates and pushes the `vscode-v*` tag
before any build runs — if a build or publish fails, the
tag exists for a version that was never released, wasting
a version number. The tag should only exist for versions
that are live on the marketplace.

Also clean up dead `startsWith(github.ref,
'refs/tags/rlsp-yaml-v')` conditions left over from the
removed tag trigger.

## Context

- **Current flow:** `resolve-version` → generates CalVer
  version + creates git tag → `build-extension` (5 platforms)
  → `publish-extension`
- **Desired flow:** `resolve-version` → generates CalVer
  version (no tag) → `build-extension` (5 platforms) →
  `publish-extension` → `tag-version` (new job, creates
  tag only after successful publish)
- **Dead conditions:** `resolve-version` (line 19) and
  `publish-extension` (line 168) both check
  `startsWith(github.ref, 'refs/tags/rlsp-yaml-v')`. This
  was for the old tag trigger removed in `6442554`. Since
  the workflow now only triggers via `workflow_dispatch` or
  push-to-main (CI), these conditions are dead code.
- **Tag timing decision:** Tag after marketplace publish
  succeeds. The tag means "this version is live."

### Key file

- `.github/workflows/vscode-extension.yml`

## Steps

- [ ] Remove tag creation from `resolve-version`
- [ ] Add `tag-version` job after `build-extension`
- [ ] Remove dead tag ref conditions
- [ ] Update `publish-extension` dependency chain

## Tasks

### Task 1: Restructure version tagging in vscode-extension.yml

All changes in `.github/workflows/vscode-extension.yml`:

**1. `resolve-version` job (lines 16-60):**
- Remove the "Create version tag" step (lines 50-60)
- Change `permissions: contents: write` to
  `permissions: contents: read` (only needs checkout now)
- Remove dead condition: change line 19 from
  `if: startsWith(github.ref, 'refs/tags/rlsp-yaml-v') || github.event_name == 'workflow_dispatch'`
  to `if: github.event_name == 'workflow_dispatch'`

**2. `publish-extension` job (lines 164-192):**
- Change `needs:` from `[resolve-version, build-extension]`
  to `[resolve-version, build-extension]` (unchanged, but
  remove dead condition)
- Remove dead condition: change line 168 from
  `if: startsWith(github.ref, 'refs/tags/rlsp-yaml-v') || github.event_name == 'workflow_dispatch'`
  to `if: needs.resolve-version.outputs.version != ''`
  (publish only when a version was resolved — same
  semantic but without the dead ref check)

**3. New `tag-version` job (after `publish-extension`):**
- `needs: [resolve-version, publish-extension]`
- Runs only when publish succeeded and a version exists:
  `if: needs.resolve-version.outputs.version != ''`
- `permissions: contents: write`
- Steps:
  1. `actions/checkout@v6` with `fetch-depth: 0`
  2. Create and push the tag (same logic as the removed
     step, using `needs.resolve-version.outputs.version`)

- [ ] Modify `resolve-version` — remove tag step, downgrade
      permissions, remove dead condition
- [ ] Modify `publish-extension` — remove dead condition
- [ ] Add `tag-version` job after `publish-extension`
- [ ] Verify YAML is structurally valid

## Decisions

- **Tag after publish:** User decision — the tag means
  "this version is live on the marketplace." Tagging is
  unlikely to fail, so doing it last adds no meaningful
  risk while ensuring the tag is a reliable release signal.
- **`publish-extension` condition:** Changed from the dead
  tag ref check to `needs.resolve-version.outputs.version
  != ''`. This is the correct gate: publish only when we
  have a version number (i.e., this is a release run, not
  a CI-only run triggered by push to main).
- **`build-extension` condition unchanged:** It uses
  `if: ${{ !failure() && !cancelled() }}` which allows it
  to run both for CI (push to main, no version) and
  release (workflow_dispatch, with version). The version
  stamp step inside is already conditional on
  `needs.resolve-version.outputs.version != ''`.
