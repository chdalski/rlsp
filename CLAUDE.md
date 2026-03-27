# Project Context

Project-specific context that all agents need regardless of
which files they touch.

## Overview

_The Rust Language Server Project_ is a collection of
language server implementations written in Rust,
built entirely by AI agents. No human-written
application code — every line of source is authored,
reviewed, and committed by AI. The purpose is to
provide users with small, fast implementations with
minimal memory footprint.

## Project Structure

```text
/
├── Cargo.toml              # Rust workspace root (single member: rlsp-yaml)
├── rlsp-yaml/              # Rust YAML LSP server crate
│   └── src/
│       ├── main.rs         # Binary entry point
│       ├── lib.rs          # Module declarations
│       ├── server.rs       # LSP Backend struct + LanguageServer trait impl
│       ├── parser.rs       # YAML parsing (saphyr)
│       ├── document_store.rs  # In-memory document cache
│       ├── code_actions.rs # Code action provider
│       ├── code_lens.rs    # Code lens provider (schema URL link)
│       ├── completion.rs   # Completion provider
│       ├── document_links.rs  # Document links / URL detection
│       ├── folding.rs      # Folding ranges
│       ├── hover.rs        # Hover information
│       ├── on_type_formatting.rs  # On-type formatting (newline indent)
│       ├── references.rs   # Go-to-definition + find references
│       ├── rename.rs       # Rename symbol
│       ├── schema.rs           # JSON Schema types, parsing, HTTP fetching, caching
│       ├── schema_validation.rs  # Schema-driven diagnostic validation
│       ├── selection.rs    # Selection ranges (AST-based, saphyr MarkedYamlOwned)
│       ├── semantic_tokens.rs  # Semantic token provider
│       ├── symbols.rs      # Document symbols
│       └── validators.rs   # Diagnostic validators (anchors, flow style, key order)
```

## Build and Test

### Rust

```sh
cargo fmt              # format
cargo clippy           # lint (zero warnings enforced)
cargo build            # build
cargo test             # run all tests
cargo clean            # clean stale build artifacts
```

Workspace lints are defined in the root `Cargo.toml` and inherited
via `lints.workspace = true`. All clippy warnings
at pedantic + nursery level; selected lints at `deny`.

## Release

Releases are automated via [release-plz](https://release-plz.ieni.dev/)
and two GitHub Actions workflows:

1. **release-plz** (`.github/workflows/release-plz.yml`) —
   runs on every push to `main`. Creates a release PR with
   version bump and changelog, and publishes to crates.io
   when the PR is merged.

2. **release-binaries** (`.github/workflows/release-binaries.yml`) —
   triggered by tag push. Builds cross-platform binaries
   and uploads them to the GitHub Release.

**Tag format:** `<package>-v<version>` (e.g. `rlsp-yaml-v0.2.0`).
Configured at the workspace level in `release-plz.toml` via
`git_tag_name = "{{ package }}-v{{ version }}"`. Each crate's
binary workflow must have a matching trigger pattern
(e.g. `rlsp-yaml-v*`). When adding a new crate, add a
corresponding `release-binaries` workflow or extend the
existing one with the new tag pattern.

**Changelogs** are auto-generated from conventional commits
via git-cliff (`cliff.toml`).

**Publishing** uses OIDC trusted publishing — no `CARGO_REGISTRY_TOKEN` secret
is needed. release-plz handles the OIDC token exchange internally. New crates
must be published manually the first time (crates.io requirement); after that,
configure trusted publishing on crates.io under Settings → Trusted Publishing,
pointing to the `release-plz.yml` workflow.

## Repository Layout

Each language server lives in its own crate: `rlsp-<language>`.
Every crate must maintain these files (see `rlsp-yaml/` for reference):

```text
rlsp-<language>/
├── README.md                  # crate overview
└── docs/
    ├── configuration.md       # all server settings — update when config changes
    └── feature-log.md         # feature decisions — update when features are added or rejected
```

## Contribution Model

This project is AI-written. External contributions
are accepted as GitHub issues (bug reports and feature
requests), not as pull requests or patches. The
maintainer reviews issues and AI implements accepted
changes.

## Trusted Sources

- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
