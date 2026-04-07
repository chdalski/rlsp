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
//! - `DocumentStart { explicit: bool, version: Option<(u8, u8)>, tags: Vec<...> }` (Task 5)
//! - `DocumentEnd { explicit: bool }` (Task 5)
//! - `Scalar { value: Cow<'input, str>, style: ScalarStyle, anchor: Option<...>, tag: Option<...> }` (Task 6+)
//! - `MappingStart / MappingEnd` (Task 9+)
//! - `SequenceStart / SequenceEnd` (Task 9+)
//! - `Alias { name: Cow<'input, str> }` (Task 12+)
//!
//! The `'input` lifetime parameter is present now so that adding
//! `Cow<'input, str>` fields in later tasks does not break the public API.

/// A high-level YAML parse event.
///
/// Parameterized by `'input` so that future scalar variants can borrow
/// directly from the input string via `Cow<'input, str>` without requiring
/// an API-breaking change.
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

    // Suppress the "unused lifetime" warning until scalar variants land.
    #[doc(hidden)]
    _Phantom(std::marker::PhantomData<&'input ()>),
}
