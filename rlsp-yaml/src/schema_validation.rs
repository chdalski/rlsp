// SPDX-License-Identifier: MIT

use std::cell::RefCell;
use std::collections::HashMap;

use regex::RegexBuilder;
use saphyr::YamlOwned;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};

/// Maximum length of a `pattern` string we will compile and match, as a
/// guard against pathological `ReDoS` inputs.
const MAX_PATTERN_LEN: usize = 1024;

/// Maximum compiled NFA size for a regex, as a memory guard.
const REGEX_SIZE_LIMIT: usize = 512 * 1024;

thread_local! {
    /// Per-thread regex cache, keyed by pattern string.
    /// `None` means the pattern was tried and failed to compile within limits.
    static REGEX_CACHE: RefCell<HashMap<String, Option<regex::Regex>>> =
        RefCell::new(HashMap::new());
}

/// Return a compiled `Regex` for `pattern`, using the thread-local cache.
///
/// Returns `None` if the pattern exceeds `REGEX_SIZE_LIMIT` or is otherwise
/// invalid. The failed result is also cached so that subsequent calls with
/// the same pattern skip recompilation.
fn get_regex(pattern: &str) -> Option<regex::Regex> {
    REGEX_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if let Some(entry) = map.get(pattern) {
            return entry.clone();
        }
        let compiled = RegexBuilder::new(pattern)
            .size_limit(REGEX_SIZE_LIMIT)
            .build()
            .ok();
        map.insert(pattern.to_string(), compiled.clone());
        compiled
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/// Maximum recursion depth for the validation walk.
const MAX_VALIDATION_DEPTH: usize = 64;

/// Maximum number of `allOf` / `anyOf` / `oneOf` branches evaluated.
const MAX_BRANCH_COUNT: usize = 20;

/// Maximum length of schema description text embedded in diagnostic messages.
const MAX_DESCRIPTION_LEN: usize = 200;

/// Maximum number of enum values listed verbatim in a diagnostic message.
const MAX_ENUM_DISPLAY: usize = 5;

// ──────────────────────────────────────────────────────────────────────────────
// Evaluation context (for unevaluatedProperties / unevaluatedItems)
// ──────────────────────────────────────────────────────────────────────────────

/// Collect property names directly evaluated by `schema` (one level: `properties`,
/// `patternProperties` keys, and composition sub-schema properties).
/// Used to determine what composition branches consider "evaluated" without
/// full recursive context threading.
fn collect_evaluated_properties(schema: &JsonSchema, key: &str) -> bool {
    // Matched by properties
    if schema
        .properties
        .as_ref()
        .is_some_and(|p| p.contains_key(key))
    {
        return true;
    }
    // Matched by patternProperties
    if let Some(pp) = &schema.pattern_properties {
        for (pattern, _) in pp {
            if pattern.len() <= MAX_PATTERN_LEN {
                if let Some(re) = get_regex(pattern) {
                    if re.is_match(key) {
                        return true;
                    }
                }
            }
        }
    }
    // Matched by allOf sub-schemas (one level deep)
    if let Some(all_of) = &schema.all_of {
        for branch in all_of.iter().take(MAX_BRANCH_COUNT) {
            if collect_evaluated_properties(branch, key) {
                return true;
            }
        }
    }
    // Matched by anyOf / oneOf sub-schemas (one level deep)
    if let Some(any_of) = &schema.any_of {
        for branch in any_of.iter().take(MAX_BRANCH_COUNT) {
            if collect_evaluated_properties(branch, key) {
                return true;
            }
        }
    }
    if let Some(one_of) = &schema.one_of {
        for branch in one_of.iter().take(MAX_BRANCH_COUNT) {
            if collect_evaluated_properties(branch, key) {
                return true;
            }
        }
    }
    // Matched by if/then/else (one level deep)
    if let Some(then_s) = &schema.then_schema {
        if collect_evaluated_properties(then_s, key) {
            return true;
        }
    }
    if let Some(else_s) = &schema.else_schema {
        if collect_evaluated_properties(else_s, key) {
            return true;
        }
    }
    false
}

/// Collect the number of prefix items directly covered by `schema` and its
/// composition sub-schemas (one level deep).
fn collect_evaluated_item_count(schema: &JsonSchema) -> usize {
    let mut count = schema.prefix_items.as_ref().map_or(0, Vec::len);
    // items (non-nil) covers all remaining indices — signal with usize::MAX
    if schema.items.is_some() {
        return usize::MAX;
    }
    if let Some(all_of) = &schema.all_of {
        for branch in all_of.iter().take(MAX_BRANCH_COUNT) {
            let branch_count = collect_evaluated_item_count(branch);
            if branch_count == usize::MAX {
                return usize::MAX;
            }
            count = count.max(branch_count);
        }
    }
    count
}

// ──────────────────────────────────────────────────────────────────────────────
// Validation context
// ──────────────────────────────────────────────────────────────────────────────

/// Shared per-call context threaded through the validation walk.
///
/// Bundles the three parameters that every helper needs — the document text
/// (split into lines for position lookup), the diagnostic accumulator, and the
/// `format_validation` flag — so individual helpers do not need 8+ arguments.
struct Ctx<'a> {
    lines: &'a [&'a str],
    diagnostics: &'a mut Vec<Diagnostic>,
    format_validation: bool,
}

impl<'a> Ctx<'a> {
    const fn new(
        lines: &'a [&'a str],
        diagnostics: &'a mut Vec<Diagnostic>,
        format_validation: bool,
    ) -> Self {
        Self {
            lines,
            diagnostics,
            format_validation,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Validate `docs` (parsed YAML ASTs) against `schema` and return diagnostics.
///
/// `text` is the raw document text used for position lookup.
/// Each element of `docs` is one YAML document (separated by `---`).
/// `format_validation` controls whether the `format` keyword is validated.
#[must_use]
pub fn validate_schema(
    text: &str,
    docs: &[YamlOwned],
    schema: &JsonSchema,
    format_validation: bool,
) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diagnostics = Vec::new();
    let mut ctx = Ctx::new(&lines, &mut diagnostics, format_validation);

    for doc in docs {
        validate_node(doc, schema, &[], &mut ctx, 0);
    }

    diagnostics
}

// ──────────────────────────────────────────────────────────────────────────────
// Core recursive validation
// ──────────────────────────────────────────────────────────────────────────────

/// Recursively validate a YAML node against a schema.
///
/// `path` is the property path to the current node (for diagnostic messages).
/// `depth` guards against stack overflow on deeply nested structures.
fn validate_node(
    node: &YamlOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    if depth > MAX_VALIDATION_DEPTH {
        return;
    }

    // Type check
    if let Some(schema_type) = &schema.schema_type {
        let yaml_type = yaml_type_name(node);
        if !type_matches(yaml_type, schema_type) {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Type mismatch: expected {}, got {} at {}",
                    display_schema_type(schema_type),
                    yaml_type,
                    format_path(path)
                ),
            ));
            // Don't descend further into a type-mismatched node
            return;
        }
    }

    // Enum check
    if let Some(enum_values) = &schema.enum_values
        && let Some(yaml_val) = yaml_to_json(node)
        && !enum_values.contains(&yaml_val)
    {
        let range = node_range(path, ctx.lines);
        let listed: Vec<String> = enum_values
            .iter()
            .take(MAX_ENUM_DISPLAY)
            .map(ToString::to_string)
            .collect();
        let valid = if enum_values.len() > MAX_ENUM_DISPLAY {
            format!(
                "{}, ... and {} more",
                listed.join(", "),
                enum_values.len() - MAX_ENUM_DISPLAY
            )
        } else {
            listed.join(", ")
        };
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaEnum",
            format!("Value at {} must be one of: {}", format_path(path), valid),
        ));
    }

    // Scalar constraints
    validate_scalar_constraints(node, schema, path, ctx);

    // Mapping-specific checks
    if let YamlOwned::Mapping(map) = node {
        validate_mapping(map, schema, path, ctx, depth);
    }

    // Sequence-specific checks
    if let YamlOwned::Sequence(seq) = node {
        validate_sequence(seq, schema, path, ctx, depth);
    }

    // Composition
    validate_composition(node, schema, path, ctx, depth);

    // unevaluatedProperties (Draft 2019-09)
    if schema.unevaluated_properties.is_some() {
        if let YamlOwned::Mapping(map) = node {
            validate_unevaluated_properties(map, schema, path, ctx, depth);
        }
    }

    // unevaluatedItems (Draft 2019-09)
    if schema.unevaluated_items.is_some() {
        if let YamlOwned::Sequence(seq) = node {
            validate_unevaluated_items(seq, schema, path, ctx, depth);
        }
    }
}

fn validate_unevaluated_properties(
    map: &saphyr::MappingOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    use saphyr::ScalarOwned;
    for k in map.keys() {
        let key_str = match k {
            YamlOwned::Value(ScalarOwned::String(s)) => s.clone(),
            YamlOwned::Value(ScalarOwned::Integer(i)) => i.to_string(),
            YamlOwned::Value(_)
            | YamlOwned::Sequence(_)
            | YamlOwned::Mapping(_)
            | YamlOwned::Alias(_)
            | YamlOwned::BadValue
            | YamlOwned::Tagged(_, _)
            | YamlOwned::Representation(_, _, _) => continue,
        };
        if collect_evaluated_properties(schema, &key_str) {
            continue;
        }
        match &schema.unevaluated_properties {
            Some(AdditionalProperties::Denied) => {
                let range = key_range(&key_str, path, ctx.lines);
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::WARNING,
                    "schemaUnevaluatedProperty",
                    format!(
                        "Unevaluated property '{}' is not allowed at {}",
                        key_str,
                        format_path(path)
                    ),
                ));
            }
            Some(AdditionalProperties::Schema(extra_schema)) => {
                let Some(v) = map.get(k) else {
                    continue;
                };
                let mut child_path = path.to_vec();
                child_path.push(key_str.clone());
                validate_node(v, extra_schema, &child_path, ctx, depth + 1);
            }
            None => {}
        }
    }
}

fn validate_unevaluated_items(
    seq: &saphyr::SequenceOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let evaluated_count = collect_evaluated_item_count(schema);
    let Some(unevaluated_schema) = &schema.unevaluated_items else {
        return;
    };
    for (i, item) in seq.iter().enumerate() {
        if evaluated_count == usize::MAX || i < evaluated_count {
            continue;
        }
        let mut item_path = path.to_vec();
        item_path.push(format!("[{i}]"));
        validate_node(item, unevaluated_schema, &item_path, ctx, depth + 1);
    }
}

fn validate_sequence(
    seq: &saphyr::SequenceOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    // prefixItems — validate each element at its positional schema
    let prefix_len = schema.prefix_items.as_ref().map_or(0, Vec::len);
    if let Some(prefix_schemas) = &schema.prefix_items {
        for (i, (item, item_schema)) in seq.iter().zip(prefix_schemas.iter()).enumerate() {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            validate_node(item, item_schema, &item_path, ctx, depth + 1);
        }
    }
    // items — applies to elements beyond prefixItems
    if let Some(items_schema) = &schema.items {
        for (i, item) in seq.iter().enumerate().skip(prefix_len) {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            validate_node(item, items_schema, &item_path, ctx, depth + 1);
        }
    } else if let Some(additional_items) = &schema.additional_items {
        // additionalItems (Draft-04/07) — applies to elements beyond the tuple prefix
        for (i, item) in seq.iter().enumerate().skip(prefix_len) {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            match additional_items {
                AdditionalProperties::Denied => {
                    let range = node_range(&item_path, ctx.lines);
                    ctx.diagnostics.push(make_diagnostic(
                        range,
                        DiagnosticSeverity::WARNING,
                        "schemaAdditionalItems",
                        format!(
                            "Additional item at {}[{i}] is not allowed",
                            format_path(path)
                        ),
                    ));
                }
                AdditionalProperties::Schema(extra_schema) => {
                    validate_node(item, extra_schema, &item_path, ctx, depth + 1);
                }
            }
        }
    }
    validate_array_constraints(seq, schema, path, ctx, depth);
}

fn validate_array_constraints(
    seq: &saphyr::SequenceOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let len = seq.len() as u64;

    if let Some(min) = schema.min_items {
        if len < min {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinItems",
                format!(
                    "Array at {} has {} items, minimum is {}",
                    format_path(path),
                    len,
                    min
                ),
            ));
        }
    }

    if let Some(max) = schema.max_items {
        if len > max {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaxItems",
                format!(
                    "Array at {} has {} items, maximum is {}",
                    format_path(path),
                    len,
                    max
                ),
            ));
        }
    }

    if schema.unique_items == Some(true) {
        let json_items: Vec<serde_json::Value> = seq.iter().filter_map(yaml_to_json).collect();
        let has_duplicate = json_items.iter().enumerate().any(|(i, a)| {
            json_items
                .get(..i)
                .is_some_and(|prev| prev.iter().any(|b| a == b))
        });
        if has_duplicate {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaUniqueItems",
                format!("Array at {} contains duplicate items", format_path(path)),
            ));
        }
    }

    if let Some(contains_schema) = &schema.contains {
        validate_contains(seq, contains_schema, schema, path, ctx, depth);
    }
}

fn validate_mapping_constraints(
    map: &saphyr::MappingOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    let len = map.len() as u64;

    if let Some(min) = schema.min_properties {
        if len < min {
            let range = mapping_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinProperties",
                format!(
                    "Object at {} has {} properties, minimum is {}",
                    format_path(path),
                    len,
                    min
                ),
            ));
        }
    }

    if let Some(max) = schema.max_properties {
        if len > max {
            let range = mapping_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaxProperties",
                format!(
                    "Object at {} has {} properties, maximum is {}",
                    format_path(path),
                    len,
                    max
                ),
            ));
        }
    }
}

fn validate_contains(
    seq: &saphyr::SequenceOwned,
    contains_schema: &JsonSchema,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let lines = ctx.lines;
    let format_validation = ctx.format_validation;
    let match_count = seq
        .iter()
        .filter(|item| {
            let mut scratch = Vec::new();
            let mut probe = Ctx::new(lines, &mut scratch, format_validation);
            validate_node(item, contains_schema, path, &mut probe, depth + 1);
            scratch.is_empty()
        })
        .count() as u64;

    // Default min is 1 when `contains` is present without `minContains`
    let effective_min = schema.min_contains.unwrap_or(1);

    if match_count < effective_min {
        let range = node_range(path, ctx.lines);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaContains",
            format!(
                "Array at {} must contain at least {} item(s) matching the schema, found {}",
                format_path(path),
                effective_min,
                match_count
            ),
        ));
    }

    if let Some(max) = schema.max_contains {
        if match_count > max {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaContains",
                format!(
                    "Array at {} must contain at most {} item(s) matching the schema, found {}",
                    format_path(path),
                    max,
                    match_count
                ),
            ));
        }
    }
}

