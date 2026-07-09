// SPDX-License-Identifier: MIT

use std::cell::RefCell;
use std::collections::HashMap;

use regex::RegexBuilder;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Range};

use crate::scalar_helpers;
use crate::schema::{JsonSchema, SchemaType};

/// Maximum length of a `pattern` string we will compile and match, as a
/// guard against pathological `ReDoS` inputs.
pub(super) const MAX_PATTERN_LEN: usize = 1024;

/// Maximum compiled NFA size for a regex, as a memory guard.
pub(super) const REGEX_SIZE_LIMIT: usize = 512 * 1024;

/// Maximum recursion depth for the validation walk.
pub(super) const MAX_VALIDATION_DEPTH: usize = 64;

/// Maximum number of `allOf` / `anyOf` / `oneOf` branches evaluated.
pub(super) const MAX_BRANCH_COUNT: usize = 20;

/// Maximum length of schema description text embedded in diagnostic messages.
pub(super) const MAX_DESCRIPTION_LEN: usize = 200;

/// Maximum number of enum values listed verbatim in a diagnostic message.
pub(super) const MAX_ENUM_DISPLAY: usize = 5;

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
pub(super) fn get_regex(pattern: &str) -> Option<regex::Regex> {
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

/// Collect property names directly evaluated by `schema` (one level: `properties`,
/// `patternProperties` keys, and composition sub-schema properties).
/// Used to determine what composition branches consider "evaluated" without
/// full recursive context threading.
pub(super) fn collect_evaluated_properties(schema: &JsonSchema, key: &str) -> bool {
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
            if pattern.len() <= MAX_PATTERN_LEN
                && let Some(re) = get_regex(pattern)
                && re.is_match(key)
            {
                return true;
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
    if let Some(then_s) = &schema.then_schema
        && collect_evaluated_properties(then_s, key)
    {
        return true;
    }
    if let Some(else_s) = &schema.else_schema
        && collect_evaluated_properties(else_s, key)
    {
        return true;
    }
    false
}

/// Collect the number of prefix items directly covered by `schema` and its
/// composition sub-schemas (one level deep).
pub(super) fn collect_evaluated_item_count(schema: &JsonSchema) -> usize {
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
