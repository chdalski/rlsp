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

### YAML 1.1 Compatibility Diagnostics [completed]

**Description:** Warn when plain scalars would be interpreted differently by YAML 1.1 parsers (Kubernetes, Ansible, GitLab CI, etc.). `yaml11Boolean` (warning) fires for the 16 YAML 1.1 boolean forms not in YAML 1.2 (`yes`, `no`, `on`, `off`, `y`, `n`, and case variants); `yaml11Octal` (info) fires for C-style octal literals (`0777`). Schema-aware variants (`schemaYaml11Boolean`, `schemaYaml11Octal`) escalate severity when the field is schema-typed as `string` — the 1.2 parser accepts the value but downstream 1.1 tools will silently corrupt it; `schemaType` messages are enhanced when a boolean-typed field receives a 1.1 boolean form. Quick fixes: quote value (universally safe, listed first) or convert to canonical 1.2 form (`true`/`false`, `0o777`). All four diagnostics are suppressed when `yamlVersion` is `"1.1"`. The VS Code extension now exposes `rlsp-yaml.yamlVersion` and `rlsp-yaml.validate` settings.
**Complexity:** Medium
**Comment:** Novel approach — parse as YAML 1.2 but warn about 1.1 ambiguities rather than switching the parser. Red Hat's yaml-language-server (issue #532, open since Aug 2021) requests similar quick fixes but provides no cross-version diagnostics. Suppression via `# rlsp-yaml-disable-next-line yaml11Boolean` or `# rlsp-yaml-disable-file yaml11Boolean`.
**Tier:** 1

### `$schema` Draft Detection [completed]

**Description:** Parse the `$schema` keyword to detect which JSON Schema draft a schema targets. Surface the draft in hover/code lens.
**Complexity:** Low
**Comment:** Parse the URI, map to a draft enum. Currently all keywords from all drafts are accepted unconditionally, which works but isn't strictly correct.
**Tier:** 1

### `$id` / `id` Base URI Resolution [completed]

**Description:** Parse `$id` (Draft-06+) and `id` (Draft-04) to establish base URIs for relative `$ref` resolution. Thread base URI through schema parsing.
**Complexity:** Medium-High
**Comment:** Foundational for remote `$ref` support. Nested schemas can override the parent's base URI. Requires threading base URI through `parse_schema_with_root`.
**Tier:** 1

### Remote `$ref` Resolution [completed]

**Description:** Resolve `$ref` URIs that point to external schema documents. Fetch, cache, and parse referenced schemas.
**Complexity:** Medium-High
**Comment:** Depends on `$id` for base URI resolution. Fetch infrastructure (`fetch_schema`, SSRF guards, caching) already exists. Work is in URI resolution and cross-document fragment lookup.
**Tier:** 1

### `format` Validation [completed]

**Description:** Validate string values against JSON Schema `format` keywords (date-time, email, uri, ipv4/v6, hostname, uuid, regex, etc.). Includes IDN/IRI formats via `idna` and `iri-string` crates.
**Complexity:** Medium
**Comment:** Most formats are simple regex or stdlib parses. IDN/IRI handled by external crates. Configurable via `formatValidation` setting (default true for Draft-04/07, annotation-only for 2019-09+).
**Tier:** 2

### `contentEncoding` / `contentMediaType` [completed]

**Description:** Validate encoded string content — decode via `contentEncoding` (base64, base32, base16) and check `contentMediaType` (application/json).
**Complexity:** Low
**Comment:** Uses `data_encoding` crate. Only `application/json` media type validated (decode + `serde_json::from_str`). Annotation-only in Draft 2019-09+.
**Tier:** 2

---

### String Constraints (`pattern`, `minLength`, `maxLength`) [completed]

**Description:** Validate string values against `pattern` regex, `minLength`, and `maxLength` from JSON Schema.
**Complexity:** Low
**Comment:** All three fields already parsed into `JsonSchema` struct but never checked in `schema_validation.rs`. Draft-04 keywords.
**Tier:** 1

### Numeric Constraints (`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`) [completed]

**Description:** Validate numeric values against bounds and divisibility constraints.
**Complexity:** Low
**Comment:** `minimum`/`maximum` already parsed but never validated. `exclusiveMinimum`/`exclusiveMaximum` need parsing (boolean in Draft-04, number in Draft-06+). `multipleOf` needs parsing. All Draft-04 keywords with Draft-06 evolution.
**Tier:** 1

### `const` Keyword [completed]

**Description:** Validate that a value matches a single fixed value specified by `const`.
**Complexity:** Low
**Comment:** Draft-06 keyword. Simple equality check against a single JSON value. Very common in real schemas — Kubernetes CRDs use `oneOf` + `const` patterns.
**Tier:** 1

### `not` Keyword [completed]

**Description:** Validate that a value does *not* match the given sub-schema.
**Complexity:** Low
**Comment:** Draft-04 keyword. Invert the validation result of a sub-schema. Straightforward composition — run validation, pass if diagnostics are produced.
**Tier:** 2

