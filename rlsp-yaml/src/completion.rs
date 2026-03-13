use std::collections::HashSet;

use saphyr::YamlOwned;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind, Position,
};

use crate::schema::{JsonSchema, SchemaType};

// Maximum number of completion items returned.
const MAX_COMPLETION_ITEMS: usize = 100;
// Maximum number of allOf/anyOf/oneOf branches walked for property collection.
const MAX_BRANCH_COUNT: usize = 20;
// Maximum number of Unicode characters in a description shown in documentation.
const MAX_DESCRIPTION_LEN: usize = 200;
// Maximum number of Unicode characters in an enum label.
const MAX_ENUM_LABEL_LEN: usize = 50;

/// Compute completion items for the given YAML text and cursor position.
///
/// When `schema` is provided, schema-defined properties and enum values are
/// merged with structural (document-based) suggestions. Falls back to structural
/// completion when `schema` is `None` or has no relevant properties.
///
/// Returns an empty list when the document is empty, the AST is unavailable,
/// the position is out of bounds, or the cursor is on a comment or separator.
#[must_use]
pub fn complete_at(
    text: &str,
    documents: Option<&Vec<YamlOwned>>,
    position: Position,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
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

    if trimmed.starts_with('#') {
        return Vec::new();
    }

    if trimmed == "---" || trimmed == "..." {
        return Vec::new();
    }

    // Blank line: schema key completions if schema is available, otherwise empty.
    if trimmed.is_empty() {
        if let Some(s) = schema {
            let current_indent = indentation_level(line);
            let path = build_key_path(&lines, line_idx, current_indent);
            let resolved = resolve_schema_path(s, &path);
            if let Some(resolved_schema) = resolved {
                let present = collect_present_keys_at_indent(&lines, line_idx, current_indent);
                return schema_key_completions(resolved_schema, &present);
            }
        }
        return Vec::new();
    }

    // Structural completion requires a parsed AST.
    let Some(documents) = documents else {
        return Vec::new();
    };
    if documents.is_empty() {
        return Vec::new();
    }

    let current_indent = indentation_level(line);
    let cursor_context = classify_cursor(line, col_idx);

    match cursor_context {
        CursorContext::Key(current_key) => {
            let structural = suggest_sibling_keys(&lines, line_idx, current_indent, &current_key);
            if let Some(s) = schema {
                let path = build_key_path(&lines, line_idx, current_indent);
                let resolved = resolve_schema_path(s, &path);
                if let Some(resolved_schema) = resolved
                    && schema_has_properties(resolved_schema)
                {
                    let present = collect_present_keys_at_indent(&lines, line_idx, current_indent);
                    let schema_items = schema_key_completions(resolved_schema, &present);
                    // Filter structural items by the same present-keys set so
                    // schema-defined properties that are already present don't
                    // re-appear through the structural fallback.
                    let filtered_structural: Vec<CompletionItem> = structural
                        .into_iter()
                        .filter(|i| !present.contains(i.label.as_str()))
                        .collect();
                    merge_completions(filtered_structural, schema_items)
                } else {
                    structural
                }
            } else {
                structural
            }
        }
        CursorContext::Value(key_name) => schema.map_or_else(
            || suggest_values_for_key(&lines, &key_name),
            |s| {
                let path = build_value_key_path(&lines, line_idx, current_indent, &key_name);
                resolve_schema_path(s, &path).map_or_else(
                    || suggest_values_for_key(&lines, &key_name),
                    |prop_schema| {
                        let schema_items = schema_value_completions(prop_schema);
                        if schema_items.is_empty() {
                            suggest_values_for_key(&lines, &key_name)
                        } else {
                            schema_items
                        }
                    },
                )
            },
        ),
    }
}

/// Merge structural and schema-sourced key completion items, deduplicating by
/// label and capping at `MAX_COMPLETION_ITEMS`.
fn merge_completions(
    structural: Vec<CompletionItem>,
    schema_items: Vec<CompletionItem>,
) -> Vec<CompletionItem> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result: Vec<CompletionItem> = Vec::new();

    // Schema items first (richer metadata), then structural fallback.
    for item in schema_items.into_iter().chain(structural) {
        if seen.insert(item.label.clone()) {
            result.push(item);
            if result.len() >= MAX_COMPLETION_ITEMS {
                break;
            }
        }
    }
    result
}

