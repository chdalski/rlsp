# rlsp-yaml

A YAML language server implementing the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) using [tower-lsp](https://github.com/ebkalderon/tower-lsp) and [saphyr](https://github.com/saphyr-rs/saphyr) for YAML parsing.

## Features

**Core editing support:**
- Hover information (with schema-aware descriptions and formatted examples)
- Completion (structural + schema-driven, with snippet support)
- Document symbols (outline view)
- Folding ranges
- Selection ranges (AST-based expand/shrink)
- On-type formatting (auto-indent after colons and block scalars)
- Rename symbol (anchors/aliases)
- Go-to-definition and find references (anchors/aliases)
- Document links (URL detection + `!include` file paths)
- Code actions (flow/block conversion, tab fix, unused anchor delete, quoted bool, block scalar)
- Code lens (schema title/URL at top of document)
- Semantic highlighting (keys, values, anchors, aliases, tags, comments)

**Validation:**
- YAML syntax errors
- Duplicate key detection
- Unused anchor warnings
- Flow style warnings
- Key ordering enforcement (opt-in)
- Custom tag validation (workspace settings + modeline)
- JSON Schema validation (required properties, type checking, enum constraints)

**Schema support:**
- Schema association via modeline (`# yaml-language-server: $schema=<url>`)
- Schema association via workspace settings (glob-based `schemas` mapping)
- Schema disable via `$schema=none` modeline
- HTTP schema fetching with caching and SSRF guards
- Deprecated property tagging in completions
- Multi-required snippet completion (inserts all missing required properties)
- Expected properties listed in diagnostic messages

**Infrastructure:**
- File watcher registration (reacts to external file changes)
- Workspace settings (`customTags`, `keyOrdering`, `schemas`)

## Architecture

Pure-function design: each feature module exports a function that takes text (and optionally parsed YAML or schema) and returns LSP types. The server layer (`server.rs`) handles document storage, schema caching, and delegates to these pure functions.

```
src/
├── main.rs              # Binary entry point
├── lib.rs               # Module declarations
├── server.rs            # LSP Backend + LanguageServer trait impl
├── parser.rs            # YAML parsing (saphyr)
├── document_store.rs    # In-memory document cache
├── schema.rs            # JSON Schema types, fetching, caching
├── schema_validation.rs # Schema-driven diagnostics
├── validators.rs        # Non-schema diagnostics (anchors, flow, keys)
├── completion.rs        # Completion provider
├── hover.rs             # Hover provider
├── symbols.rs           # Document symbols
├── references.rs        # Go-to-definition + find references
├── rename.rs            # Rename symbol
├── folding.rs           # Folding ranges
├── selection.rs         # Selection ranges
├── code_actions.rs      # Code actions
├── code_lens.rs         # Code lens
├── document_links.rs    # Document links / URL detection
├── on_type_formatting.rs # On-type formatting
└── semantic_tokens.rs   # Semantic highlighting
```

## Building

```sh
cargo build            # build
cargo test             # run all tests (~660 tests)
cargo clippy           # lint (pedantic + nursery, zero warnings)
cargo fmt              # format
```

## Usage

The server communicates over stdio using the LSP protocol. Point your editor's LSP client at the binary:

```sh
cargo build --release
# binary at target/release/rlsp-yaml
```

**VS Code:** Use a generic LSP client extension and configure it to run the binary for YAML files.

**Neovim (nvim-lspconfig):**
```lua
vim.lsp.start({
  name = "rlsp-yaml",
  cmd = { "/path/to/rlsp-yaml" },
  filetypes = { "yaml", "yml" },
})
```

## Configuration

Settings are passed via `initializationOptions` or `workspace/didChangeConfiguration`. Per-document modelines override workspace settings.

See [docs/configuration.md](docs/configuration.md) for the full reference — workspace settings, modelines, editor setup examples, and schema fetching details.

## License

[MIT](../LICENSE) — Christoph Dalski
