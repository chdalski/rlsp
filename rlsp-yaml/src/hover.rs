use saphyr::{ScalarOwned, YamlOwned};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

/// Compute hover information for the given YAML text and cursor position.
///
/// Returns `None` if the position is on whitespace, a comment, a document
/// separator, outside the document, or when no AST is available.
#[must_use]
pub fn hover_at(
    text: &str,
    documents: Option<&Vec<YamlOwned>>,
    position: Position,
) -> Option<Hover> {
    let documents = documents?;
    if documents.is_empty() {
        return None;
    }

    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    // Bounds check
    let line = lines.get(line_idx)?;

    // Check if position is beyond line length
    if col_idx > line.len() {
        return None;
    }

    let trimmed = line.trim();

    // Empty line or comment
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    // Document separator
    if trimmed == "---" || trimmed == "..." {
        return None;
    }

    // Determine which document this line belongs to
    let doc_index = document_index_for_line(&lines, line_idx);
    let doc = documents.get(doc_index)?;

    // Parse the line to find what token the cursor is on
    let token = token_at_cursor(line, col_idx)?;

    // Walk the AST to find the node and build the path
    let result = find_node_info(doc, &token, line, &lines, line_idx)?;

    let markdown = format_hover_markdown(&result.path, &result.yaml_type, result.value.as_deref());

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: None,
    })
}

/// Determine which YAML document (by `---` separator) a line belongs to.
fn document_index_for_line(lines: &[&str], target_line: usize) -> usize {
    let mut doc_idx = 0;
    for (i, line) in lines.iter().enumerate() {
        if i >= target_line {
            break;
        }
        if line.trim() == "---" && i > 0 {
            doc_idx += 1;
        }
    }
    doc_idx
}

/// Information about a YAML token at the cursor position.
enum CursorToken {
    Key(String),
    Value(String),
    SequenceValue,
}

/// Extract the token at the cursor position from a YAML line.
fn token_at_cursor(line: &str, col: usize) -> Option<CursorToken> {
    let trimmed = line.trim();

    // Check for sequence item: "  - value"
    if let Some(after_dash) = trimmed.strip_prefix("- ") {
        let dash_col = line.find("- ").map_or(0, |i| i + 2);
        if col >= dash_col {
            let value = after_dash.trim();
            // Could be a key-value inside a sequence item: "- key: value"
            if let Some(colon_pos) = find_mapping_colon(after_dash) {
                let key = after_dash[..colon_pos].trim();
                let value_part = after_dash[colon_pos + 1..].trim();
                let abs_colon = dash_col + colon_pos;
                if col < abs_colon {
                    return Some(CursorToken::Key(key.to_string()));
                }
                if !value_part.is_empty() {
                    return Some(CursorToken::Value(value_part.to_string()));
                }
                return Some(CursorToken::Key(key.to_string()));
            }
            if !value.is_empty() {
                return Some(CursorToken::SequenceValue);
            }
            return None;
        }
        // On the dash itself — treat as sequence value
        let value = after_dash.trim();
        if !value.is_empty() {
            return Some(CursorToken::SequenceValue);
        }
        return None;
    }

    // Check for bare sequence item: "  - " followed by nothing meaningful on this line
    if trimmed == "-" {
        return None;
    }

    // Regular mapping line: "key: value"
    if let Some(colon_pos) = find_mapping_colon(line) {
        let key = line[..colon_pos].trim();
        let value_part = line[colon_pos + 1..].trim();

        if col <= colon_pos || value_part.is_empty() {
            // Cursor is on the key side or there is no value
            if !key.is_empty() {
                return Some(CursorToken::Key(key.to_string()));
            }
        } else {
            // Cursor is on the value side
            return Some(CursorToken::Value(value_part.to_string()));
        }
    } else if !trimmed.is_empty() {
        // Plain scalar line (no colon) — treat as a value
        return Some(CursorToken::Value(trimmed.to_string()));
    }

    None
}

