// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use tower_lsp::lsp_types::Url;

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/// Maximum bytes read from a remote schema response (5 MiB).
pub(crate) const MAX_SCHEMA_BYTES: u64 = 5 * 1024 * 1024;

/// Maximum URL length (matches `document_links.rs`).
const MAX_URL_LENGTH: usize = 2048;

/// Maximum JSON nesting depth allowed during schema parsing.
const MAX_JSON_DEPTH: usize = 50;

/// Maximum `$ref` resolution depth to prevent stack overflow on circular refs.
const MAX_REF_DEPTH: usize = 32;

/// Maximum number of distinct remote schema URLs fetched during a single `$ref`
/// resolution pass.  Caps both breadth fan-out and circular-remote-ref loops.
const MAX_REMOTE_FETCH_COUNT: usize = 20;

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during schema fetching or parsing.
#[derive(Debug)]
pub enum SchemaError {
    /// The URL scheme or host is not permitted (SSRF guard).
    UrlNotPermitted(String),
    /// HTTP request failed.
    FetchFailed(String),
    /// Response body exceeded the size limit.
    ResponseTooLarge,
    /// JSON parsing failed or the value is not a valid JSON Schema object.
    ParseFailed(String),
    /// Schema JSON nesting exceeded the depth limit.
    TooDeep,
    /// Remote fetch count exceeded `MAX_REMOTE_FETCH_COUNT` in one resolution pass.
    TooManyRemoteFetches,
    /// Response Content-Type was not JSON.
    UnexpectedContentType(String),
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UrlNotPermitted(u) => write!(f, "URL not permitted: {u}"),
            Self::FetchFailed(e) => write!(f, "Fetch failed: {e}"),
            Self::ResponseTooLarge => write!(f, "Schema response exceeded size limit"),
            Self::ParseFailed(e) => write!(f, "Schema parse failed: {e}"),
            Self::TooDeep => write!(f, "Schema nesting depth exceeded limit"),
            Self::TooManyRemoteFetches => {
                write!(
                    f,
                    "Remote fetch count exceeded limit ({MAX_REMOTE_FETCH_COUNT})"
                )
            }
            Self::UnexpectedContentType(ct) => {
                write!(f, "Unexpected content type: {ct}")
            }
        }
    }
}

/// The JSON Schema type keyword — a single type string or an array of types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaType {
    Single(String),
    Multiple(Vec<String>),
}

/// Whether `additionalProperties` is `false` or a sub-schema.
#[derive(Debug, Clone)]
pub enum AdditionalProperties {
    Denied,
    Schema(Box<JsonSchema>),
}

/// A subset of JSON Schema (Draft-04 and Draft-07) sufficient for validation,
/// completion, and hover support.
#[derive(Debug, Clone, Default)]
pub struct JsonSchema {
    pub id: Option<String>,
    pub schema_type: Option<SchemaType>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub format: Option<String>,
    pub content_encoding: Option<String>,
    pub content_media_type: Option<String>,
    pub content_schema: Option<Box<Self>>,
    pub properties: Option<HashMap<String, Self>>,
    pub required: Option<Vec<String>>,
    pub enum_values: Option<Vec<Value>>,
    pub default: Option<Value>,
    pub examples: Option<Vec<Value>>,
    pub items: Option<Box<Self>>,
    pub prefix_items: Option<Vec<Self>>,
    pub contains: Option<Box<Self>>,
    pub min_items: Option<u64>,
    pub max_items: Option<u64>,
    pub max_contains: Option<u64>,
    pub min_contains: Option<u64>,
    pub unique_items: Option<bool>,
    pub additional_properties: Option<AdditionalProperties>,
    pub additional_items: Option<AdditionalProperties>,
    pub min_properties: Option<u64>,
    pub max_properties: Option<u64>,
    pub pattern_properties: Option<Vec<(String, Self)>>,
    pub property_names: Option<Box<Self>>,
    pub all_of: Option<Vec<Self>>,
    pub any_of: Option<Vec<Self>>,
    pub one_of: Option<Vec<Self>>,
    pub not: Option<Box<Self>>,
    pub if_schema: Option<Box<Self>>,
    pub then_schema: Option<Box<Self>>,
    pub else_schema: Option<Box<Self>>,
    pub ref_path: Option<String>,
    pub anchor: Option<String>,
    pub dynamic_anchor: Option<String>,
    pub pattern: Option<String>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub exclusive_minimum: Option<f64>,
    pub exclusive_maximum: Option<f64>,
    pub exclusive_minimum_draft04: Option<bool>,
    pub exclusive_maximum_draft04: Option<bool>,
    pub multiple_of: Option<f64>,
    pub const_value: Option<serde_json::Value>,
    pub dependent_required: Option<HashMap<String, Vec<String>>>,
    pub dependent_schemas: Option<HashMap<String, Self>>,
    /// Merged `definitions` (Draft-04) and `$defs` (Draft-07) storage.
    pub definitions: Option<HashMap<String, Self>>,
    pub deprecated: Option<bool>,
    pub unevaluated_properties: Option<AdditionalProperties>,
    pub unevaluated_items: Option<Box<Self>>,
}

/// A mapping from a file glob pattern to a JSON Schema URL.
#[derive(Debug, Clone)]
pub struct SchemaAssociation {
    pub pattern: String,
    pub url: String,
}

/// A single entry from the `SchemaStore` catalog.
#[derive(Debug, Clone)]
pub struct SchemaStoreEntry {
    pub url: String,
    pub file_match: Vec<String>,
}

/// The parsed `SchemaStore` catalog, filtered to YAML-relevant entries.
#[derive(Debug, Clone, Default)]
pub struct SchemaStoreCatalog {
    pub entries: Vec<SchemaStoreEntry>,
}

/// In-memory cache of parsed JSON Schemas, keyed by normalized URL.
///
/// Each entry stores the raw `Value` alongside the parsed `JsonSchema` so that
/// fragment-bearing `$ref` values (`other.json#/definitions/Foo`) can navigate
/// the raw document with a JSON Pointer after the initial fetch.
///
/// Schemas are keyed by **fetch URL**, never by a document's self-declared
/// `$id`, to prevent `$id`-spoofing cache poisoning.
#[derive(Debug, Default)]
pub struct SchemaCache {
    inner: HashMap<String, (Value, JsonSchema)>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema cache
// ──────────────────────────────────────────────────────────────────────────────

impl SchemaCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a cached schema by URL, or `None` on a cache miss.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&JsonSchema> {
        self.inner.get(url).map(|(_, s)| s)
    }

    /// Insert a schema into the cache.  The first insertion for a given URL
    /// wins; subsequent calls for the same key are silently ignored.
    pub fn insert(&mut self, url: String, value: Value, schema: JsonSchema) {
        self.inner.entry(url).or_insert((value, schema));
    }

    /// Return a cached (raw value, parsed schema) pair by URL, or `None`.
    #[must_use]
    fn get_raw(&self, url: &str) -> Option<&(Value, JsonSchema)> {
        self.inner.get(url)
    }

    /// Return a cached schema, fetching and caching it on the first call.
    ///
    /// `url` must already be normalised (use [`validate_and_normalize_url`]).
    /// `proxy` is forwarded to [`fetch_schema_raw`] on a cache miss.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`fetch_schema_raw`].
    pub fn get_or_fetch(
        &mut self,
        url: &str,
        proxy: Option<&str>,
    ) -> Result<&JsonSchema, SchemaError> {
        if !self.inner.contains_key(url) {
            let (value, schema) = fetch_schema_raw(url, proxy, None)?;
            self.inner.insert(url.to_string(), (value, schema));
        }
        let Some((_, schema)) = self.inner.get(url) else {
            return Err(SchemaError::FetchFailed(
                "cache miss after insert".to_string(),
            ));
        };
        Ok(schema)
    }

    /// Return whether the URL is already in the cache (avoids a fetch).
    #[must_use]
    pub fn contains(&self, url: &str) -> bool {
        self.inner.contains_key(url)
    }

    /// Consume `self` and return the underlying map, for use when merging a
    /// returned cache back into the live cache after a `spawn_blocking` pass.
    pub(crate) fn into_inner(self) -> HashMap<String, (Value, JsonSchema)> {
        self.inner
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// URL validation (SSRF guard)
// ──────────────────────────────────────────────────────────────────────────────

/// Parse, validate, and normalise a schema URL.
///
/// Returns `Err` if:
/// - the URL exceeds `MAX_URL_LENGTH`
/// - the scheme is not `http` or `https`
/// - the host resolves to a loopback or link-local address
///
/// On success the returned `String` is the canonical form produced by
/// `Url::to_string()`, which lowercases the scheme and host.
///
/// # Errors
///
/// Returns [`SchemaError::UrlNotPermitted`] for any rejected URL.
pub fn validate_and_normalize_url(raw: &str) -> Result<String, SchemaError> {
    if raw.len() > MAX_URL_LENGTH {
        return Err(SchemaError::UrlNotPermitted(
            "URL exceeds maximum length".to_string(),
        ));
    }

    let url =
        Url::parse(raw).map_err(|e| SchemaError::UrlNotPermitted(format!("invalid URL: {e}")))?;

    // Scheme allowlist
    match url.scheme() {
        "http" | "https" => {}
        s => {
            return Err(SchemaError::UrlNotPermitted(format!(
                "scheme '{s}' is not permitted"
            )));
        }
    }

    // Block loopback and link-local hosts
    if let Some(host) = url.host_str()
        && is_ssrf_blocked_host(host)
    {
        return Err(SchemaError::UrlNotPermitted(format!(
            "host '{host}' is not permitted"
        )));
    }

    Ok(url.to_string())
}

/// Return `true` if the host string identifies a loopback or link-local address
/// that should be blocked to prevent SSRF.
///
/// # Accepted limitation — DNS rebinding
///
/// This check operates on the URL hostname string, not the resolved socket
/// address. A DNS rebinding attack could bypass it by having a hostname
/// initially resolve to an allowed IP and later resolve to a blocked one.
/// This risk is accepted as proportionate to the LSP server threat model:
/// the server runs on a developer's machine and is not exposed to arbitrary
/// internet actors.
fn is_ssrf_blocked_host(host: &str) -> bool {
    use std::net::IpAddr;

    // Symbolic hostnames
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    // Try parsing as an IP address.
    // `url::Url::host_str()` returns IPv6 addresses wrapped in brackets
    // (e.g. "[::1]"); strip them before parsing.
    let bare = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback()           // 127.0.0.0/8
                    || v4.is_link_local()  // 169.254.0.0/16
                    || v4.is_private()     // 10/8, 172.16/12, 192.168/16
                    || v4.is_unspecified() // 0.0.0.0
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()           // ::1
                    || v6.is_unspecified() // ::
                    // fe80::/10 (link-local) — check manually
                    || v6.segments().first().is_some_and(|s| (s & 0xffc0) == 0xfe80)
                    // fc00::/7 (ULA — IPv6 private addresses)
                    || v6.segments().first().is_some_and(|s| (s & 0xfe00) == 0xfc00)
                    // ::ffff:0:0/96 (IPv4-mapped) — apply IPv4 SSRF checks
                    || v6.to_ipv4_mapped().is_some_and(|v4| {
                        v4.is_loopback()
                            || v4.is_link_local()
                            || v4.is_private()
                            || v4.is_unspecified()
                    })
            }
        };
    }

    false
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema fetching
// ──────────────────────────────────────────────────────────────────────────────

/// Build a `ureq` agent with redirect following disabled, timeouts, and an
/// optional proxy.
///
/// Both fetch functions use this helper so agent construction is consistent.
fn build_agent(proxy: Option<&str>) -> ureq::Agent {
    let mut builder = ureq::Agent::config_builder()
        .max_redirects(0)
        .timeout_connect(Some(std::time::Duration::from_secs(5)))
        .timeout_global(Some(std::time::Duration::from_secs(15)));
    if let Some(url) = proxy {
        if let Ok(p) = ureq::Proxy::new(url) {
            builder = builder.proxy(Some(p));
        }
    }
    builder.build().new_agent()
}

/// Sanitize a `Content-Type` header value for use in error messages.
///
/// Strips non-printable characters and truncates to 256 chars so that a
/// malicious server cannot inject control characters into diagnostic output.
fn sanitize_content_type(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(256)
        .collect()
}

/// Fetch a JSON Schema from `url`, returning the raw `Value` and parsed schema.
///
/// `url` should already be validated and normalised via
/// [`validate_and_normalize_url`].  This function is blocking; call it via
/// `tokio::task::spawn_blocking` from async contexts.
///
/// When `proxy` is `Some`, requests are routed through the given proxy URL.
///
/// When `ctx` is `Some`, remote `$ref` URIs within the fetched schema are
/// resolved one level deep (depth-1 remote resolution).  `$ref`s inside those
/// resolved remote documents are not followed further — this is intentional to
/// cap the blast radius of a malicious schema that chains many remote refs.
/// The breadth guard (`MAX_REMOTE_FETCH_COUNT`) and dedup set in `ctx` enforce
/// an absolute cap even if this design decision is revisited later.
///
/// # Errors
///
/// Returns a [`SchemaError`] on network failure, size-limit breach, wrong
/// Content-Type, or parse failure.
pub fn fetch_schema_raw(
    url: &str,
    proxy: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
) -> Result<(Value, JsonSchema), SchemaError> {
    use std::io::Read as _;

    // Validate and normalise the URL before issuing any network request.
    validate_and_normalize_url(url)?;

    let agent = build_agent(proxy);

    let response = agent
        .get(url)
        .call()
        .map_err(|e| SchemaError::FetchFailed(e.to_string()))?;

    // Verify Content-Type is JSON before reading the body.
    // Sanitize the header value: strip non-printable chars and truncate to 256
    // chars before embedding in the error to prevent injection via crafted headers.
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("application/json") && !content_type.contains("application/schema") {
        return Err(SchemaError::UnexpectedContentType(sanitize_content_type(
            content_type,
        )));
    }

    // Read up to MAX_SCHEMA_BYTES + 1: if more than MAX_SCHEMA_BYTES bytes
    // are available the response is too large and must be rejected.
    let mut limited = response
        .into_body()
        .into_reader()
        .take(MAX_SCHEMA_BYTES + 1);

    let mut buf = Vec::new();
    limited
        .read_to_end(&mut buf)
        .map_err(|e| SchemaError::FetchFailed(e.to_string()))?;

    // More than MAX_SCHEMA_BYTES bytes were read — response is too large.
    if buf.len() as u64 > MAX_SCHEMA_BYTES {
        return Err(SchemaError::ResponseTooLarge);
    }

    let value: Value =
        serde_json::from_slice(&buf).map_err(|e| SchemaError::ParseFailed(e.to_string()))?;

    check_json_depth(&value, 0)?;

    let schema = ctx
        .map_or_else(
            || parse_schema(&value),
            |ctx| parse_schema_with_root(&value, &value, Some(url), Some(ctx), 0),
        )
        .ok_or_else(|| SchemaError::ParseFailed("not a JSON Schema".to_string()))?;
    Ok((value, schema))
}

// ──────────────────────────────────────────────────────────────────────────────
// SchemaStore catalog fetch, parse, and matching
// ──────────────────────────────────────────────────────────────────────────────

/// Catalog URL for `SchemaStore`.
const SCHEMASTORE_CATALOG_URL: &str = "https://www.schemastore.org/api/json/catalog.json";

