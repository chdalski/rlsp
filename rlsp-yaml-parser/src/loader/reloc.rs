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

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "test code"
)]
mod tests {
    use super::*;
    use crate::event::ScalarStyle;
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
            tag: None,
            loc,
            leading_comments: Vec::new(),
            trailing_comment: None,
        }
    }

    #[test]
    fn reloc_scalar_replaces_loc() {
        let node = Node::Scalar {
            value: "hello".to_owned(),
            style: ScalarStyle::Plain,
            anchor: Some("a".to_owned()),
            tag: Some("!t".to_owned()),
            loc: span(1),
            leading_comments: vec!["# lc".to_owned()],
            trailing_comment: Some("# tc".to_owned()),
        };
        let result = reloc(node, span(2));
        match result {
            Node::Scalar {
                value,
                style,
                anchor,
                tag,
                loc,
                leading_comments,
                trailing_comment,
            } => {
                assert_eq!(loc, span(2));
                assert_eq!(value, "hello");
                assert_eq!(style, ScalarStyle::Plain);
                assert_eq!(anchor, Some("a".to_owned()));
                assert_eq!(tag, Some("!t".to_owned()));
                assert_eq!(leading_comments, vec!["# lc".to_owned()]);
                assert_eq!(trailing_comment, Some("# tc".to_owned()));
            }
            _ => panic!("expected Scalar"),
        }
    }

    #[test]
    fn reloc_mapping_replaces_loc() {
        let node = Node::Mapping {
            entries: vec![],
            anchor: Some("m".to_owned()),
            tag: Some("!m".to_owned()),
            loc: span(1),
            leading_comments: vec!["# lc".to_owned()],
            trailing_comment: Some("# tc".to_owned()),
        };
        let result = reloc(node, span(3));
        match result {
            Node::Mapping {
                entries,
                anchor,
                tag,
                loc,
                leading_comments,
                trailing_comment,
            } => {
                assert_eq!(loc, span(3));
                assert!(entries.is_empty());
                assert_eq!(anchor, Some("m".to_owned()));
                assert_eq!(tag, Some("!m".to_owned()));
                assert_eq!(leading_comments, vec!["# lc".to_owned()]);
                assert_eq!(trailing_comment, Some("# tc".to_owned()));
            }
            _ => panic!("expected Mapping"),
        }
    }

    #[test]
    fn reloc_sequence_replaces_loc() {
        let node = Node::Sequence {
            items: vec![],
            anchor: None,
            tag: None,
            loc: span(1),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = reloc(node, span(4));
        match result {
            Node::Sequence { items, loc, .. } => {
                assert_eq!(loc, span(4));
                assert!(items.is_empty());
            }
            _ => panic!("expected Sequence"),
        }
    }

    #[test]
    fn reloc_alias_replaces_loc() {
        let node = Node::Alias {
            name: "x".to_owned(),
            loc: span(1),
            leading_comments: Vec::new(),
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
            tag: None,
            loc: span(1),
            leading_comments: vec!["# hi".to_owned()],
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
            tag: None,
            loc: span(1),
            leading_comments: Vec::new(),
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
            anchor: None,
            tag: None,
            loc: span(1),
            leading_comments: Vec::new(),
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
