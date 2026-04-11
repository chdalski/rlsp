// SPDX-License-Identifier: MIT

use crate::error::Error;
use crate::event::{Event, ScalarStyle};
use crate::pos::{Pos, Span};

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
    /// `StreamEnd` emitted; done.
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

/// Whether the next expected token in a flow mapping is a key or value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowMappingPhase {
    /// Expecting the next key (or the closing `}`).
    Key,
    /// Expecting the value after a key has been consumed.
    Value,
}

impl CollectionEntry {
    /// The indentation column of this collection's indicator/key.
    pub const fn indent(self) -> usize {
        match self {
            Self::Sequence(col, _) | Self::Mapping(col, _, _) => col,
        }
    }
}

/// Result of consuming a mapping-entry line.
pub enum ConsumedMapping<'input> {
    /// Explicit key (`? key`).
    ExplicitKey {
        /// Whether there was key content on the same line as `?`.
        had_key_inline: bool,
    },
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
