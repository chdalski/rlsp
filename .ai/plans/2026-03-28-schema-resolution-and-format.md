# Schema Resolution and Format Validation

**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-28

## Goal

Complete JSON Schema infrastructure: `$schema` draft
detection, `$id` base URI resolution, remote `$ref`
resolution, `format` validation (all standard formats
including IDN/IRI), and `contentEncoding`/`contentMediaType`
validation. This brings rlsp-yaml to full spec compliance
for schema resolution and string format checking.

## Context

- `resolve_ref` (schema.rs line 942) currently only handles
  local fragment refs (`#/...` and `#anchor`). No remote
  or relative URI resolution.
- `parse_schema_with_root` (line 674) takes `(value, root, depth)`.
  Adding `$id` requires threading a base URI through this
  recursion.
- `fetch_schema` + SSRF guards + `SchemaCache` already
  exist for HTTP schema fetching. Remote `$ref` can reuse
  this infrastructure.
- Three new crates needed:
  - `idna` — IDN hostname validation (UTS46)
  - `iri-string` — IRI/IRI-reference parsing (RFC 3987)
  - `data_encoding` — base64/base32/base16 decode
- Format validation should be toggleable via a
  `formatValidation` workspace setting (default true).
  Draft 2019-09+ treats `format` as annotation-only by
  default, but users can opt into assertion mode.
- Security: `format` validation processes untrusted string
  values from YAML documents against schema-specified
  format names. The format names themselves come from
  schemas (untrusted). Consult security engineer for the
  `$ref` remote resolution task since it fetches external
  resources based on schema content.

## Steps

- [ ] Implement `$schema` draft detection
- [ ] Implement `$id` / `id` base URI resolution
- [ ] Implement remote `$ref` resolution
- [ ] Implement `format` validation (common formats)
- [ ] Implement `format` validation (IDN/IRI formats)
- [ ] Implement `contentEncoding` / `contentMediaType`

## Tasks

### Task 1: `$schema` draft detection

**Files:** `schema.rs`

Parse the `$schema` keyword and store the detected draft.

**New types:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaDraft {
    Draft04,
    Draft06,
    Draft07,
    Draft201909,
    Draft202012,
    #[default]
    Unknown,
}
```

**New field in `JsonSchema`:**
- `draft: SchemaDraft`

**Parsing:** In `parse_schema_with_root`, extract
`"$schema"` string and map known URIs to `SchemaDraft`:
- `http://json-schema.org/draft-04/schema#` → Draft04
- `http://json-schema.org/draft-06/schema#` → Draft06
- `http://json-schema.org/draft-07/schema#` → Draft07
- `https://json-schema.org/draft/2019-09/schema` → Draft201909
- `https://json-schema.org/draft/2020-12/schema` → Draft202012

Only parse at root level (when `depth == 0` or when it's
the first call). Sub-schemas don't have their own `$schema`.

No behavioral changes yet — just parsing and storing.
Future tasks can use the draft to adjust keyword semantics.

- [ ] Add `SchemaDraft` enum and field
- [ ] Parse `$schema` URI → draft mapping
- [ ] Unit tests
- [ ] `cargo clippy` and `cargo test` pass

### Task 2: `$id` / `id` base URI resolution

**Files:** `schema.rs`

Thread a base URI through schema parsing so `$ref` can
resolve relative URIs.

**New field in `JsonSchema`:**
- `id: Option<String>`

**Signature change:** `parse_schema_with_root` gains a
`base_uri: Option<&str>` parameter:
```rust
fn parse_schema_with_root(
    value: &Value,
    root: &Value,
    base_uri: Option<&str>,
    depth: usize,
) -> Option<JsonSchema>
```

**Parsing:**
- Extract `"$id"` (Draft-06+) or `"id"` (Draft-04) string
- If present, resolve it against the current `base_uri`:
  - Absolute URI → new base URI
  - Relative URI → resolve against parent's base URI
- Pass the new base URI to all recursive calls
- Store in `schema.id`

**URI resolution:** Use `url::Url` (already a dependency
via `tower-lsp`) to resolve relative URIs:
```rust
fn resolve_uri(base: Option<&str>, relative: &str) -> Option<String> {
    if let Ok(url) = Url::parse(relative) {
        return Some(url.to_string()); // absolute
    }
    let base_url = Url::parse(base?).ok()?;
    base_url.join(relative).ok().map(|u| u.to_string())
}
```

