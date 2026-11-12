// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::{Document, Event, LineIndex, Node, Span};
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use crate::lsp_util::last_line_of;

/// Compute folding ranges from the YAML AST.
///
/// `text` is retained solely for `Event::Comment` extraction via
/// `rlsp_yaml_parser::parse_events` — it is not used for any structural decision.
#[must_use]
pub fn folding_ranges(docs: &[Document<Span>], text: &str) -> Vec<FoldingRange> {
    let Some(first_doc) = docs.first() else {
        return Vec::new();
    };
    let idx = first_doc.line_index();
    let is_multidoc = docs.len() > 1;

    let mut ranges = Vec::new();
    for doc in docs {
        // In a multi-document stream, each document's root gets one Region
        // fold so the fold boundary matches the `---`-delimited section.
        let kind = is_multidoc.then_some(FoldingRangeKind::Region);
        fold_node(&doc.root, &mut ranges, idx, kind);
    }
    // A placeholder that never got a line after its start folds nothing.
    ranges.retain(|range| range.end_line > range.start_line);

    collect_comment_folds(text, &mut ranges);

    ranges
}

/// The line a node's span starts on, 0-based.
fn start_line_of(loc: Span, idx: &LineIndex) -> u32 {
    // `line_column` counts lines from 1; LSP lines start at 0.
    idx.line_column(loc.start).0.saturating_sub(1)
}

