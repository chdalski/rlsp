# rlsp-yaml

A YAML language server implementing the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) using [tower-lsp](https://github.com/ebkalderon/tower-lsp) and [rlsp-yaml-parser](../rlsp-yaml-parser) for YAML parsing.

## Installation

```sh
cargo install rlsp-yaml
```

Prebuilt binaries for Linux, macOS, and Windows are available on [GitHub Releases](https://github.com/chdalski/rlsp/releases).

## Editor Setup

The server communicates over stdio using the LSP protocol. Point your editor's LSP client at the binary:

```sh
cargo build --release
# binary at target/release/rlsp-yaml
```

### Neovim (nvim-lspconfig)

```lua
vim.lsp.start({
  name = "rlsp-yaml",
  cmd = { "/path/to/rlsp-yaml" },
  filetypes = { "yaml", "yml" },
  init_options = {
    customTags = { "!include", "!ref" },
    keyOrdering = false,
    kubernetesVersion = "master",
    schemaStore = true,
    formatValidation = true,
    schemas = {
      ["https://json.schemastore.org/github-workflow"] = ".github/workflows/*.yml",
    },
    formatPrintWidth = 80,
    formatSingleQuote = false,
    httpProxy = "http://proxy.corp:8080",
  },
})
```

### VS Code

A dedicated extension is available at [`integrations/vscode/`](integrations/vscode/). It bundles the compiled `rlsp-yaml` binary and configures itself automatically — no manual setup required. Platform-specific VSIX packages are built by CI and attached to each release.

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.rlsp-yaml]
command = "/path/to/rlsp-yaml"

[language-server.rlsp-yaml.config]
customTags = ["!include", "!ref"]
keyOrdering = false
kubernetesVersion = "master"
schemaStore = true
formatValidation = true
formatPrintWidth = 80
formatSingleQuote = false
# httpProxy = "http://proxy.corp:8080"

[language-server.rlsp-yaml.config.schemas]
"https://json.schemastore.org/github-workflow" = ".github/workflows/*.yml"

[[language]]
name = "yaml"
language-servers = ["rlsp-yaml"]
```

### Zed

Add to Zed settings (`~/.config/zed/settings.json` or project `.zed/settings.json`):

```json
{
  "lsp": {
    "rlsp-yaml": {
      "binary": {
        "path": "/path/to/rlsp-yaml"
      },
      "initialization_options": {
        "customTags": ["!include", "!ref"],
        "keyOrdering": false,
        "kubernetesVersion": "master",
        "schemaStore": true,
        "formatValidation": true,
        "formatPrintWidth": 80,
        "formatSingleQuote": false,
        "schemas": {
          "https://json.schemastore.org/github-workflow": ".github/workflows/*.yml"
        }
      }
    }
  },
  "languages": {
    "YAML": {
      "language_servers": ["rlsp-yaml"]
    }
  }
}
```

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

## Configuration

Settings are configured through three mechanisms: modelines (per-document comments), workspace settings (passed via `initializationOptions` or `workspace/didChangeConfiguration`), and built-in defaults.

See [docs/configuration.md](docs/configuration.md) for the full reference — workspace settings, modelines, validators, formatting, and schema fetching details.

## Architecture

Pure-function design: each feature module exports a function that takes text (and optionally parsed YAML or schema) and returns LSP types. The server layer (`server.rs`) handles document storage, schema caching, and delegates to these pure functions.

```text
src/
├── main.rs              # Binary entry point
├── lib.rs               # Module declarations
├── server.rs            # LSP Backend + LanguageServer trait impl
├── parser.rs            # YAML parsing (rlsp-yaml-parser)
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
cargo test             # run all tests
cargo clippy           # lint (pedantic + nursery, zero warnings)
cargo fmt              # format
```

## License

[MIT](../LICENSE) — Christoph Dalski

## AI Note

Every line of source in this crate was authored, reviewed, and committed by AI agents
working through a multi-agent pipeline (planning, implementation, independent review,
and test/security advisors for high-risk tasks). The human role is designing the
architecture, rules, and review process; agents execute them. Conformance against the
YAML Test Suite is a measured acceptance criterion — not an aspiration — and any change
touching parser behaviour or untrusted input passes through formal test and security
advisor review before being merged.
