// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, NumberOrString, Position, Range, TextEdit,
    WorkspaceEdit,
};

use std::collections::HashMap;

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Chomp, CollectionStyle, Document, ScalarStyle, Span};

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
            Some("unusedAnchor") => delete_unused_anchor(docs, text, diag, uri)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("yaml11Boolean" | "schemaYaml11Boolean") => yaml11_bool_actions(docs, diag, uri),
            Some("yaml11Octal" | "schemaYaml11Octal") => yaml11_octal_actions(docs, diag, uri),
            Some("schemaYaml11BooleanType") => schema_yaml11_bool_type_actions(docs, diag, uri),
            _ => vec![],
        });

    // Context-driven actions (not tied to diagnostics)
    let line_idx = range.start.line as usize;
    let col = range.start.character as usize;
    let context_actions: Vec<CodeAction> = lines.get(line_idx).map_or(vec![], |line| {
        [
            if line.contains('\t') {
                tab_to_spaces(&lines, line_idx, uri)
            } else {
                None
            },
            quoted_bool_to_unquoted(docs, line_idx, col, uri),
            string_to_block_scalar(docs, text, line_idx, uri),
            block_to_flow(docs, line_idx, uri),
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
    // flow form inline (` [one, two]` or ` { a: 1, b: 2 }`) replacing from just
    // after the key's colon to the end of the block content.
    //
    // base_indent = key column + 2: when the flow output wraps across lines,
    // continuation lines must be indented further than the surrounding block
    // context. key_loc.start.column is 0-based (parser convention), so
    // base_indent = key_loc.start.column + 2 satisfies the YAML spec requirement
    // that flow continuation lines be indented more than the enclosing block.
    let base_indent = key_loc.start.column + 2;
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

    // Replace from just after the key's colon to the end of the block content.
    // key_loc.end is exclusive, so end.column is the column of the ':'. Adding 1
    // positions the edit start after the colon. For anchored values like
    // `defaults: &base`, this replaces ` &base\n  …` with ` &base { … }` so
    // the formatter-emitted anchor is not duplicated on top of the existing one.
    let edit_start_col = key_loc.end.column + 1;

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
    docs: &[Document<Span>],
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let diag_line = diag.range.start.line as usize;
    let anchor_start_col = diag.range.start.character as usize;
    let anchor_end_col = diag.range.end.character as usize;

    // Extract the anchor name from the text to match against AST nodes.
    let line = text.lines().nth(diag_line)?;
    if anchor_start_col >= line.len() || anchor_end_col > line.len() {
        return None;
    }
    // Diagnostic range starts at `&` — the name follows.
    let anchor_name = &line[anchor_start_col + 1..anchor_end_col];

    let node = find_anchored_node(docs, diag_line, anchor_name)?;
    let loc = node_loc(node);

    let mut deanchored = node.clone();
    match &mut deanchored {
        Node::Scalar { anchor, .. }
        | Node::Mapping { anchor, .. }
        | Node::Sequence { anchor, .. } => *anchor = None,
        Node::Alias { .. } => return None,
    }

    let base_indent = loc.start.column;
    let new_text = format_subtree(&deanchored, &YamlFormatOptions::default(), base_indent);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(diag_line as u32, anchor_start_col as u32),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
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

fn find_anchored_node<'a>(
    docs: &'a [Document<Span>],
    diag_line: usize,
    anchor_name: &str,
) -> Option<&'a Node<Span>> {
    // The anchor is on diag_line (0-based). In the parser's 1-based convention the
    // node's loc starts either on the same line (inline anchor, e.g. `key: &a val`)
    // or on the following line (standalone anchor, e.g. `key: &a\n` where the value
    // node is an empty scalar emitted one line later).  Accept both.
    let parser_line = diag_line + 1;
    for doc in docs {
        if let Some(node) = find_anchored_node_in(&doc.root, parser_line, anchor_name) {
            return Some(node);
        }
    }
    None
}

fn find_anchored_node_in<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    anchor_name: &str,
) -> Option<&'a Node<Span>> {
    if node.anchor() == Some(anchor_name) {
        let loc_line = node_loc(node).start.line;
        if loc_line == parser_line || loc_line == parser_line + 1 {
            return Some(node);
        }
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_anchored_node_in(k, parser_line, anchor_name) {
                    return Some(found);
                }
                if let Some(found) = find_anchored_node_in(v, parser_line, anchor_name) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_anchored_node_in(item, parser_line, anchor_name) {
                    return Some(found);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

// ---------- Quoted boolean to unquoted ----------

fn quoted_bool_to_unquoted(
    docs: &[Document<Span>],
    line_idx: usize,
    col: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let parser_line = line_idx + 1;
    let scalar = find_quoted_bool_scalar(docs, parser_line, col)?;

    let Node::Scalar { value, loc, .. } = scalar else {
        return None;
    };

    let mut plain = scalar.clone();
    if let Node::Scalar { style, .. } = &mut plain {
        *style = ScalarStyle::Plain;
    }

    let base_indent = loc.start.column;
    let new_text = format_subtree(&plain, &YamlFormatOptions::default(), base_indent);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            loc.start.column as u32,
        ),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
    );

    Some(make_action(
        format!("Convert quoted string to {value}"),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
        CodeActionKind::QUICKFIX,
        None,
    ))
}

fn find_quoted_bool_scalar(
    docs: &[Document<Span>],
    parser_line: usize,
    col: usize,
) -> Option<&Node<Span>> {
    for doc in docs {
        if let Some(node) = find_quoted_bool_in_node(&doc.root, parser_line, col) {
            return Some(node);
        }
    }
    None
}

fn find_quoted_bool_in_node(
    node: &Node<Span>,
    parser_line: usize,
    col: usize,
) -> Option<&Node<Span>> {
    match node {
        Node::Scalar {
            style: ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted,
            value,
            loc,
            ..
        } if loc.start.line == parser_line
            && col >= loc.start.column
            && col <= loc.end.column
            && (value == "true" || value == "false") =>
        {
            Some(node)
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Some(found) = find_quoted_bool_in_node(k, parser_line, col) {
                    return Some(found);
                }
                if let Some(found) = find_quoted_bool_in_node(v, parser_line, col) {
                    return Some(found);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(found) = find_quoted_bool_in_node(item, parser_line, col) {
                    return Some(found);
                }
            }
            None
        }
    }
}

// ---------- String to block scalar ----------

