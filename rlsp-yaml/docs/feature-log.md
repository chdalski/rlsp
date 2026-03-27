# Feature Log

Feature decisions prioritized into tiers by user impact, implementation feasibility, and alignment with existing infrastructure.

## Tier 1 — High Impact, Feasible Now

### Duplicate Key Detection [completed]

**Priority:** 1

Our server currently has no duplicate key detection at all. saphyr silently keeps the last occurrence. We should emit a diagnostic when duplicate keys are found in a mapping.

> Validator infrastructure already exists (anchors, flow style, key ordering) — this follows the same pattern.

### Expected Properties in Diagnostic Messages [completed]

**Priority:** 2

Our "missing required property" diagnostics could list the expected property names to help users fix the issue.

> `schema_validation.rs` already reports "schemaRequired" diagnostics — enriching the message text is a small change.

### Exclude Deprecated Properties from Completion [completed]

**Priority:** 3

We don't check the `deprecated` flag in JSON Schema when building completion items. Deprecated properties should be de-prioritized or marked with a strikethrough.

> Schema properties are already iterated in `completion.rs` — checking the flag is minimal work.

## Tier 2 — Medium Impact, Moderate Effort

### Multi-Required Snippet Completion [completed]

**Priority:** 4

When completing a key in a mapping, offer a snippet that inserts all remaining required properties at once (with placeholder values).

### Hover Formatting Improvements [completed]

**Priority:** 5

Format JSON examples in hover with proper indentation instead of single-line JSON stringify output.

### Schema Disable via Modeline [completed]

**Priority:** 6

Allow users to disable schema validation for a specific file using a modeline like `# yaml-language-server: $schema=none`.

> Modeline parsing already exists (`extract_schema_url`). Adding a sentinel value is a small parser change.

## Tier 3 — Valuable but Higher Effort

### Semantic Highlighting [completed]

**Priority:** 7

Provide semantic tokens for richer syntax highlighting of keys, values, anchors, aliases, tags, and comments.

> Requires full SemanticTokensProvider protocol — token types, legends, delta encoding. Significant new surface area.

### Schema Association Configuration [completed]

**Priority:** 8

More flexible file-to-schema mapping: disable association for specific files, define multiple validation patterns, allow users to change default schema URLs.

### File Watcher Registration [completed]

**Priority:** 9

Register `workspace/didChangeWatchedFiles` capability so the server reacts to file changes without relying on the editor extension to push notifications.

### Kubernetes-Aware Schema Resolution [completed]

**Priority:** 10

Auto-detect Kubernetes manifests by inspecting root-level `apiVersion` and `kind` fields and fetch the correct schema from yannh/kubernetes-json-schema. Eliminates manual schema configuration for standard K8s resources.

> Motivated by redhat-developer/yaml-language-server#1213 — wrong schema version applied to HPA v2 manifests. Our approach resolves the correct version-specific schema automatically.

### SchemaStore Integration

**Priority:** 11

Automatically fetch schema associations from [SchemaStore](https://www.schemastore.org/) so common file types (GitHub Actions, Docker Compose, Ansible, etc.) validate without any user configuration.

> Red Hat's server does this. High user value — most YAML files users edit have a schema on SchemaStore. Requires fetching the SchemaStore catalog and matching filenames against it.

### Full Document Formatting

**Priority:** 12

Implement `textDocument/formatting` to reformat entire YAML documents. Red Hat uses Prettier under the hood; we'd need a pure-Rust formatter.

> Users expect formatting from a modern LSP. Significant effort — either integrate an existing Rust YAML formatter or build one. No mature Rust YAML formatting crate exists today.

### Range Formatting

**Priority:** 13

Implement `textDocument/rangeFormatting` to format a selected region of a YAML document.

> Depends on full document formatting infrastructure. Lower priority than full-document formatting.

### Proxy Support for Schema Fetching

**Priority:** 14

Allow users to configure an HTTP proxy for schema fetching, supporting corporate environments behind firewalls.

> `ureq` supports proxy configuration. Needs a new setting (`httpProxy` or similar) plumbed through to the fetch layer.

## Tier 4 — Niche or High Effort / Low Return

### Tab-to-Spaces On-Type Formatting [won't implement]

**Priority:** 10

On-type formatting only handles newline insertion. Could add tab-to-spaces conversion on typing tab characters.

> Most editors handle tab-to-spaces natively. Risk of conflicting with editor settings.

### Multiple Schemas per File [won't implement]

**Priority:** 11

Support applying different schemas to different documents within a multi-document YAML file, or to sub-values within a single document.

> Requires significant architecture changes for per-sub-value schema assignment.

### Embedded Language Support [won't implement]

**Priority:** 12

Support syntax highlighting and validation for embedded languages (JSON, SQL, etc.) within YAML string values.

> Very high effort — language embedding, delegating to sub-LSPs. Niche use case.

### Localized Validation Messages [won't implement]

**Priority:** 13

Internationalize diagnostic messages so they can be presented in the user's locale.

> Cross-cutting concern touching every diagnostic string. High maintenance cost for a developer-focused tool.
