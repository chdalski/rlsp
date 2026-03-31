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
├── Cargo.toml              # Rust workspace root (members: rlsp-fmt, rlsp-yaml)
├── rlsp-fmt/               # Generic Wadler-Lindig pretty-printing engine
│   └── src/
│       ├── lib.rs          # Public API: Doc, FormatOptions, format(), builder functions
│       ├── ir.rs           # Doc IR enum and builder functions
│       └── printer.rs      # Wadler-Lindig printer
├── rlsp-yaml/              # Rust YAML LSP server crate
│   ├── benches/
│   │   ├── fixtures/
│   │   │   └── mod.rs       # Shared benchmark fixture generators
│   │   ├── hot_path.rs      # Tier 1: keystroke hot-path benchmarks
│   │   ├── latency.rs       # Tier 2: user-perceivable latency benchmarks
│   │   └── insight.rs       # Tier 3: architectural insight benchmarks
│   └── src/
│       ├── main.rs         # Binary entry point
│       ├── lib.rs          # Module declarations
│       ├── server.rs       # LSP Backend struct + LanguageServer trait impl
│       ├── parser.rs       # YAML parsing (saphyr)
│       ├── document_store.rs  # In-memory document cache
│       ├── code_actions.rs # Code action provider
│       ├── code_lens.rs    # Code lens provider (schema URL link)
│       ├── color.rs        # Color provider (find_colors, color_presentations)
│       ├── completion.rs   # Completion provider
│       ├── document_links.rs  # Document links / URL detection
│       ├── folding.rs      # Folding ranges
│       ├── hover.rs        # Hover information
│       ├── formatter.rs       # YAML document formatting (full-document format_yaml)
│       ├── on_type_formatting.rs  # On-type formatting (newline indent)
│       ├── references.rs   # Go-to-definition + find references
│       ├── rename.rs       # Rename symbol
│       ├── schema.rs           # JSON Schema types, parsing, HTTP fetching, caching
│       ├── schema_validation.rs  # Schema-driven diagnostic validation
│       ├── selection.rs    # Selection ranges (AST-based, saphyr MarkedYamlOwned)
│       ├── semantic_tokens.rs  # Semantic token provider
│       ├── symbols.rs      # Document symbols
│       └── validators.rs   # Diagnostic validators (anchors, flow style, key order)
├── editors/
│   └── code/               # VS Code extension for rlsp-yaml
│       ├── package.json    # Extension manifest, settings contributions, scripts
│       ├── tsconfig.json   # TypeScript config (extends @tsconfig/strictest)
│       ├── eslint.config.mjs  # ESLint flat config (typescript-eslint strict)
│       ├── language-configuration.json  # YAML comment/bracket config for VS Code
│       └── src/            # TypeScript source (main.ts entry point)
```

## Build and Test

### Rust

```sh
cargo fmt              # format
cargo clippy --all-targets  # lint (zero warnings enforced)
cargo build            # build
cargo test             # run all tests
cargo bench            # run benchmarks (Criterion)
cargo clean            # clean stale build artifacts
```

Workspace lints are defined in the root `Cargo.toml` and inherited
via `lints.workspace = true`. All clippy warnings
at pedantic + nursery level; selected lints at `deny`.

### VS Code Extension

```sh
cd editors/code
pnpm install       # install dependencies
pnpm run build     # bundle extension (esbuild)
pnpm run test      # run unit tests (vitest)
pnpm run lint      # lint TypeScript source
pnpm run format    # check formatting (prettier)
```

## Release

Releases are automated via [release-plz](https://release-plz.ieni.dev/)
and a single GitHub Actions workflow:

1. **release-plz** (`.github/workflows/release-plz.yml`) —
   runs on every push to `main`. Creates a release PR with
   version bump and changelog, publishes to crates.io when
   the PR is merged, and runs a `build-binaries` matrix job
   inline when `releases_created` is true — building
   cross-platform binaries and uploading them to the GitHub
   Release.

**Tag format:** `<package>-v<version>` (e.g. `rlsp-yaml-v0.2.0`).
Configured at the workspace level in `release-plz.toml` via
`git_tag_name = "{{ package }}-v{{ version }}"`. When adding
a new crate, extend the `build-binaries` matrix in
`release-plz.yml` with the new crate's targets, or add a
separate inline job in the same workflow.

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

### Documentation Layout

The root `README.md` is the project landing page — it links to each crate's README and nothing else. Each crate `README.md` is self-contained for users: installation, editor setup, features, and a link to `docs/configuration.md` for the full settings reference. `docs/configuration.md` is a pure settings reference — workspace settings, modelines, validators, formatting, and schema fetching. It contains no usage guidance or editor setup instructions; those live in the crate README.

## Contribution Model

This project is AI-written. External contributions
are accepted as GitHub issues (bug reports and feature
requests), not as pull requests or patches. The
maintainer reviews issues and AI implements accepted
changes.

## Trusted Sources

- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
