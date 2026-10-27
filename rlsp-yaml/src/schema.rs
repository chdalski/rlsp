// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::net::IpAddr;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::Value;
use tower_lsp::lsp_types::Url;

use budget::{Meter, Meters, estimate_bytes, estimate_value};

pub use association::*;
pub use budget::SizeBudget;

/// Schema association management: glob-pattern-to-schema-URL mappings.
pub mod association;
/// Memory budgets of one schema parse.
mod budget;
/// Lookup of the sub-schema at a path through a parsed schema.
pub(crate) mod path;

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/// Maximum bytes read from a remote schema response (5 MiB).
pub(crate) const MAX_SCHEMA_BYTES: u64 = 5 * 1024 * 1024;

/// Maximum URL length (matches `document_links.rs`).
const MAX_URL_LENGTH: usize = 2048;

/// Maximum JSON nesting depth allowed during schema parsing; the parser recurses
/// no deeper than the documents it reads nest.
const MAX_JSON_DEPTH: usize = 50;

/// Maximum number of distinct remote schema URLs fetched during a single `$ref`
/// resolution pass.  Caps both breadth fan-out and circular-remote-ref loops.
const MAX_REMOTE_FETCH_COUNT: usize = 20;

// Estimated heap that the parse charges to its parsed budget (see `budget`).

/// A schema node: twice its size, because a node lives in a `Vec`, `HashMap`,
/// or `Box` that has grown, and the growth leaves up to as much room unused.
const NODE_BYTES: usize = 2 * size_of::<JsonSchema>();

/// What the allocator adds to the bytes a heap allocation asks for: a header
/// and the rounding up to its chunk size.
const ALLOCATION_OVERHEAD: usize = 32;

/// A `String` of `len` bytes: its header and its heap.
const fn string_bytes(len: usize) -> usize {
    size_of::<String>() + ALLOCATION_OVERHEAD + len
}

/// A `String` of `len` bytes in a `Vec`, whose growth leaves room for as many
/// headers again.
const fn listed_string_bytes(len: usize) -> usize {
    2 * size_of::<String>() + ALLOCATION_OVERHEAD + len
}

/// An entry of a hash table. The table doubles when it fills, and the old one
/// is alive while the entries move, so an entry counts three times.
const fn table_entry_bytes<K, V>() -> usize {
    3 * (size_of::<K>() + size_of::<V>() + 1)
}

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
    /// The parse exceeded a memory budget: the schema is not applied.
    TooLarge(SizeBudget),
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
            Self::TooLarge(_) => write!(f, "Schema is too large to parse within its memory budget"),
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

/// Index of a shared schema in the target table of a [`ParsedSchema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetId(usize);

impl TargetId {
    /// The target at `index` in the table passed to [`ParsedSchema::with_targets`].
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }
}

/// What a `$ref` node refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaRef {
    /// A schema in the target table of the [`ParsedSchema`] that holds the node.
    Target(TargetId),
    /// A `$ref` that could not be resolved, with its text.
    Unresolved(Arc<str>),
}

/// The result of parsing a schema document.
///
/// A parsed schema is shared (`Arc<ParsedSchema>`) between the cache and every
/// request that reads it, so it deliberately has no `Clone`.
///
/// A schema node whose `reference` is set is a reference node: it stands for
/// the target it refers to and has no content of its own. Code that reads a
/// sub-schema's fields must pass the sub-schema through [`Self::resolve`]
/// first; a reference node read directly looks like an empty schema.
#[derive(Debug)]
pub struct ParsedSchema {
    /// The root schema, which may be a reference node; read it through
    /// [`Self::resolved_root`].
    pub(crate) root: JsonSchema,
    /// Schemas that reference nodes share. None of them is a reference node.
    targets: Vec<JsonSchema>,
}

impl ParsedSchema {
    /// Wrap `root` as a parsed schema without shared targets.
    #[must_use]
    pub const fn new(root: JsonSchema) -> Self {
        Self {
            root,
            targets: Vec::new(),
        }
    }

    /// Build a parsed schema whose reference nodes may refer to `targets`.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::ParseFailed`] when a target is itself a
    /// reference node: a reference resolves in one step.
    pub fn with_targets(root: JsonSchema, targets: Vec<JsonSchema>) -> Result<Self, SchemaError> {
        if let Some(index) = targets.iter().position(|t| t.reference.is_some()) {
            return Err(SchemaError::ParseFailed(format!(
                "schema target {index} is a reference"
            )));
        }
        Ok(Self { root, targets })
    }

    /// The schema `schema` stands for: the target of a resolved reference node,
    /// and `schema` itself otherwise. A reference to a target that does not
    /// exist and an unresolved reference are `schema`, which reads as an empty
    /// schema.
    #[must_use]
    pub fn resolve<'a>(&'a self, schema: &'a JsonSchema) -> &'a JsonSchema {
        match &schema.reference {
            Some(SchemaRef::Target(id)) => self.targets.get(id.0).unwrap_or(schema),
            Some(SchemaRef::Unresolved(_)) | None => schema,
        }
    }

    /// The root schema, resolved.
    #[must_use]
    pub fn resolved_root(&self) -> &JsonSchema {
        self.resolve(&self.root)
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
    /// Set on a reference node, which stands for the schema it refers to.
    pub reference: Option<SchemaRef>,
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

/// Builders for schemas with shared targets, for the tests of every consumer.
#[cfg(test)]
pub(crate) mod test_support {
    use std::cell::RefCell;
    use std::fmt::Debug;

    use super::path::SchemaPathSegment;
    use super::{JsonSchema, ParsedSchema, SchemaRef, SchemaType, TargetId};

    /// The path segment for the mapping key `name`. An array item is always
    /// written as [`SchemaPathSegment::Item`], so a key spelled `[]` stays a key.
    pub fn key(name: &str) -> SchemaPathSegment {
        SchemaPathSegment::Key(name.to_string())
    }

    /// A reference node to the target at `index`.
    pub fn reference_to(index: usize) -> JsonSchema {
        JsonSchema {
            reference: Some(SchemaRef::Target(TargetId::new(index))),
            ..JsonSchema::default()
        }
    }

    /// A parsed schema without shared targets, for helpers that take the parsed
    /// schema next to the schema they inspect.
    pub fn no_targets() -> ParsedSchema {
        ParsedSchema::new(JsonSchema::default())
    }

    /// `root` with `targets` as its target table.
    pub fn with_targets(root: JsonSchema, targets: Vec<JsonSchema>) -> ParsedSchema {
        ParsedSchema::with_targets(root, targets).expect("no target is a reference node")
    }

    /// Runs `body` on a thread with a stack far smaller than the default one,
    /// so a recursion that the schema does not bound overflows it instead of
    /// passing unnoticed.
    pub fn on_a_small_stack(body: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(body)
            .expect("thread spawns")
            .join()
            .expect("the body does not panic or overflow its stack");
    }

    /// How [`Sharing::put`] places a schema.
    #[derive(Clone, Copy)]
    enum Placement {
        Inline,
        Shared,
        Dangling,
    }

    /// Places the schemas of one test schema either inline, behind reference
    /// nodes whose targets exist, or behind reference nodes whose targets do
    /// not.
    pub struct Sharing {
        placement: Placement,
        targets: RefCell<Vec<JsonSchema>>,
    }

    impl Sharing {
        const fn new(placement: Placement) -> Self {
            Self {
                placement,
                targets: RefCell::new(Vec::new()),
            }
        }

        /// `schema` itself when placing inline, otherwise a reference node to
        /// a new target that holds it. `schema` must not be a reference node.
        pub fn put(&self, schema: JsonSchema) -> JsonSchema {
            match self.placement {
                Placement::Inline => schema,
                Placement::Shared | Placement::Dangling => {
                    let mut targets = self.targets.borrow_mut();
                    targets.push(schema);
                    reference_to(targets.len() - 1)
                }
            }
        }

        fn finish(self, root: JsonSchema) -> ParsedSchema {
            match self.placement {
                Placement::Inline | Placement::Dangling => ParsedSchema::new(root),
                Placement::Shared => with_targets(root, self.targets.into_inner()),
            }
        }
    }

    /// Check that a consumer follows every reference `build` puts in a schema.
    ///
    /// `build` makes the schema, placing sub-schemas through [`Sharing::put`].
    /// `observe` reads what the consumer makes of a schema. The schema with
    /// reference nodes must give what the schema written inline gives, and the
    /// reference nodes alone, without their targets, must give something else:
    /// otherwise the case does not depend on the references.
    pub fn assert_references_followed<T: PartialEq + Debug>(
        build: impl Fn(&Sharing) -> JsonSchema,
        observe: impl Fn(&ParsedSchema) -> T,
    ) {
        let parsed = |placement| {
            let sharing = Sharing::new(placement);
            let root = build(&sharing);
            sharing.finish(root)
        };
        let expected = observe(&parsed(Placement::Inline));
        assert_eq!(
            observe(&parsed(Placement::Shared)),
            expected,
            "a reference is not followed"
        );
        assert_ne!(
            observe(&parsed(Placement::Dangling)),
            expected,
            "the case does not depend on its references"
        );
    }

    pub fn string_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        }
    }

    pub fn integer_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        }
    }

    pub fn boolean_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            ..JsonSchema::default()
        }
    }

    pub fn null_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("null".to_string())),
            ..JsonSchema::default()
        }
    }

    pub fn object_schema(props: Vec<(&str, JsonSchema)>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            ..JsonSchema::default()
        }
    }

    /// An array schema whose elements are `items`.
    pub fn array_of(items: JsonSchema) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            items: Some(Box::new(items)),
            ..JsonSchema::default()
        }
    }

    /// An object schema that declares the single string property `key`.
    pub fn declaring(key: &str) -> JsonSchema {
        object_schema(vec![(key, string_schema())])
    }

    pub fn all_of(branches: Vec<JsonSchema>) -> JsonSchema {
        JsonSchema {
            all_of: Some(branches),
            ..JsonSchema::default()
        }
    }

    pub fn any_of(branches: Vec<JsonSchema>) -> JsonSchema {
        JsonSchema {
            any_of: Some(branches),
            ..JsonSchema::default()
        }
    }

    pub fn one_of(branches: Vec<JsonSchema>) -> JsonSchema {
        JsonSchema {
            one_of: Some(branches),
            ..JsonSchema::default()
        }
    }

    /// `count` branches; branch `i` defines the single property `field_{i}`.
    pub fn field_branches(count: usize) -> Vec<JsonSchema> {
        (0..count)
            .map(|i| object_schema(vec![(&format!("field_{i}"), string_schema())]))
            .collect()
    }

    /// Wrap `schema` in `depth` levels of single-branch `allOf`.
    pub fn nest_in_all_of(schema: JsonSchema, depth: usize) -> JsonSchema {
        (0..depth).fold(schema, |inner, _| all_of(vec![inner]))
    }

    /// A schema that offers `a` and `next`, where `next` is the schema again, and
    /// that composes itself.
    pub fn cyclic() -> ParsedSchema {
        let target = JsonSchema {
            all_of: Some(vec![reference_to(0)]),
            one_of: Some(vec![reference_to(0)]),
            ..object_schema(vec![("a", string_schema()), ("next", reference_to(0))])
        };
        with_targets(reference_to(0), vec![target])
    }

    /// Length of the chains that the tests of every consumer build.
    pub const CHAIN_LENGTH: usize = 10_000;

    /// `length` targets where each composes the next, and the last one declares `leaf`.
    pub fn chain(length: usize) -> ParsedSchema {
        let mut targets: Vec<JsonSchema> = (1..length)
            .map(|next| all_of(vec![reference_to(next)]))
            .collect();
        targets.push(declaring("leaf"));
        with_targets(reference_to(0), targets)
    }

    /// `layers` layers of two targets that compose both targets of the next layer,
    /// ending in one target that declares `leaf`: `2^layers` paths, `2 * layers`
    /// targets.
    pub fn ladder(layers: usize) -> ParsedSchema {
        let mut targets = Vec::new();
        for layer in 0..layers {
            let next = 2 * layer + 2;
            let below = if layer + 1 == layers {
                vec![reference_to(next)]
            } else {
                vec![reference_to(next), reference_to(next + 1)]
            };
            targets.push(all_of(below.clone()));
            targets.push(all_of(below));
        }
        targets.push(declaring("leaf"));
        with_targets(all_of(vec![reference_to(0), reference_to(1)]), targets)
    }

    /// A schema whose property `here` is a target that offers `a` and composes
    /// itself twice through `composing`.
    pub fn composes_itself(composing: fn(Vec<JsonSchema>) -> JsonSchema) -> ParsedSchema {
        let target = JsonSchema {
            properties: Some([("a".to_string(), string_schema())].into()),
            ..composing(vec![reference_to(0), reference_to(0)])
        };
        with_targets(
            object_schema(vec![("here", reference_to(0)), ("a", string_schema())]),
            vec![target],
        )
    }
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

/// In-memory cache of fetched schema documents and parsed schemas, both keyed
/// by normalized URL.
///
/// The two maps are independent:
///
/// - `documents` holds raw fetched JSON.  A remote `$ref` stores the document
///   it fetches here and parses only the target it points at, so fragment-bearing
///   refs (`other.json#/definitions/Foo`) navigate the raw document with a JSON
///   Pointer.
/// - `schemas` holds the [`ParsedSchema`] of a whole document used as a
///   document's own schema, parsed with remote `$ref` resolution.
///
/// Entries are keyed by **fetch URL**, never by a document's self-declared
/// `$id`, to prevent `$id`-spoofing cache poisoning.  In each map the first
/// insertion for a URL wins.
#[derive(Debug, Default)]
pub struct SchemaCache {
    documents: HashMap<String, Arc<Value>>,
    schemas: HashMap<String, Arc<ParsedSchema>>,
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

    /// Return the parsed schema of the whole document at `url`, or `None` on a
    /// cache miss.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&Arc<ParsedSchema>> {
        self.schemas.get(url)
    }

    /// Insert the parsed schema of the whole document at `url`.  The first
    /// insertion for a given URL wins; subsequent calls for the same key are
    /// silently ignored.
    pub fn insert(&mut self, url: String, schema: Arc<ParsedSchema>) {
        self.schemas.entry(url).or_insert(schema);
    }

    /// Return the raw document fetched from `url`, or `None` on a cache miss.
    #[must_use]
    pub fn get_document(&self, url: &str) -> Option<&Value> {
        self.documents.get(url).map(|document| &**document)
    }

    /// Return the raw document fetched from `url` as a shared handle, for a
    /// parse that reads it while the cache is written to.
    pub(crate) fn shared_document(&self, url: &str) -> Option<Arc<Value>> {
        self.documents.get(url).cloned()
    }

    /// Insert the raw document fetched from `url`.  The first insertion for a
    /// given URL wins; subsequent calls for the same key are silently ignored.
    pub fn insert_document(&mut self, url: String, document: Value) {
        self.documents
            .entry(url)
            .or_insert_with(|| Arc::new(document));
    }

    /// Return whether the raw document at `url` is already in the cache
    /// (avoids a fetch).
    #[must_use]
    pub fn contains_document(&self, url: &str) -> bool {
        self.documents.contains_key(url)
    }

    /// Drop the raw document at `url`, which a parse that failed for its size
    /// fetched and must not leave behind.
    fn remove_document(&mut self, url: &str) {
        self.documents.remove(url);
    }

    /// Move the entries of `other` into `self`, for merging a returned cache
    /// back into the live cache after a `spawn_blocking` pass.  Each map keeps
    /// its first-insertion-wins invariant: an entry `self` already holds is not
    /// overwritten, so a URL inserted concurrently during the pass survives.
    /// The two maps merge independently, so a URL's document and parsed schema
    /// may come from different fetches.
    pub fn merge(&mut self, other: Self) {
        for (url, document) in other.documents {
            self.documents.entry(url).or_insert(document);
        }
        for (url, schema) in other.schemas {
            self.insert(url, schema);
        }
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
/// A document that a schema parse reads passes the `raw` budget of the parse:
/// the heap its text deserializes to is estimated from the text and charged
/// before `serde_json` allocates any of it.
///
/// # Errors
///
/// - [`SchemaError::FetchFailed`] on network failure, a 4xx/5xx HTTP status,
///   or a 3xx response (redirects are not followed).
/// - [`SchemaError::ResponseTooLarge`] when the body exceeds `MAX_SCHEMA_BYTES`.
/// - [`SchemaError::TooLarge`] when the estimate exceeds the `raw` budget.
/// - [`SchemaError::ParseFailed`] when the body is not valid JSON.
fn fetch_json(
    agent: &ureq::Agent,
    url: &str,
    raw: Option<&mut Meter>,
) -> Result<Value, SchemaError> {
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

    if let Some(raw) = raw
        && !raw.charge(estimate_bytes(&buf))
    {
        return Err(SchemaError::TooLarge(SizeBudget::Raw));
    }

    serde_json::from_slice(&buf).map_err(|e| SchemaError::ParseFailed(e.to_string()))
}

/// Fetch the JSON document at `url` without parsing it as a schema.
///
/// `url` is validated with [`validate_and_normalize_url`] before any network
/// request, and the document is only returned once it passed the size and
/// nesting limits — nothing unchecked reaches the cache.
///
/// # Errors
///
/// - [`SchemaError::UrlNotPermitted`] when `url` fails the SSRF guard.
/// - [`SchemaError::FetchFailed`] on network failure, a 4xx/5xx HTTP status,
///   or a 3xx response (redirects are not followed).
/// - [`SchemaError::ResponseTooLarge`] when the body exceeds `MAX_SCHEMA_BYTES`.
/// - [`SchemaError::TooLarge`] when the document would exceed the `raw` budget.
/// - [`SchemaError::ParseFailed`] when the body is not valid JSON.
/// - [`SchemaError::TooDeep`] when the JSON nests beyond the depth limit.
fn fetch_document(url: &str, proxy: Option<&str>, raw: &mut Meter) -> Result<Value, SchemaError> {
    validate_and_normalize_url(url)?;

    let value = fetch_json(&build_agent(proxy), url, Some(raw))?;

    check_json_depth(&value, 0)?;
    Ok(value)
}

/// Fetch a JSON Schema from `url`, returning the raw `Value` and the parsed
/// schema of the whole document.
///
/// `url` is validated with [`validate_and_normalize_url`] before any network
/// request.  This function is blocking; call it via
/// `tokio::task::spawn_blocking` from async contexts.
///
/// When `proxy` is `Some`, requests are routed through the given proxy URL.
///
/// When `ctx` is `Some`, remote `$ref` URIs within the fetched schema are
/// resolved one level deep (depth-1 remote resolution), and a document already
/// held by the context's cache under the normalized `url` is used instead of
/// fetching it again.  `$ref`s inside those resolved remote documents are not
/// followed further — this is intentional to cap the blast radius of a
/// malicious schema that chains many remote refs.  The breadth guard
/// (`MAX_REMOTE_FETCH_COUNT`) and dedup set in `ctx` enforce an absolute cap
/// even if this design decision is revisited later.  The parsed schema holds
/// each `$ref` target once, as [`parse_schema`] describes.
///
/// The parse is bounded in memory, together with the documents it reads: see
/// [`SchemaError::TooLarge`]. When it fails for that reason, the documents it
/// fetched for the `$ref`s are not kept in the cache.
///
/// # Errors
///
/// - [`SchemaError::UrlNotPermitted`] when `url` fails the SSRF guard.
/// - [`SchemaError::FetchFailed`] on network failure, a 4xx/5xx HTTP status,
///   or a 3xx response (redirects are not followed).
/// - [`SchemaError::ResponseTooLarge`] when the body exceeds `MAX_SCHEMA_BYTES`.
/// - [`SchemaError::TooLarge`] when the parse exceeds a memory budget.
/// - [`SchemaError::ParseFailed`] when the body is not JSON or not a schema.
/// - [`SchemaError::TooDeep`] when the JSON nests beyond the depth limit.
pub fn fetch_schema_raw(
    url: &str,
    proxy: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
) -> Result<(Value, ParsedSchema), SchemaError> {
    fetch_schema_within(url, proxy, ctx, &mut Meters::production())
}

/// [`fetch_schema_raw`] with the budgets of `meters`.
fn fetch_schema_within(
    url: &str,
    proxy: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
    meters: &mut Meters,
) -> Result<(Value, ParsedSchema), SchemaError> {
    // Validate and normalise the URL before the cache lookup or any network
    // request: the cache never answers for a URL the guard rejects.
    let url = validate_and_normalize_url(url)?;

    let value = match ctx.as_deref().and_then(|ctx| ctx.cache.get_document(&url)) {
        Some(cached) => {
            // The cache keeps its copy and the caller gets another one.
            charge_document(&mut meters.raw, cached, 2)?;
            cached.clone()
        }
        None => fetch_document(&url, proxy, &mut meters.raw)?,
    };

    let schema = parse_document(&value, Some(&url), ctx, meters)?;
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
    let value = fetch_json(&build_agent(proxy), SCHEMASTORE_CATALOG_URL, None)?;

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

/// Resolve `relative` against `base`, returning an absolute URI string.
///
/// If `relative` is already an absolute URI it is returned as-is.
/// Returns `None` when `base` is `None` or when joining fails.
fn resolve_uri(base: Option<&str>, relative: &str) -> Option<String> {
    // Bound the work of parsing before doing it: a URL beyond the limit is
    // rejected by the SSRF guard anyway.
    if relative.len() > MAX_URL_LENGTH || base.is_some_and(|b| b.len() > MAX_URL_LENGTH) {
        return None;
    }
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

/// Parse a `serde_json::Value` into a [`ParsedSchema`].
///
/// Boolean schemas:
/// - `true`  → empty (permissive) schema
/// - `false` → [`SchemaError::ParseFailed`] (no schema representation for
///   "reject everything")
///
/// Each `$ref` target is parsed once and shared: the parsed schema holds a
/// reference node wherever a `$ref` stands, and the target table holds the
/// schema it refers to.
///
/// The parse is bounded in memory: `value` and the schema built from it have
/// a budget each, in estimated heap bytes (128 MiB and 256 MiB). A parse that
/// exceeds one fails; it never returns part of a schema.
///
/// # Errors
///
/// - [`SchemaError::TooDeep`] when the JSON nests beyond the depth limit.
/// - [`SchemaError::TooLarge`] when the parse exceeds a memory budget.
/// - [`SchemaError::ParseFailed`] when the root is not a JSON object or
///   boolean `true`.
pub fn parse_schema(value: &Value) -> Result<ParsedSchema, SchemaError> {
    parse_within(value, None, None, &mut Meters::production())
}

/// Parse the schema document `value`, which the caller already holds, within
/// the budgets of `meters`: the heap of `value` counts against the raw one.
fn parse_within(
    value: &Value,
    url: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
    meters: &mut Meters,
) -> Result<ParsedSchema, SchemaError> {
    charge_document(&mut meters.raw, value, 1)?;
    parse_document(value, url, ctx, meters)
}

/// Charge `copies` times the estimated heap of the deserialized `document` to
/// the `raw` budget.
fn charge_document(raw: &mut Meter, document: &Value, copies: usize) -> Result<(), SchemaError> {
    // The estimate recurses as deep as the document nests.
    check_json_depth(document, 0)?;
    if raw.charge(estimate_value(document).saturating_mul(copies)) {
        Ok(())
    } else {
        Err(SchemaError::TooLarge(SizeBudget::Raw))
    }
}

/// Parse the schema document `value`, fetched from `url` when it has one.
///
/// With a `ctx`, remote `$ref`s of the document resolve one level deep, as
/// [`fetch_schema_raw`] describes. The documents it reads other than `value`
/// are charged to the raw budget of `meters` as they are read.
fn parse_document(
    value: &Value,
    url: Option<&str>,
    ctx: Option<&mut ParseContext<'_>>,
    meters: &mut Meters,
) -> Result<ParsedSchema, SchemaError> {
    check_json_depth(value, 0)?;
    if !matches!(value, Value::Object(_) | Value::Bool(true)) {
        return Err(SchemaError::ParseFailed("not a JSON Schema".to_string()));
    }
    SchemaParser::new(value, url, ctx, meters).parse_all(value, url)
}

/// Index of a document in the document table of one parse; the root document
/// is the one being parsed, the others are fetched for its remote `$ref`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DocId(usize);

const ROOT_DOC: DocId = DocId(0);

/// Tokens of a JSON Pointer, unescaped. Shared between the locations that
/// name the same place, so a location costs the same however long the keys on
/// its path are.
type Path = Rc<[Rc<str>]>;

/// Identity of a place in a document: the address of its value, which holds
/// still while the parse holds the document. A JSON pointer, an anchor, and
/// the document's URL that name the same value name the same key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LocationKey {
    doc: DocId,
    address: usize,
}

/// A place in a document, with the path that leads to it from the root.
#[derive(Debug, Clone)]
struct Location {
    key: LocationKey,
    path: Path,
}

/// What a `$ref` settles to once chains of `$ref`-only schemas are followed.
#[derive(Debug, Clone)]
enum Settled {
    /// A schema that is not a `$ref` itself.
    Content(Location),
    /// A `$ref` in the chain that cannot be resolved, with its text. The text
    /// is shared, so a long chain and every place that reaches it hold one copy.
    Unresolved(Arc<str>),
    /// A chain that ends in itself.
    Permissive,
}

/// Which keyword a reference was written with.
#[derive(Debug, Clone, Copy)]
enum Keyword {
    Ref,
    DynamicRef,
}

/// What a schema object's `$ref` or `$dynamicRef` stands for.
#[derive(Debug, Clone)]
enum Reference {
    /// The schema at this place, a target.
    Content(Location),
    /// A reference that cannot be resolved, with its text.
    Unresolved(Arc<str>),
    /// A chain of references that ends in itself.
    Permissive,
    /// No reference: the keywords of the object apply.
    Plain,
}

/// Where a schema is parsed: its document and the base URI that its relative
/// references resolve against.
#[derive(Debug, Clone)]
struct Scope {
    doc: DocId,
    base: Option<Rc<str>>,
}

/// A document of one parse. The root document is borrowed from the caller;
/// fetched documents are shared with the cache.
#[derive(Debug, Clone)]
enum Source<'r> {
    Borrowed(&'r Value),
    Shared(Arc<Value>),
}

impl Source<'_> {
    fn value(&self) -> &Value {
        match self {
            Source::Borrowed(value) => value,
            Source::Shared(value) => value,
        }
    }
}

static NULL: Value = Value::Null;

struct Document<'r> {
    source: Source<'r>,
    /// The URL the document was fetched from; the base URI of its root.
    url: Option<String>,
    /// Normalized URLs that name this document, so that a `$ref` to one of
    /// them is a reference into the document itself.
    names: Vec<String>,
    /// `$anchor` and `$dynamicAnchor` names to their paths, built on the
    /// first anchor reference into the document.
    anchors: Option<HashMap<String, Path>>,
}