**Update callers:** All calls to `parse_schema_with_root`
need the new parameter. Most pass `None` or propagate
the parent's base URI. `parse_schema` (the public entry
point) passes `None`.

- [ ] Add `id` field to `JsonSchema`
- [ ] Add `base_uri` parameter to `parse_schema_with_root`
- [ ] Parse `$id` / `id` and resolve against base
- [ ] Propagate base URI through recursive calls
- [ ] Update all callers
- [ ] Unit tests (absolute $id, relative $id, nested $id)
- [ ] `cargo clippy` and `cargo test` pass

### Task 3: Remote `$ref` resolution

Consult the security engineer before implementing — this
task fetches external resources based on untrusted schema
content (`$ref` URIs from remote schemas).

**Files:** `schema.rs`

Extend `resolve_ref` to handle non-fragment refs:

**Current:** Only handles `#...` (fragment-only refs).
**New:** Handle full/relative URIs:
- `https://example.com/schema.json` → fetch + parse
- `defs.json#/definitions/Foo` → resolve against base URI, fetch, parse fragment
- `#/definitions/Foo` → existing local behavior (unchanged)

**Updated `resolve_ref`:**
```rust
fn resolve_ref(
    ref_str: &str,
    root: &Value,
    base_uri: Option<&str>,
    cache: &mut SchemaCache,
    proxy: Option<&str>,
    depth: usize,
) -> Option<JsonSchema>
```

**Logic:**
1. If starts with `#` → existing local resolution
2. Split on `#` → `(uri_part, fragment_part)`
3. Resolve `uri_part` against `base_uri` using `resolve_uri`
4. Fetch the resolved URI via `cache.get_or_fetch(url, proxy)`
5. If `fragment_part` exists, apply JSON Pointer or anchor lookup on fetched schema's raw JSON
6. Return parsed schema

**Threading cache/proxy:** `resolve_ref` needs access to
the schema cache and proxy setting. Thread these through
`parse_schema_with_root` or use a context struct:
```rust
struct ParseContext<'a> {
    root: &'a Value,
    base_uri: Option<String>,
    cache: &'a mut SchemaCache,
    proxy: Option<&'a str>,
}
```

This is a bigger refactor — `parse_schema_with_root` and
all its callees need access to the context. Consider
whether to thread individual params or use the context
struct.

**Security concerns:**
- Remote `$ref` can chain: fetched schema may contain
  further `$ref`s. The existing `MAX_REF_DEPTH` guard
  limits recursion.
- SSRF: `fetch_schema` already validates URLs via
  `validate_and_normalize_url`
- Size: `MAX_SCHEMA_BYTES` already limits response size
- The security engineer should assess whether additional
  controls are needed (e.g., max number of distinct remote
  fetches per validation pass)

- [ ] Consult security engineer
- [ ] Update `resolve_ref` to handle full/relative URIs
- [ ] Thread cache + proxy through parsing (context struct or params)
- [ ] Fetch and parse remote schemas
- [ ] Handle fragment lookup in fetched schemas
- [ ] Unit tests (remote ref, relative ref, chained ref, depth limit)
- [ ] `cargo clippy` and `cargo test` pass

### Task 4: `format` validation (common formats)

**Files:** `schema.rs`, `schema_validation.rs`, `server.rs`

Add `format` keyword parsing and validation for all
non-IDN/IRI formats.

**New fields in `JsonSchema`:**
- `format: Option<String>`

**New setting in `server.rs`:**
- `format_validation: bool` (default `true`), parsed from
  `formatValidation` workspace setting

**Parsing:** Extract `"format"` string in
`parse_schema_with_root`.

**Validation:** Add a `validate_format` function called
from `validate_node` for string nodes:

| Format | Validator |
|--------|----------|
| `date-time` | RFC 3339 regex |
| `date` | `YYYY-MM-DD` regex + day validity |
| `time` | `HH:MM:SS` regex |
| `duration` | ISO 8601 duration regex |
| `email` | Basic RFC 5322 check (local@domain) |
| `ipv4` | `std::net::Ipv4Addr::parse` |
| `ipv6` | `std::net::Ipv6Addr::parse` |
| `hostname` | RFC 1123 label check |
| `uri` | `url::Url::parse` |
| `uri-reference` | `url::Url::parse` or starts with `/`/`#` |
| `uri-template` | RFC 6570 basic check |
| `uuid` | 8-4-4-4-12 hex regex |
| `regex` | `regex::Regex::new` compiles |
| `json-pointer` | Empty or starts with `/` |
| `relative-json-pointer` | Digit prefix + json-pointer |

