# Feature Log

Feature decisions for rlsp-yaml, newest first. Tiered by
user impact, implementation feasibility, and alignment
with existing infrastructure.

**Tiers:**
- **1** — High impact, feasible now
- **2** — Medium impact, moderate effort
- **3** — Valuable but higher effort
- **4** — Niche or high effort / low return

---

### String Constraints (`pattern`, `minLength`, `maxLength`) [not started]

**Description:** Validate string values against `pattern` regex, `minLength`, and `maxLength` from JSON Schema.
**Complexity:** Low
**Comment:** All three fields already parsed into `JsonSchema` struct but never checked in `schema_validation.rs`. Draft-04 keywords.
**Tier:** 1

### Numeric Constraints (`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`) [not started]

**Description:** Validate numeric values against bounds and divisibility constraints.
**Complexity:** Low
**Comment:** `minimum`/`maximum` already parsed but never validated. `exclusiveMinimum`/`exclusiveMaximum` need parsing (boolean in Draft-04, number in Draft-06+). `multipleOf` needs parsing. All Draft-04 keywords with Draft-06 evolution.
**Tier:** 1

### `const` Keyword [not started]

**Description:** Validate that a value matches a single fixed value specified by `const`.
**Complexity:** Low
**Comment:** Draft-06 keyword. Simple equality check against a single JSON value. Very common in real schemas — Kubernetes CRDs use `oneOf` + `const` patterns.
**Tier:** 1

### `not` Keyword [not started]

**Description:** Validate that a value does *not* match the given sub-schema.
**Complexity:** Low
**Comment:** Draft-04 keyword. Invert the validation result of a sub-schema. Straightforward composition — run validation, pass if diagnostics are produced.
**Tier:** 2

### `patternProperties` [not started]

**Description:** Match mapping keys against regex patterns and validate their values against corresponding schemas.
**Complexity:** Medium
**Comment:** Draft-04 keyword. Needs new field in `JsonSchema`, parsing, validation, and completion integration. Common in schemas that allow dynamic key names (e.g. Kubernetes labels, environment variables).
**Tier:** 2

### Array Constraints (`minItems`, `maxItems`, `uniqueItems`) [not started]

**Description:** Validate array length bounds and item uniqueness.
**Complexity:** Low
**Comment:** Draft-04 keywords. Need new fields in `JsonSchema` and straightforward validation checks on sequence length and item equality.
**Tier:** 2

### `dependencies` / `dependentRequired` / `dependentSchemas` [not started]

**Description:** Validate cross-property dependencies — when property A is present, require property B or validate against an additional schema.
**Complexity:** Medium
**Comment:** `dependencies` is Draft-04, split into `dependentRequired` and `dependentSchemas` in Draft 2019-09. Need to support both forms for backwards compatibility. Used in schemas with conditional requirements.
**Tier:** 2

### `propertyNames` [not started]

**Description:** Validate that all mapping keys match a given schema (typically a string schema with `pattern`).
**Complexity:** Low
**Comment:** Draft-06 keyword. Applies a schema to each key string in a mapping. Useful for enforcing key naming conventions.
**Tier:** 2

### `if` / `then` / `else` [not started]

**Description:** Conditional schema application — if a value matches the `if` schema, validate against `then`; otherwise validate against `else`.
**Complexity:** Medium
**Comment:** Draft-07 keyword. Used heavily in real-world schemas for polymorphic validation (e.g. "if type is X, require fields A and B"). Builds on existing composition validation infrastructure.
**Tier:** 2

### `contains` / `minContains` / `maxContains` [not started]

**Description:** Validate that an array contains at least one item matching a schema, with optional min/max count bounds.
**Complexity:** Medium
**Comment:** `contains` is Draft-06, `minContains`/`maxContains` added in Draft 2019-09. Needs sub-schema evaluation per array item with counting.
**Tier:** 2

### Color Provider [not started]

**Description:** Detect color values (hex codes, CSS named colors, RGB/HSL expressions) in YAML values and provide color picker integration via `textDocument/documentColor` and `textDocument/colorPresentation`.
**Complexity:** Medium
**Comment:** Format-agnostic LSP feature — applies to any YAML file with color values (theme configs, CI badge definitions, UI settings). Requires regex-based color detection and color format conversion.
**Tier:** 2

### `prefixItems` (Tuple Validation) [not started]

**Description:** Validate array items positionally — each array index validated against a different schema.
**Complexity:** Medium
**Comment:** Draft 2020-12 keyword replacing the Draft-04 tuple form of `items` (when `items` is an array). Needs new field and positional item validation logic.
**Tier:** 3

### `$anchor` / `$dynamicRef` / `$dynamicAnchor` [not started]

**Description:** Support named anchors and dynamic reference resolution across schema documents.
**Complexity:** High
**Comment:** Draft 2019-09 / 2020-12 keywords. `$anchor` names a schema location; `$dynamicRef`/`$dynamicAnchor` enable dynamic dispatch in recursive schemas. Requires changes to `$ref` resolution infrastructure.
**Tier:** 3

### `unevaluatedProperties` / `unevaluatedItems` [not started]

**Description:** Reject properties or items not evaluated by any sub-schema in `allOf`/`anyOf`/`oneOf`/`if`/`then`/`else`.
**Complexity:** High
**Comment:** Draft 2019-09 keywords. Requires tracking which properties/items were "evaluated" during composition — a cross-cutting concern that touches the entire validation walk. The hardest keywords to implement correctly.
**Tier:** 3

### `$vocabulary` [not started]

