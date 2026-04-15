// SPDX-License-Identifier: MIT

use crate::error::Error;
use crate::event::{Event, ScalarStyle};
use crate::pos::{Pos, Span};
use std::borrow::Cow;

/// Outcome of one state-machine step inside [`crate::EventIter::next`].
pub enum StepResult<'input> {
    /// The step pushed to `queue` or changed state; loop again to drain.
    Continue,
    /// The step produced an event or error to return immediately.
    Yield(Result<(Event<'input>, Span), Error>),
}

/// State of the top-level event iterator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterState {
    /// About to emit `StreamStart`.
    BeforeStream,
    /// Between documents: skip blanks/comments/directives, detect next document.
    BetweenDocs,
    /// Inside a document: consume lines until a boundary marker or EOF.
    InDocument,
    /// `StreamEnd` emitted or an error was yielded; iteration is finished.
    Done,
}

/// What the state machine expects next for an open mapping entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingPhase {
    /// The next node is a key (first half of a pair).
    Key,
    /// The next node is a value (second half of a pair).
    Value,
}

/// An entry on the collection stack, tracking open block sequences and mappings.
///
/// Flow collections are fully parsed by [`crate::EventIter::handle_flow_collection`]
/// before returning; they never leave an entry on this stack.  The combined
/// depth limit (block + flow) is enforced inside `handle_flow_collection` by
/// summing `coll_stack.len()` with the local flow-frame count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionEntry {
    /// An open block sequence.  Holds the column of its `-` indicator and
    /// whether at least one complete item has been delivered.  `has_had_item`
    /// is `false` for a freshly opened sequence and becomes `true` once a
    /// complete item (scalar or sub-collection) has been emitted.  Used by
    /// `handle_sequence_entry` to detect a `-` at the wrong indentation level.
    Sequence(usize, bool),
    /// An open block mapping.  Holds the column of its first key, the
    /// current phase (expecting key or value), and whether the mapping has
    /// had at least one key advanced to the value phase (`has_had_value`).
    /// `has_had_value` is `false` for a freshly opened mapping and becomes
    /// `true` the first time `advance_mapping_to_value` is called on it.
    /// The wrong-indentation check in `handle_mapping_entry` uses this flag
    /// to avoid false positives on explicit-key content nodes (e.g. V9D5).
    Mapping(usize, MappingPhase, bool),
}

impl CollectionEntry {
    /// The indentation column of this collection's indicator/key.
    pub const fn indent(self) -> usize {
        match self {
            Self::Sequence(col, _) | Self::Mapping(col, _, _) => col,
        }
    }
}

/// Whether the next expected token in a flow mapping is a key or value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowMappingPhase {
    /// Expecting the next key (or the closing `}`).
    Key,
    /// Expecting the value after a key has been consumed.
    Value,
}

/// Disposition of a pending anchor before it's attached to a node event.
///
/// Anchors in YAML precede the node they annotate. After scanning `&name`,
/// the parser stores the name here until the next `Scalar`, `SequenceStart`,
/// or `MappingStart` event is emitted. The disposition determines which
/// node the anchor annotates:
///
/// - `Standalone`: anchor was on its own line (`&name\n- item`) — annotates
///   the next node regardless of type (collection or scalar).
/// - `Inline`: anchor was inline with key content (`&name key: value`) —
///   annotates the key scalar, not the enclosing mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAnchor<'input> {
    Standalone(&'input str),
    Inline(&'input str),
}

impl<'input> PendingAnchor<'input> {
    pub const fn name(self) -> &'input str {
        match self {
            Self::Standalone(n) | Self::Inline(n) => n,
        }
    }
}

/// Disposition of a pending tag before it's attached to a node event.
///
/// Parallel to [`PendingAnchor`] but for tags. Tags in YAML precede the
/// node they annotate. The disposition determines which node the tag
/// annotates:
///
/// - `Standalone`: tag was on its own line (`!!seq\n- item`) — annotates
///   the next node regardless of type (collection or scalar).
/// - `Inline`: tag was inline with key content (`!!str key: value`) —
///   annotates the key scalar, not the enclosing mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingTag<'input> {
    Standalone(Cow<'input, str>),
    Inline(Cow<'input, str>),
}

impl<'input> PendingTag<'input> {
    pub fn into_cow(self) -> Cow<'input, str> {
        match self {
            Self::Standalone(c) | Self::Inline(c) => c,
        }
    }
}

/// Result of consuming a mapping-entry line.
pub enum ConsumedMapping<'input> {
    /// Explicit key (`? key`).
    ExplicitKey,
    /// Implicit key (`key: value`).
    ///
    /// The key content and span are pre-extracted so the caller can push the
    /// key `Scalar` event directly without routing it through
    /// `try_consume_plain_scalar` — which would treat the adjacent value
    /// synthetic line as a plain-scalar continuation.
    ImplicitKey {
        /// The decoded key value (may be owned if escapes were resolved).
        key_value: std::borrow::Cow<'input, str>,
        /// The scalar style of the key (`Plain`, `SingleQuoted`, or `DoubleQuoted`).
        key_style: ScalarStyle,
        /// Span covering the key text (including quotes if quoted).
        key_span: Span,
    },
    /// The inline value of an implicit key itself contained a value indicator,
    /// making it an illegal inline block mapping (e.g. `a: b: c` or `a: 'b': c`).
    /// The error position points to the start of the inline value content.
    InlineImplicitMappingError { pos: Pos },
    /// A quoted implicit key could not be decoded (e.g. bad escape sequence).
    QuotedKeyError { pos: Pos, message: String },
}

#[cfg(test)]
mod tests {
    use super::{PendingAnchor, PendingTag};
    use std::borrow::Cow;

    // A-1: Standalone variant carries the anchor name.
    #[test]
    fn pending_anchor_standalone_carries_name() {
        let a = PendingAnchor::Standalone("myanchor");
        assert_eq!(a.name(), "myanchor");
        assert!(matches!(a, PendingAnchor::Standalone("myanchor")));
    }

    // A-2: Inline variant carries the anchor name.
    #[test]
    fn pending_anchor_inline_carries_name() {
        let a = PendingAnchor::Inline("inlineanchor");
        assert_eq!(a.name(), "inlineanchor");
        assert!(matches!(a, PendingAnchor::Inline("inlineanchor")));
    }

    // A-3: None::<PendingAnchor> matches the None arm (documents cleared state).
    #[test]
    fn pending_anchor_none_matches_none_arm() {
        let a: Option<PendingAnchor<'_>> = None;
        assert!(a.is_none());
        assert!(!matches!(a, Some(PendingAnchor::Standalone(_))));
        assert!(!matches!(a, Some(PendingAnchor::Inline(_))));
    }

    // T-1: Standalone variant carries the tag string.
    #[test]
    fn pending_tag_standalone_carries_value() {
        let tag = PendingTag::Standalone(Cow::Borrowed("tag:yaml.org,2002:str"));
        assert!(matches!(&tag, PendingTag::Standalone(c) if c.as_ref() == "tag:yaml.org,2002:str"));
        assert_eq!(tag.into_cow().as_ref(), "tag:yaml.org,2002:str");
    }

    // T-2: Inline variant carries the tag string.
    #[test]
    fn pending_tag_inline_carries_value() {
        let tag = PendingTag::Inline(Cow::Borrowed("!mytag"));
        assert!(matches!(&tag, PendingTag::Inline(c) if c.as_ref() == "!mytag"));
        assert_eq!(tag.into_cow().as_ref(), "!mytag");
    }
}
