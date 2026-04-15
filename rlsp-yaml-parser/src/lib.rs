// SPDX-License-Identifier: MIT

//! A spec-faithful streaming YAML 1.2 parser.
//!
//! Use [`parse_events`] for a lazy event stream, or the [`loader`] module to
//! build a full AST.

mod chars;
/// Encoding detection and UTF-8 decoding for YAML byte streams.
pub mod encoding;
mod error;
mod event;
mod event_iter;
pub(crate) mod lexer;
/// Security limit constants for the parser and loader.
pub mod limits;
mod lines;
/// Event-to-AST loader that builds a `Vec<Document<Span>>`.
pub mod loader;
pub mod node;
mod pos;
pub use error::Error;
pub use event::{Chomp, CollectionStyle, Event, ScalarStyle};
pub use lines::{BreakType, Line, LineBuffer};
pub use loader::{LoadError, LoadMode, Loader, LoaderBuilder, LoaderOptions, load};
pub use node::{Document, Node};
pub use pos::{Pos, Span};

pub use limits::{
    MAX_ANCHOR_NAME_BYTES, MAX_COLLECTION_DEPTH, MAX_COMMENT_LEN, MAX_DIRECTIVES_PER_DOC,
    MAX_RESOLVED_TAG_LEN, MAX_TAG_HANDLE_BYTES, MAX_TAG_LEN,
};
use std::collections::VecDeque;

use event_iter::{CollectionEntry, DirectiveScope, IterState, PendingAnchor, PendingTag};

use lexer::Lexer;

/// Parse a YAML string into a lazy event stream.
///
/// The iterator yields <code>Result<([Event], [Span]), [Error]></code> items.
/// The first event is always [`Event::StreamStart`] and the last is always
/// [`Event::StreamEnd`].
///
/// # Example
///
/// ```
/// use rlsp_yaml_parser::{parse_events, Event};
///
/// let events: Vec<_> = parse_events("").collect();
/// assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
/// assert!(matches!(events.last(), Some(Ok((Event::StreamEnd, _)))));
/// ```
pub fn parse_events(input: &str) -> impl Iterator<Item = Result<(Event<'_>, Span), Error>> + '_ {
    EventIter::new(input)
}

// ---------------------------------------------------------------------------
// Iterator implementation
// ---------------------------------------------------------------------------

/// Lazy iterator that yields events by walking a [`Lexer`].
struct EventIter<'input> {
    lexer: Lexer<'input>,
    state: IterState,
    /// Queued events to emit before resuming normal state dispatch.
    ///
    /// Used when a single parse step must produce multiple consecutive events —
    /// e.g. `SequenceStart` before the first item, or multiple close events
    /// when a dedent closes several nested collections at once.
    queue: VecDeque<(Event<'input>, Span)>,
    /// Stack of open block collections (sequences and mappings).
    ///
    /// Each entry records whether the open collection is a sequence or a
    /// mapping, its indentation column, and (for mappings) whether the next
    /// expected node is a key or a value.  The combined length of this stack
    /// is bounded by [`MAX_COLLECTION_DEPTH`].
    coll_stack: Vec<CollectionEntry>,
    /// A pending anchor that has been scanned but not yet attached to a node
    /// event.  The [`PendingAnchor`] variant encodes both the anchor name and
    /// whether it was standalone (applies to the next node of any type) or
    /// inline (applies to the key scalar, not the enclosing mapping).
    pending_anchor: Option<PendingAnchor<'input>>,
    /// A pending tag that has been scanned but not yet attached to a node event.
    ///
    /// Tags in YAML precede the node they annotate (YAML 1.2 §6.8.1).  After
    /// scanning `!tag`, `!!tag`, `!<uri>`, or `!`, the parser stores the tag
    /// here and attaches it to the next `Scalar`, `SequenceStart`, or
    /// `MappingStart` event.
    ///
    /// Tags are resolved against the current directive scope at scan time:
    /// - `!<URI>`  → stored as `Cow::Borrowed("URI")` (verbatim, no change)
    /// - `!!suffix` → resolved via `!!` handle (default: `tag:yaml.org,2002:suffix`)
    /// - `!suffix` → stored as `Cow::Borrowed("!suffix")` (local tag, no expansion)
    /// - `!`       → stored as `Cow::Borrowed("!")`
    /// - `!handle!suffix` → resolved via `%TAG !handle! prefix` directive
    ///
    /// The [`PendingTag`] variant encodes both the resolved tag string and
    /// whether it was standalone (applies to the next node of any type) or
    /// inline (applies to the key scalar, not the enclosing mapping).
    pending_tag: Option<PendingTag<'input>>,
    /// Directive scope for the current document.
    ///
    /// Accumulated from `%YAML` and `%TAG` directives seen in `BetweenDocs`
    /// state.  Reset at document boundaries.
    directive_scope: DirectiveScope,
    /// Set to `true` once the root node of the current document has been
    /// fully emitted (a scalar at the top level, or a collection after its
    /// closing event empties `coll_stack`).
    ///
    /// Used to detect invalid extra content after the document root, such as
    /// `foo:\n  bar\ninvalid` where `invalid` appears after the root mapping
    /// closes.  Reset to `false` at each document boundary.
    root_node_emitted: bool,
    /// Set to `true` after consuming a `? ` explicit key indicator whose key
    /// content will appear on the NEXT line (i.e., `had_key_inline = false`).
    /// Cleared when the key content is processed.
    ///
    /// Used to allow a block sequence indicator on a line following `? ` to be
    /// treated as the explicit key's content rather than triggering the
    /// "invalid block sequence entry" guard.
    explicit_key_pending: bool,
    /// When `Some(indent)`, a `? inline-content` explicit key was consumed for the
    /// mapping at column `indent`.  The inline content opens (or is) a complex node
    /// (sub-mapping or sub-sequence), so the outer mapping stays in Key phase after
    /// the complex node closes.  The stored indent distinguishes the outer mapping
    /// from any inner mappings that advance to Value phase while the flag is active.
    /// Used to allow the subsequent `:` value-indicator line to be recognised as
    /// the explicit value indicator (rather than as an implicit empty-key entry).
    /// Cleared when the outer mapping at `indent` advances to Value phase.
    complex_key_inline: Option<usize>,
    /// When a tag or anchor appears inline on a physical line (e.g. `!!str &a key:`),
    /// the key content is prepended as a synthetic line with the key's column as its
    /// indent.  This field records the indent of the ORIGINAL physical line so that
    /// `handle_mapping_entry` can open the mapping at the correct (original) indent
    /// rather than the synthetic line's offset.
    property_origin_indent: Option<usize>,
}

impl EventIter<'_> {
    /// Current combined collection depth (sequences + mappings).
    const fn collection_depth(&self) -> usize {
        self.coll_stack.len()
    }
}

/// Build an empty plain scalar event.
pub(crate) const fn empty_scalar_event<'input>() -> Event<'input> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(""),
        style: ScalarStyle::Plain,
        anchor: None,
        tag: None,
    }
}

/// Build a span that covers exactly the 3-byte document marker at `marker_pos`.
pub(crate) const fn marker_span(marker_pos: Pos) -> Span {
    Span {
        start: marker_pos,
        end: Pos {
            byte_offset: marker_pos.byte_offset + 3,
            line: marker_pos.line,
            column: marker_pos.column + 3,
        },
    }
}

/// Build a zero-width span at `pos`.
pub(crate) const fn zero_span(pos: Pos) -> Span {
    Span {
        start: pos,
        end: pos,
    }
}
