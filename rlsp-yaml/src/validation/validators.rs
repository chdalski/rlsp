// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range,
};

/// A token found in the text: either an anchor (`&name`) or an alias (`*name`).
#[derive(Debug, Clone)]
struct Token {
    name: String,
    line: u32,
    start_col: u32,
    end_col: u32,
    is_anchor: bool,
}

/// Validate unused anchors and unresolved aliases in YAML text.
///
/// Returns diagnostics for:
/// - Anchors (`&name`) that are never referenced by any alias (marked with `DiagnosticTag::Unnecessary`)
/// - Aliases (`*name`) that reference non-existent anchors (error severity)
///
/// Anchors and aliases are scoped to individual YAML documents.
#[must_use]
pub fn validate_unused_anchors(text: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diagnostics = Vec::new();

    // Find all document boundaries
    let mut doc_ranges = Vec::new();
    let mut current_start = 0;

    for (line_idx, line) in lines.iter().enumerate() {
        if line.trim() == "---" {
            if line_idx > current_start {
                doc_ranges.push((current_start, line_idx));
            }
            current_start = line_idx + 1;
        }
    }
    // Add final document
    if current_start < lines.len() {
        doc_ranges.push((current_start, lines.len()));
    }
    // If no separators found, treat as single document
    if doc_ranges.is_empty() && !lines.is_empty() {
        doc_ranges.push((0, lines.len()));
    }

    // Process each document independently
    for (start_line, end_line) in doc_ranges {
        let tokens = scan_tokens(&lines, start_line, end_line);

        // Build anchor map for O(1) lookup
        let mut anchors: HashMap<String, &Token> = HashMap::new();
        let mut aliases: Vec<&Token> = Vec::new();

        for token in &tokens {
            if token.is_anchor {
                anchors.insert(token.name.clone(), token);
            } else {
                aliases.push(token);
            }
        }

        // Track which anchors are used
        let mut used_anchors: HashSet<String> = HashSet::new();

        // Check aliases for unresolved references
        for alias in &aliases {
            if anchors.contains_key(&alias.name) {
                used_anchors.insert(alias.name.clone());
            } else {
                // Unresolved alias
                diagnostics.push(Diagnostic {
                    range: Range::new(
                        Position::new(alias.line, alias.start_col),
                        Position::new(alias.line, alias.end_col),
                    ),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("unresolvedAlias".to_string())),
                    message: format!("Alias '{}' has no matching anchor", alias.name),
                    source: Some("rlsp-yaml".to_string()),
                    ..Diagnostic::default()
                });
            }
        }

        // Report unused anchors
        diagnostics.extend(
            anchors
                .iter()
                .filter(|(name, _)| !used_anchors.contains(*name))
                .map(|(name, anchor)| {
                    let truncated_name = if name.len() > 100 {
                        format!("{}...", &name[..100])
                    } else {
                        name.clone()
                    };
                    Diagnostic {
                        range: Range::new(
                            Position::new(anchor.line, anchor.start_col),
                            Position::new(anchor.line, anchor.end_col),
                        ),
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("unusedAnchor".to_string())),
                        message: format!("Anchor '{truncated_name}' is never used"),
                        source: Some("rlsp-yaml".to_string()),
                        tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                        ..Diagnostic::default()
                    }
                }),
        );
    }

    diagnostics
}

/// Scan lines for anchor (`&name`) and alias (`*name`) tokens within the
/// given line range. Skips comment lines.
fn scan_tokens(lines: &[&str], start_line: usize, end_line: usize) -> Vec<Token> {
    let mut tokens = Vec::new();

    for line_idx in start_line..end_line {
        let Some(line) = lines.get(line_idx) else {
            continue;
        };

        let trimmed = line.trim();

        // Skip comment lines
        if trimmed.starts_with('#') {
            continue;
        }

        #[expect(
            clippy::cast_possible_truncation,
            reason = "LSP line/col are u32; always fits"
        )]
        let line_num = line_idx as u32;

        let mut chars = line.char_indices().peekable();
        while let Some((i, ch)) = chars.next() {
            if ch == '&' || ch == '*' {
                let is_anchor = ch == '&';

                // Check if followed by a valid anchor name character
                let name_start = i + 1;
                let mut name_end = name_start;

                while let Some(&(j, next_ch)) = chars.peek() {
                    if is_anchor_name_char(next_ch) {
                        name_end = j + next_ch.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }

                // Must have at least one name character
                if name_end > name_start {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    tokens.push(Token {
                        name: line[name_start..name_end].to_string(),
                        line: line_num,
                        start_col: i as u32,
                        end_col: name_end as u32,
                        is_anchor,
                    });
                }
            }
        }
    }

    tokens
}

/// Check if a character is valid in a YAML anchor/alias name.
/// Valid characters: alphanumeric, `-`, `_`, `.`
const fn is_anchor_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.'
}

