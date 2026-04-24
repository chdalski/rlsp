// SPDX-License-Identifier: MIT

use std::fmt::Write as _;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{Pos, Span};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::schema::JsonSchema;

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/// Maximum number of examples to display in hover output.
const MAX_EXAMPLES: usize = 3;

/// Maximum characters for a schema description before truncation.
const MAX_DESCRIPTION_LEN: usize = 200;

/// Maximum characters for an example value before truncation.
const MAX_EXAMPLE_LEN: usize = 100;

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Compute hover information for the AST documents at the given cursor position.
///
/// Returns `None` if the cursor falls outside all node spans (whitespace, comment,
/// document separator, empty line) or when `docs` is empty.
#[must_use]
pub fn hover_at(
    docs: &[Document<Span>],
    position: Position,
    schema: Option<&JsonSchema>,
) -> Option<Hover> {
    if docs.is_empty() {
        return None;
    }

    // LSP Position is 0-based line; parser Pos is 1-based line, 0-based column.
    let cursor = Pos {
        byte_offset: 0,
        line: position.line as usize + 1,
        column: position.character as usize,
    };

    let (path, node) = ast_walk(docs, cursor)?;

    let formatted_path = format_path(&path);
    let yaml_type = yaml_type_name(node);
    let value = scalar_value(node);

    let mut markdown = format_hover_markdown(&formatted_path, &yaml_type, value.as_deref());

    if let Some(s) = schema {
        let key_path = build_schema_key_path(&formatted_path);
        if let Some(prop_schema) = resolve_schema_path(s, &key_path) {
            let schema_section = format_schema_section(prop_schema);
            if !schema_section.is_empty() {
                markdown.push('\n');
                markdown.push_str(&schema_section);
            }
        }
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: None,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// AST walk — cursor resolution
// ──────────────────────────────────────────────────────────────────────────────

/// A segment in a YAML key path.
#[derive(Debug, Clone)]
enum PathSegment {
    Key(String),
    Index(usize),
}

/// Walk all documents and return the path + deepest node whose span contains `cursor`.
fn ast_walk(docs: &[Document<Span>], cursor: Pos) -> Option<(Vec<PathSegment>, &Node<Span>)> {
    for doc in docs {
        if span_contains(node_loc(&doc.root), cursor) {
            let mut path = Vec::new();
            if let Some(result) = walk_node(&doc.root, cursor, &mut path) {
                return Some(result);
            }
        }
    }
    None
}

/// Recursively descend into `node` accumulating `path`. Returns the deepest
/// (path, node) pair whose span contains `cursor`.
fn walk_node<'a>(
    node: &'a Node<Span>,
    cursor: Pos,
    path: &mut Vec<PathSegment>,
) -> Option<(Vec<PathSegment>, &'a Node<Span>)> {
    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                let key_name = match key {
                    Node::Scalar { value, .. } => value.clone(),
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
                };

                if span_contains(node_loc(key), cursor) {
                    // Cursor is on the key — hover shows the value at this key.
                    path.push(PathSegment::Key(key_name));
                    return Some((path.clone(), value));
                }

                if span_contains(node_loc(value), cursor) {
                    path.push(PathSegment::Key(key_name));
                    return walk_node(value, cursor, path).or_else(|| Some((path.clone(), value)));
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for (idx, item) in items.iter().enumerate() {
                if span_contains(node_loc(item), cursor) {
                    path.push(PathSegment::Index(idx));
                    return walk_node(item, cursor, path).or_else(|| Some((path.clone(), item)));
                }
            }
            None
        }
        // Scalar or Alias: leaf node — already confirmed span contains cursor.
        Node::Scalar { .. } | Node::Alias { .. } => Some((path.clone(), node)),
    }
}

/// Returns `true` when `cursor` is within `span` using half-open `[start, end)`.
///
/// Comparison is lexicographic on `(line, column)`.
fn span_contains(span: Span, cursor: Pos) -> bool {
    let start = (span.start.line, span.start.column);
    let end = (span.end.line, span.end.column);
    let pos = (cursor.line, cursor.column);
    start <= pos && pos < end
}

