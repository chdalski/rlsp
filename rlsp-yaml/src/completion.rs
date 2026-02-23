use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};
use yaml_rust2::Yaml;

/// Compute completion items for the given YAML text and cursor position.
///
/// Returns an empty list when the document is empty, the AST is unavailable,
/// the position is out of bounds, or the cursor is on a comment or separator.
#[must_use]
pub fn complete_at(
    text: &str,
    documents: Option<&Vec<Yaml>>,
    position: Position,
) -> Vec<CompletionItem> {
    let Some(documents) = documents else {
        return Vec::new();
    };
    if documents.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    let Some(line) = lines.get(line_idx) else {
        return Vec::new();
    };

    // For completion, the cursor is often at or just past the end of the line
    // (where the user is typing). Only reject positions well beyond the line.
    // We clamp to line length for further processing.
    if col_idx > line.len() + 1 {
        return Vec::new();
    }
    let col_idx = col_idx.min(line.len());

    let trimmed = line.trim();

    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Vec::new();
    }

    if trimmed == "---" || trimmed == "..." {
        return Vec::new();
    }

    let current_indent = indentation_level(line);
    let cursor_context = classify_cursor(line, col_idx);

    match cursor_context {
        CursorContext::Key(current_key) => {
            suggest_sibling_keys(&lines, line_idx, current_indent, &current_key)
        }
        CursorContext::Value(key_name) => suggest_values_for_key(&lines, &key_name),
    }
}

/// Whether the cursor is on a key or a value.
enum CursorContext {
    Key(String),
    Value(String),
}

/// Classify where the cursor is on the line: key position or value position.
fn classify_cursor(line: &str, col: usize) -> CursorContext {
    let trimmed = line.trim();

    // Handle sequence item lines: "  - key: value"
    let effective_line = trimmed.strip_prefix("- ").unwrap_or(trimmed);

    if let Some(colon_pos) = find_mapping_colon(line) {
        let key = line[..colon_pos].trim();
        // Strip "- " prefix from key if present
        let key = key.strip_prefix("- ").unwrap_or(key);
        let value_start = colon_pos + 1;
        let value_part = line[value_start..].trim();

        if col > colon_pos && !value_part.is_empty() {
            return CursorContext::Value(key.to_string());
        }
        // Cursor on key side, or value is empty (suggest keys)
        if value_part.is_empty() && col > colon_pos {
            return CursorContext::Value(key.to_string());
        }
        return CursorContext::Key(key.to_string());
    }

    // Sequence item with key-value inside: "- key: value"
    if let Some(colon_pos) = find_mapping_colon(effective_line) {
        let key = effective_line[..colon_pos].trim();
        return CursorContext::Key(key.to_string());
    }

    CursorContext::Key(trimmed.to_string())
}

/// Suggest sibling keys at the same indentation level, excluding the current key.
fn suggest_sibling_keys(
    lines: &[&str],
    current_line: usize,
    current_indent: usize,
    current_key: &str,
) -> Vec<CompletionItem> {
    // Determine if we're inside a sequence item
    let in_sequence = is_in_sequence_item(lines, current_line, current_indent);

    if in_sequence {
        return suggest_keys_for_sequence_item(lines, current_line, current_indent);
    }

    // Collect all sibling keys at the same indent level in the same mapping block
    let sibling_keys = collect_sibling_keys(lines, current_line, current_indent);

    sibling_keys
        .into_iter()
        .filter(|k| k != current_key)
        .map(|k| CompletionItem {
            label: k,
            kind: Some(CompletionItemKind::FIELD),
            ..CompletionItem::default()
        })
        .collect()
}

