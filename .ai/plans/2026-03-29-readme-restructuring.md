**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-29

## Goal

Restructure the project's documentation so the root README
is a project landing page that scales to multiple language
servers, each language server's README is self-contained
(installation, editor setup, features, configuration link),
and configuration.md is a pure settings reference.

## Context

- Three docs overlap today: `README.md`, `rlsp-yaml/README.md`,
  `rlsp-yaml/docs/configuration.md`
- Root README duplicates installation, features, and editor
  setup from the crate README
- Editor setup lives in `configuration.md` but is usage
  guidance, not configuration reference
- Structure must scale when future language servers are added
  (e.g. rlsp-toml)
- Files involved:
  - `README.md` (root)
  - `rlsp-yaml/README.md`
  - `rlsp-yaml/docs/configuration.md`
  - `CLAUDE.md` (add documentation layout subsection)

## Steps

- [x] Clarify requirements with user
- [ ] Slim down root README to project overview + crates table
- [ ] Move editor setup from configuration.md to rlsp-yaml/README.md
- [ ] Add installation section to rlsp-yaml/README.md
- [ ] Remove editor setup from configuration.md
- [ ] Add documentation layout subsection to CLAUDE.md
- [ ] Verify all cross-links are correct

## Tasks

### Task 1: Restructure all three docs

This is a single coordinated change across three files —
splitting it would leave broken cross-references between
commits.

**Root README.md** — rewrite to:
- Project description (what RLSP is, AI-written, badges)
- Crates table with links to each server's README
- Contributing section (link to CONTRIBUTING.md)
- License

Remove: installation, features summary, editor setup (all
move to crate README).

**rlsp-yaml/README.md** — reorganize to:
- Description (what it is)
- Installation (cargo install + prebuilt binaries)
- Editor Setup (move from configuration.md: Neovim, VS Code,
  Helix, Zed with full config examples)
- Features (keep existing)
- Configuration (brief intro + link to docs/configuration.md)
- Architecture (keep existing)
- Building (keep existing)
- License

**rlsp-yaml/docs/configuration.md** — remove the Editor
Setup section (lines 208-294). Keep everything else:
workspace settings, modelines, validators, formatting,
schema resolution, schema fetching.

- [ ] Rewrite root README.md
- [ ] Reorganize rlsp-yaml/README.md
- [ ] Remove editor setup from configuration.md
- [ ] Add `### Documentation Layout` subsection under
  `## Repository Layout` in CLAUDE.md — describe the intent:
  root README is the landing page (project overview + crate
  links), each crate README is self-contained for users
  (installation, editor setup, features, config link),
  `docs/configuration.md` is pure settings reference
- [ ] Verify cross-links between all four files

## Decisions

- **Single task, not three:** the three files cross-reference
  each other; splitting into separate commits risks broken
  links between commits.
- **Editor setup in crate README, not configuration.md:**
  editor setup is "how to use this server" — it belongs
  where users land first (the crate README), not in a
  settings reference document.
- **Root README as landing page:** keeps the root doc
  stable when adding new language servers — just add a row
  to the crates table.
