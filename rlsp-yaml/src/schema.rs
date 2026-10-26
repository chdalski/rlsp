// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::net::IpAddr;
use std::sync::Arc;

use serde_json::Value;
use tower_lsp::lsp_types::Url;

pub use association::*;

/// Schema association management: glob-pattern-to-schema-URL mappings.
pub mod association;

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
        }
    }
}

/// The JSON Schema type keyword — a single type string or an array of types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaType {
    /// A single type string (e.g. `"string"`).
    Single(String),
    /// An array of type strings (e.g. `["string", "null"]`).
    Multiple(Vec<String>),
}

/// Whether `additionalProperties` is `false` or a sub-schema.
#[derive(Debug, Clone)]
pub enum AdditionalProperties {
    /// `additionalProperties: false` — no undeclared properties are allowed.
    Denied,
    /// `additionalProperties` is a sub-schema that additional properties must match.
    Schema(Box<JsonSchema>),
}

/// The result of parsing a schema document.
///
/// A parsed schema is shared (`Arc<ParsedSchema>`) between the cache and every
/// request that reads it, so it deliberately has no `Clone`.
#[derive(Debug)]
pub struct ParsedSchema {
    /// The root schema.
    pub root: JsonSchema,
}

impl ParsedSchema {
    /// Wrap `root` as a parsed schema.
    #[must_use]
    pub const fn new(root: JsonSchema) -> Self {
        Self { root }
    }
}

/// A subset of JSON Schema (Draft-04 and Draft-07) sufficient for validation,
/// completion, and hover support.
#[derive(Debug, Clone, Default)]
pub struct JsonSchema {
    /// Schema identifier (`$id` in Draft-07, `id` in Draft-04).
    pub id: Option<String>,
    /// Value of the `type` keyword.
    pub schema_type: Option<SchemaType>,
    /// Short human-readable label for the schema.
    pub title: Option<String>,
    /// Longer human-readable description for the schema.
    pub description: Option<String>,
    /// `format` annotation (e.g. `"date-time"`, `"uri"`).
    pub format: Option<String>,
    /// `contentEncoding` annotation (e.g. `"base64"`).
    pub content_encoding: Option<String>,
    /// `contentMediaType` annotation (e.g. `"application/json"`).
    pub content_media_type: Option<String>,
    /// `contentSchema` sub-schema describing the decoded content.
    pub content_schema: Option<Box<Self>>,
    /// Named property sub-schemas from the `properties` keyword.
    pub properties: Option<HashMap<String, Self>>,
    /// List of property names that must be present (`required` keyword).
    pub required: Option<Vec<String>>,
    /// Allowed values from the `enum` keyword.
    pub enum_values: Option<Vec<Value>>,
    /// Default value annotation.
    pub default: Option<Value>,
    /// Example values annotation.
    pub examples: Option<Vec<Value>>,
    /// Sub-schema for array items (`items` keyword, Draft-07 and earlier).
    pub items: Option<Box<Self>>,
    /// Positional item sub-schemas (`prefixItems` keyword, Draft-2020-12).
    pub prefix_items: Option<Vec<Self>>,
    /// Sub-schema that at least one array item must match (`contains` keyword).
    pub contains: Option<Box<Self>>,
    /// Minimum number of array items.
    pub min_items: Option<u64>,
    /// Maximum number of array items.
    pub max_items: Option<u64>,
    /// Maximum number of items matching `contains`.
    pub max_contains: Option<u64>,
    /// Minimum number of items matching `contains`.
    pub min_contains: Option<u64>,
    /// Whether all array items must be unique (`uniqueItems` keyword).
    pub unique_items: Option<bool>,
    /// Schema or denial for properties not listed in `properties`.
    pub additional_properties: Option<AdditionalProperties>,
    /// Schema or denial for items beyond `items`/`prefixItems` (`additionalItems`).
    pub additional_items: Option<AdditionalProperties>,
    /// Minimum number of object properties.
    pub min_properties: Option<u64>,
    /// Maximum number of object properties.
    pub max_properties: Option<u64>,
    /// Sub-schemas matched by property name regex (`patternProperties`).
    pub pattern_properties: Option<Vec<(String, Self)>>,
    /// Sub-schema that each property name must match (`propertyNames`).
    pub property_names: Option<Box<Self>>,
    /// All sub-schemas must be satisfied (`allOf`).
    pub all_of: Option<Vec<Self>>,
    /// At least one sub-schema must be satisfied (`anyOf`).
    pub any_of: Option<Vec<Self>>,
    /// Exactly one sub-schema must be satisfied (`oneOf`).
    pub one_of: Option<Vec<Self>>,
    /// Sub-schema that the value must NOT satisfy (`not`).
    pub not: Option<Box<Self>>,
    /// Condition sub-schema for `if`/`then`/`else`.
    pub if_schema: Option<Box<Self>>,
    /// Applied when `if_schema` is satisfied.
    pub then_schema: Option<Box<Self>>,
    /// Applied when `if_schema` is not satisfied.
    pub else_schema: Option<Box<Self>>,
    /// `$ref` target (resolved to an absolute URI or JSON-pointer fragment).
    pub ref_path: Option<String>,
    /// `$anchor` value for fragment-based `$ref` resolution.
    pub anchor: Option<String>,
    /// `$dynamicAnchor` value for dynamic `$ref` resolution.
    pub dynamic_anchor: Option<String>,
    /// Regular-expression pattern the string value must match.
    pub pattern: Option<String>,
    /// Inclusive minimum for numeric values.
    pub minimum: Option<f64>,
    /// Inclusive maximum for numeric values.
    pub maximum: Option<f64>,
    /// Minimum number of Unicode characters in a string.
    pub min_length: Option<u64>,
    /// Maximum number of Unicode characters in a string.
    pub max_length: Option<u64>,
    /// Exclusive minimum for numeric values (Draft-07+).
    pub exclusive_minimum: Option<f64>,
    /// Exclusive maximum for numeric values (Draft-07+).
    pub exclusive_maximum: Option<f64>,
    /// `exclusiveMinimum: true` flag (Draft-04 boolean form).
    pub exclusive_minimum_draft04: Option<bool>,
    /// `exclusiveMaximum: true` flag (Draft-04 boolean form).
    pub exclusive_maximum_draft04: Option<bool>,
    /// Value must be a multiple of this number.
    pub multiple_of: Option<f64>,
    /// Value must be exactly equal to this constant (`const` keyword).
    pub const_value: Option<serde_json::Value>,
    /// Property dependency requirements (`dependentRequired`).
    pub dependent_required: Option<HashMap<String, Vec<String>>>,
    /// Property dependency sub-schemas (`dependentSchemas`).
    pub dependent_schemas: Option<HashMap<String, Self>>,
    /// Whether the schema is deprecated.
    pub deprecated: Option<bool>,
    /// Schema or denial for unevaluated properties (`unevaluatedProperties`).
    pub unevaluated_properties: Option<AdditionalProperties>,
    /// Sub-schema for unevaluated array items (`unevaluatedItems`).
    pub unevaluated_items: Option<Box<Self>>,
}

/// A mapping from a file glob pattern to a JSON Schema URL.
#[derive(Debug, Clone)]
pub struct SchemaAssociation {
    /// Glob pattern matched against the document file path.
    pub pattern: String,
    /// URL of the JSON Schema to apply to matching documents.
    pub url: String,
}

/// A single entry from the `SchemaStore` catalog.
#[derive(Debug, Clone)]
pub struct SchemaStoreEntry {
    /// URL of the JSON Schema.
    pub url: String,
    /// Glob patterns for files this schema applies to.
    pub file_match: Vec<String>,
}

/// The parsed `SchemaStore` catalog, filtered to YAML-relevant entries.
#[derive(Debug, Clone, Default)]
pub struct SchemaStoreCatalog {
    /// All YAML-relevant schema entries from the catalog.
    pub entries: Vec<SchemaStoreEntry>,
}

/// In-memory cache of parsed JSON Schemas, keyed by normalized URL.
///
/// Each entry stores the raw `Value` alongside the shared `ParsedSchema` so that
/// fragment-bearing `$ref` values (`other.json#/definitions/Foo`) can navigate
/// the raw document with a JSON Pointer after the initial fetch.
///
/// Schemas are keyed by **fetch URL**, never by a document's self-declared
/// `$id`, to prevent `$id`-spoofing cache poisoning.
#[derive(Debug, Default)]
pub struct SchemaCache {
    inner: HashMap<String, (Value, Arc<ParsedSchema>)>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema cache
// ──────────────────────────────────────────────────────────────────────────────

impl SchemaCache {
    /// Create a new empty schema cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a cached schema by URL, or `None` on a cache miss.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&Arc<ParsedSchema>> {
        self.inner.get(url).map(|(_, s)| s)
    }

    /// Insert a schema into the cache.  The first insertion for a given URL
    /// wins; subsequent calls for the same key are silently ignored.
    pub fn insert(&mut self, url: String, value: Value, schema: Arc<ParsedSchema>) {
        self.inner.entry(url).or_insert((value, schema));
    }

    /// Return a cached (raw value, parsed schema) pair by URL, or `None`.
    #[must_use]
    fn get_raw(&self, url: &str) -> Option<&(Value, Arc<ParsedSchema>)> {
        self.inner.get(url)
    }

