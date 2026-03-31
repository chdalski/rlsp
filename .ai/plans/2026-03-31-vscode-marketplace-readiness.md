**Repository:** root
**Status:** Completed (2026-03-31)
**Created:** 2026-03-31

## Goal

Make the VS Code extension marketplace-ready: add an extension README for
the marketplace landing page, update the publisher ID from `chdalski` to
`chrisski`, and add a publish step to the CI workflow that publishes to
the marketplace on release tags using a `VSCE_PAT` secret.

## Context

- Extension lives at `rlsp-yaml/editors/code/`
- Current `package.json` has `publisher: "chdalski"` — needs to be `"chrisski"`
- No `README.md` exists in the extension directory — marketplace requires one
- CI workflow at `.github/workflows/vscode-extension.yml` builds VSIX artifacts
  but does not publish to the marketplace
- Publishing requires a `VSCE_PAT` secret (Azure DevOps PAT with Marketplace
  scope) — the user will create this manually on GitHub
- The user owns `chrisski.dev` and will use it as verified publisher domain

## Steps

- [x] Update publisher ID and add extension README — `834538e`
- [x] Add marketplace publish step to CI workflow — `027c51e`

## Tasks

### Task 1: Publisher ID update, extension README

- [ ] Update `package.json` publisher from `"chdalski"` to `"chrisski"`
- [ ] Create `rlsp-yaml/editors/code/README.md` — marketplace-focused:
  - Extension name and one-line description
  - Features list (matching rlsp-yaml capabilities)
  - Installation instructions (marketplace install)
  - Quick configuration overview with link to full docs
  - Screenshot placeholder section
- [ ] Update `.vscodeignore` if needed to ensure README.md is included in VSIX

### Task 2: Add publish job to CI workflow

- [ ] Add `publish-extension` job to `.github/workflows/vscode-extension.yml`:
  - Runs after `build-extension` completes, only on release tags (`rlsp-yaml-v*`)
  - Downloads all VSIX artifacts from build job
  - Publishes each platform VSIX using `pnpx @vscode/vsce publish --packagePath <vsix>`
  - Uses `VSCE_PAT` secret for authentication
  - Explicit permissions (minimal)
- [ ] Add `id-token: write` if needed for OIDC, or just `contents: read`

## Decisions

- **Publisher:** `chrisski` — user's verified domain publisher on marketplace
- **Secret:** `VSCE_PAT` — standard vsce convention, user creates manually
- **Publish trigger:** release tags only — not on every push to main
