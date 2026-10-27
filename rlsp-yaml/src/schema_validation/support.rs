// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Range};

use crate::scalar_helpers;
use crate::schema::pattern::{MAX_PATTERN_LEN, compile_priced, lookup_or_compile};
use crate::schema::{JsonSchema, ParsedSchema, SchemaType};

use super::context::{Budget, work_steps};

/// Maximum recursion depth for the validation walk.
pub(super) const MAX_VALIDATION_DEPTH: usize = 64;

/// Maximum nesting of embedded documents (`contentSchema`). Each level restarts
/// the validation depth at zero, so a schema that applies itself through
/// `contentSchema` is bounded by this alone. Reaching it ends the run early.
pub(super) const MAX_CONTENT_DEPTH: usize = 8;

/// Maximum length of schema description text embedded in diagnostic messages.
pub(super) const MAX_DESCRIPTION_LEN: usize = 200;

/// Maximum number of enum values listed verbatim in a diagnostic message.
pub(super) const MAX_ENUM_DISPLAY: usize = 5;

/// Why a pattern could not be used.
pub(super) enum PatternUnavailable {
    /// It exceeds `REGEX_SIZE_LIMIT` or is not a valid regex.
    Invalid,
    /// The budget could not pay for it.
    OverBudget,
}

/// Return a compiled `Regex` for the schema `pattern`.
///
/// A use costs one step plus the matching of the `haystack_len` bytes it will
/// be matched against, priced per tier (see `COMPILE_TIERS`). The
/// first use in a run also pays for compiling the
/// pattern (see `COMPILE_TIERS`), at a price that depends only on the pattern —
/// the same whether the thread-local cache already holds it or not — so the
/// steps a run takes depend only on the schema and the documents.
pub(super) fn compile_pattern(
    pattern: &str,
    haystack_len: usize,
    budget: &Budget,
) -> Result<regex::Regex, PatternUnavailable> {
    let paid = budget.has_paid_for_pattern(pattern);
    let compiled = lookup_or_compile(pattern, |steps, cache_hit| {
        (cache_hit && paid) || budget.spend(steps)
    })
    .ok_or(PatternUnavailable::OverBudget)?;
    budget.note_paid_for_pattern(pattern);
    // An invalid pattern is never matched, but a use still costs a step: a schema
    // can list many of them.
    let matching = if compiled.regex.is_some() {
        haystack_len.saturating_mul(compiled.matching_work_per_byte)
    } else {
        0
    };
    if !budget.spend(work_steps(matching).saturating_add(1)) {
        return Err(PatternUnavailable::OverBudget);
    }
    compiled.regex.ok_or(PatternUnavailable::Invalid)
}

/// Whether the document value `value` is a valid regex, for `format: regex`.
/// Returns `None` when the budget cannot pay for the check.
///
/// Each distinct value is compiled once per run, but every check of it pays the
/// price of that compilation, so the steps do not depend on how often the
/// value repeats. Values are not cached across runs: they come from documents.
pub(super) fn is_valid_regex_value(value: &str, budget: &Budget) -> Option<bool> {
    if value.len() > MAX_PATTERN_LEN {
        return Some(false);
    }
    if let Some((valid, steps)) = budget.recall_regex_value(value) {
        return budget.spend(steps).then_some(valid);
    }
    // Pay attempt by attempt before compiling, then keep the outcome.
    let compiled = compile_priced(value, |steps| budget.spend(steps))?;
    let valid = compiled.regex.is_some();
    budget.remember_regex_value(value, (valid, compiled.steps));
    Some(valid)
}

