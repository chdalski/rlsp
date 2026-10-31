// SPDX-License-Identifier: MIT

use crate::node::{Node, NodeMeta};
use crate::pos::Span;

/// Attach `leading_comments` to a node's `leading_comments` field.
pub(super) fn attach_leading_comments(node: &mut Node<Span>, comments: Vec<String>) {
    if comments.is_empty() {
        return;
    }
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
            .leading_comments = Some(comments);
        }
        Node::Alias {
            leading_comments, ..
        } => {
            *leading_comments = Some(comments);
        }
    }
}

/// Attach a trailing comment to a node's `trailing_comment` field.
pub(super) fn attach_trailing_comment(node: &mut Node<Span>, comment: String) {
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
            .trailing_comment = Some(comment);
        }
        Node::Alias {
            trailing_comment, ..
        } => {
            *trailing_comment = Some(comment);
        }
    }
}

/// Deep-copy a node without any leading or trailing comment, on it or inside
/// it.  The anchor and the tag locations are kept.
///
/// The anchor map in resolved mode holds such a copy, so every expansion of an
/// alias copies the anchor's nodes only, never its comments.  Cloning the node
/// first and removing the comments afterwards would make every expansion cost
/// as much as the anchor's comments.
pub(super) fn clone_without_comments(node: &Node<Span>) -> Node<Span> {
    match node {
        Node::Scalar {
            value,
            style,
            tag,
            loc,
            meta,
        } => Node::Scalar {
            value: value.clone(),
            style: *style,
            tag: tag.clone(),
            loc: *loc,
            meta: meta_without_comments(meta.as_deref()),
        },
        Node::Mapping {
            entries,
            style,
            tag,
            loc,
            meta,
        } => Node::Mapping {
            entries: entries
                .iter()
                .map(|(key, value)| (clone_without_comments(key), clone_without_comments(value)))
                .collect(),
            style: *style,
            tag: tag.clone(),
            loc: *loc,
            meta: meta_without_comments(meta.as_deref()),
        },
        Node::Sequence {
            items,
            style,
            tag,
            loc,
            meta,
        } => Node::Sequence {
            items: items.iter().map(clone_without_comments).collect(),
            style: *style,
            tag: tag.clone(),
            loc: *loc,
            meta: meta_without_comments(meta.as_deref()),
        },
        Node::Alias { name, loc, .. } => Node::Alias {
            name: name.clone(),
            loc: *loc,
            leading_comments: None,
            trailing_comment: None,
        },
    }
}

fn meta_without_comments(meta: Option<&NodeMeta<Span>>) -> Option<Box<NodeMeta<Span>>> {
    meta.and_then(|m| {
        NodeMeta {
            anchor: m.anchor.clone(),
            anchor_loc: m.anchor_loc,
            tag_loc: m.tag_loc,
            leading_comments: None,
            trailing_comment: None,
        }
        .into_option()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CollectionStyle, ScalarStyle};
    use crate::pos::Span;

    fn zero_span() -> Span {
        Span { start: 0, end: 0 }
    }

    fn scalar_node() -> Node<Span> {
        Node::Scalar {
            value: String::new(),
            style: ScalarStyle::Plain,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    fn mapping_node() -> Node<Span> {
        Node::Mapping {
            entries: Vec::new(),
            style: CollectionStyle::Block,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    fn sequence_node() -> Node<Span> {
        Node::Sequence {
            items: Vec::new(),
            style: CollectionStyle::Block,
            tag: None,
            loc: zero_span(),
            meta: None,
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
        attach_leading_comments(&mut node, vec!["# existing".to_owned()]);
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
        attach_leading_comments(&mut node, vec!["# old".to_owned()]);
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
        attach_trailing_comment(&mut node, "# old".to_owned());
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

    // clone_without_comments tests

    fn commented(mut node: Node<Span>) -> Node<Span> {
        attach_leading_comments(&mut node, vec!["# lead".to_owned()]);
        attach_trailing_comment(&mut node, "# trail".to_owned());
        node
    }

    #[test]
    fn clone_without_comments_drops_both_comment_kinds_of_a_scalar() {
        let copy = clone_without_comments(&commented(scalar_node()));
        assert!(copy.leading_comments().is_empty());
        assert_eq!(copy.trailing_comment(), None);
    }

    #[test]
    fn clone_without_comments_drops_a_meta_that_holds_only_comments() {
        let copy = clone_without_comments(&commented(mapping_node()));
        assert!(matches!(copy, Node::Mapping { meta: None, .. }));
    }

    #[test]
    fn clone_without_comments_keeps_the_anchor_and_tag_location() {
        let mut node = commented(sequence_node());
        let tag_loc = Span { start: 3, end: 5 };
        if let Node::Sequence { meta, .. } = &mut node {
            let meta = meta.as_mut().unwrap();
            meta.anchor = Some("x".to_owned());
            meta.tag_loc = Some(tag_loc);
        }
        let copy = clone_without_comments(&node);
        assert_eq!(copy.anchor(), Some("x"));
        assert_eq!(copy.tag_loc(), Some(tag_loc));
        assert_eq!(copy.trailing_comment(), None);
    }

    #[test]
    fn clone_without_comments_drops_comments_of_alias() {
        let copy = clone_without_comments(&commented(alias_node()));
        assert!(copy.leading_comments().is_empty());
        assert_eq!(copy.trailing_comment(), None);
    }

    #[test]
    fn clone_without_comments_reaches_nested_nodes_and_keeps_the_original() {
        let node = Node::Mapping {
            entries: vec![(
                commented(scalar_node()),
                Node::Sequence {
                    items: vec![commented(scalar_node())],
                    style: crate::event::CollectionStyle::Block,
                    tag: None,
                    loc: zero_span(),
                    meta: None,
                },
            )],
            style: crate::event::CollectionStyle::Block,
            tag: None,
            loc: zero_span(),
            meta: None,
        };
        let copy = clone_without_comments(&node);
        let Node::Mapping { entries, .. } = &copy else {
            panic!("expected mapping");
        };
        let Some((key, Node::Sequence { items, .. })) = entries.first() else {
            panic!("expected one entry with a sequence value");
        };
        assert_eq!(key.trailing_comment(), None);
        assert_eq!(items.first().and_then(Node::trailing_comment), None);
        let Node::Mapping { entries, .. } = &node else {
            panic!("expected mapping");
        };
        assert_eq!(
            entries.first().and_then(|(key, _)| key.trailing_comment()),
            Some("# trail")
        );
    }
}
