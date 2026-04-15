// SPDX-License-Identifier: MIT

//! YAML AST node types.
//!
//! [`Node<Loc>`] is the core type — a YAML value parameterized by its
//! location type.  For most uses `Loc = Span`.  The loader produces
//! `Vec<Document<Span>>`.

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
        /// Tag applied to this node (e.g. `!!str`), if any.
        tag: Option<String>,
        /// Source span covering this scalar in the input.
        loc: Loc,
        /// Comment lines that appear before this node (e.g. `# note`).
        /// Populated only for non-first entries in a mapping or sequence.
        /// Document-prefix leading comments are discarded by the tokenizer
        /// per YAML §9.2 and cannot be recovered here.
        leading_comments: Vec<String>,
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
        /// Tag applied to this mapping (e.g. `!!map`), if any.
        tag: Option<String>,
        /// Source span from the opening indicator to the last entry.
        loc: Loc,
        /// Comment lines that appear before this node.
        leading_comments: Vec<String>,
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
        /// Tag applied to this sequence (e.g. `!!seq`), if any.
        tag: Option<String>,
        /// Source span from the opening indicator to the last item.
        loc: Loc,
        /// Comment lines that appear before this node.
        leading_comments: Vec<String>,
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
        leading_comments: Vec<String>,
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
            } => leading_comments,
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
    use super::*;
    use crate::event::ScalarStyle;
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
            tag: None,
            loc: zero_span(),
            leading_comments: Vec::new(),
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
            tag: None,
            loc: zero_span(),
            leading_comments: vec!["# note".to_owned()],
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
            tag: None,
            loc: zero_span(),
            leading_comments: vec!["# a".to_owned()],
            trailing_comment: None,
        };
        let b = Node::Scalar {
            value: "val".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: zero_span(),
            leading_comments: vec!["# b".to_owned()],
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
            tag: None,
            loc: zero_span(),
            leading_comments: vec!["# x".to_owned()],
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
}
