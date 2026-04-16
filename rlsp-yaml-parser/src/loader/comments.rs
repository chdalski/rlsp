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
            *leading_comments = Some(comments);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CollectionStyle, ScalarStyle};
    use crate::pos::{Pos, Span};

    fn zero_span() -> Span {
        Span {
            start: Pos::ORIGIN,
            end: Pos::ORIGIN,
        }
    }

    fn scalar_node() -> Node<Span> {
        Node::Scalar {
            value: String::new(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        }
    }

    fn mapping_node() -> Node<Span> {
        Node::Mapping {
            entries: Vec::new(),
            style: CollectionStyle::Block,
            anchor: None,
            tag: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        }
    }

    fn sequence_node() -> Node<Span> {
        Node::Sequence {
            items: Vec::new(),
            style: CollectionStyle::Block,
            anchor: None,
            tag: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        }
    }

    fn alias_node() -> Node<Span> {
        Node::Alias {
            name: "anchor".to_owned(),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        }
    }

    // attach_leading_comments tests

    #[test]
    fn attach_leading_comments_noop_on_empty_vec() {
        let mut node = scalar_node();
        if let Node::Scalar {
            ref mut leading_comments,
            ..
        } = node
        {
            *leading_comments = Some(vec!["# existing".to_owned()]);
        }
        attach_leading_comments(&mut node, vec![]);
        assert_eq!(node.leading_comments(), &["# existing"]);
    }

    #[test]
    fn attach_leading_comments_sets_comments_on_scalar() {
        let mut node = scalar_node();
        attach_leading_comments(&mut node, vec!["# a".to_owned(), "# b".to_owned()]);
        assert_eq!(node.leading_comments(), &["# a", "# b"]);
    }

    #[test]
    fn attach_leading_comments_overwrites_existing_comments() {
        let mut node = scalar_node();
        if let Node::Scalar {
            ref mut leading_comments,
            ..
        } = node
        {
            *leading_comments = Some(vec!["# old".to_owned()]);
        }
        attach_leading_comments(&mut node, vec!["# new".to_owned()]);
        assert_eq!(node.leading_comments(), &["# new"]);
    }

    #[test]
    fn attach_leading_comments_works_on_mapping() {
        let mut node = mapping_node();
        attach_leading_comments(&mut node, vec!["# a".to_owned(), "# b".to_owned()]);
        assert_eq!(node.leading_comments(), &["# a", "# b"]);
    }

    #[test]
    fn attach_leading_comments_works_on_sequence() {
        let mut node = sequence_node();
        attach_leading_comments(&mut node, vec!["# a".to_owned(), "# b".to_owned()]);
        assert_eq!(node.leading_comments(), &["# a", "# b"]);
    }

    #[test]
    fn attach_leading_comments_works_on_alias() {
        let mut node = alias_node();
        attach_leading_comments(&mut node, vec!["# a".to_owned(), "# b".to_owned()]);
        assert_eq!(node.leading_comments(), &["# a", "# b"]);
    }

    #[test]
    fn attach_leading_comments_transitions_none_to_some() {
        let mut node = scalar_node();
        attach_leading_comments(&mut node, vec!["# new".to_owned()]);
        assert_eq!(node.leading_comments(), &["# new"]);
    }

    // attach_trailing_comment tests

    #[test]
    fn attach_trailing_comment_sets_comment_on_scalar() {
        let mut node = scalar_node();
        attach_trailing_comment(&mut node, "# trail".to_owned());
        assert_eq!(node.trailing_comment(), Some("# trail"));
    }

    #[test]
    fn attach_trailing_comment_overwrites_existing_comment() {
        let mut node = scalar_node();
        if let Node::Scalar {
            ref mut trailing_comment,
            ..
        } = node
        {
            *trailing_comment = Some("# old".to_owned());
        }
        attach_trailing_comment(&mut node, "# new".to_owned());
        assert_eq!(node.trailing_comment(), Some("# new"));
    }

    #[test]
    fn attach_trailing_comment_works_on_mapping() {
        let mut node = mapping_node();
        attach_trailing_comment(&mut node, "# trail".to_owned());
        assert_eq!(node.trailing_comment(), Some("# trail"));
    }

    #[test]
    fn attach_trailing_comment_works_on_sequence() {
        let mut node = sequence_node();
        attach_trailing_comment(&mut node, "# trail".to_owned());
        assert_eq!(node.trailing_comment(), Some("# trail"));
    }

    #[test]
    fn attach_trailing_comment_works_on_alias() {
        let mut node = alias_node();
        attach_trailing_comment(&mut node, "# trail".to_owned());
        assert_eq!(node.trailing_comment(), Some("# trail"));
    }
}
