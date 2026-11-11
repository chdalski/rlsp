// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::{Document, Event, LineIndex, Node, Span};
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
    let Some(first_doc) = docs.first() else {
        return Vec::new();
    };
    let idx = first_doc.line_index();

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
            let start_line = idx.line_column(root_loc.start).0;
            let end_line = idx.line_column(root_loc.end).0;
            if end_line > start_line {
                let (start, end) = node_span_0based(&doc.root, idx);
                if end > start {
                    push_fold(&mut ranges, start, end, Some(FoldingRangeKind::Region));
                }
            }
            // Walk children only — the root is already covered by the Region fold above.
            collect_children_folds(&doc.root, &mut ranges, idx);
        } else {
            collect_node_folds(&doc.root, &mut ranges, idx);
        }
    }

    collect_comment_folds(text, &mut ranges);

    ranges
}

/// Extract the 0-based `(start_line, end_line)` from a node's `loc` span.
///
/// AST positions are 1-based; `saturating_sub(1)` converts to 0-based LSP lines.
fn node_span_0based(node: &Node<Span>, idx: &LineIndex) -> (u32, u32) {
    let loc = match node {
        Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Scalar { loc, .. }
        | Node::Alias { loc, .. } => loc,
    };
    (
        idx.line_column(loc.start).0.saturating_sub(1),
        idx.line_column(loc.end).0.saturating_sub(1),
    )
}

