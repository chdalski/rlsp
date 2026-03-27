**Repository:** root
**Status:** InProgress
**Created:** 2026-03-26

## Goal

Set up release-plz for automated release management and a
GitHub Actions workflow to build cross-platform release
binaries for all target architectures.

## Context

- Version: 0.1.0 (SemVer)
- Release tool: release-plz (creates release PRs with
  changelog from conventional commits, publishes to
  crates.io on merge)
- Target platforms for release binaries:
  - Linux: x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu,
    riscv64gc-unknown-linux-gnu
  - macOS: x86_64-apple-darwin, aarch64-apple-darwin
  - Windows: x86_64-pc-windows-msvc
- The project uses conventional commits (feat:, fix:, docs:,
  chore:, etc.) — release-plz can auto-generate changelogs
- Repository: https://github.com/cdalski/rlsp
- Binary name: rlsp-yaml

## Steps

- [x] Create release-plz configuration (94287ea)
- [x] Create release-plz GitHub Actions workflow (94287ea)
- [x] Create cross-platform binary release workflow (4ad22b1)
- [ ] Add Codecov component_management for per-crate coverage
- [ ] Document CARGO_REGISTRY_TOKEN and release process

## Tasks

### Task 1: release-plz configuration + workflow

Create `release-plz.toml` at repo root:
```toml
[workspace]
changelog_config = "cliff.toml"
publish_timeout = "10m"
git_release_enable = true

[[package]]
name = "rlsp-yaml"
```

Create `cliff.toml` for changelog generation (git-cliff
config used by release-plz):
```toml
[changelog]
header = "# Changelog\n\n"
body = """
{% for group, commits in commits | group_by(attribute="group") %}
## {{ group | upper_first }}
{% for commit in commits %}
- {{ commit.message | upper_first }} ({{ commit.id | truncate(length=7, end="") }})\
{% endfor %}
{% endfor %}
"""
trim = true

[git]
conventional_commits = true
commit_parsers = [
    { message = "^feat", group = "Features" },
    { message = "^fix", group = "Bug Fixes" },
    { message = "^perf", group = "Performance" },
    { message = "^refactor", group = "Refactoring" },
    { message = "^doc", group = "Documentation" },
]
filter_commits = true
```

Create `.github/workflows/release-plz.yml`:
- **Trigger:** push to `main`
- **Jobs:**
  - `release-plz-release-pr` — runs `release-plz release-pr`
    to create/update a release PR with version bump and
    changelog
  - `release-plz-release` — runs `release-plz release` on
    merge to publish to crates.io and create git tags
- Requires `CARGO_REGISTRY_TOKEN` secret

- [x] release-plz.toml (94287ea)
- [x] cliff.toml (94287ea)
- [x] .github/workflows/release-plz.yml (94287ea)

### Task 2: Cross-platform binary release workflow

Create `.github/workflows/release-binaries.yml`:
- **Trigger:** on tag push matching `rlsp-yaml-v*`
  (release-plz creates these tags)
- **Strategy matrix:**
  ```yaml
  matrix:
    include:
      - target: x86_64-unknown-linux-gnu
        os: ubuntu-latest
      - target: aarch64-unknown-linux-gnu
        os: ubuntu-latest
      - target: riscv64gc-unknown-linux-gnu
        os: ubuntu-latest
      - target: x86_64-apple-darwin
        os: macos-latest
      - target: aarch64-apple-darwin
        os: macos-latest
      - target: x86_64-pc-windows-msvc
        os: windows-latest
  ```
- **Steps:**
  - Checkout, install Rust toolchain with target
  - For Linux cross-compilation (aarch64, riscv64): use
    `cross` or install cross-compilation toolchains
  - Build with `cargo build --release --target ${{ matrix.target }}`
  - Package binary as tar.gz (Linux/macOS) or zip (Windows)
  - Upload to GitHub Release using `softprops/action-gh-release@v2`
- Binary naming: `rlsp-yaml-{target}.tar.gz` / `.zip`

- [x] release-binaries.yml (4ad22b1)
- [x] Verify matrix covers all 6 targets (4ad22b1)
- [x] Archive naming convention documented (4ad22b1)

### Task 3: Codecov per-crate components

Add `component_management` to `codecov.yml` so each workspace
crate gets its own coverage breakdown in the Codecov UI. Uses
path-based filtering — no CI changes needed since the existing
coverage workflow already uploads a single workspace-wide report.

```yaml
component_management:
  individual_components:
    - component_id: rlsp-yaml
      paths:
        - rlsp-yaml/**
```

- [ ] Add component_management section to codecov.yml

## Decisions

- **release-plz over cargo-release** — fully automated via
  CI, creates release PRs with changelogs, publishes on
  merge. Fits the AI-driven workflow where the maintainer
  reviews and merges.
- **git-cliff for changelogs** — release-plz uses git-cliff
  internally. Conventional commit parsing generates
  categorized changelogs automatically.
- **cross for Linux cross-compilation** — Docker-based
  cross-compilation avoids installing cross-toolchains
  natively on the runner. Native compilation for macOS
  and Windows where runners match the target.
- **Tag-triggered binary builds** — decoupled from
  crates.io publish. release-plz creates the tag, which
  triggers binary builds. This ensures binaries are only
  built for actual releases.
