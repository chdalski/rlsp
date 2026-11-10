// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Pos, Span};
use tower_lsp::lsp_types::Position;

/// Find-references provider.
pub mod references;
/// Symbol rename provider.
pub mod rename;

/// `(name, span)` pairs for anchor or alias tokens.
pub(crate) type NamedSpans = Vec<(String, Span)>;

/// Walk a single document and collect `(name, span)` pairs for every anchor
/// token and every alias token.
///
/// Anchor spans come from `Node::*.anchor_loc` (the `&name` token span).
/// Alias spans come from `Node::Alias.loc` (the `*name` token span).
pub(crate) fn collect_anchor_alias_entries(doc: &Document<Span>) -> (NamedSpans, NamedSpans) {
    let mut anchors = Vec::new();
    let mut aliases = Vec::new();
    collect_node(&doc.root, &mut anchors, &mut aliases);
    (anchors, aliases)
}

fn collect_node(node: &Node<Span>, anchors: &mut NamedSpans, aliases: &mut NamedSpans) {
    match node {
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } => {
            if let (Some(name), Some(loc)) = (node.anchor(), node.anchor_loc()) {
                anchors.push((name.to_owned(), loc));
            }
        }
        Node::Alias { name, loc, .. } => {
            aliases.push((name.clone(), *loc));
        }
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                collect_node(k, anchors, aliases);
                collect_node(v, anchors, aliases);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node(item, anchors, aliases);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Find the document whose root span contains the cursor (per-document scoping).
fn containing_document(
    docs: &[Document<Span>],
    cursor: Pos,
) -> Option<(&Document<Span>, &LineIndex)> {
    docs.iter().find_map(|doc| {
        let idx = doc.line_index();
        if span_contains(node_loc(&doc.root), cursor, idx) {
            Some((doc, idx))
        } else {
            None
        }
    })
}

/// Returns the location span of a node.
const fn node_loc(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

/// Returns `true` when `cursor` is within `span` using half-open `[start, end)`.
fn span_contains(span: Span, cursor: Pos, idx: &LineIndex) -> bool {
    let start = (
        idx.line_column(span.start).0 as usize,
        idx.line_column(span.start).1 as usize,
    );
    let end = (
        idx.line_column(span.end).0 as usize,
        idx.line_column(span.end).1 as usize,
    );
    let pos = (cursor.line, cursor.column);
    pos >= start && pos < end
}

/// Convert an LSP `Position` (0-based line, 0-based character) to a parser `Pos`
/// (1-based line, 0-based column).
const fn lsp_to_pos(position: Position) -> Pos {
    Pos {
        byte_offset: 0,
        line: position.line as usize + 1,
        column: position.character as usize,
    }
}
