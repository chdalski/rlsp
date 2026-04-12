// SPDX-License-Identifier: MIT

use std::cell::RefCell;
use std::collections::HashMap;

use regex::RegexBuilder;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::scalar_helpers;
use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};

/// Helper: check if a mapping's entries contain a key with the given string value.
fn entries_contains_key(entries: &[(Node<Span>, Node<Span>)], key: &str) -> bool {
    entries
        .iter()
        .any(|(k, _)| matches!(k, Node::Scalar { value, .. } if value == key))
}

/// Helper: extract a string key from a node.
fn node_key_str(node: &Node<Span>) -> Option<String> {
    match node {
        Node::Scalar { value, .. } => Some(value.clone()),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
    }
}

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
/// Bundles the parameters that every helper needs — the diagnostic accumulator,
/// the `format_validation` flag, and a pre-built key index for O(1) position
/// lookups — so individual helpers do not need many arguments.
struct Ctx<'a> {
    diagnostics: &'a mut Vec<Diagnostic>,
    format_validation: bool,
    /// Pre-built index: key string → Range in the document.
    /// Built once in `validate_schema`; replaces per-diagnostic O(n) scans.
    key_index: &'a HashMap<String, Range>,
}

impl<'a> Ctx<'a> {
    const fn new(
        diagnostics: &'a mut Vec<Diagnostic>,
        format_validation: bool,
        key_index: &'a HashMap<String, Range>,
    ) -> Self {
        Self {
            diagnostics,
            format_validation,
            key_index,
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
    docs: &[Document<Span>],
    schema: &JsonSchema,
    format_validation: bool,
) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diagnostics = Vec::new();

    // Build a key → Range index once so all position lookups are O(1)
    // instead of scanning all lines for every diagnostic emitted.
    // Uses the same matching logic as the removed find_key_range scan.
    let key_index = build_key_index(&lines);

    let mut ctx = Ctx::new(&mut diagnostics, format_validation, &key_index);

    for doc in docs {
        validate_node(&doc.root, schema, &[], &mut ctx, 0);
    }

    diagnostics
}

/// Build a `HashMap` from key string to its `Range` in the document.
///
/// Scans `lines` once, applying the same matching logic as `find_key_range`:
/// - Strips a leading `"- "` sequence-item marker if present
/// - Matches `key:` and `key :` patterns (colon-only split, space within key preserved)
/// - First occurrence of each key wins (preserves `find_key_range` semantics)
fn build_key_index(lines: &[&str]) -> HashMap<String, Range> {
    let mut index = HashMap::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let col_offset = line.len() - trimmed.len();
        let candidate = trimmed.strip_prefix("- ").unwrap_or(trimmed);

        // Split at the first ':' and trim trailing whitespace from the key.
        // This preserves spaces within keys (e.g. "foo bar: value" → "foo bar")
        // and handles "key : value" (space before colon).
        let Some(colon_pos) = candidate.find(':') else {
            continue;
        };
        let key = candidate[..colon_pos].trim_end();
        if key.is_empty() {
            continue;
        }

        // col is the indentation of the original line, matching find_key_range.
        let col = u32::try_from(col_offset).unwrap_or(0);
        let end_col = col + u32::try_from(key.len()).unwrap_or(0);
        let line_u32 = u32::try_from(line_idx).unwrap_or(0);
        let range = Range::new(
            Position::new(line_u32, col),
            Position::new(line_u32, end_col),
        );

        // First-occurrence wins — matches find_key_range find_map semantics.
        index.entry(key.to_string()).or_insert(range);
    }
    index
}

// ──────────────────────────────────────────────────────────────────────────────
// Core recursive validation
// ──────────────────────────────────────────────────────────────────────────────

/// Recursively validate a YAML node against a schema.
///
/// `path` is the property path to the current node (for diagnostic messages).
/// `depth` guards against stack overflow on deeply nested structures.
fn validate_node(
    node: &Node<Span>,
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
            let range = node_range(path, ctx.key_index);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match type: expected {}, got {}",
                    format_path(path),
                    display_schema_type(schema_type),
                    yaml_type,
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
        let range = node_range(path, ctx.key_index);
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
    if let Node::Mapping { entries, .. } = node {
        validate_mapping(entries, schema, path, ctx, depth);
    }

    // Sequence-specific checks
    if let Node::Sequence { items, .. } = node {
        validate_sequence(items, schema, path, ctx, depth);
    }

    // Composition
    validate_composition(node, schema, path, ctx, depth);

    // unevaluatedProperties (Draft 2019-09)
    if schema.unevaluated_properties.is_some() {
        if let Node::Mapping { entries, .. } = node {
            validate_unevaluated_properties(entries, schema, path, ctx, depth);
        }
    }

    // unevaluatedItems (Draft 2019-09)
    if schema.unevaluated_items.is_some() {
        if let Node::Sequence { items, .. } = node {
            validate_unevaluated_items(items, schema, path, ctx, depth);
        }
    }
}

fn validate_unevaluated_properties(
    entries: &[(Node<Span>, Node<Span>)],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    for (k, v) in entries {
        let Some(key_str) = node_key_str(k) else {
            continue;
        };
        if collect_evaluated_properties(schema, &key_str) {
            continue;
        }
        match &schema.unevaluated_properties {
            Some(AdditionalProperties::Denied) => {
                let range = key_range(&key_str, path, ctx.key_index);
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
                let mut child_path = path.to_vec();
                child_path.push(key_str.clone());
                validate_node(v, extra_schema, &child_path, ctx, depth + 1);
            }
            None => {}
        }
    }
}

