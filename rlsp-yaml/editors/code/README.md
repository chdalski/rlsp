# rlsp-yaml

YAML language support powered by rlsp-yaml — fast, schema-aware, built in Rust.

## Features

**Core editing:**

- Hover information (schema-aware descriptions and formatted examples)
- Completion (structural + schema-driven with snippet support)
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
- Full-document and range formatting
- Color picker integration for CSS color values

**Validation:**

- YAML syntax errors
- Duplicate key detection
- Unused anchor warnings
- Flow style warnings
- Key ordering enforcement (opt-in)
- Custom tag validation
- JSON Schema validation (required properties, type checking, enum constraints)

**Schema support:**

- SchemaStore.org integration (auto-detects schemas by filename)
- Kubernetes schema auto-detection
- Schema association via modeline (`# yaml-language-server: $schema=<url>`)
- Schema association via workspace settings (glob-based mapping)
- HTTP schema fetching with caching

## Installation

Install from the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=chrisski.rlsp-yaml).

The extension bundles a prebuilt `rlsp-yaml` binary — no separate installation required.

**Build from source:**

```sh
git clone https://github.com/chdalski/rlsp
cd rlsp/rlsp-yaml/editors/code
pnpm install
pnpm run build
```

Then run `pnpm run package` to produce a `.vsix` file and install it with **Extensions: Install from VSIX** in VS Code.

## Configuration

Key settings (configured under `rlsp-yaml.*` in VS Code settings):

| Setting | Default | Description |
|---|---|---|
| `schemaStore` | `true` | Enable SchemaStore.org integration |
| `schemas` | `{}` | Map schema URLs to glob patterns |
| `kubernetesVersion` | `"master"` | Kubernetes schema version |
| `keyOrdering` | `false` | Enforce alphabetical key ordering |
| `formatPrintWidth` | `80` | Line width for formatting |
| `formatSingleQuote` | `false` | Use single quotes in formatted output |
| `customTags` | `[]` | Custom YAML tags to recognize |
| `httpProxy` | `""` | HTTP proxy URL for schema fetching |

See [docs/configuration.md](https://github.com/chdalski/rlsp/blob/main/rlsp-yaml/docs/configuration.md) for the full settings reference — modelines, validators, formatting, and schema fetching.

## Commands

All commands are available via the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command | Description |
|---|---|
| `rlsp-yaml: Restart Server` | Restart the language server |
| `rlsp-yaml: Show Output` | Open the server output channel |
| `rlsp-yaml: Show Version` | Display the running server version |