/// Build the ancestor key path (from root to the parent of the cursor line).
///
/// Returns the sequence of mapping keys that describe the nesting context of
/// the current line. For a cursor inside `a:\n  b:\n    <cursor>`, this returns
/// `["a", "b"]`. Sequence items are represented as `"[]"` sentinels so that the
/// path walker can descend into `items` schemas.
fn build_key_path(lines: &[&str], cursor_line: usize, cursor_indent: usize) -> Vec<String> {
    let cursor_trimmed = lines.get(cursor_line).map_or("", |l| l.trim());
    let cursor_in_seq = cursor_trimmed.starts_with("- ");

    // If cursor_indent is 0 and not in a sequence item, path is empty (top-level).
    if cursor_indent == 0 && !cursor_in_seq {
        return Vec::new();
    }

    let mut path: Vec<String> = Vec::new();
    let mut target_indent = cursor_indent;

    for i in (0..cursor_line).rev() {
        let line = lines.get(i).map_or("", |l| *l);
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = indentation_level(line);

        if indent >= target_indent {
            continue;
        }

        // This line is at a lower indent — it must be the parent.
        let effective = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(key) = extract_key(effective).or_else(|| extract_key(trimmed)) {
            path.push(key);
        } else if trimmed.starts_with("- ") || trimmed == "-" {
            path.push("[]".to_string());
        }

        target_indent = indent;
        if target_indent == 0 {
            break;
        }
    }

    path.reverse();

    // If the cursor line itself is a sequence item (starts with "- "), the keys
    // within it are inside an array element — append "[]" to descend into `items`.
    if cursor_in_seq {
        path.push("[]".to_string());
    }

    path
}

/// Build the key path for a value position: same as `build_key_path` but
/// appends the key whose value the cursor is on.
fn build_value_key_path(
    lines: &[&str],
    cursor_line: usize,
    cursor_indent: usize,
    key_name: &str,
) -> Vec<String> {
    let mut path = build_key_path(lines, cursor_line, cursor_indent);
    path.push(key_name.to_string());
    path
}

/// Walk the schema tree following `path`, returning the sub-schema at that
/// path if it exists. Returns `None` when the path exceeds the schema depth.
fn resolve_schema_path<'a>(schema: &'a JsonSchema, path: &[String]) -> Option<&'a JsonSchema> {
    let [key, rest @ ..] = path else {
        return Some(schema);
    };

    // Array item descent.
    if key == "[]" {
        if let Some(items) = &schema.items {
            return resolve_schema_path(items, rest);
        }
        return None;
    }

    // Direct property lookup.
    if let Some(Some(prop_schema)) = schema.properties.as_ref().map(|p| p.get(key.as_str())) {
        return resolve_schema_path(prop_schema, rest);
    }

    // Walk composition branches (capped).
    let branches: Vec<&JsonSchema> = [&schema.all_of, &schema.any_of, &schema.one_of]
        .into_iter()
        .flatten()
        .flat_map(|v| v.iter())
        .take(MAX_BRANCH_COUNT)
        .collect();

    for branch in branches {
        if let Some(found) = resolve_schema_path(branch, path) {
            return Some(found);
        }
    }

    None
}

/// Collect keys already present in the document at `cursor_indent`, to exclude
/// them from schema suggestions.
fn collect_present_keys_at_indent(
    lines: &[&str],
    cursor_line: usize,
    cursor_indent: usize,
) -> HashSet<String> {
    let mut keys = HashSet::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = indentation_level(line);
        if indent != cursor_indent {
            continue;
        }
        let effective = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(colon_pos) = find_mapping_colon(effective) {
            let key = effective[..colon_pos].trim();
            let value = effective[colon_pos + 1..].trim();
            if key.is_empty() {
                continue;
            }
            // Always exclude the cursor line's key (the one being typed).
            // For other lines, only exclude keys that have a non-empty value;
            // a key with an empty value is still worth suggesting (the user
            // may want enum hints or type info for it).
            if i == cursor_line || !value.is_empty() {
                keys.insert(key.to_string());
            }
        }
    }
    keys
}

