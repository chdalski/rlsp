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
            anchor_loc,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Scalar {
            value,
            style,
            anchor,
            anchor_loc,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Mapping {
            entries,
            style,
            anchor,
            anchor_loc,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Mapping {
            entries,
            style,
            anchor,
            anchor_loc,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Sequence {
            items,
            style,
            anchor,
            anchor_loc,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Sequence {
            items,
            style,
            anchor,
            anchor_loc,
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

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "test code"
)]
mod tests {
    use super::*;
    use crate::event::{CollectionStyle, ScalarStyle};
    use crate::pos::Pos;

    fn span(line: usize) -> Span {
        let p = Pos {
            byte_offset: 0,
            line,
            column: 0,
        };
        Span { start: p, end: p }
    }

    fn plain_scalar(loc: Span) -> Node<Span> {
        Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            loc,
            leading_comments: None,
            trailing_comment: None,
        }
    }

    #[test]
    fn reloc_scalar_replaces_loc() {
        let node = Node::Scalar {
            value: "hello".to_owned(),
            style: ScalarStyle::Plain,
            anchor: Some("a".to_owned()),
            anchor_loc: Some(span(5)),
            tag: Some("!t".to_owned()),
            loc: span(1),
            leading_comments: Some(vec!["# lc".to_owned()]),
            trailing_comment: Some("# tc".to_owned()),
        };
        let result = reloc(node, span(2));
        match result {
            Node::Scalar {
                value,
                style,
                anchor,
                anchor_loc,
                tag,
                loc,
                leading_comments,
                trailing_comment,
            } => {
                assert_eq!(loc, span(2));
                assert_eq!(anchor_loc, Some(span(5)), "anchor_loc must be preserved");
                assert_eq!(value, "hello");
                assert_eq!(style, ScalarStyle::Plain);
                assert_eq!(anchor, Some("a".to_owned()));
                assert_eq!(tag, Some("!t".to_owned()));
                assert_eq!(leading_comments, Some(vec!["# lc".to_owned()]));
                assert_eq!(trailing_comment, Some("# tc".to_owned()));
            }
            _ => panic!("expected Scalar"),
        }
    }

    #[test]
    fn reloc_mapping_replaces_loc() {
        let node = Node::Mapping {
            entries: vec![],
            style: CollectionStyle::Block,
            anchor: Some("m".to_owned()),
            anchor_loc: Some(span(5)),
            tag: Some("!m".to_owned()),
            loc: span(1),
            leading_comments: Some(vec!["# lc".to_owned()]),
            trailing_comment: Some("# tc".to_owned()),
        };
        let result = reloc(node, span(3));
        match result {
            Node::Mapping {
                entries,
                anchor,
                anchor_loc,
                tag,
                loc,
                leading_comments,
                trailing_comment,
                ..
            } => {
                assert_eq!(loc, span(3));
                assert_eq!(anchor_loc, Some(span(5)), "anchor_loc must be preserved");
                assert!(entries.is_empty());
                assert_eq!(anchor, Some("m".to_owned()));
                assert_eq!(tag, Some("!m".to_owned()));
                assert_eq!(leading_comments, Some(vec!["# lc".to_owned()]));
                assert_eq!(trailing_comment, Some("# tc".to_owned()));
            }
            _ => panic!("expected Mapping"),
        }
    }

    #[test]
    fn reloc_sequence_replaces_loc() {
        let node = Node::Sequence {
            items: vec![],
            style: CollectionStyle::Block,
            anchor: Some("s".to_owned()),
            anchor_loc: Some(span(5)),
            tag: None,
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(4));
        match result {
            Node::Sequence {
                items,
                loc,
                anchor_loc,
                ..
            } => {
                assert_eq!(loc, span(4));
                assert_eq!(anchor_loc, Some(span(5)), "anchor_loc must be preserved");
                assert!(items.is_empty());
            }
            _ => panic!("expected Sequence"),
        }
    }

    // reloc_anchor_loc_none_preserved: None anchor_loc stays None after reloc
    #[test]
    fn reloc_anchor_loc_none_preserved() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(99));
        match result {
            Node::Scalar {
                anchor_loc, loc, ..
            } => {
                assert_eq!(loc, span(99));
                assert_eq!(anchor_loc, None, "None anchor_loc must remain None");
            }
            _ => panic!("expected Scalar"),
        }
    }

    #[test]
    fn reloc_alias_replaces_loc() {
        let node = Node::Alias {
            name: "x".to_owned(),
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(5));
        match result {
            Node::Alias { name, loc, .. } => {
                assert_eq!(loc, span(5));
                assert_eq!(name, "x");
            }
            _ => panic!("expected Alias"),
        }
    }

    #[test]
    fn reloc_preserves_leading_comments() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            loc: span(1),
            leading_comments: Some(vec!["# hi".to_owned()]),
            trailing_comment: None,
        };
        let result = reloc(node, span(2));
        assert_eq!(result.leading_comments(), &["# hi"]);
    }

    #[test]
    fn reloc_preserves_trailing_comment() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            loc: span(1),
            leading_comments: None,
            trailing_comment: Some("# tail".to_owned()),
        };
        let result = reloc(node, span(2));
        assert_eq!(result.trailing_comment(), Some("# tail"));
    }

    // reloc is shallow: only the top-level loc is replaced; child locs are unchanged.
    #[test]
    fn reloc_mapping_with_entries_only_replaces_top_loc() {
        let node = Node::Mapping {
            entries: vec![(plain_scalar(span(10)), plain_scalar(span(10)))],
            style: CollectionStyle::Block,
            anchor: None,
            anchor_loc: None,
            tag: None,
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(99));
        match result {
            Node::Mapping { entries, loc, .. } => {
                assert_eq!(loc, span(99));
                let (k, v) = &entries[0];
                match k {
                    Node::Scalar { loc: child_loc, .. } => assert_eq!(*child_loc, span(10)),
                    _ => panic!("expected Scalar key"),
                }
                match v {
                    Node::Scalar { loc: child_loc, .. } => assert_eq!(*child_loc, span(10)),
                    _ => panic!("expected Scalar value"),
                }
            }
            _ => panic!("expected Mapping"),
        }
    }
}
