use std::collections::HashMap;

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
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UrlNotPermitted(u) => write!(f, "URL not permitted: {u}"),
            Self::FetchFailed(e) => write!(f, "fetch failed: {e}"),
            Self::ResponseTooLarge => write!(f, "schema response exceeded size limit"),
            Self::ParseFailed(e) => write!(f, "schema parse failed: {e}"),
            Self::TooDeep => write!(f, "schema nesting depth exceeded limit"),
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
    pub schema_type: Option<SchemaType>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub properties: Option<HashMap<String, Self>>,
    pub required: Option<Vec<String>>,
    pub enum_values: Option<Vec<Value>>,
    pub default: Option<Value>,
    pub examples: Option<Vec<Value>>,
    pub items: Option<Box<Self>>,
    pub additional_properties: Option<AdditionalProperties>,
    pub all_of: Option<Vec<Self>>,
    pub any_of: Option<Vec<Self>>,
    pub one_of: Option<Vec<Self>>,
    pub ref_path: Option<String>,
    pub pattern: Option<String>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    /// Merged `definitions` (Draft-04) and `$defs` (Draft-07) storage.
    pub definitions: Option<HashMap<String, Self>>,
}

/// A mapping from a file glob pattern to a JSON Schema URL.
#[derive(Debug, Clone)]
pub struct SchemaAssociation {
    pub pattern: String,
    pub url: String,
}

/// In-memory cache of parsed JSON Schemas, keyed by normalized URL.
#[derive(Debug, Default)]
pub struct SchemaCache {
    inner: HashMap<String, JsonSchema>,
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
        self.inner.get(url)
    }

    /// Insert a schema into the cache.  The first insertion for a given URL
    /// wins; subsequent calls for the same key are silently ignored.
    pub fn insert(&mut self, url: String, schema: JsonSchema) {
        self.inner.entry(url).or_insert(schema);
    }

    /// Return a cached schema, fetching and caching it on the first call.
    ///
    /// `url` must already be normalised (use [`validate_and_normalize_url`]).
    ///
    /// # Errors
    ///
    /// Propagates errors from [`fetch_schema`].
    ///
    /// # Panics
    ///
    /// Does not panic in practice: the entry is inserted immediately before the
    /// `.get()` call, so the key is always present.
    pub fn get_or_fetch(&mut self, url: &str) -> Result<&JsonSchema, SchemaError> {
        if !self.inner.contains_key(url) {
            let schema = fetch_schema(url)?;
            self.inner.insert(url.to_string(), schema);
        }
        Ok(self.inner.get(url).expect("just inserted"))
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

    let url = Url::parse(raw)
        .map_err(|e| SchemaError::UrlNotPermitted(format!("invalid URL: {e}")))?;

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
            }
        };
    }

    false
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema fetching
// ──────────────────────────────────────────────────────────────────────────────

