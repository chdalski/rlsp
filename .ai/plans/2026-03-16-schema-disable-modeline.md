**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-16

## Goal

Allow users to disable schema validation for a specific file
using a modeline: `# yaml-language-server: $schema=none`.
When this sentinel is detected, skip all schema processing
(validation, completion, hover schema info, code lens).

## Context

- `schema.rs:548-561` — `extract_schema_url` parses the
  modeline from the first 10 lines, returns `Option<String>`
- `server.rs:117-163` — `parse_and_publish` checks the
  extracted URL, normalizes it, fetches the schema, and runs
  validation
- The simplest approach: check for `"none"` sentinel right
  after `extract_schema_url` returns, before URL validation
- Also need to clear any previously cached schema association
  for the document (if the user adds the modeline to a file
  that previously had a schema)

## Steps

- [x] Clarify approach with user
- [ ] Implement schema disable via modeline

## Tasks

### Task 1: Add "none" sentinel to schema extraction

1. **schema.rs** — In `extract_schema_url`, no change needed.
   The function will return `Some("none")` for the modeline
   `# yaml-language-server: $schema=none`.

2. **server.rs** (~line 117) — In `parse_and_publish`, after
   `extract_schema_url` returns `Some(schema_url)`, check if
   `schema_url == "none"` (case-insensitive). If so:
   - Remove the document from `schema_associations` (so
     previously cached associations don't persist)
   - Skip all schema processing (don't normalize, fetch, or
     validate)
   - The `else` branch (no modeline) already skips schema
     processing, so the "none" case just needs an early
     continue/skip

   ```rust
   if let Some(schema_url) = crate::schema::extract_schema_url(text) {
       if schema_url.eq_ignore_ascii_case("none") {
           // Clear any previous association
           if let Ok(mut assoc) = self.schema_associations.lock() {
               assoc.remove(&uri);
           }
       } else {
           // Existing schema processing...
       }
   }
   ```

3. **Tests** — Add tests:
   - `extract_schema_url` returns `Some("none")` for the
     modeline `# yaml-language-server: $schema=none`
   - Integration-level: document with `$schema=none` produces
     no schema diagnostics even if the YAML has schema errors
   - Case insensitivity: `$schema=None` and `$schema=NONE`

Files:
- `rlsp-yaml/src/schema.rs` (tests only)
- `rlsp-yaml/src/server.rs`

Acceptance criteria:
- [ ] `$schema=none` disables schema validation
- [ ] Case-insensitive matching (none, None, NONE)
- [ ] Previous schema association cleared
- [ ] Non-schema diagnostics still work (anchors, etc.)
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Sentinel value** — `"none"` (case-insensitive), matching
  the convention from the upstream yaml-language-server
- **Check location** — in `parse_and_publish` after extraction,
  before URL normalization; keeps `extract_schema_url` simple
- **Clear association** — remove from `schema_associations` so
  a file that previously had a schema URL doesn't keep its
  old association when the user adds `$schema=none`
