// SPDX-License-Identifier: MIT

use crate::node::Node;
use crate::pos::Span;

/// Replace the location of a node (used when stamping alias-site spans).
pub(super) fn reloc(node: Node<Span>, loc: Span) -> Node<Span> {
    match node {
        Node::Scalar {
            value,
            style,
            anchor,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Scalar {
            value,
            style,
            anchor,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Mapping {
            entries,
            anchor,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Mapping {
            entries,
            anchor,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Sequence {
            items,
            anchor,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Sequence {
            items,
            anchor,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Alias {
            name,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Alias {
            name,
            loc,
            leading_comments,
            trailing_comment,
        },
    }
}