### `patternProperties` [completed]

**Description:** Match mapping keys against regex patterns and validate their values against corresponding schemas.
**Complexity:** Medium
**Comment:** Draft-04 keyword. Needs new field in `JsonSchema`, parsing, validation, and completion integration. Common in schemas that allow dynamic key names (e.g. Kubernetes labels, environment variables).
**Tier:** 2

### Array Constraints (`minItems`, `maxItems`, `uniqueItems`) [completed]

**Description:** Validate array length bounds and item uniqueness.
**Complexity:** Low
**Comment:** Draft-04 keywords. Need new fields in `JsonSchema` and straightforward validation checks on sequence length and item equality.
**Tier:** 2

### Object Cardinality (`minProperties`, `maxProperties`) [completed]

**Description:** Validate that a mapping has at least `minProperties` and at most `maxProperties` entries.
**Complexity:** Low
**Comment:** Draft-04 keywords. Mirror of `minItems`/`maxItems` for objects. New fields in `JsonSchema`, parsed in `parse_object_fields`, validated in `validate_mapping` via a `validate_mapping_constraints` helper.
**Tier:** 2

### `additionalItems` (Draft-04/07 Tuple Restriction) [completed]

**Description:** Restrict elements beyond a Draft-04/07 tuple prefix (`items` array form). `false` denies extra elements; a schema validates them.
**Complexity:** Low
**Comment:** Draft-04 keyword. Only active when `items` is an array (tuple mode) — suppressed when `prefixItems` (Draft 2020-12) is used instead. Reuses the `AdditionalProperties` enum. Validation extracted into a `validate_sequence` helper to keep `validate_node` under the line-length lint limit.
**Tier:** 2

### `dependencies` / `dependentRequired` / `dependentSchemas` [completed]

**Description:** Validate cross-property dependencies — when property A is present, require property B or validate against an additional schema.
**Complexity:** Medium
**Comment:** `dependencies` is Draft-04, split into `dependentRequired` and `dependentSchemas` in Draft 2019-09. Need to support both forms for backwards compatibility. Used in schemas with conditional requirements.
**Tier:** 2

### `propertyNames` [completed]

**Description:** Validate that all mapping keys match a given schema (typically a string schema with `pattern`).
**Complexity:** Low
**Comment:** Draft-06 keyword. Applies a schema to each key string in a mapping. Useful for enforcing key naming conventions.
**Tier:** 2

### `if` / `then` / `else` [completed]

**Description:** Conditional schema application — if a value matches the `if` schema, validate against `then`; otherwise validate against `else`.
**Complexity:** Medium
**Comment:** Draft-07 keyword. Used heavily in real-world schemas for polymorphic validation (e.g. "if type is X, require fields A and B"). Builds on existing composition validation infrastructure.
**Tier:** 2

### `contains` / `minContains` / `maxContains` [completed]

**Description:** Validate that an array contains at least one item matching a schema, with optional min/max count bounds.
**Complexity:** Medium
**Comment:** `contains` is Draft-06, `minContains`/`maxContains` added in Draft 2019-09. Needs sub-schema evaluation per array item with counting.
**Tier:** 2

### Color Provider [completed]

**Description:** Detect color values (hex codes, CSS named colors, RGB/HSL expressions) in YAML values and provide color picker integration via `textDocument/documentColor` and `textDocument/colorPresentation`.
**Complexity:** Medium
**Comment:** Format-agnostic LSP feature — applies to any YAML file with color values (theme configs, CI badge definitions, UI settings). Requires regex-based color detection and color format conversion.
**Tier:** 2

### `prefixItems` (Tuple Validation) [completed]

**Description:** Validate array items positionally — each array index validated against a different schema.
**Complexity:** Medium
**Comment:** Draft 2020-12 keyword replacing the Draft-04 tuple form of `items` (when `items` is an array). Needs new field and positional item validation logic.
**Tier:** 3

### `$anchor` / `$dynamicRef` / `$dynamicAnchor` [completed]

**Description:** Support named anchors and dynamic reference resolution across schema documents.
**Complexity:** High
**Comment:** Draft 2019-09 / 2020-12 keywords. `$anchor` names a schema location; `$dynamicRef`/`$dynamicAnchor` enable dynamic dispatch in recursive schemas. Requires changes to `$ref` resolution infrastructure.
**Tier:** 3

### `unevaluatedProperties` / `unevaluatedItems` [completed]

**Description:** Reject properties or items not evaluated by any sub-schema in `allOf`/`anyOf`/`oneOf`/`if`/`then`/`else`.
**Complexity:** High
**Comment:** Draft 2019-09 keywords. Requires tracking which properties/items were "evaluated" during composition — a cross-cutting concern that touches the entire validation walk. The hardest keywords to implement correctly.
**Tier:** 3

### `$vocabulary` [completed]

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
**Comment:** Text-based detection (parsed ASTs deduplicate keys). Document-scoped with flow-style and sequence-item scope handling.
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
