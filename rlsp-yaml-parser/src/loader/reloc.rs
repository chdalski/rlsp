// SPDX-License-Identifier: MIT

use crate::node::Node;
use crate::pos::Span;

/// Replace the location of a node (used when stamping alias-site spans).
pub(super) fn reloc(node: Node<Span>, loc: Span) -> Node<Span> {
    match node {
        Node::Scalar {
            value,
            style,
            tag,
            meta,
            ..
        } => Node::Scalar {
            value,
            style,
            tag,
            loc,
            meta,
        },
        Node::Mapping {
            entries,
            style,
            tag,
            meta,
            ..
        } => Node::Mapping {
            entries,
            style,
            tag,
            loc,
            meta,
        },
        Node::Sequence {
            items,
            style,
            tag,
            meta,
            ..
        } => Node::Sequence {
            items,
            style,
            tag,
            loc,
            meta,
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
    clippy::expect_used,
    reason = "test code"
)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::event::{CollectionStyle, ScalarStyle};
    use crate::node::NodeMeta;
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
            tag: None,
            loc,
            meta: None,
        }
    }

    #[test]
    fn reloc_scalar_replaces_loc() {
        let node = Node::Scalar {
            value: "hello".to_owned(),
            style: ScalarStyle::Plain,
            tag: Some(Cow::Owned("!t".to_owned())),
            loc: span(1),
            meta: NodeMeta {
                anchor: Some("a".to_owned()),
                anchor_loc: Some(span(5)),
                tag_loc: Some(span(6)),
                leading_comments: Some(vec!["# lc".to_owned()]),
                trailing_comment: Some("# tc".to_owned()),
            }
            .into_option(),
        };
        let result = reloc(node, span(2));
        match result {
            Node::Scalar {
                value,
                style,
                tag,
                loc,
                meta,
            } => {
                assert_eq!(loc, span(2));
                let m = meta.as_deref().expect("meta must be Some");
                assert_eq!(m.anchor_loc, Some(span(5)), "anchor_loc must be preserved");
                assert_eq!(m.tag_loc, Some(span(6)), "tag_loc must be preserved");
                assert_eq!(value, "hello");
                assert_eq!(style, ScalarStyle::Plain);
                assert_eq!(m.anchor, Some("a".to_owned()));
                assert_eq!(tag.as_deref(), Some("!t"));
                assert_eq!(m.leading_comments, Some(vec!["# lc".to_owned()]));
                assert_eq!(m.trailing_comment, Some("# tc".to_owned()));
            }
            _ => panic!("expected Scalar"),
        }
    }

    #[test]
    fn reloc_mapping_replaces_loc() {
        let node = Node::Mapping {
            entries: vec![],
            style: CollectionStyle::Block,
            tag: Some(Cow::Owned("!m".to_owned())),
            loc: span(1),
            meta: NodeMeta {
                anchor: Some("m".to_owned()),
                anchor_loc: Some(span(5)),
                tag_loc: Some(span(6)),
                leading_comments: Some(vec!["# lc".to_owned()]),
                trailing_comment: Some("# tc".to_owned()),
            }
            .into_option(),
        };
        let result = reloc(node, span(3));
        match result {
            Node::Mapping {
                entries,
                tag,
                loc,
                meta,
                ..
            } => {
                assert_eq!(loc, span(3));
                let m = meta.as_deref().expect("meta must be Some");
                assert_eq!(m.anchor_loc, Some(span(5)), "anchor_loc must be preserved");
                assert!(entries.is_empty());
                assert_eq!(m.anchor, Some("m".to_owned()));
                assert_eq!(tag.as_deref(), Some("!m"));
                assert_eq!(m.leading_comments, Some(vec!["# lc".to_owned()]));
                assert_eq!(m.trailing_comment, Some("# tc".to_owned()));
            }
            _ => panic!("expected Mapping"),
        }
    }

    #[test]
    fn reloc_sequence_replaces_loc() {
        let node = Node::Sequence {
            items: vec![],
            style: CollectionStyle::Block,
            tag: None,
            loc: span(1),
            meta: NodeMeta {
                anchor: Some("s".to_owned()),
                anchor_loc: Some(span(5)),
                tag_loc: None,
                leading_comments: None,
                trailing_comment: None,
            }
            .into_option(),
        };
        let result = reloc(node, span(4));
        match result {
            Node::Sequence {
                items, loc, meta, ..
            } => {
                assert_eq!(loc, span(4));
                let m = meta.as_deref().expect("meta must be Some");
                assert_eq!(m.anchor_loc, Some(span(5)), "anchor_loc must be preserved");
                assert!(items.is_empty());
            }
            _ => panic!("expected Sequence"),
        }
    }

    // reloc_anchor_loc_none_preserved: None anchor_loc stays None after reloc
    #[test]
    fn reloc_anchor_loc_none_preserved() {
        let node = plain_scalar(span(1));
        let result = reloc(node, span(99));
        match result {
            Node::Scalar { meta, loc, .. } => {
                assert_eq!(loc, span(99));
                assert!(
                    meta.is_none(),
                    "None anchor_loc must remain None (meta stays None)"
                );
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
            tag: None,
            loc: span(1),
            meta: NodeMeta {
                anchor: None,
                anchor_loc: None,
                tag_loc: None,
                leading_comments: Some(vec!["# hi".to_owned()]),
                trailing_comment: None,
            }
            .into_option(),
        };
        let result = reloc(node, span(2));
        assert_eq!(result.leading_comments(), &["# hi"]);
    }

    #[test]
    fn reloc_preserves_trailing_comment() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            tag: None,
            loc: span(1),
            meta: NodeMeta {
                anchor: None,
                anchor_loc: None,
                tag_loc: None,
                leading_comments: None,
                trailing_comment: Some("# tail".to_owned()),
            }
            .into_option(),
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
            tag: None,
            loc: span(1),
            meta: None,
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
            tag: Some(Cow::Owned("!t".to_owned())),
            loc: span(1),
            meta: NodeMeta {
                anchor: None,
                anchor_loc: None,
                tag_loc: Some(span(5)),
                leading_comments: None,
                trailing_comment: None,
            }
            .into_option(),
        };
        let result = reloc(node, span(2));
        match result {
            Node::Scalar { meta, loc, .. } => {
                assert_eq!(loc, span(2));
                let m = meta.as_deref().expect("meta must be Some");
                assert_eq!(m.tag_loc, Some(span(5)), "tag_loc must be preserved");
            }
            _ => panic!("expected Scalar"),
        }
    }

    // TL-RELOC: reloc_tag_loc_none_preserved
    #[test]
    fn reloc_tag_loc_none_preserved() {
        let node = plain_scalar(span(1));
        let result = reloc(node, span(99));
        match result {
            Node::Scalar { meta, loc, .. } => {
                assert_eq!(loc, span(99));
                assert!(
                    meta.is_none(),
                    "None tag_loc must remain None (meta stays None)"
                );
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
            tag: Some(Cow::Owned("!m".to_owned())),
            loc: span(1),
            meta: NodeMeta {
                anchor: None,
                anchor_loc: None,
                tag_loc: Some(span(5)),
                leading_comments: None,
                trailing_comment: None,
            }
            .into_option(),
        };
        let result = reloc(node, span(3));
        match result {
            Node::Mapping { meta, loc, .. } => {
                assert_eq!(loc, span(3));
                let m = meta.as_deref().expect("meta must be Some");
                assert_eq!(m.tag_loc, Some(span(5)), "tag_loc must be preserved");
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
            tag: Some(Cow::Owned("!s".to_owned())),
            loc: span(1),
            meta: NodeMeta {
                anchor: None,
                anchor_loc: None,
                tag_loc: Some(span(5)),
                leading_comments: None,
                trailing_comment: None,
            }
            .into_option(),
        };
        let result = reloc(node, span(4));
        match result {
            Node::Sequence { meta, loc, .. } => {
                assert_eq!(loc, span(4));
                let m = meta.as_deref().expect("meta must be Some");
                assert_eq!(m.tag_loc, Some(span(5)), "tag_loc must be preserved");
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
            tag: Some(Cow::Borrowed("tag:yaml.org,2002:str")),
            loc: span(1),
            meta: None,
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
            tag: Some(Cow::Owned("!custom".to_owned())),
            loc: span(1),
            meta: None,
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

    // -----------------------------------------------------------------------
    // RELOC-META: NodeMeta boxing preserved by reloc
    // -----------------------------------------------------------------------

    // RELOC-META-1: reloc_scalar_meta_none_preserved
    #[test]
    fn reloc_scalar_meta_none_preserved() {
        let node = plain_scalar(span(1));
        let result = reloc(node, span(42));
        match result {
            Node::Scalar { meta, loc, .. } => {
                assert_eq!(loc, span(42));
                assert!(meta.is_none(), "meta must remain None after reloc");
            }
            _ => panic!("expected Scalar"),
        }
    }

    // RELOC-META-2: reloc_scalar_meta_some_preserved
    #[test]
    fn reloc_scalar_meta_some_preserved() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            tag: None,
            loc: span(1),
            meta: NodeMeta {
                anchor: Some("a".to_owned()),
                anchor_loc: None,
                tag_loc: None,
                leading_comments: None,
                trailing_comment: None,
            }
            .into_option(),
        };
        let result = reloc(node, span(5));
        assert_eq!(result.anchor(), Some("a"), "anchor must survive reloc");
        match result {
            Node::Scalar { loc, .. } => assert_eq!(loc, span(5)),
            _ => panic!("expected Scalar"),
        }
    }
}
