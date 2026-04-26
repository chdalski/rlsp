// SPDX-License-Identifier: MIT

//! YAML AST node types.
//!
//! [`Node<Loc>`] is the core type — a YAML value parameterized by its
//! location type.  For most uses `Loc = Span`.  The loader produces
//! `Vec<Document<Span>>`.

use std::borrow::Cow;

use crate::event::{CollectionStyle, ScalarStyle};
use crate::pos::Span;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A YAML document: a root node plus directive metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Document<Loc = Span> {
    /// The root node of the document.
    pub root: Node<Loc>,
    /// YAML version declared by a `%YAML` directive, if present (e.g. `(1, 2)`).
    pub version: Option<(u8, u8)>,
    /// Tag handle/prefix pairs declared by `%TAG` directives (handle, prefix).
    pub tags: Vec<(String, String)>,
    /// Comments that appear at document level (before or between nodes).
    pub comments: Vec<String>,
    /// Whether the document was introduced with an explicit `---` marker.
    pub explicit_start: bool,
    /// Whether the document was closed with an explicit `...` marker.
    pub explicit_end: bool,
}

/// A YAML node parameterized by its location type.
#[derive(Debug, Clone, PartialEq)]
pub enum Node<Loc = Span> {
    /// A scalar value.
    Scalar {
        /// The scalar content as a UTF-8 string (after block/flow unfolding).
        value: String,
        /// The presentation style used in the source (plain, single-quoted, etc.).
        style: ScalarStyle,
        /// Anchor name defined on this node (e.g. `&anchor`), if any.
        anchor: Option<String>,
        /// Source span of the `&name` anchor token — from `&` through the last byte of the
        /// name.  `Some` when `anchor` is `Some`; `None` otherwise.
        anchor_loc: Option<Loc>,
        /// Tag applied to this node (e.g. `!!str`), if any.
        tag: Option<Cow<'static, str>>,
        /// Source span of the tag token — from `!` through the last byte of the tag.
        /// `Some` when `tag` is `Some`; `None` otherwise.
        tag_loc: Option<Loc>,
        /// Source span covering this scalar in the input.
        loc: Loc,
        /// Comment lines that appear before this node (e.g. `# note`).
        /// Populated only for non-first entries in a mapping or sequence.
        /// Document-prefix leading comments are discarded by the tokenizer
        /// per YAML §9.2 and cannot be recovered here.
        leading_comments: Option<Vec<String>>,
        /// Inline comment on the same line as this node (e.g. `# note`).
        trailing_comment: Option<String>,
    },
    /// A mapping (sequence of key–value pairs preserving declaration order).
    Mapping {
        /// Key–value pairs in declaration order.
        entries: Vec<(Self, Self)>,
        /// The presentation style used in the source (block or flow).
        style: CollectionStyle,
        /// Anchor name defined on this mapping (e.g. `&anchor`), if any.
        anchor: Option<String>,
        /// Source span of the `&name` anchor token — from `&` through the last byte of the
        /// name.  `Some` when `anchor` is `Some`; `None` otherwise.
        anchor_loc: Option<Loc>,
        /// Tag applied to this mapping (e.g. `!!map`), if any.
        tag: Option<Cow<'static, str>>,
        /// Source span of the tag token — from `!` through the last byte of the tag.
        /// `Some` when `tag` is `Some`; `None` otherwise.
        tag_loc: Option<Loc>,
        /// Source span from the opening indicator to the last entry.
        loc: Loc,
        /// Comment lines that appear before this node.
        leading_comments: Option<Vec<String>>,
        /// Inline comment on the same line as this node.
        trailing_comment: Option<String>,
    },
    /// A sequence (ordered list of nodes).
    Sequence {
        /// Ordered list of child nodes.
        items: Vec<Self>,
        /// The presentation style used in the source (block or flow).
        style: CollectionStyle,
        /// Anchor name defined on this sequence (e.g. `&anchor`), if any.
        anchor: Option<String>,
        /// Source span of the `&name` anchor token — from `&` through the last byte of the
        /// name.  `Some` when `anchor` is `Some`; `None` otherwise.
        anchor_loc: Option<Loc>,
        /// Tag applied to this sequence (e.g. `!!seq`), if any.
        tag: Option<Cow<'static, str>>,
        /// Source span of the tag token — from `!` through the last byte of the tag.
        /// `Some` when `tag` is `Some`; `None` otherwise.
        tag_loc: Option<Loc>,
        /// Source span from the opening indicator to the last item.
        loc: Loc,
        /// Comment lines that appear before this node.
        leading_comments: Option<Vec<String>>,
        /// Inline comment on the same line as this node.
        trailing_comment: Option<String>,
    },
    /// An alias reference (lossless mode only — resolved mode expands these).
    Alias {
        /// The anchor name this alias refers to (without the `*` sigil).
        name: String,
        /// Source span covering the `*name` alias token.
        loc: Loc,
        /// Comment lines that appear before this node.
        leading_comments: Option<Vec<String>>,
        /// Inline comment on the same line as this node.
        trailing_comment: Option<String>,
    },
}