/// Check if the current line is inside a sequence item (preceded by a "- " at same or parent indent).
fn is_in_sequence_item(lines: &[&str], current_line: usize, current_indent: usize) -> bool {
    // Check if current line starts with "- "
    let current_trimmed = lines.get(current_line).map_or("", |l| l.trim());
    if current_trimmed.starts_with("- ") {
        return true;
    }

    // Walk backwards to find if we're a continuation of a sequence item
    for i in (0..current_line).rev() {
        let prev_line = lines.get(i).map_or("", |l| *l);
        let prev_trimmed = prev_line.trim();

        if prev_trimmed.is_empty() || prev_trimmed.starts_with('#') {
            continue;
        }

        let prev_indent = indentation_level(prev_line);

        if prev_indent < current_indent {
            // Parent level - check if it's a sequence item
            if prev_trimmed.starts_with("- ") {
                return true;
            }
            break;
        }

        if prev_indent == current_indent {
            // Same level - if it's a sequence item start, we're in a sequence
            if prev_trimmed.starts_with("- ") {
                return true;
            }
        }
    }

    false
}

/// Suggest keys for a sequence item by looking at sibling sequence items' keys.
fn suggest_keys_for_sequence_item(
    lines: &[&str],
    current_line: usize,
    current_indent: usize,
) -> Vec<CompletionItem> {
    // Find all keys in the current sequence item
    let current_item_keys = collect_current_sequence_item_keys(lines, current_line, current_indent);

    // Find the start of the sequence (parent with "- " items)
    let sequence_indent = find_sequence_indent(lines, current_line, current_indent);

    // Collect keys from all sibling sequence items
    let mut all_keys: HashSet<String> = HashSet::new();
    collect_all_sequence_item_keys(lines, current_line, sequence_indent, &mut all_keys);

    // Exclude keys already present in current item
    all_keys
        .into_iter()
        .filter(|k| !current_item_keys.contains(k))
        .map(|k| CompletionItem {
            label: k,
            kind: Some(CompletionItemKind::FIELD),
            ..CompletionItem::default()
        })
        .collect()
}

/// Collect keys in the current sequence item.
fn collect_current_sequence_item_keys(
    lines: &[&str],
    current_line: usize,
    current_indent: usize,
) -> HashSet<String> {
    let mut keys = HashSet::new();

    // Find the start of the current sequence item (walk back to the "- " line)
    let item_start = find_current_item_start(lines, current_line, current_indent);

    // The indent for keys within this item
    let key_indent = current_indent;

    // Walk from item_start forward through the current item
    for i in item_start..lines.len() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        // If we've hit a line at a lower indent or a new sequence item at the sequence level, stop
        if i > item_start && indent < key_indent {
            // Check if this is a new sequence item at the same sequence level
            break;
        }
        if i > item_start && indent == key_indent && trimmed.starts_with("- ") {
            break;
        }
        // Also stop if we find a sibling "- " at the item start indent
        if i > item_start {
            let start_line = lines.get(item_start).map_or("", |l| *l);
            let start_indent = indentation_level(start_line);
            if indent == start_indent && trimmed.starts_with("- ") {
                break;
            }
        }

        if let Some(key) = extract_key(trimmed) {
            keys.insert(key);
        }
    }

    keys
}

/// Find the start line of the current sequence item.
fn find_current_item_start(lines: &[&str], current_line: usize, current_indent: usize) -> usize {
    let current_trimmed = lines.get(current_line).map_or("", |l| l.trim());
    if current_trimmed.starts_with("- ") {
        return current_line;
    }

    for i in (0..current_line).rev() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        if indent < current_indent && trimmed.starts_with("- ") {
            return i;
        }
        if indent < current_indent {
            break;
        }
        if indent == current_indent && trimmed.starts_with("- ") {
            return i;
        }
    }

    current_line
}

/// Find the indent level of the sequence (the "- " lines).
fn find_sequence_indent(lines: &[&str], current_line: usize, current_indent: usize) -> usize {
    let current_trimmed = lines.get(current_line).map_or("", |l| l.trim());
    if current_trimmed.starts_with("- ") {
        return current_indent;
    }

    for i in (0..current_line).rev() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        if indent < current_indent && trimmed.starts_with("- ") {
            return indent;
        }
        if indent < current_indent {
            break;
        }
    }

    current_indent
}

