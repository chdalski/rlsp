**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-27

## Goal

Fix the `build-binaries` job in `release-plz.yml` so it only
builds and uploads binary assets for crates that produce
binaries. Currently it triggers on any release (including
library-only crates like `rlsp-fmt`) and uploads `rlsp-yaml`
binaries to the wrong release tag.

## Context

- **Problem:** The `build-binaries` job unconditionally uses
  `releases[0].tag` to check out and upload. When `rlsp-fmt`
  releases alone, the job builds `rlsp-yaml` binaries and
  attaches them to the `rlsp-fmt` release — wrong on both counts.
- **Scaling requirement:** The solution must work when future
  binary crates are added (e.g. `rlsp-json`). Hard-coding
  `rlsp-yaml` as the only binary crate would break when a second
  LSP is added.
- **release-plz outputs:** The `releases` output is a JSON array
  of objects with `package_name`, `tag`, `version`, etc. We can
  iterate over this to find which releases have binary crates.
- **Key file:** `.github/workflows/release-plz.yml`

## Steps

- [x] Analyze current workflow and identify the bug
- [x] Review release-plz output format
- [ ] Redesign build-binaries to iterate over released binary crates
- [ ] Ensure library-only crates skip binary builds
- [ ] Test with a dry-run or validation

## Tasks

### Task 1: Filter and iterate binary releases in workflow

Redesign the `build-binaries` job to dynamically determine which
released crates are binaries and build only for those. The
approach: add a job that filters `releases` output to only crates
that have binary targets, then matrix over both the filtered
crates and the target platforms. Library crates like `rlsp-fmt`
produce no matrix entries and skip the build entirely.

- [ ] Add a `filter-binaries` job that parses the `releases`
      JSON and outputs only binary crate names + tags
- [ ] Update `build-binaries` to matrix over the filtered list
      combined with the target platforms
- [ ] Each matrix entry checks out the correct tag, builds the
      correct binary, packages with the correct name, and uploads
      to the correct release
- [ ] When no binary crates are released, the matrix is empty
      and the job is skipped
- [ ] Validate with `act` or manual review

## Decisions

- **Dynamic filtering over hard-coded allow-list:** A hard-coded
  list of binary crates would need updating every time a new LSP
  is added. Instead, detect binary crates from workspace metadata
  (`cargo metadata`) or maintain a list in the matrix that maps
  crate names to their binary targets. The latter is simpler and
  more explicit — the matrix already lists targets, so adding a
  crate dimension is natural.
