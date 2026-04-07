// SPDX-License-Identifier: MIT

//! High-level parse events produced by the streaming parser.
//!
//! The public entry point is [`crate::parse_events`], which returns an
//! iterator of <code>Result<([Event], [crate::pos::Span]), [crate::error::Error]></code>.
//!
//! Each event carries a [`crate::pos::Span`] covering the input bytes that
//! contributed to it.  For zero-width synthetic events (e.g. `StreamStart`
//! at the very beginning of input), the span has equal `start` and `end`.
//!
//! # Planned variants
//!
//! Future tasks will add:
//! - `DocumentStart { explicit: bool, version: Option<(u8, u8)>, tags: Vec<...> }` (Task 18)
//! - `MappingStart / MappingEnd` (Task 10+)
//! - `SequenceStart / SequenceEnd` (Task 10+)
//! - `Alias { name: Cow<'input, str> }` (Task 15+)

use std::borrow::Cow;

/// The style in which a scalar value was written in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarStyle {
    /// An unquoted plain scalar (YAML 1.2 §7.3.3).
    Plain,
    /// A `'single-quoted'` scalar (YAML 1.2 §7.3.2).
    SingleQuoted,
    /// A `"double-quoted"` scalar (YAML 1.2 §7.3.1).
    DoubleQuoted,
    // Literal(Chomp) (Task 8), Folded(Chomp) (Task 9) added in their tasks.
}

/// A high-level YAML parse event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<'input> {
    /// The YAML stream has started.
    ///
    /// Always the first event in any parse.  The associated span is a
    /// zero-width span at [`crate::pos::Pos::ORIGIN`].
    StreamStart,
    /// The YAML stream has ended.
    ///
    /// Always the last event in any parse.  The associated span is a
    /// zero-width span at the position immediately after the last byte of
    /// input.
    StreamEnd,
    /// A document has started.
    ///
    /// `explicit` is `true` when the document was introduced with `---`.
    /// `false` for bare documents (no marker).
    ///
    /// Note: `version` and `tags` (from `%YAML` / `%TAG` directives) are
    /// deferred to Task 18.
    DocumentStart {
        /// Whether the document was introduced with `---`.
        explicit: bool,
    },
    /// A document has ended.
    ///
    /// `explicit` is `true` when the document was closed with `...`.
    /// `false` for implicitly-ended documents.
    DocumentEnd {
        /// Whether the document was closed with `...`.
        explicit: bool,
    },
    /// A scalar value.
    ///
    /// `value` borrows from input when no transformation is required (the
    /// vast majority of plain scalars).  It owns when line folding produces
    /// a string that doesn't exist contiguously in the input.
    ///
    /// `anchor` and `tag` are `None` until Tasks 15/16 implement anchor and
    /// tag tokenization respectively.
    Scalar {
        /// The scalar's decoded value.
        value: Cow<'input, str>,
        /// The style in which the scalar appeared in the source.
        style: ScalarStyle,
        /// The anchor name, if any (e.g. `&foo`).  Populated in Task 15.
        anchor: Option<&'input str>,
        /// The tag, if any (e.g. `!!str`).  Populated in Task 16.
        tag: Option<&'input str>,
    },
}