    /// Return whether the URL is already in the cache (avoids a fetch).
    #[must_use]
    pub fn contains(&self, url: &str) -> bool {
        self.inner.contains_key(url)
    }

    /// Consume `self` and return the underlying map, for use when merging a
    /// returned cache back into the live cache after a `spawn_blocking` pass.
    pub(crate) fn into_inner(self) -> HashMap<String, (Value, Arc<ParsedSchema>)> {
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
/// `fetch_json` callers use this helper so agent construction is consistent.
fn build_agent(proxy: Option<&str>) -> ureq::Agent {
    let mut builder = ureq::Agent::config_builder()
        .max_redirects(0)
        .timeout_connect(Some(std::time::Duration::from_secs(5)))
        .timeout_global(Some(std::time::Duration::from_secs(15)));
    if let Some(url) = proxy {
        // A malformed proxy URL is silently ignored; the agent falls back to
        // direct connections. Intentional: for an LSP server the worst outcome
        // is a failed schema fetch, not a security incident.
        if let Ok(p) = ureq::Proxy::new(url) {
            builder = builder.proxy(Some(p));
        }
    }
    builder.build().new_agent()
}

/// GET `url` with `agent` and parse the response body as JSON.
///
/// The response is not required to carry any particular `Content-Type`:
/// hosts such as `raw.githubusercontent.com` serve JSON as `text/plain`, so
/// the size cap and the JSON parse decide whether a response is usable.
/// The body is read only after the status is checked, through the decompressed
/// reader, and never beyond `MAX_SCHEMA_BYTES + 1` bytes.
///
/// # Errors
///
/// - [`SchemaError::FetchFailed`] on network failure, a 4xx/5xx HTTP status,
///   or a 3xx response (redirects are not followed).
/// - [`SchemaError::ResponseTooLarge`] when the body exceeds `MAX_SCHEMA_BYTES`.
/// - [`SchemaError::ParseFailed`] when the body is not valid JSON.
fn fetch_json(agent: &ureq::Agent, url: &str) -> Result<Value, SchemaError> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| SchemaError::FetchFailed(e.to_string()))?;