**Description:** Declare which JSON Schema vocabularies a schema uses, enabling vocabulary-aware validation.
**Complexity:** High
**Comment:** Draft 2019-09 keyword. Meta-schema feature that controls which keyword sets are recognized. Affects schema parsing — requires vocabulary registry and conditional keyword handling.
**Tier:** 3

---

### Proxy Support for Schema Fetching [completed]

**Description:** Allow users to configure an HTTP proxy for schema fetching, supporting corporate environments behind firewalls.
**Complexity:** Low
**Comment:** `ureq` supports proxy configuration. Added `httpProxy` setting plumbed through to the fetch layer.
**Tier:** 3

### Range Formatting [completed]

**Description:** Implement `textDocument/rangeFormatting` to format a selected region of a YAML document.
**Complexity:** Low
**Comment:** Depends on full document formatting infrastructure. Full document formatted internally, requested range lines returned as edits.
**Tier:** 3

### Full Document Formatting [completed]

**Description:** Implement `textDocument/formatting` to reformat entire YAML documents using Wadler-Lindig pretty-printing engine.
**Complexity:** High
**Comment:** Built on `rlsp-fmt`, a workspace crate implementing the Wadler-Lindig algorithm. Configurable via `formatPrintWidth` and `formatSingleQuote` settings.
**Tier:** 3

### SchemaStore Integration [completed]

**Description:** Automatically fetch schema associations from SchemaStore so common file types validate without user configuration.
**Complexity:** Medium
**Comment:** Fetches SchemaStore catalog and matches filenames against `fileMatch` patterns. High user value — most YAML files have a schema on SchemaStore.
**Tier:** 3

### Kubernetes-Aware Schema Resolution [completed]

**Description:** Auto-detect Kubernetes manifests by inspecting `apiVersion` and `kind` fields and fetch the correct schema from yannh/kubernetes-json-schema.
**Complexity:** Medium
**Comment:** Motivated by redhat-developer/yaml-language-server#1213. Resolves version-specific schemas automatically using `kubernetesVersion` setting.
**Tier:** 3

### File Watcher Registration [completed]

**Description:** Register `workspace/didChangeWatchedFiles` capability so the server reacts to file changes without relying on the editor extension.
**Complexity:** Low
**Comment:** Registers for `**/*.yaml` and `**/*.yml` file patterns.
**Tier:** 3

### Schema Association Configuration [completed]

**Description:** More flexible file-to-schema mapping via workspace `schemas` setting with glob patterns.
**Complexity:** Medium
**Comment:** Maps schema URL to glob patterns. Combined with modeline and SchemaStore for multi-source schema resolution.
**Tier:** 3

### Semantic Highlighting [completed]

**Description:** Provide semantic tokens for richer syntax highlighting of keys, values, anchors, aliases, tags, and comments.
**Complexity:** High
**Comment:** Full SemanticTokensProvider protocol — token types, legends, delta encoding. 8 token types, 1 modifier.
**Tier:** 3

### Schema Disable via Modeline [completed]

**Description:** Allow users to disable schema validation for a specific file using `# yaml-language-server: $schema=none`.
**Complexity:** Low
**Comment:** Added sentinel value check to existing modeline parser.
**Tier:** 2

### Hover Formatting Improvements [completed]

**Description:** Format JSON examples in hover with proper indentation instead of single-line JSON stringify output.
**Complexity:** Low
**Comment:** Improved hover display with formatted schema sections, max 3 examples, truncated descriptions.
**Tier:** 2

### Multi-Required Snippet Completion [completed]

**Description:** When completing a key, offer a snippet that inserts all remaining required properties at once with placeholder values.
**Complexity:** Medium
**Comment:** Generates snippets with placeholders for all missing required properties in a single completion item.
**Tier:** 2

### Exclude Deprecated Properties from Completion [completed]

**Description:** Mark deprecated schema properties with strikethrough in completion results.
**Complexity:** Low
**Comment:** Checks the `deprecated` flag in JSON Schema during completion item construction.
**Tier:** 1

### Expected Properties in Diagnostic Messages [completed]

**Description:** List expected property names in "missing required property" diagnostics.
**Complexity:** Low
**Comment:** Enriched `schemaRequired` diagnostic message text with property name list.
**Tier:** 1

### Duplicate Key Detection [completed]

**Description:** Emit diagnostics when duplicate keys are found in a YAML mapping.
**Complexity:** Medium
**Comment:** Text-based detection (saphyr deduplicates in AST). Document-scoped with flow-style and sequence-item scope handling.
**Tier:** 1

### Tab-to-Spaces On-Type Formatting [won't implement]

**Description:** Convert tab characters to spaces on-type.
**Complexity:** Low
**Comment:** Most editors handle tab-to-spaces natively. Risk of conflicting with editor settings.
**Tier:** 4

### Multiple Schemas per File [won't implement]

**Description:** Support different schemas for different documents within a multi-document YAML file or sub-values within a document.
**Complexity:** High
**Comment:** Requires significant architecture changes for per-sub-value schema assignment.
**Tier:** 4

### Embedded Language Support [won't implement]

**Description:** Support syntax highlighting and validation for embedded languages (JSON, SQL, etc.) within YAML string values.
**Complexity:** High
**Comment:** Very high effort — language embedding, delegating to sub-LSPs. Niche use case.
**Tier:** 4

### Localized Validation Messages [won't implement]

**Description:** Internationalize diagnostic messages for user locale support.
**Complexity:** High
**Comment:** Cross-cutting concern touching every diagnostic string. High maintenance cost for a developer-focused tool.
**Tier:** 4
