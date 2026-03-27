**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-27

## Goal

Fix GitHub code scanning alerts #1, #2, and #3 — all
`actions/missing-workflow-permissions`. Both `ci.yml` and
`coverage.yml` lack explicit `permissions` declarations,
meaning they run with default token permissions that may be
broader than needed. Adding least-privilege permissions
hardens the CI pipeline.

## Context

- Three open alerts on `chdalski/rlsp` code scanning, all
  the same rule: `actions/missing-workflow-permissions`
- Alert #2: `ci.yml:14` (check job), Alert #3: `ci.yml:28`
  (test job), Alert #1: `coverage.yml:14` (coverage job)
- Both workflows only need read access to checkout code and
  run cargo commands (fmt, clippy, test, coverage)
- The coverage workflow uploads to Codecov via a secret
  token (`CODECOV_TOKEN`), not via GitHub permissions
- `release-plz.yml` already has per-job permissions and is
  not flagged
- Fix: add a top-level `permissions: contents: read` block
  to each workflow file — this applies to all jobs in the
  workflow and follows least-privilege principle

### Key files

- `.github/workflows/ci.yml` — format + clippy + test
- `.github/workflows/coverage.yml` — code coverage + Codecov upload

## Steps

- [x] Identify affected workflows and required permissions
- [x] Confirm `release-plz.yml` already has permissions
- [ ] Add `permissions: contents: read` to `ci.yml`
- [ ] Add `permissions: contents: read` to `coverage.yml`

## Tasks

### Task 1: Add workflow permissions to ci.yml and coverage.yml

Add a top-level `permissions: contents: read` block to both
workflow files, placed between the `on:` trigger block and
the `env:` / `jobs:` block (consistent with YAML workflow
conventions).

- [ ] `ci.yml`: add `permissions:` block after `on:` block
- [ ] `coverage.yml`: add `permissions:` block after `on:` block

## Decisions

- **Top-level vs per-job permissions:** Top-level, because
  all jobs in both workflows need the same permission
  (`contents: read`). Per-job would add duplication with no
  benefit.
- **`contents: read` only:** Neither workflow writes to the
  repo, creates releases, or interacts with PRs/issues.
  Read access is sufficient.