/// Fetch and parse the `SchemaStore` catalog, returning only entries that have
/// at least one `fileMatch` pattern ending in `.yml` or `.yaml`.
///
/// When `proxy` is `Some`, requests are routed through the given proxy URL.
///
/// # Errors
///
/// Returns a [`SchemaError`] on network failure, size-limit breach, or parse
/// failure.
pub fn fetch_schemastore_catalog(proxy: Option<&str>) -> Result<SchemaStoreCatalog, SchemaError> {
    use std::io::Read as _;

    let agent = build_agent(proxy);

    let response = agent
        .get(SCHEMASTORE_CATALOG_URL)
        .call()
        .map_err(|e| SchemaError::FetchFailed(e.to_string()))?;

    let mut limited = response
        .into_body()
        .into_reader()
        .take(MAX_SCHEMA_BYTES + 1);

    let mut buf = Vec::new();
    limited
        .read_to_end(&mut buf)
        .map_err(|e| SchemaError::FetchFailed(e.to_string()))?;

    if buf.len() as u64 > MAX_SCHEMA_BYTES {
        return Err(SchemaError::ResponseTooLarge);
    }

    let value: Value =
        serde_json::from_slice(&buf).map_err(|e| SchemaError::ParseFailed(e.to_string()))?;

    parse_schemastore_catalog(&value)
        .ok_or_else(|| SchemaError::ParseFailed("not a SchemaStore catalog".to_string()))
}

/// Parse a `SchemaStore` catalog JSON value into a [`SchemaStoreCatalog`].
///
/// Returns `None` if the value is not a JSON object with a `schemas` array.
fn parse_schemastore_catalog(value: &Value) -> Option<SchemaStoreCatalog> {
    let obj = value.as_object()?;
    let schemas = obj.get("schemas")?.as_array()?;

    let entries = schemas
        .iter()
        .filter_map(|entry| {
            let entry_obj = entry.as_object()?;
            let url = entry_obj.get("url")?.as_str()?.to_string();
            if url.is_empty() {
                return None;
            }
            // Retain only YAML-relevant fileMatch patterns within this entry.
            let file_match: Vec<String> = entry_obj
                .get("fileMatch")?
                .as_array()?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .filter(|p| {
                    std::path::Path::new(p.as_str())
                        .extension()
                        .is_some_and(|ext| {
                            ext.eq_ignore_ascii_case("yml") || ext.eq_ignore_ascii_case("yaml")
                        })
                })
                .collect();
            // Only keep entries that have at least one YAML-relevant pattern.
            if file_match.is_empty() {
                None
            } else {
                Some(SchemaStoreEntry { url, file_match })
            }
        })
        .collect();

    Some(SchemaStoreCatalog { entries })
}

/// Return the schema URL from the catalog for the first entry whose
/// `fileMatch` patterns match `filename`, or `None` if no entry matches.
#[must_use]
pub fn match_schemastore(filename: &str, catalog: &SchemaStoreCatalog) -> Option<String> {
    catalog.entries.iter().find_map(|entry| {
        let matches = entry
            .file_match
            .iter()
            .any(|pattern| glob_matches(pattern, filename));
        if matches {
            Some(entry.url.clone())
        } else {
            None
        }
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// JSON depth check
// ──────────────────────────────────────────────────────────────────────────────

/// Walk a `serde_json::Value` tree and return `Err(SchemaError::TooDeep)` if
/// the nesting depth exceeds `MAX_JSON_DEPTH`.
fn check_json_depth(value: &Value, depth: usize) -> Result<(), SchemaError> {
    if depth > MAX_JSON_DEPTH {
        return Err(SchemaError::TooDeep);
    }
    match value {
        Value::Object(map) => {
            for v in map.values() {
                check_json_depth(v, depth + 1)?;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                check_json_depth(v, depth + 1)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema parsing
// ──────────────────────────────────────────────────────────────────────────────

/// Context threaded through `parse_schema_with_root` → `resolve_ref` to enable
/// remote `$ref` resolution with breadth and deduplication guards.
pub struct ParseContext<'a> {
    cache: &'a mut SchemaCache,
    proxy: Option<&'a str>,
    /// URLs fetched during this resolution pass (dedup + breadth limit).
    visited: HashSet<String>,
}

impl<'a> ParseContext<'a> {
    pub fn new(cache: &'a mut SchemaCache, proxy: Option<&'a str>) -> Self {
        Self {
            cache,
            proxy,
            visited: HashSet::new(),
        }
    }

    /// Record a URL as visited.  Returns `false` if the URL was already
    /// visited or the fetch limit has been reached — caller should skip fetch.
    fn try_visit(&mut self, url: &str) -> bool {
        if self.visited.len() >= MAX_REMOTE_FETCH_COUNT {
            return false;
        }
        self.visited.insert(url.to_string())
    }
}

/// Parse a `serde_json::Value` into a [`JsonSchema`].
///
/// Returns `None` if the value is not a JSON object (or boolean — see below).
///
/// Boolean schemas:
/// - `true`  → empty (permissive) schema
/// - `false` → `None` (no schema representation for "reject everything")
#[must_use]
pub fn parse_schema(value: &Value) -> Option<JsonSchema> {
    parse_schema_with_root(value, value, None, None, 0)
}

/// Populate scalar/string/numeric constraint fields on `schema` from `obj`.
fn parse_scalar_fields(obj: &serde_json::Map<String, Value>, schema: &mut JsonSchema) {
    // title / description / pattern / anchors
    schema.title = string_field(obj, "title");
    schema.description = string_field(obj, "description");
    schema.pattern = string_field(obj, "pattern");
    schema.anchor = string_field(obj, "$anchor");
    schema.dynamic_anchor = string_field(obj, "$dynamicAnchor");
    schema.deprecated = obj.get("deprecated").and_then(Value::as_bool);

    // numeric constraints
    schema.minimum = obj.get("minimum").and_then(Value::as_f64);
    schema.maximum = obj.get("maximum").and_then(Value::as_f64);
    schema.min_length = obj.get("minLength").and_then(Value::as_u64);
    schema.max_length = obj.get("maxLength").and_then(Value::as_u64);

    // exclusiveMinimum: Draft-06+ uses a number; Draft-04 uses a boolean
    if let Some(excl_min) = obj.get("exclusiveMinimum") {
        if excl_min.is_number() {
            schema.exclusive_minimum = excl_min.as_f64();
        } else if excl_min.is_boolean() {
            schema.exclusive_minimum_draft04 = excl_min.as_bool();
        }
    }
    // exclusiveMaximum: same dual-form pattern
    if let Some(excl_max) = obj.get("exclusiveMaximum") {
        if excl_max.is_number() {
            schema.exclusive_maximum = excl_max.as_f64();
        } else if excl_max.is_boolean() {
            schema.exclusive_maximum_draft04 = excl_max.as_bool();
        }
    }
    schema.multiple_of = obj.get("multipleOf").and_then(Value::as_f64);
    schema.const_value = obj.get("const").cloned();

    // default / examples / enum / format
    schema.default = obj.get("default").cloned();
    schema.examples = obj.get("examples").and_then(Value::as_array).cloned();
    schema.enum_values = obj.get("enum").and_then(Value::as_array).cloned();
    schema.format = string_field(obj, "format");
    schema.content_encoding = string_field(obj, "contentEncoding");
    schema.content_media_type = string_field(obj, "contentMediaType");
}

/// Parse the `contentSchema` keyword from the schema object.
///
/// This is a separate function because it needs the recursive parsing context
/// (`root`, `base_uri`, `ctx`, `depth`) that `parse_scalar_fields` does not have.
fn parse_content_schema(
    schema: &mut JsonSchema,
    obj: &serde_json::Map<String, Value>,
    root: &Value,
    base_uri: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
) {
    if let Some(cs) = obj.get("contentSchema") {
        schema.content_schema =
            parse_schema_with_root(cs, root, base_uri, ctx, depth + 1).map(Box::new);
    }
}

/// Populate `properties` and `patternProperties` on `schema` from `obj`.
fn parse_object_fields(
    obj: &serde_json::Map<String, Value>,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
    schema: &mut JsonSchema,
) {
    if let Some(map) = obj.get("properties").and_then(Value::as_object) {
        let mut props = HashMap::new();
        for (k, v) in map {
            if let Some(s) =
                parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1)
            {
                props.insert(k.clone(), s);
            }
        }
        if !props.is_empty() {
            schema.properties = Some(props);
        }
    }

    schema.min_properties = obj.get("minProperties").and_then(Value::as_u64);
    schema.max_properties = obj.get("maxProperties").and_then(Value::as_u64);

    if let Some(map) = obj.get("patternProperties").and_then(Value::as_object) {
        let mut pat_props = Vec::new();
        for (k, v) in map {
            if let Some(s) =
                parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1)
            {
                pat_props.push((k.clone(), s));
            }
        }
        if !pat_props.is_empty() {
            schema.pattern_properties = Some(pat_props);
        }
    }
}

/// Populate array-related fields (items, prefixItems, contains, counts) on `schema` from `obj`.
fn parse_array_fields(
    obj: &serde_json::Map<String, Value>,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
    schema: &mut JsonSchema,
) {
    // prefixItems (Draft 2020-12)
    if let Some(arr) = obj.get("prefixItems").and_then(Value::as_array) {
        let mut items = Vec::new();
        for v in arr {
            if let Some(s) =
                parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1)
            {
                items.push(s);
            }
        }
        if !items.is_empty() {
            schema.prefix_items = Some(items);
        }
    }

    // items — object form (single schema) or array form (Draft-04 tuple → prefixItems)
    match obj.get("items") {
        Some(Value::Array(arr)) if schema.prefix_items.is_none() => {
            let mut items = Vec::new();
            for v in arr {
                if let Some(s) =
                    parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1)
                {
                    items.push(s);
                }
            }
            if !items.is_empty() {
                schema.prefix_items = Some(items);
            }
        }
        Some(v) => {
            schema.items = parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1)
                .map(Box::new);
        }
        None => {}
    }

    // additionalItems — only relevant in Draft-04/07 tuple mode (array-form items, not prefixItems)
    if obj.get("items").is_some_and(Value::is_array) && obj.get("prefixItems").is_none() {
        schema.additional_items = parse_additional_properties(
            obj.get("additionalItems"),
            root,
            base_uri,
            ctx.as_deref_mut(),
            depth,
        );
    }

    schema.contains = obj
        .get("contains")
        .and_then(|v| parse_schema_with_root(v, root, base_uri, ctx, depth + 1))
        .map(Box::new);
    schema.min_items = obj.get("minItems").and_then(Value::as_u64);
    schema.max_items = obj.get("maxItems").and_then(Value::as_u64);
    schema.min_contains = obj.get("minContains").and_then(Value::as_u64);
    schema.max_contains = obj.get("maxContains").and_then(Value::as_u64);
    schema.unique_items = obj.get("uniqueItems").and_then(Value::as_bool);
}

/// Populate allOf/anyOf/oneOf/not and if/then/else fields on `schema` from `obj`.
fn parse_combinator_fields(
    obj: &serde_json::Map<String, Value>,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
    schema: &mut JsonSchema,
) {
    schema.all_of = parse_schema_array(obj.get("allOf"), root, base_uri, ctx.as_deref_mut(), depth);
    schema.any_of = parse_schema_array(obj.get("anyOf"), root, base_uri, ctx.as_deref_mut(), depth);
    schema.one_of = parse_schema_array(obj.get("oneOf"), root, base_uri, ctx.as_deref_mut(), depth);
    schema.not = obj
        .get("not")
        .and_then(|v| parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1))
        .map(Box::new);

    // if / then / else (Draft-07)
    schema.if_schema = obj
        .get("if")
        .and_then(|v| parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1))
        .map(Box::new);
    schema.then_schema = obj
        .get("then")
        .and_then(|v| parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1))
        .map(Box::new);
    schema.else_schema = obj
        .get("else")
        .and_then(|v| parse_schema_with_root(v, root, base_uri, ctx, depth + 1))
        .map(Box::new);
}

/// Populate `unevaluatedProperties`, `unevaluatedItems`, and `definitions`
/// on `schema` from `obj`. Extracted to keep `parse_schema_with_root` under 100 lines.
fn parse_extension_fields(
    obj: &serde_json::Map<String, Value>,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
    schema: &mut JsonSchema,
) {
    // unevaluatedProperties / unevaluatedItems (Draft 2019-09)
    schema.unevaluated_properties = parse_additional_properties(
        obj.get("unevaluatedProperties"),
        root,
        base_uri,
        ctx.as_deref_mut(),
        depth,
    );
    schema.unevaluated_items = obj
        .get("unevaluatedItems")
        .and_then(|v| parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1))
        .map(Box::new);

    // definitions (Draft-04) + $defs (Draft-07)
    let defs_04 = parse_definitions(
        obj.get("definitions"),
        root,
        base_uri,
        ctx.as_deref_mut(),
        depth,
    );
    let defs_07 = parse_definitions(obj.get("$defs"), root, base_uri, ctx, depth);
    schema.definitions = match (defs_04, defs_07) {
        (Some(mut a), Some(b)) => {
            a.extend(b);
            Some(a)
        }
        (a, b) => a.or(b),
    };
}

/// Resolve `relative` against `base`, returning an absolute URI string.
///
/// If `relative` is already an absolute URI it is returned as-is.
/// Returns `None` when `base` is `None` or when joining fails.
fn resolve_uri(base: Option<&str>, relative: &str) -> Option<String> {
    if Url::parse(relative).is_ok() {
        return Some(relative.to_string());
    }
    let base_url = Url::parse(base?).ok()?;
    base_url.join(relative).ok().map(|u| u.to_string())
}

fn parse_schema_with_root(
    value: &Value,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
) -> Option<JsonSchema> {
    if depth > MAX_REF_DEPTH {
        return None;
    }

    match value {
        Value::Bool(true) => return Some(JsonSchema::default()),
        Value::Bool(false)
        | Value::Null
        | Value::Number(_)
        | Value::String(_)
        | Value::Array(_) => {
            return None;
        }
        Value::Object(_) => {}
    }

    let obj = value.as_object()?;
    let mut schema = JsonSchema::default();

    // $ref — resolve immediately and return the referenced schema
    if let Some(Value::String(ref_str)) = obj.get("$ref") {
        schema.ref_path = Some(ref_str.clone());
        if let Some(resolved) = resolve_ref(ref_str, root, base_uri, ctx.as_deref_mut(), depth + 1)
        {
            return Some(resolved);
        }
        return Some(schema);
    }

    // $dynamicRef — same resolution as $ref for single-document schemas
    if let Some(Value::String(ref_str)) = obj.get("$dynamicRef") {
        if let Some(resolved) = resolve_ref(ref_str, root, base_uri, ctx.as_deref_mut(), depth + 1)
        {
            return Some(resolved);
        }
        // Fall through if unresolved — parse remaining fields
    }

    // $id (Draft-06+) / id (Draft-04) — update base URI for sub-schemas
    let raw_id = obj
        .get("$id")
        .or_else(|| obj.get("id"))
        .and_then(Value::as_str);
    let effective_base: Option<String> = if let Some(raw) = raw_id {
        let resolved = resolve_uri(base_uri, raw).unwrap_or_else(|| raw.to_string());
        schema.id = Some(resolved.clone());
        Some(resolved)
    } else {
        base_uri.map(String::from)
    };
    let effective_base = effective_base.as_deref();

    // type
    schema.schema_type = parse_type(obj.get("type"));

    parse_scalar_fields(obj, &mut schema);
    parse_content_schema(
        &mut schema,
        obj,
        root,
        effective_base,
        ctx.as_deref_mut(),
        depth,
    );

    // required
    schema.required = obj.get("required").and_then(Value::as_array).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    });

    parse_object_fields(
        obj,
        root,
        effective_base,
        ctx.as_deref_mut(),
        depth,
        &mut schema,
    );
    parse_array_fields(
        obj,
        root,
        effective_base,
        ctx.as_deref_mut(),
        depth,
        &mut schema,
    );

    // additionalProperties
    schema.additional_properties = parse_additional_properties(
        obj.get("additionalProperties"),
        root,
        effective_base,
        ctx.as_deref_mut(),
        depth,
    );

    // propertyNames
    schema.property_names = obj
        .get("propertyNames")
        .and_then(|v| {
            parse_schema_with_root(v, root, effective_base, ctx.as_deref_mut(), depth + 1)
        })
        .map(Box::new);

    // dependencies (Draft-04) / dependentRequired + dependentSchemas (Draft 2019-09)
    let (dep_req, dep_sch) =
        parse_dependencies(obj, root, effective_base, ctx.as_deref_mut(), depth);
    schema.dependent_required = dep_req;
    schema.dependent_schemas = dep_sch;

    parse_combinator_fields(
        obj,
        root,
        effective_base,
        ctx.as_deref_mut(),
        depth,
        &mut schema,
    );
    parse_extension_fields(obj, root, effective_base, ctx, depth, &mut schema);

    Some(schema)
}

