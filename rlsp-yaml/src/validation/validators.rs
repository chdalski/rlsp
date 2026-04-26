// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{CollectionStyle, Span};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range,
};

/// An anchor definition collected from the AST.
struct AnchorEntry {
    name: String,
    range: Range,
}

/// Validate unused anchors and unresolved aliases in YAML documents.
///
/// Returns diagnostics for:
/// - Anchors (`&name`) that are never referenced by any alias (marked with `DiagnosticTag::Unnecessary`)
/// - Aliases (`*name`) that reference non-existent anchors (error severity)
///
/// Anchors and aliases are scoped to individual YAML documents. Diagnostic ranges
/// use the anchor-carrying node's `loc` span directly.
#[must_use]
pub fn validate_unused_anchors(docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for doc in docs {
        let mut anchors: Vec<AnchorEntry> = Vec::new();
        let mut alias_names: Vec<(String, Range)> = Vec::new();

        collect_anchors_and_aliases(&doc.root, &mut anchors, &mut alias_names, 0);

        // Build anchor name → range map for lookup (last definition wins on duplicates)
        let anchor_map: HashMap<&str, &AnchorEntry> =
            anchors.iter().map(|e| (e.name.as_str(), e)).collect();

        let mut used: HashSet<&str> = HashSet::new();

        for (alias_name, alias_range) in &alias_names {
            if anchor_map.contains_key(alias_name.as_str()) {
                used.insert(alias_name.as_str());
            } else {
                diagnostics.push(Diagnostic {
                    range: *alias_range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("unresolvedAlias".to_string())),
                    message: format!("Alias '{alias_name}' has no matching anchor"),
                    source: Some("rlsp-yaml".to_string()),
                    ..Diagnostic::default()
                });
            }
        }

        for entry in &anchors {
            if !used.contains(entry.name.as_str()) {
                let truncated_name = if entry.name.len() > 100 {
                    format!("{}...", &entry.name[..100])
                } else {
                    entry.name.clone()
                };
                diagnostics.push(Diagnostic {
                    range: entry.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unusedAnchor".to_string())),
                    message: format!("Anchor '{truncated_name}' is never used"),
                    source: Some("rlsp-yaml".to_string()),
                    tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                    ..Diagnostic::default()
                });
            }
        }
    }

    diagnostics
}

/// Recursively walk the AST collecting anchor definitions and alias references.
fn collect_anchors_and_aliases(
    node: &Node<Span>,
    anchors: &mut Vec<AnchorEntry>,
    aliases: &mut Vec<(String, Range)>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Alias { name, loc, .. } => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "LSP line/col are u32; always fits"
            )]
            let range = Range::new(
                Position::new(
                    loc.start.line.saturating_sub(1) as u32,
                    loc.start.column as u32,
                ),
                Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
            );
            aliases.push((name.clone(), range));
        }
        Node::Scalar { loc, .. } | Node::Mapping { loc, .. } | Node::Sequence { loc, .. } => {
            if let Some(name) = node.anchor() {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "LSP line/col are u32; always fits"
                )]
                let range = Range::new(
                    Position::new(
                        loc.start.line.saturating_sub(1) as u32,
                        loc.start.column as u32,
                    ),
                    Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
                );
                anchors.push(AnchorEntry {
                    name: name.to_owned(),
                    range,
                });
            }
            match node {
                Node::Mapping { entries, .. } => {
                    for (key, value) in entries {
                        collect_anchors_and_aliases(key, anchors, aliases, depth + 1);
                        collect_anchors_and_aliases(value, anchors, aliases, depth + 1);
                    }
                }
                Node::Sequence { items, .. } => {
                    for item in items {
                        collect_anchors_and_aliases(item, anchors, aliases, depth + 1);
                    }
                }
                Node::Scalar { .. } | Node::Alias { .. } => {}
            }
        }
    }
}