/// The state of one parse: the documents it reads, the remote context, the
/// table of targets, and the budgets.
///
/// Each place that a `$ref` names gets one target, parsed once from a
/// worklist, so the parse recurses only as deep as the JSON nests.
///
/// The parse has two passes. The discovery pass walks the schema positions of
/// the root and of every target and settles each reference, so that all
/// targets are known before any is built. The build pass then emits a reference
/// node wherever a target sits, inside the root or inside another target, so
/// every JSON object is parsed at most once and the parse has at most as many
/// schema nodes as the documents have objects.
///
/// Before the parse makes an allocation that grows with its input it charges
/// an estimate to the parsed budget; once a budget is exceeded it makes no
/// further allocation of that kind, fetches nothing, and fails as a whole, so
/// that the places that read a missing schema as "no schema" cannot let a
/// partly built one through.
struct SchemaParser<'r, 'c> {
    documents: Vec<Document<'r>>,
    /// Fetched documents by URL; `None` for a document that cannot be used.
    remote_documents: HashMap<String, Option<DocId>>,
    ctx: Option<&'r mut ParseContext<'c>>,
    meters: &'r mut Meters,
    /// The URLs of the documents this parse added to the cache.
    fetched: Vec<String>,
    root_key: LocationKey,
    target_ids: HashMap<LocationKey, usize>,
    /// The place of each target, by target id.
    target_locations: Vec<Location>,
    /// Parsed targets by id; `None` until the build pass reaches them.
    targets: Vec<Option<JsonSchema>>,
    /// Schema objects the discovery pass has walked.
    walked: HashSet<LocationKey>,
    settled: HashMap<LocationKey, Settled>,
    /// The base URI that the `$id` of a schema object establishes, by object.
    bases: HashMap<LocationKey, Rc<str>>,
}

fn address_of<T>(value: &T) -> usize {
    std::ptr::from_ref(value).addr()
}

/// The `$id` (Draft-06+) or `id` (Draft-04) of a schema object.
fn declared_id(obj: &serde_json::Map<String, Value>) -> Option<&str> {
    obj.get("$id")
        .or_else(|| obj.get("id"))
        .and_then(Value::as_str)
}

/// The reference a schema object holds, `$ref` before `$dynamicRef`.
fn reference_keyword(obj: &serde_json::Map<String, Value>) -> Option<(&str, Keyword)> {
    if let Some(Value::String(text)) = obj.get("$ref") {
        return Some((text, Keyword::Ref));
    }
    if let Some(Value::String(text)) = obj.get("$dynamicRef") {
        return Some((text, Keyword::DynamicRef));
    }
    None
}

/// Whether the array form of `items` is read as a tuple: `prefixItems` holds
/// no schema, so it does not take that role. Parse and discovery share this.
fn reads_tuple_items(obj: &serde_json::Map<String, Value>) -> bool {
    let prefix_holds_a_schema = obj
        .get("prefixItems")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(is_schema_value));
    obj.get("items").is_some_and(Value::is_array) && !prefix_holds_a_schema
}

/// Whether `additionalItems` is read: only next to a tuple written in the
/// array form of `items`, without any `prefixItems`. Parse and discovery share
/// this.
fn reads_additional_items(obj: &serde_json::Map<String, Value>) -> bool {
    obj.get("items").is_some_and(Value::is_array) && obj.get("prefixItems").is_none()
}

/// The values at the schema positions of `obj`: the ones `parse_object` reads
/// as sub-schemas, wherever the references among them lead.
fn schema_children(obj: &serde_json::Map<String, Value>) -> Vec<&Value> {
    let values_of = |key: &str| -> Vec<&Value> {
        obj.get(key)
            .and_then(Value::as_object)
            .map_or_else(Vec::new, |map| map.values().collect())
    };
    let items_of = |key: &str| -> Vec<&Value> {
        obj.get(key)
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |items| items.iter().collect())
    };
    let single = |key: &str| {
        obj.get(key)
            .filter(|value| !matches!(value, Value::Bool(false)))
    };
    let tuple_items = reads_tuple_items(obj);
    let dependencies = match obj.get("dependencies") {
        Some(Value::Object(map)) => map.values().filter(|value| !value.is_array()).collect(),
        _ => Vec::new(),
    };

    let mut children = values_of("properties");
    children.extend(values_of("patternProperties"));
    children.extend(items_of("prefixItems"));
    match obj.get("items") {
        Some(Value::Array(items)) if tuple_items => children.extend(items),
        Some(Value::Array(_)) | None => {}
        Some(items) => children.push(items),
    }
    if reads_additional_items(obj) {
        children.extend(single("additionalItems"));
    }
    children.extend(dependencies);
    children.extend(values_of("dependentSchemas"));
    children.extend(items_of("allOf"));
    children.extend(items_of("anyOf"));
    children.extend(items_of("oneOf"));
    for key in [
        "contentSchema",
        "contains",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
        "unevaluatedItems",
    ] {
        children.extend(obj.get(key));
    }
    children.extend(single("additionalProperties"));
    children.extend(single("unevaluatedProperties"));
    children
}

fn reference_node(target: usize) -> JsonSchema {
    JsonSchema {
        reference: Some(SchemaRef::Target(TargetId::new(target))),
        ..JsonSchema::default()
    }
}

/// The tokens of a JSON Pointer (RFC 6901), unescaped.
fn pointer_path(pointer: &str) -> Path {
    pointer
        .split('/')
        .skip(1)
        .map(|token| Rc::from(token.replace("~1", "/").replace("~0", "~")))
        .collect()
}

/// An array index token as `serde_json` reads it: no sign, no leading zero.
fn array_index(token: &str) -> Option<usize> {
    if token.starts_with('+') || (token.starts_with('0') && token.len() > 1) {
        return None;
    }
    token.parse().ok()
}

fn child<'a>(node: &'a Value, token: &str) -> Option<&'a Value> {
    match node {
        Value::Object(map) => map.get(token),
        Value::Array(items) => array_index(token).and_then(|index| items.get(index)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

fn navigate<'a>(root: &'a Value, path: &[Rc<str>]) -> Option<&'a Value> {
    path.iter().try_fold(root, |node, token| child(node, token))
}

/// Whether `value` can be the target of a reference.
const fn is_schema_value(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Bool(true))
}

/// Index the `$anchor` and `$dynamicAnchor` names of `root` to their paths.
/// The first object in document order wins, as for a search of the tree.
///
/// Each anchor is charged to `meter` before it is indexed, and the index stops
/// growing once `meter` is exceeded.
fn index_anchors(root: &Value, meter: &mut Meter) -> HashMap<String, Path> {
    let mut index = HashMap::new();
    index_anchors_below(root, &mut Vec::new(), &mut index, meter);
    index
}

