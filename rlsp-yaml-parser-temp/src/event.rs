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
//! - `MappingStart / MappingEnd` (Task 12+)
//! - `Alias { name: Cow<'input, str> }` (Task 15+)

use std::borrow::Cow;

/// Block scalar chomping mode per YAML 1.2 §8.1.1.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chomp {
    /// `-` — trailing newlines stripped.
    Strip,
    /// (default, no indicator) — single trailing newline kept.
    Clip,
    /// `+` — all trailing newlines kept.
    Keep,
}

/// The style (block or flow) of a collection (sequence or mapping).
///
/// Currently only `Block` is produced; `Flow` will be used when flow sequences
/// (`[a, b]`) and flow mappings (`{a: b}`) are implemented in Tasks 14/12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionStyle {
    /// A block-style collection using indentation and `-`/`:` indicators.
    Block,
    /// A flow-style collection using `[]` or `{}` delimiters (Task 14/12).
    Flow,
}

/// The style in which a scalar value was written in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarStyle {
    /// An unquoted plain scalar (YAML 1.2 §7.3.3).
    Plain,
    /// A `'single-quoted'` scalar (YAML 1.2 §7.3.2).
    SingleQuoted,
    /// A `"double-quoted"` scalar (YAML 1.2 §7.3.1).
    DoubleQuoted,
    /// A `|` literal block scalar (YAML 1.2 §8.1.2).
    Literal(Chomp),
    /// A `>` folded block scalar (YAML 1.2 §8.1.3).
    ///
    /// Line folding is applied to the collected content: a single line break
    /// between two equally-indented non-blank lines becomes a space; N blank
    /// lines between non-blank lines produce N newlines; more-indented lines
    /// preserve their relative leading whitespace and the line break before
    /// them is kept as `\n` rather than folded to a space.  Callers must not
    /// treat the value as whitespace-safe — more-indented lines can inject
    /// arbitrary leading spaces into the parsed value.
    Folded(Chomp),
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
    /// A block or flow sequence has started.
    ///
    /// Followed by zero or more node events (scalars or nested collections),
    /// then a matching [`Event::SequenceEnd`].
    ///
    /// `anchor` and `tag` are `None` until Tasks 15/16.
    SequenceStart {
        /// The anchor name, if any (e.g. `&foo`).  Populated in Task 15.
        anchor: Option<&'input str>,
        /// The tag, if any (e.g. `!!seq`).  Populated in Task 16.
        tag: Option<&'input str>,
        /// Whether this is a block (`-` indicator) or flow (`[...]`) sequence.
        style: CollectionStyle,
    },
    /// A sequence has ended.
    ///
    /// Matches the most recent [`Event::SequenceStart`] on the event stack.
    SequenceEnd,
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
