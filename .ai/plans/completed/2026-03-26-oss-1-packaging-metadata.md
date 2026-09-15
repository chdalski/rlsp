**Repository:** root
**Status:** Completed (2026-03-26)
**Created:** 2026-03-26

## Goal

Prepare rlsp-yaml for crates.io publication by adding all
required and recommended metadata to Cargo.toml, adding SPDX
license headers to all source files, and updating CLAUDE.md
to document the project's AI-written identity and
issues-only contribution model.

## Context

- The crate currently has minimal metadata: just `name`,
  `version`, and `edition`
- crates.io requires `description` and `license` at minimum
- 20 `.rs` source files need SPDX headers
- CLAUDE.md needs an updated Overview section and a new
  Contribution Model section
- Repository: https://github.com/cdalski/rlsp
- License: MIT, copyright Christoph Dalski
- Version: 0.1.0 (SemVer, keeping current)
- MSRV: Rust 1.85.0 (minimum for edition 2024)

## Steps

- [x] Add crates.io metadata to rlsp-yaml/Cargo.toml
- [x] Add workspace-level metadata to root Cargo.toml
- [x] Add SPDX headers to all 20 .rs source files
- [x] Update CLAUDE.md (Overview + Contribution Model)
- [x] Verify `cargo package --list` looks correct

## Tasks

### Task 1: Cargo.toml metadata + SPDX headers

Add complete metadata to both Cargo.toml files and SPDX
headers to all source files.

**Root Cargo.toml** — add workspace-level metadata:
```toml
[workspace.package]
authors = ["Christoph Dalski"]
license = "MIT"
repository = "https://github.com/cdalski/rlsp"
```

**rlsp-yaml/Cargo.toml** — add/inherit:
```toml
[package]
name = "rlsp-yaml"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
description = "A fast, lightweight YAML language server"
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage = "https://github.com/cdalski/rlsp"
keywords = ["yaml", "lsp", "language-server"]
categories = ["development-tools", "text-editors"]
```

**SPDX headers** — add to all 20 `.rs` files in
`rlsp-yaml/src/`:
```rust
// SPDX-License-Identifier: MIT
```
As the first line, followed by an empty line, then the
existing content.

**Test files** — also add to `rlsp-yaml/tests/lsp_lifecycle.rs`
if it exists.

- [ ] Root Cargo.toml workspace metadata
- [ ] rlsp-yaml/Cargo.toml full metadata + exclude
- [ ] SPDX headers on all .rs files (20 src + 1 test)
- [ ] Verify with `cargo package --list` (dry run)

Also update the CLAUDE.md project structure to remove the
`yaml-language-server/` entry, which no longer exists.

### Task 2: Update CLAUDE.md

Update the project-root CLAUDE.md with two changes:

1. **Overview section** — expand to state the AI-written
   nature explicitly:
   > _The Rust Language Server Project_ is a collection of
   > language server implementations written in Rust,
   > built entirely by AI agents. No human-written
   > application code — every line of source is authored,
   > reviewed, and committed by AI. The purpose is to
   > provide users with small, fast implementations with
   > minimal memory footprint.

2. **Contribution Model section** — add after Repository
   Layout:
   > ## Contribution Model
   >
   > This project is AI-written. External contributions
   > are accepted as GitHub issues (bug reports and feature
   > requests), not as pull requests or patches. The
   > maintainer reviews issues and AI implements accepted
   > changes.

- [ ] Update Overview section
- [ ] Add Contribution Model section
- [ ] Verify CLAUDE.md is valid markdown

## Decisions

- **MSRV = 1.87.0** — initially planned 1.85 (minimum for
  edition 2024), but bumped to 1.87 because the codebase
  uses `str::as_str()` in a const context
  (`code_actions.rs`), which is only stable from 1.87.
  The `incompatible_msrv` clippy lint (deny-level) caught
  this.
- **Workspace-level metadata inheritance** — `authors`,
  `license`, `repository` defined once in the workspace
  root and inherited by member crates. Reduces duplication
  as more rlsp-* crates are added.
- **Remove yaml-language-server from CLAUDE.md** — the
  directory no longer exists; the project structure
  section should reflect the current state.
