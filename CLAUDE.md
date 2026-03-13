# Project Context

Project-specific context that all agents need regardless of
which files they touch. Auto-generated sections are filled
by `/project-init`; TODO sections need human curation.
This file is written to the project root as `CLAUDE.md`,
where Claude Code loads it at cache level 3 alongside
`.claude/CLAUDE.md` — both files are always in context.

## Overview

<!-- TODO: Add project name and a brief description of what
this project does, who it serves, and its core purpose.
Auto-detection cannot infer intent — only humans know why
the project exists. -->

## Languages and Frameworks

**Rust** (edition 2024) — primary language for the LSP server
- `tower-lsp` 0.20 — LSP server framework (with `proposed` features)
- `tokio` 1 — async runtime (`full` feature set)
- `serde_json` 1 — JSON serialization for LSP protocol messages
- `saphyr` 0.0.6 — YAML parsing (with span support via `MarkedYamlOwned`)
- `regex` 1 — regular expression support
- `once_cell` 1 — lazy static initialization

**TypeScript** (target ES2020, CommonJS modules) — upstream reference implementation
- `vscode-languageserver` ^9.0.0 — LSP server SDK
- `vscode-languageserver-types` ^3.16.0 — LSP type definitions
- `yaml` 2.7.1 — YAML parsing
- `ajv` ^8.17.1 — JSON schema validation
- Test tooling: Mocha 11 + Chai + Sinon

## Project Structure

```
/
├── Cargo.toml              # Rust workspace root (single member: rlsp-yaml)
├── rlsp-yaml/              # Rust YAML LSP server crate
│   └── src/
│       ├── main.rs         # Binary entry point
│       ├── lib.rs          # Module declarations
│       ├── server.rs       # LSP Backend struct + LanguageServer trait impl
│       ├── parser.rs       # YAML parsing (saphyr)
│       ├── document_store.rs  # In-memory document cache
│       ├── completion.rs   # Completion provider
│       ├── document_links.rs  # Document links / URL detection
│       ├── folding.rs      # Folding ranges
│       ├── hover.rs        # Hover information
│       ├── references.rs   # Go-to-definition + find references
│       ├── rename.rs       # Rename symbol
│       ├── schema.rs           # JSON Schema types, parsing, HTTP fetching, caching
│       ├── selection.rs    # Selection ranges (AST-based, saphyr MarkedYamlOwned)
│       ├── symbols.rs      # Document symbols
│       └── validators.rs   # Diagnostic validators (anchors, flow style, key order)
└── yaml-language-server/   # TypeScript YAML LS (Red Hat upstream, v1.20.0)
    ├── src/                # TypeScript source
    └── test/               # Mocha tests
```

## Active Rules

All source code:
- `code-principles.md` — Kent Beck's four rules of simple design
- `code-mass.md` — minimize code mass

Rust (`rlsp-yaml/`):
- `lang-rust.md` — Rust idioms, ownership, error handling, async
- `functional-style.md` — immutability-first, iterator chains, composition

TypeScript (`yaml-language-server/`):
- `lang-typescript.md` — TypeScript idioms and patterns
- `functional-style.md` — functional style for TS

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
by `rlsp-yaml` via `lints.workspace = true`. All clippy warnings
at pedantic + nursery level; selected lints at `deny`.

### TypeScript (`yaml-language-server/`)

```sh
npm run compile        # tsc -p .
npm test               # mocha with ts-node
npm run lint           # eslint --max-warnings 0
npm run build          # clean + lint + compile + build libs
```

## Architecture

<!-- TODO: Describe the high-level architecture — layers,
modules, data flow, key abstractions. Auto-detection can
find files but cannot infer design intent or system
boundaries. -->

## Code Exemplars

<!-- TODO: List 2-3 files that best represent the project's
coding style and conventions. These serve as concrete
examples for agents to follow — style guides describe
principles, exemplars show them in practice. -->

## Anti-Patterns

<!-- TODO: List project-specific "never do this" patterns.
Every project accumulates hard-won knowledge about what
NOT to do — patterns that cause bugs, performance issues,
or maintenance pain in THIS specific codebase. -->

## Trusted Sources

<!-- TODO: List authoritative references for this project —
API docs, RFCs, internal design docs, team wikis. Agents
use these as ground truth when general knowledge conflicts
with project-specific conventions. -->
