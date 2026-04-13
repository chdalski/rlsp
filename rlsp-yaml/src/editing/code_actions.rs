// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, NumberOrString, Position, Range, TextEdit,
    WorkspaceEdit,
};

use std::collections::HashMap;

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
            Some("flowMap") => flow_map_to_block(&lines, diag, uri)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("flowSeq") => flow_seq_to_block(&lines, diag, uri)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("unusedAnchor") => delete_unused_anchor(&lines, diag, uri)
                .into_iter()
                .collect::<Vec<_>>(),
            Some("yaml11Boolean") => yaml11_bool_actions(&lines, diag, uri),
            Some("yaml11Octal") => yaml11_octal_actions(&lines, diag, uri),
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
            block_to_flow(&lines, line_idx, uri),
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

    let flow_content = &line[start_col..end_col];
    if !flow_content.starts_with('{') || !flow_content.ends_with('}') {
        return None;
    }

    // Determine the indentation for the block output
    let prefix = &line[..start_col];
    let base_indent = if prefix.trim_end().ends_with(':') {
        // The flow map is a value: `key: {a: 1}` → indent under the key
        let key_indent = prefix.len() - prefix.trim_start().len();
        key_indent + 2
    } else {
        // Standalone flow map
        start_col + 2
    };

    let inner = &flow_content[1..flow_content.len() - 1].trim();
    let pairs = split_flow_items(inner);
    if pairs.is_empty() {
        return None;
    }

    let indent_str = " ".repeat(base_indent);
    let block_lines: Vec<String> = pairs
        .iter()
        .map(|p| p.trim())
        .filter(|t| !t.is_empty())
        .map(|t| format!("{indent_str}{t}"))
        .collect();

    if block_lines.is_empty() {
        return None;
    }

    // Build the replacement: replace from the `{` to end of flow, keeping the prefix
    let new_text = if prefix.trim_end().ends_with(':') {
        // `key: {a: 1}` → `key:\n  a: 1`
        let key_part = prefix.trim_end().trim_end_matches(':');
        let key_indent = prefix.len() - prefix.trim_start().len();
        let key_indent_str = " ".repeat(key_indent);
        format!(
            "{key_indent_str}{}:\n{}",
            key_part.trim_start(),
            block_lines.join("\n")
        )
    } else {
        block_lines.join("\n")
    };

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(diag.range.start.line, 0),
        Position::new(diag.range.start.line, line.len() as u32),
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

// ---------- Flow seq to block ----------