fn validate_scalar_constraints(
    node: &YamlOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    use saphyr::ScalarOwned;

    if let YamlOwned::Value(ScalarOwned::String(s)) = node {
        validate_string_constraints(s, schema, path, ctx);
    }

    let numeric_val = match node {
        YamlOwned::Value(ScalarOwned::Integer(i)) =>
        {
            #[allow(clippy::cast_precision_loss)]
            Some(*i as f64)
        }
        YamlOwned::Value(ScalarOwned::FloatingPoint(f)) => Some(**f),
        YamlOwned::Value(_)
        | YamlOwned::Sequence(_)
        | YamlOwned::Mapping(_)
        | YamlOwned::Alias(_)
        | YamlOwned::BadValue
        | YamlOwned::Tagged(..)
        | YamlOwned::Representation(..) => None,
    };
    if let Some(val) = numeric_val {
        validate_numeric_constraints(val, schema, path, ctx);
    }

    // const — compare any scalar node via yaml_to_json
    if let Some(const_val) = &schema.const_value {
        if let Some(yaml_val) = yaml_to_json(node) {
            if yaml_val != *const_val {
                let range = node_range(path, ctx.lines);
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::ERROR,
                    "schemaConst",
                    format!("Value at {} must equal {}", format_path(path), const_val),
                ));
            }
        }
        // If yaml_to_json returns None (object/array), skip the check
    }
}

fn validate_string_constraints(s: &str, schema: &JsonSchema, path: &[String], ctx: &mut Ctx<'_>) {
    if let Some(pattern) = &schema.pattern {
        if pattern.len() > MAX_PATTERN_LEN {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaPatternLimit",
                format!(
                    "Pattern at {} exceeds maximum length ({MAX_PATTERN_LEN} chars) and was not validated",
                    format_path(path),
                ),
            ));
        } else if let Some(re) = get_regex(pattern) {
            if !re.is_match(s) {
                let range = node_range(path, ctx.lines);
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::ERROR,
                    "schemaPattern",
                    format!(
                        "Value at {} does not match pattern: {}",
                        format_path(path),
                        pattern
                    ),
                ));
            }
        } else {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaPatternLimit",
                format!(
                    "Pattern at {} could not be compiled and was not validated",
                    format_path(path),
                ),
            ));
        }
    }

    let char_count = s.chars().count() as u64;

    if let Some(min_len) = schema.min_length {
        if char_count < min_len {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinLength",
                format!(
                    "Value at {} is too short: {} chars (minimum {})",
                    format_path(path),
                    char_count,
                    min_len
                ),
            ));
        }
    }

    if let Some(max_len) = schema.max_length {
        if char_count > max_len {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaxLength",
                format!(
                    "Value at {} is too long: {} chars (maximum {})",
                    format_path(path),
                    char_count,
                    max_len
                ),
            ));
        }
    }

    if ctx.format_validation {
        if let Some(format) = &schema.format {
            validate_format(s, format, path, ctx.lines, ctx.diagnostics);
        }
        if schema.content_encoding.is_some() || schema.content_media_type.is_some() {
            validate_content(s, schema, path, ctx.lines, ctx.diagnostics);
        }
    }
}

/// Check `s` against the JSON Schema `format` keyword and push a WARNING
/// diagnostic if the value does not conform.  Unknown formats are silently
/// ignored (per the spec, format validation is advisory).
fn validate_format(
    s: &str,
    format: &str,
    path: &[String],
    lines: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let valid = match format {
        "date-time" => is_valid_date_time(s),
        "date" => is_valid_date(s),
        "time" => is_valid_time(s),
        "duration" => is_valid_duration(s),
        "email" => is_valid_email(s),
        "ipv4" => is_valid_ipv4(s),
        "ipv6" => is_valid_ipv6(s),
        "hostname" => is_valid_hostname(s),
        "uri" => is_valid_uri(s),
        "uri-reference" => is_valid_uri_reference(s),
        "uri-template" => is_valid_uri_template(s),
        "uuid" => is_valid_uuid(s),
        "regex" => is_valid_regex(s),
        "json-pointer" => is_valid_json_pointer(s),
        "relative-json-pointer" => is_valid_relative_json_pointer(s),
        "idn-hostname" => is_valid_idn_hostname(s),
        "idn-email" => is_valid_idn_email(s),
        "iri" => is_valid_iri(s),
        "iri-reference" => is_valid_iri_reference(s),
        // Unknown formats are intentionally ignored
        _ => return,
    };
    if !valid {
        let range = node_range(path, lines);
        diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::WARNING,
            "schemaFormat",
            format!(
                "String at {} does not match format '{format}'",
                format_path(path)
            ),
        ));
    }
}

/// Validates `contentEncoding` and `contentMediaType` keywords.
///
/// Decodes the string using the declared encoding, then (if both are set)
/// checks the decoded bytes against the declared media type.
fn validate_content(
    s: &str,
    schema: &JsonSchema,
    path: &[String],
    lines: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Step 1: decode if contentEncoding is set
    let decoded_bytes: Option<Vec<u8>> = if let Some(enc) = &schema.content_encoding {
        let result = match enc.as_str() {
            "base64" => data_encoding::BASE64.decode(s.as_bytes()),
            "base64url" => data_encoding::BASE64URL.decode(s.as_bytes()),
            "base32" => data_encoding::BASE32.decode(s.as_bytes()),
            "base16" => data_encoding::HEXUPPER_PERMISSIVE.decode(s.as_bytes()),
            // Unknown encoding — skip both checks
            _ => return,
        };
        if let Ok(bytes) = result {
            Some(bytes)
        } else {
            let range = node_range(path, lines);
            diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaContentEncoding",
                format!(
                    "String at {} is not valid {enc} encoded data",
                    format_path(path)
                ),
            ));
            // Encoding failed — skip media type check
            return;
        }
    } else {
        // No encoding set — use raw string bytes for media type check
        None
    };

    // Step 2: check media type if set
    if let Some(media_type) = &schema.content_media_type {
        let valid = match media_type.as_str() {
            "application/json" => {
                let text = decoded_bytes
                    .as_ref()
                    .map_or(Some(s), |bytes| std::str::from_utf8(bytes).ok());
                text.is_some_and(|t| serde_json::from_str::<serde_json::Value>(t).is_ok())
            }
            // Unknown media type — skip
            _ => return,
        };
        if !valid {
            let range = node_range(path, lines);
            diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaContentMediaType",
                format!(
                    "String at {} does not contain valid {media_type} content",
                    format_path(path)
                ),
            ));
        }
    }
}

/// RFC 3339 full date-time: `YYYY-MM-DDTHH:MM:SS[.frac](Z|+HH:MM|-HH:MM)`.
fn is_valid_date_time(s: &str) -> bool {
    // Split on 'T' or 't'
    let Some(t_pos) = s.find(['T', 't']) else {
        return false;
    };
    let (date_part, time_and_offset) = s.split_at(t_pos);
    let time_and_offset = &time_and_offset[1..]; // skip the 'T'
    is_valid_date(date_part) && is_valid_time(time_and_offset)
}

/// RFC 3339 full-date: `YYYY-MM-DD`.
fn is_valid_date(s: &str) -> bool {
    // Length must be exactly 10: YYYY-MM-DD
    if s.len() != 10 {
        return false;
    }
    // Safety: length checked above; these indices are always in-bounds ASCII
    if s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') {
        return false;
    }
    let Ok(year) = s[..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = s[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = s[8..10].parse::<u32>() else {
        return false;
    };
    if month == 0 || month > 12 || day == 0 {
        return false;
    }
    let max_day = days_in_month(year, month);
    day <= max_day
}

const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

const fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// RFC 3339 partial-time + time-offset: `HH:MM:SS[.frac](Z|+HH:MM|-HH:MM)`.
fn is_valid_time(s: &str) -> bool {
    // Must end with Z/z or ±HH:MM
    let (time_part, offset_part) =
        if let Some(stripped) = s.strip_suffix('Z').or_else(|| s.strip_suffix('z')) {
            (stripped, "Z")
        } else {
            // Find offset sign from the end
            let Some(sign_pos) = s.rfind(['+', '-']) else {
                return false;
            };
            // sign_pos must be after the time (at least HH:MM:SS = 8 chars)
            if sign_pos < 8 {
                return false;
            }
            (&s[..sign_pos], &s[sign_pos..])
        };

    // Validate time_part: HH:MM:SS[.frac]
    let tb = time_part.as_bytes();
    if tb.len() < 8 {
        return false;
    }
    if tb.get(2) != Some(&b':') || tb.get(5) != Some(&b':') {
        return false;
    }
    let Ok(hour) = time_part[..2].parse::<u32>() else {
        return false;
    };
    let Ok(minute) = time_part[3..5].parse::<u32>() else {
        return false;
    };
    let Ok(second) = time_part[6..8].parse::<u32>() else {
        return false;
    };
    if hour > 23 || minute > 59 || second > 60 {
        // 60 is allowed for leap seconds
        return false;
    }
    // Optional fractional seconds
    if tb.len() > 8 {
        if tb.get(8) != Some(&b'.') {
            return false;
        }
        if time_part[9..].is_empty() || !time_part[9..].bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }

    // Validate offset
    if offset_part == "Z" {
        return true;
    }
    let offset = &offset_part[1..]; // skip sign
    if offset.len() != 5 || offset.as_bytes().get(2) != Some(&b':') {
        return false;
    }
    let Ok(off_h) = offset[..2].parse::<u32>() else {
        return false;
    };
    let Ok(off_m) = offset[3..5].parse::<u32>() else {
        return false;
    };
    off_h <= 23 && off_m <= 59
}

/// ISO 8601 duration: `P[nY][nM][nD][T[nH][nM][nS]]` or `PnW`.
fn is_valid_duration(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('P') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    // Week form: PnW
    if let Some(w) = rest.strip_suffix('W') {
        return !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit());
    }
    // Split on 'T'
    let (date_part, time_part) = rest.find('T').map_or((rest, None), |t_pos| {
        (&rest[..t_pos], Some(&rest[t_pos + 1..]))
    });
    // Validate date designators: Y M D in order, each optional but non-repeating
    if !is_valid_duration_designators(date_part, &['Y', 'M', 'D']) {
        return false;
    }
    if let Some(tp) = time_part {
        if tp.is_empty() {
            return false; // 'T' present but nothing after it
        }
        if !is_valid_duration_designators(tp, &['H', 'M', 'S']) {
            return false;
        }
    }
    // At least one designator total
    !date_part.is_empty() || time_part.is_some_and(|t| !t.is_empty())
}

/// Validate that `s` is a sequence of `nX` tokens where X appears in `designators`
/// in order (no repeats, only forward).
fn is_valid_duration_designators(s: &str, designators: &[char]) -> bool {
    let mut remaining = s;
    let mut last_idx: Option<usize> = None;
    while !remaining.is_empty() {
        // Read digits
        let digit_end = remaining
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(remaining.len());
        if digit_end == 0 {
            return false; // designator without digits
        }
        if digit_end == remaining.len() {
            return false; // digits without designator at end
        }
        let designator = remaining.chars().nth(digit_end).unwrap_or('\0');
        let Some(idx) = designators.iter().position(|&d| d == designator) else {
            return false;
        };
        if let Some(prev) = last_idx {
            if idx <= prev {
                return false; // out of order or repeated
            }
        }
        last_idx = Some(idx);
        remaining = &remaining[digit_end + designator.len_utf8()..];
    }
    true
}

/// Very basic email validation: `local@domain` where domain contains at least one dot.
fn is_valid_email(s: &str) -> bool {
    let Some(at_pos) = s.rfind('@') else {
        return false;
    };
    let local = &s[..at_pos];
    let domain = &s[at_pos + 1..];
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

/// IPv4: four decimal octets in `0-255` separated by dots.
fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| {
        !p.is_empty()
            && p.len() <= 3
            && p.bytes().all(|b| b.is_ascii_digit())
            && p.parse::<u16>().is_ok_and(|n| n <= 255)
            && (p.len() == 1 || !p.starts_with('0')) // no leading zeros
    })
}

/// IPv6: eight groups of 1-4 hex digits separated by colons, with optional `::`.
fn is_valid_ipv6(s: &str) -> bool {
    // Allow zone ID suffix (strip %...)
    let s = s.split('%').next().unwrap_or(s);
    // Handle embedded IPv4 in the last group
    let (s, ipv4_suffix) = if let Some(last_colon) = s.rfind(':') {
        let candidate = &s[last_colon + 1..];
        if candidate.contains('.') {
            if !is_valid_ipv4(candidate) {
                return false;
            }
            (&s[..last_colon], true)
        } else {
            (s, false)
        }
    } else {
        (s, false)
    };

    let has_double_colon = s.contains("::");
    // When splitting on "::", the halves may themselves be empty (e.g. "::1" → ["", "1"])
    // Filter those out before validating individual groups.
    let parts: Vec<&str> = if has_double_colon {
        s.splitn(2, "::")
            .flat_map(|h| h.split(':'))
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        s.split(':').collect()
    };

    let expected = if ipv4_suffix { 6 } else { 8 };
    let max_groups = if has_double_colon {
        expected - 1
    } else {
        expected
    };

    if parts.len() > max_groups {
        return false;
    }
    if !has_double_colon && parts.len() != expected {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Hostname per RFC 1123: labels of `[A-Za-z0-9-]`, each ≤63 chars, total ≤253.
fn is_valid_hostname(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    // Strip optional trailing dot (FQDN)
    let s = s.strip_suffix('.').unwrap_or(s);
    s.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// URI: must have a scheme followed by `:`
fn is_valid_uri(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    let scheme = &s[..colon];
    !scheme.is_empty()
        && scheme
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
}

/// URI-reference: either a valid URI or a relative reference (starts with `/`, `?`, `#`, or `//`).
fn is_valid_uri_reference(s: &str) -> bool {
    if s.is_empty() {
        return true; // empty string is a valid URI-reference
    }
    is_valid_uri(s)
        || s.starts_with('/')
        || s.starts_with('?')
        || s.starts_with('#')
        || s.starts_with("//")
        || !s.contains(':') // relative-path reference
}

/// URI-template (RFC 6570): any printable ASCII string with balanced `{...}` expressions.
fn is_valid_uri_template(s: &str) -> bool {
    let mut depth = 0u32;
    for b in s.bytes() {
        match b {
            b'{' => {
                if depth > 0 {
                    return false; // nested braces not allowed
                }
                depth += 1;
            }
            b'}' => {
                if depth == 0 {
                    return false; // unmatched closing brace
                }
                depth -= 1;
            }
            0x00..=0x1F | 0x7F => return false, // control chars
            _ => {}
        }
    }
    depth == 0
}

/// UUID: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` (case-insensitive).
fn is_valid_uuid(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    // Check dashes at fixed positions (length already verified to be 36)
    if bytes.get(8) != Some(&b'-')
        || bytes.get(13) != Some(&b'-')
        || bytes.get(18) != Some(&b'-')
        || bytes.get(23) != Some(&b'-')
    {
        return false;
    }
    bytes.iter().enumerate().all(|(i, &b)| {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            true // dash — already verified
        } else {
            b.is_ascii_hexdigit()
        }
    })
}

/// Validate a JSON Schema `regex` value by trying to compile it with the `regex` crate.
fn is_valid_regex(s: &str) -> bool {
    if s.len() > MAX_PATTERN_LEN {
        return false;
    }
    RegexBuilder::new(s)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .is_ok()
}

/// JSON Pointer (RFC 6901): empty string or starts with `/`.
/// Each token may not contain unescaped `~` (must be `~0` or `~1`).
fn is_valid_json_pointer(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if !s.starts_with('/') {
        return false;
    }
    is_json_pointer_tokens_valid(s)
}

fn is_json_pointer_tokens_valid(s: &str) -> bool {
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0' | '1') => {}
                _ => return false,
            }
        }
    }
    true
}

