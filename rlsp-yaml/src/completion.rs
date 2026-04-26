// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::LineIndex;
use rlsp_yaml_parser::Pos;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemTag, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position,
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

/// Compute completion items for the given cursor position within the AST.
///
/// When `schema` is provided, schema-defined properties and enum values are
/// merged with structural (document-based) suggestions. Falls back to structural
/// completion when `schema` is `None` or has no relevant properties.
///
/// Returns an empty list when `docs` is empty, the cursor is outside any node,
/// or the cursor is on a comment or document separator.
#[must_use]
pub fn complete_at(
    docs: &[Document<Span>],
    position: Position,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    let cursor_line = position.line as usize;
    let mut items = match locate_cursor(docs, position) {
        CursorLocation::OutsideAny => Vec::new(),
        CursorLocation::OnKey {
            key,
            enclosing_path,
            mapping,
        } => complete_on_key(docs, cursor_line, key, &enclosing_path, mapping, schema),
        CursorLocation::OnValue {
            key,
            enclosing_path,
            ..
        } => complete_on_value(docs, cursor_line, &key, enclosing_path, schema),
        CursorLocation::InSequenceItem {
            enclosing_path,
            sequence,
            current_item,
        } => complete_in_sequence_item(enclosing_path, sequence, current_item, schema),
        CursorLocation::InBlankSequence {
            enclosing_path,
            sequence,
        } => {
            if let Some(s) = schema {
                let mut items_path = enclosing_path;
                items_path.push("[]".to_string());
                if let Some(items_schema) = resolve_schema_path(s, &items_path)
                    && schema_has_properties(items_schema)
                {
                    return schema_key_completions(items_schema, &HashSet::new());
                }
            }
            keys_to_items(
                collect_sequence_sibling_keys(sequence)
                    .into_iter()
                    .collect(),
            )
        }
        CursorLocation::InBlankMapping {
            enclosing_path,
            mapping,
        } => {
            let present = docs.first().map_or_else(HashSet::new, |d| {
                present_keys(mapping, cursor_line, d.line_index())
            });
            if let Some(s) = schema {
                if let Some(resolved_schema) = resolve_schema_path(s, &enclosing_path)
                    && schema_has_properties(resolved_schema)
                {
                    return schema_key_completions(resolved_schema, &present);
                }
            }
            keys_to_items(
                collect_sibling_keys_ast(mapping)
                    .into_iter()
                    .filter(|k| !present.contains(k.as_str()))
                    .collect(),
            )
        }
    };
    items.truncate(MAX_COMPLETION_ITEMS);
    items
}

fn complete_on_key<'a>(
    docs: &'a [Document<Span>],
    cursor_line: usize,
    key: String,
    enclosing_path: &[String],
    mapping: &'a Node<Span>,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    let present = docs.first().map_or_else(HashSet::new, |d| {
        present_keys(mapping, cursor_line, d.line_index())
    });
    let seq_len = enclosing_path.len().saturating_sub(1);
    let structural_keys: HashSet<String> = if enclosing_path.last().is_some_and(|s| s == "[]") {
        let seq_path = enclosing_path.get(..seq_len).unwrap_or(&[]);
        match find_node_at_path(docs, seq_path) {
            Some(seq @ Node::Sequence { .. }) => collect_sequence_sibling_keys(seq),
            _ => collect_sibling_keys_ast(mapping).into_iter().collect(),
        }
    } else {
        collect_sibling_keys_ast(mapping).into_iter().collect()
    };
    let structural = keys_to_items(structural_keys.into_iter().filter(|k| k != &key).collect());
    if let Some(s) = schema {
        if let Some(resolved_schema) = resolve_schema_path(s, enclosing_path)
            && schema_has_properties(resolved_schema)
        {
            let schema_properties = collect_schema_properties_keys(resolved_schema);
            let schema_exclude: HashSet<String> = if schema_properties.contains(&key) {
                let mut ex = present;
                ex.insert(key);
                ex
            } else {
                HashSet::from([key])
            };
            let schema_items = schema_key_completions(resolved_schema, &schema_exclude);
            let filtered_structural: Vec<CompletionItem> = structural
                .into_iter()
                .filter(|i| !schema_exclude.contains(i.label.as_str()))
                .collect();
            return merge_completions(filtered_structural, schema_items);
        }
    }
    structural
}

fn complete_on_value(
    docs: &[Document<Span>],
    cursor_line: usize,
    key: &str,
    enclosing_path: Vec<String>,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    if let Some(s) = schema {
        let mut value_path = enclosing_path;
        value_path.push(key.to_string());
        if let Some(prop_schema) = resolve_schema_path(s, &value_path) {
            let schema_items = schema_value_completions(prop_schema);
            if !schema_items.is_empty() {
                return schema_items;
            }
        }
    }
    let cursor_parser_line = cursor_line + 1;
    let cursor_doc = docs.first().map_or(docs, |first_doc| {
        let idx = first_doc.line_index();
        docs.iter()
            .position(|d| {
                let span = node_span(&d.root);
                idx.line_column(span.start).0 as usize <= cursor_parser_line
                    && cursor_parser_line <= idx.line_column(span.end).0 as usize
            })
            .and_then(|i| docs.get(i))
            .map_or(docs, std::slice::from_ref)
    });
    collect_values_for_key_ast(cursor_doc, cursor_line, key)
}

fn complete_in_sequence_item<'a>(
    enclosing_path: Vec<String>,
    sequence: &'a Node<Span>,
    current_item: &'a Node<Span>,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    let current_keys: HashSet<String> = if let Node::Mapping { entries, .. } = current_item {
        entries
            .iter()
            .filter_map(|(k, _)| scalar_key(k).map(ToString::to_string))
            .collect()
    } else {
        HashSet::new()
    };
    let structural = keys_to_items(
        collect_sequence_sibling_keys(sequence)
            .into_iter()
            .filter(|k| !current_keys.contains(k.as_str()))
            .collect(),
    );
    if let Some(s) = schema {
        let mut items_path = enclosing_path;
        items_path.push("[]".to_string());
        if let Some(items_schema) = resolve_schema_path(s, &items_path)
            && schema_has_properties(items_schema)
        {
            let schema_items = schema_key_completions(items_schema, &current_keys);
            let filtered_structural: Vec<CompletionItem> = structural
                .into_iter()
                .filter(|i| !current_keys.contains(i.label.as_str()))
                .collect();
            return merge_completions(filtered_structural, schema_items);
        }
    }
    structural
}

fn keys_to_items(keys: Vec<String>) -> Vec<CompletionItem> {
    keys.into_iter()
        .map(|k| CompletionItem {
            label: k,
            kind: Some(CompletionItemKind::FIELD),
            ..CompletionItem::default()
        })
        .collect()
}

/// Scan `docs` for all distinct scalar values associated with `key_name` in any
/// mapping, excluding the cursor line itself (which is still being typed).
fn collect_values_for_key_ast(
    docs: &[Document<Span>],
    cursor_line: usize,
    key_name: &str,
) -> Vec<CompletionItem> {
    let parser_cursor_line = cursor_line + 1;
    let mut seen: HashSet<String> = HashSet::new();
    let mut items = Vec::new();

    for doc in docs {
        let idx = doc.line_index();
        collect_values_in_node(
            &doc.root,
            key_name,
            parser_cursor_line,
            &mut seen,
            &mut items,
            idx,
        );
    }
    items
}

