// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::{Document, Event, Node, Span};
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

/// Compute folding ranges from the YAML AST.
///
/// `text` is retained solely for `Event::Comment` extraction via
/// `rlsp_yaml_parser::parse_events` — it is not used for any structural decision.
#[must_use]
pub fn folding_ranges(docs: &[Document<Span>], text: &str) -> Vec<FoldingRange> {
    if docs.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let is_multidoc = docs.len() > 1;

    for doc in docs {
        if is_multidoc {
            // In a multi-document stream, emit one Region fold for each document's
            // root span so the fold boundary matches the `---`-delimited section.
            // Only emit if the root spans more than one source line.
            let root_loc = match &doc.root {
                Node::Mapping { loc, .. }
                | Node::Sequence { loc, .. }
                | Node::Scalar { loc, .. }
                | Node::Alias { loc, .. } => loc,
            };
            if root_loc.end.line > root_loc.start.line {
                let (start, end) = node_span_0based(&doc.root);
                if end > start {
                    push_fold(&mut ranges, start, end, Some(FoldingRangeKind::Region));
                }
            }
            // Walk children only — the root is already covered by the Region fold above.
            collect_children_folds(&doc.root, &mut ranges);
        } else {
            collect_node_folds(&doc.root, &mut ranges);
        }
    }

    collect_comment_folds(text, &mut ranges);

    ranges
}

/// Extract the 0-based `(start_line, end_line)` from a node's `loc` span.
///
/// AST positions are 1-based; `saturating_sub(1)` converts to 0-based LSP lines.
const fn node_span_0based(node: &Node<Span>) -> (usize, usize) {
    let loc = match node {
        Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Scalar { loc, .. }
        | Node::Alias { loc, .. } => loc,
    };
    (
        loc.start.line.saturating_sub(1),
        loc.end.line.saturating_sub(1),
    )
}