**Diagnostic code:** `schemaFormat`
**Severity:** WARNING (format validation is advisory)

When `format_validation` setting is false, skip the check
entirely (annotation-only mode).

- [ ] Add `format` field to `JsonSchema`
- [ ] Add `formatValidation` setting
- [ ] Parse format in `parse_schema_with_root`
- [ ] Implement format validators
- [ ] Add `validate_format` call in `validate_node`
- [ ] Unit tests for each format
- [ ] `cargo clippy` and `cargo test` pass

### Task 5: `format` validation (IDN/IRI formats)

**Files:** `schema_validation.rs`, `Cargo.toml`

Add the IDN and IRI format validators using external crates.

**New dependencies in `rlsp-yaml/Cargo.toml`:**
```toml
idna = "1.0"
iri-string = "0.7"
```

**Validators:**

| Format | Implementation |
|--------|---------------|
| `idn-hostname` | `idna::domain_to_ascii(s).is_ok()` |
| `idn-email` | Split on `@`, validate local part, `idna::domain_to_ascii(domain).is_ok()` |
| `iri` | `iri_string::types::IriStr::new(s).is_ok()` |
| `iri-reference` | `iri_string::types::IriReferenceStr::new(s).is_ok()` |

Add these to the existing format validator dispatch.

- [ ] Add `idna` and `iri-string` dependencies
- [ ] Implement IDN format validators
- [ ] Implement IRI format validators
- [ ] Unit tests for each IDN/IRI format
- [ ] `cargo clippy` and `cargo test` pass

### Task 6: `contentEncoding` / `contentMediaType`

**Files:** `schema.rs`, `schema_validation.rs`, `Cargo.toml`

**New dependency:**
```toml
data-encoding = "2.9"
```

**New fields in `JsonSchema`:**
- `content_encoding: Option<String>`
- `content_media_type: Option<String>`

**Parsing:** Extract `"contentEncoding"` and
`"contentMediaType"` strings.

**Validation in `validate_node`** (for string nodes):

1. If `content_encoding` is set:
   - `"base64"` → `data_encoding::BASE64.decode(s.as_bytes()).is_ok()`
   - `"base64url"` → `data_encoding::BASE64URL.decode(s.as_bytes()).is_ok()`
   - `"base32"` → `data_encoding::BASE32.decode(s.as_bytes()).is_ok()`
   - `"base16"` → `data_encoding::HEXUPPER_PERMISSIVE.decode(s.as_bytes()).is_ok()`
   - Unknown encoding → skip (no diagnostic)

2. If `content_media_type` is set (and encoding decoded successfully):
   - `"application/json"` → `serde_json::from_str::<Value>(decoded).is_ok()`
   - Unknown media type → skip

3. If both are set: decode first, then check media type on decoded content.

**Diagnostic codes:**
- `schemaContentEncoding`: string is not valid for declared encoding
- `schemaContentMediaType`: decoded content doesn't match media type

**Draft behavior:** In Draft 2019-09+, these are
annotation-only by default. Use the same `format_validation`
setting to control assertion vs annotation mode, or add a
separate `contentValidation` toggle. For simplicity, tie
it to `formatValidation` since both are "optional assertion"
keywords.

- [ ] Add `data-encoding` dependency
- [ ] Add fields to `JsonSchema`
- [ ] Parse content keywords
- [ ] Implement encoding validators
- [ ] Implement media type validator
- [ ] Combined encoding + media type validation
- [ ] Unit tests
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **`$id` before remote `$ref`:** Base URI resolution is a
  prerequisite for correct relative ref handling. Implement
  in dependency order.

- **Context struct for parsing:** Threading `base_uri`,
  `cache`, and `proxy` as individual params makes the
  signature unwieldy. A `ParseContext` struct is cleaner
  and extensible.

- **Format as WARNING not ERROR:** The JSON Schema spec
  makes `format` optional for assertion. Using WARNING
  lets users see issues without blocking their workflow.

- **`formatValidation` setting controls both `format` and
  `contentEncoding`/`contentMediaType`:** Both are
  "optional assertion" keywords. One toggle is simpler
  than two.

- **External crates for IDN/IRI:** `idna` (UTS46) and
  `iri-string` (RFC 3987) are lightweight, well-maintained,
  and handle the Unicode complexity correctly. Rolling our
  own would be error-prone.

- **`data_encoding` for content encoding:** Proven crate
  with support for all RFC 4648 encodings. Tiny dependency.