fn validate_unevaluated_items(
    seq: &[Node<Span>],
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
    seq: &[Node<Span>],
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
                    let range = node_range(&item_path, ctx.key_index);
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
    seq: &[Node<Span>],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let len = seq.len() as u64;

    if let Some(min) = schema.min_items {
        if len < min {
            let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
    entries: &[(Node<Span>, Node<Span>)],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    let len = entries.len() as u64;

    if let Some(min) = schema.min_properties {
        if len < min {
            let range = mapping_range(path, ctx.key_index);
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
            let range = mapping_range(path, ctx.key_index);
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
    seq: &[Node<Span>],
    contains_schema: &JsonSchema,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let key_index = ctx.key_index;
    let format_validation = ctx.format_validation;
    let match_count = seq
        .iter()
        .filter(|item| {
            let mut scratch = Vec::new();
            let mut probe = Ctx::new(&mut scratch, format_validation, key_index);
            validate_node(item, contains_schema, path, &mut probe, depth + 1);
            scratch.is_empty()
        })
        .count() as u64;

    // Default min is 1 when `contains` is present without `minContains`
    let effective_min = schema.min_contains.unwrap_or(1);

    if match_count < effective_min {
        let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
    node: &Node<Span>,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
) {
    use rlsp_yaml_parser::ScalarStyle;
    if let Node::Scalar { value, style, .. } = node {
        let is_plain = matches!(style, ScalarStyle::Plain);

        // String constraints apply to all scalars that resolve to string type.
        // Quoted/block scalars are always strings; plain scalars are strings
        // only if they don't match null/bool/int/float patterns.
        if !is_plain
            || (!scalar_helpers::is_null(value)
                && !scalar_helpers::is_bool(value)
                && !scalar_helpers::is_integer(value)
                && !scalar_helpers::is_float(value))
        {
            validate_string_constraints(value, schema, path, ctx);
        }

        // Numeric constraints only apply to plain scalars.
        if is_plain {
            let numeric_val = scalar_helpers::parse_integer(value)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    {
                        i as f64
                    }
                })
                .or_else(|| scalar_helpers::parse_float(value));
            if let Some(val) = numeric_val {
                validate_numeric_constraints(val, schema, path, ctx);
            }
        }
    }

    // const — compare any scalar node via yaml_to_json
    if let Some(const_val) = &schema.const_value {
        if let Some(yaml_val) = yaml_to_json(node) {
            if yaml_val != *const_val {
                let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
                let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
            validate_format(s, format, path, ctx.key_index, ctx.diagnostics);
        }
        if schema.content_encoding.is_some()
            || schema.content_media_type.is_some()
            || schema.content_schema.is_some()
        {
            validate_content(s, schema, path, ctx.key_index, ctx.diagnostics);
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
    key_index: &HashMap<String, Range>,
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
        let range = node_range(path, key_index);
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

/// Validates `contentEncoding`, `contentMediaType`, and `contentSchema` keywords.
///
/// Decodes the string using the declared encoding, then (if set) checks the
/// decoded bytes against the declared media type. Finally, if `contentSchema`
/// is present, parses the (possibly decoded) content as YAML and validates
/// the result against the sub-schema.
fn validate_content(
    s: &str,
    schema: &JsonSchema,
    path: &[String],
    key_index: &HashMap<String, Range>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Step 1: decode if contentEncoding is set
    let decoded_bytes: Option<Vec<u8>> = if let Some(enc) = &schema.content_encoding {
        let result = match enc.as_str() {
            "base64" => data_encoding::BASE64.decode(s.as_bytes()),
            "base64url" => data_encoding::BASE64URL.decode(s.as_bytes()),
            "base32" => data_encoding::BASE32.decode(s.as_bytes()),
            "base16" => data_encoding::HEXUPPER_PERMISSIVE.decode(s.as_bytes()),
            // Unknown encoding — skip all checks
            _ => return,
        };
        if let Ok(bytes) = result {
            Some(bytes)
        } else {
            let range = node_range(path, key_index);
            diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "schemaContentEncoding",
                format!(
                    "String at {} is not valid {enc} encoded data",
                    format_path(path)
                ),
            ));
            // Encoding failed — skip media type and schema checks
            return;
        }
    } else {
        // No encoding set — use raw string bytes for media type check
        None
    };

    // Step 2: check media type if set
    if let Some(media_type) = &schema.content_media_type {
        if media_type == "application/json" {
            let text = decoded_bytes
                .as_ref()
                .map_or(Some(s), |bytes| std::str::from_utf8(bytes).ok());
            let valid = text.is_some_and(|t| serde_json::from_str::<serde_json::Value>(t).is_ok());
            if !valid {
                let range = node_range(path, key_index);
                diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::WARNING,
                    "schemaContentMediaType",
                    format!(
                        "String at {} does not contain valid {media_type} content",
                        format_path(path)
                    ),
                ));
                // Media type check failed — skip contentSchema validation
                return;
            }
        }
        // Unknown media type — fall through to contentSchema if present
    }

    // Step 3: validate decoded content against contentSchema if present
    validate_content_schema(
        s,
        decoded_bytes.as_deref(),
        schema,
        path,
        key_index,
        diagnostics,
    );
}

/// If `contentSchema` is present, parse the (possibly decoded) content as YAML
/// and validate the parsed result against the sub-schema.
fn validate_content_schema(
    raw: &str,
    decoded_bytes: Option<&[u8]>,
    schema: &JsonSchema,
    path: &[String],
    key_index: &HashMap<String, Range>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(content_schema) = &schema.content_schema else {
        return;
    };

    // Determine the text to parse: decoded bytes (as UTF-8) or raw string.
    let content_text = decoded_bytes
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or(raw);

    // Parse the content as YAML.
    let Ok(docs) = rlsp_yaml_parser::load(content_text) else {
        let range = node_range(path, key_index);
        diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::WARNING,
            "schemaContentSchema",
            format!(
                "Decoded content at {} could not be parsed as YAML",
                format_path(path)
            ),
        ));
        return;
    };

    // Validate each parsed document against the content schema.
    for doc in &docs {
        let mut content_path = path.to_vec();
        content_path.push("(content)".to_string());
        let content_key_index = build_key_index(&content_text.lines().collect::<Vec<_>>());
        let mut ctx = Ctx::new(diagnostics, true, &content_key_index);
        validate_node(&doc.root, content_schema, &content_path, &mut ctx, 0);
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
            let range = node_range(path, ctx.key_index);
            let msg = if exclusive {
                format!(
                    "Value at {} is below exclusive minimum {minimum}",
                    format_path(path),
                )
            } else {
                format!("Value at {} is below minimum {minimum}", format_path(path),)
            };
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinimum",
                msg,
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
            let range = node_range(path, ctx.key_index);
            let msg = if exclusive {
                format!(
                    "Value at {} is above exclusive maximum {maximum}",
                    format_path(path),
                )
            } else {
                format!("Value at {} is above maximum {maximum}", format_path(path),)
            };
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaximum",
                msg,
            ));
        }
    }

    // exclusiveMinimum (Draft-06+ number form)
    if let Some(excl_min) = schema.exclusive_minimum {
        if val <= excl_min {
            let range = node_range(path, ctx.key_index);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinimum",
                format!(
                    "Value at {} is below exclusive minimum {excl_min}",
                    format_path(path),
                ),
            ));
        }
    }

    // exclusiveMaximum (Draft-06+ number form)
    if let Some(excl_max) = schema.exclusive_maximum {
        if val >= excl_max {
            let range = node_range(path, ctx.key_index);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaximum",
                format!(
                    "Value at {} is above exclusive maximum {excl_max}",
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
                let range = node_range(path, ctx.key_index);
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
    entries: &[(Node<Span>, Node<Span>)],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    validate_mapping_constraints(entries, schema, path, ctx);

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
                .filter(|req_key| !entries_contains_key(entries, req_key))
                .map(|req_key| {
                    let range = mapping_range(path, ctx.key_index);
                    make_diagnostic(
                        range,
                        DiagnosticSeverity::ERROR,
                        "schemaRequired",
                        format!(
                            "Object at {} is missing required property '{}'. Expected: {}.",
                            format_path(path),
                            req_key,
                            props_list
                        ),
                    )
                }),
        );
    }

    // Validate known properties and check for additional properties
    for (k, v) in entries {
        let Some(key_str) = node_key_str(k) else {
            continue;
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
                        let range = key_range(&key_str, path, ctx.key_index);
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
            // Create a temporary scalar node for the key string.
            let key_node = Node::Scalar {
                value: key_str.clone(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                anchor: None,
                tag: None,
                loc: rlsp_yaml_parser::Span {
                    start: rlsp_yaml_parser::Pos::ORIGIN,
                    end: rlsp_yaml_parser::Pos::ORIGIN,
                },
                leading_comments: Vec::new(),
                trailing_comment: None,
            };
            validate_node(&key_node, pn_schema, path, ctx, depth + 1);
        }
    }

    validate_dependencies(entries, schema, path, ctx, depth);
}

fn validate_dependencies(
    entries: &[(Node<Span>, Node<Span>)],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    // dependentRequired: if trigger key is present, listed keys must also be present
    if let Some(dep_req) = &schema.dependent_required {
        for (trigger, required_keys) in dep_req {
            if entries_contains_key(entries, trigger) {
                for missing in required_keys {
                    if !entries_contains_key(entries, missing) {
                        let range = mapping_range(path, ctx.key_index);
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
            if entries_contains_key(entries, trigger) {
                // Reconstruct a mapping node to validate against the dep schema.
                let mapping_node = Node::Mapping {
                    entries: entries.to_vec(),
                    anchor: None,
                    tag: None,
                    loc: rlsp_yaml_parser::Span {
                        start: rlsp_yaml_parser::Pos::ORIGIN,
                        end: rlsp_yaml_parser::Pos::ORIGIN,
                    },
                    leading_comments: Vec::new(),
                    trailing_comment: None,
                };
                validate_node(&mapping_node, dep_schema, path, ctx, depth + 1);
            }
        }
    }
}

/// Validate `value` against any `patternProperties` patterns that match `key`.
/// Returns `true` if the key was matched by at least one pattern.
fn validate_pattern_properties(
    value: &Node<Span>,
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
            let range = node_range(path, ctx.key_index);
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
            let range = node_range(path, ctx.key_index);
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
    node: &Node<Span>,
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
        let key_index = ctx.key_index;
        let format_validation = ctx.format_validation;
        let branch_count = any_of.iter().take(MAX_BRANCH_COUNT).count();
        let any_passes = any_of.iter().take(MAX_BRANCH_COUNT).any(|branch| {
            let mut scratch = Vec::new();
            let mut probe = Ctx::new(&mut scratch, format_validation, key_index);
            validate_node(node, branch, path, &mut probe, depth + 1);
            scratch.is_empty()
        });
        if !any_passes {
            let range = node_range(path, ctx.key_index);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match any of the {branch_count} allowed schemas (anyOf)",
                    format_path(path)
                ),
            ));
        }
    }

    // oneOf: exactly one branch must pass
    if let Some(one_of) = &schema.one_of {
        let key_index = ctx.key_index;
        let format_validation = ctx.format_validation;
        let total = one_of.iter().take(MAX_BRANCH_COUNT).count();
        let passing = one_of
            .iter()
            .take(MAX_BRANCH_COUNT)
            .filter(|branch| {
                let mut scratch = Vec::new();
                let mut probe = Ctx::new(&mut scratch, format_validation, key_index);
                validate_node(node, branch, path, &mut probe, depth + 1);
                scratch.is_empty()
            })
            .count();

        if passing == 0 {
            let range = node_range(path, ctx.key_index);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match any of the {total} oneOf schemas",
                    format_path(path)
                ),
            ));
        } else if passing > 1 {
            let range = node_range(path, ctx.key_index);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} matches {passing} of the {total} oneOf schemas (expected exactly 1)",
                    format_path(path)
                ),
            ));
        }
    }

    // not: the value must NOT match the sub-schema
    if let Some(not_schema) = &schema.not {
        let key_index = ctx.key_index;
        let format_validation = ctx.format_validation;
        let mut scratch = Vec::new();
        let mut probe = Ctx::new(&mut scratch, format_validation, key_index);
        validate_node(node, not_schema, path, &mut probe, depth + 1);
        if scratch.is_empty() {
            let range = node_range(path, ctx.key_index);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaNot",
                format!(
                    "Value at {} must not match the schema defined in 'not'",
                    format_path(path)
                ),
            ));
        }
    }

    // if / then / else (Draft-07)
    if let Some(if_schema) = &schema.if_schema {
        let key_index = ctx.key_index;
        let format_validation = ctx.format_validation;
        let mut scratch = Vec::new();
        let mut probe = Ctx::new(&mut scratch, format_validation, key_index);
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

fn yaml_type_name(node: &Node<Span>) -> &'static str {
    use rlsp_yaml_parser::ScalarStyle;
    match node {
        Node::Scalar { value, style, .. } => {
            // Only plain (unquoted) scalars undergo type inference.
            // Quoted and block scalars are always strings.
            if !matches!(style, ScalarStyle::Plain) {
                return "string";
            }
            if scalar_helpers::is_null(value) {
                "null"
            } else if scalar_helpers::is_bool(value) {
                "boolean"
            } else if scalar_helpers::is_integer(value) {
                "integer"
            } else if scalar_helpers::is_float(value) {
                "number"
            } else {
                "string"
            }
        }
        Node::Mapping { .. } => "object",
        Node::Sequence { .. } => "array",
        Node::Alias { .. } => "unknown",
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

fn yaml_to_json(node: &Node<Span>) -> Option<serde_json::Value> {
    use rlsp_yaml_parser::ScalarStyle;
    match node {
        Node::Scalar { value, style, .. } => {
            // Quoted/block scalars are always strings — skip type inference.
            if !matches!(style, ScalarStyle::Plain) {
                return Some(serde_json::Value::String(value.clone()));
            }
            if scalar_helpers::is_null(value) {
                Some(serde_json::Value::Null)
            } else if scalar_helpers::is_bool(value) {
                Some(serde_json::Value::Bool(matches!(
                    value.as_str(),
                    "true" | "True" | "TRUE"
                )))
            } else if let Some(i) = scalar_helpers::parse_integer(value) {
                Some(serde_json::Value::Number(i.into()))
            } else if let Some(f) = scalar_helpers::parse_float(value) {
                serde_json::Number::from_f64(f).map(serde_json::Value::Number)
            } else {
                Some(serde_json::Value::String(value.clone()))
            }
        }
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
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

/// Find the range for a node identified by its path.
/// Falls back to `(0,0)-(0,0)` if not found.
fn node_range(path: &[String], key_index: &HashMap<String, Range>) -> Range {
    path.last().map_or_else(
        || Range::new(Position::new(0, 0), Position::new(0, 0)),
        |key| find_key_range(key, key_index),
    )
}

/// Find the range for the opening of a mapping (for required-property errors).
fn mapping_range(path: &[String], key_index: &HashMap<String, Range>) -> Range {
    path.last().map_or_else(
        || Range::new(Position::new(0, 0), Position::new(0, 0)),
        |key| find_key_range(key, key_index),
    )
}

/// Find the range for a specific key within the document text.
fn key_range(key: &str, _path: &[String], key_index: &HashMap<String, Range>) -> Range {
    find_key_range(key, key_index)
}

/// Look up `key` in the pre-built index and return its Range.
/// Returns `(0,0)-(0,0)` if the key is not found.
fn find_key_range(key: &str, key_index: &HashMap<String, Range>) -> Range {
    // Strip array-index brackets if present (e.g. "[0]")
    let key = key.trim_start_matches('[').trim_end_matches(']');
    key_index
        .get(key)
        .copied()
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
    use rstest::rstest;

    use super::*;
    use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};
    use serde_json::json;

    fn parse_docs(text: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(text).unwrap_or_default()
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

    // Tests 8–13: type mismatch → schemaType ERROR
    #[rstest]
    #[case::string_where_integer_expected(
        object_schema_with_props(vec![("count", integer_schema())]),
        "count: \"hello\""
    )]
    #[case::integer_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: 42"
    )]
    #[case::boolean_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: true"
    )]
    #[case::mapping_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name:\n  nested: value"
    )]
    #[case::sequence_where_object_expected(
        object_schema_with_props(vec![("config", JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        })]),
        "config:\n  - item"
    )]
    #[case::null_where_string_expected(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: ~"
    )]
    fn type_mismatch_produces_schematype_error(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 8: message names the expected type
    #[test]
    fn type_mismatch_message_names_expected_type() {
        let schema = object_schema_with_props(vec![("count", integer_schema())]);
        let docs = parse_docs("count: \"hello\"");
        let result = validate_schema("count: \"hello\"", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("integer"));
    }

    // Tests 7, 14–18: correct type → no diagnostics
    #[rstest]
    #[case::string_value_matches_string_type(
        object_schema_with_props(vec![("name", string_schema())]),
        "name: Alice"
    )]
    #[case::null_in_type_array_accepts_null(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Multiple(vec![
                "string".to_string(),
                "null".to_string(),
            ])),
            ..JsonSchema::default()
        })]),
        "name: ~"
    )]
    #[case::no_type_specified_accepts_any(
        object_schema_with_props(vec![("name", JsonSchema::default())]),
        "name: 42"
    )]
    #[case::integer_value_matches_integer_type(
        object_schema_with_props(vec![("port", integer_schema())]),
        "port: 8080"
    )]
    #[case::boolean_value_matches_boolean_type(
        object_schema_with_props(vec![("enabled", JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            ..JsonSchema::default()
        })]),
        "enabled: true"
    )]
    #[case::sequence_value_matches_array_type(
        object_schema_with_props(vec![("items", JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            ..JsonSchema::default()
        })]),
        "items:\n  - one\n  - two"
    )]
    fn type_match_produces_no_diagnostics(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Enum violations
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 19, 21, 23: enum match → no diagnostics
    #[rstest]
    #[case::string_value_in_string_enum(
        object_schema_with_props(vec![("env", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            enum_values: Some(vec![json!("prod"), json!("staging"), json!("dev")]),
            ..JsonSchema::default()
        })]),
        "env: staging"
    )]
    #[case::integer_value_in_integer_enum(
        object_schema_with_props(vec![("level", JsonSchema {
            enum_values: Some(vec![json!(1), json!(2), json!(3)]),
            ..JsonSchema::default()
        })]),
        "level: 2"
    )]
    #[case::string_value_in_mixed_type_enum(
        object_schema_with_props(vec![("value", JsonSchema {
            enum_values: Some(vec![json!("auto"), json!(0), serde_json::Value::Null]),
            ..JsonSchema::default()
        })]),
        "value: auto"
    )]
    fn enum_match_produces_no_diagnostics(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 20: enum value missing — message lists valid values (unique assertion shape)
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

    // Test 22: integer enum mismatch → schemaEnum error
    #[rstest]
    #[case::integer_value_not_in_enum(
        object_schema_with_props(vec![("level", JsonSchema {
            enum_values: Some(vec![json!(1), json!(2), json!(3)]),
            ..JsonSchema::default()
        })]),
        "level: 5"
    )]
    fn enum_mismatch_produces_schemaenum_error(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaEnum");
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
        assert_eq!(
            result
                .iter()
                .filter(|d| code_of(d) == "schemaRequired")
                .count(),
            1
        );
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
            msg.contains("Expected:"),
            "message should contain 'Expected:', got: {msg}"
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

    // Tests 70–71: rejected pattern → schemaPatternLimit WARNING
    #[rstest]
    #[case::pattern_exceeds_max_length("a".repeat(1025))]
    #[case::pattern_fails_to_compile("[invalid".to_string())]
    fn pattern_rejected_produces_schemapatternlimit_warning(#[case] pattern: String) {
        let schema = object_schema_with_props(vec![(
            "val",
            JsonSchema {
                pattern: Some(pattern),
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

    // Tests 72, 74: string meets length constraint → no diagnostics
    #[rstest]
    #[case::string_meets_min_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            min_length: Some(3),
            ..JsonSchema::default()
        })]),
        "name: abc"
    )]
    #[case::string_meets_max_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            max_length: Some(10),
            ..JsonSchema::default()
        })]),
        "name: hello"
    )]
    fn string_length_constraint_valid_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Tests 73, 75: string violates length constraint → error with specific code
    #[rstest]
    #[case::string_shorter_than_min_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            min_length: Some(5),
            ..JsonSchema::default()
        })]),
        "name: hi",
        "schemaMinLength"
    )]
    #[case::string_exceeds_max_length(
        object_schema_with_props(vec![("name", JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            max_length: Some(3),
            ..JsonSchema::default()
        })]),
        "name: toolong",
        "schemaMaxLength"
    )]
    fn string_length_constraint_violated_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — minimum / maximum (inclusive)
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 76, 78: integer meets inclusive bound → no diagnostics
    #[rstest]
    #[case::integer_meets_minimum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            minimum: Some(1.0),
            ..JsonSchema::default()
        })]),
        "port: 80"
    )]
    #[case::integer_meets_maximum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            maximum: Some(65535.0),
            ..JsonSchema::default()
        })]),
        "port: 8080"
    )]
    fn numeric_inclusive_bound_valid_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Tests 77, 79: integer violates inclusive bound → error with specific code
    #[rstest]
    #[case::integer_below_minimum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            minimum: Some(1.0),
            ..JsonSchema::default()
        })]),
        "port: 0",
        "schemaMinimum"
    )]
    #[case::integer_exceeds_maximum(
        object_schema_with_props(vec![("port", JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            maximum: Some(65535.0),
            ..JsonSchema::default()
        })]),
        "port: 99999",
        "schemaMaximum"
    )]
    fn numeric_inclusive_bound_violated_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — Draft-04 exclusiveMinimum / exclusiveMaximum (bool)
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 80, 82: Draft-04 exclusiveMinimum/Maximum=true at boundary → error
    #[rstest]
    #[case::value_equals_exclusive_minimum(
        object_schema_with_props(vec![("val", JsonSchema {
            minimum: Some(5.0),
            exclusive_minimum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "schemaMinimum"
    )]
    #[case::value_equals_exclusive_maximum(
        object_schema_with_props(vec![("val", JsonSchema {
            maximum: Some(10.0),
            exclusive_maximum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "schemaMaximum"
    )]
    fn draft04_exclusive_bound_at_boundary_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
    }

    // Test 81: exclusive=false at boundary → no error (unique schema combination)
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

    // ══════════════════════════════════════════════════════════════════════════
    // Scalar constraints — Draft-06+ exclusiveMinimum / exclusiveMaximum (f64)
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 83, 85: Draft-06 exclusive bound at boundary → error
    #[rstest]
    #[case::value_equals_exclusive_minimum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_minimum: Some(5.0),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "schemaMinimum"
    )]
    #[case::value_equals_exclusive_maximum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_maximum: Some(10.0),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "schemaMaximum"
    )]
    fn draft06_exclusive_bound_at_boundary_produces_error(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_code: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), expected_code);
    }

    // Tests 84, 86: Draft-06 exclusive bound past boundary → no diagnostics
    #[rstest]
    #[case::value_exceeds_exclusive_minimum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_minimum: Some(5.0),
            ..JsonSchema::default()
        })]),
        "val: 6"
    )]
    #[case::value_below_exclusive_maximum(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_maximum: Some(10.0),
            ..JsonSchema::default()
        })]),
        "val: 9"
    )]
    fn draft06_exclusive_bound_past_boundary_produces_no_diagnostics(
        #[case] schema: JsonSchema,
        #[case] text: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
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

    // Tests 89, 91: value equals const → no diagnostics
    #[rstest]
    #[case::string_value_equals_const(
        object_schema_with_props(vec![("version", JsonSchema {
            const_value: Some(json!("v1")),
            ..JsonSchema::default()
        })]),
        "version: v1"
    )]
    #[case::integer_value_equals_const(
        object_schema_with_props(vec![("level", JsonSchema {
            const_value: Some(json!(42)),
            ..JsonSchema::default()
        })]),
        "level: 42"
    )]
    fn const_match_produces_no_diagnostics(#[case] schema: JsonSchema, #[case] text: &str) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(result.is_empty());
    }

    // Test 90: const mismatch → schemaConst ERROR (standalone)
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
        assert!(result.iter().all(|d| code_of(d) != "schemaConst"));
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
        assert_eq!(
            result.iter().filter(|d| code_of(d) == "schemaType").count(),
            1
        );
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
        assert_eq!(
            result
                .iter()
                .filter(|d| code_of(d) == "schemaPattern")
                .count(),
            2
        );
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
        assert!(
            result.iter().all(|d| !d.message.contains("extra")),
            "extra should be evaluated by then"
        );
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

    // Tests 157, 159, 161, 163, 165, 167, 169, 171, 173, 175, 177, 179, 181, 183, 185:
    // valid format value → no diagnostics
    #[rstest]
    #[case::date_time_utc("2023-01-15T10:30:00Z", "date-time")]
    #[case::date_time_with_offset("2023-01-15T10:30:00+05:30", "date-time")]
    #[case::date_time_with_milliseconds("2023-01-15T10:30:00.123Z", "date-time")]
    #[case::date_simple("2023-01-15", "date")]
    #[case::date_leap_year("2024-02-29", "date")]
    #[case::email_simple("user@example.com", "email")]
    #[case::email_with_plus_and_subdomain("a+b@sub.domain.org", "email")]
    #[case::ipv4_typical("192.168.1.1", "ipv4")]
    #[case::ipv4_all_zeros("0.0.0.0", "ipv4")]
    #[case::ipv4_all_255("255.255.255.255", "ipv4")]
    #[case::ipv6_full("2001:0db8:85a3:0000:0000:8a2e:0370:7334", "ipv6")]
    #[case::ipv6_loopback("::1", "ipv6")]
    #[case::ipv6_link_local("fe80::1", "ipv6")]
    #[case::hostname_with_dots("example.com", "hostname")]
    #[case::hostname_subdomain("sub.example.com", "hostname")]
    #[case::hostname_single_label("localhost", "hostname")]
    #[case::uri_https("https://example.com/path", "uri")]
    #[case::uri_http("http://example.com", "uri")]
    #[case::uri_urn("urn:isbn:0451450523", "uri")]
    #[case::uuid_lowercase("550e8400-e29b-41d4-a716-446655440000", "uuid")]
    #[case::uuid_uppercase("550E8400-E29B-41D4-A716-446655440000", "uuid")]
    #[case::json_pointer_empty("", "json-pointer")]
    #[case::json_pointer_simple_path("/foo/bar", "json-pointer")]
    #[case::json_pointer_with_index("/foo/0", "json-pointer")]
    #[case::json_pointer_tilde0_escape("/a~0b", "json-pointer")]
    #[case::json_pointer_tilde1_escape("/a~1b", "json-pointer")]
    #[case::regex_anchored("^[a-z]+$", "regex")]
    #[case::regex_wildcard(".*", "regex")]
    #[case::idn_hostname_ascii("example.com", "idn-hostname")]
    #[case::idn_hostname_punycode("xn--nxasmq6b.com", "idn-hostname")]
    #[case::idn_hostname_subdomain("sub.example.org", "idn-hostname")]
    #[case::idn_email_ascii("user@example.com", "idn-email")]
    #[case::idn_email_punycode_domain("user@xn--nxasmq6b.com", "idn-email")]
    #[case::iri_https("https://example.com/path", "iri")]
    #[case::iri_http("http://example.com", "iri")]
    #[case::iri_urn("urn:isbn:0451450523", "iri")]
    #[case::iri_reference_absolute("https://example.com/path", "iri-reference")]
    #[case::iri_reference_root_relative("/relative/path", "iri-reference")]
    #[case::iri_reference_relative("relative/path", "iri-reference")]
    #[case::unknown_format_silently_ignored("anything", "some-unknown-format")]
    fn format_valid_produces_no_diagnostics(#[case] value: &str, #[case] fmt: &str) {
        assert!(run_format(value, fmt).is_empty());
    }

    // Tests 158, 160, 162, 164, 166, 168, 170, 172, 174, 178, 180, 182, 184, 186:
    // invalid format value → schemaFormat WARNING with format name in message
    #[rstest]
    #[case::date_time_not_a_date("not-a-date", "date-time")]
    #[case::date_time_invalid_month("2023-13-01T00:00:00Z", "date-time")]
    #[case::date_time_space_separator("2023-01-15 10:30:00Z", "date-time")]
    #[case::date_invalid_month("2023-13-01", "date")]
    #[case::date_non_leap_year_feb29("2023-02-29", "date")]
    #[case::date_not_a_date("not-a-date", "date")]
    #[case::email_no_at_sign("no-at-sign", "email")]
    #[case::email_missing_domain_dot("missing-domain-dot@nodot", "email")]
    #[case::email_no_domain("user-no-domain@", "email")]
    #[case::ipv4_octet_too_large("256.0.0.1", "ipv4")]
    #[case::ipv4_too_few_octets("192.168.1", "ipv4")]
    #[case::ipv4_too_many_octets("192.168.1.1.1", "ipv4")]
    #[case::ipv4_leading_zero("01.0.0.1", "ipv4")]
    #[case::ipv6_too_many_groups("not::an::ipv6::address::with::too::many::groups::here", "ipv6")]
    #[case::hostname_leading_hyphen("-invalid.com", "hostname")]
    #[case::hostname_trailing_hyphen_label("invalid-.com", "hostname")]
    #[case::hostname_double_dot("invalid..com", "hostname")]
    #[case::uri_no_scheme("not-a-uri", "uri")]
    #[case::uri_relative_reference("//no-scheme", "uri")]
    #[case::uuid_not_uuid("not-a-uuid", "uuid")]
    #[case::uuid_invalid_char("550e8400-e29b-41d4-a716-44665544000g", "uuid")]
    #[case::uuid_no_hyphens("550e8400e29b41d4a716446655440000", "uuid")]
    #[case::json_pointer_no_leading_slash("foo", "json-pointer")]
    #[case::json_pointer_invalid_escape("/foo~2bar", "json-pointer")]
    #[case::json_pointer_trailing_tilde("/foo~", "json-pointer")]
    #[case::regex_unclosed_paren("(unclosed-paren", "regex")]
    #[case::idn_hostname_with_space("not a hostname", "idn-hostname")]
    #[case::idn_hostname_leading_hyphen("-bad-start.com", "idn-hostname")]
    #[case::idn_email_no_at_sign("no-at-sign", "idn-email")]
    #[case::idn_email_bad_domain("user@-bad-domain.com", "idn-email")]
    #[case::iri_with_space("not an iri", "iri")]
    #[case::iri_missing_scheme("://missing-scheme", "iri")]
    #[case::iri_reference_with_space("not valid iri ref", "iri-reference")]
    fn format_invalid_produces_schemaformat_warning(#[case] value: &str, #[case] fmt: &str) {
        let result = run_format(value, fmt);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaFormat");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(result[0].message.contains(fmt));
    }

    // Test 176 — format_validation disabled: no diagnostics emitted
    // Uses validate_schema(..., false) directly — tests the feature flag, not a format value.
    #[test]
    fn format_validation_disabled_produces_no_format_diagnostics() {
        let schema = format_schema("date");
        let docs = parse_docs("not-a-date");
        let result = validate_schema("not-a-date", &docs, &schema, false);
        assert!(result.is_empty());
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

    // Tests 187, 189, 191, 193: valid contentEncoding → no diagnostics
    #[rstest]
    #[case::base64_valid("aGVsbG8=", "base64")]
    #[case::base64_empty_valid("", "base64")]
    #[case::base64url_valid("aGVsbG8=", "base64url")]
    #[case::base32_valid("NBSWY3DPEB3W64TMMQ======", "base32")]
    #[case::base16_uppercase_valid("48656C6C6F", "base16")]
    #[case::base16_lowercase_valid("48656c6c6f", "base16")]
    fn content_encoding_valid_produces_no_diagnostics(#[case] value: &str, #[case] encoding: &str) {
        assert!(run_content(value, Some(encoding), None).is_empty());
    }

    // Tests 188, 190, 192, 194: invalid contentEncoding → one diagnostic
    #[rstest]
    #[case::base64_invalid("not-valid-base64!!!", "base64")]
    #[case::base64url_invalid("not+valid/base64url!!!", "base64url")]
    #[case::base32_invalid("not-valid-base32!!!", "base32")]
    #[case::base16_invalid("ZZZZ", "base16")]
    fn content_encoding_invalid_produces_error(#[case] value: &str, #[case] encoding: &str) {
        assert_eq!(run_content(value, Some(encoding), None).len(), 1);
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
    // contentSchema
    // ══════════════════════════════════════════════════════════════════════════

    fn content_schema_with_sub(
        encoding: Option<&str>,
        media_type: Option<&str>,
        sub_schema: JsonSchema,
    ) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            content_encoding: encoding.map(str::to_string),
            content_media_type: media_type.map(str::to_string),
            content_schema: Some(Box::new(sub_schema)),
            ..JsonSchema::default()
        }
    }

    // contentSchema with base64-encoded JSON that matches the sub-schema
    #[test]
    fn content_schema_base64_json_valid() {
        // base64("42") = "NDI="
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let docs = parse_docs("\"NDI=\"");
        assert!(
            validate_schema("\"NDI=\"", &docs, &schema, true).is_empty(),
            "valid base64-encoded integer should pass contentSchema validation"
        );
    }

    // contentSchema with base64-encoded JSON that fails the sub-schema
    #[test]
    fn content_schema_base64_json_type_mismatch() {
        // base64("\"hello\"") = "ImhlbGxvIg=="
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let docs = parse_docs("\"ImhlbGxvIg==\"");
        let result = validate_schema("\"ImhlbGxvIg==\"", &docs, &schema, true);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaType"),
            "string decoded where integer expected should produce schemaType error: {result:?}"
        );
    }

    // contentSchema without encoding — validate raw string as YAML
    #[test]
    fn content_schema_no_encoding_validates_raw_string() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        // The raw YAML value "42" (as a quoted string) should be parsed as YAML integer
        let docs = parse_docs("\"42\"");
        assert!(
            validate_schema("\"42\"", &docs, &schema, true).is_empty(),
            "raw string '42' should validate as integer against contentSchema"
        );
    }

    // contentSchema without encoding — validation failure
    #[test]
    fn content_schema_no_encoding_type_mismatch() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        let docs = parse_docs("\"hello\"");
        let result = validate_schema("\"hello\"", &docs, &schema, true);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaType"),
            "string 'hello' should fail integer contentSchema: {result:?}"
        );
    }

    // contentSchema with encoding failure — contentSchema not checked
    #[test]
    fn content_schema_skipped_when_encoding_fails() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let docs = parse_docs("\"not-valid-base64!!!\"");
        let result = validate_schema("\"not-valid-base64!!!\"", &docs, &schema, true);
        // Should get encoding error but NOT contentSchema type error
        assert!(
            result.iter().any(|d| code_of(d) == "schemaContentEncoding"),
            "should report encoding error: {result:?}"
        );
        assert!(
            !result.iter().any(|d| code_of(d) == "schemaType"),
            "should NOT check contentSchema when encoding fails: {result:?}"
        );
    }

    // contentSchema with media type failure — contentSchema not checked
    #[test]
    fn content_schema_skipped_when_media_type_fails() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, Some("application/json"), sub);
        let docs = parse_docs("\"not json at all\"");
        let result = validate_schema("\"not json at all\"", &docs, &schema, true);
        assert!(
            result
                .iter()
                .any(|d| code_of(d) == "schemaContentMediaType"),
            "should report media type error: {result:?}"
        );
        assert!(
            !result.iter().any(|d| code_of(d) == "schemaType"),
            "should NOT check contentSchema when media type fails: {result:?}"
        );
    }

    // TE test 5: contentSchema validates embedded YAML mapping via base64
    // (using base64 encoding to avoid the known parser limitation with
    // colon-space inside double-quoted strings)
    #[test]
    fn content_schema_validates_embedded_yaml_mapping() {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            },
        );
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        // base64("name: alice\n") = "bmFtZTogYWxpY2UK"
        let text = "\"bmFtZTogYWxpY2UK\"";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(
            result.is_empty(),
            "embedded YAML mapping should validate: {result:?}"
        );
    }

    // TE test 6: contentSchema embedded mapping with type mismatch via base64
    #[test]
    fn content_schema_validates_embedded_yaml_mapping_invalid() {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            },
        );
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        // base64("name: 42\n") = "bmFtZTogNDIK"
        let text = "\"bmFtZTogNDIK\"";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(
            !result.is_empty(),
            "embedded mapping with integer name should fail string check: {result:?}"
        );
    }

    // TE test 7: contentSchema skipped when format_validation is off
    #[test]
    fn content_schema_skipped_when_format_validation_off() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        let docs = parse_docs("\"hello\"");
        // format_validation = false → content keywords not checked
        let result = validate_schema("\"hello\"", &docs, &schema, false);
        assert!(
            result.is_empty(),
            "contentSchema should not be checked when format_validation is off: {result:?}"
        );
    }

    // TE test 11: all three checks pass (encoding + media type + contentSchema)
    #[test]
    fn content_schema_with_encoding_and_media_type_all_pass() {
        // base64({"key": "value"}) = eyJrZXkiOiAidmFsdWUifQ==
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), Some("application/json"), sub);
        let text = "\"eyJrZXkiOiAidmFsdWUifQ==\"";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(
            result.is_empty(),
            "all three checks should pass: {result:?}"
        );
    }

    // TE test 13: valid base64 but decoded YAML is unparseable
    #[test]
    fn content_schema_decoded_yaml_invalid() {
        // base64(": bad: [") = OiBiYWQ6IFs=
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        let text = "\"OiBiYWQ6IFs=\"";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(
            result.iter().any(|d| code_of(d) == "schemaContentSchema"),
            "unparseable decoded YAML should produce schemaContentSchema: {result:?}"
        );
    }

    // TE test 14: empty content with contentSchema
    #[test]
    fn content_schema_with_empty_decoded_content() {
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(None, None, sub);
        let text = "\"\"";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        // load("") returns 0 documents — nothing to validate, no diagnostics.
        assert!(
            result.is_empty(),
            "empty content should produce no diagnostics: {result:?}"
        );
    }

    // TE test 15: nested sub-schema runs full validation (via base64
    // to avoid known parser limitation with colon-space in double-quoted strings)
    #[test]
    fn content_schema_nested_sub_schema_uses_full_validation() {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "value".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            },
        );
        let sub = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props),
            ..JsonSchema::default()
        };
        let schema = content_schema_with_sub(Some("base64"), None, sub);
        // base64("value: 42\n") = "dmFsdWU6IDQyCg=="
        // Embedded YAML: value is 42 (integer), but schema expects string
        let text = "\"dmFsdWU6IDQyCg==\"";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert!(
            !result.is_empty(),
            "nested schema should catch type mismatch: {result:?}"
        );
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

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — type mismatch format
    // ══════════════════════════════════════════════════════════════════════════

    // Test 215
    #[test]
    fn type_mismatch_message_uses_value_at_path_subject() {
        let schema = object_schema_with_props(vec![("replicas", integer_schema())]);
        let text = "replicas: \"hello\"";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.starts_with("Value at"),
            "message should start with 'Value at', got: {msg}"
        );
        assert!(
            msg.contains("does not match type"),
            "message should contain 'does not match type', got: {msg}"
        );
        assert!(
            msg.contains("integer"),
            "message should contain expected type 'integer', got: {msg}"
        );
        assert!(
            msg.contains("string"),
            "message should contain actual type 'string', got: {msg}"
        );
    }

    // Test 216
    #[test]
    fn type_mismatch_message_includes_property_path() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some([("replicas".to_string(), integer_schema())].into()),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  replicas: not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("spec.replicas"),
            "message should contain nested path 'spec.replicas', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — numeric min/max unified format
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::inclusive_minimum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            minimum: Some(5.0),
            exclusive_minimum_draft04: Some(false),
            ..JsonSchema::default()
        })]),
        "val: 4",
        "is below minimum 5",
        "(inclusive)"
    )]
    #[case::exclusive_minimum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            minimum: Some(5.0),
            exclusive_minimum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "is below exclusive minimum 5",
        "(exclusive)"
    )]
    #[case::inclusive_maximum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            maximum: Some(10.0),
            exclusive_maximum_draft04: Some(false),
            ..JsonSchema::default()
        })]),
        "val: 11",
        "is above maximum 10",
        "(inclusive)"
    )]
    #[case::exclusive_maximum_draft04(
        object_schema_with_props(vec![("val", JsonSchema {
            maximum: Some(10.0),
            exclusive_maximum_draft04: Some(true),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "is above exclusive maximum 10",
        "(exclusive)"
    )]
    #[case::exclusive_minimum_draft06(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_minimum: Some(5.0),
            ..JsonSchema::default()
        })]),
        "val: 5",
        "is below exclusive minimum 5",
        "must be greater than"
    )]
    #[case::exclusive_maximum_draft06(
        object_schema_with_props(vec![("val", JsonSchema {
            exclusive_maximum: Some(10.0),
            ..JsonSchema::default()
        })]),
        "val: 10",
        "is above exclusive maximum 10",
        "must be less than"
    )]
    fn numeric_bound_message_uses_correct_phrase(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] expected_phrase: &str,
        #[case] excluded_phrase: &str,
    ) {
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains(expected_phrase),
            "message should contain '{expected_phrase}', got: {msg}"
        );
        assert!(
            !msg.contains(excluded_phrase),
            "message should not contain '{excluded_phrase}', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — anyOf branch count
    // ══════════════════════════════════════════════════════════════════════════

    // Test 223
    #[test]
    fn any_of_message_includes_branch_count() {
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
        let any_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &any_of_diag.message;
        assert!(
            msg.contains('2'),
            "message should contain branch count '2', got: {msg}"
        );
        assert!(
            msg.contains("(anyOf)"),
            "message should contain '(anyOf)', got: {msg}"
        );
    }

    // Test 224
    #[test]
    fn any_of_message_branch_count_capped_at_max_branch_count() {
        // 25 branches (> MAX_BRANCH_COUNT=20) — message should show 20
        let branches: Vec<JsonSchema> = (0..25)
            .map(|i| JsonSchema {
                required: Some(vec![format!("field_{i}")]),
                ..JsonSchema::default()
            })
            .collect();
        let schema = JsonSchema {
            any_of: Some(branches),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema("other: value", &docs, &schema, true);
        assert!(!result.is_empty());
        let any_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &any_of_diag.message;
        assert!(
            msg.contains("20"),
            "message should contain capped count '20', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — oneOf branch count
    // ══════════════════════════════════════════════════════════════════════════

    // Test 225
    #[test]
    fn one_of_zero_match_message_includes_branch_count() {
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
        let one_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &one_of_diag.message;
        assert!(
            msg.contains('2'),
            "message should contain branch count '2', got: {msg}"
        );
        assert!(
            msg.contains("oneOf schemas"),
            "message should contain 'oneOf schemas', got: {msg}"
        );
    }

    // Test 226
    #[test]
    fn one_of_multi_match_message_includes_passing_count() {
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
        // "a: hello" matches both branches
        let docs = parse_docs("a: hello");
        let result = validate_schema("a: hello", &docs, &schema, true);
        assert!(!result.is_empty());
        let one_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &one_of_diag.message;
        assert!(
            msg.contains("expected exactly 1"),
            "message should contain 'expected exactly 1', got: {msg}"
        );
    }

    // Test 227
    #[test]
    fn one_of_multi_match_message_includes_total_count() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    required: Some(vec!["a".to_string()]),
                    ..JsonSchema::default()
                },
                object_schema_with_props(vec![("a", string_schema())]),
                JsonSchema {
                    required: Some(vec!["b".to_string()]),
                    ..JsonSchema::default()
                },
            ]),
            ..JsonSchema::default()
        };
        // "a: hello" matches the first two branches (2 of 3)
        let docs = parse_docs("a: hello");
        let result = validate_schema("a: hello", &docs, &schema, true);
        assert!(!result.is_empty());
        let one_of_diag = result
            .iter()
            .find(|d| code_of(d) == "schemaType")
            .expect("should have a schemaType diagnostic");
        let msg = &one_of_diag.message;
        assert!(
            msg.contains('3'),
            "message should contain total count '3', got: {msg}"
        );
        assert!(
            msg.contains('2'),
            "message should contain passing count '2', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — not schema wording
    // ══════════════════════════════════════════════════════════════════════════

    // Test 228
    #[test]
    fn not_schema_message_references_not_keyword() {
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
        let msg = &result[0].message;
        assert!(
            msg.contains("schema defined in 'not'"),
            "message should contain \"schema defined in 'not'\", got: {msg}"
        );
        assert!(
            !msg.contains("excluded schema"),
            "message should not contain old phrasing 'excluded schema', got: {msg}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Message consistency — required property format
    // ══════════════════════════════════════════════════════════════════════════

    // Test 229
    #[test]
    fn required_property_message_uses_object_at_subject() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("age: 30");
        let result = validate_schema("age: 30", &docs, &schema, true);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("Object at"),
            "message should contain 'Object at', got: {msg}"
        );
        assert!(
            msg.contains("is missing required property"),
            "message should contain 'is missing required property', got: {msg}"
        );
        assert!(
            !msg.contains("Missing required property"),
            "message should not use old phrasing 'Missing required property', got: {msg}"
        );
    }

    // Test 230
    #[test]
    fn required_property_message_uses_expected_label() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string(), "age".to_string()]),
            ..JsonSchema::default()
        };
        let docs = parse_docs("other: value");
        let result = validate_schema("other: value", &docs, &schema, true);
        assert!(!result.is_empty());
        let msg = &result[0].message;
        assert!(
            msg.contains("Expected:"),
            "message should contain 'Expected:', got: {msg}"
        );
        assert!(
            !msg.contains("Expected properties:"),
            "message should not contain old label 'Expected properties:', got: {msg}"
        );
    }

    // Test 231
    #[test]
    fn required_property_message_includes_nested_path() {
        let spec_schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            required: Some(vec!["replicas".to_string()]),
            ..JsonSchema::default()
        };
        let schema = object_schema_with_props(vec![("spec", spec_schema)]);
        let text = "spec:\n  other: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema, true);
        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("Object at spec"),
            "message should contain 'Object at spec', got: {msg}"
        );
        assert!(
            msg.contains("replicas"),
            "message should contain the missing property name 'replicas', got: {msg}"
        );
    }
}