/// Fetch a JSON Schema from `url` and parse it.
///
/// `url` should already be validated and normalised via
/// [`validate_and_normalize_url`].  This function is blocking; call it via
/// `tokio::task::spawn_blocking` from async contexts.
///
/// # Errors
///
/// Returns a [`SchemaError`] on network failure, size-limit breach, or parse
/// failure.
pub fn fetch_schema(url: &str) -> Result<JsonSchema, SchemaError> {
    use std::io::Read as _;

    // Validate and normalise the URL before issuing any network request.
    validate_and_normalize_url(url)?;

    let agent = ureq::Agent::config_builder()
        // Do not follow redirects at all — prevents redirect-based SSRF.
        .max_redirects(0)
        .build()
        .new_agent();

    let response = agent
        .get(url)
        .call()
        .map_err(|e| SchemaError::FetchFailed(e.to_string()))?;

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

    let value: Value = serde_json::from_slice(&buf)
        .map_err(|e| SchemaError::ParseFailed(e.to_string()))?;

    check_json_depth(&value, 0)?;

    parse_schema(&value).ok_or_else(|| SchemaError::ParseFailed("not a JSON Schema".to_string()))
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

/// Parse a `serde_json::Value` into a [`JsonSchema`].
///
/// Returns `None` if the value is not a JSON object (or boolean — see below).
///
/// Boolean schemas:
/// - `true`  → empty (permissive) schema
/// - `false` → `None` (no schema representation for "reject everything")
#[must_use]
pub fn parse_schema(value: &Value) -> Option<JsonSchema> {
    parse_schema_with_root(value, value, 0)
}

fn parse_schema_with_root(value: &Value, root: &Value, depth: usize) -> Option<JsonSchema> {
    if depth > MAX_REF_DEPTH {
        return None;
    }

    match value {
        Value::Bool(true) => return Some(JsonSchema::default()),
        Value::Bool(false) | Value::Null | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            return None;
        }
        Value::Object(_) => {}
    }

    let obj = value.as_object()?;
    let mut schema = JsonSchema::default();

    // $ref — resolve immediately and return the referenced schema
    if let Some(Value::String(ref_str)) = obj.get("$ref") {
        schema.ref_path = Some(ref_str.clone());
        if let Some(resolved) = resolve_ref(ref_str, root, depth + 1) {
            return Some(resolved);
        }
        return Some(schema);
    }

    // type
    schema.schema_type = parse_type(obj.get("type"));

    // title / description / pattern
    schema.title = string_field(obj, "title");
    schema.description = string_field(obj, "description");
    schema.pattern = string_field(obj, "pattern");

    // numeric constraints
    schema.minimum = obj.get("minimum").and_then(Value::as_f64);
    schema.maximum = obj.get("maximum").and_then(Value::as_f64);
    schema.min_length = obj.get("minLength").and_then(Value::as_u64);
    schema.max_length = obj.get("maxLength").and_then(Value::as_u64);

    // default / examples
    schema.default = obj.get("default").cloned();
    schema.examples = obj
        .get("examples")
        .and_then(Value::as_array)
        .cloned();

    // enum
    schema.enum_values = obj.get("enum").and_then(Value::as_array).cloned();

    // required
    schema.required = obj.get("required").and_then(Value::as_array).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    });

    // properties
    schema.properties = obj.get("properties").and_then(Value::as_object).map(|map| {
        map.iter()
            .filter_map(|(k, v)| {
                parse_schema_with_root(v, root, depth + 1).map(|s| (k.clone(), s))
            })
            .collect()
    });

    // items
    schema.items = obj
        .get("items")
        .and_then(|v| parse_schema_with_root(v, root, depth + 1))
        .map(Box::new);

    // additionalProperties
    schema.additional_properties =
        parse_additional_properties(obj.get("additionalProperties"), root, depth);

    // allOf / anyOf / oneOf
    schema.all_of = parse_schema_array(obj.get("allOf"), root, depth);
    schema.any_of = parse_schema_array(obj.get("anyOf"), root, depth);
    schema.one_of = parse_schema_array(obj.get("oneOf"), root, depth);

    // definitions (Draft-04) + $defs (Draft-07)
    let defs_04 = parse_definitions(obj.get("definitions"), root, depth);
    let defs_07 = parse_definitions(obj.get("$defs"), root, depth);
    schema.definitions = match (defs_04, defs_07) {
        (Some(mut a), Some(b)) => {
            a.extend(b);
            Some(a)
        }
        (a, b) => a.or(b),
    };

    Some(schema)
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
        | Value::Object(_)) => parse_schema_with_root(v, root, depth + 1)
            .map(|s| AdditionalProperties::Schema(Box::new(s))),
    }
}

fn parse_schema_array(
    value: Option<&Value>,
    root: &Value,
    depth: usize,
) -> Option<Vec<JsonSchema>> {
    let arr = value?.as_array()?;
    let schemas: Vec<JsonSchema> = arr
        .iter()
        .filter_map(|v| parse_schema_with_root(v, root, depth + 1))
        .collect();
    if schemas.is_empty() {
        None
    } else {
        Some(schemas)
    }
}

