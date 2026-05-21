// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{ScalarStyle, Span};

use super::comment_preservation::{Comment, find_comment_on_line};

/// A content line from the original text, with its blank-line and doc-prefix comment context.
pub(super) struct ContentEntry {
    pub(super) signature: String,
    /// Number of blank lines that preceded this content line in the original.
    /// Capped at 1 — multiple consecutive blank lines collapse to one.
    pub(super) blank_lines_before: usize,
    /// Document-prefix leading comment lines that precede this content line.
    pub(super) leading: Vec<String>,
}

/// Extract the content signature from a line: the trimmed non-comment portion.
pub(super) fn content_signature(line: &str) -> String {
    if let Some((byte_pos, _)) = find_comment_on_line(line) {
        line[..byte_pos].trim().to_string()
    } else {
        line.trim().to_string()
    }
}

/// Compute the last "YAML content" line index from the AST.
///
/// Block scalar values (literal `|` or folded `>`) can contain lines that start with
/// `#`, which the text-scanning heuristic in `last_content_line_idx` misidentifies as
/// standalone comments.  This function derives the last content line from AST span
/// information, which is authoritative.
///
/// For each scalar node: `last_line = (idx.line_column(loc.start).0 as usize) + lines_in_value - 1`.
/// The `(idx.line_column(loc.start).0 as usize)` is the first content line (not the header line for block scalars).
/// `lines_in_value` counts the lines in the decoded value — for block scalars this
/// includes any trailing blank lines consumed by the chomp indicator.
pub(super) fn last_content_line_from_ast(docs: &[Document<Span>]) -> Option<usize> {
    fn node_last_content_line(
        node: &Node<Span>,
        idx: &rlsp_yaml_parser::LineIndex,
    ) -> Option<usize> {
        match node {
            // Block scalars occupy multiple source lines.  The decoded value
            // includes trailing blank lines consumed by the chomp indicator,
            // so `lines().count()` gives the exact number of occupied source
            // lines (content + any trailing blanks consumed by keep/clip/strip).
            Node::Scalar {
                style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
                loc,
                value,
                ..
            } => {
                // line_column returns 1-based line; convert to 0-based first.
                let start_0 = idx.line_column(loc.start).0.saturating_sub(1) as usize;
                let line_count = value.lines().count();
                Some(start_0 + line_count.saturating_sub(1))
            }
            // Non-block scalars (plain, single-quoted, double-quoted) always
            // occupy a single source line regardless of how many newlines are
            // embedded in the decoded value.
            Node::Scalar { loc, .. } | Node::Alias { loc, .. } => {
                Some(idx.line_column(loc.start).0.saturating_sub(1) as usize)
            }
            Node::Mapping { entries, .. } => entries
                .iter()
                .flat_map(|(k, v)| {
                    [
                        node_last_content_line(k, idx),
                        node_last_content_line(v, idx),
                    ]
                })
                .flatten()
                .max(),
            Node::Sequence { items, .. } => items
                .iter()
                .filter_map(|n| node_last_content_line(n, idx))
                .max(),
        }
    }
    docs.iter()
        .filter_map(|doc| node_last_content_line(&doc.root, doc.line_index()))
        .max()
}

/// Compute the last content line index via text scan.
///
/// `#` lines that appear in the `line_to_comment` map are doc-prefix comments and count
/// as content boundaries; all other `#`-prefixed lines are comments that do not.
pub(super) fn last_content_line_idx(
    original: &str,
    line_to_comment: &std::collections::HashMap<usize, &Comment>,
) -> Option<usize> {
    original
        .lines()
        .enumerate()
        .filter(|(idx, line)| {
            !line.trim().is_empty()
                && (!line.trim_start().starts_with('#') || line_to_comment.contains_key(idx))
        })
        .map(|(idx, _)| idx)
        .last()
}
