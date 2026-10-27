// SPDX-License-Identifier: MIT

use std::fmt::Write as _;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Pos, Span};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::schema::path::{SchemaPathSegment, resolve_schema_path};
use crate::schema::{JsonSchema, ParsedSchema};

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
    schema: Option<&ParsedSchema>,
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

    if let Some(parsed) = schema
        && let Some(prop_schema) = resolve_schema_path(parsed, &parsed.root, &path)
    {
        let schema_section = format_schema_section(prop_schema);
        if !schema_section.is_empty() {
            markdown.push('\n');
            markdown.push_str(&schema_section);
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

/// Walk all documents and return the path + deepest node whose span contains `cursor`.
fn ast_walk(docs: &[Document<Span>], cursor: Pos) -> Option<(Vec<SchemaPathSegment>, &Node<Span>)> {
    for doc in docs {
        let idx = doc.line_index();
        if span_contains(node_loc(&doc.root), cursor, idx) {
            let mut path = Vec::new();
            if let Some(result) = walk_node(&doc.root, cursor, &mut path, idx) {
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
    path: &mut Vec<SchemaPathSegment>,
    idx: &LineIndex,
) -> Option<(Vec<SchemaPathSegment>, &'a Node<Span>)> {
    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                let key_name = match key {
                    Node::Scalar { value, .. } => value.clone(),
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
                };

                if span_contains(node_loc(key), cursor, idx) {
                    // Cursor is on the key — hover shows the value at this key.
                    path.push(SchemaPathSegment::Key(key_name));
                    return Some((path.clone(), value));
                }

                if span_contains(node_loc(value), cursor, idx) {
                    path.push(SchemaPathSegment::Key(key_name));
                    return walk_node(value, cursor, path, idx)
                        .or_else(|| Some((path.clone(), value)));
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                if span_contains(node_loc(item), cursor, idx) {
                    path.push(SchemaPathSegment::Item(i));
                    return walk_node(item, cursor, path, idx)
                        .or_else(|| Some((path.clone(), item)));
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
fn span_contains(span: Span, cursor: Pos, idx: &LineIndex) -> bool {
    let (start_line, start_col) = idx.line_column(span.start);
    let (end_line, end_col) = idx.line_column(span.end);
    let start = (start_line as usize, start_col as usize);
    let end = (end_line as usize, end_col as usize);
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
fn format_path(path: &[SchemaPathSegment]) -> String {
    let mut result = String::new();
    for (i, segment) in path.iter().enumerate() {
        match segment {
            SchemaPathSegment::Key(key) => {
                if i > 0 {
                    result.push('.');
                }
                result.push_str(key);
            }
            SchemaPathSegment::Item(idx) => {
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
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;
    use serde_json::Value as JsonValue;
    use serde_json::json;

    use super::*;
    use crate::schema::test_support::{
        all_of, any_of, array_of, denying_additional_items, null_schema, object_schema, one_of,
        tuple_of, with_additional_items,
    };
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

    #[test]
    fn should_return_hover_with_scalar_value() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 6), None);

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("8080"), "should contain value '8080'");
    }

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

    // empty docs slice simulates failed parse
    #[test]
    fn should_return_none_when_document_failed_to_parse() {
        let result = hover_at(&[], pos(0, 0), None);
        assert!(result.is_none());
    }

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

    // schema with description appended below structural hover
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("name"), "should contain key path 'name'");
        assert!(
            content.contains("The user's display name"),
            "should contain schema description"
        );
    }

    // schema type shown in hover
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("integer"),
            "should contain schema type 'integer'"
        );
    }

    // schema default shown in hover
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("30"), "should contain default value '30'");
        assert!(
            content.to_lowercase().contains("default"),
            "should mention 'default'"
        );
    }

    // schema examples shown in hover (up to 3)
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.to_lowercase().contains("example"),
            "should mention examples"
        );
        assert!(content.contains("Alice"), "should show first example");
        assert!(content.contains("Bob"), "should show second example");
    }

    // no schema info when key not in schema properties
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // schema info for nested key resolves through properties
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
        let result = hover_at(&docs, pos(1, 2), Some(&ParsedSchema::new(schema)));

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

    // hovering on value shows schema description for the parent key
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
        let result = hover_at(&docs, pos(0, 6), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("The user's display name"),
            "should show schema description when hovering on value"
        );
    }

    // hovering on value shows schema type for the parent key
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
        let result = hover_at(&docs, pos(0, 6), Some(&ParsedSchema::new(schema)));

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

    // existing structural hover section present before schema section
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // long description is truncated to ≤200 chars + ellipsis
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // long example value truncated to ≤100 chars (char-based)
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // at most 3 examples shown; "and N more" note for overflow
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // schema info for two-level nested key
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
        let result = hover_at(&docs, pos(1, 2), Some(&ParsedSchema::new(schema)));

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

    // schema info not shown for key two levels deeper than schema has
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
        let result = hover_at(&docs, pos(2, 4), Some(&ParsedSchema::new(schema)));

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

    // allOf branch: property description found in first matching branch
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("Name from allOf branch"),
            "should find property description from allOf branch"
        );
    }

    // anyOf branch: property description found in first matching branch
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // None schema: structural hover works, no schema section appended
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

    // Schema with no properties for cursor key: structural hover only
    #[test]
    fn schema_without_matching_property_shows_structural_only() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        // Schema has description at root but no properties
        let schema = schema_with_description("Root schema description");
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("port"), "structural hover path present");
        assert!(
            !content.contains("Root schema description"),
            "root description should not appear for a specific key"
        );
    }

    // Schema present but empty docs: returns None
    #[test]
    fn schema_present_but_parse_fails_returns_none() {
        let schema = schema_with_description("some desc");
        let result = hover_at(&[], pos(0, 0), Some(&ParsedSchema::new(schema)));

        assert!(result.is_none(), "should return None when no parsed docs");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Group H — Edge cases (Tests 39–42)
    // ──────────────────────────────────────────────────────────────────────────

    // empty description string: schema section not shown
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // title shown when description absent
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("User Name"),
            "should show title when description is absent"
        );
    }

    // null default value shown
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // title NOT shown when description present (description takes priority)
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // 10 examples: show first 3, note "and 7 more"
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // description truncation is char-based (not byte-based), cap is 200 chars
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // backtick in schema default value rendered safely
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // backtick in YAML value escaped in **Value:** code span
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

    // no schema section when schema has no info for the hovered key
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // cursor on sequence item
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

    // plain scalar sequence item (no colon)
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

    // key with no value: parser produces null scalar, AST finds it
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

    // mapping value token when cursor is on value side
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

    // key with empty name after colon strip: must not panic
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

    // object example renders as fenced ```json code block
    #[test]
    fn object_example_renders_as_fenced_code_block() {
        let schema = schema_with_examples(vec![json!({"name": "Alice", "age": 30})]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // array example renders as fenced ```json code block
    #[test]
    fn array_example_renders_as_fenced_code_block() {
        let schema = schema_with_examples(vec![json!(["a", "b", "c"])]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("```json"),
            "array example should be in a fenced json code block, got: {content}"
        );
    }

    // simple value (string, number, bool) uses list-item format, no code block
    #[test]
    fn simple_value_examples_use_list_item_format() {
        let schema = schema_with_examples(vec![json!("hello"), json!(42), json!(true)]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // mixed examples: object gets code block, simple gets list item
    #[test]
    fn mixed_examples_dispatch_correctly_per_type() {
        let schema = schema_with_examples(vec![json!({"host": "localhost"}), json!("simple")]);
        let text = "key: v\n";
        let docs = parse_docs(text);
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // long object exceeding MAX_EXAMPLE_LEN falls back to compact inline
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
        let result = hover_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

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

    // ══════════════════════════════════════════════════════════════════════
    // Schema docs through the shared resolver
    // ══════════════════════════════════════════════════════════════════════

    fn with_root_description(description: &str, schema: JsonSchema) -> JsonSchema {
        JsonSchema {
            description: Some(description.to_string()),
            ..schema
        }
    }

    fn hover_with_schema(schema: JsonSchema, text: &str, cursor: Position) -> String {
        let docs = parse_docs(text);
        let hover = hover_at(&docs, cursor, Some(&ParsedSchema::new(schema)))
            .expect("structural hover should exist");
        hover_content(&hover).to_string()
    }

    #[rstest]
    #[case::key_below_array_item(
        "items:\n  - name: x\n",
        pos(1, 4),
        object_schema(vec![("items", array_of(object_schema(vec![("name", schema_with_description("Item name"))])))]),
        "items[0].name",
        "Item name"
    )]
    #[case::array_item_itself(
        "items:\n  - x\n",
        pos(1, 4),
        object_schema(vec![("items", array_of(schema_with_description("Item doc")))]),
        "items[0]",
        "Item doc"
    )]
    #[case::item_of_nested_sequence(
        "a:\n  - - p\n    - q\n",
        pos(2, 6),
        object_schema(vec![("a", array_of(array_of(schema_with_description("Cell doc"))))]),
        "a[0][1]",
        "Cell doc"
    )]
    #[case::key_below_item_of_root_sequence(
        "- name: x\n",
        pos(0, 2),
        array_of(object_schema(vec![("name", schema_with_description("Root item name"))])),
        "[0].name",
        "Root item name"
    )]
    #[case::key_in_branch_of_branch(
        "name: x\n",
        pos(0, 0),
        all_of(vec![any_of(vec![one_of(vec![object_schema(vec![("name", schema_with_description("Deep branch name"))])])])]),
        "name",
        "Deep branch name"
    )]
    #[case::later_branch_resolves_when_first_dead_ends(
        "a:\n  b: 1\n",
        pos(1, 2),
        all_of(vec![
            object_schema(vec![("a", object_schema(vec![("other", schema_with_description("Wrong branch"))]))]),
            object_schema(vec![("a", object_schema(vec![("b", schema_with_description("Right branch"))]))]),
        ]),
        "a.b",
        "Right branch"
    )]
    #[case::key_containing_dots(
        "metadata:\n  app.kubernetes.io/name: x\n",
        pos(1, 2),
        object_schema(vec![("metadata", object_schema(vec![("app.kubernetes.io/name", schema_with_description("App label"))]))]),
        "metadata.app.kubernetes.io/name",
        "App label"
    )]
    #[case::key_containing_brackets(
        "foo[bar]: x\n",
        pos(0, 0),
        object_schema(vec![("foo[bar]", schema_with_description("Bracketed key"))]),
        "foo[bar]",
        "Bracketed key"
    )]
    #[case::quoted_brackets_key_is_a_property(
        "\"[]\": x\n",
        pos(0, 0),
        JsonSchema {
            items: Some(Box::new(schema_with_description("Items doc"))),
            ..object_schema(vec![("[]", schema_with_description("Brackets property"))])
        },
        "[]",
        "Brackets property"
    )]
    #[case::empty_key_at_root(
        "\"\": x\n",
        pos(0, 0),
        with_root_description("Root doc", object_schema(vec![("", schema_with_description("Empty key doc"))])),
        "",
        "Empty key doc"
    )]
    #[case::empty_key_below_another_key(
        "a:\n  \"\": x\n",
        pos(1, 2),
        object_schema(vec![("a", with_root_description("A doc", object_schema(vec![("", schema_with_description("Empty child doc"))])))]),
        "a.",
        "Empty child doc"
    )]
    #[case::scalar_only_document_shows_root_docs(
        "just text\n",
        pos(0, 0),
        with_root_description("Root doc", JsonSchema::default()),
        "",
        "Root doc"
    )]
    #[case::dotted_key_is_one_property(
        "a.b: x\n",
        pos(0, 0),
        object_schema(vec![
            ("a.b", schema_with_description("Dotted doc")),
            ("a", object_schema(vec![("b", schema_with_description("Nested doc"))])),
        ]),
        "a.b",
        "Dotted doc"
    )]
    #[case::nested_keys_are_not_one_dotted_property(
        "a:\n  b: x\n",
        pos(1, 2),
        object_schema(vec![
            ("a.b", schema_with_description("Dotted doc")),
            ("a", object_schema(vec![("b", schema_with_description("Nested doc"))])),
        ]),
        "a.b",
        "Nested doc"
    )]
    #[case::bracketed_key_is_not_a_key_with_an_item(
        "foo[bar]: x\n",
        pos(0, 0),
        object_schema(vec![
            ("foo[bar]", schema_with_description("Bracketed doc")),
            ("foo", array_of(schema_with_description("Foo item doc"))),
        ]),
        "foo[bar]",
        "Bracketed doc"
    )]
    #[case::quoted_index_looking_key_is_a_key(
        "\"[0]\": x\n",
        pos(0, 0),
        JsonSchema {
            items: Some(Box::new(schema_with_description("Items doc"))),
            ..object_schema(vec![("[0]", schema_with_description("Index-looking key doc"))])
        },
        "[0]",
        "Index-looking key doc"
    )]
    #[case::root_sequence_item_is_an_item_not_a_brackets_key(
        "- x\n",
        pos(0, 2),
        JsonSchema {
            items: Some(Box::new(schema_with_description("Items doc"))),
            ..object_schema(vec![("[]", schema_with_description("Brackets property"))])
        },
        "[0]",
        "Items doc"
    )]
    #[case::own_property_wins_over_a_branch_with_the_same_key(
        "k: x\n",
        pos(0, 0),
        JsonSchema {
            all_of: Some(vec![object_schema(vec![("k", schema_with_description("Branch doc"))])]),
            ..object_schema(vec![("k", schema_with_description("Direct doc"))])
        },
        "k",
        "Direct doc"
    )]
    #[case::item_of_an_any_of_array(
        "tags:\n  - x\n",
        pos(1, 4),
        object_schema(vec![("tags", any_of(vec![array_of(schema_with_description("Tag doc")), null_schema()]))]),
        "tags[0]",
        "Tag doc"
    )]
    #[case::key_below_item_of_an_any_of_array(
        "containers:\n  - image: x\n",
        pos(1, 4),
        object_schema(vec![(
            "containers",
            any_of(vec![
                array_of(object_schema(vec![("image", schema_with_description("Image doc"))])),
                null_schema(),
            ]),
        )]),
        "containers[0].image",
        "Image doc"
    )]
    #[case::item_of_a_root_sequence_in_a_branch(
        "- name: x\n",
        pos(0, 2),
        any_of(vec![
            array_of(object_schema(vec![("name", schema_with_description("Root item name"))])),
            null_schema(),
        ]),
        "[0].name",
        "Root item name"
    )]
    #[case::own_items_win_over_items_in_a_branch(
        "tags:\n  - x\n",
        pos(1, 4),
        object_schema(vec![(
            "tags",
            JsonSchema {
                items: Some(Box::new(schema_with_description("Own item doc"))),
                any_of: Some(vec![array_of(schema_with_description("Branch item doc"))]),
                ..JsonSchema::default()
            },
        )]),
        "tags[0]",
        "Own item doc"
    )]
    #[case::second_item_of_the_second_row(
        "a:\n  - - p\n  - - q\n    - r\n",
        pos(3, 6),
        object_schema(vec![("a", array_of(array_of(schema_with_description("Cell doc"))))]),
        "a[1][1]",
        "Cell doc"
    )]
    #[case::first_item_of_the_second_row(
        "a:\n  - - p\n  - - q\n    - r\n",
        pos(2, 6),
        object_schema(vec![("a", array_of(array_of(schema_with_description("Cell doc"))))]),
        "a[1][0]",
        "Cell doc"
    )]
    #[case::key_below_the_second_item(
        "items:\n  - x: 1\n  - name: y\n",
        pos(2, 4),
        object_schema(vec![(
            "items",
            array_of(object_schema(vec![("name", schema_with_description("Name doc"))])),
        )]),
        "items[1].name",
        "Name doc"
    )]
    #[case::first_item_of_a_tuple(
        "pair:\n  - a\n  - b\n",
        pos(1, 4),
        object_schema(vec![("pair", tuple_of(vec![schema_with_description("First doc"), schema_with_description("Second doc")]))]),
        "pair[0]",
        "First doc"
    )]
    #[case::second_item_of_a_tuple(
        "pair:\n  - a\n  - b\n",
        pos(2, 4),
        object_schema(vec![("pair", tuple_of(vec![schema_with_description("First doc"), schema_with_description("Second doc")]))]),
        "pair[1]",
        "Second doc"
    )]
    #[case::second_item_of_a_root_tuple(
        "- a\n- b\n",
        pos(1, 2),
        tuple_of(vec![schema_with_description("First doc"), schema_with_description("Second doc")]),
        "[1]",
        "Second doc"
    )]
    #[case::key_in_the_first_item_of_a_tuple(
        "t:\n  - n: 1\n  - m: 2\n",
        pos(1, 4),
        object_schema(vec![("t", tuple_of(vec![
            object_schema(vec![("n", schema_with_description("N doc"))]),
            object_schema(vec![("m", schema_with_description("M doc"))]),
        ]))]),
        "t[0].n",
        "N doc"
    )]
    #[case::key_in_the_second_item_of_a_tuple(
        "t:\n  - n: 1\n  - m: 2\n",
        pos(2, 4),
        object_schema(vec![("t", tuple_of(vec![
            object_schema(vec![("n", schema_with_description("N doc"))]),
            object_schema(vec![("m", schema_with_description("M doc"))]),
        ]))]),
        "t[1].m",
        "M doc"
    )]
    #[case::item_beyond_the_prefix_takes_items(
        "t:\n  - a\n  - b\n  - c\n",
        pos(3, 4),
        object_schema(vec![("t", JsonSchema {
            items: Some(Box::new(schema_with_description("Rest doc"))),
            ..tuple_of(vec![schema_with_description("First doc"), schema_with_description("Second doc")])
        })]),
        "t[2]",
        "Rest doc"
    )]
    #[case::item_beyond_the_tuple_takes_additional_items(
        "t:\n  - a\n  - b\n  - c\n",
        pos(3, 4),
        object_schema(vec![("t", with_additional_items(
            tuple_of(vec![schema_with_description("First doc"), schema_with_description("Second doc")]),
            schema_with_description("Additional doc"),
        ))]),
        "t[2]",
        "Additional doc"
    )]
    #[case::nested_tuples_use_both_indices_inner(
        "a:\n  - - p\n    - q\n  - - r\n",
        pos(2, 6),
        object_schema(vec![("a", tuple_of(vec![
            tuple_of(vec![schema_with_description("Doc 00"), schema_with_description("Doc 01")]),
            tuple_of(vec![schema_with_description("Doc 10"), schema_with_description("Doc 11")]),
        ]))]),
        "a[0][1]",
        "Doc 01"
    )]
    #[case::nested_tuples_use_both_indices_outer(
        "a:\n  - - p\n    - q\n  - - r\n",
        pos(3, 6),
        object_schema(vec![("a", tuple_of(vec![
            tuple_of(vec![schema_with_description("Doc 00"), schema_with_description("Doc 01")]),
            tuple_of(vec![schema_with_description("Doc 10"), schema_with_description("Doc 11")]),
        ]))]),
        "a[1][0]",
        "Doc 10"
    )]
    #[case::own_tuple_wins_over_items_in_a_branch(
        "tags:\n  - x\n",
        pos(1, 4),
        object_schema(vec![(
            "tags",
            JsonSchema {
                any_of: Some(vec![array_of(schema_with_description("Branch item doc"))]),
                ..tuple_of(vec![schema_with_description("Own position doc")])
            },
        )]),
        "tags[0]",
        "Own position doc"
    )]
    fn hover_shows_the_schema_docs_of_the_hovered_path(
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] schema: JsonSchema,
        #[case] expected_path: &str,
        #[case] expected_docs: &str,
    ) {
        let content = hover_with_schema(schema, text, cursor);

        assert!(
            content.contains(&format!("**Path:** `{expected_path}`")),
            "path {expected_path:?} missing: {content}"
        );
        assert!(
            content.contains(&format!("**Description:** {expected_docs}")),
            "docs {expected_docs:?} missing: {content}"
        );
        assert_eq!(
            content.matches("**Description:**").count(),
            1,
            "exactly one schema's docs expected: {content}"
        );
    }

    #[rstest]
    #[case::quoted_brackets_key_is_not_an_item(
        "\"[]\": x\n",
        pos(0, 0),
        JsonSchema {
            items: Some(Box::new(schema_with_description("Items doc"))),
            ..JsonSchema::default()
        }
    )]
    #[case::own_property_shadows_branches(
        "a:\n  b: 1\n",
        pos(1, 2),
        JsonSchema {
            all_of: Some(vec![object_schema(vec![("a", object_schema(vec![("b", schema_with_description("Branch docs"))]))])]),
            ..object_schema(vec![("a", object_schema(vec![]))])
        }
    )]
    #[case::empty_key_is_not_its_parent(
        "a:\n  \"\": x\n",
        pos(1, 2),
        object_schema(vec![("a", with_root_description("A doc", JsonSchema::default()))])
    )]
    #[case::sequence_index_is_not_a_key(
        "a:\n  - x\n",
        pos(1, 4),
        object_schema(vec![(
            "a",
            with_root_description("A doc", object_schema(vec![("0", schema_with_description("Zero key doc"))])),
        )])
    )]
    #[case::item_where_schema_has_no_items(
        "tags:\n  - x\n",
        pos(1, 4),
        object_schema(vec![("tags", schema_with_description("Tags doc"))])
    )]
    #[case::key_below_item_missing_from_items_schema(
        "items:\n  - other: x\n",
        pos(1, 4),
        object_schema(vec![("items", array_of(object_schema(vec![("name", schema_with_description("Name doc"))])))])
    )]
    #[case::one_index_more_than_the_schema_has_levels(
        "a:\n  - - x\n",
        pos(1, 6),
        object_schema(vec![("a", array_of(schema_with_description("Single level doc")))])
    )]
    #[case::item_beyond_a_tuple_that_denies_additional_items(
        "t:\n  - a\n  - b\n  - c\n",
        pos(3, 4),
        object_schema(vec![("t", JsonSchema {
            any_of: Some(vec![array_of(schema_with_description("Branch item doc"))]),
            ..denying_additional_items(tuple_of(vec![
                schema_with_description("First doc"),
                schema_with_description("Second doc"),
            ]))
        })])
    )]
    #[case::item_beyond_a_tuple_without_a_schema_for_it(
        "t:\n  - a\n  - b\n  - c\n",
        pos(3, 4),
        object_schema(vec![("t", tuple_of(vec![
            schema_with_description("First doc"),
            schema_with_description("Second doc"),
        ]))])
    )]
    #[case::key_where_schema_is_an_array(
        "list:\n  name: x\n",
        pos(1, 2),
        object_schema(vec![(
            "list",
            with_root_description("List doc", array_of(object_schema(vec![("name", schema_with_description("Name doc"))]))),
        )])
    )]
    #[case::sequence_where_schema_is_an_object(
        "obj:\n  - x\n",
        pos(1, 4),
        object_schema(vec![(
            "obj",
            with_root_description("Obj doc", object_schema(vec![("x", schema_with_description("X doc"))])),
        )])
    )]
    #[case::item_where_no_branch_has_items(
        "tags:\n  - x\n",
        pos(1, 4),
        object_schema(vec![("tags", any_of(vec![null_schema(), schema_with_description("Branch doc")]))])
    )]
    #[case::own_items_shadow_branch_items_for_a_key_below_the_item(
        "tags:\n  - name: x\n",
        pos(1, 4),
        object_schema(vec![(
            "tags",
            JsonSchema {
                items: Some(Box::new(object_schema(vec![("other", schema_with_description("Other doc"))]))),
                any_of: Some(vec![array_of(object_schema(vec![("name", schema_with_description("Branch name"))]))]),
                ..JsonSchema::default()
            },
        )])
    )]
    fn hover_shows_no_schema_docs_when_the_path_does_not_resolve(
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] schema: JsonSchema,
    ) {
        let content = hover_with_schema(schema, text, cursor);

        assert!(
            !content.contains("**Description:**"),
            "no schema docs expected: {content}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════
    // Reference nodes
    // ══════════════════════════════════════════════════════════════════════

    mod references {
        use super::*;
        use crate::schema::test_support::{
            Sharing, assert_references_followed, reference_to, with_targets,
        };

        /// A schema with everything hover prints from a property schema.
        fn described() -> JsonSchema {
            JsonSchema {
                description: Some("The name".to_string()),
                schema_type: Some(SchemaType::Single("string".to_string())),
                default: Some(json!("anon")),
                examples: Some(vec![json!("a"), json!("b")]),
                ..JsonSchema::default()
            }
        }

        #[rstest]
        #[case::root(|t: &Sharing| t.put(object_schema(vec![("name", described())])), "name: x\n", pos(0, 0))]
        #[case::property(
            |t: &Sharing| object_schema(vec![("name", t.put(described()))]),
            "name: x\n",
            pos(0, 0)
        )]
        #[case::property_value(
            |t: &Sharing| object_schema(vec![("name", t.put(described()))]),
            "name: x\n",
            pos(0, 6)
        )]
        #[case::property_on_the_path(
            |t: &Sharing| object_schema(vec![("a", t.put(object_schema(vec![("name", t.put(described()))])))]),
            "a:\n  name: x\n",
            pos(1, 2)
        )]
        #[case::all_of_branch(
            |t: &Sharing| JsonSchema {
                all_of: Some(vec![t.put(object_schema(vec![("name", t.put(described()))]))]),
                ..JsonSchema::default()
            },
            "name: x\n",
            pos(0, 0)
        )]
        #[case::any_of_branch(
            |t: &Sharing| JsonSchema {
                any_of: Some(vec![t.put(object_schema(vec![("name", t.put(described()))]))]),
                ..JsonSchema::default()
            },
            "name: x\n",
            pos(0, 0)
        )]
        #[case::one_of_branch(
            |t: &Sharing| JsonSchema {
                one_of: Some(vec![t.put(object_schema(vec![("name", t.put(described()))]))]),
                ..JsonSchema::default()
            },
            "name: x\n",
            pos(0, 0)
        )]
        #[case::branch_on_the_path(
            |t: &Sharing| object_schema(vec![(
                "a",
                JsonSchema {
                    all_of: Some(vec![t.put(object_schema(vec![("name", t.put(described()))]))]),
                    ..JsonSchema::default()
                },
            )]),
            "a:\n  name: x\n",
            pos(1, 2)
        )]
        #[case::root_of_a_scalar_only_document(
            |t: &Sharing| t.put(described()),
            "x\n",
            pos(0, 0)
        )]
        #[case::array_item(
            |t: &Sharing| object_schema(vec![("list", t.put(array_of(t.put(described()))))]),
            "list:\n  - x\n",
            pos(1, 4)
        )]
        #[case::key_below_array_item(
            |t: &Sharing| object_schema(vec![(
                "list",
                array_of(t.put(object_schema(vec![("name", t.put(described()))]))),
            )]),
            "list:\n  - name: x\n",
            pos(1, 4)
        )]
        #[case::array_item_behind_a_branch(
            |t: &Sharing| object_schema(vec![(
                "list",
                any_of(vec![t.put(array_of(t.put(described()))), null_schema()]),
            )]),
            "list:\n  - x\n",
            pos(1, 4)
        )]
        #[case::key_below_item_behind_a_branch_of_a_reference(
            |t: &Sharing| object_schema(vec![(
                "list",
                t.put(all_of(vec![t.put(array_of(t.put(object_schema(vec![("name", t.put(described()))]))))])),
            )]),
            "list:\n  - name: x\n",
            pos(1, 4)
        )]
        fn a_reference_at_a_position_gives_the_hover_of_the_schema_written_inline(
            #[case] build: fn(&Sharing) -> JsonSchema,
            #[case] text: &str,
            #[case] cursor: Position,
        ) {
            let docs = parse_docs(text);

            assert_references_followed(build, |schema| hover_at(&docs, cursor, Some(schema)));
        }

        #[test]
        fn hover_ends_on_a_schema_that_references_itself() {
            let target = JsonSchema {
                all_of: Some(vec![reference_to(0)]),
                ..object_schema(vec![("next", reference_to(0)), ("name", described())])
            };
            let schema = with_targets(reference_to(0), vec![target]);
            let docs = parse_docs("next:\n  next:\n    next:\n      name: x\n");

            let hover = hover_at(&docs, pos(3, 6), Some(&schema)).expect("hover");

            assert!(
                hover_content(&hover).contains("**Description:** The name"),
                "{}",
                hover_content(&hover)
            );
        }

        #[test]
        fn hover_follows_two_schemas_that_reference_each_other() {
            let first = JsonSchema {
                description: Some("first".to_string()),
                ..object_schema(vec![("down", reference_to(1))])
            };
            let second = JsonSchema {
                description: Some("second".to_string()),
                ..object_schema(vec![("down", reference_to(0))])
            };
            let schema = with_targets(reference_to(0), vec![first, second]);
            let docs = parse_docs("down:\n  down:\n    down: x\n");

            let at = |line, character| {
                let hover = hover_at(&docs, pos(line, character), Some(&schema)).expect("hover");
                hover_content(&hover).to_string()
            };

            assert!(at(0, 0).contains("**Description:** second"), "{}", at(0, 0));
            assert!(at(1, 2).contains("**Description:** first"), "{}", at(1, 2));
            assert!(at(2, 4).contains("**Description:** second"), "{}", at(2, 4));
        }

        #[rstest]
        #[case::json_pointer(json!({ "$ref": "#/definitions/Name" }))]
        #[case::anchor(json!({ "$ref": "#name" }))]
        fn hover_on_a_property_of_a_parsed_schema_reads_the_description_behind_its_reference(
            #[case] reference: JsonValue,
        ) {
            let schema = crate::schema::parse_schema(&json!({
                "properties": { "name": reference },
                "definitions": { "Name": { "$anchor": "name", "description": "The name" } }
            }))
            .expect("parses");
            let docs = parse_docs("name: x\n");

            let hover = hover_at(&docs, pos(0, 0), Some(&schema)).expect("hover");

            assert!(hover_content(&hover).contains("**Description:** The name"));
        }

        #[rstest]
        #[case::item("containers:\n  - x\n", "Container item doc")]
        #[case::key_below_item("containers:\n  - image: x\n", "Image doc")]
        fn hover_on_an_array_declared_in_an_all_of_branch_of_a_reference_target_shows_the_item_docs(
            #[case] text: &str,
            #[case] expected: &str,
        ) {
            let schema = crate::schema::parse_schema(&json!({
                "properties": { "containers": { "$ref": "#/definitions/List" } },
                "definitions": { "List": { "allOf": [{
                    "type": "array",
                    "items": {
                        "type": "object",
                        "description": "Container item doc",
                        "properties": { "image": { "description": "Image doc" } }
                    }
                }] } }
            }))
            .expect("parses");
            let docs = parse_docs(text);

            let hover = hover_at(&docs, pos(1, 4), Some(&schema)).expect("hover");

            assert!(
                hover_content(&hover).contains(&format!("**Description:** {expected}")),
                "{}",
                hover_content(&hover)
            );
        }

        #[rstest]
        #[case::unresolved(JsonSchema {
            reference: Some(crate::schema::SchemaRef::Unresolved("#/missing".into())),
            ..JsonSchema::default()
        })]
        #[case::target_outside_the_table(reference_to(7))]
        fn hover_on_a_reference_without_a_target_has_no_schema_section(#[case] node: JsonSchema) {
            let schema = with_targets(object_schema(vec![("name", node)]), vec![described()]);
            let docs = parse_docs("name: x\n");

            let hover = hover_at(&docs, pos(0, 0), Some(&schema)).expect("hover");

            assert!(
                !hover_content(&hover).contains("**Description:**"),
                "{}",
                hover_content(&hover)
            );
        }

        #[test]
        fn hover_reads_a_reference_node_as_its_target_and_not_by_its_own_fields() {
            let node = JsonSchema {
                description: Some("own".to_string()),
                ..reference_to(0)
            };
            let schema = with_targets(object_schema(vec![("name", node)]), vec![described()]);
            let docs = parse_docs("name: x\n");

            let hover = hover_at(&docs, pos(0, 0), Some(&schema)).expect("hover");

            assert!(hover_content(&hover).contains("**Description:** The name"));
            assert!(!hover_content(&hover).contains("own"));
        }

        #[test]
        fn hover_on_a_key_the_cyclic_schema_does_not_declare_has_no_schema_section() {
            let target = JsonSchema {
                all_of: Some(vec![reference_to(0)]),
                ..object_schema(vec![("next", reference_to(0))])
            };
            let schema = with_targets(reference_to(0), vec![target]);
            let docs = parse_docs("next:\n  missing: x\n");

            let hover = hover_at(&docs, pos(1, 2), Some(&schema)).expect("hover");

            assert!(
                !hover_content(&hover).contains("**Description:**"),
                "{}",
                hover_content(&hover)
            );
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Keys described by patternProperties and additionalProperties
    // ══════════════════════════════════════════════════════════════════════

    mod key_patterns {
        use super::*;
        use crate::schema::ParsedSchema;
        use crate::schema::test_support::{
            denying_additional, described, reference_to, with_additional, with_patterns,
            with_targets,
        };

        /// A schema whose `jobs` map is `jobs`, with the job behind a reference
        /// and its steps behind a second one.
        fn workflow(jobs: impl FnOnce(JsonSchema) -> JsonSchema) -> ParsedSchema {
            let job = JsonSchema {
                description: Some("Job docs".to_string()),
                ..object_schema(vec![
                    ("runs-on", schema_with_description("Runner docs")),
                    ("steps", array_of(reference_to(1))),
                ])
            };
            let step = object_schema(vec![("uses", schema_with_description("Uses docs"))]);
            with_targets(
                object_schema(vec![("jobs", jobs(reference_to(0)))]),
                vec![job, step],
            )
        }

        fn through_pattern(job: JsonSchema) -> JsonSchema {
            with_patterns(JsonSchema::default(), vec![("^[a-z]+$", job)])
        }

        fn through_additional_properties(job: JsonSchema) -> JsonSchema {
            with_additional(JsonSchema::default(), job)
        }

        fn description_at(schema: &ParsedSchema, text: &str, cursor: Position) -> Option<String> {
            let docs = parse_docs(text);
            let hover = hover_at(&docs, cursor, Some(schema)).expect("hover");
            hover_content(&hover)
                .lines()
                .find_map(|line| line.strip_prefix("**Description:** "))
                .map(ToString::to_string)
        }

        #[rstest]
        #[case::the_key_of_a_job("jobs:\n  build:\n    runs-on: x\n", pos(1, 2), "Job docs")]
        #[case::a_key_below_the_job("jobs:\n  build:\n    runs-on: x\n", pos(2, 4), "Runner docs")]
        #[case::a_key_below_an_item_of_the_job(
            "jobs:\n  build:\n    steps:\n      - uses: x\n",
            pos(3, 8),
            "Uses docs"
        )]
        fn hover_shows_the_docs_below_a_key_a_pattern_or_additional_properties_describes(
            #[case] text: &str,
            #[case] cursor: Position,
            #[case] expected: &str,
            #[values(through_pattern, through_additional_properties)] jobs: fn(
                JsonSchema,
            )
                -> JsonSchema,
        ) {
            let schema = workflow(jobs);

            assert_eq!(
                description_at(&schema, text, cursor).as_deref(),
                Some(expected)
            );
        }

        #[rstest]
        #[case::no_pattern_matches(
            |job| denying_additional(through_pattern(job)),
        )]
        #[case::only_additional_properties_false(
            |_| denying_additional(JsonSchema::default()),
        )]
        fn hover_shows_no_docs_below_a_key_that_nothing_describes(
            #[case] jobs: fn(JsonSchema) -> JsonSchema,
        ) {
            let schema = workflow(jobs);

            assert_eq!(
                description_at(&schema, "jobs:\n  Bad1:\n    runs-on: x\n", pos(1, 2)),
                None
            );
            assert_eq!(
                description_at(&schema, "jobs:\n  Bad1:\n    runs-on: x\n", pos(2, 4)),
                None
            );
        }

        #[test]
        fn hover_shows_the_docs_of_the_first_matching_pattern_in_pattern_text_order() {
            let schema = ParsedSchema::new(object_schema(vec![(
                "jobs",
                with_patterns(
                    JsonSchema::default(),
                    vec![
                        ("^a", described("First docs")),
                        ("^ab", described("Second docs")),
                    ],
                ),
            )]));

            assert_eq!(
                description_at(&schema, "jobs:\n  abc: x\n", pos(1, 2)).as_deref(),
                Some("First docs")
            );
        }
    }
}