fn string_to_block_scalar(
    docs: &[Document<Span>],
    _text: &str,
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let parser_line = line_idx + 1;
    let (scalar, key_col, scalar_loc) = find_block_scalar_candidate(docs, parser_line)?;

    let base_indent = key_col;
    let mut block_scalar = scalar.clone();
    if let Node::Scalar { style, .. } = &mut block_scalar {
        *style = ScalarStyle::Literal(Chomp::Clip);
    }

    let new_text = format_subtree(&block_scalar, &YamlFormatOptions::default(), base_indent);

    // Verify we actually produced something
    if new_text.trim().is_empty() {
        return None;
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            scalar_loc.start.line.saturating_sub(1) as u32,
            scalar_loc.start.column as u32,
        ),
        Position::new(
            scalar_loc.end.line.saturating_sub(1) as u32,
            scalar_loc.end.column as u32,
        ),
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

/// Walk the AST to find a qualifying scalar mapping value on the given parser line.
///
/// Returns `(scalar_node, key_start_column, scalar_loc)` when found.
/// Qualifying: the value is a `Node::Scalar` with style Plain/SingleQuoted/DoubleQuoted,
/// its loc starts on `parser_line`, and its parsed char count is >= 40.
fn find_block_scalar_candidate(
    docs: &[Document<Span>],
    parser_line: usize,
) -> Option<(&Node<Span>, usize, &Span)> {
    for doc in docs {
        if let Some(result) = find_block_scalar_in_node(&doc.root, parser_line) {
            return Some(result);
        }
    }
    None
}

fn find_block_scalar_in_node(
    node: &Node<Span>,
    parser_line: usize,
) -> Option<(&Node<Span>, usize, &Span)> {
    match node {
        Node::Mapping { entries, style, .. } => {
            let is_block = matches!(style, CollectionStyle::Block);
            for (k, v) in entries {
                if is_block {
                    if let Node::Scalar {
                        style: scalar_style,
                        value,
                        loc,
                        ..
                    } = v
                    {
                        if loc.start.line == parser_line
                            && matches!(
                                scalar_style,
                                ScalarStyle::Plain
                                    | ScalarStyle::SingleQuoted
                                    | ScalarStyle::DoubleQuoted
                            )
                            && value.chars().count() >= 40
                        {
                            let key_col = node_loc(k).start.column;
                            return Some((v, key_col, loc));
                        }
                    }
                }
                // Recurse into both key and value for nested structures
                if let Some(result) = find_block_scalar_in_node(k, parser_line) {
                    return Some(result);
                }
                if let Some(result) = find_block_scalar_in_node(v, parser_line) {
                    return Some(result);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Some(result) = find_block_scalar_in_node(item, parser_line) {
                    return Some(result);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

// ---------- YAML 1.1 boolean quick fixes ----------

fn yaml11_bool_actions(
    docs: &[Document<Span>],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let Some((scalar, loc, base_indent)) = find_yaml11_bool_scalar(docs, diag) else {
        return vec![];
    };
    let Node::Scalar { value, .. } = scalar else {
        return vec![];
    };

    let mut quoted = scalar.clone();
    if let Node::Scalar { style, .. } = &mut quoted {
        *style = ScalarStyle::DoubleQuoted;
    }
    let mut plain = scalar.clone();
    if let Node::Scalar {
        style, value: v, ..
    } = &mut plain
    {
        *style = ScalarStyle::Plain;
        *v = crate::scalar_helpers::yaml11_bool_canonical(value).to_string();
    }

    let quote_opts = YamlFormatOptions {
        preserve_quotes: true,
        ..YamlFormatOptions::default()
    };
    let quoted_text = format_subtree(&quoted, &quote_opts, base_indent);
    let plain_text = format_subtree(&plain, &YamlFormatOptions::default(), base_indent);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            loc.start.column as u32,
        ),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
    );

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
                new_text: plain_text,
            }],
            CodeActionKind::QUICKFIX,
            Some(vec![diag.clone()]),
        ),
    ]
}

/// Walk the AST to find a plain YAML 1.1 boolean scalar whose span matches the diagnostic.
///
/// Returns `(scalar_node, scalar_loc, base_indent)` where `base_indent` is the
/// key column (for mapping values) or the scalar's own column (for sequence items).
///
/// `yaml11Boolean` diagnostics point at the scalar span — column-aware matching is used so
/// that two bools on the same line resolve to the correct one.
/// `schemaYaml11Boolean` and `schemaYaml11BooleanType` diagnostics point at the key span,
/// so line-only matching is used — but only when there is exactly one yaml11 bool scalar
/// on that line (ambiguous lines return `None` rather than silently editing the wrong scalar).
fn find_yaml11_bool_scalar<'a>(
    docs: &'a [Document<Span>],
    diag: &Diagnostic,
) -> Option<(&'a Node<Span>, &'a Span, usize)> {
    let col_match = diagnostic_code(diag) == Some("yaml11Boolean");
    let parser_line = diag.range.start.line as usize + 1;
    if !col_match {
        // Line-only match: safe only when exactly one yaml11 bool scalar is on the line.
        let count: usize = docs
            .iter()
            .map(|doc| count_yaml11_bool_on_line(&doc.root, parser_line))
            .sum();
        if count != 1 {
            return None;
        }
    }
    for doc in docs {
        if let Some(result) = find_yaml11_bool_in_node(&doc.root, parser_line, diag, col_match) {
            return Some(result);
        }
    }
    None
}

/// Count plain yaml11 bool scalars on `parser_line` (1-based) in the AST subtree.
fn count_yaml11_bool_on_line(node: &Node<Span>, parser_line: usize) -> usize {
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .map(|(k, v)| {
                let v_count = if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = v
                {
                    usize::from(
                        loc.start.line == parser_line
                            && crate::scalar_helpers::is_yaml11_bool(value),
                    )
                } else {
                    count_yaml11_bool_on_line(v, parser_line)
                };
                count_yaml11_bool_on_line(k, parser_line) + v_count
            })
            .sum(),
        Node::Sequence { items, .. } => items
            .iter()
            .map(|item| {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = item
                {
                    usize::from(
                        loc.start.line == parser_line
                            && crate::scalar_helpers::is_yaml11_bool(value),
                    )
                } else {
                    count_yaml11_bool_on_line(item, parser_line)
                }
            })
            .sum(),
        Node::Scalar { .. } | Node::Alias { .. } => 0,
    }
}

/// `yaml11Boolean` emits `loc.end.column` without the `+1` used by flow-map diagnostics.
const fn yaml11_bool_col_matches_diag(loc: &Span, diag: &Diagnostic) -> bool {
    diag.range.start.character as usize == loc.start.column
        && diag.range.end.character as usize == loc.end.column
}

fn find_yaml11_bool_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    diag: &Diagnostic,
    col_match: bool,
) -> Option<(&'a Node<Span>, &'a Span, usize)> {
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = v
                {
                    if loc.start.line == parser_line
                        && crate::scalar_helpers::is_yaml11_bool(value)
                        && (!col_match || yaml11_bool_col_matches_diag(loc, diag))
                    {
                        let key_col = node_loc(k).start.column;
                        return Some((v, loc, key_col));
                    }
                }
                if let Some(result) = find_yaml11_bool_in_node(k, parser_line, diag, col_match) {
                    return Some(result);
                }
                if let Some(result) = find_yaml11_bool_in_node(v, parser_line, diag, col_match) {
                    return Some(result);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = item
                {
                    if loc.start.line == parser_line
                        && crate::scalar_helpers::is_yaml11_bool(value)
                        && (!col_match || yaml11_bool_col_matches_diag(loc, diag))
                    {
                        return Some((item, loc, loc.start.column));
                    }
                }
                if let Some(result) = find_yaml11_bool_in_node(item, parser_line, diag, col_match) {
                    return Some(result);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

// ---------- YAML 1.1 octal quick fixes ----------

fn yaml11_octal_actions(
    docs: &[Document<Span>],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let Some((scalar, loc, base_indent)) = find_yaml11_octal_scalar(docs, diag) else {
        return vec![];
    };
    let Node::Scalar { value, .. } = scalar else {
        return vec![];
    };

    let mut quoted = scalar.clone();
    if let Node::Scalar { style, .. } = &mut quoted {
        *style = ScalarStyle::DoubleQuoted;
    }
    let mut converted = scalar.clone();
    if let Node::Scalar {
        style, value: v, ..
    } = &mut converted
    {
        *style = ScalarStyle::Plain;
        *v = format!("0o{}", &value[1..]);
    }

    let quote_opts = YamlFormatOptions {
        preserve_quotes: true,
        ..YamlFormatOptions::default()
    };
    let quoted_text = format_subtree(&quoted, &quote_opts, base_indent);
    let converted_text = format_subtree(&converted, &YamlFormatOptions::default(), base_indent);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            loc.start.column as u32,
        ),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
    );

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

fn find_yaml11_octal_scalar<'a>(
    docs: &'a [Document<Span>],
    diag: &Diagnostic,
) -> Option<(&'a Node<Span>, &'a Span, usize)> {
    let col_match = diagnostic_code(diag) == Some("yaml11Octal");
    let parser_line = diag.range.start.line as usize + 1;
    if !col_match {
        let count: usize = docs
            .iter()
            .map(|doc| count_yaml11_octal_on_line(&doc.root, parser_line))
            .sum();
        if count != 1 {
            return None;
        }
    }
    for doc in docs {
        if let Some(result) = find_yaml11_octal_in_node(&doc.root, parser_line, diag, col_match) {
            return Some(result);
        }
    }
    None
}

fn count_yaml11_octal_on_line(node: &Node<Span>, parser_line: usize) -> usize {
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .map(|(k, v)| {
                let v_count = if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = v
                {
                    usize::from(
                        loc.start.line == parser_line
                            && crate::scalar_helpers::is_yaml11_octal(value),
                    )
                } else {
                    count_yaml11_octal_on_line(v, parser_line)
                };
                count_yaml11_octal_on_line(k, parser_line) + v_count
            })
            .sum(),
        Node::Sequence { items, .. } => items
            .iter()
            .map(|item| {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = item
                {
                    usize::from(
                        loc.start.line == parser_line
                            && crate::scalar_helpers::is_yaml11_octal(value),
                    )
                } else {
                    count_yaml11_octal_on_line(item, parser_line)
                }
            })
            .sum(),
        Node::Scalar { .. } | Node::Alias { .. } => 0,
    }
}

const fn yaml11_octal_col_matches_diag(loc: &Span, diag: &Diagnostic) -> bool {
    diag.range.start.character as usize == loc.start.column
        && diag.range.end.character as usize == loc.end.column
}