/// Walk a node's immediate children and fold each `Mapping` / `Sequence` child.
fn collect_children_folds(node: &Node<Span>, ranges: &mut Vec<FoldingRange>) {
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                collect_node_folds(k, ranges);
                collect_node_folds(v, ranges);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node_folds(item, ranges);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Recursively walk a node and emit a fold for every `Mapping` and `Sequence`.
fn collect_node_folds(node: &Node<Span>, ranges: &mut Vec<FoldingRange>) {
    match node {
        Node::Mapping { entries, loc, .. } => {
            if loc.end.line > loc.start.line {
                let (start, end) = (
                    loc.start.line.saturating_sub(1),
                    loc.end.line.saturating_sub(1),
                );
                if end > start {
                    push_fold(ranges, start, end, None);
                }
            }
            for (k, v) in entries {
                collect_node_folds(k, ranges);
                collect_node_folds(v, ranges);
            }
        }
        Node::Sequence { items, loc, .. } => {
            if loc.end.line > loc.start.line {
                let (start, end) = (
                    loc.start.line.saturating_sub(1),
                    loc.end.line.saturating_sub(1),
                );
                if end > start {
                    push_fold(ranges, start, end, None);
                }
            }
            for item in items {
                collect_node_folds(item, ranges);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Group contiguous `Event::Comment` spans (≥2 consecutive lines) into folds.
fn collect_comment_folds(yaml: &str, ranges: &mut Vec<FoldingRange>) {
    let comment_lines: Vec<usize> = rlsp_yaml_parser::parse_events(yaml)
        .filter_map(|result| {
            if let Ok((Event::Comment { .. }, span)) = result {
                Some(span.start.line) // 1-based
            } else {
                None
            }
        })
        .collect();

    let Some(&first) = comment_lines.first() else {
        return;
    };

    let mut group_start = first;
    let mut group_end = first;

    for &line in comment_lines.iter().skip(1) {
        if line != group_end + 1 {
            if group_end > group_start {
                push_fold(
                    ranges,
                    group_start.saturating_sub(1),
                    group_end.saturating_sub(1),
                    Some(FoldingRangeKind::Comment),
                );
            }
            group_start = line;
        }
        group_end = line;
    }
    if group_end > group_start {
        push_fold(
            ranges,
            group_start.saturating_sub(1),
            group_end.saturating_sub(1),
            Some(FoldingRangeKind::Comment),
        );
    }
}

/// Push a folding range, performing the `usize` to `u32` conversion.
fn push_fold(
    ranges: &mut Vec<FoldingRange>,
    start: usize,
    end: usize,
    kind: Option<FoldingRangeKind>,
) {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    ranges.push(FoldingRange {
        start_line: start as u32,
        start_character: None,
        end_line: end as u32,
        end_character: None,
        kind,
        collapsed_text: None,
    });
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "test code")]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::FoldingRangeKind;

    use super::*;

    fn load_docs(yaml: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(yaml).expect("test input must be valid YAML")
    }

    fn ranges_as_tuples(ranges: &[FoldingRange]) -> Vec<(u32, u32)> {
        ranges.iter().map(|r| (r.start_line, r.end_line)).collect()
    }

    // ---- Mappings / Sequences ----

    // Group: folding_ranges_contains_range — single tuples.contains assertion
    #[rstest]
    // ---- Mappings ----
    #[case::mapping_with_nested_content("server:\n  host: localhost\n  port: 8080\n", (0, 3))]
    // ---- Sequences ----
    #[case::sequence("items:\n  - one\n  - two\n  - three\n", (0, 4))]
    #[case::sequence_of_mappings("users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n", (0, 5))]
    // ---- Comments mixed in ----
    #[case::comment_within_server_fold("server:\n  # This is a comment\n  host: localhost\n  port: 8080\n", (0, 4))]
    // ---- Edge Cases ----
    #[case::blank_lines_within_fold("server:\n  host: localhost\n\n  port: 8080\n", (0, 4))]
    #[case::mixed_content_types("config:\n  name: app\n  ports:\n    - 80\n    - 443\n  description: |\n    A multi-line\n    description\n", (0, 8))]
    fn folding_ranges_contains_range(#[case] text: &str, #[case] expected: (u32, u32)) {
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&expected),
            "expected tuple {expected:?} in folding ranges, got: {tuples:?}"
        );
    }

    // Group: folding_ranges_returns_empty — assert result.is_empty()
    #[rstest]
    #[case::empty_document("")]
    #[case::single_line_document("key: value")]
    fn folding_ranges_returns_empty(#[case] text: &str) {
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        assert!(result.is_empty(), "expected empty result, got: {result:?}");
    }

    // ---- Multi-Document Sections ----

    #[test]
    fn should_fold_document_sections() {
        let text = "key1: val1\nkey2: val2\n---\nkey3: val3\nkey4: val4\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        assert!(
            result.len() >= 2,
            "should have at least 2 folding ranges for 2 document sections, got: {}",
            result.len()
        );
    }

    #[test]
    fn should_fold_document_sections_with_nested_content() {
        let text = "doc1:\n  key: val\n---\ndoc2:\n  key: val\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 2)),
            "should fold doc1 section (lines 0-2), got: {tuples:?}"
        );
        assert!(
            tuples.contains(&(3, 5)),
            "should fold doc2 section (lines 3-5), got: {tuples:?}"
        );
    }

    // ---- Comments ----

    #[test]
    fn should_fold_consecutive_comment_block() {
        let text = "# Header comment\n# continues here\n# and here\nkey: value\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        // Comment block folding is optional. If present, it should use Comment kind.
        let comment_folds: Vec<&FoldingRange> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        let region_folds: Vec<&FoldingRange> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Region) || r.kind.is_none())
            .collect();

        // The comment block should not produce a Region or mapping fold starting at line 0
        for fold in &region_folds {
            assert!(
                fold.start_line > 2,
                "comment block should not produce a structural fold starting at line {}, got: {fold:?}",
                fold.start_line
            );
        }

        // If comment folds are present, verify them
        if !comment_folds.is_empty() {
            let tuples: Vec<(u32, u32)> = comment_folds
                .iter()
                .map(|r| (r.start_line, r.end_line))
                .collect();
            assert!(
                tuples.contains(&(0, 2)),
                "comment fold should span lines 0-2, got: {tuples:?}"
            );
        }
    }

    // ---- Edge Cases ----

    #[test]
    fn should_return_empty_for_comment_only_document() {
        let text = "# just a comment\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        // Either empty or a comment fold -- both are acceptable
        for fold in &result {
            assert!(
                fold.kind == Some(FoldingRangeKind::Comment) || fold.kind.is_none(),
                "comment-only document should not produce Region folds"
            );
        }
    }

    // multi-document: three sections
    #[test]
    fn should_fold_three_document_sections() {
        let text = "a: 1\nb: 2\n---\nc: 3\nd: 4\n---\ne: 5\nf: 6\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        assert!(
            result.len() >= 3,
            "three document sections should produce at least 3 folding ranges, got: {result:?}"
        );
    }

    // multi-document: content only after last separator (last section fold)
    #[test]
    fn should_fold_last_section_after_final_separator() {
        let text = "---\nkey1: val1\nkey2: val2\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        // Single document with explicit start — mapping fold covers lines 1-2 (0-based)
        assert!(
            !result.is_empty(),
            "content after separator should produce a fold, got: {result:?}"
        );
    }

    // comment block at end of file (no trailing non-comment line)
    #[test]
    fn should_fold_comment_block_at_end_of_file() {
        let text = "key: value\n# comment line 1\n# comment line 2\n# comment line 3\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        let comment_folds: Vec<_> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        assert!(
            !comment_folds.is_empty(),
            "comment block at end of file should produce a Comment fold, got: {result:?}"
        );
        let tuples: Vec<(u32, u32)> = comment_folds
            .iter()
            .map(|r| (r.start_line, r.end_line))
            .collect();
        assert!(
            tuples.contains(&(1, 3)),
            "comment fold should span lines 1-3, got: {tuples:?}"
        );
    }

    // comment block of exactly 1 line (should NOT fold)
    #[test]
    fn should_not_fold_single_comment_line() {
        let text = "# only one comment\nkey: value\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);

        let comment_folds: Vec<_> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        assert!(
            comment_folds.is_empty(),
            "single comment line should not fold, got: {comment_folds:?}"
        );
    }

    // ---- Regression cases (mandatory rstest) ----

    // Regression (a) + (b): mapping at top level produces a fold matching AST loc;
    // nested mapping produces nested folds.
    #[rstest]
    #[case::top_level_mapping_matches_ast_loc(
        "server:\n  host: localhost\n  port: 8080\n",
        (0, 3),
        (1, 3)
    )]
    fn nested_mapping_produces_nested_folds(
        #[case] text: &str,
        #[case] outer: (u32, u32),
        #[case] inner: (u32, u32),
    ) {
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&outer),
            "expected outer fold {outer:?} in {tuples:?}"
        );
        assert!(
            tuples.contains(&inner),
            "expected inner fold {inner:?} in {tuples:?}"
        );
    }

    // Regression (c): multi-document YAML produces one fold per document.
    #[rstest]
    #[case::two_doc_stream(
        "key1: val1\nkey2: val2\n---\nkey3: val3\nkey4: val4\n",
        (0, 2),
        (3, 5)
    )]
    fn multi_doc_produces_fold_per_document(
        #[case] text: &str,
        #[case] doc0_fold: (u32, u32),
        #[case] doc1_fold: (u32, u32),
    ) {
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&doc0_fold),
            "expected doc0 fold {doc0_fold:?} in {tuples:?}"
        );
        assert!(
            tuples.contains(&doc1_fold),
            "expected doc1 fold {doc1_fold:?} in {tuples:?}"
        );
    }

    // Regression (d): contiguous block of ≥2 comments produces exactly one comment fold.
    #[rstest]
    #[case::two_consecutive_comments(
        "# first comment\n# second comment\nkey: value\n",
        (0, 1)
    )]
    #[case::three_consecutive_comments(
        "# line one\n# line two\n# line three\nkey: value\n",
        (0, 2)
    )]
    fn contiguous_comment_block_produces_one_fold(
        #[case] text: &str,
        #[case] expected: (u32, u32),
    ) {
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        let comment_folds: Vec<_> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        assert_eq!(
            comment_folds.len(),
            1,
            "expected exactly 1 comment fold, got: {comment_folds:?}"
        );
        let fold = comment_folds.first().expect("already asserted len == 1");
        assert_eq!(
            (fold.start_line, fold.end_line),
            expected,
            "comment fold span mismatch"
        );
    }

    // ---- Additional new tests ----

    // sequence_fold_basic: bare root sequence produces a fold.
    #[test]
    fn sequence_fold_basic() {
        let text = "- one\n- two\n- three\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        let tuples = ranges_as_tuples(&result);
        assert!(
            tuples.contains(&(0, 3)),
            "expected root sequence fold (0, 3), got: {tuples:?}"
        );
    }

    // flow_mapping_single_line_no_fold: single-line flow mapping produces no fold.
    #[test]
    fn flow_mapping_single_line_no_fold() {
        let text = "{a: 1, b: 2}\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        assert!(
            result.is_empty(),
            "single-line flow mapping should produce no fold, got: {result:?}"
        );
    }
}
