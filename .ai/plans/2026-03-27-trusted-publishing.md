**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-27

## Goal

Switch the release-plz workflow from a manually managed
`CARGO_REGISTRY_TOKEN` secret to crates.io trusted
publishing (OIDC). This eliminates long-lived API tokens
and uses short-lived, cryptographically verified tokens
instead.

## Context

- crates.io supports trusted publishing via OIDC
  (see https://crates.io/docs/trusted-publishing)
- release-plz has built-in trusted publishing support —
  it handles the OIDC token exchange internally, so the
  `rust-lang/crates-io-auth-action` is not needed
- To enable: add `id-token: write` permission to the
  release job, remove `CARGO_REGISTRY_TOKEN` from env
- The release-pr job only creates PRs (no publishing),
  so it never needed `CARGO_REGISTRY_TOKEN` either —
  remove it from both jobs
- Prerequisite: the crate must already be published to
  crates.io (first publish requires an API token) and
  trusted publishing must be configured on crates.io
  for rlsp-yaml (Settings → Trusted Publishing)
- The user will configure trusted publishing on crates.io
  after the workflow change is merged
- File: `.github/workflows/release-plz.yml`

## Steps

- [ ] Update release-plz workflow for trusted publishing
- [ ] Update CLAUDE.md release section to mention trusted
      publishing

## Tasks

### Task 1: Update release-plz workflow

Modify `.github/workflows/release-plz.yml`:

1. In the `release-plz-release` job:
   - Add `id-token: write` to permissions (alongside
     existing `contents: write`)
   - Remove `CARGO_REGISTRY_TOKEN` from the env block
     (keep only `GITHUB_TOKEN`)

2. In the `release-plz-release-pr` job:
   - Remove `CARGO_REGISTRY_TOKEN` from the env block
     (keep only `GITHUB_TOKEN`) — this job only creates
     PRs, it never publishes

The result should look like:

```yaml
jobs:
  release-plz-release-pr:
    permissions:
      contents: write
      pull-requests: write
    steps:
      # ...
      env:
        GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}

  release-plz-release:
    permissions:
      contents: write
      id-token: write
    steps:
      # ...
      env:
        GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### Task 2: Update CLAUDE.md release section

Add a note about trusted publishing to the Release section
in the root `CLAUDE.md`. Mention that publishing uses OIDC
via trusted publishing (no `CARGO_REGISTRY_TOKEN` needed),
and that new crates must be published manually the first
time before trusted publishing can be configured.

## Decisions

- **release-plz built-in OIDC over crates-io-auth-action** —
  release-plz implements the OIDC exchange internally,
  adding a separate action would be redundant
- **Remove token from both jobs** — the release-pr job
  never publishes, so it never needed the token
- **No fallback to token-based auth** — trusted publishing
  is the only supported path going forward; keeping the
  token as a fallback defeats the security benefit
