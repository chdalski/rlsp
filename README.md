# RLSP — Rust Language Server Project

Language server implementations written in Rust, built entirely by AI agents.
No human-written application code — every line of source was authored, reviewed, and committed by AI.

![CI](https://github.com/chdalski/rlsp/actions/workflows/ci.yml/badge.svg) [![codecov](https://codecov.io/gh/chdalski/rlsp/graph/badge.svg)](https://codecov.io/gh/chdalski/rlsp) [![crates.io](https://img.shields.io/crates/v/rlsp-yaml.svg)](https://crates.io/crates/rlsp-yaml) ![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)

## Installation

```sh
cargo install rlsp-yaml
```

Prebuilt binaries for Linux, macOS, and Windows are available on [GitHub Releases](https://github.com/chdalski/rlsp/releases).

## Features

**Editing**
- Hover, completion (schema-driven with snippets), document symbols, folding, selection ranges
- Rename, go-to-definition, and find references for anchors and aliases
- Code actions: flow/block conversion, tab fix, unused anchor delete, quoted bool normalization
- Semantic highlighting and code lens (schema title at top of document)

**Validation**
- YAML syntax errors, duplicate key detection, unused anchor warnings
- JSON Schema validation: required properties, type checking, enum constraints

**Schema support**
- Associate schemas via modeline (`# yaml-language-server: $schema=<url>`) or workspace glob settings
- HTTP schema fetching with caching and SSRF guards

See [rlsp-yaml/README.md](rlsp-yaml/README.md) for the full feature list and architecture details.

## Editor Setup

**Neovim:**

```lua
vim.lsp.start({
  name = "rlsp-yaml",
  cmd = { "/path/to/rlsp-yaml" },
  filetypes = { "yaml", "yml" },
})
```

**VS Code:** Use a generic LSP client extension and configure it to run the `rlsp-yaml` binary for YAML files.

Full configuration reference (workspace settings, modelines, schema fetching): [rlsp-yaml/docs/configuration.md](rlsp-yaml/docs/configuration.md)

## Crates

| Crate | Description |
|-------|-------------|
| [rlsp-yaml](rlsp-yaml/) | YAML language server |

## Contributing

This project accepts bug reports and feature requests via [GitHub Issues](https://github.com/chdalski/rlsp/issues). External code contributions are not accepted — all implementation is done by AI. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## License

[MIT](LICENSE) — Christoph Dalski
