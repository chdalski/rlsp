**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-27

## Goal

Fix the tag format mismatch between release-plz and the
release-binaries workflow. release-plz creates tags like
`v0.1.0` but the binary build workflow triggers on
`rlsp-yaml-v*`. The workspace will grow to multiple crates
(e.g. rlsp-toml, rlsp-json), so the tag format must include
the package name to keep releases independent.

## Context

- `release-plz.toml` configures release-plz but does not
  set `git_tag_name`, so it defaults to `v{{ version }}`
- Existing tags: `v0.1.0`, `v0.1.1` (never triggered binaries)
- `.github/workflows/release-binaries.yml` triggers on
  `rlsp-yaml-v*` — correct for the multi-crate future
- release-plz supports `git_tag_name` at workspace level
  with `{{ package }}` and `{{ version }}` template vars
- Setting it at workspace level means future crates
  automatically get the right tag format without extra config
- Related plan: `2026-03-26-oss-3-release-automation.md`
  (completed) — original setup that introduced both workflows

## Steps

- [ ] Add `git_tag_name` to `release-plz.toml` workspace config
- [ ] Verify the format aligns with `release-binaries.yml` trigger

## Tasks

### Task 1: Configure release-plz tag format

Add `git_tag_name = "{{ package }}-v{{ version }}"` to the
`[workspace]` section in `release-plz.toml`. This produces
tags like `rlsp-yaml-v0.1.0` which match the existing
`release-binaries.yml` trigger pattern `rlsp-yaml-v*`.

Files:
- `release-plz.toml` — add the `git_tag_name` line

No test or security consultation needed — this is a
one-line configuration change with no code or trust
boundary implications.

## Decisions

- **Workspace-level over per-package** — setting
  `git_tag_name` at `[workspace]` with `{{ package }}`
  template means future crates (rlsp-toml, rlsp-json)
  automatically get correct `<name>-v<version>` tags
  without additional config
- **Keep existing workflow trigger** — `release-binaries.yml`
  already uses the correct multi-crate pattern
  `rlsp-yaml-v*`; only the tag source needs fixing
- **Orphaned tags left as-is** — `v0.1.0` and `v0.1.1`
  never triggered binaries and are harmless to leave
