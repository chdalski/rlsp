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
            tag_loc,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Scalar {
            value,
            style,
            anchor,
            anchor_loc,
            tag,
            tag_loc,
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
            tag_loc,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Mapping {
            entries,
            style,
            anchor,
            anchor_loc,
            tag,
            tag_loc,
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
            tag_loc,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Sequence {
            items,
            style,
            anchor,
            anchor_loc,
            tag,
            tag_loc,
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
    use std::borrow::Cow;

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
            tag_loc: None,
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
            tag: Some(Cow::Owned("!t".to_owned())),
            tag_loc: Some(span(6)),
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
                tag_loc,
                loc,
                leading_comments,
                trailing_comment,
            } => {
                assert_eq!(loc, span(2));
                assert_eq!(anchor_loc, Some(span(5)), "anchor_loc must be preserved");
                assert_eq!(tag_loc, Some(span(6)), "tag_loc must be preserved");
                assert_eq!(value, "hello");
                assert_eq!(style, ScalarStyle::Plain);
                assert_eq!(anchor, Some("a".to_owned()));
                assert_eq!(tag.as_deref(), Some("!t"));
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
            tag: Some(Cow::Owned("!m".to_owned())),
            tag_loc: Some(span(6)),
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
                assert_eq!(tag.as_deref(), Some("!m"));
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
            tag_loc: None,
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
            tag_loc: None,
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
            tag_loc: None,
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
            tag_loc: None,
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
            tag_loc: None,
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

    // TL-RELOC: reloc_tag_loc_some_preserved
    #[test]
    fn reloc_tag_loc_some_preserved() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!t".to_owned())),
            tag_loc: Some(span(5)),
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(2));
        match result {
            Node::Scalar { tag_loc, loc, .. } => {
                assert_eq!(loc, span(2));
                assert_eq!(tag_loc, Some(span(5)), "tag_loc must be preserved");
            }
            _ => panic!("expected Scalar"),
        }
    }

    // TL-RELOC: reloc_tag_loc_none_preserved
    #[test]
    fn reloc_tag_loc_none_preserved() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(99));
        match result {
            Node::Scalar { tag_loc, loc, .. } => {
                assert_eq!(loc, span(99));
                assert_eq!(tag_loc, None, "None tag_loc must remain None");
            }
            _ => panic!("expected Scalar"),
        }
    }

    // TL-RELOC: reloc_mapping_tag_loc_preserved
    #[test]
    fn reloc_mapping_tag_loc_preserved() {
        let node = Node::Mapping {
            entries: vec![],
            style: CollectionStyle::Block,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!m".to_owned())),
            tag_loc: Some(span(5)),
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(3));
        match result {
            Node::Mapping { tag_loc, loc, .. } => {
                assert_eq!(loc, span(3));
                assert_eq!(tag_loc, Some(span(5)), "tag_loc must be preserved");
            }
            _ => panic!("expected Mapping"),
        }
    }

    // TL-RELOC: reloc_sequence_tag_loc_preserved
    #[test]
    fn reloc_sequence_tag_loc_preserved() {
        let node = Node::Sequence {
            items: vec![],
            style: CollectionStyle::Block,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!s".to_owned())),
            tag_loc: Some(span(5)),
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(4));
        match result {
            Node::Sequence { tag_loc, loc, .. } => {
                assert_eq!(loc, span(4));
                assert_eq!(tag_loc, Some(span(5)), "tag_loc must be preserved");
            }
            _ => panic!("expected Sequence"),
        }
    }

    // -----------------------------------------------------------------------
    // COW-RELOC: reloc preserves Cow variant identity
    // -----------------------------------------------------------------------

    // COW-RELOC-1: reloc preserves Cow::Borrowed tag
    #[test]
    fn reloc_preserves_borrowed_tag() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Borrowed("tag:yaml.org,2002:str")),
            tag_loc: None,
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(2));
        match result {
            Node::Scalar { tag, .. } => {
                assert!(
                    matches!(tag, Some(Cow::Borrowed(_))),
                    "reloc must not reallocate a Borrowed tag"
                );
            }
            _ => panic!("expected Scalar"),
        }
    }

    // COW-RELOC-2: reloc preserves Cow::Owned tag
    #[test]
    fn reloc_preserves_owned_tag() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!custom".to_owned())),
            tag_loc: None,
            loc: span(1),
            leading_comments: None,
            trailing_comment: None,
        };
        let result = reloc(node, span(2));
        match result {
            Node::Scalar { tag, .. } => {
                assert!(
                    matches!(tag, Some(Cow::Owned(_))),
                    "reloc must preserve an Owned tag as Owned"
                );
            }
            _ => panic!("expected Scalar"),
        }
    }
}
