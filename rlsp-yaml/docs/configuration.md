# Configuration

rlsp-yaml is configured through three mechanisms, listed from highest to lowest priority:

1. **Modelines** — per-document comments that override workspace settings
2. **Workspace settings** — passed via `initializationOptions` or `workspace/didChangeConfiguration`
3. **Defaults** — sensible built-in defaults when no configuration is provided

## Workspace Settings

Settings are passed as a JSON object via LSP `initializationOptions` at startup or `workspace/didChangeConfiguration` at runtime. Each update replaces the entire settings object.

```json
{
  "customTags": ["!include", "!ref"],
  "keyOrdering": false,
  "kubernetesVersion": "1.32.0",
  "schemas": {
    "https://json.schemastore.org/github-workflow": ".github/workflows/*.yml",
    "https://example.com/schema.json": "deploy/**/*.yaml"
  }
}
```

### `customTags`

- **Type:** `string[]`
- **Default:** `[]` (empty — custom tag validation disabled)

A list of allowed YAML custom tag names. When non-empty, documents are validated against this list and unknown tags produce diagnostics.

Tags from the workspace setting are merged with any tags declared via modeline (see below).

### `keyOrdering`

- **Type:** `boolean`
- **Default:** `false`

When enabled, the server warns if mapping keys are not in alphabetical order.

### `kubernetesVersion`

- **Type:** `string`
- **Default:** `"1.32.0"`

The Kubernetes cluster version used for automatic schema resolution. When a document contains root-level `apiVersion` and `kind` fields and no schema is configured via modeline or glob, the server fetches the corresponding schema from [yannh/kubernetes-json-schema](https://github.com/yannh/kubernetes-json-schema) using this version string.

Set this to match your cluster version to get accurate validation:

```json
{ "kubernetesVersion": "1.29.0" }
```

Schema resolution priority (highest to lowest):
1. `$schema=` modeline in the document
2. Workspace `schemas` glob match
3. Kubernetes auto-detection (this setting)

### `schemas`

- **Type:** `object` (schema URL → glob pattern)
- **Default:** `{}` (empty — no schema associations)

Maps JSON Schema URLs to file glob patterns. When a document's URI matches a pattern, the corresponding schema is fetched and used for validation, completion, and hover.

Glob syntax:
- `*` matches any characters except `/`
- `**` matches any characters including `/`

A modeline `$schema=` in the document takes priority over glob-based associations.

## Modelines

Modelines are special YAML comments in the **first 10 lines** of a file. They override workspace settings on a per-document basis.

### Schema modeline

```yaml
# yaml-language-server: $schema=<url>
```

Associates the document with a JSON Schema URL. This takes priority over any workspace `schemas` glob match.

**Disable schema validation** for a single file:

```yaml
# yaml-language-server: $schema=none
```

The `none` sentinel (case-insensitive) disables schema fetching and schema-driven validation for that document. Other validators (duplicate keys, anchors, flow style) still run.

### Custom tags modeline

```yaml
# yaml-language-server: $tags=!include,!ref,!custom
```

Declares additional custom tags for the document. These are merged with the workspace `customTags` setting — both sources contribute to the allowed tag set.

## Editor Setup

### Neovim (nvim-lspconfig)

```lua
vim.lsp.start({
  name = "rlsp-yaml",
  cmd = { "/path/to/rlsp-yaml" },
  filetypes = { "yaml", "yml" },
  init_options = {
    customTags = { "!include", "!ref" },
    keyOrdering = false,
    kubernetesVersion = "1.32.0",
    schemas = {
      ["https://json.schemastore.org/github-workflow"] = ".github/workflows/*.yml",
    },
  },
})
```

### VS Code

Use a generic LSP client extension (e.g., [vscode-languageclient](https://github.com/microsoft/vscode-languageserver-node)) and configure the binary as the server command for YAML files. Pass settings via `initializationOptions`.

## Validators

Some validators are always active; others depend on settings.

| Validator | Controlled by | Default |
|-----------|---------------|---------|
| YAML syntax errors | always active | — |
| Duplicate key detection | always active | — |
| Unused anchor warnings | always active | — |
| Flow style warnings | always active | — |
| Key ordering | `keyOrdering` | off |
| Custom tag validation | `customTags` | off (empty = disabled) |
| JSON Schema validation | `schemas` / modeline / K8s auto-detect | off (no schema = disabled) |

## Schema Fetching

Schemas are fetched over HTTP/HTTPS and cached in memory for the session.

**Limits:**
- Maximum URL length: 2048 characters
- Maximum response size: 5 MiB
- Maximum JSON nesting depth: 50
- Maximum `$ref` resolution depth: 32
- Redirects: disabled

**SSRF protection** — the server blocks schema URLs that resolve to:
- Loopback addresses (`127.0.0.0/8`, `::1`, `localhost`)
- Private networks (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`)
- Link-local addresses (`169.254.0.0/16`, `fe80::/10`)
- Unspecified addresses (`0.0.0.0`, `::`)