/// Return true if the schema has any properties to suggest (direct or via composition).
fn schema_has_properties(schema: &JsonSchema) -> bool {
    if schema.properties.as_ref().is_some_and(|p| !p.is_empty()) {
        return true;
    }
    for branch_list in [&schema.all_of, &schema.any_of, &schema.one_of]
        .into_iter()
        .flatten()
    {
        if branch_list.iter().any(schema_has_properties) {
            return true;
        }
    }
    false
}

/// Produce key completion items from a resolved schema, excluding already-present keys.
fn schema_key_completions(schema: &JsonSchema, present: &HashSet<String>) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = Vec::new();
    collect_schema_properties(schema, present, &mut items, 0);
    items
}

/// Recursively collect property names from a schema and its composition branches.
fn collect_schema_properties(
    schema: &JsonSchema,
    present: &HashSet<String>,
    items: &mut Vec<CompletionItem>,
    depth: usize,
) {
    if depth >= MAX_BRANCH_COUNT {
        return;
    }

    if let Some(props) = &schema.properties {
        for (key, prop_schema) in props {
            if present.contains(key.as_str()) {
                continue;
            }
            if items.len() >= MAX_COMPLETION_ITEMS {
                return;
            }
            let detail = type_label(prop_schema);
            let documentation = prop_schema.description.as_deref().map(|d| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: truncate_description(d),
                })
            });
            items.push(CompletionItem {
                label: key.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail,
                documentation,
                ..CompletionItem::default()
            });
        }
    }

    // Walk composition branches, capped.
    let branch_lists = [&schema.all_of, &schema.any_of, &schema.one_of];
    let mut branch_count = 0;
    for branch_list in branch_lists.into_iter().flatten() {
        for branch in branch_list {
            if branch_count >= MAX_BRANCH_COUNT {
                return;
            }
            collect_schema_properties(branch, present, items, depth + 1);
            branch_count += 1;
        }
    }
}

/// Produce value completion items from a schema (enum values or boolean type).
fn schema_value_completions(schema: &JsonSchema) -> Vec<CompletionItem> {
    // Enum values take priority.
    if let Some(enum_vals) = &schema.enum_values {
        let detail = type_label(schema);
        return enum_vals
            .iter()
            .filter_map(|v| {
                let label = json_value_to_yaml_label(v)?;
                let label = truncate_enum_label(&label);
                Some(CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::VALUE),
                    detail: detail.clone(),
                    ..CompletionItem::default()
                })
            })
            .collect();
    }

    // Boolean type → suggest "true" and "false".
    if matches!(&schema.schema_type, Some(SchemaType::Single(t)) if t == "boolean") {
        return vec![
            CompletionItem {
                label: "true".to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "false".to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..CompletionItem::default()
            },
        ];
    }

    Vec::new()
}

/// Convert a `serde_json::Value` to a YAML scalar label string.
/// Returns `None` for values that have no natural YAML scalar representation
/// (arrays, objects).
fn json_value_to_yaml_label(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Null => Some("null".to_string()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => None,
    }
}

/// Return the type label string for a schema (e.g. `"string"`, `"integer"`),
/// or `None` if no type is defined.
fn type_label(schema: &JsonSchema) -> Option<String> {
    match &schema.schema_type {
        Some(SchemaType::Single(t)) => Some(t.clone()),
        Some(SchemaType::Multiple(ts)) => Some(ts.join(" | ")),
        None => None,
    }
}

/// Truncate a description so the result (including ellipsis) is at most
/// `MAX_DESCRIPTION_LEN` Unicode characters.
fn truncate_description(desc: &str) -> String {
    if desc.chars().count() <= MAX_DESCRIPTION_LEN {
        return desc.to_string();
    }
    // Keep MAX_DESCRIPTION_LEN-1 chars, then append "…" (1 char) = MAX_DESCRIPTION_LEN total.
    let keep = MAX_DESCRIPTION_LEN - 1;
    let boundary = desc.char_indices().nth(keep).map_or(desc.len(), |(i, _)| i);
    format!("{}…", &desc[..boundary])
}

