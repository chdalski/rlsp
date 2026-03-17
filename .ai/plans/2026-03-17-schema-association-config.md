**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-17

## Goal

Allow users to configure file-to-schema mappings via
workspace settings (`yaml.schemas`) instead of requiring
a modeline in every file. This enables glob-based schema
association (e.g., all `deploy/*.yaml` files use the
Kubernetes schema) without editing the YAML files themselves.

## Context

- `schema.rs` already has `SchemaAssociation` struct,
  `match_schema_by_filename()`, and `glob_matches()` with
  full test coverage — the matching infrastructure is ready
- `server.rs:28-31` — `Settings` struct currently has
  `custom_tags` and `key_ordering`; needs a `schemas` field
- `server.rs:83-184` — `parse_and_publish` currently only
  resolves schemas via modeline (`extract_schema_url`)
- `server.rs:225-245` — `initialize` and
  `did_change_configuration` deserialize `Settings`
- Modeline must take priority over workspace associations
  (document-specific override wins)
- The `schemas` setting maps glob patterns to schema URLs,
  matching the convention from the upstream yaml-language-server
  (`yaml.schemas` in VS Code settings)

## Steps

- [x] Clarify approach with user
- [ ] Implement schema association configuration

## Tasks

### Task 1: Add workspace schema associations

1. **server.rs — Settings struct** (line 28-31): Add
   `pub schemas: HashMap<String, String>` field. The key is
   the schema URL, the value is the glob pattern (matching
   upstream convention where the setting is
   `{ "schemaUrl": "globPattern" }`). Use `#[serde(default)]`
   so existing configs without the field still deserialize.

2. **server.rs — Backend**: Add a helper method
   `get_schema_associations(&self) -> Vec<SchemaAssociation>`
   that reads `self.settings`, converts the `schemas` HashMap
   into a `Vec<SchemaAssociation>`, and returns it. Acquire
   and release the settings lock before any other lock
   (following the existing lock ordering pattern from
   `get_custom_tags`).

3. **server.rs — parse_and_publish** (~line 117): After the
   existing modeline check, add a fallback. If
   `extract_schema_url` returns `None`, call
   `get_schema_associations()` and then
   `match_schema_by_filename()` with the document URI's
   path. If a match is found, use that schema URL for
   validation (same flow as the modeline path: normalize,
   fetch, cache, validate).

   The structure becomes:
   ```
   if modeline found:
       if "none" → clear association, skip schema
       else → use modeline URL
   else:
       check workspace associations by glob
       if match → use matched URL
   ```

   Extract the shared schema-processing logic (normalize →
   fetch → cache → validate) to avoid duplicating the
   modeline path. A local closure or helper within
   `parse_and_publish` is fine — no need for a new method.

4. **Tests**:
   - Settings deserializes `schemas` field from JSON
   - Settings defaults to empty `schemas` when field missing
   - `get_schema_associations` converts HashMap to Vec
   - Capability test: settings round-trip with schemas
   - Modeline takes priority over workspace association
     (unit test: document with modeline ignores glob match)

Files:
- `rlsp-yaml/src/server.rs`
- `rlsp-yaml/src/schema.rs` (no changes needed — existing
  infrastructure)

Acceptance criteria:
- [ ] `schemas` setting accepted via init options and
      `didChangeConfiguration`
- [ ] Glob patterns match document URIs to schema URLs
- [ ] Modeline takes priority over workspace associations
- [ ] `$schema=none` still disables schema for that file
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Setting format** — `{ "schemas": { "url": "glob" } }`
  matching upstream yaml-language-server convention where
  the schema URL is the key and the glob pattern is the value
- **Priority** — modeline > workspace association. A
  document-level override should always win.
- **No new dependencies** — glob matching already exists
