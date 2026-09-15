**Repository:** root
**Status:** Completed (2026-09-08)
**Created:** 2026-09-08

# Bring the Zed extension crate under Dependabot

## Goal

The Zed extension crate declares its own `[workspace]` and therefore sits
outside the root Cargo workspace, so the existing `cargo` entry in
`.github/dependabot.yml` (which points at `/`) never reaches it — its
dependencies have received no automated update PRs since the extension landed.
Add a fourth `cargo` update entry covering the extension's manifest directory
so its `Cargo.toml` and `Cargo.lock` get the same weekly grouped update flow
the rest of the repository already has.

## Context

- **Current config.** `.github/dependabot.yml` has three `updates` entries:
  `cargo` at `/`, `github-actions` at `/`, and `npm` at
  `/rlsp-yaml/integrations/vscode`. Every entry uses a weekly schedule and
  collapses its updates into a single named group with `patterns: ["*"]`. Two
  entries carry an `ignore` block, each preceded by a comment explaining why
  the exclusion exists. The new entry follows this shape.
- **Why the crate is outside the workspace.**
  `rlsp-yaml/integrations/zed/Cargo.toml` declares an empty `[workspace]`
  table. The extension is a `cdylib` compiled to `wasm32-wasip2` against
  `zed_extension_api`, and it releases on its own cadence (`zed-v<semver>`
  tags, version tracked in `extension.toml`) — it is deliberately not a
  workspace member. Dependabot's cargo updater resolves per manifest
  directory, so a second `cargo` entry is the only way to cover it.
- **Lockfile is tracked.** `rlsp-yaml/integrations/zed/Cargo.lock` is committed,
  so Dependabot can update both the manifest requirement and the lock. Current
  dependencies: `zed_extension_api = "0.7"`, `serde_json = "1"`, and the
  `rstest = "0.26"` dev-dependency.
- **CI already gates the directory.** `.github/workflows/zed-release.yml`
  triggers on `pull_request` and `push` filtered to
  `rlsp-yaml/integrations/zed/**`, and its `check` job runs `cargo check` and
  `cargo clippy --all-targets -- -D warnings` against `wasm32-wasip2` with
  `permissions: contents: read`. Every Dependabot PR in this directory is
  therefore compiled and linted before merge. The version-bump, tag, and
  registry-PR jobs in that workflow are all guarded by
  `if: github.event_name == 'workflow_dispatch'`, so merging a dependency bump
  to `main` does not trigger a Zed extension release.
- **MSRV is not a Dependabot input.** The crate sets `rust-version = "1.97"`
  and CI pins `dtolnay/rust-toolchain@1.97.1`. Dependabot does not consider a
  crate's MSRV when proposing updates, so a dependency that requires a newer
  toolchain shows up as a failing check on the PR rather than being filtered
  out at config level.
- **User decision — no ignore rules.** `zed_extension_api` stays fully
  automated. Note that a `update-types: ["version-update:semver-major"]`
  filter would not exclude it in any case: Dependabot reads `0.7 → 0.8` as a
  *minor* update, not a major one. A breaking API bump surfaces as a red
  `Check + Clippy` job on the Dependabot PR.
- **Reference.** [Dependabot options
  reference](https://docs.github.com/en/code-security/dependabot/working-with-dependabot/dependabot-options-reference)
  — authoritative list of valid keys for an `updates` entry.

## Steps

- [x] Clarify scope and `zed_extension_api` handling with the user
- [x] Confirm the crate is outside the workspace and its lockfile is tracked
- [x] Confirm CI gates pull requests touching the Zed extension directory
- [x] Add the `cargo` entry for the Zed extension directory
- [x] Verify the config parses and every key in the new entry is valid

## Tasks

### Task 1: Cover the Zed extension crate with Dependabot

Add a fourth `cargo` update entry to `.github/dependabot.yml` pointed at the
Zed extension's manifest directory, so its dependencies receive the same
weekly grouped updates as the root workspace, GitHub Actions, and the VS Code
extension.

- [x] `.github/dependabot.yml` contains a `cargo` entry whose directory is the
      Zed extension's manifest directory, on the same schedule interval as the
      three existing entries
- [x] The entry's updates arrive as a single group whose name follows the
      naming convention of the existing groups
- [x] The entry carries no `ignore` rules
- [x] A comment on the entry states why the Zed crate needs an entry of its
      own rather than being covered by the `cargo` entry at `/`
- [x] The file parses as valid YAML, and every key used in the new entry is
      one the Dependabot options reference defines for an `updates` entry
- [x] The three existing entries — including their groups and their commented
      `ignore` rules — are byte-for-byte unchanged
- [x] The directory named in the new entry is the one containing the Zed
      extension's `Cargo.toml` and its committed `Cargo.lock`

## Decisions

- **No `ignore` rules on the new entry** (user decision) — `zed_extension_api`
  bumps flow through as ordinary PRs; the `Check + Clippy` job on
  `wasm32-wasip2` is the gate that catches a breaking API change. An ignore
  can be added later if the PRs prove noisy. This differs from the
  `dtolnay/rust-toolchain` and `typescript` ignores, which exist because those
  updates are *predictably* wrong, not merely potentially breaking.
- **Group name `zed-extension-dependencies`** — mirrors
  `vscode-extension-dependencies`, so a grouped PR title identifies which
  component it targets.
- **Separate entry rather than folding the crate into the workspace** — the
  standalone workspace is intentional (wasm target, independent release
  cadence). The Dependabot config adapts to the crate layout, not the reverse.

## Non-Goals

- Dependabot coverage for `rlsp-yaml/integrations/claude-code` — it has no
  package manifest to update.
- Tuning the three existing entries (`ignore`, `cooldown`,
  `open-pull-requests-limit`, `versioning-strategy`, schedule changes).
- Updating any Zed dependency or regenerating
  `rlsp-yaml/integrations/zed/Cargo.lock` — this plan only establishes the
  automation; the first bump arrives as a Dependabot PR.
- Changes to `zed-release.yml` or any other workflow, including auto-merge
  automation for Dependabot PRs.
