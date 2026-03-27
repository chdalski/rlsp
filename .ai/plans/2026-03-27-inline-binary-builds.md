**Repository:** root
**Status:** Completed (2026-03-27)
**Created:** 2026-03-27

## Goal

Move binary builds from a separate tag-triggered workflow
into the release-plz workflow as a dependent job. The
separate workflow never fires because `GITHUB_TOKEN`-created
tags don't trigger other workflows. Inlining the build as
a dependent job that gates on the `releases_created` output
solves this without requiring additional tokens.

## Context

- `release-binaries.yml` triggers on `push: tags: rlsp-yaml-v*`
  but never fires because release-plz creates tags via
  `GITHUB_TOKEN`, and GitHub Actions does not trigger
  workflows for events created by `GITHUB_TOKEN`
- The release-plz action outputs `releases_created` (bool)
  and `releases` (JSON array with `package_name`, `version`,
  `tag` per released package)
- We can add a `build-binaries` job to `release-plz.yml`
  that depends on the release job and only runs when
  `releases_created == 'true'`
- The `softprops/action-gh-release` action needs the tag
  name to upload assets to the correct release — we extract
  it from the `releases` output
- File to modify: `.github/workflows/release-plz.yml`
- File to delete: `.github/workflows/release-binaries.yml`

## Steps

- [x] Merge binary build matrix into release-plz workflow (9d63005)
- [x] Delete the standalone release-binaries workflow (9d63005)

## Tasks

### Task 1: Inline binary builds into release-plz workflow

Modify `.github/workflows/release-plz.yml`:

1. Add a step `id` to the release-plz action step in the
   release job so its outputs are accessible
2. Add `outputs` to the release job to expose
   `releases_created` and `releases`
3. Add a `build-binaries` matrix job that:
   - `needs: release-plz-release`
   - `if: needs.release-plz-release.outputs.releases_created == 'true'`
   - Has `permissions: contents: write`
   - Uses the same build matrix from `release-binaries.yml`
     (6 targets across 3 OS)
   - Checks out at the release tag (use `ref` from releases
     output)
   - Builds, packages, and uploads to the GitHub Release
     using `softprops/action-gh-release@v2` with the
     `tag_name` set from the releases output

4. Delete `.github/workflows/release-binaries.yml`

The `softprops/action-gh-release` needs `tag_name` to
identify which release to upload to. Extract it from the
releases JSON: the tag for rlsp-yaml is at
`fromJSON(releases)[0].tag`.

## Decisions

- **Inline over workflow_run** — `workflow_run` requires
  heuristics (time-based or asset-based) to detect whether
  a release happened; inlining uses the action's direct
  outputs, which is deterministic
- **Inline over PAT** — avoids introducing a manually
  managed token just to work around a GitHub Actions
  limitation
- **Single-crate matrix for now** — the build matrix
  targets rlsp-yaml only; when a second crate is added,
  extend the matrix to include package name