/// Find the position of the mapping colon in a YAML line.
/// Skips colons inside quoted strings.
fn find_mapping_colon(line: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ':' if !in_single_quote && !in_double_quote => {
                // Must be followed by space, end of line, or be at end
                let rest = &line[i + 1..];
                if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

struct NodeInfo {
    path: String,
    yaml_type: String,
    value: Option<String>,
}

/// Find the node info (path, type, value) for the token at the cursor.
fn find_node_info(
    doc: &YamlOwned,
    token: &CursorToken,
    line: &str,
    lines: &[&str],
    line_idx: usize,
) -> Option<NodeInfo> {
    // Build the key path by analyzing indentation
    let path = build_key_path(token, line, lines, line_idx);

    // Look up the value in the AST using the path
    let node = resolve_path(doc, &path)?;

    let yaml_type = yaml_type_name(node);
    let value = scalar_value(node);

    Some(NodeInfo {
        path: format_path(&path),
        yaml_type,
        value,
    })
}

/// A segment in a YAML key path.
#[derive(Debug)]
enum PathSegment {
    Key(String),
    Index(usize),
}

/// Build the key path from indentation analysis.
fn build_key_path(
    token: &CursorToken,
    line: &str,
    lines: &[&str],
    line_idx: usize,
) -> Vec<PathSegment> {
    let mut path = Vec::new();

    // Collect parent keys by walking up the indentation
    let current_indent = indentation_level(line);

    // Find parent keys
    let mut parents = Vec::new();
    let mut target_indent = current_indent;

    if target_indent > 0 {
        let mut i = line_idx;
        while i > 0 {
            i -= 1;
            let Some(prev_line) = lines.get(i).copied() else {
                continue;
            };
            let prev_trimmed = prev_line.trim();

            // Skip empty lines, comments, separators
            if prev_trimmed.is_empty()
                || prev_trimmed.starts_with('#')
                || prev_trimmed == "---"
                || prev_trimmed == "..."
            {
                continue;
            }

            let prev_indent = indentation_level(prev_line);
            if prev_indent < target_indent {
                // This is a parent
                if let Some(key) = extract_key(prev_trimmed) {
                    parents.push(key);
                    target_indent = prev_indent;
                    if target_indent == 0 {
                        break;
                    }
                }
            }
        }
    }

    // Reverse parents (we collected bottom-up)
    parents.reverse();
    for key in parents {
        path.push(PathSegment::Key(key));
    }

    // Add current token to path
    match token {
        CursorToken::Key(key) | CursorToken::Value(key) => {
            // For a value, we need the key from the same line
            if matches!(token, CursorToken::Value(_)) {
                if let Some(key) = extract_key(line.trim()) {
                    path.push(PathSegment::Key(key));
                }
            } else {
                path.push(PathSegment::Key(key.clone()));
            }
        }
        CursorToken::SequenceValue => {
            // Find the index within the sequence
            let idx = sequence_index(lines, line_idx);
            path.push(PathSegment::Index(idx));
        }
    }

    path
}

/// Get the indentation level (number of leading spaces) of a line.
fn indentation_level(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Extract the key from a YAML line like "key: value" or "key:".
fn extract_key(trimmed_line: &str) -> Option<String> {
    // Handle sequence item with key: "- key: value"
    let line = trimmed_line.strip_prefix("- ").unwrap_or(trimmed_line);

    if let Some(colon_pos) = find_mapping_colon(line) {
        let key = line[..colon_pos].trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    None
}

/// Find the index of a sequence item by counting preceding siblings.
fn sequence_index(lines: &[&str], line_idx: usize) -> usize {
    let current_indent = lines.get(line_idx).map_or(0, |l| indentation_level(l));
    let mut idx = 0;

    for i in (0..line_idx).rev() {
        let Some(prev_line) = lines.get(i).copied() else {
            continue;
        };
        let prev_trimmed = prev_line.trim();
        let prev_indent = indentation_level(prev_line);

        if prev_trimmed.is_empty() || prev_trimmed.starts_with('#') {
            continue;
        }

        if prev_indent < current_indent {
            break; // Reached parent
        }

        if prev_indent == current_indent && prev_trimmed.starts_with("- ") {
            idx += 1;
        }
    }

    idx
}

/// Resolve a path through the YAML AST to find the target node.
fn resolve_path<'a>(doc: &'a YamlOwned, path: &[PathSegment]) -> Option<&'a YamlOwned> {
    let mut current = doc;

    for segment in path {
        match segment {
            PathSegment::Key(key) => {
                let yaml_key = YamlOwned::Value(ScalarOwned::String(key.clone()));
                match current {
                    YamlOwned::Mapping(map) => {
                        current = map.get(&yaml_key)?;
                    }
                    YamlOwned::Value(_)
                    | YamlOwned::Sequence(_)
                    | YamlOwned::Alias(_)
                    | YamlOwned::BadValue
                    | YamlOwned::Tagged(_, _)
                    | YamlOwned::Representation(_, _, _) => return None,
                }
            }
            PathSegment::Index(idx) => match current {
                YamlOwned::Sequence(arr) => {
                    current = arr.get(*idx)?;
                }
                YamlOwned::Value(_)
                | YamlOwned::Mapping(_)
                | YamlOwned::Alias(_)
                | YamlOwned::BadValue
                | YamlOwned::Tagged(_, _)
                | YamlOwned::Representation(_, _, _) => return None,
            },
        }
    }

    Some(current)
}

/// Get the type name for a YAML node.
fn yaml_type_name(yaml: &YamlOwned) -> String {
    match yaml {
        YamlOwned::Mapping(_) => "mapping".to_string(),
        YamlOwned::Sequence(_) => "sequence".to_string(),
        YamlOwned::Value(_) => "scalar".to_string(),
        YamlOwned::Alias(_) => "alias".to_string(),
        YamlOwned::Tagged(_, _) => "tagged".to_string(),
        YamlOwned::BadValue | YamlOwned::Representation(_, _, _) => "bad value".to_string(),
    }
}

/// Get the scalar value representation for display.
fn scalar_value(yaml: &YamlOwned) -> Option<String> {
    match yaml {
        YamlOwned::Value(ScalarOwned::String(s)) => Some(s.clone()),
        YamlOwned::Value(ScalarOwned::Integer(i)) => Some(i.to_string()),
        YamlOwned::Value(ScalarOwned::FloatingPoint(f)) => Some(f.to_string()),
        YamlOwned::Value(ScalarOwned::Boolean(b)) => Some(b.to_string()),
        YamlOwned::Value(ScalarOwned::Null) => Some("null".to_string()),
        YamlOwned::Mapping(_)
        | YamlOwned::Sequence(_)
        | YamlOwned::Alias(_)
        | YamlOwned::BadValue
        | YamlOwned::Tagged(_, _)
        | YamlOwned::Representation(_, _, _) => None,
    }
}

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

/// Format the hover content as Markdown.
fn format_hover_markdown(path: &str, yaml_type: &str, value: Option<&str>) -> String {
    use std::fmt::Write;
    let mut md = String::new();
    let _ = write!(md, "**Path:** `{path}`\n\n");
    let _ = writeln!(md, "**Type:** {yaml_type}");
    if let Some(val) = value {
        let _ = write!(md, "\n**Value:** `{val}`\n");
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn hover_content(hover: &Hover) -> &str {
        match &hover.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected MarkupContent"),
        }
    }

    fn parse_docs(text: &str) -> Option<Vec<YamlOwned>> {
        use saphyr::LoadableYamlNode;
        YamlOwned::load_from_str(text).ok()
    }

    // Test 1
    #[test]
    fn should_return_hover_for_simple_key() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 0));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("name"), "should contain key path 'name'");
        assert!(
            content.to_lowercase().contains("scalar"),
            "should mention scalar type"
        );
    }

    // Test 2
    #[test]
    fn should_return_hover_for_simple_value() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 6));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("name"), "should contain key path 'name'");
        assert!(
            content.to_lowercase().contains("scalar"),
            "should mention scalar type"
        );
        assert!(content.contains("Alice"), "should contain value 'Alice'");
    }

    // Test 3
    #[test]
    fn should_return_none_for_whitespace() {
        let text = "key: value\n\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(1, 0));

        assert!(result.is_none());
    }

    // Test 4
    #[test]
    fn should_return_none_for_comment() {
        let text = "# comment\nkey: value\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 2));

        assert!(result.is_none());
    }

    // Test 5
    #[test]
    fn should_return_hover_for_nested_key() {
        let text = "server:\n  port: 8080\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(1, 2));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("server.port"),
            "should contain key path 'server.port'"
        );
        assert!(
            content.to_lowercase().contains("scalar"),
            "should mention scalar type"
        );
    }

    // Test 6
    #[test]
    fn should_return_hover_for_deeply_nested_key() {
        let text = "a:\n  b:\n    c: deep\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(2, 4));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("a.b.c"), "should contain key path 'a.b.c'");
        assert!(
            content.to_lowercase().contains("scalar"),
            "should mention scalar type"
        );
    }

    // Test 7
    #[test]
    fn should_return_hover_for_sequence_item() {
        let text = "items:\n  - first\n  - second\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(1, 4));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("items[0]") || content.contains("items.0"),
            "should contain path like 'items[0]' or 'items.0'"
        );
        assert!(content.contains("first"), "should contain value 'first'");
    }

    // Test 8
    #[test]
    fn should_return_hover_for_mapping_value_type() {
        let text = "server:\n  port: 8080\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 0));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("server"),
            "should contain key path 'server'"
        );
        assert!(
            content.to_lowercase().contains("mapping"),
            "should mention mapping type"
        );
    }

    // Test 9
    #[test]
    fn should_return_hover_for_sequence_value_type() {
        let text = "items:\n  - one\n  - two\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 0));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("items"), "should contain key path 'items'");
        assert!(
            content.to_lowercase().contains("sequence"),
            "should mention sequence type"
        );
    }

    // Test 10
    #[test]
    fn should_return_hover_with_scalar_value() {
        let text = "port: 8080\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 6));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(content.contains("8080"), "should contain value '8080'");
    }

    // Test 11
    #[test]
    fn should_format_hover_as_markdown() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 0));

        let hover = result.expect("should return hover");
        match &hover.contents {
            HoverContents::Markup(m) => {
                assert_eq!(m.kind, MarkupKind::Markdown);
            }
            _ => panic!("expected MarkupContent"),
        }
    }

    // Test 12
    #[test]
    fn should_return_none_for_empty_document() {
        let text = "";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 0));

        assert!(result.is_none());
    }

    // Test 13
    #[test]
    fn should_return_none_when_document_failed_to_parse() {
        let text = "key: [bad";
        let result = hover_at(text, None, pos(0, 0));

        assert!(result.is_none());
    }

    // Test 14
    #[test]
    fn should_return_hover_in_multi_document_yaml() {
        let text = "doc1key: value1\n---\ndoc2key: value2\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(2, 0));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.contains("doc2key"),
            "should contain key path 'doc2key'"
        );
    }

    // Test 15
    #[test]
    fn should_return_none_for_position_beyond_document() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(5, 0));

        assert!(result.is_none());
    }

    // Test 16
    #[test]
    fn should_return_none_for_document_separator_line() {
        let text = "key1: value1\n---\nkey2: value2\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(1, 0));

        assert!(result.is_none());
    }

    // Test 17
    #[test]
    fn should_return_hover_for_boolean_value() {
        let text = "enabled: true\n";
        let docs = parse_docs(text);
        let result = hover_at(text, docs.as_ref(), pos(0, 9));

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
        let result = hover_at(text, docs.as_ref(), pos(0, 7));

        let hover = result.expect("should return hover");
        let content = hover_content(&hover);
        assert!(
            content.to_lowercase().contains("scalar") || content.to_lowercase().contains("null"),
            "should mention scalar or null type"
        );
    }
}