/// Collect keys from all sequence items at the given sequence indent.
fn collect_all_sequence_item_keys(
    lines: &[&str],
    current_line: usize,
    sequence_indent: usize,
    all_keys: &mut HashSet<String>,
) {
    // Walk backwards to find the start of the sequence
    let mut seq_start = current_line;
    for i in (0..current_line).rev() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        if indent < sequence_indent {
            break;
        }
        if indent == sequence_indent && trimmed.starts_with("- ") {
            seq_start = i;
        }
    }

    // Walk forward from seq_start collecting keys from all items
    let key_indent = sequence_indent + 2; // keys inside "- " are indented by dash + space
    let mut in_sequence = false;

    for i in seq_start..lines.len() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        if indent < sequence_indent {
            break;
        }

        if indent == sequence_indent && trimmed.starts_with("- ") {
            in_sequence = true;
            // Extract key from "- key: value"
            if let Some(after_dash) = trimmed.strip_prefix("- ")
                && let Some(key) = extract_key(after_dash)
            {
                all_keys.insert(key);
            }
            continue;
        }

        if in_sequence
            && indent >= key_indent
            && let Some(key) = extract_key(trimmed)
        {
            all_keys.insert(key);
        }
    }
}

/// Collect sibling keys at the same indent level in the same mapping block.
fn collect_sibling_keys(lines: &[&str], current_line: usize, current_indent: usize) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();

    // Walk backwards from current line
    for i in (0..current_line).rev() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        if indent < current_indent {
            break;
        }

        if indent == current_indent
            && let Some(key) = extract_key(trimmed)
            && seen.insert(key.clone())
        {
            keys.push(key);
        }
    }

    // Walk forward from current line
    for i in (current_line + 1)..lines.len() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        if indent < current_indent {
            break;
        }

        if indent == current_indent
            && let Some(key) = extract_key(trimmed)
            && seen.insert(key.clone())
        {
            keys.push(key);
        }
    }

    keys
}

/// Suggest values for a key by finding the same key name elsewhere in the document.
fn suggest_values_for_key(lines: &[&str], key_name: &str) -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        let effective = trimmed.strip_prefix("- ").unwrap_or(trimmed);

        if let Some(colon_pos) = find_mapping_colon(effective) {
            let k = effective[..colon_pos].trim();
            if k == key_name {
                let val = effective[colon_pos + 1..].trim();
                if !val.is_empty() && seen.insert(val.to_string()) {
                    items.push(CompletionItem {
                        label: val.to_string(),
                        kind: Some(CompletionItemKind::VALUE),
                        ..CompletionItem::default()
                    });
                }
            }
        }
    }

    items
}

