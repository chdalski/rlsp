use std::collections::{HashMap, HashSet};

use saphyr::{ScalarOwned, YamlOwned};
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

        #[allow(clippy::cast_possible_truncation)]
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
                    #[allow(clippy::cast_possible_truncation)]
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
        #[allow(clippy::cast_possible_truncation)]
        let line_num = line_idx as u32;

        let mut in_single_quote = false;
        let mut in_double_quote = false;

        for (i, ch) in line.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                '{' if !in_single_quote && !in_double_quote => {
                    // Find matching closing brace
                    if let Some(close_pos) = find_closing_char(line, i, '{', '}') {
                        #[allow(clippy::cast_possible_truncation)]
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(line_num, i as u32),
                                Position::new(line_num, (close_pos + 1) as u32),
                            ),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("flowMap".to_string())),
                            message: "Flow mapping style detected".to_string(),
                            source: Some("rlsp-yaml".to_string()),
                            ..Diagnostic::default()
                        });
                    }
                }
                '[' if !in_single_quote && !in_double_quote => {
                    // Find matching closing bracket
                    if let Some(close_pos) = find_closing_char(line, i, '[', ']') {
                        #[allow(clippy::cast_possible_truncation)]
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(line_num, i as u32),
                                Position::new(line_num, (close_pos + 1) as u32),
                            ),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("flowSeq".to_string())),
                            message: "Flow sequence style detected".to_string(),
                            source: Some("rlsp-yaml".to_string()),
                            ..Diagnostic::default()
                        });
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
    docs: &[YamlOwned],
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
            doc,
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
    node: &YamlOwned,
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

    match node {
        YamlOwned::Tagged(tag, inner) => {
            let tag_str = tag.to_string();
            if !allowed_tags.contains(&tag_str) {
                let occurrence = *seen_counts.get(&tag_str).unwrap_or(&0);
                seen_counts.insert(tag_str.clone(), occurrence + 1);

                if let Some(range) = find_tag_occurrence(lines, &tag_str, occurrence) {
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
            collect_tag_diagnostics(
                inner,
                lines,
                allowed_tags,
                seen_counts,
                diagnostics,
                depth + 1,
            );
        }
        YamlOwned::Mapping(map) => {
            for (key, value) in map {
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
        YamlOwned::Sequence(arr) => {
            for item in arr {
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
        YamlOwned::Value(_)
        | YamlOwned::Alias(_)
        | YamlOwned::BadValue
        | YamlOwned::Representation(_, _, _) => {}
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
                    #[allow(clippy::cast_possible_truncation)]
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
/// The `docs` parameter contains the parsed YAML documents from saphyr.
#[must_use]
pub fn validate_key_ordering(text: &str, docs: &[YamlOwned]) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diagnostics = Vec::new();

    for doc in docs {
        check_yaml_ordering(doc, &lines, &mut diagnostics, 0);
    }

    diagnostics
}

/// Recursively check YAML nodes for key ordering, with depth limit.
fn check_yaml_ordering(
    node: &YamlOwned,
    lines: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        YamlOwned::Mapping(map) => {
            // Extract keys in order
            let keys: Vec<String> = map
                .keys()
                .filter_map(|k| match k {
                    YamlOwned::Value(ScalarOwned::String(s)) => Some(s.clone()),
                    YamlOwned::Value(ScalarOwned::Integer(i)) => Some(i.to_string()),
                    YamlOwned::Value(ScalarOwned::FloatingPoint(f)) => Some(f.to_string()),
                    YamlOwned::Value(ScalarOwned::Boolean(b)) => Some(b.to_string()),
                    YamlOwned::Sequence(_)
                    | YamlOwned::Mapping(_)
                    | YamlOwned::Alias(_)
                    | YamlOwned::Value(ScalarOwned::Null)
                    | YamlOwned::BadValue
                    | YamlOwned::Tagged(_, _)
                    | YamlOwned::Representation(_, _, _) => None,
                })
                .collect();

            // Check if keys are in alphabetical order
            // Track the maximum key seen so far to catch all out-of-order keys
            let mut max_key: &str = keys.first().map_or("", String::as_str);

            for key in keys.iter().skip(1) {
                if key.as_str() < max_key {
                    // Find the line number for this key
                    if let Some(line_num) = find_key_line(key, lines) {
                        #[allow(clippy::cast_possible_truncation)]
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
            for value in map.values() {
                check_yaml_ordering(value, lines, diagnostics, depth + 1);
            }
        }
        YamlOwned::Sequence(arr) => {
            // Recursively check array elements
            for item in arr {
                check_yaml_ordering(item, lines, diagnostics, depth + 1);
            }
        }
        YamlOwned::Value(_)
        | YamlOwned::Alias(_)
        | YamlOwned::BadValue
        | YamlOwned::Tagged(_, _)
        | YamlOwned::Representation(_, _, _) => {}
    }
}

/// Find the line number where a key appears in the text.
fn find_key_line(key: &str, lines: &[&str]) -> Option<u32> {
    lines.iter().enumerate().find_map(|(line_idx, line)| {
        let trimmed = line.trim_start();
        if trimmed.starts_with(key) && trimmed[key.len()..].trim_start().starts_with(':') {
            #[allow(clippy::cast_possible_truncation)]
            Some(line_idx as u32)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Unused Anchors Validator: Happy Paths ----

    #[test]
    fn should_return_empty_for_document_with_no_anchors() {
        let result = validate_unused_anchors("key: value\n");

        assert!(result.is_empty());
    }

    #[test]
    fn should_return_empty_when_all_anchors_are_used() {
        let text = "defaults: &defaults\n  key: val\nproduction:\n  <<: *defaults\n";
        let result = validate_unused_anchors(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_detect_unused_anchor() {
        let text = "defaults: &unused\n  key: val\nproduction:\n  key: other\n";
        let result = validate_unused_anchors(text);

        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .tags
                .as_ref()
                .is_some_and(|tags| tags.contains(&DiagnosticTag::UNNECESSARY))
        );
    }

    #[test]
    fn should_detect_multiple_unused_anchors() {
        let text = "a: &first\n  k: v\nb: &second\n  k: v\nc: value\n";
        let result = validate_unused_anchors(text);

        assert_eq!(result.len(), 2);
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
    fn should_mark_diagnostic_with_unnecessary_tag() {
        let text = "defaults: &unused\n  key: val\n";
        let result = validate_unused_anchors(text);

        assert_eq!(result.len(), 1);
        assert!(
            result[0]
                .tags
                .as_ref()
                .is_some_and(|tags| tags.contains(&DiagnosticTag::UNNECESSARY))
        );
    }

    // ---- Unused Anchors Validator: Unresolved Alias Detection ----

    #[test]
    fn should_detect_alias_with_no_matching_anchor() {
        let text = "production:\n  <<: *undefined\n";
        let result = validate_unused_anchors(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn should_detect_multiple_unresolved_aliases() {
        let text = "a: *missing1\nb: *missing2\n";
        let result = validate_unused_anchors(text);

        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        );
    }

    // ---- Unused Anchors Validator: Edge Cases ----

    #[test]
    fn should_return_empty_for_empty_document() {
        let result = validate_unused_anchors("");

        assert!(result.is_empty());
    }

    #[test]
    fn should_return_empty_for_comment_only_document() {
        let result = validate_unused_anchors("# just a comment\n");

        assert!(result.is_empty());
    }

    #[test]
    fn should_not_report_anchors_in_comments() {
        let text = "# &fake anchor\nkey: value\n";
        let result = validate_unused_anchors(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_handle_anchor_used_multiple_times() {
        let text = "defaults: &shared\n  k: v\na: *shared\nb: *shared\n";
        let result = validate_unused_anchors(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_handle_anchor_with_special_characters() {
        let text = "data: &my-anchor_v2.0\n  k: v\nref: *my-anchor_v2.0\n";
        let result = validate_unused_anchors(text);

        assert!(result.is_empty());
    }

    // ---- Unused Anchors Validator: Multi-Document Scoping ----

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

    #[test]
    fn should_treat_same_anchor_name_in_different_documents_independently() {
        let text = "a: &name\n  k: v\n---\nb: &name\n  k: v\nref: *name\n";
        let result = validate_unused_anchors(text);

        // &name in doc1 is unused
        // &name in doc2 is used by *name in doc2
        assert_eq!(result.len(), 1);
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
        // Generate YAML with 100+ anchors, some used, some not
        let mut text = String::new();
        for i in 0..120 {
            text.push_str(&format!("anchor{}: &anchor{}\n  key: val\n", i, i));
        }
        // Use only even-numbered anchors
        for i in (0..120).step_by(2) {
            text.push_str(&format!("ref{}: *anchor{}\n", i, i));
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
        let text = format!("data: &{}\n  k: v\n", long_name);
        let result = validate_unused_anchors(&text);

        assert_eq!(result.len(), 1);
        // Message should exist and not crash
        assert!(!result[0].message.is_empty());
    }

    // ---- Unused Anchors Validator: Additional Security Tests ----

    #[test]
    fn should_ignore_invalid_anchor_name_characters() {
        // &anchor!@# parses as anchor "anchor" (! terminates the name)
        let text = "data: &anchor!@# value\nref: *anchor\n";
        let result = validate_unused_anchors(text);

        // anchor "anchor" is used by alias "anchor" - no diagnostics
        assert!(result.is_empty());
    }

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

    // ---- Flow Style Validator: Happy Paths ----

    #[test]
    fn should_return_empty_for_block_style_only() {
        let text = "key:\n  nested: value\n";
        let result = validate_flow_style(text);

        assert!(result.is_empty());
    }

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
    fn should_return_correct_range_for_flow_mapping() {
        let text = "config: {key: value}\n";
        let result = validate_flow_style(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 0);
    }

    #[test]
    fn should_return_correct_range_for_flow_sequence() {
        let text = "items: [a, b]\n";
        let result = validate_flow_style(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 0);
    }

    // ---- Flow Style Validator: Edge Cases ----

    #[test]
    fn should_return_empty_for_empty_document_flow() {
        let result = validate_flow_style("");

        assert!(result.is_empty());
    }

    #[test]
    fn should_not_detect_brackets_in_quoted_strings() {
        // Implementation choice: quote-aware scanning (avoid false positives)
        let text = "message: \"array is [1,2,3]\"\n";
        let result = validate_flow_style(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_not_detect_braces_in_quoted_strings() {
        // Implementation choice: quote-aware scanning (avoid false positives)
        let text = "message: 'object is {a: 1}'\n";
        let result = validate_flow_style(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_detect_nested_flow_styles() {
        let text = "data: {outer: [inner]}\n";
        let result = validate_flow_style(text);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn should_handle_flow_style_in_multi_document() {
        let text = "doc1: {a: 1}\n---\ndoc2: [x]\n";
        let result = validate_flow_style(text);

        assert_eq!(result.len(), 2);
    }

    // ---- Map Key Order Validator: Happy Paths ----

    #[test]
    fn should_return_empty_for_alphabetically_ordered_keys() {
        let text = "apple: 1\nbanana: 2\ncherry: 3\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert!(result.is_empty());
    }

    #[test]
    fn should_detect_out_of_order_keys() {
        let text = "banana: 2\napple: 1\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
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
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "apple is on line 1");
    }

    #[test]
    fn should_detect_multiple_out_of_order_keys() {
        let text = "charlie: 3\nalpha: 1\nbravo: 2\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert_eq!(result.len(), 2);
    }

    // ---- Map Key Order Validator: Nested Structures ----

    #[test]
    fn should_check_ordering_within_nested_mappings() {
        let text = "outer:\n  zebra: 1\n  alpha: 2\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert_eq!(result.len(), 1, "alpha is out of order within outer");
    }

    #[test]
    fn should_check_ordering_at_each_level_independently() {
        let text = "b_parent:\n  a_child: 1\na_parent:\n  key: val\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert_eq!(result.len(), 1, "a_parent is out of order at top level");
    }

    // ---- Map Key Order Validator: Edge Cases ----

    #[test]
    fn should_return_empty_for_empty_document_ordering() {
        let text = "";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert!(result.is_empty());
    }

    #[test]
    fn should_return_empty_for_single_key() {
        let text = "only: value\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert!(result.is_empty());
    }

    #[test]
    fn should_handle_numeric_string_keys() {
        // Implementation choice: lexicographic comparison ("10" < "2" lexicographically)
        let text = "2: two\n10: ten\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        // "10" comes after "2" but should come before (lexicographically "1" < "2")
        assert_eq!(result.len(), 1, "10 should be flagged as out of order");
    }

    #[test]
    fn should_ignore_sequence_items_for_ordering() {
        let text = "items:\n  - zebra\n  - alpha\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        assert!(result.is_empty());
    }

    #[test]
    fn should_handle_multi_document_key_ordering() {
        let text = "z: 1\n---\na: 2\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        // First doc has single key, second doc has single key
        assert!(result.is_empty());
    }

    #[test]
    fn should_be_case_sensitive() {
        // Implementation choice: case-sensitive comparison ("Apple" != "apple", "Apple" < "apple")
        let text = "Apple: 1\napple: 2\n";
        let docs = {
            use saphyr::LoadableYamlNode;
            YamlOwned::load_from_str(text).unwrap()
        };
        let result = validate_key_ordering(text, &docs);

        // "Apple" < "apple" lexicographically (uppercase comes before lowercase in ASCII)
        assert!(result.is_empty());
    }

    // ---- Custom Tags Validator: helpers ----

    fn parse_docs(text: &str) -> Vec<YamlOwned> {
        use saphyr::LoadableYamlNode;
        YamlOwned::load_from_str(text).unwrap()
    }

    fn allowed(tags: &[&str]) -> HashSet<String> {
        tags.iter().map(|s| (*s).to_string()).collect()
    }

    // ---- Custom Tags Validator: Happy Paths ----

    #[test]
    fn unknown_tag_produces_warning_with_unknown_tag_code() {
        let text = "value: !include foo.yaml\n";
        let docs = parse_docs(text);
        let result = validate_custom_tags(text, &docs, &allowed(&["!other"]));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag")
        );
        assert!(result[0].message.contains("!include"));
        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    #[test]
    fn allowed_tag_produces_no_diagnostic() {
        let text = "value: !include foo.yaml\n";
        let docs = parse_docs(text);
        let result = validate_custom_tags(text, &docs, &allowed(&["!include"]));
        assert!(result.is_empty());
    }

    #[test]
    fn empty_allowed_tags_returns_no_diagnostics() {
        // Even though !include is present, empty set skips validation
        let text = "value: !include foo.yaml\n";
        let docs = parse_docs(text);
        let result = validate_custom_tags(text, &docs, &allowed(&[]));
        assert!(result.is_empty());
    }

    #[test]
    fn multiple_tags_only_unknown_ones_flagged() {
        let text = "a: !include foo.yaml\nb: !ref bar.yaml\n";
        let docs = parse_docs(text);
        let result = validate_custom_tags(text, &docs, &allowed(&["!include"]));
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("!ref"));
    }

    #[test]
    fn no_tags_in_document_returns_empty_vec() {
        let text = "key: value\nother: 123\n";
        let docs = parse_docs(text);
        let result = validate_custom_tags(text, &docs, &allowed(&["!include"]));
        assert!(result.is_empty());
    }

    // ---- Custom Tags Validator: Multi-document ----

    #[test]
    fn tags_in_multi_document_yaml_are_all_checked() {
        let text = "a: !include foo.yaml\n---\nb: !ref bar.yaml\n";
        let docs = parse_docs(text);

        // Neither allowed
        let result = validate_custom_tags(text, &docs, &allowed(&["!other"]));
        assert_eq!(result.len(), 2);

        // Both allowed
        let result = validate_custom_tags(text, &docs, &allowed(&["!include", "!ref"]));
        assert!(result.is_empty());
    }

    // ---- Custom Tags Validator: Nested tags ----

    #[test]
    fn nested_tagged_value_is_found() {
        // Tag on a value inside a mapping
        let text = "outer:\n  inner: !include nested.yaml\n";
        let docs = parse_docs(text);
        let result = validate_custom_tags(text, &docs, &allowed(&["!other"]));
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("!include"));
    }

    // ---- Custom Tags Validator: Quote-aware position scanning ----

    #[test]
    fn tag_in_quoted_string_does_not_shadow_real_tag_range() {
        // The AST sees one Tagged node (!include on line 1).
        // Raw text has "!include" on line 0 inside quotes — must be skipped.
        let text = "note: \"use !include for files\"\nvalue: !include actual.yaml\n";
        let docs = parse_docs(text);
        let result = validate_custom_tags(text, &docs, &allowed(&["!other"]));
        assert_eq!(result.len(), 1);
        // Diagnostic must point to line 1, not line 0 (the quoted mention).
        assert_eq!(result[0].range.start.line, 1);
    }
}