/// Walk a node's immediate children and fold each `Mapping` / `Sequence` child.
fn collect_children_folds(node: &Node<Span>, ranges: &mut Vec<FoldingRange>, idx: &LineIndex) {
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                collect_node_folds(k, ranges, idx);
                collect_node_folds(v, ranges, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node_folds(item, ranges, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Recursively walk a node and emit a fold for every `Mapping` and `Sequence`.
fn collect_node_folds(node: &Node<Span>, ranges: &mut Vec<FoldingRange>, idx: &LineIndex) {
    match node {
        Node::Mapping { entries, loc, .. } => {
            let start_line = idx.line_column(loc.start).0;
            let end_line = idx.line_column(loc.end).0;
            if end_line > start_line {
                let start = start_line.saturating_sub(1);
                let end = end_line.saturating_sub(1);
                if end > start {
                    push_fold(ranges, start, end, None);
                }
            }
            for (k, v) in entries {
                collect_node_folds(k, ranges, idx);
                collect_node_folds(v, ranges, idx);
            }
        }
        Node::Sequence { items, loc, .. } => {
            let start_line = idx.line_column(loc.start).0;
            let end_line = idx.line_column(loc.end).0;
            if end_line > start_line {
                let start = start_line.saturating_sub(1);
                let end = end_line.saturating_sub(1);
                if end > start {
                    push_fold(ranges, start, end, None);
                }
            }
            for item in items {
                collect_node_folds(item, ranges, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Whether only blanks (or a leading BOM) precede the comment starting at
/// `comment_start` on its line.
///
/// A trailing comment sits behind content and would, in a run of consecutive
/// lines, hide the content between its comments if it took part in a fold.
fn is_own_line_comment(yaml: &str, comment_start: u32) -> bool {
    yaml.get(..comment_start as usize).is_some_and(|before| {
        before
            .rsplit(['\n', '\r'])
            .next()
            .is_some_and(|line_prefix| {
                line_prefix
                    .chars()
                    .all(|c| matches!(c, ' ' | '\t' | '\u{FEFF}'))
            })
    })
}

/// Group contiguous own-line `Event::Comment` spans (≥2 consecutive lines)
/// into folds.
fn collect_comment_folds(yaml: &str, ranges: &mut Vec<FoldingRange>) {
    let idx = LineIndex::new(yaml);
    let comment_lines: Vec<u32> = rlsp_yaml_parser::parse_events(yaml)
        .filter_map(|result| match result {
            Ok((Event::Comment { .. }, span)) if is_own_line_comment(yaml, span.start) => {
                Some(idx.line_column(span.start).0) // 1-based
            }
            _ => None,
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

/// Push a folding range.
fn push_fold(ranges: &mut Vec<FoldingRange>, start: u32, end: u32, kind: Option<FoldingRangeKind>) {
    ranges.push(FoldingRange {
        start_line: start,
        start_character: None,
        end_line: end,
        end_character: None,
        kind,
        collapsed_text: None,
    });
}

#[cfg(test)]
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
        let comment_folds: Vec<&FoldingRange> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        let region_folds: Vec<&FoldingRange> = result
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Region) || r.kind.is_none())
            .collect();
        for fold in &region_folds {
            assert!(
                fold.start_line > 2,
                "comment block should not produce a structural fold starting at line {}, got: {fold:?}",
                fold.start_line
            );
        }
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

    #[test]
    fn should_return_empty_for_comment_only_document() {
        let text = "# just a comment\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        for fold in &result {
            assert!(
                fold.kind == Some(FoldingRangeKind::Comment) || fold.kind.is_none(),
                "comment-only document should not produce Region folds"
            );
        }
    }

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

    #[test]
    fn should_fold_last_section_after_final_separator() {
        let text = "---\nkey1: val1\nkey2: val2\n";
        let docs = load_docs(text);
        let result = folding_ranges(&docs, text);
        assert!(
            !result.is_empty(),
            "content after separator should produce a fold, got: {result:?}"
        );
    }

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

    // The handler converts the columns of a range to the client's, and a range
    // without columns stays one: folds are whole lines.
    #[rstest]
    #[case::astral_in_a_comment_run("# \u{1f600}\n# \u{1f600}\nk: v\n")]
    #[case::astral_in_a_nested_mapping("k:\n  \u{1f600}: 1\n  b: 2\n")]
    #[case::several_documents("a:\n  b: 1\n---\nc:\n  d: \u{1f600}\n")]
    fn folding_ranges_carry_no_columns(#[case] text: &str) {
        let docs = load_docs(text);

        let result = folding_ranges(&docs, text);

        assert!(!result.is_empty());
        assert!(
            result
                .iter()
                .all(|r| r.start_character.is_none() && r.end_character.is_none()),
            "{result:?}"
        );
    }

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

    // A multi-line quoted scalar folds exactly like the plain scalar of the same
    // layout: the fold of its mapping or document ends where the scalar's text
    // ends, not on the line after.
    #[rstest]
    #[case::mapping_value("k: \"a\n b\"\nz: 1\n", "k: a\n b\nz: 1\n")]
    #[case::nested_mapping_value("a:\n  k: \"a\n   b\"\n", "a:\n  k: a\n   b\n")]
    #[case::single_quoted_value("k: 'a\n b'\nz: 1\n", "k: a\n b\nz: 1\n")]
    #[case::sequence_item("- \"a\n b\"\n- z\n", "- a\n b\n- z\n")]
    #[case::root_of_a_document_stream("\"a\n b\"\n---\nx\n", "a\n b\n---\nx\n")]
    #[case::root_of_a_document_stream_with_end("\"a\n b\"\n...\n---\nx\n", "a\n b\n...\n---\nx\n")]
    fn multi_line_quoted_scalar_folds_like_the_plain_scalar(
        #[case] quoted: &str,
        #[case] plain: &str,
    ) {
        let folds = |text: &str| ranges_as_tuples(&folding_ranges(&load_docs(text), text));
        assert_eq!(folds(quoted), folds(plain));
    }

    #[test]
    fn multi_line_quoted_root_in_a_document_stream_folds_its_own_lines() {
        let text = "\"a\n b\"\n---\nx\n";
        let tuples = ranges_as_tuples(&folding_ranges(&load_docs(text), text));
        assert!(tuples.contains(&(0, 1)), "{tuples:?}");
        assert!(!tuples.contains(&(0, 2)), "{tuples:?}");
    }

    fn comment_folds(yaml: &str) -> Vec<(u32, u32)> {
        let docs = load_docs(yaml);
        let comment_ranges: Vec<FoldingRange> = folding_ranges(&docs, yaml)
            .into_iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        ranges_as_tuples(&comment_ranges)
    }

    // Only comments on a line of their own take part in a comment fold: a
    // trailing comment sits behind content, so a run of them would hide the
    // content between them.
    #[rstest]
    // ---- Core ----
    #[case::trailing_on_consecutive_lines("a: 1 # x\nb: 2 # y\n", &[])]
    #[case::trailing_on_three_lines("a: 1 # x\nb: 2 # y\nc: 3 # z\n", &[])]
    #[case::own_line_run_inside_mapping_indented("k:\n  # a\n  # b\n  v: 1\n", &[(1, 2)])]
    // ---- Mixed runs ----
    #[case::trailing_then_own_line_run("a: 1 # x\n# y\n# z\nb: 2\n", &[(1, 2)])]
    #[case::own_line_run_then_trailing("# a\n# b\nk: 1 # t\n", &[(0, 1)])]
    #[case::own_line_then_trailing_single("# a\nk: 1 # t\n", &[])]
    #[case::trailing_between_own_line_comments_breaks_run("# a\nk: 1 # t\n# b\n", &[])]
    #[case::trailing_splits_run_in_two("# a\n# b\nk: 1 # t\n# c\n# d\n", &[(0, 1), (3, 4)])]
    #[case::blank_line_separates_runs("# a\n\n# b\nk: v\n", &[])]
    // ---- Whitespace and characters ----
    #[case::tab_indented_own_line_run("a: 1\n\t# t1\n  # t2\n", &[(1, 2)])]
    #[case::tab_before_trailing_comments("a:\t# x\nb:\t# y\n", &[])]
    #[case::non_ascii_before_trailing_comments("ключ: 1 # x\nключ2: 2 # y\n", &[])]
    #[case::non_ascii_trailing_then_own_line_run("ключ: 1 # x\n# y\n# z\nb: 2\n", &[(1, 2)])]
    #[case::non_ascii_own_line_run("k:\n  # привет\n  # мир\n  v: 1\n", &[(1, 2)])]
    #[case::bom_before_first_own_line_run("\u{FEFF}# a\n# b\nk: v\n", &[(0, 1)])]
    #[case::crlf_own_line_run("# a\r\n# b\r\nk: v\r\n", &[(0, 1)])]
    #[case::crlf_trailing("a: 1 # x\r\nb: 2 # y\r\n", &[])]
    #[case::crlf_trailing_then_own("a: 1 # x\r\n# y\r\n# z\r\nb: 2\r\n", &[(1, 2)])]
    #[case::bare_cr_own_line_run("# a\r# b\rk: v\r", &[(0, 1)])]
    #[case::own_line_run_at_eof_without_newline("k: v\n# a\n# b", &[(1, 2)])]
    // ---- Document structure ----
    #[case::after_document_start_own_line_run("---\n# a\n# b\nk: v\n", &[(1, 2)])]
    #[case::document_start_trailing_then_single_own("--- # a\n# b\nk: v\n", &[])]
    #[case::document_start_trailing_then_own_run("--- # a\n# b\n# c\nk: v\n", &[(1, 2)])]
    #[case::directive_and_marker_trailing("%YAML 1.2 # d\n--- # e\nk: v\n", &[])]
    #[case::multi_doc_runs_per_document("# a\n# b\nx: 1\n---\n# c\n# d\ny: 2\n", &[(0, 1), (4, 5)])]
    #[case::multi_doc_trailing_only("x: 1 # t\n---\ny: 2 # u\n", &[])]
    #[case::after_document_end_marker("a: 1\n...\n# c1\n# c2\n---\nb: 2\n", &[(2, 3)])]
    // ---- Collection and scalar contexts ----
    #[case::trailing_in_block_sequence("- a # x\n- b # y\n", &[])]
    #[case::indicator_trailing_then_own("- # c\n  # d\n  v\n", &[])]
    #[case::key_trailing_then_own_run("k: # a\n  # b\n  # c\n  v: 1\n", &[(1, 2)])]
    #[case::trailing_in_multiline_flow("k: [a, # c1\n  b] # c2\n", &[])]
    #[case::flow_own_line_run("k: [\n  a, # t\n  # o1\n  # o2\n  b\n  ]\n", &[(2, 3)])]
    #[case::block_scalar_hash_lines_are_content("k: |\n  # a\n  # b\n", &[])]
    #[case::block_scalar_header_trailing("k: |  # hdr\n  text\n# own\n", &[])]
    #[case::own_line_run_after_block_scalar("k: |\n  text\n# a\n# b\n", &[(2, 3)])]
    fn comment_folds_cover_only_own_line_comments(
        #[case] text: &str,
        #[case] expected: &[(u32, u32)],
    ) {
        assert_eq!(comment_folds(text), expected);
    }
}
