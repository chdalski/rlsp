// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, NumberOrString, Position, Range, TextEdit,
    WorkspaceEdit,
};

use std::collections::HashMap;

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{CollectionStyle, Document, Span};

use crate::editing::formatter::{YamlFormatOptions, format_subtree};

/// Compute code actions available for the given text, range, and diagnostics.
///
/// Returns actions for:
/// - Converting flow mappings to block style (when cursor is on a `flowMap` diagnostic)
/// - Converting flow sequences to block style (when cursor is on a `flowSeq` diagnostic)
/// - Converting block mappings to flow style (when cursor is on a block mapping key)
/// - Replacing tabs with spaces (when the line contains tabs)
/// - Deleting unused anchors (when cursor is on an `unusedAnchor` diagnostic)
/// - Converting quoted booleans to unquoted (`"true"` -> `true`)
/// - Converting long strings to block scalars (`|` style)
#[must_use]
pub fn code_actions(
    docs: &[Document<Span>],
    text: &str,
    range: Range,
    diagnostics: &[Diagnostic],
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let lines: Vec<&str> = text.lines().collect();

    // Diagnostic-driven actions
    let diag_actions = diagnostics
        .iter()
        .filter(|diag| ranges_overlap(&diag.range, &range))
        .flat_map(|diag| match diagnostic_code(diag) {
            Some("flowMap") => flow_map_to_block(docs, text, diag, uri)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("flowSeq") => flow_seq_to_block(docs, text, diag, uri)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("unusedAnchor") => delete_unused_anchor(&lines, diag, uri)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("yaml11Boolean" | "schemaYaml11Boolean") => yaml11_bool_actions(&lines, diag, uri),
            Some("yaml11Octal" | "schemaYaml11Octal") => yaml11_octal_actions(&lines, diag, uri),
            Some("schemaYaml11BooleanType") => schema_yaml11_bool_type_actions(&lines, diag, uri),
            _ => vec![],
        });

    // Context-driven actions (not tied to diagnostics)
    let line_idx = range.start.line as usize;
    let context_actions: Vec<CodeAction> = lines.get(line_idx).map_or(vec![], |line| {
        [
            if line.contains('\t') {
                tab_to_spaces(&lines, line_idx, uri)
            } else {
                None
            },
            quoted_bool_to_unquoted(line, line_idx, range, uri),
            string_to_block_scalar(line, line_idx, uri),
            block_to_flow(docs, text, line_idx, uri),
        ]
        .into_iter()
        .flatten()
        .collect()
    });

    diag_actions.chain(context_actions).collect()
}

const fn diagnostic_code(diag: &Diagnostic) -> Option<&str> {
    match &diag.code {
        Some(NumberOrString::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

const fn ranges_overlap(a: &Range, b: &Range) -> bool {
    a.start.line <= b.end.line && b.start.line <= a.end.line
}

fn make_action(
    title: String,
    uri: &tower_lsp::lsp_types::Url,
    edits: Vec<TextEdit>,
    kind: CodeActionKind,
    diagnostics: Option<Vec<Diagnostic>>,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    CodeAction {
        title,
        kind: Some(kind),
        diagnostics,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        }),
        ..CodeAction::default()
    }
}

// ---------- Flow map to block ----------

fn flow_map_to_block(
    docs: &[Document<Span>],
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let node = find_flow_mapping(docs, diag)?;
    let Node::Mapping { loc, .. } = node else {
        return None;
    };

    let mut block_node = node.clone();
    if let Node::Mapping { style, .. } = &mut block_node {
        *style = CollectionStyle::Block;
    }

    let (new_text, edit_start_col) = block_text_and_start_col(&block_node, loc, text);
    if new_text.trim().is_empty() {
        return None;
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            edit_start_col as u32,
        ),
        Position::new(
            loc.end.line.saturating_sub(1) as u32,
            (loc.end.column + 1) as u32,
        ),
    );

    Some(make_action(
        "Convert flow mapping to block style".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        Some(vec![diag.clone()]),
    ))
}

/// Walk the AST to find a flow mapping node whose span matches the diagnostic range.
fn find_flow_mapping<'a>(docs: &'a [Document<Span>], diag: &Diagnostic) -> Option<&'a Node<Span>> {
    for doc in docs {
        if let Some(node) = find_flow_mapping_in_node(&doc.root, diag) {
            return Some(node);
        }
    }
    None
}

