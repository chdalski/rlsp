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
  "yamlVersion": "1.2",
  "httpProxy": "http://proxy.corp:8080",
  "validate": true,
  "hover": true,
  "completion": true,
  "maxItemsComputed": 5000,
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

### `formatEnforceBlockStyle`

- **Type:** `boolean`
- **Default:** `false`

When `true`, the formatter converts all flow-style collections to block style during formatting. Flow mappings (`{a: 1, b: 2}`) become block mappings and flow sequences (`[a, b]`) become block sequences.

When `false` (the default), the formatter preserves the original style — block collections stay block and flow collections stay flow.

```json
{ "formatEnforceBlockStyle": true }
```

### `yamlVersion`

- **Type:** `string` (optional)
- **Values:** `"1.1"` or `"1.2"`
- **Default:** `"1.2"`

Controls which YAML specification version the server uses for formatting quoting decisions and YAML 1.1 compatibility diagnostics.

**Formatting:** in `"1.1"` mode the formatter quotes YAML 1.1 boolean keywords to prevent ambiguity — for example, `on: push` stays plain (it is a key), but a value like `enabled: "yes"` will have its quotes preserved rather than stripped.

| Version | Keywords that require quoting |
|---------|-------------------------------|
| `"1.2"` | `true`, `false`, `null` (and case variants) |
| `"1.1"` | All of the above, plus `on`, `off`, `yes`, `no` (and case variants) |

**Diagnostics:** in `"1.2"` mode (the default) the server emits compatibility warnings for plain scalars that would be interpreted differently by YAML 1.1 parsers — the ~80% of YAML-consuming tools (Kubernetes, Ansible, GitLab CI) that use 1.1 parsers such as go-yaml v2, PyYAML, and SnakeYAML.

| Diagnostic | Severity | Trigger |
|---|---|---|
| `yaml11Boolean` | Warning | Plain scalar matches a YAML 1.1 boolean form not in YAML 1.2 (`yes`, `no`, `on`, `off`, `y`, `n`, and case variants) |
| `yaml11Octal` | Info | Plain scalar is a C-style octal literal (`0777`) — octal in YAML 1.1, a plain string in YAML 1.2 |
| `schemaYaml11Boolean` | Warning | Field is schema-typed as `string` and value is a YAML 1.1 boolean — passes the 1.2 type check but downstream 1.1 tools will corrupt it silently |
| `schemaYaml11Octal` | Warning | Field is schema-typed as `string` and value is a C-style octal — same cross-version risk |
| `schemaYaml11BooleanType` | Error | Field is schema-typed as `boolean` and value is a YAML 1.1 boolean form — enhanced type-mismatch error explaining that `yes` is not a boolean in YAML 1.2 |

In `"1.1"` mode all five diagnostics are suppressed — the user has explicitly opted into 1.1 semantics, so these values are intentional.

**Quick fixes** are available for `yaml11Boolean`, `yaml11Octal`, `schemaYaml11Boolean`, `schemaYaml11Octal`, and `schemaYaml11BooleanType`:
- **"Quote value"** — wraps the value in double quotes (`yes` → `"yes"`). Available for `yaml11Boolean`, `yaml11Octal`, `schemaYaml11Boolean`, and `schemaYaml11Octal`. Universally safe; listed first.
- **"Convert to boolean"** — converts to the canonical 1.2 form (`yes` → `true`, `no` → `false`). Available for `yaml11Boolean`, `schemaYaml11Boolean`, and `schemaYaml11BooleanType`.
- **"Convert to YAML 1.2 octal"** — converts `0777` → `0o777`. Available for `yaml11Octal` and `schemaYaml11Octal`. Appropriate only when downstream consumers use 1.2 parsers.

These diagnostics can be suppressed per-line or per-file:

```yaml
# rlsp-yaml-disable-next-line yaml11Boolean
enabled: yes

# rlsp-yaml-disable-file yaml11Octal
```

> **Parser limitation:** the underlying parser (rlsp-yaml-parser) always processes documents as YAML 1.2 regardless of this setting. Octal literals (`0644`) and sexagesimal values (`1:30:00`) are treated as plain strings, not integers. The diagnostics above bridge this gap — the server parses as 1.2 but warns when values would be interpreted differently by 1.1 parsers.

Override this setting for a single document with the `$yamlVersion` modeline (see below).

```json
{ "yamlVersion": "1.1" }
```

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