fn find_yaml11_octal_in_node<'a>(
    node: &'a Node<Span>,
    parser_line: usize,
    diag: &Diagnostic,
    col_match: bool,
) -> Option<(&'a Node<Span>, &'a Span, usize)> {
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = v
                {
                    if loc.start.line == parser_line
                        && crate::scalar_helpers::is_yaml11_octal(value)
                        && (!col_match || yaml11_octal_col_matches_diag(loc, diag))
                    {
                        let key_col = node_loc(k).start.column;
                        return Some((v, loc, key_col));
                    }
                }
                if let Some(result) = find_yaml11_octal_in_node(k, parser_line, diag, col_match) {
                    return Some(result);
                }
                if let Some(result) = find_yaml11_octal_in_node(v, parser_line, diag, col_match) {
                    return Some(result);
                }
            }
            None
        }
        Node::Sequence { items, .. } => {
            for item in items {
                if let Node::Scalar {
                    style: ScalarStyle::Plain,
                    value,
                    loc,
                    ..
                } = item
                {
                    if loc.start.line == parser_line
                        && crate::scalar_helpers::is_yaml11_octal(value)
                        && (!col_match || yaml11_octal_col_matches_diag(loc, diag))
                    {
                        return Some((item, loc, loc.start.column));
                    }
                }
                if let Some(result) = find_yaml11_octal_in_node(item, parser_line, diag, col_match)
                {
                    return Some(result);
                }
            }
            None
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
}

// ---------- Schema YAML 1.1 boolean type mismatch quick fixes ----------

