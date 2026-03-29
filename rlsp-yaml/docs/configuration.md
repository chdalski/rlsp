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
  "kubernetesVersion": "master",
  "schemaStore": true,
  "formatValidation": true,
  "formatPrintWidth": 80,
  "formatSingleQuote": false,
  "httpProxy": "http://proxy.corp:8080",
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
- **Default:** `"master"` (tracks the latest available schemas)

The Kubernetes cluster version used for automatic schema resolution. When a document contains root-level `apiVersion` and `kind` fields and no schema is configured via modeline or glob, the server fetches the corresponding schema from [yannh/kubernetes-json-schema](https://github.com/yannh/kubernetes-json-schema) using this version string.

Set this to match your cluster version to get accurate validation:

```json
{ "kubernetesVersion": "1.29.0" }
```

### `schemaStore`

- **Type:** `boolean`
- **Default:** `true`

Enable automatic schema association using the [SchemaStore](https://www.schemastore.org/) catalog. When enabled, the server fetches the SchemaStore catalog on first use and matches each YAML file's path against the catalog's `fileMatch` patterns. If a match is found, the corresponding schema is fetched and used for validation, completion, and hover.

This is the lowest-priority fallback — it only applies when no modeline, workspace glob, or Kubernetes auto-detection match is found for a file.

The catalog is fetched lazily (on first need) and cached in memory for the session. If the catalog fetch fails (e.g. no network), the server continues without SchemaStore and no diagnostics are lost.

To disable SchemaStore association:

```json
{ "schemaStore": false }
```

### `schemas`

- **Type:** `object` (schema URL → glob pattern)
- **Default:** `{}` (empty — no schema associations)

Maps JSON Schema URLs to file glob patterns. When a document's URI matches a pattern, the corresponding schema is fetched and used for validation, completion, and hover.

Glob syntax:
- `*` matches any characters except `/`
- `**` matches any characters including `/`

A modeline `$schema=` in the document takes priority over glob-based associations.

### `formatPrintWidth`

- **Type:** `number`
- **Default:** `80`

Maximum line width for the full-document formatter. The formatter tries to fit content on a single line up to this width; if it doesn't fit, it breaks to block style.

### `formatSingleQuote`

- **Type:** `boolean`
- **Default:** `false`

When `true`, string scalars are wrapped in single quotes instead of double quotes. Strings that contain single quotes are always double-quoted regardless of this setting.

### `formatValidation`

- **Type:** `boolean` (optional)
- **Default:** `true`

Enable validation of the JSON Schema `format` keyword, `contentEncoding`, and `contentMediaType` keywords. When enabled:

- String values are checked against the declared `format` and a **warning** diagnostic (`schemaFormat`) is emitted for values that do not conform.
- String values are decoded against the declared `contentEncoding` and a **warning** diagnostic (`schemaContentEncoding`) is emitted for values that cannot be decoded.
- Decoded content is checked against the declared `contentMediaType` and a **warning** diagnostic (`schemaContentMediaType`) is emitted for content that does not match.

When disabled, all three keyword checks are skipped (annotation-only mode, per Draft 2019-09+).

Supported `format` values:

| Format | Description |
|--------|-------------|
| `date-time` | RFC 3339 full date-time (e.g. `2023-01-15T10:30:00Z`) |
| `date` | RFC 3339 full date (e.g. `2023-01-15`) |
| `time` | RFC 3339 partial-time with offset (e.g. `10:30:00Z`) |
| `duration` | ISO 8601 duration (e.g. `P1Y2M3DT4H`) |
| `email` | Basic email address |
| `ipv4` | IPv4 dotted-quad address |
| `ipv6` | IPv6 address |
| `hostname` | RFC 1123 hostname |
| `uri` | URI with scheme |
| `uri-reference` | URI or relative reference |
| `uri-template` | RFC 6570 URI template |
| `uuid` | UUID (case-insensitive) |
| `regex` | Valid regular expression |
| `json-pointer` | RFC 6901 JSON Pointer |
| `relative-json-pointer` | Relative JSON Pointer |
| `idn-hostname` | Internationalized domain name (IDNA UTS#46) |
| `idn-email` | Internationalized email address |
| `iri` | Internationalized Resource Identifier (RFC 3987) |
| `iri-reference` | IRI or relative IRI reference (RFC 3987) |

Unknown `format` values are silently ignored (per the JSON Schema specification, format validation is advisory).

Supported `contentEncoding` values: `base64`, `base64url`, `base32`, `base16`. Unknown encodings are silently ignored.

Supported `contentMediaType` values: `application/json`. Unknown media types are silently ignored.

To disable format and content validation:

```json
{ "formatValidation": false }
```

### `httpProxy`

- **Type:** `string` (optional)
- **Default:** `null` (no proxy)

HTTP proxy URL used for all schema fetching (individual schemas and the SchemaStore catalog). Format: `http://host:port` or `https://host:port`.

```json
{ "httpProxy": "http://proxy.corp:8080" }
```

When absent or `null`, requests are made directly. Invalid proxy URLs are silently ignored and requests fall back to direct connections.

### `colorDecorators`

- **Type:** `boolean` (optional)
- **Default:** `true`

Enable color picker integration for color values in YAML documents. When enabled, the server responds to `textDocument/documentColor` requests by detecting hex codes (`#rrggbb`, `#rgb`), CSS named colors (`red`, `blue`, etc.), and CSS color functions (`rgb()`, `rgba()`, `hsl()`, `hsla()`) in YAML values, and returns them to the editor for color picker decoration. The server also handles `textDocument/colorPresentation` requests to convert a picked color back to hex, RGB, or HSL notation.

To disable color decorators:

```json
{ "colorDecorators": false }
```

> **Indentation** (`tabWidth`, `useTabs`) is not configurable via workspace settings — it is taken directly from the LSP `textDocument/formatting` request, which carries the editor's indentation preferences. Configure indentation in your editor settings.

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

Use a generic LSP client extension (e.g., [vscode-languageclient](https://github.com/microsoft/vscode-languageserver-node)) and configure the binary as the server command for YAML files. Pass settings via `initializationOptions`.

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
| JSON Schema validation | `schemas` / modeline / K8s auto-detect / SchemaStore | off (no schema = disabled) |
| Schema `format` keyword validation | `formatValidation` | on (when schema is active) |
| SchemaStore auto-association | `schemaStore` | on |

## Formatting

The server implements `textDocument/formatting` for full-document YAML formatting and `textDocument/rangeFormatting` for formatting a selected region.

**Behavior:**

- **Indentation** (tab size, tabs vs spaces) is controlled by the editor — the LSP formatting requests carry `tab_size` and `insert_spaces` from the editor's own settings.
- **Style options** (print width, quote style) are controlled via workspace settings (`formatPrintWidth`, `formatSingleQuote`).
- **Comments** are preserved during formatting. The formatter extracts comments from the original text and reattaches them to the formatted output.
- **Syntax errors** — if the document cannot be parsed, the original text is returned unchanged so no content is lost.

**Range formatting** uses the same settings and formatter as full-document formatting. The full document is formatted internally and the resulting lines that correspond to the requested range are returned as the edit. This ensures consistent line-breaking decisions — the printer needs surrounding context to make correct choices.

The formatter is built on `rlsp-fmt`, an internal Wadler-Lindig pretty-printing engine. It walks saphyr's AST and emits IR nodes that the engine renders with line-width awareness.

## Schema Resolution Priority

When a YAML file is opened or changed, the server resolves a schema using the following chain (first match wins):

1. **Modeline** — `# yaml-language-server: $schema=<url>` in the first 10 lines of the file. Highest priority; overrides everything else. Use `$schema=none` to disable schema processing for a specific file.
2. **Workspace glob** — the `schemas` setting maps schema URLs to glob patterns. The first pattern that matches the file's URI path wins.
3. **Kubernetes auto-detection** — if the document's root mapping contains both `apiVersion` and `kind`, the server fetches the corresponding schema from [yannh/kubernetes-json-schema](https://github.com/yannh/kubernetes-json-schema) using the configured `kubernetesVersion`.
4. **SchemaStore** — if enabled (`schemaStore: true`, the default), the server fetches the [SchemaStore](https://www.schemastore.org/) catalog and matches the file's path against catalog `fileMatch` patterns. The catalog is fetched lazily on first need and cached for the session.

If none of the above produce a match, no schema is applied and only syntax/structural validators run.

## Schema Fetching

Schemas are fetched over HTTP/HTTPS and cached in memory for the session.

**Limits:**
- Maximum URL length: 2048 characters
- Maximum response size: 5 MiB
- Maximum JSON nesting depth: 50
- Maximum `$ref` resolution depth: 32
- Redirects: disabled
- Connection timeout: 5 seconds
- Total request timeout: 15 seconds

**SSRF protection** — the server blocks schema URLs that resolve to:
- Loopback addresses (`127.0.0.0/8`, `::1`, `localhost`)
- Private networks (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`)
- Link-local addresses (`169.254.0.0/16`, `fe80::/10`)
- Unspecified addresses (`0.0.0.0`, `::`)