fn find_flow_mapping_in_node<'a>(
    node: &'a Node<Span>,
    diag: &Diagnostic,
) -> Option<&'a Node<Span>> {
    match node {
        Node::Mapping {
            style: CollectionStyle::Flow,
            loc,
            entries,
            ..
        } => {
            if span_matches_diag(loc, diag) {
                return Some(node);
            }
            // Search nested nodes
            for (k, v) in entries {
                if let Some(found) = find_flow_mapping_in_node(k, diag) {
                    return Some(found);
                }
                if let Some(found) = find_flow_mapping_in_node(v, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_flow_mapping_in_node(k, diag) {
                    return Some(found);
                }
                if let Some(found) = find_flow_mapping_in_node(v, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_flow_mapping_in_node(item, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

// ---------- Flow seq to block ----------

fn flow_seq_to_block(
    docs: &[Document<Span>],
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let node = find_flow_sequence(docs, diag)?;
    let Node::Sequence { loc, .. } = node else {
        return None;
    };

    let mut block_node = node.clone();
    if let Node::Sequence { style, .. } = &mut block_node {
        *style = CollectionStyle::Block;
    }

    let (new_text, edit_start_col) = block_text_and_start_col(&block_node, loc, text);
    if new_text.trim().is_empty() {
        return None;
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            edit_start_col as u32,
        ),
        Position::new(
            loc.end.line.saturating_sub(1) as u32,
            (loc.end.column + 1) as u32,
        ),
    );

    Some(make_action(
        "Convert flow sequence to block style".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        Some(vec![diag.clone()]),
    ))
}

/// Walk the AST to find a flow sequence node whose span matches the diagnostic range.
fn find_flow_sequence<'a>(docs: &'a [Document<Span>], diag: &Diagnostic) -> Option<&'a Node<Span>> {
    for doc in docs {
        if let Some(node) = find_flow_sequence_in_node(&doc.root, diag) {
            return Some(node);
        }
    }
    None
}

fn find_flow_sequence_in_node<'a>(
    node: &'a Node<Span>,
    diag: &Diagnostic,
) -> Option<&'a Node<Span>> {
    match node {
        Node::Sequence {
            style: CollectionStyle::Flow,
            loc,
            items,
            ..
        } => {
            if span_matches_diag(loc, diag) {
                return Some(node);
            }
            for item in items {
                if let Some(found) = find_flow_sequence_in_node(item, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_flow_sequence_in_node(item, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_flow_sequence_in_node(k, diag) {
                    return Some(found);
                }
                if let Some(found) = find_flow_sequence_in_node(v, diag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

/// Format a block node and compute the replacement text and edit start column.
///
/// When the node is a mapping value (`key: {…}` or `key: […]`), inserting block
/// items inline after `key: ` produces invalid YAML. In that case this function
/// produces `"\n<indent><first-item>\n<indent><second-item>..."` so the edit
/// replaces from the `{`/`[` with a newline-plus-properly-indented block form.
fn block_text_and_start_col(node: &Node<Span>, loc: &Span, text: &str) -> (String, usize) {
    let start_col = loc.start.column;
    let line_idx = loc.start.line.saturating_sub(1);
    let lines: Vec<&str> = text.lines().collect();

    let is_mapping_value = lines.get(line_idx).is_some_and(|line| {
        let prefix = if start_col <= line.len() {
            &line[..start_col]
        } else {
            line
        };
        prefix.trim_end().ends_with(':')
    });

    if is_mapping_value {
        let key_indent = lines
            .get(line_idx)
            .map_or(0, |line| line.len() - line.trim_start().len());
        let base_indent = key_indent + 2;
        let indent_str = " ".repeat(base_indent);
        // format_subtree: first line at col 0, continuation lines at base_indent.
        // We need all lines at base_indent, so we prepend "\n<indent>" before the first line.
        let formatted = format_subtree(node, &YamlFormatOptions::default(), base_indent);
        (format!("\n{indent_str}{formatted}"), start_col)
    } else {
        let formatted = format_subtree(node, &YamlFormatOptions::default(), start_col);
        (formatted, start_col)
    }
}

/// Check if a `Span` matches a diagnostic range, applying the `end_col + 1` convention
/// used by `flow_diagnostic` in validators.rs.
#[expect(
    clippy::cast_possible_truncation,
    reason = "LSP line/col are u32; always fits"
)]
const fn span_matches_diag(loc: &Span, diag: &Diagnostic) -> bool {
    let start_line = loc.start.line.saturating_sub(1) as u32;
    let start_col = loc.start.column as u32;
    let end_line = loc.end.line.saturating_sub(1) as u32;
    let end_col = (loc.end.column + 1) as u32;

    diag.range.start.line == start_line
        && diag.range.start.character == start_col
        && diag.range.end.line == end_line
        && diag.range.end.character == end_col
}

// ---------- Block to flow ----------

fn block_to_flow(
    docs: &[Document<Span>],
    text: &str,
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let (node, key_loc) = find_innermost_block_collection(docs, line_idx)?;
    let loc = node_loc(node);

    // Only offer the action if the block collection starts after the key line.
    // Block collections always begin on the line after their key, so this
    // filters out any degenerate cases.
    if loc.start.line <= key_loc.start.line {
        return None;
    }

    if has_nested_collection_child(node) {
        return None;
    }

    let mut flow_node = node.clone();
    match &mut flow_node {
        Node::Mapping { style, .. } | Node::Sequence { style, .. } => {
            *style = CollectionStyle::Flow;
        }
        Node::Scalar { .. } | Node::Alias { .. } => return None,
    }

    // Compute the edit text and range. The block collection is always the VALUE
    // of a mapping entry whose key starts on line_idx. The replacement emits the
    // flow form inline (` [one, two]` or ` { a: 1, b: 2 }`) replacing from the
    // end of the key line to the end of the block content.
    //
    // base_indent = key column + 2: when the flow output wraps across lines,
    // continuation lines must be indented further than the surrounding block
    // context. key_loc.start.column is 0-based (parser convention), so
    // base_indent = key_loc.start.column + 2 satisfies the YAML spec requirement
    // that flow continuation lines be indented more than the enclosing block.
    let base_indent = key_loc.start.column + 2;
    let lines: Vec<&str> = text.lines().collect();
    let key_line = key_loc.start.line.saturating_sub(1); // 0-based
    let formatted = format_subtree(&flow_node, &YamlFormatOptions::default(), base_indent);
    let new_text = format!(" {formatted}");

    if new_text.trim().is_empty() {
        return None;
    }

    let title = if new_text.len() > 80 {
        "Convert block to flow style (long line)".to_string()
    } else {
        "Convert block to flow style".to_string()
    };

    // Replace from the end of the key line to the end of the block content.
    let key_line_text = lines.get(key_line).copied().unwrap_or("");
    let edit_start_col = key_line_text.len();

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(key_line as u32, edit_start_col as u32),
        Position::new(
            loc.end.line.saturating_sub(1) as u32,
            (loc.end.column + 1) as u32,
        ),
    );

    Some(make_action(
        title,
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        None,
    ))
}

/// Walk the AST to find the block `Mapping` or `Sequence` to convert when the
/// cursor is on `line_idx` (LSP 0-based).
///
/// Returns the block collection node and the span of the mapping key whose line
/// matches the cursor. The key span is used to compute `base_indent` and the
/// edit range in `block_to_flow`.
///
/// Matches the VALUE of a block mapping entry whose KEY starts on the cursor
/// line, when that value is itself a block collection. This covers both
/// `items:\n  - a\n` (sequence value) and `config:\n  a: 1\n` (mapping value).
///
/// "Innermost" wins: if nested mapping entries also have keys on the cursor line,
/// the deepest match is used (later assignments in the DFS override earlier ones).
///
/// Note: matching on the parent KEY rather than the collection node itself is
/// equivalent to "collection whose start is on the cursor line" for block
/// collections (which always start on the line after their key), and
/// additionally provides the key's column for `base_indent` computation. Bare
/// top-level block sequences with no parent key are out of scope — the action
/// requires a `key:` colon to anchor the inline replacement.
fn find_innermost_block_collection<'a>(
    docs: &'a [Document<Span>],
    line_idx: usize,
) -> Option<(&'a Node<Span>, &'a Span)> {
    let parser_line = line_idx + 1;
    let mut best: Option<(&'a Node<Span>, &'a Span)> = None;
    for doc in docs {
        find_innermost_block_in_node(&doc.root, parser_line, &mut best);
    }
    best
}

fn find_innermost_block_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    best: &mut Option<(&'a Node<Span>, &'a Span)>,
) {
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                // If the key starts on the cursor line and the value is a block
                // collection, the value is the conversion target.
                if node_loc(k).start.line == parser_line && is_block_collection(v) {
                    *best = Some((v, node_loc(k)));
                }
                find_innermost_block_in_node(k, parser_line, best);
                find_innermost_block_in_node(v, parser_line, best);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                find_innermost_block_in_node(item, parser_line, best);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

const fn is_block_collection(node: &Node<Span>) -> bool {
    matches!(
        node,
        Node::Mapping {
            style: CollectionStyle::Block,
            ..
        } | Node::Sequence {
            style: CollectionStyle::Block,
            ..
        }
    )
}

/// Return true if any direct child of the given block collection node is
/// itself a block-style `Mapping` or `Sequence`. Flow-style nested collections
/// are already in a valid flow form and do not block the conversion.
fn has_nested_collection_child(node: &Node<Span>) -> bool {
    match node {
        Node::Mapping { entries, .. } => entries.iter().any(|(_, v)| is_block_collection(v)),
        Node::Sequence { items, .. } => items.iter().any(is_block_collection),
        Node::Scalar { .. } | Node::Alias { .. } => false,
    }
}

const fn node_loc(node: &Node<Span>) -> &Span {
    match node {
        Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Scalar { loc, .. }
        | Node::Alias { loc, .. } => loc,
    }
}

// ---------- Tab to spaces ----------

fn tab_to_spaces(
    lines: &[&str],
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let line = lines.get(line_idx)?;
    if !line.contains('\t') {
        return None;
    }

    let new_text = line.replace('\t', "  ");

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(line_idx as u32, 0),
        Position::new(line_idx as u32, line.len() as u32),
    );

    Some(make_action(
        "Convert tabs to spaces".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::QUICKFIX,
        None,
    ))
}

// ---------- Delete unused anchor ----------

fn delete_unused_anchor(
    lines: &[&str],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let line_idx = diag.range.start.line as usize;
    let line = lines.get(line_idx)?;
    let start_col = diag.range.start.character as usize;
    let end_col = diag.range.end.character as usize;

    if start_col >= line.len() || end_col > line.len() {
        return None;
    }

    // The anchor includes `&name` — remove it and any trailing space
    let before = &line[..start_col];
    let after = &line[end_col..];
    let after = after.strip_prefix(' ').unwrap_or(after);
    let new_text = format!("{before}{after}");

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(line_idx as u32, 0),
        Position::new(line_idx as u32, line.len() as u32),
    );

    Some(make_action(
        "Delete unused anchor".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::QUICKFIX,
        Some(vec![diag.clone()]),
    ))
}

// ---------- Quoted boolean to unquoted ----------

fn quoted_bool_to_unquoted(
    line: &str,
    line_idx: usize,
    range: Range,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let col = range.start.character as usize;

    // Look for quoted boolean patterns in the line
    for pattern in &["\"true\"", "\"false\"", "'true'", "'false'"] {
        if let Some(pos) = line.find(pattern) {
            // Check if the cursor is near this pattern
            let pattern_end = pos + pattern.len();
            if col <= pattern_end {
                let unquoted = &pattern[1..pattern.len() - 1];
                let before = &line[..pos];
                let after = &line[pattern_end..];
                let new_text = format!("{before}{unquoted}{after}");

                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "LSP line/col are u32; always fits"
                )]
                let edit_range = Range::new(
                    Position::new(line_idx as u32, 0),
                    Position::new(line_idx as u32, line.len() as u32),
                );

                return Some(make_action(
                    format!("Convert quoted string to {unquoted}"),
                    uri,
                    vec![TextEdit {
                        range: edit_range,
                        new_text,
                    }],
                    CodeActionKind::QUICKFIX,
                    None,
                ));
            }
        }
    }
    None
}

