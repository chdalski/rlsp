**Repository:** root
**Status:** Completed (2026-04-18)
**Created:** 2026-04-18

## Goal

`rlsp-yaml-parser/Cargo.toml` is missing the `homepage`,
`keywords`, and `categories` crates.io metadata fields
that its sibling crates (`rlsp-yaml`, `rlsp-fmt`) already
publish. Add these three fields so the parser appears in
the same discoverability categories and links back to the
workspace GitHub page when published to crates.io, on par
with the sibling crates.

## Context

- Sibling crate metadata (current state on `main`):
  - `rlsp-yaml/Cargo.toml` has
    `homepage = "https://github.com/chdalski/rlsp"`,
    `keywords = ["yaml", "lsp", "language-server"]`,
    `categories = ["development-tools", "text-editors"]`.
  - `rlsp-fmt/Cargo.toml` has
    `homepage = "https://github.com/chdalski/rlsp"`,
    `keywords = ["formatter", "pretty-print", "wadler-lindig"]`,
    `categories = ["development-tools", "text-editors"]`.
  - `rlsp-yaml-parser/Cargo.toml` has no `homepage`,
    no `keywords`, no `categories`.
- `rlsp-yaml-parser` is a workspace member (see
  `Cargo.toml` `members = ["rlsp-fmt", "rlsp-yaml", "rlsp-yaml-parser"]`)
  and already inherits `license`, `authors`, `repository`
  from `[workspace.package]`.
- The original OSS-1 packaging-metadata plan
  (`.ai/plans/2026-03-26-oss-1-packaging-metadata.md`,
  Completed 2026-03-26) added full crates.io metadata to
  `rlsp-yaml`, but the parser was extracted into its own
  crate afterwards and never received the same metadata
  fields.
- The crate has been published previously (tags show
  `rlsp-yaml-parser-v0.*` releases via release-plz), so
  this change will surface on the next published version
  — there is no need to gate the change behind a release
  step.
- `keywords` and `categories` values were chosen by the
  user during clarification:
  - `keywords = ["yaml", "parser", "streaming"]`
  - `categories = ["parser-implementations"]`
  - `homepage = "https://github.com/chdalski/rlsp"`
- `parser-implementations` is a valid crates.io category
  slug (the canonical slug for libraries that parse a
  specific format).

## Steps

- [x] Add `homepage`, `keywords`, `categories` to
      `rlsp-yaml-parser/Cargo.toml`.
- [x] Verify with `cargo package --list --package
      rlsp-yaml-parser` (dry-run) that the packaged
      manifest contains the new fields and no crates.io
      warnings are emitted.
- [x] Verify `cargo fmt`, `cargo clippy --all-targets`,
      and `cargo build` still succeed for the workspace.

## Tasks

### Task 1: Add homepage, keywords, categories to rlsp-yaml-parser/Cargo.toml

**Committed:** `bf7a8330492a0126678a3412e7a4efc095c6fb61`

Add the three missing crates.io metadata fields to
`rlsp-yaml-parser/Cargo.toml` inside the existing
`[package]` table, so the parser matches the
discoverability metadata already present on sibling
crates.

Exact fields to add (after the existing `description`
line, preserving the file's ordering pattern used by
`rlsp-yaml/Cargo.toml`):

```toml
homepage = "https://github.com/chdalski/rlsp"
keywords = ["yaml", "parser", "streaming"]
categories = ["parser-implementations"]
```

- [x] `[package]` table in `rlsp-yaml-parser/Cargo.toml`
      contains `homepage = "https://github.com/chdalski/rlsp"`.
- [x] `[package]` table in `rlsp-yaml-parser/Cargo.toml`
      contains `keywords = ["yaml", "parser", "streaming"]`
      (exactly these three strings, in this order).
- [x] `[package]` table in `rlsp-yaml-parser/Cargo.toml`
      contains `categories = ["parser-implementations"]`
      (exactly this one string).
- [x] No other fields in `rlsp-yaml-parser/Cargo.toml`
      change (`name`, `version`, `edition`,
      `rust-version`, `license.workspace`,
      `authors.workspace`, `repository.workspace`,
      `description`, `[dependencies]`,
      `[dev-dependencies]`, `[[bench]]` entries, and
      `[lints]` remain exactly as they are on `main`
      before this task).
- [x] `cargo package --list --package rlsp-yaml-parser`
      exits with status 0 and prints no warnings about
      unknown categories, invalid keywords, or invalid
      homepage.
- [x] `cargo fmt --check` exits with status 0.
- [x] `cargo clippy --all-targets` exits with status 0
      (no new warnings).
- [x] `cargo build` for the workspace exits with status 0.

## Decisions

- **Keywords = `["yaml", "parser", "streaming"]`** — user
  selection during clarification. Emphasizes what the
  crate IS (a streaming YAML parser); mirrors the
  description line.
- **Categories = `["parser-implementations"]`** — user
  selection during clarification. Single, canonical
  crates.io category for a format-parsing library. No
  secondary category added since one precise fit beats
  two partial ones.
- **Homepage = `https://github.com/chdalski/rlsp`** —
  user selection during clarification. Matches the
  workspace-root GitHub URL already used by `rlsp-yaml`
  and `rlsp-fmt`, so users landing on any of the three
  crates reach the same project page.
- **Single task, no decomposition** — the change is one
  three-line addition to one file, gated by workspace
  build/lint/format checks. Splitting would only add
  overhead.
