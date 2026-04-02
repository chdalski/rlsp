**Repository:** root
**Status:** InProgress
**Created:** 2026-04-02

## Goal

Add version synchronization and manual dispatch to the VS Code extension
CI workflow. The extension version in `package.json` should automatically
match the crate release tag. A `workflow_dispatch` trigger allows
extension-only publishes without a crate release. The publish job must be
disabled until a `VSCE_PAT` secret is configured.

## Context

- Workflow at `.github/workflows/vscode-extension.yml`
- Extension version is hardcoded at `0.1.0` in `package.json` — never
  synced from the release tag
- No way to trigger an extension-only publish currently
- User does not have a `VSCE_PAT` yet — publish job must be disabled
  to prevent CI failures on tag pushes
- Build job on push to main should remain active for CI validation

## Steps

- [x] Add version sync, workflow_dispatch, and disable publish job (a188d65)

## Tasks

### Task 1: Version sync, workflow_dispatch, and disable publish

Changes to `.github/workflows/vscode-extension.yml`:

- [x] Add `workflow_dispatch` trigger with a `version` input:
  ```yaml
  workflow_dispatch:
    inputs:
      version:
        description: 'Extension version to publish (e.g., 0.5.1)'
        required: true
  ```
- [x] Add a version-sync step in `build-extension` job before packaging:
  ```yaml
  - name: Set extension version
    working-directory: rlsp-yaml/integrations/vscode
    run: |
      if [ -n "${{ inputs.version }}" ]; then
        VERSION="${{ inputs.version }}"
      else
        VERSION="${GITHUB_REF_NAME#rlsp-yaml-v}"
      fi
      pnpm pkg set version="$VERSION"
  ```
  On Windows, use PowerShell equivalent.
- [x] Disable the `publish-extension` job by adding `if: false` (temporary
  until `VSCE_PAT` is configured). Add a comment explaining why.
- [x] Update the publish job's `if` condition to also trigger on
  `workflow_dispatch` (for when it's re-enabled):
  ```yaml
  if: false  # Disabled until VSCE_PAT secret is configured
  # Original condition (re-enable when ready):
  # if: startsWith(github.ref, 'refs/tags/rlsp-yaml-v') || github.event_name == 'workflow_dispatch'
  ```

## Decisions

- **Version sync:** extract from tag (`rlsp-yaml-v0.5.0` → `0.5.0`) or
  from `workflow_dispatch` input — keeps extension version in lockstep
  with crate version for tag-triggered builds, independent for manual
  dispatches
- **Disable publish:** `if: false` with a comment — minimal change,
  easy to re-enable by removing one line
- **`package.json` version stays at `0.1.0`** in git — it's overwritten
  at build time, so the committed value is a placeholder
