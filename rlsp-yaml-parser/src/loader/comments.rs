// SPDX-License-Identifier: MIT

use crate::node::Node;
use crate::pos::Span;

/// Attach `leading_comments` to a node's `leading_comments` field.
pub(super) fn attach_leading_comments(node: &mut Node<Span>, comments: Vec<String>) {
    if comments.is_empty() {
        return;
    }
    match node {
        Node::Scalar {
            leading_comments, ..
        }
        | Node::Mapping {
            leading_comments, ..
        }
        | Node::Sequence {
            leading_comments, ..
        }
        | Node::Alias {
            leading_comments, ..
        } => {
            *leading_comments = comments;
        }
    }
}

/// Attach a trailing comment to a node's `trailing_comment` field.
pub(super) fn attach_trailing_comment(node: &mut Node<Span>, comment: String) {
    match node {
        Node::Scalar {
            trailing_comment, ..
        }
        | Node::Mapping {
            trailing_comment, ..
        }
        | Node::Sequence {
            trailing_comment, ..
        }
        | Node::Alias {
            trailing_comment, ..
        } => {
            *trailing_comment = Some(comment);
        }
    }
}