/// Extract the `loc` span from any AST node.
pub(super) const fn node_loc(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

/// Helper: check if a mapping's entries contain a key with the given string value.
pub(super) fn entries_contains_key(entries: &[(Node<Span>, Node<Span>)], key: &str) -> bool {
    entries
        .iter()
        .any(|(k, _)| matches!(k, Node::Scalar { value, .. } if value == key))
}

/// Helper: extract a string key from a node.
pub(super) fn node_key_str(node: &Node<Span>) -> Option<String> {
    match node {
        Node::Scalar { value, .. } => Some(value.clone()),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
    }
}

/// The sub-schemas applied to the same instance as `schema`: every `allOf`,
/// `anyOf`, and `oneOf` branch, then `then` and `else`.
fn applicator_children(schema: &JsonSchema) -> impl Iterator<Item = &JsonSchema> {
    schema
        .all_of
        .iter()
        .chain(&schema.any_of)
        .chain(&schema.one_of)
        .flatten()
        .chain(schema.then_schema.as_deref())
        .chain(schema.else_schema.as_deref())
}

/// Whether `schema` or any schema nested in its composition keywords evaluates
/// the property `key`: through `properties`, a matching `patternProperties`
/// pattern, or a sub-schema.
///
/// Used by `unevaluatedProperties` to learn what composition branches consider
/// evaluated. Visiting a schema node costs one `budget` step. Returns `None`
/// when the budget or `MAX_VALIDATION_DEPTH` cut the scan short and the answer
/// is unknown.
pub(super) fn collect_evaluated_properties(
    parsed: &ParsedSchema,
    schema: &JsonSchema,
    key: &str,
    budget: &Budget,
    depth: usize,
) -> Option<bool> {
    let schema = parsed.resolve(schema);
    if depth > MAX_VALIDATION_DEPTH || !budget.spend(1) {
        return None;
    }
    // Matched by properties
    if schema
        .properties
        .as_ref()
        .is_some_and(|p| p.contains_key(key))
    {
        return Some(true);
    }
    // Matched by patternProperties
    if let Some(pp) = &schema.pattern_properties {
        for (pattern, _) in pp {
            if pattern.len() > MAX_PATTERN_LEN {
                continue;
            }
            match compile_pattern(pattern, key.len(), budget) {
                Ok(re) if re.is_match(key) => return Some(true),
                Ok(_) | Err(PatternUnavailable::Invalid) => {}
                Err(PatternUnavailable::OverBudget) => return None,
            }
        }
    }
    for child in applicator_children(schema) {
        if collect_evaluated_properties(parsed, child, key, budget, depth + 1)? {
            return Some(true);
        }
    }
    Some(false)
}

/// Number of leading array items evaluated by `schema` and the `allOf`
/// branches nested in it: `prefixItems` covers its length, `items` covers
/// everything (`usize::MAX`).
///
/// Visiting a schema node costs one `budget` step. Returns `None` when the
/// budget or `MAX_VALIDATION_DEPTH` cut the scan short and the answer is
/// unknown.
pub(super) fn collect_evaluated_item_count(
    parsed: &ParsedSchema,
    schema: &JsonSchema,
    budget: &Budget,
    depth: usize,
) -> Option<usize> {
    let schema = parsed.resolve(schema);
    if depth > MAX_VALIDATION_DEPTH || !budget.spend(1) {
        return None;
    }
    // items (non-nil) covers all remaining indices — signal with usize::MAX
    if schema.items.is_some() {
        return Some(usize::MAX);
    }
    let mut count = schema.prefix_items.as_ref().map_or(0, Vec::len);
    for branch in schema.all_of.iter().flatten() {
        let branch_count = collect_evaluated_item_count(parsed, branch, budget, depth + 1)?;
        if branch_count == usize::MAX {
            return Some(usize::MAX);
        }
        count = count.max(branch_count);
    }
    Some(count)
}

pub(super) fn yaml_type_name(node: &Node<Span>) -> &'static str {
    match node {
        Node::Scalar { tag, .. } => match tag.as_deref() {
            Some("tag:yaml.org,2002:null") => "null",
            Some("tag:yaml.org,2002:bool") => "boolean",
            Some("tag:yaml.org,2002:int") => "integer",
            Some("tag:yaml.org,2002:float") => "number",
            _ => "string",
        },
        Node::Mapping { .. } => "object",
        Node::Sequence { .. } => "array",
        Node::Alias { .. } => "unknown",
    }
}

pub(super) fn type_matches(yaml_type: &str, schema_type: &SchemaType) -> bool {
    match schema_type {
        SchemaType::Single(t) => single_type_matches(yaml_type, t),
        SchemaType::Multiple(ts) => ts.iter().any(|t| single_type_matches(yaml_type, t)),
    }
}

/// Returns `true` if the schema type is or includes the given type name.
pub(super) fn single_type_or_contains(schema_type: &SchemaType, target: &str) -> bool {
    match schema_type {
        SchemaType::Single(t) => t == target,
        SchemaType::Multiple(ts) => ts.iter().any(|t| t == target),
    }
}

/// JSON Schema allows "number" to also accept integers.
pub(super) fn single_type_matches(yaml_type: &str, schema_type: &str) -> bool {
    if yaml_type == schema_type {
        return true;
    }
    // JSON Schema: "number" accepts both "number" and "integer"
    if schema_type == "number" && yaml_type == "integer" {
        return true;
    }
    false
}

pub(super) fn display_schema_type(schema_type: &SchemaType) -> String {
    match schema_type {
        SchemaType::Single(t) => t.clone(),
        SchemaType::Multiple(ts) => ts.join(" | "),
    }
}

pub(super) fn yaml_to_json(node: &Node<Span>) -> Option<serde_json::Value> {
    match node {
        Node::Scalar { value, tag, .. } => match tag.as_deref() {
            Some("tag:yaml.org,2002:null") => Some(serde_json::Value::Null),
            Some("tag:yaml.org,2002:bool") => Some(serde_json::Value::Bool(matches!(
                value.as_str(),
                "true" | "True" | "TRUE"
            ))),
            Some("tag:yaml.org,2002:int") => {
                scalar_helpers::parse_integer(value).map(|i| serde_json::Value::Number(i.into()))
            }
            Some("tag:yaml.org,2002:float") => scalar_helpers::parse_float(value)
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number),
            _ => Some(serde_json::Value::String(value.clone())),
        },
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
    }
}

pub(super) fn make_diagnostic(
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

pub(super) fn truncate_message(msg: String) -> String {
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

pub(super) fn format_path(path: &[String]) -> String {
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

#[cfg(test)]
pub(super) mod test_fixtures {
    use tower_lsp::lsp_types::NumberOrString;

    use crate::schema::{JsonSchema, SchemaType};

    pub(in super::super) fn string_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        }
    }

    pub(in super::super) fn integer_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        }
    }

    pub(in super::super) fn boolean_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            ..JsonSchema::default()
        }
    }

    pub(in super::super) fn object_schema_with_props(props: Vec<(&str, JsonSchema)>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            ..JsonSchema::default()
        }
    }

    pub(in super::super) fn code_of(d: &tower_lsp::lsp_types::Diagnostic) -> &str {
        match &d.code {
            Some(NumberOrString::String(s)) => s.as_str(),
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    #[test]
    fn should_continue_without_schema_validation_when_cache_lock_poisoned() {
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
}
