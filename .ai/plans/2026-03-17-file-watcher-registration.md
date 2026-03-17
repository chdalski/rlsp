**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-17

## Goal

Register `workspace/didChangeWatchedFiles` so the server
reacts to external file changes (schema files modified,
YAML files created/deleted) without relying on the editor
to push notifications via `didOpen`/`didChange`.

## Context

- `server.rs:247-249` — `initialized()` is currently empty
  (just references `self.client`)
- `tower-lsp` 0.20 provides `did_change_watched_files`
  method on the `LanguageServer` trait and
  `client.register_capability()` for dynamic registration
- The main use case: when a schema file referenced by
  workspace associations (Feature 8) changes on disk, the
  server should invalidate the cache entry and re-validate
  open documents
- Secondary use case: when YAML files change externally
  (e.g., git checkout), re-validate open documents
- Lock ordering: document_store → schema_associations →
  schema_cache → diagnostics → settings

## Steps

- [x] Clarify approach with user
- [ ] Implement file watcher registration

## Tasks

### Task 1: Register file watchers and handle changes

1. **server.rs — initialized()**: Use
   `client.register_capability()` to dynamically register
   a `workspace/didChangeWatchedFiles` watcher with glob
   patterns `**/*.yaml` and `**/*.yml`.

   ```rust
   use tower_lsp::lsp_types::{
       Registration, DidChangeWatchedFilesRegistrationOptions,
       FileSystemWatcher, GlobPattern,
   };
   ```

2. **server.rs — did_change_watched_files()**: Implement
   the handler. On any file change event:
   - Iterate all open documents in `document_store`
   - For each open document, call `parse_and_publish`
     to re-validate with potentially updated schemas
   - This is simple but effective — the number of open
     documents is small, and `parse_and_publish` already
     handles schema cache lookups efficiently

   Note: we do NOT need to invalidate the schema cache
   on every file change — schema files are fetched via
   HTTP and cached by URL. If a user changes a local
   schema file, that's a future enhancement (local file
   schemas aren't supported yet). The primary value is
   re-triggering validation when workspace association
   settings or YAML files change externally.

3. **Tests**:
   - Capability test: `initialized` registers watchers
     (may need integration-level test or just verify the
     method compiles and runs)
   - Handler test: verify `did_change_watched_files`
     triggers re-validation of open documents

Files:
- `rlsp-yaml/src/server.rs`

Acceptance criteria:
- [ ] File watchers registered for `**/*.yaml` and
      `**/*.yml` on initialization
- [ ] External file changes trigger re-validation of
      open documents
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Dynamic registration** — use `register_capability` in
  `initialized()` rather than static capability in
  `ServerCapabilities`, since the LSP spec recommends
  dynamic registration for file watchers
- **Re-validate all open docs** — simplest approach; open
  document count is small so performance is fine
- **No schema cache invalidation** — schemas are HTTP-fetched;
  local file schemas are not yet supported
