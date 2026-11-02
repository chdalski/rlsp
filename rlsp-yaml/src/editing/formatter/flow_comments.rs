// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Node, NodeMeta};
use rlsp_yaml_parser::{CollectionStyle, Span};

/// Settle the style of the collections that cannot print as the source wrote
/// them, and move the comments that only a block collection can print.
///
/// First, every flow collection that holds a comment becomes a block
/// collection, so that the block renderers print the comment.  Flow style has
/// no slot for one: a comment ends at the line break, and a flow collection
/// that prints on one line has none.  A collection holds a comment when a node
/// inside it carries one.  The comments of the collection itself do not count:
/// `a: [1] # c` stays flow.  A block collection cannot sit inside a flow
/// collection, so every flow collection above a comment changes style; its
/// siblings without a comment stay flow.
///
/// Second, a flow collection used as a key stays flow, and the comments inside it
/// become leading comments of the key, printed above the entry: a key is printed
/// on one line after the `? `, and a block collection cannot sit inside a flow
/// key.  A block collection key keeps its style, and so do the comments inside
/// it.  `enforce_block` (the option that prints every collection as block) does
/// not change a flow key; it is handled where the key prints.
///
/// This is the one place that decides the style of such a collection.  The
/// renderers read the style from the node, so no decision of theirs can differ.
/// One pass from the leaves up.
///
/// Returns whether `node` or a node inside it carries a comment.
pub(super) fn settle_collection_styles(node: &mut Node<Span>) -> bool {
    let own = !node.leading_comments().is_empty() || node.trailing_comment().is_some();
    let inside = match node {
        Node::Mapping { entries, style, .. } => {
            // `|`, not `||`: every subtree is visited, each can hold a collection to change.
            let inside = entries.iter_mut().fold(false, |found, (key, value)| {
                settle_key(key) | settle_collection_styles(value) | found
            });
            if inside {
                *style = CollectionStyle::Block;
            }
            inside
        }
        Node::Sequence { items, style, .. } => {
            let inside = items
                .iter_mut()
                .fold(false, |found, item| settle_collection_styles(item) | found);
            if inside {
                *style = CollectionStyle::Block;
            }
            inside
        }
        Node::Scalar { .. } | Node::Alias { .. } => false,
    };
    own || inside
}

/// Settle the key of a mapping entry: a flow collection key stays flow and
/// moves the comments inside it to its leading comments; any other key settles
/// like a value.
fn settle_key(key: &mut Node<Span>) -> bool {
    match key {
        Node::Mapping {
            style: CollectionStyle::Flow,
            ..
        }
        | Node::Sequence {
            style: CollectionStyle::Flow,
            ..
        } => keep_flow_key(key),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Scalar { .. } | Node::Alias { .. } => {
            settle_collection_styles(key)
        }
    }
}

/// A flow collection key: move the comments inside it to its leading comments.
/// Returns whether the key now carries a comment.
fn keep_flow_key(key: &mut Node<Span>) -> bool {
    let mut moved = Vec::new();
    drain_inner_comments(key, &mut moved);
    add_leading_comments(key, moved);
    !key.leading_comments().is_empty() || key.trailing_comment().is_some()
}

/// Append `comments` to the leading comments of a collection.
fn add_leading_comments(node: &mut Node<Span>, comments: Vec<String>) {
    if !comments.is_empty() {
        leading_comments_mut(node).extend(comments);
    }
}

/// The leading comments of `node`, created empty when it has none.
pub(super) fn leading_comments_mut(node: &mut Node<Span>) -> &mut Vec<String> {
    match node {
        Node::Scalar { meta, .. } | Node::Mapping { meta, .. } | Node::Sequence { meta, .. } => {
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
        }
        Node::Alias {
            leading_comments, ..
        } => leading_comments.get_or_insert_with(Vec::new),
    }
}

/// Take the comments of `node` and of the nodes inside it, in the order they
/// read in the source: a node's leading comments, the nodes inside it, its
/// trailing comment.
pub(super) fn drain_comments(node: &mut Node<Span>, out: &mut Vec<String>) {
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
