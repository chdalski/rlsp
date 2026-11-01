// SPDX-License-Identifier: MIT

use std::iter;

use rlsp_yaml_parser::node::{Node, NodeMeta};
use rlsp_yaml_parser::{CollectionStyle, ScalarStyle, Span};

use super::mapping_render;

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
/// Second, a collection key that has no block form the parser reads in every
/// position keeps the flow style or takes it: a mapping after the first entry of
/// a mapping (`x: 1`, then `? a: 1`, then `: v`), and a collection that holds an
/// entry written with `? ` at any depth (`? [a]: x`, `? ? a`).  The parser reads
/// the `:` that follows such a key as the start of a new entry.  The comments
/// inside the key become leading comments of the key, printed above the entry.
/// `enforce_block` (the option that prints every collection as block) does not
/// change this: it decides which nodes print as block, and these keys do not.
///
/// This is the one place that decides the style of such a collection.  The
/// renderers read the style from the node, so no decision of theirs can differ.
/// One pass from the leaves up; the test for an entry written with `? ` runs for
/// each collection key over the nodes inside it.
///
/// Returns whether `node` or a node inside it carries a comment.
pub(super) fn settle_collection_styles(node: &mut Node<Span>, enforce_block: bool) -> bool {
    let own = !node.leading_comments().is_empty() || node.trailing_comment().is_some();
    let inside = match node {
        Node::Mapping { entries, style, .. } => {
            // `|`, not `||`: every subtree is visited, each can hold a collection to change.
            let inside =
                entries
                    .iter_mut()
                    .enumerate()
                    .fold(false, |found, (index, (key, value))| {
                        resolve_key(key, index > 0, enforce_block)
                            | settle_collection_styles(value, enforce_block)
                            | found
                    });
            if inside {
                *style = CollectionStyle::Block;
            }
            inside
        }
        Node::Sequence { items, style, .. } => {
            let inside = items.iter_mut().fold(false, |found, item| {
                settle_collection_styles(item, enforce_block) | found
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

/// Resolve the key of a mapping entry: a collection key that cannot print as
/// block keeps or takes the flow style, and moves the comments inside it to its
/// leading comments.
fn resolve_key(key: &mut Node<Span>, after_first_entry: bool, enforce_block: bool) -> bool {
    match key {
        Node::Mapping {
            style: CollectionStyle::Flow,
            ..
        }
        | Node::Sequence {
            style: CollectionStyle::Flow,
            ..
        } => keep_flow_key(key),
        Node::Mapping { .. } | Node::Sequence { .. } => {
            let holds_comment = settle_collection_styles(key, enforce_block);
            let block_mapping_after_first = after_first_entry
                && matches!(key, Node::Mapping { entries, .. } if !entries.is_empty());
            if block_mapping_after_first || holds_explicit_entry(key, enforce_block, false) {
                let mut moved = Vec::new();
                drain_inner_comments(key, &mut moved);
                add_leading_comments(key, moved);
                into_flow(key);
                !key.leading_comments().is_empty() || key.trailing_comment().is_some()
            } else {
                holds_comment
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => settle_collection_styles(key, enforce_block),
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

/// Whether the block form of `node` has an entry written with `? ` at any depth.
/// `enforce_block` makes every collection a block collection.  `value_indicator`
/// tells whether `node` is the value of an entry (see `mapping_render::Lead`).
fn holds_explicit_entry(node: &Node<Span>, enforce_block: bool, value_indicator: bool) -> bool {
    let is_block = |style: CollectionStyle| enforce_block || style == CollectionStyle::Block;
    match node {
        Node::Mapping {
            entries,
            style,
            tag,
            ..
        } if is_block(*style) => {
            let lead = mapping_render::Lead {
                properties: mapping_render::collection_properties(node.anchor(), tag.as_deref())
                    .is_some(),
                value_indicator,
            };
            let previous_values =
                iter::once(None).chain(entries.iter().map(|(_, value)| Some(value)));
            entries
                .iter()
                .zip(previous_values)
                .any(|((key, value), previous)| {
                    mapping_render::needs_explicit_key(key)
                        || mapping_render::forced_explicit_key(previous, key, lead)
                        || holds_explicit_entry(value, enforce_block, true)
                })
        }
        Node::Sequence { items, style, .. } if is_block(*style) => items
            .iter()
            .any(|item| holds_explicit_entry(item, enforce_block, false)),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Scalar { .. } | Node::Alias { .. } => {
            false
        }
    }
}

/// Give every collection in `node` the flow style and every block scalar the
/// double-quoted style; the values stay.
fn into_flow(node: &mut Node<Span>) {
    match node {
        Node::Mapping { entries, style, .. } => {
            *style = CollectionStyle::Flow;
            for (key, value) in entries.iter_mut() {
                into_flow(key);
                into_flow(value);
            }
        }
        Node::Sequence { items, style, .. } => {
            *style = CollectionStyle::Flow;
            items.iter_mut().for_each(into_flow);
        }
        Node::Scalar { style, .. } => {
            if matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_)) {
                *style = ScalarStyle::DoubleQuoted;
            }
        }
        Node::Alias { .. } => {}
    }
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