// ---------- String to block scalar ----------

fn string_to_block_scalar(
    line: &str,
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    // Match pattern: `key: "long string"` or `key: 'long string'` or `key: long string`
    let colon_pos = line.find(':')?;
    let after_colon = line[colon_pos + 1..].trim();

    // Need a string value that's long enough to benefit from block scalar
    let min_length = 40;

    let (value, is_quoted) = if (after_colon.starts_with('"') && after_colon.ends_with('"'))
        || (after_colon.starts_with('\'') && after_colon.ends_with('\''))
    {
        (&after_colon[1..after_colon.len() - 1], true)
    } else {
        (after_colon, false)
    };

    if value.len() < min_length {
        return None;
    }

    // Don't convert values that look like flow collections or special YAML
    if value.starts_with('{')
        || value.starts_with('[')
        || value.starts_with('&')
        || value.starts_with('*')
    {
        return None;
    }

    let base_indent = line.len() - line.trim_start().len();
    let indent_str = " ".repeat(base_indent + 2);
    let key_part = &line[..=colon_pos];

    // Use literal block scalar (|) — preserves newlines if present
    let block_value = if is_quoted {
        value.replace("\\n", &format!("\n{indent_str}"))
    } else {
        value.to_string()
    };

    let new_text = format!("{key_part} |\n{indent_str}{block_value}");

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(line_idx as u32, 0),
        Position::new(line_idx as u32, line.len() as u32),
    );

    Some(make_action(
        "Convert to block scalar".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::REFACTOR_REWRITE,
        None,
    ))
}

// ---------- YAML 1.1 boolean quick fixes ----------