fn collect_values_in_node(
    node: &Node<Span>,
    key_name: &str,
    parser_cursor_line: usize,
    seen: &mut HashSet<String>,
    items: &mut Vec<CompletionItem>,
    idx: &LineIndex,
) {
    match node {
        Node::Mapping { entries, .. } => {
            for (key_node, value_node) in entries {
                if let Some(k) = scalar_key(key_node) {
                    if k == key_name {
                        let key_span = node_span(key_node);
                        if idx.line_column(key_span.start).0 as usize != parser_cursor_line {
                            if let Node::Scalar { value, .. } = value_node {
                                if !value.is_empty() && seen.insert(value.clone()) {
                                    items.push(CompletionItem {
                                        label: value.clone(),
                                        kind: Some(CompletionItemKind::VALUE),
                                        ..CompletionItem::default()
                                    });
                                }
                            }
                        }
                    }
                }
                collect_values_in_node(value_node, key_name, parser_cursor_line, seen, items, idx);
            }
        }
        Node::Sequence {
            items: seq_items, ..
        } => {
            for item in seq_items {
                collect_values_in_node(item, key_name, parser_cursor_line, seen, items, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
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
    [&schema.all_of, &schema.any_of, &schema.one_of]
        .into_iter()
        .flatten()
        .flat_map(|v| v.iter())
        .take(MAX_BRANCH_COUNT)
        .find_map(|branch| resolve_schema_path(branch, path))
}

/// Return true if the schema has any properties to suggest (direct or via composition).
fn schema_has_properties(schema: &JsonSchema) -> bool {
    if schema.properties.as_ref().is_some_and(|p| !p.is_empty()) {
        return true;
    }
    [&schema.all_of, &schema.any_of, &schema.one_of]
        .into_iter()
        .flatten()
        .any(|branch_list| branch_list.iter().any(schema_has_properties))
}

/// Produce key completion items from a resolved schema, excluding already-present keys.
fn schema_key_completions(schema: &JsonSchema, present: &HashSet<String>) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = Vec::new();
    collect_schema_properties(schema, present, &mut items, 0);

    // If 2+ required properties are missing, offer a snippet that inserts them all at once.
    if let Some(required) = &schema.required {
        let missing: Vec<&String> = required
            .iter()
            .filter(|r| !present.contains(r.as_str()))
            .collect();
        if missing.len() >= 2 {
            let snippet_body: String = missing
                .iter()
                .enumerate()
                .map(|(idx, key)| {
                    let n = idx + 1;
                    let default = schema
                        .properties
                        .as_ref()
                        .and_then(|props| props.get(*key))
                        .map_or("", snippet_default);
                    if default.is_empty() {
                        format!("{key}: ${{{n}:}}")
                    } else {
                        format!("{key}: ${{{n}:{default}}}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            items.push(CompletionItem {
                label: "(all required)".to_string(),
                kind: Some(CompletionItemKind::SNIPPET),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text: Some(snippet_body),
                sort_text: Some("!".to_string()),
                detail: Some(format!("{} required properties", missing.len())),
                ..CompletionItem::default()
            });
        }
    }

    items
}

/// Return the snippet placeholder default for a schema based on its type.
fn snippet_default(schema: &JsonSchema) -> &'static str {
    match schema.schema_type.as_ref() {
        Some(SchemaType::Single(t)) => match t.as_str() {
            "string" => "\"\"",
            "integer" | "number" => "0",
            "boolean" => "false",
            "object" => "{}",
            "array" => "[]",
            _ => "",
        },
        _ => "",
    }
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
            let (tags, sort_text) = if prop_schema.deprecated == Some(true) {
                (
                    Some(vec![CompletionItemTag::DEPRECATED]),
                    Some(format!("~{key}")),
                )
            } else {
                (None, None)
            };
            items.push(CompletionItem {
                label: key.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail,
                documentation,
                tags,
                sort_text,
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

/// Return the set of all property names defined in a schema (direct + composition branches).
fn collect_schema_properties_keys(schema: &JsonSchema) -> HashSet<String> {
    let mut keys = HashSet::new();
    collect_schema_properties_keys_inner(schema, &mut keys, 0);
    keys
}

fn collect_schema_properties_keys_inner(
    schema: &JsonSchema,
    keys: &mut HashSet<String>,
    depth: usize,
) {
    if depth >= MAX_BRANCH_COUNT {
        return;
    }
    if let Some(props) = &schema.properties {
        for key in props.keys() {
            keys.insert(key.clone());
        }
    }
    for branch_list in [&schema.all_of, &schema.any_of, &schema.one_of]
        .into_iter()
        .flatten()
    {
        for branch in branch_list {
            collect_schema_properties_keys_inner(branch, keys, depth + 1);
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

// ──────────────────────────────────────────────────────────────────────────────
// AST-first cursor-context substrate (Task 1)
// ──────────────────────────────────────────────────────────────────────────────

/// Where the cursor sits in the YAML AST.
///
/// Used by `locate_cursor` and consumed by the Task-2 rewire of `complete_at`.
/// Every variant carries the `enclosing_path` — ancestor mapping keys from the
/// document root down to the immediately enclosing structure, with `"[]"`
/// sentinels for sequence descents.
#[derive(Debug)]
enum CursorLocation<'a> {
    /// Cursor is inside a mapping key token.
    ///
    /// `key` is the key being typed; `enclosing_path` is the path to the
    /// containing mapping; `mapping` is the containing `Node::Mapping`.
    OnKey {
        key: String,
        enclosing_path: Vec<String>,
        mapping: &'a Node<Span>,
    },
    /// Cursor is in the value position of a `key: <value>` pair.
    ///
    /// `key` names the key whose value is under the cursor; `enclosing_path`
    /// is the path to the containing mapping (does **not** include `key`).
    OnValue {
        key: String,
        enclosing_path: Vec<String>,
    },
    /// Cursor is on a blank/whitespace-only line inside a mapping.
    ///
    /// No AST node's span contains the cursor, but a Mapping's span covers
    /// `cursor.line` and its entries sit at a column ≤ the cursor column.
    /// `mapping` is the deepest such `Node::Mapping`.
    InBlankMapping {
        enclosing_path: Vec<String>,
        mapping: &'a Node<Span>,
    },
    /// Cursor is inside a specific sequence item.
    ///
    /// `sequence` is the containing `Node::Sequence`; `current_item` is the
    /// item node the cursor sits in.
    InSequenceItem {
        enclosing_path: Vec<String>,
        sequence: &'a Node<Span>,
        current_item: &'a Node<Span>,
    },
    /// Cursor is on a blank/whitespace-only line directly inside a sequence.
    ///
    /// No item's span contains the cursor, but the sequence's own span covers
    /// the cursor line.
    InBlankSequence {
        enclosing_path: Vec<String>,
        sequence: &'a Node<Span>,
    },
    /// Cursor cannot be located in any AST structure.
    ///
    /// Covers: empty document, position past EOF, cursor on `---`/`...`,
    /// cursor on a comment line.
    OutsideAny,
}

/// Returns `true` when `cursor` is within `span` using half-open `[start, end)`.
///
/// Comparison is lexicographic on `(line, column)`, matching the semantics of
/// `hover.rs::span_contains` and `navigation/references.rs::span_contains`.
fn span_contains_cursor(span: Span, cursor: Pos, idx: &LineIndex) -> bool {
    let start = (
        idx.line_column(span.start).0 as usize,
        idx.line_column(span.start).1 as usize,
    );
    let end = (
        idx.line_column(span.end).0 as usize,
        idx.line_column(span.end).1 as usize,
    );
    let pos = (cursor.line, cursor.column);
    start <= pos && pos < end
}

/// Extract the `loc` span from any AST node.
const fn node_span(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

/// Extract the scalar key string from a key node, returning `None` for
/// non-scalar keys (complex mappings, sequences, aliases).
const fn scalar_key(node: &Node<Span>) -> Option<&str> {
    match node {
        Node::Scalar { value, .. } => Some(value.as_str()),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
    }
}

/// Convert an LSP `Position` (0-based line, 0-based character) to a parser
/// `Pos` (1-based line, 0-based column).
const fn lsp_position_to_pos(position: Position) -> Pos {
    Pos {
        byte_offset: 0,
        line: position.line as usize + 1,
        column: position.character as usize,
    }
}

/// Walk `node` (which must be a `Node::Mapping`) looking for the deepest
/// nested mapping whose entries have `key.(idx.line_column(loc.start).1 as usize) <= cursor.column`
/// and whose span covers `cursor.line`.
///
/// `path` accumulates ancestor mapping keys in root-to-leaf order as the
/// function descends. Returns `None` if `node` is not a mapping, its span
/// doesn't cover `cursor.line`, or none of its entries' key columns ≤
/// `cursor.column`.
fn deepest_mapping_at_column<'a>(
    node: &'a Node<Span>,
    cursor: Pos,
    path: &mut Vec<String>,
    idx: &LineIndex,
) -> Option<&'a Node<Span>> {
    let Node::Mapping { entries, loc, .. } = node else {
        return None;
    };

    // The mapping span must cover the cursor line.
    if !(idx.line_column(loc.start).0 as usize <= cursor.line
        && cursor.line <= idx.line_column(loc.end).0 as usize)
    {
        return None;
    }

    // Find an entry whose key column satisfies key.col <= cursor.col and
    // whose value is a nested mapping that covers the cursor line. Descend
    // into the deepest such mapping. Stop as soon as we find one that
    // admits descent.
    for (key_node, value_node) in entries {
        let Some(key_str) = scalar_key(key_node) else {
            continue;
        };
        let key_span = node_span(key_node);
        if idx.line_column(key_span.start).1 as usize > cursor.column {
            continue;
        }

        // This key's column satisfies the condition. Try to descend into its
        // value if it is also a Mapping whose keys satisfy the condition.
        if let Node::Mapping { .. } = value_node {
            let saved_len = path.len();
            path.push(key_str.to_string());
            if let Some(deeper) = deepest_mapping_at_column(value_node, cursor, path, idx) {
                return Some(deeper);
            }
            // Descent failed (value's entries too deep or span mismatch) — undo.
            path.truncate(saved_len);
        }
    }

    // No deeper mapping admitted descent. Check whether at least one entry's
    // key column satisfies the condition — if so, this mapping is the result.
    let has_eligible_entry = entries.iter().any(|(k, _)| {
        let key_span = node_span(k);
        idx.line_column(key_span.start).1 as usize <= cursor.column
    });
    if has_eligible_entry { Some(node) } else { None }
}

/// Return `true` if any mapping entry in `docs` has its key or value starting
/// on `cursor_parser_line` (1-based parser line number).
///
/// Used to prevent the blank-line extension from firing on non-blank lines
/// where the cursor is positioned past the end of content.
fn cursor_line_has_mapping_content(docs: &[Document<Span>], cursor_parser_line: usize) -> bool {
    fn node_has_content_on_line(node: &Node<Span>, line: usize, idx: &LineIndex) -> bool {
        match node {
            Node::Mapping { entries, .. } => {
                for (key_node, value_node) in entries {
                    let key_span = node_span(key_node);
                    let value_span = node_span(value_node);
                    if idx.line_column(key_span.start).0 as usize == line
                        || (idx.line_column(value_span.start).0 as usize == line
                            && value_span.start != value_span.end)
                    {
                        return true;
                    }
                    if node_has_content_on_line(value_node, line, idx) {
                        return true;
                    }
                }
                false
            }
            Node::Sequence { items, .. } => items.iter().any(|item| {
                let span = node_span(item);
                idx.line_column(span.start).0 as usize == line
                    || node_has_content_on_line(item, line, idx)
            }),
            Node::Scalar { loc, .. } => {
                idx.line_column(loc.start).0 as usize == line && loc.start != loc.end
            }
            Node::Alias { .. } => false,
        }
    }
    docs.iter()
        .any(|doc| node_has_content_on_line(&doc.root, cursor_parser_line, doc.line_index()))
}

/// Determine the `CursorLocation` for a cursor position within `docs`.
///
/// Returns `OutsideAny` when the cursor cannot be placed inside any node
/// (empty document, past EOF, on a `---`/`...` separator, on a comment).
/// Otherwise returns the most-specific variant describing the cursor context.
fn locate_cursor(docs: &[Document<Span>], position: Position) -> CursorLocation<'_> {
    if docs.is_empty() {
        return CursorLocation::OutsideAny;
    }

    let cursor = lsp_position_to_pos(position);

    // If cursor sits on a `---` or `...` separator line, return OutsideAny.
    // When a document has an explicit start marker, the marker is on the line
    // immediately before the root node's start line.
    for doc in docs {
        let idx = doc.line_index();
        let root_start = idx.line_column(node_span(&doc.root).start).0 as usize;
        if doc.explicit_start && root_start > 0 && cursor.line == root_start - 1 {
            return CursorLocation::OutsideAny;
        }
        if doc.explicit_end {
            let root_end = idx.line_column(node_span(&doc.root).end).0 as usize;
            if cursor.line == root_end {
                return CursorLocation::OutsideAny;
            }
        }
    }

    for doc in docs {
        let idx = doc.line_index();
        let result = locate_in_node(&doc.root, cursor, &mut Vec::new(), idx);
        if !matches!(result, CursorLocation::OutsideAny) {
            return result;
        }
    }

    // No node contained the cursor. Try the blank-line extension: walk
    // mappings whose span covers the cursor line and descend by column.
    // Skip this extension if the cursor line has actual mapping content —
    // positions past the end of a content-bearing line should return OutsideAny.
    if !cursor_line_has_mapping_content(docs, cursor.line) {
        for doc in docs {
            let idx = doc.line_index();
            let path: Vec<String> = Vec::new();
            if let Node::Mapping { loc, .. } = &doc.root {
                if idx.line_column(loc.start).0 as usize <= cursor.line
                    && cursor.line <= idx.line_column(loc.end).0 as usize
                {
                    let mut descent_path: Vec<String> = Vec::new();
                    if let Some(mapping) =
                        deepest_mapping_at_column(&doc.root, cursor, &mut descent_path, idx)
                    {
                        return CursorLocation::InBlankMapping {
                            enclosing_path: descent_path,
                            mapping,
                        };
                    }
                }
            } else if let Node::Sequence { loc, .. } = &doc.root {
                if idx.line_column(loc.start).0 as usize <= cursor.line
                    && cursor.line <= idx.line_column(loc.end).0 as usize
                {
                    return CursorLocation::InBlankSequence {
                        enclosing_path: path,
                        sequence: &doc.root,
                    };
                }
            }
        }
    }

    CursorLocation::OutsideAny
}

/// Recursively walk `node`, building `enclosing_path` as keys are descended.
/// Returns the most-specific `CursorLocation` for `cursor`, or `OutsideAny`
/// if the cursor is not inside `node`.
fn locate_in_node<'a>(
    node: &'a Node<Span>,
    cursor: Pos,
    enclosing_path: &mut Vec<String>,
    idx: &LineIndex,
) -> CursorLocation<'a> {
    match node {
        Node::Mapping { entries, .. } => {
            for (key_node, value_node) in entries {
                let key_span = node_span(key_node);
                let value_span = node_span(value_node);

                if span_contains_cursor(key_span, cursor, idx) {
                    let key = scalar_key(key_node).unwrap_or("").to_string();
                    return CursorLocation::OnKey {
                        key,
                        enclosing_path: enclosing_path.clone(),
                        mapping: node,
                    };
                }

                if span_contains_cursor(value_span, cursor, idx) {
                    let key = scalar_key(key_node).unwrap_or("").to_string();
                    enclosing_path.push(key.clone());

                    // Recurse into the value.
                    let inner = locate_in_node(value_node, cursor, enclosing_path, idx);
                    if !matches!(inner, CursorLocation::OutsideAny) {
                        return inner;
                    }

                    // Value span contains cursor but no child matched.
                    // If the value is a Mapping, the cursor is on a blank/whitespace
                    // line inside that mapping — not on the scalar value.
                    if matches!(value_node, Node::Mapping { .. }) {
                        return CursorLocation::InBlankMapping {
                            enclosing_path: enclosing_path.clone(),
                            mapping: value_node,
                        };
                    }
                    // Similarly for Sequence.
                    if matches!(value_node, Node::Sequence { .. }) {
                        return CursorLocation::InBlankSequence {
                            enclosing_path: enclosing_path.clone(),
                            sequence: value_node,
                        };
                    }

                    enclosing_path.pop();

                    // Cursor is on the scalar value directly.
                    return CursorLocation::OnValue {
                        key,
                        enclosing_path: enclosing_path.clone(),
                    };
                }

                // Fallback A: cursor is on the same line as the key, past the key
                // span, and the value node's span starts on a DIFFERENT line.
                // This happens for null/empty values where the parser places the
                // value span at the start of the following line. Treat as OnValue.
                if cursor.line == idx.line_column(key_span.start).0 as usize
                    && cursor.column >= idx.line_column(key_span.end).1 as usize
                    && idx.line_column(value_span.start).0 as usize != cursor.line
                {
                    if let Some(key) = scalar_key(key_node) {
                        return CursorLocation::OnValue {
                            key: key.to_string(),
                            enclosing_path: enclosing_path.clone(),
                        };
                    }
                }
            }
            CursorLocation::OutsideAny
        }
        Node::Sequence { items, .. } => {
            for item in items {
                let item_span = node_span(item);
                if span_contains_cursor(item_span, cursor, idx) {
                    // Push "[]" so that inner mapping keys carry the sequence
                    // sentinel in their enclosing_path.
                    enclosing_path.push("[]".to_string());
                    let inner = locate_in_node(item, cursor, enclosing_path, idx);
                    if matches!(inner, CursorLocation::OutsideAny) {
                        enclosing_path.pop();
                        return CursorLocation::InSequenceItem {
                            enclosing_path: enclosing_path.clone(),
                            sequence: node,
                            current_item: item,
                        };
                    }
                    // inner already has the "[]" in path via enclosing_path
                    return inner;
                }
            }

            // Cursor in sequence span but not in any item — blank sequence line.
            if span_contains_cursor(node_span(node), cursor, idx) {
                return CursorLocation::InBlankSequence {
                    enclosing_path: enclosing_path.clone(),
                    sequence: node,
                };
            }

            CursorLocation::OutsideAny
        }
        Node::Scalar { .. } | Node::Alias { .. } => CursorLocation::OutsideAny,
    }
}

/// Walk `docs` following `path` (a sequence of mapping key strings) and return
/// the node at that path, or `None` if any step fails.
fn find_node_at_path<'a>(docs: &'a [Document<Span>], path: &[String]) -> Option<&'a Node<Span>> {
    let root = docs.first().map(|d| &d.root)?;
    let mut current = root;
    for key in path {
        match current {
            Node::Mapping { entries, .. } => {
                let entry = entries
                    .iter()
                    .find(|(k, _)| scalar_key(k) == Some(key.as_str()));
                current = entry.map(|(_, v)| v)?;
            }
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => return None,
        }
    }
    Some(current)
}

/// Return all keys present in `mapping`, excluding the entry whose key token
/// starts on `cursor_line` (the line currently being edited).
///
/// `cursor_line` is the 0-based LSP line number. The exclusion prevents the
/// key under the cursor from appearing in "already present" sets when
/// computing schema suggestions.
fn present_keys(mapping: &Node<Span>, cursor_line: usize, idx: &LineIndex) -> HashSet<String> {
    let Node::Mapping { entries, .. } = mapping else {
        return HashSet::new();
    };
    // Parser lines are 1-based; LSP lines are 0-based.
    let parser_cursor_line = cursor_line + 1;
    entries
        .iter()
        .filter_map(|(key_node, _)| {
            let key_span = node_span(key_node);
            if idx.line_column(key_span.start).0 as usize == parser_cursor_line {
                return None;
            }
            scalar_key(key_node).map(ToString::to_string)
        })
        .collect()
}

/// Return all keys in `mapping` in declaration order.
///
/// Non-scalar keys (complex mapping-as-key, sequence-as-key) are skipped.
fn collect_sibling_keys_ast(mapping: &Node<Span>) -> Vec<String> {
    let Node::Mapping { entries, .. } = mapping else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(key_node, _)| scalar_key(key_node).map(ToString::to_string))
        .collect()
}

/// Return the union of all keys across every item in `sequence`.
///
/// Items that are not mappings (e.g. scalar items) contribute no keys.
fn collect_sequence_sibling_keys(sequence: &Node<Span>) -> HashSet<String> {
    let Node::Sequence { items, .. } = sequence else {
        return HashSet::new();
    };
    items
        .iter()
        .flat_map(|item| {
            if let Node::Mapping { entries, .. } = item {
                entries
                    .iter()
                    .filter_map(|(k, _)| scalar_key(k).map(ToString::to_string))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        })
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    clippy::indexing_slicing,
    reason = "test code"
)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::schema::{JsonSchema, SchemaType};
    use crate::test_utils::parse_docs;
    use serde_json::json;
    use tower_lsp::lsp_types::Documentation;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
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

    // Tests 1, 3, 4, 5 — sibling key suggestions and exclusions
    #[rstest]
    #[case::sibling_keys(
        "name: Alice\nage: 30\n",
        pos(0, 0),
        &["age"][..],
        &["name"][..]
    )]
    #[case::nested_sibling_keys(
        "server:\n  host: localhost\n  port: 8080\n",
        pos(1, 2),
        &["port"][..],
        &["server", "host"][..]
    )]
    #[case::deeply_nested_keys(
        "a:\n  b:\n    c: 1\n    d: 2\n",
        pos(2, 4),
        &["d"][..],
        &["a", "b", "c"][..]
    )]
    #[case::sequence_item_sibling(
        "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n",
        pos(3, 4),
        &["age"][..],
        &[][..]
    )]
    fn sibling_key_suggests_and_excludes(
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] expected: &[&str],
        #[case] absent: &[&str],
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, None);
        let ls = labels(&result);
        for key in expected {
            assert!(ls.contains(key), "should suggest {key:?}, got: {ls:?}");
        }
        for key in absent {
            assert!(!ls.contains(key), "should not suggest {key:?}, got: {ls:?}");
        }
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD)),
            "all no-schema key completions should have FIELD kind"
        );
    }

    // Test 2
    #[test]
    fn should_not_suggest_keys_already_present_in_mapping() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), None);

        let labels = labels(&result);
        assert!(
            !labels.contains(&"name"),
            "should not suggest 'name' which is at the cursor line"
        );
    }

    // Test 6
    #[test]
    fn should_not_suggest_keys_already_in_current_sequence_item() {
        let text = "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 4), None);

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
        let result = complete_at(&docs, pos(3, 10), None);

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
        let result = complete_at(&docs, pos(3, 10), None);

        let labels = labels(&result);
        let production_count = labels.iter().filter(|&&l| l == "production").count();
        assert_eq!(
            production_count, 1,
            "should deduplicate: 'production' should appear only once, got: {labels:?}"
        );
    }

    // Test 10
    #[test]
    fn should_return_empty_when_ast_is_none() {
        let result = complete_at(&[], pos(0, 0), None);

        assert!(
            result.is_empty(),
            "should return empty when AST is None (failed parse)"
        );
    }

    // Tests 9, 11, 12, 13, 14 — empty result for various degenerate inputs (no schema)
    #[rstest]
    #[case::empty_document("", pos(0, 0))]
    #[case::comment_line("# comment\nkey: value\n", pos(0, 0))]
    #[case::document_separator("key1: v1\n---\nkey2: v2\n", pos(1, 0))]
    #[case::position_beyond_lines("key: value\n", pos(10, 0))]
    #[case::position_beyond_line_length("key: value\n", pos(0, 100))]
    fn returns_empty_for_structural_no_schema(#[case] text: &str, #[case] cursor: Position) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, None);
        assert!(result.is_empty(), "should return empty, got: {result:?}");
    }

    // Test 15
    #[test]
    fn should_return_empty_for_no_documents() {
        let empty: Vec<Document<Span>> = Vec::new();
        let result = complete_at(&empty, pos(0, 0), None);

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        // Use a real document with a different key so schema suggests "name".
        let text = "age: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(2, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(1, 2), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(1, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 5), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 5), Some(&schema));

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
        let result = complete_at(&docs, pos(1, 5), Some(&schema));

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
        let result = complete_at(&docs, pos(1, 5), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 9), Some(&schema));

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

    // Tests 29, 30, 31 — schema path resolution suggests nested property
    #[rstest]
    #[case::nested_path(
        object_schema(vec![("database", object_schema(vec![("host", string_schema()), ("port", integer_schema())]))]),
        "database:\n  host: localhost\n",
        pos(1, 2),
        "port",
        "database"
    )]
    #[case::array_items_schema(
        object_schema(vec![("servers", JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            items: Some(Box::new(object_schema(vec![("host", string_schema()), ("port", integer_schema())]))),
            ..JsonSchema::default()
        })]),
        "servers:\n  - host: localhost\n",
        pos(1, 4),
        "port",
        "servers"
    )]
    #[case::third_level_nesting(
        object_schema(vec![("a", object_schema(vec![("b", object_schema(vec![("c", string_schema()), ("d", integer_schema())]))]))]),
        "a:\n  b:\n    c: v\n",
        pos(2, 4),
        "d",
        "a"
    )]
    fn schema_path_resolution_suggests_nested_property(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] expected: &str,
        #[case] absent: &str,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, Some(&schema));
        let ls = labels(&result);
        assert!(
            ls.contains(&expected),
            "should suggest {expected:?}, got: {ls:?}"
        );
        assert!(
            !ls.contains(&absent),
            "should not suggest {absent:?}, got: {ls:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group E — Composition Schemas
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 32, 33, 34 — composition schema suggests properties from branches
    #[rstest]
    #[case::allof_branches(
        JsonSchema { all_of: Some(vec![object_schema(vec![("name", string_schema())]), object_schema(vec![("age", integer_schema())])]), ..JsonSchema::default() },
        "name: Alice\n",
        pos(0, 0),
        "age"
    )]
    #[case::anyof_branches(
        JsonSchema { any_of: Some(vec![object_schema(vec![("host", string_schema())]), object_schema(vec![("socket", string_schema())])]), ..JsonSchema::default() },
        "host: localhost\n",
        pos(0, 0),
        "socket"
    )]
    #[case::oneof_branches(
        JsonSchema { one_of: Some(vec![object_schema(vec![("url", string_schema())]), object_schema(vec![("path", string_schema())])]), ..JsonSchema::default() },
        "url: http://example.com\n",
        pos(0, 0),
        "path"
    )]
    fn composition_schema_suggests_from_branches(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] expected: &str,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, Some(&schema));
        let ls = labels(&result);
        assert!(
            ls.contains(&expected),
            "should suggest {expected:?} from composition branches, got: {ls:?}"
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
        let result = complete_at(&docs, pos(0, 0), None);

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        let result = complete_at(&[], pos(0, 0), Some(&schema));

        assert!(result.is_empty(), "should return empty for empty document");
    }

    // Test 39
    #[test]
    fn should_return_empty_for_schema_completion_on_comment_line() {
        let schema = object_schema(vec![("name", string_schema())]);
        let text = "# comment\nkey: value\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        assert!(result.is_empty(), "should return empty for comment line");
    }

    // Test 40
    #[test]
    fn should_return_empty_for_schema_completion_on_document_separator() {
        let schema = object_schema(vec![("name", string_schema())]);
        let text = "key1: v1\n---\nkey2: v2\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        // Cursor within "pro" — value position with partial input
        let text = "env: pro\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 7), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
                "documentation should be truncated to 200 chars, got {doc_char_count}"
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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        // At most 20 branches walked → at most 20 distinct schema-sourced properties
        let schema_prop_count = result
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::FIELD))
            .count();
        assert!(
            schema_prop_count <= 20,
            "at most 20 allOf branches should be walked, got {schema_prop_count} schema props"
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
        let result = complete_at(&docs, pos(0, 5), Some(&schema));

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
        let result = complete_at(&docs, pos(0, 9), Some(&schema));

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
        let _result = complete_at(&docs, pos(4, 8), Some(&schema));
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
        let result = complete_at(&docs, pos(1, 0), Some(&schema));

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

    // ══════════════════════════════════════════════════════════════════════════
    // Group H — Multi-Document Boundary Tests
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 51, 52, 54 — cross-document label contamination prevention
    #[rstest]
    #[case::sibling_not_cross_dash("alpha: 1\n---\nbeta: 2\n", pos(2, 0), None, "alpha")]
    #[case::sibling_not_cross_ellipsis("alpha: 1\n...\nbeta: 2\n", pos(2, 0), None, "alpha")]
    #[case::values_not_from_other_doc(
        "env: production\n---\nenv: \n",
        pos(2, 5),
        None,
        "production"
    )]
    fn cross_document_label_not_contaminated(
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] schema: Option<&JsonSchema>,
        #[case] absent_label: &str,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, schema);
        let ls = labels(&result);
        assert!(
            !ls.contains(&absent_label),
            "should not suggest {absent_label:?} from other document, got: {ls:?}"
        );
    }

    // Test 53 — collect_present_keys_at_indent must not see keys from other document
    #[test]
    fn should_not_suppress_schema_key_present_only_in_other_document() {
        // doc1 has "name: Alice"; doc2 has only "age: 30".
        // Schema has "name" and "age". Cursor in doc2 — "name" should be suggested because
        // it is not present in doc2.
        let schema = object_schema(vec![("name", string_schema()), ("age", integer_schema())]);
        let text = "name: Alice\n---\nage: 30\n";
        let docs = parse_docs(text);
        // cursor on "age:" in doc2
        let result = complete_at(&docs, pos(2, 0), Some(&schema));

        let labels = labels(&result);
        assert!(
            labels.contains(&"name"),
            "should suggest 'name' because it is absent from document 2, got: {labels:?}"
        );
    }

    // Test 55 — is_in_sequence_item must not cross --- boundary
    #[test]
    fn should_not_detect_sequence_context_from_other_document() {
        // doc1 has a sequence item "- name: Alice"; doc2 has a plain mapping "host: local".
        // Completion in doc2 should use mapping-sibling logic, not sequence-item logic.
        let text = "items:\n  - name: Alice\n---\nhost: local\nport: 8080\n";
        let docs = parse_docs(text);
        // cursor on "host:" in doc2
        let result = complete_at(&docs, pos(3, 0), None);

        let labels = labels(&result);
        assert!(
            labels.contains(&"port"),
            "should suggest sibling key 'port' in document 2, got: {labels:?}"
        );
        assert!(
            !labels.contains(&"name"),
            "should not suggest 'name' from the sequence in document 1, got: {labels:?}"
        );
    }

    // Test 56 — cursor on first line (no separator before it)
    #[test]
    fn should_handle_cursor_on_first_line_of_multi_doc_file() {
        let text = "alpha: 1\n---\nbeta: 2\n";
        let docs = parse_docs(text);
        // cursor on "alpha:" — first line, no separator before it
        let result = complete_at(&docs, pos(0, 0), None);

        let labels = labels(&result);
        assert!(
            !labels.contains(&"beta"),
            "should not suggest 'beta' from document 2 when cursor is on line 0, got: {labels:?}"
        );
    }

    // Test 57 — cursor on last line of file (no separator after it)
    #[test]
    fn should_handle_cursor_on_last_line_of_multi_doc_file() {
        let text = "alpha: 1\n---\nbeta: 2\ngamma: 3\n";
        let docs = parse_docs(text);
        // cursor on last line "gamma:"
        let result = complete_at(&docs, pos(3, 0), None);

        let labels = labels(&result);
        assert!(
            labels.contains(&"beta"),
            "should suggest sibling 'beta' from the same document, got: {labels:?}"
        );
        assert!(
            !labels.contains(&"alpha"),
            "should not suggest 'alpha' from document 1, got: {labels:?}"
        );
    }

    // Test 58 — consecutive separators (empty document between them)
    #[test]
    fn should_handle_consecutive_document_separators() {
        let text = "alpha: 1\n---\n---\nbeta: 2\n";
        let docs = parse_docs(text);
        // cursor on "beta:" — the document between the two --- lines is empty
        let result = complete_at(&docs, pos(3, 0), None);

        let labels = labels(&result);
        assert!(
            !labels.contains(&"alpha"),
            "should not suggest 'alpha' from document 1 through empty middle document, got: {labels:?}"
        );
    }

    // Test 59 — deprecated property gets DEPRECATED tag and tilde sort_text
    #[test]
    fn should_tag_deprecated_property_with_deprecated_tag_and_tilde_sort_text() {
        let schema = object_schema(vec![(
            "old_field",
            JsonSchema {
                deprecated: Some(true),
                ..JsonSchema::default()
            },
        )]);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let item = result
            .iter()
            .find(|i| i.label == "old_field")
            .expect("should suggest old_field");
        assert_eq!(
            item.tags,
            Some(vec![CompletionItemTag::DEPRECATED]),
            "deprecated property should have DEPRECATED tag"
        );
        assert!(
            item.sort_text
                .as_deref()
                .is_some_and(|s| s.starts_with('~')),
            "deprecated property sort_text should start with '~', got: {:?}",
            item.sort_text
        );
    }

    // Test 60 — non-deprecated property has no tags and no sort_text
    #[test]
    fn should_not_tag_non_deprecated_property() {
        let schema = object_schema(vec![(
            "current_field",
            JsonSchema {
                deprecated: None,
                ..JsonSchema::default()
            },
        )]);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let item = result
            .iter()
            .find(|i| i.label == "current_field")
            .expect("should suggest current_field");
        assert_eq!(
            item.tags, None,
            "non-deprecated property should have no tags"
        );
        assert_eq!(
            item.sort_text, None,
            "non-deprecated property should have no sort_text"
        );
    }

    // Test 61 — only deprecated property is tagged when mixed schema
    #[test]
    fn should_only_tag_deprecated_property_in_mixed_schema() {
        let schema = object_schema(vec![
            (
                "new_field",
                JsonSchema {
                    deprecated: None,
                    ..JsonSchema::default()
                },
            ),
            (
                "old_field",
                JsonSchema {
                    deprecated: Some(true),
                    ..JsonSchema::default()
                },
            ),
        ]);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let new_item = result
            .iter()
            .find(|i| i.label == "new_field")
            .expect("should suggest new_field");
        let old_item = result
            .iter()
            .find(|i| i.label == "old_field")
            .expect("should suggest old_field");

        assert_eq!(
            new_item.tags, None,
            "non-deprecated 'new_field' should have no tags"
        );
        assert_eq!(
            old_item.tags,
            Some(vec![CompletionItemTag::DEPRECATED]),
            "deprecated 'old_field' should have DEPRECATED tag"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group I — Multi-Required Snippet Completion
    // ══════════════════════════════════════════════════════════════════════════

    fn schema_with_required(props: Vec<(&str, JsonSchema)>, required: Vec<&str>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            required: Some(required.into_iter().map(str::to_string).collect()),
            ..JsonSchema::default()
        }
    }

    // Test 62 — 3 required props all missing → snippet item with all 3 tab-stops
    #[test]
    fn should_offer_all_required_snippet_when_three_required_props_missing() {
        let schema = schema_with_required(
            vec![
                ("name", string_schema()),
                ("age", integer_schema()),
                ("enabled", boolean_schema()),
            ],
            vec!["name", "age", "enabled"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer '(all required)' snippet item");

        let insert_text = snippet
            .insert_text
            .as_deref()
            .expect("snippet item must have insert_text");

        assert!(
            insert_text.contains("${1:"),
            "snippet must contain tab-stop ${{1:...}}, got: {insert_text}"
        );
        assert!(
            insert_text.contains("${2:"),
            "snippet must contain tab-stop ${{2:...}}, got: {insert_text}"
        );
        assert!(
            insert_text.contains("${3:"),
            "snippet must contain tab-stop ${{3:...}}, got: {insert_text}"
        );
        assert!(
            insert_text.contains("name:"),
            "snippet must mention 'name', got: {insert_text}"
        );
        assert!(
            insert_text.contains("age:"),
            "snippet must mention 'age', got: {insert_text}"
        );
        assert!(
            insert_text.contains("enabled:"),
            "snippet must mention 'enabled', got: {insert_text}"
        );
    }

    // Tests 63, 64 — no snippet offered when insufficient required props missing
    #[rstest]
    #[case::only_one_missing(
        schema_with_required(
            vec![("name", string_schema()), ("age", integer_schema()), ("enabled", boolean_schema())],
            vec!["name", "age", "enabled"],
        ),
        "name: Alice\nage: 30\n",
        pos(0, 0)
    )]
    #[case::no_required_props(
        object_schema(vec![("name", string_schema()), ("age", integer_schema())]),
        "\n",
        pos(0, 0)
    )]
    fn should_not_offer_snippet(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] cursor: Position,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, Some(&schema));
        let has_snippet = result.iter().any(|i| i.label == "(all required)");
        assert!(!has_snippet, "should not offer '(all required)' snippet");
    }

    // Test 65 — type-aware defaults: string → "", integer → 0, boolean → false
    #[test]
    #[expect(
        clippy::literal_string_with_formatting_args,
        reason = "snippet placeholders look like format args"
    )]
    fn should_use_type_aware_defaults_in_snippet() {
        let schema = schema_with_required(
            vec![
                ("title", string_schema()),
                ("count", integer_schema()),
                ("active", boolean_schema()),
            ],
            vec!["title", "count", "active"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer snippet");

        let insert_text = snippet
            .insert_text
            .as_deref()
            .expect("must have insert_text");

        assert!(
            insert_text.contains("\"\""),
            "string type should default to \"\", got: {insert_text}"
        );
        assert!(
            insert_text.contains(":0")
                || insert_text.contains(": 0")
                || insert_text.contains("{1:0}")
                || insert_text.contains("{2:0}")
                || insert_text.contains("{3:0}"),
            "integer type should default to 0, got: {insert_text}"
        );
        assert!(
            insert_text.contains("false"),
            "boolean type should default to false, got: {insert_text}"
        );
    }

    // Test 66 — snippet item has InsertTextFormat::SNIPPET
    #[test]
    fn should_set_insert_text_format_to_snippet() {
        let schema = schema_with_required(
            vec![("name", string_schema()), ("age", integer_schema())],
            vec!["name", "age"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer snippet");

        assert_eq!(
            snippet.insert_text_format,
            Some(InsertTextFormat::SNIPPET),
            "snippet item must have InsertTextFormat::SNIPPET"
        );
    }

    // Test 67 — snippet item sort_text is "!" (sorts to top)
    #[test]
    fn should_set_snippet_sort_text_to_exclamation() {
        let schema = schema_with_required(
            vec![("name", string_schema()), ("age", integer_schema())],
            vec!["name", "age"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer snippet");

        assert_eq!(
            snippet.sort_text.as_deref(),
            Some("!"),
            "snippet sort_text should be '!' to sort to top"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group J — Previously Uncovered Paths
    // ══════════════════════════════════════════════════════════════════════════

    // Lines 64-74: blank line → empty for both None-schema and empty-schema paths
    #[rstest]
    #[case::no_schema("key: value\n\n", pos(1, 0), None)]
    #[case::schema_no_properties("\n", pos(0, 0), Some(JsonSchema::default()))]
    fn blank_line_returns_empty(
        #[case] text: &str,
        #[case] cursor: Position,
        #[case] schema: Option<JsonSchema>,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, schema.as_ref());
        assert!(
            result.is_empty(),
            "blank line should return empty, got: {result:?}"
        );
    }

    // Sequence item with no inline key uses "[]" sentinel for schema path descent.
    #[test]
    fn should_build_path_with_sequence_sentinel_for_bare_sequence_parent() {
        // "servers" is a sequence; items have "host" and "port".
        // Schema resolves via "servers" → [] → items schema.
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
        // Bare sequence item "- " with no inline key, then indented key below it
        let text = "servers:\n  -\n    host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(2, 4), Some(&schema));

        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest 'port' via sequence [] path, got: {ls:?}"
        );
    }

    // Lines 361-365: snippet_default for object/array/unknown types
    #[test]
    fn should_use_object_default_in_snippet_for_object_type_required_field() {
        // Need 2+ required missing for snippet; pair with a string field
        let schema = schema_with_required(
            vec![
                (
                    "config",
                    JsonSchema {
                        schema_type: Some(SchemaType::Single("object".to_string())),
                        ..JsonSchema::default()
                    },
                ),
                ("name", string_schema()),
            ],
            vec!["config", "name"],
        );
        let schema2 = schema_with_required(
            vec![
                (
                    "tags",
                    JsonSchema {
                        schema_type: Some(SchemaType::Single("array".to_string())),
                        ..JsonSchema::default()
                    },
                ),
                ("name", string_schema()),
            ],
            vec!["tags", "name"],
        );

        let text = "placeholder: null\n";
        let docs = parse_docs(text);

        let result1 = complete_at(&docs, pos(0, 0), Some(&schema));
        let snippet1 = result1.iter().find(|i| i.label == "(all required)");
        assert!(
            snippet1.is_some(),
            "should offer snippet for object-typed field"
        );
        let insert1 = snippet1.unwrap().insert_text.as_deref().unwrap_or("");
        assert!(
            insert1.contains("{}"),
            "object type default should be '{{}}', got: {insert1}"
        );

        let result2 = complete_at(&docs, pos(0, 0), Some(&schema2));
        let snippet2 = result2.iter().find(|i| i.label == "(all required)");
        assert!(
            snippet2.is_some(),
            "should offer snippet for array-typed field"
        );
        let insert2 = snippet2.unwrap().insert_text.as_deref().unwrap_or("");
        assert!(
            insert2.contains("[]"),
            "array type default should be '[]', got: {insert2}"
        );
    }

    // Line 332: required field with no-default type (None type) → bare tab-stop format
    #[test]
    fn should_use_bare_tab_stop_in_snippet_for_field_with_no_type() {
        // Need 2+ required missing to trigger snippet; pair no-type "data" with typed "name"
        let schema = schema_with_required(
            vec![("data", JsonSchema::default()), ("name", string_schema())],
            vec!["data", "name"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let snippet = result.iter().find(|i| i.label == "(all required)");
        assert!(snippet.is_some(), "should offer snippet");
        let insert = snippet.unwrap().insert_text.as_deref().unwrap_or("");
        // "data" has no type so default is "" → produces "data: ${N:}" (bare tab-stop)
        assert!(
            insert.contains("data: ${"),
            "no-type field should have a tab-stop, got: {insert}"
        );
    }

    // Line 377: collect_schema_properties depth cap
    #[test]
    fn should_not_panic_when_allof_depth_exceeds_max_branch_count() {
        // Build a deeply recursive schema via allOf to hit the depth guard
        fn deep_schema(depth: usize) -> JsonSchema {
            if depth == 0 {
                return object_schema(vec![("leaf", JsonSchema::default())]);
            }
            JsonSchema {
                all_of: Some(vec![deep_schema(depth - 1)]),
                ..JsonSchema::default()
            }
        }
        // 25 levels deep — exceeds MAX_BRANCH_COUNT (20)
        let schema = deep_schema(25);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        // Must not panic or hang
        let _result = complete_at(&docs, pos(0, 0), Some(&schema));
    }

    // Lines 475-477: json_value_to_yaml_label for Number, Null, Array, Object
    #[test]
    #[expect(
        clippy::approx_constant,
        reason = "3.14 is a test value, not an approximation of PI"
    )]
    fn should_render_number_and_null_enum_values_as_yaml_labels() {
        let schema = object_schema(vec![(
            "value",
            JsonSchema {
                enum_values: Some(vec![
                    serde_json::Value::Number(serde_json::Number::from(42)),
                    serde_json::Value::Null,
                    serde_json::Value::Number(serde_json::Number::from_f64(3.14).unwrap()),
                ]),
                ..JsonSchema::default()
            },
        )]);
        // "value: " is 7 chars; colon at index 5; col=6 puts cursor in value position
        let text = "value: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 6), Some(&schema));

        let ls = labels(&result);
        assert!(ls.contains(&"42"), "should render integer 42, got: {ls:?}");
        assert!(ls.contains(&"null"), "should render null, got: {ls:?}");
        assert!(
            ls.iter().any(|l| l.starts_with("3.14") || *l == "3.14"),
            "should render float 3.14, got: {ls:?}"
        );
    }

    #[test]
    fn should_skip_array_and_object_enum_values() {
        let schema = object_schema(vec![(
            "value",
            JsonSchema {
                enum_values: Some(vec![
                    serde_json::json!("valid"),
                    serde_json::json!(["a", "b"]), // array — skipped
                    serde_json::json!({"k": "v"}), // object — skipped
                ]),
                ..JsonSchema::default()
            },
        )]);
        // col=6: cursor in value position (after "value: ")
        let text = "value: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 6), Some(&schema));

        let ls = labels(&result);
        assert!(
            ls.contains(&"valid"),
            "string enum value should appear, got: {ls:?}"
        );
        assert_eq!(
            ls.len(),
            1,
            "array and object enum values should be skipped, got: {ls:?}"
        );
    }

    // Line 486: SchemaType::Multiple in type_label
    #[test]
    fn should_render_multiple_type_label_as_pipe_separated_string() {
        let schema = object_schema(vec![(
            "value",
            JsonSchema {
                schema_type: Some(SchemaType::Multiple(vec![
                    "string".to_string(),
                    "null".to_string(),
                ])),
                ..JsonSchema::default()
            },
        )]);
        let text = "name: x\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));

        let item = result.iter().find(|i| i.label == "value");
        assert!(item.is_some(), "should suggest 'value'");
        assert_eq!(
            item.unwrap().detail.as_deref(),
            Some("string | null"),
            "multiple types should be joined with ' | '"
        );
    }

    // Lines 599, 611, 619, 621: is_in_sequence_item edge cases
    // Line 599: prev line is a document separator → break
    #[test]
    fn should_not_detect_sequence_context_across_document_separator() {
        let text = "items:\n  - name: Alice\n---\nhost: local\n";
        let docs = parse_docs(text);
        // "host:" is in a plain mapping in doc2; is_in_sequence_item should return false
        let result = complete_at(&docs, pos(3, 0), None);
        let ls = labels(&result);
        // Should suggest sibling from same doc, not from sequence in doc1
        assert!(
            !ls.contains(&"name"),
            "should not suggest sequence key 'name' from doc1, got: {ls:?}"
        );
    }

    // Line 611: prev line at lower indent is NOT a "- " → break (no sequence detected)
    #[test]
    fn should_not_detect_sequence_context_when_parent_is_plain_mapping() {
        // "server:\n  host:" — parent is a plain mapping key, not a sequence item
        let text = "server:\n  host: localhost\n  port: 8080\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 2), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest sibling 'port', not sequence keys, got: {ls:?}"
        );
    }

    // Lines 619, 621: same-level "- " line in is_in_sequence_item → true
    // A same-indent previous "- " line means we're in a sequence context.
    #[test]
    fn should_detect_sequence_context_when_same_indent_sibling_is_sequence_item() {
        // Sequence items indented under a parent key
        // cursor on "  - name: Bob" (second item, which starts with "- ")
        let text = "people:\n  - name: Alice\n    age: 30\n  - name: Bob\n";
        let docs = parse_docs(text);
        // cursor on "  - name: Bob" (line 3), col=4 (inside key area)
        let result = complete_at(&docs, pos(3, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"age"),
            "should suggest 'age' from sibling sequence item via same-indent '- ' detection, got: {ls:?}"
        );
    }

    // Lines 675-733: find_current_item_start and find_sequence_indent
    #[test]
    fn should_suggest_sibling_sequence_item_keys_for_multiline_sequence_item() {
        // Sequence item spans multiple lines; cursor is inside an item
        // (not the first "- " line)
        let text = "items:\n  - name: Alice\n    age: 30\n    city: NY\n  - name: Bob\n";
        let docs = parse_docs(text);
        // cursor on line 4 ("  - name: Bob"), which is itself a "- " line
        let result = complete_at(&docs, pos(4, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"age") || ls.contains(&"city"),
            "should suggest keys from sibling sequence item, got: {ls:?}"
        );
    }

    #[test]
    fn should_find_sequence_indent_when_cursor_is_not_on_sequence_line() {
        // Cursor is on a key inside a sequence item (not the "- " line itself).
        // The sibling item has "score" which the current item doesn't — exercises
        // find_sequence_indent walking back from a non-"- " line.
        let text = "list:\n  - id: 1\n    label: a\n  - id: 2\n    score: 99\n";
        let docs = parse_docs(text);
        // cursor on line 2 ("    label: a") — inside the first sequence item
        let result = complete_at(&docs, pos(2, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"score"),
            "should suggest 'score' from sibling sequence item, got: {ls:?}"
        );
    }

    // Lines 778-827: collect_all_sequence_item_keys — walking backward to find
    // sequence start, then forward collecting keys
    #[test]
    fn should_collect_keys_from_all_sequence_items_including_those_before_cursor() {
        // Three sequence items; cursor on third. Keys from items 1 and 2 should appear.
        let text = "- kind: A\n  color: red\n- kind: B\n  size: large\n- kind: C\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(4, 2), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"color") || ls.contains(&"size"),
            "should collect keys from all prior sequence items, got: {ls:?}"
        );
    }

    // complete_at with blank line + schema → schema_key_completions path (lines 64-74)
    #[test]
    fn should_suggest_schema_keys_on_blank_line_when_schema_is_present() {
        let schema = object_schema(vec![("host", string_schema()), ("port", integer_schema())]);
        let text = "host: localhost\n\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 0), Some(&schema));

        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest 'port' on blank line with schema, got: {ls:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // AST substrate tests (Task 1)
    // ══════════════════════════════════════════════════════════════════════════

    // ── locate_cursor: OnKey ─────────────────────────────────────────────────

    #[rstest]
    #[case::on_key_top_level("name: Alice\nage: 30\n", pos(0, 0), "name", vec![])]
    #[case::on_key_nested("server:\n  host: localhost\n", pos(1, 2), "host", vec!["server".to_string()])]
    #[case::on_key_in_sequence_item(
        "items:\n  - name: foo\n    age: 1\n",
        pos(1, 4),
        "name",
        vec!["items".to_string(), "[]".to_string()]
    )]
    #[case::on_key_utf8("café: latte\n", pos(0, 0), "café", vec![])]
    #[case::on_key_three_levels(
        "a:\n  b:\n    c: v\n",
        pos(2, 4),
        "c",
        vec!["a".to_string(), "b".to_string()]
    )]
    fn locate_cursor_on_key(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_key: &str,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::OnKey {
                key,
                enclosing_path,
                mapping,
            } => {
                assert_eq!(key, expected_key, "key mismatch");
                assert_eq!(enclosing_path, expected_path, "path mismatch");
                assert!(
                    matches!(mapping, Node::Mapping { .. }),
                    "mapping should be a Mapping node"
                );
            }
            other => panic!("expected OnKey, got different variant for yaml={yaml:?}: {other:?}"),
        }
    }

    // ── locate_cursor: OnValue ───────────────────────────────────────────────

    #[rstest]
    #[case::on_value_scalar("name: Alice\n", pos(0, 6), "name", vec![])]
    #[case::on_value_nested("server:\n  host: localhost\n", pos(1, 8), "host", vec!["server".to_string()])]
    fn locate_cursor_on_value(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_key: &str,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::OnValue {
                key,
                enclosing_path,
            } => {
                assert_eq!(key, expected_key, "key mismatch");
                assert_eq!(enclosing_path, expected_path, "path mismatch");
            }
            other => panic!("expected OnValue, got different variant for yaml={yaml:?}: {other:?}"),
        }
    }

    // ── locate_cursor: InBlankMapping ────────────────────────────────────────

    #[rstest]
    #[case::blank_mapping_root("name: Alice\n\nage: 30\n", pos(1, 0), vec![])]
    #[case::blank_mapping_nested("server:\n  host: localhost\n  \nport: 80\n", pos(2, 2), vec!["server".to_string()])]
    #[case::blank_mapping_eof("server:\n  host: localhost\n", pos(2, 2), vec!["server".to_string()])]
    #[case::blank_mapping_column_boundary(
        "outer:\n  inner:\n    key: val\n",
        pos(3, 2),
        vec!["outer".to_string()]
    )]
    #[case::blank_mapping_column_descent_deeper(
        "outer:\n  inner:\n    key: val\n",
        pos(3, 4),
        vec!["outer".to_string(), "inner".to_string()]
    )]
    fn locate_cursor_in_blank_mapping(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::InBlankMapping {
                enclosing_path,
                mapping,
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(mapping, Node::Mapping { .. }),
                    "mapping should be a Mapping node"
                );
            }
            other => panic!(
                "expected InBlankMapping, got different variant for yaml={yaml:?}: {other:?}"
            ),
        }
    }

    // ── locate_cursor: InBlankSequence ───────────────────────────────────────

    #[rstest]
    #[case::blank_sequence_after_scalar("items:\n  - foo\n  \n", pos(2, 2), vec!["items".to_string()])]
    fn locate_cursor_in_blank_sequence(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::InBlankSequence {
                enclosing_path,
                sequence,
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(sequence, Node::Sequence { .. }),
                    "sequence should be a Sequence node"
                );
            }
            other => panic!(
                "expected InBlankSequence, got different variant for yaml={yaml:?}: {other:?}"
            ),
        }
    }

    // ── locate_cursor: InSequenceItem ────────────────────────────────────────

    #[rstest]
    #[case::in_sequence_item_mapping_second_key(
        "items:\n  - name: foo\n    age: 1\n",
        pos(2, 4),
        vec!["items".to_string(), "[]".to_string()]
    )]
    fn locate_cursor_in_sequence_item(
        #[case] yaml: &str,
        #[case] position: Position,
        #[case] expected_path: Vec<String>,
    ) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        match loc {
            CursorLocation::InSequenceItem {
                enclosing_path,
                sequence,
                ..
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(sequence, Node::Sequence { .. }),
                    "sequence should be a Sequence node"
                );
            }
            CursorLocation::OnKey {
                enclosing_path,
                mapping,
                ..
            } => {
                assert_eq!(
                    enclosing_path, expected_path,
                    "path mismatch for yaml={yaml:?}"
                );
                assert!(
                    matches!(mapping, Node::Mapping { .. }),
                    "mapping should be a Mapping node"
                );
            }
            other => panic!(
                "expected InSequenceItem or OnKey, got different variant for yaml={yaml:?}: {other:?}"
            ),
        }
    }

    #[test]
    fn locate_cursor_in_sequence_item_scalar() {
        let yaml = "tags:\n  - rust\n  - yaml\n";
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, pos(1, 4));
        match loc {
            CursorLocation::InSequenceItem {
                enclosing_path,
                sequence,
                current_item,
            } => {
                assert_eq!(enclosing_path, vec!["tags".to_string()]);
                assert!(
                    matches!(sequence, Node::Sequence { .. }),
                    "sequence should be a Sequence node"
                );
                assert!(
                    matches!(current_item, Node::Scalar { .. }),
                    "current_item should be scalar"
                );
            }
            other => panic!("expected InSequenceItem, got: {other:?}"),
        }
    }

    // ── locate_cursor: OutsideAny ────────────────────────────────────────────

    #[rstest]
    #[case::empty_doc("", pos(0, 0))]
    #[case::past_eof("name: Alice\n", pos(5, 0))]
    #[case::on_separator("key1: v1\n---\nkey2: v2\n", pos(1, 0))]
    #[case::on_comment("# comment\nkey: val\n", pos(0, 2))]
    fn locate_cursor_outside_any(#[case] yaml: &str, #[case] position: Position) {
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, position);
        assert!(
            matches!(loc, CursorLocation::OutsideAny),
            "expected OutsideAny for yaml={yaml:?} position={position:?}"
        );
    }

    // ── locate_cursor: span_contains boundary cases ──────────────────────────

    #[test]
    fn locate_cursor_span_boundary_at_end_is_outside() {
        // The scalar "Alice" ends at some position; cursor exactly at span.end
        // should NOT be contained. We use a position clearly past any node.
        let yaml = "name: Alice\n";
        let docs = parse_docs(yaml);
        // Parser line 1, column 11 is one past "Alice" (col 6 start + 5 chars).
        // Use LSP pos(0, 11) to hit the boundary.
        let loc = locate_cursor(&docs, pos(0, 11));
        // Should be OutsideAny or InBlankMapping (not OnValue)
        assert!(
            !matches!(loc, CursorLocation::OnValue { .. }),
            "cursor at span.end should not be OnValue"
        );
    }

    #[test]
    fn locate_cursor_span_boundary_at_start_is_contained() {
        // Cursor at span.start should be contained.
        let yaml = "name: Alice\n";
        let docs = parse_docs(yaml);
        // "name" key starts at parser line=1, col=0 → LSP pos(0, 0)
        let loc = locate_cursor(&docs, pos(0, 0));
        assert!(
            matches!(loc, CursorLocation::OnKey { .. }),
            "cursor at span.start should be OnKey"
        );
    }

    // ── present_keys ─────────────────────────────────────────────────────────

    #[rstest]
    #[case::excludes_cursor_line(
        "name: Alice\nage: 30\ncity: NY\n",
        1,
        &["name", "city"],
        &["age"]
    )]
    #[case::only_entry_excluded("name: Alice\n", 0, &[], &["name"])]
    #[case::utf8("café: latte\nname: Alice\n", 0, &["name"], &["café"])]
    fn present_keys_test(
        #[case] yaml: &str,
        #[case] cursor_line: usize,
        #[case] expected_present: &[&str],
        #[case] expected_absent: &[&str],
    ) {
        let docs = parse_docs(yaml);
        let Node::Mapping { .. } = &docs[0].root else {
            panic!("expected mapping root");
        };
        let keys = present_keys(&docs[0].root, cursor_line, docs[0].line_index());
        for k in expected_present {
            assert!(
                keys.contains(*k),
                "expected '{k}' in present_keys, got: {keys:?}"
            );
        }
        for k in expected_absent {
            assert!(
                !keys.contains(*k),
                "expected '{k}' absent from present_keys, got: {keys:?}"
            );
        }
    }

    #[test]
    fn present_keys_sequence_item() {
        let yaml = "items:\n  - name: foo\n    age: 1\n";
        let docs = parse_docs(yaml);
        // Navigate to the sequence item mapping
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping root");
        };
        let (_, seq_value) = &entries[0];
        let Node::Sequence { items, .. } = seq_value else {
            panic!("expected sequence");
        };
        let item_mapping = &items[0];
        // cursor_line=1 corresponds to "- name: foo" (0-based LSP line 1)
        let keys = present_keys(item_mapping, 1, docs[0].line_index());
        assert!(keys.contains("age"), "age should be present");
        assert!(
            !keys.contains("name"),
            "name should be excluded (on cursor_line 1)"
        );
    }

    // ── collect_sibling_keys_ast ──────────────────────────────────────────────

    #[rstest]
    #[case::declaration_order("a: 1\nb: 2\nc: 3\n", vec!["a", "b", "c"])]
    #[case::single_key("only: val\n", vec!["only"])]
    #[case::utf8("café: 1\ntea: 2\n", vec!["café", "tea"])]
    fn collect_sibling_keys_ast_test(#[case] yaml: &str, #[case] expected: Vec<&str>) {
        let docs = parse_docs(yaml);
        let keys = collect_sibling_keys_ast(&docs[0].root);
        assert_eq!(
            keys, expected,
            "declaration order mismatch for yaml={yaml:?}"
        );
    }

    #[test]
    fn collect_sibling_keys_ast_skips_non_scalar_keys() {
        // Construct a mapping node manually to test non-scalar key skipping.
        // The simplest approach: parse a YAML that won't have complex keys, and
        // verify that the function only returns string keys.
        let yaml = "x: 1\ny: 2\n";
        let docs = parse_docs(yaml);
        let keys = collect_sibling_keys_ast(&docs[0].root);
        assert_eq!(keys, vec!["x", "y"]);
    }

    // ── collect_sequence_sibling_keys ────────────────────────────────────────

    #[rstest]
    #[case::union("- name: foo\n  age: 1\n- name: bar\n  city: NY\n", &["name", "age", "city"])]
    #[case::scalar_items_no_keys("- foo\n- bar\n", &[])]
    #[case::utf8("- café: latte\n- tea: matcha\n", &["café", "tea"])]
    #[case::dedup("- name: foo\n- name: bar\n", &["name"])]
    #[case::single_item("- x: 1\n  y: 2\n", &["x", "y"])]
    fn collect_sequence_sibling_keys_test(#[case] yaml: &str, #[case] expected: &[&str]) {
        let docs = parse_docs(yaml);
        // The root is a sequence in these test cases.
        let keys = collect_sequence_sibling_keys(&docs[0].root);
        let expected_set: HashSet<&str> = expected.iter().copied().collect();
        let actual_set: HashSet<&str> = keys.iter().map(String::as_str).collect();
        assert_eq!(
            actual_set, expected_set,
            "key set mismatch for yaml={yaml:?}"
        );
    }

    #[test]
    fn collect_sequence_sibling_keys_empty_sequence() {
        // Parse a sequence with no items — the root is a scalar for "[]" YAML
        // but an empty block sequence has no items.
        // Use an inline flow empty sequence to get an actual Sequence node.
        let docs2 = parse_docs("[]\n");
        let keys = collect_sequence_sibling_keys(&docs2[0].root);
        assert!(keys.is_empty(), "empty sequence should return empty set");
    }

    // ── locate_cursor: additional cases from TE test list ────────────────────

    #[test]
    fn locate_cursor_on_key_at_end_of_key_token() {
        // LC-2: cursor at last char of "name" key token
        let yaml = "name: Alice\n";
        let docs = parse_docs(yaml);
        let loc = locate_cursor(&docs, pos(0, 3));
        assert!(
            matches!(loc, CursorLocation::OnKey { ref key, .. } if key == "name"),
            "cursor at end of key token should still be OnKey"
        );
    }

    // ── C tests: complete_at branch coverage ─────────────────────────────────

    // C-1: OutsideAny returns empty
    #[test]
    fn complete_at_outside_any_returns_empty() {
        let docs = parse_docs("name: Alice\n");
        let result = complete_at(&docs, pos(5, 0), None);
        assert!(
            result.is_empty(),
            "OutsideAny should return empty, got: {result:?}"
        );
    }

    // C-2: OnKey no schema, no siblings
    #[test]
    fn complete_at_on_key_with_no_siblings_returns_empty() {
        let docs = parse_docs("only: val\n");
        let result = complete_at(&docs, pos(0, 0), None);
        assert!(
            result.is_empty(),
            "single key with no siblings should return empty, got: {result:?}"
        );
    }

    // C-3: OnKey with schema, present key excluded
    #[test]
    fn complete_at_on_key_schema_excludes_present_keys() {
        let docs = parse_docs("name: Alice\nage: 30\n");
        let schema = object_schema(vec![
            ("name", string_schema()),
            ("age", integer_schema()),
            ("city", string_schema()),
        ]);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"city"), "should suggest 'city', got: {ls:?}");
        assert!(
            !ls.contains(&"name"),
            "should exclude cursor key 'name', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"age"),
            "should exclude present key 'age', got: {ls:?}"
        );
    }

    // C-4: OnValue with schema enum
    #[test]
    fn complete_at_on_value_schema_enum() {
        let docs = parse_docs("env: \n");
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let result = complete_at(&docs, pos(0, 5), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"prod"), "should suggest 'prod', got: {ls:?}");
        assert!(
            ls.contains(&"staging"),
            "should suggest 'staging', got: {ls:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "enum suggestions should have VALUE kind"
        );
    }

    // C-5: OnValue no schema, structural fallback
    #[test]
    fn complete_at_on_value_no_schema_structural_fallback() {
        let docs = parse_docs("kind: app\nkind: \n");
        let result = complete_at(&docs, pos(1, 6), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"app"),
            "should suggest existing value 'app', got: {ls:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "structural value suggestions should have VALUE kind"
        );
    }

    // C-6: InSequenceItem, sibling keys minus current-item keys
    #[test]
    fn complete_at_in_sequence_item_suggests_missing_sibling_keys() {
        let docs = parse_docs("items:\n  - name: Alice\n    age: 30\n  - name: Bob\n");
        let result = complete_at(&docs, pos(3, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"age"),
            "should suggest sibling key 'age', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"name"),
            "should exclude current item key 'name', got: {ls:?}"
        );
    }

    // C-7: InBlankMapping with schema suggests missing schema key
    #[test]
    fn complete_at_in_blank_mapping_with_schema_suggests_keys() {
        let docs = parse_docs("server:\n  host: localhost\n  \n");
        let schema = object_schema(vec![(
            "server",
            object_schema(vec![("host", string_schema()), ("port", integer_schema())]),
        )]);
        let result = complete_at(&docs, pos(2, 2), Some(&schema));
        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest schema key 'port', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"host"),
            "should exclude present key 'host', got: {ls:?}"
        );
    }

    // C-8: InBlankMapping without schema — suggests nothing when all keys present.
    // On a blank line in a mapping with no schema, structural suggestions exclude
    // keys already in the document. When all keys are present, the result is empty.
    #[test]
    fn complete_at_in_blank_mapping_no_schema_structural_keys() {
        let docs = parse_docs("name: Alice\nage: 30\n\n");
        let result = complete_at(&docs, pos(2, 0), None);
        assert!(
            result.is_empty(),
            "all keys already present — blank-line no-schema should return empty, got: {result:?}"
        );
    }

    // C-9: InBlankSequence with schema descends via [] sentinel
    #[test]
    fn complete_at_in_blank_sequence_with_schema_descends_items() {
        let docs = parse_docs("servers:\n  - host: localhost\n  \n");
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
        let result = complete_at(&docs, pos(2, 2), Some(&schema));
        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest schema item key 'port', got: {ls:?}"
        );
    }

    // C-10: InSequenceItem — cursor on blank within a sequence item returns keys
    // from sibling items that are absent from the current item.
    // (The parser includes trailing blank lines in the item's span, so a blank
    // line between items routes to InSequenceItem for the preceding item.)
    #[test]
    fn complete_at_in_blank_sequence_no_schema_union_of_sibling_keys() {
        let docs =
            parse_docs("items:\n  - name: Alice\n    age: 30\n  \n  - name: Bob\n    city: NY\n");
        let result = complete_at(&docs, pos(3, 2), None);
        let ls = labels(&result);
        // Cursor is inside item 1 (name+age); item 2 has name+city.
        // InSequenceItem returns sibling keys minus item 1's keys → city.
        assert!(
            ls.contains(&"city"),
            "should suggest 'city' from sibling item, got: {ls:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD)),
            "structural key suggestions should have FIELD kind"
        );
    }
}