### `flowStyle`

- **Type:** `string`
- **Values:** `"off"`, `"warning"`, `"error"`
- **Default:** `"warning"`

Controls the severity of `flowMap` and `flowSeq` diagnostics, which are emitted when the document uses flow-style collections (`{...}` or `[...]`) where block style is preferred.

| Value | Behavior |
|-------|----------|
| `"off"` | `flowMap` and `flowSeq` diagnostics are disabled |
| `"warning"` | Flow-style collections are flagged with a warning squiggle (default) |
| `"error"` | Flow-style collections are flagged as errors |

When `formatEnforceBlockStyle` is `true`, the formatter will rewrite flow-style collections to block style on save, which pairs well with `"error"` to enforce block style as a hard rule.

```json
{ "flowStyle": "error" }
```

### `duplicateKeys`

- **Type:** `string`
- **Values:** `"off"`, `"warning"`, `"error"`
- **Default:** `"error"`

Controls the severity of `duplicateKey` diagnostics, which are emitted when a mapping contains more than one entry with the same key.

| Value | Behavior |
|-------|----------|
| `"off"` | Duplicate key detection is disabled |
| `"warning"` | Duplicate keys are flagged with a warning squiggle |
| `"error"` | Duplicate keys are flagged as errors (default) |

```json
{ "duplicateKeys": "warning" }
```

### `formatRemoveDuplicateKeys`

- **Type:** `boolean`
- **Default:** `false`

When `true`, the formatter removes duplicate mapping keys during formatting. Only the last occurrence of each key is kept, which matches the YAML specification behavior (later definitions shadow earlier ones).

When `false` (the default), duplicate keys are preserved as-is during formatting.

```json
{ "formatRemoveDuplicateKeys": true }
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

### `validate`

- **Type:** `boolean` (optional)
- **Default:** `true`

Enable or disable diagnostic validation. When `false`, the server publishes no diagnostics for any open document — syntax errors, duplicate keys, schema violations, and all other validators are suppressed. Existing diagnostics are cleared immediately when validation is disabled.

```json
{ "validate": false }
```

### `hover`

- **Type:** `boolean` (optional)
- **Default:** `true`

Enable or disable hover information. When `false`, the server returns no results for `textDocument/hover` requests.

```json
{ "hover": false }
```

### `completion`

- **Type:** `boolean` (optional)
- **Default:** `true`

Enable or disable completion suggestions. When `false`, the server returns no results for `textDocument/completion` requests.

```json
{ "completion": false }
```

### `maxItemsComputed`

- **Type:** `number` (optional)
- **Default:** `5000`

Maximum number of items returned by document symbol (`textDocument/documentSymbol`) and folding range (`textDocument/foldingRange`) requests. Results are truncated to this limit before being returned to the client. Setting this to `0` suppresses all results from these two requests.

This limit prevents performance degradation when editing very large YAML files that would otherwise produce thousands of symbols or folding regions.

```json
{ "maxItemsComputed": 1000 }
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

### YAML version modeline

```yaml
# yaml-language-server: $yamlVersion=1.1
```

Overrides the workspace `yamlVersion` setting for this document. Accepted values: `1.1` and `1.2`. Any other value is ignored and the workspace setting applies.

Useful in mixed repositories where different files target different YAML versions — for example, Ansible playbooks (YAML 1.1) alongside Kubernetes manifests (YAML 1.2):

```yaml
# yaml-language-server: $yamlVersion=1.1
# Ansible playbook — on/off/yes/no are boolean keywords here
- hosts: all
  become: yes
```

## Diagnostic Suppression

Diagnostics can be silenced on a per-line or per-file basis using suppression comments. This is useful for false positives or intentional style deviations without disabling a validator globally.

### Suppress the next line

```yaml
# rlsp-yaml-disable-next-line
key: value  # all diagnostics on this line are suppressed
```

```yaml
# rlsp-yaml-disable-next-line duplicateKey
name: first
name: second  # duplicateKey suppressed; other codes still reported
```

```yaml
# rlsp-yaml-disable-next-line duplicateKey, flowMap
```

The comment must appear on the line **immediately before** the line to suppress. Only the one following line is affected.

### Suppress the entire file

```yaml
# rlsp-yaml-disable-file flowMap
config: {a: 1, b: 2}  # no flowMap warning anywhere in this file
```

