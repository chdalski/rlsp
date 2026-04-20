**Repository:** root
**Status:** Completed (2026-04-04)
**Created:** 2026-04-04

## Goal

Switch the VS Code extension versioning from semver to CalVer
(`YYYY.MM.NN`). Tag-triggered builds default to `YYYY.MM.0`,
manual `workflow_dispatch` accepts the full version. This decouples
the extension version from the crate version while keeping it
simple and predictable.

## Context

- Current version-sync step extracts version from the crate tag
  (`rlsp-yaml-v0.5.0` → `0.5.0`) or from `workflow_dispatch` input
- New scheme: `YYYY.MM.NN` — e.g., `2026.4.0`, `2026.4.1`
- For tag-triggered builds: auto-generate `YYYY.MM.0` from the
  current date
- For `workflow_dispatch`: user provides the full version (e.g.,
  `2026.4.1` for a second release in the same month)
- `package.json` version in git stays as a placeholder — overwritten
  at build time

## Steps

- [x] Update version-sync steps in CI workflow (55ca46e)

## Tasks

### Task 1: Switch to CalVer in vscode-extension.yml

Update `.github/workflows/vscode-extension.yml` to add a
`resolve-version` job and simplify the version-sync steps:

- [x] Change the `workflow_dispatch` input: make `version` optional,
  add description reflecting CalVer format `YYYY.MM.NN`
- [x] Replace the two platform-specific version-sync steps with a
  single step that: queries `vscode-v*` tags for the current month,
  auto-increments `NN`, sets the version, and creates + pushes a
  `vscode-v{version}` tag. If `workflow_dispatch` provides a version,
  use it directly instead of auto-incrementing.
- [x] Update checkout step to `fetch-depth: 0` (or fetch tags) so
  `git tag -l` can find existing tags
- [x] Update `build-extension` job permissions to `contents: write`
  (needed to push the tag)
- [x] Add a `resolve-version` job that runs before `build-extension`:
  - Runs on `ubuntu-latest`, `contents: write` permissions
  - Checkout with `fetch-depth: 0` (needs all tags)
  - If `workflow_dispatch` with version input: use it directly
  - Otherwise: compute `YYYY.MM.NN` from `vscode-v*` tags
  - Create and push the `vscode-v{version}` tag
  - Output the version string for downstream jobs
  - Only runs on tags or dispatch (same `if` as version-sync had)
- [x] Update `build-extension` to depend on `resolve-version` (for
  tag/dispatch triggers) and consume its version output
- [x] Replace the two platform-specific version-sync steps with a
  single cross-platform step that reads the version from the
  `resolve-version` job output and sets it via `pnpm pkg set`
- [x] `build-extension` permissions can stay at `contents: read`
  (tag push is in `resolve-version`)
- [x] Keep `build-extension` runnable without `resolve-version` on
  plain push to main (no version sync needed for CI-only builds)

## Decisions

- **Format:** `YYYY.MM.NN` — unambiguous, sorts correctly, reads
  naturally. No day component (MMDD) — monthly granularity is
  sufficient
- **`NN` management:** auto-incremented from existing `vscode-v*` git
  tags. Query `vscode-v{YYYY.MM}.*`, extract highest `NN`, increment.
  Falls back to `0` if no tag exists for the current month.
- **Version store:** git tags (`vscode-v2026.4.0`) — no external API
  dependency, serves as release audit trail
- **Tag permissions:** `contents: write` on `build-extension` job to
  push the version tag. Only one matrix entry should push the tag to
  avoid race conditions — gate on a specific runner (e.g., linux-x64)