fn index_anchors_below(
    value: &Value,
    path: &mut Vec<Rc<str>>,
    index: &mut HashMap<String, Path>,
    meter: &mut Meter,
) {
    match value {
        Value::Object(obj) => {
            for keyword in ["$anchor", "$dynamicAnchor"] {
                if let Some(Value::String(name)) = obj.get(keyword)
                    && !index.contains_key(name)
                    && !meter.is_over()
                {
                    // The name, its entry, and a path of tokens that no other
                    // path may share.
                    let entry = table_entry_bytes::<String, Path>()
                        + string_bytes(name.len())
                        + path.len() * (size_of::<Rc<str>>() + 2 * ALLOCATION_OVERHEAD)
                        + ALLOCATION_OVERHEAD;
                    if meter.charge(entry) {
                        index.insert(name.clone(), Path::from(path.as_slice()));
                    }
                }
            }
            for (key, member) in obj {
                index_anchors_in(member, key, path, index, meter);
            }
        }
        Value::Array(items) => {
            for (position, member) in items.iter().enumerate() {
                index_anchors_in(member, &position.to_string(), path, index, meter);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn index_anchors_in(
    member: &Value,
    token: &str,
    path: &mut Vec<Rc<str>>,
    index: &mut HashMap<String, Path>,
    meter: &mut Meter,
) {
    if !meter.is_over() && matches!(member, Value::Object(_) | Value::Array(_)) {
        path.push(Rc::from(token));
        index_anchors_below(member, path, index, meter);
        path.pop();
    }
}

/// The normalized form of `raw` without its fragment, if it is a URL that
/// passes the SSRF guard.
fn document_name(raw: &str) -> Option<String> {
    if raw.len() > MAX_URL_LENGTH {
        return None;
    }
    let mut url = Url::parse(raw).ok()?;
    url.set_fragment(None);
    validate_and_normalize_url(url.as_str()).ok()
}

impl<'r, 'c> SchemaParser<'r, 'c> {
    fn new(
        root: &'r Value,
        url: Option<&str>,
        ctx: Option<&'r mut ParseContext<'c>>,
        meters: &'r mut Meters,
    ) -> Self {
        let declared = root
            .as_object()
            .and_then(declared_id)
            .and_then(|raw| resolve_uri(url, raw));
        let names = url
            .map(String::from)
            .into_iter()
            .chain(declared)
            .filter_map(|name| document_name(&name))
            .collect();
        Self {
            documents: vec![Document {
                source: Source::Borrowed(root),
                url: url.map(String::from),
                names,
                anchors: None,
            }],
            remote_documents: HashMap::new(),
            ctx,
            meters,
            fetched: Vec::new(),
            root_key: LocationKey {
                doc: ROOT_DOC,
                address: address_of(root),
            },
            target_ids: HashMap::new(),
            target_locations: Vec::new(),
            targets: Vec::new(),
            walked: HashSet::new(),
            settled: HashMap::new(),
            bases: HashMap::new(),
        }
    }

    /// Discover the targets, then build the root schema and every target, and
    /// assemble the result.
    ///
    /// A budget that is exceeded in discovery ends the parse before any node
    /// is built, and one that is exceeded while building ends it at once.
    fn parse_all(&mut self, root: &Value, url: Option<&str>) -> Result<ParsedSchema, SchemaError> {
        let scope = Scope {
            doc: ROOT_DOC,
            base: url.map(Rc::from),
        };
        self.discover_node(root, &scope);
        let mut discovered = 0;
        while !self.meters.is_over()
            && let Some(location) = self.target_locations.get(discovered).cloned()
        {
            self.discover_location(&location);
            discovered += 1;
        }
        self.finish_if_over()?;

        // A reference to the root makes the root a target like any other.
        let root_schema = match self.target_ids.get(&self.root_key).copied() {
            Some(id) => {
                if self.charge_node() {
                    reference_node(id)
                } else {
                    JsonSchema::default()
                }
            }
            None => self.parse_node(root, &scope).unwrap_or_default(),
        };
        // Building a target can find another one that discovery did not.
        let mut built = 0;
        while !self.meters.is_over()
            && let Some(location) = self.target_locations.get(built).cloned()
        {
            let schema = self.parse_target(&location);
            if let Some(slot) = self.targets.get_mut(built) {
                *slot = Some(schema);
            }
            built += 1;
        }
        self.finish_if_over()?;

        let targets = std::mem::take(&mut self.targets)
            .into_iter()
            .map(Option::unwrap_or_default);
        ParsedSchema::with_targets(root_schema, targets.collect())
    }

    /// Fail with the budget that is exceeded, if one is, and drop the
    /// documents this parse added to the cache: a parse that was too large
    /// must not leave what it fetched behind.
    fn finish_if_over(&mut self) -> Result<(), SchemaError> {
        let Some(budget) = self.meters.exceeded() else {
            return Ok(());
        };
        if let Some(ctx) = self.ctx.as_deref_mut() {
            for url in &self.fetched {
                ctx.cache.remove_document(url);
            }
        }
        Err(SchemaError::TooLarge(budget))
    }

    /// Charge `bytes` to the parsed budget; `false` once it is exceeded, and
    /// the caller then leaves out the allocation it was about to make.
    const fn charge(&mut self, bytes: usize) -> bool {
        self.meters.parsed.charge(bytes)
    }

    /// Charge the parsed budget for a schema node; `false` once it is
    /// exceeded, and the caller then builds no node.
    const fn charge_node(&mut self) -> bool {
        #[cfg(test)]
        {
            if self.meters.nodes == 0 {
                self.meters.spent_before_first_node = self.meters.parsed.spent();
            }
            self.meters.nodes += 1;
        }
        self.charge(NODE_BYTES)
    }

    /// A copy of `value`, or `None` once the parsed budget is exceeded.
    fn copy_string(&mut self, value: &str) -> Option<String> {
        self.charge(string_bytes(value.len()))
            .then(|| value.to_string())
    }

    /// A copy of the string at `key` of `obj`.
    fn string_field(&mut self, obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
        self.copy_string(obj.get(key)?.as_str()?)
    }

    /// The strings among `items`, copied up to the parsed budget.
    fn copy_strings(&mut self, items: &[Value]) -> Vec<String> {
        items
            .iter()
            .filter_map(Value::as_str)
            .take_while(|text| self.charge(listed_string_bytes(text.len())))
            .map(String::from)
            .collect()
    }

    /// A copy of `value`, or `None` once the parsed budget is exceeded. The
    /// copy is charged before it is made.
    fn copy_value(&mut self, value: &Value) -> Option<Value> {
        (!self.meters.is_over() && self.charge(estimate_value(value))).then(|| value.clone())
    }

    /// A copy of `values`, or `None` once the parsed budget is exceeded.
    fn copy_values(&mut self, values: &[Value]) -> Option<Vec<Value>> {
        if self.meters.is_over() {
            return None;
        }
        let bytes = values.iter().fold(0, |total: usize, value| {
            total.saturating_add(estimate_value(value))
        });
        self.charge(bytes).then(|| values.to_vec())
    }

    fn source(&self, doc: DocId) -> Source<'r> {
        self.documents
            .get(doc.0)
            .map_or(Source::Borrowed(&NULL), |document| document.source.clone())
    }

    /// The id of the target for `location`, allocated before the target is
    /// parsed so that references to it from inside itself terminate; `None`
    /// once the parsed budget is exceeded.
    fn target_id(&mut self, location: &Location) -> Option<usize> {
        if let Some(&id) = self.target_ids.get(&location.key) {
            return Some(id);
        }
        // The slot for the target, its place, and the table entry that finds
        // it. The path of the place is shared with the places that name it,
        // and was charged where it was made.
        let bytes = 2 * size_of::<Option<JsonSchema>>()
            + 2 * size_of::<Location>()
            + table_entry_bytes::<LocationKey, usize>();
        if !self.charge(bytes) {
            return None;
        }
        let id = self.targets.len();
        self.targets.push(None);
        self.target_locations.push(location.clone());
        self.target_ids.insert(location.key, id);
        Some(id)
    }

    fn parse_target(&mut self, location: &Location) -> JsonSchema {
        if !self.charge_node() {
            return JsonSchema::default();
        }
        let source = self.source(location.key.doc);
        let Some(Value::Object(obj)) = navigate(source.value(), &location.path) else {
            return JsonSchema::default();
        };
        let scope = Scope {
            doc: location.key.doc,
            base: self.enclosing_base(location),
        };
        self.parse_object(obj, &scope)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Base URIs
    // ──────────────────────────────────────────────────────────────────────────

    /// The base URI that `raw`, the `$id` of `obj`, establishes below `obj`.
    /// Resolved once per object.
    fn resolve_id(
        &mut self,
        doc: DocId,
        obj: &serde_json::Map<String, Value>,
        parent_base: Option<&str>,
        raw: &str,
    ) -> Rc<str> {
        let key = LocationKey {
            doc,
            address: address_of(obj),
        };
        if let Some(base) = self.bases.get(&key) {
            return Rc::clone(base);
        }
        // The resolved URI is at most `MAX_URL_LENGTH` long, or `raw` itself;
        // it is made before it is charged, and the memo entry after.
        let resolved: Rc<str> =
            Rc::from(resolve_uri(parent_base, raw).unwrap_or_else(|| raw.to_string()));
        let bytes = table_entry_bytes::<LocationKey, Rc<str>>()
            + size_of::<Rc<str>>()
            + ALLOCATION_OVERHEAD
            + resolved.len();
        if self.charge(bytes) {
            self.bases.insert(key, Rc::clone(&resolved));
        }
        resolved
    }

    /// The base URI in force around `location`: the document's URL, moved by
    /// the `$id` of each object on the way to the location.
    fn enclosing_base(&mut self, location: &Location) -> Option<Rc<str>> {
        let doc = location.key.doc;
        let source = self.source(doc);
        let mut base: Option<Rc<str>> = self
            .documents
            .get(doc.0)
            .and_then(|document| document.url.as_deref())
            .map(Rc::from);
        let mut node = source.value();
        for token in location.path.iter() {
            if let Value::Object(obj) = node
                && let Some(raw) = declared_id(obj)
            {
                base = Some(self.resolve_id(doc, obj, base.as_deref(), raw));
            }
            let Some(next) = child(node, token) else {
                break;
            };
            node = next;
        }
        base
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Schema nodes
    // ──────────────────────────────────────────────────────────────────────────

    /// Parse the schema `value`; `None` for a value that is no schema.
    ///
    /// A schema that a reference names is a target, built once on its own;
    /// where it sits it is a reference node.
    fn parse_node(&mut self, value: &Value, scope: &Scope) -> Option<JsonSchema> {
        if is_schema_value(value) {
            if self.meters.is_over() || !self.charge_node() {
                return None;
            }
            let key = LocationKey {
                doc: scope.doc,
                address: address_of(value),
            };
            if let Some(&id) = self.target_ids.get(&key) {
                return Some(reference_node(id));
            }
        }
        match value {
            Value::Bool(true) => Some(JsonSchema::default()),
            Value::Object(obj) => {
                if let Some(reference) = self.parse_reference(obj, scope) {
                    return Some(reference);
                }
                Some(self.parse_object(obj, scope))
            }
            Value::Bool(false)
            | Value::Null
            | Value::Number(_)
            | Value::String(_)
            | Value::Array(_) => None,
        }
    }

    /// What the `$ref` or `$dynamicRef` of `obj` stands for. An unresolved
    /// `$dynamicRef` is [`Reference::Plain`]: the keywords next to it apply.
    fn reference_of(&mut self, obj: &serde_json::Map<String, Value>, scope: &Scope) -> Reference {
        let Some((text, keyword)) = reference_keyword(obj) else {
            return Reference::Plain;
        };
        let Some(location) = self.locate(text, scope.doc, scope.base.as_deref()) else {
            return match keyword {
                Keyword::Ref => self
                    .unresolved(text)
                    .map_or(Reference::Permissive, Reference::Unresolved),
                Keyword::DynamicRef => Reference::Plain,
            };
        };
        match self.settle(location) {
            Settled::Content(content) => Reference::Content(content),
            Settled::Unresolved(text) => Reference::Unresolved(text),
            Settled::Permissive => Reference::Permissive,
        }
    }

    /// A copy of `reference`, the value of a `$ref` that cannot be resolved;
    /// not made once the parsed budget is exceeded.
    fn unresolved(&mut self, reference: &str) -> Option<Arc<str>> {
        self.charge(size_of::<Arc<str>>() + ALLOCATION_OVERHEAD + reference.len())
            .then(|| Arc::from(reference))
    }

    /// The reference node for the `$ref` or `$dynamicRef` of `obj`, which
    /// stands for the schema it names whatever else `obj` holds; `None` when
    /// `obj` has no reference.
    fn parse_reference(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        scope: &Scope,
    ) -> Option<JsonSchema> {
        match self.reference_of(obj, scope) {
            Reference::Content(content) => Some(
                self.target_id(&content)
                    .map_or_else(JsonSchema::default, reference_node),
            ),
            Reference::Unresolved(text) => Some(JsonSchema {
                reference: Some(SchemaRef::Unresolved(text)),
                ..JsonSchema::default()
            }),
            Reference::Permissive => Some(JsonSchema::default()),
            Reference::Plain => None,
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Discovery
    // ──────────────────────────────────────────────────────────────────────────

    /// Walk the schema positions below the target at `location`.
    fn discover_location(&mut self, location: &Location) {
        let source = self.source(location.key.doc);
        let Some(value) = navigate(source.value(), &location.path) else {
            return;
        };
        let scope = Scope {
            doc: location.key.doc,
            base: self.enclosing_base(location),
        };
        self.discover_node(value, &scope);
    }

    /// Settle the references in `value` and below, once per object. The
    /// positions are those `parse_object` reads.
    fn discover_node(&mut self, value: &Value, scope: &Scope) {
        let Value::Object(obj) = value else {
            return;
        };
        let key = LocationKey {
            doc: scope.doc,
            address: address_of(value),
        };
        if self.walked.contains(&key)
            || self.meters.is_over()
            || !self.charge(table_entry_bytes::<LocationKey, ()>())
        {
            return;
        }
        self.walked.insert(key);
        match self.reference_of(obj, scope) {
            Reference::Content(content) => {
                self.target_id(&content);
            }
            Reference::Unresolved(_) | Reference::Permissive => {}
            Reference::Plain => {
                let (_, below) = self.enter(obj, scope);
                for child in schema_children(obj) {
                    self.discover_node(child, &below);
                }
            }
        }
    }

    /// The `$id` base that `obj` declares, and the scope for what is below it.
    fn enter(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        scope: &Scope,
    ) -> (Option<Rc<str>>, Scope) {
        let Some(raw) = declared_id(obj) else {
            return (None, scope.clone());
        };
        let base = self.resolve_id(scope.doc, obj, scope.base.as_deref(), raw);
        let below = Scope {
            doc: scope.doc,
            base: Some(Rc::clone(&base)),
        };
        (Some(base), below)
    }

    /// Follow `location` through schemas that consist of a reference to the
    /// schema that stands for them. A step costs the same however long the
    /// chain is, and every location on the chain is remembered, so a long
    /// chain is walked once.
    fn settle(&mut self, start: Location) -> Settled {
        let mut chain: Vec<LocationKey> = Vec::new();
        let mut on_chain: HashSet<LocationKey> = HashSet::new();
        let mut current = start;
        let result = loop {
            if let Some(known) = self.settled.get(&current.key) {
                break known.clone();
            }
            if self.meters.is_over() {
                break Settled::Permissive;
            }
            if !on_chain.insert(current.key) {
                break Settled::Permissive;
            }
            // The chain, its set, and the memo entry that it ends up as.
            self.charge(
                size_of::<LocationKey>()
                    + table_entry_bytes::<LocationKey, ()>()
                    + table_entry_bytes::<LocationKey, Settled>(),
            );
            chain.push(current.key);
            let source = self.source(current.key.doc);
            let keyword = match navigate(source.value(), &current.path) {
                Some(Value::Object(obj)) => reference_keyword(obj),
                _ => None,
            };
            let Some((text, keyword)) = keyword else {
                break Settled::Content(current);
            };
            let base = if text.starts_with('#') {
                None
            } else {
                self.enclosing_base(&current)
            };
            match self.locate(text, current.key.doc, base.as_deref()) {
                Some(next) => current = next,
                None => {
                    break match keyword {
                        Keyword::Ref => self
                            .unresolved(text)
                            .map_or(Settled::Permissive, Settled::Unresolved),
                        Keyword::DynamicRef => Settled::Content(current),
                    };
                }
            }
        };
        for key in chain {
            self.settled.insert(key, result.clone());
        }
        result
    }

    // ──────────────────────────────────────────────────────────────────────────
    // $ref resolution
    // ──────────────────────────────────────────────────────────────────────────

    /// Find the place that a `$ref` value names, handling both local fragment
    /// refs and remote URIs. `doc` is the document that holds the `$ref` and
    /// `base` the base URI its relative references resolve against.
    ///
    /// **Fragment refs** (start with `#`) name a place in `doc`:
    /// - `#`       → the document root
    /// - `#/a/b`   → JSON Pointer (RFC 6901)
    /// - `#name`   → named anchor (`$anchor` or `$dynamicAnchor`)
    ///
    /// **URI refs** (everything else) are resolved against `base` and checked
    /// by the SSRF guard. A URI that names `doc` itself, by its URL or by the
    /// `$id` of the root document, names a place in `doc`. Otherwise it
    /// names a place in a fetched document, which only the root document may
    /// reach and only with the remote context: the `$ref`s of a fetched
    /// document that name other documents stay unresolved (depth-1 remote
    /// resolution).
    ///
    /// Returns `None` if the ref cannot be resolved, limits are exceeded, or
    /// the place is not a schema.
    fn locate(&mut self, ref_str: &str, doc: DocId, base: Option<&str>) -> Option<Location> {
        let (uri_part, fragment) = ref_str.split_once('#').unwrap_or((ref_str, ""));
        if ref_str.starts_with('#') {
            return self.in_document(doc, fragment);
        }

        let absolute_uri = resolve_uri(base, uri_part)?;
        let normalized = validate_and_normalize_url(&absolute_uri).ok()?;
        let names_doc = self
            .documents
            .get(doc.0)
            .is_some_and(|document| document.names.contains(&normalized));
        let target_doc = if names_doc {
            doc
        } else if doc == ROOT_DOC {
            self.remote_document(&normalized)?
        } else {
            return None;
        };
        self.in_document(target_doc, fragment)
    }

    /// The fetched document at `url` (normalized), fetching it unless the
    /// cache holds it. First insertion wins in the cache, which prevents
    /// `$id`-spoofing overwrite; the dedup and breadth guard skip a URL that
    /// was already visited or beyond the limit. A document is read as it is
    /// stored, raw: only the targets that references name are parsed.
    ///
    /// The document is charged to the raw budget once, when the parse first
    /// reads it: from the bytes of the response before they are deserialized,
    /// or from the value when the cache held it. Nothing is fetched once a
    /// budget is exceeded.
    fn remote_document(&mut self, url: &str) -> Option<DocId> {
        if let Some(&known) = self.remote_documents.get(url) {
            return known;
        }
        if self.meters.is_over() {
            return None;
        }
        let ctx = self.ctx.as_deref_mut()?; // remote resolution requires a context
        let in_cache = ctx.cache.contains_document(url);
        if !in_cache {
            if !ctx.try_visit(url) {
                return None;
            }
            let document = fetch_document(url, ctx.proxy, &mut self.meters.raw).ok()?;
            ctx.cache.insert_document(url.to_string(), document);
            self.fetched.push(url.to_string());
        }
        let shared = ctx.cache.shared_document(url)?;
        // The cache is public: a document put there may not have passed the
        // depth check of a fetch.
        let charged = if in_cache {
            charge_document(&mut self.meters.raw, &shared, 1)
        } else {
            check_json_depth(&shared, 0)
        };
        let id = charged.ok().map(|()| {
            self.documents.push(Document {
                source: Source::Shared(shared),
                url: Some(url.to_string()),
                names: vec![url.to_string()],
                anchors: None,
            });
            DocId(self.documents.len() - 1)
        });
        self.remote_documents.insert(url.to_string(), id);
        id
    }

    /// The schema in `doc` that `fragment` names: the root for the empty
    /// fragment, a JSON Pointer when it starts with `/`, an anchor otherwise.
    fn in_document(&mut self, doc: DocId, fragment: &str) -> Option<Location> {
        let source = self.source(doc);
        let root = source.value();
        let (value, path) = if fragment.is_empty() {
            (root, Path::default())
        } else if fragment.starts_with('/') {
            let value = root.pointer(fragment)?;
            // Each token is an `Rc<str>` of its own, and the path is a slice
            // of them; the tokens are as long as the fragment together.
            let tokens = fragment.matches('/').count();
            let path_bytes = tokens * (size_of::<Rc<str>>() + ALLOCATION_OVERHEAD)
                + ALLOCATION_OVERHEAD
                + fragment.len();
            if !self.charge(path_bytes) {
                return None;
            }
            (value, pointer_path(fragment))
        } else {
            let parsed_meter = &mut self.meters.parsed;
            let path = self
                .documents
                .get_mut(doc.0)?
                .anchors
                .get_or_insert_with(|| index_anchors(root, parsed_meter))
                .get(fragment)
                .cloned()?;
            (navigate(root, &path)?, path)
        };
        is_schema_value(value).then(|| Location {
            key: LocationKey {
                doc,
                address: address_of(value),
            },
            path,
        })
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Keywords
    // ──────────────────────────────────────────────────────────────────────────

    /// Populate scalar/string/numeric constraint fields on `schema` from `obj`.
    fn parse_scalar_fields(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        schema: &mut JsonSchema,
    ) {
        // title / description / pattern / anchors
        schema.title = self.string_field(obj, "title");
        schema.description = self.string_field(obj, "description");
        schema.pattern = self.string_field(obj, "pattern");
        schema.anchor = self.string_field(obj, "$anchor");
        schema.dynamic_anchor = self.string_field(obj, "$dynamicAnchor");
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
        schema.const_value = obj.get("const").and_then(|value| self.copy_value(value));

        // default / examples / enum / format
        schema.default = obj.get("default").and_then(|value| self.copy_value(value));
        schema.examples = obj
            .get("examples")
            .and_then(Value::as_array)
            .and_then(|values| self.copy_values(values));
        schema.enum_values = obj
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|values| self.copy_values(values));
        schema.format = self.string_field(obj, "format");
        schema.content_encoding = self.string_field(obj, "contentEncoding");
        schema.content_media_type = self.string_field(obj, "contentMediaType");
    }

    /// Parse the contents of `contentSchema` from the schema object.
    fn parse_content_schema(
        &mut self,
        schema: &mut JsonSchema,
        obj: &serde_json::Map<String, Value>,
        scope: &Scope,
    ) {
        if let Some(cs) = obj.get("contentSchema") {
            schema.content_schema = self.parse_node(cs, scope).map(Box::new);
        }
    }

    /// Populate `properties` and `patternProperties` on `schema` from `obj`.
    fn parse_object_fields(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        scope: &Scope,
        schema: &mut JsonSchema,
    ) {
        if let Some(map) = obj.get("properties").and_then(Value::as_object) {
            let mut props = HashMap::new();
            for (k, v) in map {
                if let Some(s) = self.parse_node(v, scope)
                    && let Some(key) = self.copy_string(k)
                {
                    props.insert(key, s);
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
                if let Some(s) = self.parse_node(v, scope)
                    && let Some(key) = self.copy_string(k)
                {
                    pat_props.push((key, s));
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
        scope: &Scope,
        schema: &mut JsonSchema,
    ) {
        // prefixItems (Draft 2020-12)
        if let Some(arr) = obj.get("prefixItems").and_then(Value::as_array) {
            let mut items = Vec::new();
            for v in arr {
                if let Some(s) = self.parse_node(v, scope) {
                    items.push(s);
                }
            }
            if !items.is_empty() {
                schema.prefix_items = Some(items);
            }
        }

        // items — object form (single schema) or array form (Draft-04 tuple → prefixItems)
        match obj.get("items") {
            Some(Value::Array(arr)) if reads_tuple_items(obj) => {
                let mut items = Vec::new();
                for v in arr {
                    if let Some(s) = self.parse_node(v, scope) {
                        items.push(s);
                    }
                }
                if !items.is_empty() {
                    schema.prefix_items = Some(items);
                }
            }
            Some(v) => {
                schema.items = self.parse_node(v, scope).map(Box::new);
            }
            None => {}
        }

        // additionalItems — only relevant in Draft-04/07 tuple mode (array-form items, not prefixItems)
        if reads_additional_items(obj) {
            schema.additional_items =
                self.parse_additional_properties(obj.get("additionalItems"), scope);
        }

        schema.contains = obj
            .get("contains")
            .and_then(|v| self.parse_node(v, scope))
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
        scope: &Scope,
        schema: &mut JsonSchema,
    ) {
        schema.all_of = self.parse_schema_array(obj.get("allOf"), scope);
        schema.any_of = self.parse_schema_array(obj.get("anyOf"), scope);
        schema.one_of = self.parse_schema_array(obj.get("oneOf"), scope);
        schema.not = obj
            .get("not")
            .and_then(|v| self.parse_node(v, scope))
            .map(Box::new);

        // if / then / else (Draft-07)
        schema.if_schema = obj
            .get("if")
            .and_then(|v| self.parse_node(v, scope))
            .map(Box::new);
        schema.then_schema = obj
            .get("then")
            .and_then(|v| self.parse_node(v, scope))
            .map(Box::new);
        schema.else_schema = obj
            .get("else")
            .and_then(|v| self.parse_node(v, scope))
            .map(Box::new);
    }

    /// Populate `unevaluatedProperties` and `unevaluatedItems` on `schema`
    /// from `obj`. Extracted to keep `parse_object` short.
    fn parse_extension_fields(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        scope: &Scope,
        schema: &mut JsonSchema,
    ) {
        // unevaluatedProperties / unevaluatedItems (Draft 2019-09)
        schema.unevaluated_properties =
            self.parse_additional_properties(obj.get("unevaluatedProperties"), scope);
        schema.unevaluated_items = obj
            .get("unevaluatedItems")
            .and_then(|v| self.parse_node(v, scope))
            .map(Box::new);
    }

    /// Parse the keywords of the schema object `obj`, which is not a
    /// reference.
    fn parse_object(&mut self, obj: &serde_json::Map<String, Value>, scope: &Scope) -> JsonSchema {
        let mut schema = JsonSchema::default();

        // $id (Draft-06+) / id (Draft-04) — update base URI for sub-schemas
        let (declared, child_scope) = self.enter(obj, scope);
        schema.id = declared.and_then(|base| self.copy_string(&base));
        let scope = &child_scope;

        // type
        let type_keyword = obj.get("type");
        schema.schema_type = self
            .charge_type_strings(type_keyword)
            .then(|| parse_type(type_keyword))
            .flatten();

        self.parse_scalar_fields(obj, &mut schema);
        self.parse_content_schema(&mut schema, obj, scope);

        // required
        schema.required = obj
            .get("required")
            .and_then(Value::as_array)
            .map(|arr| self.copy_strings(arr));

        self.parse_object_fields(obj, scope, &mut schema);
        self.parse_array_fields(obj, scope, &mut schema);

        // additionalProperties
        schema.additional_properties =
            self.parse_additional_properties(obj.get("additionalProperties"), scope);

        // propertyNames
        schema.property_names = obj
            .get("propertyNames")
            .and_then(|v| self.parse_node(v, scope))
            .map(Box::new);

        // dependencies (Draft-04) / dependentRequired + dependentSchemas (Draft 2019-09)
        let (dep_req, dep_sch) = self.parse_dependencies(obj, scope);
        schema.dependent_required = dep_req;
        schema.dependent_schemas = dep_sch;

        self.parse_combinator_fields(obj, scope, &mut schema);
        self.parse_extension_fields(obj, scope, &mut schema);

        schema
    }

    /// Charge the strings of the `type` keyword at `value` to the parsed
    /// budget, before `parse_type` copies them; `false` once it is exceeded.
    fn charge_type_strings(&mut self, value: Option<&Value>) -> bool {
        match value {
            Some(Value::String(text)) => self.charge(string_bytes(text.len())),
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .all(|text| self.charge(listed_string_bytes(text.len()))),
            _ => true,
        }
    }

    /// Parse `dependencies` (Draft-04), `dependentRequired`, and `dependentSchemas`
    /// (Draft 2019-09) from a schema object, merging into a unified pair of maps.
    /// 2019-09 entries take precedence over Draft-04 `dependencies` on key collision.
    fn parse_dependencies(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        scope: &Scope,
    ) -> ParsedDependencies {
        let mut dep_req: HashMap<String, Vec<String>> = HashMap::new();
        let mut dep_sch: HashMap<String, JsonSchema> = HashMap::new();

        // Draft-04 `dependencies`
        if let Some(Value::Object(deps)) = obj.get("dependencies") {
            for (key, val) in deps {
                if let Some(arr) = val.as_array() {
                    // Array of strings → dependentRequired
                    let reqs = self.copy_strings(arr);
                    if let Some(key) = self.copy_string(key) {
                        dep_req.insert(key, reqs);
                    }
                } else if let Some(schema) = self.parse_node(val, scope)
                    && let Some(key) = self.copy_string(key)
                {
                    // Sub-schema → dependentSchemas
                    dep_sch.insert(key, schema);
                }
            }
        }

        // Draft 2019-09 `dependentRequired` — merges over Draft-04 entries
        if let Some(Value::Object(dr)) = obj.get("dependentRequired") {
            for (key, val) in dr {
                if let Some(arr) = val.as_array() {
                    let reqs = self.copy_strings(arr);
                    if let Some(key) = self.copy_string(key) {
                        dep_req.insert(key, reqs);
                    }
                }
            }
        }

        // Draft 2019-09 `dependentSchemas` — merges over Draft-04 entries
        if let Some(Value::Object(ds)) = obj.get("dependentSchemas") {
            for (key, val) in ds {
                if let Some(schema) = self.parse_node(val, scope)
                    && let Some(key) = self.copy_string(key)
                {
                    dep_sch.insert(key, schema);
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
        scope: &Scope,
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
                .parse_node(v, scope)
                .map(|s| AdditionalProperties::Schema(Box::new(s))),
        }
    }

    fn parse_schema_array(
        &mut self,
        value: Option<&Value>,
        scope: &Scope,
    ) -> Option<Vec<JsonSchema>> {
        let arr = value?.as_array()?;
        let mut schemas = Vec::new();
        for v in arr {
            if let Some(s) = self.parse_node(v, scope) {
                schemas.push(s);
            }
        }
        if schemas.is_empty() {
            None
        } else {
            Some(schemas)
        }
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
    // ParsedSchema: target table and reference resolution
    // ══════════════════════════════════════════════════════════════════════════

    fn titled(title: &str) -> JsonSchema {
        JsonSchema {
            title: Some(title.to_string()),
            ..JsonSchema::default()
        }
    }

    fn unresolved(ref_text: &str) -> JsonSchema {
        JsonSchema {
            reference: Some(SchemaRef::Unresolved(ref_text.into())),
            ..JsonSchema::default()
        }
    }

    #[test]
    fn with_targets_keeps_targets_that_are_not_reference_nodes_in_order() {
        let parsed = ParsedSchema::with_targets(
            test_support::reference_to(1),
            vec![titled("a"), titled("b")],
        )
        .expect("no target is a reference node");

        assert_eq!(parsed.resolved_root().title.as_deref(), Some("b"));
        assert_eq!(
            parsed
                .resolve(&test_support::reference_to(0))
                .title
                .as_deref(),
            Some("a")
        );
    }

    #[rstest]
    #[case::self_reference(test_support::reference_to(0))]
    #[case::reference_to_another_target(test_support::reference_to(1))]
    #[case::unresolved(unresolved("#/x"))]
    fn with_targets_rejects_a_target_that_is_a_reference_node(#[case] target: JsonSchema) {
        let result = ParsedSchema::with_targets(JsonSchema::default(), vec![titled("a"), target]);

        assert!(
            matches!(result, Err(SchemaError::ParseFailed(_))),
            "{result:?}"
        );
    }

    #[test]
    fn resolve_returns_a_schema_that_is_not_a_reference_node_itself() {
        let parsed = ParsedSchema::new(JsonSchema::default());
        let schema = titled("plain");

        assert!(std::ptr::eq(parsed.resolve(&schema), &raw const schema));
    }

    #[test]
    fn resolve_follows_a_reference_node_to_the_table_entry_in_one_step() {
        let parsed = test_support::with_targets(JsonSchema::default(), vec![titled("a")]);
        let node = test_support::reference_to(0);

        let target = parsed.resolve(&node);

        assert_eq!(target.title.as_deref(), Some("a"));
        assert!(std::ptr::eq(parsed.resolve(target), target));
    }

    #[test]
    fn resolve_ignores_the_own_fields_of_a_reference_node() {
        let parsed = test_support::with_targets(JsonSchema::default(), vec![JsonSchema::default()]);
        let node = JsonSchema {
            title: Some("own".to_string()),
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..test_support::reference_to(0)
        };

        let target = parsed.resolve(&node);

        assert!(target.title.is_none());
        assert!(target.schema_type.is_none());
    }

    #[rstest]
    #[case::past_the_table(1)]
    #[case::far_past_the_table(usize::MAX)]
    fn resolve_of_a_target_id_outside_the_table_is_the_node_itself(#[case] index: usize) {
        let parsed = test_support::with_targets(JsonSchema::default(), vec![titled("a")]);
        let node = test_support::reference_to(index);

        assert!(std::ptr::eq(parsed.resolve(&node), &raw const node));
    }

    #[test]
    fn resolve_of_a_reference_node_without_any_table_is_the_node_itself() {
        let parsed = ParsedSchema::new(JsonSchema::default());
        let node = test_support::reference_to(0);

        assert!(std::ptr::eq(parsed.resolve(&node), &raw const node));
    }

    #[test]
    fn resolve_of_an_unresolved_reference_is_the_node_itself() {
        let parsed = test_support::with_targets(JsonSchema::default(), vec![titled("a")]);
        let node = unresolved("#/missing");

        assert!(std::ptr::eq(parsed.resolve(&node), &raw const node));
    }

    #[test]
    fn a_default_schema_is_not_a_reference_node() {
        assert!(JsonSchema::default().reference.is_none());
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

    #[rstest]
    #[case::null(Value::Null)]
    #[case::string(Value::String("not a schema".into()))]
    #[case::number(json!(1))]
    #[case::array(json!([{"type": "string"}]))]
    #[case::boolean_false(Value::Bool(false))]
    fn parse_schema_rejects_a_root_that_is_not_a_schema_object(#[case] root: Value) {
        let result = parse_schema(&root);

        assert!(
            matches!(result, Err(SchemaError::ParseFailed(_))),
            "got {result:?}"
        );
    }

    #[test]
    fn should_return_none_for_null_input() {
        assert!(parse_schema(&Value::Null).is_err());
    }

    #[test]
    fn should_return_none_for_non_object_json() {
        assert!(parse_schema(&Value::String("not a schema".into())).is_err());
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
        assert!(parse_schema(&Value::Bool(false)).is_err());
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
        let parsed = parse_schema(&v).expect("should resolve");
        let s = parsed.resolved_root();
        assert_eq!(schema_type_str(s), Some("string"));
    }

    #[test]
    fn should_resolve_ref_into_defs_container() {
        let v = json!({
            "$ref": "#/$defs/MyType",
            "$defs": {"MyType": {"type": "string"}}
        });
        let parsed = parse_schema(&v).expect("should resolve");
        let s = parsed.resolved_root();
        assert_eq!(schema_type_str(s), Some("string"));
        assert!(s.reference.is_none());
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
        let parsed = parse_schema(&v).expect("should parse");
        let props = parsed
            .resolved_root()
            .properties
            .as_ref()
            .expect("should have properties");
        let foo = props.get("foo").expect("should have foo");
        assert_eq!(schema_type_str(parsed.resolve(foo)), Some("integer"));
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

    // ── Parsed once and shared ───────────────────────────────────────────────

    /// The id of the target that the reference node `schema` refers to.
    fn target_of(schema: &JsonSchema) -> Option<usize> {
        match schema.reference {
            Some(SchemaRef::Target(id)) => Some(id.0),
            Some(SchemaRef::Unresolved(_)) | None => None,
        }
    }

    /// The property `key` of `schema`, as parsed: a reference node stays one.
    fn property<'a>(schema: &'a JsonSchema, key: &str) -> &'a JsonSchema {
        schema
            .properties
            .as_ref()
            .and_then(|properties| properties.get(key))
            .unwrap_or_else(|| panic!("property {key}"))
    }

    /// The number of schema nodes in a parse: the root and every target, with
    /// their children.
    fn node_count(parsed: &ParsedSchema) -> usize {
        debug_of(parsed).matches("JsonSchema {").count()
    }

    fn json_value_count(value: &Value) -> usize {
        1 + match value {
            Value::Object(map) => map.values().map(json_value_count).sum(),
            Value::Array(items) => items.iter().map(json_value_count).sum(),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 0,
        }
    }

    /// A parse is well formed when every reference node refers to a target
    /// that exists and no target is a reference node.
    fn assert_well_formed(parsed: &ParsedSchema) {
        let referenced: Vec<usize> = debug_of(parsed)
            .split("Target(TargetId(")
            .skip(1)
            .filter_map(|rest| rest.split(')').next()?.parse().ok())
            .collect();
        assert!(
            referenced.iter().all(|&id| id < parsed.targets.len()),
            "a reference to a target outside the table: {referenced:?}"
        );
        assert!(
            parsed.targets.iter().all(|t| t.reference.is_none()),
            "a target is a reference node"
        );
    }

    #[test]
    fn node_count_counts_every_schema_node_of_a_parse() {
        let parsed = parse_schema(&json!({
            "properties": { "a": { "type": "string" }, "b": { "items": { "type": "integer" } } }
        }))
        .expect("parses");

        assert_eq!(node_count(&parsed), 4);
    }

    #[test]
    fn fan_out_self_reference_parses_one_shared_target() {
        let reference = json!({ "$ref": "#/definitions/N" });
        let v = json!({
            "$ref": "#/definitions/N",
            "definitions": { "N": { "properties": {
                "a": reference, "b": reference, "c": reference, "d": reference
            } } }
        });

        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.targets.len(), 1);
        let shared = target_of(&parsed.root).expect("the root is a reference");
        for key in ["a", "b", "c", "d"] {
            let node = property(parsed.resolved_root(), key);
            assert_eq!(target_of(node), Some(shared), "property {key}");
        }
        assert!(node_count(&parsed) <= json_value_count(&v));
    }

    #[test]
    fn layered_fan_out_parses_in_size_linear_in_the_source() {
        let layers = 30;
        let mut definitions = serde_json::Map::new();
        for i in 0..layers {
            let next = json!({ "$ref": format!("#/definitions/L{}", i + 1) });
            definitions.insert(
                format!("L{i}"),
                json!({ "properties": { "a": next, "b": next, "c": next, "d": next } }),
            );
        }
        definitions.insert(format!("L{layers}"), json!({ "type": "string" }));
        let v = json!({ "$ref": "#/definitions/L0", "definitions": definitions });

        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.targets.len(), layers + 1);
        assert!(node_count(&parsed) <= json_value_count(&v));
    }

    // Each case names one place that two `$ref`s reach by different routes.
    #[rstest]
    #[case::pointer_twice(
        json!({ "properties": {
            "a": { "$ref": "#/definitions/A" }, "b": { "$ref": "#/definitions/A" } },
            "definitions": { "A": { "type": "string" } } })
    )]
    #[case::pointer_and_anchor(
        json!({ "properties": {
            "a": { "$ref": "#/definitions/A" }, "b": { "$ref": "#anchor" } },
            "definitions": { "A": { "$anchor": "anchor", "type": "string" } } })
    )]
    #[case::pointer_and_dynamic_anchor(
        json!({ "properties": {
            "a": { "$ref": "#/definitions/A" }, "b": { "$dynamicRef": "#anchor" } },
            "definitions": { "A": { "$dynamicAnchor": "anchor", "type": "string" } } })
    )]
    #[case::ref_and_dynamic_ref_to_one_anchor(
        json!({ "properties": {
            "a": { "$ref": "#anchor" }, "b": { "$dynamicRef": "#anchor" } },
            "definitions": { "A": { "$anchor": "anchor", "type": "string" } } })
    )]
    #[case::direct_and_through_an_alias_chain(
        json!({ "properties": {
            "a": { "$ref": "#/definitions/A" }, "b": { "$ref": "#/definitions/Alias" } },
            "definitions": { "A": { "type": "string" },
                             "Alias": { "$ref": "#/definitions/Alias2" },
                             "Alias2": { "$ref": "#/definitions/A" } } })
    )]
    #[case::root_by_hash_and_by_empty_fragment_url(
        json!({ "$id": "https://example.com/s.json#", "type": "object", "properties": {
            "a": { "$ref": "#" }, "b": { "$ref": "https://example.com/s.json" },
            "c": { "$ref": "https://example.com/s.json#" }, "d": { "$ref": "s.json" } } })
    )]
    fn one_location_named_in_two_ways_is_one_target(#[case] v: Value) {
        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.targets.len(), 1);
        let root = parsed.resolved_root();
        let ids: Vec<_> = ["a", "b", "c", "d"]
            .iter()
            .filter(|key| {
                root.properties
                    .as_ref()
                    .is_some_and(|p| p.contains_key(**key))
            })
            .map(|key| target_of(property(root, key)))
            .collect();
        assert!(ids.len() >= 2 && ids.iter().all(|id| id.is_some() && *id == ids[0]));
    }

    #[test]
    fn recursive_schema_shares_one_target() {
        let v = json!({
            "type": "object",
            "properties": { "p": { "$ref": "#" }, "q": { "$ref": "#" } }
        });

        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.targets.len(), 1);
        let root = parsed.resolved_root();
        assert_eq!(
            target_of(property(root, "p")),
            target_of(property(root, "q"))
        );
        assert!(target_of(property(root, "p")).is_some());
        assert_eq!(
            schema_type_str(parsed.resolve(property(root, "p"))),
            Some("object")
        );
        assert_eq!(target_of(&parsed.root), target_of(property(root, "p")));
    }

    #[test]
    fn ref_to_root_hash_resolves_to_root_content() {
        let v = json!({
            "properties": { "child": { "$ref": "#" } },
            "type": "object"
        });

        let parsed = parse_schema(&v).expect("should parse");

        let child = property(parsed.resolved_root(), "child");
        assert_eq!(schema_type_str(parsed.resolve(child)), Some("object"));
        assert_eq!(schema_type_str(parsed.resolved_root()), Some("object"));
    }

    #[test]
    fn ref_into_the_interior_of_the_tree_terminates() {
        let v = json!({ "properties": { "a": { "properties": {
            "b": { "$ref": "#/properties/a" }, "c": { "type": "string" } } } } });

        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.targets.len(), 1);
        // `a` is the target, so it sits in the root as a reference node.
        let a = property(parsed.resolved_root(), "a");
        assert!(target_of(a).is_some());
        let b = property(parsed.resolve(a), "b");
        assert_eq!(target_of(b), target_of(a));
        assert_eq!(
            schema_type_str(property(parsed.resolve(b), "c")),
            Some("string")
        );
        assert!(node_count(&parsed) <= json_value_count(&v));
    }

    // A target is built once, however deep inside other schemas it sits: where
    // it sits it is a reference node, so the parse has no more nodes than the
    // source has JSON values.
    #[rstest]
    #[case::one_level(1)]
    #[case::ten_levels(10)]
    #[case::twenty_levels(20)]
    fn targets_nested_in_each_other_are_parsed_once(#[case] levels: usize) {
        let leaves: serde_json::Map<String, Value> = (0..1000)
            .map(|i| (format!("leaf{i}"), json!({ "type": "string" })))
            .collect();
        let nest = (0..levels).fold(
            json!({ "properties": leaves }),
            |inner, _| json!({ "properties": { "n": inner } }),
        );
        // The outermost level is `#/x-nest`; each reference names one level.
        let references: serde_json::Map<String, Value> = (0..levels)
            .map(|level| {
                let path = format!("#/x-nest{}", "/properties/n".repeat(level));
                (format!("r{level}"), json!({ "$ref": path }))
            })
            .collect();
        let v = json!({ "x-nest": nest, "properties": references });

        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.targets.len(), levels);
        assert!(
            node_count(&parsed) <= json_value_count(&v),
            "{} nodes for {} JSON values",
            node_count(&parsed),
            json_value_count(&v)
        );
    }

    // The same holds for the targets of a fetched document.
    #[test]
    fn targets_nested_in_a_fetched_document_are_parsed_once() {
        let url = "https://example.com/nest.json";
        let levels = 10;
        let leaves: serde_json::Map<String, Value> = (0..200)
            .map(|i| (format!("leaf{i}"), json!({ "type": "string" })))
            .collect();
        let nest = (0..levels).fold(
            json!({ "properties": leaves }),
            |inner, _| json!({ "properties": { "n": inner } }),
        );
        let document = json!({ "x-nest": nest });
        let references: serde_json::Map<String, Value> = (0..levels)
            .map(|level| {
                let path = format!("{url}#/x-nest{}", "/properties/n".repeat(level));
                (format!("r{level}"), json!({ "$ref": path }))
            })
            .collect();
        let root = json!({ "properties": references });
        let mut cache = cache_with(&[(url, document.clone())]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let parsed = parse_with(&root, None, Some(&mut ctx)).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.targets.len(), levels);
        assert!(node_count(&parsed) <= json_value_count(&root) + json_value_count(&document));
    }

    // Discovery reads the same positions as the build, also where `prefixItems`
    // and the array form of `items` meet.
    #[rstest]
    #[case::prefix_items_without_a_schema(json!({ "prefixItems": [false] }))]
    #[case::prefix_items_not_an_array(json!({ "prefixItems": 5 }))]
    #[case::prefix_items_empty(json!({ "prefixItems": [] }))]
    #[case::no_prefix_items(json!({}))]
    fn tuple_items_are_discovered_where_the_build_reads_them(#[case] header: Value) {
        let mut b = json!({ "items": [{ "$ref": "#/definitions/A/properties/p" }] });
        b.as_object_mut()
            .expect("object")
            .extend(header.as_object().expect("object").clone());
        let v = json!({
            "properties": { "a": { "$ref": "#/definitions/A" }, "b": { "$ref": "#/definitions/B" } },
            "definitions": { "A": { "properties": { "p": { "type": "string" } } }, "B": b }
        });

        let parsed = parse_schema(&v).expect("parses");

        let a = parsed.resolve(property(parsed.resolved_root(), "a"));
        assert!(target_of(property(a, "p")).is_some(), "built inline");
        assert!(node_count(&parsed) <= json_value_count(&v));
    }

    #[rstest]
    #[case::prefix_items_empty(json!({ "prefixItems": [] }))]
    #[case::prefix_items_not_an_array(json!({ "prefixItems": 5 }))]
    #[case::no_items_array(json!({ "items": { "type": "string" } }))]
    fn additional_items_that_the_build_does_not_read_are_not_resolved(#[case] header: Value) {
        let url = "https://schema.invalid/never.json";
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, Some(CLOSED_PROXY));
        let mut value = json!({ "items": [], "additionalItems": { "$ref": url } });
        value
            .as_object_mut()
            .expect("object")
            .extend(header.as_object().expect("object").clone());

        parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert!(
            ctx.visited.is_empty(),
            "a position the build skips was resolved"
        );
    }

    #[test]
    fn target_inside_another_target_is_a_reference_node_there() {
        let v = json!({
            "properties": { "a": { "$ref": "#/definitions/A" }, "b": { "$ref": "#/definitions/A/properties/p" } },
            "definitions": { "A": { "properties": { "p": { "type": "string" } } } }
        });

        let parsed = parse_schema(&v).expect("parses");

        let root = parsed.resolved_root();
        let a = parsed.resolve(property(root, "a"));
        assert_eq!(target_of(property(a, "p")), target_of(property(root, "b")));
        assert_eq!(parsed.targets.len(), 2);
        assert!(node_count(&parsed) <= json_value_count(&v));
    }

    // Discovery must walk every position that `parse_object` reads: a target
    // found only by building another target would be parsed inline there too.
    #[test]
    fn every_subschema_keyword_is_discovered_before_the_targets_are_built() {
        for keyword in SUBSCHEMA_KEYWORDS {
            let value = json!({
                "properties": { "a": { "$ref": "#/definitions/A" }, "b": { "$ref": "#/definitions/B" } },
                "definitions": {
                    "A": { "properties": { "p": { "type": "string" } } },
                    "B": wrap(keyword, &json!({ "$ref": "#/definitions/A/properties/p" }))
                }
            });

            let parsed = parse_schema(&value).expect("parses");

            let root = parsed.resolved_root();
            let a = parsed.resolve(property(root, "a"));
            assert!(
                target_of(property(a, "p")).is_some(),
                "keyword {keyword}: the nested target was built inline"
            );
        }
    }

    #[test]
    fn ref_into_the_siblings_of_a_ref_still_resolves_by_syntax() {
        let v = json!({
            "$ref": "#/definitions/A/properties/x",
            "definitions": {
                "A": { "$ref": "#/definitions/B", "properties": { "x": { "type": "string" } } },
                "B": { "type": "integer" }
            }
        });

        let parsed = parse_schema(&v).expect("parses");

        assert_eq!(schema_type_str(parsed.resolved_root()), Some("string"));
    }

    #[test]
    fn a_pointer_does_not_traverse_through_a_ref() {
        let v = json!({
            "$ref": "#/definitions/A/properties/x",
            "definitions": { "A": { "$ref": "#/definitions/B" }, "B": { "properties": {} } }
        });

        let parsed = parse_schema(&v).expect("parses");

        assert_eq!(
            unresolved_ref(&parsed.root),
            Some("#/definitions/A/properties/x")
        );
    }

    // ── Chains of references and cycles ──────────────────────────────────────

    fn alias_chain(hops: usize, end: Value) -> Value {
        let mut chain = serde_json::Map::new();
        for i in 0..hops {
            chain.insert(
                format!("R{i}"),
                json!({ "$ref": format!("#/x-chain/R{}", i + 1) }),
            );
        }
        chain.insert(format!("R{hops}"), end);
        json!({ "$ref": "#/x-chain/R0", "x-chain": chain })
    }

    // A chain of references resolves in one step, however long it is; the
    // 15 and 16 hop chains used to be the longest that fit a fixed depth.
    #[rstest]
    #[case::one_hop(1)]
    #[case::formerly_the_longest(15)]
    #[case::formerly_cut(16)]
    #[case::beyond_any_old_limit(100)]
    fn alias_chain_resolves_in_one_step_to_its_content(#[case] hops: usize) {
        let v = alias_chain(hops, json!({ "description": "LEAF" }));

        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        assert_eq!(parsed.resolved_root().description.as_deref(), Some("LEAF"));
        assert!(parsed.resolved_root().reference.is_none());
        assert_eq!(parsed.targets.len(), 1);
    }

    #[test]
    fn alias_chain_that_ends_in_true_resolves_to_a_permissive_schema() {
        let parsed = parse_schema(&alias_chain(3, json!(true))).expect("parses");

        assert!(target_of(&parsed.root).is_some());
        assert!(parsed.resolved_root().schema_type.is_none());
    }

    #[rstest]
    #[case::boolean_false(json!(false))]
    #[case::number(json!(5))]
    #[case::string(json!("s"))]
    #[case::null(Value::Null)]
    #[case::array(json!([]))]
    fn ref_to_a_value_that_is_no_schema_stays_unresolved(#[case] end: Value) {
        let v = alias_chain(2, end);

        let parsed = parse_schema(&v).expect("parses");

        // The text is the one of the reference that failed, the last of the chain.
        assert_eq!(unresolved_ref(&parsed.root), Some("#/x-chain/R2"));
    }

    // A chain of `$ref`-only schemas that comes back to itself matches
    // anything, whatever sits next to the references.
    #[rstest]
    #[case::itself(json!({ "$ref": "#/definitions/A", "definitions": {
        "A": { "$ref": "#/definitions/A" } } }))]
    #[case::two_nodes(json!({ "$ref": "#/definitions/A", "definitions": {
        "A": { "$ref": "#/definitions/B" }, "B": { "$ref": "#/definitions/A" } } }))]
    #[case::three_nodes(json!({ "$ref": "#/definitions/A", "definitions": {
        "A": { "$ref": "#/definitions/B" }, "B": { "$ref": "#/definitions/C" },
        "C": { "$ref": "#/definitions/A" } } }))]
    #[case::root_to_itself(json!({ "$ref": "#" }))]
    #[case::root_through_a_definition(json!({ "$ref": "#/definitions/A", "definitions": {
        "A": { "$ref": "#" } } }))]
    #[case::cycle_members_with_ignored_siblings(json!({ "$ref": "#/definitions/A",
        "definitions": { "A": { "$ref": "#/definitions/B", "type": "string" },
                         "B": { "$ref": "#/definitions/A" } } }))]
    #[case::leading_into_a_cycle(json!({ "$ref": "#/definitions/X", "definitions": {
        "X": { "$ref": "#/definitions/A" }, "A": { "$ref": "#/definitions/B" },
        "B": { "$ref": "#/definitions/A" } } }))]
    #[case::entered_through_a_dynamic_ref(json!({ "$dynamicRef": "#/definitions/A", "type": "string",
        "definitions": { "A": { "$ref": "#/definitions/A" } } }))]
    fn pure_ref_cycle_is_permissive(#[case] v: Value) {
        let parsed = parse_schema(&v).expect("parses");

        assert_well_formed(&parsed);
        let root = parsed.resolved_root();
        assert!(root.reference.is_none());
        assert!(root.schema_type.is_none());
        assert!(root.properties.is_none());
    }

    #[test]
    fn alias_chain_that_ends_in_a_missing_pointer_stays_unresolved_with_that_text() {
        let v = json!({
            "$ref": "#/x/A", "x": { "A": { "$ref": "#/x/B" }, "B": { "$ref": "#/x/Missing" } }
        });

        let parsed = parse_schema(&v).expect("parses");

        assert_eq!(unresolved_ref(&parsed.root), Some("#/x/Missing"));
    }

    #[rstest]
    #[case::ref_keyword("$ref")]
    #[case::dynamic_ref_keyword("$dynamicRef")]
    fn alias_chain_through_anchors_resolves(#[case] keyword: &str) {
        let v = json!({
            keyword: "#first",
            "x": { "$anchor": "first", "$ref": "#second" },
            "y": { "$anchor": "second", "$dynamicRef": "#/z" },
            "z": { "type": "string" }
        });

        let parsed = parse_schema(&v).expect("parses");

        assert_eq!(schema_type_str(parsed.resolved_root()), Some("string"));
        assert_eq!(parsed.targets.len(), 1);
    }

    // The empty reference is the document's own URL: the document with a
    // base, nothing without one.
    #[test]
    fn empty_ref_names_the_document_when_it_has_a_url_and_is_unresolved_otherwise() {
        let value = json!({ "type": "object", "properties": { "p": { "$ref": "" } } });
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);

        let with_url = parse_with(&value, Some(ROOT_URL), Some(&mut ctx)).expect("parses");
        let without_url = parse_schema(&value).expect("parses");

        let p = property(with_url.resolved_root(), "p");
        assert_eq!(schema_type_str(with_url.resolve(p)), Some("object"));
        assert_eq!(
            unresolved_ref(property(without_url.resolved_root(), "p")),
            Some("")
        );
    }

    #[test]
    fn pointer_through_the_documents_own_url_names_the_same_target_as_a_local_pointer() {
        let own = "https://schema.invalid/own.json";
        let document = json!({
            "properties": {
                "a": { "$ref": "#/definitions/D" },
                "b": { "$ref": format!("{own}#/definitions/D") }
            },
            "definitions": { "D": { "type": "integer" } }
        });
        let mut cache = cache_with(&[(own, document)]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let (_, parsed) = fetch_schema_raw(own, None, Some(&mut ctx)).expect("parses");

        let root = parsed.resolved_root();
        assert_eq!(
            target_of(property(root, "a")),
            target_of(property(root, "b"))
        );
        assert_eq!(parsed.targets.len(), 1);
        assert!(ctx.visited.is_empty());
    }

    // The text of the failing reference is stored once, however long the chain
    // and however many places reach it.
    #[test]
    fn unresolved_text_of_a_long_chain_is_shared_by_every_place_that_reaches_it() {
        let count = 2_000;
        let missing = format!("#/definitions/{}", "x".repeat(256 * 1024));
        let mut definitions = serde_json::Map::new();
        let mut properties = serde_json::Map::new();
        for i in 0..count {
            definitions.insert(
                format!("d{i}"),
                json!({ "$ref": format!("#/definitions/d{}", i + 1) }),
            );
            properties.insert(
                format!("p{i}"),
                json!({ "$ref": format!("#/definitions/d{i}") }),
            );
        }
        definitions.insert(format!("d{count}"), json!({ "$ref": missing }));
        let value = json!({ "properties": properties, "definitions": definitions });

        let parsed = parse_schema(&value).expect("parses");

        let texts: Vec<&Arc<str>> = parsed
            .resolved_root()
            .properties
            .as_ref()
            .expect("properties")
            .values()
            .filter_map(|p| match &p.reference {
                Some(SchemaRef::Unresolved(text)) => Some(text),
                Some(SchemaRef::Target(_)) | None => None,
            })
            .collect();
        assert_eq!(texts.len(), count);
        assert_eq!(&**texts[0], missing);
        assert!(texts.iter().all(|text| Arc::ptr_eq(text, texts[0])));
    }

    // ── Scale ────────────────────────────────────────────────────────────────

    #[test]
    fn ten_thousand_alias_chain_parses_on_a_small_stack() {
        test_support::on_a_small_stack(|| {
            let hops = 10_000;
            let v = alias_chain(hops, json!({ "type": "string" }));

            let parsed = parse_schema(&v).expect("parses");

            assert_eq!(schema_type_str(parsed.resolved_root()), Some("string"));
            assert_eq!(parsed.targets.len(), 1);
        });
    }

    #[test]
    fn ten_thousand_alias_chain_that_comes_back_to_itself_is_permissive() {
        test_support::on_a_small_stack(|| {
            let hops = 10_000;
            let mut v = alias_chain(hops, json!({ "$ref": "#/x-chain/R0" }));
            v["$ref"] = json!("#/x-chain/R5000");

            let parsed = parse_schema(&v).expect("parses");

            assert!(parsed.resolved_root().schema_type.is_none());
            assert!(parsed.resolved_root().reference.is_none());
        });
    }

    #[test]
    fn ten_thousand_aliases_all_used_are_walked_once() {
        test_support::on_a_small_stack(|| {
            let hops = 10_000;
            let mut v = alias_chain(hops, json!({ "type": "string" }));
            let mut properties = serde_json::Map::new();
            for i in 0..hops {
                properties.insert(
                    format!("p{i}"),
                    json!({ "$ref": format!("#/x-chain/R{i}") }),
                );
            }
            v["properties"] = Value::Object(properties);

            let parsed = parse_schema(&v).expect("parses");

            assert_eq!(parsed.targets.len(), 1);
        });
    }

    #[test]
    fn ten_thousand_distinct_targets_parse_on_a_small_stack() {
        test_support::on_a_small_stack(|| {
            let length = 10_000;
            let mut definitions = serde_json::Map::new();
            for i in 0..length {
                definitions.insert(
                    format!("d{i}"),
                    json!({ "type": "object", "properties": {
                        "next": { "$ref": format!("#/definitions/d{}", i + 1) } } }),
                );
            }
            definitions.insert(format!("d{length}"), json!({ "type": "string" }));
            let v = json!({ "$ref": "#/definitions/d0", "definitions": definitions });

            let parsed = parse_schema(&v).expect("parses");

            assert_well_formed(&parsed);
            assert_eq!(parsed.targets.len(), length + 1);
            assert!(node_count(&parsed) <= json_value_count(&v));
            let mut hops = 0;
            let mut node = parsed.resolved_root();
            while let Some(next) = node.properties.as_ref().and_then(|p| p.get("next")) {
                node = parsed.resolve(next);
                hops += 1;
            }
            assert_eq!(hops, length);
            assert_eq!(schema_type_str(node), Some("string"));
        });
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
        cache.insert(url.to_string(), Arc::clone(&schema));

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
            Arc::new(schema_a),
        );
        cache.insert(
            "https://example.com/schema.json".to_string(),
            Arc::new(schema_b),
        );

        let result = cache
            .get("https://example.com/schema.json")
            .expect("should be cached");
        assert_eq!(result.root.description.as_deref(), Some("first"));
    }

    fn parsed_with_description(description: &str) -> Arc<ParsedSchema> {
        Arc::new(ParsedSchema::new(JsonSchema {
            description: Some(description.to_string()),
            ..JsonSchema::default()
        }))
    }

    #[test]
    fn empty_cache_misses_in_both_maps() {
        let cache = SchemaCache::new();
        let url = "https://example.com/schema.json";

        assert!(cache.get(url).is_none());
        assert!(cache.get_document(url).is_none());
        assert!(!cache.contains_document(url));
    }

    #[test]
    fn documents_and_parsed_schemas_are_stored_independently() {
        let url = "https://example.com/schema.json";
        let mut documents_only = SchemaCache::new();
        documents_only.insert_document(url.to_string(), json!({ "type": "string" }));
        let mut schemas_only = SchemaCache::new();
        schemas_only.insert(url.to_string(), parsed_with_description("x"));

        assert!(documents_only.get(url).is_none());
        assert_eq!(
            documents_only.get_document(url),
            Some(&json!({ "type": "string" }))
        );
        assert!(schemas_only.get_document(url).is_none());
        assert!(!schemas_only.contains_document(url));
    }

    // The document map keys by fetch URL and keeps the first document, so a
    // fetched document cannot replace another URL's entry by claiming its URL
    // through `$id`.
    #[test]
    fn dollar_id_spoofing_cannot_overwrite_document_entry() {
        let url = "https://json-schema.org/draft/2020-12/schema";
        let mut cache = SchemaCache::new();
        cache.insert_document(url.to_string(), json!({ "description": "legitimate" }));

        cache.insert_document(
            url.to_string(),
            json!({ "$id": url, "description": "malicious" }),
        );

        assert_eq!(
            cache.get_document(url),
            Some(&json!({ "description": "legitimate" }))
        );
    }

    #[test]
    fn merge_into_an_empty_cache_takes_both_maps() {
        let url = "https://example.com/schema.json";
        let schema = parsed_with_description("x");
        let mut other = SchemaCache::new();
        other.insert_document(url.to_string(), json!({ "type": "string" }));
        other.insert(url.to_string(), Arc::clone(&schema));
        let mut cache = SchemaCache::new();

        cache.merge(other);

        assert_eq!(cache.get_document(url), Some(&json!({ "type": "string" })));
        assert!(Arc::ptr_eq(cache.get(url).expect("merged"), &schema));
    }

    #[test]
    fn merge_keeps_existing_entries_first_wins() {
        let url = "https://example.com/schema.json";
        let mut cache = SchemaCache::new();
        cache.insert_document(url.to_string(), json!({ "description": "first" }));
        cache.insert(url.to_string(), parsed_with_description("first"));
        let mut other = SchemaCache::new();
        other.insert_document(url.to_string(), json!({ "description": "second" }));
        other.insert(url.to_string(), parsed_with_description("second"));

        cache.merge(other);

        assert_eq!(
            cache.get_document(url),
            Some(&json!({ "description": "first" }))
        );
        assert_eq!(
            cache.get(url).expect("kept").root.description.as_deref(),
            Some("first")
        );
    }

    #[test]
    fn merge_unions_entries_across_maps() {
        let url = "https://example.com/schema.json";
        let mut cache = SchemaCache::new();
        cache.insert_document(url.to_string(), json!({ "type": "string" }));
        let mut other = SchemaCache::new();
        other.insert(url.to_string(), parsed_with_description("x"));

        cache.merge(other);

        assert!(cache.contains_document(url));
        assert!(cache.get(url).is_some());
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

    // Sec-7: parse_schema rejects a deeply nested schema
    #[rstest]
    #[case::sixty_levels(60)]
    #[case::one_hundred_levels(100)]
    fn parse_schema_rejects_a_schema_nested_beyond_the_json_depth_limit(#[case] levels: usize) {
        let mut v = json!({"type": "string"});
        for _ in 0..levels {
            v = json!({"type": "object", "properties": {"x": v}});
        }

        let result = parse_schema(&v);

        assert!(
            matches!(result, Err(SchemaError::TooDeep)),
            "got {result:?}"
        );
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
            result.is_ok(),
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
    fn ref_without_hash_prefix_yields_unresolved_reference_only() {
        // $ref without '#' prefix cannot be resolved locally — returns a reference node
        let v = json!({"$ref": "http://example.com/other-schema.json"});
        let parsed = parse_schema(&v).expect("a reference that does not resolve still parses");
        assert_eq!(
            unresolved_ref(&parsed.root),
            Some("http://example.com/other-schema.json")
        );
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
        let parsed = parse_schema(&value).unwrap();
        let foo = parsed.root.properties.as_ref().unwrap();
        let foo_schema = parsed.resolve(foo.get("foo").unwrap());
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
        let parsed = parse_schema(&value).unwrap();
        let bar_schema = parsed.root.properties.as_ref().unwrap();
        let bar = parsed.resolve(bar_schema.get("bar").unwrap());
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
        let parsed = parse_schema(&value).unwrap();
        let val_schema = parsed.root.properties.as_ref().unwrap();
        let val = parsed.resolve(val_schema.get("val").unwrap());
        assert_eq!(
            val.schema_type,
            Some(SchemaType::Single("boolean".to_string()))
        );
    }

    // Anchor-4: anchor not found — ref unresolved, schema is an unresolved reference node without a type
    #[test]
    fn ref_returns_schema_with_unresolved_reference_when_anchor_not_found() {
        let value = json!({ "$ref": "#nonexistent" });
        let schema = parse_schema(&value).unwrap().root;
        assert_eq!(unresolved_ref(&schema), Some("#nonexistent"));
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
        let parsed = parse_schema(&value).unwrap();
        let x = parsed.root.properties.as_ref().unwrap();
        let x_schema = parsed.resolve(x.get("x").unwrap());
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
        let parsed = parse_schema(&value).unwrap();
        let name = parsed.root.properties.as_ref().unwrap();
        let name_schema = parsed.resolve(name.get("name").unwrap());
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
        let parsed = parse_with(&value, Some("https://example.com/root.json"), None).unwrap();
        let schema = parsed.resolved_root();
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
        // The $ref falls back to an unresolved reference node (no remote traversal).
        let mut ctx = ParseContext::new(&mut cache, None);
        let parsed = parse_with(&value, None, Some(&mut ctx)).unwrap();
        let schema = parsed.resolved_root();
        // Remote fetch was blocked — schema is an unresolved reference node without sub-schema content.
        assert_eq!(unresolved_ref(schema), Some("http://127.0.0.1/evil.json"));
        // Nothing was added to the cache (fetch never happened).
        assert!(!cache.contains_document("http://127.0.0.1/evil.json"));
    }

    // Sec-R2: relative $ref is resolved against base URI before SSRF guard runs.
    // The resolved URL must be validated — a relative ref resolving to loopback is blocked.
    #[test]
    fn relative_ref_resolved_against_base_uri_before_ssrf_check() {
        // "evil.json" relative to "http://127.0.0.1/" resolves to "http://127.0.0.1/evil.json"
        // which is blocked by SSRF.
        let value = json!({ "$ref": "evil.json" });
        let mut cache = SchemaCache::new();
        let parsed = parse_with(
            &value,
            Some("http://127.0.0.1/"),
            Some(&mut ParseContext::new(&mut cache, None)),
        )
        .unwrap();
        let schema = parsed.resolved_root();
        // Blocked — unresolved reference kept, no cached fetch.
        assert_eq!(unresolved_ref(schema), Some("evil.json"));
        assert!(!cache.contains_document("http://127.0.0.1/evil.json"));
    }

    // Sec-R3: circular remote refs (A → B → A) are broken by the visited-URL dedup.
    #[test]
    fn circular_remote_refs_are_deduplicated() {
        // Pre-populate cache with schema A that has a $ref to schema B,
        // and schema B that has a $ref back to schema A.
        let json_a = json!({ "$ref": "https://example.com/b.json" });
        let json_b = json!({ "$ref": "https://example.com/a.json" });

        let mut cache = SchemaCache::new();
        cache.insert_document("https://example.com/a.json".to_string(), json_a);
        cache.insert_document("https://example.com/b.json".to_string(), json_b);

        // Resolving A should terminate — the cycle is broken when B tries to
        // re-visit A (already in visited set).
        let mut ctx = ParseContext::new(&mut cache, None);
        let result = resolve_with("https://example.com/a.json", None, Some(&mut ctx));
        // Must terminate; result may be None or a partial schema.
        let _ = result;
    }

    // A fetched document resolves no references to other documents, so a cycle
    // between remote documents ends at the first hop: the document's own `$ref`
    // stays unresolved.
    #[test]
    fn circular_remote_refs_end_at_the_first_hop() {
        let url_a = "https://example.com/a.json";
        let url_b = "https://example.com/b.json";
        let json_a = json!({ "$ref": url_b });
        let mut cache = cache_with(&[(url_a, json_a), (url_b, json!({ "$ref": url_a }))]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let result = resolve_with(url_a, None, Some(&mut ctx)).expect("resolves");

        assert_eq!(unresolved_ref(result.resolved_root()), Some(url_b));
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
            Arc::new(ParsedSchema::new(JsonSchema {
                description: Some("legitimate".to_string()),
                ..JsonSchema::default()
            })),
        );

        // A malicious schema tries to claim the same URL via $id.
        cache.insert(
            "https://json-schema.org/draft/2020-12/schema".to_string(),
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
        let parsed = parse_with(&value, None, Some(&mut ctx)).unwrap();
        let schema = parsed.resolved_root();
        // SSRF guard blocks — unresolved reference kept, nothing cached.
        assert_eq!(unresolved_ref(schema), Some("file:///etc/passwd"));
        assert!(!cache.contains_document("file:///etc/passwd"));
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

        let mut cache = SchemaCache::new();
        cache.insert_document(ref_url.to_string(), remote_value);

        let mut ctx = ParseContext::new(&mut cache, None);
        let resolved = resolve_with(ref_url, None, Some(&mut ctx));

        assert!(resolved.is_some(), "remote ref should resolve from cache");
        let resolved = resolved.unwrap();
        assert_eq!(
            resolved.resolved_root().description.as_deref(),
            Some("remote schema")
        );
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

        let mut cache = SchemaCache::new();
        let url = "https://example.com/types.json".to_string();
        cache.insert_document(url.clone(), remote_value);

        let ref_str = format!("{url}#/definitions/Address");
        let mut ctx = ParseContext::new(&mut cache, None);
        let resolved = resolve_with(&ref_str, None, Some(&mut ctx));

        assert!(
            resolved.is_some(),
            "fragment ref into remote doc should resolve"
        );
        let resolved = resolved.unwrap();
        assert_eq!(
            resolved.resolved_root().description.as_deref(),
            Some("an address")
        );
    }

    // Remote ctx threading: $ref inside properties is resolved remotely.
    #[test]
    fn remote_ref_inside_properties_resolves_via_ctx() {
        // Pre-populate cache with a valid HTTPS URL so no real network call is made.
        let ref_url = "https://example.com/address.json";
        let remote_value: Value = json!({ "type": "object", "description": "an address" });

        let mut cache = SchemaCache::new();
        cache.insert_document(ref_url.to_string(), remote_value);

        let root = json!({
            "type": "object",
            "properties": {
                "address": { "$ref": ref_url }
            }
        });

        let mut ctx = ParseContext::new(&mut cache, None);
        let parsed = parse_with(&root, None, Some(&mut ctx));

        assert!(parsed.is_ok(), "outer schema should parse");
        let parsed = parsed.unwrap();
        let props = parsed
            .resolved_root()
            .properties
            .as_ref()
            .expect("properties should be present");
        let address = parsed.resolve(
            props
                .get("address")
                .expect("address property should be present"),
        );
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
        let mut cache = SchemaCache::new();
        cache.insert_document(ref_url.to_string(), remote_value);

        // Call exactly the code path that fetch_schema_raw uses when ctx=Some.
        let mut ctx = ParseContext::new(&mut cache, None);
        let parsed = parse_with(&body, Some(top_level_url), Some(&mut ctx))
            .expect("should parse top-level schema");
        let schema = parsed.resolved_root();

        // The $ref should have been resolved from the pre-populated cache.
        let props = schema.properties.as_ref().expect("should have properties");
        let home = parsed.resolve(props.get("home").expect("should have home property"));
        assert_eq!(
            home.description.as_deref(),
            Some("an address"),
            "remote $ref should resolve to the cached schema"
        );
        // The ParseContext cache is populated with the $ref target entry.
        assert!(
            cache.contains_document(ref_url),
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
        // Without ctx, the $ref is kept as an unresolved reference.
        assert_eq!(
            unresolved_ref(home),
            Some(ref_url),
            "without ctx the $ref is unresolved, only the unresolved reference is set"
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

        let value =
            fetch_json(&build_agent(None), &server.url, None).expect("valid JSON must load");

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

        let result = fetch_json(&build_agent(None), &server.url, None);

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

        let result = fetch_json(&build_agent(None), &server.url, None);

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

        let result = fetch_json(&build_agent(None), &server.url, None);

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

        let result = fetch_json(&build_agent(None), &server.url, None);

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

        let result = fetch_json(&build_agent(None), &server.url, None);

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

        let result = fetch_json(&build_agent(None), &url, None);

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
    ) -> Result<ParsedSchema, SchemaError> {
        parse_document(value, base_uri, ctx, &mut Meters::production())
    }

    /// The parse of a document that consists of the `$ref` `ref_str` alone,
    /// fetched from `base_uri`; `None` when `ref_str` itself is unresolved (a
    /// target that holds an unresolved reference is a resolution).
    fn resolve_with(
        ref_str: &str,
        base_uri: Option<&str>,
        ctx: Option<&mut ParseContext<'_>>,
    ) -> Option<ParsedSchema> {
        let parsed = parse_with(&json!({ "$ref": ref_str }), base_uri, ctx).ok()?;
        match &parsed.root.reference {
            Some(SchemaRef::Unresolved(text)) if &**text == ref_str => None,
            Some(SchemaRef::Unresolved(_) | SchemaRef::Target(_)) | None => Some(parsed),
        }
    }

    fn cache_with(entries: &[(&str, Value)]) -> SchemaCache {
        let mut cache = SchemaCache::new();
        for (url, value) in entries {
            cache.insert_document((*url).to_string(), value.clone());
        }
        cache
    }

    /// The text of the unresolved `$ref` that `schema` stands for.
    fn unresolved_ref(schema: &JsonSchema) -> Option<&str> {
        match &schema.reference {
            Some(SchemaRef::Unresolved(text)) => Some(&**text),
            Some(SchemaRef::Target(_)) | None => None,
        }
    }

    /// The debug text of the whole parse: the root and the target table.
    fn debug_of(parsed: &ParsedSchema) -> String {
        format!("{parsed:?}")
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

    /// How many JSON levels one [`wrap`] of `keyword` adds.
    fn json_levels_per_wrap(keyword: &str) -> usize {
        match keyword {
            "properties" | "patternProperties" | "dependencies" | "dependentSchemas"
            | "prefixItems" | "allOf" | "anyOf" | "oneOf" | "tuple-items" => 2,
            _ => 1,
        }
    }

    // The schemas nest as deep as the JSON does: nothing cuts them at a fixed
    // schema depth below `MAX_JSON_DEPTH`.
    // Runs on a thread with the 2 MiB stack of a tokio blocking thread, where
    // the server parses: the unoptimized build of the deepest parse fits it.
    #[test]
    fn every_subschema_keyword_parses_to_the_json_depth_limit() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                for keyword in SUBSCHEMA_KEYWORDS {
                    let levels = (MAX_JSON_DEPTH - 1) / json_levels_per_wrap(keyword);
                    let value = nest(keyword, levels, json!({ "description": "LEAF" }));
                    let parsed = parse_schema(&value).expect("parses");
                    assert!(debug_of(&parsed).contains("LEAF"), "keyword {keyword}");
                }
            })
            .expect("thread spawns")
            .join()
            .expect("the parse fits the stack");
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
            let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
            assert!(debug_of(&parsed).contains("REMOTE"), "keyword {keyword}");
        }
    }

    #[test]
    fn every_subschema_keyword_propagates_base_uri_to_children() {
        for keyword in SUBSCHEMA_KEYWORDS {
            let mut value = wrap(keyword, &json!({ "$id": "leaf.json" }));
            value["$id"] = json!("https://example.com/root.json");
            let parsed = parse_schema(&value).expect("parses");
            assert!(
                debug_of(&parsed).contains("https://example.com/leaf.json"),
                "keyword {keyword}"
            );
        }
    }

    #[rstest]
    #[case::missing_pointer("#/x/missing")]
    #[case::missing_anchor("#nope")]
    fn unresolved_ref_returns_unresolved_reference_only_ignoring_siblings(#[case] ref_str: &str) {
        let value = json!({ "$ref": ref_str, "type": "string" });
        let schema = parse_schema(&value).expect("parses").root;
        assert_eq!(unresolved_ref(&schema), Some(ref_str));
        assert!(schema.schema_type.is_none());
    }

    #[test]
    fn resolved_ref_ignores_sibling_keywords() {
        let value = json!({
            "$ref": "#/x-targets/T",
            "description": "SIBLING",
            "x-targets": { "T": { "type": "string" } }
        });
        let parsed = parse_schema(&value).expect("parses");
        let schema = parsed.resolved_root();
        assert_eq!(schema_type_str(schema), Some("string"));
        assert!(schema.description.is_none());
    }

    #[test]
    fn unresolved_dynamic_ref_falls_through_to_sibling_fields() {
        let value = json!({ "$dynamicRef": "#missing", "type": "string" });
        let schema = parse_schema(&value).expect("parses").root;
        assert_eq!(schema_type_str(&schema), Some("string"));
        assert!(schema.reference.is_none());
    }

    // A `$ref` inside a target that a local `$ref` names resolves like any
    // other: with the remote context.
    #[test]
    fn remote_ref_inside_a_local_target_resolves() {
        let url = "https://example.com/addr.json";
        let mut cache = cache_with(&[(url, json!({ "description": "REMOTE" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "x-targets": {
                "T": { "$ref": url },
                "U": { "properties": { "p": { "$ref": url } } }
            },
            "properties": {
                "a": { "$ref": "#/x-targets/T" },
                "b": { "$ref": url },
                "c": { "$ref": "#/x-targets/U" }
            }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert_well_formed(&parsed);
        let root = parsed.resolved_root();
        let a = property(root, "a");
        assert_eq!(parsed.resolve(a).description.as_deref(), Some("REMOTE"));
        assert_eq!(target_of(a), target_of(property(root, "b")));
        let p = property(parsed.resolve(property(root, "c")), "p");
        assert_eq!(target_of(p), target_of(a));
    }

    // A local target is parsed against the base URI in force where it sits:
    // the `$id` of its nearest enclosing object, or the document's URL.
    #[rstest]
    #[case::pointer("#/x-targets/T")]
    #[case::anchor("#anch")]
    fn local_ref_target_resolves_its_id_against_the_enclosing_base(#[case] ref_str: &str) {
        let value = json!({
            "$id": "https://example.com/root.json",
            "x-targets": { "T": { "$anchor": "anch", "$id": "child.json" } },
            "properties": { "p": { "$ref": ref_str } }
        });

        let parsed = parse_schema(&value).expect("parses");

        let p = property(parsed.resolved_root(), "p");
        assert_eq!(
            parsed.resolve(p).id.as_deref(),
            Some("https://example.com/child.json")
        );
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
        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();
        let x = parsed.resolve(&schema.properties.as_ref().expect("properties")["x"]);
        assert_eq!(x.description.as_deref(), Some("PARENT"));
    }

    #[test]
    fn relative_remote_ref_without_base_stays_unresolved() {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({ "$ref": "other.json" });
        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();
        assert_eq!(unresolved_ref(schema), Some("other.json"));
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
        let parsed = resolve_with(url, None, Some(&mut ctx)).expect("resolves");
        let schema = parsed.resolved_root();
        let a = parsed.resolve(&schema.properties.as_ref().expect("properties")["a"]);
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
        let parsed = resolve_with(a_url, None, Some(&mut ctx)).expect("resolves");
        let schema = parsed.resolved_root();
        let p = &schema.properties.as_ref().expect("properties")["p"];
        assert_eq!(unresolved_ref(p), Some(b_url));
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
        let parsed = resolve_with(ref_str, None, Some(&mut ctx)).expect("resolves");
        let schema = parsed.resolved_root();
        assert_eq!(schema.id.as_deref(), Some("https://example.com/sub.json"));
    }

    #[rstest]
    #[case::missing_pointer("https://example.com/doc.json#/definitions/Nope")]
    #[case::missing_anchor("https://example.com/doc.json#nope")]
    fn remote_fragment_miss_yields_unresolved_reference_only(#[case] ref_str: &str) {
        let mut cache = cache_with(&[(
            "https://example.com/doc.json",
            json!({ "description": "DOC" }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({ "$ref": ref_str });
        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();
        assert_eq!(unresolved_ref(schema), Some(ref_str));
        assert!(schema.description.is_none());
    }

    #[test]
    fn branching_self_reference_terminates() {
        let value = json!({
            "properties": { "p": { "$ref": "#" }, "q": { "$ref": "#" } }
        });
        let parsed = parse_schema(&value).expect("parses");
        assert!(parsed.resolved_root().properties.is_some());
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
        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();
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
        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();
        assert_eq!(unresolved_ref(schema), Some(url));
        assert!(!cache.contains_document(url));
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
        let parsed = parse_with(&value, base, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();
        assert_eq!(unresolved_ref(schema), Some(ref_str));
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
        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        assert!(!debug_of(&parsed).contains("REMOTE"));
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
        assert!(!ctx.cache.contains_document(url));
    }

    // ── Raw document cache: remote targets are parsed, documents are not ─────

    #[test]
    fn remote_ref_pointer_into_a_non_schema_document_resolves() {
        let url = "https://example.com/list.json";
        let mut cache = cache_with(&[(url, json!([{ "description": "ITEM" }]))]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let resolved = resolve_with(&format!("{url}#/0"), None, Some(&mut ctx));

        let resolved = resolved.expect("pointer target parses");
        assert_eq!(
            resolved.resolved_root().description.as_deref(),
            Some("ITEM")
        );
    }

    #[test]
    fn remote_ref_to_a_whole_non_schema_document_stays_unresolved() {
        let url = "https://example.com/list.json";
        let mut cache = cache_with(&[(url, json!([{ "description": "ITEM" }]))]);
        let value = json!({ "$ref": url });
        let mut ctx = ParseContext::new(&mut cache, None);

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();

        assert_eq!(unresolved_ref(schema), Some(url));
    }

    #[rstest]
    #[case::whole_document("")]
    #[case::empty_fragment("#")]
    #[case::pointer_fragment("#/definitions/D")]
    #[case::anchor_fragment("#anchor")]
    fn remote_ref_leaves_the_parsed_schema_map_untouched(#[case] fragment: &str) {
        let url = "https://example.com/doc.json";
        let mut cache = cache_with(&[(
            url,
            json!({ "definitions": { "D": { "$anchor": "anchor" } }, "$anchor": "anchor" }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let resolved = resolve_with(&format!("{url}{fragment}"), None, Some(&mut ctx));

        assert!(resolved.is_some(), "the ref resolves");
        assert!(cache.get(url).is_none());
        assert!(cache.contains_document(url));
    }

    #[test]
    fn remote_ref_ignores_a_parsed_schema_without_its_document() {
        let url = "https://example.com/doc.json";
        let mut cache = SchemaCache::new();
        cache.insert(url.to_string(), parsed_with_description("PARSED"));
        let mut ctx = ParseContext::new(&mut cache, None);
        for i in 0..MAX_REMOTE_FETCH_COUNT {
            assert!(ctx.try_visit(&format!("https://example.com/s{i}.json")));
        }
        let value = json!({ "$ref": url });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();

        assert_eq!(unresolved_ref(schema), Some(url));
        assert!(!cache.contains_document(url));
    }

    #[test]
    fn remote_ref_url_is_normalized_to_the_cache_key() {
        let mut cache = cache_with(&[(
            "https://example.com/Doc.json",
            json!({ "description": "REMOTE" }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let resolved =
            resolve_with("HTTPS://EXAMPLE.com/Doc.json", None, Some(&mut ctx)).expect("resolves");

        assert_eq!(
            resolved.resolved_root().description.as_deref(),
            Some("REMOTE")
        );
    }

    #[test]
    fn cached_document_for_a_blocked_url_is_not_served() {
        let url = "http://127.0.0.1/evil.json";
        let mut cache = cache_with(&[(url, json!({ "description": "EVIL" }))]);
        let value = json!({ "$ref": url });
        let mut ctx = ParseContext::new(&mut cache, None);

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");
        let schema = parsed.resolved_root();

        assert_eq!(unresolved_ref(schema), Some(url));
    }

    // ── fetch_schema_raw with a cached document (offline) ───────────────────

    #[test]
    fn fetch_schema_raw_uses_the_cached_document_and_resolves_remote_refs() {
        let own = "https://schema.invalid/own.json";
        let target = "https://schema.invalid/target.json";
        let own_document = json!({ "properties": { "home": { "$ref": target } } });
        let mut cache = cache_with(&[
            (own, own_document.clone()),
            (target, json!({ "description": "an address" })),
        ]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let (value, parsed) =
            fetch_schema_raw(own, None, Some(&mut ctx)).expect("cached document parses");

        assert_eq!(value, own_document);
        let home = parsed
            .root
            .properties
            .as_ref()
            .and_then(|p| p.get("home"))
            .expect("home property");
        assert_eq!(
            parsed.resolve(home).description.as_deref(),
            Some("an address")
        );
    }

    #[test]
    fn fetch_schema_raw_resolves_a_self_reference_through_the_cached_document() {
        let own = "https://schema.invalid/own.json";
        let own_document = json!({
            "properties": { "count": { "$ref": format!("{own}#/definitions/D") } },
            "definitions": { "D": { "type": "integer" } }
        });
        let mut cache = cache_with(&[(own, own_document)]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let (_, parsed) =
            fetch_schema_raw(own, None, Some(&mut ctx)).expect("cached document parses");

        let count = parsed
            .root
            .properties
            .as_ref()
            .and_then(|p| p.get("count"))
            .expect("count property");
        assert_eq!(schema_type_str(parsed.resolve(count)), Some("integer"));
    }

    #[test]
    fn fetch_schema_raw_of_a_cached_non_schema_document_fails_to_parse() {
        let own = "https://schema.invalid/list.json";
        let mut cache = cache_with(&[(own, json!([1]))]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let result = fetch_schema_raw(own, None, Some(&mut ctx));

        assert!(
            matches!(result, Err(SchemaError::ParseFailed(_))),
            "got {result:?}"
        );
    }

    #[test]
    fn fetch_schema_raw_rejects_a_blocked_url_even_when_its_document_is_cached() {
        let url = "http://127.0.0.1/x.json";
        let mut cache = cache_with(&[(url, json!({ "type": "string" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let result = fetch_schema_raw(url, None, Some(&mut ctx));

        assert!(
            matches!(result, Err(SchemaError::UrlNotPermitted(_))),
            "got {result:?}"
        );
    }

    #[rstest]
    #[case::loopback_ipv4("http://127.0.0.1:1/schema.json")]
    #[case::localhost("http://localhost:1/schema.json")]
    #[case::link_local("http://169.254.169.254/schema.json")]
    #[case::private("http://10.0.0.1/schema.json")]
    #[case::file_scheme("file:///etc/passwd")]
    fn fetch_document_rejects_blocked_urls(#[case] url: &str) {
        let result = fetch_document(url, None, &mut Meter::new(usize::MAX));

        assert!(
            matches!(result, Err(SchemaError::UrlNotPermitted(_))),
            "got {result:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // parse_schema errors and the JSON depth limit
    // ══════════════════════════════════════════════════════════════════════════

    /// A document whose deepest value, a scalar, sits `depth` levels down.
    fn document_with_leaf_at_depth(depth: usize) -> Value {
        let leaf = (1..depth).fold(json!(1), |inner, _| json!([inner]));
        json!({ "x-deep": leaf })
    }

    #[rstest]
    #[case::at_the_limit(MAX_JSON_DEPTH, true)]
    #[case::one_level_beyond(MAX_JSON_DEPTH + 1, false)]
    fn parse_schema_applies_the_json_depth_limit(#[case] depth: usize, #[case] accepted: bool) {
        let result = parse_schema(&document_with_leaf_at_depth(depth));

        assert_eq!(result.is_ok(), accepted, "got {result:?}");
        if !accepted {
            assert!(matches!(result, Err(SchemaError::TooDeep)));
        }
    }

    #[test]
    fn too_deep_is_reported_before_a_root_that_is_no_schema() {
        let mut value = json!(1);
        for _ in 0..60 {
            value = json!([value]);
        }

        let result = parse_schema(&value);

        assert!(
            matches!(result, Err(SchemaError::TooDeep)),
            "got {result:?}"
        );
    }

    #[rstest]
    #[case::boolean_false(json!(false), true)]
    #[case::nested_beyond_the_limit(document_with_leaf_at_depth(MAX_JSON_DEPTH + 1), false)]
    fn fetch_schema_raw_reports_the_errors_of_a_cached_document(
        #[case] document: Value,
        #[case] parse_failed: bool,
    ) {
        let own = "https://schema.invalid/own.json";
        let mut cache = cache_with(&[(own, document)]);
        let mut ctx = ParseContext::new(&mut cache, None);

        let result = fetch_schema_raw(own, None, Some(&mut ctx));

        match result {
            Err(SchemaError::ParseFailed(_)) => assert!(parse_failed),
            Err(SchemaError::TooDeep) => assert!(!parse_failed),
            other => panic!("got {other:?}"),
        }
    }

    #[rstest]
    #[case::not_an_object(json!([1]))]
    #[case::too_deep(document_with_leaf_at_depth(MAX_JSON_DEPTH + 1))]
    fn schema_errors_do_not_quote_the_document(#[case] document: Value) {
        let mut marked = document;
        if let Value::Array(items) = &mut marked {
            items.push(json!("MARKER-FROM-THE-DOCUMENT"));
        } else {
            marked["x-marker"] = json!("MARKER-FROM-THE-DOCUMENT");
        }

        let message = parse_schema(&marked).expect_err("is rejected").to_string();

        assert!(!message.contains("MARKER"), "got {message}");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Real schemas: the parse is no larger than its source
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::draft_07_meta_schema(include_str!("../tests/fixtures/schemas/json-schema-draft-07.json"))]
    #[case::cloud_init(include_str!("../tests/fixtures/schemas/cloud-init.json"))]
    #[case::github_workflow(include_str!("../tests/fixtures/schemas/github-workflow.json"))]
    #[case::compose_spec(include_str!("../tests/fixtures/schemas/compose-spec.json"))]
    #[case::kubernetes_deployment(
        include_str!("../tests/fixtures/schemas/kubernetes-deployment-apps-v1.json")
    )]
    fn parsed_fixture_has_no_more_nodes_than_its_source_has_json_values(#[case] raw: &str) {
        let source: Value = serde_json::from_str(raw).expect("fixture is JSON");

        let parsed = parse_schema(&source).expect("fixture is a schema");

        assert_well_formed(&parsed);
        assert!(node_count(&parsed) <= json_value_count(&source));
        assert!(
            !debug_of(&parsed).contains("Unresolved("),
            "a fixture reference is unresolved"
        );
    }

    #[test]
    fn draft_07_meta_schema_references_to_the_root_share_one_target() {
        let source: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/schemas/json-schema-draft-07.json"
        ))
        .expect("fixture is JSON");

        let parsed = parse_schema(&source).expect("parses");

        let root = parsed.resolved_root();
        let root_id = target_of(&parsed.root).expect("`#` makes the root a target");
        let items = property(root, "items");
        let not = property(root, "not");
        let first_of_items = items.any_of.as_ref().and_then(|v| v.first());
        assert_eq!(
            target_of(not).or_else(|| target_of(first_of_items.expect("items anyOf"))),
            Some(root_id)
        );
        let mut ids = target_ids_of(&parsed);
        ids.sort_unstable();
        ids.dedup();
        assert!(
            ids.len() < 30,
            "the meta-schema has few distinct targets: {ids:?}"
        );
    }

    /// Every target id in the debug text of `parsed`.
    fn target_ids_of(parsed: &ParsedSchema) -> Vec<usize> {
        debug_of(parsed)
            .split("Target(TargetId(")
            .skip(1)
            .filter_map(|rest| rest.split(')').next()?.parse().ok())
            .collect()
    }

    // ══════════════════════════════════════════════════════════════════════════
    // References are parsed in full wherever they sit
    // ══════════════════════════════════════════════════════════════════════════

    // Neither the depth of the `$ref` nor the depth of its target cuts it.
    #[rstest]
    fn ref_target_is_parsed_in_full_whatever_the_site_and_target_depth(
        #[values(false, true)] remote: bool,
        #[values(0, 31, 33)] site_levels: usize,
        #[values(0, 31, 33)] target_levels: usize,
    ) {
        let url = "https://example.com/target.json";
        let target = nest("not", target_levels, json!({ "description": "LEAF" }));
        let mut cache = cache_with(&[(url, target.clone())]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let ref_str = if remote { url } else { "#/x-targets/T" };
        let mut value = nest("not", site_levels, json!({ "$ref": ref_str }));
        value["x-targets"] = json!({ "T": target });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert!(debug_of(&parsed).contains("LEAF"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Keywords next to and instead of `$ref`
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn ref_wins_over_dynamic_ref_when_both_are_present() {
        let value = json!({
            "$ref": "#/x/A", "$dynamicRef": "#/x/B",
            "x": { "A": { "type": "string" }, "B": { "type": "integer" } }
        });

        let parsed = parse_schema(&value).expect("parses");

        assert_eq!(schema_type_str(parsed.resolved_root()), Some("string"));
    }

    #[test]
    fn unresolved_ref_does_not_fall_back_to_dynamic_ref() {
        let value = json!({
            "$ref": "#/x/Missing", "$dynamicRef": "#/x/B", "x": { "B": { "type": "integer" } }
        });

        let parsed = parse_schema(&value).expect("parses");

        assert_eq!(unresolved_ref(&parsed.root), Some("#/x/Missing"));
    }

    #[test]
    fn non_string_ref_value_is_ignored() {
        let parsed = parse_schema(&json!({ "$ref": 5, "type": "string" })).expect("parses");

        assert_eq!(schema_type_str(parsed.resolved_root()), Some("string"));
        assert!(parsed.root.reference.is_none());
    }

    #[test]
    fn property_named_dollar_ref_is_a_property_not_a_reference() {
        let value = json!({ "properties": { "$ref": { "type": "string" } } });

        let parsed = parse_schema(&value).expect("parses");

        let property = property(parsed.resolved_root(), "$ref");
        assert_eq!(schema_type_str(property), Some("string"));
        assert!(property.reference.is_none());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // JSON pointers and anchors name places
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::escaped_slash("#/definitions/a~1b", Some("string"))]
    #[case::escaped_tilde("#/definitions/a~0b", Some("integer"))]
    #[case::array_index("#/allOf/0", Some("boolean"))]
    #[case::leading_zero_index("#/allOf/01", None)]
    #[case::index_beyond_the_array("#/allOf/9", None)]
    #[case::below_a_missing_key("#/definitions/Missing/x", None)]
    #[case::percent_encoding_is_not_decoded("#/definitions/a%20b", None)]
    fn pointer_tokens_are_read_as_rfc_6901_says(#[case] pointer: &str, #[case] ty: Option<&str>) {
        let value = json!({
            "properties": { "p": { "$ref": pointer } },
            "allOf": [{ "type": "boolean" }],
            "definitions": {
                "a/b": { "type": "string" }, "a~b": { "type": "integer" },
                "a b": { "type": "number" }
            }
        });

        let parsed = parse_schema(&value).expect("parses");

        let p = property(parsed.resolved_root(), "p");
        match ty {
            Some(ty) => assert_eq!(schema_type_str(parsed.resolve(p)), Some(ty)),
            None => assert_eq!(unresolved_ref(p), Some(pointer)),
        }
    }

    #[test]
    fn hash_and_hash_slash_name_different_places() {
        let value = json!({
            "": { "type": "string" },
            "type": "object",
            "properties": { "a": { "$ref": "#/" }, "b": { "$ref": "#" } }
        });

        let parsed = parse_schema(&value).expect("parses");

        let root = parsed.resolved_root();
        assert_eq!(
            schema_type_str(parsed.resolve(property(root, "a"))),
            Some("string")
        );
        assert_eq!(
            schema_type_str(parsed.resolve(property(root, "b"))),
            Some("object")
        );
    }

    #[rstest]
    #[case::two_anchors(
        json!({ "x-a": { "$anchor": "dup", "type": "string" },
                "x-b": { "$anchor": "dup", "type": "integer" } })
    )]
    #[case::anchor_and_dynamic_anchor(
        json!({ "x-a": { "$dynamicAnchor": "dup", "type": "string" },
                "x-b": { "$anchor": "dup", "type": "integer" } })
    )]
    fn duplicate_anchor_names_resolve_to_the_first_in_document_order(#[case] mut value: Value) {
        value["properties"] = json!({ "p": { "$ref": "#dup" } });

        let parsed = parse_schema(&value).expect("parses");

        let p = property(parsed.resolved_root(), "p");
        assert_eq!(schema_type_str(parsed.resolve(p)), Some("string"));
    }

    #[test]
    fn id_of_a_nested_schema_is_not_an_alias_of_its_location() {
        let value = json!({
            "properties": { "p": { "$ref": "https://example.com/inner.json" } },
            "definitions": { "I": { "$id": "https://example.com/inner.json", "type": "string" } }
        });

        let parsed = parse_schema(&value).expect("parses");

        assert_eq!(
            unresolved_ref(property(parsed.resolved_root(), "p")),
            Some("https://example.com/inner.json")
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Documents and the location of a reference
    // ══════════════════════════════════════════════════════════════════════════

    const ROOT_URL: &str = "https://example.com/schemas/root.json";

    #[test]
    fn relative_remote_ref_in_a_local_target_uses_the_nearest_enclosing_id() {
        let mut cache = cache_with(&[
            (
                "https://example.com/schemas/child.json",
                json!({ "description": "WRONG" }),
            ),
            (
                "https://example.com/schemas/sub/child.json",
                json!({ "description": "RIGHT" }),
            ),
        ]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "$id": ROOT_URL,
            "properties": { "q": { "$ref": "#/definitions/T" } },
            "definitions": { "T": {
                "$id": "sub/t.json", "properties": { "p": { "$ref": "child.json" } } } }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        let t = parsed.resolve(property(parsed.resolved_root(), "q"));
        assert_eq!(
            parsed.resolve(property(t, "p")).description.as_deref(),
            Some("RIGHT")
        );
    }

    #[test]
    fn relative_ref_in_a_target_resolves_against_the_target_not_the_referencing_site() {
        let mut cache = cache_with(&[
            (
                "https://example.com/schemas/rel.json",
                json!({ "description": "RIGHT" }),
            ),
            (
                "https://other.example/rel.json",
                json!({ "description": "WRONG" }),
            ),
        ]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "$id": ROOT_URL,
            "properties": {
                "x": { "$id": "https://other.example/x.json",
                       "properties": { "u": { "$ref": "#/definitions/T" } } },
                "y": { "$ref": "#/definitions/T" }
            },
            "definitions": { "T": { "properties": { "r": { "$ref": "rel.json" } } } }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        let root = parsed.resolved_root();
        let u = property(property(root, "x"), "u");
        assert_eq!(target_of(u), target_of(property(root, "y")));
        let r = property(parsed.resolve(u), "r");
        assert_eq!(parsed.resolve(r).description.as_deref(), Some("RIGHT"));
    }

    #[rstest]
    #[case::without_a_root_id(json!({}), "https://example.com/schemas/rel.json")]
    #[case::with_a_root_id(
        json!({ "$id": "https://example.com/other/root.json" }),
        "https://example.com/other/rel.json"
    )]
    fn local_target_uses_the_root_id_or_else_the_document_url_as_base(
        #[case] header: Value,
        #[case] expected_url: &str,
    ) {
        let mut cache = cache_with(&[(expected_url, json!({ "description": "RIGHT" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let mut value = json!({
            "properties": { "q": { "$ref": "#/definitions/T" } },
            "definitions": { "T": { "$ref": "rel.json" } }
        });
        if let (Value::Object(map), Value::Object(header)) = (&mut value, header) {
            map.extend(header);
        }

        let parsed = parse_with(&value, Some(ROOT_URL), Some(&mut ctx)).expect("parses");

        let q = property(parsed.resolved_root(), "q");
        assert_eq!(parsed.resolve(q).description.as_deref(), Some("RIGHT"));
    }

    // The base of a target is found by walking the raw objects above it, so an
    // `$id` that sits on a container that is no schema moves the base too. The
    // SSRF guard checks every resulting URL, so this costs no safety.
    #[test]
    fn id_on_a_container_above_a_target_moves_its_base_like_any_other_id() {
        let mut cache = cache_with(&[(
            "https://data.example/rel.json",
            json!({ "description": "RIGHT" }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "properties": { "q": { "$ref": "#/x-data/inner" } },
            "x-data": { "$id": "https://data.example/", "inner": {
                "properties": { "p": { "$ref": "rel.json" } } } }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        let inner = parsed.resolve(property(parsed.resolved_root(), "q"));
        assert_eq!(
            parsed.resolve(property(inner, "p")).description.as_deref(),
            Some("RIGHT")
        );
    }

    #[test]
    fn fragment_ref_below_an_id_still_names_the_document_root() {
        let value = json!({
            "$id": ROOT_URL,
            "properties": { "x": { "$id": "https://other.example/x.json",
                                   "properties": { "p": { "$ref": "#/definitions/D" } } } },
            "definitions": { "D": { "type": "string" } }
        });

        let parsed = parse_schema(&value).expect("parses");

        let p = property(property(parsed.resolved_root(), "x"), "p");
        assert_eq!(schema_type_str(parsed.resolve(p)), Some("string"));
    }

    // Depth-1: a document fetched for a `$ref` resolves the references that stay
    // inside it, and none that reach another document, cached or not.
    #[rstest]
    fn fetched_document_resolves_only_references_inside_itself(
        #[values(false, true)] cached: bool,
    ) {
        let b = "https://example.com/b.json";
        let c = "https://example.com/c.json";
        let mut entries = vec![(
            b,
            json!({
                "properties": { "p": { "$ref": c }, "q": { "$ref": "#/x" }, "s": { "$ref": b } },
                "x": { "description": "X" }
            }),
        )];
        if cached {
            entries.push((c, json!({ "description": "C" })));
        }
        let mut cache = cache_with(&entries);
        let mut ctx = ParseContext::new(&mut cache, None);

        let parsed = parse_with(&json!({ "$ref": b }), None, Some(&mut ctx)).expect("parses");

        let b_root = parsed.resolved_root();
        assert_eq!(unresolved_ref(property(b_root, "p")), Some(c));
        assert_eq!(
            parsed.resolve(property(b_root, "q")).description.as_deref(),
            Some("X")
        );
        assert_eq!(
            target_of(property(b_root, "s")),
            target_of(&parsed.root),
            "a fetched document's own URL names the document"
        );
        assert!(ctx.visited.is_empty(), "nothing was fetched");
    }

    #[test]
    fn chain_of_cached_documents_is_entered_once() {
        let url = |i: usize| format!("https://example.com/d{i}.json");
        let entries: Vec<(String, Value)> = (0..100)
            .map(|i| {
                (
                    url(i),
                    json!({ "properties": { "next": { "$ref": url(i + 1) } } }),
                )
            })
            .collect();
        let mut cache = SchemaCache::new();
        for (u, doc) in &entries {
            cache.insert_document(u.clone(), doc.clone());
        }
        let mut ctx = ParseContext::new(&mut cache, None);

        let parsed = parse_with(&json!({ "$ref": url(0) }), None, Some(&mut ctx)).expect("parses");

        assert_eq!(parsed.targets.len(), 1);
        let next = property(parsed.resolved_root(), "next");
        assert_eq!(unresolved_ref(next), Some(url(1).as_str()));
    }

    #[test]
    fn same_pointer_in_two_documents_names_two_targets() {
        let b = "https://example.com/b.json";
        let mut cache = cache_with(&[(
            b,
            json!({ "definitions": { "D": { "description": "REMOTE" } } }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "properties": { "a": { "$ref": "#/definitions/D" },
                            "b": { "$ref": format!("{b}#/definitions/D") } },
            "definitions": { "D": { "description": "MAIN" } }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        let root = parsed.resolved_root();
        let (a, b) = (property(root, "a"), property(root, "b"));
        assert_ne!(target_of(a), target_of(b));
        assert_eq!(parsed.resolve(a).description.as_deref(), Some("MAIN"));
        assert_eq!(parsed.resolve(b).description.as_deref(), Some("REMOTE"));
    }

    #[test]
    fn same_anchor_name_in_two_documents_resolves_per_document() {
        let b = "https://example.com/b.json";
        let mut cache = cache_with(&[(
            b,
            json!({ "x": { "$anchor": "n", "description": "REMOTE" } }),
        )]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "properties": { "a": { "$ref": "#n" }, "b": { "$ref": format!("{b}#n") } },
            "x": { "$anchor": "n", "description": "MAIN" }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        let root = parsed.resolved_root();
        assert_eq!(
            parsed.resolve(property(root, "a")).description.as_deref(),
            Some("MAIN")
        );
        assert_eq!(
            parsed.resolve(property(root, "b")).description.as_deref(),
            Some("REMOTE")
        );
    }

    #[test]
    fn id_of_a_fetched_document_does_not_name_it() {
        let (b, other) = (
            "https://example.com/b.json",
            "https://example.com/other.json",
        );
        let mut cache = cache_with(&[(b, json!({ "$id": other, "description": "B" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({ "properties": { "x": { "$ref": b }, "y": { "$ref": other } } });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        let root = parsed.resolved_root();
        assert_eq!(
            parsed.resolve(property(root, "x")).description.as_deref(),
            Some("B")
        );
        assert_eq!(unresolved_ref(property(root, "y")), Some(other));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Remote references in local targets: fetch limit and SSRF guard
    // ══════════════════════════════════════════════════════════════════════════

    // A proxy on a closed port fails every fetch at once, without network.
    const CLOSED_PROXY: &str = "http://127.0.0.1:1";

    #[test]
    fn fetch_limit_covers_references_inside_local_targets() {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, Some(CLOSED_PROXY));
        let count = MAX_REMOTE_FETCH_COUNT + 5;
        let mut properties = serde_json::Map::new();
        let mut definitions = serde_json::Map::new();
        for i in 0..count {
            properties.insert(
                format!("p{i}"),
                json!({ "$ref": format!("#/definitions/D{i}") }),
            );
            definitions.insert(
                format!("D{i}"),
                json!({ "properties": { "r": { "$ref": format!("https://r{i}.invalid/x.json") } } }),
            );
        }
        let value = json!({ "properties": properties, "definitions": definitions });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert_eq!(ctx.visited.len(), MAX_REMOTE_FETCH_COUNT);
        let unresolved = debug_of(&parsed).matches("Unresolved(").count();
        assert_eq!(
            unresolved, count,
            "no fetch succeeds, so every reference is unresolved"
        );
    }

    #[test]
    fn failed_fetch_is_attempted_once_per_parse() {
        let url = "https://r.invalid/x.json";
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, Some(CLOSED_PROXY));
        let value = json!({
            "properties": { "a": { "$ref": url }, "b": { "$ref": "#/definitions/D" } },
            "definitions": { "D": { "properties": { "r": { "$ref": url } } } }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert_eq!(ctx.visited.len(), 1);
        assert_eq!(debug_of(&parsed).matches("Unresolved(").count(), 2);
    }

    #[rstest]
    #[case::loopback("http://127.0.0.1/evil.json", None)]
    #[case::link_local("http://169.254.169.254/latest", None)]
    #[case::private("http://10.0.0.1/x.json", None)]
    #[case::ipv6_loopback("http://[::1]/x.json", None)]
    #[case::ipv4_mapped_ipv6("http://[::ffff:127.0.0.1]/x.json", None)]
    #[case::decimal_ipv4("http://2130706433/x.json", None)]
    #[case::file_scheme("file:///etc/passwd", None)]
    #[case::ftp_scheme("ftp://example.com/x.json", None)]
    #[case::relative_against_a_loopback_id("meta.json", Some("http://127.0.0.1/"))]
    #[case::relative_against_a_link_local_id("meta.json", Some("http://169.254.169.254/"))]
    fn blocked_ref_inside_a_local_target_stays_unresolved(
        #[case] ref_str: &str,
        #[case] id: Option<&str>,
    ) {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, Some(CLOSED_PROXY));
        let mut value = json!({
            "properties": { "p": { "$ref": "#/definitions/T" } },
            "definitions": { "T": { "properties": { "r": { "$ref": ref_str } } } }
        });
        if let Some(id) = id {
            value["$id"] = json!(id);
        }

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        let t = parsed.resolve(property(parsed.resolved_root(), "p"));
        assert_eq!(unresolved_ref(property(t, "r")), Some(ref_str));
        assert!(ctx.visited.is_empty(), "nothing is fetched");
        assert!(!ctx.cache.contains_document(ref_str));
    }

    #[test]
    fn cached_document_for_a_blocked_url_is_not_served_to_a_local_target() {
        let url = "http://127.0.0.1/evil.json";
        let mut cache = cache_with(&[(url, json!({ "description": "EVIL" }))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let value = json!({
            "properties": { "p": { "$ref": "#/definitions/T" } },
            "definitions": { "T": { "$ref": url } }
        });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert_eq!(
            unresolved_ref(property(parsed.resolved_root(), "p")),
            Some(url)
        );
    }

    #[test]
    fn root_id_naming_a_blocked_url_is_no_way_around_the_guard() {
        let url = "http://127.0.0.1/s.json";
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, Some(CLOSED_PROXY));
        let value = json!({ "$id": url, "properties": { "p": { "$ref": format!("{url}#/definitions/D") } },
                            "definitions": { "D": { "type": "string" } } });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert!(unresolved_ref(property(parsed.resolved_root(), "p")).is_some());
        assert!(ctx.visited.is_empty());
    }

    // The root document's `$id` names the root document, and nothing is read
    // from or written to the cache under it.
    #[test]
    fn root_id_names_the_root_document_and_never_touches_the_cache() {
        let id = "https://trusted.example/s.json";
        let mut cache = cache_with(&[(
            id,
            json!({ "definitions": { "y": { "description": "CACHED" } } }),
        )]);
        let before = cache.get_document(id).cloned();
        let value = json!({
            "$id": id,
            "properties": { "p": { "$ref": format!("{id}#/definitions/y") } },
            "definitions": { "y": { "description": "OWN" } }
        });

        let mut ctx = ParseContext::new(&mut cache, None);
        let parsed = parse_with(
            &value,
            Some("https://other.example/doc.json"),
            Some(&mut ctx),
        )
        .expect("parses");

        assert_eq!(
            parsed
                .resolve(property(parsed.resolved_root(), "p"))
                .description
                .as_deref(),
            Some("OWN")
        );
        assert!(ctx.visited.is_empty());
        assert_eq!(cache.get_document(id).cloned(), before);
        assert!(!cache.contains_document("https://other.example/doc.json"));
    }

    #[test]
    fn document_deeper_than_the_limit_in_the_cache_is_not_entered() {
        test_support::on_a_small_stack(|| {
            let url = "https://example.com/deep.json";
            // Building, copying, or dropping the document recursively would
            // overflow a small stack on its own, so only move it.
            let mut deep = json!({ "description": "DEEP" });
            for _ in 0..5_000 {
                deep = Value::Array(vec![deep]);
            }
            let mut cache = SchemaCache::new();
            cache.insert_document(url.to_string(), deep);
            let mut ctx = ParseContext::new(&mut cache, None);

            let parsed = parse_with(&json!({ "$ref": url }), None, Some(&mut ctx)).expect("parses");

            assert_eq!(unresolved_ref(&parsed.root), Some(url));
            std::mem::forget(cache);
        });
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Work stays bounded for hostile identifiers and keys
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn huge_id_and_many_relative_refs_resolve_nothing_and_fetch_nothing() {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, Some(CLOSED_PROXY));
        let mut properties = serde_json::Map::new();
        for i in 0..5_000 {
            properties.insert(format!("p{i}"), json!({ "$ref": format!("rel{i}.json") }));
        }
        let id = format!("https://example.com/{}/", "a".repeat(1 << 20));
        let value = json!({ "$id": id, "properties": properties });

        let parsed = parse_with(&value, None, Some(&mut ctx)).expect("parses");

        assert!(ctx.visited.is_empty());
        assert_eq!(debug_of(&parsed).matches("Unresolved(").count(), 5_000);
    }

    #[test]
    fn ref_with_a_huge_uri_part_is_unresolved_and_not_fetched() {
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, Some(CLOSED_PROXY));
        let ref_str = format!("https://example.com/{}.json", "a".repeat(1 << 20));

        let parsed = parse_with(&json!({ "$ref": ref_str }), None, Some(&mut ctx)).expect("parses");

        assert!(unresolved_ref(&parsed.root).is_some());
        assert!(ctx.visited.is_empty());
    }

    #[test]
    fn anchors_below_a_huge_key_do_not_copy_the_key() {
        let key = "k".repeat(256 * 1024);
        let mut anchors = serde_json::Map::new();
        let mut properties = serde_json::Map::new();
        for i in 0..4_000 {
            anchors.insert(
                format!("a{i}"),
                json!({ "$anchor": format!("n{i}"), "type": "string" }),
            );
            properties.insert(format!("p{i}"), json!({ "$ref": format!("#n{i}") }));
        }
        let value = json!({ key: anchors, "properties": properties });

        let parsed = parse_schema(&value).expect("parses");

        assert_eq!(parsed.targets.len(), 4_000);
        assert_well_formed(&parsed);
    }

    #[test]
    fn parsing_twice_gives_the_same_table() {
        let value = json!({
            "properties": { "a": { "$ref": "#/definitions/A" }, "b": { "$ref": "#/definitions/B" } },
            "definitions": { "A": { "properties": { "x": { "$ref": "#/definitions/B" } } },
                             "B": { "type": "string" } }
        });

        let first = parse_schema(&value).expect("parses");
        let second = parse_schema(&value).expect("parses");

        assert_eq!(first.targets.len(), second.targets.len());
        let mut ids = (target_ids_of(&first), target_ids_of(&second));
        ids.0.sort_unstable();
        ids.1.sort_unstable();
        assert_eq!(ids.0, ids.1);
        assert_eq!(node_count(&first), node_count(&second));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Memory budgets
    // ══════════════════════════════════════════════════════════════════════════

    /// Parse `value` with the given budgets and return the result with the
    /// meters, to read what the parse charged.
    fn metered(
        value: &Value,
        url: Option<&str>,
        ctx: Option<&mut ParseContext<'_>>,
        parsed_limit: usize,
        raw_limit: usize,
    ) -> (Result<ParsedSchema, SchemaError>, Meters) {
        let mut meters = Meters::new(parsed_limit, raw_limit);
        let result = parse_within(value, url, ctx, &mut meters);
        (result, meters)
    }

    /// What parsing `value` without any limit charges: the parsed budget, then
    /// the raw one.
    fn charges_of(value: &Value) -> (usize, usize) {
        let (result, meters) = metered(value, None, None, usize::MAX, usize::MAX);
        result.expect("parses without limits");
        (meters.parsed.spent(), meters.raw.spent())
    }

    const FIXTURES: [(&str, &str); 5] = [
        (
            "draft-07",
            include_str!("../tests/fixtures/schemas/json-schema-draft-07.json"),
        ),
        (
            "cloud-init",
            include_str!("../tests/fixtures/schemas/cloud-init.json"),
        ),
        (
            "github-workflow",
            include_str!("../tests/fixtures/schemas/github-workflow.json"),
        ),
        (
            "compose-spec",
            include_str!("../tests/fixtures/schemas/compose-spec.json"),
        ),
        (
            "kubernetes-deployment",
            include_str!("../tests/fixtures/schemas/kubernetes-deployment-apps-v1.json"),
        ),
    ];

    /// The budgets are 256 MiB parsed and 128 MiB raw; real schemas use at most
    /// half of each.
    #[test]
    fn real_schemas_use_at_most_half_of_each_budget() {
        for (name, text) in FIXTURES {
            let value: Value = serde_json::from_str(text).expect("fixture is JSON");

            let (parsed, raw) = charges_of(&value);

            eprintln!(
                "{name}: {} bytes, parsed charge {parsed}, raw charge {raw}, raw estimate of the text {}",
                text.len(),
                estimate_bytes(text.as_bytes())
            );
            assert!(
                parsed <= budget::PARSED_BUDGET / 2,
                "{name}: parsed {parsed}"
            );
            assert!(raw <= budget::RAW_BUDGET / 2, "{name}: raw {raw}");
            assert!(
                estimate_bytes(text.as_bytes()) <= budget::RAW_BUDGET / 2,
                "{name}: raw estimate of the text"
            );
        }
    }

    #[test]
    fn too_large_says_so_for_either_budget_and_differs_from_a_large_response() {
        let parsed = SchemaError::TooLarge(SizeBudget::Parsed).to_string();
        let raw = SchemaError::TooLarge(SizeBudget::Raw).to_string();

        assert_eq!(parsed, raw);
        assert!(parsed.contains("too large"), "{parsed}");
        assert_ne!(parsed, SchemaError::ResponseTooLarge.to_string());
    }

    /// Documents that between them put every kind of allocation the parse
    /// charges to the parsed budget in play.
    #[rstest]
    #[case::empty_object(json!({}))]
    #[case::true_schema(json!(true))]
    #[case::nested_properties(json!({"properties": {"a": {"properties": {"b": {}}}}}))]
    #[case::three_empty_schemas(json!({"allOf": [{}, {}, {}]}))]
    #[case::copied_values(json!({
        "enum": [1, "a", {"b": [2, null]}],
        "const": {"k": [true]},
        "default": [],
        "examples": ["x", {"y": 1}]
    }))]
    #[case::string_lists(json!({
        "type": ["string", "null"],
        "required": ["a", "bb"],
        "dependentRequired": {"a": ["b", "c"]}
    }))]
    #[case::copied_strings(json!({
        "title": "t", "description": "d", "pattern": "^p$", "format": "f",
        "contentEncoding": "base64", "contentMediaType": "text/plain",
        "$id": "https://example.com/s.json", "$anchor": "n"
    }))]
    #[case::keys(json!({
        "properties": {"alpha": {}},
        "patternProperties": {"^b": {}},
        "dependencies": {"c": {}, "d": ["e"]},
        "dependentSchemas": {"f": {}}
    }))]
    #[case::shared_target(json!({
        "properties": {"a": {"$ref": "#/definitions/D"}, "b": {"$ref": "#/definitions/D"}},
        "definitions": {"D": {"type": "string"}}
    }))]
    #[case::unresolved_reference(json!({"properties": {"a": {"$ref": "#/definitions/Missing"}}}))]
    #[case::reference_cycle(json!({
        "properties": {"a": {"$ref": "#/definitions/A"}},
        "definitions": {"A": {"$ref": "#/definitions/A"}}
    }))]
    #[case::anchor(json!({
        "properties": {"a": {"$ref": "#n"}},
        "definitions": {"D": {"$anchor": "n", "type": "string"}}
    }))]
    #[case::self_reference(json!({"properties": {"a": {"$ref": "#"}}}))]
    #[case::base_uris(json!({
        "$id": "https://example.com/root.json",
        "properties": {"a": {"$id": "sub.json", "type": "string"}}
    }))]
    fn charge_boundary_is_exact_for_both_budgets(#[case] document: Value) {
        let (parsed, raw) = charges_of(&document);
        let unlimited = node_count(&parse_schema(&document).expect("parses"));

        for (parsed_limit, raw_limit) in [(parsed, usize::MAX), (usize::MAX, raw)] {
            let (result, _) = metered(&document, None, None, parsed_limit, raw_limit);
            assert_eq!(
                node_count(&result.expect("a parse that charges the limit is accepted")),
                unlimited
            );
        }
        let (over_parsed, _) = metered(&document, None, None, parsed - 1, usize::MAX);
        let (over_raw, _) = metered(&document, None, None, usize::MAX, raw - 1);
        assert!(
            matches!(over_parsed, Err(SchemaError::TooLarge(SizeBudget::Parsed))),
            "{over_parsed:?}"
        );
        assert!(
            matches!(over_raw, Err(SchemaError::TooLarge(SizeBudget::Raw))),
            "{over_raw:?}"
        );
    }

    #[test]
    fn charges_do_not_change_from_parse_to_parse() {
        let document = json!({
            "properties": {"a": {"$ref": "#/definitions/A"}, "b": {"$ref": "#/definitions/A"}},
            "definitions": {"A": {"properties": {"x": {"enum": [1, 2]}}}}
        });

        let first = charges_of(&document);

        for _ in 0..5 {
            assert_eq!(charges_of(&document), first);
        }
    }

    /// The limits to try below `charge`: a spread of them, and the ones next to it.
    fn limits_below(charge: usize) -> Vec<usize> {
        let step = (charge / 40).max(1);
        (0..charge)
            .step_by(step)
            .chain([charge - 1, charge.saturating_sub(2), 1])
            .collect()
    }

    /// A parse that exceeds a budget never returns a schema, whichever place
    /// of the schema it exceeds the budget in: the places that read a missing
    /// sub-schema as "no schema" must not let a partly built one through.
    #[rstest]
    fn no_budget_below_the_charge_yields_a_schema(
        #[values(
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
            "dependentSchemas"
        )]
        keyword: &str,
    ) {
        let leaf = json!({"const": [1, 2, 3], "description": "leaf"});
        let document = nest(keyword, 3, leaf);
        let (parsed, raw) = charges_of(&document);

        for limit in limits_below(parsed) {
            let (result, _) = metered(&document, None, None, limit, usize::MAX);
            assert!(
                matches!(result, Err(SchemaError::TooLarge(SizeBudget::Parsed))),
                "{keyword}, parsed limit {limit}: {result:?}"
            );
        }
        for limit in limits_below(raw) {
            let (result, _) = metered(&document, None, None, usize::MAX, limit);
            assert!(
                matches!(result, Err(SchemaError::TooLarge(SizeBudget::Raw))),
                "{keyword}, raw limit {limit}: {result:?}"
            );
        }
    }

    #[rstest]
    #[case::fixtures_parsed(true)]
    #[case::fixtures_raw(false)]
    fn no_budget_below_the_charge_yields_a_schema_for_real_schemas(#[case] parsed_budget: bool) {
        for (name, text) in FIXTURES {
            let value: Value = serde_json::from_str(text).expect("fixture is JSON");
            let (parsed, raw) = charges_of(&value);
            let charge = if parsed_budget { parsed } else { raw };

            for limit in limits_below(charge) {
                let (result, _) = if parsed_budget {
                    metered(&value, None, None, limit, usize::MAX)
                } else {
                    metered(&value, None, None, usize::MAX, limit)
                };
                assert!(
                    matches!(result, Err(SchemaError::TooLarge(_))),
                    "{name}, limit {limit}: {result:?}"
                );
            }
        }
    }

    /// The charge of `document` with the one change `grown` makes to it,
    /// minus the charge of `document`.
    #[rstest]
    #[case::node_in_properties(
        json!({"properties": {"a": {}}}),
        json!({"properties": {"a": {}, "b": {}}}),
        2 * size_of::<JsonSchema>()
    )]
    #[case::node_in_all_of(
        json!({"allOf": [{}]}),
        json!({"allOf": [{}, {}]}),
        2 * size_of::<JsonSchema>()
    )]
    #[case::copied_enum_value(
        json!({"enum": [0]}),
        json!({"enum": [0, 0]}),
        size_of::<Value>()
    )]
    #[case::copied_const_string(
        json!({"const": "a"}),
        json!({"const": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::copied_default_object(
        json!({"default": {}}),
        json!({"default": {"k": [1, 2, 3]}}),
        size_of::<Value>()
    )]
    #[case::copied_examples_value(
        json!({"examples": [1]}),
        json!({"examples": [1, {"a": 1}]}),
        size_of::<Value>()
    )]
    #[case::required_string(
        json!({"required": ["a"]}),
        json!({"required": ["a", "a"]}),
        size_of::<String>() + 1
    )]
    #[case::type_string(
        json!({"type": ["string"]}),
        json!({"type": ["string", "null"]}),
        size_of::<String>() + 4
    )]
    #[case::dependent_required_string(
        json!({"dependentRequired": {"a": ["b"]}}),
        json!({"dependentRequired": {"a": ["b", "c"]}}),
        size_of::<String>() + 1
    )]
    #[case::title(
        json!({"title": "a"}),
        json!({"title": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::description(
        json!({"description": "a"}),
        json!({"description": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::pattern(
        json!({"pattern": "a"}),
        json!({"pattern": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::format(
        json!({"format": "a"}),
        json!({"format": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::content_encoding(
        json!({"contentEncoding": "a"}),
        json!({"contentEncoding": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::content_media_type(
        json!({"contentMediaType": "a"}),
        json!({"contentMediaType": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::anchor_keyword(
        json!({"$anchor": "a"}),
        json!({"$anchor": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::dynamic_anchor(
        json!({"$dynamicAnchor": "a"}),
        json!({"$dynamicAnchor": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::property_name(
        json!({"properties": {"a": {}}}),
        json!({"properties": {"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {}}}),
        64
    )]
    #[case::pattern_property_name(
        json!({"patternProperties": {"a": {}}}),
        json!({"patternProperties": {"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {}}}),
        64
    )]
    #[case::dependency_name(
        json!({"dependencies": {"a": ["b"]}}),
        json!({"dependencies": {"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": ["b"]}}),
        64
    )]
    #[case::id(
        json!({"$id": "https://example.com/a"}),
        json!({"$id": "https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::unresolved_reference_text(
        json!({"$ref": "#/a"}),
        json!({"$ref": "#/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        64
    )]
    #[case::target_slot(
        json!({"properties": {"a": {"$ref": "#/definitions/A"}}, "definitions": {"A": {}}}),
        json!({"properties": {"a": {"$ref": "#/definitions/A"}, "b": {"$ref": "#/definitions/B"}},
               "definitions": {"A": {}, "B": {}}}),
        3 * size_of::<JsonSchema>()
    )]
    #[case::visited_object_in_a_target(
        json!({"properties": {"a": {"$ref": "#/definitions/A"}}, "definitions": {"A": {}}}),
        json!({"properties": {"a": {"$ref": "#/definitions/A"}},
               "definitions": {"A": {"properties": {"x": {}}}}}),
        2 * size_of::<JsonSchema>()
    )]
    #[case::anchor_in_the_index(
        json!({"properties": {"a": {"$ref": "#n"}}, "definitions": {"D": {"$anchor": "n"}}}),
        json!({"properties": {"a": {"$ref": "#n"}},
               "definitions": {"D": {"$anchor": "n"}, "E": {"$anchor": "m"}}}),
        64
    )]
    #[case::pointer_token(
        json!({"properties": {"a": {"$ref": "#/definitions/D"}}, "definitions": {"D": {}}}),
        json!({"properties": {"a": {"$ref": "#/definitions/DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD"}},
               "definitions": {"DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD": {}}}),
        64
    )]
    fn parsed_charge_grows_with_each_allocation_class(
        #[case] small: Value,
        #[case] grown: Value,
        #[case] at_least: usize,
    ) {
        let (before, _) = charges_of(&small);
        let (after, _) = charges_of(&grown);

        assert!(
            after >= before + at_least,
            "charged {} more, expected at least {at_least}",
            after.saturating_sub(before)
        );
    }

    #[rstest]
    #[case::one_reference(1)]
    #[case::two_references(2)]
    #[case::ten_references(10)]
    fn a_shared_target_is_charged_once_whatever_the_number_of_references(
        #[case] references: usize,
    ) {
        let properties: serde_json::Map<String, Value> = (0..references)
            .map(|i| (format!("p{i}"), json!({"$ref": "#/definitions/D"})))
            .collect();
        let with = |count: usize| {
            let mut value = json!({"definitions": {"D": {"const": [1, 2, 3]}}});
            value["properties"] =
                Value::Object(properties.clone().into_iter().take(count).collect());
            charges_of(&value).0
        };

        let one = with(1);
        let many = with(references);

        // Each further reference costs a reference node, a property name, and
        // a visit; it never costs the target again.
        let per_reference = (many - one) / (references - 1).max(1);
        assert!(
            references == 1 || per_reference < 3 * size_of::<JsonSchema>(),
            "{per_reference} bytes per further reference"
        );
    }

    #[test]
    fn siblings_of_a_reference_are_not_charged() {
        let plain =
            json!({"properties": {"a": {"$ref": "#/definitions/D"}}, "definitions": {"D": {}}});
        let with_siblings = json!({
            "properties": {"a": {"$ref": "#/definitions/D", "const": [1, 2, 3], "description": "never built"}},
            "definitions": {"D": {}}
        });

        assert_eq!(charges_of(&plain).0, charges_of(&with_siblings).0);
    }

    #[test]
    fn unreferenced_definitions_are_charged_to_the_raw_budget_only() {
        let plain = json!({"properties": {"a": {}}});
        let with_definitions = json!({
            "properties": {"a": {}},
            "definitions": {"D": {"const": [1, 2, 3]}},
            "$defs": {"E": {"enum": [1, 2]}},
            "x-padding": [1, 2, 3, {"a": 1}]
        });

        let (parsed_plain, raw_plain) = charges_of(&plain);
        let (parsed_with, raw_with) = charges_of(&with_definitions);

        assert_eq!(parsed_plain, parsed_with);
        assert!(raw_with > raw_plain);
    }

    /// Four distinct targets that each of five schemas references once.
    fn distinct_targets_document() -> Value {
        let references: Vec<Value> = (0..4)
            .map(|i| json!({"$ref": format!("#/definitions/T{i}")}))
            .collect();
        let definitions: serde_json::Map<String, Value> = (0..4)
            .map(|i| (format!("T{i}"), json!({"type": "string"})))
            .collect();
        json!({"allOf": references, "definitions": definitions})
    }

    #[test]
    fn exceeding_the_budget_while_discovering_targets_builds_no_node() {
        let document = distinct_targets_document();
        let mut meters = Meters::new(usize::MAX, usize::MAX);
        parse_within(&document, None, None, &mut meters).expect("parses");
        let discovery = meters.spent_before_first_node;
        assert!(discovery > 0 && meters.nodes > 0);

        let (result, meters) = metered(&document, None, None, discovery - 1, usize::MAX);
        assert!(matches!(
            result,
            Err(SchemaError::TooLarge(SizeBudget::Parsed))
        ));
        assert_eq!(
            meters.nodes, 0,
            "a node was built during a failed discovery"
        );

        let (result, meters) = metered(&document, None, None, discovery, usize::MAX);
        assert!(matches!(
            result,
            Err(SchemaError::TooLarge(SizeBudget::Parsed))
        ));
        assert!(meters.nodes > 0, "the build starts once discovery fits");
    }

    #[test]
    fn exceeding_the_budget_while_building_stops_the_build() {
        let document = distinct_targets_document();
        let (_, full) = metered(&document, None, None, usize::MAX, usize::MAX);
        // Room for the discovery and one node: the second node is refused.
        let limit = full.spent_before_first_node + NODE_BYTES;

        let (result, meters) = metered(&document, None, None, limit, usize::MAX);

        assert!(matches!(
            result,
            Err(SchemaError::TooLarge(SizeBudget::Parsed))
        ));
        assert_eq!(meters.nodes, 2, "the build went on after the budget");
        assert!(meters.nodes < full.nodes);
    }

    #[test]
    fn deep_documents_are_rejected_as_too_deep_before_any_charge() {
        let mut deep = json!({});
        for _ in 0..=MAX_JSON_DEPTH {
            deep = json!({ "not": deep });
        }

        let (result, meters) = metered(&deep, None, None, 0, 0);

        assert!(matches!(result, Err(SchemaError::TooDeep)), "{result:?}");
        assert_eq!(meters.raw.spent(), 0);
    }

    #[test]
    fn a_document_that_is_no_schema_is_rejected_as_such_within_budget() {
        let (result, _) = metered(&json!(false), None, None, usize::MAX, usize::MAX);

        assert!(matches!(result, Err(SchemaError::ParseFailed(_))));
    }

    #[test]
    fn the_counters_belong_to_one_parse() {
        let document = distinct_targets_document();
        let (parsed, raw) = charges_of(&document);
        let mut cache = SchemaCache::new();
        let mut ctx = ParseContext::new(&mut cache, None);

        for _ in 0..2 {
            let mut meters = Meters::new(parsed, raw);
            parse_within(&document, None, Some(&mut ctx), &mut meters)
                .expect("a fresh parse starts at zero");
        }
    }

    const REMOTE_URL: &str = "https://schema.invalid/d.json";

    fn root_with_remote_reference() -> Value {
        json!({"properties": {"a": {"$ref": format!("{REMOTE_URL}#/definitions/D")}}})
    }

    fn remote_document() -> Value {
        json!({"definitions": {"D": {"properties": {"x": {}, "y": {}}}}, "x-unread": [1, 2, 3]})
    }

    /// Fetch `ROOT_URL` from a cache that holds the root and the remote document.
    fn fetch_with_remote(
        parsed_limit: usize,
        raw_limit: usize,
    ) -> (Result<(Value, ParsedSchema), SchemaError>, Meters) {
        let mut cache = cache_with(&[
            (ROOT_URL, root_with_remote_reference()),
            (REMOTE_URL, remote_document()),
        ]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let mut meters = Meters::new(parsed_limit, raw_limit);
        let result = fetch_schema_within(ROOT_URL, None, Some(&mut ctx), &mut meters);
        (result, meters)
    }

    #[test]
    fn the_budgets_cover_the_root_and_the_documents_fetched_for_it() {
        let (result, meters) = fetch_with_remote(usize::MAX, usize::MAX);
        result.expect("parses");
        let (parsed, raw) = (meters.parsed.spent(), meters.raw.spent());

        // The root is cached: the cache keeps a copy and the caller gets one.
        assert_eq!(
            raw,
            2 * estimate_value(&root_with_remote_reference()) + estimate_value(&remote_document())
        );
        let (alone, _) = charges_of(&root_with_remote_reference());
        assert!(
            parsed > alone,
            "the remote target is charged: {parsed} vs {alone}"
        );

        let (at_parsed, _) = fetch_with_remote(parsed, usize::MAX);
        let (over_parsed, _) = fetch_with_remote(parsed - 1, usize::MAX);
        let (at_raw, _) = fetch_with_remote(usize::MAX, raw);
        let (over_raw, _) = fetch_with_remote(usize::MAX, raw - 1);
        assert!(at_parsed.is_ok() && at_raw.is_ok());
        assert!(matches!(
            over_parsed,
            Err(SchemaError::TooLarge(SizeBudget::Parsed))
        ));
        assert!(matches!(
            over_raw,
            Err(SchemaError::TooLarge(SizeBudget::Raw))
        ));
    }

    #[test]
    fn a_remote_document_is_charged_once_for_several_references() {
        let reference =
            |fragment: &str| json!({"$ref": format!("{REMOTE_URL}#/definitions/{fragment}")});
        let remote = json!({"definitions": {"D": {}, "E": {}}});
        let raw_with = |root: Value| {
            let mut cache = cache_with(&[(ROOT_URL, root), (REMOTE_URL, remote.clone())]);
            let mut ctx = ParseContext::new(&mut cache, None);
            let mut meters = Meters::new(usize::MAX, usize::MAX);
            fetch_schema_within(ROOT_URL, None, Some(&mut ctx), &mut meters).expect("parses");
            meters.raw.spent()
                - 2 * estimate_value(&cache.get_document(ROOT_URL).cloned().unwrap_or_default())
        };

        let once = raw_with(json!({"properties": {"a": reference("D")}}));
        let thrice = raw_with(
            json!({"properties": {"a": reference("D"), "b": reference("D"), "c": reference("E")}}),
        );

        assert_eq!(once, estimate_value(&remote));
        assert_eq!(thrice, estimate_value(&remote));
    }

    #[test]
    fn no_further_document_is_read_once_a_budget_is_exceeded() {
        let remote_a = "https://schema.invalid/a.json";
        let remote_b = "https://schema.invalid/b.json";
        let root = json!({"properties": {
            "a": {"$ref": remote_a}, "b": {"$ref": remote_b}
        }});
        let mut cache = cache_with(&[
            (ROOT_URL, root.clone()),
            (remote_a, json!({})),
            (remote_b, json!({})),
        ]);
        let mut ctx = ParseContext::new(&mut cache, None);
        // The root alone fits (twice, as it is cached), the first document does not.
        let limit = 2 * estimate_value(&root) + estimate_value(&json!({})) - 1;
        let mut meters = Meters::new(usize::MAX, limit);

        let result = fetch_schema_within(ROOT_URL, None, Some(&mut ctx), &mut meters);

        assert!(
            matches!(result, Err(SchemaError::TooLarge(SizeBudget::Raw))),
            "{result:?}"
        );
        assert_eq!(
            meters.raw.spent(),
            2 * estimate_value(&root) + estimate_value(&json!({})),
            "the second document was charged after the budget was exceeded"
        );
    }

    #[test]
    fn a_cached_document_the_budget_cannot_hold_is_rejected_as_too_large() {
        let mut cache = cache_with(&[(ROOT_URL, json!({"enum": [0, 0, 0]}))]);
        let mut ctx = ParseContext::new(&mut cache, None);
        let mut meters = Meters::new(usize::MAX, 10);

        let result = fetch_schema_within(ROOT_URL, None, Some(&mut ctx), &mut meters);

        assert!(
            matches!(result, Err(SchemaError::TooLarge(SizeBudget::Raw))),
            "{result:?}"
        );
    }

    // ── The estimate of a response, charged before it is deserialized ───────

    /// Fetch `body` from a canned server with `raw` as the raw budget.
    fn fetch_body_within(body: &[u8], raw: &mut Meter) -> Result<Value, SchemaError> {
        let server = CannedServer::start(200, &[], body.to_vec(), false);
        fetch_json(&build_agent(None), &server.url, Some(raw))
    }

    #[test]
    fn a_response_is_charged_to_the_raw_budget_by_its_estimate() {
        let body = br#"{"properties":{"a":{"type":"string"}}}"#;
        let mut raw = Meter::new(usize::MAX);

        let value = fetch_body_within(body, &mut raw).expect("valid JSON");

        assert_eq!(raw.spent(), estimate_bytes(body));
        assert_eq!(raw.spent(), estimate_value(&value));
    }

    #[test]
    fn a_response_whose_estimate_fits_the_budget_exactly_is_accepted() {
        let body = br#"{"a":[1,2,3]}"#;

        let mut at = Meter::new(estimate_bytes(body));
        let mut over = Meter::new(estimate_bytes(body) - 1);

        assert!(fetch_body_within(body, &mut at).is_ok());
        assert!(matches!(
            fetch_body_within(body, &mut over),
            Err(SchemaError::TooLarge(SizeBudget::Raw))
        ));
    }

    #[test]
    fn a_response_is_rejected_by_its_estimate_before_it_is_deserialized() {
        // Not valid JSON: deserializing it would fail with `ParseFailed`.
        let body = b"[[[[[[[[[[[[[[[[[[[[[[[[";
        let mut raw = Meter::new(estimate_bytes(body) - 1);

        let result = fetch_body_within(body, &mut raw);

        assert!(
            matches!(result, Err(SchemaError::TooLarge(SizeBudget::Raw))),
            "{result:?}"
        );
    }

    #[test]
    fn the_response_size_limit_comes_before_the_raw_budget() {
        let body = vec![b' '; MAX_SCHEMA_BYTES as usize + 1];
        let mut raw = Meter::new(0);

        let result = fetch_body_within(&body, &mut raw);

        assert!(
            matches!(result, Err(SchemaError::ResponseTooLarge)),
            "{result:?}"
        );
        assert_eq!(raw.spent(), 0);
    }

    #[test]
    fn a_response_without_a_budget_is_not_charged() {
        let server = CannedServer::start(200, &[], br#"{"a":1}"#.to_vec(), false);

        let value = fetch_json(&build_agent(None), &server.url, None).expect("valid JSON");

        assert_eq!(value, json!({"a": 1}));
    }

    /// Text that is large and legitimate must not be rejected by the estimate.
    #[test]
    fn long_strings_and_padding_cost_their_length_not_a_multiple_of_it() {
        let description = format!(
            r#"{{"description":"{}"}}"#,
            "x".repeat(5 * 1024 * 1024 - 32)
        );
        let padded = format!(r#"{{"type":"string"{}}}"#, " ".repeat(5 * 1024 * 1024 - 32));

        for text in [description, padded] {
            assert!(text.len() as u64 <= MAX_SCHEMA_BYTES);
            assert!(estimate_bytes(text.as_bytes()) <= text.len() + 1024);
        }
    }
}
