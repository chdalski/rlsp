// SPDX-License-Identifier: MIT

//! YAML AST node types.
//!
//! [`Node<Loc>`] is the core type — a YAML value parameterized by its
//! location type.  For most uses `Loc = Span`.  The loader produces
//! `Vec<Document<Span>>`.

use crate::event::ScalarStyle;
use crate::pos::Span;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A YAML document: a root node plus directive metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Document<Loc = Span> {
    /// The root node of the document.
    pub root: Node<Loc>,
    /// YAML version from `%YAML` directive, if present.
    pub version: Option<(u8, u8)>,
    /// Tag handle/prefix pairs from `%TAG` directives.
    pub tags: Vec<(String, String)>,
    /// Comments that appear at document level (before or between nodes).
    pub comments: Vec<String>,
}

/// A YAML node parameterized by its location type.
#[derive(Debug, Clone, PartialEq)]
pub enum Node<Loc = Span> {
    /// A scalar value.
    Scalar {
        value: String,
        style: ScalarStyle,
        anchor: Option<String>,
        tag: Option<String>,
        loc: Loc,
    },
    /// A mapping (sequence of key–value pairs preserving declaration order).
    Mapping {
        entries: Vec<(Self, Self)>,
        anchor: Option<String>,
        tag: Option<String>,
        loc: Loc,
    },
    /// A sequence (ordered list of nodes).
    Sequence {
        items: Vec<Self>,
        anchor: Option<String>,
        tag: Option<String>,
        loc: Loc,
    },
    /// An alias reference (lossless mode only — resolved mode expands these).
    Alias { name: String, loc: Loc },
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
}
