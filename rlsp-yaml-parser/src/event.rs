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

use std::borrow::Cow;

use crate::pos::Span;

/// Rare per-event fields for node-typed events (`Scalar`, `SequenceStart`, `MappingStart`).
///
/// Bundled behind `Option<Box<EventMeta>>` so that the common case — no anchor, no
/// source-text tag — pays only one 8-byte pointer instead of ~96 bytes of inline storage.
/// Events with tags and anchors are rare in block-heavy and Kubernetes YAML; boxing them
/// moves the cost to the uncommon path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventMeta<'input> {
    /// The anchor name, if any (e.g. `&foo`).
    pub anchor: Option<&'input str>,
    /// Source span of the `&name` anchor token — from `&` through the last byte of the name.
    /// `Some` when `anchor` is `Some`, `None` otherwise.
    pub anchor_loc: Option<Span>,
    /// The resolved tag, if any (e.g. `"tag:yaml.org,2002:str"` for `!!str`).
    ///
    /// Verbatim tags (`!<URI>`) borrow from input.  Shorthand tags resolved via `%TAG`
    /// directives or the built-in `!!` default produce owned strings.
    pub tag: Option<Cow<'input, str>>,
    /// Source span of the tag token — from `!` through the last byte of the tag token.
    /// `Some` when `tag` is `Some`, `None` otherwise.
    pub tag_loc: Option<Span>,
}

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
/// (`[a, b]`) and flow mappings (`{a: b}`) are implemented in Task 14.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionStyle {
    /// A block-style collection using indentation and `-`/`:` indicators.
    Block,
    /// A flow-style collection using `[]` or `{}` delimiters (Task 14).
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
    /// A YAML comment (YAML 1.2 §6.6).
    ///
    /// `text` is the comment body — the content of the line after the `#`
    /// character, with the `#` itself excluded.  Leading whitespace after `#`
    /// is preserved (e.g. `# hello` → text `" hello"`; `#nospace` → text
    /// `"nospace"`).  The associated span covers from the `#` character
    /// through the last byte of comment text (the newline is not included).
    ///
    /// One `Comment` event is emitted per physical line.
    Comment {
        /// Comment body (everything after the `#`, excluding the newline).
        text: &'input str,
    },
    /// An alias node (`*name`) that references a previously anchored node.
    ///
    /// The associated span covers the entire `*name` token (from `*` through
    /// the last character of the name).  Resolution of the alias to its
    /// anchored node is the loader's responsibility (Task 20) — the parser
    /// emits this event without expansion.
    Alias {
        /// The anchor name being referenced (e.g. `"foo"` for `*foo`).
        /// Borrowed directly from input — no allocation.
        name: &'input str,
    },
    /// A document has started.
    ///
    /// `explicit` is `true` when the document was introduced with `---`.
    /// `false` for bare documents (no marker).
    DocumentStart {
        /// Whether the document was introduced with `---`.
        explicit: bool,
        /// Version from the `%YAML` directive preceding this document, if any.
        ///
        /// `Some((1, 2))` for `%YAML 1.2`, `None` when no `%YAML` directive was present.
        version: Option<(u8, u8)>,
        /// Tag handle/prefix pairs from `%TAG` directives preceding this document.
        ///
        /// Each entry is `(handle, prefix)` — e.g. `("!foo!", "tag:example.com,2026:")`.
        /// Empty when no `%TAG` directives were present.
        tag_directives: Vec<(String, String)>,
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
    SequenceStart {
        /// Whether this is a block (`-` indicator) or flow (`[...]`) sequence.
        style: CollectionStyle,
        /// Rare fields: `anchor`, `anchor_loc`, `tag`, `tag_loc`.
        /// `None` when no anchor or source-text tag is present (the common case).
        meta: Option<Box<EventMeta<'input>>>,
    },
    /// A sequence has ended.
    ///
    /// Matches the most recent [`Event::SequenceStart`] on the event stack.
    SequenceEnd,
    /// A block or flow mapping has started.
    ///
    /// Followed by alternating key/value node events (scalars or nested
    /// collections), then a matching [`Event::MappingEnd`].
    MappingStart {
        /// Whether this is a block (indentation-based) or flow (`{...}`) mapping.
        style: CollectionStyle,
        /// Rare fields: `anchor`, `anchor_loc`, `tag`, `tag_loc`.
        /// `None` when no anchor or source-text tag is present (the common case).
        meta: Option<Box<EventMeta<'input>>>,
    },
    /// A mapping has ended.
    ///
    /// Matches the most recent [`Event::MappingStart`] on the event stack.
    MappingEnd,
    /// A scalar value.
    ///
    /// `value` borrows from input when no transformation is required (the
    /// vast majority of plain scalars).  It owns when line folding produces
    /// a string that doesn't exist contiguously in the input.
    Scalar {
        /// The scalar's decoded value.
        value: Cow<'input, str>,
        /// The style in which the scalar appeared in the source.
        style: ScalarStyle,
        /// Rare fields: `anchor`, `anchor_loc`, `tag`, `tag_loc`.
        /// `None` when no anchor or source-text tag is present (the common case).
        meta: Option<Box<EventMeta<'input>>>,
    },
}