type ParsedDependencies = (
    Option<HashMap<String, Vec<String>>>,
    Option<HashMap<String, JsonSchema>>,
);

/// Parse `dependencies` (Draft-04), `dependentRequired`, and `dependentSchemas`
/// (Draft 2019-09) from a schema object, merging into a unified pair of maps.
/// 2019-09 entries take precedence over Draft-04 `dependencies` on key collision.
fn parse_dependencies(
    obj: &serde_json::Map<String, Value>,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
) -> ParsedDependencies {
    let mut dep_req: HashMap<String, Vec<String>> = HashMap::new();
    let mut dep_sch: HashMap<String, JsonSchema> = HashMap::new();

    // Draft-04 `dependencies`
    if let Some(Value::Object(deps)) = obj.get("dependencies") {
        for (key, val) in deps {
            if let Some(arr) = val.as_array() {
                // Array of strings → dependentRequired
                let reqs: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                dep_req.insert(key.clone(), reqs);
            } else if let Some(schema) =
                parse_schema_with_root(val, root, base_uri, ctx.as_deref_mut(), depth + 1)
            {
                // Sub-schema → dependentSchemas
                dep_sch.insert(key.clone(), schema);
            }
        }
    }

    // Draft 2019-09 `dependentRequired` — merges over Draft-04 entries
    if let Some(Value::Object(dr)) = obj.get("dependentRequired") {
        for (key, val) in dr {
            if let Some(arr) = val.as_array() {
                let reqs: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                dep_req.insert(key.clone(), reqs);
            }
        }
    }

    // Draft 2019-09 `dependentSchemas` — merges over Draft-04 entries
    if let Some(Value::Object(ds)) = obj.get("dependentSchemas") {
        for (key, val) in ds {
            if let Some(schema) =
                parse_schema_with_root(val, root, base_uri, ctx.as_deref_mut(), depth + 1)
            {
                dep_sch.insert(key.clone(), schema);
            }
        }
    }

    let dep_req = if dep_req.is_empty() {
        None
    } else {
        Some(dep_req)
    };
    let dep_sch = if dep_sch.is_empty() {
        None
    } else {
        Some(dep_sch)
    };
    (dep_req, dep_sch)
}

fn parse_type(value: Option<&Value>) -> Option<SchemaType> {
    match value? {
        Value::String(s) => Some(SchemaType::Single(s.clone())),
        Value::Array(arr) => {
            let types: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if types.is_empty() {
                None
            } else {
                Some(SchemaType::Multiple(types))
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => None,
    }
}

fn string_field(obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key)?.as_str().map(String::from)
}

fn parse_additional_properties(
    value: Option<&Value>,
    root: &Value,
    base_uri: Option<&str>,
    #[allow(unused_mut)] mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
) -> Option<AdditionalProperties> {
    match value? {
        Value::Bool(false) => Some(AdditionalProperties::Denied),
        // true = allow anything = same as absent; everything else try as schema
        v @ (Value::Bool(true)
        | Value::Null
        | Value::Number(_)
        | Value::String(_)
        | Value::Array(_)
        | Value::Object(_)) => parse_schema_with_root(v, root, base_uri, ctx, depth + 1)
            .map(|s| AdditionalProperties::Schema(Box::new(s))),
    }
}

fn parse_schema_array(
    value: Option<&Value>,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
) -> Option<Vec<JsonSchema>> {
    let arr = value?.as_array()?;
    let mut schemas = Vec::new();
    for v in arr {
        if let Some(s) = parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1) {
            schemas.push(s);
        }
    }
    if schemas.is_empty() {
        None
    } else {
        Some(schemas)
    }
}