/// Get the indentation level (number of leading spaces) of a line.
fn indentation_level(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Extract the key from a YAML line like "key: value" or "- key: value".
fn extract_key(trimmed_line: &str) -> Option<String> {
    let line = trimmed_line.strip_prefix("- ").unwrap_or(trimmed_line);

    if let Some(colon_pos) = find_mapping_colon(line) {
        let key = line[..colon_pos].trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn parse_docs(text: &str) -> Option<Vec<Yaml>> {
        yaml_rust2::YamlLoader::load_from_str(text).ok()
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    // Test 1
    #[test]
    fn should_suggest_sibling_keys_not_yet_present() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0));

        let labels = labels(&result);
        assert!(
            labels.contains(&"age"),
            "should suggest sibling key 'age', got: {labels:?}"
        );
        assert!(
            !labels.contains(&"name"),
            "should not suggest the key at cursor position"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD)),
            "key completions should have FIELD kind"
        );
    }

    // Test 2
    #[test]
    fn should_not_suggest_keys_already_present_in_mapping() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0));

        let labels = labels(&result);
        assert!(
            !labels.contains(&"name"),
            "should not suggest 'name' which is at the cursor line"
        );
    }

    // Test 3
    #[test]
    fn should_suggest_nested_sibling_keys() {
        let text = "server:\n  host: localhost\n  port: 8080\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 2));

        let labels = labels(&result);
        assert!(
            labels.contains(&"port"),
            "should suggest sibling key 'port', got: {labels:?}"
        );
        assert!(
            !labels.contains(&"server"),
            "should not suggest parent key 'server'"
        );
        assert!(
            !labels.contains(&"host"),
            "should not suggest the key at cursor line"
        );
    }

    // Test 4
    #[test]
    fn should_suggest_keys_from_deeply_nested_mapping() {
        let text = "a:\n  b:\n    c: 1\n    d: 2\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(2, 4));

        let labels = labels(&result);
        assert!(
            labels.contains(&"d"),
            "should suggest sibling key 'd', got: {labels:?}"
        );
        assert!(
            !labels.contains(&"a"),
            "should not suggest ancestor key 'a'"
        );
        assert!(!labels.contains(&"b"), "should not suggest parent key 'b'");
        assert!(
            !labels.contains(&"c"),
            "should not suggest the key at cursor line"
        );
    }

    // Test 5
    #[test]
    fn should_suggest_keys_from_sibling_sequence_items() {
        let text = "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(3, 4));

        let labels = labels(&result);
        assert!(
            labels.contains(&"age"),
            "should suggest 'age' from sibling sequence item, got: {labels:?}"
        );
    }

    // Test 6
    #[test]
    fn should_not_suggest_keys_already_in_current_sequence_item() {
        let text = "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(3, 4));

        let labels = labels(&result);
        assert!(
            !labels.contains(&"name"),
            "should not suggest 'name' already present in current sequence item"
        );
    }

    // Test 7
    #[test]
    fn should_suggest_values_seen_for_same_key_name() {
        let text = "items:\n  - env: production\n  - env: staging\n  - env: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(3, 10));

        let labels = labels(&result);
        assert!(
            labels.contains(&"production"),
            "should suggest value 'production', got: {labels:?}"
        );
        assert!(
            labels.contains(&"staging"),
            "should suggest value 'staging', got: {labels:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "value completions should have VALUE kind"
        );
    }

    // Test 8
    #[test]
    fn should_not_suggest_duplicate_values() {
        let text = "items:\n  - env: production\n  - env: production\n  - env: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(3, 10));

        let labels = labels(&result);
        let production_count = labels.iter().filter(|&&l| l == "production").count();
        assert_eq!(
            production_count, 1,
            "should deduplicate: 'production' should appear only once, got: {labels:?}"
        );
    }

    // Test 9
    #[test]
    fn should_return_empty_for_empty_document() {
        let text = "";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0));

        assert!(result.is_empty(), "should return empty for empty document");
    }

    // Test 10
    #[test]
    fn should_return_empty_when_ast_is_none() {
        let text = "key: [bad";
        let result = complete_at(text, None, pos(0, 0));

        assert!(
            result.is_empty(),
            "should return empty when AST is None (failed parse)"
        );
    }

    // Test 11
    #[test]
    fn should_return_empty_for_comment_line() {
        let text = "# comment\nkey: value\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0));

        assert!(result.is_empty(), "should return empty for comment line");
    }

    // Test 12
    #[test]
    fn should_return_empty_for_document_separator() {
        let text = "key1: v1\n---\nkey2: v2\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 0));

        assert!(
            result.is_empty(),
            "should return empty for document separator"
        );
    }

    // Test 13
    #[test]
    fn should_return_empty_for_position_beyond_document_lines() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(10, 0));

        assert!(
            result.is_empty(),
            "should return empty for position beyond document lines"
        );
    }

    // Test 14
    #[test]
    fn should_return_empty_for_position_beyond_line_length() {
        let text = "key: value\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 100));

        assert!(
            result.is_empty(),
            "should return empty for position beyond line length"
        );
    }

    // Test 15
    #[test]
    fn should_return_empty_for_no_documents() {
        let text = "key: value\n";
        let empty: Vec<Yaml> = Vec::new();
        let result = complete_at(text, Some(&empty), pos(0, 0));

        assert!(
            result.is_empty(),
            "should return empty for empty documents vector"
        );
    }
}