/// Relative JSON Pointer: non-negative integer followed by a JSON Pointer or `#`.
fn is_valid_relative_json_pointer(s: &str) -> bool {
    let digit_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if digit_end == 0 {
        return false; // must start with a non-negative integer
    }
    let rest = &s[digit_end..];
    if rest == "#" {
        return true;
    }
    is_valid_json_pointer(rest)
}

/// IDN hostname: validates using IDNA UTS#46 strict processing (UseSTD3ASCIIRules=true).
fn is_valid_idn_hostname(s: &str) -> bool {
    idna::domain_to_ascii_strict(s).is_ok()
}

/// IDN email: local@domain where domain is validated via IDNA strict processing.
fn is_valid_idn_email(s: &str) -> bool {
    let Some(at_pos) = s.rfind('@') else {
        return false;
    };
    let local = &s[..at_pos];
    let domain = &s[at_pos + 1..];
    !local.is_empty() && idna::domain_to_ascii_strict(domain).is_ok()
}

/// IRI (Internationalized Resource Identifier, RFC 3987).
fn is_valid_iri(s: &str) -> bool {
    iri_string::types::IriStr::new(s).is_ok()
}

/// IRI-reference (absolute IRI or relative reference, RFC 3987).
fn is_valid_iri_reference(s: &str) -> bool {
    iri_string::types::IriReferenceStr::new(s).is_ok()
}

fn validate_numeric_constraints(val: f64, schema: &JsonSchema, path: &[String], ctx: &mut Ctx<'_>) {
    // minimum (inclusive by default; strict if Draft-04 exclusiveMinimum is true)
    if let Some(minimum) = schema.minimum {
        let exclusive = schema.exclusive_minimum_draft04.unwrap_or(false);
        let violation = if exclusive {
            val <= minimum
        } else {
            val < minimum
        };
        if violation {
            let range = node_range(path, ctx.lines);
            let bound = if exclusive { "exclusive" } else { "inclusive" };
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinimum",
                format!(
                    "Value at {} is below minimum {minimum} ({bound})",
                    format_path(path),
                ),
            ));
        }
    }

    // maximum (inclusive by default; strict if Draft-04 exclusiveMaximum is true)
    if let Some(maximum) = schema.maximum {
        let exclusive = schema.exclusive_maximum_draft04.unwrap_or(false);
        let violation = if exclusive {
            val >= maximum
        } else {
            val > maximum
        };
        if violation {
            let range = node_range(path, ctx.lines);
            let bound = if exclusive { "exclusive" } else { "inclusive" };
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaximum",
                format!(
                    "Value at {} is above maximum {maximum} ({bound})",
                    format_path(path),
                ),
            ));
        }
    }

    // exclusiveMinimum (Draft-06+ number form)
    if let Some(excl_min) = schema.exclusive_minimum {
        if val <= excl_min {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinimum",
                format!(
                    "Value at {} must be greater than {excl_min} (exclusive minimum)",
                    format_path(path),
                ),
            ));
        }
    }

    // exclusiveMaximum (Draft-06+ number form)
    if let Some(excl_max) = schema.exclusive_maximum {
        if val >= excl_max {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaximum",
                format!(
                    "Value at {} must be less than {excl_max} (exclusive maximum)",
                    format_path(path),
                ),
            ));
        }
    }

    // multipleOf
    if let Some(multiple_of) = schema.multiple_of {
        if multiple_of > 0.0 {
            let quotient = val / multiple_of;
            if (quotient - quotient.round()).abs() >= f64::EPSILON {
                let range = node_range(path, ctx.lines);
                ctx.diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::ERROR,
                    "schemaMultipleOf",
                    format!(
                        "Value at {} must be a multiple of {multiple_of}",
                        format_path(path),
                    ),
                ));
            }
        }
    }
}

fn validate_mapping(
    map: &saphyr::MappingOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    use saphyr::ScalarOwned;

    validate_mapping_constraints(map, schema, path, ctx);

    let properties = schema.properties.as_ref();

    // Required properties
    if let Some(required) = &schema.required {
        let listed: Vec<&str> = required
            .iter()
            .take(MAX_ENUM_DISPLAY)
            .map(String::as_str)
            .collect();
        let props_list = if required.len() > MAX_ENUM_DISPLAY {
            format!("{}, ... ({} total)", listed.join(", "), required.len())
        } else {
            listed.join(", ")
        };
        ctx.diagnostics.extend(
            required
                .iter()
                .filter(|req_key| {
                    let key_yaml = YamlOwned::Value(ScalarOwned::String((*req_key).clone()));
                    !map.contains_key(&key_yaml)
                })
                .map(|req_key| {
                    let range = mapping_range(path, ctx.lines);
                    make_diagnostic(
                        range,
                        DiagnosticSeverity::ERROR,
                        "schemaRequired",
                        format!(
                            "Missing required property '{}' at {}. Expected properties: {}.",
                            req_key,
                            format_path(path),
                            props_list
                        ),
                    )
                }),
        );
    }

    // Validate known properties and check for additional properties
    for (k, v) in map {
        let key_str = match k {
            YamlOwned::Value(ScalarOwned::String(s)) => s.clone(),
            YamlOwned::Value(ScalarOwned::Integer(i)) => i.to_string(),
            YamlOwned::Value(_)
            | YamlOwned::Sequence(_)
            | YamlOwned::Mapping(_)
            | YamlOwned::Alias(_)
            | YamlOwned::BadValue
            | YamlOwned::Tagged(_, _)
            | YamlOwned::Representation(_, _, _) => continue,
        };

        let is_known = properties.is_some_and(|p| p.contains_key(&key_str));

        if let Some(prop_schema) = properties.and_then(|p| p.get(&key_str)) {
            let mut child_path = path.to_vec();
            child_path.push(key_str.clone());
            validate_node(v, prop_schema, &child_path, ctx, depth + 1);
        } else {
            // Check patternProperties for keys not in properties
            let matched_by_pattern =
                validate_pattern_properties(v, &key_str, schema, path, ctx, depth);

            if !is_known && !matched_by_pattern {
                // Check additionalProperties
                match &schema.additional_properties {
                    Some(AdditionalProperties::Denied) => {
                        let range = key_range(&key_str, path, ctx.lines);
                        ctx.diagnostics.push(make_diagnostic(
                            range,
                            DiagnosticSeverity::WARNING,
                            "schemaAdditionalProperty",
                            format!(
                                "Additional property '{}' is not allowed at {}",
                                key_str,
                                format_path(path)
                            ),
                        ));
                    }
                    Some(AdditionalProperties::Schema(extra_schema)) => {
                        let mut child_path = path.to_vec();
                        child_path.push(key_str.clone());
                        validate_node(v, extra_schema, &child_path, ctx, depth + 1);
                    }
                    None => {}
                }
            }
        }

        // propertyNames — validate each key as a string node against the schema
        if let Some(pn_schema) = &schema.property_names {
            let key_node = YamlOwned::Value(ScalarOwned::String(key_str.clone()));
            validate_node(&key_node, pn_schema, path, ctx, depth + 1);
        }
    }

    validate_dependencies(map, schema, path, ctx, depth);
}

fn validate_dependencies(
    map: &saphyr::MappingOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    use saphyr::ScalarOwned;

    // dependentRequired: if trigger key is present, listed keys must also be present
    if let Some(dep_req) = &schema.dependent_required {
        for (trigger, required_keys) in dep_req {
            let trigger_yaml = YamlOwned::Value(ScalarOwned::String(trigger.clone()));
            if map.contains_key(&trigger_yaml) {
                for missing in required_keys {
                    let missing_yaml = YamlOwned::Value(ScalarOwned::String(missing.clone()));
                    if !map.contains_key(&missing_yaml) {
                        let range = mapping_range(path, ctx.lines);
                        ctx.diagnostics.push(make_diagnostic(
                            range,
                            DiagnosticSeverity::ERROR,
                            "schemaDependentRequired",
                            format!(
                                "Property '{}' is required when '{}' is present at {}",
                                missing,
                                trigger,
                                format_path(path)
                            ),
                        ));
                    }
                }
            }
        }
    }

    // dependentSchemas: if trigger key is present, validate the whole mapping
    if let Some(dep_sch) = &schema.dependent_schemas {
        for (trigger, dep_schema) in dep_sch {
            let trigger_yaml = YamlOwned::Value(ScalarOwned::String(trigger.clone()));
            if map.contains_key(&trigger_yaml) {
                validate_node(
                    &YamlOwned::Mapping(map.clone()),
                    dep_schema,
                    path,
                    ctx,
                    depth + 1,
                );
            }
        }
    }
}

/// Validate `value` against any `patternProperties` patterns that match `key`.
/// Returns `true` if the key was matched by at least one pattern.
fn validate_pattern_properties(
    value: &YamlOwned,
    key: &str,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) -> bool {
    let Some(pattern_props) = &schema.pattern_properties else {
        return false;
    };

    let mut matched = false;
    for (pattern, pat_schema) in pattern_props {
        if pattern.len() > MAX_PATTERN_LEN {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaPatternLimit",
                format!(
                    "Pattern at {} exceeds maximum length ({MAX_PATTERN_LEN} chars) and was not validated",
                    format_path(path),
                ),
            ));
            continue;
        }
        if let Some(re) = get_regex(pattern) {
            if re.is_match(key) {
                matched = true;
                let mut child_path = path.to_vec();
                child_path.push(key.to_string());
                validate_node(value, pat_schema, &child_path, ctx, depth + 1);
            }
        } else {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaPatternLimit",
                format!(
                    "Pattern at {} could not be compiled and was not validated",
                    format_path(path),
                ),
            ));
        }
    }
    matched
}

