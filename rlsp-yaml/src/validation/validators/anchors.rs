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
/// Anchors and aliases are scoped to individual YAML documents. The range of an
/// unused-anchor diagnostic is the `&name` token (`Node::anchor_loc`), not the
/// node it anchors; the range of an unresolved-alias diagnostic is the alias
/// node's span (`*name`).
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

/// The deepest node level the unused-anchor validator and its quick fix look at.
pub(crate) const MAX_DEPTH: usize = 100;

/// Recursively walk the AST collecting anchor definitions and alias references.
fn collect_anchors_and_aliases(
    node: &Node<Span>,
    anchors: &mut Vec<AnchorEntry>,
    aliases: &mut Vec<(String, Range)>,
    depth: usize,
    idx: &LineIndex,
) {
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Alias { name, loc, .. } => {
            let range = span_to_lsp(*loc, idx);
            aliases.push((name.clone(), range));
        }
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } => {
            if let (Some(name), Some(loc)) = (node.anchor(), node.anchor_loc()) {
                anchors.push(AnchorEntry {
                    name: name.to_owned(),
                    range: span_to_lsp(loc, idx),
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
    use tower_lsp::lsp_types::Position;

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
        // The token follows multibyte text on its own line; columns count
        // characters, not bytes.
        let text = "name: 中文\n中文: &unused\n  key: val\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 1, "the token is on line 1");
        assert_eq!(diag.range.start.character, 4);
        assert_eq!(diag.range.end.character, 11);
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
        // `b: &unused` is on line 4 (0-based), the token starts at col 3.
        assert_eq!(result[0].range.start.line, 4, "the token is on line 4");
        assert_eq!(result[0].range.start.character, 3);
        assert_eq!(result[0].range.end.character, 10);
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

    /// The characters of `range` (a range within one line) in `yaml`; lines end
    /// at `\n`, `\r\n` or a lone `\r`.
    fn text_of(yaml: &str, range: Range) -> String {
        assert_eq!(range.start.line, range.end.line, "a one-line range");
        yaml.replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .nth(range.start.line as usize)
            .expect("the line exists")
            .chars()
            .skip(range.start.character as usize)
            .take((range.end.character - range.start.character) as usize)
            .collect()
    }

    // The range of an unused-anchor diagnostic is the `&name` token, wherever
    // the anchored node starts or ends.
    #[rstest]
    #[case::inline_scalar("key: &myanchor value\n", 0, 5, "&myanchor")]
    #[case::scalar_before_a_comment("key: &anchor value # this is a comment\n", 0, 5, "&anchor")]
    #[case::block_mapping("defaults: &defaults\n  key: val\n", 0, 10, "&defaults")]
    #[case::block_sequence("list: &s\n  - 1\n", 0, 6, "&s")]
    #[case::flow_sequence("items: &a [1, 2, 3]\n", 0, 7, "&a")]
    #[case::literal_block_scalar("a: &x |\n  line one\nb: 1\n", 0, 3, "&x")]
    #[case::block_scalar_header_comment("a: &x | # c\n  t\n", 0, 3, "&x")]
    #[case::empty_value("a: &y\nb: 1\n", 0, 3, "&y")]
    #[case::empty_sequence_item("- &y !t\n- x\n", 0, 2, "&y")]
    #[case::empty_explicit_value("? k\n: &v\n", 1, 2, "&v")]
    #[case::empty_value_at_the_end("a: &x", 0, 3, "&x")]
    #[case::tag_after("&a !t v\n", 0, 0, "&a")]
    #[case::tag_before("!t &a v\n", 0, 3, "&a")]
    #[case::tag_before_a_block_mapping("k: !t &a\n  x: 1\n", 0, 6, "&a")]
    #[case::mapping_key("&a k: v\n", 0, 0, "&a")]
    #[case::key_in_sequence_item("- &a x: 1\n", 0, 2, "&a")]
    #[case::mapping_in_sequence_item("- &a\n  x: 1\n", 0, 2, "&a")]
    #[case::explicit_key("? &k\n: v\n", 0, 2, "&k")]
    #[case::after_document_start("--- &x\nk: v\n", 0, 4, "&x")]
    #[case::flow_mapping_value("{k: &a}", 0, 4, "&a")]
    #[case::flow_sequence_item("[&a x]", 0, 1, "&a")]
    #[case::multibyte_text_before("名前: &u v\n", 0, 4, "&u")]
    #[case::multibyte_name("k: &ü v\n", 0, 3, "&ü")]
    #[case::second_line("a: 1\nb: &y 2\n", 1, 3, "&y")]
    #[case::crlf("a: 1\r\nb: &y 2\r\n", 1, 3, "&y")]
    #[case::lone_cr("a: 1\rb: &y 2\r", 1, 3, "&y")]
    #[case::block_mapping_crlf("k: &a\r\n  x: 1\r\n", 0, 3, "&a")]
    fn unused_anchor_range_is_the_anchor_token(
        #[case] text: &str,
        #[case] line: u32,
        #[case] start: u32,
        #[case] token: &str,
    ) {
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1, "{text:?}");
        let diag = &result[0];
        let width = u32::try_from(token.chars().count()).unwrap();
        assert_eq!(
            diag.range,
            Range::new(
                Position::new(line, start),
                Position::new(line, start + width)
            ),
            "{text:?}"
        );
        assert_eq!(text_of(text, diag.range), token);
        assert_eq!(
            diag.code,
            Some(NumberOrString::String("unusedAnchor".to_string()))
        );
        assert_eq!(diag.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diag.tags, Some(vec![DiagnosticTag::UNNECESSARY]));
        assert_eq!(
            diag.message,
            format!("Anchor '{}' is never used", &token[1..])
        );
    }

    // Every unused anchor of a document gets its own token range, in the order
    // of the document; documents are told apart by their lines.
    #[rstest]
    #[case::key_and_value("&k key: &v val\n", &[(0, 0, "&k"), (0, 8, "&v")])]
    #[case::same_name_twice("a: &x 1\nb: &x 2\n", &[(0, 3, "&x"), (1, 3, "&x")])]
    #[case::one_of_two_used("a: &used 1\nb: &unused 2\nc: *used\n", &[(1, 3, "&unused")])]
    #[case::every_document("a: &x 1\n---\nb: &y 2\n---\nc: &z 3\n", &[(0, 3, "&x"), (2, 3, "&y"), (4, 3, "&z")])]
    #[case::after_a_directive(
        "%YAML 1.2\n---\na: &x 1\n...\n%YAML 1.2\n---\nb: &y 2\n",
        &[(2, 3, "&x"), (6, 3, "&y")]
    )]
    #[case::multibyte_first_document("名前: 1\n---\n名前: &u v\n", &[(2, 4, "&u")])]
    fn unused_anchor_ranges_of_several_anchors(
        #[case] text: &str,
        #[case] expected: &[(u32, u32, &str)],
    ) {
        let result = parse_anchors(text);

        let found: Vec<(u32, u32, String)> = result
            .iter()
            .map(|d| {
                (
                    d.range.start.line,
                    d.range.start.character,
                    text_of(text, d.range),
                )
            })
            .collect();
        let expected: Vec<(u32, u32, String)> = expected
            .iter()
            .map(|(line, col, token)| (*line, *col, (*token).to_owned()))
            .collect();
        assert_eq!(found, expected, "{text:?}");
    }

    // The range covers the whole name even where the message shortens it.
    #[test]
    fn unused_anchor_range_covers_a_long_name() {
        let text = format!("data: &{}\n  k: v\n", "a".repeat(200));
        let result = parse_anchors(&text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.character, 6);
        assert_eq!(result[0].range.end.character, 6 + 201);
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
        // The `&unused` token of doc2 is on line 4 (0-based), not on the line of
        // the mapping it anchors.
        let text = "a: &used\n  k: v\nref: *used\n---\nb: &unused\n  k: v\n";
        let result = parse_anchors(text);

        assert_eq!(result.len(), 1);
        let diag = &result[0];
        assert_eq!(diag.range.start.line, 4, "the token is on line 4");
        assert_eq!(diag.range.start.character, 3);
        assert_eq!(diag.range.end.character, 10);
    }
}