fn yaml11_bool_actions(
    lines: &[&str],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let line_idx = diag.range.start.line as usize;
    let Some(line) = lines.get(line_idx) else {
        return vec![];
    };
    let start_col = diag.range.start.character as usize;
    let end_col = diag.range.end.character as usize;

    if start_col >= line.len() || end_col > line.len() {
        return vec![];
    }

    let value = &line[start_col..end_col];
    let before = &line[..start_col];
    let after = &line[end_col..];

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(diag.range.start.line, 0),
        Position::new(diag.range.start.line, line.len() as u32),
    );

    let quoted_text = format!("{before}\"{value}\"{after}");
    let canonical = crate::scalar_helpers::yaml11_bool_canonical(value);
    let converted_text = format!("{before}{canonical}{after}");

    vec![
        make_action(
            "Quote value".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: quoted_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
        make_action(
            "Convert to boolean".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: converted_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
    ]
}

// ---------- YAML 1.1 octal quick fixes ----------

fn yaml11_octal_actions(
    lines: &[&str],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let line_idx = diag.range.start.line as usize;
    let Some(line) = lines.get(line_idx) else {
        return vec![];
    };
    let start_col = diag.range.start.character as usize;
    let end_col = diag.range.end.character as usize;

    if start_col >= line.len() || end_col > line.len() {
        return vec![];
    }

    let value = &line[start_col..end_col];
    let before = &line[..start_col];
    let after = &line[end_col..];

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(diag.range.start.line, 0),
        Position::new(diag.range.start.line, line.len() as u32),
    );

    let quoted_text = format!("{before}\"{value}\"{after}");
    // Insert 'o' after the leading '0': "0755" → "0o755"
    let yaml12_octal = format!("0o{}", &value[1..]);
    let converted_text = format!("{before}{yaml12_octal}{after}");

    vec![
        make_action(
            "Quote as string".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: quoted_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
        make_action(
            "Convert to YAML 1.2 octal".to_string(),
            uri,
            vec![TextEdit {
                range: edit_range,
                new_text: converted_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
    ]
}

// ---------- Schema YAML 1.1 boolean type mismatch quick fixes ----------

fn schema_yaml11_bool_type_actions(
    lines: &[&str],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let line_idx = diag.range.start.line as usize;
    let Some(line) = lines.get(line_idx) else {
        return vec![];
    };
    let start_col = diag.range.start.character as usize;
    let end_col = diag.range.end.character as usize;

    if start_col >= line.len() || end_col > line.len() {
        return vec![];
    }

    let value = &line[start_col..end_col];
    let before = &line[..start_col];
    let after = &line[end_col..];

    if !crate::scalar_helpers::is_yaml11_bool(value) {
        return vec![];
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(diag.range.start.line, 0),
        Position::new(diag.range.start.line, line.len() as u32),
    );

    let canonical = crate::scalar_helpers::yaml11_bool_canonical(value);
    let converted_text = format!("{before}{canonical}{after}");

    vec![make_action(
        "Convert to boolean".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text: converted_text,
        }],
        CodeActionKind::QUICKFIX,
        Some(vec![diag.clone()]),
    )]
}

// ---------- Helpers ----------

/// Quote a block sequence item for use in a flow sequence if it contains
/// characters that are unsafe in flow context.
///
/// Already-quoted items (surrounded by matching `"…"` or `'…'`) are returned
/// as-is to prevent double-quoting.
///
/// Flow-unsafe: contains `,`, `[`, `]`, `{`, `}`, or starts with a character
/// that would cause ambiguity (`#`, `&`, `*`, `!`, `|`, `>`, `'`, `"`, `%`,
/// `@`, `` ` ``).
#[cfg(test)]
fn quote_flow_item(item: &str) -> String {
    if (item.len() >= 2 && item.starts_with('"') && item.ends_with('"'))
        || (item.len() >= 2 && item.starts_with('\'') && item.ends_with('\''))
    {
        return item.to_string();
    }
    let needs_quotes = item.contains([',', '[', ']', '{', '}'])
        || item.chars().next().is_some_and(|c| {
            matches!(
                c,
                '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`'
            )
        });
    if needs_quotes {
        format!("\"{item}\"")
    } else {
        item.to_string()
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cloned_ref_to_slice_refs,
    clippy::items_after_statements,
    reason = "test code"
)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::parser::parse_yaml;

    fn test_uri() -> tower_lsp::lsp_types::Url {
        tower_lsp::lsp_types::Url::parse("file:///test.yaml").unwrap()
    }

    fn cursor_range(line: u32, col: u32) -> Range {
        Range::new(Position::new(line, col), Position::new(line, col))
    }

    fn line_range(line: u32) -> Range {
        Range::new(Position::new(line, 0), Position::new(line, 999))
    }

    fn make_flow_diag(
        code: &str,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> Diagnostic {
        Diagnostic {
            range: Range::new(
                Position::new(start_line, start_char),
                Position::new(end_line, end_char),
            ),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some("rlsp-yaml".to_string()),
            ..Diagnostic::default()
        }
    }

    fn make_diagnostic(line: u32, start: u32, end: u32, code: &str) -> Diagnostic {
        make_flow_diag(code, line, start, line, end)
    }

    fn docs_for(text: &str) -> Vec<Document<Span>> {
        parse_yaml(text).documents
    }

    /// Apply the first block-to-flow edit to `text` and return the resulting string.
    fn apply_block_to_flow_edit(text: &str, line: u32) -> String {
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(line, 0),
            &[],
            &test_uri(),
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .expect("expected block-to-flow action");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let edit = &edits[0];
        let start_line = edit.range.start.line as usize;
        let start_col = edit.range.start.character as usize;
        let end_line = edit.range.end.line as usize;
        let end_col = edit.range.end.character as usize;
        let source_lines: Vec<&str> = text.lines().collect();
        let mut result = String::new();
        for (i, src_line) in source_lines.iter().enumerate() {
            if i < start_line || i > end_line {
                result.push_str(src_line);
                result.push('\n');
            } else if i == start_line && i == end_line {
                result.push_str(&src_line[..start_col]);
                result.push_str(&edit.new_text);
                result.push_str(&src_line[end_col..]);
                result.push('\n');
            } else if i == start_line {
                result.push_str(&src_line[..start_col]);
                result.push_str(&edit.new_text);
                result.push('\n');
            }
            // lines between start and end are replaced (absorbed into the edit)
        }
        result
    }

    // ---- Flow map to block ----

    #[test]
    fn should_convert_simple_flow_map_to_block() {
        let text = "config: {a: 1, b: 2}\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow mapping"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("a: 1"));
        assert!(edits[0].new_text.contains("b: 2"));
        assert!(!edits[0].new_text.contains('{'));
    }

    #[rstest]
    #[case::flow_map_invalid_range("config: {a: 1}\n", "flowMap", "flow mapping")]
    #[case::flow_seq_invalid_range("items: [a]\n", "flowSeq", "flow sequence")]
    #[case::unused_anchor_invalid_range("data: &unused\n", "unusedAnchor", "unused anchor")]
    fn invalid_range_produces_no_action(
        #[case] text: &str,
        #[case] code: &str,
        #[case] title_fragment: &str,
    ) {
        let diag = make_diagnostic(0, 100, 200, code);
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains(title_fragment)));
    }

    // ---- Flow seq to block ----

    #[test]
    fn should_convert_simple_flow_seq_to_block() {
        let text = "items: [one, two, three]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("- one"));
        assert!(edits[0].new_text.contains("- two"));
        assert!(edits[0].new_text.contains("- three"));
        assert!(!edits[0].new_text.contains('['));
    }

    #[test]
    fn should_indent_block_items_under_key_when_nested() {
        // Key at indent 6 — the AST-based rewrite replaces only the [...]  node.
        // Items in the new_text are indented relative to the node's start column.
        let text = "      command: [\"python\", \"-m\"]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("python"),
            "python must be present: {new_text:?}"
        );
        assert!(
            new_text.contains("\"-m\"") || new_text.contains("'-m'") || new_text.contains("- -m"),
            "second item must be present: {new_text:?}"
        );
        // edit replaces only the node span, not the key prefix
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 (key is preserved by caller): {:?}",
            edits[0].range
        );
    }

    #[test]
    fn should_indent_block_items_at_top_level_key() {
        // The AST-based rewrite replaces only the [...] node; key is preserved by the caller.
        let text = "items: [one, two]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("- one"),
            "one must be present: {new_text:?}"
        );
        assert!(
            new_text.contains("- two"),
            "two must be present: {new_text:?}"
        );
        // edit replaces only the node span
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 (key is preserved by caller): {:?}",
            edits[0].range
        );
    }

    #[test]
    fn should_indent_block_items_under_key_at_indent_2() {
        // The AST-based rewrite replaces only the [...] node; key is preserved by the caller.
        let text = "  command: [\"a\", \"b\"]\n";
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &flow_diags_for(text),
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("- a") || new_text.contains("- \"a\""),
            "a must be present: {new_text:?}"
        );
        assert!(
            new_text.contains("- b") || new_text.contains("- \"b\""),
            "b must be present: {new_text:?}"
        );
        // edit replaces only the node span
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 (key is preserved by caller): {:?}",
            edits[0].range
        );
    }

    // ---- Block to flow ----

    #[test]
    fn should_convert_block_mapping_to_flow() {
        let text = "config:\n  a: 1\n  b: 2\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("block to flow"));
        assert!(action.is_some());
        let edit = action.unwrap().edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("{ a: 1, b: 2 }"),
            "expected flow mapping with bracket spacing: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_convert_block_sequence_to_flow() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("block to flow"));
        assert!(action.is_some());
        let edit = action.unwrap().edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("[one, two]"));
    }

    #[test]
    fn should_not_offer_block_to_flow_for_inline_value() {
        let text = "key: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block to flow")));
    }

    #[test]
    fn should_not_offer_block_to_flow_for_nested_structures() {
        let text = "config:\n  a:\n    nested: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block to flow")));
    }

    #[test]
    fn should_handle_flow_sequence_item_when_converting_block_to_flow() {
        // The AST correctly represents `[nested]` as a flow sequence, not a raw string.
        // The converted output nests the flow sequence inside the outer flow sequence.
        let text = "args:\n  - [nested]\n  - safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[nested]"),
            "nested flow sequence must appear in output: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("safe"),
            "safe item should be present: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_quote_item_containing_comma_when_converting_block_to_flow() {
        let text = "args:\n  - a, b\n  - c\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"a, b\""),
            "comma-containing item must be quoted: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains('c'),
            "safe item should be present: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_not_quote_safe_items_when_converting_block_to_flow() {
        // Regression guard: safe items must not get unnecessary quotes.
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[one, two]"),
            "safe items should not be quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_append_long_line_warning_when_result_exceeds_80_chars() {
        let text = "items:\n  - long_item_aaa\n  - long_item_bbb\n  - long_item_ccc\n  - long_item_ddd\n  - long_item_eee\n  - long_item_fff\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        assert!(
            action.title.contains("(long line)"),
            "long result should include warning in title: {:?}",
            action.title
        );
    }

    #[test]
    fn should_not_append_long_line_warning_for_short_result() {
        let text = "items:\n  - a\n  - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        assert_eq!(
            action.title, "Convert block to flow style",
            "short result must not include long-line warning: {:?}",
            action.title
        );
    }

    // Reparse validation: long sequence at top level (key at column 0) must produce valid YAML
    #[test]
    fn should_produce_reparseable_yaml_when_long_sequence_wraps() {
        let text = "items:\n  - long_item_aaa\n  - long_item_bbb\n  - long_item_ccc\n  - long_item_ddd\n  - long_item_eee\n  - long_item_fff\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "edited YAML must reparse without diagnostics; got: {:?}\nresult text:\n{result}",
            parse_result.diagnostics
        );
        assert_eq!(
            parse_result.documents.len(),
            1,
            "edited YAML must produce exactly one document; result text:\n{result}"
        );
    }

    // Reparse validation: long nested mapping (cursor on inner key at column 2) must produce valid YAML
    #[test]
    fn should_produce_reparseable_yaml_when_long_nested_mapping_wraps() {
        let text = "outer:\n  inner:\n    key_aaa: val_aaa\n    key_bbb: val_bbb\n    key_ccc: val_ccc\n    key_ddd: val_ddd\n    key_eee: val_eee\n";
        let result = apply_block_to_flow_edit(text, 1);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "edited YAML must reparse without diagnostics; got: {:?}\nresult text:\n{result}",
            parse_result.diagnostics
        );
        assert_eq!(
            parse_result.documents.len(),
            1,
            "edited YAML must produce exactly one document; result text:\n{result}"
        );
    }

    // ---- Block to flow: new tests (groups B–G) ----

    // B-1: cursor on block collection start line → action offered
    #[test]
    fn should_offer_block_to_flow_when_cursor_is_on_block_collection_line() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action on block collection start line"
        );
    }

    // B-2: cursor on scalar line → no action (covered by should_not_offer_block_to_flow_for_inline_value)

    // B-3: cursor on child line (not collection start) → no action
    #[test]
    fn should_not_offer_block_to_flow_when_cursor_is_on_child_line_not_collection_start() {
        let text = "items:\n  - one\n  - two\n";
        // Cursor at line 1 (the `- one` line), not line 0 (the `items:` line)
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when cursor is on a child line, not the collection start"
        );
    }

    // B-4: innermost block on cursor line is targeted
    #[test]
    fn should_offer_block_to_flow_for_nested_block_when_cursor_is_on_inner_collection_line() {
        let text = "outer:\n  inner:\n    - a\n    - b\n";
        // Cursor at line 1 (the `inner:` line, LSP line 1)
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action targeting the inner sequence at cursor line 1"
        );
    }

    // B-5: cursor on comment line → no action
    #[test]
    fn should_not_offer_block_to_flow_when_no_block_collection_on_cursor_line() {
        let text = "# comment\nkey: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow on a comment-only line"
        );
    }

    // C-2: sequence items are mappings → no action
    #[test]
    fn should_not_offer_block_to_flow_when_sequence_item_is_nested_mapping() {
        let text = "items:\n  - key: val\n  - other: val\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when sequence items are mappings"
        );
    }

    // C-3: sequence item is a nested block sequence → no action
    #[test]
    fn should_not_offer_block_to_flow_when_sequence_item_is_nested_sequence() {
        let text = "matrix:\n  -\n    - a\n    - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when sequence items are nested sequences"
        );
    }

    // C-4: all sequence items are scalars → action offered
    #[test]
    fn should_offer_block_to_flow_when_all_sequence_items_are_scalars() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action when all sequence items are scalars"
        );
    }

    // C-5: all mapping values are scalars → action offered
    #[test]
    fn should_offer_block_to_flow_when_all_mapping_values_are_scalars() {
        let text = "point:\n  x: 1\n  y: 2\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "expected block-to-flow action when all mapping values are scalars"
        );
    }

    // D-1: top-level block sequence value → flow inline (no leading newline)
    #[test]
    fn should_emit_flow_inline_when_block_sequence_is_top_level_value() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            !edits[0].new_text.starts_with('\n'),
            "top-level value edit must not start with newline: {:?}",
            edits[0].new_text
        );
    }

    // D-2: block mapping that is itself a mapping value → flow emitted inline after key colon
    #[test]
    fn should_emit_flow_inline_when_block_is_mapping_value() {
        let text = "outer:\n  inner:\n    x: 1\n    y: 2\n";
        // Cursor at line 1 (the `inner:` line)
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        // The edit text starts with a space (` { x: 1, y: 2 }`) and is placed
        // inline after `inner:` — no leading newline.
        assert!(
            edits[0].new_text.starts_with(' '),
            "mapping-value block edit must start with space (inline placement): {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("x: 1") && edits[0].new_text.contains("y: 2"),
            "mapping keys must appear in output: {:?}",
            edits[0].new_text
        );
    }

    // G-1: empty document → no action
    #[test]
    fn should_not_offer_block_to_flow_for_empty_document() {
        let text = "\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow for empty document"
        );
    }

    // G-2: multi-document YAML — cursor on block collection in second document → action offered
    #[test]
    fn should_offer_block_to_flow_for_second_document_block_collection() {
        // 6-line file: doc 1 is lines 0-1, separator at line 2, doc 2 has `items:` at line 3
        let text = "key: value\n---\nother: stuff\nitems:\n  - a\n  - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(3, 0), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("block to flow")),
            "must offer block-to-flow for block collection in second document"
        );
    }

    // G-3: flow collection on cursor line → no action
    #[test]
    fn should_not_offer_block_to_flow_when_cursor_is_on_flow_collection_line() {
        let text = "items: [a, b]\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("block to flow")),
            "must not offer block-to-flow when collection is already flow style"
        );
    }

    // D-3: top-level block sequence (key at column 0) — base_indent = 0 + 2 = 2
    // Single-item input keeps output on one line so the space prefix is the only indentation.
    #[test]
    fn should_use_loc_start_column_as_base_indent_for_top_level_block() {
        let text = "items:\n  - a\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        // base_indent = key_column(0) + 2 = 2. Single-item flow fits on one line:
        // new_text = " [a]". Stripping the leading space leaves "[a]" with no additional indent.
        assert_eq!(
            new_text.trim_start(),
            "[a]",
            "top-level block must produce single-line flow text: {new_text:?}"
        );
    }

    // D-4: nested mapping value block (key at column 2) — base_indent = 2 + 2 = 4
    // Single-entry input keeps output on one line; the space prefix places it inline after `inner:`.
    #[test]
    fn should_use_key_indent_plus_2_as_base_indent_for_mapping_value_block() {
        let text = "outer:\n  inner:\n    x: 1\n";
        // Cursor at line 1 (the `inner:` line); key column = 2, so base_indent = 4.
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        // Single-entry flow fits on one line: " { x: 1 }". Starts with space (inline placement).
        assert!(
            new_text.starts_with(' ') && new_text.contains("x: 1"),
            "nested mapping value must produce inline flow with correct content: {new_text:?}"
        );
    }

    // ---- Tab to spaces ----

    #[test]
    fn should_convert_tabs_to_spaces() {
        let text = "\tkey: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("tabs to spaces"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "  key: value");
        assert!(!edits[0].new_text.contains('\t'));
    }

    #[test]
    fn should_not_offer_tab_conversion_without_tabs() {
        let text = "  key: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("tabs")));
    }

    // ---- Delete unused anchor ----

    #[test]
    fn should_delete_unused_anchor() {
        let text = "defaults: &unused value\n";
        let diag = make_diagnostic(0, 10, 17, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "defaults: value");
    }

    #[test]
    fn should_delete_anchor_at_end_of_value() {
        let text = "data: &unused\n";
        let diag = make_diagnostic(0, 6, 13, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "data: ");
    }

    // ---- Quoted bool to unquoted ----

    #[test]
    fn should_convert_double_quoted_true_to_unquoted() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("true")).unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: true");
    }

    #[test]
    fn should_convert_single_quoted_false_to_unquoted() {
        let text = "enabled: 'false'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("false")).unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: false");
    }

    #[test]
    fn should_not_offer_bool_conversion_for_non_bool_string() {
        let text = "name: \"hello\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    // ---- String to block scalar ----

    #[test]
    fn should_convert_long_string_to_block_scalar() {
        let long_value = "a".repeat(50);
        let text = format!("description: \"{long_value}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );

        let action = actions
            .iter()
            .find(|a| a.title.contains("block scalar"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("|\n"));
        assert!(edits[0].new_text.contains(&long_value));
    }

    #[test]
    fn should_not_offer_block_scalar_for_short_string() {
        let text = "key: \"short\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block scalar")));
    }

    #[test]
    fn should_not_offer_block_scalar_for_flow_collection() {
        let long_value = format!("{{{}:1}}", "a".repeat(50));
        let text = format!("key: {long_value}\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );

        assert!(actions.iter().all(|a| !a.title.contains("block scalar")));
    }

    // ---- Diagnostic overlap ----

    #[test]
    fn should_not_produce_actions_for_non_overlapping_diagnostics() {
        let text = "config: {a: 1}\nother: value\n";
        let diag = make_diagnostic(0, 8, 14, "flowMap");
        // Request actions for line 1, where the diagnostic is not
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(1, 0),
            &[diag],
            &test_uri(),
        );

        assert!(actions.iter().all(|a| !a.title.contains("flow mapping")));
    }

    // ---- Empty diagnostics ----

    #[test]
    fn should_return_empty_for_plain_yaml_no_diagnostics() {
        let text = "key: value\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        // No tabs, no quoted bools, no long strings, no block children
        assert!(actions.is_empty());
    }

    // ---- quote_flow_item ----

    #[rstest]
    #[case::double_quoted_passthrough("\"true\"", "\"true\"")]
    #[case::single_quoted_passthrough("'hello'", "'hello'")]
    #[case::plain_item_unchanged("plain", "plain")]
    #[case::comma_triggers_quoting("value, with comma", "\"value, with comma\"")]
    #[case::hash_prefix_triggers_quoting("#comment-like", "\"#comment-like\"")]
    #[case::brackets_trigger_quoting("[nested]", "\"[nested]\"")]
    // Starts with `"` but does not end with `"` — not a complete quoted string.
    // Gets wrapped: `"` + `"unclosed` + `"` = `""unclosed"`
    #[case::unclosed_opening_double_quote("\"unclosed", "\"\"unclosed\"")]
    // Ends with `"` but does not start with `"` — safe, returned as-is.
    #[case::only_trailing_double_quote("unclosed\"", "unclosed\"")]
    // Single `"` char: starts and ends with `"` but len == 1, so not pre-quoted.
    // Falls through to flow-unsafe path and gets wrapped: `"` + `"` + `"` = `"""`
    #[case::single_double_quote_char("\"", "\"\"\"")]
    fn quote_flow_item_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(quote_flow_item(input), expected);
    }

    #[test]
    fn should_preserve_double_quoted_item_when_converting_block_seq_to_flow() {
        let text = "items:\n  - \"true\"\n  - \"false\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[\"true\", \"false\"]"),
            "pre-quoted items must not be double-quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_preserve_single_quoted_item_when_converting_block_seq_to_flow() {
        // The formatter (preserve_quotes=false by default) strips unnecessary quotes
        // from safe plain scalars. `hello` and `world` are safe and appear unquoted.
        let text = "items:\n  - 'hello'\n  - 'world'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("hello") && edits[0].new_text.contains("world"),
            "single-quoted items must appear in flow output: {:?}",
            edits[0].new_text
        );
        assert!(
            !edits[0].new_text.contains("\"hello\"") && !edits[0].new_text.contains("\"world\""),
            "safe items must not be double-quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_quote_unsafe_item_alongside_pre_quoted_item() {
        let text = "args:\n  - \"true\"\n  - value, with comma\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0]
                .new_text
                .contains("[\"true\", \"value, with comma\"]"),
            "pre-quoted item preserved and unsafe item quoted: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_not_quote_plain_item_alongside_pre_quoted_item() {
        let text = "args:\n  - \"true\"\n  - plain\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("[\"true\", plain]"),
            "pre-quoted item preserved and plain item unquoted: {:?}",
            edits[0].new_text
        );
    }

    // ---- yaml11Boolean quick fixes ----

    #[test]
    fn should_quote_yaml11_bool_yes_lowercase() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: \"yes\"");
    }

    #[test]
    fn should_quote_yaml11_bool_uppercase_on() {
        let text = "flag: ON\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "flag: \"ON\"");
    }

    #[test]
    fn should_quote_yaml11_bool_with_indentation() {
        let text = "  enabled: yes\n";
        let diag = make_diagnostic(0, 11, 14, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "  enabled: \"yes\"");
    }

    #[test]
    fn should_not_offer_yaml11_bool_quote_for_non_overlapping_diagnostic() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 100, 103, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote value"));
    }

    #[test]
    fn should_convert_yaml11_bool_yes_to_true() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: true");
    }

    #[test]
    fn should_convert_yaml11_bool_no_to_false() {
        let text = "enabled: No\n";
        let diag = make_diagnostic(0, 9, 11, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: false");
    }

    #[test]
    fn should_convert_yaml11_bool_on_to_true() {
        let text = "flag: ON\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "flag: true");
    }

    #[test]
    fn should_convert_yaml11_bool_off_to_false() {
        let text = "flag: OFF\n";
        let diag = make_diagnostic(0, 6, 9, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "flag: false");
    }

    #[test]
    fn should_convert_yaml11_bool_y_to_true() {
        let text = "active: Y\n";
        let diag = make_diagnostic(0, 8, 9, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "active: true");
    }

    #[test]
    fn should_convert_yaml11_bool_n_to_false() {
        let text = "active: N\n";
        let diag = make_diagnostic(0, 8, 9, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "active: false");
    }

    #[test]
    fn should_convert_yaml11_bool_preserving_indentation() {
        let text = "  active: yes\n";
        let diag = make_diagnostic(0, 10, 13, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "  active: true");
    }

    #[test]
    fn yaml11_bool_produces_exactly_two_actions() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert_eq!(
            actions
                .iter()
                .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
                .count(),
            2
        );
    }

    #[test]
    fn yaml11_bool_actions_attach_diagnostic() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            std::slice::from_ref(&diag),
            &test_uri(),
        );

        for action in actions
            .iter()
            .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
        {
            let attached = action.diagnostics.as_ref().unwrap();
            assert_eq!(attached.len(), 1);
            assert_eq!(
                attached[0].code,
                Some(NumberOrString::String("yaml11Boolean".to_string()))
            );
        }
    }

    // ---- yaml11Octal quick fixes ----

    #[test]
    fn should_quote_yaml11_octal_0755() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "mode: \"0755\"");
    }

    #[test]
    fn should_quote_yaml11_octal_007() {
        let text = "file: 007\n";
        let diag = make_diagnostic(0, 6, 9, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "file: \"007\"");
    }

    #[test]
    fn should_quote_yaml11_octal_with_indentation() {
        let text = "  mode: 0755\n";
        let diag = make_diagnostic(0, 8, 12, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "  mode: \"0755\"");
    }

    #[test]
    fn should_not_offer_yaml11_octal_quote_for_out_of_bounds_range() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 100, 104, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote as string"));
    }

    #[test]
    fn should_convert_yaml11_octal_0755_to_yaml12() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "mode: 0o755");
    }

    #[test]
    fn should_convert_yaml11_octal_007_to_yaml12() {
        let text = "file: 007\n";
        let diag = make_diagnostic(0, 6, 9, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "file: 0o07");
    }

    #[test]
    fn should_convert_yaml11_octal_with_indentation() {
        let text = "  mode: 0755\n";
        let diag = make_diagnostic(0, 8, 12, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "  mode: 0o755");
    }

    #[test]
    fn yaml11_octal_produces_exactly_two_actions() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert_eq!(
            actions
                .iter()
                .filter(|a| {
                    a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal"
                })
                .count(),
            2
        );
    }

    #[test]
    fn yaml11_octal_actions_attach_diagnostic() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            std::slice::from_ref(&diag),
            &test_uri(),
        );

        for action in actions
            .iter()
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
        {
            let attached = action.diagnostics.as_ref().unwrap();
            assert_eq!(attached.len(), 1);
            assert_eq!(
                attached[0].code,
                Some(NumberOrString::String("yaml11Octal".to_string()))
            );
        }
    }

    #[test]
    fn yaml11_bool_on_line_other_than_zero() {
        let text = "key: value\nflag: yes\n";
        let diag = make_diagnostic(1, 6, 9, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edit = action.edit.as_ref().unwrap();
        let edits = &edit.changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].range.start.line, 1, "edit must target line 1");
        assert_eq!(edits[0].new_text, "flag: \"yes\"");
    }

    #[test]
    fn yaml11_octal_on_line_other_than_zero() {
        let text = "name: foo\nmode: 0755\n";
        let diag = make_diagnostic(1, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let edits = &edit.changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].range.start.line, 1, "edit must target line 1");
        assert_eq!(edits[0].new_text, "mode: \"0755\"");
    }

    #[test]
    fn yaml11_bool_diagnostic_not_triggered_by_other_codes() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 0, 12, "flowMap");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote value"));
        assert!(actions.iter().all(|a| a.title != "Convert to boolean"));
    }

    #[test]
    fn yaml11_octal_diagnostic_not_triggered_by_other_codes() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 0, 10, "flowSeq");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    // ════════════════════════════════════════════════════════════════════
    // Group E: schemaYaml11Boolean code actions
    // ════════════════════════════════════════════════════════════════════

    // E1: "Quote value" action replaces the plain value with a quoted string
    #[test]
    fn schema_yaml11_boolean_quote_value_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "flag: \"yes\"");
    }

    // E2: "Convert to boolean" action replaces the value with canonical YAML 1.2 boolean
    #[test]
    fn schema_yaml11_boolean_convert_to_boolean_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "flag: true");
    }

    // E3: exactly two actions offered for schemaYaml11Boolean
    #[test]
    fn schema_yaml11_boolean_offers_exactly_two_actions() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let count = actions
            .iter()
            .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
            .count();
        assert_eq!(count, 2);
    }

    // E4: both actions attach the triggering schemaYaml11Boolean diagnostic
    #[test]
    fn schema_yaml11_boolean_actions_attach_diagnostic() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            std::slice::from_ref(&diag),
            &test_uri(),
        );
        for action in actions
            .iter()
            .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
        {
            let diags = action.diagnostics.as_ref().unwrap();
            assert_eq!(diags.len(), 1);
            assert_eq!(
                diagnostic_code(&diags[0]),
                Some("schemaYaml11Boolean"),
                "action '{}' should attach schemaYaml11Boolean diagnostic",
                action.title
            );
        }
    }

    // E5: "Convert to boolean" maps false-family values to false
    #[test]
    fn schema_yaml11_boolean_converts_false_family_to_false() {
        let text = "flag: NO\n";
        let diag = make_diagnostic(0, 6, 8, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "flag: false");
    }

    // ════════════════════════════════════════════════════════════════════
    // Group F: schemaYaml11Octal code actions
    // ════════════════════════════════════════════════════════════════════

    // F1: "Quote as string" wraps the value in double quotes
    #[test]
    fn schema_yaml11_octal_quote_as_string_action() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "schemaYaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "mode: \"0755\"");
    }

    // F2: "Convert to YAML 1.2 octal" inserts 'o' after leading '0'
    #[test]
    fn schema_yaml11_octal_convert_to_yaml12_action() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "schemaYaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "mode: 0o755");
    }

    // F3: exactly two actions offered for schemaYaml11Octal
    #[test]
    fn schema_yaml11_octal_offers_exactly_two_actions() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "schemaYaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let count = actions
            .iter()
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
            .count();
        assert_eq!(count, 2);
    }

    // F4: both actions attach the triggering schemaYaml11Octal diagnostic
    #[test]
    fn schema_yaml11_octal_actions_attach_diagnostic() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "schemaYaml11Octal");
        let actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            std::slice::from_ref(&diag),
            &test_uri(),
        );
        for action in actions
            .iter()
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
        {
            let diags = action.diagnostics.as_ref().unwrap();
            assert_eq!(diags.len(), 1);
            assert_eq!(
                diagnostic_code(&diags[0]),
                Some("schemaYaml11Octal"),
                "action '{}' should attach schemaYaml11Octal diagnostic",
                action.title
            );
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // Group G: enhanced schemaYaml11BooleanType code action
    // ════════════════════════════════════════════════════════════════════

    // G1: "Convert to boolean" offered for schemaYaml11BooleanType diagnostic
    #[test]
    fn schema_yaml11_boolean_type_convert_to_boolean_action() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: true");
    }

    // G2: "Convert to boolean" maps false-family 1.1 values correctly
    #[test]
    fn schema_yaml11_boolean_type_converts_false_family_correctly() {
        let text = "enabled: OFF\n";
        let diag = make_diagnostic(0, 9, 12, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: false");
    }

    // G3: generic schemaType diagnostic (non-1.1 mismatch) does NOT offer "Convert to boolean"
    #[test]
    fn schema_type_generic_no_convert_to_boolean_action() {
        let text = "enabled: hello\n";
        // Use schemaType code (not schemaYaml11BooleanType) — generic mismatch
        let diag = make_diagnostic(0, 9, 14, "schemaType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "generic schemaType should not offer 'Convert to boolean': {actions:?}"
        );
    }

    // ---- AST-based flow_map_to_block (FM-* tests from test-engineer spec) ----

    fn flow_diags_for(text: &str) -> Vec<Diagnostic> {
        use crate::validation::validators::validate_flow_style;
        let docs = docs_for(text);
        validate_flow_style(&docs)
    }

    fn flow_map_action(text: &str) -> Option<CodeAction> {
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let diag = diags
            .iter()
            .find(|d| d.code == Some(NumberOrString::String("flowMap".to_string())))?;
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        code_actions(&docs, text, whole, &[diag.clone()], &test_uri())
            .into_iter()
            .find(|a| a.title.contains("flow mapping"))
    }

    fn flow_seq_action(text: &str) -> Option<CodeAction> {
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let diag = diags
            .iter()
            .find(|d| d.code == Some(NumberOrString::String("flowSeq".to_string())))?;
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        code_actions(&docs, text, whole, &[diag.clone()], &test_uri())
            .into_iter()
            .find(|a| a.title.contains("flow sequence"))
    }

    fn new_text_for(action: &CodeAction) -> String {
        action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()
            .get(&test_uri())
            .unwrap()[0]
            .new_text
            .clone()
    }

    // FM-1: sequence-item flow mapping produces clean block output, no data loss
    #[test]
    fn flow_map_sequence_item_no_data_loss() {
        let text = "- {target: linux, os: ubuntu}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("target: linux"), "new_text: {new_text:?}");
        assert!(new_text.contains("os: ubuntu"), "new_text: {new_text:?}");
        assert!(!new_text.contains('{'), "new_text: {new_text:?}");
        assert!(!new_text.contains('}'), "new_text: {new_text:?}");
        // edit range must NOT start at col 0 (should cover only the node, not the `- `)
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0];
        assert!(
            edit.range.start.character > 0,
            "range must not start at col 0: {:?}",
            edit.range
        );
    }

    // FM-2: multi-line flow mapping spans all input lines
    #[test]
    fn flow_map_multiline_spans_all_lines() {
        let text = "key:\n  {a: 1,\n  b: 2}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("a: 1"), "new_text: {new_text:?}");
        assert!(new_text.contains("b: 2"), "new_text: {new_text:?}");
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0];
        assert_eq!(
            edit.range.start.line, 1,
            "range must start on line 1: {:?}",
            edit.range
        );
        assert_eq!(
            edit.range.end.line, 2,
            "range must end on line 2: {:?}",
            edit.range
        );
    }

    // FM-3: flow mapping as mapping value — edit covers only the node
    #[test]
    fn flow_map_as_mapping_value_edit_covers_node_only() {
        let text = "key: {a: 1, b: 2}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("a: 1"), "new_text: {new_text:?}");
        assert!(new_text.contains("b: 2"), "new_text: {new_text:?}");
        // The edit range covers only the node span; the `key: ` prefix is preserved by the caller
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()][0];
        assert!(
            edit.range.start.character > 0,
            "edit should start past col 0: {:?}",
            edit.range
        );
    }

    // FM-4: nested flow mapping — no scalar loss
    #[test]
    fn flow_map_nested_no_data_loss() {
        let text = "outer: {inner: {x: 1}}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(
            new_text.contains('1'),
            "scalar value 1 must survive: {new_text:?}"
        );
        assert!(
            new_text.contains('x'),
            "scalar key x must survive: {new_text:?}"
        );
    }

    // FM-5: empty flow mapping — no action offered
    #[test]
    fn flow_map_empty_no_action() {
        let text = "key: {}\n";
        // Empty flow maps may or may not produce a flowMap diagnostic depending on
        // validator implementation; the important thing is no destructive action is offered
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());
        let has_map_action = actions.iter().any(|a| a.title.contains("flow mapping"));
        if has_map_action {
            // If an action is offered, ensure new_text is non-empty (not destructive)
            let action = actions
                .iter()
                .find(|a| a.title.contains("flow mapping"))
                .unwrap();
            let new_text = new_text_for(action);
            assert!(
                !new_text.is_empty(),
                "action for empty map must produce non-empty text"
            );
        }
        // The key acceptance criterion: empty collections stay inline via format_subtree
        // so if a diagnostic IS produced for `{}`, the action will not drop content
    }

    // FM-6: diagnostic range matches no AST node — no action offered
    #[test]
    fn flow_map_no_matching_node_no_action() {
        let text = "key: value\n";
        let docs = docs_for(text);
        let fake_diag = make_flow_diag("flowMap", 0, 5, 0, 10);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &[fake_diag], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("flow mapping")),
            "no matching node should yield no action"
        );
    }

    // FM-7: flow mapping with nested flow sequence as value — no data loss
    #[test]
    fn flow_map_with_nested_flow_seq_no_data_loss() {
        let text = "root: {a: [1, 2]}\n";
        let action = flow_map_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains('a'), "key a must survive: {new_text:?}");
        assert!(
            new_text.contains('1'),
            "scalar 1 must survive: {new_text:?}"
        );
        assert!(
            new_text.contains('2'),
            "scalar 2 must survive: {new_text:?}"
        );
    }

    // FM-4b: flow mapping nested inside a flow sequence — map-to-block action triggers on
    // the inner map only, leaving the outer sequence untouched.
    #[test]
    fn flow_map_to_block_inside_flow_seq_no_data_loss() {
        let text = "list: [{a: 1, b: 2}]\n";
        let action = flow_map_action(text).expect("flow-map action must be offered for inner map");
        let new_text = new_text_for(&action);
        assert!(new_text.contains('a'), "key a must survive: {new_text:?}");
        assert!(
            new_text.contains('1'),
            "scalar 1 must survive: {new_text:?}"
        );
        assert!(new_text.contains('b'), "key b must survive: {new_text:?}");
        assert!(
            new_text.contains('2'),
            "scalar 2 must survive: {new_text:?}"
        );
        assert!(
            !new_text.contains('{'),
            "block output must not contain '{{': {new_text:?}"
        );
    }

    // ---- AST-based flow_seq_to_block (FS-* tests from test-engineer spec) ----

    // FS-1: all-scalars flow sequence
    #[test]
    fn flow_seq_all_scalars_to_block() {
        let text = "list: [a, b, c]\n";
        let action = flow_seq_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(new_text.contains("- a"), "new_text: {new_text:?}");
        assert!(new_text.contains("- b"), "new_text: {new_text:?}");
        assert!(new_text.contains("- c"), "new_text: {new_text:?}");
        assert!(!new_text.contains('['), "new_text: {new_text:?}");
        assert!(!new_text.contains(']'), "new_text: {new_text:?}");
    }

    // FS-2: flow sequence of flow mappings — no data loss
    #[test]
    fn flow_seq_of_flow_maps_no_data_loss() {
        let text = "items: [{x: 1}, {x: 2}]\n";
        let action = flow_seq_action(text).expect("action must be offered");
        let new_text = new_text_for(&action);
        assert!(
            new_text.contains('1'),
            "scalar 1 must survive: {new_text:?}"
        );
        assert!(
            new_text.contains('2'),
            "scalar 2 must survive: {new_text:?}"
        );
    }

    // FS-3: empty flow sequence — no destructive action
    #[test]
    fn flow_seq_empty_no_destructive_action() {
        let text = "list: []\n";
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());
        let has_seq_action = actions.iter().any(|a| a.title.contains("flow sequence"));
        if has_seq_action {
            let action = actions
                .iter()
                .find(|a| a.title.contains("flow sequence"))
                .unwrap();
            let new_text = new_text_for(action);
            assert!(
                !new_text.is_empty(),
                "action for empty seq must produce non-empty text"
            );
        }
    }

    // ---- SIG-* signature compatibility tests ----

    // SIG-1: code_actions accepts empty docs slice
    #[test]
    fn sig_accepts_empty_docs() {
        let actions = code_actions(&[], "key: value\n", cursor_range(0, 0), &[], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains("flow")));
    }

    // SIG-2: parsed docs + matching flowMap diagnostic returns action
    #[test]
    fn sig_parsed_docs_with_flow_map_returns_action() {
        let text = "- {k: v}\n";
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("flow mapping")),
            "should return flow map action: {actions:?}"
        );
    }

    // SIG-3: non-flow diagnostics still produce actions when docs are provided
    #[test]
    fn sig_tab_action_still_works_with_docs() {
        let text = "\tkey: value\n";
        let docs = docs_for(text);
        let actions = code_actions(&docs, text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("tabs to spaces")),
            "tab action should still be offered: {actions:?}"
        );
    }

    // ---- INT-* integration tests (check_i4_scalar_preservation via code_actions) ----

    // INT-1: sequence-item flow map preserves all scalars end-to-end
    #[test]
    fn int_sequence_item_flow_map_preserves_all_scalars() {
        let text = "- {target: linux, os: ubuntu}\n";
        let docs = docs_for(text);
        let pre_scalars: Vec<String> = {
            let mut out = Vec::new();
            fn collect(node: &Node<Span>, out: &mut Vec<String>) {
                match node {
                    Node::Scalar { value, .. } => out.push(value.clone()),
                    Node::Mapping { entries, .. } => {
                        for (k, v) in entries {
                            collect(k, out);
                            collect(v, out);
                        }
                    }
                    Node::Sequence { items, .. } => {
                        for item in items {
                            collect(item, out);
                        }
                    }
                    Node::Alias { .. } => {}
                }
            }
            for doc in &docs {
                collect(&doc.root, &mut out);
            }
            out
        };

        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());

        for action in &actions {
            if action.kind.as_ref() != Some(&CodeActionKind::REFACTOR_REWRITE) {
                continue;
            }
            let Some(edits) = action
                .edit
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .and_then(|c| c.get(&test_uri()))
            else {
                continue;
            };
            if edits.is_empty() {
                continue;
            }

            // Apply the edit by replacing the node span
            let edit = &edits[0];
            let lines: Vec<&str> = text.lines().collect();
            let start_line = edit.range.start.line as usize;
            let end_line = edit.range.end.line as usize;
            let start_char = edit.range.start.character as usize;
            let end_char = edit.range.end.character as usize;

            let mut result = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i < start_line || i > end_line {
                    result.push_str(line);
                    result.push('\n');
                } else if i == start_line && i == end_line {
                    result.push_str(&line[..start_char]);
                    result.push_str(&edit.new_text);
                    result.push_str(&line[end_char..]);
                    result.push('\n');
                } else if i == start_line {
                    result.push_str(&line[..start_char]);
                    result.push_str(&edit.new_text);
                } else if i == end_line {
                    result.push_str(&line[end_char..]);
                    result.push('\n');
                }
            }

            let post_docs = docs_for(&result);
            let mut post_scalars = Vec::new();
            fn collect2(node: &Node<Span>, out: &mut Vec<String>) {
                match node {
                    Node::Scalar { value, .. } => out.push(value.clone()),
                    Node::Mapping { entries, .. } => {
                        for (k, v) in entries {
                            collect2(k, out);
                            collect2(v, out);
                        }
                    }
                    Node::Sequence { items, .. } => {
                        for item in items {
                            collect2(item, out);
                        }
                    }
                    Node::Alias { .. } => {}
                }
            }
            for doc in &post_docs {
                collect2(&doc.root, &mut post_scalars);
            }

            for scalar in &pre_scalars {
                assert!(
                    post_scalars.contains(scalar),
                    "scalar {scalar:?} was lost after applying action {:?}\npre: {pre_scalars:?}\npost: {post_scalars:?}\nresult: {result:?}",
                    action.title
                );
            }
        }
    }

    // INT-2: github-token expression value preserved
    #[test]
    fn int_github_token_expression_preserved() {
        let text = "env:\n  - {GITHUB_TOKEN: \"${{ secrets.GITHUB_TOKEN }}\"}\n";
        let docs = docs_for(text);
        let diags = flow_diags_for(text);
        let whole = Range::new(Position::new(0, 0), Position::new(999, 0));
        let actions = code_actions(&docs, text, whole, &diags, &test_uri());

        for action in &actions {
            if action.kind.as_ref() != Some(&CodeActionKind::REFACTOR_REWRITE) {
                continue;
            }
            let Some(edits) = action
                .edit
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .and_then(|c| c.get(&test_uri()))
            else {
                continue;
            };
            if edits.is_empty() {
                continue;
            }
            let new_text = &edits[0].new_text;
            assert!(
                new_text.contains("GITHUB_TOKEN"),
                "GITHUB_TOKEN key must be preserved: {new_text:?}"
            );
            assert!(
                new_text.contains("secrets.GITHUB_TOKEN"),
                "token value must be preserved: {new_text:?}"
            );
        }
    }
}
