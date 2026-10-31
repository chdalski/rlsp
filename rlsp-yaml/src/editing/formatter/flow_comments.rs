// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Node, NodeMeta};

/// Turn every flow collection that holds a comment into a block collection, so
/// that the block renderers print the comment.  Flow style has no slot for one:
/// a comment ends at the line break, and a flow collection that prints on one
/// line has none.
///
/// A collection holds a comment when a node inside it carries one.  The
/// comments of the collection itself do not count: `a: [1] # c` stays flow.  A
/// block collection cannot sit inside a flow collection, so every flow collection
/// above a comment changes style; its siblings without a comment stay flow.
///
/// One exception: a flow collection that is a key has no block form the parser
/// reads in every position (a mapping after the first entry of a mapping: `x: 1`,
/// then `? a: 1`, then `: v`; any key that holds an entry written with `? `,
/// as in `? ? a`).  It stays flow, and the comments inside it become leading
/// comments of the key, printed above the entry.
///
/// This is the one place that decides the style of such a collection.  The
/// renderers read the style from the node, so no decision of theirs can differ.
/// One pass from the leaves up, linear in the number of nodes.
///
/// Returns whether `node` or a node inside it carries a comment.
pub(super) fn block_flow_collections_holding_comments(node: &mut Node<Span>) -> bool {
    let own = !node.leading_comments().is_empty() || node.trailing_comment().is_some();
    let inside = match node {
        Node::Mapping { entries, style, .. } => {
            // `|`, not `||`: every subtree is visited, each can hold a collection to change.
            let inside = entries.iter_mut().fold(false, |found, (key, value)| {
                resolve_key(key) | block_flow_collections_holding_comments(value) | found
            });
            if inside {
                *style = CollectionStyle::Block;
            }
            inside
        }
        Node::Sequence { items, style, .. } => {
            let inside = items.iter_mut().fold(false, |found, item| {
                block_flow_collections_holding_comments(item) | found
            });
            if inside {
                *style = CollectionStyle::Block;
            }
            inside
        }
        Node::Scalar { .. } | Node::Alias { .. } => false,
    };
    own || inside
}

/// Resolve the key of a mapping entry: a flow collection key keeps its style and
/// moves the comments inside it to its leading comments.
fn resolve_key(key: &mut Node<Span>) -> bool {
    match key {
        Node::Mapping {
            style: CollectionStyle::Flow,
            ..
        }
        | Node::Sequence {
            style: CollectionStyle::Flow,
            ..
        } => {
            let mut moved = Vec::new();
            drain_inner_comments(key, &mut moved);
            add_leading_comments(key, moved);
            !key.leading_comments().is_empty() || key.trailing_comment().is_some()
        }
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            block_flow_collections_holding_comments(key)
        }
    }
}

/// Append `comments` to the leading comments of a collection.
fn add_leading_comments(node: &mut Node<Span>, comments: Vec<String>) {
    if comments.is_empty() {
        return;
    }
    if let Node::Mapping { meta, .. } | Node::Sequence { meta, .. } = node {
        meta.get_or_insert_with(|| {
            Box::new(NodeMeta {
                anchor: None,
                anchor_loc: None,
                tag_loc: None,
                leading_comments: None,
                trailing_comment: None,
            })
        })
        .leading_comments
        .get_or_insert_with(Vec::new)
        .extend(comments);
    }
}

/// Take the comments of `node` and of the nodes inside it, in the order they
/// read in the source: a node's leading comments, the nodes inside it, its
/// trailing comment.
fn drain_comments(node: &mut Node<Span>, out: &mut Vec<String>) {
    out.extend(take_leading_comments(node));
    drain_inner_comments(node, out);
    out.extend(take_trailing_comment(node));
}

/// Take the comments of the nodes inside `node`, not its own.
fn drain_inner_comments(node: &mut Node<Span>, out: &mut Vec<String>) {
    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries.iter_mut() {
                drain_comments(key, out);
                drain_comments(value, out);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items.iter_mut() {
                drain_comments(item, out);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

fn take_leading_comments(node: &mut Node<Span>) -> Vec<String> {
    match node {
        Node::Scalar { meta, .. } | Node::Mapping { meta, .. } | Node::Sequence { meta, .. } => {
            meta.as_mut().and_then(|m| m.leading_comments.take())
        }
        Node::Alias {
            leading_comments, ..
        } => leading_comments.take(),
    }
    .unwrap_or_default()
}

fn take_trailing_comment(node: &mut Node<Span>) -> Option<String> {
    match node {
        Node::Scalar { meta, .. } | Node::Mapping { meta, .. } | Node::Sequence { meta, .. } => {
            meta.as_mut().and_then(|m| m.trailing_comment.take())
        }
        Node::Alias {
            trailing_comment, ..
        } => trailing_comment.take(),
    }
}