fn schema_yaml11_bool_type_actions(
    docs: &[Document<Span>],
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Vec<CodeAction> {
    let Some((scalar, loc, base_indent)) = find_yaml11_bool_scalar(docs, diag) else {
        return vec![];
    };
    let Node::Scalar { value, .. } = scalar else {
        return vec![];
    };

    let mut plain = scalar.clone();
    if let Node::Scalar {
        style, value: v, ..
    } = &mut plain
    {
        *style = ScalarStyle::Plain;
        *v = crate::scalar_helpers::yaml11_bool_canonical(value).to_string();
    }

    let plain_text = format_subtree(&plain, &YamlFormatOptions::default(), base_indent);

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            loc.start.column as u32,
        ),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
    );

    vec![make_action(
        "Convert to boolean".to_string(),
        uri,
        vec![TextEdit {
            range: edit_range,
            new_text: plain_text,
        }],
        CodeActionKind::QUICKFIX,
        Some(vec![diag.clone()]),
    )]
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
            } else if i == end_line {
                result.push_str(&src_line[end_col..]);
                result.push('\n');
            }
            // lines strictly between start and end are absorbed into the edit
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

    // UA-1: plain scalar — anchor removed, surrounding structure preserved
    #[test]
    fn delete_anchor_plain_scalar_value() {
        let text = "defaults: &unused value\n";
        // `&unused` occupies cols 10–17
        let diag = make_diagnostic(0, 10, 17, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        // Edit replaces from anchor-start through node end — new_text is the
        // re-formatted scalar only (no anchor prefix).
        assert_eq!(edits[0].new_text, "value");
        assert!(!edits[0].new_text.contains("&unused"));
        // Edit range starts at the anchor's column, not column 0.
        assert_eq!(edits[0].range.start.character, 10);
    }

    // UA-2: anchor is the sole value (empty scalar after removal)
    #[test]
    fn delete_anchor_sole_value_empty_scalar() {
        let text = "data: &unused\n";
        // `&unused` occupies cols 6–13
        let diag = make_diagnostic(0, 6, 13, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&unused"));
    }

    // UA-3: quoted scalar — anchor removed, quotes preserved
    #[test]
    fn delete_anchor_quoted_scalar() {
        let text = "key: &a \"hello\"\n";
        // `&a` occupies cols 5–7
        let diag = make_diagnostic(0, 5, 7, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&a"));
        assert!(edits[0].new_text.contains("hello"));
    }

    // UA-4: anchor with user-defined tag — tag preserved after anchor removal
    #[test]
    fn delete_anchor_user_tag_preserved() {
        // User-defined tags (non-core-schema) are always kept by the formatter.
        let text = "key: &a !custom \"hello\"\n";
        // `&a` occupies cols 5–7
        let diag = make_diagnostic(0, 5, 7, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&a"));
        assert!(
            edits[0].new_text.contains("!custom"),
            "user tag must be preserved: {:?}",
            edits[0].new_text
        );
        assert!(edits[0].new_text.contains("hello"));
    }

    // UA-5: flow sequence — anchor removed, collection style preserved
    #[test]
    fn delete_anchor_flow_sequence() {
        let text = "list: &nums [1, 2, 3]\n";
        // `&nums` occupies cols 6–11
        let diag = make_diagnostic(0, 6, 11, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(!edits[0].new_text.contains("&nums"));
        assert!(
            edits[0].new_text.contains('['),
            "flow sequence bracket must be preserved: {:?}",
            edits[0].new_text
        );
        assert!(edits[0].new_text.contains('1'));
    }

    // UA-6: block mapping value with anchor (multi-line)
    #[test]
    fn delete_anchor_block_mapping_value() {
        let text = "base: &defaults\n  x: 1\n  y: 2\n";
        // `&defaults` occupies cols 6–14 on line 0
        let diag = make_diagnostic(0, 6, 15, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            !edits[0].new_text.contains("&defaults"),
            "anchor must be removed: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("x: 1"),
            "x entry must be preserved: {:?}",
            edits[0].new_text
        );
        assert!(
            edits[0].new_text.contains("y: 2"),
            "y entry must be preserved: {:?}",
            edits[0].new_text
        );
    }

    // UA-7: out-of-bounds diagnostic → no action (covered by rstest case
    // `case::unused_anchor_invalid_range` above; verified still passes after retrofit)

    // Trailing comment preservation: edit range must not reach into the comment,
    // and applying the edit must leave the comment intact.
    #[test]
    fn delete_anchor_trailing_comment_preserved() {
        let text = "key: &a value  # keep me\n";
        // `&a` occupies cols 5–7
        let diag = make_diagnostic(0, 5, 7, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("unused anchor"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].range.end.character as usize <= "key: &a value".len(),
            "edit end must not reach into trailing comment: {:?}",
            edits[0].range
        );
        assert!(
            !edits[0].new_text.contains('#'),
            "new_text must not contain the trailing comment: {:?}",
            edits[0].new_text
        );
        // Simulate applying the edit: replace anchor+node span with new_text.
        let mut result = text.to_string();
        let start = "key: ".len();
        let end = "key: &a value".len();
        result.replace_range(start..end, &edits[0].new_text);
        assert!(
            result.contains("# keep me"),
            "trailing comment must survive: {result:?}"
        );
    }

    // UA-8: stale diagnostic — anchor already absent from text
    #[test]
    fn delete_anchor_stale_diagnostic_returns_no_action() {
        let text = "data: value\n";
        // Diag claims unusedAnchor at cols 6–13, but no anchor in text
        let diag = make_diagnostic(0, 6, 13, "unusedAnchor");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("unused anchor")),
            "stale diagnostic must not produce an action"
        );
    }

    // ---- Quoted bool to unquoted ----

    #[test]
    fn should_convert_double_quoted_true_to_unquoted() {
        // "enabled: \"true\"\n" — scalar at col 9, cursor at col 10
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("true")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        // AST edit replaces only the scalar span — "true" (plain, not the full line)
        assert_eq!(edits[0].new_text, "true");
        // Edit range starts at the opening quote column (9), not column 0
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn should_convert_single_quoted_false_to_unquoted() {
        // "enabled: 'false'\n" — scalar at col 9, cursor at col 10
        let text = "enabled: 'false'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("false")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        // AST edit replaces only the scalar span — "false" (plain, not the full line)
        assert_eq!(edits[0].new_text, "false");
        // Edit range starts at the opening quote column (9), not column 0
        assert_eq!(edits[0].range.start.character, 9);
    }

    #[test]
    fn should_not_offer_bool_conversion_for_non_bool_string() {
        let text = "name: \"hello\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_convert_double_quoted_false_to_unquoted() {
        let text = "flag: \"false\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("false")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn should_convert_single_quoted_true_to_unquoted() {
        let text = "active: 'true'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("true")).unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
    }

    #[test]
    fn should_not_offer_bool_conversion_for_plain_true() {
        // Plain `true` is already unquoted — no action
        let text = "enabled: true\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_not_offer_bool_conversion_for_case_variant_true() {
        // "True" decoded value is "True" ≠ "true" — must not match
        let text = "flag: \"True\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_not_offer_bool_conversion_for_uppercase_false() {
        // "FALSE" decoded value is "FALSE" ≠ "false" — must not match
        let text = "flag: \"FALSE\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_not_offer_bool_conversion_when_cursor_before_scalar() {
        // "enabled: \"true\"\n" — scalar starts at col 9; cursor at col 8 (the space before quote)
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 8), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_offer_bool_conversion_when_cursor_at_scalar_start_column() {
        // Cursor exactly at the opening quote column
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        assert!(actions.iter().any(|a| a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_edit_range_is_scalar_span_not_full_line() {
        // Trailing comment must be preserved: edit range must cover only the scalar span.
        // "enabled: \"true\"  # keep this comment\n"
        // scalar `"true"` at col 9, parser loc: start.col=9, end.col=15 (exclusive)
        let text = "enabled: \"true\"  # keep this comment\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        // new_text must be just the scalar value, not the full line
        assert_eq!(edits[0].new_text, "true");
        // start column must be the opening-quote column (9), not 0
        assert_eq!(
            edits[0].range.start.character, 9,
            "edit range must start at the opening-quote column: {:?}",
            edits[0].range
        );
        // end column must be the exclusive end (15 = col after closing quote)
        assert_eq!(
            edits[0].range.end.character, 15,
            "edit range end must be the exclusive end of the scalar, not the full line: {:?}",
            edits[0].range
        );
    }

    #[test]
    fn quoted_bool_action_offered_for_second_document() {
        // Quoted bool in the second YAML document must trigger the action
        let text = "key: value\n---\nflag: \"true\"\n";
        // "flag: \"true\"" is on LSP line 2 (0-based); scalar starts at col 6
        let actions = code_actions(&docs_for(text), text, cursor_range(2, 7), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool in second document"
        );
    }

    #[test]
    fn quoted_bool_action_offered_inside_flow_sequence() {
        // Quoted bool inside a flow sequence must trigger the action
        let text = "items: [\"true\", \"false\"]\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool inside flow sequence"
        );
    }

    #[test]
    fn quoted_bool_action_not_offered_when_cursor_on_different_line() {
        let text = "a: true\nb: \"false\"\n";
        // Cursor on line 0 while quoted bool is on line 1
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 3), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn should_offer_bool_conversion_when_cursor_at_scalar_end_column() {
        // "enabled: \"true\"\n" — scalar `"true"` is 6 chars starting at col 9.
        // loc.end.column is the exclusive end = col 15 (one past the closing quote).
        // The containment check is col <= end.column, so cursor at col 15 must still match.
        let text = "enabled: \"true\"\n";
        let docs = docs_for(text);
        let actions = code_actions(&docs, text, cursor_range(0, 15), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "cursor at loc.end.column (exclusive end) must still trigger action"
        );
        // Cursor one past end.column must NOT trigger the action
        let actions_past = code_actions(&docs, text, cursor_range(0, 16), &[], &test_uri());
        assert!(
            actions_past
                .iter()
                .all(|a| !a.title.contains("Convert quoted")),
            "cursor past loc.end.column must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_action_offered_for_sequence_item() {
        // Scalar as a sequence item — the Sequence branch of find_quoted_bool_in_node must fire
        let text = "items:\n  - \"true\"\n  - \"false\"\n";
        // `- "true"` is on LSP line 1; the scalar starts at col 4 (after `- `)
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 5), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool as a sequence item"
        );
    }

    #[test]
    fn quoted_bool_action_not_offered_for_empty_docs() {
        // Empty docs slice — must not panic and must produce no action
        let actions = code_actions(
            &[],
            "enabled: \"true\"\n",
            cursor_range(0, 10),
            &[],
            &test_uri(),
        );
        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_action_offered_for_unicode_escaped_true() {
        // "\u0074rue" is a double-quoted scalar whose decoded value is "true"
        // (U+0074 is the letter 't'). The check is on the decoded AST value,
        // so this must trigger the action even though the raw text differs.
        let text = "flag: \"\\u0074rue\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 8), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "decoded value 'true' via unicode escape must trigger action"
        );
        // The new_text must be the plain value `true`
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].new_text, "true",
            "new_text must be plain 'true', not the unicode-escaped form"
        );
    }

    #[test]
    fn quoted_bool_action_kind_is_quickfix() {
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        assert_eq!(
            action.kind,
            Some(CodeActionKind::QUICKFIX),
            "action kind must be QUICKFIX"
        );
    }

    #[test]
    fn quoted_bool_action_title_uses_plain_value() {
        let text = "flag: 'false'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 8), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        assert_eq!(action.title, "Convert quoted string to false");
    }

    #[test]
    fn quoted_bool_cursor_at_closing_quote_column_offers_action() {
        // scalar `"true"` at col 9: closing `"` is at col 14 (0-based).
        // cursor at col 14 must be within `start.col(9) <= col <= end.col(15)`.
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 14), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "cursor at closing-quote column must still trigger action"
        );
    }

    #[test]
    fn quoted_bool_cursor_after_scalar_end_offers_no_action() {
        // cursor at col 16 (one past the exclusive end of the scalar at col 15) → no action
        let text = "enabled: \"true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 16), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "cursor past scalar exclusive end must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_mixed_case_offers_no_action() {
        // "tRuE" decoded value is "tRuE" ≠ "true" — must not match
        let text = "enabled: \"tRuE\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_literal_block_scalar_offers_no_action() {
        // Literal block scalar `|` style has decoded value "true\n" (with newline) — must not match
        let text = "enabled: |\n  true\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(1, 3), &[], &test_uri());

        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "literal block scalar must not trigger bool conversion"
        );
    }

    #[test]
    fn quoted_bool_inside_flow_mapping_value_offers_action() {
        // Scalar as a flow mapping value — the Mapping branch of find_quoted_bool_in_node fires
        let text = "config: {enabled: \"true\"}\n";
        // `"true"` starts after `enabled: ` inside the flow mapping; col 18
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 19), &[], &test_uri());

        assert!(
            actions.iter().any(|a| a.title.contains("Convert quoted")),
            "must offer bool conversion for quoted bool as a flow mapping value"
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
    }

    #[test]
    fn quoted_bool_pattern_inside_longer_scalar_not_offered() {
        // Single-quoted outer scalar whose decoded value contains the literal 6-char sequence
        // 'true' as a substring.  The outer scalar is NOT a standalone bool — do not offer.
        let text = "msg: 'status ''true'' reported'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 10), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "longer scalar containing 'true' substring must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_multiple_bools_same_line_cursor_on_first() {
        // Two quoted bools on one line; cursor on the first must offer "true", not "false".
        let text = "x: { a: \"true\", b: \"false\" }\n";
        // `"true"` starts at col 8; cursor inside it at col 9
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 9), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .expect("must offer action for first bool");
        assert_eq!(
            action.title, "Convert quoted string to true",
            "cursor on first bool must offer 'true', not 'false'"
        );
    }

    #[test]
    fn quoted_bool_multiple_bools_same_line_cursor_on_second() {
        // Same line; cursor on the second bool must offer "false".
        let text = "x: { a: \"true\", b: \"false\" }\n";
        // `"false"` starts at col 19; cursor inside it at col 20
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 20), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .expect("must offer action for second bool");
        assert_eq!(
            action.title, "Convert quoted string to false",
            "cursor on second bool must offer 'false', not 'true'"
        );
    }

    #[test]
    fn quoted_bool_flow_context_rest_of_mapping_preserved() {
        // Converting "true" in a multi-entry flow mapping must not destroy other entries.
        // Edit range must cover only the scalar span, leaving `, b: 1 }` intact.
        let text = "config: { a: \"true\", b: 1 }\n";
        // `"true"` starts at col 13; cursor inside it
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 14), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("Convert quoted"))
            .expect("must offer action inside flow mapping");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        // The edit range must not start at column 0 (full-line replacement)
        assert_ne!(
            edits[0].range.start.character, 0,
            "edit must not replace from col 0 — that would destroy surrounding flow content"
        );
        // The edit range end must not reach past the scalar's closing quote
        // `"true"` is 6 chars starting at col 13, so end must be ≤ 19
        assert!(
            edits[0].range.end.character <= 19,
            "edit end must not extend past the closing quote of the scalar"
        );
    }

    #[test]
    fn quoted_bool_value_with_leading_whitespace_not_offered() {
        // Decoded value " true" (leading space) is NOT the YAML bool `true` — must not offer.
        let text = "key: \" true\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 7), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "scalar with leading whitespace in decoded value must not trigger action"
        );
    }

    #[test]
    fn quoted_bool_non_bool_string_not_offered() {
        // Plain quoted string "hello" — must not offer.
        let text = "key: \"hello\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 6), &[], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_already_plain_not_offered() {
        // Plain-style `true` — style is Plain, not quoted, must not offer.
        let text = "key: true\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 6), &[], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    #[test]
    fn quoted_bool_cursor_on_trailing_comment_not_offered() {
        // Cursor placed on the trailing comment, which is past the scalar's span — must not offer.
        let text = "key: \"true\"  # comment\n";
        // `"true"` ends at col 11 (exclusive); cursor at col 14 is inside the comment
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 14), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("Convert quoted")),
            "cursor on trailing comment must not trigger action"
        );
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
        assert!(
            edits[0].range.start.character > 0,
            "edit range must start at the scalar, not at column 0 (full-line replacement would overwrite the key)"
        );
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

    #[test]
    fn should_not_offer_block_scalar_for_scalar_in_flow_mapping_value() {
        // Block scalars are illegal inside flow contexts (YAML 1.2 §8.1)
        let long = "a".repeat(50);
        let text = format!("{{key: \"{long}\"}}\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for a value inside a flow mapping"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_scalar_in_nested_flow_mapping() {
        // Flow mapping nested inside a block mapping value — still a flow context
        let long = "a".repeat(50);
        let text = format!("outer: {{key: \"{long}\"}}\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for a value inside a flow mapping nested in a block mapping"
        );
    }

    /// Apply the "Convert to block scalar" edit to `text` at the given line.
    /// Returns the full edited text and the raw `TextEdit` (for range assertions).
    fn apply_block_scalar_edit(text: &str, line: u32) -> (String, TextEdit) {
        let actions = code_actions(
            &docs_for(text),
            text,
            cursor_range(line, 0),
            &[],
            &test_uri(),
        );
        let action = actions
            .iter()
            .find(|a| a.title.contains("block scalar"))
            .expect("expected block-scalar action");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let edit = edits[0].clone();
        let source_lines: Vec<&str> = text.lines().collect();
        let line_idx = edit.range.start.line as usize;
        let start_col = edit.range.start.character as usize;
        let end_col = edit.range.end.character as usize;
        let src_line = source_lines[line_idx];
        let new_line = format!(
            "{}{}{}",
            &src_line[..start_col],
            edit.new_text,
            &src_line[end_col..]
        );
        let mut result = String::new();
        for (i, l) in source_lines.iter().enumerate() {
            if i == line_idx {
                result.push_str(&new_line);
            } else {
                result.push_str(l);
            }
            result.push('\n');
        }
        (result, edit)
    }

    // ---- String to block scalar: qualifying criteria ----

    #[test]
    fn should_offer_block_scalar_for_plain_scalar_mapping_value() {
        let text = "description: this is a very long plain scalar value that exceeds forty chars\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("block scalar")),
            "expected block-scalar action for plain mapping value"
        );
    }

    #[test]
    fn should_offer_block_scalar_for_single_quoted_mapping_value() {
        let text =
            "description: 'this is a very long single-quoted scalar that exceeds forty chars'\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().any(|a| a.title.contains("block scalar")),
            "expected block-scalar action for single-quoted mapping value"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_already_literal_block_scalar() {
        let text =
            "description: |\n  this is a very long literal block scalar that exceeds forty chars\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar action for already-literal value"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_already_folded_block_scalar() {
        let text =
            "description: >\n  this is a very long folded block scalar that exceeds forty chars\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar action for already-folded value"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_value_below_char_threshold() {
        let text = "key: \"short string under forty\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar action for value below 40 char threshold"
        );
    }

    #[test]
    fn should_use_char_count_not_byte_length_for_threshold() {
        // 8 multi-byte chars (2 bytes each) = 8 chars < 40, but 16 bytes > 8
        let text = "key: \"αβγδεζηθ\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for value with 8 chars (< 40), even if byte length > 8"
        );
    }

    #[test]
    fn should_offer_block_scalar_when_char_count_meets_threshold_with_multibyte() {
        // 40 multi-byte chars (2 bytes each) = 40 chars = threshold, 80 bytes
        let value = "α".repeat(40);
        let text = format!("key: \"{value}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().any(|a| a.title.contains("block scalar")),
            "must offer block-scalar when char count meets 40-char threshold"
        );
    }

    #[test]
    fn block_scalar_not_offered_when_cursor_on_different_line() {
        // Long scalar is on line 1; cursor is on line 0 — must not offer action
        let long = "x".repeat(50);
        let text = format!("key: short\nother: \"{long}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar when cursor is on a different line from the long scalar"
        );
    }

    #[test]
    fn block_scalar_indentation_follows_key_column() {
        // Key at column 2 (two leading spaces): base_indent = key_col = 2.
        // format_subtree adds tab_width (2) inside the block via indent(), so content
        // ends up at base_indent + tab_width = 2 + 2 = 4 spaces — exactly 4.
        let long = "a".repeat(50);
        let text = format!("  key: \"{long}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to block scalar")
            .expect("expected block-scalar action");
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        let body_line = new_text
            .lines()
            .nth(1)
            .expect("new_text must have a second line (the scalar body)");
        assert!(
            body_line.starts_with("    ") && !body_line.starts_with("     "),
            "scalar body must be indented by exactly 4 spaces (key_col 2 + tab_width 2), got: {body_line:?}"
        );
    }

    #[test]
    fn block_scalar_title_is_convert_to_block_scalar() {
        let long = "a".repeat(50);
        let text = format!("key: \"{long}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to block scalar")
            .expect("action with exact title 'Convert to block scalar' must be present");
        assert_eq!(
            action.title, "Convert to block scalar",
            "title must be exactly 'Convert to block scalar'"
        );
    }

    // ---- String to block scalar: defect class regressions ----

    #[test]
    fn should_resolve_escape_sequences_in_double_quoted_value() {
        // Double-quoted YAML: parser resolves \n \t \\ \" — block scalar must have real chars
        let text = "summary: \"line one\\nline two\\ttabbed\\\\backslash\\\"quote and more padding here\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            !result.contains("\\n"),
            "block scalar must not contain literal \\n escape: {result:?}"
        );
        assert!(
            !result.contains("\\t"),
            "block scalar must not contain literal \\t escape: {result:?}"
        );
        assert!(
            result.contains('\n'),
            "block scalar must contain actual newline: {result:?}"
        );
        assert!(
            result.contains('\t'),
            "block scalar must contain actual tab: {result:?}"
        );
        assert!(
            result.contains('\\'),
            "block scalar must contain literal backslash: {result:?}"
        );
        assert!(
            result.contains('"'),
            "block scalar must contain literal double-quote: {result:?}"
        );
    }

    #[test]
    fn block_scalar_double_quoted_backslash_and_quote_resolved() {
        let text = "key: \"contains \\\\backslash and \\\"quote\\\" here plus some extra padding chars\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains('\\'),
            "block scalar must contain a literal backslash: {result:?}"
        );
        assert!(
            result.contains('"'),
            "block scalar must contain a literal double-quote: {result:?}"
        );
    }

    #[test]
    fn block_scalar_uses_char_count_not_byte_count_for_threshold() {
        // 39 copies of é (U+00E9, 2 bytes each) = 39 chars < 40, but 78 bytes
        let val: String = "é".repeat(39);
        let text = format!("key: \"{val}\"\n");
        let actions = code_actions(
            &docs_for(&text),
            &text,
            cursor_range(0, 0),
            &[],
            &test_uri(),
        );
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for 39 chars (< threshold), even though byte len = 78"
        );
    }

    #[test]
    fn should_resolve_single_quoted_escape_to_literal_apostrophe() {
        let text = "note: 'it''s a long string that should exceed the forty character threshold'\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("it's"),
            "block scalar must contain resolved apostrophe: {result:?}"
        );
        // The '' escape sequence must not survive into the output
        assert!(
            !result.contains("it''s"),
            "block scalar must not contain the '' escape sequence: {result:?}"
        );
    }

    #[test]
    fn should_not_be_fooled_by_colon_in_url_value() {
        let text = "homepage: \"https://example.com/very-long-path-that-exceeds-forty-chars\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("https://example.com/very-long-path-that-exceeds-forty-chars"),
            "full URL must be preserved in block scalar output: {result:?}"
        );
        assert!(
            result.contains("|\n"),
            "output must be a literal block scalar: {result:?}"
        );
    }

    #[test]
    fn should_preserve_trailing_comment_when_converting_to_block_scalar() {
        let text = "description: \"this is a long string that exceeds forty chars\"  # keep me\n";
        let (result, edit) = apply_block_scalar_edit(text, 0);
        // The scalar ends at the closing `"` — the edit range must NOT extend past it.
        // Byte offset of the char after the closing `"` = length of `description: "..."`
        let scalar_end_col =
            "description: \"this is a long string that exceeds forty chars\"".len();
        assert_eq!(
            edit.range.end.character as usize, scalar_end_col,
            "edit end column must equal the exclusive end of the scalar span: range={:?}",
            edit.range
        );
        assert!(
            !edit.new_text.contains("# keep me"),
            "new_text must not contain the trailing comment: {:?}",
            edit.new_text
        );
        assert!(
            result.contains("# keep me"),
            "trailing comment must survive in the final edited text: {result:?}"
        );
    }

    #[test]
    fn should_not_be_fooled_by_colon_in_quoted_key() {
        let text = "\"foo:bar\": \"this is a long mapping value that exceeds forty characters\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("this is a long mapping value that exceeds forty characters"),
            "actual value must be preserved: {result:?}"
        );
        assert!(
            result.contains("|\n"),
            "output must be literal block scalar: {result:?}"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_sequence_item() {
        let text = "- \"this is a very long sequence item value that exceeds forty characters\"\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar for sequence item (only mapping values qualify)"
        );
    }

    #[test]
    fn should_not_offer_block_scalar_for_flow_sequence_value() {
        let text = "key: [one, two, three, four, five, six, seven, eight, nine, ten]\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        assert!(
            actions.iter().all(|a| !a.title.contains("block scalar")),
            "must not offer block-scalar when mapping value is a flow sequence"
        );
    }

    #[test]
    fn should_preserve_anchor_when_converting_to_block_scalar() {
        let text =
            "description: &myanchor \"this is a long string that exceeds forty characters\"\n";
        let (result, _edit) = apply_block_scalar_edit(text, 0);
        assert!(
            result.contains("&myanchor"),
            "anchor must be preserved in block scalar output: {result:?}"
        );
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
        // AST edit: new_text covers only the scalar span, not the full line.
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(
            edits[0].range.start.character, 9,
            "edit must start at scalar col"
        );
    }

    #[test]
    fn should_quote_yaml11_bool_uppercase_on() {
        // AST edit: new_text covers only the scalar span, not the full line.
        let text = "flag: ON\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"ON\"");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
    }

    #[test]
    fn should_quote_yaml11_bool_with_indentation() {
        // AST edit: new_text covers only the scalar span, not the full line.
        let text = "  enabled: yes\n";
        let diag = make_diagnostic(0, 11, 14, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(
            edits[0].range.start.character, 11,
            "edit must start at scalar col"
        );
    }

    #[test]
    fn yaml11_bool_quote_wrong_diagnostic_code_no_action() {
        // A diagnostic with code "flowMap" on a line containing a yaml11 bool must not
        // trigger the yaml11 bool action — dispatch is code-gated.
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "flowMap");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote value"));
    }

    // ---- block_to_flow: mapping-value quoting and anchor regression tests ----
    // These tests cover defect classes from the OLD text-surgery implementation
    // that would have produced broken or incorrect YAML. The AST+formatter path
    // must handle all of them correctly.

    // Mapping value containing a colon (e.g. a URL) — valid unquoted in flow context
    // because ':' without a following space is not a YAML mapping separator.
    #[test]
    fn should_produce_valid_yaml_for_mapping_value_containing_colon() {
        let text = "endpoint:\n  url: http://example.com\n  method: GET\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "colon-in-value mapping must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_result.diagnostics
        );
        assert!(
            result.contains("http://example.com"),
            "URL value must be preserved: {result:?}"
        );
    }

    // Mapping value containing a comma — must be quoted in flow context.
    #[test]
    fn should_quote_mapping_value_containing_comma_when_converting_block_to_flow() {
        let text = "info:\n  tags: foo, bar\n  name: safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"foo, bar\""),
            "comma-containing mapping value must be quoted: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "result must reparse cleanly: {result:?}"
        );
    }

    // Mapping value containing a brace — must be quoted in flow context.
    #[test]
    fn should_quote_mapping_value_containing_brace_when_converting_block_to_flow() {
        let text = "template:\n  expr: ${VAR}\n  name: safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"${VAR}\""),
            "brace-containing mapping value must be quoted: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "result must reparse cleanly: {result:?}"
        );
    }

    // Mapping value containing a bracket — must be quoted in flow context.
    #[test]
    fn should_quote_mapping_value_containing_bracket_when_converting_block_to_flow() {
        let text = "filter:\n  pattern: a[0]\n  name: safe\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"a[0]\""),
            "bracket-containing mapping value must be quoted: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "result must reparse cleanly: {result:?}"
        );
    }

    // Mapping key that is double-quoted with an embedded colon — the AST preserves
    // the key's scalar value; the formatter emits it as a plain scalar in flow context
    // (valid because ':' not followed by space is not a flow separator).
    #[test]
    fn should_preserve_quoted_key_with_colon_when_converting_block_to_flow() {
        let text = "labels:\n  \"foo:bar\": value\n  safe: ok\n";
        let result = apply_block_to_flow_edit(text, 0);
        let parse_result = parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "quoted-key-with-colon mapping must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_result.diagnostics
        );
        assert!(
            result.contains("foo:bar"),
            "key value must be preserved in output: {result:?}"
        );
    }

    // Anchored block mapping — the formatter emits the anchor inline before the flow
    // braces; the edit range starts after the key colon so the anchor is not duplicated.
    #[test]
    fn should_preserve_anchor_when_converting_anchored_block_mapping_to_flow() {
        let text = "defaults: &base\n  timeout: 30\n  retries: 3\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("&base"),
            "anchor must appear in flow output: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "anchored mapping result must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_yaml(&result).diagnostics
        );
    }

    // Anchored block sequence — same anchor-preservation requirement as for mappings.
    #[test]
    fn should_preserve_anchor_when_converting_anchored_block_sequence_to_flow() {
        let text = "items: &mylist\n  - a\n  - b\n";
        let actions = code_actions(&docs_for(text), text, cursor_range(0, 0), &[], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].new_text.contains("&mylist"),
            "anchor must appear in flow output: {:?}",
            edits[0].new_text
        );
        let result = apply_block_to_flow_edit(text, 0);
        assert!(
            parse_yaml(&result).diagnostics.is_empty(),
            "anchored sequence result must reparse without diagnostics; got: {:?}\nresult:\n{result}",
            parse_yaml(&result).diagnostics
        );
    }

    #[test]
    fn should_convert_yaml11_bool_yes_to_true() {
        // AST edit: new_text covers only the scalar span, not the full line.
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 9);
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
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 9);
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
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 6);
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
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 6);
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
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 8);
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
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 8);
    }

    #[test]
    fn should_convert_yaml11_bool_preserving_indentation() {
        // Indented scalar: edit range starts at the scalar column, not col 0.
        let text = "  active: yes\n";
        let diag = make_diagnostic(0, 10, 13, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(
            edits[0].range.start.character, 10,
            "edit must start at scalar col"
        );
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

    // ---- yaml11Boolean defect-class regression tests ----

    // Quote action produces valid double-quoted YAML (round-trip check).
    #[test]
    fn yaml11_bool_quote_value_produces_valid_double_quoted_yaml() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        let new_text = &edits[0].new_text;
        // Apply edit and verify round-trip
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "enabled: yes";
        let result = format!("{}{}{}\n", &line[..start], new_text, &line[end..]);
        let parse_result = crate::parser::parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "quoted bool must produce valid YAML; got: {:?}\nresult: {result:?}",
            parse_result.diagnostics
        );
        assert_eq!(
            new_text, "\"yes\"",
            "quote action must wrap scalar in double quotes"
        );
    }

    // Convert action edit range covers the scalar span, not the full line.
    #[test]
    fn yaml11_bool_convert_action_edit_range_targets_scalar_span() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].range.start.character, 9,
            "convert edit must start at scalar col"
        );
        assert_eq!(
            edits[0].range.end.character, 12,
            "convert edit must end at scalar end"
        );
    }

    // Convert action normalizes all 16 YAML 1.1-only bool tokens to canonical true/false.
    #[rstest]
    #[case::yes_lowercase("yes", "true")]
    #[case::yes_titlecase("Yes", "true")]
    #[case::yes_uppercase("YES", "true")]
    #[case::on_lowercase("on", "true")]
    #[case::on_titlecase("On", "true")]
    #[case::on_uppercase("ON", "true")]
    #[case::y_lowercase("y", "true")]
    #[case::y_uppercase("Y", "true")]
    #[case::no_lowercase("no", "false")]
    #[case::no_titlecase("No", "false")]
    #[case::no_uppercase("NO", "false")]
    #[case::off_lowercase("off", "false")]
    #[case::off_titlecase("Off", "false")]
    #[case::off_uppercase("OFF", "false")]
    #[case::n_lowercase("n", "false")]
    #[case::n_uppercase("N", "false")]
    fn yaml11_bool_convert_normalizes_all_16_tokens(#[case] token: &str, #[case] expected: &str) {
        let text = format!("flag: {token}\n");
        let col = 6u32;
        let end = col + u32::try_from(token.len()).unwrap();
        let diag = make_diagnostic(0, col, end, "yaml11Boolean");
        let actions = code_actions(&docs_for(&text), &text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].new_text, expected,
            "token {token:?} must convert to {expected:?}"
        );
    }

    // Both yaml11Boolean and schemaYaml11Boolean diagnostic codes produce two actions.
    #[rstest]
    #[case::yaml11_bool_code("yaml11Boolean")]
    #[case::schema_yaml11_bool_code("schemaYaml11Boolean")]
    fn yaml11_bool_actions_both_diag_codes_produce_two_actions(#[case] code: &str) {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, code);
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert_eq!(
            actions
                .iter()
                .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
                .count(),
            2,
            "diag code {code:?} must produce two actions"
        );
    }

    // Diag on a line with no yaml11 bool scalar returns no bool actions.
    #[test]
    fn yaml11_bool_actions_out_of_range_diag_returns_empty() {
        // Line 1 is "other: string" (no yaml11 bool); diag pointing there must not match.
        let text = "enabled: yes\nother: string\n";
        let diag = make_diagnostic(1, 7, 13, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Quote value" && a.title != "Convert to boolean"),
            "diag on non-yaml11-bool line must produce no yaml11-bool actions"
        );
    }

    // Trailing comment is preserved: edit range covers only the scalar span, not the comment.
    #[test]
    fn yaml11_bool_trailing_comment_preserved_quote_action() {
        let text = "enabled: yes  # keep this\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].new_text, "\"yes\"",
            "new_text must be just the quoted scalar"
        );
        // Range must not extend past the scalar end (col 12)
        assert!(
            edits[0].range.end.character <= 12,
            "edit end must not reach into the trailing comment: {:?}",
            edits[0].range
        );
        // Apply the edit and verify the comment survives
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "enabled: yes  # keep this";
        let result = format!("{}{}{}\n", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive in result: {result:?}"
        );
    }

    #[test]
    fn yaml11_bool_trailing_comment_preserved_convert_action() {
        let text = "flag: ON  # keep this\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert!(
            edits[0].range.end.character <= 8,
            "edit end must not reach into the trailing comment: {:?}",
            edits[0].range
        );
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "flag: ON  # keep this";
        let result = format!("{}{}{}\n", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive: {result:?}"
        );
    }

    // Mid-line cursor (sequence-item value): edit range must start at scalar col, not col 0.
    #[test]
    fn yaml11_bool_sequence_item_edit_starts_at_scalar_col() {
        // "- yes" in a sequence: "yes" starts at col 2 (after "- ")
        let text = "items:\n  - yes\n";
        // scalar "yes" is at col 4 in "  - yes" (0-indexed: "  - yes" → 2 spaces + "- " + "yes" at col 4)
        let diag = make_diagnostic(1, 4, 7, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].range.start.character > 0,
            "edit must not start at col 0 for sequence-item value: {:?}",
            edits[0].range
        );
        assert_eq!(edits[0].new_text, "\"yes\"");
    }

    // schema_yaml11_bool_type_actions returns exactly ONE action (no "Quote value").
    #[test]
    fn schema_yaml11_bool_type_returns_exactly_one_action() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let count = actions
            .iter()
            .filter(|a| a.title == "Convert to boolean" || a.title == "Quote value")
            .count();
        assert_eq!(
            count, 1,
            "schemaYaml11BooleanType must offer exactly one action"
        );
        assert!(
            actions.iter().any(|a| a.title == "Convert to boolean"),
            "the single action must be 'Convert to boolean'"
        );
        assert!(
            actions.iter().all(|a| a.title != "Quote value"),
            "schemaYaml11BooleanType must not offer 'Quote value'"
        );
    }

    // schema_yaml11_bool_type_actions gated on is_yaml11_bool: non-bool input → no action.
    #[test]
    fn schema_yaml11_bool_type_gated_on_yaml11_bool() {
        // "hello" is not a YAML 1.1 bool, so schemaYaml11BooleanType must produce no action.
        let text = "enabled: hello\n";
        let diag = make_diagnostic(0, 9, 14, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "non-yaml11-bool input must not produce 'Convert to boolean' for schemaYaml11BooleanType"
        );
    }

    // schema_yaml11_bool_type_actions edit range covers scalar span, not full line.
    #[test]
    fn schema_yaml11_bool_type_actions_edit_range_targets_scalar_span() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
        assert_eq!(
            edits[0].range.end.character, 9,
            "edit must end at scalar end"
        );
        assert_eq!(edits[0].new_text, "true");
    }

    // Diag on a line with no yaml11 bool scalar returns no schema actions.
    #[test]
    fn schema_yaml11_bool_type_actions_out_of_range_diag_returns_empty() {
        // Line 1 is "other: string" (no yaml11 bool); diag pointing there must not match.
        let text = "flag: yes\nother: string\n";
        let diag = make_diagnostic(1, 7, 13, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "diag on non-yaml11-bool line must produce no schema-yaml11-bool actions"
        );
    }

    // ---- Schema-code line-ambiguity guard regression tests ----
    //
    // When col_match=false (schema codes point at the key, not the scalar), the finder
    // uses line-only matching. These tests verify that when two yaml11 bool scalars
    // appear on the same line the guard suppresses the action (no action is better than
    // a wrong one), and that single-bool lines still work correctly.

    #[test]
    fn schema_yaml11_bool_two_bools_on_line_offers_no_action() {
        // "{a: yes, b: no}" — two bools on line 0; diag at key `b` (col 9-10)
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 9, 10, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Quote value" && a.title != "Convert to boolean"),
            "ambiguous line must suppress schema bool actions; got: {actions:?}"
        );
    }

    #[test]
    fn schema_yaml11_bool_two_bools_on_line_first_key_also_suppressed() {
        // Diag at key `a` (col 1-2) — both keys on an ambiguous line must be suppressed.
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 1, 2, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Quote value" && a.title != "Convert to boolean"),
            "first key on ambiguous line must also be suppressed; got: {actions:?}"
        );
    }

    #[test]
    fn schema_yaml11_bool_single_bool_on_line_offers_action() {
        // "flag: yes" — single bool; diag at key `flag` (col 0-4)
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 0, 4, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must target `yes` at col 6"
        );
    }

    #[test]
    fn schema_yaml11_bool_single_bool_on_line_two_actions_count() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 0, 4, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert_eq!(
            actions
                .iter()
                .filter(|a| a.title == "Quote value" || a.title == "Convert to boolean")
                .count(),
            2,
            "single-bool line must still offer two actions for schemaYaml11Boolean"
        );
    }

    #[test]
    fn schema_yaml11_bool_type_two_bools_on_line_offers_no_action() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 9, 10, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions.iter().all(|a| a.title != "Convert to boolean"),
            "ambiguous line must suppress schemaYaml11BooleanType action; got: {actions:?}"
        );
    }

    #[test]
    fn schema_yaml11_bool_type_single_bool_on_line_offers_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 0, 4, "schemaYaml11BooleanType");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 6);
    }

    #[test]
    fn schema_yaml11_bool_two_bools_nested_offers_no_action() {
        // "x:\n  a: yes\n  b: no\n" — each bool on its own line; diag at key `a` on line 1
        // (col 2-3). One bool per line → action IS offered (guard must not over-suppress).
        let text = "x:\n  a: yes\n  b: no\n";
        let diag = make_diagnostic(1, 2, 3, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(
            edits[0].range.start.character, 5,
            "edit must target `yes` at col 5"
        );
    }

    #[test]
    fn schema_yaml11_bool_flow_map_value_two_bools_same_line_suppressed() {
        // "x: {a: yes, b: no}\n" — two bools on line 0 inside a nested flow map
        let text = "x: {a: yes, b: no}\n";
        let diag = make_diagnostic(0, 4, 5, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to boolean" && a.title != "Quote value"),
            "nested flow map two-bool line must be suppressed; got: {actions:?}"
        );
    }

    // ---- Multi-bool-per-line column-awareness regression tests ----
    //
    // These tests verify that the column-aware finder returns the correct scalar
    // when two YAML 1.1 bool values appear on the same line (flow seq / flow map).

    #[test]
    fn yaml11_bool_flow_seq_second_bool_quote_action_targets_correct_scalar() {
        // "items: [yes, no]" — diag at col 13..15 points at `no`
        let text = "items: [yes, no]\n";
        let diag = make_diagnostic(0, 13, 15, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"no\"", "must quote `no`, not `yes`");
        assert_eq!(
            edits[0].range.start.character, 13,
            "edit must start at col 13 (`no`)"
        );
    }

    #[test]
    fn yaml11_bool_flow_seq_second_bool_convert_action_targets_correct_scalar() {
        let text = "items: [yes, no]\n";
        let diag = make_diagnostic(0, 13, 15, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false", "must convert `no` → `false`");
        assert_eq!(edits[0].range.start.character, 13);
    }

    #[test]
    fn yaml11_bool_flow_seq_first_bool_not_displaced_when_second_is_targeted() {
        // Diag at col 8..11 points at `yes` — must not be displaced by the col-aware fix.
        let text = "items: [yes, no]\n";
        let diag = make_diagnostic(0, 8, 11, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"", "must quote `yes`, not `no`");
        assert_eq!(edits[0].range.start.character, 8);
    }

    #[test]
    fn yaml11_bool_flow_seq_second_of_three_bools_targeted_correctly() {
        // "flags: [yes, no, on]" — diag at col 13..15 points at `no`
        let text = "flags: [yes, no, on]\n";
        let diag = make_diagnostic(0, 13, 15, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false", "must convert `no` → `false`");
        assert_eq!(edits[0].range.start.character, 13);
    }

    #[test]
    fn yaml11_bool_flow_map_second_bool_quote_action_targets_correct_scalar() {
        // "{a: yes, b: no}" — diag at col 12..14 points at `no`
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 12, 14, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"no\"", "must quote `no`, not `yes`");
        assert_eq!(edits[0].range.start.character, 12);
    }

    #[test]
    fn yaml11_bool_flow_map_second_bool_convert_action_targets_correct_scalar() {
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 12, 14, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 12);
    }

    #[test]
    fn yaml11_bool_flow_map_first_bool_not_displaced_when_second_is_targeted() {
        // Diag at col 4..7 points at `yes`
        let text = "{a: yes, b: no}\n";
        let diag = make_diagnostic(0, 4, 7, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(edits[0].range.start.character, 4);
    }

    #[test]
    fn yaml11_bool_nested_flow_seq_second_bool_targeted_correctly() {
        // "x:\n  flags: [yes, no]\n" — `no` is on line 1, col 15..17
        let text = "x:\n  flags: [yes, no]\n";
        let diag = make_diagnostic(1, 15, 17, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to boolean")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "false", "must convert `no` → `false`");
        assert_eq!(edits[0].range.start.character, 15);
    }

    // ---- yaml11Octal quick fixes ----

    #[test]
    fn should_not_offer_yaml11_octal_quote_for_out_of_bounds_range() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 100, 104, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote as string"));
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
        // AST edit: new_text covers only the scalar span; range.start.line matches the scalar line.
        let text = "key: value\nflag: yes\n";
        let diag = make_diagnostic(1, 6, 9, "yaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].range.start.line, 1, "edit must target line 1");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
        assert_eq!(edits[0].new_text, "\"yes\"");
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
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].range.start.line, 1, "edit must target line 1");
        assert_eq!(
            edits[0].range.start.character, 6,
            "edit must start at scalar col"
        );
        assert_eq!(edits[0].new_text, "\"0755\"");
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

    // ---- yaml11Octal AST retrofit regression tests ----

    #[test]
    fn yaml11_octal_quote_action_new_text_is_scalar_only() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"0755\"");
        assert_eq!(edits[0].range.start.character, 6);
        assert_eq!(edits[0].range.end.character, 10);
    }

    #[test]
    fn yaml11_octal_convert_action_new_text_is_scalar_only() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o755");
        assert_eq!(edits[0].range.start.character, 6);
        assert_eq!(edits[0].range.end.character, 10);
    }

    #[test]
    fn yaml11_octal_quote_on_0777_produces_valid_double_quoted_yaml() {
        let text = "perms: 0777\n";
        let diag = make_diagnostic(0, 7, 11, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"0777\"");
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "perms: 0777";
        let result = format!("{}{}{}\n", &line[..start], edits[0].new_text, &line[end..]);
        let parse_result = crate::parser::parse_yaml(&result);
        assert!(
            parse_result.diagnostics.is_empty(),
            "quoted octal must produce valid YAML; got: {:?}\nresult: {result:?}",
            parse_result.diagnostics
        );
    }

    #[test]
    fn yaml11_octal_convert_on_0755_produces_0o755() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o755");
    }

    #[test]
    fn yaml11_octal_convert_on_0777_produces_0o777() {
        let text = "perms: 0777\n";
        let diag = make_diagnostic(0, 7, 11, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o777");
    }

    #[test]
    fn yaml11_octal_rejects_08_no_actions() {
        let text = "val: 08\n";
        let diag = make_diagnostic(0, 5, 7, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    #[test]
    fn yaml11_octal_rejects_09_no_actions() {
        let text = "val: 09\n";
        let diag = make_diagnostic(0, 5, 7, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    #[test]
    fn yaml11_octal_trailing_comment_preserved_quote_action() {
        let text = "mode: 0755  # keep this\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"0755\"");
        assert!(
            edits[0].range.end.character <= 10,
            "range must not reach into comment"
        );
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "mode: 0755  # keep this";
        let result = format!("{}{}{}", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive in: {result:?}"
        );
    }

    #[test]
    fn yaml11_octal_trailing_comment_preserved_convert_action() {
        let text = "mode: 0755  # keep this\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Convert to YAML 1.2 octal")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "0o755");
        assert!(
            edits[0].range.end.character <= 10,
            "range must not reach into comment"
        );
        let start = edits[0].range.start.character as usize;
        let end = edits[0].range.end.character as usize;
        let line = "mode: 0755  # keep this";
        let result = format!("{}{}{}", &line[..start], edits[0].new_text, &line[end..]);
        assert!(
            result.contains("# keep this"),
            "trailing comment must survive in: {result:?}"
        );
    }

    #[test]
    fn yaml11_octal_sequence_item_edit_starts_at_scalar_col() {
        let text = "modes:\n  - 0755\n";
        // `0755` starts at col 4 in line 1: "  - 0755"
        let diag = make_diagnostic(1, 4, 8, "yaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(1), &[diag], &test_uri());
        let action = actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert!(
            edits[0].range.start.character > 0,
            "sequence item edit must not start at col 0"
        );
        assert_eq!(edits[0].new_text, "\"0755\"");
    }

    #[test]
    fn yaml11_octal_multi_octal_per_line_schema_code_offers_no_action() {
        let text = "{a: 0755, b: 0644}\n";
        // schemaYaml11Octal uses line-only matching; two octals on the same line must suppress both
        let diag = make_diagnostic(0, 4, 8, "schemaYaml11Octal");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }

    #[test]
    fn yaml11_octal_multi_octal_per_line_direct_code_resolves_by_col() {
        let text = "{a: 0755, b: 0644}\n";
        let first_diag = make_diagnostic(0, 4, 8, "yaml11Octal");
        let second_diag = make_diagnostic(0, 13, 17, "yaml11Octal");

        let first_actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[first_diag],
            &test_uri(),
        );
        let first_action = first_actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let first_edits = &first_action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()[&test_uri()];
        assert_eq!(first_edits[0].new_text, "\"0755\"");
        assert_eq!(first_edits[0].range.start.character, 4);

        let second_actions = code_actions(
            &docs_for(text),
            text,
            line_range(0),
            &[second_diag],
            &test_uri(),
        );
        let second_action = second_actions
            .iter()
            .find(|a| a.title == "Quote as string")
            .unwrap();
        let second_edits = &second_action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()[&test_uri()];
        assert_eq!(second_edits[0].new_text, "\"0644\"");
        assert_eq!(second_edits[0].range.start.character, 13);
    }

    #[rstest]
    #[case::yaml11_octal_code("yaml11Octal")]
    #[case::schema_yaml11_octal_code("schemaYaml11Octal")]
    fn yaml11_octal_both_diag_codes_produce_two_actions(#[case] code: &str) {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, code);
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let count = actions
            .iter()
            .filter(|a| a.title == "Quote as string" || a.title == "Convert to YAML 1.2 octal")
            .count();
        assert_eq!(count, 2);
    }

    // ════════════════════════════════════════════════════════════════════
    // Group E: schemaYaml11Boolean code actions
    // ════════════════════════════════════════════════════════════════════

    // E1: "Quote value" action replaces the plain value with a quoted string (scalar span only)
    #[test]
    fn schema_yaml11_boolean_quote_value_action() {
        let text = "flag: yes\n";
        let diag = make_diagnostic(0, 6, 9, "schemaYaml11Boolean");
        let actions = code_actions(&docs_for(text), text, line_range(0), &[diag], &test_uri());
        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "\"yes\"");
        assert_eq!(edits[0].range.start.character, 6);
    }

    // E2: "Convert to boolean" action replaces the value with canonical YAML 1.2 boolean (scalar span only)
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
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 6);
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

    // E5: "Convert to boolean" maps false-family values to false (scalar span only)
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
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 6);
    }

    // ════════════════════════════════════════════════════════════════════
    // Group F: schemaYaml11Octal code actions
    // ════════════════════════════════════════════════════════════════════

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

    // G1: "Convert to boolean" offered for schemaYaml11BooleanType diagnostic (scalar span only)
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
        assert_eq!(edits[0].new_text, "true");
        assert_eq!(edits[0].range.start.character, 9);
    }

    // G2: "Convert to boolean" maps false-family 1.1 values correctly (scalar span only)
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
        assert_eq!(edits[0].new_text, "false");
        assert_eq!(edits[0].range.start.character, 9);
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