/// Extract the `loc` span from any AST node.
const fn node_loc(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Path formatting
// ──────────────────────────────────────────────────────────────────────────────

/// Format the path segments into a dotted path string.
fn format_path(path: &[PathSegment]) -> String {
    let mut result = String::new();
    for (i, segment) in path.iter().enumerate() {
        match segment {
            PathSegment::Key(key) => {
                if i > 0 {
                    result.push('.');
                }
                result.push_str(key);
            }
            PathSegment::Index(idx) => {
                result.push('[');
                result.push_str(&idx.to_string());
                result.push(']');
            }
        }
    }
    result
}

// ──────────────────────────────────────────────────────────────────────────────
// Node info helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Get the type name for a YAML node.
fn yaml_type_name(node: &Node<Span>) -> String {
    match node {
        Node::Mapping { .. } => "mapping".to_string(),
        Node::Sequence { .. } => "sequence".to_string(),
        Node::Scalar { .. } => "scalar".to_string(),
        Node::Alias { .. } => "alias".to_string(),
    }
}

/// Get the scalar value representation for display.
fn scalar_value(node: &Node<Span>) -> Option<String> {
    match node {
        Node::Scalar { value, .. } => Some(value.clone()),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Markdown formatting
// ──────────────────────────────────────────────────────────────────────────────

/// Escape backtick characters in a string for safe embedding in a markdown code span.
fn escape_for_code_span(s: &str) -> String {
    s.replace('`', "\\`")
}

/// Truncate a string to at most `max_chars` characters.
/// If truncated, appends the Unicode ellipsis character (U+2026).
fn truncate_to(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let keep = max_chars - 1;
    let truncated: String = s
        .char_indices()
        .nth(keep)
        .map_or_else(|| s.to_string(), |(byte_idx, _)| s[..byte_idx].to_string());
    format!("{truncated}\u{2026}")
}

/// Format the hover content as Markdown.
fn format_hover_markdown(path: &str, yaml_type: &str, value: Option<&str>) -> String {
    let mut md = String::new();
    let escaped_path = escape_for_code_span(path);
    let _ = write!(md, "**Path:** `{escaped_path}`\n\n");
    let _ = writeln!(md, "**Type:** {yaml_type}");
    if let Some(val) = value {
        let escaped_val = escape_for_code_span(val);
        let _ = write!(md, "\n**Value:** `{escaped_val}`\n");
    }
    md
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema lookup
// ──────────────────────────────────────────────────────────────────────────────

/// Build a schema key path (list of string keys) from a dotted path string like "a.b.c".
/// Array index segments like "[0]" are represented as "[]" to match schema `items` lookups.
fn build_schema_key_path(dotted_path: &str) -> Vec<String> {
    dotted_path
        .split('.')
        .flat_map(|segment| {
            // Handle "foo[0]" → ["foo", "[]"]
            segment.find('[').map_or_else(
                || vec![segment.to_string()],
                |bracket_pos| {
                    let key = &segment[..bracket_pos];
                    if key.is_empty() {
                        vec!["[]".to_string()]
                    } else {
                        vec![key.to_string(), "[]".to_string()]
                    }
                },
            )
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Resolve a dotted key path through a `JsonSchema`, returning the matching sub-schema
/// for the final key. Returns `None` if the path cannot be resolved.
fn resolve_schema_path<'a>(schema: &'a JsonSchema, path: &[String]) -> Option<&'a JsonSchema> {
    let [key, rest @ ..] = path else {
        return None;
    };

    // Look in direct properties
    let found = schema
        .properties
        .as_ref()
        .and_then(|props| props.get(key.as_str()));

    // If not in direct properties, check composition branches
    let found = found.or_else(|| find_in_branches(schema, key));

    let child = found?;

    if rest.is_empty() {
        Some(child)
    } else {
        resolve_schema_path(child, rest)
    }
}

/// Search composition branches (allOf / anyOf / oneOf) for a property key.
fn find_in_branches<'a>(schema: &'a JsonSchema, key: &str) -> Option<&'a JsonSchema> {
    schema
        .all_of
        .iter()
        .flatten()
        .chain(schema.any_of.iter().flatten())
        .chain(schema.one_of.iter().flatten())
        .find_map(|branch| branch.properties.as_ref()?.get(key))
}

/// Format the schema information section appended below the structural hover.
fn format_schema_section(schema: &JsonSchema) -> String {
    let mut md = String::new();

    // Description takes priority over title; skip empty strings
    let text = schema
        .description
        .as_deref()
        .filter(|d| !d.is_empty())
        .or_else(|| schema.title.as_deref().filter(|t| !t.is_empty()));

    if let Some(desc) = text {
        let truncated = truncate_to(desc, MAX_DESCRIPTION_LEN);
        let _ = writeln!(md, "\n**Description:** {truncated}");
    }

    // Schema type
    if let Some(schema_type) = &schema.schema_type {
        let type_str = match schema_type {
            crate::schema::SchemaType::Single(t) => t.clone(),
            crate::schema::SchemaType::Multiple(ts) => ts.join(" | "),
        };
        let _ = writeln!(md, "\n**Schema type:** {type_str}");
    }

    // Default value
    if let Some(default) = &schema.default {
        let _ = writeln!(md, "\n**Default:** {default}");
    }

    // Examples — at most MAX_EXAMPLES, with "and N more" note
    if let Some(examples) = &schema.examples
        && !examples.is_empty()
    {
        let shown = examples.len().min(MAX_EXAMPLES);
        let _ = write!(md, "\n**Examples:**");
        examples.iter().take(shown).for_each(|ex| {
            match ex {
                serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                    // Pretty-print objects/arrays in a fenced code block.
                    // Fall back to compact inline if pretty form exceeds the limit
                    // (a truncated code block renders incorrectly).
                    if let Ok(pretty) = serde_json::to_string_pretty(ex) {
                        if pretty.chars().count() <= MAX_EXAMPLE_LEN {
                            let _ = write!(md, "\n```json\n{pretty}\n```");
                        } else {
                            let compact = json_value_to_display_string(ex);
                            let truncated = truncate_to(&compact, MAX_EXAMPLE_LEN);
                            let _ = write!(md, "\n- {truncated}");
                        }
                    } else {
                        let truncated =
                            truncate_to(&json_value_to_display_string(ex), MAX_EXAMPLE_LEN);
                        let _ = write!(md, "\n- {truncated}");
                    }
                }
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_) => {
                    let truncated = truncate_to(&json_value_to_display_string(ex), MAX_EXAMPLE_LEN);
                    let _ = write!(md, "\n- {truncated}");
                }
            }
        });
        let remaining = examples.len().saturating_sub(shown);
        if remaining > 0 {
            let _ = write!(md, "\n- *and {remaining} more*");
        }
        md.push('\n');
    }

    md
}

/// Convert a `serde_json::Value` to a display string for hover output.
/// Uses `Value::to_string()` which produces the JSON representation.
fn json_value_to_display_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::Array(_)
        | serde_json::Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;
    use serde_json::Value as JsonValue;
    use serde_json::json;

    use super::*;
    use crate::schema::{JsonSchema, SchemaType};
    use crate::test_utils::parse_docs;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn hover_content(hover: &Hover) -> &str {
        match &hover.contents {
            HoverContents::Markup(m) => &m.value,
            HoverContents::Scalar(_) | HoverContents::Array(_) => panic!("expected MarkupContent"),
        }
    }

    fn schema_with_description(description: &str) -> JsonSchema {
        JsonSchema {
            description: Some(description.to_string()),
            ..Default::default()
        }
    }

    // Tests 1, 5, 6 — hover contains key path and scalar type mention
    #[rstest]
    #[case::simple_key("name: Alice\n", pos(0, 0), "name")]
    #[case::nested_key("server:\n  port: 8080\n", pos(1, 2), "server.port")]
    #[case::deeply_nested_key("a:\n  b:\n    c: deep\n", pos(2, 4), "a.b.c")]
    fn hover_contains_key_path_and_scalar_type(
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] expected_path: &str,
    ) {
        let docs = parse_docs(text);
        let hover = hover_at(&docs, cursor, None).expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains(expected_path),
            "should contain key path {expected_path:?}, got: {content}"
        );
        assert!(
            content.to_lowercase().contains("scalar"),
            "should mention scalar type, got: {content}"
        );
    }

    // Test 2
    #[test]
    fn should_return_hover_for_simple_value() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 6), None);

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("name"), "should contain key path 'name'");
        assert!(
            content.to_lowercase().contains("scalar"),
            "should mention scalar type"
        );
        assert!(content.contains("Alice"), "should contain value 'Alice'");
    }

    // Tests 3, 4, 12, 15, 16 — hover returns None for degenerate structural positions
    #[rstest]
    #[case::whitespace("key: value\n\n", pos(1, 0))]
    #[case::comment("# comment\nkey: value\n", pos(0, 2))]
    #[case::empty_document("", pos(0, 0))]
    #[case::position_beyond_document("key: value\n", pos(5, 0))]
    #[case::document_separator_line("key1: value1\n---\nkey2: value2\n", pos(1, 0))]
    fn hover_returns_none_for_structural_cases(#[case] text: &str, #[case] cursor: Position) {
        let docs = parse_docs(text);
        let result = hover_at(&docs, cursor, None);
        assert!(result.is_none(), "expected None but got Some hover");
    }

    // Test 7
    #[test]
    fn should_return_hover_for_sequence_item() {
        let text = "items:\n  - first\n  - second\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(1, 4), None);

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("items[0]") || content.contains("items.0"),
            "should contain path like 'items[0]' or 'items.0'"
        );
        assert!(content.contains("first"), "should contain value 'first'");
    }

    // Tests 8, 9 — hover contains key path and compound (mapping/sequence) type mention
    #[rstest]
    #[case::mapping_value_type("server:\n  port: 8080\n", pos(0, 0), "server", "mapping")]
    #[case::sequence_value_type("items:\n  - one\n  - two\n", pos(0, 0), "items", "sequence")]
    fn hover_contains_key_path_and_compound_type(
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] expected_path: &str,
        #[case] expected_type: &str,
    ) {
        let docs = parse_docs(text);
        let hover = hover_at(&docs, cursor, None).expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains(expected_path),
            "should contain key path {expected_path:?}, got: {content}"
        );
        assert!(
            content.to_lowercase().contains(expected_type),
            "should mention {expected_type:?} type, got: {content}"
        );
    }

    // Test 10
    #[test]
    fn should_return_hover_with_scalar_value() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 6), None);

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("8080"), "should contain value '8080'");
    }

    // Test 11
    #[test]
    fn should_format_hover_as_markdown() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), None);

        let hover = result.expect("should return hover");
        match &hover.contents {
            HoverContents::Markup(m) => {
                assert_eq!(m.kind, MarkupKind::Markdown);
            }
            HoverContents::Scalar(_) | HoverContents::Array(_) => panic!("expected MarkupContent"),
        }
    }

    // Test 13 (adapted) — empty docs slice simulates failed parse
    #[test]
    fn should_return_none_when_document_failed_to_parse() {
        let result = hover_at(&[], pos(0, 0), None);
        assert!(result.is_none());
    }

    // Test 14
    #[test]
    fn should_return_hover_in_multi_document_yaml() {
        let text = "doc1key: value1\n---\ndoc2key: value2\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(2, 0), None);

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("doc2key"),
            "should contain key path 'doc2key'"
        );
    }

    // Test 17
    #[test]
    fn should_return_hover_for_boolean_value() {
        let text = "enabled: true\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 9), None);

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.to_lowercase().contains("scalar") || content.to_lowercase().contains("boolean"),
            "should mention scalar or boolean type"
        );
        assert!(content.contains("true"), "should contain value 'true'");
    }

    // Test 18
    #[test]
    fn should_return_hover_for_null_value() {
        let text = "empty: ~\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 7), None);

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.to_lowercase().contains("scalar") || content.to_lowercase().contains("null"),
            "should mention scalar or null type"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group B — Schema info at key position (Tests 19–24)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 19 — schema with description appended below structural hover
    #[test]
    fn schema_description_appended_for_key_at_root() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            schema_with_description("The user's display name"),
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("name"), "should contain key path 'name'");
        assert!(
            content.contains("The user's display name"),
            "should contain schema description"
        );
    }

    // Test 20 — schema type shown in hover
    #[test]
    fn schema_type_shown_for_key() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "port".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("integer"),
            "should contain schema type 'integer'"
        );
    }

    // Test 21 — schema default shown in hover
    #[test]
    fn schema_default_shown_for_key() {
        let text = "timeout: 30\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "timeout".to_string(),
            JsonSchema {
                default: Some(JsonValue::Number(30.into())),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("30"), "should contain default value '30'");
        assert!(
            content.to_lowercase().contains("default"),
            "should mention 'default'"
        );
    }

    // Test 22 — schema examples shown in hover (up to 3)
    #[test]
    fn schema_examples_shown_for_key() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                examples: Some(vec![
                    JsonValue::String("Alice".to_string()),
                    JsonValue::String("Bob".to_string()),
                ]),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.to_lowercase().contains("example"),
            "should mention examples"
        );
        assert!(content.contains("Alice"), "should show first example");
        assert!(content.contains("Bob"), "should show second example");
    }

    // Test 23 — no schema info when key not in schema properties
    #[test]
    fn no_schema_info_for_unknown_key() {
        let text = "unknown: value\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "known".to_string(),
            schema_with_description("A known property"),
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        // Structural hover still works
        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("unknown"),
            "should contain key path 'unknown'"
        );
        // Schema description not shown for unknown key
        assert!(
            !content.contains("A known property"),
            "should not show description for unknown key"
        );
    }

    // Test 24 — schema info for nested key resolves through properties
    #[test]
    fn schema_description_for_nested_key() {
        let text = "server:\n  port: 8080\n";
        let docs = parse_docs(text);
        let mut port_props = HashMap::new();
        port_props.insert(
            "port".to_string(),
            JsonSchema {
                description: Some("HTTP port number".to_string()),
                schema_type: Some(SchemaType::Single("integer".to_string())),
                ..Default::default()
            },
        );
        let mut root_props = HashMap::new();
        root_props.insert(
            "server".to_string(),
            JsonSchema {
                properties: Some(port_props),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(root_props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(1, 2), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("server.port"),
            "should contain nested path"
        );
        assert!(
            content.contains("HTTP port number"),
            "should contain nested schema description"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group C — Schema info at value position (Tests 25–26)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 25 — hovering on value shows schema description for the parent key
    #[test]
    fn schema_description_shown_for_value_position() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            schema_with_description("The user's display name"),
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 6), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("The user's display name"),
            "should show schema description when hovering on value"
        );
    }

    // Test 26 — hovering on value shows schema type for the parent key
    #[test]
    fn schema_type_shown_for_value_position() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "port".to_string(),
            JsonSchema {
                schema_type: Some(SchemaType::Single("integer".to_string())),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 6), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("integer"),
            "should show schema type when hovering on value"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group D — Formatting and truncation (Tests 27–31)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 27 — existing structural hover section present before schema section
    #[test]
    fn schema_info_appended_below_structural_hover() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            schema_with_description("The user's display name"),
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        let path_pos = content.find("Path").expect("should contain 'Path'");
        let schema_pos = content
            .find("The user's display name")
            .expect("should contain description");
        assert!(
            path_pos < schema_pos,
            "structural hover (Path) should appear before schema info"
        );
    }

    // Test 28 — long description is truncated to ≤200 chars + ellipsis
    #[test]
    fn long_description_truncated() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let long_desc = "A".repeat(500);
        let mut props = HashMap::new();
        props.insert("name".to_string(), schema_with_description(&long_desc));
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        // The full 500-char description must not appear verbatim
        assert!(
            !content.contains(&long_desc),
            "full 500-char description must not appear"
        );
        assert!(
            content.contains('\u{2026}'),
            "truncated description must end with ellipsis"
        );
        // Description body must be ≤199 chars (plus ellipsis = 200)
        let a_run: String = content
            .chars()
            .skip_while(|&c| c != 'A')
            .take_while(|&c| c == 'A')
            .collect();
        assert!(
            a_run.chars().count() <= 199,
            "truncated description body must be ≤199 chars (plus ellipsis = 200), got {}",
            a_run.chars().count()
        );
    }

    // Test 29/30/46 (consolidated) — long example value truncated to ≤100 chars (char-based)
    #[rstest]
    #[case::ascii_200("a".repeat(200), 'a')]
    #[case::unicode_200("é".repeat(200), 'é')]
    fn long_example_value_truncated(#[case] long_example: String, #[case] marker_char: char) {
        let text = "key: v\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "key".to_string(),
            JsonSchema {
                examples: Some(vec![JsonValue::String(long_example.clone())]),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            !content.contains(&long_example),
            "full 200-char example must not appear verbatim"
        );
        assert!(
            content.contains('\u{2026}'),
            "truncated example must end with ellipsis"
        );
        let char_run: String = content
            .chars()
            .skip_while(|&c| c != marker_char)
            .take_while(|&c| c == marker_char)
            .collect();
        assert!(
            char_run.chars().count() <= 100,
            "displayed example must be at most 100 chars (got {})",
            char_run.chars().count()
        );
    }

    // Test 31 — at most 3 examples shown; "and N more" note for overflow
    #[test]
    fn should_show_at_most_3_examples_with_overflow_note() {
        let text = "key: v\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "key".to_string(),
            JsonSchema {
                examples: Some(vec![
                    JsonValue::String("a".to_string()),
                    JsonValue::String("b".to_string()),
                    JsonValue::String("c".to_string()),
                    JsonValue::String("d".to_string()),
                    JsonValue::String("e".to_string()),
                ]),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        // Cap is 3, not 5
        assert!(
            content.contains("and 2 more") || content.contains("2 more"),
            "should show 'and 2 more' note for 5 examples capped at 3, got: {content}"
        );
        // items 4–5 must be absent as standalone example values
        let lines_with_d = content
            .lines()
            .filter(|l| l.trim() == "- d" || l.trim() == "d")
            .count();
        let lines_with_e = content
            .lines()
            .filter(|l| l.trim() == "- e" || l.trim() == "e")
            .count();
        assert_eq!(
            lines_with_d, 0,
            "example 'd' (4th) must not appear as a list item"
        );
        assert_eq!(
            lines_with_e, 0,
            "example 'e' (5th) must not appear as a list item"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group E — Nested paths through schema (Tests 32–33)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 32 — schema info for two-level nested key
    #[test]
    fn schema_info_for_two_level_nested_key() {
        let text = "database:\n  host: localhost\n";
        let docs = parse_docs(text);
        let mut db_props = HashMap::new();
        db_props.insert(
            "host".to_string(),
            schema_with_description("Database host address"),
        );
        let mut root_props = HashMap::new();
        root_props.insert(
            "database".to_string(),
            JsonSchema {
                properties: Some(db_props),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(root_props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(1, 2), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("database.host"),
            "should contain nested path"
        );
        assert!(
            content.contains("Database host address"),
            "should contain nested schema description"
        );
    }

    // Test 33 — schema info not shown for key two levels deeper than schema has
    #[test]
    fn no_schema_info_for_deeper_than_schema_provides() {
        let text = "a:\n  b:\n    c: deep\n";
        let docs = parse_docs(text);
        // Schema only describes 'a' with no nested properties
        let mut root_props = HashMap::new();
        root_props.insert("a".to_string(), schema_with_description("Top level A"));
        let schema = JsonSchema {
            properties: Some(root_props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(2, 4), Some(&schema));

        // Structural hover for a.b.c should still work
        let hover = result.expect("structural hover should work");
        let content = hover_content(&hover);
        assert!(content.contains("a.b.c"), "should contain path a.b.c");
        // Schema description for 'a' should NOT appear when on 'c'
        assert!(
            !content.contains("Top level A"),
            "should not show parent description when on nested key"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group F — Composition schemas (allOf / anyOf) (Tests 34–35)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 34 — allOf branch: property description found in first matching branch
    #[test]
    fn schema_info_from_all_of_branch() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut branch_props = HashMap::new();
        branch_props.insert(
            "name".to_string(),
            schema_with_description("Name from allOf branch"),
        );
        let schema = JsonSchema {
            all_of: Some(vec![JsonSchema {
                properties: Some(branch_props),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("Name from allOf branch"),
            "should find property description from allOf branch"
        );
    }

    // Test 35 — anyOf branch: property description found in first matching branch
    #[test]
    fn schema_info_from_any_of_branch() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut branch_props = HashMap::new();
        branch_props.insert(
            "name".to_string(),
            schema_with_description("Name from anyOf branch"),
        );
        let schema = JsonSchema {
            any_of: Some(vec![JsonSchema {
                properties: Some(branch_props),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("Name from anyOf branch"),
            "should find property description from anyOf branch"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group G — Fallback behaviour (Tests 36–38)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 36 — None schema: structural hover works, no schema section appended
    #[test]
    fn no_schema_hover_unchanged() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), None);

        let hover = result.expect("should return hover with None schema");
        let content = hover_content(&hover);
        assert!(
            content.contains("port"),
            "structural hover path must be present"
        );
        assert!(
            !content.contains("---"),
            "no schema section must be appended when schema is None"
        );
    }

    // Test 37 — Schema with no properties for cursor key: structural hover only
    #[test]
    fn schema_without_matching_property_shows_structural_only() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        // Schema has description at root but no properties
        let schema = schema_with_description("Root schema description");
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("port"), "structural hover path present");
        assert!(
            !content.contains("Root schema description"),
            "root description should not appear for a specific key"
        );
    }

    // Test 38 (adapted) — Schema present but empty docs: returns None
    #[test]
    fn schema_present_but_parse_fails_returns_none() {
        let schema = schema_with_description("some desc");
        let result = hover_at(&[], pos(0, 0), Some(&schema));

        assert!(result.is_none(), "should return None when no parsed docs");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group H — Edge cases (Tests 39–42)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 39 — empty description string: schema section not shown
    #[test]
    fn empty_description_not_shown() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                description: Some(String::new()),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        // Should still contain structural hover
        assert!(content.contains("name"), "structural hover present");
        // Schema section should not be added for empty description
        assert!(
            !content.contains("**Description:**"),
            "should not show description section for empty description"
        );
    }

    // Test 40 — title shown when description absent
    #[test]
    fn title_shown_when_description_absent() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                title: Some("User Name".to_string()),
                description: None,
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("User Name"),
            "should show title when description is absent"
        );
    }

    // Test 41 — null default value shown
    #[test]
    fn null_default_shown() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                default: Some(JsonValue::Null),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.to_lowercase().contains("null"),
            "should show null default"
        );
        assert!(
            content.to_lowercase().contains("default"),
            "should label the default"
        );
    }

    // Test 42 — title NOT shown when description present (description takes priority)
    #[test]
    fn description_takes_priority_over_title() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "name".to_string(),
            JsonSchema {
                title: Some("User Name".to_string()),
                description: Some("The full display name of the user".to_string()),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("The full display name of the user"),
            "should show description"
        );
        // Title should not appear when description is present
        assert!(
            !content.contains("User Name"),
            "should not show title when description is present"
        );
    }

    // Test 42b — 10 examples: show first 3, note "and 7 more"
    #[test]
    fn should_show_only_first_3_examples_when_10_provided() {
        let text = "key: v\n";
        let docs = parse_docs(text);
        let examples: Vec<JsonValue> = (0..10)
            .map(|i| JsonValue::String(format!("ex{i}")))
            .collect();
        let mut props = HashMap::new();
        props.insert(
            "key".to_string(),
            JsonSchema {
                examples: Some(examples),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        // First 3 shown; ex3 through ex9 must be absent as list items
        assert!(content.contains("ex0"), "ex0 must appear");
        for i in 3..10 {
            let item = format!("ex{i}");
            let lines_with_item = content
                .lines()
                .filter(|l| l.trim() == format!("- {item}") || l.trim() == item)
                .count();
            assert_eq!(
                lines_with_item, 0,
                "example '{item}' must not appear as a list item"
            );
        }
        // "and 7 more" note expected
        assert!(
            content.contains("7 more"),
            "should contain 'and 7 more' note, got: {content}"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Security tests (Tests 43–47)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 43 — description truncation is char-based (not byte-based), cap is 200 chars
    #[test]
    fn should_truncate_long_description_in_hover_at_200_chars() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        // 300 × 'é' = 600 bytes but 300 chars; truncated at 200 chars (199 body + ellipsis)
        let long_desc: String = "é".repeat(300);
        let mut props = HashMap::new();
        props.insert("name".to_string(), schema_with_description(&long_desc));
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        // Extract the é-run from the content
        let e_run: String = content
            .chars()
            .skip_while(|&c| c != 'é')
            .take_while(|&c| c == 'é')
            .collect();
        assert!(
            e_run.chars().count() <= 199,
            "truncation must use chars not bytes; body must be ≤199 chars (got {})",
            e_run.chars().count()
        );
        assert!(
            content.contains('\u{2026}'),
            "truncated description must end with ellipsis"
        );
    }

    // Test 44 — backtick in schema default value rendered safely
    #[test]
    fn should_escape_backtick_in_schema_default_value() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert(
            "key".to_string(),
            JsonSchema {
                default: Some(JsonValue::String("foo`bar".to_string())),
                ..Default::default()
            },
        );
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        // Must NOT contain a broken code span pattern "`foo`bar`"
        assert!(
            !content.contains("`foo`bar`"),
            "must not contain broken code span '`foo`bar`', got: {content}"
        );
        // Must still render something that includes "foo" (the default appears)
        assert!(
            content.contains("foo"),
            "default value 'foo' must appear somewhere"
        );
    }

    // Test 47 — backtick in YAML value escaped in **Value:** code span
    #[test]
    fn should_escape_backtick_in_yaml_value_display() {
        // YAML value contains a backtick — must be escaped so the code span doesn't break
        let text = "foo: bar`baz\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 5), None);

        assert!(result.is_some(), "should return hover");
        let hover = result.unwrap();
        let content = hover_content(&hover);
        // The raw markdown must not contain the broken pattern "`bar`baz`"
        assert!(
            !content.contains("`bar`baz`"),
            "must not contain broken code span '`bar`baz`', got: {content}"
        );
        // "bar" must still appear in some form
        assert!(content.contains("bar"), "value 'bar' must appear in hover");
    }

    // Test 48 — no schema section when schema has no info for the hovered key
    #[test]
    fn should_show_structural_only_when_schema_has_no_info_for_hovered_key() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let mut props = HashMap::new();
        props.insert("other".to_string(), schema_with_description("Other"));
        let schema = JsonSchema {
            properties: Some(props),
            ..Default::default()
        };
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        assert!(result.is_some(), "should return hover");
        let hover = result.unwrap();
        let content = hover_content(&hover);
        // No schema section separator or "Other" description
        assert!(
            !content.contains("---"),
            "should not contain schema section separator"
        );
        assert!(
            !content.contains("Other"),
            "should not show 'Other' description"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group I — Previously uncovered paths (Tests 56–65, adapted)
    // ──────────────────────────────────────────────────────────────────────────

    // Test 57 — cursor on sequence item
    #[test]
    fn hover_returns_sequence_value_when_cursor_on_item() {
        let text = "items:\n  - first\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(1, 4), None);

        let hover = result.expect("should return hover for sequence item");
        let content = hover_content(&hover);
        assert!(
            content.contains("items"),
            "should contain parent key 'items'"
        );
    }

    // Test 59 — plain scalar sequence item (no colon)
    #[test]
    fn hover_on_plain_scalar_line_returns_value_token() {
        let text = "- plainvalue\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 4), None);

        let hover = result.expect("should return hover for plain scalar in sequence");
        let content = hover_content(&hover);
        assert!(
            content.contains('0') || content.contains("plainvalue"),
            "should contain sequence index or value"
        );
    }

    // Test 60 (strengthened) — key with no value: parser produces null scalar, AST finds it
    #[test]
    fn hover_on_sequence_item_key_with_no_value_returns_hover() {
        let text = "items:\n  - name:\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(1, 6), None);

        // Parser produces a null scalar for "name:" — AST walk must find it.
        let hover = result.expect("should return hover for key with null value");
        let content = hover_content(&hover);
        assert!(
            content.contains("name") || content.contains("items"),
            "path should reference the key"
        );
    }

    // Test 61 — mapping value token when cursor is on value side
    #[test]
    fn hover_on_value_side_of_mapping_returns_value_token() {
        let text = "status: active\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 8), None);

        let hover = result.expect("should return hover for value side of mapping");
        let content = hover_content(&hover);
        assert!(
            content.contains("status"),
            "path should contain key 'status'"
        );
        assert!(content.contains("active"), "should contain value 'active'");
    }

    // Tests 58, 65 — hover returns None for degenerate input
    #[rstest]
    #[case::ellipsis_terminator("key: value\n...\n", pos(1, 0))]
    fn hover_returns_none_for_degenerate_input(#[case] text: &str, #[case] cursor: Position) {
        let docs = parse_docs(text);
        let result = hover_at(&docs, cursor, None);
        assert!(result.is_none(), "expected None but got Some hover");
    }

    // Test 62 — key with empty name after colon strip: must not panic
    #[test]
    fn hover_does_not_panic_for_line_starting_with_colon() {
        let text = ": orphan\n";
        let docs = parse_docs(text);
        let _result = hover_at(&docs, pos(0, 0), None);
    }

    // Tests 63, 64 — hover does not panic on sequence item positions
    #[rstest]
    #[case::key_before_colon("people:\n  - name: Alice\n", pos(1, 5))]
    #[case::value_after_colon("people:\n  - name: Alice\n", pos(1, 12))]
    fn hover_does_not_panic_on_sequence_item_positions(
        #[case] text: &str,
        #[case] cursor: Position,
    ) {
        let docs = parse_docs(text);
        let _result = hover_at(&docs, cursor, None);
        // must not panic — AST path resolution may or may not find the node
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group J — Hover examples formatting (Tests 51–55)
    // ──────────────────────────────────────────────────────────────────────────

    fn schema_with_examples(examples: Vec<JsonValue>) -> JsonSchema {
        let mut props = HashMap::new();
        props.insert(
            "key".to_string(),
            JsonSchema {
                examples: Some(examples),
                ..Default::default()
            },
        );
        JsonSchema {
            properties: Some(props),
            ..Default::default()
        }
    }

    // Test 51 — object example renders as fenced ```json code block
    #[test]
    fn object_example_renders_as_fenced_code_block() {
        let schema = schema_with_examples(vec![json!({"name": "Alice", "age": 30})]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("```json"),
            "object example should be in a fenced json code block, got: {content}"
        );
        // Pretty-printed: each key on its own line
        assert!(
            content.contains("\"name\": \"Alice\"") || content.contains("\"age\": 30"),
            "object example should be pretty-printed, got: {content}"
        );
    }

    // Test 52 — array example renders as fenced ```json code block
    #[test]
    fn array_example_renders_as_fenced_code_block() {
        let schema = schema_with_examples(vec![json!(["a", "b", "c"])]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("```json"),
            "array example should be in a fenced json code block, got: {content}"
        );
    }

    // Test 53 — simple value (string, number, bool) uses list-item format, no code block
    #[test]
    fn simple_value_examples_use_list_item_format() {
        let schema = schema_with_examples(vec![json!("hello"), json!(42), json!(true)]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            !content.contains("```json"),
            "simple examples must not use code blocks, got: {content}"
        );
        assert!(
            content.contains("- hello"),
            "string example should appear as list item '- hello', got: {content}"
        );
        assert!(
            content.contains("- 42"),
            "number example should appear as list item '- 42', got: {content}"
        );
        assert!(
            content.contains("- true"),
            "bool example should appear as list item '- true', got: {content}"
        );
    }

    // Test 54 — mixed examples: object gets code block, simple gets list item
    #[test]
    fn mixed_examples_dispatch_correctly_per_type() {
        let schema = schema_with_examples(vec![json!({"host": "localhost"}), json!("simple")]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("```json"),
            "object example should use code block, got: {content}"
        );
        assert!(
            content.contains("- simple"),
            "string example should use list item format, got: {content}"
        );
    }

    // Test 55 — long object exceeding MAX_EXAMPLE_LEN falls back to compact inline
    #[test]
    fn long_object_example_falls_back_to_compact_inline() {
        // Build an object whose pretty-printed form exceeds 100 chars
        let big_value: String = "x".repeat(50);
        let schema = schema_with_examples(vec![json!({
            "field_a": big_value,
            "field_b": "another long value that pushes past the limit"
        })]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&schema));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        // The pretty form exceeds 100 chars, so no code block should appear
        assert!(
            !content.contains("```json"),
            "long object should fall back to compact inline (no code block), got: {content}"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group K — AST span walk regression tests (new)
    // ──────────────────────────────────────────────────────────────────────────

    // New test 4 — multi-doc: correct document from span, not --- counting
    #[test]
    fn hover_uses_span_containment_not_line_counting() {
        let text = "a: 1\n---\nb: 2\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(2, 0), None);

        let hover = result.expect("should return hover for second document");
        let content = hover_content(&hover);
        assert!(
            content.contains('b'),
            "should resolve to second document key 'b', got: {content}"
        );
        assert!(
            !content.contains("**Path:** `a`"),
            "should not resolve to first document key 'a'"
        );
    }

    // New test 7 — empty line between nodes returns None
    #[test]
    fn hover_on_empty_line_between_nodes_returns_none() {
        let text = "a: 1\n\nb: 2\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(1, 0), None);
        assert!(result.is_none(), "empty line should return None");
    }

    // New test 8 — trailing comment position returns None
    #[test]
    fn hover_on_trailing_comment_returns_none() {
        let text = "key: value  # comment\n";
        let docs = parse_docs(text);
        // col 13 is within "# comment"
        let result = hover_at(&docs, pos(0, 13), None);
        assert!(
            result.is_none(),
            "cursor in comment region should return None"
        );
    }

    // New test 10 — sequence index is 0-based, third item = index 2
    #[test]
    fn hover_on_sequence_item_path_uses_zero_based_index() {
        let text = "items:\n  - first\n  - second\n  - third\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(3, 4), None);

        let hover = result.expect("should return hover for third item");
        let content = hover_content(&hover);
        assert!(
            content.contains("items[2]") || content.contains("items.2"),
            "third item should have 0-based index 2, got: {content}"
        );
    }

    // New test 11 — second sequence item index
    #[test]
    fn hover_on_second_sequence_item_has_correct_index() {
        let text = "list:\n  - a\n  - b\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(2, 4), None);

        let hover = result.expect("should return hover for second item");
        let content = hover_content(&hover);
        assert!(
            content.contains("list[1]") || content.contains("list.1"),
            "second item should have index 1, got: {content}"
        );
    }

    // New test 12 — nested mapping value path
    #[test]
    fn hover_on_nested_mapping_value_returns_correct_path() {
        let text = "outer:\n  inner: hello\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(1, 9), None);

        let hover = result.expect("should return hover for nested value");
        let content = hover_content(&hover);
        assert!(
            content.contains("outer.inner"),
            "should contain nested path 'outer.inner', got: {content}"
        );
        assert!(content.contains("hello"), "should contain value 'hello'");
    }

    // New test 13 — flow mapping: at minimum no panic
    #[test]
    fn hover_on_flow_mapping_key_does_not_panic() {
        let text = "meta: {name: Alice}\n";
        let docs = parse_docs(text);
        // Cursor on "meta" key
        let _result = hover_at(&docs, pos(0, 0), None);
        // Must not panic; result depends on parser span granularity for flow content
    }

    // New test 14 — three-document stream: correct document resolved
    #[test]
    fn hover_returns_correct_document_in_three_doc_stream() {
        let text = "a: 1\n---\nb: 2\n---\nc: 3\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(4, 0), None);

        let hover = result.expect("should return hover for third document");
        let content = hover_content(&hover);
        assert!(
            content.contains('c'),
            "should resolve to third document key 'c', got: {content}"
        );
    }

    // Span boundary: start is inclusive
    #[test]
    fn hover_at_span_start_is_inclusive() {
        let text = "key: val\n";
        let docs = parse_docs(text);
        // col 5 is the start of "val" (after "key: ")
        let result = hover_at(&docs, pos(0, 5), None);
        assert!(
            result.is_some(),
            "cursor at span start should be included (start-inclusive)"
        );
    }
}
