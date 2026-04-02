# Wire Remote $ref Resolution into Server

**Repository:** root
**Status:** InProgress
**Created:** 2026-04-02

## Goal

Connect the existing remote `$ref` resolution
infrastructure to the server so schemas with non-local
`$ref` URIs are actually resolved. The `ParseContext`,
`SchemaCache`, SSRF guards, breadth limits, and URL dedup
were built in commit `3d10310` but never activated at
runtime — `fetch_schema_raw` calls `parse_schema` (no
remote context) and remote `$ref`s silently fail.

## Context

- Audit report:
  `.ai/reports/2026-04-02-dead-code-and-incomplete-delivery.md`
- `fetch_schema_raw` (schema.rs:410) fetches a schema over
  HTTP and parses it with `parse_schema` — which passes
  `None` for the `ParseContext`, disabling remote `$ref`
  resolution.
- `ParseContext` (schema.rs:616) holds a `&mut SchemaCache`,
  proxy setting, visited-URL set, and fetch counter. When
  passed as `Some(ctx)` to `parse_schema_with_root`, remote
  `$ref`s trigger HTTP fetches during parsing.
- `server.rs` `process_schema` (line 226) calls
  `fetch_schema_raw` inside `spawn_blocking`, then caches
  the result. The server's `SchemaCache` is behind a
  `Mutex<SchemaCache>`.
- **Threading constraint:** `parse_schema_with_remote`
  requires `&mut SchemaCache`. The server holds
  `Mutex<SchemaCache>`. We cannot hold the Mutex across
  `spawn_blocking`. Options:
  1. Take the cache out of the Mutex via
     `std::mem::take`, pass it into the blocking task,
     put it back after. Simple but blocks other documents
     from using the cache during resolution.
  2. Clone the cache before the blocking task, use the
     clone inside, merge new entries back. Memory cost
     of cloning but no contention.
  3. Restructure: separate the fetch+parse into a new
     function that accepts a `&mut SchemaCache` directly,
     called inside `spawn_blocking` after extracting the
     cache.
- The existing security controls are sufficient:
  `MAX_REMOTE_FETCH_COUNT` (20), `MAX_REF_DEPTH` (32),
  SSRF guards, Content-Type validation, response size
  limits. These were approved by the security engineer
  in the original commit.
- After the dead code removal plan,
  `parse_schema_with_remote` will be removed but
  `ParseContext` and all threading remain. The server
  needs to construct a `ParseContext` and pass it through.

## Steps

- [x] Update `fetch_schema_raw` to accept optional
      `ParseContext`
- [x] Update `process_schema` in `server.rs` to pass
      a `ParseContext` when fetching
- [x] Add integration tests for remote `$ref` resolution
- [x] Verify `cargo clippy --all-targets` and `cargo test`

## Tasks

### Task 1: Enable remote $ref in fetch_schema_raw — `632f66e`

Consult the security engineer before implementing — this
task activates remote resource fetching from schema content
that was previously blocked. The security controls exist
but have never been exercised in the production code path.

**Files:** `schema.rs`, `server.rs`

**Approach:** Add an optional `ParseContext` parameter to
`fetch_schema_raw`. When `Some`, use it to parse the
fetched JSON with remote `$ref` resolution enabled. When
`None`, fall back to `parse_schema` (preserving existing
behavior for any callers that don't need remote resolution).

**schema.rs changes:**

Update `fetch_schema_raw` signature:
```rust
pub fn fetch_schema_raw(
    url: &str,
    proxy: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
) -> Result<(Value, JsonSchema), SchemaError>
```

At line 458, replace:
```rust
let schema = parse_schema(&value)?;
```
with:
```rust
let schema = match ctx {
    Some(ctx) => parse_schema_with_root(
        &value, &value, Some(url), Some(ctx), 0,
    ),
    None => parse_schema(&value),
}.ok_or_else(|| SchemaError::ParseFailed(
    "not a JSON Schema".to_string(),
))?;
```

Note: passing `Some(url)` as `base_uri` enables relative
`$ref` resolution against the fetched schema's own URL.

**server.rs changes:**

In `process_schema`, before the `spawn_blocking` call:
1. Take the `SchemaCache` out of the Mutex:
   `let mut cache = std::mem::take(&mut *self.schema_cache.lock()...)`
2. Pass it into the blocking closure along with a new
   `ParseContext`
3. After `spawn_blocking` completes, put the (possibly
   enriched) cache back into the Mutex

This approach is simple and correct. The cache is
unavailable to other documents during the parse, but
schema parsing is fast relative to the HTTP fetch, and
the server already serializes schema processing per
document anyway.

**Verification:**
- [x] `cargo fmt`
- [x] `cargo clippy --all-targets` — zero warnings
- [x] `cargo test` — all 1084 tests pass
- [ ] Manual test: a YAML file with a schema that uses
      remote `$ref` should now get validation, completion,
      and hover for the referenced properties

## Decisions

- **`std::mem::take` approach** — simplest way to get
  `&mut SchemaCache` into `spawn_blocking` without changing
  the Mutex-based architecture. The cache is temporarily
  empty for other documents, but `process_schema` is called
  per-document and schema parsing is fast. If contention
  becomes a problem, the cache can be moved to an
  `Arc<Mutex<>>` with interior locking later.

- **Optional ctx parameter** — keeps `fetch_schema_raw`
  backwards-compatible. Callers that don't need remote
  resolution (like tests) can pass `None`.