/// Fold the structural nodes below `node` and return the last line its last
/// leaf covers, or `None` if the subtree has no leaf with text.
///
/// A leaf is a scalar or alias with a non-empty span, or a collection without
/// entries or items. A zero-width scalar (an empty value or item) marks no
/// position in the text, so it is skipped: the fold of its collection ends on
/// the text before it. The span of a block collection reaches into the next
/// sibling's line, past trailing blank lines and comments, so the fold takes
/// its end from the leaf, never from the span.
///
/// A collection, and any node given a `kind` (the root of a document in a
/// stream), gets a fold of that kind from the line its span starts on. Its
/// slot is pushed before the children so folds come out outer before inner;
/// the end line is filled in once the children are known, and
/// `folding_ranges` drops the slots that stay empty.
fn fold_node(
    node: &Node<Span>,
    ranges: &mut Vec<FoldingRange>,
    idx: &LineIndex,
    kind: Option<FoldingRangeKind>,
) -> Option<u32> {
    let is_collection = matches!(node, Node::Mapping { .. } | Node::Sequence { .. });
    let loc = match node {
        Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Scalar { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    };
    let start_line = start_line_of(loc, idx);
    let slot = (is_collection || kind.is_some()).then(|| {
        push_fold(ranges, start_line, start_line, kind);
        ranges.len() - 1
    });

    let last_line = match node {
        Node::Mapping { entries, .. } if entries.is_empty() => Some(last_line_of(loc, idx)),
        Node::Sequence { items, .. } if items.is_empty() => Some(last_line_of(loc, idx)),
        Node::Mapping { entries, .. } => entries
            .iter()
            .flat_map(|(key, value)| [key, value])
            .fold(None, |last, child| {
                fold_node(child, ranges, idx, None).or(last)
            }),
        Node::Sequence { items, .. } => items.iter().fold(None, |last, child| {
            fold_node(child, ranges, idx, None).or(last)
        }),
        Node::Scalar { .. } | Node::Alias { .. } => {
            (loc.start < loc.end).then(|| last_line_of(loc, idx))
        }
    };

    if let (Some(end_line), Some(range)) = (last_line, slot.and_then(|slot| ranges.get_mut(slot))) {
        range.end_line = end_line;
    }
    last_line
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

    const REGION: Option<FoldingRangeKind> = Some(FoldingRangeKind::Region);
    const COMMENT: Option<FoldingRangeKind> = Some(FoldingRangeKind::Comment);

    fn load_docs(yaml: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(yaml).expect("test input must be valid YAML")
    }

    fn ranges_as_tuples(ranges: &[FoldingRange]) -> Vec<(u32, u32)> {
        ranges.iter().map(|r| (r.start_line, r.end_line)).collect()
    }

    /// Every fold in the order the handler truncates them: start line, end
    /// line, kind.
    fn folds(yaml: &str) -> Vec<(u32, u32, Option<FoldingRangeKind>)> {
        folding_ranges(&load_docs(yaml), yaml)
            .into_iter()
            .map(|r| (r.start_line, r.end_line, r.kind))
            .collect()
    }

    // A structural fold ends on the last line of its collection's last leaf:
    // the span of a block collection reaches into the next sibling, past
    // trailing blank lines and comments. Folds come out outer before inner, the
    // Region fold of a stream's document root before the folds of its
    // children, and comment folds last.
    #[rstest]
    // ---- No fold ----
    #[case::empty_document("", &[])]
    #[case::single_line_mapping("key: value", &[])]
    #[case::single_line_block_sequence("- a", &[])]
    #[case::single_line_flow_mapping("{a: 1, b: 2}\n", &[])]
    #[case::single_line_flow_sequence("[a, b]", &[])]
    #[case::single_entry_flow_mapping("{a: 1}", &[])]
    #[case::root_without_a_leaf_with_text("-\n-\n", &[])]
    #[case::only_empty_documents("---\n---\n", &[])]
    #[case::multi_line_scalar_root_of_one_document("a\n b\n", &[])]
    // ---- Basic shapes ----
    #[case::root_sequence("- one\n- two\n- three\n", &[(0, 2, None)])]
    #[case::nested_mapping(
        "server:\n  host: localhost\n  port: 8080\n",
        &[(0, 2, None), (1, 2, None)]
    )]
    #[case::nested_sequence(
        "items:\n  - one\n  - two\n  - three\n",
        &[(0, 3, None), (1, 3, None)]
    )]
    #[case::sequence_of_mappings(
        "users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n",
        &[(0, 4, None), (1, 4, None), (1, 2, None), (3, 4, None)]
    )]
    #[case::comment_inside_a_mapping(
        "server:\n  # This is a comment\n  host: localhost\n  port: 8080\n",
        &[(0, 3, None), (2, 3, None)]
    )]
    #[case::blank_line_inside_a_mapping(
        "server:\n  host: localhost\n\n  port: 8080\n",
        &[(0, 3, None), (1, 3, None)]
    )]
    #[case::mixed_content_types(
        "config:\n  name: app\n  ports:\n    - 80\n    - 443\n  description: |\n    A multi-line\n    description\n",
        &[(0, 7, None), (1, 7, None), (3, 4, None)]
    )]
    #[case::fold_ends_before_the_next_sibling("a:\n  b: 1\nc: 2\n", &[(0, 2, None)])]
    #[case::inner_fold_ends_before_the_next_sibling(
        "a:\n  b: 1\n  c: 3\nd: 4\n",
        &[(0, 3, None), (1, 2, None)]
    )]
    #[case::nested_mappings_without_a_final_line_break(
        "a:\n  b:\n    c: 1\n    d: 2",
        &[(0, 3, None), (1, 3, None), (2, 3, None)]
    )]
    #[case::sequence_at_the_indent_of_its_key(
        "a:\n- x\n- y\nb: 1\n",
        &[(0, 3, None), (1, 2, None)]
    )]
    #[case::compact_nested_sequence("- - a\n  - b\n- c\n", &[(0, 2, None), (0, 1, None)])]
    #[case::compact_mapping_items("- a: 1\n  b: 2\n- c: 3\n", &[(0, 2, None), (0, 1, None)])]
    #[case::multi_line_plain_scalar_leaf(
        "a:\n  k: x\n    y\n",
        &[(0, 2, None), (1, 2, None)]
    )]
    #[case::multi_byte_keys_and_values(
        "ключ:\n  \u{1f600}: 1\n  b: \u{1f600}\nz: 1\n",
        &[(0, 3, None), (1, 2, None)]
    )]
    // ---- Trailing blank lines and comments stay outside ----
    #[case::trailing_blank_lines_at_the_end(
        "a:\n  b: 1\n  c: 2\n\n\n",
        &[(0, 2, None), (1, 2, None)]
    )]
    #[case::trailing_indented_comments(
        "a:\n  b: 1\n  c: 2\n  # t1\n  # t2\n",
        &[(0, 2, None), (1, 2, None), (3, 4, COMMENT)]
    )]
    #[case::trailing_comments_at_column_0(
        "a:\n  b: 1\n  c: 2\n# t1\n# t2\n",
        &[(0, 2, None), (1, 2, None), (3, 4, COMMENT)]
    )]
    #[case::comments_on_the_leaf_lines(
        "a:\n  b: 1 # c\n  c: 2 # d\nz: 1",
        &[(0, 3, None), (1, 2, None)]
    )]
    #[case::comment_fold_comes_last(
        "a:\n  # x\n  # y\n  b: 1",
        &[(0, 3, None), (1, 2, COMMENT)]
    )]
    // ---- Empty values and items are skipped ----
    #[case::empty_last_value_with_blank_line_and_comment(
        "a:\n  b: 1\n  c:\n\n# x\nd: 1\n",
        &[(0, 5, None), (1, 2, None)]
    )]
    #[case::question_key_without_a_value(
        "a:\n  b: 1\n  ? k\n\nc: 1\n",
        &[(0, 4, None), (1, 2, None)]
    )]
    #[case::empty_value_with_a_tag(
        "a:\n  b: !!str\n\n# x\nc: 1\n",
        &[(0, 4, None)]
    )]
    #[case::empty_last_item(
        "a:\n  - x\n  -\n\n# x\nd: 1\n",
        &[(0, 5, None)]
    )]
    #[case::only_empty_items("a:\n  -\n  -\nb: 1\n", &[(0, 3, None)])]
    #[case::mapping_with_only_empty_values("a:\nb:\n", &[(0, 1, None)])]
    #[case::nested_mapping_with_only_empty_values(
        "a:\n  b:\n  c:\nd: 1\n",
        &[(0, 3, None), (1, 2, None)]
    )]
    #[case::empty_string_is_a_leaf(
        "a:\n  - x\n  - \"\"\nb: 1\n",
        &[(0, 3, None), (1, 2, None)]
    )]
    #[case::anchor_and_tag_without_a_value("a: &an\nb: !!str\n", &[(0, 1, None)])]
    // ---- Aliases, anchors ----
    #[case::alias_is_a_leaf("- &x 1\n- *x\n", &[(0, 1, None)])]
    #[case::anchored_collection_starts_on_its_first_entry(
        "k: &x\n  a: 1\n  b: 2\nj: *x\n",
        &[(0, 3, None), (1, 2, None)]
    )]
    // ---- Flow collections ----
    #[case::multi_line_flow_sequence("k: [a,\n  b]", &[(0, 1, None), (0, 1, None)])]
    #[case::multi_line_flow_mapping("k: {a: 1,\n  b: 2}", &[(0, 1, None), (0, 1, None)])]
    #[case::closing_bracket_on_its_own_line(
        "k: [a,\n  b\n  ]\n",
        &[(0, 1, None), (0, 1, None)]
    )]
    #[case::closing_bracket_on_its_own_line_before_a_sibling(
        "k: [a,\n  b\n  ]\nz: 1\n",
        &[(0, 3, None), (0, 1, None)]
    )]
    #[case::flow_mapping_entry_without_a_value(
        "k: {a: 1,\n  b}\nz: 1\n",
        &[(0, 2, None), (0, 1, None)]
    )]
    #[case::nested_multi_line_flow(
        "k: [a,\n  [b,\n   c]]\n",
        &[(0, 2, None), (0, 2, None), (1, 2, None)]
    )]
    #[case::empty_flow_sequence_is_a_leaf(
        "a:\n  - x\n  - []\nb: 1\n",
        &[(0, 3, None), (1, 2, None)]
    )]
    #[case::implicit_pairs_in_a_flow_sequence(
        "k: [a: 1,\n  b: 2]\n",
        &[(0, 1, None), (0, 1, None)]
    )]
    // ---- Complex keys ----
    #[case::key_is_a_flow_sequence_with_a_value(
        "? [a,\n  b]\n: v\n",
        &[(0, 2, None), (0, 1, None)]
    )]
    #[case::key_is_a_flow_sequence_without_a_value(
        "? [a,\n  b]\n",
        &[(0, 1, None), (0, 1, None)]
    )]
    #[case::key_and_value_are_block_sequences(
        "? - a\n  - b\n: - c\n  - d\n",
        &[(0, 3, None), (0, 1, None), (2, 3, None)]
    )]
    // ---- Block scalars keep their trailing blank lines ----
    #[case::block_scalar_then_blank_line_and_comment(
        "a:\n  k: |\n    t\n\n# x\nz: 1\n",
        &[(0, 5, None), (1, 3, None)]
    )]
    #[case::block_scalar_with_keep_chomping(
        "a:\n  k: |+\n    t\n\n# x\nz: 1\n",
        &[(0, 5, None), (1, 3, None)]
    )]
    #[case::block_scalar_with_blank_lines_at_the_end("k: |\n  a\n\n\n", &[(0, 3, None)])]
    #[case::block_scalar_without_a_final_line_break("k: |\n  a\n  b", &[(0, 2, None)])]
    #[case::block_scalar_with_a_final_line_break("k: |\n  a\n  b\n", &[(0, 2, None)])]
    #[case::block_scalar_then_whitespace_only_line("k: |\n  a\n\n  \n# x\n", &[(0, 3, None)])]
    // ---- Document streams ----
    #[case::two_documents(
        "key1: val1\nkey2: val2\n---\nkey3: val3\nkey4: val4\n",
        &[(0, 1, REGION), (3, 4, REGION)]
    )]
    #[case::two_documents_with_nested_content(
        "doc1:\n  key: val\n---\ndoc2:\n  key: val\n",
        &[(0, 1, REGION), (3, 4, REGION)]
    )]
    #[case::three_documents(
        "a: 1\nb: 2\n---\nc: 3\nd: 4\n---\ne: 5\nf: 6\n",
        &[(0, 1, REGION), (3, 4, REGION), (6, 7, REGION)]
    )]
    #[case::one_document_after_a_start_marker(
        "---\nkey1: val1\nkey2: val2\n",
        &[(1, 2, None)]
    )]
    #[case::nested_folds_after_their_document_fold(
        "a:\n  b: 1\n  c: 2\n---\nx: 1\n",
        &[(0, 2, REGION), (1, 2, None)]
    )]
    #[case::root_region_before_its_children(
        "---\n---\nc:\n  d:\n    x: 1\n    y: 2",
        &[(2, 5, REGION), (3, 5, None), (4, 5, None)]
    )]
    #[case::comment_before_the_separator(
        "a: 1\nb: 2\n# c\n---\nd: 1\ne: 2\n",
        &[(0, 1, REGION), (4, 5, REGION)]
    )]
    #[case::comment_run_before_the_separator(
        "a: 1\nb: 2\n\n# c1\n# c2\n---\nd: 1\n",
        &[(0, 1, REGION), (3, 4, COMMENT)]
    )]
    #[case::document_end_marker(
        "a: 1\nb: 2\n...\n---\nc: 3\nd: 4\n",
        &[(0, 1, REGION), (4, 5, REGION)]
    )]
    #[case::empty_document_between(
        "a: 1\nb: 2\n---\n---\nc: 3\nd: 4\n",
        &[(0, 1, REGION), (4, 5, REGION)]
    )]
    #[case::document_without_a_leaf(
        "-\n-\n---\nx: 1\ny: 2\n",
        &[(3, 4, REGION)]
    )]
    #[case::anchor_on_the_marker_line(
        "--- &x\na: 1\nb: 2\n---\nc: 1\n",
        &[(1, 2, REGION)]
    )]
    #[case::directive(
        "%YAML 1.2\n---\na: 1\nb: 2\n---\nc: 3\nd: 4\n",
        &[(2, 3, REGION), (5, 6, REGION)]
    )]
    #[case::block_scalar_root(
        "--- |\n  a\n  b\n\n---\nx\n",
        &[(0, 3, REGION)]
    )]
    #[case::quoted_root("\"a\n b\"\n---\nx\n", &[(0, 1, REGION)])]
    fn structural_folds_end_on_the_last_line_of_the_last_leaf(
        #[case] text: &str,
        #[case] expected: &[(u32, u32, Option<FoldingRangeKind>)],
    ) {
        assert_eq!(folds(text), expected);
    }

    // The fold lines do not depend on how lines break or whether the text ends
    // on a break, also where a span ends at the start of a line.
    #[rstest]
    #[case::empty_value_blank_line_and_comment(
        "a:\n  b: 1\n  c:\n\n# x\nd: 1\n",
        &[(0, 5, None), (1, 2, None)]
    )]
    #[case::block_scalar_then_blank_line_and_comment(
        "a:\n  k: |\n    t\n\n# x\nz: 1\n",
        &[(0, 5, None), (1, 3, None)]
    )]
    #[case::block_scalar_with_blank_lines_at_the_end("k: |\n  a\n\n\n", &[(0, 3, None)])]
    #[case::two_documents(
        "a: 1\nb: 2\n---\nc: 3\nd: 4\n",
        &[(0, 1, REGION), (3, 4, REGION)]
    )]
    #[case::no_final_line_break("a:\n  b: 1\n  c: 2", &[(0, 2, None), (1, 2, None)])]
    fn structural_folds_do_not_depend_on_the_line_break(
        #[case] text: &str,
        #[case] expected: &[(u32, u32, Option<FoldingRangeKind>)],
    ) {
        assert_eq!(folds(text), expected, "LF");
        assert_eq!(folds(&text.replace('\n', "\r\n")), expected, "CRLF");
        assert_eq!(folds(&text.replace('\n', "\r")), expected, "bare CR");
    }

    // A hostile document is a deep or a wide one. The walk visits each node
    // once, so these inputs stay linear, and the depth stays the loader's.
    const LOADER_DEPTH_LIMIT: usize = 256;

    fn line(n: usize) -> u32 {
        u32::try_from(n).expect("a test line fits in u32")
    }

    #[test]
    fn a_collection_with_many_empty_items_folds_nothing() {
        let text = "-\n".repeat(10_000);

        assert_eq!(folds(&text), &[]);
    }

    #[test]
    fn a_mapping_with_many_empty_values_ends_on_its_last_key() {
        let text = "k:\n".repeat(10_000);

        assert_eq!(folds(&text), &[(0, 9_999, None)]);
    }

    #[test]
    fn many_empty_items_before_the_one_leaf_end_on_the_leaf() {
        let text = format!("{}- x\n", "-\n".repeat(10_000));

        assert_eq!(folds(&text), &[(0, 10_000, None)]);
    }

    #[test]
    fn a_chain_of_empty_collections_folds_nothing_at_the_loader_depth_limit() {
        let text: String = (0..LOADER_DEPTH_LIMIT)
            .map(|level| ["  ".repeat(level).as_str(), "-\n"].concat())
            .collect();

        assert_eq!(folds(&text), &[]);
    }

    #[test]
    fn nested_block_mappings_at_the_loader_depth_limit_each_end_on_the_leaf() {
        let last = LOADER_DEPTH_LIMIT - 1;
        let text: String = (0..last)
            .map(|level| format!("{}k:\n", "  ".repeat(level)))
            .chain([format!("{}leaf: 1\n", "  ".repeat(last))])
            .collect();

        let expected: Vec<_> = (0..last)
            .map(|level| (line(level), line(last), None))
            .collect();
        assert_eq!(folds(&text), expected);
    }

    #[test]
    fn nested_flow_sequences_at_the_loader_depth_limit_each_end_on_the_leaf() {
        let text = format!(
            "{}x\n{}\n",
            "[\n".repeat(LOADER_DEPTH_LIMIT),
            "]".repeat(LOADER_DEPTH_LIMIT)
        );

        let expected: Vec<_> = (0..LOADER_DEPTH_LIMIT)
            .map(|level| (line(level), line(LOADER_DEPTH_LIMIT), None))
            .collect();
        assert_eq!(folds(&text), expected);
    }

    #[test]
    fn nested_flow_collections_end_on_the_innermost_empty_collection() {
        let text = format!("{}[]{}", "{a: [\n".repeat(100), "]}".repeat(100));

        let expected: Vec<_> = (0..100)
            .flat_map(|line| [(line, 100, None), (line, 100, None)])
            .collect();
        assert_eq!(folds(&text), expected);
    }

    #[test]
    fn aliases_are_leaves_and_are_not_expanded() {
        let text = "a: &a [x,\n  x]\nb: &b [*a,\n  *a]\nc: [*b,\n  *b]\n";

        assert_eq!(
            folds(text),
            &[(0, 5, None), (0, 1, None), (2, 3, None), (4, 5, None)]
        );
    }

    // A byte order mark in front of the first line moves no fold.
    #[test]
    fn a_leading_bom_does_not_move_the_folds() {
        let text = "\u{FEFF}a:\n  b: 1\n  c: 2\n# x\n";
        assert_eq!(folds(text), &[(0, 2, None), (1, 2, None)]);
    }

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