/// Truncate an enum label so the result (including ellipsis) is at most
/// `MAX_ENUM_LABEL_LEN` Unicode characters.
fn truncate_enum_label(label: &str) -> String {
    if label.chars().count() <= MAX_ENUM_LABEL_LEN {
        return label.to_string();
    }
    // Keep MAX_ENUM_LABEL_LEN-1 chars, then append "…" (1 char) = MAX_ENUM_LABEL_LEN total.
    let keep = MAX_ENUM_LABEL_LEN - 1;
    let boundary = label
        .char_indices()
        .nth(keep)
        .map_or(label.len(), |(i, _)| i);
    format!("{}…", &label[..boundary])
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
    use crate::schema::{JsonSchema, SchemaType};
    use serde_json::json;
    use tower_lsp::lsp_types::Documentation;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn parse_docs(text: &str) -> Option<Vec<YamlOwned>> {
        use saphyr::LoadableYamlNode;
        YamlOwned::load_from_str(text).ok()
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
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

    fn boolean_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            ..JsonSchema::default()
        }
    }

    fn object_schema(props: Vec<(&str, JsonSchema)>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            ..JsonSchema::default()
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Backward-Compatibility Tests (Tests 1–15): None schema
    // ══════════════════════════════════════════════════════════════════════════

    // Test 1
    #[test]
    fn should_suggest_sibling_keys_not_yet_present() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), None);

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
        let result = complete_at(text, docs.as_ref(), pos(0, 0), None);

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
        let result = complete_at(text, docs.as_ref(), pos(1, 2), None);

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
        let result = complete_at(text, docs.as_ref(), pos(2, 4), None);

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
        let result = complete_at(text, docs.as_ref(), pos(3, 4), None);

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
        let result = complete_at(text, docs.as_ref(), pos(3, 4), None);

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
        let result = complete_at(text, docs.as_ref(), pos(3, 10), None);

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
        let result = complete_at(text, docs.as_ref(), pos(3, 10), None);

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
        let result = complete_at(text, docs.as_ref(), pos(0, 0), None);

        assert!(result.is_empty(), "should return empty for empty document");
    }

    // Test 10
    #[test]
    fn should_return_empty_when_ast_is_none() {
        let text = "key: [bad";
        let result = complete_at(text, None, pos(0, 0), None);

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
        let result = complete_at(text, docs.as_ref(), pos(0, 0), None);

        assert!(result.is_empty(), "should return empty for comment line");
    }

    // Test 12
    #[test]
    fn should_return_empty_for_document_separator() {
        let text = "key1: v1\n---\nkey2: v2\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 0), None);

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
        let result = complete_at(text, docs.as_ref(), pos(10, 0), None);

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
        let result = complete_at(text, docs.as_ref(), pos(0, 100), None);

        assert!(
            result.is_empty(),
            "should return empty for position beyond line length"
        );
    }

    // Test 15
    #[test]
    fn should_return_empty_for_no_documents() {
        let text = "key: value\n";
        let empty: Vec<YamlOwned> = Vec::new();
        let result = complete_at(text, Some(&empty), pos(0, 0), None);

        assert!(
            result.is_empty(),
            "should return empty for empty documents vector"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group B — Schema Key Completion at Key Positions
    // ══════════════════════════════════════════════════════════════════════════

    // Test 17
    #[test]
    fn should_suggest_schema_properties_at_top_level_key_position() {
        let schema = object_schema(vec![("name", string_schema()), ("age", integer_schema())]);
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"age"),
            "should suggest schema property 'age', got: {labels:?}"
        );
        assert!(
            !labels.contains(&"name"),
            "should not suggest 'name' which is already present"
        );
        assert!(
            result
                .iter()
                .any(|i| i.kind == Some(CompletionItemKind::FIELD)),
            "schema key completions should have FIELD kind"
        );
    }

    // Test 18
    #[test]
    fn should_include_schema_detail_and_documentation_in_key_suggestion() {
        let schema = object_schema(vec![(
            "name",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                description: Some("The user's name".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let text = "\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let item = result.iter().find(|i| i.label == "name");
        assert!(
            item.is_some(),
            "should suggest 'name', got: {:?}",
            labels(&result)
        );
        let item = item.unwrap();
        assert_eq!(
            item.detail.as_deref(),
            Some("string"),
            "detail should be the type 'string'"
        );
        let has_description = match &item.documentation {
            Some(Documentation::String(s)) => s.contains("The user's name"),
            Some(Documentation::MarkupContent(m)) => m.value.contains("The user's name"),
            None => false,
        };
        assert!(
            has_description,
            "documentation should contain 'The user's name'"
        );
    }

    // Test 19
    #[test]
    fn should_suggest_all_schema_properties_when_mapping_is_empty() {
        let schema = object_schema(vec![
            ("host", JsonSchema::default()),
            ("port", JsonSchema::default()),
            ("timeout", JsonSchema::default()),
        ]);
        let text = "host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(labels.contains(&"port"), "should suggest 'port'");
        assert!(labels.contains(&"timeout"), "should suggest 'timeout'");
        assert!(
            !labels.contains(&"host"),
            "should not suggest 'host' (already present)"
        );
    }

    // Test 20
    #[test]
    fn should_not_suggest_schema_properties_already_in_document() {
        let schema = object_schema(vec![
            ("a", JsonSchema::default()),
            ("b", JsonSchema::default()),
            ("c", JsonSchema::default()),
        ]);
        let text = "a: 1\nb: 2\nc: \n";
        let docs = parse_docs(text);
        // cursor on line 2 ("c:"), key position
        let result = complete_at(text, docs.as_ref(), pos(2, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            !labels.contains(&"a"),
            "should not suggest 'a' (already present)"
        );
        assert!(
            !labels.contains(&"b"),
            "should not suggest 'b' (already present)"
        );
        assert!(
            !labels.contains(&"c"),
            "should not suggest 'c' (current line)"
        );
    }

    // Test 21
    #[test]
    fn should_suggest_schema_properties_for_nested_key_position() {
        let schema = object_schema(vec![(
            "server",
            object_schema(vec![("host", string_schema()), ("port", integer_schema())]),
        )]);
        let text = "server:\n  host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 2), Some(&schema));

        let labels = labels(&result);
        assert!(labels.contains(&"port"), "should suggest nested 'port'");
        assert!(
            !labels.contains(&"host"),
            "should not suggest 'host' (already present)"
        );
        assert!(
            !labels.contains(&"server"),
            "should not suggest parent 'server'"
        );
    }

    // Test 22
    #[test]
    fn should_merge_schema_and_structural_suggestions() {
        let schema = object_schema(vec![("kind", string_schema())]);
        let text = "name: Alice\nkind: \n";
        let docs = parse_docs(text);
        // cursor at key position on line 0 ("name:")
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"kind"),
            "schema property 'kind' should appear"
        );
        assert!(
            !labels.contains(&"name"),
            "current key 'name' should not appear"
        );
    }

    // Test 23
    #[test]
    fn should_deduplicate_when_schema_and_structure_both_suggest_same_key() {
        let schema = object_schema(vec![("env", string_schema())]);
        let text = "env: production\nregion: us-east\n";
        let docs = parse_docs(text);
        // cursor at key position on line 1 ("region:")
        let result = complete_at(text, docs.as_ref(), pos(1, 0), Some(&schema));

        let labels = labels(&result);
        let env_count = labels.iter().filter(|&&l| l == "env").count();
        assert!(
            env_count <= 1,
            "'env' should appear at most once, got: {labels:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group C — Schema Enum Completion at Value Positions
    // ══════════════════════════════════════════════════════════════════════════

    // Test 24
    #[test]
    fn should_suggest_enum_values_at_value_position() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![
                    json!("production"),
                    json!("staging"),
                    json!("development"),
                ]),
                ..JsonSchema::default()
            },
        )]);
        let text = "env: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 5), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"production"),
            "should suggest 'production'"
        );
        assert!(labels.contains(&"staging"), "should suggest 'staging'");
        assert!(
            labels.contains(&"development"),
            "should suggest 'development'"
        );
        assert!(
            result
                .iter()
                .any(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "enum completions should have VALUE kind"
        );
    }

    // Test 25
    #[test]
    fn should_include_schema_detail_in_enum_suggestion() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                enum_values: Some(vec![json!("prod"), json!("dev")]),
                description: Some("Deployment target".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let text = "env: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 5), Some(&schema));

        assert!(!result.is_empty(), "should have enum suggestions");
        assert!(
            result
                .iter()
                .any(|i| i.detail.as_deref().is_some_and(|d| d.contains("string"))),
            "at least one suggestion should have detail containing 'string'"
        );
    }

    // Test 26
    #[test]
    fn should_not_duplicate_enum_value_already_used_in_same_key() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("production"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let text = "env: production\nenv: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 5), Some(&schema));

        let labels = labels(&result);
        let prod_count = labels.iter().filter(|&&l| l == "production").count();
        assert!(prod_count <= 1, "'production' should appear at most once");
    }

    // Test 27
    #[test]
    fn should_fall_back_to_structural_value_suggestions_when_no_schema_enum() {
        let schema = object_schema(vec![("env", string_schema())]);
        let text = "env: production\nenv: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 5), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"production"),
            "structural value 'production' should still appear as fallback"
        );
    }

    // Test 28
    #[test]
    fn should_suggest_boolean_values_for_boolean_schema_type() {
        let schema = object_schema(vec![("enabled", boolean_schema())]);
        let text = "enabled: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 9), Some(&schema));

        let labels = labels(&result);
        assert!(labels.contains(&"true"), "should suggest 'true'");
        assert!(labels.contains(&"false"), "should suggest 'false'");
        assert!(
            result
                .iter()
                .any(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "boolean completions should have VALUE kind"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group D — Path Resolution
    // ══════════════════════════════════════════════════════════════════════════

    // Test 29
    #[test]
    fn should_resolve_nested_path_for_schema_property_completion() {
        let schema = object_schema(vec![(
            "database",
            object_schema(vec![("host", string_schema()), ("port", integer_schema())]),
        )]);
        let text = "database:\n  host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 2), Some(&schema));

        let labels = labels(&result);
        assert!(labels.contains(&"port"), "should suggest nested 'port'");
        assert!(
            !labels.contains(&"database"),
            "should not suggest parent 'database'"
        );
    }

    // Test 30
    #[test]
    fn should_resolve_array_items_schema_for_key_completion() {
        let schema = object_schema(vec![(
            "servers",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(object_schema(vec![
                    ("host", string_schema()),
                    ("port", integer_schema()),
                ]))),
                ..JsonSchema::default()
            },
        )]);
        let text = "servers:\n  - host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 4), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"port"),
            "should suggest 'port' from items schema"
        );
    }

    // Test 31
    #[test]
    fn should_resolve_path_to_third_level_nesting() {
        let schema = object_schema(vec![(
            "a",
            object_schema(vec![(
                "b",
                object_schema(vec![("c", string_schema()), ("d", integer_schema())]),
            )]),
        )]);
        let text = "a:\n  b:\n    c: v\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(2, 4), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"d"),
            "should suggest deep schema property 'd'"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group E — Composition Schemas
    // ══════════════════════════════════════════════════════════════════════════

    // Test 32
    #[test]
    fn should_suggest_properties_from_allof_branches() {
        let schema = JsonSchema {
            all_of: Some(vec![
                object_schema(vec![("name", string_schema())]),
                object_schema(vec![("age", integer_schema())]),
            ]),
            ..JsonSchema::default()
        };
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"age"),
            "should suggest 'age' merged from allOf branches"
        );
    }

    // Test 33
    #[test]
    fn should_suggest_properties_from_anyof_branches() {
        let schema = JsonSchema {
            any_of: Some(vec![
                object_schema(vec![("host", string_schema())]),
                object_schema(vec![("socket", string_schema())]),
            ]),
            ..JsonSchema::default()
        };
        let text = "host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"socket"),
            "should suggest 'socket' merged from anyOf branches"
        );
    }

    // Test 34
    #[test]
    fn should_suggest_properties_from_oneof_branches() {
        let schema = JsonSchema {
            one_of: Some(vec![
                object_schema(vec![("url", string_schema())]),
                object_schema(vec![("path", string_schema())]),
            ]),
            ..JsonSchema::default()
        };
        let text = "url: http://example.com\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"path"),
            "should suggest 'path' from oneOf branches"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group F — Fallback Behavior
    // ══════════════════════════════════════════════════════════════════════════

    // Test 35
    #[test]
    fn should_fall_back_to_structural_completion_when_schema_is_none() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), None);

        let labels = labels(&result);
        assert!(
            labels.contains(&"age"),
            "structural sibling 'age' should appear when schema is None"
        );
    }

    // Test 36
    #[test]
    fn should_fall_back_to_structural_when_schema_has_no_properties() {
        let schema = JsonSchema::default();
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"age"),
            "structural sibling 'age' should appear when schema has no properties"
        );
    }

    // Test 37
    #[test]
    fn should_offer_schema_property_when_structural_has_no_siblings() {
        // Schema has "unrelated"; document only has "name" (no siblings for structural).
        let schema = object_schema(vec![("unrelated", JsonSchema::default())]);
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        // cursor at key position on the only key "name"; no structural siblings, but schema
        // offers "unrelated"
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"unrelated"),
            "schema property 'unrelated' should appear even when no structural siblings"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group G — Edge Cases
    // ══════════════════════════════════════════════════════════════════════════

    // Test 38
    #[test]
    fn should_return_empty_for_schema_completion_on_empty_document() {
        let schema = object_schema(vec![("name", string_schema())]);
        let docs: Option<Vec<YamlOwned>> = None;
        let result = complete_at("", docs.as_ref(), pos(0, 0), Some(&schema));

        assert!(result.is_empty(), "should return empty for empty document");
    }

    // Test 39
    #[test]
    fn should_return_empty_for_schema_completion_on_comment_line() {
        let schema = object_schema(vec![("name", string_schema())]);
        let text = "# comment\nkey: value\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        assert!(result.is_empty(), "should return empty for comment line");
    }

    // Test 40
    #[test]
    fn should_return_empty_for_schema_completion_on_document_separator() {
        let schema = object_schema(vec![("name", string_schema())]);
        let text = "key1: v1\n---\nkey2: v2\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(1, 0), Some(&schema));

        assert!(
            result.is_empty(),
            "should return empty for document separator"
        );
    }

    // Test 41
    #[test]
    fn should_handle_schema_property_with_no_type_gracefully() {
        let schema = object_schema(vec![("data", JsonSchema::default())]);
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let item = result.iter().find(|i| i.label == "data");
        assert!(item.is_some(), "should suggest 'data' without panicking");
        // detail may be None or empty — no type to show
        let item = item.unwrap();
        if let Some(detail) = &item.detail {
            assert!(
                detail.is_empty(),
                "detail should be empty when schema has no type, got: {detail:?}"
            );
        }
    }

    // Test 42
    #[test]
    fn should_handle_enum_completion_with_partial_value_at_cursor() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("production"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        // Cursor after "pro" — value position with partial input
        let text = "env: pro\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 8), Some(&schema));

        let labels = labels(&result);
        // LSP filtering is client-side; server should return all enum options
        assert!(
            labels.contains(&"production") || labels.contains(&"staging"),
            "should return enum suggestions even with partial value at cursor"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security Tests (Tests 43–50)
    // ══════════════════════════════════════════════════════════════════════════

    // Test 43 — description truncated at 200 Unicode chars
    #[test]
    fn should_truncate_description_at_200_chars_in_completion_documentation() {
        let long_desc = "x".repeat(500);
        let schema = object_schema(vec![(
            "name",
            JsonSchema {
                description: Some(long_desc),
                ..JsonSchema::default()
            },
        )]);
        let text = "age: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        let item = result.iter().find(|i| i.label == "name");
        assert!(item.is_some(), "should suggest 'name'");
        if let Some(item) = item {
            let doc_char_count = match &item.documentation {
                Some(Documentation::String(s)) => s.chars().count(),
                Some(Documentation::MarkupContent(m)) => m.value.chars().count(),
                None => 0,
            };
            assert!(
                doc_char_count <= 200,
                "documentation should be truncated to 200 chars, got {}",
                doc_char_count
            );
        }
    }

    // Test 44 — item count cap at 100
    #[test]
    fn should_cap_completion_items_at_100_when_schema_has_many_properties() {
        let properties: std::collections::HashMap<String, JsonSchema> = (0..150)
            .map(|i| (format!("prop_{i:03}"), JsonSchema::default()))
            .collect();
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(properties),
            ..JsonSchema::default()
        };
        let text = "prop_000: x\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        assert!(
            result.len() <= 100,
            "completion items should be capped at 100, got {}",
            result.len()
        );
    }

    // Test 45 — allOf branch walking capped at MAX_BRANCH_COUNT (20)
    #[test]
    fn should_cap_allof_branch_walking_at_max_branch_count() {
        // 30 branches — only MAX_BRANCH_COUNT (20) should be walked
        let branches: Vec<JsonSchema> = (0..30)
            .map(|i| JsonSchema {
                properties: Some(
                    std::iter::once((format!("field_{i}"), JsonSchema::default())).collect(),
                ),
                ..JsonSchema::default()
            })
            .collect();
        let schema = JsonSchema {
            all_of: Some(branches),
            ..JsonSchema::default()
        };
        let text = "irrelevant: x\n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 0), Some(&schema));

        // At most 20 branches walked → at most 20 distinct schema-sourced properties
        let schema_prop_count = result
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::FIELD))
            .count();
        assert!(
            schema_prop_count <= 20,
            "at most 20 allOf branches should be walked, got {} schema props",
            schema_prop_count
        );
    }

    // Test 46 — enum labels truncated at 50 chars
    #[test]
    fn should_truncate_long_enum_labels_at_50_chars() {
        let long_val = "a".repeat(60);
        let schema = object_schema(vec![(
            "key",
            JsonSchema {
                enum_values: Some(vec![json!(long_val)]),
                ..JsonSchema::default()
            },
        )]);
        let text = "key: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 5), Some(&schema));

        assert!(!result.is_empty(), "should have enum suggestion");
        for item in &result {
            assert!(
                item.label.chars().count() <= 50,
                "enum label should be truncated to 50 chars, got {} chars: {}",
                item.label.chars().count(),
                item.label
            );
        }
    }

    // Test 47 — JSON boolean enum values produce YAML scalar labels "true"/"false"
    #[test]
    fn should_convert_json_boolean_enum_to_yaml_scalar_true_false() {
        let schema = object_schema(vec![(
            "enabled",
            JsonSchema {
                enum_values: Some(vec![json!(true), json!(false)]),
                ..JsonSchema::default()
            },
        )]);
        let text = "enabled: \n";
        let docs = parse_docs(text);
        let result = complete_at(text, docs.as_ref(), pos(0, 9), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"true"),
            "JSON boolean true should produce label 'true', got: {labels:?}"
        );
        assert!(
            labels.contains(&"false"),
            "JSON boolean false should produce label 'false', got: {labels:?}"
        );
        assert!(
            !labels.contains(&"\"true\""),
            "should not produce JSON-quoted string '\"true\"'"
        );
        assert!(
            !labels.contains(&"\"false\""),
            "should not produce JSON-quoted string '\"false\"'"
        );
    }

    // Test 48 — path depth exceeds schema depth: graceful bail, no panic
    #[test]
    fn should_return_no_schema_context_when_yaml_path_exceeds_schema_depth() {
        // Schema is only 2 levels deep; YAML cursor is 5 levels deep.
        // The path walker runs out of properties before reaching the cursor — must bail cleanly.
        let schema = object_schema(vec![("a", object_schema(vec![("b", string_schema())]))]);
        let text = "a:\n  b:\n    c:\n      d:\n        e: v\n";
        let docs = parse_docs(text);
        // Must not panic or hang regardless of result
        let _result = complete_at(text, docs.as_ref(), pos(4, 8), Some(&schema));
    }

    // Test 49 — already-present keys excluded from schema suggestions
    #[test]
    fn should_exclude_already_present_keys_from_schema_suggestions() {
        let schema = object_schema(vec![
            ("a", JsonSchema::default()),
            ("b", JsonSchema::default()),
            ("c", JsonSchema::default()),
        ]);
        let text = "a: 1\nb: 2\n";
        let docs = parse_docs(text);
        // cursor on a new blank line at indent 0, key position after "b"
        let result = complete_at(text, docs.as_ref(), pos(1, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            !labels.contains(&"a"),
            "'a' is already present, should not appear"
        );
        assert!(
            !labels.contains(&"b"),
            "'b' is on cursor line, should not appear"
        );
        assert!(labels.contains(&"c"), "'c' is not present, should appear");
    }

    // Test 50 — lock ordering: document_store released before schema_associations
    #[test]
    #[ignore]
    fn lock_ordering_document_store_released_before_schema_associations() {
        // Structural/code-review enforcement test — not executable at unit test level.
        //
        // The completion handler in server.rs must acquire locks in this order:
        //   document_store → schema_associations → schema_cache
        // Each lock must be fully released (guard dropped) before the next is acquired.
        // No std::sync::Mutex guard may be held across an .await point.
        // Enforcement: code review + the lock ordering comment on the Backend struct.
    }
}
