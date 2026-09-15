**Repository:** root
**Status:** Completed (2026-05-13)
**Created:** 2026-05-13

## Goal

Add a Zed editor extension for rlsp-yaml at
`rlsp-yaml/integrations/zed/`, with fully automated
release pipeline. After one-time setup (PAT creation and
initial registry PR), subsequent `rlsp-yaml` crate releases
automatically bump the Zed extension version, commit, tag,
and open a PR against the `zed-industries/extensions`
registry.

## Context

- **GitHub issue:** #36 — community request with sample
  implementation at https://github.com/cyrillsemenov/rlsp
- **Existing pattern:** VS Code extension lives at
  `rlsp-yaml/integrations/vscode/` with its own
  `package.json`, TypeScript source, and CI workflow
- **Zed extension model:** Rust compiled to WASM
  (`wasm32-wasip2` target, `cdylib` crate type). Extensions
  run sandboxed via Wasmtime. They implement the
  `zed::Extension` trait and return a `Command` struct —
  Zed spawns the LSP process outside the sandbox.
- **Zed registry:** Extensions are published via PR to
  `zed-industries/extensions`. The registry supports
  in-repo subdirectories via a `path` field in
  `extensions.toml` (52 of 1,174 extensions use this
  pattern). Registry CI builds the WASM — we don't.
- **Binary distribution:** The extension downloads the
  `rlsp-yaml` binary from GitHub Releases at runtime via
  `zed::latest_github_release()`. Cannot bundle binaries.
  Falls back to PATH lookup via `worktree.which()`.
- **Settings:** Zed has no extension-defined settings
  schema. Users configure LSP settings as raw JSON in
  their Zed `settings.json`. The extension uses
  pass-through only — forwards whatever the user sets,
  server applies its own defaults for missing keys.
- **Release flow:** release-plz creates GitHub Releases
  for crate versions. The existing `trigger-vscode` job
  in `release-plz.yml` checks if `rlsp-yaml` was released
  and dispatches the VS Code extension workflow. The Zed
  trigger follows the same pattern but targets a
  `zed-extension.yml` workflow.
- **Auth:** A classic PAT with `public_repo` scope stored
  as `ZED_REGISTRY_PAT` is needed for cross-repo PRs to
  `zed-industries/extensions`.
- **Zed extension API:** `zed_extension_api = "0.7"`,
  target `wasm32-wasip2`
- **Sample implementation reference:**
  https://github.com/cyrillsemenov/rlsp — provides the
  basic structure but needs adjustments (pass-through
  settings instead of hardcoded defaults, version cleanup,
  old-version directory cleanup)