fn flow_seq_to_block(
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

    let flow_content = &line[start_col..end_col];
    if !flow_content.starts_with('[') || !flow_content.ends_with(']') {
        return None;
    }

    let prefix = &line[..start_col];
    let base_indent = if prefix.trim_end().ends_with(':') {
        // Items indent 2 deeper than the key, matching `flow_map_to_block`.
        let key_indent = prefix.len() - prefix.trim_start().len();
        key_indent + 2
    } else {
        start_col
    };

    let inner = &flow_content[1..flow_content.len() - 1].trim();
    let items = split_flow_items(inner);
    if items.is_empty() {
        return None;
    }

    let indent_str = " ".repeat(base_indent);
    let block_lines: Vec<String> = items
        .iter()
        .map(|i| i.trim())
        .filter(|t| !t.is_empty())
        .map(|t| format!("{indent_str}- {t}"))
        .collect();

    if block_lines.is_empty() {
        return None;
    }

    let new_text = if prefix.trim_end().ends_with(':') {
        let key_part = prefix.trim_end().trim_end_matches(':');
        let key_indent = prefix.len() - prefix.trim_start().len();
        let key_indent_str = " ".repeat(key_indent);
        format!(
            "{key_indent_str}{}:\n{}",
            key_part.trim_start(),
            block_lines.join("\n")
        )
    } else {
        block_lines.join("\n")
    };

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(diag.range.start.line, 0),
        Position::new(diag.range.start.line, line.len() as u32),
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

// ---------- Block to flow ----------

fn block_to_flow(
    lines: &[&str],
    line_idx: usize,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let line = lines.get(line_idx)?;
    let trimmed = line.trim();

    // Must be a key line (key: or key: value) that starts a block mapping or sequence
    let colon_pos = trimmed.find(':')?;
    let key = trimmed[..colon_pos].trim();
    if key.is_empty() {
        return None;
    }

    let after_colon = trimmed[colon_pos + 1..].trim();

    // The key must have children on subsequent lines (block style)
    // If there's a non-empty value after the colon, it's already inline
    if !after_colon.is_empty() {
        return None;
    }

    let base_indent = line.len() - line.trim_start().len();
    let child_indent = base_indent + 2;

    // Collect child lines
    let mut children = Vec::new();
    let mut end_line = line_idx;
    let is_sequence = lines
        .get(line_idx + 1)
        .is_some_and(|l| l.trim_start().starts_with("- "));

    for (i, child_line) in lines.iter().enumerate().skip(line_idx + 1) {
        let child_trimmed = child_line.trim();
        if child_trimmed.is_empty() {
            break;
        }
        let child_line_indent = child_line.len() - child_line.trim_start().len();
        if child_line_indent < child_indent {
            break;
        }
        // Only collect direct children (not deeply nested)
        if child_line_indent > child_indent {
            return None; // Nested structure — too complex for simple flow conversion
        }
        children.push(child_trimmed);
        end_line = i;
    }

    if children.is_empty() {
        return None;
    }

    let indent_str = " ".repeat(base_indent);

    let flow_value = if is_sequence {
        let items: Vec<String> = children
            .iter()
            .map(|c| c.strip_prefix("- ").unwrap_or(c))
            .map(quote_flow_item)
            .collect();
        format!("[{}]", items.join(", "))
    } else {
        // Block mapping children: each line is `child_key: child_value`
        let pairs = children.clone();
        format!("{{{}}}", pairs.join(", "))
    };

    let new_text = format!("{indent_str}{key}: {flow_value}");

    let title = if new_text.len() > 80 {
        "Convert block to flow style (long line)".to_string()
    } else {
        "Convert block to flow style".to_string()
    };

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let edit_range = Range::new(
        Position::new(line_idx as u32, 0),
        Position::new(
            end_line as u32,
            lines.get(end_line).map_or(0, |l| l.len() as u32),
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

/// Split a flow collection's inner content by commas, respecting nesting.
fn split_flow_items(content: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for ch in content.chars() {
        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                current.push(ch);
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                current.push(ch);
            }
            '{' | '[' if !in_single_quote && !in_double_quote => {
                depth += 1;
                current.push(ch);
            }
            '}' | ']' if !in_single_quote && !in_double_quote => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 && !in_single_quote && !in_double_quote => {
                items.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(ch),
        }
    }

    let final_item = current.trim().to_string();
    if !final_item.is_empty() {
        items.push(final_item);
    }

    items
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::unwrap_used, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;

    fn test_uri() -> tower_lsp::lsp_types::Url {
        tower_lsp::lsp_types::Url::parse("file:///test.yaml").unwrap()
    }

    fn cursor_range(line: u32, col: u32) -> Range {
        Range::new(Position::new(line, col), Position::new(line, col))
    }

    fn line_range(line: u32) -> Range {
        Range::new(Position::new(line, 0), Position::new(line, 999))
    }

    fn make_diagnostic(line: u32, start: u32, end: u32, code: &str) -> Diagnostic {
        Diagnostic {
            range: Range::new(Position::new(line, start), Position::new(line, end)),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some("rlsp-yaml".to_string()),
            ..Diagnostic::default()
        }
    }

    // ---- Flow map to block ----

    #[test]
    fn should_convert_simple_flow_map_to_block() {
        let text = "config: {a: 1, b: 2}\n";
        let diag = make_diagnostic(0, 8, 20, "flowMap");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());
        assert!(actions.iter().all(|a| !a.title.contains(title_fragment)));
    }

    // ---- Flow seq to block ----

    #[test]
    fn should_convert_simple_flow_seq_to_block() {
        let text = "items: [one, two, three]\n";
        let diag = make_diagnostic(0, 7, 24, "flowSeq");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        // Key at indent 6 — items must be at indent 8 (6 + 2), not 6.
        let text = "      command: [\"python\", \"-m\"]\n";
        let start_col = u32::try_from(text.find('[').unwrap()).unwrap();
        let end_col = u32::try_from(text.trim_end_matches('\n').len()).unwrap();
        let diag = make_diagnostic(0, start_col, end_col, "flowSeq");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("      command:\n"),
            "key should be at 6-space indent: {new_text:?}"
        );
        assert!(
            new_text.contains("        - "),
            "items should be at 8-space indent (6+2): {new_text:?}"
        );
        // Items at exactly 6 spaces would start with "      - " but NOT "       - "
        // (7 spaces). Since correct items are at 8 spaces, any line starting with
        // exactly "      - " (6 spaces then "- ") is a sign of wrong indentation.
        for line in new_text.lines() {
            if line.starts_with("- ") || line.trim_start().starts_with("- ") {
                let indent = line.len() - line.trim_start().len();
                assert!(
                    indent != 6,
                    "item at key-level indent (6) must not occur: {line:?}"
                );
            }
        }
    }

    #[test]
    fn should_indent_block_items_at_top_level_key() {
        // Regression guard: zero-indent key → items at indent 2.
        let text = "items: [one, two]\n";
        let start_col = u32::try_from(text.find('[').unwrap()).unwrap();
        let end_col = u32::try_from(text.trim_end_matches('\n').len()).unwrap();
        let diag = make_diagnostic(0, start_col, end_col, "flowSeq");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("items:\n"),
            "key should appear with no indent: {new_text:?}"
        );
        assert!(
            new_text.contains("  - one"),
            "items should be at 2-space indent: {new_text:?}"
        );
    }

    #[test]
    fn should_indent_block_items_under_key_at_indent_2() {
        // Regression guard: key at indent 2 → items at indent 4.
        let text = "  command: [\"a\", \"b\"]\n";
        let start_col = u32::try_from(text.find('[').unwrap()).unwrap();
        let end_col = u32::try_from(text.trim_end_matches('\n').len()).unwrap();
        let diag = make_diagnostic(0, start_col, end_col, "flowSeq");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("flow sequence"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        let new_text = &edits[0].new_text;
        assert!(
            new_text.contains("  command:\n"),
            "key should be at 2-space indent: {new_text:?}"
        );
        assert!(
            new_text.contains("    - "),
            "items should be at 4-space indent (2+2): {new_text:?}"
        );
        for line in new_text.lines() {
            if line.trim_start().starts_with("- ") {
                let indent = line.len() - line.trim_start().len();
                assert!(
                    indent != 2,
                    "item at key-level indent (2) must not occur: {line:?}"
                );
            }
        }
    }

    // ---- Block to flow ----

    #[test]
    fn should_convert_block_mapping_to_flow() {
        let text = "config:\n  a: 1\n  b: 2\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("block to flow"));
        assert!(action.is_some());
        let edit = action.unwrap().edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(edits[0].new_text.contains("{a: 1, b: 2}"));
    }

    #[test]
    fn should_convert_block_sequence_to_flow() {
        let text = "items:\n  - one\n  - two\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block to flow")));
    }

    #[test]
    fn should_not_offer_block_to_flow_for_nested_structures() {
        let text = "config:\n  a:\n    nested: value\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block to flow")));
    }

    #[test]
    fn should_quote_bracket_containing_item_when_converting_block_to_flow() {
        let text = "args:\n  - [nested]\n  - safe\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("\"[nested]\""),
            "bracket-containing item must be quoted: {:?}",
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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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

    // ---- Tab to spaces ----

    #[test]
    fn should_convert_tabs_to_spaces() {
        let text = "\tkey: value\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("tabs")));
    }

    // ---- Delete unused anchor ----

    #[test]
    fn should_delete_unused_anchor() {
        let text = "defaults: &unused value\n";
        let diag = make_diagnostic(0, 10, 17, "unusedAnchor");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("true")).unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: true");
    }

    #[test]
    fn should_convert_single_quoted_false_to_unquoted() {
        let text = "enabled: 'false'\n";
        let actions = code_actions(text, cursor_range(0, 10), &[], &test_uri());

        let action = actions.iter().find(|a| a.title.contains("false")).unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: false");
    }

    #[test]
    fn should_not_offer_bool_conversion_for_non_bool_string() {
        let text = "name: \"hello\"\n";
        let actions = code_actions(text, cursor_range(0, 7), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("Convert quoted")));
    }

    // ---- String to block scalar ----

    #[test]
    fn should_convert_long_string_to_block_scalar() {
        let long_value = "a".repeat(50);
        let text = format!("description: \"{long_value}\"\n");
        let actions = code_actions(&text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block scalar")));
    }

    #[test]
    fn should_not_offer_block_scalar_for_flow_collection() {
        let long_value = format!("{{{}:1}}", "a".repeat(50));
        let text = format!("key: {long_value}\n");
        let actions = code_actions(&text, cursor_range(0, 0), &[], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("block scalar")));
    }

    // ---- split_flow_items helper ----

    #[rstest]
    #[case::simple_pairs("a: 1, b: 2, c: 3", vec!["a: 1", "b: 2", "c: 3"])]
    #[case::nested_braces("a: {x: 1}, b: 2", vec!["a: {x: 1}", "b: 2"])]
    #[case::nested_brackets("a: [1, 2], b: 3", vec!["a: [1, 2]", "b: 3"])]
    #[case::quoted_comma("a: \"hello, world\", b: 2", vec!["a: \"hello, world\"", "b: 2"])]
    fn split_flow_items_cases(#[case] input: &str, #[case] expected: Vec<&str>) {
        assert_eq!(split_flow_items(input), expected);
    }

    #[test]
    fn split_flow_items_empty_input() {
        assert!(split_flow_items("").is_empty());
    }

    // ---- Diagnostic overlap ----

    #[test]
    fn should_not_produce_actions_for_non_overlapping_diagnostics() {
        let text = "config: {a: 1}\nother: value\n";
        let diag = make_diagnostic(0, 8, 14, "flowMap");
        // Request actions for line 1, where the diagnostic is not
        let actions = code_actions(text, cursor_range(1, 0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| !a.title.contains("flow mapping")));
    }

    // ---- Empty diagnostics ----

    #[test]
    fn should_return_empty_for_plain_yaml_no_diagnostics() {
        let text = "key: value\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let text = "items:\n  - 'hello'\n  - 'world'\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

        let action = actions
            .iter()
            .find(|a| a.title.contains("block to flow"))
            .unwrap();
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = &changes[&test_uri()];
        assert!(
            edits[0].new_text.contains("['hello', 'world']"),
            "pre-quoted single-quoted items must not be wrapped: {:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn should_quote_unsafe_item_alongside_pre_quoted_item() {
        let text = "args:\n  - \"true\"\n  - value, with comma\n";
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, cursor_range(0, 0), &[], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "enabled: \"yes\"");
    }

    #[test]
    fn should_quote_yaml11_bool_uppercase_on() {
        let text = "flag: ON\n";
        let diag = make_diagnostic(0, 6, 8, "yaml11Boolean");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "flag: \"ON\"");
    }

    #[test]
    fn should_quote_yaml11_bool_with_indentation() {
        let text = "  enabled: yes\n";
        let diag = make_diagnostic(0, 11, 14, "yaml11Boolean");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        let action = actions.iter().find(|a| a.title == "Quote value").unwrap();
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(edits[0].new_text, "  enabled: \"yes\"");
    }

    #[test]
    fn should_not_offer_yaml11_bool_quote_for_non_overlapping_diagnostic() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 100, 103, "yaml11Boolean");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote value"));
    }

    #[test]
    fn should_convert_yaml11_bool_yes_to_true() {
        let text = "enabled: yes\n";
        let diag = make_diagnostic(0, 9, 12, "yaml11Boolean");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote as string"));
    }

    #[test]
    fn should_convert_yaml11_octal_0755_to_yaml12() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 6, 10, "yaml11Octal");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(1), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(1), &[diag], &test_uri());

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
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote value"));
        assert!(actions.iter().all(|a| a.title != "Convert to boolean"));
    }

    #[test]
    fn yaml11_octal_diagnostic_not_triggered_by_other_codes() {
        let text = "mode: 0755\n";
        let diag = make_diagnostic(0, 0, 10, "flowSeq");
        let actions = code_actions(text, line_range(0), &[diag], &test_uri());

        assert!(actions.iter().all(|a| a.title != "Quote as string"));
        assert!(
            actions
                .iter()
                .all(|a| a.title != "Convert to YAML 1.2 octal")
        );
    }
}