impl<Loc> Node<Loc> {
    /// Returns the anchor name if this node defines one.
    pub fn anchor(&self) -> Option<&str> {
        match self {
            Self::Scalar { anchor, .. }
            | Self::Mapping { anchor, .. }
            | Self::Sequence { anchor, .. } => anchor.as_deref(),
            Self::Alias { .. } => None,
        }
    }

    /// Returns the source span of the `&name` anchor token, if any.
    ///
    /// `Some(span)` when `anchor()` is `Some`; `None` otherwise.
    /// Always `None` for [`Node::Alias`] — the alias span is in `loc`.
    pub const fn anchor_loc(&self) -> Option<Loc>
    where
        Loc: Copy,
    {
        match self {
            Self::Scalar { anchor_loc, .. }
            | Self::Mapping { anchor_loc, .. }
            | Self::Sequence { anchor_loc, .. } => *anchor_loc,
            Self::Alias { .. } => None,
        }
    }

    /// Returns the source span of the tag token, if any.
    ///
    /// `Some(span)` when `tag()` is `Some`; `None` otherwise.
    /// Always `None` for [`Node::Alias`].
    pub const fn tag_loc(&self) -> Option<Loc>
    where
        Loc: Copy,
    {
        match self {
            Self::Scalar { tag_loc, .. }
            | Self::Mapping { tag_loc, .. }
            | Self::Sequence { tag_loc, .. } => *tag_loc,
            Self::Alias { .. } => None,
        }
    }

    /// Returns the leading comments for this node.
    pub fn leading_comments(&self) -> &[String] {
        match self {
            Self::Scalar {
                leading_comments, ..
            }
            | Self::Mapping {
                leading_comments, ..
            }
            | Self::Sequence {
                leading_comments, ..
            }
            | Self::Alias {
                leading_comments, ..
            } => leading_comments.as_deref().unwrap_or(&[]),
        }
    }

    /// Returns the trailing comment for this node, if any.
    pub fn trailing_comment(&self) -> Option<&str> {
        match self {
            Self::Scalar {
                trailing_comment, ..
            }
            | Self::Mapping {
                trailing_comment, ..
            }
            | Self::Sequence {
                trailing_comment, ..
            }
            | Self::Alias {
                trailing_comment, ..
            } => trailing_comment.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::event::{CollectionStyle, ScalarStyle};
    use crate::pos::{Pos, Span};

    fn zero_span() -> Span {
        Span {
            start: Pos::ORIGIN,
            end: Pos::ORIGIN,
        }
    }

    fn plain_scalar(value: &str) -> Node<Span> {
        Node::Scalar {
            value: value.to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        }
    }

    // NF-1: node_debug_includes_leading_comments
    #[test]
    fn node_debug_includes_leading_comments() {
        let node = Node::Scalar {
            value: "val".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: Some(vec!["# note".to_owned()]),
            trailing_comment: None,
        };
        let debug = format!("{node:?}");
        assert!(debug.contains("# note"), "debug output: {debug}");
    }

    // NF-2: node_partial_eq_considers_leading_comments
    #[test]
    fn node_partial_eq_considers_leading_comments() {
        let a = Node::Scalar {
            value: "val".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: Some(vec!["# a".to_owned()]),
            trailing_comment: None,
        };
        let b = Node::Scalar {
            value: "val".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: Some(vec!["# b".to_owned()]),
            trailing_comment: None,
        };
        assert_ne!(a, b);
    }

    // NF-3: node_clone_preserves_comments
    #[test]
    fn node_clone_preserves_comments() {
        let node = Node::Scalar {
            value: "val".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: Some(vec!["# x".to_owned()]),
            trailing_comment: Some("# y".to_owned()),
        };
        let cloned = node.clone();
        assert_eq!(node, cloned);
        assert_eq!(cloned.leading_comments(), &["# x"]);
        assert_eq!(cloned.trailing_comment(), Some("# y"));
    }

    // Sanity: plain_scalar helper produces empty comment fields.
    #[test]
    fn plain_scalar_has_empty_comments() {
        let n = plain_scalar("hello");
        assert!(n.leading_comments().is_empty());
        assert!(n.trailing_comment().is_none());
    }

    #[test]
    fn node_accessor_returns_empty_slice_for_none() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.leading_comments(), &[] as &[String]);
    }