/// Validate flow style usage in YAML documents.
///
/// Returns warning diagnostics for:
/// - Flow mappings (`{...}`) with code `flowMap`
/// - Flow sequences (`[...]`) with code `flowSeq`
///
/// Empty collections (`{}`, `[]`) produce no diagnostic. Uses the parser AST
/// so plain scalars containing `{`/`[` (e.g. `${{ env.VAR }}`) are never
/// false-flagged. Multi-line flow collections are detected because the AST
/// spans across lines.
#[must_use]
pub fn validate_flow_style(docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for doc in docs {
        collect_flow_style_diagnostics(&doc.root, &mut diagnostics, 0);
    }
    diagnostics
}

/// Recursively walk a node and emit diagnostics for non-empty flow collections.
fn collect_flow_style_diagnostics(
    node: &Node<Span>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Mapping {
            style: CollectionStyle::Flow,
            entries,
            loc,
            ..
        } if !entries.is_empty() => {
            diagnostics.push(flow_diagnostic(
                "flowMap",
                "Flow mapping style: use block style instead",
                loc,
            ));
            for (key, value) in entries {
                collect_flow_style_diagnostics(key, diagnostics, depth + 1);
                collect_flow_style_diagnostics(value, diagnostics, depth + 1);
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_flow_style_diagnostics(key, diagnostics, depth + 1);
                collect_flow_style_diagnostics(value, diagnostics, depth + 1);
            }
        }
        Node::Sequence {
            style: CollectionStyle::Flow,
            items,
            loc,
            ..
        } if !items.is_empty() => {
            diagnostics.push(flow_diagnostic(
                "flowSeq",
                "Flow sequence style: use block style instead",
                loc,
            ));
            for item in items {
                collect_flow_style_diagnostics(item, diagnostics, depth + 1);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_flow_style_diagnostics(item, diagnostics, depth + 1);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

fn flow_diagnostic(code: &str, message: &str, loc: &Span) -> Diagnostic {
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
    let end_line = loc.end.line.saturating_sub(1) as u32;
    // The AST end span is at the closing `}` or `]` character (zero-width span).
    // Add 1 so the LSP range end is exclusive — past the delimiter — which
    // lets flow_map_to_block/flow_seq_to_block extract the full `{...}` slice.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let end_col = (loc.end.column + 1) as u32;
    Diagnostic {
        range: Range::new(
            Position::new(start_line, start_col),
            Position::new(end_line, end_col),
        ),
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String(code.to_string())),
        message: message.to_string(),
        source: Some("rlsp-yaml".to_string()),
        ..Diagnostic::default()
    }
}

/// Validate custom YAML tags against an allowed set.
///
/// Returns warning diagnostics for any `!tag` found in the YAML documents that is not
/// listed in `allowed_tags`. When `allowed_tags` is empty, validation is skipped and
/// an empty vec is returned — no tags configured means no warnings.
///
/// Diagnostic ranges use the tagged node's full `loc` span. Quoted scalars are never
/// flagged because the parser stores `tag: None` for them.
#[must_use]
pub fn validate_custom_tags<S: std::hash::BuildHasher>(
    docs: &[Document<Span>],
    allowed_tags: &HashSet<String, S>,
) -> Vec<Diagnostic> {
    if allowed_tags.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();

    for doc in docs {
        collect_tag_diagnostics(&doc.root, allowed_tags, &mut diagnostics, 0);
    }

    diagnostics
}

/// Recursively walk a YAML node and emit diagnostics for unknown tags.
fn collect_tag_diagnostics<S: std::hash::BuildHasher>(
    node: &Node<Span>,
    allowed_tags: &HashSet<String, S>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    // Check the tag and loc fields on this node (all non-Alias variants carry them).
    let tag_and_loc = match node {
        Node::Scalar { tag, loc, .. }
        | Node::Mapping { tag, loc, .. }
        | Node::Sequence { tag, loc, .. } => tag.as_deref().map(|t| (t, loc)),
        Node::Alias { .. } => None,
    };
    if let Some((tag_str, loc)) = tag_and_loc {
        // Skip tags injected by the Core schema resolver — these are never user-supplied.
        if tag_str.starts_with("tag:yaml.org,2002:") {
            // Fall through to child recursion only.
        } else if !allowed_tags.contains(tag_str) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "LSP line/col are u32; always fits"
            )]
            let range = Range::new(
                Position::new(
                    loc.start.line.saturating_sub(1) as u32,
                    loc.start.column as u32,
                ),
                Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
            );
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

    // Recurse into children.
    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_tag_diagnostics(key, allowed_tags, diagnostics, depth + 1);
                collect_tag_diagnostics(value, allowed_tags, diagnostics, depth + 1);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_tag_diagnostics(item, allowed_tags, diagnostics, depth + 1);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Validate map key ordering in YAML documents.
///
/// Returns warning diagnostics for map keys that are not in alphabetical order.
/// Uses case-sensitive lexicographic comparison. Diagnostic ranges use the key
/// node's `loc` span directly.
#[must_use]
pub fn validate_key_ordering(docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for doc in docs {
        check_yaml_ordering(&doc.root, &mut diagnostics, 0);
    }

    diagnostics
}

/// Recursively check YAML nodes for key ordering, with depth limit.
fn check_yaml_ordering(node: &Node<Span>, diagnostics: &mut Vec<Diagnostic>, depth: usize) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Mapping { entries, .. } => {
            // Collect (key_string, key_loc) pairs, skipping null keys.
            let keys: Vec<(&str, &Span)> = entries
                .iter()
                .filter_map(|(k, _)| match k {
                    Node::Scalar {
                        tag, value, loc, ..
                    } if tag.as_deref() != Some("tag:yaml.org,2002:null") => {
                        Some((value.as_str(), loc))
                    }
                    Node::Scalar { .. }
                    | Node::Mapping { .. }
                    | Node::Sequence { .. }
                    | Node::Alias { .. } => None,
                })
                .collect();

            // Track the maximum key seen so far to catch all out-of-order keys.
            let mut max_key: &str = keys.first().map_or("", |&(k, _)| k);

            for &(key, loc) in keys.iter().skip(1) {
                if key < max_key {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "LSP line/col are u32; always fits"
                    )]
                    let range = Range::new(
                        Position::new(
                            loc.start.line.saturating_sub(1) as u32,
                            loc.start.column as u32,
                        ),
                        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
                    );
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("mapKeyOrder".to_string())),
                        message: format!("Key '{key}' is out of alphabetical order"),
                        source: Some("rlsp-yaml".to_string()),
                        ..Diagnostic::default()
                    });
                } else if key > max_key {
                    max_key = key;
                }
            }

            // Recursively check nested structures.
            for (_, value) in entries {
                check_yaml_ordering(value, diagnostics, depth + 1);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_yaml_ordering(item, diagnostics, depth + 1);
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
    use crate::test_utils::parse_docs;

    fn parse_anchors(yaml: &str) -> Vec<super::Diagnostic> {
        validate_unused_anchors(&parse_docs(yaml))
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
    #[case::invalid_anchor_chars_terminates_name("data: &anchor!@# value\nref: *anchor!@#\n")]
    fn unused_anchors_returns_empty(#[case] input: &str) {
        let result = parse_anchors(input);

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
        let result = parse_anchors(input);

        assert_eq!(result.len(), expected);
    }

    // ---- Unused Anchors Validator: Unresolved Alias Detection ----

    #[rstest]
    #[case::single_unresolved_alias("production:\n  <<: *undefined\n")]
    #[case::two_unresolved_aliases("a: *missing1\nb: *missing2\n")]
    fn unused_anchors_all_errors(#[case] input: &str) {
        let result = parse_anchors(input);

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
        let result = parse_anchors(input);

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

        let result = parse_anchors(&text);

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
        let result = parse_anchors(&text);

        assert_eq!(result.len(), 1);
        assert!(!result[0].message.is_empty());
    }

    // ---- Unused Anchors Validator: Multi-Document Scoping (standalone) ----

    #[test]
    fn should_report_unused_anchor_scoped_to_document() {
        let text = "doc1: &shared\n  k: v\n---\ndoc2:\n  ref: *shared\n";
        let result = parse_anchors(text);

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
        // &unused anchors a Mapping whose content starts at line 2 (0-based), col 2.
        let text = "name: 中文\ndata: &unused\n  key: val\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 2, "mapping content is on line 2");
        assert_eq!(
            diag.range.start.character, 2,
            "mapping content starts at col 2"
        );
    }

    #[test]
    fn should_not_satisfy_alias_in_doc1_with_anchor_in_doc2() {
        let text = "ref: *later\n---\ndata: &later\n  key: val\n";
        let result = parse_anchors(text);

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
        let result = parse_anchors(text);

        // Only doc2's &unused should be flagged
        assert_eq!(result.len(), 1);
        // &unused anchors a Mapping whose content (  k: v) is on line 5 (0-based), col 2.
        assert_eq!(
            result[0].range.start.line, 5,
            "mapping content is on line 5"
        );
        assert_eq!(
            result[0].range.start.character, 2,
            "mapping content starts at col 2"
        );
        assert!(
            result[0]
                .tags
                .as_ref()
                .is_some_and(|t| t.contains(&DiagnosticTag::UNNECESSARY))
        );
    }

    // ---- Unused Anchors Validator: AC-4 Regression Tests ----

    #[test]
    fn ac4_undefined_alias_produces_error_not_warning() {
        // "ref: *does_not_exist\n" — *does_not_exist starts at col 5, ends at col 20.
        let text = "ref: *does_not_exist\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].severity,
            Some(DiagnosticSeverity::ERROR),
            "undefined alias should be an error"
        );
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("unresolvedAlias".to_string()))
        );
        assert_eq!(result[0].range.start.character, 5, "alias starts at col 5");
        assert_eq!(result[0].range.end.character, 20, "alias ends at col 20");
    }

    #[test]
    fn unused_anchor_on_block_mapping_node_loc_used() {
        // The plan specifies using the anchor-carrying node's loc directly.
        // &defaults anchors a Mapping; the Mapping's loc.start is at the content
        // (line 1, col 2 in LSP), not at the &defaults token on line 0.
        let text = "defaults: &defaults\n  key: val\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 1, "mapping content is on line 1");
        assert_eq!(
            diag.range.start.character, 2,
            "mapping content starts at col 2"
        );
        assert_eq!(diag.range.end.line, 2, "mapping content ends on line 2");
        assert_eq!(diag.range.end.character, 0);
    }

    #[test]
    fn unused_anchor_on_flow_sequence_node_loc_used() {
        // &a anchors a Sequence; the Sequence's loc covers `[1, 2, 3]` starting at col 10.
        let text = "items: &a [1, 2, 3]\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 0);
        assert_eq!(
            diag.range.start.character, 10,
            "sequence starts at col 10 (the `[`)"
        );
        assert_eq!(diag.range.end.character, 18, "sequence ends at col 18");
    }

    #[test]
    fn unused_anchor_on_inline_scalar_node_loc_used() {
        // &myanchor anchors a Scalar; the Scalar's loc covers `value` starting at col 15.
        let text = "key: &myanchor value\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 0);
        assert_eq!(
            diag.range.start.character, 15,
            "scalar `value` starts at col 15"
        );
        assert_eq!(
            diag.range.end.character, 20,
            "scalar `value` ends at col 20"
        );
    }

    #[test]
    fn unused_anchor_trailing_comment_node_loc_excludes_comment() {
        // &anchor anchors a Scalar; the Scalar's loc covers `value` (col 13–18),
        // not the trailing comment. Node loc is tighter than the full line.
        let text = "key: &anchor value # this is a comment\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(
            diag.range.start.character, 13,
            "scalar `value` starts at col 13"
        );
        assert_eq!(
            diag.range.end.character, 18,
            "scalar `value` ends at col 18, before comment"
        );
    }

    #[test]
    fn unresolved_alias_range_points_to_asterisk() {
        // Node::Alias { loc } covers *missing directly — unchanged by this refactor.
        let text = "ref: *missing\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.character, 5, "* sigil at col 5");
        assert_eq!(diag.range.end.character, 13, "*missing ends at col 13");
    }

    #[test]
    fn unused_anchor_second_document_correct_line() {
        // &unused anchors a Mapping in doc2; the Mapping's content (  k: v) is
        // on line 5 (0-based), col 2 — one line below `b: &unused` on line 4.
        let text = "a: &used\n  k: v\nref: *used\n---\nb: &unused\n  k: v\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 5, "mapping content is on line 5");
        assert_eq!(
            diag.range.start.character, 2,
            "mapping content starts at col 2"
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
        let docs = parse_docs(input);
        let result = validate_flow_style(&docs);

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
        let docs = parse_docs(input);
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), expected);
    }

    #[rstest]
    #[case::flow_mapping("config: {key: value}\n")]
    #[case::flow_sequence("items: [a, b]\n")]
    fn flow_style_range_start_line_zero(#[case] input: &str) {
        let docs = parse_docs(input);
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 0);
    }

    // ---- Flow Style Validator: standalone ----

    #[test]
    fn should_detect_flow_mapping() {
        let docs = parse_docs("config: {key: value}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn should_detect_flow_sequence() {
        let docs = parse_docs("items: [one, two, three]\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowSeq")
        );
    }

    #[test]
    fn should_detect_both_flow_mapping_and_sequence() {
        let docs = parse_docs("config: {key: value}\nitems: [a, b]\n");
        let result = validate_flow_style(&docs);

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
        let docs = parse_docs("data: {a: {}}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn should_warn_only_on_non_empty_when_mixed_with_empty() {
        let docs = parse_docs("a: {}\nb: {x: 1}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1);
    }

    // ---- Flow Style Validator: API contract — diagnostic field identity ----

    #[test]
    fn flow_map_diagnostic_message_text() {
        let docs = parse_docs("config: {key: value}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(
            result[0].message,
            "Flow mapping style: use block style instead"
        );
    }

    #[test]
    fn flow_seq_diagnostic_message_text() {
        let docs = parse_docs("items: [a, b]\n");
        let result = validate_flow_style(&docs);

        assert_eq!(
            result[0].message,
            "Flow sequence style: use block style instead"
        );
    }

    #[test]
    fn flow_map_diagnostic_source() {
        let docs = parse_docs("config: {key: value}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    #[test]
    fn flow_seq_diagnostic_source() {
        let docs = parse_docs("items: [a, b]\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    // ---- Flow Style Validator: GHA-style plain scalar expressions ----

    #[test]
    fn gha_expression_in_plain_scalar_no_diagnostic() {
        // `${{ … }}` is a plain scalar in block context — AST does not see a flow mapping.
        let docs = parse_docs("token: ${{ secrets.GITHUB_TOKEN }}\n");
        let result = validate_flow_style(&docs);

        assert!(result.is_empty());
    }

    #[test]
    fn gha_expression_double_brace_no_diagnostic() {
        let docs = parse_docs("run: echo ${{ env.MY_VAR }}\n");
        let result = validate_flow_style(&docs);

        assert!(result.is_empty());
    }

    #[test]
    fn gha_expression_nested_no_diagnostic() {
        let docs = parse_docs("env:\n  TOKEN: ${{ secrets.TOKEN }}\n");
        let result = validate_flow_style(&docs);

        assert!(result.is_empty());
    }

    #[test]
    fn gha_expression_alongside_real_flow_map() {
        // GHA expression line: zero diagnostics; real flow map line: one diagnostic.
        let docs = parse_docs("token: ${{ secrets.TOKEN }}\nconfig: {key: value}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    // ---- Flow Style Validator: multi-line flow collections ----

    #[test]
    fn multiline_flow_map_detected() {
        // Current text scanner misses multi-line flow maps; AST walk finds them.
        // Closing `}` must be indented >= the key column per YAML 1.2 flow rules.
        let docs = parse_docs("foo: {\n       a: 1,\n     }\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn multiline_flow_seq_detected() {
        let docs = parse_docs("items: [\n         a,\n         b,\n       ]\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowSeq")
        );
    }

    #[test]
    fn multiline_flow_map_range_starts_on_opening_line() {
        let docs = parse_docs("foo: {\n       a: 1,\n     }\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result[0].range.start.line, 0);
    }

    // ---- Flow Style Validator: no double-reporting ----

    #[test]
    fn nested_nonempty_flow_maps_no_double_report() {
        // outer {outer: {inner: 1}} → 2 diagnostics (one each), not more.
        let docs = parse_docs("data: {outer: {inner: 1}}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 2);
        assert!(
            result.iter().all(
                |d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
            )
        );
    }

    #[test]
    fn deeply_nested_flow_seq_count() {
        // [[1, 2], [3, 4]] → 3 diagnostics: outer seq + two inner seqs.
        let docs = parse_docs("data: [[1, 2], [3, 4]]\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 3);
    }

    // ---- Flow Style Validator: empty-collection edge cases ----

    #[test]
    fn empty_nested_seq_inside_nonempty_map_no_extra_diagnostic() {
        // {a: []} → 1 diagnostic for the outer map; inner empty seq: none.
        let docs = parse_docs("data: {a: []}\n");
        let result = validate_flow_style(&docs);

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    // ---- Flow Style Validator: GHA expression regression (Task 3) ----

    #[test]
    fn flow_style_ignores_github_actions_expressions() {
        // Regression guard: all four expression forms must produce zero diagnostics;
        // the real flow mapping in the same document must still be detected.
        let yaml = "\
jobs:
  build:
    env:
      TOKEN: ${{ secrets.GITHUB_TOKEN }}
      MATRIX_JSON: ${{ fromJSON(needs.x.outputs.y) }}
      COMBINED: ${{ x }} and ${{ y }}
    strategy:
      matrix: { target: linux, os: ubuntu }
";
        let docs = parse_docs(yaml);
        let result = validate_flow_style(&docs);

        // Only the real flow mapping on the `matrix:` line should be reported.
        assert_eq!(
            result.len(),
            1,
            "expected exactly 1 diagnostic (matrix line), got: {result:?}"
        );
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap"),
            "expected flowMap diagnostic on matrix line, got: {:?}",
            result[0].code,
        );
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
        let result = validate_key_ordering(&docs);

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
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), expected);
    }

    // ---- Map Key Order Validator: standalone ----

    #[test]
    fn should_detect_out_of_order_keys() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

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
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "apple is on line 1");
        assert_eq!(result[0].range.start.character, 0, "apple starts at col 0");
        assert_eq!(result[0].range.end.character, 5, "apple is 5 chars long");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group 7: check_yaml_ordering — tag-based null key filtering
    // ══════════════════════════════════════════════════════════════════════════

    // T7.1 — null key is excluded from ordering check with tag-based null detection
    #[test]
    fn tag_driven_null_key_excluded_from_ordering_check() {
        // ~ is null; zebra and alpha are out of order — only 1 ordering diagnostic expected
        let text = "~: value\nzebra: 1\nalpha: 2\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);
        assert_eq!(
            result.len(),
            1,
            "null key must be excluded; only alpha is out of order"
        );
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "mapKeyOrder")
        );
    }

    // T7.2 — non-null plain scalar key is included in ordering check (baseline)
    #[test]
    fn tag_driven_non_null_key_included_in_ordering_check() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);
        assert_eq!(result.len(), 1, "apple is out of order and must be flagged");
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
        let result = validate_custom_tags(&docs, &allowed);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::unknown_tag("value: !include foo.yaml\n", &["!other"] as &[&str])]
    #[case::multiple_tags_only_unknown_flagged("a: !include foo.yaml\nb: !ref bar.yaml\n", &["!include"])]
    #[case::nested_tagged_value("outer:\n  inner: !include nested.yaml\n", &["!other"])]
    fn custom_tags_single_diagnostic(#[case] input: &str, #[case] allowed_tags: &[&str]) {
        let docs = parse_docs(input);
        let allowed: HashSet<String> = allowed_tags.iter().map(|s| (*s).to_string()).collect();
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 1);
    }

    // ---- Custom Tags Validator: standalone ----

    #[test]
    fn unknown_tag_produces_warning_with_unknown_tag_code() {
        let text = "value: !include foo.yaml\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(&docs, &allowed);

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
        let result = validate_custom_tags(&docs, &neither);
        assert_eq!(result.len(), 2);

        // Both allowed
        let both: HashSet<String> = HashSet::from(["!include".to_string(), "!ref".to_string()]);
        let result = validate_custom_tags(&docs, &both);
        assert!(result.is_empty());
    }

    // ---- Custom Tags Validator: AST-range regression tests ----

    #[test]
    fn tag_on_mapping_value_range_equals_scalar_loc() {
        let text = "key: !custom value\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 0, "scalar on line 0");
        // The parser's loc.start.column for a tagged scalar points to where the
        // value content begins (after the tag token and separating space).
        // "key: !custom value" — "value" starts at col 13.
        assert_eq!(
            result[0].range.start.character, 13,
            "scalar value content starts at col 13 (after '!custom ')"
        );
    }

    #[test]
    fn tag_on_sequence_item_range_equals_scalar_loc() {
        let text = "items:\n  - !custom value\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "scalar on line 1");
        // "  - !custom value" — "value" starts at col 12.
        assert_eq!(
            result[0].range.start.character, 12,
            "scalar value content starts at col 12 (after '  - !custom ')"
        );
    }

    #[test]
    fn repeated_tag_strings_each_emit_own_diagnostic() {
        let text = "a: !include file1.yaml\nb: !include file2.yaml\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].range.start.line, 0, "first node on line 0");
        assert_eq!(result[1].range.start.line, 1, "second node on line 1");
    }

    #[test]
    fn quoted_scalar_text_containing_tag_syntax_not_flagged() {
        let text = "note: \"use !include for files\"\n";
        let docs = parse_docs(text);
        let allowed: HashSet<String> = HashSet::from(["!other".to_string()]);
        let result = validate_custom_tags(&docs, &allowed);

        assert!(
            result.is_empty(),
            "quoted scalar has tag: None — no diagnostic expected"
        );
    }

    // ---- Key Ordering Validator: AST-range regression tests ----

    #[test]
    fn out_of_order_key_range_covers_key_span() {
        let text = "banana: 2\napple: 1\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "apple on line 1");
        assert_eq!(result[0].range.start.character, 0, "apple at col 0");
        assert_eq!(result[0].range.end.character, 5, "apple is 5 chars");
    }

    #[test]
    fn out_of_order_key_in_indented_block_has_correct_column() {
        let text = "outer:\n  zebra: 1\n  alpha: 2\n";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 2, "alpha on line 2");
        assert_eq!(result[0].range.start.character, 2, "alpha indented by 2");
        assert_eq!(
            result[0].range.end.character, 7,
            "col 2 + len('alpha') == 7"
        );
    }

    #[test]
    fn out_of_order_keys_in_flow_mapping() {
        let text = "{zebra: 1, alpha: 2}";
        let docs = rlsp_yaml_parser::load(text).unwrap();
        let result = validate_key_ordering(&docs);

        assert_eq!(result.len(), 1);
        assert!(
            result[0].range.start.character > 0,
            "alpha's column in flow mapping is non-zero"
        );
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
            validate_flow_style(
                &rlsp_yaml_parser::load(text_with_v1_1_keywords).unwrap_or_default()
            )
            .len(),
            validate_flow_style(&rlsp_yaml_parser::load(text_plain).unwrap_or_default()).len(),
            "flow-style diagnostics must not differ based on v1.1 keyword presence"
        );
    }
}
