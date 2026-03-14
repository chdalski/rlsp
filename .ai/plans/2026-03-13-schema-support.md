**Repository:** root
**Status:** Completed (2026-03-13)
**Created:** 2026-03-13
**Author:** Architect

## Goal

Implement JSON Schema support for the rlsp-yaml YAML Language Server. This adds
schema loading, association, validation, completion, and hover — the four parts
described in Task 3 of task_open.txt. Schema support is the most impactful
remaining feature: it transforms the server from a structural YAML tool into a
schema-aware development assistant.

## Context

### Codebase Architecture

The server follows a pure-function design. Each feature module exports a function
that takes text (and optionally a parsed AST) plus a position, and returns LSP
results. The `Backend` struct in `server.rs` orchestrates: it holds a
`DocumentStore` (in-memory cache of document text + parsed YAML ASTs behind a
`Mutex`), a diagnostics cache, and a `Client` for publishing. The
`parse_and_publish` method runs on every open/change, collecting diagnostics from
the parser and validators.

Key patterns:
- Pure functions: `hover_at(text, docs, position) -> Option<Hover>`
- Document store caches both `YamlOwned` and `MarkedYamlOwned` ASTs
- Validators return `Vec<Diagnostic>` and are called in `parse_and_publish`
- All modules use `saphyr` 0.0.6 for YAML parsing
- Capabilities are registered in `Backend::capabilities()`
- tower-lsp v0.20 with `proposed` features, tokio async runtime

### Dependencies

Current: tower-lsp, tokio, serde_json, saphyr, regex, once_cell.
New: `ureq` for HTTP schema fetching (blocking client, use `spawn_blocking`),
`serde` for JSON Schema deserialization.

### Constraints

- Rust edition 2024, clippy pedantic + nursery lints enforced (deny on several)
- 338 tests currently passing
- JSON Schema drafts: Draft-04 + Draft-07
- HTTP client: ureq (approved by user)

### Key Files

- `/workspace/rlsp-yaml/src/server.rs` — Backend, LanguageServer impl, capabilities
- `/workspace/rlsp-yaml/src/parser.rs` — YAML parsing, ParseResult
- `/workspace/rlsp-yaml/src/document_store.rs` — DocumentStore
- `/workspace/rlsp-yaml/src/completion.rs` — `complete_at()` pure function
- `/workspace/rlsp-yaml/src/hover.rs` — `hover_at()` pure function
- `/workspace/rlsp-yaml/src/validators.rs` — diagnostic validators
- `/workspace/rlsp-yaml/src/lib.rs` — module declarations
- `/workspace/rlsp-yaml/Cargo.toml` — dependencies

## Steps

- [x] Add `ureq` and `serde` dependencies to Cargo.toml (committed 130a88a)
- [x] Create `src/schema.rs` — JSON Schema types, parsing, fetching, caching (committed 130a88a)
- [x] Implement schema association (modeline, file patterns, settings) (committed 130a88a)
- [x] Implement schema-driven validation in `src/schema_validation.rs` (committed d5d4ba1)
- [x] Integrate schema validation into `parse_and_publish` in server.rs (committed d5d4ba1)
- [x] Extend completion with schema context in `src/completion.rs` (committed c31fc63)
- [x] Extend hover with schema context in `src/hover.rs` (committed Task 4)
- [x] Wire schema association into Backend (store schema per document) (committed d5d4ba1)
- [x] Register any new capabilities — none needed, existing providers gained schema awareness
- [x] Update lib.rs with new module declarations (committed across Tasks 1-2)

## Tasks

### Task 1: Schema Types, Parsing, and Fetching

Create the foundational `src/schema.rs` module with JSON Schema representation,
parsing, HTTP fetching, and in-memory caching.

**What:**
- Add `ureq` (blocking HTTP) and `serde`/`serde_json` (with derive) to Cargo.toml
- Define a `JsonSchema` struct that represents the subset of JSON Schema needed:
  type, properties, required, enum, description, default, examples, items,
  additionalProperties, allOf/anyOf/oneOf, $ref. Support Draft-04 and Draft-07.
- Parse JSON Schema from `serde_json::Value` into the `JsonSchema` type, resolving
  `$ref` references within the same document.
- Fetch schemas from URLs using `ureq` (via `tokio::task::spawn_blocking`), with
  an in-memory cache (`HashMap<String, JsonSchema>` behind a `Mutex` or similar).
- Implement schema association: parse modeline comments
  (`# yaml-language-server: $schema=<url>`) from document text, support file
  pattern matching (glob patterns mapping to schema URLs).