    #[test]
    fn node_accessor_returns_slice_for_some() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: Some(vec!["# x".to_owned()]),
            trailing_comment: None,
        };
        assert_eq!(node.leading_comments(), &["# x"]);
    }

    fn bare_document(explicit_start: bool, explicit_end: bool) -> Document<Span> {
        Document {
            root: plain_scalar("val"),
            version: None,
            tags: Vec::new(),
            comments: Vec::new(),
            explicit_start,
            explicit_end,
        }
    }

    // NF-DOC-1: explicit_start and explicit_end default to false
    #[test]
    fn document_explicit_flags_in_equality() {
        let a = bare_document(false, false);
        let b = bare_document(false, false);
        assert_eq!(a, b);
    }

    // NF-DOC-2: PartialEq distinguishes differing explicit_start
    #[test]
    fn document_partial_eq_distinguishes_explicit_start() {
        let a = bare_document(true, false);
        let b = bare_document(false, false);
        assert_ne!(a, b);
    }

    // NF-DOC-3: PartialEq distinguishes differing explicit_end
    #[test]
    fn document_partial_eq_distinguishes_explicit_end() {
        let a = bare_document(false, true);
        let b = bare_document(false, false);
        assert_ne!(a, b);
    }

    // NF-DOC-4: Clone preserves both flags
    #[test]
    fn document_clone_preserves_explicit_flags() {
        let doc = bare_document(true, true);
        let cloned = doc.clone();
        assert_eq!(doc, cloned);
        assert!(cloned.explicit_start);
        assert!(cloned.explicit_end);
    }

    // -----------------------------------------------------------------------
    // AL-NODE: anchor_loc() accessor
    // -----------------------------------------------------------------------

    // AL-NODE-1: anchor_loc_accessor_returns_some_for_anchored_scalar
    #[test]
    fn anchor_loc_accessor_returns_some_for_anchored_scalar() {
        let span = zero_span();
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: Some("a".to_owned()),
            anchor_loc: Some(span),
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.anchor_loc(), Some(span));
    }

    // AL-NODE-2: anchor_loc_accessor_returns_none_for_unanchored_scalar
    #[test]
    fn anchor_loc_accessor_returns_none_for_unanchored_scalar() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.anchor_loc(), None);
    }

    // AL-NODE-3: anchor_loc_accessor_returns_none_for_alias
    #[test]
    fn anchor_loc_accessor_returns_none_for_alias() {
        let node = Node::Alias {
            name: "x".to_owned(),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.anchor_loc(), None);
    }

    // AL-NODE-4: anchor_loc_accessor_returns_some_for_anchored_mapping
    #[test]
    fn anchor_loc_accessor_returns_some_for_anchored_mapping() {
        let span = zero_span();
        let node = Node::Mapping {
            entries: vec![],
            style: CollectionStyle::Block,
            anchor: Some("m".to_owned()),
            anchor_loc: Some(span),
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.anchor_loc(), Some(span));
    }

    // AL-NODE-5: anchor_loc_accessor_returns_some_for_anchored_sequence
    #[test]
    fn anchor_loc_accessor_returns_some_for_anchored_sequence() {
        let span = zero_span();
        let node = Node::Sequence {
            items: vec![],
            style: CollectionStyle::Block,
            anchor: Some("s".to_owned()),
            anchor_loc: Some(span),
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.anchor_loc(), Some(span));
    }

    // -----------------------------------------------------------------------
    // TL-NODE: tag_loc() accessor
    // -----------------------------------------------------------------------

    // TL-NODE-1: tag_loc_accessor_returns_some_for_tagged_scalar
    #[test]
    fn tag_loc_accessor_returns_some_for_tagged_scalar() {
        let span = zero_span();
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!t".to_owned())),
            tag_loc: Some(span),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.tag_loc(), Some(span));
    }

    // TL-NODE-2: tag_loc_accessor_returns_none_for_untagged_scalar
    #[test]
    fn tag_loc_accessor_returns_none_for_untagged_scalar() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: None,
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.tag_loc(), None);
    }

    // TL-NODE-3: tag_loc_accessor_returns_none_for_alias
    #[test]
    fn tag_loc_accessor_returns_none_for_alias() {
        let node = Node::Alias {
            name: "x".to_owned(),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.tag_loc(), None);
    }

    // TL-NODE-4: tag_loc_accessor_returns_some_for_tagged_mapping
    #[test]
    fn tag_loc_accessor_returns_some_for_tagged_mapping() {
        let span = zero_span();
        let node = Node::Mapping {
            entries: vec![],
            style: CollectionStyle::Block,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!!map".to_owned())),
            tag_loc: Some(span),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.tag_loc(), Some(span));
    }

    // TL-NODE-5: tag_loc_accessor_returns_some_for_tagged_sequence
    #[test]
    fn tag_loc_accessor_returns_some_for_tagged_sequence() {
        let span = zero_span();
        let node = Node::Sequence {
            items: vec![],
            style: CollectionStyle::Block,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!!seq".to_owned())),
            tag_loc: Some(span),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.tag_loc(), Some(span));
    }

    // -----------------------------------------------------------------------
    // COW-NODE: Cow variant construction, equality, and clone
    // -----------------------------------------------------------------------

    // COW-NODE-1: node construction with Cow::Borrowed tag compiles and round-trips
    #[test]
    fn node_construction_with_borrowed_tag() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Borrowed("tag:yaml.org,2002:str")),
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.tag_loc(), None);
        if let Node::Scalar { tag, .. } = &node {
            assert!(matches!(tag, Some(Cow::Borrowed(_))));
        }
    }

    // COW-NODE-2: node construction with Cow::Owned tag compiles and round-trips
    #[test]
    fn node_construction_with_owned_tag() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("!custom".to_owned())),
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(node.tag_loc(), None);
        if let Node::Scalar { tag, .. } = &node {
            assert_eq!(tag.as_deref(), Some("!custom"));
        }
    }

    // COW-NODE-3: Borrowed and Owned with the same content compare equal
    #[test]
    fn node_partial_eq_borrowed_vs_owned_same_content() {
        let borrowed = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Borrowed("tag:yaml.org,2002:str")),
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        let owned = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Owned("tag:yaml.org,2002:str".to_owned())),
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        assert_eq!(borrowed, owned);
    }

    // COW-NODE-4: clone of Cow::Borrowed tag stays Borrowed
    #[test]
    fn node_clone_preserves_cow_variant() {
        let node = Node::Scalar {
            value: "v".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            anchor_loc: None,
            tag: Some(Cow::Borrowed("tag:yaml.org,2002:str")),
            tag_loc: None,
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        // The tag field is Copy-compatible via Cow::clone. Verify the cloned
        // Scalar's tag is still Borrowed by re-constructing from the original.
        let cloned_tag = if let Node::Scalar { tag, .. } = &node {
            tag.clone()
        } else {
            unreachable!()
        };
        assert!(
            matches!(cloned_tag, Some(Cow::Borrowed(_))),
            "cloned Borrowed tag must remain Borrowed"
        );
    }
}