```yaml
# rlsp-yaml-disable-file
# all diagnostics suppressed for this file
```

The comment can appear anywhere in the file. The first `# rlsp-yaml-disable-file` comment wins; subsequent ones are ignored.

### Available diagnostic codes

| Code | Emitted by |
|------|-----------|
| `duplicateKey` | Duplicate mapping key in the same scope (severity controlled by `duplicateKeys`) |
| `flowMap` | Flow mapping (`{a: 1}`) where block style is preferred (severity controlled by `flowStyle`) |
| `flowSeq` | Flow sequence (`[a, b]`) where block style is preferred (severity controlled by `flowStyle`) |
| `unusedAnchor` | Anchor defined but never aliased |
| `unresolvedAlias` | Alias references an undefined anchor |
| `unknownTag` | Tag not in the allowed `customTags` list |
| `mapKeyOrder` | Mapping keys not in alphabetical order (requires `keyOrdering: true`) |
| `yamlSyntax` | YAML parse error |
| `schemaRequired` | Required property missing (JSON Schema `required`) |
| `schemaType` | Value does not match declared JSON Schema type |
| `schemaEnum` | Value not in the declared `enum` list |
| `schemaAdditionalProperty` | Additional property not allowed by schema |
| `schemaFormat` | Value does not match declared `format` (requires `formatValidation: true`) |
| `schemaContentEncoding` | Value cannot be decoded with declared `contentEncoding` |
| `schemaContentMediaType` | Decoded content does not match declared `contentMediaType` |
| `yaml11Boolean` | Plain scalar matches a YAML 1.1 boolean form not in YAML 1.2 (suppressed when `yamlVersion: "1.1"`) |
| `yaml11Octal` | Plain scalar is a C-style octal literal (`0777`) — octal in YAML 1.1, string in YAML 1.2 (suppressed when `yamlVersion: "1.1"`) |
| `schemaYaml11Boolean` | Field is schema-typed as `string` and value is a YAML 1.1 boolean form (suppressed when `yamlVersion: "1.1"`) |
| `schemaYaml11Octal` | Field is schema-typed as `string` and value is a C-style octal literal (suppressed when `yamlVersion: "1.1"`) |
| `schemaYaml11BooleanType` | Boolean-typed field receives a YAML 1.1 boolean form; enhanced `schemaType` message explaining the 1.1/1.2 difference (suppressed when `yamlVersion: "1.1"`) |

Codes not listed here (e.g. codes produced by future validators) can also be suppressed — the suppression comment accepts any string code.

## Validators

Some validators are always active; others depend on settings.

| Validator | Controlled by | Default |
|-----------|---------------|---------|
| YAML syntax errors | `validate` | on |
| Duplicate key detection | `validate`, `duplicateKeys` | on (error severity) |
| Unused anchor warnings | `validate` | on |
| Flow style warnings | `flowStyle` | warning |
| Key ordering | `keyOrdering` | off |
| Custom tag validation | `customTags` | off (empty = disabled) |
| JSON Schema validation | `schemas` / modeline / K8s auto-detect / SchemaStore | off (no schema = disabled) |
| Schema `format` keyword validation | `formatValidation` | on (when schema is active) |
| YAML 1.1 compatibility diagnostics | `yamlVersion` | on (suppressed when `"1.1"`) |
| SchemaStore auto-association | `schemaStore` | on |

## Formatting

The server implements `textDocument/formatting` for full-document YAML formatting and `textDocument/rangeFormatting` for formatting a selected region.

**Behavior:**

- **Indentation** (tab size, tabs vs spaces) is controlled by the editor — the LSP formatting requests carry `tab_size` and `insert_spaces` from the editor's own settings.
- **Style options** (print width, quote style) are controlled via workspace settings (`formatPrintWidth`, `formatSingleQuote`).
- **Comments** are preserved during formatting. The formatter extracts comments from the original text and reattaches them to the formatted output.
- **Syntax errors** — if the document cannot be parsed, the original text is returned unchanged so no content is lost.

**Range formatting** uses the same settings and formatter as full-document formatting. The full document is formatted internally and the resulting lines that correspond to the requested range are returned as the edit. This ensures consistent line-breaking decisions — the printer needs surrounding context to make correct choices.

The formatter is built on `rlsp-fmt`, an internal Wadler-Lindig pretty-printing engine. It walks the parsed AST and emits IR nodes that the engine renders with line-width awareness.

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