- Pure function for modeline extraction: `fn extract_schema_url(text: &str) -> Option<String>`

**Acceptance criteria:**
- `JsonSchema` can represent Draft-04 and Draft-07 schemas
- Modeline parsing extracts schema URLs from first-line comments
- Schema fetching retrieves and caches schemas from URLs
- `$ref` resolution works for local references within a schema document
- All new code compiles with zero clippy warnings

### Task 2: Schema-Driven Validation

Create `src/schema_validation.rs` — a pure function that validates a parsed YAML
document against a `JsonSchema` and returns diagnostics.

**What:**
- Pure function: `fn validate_schema(text: &str, docs: &[YamlOwned], schema: &JsonSchema) -> Vec<Diagnostic>`
- Validate: required properties, type mismatches, enum values, additionalProperties
- Produce clear diagnostic messages with property paths (e.g., "Missing required
  property 'apiVersion' at spec.containers[0]")
- Integrate into `parse_and_publish` in `server.rs`: after existing validators,
  if a schema is associated with the document, run schema validation and append
  diagnostics
- Add schema storage to Backend: when a document is opened/changed, check for
  modeline schema URL, fetch/cache the schema, store the association

**Acceptance criteria:**
- Missing required properties produce error diagnostics with paths
- Type mismatches produce error diagnostics
- Enum violations produce error diagnostics listing valid values
- additionalProperties violations produce warning diagnostics
- Schema validation integrates into the existing diagnostic pipeline
- Diagnostics have source "rlsp-yaml" and appropriate codes (e.g., "schemaRequired", "schemaType", "schemaEnum", "schemaAdditionalProperty")

### Task 3: Schema-Driven Completion

Extend the completion provider to suggest schema-defined properties and enum values.

**What:**
- Extend `complete_at()` signature to accept an optional `&JsonSchema`
- When a schema is available:
  - At key positions: suggest property names from the schema for the current path,
    excluding properties already present in the document
  - At value positions: suggest enum values if the schema defines them for that property
  - Include `detail` (type info) and `documentation` (description from schema)
    in completion items
- Fall back to existing structural completion when no schema is available
- Update the `completion` handler in `server.rs` to pass the schema if associated

**Acceptance criteria:**
- Schema properties appear as completion suggestions at key positions
- Enum values appear as completion suggestions at value positions
- Already-present keys are excluded from suggestions
- Completion items include schema descriptions
- Existing structural completion still works when no schema is present
- The `complete_at` API remains backward-compatible (Option parameter)

### Task 4: Schema-Driven Hover

Extend the hover provider to show schema descriptions, types, defaults, and examples.

**What:**
- Extend `hover_at()` signature to accept an optional `&JsonSchema`
- When a schema is available and the cursor is on a known property:
  - Show the property description from the schema
  - Show the expected type
  - Show default value and examples if present
  - Append schema info below existing hover content (path, type, value)
- Fall back to existing structural hover when no schema is available
- Update the `hover` handler in `server.rs` to pass the schema if associated

**Acceptance criteria:**
- Hovering over a schema-known key shows description, type, default, examples
- Hovering over a value shows the property's schema description
- Existing hover behavior preserved when no schema is present
- Schema info is clearly formatted in markdown
- The `hover_at` API remains backward-compatible (Option parameter)

## Decisions

- **HTTP client:** `ureq` — blocking, small dependency footprint. Called via
  `tokio::task::spawn_blocking` to avoid blocking the async runtime. Approved
  by user.
- **JSON Schema drafts:** Draft-04 + Draft-07 — covers the vast majority of
  schemas in schemastore.org. Draft-2019-09 and later deferred.
- **Schema representation:** Custom `JsonSchema` struct deserialized from
  `serde_json::Value`, not a third-party JSON Schema library. Keeps control
  over the subset we support and avoids heavy dependencies.
- **Schema caching:** In-memory `HashMap` behind a `Mutex` in Backend, keyed
  by URL string. No disk caching in this iteration.
- **Module structure:** `schema.rs` for types/loading/association,
  `schema_validation.rs` for validation. Completion and hover extensions stay
  in their existing modules with expanded signatures.
- **Task ordering:** Task 1 (types + fetching) -> Task 2 (validation) -> Task 3
  (completion) -> Task 4 (hover). Each builds on the previous: Task 2 needs the
  schema types from Task 1, Task 3 and 4 need the same types plus the server
  integration from Task 2.