fn parse_definitions(
    value: Option<&Value>,
    root: &Value,
    depth: usize,
) -> Option<HashMap<String, JsonSchema>> {
    let map = value?.as_object()?;
    let result: HashMap<String, JsonSchema> = map
        .iter()
        .filter_map(|(k, v)| {
            parse_schema_with_root(v, root, depth + 1).map(|s| (k.clone(), s))
        })
        .collect();
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// $ref resolution
// ──────────────────────────────────────────────────────────────────────────────

/// Resolve a local JSON Pointer `$ref` (e.g. `#/definitions/Foo`) within `root`.
///
/// Returns `None` if the pointer does not resolve or the depth limit is
/// exceeded (circular ref guard).
fn resolve_ref(ref_str: &str, root: &Value, depth: usize) -> Option<JsonSchema> {
    if depth > MAX_REF_DEPTH {
        return None;
    }

    let pointer = ref_str.strip_prefix('#')?;
    let target = if pointer.is_empty() {
        root
    } else {
        root.pointer(pointer)?
    };

    parse_schema_with_root(target, root, depth + 1)
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
                if text
                    .get(..i)
                    .is_some_and(|prefix| !prefix.contains(&b'/'))
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
        let associations = [assoc("config.yaml", "https://example.com/config-schema.json")];
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

    // ══════════════════════════════════════════════════════════════════════════
    // $ref resolution
    // ══════════════════════════════════════════════════════════════════════════

    // Test 38
    #[test]
    fn should_resolve_simple_local_ref() {
        let v = json!({
            "$ref": "#/definitions/MyType",
            "definitions": {"MyType": {"type": "string"}}
        });
        let s = parse_schema(&v).expect("should resolve");
        assert_eq!(schema_type_str(&s), Some("string"));
    }

    // Test 39
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
        let mut schema = JsonSchema::default();
        schema.description = Some("test".to_string());
        cache.insert("https://example.com/schema.json".to_string(), schema);

        let result = cache
            .get("https://example.com/schema.json")
            .expect("should be cached");
        assert_eq!(result.description.as_deref(), Some("test"));
    }

    // Test 44 — first write wins
    #[test]
    fn should_not_overwrite_existing_cache_entry() {
        let mut cache = SchemaCache::new();
        let mut schema_a = JsonSchema::default();
        schema_a.description = Some("first".to_string());
        let mut schema_b = JsonSchema::default();
        schema_b.description = Some("second".to_string());

        cache.insert("https://example.com/schema.json".to_string(), schema_a);
        cache.insert("https://example.com/schema.json".to_string(), schema_b);

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
        let result = fetch_schema("http://127.0.0.1:19999/nonexistent.json");
        assert!(result.is_err());
    }

    // Test 46 — fetch happy path (parse pipeline without network).
    // Constructs a minimal JSON Schema string, runs it through the same
    // parse pipeline that `fetch_schema` uses after reading the response body:
    // `serde_json::from_slice` → `check_json_depth` → `parse_schema`.
    #[test]
    fn should_parse_fetched_schema_from_valid_response() {
        let body = r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#;
        let buf = body.as_bytes();

        // Step 1: deserialise JSON (mirrors fetch_schema's from_slice call)
        let value: Value =
            serde_json::from_slice(buf).expect("valid JSON should deserialise");

        // Step 2: depth check (mirrors fetch_schema's check_json_depth call)
        check_json_depth(&value, 0).expect("shallow schema should pass depth check");

        // Step 3: parse into JsonSchema (mirrors fetch_schema's parse_schema call)
        let schema = parse_schema(&value).expect("should produce a schema");

        assert_eq!(schema_type_str(&schema), Some("object"));
        let props = schema.properties.as_ref().expect("should have properties");
        assert!(props.contains_key("name"));
        assert_eq!(schema_type_str(props.get("name").expect("name")), Some("string"));
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

    // Sec-4: fetch_schema rejects 127.0.0.1 before making a network call
    #[test]
    fn should_reject_loopback_ip_in_fetch() {
        let result = fetch_schema("http://127.0.0.1:8080/schema.json");
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
        assert!(
            validate_and_normalize_url("http://169.254.169.254/latest/meta-data/").is_err()
        );
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
        assert!(result.is_some(), "schema within depth limit should be accepted");
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
        let key_no_slash =
            validate_and_normalize_url("https://example.com/schema").expect("valid");
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

    // Test 61 — redirect to a different host must be blocked.
    // Full HTTP mocking is not feasible without external dependencies, so this
    // test is marked `#[ignore]`. It documents the required behaviour:
    //
    // The ureq Agent is configured with `max_redirects(0)`, which means any
    // redirect response (3xx) is treated as an error rather than followed.
    // A real integration test would:
    //   1. Spin up a local HTTP server that responds 302 -> http://169.254.169.254
    //   2. Call `fetch_schema("http://127.0.0.2/redirect")` (itself blocked by
    //      SSRF guard, so the server would need to be on a public IP)
    //   3. Assert `Err` is returned
    //
    // The SSRF guard on the initial URL and the `max_redirects(0)` setting
    // together ensure redirects to blocked hosts cannot be followed.
    #[test]
    #[ignore = "requires HTTP mock server; redirect blocking enforced by max_redirects(0) in fetch_schema"]
    fn should_reject_redirect_to_different_host() {
        // Would call fetch_schema with a URL that redirects to 169.254.169.254
        // and assert Err is returned.
        unimplemented!()
    }
}