- **Specifications:**
  - [Zed extension development docs](https://zed.dev/docs/extensions/developing-extensions)
  - [zed_extension_api on docs.rs](https://docs.rs/zed_extension_api)
  - [zed-industries/extensions registry](https://github.com/zed-industries/extensions)

## Steps

- [x] Clarify requirements with user
- [x] Create Zed extension crate
- [x] Add CI workflow for Zed extension
- [x] Add release automation (trigger job + release workflow)
- [x] Update documentation (CLAUDE.md, rlsp-yaml README)

## Tasks

### Task 1: Create Zed extension crate

Create the extension at `rlsp-yaml/integrations/zed/` as a
standalone Rust crate (not part of the workspace — declares
its own `[workspace]` in `Cargo.toml`, same pattern as the
VS Code extension being outside the Cargo workspace).

**Completed:** commit `303b839`

- [x] `extension.toml` — manifest with `schema_version = 1`,
  `id = "rlsp-yaml"`, SemVer version `0.1.0`,
  authors `["Christoph Dalski", "Kirill Semenov"]`,
  repo `https://github.com/chdalski/rlsp`,
  language server entry for YAML
- [x] `Cargo.toml` — standalone crate `rlsp-yaml-zed`,
  `cdylib` crate type, deps: `zed_extension_api = "0.7"`,
  `serde_json = "1"`. Declares `[workspace]` to stay
  isolated from root workspace. License `MIT`.
- [x] `src/lib.rs` — implements `zed::Extension` trait:
  - `new()` — returns `Self`
  - `language_server_command()` — PATH lookup via
    `worktree.which("rlsp-yaml")`, then GitHub release
    download via `zed::latest_github_release("chdalski/rlsp")`
    with platform detection, archive extraction, and
    `make_file_executable`. Cleans up old version
    directories after successful download.
  - `language_server_initialization_options()` — reads
    `LspSettings::for_worktree()` and passes through
    user settings. Returns `None` when user has no config
    (server applies its own defaults).
  - `language_server_workspace_configuration()` — same
    pass-through pattern for workspace config.
  - `platform_target()` helper — maps `(Os, Architecture)`
    to Rust target triples (5 supported:
    x86_64/aarch64 linux-gnu, x86_64/aarch64 apple-darwin,
    x86_64 windows-msvc)
- [x] `LICENSE` file — copy of root MIT license (required
  by Zed registry for subdirectory extensions)
- [x] `README.md` — user-facing extension documentation:
  installation from Zed marketplace, configuration via
  `settings.json`, link to `docs/configuration.md` for
  full settings reference

**Acceptance criteria:**
- `cargo check --manifest-path rlsp-yaml/integrations/zed/Cargo.toml --target wasm32-wasip2` succeeds
- `cargo clippy --manifest-path rlsp-yaml/integrations/zed/Cargo.toml --target wasm32-wasip2` reports zero warnings
- Extension crate is not listed in root `Cargo.toml` workspace members
- `lib.rs` does not hardcode initialization option defaults — only passes through user settings

### Task 2: Add CI workflow

Create `.github/workflows/zed-extension.yml` for
check/lint of the Zed extension crate on PRs and pushes.

**Completed:** commit `243ddd3`

- [x] Workflow triggers: push to main + PRs when files
  under `rlsp-yaml/integrations/zed/**` change
- [x] Job `check`: install `wasm32-wasip2` target via
  `rustup target add`, run `cargo check` and
  `cargo clippy --all-targets` targeting `wasm32-wasip2`,
  using `--manifest-path rlsp-yaml/integrations/zed/Cargo.toml`
- [x] Explicit `permissions: contents: read`
- [x] Uses latest stable action versions (`actions/checkout@v6`,
  `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`)

**Acceptance criteria:**
- Workflow file exists at `.github/workflows/zed-extension.yml`
- Workflow has explicit permissions block
- Workflow uses `wasm32-wasip2` target (not `wasip1`)
- Path filter limits triggers to `rlsp-yaml/integrations/zed/**`

### Task 3: Add release automation

**Completed:** commit `0834ba4`

Wire the Zed extension into the release pipeline with two
changes:

**A) Add `trigger-zed` job to `release-plz.yml`:**
- Parallel to existing `trigger-vscode` job
- Same pattern: depends on `release-plz-release`, checks
  if `rlsp-yaml` was released via jq filter, dispatches
  `zed-extension.yml` with `workflow_dispatch`
- Permissions: `actions: write`, `contents: read`

**B) Add release job to `zed-extension.yml`:**
- `workflow_dispatch` trigger (no inputs needed — version
  is computed automatically)
- `release` job (runs only on `workflow_dispatch`, not on
  push/PR):
  1. Compute next SemVer patch version by reading current
     `extension.toml` version and incrementing patch
  2. Update version in `extension.toml`
  3. Commit: `chore(zed): bump extension to v<version>`
  4. Tag: `zed-v<version>`
  5. Push commit + tag
  6. Fork `zed-industries/extensions` (if not already
     forked), update the extension's version field in
     `extensions.toml` to match the new version, commit,
     and open a PR against the registry
- Uses `ZED_REGISTRY_PAT` secret for the cross-repo PR
  step
- Permissions: `contents: write` for commit/tag/push

- [x] `trigger-zed` job added to `release-plz.yml`
- [x] `release` job added to `zed-extension.yml` with
  version bump, commit, tag, push
- [x] Registry PR step using `ZED_REGISTRY_PAT`
- [x] `workflow_dispatch` trigger added to
  `zed-extension.yml`
- [x] Release job is conditional on `workflow_dispatch`
  event (CI checks still run on push/PR without releasing)

**Acceptance criteria:**
- `trigger-zed` job in `release-plz.yml` follows the
  exact same pattern as `trigger-vscode` (same jq filter,
  same conditional structure)
- `zed-extension.yml` has both CI path (push/PR → check)
  and release path (workflow_dispatch → version bump +
  tag + registry PR)
- Release job only runs on `workflow_dispatch`, not on
  push/PR triggers
- All jobs have explicit permissions blocks
- `ZED_REGISTRY_PAT` secret is referenced only in the
  registry PR step

### Task 4: Update documentation

**Completed:** commit `781d7e4`

Update project documentation to reflect the new Zed
integration.

- [x] Root `CLAUDE.md` — add
  `rlsp-yaml/integrations/zed/` row to Components table
- [x] Root `CLAUDE.md` — add Zed extension tag format to
  Conventions: `zed-v<semver>` for Zed extension releases
- [x] `rlsp-yaml/README.md` — update Zed section to
  reference the extension (install from marketplace) while
  keeping the manual config as alternative. Follow the
  same pattern as the VS Code section.
- [x] Root `CLAUDE.md` — add `### Zed Extension` subsection
  to Build and Test with `cargo check` / `cargo clippy`
  commands for `wasm32-wasip2` target, parallel to the
  existing `### VS Code Extension` subsection
- [x] Root `README.md` — rename "VS Code Extension" section
  to "Editor Extensions" (or similar) and add a Zed entry
  alongside the VS Code entry
- [x] `rlsp-yaml/docs/feature-log.md` — add entry for Zed
  extension (user-facing feature)

**Acceptance criteria:**
- Components table has 5 entries (was 4)
- Conventions section mentions `zed-v<semver>` tag format
- Build and Test section has a `### Zed Extension`
  subsection with `cargo check` and `cargo clippy` commands
- Zed section in `rlsp-yaml/README.md` includes extension
  install instructions and retains the manual LSP config
  block as an alternative
- Root `README.md` references the Zed extension alongside
  VS Code
- `feature-log.md` has a Zed extension entry

## Non-Goals

- Initial Zed registry submission — the user must submit
  the first PR to `zed-industries/extensions` manually to
  register the extension. The automation handles all
  subsequent version updates.
- Tree-sitter grammar or language configuration — Zed has
  built-in YAML support; the extension only adds the
  language server.
- Settings schema UI — Zed does not support
  extension-defined settings schemas. Users configure
  via raw JSON.

## Decisions

- **Pass-through settings:** Extension does not hardcode
  initialization option defaults. Server handles its own
  defaults. This eliminates a sync point — new server
  options work without extension updates.
- **In-repo subdirectory:** Extension lives at
  `rlsp-yaml/integrations/zed/` using the registry's
  `path` field. Avoids maintaining a separate repo.
- **Standalone workspace:** The Zed crate declares its own
  `[workspace]` and is not added to root `Cargo.toml`
  members. It targets `wasm32-wasip2` which is
  incompatible with the host-target workspace builds.
- **SemVer + auto-bump:** Extension uses SemVer (Zed
  requirement), auto-increments patch on each release.
  Tag format: `zed-v<semver>`.
- **Classic PAT with `public_repo`:** Required for
  cross-repo PRs to `zed-industries/extensions`. Stored
  as `ZED_REGISTRY_PAT` secret.
- **Both authors attributed:** `extension.toml` lists both
  Christoph Dalski and Kirill Semenov (original sample
  implementation author from issue #36).
- **Based on community sample:** Implementation draws from
  the sample at https://github.com/cyrillsemenov/rlsp
  with modifications (pass-through settings, old-version
  cleanup, adjusted to current conventions).
