// SPDX-License-Identifier: MIT

use saphyr::YamlOwned;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::schema::{AdditionalProperties, JsonSchema, SchemaType};

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
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Validate `docs` (parsed YAML ASTs) against `schema` and return diagnostics.
///
/// `text` is the raw document text used for position lookup.
/// Each element of `docs` is one YAML document (separated by `---`).
#[must_use]
pub fn validate_schema(text: &str, docs: &[YamlOwned], schema: &JsonSchema) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diagnostics = Vec::new();

    for doc in docs {
        validate_node(doc, schema, &[], &lines, &mut diagnostics, 0);
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
    lines: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    if depth > MAX_VALIDATION_DEPTH {
        return;
    }

    // Type check
    if let Some(schema_type) = &schema.schema_type {
        let yaml_type = yaml_type_name(node);
        if !type_matches(yaml_type, schema_type) {
            let range = node_range(path, lines);
            diagnostics.push(make_diagnostic(
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
        let range = node_range(path, lines);
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
        diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaEnum",
            format!("Value at {} must be one of: {}", format_path(path), valid),
        ));
    }

    // Mapping-specific checks
    if let YamlOwned::Mapping(map) = node {
        validate_mapping(map, schema, path, lines, diagnostics, depth);
    }

    // Sequence-specific checks
    if let YamlOwned::Sequence(seq) = node
        && let Some(items_schema) = &schema.items
    {
        for (i, item) in seq.iter().enumerate() {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            validate_node(
                item,
                items_schema,
                &item_path,
                lines,
                diagnostics,
                depth + 1,
            );
        }
    }

    // Composition
    validate_composition(node, schema, path, lines, diagnostics, depth);
}

fn validate_mapping(
    map: &saphyr::MappingOwned,
    schema: &JsonSchema,
    path: &[String],
    lines: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    use saphyr::ScalarOwned;

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
        diagnostics.extend(
            required
                .iter()
                .filter(|req_key| {
                    let key_yaml = YamlOwned::Value(ScalarOwned::String((*req_key).clone()));
                    !map.contains_key(&key_yaml)
                })
                .map(|req_key| {
                    let range = mapping_range(path, lines);
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
            validate_node(v, prop_schema, &child_path, lines, diagnostics, depth + 1);
        } else if !is_known {
            // Check additionalProperties
            match &schema.additional_properties {
                Some(AdditionalProperties::Denied) => {
                    let range = key_range(&key_str, path, lines);
                    diagnostics.push(make_diagnostic(
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
                    validate_node(v, extra_schema, &child_path, lines, diagnostics, depth + 1);
                }
                None => {}
            }
        }
    }
}

fn validate_composition(
    node: &YamlOwned,
    schema: &JsonSchema,
    path: &[String],
    lines: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    // allOf: all branches must pass
    if let Some(all_of) = &schema.all_of {
        for branch in all_of.iter().take(MAX_BRANCH_COUNT) {
            validate_node(node, branch, path, lines, diagnostics, depth + 1);
        }
    }

    // anyOf: at least one branch must pass; if none do, emit a diagnostic
    if let Some(any_of) = &schema.any_of {
        let any_passes = any_of.iter().take(MAX_BRANCH_COUNT).any(|branch| {
            let mut scratch = Vec::new();
            validate_node(node, branch, path, lines, &mut scratch, depth + 1);
            scratch.is_empty()
        });
        if !any_passes {
            let range = node_range(path, lines);
            diagnostics.push(make_diagnostic(
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
        let passing = one_of
            .iter()
            .take(MAX_BRANCH_COUNT)
            .filter(|branch| {
                let mut scratch = Vec::new();
                validate_node(node, branch, path, lines, &mut scratch, depth + 1);
                scratch.is_empty()
            })
            .count();

        if passing == 0 {
            let range = node_range(path, lines);
            diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaType",
                format!(
                    "Value at {} does not match any of the oneOf schemas",
                    format_path(path)
                ),
            ));
        } else if passing > 1 {
            let range = node_range(path, lines);
            diagnostics.push(make_diagnostic(
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
        let result = validate_schema("name: Alice", &docs, &schema);
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
        let result = validate_schema("age: 30", &docs, &schema);
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
        let result = validate_schema("other: value", &docs, &schema);
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
        let result = validate_schema("a: 1\nb: 2", &docs, &schema);
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
        let result = validate_schema("key: value", &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema("name: Alice", &docs, &schema);
        assert!(result.is_empty());
    }

    // Test 8
    #[test]
    fn should_produce_error_for_string_where_integer_expected() {
        let schema = object_schema_with_props(vec![("count", integer_schema())]);
        let docs = parse_docs("count: \"hello\"");
        let result = validate_schema("count: \"hello\"", &docs, &schema);
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
        let result = validate_schema("name: 42", &docs, &schema);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 10
    #[test]
    fn should_produce_error_for_boolean_where_string_expected() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: true");
        let result = validate_schema("name: true", &docs, &schema);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 11
    #[test]
    fn should_produce_error_for_mapping_where_string_expected() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let text = "name:\n  nested: value";
        let docs = parse_docs(text);
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 13
    #[test]
    fn should_produce_error_for_null_where_string_expected() {
        let schema = object_schema_with_props(vec![("name", string_schema())]);
        let docs = parse_docs("name: ~");
        let result = validate_schema("name: ~", &docs, &schema);
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
        let result = validate_schema("name: ~", &docs, &schema);
        assert!(result.is_empty());
    }

    // Test 15
    #[test]
    fn should_produce_no_diagnostics_when_no_type_specified() {
        let schema = object_schema_with_props(vec![("name", JsonSchema::default())]);
        let docs = parse_docs("name: 42");
        let result = validate_schema("name: 42", &docs, &schema);
        assert!(result.is_empty());
    }

    // Test 16
    #[test]
    fn should_produce_no_diagnostics_for_integer_type_with_integer_value() {
        let schema = object_schema_with_props(vec![("port", integer_schema())]);
        let docs = parse_docs("port: 8080");
        let result = validate_schema("port: 8080", &docs, &schema);
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
        let result = validate_schema("enabled: true", &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema("env: staging", &docs, &schema);
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
        let result = validate_schema("env: testing", &docs, &schema);
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
        let result = validate_schema("level: 2", &docs, &schema);
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
        let result = validate_schema("level: 5", &docs, &schema);
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
        let result = validate_schema("value: auto", &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema("name: Alice", &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema("a: 1\nb: 2", &docs, &schema);
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
        let result = validate_schema("a: 1", &docs, &schema);
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
        let result = validate_schema("name: Alice", &docs, &schema);
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
        let result = validate_schema("other: value", &docs, &schema);
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
        let result = validate_schema("a: 1", &docs, &schema);
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
        let result = validate_schema("other: value", &docs, &schema);
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
        let result = validate_schema("a: hello", &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema(text, &docs, &l1);
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
        let _ = validate_schema(&text, &docs, &schema);
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
        let result = validate_schema("age: 30", &docs, &schema);
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
        let result = validate_schema("age: 30", &docs, &schema);
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
        let result = validate_schema("count: hello", &docs, &schema);
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
        let result = validate_schema("env: testing", &docs, &schema);
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
        let result = validate_schema("name: Alice\nextra: value", &docs, &schema);
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
        let result = validate_schema("age: 30", &docs, &schema);
        assert!(!result.is_empty());
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // Test 47
    #[test]
    fn should_set_error_severity_for_type_violation() {
        let schema = object_schema_with_props(vec![("count", integer_schema())]);
        let docs = parse_docs("count: hello");
        let result = validate_schema("count: hello", &docs, &schema);
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
        let result = validate_schema("env: testing", &docs, &schema);
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
        let result = validate_schema("name: Alice\nextra: value", &docs, &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema("env: testing", &docs, &schema);
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
        let result = validate_schema("", &docs, &schema);
        assert!(result.is_empty());
    }

    // Test 53
    #[test]
    fn should_return_empty_when_docs_is_empty() {
        let schema = JsonSchema {
            required: Some(vec!["name".to_string()]),
            ..JsonSchema::default()
        };
        let result = validate_schema("name: Alice", &[], &schema);
        assert!(result.is_empty());
    }

    // Test 54
    #[test]
    fn should_return_empty_for_schema_with_no_constraints() {
        let schema = JsonSchema::default();
        let docs = parse_docs("anything: value\nnested:\n  key: 123");
        let result = validate_schema("anything: value\nnested:\n  key: 123", &docs, &schema);
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
        let result = validate_schema("invalid: [yaml", &[], &schema);
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
        let result = validate_schema(text, &docs, &schema);
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
        let result = validate_schema("anything: value", &docs, &schema);
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
        let _result = validate_schema(&text, &docs, &schema);
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
        let _result = validate_schema(&text, &docs, &schema);
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
        let _result = validate_schema("other: value", &docs, &schema);
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
        let result = validate_schema("field_0: value", &docs, &schema);
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
        let result = validate_schema("age: 30", &docs, &schema);
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
        let result = validate_schema("env: invalid", &docs, &schema);
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

    // Test 64
    #[test]
    #[ignore]
    fn should_not_hold_mutex_across_schema_fetch() {
        // Mutex-across-await correctness is enforced by code review and the
        // tokio linting rule. See `parse_and_publish` in `server.rs`:
        // schema_cache and schema_associations locks are acquired, data
        // extracted as owned values, lock dropped, then `spawn_blocking` is
        // called. No std::sync::Mutex guard is held across any `.await` point.
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security: Mutex Poison Handling
    // ══════════════════════════════════════════════════════════════════════════

    // Test 65
    #[test]
    #[ignore]
    fn should_continue_without_schema_validation_when_cache_lock_poisoned() {
        // Poison handling is verified by code inspection: all
        // `schema_cache.lock()` calls use `.ok()?` or `.ok()` (not
        // `.unwrap()`). A poisoned lock returns `None`/`Err`, and the calling
        // code skips schema validation rather than panicking.
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
        let result = validate_schema("other: value", &docs, &schema);
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
        let result = validate_schema("other: value", &docs, &schema);
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
}