    // ureq reports 4xx/5xx as errors itself, so only 2xx and 3xx reach here.
    // `build_agent` disables redirect following, so a 3xx response comes back
    // as a success; its body is not a schema.
    let status = response.status();
    if status.is_redirection() {
        return Err(SchemaError::FetchFailed(format!(
            "HTTP status {}: redirects are not followed",
            status.as_u16()
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

    if buf.len() as u64 > MAX_SCHEMA_BYTES {
        return Err(SchemaError::ResponseTooLarge);
    }

    serde_json::from_slice(&buf).map_err(|e| SchemaError::ParseFailed(e.to_string()))
}

/// Fetch a JSON Schema from `url`, returning the raw `Value` and parsed schema.
///
/// `url` is validated with [`validate_and_normalize_url`] before any network
/// request.  This function is blocking; call it via
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
/// - [`SchemaError::UrlNotPermitted`] when `url` fails the SSRF guard.
/// - [`SchemaError::FetchFailed`] on network failure, a 4xx/5xx HTTP status,
///   or a 3xx response (redirects are not followed).
/// - [`SchemaError::ResponseTooLarge`] when the body exceeds `MAX_SCHEMA_BYTES`.
/// - [`SchemaError::ParseFailed`] when the body is not JSON or not a schema.
/// - [`SchemaError::TooDeep`] when the JSON nests beyond the depth limit.
pub fn fetch_schema_raw(
    url: &str,
    proxy: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
) -> Result<(Value, ParsedSchema), SchemaError> {
    // Validate and normalise the URL before issuing any network request.
    validate_and_normalize_url(url)?;

    let value = fetch_json(&build_agent(proxy), url)?;

    check_json_depth(&value, 0)?;

    let schema = ctx
        .map_or_else(
            || parse_schema(&value),
            |ctx| {
                SchemaParser::new(&value, Some(ctx))
                    .parse(&value, Some(url), 0)
                    .map(ParsedSchema::new)
            },
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
/// - [`SchemaError::FetchFailed`] on network failure, a 4xx/5xx HTTP status,
///   or a 3xx response (redirects are not followed).
/// - [`SchemaError::ResponseTooLarge`] when the body exceeds `MAX_SCHEMA_BYTES`.
/// - [`SchemaError::ParseFailed`] when the body is not JSON or not a catalog.
pub fn fetch_schemastore_catalog(proxy: Option<&str>) -> Result<SchemaStoreCatalog, SchemaError> {
    let value = fetch_json(&build_agent(proxy), SCHEMASTORE_CATALOG_URL)?;

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
            .any(|pattern| association::glob_matches(pattern, filename));
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

/// Context held by the schema parser to enable remote `$ref` resolution with
/// breadth and deduplication guards.
pub struct ParseContext<'a> {
    cache: &'a mut SchemaCache,
    proxy: Option<&'a str>,
    /// URLs fetched during this resolution pass (dedup + breadth limit).
    visited: HashSet<String>,
}

impl<'a> ParseContext<'a> {
    /// Create a new parse context with the given cache and optional proxy URL.
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

type ParsedDependencies = (
    Option<HashMap<String, Vec<String>>>,
    Option<HashMap<String, JsonSchema>>,
);

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

/// Parse a `serde_json::Value` into a [`ParsedSchema`].
///
/// Returns `None` if the value is not a JSON object (or boolean — see below).
///
/// Boolean schemas:
/// - `true`  → empty (permissive) schema
/// - `false` → `None` (no schema representation for "reject everything")
#[must_use]
pub fn parse_schema(value: &Value) -> Option<ParsedSchema> {
    SchemaParser::new(value, None)
        .parse(value, None, 0)
        .map(ParsedSchema::new)
}

/// The state that stays constant while one document is parsed: the document
/// that `$ref` fragments resolve against and the optional remote context.
///
/// The base URI and depth vary per sub-schema, so they are arguments of the
/// parse methods instead.
struct SchemaParser<'r, 'c> {
    root: &'r Value,
    ctx: Option<&'r mut ParseContext<'c>>,
}

impl<'r, 'c> SchemaParser<'r, 'c> {
    const fn new(root: &'r Value, ctx: Option<&'r mut ParseContext<'c>>) -> Self {
        Self { root, ctx }
    }

    /// Parse the contents of `contentSchema` from the schema object.
    fn parse_content_schema(
        &mut self,
        schema: &mut JsonSchema,
        obj: &serde_json::Map<String, Value>,
        base_uri: Option<&str>,
        depth: usize,
    ) {
        if let Some(cs) = obj.get("contentSchema") {
            schema.content_schema = self.parse(cs, base_uri, depth + 1).map(Box::new);
        }
    }

    /// Populate `properties` and `patternProperties` on `schema` from `obj`.
    fn parse_object_fields(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        base_uri: Option<&str>,
        depth: usize,
        schema: &mut JsonSchema,
    ) {
        if let Some(map) = obj.get("properties").and_then(Value::as_object) {
            let mut props = HashMap::new();
            for (k, v) in map {
                if let Some(s) = self.parse(v, base_uri, depth + 1) {
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
                if let Some(s) = self.parse(v, base_uri, depth + 1) {
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
        &mut self,
        obj: &serde_json::Map<String, Value>,
        base_uri: Option<&str>,
        depth: usize,
        schema: &mut JsonSchema,
    ) {
        // prefixItems (Draft 2020-12)
        if let Some(arr) = obj.get("prefixItems").and_then(Value::as_array) {
            let mut items = Vec::new();
            for v in arr {
                if let Some(s) = self.parse(v, base_uri, depth + 1) {
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
                    if let Some(s) = self.parse(v, base_uri, depth + 1) {
                        items.push(s);
                    }
                }
                if !items.is_empty() {
                    schema.prefix_items = Some(items);
                }
            }
            Some(v) => {
                schema.items = self.parse(v, base_uri, depth + 1).map(Box::new);
            }
            None => {}
        }

        // additionalItems — only relevant in Draft-04/07 tuple mode (array-form items, not prefixItems)
        if obj.get("items").is_some_and(Value::is_array) && obj.get("prefixItems").is_none() {
            schema.additional_items =
                self.parse_additional_properties(obj.get("additionalItems"), base_uri, depth);
        }

        schema.contains = obj
            .get("contains")
            .and_then(|v| self.parse(v, base_uri, depth + 1))
            .map(Box::new);
        schema.min_items = obj.get("minItems").and_then(Value::as_u64);
        schema.max_items = obj.get("maxItems").and_then(Value::as_u64);
        schema.min_contains = obj.get("minContains").and_then(Value::as_u64);
        schema.max_contains = obj.get("maxContains").and_then(Value::as_u64);
        schema.unique_items = obj.get("uniqueItems").and_then(Value::as_bool);
    }

    /// Populate allOf/anyOf/oneOf/not and if/then/else fields on `schema` from `obj`.
    fn parse_combinator_fields(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        base_uri: Option<&str>,
        depth: usize,
        schema: &mut JsonSchema,
    ) {
        schema.all_of = self.parse_schema_array(obj.get("allOf"), base_uri, depth);
        schema.any_of = self.parse_schema_array(obj.get("anyOf"), base_uri, depth);
        schema.one_of = self.parse_schema_array(obj.get("oneOf"), base_uri, depth);
        schema.not = obj
            .get("not")
            .and_then(|v| self.parse(v, base_uri, depth + 1))
            .map(Box::new);

        // if / then / else (Draft-07)
        schema.if_schema = obj
            .get("if")
            .and_then(|v| self.parse(v, base_uri, depth + 1))
            .map(Box::new);
        schema.then_schema = obj
            .get("then")
            .and_then(|v| self.parse(v, base_uri, depth + 1))
            .map(Box::new);
        schema.else_schema = obj
            .get("else")
            .and_then(|v| self.parse(v, base_uri, depth + 1))
            .map(Box::new);
    }

    /// Populate `unevaluatedProperties` and `unevaluatedItems` on `schema`
    /// from `obj`. Extracted to keep `parse` under 100 lines.
    fn parse_extension_fields(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        base_uri: Option<&str>,
        depth: usize,
        schema: &mut JsonSchema,
    ) {
        // unevaluatedProperties / unevaluatedItems (Draft 2019-09)
        schema.unevaluated_properties =
            self.parse_additional_properties(obj.get("unevaluatedProperties"), base_uri, depth);
        schema.unevaluated_items = obj
            .get("unevaluatedItems")
            .and_then(|v| self.parse(v, base_uri, depth + 1))
            .map(Box::new);
    }

    fn parse(&mut self, value: &Value, base_uri: Option<&str>, depth: usize) -> Option<JsonSchema> {
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
            if let Some(resolved) = self.resolve_ref(ref_str, base_uri, depth + 1) {
                return Some(resolved);
            }
            return Some(schema);
        }

        // $dynamicRef — same resolution as $ref for single-document schemas
        if let Some(Value::String(ref_str)) = obj.get("$dynamicRef")
            && let Some(resolved) = self.resolve_ref(ref_str, base_uri, depth + 1)
        {
            return Some(resolved);
        }
        // Fall through if unresolved — parse remaining fields

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
        self.parse_content_schema(&mut schema, obj, effective_base, depth);

        // required
        schema.required = obj.get("required").and_then(Value::as_array).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

        self.parse_object_fields(obj, effective_base, depth, &mut schema);
        self.parse_array_fields(obj, effective_base, depth, &mut schema);

        // additionalProperties
        schema.additional_properties = self.parse_additional_properties(
            obj.get("additionalProperties"),
            effective_base,
            depth,
        );

        // propertyNames
        schema.property_names = obj
            .get("propertyNames")
            .and_then(|v| self.parse(v, effective_base, depth + 1))
            .map(Box::new);

        // dependencies (Draft-04) / dependentRequired + dependentSchemas (Draft 2019-09)
        let (dep_req, dep_sch) = self.parse_dependencies(obj, effective_base, depth);
        schema.dependent_required = dep_req;
        schema.dependent_schemas = dep_sch;

        self.parse_combinator_fields(obj, effective_base, depth, &mut schema);
        self.parse_extension_fields(obj, effective_base, depth, &mut schema);

        Some(schema)
    }

    /// Parse `dependencies` (Draft-04), `dependentRequired`, and `dependentSchemas`
    /// (Draft 2019-09) from a schema object, merging into a unified pair of maps.
    /// 2019-09 entries take precedence over Draft-04 `dependencies` on key collision.
    fn parse_dependencies(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        base_uri: Option<&str>,
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
                } else if let Some(schema) = self.parse(val, base_uri, depth + 1) {
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
                if let Some(schema) = self.parse(val, base_uri, depth + 1) {
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

    fn parse_additional_properties(
        &mut self,
        value: Option<&Value>,
        base_uri: Option<&str>,
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
            | Value::Object(_)) => self
                .parse(v, base_uri, depth + 1)
                .map(|s| AdditionalProperties::Schema(Box::new(s))),
        }
    }

    fn parse_schema_array(
        &mut self,
        value: Option<&Value>,
        base_uri: Option<&str>,
        depth: usize,
    ) -> Option<Vec<JsonSchema>> {
        let arr = value?.as_array()?;
        let mut schemas = Vec::new();
        for v in arr {
            if let Some(s) = self.parse(v, base_uri, depth + 1) {
                schemas.push(s);
            }
        }
        if schemas.is_empty() {
            None
        } else {
            Some(schemas)
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // $ref resolution
    // ──────────────────────────────────────────────────────────────────────────

    /// Resolve a `$ref` value, handling both local fragment refs and remote URIs.
    ///
    /// **Local refs** (start with `#`):
    /// - `#`       → root schema
    /// - `#/a/b`   → JSON Pointer (RFC 6901) into root
    /// - `#name`   → named anchor (`$anchor` or `$dynamicAnchor`) in root
    ///
    /// **Remote refs** (everything else — requires the remote context):
    /// - `https://example.com/other.json`          → fetch entire document
    /// - `https://example.com/other.json#/defs/Foo` → fetch + JSON Pointer
    /// - `sub.json` resolved against `base_uri`
    ///
    /// Every target is parsed by a fresh parser without a remote context, so
    /// `$ref`s inside a target are not fetched (depth-1 remote resolution).
    ///
    /// Returns `None` if the ref cannot be resolved or limits are exceeded.
    fn resolve_ref(
        &mut self,
        ref_str: &str,
        base_uri: Option<&str>,
        depth: usize,
    ) -> Option<JsonSchema> {
        if depth > MAX_REF_DEPTH {
            return None;
        }

        // ── Local fragment ref ────────────────────────────────────────────────
        if let Some(pointer) = ref_str.strip_prefix('#') {
            let root = self.root;
            let target = if pointer.is_empty() {
                root
            } else if pointer.starts_with('/') {
                root.pointer(pointer)?
            } else {
                // Named anchor lookup
                find_anchor_in_value(pointer, root)?
            };
            return SchemaParser::new(root, None).parse(target, None, depth + 1);
        }

        // ── Remote ref ────────────────────────────────────────────────────────
        let ctx = self.ctx.as_deref_mut()?; // remote resolution requires a context

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
            ctx.cache
                .insert(normalized.clone(), value, Arc::new(schema));
        }

        let (remote_value, _) = ctx.cache.get_raw(&normalized)?;
        let remote_value = remote_value.clone(); // clone to release borrow on cache

        let target = match fragment {
            // No fragment — parse the entire fetched document.
            None | Some("") => &remote_value,
            // JSON Pointer fragment.
            Some(frag) if frag.starts_with('/') => remote_value.pointer(frag)?,
            // Named anchor in the remote document.
            Some(name) => find_anchor_in_value(name, &remote_value)?,
        };
        SchemaParser::new(&remote_value, None).parse(target, Some(&normalized), depth + 1)
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
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[expect(clippy::cast_possible_truncation, reason = "test code")]
mod tests {
    use rstest::rstest;

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
    // parse_schema
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_parse_minimal_object_schema() {
        let v = json!({"type": "object"});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(schema_type_str(&s), Some("object"));
    }

    #[test]
    fn should_parse_schema_with_properties() {
        let v = json!({"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "integer"}}});
        let s = parse_schema(&v).expect("should parse").root;
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

    #[test]
    fn should_parse_required_fields() {
        let v = json!({"type": "object", "required": ["name", "age"]});
        let s = parse_schema(&v).expect("should parse").root;
        let req = s.required.as_ref().expect("should have required");
        assert!(req.contains(&"name".to_string()));
        assert!(req.contains(&"age".to_string()));
    }

    #[test]
    fn should_parse_enum_values() {
        let v = json!({"type": "string", "enum": ["alpha", "beta", "gamma"]});
        let s = parse_schema(&v).expect("should parse").root;
        let enums = s.enum_values.as_ref().expect("should have enum");
        assert_eq!(enums.len(), 3);
        assert!(enums.contains(&json!("alpha")));
        assert!(enums.contains(&json!("beta")));
        assert!(enums.contains(&json!("gamma")));
    }

    #[test]
    fn should_parse_description() {
        let v = json!({"type": "string", "description": "A human-readable name"});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(s.description.as_deref(), Some("A human-readable name"));
    }

    #[test]
    fn should_parse_default_value() {
        let v = json!({"type": "integer", "default": 42});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(s.default, Some(json!(42)));
    }

    #[test]
    fn should_parse_array_schema_with_items() {
        let v = json!({"type": "array", "items": {"type": "string"}});
        let s = parse_schema(&v).expect("should parse").root;
        let items = s.items.as_ref().expect("should have items");
        assert_eq!(schema_type_str(items), Some("string"));
    }

    #[test]
    fn should_parse_additional_properties_false() {
        let v = json!({"type": "object", "additionalProperties": false});
        let s = parse_schema(&v).expect("should parse").root;
        assert!(matches!(
            s.additional_properties,
            Some(AdditionalProperties::Denied)
        ));
    }

    #[test]
    fn should_parse_additional_properties_as_schema() {
        let v = json!({"type": "object", "additionalProperties": {"type": "string"}});
        let s = parse_schema(&v).expect("should parse").root;
        assert!(matches!(
            s.additional_properties,
            Some(AdditionalProperties::Schema(_))
        ));
    }

    #[test]
    fn should_parse_min_properties_and_max_properties() {
        let v = json!({"type": "object", "minProperties": 1, "maxProperties": 5});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(s.min_properties, Some(1));
        assert_eq!(s.max_properties, Some(5));
    }

    // Test P-1
    #[test]
    fn should_parse_additional_items_false() {
        let v = json!({"items": [{"type": "string"}], "additionalItems": false});
        let s = parse_schema(&v).expect("should parse").root;
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
        let s = parse_schema(&v).expect("should parse").root;
        assert!(matches!(
            s.additional_items,
            Some(AdditionalProperties::Schema(_))
        ));
    }

    // Test P-3
    #[test]
    fn should_not_parse_additional_items_when_prefix_items_set_from_prefix_items_key() {
        let v = json!({"prefixItems": [{"type": "string"}], "additionalItems": false});
        let s = parse_schema(&v).expect("should parse").root;
        assert!(s.additional_items.is_none());
    }

    // Test P-4
    #[test]
    fn should_not_parse_additional_items_when_no_array_items() {
        let v = json!({"type": "array", "additionalItems": false});
        let s = parse_schema(&v).expect("should parse").root;
        assert!(s.additional_items.is_none());
    }

    #[test]
    fn should_parse_all_of() {
        let v = json!({"allOf": [{"type": "object"}, {"required": ["name"]}]});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(s.all_of.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn should_parse_any_of() {
        let v = json!({"anyOf": [{"type": "string"}, {"type": "integer"}]});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(s.any_of.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn should_parse_one_of() {
        let v = json!({"oneOf": [{"type": "string"}, {"type": "null"}]});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(s.one_of.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn should_return_none_for_null_input() {
        assert!(parse_schema(&Value::Null).is_none());
    }

    #[test]
    fn should_return_none_for_non_object_json() {
        assert!(parse_schema(&Value::String("not a schema".into())).is_none());
    }

    #[test]
    fn should_parse_empty_object_as_permissive_schema() {
        let v = json!({});
        let s = parse_schema(&v).expect("should parse").root;
        assert!(s.schema_type.is_none());
        assert!(s.properties.is_none());
        assert!(s.required.is_none());
    }

    // boolean true → permissive schema
    #[test]
    fn should_parse_boolean_true_schema() {
        let s = parse_schema(&Value::Bool(true))
            .expect("should return Some for true")
            .root;
        assert!(s.schema_type.is_none());
    }

    // boolean false → None
    #[test]
    fn should_parse_boolean_false_schema() {
        assert!(parse_schema(&Value::Bool(false)).is_none());
    }

    // deprecated: true parses to Some(true)
    #[test]
    fn should_parse_deprecated_true() {
        let v = json!({"type": "string", "deprecated": true});
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(s.deprecated, Some(true));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // $ref resolution
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_resolve_simple_local_ref() {
        let v = json!({
            "$ref": "#/definitions/MyType",
            "definitions": {"MyType": {"type": "string"}}
        });
        let s = parse_schema(&v).expect("should resolve").root;
        assert_eq!(schema_type_str(&s), Some("string"));
    }

    #[test]
    fn should_resolve_ref_into_defs_container() {
        let v = json!({
            "$ref": "#/$defs/MyType",
            "$defs": {"MyType": {"type": "string"}}
        });
        let s = parse_schema(&v).expect("should resolve").root;
        assert_eq!(schema_type_str(&s), Some("string"));
        assert!(s.ref_path.is_none());
    }

    #[test]
    fn should_return_none_for_missing_ref_target() {
        let v = json!({"$ref": "#/definitions/Missing"});
        // Should not panic; result is None or a schema without type
        let _ = parse_schema(&v);
    }

    #[test]
    fn should_handle_nested_ref_resolution() {
        let v = json!({
            "type": "object",
            "properties": {
                "foo": {"$ref": "#/definitions/Bar"}
            },
            "definitions": {"Bar": {"type": "integer"}}
        });
        let s = parse_schema(&v).expect("should parse").root;
        let props = s.properties.as_ref().expect("should have properties");
        let foo = props.get("foo").expect("should have foo");
        assert_eq!(schema_type_str(foo), Some("integer"));
    }

    // circular ref must terminate
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

    #[test]
    fn should_return_none_on_cache_miss() {
        let cache = SchemaCache::new();
        assert!(cache.get("https://example.com/schema.json").is_none());
    }

    #[test]
    fn parsed_schema_new_wraps_the_root_schema() {
        let parsed = ParsedSchema::new(JsonSchema {
            description: Some("x".to_string()),
            ..JsonSchema::default()
        });

        assert_eq!(parsed.root.description.as_deref(), Some("x"));
    }

    #[test]
    fn cache_get_returns_the_inserted_arc_without_copying() {
        let url = "https://example.com/schema.json";
        let mut cache = SchemaCache::new();
        let schema = Arc::new(ParsedSchema::new(JsonSchema::default()));
        cache.insert(url.to_string(), Value::Null, Arc::clone(&schema));

        let first = cache.get(url).expect("should be cached");
        let second = cache.get(url).expect("should be cached");

        assert!(Arc::ptr_eq(first, second));
        assert!(Arc::ptr_eq(first, &schema));
        assert_eq!(Arc::strong_count(&schema), 2, "local handle plus the cache");
    }

    #[test]
    fn should_return_cached_schema_on_cache_hit() {
        let mut cache = SchemaCache::new();
        let schema = ParsedSchema::new(JsonSchema {
            description: Some("test".to_string()),
            ..JsonSchema::default()
        });
        cache.insert(
            "https://example.com/schema.json".to_string(),
            Value::Null,
            Arc::new(schema),
        );

        let result = cache
            .get("https://example.com/schema.json")
            .expect("should be cached");
        assert_eq!(result.root.description.as_deref(), Some("test"));
    }

    // first write wins
    #[test]
    fn should_not_overwrite_existing_cache_entry() {
        let mut cache = SchemaCache::new();
        let schema_a = ParsedSchema::new(JsonSchema {
            description: Some("first".to_string()),
            ..JsonSchema::default()
        });
        let schema_b = ParsedSchema::new(JsonSchema {
            description: Some("second".to_string()),
            ..JsonSchema::default()
        });

        cache.insert(
            "https://example.com/schema.json".to_string(),
            Value::Null,
            Arc::new(schema_a),
        );
        cache.insert(
            "https://example.com/schema.json".to_string(),
            Value::Null,
            Arc::new(schema_b),
        );

        let result = cache
            .get("https://example.com/schema.json")
            .expect("should be cached");
        assert_eq!(result.root.description.as_deref(), Some("first"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security tests (from Security Engineer assessment)
    // ══════════════════════════════════════════════════════════════════════════

    // Sec-5: URL exceeding 2048 chars is rejected (uses runtime format!, cannot be rstest literal)
    #[test]
    fn should_reject_url_exceeding_max_length() {
        let long_url = format!("https://example.com/{}", "a".repeat(2050));
        let result = validate_and_normalize_url(&long_url);
        assert!(result.is_err());
    }

    // URL exceeding max length rejected (uses runtime format!, cannot be rstest literal)
    #[test]
    fn should_reject_url_exceeding_max_length_52() {
        let long_url = format!("https://example.com/{}", "a".repeat(2048));
        assert!(validate_and_normalize_url(&long_url).is_err());
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

    // ftp:// scheme — kept standalone: extra assertion on error message text
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

    // All validate_and_normalize_url() calls that must return Err, differing only
    // in the URL string.  Duplicates from the earlier Sec-* and Test-4X groups
    // are merged here; the duplicate entries (Sec-1/Test-47, Sec-2/Test-48,
    // Sec-3/Test-51) appear once each.
    #[rstest]
    #[case::localhost("http://localhost/schema.json")]
    #[case::link_local_aws_metadata("http://169.254.169.254/latest/meta-data/")]
    #[case::loopback_ip("http://127.0.0.1/schema.json")]
    #[case::ipv6_loopback("http://[::1]/schema.json")]
    #[case::private_ipv4_10_range("http://10.0.0.1/schema.json")]
    #[case::private_ipv4_192_168_range("http://192.168.1.1/schema.json")]
    #[case::private_ipv4_172_16_range("http://172.16.0.1/schema.json")]
    #[case::unspecified_ipv4_0_0_0_0("http://0.0.0.0/schema.json")]
    #[case::ipv6_unspecified_double_colon("http://[::]/schema.json")]
    #[case::ipv6_link_local_fe80("http://[fe80::1]/schema.json")]
    #[case::unparseable_url("not a url at all")]
    #[case::ipv6_ula_fd00("http://[fd00::1]/schema.json")]
    #[case::ipv6_ula_fc00("http://[fc00::1]/schema.json")]
    #[case::ipv4_mapped_private("http://[::ffff:192.168.1.1]/schema.json")]
    #[case::ipv4_mapped_loopback("http://[::ffff:127.0.0.1]/schema.json")]
    fn validate_and_normalize_url_rejects(#[case] url: &str) {
        assert!(
            validate_and_normalize_url(url).is_err(),
            "must reject: {url}"
        );
    }

    // All validate_and_normalize_url() calls that must return Ok.
    #[rstest]
    #[case::valid_https_url("https://schemastore.azurewebsites.net/schemas/json/package.json")]
    #[case::valid_http_url("http://json.schemastore.org/package")]
    #[case::ipv4_mapped_public("http://[::ffff:8.8.8.8]/schema.json")]
    fn validate_and_normalize_url_accepts(#[case] url: &str) {
        assert!(
            validate_and_normalize_url(url).is_ok(),
            "must accept: {url}"
        );
    }

    // schema with 60-level nesting is rejected or truncated (does not hang)
    #[test]
    fn should_reject_schema_exceeding_nesting_depth() {
        let mut v = json!({"type": "string"});
        for _ in 0..60 {
            v = json!({"type": "object", "properties": {"child": v}});
        }
        // Must terminate; truncated result or None is acceptable
        let _ = parse_schema(&v);
    }

    // schema with 10-level nesting is accepted
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

    // two-node circular $ref does not hang
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

    // trailing-slash path variants produce distinct cache keys.
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

    // cache key host case normalization (explicit cache test)
    #[test]
    fn should_normalize_cache_key_host_case() {
        let key_upper = validate_and_normalize_url("https://EXAMPLE.COM/schema").expect("valid");
        let key_lower = validate_and_normalize_url("https://example.com/schema").expect("valid");
        assert_eq!(
            key_upper, key_lower,
            "host should be normalized to lowercase in cache key"
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

    // ── parse_type edge cases ─────────────────────────────────────────────────

    #[test]
    fn parse_type_returns_none_for_non_string_non_array() {
        // type: 42 (number) — should be ignored
        let v = json!({"type": 42});
        let s = parse_schema(&v)
            .expect("should parse as object schema")
            .root;
        assert!(
            s.schema_type.is_none(),
            "non-string/non-array type should yield None"
        );
    }

    #[test]
    fn parse_type_returns_none_for_empty_type_array() {
        // type: [] — empty array has no types, should yield None
        let v = json!({"type": []});
        let s = parse_schema(&v).expect("should parse").root;
        assert!(
            s.schema_type.is_none(),
            "empty type array should yield None schema_type"
        );
    }

    #[test]
    fn parse_type_filters_non_string_items_from_array() {
        // type: [42, "string"] — non-string items filtered out; "string" survives
        let v = json!({"type": [42, "string"]});
        let s = parse_schema(&v).expect("should parse").root;
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
        let s = parse_schema(&v).expect("should parse").root;
        assert_eq!(schema_type_str(&s), Some("object"));
    }

    #[test]
    fn ref_without_hash_prefix_yields_ref_path_only() {
        // $ref without '#' prefix cannot be resolved locally — returns schema with ref_path set
        let v = json!({"$ref": "http://example.com/other-schema.json"});
        let result = parse_schema(&v);
        // resolve_ref returns None for non-# refs; SchemaParser::parse returns Some(schema)
        // with only ref_path set
        if let Some(s) = result {
            assert_eq!(
                s.root.ref_path.as_deref(),
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
        let s = parse_schema(&v).expect("should parse").root;
        assert!(s.all_of.is_none(), "empty allOf should yield None");
    }

    #[test]
    fn all_of_with_non_object_entries_filtered_out_yields_none() {
        // allOf: ["string"] — non-object entries filtered by SchemaParser::parse
        let v = json!({"allOf": ["not a schema"]});
        let s = parse_schema(&v).expect("should parse").root;
        assert!(
            s.all_of.is_none(),
            "allOf with only invalid entries should yield None"
        );
    }

    // ── additionalProperties: true ────────────────────────────────────────────

    #[test]
    fn additional_properties_true_parsed_as_permissive_schema() {
        // additionalProperties: true — boolean true is a permissive schema
        let v = json!({"type": "object", "additionalProperties": true});
        let s = parse_schema(&v).expect("should parse").root;
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
        let s = parse_schema(&v).expect("should parse").root;
        let req = s.required.as_ref().expect("should have required");
        assert_eq!(req.len(), 1, "only string 'name' should survive filtering");
        assert!(req.contains(&"name".to_string()));
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

    // SS-1: catalog with one YAML entry is parsed and kept — also checks field values
    // Different assertion shape (checks url and file_match fields) — left standalone.
    #[test]
    fn should_parse_catalog_entry_with_yaml_pattern() {
        let v = make_catalog_json(&[("https://example.com/schema.json", &["*.yaml"])]);
        let catalog = parse_schemastore_catalog(&v).expect("should parse");
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].url, "https://example.com/schema.json");
        assert_eq!(catalog.entries[0].file_match, vec!["*.yaml"]);
    }

    // SS-3: entry with mixed YAML and JSON patterns is kept; non-YAML patterns are discarded
    // Different assertion shape (checks file_match field value) — left standalone.
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

    #[rstest]
    #[case::json_only_entry_filtered(
        make_catalog_json(&[("https://example.com/schema.json", &["*.json"])]),
        0
    )]
    #[case::yml_extension_kept(
        make_catalog_json(&[("https://example.com/schema.json", &["*.yml"])]),
        1
    )]
    #[case::empty_schemas_array(json!({ "schemas": [] }), 0)]
    #[case::empty_url_skipped(
        json!({"schemas": [{"name": "Empty URL", "url": "", "fileMatch": ["*.yaml"]}]}),
        0
    )]
    #[case::missing_file_match_skipped(
        json!({"schemas": [{"name": "No FileMatch", "url": "https://example.com/schema.json"}]}),
        0
    )]
    #[case::two_yaml_kept_one_json_filtered(
        make_catalog_json(&[
            ("https://example.com/workflow.json", &["**/.github/workflows/*.yml"]),
            ("https://example.com/compose.json", &["docker-compose.yaml"]),
            ("https://example.com/package.json", &["package.json"]),
        ]),
        2
    )]
    fn parse_schemastore_catalog_entry_count(#[case] input: Value, #[case] expected_len: usize) {
        let catalog = parse_schemastore_catalog(&input).expect("should parse");
        assert_eq!(catalog.entries.len(), expected_len);
    }

    #[rstest]
    #[case::non_object_input(json!(["not", "an", "object"]))]
    #[case::missing_schemas_key(json!({ "other": "data" }))]
    fn parse_schemastore_catalog_returns_none(#[case] input: Value) {
        assert!(parse_schemastore_catalog(&input).is_none());
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
        assert_eq!(schema.anchor, Some("myanchor".to_string()));
    }

    // Anchor-8: $dynamicAnchor field stored on parsed schema
    #[test]
    fn dynamic_anchor_field_stored_on_schema() {
        let value = json!({ "$dynamicAnchor": "myloop", "type": "array" });
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
        assert_eq!(
            schema.id,
            Some("https://example.com/schema.json".to_string())
        );
    }

    // Id-2: relative $id is resolved against supplied base URI
    #[test]
    fn relative_dollar_id_is_resolved_against_base_uri() {
        // Use SchemaParser::parse directly to supply a base URI
        let value = json!({ "$id": "sub.json", "type": "object" });
        let schema = parse_with(&value, Some("https://example.com/root.json"), None).unwrap();
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        let schema = parse_schema(&value).unwrap().root;
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
        // SchemaParser::parse + ParseContext attempts to fetch; SSRF guard blocks it.
        // The $ref falls back to ref_path-only schema (no remote traversal).
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = parse_with(&value, None, Some(&mut ctx)).unwrap();
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
        let schema = parse_with(
            &value,
            Some("http://127.0.0.1/"),
            Some(&mut ParseContext::new(&mut cache, None)),
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
            Arc::new(schema_a),
        );
        cache.insert(
            "https://example.com/b.json".to_string(),
            json_b,
            Arc::new(schema_b),
        );

        // Resolving A should terminate — the cycle is broken when B tries to
        // re-visit A (already in visited set).
        let mut ctx = ParseContext::new(&mut cache, None);
        let result = resolve_with("https://example.com/a.json", &json_a, None, Some(&mut ctx));
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
            Arc::new(ParsedSchema::new(JsonSchema {
                description: Some("legitimate".to_string()),
                ..JsonSchema::default()
            })),
        );

        // A malicious schema tries to claim the same URL via $id.
        cache.insert(
            "https://json-schema.org/draft/2020-12/schema".to_string(),
            Value::Null,
            Arc::new(ParsedSchema::new(JsonSchema {
                description: Some("malicious".to_string()),
                ..JsonSchema::default()
            })),
        );

        let cached = cache
            .get("https://json-schema.org/draft/2020-12/schema")
            .unwrap();
        assert_eq!(
            cached.root.description.as_deref(),
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
        let schema = parse_with(&value, None, Some(&mut ctx)).unwrap();
        // SSRF guard blocks — ref_path preserved, nothing cached.
        assert_eq!(schema.ref_path.as_deref(), Some("file:///etc/passwd"));
        assert!(cache.get("file:///etc/passwd").is_none());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Remote $ref resolution — functional tests (with tiny_http server)
    // ══════════════════════════════════════════════════════════════════════════

    // Remote-1: absolute $ref URL resolves from the cache.
    //
    // The URL is https://example.com/... passed through the cache
    // pre-population path to verify the full resolve_ref → cache flow without
    // hitting the loopback SSRF guard.
    #[test]
    fn remote_ref_absolute_url_resolves_to_fetched_schema() {
        // Pre-populate cache with a valid HTTPS URL (no actual network call).
        let ref_url = "https://example.com/other.json";
        let remote_value: Value = json!({ "type": "string", "description": "remote schema" });
        let remote_schema = parse_schema(&remote_value).unwrap();

        let mut cache = SchemaCache::new();
        cache.insert(ref_url.to_string(), remote_value, Arc::new(remote_schema));

        let root = json!({});
        let mut ctx = ParseContext::new(&mut cache, None);
        let resolved = resolve_with(ref_url, &root, None, Some(&mut ctx));

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
        cache.insert(url.clone(), remote_value, Arc::new(remote_schema));

        let root = json!({});
        let ref_str = format!("{url}#/definitions/Address");
        let mut ctx = ParseContext::new(&mut cache, None);
        let resolved = resolve_with(&ref_str, &root, None, Some(&mut ctx));

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
        cache.insert(ref_url.to_string(), remote_value, Arc::new(remote_schema));

        let root = json!({
            "type": "object",
            "properties": {
                "address": { "$ref": ref_url }
            }
        });

        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = parse_with(&root, None, Some(&mut ctx));

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

    // ══════════════════════════════════════════════════════════════════════════
    // fetch_schema_raw + ParseContext integration (Finding 1)
    // ══════════════════════════════════════════════════════════════════════════

    // FetchCtx-1: fetch_schema_raw with a ParseContext resolves remote $refs
    // within the fetched schema.
    //
    // fetch_schema_raw calls validate_and_normalize_url which blocks loopback
    // addresses, so this test cannot use a tiny_http server for the top-level
    // fetch.  Instead it exercises the exact code path that fetch_schema_raw
    // takes when ctx=Some: `SchemaParser::new(&value,
    // Some(ctx)).parse(&value, Some(url), 0)`.  The $ref target is pre-populated in the ParseContext
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
        cache.insert(ref_url.to_string(), remote_value, Arc::new(remote_schema));

        // Call exactly the code path that fetch_schema_raw uses when ctx=Some.
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = parse_with(&body, Some(top_level_url), Some(&mut ctx))
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
        let schema = parse_schema(&body).expect("should parse").root;
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
    // fetch_json — shared request / bounded read / parse path
    // ══════════════════════════════════════════════════════════════════════════

    /// A loopback HTTP server that answers every request with one canned
    /// response and records the request paths it saw.
    ///
    /// `fetch_json` is called against it directly: the SSRF guard lives in
    /// `fetch_schema_raw`, which refuses loopback targets.
    struct CannedServer {
        url: String,
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl CannedServer {
        /// `chunked` sends the body without a `Content-Length`.
        fn start(status: u16, headers: &[(&str, &str)], body: Vec<u8>, chunked: bool) -> Self {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let addr = server.server_addr().to_ip().unwrap();
            let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let headers: Vec<tiny_http::Header> = headers
                .iter()
                .map(|(k, v)| tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap())
                .collect();

            let (thread_seen, thread_stop) = (seen.clone(), stop.clone());
            std::thread::spawn(move || {
                while !thread_stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let Ok(Some(req)) = server.recv_timeout(std::time::Duration::from_millis(50))
                    else {
                        continue;
                    };
                    thread_seen.lock().unwrap().push(req.url().to_string());
                    let len = (!chunked).then_some(body.len());
                    let response = tiny_http::Response::new(
                        tiny_http::StatusCode(status),
                        headers.clone(),
                        std::io::Cursor::new(body.clone()),
                        len,
                        None,
                    );
                    let _ = req.respond(response);
                }
            });

            Self {
                url: format!("http://{addr}/schema.json"),
                seen,
                stop,
            }
        }

        fn seen(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }

    impl Drop for CannedServer {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Valid JSON of exactly `len` bytes.
    fn json_body_of_len(len: usize) -> Vec<u8> {
        let mut body = br#"{"k":""#.to_vec();
        body.resize(len - 2, b'a');
        body.extend_from_slice(br#""}"#);
        assert_eq!(body.len(), len);
        body
    }

    #[rstest]
    #[case::text_plain(&[("Content-Type", "text/plain")])]
    #[case::text_plain_charset_utf8(&[("Content-Type", "text/plain; charset=utf-8")])]
    #[case::application_octet_stream(&[("Content-Type", "application/octet-stream")])]
    #[case::text_html(&[("Content-Type", "text/html")])]
    #[case::application_json(&[("Content-Type", "application/json")])]
    #[case::no_content_type_header(&[])]
    fn fetch_json_accepts_valid_json_regardless_of_content_type(#[case] headers: &[(&str, &str)]) {
        let server = CannedServer::start(200, headers, br#"{"type":"object"}"#.to_vec(), false);

        let value = fetch_json(&build_agent(None), &server.url).expect("valid JSON must load");

        assert_eq!(value, json!({"type": "object"}));
    }

    #[rstest]
    #[case::html_body("text/html", b"<html>CANARY-7f3a</html>".to_vec())]
    #[case::json_content_type_html_body("application/json", b"<html>CANARY-7f3a</html>".to_vec())]
    #[case::empty_body("text/plain", Vec::new())]
    fn fetch_json_returns_parse_failed_for_non_json_body(
        #[case] content_type: &str,
        #[case] body: Vec<u8>,
    ) {
        let server = CannedServer::start(200, &[("Content-Type", content_type)], body, false);

        let result = fetch_json(&build_agent(None), &server.url);

        let Err(SchemaError::ParseFailed(msg)) = result else {
            panic!("expected ParseFailed, got {result:?}");
        };
        assert!(
            !msg.contains("CANARY-7f3a"),
            "body leaked into error: {msg}"
        );
    }

    #[rstest]
    #[case::exactly_max_bytes(MAX_SCHEMA_BYTES as usize, false, true)]
    #[case::one_byte_over(MAX_SCHEMA_BYTES as usize + 1, false, false)]
    #[case::one_byte_over_chunked(MAX_SCHEMA_BYTES as usize + 1, true, false)]
    fn fetch_json_enforces_size_limit(
        #[case] len: usize,
        #[case] chunked: bool,
        #[case] accepted: bool,
    ) {
        let server = CannedServer::start(200, &[], json_body_of_len(len), chunked);

        let result = fetch_json(&build_agent(None), &server.url);

        if accepted {
            let value = result.expect("body of exactly MAX_SCHEMA_BYTES must load");
            assert_eq!(value["k"].as_str().map(str::len), Some(len - 8));
        } else {
            assert!(
                matches!(result, Err(SchemaError::ResponseTooLarge)),
                "got {result:?}"
            );
        }
    }

    /// Gzip stream that inflates to more than `MAX_SCHEMA_BYTES` from a few
    /// kilobytes: one literal followed by repeated 258-byte matches at distance
    /// 1, all in a single fixed-Huffman deflate block.
    fn gzip_bomb() -> Vec<u8> {
        struct Bits {
            out: Vec<u8>,
            acc: u32,
            used: u32,
        }
        impl Bits {
            fn put(&mut self, value: u32, count: u32) {
                for i in 0..count {
                    self.acc |= ((value >> i) & 1) << self.used;
                    self.used += 1;
                    if self.used == 8 {
                        self.out.push(self.acc as u8);
                        self.acc = 0;
                        self.used = 0;
                    }
                }
            }
            // Huffman codes are packed most-significant bit first.
            fn put_code(&mut self, code: u32, count: u32) {
                for i in (0..count).rev() {
                    self.put((code >> i) & 1, 1);
                }
            }
        }

        let mut bits = Bits {
            out: vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff],
            acc: 0,
            used: 0,
        };
        bits.put(1, 1); // BFINAL
        bits.put(1, 2); // BTYPE = fixed Huffman
        bits.put_code(0x30 + u32::from(b'x'), 8); // literal 'x'
        for _ in 0..(MAX_SCHEMA_BYTES / 258 + 2) {
            bits.put_code(0xC5, 8); // length symbol 285 = 258
            bits.put_code(0, 5); // distance symbol 0 = 1
        }
        bits.put_code(0, 7); // end of block
        bits.put(0, 7); // flush the partial byte
        // CRC-32 and ISIZE are never read: the size cap stops the read first.
        bits.out.extend_from_slice(&[0; 8]);
        bits.out
    }

    #[test]
    fn fetch_json_caps_decompressed_size() {
        let bomb = gzip_bomb();
        assert!(bomb.len() < 64 * 1024, "bomb must be small on the wire");
        let server = CannedServer::start(
            200,
            &[
                ("Content-Type", "application/json"),
                ("Content-Encoding", "gzip"),
            ],
            bomb,
            false,
        );

        let result = fetch_json(&build_agent(None), &server.url);

        assert!(
            matches!(result, Err(SchemaError::ResponseTooLarge)),
            "got {result:?}"
        );
    }

    // max_redirects(0): ureq hands back the 3xx response; fetch_json must fail
    // on it without following the Location or using its body.
    #[rstest]
    #[case::moved_permanently(301)]
    #[case::found(302)]
    #[case::temporary_redirect(307)]
    #[case::permanent_redirect(308)]
    fn fetch_json_rejects_redirect_without_following(#[case] status: u16) {
        let server = CannedServer::start(
            status,
            &[
                ("Location", "/redirected-CANARY"),
                ("Content-Type", "application/json"),
            ],
            br#"{"type":"object"}"#.to_vec(),
            false,
        );

        let result = fetch_json(&build_agent(None), &server.url);

        let Err(SchemaError::FetchFailed(msg)) = result else {
            panic!("expected FetchFailed, got {result:?}");
        };
        assert!(msg.contains(&status.to_string()), "got: {msg}");
        assert!(msg.contains("redirects are not followed"), "got: {msg}");
        assert!(!msg.contains("CANARY"), "Location leaked into error: {msg}");
        assert_eq!(server.seen(), ["/schema.json"], "redirect was followed");
    }

    #[rstest]
    #[case::not_found(404)]
    #[case::internal_server_error(500)]
    fn fetch_json_returns_fetch_failed_for_http_error_status(#[case] status: u16) {
        let server = CannedServer::start(
            status,
            &[("Content-Type", "application/json")],
            br#"{"type":"object","note":"CANARY-7f3a"}"#.to_vec(),
            false,
        );

        let result = fetch_json(&build_agent(None), &server.url);

        let Err(SchemaError::FetchFailed(msg)) = result else {
            panic!("expected FetchFailed, got {result:?}");
        };
        assert!(msg.contains(&status.to_string()), "got: {msg}");
        assert!(
            !msg.contains("CANARY-7f3a"),
            "body leaked into error: {msg}"
        );
    }

    #[test]
    fn fetch_json_returns_fetch_failed_when_connection_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/schema.json", listener.local_addr().unwrap());
        drop(listener);

        let result = fetch_json(&build_agent(None), &url);

        assert!(
            matches!(result, Err(SchemaError::FetchFailed(_))),
            "got {result:?}"
        );
    }

    // The SSRF guard runs before any network call: a listener on the target
    // port must never see a connection.
    #[rstest]
    #[case::loopback_ipv4("127.0.0.1")]
    #[case::localhost("localhost")]
    #[case::loopback_ipv6("[::1]")]
    #[case::link_local("169.254.169.254")]
    #[case::private("10.0.0.1")]
    fn fetch_schema_raw_rejects_blocked_targets_before_any_network_call(#[case] host: &str) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://{host}:{port}/schema.json");

        let result = fetch_schema_raw(&url, None, None);

        assert!(
            matches!(result, Err(SchemaError::UrlNotPermitted(_))),
            "got {result:?}"
        );
        assert_eq!(
            listener.accept().map(|_| ()).map_err(|e| e.kind()),
            Err(std::io::ErrorKind::WouldBlock),
            "guard must reject before connecting"
        );
    }

    #[test]
    fn fetch_schema_raw_rejects_file_scheme() {
        let result = fetch_schema_raw("file:///etc/passwd", None, None);

        assert!(
            matches!(result, Err(SchemaError::UrlNotPermitted(_))),
            "got {result:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Parser internals — pins for the per-parse state (root, base URI, remote
    // context, depth)
    // ══════════════════════════════════════════════════════════════════════════

    fn parse_with(
        value: &Value,
        base_uri: Option<&str>,
        ctx: Option<&mut ParseContext<'_>>,
    ) -> Option<JsonSchema> {
        SchemaParser::new(value, ctx).parse(value, base_uri, 0)
    }

    fn resolve_with(
        ref_str: &str,
        root: &Value,
        base_uri: Option<&str>,
        ctx: Option<&mut ParseContext<'_>>,
    ) -> Option<JsonSchema> {
        SchemaParser::new(root, ctx).resolve_ref(ref_str, base_uri, 0)
    }

    fn cache_with(entries: &[(&str, Value)]) -> SchemaCache {
        let mut cache = SchemaCache::new();
        for (url, value) in entries {
            let schema = parse_schema(value).expect("fixture parses");
            cache.insert((*url).to_string(), value.clone(), Arc::new(schema));
        }
        cache
    }

    fn debug_of(schema: &JsonSchema) -> String {
        format!("{schema:?}")
    }

    /// Every keyword that holds child schemas, so a keyword that stops
    /// passing the parse state on is caught.
    const SUBSCHEMA_KEYWORDS: &[&str] = &[
        "properties",
        "patternProperties",
        "prefixItems",
        "items",
        "tuple-items",
        "additionalItems",
        "additionalProperties",
        "unevaluatedProperties",
        "unevaluatedItems",
        "contains",
        "contentSchema",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
        "allOf",
        "anyOf",
        "oneOf",
        "dependencies",
        "dependentSchemas",
    ];

    /// Put `inner` under `keyword`; `tuple-items` is the array form of `items`.
    fn wrap(keyword: &str, inner: &Value) -> Value {
        match keyword {
            "properties" | "patternProperties" | "dependencies" | "dependentSchemas" => {
                json!({ keyword: { "k": inner } })
            }
            "prefixItems" | "allOf" | "anyOf" | "oneOf" => json!({ keyword: [inner] }),
            "tuple-items" => json!({ "items": [inner] }),
            "additionalItems" => json!({ "items": [], "additionalItems": inner }),
            _ => json!({ keyword: inner }),
        }
    }

    fn nest(keyword: &str, levels: usize, leaf: Value) -> Value {
        (0..levels).fold(leaf, |inner, _| wrap(keyword, &inner))
    }

    #[test]
    fn every_subschema_keyword_parses_to_the_depth_limit() {
        for keyword in SUBSCHEMA_KEYWORDS {
            let value = nest(keyword, MAX_REF_DEPTH, json!({ "description": "LEAF" }));
            let schema = parse_schema(&value).expect("parses").root;
            assert!(debug_of(&schema).contains("LEAF"), "keyword {keyword}");
        }
    }

    #[test]
    fn every_subschema_keyword_truncates_beyond_the_depth_limit() {
        for keyword in SUBSCHEMA_KEYWORDS {
            let value = nest(keyword, MAX_REF_DEPTH + 1, json!({ "description": "LEAF" }));
            let schema = parse_schema(&value).expect("parses").root;
            assert!(!debug_of(&schema).contains("LEAF"), "keyword {keyword}");
        }
    }

    #[test]
    fn every_subschema_keyword_resolves_remote_ref_via_ctx() {
        for keyword in SUBSCHEMA_KEYWORDS {
            let mut cache = cache_with(&[(
                "https://example.com/child.json",
                json!({ "description": "REMOTE" }),
            )]);
            let mut ctx = ParseContext::new(&mut cache, None);
            let value = wrap(
                keyword,
                &json!({ "$ref": "https://example.com/child.json" }),
            );
            let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
            assert!(debug_of(&schema).contains("REMOTE"), "keyword {keyword}");
        }
    }

    #[test]
    fn every_subschema_keyword_propagates_base_uri_to_children() {
        for keyword in SUBSCHEMA_KEYWORDS {
            let mut value = wrap(keyword, &json!({ "$id": "leaf.json" }));
            value["$id"] = json!("https://example.com/root.json");
            let schema = parse_schema(&value).expect("parses").root;
            assert!(
                debug_of(&schema).contains("https://example.com/leaf.json"),
                "keyword {keyword}"
            );
        }
    }

    #[rstest]
    #[case::missing_pointer("#/x/missing")]
    #[case::missing_anchor("#nope")]
    fn unresolved_ref_returns_ref_path_only_ignoring_siblings(#[case] ref_str: &str) {
        let value = json!({ "$ref": ref_str, "type": "string" });
        let schema = parse_schema(&value).expect("parses").root;
        assert_eq!(schema.ref_path.as_deref(), Some(ref_str));
        assert!(schema.schema_type.is_none());
    }

    #[test]
    fn resolved_ref_ignores_sibling_keywords() {
        let value = json!({
            "$ref": "#/x-targets/T",
            "description": "SIBLING",
            "x-targets": { "T": { "type": "string" } }
        });
        let schema = parse_schema(&value).expect("parses").root;
        assert_eq!(schema_type_str(&schema), Some("string"));
        assert!(schema.description.is_none());
    }

    #[test]
    fn unresolved_dynamic_ref_falls_through_to_sibling_fields() {
        let value = json!({ "$dynamicRef": "#missing", "type": "string" });
        let schema = parse_schema(&value).expect("parses").root;
        assert_eq!(schema_type_str(&schema), Some("string"));
        assert!(schema.ref_path.is_none());
    }

    #[test]
    fn local_ref_target_is_parsed_without_ctx() {
        let url = "https://example.com/addr.json";
        let mut cache = cache_with(&[(url, json!({ "description": "REMOTE" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "x-targets": { "T": { "$ref": url } },
            "properties": {
                "a": { "$ref": "#/x-targets/T" },
                "b": { "$ref": url }
            }
        });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let props = schema.properties.as_ref().expect("properties");
        assert_eq!(
            props["b"].description.as_deref(),
            Some("REMOTE"),
            "a direct remote ref resolves with ctx"
        );
        assert_eq!(props["a"].ref_path.as_deref(), Some(url));
        assert!(
            props["a"].description.is_none(),
            "a local target is parsed without ctx"
        );
    }

    // Characterization: a local target is parsed without a base URI, so its
    // relative `$id` stays unresolved.
    #[rstest]
    #[case::pointer("#/x-targets/T")]
    #[case::anchor("#anch")]
    fn local_ref_target_is_parsed_without_base_uri(#[case] ref_str: &str) {
        let value = json!({
            "$id": "https://example.com/root.json",
            "x-targets": { "T": { "$anchor": "anch", "$id": "child.json" } },
            "properties": { "p": { "$ref": ref_str } }
        });
        let schema = parse_schema(&value).expect("parses").root;
        let p = &schema.properties.as_ref().expect("properties")["p"];
        assert_eq!(p.id.as_deref(), Some("child.json"));
    }

    #[rstest]
    #[case::ref_keyword("$ref")]
    #[case::dynamic_ref_keyword("$dynamicRef")]
    fn ref_resolves_against_parent_base_not_own_id(#[case] keyword: &str) {
        let mut cache = cache_with(&[
            (
                "https://example.com/y.json",
                json!({ "description": "PARENT" }),
            ),
            (
                "https://example.com/other/y.json",
                json!({ "description": "OWN" }),
            ),
        ]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "$id": "https://example.com/root.json",
            "properties": {
                "x": { "$id": "https://example.com/other/x.json", keyword: "y.json" }
            }
        });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let x = &schema.properties.as_ref().expect("properties")["x"];
        assert_eq!(x.description.as_deref(), Some("PARENT"));
    }

    #[test]
    fn relative_remote_ref_without_base_stays_unresolved() {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({ "$ref": "other.json" });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert_eq!(schema.ref_path.as_deref(), Some("other.json"));
        assert!(ctx.visited.is_empty());
    }

    #[test]
    fn remote_document_is_its_own_root_for_local_refs() {
        let url = "https://example.com/doc.json";
        let mut cache = cache_with(&[(
            url,
            json!({
                "properties": { "a": { "$ref": "#/definitions/D" } },
                "definitions": { "D": { "description": "D-REMOTE" } }
            }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = resolve_with(url, &json!({}), None, Some(&mut ctx)).expect("resolves");
        let a = &schema.properties.as_ref().expect("properties")["a"];
        assert_eq!(a.description.as_deref(), Some("D-REMOTE"));
    }

    #[test]
    fn remote_document_is_parsed_without_ctx() {
        let a_url = "https://example.com/a.json";
        let b_url = "https://example.com/b.json";
        let mut cache = cache_with(&[
            (a_url, json!({ "properties": { "p": { "$ref": b_url } } })),
            (b_url, json!({ "description": "B" })),
        ]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = resolve_with(a_url, &json!({}), None, Some(&mut ctx)).expect("resolves");
        let p = &schema.properties.as_ref().expect("properties")["p"];
        assert_eq!(p.ref_path.as_deref(), Some(b_url));
        assert!(
            p.description.is_none(),
            "depth-1 rule: b.json stays unresolved"
        );
    }

    #[rstest]
    #[case::whole_document("https://example.com/doc.json")]
    #[case::empty_fragment("https://example.com/doc.json#")]
    #[case::pointer_fragment("https://example.com/doc.json#/definitions/X")]
    #[case::anchor_fragment("https://example.com/doc.json#anchorX")]
    fn remote_document_uses_its_own_url_as_base(#[case] ref_str: &str) {
        let mut cache = cache_with(&[(
            "https://example.com/doc.json",
            json!({
                "$id": "sub.json",
                "definitions": { "X": { "$id": "sub.json", "$anchor": "anchorX" } }
            }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let schema = resolve_with(ref_str, &json!({}), None, Some(&mut ctx)).expect("resolves");
        assert_eq!(schema.id.as_deref(), Some("https://example.com/sub.json"));
    }

    #[rstest]
    #[case::missing_pointer("https://example.com/doc.json#/definitions/Nope")]
    #[case::missing_anchor("https://example.com/doc.json#nope")]
    fn remote_fragment_miss_yields_ref_path_only(#[case] ref_str: &str) {
        let mut cache = cache_with(&[(
            "https://example.com/doc.json",
            json!({ "description": "DOC" }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({ "$ref": ref_str });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert_eq!(schema.ref_path.as_deref(), Some(ref_str));
        assert!(schema.description.is_none());
    }

    // The `$ref` leaf sits at depth `levels`; resolve_ref runs at `levels + 1`
    // and the target is parsed at `levels + 2`.
    #[rstest]
    #[case::local_fits(false, MAX_REF_DEPTH - 2, true)]
    #[case::local_too_deep(false, MAX_REF_DEPTH - 1, false)]
    #[case::remote_fits(true, MAX_REF_DEPTH - 2, true)]
    #[case::remote_too_deep(true, MAX_REF_DEPTH - 1, false)]
    fn ref_target_must_fit_the_depth_budget(
        #[case] remote: bool,
        #[case] levels: usize,
        #[case] resolved: bool,
    ) {
        let url = "https://example.com/target.json";
        let mut cache = cache_with(&[(url, json!({ "description": "TARGET" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let ref_str = if remote { url } else { "#/x-targets/T" };
        let mut value = nest("properties", levels, json!({ "$ref": ref_str }));
        value["x-targets"] = json!({ "T": { "description": "TARGET" } });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert_eq!(debug_of(&schema).contains("TARGET"), resolved);
    }

    // The root `$ref` is at depth 0, so the target is parsed at depth 2.
    #[rstest]
    #[case::local_fits(false, MAX_REF_DEPTH - 2, true)]
    #[case::local_too_deep(false, MAX_REF_DEPTH - 1, false)]
    #[case::remote_fits(true, MAX_REF_DEPTH - 2, true)]
    #[case::remote_too_deep(true, MAX_REF_DEPTH - 1, false)]
    fn ref_target_continues_the_depth_count(
        #[case] remote: bool,
        #[case] levels: usize,
        #[case] leaf_present: bool,
    ) {
        let url = "https://example.com/target.json";
        let target = nest("properties", levels, json!({ "description": "LEAF" }));
        let mut cache = cache_with(&[(url, target.clone())]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = if remote {
            json!({ "$ref": url })
        } else {
            json!({ "$ref": "#/x-targets/T", "x-targets": { "T": target } })
        };
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert_eq!(debug_of(&schema).contains("LEAF"), leaf_present);
    }

    // Each `$ref` hop costs two depth units, so the chain target `R{hops}`
    // is parsed at depth `2 + 2 * hops`.
    #[rstest]
    #[case::fits(15, true)]
    #[case::too_deep(16, false)]
    fn ref_chain_hops_cost_two_depth_units_each(#[case] hops: usize, #[case] resolved: bool) {
        let mut chain = serde_json::Map::new();
        for i in 0..hops {
            chain.insert(
                format!("R{i}"),
                json!({ "$ref": format!("#/x-chain/R{}", i + 1) }),
            );
        }
        chain.insert(format!("R{hops}"), json!({ "description": "LEAF" }));
        let value = json!({ "$ref": "#/x-chain/R0", "x-chain": chain });
        let schema = parse_schema(&value).expect("parses").root;
        assert_eq!(schema.description.as_deref() == Some("LEAF"), resolved);
        assert_eq!(schema.ref_path.is_none(), resolved);
    }

    #[test]
    fn branching_self_reference_terminates() {
        let value = json!({
            "properties": { "p": { "$ref": "#" }, "q": { "$ref": "#" } }
        });
        let schema = parse_schema(&value).expect("parses").root;
        assert!(schema.properties.is_some());
    }

    #[test]
    fn cached_remote_target_bypasses_exhausted_fetch_budget() {
        let url = "https://example.com/cached.json";
        let mut cache = cache_with(&[(url, json!({ "description": "CACHED" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        for i in 0..MAX_REMOTE_FETCH_COUNT {
            assert!(ctx.try_visit(&format!("https://example.com/s{i}.json")));
        }
        let value = json!({ "$ref": url });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert_eq!(schema.description.as_deref(), Some("CACHED"));
    }

    #[test]
    fn uncached_remote_target_is_skipped_when_fetch_budget_is_exhausted() {
        let url = "https://example.com/uncached.json";
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);
        for i in 0..MAX_REMOTE_FETCH_COUNT {
            assert!(ctx.try_visit(&format!("https://example.com/s{i}.json")));
        }
        let value = json!({ "$ref": url });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert_eq!(schema.ref_path.as_deref(), Some(url));
        assert!(!cache.contains(url));
    }

    #[rstest]
    #[case::loopback("http://127.0.0.1/evil.json", None)]
    #[case::file_scheme("file:///etc/passwd", None)]
    #[case::relative_against_loopback_base("evil.json", Some("http://127.0.0.1/"))]
    fn ssrf_blocked_refs_do_not_consume_the_fetch_budget(
        #[case] ref_str: &str,
        #[case] base: Option<&str>,
    ) {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({ "$ref": ref_str });
        let schema = parse_with(&value, base, Some(&mut ctx)).expect("parses");
        assert_eq!(schema.ref_path.as_deref(), Some(ref_str));
        assert!(ctx.visited.is_empty());
    }

    #[test]
    fn remote_ref_in_an_unused_definition_is_not_resolved() {
        let url = "https://example.com/child.json";
        let mut cache = cache_with(&[(url, json!({ "description": "REMOTE" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "definitions": { "D": { "$ref": url } },
            "$defs": { "E": { "$ref": url } }
        });
        let schema = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert!(!debug_of(&schema).contains("REMOTE"));
    }

    #[test]
    fn remote_ref_in_an_unused_definition_is_not_fetched() {
        let url = "https://schema.invalid/unused.json";
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "definitions": { "D": { "$ref": url } },
            "$defs": { "E": { "$ref": url } }
        });
        parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert!(ctx.visited.is_empty());
        assert!(!ctx.cache.contains(url));
    }
}