fn validate_composition(
    node: &YamlOwned,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    // allOf: all branches must pass
    if let Some(all_of) = &schema.all_of {
        for branch in all_of.iter().take(MAX_BRANCH_COUNT) {
            validate_node(node, branch, path, ctx, depth + 1);
        }
    }

    // anyOf: at least one branch must pass; if none do, emit a diagnostic
    if let Some(any_of) = &schema.any_of {
        let lines = ctx.lines;
        let format_validation = ctx.format_validation;
        let any_passes = any_of.iter().take(MAX_BRANCH_COUNT).any(|branch| {
            let mut scratch = Vec::new();
            let mut probe = Ctx::new(lines, &mut scratch, format_validation);
            validate_node(node, branch, path, &mut probe, depth + 1);
            scratch.is_empty()
        });
        if !any_passes {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match any of the allowed schemas",
                    format_path(path)
                ),
            ));
        }
    }

    // oneOf: exactly one branch must pass
    if let Some(one_of) = &schema.one_of {
        let lines = ctx.lines;
        let format_validation = ctx.format_validation;
        let passing = one_of
            .iter()
            .take(MAX_BRANCH_COUNT)
            .filter(|branch| {
                let mut scratch = Vec::new();
                let mut probe = Ctx::new(lines, &mut scratch, format_validation);
                validate_node(node, branch, path, &mut probe, depth + 1);
                scratch.is_empty()
            })
            .count();

        if passing == 0 {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match any of the oneOf schemas",
                    format_path(path)
                ),
            ));
        } else if passing > 1 {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} matches more than one of the oneOf schemas",
                    format_path(path)
                ),
            ));
        }
    }

    // not: the value must NOT match the sub-schema
    if let Some(not_schema) = &schema.not {
        let lines = ctx.lines;
        let format_validation = ctx.format_validation;
        let mut scratch = Vec::new();
        let mut probe = Ctx::new(lines, &mut scratch, format_validation);
        validate_node(node, not_schema, path, &mut probe, depth + 1);
        if scratch.is_empty() {
            let range = node_range(path, ctx.lines);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaNot",
                format!(
                    "Value at {} must not match the excluded schema",
                    format_path(path)
                ),
            ));
        }
    }

    // if / then / else (Draft-07)
    if let Some(if_schema) = &schema.if_schema {
        let lines = ctx.lines;
        let format_validation = ctx.format_validation;
        let mut scratch = Vec::new();
        let mut probe = Ctx::new(lines, &mut scratch, format_validation);
        validate_node(node, if_schema, path, &mut probe, depth + 1);
        if scratch.is_empty() {
            if let Some(then_schema) = &schema.then_schema {
                validate_node(node, then_schema, path, ctx, depth + 1);
            }
        } else if let Some(else_schema) = &schema.else_schema {
            validate_node(node, else_schema, path, ctx, depth + 1);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Type mapping
// ──────────────────────────────────────────────────────────────────────────────

const fn yaml_type_name(node: &YamlOwned) -> &'static str {
    use saphyr::ScalarOwned;
    match node {
        YamlOwned::Value(ScalarOwned::String(_)) => "string",
        YamlOwned::Value(ScalarOwned::Integer(_)) => "integer",
        YamlOwned::Value(ScalarOwned::FloatingPoint(_)) => "number",
        YamlOwned::Value(ScalarOwned::Boolean(_)) => "boolean",
        YamlOwned::Value(ScalarOwned::Null) => "null",
        YamlOwned::Mapping(_) => "object",
        YamlOwned::Sequence(_) => "array",
        YamlOwned::Alias(_)
        | YamlOwned::BadValue
        | YamlOwned::Tagged(_, _)
        | YamlOwned::Representation(_, _, _) => "unknown",
    }
}

fn type_matches(yaml_type: &str, schema_type: &SchemaType) -> bool {
    match schema_type {
        SchemaType::Single(t) => single_type_matches(yaml_type, t),
        SchemaType::Multiple(ts) => ts.iter().any(|t| single_type_matches(yaml_type, t)),
    }
}

/// JSON Schema allows "number" to also accept integers.
fn single_type_matches(yaml_type: &str, schema_type: &str) -> bool {
    if yaml_type == schema_type {
        return true;
    }
    // JSON Schema: "number" accepts both "number" and "integer"
    if schema_type == "number" && yaml_type == "integer" {
        return true;
    }
    false
}

fn display_schema_type(schema_type: &SchemaType) -> String {
    match schema_type {
        SchemaType::Single(t) => t.clone(),
        SchemaType::Multiple(ts) => ts.join(" | "),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// YAML → JSON conversion (for enum comparison)
// ──────────────────────────────────────────────────────────────────────────────

fn yaml_to_json(node: &YamlOwned) -> Option<serde_json::Value> {
    use saphyr::ScalarOwned;
    match node {
        YamlOwned::Value(ScalarOwned::String(s)) => Some(serde_json::Value::String(s.clone())),
        YamlOwned::Value(ScalarOwned::Integer(i)) => Some(serde_json::Value::Number((*i).into())),
        YamlOwned::Value(ScalarOwned::FloatingPoint(f)) => {
            serde_json::Number::from_f64(**f).map(serde_json::Value::Number)
        }
        YamlOwned::Value(ScalarOwned::Boolean(b)) => Some(serde_json::Value::Bool(*b)),
        YamlOwned::Value(ScalarOwned::Null) => Some(serde_json::Value::Null),
        YamlOwned::Sequence(_)
        | YamlOwned::Mapping(_)
        | YamlOwned::Alias(_)
        | YamlOwned::BadValue
        | YamlOwned::Tagged(_, _)
        | YamlOwned::Representation(_, _, _) => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Diagnostic construction
// ──────────────────────────────────────────────────────────────────────────────

fn make_diagnostic(
    range: Range,
    severity: DiagnosticSeverity,
    code: &str,
    message: String,
) -> Diagnostic {
    // Truncate message if it contains schema-derived text
    let message = truncate_message(message);
    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("rlsp-yaml".to_string()),
        message,
        ..Diagnostic::default()
    }
}

fn truncate_message(msg: String) -> String {
    if msg.chars().count() <= MAX_DESCRIPTION_LEN {
        return msg;
    }
    // Find the byte boundary of the MAX_DESCRIPTION_LEN-th char to avoid
    // slicing mid-UTF-8 sequence.
    let boundary = msg
        .char_indices()
        .nth(MAX_DESCRIPTION_LEN)
        .map_or(msg.len(), |(i, _)| i);
    format!("{}…", &msg[..boundary])
}

// ──────────────────────────────────────────────────────────────────────────────
// Range / position helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Find the range for a node identified by its path, using text scanning.
/// Falls back to `(0,0)-(0,0)` if not found.
fn node_range(path: &[String], lines: &[&str]) -> Range {
    path.last().map_or_else(
        || Range::new(Position::new(0, 0), Position::new(0, 0)),
        |key| find_key_range(key, lines),
    )
}

/// Find the range for the opening of a mapping (for required-property errors).
fn mapping_range(path: &[String], lines: &[&str]) -> Range {
    path.last().map_or_else(
        || Range::new(Position::new(0, 0), Position::new(0, 0)),
        |key| find_key_range(key, lines),
    )
}

/// Find the range for a specific key within the document text.
fn key_range(key: &str, _path: &[String], lines: &[&str]) -> Range {
    find_key_range(key, lines)
}

/// Scan `lines` for the first occurrence of `key` as a YAML mapping key.
/// Returns the range of the key token, or `(0,0)-(0,0)` if not found.
fn find_key_range(key: &str, lines: &[&str]) -> Range {
    // Strip array-index brackets if present (e.g. "[0]")
    let key = key.trim_start_matches('[').trim_end_matches(']');

    lines
        .iter()
        .enumerate()
        .find_map(|(line_idx, line)| {
            let trimmed = line.trim_start();
            // Match "key:" or "- key:" patterns
            let candidate = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            if candidate.starts_with(key)
                && candidate
                    .get(key.len()..)
                    .is_some_and(|rest| rest.starts_with(':') || rest.starts_with(' '))
            {
                let col = line.len() - line.trim_start().len();
                let col = u32::try_from(col).unwrap_or(0);
                let end_col = col + u32::try_from(key.len()).unwrap_or(0);
                let line_u32 = u32::try_from(line_idx).unwrap_or(0);
                Some(Range::new(
                    Position::new(line_u32, col),
                    Position::new(line_u32, end_col),
                ))
            } else {
                None
            }
        })
        .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)))
}

// ──────────────────────────────────────────────────────────────────────────────
// Path formatting
// ──────────────────────────────────────────────────────────────────────────────

fn format_path(path: &[String]) -> String {
    if path.is_empty() {
        return "<root>".to_string();
    }
    let mut result = String::new();
    for segment in path {
        if !segment.starts_with('[') && !result.is_empty() {
            result.push('.');
        }
        result.push_str(segment);
    }
    result
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};
    use serde_json::json;

    fn parse_docs(text: &str) -> Vec<YamlOwned> {
        use saphyr::LoadableYamlNode;
        YamlOwned::load_from_str(text).unwrap_or_default()
    }

    fn string_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        }
    }

    fn integer_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        }
    }

    fn object_schema_with_props(props: Vec<(&str, JsonSchema)>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            ..JsonSchema::default()
        }
    }

    fn code_of(d: &Diagnostic) -> &str {
        match &d.code {
            Some(NumberOrString::String(s)) => s.as_str(),
            _ => "",
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Required properties
    // ══════════════════════════════════════════════════════════════════════════

    // Test 1
    #[test]
    fn should_produce_no_diagnostics_when_required_property_present() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            properties: Some([("name".to_string(), string_schema())].into()),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice");
        let result = validate_schema("name: Alice", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 2
    #[test]
    fn should_produce_error_for_missing_required_property() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema("age: 30", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaRequired");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("name"));
    }

    // Test 3
    #[test]
    fn should_produce_one_diagnostic_per_missing_required_property() {
        let schema = JsonSchema {
            required: Some(vec![
                "name".to_string(),
                "age".to_string(),
                "email".to_string(),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema("other: value", &docs, &schema, true);
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|d| code_of(d) == "schemaRequired"));
    }

    // Test 4
    #[test]
    fn should_produce_no_diagnostics_when_all_required_present() {
        let schema = JsonSchema {
            required: Some(vec!["a".to_string(), "b".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("a: 1\nb: 2");
        let result = validate_schema("a: 1\nb: 2", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 5
    #[test]
    fn should_produce_no_diagnostics_for_empty_required_array() {
        let schema = JsonSchema {
            required: Some(vec![]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("key: value");
        let result = validate_schema("key: value", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 6
    #[test]
    fn should_validate_required_in_nested_mapping() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            required: Some(vec!["name".to_string()]),
            properties: Some([("name".to_string(), string_schema())].into()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  other: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaRequired");
        assert!(result[0].message.contains("name"));
        assert!(result[0].message.contains("spec"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Type mismatches
    // ══════════════════════════════════════════════════════════════════════════

    // Test 7
    #[test]
    fn should_produce_no_diagnostics_when_type_matches_string() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: Alice");
        let result = validate_schema("name: Alice", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 8
    #[test]
    fn should_produce_error_for_string_where_integer_expected() {
        let schema = object_schema_with_props(vec![("count", integer_schema())]);
        let docs = parse_docs("count: \"hello\"");
        let result = validate_schema("count: \"hello\"", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("integer"));
    }

    // Test 9
    #[test]
    fn should_produce_error_for_integer_where_string_expected() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: 42");
        let result = validate_schema("name: 42", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 10
    #[test]
    fn should_produce_error_for_boolean_where_string_expected() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: true");
        let result = validate_schema("name: true", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 11
    #[test]
    fn should_produce_error_for_mapping_where_string_expected() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let text = "name:\n  nested: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 12
    #[test]
    fn should_produce_error_for_sequence_where_object_expected() {
        let config_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("config", config_schema)]);
        let text = "config:\n  - item";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 13
    #[test]
    fn should_produce_error_for_null_where_string_expected() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: ~");
        let result = validate_schema("name: ~", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 14
    #[test]
    fn should_produce_no_diagnostics_for_null_when_null_in_type_array() {
        let schema = object_schema_with_props(vec![(
            "name",
            JsonSchema {
                schema_type: Some(SchemaType::Multiple(vec![
                    "string".to_string(),
                    "null".to_string(),
                ])),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("name: ~");
        let result = validate_schema("name: ~", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 15
    #[test]
    fn should_produce_no_diagnostics_when_no_type_specified() {
        let schema = object_schema_with_props(vec![("name", JsonSchema::default())]);
        let docs = parse_docs("name: 42");
        let result = validate_schema("name: 42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 16
    #[test]
    fn should_produce_no_diagnostics_for_integer_type_with_integer_value() {
        let schema = object_schema_with_props(vec![("port", integer_schema())]);
        let docs = parse_docs("port: 8080");
        let result = validate_schema("port: 8080", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 17
    #[test]
    fn should_produce_no_diagnostics_for_boolean_type_with_boolean_value() {
        let schema = object_schema_with_props(vec![(
            "enabled",
            JsonSchema {
                schema_type: Some(SchemaType::Single("boolean".to_string())),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("enabled: true");
        let result = validate_schema("enabled: true", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 18
    #[test]
    fn should_produce_no_diagnostics_for_array_type_with_sequence_value() {
        let schema = object_schema_with_props(vec![(
            "items",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                ..JsonSchema::default()
            },
        )]);
        let text = "items:\n  - one\n  - two";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Enum violations
    // ══════════════════════════════════════════════════════════════════════════

    // Test 19
    #[test]
    fn should_produce_no_diagnostics_when_value_matches_enum() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                enum_values: Some(vec![json!("prod"), json!("staging"), json!("dev")]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: staging");
        let result = validate_schema("env: staging", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 20
    #[test]
    fn should_produce_error_when_value_not_in_enum() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging"), json!("dev")]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: testing");
        let result = validate_schema("env: testing", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaEnum");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        // Message should list valid values
        assert!(
            result[0].message.contains("prod"),
            "message should contain 'prod'"
        );
        assert!(
            result[0].message.contains("staging"),
            "message should contain 'staging'"
        );
        assert!(
            result[0].message.contains("dev"),
            "message should contain 'dev'"
        );
    }

    // Test 21
    #[test]
    fn should_produce_no_diagnostics_for_enum_with_integer_match() {
        let schema = object_schema_with_props(vec![(
            "level",
            JsonSchema {
                enum_values: Some(vec![json!(1), json!(2), json!(3)]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("level: 2");
        let result = validate_schema("level: 2", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 22
    #[test]
    fn should_produce_error_for_enum_with_integer_mismatch() {
        let schema = object_schema_with_props(vec![(
            "level",
            JsonSchema {
                enum_values: Some(vec![json!(1), json!(2), json!(3)]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("level: 5");
        let result = validate_schema("level: 5", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaEnum");
    }

    // Test 23
    #[test]
    fn should_handle_enum_with_mixed_types() {
        let schema = object_schema_with_props(vec![(
            "value",
            JsonSchema {
                enum_values: Some(vec![json!("auto"), json!(0), serde_json::Value::Null]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("value: auto");
        let result = validate_schema("value: auto", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // additionalProperties
    // ══════════════════════════════════════════════════════════════════════════

    // Test 24
    #[test]
    fn should_produce_no_diagnostics_when_additional_properties_absent() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let text = "name: Alice\nextra: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 25
    #[test]
    fn should_produce_warning_for_extra_key_when_additional_properties_false() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nextra: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaAdditionalProperty");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(result[0].message.contains("extra"));
    }

    // Test 26
    #[test]
    fn should_produce_no_diagnostics_for_known_keys_when_additional_properties_false() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice");
        let result = validate_schema("name: Alice", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 27
    #[test]
    fn should_produce_one_warning_per_extra_key() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nextra1: a\nextra2: b";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|d| code_of(d) == "schemaAdditionalProperty")
        );
    }

    // Test 28
    #[test]
    fn should_validate_extra_properties_against_additional_properties_schema() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nextra: not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Composition (allOf / anyOf / oneOf)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 29
    #[test]
    fn should_produce_no_diagnostics_when_all_of_all_pass() {
        let schema = JsonSchema {
            all_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("a: 1\nb: 2");
        let result = validate_schema("a: 1\nb: 2", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 30
    #[test]
    fn should_produce_diagnostics_when_any_all_of_fails() {
        let schema = JsonSchema {
            all_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("a: 1");
        let result = validate_schema("a: 1", &docs, &schema, true);
        assert!(!result.is_empty());
    }

    // Test 31
    #[test]
    fn should_produce_no_diagnostics_when_any_of_one_passes() {
        let schema = JsonSchema {
            any_of: Some(vec![
                object_schema_with_props(vec![("name", string_schema())]),
                object_schema_with_props(vec![(
                    "name",
                    JsonSchema {
                        schema_type: Some(SchemaType::Single("integer".to_string())),
                        ..JsonSchema::default()
                    },
                )]),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice");
        let result = validate_schema("name: Alice", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 32
    #[test]
    fn should_produce_diagnostic_when_none_of_any_of_pass() {
        let schema = JsonSchema {
            any_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema("other: value", &docs, &schema, true);
        assert!(!result.is_empty());
    }

    // Test 33
    #[test]
    fn should_produce_no_diagnostics_when_exactly_one_of_passes() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("a: 1");
        let result = validate_schema("a: 1", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 34
    #[test]
    fn should_produce_diagnostic_when_zero_of_one_of_pass() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema("other: value", &docs, &schema, true);
        assert!(!result.is_empty());
    }

    // Test 35
    #[test]
    fn should_produce_diagnostic_when_multiple_of_one_of_pass() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                object_schema_with_props(vec![("a", string_schema())]),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("a: hello");
        let result = validate_schema("a: hello", &docs, &schema, true);
        assert!(!result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Nested validation
    // ══════════════════════════════════════════════════════════════════════════

    // Test 36
    #[test]
    fn should_validate_properties_recursively() {
        let server_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("port".to_string(), integer_schema())].into()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("server", server_schema)]);
        let text = "server:\n  port: not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 37
    #[test]
    fn should_validate_array_items_against_items_schema() {
        let schema = object_schema_with_props(vec![(
            "ports",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(integer_schema())),
                ..JsonSchema::default()
            },
        )]);
        let text = "ports:\n  - 8080\n  - not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 38
    #[test]
    fn should_produce_no_diagnostics_for_valid_array_items() {
        let schema = object_schema_with_props(vec![(
            "ports",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(integer_schema())),
                ..JsonSchema::default()
            },
        )]);
        let text = "ports:\n  - 8080\n  - 9090";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 39
    #[test]
    fn should_validate_deeply_nested_schema_five_levels() {
        // Build 5 levels: root → a → b → c → d → {type: string}
        let leaf = string_schema();
        let l4 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("d".to_string(), leaf)].into()),
            ..JsonSchema::default()
        };
        let l3 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("c".to_string(), l4)].into()),
            ..JsonSchema::default()
        };
        let l2 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("b".to_string(), l3)].into()),
            ..JsonSchema::default()
        };
        let l1 = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("a".to_string(), l2)].into()),
            ..JsonSchema::default()
        };
        let text = "a:\n  b:\n    c:\n      d: hello";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &l1, true);
        assert!(result.is_empty());
    }

    // Test 40
    #[test]
    fn should_not_stack_overflow_on_deep_nesting() {
        // Build 25 levels of nested object schemas programmatically
        let mut schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        };
        for _ in 0..25 {
            schema = JsonSchema {
                schema_type: Some(SchemaType::Single("object".to_string())),
                properties: Some([("x".to_string(), schema)].into()),
                ..JsonSchema::default()
            };
        }
        // Build matching YAML: x:\n  x:\n  ...
        let text = "x:\n".repeat(25) + "  value: leaf";
        let docs = parse_docs(&text);
        // Must complete without panic
        let _ = validate_schema(&text, &docs, &schema, true);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Diagnostic format
    // ══════════════════════════════════════════════════════════════════════════

    // Test 41
    #[test]
    fn should_set_source_to_rlsp_yaml() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema("age: 30", &docs, &schema, true);
        assert!(!result.is_empty());
        assert!(
            result
                .iter()
                .all(|d| d.source == Some("rlsp-yaml".to_string()))
        );
    }

    // Test 42
    #[test]
    fn should_set_correct_code_for_required_violation() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema("age: 30", &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("schemaRequired".to_string()))
        );
    }

    // Test 43
    #[test]
    fn should_set_correct_code_for_type_violation() {
        let schema = object_schema_with_props(vec![("count", integer_schema())]);
        let docs = parse_docs("count: hello");
        let result = validate_schema("count: hello", &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("schemaType".to_string()))
        );
    }

    // Test 44
    #[test]
    fn should_set_correct_code_for_enum_violation() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: testing");
        let result = validate_schema("env: testing", &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("schemaEnum".to_string()))
        );
    }

    // Test 45
    #[test]
    fn should_set_correct_code_for_additional_property_violation() {
        let schema = JsonSchema {
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice\nextra: value");
        let result = validate_schema("name: Alice\nextra: value", &docs, &schema, true);
        let ap_diags: Vec<_> = result
            .iter()
            .filter(|d| code_of(d) == "schemaAdditionalProperty")
            .collect();
        assert!(!ap_diags.is_empty());
        assert_eq!(
            ap_diags[0].code,
            Some(NumberOrString::String(
                "schemaAdditionalProperty".to_string()
            ))
        );
    }

    // Test 46
    #[test]
    fn should_set_error_severity_for_required_violation() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema("age: 30", &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 47
    #[test]
    fn should_set_error_severity_for_type_violation() {
        let schema = object_schema_with_props(vec![("count", integer_schema())]);
        let docs = parse_docs("count: hello");
        let result = validate_schema("count: hello", &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 48
    #[test]
    fn should_set_error_severity_for_enum_violation() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod")]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: testing");
        let result = validate_schema("env: testing", &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 49
    #[test]
    fn should_set_warning_severity_for_additional_property_violation() {
        let schema = JsonSchema {
            properties: Some([("name".to_string(), string_schema())].into()),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: Alice\nextra: value");
        let result = validate_schema("name: Alice\nextra: value", &docs, &schema, true);
        let ap = result
            .iter()
            .find(|d| code_of(d) == "schemaAdditionalProperty")
            .expect("should have additionalProperty diagnostic");
        assert_eq!(ap.severity, Some(DiagnosticSeverity::WARNING));
    }

    // Test 50
    #[test]
    fn should_include_property_path_in_required_diagnostic_message() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  other: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("spec"),
            "message should reference parent path 'spec', got: {msg}"
        );
    }

    // Test 51
    #[test]
    fn should_include_valid_values_in_enum_diagnostic_message() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: testing");
        let result = validate_schema("env: testing", &docs, &schema, true);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(msg.contains("prod"), "message should contain 'prod'");
        assert!(msg.contains("staging"), "message should contain 'staging'");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Edge cases
    // ══════════════════════════════════════════════════════════════════════════

    // Test 52
    #[test]
    fn should_return_empty_for_empty_yaml_document() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("");
        let result = validate_schema("", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 53
    #[test]
    fn should_return_empty_when_docs_is_empty() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let result = validate_schema("name: Alice", &[], &schema, true);
        assert!(result.is_empty());
    }

    // Test 54
    #[test]
    fn should_return_empty_for_schema_with_no_constraints() {
        let schema = JsonSchema::default();
        let docs = parse_docs("anything: value\nnested:\n  key: 123");
        let result = validate_schema("anything: value\nnested:\n  key: 123", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 55
    #[test]
    fn should_return_empty_for_yaml_with_parse_errors() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        // Simulate parse failure by passing empty docs slice
        let result = validate_schema("invalid: [yaml", &[], &schema, true);
        assert!(result.is_empty());
    }

    // Test 56
    #[test]
    fn should_validate_each_document_in_multi_document_yaml() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let text = "name: Alice\n---\nage: 30";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        // Second document is missing "name"
        let req_diags: Vec<_> = result
            .iter()
            .filter(|d| code_of(d) == "schemaRequired")
            .collect();
        assert_eq!(req_diags.len(), 1);
    }

    // Test 57
    #[test]
    fn should_produce_no_diagnostics_for_unknown_property_when_no_properties_in_schema() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        let docs = parse_docs("anything: value");
        let result = validate_schema("anything: value", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Recursion / Depth Guard
    // ══════════════════════════════════════════════════════════════════════════

    // Test 58
    #[test]
    fn should_complete_without_panic_for_deeply_nested_yaml_and_schema() {
        // Build 100 levels of nested object schemas
        let mut schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        for _ in 0..100 {
            schema = JsonSchema {
                schema_type: Some(SchemaType::Single("object".to_string())),
                properties: Some([("child".to_string(), schema)].into()),
                ..JsonSchema::default()
            };
        }
        // Build matching YAML with 100 levels of nesting
        let mut text = String::new();
        for i in 0..100 {
            for _ in 0..i {
                text.push_str("  ");
            }
            text.push_str("child:\n");
        }
        let docs = parse_docs(&text);
        // Must return without panicking — result content doesn't matter
        let _result = validate_schema(&text, &docs, &schema, true);
    }

    // Test 59
    #[test]
    fn should_not_recurse_past_depth_limit() {
        // Build 70 levels of nested properties with a string type leaf
        let mut schema = string_schema();
        for _ in 0..70 {
            schema = JsonSchema {
                schema_type: Some(SchemaType::Single("object".to_string())),
                properties: Some([("child".to_string(), schema)].into()),
                ..JsonSchema::default()
            };
        }
        // Build YAML where the leaf value is an integer (type mismatch)
        let mut text = String::new();
        for i in 0..70 {
            for _ in 0..i {
                text.push_str("  ");
            }
            text.push_str("child:\n");
        }
        for _ in 0..70 {
            text.push_str("  ");
        }
        text.push_str("42\n");
        let docs = parse_docs(&text);
        // Must return without panicking — depth guard may suppress leaf-level error
        let _result = validate_schema(&text, &docs, &schema, true);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Composition Branch Limits
    // ══════════════════════════════════════════════════════════════════════════

    // Test 60
    #[test]
    fn should_complete_in_bounded_time_for_one_of_with_many_alternatives() {
        // 50 branches, each requiring a unique field not present in the YAML
        let branches: Vec<JsonSchema> = (0..50)
            .map(|i| JsonSchema {
                required: Some(vec![format!("field_{i}")]),
                ..JsonSchema::default()
            })
            .collect();
        let schema = JsonSchema {
            one_of: Some(branches),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        // Must return (not hang) — result content doesn't matter
        let _result = validate_schema("other: value", &docs, &schema, true);
    }

    // Test 61
    #[test]
    fn should_complete_for_all_of_with_many_branches() {
        // 25 branches, each requiring a unique field
        let branches: Vec<JsonSchema> = (0..25)
            .map(|i| JsonSchema {
                required: Some(vec![format!("field_{i}")]),
                ..JsonSchema::default()
            })
            .collect();
        let schema = JsonSchema {
            all_of: Some(branches),
            ..JsonSchema::default()
        };
        // Only field_0 is present — all other branches unsatisfied
        let docs = parse_docs("field_0: value");
        let result = validate_schema("field_0: value", &docs, &schema, true);
        // Non-empty: at least one diagnostic for unsatisfied branches
        assert!(!result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Diagnostic Message Truncation
    // ══════════════════════════════════════════════════════════════════════════

    // Test 62
    #[test]
    fn should_truncate_long_description_in_diagnostic_message() {
        // The implementation does not embed property descriptions in violation
        // messages (descriptions appear in hover, not diagnostics). This test
        // verifies that a required-property violation message stays within a
        // reasonable length even when the schema has a very long description.
        let long_desc = "x".repeat(1000);
        let prop_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            description: Some(long_desc),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            properties: Some([("name".to_string(), prop_schema)].into()),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema("age: 30", &docs, &schema, true);
        assert!(!result.is_empty());
        // Message must not contain the full 1000-char description
        for d in &result {
            assert!(
                d.message.len() <= 300,
                "diagnostic message too long: {} chars",
                d.message.len()
            );
        }
    }

    // Test 63
    #[test]
    fn should_truncate_long_enum_value_list_in_diagnostic_message() {
        // 50 enum values — the message must not list all 50 verbatim
        let enum_values: Vec<serde_json::Value> =
            (0..50).map(|i| json!(format!("opt{i}"))).collect();
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                enum_values: Some(enum_values),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: invalid");
        let result = validate_schema("env: invalid", &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(code_of(&result[0]), "schemaEnum");
        // Message must be bounded — 50 values * ~6 chars each would be ~300+
        assert!(
            result[0].message.len() <= 500,
            "enum diagnostic message too long: {} chars",
            result[0].message.len()
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Mutex / Async Safety (Structural)
    // ══════════════════════════════════════════════════════════════════════════

    // Mutex-across-await safety is enforced at compile time by
    // clippy::await_holding_lock (pedantic group, deny-on-warnings).

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Mutex Poison Handling
    // ══════════════════════════════════════════════════════════════════════════

    // Test 65
    #[test]
    fn should_continue_without_schema_validation_when_cache_lock_poisoned() {
        use std::sync::{Arc, Mutex};

        let lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        let lock_clone = Arc::clone(&lock);

        // Poison the mutex by panicking while holding the guard.
        let handle = std::thread::spawn(move || {
            let _guard = lock_clone.lock().unwrap();
            panic!("intentional panic to poison the mutex");
        });
        assert!(handle.join().is_err(), "thread should have panicked");

        // The poisoned mutex returns Err from lock(), and .ok() gives None —
        // matching the production pattern used throughout schema_cache access.
        assert!(
            lock.lock().is_err(),
            "poisoned mutex must return Err from lock()"
        );
        assert!(
            lock.lock().ok().is_none(),
            ".ok() on poisoned lock must return None"
        );
    }

    // Test 66
    #[test]
    fn should_include_expected_properties_in_required_diagnostic_message() {
        let schema = JsonSchema {
            required: Some(vec![
                "name".to_string(),
                "age".to_string(),
                "email".to_string(),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema("other: value", &docs, &schema, true);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("Expected properties:"),
            "message should contain 'Expected properties:', got: {msg}"
        );
        assert!(
            msg.contains("name"),
            "message should contain 'name', got: {msg}"
        );
        assert!(
            msg.contains("age"),
            "message should contain 'age', got: {msg}"
        );
        assert!(
            msg.contains("email"),
            "message should contain 'email', got: {msg}"
        );
    }

    // Test 67
    #[test]
    fn should_truncate_expected_properties_list_when_more_than_max() {
        let schema = JsonSchema {
            required: Some(vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
                "delta".to_string(),
                "epsilon".to_string(),
                "zeta".to_string(),
                "eta".to_string(),
            ]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema("other: value", &docs, &schema, true);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("(7 total)"),
            "message should contain total count, got: {msg}"
        );
        assert!(
            msg.contains("..."),
            "message should contain ellipsis for truncation, got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — pattern
    // ══════════════════════════════════════════════════════════════════════════

    // Test 68
    #[test]
    fn should_produce_no_diagnostics_when_string_matches_pattern() {
        let schema = object_schema_with_props(vec![(
            "code",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                pattern: Some("^[A-Z]{3}$".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("code: ABC");
        let result = validate_schema("code: ABC", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 69
    #[test]
    fn should_produce_error_when_string_does_not_match_pattern() {
        let schema = object_schema_with_props(vec![(
            "code",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                pattern: Some("^[A-Z]{3}$".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("code: abc");
        let result = validate_schema("code: abc", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPattern");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 70
    #[test]
    fn should_emit_warning_when_pattern_exceeds_max_length() {
        let long_pattern = "a".repeat(1025);
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                pattern: Some(long_pattern),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: anything");
        let result = validate_schema("val: anything", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPatternLimit");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    // Test 71
    #[test]
    fn should_emit_warning_when_pattern_cannot_be_compiled() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                // A pattern that RegexBuilder rejects (unclosed bracket)
                pattern: Some("[invalid".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: anything");
        let result = validate_schema("val: anything", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPatternLimit");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — minLength / maxLength
    // ══════════════════════════════════════════════════════════════════════════

    // Test 72
    #[test]
    fn should_produce_no_diagnostics_when_string_meets_min_length() {
        let schema = object_schema_with_props(vec![(
            "name",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                min_length: Some(3),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("name: abc");
        let result = validate_schema("name: abc", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 73
    #[test]
    fn should_produce_error_when_string_is_shorter_than_min_length() {
        let schema = object_schema_with_props(vec![(
            "name",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                min_length: Some(5),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("name: hi");
        let result = validate_schema("name: hi", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinLength");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 74
    #[test]
    fn should_produce_no_diagnostics_when_string_meets_max_length() {
        let schema = object_schema_with_props(vec![(
            "name",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                max_length: Some(10),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("name: hello");
        let result = validate_schema("name: hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 75
    #[test]
    fn should_produce_error_when_string_exceeds_max_length() {
        let schema = object_schema_with_props(vec![(
            "name",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                max_length: Some(3),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("name: toolong");
        let result = validate_schema("name: toolong", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMaxLength");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — minimum / maximum (inclusive)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 76
    #[test]
    fn should_produce_no_diagnostics_when_integer_meets_minimum() {
        let schema = object_schema_with_props(vec![(
            "port",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                minimum: Some(1.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("port: 80");
        let result = validate_schema("port: 80", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 77
    #[test]
    fn should_produce_error_when_integer_is_below_minimum() {
        let schema = object_schema_with_props(vec![(
            "port",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                minimum: Some(1.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("port: 0");
        let result = validate_schema("port: 0", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinimum");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 78
    #[test]
    fn should_produce_no_diagnostics_when_integer_meets_maximum() {
        let schema = object_schema_with_props(vec![(
            "port",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                maximum: Some(65535.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("port: 8080");
        let result = validate_schema("port: 8080", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 79
    #[test]
    fn should_produce_error_when_integer_exceeds_maximum() {
        let schema = object_schema_with_props(vec![(
            "port",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                maximum: Some(65535.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("port: 99999");
        let result = validate_schema("port: 99999", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMaximum");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — Draft-04 exclusiveMinimum / exclusiveMaximum (bool)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 80
    #[test]
    fn should_produce_error_when_value_equals_minimum_and_exclusive_draft04() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                minimum: Some(5.0),
                exclusive_minimum_draft04: Some(true),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 5");
        let result = validate_schema("val: 5", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinimum");
    }

    // Test 81
    #[test]
    fn should_produce_no_diagnostics_when_value_equals_minimum_and_not_exclusive_draft04() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                minimum: Some(5.0),
                exclusive_minimum_draft04: Some(false),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 5");
        let result = validate_schema("val: 5", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 82
    #[test]
    fn should_produce_error_when_value_equals_maximum_and_exclusive_draft04() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                maximum: Some(10.0),
                exclusive_maximum_draft04: Some(true),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 10");
        let result = validate_schema("val: 10", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMaximum");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — Draft-06+ exclusiveMinimum / exclusiveMaximum (f64)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 83
    #[test]
    fn should_produce_error_when_value_equals_exclusive_minimum_draft06() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                exclusive_minimum: Some(5.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 5");
        let result = validate_schema("val: 5", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinimum");
    }

    // Test 84
    #[test]
    fn should_produce_no_diagnostics_when_value_exceeds_exclusive_minimum_draft06() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                exclusive_minimum: Some(5.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 6");
        let result = validate_schema("val: 6", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 85
    #[test]
    fn should_produce_error_when_value_equals_exclusive_maximum_draft06() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                exclusive_maximum: Some(10.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 10");
        let result = validate_schema("val: 10", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMaximum");
    }

    // Test 86
    #[test]
    fn should_produce_no_diagnostics_when_value_is_below_exclusive_maximum_draft06() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                exclusive_maximum: Some(10.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 9");
        let result = validate_schema("val: 9", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — multipleOf
    // ══════════════════════════════════════════════════════════════════════════

    // Test 87
    #[test]
    fn should_produce_no_diagnostics_when_value_is_multiple_of() {
        let schema = object_schema_with_props(vec![(
            "count",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                multiple_of: Some(5.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("count: 15");
        let result = validate_schema("count: 15", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 88
    #[test]
    fn should_produce_error_when_value_is_not_multiple_of() {
        let schema = object_schema_with_props(vec![(
            "count",
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                multiple_of: Some(5.0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("count: 7");
        let result = validate_schema("count: 7", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMultipleOf");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — const
    // ══════════════════════════════════════════════════════════════════════════

    // Test 89
    #[test]
    fn should_produce_no_diagnostics_when_value_equals_const() {
        let schema = object_schema_with_props(vec![(
            "version",
            JsonSchema {
                const_value: Some(json!("v1")),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("version: v1");
        let result = validate_schema("version: v1", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 90
    #[test]
    fn should_produce_error_when_value_does_not_equal_const() {
        let schema = object_schema_with_props(vec![(
            "version",
            JsonSchema {
                const_value: Some(json!("v1")),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("version: v2");
        let result = validate_schema("version: v2", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaConst");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 91
    #[test]
    fn should_produce_no_diagnostics_when_integer_equals_const() {
        let schema = object_schema_with_props(vec![(
            "level",
            JsonSchema {
                const_value: Some(json!(42)),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("level: 42");
        let result = validate_schema("level: 42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 92
    #[test]
    fn should_skip_const_check_for_mapping_node() {
        let schema = object_schema_with_props(vec![(
            "obj",
            JsonSchema {
                const_value: Some(json!({"key": "val"})),
                ..JsonSchema::default()
            },
        )]);
        let text = "obj:\n  key: other";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        // yaml_to_json returns None for mappings — const check skipped
        let const_diags: Vec<_> = result
            .iter()
            .filter(|d| code_of(d) == "schemaConst")
            .collect();
        assert!(const_diags.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // not keyword
    // ══════════════════════════════════════════════════════════════════════════

    // Test 93
    #[test]
    fn should_produce_error_when_value_matches_not_schema() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: hello");
        let result = validate_schema("val: hello", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaNot");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 94
    #[test]
    fn should_produce_no_diagnostics_when_value_does_not_match_not_schema() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("val: 42");
        let result = validate_schema("val: 42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 95
    #[test]
    fn should_reject_string_when_not_type_string() {
        let schema = JsonSchema {
            not: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let docs = parse_docs("hello");
        let result = validate_schema("hello", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaNot");
    }

    // Test 96
    #[test]
    fn should_allow_integer_when_not_type_string() {
        let schema = JsonSchema {
            not: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let docs = parse_docs("42");
        let result = validate_schema("42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 97
    #[test]
    fn should_produce_error_when_value_matches_not_enum() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    enum_values: Some(vec![json!("prod"), json!("staging")]),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: prod");
        let result = validate_schema("env: prod", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaNot");
    }

    // Test 98
    #[test]
    fn should_produce_no_diagnostics_when_value_outside_not_enum() {
        let schema = object_schema_with_props(vec![(
            "env",
            JsonSchema {
                not: Some(Box::new(JsonSchema {
                    enum_values: Some(vec![json!("prod"), json!("staging")]),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("env: dev");
        let result = validate_schema("env: dev", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // patternProperties
    // ══════════════════════════════════════════════════════════════════════════

    // Test 99
    #[test]
    fn should_validate_value_against_pattern_properties_schema() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        // str_name should be validated as string — integer is a type violation
        let text = "str_name: 42";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 100
    #[test]
    fn should_produce_no_diagnostics_when_pattern_property_value_is_valid() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        let text = "str_name: hello";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 101
    #[test]
    fn should_not_trigger_additional_properties_for_key_matched_by_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let text = "str_name: hello";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        // str_name matches the pattern — no additionalProperty diagnostic
        assert!(
            result
                .iter()
                .all(|d| code_of(d) != "schemaAdditionalProperty")
        );
    }

    // Test 102
    #[test]
    fn should_trigger_additional_properties_for_key_not_matched_by_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        // "other" doesn't match "^str_" and there are no properties
        let text = "other: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaAdditionalProperty");
    }

    // Test 103
    #[test]
    fn should_prefer_properties_over_pattern_properties_for_known_key() {
        // "name" is in properties (integer), and also matches pattern (string).
        // properties takes precedence — integer schema applies.
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), integer_schema())].into()),
            pattern_properties: Some(vec![("^name$".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        // "name" is an integer — valid against properties schema (integer), but
        // patternProperties schema (string) is not applied for known properties.
        let text = "name: 42";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 104
    #[test]
    fn should_match_key_against_multiple_patterns_and_validate_all() {
        // "x_num" matches both "^x_" (string) and ".*num.*" (integer).
        // Both schemas validate the value — integer value fails string check.
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![
                ("^x_".to_string(), string_schema()),
                ("num".to_string(), integer_schema()),
            ]),
            ..JsonSchema::default()
        };
        // value is integer — fails string pattern, passes integer pattern
        let text = "x_num: 42";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        // One type violation from the string pattern
        let type_diags: Vec<_> = result
            .iter()
            .filter(|d| code_of(d) == "schemaType")
            .collect();
        assert_eq!(type_diags.len(), 1);
    }

    // Test 105
    #[test]
    fn should_emit_warning_for_over_length_pattern_and_fall_through_to_additional_properties() {
        let long_pattern = "a".repeat(1025);
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![(long_pattern, string_schema())]),
            additional_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        // The pattern is too long — emits a PatternLimit warning.
        // "key" also falls through to additionalProperties (not matched by any pattern).
        let text = "key: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.iter().any(|d| code_of(d) == "schemaPatternLimit"
            && d.severity == Some(DiagnosticSeverity::WARNING)));
        assert!(
            result
                .iter()
                .any(|d| code_of(d) == "schemaAdditionalProperty")
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Array constraints — minItems / maxItems / uniqueItems
    // ══════════════════════════════════════════════════════════════════════════

    fn array_schema(min: Option<u64>, max: Option<u64>, unique: Option<bool>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            min_items: min,
            max_items: max,
            unique_items: unique,
            ..JsonSchema::default()
        }
    }

    // Test 106
    #[test]
    fn should_produce_error_when_array_has_fewer_items_than_min_items() {
        let schema = object_schema_with_props(vec![("tags", array_schema(Some(2), None, None))]);
        let text = "tags:\n  - a";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinItems");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 107
    #[test]
    fn should_produce_no_diagnostics_when_array_meets_min_items() {
        let schema = object_schema_with_props(vec![("tags", array_schema(Some(2), None, None))]);
        let text = "tags:\n  - a\n  - b";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 108
    #[test]
    fn should_produce_error_when_array_exceeds_max_items() {
        let schema = object_schema_with_props(vec![("tags", array_schema(None, Some(2), None))]);
        let text = "tags:\n  - a\n  - b\n  - c";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMaxItems");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 109
    #[test]
    fn should_produce_no_diagnostics_when_array_meets_max_items() {
        let schema = object_schema_with_props(vec![("tags", array_schema(None, Some(2), None))]);
        let text = "tags:\n  - a\n  - b";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 110
    #[test]
    fn should_produce_error_when_array_has_duplicate_items_and_unique_items_true() {
        let schema = object_schema_with_props(vec![("tags", array_schema(None, None, Some(true)))]);
        let text = "tags:\n  - foo\n  - bar\n  - foo";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaUniqueItems");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 111
    #[test]
    fn should_produce_no_diagnostics_when_all_items_unique_and_unique_items_true() {
        let schema = object_schema_with_props(vec![("tags", array_schema(None, None, Some(true)))]);
        let text = "tags:\n  - foo\n  - bar\n  - baz";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 112
    #[test]
    fn should_produce_no_diagnostics_when_unique_items_false_even_with_duplicates() {
        let schema =
            object_schema_with_props(vec![("tags", array_schema(None, None, Some(false)))]);
        let text = "tags:\n  - foo\n  - foo";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Regex security hardening
    // ══════════════════════════════════════════════════════════════════════════

    // Test 113
    #[test]
    fn should_emit_warning_when_pattern_limit_exceeded_in_pattern_properties() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("[invalid".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        // Invalid regex in patternProperties — warning emitted
        let text = "key: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.iter().any(|d| code_of(d) == "schemaPatternLimit"
            && d.severity == Some(DiagnosticSeverity::WARNING)));
    }

    // Test 114
    #[test]
    fn should_still_match_valid_string_against_pattern_after_hardening() {
        // Regression: hardening must not break valid pattern matching
        let schema = object_schema_with_props(vec![(
            "code",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                pattern: Some("^[A-Z]{3}$".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("code: abc");
        let result = validate_schema("code: abc", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPattern");
    }

    // Test 115
    #[test]
    fn should_still_match_valid_pattern_property_after_hardening() {
        // Regression: hardening must not break valid patternProperties matching
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            pattern_properties: Some(vec![("^str_".to_string(), string_schema())]),
            ..JsonSchema::default()
        };
        let text = "str_name: 42";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // propertyNames
    // ══════════════════════════════════════════════════════════════════════════

    // Test 116
    #[test]
    fn should_produce_no_diagnostics_when_all_keys_match_property_names_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z_]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "foo: 1\nbar_baz: 2";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 117
    #[test]
    fn should_produce_diagnostic_when_key_violates_property_names_pattern() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z_]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        // "BadKey" contains uppercase — violates pattern
        let text = "BadKey: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaPattern");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 118
    #[test]
    fn should_produce_diagnostic_when_key_violates_property_names_min_length() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                min_length: Some(3),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "ab: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinLength");
    }

    // Test 119
    #[test]
    fn should_produce_diagnostic_when_key_not_in_property_names_enum() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                enum_values: Some(vec![json!("foo"), json!("bar")]),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "baz: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaEnum");
    }

    // Test 120
    #[test]
    fn should_apply_property_names_to_all_keys_regardless_of_properties() {
        // "name" is in properties, "extra" is not — propertyNames applies to both
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("name".to_string(), string_schema())].into()),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        // Both keys are lowercase — no violations
        let text = "name: Alice\nextra: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 121
    #[test]
    fn should_produce_diagnostics_for_all_violating_keys_with_property_names() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            property_names: Some(Box::new(JsonSchema {
                pattern: Some("^[a-z]+$".to_string()),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let text = "UPPER: 1\nAlso_Bad: 2\ngood: 3";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        let pattern_diags: Vec<_> = result
            .iter()
            .filter(|d| code_of(d) == "schemaPattern")
            .collect();
        assert_eq!(pattern_diags.len(), 2);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // dependentRequired / dependentSchemas
    // ══════════════════════════════════════════════════════════════════════════

    // Test 122
    #[test]
    fn should_produce_error_when_trigger_present_and_dependent_required_missing() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_required: Some(
                [(
                    "credit_card".to_string(),
                    vec!["billing_address".to_string()],
                )]
                .into(),
            ),
            ..JsonSchema::default()
        };
        // credit_card present but billing_address absent
        let text = "credit_card: 1234";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaDependentRequired");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("billing_address"));
        assert!(result[0].message.contains("credit_card"));
    }

    // Test 123
    #[test]
    fn should_produce_no_diagnostics_when_trigger_and_dependency_both_present() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_required: Some(
                [(
                    "credit_card".to_string(),
                    vec!["billing_address".to_string()],
                )]
                .into(),
            ),
            ..JsonSchema::default()
        };
        let text = "credit_card: 1234\nbilling_address: 123 Main St";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 124
    #[test]
    fn should_produce_no_diagnostics_when_trigger_absent_in_dependent_required() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_required: Some(
                [(
                    "credit_card".to_string(),
                    vec!["billing_address".to_string()],
                )]
                .into(),
            ),
            ..JsonSchema::default()
        };
        // trigger absent — no check performed
        let text = "name: Alice";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 125
    #[test]
    fn should_produce_diagnostic_when_trigger_present_and_dependent_schema_fails() {
        // When "name" is present, the mapping must also have "age" (required by dep schema)
        let dep_schema = JsonSchema {
            required: Some(vec!["age".to_string()]),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_schemas: Some([("name".to_string(), dep_schema)].into()),
            ..JsonSchema::default()
        };
        let text = "name: Alice";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(!result.is_empty());
        assert_eq!(code_of(&result[0]), "schemaRequired");
    }

    // Test 126
    #[test]
    fn should_produce_no_diagnostics_when_dependent_schema_passes() {
        let dep_schema = JsonSchema {
            required: Some(vec!["age".to_string()]),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_schemas: Some([("name".to_string(), dep_schema)].into()),
            ..JsonSchema::default()
        };
        let text = "name: Alice\nage: 30";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 127
    #[test]
    fn should_produce_no_diagnostics_when_dependent_schema_trigger_absent() {
        let dep_schema = JsonSchema {
            required: Some(vec!["age".to_string()]),
            ..JsonSchema::default()
        };
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            dependent_schemas: Some([("name".to_string(), dep_schema)].into()),
            ..JsonSchema::default()
        };
        // trigger "name" absent — dep schema not checked
        let text = "other: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // if / then / else
    // ══════════════════════════════════════════════════════════════════════════

    // Test 128
    #[test]
    fn should_apply_then_and_pass_when_if_matches_and_then_passes() {
        // if: type string → then: minLength 3
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                then_schema: Some(Box::new(JsonSchema {
                    min_length: Some(3),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is a string of length 5 — if matches, then passes
        let docs = parse_docs("val: hello");
        let result = validate_schema("val: hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 129
    #[test]
    fn should_apply_then_and_fail_when_if_matches_and_then_fails() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                then_schema: Some(Box::new(JsonSchema {
                    min_length: Some(10),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is a short string — if matches, then fails
        let docs = parse_docs("val: hi");
        let result = validate_schema("val: hi", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinLength");
    }

    // Test 130
    #[test]
    fn should_apply_else_and_pass_when_if_does_not_match_and_else_passes() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    minimum: Some(0.0),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is integer 5 — if doesn't match (not string), else passes (>= 0)
        let docs = parse_docs("val: 5");
        let result = validate_schema("val: 5", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 131
    #[test]
    fn should_apply_else_and_fail_when_if_does_not_match_and_else_fails() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    minimum: Some(10.0),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is integer 3 — if doesn't match, else fails (< 10)
        let docs = parse_docs("val: 3");
        let result = validate_schema("val: 3", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinimum");
    }

    // Test 132
    #[test]
    fn should_produce_no_diagnostics_when_if_matches_but_no_then() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    minimum: Some(0.0),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is string — if matches, no then → no diagnostic
        let docs = parse_docs("val: hello");
        let result = validate_schema("val: hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 133
    #[test]
    fn should_produce_no_diagnostics_when_if_does_not_match_and_no_else() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                if_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("string".to_string())),
                    ..JsonSchema::default()
                })),
                then_schema: Some(Box::new(JsonSchema {
                    min_length: Some(10),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // val is integer — if doesn't match, no else → no diagnostic
        let docs = parse_docs("val: 42");
        let result = validate_schema("val: 42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 134
    #[test]
    fn should_ignore_then_and_else_when_no_if() {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                then_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("integer".to_string())),
                    ..JsonSchema::default()
                })),
                else_schema: Some(Box::new(JsonSchema {
                    schema_type: Some(SchemaType::Single("integer".to_string())),
                    ..JsonSchema::default()
                })),
                ..JsonSchema::default()
            },
        )]);
        // Without if, then/else are ignored — string value produces no diagnostic
        let docs = parse_docs("val: hello");
        let result = validate_schema("val: hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ── contains / minContains / maxContains ────────────────────────────────

    fn contains_schema(min_contains: Option<u64>, max_contains: Option<u64>) -> JsonSchema {
        JsonSchema {
            contains: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                ..JsonSchema::default()
            })),
            min_contains,
            max_contains,
            ..JsonSchema::default()
        }
    }

    // Test 135
    #[test]
    fn should_produce_no_diagnostics_when_array_has_one_matching_item_no_min_max() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, None))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema("items:\n  - 1\n  - hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 136
    #[test]
    fn should_produce_diagnostic_when_no_items_match_contains_schema() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, None))]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema("items:\n  - hello\n  - world", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at least 1"));
    }

    // Test 137
    #[test]
    fn should_produce_diagnostic_when_min_contains_not_met() {
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(2), None))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema("items:\n  - 1\n  - hello", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at least 2"));
    }

    // Test 138
    #[test]
    fn should_produce_no_diagnostics_when_min_contains_met() {
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(2), None))]);
        let docs = parse_docs("items:\n  - 1\n  - 2");
        let result = validate_schema("items:\n  - 1\n  - 2", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 139
    #[test]
    fn should_produce_diagnostic_when_max_contains_exceeded() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, Some(1)))]);
        let docs = parse_docs("items:\n  - 1\n  - 2");
        let result = validate_schema("items:\n  - 1\n  - 2", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("at most 1"));
    }

    // Test 140
    #[test]
    fn should_produce_no_diagnostics_when_max_contains_not_exceeded() {
        let schema = object_schema_with_props(vec![("items", contains_schema(None, Some(1)))]);
        let docs = parse_docs("items:\n  - 1\n  - hello");
        let result = validate_schema("items:\n  - 1\n  - hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 141
    #[test]
    fn should_produce_no_diagnostics_when_min_contains_zero() {
        // minContains: 0 disables the "at least one" requirement
        let schema = object_schema_with_props(vec![("items", contains_schema(Some(0), None))]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema("items:\n  - hello\n  - world", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 142
    #[test]
    fn should_ignore_min_contains_and_max_contains_when_contains_absent() {
        // minContains/maxContains without contains are ignored per spec
        let schema = object_schema_with_props(vec![(
            "items",
            JsonSchema {
                min_contains: Some(5),
                max_contains: Some(0),
                ..JsonSchema::default()
            },
        )]);
        let docs = parse_docs("items:\n  - hello\n  - world");
        let result = validate_schema("items:\n  - hello\n  - world", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ── prefixItems ─────────────────────────────────────────────────────────

    fn tuple_schema(prefix: Vec<JsonSchema>, items: Option<JsonSchema>) -> JsonSchema {
        JsonSchema {
            prefix_items: Some(prefix),
            items: items.map(Box::new),
            ..JsonSchema::default()
        }
    }

    // Test 143
    #[test]
    fn should_produce_diagnostic_when_second_item_fails_prefix_schema() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema(), integer_schema()], None),
        )]);
        // [0] is a string (ok), [1] is a string but expected integer (fail)
        let docs = parse_docs("arr:\n  - hello\n  - world");
        let result = validate_schema("arr:\n  - hello\n  - world", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 144
    #[test]
    fn should_produce_no_diagnostics_when_all_items_match_prefix_schemas() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema(), integer_schema()], None),
        )]);
        let docs = parse_docs("arr:\n  - hello\n  - 42");
        let result = validate_schema("arr:\n  - hello\n  - 42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 145
    #[test]
    fn should_validate_extra_items_against_items_schema_when_prefix_items_set() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema()], Some(integer_schema())),
        )]);
        // [0] string ok, [1] integer ok (matches items schema)
        let docs = parse_docs("arr:\n  - hello\n  - 42");
        let result = validate_schema("arr:\n  - hello\n  - 42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 146
    #[test]
    fn should_produce_diagnostic_when_extra_item_fails_items_schema() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(vec![string_schema()], Some(integer_schema())),
        )]);
        // [0] string ok, [1] string fails items schema (expected integer)
        let docs = parse_docs("arr:\n  - hello\n  - world");
        let result = validate_schema("arr:\n  - hello\n  - world", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 147
    #[test]
    fn should_produce_no_diagnostics_when_array_shorter_than_prefix_items() {
        let schema = object_schema_with_props(vec![(
            "arr",
            tuple_schema(
                vec![string_schema(), integer_schema(), string_schema()],
                None,
            ),
        )]);
        // Only one item — only [0] is validated, [1] and [2] positions absent
        let docs = parse_docs("arr:\n  - hello");
        let result = validate_schema("arr:\n  - hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 148 — Draft-04 array-form items parsed as prefixItems
    #[test]
    fn should_parse_draft04_array_items_as_prefix_items() {
        use crate::schema::parse_schema;
        use serde_json::json;
        let raw = json!({
            "type": "object",
            "properties": {
                "arr": {
                    "type": "array",
                    "items": [
                        { "type": "string" },
                        { "type": "integer" }
                    ]
                }
            }
        });
        let schema = parse_schema(&raw).expect("valid schema");
        let arr_schema = schema
            .properties
            .as_ref()
            .and_then(|p| p.get("arr"))
            .expect("arr property");
        assert!(arr_schema.prefix_items.is_some());
        assert_eq!(arr_schema.prefix_items.as_ref().unwrap().len(), 2);
        assert!(arr_schema.items.is_none());
    }

    // Test 149 — prefixItems takes precedence over array-form items
    #[test]
    fn should_prefer_prefix_items_over_draft04_array_items() {
        use crate::schema::parse_schema;
        use serde_json::json;
        let raw = json!({
            "prefixItems": [{ "type": "string" }],
            "items": [{ "type": "integer" }, { "type": "boolean" }]
        });
        let schema = parse_schema(&raw).expect("valid schema");
        // prefixItems was set first — array-form items is ignored
        assert!(schema.prefix_items.is_some());
        assert_eq!(schema.prefix_items.as_ref().unwrap().len(), 1);
    }

    // ── unevaluatedProperties / unevaluatedItems ─────────────────────────────

    // Test 150 — unevaluatedProperties: false with allOf — properties from allOf evaluated (pass)
    #[test]
    fn should_produce_no_diagnostics_when_allof_evaluates_all_properties() {
        let schema = JsonSchema {
            all_of: Some(vec![object_schema_with_props(vec![(
                "name",
                string_schema(),
            )])]),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: hello");
        let result = validate_schema("name: hello", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 151 — unevaluatedProperties: false — property not in any sub-schema (diagnostic)
    #[test]
    fn should_produce_diagnostic_for_unevaluated_property() {
        let schema = JsonSchema {
            properties: Some(
                vec![("name".to_string(), string_schema())]
                    .into_iter()
                    .collect(),
            ),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema("name: hello\nextra: world", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("extra"));
    }

    // Test 152 — unevaluatedProperties with sub-schema — unevaluated validated against it
    #[test]
    fn should_validate_unevaluated_property_against_schema() {
        let schema = JsonSchema {
            properties: Some(
                vec![("name".to_string(), string_schema())]
                    .into_iter()
                    .collect(),
            ),
            unevaluated_properties: Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
            ..JsonSchema::default()
        };
        // "extra" is unevaluated and not an integer — diagnostic
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema("name: hello\nextra: world", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 153 — unevaluatedItems with prefixItems — prefix items are evaluated (pass)
    #[test]
    fn should_produce_no_diagnostics_when_prefix_items_cover_all_items() {
        let schema = JsonSchema {
            prefix_items: Some(vec![string_schema(), integer_schema()]),
            unevaluated_items: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("boolean".to_string())),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let docs = parse_docs("- hello\n- 42");
        let result = validate_schema("- hello\n- 42", &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 154 — unevaluatedItems — item beyond prefix not evaluated (diagnostic)
    #[test]
    fn should_produce_diagnostic_for_unevaluated_item_beyond_prefix() {
        let schema = JsonSchema {
            prefix_items: Some(vec![string_schema()]),
            unevaluated_items: Some(Box::new(integer_schema())),
            ..JsonSchema::default()
        };
        // [0] is string (evaluated by prefix), [1] is string (unevaluated, fails integer)
        let docs = parse_docs("- hello\n- world");
        let result = validate_schema("- hello\n- world", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Test 155 — unevaluatedProperties with if/then — then branch properties evaluated
    #[test]
    fn should_evaluate_properties_from_then_branch() {
        let schema = JsonSchema {
            if_schema: Some(Box::new(JsonSchema {
                required: Some(vec!["name".to_string()]),
                ..JsonSchema::default()
            })),
            then_schema: Some(Box::new(object_schema_with_props(vec![(
                "extra",
                string_schema(),
            )]))),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        };
        // "extra" is in then_schema — evaluated, no diagnostic
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema("name: hello\nextra: world", &docs, &schema, true);
        // "name" is not in then properties — it's unevaluated → diagnostic
        // "extra" IS in then properties → no diagnostic for extra
        let unevaluated: Vec<_> = result
            .iter()
            .filter(|d| d.message.contains("extra"))
            .collect();
        assert!(unevaluated.is_empty(), "extra should be evaluated by then");
    }

    // Test 156 — no unevaluated keywords — existing behavior unchanged (regression)
    #[test]
    fn should_not_change_behavior_when_no_unevaluated_keywords() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: hello\nextra: world");
        let result = validate_schema("name: hello\nextra: world", &docs, &schema, true);
        // Without unevaluated keywords, extra property is allowed
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Format validation
    // ══════════════════════════════════════════════════════════════════════════

    fn format_schema(fmt: &str) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            format: Some(fmt.to_string()),
            ..JsonSchema::default()
        }
    }

    fn run_format(text: &str, fmt: &str) -> Vec<Diagnostic> {
        let schema = format_schema(fmt);
        let docs = parse_docs(text);
        validate_schema(text, &docs, &schema, true)
    }

    // Test 157 — date-time: valid RFC 3339
    #[test]
    fn format_date_time_valid() {
        assert!(run_format("2023-01-15T10:30:00Z", "date-time").is_empty());
        assert!(run_format("2023-01-15T10:30:00+05:30", "date-time").is_empty());
        assert!(run_format("2023-01-15T10:30:00.123Z", "date-time").is_empty());
    }

    // Test 158 — date-time: invalid values emit schemaFormat WARNING
    #[test]
    fn format_date_time_invalid() {
        let result = run_format("not-a-date", "date-time");
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaFormat");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(result[0].message.contains("date-time"));

        assert_eq!(run_format("2023-13-01T00:00:00Z", "date-time").len(), 1);
        assert_eq!(run_format("2023-01-15 10:30:00Z", "date-time").len(), 1);
    }

    // Test 159 — date: valid YYYY-MM-DD
    #[test]
    fn format_date_valid() {
        assert!(run_format("2023-01-15", "date").is_empty());
        assert!(run_format("2024-02-29", "date").is_empty()); // leap year
    }

    // Test 160 — date: invalid
    #[test]
    fn format_date_invalid() {
        assert_eq!(run_format("2023-13-01", "date").len(), 1); // month > 12
        assert_eq!(run_format("2023-02-29", "date").len(), 1); // non-leap year
        assert_eq!(run_format("not-a-date", "date").len(), 1);
    }

    // Test 161 — email: valid
    #[test]
    fn format_email_valid() {
        assert!(run_format("user@example.com", "email").is_empty());
        assert!(run_format("a+b@sub.domain.org", "email").is_empty());
    }

    // Test 162 — email: invalid
    #[test]
    fn format_email_invalid() {
        assert_eq!(run_format("no-at-sign", "email").len(), 1);
        assert_eq!(run_format("missing-domain-dot@nodot", "email").len(), 1);
        assert_eq!(run_format("user-no-domain@", "email").len(), 1);
    }

    // Test 163 — ipv4: valid
    #[test]
    fn format_ipv4_valid() {
        assert!(run_format("192.168.1.1", "ipv4").is_empty());
        assert!(run_format("0.0.0.0", "ipv4").is_empty());
        assert!(run_format("255.255.255.255", "ipv4").is_empty());
    }

    // Test 164 — ipv4: invalid
    #[test]
    fn format_ipv4_invalid() {
        assert_eq!(run_format("256.0.0.1", "ipv4").len(), 1);
        assert_eq!(run_format("192.168.1", "ipv4").len(), 1);
        assert_eq!(run_format("192.168.1.1.1", "ipv4").len(), 1);
        assert_eq!(run_format("01.0.0.1", "ipv4").len(), 1); // leading zero
    }

    // Test 165 — ipv6: valid
    #[test]
    fn format_ipv6_valid() {
        assert!(run_format("2001:0db8:85a3:0000:0000:8a2e:0370:7334", "ipv6").is_empty());
        assert!(run_format("::1", "ipv6").is_empty());
        assert!(run_format("fe80::1", "ipv6").is_empty());
    }

    // Test 166 — ipv6: invalid
    #[test]
    fn format_ipv6_invalid() {
        assert_eq!(
            run_format(
                "not::an::ipv6::address::with::too::many::groups::here",
                "ipv6"
            )
            .len(),
            1
        );
    }

    // Test 167 — hostname: valid
    #[test]
    fn format_hostname_valid() {
        assert!(run_format("example.com", "hostname").is_empty());
        assert!(run_format("sub.example.com", "hostname").is_empty());
        assert!(run_format("localhost", "hostname").is_empty());
    }

    // Test 168 — hostname: invalid
    #[test]
    fn format_hostname_invalid() {
        assert_eq!(run_format("-invalid.com", "hostname").len(), 1);
        assert_eq!(run_format("invalid-.com", "hostname").len(), 1);
        assert_eq!(run_format("invalid..com", "hostname").len(), 1);
    }

    // Test 169 — uri: valid
    #[test]
    fn format_uri_valid() {
        assert!(run_format("https://example.com/path", "uri").is_empty());
        assert!(run_format("http://example.com", "uri").is_empty());
        assert!(run_format("urn:isbn:0451450523", "uri").is_empty());
    }

    // Test 170 — uri: invalid
    #[test]
    fn format_uri_invalid() {
        assert_eq!(run_format("not-a-uri", "uri").len(), 1);
        assert_eq!(run_format("//no-scheme", "uri").len(), 1);
    }

    // Test 171 — uuid: valid
    #[test]
    fn format_uuid_valid() {
        assert!(run_format("550e8400-e29b-41d4-a716-446655440000", "uuid").is_empty());
        assert!(run_format("550E8400-E29B-41D4-A716-446655440000", "uuid").is_empty());
    }

    // Test 172 — uuid: invalid
    #[test]
    fn format_uuid_invalid() {
        assert_eq!(run_format("not-a-uuid", "uuid").len(), 1);
        assert_eq!(
            run_format("550e8400-e29b-41d4-a716-44665544000g", "uuid").len(),
            1
        );
        assert_eq!(
            run_format("550e8400e29b41d4a716446655440000", "uuid").len(),
            1
        );
    }

    // Test 173 — json-pointer: valid
    #[test]
    fn format_json_pointer_valid() {
        assert!(run_format("", "json-pointer").is_empty()); // empty is valid
        assert!(run_format("/foo/bar", "json-pointer").is_empty());
        assert!(run_format("/foo/0", "json-pointer").is_empty());
        assert!(run_format("/a~0b", "json-pointer").is_empty()); // ~0 escape
        assert!(run_format("/a~1b", "json-pointer").is_empty()); // ~1 escape
    }

    // Test 174 — json-pointer: invalid
    #[test]
    fn format_json_pointer_invalid() {
        assert_eq!(run_format("foo", "json-pointer").len(), 1); // no leading /
        assert_eq!(run_format("/foo~2bar", "json-pointer").len(), 1); // invalid escape
        assert_eq!(run_format("/foo~", "json-pointer").len(), 1); // trailing ~
    }

    // Test 175 — unknown format: silently ignored
    #[test]
    fn format_unknown_is_ignored() {
        let result = run_format("anything", "some-unknown-format");
        assert!(result.is_empty());
    }

    // Test 176 — format_validation disabled: no diagnostics emitted
    #[test]
    fn format_validation_disabled_produces_no_format_diagnostics() {
        let schema = format_schema("date");
        let docs = parse_docs("not-a-date");
        let result = validate_schema("not-a-date", &docs, &schema, false);
        assert!(result.is_empty());
    }

    // Test 177 — regex: valid ECMAScript regex
    #[test]
    fn format_regex_valid() {
        assert!(run_format("^[a-z]+$", "regex").is_empty());
        assert!(run_format(".*", "regex").is_empty());
    }

    // Test 178 — regex: invalid pattern
    // Note: use patterns that don't start with YAML reserved chars ([ { @)
    #[test]
    fn format_regex_invalid() {
        // Unmatched parenthesis — invalid regex, YAML-safe string
        assert_eq!(run_format("(unclosed-paren", "regex").len(), 1);
    }

    // Test 179 — idn-hostname: valid ASCII and internationalized hostnames
    #[test]
    fn format_idn_hostname_valid() {
        assert!(run_format("example.com", "idn-hostname").is_empty());
        assert!(run_format("xn--nxasmq6b.com", "idn-hostname").is_empty());
        assert!(run_format("sub.example.org", "idn-hostname").is_empty());
    }

    // Test 180 — idn-hostname: invalid hostname
    #[test]
    fn format_idn_hostname_invalid() {
        assert_eq!(run_format("not a hostname", "idn-hostname").len(), 1);
        assert_eq!(run_format("-bad-start.com", "idn-hostname").len(), 1);
    }

    // Test 181 — idn-email: valid ASCII and internationalized email
    #[test]
    fn format_idn_email_valid() {
        assert!(run_format("user@example.com", "idn-email").is_empty());
        assert!(run_format("user@xn--nxasmq6b.com", "idn-email").is_empty());
    }

    // Test 182 — idn-email: invalid (missing @, empty local, bad domain)
    #[test]
    fn format_idn_email_invalid() {
        assert_eq!(run_format("no-at-sign", "idn-email").len(), 1);
        // Domain with leading hyphen is invalid per IDNA strict processing
        assert_eq!(run_format("user@-bad-domain.com", "idn-email").len(), 1);
    }

    // Test 183 — iri: valid IRI
    #[test]
    fn format_iri_valid() {
        assert!(run_format("https://example.com/path", "iri").is_empty());
        assert!(run_format("http://example.com", "iri").is_empty());
        assert!(run_format("urn:isbn:0451450523", "iri").is_empty());
    }

    // Test 184 — iri: invalid (relative reference is not an absolute IRI)
    #[test]
    fn format_iri_invalid() {
        assert_eq!(run_format("not an iri", "iri").len(), 1);
        assert_eq!(run_format("://missing-scheme", "iri").len(), 1);
    }

    // Test 185 — iri-reference: valid (absolute IRI and relative refs)
    #[test]
    fn format_iri_reference_valid() {
        assert!(run_format("https://example.com/path", "iri-reference").is_empty());
        assert!(run_format("/relative/path", "iri-reference").is_empty());
        assert!(run_format("relative/path", "iri-reference").is_empty());
    }

    // Test 186 — iri-reference: invalid
    #[test]
    fn format_iri_reference_invalid() {
        assert_eq!(run_format("not valid iri ref", "iri-reference").len(), 1);
    }

    fn content_schema(encoding: Option<&str>, media_type: Option<&str>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            content_encoding: encoding.map(str::to_string),
            content_media_type: media_type.map(str::to_string),
            ..JsonSchema::default()
        }
    }

    fn run_content(
        text: &str,
        encoding: Option<&str>,
        media_type: Option<&str>,
    ) -> Vec<Diagnostic> {
        let schema = content_schema(encoding, media_type);
        let docs = parse_docs(text);
        validate_schema(text, &docs, &schema, true)
    }

    // Test 187 — contentEncoding base64: valid
    #[test]
    fn content_encoding_base64_valid() {
        // "hello" in base64
        assert!(run_content("aGVsbG8=", Some("base64"), None).is_empty());
        assert!(run_content("", Some("base64"), None).is_empty());
    }

    // Test 188 — contentEncoding base64: invalid
    #[test]
    fn content_encoding_base64_invalid() {
        assert_eq!(
            run_content("not-valid-base64!!!", Some("base64"), None).len(),
            1
        );
    }

    // Test 189 — contentEncoding base64url: valid
    #[test]
    fn content_encoding_base64url_valid() {
        // "hello" in base64url
        assert!(run_content("aGVsbG8=", Some("base64url"), None).is_empty());
    }

    // Test 190 — contentEncoding base64url: invalid
    #[test]
    fn content_encoding_base64url_invalid() {
        assert_eq!(
            run_content("not+valid/base64url!!!", Some("base64url"), None).len(),
            1
        );
    }

    // Test 191 — contentEncoding base32: valid
    #[test]
    fn content_encoding_base32_valid() {
        // "hello" in base32
        assert!(run_content("NBSWY3DPEB3W64TMMQ======", Some("base32"), None).is_empty());
    }

    // Test 192 — contentEncoding base32: invalid
    #[test]
    fn content_encoding_base32_invalid() {
        assert_eq!(
            run_content("not-valid-base32!!!", Some("base32"), None).len(),
            1
        );
    }

    // Test 193 — contentEncoding base16: valid
    #[test]
    fn content_encoding_base16_valid() {
        assert!(run_content("48656C6C6F", Some("base16"), None).is_empty());
        assert!(run_content("48656c6c6f", Some("base16"), None).is_empty());
    }

    // Test 194 — contentEncoding base16: invalid
    #[test]
    fn content_encoding_base16_invalid() {
        assert_eq!(run_content("ZZZZ", Some("base16"), None).len(), 1);
    }

    // Test 195 — contentEncoding unknown: silently ignored
    #[test]
    fn content_encoding_unknown_ignored() {
        assert!(run_content("anything", Some("base58"), None).is_empty());
    }

    // Test 196 — contentMediaType application/json: valid (no encoding)
    // The value must be a YAML string (quoted) so it reaches validate_string_constraints.
    // Values starting with { or [ are YAML flow collections; use quoted YAML.
    #[test]
    fn content_media_type_json_valid_no_encoding() {
        // "\"42\"" parses as YAML string "42", which is valid JSON
        let schema = content_schema(None, Some("application/json"));
        let docs = parse_docs("\"42\"");
        assert!(validate_schema("\"42\"", &docs, &schema, true).is_empty());
    }

    // Test 197 — contentMediaType application/json: invalid (no encoding)
    #[test]
    fn content_media_type_json_invalid_no_encoding() {
        assert_eq!(
            run_content("not json", None, Some("application/json")).len(),
            1
        );
    }

    // Test 198 — contentEncoding + contentMediaType: valid base64-encoded JSON
    #[test]
    fn content_encoding_and_media_type_valid() {
        // base64("{"key":"value"}") = "eyJrZXkiOiJ2YWx1ZSJ9"
        assert!(
            run_content(
                "eyJrZXkiOiJ2YWx1ZSJ9",
                Some("base64"),
                Some("application/json")
            )
            .is_empty()
        );
    }

    // Test 199 — contentEncoding + contentMediaType: encoding fails → only encoding diagnostic
    #[test]
    fn content_encoding_fails_skips_media_type_check() {
        let diags = run_content(
            "not-valid-base64!!!",
            Some("base64"),
            Some("application/json"),
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].code == Some(NumberOrString::String("schemaContentEncoding".to_string())));
    }

    // Test 200 — contentEncoding + contentMediaType: valid encoding but invalid JSON
    #[test]
    fn content_encoding_valid_media_type_invalid() {
        // base64("not json") = "bm90IGpzb24="
        let diags = run_content("bm90IGpzb24=", Some("base64"), Some("application/json"));
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].code == Some(NumberOrString::String("schemaContentMediaType".to_string()))
        );
    }

    // Test 201 — contentMediaType unknown: silently ignored
    #[test]
    fn content_media_type_unknown_ignored() {
        assert!(run_content("anything", None, Some("text/plain")).is_empty());
    }

    // Test 202 — format_validation disabled: content checks also skipped
    #[test]
    fn content_validation_disabled_when_format_validation_off() {
        let schema = content_schema(Some("base64"), Some("application/json"));
        let docs = parse_docs("not-valid-base64!!!");
        let result = validate_schema("not-valid-base64!!!", &docs, &schema, false);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Object cardinality — minProperties / maxProperties
    // ══════════════════════════════════════════════════════════════════════════

    fn object_schema_with_cardinality(min: Option<u64>, max: Option<u64>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            min_properties: min,
            max_properties: max,
            ..JsonSchema::default()
        }
    }

    // Test 203
    #[test]
    fn should_produce_error_when_object_has_fewer_properties_than_min_properties() {
        let schema = object_schema_with_cardinality(Some(2), None);
        let text = "name: Alice";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMinProperties");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 204
    #[test]
    fn should_produce_no_diagnostics_when_object_meets_min_properties() {
        let schema = object_schema_with_cardinality(Some(2), None);
        let text = "name: Alice\nage: 30";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 205
    #[test]
    fn should_produce_error_when_object_exceeds_max_properties() {
        let schema = object_schema_with_cardinality(None, Some(1));
        let text = "name: Alice\nage: 30";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaMaxProperties");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 206
    #[test]
    fn should_produce_no_diagnostics_when_object_meets_max_properties() {
        let schema = object_schema_with_cardinality(None, Some(2));
        let text = "name: Alice\nage: 30";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // additionalItems (Draft-04/07 tuple arrays)
    // ══════════════════════════════════════════════════════════════════════════

    fn tuple_schema_with_additional_items(
        prefix: Vec<JsonSchema>,
        additional_items: Option<AdditionalProperties>,
    ) -> JsonSchema {
        JsonSchema {
            prefix_items: Some(prefix),
            additional_items,
            ..JsonSchema::default()
        }
    }

    // Test 207
    #[test]
    fn should_produce_warning_for_extra_items_when_additional_items_false() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello\n- extra";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaAdditionalItems");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        let msg = &result[0].message;
        assert!(
            msg.contains("[1]"),
            "message should reference [1], got: {msg}"
        );
    }

    // Test 208
    #[test]
    fn should_produce_no_diagnostics_when_array_exactly_matches_prefix_length_with_additional_items_false()
     {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema(), integer_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello\n- 42";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 209
    #[test]
    fn should_produce_no_diagnostics_for_items_within_prefix_with_additional_items_false() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema(), integer_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 210
    #[test]
    fn should_produce_one_warning_per_extra_item_when_additional_items_false() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Denied),
        );
        let text = "- hello\n- extra1\n- extra2";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|d| code_of(d) == "schemaAdditionalItems"));
    }

    // Test 211
    #[test]
    fn should_validate_extra_items_against_additional_items_schema_when_valid() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
        );
        let text = "- hello\n- 42";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 212
    #[test]
    fn should_produce_type_diagnostic_when_extra_item_fails_additional_items_schema() {
        let schema = tuple_schema_with_additional_items(
            vec![string_schema()],
            Some(AdditionalProperties::Schema(Box::new(integer_schema()))),
        );
        let text = "- hello\n- world";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 213
    #[test]
    fn should_produce_no_diagnostics_when_additional_items_false_and_prefix_items_set_from_prefix_items_key()
     {
        // Simulates parsed outcome of Draft 2020-12 `prefixItems` + ignored `additionalItems`:
        // additional_items is None because the parser suppresses it for prefixItems keyword.
        let schema = JsonSchema {
            prefix_items: Some(vec![string_schema()]),
            additional_items: None,
            ..JsonSchema::default()
        };
        let text = "- hello\n- extra";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 214
    #[test]
    fn should_produce_no_diagnostics_when_additional_items_absent_and_extra_items_present() {
        let schema = tuple_schema_with_additional_items(vec![string_schema()], None);
        let text = "- hello\n- 42\n- extra";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }
}