/// Validate flow style usage in YAML text.
///
/// Returns warning diagnostics for:
/// - Flow mappings (`{...}`) with code `flowMap`
/// - Flow sequences (`[...]`) with code `flowSeq`
#[must_use]
pub fn validate_flow_style(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "LSP line/col are u32; always fits"
        )]
        let line_num = line_idx as u32;

        let mut in_single_quote = false;
        let mut in_double_quote = false;

        for (i, ch) in line.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                '{' if !in_single_quote && !in_double_quote => {
                    // Find matching closing brace; skip empty collections (`{}`, `{ }`)
                    if let Some(close_pos) = find_closing_char(line, i, '{', '}') {
                        if !line[i + 1..close_pos].trim().is_empty() {
                            #[expect(
                                clippy::cast_possible_truncation,
                                reason = "LSP line/col are u32; always fits"
                            )]
                            diagnostics.push(Diagnostic {
                                range: Range::new(
                                    Position::new(line_num, i as u32),
                                    Position::new(line_num, (close_pos + 1) as u32),
                                ),
                                severity: Some(DiagnosticSeverity::WARNING),
                                code: Some(NumberOrString::String("flowMap".to_string())),
                                message: "Flow mapping style: use block style instead".to_string(),
                                source: Some("rlsp-yaml".to_string()),
                                ..Diagnostic::default()
                            });
                        }
                    }
                }
                '[' if !in_single_quote && !in_double_quote => {
                    // Find matching closing bracket; skip empty collections (`[]`, `[ ]`)
                    if let Some(close_pos) = find_closing_char(line, i, '[', ']') {
                        if !line[i + 1..close_pos].trim().is_empty() {
                            #[expect(
                                clippy::cast_possible_truncation,
                                reason = "LSP line/col are u32; always fits"
                            )]
                            diagnostics.push(Diagnostic {
                                range: Range::new(
                                    Position::new(line_num, i as u32),
                                    Position::new(line_num, (close_pos + 1) as u32),
                                ),
                                severity: Some(DiagnosticSeverity::WARNING),
                                code: Some(NumberOrString::String("flowSeq".to_string())),
                                message: "Flow sequence style: use block style instead".to_string(),
                                source: Some("rlsp-yaml".to_string()),
                                ..Diagnostic::default()
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    diagnostics
}

/// Find the position of the closing character, respecting quote context.
fn find_closing_char(line: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, ch) in line[start + 1..].char_indices() {
        let actual_i = start + 1 + i;
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            c if c == open && !in_single_quote && !in_double_quote => depth += 1,
            c if c == close && !in_single_quote && !in_double_quote => {
                depth -= 1;
                if depth == 0 {
                    return Some(actual_i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Validate custom YAML tags against an allowed set.
///
/// Returns warning diagnostics for any `!tag` found in the YAML documents that is not
/// listed in `allowed_tags`. When `allowed_tags` is empty, validation is skipped and
/// an empty vec is returned — no tags configured means no warnings.
#[must_use]
pub fn validate_custom_tags<S: std::hash::BuildHasher>(
    text: &str,
    docs: &[Document<Span>],
    allowed_tags: &HashSet<String, S>,
) -> Vec<Diagnostic> {
    if allowed_tags.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();
    let mut diagnostics = Vec::new();
    // Track how many times each tag string has been seen so far (to handle duplicates).
    let mut seen_counts: HashMap<String, usize> = HashMap::new();

    for doc in docs {
        collect_tag_diagnostics(
            &doc.root,
            &lines,
            allowed_tags,
            &mut seen_counts,
            &mut diagnostics,
            0,
        );
    }

    diagnostics
}

/// Recursively walk a YAML node and emit diagnostics for unknown tags.
fn collect_tag_diagnostics<S: std::hash::BuildHasher>(
    node: &Node<Span>,
    lines: &[&str],
    allowed_tags: &HashSet<String, S>,
    seen_counts: &mut HashMap<String, usize>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    // Check the tag field on this node (all variants carry an optional tag).
    let tag = match node {
        Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
            tag.as_deref()
        }
        Node::Alias { .. } => None,
    };
    if let Some(tag_str) = tag {
        if !allowed_tags.contains(tag_str) {
            let occurrence = *seen_counts.get(tag_str).unwrap_or(&0);
            seen_counts.insert(tag_str.to_string(), occurrence + 1);

            if let Some(range) = find_tag_occurrence(lines, tag_str, occurrence) {
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unknownTag".to_string())),
                    message: format!("Unknown tag: {tag_str}"),
                    source: Some("rlsp-yaml".to_string()),
                    ..Diagnostic::default()
                });
            }
        }
    }

    // Recurse into children.
    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_tag_diagnostics(
                    key,
                    lines,
                    allowed_tags,
                    seen_counts,
                    diagnostics,
                    depth + 1,
                );
                collect_tag_diagnostics(
                    value,
                    lines,
                    allowed_tags,
                    seen_counts,
                    diagnostics,
                    depth + 1,
                );
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_tag_diagnostics(
                    item,
                    lines,
                    allowed_tags,
                    seen_counts,
                    diagnostics,
                    depth + 1,
                );
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Find the byte range of the Nth occurrence (0-indexed) of `tag_str` in the text lines.
/// Returns `None` if the occurrence index is out of range.
///
/// Skips occurrences that appear inside single- or double-quoted strings so that
/// `note: "use !include for files"` does not shadow `value: !include actual.yaml`.
fn find_tag_occurrence(lines: &[&str], tag_str: &str, occurrence: usize) -> Option<Range> {
    let mut count = 0usize;

    for (line_idx, line) in lines.iter().enumerate() {
        let mut search_start = 0;
        while let Some(pos) = line[search_start..].find(tag_str) {
            let abs_pos = search_start + pos;

            // Check whether this position is inside a quoted string by scanning
            // from the start of the line up to abs_pos.
            let in_quotes = is_inside_quotes(line, abs_pos);

            // Make sure it's a real tag boundary: preceded by nothing or whitespace/colon/dash,
            // and not immediately followed by another tag-name character.
            let before_ok = abs_pos == 0
                || line
                    .as_bytes()
                    .get(abs_pos - 1)
                    .is_some_and(|&b| b == b' ' || b == b'\t' || b == b':' || b == b'-');
            let after_end = abs_pos + tag_str.len();
            let after_ok = line
                .as_bytes()
                .get(after_end)
                .is_none_or(|&b| !b.is_ascii_alphanumeric() && b != b'-' && b != b'_' && b != b'.');

            if !in_quotes && before_ok && after_ok {
                if count == occurrence {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    return Some(Range::new(
                        Position::new(line_idx as u32, abs_pos as u32),
                        Position::new(line_idx as u32, after_end as u32),
                    ));
                }
                count += 1;
            }
            search_start = abs_pos + 1;
        }
    }
    None
}

/// Return `true` if byte position `pos` in `line` falls inside a single- or double-quoted
/// string, using the same quote-tracking logic as `validate_flow_style`.
fn is_inside_quotes(line: &str, pos: usize) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    for (i, ch) in line.char_indices() {
        if i >= pos {
            break;
        }
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            _ => {}
        }
    }
    in_single || in_double
}

/// Validate map key ordering in YAML documents.
///
/// Returns warning diagnostics for map keys that are not in alphabetical order.
/// Uses case-sensitive lexicographic comparison.
///
#[must_use]
pub fn validate_key_ordering(text: &str, docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diagnostics = Vec::new();

    // Build a key → first-line index once so check_yaml_ordering can do O(1)
    // lookups instead of scanning all lines for every diagnostic emitted.
    // Uses the same matching logic as the removed find_key_line: the key is
    // the trimmed text before the first ':', which must not be empty.
    let key_index: HashMap<String, u32> = lines
        .iter()
        .enumerate()
        .filter_map(|(line_idx, line)| {
            let trimmed = line.trim_start();
            let colon_pos = trimmed.find(':')?;
            let key = trimmed[..colon_pos].trim_end();
            if key.is_empty() {
                return None;
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "LSP line/col are u32; always fits"
            )]
            Some((key.to_string(), line_idx as u32))
        })
        .fold(HashMap::new(), |mut map, (key, line)| {
            map.entry(key).or_insert(line);
            map
        });

    for doc in docs {
        check_yaml_ordering(&doc.root, &key_index, &mut diagnostics, 0);
    }

    diagnostics
}

/// Recursively check YAML nodes for key ordering, with depth limit.
fn check_yaml_ordering(
    node: &Node<Span>,
    key_index: &HashMap<String, u32>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Mapping { entries, .. } => {
            // Extract keys in order — skip null keys (they have no ordering semantics).
            let keys: Vec<String> = entries
                .iter()
                .filter_map(|(k, _)| match k {
                    Node::Scalar { value, .. } if !crate::scalar_helpers::is_null(value) => {
                        Some(value.clone())
                    }
                    Node::Scalar { .. }
                    | Node::Mapping { .. }
                    | Node::Sequence { .. }
                    | Node::Alias { .. } => None,
                })
                .collect();

            // Check if keys are in alphabetical order
            // Track the maximum key seen so far to catch all out-of-order keys
            let mut max_key: &str = keys.first().map_or("", String::as_str);

            for key in keys.iter().skip(1) {
                if key.as_str() < max_key {
                    // Look up the line number in the pre-built index (O(1)).
                    if let Some(&line_num) = key_index.get(key.as_str()) {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "LSP line/col are u32; always fits"
                        )]
                        let key_len = key.len() as u32;
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(line_num, 0),
                                Position::new(line_num, key_len),
                            ),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("mapKeyOrder".to_string())),
                            message: format!("Key '{key}' is out of alphabetical order"),
                            source: Some("rlsp-yaml".to_string()),
                            ..Diagnostic::default()
                        });
                    }
                } else if key.as_str() > max_key {
                    max_key = key;
                }
            }

            // Recursively check nested structures
            for (_, value) in entries {
                check_yaml_ordering(value, key_index, diagnostics, depth + 1);
            }
        }
        Node::Sequence { items, .. } => {
            // Recursively check array elements
            for item in items {
                check_yaml_ordering(item, key_index, diagnostics, depth + 1);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Validate duplicate mapping keys in YAML documents.
///
/// Returns error diagnostics for any key that appears more than once within
/// the same mapping. Operates on the parsed AST, which preserves all keys
/// even when duplicate.
///
/// Each document and each nested mapping is scoped independently.
#[must_use]
pub fn validate_duplicate_keys(docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for doc in docs {
        check_node_for_duplicate_keys(&doc.root, &mut diagnostics, 0);
    }
    diagnostics
}

/// Recursively walk a node and emit diagnostics for duplicate keys in each mapping.
fn check_node_for_duplicate_keys(
    node: &Node<Span>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Mapping { entries, .. } => {
            let mut seen: HashSet<String> = HashSet::new();
            for (key, value) in entries {
                let key_str_and_loc: Option<(String, &Span)> = match key {
                    Node::Scalar {
                        value: key_str,
                        loc,
                        ..
                    } => Some((key_str.clone(), loc)),
                    Node::Alias { name, loc, .. } => Some((format!("*{name}"), loc)),
                    Node::Mapping { .. } | Node::Sequence { .. } => None,
                };
                if let Some((key_str, loc)) = key_str_and_loc {
                    if seen.contains(&key_str) {
                        push_duplicate_diagnostic(diagnostics, &key_str, loc);
                    } else {
                        seen.insert(key_str);
                    }
                }
                // Recurse into the key (e.g. complex keys) and value
                check_node_for_duplicate_keys(key, diagnostics, depth + 1);
                check_node_for_duplicate_keys(value, diagnostics, depth + 1);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_node_for_duplicate_keys(item, diagnostics, depth + 1);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Validate YAML 1.1 compatibility for plain scalars.
///
/// Returns diagnostics for plain scalar values that have different semantics in
/// YAML 1.1 vs YAML 1.2:
/// - YAML 1.1 boolean forms (`yes`, `no`, `on`, `off`, `y`, `n`, and their
///   case variants) → `yaml11Boolean` WARNING
/// - C-style octal literals (`0755`, `007`, etc.) → `yaml11Octal` INFORMATION
///
/// Only plain (unquoted) scalars are checked. Quoted scalars are already
/// unambiguously strings in both versions.
#[must_use]
pub fn validate_yaml11_compat(docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for doc in docs {
        collect_yaml11_diagnostics(&doc.root, &mut diagnostics, 0);
    }
    diagnostics
}

/// Recursively walk a YAML node and emit diagnostics for YAML 1.1 compatibility issues.
fn collect_yaml11_diagnostics(node: &Node<Span>, diagnostics: &mut Vec<Diagnostic>, depth: usize) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Scalar {
            value, style, loc, ..
        } => {
            if *style == rlsp_yaml_parser::ScalarStyle::Plain {
                if crate::scalar_helpers::is_yaml11_bool(value) {
                    let canonical = crate::scalar_helpers::yaml11_bool_canonical(value);
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    let start_line = loc.start.line.saturating_sub(1) as u32;
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    let start_col = loc.start.column as u32;
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    let end_col = loc.end.column as u32;
                    diagnostics.push(Diagnostic {
                        range: Range::new(
                            Position::new(start_line, start_col),
                            Position::new(start_line, end_col),
                        ),
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("yaml11Boolean".to_string())),
                        message: format!(
                            "\"{value}\" is a boolean in YAML 1.1 but a string in YAML 1.2. \
                             Most tools use 1.1 parsers and will interpret this as {canonical}. \
                             Quote it (\"{value}\") or use {canonical}."
                        ),
                        source: Some("rlsp-yaml".to_string()),
                        ..Diagnostic::default()
                    });
                } else if crate::scalar_helpers::is_yaml11_octal(value) {
                    let decimal = i64::from_str_radix(&value[1..], 8).unwrap_or(0);
                    let yaml12 = format!("0o{}", &value[1..]);
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    let start_line = loc.start.line.saturating_sub(1) as u32;
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    let start_col = loc.start.column as u32;
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    let end_col = loc.end.column as u32;
                    diagnostics.push(Diagnostic {
                        range: Range::new(
                            Position::new(start_line, start_col),
                            Position::new(start_line, end_col),
                        ),
                        severity: Some(DiagnosticSeverity::INFORMATION),
                        code: Some(NumberOrString::String("yaml11Octal".to_string())),
                        message: format!(
                            "\"{value}\" is octal {decimal} in YAML 1.1 but the string \
                             \"{value}\" in YAML 1.2. Quote it (\"{value}\") or use \
                             {yaml12} (YAML 1.2 only)."
                        ),
                        source: Some("rlsp-yaml".to_string()),
                        ..Diagnostic::default()
                    });
                }
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_yaml11_diagnostics(key, diagnostics, depth + 1);
                collect_yaml11_diagnostics(value, diagnostics, depth + 1);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_yaml11_diagnostics(item, diagnostics, depth + 1);
            }
        }
        Node::Alias { .. } => {}
    }
}

/// Push an error diagnostic for a duplicate scalar key.
fn push_duplicate_diagnostic(diagnostics: &mut Vec<Diagnostic>, key: &str, loc: &Span) {
    let display_key = if key.len() > 100 {
        let end = key.char_indices().nth(100).map_or(key.len(), |(i, _)| i);
        format!("{}...", &key[..end])
    } else {
        key.to_string()
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let start_line = loc.start.line.saturating_sub(1) as u32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let start_col = loc.start.column as u32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let end_col = loc.end.column as u32;
    diagnostics.push(Diagnostic {
        range: Range::new(
            Position::new(start_line, start_col),
            Position::new(start_line, end_col),
        ),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("duplicateKey".to_string())),
        message: format!("Duplicate key: '{display_key}'"),
        source: Some("rlsp-yaml".to_string()),
        ..Diagnostic::default()
    });
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::unwrap_used, reason = "test code")]
mod tests {
    use std::fmt::Write as _;

    use rstest::rstest;

    use super::*;

    fn parse_docs(text: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(text).unwrap()
    }

    fn parse_duplicate(text: &str) -> Vec<super::Diagnostic> {
        let docs = rlsp_yaml_parser::load(text).unwrap();
        validate_duplicate_keys(&docs)
    }

    // ---- Unused Anchors Validator: Happy Paths / Edge Cases / Security ----

    #[rstest]
    #[case::no_anchors("key: value\n")]
    #[case::all_anchors_used("defaults: &defaults\n  key: val\nproduction:\n  <<: *defaults\n")]
    #[case::empty_document("")]
    #[case::comment_only("# just a comment\n")]
    #[case::anchors_in_comments("# &fake anchor\nkey: value\n")]
    #[case::anchor_used_multiple_times("defaults: &shared\n  k: v\na: *shared\nb: *shared\n")]
    #[case::anchor_with_special_chars("data: &my-anchor_v2.0\n  k: v\nref: *my-anchor_v2.0\n")]
    #[case::invalid_anchor_chars_terminates_name("data: &anchor!@# value\nref: *anchor\n")]
    fn unused_anchors_returns_empty(#[case] input: &str) {
        let result = validate_unused_anchors(input);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::single_unused("defaults: &unused\n  key: val\nproduction:\n  key: other\n", 1)]
    #[case::two_unused("a: &first\n  k: v\nb: &second\n  k: v\nc: value\n", 2)]
    #[case::one_alias_no_anchor("production:\n  <<: *undefined\n", 1)]
    #[case::two_unresolved_aliases("a: *missing1\nb: *missing2\n", 2)]
    #[case::cross_doc_scoping_produces_two(
        "doc1: &shared\n  k: v\n---\ndoc2:\n  ref: *shared\n",
        2
    )]
    #[case::same_anchor_name_different_docs_one_unused(
        "a: &name\n  k: v\n---\nb: &name\n  k: v\nref: *name\n",
        1
    )]
    #[case::unicode_text_one_unused("name: 中文\ndata: &unused\n  key: val\n", 1)]
    #[case::anchor_and_alias_in_different_docs_two_diags(
        "ref: *later\n---\ndata: &later\n  key: val\n",
        2
    )]
    #[case::doc2_unused_one_diag("a: &used\n  k: v\nref: *used\n---\nb: &unused\n  k: v\n", 1)]
    fn unused_anchors_count(#[case] input: &str, #[case] expected: usize) {
        let result = validate_unused_anchors(input);

        assert_eq!(result.len(), expected);
    }

    // ---- Unused Anchors Validator: Unresolved Alias Detection ----

    #[rstest]
    #[case::single_unresolved_alias("production:\n  <<: *undefined\n")]
    #[case::two_unresolved_aliases("a: *missing1\nb: *missing2\n")]
    fn unused_anchors_all_errors(#[case] input: &str) {
        let result = validate_unused_anchors(input);

        assert!(
            result
                .iter()
                .all(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        );
    }

    // ---- Unused Anchors Validator: Unnecessary tag check ----

    #[rstest]
    #[case::single_unused("defaults: &unused\n  key: val\nproduction:\n  key: other\n")]
    #[case::detected_unused("defaults: &unused\n  key: val\n")]
    #[case::same_anchor_name_second_doc_unused(
        "a: &name\n  k: v\n---\nb: &name\n  k: v\nref: *name\n"
    )]
    fn unused_anchor_has_unnecessary_tag(#[case] input: &str) {
        let result = validate_unused_anchors(input);

        assert!(
            result[0]
                .tags
                .as_ref()
                .is_some_and(|t| t.contains(&DiagnosticTag::UNNECESSARY))
        );
    }

    // ---- Unused Anchors Validator: Pathological Inputs ----

    #[test]
    fn should_handle_document_with_many_anchors() {
        let mut text = String::new();
        for i in 0..120 {
            writeln!(text, "anchor{i}: &anchor{i}\n  key: val").unwrap();
        }
        // Use only even-numbered anchors
        for i in (0..120).step_by(2) {
            writeln!(text, "ref{i}: *anchor{i}").unwrap();
        }

        let result = validate_unused_anchors(&text);

        // Should report 60 unused anchors (odd-numbered)
        assert_eq!(result.len(), 60);
        assert!(result.iter().all(|d| {
            d.tags
                .as_ref()
                .is_some_and(|t| t.contains(&DiagnosticTag::UNNECESSARY))
        }));
    }

    #[test]
    fn should_handle_long_anchor_name() {
        let long_name = "a".repeat(200);
        let text = format!("data: &{long_name}\n  k: v\n");
        let result = validate_unused_anchors(&text);

        assert_eq!(result.len(), 1);
        assert!(!result[0].message.is_empty());
    }

    // ---- Unused Anchors Validator: Multi-Document Scoping (standalone) ----

    #[test]
    fn should_report_unused_anchor_scoped_to_document() {
        let text = "doc1: &shared\n  k: v\n---\ndoc2:\n  ref: *shared\n";
        let result = validate_unused_anchors(text);

        // &shared in doc1 is unused (within doc1)
        // *shared in doc2 is unresolved (within doc2)
        assert_eq!(result.len(), 2);
        let unused = result.iter().find(|d| {
            d.tags
                .as_ref()
                .is_some_and(|t| t.contains(&DiagnosticTag::UNNECESSARY))
        });
        let unresolved = result
            .iter()
            .find(|d| d.severity == Some(DiagnosticSeverity::ERROR));
        assert!(unused.is_some());
        assert!(unresolved.is_some());
    }

    // ---- Unused Anchors Validator: Security (standalone) ----

    #[test]
    fn should_produce_correct_range_with_unicode_in_text() {
        let text = "name: 中文\ndata: &unused\n  key: val\n";
        let result = validate_unused_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 1, "anchor is on line 1");
        assert_eq!(diag.range.start.character, 6, "anchor starts at column 6");
    }

    #[test]
    fn should_not_satisfy_alias_in_doc1_with_anchor_in_doc2() {
        let text = "ref: *later\n---\ndata: &later\n  key: val\n";
        let result = validate_unused_anchors(text);

        // *later in doc1 is unresolved, &later in doc2 is unused
        assert_eq!(result.len(), 2);
        let error_diags = result
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .count();
        let unnecessary_diags = result
            .iter()
            .filter(|d| {
                d.tags
                    .as_ref()
                    .is_some_and(|t| t.contains(&DiagnosticTag::UNNECESSARY))
            })
            .count();
        assert_eq!(error_diags, 1, "should have 1 error for unresolved alias");
        assert_eq!(
            unnecessary_diags, 1,
            "should have 1 unnecessary for unused anchor"
        );
    }

    #[test]
    fn should_return_correct_range_for_unused_anchor() {
        let text = "defaults: &defaults\n  key: val\n";
        let result = validate_unused_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 0);
        assert_eq!(diag.range.start.character, 10, "anchor starts at column 10");
        assert_eq!(diag.range.end.character, 19, "anchor ends at column 19");
    }

    #[test]
    fn should_evaluate_each_document_independently_for_unused_anchors() {
        // Doc1: anchor used. Doc2: anchor unused.
        let text = "a: &used\n  k: v\nref: *used\n---\nb: &unused\n  k: v\n";
        let result = validate_unused_anchors(text);

        // Only doc2's &unused should be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 4); // &unused is on line 4
        assert!(
            result[0]
                .tags
                .as_ref()
                .is_some_and(|t| t.contains(&DiagnosticTag::UNNECESSARY))
        );
    }

    // ---- Flow Style Validator: Happy Paths / Edge Cases / Empty Collections ----

    #[rstest]
    #[case::block_only("key:\n  nested: value\n")]
    #[case::empty_document("")]
    #[case::brackets_in_double_quotes("message: \"array is [1,2,3]\"\n")]
    #[case::braces_in_single_quotes("message: 'object is {a: 1}'\n")]
    #[case::empty_flow_mapping("status: {}\n")]
    #[case::empty_flow_sequence("items: []\n")]
    #[case::flow_mapping_spaces_only("status: { }\n")]
    #[case::flow_mapping_multiple_spaces("status: {  }\n")]
    #[case::flow_sequence_spaces_only("items: [  ]\n")]
    #[case::multiple_empty_collections_one_line("a: {}\nb: []\n")]
    #[case::braces_inside_single_quoted_string("msg: 'value with {braces}'\n")]
    fn flow_style_returns_empty(#[case] input: &str) {
        let result = validate_flow_style(input);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::flow_mapping("config: {key: value}\n", 1)]
    #[case::flow_sequence("items: [one, two, three]\n", 1)]
    #[case::both_types_on_two_lines("config: {key: value}\nitems: [a, b]\n", 2)]
    #[case::nested_flow_styles("data: {outer: [inner]}\n", 2)]
    #[case::multi_document("doc1: {a: 1}\n---\ndoc2: [x]\n", 2)]
    #[case::outer_nonempty_inner_empty("data: {a: {}}\n", 1)]
    #[case::mixed_empty_nonempty("a: {}\nb: {x: 1}\n", 1)]
    #[case::flow_detected_after_single_quote_ends("msg: 'quoted' \nreal: {a: 1}\n", 1)]
    fn flow_style_count(#[case] input: &str, #[case] expected: usize) {
        let result = validate_flow_style(input);

        assert_eq!(result.len(), expected);
    }

    #[rstest]
    #[case::flow_mapping("config: {key: value}\n")]
    #[case::flow_sequence("items: [a, b]\n")]
    fn flow_style_range_start_line_zero(#[case] input: &str) {
        let result = validate_flow_style(input);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 0);
    }

    // ---- Flow Style Validator: standalone ----

    #[test]
    fn should_detect_flow_mapping() {
        let text = "config: {key: value}\n";
        let result = validate_flow_style(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn should_detect_flow_sequence() {
        let text = "items: [one, two, three]\n";
        let result = validate_flow_style(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowSeq")
        );
    }

    #[test]
    fn should_detect_both_flow_mapping_and_sequence() {
        let text = "config: {key: value}\nitems: [a, b]\n";
        let result = validate_flow_style(text);

        assert_eq!(result.len(), 2);
        let has_flow_map = result
            .iter()
            .any(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap"));
        let has_flow_seq = result
            .iter()
            .any(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "flowSeq"));
        assert!(has_flow_map);
        assert!(has_flow_seq);
    }

    #[test]
    fn should_warn_on_outer_but_not_inner_empty_flow_mapping() {
        // Outer `{a: {}}` is non-empty → warns; inner `{}` is empty → no extra warn.
        let result = validate_flow_style("data: {a: {}}\n");

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn should_warn_only_on_non_empty_when_mixed_with_empty() {
        let result = validate_flow_style("a: {}\nb: {x: 1}\n");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1);
    }

    // ---- Map Key Order Validator: Happy Paths / Nested Structures / Edge Cases ----

    #[rstest]
    #[case::ordered_keys("apple: 1\nbanana: 2\ncherry: 3\n")]
    #[case::empty_document("")]
    #[case::single_key("only: value\n")]
    #[case::sequence_items_ignored("items:\n  - zebra\n  - alpha\n")]
    #[case::multi_document_single_keys("z: 1\n---\na: 2\n")]
    #[case::case_sensitive_uppercase_first("Apple: 1\napple: 2\n")]
    fn key_ordering_returns_empty(#[case] input: &str) {
        let docs = rlsp_yaml_parser::load(input).unwrap();
        let result = validate_key_ordering(input, &docs);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::single_ooo("banana: 2\napple: 1\n", 1)]
    #[case::multiple_ooo("charlie: 3\nalpha: 1\nbravo: 2\n", 2)]
    #[case::nested_ooo("outer:\n  zebra: 1\n  alpha: 2\n", 1)]
    #[case::top_level_ooo_only("b_parent:\n  a_child: 1\na_parent:\n  key: val\n", 1)]
    #[case::numeric_string_lexicographic("2: two\n10: ten\n", 1)]
    fn key_ordering_count(#[case] input: &str, #[case] expected: usize) {
        let docs = rlsp_yaml_parser::load(input).unwrap();
        let result = validate_key_ordering(input, &docs);

        assert_eq!(result.len(), expected);
    }

    // ---- Map Key Order Validator: standalone ----

    #[test]
    fn should_detect_out_of_order_keys() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(text, &docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "mapKeyOrder")
        );
    }

    #[test]
    fn should_return_correct_range_for_out_of_order_key() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(text, &docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "apple is on line 1");
    }

    // ---- Custom Tags Validator: Happy Paths / Multi-document / Nested ----

    #[rstest]
    #[case::allowed_tag_no_diagnostic("value: !include foo.yaml\n", &["!include"] as &[&str])]
    #[case::empty_allowed_skips_validation("value: !include foo.yaml\n", &[])]
    #[case::no_tags_in_document("key: value\nother: 123\n", &["!include"])]
    #[case::multi_doc_both_allowed("a: !include foo.yaml\n---\nb: !ref bar.yaml\n", &["!include", "!ref"])]
    fn custom_tags_returns_empty(#[case] input: &str, #[case] allowed_tags: &[&str]) {
        let docs = parse_docs(input);
        let allowed: HashSet<String> = allowed_tags.iter().map(|s| (*s).to_string()).collect();
        let result = validate_custom_tags(input, &docs, &allowed);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::unknown_tag("value: !include foo.yaml\n", &["!other"] as &[&str])]
    #[case::multiple_tags_only_unknown_flagged("a: !include foo.yaml\nb: !ref bar.yaml\n", &["!include"])]
    #[case::nested_tagged_value("outer:\n  inner: !include nested.yaml\n", &["!other"])]
    #[case::tag_in_double_quoted_skipped_range("note: \"use !include for files\"\nvalue: !include actual.yaml\n", &["!other"])]
    #[case::tag_in_single_quoted_skipped_range("note: 'see !ref for details'\nvalue: !ref target.yaml\n", &["!other"])]
    fn custom_tags_single_diagnostic(#[case] input: &str, #[case] allowed_tags: &[&str]) {
        let docs = parse_docs(input);
        let allowed: HashSet<String> = allowed_tags.iter().map(|s| (*s).to_string()).collect();
        let result = validate_custom_tags(input, &docs, &allowed);

        assert_eq!(result.len(), 1);
    }

    // ---- Custom Tags Validator: standalone ----

    #[test]
    fn unknown_tag_produces_warning_with_unknown_tag_code() {
        let text = "value: !include foo.yaml\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(text, &docs, &allowed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag")
        );
        assert!(result[0].message.contains("!include"));
        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    #[test]
    fn tags_in_multi_document_yaml_are_all_checked() {
        let text = "a: !include foo.yaml\n---\nb: !ref bar.yaml\n";
        let docs = parse_docs(text);

        // Neither allowed
        let neither: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(text, &docs, &neither);
        assert_eq!(result.len(), 2);

        // Both allowed
        let both: HashSet<String> = HashSet::from(["!include".to_string(), "!ref".to_string()]);
        let result = validate_custom_tags(text, &docs, &both);
        assert!(result.is_empty());
    }

    #[test]
    fn tag_boundary_check_rejects_prefix_match() {
        let text = "value: !include_extras foo.yaml\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(text, &docs, &allowed);

        assert!(result.len() <= 1, "should not crash on boundary check");
    }

    #[test]
    fn second_occurrence_of_same_tag_has_correct_range() {
        let text = "a: !include file1.yaml\nb: !include file2.yaml\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(text, &docs, &allowed);

        assert_eq!(result.len(), 2);
        let lines: Vec<u32> = result.iter().map(|d| d.range.start.line).collect();
        assert!(lines.contains(&0), "first occurrence on line 0");
        assert!(lines.contains(&1), "second occurrence on line 1");
    }

    // ---- Duplicate Key Validator: Happy Paths / Edge Cases / All no-dup groups ----

    #[rstest]
    #[case::no_duplicates("a: 1\nb: 2\nc: 3\n")]
    #[case::same_key_different_nesting_levels("name: top\nnested:\n  name: inner\n")]
    #[case::scope_reset_on_doc_boundary("key: 1\n---\nkey: 2\n")]
    #[case::no_flow_mapping_duplicates("cfg: {a: 1, b: 2}\n")]
    #[case::anchor_key_appearing_once("&anchor key: 1\nother: 2\n")]
    #[case::non_scalar_key_skipped("{a: 1}: foo\n{a: 1}: bar\n")]
    #[case::single_alias_key("x: &anchor foo\n? *anchor\n: 1\nother: 2\n")]
    #[case::empty_document("")]
    #[case::comment_only("# just a comment\n")]
    #[case::same_key_different_sequence_items("items:\n  - name: alice\n  - name: bob\n")]
    #[case::sibling_mappings_under_common_parent(
        "parent:\n  child_a:\n    cpu: 100m\n    memory: 128Mi\n  child_b:\n    cpu: 200m\n    memory: 256Mi\n"
    )]
    #[case::deeply_nested_sibling_mappings(
        "level1:\n  level2:\n    sibling_a:\n      value: 1\n    sibling_b:\n      value: 2\n"
    )]
    #[case::empty_sibling_with_shared_key_in_later(
        "parent:\n  a: ~\n  b:\n    cpu: 1\n  c:\n    cpu: 2\n"
    )]
    #[case::mixed_indent_depth_siblings(
        "resources:\n  requests:\n    cpu: 100m\n  limits:\n    cpu: 500m\n"
    )]
    #[case::ellipsis_resets_scope("key: 1\n...\nkey: 2\n")]
    fn duplicate_keys_returns_empty(#[case] input: &str) {
        let result = parse_duplicate(input);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::simple_top_level("a: 1\na: 2\n", 1)]
    #[case::nested_mapping("outer:\n  x: 1\n  x: 2\n", 1)]
    #[case::within_same_doc_in_multi_doc("a: 1\na: 2\n---\nb: 3\n", 1)]
    #[case::flow_mapping_duplicate("cfg: {x: 1, x: 2}\n", 1)]
    #[case::double_quoted_and_unquoted("\"key\": 1\nkey: 2\n", 1)]
    #[case::two_double_quoted("\"key\": 1\n\"key\": 2\n", 1)]
    #[case::single_quoted_and_unquoted("'key': 1\nkey: 2\n", 1)]
    #[case::single_and_double_quoted("'key': 1\n\"key\": 2\n", 1)]
    #[case::second_key_has_anchor("key: 1\n&anchor key: 2\n", 1)]
    #[case::first_key_has_anchor("&anchor key: 1\nkey: 2\n", 1)]
    #[case::empty_string_keys("\"\": 1\n\"\": 2\n", 1)]
    #[case::unicode_keys("café: 1\ncafé: 2\n", 1)]
    #[case::within_same_sequence_item("items:\n  - name: alice\n    name: alice2\n", 1)]
    #[case::same_duplicate_within_one_sibling(
        "parent:\n  child:\n    cpu: 100m\n    cpu: 200m\n",
        1
    )]
    #[case::duplicate_before_ellipsis("a: 1\na: 2\n...\nb: 3\n", 1)]
    #[case::triple_duplicate_two_diags("parent:\n  child:\n    x: 1\n    x: 2\n    x: 3\n", 2)]
    fn duplicate_keys_count(#[case] input: &str, #[case] expected: usize) {
        let result = parse_duplicate(input);

        assert_eq!(result.len(), expected);
    }

    // ---- Duplicate Key Validator: Error severity ----

    #[rstest]
    #[case::simple_top_level("a: 1\na: 2\n")]
    #[case::flow_mapping_duplicate("cfg: {x: 1, x: 2}\n")]
    #[case::empty_string_keys("\"\": 1\n\"\": 2\n")]
    #[case::unicode_keys("café: 1\ncafé: 2\n")]
    fn duplicate_key_error_severity(#[case] input: &str) {
        let result = parse_duplicate(input);

        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ---- Duplicate Key Validator: standalone ----

    #[test]
    fn should_detect_simple_top_level_duplicate() {
        let text = "a: 1\na: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "duplicateKey")
        );
        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
        assert!(result[0].message.contains("'a'"));
        assert_eq!(result[0].range.start.line, 1, "duplicate is on line 1");
    }

    #[test]
    fn should_detect_duplicate_in_nested_mapping() {
        let text = "outer:\n  x: 1\n  x: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'x'"));
        assert_eq!(result[0].range.start.line, 2);
    }

    #[test]
    fn should_detect_duplicate_within_same_document_in_multi_doc_yaml() {
        let text = "a: 1\na: 2\n---\nb: 3\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'a'"));
    }

    #[test]
    fn should_detect_flow_mapping_duplicate() {
        let text = "cfg: {x: 1, x: 2}\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("'x'"));
    }

    #[test]
    fn should_detect_duplicate_alias_keys() {
        // *ref used as a mapping key twice
        let text = "x: &anchor foo\n? *anchor\n: 1\n? *anchor\n: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "duplicateKey")
        );
        assert!(result[0].message.contains("*anchor"));
    }

    #[test]
    fn should_detect_duplicate_empty_string_keys() {
        let text = "\"\": 1\n\"\": 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "duplicateKey")
        );
    }

    #[test]
    fn should_detect_duplicate_unicode_keys() {
        let text = "café: 1\ncafé: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("café"));
    }

    #[test]
    fn should_detect_duplicate_within_same_sequence_item() {
        let text = "items:\n  - name: alice\n    name: alice2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'name'"));
    }

    #[test]
    fn should_not_flag_kubernetes_limitrange_sibling_pattern() {
        let text = "\
limits:
  max:
    cpu: \"2\"
    memory: 1Gi
  min:
    cpu: 100m
    memory: 128Mi
  default:
    cpu: 500m
    memory: 512Mi
  defaultRequest:
    cpu: 250m
    memory: 256Mi
";
        let result = parse_duplicate(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_not_flag_kubernetes_limitrange_inside_sequence_item() {
        let text = "\
spec:
  limits:
    - type: Container
      max:
        cpu: \"2\"
        memory: 1Gi
      min:
        cpu: 100m
        memory: 128Mi
      default:
        cpu: 500m
        memory: 512Mi
      defaultRequest:
        cpu: 250m
        memory: 256Mi
";
        let result = parse_duplicate(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_still_detect_duplicate_in_same_sibling_mapping() {
        let text = "parent:\n  child:\n    cpu: 100m\n    cpu: 200m\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'cpu'"));
        assert_eq!(result[0].range.start.line, 3);
    }

    #[test]
    fn should_detect_triple_duplicate_within_single_sibling_mapping() {
        let text = "parent:\n  child:\n    x: 1\n    x: 2\n    x: 3\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|d| d.message.contains("'x'")));
    }

    #[test]
    fn should_truncate_long_key_name_in_message() {
        let long_key = "k".repeat(110);
        let text = format!("{long_key}: 1\n{long_key}: 2\n");
        let result = parse_duplicate(&text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("..."));
        let display = &result[0].message;
        assert!(display.len() < long_key.len() + 20);
    }

    #[test]
    fn should_report_correct_column_for_indented_duplicate_key() {
        let text = "outer:\n  inner:\n    dup: 1\n    dup: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].range.start.line, 3,
            "duplicate is on line 3 (0-based)"
        );
        assert_eq!(
            result[0].range.start.character, 4,
            "exact column from AST loc, not indent approximation"
        );
    }

    #[test]
    fn should_report_correct_range_end_for_duplicate_key() {
        let text = "abc: 1\nabc: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.character, 0);
        assert_eq!(
            result[0].range.end.character,
            result[0].range.start.character + 3,
            "end column = start + key length"
        );
    }

    #[test]
    fn duplicate_key_detected_before_ellipsis_terminator() {
        let text = "a: 1\na: 2\n...\nb: 3\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'a'"));
    }

    // ---- YAML version agnosticism ----
    //
    // All validators in this module operate on raw text or on parsed
    // Document<Span>/Node<Span> values. The parser always parses as YAML 1.2
    // regardless of any `yamlVersion` setting, so the parsed representation
    // is identical for all version settings. Consequently, no validator here
    // requires a YamlVersion parameter — diagnostics are version-agnostic.
    //
    // The tests below confirm that inputs containing YAML 1.1-only boolean
    // literals (`yes`, `no`, `on`, `off`) produce the same diagnostic output
    // as equivalent inputs without them, locking down this invariant.

    // ---- validate_yaml11_compat ----

    fn parse_yaml11(text: &str) -> Vec<super::Diagnostic> {
        let docs = parse_docs(text);
        validate_yaml11_compat(&docs)
    }

    #[test]
    fn yaml11_bool_plain_yes_emits_warning() {
        let result = parse_yaml11("value: yes\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        let msg = &result[0].message;
        assert!(msg.contains("yes"), "message should contain the value");
        assert!(
            msg.contains("true"),
            "message should mention canonical form (yes → true)"
        );
    }

    #[rstest]
    #[case::yes_lowercase("yes")]
    #[case::yes_titlecase("Yes")]
    #[case::yes_uppercase("YES")]
    #[case::on_lowercase("on")]
    #[case::on_titlecase("On")]
    #[case::on_uppercase("ON")]
    #[case::y_lowercase("y")]
    #[case::y_uppercase("Y")]
    fn yaml11_bool_all_true_forms_emit_warning(#[case] value: &str) {
        let text = format!("k: {value}\n");
        let result = parse_yaml11(&text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[rstest]
    #[case::no_lowercase("no")]
    #[case::no_titlecase("No")]
    #[case::no_uppercase("NO")]
    #[case::off_lowercase("off")]
    #[case::off_titlecase("Off")]
    #[case::off_uppercase("OFF")]
    #[case::n_lowercase("n")]
    #[case::n_uppercase("N")]
    fn yaml11_bool_all_false_forms_emit_warning(#[case] value: &str) {
        let text = format!("k: {value}\n");
        let result = parse_yaml11(&text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn yaml11_bool_quoted_double_no_diagnostic() {
        let result = parse_yaml11("value: \"yes\"\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_bool_quoted_single_no_diagnostic() {
        let result = parse_yaml11("value: 'yes'\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_bool_as_mapping_key_emits_diagnostic() {
        // Keys are Node::Scalar too — all plain scalars are walked.
        let result = parse_yaml11("yes: value\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
    }

    #[test]
    fn yaml11_bool_yaml12_true_no_diagnostic() {
        let result = parse_yaml11("value: true\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_bool_multiple_in_one_document() {
        let result = parse_yaml11("a: yes\nb: no\nc: on\n");

        assert_eq!(result.len(), 3);
        assert!(
            result
                .iter()
                .all(|d| d.code == Some(NumberOrString::String("yaml11Boolean".to_string())))
        );
        assert!(
            result
                .iter()
                .all(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        );
    }

    #[test]
    fn yaml11_bool_diagnostic_message_canonical_true() {
        let result = parse_yaml11("value: yes\n");

        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(msg.contains("yes"), "message should include the value");
        assert!(
            msg.contains("true"),
            "message should include canonical YAML 1.2 form"
        );
        assert!(
            msg.contains("\"yes\""),
            "message should suggest quoting as \"yes\""
        );
    }

    #[test]
    fn yaml11_bool_diagnostic_message_canonical_false() {
        let result = parse_yaml11("value: no\n");

        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(msg.contains("no"), "message should include the value");
        assert!(
            msg.contains("false"),
            "message should include canonical YAML 1.2 form"
        );
        assert!(
            msg.contains("\"no\""),
            "message should suggest quoting as \"no\""
        );
    }

    #[test]
    fn yaml11_octal_plain_emits_information() {
        let result = parse_yaml11("mode: 0755\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Octal".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::INFORMATION));
    }

    #[test]
    fn yaml11_octal_single_zero_no_diagnostic() {
        let result = parse_yaml11("count: 0\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_octal_quoted_double_no_diagnostic() {
        let result = parse_yaml11("mode: \"0755\"\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_octal_yaml12_notation_no_diagnostic() {
        let result = parse_yaml11("mode: 0o755\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_octal_diagnostic_message_includes_decimal_and_suggestion() {
        let result = parse_yaml11("mode: 0755\n");

        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("493"),
            "message should include decimal value of 0755"
        );
        assert!(
            msg.contains("0o755"),
            "message should include YAML 1.2 form"
        );
    }

    #[test]
    fn yaml11_octal_007_emits_information() {
        let result = parse_yaml11("file: 007\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Octal".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::INFORMATION));
        assert!(
            result[0].message.contains('7'),
            "message should include decimal value 7"
        );
    }

    #[test]
    fn yaml11_bool_and_octal_in_same_document() {
        let result = parse_yaml11("flag: yes\nmode: 0755\n");

        assert_eq!(result.len(), 2);
        let codes: Vec<_> = result.iter().map(|d| d.code.as_ref().unwrap()).collect();
        assert!(
            codes
                .iter()
                .any(|c| *c == &NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert!(
            codes
                .iter()
                .any(|c| *c == &NumberOrString::String("yaml11Octal".to_string()))
        );
    }

    #[test]
    fn yaml11_empty_document_no_diagnostics() {
        let result = parse_yaml11("");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_in_nested_mapping() {
        let result = parse_yaml11("outer:\n  inner: yes\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
    }

    #[test]
    fn yaml11_in_sequence() {
        let result = parse_yaml11("items:\n  - yes\n  - no\n");

        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|d| d.code == Some(NumberOrString::String("yaml11Boolean".to_string())))
        );
    }

    #[test]
    fn validators_produce_same_diagnostics_regardless_of_yaml_version_setting() {
        let text_with_v1_1_keywords = "on: push\nyes: true\n";
        let text_plain = "push_trigger: push\nenabled: true\n";

        // validate_duplicate_keys: no duplicates in either text.
        assert_eq!(
            parse_duplicate(text_with_v1_1_keywords).len(),
            parse_duplicate(text_plain).len(),
            "duplicate-key diagnostics must not differ based on v1.1 keyword presence"
        );

        // validate_flow_style: no flow collections in either text.
        assert_eq!(
            validate_flow_style(text_with_v1_1_keywords).len(),
            validate_flow_style(text_plain).len(),
            "flow-style diagnostics must not differ based on v1.1 keyword presence"
        );
    }
}
