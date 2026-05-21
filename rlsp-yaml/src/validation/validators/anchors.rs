// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Range};

use crate::lsp_util::span_to_lsp;

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
        let idx = doc.line_index();
        let mut anchors: Vec<AnchorEntry> = Vec::new();
        let mut alias_names: Vec<(String, Range)> = Vec::new();

        collect_anchors_and_aliases(&doc.root, &mut anchors, &mut alias_names, 0, idx);

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
    idx: &LineIndex,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Alias { name, loc, .. } => {
            let range = span_to_lsp(*loc, idx);
            aliases.push((name.clone(), range));
        }
        Node::Scalar { loc, .. } | Node::Mapping { loc, .. } | Node::Sequence { loc, .. } => {
            if let Some(name) = node.anchor() {
                let range = span_to_lsp(*loc, idx);
                anchors.push(AnchorEntry {
                    name: name.to_owned(),
                    range,
                });
            }
            match node {
                Node::Mapping { entries, .. } => {
                    for (key, value) in entries {
                        collect_anchors_and_aliases(key, anchors, aliases, depth + 1, idx);
                        collect_anchors_and_aliases(value, anchors, aliases, depth + 1, idx);
                    }
                }
                Node::Sequence { items, .. } => {
                    for item in items {
                        collect_anchors_and_aliases(item, anchors, aliases, depth + 1, idx);
                    }
                }
                Node::Scalar { .. } | Node::Alias { .. } => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use rstest::rstest;

    use super::*;
    use crate::test_utils::parse_docs;

    fn parse_anchors(yaml: &str) -> Vec<Diagnostic> {
        validate_unused_anchors(&parse_docs(yaml))
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
}