impl Event<'_> {
    /// Returns the anchor name if this event defines one.
    #[must_use]
    #[inline]
    pub fn anchor(&self) -> Option<&str> {
        match self {
            Self::Scalar { meta, .. }
            | Self::SequenceStart { meta, .. }
            | Self::MappingStart { meta, .. } => meta.as_ref().and_then(|m| m.anchor),
            Self::StreamStart
            | Self::StreamEnd
            | Self::Comment { .. }
            | Self::Alias { .. }
            | Self::DocumentStart { .. }
            | Self::DocumentEnd { .. }
            | Self::SequenceEnd
            | Self::MappingEnd => None,
        }
    }

    /// Returns the source span of the `&name` anchor token, if any.
    #[must_use]
    #[inline]
    pub fn anchor_loc(&self) -> Option<Span> {
        match self {
            Self::Scalar { meta, .. }
            | Self::SequenceStart { meta, .. }
            | Self::MappingStart { meta, .. } => meta.as_ref().and_then(|m| m.anchor_loc),
            Self::StreamStart
            | Self::StreamEnd
            | Self::Comment { .. }
            | Self::Alias { .. }
            | Self::DocumentStart { .. }
            | Self::DocumentEnd { .. }
            | Self::SequenceEnd
            | Self::MappingEnd => None,
        }
    }

    /// Returns the resolved tag string, if any.
    #[must_use]
    #[inline]
    pub fn tag(&self) -> Option<&str> {
        match self {
            Self::Scalar { meta, .. }
            | Self::SequenceStart { meta, .. }
            | Self::MappingStart { meta, .. } => meta.as_ref().and_then(|m| m.tag.as_deref()),
            Self::StreamStart
            | Self::StreamEnd
            | Self::Comment { .. }
            | Self::Alias { .. }
            | Self::DocumentStart { .. }
            | Self::DocumentEnd { .. }
            | Self::SequenceEnd
            | Self::MappingEnd => None,
        }
    }

    /// Returns the source span of the tag token, if any.
    #[must_use]
    #[inline]
    pub fn tag_loc(&self) -> Option<Span> {
        match self {
            Self::Scalar { meta, .. }
            | Self::SequenceStart { meta, .. }
            | Self::MappingStart { meta, .. } => meta.as_ref().and_then(|m| m.tag_loc),
            Self::StreamStart
            | Self::StreamEnd
            | Self::Comment { .. }
            | Self::Alias { .. }
            | Self::DocumentStart { .. }
            | Self::DocumentEnd { .. }
            | Self::SequenceEnd
            | Self::MappingEnd => None,
        }
    }
}

/// Build an `EventMeta` box when at least one field is `Some`.
///
/// Returns `None` when all four fields are `None` (the common case).
#[expect(
    clippy::redundant_pub_crate,
    reason = "pub(crate) inside private module — accessibility requires crate-wide visibility"
)]
#[inline]
pub(crate) fn make_meta<'input>(
    anchor: Option<&'input str>,
    anchor_loc: Option<Span>,
    tag: Option<Cow<'input, str>>,
    tag_loc: Option<Span>,
) -> Option<Box<EventMeta<'input>>> {
    if anchor.is_none() && tag.is_none() {
        None
    } else {
        Some(Box::new(EventMeta {
            anchor,
            anchor_loc,
            tag,
            tag_loc,
        }))
    }
}

const _: () = assert!(
    std::mem::size_of::<Event<'_>>() <= 56,
    "Event must be at most 56 bytes after EventMeta boxing"
);

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::pos::Span;

    const SPAN: Span = Span { start: 0, end: 4 };

    // EM-1: meta is None when all four fields are absent.
    #[test]
    fn make_meta_returns_none_when_all_fields_absent() {
        let meta = make_meta(None, None, None, None);
        assert!(
            meta.is_none(),
            "make_meta must return None when anchor and tag are both None"
        );
    }

    // EM-2: meta is Some when only anchor is present.
    #[test]
    fn make_meta_returns_some_when_anchor_only() {
        let meta = make_meta(Some("a"), Some(SPAN), None, None).unwrap();
        assert_eq!(meta.anchor, Some("a"));
        assert_eq!(meta.anchor_loc, Some(SPAN));
        assert!(meta.tag.is_none());
        assert!(meta.tag_loc.is_none());
    }

    // EM-3: meta is Some when only tag is present.
    #[test]
    fn make_meta_returns_some_when_tag_only() {
        let meta = make_meta(None, None, Some(Cow::Borrowed("!str")), Some(SPAN)).unwrap();
        assert!(meta.anchor.is_none());
        assert!(meta.anchor_loc.is_none());
        assert_eq!(meta.tag.as_deref(), Some("!str"));
        assert_eq!(meta.tag_loc, Some(SPAN));
    }

    // EM-4: meta is Some when both anchor and tag are present.
    #[test]
    fn make_meta_returns_some_when_both_anchor_and_tag() {
        let meta = make_meta(
            Some("a"),
            Some(SPAN),
            Some(Cow::Borrowed("!str")),
            Some(SPAN),
        )
        .unwrap();
        assert_eq!(meta.anchor, Some("a"));
        assert_eq!(meta.tag.as_deref(), Some("!str"));
    }

    // EM-5: Event size at or below 56 bytes.
    #[test]
    fn event_size_at_most_56_bytes() {
        assert!(
            std::mem::size_of::<Event<'_>>() <= 56,
            "Event size {} exceeds 56 bytes",
            std::mem::size_of::<Event<'_>>()
        );
    }
}