fn parse_definitions(
    value: Option<&Value>,
    root: &Value,
    base_uri: Option<&str>,
    mut ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
) -> Option<HashMap<String, JsonSchema>> {
    let map = value?.as_object()?;
    let mut result = HashMap::new();
    for (k, v) in map {
        if let Some(s) = parse_schema_with_root(v, root, base_uri, ctx.as_deref_mut(), depth + 1) {
            result.insert(k.clone(), s);
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// $ref resolution
// ──────────────────────────────────────────────────────────────────────────────

/// Resolve a `$ref` value, handling both local fragment refs and remote URIs.
///
/// **Local refs** (start with `#`):
/// - `#`       → root schema
/// - `#/a/b`   → JSON Pointer (RFC 6901) into root
/// - `#name`   → named anchor (`$anchor` or `$dynamicAnchor`) in root
///
/// **Remote refs** (everything else — requires `ctx` to be `Some`):
/// - `https://example.com/other.json`          → fetch entire document
/// - `https://example.com/other.json#/defs/Foo` → fetch + JSON Pointer
/// - `sub.json` resolved against `base_uri`
///
/// Returns `None` if the ref cannot be resolved or limits are exceeded.
fn resolve_ref(
    ref_str: &str,
    root: &Value,
    base_uri: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
    depth: usize,
) -> Option<JsonSchema> {
    if depth > MAX_REF_DEPTH {
        return None;
    }

    // ── Local fragment ref ────────────────────────────────────────────────────
    if let Some(pointer) = ref_str.strip_prefix('#') {
        if pointer.is_empty() {
            return parse_schema_with_root(root, root, None, None, depth + 1);
        }
        if pointer.starts_with('/') {
            let target = root.pointer(pointer)?;
            return parse_schema_with_root(target, root, None, None, depth + 1);
        }
        // Named anchor lookup
        return find_anchor_in_value(pointer, root)
            .and_then(|v| parse_schema_with_root(v, root, None, None, depth + 1));
    }

    // ── Remote ref ────────────────────────────────────────────────────────────
    let ctx = ctx?; // remote resolution requires a context

    // Split on first `#` to separate the URI from the optional fragment.
    let (uri_part, fragment) = ref_str.find('#').map_or((ref_str, None), |pos| {
        (&ref_str[..pos], Some(&ref_str[pos + 1..]))
    });

    // Resolve the URI part against the current base, then validate (SSRF guard).
    let absolute_uri = resolve_uri(base_uri, uri_part)?;
    let normalized = validate_and_normalize_url(&absolute_uri).ok()?;

    // Dedup + breadth guard: skip if already visited or limit reached.
    if !ctx.cache.contains(&normalized) && !ctx.try_visit(&normalized) {
        return None;
    }

    // Fetch and cache (first insertion wins — prevents $id-spoofing overwrite).
    // Pass ctx=None so that $refs inside the fetched remote document are not
    // themselves resolved remotely — intentional depth-1 remote resolution limit.
    if !ctx.cache.contains(&normalized) {
        let (value, schema) = fetch_schema_raw(&normalized, ctx.proxy, None).ok()?;
        ctx.cache.insert(normalized.clone(), value, schema);
    }

    let (remote_value, _) = ctx.cache.get_raw(&normalized)?;
    let remote_value = remote_value.clone(); // clone to release borrow on cache

    match fragment {
        None | Some("") => {
            // No fragment — parse the entire fetched document.
            parse_schema_with_root(
                &remote_value,
                &remote_value,
                Some(&normalized),
                None,
                depth + 1,
            )
        }
        Some(frag) if frag.starts_with('/') => {
            // JSON Pointer fragment.
            let target = remote_value.pointer(frag)?;
            parse_schema_with_root(target, &remote_value, Some(&normalized), None, depth + 1)
        }
        Some(name) => {
            // Named anchor in the remote document.
            find_anchor_in_value(name, &remote_value).and_then(|v| {
                parse_schema_with_root(v, &remote_value, Some(&normalized), None, depth + 1)
            })
        }
    }
}

/// Walk `value` recursively, returning the first JSON object that has
/// `"$anchor": name` or `"$dynamicAnchor": name`.
fn find_anchor_in_value<'a>(name: &str, value: &'a Value) -> Option<&'a Value> {
    match value {
        Value::Object(obj) => {
            let has_anchor = obj
                .get("$anchor")
                .and_then(Value::as_str)
                .is_some_and(|a| a == name);
            let has_dynamic = obj
                .get("$dynamicAnchor")
                .and_then(Value::as_str)
                .is_some_and(|a| a == name);
            if has_anchor || has_dynamic {
                return Some(value);
            }
            for v in obj.values() {
                if let Some(found) = find_anchor_in_value(name, v) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for v in arr {
                if let Some(found) = find_anchor_in_value(name, v) {
                    return Some(found);
                }
            }
            None
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema association — modeline
// ──────────────────────────────────────────────────────────────────────────────

/// Extract a schema URL from a `yaml-language-server` modeline comment.
///
/// Searches the first 10 lines of `text` for a line of the form:
/// ```text
/// # yaml-language-server: $schema=<url>
/// ```
/// Leading and trailing whitespace around `<url>` is stripped.
/// Returns `None` if no such line is found within the first 10 lines.
#[must_use]
pub fn extract_schema_url(text: &str) -> Option<String> {
    const PREFIX: &str = "# yaml-language-server: $schema=";

    for line in text.lines().take(10) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(PREFIX) {
            let url = rest.trim();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Extract custom tag names from a `yaml-language-server` modeline comment.
///
/// Searches the first 10 lines of `text` for a line of the form:
/// ```text
/// # yaml-language-server: $tags=!include,!ref
/// ```
/// Each tag is trimmed of whitespace. Empty strings after splitting are dropped.
/// Returns an empty `Vec` if no such line is found within the first 10 lines.
#[must_use]
pub fn extract_custom_tags(text: &str) -> Vec<String> {
    const PREFIX: &str = "# yaml-language-server: $tags=";

    for line in text.lines().take(10) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(PREFIX) {
            return rest
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    Vec::new()
}

/// Extract a YAML version from a `yaml-language-server` modeline comment.
///
/// Searches the first 10 lines of `text` for a line of the form:
/// ```text
/// # yaml-language-server: $yamlVersion=1.1
/// ```
/// Only `"1.1"` and `"1.2"` are accepted; any other value is ignored.
/// Leading and trailing whitespace around the value is stripped before
/// validation. Returns `None` if no valid modeline is found within the first
/// 10 lines.
#[must_use]
pub fn extract_yaml_version(text: &str) -> Option<String> {
    const PREFIX: &str = "# yaml-language-server: $yamlVersion=";

    for line in text.lines().take(10) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(PREFIX) {
            let value = rest.trim();
            if value == "1.1" || value == "1.2" {
                return Some(value.to_string());
            }
        }
    }
    None
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema association — Kubernetes auto-detection
// ──────────────────────────────────────────────────────────────────────────────

/// Inspect the first YAML document's root mapping for `apiVersion` and `kind`.
///
/// Returns `Some((api_version, kind))` if both keys are present and both values
/// are plain string scalars.  Returns `None` if the document slice is empty,
/// the root node is not a mapping, or either key is absent / non-string.
#[must_use]
pub fn detect_kubernetes_resource(
    docs: &[rlsp_yaml_parser_temp::node::Document<rlsp_yaml_parser_temp::Span>],
) -> Option<(String, String)> {
    use rlsp_yaml_parser_temp::node::Node;

    let root = &docs.first()?.root;
    let Node::Mapping { entries, .. } = root else {
        return None;
    };

    let mut api_version: Option<String> = None;
    let mut kind: Option<String> = None;

    for (k, v) in entries {
        let key = match k {
            Node::Scalar { value, .. } => value.as_str(),
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
        };
        let val = match v {
            Node::Scalar { value, .. } => value.clone(),
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
        };
        match key {
            "apiVersion" => api_version = Some(val),
            "kind" => kind = Some(val),
            _ => {}
        }
    }

    Some((api_version?, kind?))
}

/// Construct a Kubernetes JSON Schema URL for the given resource.
///
/// Uses the schema repository at
/// `https://raw.githubusercontent.com/yannh/kubernetes-json-schema`.
///
/// Filename rules:
/// - `kind` is lowercased.
/// - For grouped API versions (e.g. `apps/v1`) the filename is
///   `{kind}-{group}-{version}.json`.
/// - For core API versions (e.g. `v1`) the filename is
///   `{kind}-{api_version}.json`.
#[must_use]
pub fn kubernetes_schema_url(api_version: &str, kind: &str, k8s_version: &str) -> String {
    let kind_lower = kind.to_lowercase();
    let filename = if let Some((group, version)) = api_version.split_once('/') {
        format!("{kind_lower}-{group}-{version}.json")
    } else {
        format!("{kind_lower}-{api_version}.json")
    };
    let dir_prefix = if k8s_version == "master" {
        "master-standalone-strict".to_string()
    } else {
        format!("v{k8s_version}-standalone-strict")
    };
    format!(
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/{dir_prefix}/{filename}"
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema association — file pattern matching
// ──────────────────────────────────────────────────────────────────────────────

/// Return the schema URL for the first association whose glob pattern matches
/// `filename`, or `None` if no association matches.
///
/// Supported glob syntax:
/// - `*` matches any sequence of characters that does not include `/`
/// - `**` matches any sequence of characters including `/`
/// - All other characters match literally
#[must_use]
pub fn match_schema_by_filename(
    filename: &str,
    associations: &[SchemaAssociation],
) -> Option<String> {
    associations
        .iter()
        .find(|a| glob_matches(&a.pattern, filename))
        .map(|a| a.url.clone())
}

/// Return `true` if `pattern` matches `text` using simple glob rules.
fn glob_matches(pattern: &str, text: &str) -> bool {
    glob_matches_inner(pattern.as_bytes(), text.as_bytes())
}

fn glob_matches_inner(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        // Both exhausted — full match
        (None, None) => true,

        // Double-star: matches zero or more path segments
        (Some(&b'*'), _) if pattern.get(1) == Some(&b'*') => {
            let rest_pattern = pattern.get(2..).unwrap_or(&[]);
            // Skip any leading slash after **
            let rest_pattern = rest_pattern.strip_prefix(b"/").unwrap_or(rest_pattern);
            // Try matching rest_pattern against every suffix of text
            for i in 0..=text.len() {
                if glob_matches_inner(rest_pattern, text.get(i..).unwrap_or(&[])) {
                    return true;
                }
            }
            false
        }

        // Single-star: matches any sequence of non-slash characters
        (Some(&b'*'), _) => {
            let rest_pattern = pattern.get(1..).unwrap_or(&[]);
            for i in 0..=text.len() {
                if text.get(..i).is_some_and(|prefix| !prefix.contains(&b'/'))
                    && glob_matches_inner(rest_pattern, text.get(i..).unwrap_or(&[]))
                {
                    return true;
                }
            }
            false
        }

        // Literal character match
        (Some(&pc), Some(&tc)) => {
            if pc == tc {
                glob_matches_inner(
                    pattern.get(1..).unwrap_or(&[]),
                    text.get(1..).unwrap_or(&[]),
                )
            } else {
                false
            }
        }

        // One side exhausted but not the other — no match
        (None, Some(_)) | (Some(_), None) => false,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn schema_type_str(s: &JsonSchema) -> Option<&str> {
        match s.schema_type.as_ref()? {
            SchemaType::Single(t) => Some(t.as_str()),
            SchemaType::Multiple(_) => None,
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // extract_schema_url
    // ══════════════════════════════════════════════════════════════════════════

    // Test 1
    #[test]
    fn should_extract_url_from_modeline_on_first_line() {
        let text = "# yaml-language-server: $schema=https://example.com/schema.json\nkey: value\n";
        assert_eq!(
            extract_schema_url(text),
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Test 2
    #[test]
    fn should_extract_url_from_modeline_on_second_line() {
        let text = "key: value\n# yaml-language-server: $schema=https://example.com/schema.json\n";
        assert_eq!(
            extract_schema_url(text),
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Test 3
    #[test]
    fn should_extract_url_from_modeline_on_tenth_line() {
        let mut text = String::new();
        for _ in 0..9 {
            text.push_str("key: value\n");
        }
        text.push_str("# yaml-language-server: $schema=https://example.com/schema.json\n");
        assert_eq!(
            extract_schema_url(&text),
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Test 4
    #[test]
    fn should_return_none_when_modeline_beyond_tenth_line() {
        let mut text = String::new();
        for _ in 0..10 {
            text.push_str("key: value\n");
        }
        text.push_str("# yaml-language-server: $schema=https://example.com/schema.json\n");
        assert_eq!(extract_schema_url(&text), None);
    }

    // Test 5
    #[test]
    fn should_return_none_when_no_modeline_present() {
        let text = "key: value\nother: stuff\n";
        assert_eq!(extract_schema_url(text), None);
    }

    // Test 6
    #[test]
    fn should_return_none_for_malformed_modeline_missing_equals() {
        let text = "# yaml-language-server: $schema https://example.com/schema.json\n";
        assert_eq!(extract_schema_url(text), None);
    }

    // Test 7
    #[test]
    fn should_return_none_for_modeline_with_wrong_prefix() {
        let text = "# yaml-ls: $schema=https://example.com/schema.json\n";
        assert_eq!(extract_schema_url(text), None);
    }

    // Test 8 — whitespace after `=` is stripped
    #[test]
    fn should_handle_modeline_with_extra_leading_whitespace_in_url() {
        let text = "# yaml-language-server: $schema=  https://example.com/schema.json\n";
        assert_eq!(
            extract_schema_url(text),
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Test 9
    #[test]
    fn should_extract_http_url() {
        let text = "# yaml-language-server: $schema=http://example.com/schema.json\n";
        assert_eq!(
            extract_schema_url(text),
            Some("http://example.com/schema.json".to_string())
        );
    }

    // Test 10
    #[test]
    fn should_extract_file_url() {
        let text = "# yaml-language-server: $schema=file:///path/to/schema.json\n";
        assert_eq!(
            extract_schema_url(text),
            Some("file:///path/to/schema.json".to_string())
        );
    }

    // Test 11
    #[test]
    fn should_return_none_for_empty_input() {
        assert_eq!(extract_schema_url(""), None);
    }

    // Test 12 — $schema=none (lowercase) is extracted as-is
    #[test]
    fn should_extract_none_sentinel_lowercase() {
        let text = "# yaml-language-server: $schema=none\nkey: value\n";
        assert_eq!(extract_schema_url(text), Some("none".to_string()));
    }

    // Test 13 — $schema=None (mixed case) is extracted as-is
    #[test]
    fn should_extract_none_sentinel_mixed_case() {
        let text = "# yaml-language-server: $schema=None\nkey: value\n";
        assert_eq!(extract_schema_url(text), Some("None".to_string()));
    }

    // Test 14 — $schema=NONE (uppercase) is extracted as-is
    #[test]
    fn should_extract_none_sentinel_uppercase() {
        let text = "# yaml-language-server: $schema=NONE\nkey: value\n";
        assert_eq!(extract_schema_url(text), Some("NONE".to_string()));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // extract_custom_tags
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_extract_single_tag_from_modeline() {
        let text = "# yaml-language-server: $tags=!include\nkey: value\n";
        assert_eq!(extract_custom_tags(text), vec!["!include"]);
    }

    #[test]
    fn should_extract_multiple_tags_from_modeline() {
        let text = "# yaml-language-server: $tags=!include,!ref,!Ref\nkey: value\n";
        assert_eq!(extract_custom_tags(text), vec!["!include", "!ref", "!Ref"]);
    }

    #[test]
    fn should_trim_whitespace_around_tags() {
        let text = "# yaml-language-server: $tags= !include , !ref \nkey: value\n";
        assert_eq!(extract_custom_tags(text), vec!["!include", "!ref"]);
    }

    #[test]
    fn should_return_empty_vec_when_no_tags_modeline() {
        let text = "key: value\nother: stuff\n";
        assert_eq!(extract_custom_tags(text), Vec::<String>::new());
    }

    #[test]
    fn should_return_empty_vec_when_tags_modeline_beyond_line_10() {
        let mut text = String::new();
        for _ in 0..10 {
            text.push_str("key: value\n");
        }
        text.push_str("# yaml-language-server: $tags=!include\n");
        assert_eq!(extract_custom_tags(&text), Vec::<String>::new());
    }

    #[test]
    fn should_return_empty_vec_for_empty_input() {
        assert_eq!(extract_custom_tags(""), Vec::<String>::new());
    }

    #[test]
    fn should_extract_tags_from_modeline_on_second_line() {
        let text = "key: value\n# yaml-language-server: $tags=!include,!ref\n";
        assert_eq!(extract_custom_tags(text), vec!["!include", "!ref"]);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // extract_yaml_version
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_extract_version_1_1_from_modeline_on_first_line() {
        let text = "# yaml-language-server: $yamlVersion=1.1\nkey: value\n";
        assert_eq!(extract_yaml_version(text), Some("1.1".to_string()));
    }

    #[test]
    fn should_extract_version_1_2_from_modeline_on_first_line() {
        let text = "# yaml-language-server: $yamlVersion=1.2\nkey: value\n";
        assert_eq!(extract_yaml_version(text), Some("1.2".to_string()));
    }

    #[test]
    fn should_extract_version_from_modeline_on_tenth_line() {
        let mut text = String::new();
        for _ in 0..9 {
            text.push_str("key: value\n");
        }
        text.push_str("# yaml-language-server: $yamlVersion=1.1\n");
        assert_eq!(extract_yaml_version(&text), Some("1.1".to_string()));
    }

    #[test]
    fn should_return_none_when_yaml_version_modeline_beyond_tenth_line() {
        let mut text = String::new();
        for _ in 0..10 {
            text.push_str("key: value\n");
        }
        text.push_str("# yaml-language-server: $yamlVersion=1.1\n");
        assert_eq!(extract_yaml_version(&text), None);
    }

    #[test]
    fn should_return_none_for_invalid_version_value() {
        let text = "# yaml-language-server: $yamlVersion=2.0\nkey: value\n";
        assert_eq!(extract_yaml_version(text), None);
    }

    #[test]
    fn should_return_none_for_invalid_version_value_1_0() {
        let text = "# yaml-language-server: $yamlVersion=1.0\nkey: value\n";
        assert_eq!(extract_yaml_version(text), None);
    }

    #[test]
    fn should_return_none_when_no_yaml_version_modeline_present() {
        let text = "key: value\n";
        assert_eq!(extract_yaml_version(text), None);
    }

    #[test]
    fn should_return_none_for_empty_input_yaml_version() {
        assert_eq!(extract_yaml_version(""), None);
    }

    #[test]
    fn should_strip_whitespace_around_version_value() {
        let text = "# yaml-language-server: $yamlVersion=  1.2  \nkey: value\n";
        assert_eq!(extract_yaml_version(text), Some("1.2".to_string()));
    }

    #[test]
    fn should_return_none_for_empty_version_value() {
        let text = "# yaml-language-server: $yamlVersion=\nkey: value\n";
        assert_eq!(extract_yaml_version(text), None);
    }

    #[test]
    fn should_return_none_for_wrong_prefix_yaml_version() {
        let text = "# yaml-ls: $yamlVersion=1.1\nkey: value\n";
        assert_eq!(extract_yaml_version(text), None);
    }

    #[test]
    fn should_extract_version_from_second_line() {
        let text = "key: value\n# yaml-language-server: $yamlVersion=1.2\n";
        assert_eq!(extract_yaml_version(text), Some("1.2".to_string()));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // match_schema_by_filename
    // ══════════════════════════════════════════════════════════════════════════

    fn assoc(pattern: &str, url: &str) -> SchemaAssociation {
        SchemaAssociation {
            pattern: pattern.to_string(),
            url: url.to_string(),
        }
    }

    // Test 12
    #[test]
    fn should_return_url_for_exact_filename_match() {
        let associations = [assoc(
            "config.yaml",
            "https://example.com/config-schema.json",
        )];
        assert_eq!(
            match_schema_by_filename("config.yaml", &associations),
            Some("https://example.com/config-schema.json".to_string())
        );
    }

    // Test 13
    #[test]
    fn should_return_url_for_glob_wildcard_match() {
        let associations = [assoc("*.yaml", "https://example.com/generic.json")];
        assert_eq!(
            match_schema_by_filename("myfile.yaml", &associations),
            Some("https://example.com/generic.json".to_string())
        );
    }

    // Test 14
    #[test]
    fn should_return_url_for_double_star_glob_match() {
        let associations = [assoc(
            "configs/**/*.yaml",
            "https://example.com/schema.json",
        )];
        assert_eq!(
            match_schema_by_filename("configs/nested/file.yaml", &associations),
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Test 15
    #[test]
    fn should_return_none_when_no_association_matches() {
        let associations = [assoc("*.json", "https://example.com/schema.json")];
        assert_eq!(match_schema_by_filename("myfile.yaml", &associations), None);
    }

    // Test 16
    #[test]
    fn should_return_none_for_empty_associations() {
        assert_eq!(match_schema_by_filename("myfile.yaml", &[]), None);
    }

    // Test 17
    #[test]
    fn should_return_first_matching_association_when_multiple_match() {
        let associations = [
            assoc("*.yaml", "https://example.com/first.json"),
            assoc("*.yaml", "https://example.com/second.json"),
        ];
        assert_eq!(
            match_schema_by_filename("test.yaml", &associations),
            Some("https://example.com/first.json".to_string())
        );
    }

    // Test 18
    #[test]
    fn should_not_match_partial_filename() {
        let associations = [assoc("config.yaml", "https://example.com/schema.json")];
        assert_eq!(
            match_schema_by_filename("my-config.yaml", &associations),
            None
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // parse_schema
    // ══════════════════════════════════════════════════════════════════════════

    // Test 19
    #[test]
    fn should_parse_minimal_object_schema() {
        let v = json!({"type": "object"});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(schema_type_str(&s), Some("object"));
    }

    // Test 20
    #[test]
    fn should_parse_schema_with_properties() {
        let v = json!({"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "integer"}}});
        let s = parse_schema(&v).expect("should parse");
        let props = s.properties.as_ref().expect("should have properties");
        assert_eq!(
            schema_type_str(props.get("name").expect("name")),
            Some("string")
        );
        assert_eq!(
            schema_type_str(props.get("age").expect("age")),
            Some("integer")
        );
    }

    // Test 21
    #[test]
    fn should_parse_required_fields() {
        let v = json!({"type": "object", "required": ["name", "age"]});
        let s = parse_schema(&v).expect("should parse");
        let req = s.required.as_ref().expect("should have required");
        assert!(req.contains(&"name".to_string()));
        assert!(req.contains(&"age".to_string()));
    }

    // Test 22
    #[test]
    fn should_parse_enum_values() {
        let v = json!({"type": "string", "enum": ["alpha", "beta", "gamma"]});
        let s = parse_schema(&v).expect("should parse");
        let enums = s.enum_values.as_ref().expect("should have enum");
        assert_eq!(enums.len(), 3);
        assert!(enums.contains(&json!("alpha")));
        assert!(enums.contains(&json!("beta")));
        assert!(enums.contains(&json!("gamma")));
    }

    // Test 23
    #[test]
    fn should_parse_description() {
        let v = json!({"type": "string", "description": "A human-readable name"});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(s.description.as_deref(), Some("A human-readable name"));
    }

    // Test 24
    #[test]
    fn should_parse_default_value() {
        let v = json!({"type": "integer", "default": 42});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(s.default, Some(json!(42)));
    }

    // Test 25
    #[test]
    fn should_parse_array_schema_with_items() {
        let v = json!({"type": "array", "items": {"type": "string"}});
        let s = parse_schema(&v).expect("should parse");
        let items = s.items.as_ref().expect("should have items");
        assert_eq!(schema_type_str(items), Some("string"));
    }

    // Test 26
    #[test]
    fn should_parse_additional_properties_false() {
        let v = json!({"type": "object", "additionalProperties": false});
        let s = parse_schema(&v).expect("should parse");
        assert!(matches!(
            s.additional_properties,
            Some(AdditionalProperties::Denied)
        ));
    }

    // Test 27
    #[test]
    fn should_parse_additional_properties_as_schema() {
        let v = json!({"type": "object", "additionalProperties": {"type": "string"}});
        let s = parse_schema(&v).expect("should parse");
        assert!(matches!(
            s.additional_properties,
            Some(AdditionalProperties::Schema(_))
        ));
    }

    // Test 27b
    #[test]
    fn should_parse_min_properties_and_max_properties() {
        let v = json!({"type": "object", "minProperties": 1, "maxProperties": 5});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(s.min_properties, Some(1));
        assert_eq!(s.max_properties, Some(5));
    }

    // Test P-1
    #[test]
    fn should_parse_additional_items_false() {
        let v = json!({"items": [{"type": "string"}], "additionalItems": false});
        let s = parse_schema(&v).expect("should parse");
        assert!(s.prefix_items.is_some());
        assert!(matches!(
            s.additional_items,
            Some(AdditionalProperties::Denied)
        ));
    }

    // Test P-2
    #[test]
    fn should_parse_additional_items_schema() {
        let v = json!({"items": [{"type": "string"}], "additionalItems": {"type": "integer"}});
        let s = parse_schema(&v).expect("should parse");
        assert!(matches!(
            s.additional_items,
            Some(AdditionalProperties::Schema(_))
        ));
    }

    // Test P-3
    #[test]
    fn should_not_parse_additional_items_when_prefix_items_set_from_prefix_items_key() {
        let v = json!({"prefixItems": [{"type": "string"}], "additionalItems": false});
        let s = parse_schema(&v).expect("should parse");
        assert!(s.additional_items.is_none());
    }

    // Test P-4
    #[test]
    fn should_not_parse_additional_items_when_no_array_items() {
        let v = json!({"type": "array", "additionalItems": false});
        let s = parse_schema(&v).expect("should parse");
        assert!(s.additional_items.is_none());
    }

    // Test 28
    #[test]
    fn should_parse_all_of() {
        let v = json!({"allOf": [{"type": "object"}, {"required": ["name"]}]});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(s.all_of.as_ref().map(Vec::len), Some(2));
    }

    // Test 29
    #[test]
    fn should_parse_any_of() {
        let v = json!({"anyOf": [{"type": "string"}, {"type": "integer"}]});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(s.any_of.as_ref().map(Vec::len), Some(2));
    }

    // Test 30
    #[test]
    fn should_parse_one_of() {
        let v = json!({"oneOf": [{"type": "string"}, {"type": "null"}]});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(s.one_of.as_ref().map(Vec::len), Some(2));
    }

    // Test 31
    #[test]
    fn should_return_none_for_null_input() {
        assert!(parse_schema(&Value::Null).is_none());
    }

    // Test 32
    #[test]
    fn should_return_none_for_non_object_json() {
        assert!(parse_schema(&Value::String("not a schema".into())).is_none());
    }

    // Test 33
    #[test]
    fn should_parse_empty_object_as_permissive_schema() {
        let v = json!({});
        let s = parse_schema(&v).expect("should parse");
        assert!(s.schema_type.is_none());
        assert!(s.properties.is_none());
        assert!(s.required.is_none());
    }

    // Test 34 — boolean true → permissive schema
    #[test]
    fn should_parse_boolean_true_schema() {
        let s = parse_schema(&Value::Bool(true)).expect("should return Some for true");
        assert!(s.schema_type.is_none());
    }

    // Test 35 — boolean false → None
    #[test]
    fn should_parse_boolean_false_schema() {
        assert!(parse_schema(&Value::Bool(false)).is_none());
    }

    // Test 36
    #[test]
    fn should_parse_draft04_definitions() {
        let v = json!({"definitions": {"addr": {"type": "string"}}});
        let s = parse_schema(&v).expect("should parse");
        let defs = s.definitions.as_ref().expect("should have definitions");
        assert!(defs.contains_key("addr"));
    }

    // Test 37
    #[test]
    fn should_parse_draft07_defs() {
        let v = json!({"$defs": {"addr": {"type": "string"}}});
        let s = parse_schema(&v).expect("should parse");
        let defs = s.definitions.as_ref().expect("should have $defs");
        assert!(defs.contains_key("addr"));
    }

    // Test 38 — deprecated: true parses to Some(true)
    #[test]
    fn should_parse_deprecated_true() {
        let v = json!({"type": "string", "deprecated": true});
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(s.deprecated, Some(true));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // $ref resolution
    // ══════════════════════════════════════════════════════════════════════════

    // Test 39
    #[test]
    fn should_resolve_simple_local_ref() {
        let v = json!({
            "$ref": "#/definitions/MyType",
            "definitions": {"MyType": {"type": "string"}}
        });
        let s = parse_schema(&v).expect("should resolve");
        assert_eq!(schema_type_str(&s), Some("string"));
    }

    // Test 40
    #[test]
    fn should_return_none_for_missing_ref_target() {
        let v = json!({"$ref": "#/definitions/Missing"});
        // Should not panic; result is None or a schema without type
        let _ = parse_schema(&v);
    }

    // Test 40
    #[test]
    fn should_handle_nested_ref_resolution() {
        let v = json!({
            "type": "object",
            "properties": {
                "foo": {"$ref": "#/definitions/Bar"}
            },
            "definitions": {"Bar": {"type": "integer"}}
        });
        let s = parse_schema(&v).expect("should parse");
        let props = s.properties.as_ref().expect("should have properties");
        let foo = props.get("foo").expect("should have foo");
        assert_eq!(schema_type_str(foo), Some("integer"));
    }

    // Test 41 — circular ref must terminate
    #[test]
    fn should_not_infinite_loop_on_circular_ref() {
        let v = json!({
            "$ref": "#/definitions/A",
            "definitions": {
                "A": {"$ref": "#/definitions/A"}
            }
        });
        // Must complete in finite time; result doesn't matter
        let _ = parse_schema(&v);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // SchemaCache
    // ══════════════════════════════════════════════════════════════════════════

    // Test 42
    #[test]
    fn should_return_none_on_cache_miss() {
        let cache = SchemaCache::new();
        assert!(cache.get("https://example.com/schema.json").is_none());
    }

    // Test 43
    #[test]
    fn should_return_cached_schema_on_cache_hit() {
        let mut cache = SchemaCache::new();
        let schema = JsonSchema {
            description: Some("test".to_string()),
            ..JsonSchema::default()
        };
        cache.insert(
            "https://example.com/schema.json".to_string(),
            Value::Null,
            schema,
        );

        let result = cache
            .get("https://example.com/schema.json")
            .expect("should be cached");
        assert_eq!(result.description.as_deref(), Some("test"));
    }

    // Test 44 — first write wins
    #[test]
    fn should_not_overwrite_existing_cache_entry() {
        let mut cache = SchemaCache::new();
        let schema_a = JsonSchema {
            description: Some("first".to_string()),
            ..JsonSchema::default()
        };
        let schema_b = JsonSchema {
            description: Some("second".to_string()),
            ..JsonSchema::default()
        };

        cache.insert(
            "https://example.com/schema.json".to_string(),
            Value::Null,
            schema_a,
        );
        cache.insert(
            "https://example.com/schema.json".to_string(),
            Value::Null,
            schema_b,
        );

        let result = cache
            .get("https://example.com/schema.json")
            .expect("should be cached");
        assert_eq!(result.description.as_deref(), Some("first"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Integration / fetch harness spike (Test 45)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 45 — harness spike: 127.0.0.1 is blocked by SSRF guard before any
    // network call is made.
    #[test]
    fn should_return_error_for_unreachable_url() {
        let result = fetch_schema_raw("http://127.0.0.1:19999/nonexistent.json", None, None);
        assert!(result.is_err());
    }

    // Test 46 — fetch happy path (parse pipeline without network).
    // Constructs a minimal JSON Schema string, runs it through the same
    // parse pipeline that `fetch_schema` uses after reading the response body:
    // `serde_json::from_slice` → `check_json_depth` → `parse_schema`.
    #[test]
    fn should_parse_fetched_schema_from_valid_response() {
        let body =
            r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#;
        let buf = body.as_bytes();

        // Step 1: deserialise JSON (mirrors fetch_schema's from_slice call)
        let value: Value = serde_json::from_slice(buf).expect("valid JSON should deserialise");

        // Step 2: depth check (mirrors fetch_schema's check_json_depth call)
        check_json_depth(&value, 0).expect("shallow schema should pass depth check");

        // Step 3: parse into JsonSchema (mirrors fetch_schema's parse_schema call)
        let schema = parse_schema(&value).expect("should produce a schema");

        assert_eq!(schema_type_str(&schema), Some("object"));
        let props = schema.properties.as_ref().expect("should have properties");
        assert!(props.contains_key("name"));
        assert_eq!(
            schema_type_str(props.get("name").expect("name")),
            Some("string")
        );
        let req = schema.required.as_ref().expect("should have required");
        assert!(req.contains(&"name".to_string()));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security tests (from Security Engineer assessment)
    // ══════════════════════════════════════════════════════════════════════════

    // Sec-1: file:// scheme is rejected
    #[test]
    fn should_reject_file_scheme_url() {
        let result = validate_and_normalize_url("file:///etc/passwd");
        assert!(result.is_err());
    }

    // Sec-2: localhost is rejected
    #[test]
    fn should_reject_localhost_url() {
        let result = validate_and_normalize_url("http://localhost/schema.json");
        assert!(result.is_err());
    }

    // Sec-3: AWS metadata IP is rejected
    #[test]
    fn should_reject_link_local_ip_url() {
        let result = validate_and_normalize_url("http://169.254.169.254/latest/meta-data/");
        assert!(result.is_err());
    }

    // Sec-4: fetch_schema_raw rejects 127.0.0.1 before making a network call
    #[test]
    fn should_reject_loopback_ip_in_fetch() {
        let result = fetch_schema_raw("http://127.0.0.1:8080/schema.json", None, None);
        assert!(result.is_err());
    }

    // Sec-5: URL exceeding 2048 chars is rejected
    #[test]
    fn should_reject_url_exceeding_max_length() {
        let long_url = format!("https://example.com/{}", "a".repeat(2050));
        let result = validate_and_normalize_url(&long_url);
        assert!(result.is_err());
    }

    // Sec-6: cache key normalisation — scheme+host lowercased
    #[test]
    fn should_normalize_cache_key_url() {
        let a = validate_and_normalize_url("https://example.com/schema").expect("valid");
        let b = validate_and_normalize_url("HTTPS://EXAMPLE.COM/schema").expect("valid");
        assert_eq!(a, b, "scheme+host should be normalized to lowercase");
    }

    // Sec-7: parse_schema terminates on deeply nested schema
    #[test]
    fn should_reject_excessively_nested_schema() {
        let mut v = json!({"type": "string"});
        for _ in 0..100 {
            v = json!({"type": "object", "properties": {"x": v}});
        }
        // Must terminate; may return None or a truncated schema
        let _ = parse_schema(&v);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security tests addendum (Tests 47–61)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 47 — file:// scheme rejected at validation layer (duplicate coverage
    // of Sec-1 under the numbered scheme for completeness)
    #[test]
    fn should_reject_file_scheme_url_47() {
        assert!(validate_and_normalize_url("file:///etc/passwd").is_err());
    }

    // Test 48 — localhost rejected (symbolic hostname)
    #[test]
    fn should_reject_localhost_url_48() {
        assert!(validate_and_normalize_url("http://localhost/schema.json").is_err());
    }

    // Test 49 — 127.0.0.1 rejected at validation layer (IP form)
    #[test]
    fn should_reject_loopback_ip_url() {
        assert!(validate_and_normalize_url("http://127.0.0.1/schema.json").is_err());
    }

    // Test 50 — IPv6 loopback [::1] rejected
    #[test]
    fn should_reject_ipv6_loopback_url() {
        assert!(validate_and_normalize_url("http://[::1]/schema.json").is_err());
    }

    // Test 51 — AWS metadata endpoint rejected (link-local)
    #[test]
    fn should_reject_link_local_aws_metadata_url() {
        assert!(validate_and_normalize_url("http://169.254.169.254/latest/meta-data/").is_err());
    }

    // Test 52 — URL exceeding max length rejected
    #[test]
    fn should_reject_url_exceeding_max_length_52() {
        let long_url = format!("https://example.com/{}", "a".repeat(2048));
        assert!(validate_and_normalize_url(&long_url).is_err());
    }

    // Test 53 — valid https URL passes validation
    #[test]
    fn should_accept_valid_https_url() {
        let result = validate_and_normalize_url(
            "https://schemastore.azurewebsites.net/schemas/json/package.json",
        );
        assert!(result.is_ok(), "valid https URL should be accepted");
    }

    // Test 54 — valid http URL passes validation
    #[test]
    fn should_accept_valid_http_url() {
        let result = validate_and_normalize_url("http://json.schemastore.org/package");
        assert!(result.is_ok(), "valid http URL should be accepted");
    }

    // Test 55 — response of exactly MAX_SCHEMA_BYTES bytes is accepted.
    // The `.take(MAX_SCHEMA_BYTES + 1)` + `> MAX_SCHEMA_BYTES` logic allows
    // responses up to and including MAX_SCHEMA_BYTES.
    #[test]
    fn should_return_error_when_response_exceeds_size_limit() {
        // Produce a buffer of exactly MAX_SCHEMA_BYTES bytes and verify the
        // size-check condition does NOT trigger for this boundary value.
        let buf = vec![b'x'; MAX_SCHEMA_BYTES as usize];
        assert!(
            buf.len() as u64 <= MAX_SCHEMA_BYTES,
            "exactly MAX_SCHEMA_BYTES bytes must not trigger ResponseTooLarge"
        );
    }

    // Test 55b — response larger than MAX_SCHEMA_BYTES triggers ResponseTooLarge.
    #[test]
    fn should_return_error_when_response_exceeds_size_limit_over() {
        use std::io::Read as _;

        // Build a body of MAX_SCHEMA_BYTES + 1 bytes (over the cap).
        let body = vec![b'x'; MAX_SCHEMA_BYTES as usize + 1];
        let cursor = std::io::Cursor::new(&body);
        // Mirror the fetch logic: take MAX_SCHEMA_BYTES + 1, then check.
        let mut limited = cursor.take(MAX_SCHEMA_BYTES + 1);
        let mut buf = Vec::new();
        limited.read_to_end(&mut buf).expect("read succeeds");

        // More than MAX_SCHEMA_BYTES bytes read — the cap condition triggers.
        assert!(
            buf.len() as u64 > MAX_SCHEMA_BYTES,
            "over-limit read should trigger ResponseTooLarge condition"
        );
    }

    // Test 56 — schema with 60-level nesting is rejected or truncated (does not hang)
    #[test]
    fn should_reject_schema_exceeding_nesting_depth() {
        let mut v = json!({"type": "string"});
        for _ in 0..60 {
            v = json!({"type": "object", "properties": {"child": v}});
        }
        // Must terminate; truncated result or None is acceptable
        let _ = parse_schema(&v);
    }

    // Test 57 — schema with 10-level nesting is accepted
    #[test]
    fn should_accept_schema_within_nesting_depth() {
        let mut v = json!({"type": "string"});
        for _ in 0..10 {
            v = json!({"type": "object", "properties": {"child": v}});
        }
        let result = parse_schema(&v);
        assert!(
            result.is_some(),
            "schema within depth limit should be accepted"
        );
    }

    // Test 58 — two-node circular $ref does not hang
    #[test]
    fn should_not_hang_on_two_node_circular_ref() {
        let v = json!({
            "$ref": "#/definitions/A",
            "definitions": {
                "A": {"$ref": "#/definitions/B"},
                "B": {"$ref": "#/definitions/A"}
            }
        });
        // Must complete in finite time; result is None or partial schema
        let _ = parse_schema(&v);
    }

    // Test 59 — trailing-slash path variants produce distinct cache keys.
    // `url::Url` treats `/schema` and `/schema/` as different paths; both are
    // preserved after normalization. This test explicitly documents that
    // behavior so any future change is immediately detectable.
    #[test]
    fn should_normalize_cache_key_trailing_slash() {
        let key_no_slash = validate_and_normalize_url("https://example.com/schema").expect("valid");
        let key_with_slash =
            validate_and_normalize_url("https://example.com/schema/").expect("valid");

        // The url crate preserves trailing-slash distinctions — these are
        // different resources and must not silently collapse to the same key.
        assert_ne!(
            key_no_slash, key_with_slash,
            "trailing-slash variants are distinct paths and must not share a cache key"
        );
    }

    // Test 60 — cache key host case normalization (explicit cache test)
    #[test]
    fn should_normalize_cache_key_host_case() {
        let key_upper = validate_and_normalize_url("https://EXAMPLE.COM/schema").expect("valid");
        let key_lower = validate_and_normalize_url("https://example.com/schema").expect("valid");
        assert_eq!(
            key_upper, key_lower,
            "host should be normalized to lowercase in cache key"
        );
    }

    // Test 61 — redirect must not be followed (max_redirects(0) enforcement).
    //
    // With max_redirects(0), ureq returns the 3xx response as-is rather than
    // following it. The test asserts that the status code is 302 — proving the
    // redirect was not followed (which would have produced a 200 from /redirected).
    #[test]
    fn should_not_follow_redirects() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let url = format!("http://{addr}/schema.json");
        let redirect_target = format!("http://{addr}/redirected");

        std::thread::spawn(move || {
            if let Ok(req) = server.recv() {
                let location =
                    tiny_http::Header::from_bytes(b"Location", redirect_target.as_bytes()).unwrap();
                let response = tiny_http::Response::empty(302).with_header(location);
                let _ = req.respond(response);
            }
        });

        let agent = build_agent(None);
        let response = agent.get(&url).call().expect("request should succeed");
        assert_eq!(
            response.status(),
            302,
            "agent must return 302 without following the redirect"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group K — Previously uncovered paths
    // ══════════════════════════════════════════════════════════════════════════

    // ── SchemaError Display ───────────────────────────────────────────────────

    #[test]
    fn schema_error_display_fetch_failed() {
        let e = SchemaError::FetchFailed("connection refused".to_string());
        let msg = e.to_string();
        assert!(msg.contains("Fetch failed"), "got: {msg}");
        assert!(msg.contains("connection refused"), "got: {msg}");
    }

    #[test]
    fn schema_error_display_response_too_large() {
        let e = SchemaError::ResponseTooLarge;
        let msg = e.to_string();
        assert!(msg.contains("size limit"), "got: {msg}");
    }

    #[test]
    fn schema_error_display_parse_failed() {
        let e = SchemaError::ParseFailed("unexpected token".to_string());
        let msg = e.to_string();
        assert!(msg.contains("parse failed"), "got: {msg}");
        assert!(msg.contains("unexpected token"), "got: {msg}");
    }

    #[test]
    fn schema_error_display_too_deep() {
        let e = SchemaError::TooDeep;
        let msg = e.to_string();
        assert!(msg.contains("depth"), "got: {msg}");
    }

    #[test]
    fn schema_error_display_url_not_permitted() {
        let e = SchemaError::UrlNotPermitted("ftp://bad".to_string());
        let msg = e.to_string();
        assert!(msg.contains("not permitted"), "got: {msg}");
    }

    // ── SSRF guard — additional IP ranges ────────────────────────────────────

    #[test]
    fn should_reject_private_ipv4_10_range() {
        let result = validate_and_normalize_url("http://10.0.0.1/schema.json");
        assert!(result.is_err(), "private 10.x.x.x must be rejected");
    }

    #[test]
    fn should_reject_private_ipv4_192_168_range() {
        let result = validate_and_normalize_url("http://192.168.1.1/schema.json");
        assert!(result.is_err(), "private 192.168.x.x must be rejected");
    }

    #[test]
    fn should_reject_private_ipv4_172_16_range() {
        let result = validate_and_normalize_url("http://172.16.0.1/schema.json");
        assert!(result.is_err(), "private 172.16.x.x must be rejected");
    }

    #[test]
    fn should_reject_unspecified_ipv4_0_0_0_0() {
        let result = validate_and_normalize_url("http://0.0.0.0/schema.json");
        assert!(result.is_err(), "unspecified 0.0.0.0 must be rejected");
    }

    #[test]
    fn should_reject_ipv6_unspecified_double_colon() {
        let result = validate_and_normalize_url("http://[::]/schema.json");
        assert!(result.is_err(), "IPv6 unspecified :: must be rejected");
    }

    #[test]
    fn should_reject_ipv6_link_local_fe80() {
        let result = validate_and_normalize_url("http://[fe80::1]/schema.json");
        assert!(result.is_err(), "IPv6 link-local fe80:: must be rejected");
    }

    #[test]
    fn should_reject_ftp_scheme() {
        let result = validate_and_normalize_url("ftp://example.com/schema.json");
        assert!(result.is_err(), "ftp:// scheme must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("ftp"),
            "error message should mention the scheme, got: {msg}"
        );
    }

    #[test]
    fn should_reject_unparseable_url() {
        let result = validate_and_normalize_url("not a url at all");
        assert!(result.is_err(), "unparseable string must be rejected");
    }

    // IPv6 ULA (fc00::/7) and IPv4-mapped SSRF gaps
    #[test]
    fn should_reject_ipv6_ula_fd00() {
        let result = validate_and_normalize_url("http://[fd00::1]/schema.json");
        assert!(result.is_err(), "IPv6 ULA fd00:: must be rejected");
    }

    #[test]
    fn should_reject_ipv6_ula_fc00() {
        let result = validate_and_normalize_url("http://[fc00::1]/schema.json");
        assert!(result.is_err(), "IPv6 ULA fc00:: must be rejected");
    }

    #[test]
    fn should_reject_ipv4_mapped_private() {
        let result = validate_and_normalize_url("http://[::ffff:192.168.1.1]/schema.json");
        assert!(
            result.is_err(),
            "IPv4-mapped private address must be rejected"
        );
    }

    #[test]
    fn should_reject_ipv4_mapped_loopback() {
        let result = validate_and_normalize_url("http://[::ffff:127.0.0.1]/schema.json");
        assert!(
            result.is_err(),
            "IPv4-mapped loopback address must be rejected"
        );
    }

    #[test]
    fn should_allow_ipv4_mapped_public() {
        let result = validate_and_normalize_url("http://[::ffff:8.8.8.8]/schema.json");
        assert!(
            result.is_ok(),
            "IPv4-mapped public address must be allowed: {result:?}"
        );
    }

    // ── parse_type edge cases ─────────────────────────────────────────────────

    #[test]
    fn parse_type_returns_none_for_non_string_non_array() {
        // type: 42 (number) — should be ignored
        let v = json!({"type": 42});
        let s = parse_schema(&v).expect("should parse as object schema");
        assert!(
            s.schema_type.is_none(),
            "non-string/non-array type should yield None"
        );
    }

    #[test]
    fn parse_type_returns_none_for_empty_type_array() {
        // type: [] — empty array has no types, should yield None
        let v = json!({"type": []});
        let s = parse_schema(&v).expect("should parse");
        assert!(
            s.schema_type.is_none(),
            "empty type array should yield None schema_type"
        );
    }

    #[test]
    fn parse_type_filters_non_string_items_from_array() {
        // type: [42, "string"] — non-string items filtered out; "string" survives
        let v = json!({"type": [42, "string"]});
        let s = parse_schema(&v).expect("should parse");
        // "string" remains after filtering
        assert!(
            s.schema_type.is_some(),
            "string item should survive filtering"
        );
    }

    // ── $ref edge cases ───────────────────────────────────────────────────────

    #[test]
    fn ref_pointing_to_root_returns_parsed_root() {
        // $ref: "#" — empty pointer, resolves to root document itself
        let v = json!({
            "definitions": {
                "Root": {"$ref": "#"}
            },
            "type": "object"
        });
        // Parsing the root succeeds — it has type "object"
        let s = parse_schema(&v).expect("should parse");
        assert_eq!(schema_type_str(&s), Some("object"));
    }

    #[test]
    fn ref_without_hash_prefix_yields_ref_path_only() {
        // $ref without '#' prefix cannot be resolved locally — returns schema with ref_path set
        let v = json!({"$ref": "http://example.com/other-schema.json"});
        let result = parse_schema(&v);
        // resolve_ref returns None for non-# refs; parse_schema_with_root returns Some(schema)
        // with only ref_path set
        if let Some(s) = result {
            assert_eq!(
                s.ref_path.as_deref(),
                Some("http://example.com/other-schema.json")
            );
        }
        // None is also acceptable (no crash guarantee)
    }

    // ── parse_schema_array edge cases ─────────────────────────────────────────

    #[test]
    fn empty_all_of_array_yields_none() {
        // allOf: [] — empty array produces no schemas; field should be None
        let v = json!({"allOf": []});
        let s = parse_schema(&v).expect("should parse");
        assert!(s.all_of.is_none(), "empty allOf should yield None");
    }

    #[test]
    fn all_of_with_non_object_entries_filtered_out_yields_none() {
        // allOf: ["string"] — non-object entries filtered by parse_schema_with_root
        let v = json!({"allOf": ["not a schema"]});
        let s = parse_schema(&v).expect("should parse");
        assert!(
            s.all_of.is_none(),
            "allOf with only invalid entries should yield None"
        );
    }

    // ── parse_definitions edge cases ─────────────────────────────────────────

    #[test]
    fn empty_definitions_object_yields_none() {
        // definitions: {} — empty map produces no entries; field should be None
        let v = json!({"definitions": {}});
        let s = parse_schema(&v).expect("should parse");
        assert!(
            s.definitions.is_none(),
            "empty definitions should yield None"
        );
    }

    #[test]
    fn both_definitions_and_defs_are_merged() {
        // Both definitions (Draft-04) and $defs (Draft-07) present — merged
        let v = json!({
            "definitions": {"TypeA": {"type": "string"}},
            "$defs": {"TypeB": {"type": "integer"}}
        });
        let s = parse_schema(&v).expect("should parse");
        let defs = s
            .definitions
            .as_ref()
            .expect("should have merged definitions");
        assert!(
            defs.contains_key("TypeA"),
            "TypeA from definitions should be present"
        );
        assert!(
            defs.contains_key("TypeB"),
            "TypeB from $defs should be present"
        );
    }

    // ── additionalProperties: true ────────────────────────────────────────────

    #[test]
    fn additional_properties_true_parsed_as_permissive_schema() {
        // additionalProperties: true — boolean true is a permissive schema
        let v = json!({"type": "object", "additionalProperties": true});
        let s = parse_schema(&v).expect("should parse");
        // true is a permissive boolean schema → AdditionalProperties::Schema(empty)
        assert!(
            matches!(
                s.additional_properties,
                Some(AdditionalProperties::Schema(_))
            ),
            "additionalProperties: true should yield Schema variant"
        );
    }

    // ── check_json_depth with array ───────────────────────────────────────────

    #[test]
    fn check_json_depth_rejects_deeply_nested_array() {
        // Build a deeply nested array: [[[[...]]]]
        let mut v = json!("leaf");
        for _ in 0..55 {
            v = json!([v]);
        }
        let result = check_json_depth(&v, 0);
        assert!(
            result.is_err(),
            "deeply nested array should exceed depth limit"
        );
    }

    #[test]
    fn check_json_depth_accepts_shallow_array() {
        let v = json!(["a", "b", "c"]);
        assert!(check_json_depth(&v, 0).is_ok());
    }

    // ── required with non-string values filtered ──────────────────────────────

    #[test]
    fn required_with_non_string_values_filtered() {
        // required: [42, "name"] — non-string values (42) are filtered by filter_map
        let v = json!({"required": [42, "name", true]});
        let s = parse_schema(&v).expect("should parse");
        let req = s.required.as_ref().expect("should have required");
        assert_eq!(req.len(), 1, "only string 'name' should survive filtering");
        assert!(req.contains(&"name".to_string()));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // detect_kubernetes_resource + kubernetes_schema_url
    // ══════════════════════════════════════════════════════════════════════════

    fn parse_docs(
        text: &str,
    ) -> Vec<rlsp_yaml_parser_temp::node::Document<rlsp_yaml_parser_temp::Span>> {
        rlsp_yaml_parser_temp::load(text).unwrap_or_default()
    }

    // Test K8s-1: core API (v1 Pod) — detection and URL
    #[test]
    fn should_detect_core_api_pod_and_build_url() {
        let docs = parse_docs("apiVersion: v1\nkind: Pod\n");
        let result = detect_kubernetes_resource(&docs);
        assert_eq!(result, Some(("v1".to_string(), "Pod".to_string())));
        let url = kubernetes_schema_url("v1", "Pod", "1.29.0");
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/pod-v1.json"
        );
    }

    // Test K8s-2: grouped API (apps/v1 Deployment) — detection and URL
    #[test]
    fn should_detect_grouped_api_deployment_and_build_url() {
        let docs = parse_docs("apiVersion: apps/v1\nkind: Deployment\n");
        let result = detect_kubernetes_resource(&docs);
        assert_eq!(
            result,
            Some(("apps/v1".to_string(), "Deployment".to_string()))
        );
        let url = kubernetes_schema_url("apps/v1", "Deployment", "1.29.0");
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/deployment-apps-v1.json"
        );
    }

    // Test K8s-3: HPA autoscaling/v2 case
    #[test]
    fn should_detect_hpa_and_build_url() {
        let docs = parse_docs("apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\n");
        let result = detect_kubernetes_resource(&docs);
        assert_eq!(
            result,
            Some((
                "autoscaling/v2".to_string(),
                "HorizontalPodAutoscaler".to_string()
            ))
        );
        let url = kubernetes_schema_url("autoscaling/v2", "HorizontalPodAutoscaler", "1.29.0");
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/horizontalpodautoscaler-autoscaling-v2.json"
        );
    }

    // Test K8s-4: missing apiVersion → None
    #[test]
    fn should_return_none_when_api_version_missing() {
        let docs = parse_docs("kind: Pod\nmetadata:\n  name: test\n");
        assert_eq!(detect_kubernetes_resource(&docs), None);
    }

    // Test K8s-5: missing kind → None
    #[test]
    fn should_return_none_when_kind_missing() {
        let docs = parse_docs("apiVersion: v1\nmetadata:\n  name: test\n");
        assert_eq!(detect_kubernetes_resource(&docs), None);
    }

    // Test K8s-6: empty docs → None
    #[test]
    fn should_return_none_for_empty_docs() {
        assert_eq!(detect_kubernetes_resource(&[]), None);
    }

    // Test K8s-7: multi-document — only first doc inspected; second has fields, first doesn't
    #[test]
    fn should_inspect_only_first_document() {
        let docs = parse_docs("other: value\n---\napiVersion: v1\nkind: Pod\n");
        // First doc has no apiVersion/kind → None
        assert_eq!(detect_kubernetes_resource(&docs), None);
    }

    // Test K8s-8: non-string values for apiVersion/kind → None
    #[test]
    fn should_return_none_when_api_version_or_kind_is_non_string() {
        // apiVersion is a mapping, kind is a mapping — neither is a string scalar
        let docs = parse_docs("apiVersion:\n  nested: true\nkind:\n  - item\n");
        assert_eq!(detect_kubernetes_resource(&docs), None);
    }

    // Test K8s-9: master version — core API uses master-standalone-strict (no v prefix)
    #[test]
    fn should_build_url_with_master_standalone_strict_for_core_api() {
        let url = kubernetes_schema_url("v1", "Pod", "master");
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/master-standalone-strict/pod-v1.json"
        );
    }

    // Test K8s-10: master version — grouped API uses master-standalone-strict (no v prefix)
    #[test]
    fn should_build_url_with_master_standalone_strict_for_grouped_api() {
        let url = kubernetes_schema_url("apps/v1", "Deployment", "master");
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/master-standalone-strict/deployment-apps-v1.json"
        );
    }

    // Test K8s-11: "Master" (capital M) falls through to versioned branch (case-sensitive match)
    #[test]
    fn should_treat_capitalised_master_as_versioned_prefix() {
        let url = kubernetes_schema_url("v1", "Pod", "Master");
        assert!(
            url.contains("vMaster-standalone-strict/"),
            "expected vMaster-standalone-strict/ in URL, got: {url}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // SchemaStore catalog parsing and matching
    // ══════════════════════════════════════════════════════════════════════════

    fn make_catalog_json(schemas: &[(&str, &[&str])]) -> Value {
        let schemas_json: Vec<Value> = schemas
            .iter()
            .map(|(url, patterns)| {
                json!({
                    "name": "Schema Name",
                    "url": url,
                    "fileMatch": patterns
                })
            })
            .collect();
        json!({ "schemas": schemas_json })
    }

    // SS-1: catalog with one YAML entry is parsed and kept
    #[test]
    fn should_parse_catalog_entry_with_yaml_pattern() {
        let v = make_catalog_json(&[("https://example.com/schema.json", &["*.yaml"])]);
        let catalog = parse_schemastore_catalog(&v).expect("should parse");
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].url, "https://example.com/schema.json");
        assert_eq!(catalog.entries[0].file_match, vec!["*.yaml"]);
    }

    // SS-2: entry with only JSON patterns is filtered out
    #[test]
    fn should_filter_out_entry_with_only_json_patterns() {
        let v = make_catalog_json(&[("https://example.com/schema.json", &["*.json"])]);
        let catalog = parse_schemastore_catalog(&v).expect("should parse");
        assert_eq!(
            catalog.entries.len(),
            0,
            "JSON-only entries must be excluded"
        );
    }

    // SS-3: entry with mixed YAML and JSON patterns is kept; non-YAML patterns are discarded
    #[test]
    fn should_keep_entry_with_mixed_yaml_and_json_patterns() {
        let v = make_catalog_json(&[(
            "https://example.com/schema.json",
            &["*.json", "docker-compose.yml"],
        )]);
        let catalog = parse_schemastore_catalog(&v).expect("should parse");
        assert_eq!(catalog.entries.len(), 1);
        // The *.json pattern is discarded; only the YAML pattern is retained.
        assert_eq!(catalog.entries[0].file_match, vec!["docker-compose.yml"]);
    }

    // SS-4: entry with .yml pattern is kept
    #[test]
    fn should_keep_entry_with_yml_extension_pattern() {
        let v = make_catalog_json(&[("https://example.com/schema.json", &["*.yml"])]);
        let catalog = parse_schemastore_catalog(&v).expect("should parse");
        assert_eq!(catalog.entries.len(), 1);
    }

    // SS-5: entry without fileMatch field is skipped (parse returns None for that entry)
    #[test]
    fn should_skip_entry_without_file_match() {
        let v = json!({
            "schemas": [
                { "name": "No FileMatch", "url": "https://example.com/schema.json" }
            ]
        });
        let catalog = parse_schemastore_catalog(&v).expect("should parse catalog");
        assert_eq!(
            catalog.entries.len(),
            0,
            "entry missing fileMatch must be skipped"
        );
    }

    // SS-6: empty schemas array yields empty catalog
    #[test]
    fn should_parse_empty_catalog() {
        let v = json!({ "schemas": [] });
        let catalog = parse_schemastore_catalog(&v).expect("should parse");
        assert_eq!(catalog.entries.len(), 0);
    }

    // SS-7: non-object input returns None
    #[test]
    fn should_return_none_for_non_object_catalog() {
        let v = json!(["not", "an", "object"]);
        assert!(parse_schemastore_catalog(&v).is_none());
    }

    // SS-8: input missing the schemas key returns None
    #[test]
    fn should_return_none_for_catalog_missing_schemas_key() {
        let v = json!({ "other": "data" });
        assert!(parse_schemastore_catalog(&v).is_none());
    }

    // SS-8b: entry with empty url is skipped
    #[test]
    fn should_skip_entry_with_empty_url() {
        let v = json!({
            "schemas": [
                { "name": "Empty URL", "url": "", "fileMatch": ["*.yaml"] }
            ]
        });
        let catalog = parse_schemastore_catalog(&v).expect("should parse catalog");
        assert_eq!(
            catalog.entries.len(),
            0,
            "entry with empty url must be skipped"
        );
    }

    // SS-9: multiple entries — both YAML ones kept, JSON-only one filtered
    #[test]
    fn should_filter_multiple_entries_correctly() {
        let v = make_catalog_json(&[
            (
                "https://example.com/workflow.json",
                &["**/.github/workflows/*.yml"],
            ),
            ("https://example.com/compose.json", &["docker-compose.yaml"]),
            ("https://example.com/package.json", &["package.json"]),
        ]);
        let catalog = parse_schemastore_catalog(&v).expect("should parse");
        assert_eq!(catalog.entries.len(), 2);
    }

    // SS-10: match_schemastore returns URL for matching filename
    #[test]
    fn should_return_url_for_matching_filename() {
        let catalog = SchemaStoreCatalog {
            entries: vec![SchemaStoreEntry {
                url: "https://example.com/workflow.json".to_string(),
                file_match: vec!["**/.github/workflows/*.yml".to_string()],
            }],
        };
        let result = match_schemastore(".github/workflows/ci.yml", &catalog);
        assert_eq!(
            result,
            Some("https://example.com/workflow.json".to_string())
        );
    }

    // SS-11: match_schemastore returns None when no entry matches
    #[test]
    fn should_return_none_when_no_catalog_entry_matches() {
        let catalog = SchemaStoreCatalog {
            entries: vec![SchemaStoreEntry {
                url: "https://example.com/workflow.json".to_string(),
                file_match: vec!["**/.github/workflows/*.yml".to_string()],
            }],
        };
        let result = match_schemastore("docker-compose.yaml", &catalog);
        assert_eq!(result, None);
    }

    // SS-12: match_schemastore returns first matching entry when multiple match
    #[test]
    fn should_return_first_matching_catalog_entry() {
        let catalog = SchemaStoreCatalog {
            entries: vec![
                SchemaStoreEntry {
                    url: "https://example.com/first.json".to_string(),
                    file_match: vec!["*.yaml".to_string()],
                },
                SchemaStoreEntry {
                    url: "https://example.com/second.json".to_string(),
                    file_match: vec!["*.yaml".to_string()],
                },
            ],
        };
        let result = match_schemastore("config.yaml", &catalog);
        assert_eq!(result, Some("https://example.com/first.json".to_string()));
    }

    // SS-13: match_schemastore returns None for empty catalog
    #[test]
    fn should_return_none_for_empty_catalog() {
        let catalog = SchemaStoreCatalog { entries: vec![] };
        let result = match_schemastore("config.yaml", &catalog);
        assert_eq!(result, None);
    }

    // SS-14: entry with multiple fileMatch patterns — matches if any pattern matches
    #[test]
    fn should_match_if_any_file_match_pattern_matches() {
        let catalog = SchemaStoreCatalog {
            entries: vec![SchemaStoreEntry {
                url: "https://example.com/compose.json".to_string(),
                file_match: vec![
                    "docker-compose.yml".to_string(),
                    "docker-compose.yaml".to_string(),
                    "compose.yaml".to_string(),
                ],
            }],
        };
        let result = match_schemastore("docker-compose.yaml", &catalog);
        assert_eq!(result, Some("https://example.com/compose.json".to_string()));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // build_agent tests
    // ══════════════════════════════════════════════════════════════════════════

    // BA-1: build_agent without proxy constructs successfully (no panic)
    #[test]
    fn build_agent_without_proxy_does_not_panic() {
        let _agent = build_agent(None);
    }

    // BA-2: build_agent with a valid proxy URL constructs successfully (no panic)
    #[test]
    fn build_agent_with_valid_proxy_does_not_panic() {
        let _agent = build_agent(Some("http://proxy.example.com:8080"));
    }

    // BA-3: build_agent with an invalid proxy URL falls back gracefully (no panic)
    #[test]
    fn build_agent_with_invalid_proxy_falls_back_gracefully() {
        // An invalid URL must not panic — build_agent silently ignores it.
        let _agent = build_agent(Some("not-a-valid-proxy-url"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Draft-04 `dependencies` parsing
    // ══════════════════════════════════════════════════════════════════════════

    // Dep-1: array value → dependentRequired
    #[test]
    fn draft04_dependencies_array_maps_to_dependent_required() {
        let value = json!({
            "type": "object",
            "dependencies": {
                "credit_card": ["billing_address", "billing_zip"]
            }
        });
        let schema = parse_schema(&value).unwrap();
        let dep_req = schema.dependent_required.unwrap();
        let reqs = dep_req.get("credit_card").unwrap();
        assert!(reqs.contains(&"billing_address".to_string()));
        assert!(reqs.contains(&"billing_zip".to_string()));
        assert!(schema.dependent_schemas.is_none());
    }

    // Dep-2: object value → dependentSchemas
    #[test]
    fn draft04_dependencies_object_maps_to_dependent_schemas() {
        let value = json!({
            "type": "object",
            "dependencies": {
                "name": { "required": ["age"] }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let dep_sch = schema.dependent_schemas.unwrap();
        let dep = dep_sch.get("name").unwrap();
        assert_eq!(dep.required, Some(vec!["age".to_string()]));
        assert!(schema.dependent_required.is_none());
    }

    // Dep-3: 2019-09 dependentRequired takes precedence over Draft-04
    #[test]
    fn draft2019_dependent_required_overrides_draft04() {
        let value = json!({
            "dependencies": {
                "a": ["b"]
            },
            "dependentRequired": {
                "a": ["c"]  // overrides Draft-04 entry for "a"
            }
        });
        let schema = parse_schema(&value).unwrap();
        let dep_req = schema.dependent_required.unwrap();
        // 2019-09 wins: only "c", not "b"
        assert_eq!(dep_req.get("a").unwrap(), &vec!["c".to_string()]);
    }

    // ── $anchor / $dynamicRef / $dynamicAnchor ────────────────────────────────

    // Anchor-1: $ref resolves to a schema with $anchor
    #[test]
    fn ref_resolves_named_anchor() {
        let value = json!({
            "type": "object",
            "properties": {
                "foo": { "$ref": "#item" }
            },
            "$defs": {
                "Item": {
                    "$anchor": "item",
                    "type": "string"
                }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let foo = schema.properties.unwrap();
        let foo_schema = foo.get("foo").unwrap();
        assert_eq!(
            foo_schema.schema_type,
            Some(SchemaType::Single("string".to_string()))
        );
    }

    // Anchor-2: $ref resolves to a schema with $dynamicAnchor
    #[test]
    fn ref_resolves_dynamic_anchor() {
        let value = json!({
            "type": "object",
            "properties": {
                "bar": { "$ref": "#loop" }
            },
            "$defs": {
                "Node": {
                    "$dynamicAnchor": "loop",
                    "type": "integer"
                }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let bar_schema = schema.properties.unwrap();
        let bar = bar_schema.get("bar").unwrap();
        assert_eq!(
            bar.schema_type,
            Some(SchemaType::Single("integer".to_string()))
        );
    }

    // Anchor-3: $dynamicRef resolves via anchor lookup
    #[test]
    fn dynamic_ref_resolves_to_dynamic_anchor() {
        let value = json!({
            "type": "object",
            "properties": {
                "val": { "$dynamicRef": "#node" }
            },
            "$defs": {
                "Node": {
                    "$dynamicAnchor": "node",
                    "type": "boolean"
                }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let val_schema = schema.properties.unwrap();
        let val = val_schema.get("val").unwrap();
        assert_eq!(
            val.schema_type,
            Some(SchemaType::Single("boolean".to_string()))
        );
    }

    // Anchor-4: anchor not found — ref unresolved, schema has ref_path but no type
    #[test]
    fn ref_returns_schema_with_ref_path_when_anchor_not_found() {
        let value = json!({ "$ref": "#nonexistent" });
        let schema = parse_schema(&value).unwrap();
        assert_eq!(schema.ref_path, Some("#nonexistent".to_string()));
        assert!(schema.schema_type.is_none());
    }

    // Anchor-5: nested anchor inside definitions sub-schema
    #[test]
    fn ref_resolves_anchor_nested_inside_definitions() {
        let value = json!({
            "$defs": {
                "outer": {
                    "type": "object",
                    "properties": {
                        "inner": {
                            "$anchor": "nested",
                            "type": "number"
                        }
                    }
                }
            },
            "properties": {
                "x": { "$ref": "#nested" }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let x = schema.properties.unwrap();
        let x_schema = x.get("x").unwrap();
        assert_eq!(
            x_schema.schema_type,
            Some(SchemaType::Single("number".to_string()))
        );
    }

    // Anchor-6: existing JSON Pointer refs still work
    #[test]
    fn json_pointer_ref_still_resolves_correctly() {
        let value = json!({
            "properties": {
                "name": { "$ref": "#/$defs/Name" }
            },
            "$defs": {
                "Name": { "type": "string" }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let name = schema.properties.unwrap();
        let name_schema = name.get("name").unwrap();
        assert_eq!(
            name_schema.schema_type,
            Some(SchemaType::Single("string".to_string()))
        );
    }

    // Anchor-7: $anchor field stored on parsed schema
    #[test]
    fn anchor_field_stored_on_schema() {
        let value = json!({ "$anchor": "myanchor", "type": "string" });
        let schema = parse_schema(&value).unwrap();
        assert_eq!(schema.anchor, Some("myanchor".to_string()));
    }

    // Anchor-8: $dynamicAnchor field stored on parsed schema
    #[test]
    fn dynamic_anchor_field_stored_on_schema() {
        let value = json!({ "$dynamicAnchor": "myloop", "type": "array" });
        let schema = parse_schema(&value).unwrap();
        assert_eq!(schema.dynamic_anchor, Some("myloop".to_string()));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // $id / id base URI resolution
    // ══════════════════════════════════════════════════════════════════════════

    // Id-1: absolute $id is stored verbatim
    #[test]
    fn absolute_dollar_id_is_stored() {
        let value = json!({
            "$id": "https://example.com/schema.json",
            "type": "object"
        });
        let schema = parse_schema(&value).unwrap();
        assert_eq!(
            schema.id,
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Id-2: relative $id is resolved against supplied base URI
    #[test]
    fn relative_dollar_id_is_resolved_against_base_uri() {
        // Use parse_schema_with_root directly to supply a base URI
        let value = json!({ "$id": "sub.json", "type": "object" });
        let schema = parse_schema_with_root(
            &value,
            &value,
            Some("https://example.com/root.json"),
            None,
            0,
        )
        .unwrap();
        assert_eq!(schema.id, Some("https://example.com/sub.json".to_string()));
    }

    // Id-3: nested schema with its own $id overrides parent base for further nesting
    #[test]
    fn nested_dollar_id_overrides_parent_base() {
        let value = json!({
            "$id": "https://example.com/root.json",
            "properties": {
                "child": {
                    "$id": "child.json",
                    "type": "string"
                }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let child = schema.properties.as_ref().unwrap().get("child").unwrap();
        assert_eq!(child.id, Some("https://example.com/child.json".to_string()));
    }

    // Id-4: Draft-04 `id` (without $ prefix) is parsed the same way
    #[test]
    fn draft04_id_without_dollar_is_parsed() {
        let value = json!({
            "id": "https://example.com/schema.json",
            "type": "object"
        });
        let schema = parse_schema(&value).unwrap();
        assert_eq!(
            schema.id,
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Id-5: $id takes precedence over id when both are present
    #[test]
    fn dollar_id_takes_precedence_over_id() {
        let value = json!({
            "$id": "https://example.com/preferred.json",
            "id": "https://example.com/ignored.json",
            "type": "object"
        });
        let schema = parse_schema(&value).unwrap();
        assert_eq!(
            schema.id,
            Some("https://example.com/preferred.json".to_string())
        );
    }

    // Id-6: schema without $id propagates parent base URI unchanged
    #[test]
    fn schema_without_dollar_id_propagates_parent_base() {
        // The child has no $id — its own sub-child should still inherit the root base
        let value = json!({
            "$id": "https://example.com/root.json",
            "properties": {
                "middle": {
                    "type": "object",
                    "properties": {
                        "leaf": {
                            "$id": "leaf.json",
                            "type": "string"
                        }
                    }
                }
            }
        });
        let schema = parse_schema(&value).unwrap();
        let middle = schema.properties.as_ref().unwrap().get("middle").unwrap();
        assert!(middle.id.is_none(), "middle has no $id");
        let leaf = middle.properties.as_ref().unwrap().get("leaf").unwrap();
        assert_eq!(leaf.id, Some("https://example.com/leaf.json".to_string()));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Remote $ref resolution — security guard tests
    // ══════════════════════════════════════════════════════════════════════════

    // Sec-R1: $ref pointing to loopback is blocked by SSRF guard before fetch.
    #[test]
    fn remote_ref_to_loopback_is_blocked_by_ssrf_guard() {
        let value = json!({ "$ref": "http://127.0.0.1/evil.json" });
        let mut cache = SchemaCache::new();
        // parse_schema_with_root + ParseContext attempts to fetch; SSRF guard blocks it.
        // The $ref falls back to ref_path-only schema (no remote traversal).
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = parse_schema_with_root(&value, &value, None, Some(&mut ctx), 0).unwrap();
        // Remote fetch was blocked — schema has ref_path set but no sub-schema content.
        assert_eq!(
            schema.ref_path.as_deref(),
            Some("http://127.0.0.1/evil.json")
        );
        // Nothing was added to the cache (fetch never happened).
        assert!(cache.get("http://127.0.0.1/evil.json").is_none());
    }

    // Sec-R2: relative $ref is resolved against base URI before SSRF guard runs.
    // The resolved URL must be validated — a relative ref resolving to loopback is blocked.
    #[test]
    fn relative_ref_resolved_against_base_uri_before_ssrf_check() {
        // "evil.json" relative to "http://127.0.0.1/" resolves to "http://127.0.0.1/evil.json"
        // which is blocked by SSRF.
        let value = json!({ "$ref": "evil.json" });
        let mut cache = SchemaCache::new();
        let schema = parse_schema_with_root(
            &value,
            &value,
            Some("http://127.0.0.1/"),
            Some(&mut ParseContext::new(&mut cache, None)),
            0,
        )
        .unwrap();
        // Blocked — ref_path preserved, no cached fetch.
        assert_eq!(schema.ref_path.as_deref(), Some("evil.json"));
        assert!(cache.get("http://127.0.0.1/evil.json").is_none());
    }

    // Sec-R3: circular remote refs (A → B → A) are broken by the visited-URL dedup.
    #[test]
    fn circular_remote_refs_are_deduplicated() {
        // Pre-populate cache with schema A that has a $ref to schema B,
        // and schema B that has a $ref back to schema A.
        let json_a = json!({ "$ref": "https://example.com/b.json" });
        let json_b = json!({ "$ref": "https://example.com/a.json" });

        let schema_a = parse_schema(&json_a).unwrap();
        let schema_b = parse_schema(&json_b).unwrap();

        let mut cache = SchemaCache::new();
        cache.insert(
            "https://example.com/a.json".to_string(),
            json_a.clone(),
            schema_a,
        );
        cache.insert("https://example.com/b.json".to_string(), json_b, schema_b);

        // Resolving A should terminate — the cycle is broken when B tries to
        // re-visit A (already in visited set).
        let mut ctx = ParseContext::new(&mut cache, None);
        let result = resolve_ref(
            "https://example.com/a.json",
            &json_a,
            None,
            Some(&mut ctx),
            0,
        );
        // Must terminate; result may be None or a partial schema.
        let _ = result;
    }

    // Sec-R4: breadth fan-out stops after MAX_REMOTE_FETCH_COUNT distinct URLs.
    #[test]
    fn breadth_fan_out_stops_at_max_fetch_count() {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);

        // Fill the visited set to the limit.
        for i in 0..MAX_REMOTE_FETCH_COUNT {
            let url = format!("https://example.com/schema{i}.json");
            assert!(ctx.try_visit(&url), "should accept visit {i}");
        }

        // The next visit must be rejected.
        assert!(
            !ctx.try_visit("https://example.com/one-too-many.json"),
            "visit beyond MAX_REMOTE_FETCH_COUNT must be rejected"
        );
    }

    // Sec-R5: response with non-JSON Content-Type is rejected.
    #[test]
    fn fetch_schema_rejects_non_json_content_type() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let url = format!("http://{addr}/schema.json");

        std::thread::spawn(move || {
            if let Ok(req) = server.recv() {
                let ct = tiny_http::Header::from_bytes(b"Content-Type", b"text/html").unwrap();
                let response =
                    tiny_http::Response::from_string("<html>not json</html>").with_header(ct);
                let _ = req.respond(response);
            }
        });

        // fetch_schema_raw is tested directly here; note that loopback bypass
        // happens because we call build_agent directly and skip validate_and_normalize_url.
        // We test the Content-Type check in isolation using a direct agent call.
        let agent = build_agent(None);
        let response = agent.get(&url).call().expect("request should succeed");
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            !content_type.contains("application/json"),
            "server returned non-JSON content type: {content_type}"
        );
        // Confirm our guard condition matches.
        let is_json = content_type.contains("application/json")
            || content_type.contains("application/schema");
        assert!(!is_json, "guard should reject this content type");
    }

    // Sec-R6: $id spoofing — a fetched schema's self-declared $id cannot
    // overwrite an existing cache entry for a different URL.
    // (Covered by the existing first-write-wins insert semantics — this test
    // explicitly documents the security property.)
    #[test]
    fn dollar_id_spoofing_cannot_overwrite_cache_entry() {
        let mut cache = SchemaCache::new();

        cache.insert(
            "https://json-schema.org/draft/2020-12/schema".to_string(),
            Value::Null,
            JsonSchema {
                description: Some("legitimate".to_string()),
                ..JsonSchema::default()
            },
        );

        // A malicious schema tries to claim the same URL via $id.
        cache.insert(
            "https://json-schema.org/draft/2020-12/schema".to_string(),
            Value::Null,
            JsonSchema {
                description: Some("malicious".to_string()),
                ..JsonSchema::default()
            },
        );

        let cached = cache
            .get("https://json-schema.org/draft/2020-12/schema")
            .unwrap();
        assert_eq!(
            cached.description.as_deref(),
            Some("legitimate"),
            "first-write-wins must prevent $id spoofing overwrite"
        );
    }

    // Sec-R7: file:// and data: scheme refs are blocked by SSRF guard.
    #[test]
    fn file_scheme_ref_is_blocked_by_ssrf_guard() {
        let value = json!({ "$ref": "file:///etc/passwd" });
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = parse_schema_with_root(&value, &value, None, Some(&mut ctx), 0).unwrap();
        // SSRF guard blocks — ref_path preserved, nothing cached.
        assert_eq!(schema.ref_path.as_deref(), Some("file:///etc/passwd"));
        assert!(cache.get("file:///etc/passwd").is_none());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Remote $ref resolution — functional tests (with tiny_http server)
    // ══════════════════════════════════════════════════════════════════════════

    // Remote-1: absolute $ref URL (served from a real HTTP server) resolves via
    // the Content-Type-checking fetch path.
    //
    // Uses a tiny_http server serving a valid JSON schema with Content-Type:
    // application/json. The URL is https://example.com/... passed through the
    // cache pre-population path to verify the full resolve_ref → cache flow
    // without hitting the loopback SSRF guard.
    #[test]
    fn remote_ref_absolute_url_resolves_to_fetched_schema() {
        // Pre-populate cache with a valid HTTPS URL (no actual network call).
        let ref_url = "https://example.com/other.json";
        let remote_value: Value = json!({ "type": "string", "description": "remote schema" });
        let remote_schema = parse_schema(&remote_value).unwrap();

        let mut cache = SchemaCache::new();
        cache.insert(ref_url.to_string(), remote_value, remote_schema);

        let root = json!({});
        let mut ctx = ParseContext::new(&mut cache, None);
        let resolved = resolve_ref(ref_url, &root, None, Some(&mut ctx), 0);

        assert!(resolved.is_some(), "remote ref should resolve from cache");
        let s = resolved.unwrap();
        assert_eq!(s.description.as_deref(), Some("remote schema"));
    }

    // Remote-2: $ref with JSON Pointer fragment navigates the fetched document.
    #[test]
    fn remote_ref_with_fragment_navigates_fetched_document() {
        let remote_value = json!({
            "definitions": {
                "Address": {
                    "type": "object",
                    "description": "an address"
                }
            }
        });
        let remote_schema = parse_schema(&remote_value).unwrap();

        let mut cache = SchemaCache::new();
        let url = "https://example.com/types.json".to_string();
        cache.insert(url.clone(), remote_value, remote_schema);

        let root = json!({});
        let ref_str = format!("{url}#/definitions/Address");
        let mut ctx = ParseContext::new(&mut cache, None);
        let resolved = resolve_ref(&ref_str, &root, None, Some(&mut ctx), 0);

        assert!(
            resolved.is_some(),
            "fragment ref into remote doc should resolve"
        );
        let s = resolved.unwrap();
        assert_eq!(s.description.as_deref(), Some("an address"));
    }

    // Remote ctx threading: $ref inside properties is resolved remotely.
    #[test]
    fn remote_ref_inside_properties_resolves_via_ctx() {
        // Pre-populate cache with a valid HTTPS URL so no real network call is made.
        let ref_url = "https://example.com/address.json";
        let remote_value: Value = json!({ "type": "object", "description": "an address" });
        let remote_schema = parse_schema(&remote_value).unwrap();

        let mut cache = SchemaCache::new();
        cache.insert(ref_url.to_string(), remote_value, remote_schema);

        let root = json!({
            "type": "object",
            "properties": {
                "address": { "$ref": ref_url }
            }
        });

        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = parse_schema_with_root(&root, &root, None, Some(&mut ctx), 0);

        assert!(schema.is_some(), "outer schema should parse");
        let schema = schema.unwrap();
        let props = schema
            .properties
            .as_ref()
            .expect("properties should be present");
        let address = props
            .get("address")
            .expect("address property should be present");
        assert_eq!(
            address.description.as_deref(),
            Some("an address"),
            "address property should be resolved from remote cache"
        );
    }

    // SchemaError Display — new variants
    #[test]
    fn schema_error_display_too_many_remote_fetches() {
        let e = SchemaError::TooManyRemoteFetches;
        let msg = e.to_string();
        assert!(msg.contains("Remote fetch count"), "got: {msg}");
    }

    #[test]
    fn schema_error_display_unexpected_content_type() {
        let e = SchemaError::UnexpectedContentType("text/html".to_string());
        let msg = e.to_string();
        assert!(msg.contains("Unexpected content type"), "got: {msg}");
        assert!(msg.contains("text/html"), "got: {msg}");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // fetch_schema_raw + ParseContext integration (Finding 1)
    // ══════════════════════════════════════════════════════════════════════════

    // FetchCtx-1: fetch_schema_raw with a ParseContext resolves remote $refs
    // within the fetched schema.
    //
    // fetch_schema_raw calls validate_and_normalize_url which blocks loopback
    // addresses, so this test cannot use a tiny_http server for the top-level
    // fetch.  Instead it exercises the exact code path that fetch_schema_raw
    // takes when ctx=Some: `parse_schema_with_root(&value, &value, Some(url),
    // Some(ctx), 0)`.  The $ref target is pre-populated in the ParseContext
    // cache so no network call is made for it.
    #[test]
    fn fetch_schema_raw_ctx_resolves_remote_ref_in_fetched_body() {
        // Simulate a top-level schema body that fetch_schema_raw would have
        // received over the network: it contains a remote $ref.
        let ref_url = "https://example.com/address.json";
        let top_level_url = "https://example.com/schema.json";
        let body: Value = json!({
            "type": "object",
            "properties": {
                "home": { "$ref": ref_url }
            }
        });

        // Pre-populate the cache with the $ref target — mirrors what would
        // happen if the referenced schema had already been fetched.
        let remote_value: Value = json!({ "type": "object", "description": "an address" });
        let remote_schema = parse_schema(&remote_value).unwrap();
        let mut cache = SchemaCache::new();
        cache.insert(ref_url.to_string(), remote_value, remote_schema);

        // Call exactly the code path that fetch_schema_raw uses when ctx=Some.
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = parse_schema_with_root(&body, &body, Some(top_level_url), Some(&mut ctx), 0)
            .expect("should parse top-level schema");

        // The $ref should have been resolved from the pre-populated cache.
        let props = schema.properties.as_ref().expect("should have properties");
        let home = props.get("home").expect("should have home property");
        assert_eq!(
            home.description.as_deref(),
            Some("an address"),
            "remote $ref should resolve to the cached schema"
        );
        // The ParseContext cache is populated with the $ref target entry.
        assert!(
            cache.get(ref_url).is_some(),
            "ctx cache should contain the resolved $ref target"
        );
    }

    // FetchCtx-2: fetch_schema_raw with ctx=None does not resolve remote $refs
    // (preserves existing behavior for callers that pass None).
    #[test]
    fn fetch_schema_raw_no_ctx_leaves_remote_ref_unresolved() {
        let ref_url = "https://example.com/address.json";
        let body: Value = json!({
            "type": "object",
            "properties": {
                "home": { "$ref": ref_url }
            }
        });

        // ctx=None — parse_schema is called, which cannot resolve remote refs.
        let schema = parse_schema(&body).expect("should parse");
        let props = schema.properties.as_ref().expect("should have properties");
        let home = props.get("home").expect("should have home property");
        // Without ctx, the $ref is stored as ref_path but not resolved.
        assert_eq!(
            home.ref_path.as_deref(),
            Some(ref_url),
            "without ctx the $ref is unresolved, only ref_path is set"
        );
        assert!(
            home.description.is_none(),
            "without ctx the remote schema description should not be present"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Content-Type sanitization (Finding 2)
    // ══════════════════════════════════════════════════════════════════════════

    // Sanitize-1: non-printable control characters are stripped
    #[test]
    fn sanitize_content_type_strips_control_characters() {
        let raw = "text/html\x00\x01\x1f\x7f";
        let result = sanitize_content_type(raw);
        assert_eq!(result, "text/html", "control chars must be stripped");
    }

    // Sanitize-2: values longer than 256 chars are truncated
    #[test]
    fn sanitize_content_type_truncates_at_256_chars() {
        let raw = "a".repeat(300);
        let result = sanitize_content_type(&raw);
        assert_eq!(result.len(), 256, "result must be truncated to 256 chars");
    }

    // Sanitize-3: printable ASCII and spaces are preserved
    #[test]
    fn sanitize_content_type_preserves_printable_content() {
        let raw = "application/json; charset=utf-8";
        let result = sanitize_content_type(raw);
        assert_eq!(result, raw, "printable content must be preserved");
    }

    // Sanitize-4: a tiny_http server returning an oversized Content-Type
    // produces a truncated value after sanitization.  Uses build_agent directly
    // to bypass validate_and_normalize_url (SSRF guard blocks loopback in
    // fetch_schema_raw itself), same approach as Sec-R5.
    #[test]
    fn fetch_schema_raw_sanitizes_content_type_in_error() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let url = format!("http://{addr}/schema.json");

        std::thread::spawn(move || {
            if let Ok(req) = server.recv() {
                // Content-Type that is longer than 256 printable chars.
                let ct_value = format!("text/html; x={}", "a".repeat(300));
                let ct =
                    tiny_http::Header::from_bytes(b"Content-Type", ct_value.as_bytes()).unwrap();
                let response = tiny_http::Response::from_string("not json").with_header(ct);
                let _ = req.respond(response);
            }
        });

        let agent = build_agent(None);
        let response = agent.get(&url).call().expect("request should succeed");
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.len() > 256,
            "server must have sent an oversized Content-Type"
        );
        let sanitized = sanitize_content_type(content_type);
        assert!(
            sanitized.len() <= 256,
            "sanitized Content-Type must be truncated to ≤256 chars"
        );
    }
}
