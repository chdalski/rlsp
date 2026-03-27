**Repository:** root
**Status:** InProgress
**Created:** 2026-03-27

## Goal

Automatically associate YAML files with JSON Schemas from
SchemaStore based on filename patterns. This makes common
file types (GitHub Actions workflows, Docker Compose,
Azure Pipelines, etc.) validate out of the box without any
user configuration.

## Context

- SchemaStore catalog: `https://www.schemastore.org/api/json/catalog.json`
  (~340KB, 1240 schemas, 643 with YAML fileMatch patterns).
- Catalog entry structure: `{ name, url, fileMatch: ["**/*.yml", ...] }`.
- The `fileMatch` patterns use the same glob syntax we
  already support (`*`, `**`).
- Current resolution chain: modeline → workspace glob →
  K8s auto-detect. SchemaStore becomes the fourth and
  lowest-priority fallback.
- Enabled by default. New setting `schemaStore` (bool,
  default `true`) to disable.
- Lazy fetch: catalog is fetched on first need (first YAML
  file opened that reaches the SchemaStore fallback), then
  cached in memory for the session.
- The existing `SchemaCache`, `fetch_schema`,
  `process_schema`, `glob_matches`, and
  `match_schema_by_filename` infrastructure can be reused.
- Key files: `schema.rs` (catalog fetch/parse, matching),
  `server.rs` (integration, settings), `configuration.md`,
  `feature-log.md`.
- Only YAML-relevant entries from the catalog should be
  used (entries where at least one `fileMatch` pattern
  ends in `.yml` or `.yaml`).

## Steps

- [x] Add SchemaStore catalog types and fetch logic (34d135a)
- [x] Add catalog matching function (34d135a)
- [x] Add `schema_store` setting and Backend integration (128583f)
- [x] Write tests (34d135a, 128583f)
- [ ] Update documentation

## Tasks

### Task 1: Catalog fetch, parse, and matching

Add SchemaStore catalog support to `schema.rs`:

1. A struct `SchemaStoreCatalog` that holds parsed
   catalog entries (just `url` + `fileMatch` patterns,
   filtered to YAML-relevant entries only).
2. A function `fetch_schemastore_catalog() ->
   Result<SchemaStoreCatalog, SchemaError>` that fetches
   the catalog JSON from schemastore.org, parses it, and
   filters to entries with at least one YAML fileMatch.
3. A function `match_schemastore(filename: &str,
   catalog: &SchemaStoreCatalog) -> Option<String>` that
   finds the first matching entry by running `glob_matches`
   against the catalog's fileMatch patterns. Returns the
   schema URL.

The catalog JSON structure:
```json
{
  "schemas": [
    {
      "name": "GitHub Workflow",
      "url": "https://www.schemastore.org/github-workflow.json",
      "fileMatch": ["**/.github/workflows/*.yml", ...]
    }
  ]
}
```

Only the `url` and `fileMatch` fields are needed. Other
fields (`name`, `description`, `versions`) can be ignored.

Files: `rlsp-yaml/src/schema.rs`

- [ ] `SchemaStoreCatalog` struct with `entries: Vec<SchemaStoreEntry>`
- [ ] `SchemaStoreEntry` struct: `url: String`, `file_match: Vec<String>`
- [ ] `fetch_schemastore_catalog()` — fetch, parse, filter
- [ ] `match_schemastore()` — match filename against catalog
- [ ] Unit tests: catalog parsing, YAML filtering, matching

### Task 2: Settings and server integration

Wire SchemaStore into the schema resolution pipeline.

Files: `rlsp-yaml/src/server.rs`

- [ ] Add `schema_store: Option<bool>` to `Settings`
      (serde default `None`, treated as `true`)
- [ ] Add `schemastore_catalog: Mutex<Option<SchemaStoreCatalog>>`
      to `Backend` — lazy-initialized on first use
- [ ] Add `get_schema_store_enabled()` helper on `Backend`
- [ ] Add `get_or_fetch_schemastore_catalog()` async method
      on `Backend` — returns cached catalog or fetches on
      first call. On fetch failure, log warning and return
      None (don't block the user).
- [ ] In `parse_and_publish`, after the K8s auto-detect
      block (line ~243), add the fourth fallback: if
      SchemaStore is enabled and no prior match, call
      `get_or_fetch_schemastore_catalog()`, then
      `match_schemastore()`, and if found, pass to
      `process_schema`.
- [ ] Update lock ordering comment to include the new mutex

Note on lock ordering: `schemastore_catalog` should be
acquired after `settings` but before `schema_associations`.
Since the catalog fetch is async (spawn_blocking), no
mutex should be held across the fetch. Pattern: check cache
under lock → if miss, drop lock → fetch → re-acquire lock
→ insert. Same pattern as `process_schema` uses for
`schema_cache`.

### Task 3: Documentation

Update docs to reflect SchemaStore integration.

Files: `rlsp-yaml/docs/configuration.md`,
`rlsp-yaml/docs/feature-log.md`

- [ ] Add `schemaStore` setting to configuration.md
- [ ] Document SchemaStore auto-association behavior,
      priority chain, and how to disable
- [ ] Mark SchemaStore Integration as `[completed]` in
      feature-log.md

## Decisions

- **Enabled by default:** Most users benefit from
  automatic schema association. Users who want full
  control can set `schemaStore: false`.
- **Lazy fetch:** The ~340KB catalog is only downloaded
  when a file actually needs it (no modeline, no glob,
  no K8s match). Avoids startup latency and unnecessary
  network requests.
- **YAML filter:** Only keep catalog entries with at
  least one `.yml`/`.yaml` fileMatch pattern. This
  reduces the in-memory catalog size and avoids matching
  JSON-only schemas against YAML files.
- **Fetch failure is non-fatal:** If the catalog can't
  be fetched (network error, offline), the server
  continues without SchemaStore — no diagnostics are
  lost, the user just doesn't get schema validation for
  that file.
- **No catalog refresh:** The catalog is cached for the
  entire session. Refreshing mid-session would invalidate
  cached schemas and cause inconsistent behavior. A
  server restart picks up catalog updates.
