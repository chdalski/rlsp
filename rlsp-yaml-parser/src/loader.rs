// SPDX-License-Identifier: MIT

//! Event-to-AST loader.
//!
//! Consumes the event stream from [`crate::parse_events`] and builds a
//! `Vec<Document<Span>>`.
//!
//! Two modes are available:
//! - **Lossless** (default): alias references are kept as [`Node::Alias`]
//!   nodes — no expansion, safe for untrusted input without any expansion
//!   limit.
//! - **Resolved**: aliases are expanded inline.  An expansion-node counter
//!   guards against alias bombs (Billion Laughs attack).  The expansion
//!   carries none of the anchor's comments.
//!
//! Security controls (all active in both modes unless noted):
//! - `max_nesting_depth` — caps sequence/mapping nesting, which bounds the
//!   recursion of loading without alias expansion (default
//!   [`DEFAULT_MAX_NESTING_DEPTH`], 256).
//! - `max_anchors` — caps distinct anchor registrations to bound anchor-map
//!   memory (default 10 000).
//! - `max_expanded_nodes` — caps total nodes produced by alias expansion in
//!   resolved mode only (default 1 000 000).
//!
//! # Accepted risks
//!
//! `expand_node` does not detect the case where an anchor-within-expansion
//! references a previously defined anchor, forming an indirect cycle not
//! caught by the `in_progress` set until the second traversal.  This
//! limitation exists in the old loader and is acceptable in the LSP context
//! where Lossless mode is the default.  The `expanded_nodes` volume limit
//! provides the backstop.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::iter::Peekable;

use std::sync::Arc;

use crate::error::{Error, ErrorKind};
use crate::event::{Event, EventMeta, ScalarStyle};
use crate::limits::MAX_COLLECTION_DEPTH;
use crate::node::{Document, Node, NodeMeta};
use crate::pos::{LineIndex, Pos, Span};
use crate::schema::{CollectionKind, Schema, resolve_collection, resolve_scalar};

use comments::{attach_leading_comments, attach_trailing_comment, clone_without_comments};
use reloc::reloc;
use stream::{
    consume_leading_comments, consume_leading_comments_at, next_from, peek_trailing_comment,
    with_hash_prefix,
};

mod comments;
mod reloc;
mod stream;

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Errors produced by the loader.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The event stream contained a parse error.
    #[error("parse error at {pos:?}: {message}")]
    #[non_exhaustive]
    Parse {
        /// Source position where the parse error was detected.
        pos: Pos,
        /// Human-readable description of the error.
        message: String,
        /// Broad category of the error, for routing without message-string matching.
        kind: ErrorKind,
    },

    /// The event stream ended unexpectedly mid-document.
    #[error("unexpected end of event stream")]
    UnexpectedEndOfStream,

    /// Nesting depth exceeded the configured limit.
    #[error("nesting depth limit exceeded at {pos:?} (max: {limit})")]
    NestingDepthLimitExceeded {
        /// The configured nesting depth limit that was exceeded.
        limit: usize,
        /// Source position of the collection start that exceeded the limit.
        pos: Pos,
    },

    /// Too many distinct anchor names were defined.
    #[error("anchor count limit exceeded at {pos:?} (max: {limit})")]
    AnchorCountLimitExceeded {
        /// The configured anchor count limit that was exceeded.
        limit: usize,
        /// Source position of the anchor that exceeded the limit.
        pos: Pos,
    },

    /// Alias expansion produced more nodes than the configured limit.
    #[error("alias expansion node limit exceeded at {pos:?} (max: {limit})")]
    AliasExpansionLimitExceeded {
        /// The configured expansion node limit that was exceeded.
        limit: usize,
        /// Source position of the node that exceeded the expansion limit.
        pos: Pos,
    },

    /// A circular alias reference was detected.
    #[error("circular alias reference at {pos:?}: '{name}'")]
    CircularAlias {
        /// The anchor name involved in the cycle.
        name: String,
        /// Source position of the alias that triggered the cycle detection.
        pos: Pos,
    },

    /// An alias referred to an anchor that was never defined.
    #[error("undefined alias at {pos:?}: '{name}'")]
    UndefinedAlias {
        /// The alias name that had no corresponding anchor definition.
        name: String,
        /// Source position of the alias reference.
        pos: Pos,
    },

    /// A plain scalar could not be resolved under the JSON schema.
    ///
    /// The JSON schema has no fallback: every untagged plain scalar must match
    /// one of its patterns (null, bool, int, float).  If none match, the scalar
    /// is an error per YAML 1.2.2 §10.2.
    ///
    /// `value` is truncated to 128 Unicode scalar values and ASCII control
    /// characters (U+0000–U+001F, U+007F) are replaced with `\uXXXX` escapes
    /// to prevent log injection via the `Display` impl.
    #[error("JSON schema: plain scalar does not match any type pattern")]
    UnresolvedScalar {
        /// The sanitized, truncated scalar value that failed resolution.
        value: String,
        /// Source position of the scalar.
        pos: Pos,
    },
}

// Convenience alias used inside the module.
type Result<T> = std::result::Result<T, LoadError>;

// The recursive part of the loader boxes its errors: `LoadError` is many words
// large, and in an unoptimized build every `?` keeps another copy of the
// `Result` in the frame of each level of nesting.  `LoadState::run` unboxes.
type BoxedResult<T> = std::result::Result<T, Box<LoadError>>;

// Type alias for the peekable event stream used throughout the loader.
type EventStream<'a> =
    Peekable<Box<dyn Iterator<Item = std::result::Result<(Event<'a>, Span), Error>> + 'a>>;

/// Unpack an `Option<Box<EventMeta>>` into its four constituent fields.
#[expect(
    clippy::type_complexity,
    reason = "four-tuple mirrors EventMeta fields; extracting a type alias here would obscure the one-to-one correspondence"
)]
#[inline]
fn unpack_meta(
    meta: Option<Box<EventMeta<'_>>>,
) -> (
    Option<&'_ str>,
    Option<Span>,
    Option<std::borrow::Cow<'_, str>>,
    Option<Span>,
) {
    meta.map_or((None, None, None, None), |m| {
        (m.anchor, m.anchor_loc, m.tag, m.tag_loc)
    })
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Loader mode — controls how alias references are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    /// Preserve aliases as [`Node::Alias`] nodes (default, safe for LSP).
    Lossless,
    /// Expand aliases inline; subject to `max_expanded_nodes` limit.
    ///
    /// A node produced by an alias carries none of the anchor's comments (the
    /// leading and trailing comments of the anchored node and of the nodes
    /// inside it): the definition site keeps them, and the comments of the
    /// alias site stay on the expanded node.
    Resolved,
}

/// Security and behaviour options for the loader.
#[derive(Debug, Clone)]
pub struct LoaderOptions {
    /// Maximum mapping/sequence nesting depth before returning
    /// [`LoadError::NestingDepthLimitExceeded`] (default:
    /// [`DEFAULT_MAX_NESTING_DEPTH`], 256).
    ///
    /// The loader recurses once per level, so the limit guards the stack when
    /// loading without alias expansion: the default loads on a 2 MiB thread in
    /// a debug build, the build with the largest frames.  A higher value needs
    /// a stack to match, and the event parser's own cap
    /// ([`crate::MAX_COLLECTION_DEPTH`]) holds whatever this value.  The limit
    /// does not bound the tree that alias expansion builds in
    /// [`LoadMode::Resolved`], nor code that walks the loaded tree.
    pub max_nesting_depth: usize,
    /// Maximum number of distinct anchor names per document before returning
    /// [`LoadError::AnchorCountLimitExceeded`] (default: 10 000).
    pub max_anchors: usize,
    /// Maximum total nodes produced by alias expansion in resolved mode before
    /// returning [`LoadError::AliasExpansionLimitExceeded`] (default: 1 000 000).
    pub max_expanded_nodes: usize,
    /// Controls how alias references are handled during loading.
    pub mode: LoadMode,
    /// YAML 1.2.2 §10 schema to apply during loading (default: [`Schema::Core`]).
    ///
    /// Each node's tag is resolved according to this schema after the node is
    /// constructed.  Nodes with explicit source tags are left unchanged.
    pub schema: Schema,
}

/// The nesting depth that [`LoaderOptions::default`] allows.
///
/// A document nested one level deeper makes [`load`] return
/// [`LoadError::NestingDepthLimitExceeded`].  Callers that default their own
/// nesting limit take it from here, so that all of them agree.
pub const DEFAULT_MAX_NESTING_DEPTH: usize = 256;

const _: () = assert!(
    DEFAULT_MAX_NESTING_DEPTH <= MAX_COLLECTION_DEPTH,
    "a default above the event parser's cap would never be reached"
);

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            max_nesting_depth: DEFAULT_MAX_NESTING_DEPTH,
            max_anchors: 10_000,
            max_expanded_nodes: 1_000_000,
            mode: LoadMode::Lossless,
            schema: Schema::Core,
        }
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for configuring and creating a [`Loader`].
///
/// ```
/// use rlsp_yaml_parser::loader::LoaderBuilder;
///
/// let docs = LoaderBuilder::new().lossless().build().load("hello\n").unwrap();
/// assert_eq!(docs.len(), 1);
/// ```
pub struct LoaderBuilder {
    options: LoaderOptions,
}

impl LoaderBuilder {
    /// Create a builder with default options (lossless mode, safe limits).
    #[must_use]
    pub fn new() -> Self {
        Self {
            options: LoaderOptions::default(),
        }
    }

    /// Use lossless mode — aliases become [`Node::Alias`] nodes.
    #[must_use]
    pub const fn lossless(mut self) -> Self {
        self.options.mode = LoadMode::Lossless;
        self
    }

    /// Use resolved mode — aliases are expanded inline.
    #[must_use]
    pub const fn resolved(mut self) -> Self {
        self.options.mode = LoadMode::Resolved;
        self
    }

    /// Override the maximum nesting depth.
    #[must_use]
    pub const fn max_nesting_depth(mut self, limit: usize) -> Self {
        self.options.max_nesting_depth = limit;
        self
    }

    /// Override the maximum anchor count.
    #[must_use]
    pub const fn max_anchors(mut self, limit: usize) -> Self {
        self.options.max_anchors = limit;
        self
    }

    /// Override the maximum expanded-node count (resolved mode only).
    #[must_use]
    pub const fn max_expanded_nodes(mut self, limit: usize) -> Self {
        self.options.max_expanded_nodes = limit;
        self
    }

    /// Override the YAML 1.2.2 §10 schema used for tag resolution during loading.
    ///
    /// The default is [`Schema::Core`].  Untagged nodes receive resolved tag URIs
    /// in the AST; nodes with explicit source tags are not modified.
    #[must_use]
    pub const fn schema(mut self, s: Schema) -> Self {
        self.options.schema = s;
        self
    }

    /// Consume the builder and produce a [`Loader`].
    #[must_use]
    pub const fn build(self) -> Loader {
        Loader {
            options: self.options,
        }
    }
}

impl Default for LoaderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Loader
// ---------------------------------------------------------------------------

/// A configured YAML loader.
pub struct Loader {
    options: LoaderOptions,
}

impl Loader {
    /// Load YAML text into a sequence of documents.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the input contains a parse error, exceeds a configured
    /// security limit, or (in resolved mode) references an undefined anchor.
    pub fn load(&self, input: &str) -> std::result::Result<Vec<Document<Span>>, LoadError> {
        let mut state = LoadState::new(&self.options, input);
        let iter: Box<dyn Iterator<Item = std::result::Result<(Event<'_>, Span), Error>> + '_> =
            Box::new(crate::parse_events(input));
        state.run(iter.peekable())
    }
}

// ---------------------------------------------------------------------------
// Convenience entry point
// ---------------------------------------------------------------------------

/// Load YAML text using lossless mode, default security limits, and Core schema tag
/// resolution (YAML 1.2.2 §10.3).
///
/// Returns one `Document<Span>` per YAML document in the stream.  Untagged nodes
/// receive resolved tag URIs according to the Core schema; nodes with explicit source
/// tags are left unchanged.
///
/// # Errors
///
/// Returns `Err` if the input contains a parse error or exceeds a security
/// limit (nesting depth or anchor count).
///
/// ```
/// use rlsp_yaml_parser::loader::load;
/// use rlsp_yaml_parser::Node;
///
/// let docs = load("hello\n").unwrap();
/// assert_eq!(docs.len(), 1);
/// let Node::Scalar { tag, .. } = &docs[0].root else { panic!() };
/// assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
/// ```
pub fn load(input: &str) -> std::result::Result<Vec<Document<Span>>, LoadError> {
    LoaderBuilder::new().lossless().build().load(input)
}

// ---------------------------------------------------------------------------
// Internal loader state
// ---------------------------------------------------------------------------

struct LoadState<'opt> {
    options: &'opt LoaderOptions,
    /// Anchors registered so far in the current document: name → node.
    anchor_map: HashMap<String, Node<Span>>,
    /// Count of distinct anchors registered (resets per document).
    anchor_count: usize,
    /// Current nesting depth (incremented on Begin, decremented on End).
    depth: usize,
    /// Total nodes produced via alias expansion (resolved mode only).
    expanded_nodes: usize,
    /// Leading comments accumulated by `begin_node` when it meets a run of
    /// `Comment` events at a mapping value position (between the key and the
    /// value, whatever the value is), or by a sequence/mapping loop when it
    /// hits End with leftover leading comments.  The next mapping/sequence
    /// loop iteration picks these up and prepends them to the next entry's
    /// leading comments; when no entry follows (the document ends), the
    /// document takes them as its `end_comments`.
    pending_leading: Vec<String>,
    /// Line index for the current document source; shared across all documents
    /// produced from the same input via `Arc` to avoid N full copies.
    line_index: Arc<LineIndex>,
}

/// Comments read before a node, split by [`LoadState::split_comments_after_empty_node`].
struct SplitComments {
    /// The comments that belong to the node.
    before: Vec<String>,
    /// The comments that belong to what follows it.
    after: Vec<String>,
}

/// The comments that lead to a key or item, boxed so that they cost the
/// recursion one word.  Most entries have none and allocate nothing.
struct EntryComments {
    /// The comments read directly before the node, with their start offsets.
    raw: Vec<(String, u32)>,
    /// The comments passed on from what came before them.
    inherited: Vec<String>,
}

/// What [`LoadState::begin_node`] left in the slot.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Children {
    /// The node is complete: a scalar, an alias, or an empty scalar where no
    /// node starts.
    Absent,
    /// A mapping or sequence whose entries or items are still in the stream.
    Follow,
}

/// What [`LoadState::next_entry`] found in a collection.
enum NextEntry {
    /// The collection's end event is next.
    Closed,
    /// A key or item is next, led by these comments.
    Entry(Option<Box<EntryComments>>),
}

impl<'opt> LoadState<'opt> {
    fn new(options: &'opt LoaderOptions, input: &str) -> Self {
        Self {
            options,
            anchor_map: HashMap::new(),
            anchor_count: 0,
            depth: 0,
            expanded_nodes: 0,
            pending_leading: Vec::new(),
            line_index: Arc::new(LineIndex::new(input)),
        }
    }

    fn reset_for_document(&mut self) {
        self.anchor_map.clear();
        self.anchor_count = 0;
        self.expanded_nodes = 0;
        self.pending_leading.clear();
    }

    fn run(&mut self, mut stream: EventStream<'_>) -> Result<Vec<Document<Span>>> {
        let mut docs: Vec<Document<Span>> = Vec::new();

        // Skip StreamStart.
        match stream.next() {
            Some(Ok(_)) | None => {}
            Some(Err(e)) => {
                return Err(LoadError::Parse {
                    pos: e.pos,
                    message: e.message,
                    kind: e.kind,
                });
            }
        }

        // Comments outside any document: they wait here until the next
        // `DocumentStart` takes them as its `prefix_comments`.  What is left
        // at the end of the stream follows the last document's `...`.
        let mut outside_comments: Vec<String> = Vec::new();

        loop {
            match next_from(&mut stream)? {
                None | Some((Event::StreamEnd, _)) => break,
                Some((
                    Event::DocumentStart {
                        explicit,
                        version,
                        tag_directives,
                    },
                    _,
                )) => {
                    let doc_explicit_start = explicit;
                    let doc_version = version;
                    let doc_tags = tag_directives;
                    self.reset_for_document();

                    // Comments after `---` and before the root.
                    let doc_comments = consume_leading_comments(&mut stream)?;

                    // Parse root node (may be absent for empty documents).
                    let mut root = empty_scalar();
                    if is_document_end(stream.peek()) {
                        // Empty document — the root is an empty scalar.
                        apply_schema_to_node(&mut root, self.options.schema, &self.line_index)?;
                    } else {
                        self.parse_node(&mut stream, &mut root)
                            .map_err(|error| *error)?;
                    }
                    attach_line_comment(&mut stream, &mut root, &self.line_index)?;

                    // Comments that no node inherited come first, in source
                    // order, then the comments after the root.  The latter
                    // must also not hide the `...` that follows them from the
                    // check below.
                    let mut end_comments = std::mem::take(&mut self.pending_leading);
                    end_comments.extend(consume_leading_comments(&mut stream)?);

                    // The root is complete.  A closing event here closes
                    // nothing, and a node here is a second root that the
                    // document has no place for.
                    match stream.peek() {
                        Some(Ok((Event::MappingEnd | Event::SequenceEnd, end))) => {
                            return Err(self.unbalanced_end(*end));
                        }
                        Some(Ok((
                            Event::Scalar { .. }
                            | Event::Alias { .. }
                            | Event::MappingStart { .. }
                            | Event::SequenceStart { .. },
                            at,
                        ))) => return Err(self.misplaced_node(*at)),
                        _ => {}
                    }

                    // Consume DocumentEnd if present and capture its explicit flag.
                    let doc_explicit_end =
                        if let Some(Ok((Event::DocumentEnd { explicit }, _))) = stream.peek() {
                            let end_explicit = *explicit;
                            let _ = stream.next();
                            end_explicit
                        } else {
                            false
                        };

                    docs.push(Document {
                        root,
                        version: doc_version,
                        tags: doc_tags,
                        prefix_comments: std::mem::take(&mut outside_comments),
                        comments: doc_comments,
                        end_comments,
                        after_end_comments: Vec::new(),
                        explicit_start: doc_explicit_start,
                        explicit_end: doc_explicit_end,
                        line_index: Some(self.line_index.clone()),
                    });
                }
                Some((Event::Comment { text }, _)) => {
                    outside_comments.push(with_hash_prefix(text));
                }
                // These carry no text, so skipping them loses nothing.
                Some((
                    Event::StreamStart
                    | Event::DocumentEnd { .. }
                    | Event::MappingEnd
                    | Event::SequenceEnd,
                    _,
                )) => {}
                // A node outside a document has no document to hold it, and
                // skipping it would drop its text from the result.
                Some((
                    Event::Scalar { .. }
                    | Event::Alias { .. }
                    | Event::MappingStart { .. }
                    | Event::SequenceStart { .. },
                    at,
                )) => return Err(self.misplaced_node(at)),
            }
        }

        // A stream that holds only comments has no document to keep them.
        if let Some(last) = docs.last_mut() {
            last.after_end_comments = outside_comments;
        }

        Ok(docs)
    }

    /// Parse a single node from the stream into `slot`.
    ///
    /// Advances the stream past the node (including end-of-container events).
    ///
    /// The caller passes a slot that holds an empty scalar, and it still does
    /// when no node starts here: a structural end event is left in the stream
    /// for the caller's collection loop, and the empty scalar stands for the
    /// missing node.  Any other node replaces it.
    ///
    /// This function and the two collection loops, `parse_entries` and
    /// `parse_items`, are the recursion: each level of nesting holds one frame
    /// of each.  Their frames stay small because the node is built in its slot
    /// in the parent's vector, so the recursion holds references and no node,
    /// and everything that needs more than a few words of stack lives in the
    /// `#[inline(never)]` helpers, whose frames are gone before the recursion
    /// goes deeper.  That keeps the stack use per level low enough for the
    /// nesting limit to protect a 2 MiB stack in a debug build, which has the
    /// largest frames.
    fn parse_node(
        &mut self,
        stream: &mut EventStream<'_>,
        slot: &mut Node<Span>,
    ) -> BoxedResult<()> {
        if self.begin_node(stream, slot)? == Children::Absent {
            return Ok(());
        }
        match slot {
            Node::Mapping { entries, .. } => self.parse_entries(stream, entries)?,
            Node::Sequence { items, .. } => self.parse_items(stream, items)?,
            // `begin_node` reports `Children::Follow` only for the mapping or
            // sequence it just put in the slot, so a complete node never gets
            // here.  Reaching this arm would mean a bug in `begin_node`; it has
            // no children to read and nothing to finish.
            Node::Scalar { .. } | Node::Alias { .. } => return Ok(()),
        }
        self.finish_collection(stream, slot)
    }

    /// Read the events up to the next node's first one and start the node in
    /// `slot`.  A mapping or sequence is only started: its entries or items
    /// come next ([`Children::Follow`]).  Any other node is complete.
    // Not inlined: in a release build this function would join the frame of
    // `parse_node`, and its event and error temporaries would add to the stack
    // use of every level of nesting.
    #[inline(never)]
    fn begin_node(
        &mut self,
        stream: &mut EventStream<'_>,
        slot: &mut Node<Span>,
    ) -> BoxedResult<Children> {
        // Structural end events close the caller's collection loop — do NOT
        // consume them here.  Leave the empty scalar in the slot and the event
        // in the stream so the outer mapping/sequence loop can see and consume
        // it.
        if matches!(
            stream.peek(),
            Some(Ok((
                Event::MappingEnd | Event::SequenceEnd | Event::DocumentEnd { .. },
                _
            )))
        ) {
            return Ok(Children::Absent);
        }

        let Some((event, span)) = next_from(stream)? else {
            return Ok(Children::Absent);
        };

        match event {
            Event::Scalar { value, style, meta } => {
                *slot = self.scalar_node(value, style, meta, span)?;
                Ok(Children::Absent)
            }

            Event::MappingStart { style, meta } => {
                self.enter_collection(span)?;
                let (tag, meta) = node_properties(meta);
                *slot = Node::Mapping {
                    entries: Vec::new(),
                    style,
                    tag,
                    loc: span,
                    meta,
                };
                Ok(Children::Follow)
            }

            Event::SequenceStart { style, meta } => {
                self.enter_collection(span)?;
                let (tag, meta) = node_properties(meta);
                *slot = Node::Sequence {
                    items: Vec::new(),
                    style,
                    tag,
                    loc: span,
                    meta,
                };
                Ok(Children::Follow)
            }

            Event::Alias { name } => {
                *slot = self.resolve_alias(name, span)?;
                Ok(Children::Absent)
            }

            Event::Comment { text } => {
                // Comment run between a mapping key and its value (e.g.
                // `key:\n  # comment\n  subkey: val`): only the mapping value
                // position reaches this arm, because keys, sequence items,
                // and the document root drain their comments in a loop first.
                // Save the run in `pending_leading`; the node that follows the
                // run decides who inherits it — the first entry of a nested
                // collection, or, for a scalar, empty, or alias value, the
                // next key of the mapping (or the parent's next entry).  When
                // the mapping or document ends after the value, no node does.
                //
                // Drain the rest of the run here so the recursive call below
                // never sees another `Comment`: the stack depth stays
                // constant however many comments the run holds.
                self.pending_leading.push(with_hash_prefix(text));
                self.pending_leading
                    .extend(consume_leading_comments(stream)?);
                self.begin_node(stream, slot)
            }

            // Structural event where a node is expected — leave the empty scalar.
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::MappingEnd
            | Event::SequenceEnd => Ok(Children::Absent),
        }
    }

    fn scalar_node(
        &mut self,
        value: Cow<'_, str>,
        style: ScalarStyle,
        meta: Option<Box<EventMeta<'_>>>,
        span: Span,
    ) -> Result<Node<Span>> {
        let (tag, meta) = node_properties(meta);
        let mut node = Node::Scalar {
            value: value.into_owned(),
            style,
            tag,
            loc: span,
            meta,
        };
        self.complete_node(&mut node, span)?;
        Ok(node)
    }

    /// Apply the schema to a node that has everything but its comments, and
    /// register its anchor.  `span` starts where the node does.
    fn complete_node(&mut self, node: &mut Node<Span>, span: Span) -> Result<()> {
        apply_schema_to_node(node, self.options.schema, &self.line_index)?;
        if let Some(name) = node.anchor() {
            let anchor_span = node.anchor_loc().unwrap_or(span);
            self.register_anchor(name.to_owned(), node, anchor_span)?;
        }
        Ok(())
    }

    /// Count a mapping or sequence that starts at `span` toward the nesting
    /// depth.
    fn enter_collection(&mut self, span: Span) -> Result<()> {
        self.depth += 1;
        if self.depth > self.options.max_nesting_depth {
            return Err(LoadError::NestingDepthLimitExceeded {
                limit: self.options.max_nesting_depth,
                pos: span_start_to_pos(span.start, &self.line_index),
            });
        }
        Ok(())
    }

    /// Load the keys and values of the mapping that `begin_node` started, up to
    /// its end event.
    fn parse_entries(
        &mut self,
        stream: &mut EventStream<'_>,
        entries: &mut Vec<(Node<Span>, Node<Span>)>,
    ) -> BoxedResult<()> {
        while let NextEntry::Entry(comments) = self.next_entry(stream, CollectionKind::Mapping)? {
            let (key, value) = push_empty_entry(entries);
            self.parse_node(stream, key)?;
            self.finish_key(stream, key, comments)?;
            self.parse_node(stream, value)?;
            self.finish_value(stream, value)?;
        }
        Ok(())
    }

    /// Load the items of the sequence that `begin_node` started, up to its end
    /// event.
    fn parse_items(
        &mut self,
        stream: &mut EventStream<'_>,
        items: &mut Vec<Node<Span>>,
    ) -> BoxedResult<()> {
        while let NextEntry::Entry(comments) = self.next_entry(stream, CollectionKind::Sequence)? {
            let item = push_empty_item(items);
            self.parse_node(stream, item)?;
            self.finish_item(stream, item, comments)?;
        }
        Ok(())
    }

    /// Decide whether the collection of the given kind has another key or item,
    /// and collect the comments that lead to it.  The end event of the
    /// collection stays in the stream.
    // Not inlined: the comment vectors and the event temporaries would join
    // the frame of the collection loop in a release build, and the loop is part
    // of the recursion.
    #[inline(never)]
    fn next_entry(
        &mut self,
        stream: &mut EventStream<'_>,
        kind: CollectionKind,
    ) -> BoxedResult<NextEntry> {
        // Consume leading comments before the next key or item.  Also collect
        // any comments that spilled over from a sibling value's collection end
        // (stored in `pending_leading`).
        let raw = consume_leading_comments_at(stream)?;
        let inherited = std::mem::take(&mut self.pending_leading);

        let closes = match stream.peek() {
            // The stream stops inside the collection.
            None | Some(Ok((Event::StreamEnd, _))) => {
                return Err(Box::new(LoadError::UnexpectedEndOfStream));
            }
            Some(Ok((Event::MappingEnd, _))) if kind == CollectionKind::Mapping => true,
            Some(Ok((Event::SequenceEnd, _))) if kind == CollectionKind::Sequence => true,
            Some(Ok((Event::MappingEnd | Event::SequenceEnd | Event::DocumentEnd { .. }, end))) => {
                return Err(Box::new(self.unbalanced_end(*end)));
            }
            Some(Err(_)) => {
                // Consume the error.
                return Err(Box::new(match stream.next() {
                    Some(Err(e)) => LoadError::Parse {
                        pos: e.pos,
                        message: e.message,
                        kind: e.kind,
                    },
                    _ => LoadError::UnexpectedEndOfStream,
                }));
            }
            Some(Ok(_)) => false,
        };

        if closes {
            // Save any collected leading comments so the next sibling entry in
            // the parent collection can inherit them (e.g. a comment just
            // before the end event that belongs to the parent's following
            // entry).
            let mut leading = inherited;
            leading.extend(raw.into_iter().map(|(text, _)| text));
            if !leading.is_empty() {
                self.pending_leading = leading;
            }
            return Ok(NextEntry::Closed);
        }

        Ok(NextEntry::Entry(
            if raw.is_empty() && inherited.is_empty() {
                None
            } else {
                Some(Box::new(EntryComments { raw, inherited }))
            },
        ))
    }

    /// Give a loaded key or item the comments read before it.
    fn attach_entry_comments(
        &mut self,
        node: &mut Node<Span>,
        comments: Option<Box<EntryComments>>,
    ) {
        let Some(comments) = comments else {
            return;
        };
        let EntryComments { raw, mut inherited } = *comments;
        let split = self.split_comments_after_empty_node(node, raw);
        inherited.extend(split.before);
        attach_leading_comments(node, inherited);
        self.pending_leading.extend(split.after);
    }

    /// Give a loaded mapping key the comments around it.
    // Not inlined: the comment handling would join the frame of the collection
    // loop in a release build.
    #[inline(never)]
    fn finish_key(
        &mut self,
        stream: &mut EventStream<'_>,
        key: &mut Node<Span>,
        comments: Option<Box<EntryComments>>,
    ) -> BoxedResult<()> {
        self.attach_entry_comments(key, comments);
        // The comment after the key on its own line (`k: # c`, `a: &x # c`) sits
        // between the key and the value event.
        attach_key_line_comment(stream, key, &self.line_index)?;
        Ok(())
    }

    /// Give a loaded mapping value the comment on its line.
    // Not inlined: the comment handling would join the frame of the collection
    // loop in a release build.
    #[inline(never)]
    fn finish_value(
        &self,
        stream: &mut EventStream<'_>,
        value: &mut Node<Span>,
    ) -> BoxedResult<()> {
        attach_line_comment(stream, value, &self.line_index)?;
        Ok(())
    }

    /// Give a loaded sequence item the comments around it.
    // Not inlined: the comment handling would join the frame of the collection
    // loop in a release build.
    #[inline(never)]
    fn finish_item(
        &mut self,
        stream: &mut EventStream<'_>,
        item: &mut Node<Span>,
        comments: Option<Box<EntryComments>>,
    ) -> BoxedResult<()> {
        self.attach_entry_comments(item, comments);
        attach_line_comment(stream, item, &self.line_index)?;
        Ok(())
    }

    /// Complete a mapping or sequence whose entries or items are loaded:
    /// consume its end event, leave its nesting level, apply the schema, and
    /// register its anchor.
    // Not inlined: the end event and the schema and anchor work hold large
    // temporaries that would join the frame of `parse_node` in a release build.
    #[inline(never)]
    fn finish_collection(
        &mut self,
        stream: &mut EventStream<'_>,
        node: &mut Node<Span>,
    ) -> BoxedResult<()> {
        let loc = loc_mut(node);
        if let Some(Ok((Event::MappingEnd | Event::SequenceEnd, end))) = stream.peek() {
            loc.end = end.end;
            let _ = stream.next();
        }
        let span = *loc;
        self.depth -= 1;
        self.complete_node(node, span)?;
        Ok(())
    }

    /// The error for a closing event that does not close the collection being
    /// loaded: a mapping that meets a `SequenceEnd`, a sequence that meets a
    /// `MappingEnd`, either that meets a `DocumentEnd`.  The parser emits such a
    /// stream only for input it should have rejected; `begin_node` leaves a
    /// closing event in the stream, so a loop that did not stop here would never
    /// consume it and would add entries until memory ran out.
    fn unbalanced_end(&self, end: Span) -> LoadError {
        LoadError::Parse {
            pos: span_start_to_pos(end.start, &self.line_index),
            message: "closing event does not match the open collection".into(),
            kind: ErrorKind::Syntax,
        }
    }

    /// The error for a node event that has no place in the document: one after
    /// the root, or one outside any document.  The parser emits such a stream
    /// only for input it should have rejected; skipping the node would drop its
    /// text from the result without a trace.
    fn misplaced_node(&self, at: Span) -> LoadError {
        LoadError::Parse {
            pos: span_start_to_pos(at.start, &self.line_index),
            message: "node outside the document root".into(),
            kind: ErrorKind::Syntax,
        }
    }

    /// Split the comments read before `node` into those that stay with it and
    /// those that stand on lines below the properties of an empty node.
    ///
    /// A tag or an anchor alone (`- !!str`) is a node of its own that ends with
    /// its properties.  The parser emits its scalar after the comment lines
    /// that follow (the scalar sits where the next token starts), so those
    /// comments reach the loader as comments before the node.  They belong to
    /// what follows it; a comment on the properties' own line stays.
    fn split_comments_after_empty_node(
        &self,
        node: &Node<Span>,
        comments: Vec<(String, u32)>,
    ) -> SplitComments {
        let properties_end = match node {
            Node::Scalar {
                value,
                style: ScalarStyle::Plain,
                meta: Some(meta),
                ..
            } if value.is_empty() => [meta.anchor_loc, meta.tag_loc]
                .into_iter()
                .flatten()
                .map(|loc| loc.end)
                .max(),
            Node::Scalar { .. }
            | Node::Mapping { .. }
            | Node::Sequence { .. }
            | Node::Alias { .. } => None,
        };
        let split_at = properties_end.map_or(comments.len(), |end| {
            let properties_line = self.line_index.line_column(end).0;
            comments
                .iter()
                .position(|&(_, start)| self.line_index.line_column(start).0 > properties_line)
                .unwrap_or(comments.len())
        });
        let mut before: Vec<String> = comments.into_iter().map(|(text, _)| text).collect();
        let after = before.split_off(split_at);
        SplitComments { before, after }
    }

    fn register_anchor(
        &mut self,
        name: String,
        node: &Node<Span>,
        anchor_span: Span,
    ) -> Result<()> {
        let pos = span_start_to_pos(anchor_span.start, &self.line_index);
        if !self.anchor_map.contains_key(&name) {
            self.anchor_count += 1;
            if self.anchor_count > self.options.max_anchors {
                return Err(LoadError::AnchorCountLimitExceeded {
                    limit: self.options.max_anchors,
                    pos,
                });
            }
        }
        // Count the anchor node itself toward the expansion budget in resolved
        // mode so that the total reflects every node present in the expanded
        // document (anchor definition + each alias expansion).
        if self.options.mode == LoadMode::Resolved {
            self.expanded_nodes += 1;
            if self.expanded_nodes > self.options.max_expanded_nodes {
                return Err(LoadError::AliasExpansionLimitExceeded {
                    limit: self.options.max_expanded_nodes,
                    pos,
                });
            }
            // The copy has no comments: they belong to the definition site, and
            // copying them to every alias site would multiply them by the
            // expansion count, which the node budget does not count.  The
            // comments of the alias site are attached by the caller after the
            // expansion.
            self.anchor_map.insert(name, clone_without_comments(node));
        } else {
            // Lossless mode never reads anchor_map for expansion; store a
            // zero-cost placeholder so contains_key still detects re-definitions.
            self.anchor_map.insert(name, empty_scalar());
        }
        Ok(())
    }

    fn resolve_alias(&mut self, name: &str, loc: Span) -> Result<Node<Span>> {
        match self.options.mode {
            LoadMode::Lossless => Ok(Node::Alias {
                name: name.to_owned(),
                loc,
                leading_comments: None,
                trailing_comment: None,
            }),
            LoadMode::Resolved => {
                let pos = span_start_to_pos(loc.start, &self.line_index);
                let anchored = self.anchor_map.get(name).cloned().ok_or_else(|| {
                    LoadError::UndefinedAlias {
                        name: name.to_owned(),
                        pos,
                    }
                })?;
                let mut in_progress: HashSet<String> = HashSet::new();
                let expanded = self.expand_node(anchored, &mut in_progress, loc)?;
                // The node stands at the alias site, so a comment on its line
                // is found there, not on the anchor's line.
                Ok(reloc(expanded, loc))
            }
        }
    }

    /// Recursively expand a node, counting every node produced against the
    /// expansion limit and checking for cycles via `in_progress`.
    ///
    /// `alias_loc` is the span of the alias site that triggered this expansion
    /// chain; it is used for error positions when the limit or a cycle is
    /// detected inside expanded content.
    fn expand_node(
        &mut self,
        node: Node<Span>,
        in_progress: &mut HashSet<String>,
        alias_loc: Span,
    ) -> Result<Node<Span>> {
        // Increment at the top — before child recursion — so every node
        // (including non-alias nodes inside expanded trees) counts against the
        // budget.
        self.expanded_nodes += 1;
        if self.expanded_nodes > self.options.max_expanded_nodes {
            return Err(LoadError::AliasExpansionLimitExceeded {
                limit: self.options.max_expanded_nodes,
                pos: span_start_to_pos(alias_loc.start, &self.line_index),
            });
        }

        match node {
            Node::Alias { ref name, loc, .. } => {
                let pos = span_start_to_pos(loc.start, &self.line_index);
                if in_progress.contains(name) {
                    return Err(LoadError::CircularAlias {
                        name: name.clone(),
                        pos,
                    });
                }
                let target = self.anchor_map.get(name).cloned().ok_or_else(|| {
                    LoadError::UndefinedAlias {
                        name: name.clone(),
                        pos,
                    }
                })?;
                in_progress.insert(name.clone());
                // Pass the inner alias loc as the new alias_loc for deeper expansion.
                let expanded = self.expand_node(target, in_progress, loc)?;
                in_progress.remove(name);
                // Re-stamp with the alias site's location.
                Ok(reloc(expanded, loc))
            }
            Node::Mapping {
                entries,
                style,
                tag,
                loc,
                meta,
            } => {
                let mut expanded_entries = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    let ek = self.expand_node(k, in_progress, alias_loc)?;
                    let ev = self.expand_node(v, in_progress, alias_loc)?;
                    expanded_entries.push((ek, ev));
                }
                Ok(Node::Mapping {
                    entries: expanded_entries,
                    style,
                    tag,
                    loc,
                    meta,
                })
            }
            Node::Sequence {
                items,
                style,
                tag,
                loc,
                meta,
            } => {
                let mut expanded_items = Vec::with_capacity(items.len());
                for item in items {
                    expanded_items.push(self.expand_node(item, in_progress, alias_loc)?);
                }
                Ok(Node::Sequence {
                    items: expanded_items,
                    style,
                    tag,
                    loc,
                    meta,
                })
            }
            // Scalars and already-resolved nodes — pass through.
            scalar @ Node::Scalar { .. } => Ok(scalar),
        }
    }
}

/// Return `true` if the peeked item signals end of document (or stream).
const fn is_document_end(peeked: Option<&std::result::Result<(Event<'_>, Span), Error>>) -> bool {
    matches!(
        peeked,
        None | Some(Ok((Event::DocumentEnd { .. } | Event::StreamEnd, _)))
    )
}

/// Convert a `Span.start` byte offset to a `Pos` with accurate line/column.
#[inline]
fn span_start_to_pos(offset: u32, line_index: &LineIndex) -> Pos {
    let (line, column) = line_index.line_column(offset);
    Pos {
        byte_offset: offset as usize,
        line: line as usize,
        column: column as usize,
    }
}

/// Attach the comment on a node's line as its `trailing_comment`, if the next
/// event is that comment.
///
/// A block scalar's line is its header line: its span ends on the line after
/// its content (chomping consumes trailing blank lines), so that line can
/// coincide with the next own-line comment, which is a leading comment of the
/// next node.  Every other node's line is the line its text ends on; a node
/// without text (an empty scalar) has no line to hold a comment, and a comment
/// after it belongs to what follows.
fn attach_line_comment(
    stream: &mut EventStream<'_>,
    node: &mut Node<Span>,
    line_index: &LineIndex,
) -> Result<()> {
    let line = if is_block_scalar(node) {
        Some(node_start_line(node, line_index))
    } else {
        node_end_line(node, line_index)
    };
    attach_comment_on_line(stream, node, line, line_index)
}

/// [`attach_line_comment`] for a mapping key.  An empty key sits on the line of
/// its `:`, so the comment after it on that line is the key's.
fn attach_key_line_comment(
    stream: &mut EventStream<'_>,
    key: &mut Node<Span>,
    line_index: &LineIndex,
) -> Result<()> {
    let line = if is_block_scalar(key) {
        node_start_line(key, line_index)
    } else {
        node_end_line(key, line_index).unwrap_or_else(|| node_start_line(key, line_index))
    };
    attach_comment_on_line(stream, key, Some(line), line_index)
}

/// Attach the next event as `node`'s trailing comment if it is a comment on
/// `line`.
fn attach_comment_on_line(
    stream: &mut EventStream<'_>,
    node: &mut Node<Span>,
    line: Option<u32>,
    line_index: &LineIndex,
) -> Result<()> {
    if !matches!(stream.peek(), Some(Ok((Event::Comment { .. }, _)))) {
        return Ok(());
    }
    let Some(line) = line else {
        return Ok(());
    };
    if let Some(comment) = peek_trailing_comment(stream, line, line_index)? {
        attach_trailing_comment(node, comment);
    }
    Ok(())
}

/// Return the line number of a node's span start position.
///
/// Used to find the comment on a block scalar's header line.
#[inline]
fn node_start_line(node: &Node<Span>, line_index: &LineIndex) -> u32 {
    let start_offset = match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => loc.start,
    };
    line_index.line_column(start_offset).0
}

/// Return the line number of the last line of a node's text, or `None` for a
/// node without text.
///
/// Used to determine whether the next `Comment` event is trailing (same line)
/// or leading (different line).  A span that ends at the start of a line ends
/// on the line before: a block collection's span runs through the line break of
/// its last line, so the comment on the next item's dash line starts where the
/// span ends but is not on the collection's line.  An empty scalar's span is
/// empty and sits at the start of the next token's line.
#[inline]
fn node_end_line(node: &Node<Span>, line_index: &LineIndex) -> Option<u32> {
    let (start, end) = match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => (loc.start, loc.end),
    };
    if end == start {
        return None;
    }
    let (line, column) = line_index.line_column(end);
    Some(if column == 0 {
        line.saturating_sub(1)
    } else {
        line
    })
}

/// Return `true` if the node is a block scalar (literal `|` or folded `>`).
///
/// Block scalars consume trailing blank lines as part of chomping, so their
/// `span.end` falls on the line *after* the last consumed line.  This means a
/// comment on the immediately following line has the same line number as
/// `span.end.line`, which would cause `peek_trailing_comment` to falsely
/// classify it as an inline trailing comment.  `attach_line_comment` uses this
/// predicate to look at the header line instead.
#[inline]
const fn is_block_scalar(node: &Node<Span>) -> bool {
    matches!(
        node,
        Node::Scalar {
            style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
            ..
        }
    )
}

// ---------------------------------------------------------------------------
// Schema resolution helpers
// ---------------------------------------------------------------------------

/// Maximum number of Unicode scalar values kept in [`LoadError::UnresolvedScalar`]
/// value field.  Prevents unbounded allocation when storing user-supplied input
/// in error messages.
const UNRESOLVED_VALUE_MAX_CHARS: usize = 128;

/// Sanitize a raw scalar value for inclusion in an error message.
///
/// - Truncates to [`UNRESOLVED_VALUE_MAX_CHARS`] Unicode scalar values,
///   appending `"..."` when truncated.
/// - Replaces ASCII control characters (U+0000–U+001F and U+007F) with
///   `\uXXXX` hex escapes to prevent log injection via the `Display` impl.
fn sanitize_scalar_for_error(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(UNRESOLVED_VALUE_MAX_CHARS * 2));
    let mut truncated = false;

    for (i, ch) in raw.chars().enumerate() {
        if i >= UNRESOLVED_VALUE_MAX_CHARS {
            truncated = true;
            break;
        }
        if ch.is_ascii_control() {
            // Replace control chars with \uXXXX escape to prevent log injection.
            let escaped = format!("\\u{:04X}", ch as u32);
            out.push_str(&escaped);
        } else {
            out.push(ch);
        }
    }

    if truncated {
        out.push_str("...");
    }
    out
}

/// Apply schema tag resolution to a freshly-constructed node.
///
/// - For scalars: translates bare `!` to `None` (non-specific), then calls
///   `resolve_scalar`.
/// - For mappings/sequences: translates bare `!` to `None`, then calls
///   `resolve_collection`.
/// - On `Ok(Some(tag))`: overwrites `node.tag`; `tag_loc` is left `None`
///   (no source position for a resolved tag).
/// - On `Ok(None)` (explicit tag present): leaves `node.tag` unchanged.
///
/// # Errors
///
/// Returns [`LoadError::UnresolvedScalar`] when `schema` is [`Schema::Json`]
/// and a plain scalar does not match any JSON type pattern.
#[inline]
fn apply_schema_to_node(
    node: &mut Node<Span>,
    schema: Schema,
    line_index: &LineIndex,
) -> Result<()> {
    match node {
        Node::Scalar {
            value,
            style,
            tag,
            loc,
            meta,
        } => {
            // Bare `!` on a scalar is the non-specific scalar tag — it resolves
            // unconditionally to !!str regardless of content (YAML 1.2.2 §10.2.1,
            // §10.3.2: "non-specific" tag for scalars = Failsafe str).  We handle
            // it before calling the schema resolver so Core doesn't pattern-match
            // the value.
            //
            // `tag_loc` is preserved here (NOT cleared) because `!` is explicitly
            // written in the source.  Preserving `tag_loc` lets downstream consumers
            // (e.g. the formatter) distinguish user-authored tags from resolver-injected
            // ones, which is critical for correct idempotent output.
            if tag.as_deref() == Some("!") {
                *tag = Some(Cow::Borrowed(crate::schema::ResolvedTag::Str.as_str()));
                return Ok(());
            }
            // All other tags: pass through as-is (Some(non-!) = explicit tag → Ok(None)).
            match resolve_scalar(schema, *style, value, tag.as_deref()) {
                Ok(Some(resolved)) => {
                    *tag = Some(Cow::Borrowed(resolved.as_str()));
                    // Clear tag_loc: resolver-injected tags have no source position.
                    if let Some(m) = meta.as_mut() {
                        m.tag_loc = None;
                        if m.is_all_none() {
                            *meta = None;
                        }
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    return Err(LoadError::UnresolvedScalar {
                        value: sanitize_scalar_for_error(value),
                        pos: span_start_to_pos(loc.start, line_index),
                    });
                }
            }
        }
        Node::Mapping { tag, meta, .. } => {
            // Bare `!` on a collection means non-specific collection tag — translate
            // to None so the resolver returns the kind-based tag (!!map / !!seq).
            // `tag_loc` stays for a bare `!`, as on a scalar: it is written in the
            // source, and a consumer that prints the node needs to tell it from a
            // resolver-injected tag.
            let non_specific = tag.as_deref() == Some("!");
            let effective_tag = tag.as_deref().filter(|t| *t != "!");
            if let Some(resolved) =
                resolve_collection(schema, CollectionKind::Mapping, effective_tag)
            {
                *tag = Some(Cow::Borrowed(resolved.as_str()));
                if let Some(m) = meta.as_mut().filter(|_| !non_specific) {
                    m.tag_loc = None;
                    if m.is_all_none() {
                        *meta = None;
                    }
                }
            }
        }
        Node::Sequence { tag, meta, .. } => {
            let non_specific = tag.as_deref() == Some("!");
            let effective_tag = tag.as_deref().filter(|t| *t != "!");
            if let Some(resolved) =
                resolve_collection(schema, CollectionKind::Sequence, effective_tag)
            {
                *tag = Some(Cow::Borrowed(resolved.as_str()));
                if let Some(m) = meta.as_mut().filter(|_| !non_specific) {
                    m.tag_loc = None;
                    if m.is_all_none() {
                        *meta = None;
                    }
                }
            }
        }
        Node::Alias { .. } => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Node helpers
// ---------------------------------------------------------------------------

const fn empty_scalar() -> Node<Span> {
    Node::Scalar {
        value: String::new(),
        style: ScalarStyle::Plain,
        tag: None,
        loc: Span { start: 0, end: 0 },
        meta: None,
    }
}

/// The source span of any node.
const fn loc_mut(node: &mut Node<Span>) -> &mut Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => loc,
    }
}

/// Add an empty slot for the next sequence item.
// Not inlined: in a release build the empty scalars would be built in the frame
// of the collection loop, which is part of the recursion.
#[inline(never)]
fn push_empty_item(items: &mut Vec<Node<Span>>) -> &mut Node<Span> {
    items.push_mut(empty_scalar())
}

/// Add empty slots for the next mapping key and value.
// Not inlined: for the same reason as `push_empty_item`.
#[inline(never)]
fn push_empty_entry(entries: &mut Vec<(Node<Span>, Node<Span>)>) -> &mut (Node<Span>, Node<Span>) {
    entries.push_mut((empty_scalar(), empty_scalar()))
}

/// The tag and the metadata a node takes from its event's properties.
fn node_properties(
    meta: Option<Box<EventMeta<'_>>>,
) -> (Option<Cow<'static, str>>, Option<Box<NodeMeta<Span>>>) {
    let (anchor, anchor_loc, tag, tag_loc) = unpack_meta(meta);
    (
        tag.map(|t| Cow::Owned(t.into_owned())),
        NodeMeta {
            anchor: anchor.map(str::to_owned),
            anchor_loc,
            tag_loc,
            leading_comments: None,
            trailing_comment: None,
        }
        .into_option(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::CollectionStyle;
    use rstest::rstest;

    #[test]
    fn loader_state_resets_anchor_map_between_documents() {
        // In resolved mode: anchor defined in doc 1 must not be visible in doc 2.
        let result = LoaderBuilder::new()
            .resolved()
            .build()
            .load("---\n- &foo hello\n...\n---\n- *foo\n...\n");
        assert!(
            result.is_err(),
            "expected Err: *foo in doc 2 should be undefined"
        );
        assert!(matches!(
            result.unwrap_err(),
            LoadError::UndefinedAlias { .. }
        ));
    }

    /// Load a hand-made event stream, as the loader would get it from the parser.
    fn load_events(
        events: Vec<Event<'static>>,
    ) -> std::result::Result<Vec<Document<Span>>, LoadError> {
        load_events_with(&LoaderOptions::default(), events)
    }

    fn load_events_with(
        options: &LoaderOptions,
        events: Vec<Event<'static>>,
    ) -> std::result::Result<Vec<Document<Span>>, LoadError> {
        let mut state = LoadState::new(options, "");
        let span = Span { start: 0, end: 0 };
        let stream: Box<dyn Iterator<Item = std::result::Result<(Event<'_>, Span), Error>>> =
            Box::new(events.into_iter().map(move |event| Ok((event, span))));
        state.run(stream.peekable())
    }

    fn document_of(body: Vec<Event<'static>>) -> Vec<Event<'static>> {
        let mut events = vec![
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: Vec::new(),
            },
        ];
        events.extend(body);
        events.push(Event::DocumentEnd { explicit: false });
        events.push(Event::StreamEnd);
        events
    }

    fn start_of(style: CollectionStyle, mapping: bool) -> Event<'static> {
        if mapping {
            Event::MappingStart { style, meta: None }
        } else {
            Event::SequenceStart { style, meta: None }
        }
    }

    /// A stream whose closing event does not close the open collection ends in an
    /// error.  The loader once added empty entries to the collection for ever,
    /// since a closing event is left in the stream for the collection to consume.
    #[rstest]
    #[case::mapping_closed_by_a_sequence_end(true, Event::SequenceEnd)]
    #[case::sequence_closed_by_a_mapping_end(false, Event::MappingEnd)]
    #[case::mapping_closed_by_a_document_end(true, Event::DocumentEnd { explicit: false })]
    #[case::sequence_closed_by_a_document_end(false, Event::DocumentEnd { explicit: false })]
    fn an_unbalanced_event_stream_is_an_error(
        #[case] mapping: bool,
        #[case] wrong: Event<'static>,
    ) {
        let events = document_of(vec![start_of(CollectionStyle::Flow, mapping), wrong]);
        let result = load_events(events);
        assert!(
            matches!(
                result,
                Err(LoadError::Parse {
                    kind: ErrorKind::Syntax,
                    ..
                })
            ),
            "{result:?}"
        );
    }

    /// A stream that stops inside a collection, and one with a closing event at
    /// the root, end in an error too: neither closes what it opened.
    #[rstest]
    #[case::stream_ends_in_a_sequence(vec![
        Event::StreamStart,
        Event::DocumentStart { explicit: false, version: None, tag_directives: Vec::new() },
        start_of(CollectionStyle::Flow, false),
        Event::StreamEnd,
    ])]
    #[case::stream_ends_in_a_mapping(vec![
        Event::StreamStart,
        Event::DocumentStart { explicit: false, version: None, tag_directives: Vec::new() },
        start_of(CollectionStyle::Flow, true),
        Event::StreamEnd,
    ])]
    #[case::stream_stops_in_a_mapping(vec![
        Event::StreamStart,
        Event::DocumentStart { explicit: false, version: None, tag_directives: Vec::new() },
        start_of(CollectionStyle::Flow, true),
    ])]
    #[case::stray_mapping_end_at_the_root(document_of(vec![Event::MappingEnd]))]
    #[case::stray_sequence_end_at_the_root(document_of(vec![Event::SequenceEnd]))]
    fn an_event_stream_that_does_not_close_what_it_opened_is_an_error(
        #[case] events: Vec<Event<'static>>,
    ) {
        let result = load_events(events);
        assert!(result.is_err(), "{result:?}");
    }

    /// A closing event where a key's value is expected closes the collection:
    /// the value is the empty scalar that the slot held, untouched by the
    /// schema, so it has neither a tag nor a position.
    #[test]
    fn a_key_without_a_value_gets_the_untouched_empty_scalar() {
        let events = document_of(vec![
            start_of(CollectionStyle::Block, true),
            scalar_of("k"),
            Event::MappingEnd,
        ]);
        let docs = load_events(events).unwrap();
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected a mapping, got {:?}", docs[0].root);
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, empty_scalar());
    }

    /// A closing event of the wrong kind where a value is expected is not
    /// consumed by the value: the collection's loop reports it.
    #[rstest]
    #[case::sequence_end_in_a_mapping(Event::SequenceEnd)]
    #[case::document_end_in_a_mapping(Event::DocumentEnd { explicit: false })]
    fn a_wrong_closing_event_where_a_value_is_expected_is_an_error(#[case] wrong: Event<'static>) {
        let events = document_of(vec![
            start_of(CollectionStyle::Block, true),
            scalar_of("k"),
            wrong,
        ]);
        assert_syntax_error(&load_events(events));
    }

    /// An event that opens or closes a stream or a document where a value is
    /// expected is consumed and stands for an empty value.
    #[rstest]
    #[case::stream_start(Event::StreamStart)]
    #[case::document_start(document_start())]
    #[case::stream_end(Event::StreamEnd)]
    fn a_stream_or_document_event_where_a_value_is_expected_is_an_empty_value(
        #[case] event: Event<'static>,
    ) {
        let events = document_of(vec![
            start_of(CollectionStyle::Block, true),
            scalar_of("k"),
            event,
            Event::MappingEnd,
        ]);
        let docs = load_events(events).unwrap();
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected a mapping, got {:?}", docs[0].root);
        };
        assert_eq!(entries[0].1, empty_scalar());
    }

    /// A stream that ends many levels down is an error, not a stack overflow
    /// or a half-built tree.
    #[test]
    fn a_stream_that_ends_deep_inside_collections_is_an_error() {
        let mut events = vec![Event::StreamStart, document_start()];
        events.extend((0..300).map(|level| start_of(CollectionStyle::Block, level % 2 == 0)));
        events.push(Event::StreamEnd);
        let result = on_stack(TWO_MIB, move || {
            let options = LoaderOptions {
                max_nesting_depth: 512,
                ..LoaderOptions::default()
            };
            load_events_with(&options, events)
        });
        assert!(
            matches!(result, Err(LoadError::UnexpectedEndOfStream)),
            "{result:?}"
        );
    }

    #[test]
    fn a_balanced_event_stream_loads() {
        let events = document_of(vec![
            start_of(CollectionStyle::Flow, false),
            start_of(CollectionStyle::Flow, true),
            Event::MappingEnd,
            Event::SequenceEnd,
        ]);
        assert!(load_events(events).is_ok());
    }

    fn scalar_of(value: &'static str) -> Event<'static> {
        Event::Scalar {
            value: Cow::Borrowed(value),
            style: ScalarStyle::Plain,
            meta: None,
        }
    }

    fn document_start() -> Event<'static> {
        Event::DocumentStart {
            explicit: false,
            version: None,
            tag_directives: Vec::new(),
        }
    }

    fn assert_syntax_error(result: &std::result::Result<Vec<Document<Span>>, LoadError>) {
        assert!(
            matches!(
                result,
                Err(LoadError::Parse {
                    kind: ErrorKind::Syntax,
                    ..
                })
            ),
            "{result:?}"
        );
    }

    /// A node after the root is a second root.  Skipping it would drop its text
    /// from the result, so the loader ends with an error instead.  The check
    /// peeks: an alias that nothing defines is still reported as a misplaced
    /// node, not as an undefined alias.
    #[rstest]
    #[case::scalar_root_then_scalar(vec![scalar_of("a"), scalar_of("b")])]
    #[case::scalar_root_then_alias(vec![scalar_of("a"), Event::Alias { name: "nope" }])]
    #[case::scalar_root_then_mapping_start(vec![scalar_of("a"), start_of(CollectionStyle::Flow, true)])]
    #[case::scalar_root_then_sequence_start(vec![scalar_of("a"), start_of(CollectionStyle::Flow, false)])]
    #[case::alias_root_then_scalar(vec![Event::Alias { name: "x" }, scalar_of("b")])]
    #[case::mapping_root_then_scalar(vec![
        start_of(CollectionStyle::Block, true),
        scalar_of("k"),
        scalar_of("v"),
        Event::MappingEnd,
        scalar_of("b"),
    ])]
    #[case::sequence_root_then_mapping(vec![
        start_of(CollectionStyle::Block, false),
        scalar_of("a"),
        Event::SequenceEnd,
        start_of(CollectionStyle::Block, true),
        Event::MappingEnd,
    ])]
    #[case::empty_sequence_root_then_alias(vec![
        start_of(CollectionStyle::Flow, false),
        Event::SequenceEnd,
        Event::Alias { name: "x" },
    ])]
    #[case::comment_between_the_roots(vec![
        scalar_of("a"),
        Event::Comment { text: " c" },
        scalar_of("b"),
    ])]
    #[case::three_roots(vec![scalar_of("a"), scalar_of("b"), scalar_of("c")])]
    fn a_second_root_after_the_root_is_an_error(#[case] body: Vec<Event<'static>>) {
        assert_syntax_error(&load_events(document_of(body)));
    }

    /// A node that no document holds is dropped by no path: not after the
    /// document's end, not before the first document, and not when the document
    /// has no end event.
    #[rstest]
    #[case::scalar_after_document_end(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        Event::DocumentEnd { explicit: false },
        scalar_of("b"),
        Event::StreamEnd,
    ])]
    #[case::sequence_start_after_document_end(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        Event::DocumentEnd { explicit: false },
        start_of(CollectionStyle::Flow, false),
        Event::StreamEnd,
    ])]
    #[case::alias_after_document_end(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        Event::DocumentEnd { explicit: false },
        Event::Alias { name: "x" },
        Event::StreamEnd,
    ])]
    #[case::scalar_before_the_first_document(vec![
        Event::StreamStart,
        scalar_of("a"),
        Event::StreamEnd,
    ])]
    #[case::scalar_after_a_comment_outside_documents(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        Event::DocumentEnd { explicit: false },
        Event::Comment { text: " c" },
        scalar_of("b"),
        Event::StreamEnd,
    ])]
    #[case::second_root_without_a_document_end(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        scalar_of("b"),
        Event::StreamEnd,
    ])]
    #[case::second_document_with_a_second_root(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        Event::DocumentEnd { explicit: false },
        document_start(),
        scalar_of("b"),
        scalar_of("c"),
        Event::DocumentEnd { explicit: false },
        Event::StreamEnd,
    ])]
    fn a_node_that_no_document_holds_is_an_error(#[case] events: Vec<Event<'static>>) {
        assert_syntax_error(&load_events(events));
    }

    /// The new checks reject node events only: a stream with one root per
    /// document still loads.
    #[rstest]
    #[case::two_documents(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        Event::DocumentEnd { explicit: false },
        document_start(),
        scalar_of("b"),
        Event::DocumentEnd { explicit: false },
        Event::StreamEnd,
    ], 2)]
    #[case::empty_document_then_another(vec![
        Event::StreamStart,
        document_start(),
        Event::DocumentEnd { explicit: false },
        document_start(),
        scalar_of("b"),
        Event::DocumentEnd { explicit: false },
        Event::StreamEnd,
    ], 2)]
    #[case::comment_after_the_root(document_of(vec![
        scalar_of("a"),
        Event::Comment { text: " c" },
    ]), 1)]
    #[case::root_without_a_document_end(vec![
        Event::StreamStart,
        document_start(),
        scalar_of("a"),
        Event::StreamEnd,
    ], 1)]
    fn a_stream_with_one_root_per_document_loads(
        #[case] events: Vec<Event<'static>>,
        #[case] documents: usize,
    ) {
        let result = load_events(events);
        assert!(
            matches!(&result, Ok(docs) if docs.len() == documents),
            "{result:?}"
        );
    }

    #[test]
    fn a_comment_after_the_root_stays_with_its_document() {
        let docs = load_events(document_of(vec![
            scalar_of("a"),
            Event::Comment { text: " c" },
        ]))
        .unwrap();
        assert_eq!(docs[0].end_comments, vec!["# c".to_owned()]);
    }

    #[test]
    fn an_explicit_document_end_is_kept() {
        let docs = load_events(vec![
            Event::StreamStart,
            document_start(),
            scalar_of("a"),
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ])
        .unwrap();
        assert!(docs[0].explicit_end);
    }

    #[test]
    fn register_anchor_increments_count() {
        let options = LoaderOptions {
            max_anchors: 2,
            ..LoaderOptions::default()
        };
        let mut state = LoadState::new(&options, "");
        let node = Node::Scalar {
            value: "x".to_owned(),
            style: ScalarStyle::Plain,
            tag: None,
            loc: Span { start: 0, end: 0 },
            meta: None,
        };
        let dummy_span = Span { start: 0, end: 0 };
        assert!(
            state
                .register_anchor("a".to_owned(), &node, dummy_span)
                .is_ok()
        );
        assert!(
            state
                .register_anchor("b".to_owned(), &node, dummy_span)
                .is_ok()
        );
        let err = state
            .register_anchor("c".to_owned(), &node, dummy_span)
            .expect_err("expected AnchorCountLimitExceeded");
        assert!(matches!(
            err,
            LoadError::AnchorCountLimitExceeded { limit: 2, .. }
        ));
    }

    #[test]
    fn expand_node_detects_circular_alias() {
        let options = LoaderOptions {
            mode: LoadMode::Resolved,
            ..LoaderOptions::default()
        };
        let mut state = LoadState::new(&options, "");
        // Insert a self-referential alias node.
        let alias_node = Node::Alias {
            name: "a".to_owned(),
            loc: Span { start: 0, end: 0 },
            leading_comments: None,
            trailing_comment: None,
        };
        state.anchor_map.insert("a".to_owned(), alias_node.clone());
        let mut in_progress = HashSet::new();
        let alias_loc = Span { start: 0, end: 0 };
        let result = state.expand_node(alias_node, &mut in_progress, alias_loc);
        assert!(
            matches!(result, Err(LoadError::CircularAlias { .. })),
            "expected CircularAlias, got: {result:?}"
        );
    }

    fn comment_count(node: &Node<Span>) -> usize {
        let own = node.leading_comments().len() + usize::from(node.trailing_comment().is_some());
        own + match node {
            Node::Mapping { entries, .. } => entries
                .iter()
                .map(|(key, value)| comment_count(key) + comment_count(value))
                .sum(),
            Node::Sequence { items, .. } => items.iter().map(comment_count).sum(),
            Node::Scalar { .. } | Node::Alias { .. } => 0,
        }
    }

    // Every alias expansion copies the node in the anchor map, so that copy
    // must not hold the comments of the definition site.
    #[test]
    fn resolved_anchor_map_holds_no_comments() {
        let options = LoaderOptions {
            mode: LoadMode::Resolved,
            ..LoaderOptions::default()
        };
        let input = "base: &a\n  # lead\n  - x # trail\n  # tail\n  - &b y # more\n";
        let mut state = LoadState::new(&options, input);
        let events: Box<dyn Iterator<Item = std::result::Result<(Event<'_>, Span), Error>> + '_> =
            Box::new(crate::parse_events(input));
        let docs = state.run(events.peekable()).unwrap();
        assert!(
            comment_count(&docs[0].root) > 0,
            "the definition keeps its comments"
        );
        for name in ["a", "b"] {
            let stored = state.anchor_map.get(name).unwrap();
            assert_eq!(comment_count(stored), 0, "anchor {name}");
        }
    }

    // -----------------------------------------------------------------------
    // Comment between mapping key and nested collection is attached to first nested entry
    // -----------------------------------------------------------------------

    #[test]
    fn comment_between_key_and_nested_mapping_is_attached_to_first_key() {
        let docs = load("outer:\n  # Style 1\n  inner: val\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        assert_eq!(entries.len(), 1);
        let (_outer_key, outer_value) = &entries[0];
        let Node::Mapping {
            entries: nested, ..
        } = outer_value
        else {
            panic!("expected nested mapping");
        };
        assert_eq!(nested.len(), 1);
        let (inner_key, _) = &nested[0];
        assert_eq!(
            inner_key.leading_comments(),
            &["# Style 1"],
            "comment should be attached to the first nested key"
        );
    }

    #[test]
    fn comment_between_key_and_nested_sequence_is_attached_to_first_item() {
        let docs = load("key:\n  # leading\n  - item1\n  - item2\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_key, seq_value) = &entries[0];
        let Node::Sequence { items, .. } = seq_value else {
            panic!("expected sequence value");
        };
        assert_eq!(
            items[0].leading_comments(),
            &["# leading"],
            "comment should be attached to first sequence item"
        );
    }

    #[test]
    fn multiple_comments_between_key_and_collection_all_preserved() {
        let docs = load("key:\n  # first\n  # second\n  - item\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_key, seq_value) = &entries[0];
        let Node::Sequence { items, .. } = seq_value else {
            panic!("expected sequence value");
        };
        assert_eq!(
            items[0].leading_comments(),
            &["# first", "# second"],
            "both comments should be on first item"
        );
    }

    #[test]
    fn comment_between_key_and_collection_does_not_corrupt_key_node() {
        let docs = load("outer:\n  # Style 1\n  inner: val\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (outer_key, _) = &entries[0];
        assert!(
            outer_key.leading_comments().is_empty(),
            "outer key should have no leading comments"
        );
        assert!(
            outer_key.trailing_comment().is_none(),
            "outer key should have no trailing comment"
        );
    }

    // -----------------------------------------------------------------------
    // Today's placement of comments between a mapping key and a non-collection
    // (or flow collection) value
    // -----------------------------------------------------------------------

    /// Collects `(label, leading comments)` for every node with leading
    /// comments, in document order. Scalars are labelled by value, aliases by
    /// anchor name, collections by kind.
    fn nodes_with_leading_comments(docs: &[Document<Span>]) -> Vec<(String, Vec<String>)> {
        fn walk(node: &Node<Span>, out: &mut Vec<(String, Vec<String>)>) {
            if !node.leading_comments().is_empty() {
                let label = match node {
                    Node::Scalar { value, .. } => value.clone(),
                    Node::Alias { name, .. } => name.clone(),
                    Node::Mapping { .. } => "mapping".to_owned(),
                    Node::Sequence { .. } => "sequence".to_owned(),
                };
                out.push((label, node.leading_comments().to_vec()));
            }
            match node {
                Node::Mapping { entries, .. } => {
                    for (key, value) in entries {
                        walk(key, out);
                        walk(value, out);
                    }
                }
                Node::Sequence { items, .. } => {
                    for item in items {
                        walk(item, out);
                    }
                }
                Node::Scalar { .. } | Node::Alias { .. } => {}
            }
        }
        let mut out = Vec::new();
        for doc in docs {
            walk(&doc.root, &mut out);
        }
        out
    }

    fn expected_placement(expected: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        expected
            .iter()
            .map(|(label, comments)| {
                (
                    (*label).to_owned(),
                    comments.iter().map(|c| (*c).to_owned()).collect(),
                )
            })
            .collect()
    }

    // Each case pins the node that receives the comments today. Comments
    // before a scalar, empty, alias, or empty flow value go to the next key of
    // the same mapping (or the parent collection's next entry); comments
    // before a non-empty flow collection go to its first element.
    #[rstest]
    #[case::scalar_plain("a:\n  # c\n  v\nb: 1\n", &[("b", &["# c"][..])])]
    #[case::scalar_multiple_comments(
        "a:\n  # c1\n  # c2\n  v\nb: 1\n",
        &[("b", &["# c1", "# c2"][..])]
    )]
    #[case::scalar_blank_line_between_comments(
        "a:\n  # c1\n\n  # c2\n  v\nb: 1\n",
        &[("b", &["# c1", "# c2"][..])]
    )]
    #[case::scalar_double_quoted("a:\n  # c\n  \"q\"\nb: 1\n", &[("b", &["# c"][..])])]
    #[case::scalar_literal_block("a:\n  # c\n  |\n  lit\nb: 1\n", &[("b", &["# c"][..])])]
    #[case::scalar_tagged("a:\n  # c\n  !!str t\nb: 1\n", &[("b", &["# c"][..])])]
    #[case::scalar_after_anchor_on_key_line("a: &x\n  # c\n  v\nb: 1\n", &[("b", &["# c"][..])])]
    #[case::scalar_last_entry_comment_reaches_no_node("a:\n  # c\n  v\n", &[])]
    #[case::scalar_in_nested_mapping_reaches_parent_sibling(
        "outer:\n  a:\n    # c\n    v\nz: 1\n",
        &[("z", &["# c"][..])]
    )]
    #[case::scalar_in_sequence_mapping_reaches_next_item(
        "- a:\n  # c\n  v\n- z\n",
        &[("z", &["# c"][..])]
    )]
    #[case::scalar_at_document_end_does_not_leak_into_next_document(
        "a:\n  # c\n  v\n---\nb: 1\n",
        &[]
    )]
    #[case::empty_value_comment_to_next_key("a:\n  # c\nb: 1\n", &[("b", &["# c"][..])])]
    #[case::empty_value_multiple_comments(
        "a:\n  # c1\n  # c2\nb: 1\n",
        &[("b", &["# c1", "# c2"][..])]
    )]
    #[case::empty_value_mixed_indentation(
        "a:\n  # c1\n# c2\nb: 1\n",
        &[("b", &["# c1", "# c2"][..])]
    )]
    #[case::empty_value_last_entry_comment_reaches_no_node("a:\n  # c\n", &[])]
    #[case::empty_value_mapping_ends_comment_goes_to_next_sequence_item(
        "- a:\n  # c\n- z\n",
        &[("z", &["# c"][..])]
    )]
    #[case::empty_value_multiple_comments_to_next_sequence_item(
        "- a:\n  # c1\n  # c2\n- z\n",
        &[("z", &["# c1", "# c2"][..])]
    )]
    #[case::empty_value_after_other_entry_in_sequence_mapping(
        "- a: 1\n  b:\n  # c\n- z\n",
        &[("z", &["# c"][..])]
    )]
    #[case::alias_comment_to_next_key(
        "x: &x 1\na:\n  # c\n  *x\nb: 2\n",
        &[("b", &["# c"][..])]
    )]
    #[case::alias_in_nested_mapping_reaches_parent_sibling(
        "x: &x 1\nouter:\n  a:\n    # c\n    *x\nz: 1\n",
        &[("z", &["# c"][..])]
    )]
    #[case::alias_in_sequence_mapping_reaches_next_item(
        "- &x 1\n- a:\n  # c\n  *x\n- z\n",
        &[("z", &["# c"][..])]
    )]
    #[case::alias_at_document_end_no_node_receives_comment("x: &x 1\na:\n  # c\n  *x\n", &[])]
    #[case::alias_at_document_end_does_not_leak_into_next_document(
        "x: &x 1\na:\n  # c\n  *x\n---\nb: 1\n",
        &[]
    )]
    #[case::flow_mapping_comment_goes_to_first_flow_key(
        "a:\n  # c\n  {k: v}\nb: 1\n",
        &[("k", &["# c"][..])]
    )]
    #[case::flow_mapping_multiple_comments(
        "a:\n  # c1\n  # c2\n  {k: v}\nb: 1\n",
        &[("k", &["# c1", "# c2"][..])]
    )]
    #[case::flow_sequence_comment_goes_to_first_item(
        "a:\n  # c\n  [1, 2]\nb: 1\n",
        &[("1", &["# c"][..])]
    )]
    #[case::flow_sequence_tail_comment_reaches_next_key(
        "a:\n  # c\n  [1, 2]\n  # tail\nb: 1\n",
        &[("1", &["# c"][..]), ("b", &["# tail"][..])]
    )]
    #[case::flow_mapping_in_sequence_mapping_ends_after_value(
        "- x\n- a:\n  # c\n  {k: v}\n- z\n",
        &[("k", &["# c"][..])]
    )]
    #[case::flow_sequence_in_sequence_mapping(
        "- a:\n  # c\n  [1]\n- z\n",
        &[("1", &["# c"][..])]
    )]
    #[case::empty_flow_mapping_comment_goes_to_next_key(
        "a:\n  # c\n  {}\nb: 1\n",
        &[("b", &["# c"][..])]
    )]
    #[case::empty_flow_sequence_comment_goes_to_next_key(
        "a:\n  # c\n  []\nb: 1\n",
        &[("b", &["# c"][..])]
    )]
    #[case::empty_flow_sequence_in_sequence_mapping_goes_to_next_item(
        "- a:\n  # c\n  []\n- z\n",
        &[("z", &["# c"][..])]
    )]
    #[case::same_line_comment_after_key_is_not_leading_comment(
        "a: # same-line\n  v\nb: 1\n",
        &[]
    )]
    #[case::same_line_then_own_line_comment(
        "a: # same-line\n  # own\n  v\nb: 1\n",
        &[("b", &["# own"][..])]
    )]
    #[case::literal_header_comment_is_not_leading_comment_of_next_key(
        "a: | # same-line\n  t\nb: 1\n",
        &[]
    )]
    #[case::folded_header_comment_is_not_leading_comment_of_next_key(
        "a: > # same-line\n  t\nb: 1\n",
        &[]
    )]
    #[case::block_scalar_item_header_comment_is_not_leading_comment_of_next_item(
        "- | # same-line\n  t\n- z\n",
        &[]
    )]
    #[case::empty_value_same_line_comment_is_not_leading_comment_of_next_key(
        "a: # same-line\nb: 1\n",
        &[]
    )]
    #[case::anchor_same_line_comment_is_not_leading_comment_of_nested_key(
        "a: &x # same-line\n  k: 1\nb: 1\n",
        &[]
    )]
    fn comment_between_key_and_value_placement(
        #[case] input: &str,
        #[case] expected: &[(&str, &[&str])],
    ) {
        let docs = load(input).unwrap();
        assert_eq!(
            nodes_with_leading_comments(&docs),
            expected_placement(expected),
            "input: {input:?}"
        );
    }

    #[rstest]
    #[case::alias_comment_to_next_key(
        "x: &x 1\na:\n  # c\n  *x\nb: 2\n",
        &[("b", &["# c"][..])]
    )]
    #[case::alias_at_document_end_no_node_receives_comment("x: &x 1\na:\n  # c\n  *x\n", &[])]
    fn comment_before_alias_value_resolved_mode_placement(
        #[case] input: &str,
        #[case] expected: &[(&str, &[&str])],
    ) {
        let docs = LoaderBuilder::new().resolved().build().load(input).unwrap();
        assert_eq!(
            nodes_with_leading_comments(&docs),
            expected_placement(expected),
            "input: {input:?}"
        );
    }

    #[test]
    fn comment_before_scalar_value_coexists_with_trailing_comment() {
        let docs = load("a:\n  # c\n  v # tr\nb: 1\n").unwrap();
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected root mapping");
        };
        let (_, value) = &entries[0];
        assert!(value.leading_comments().is_empty());
        assert_eq!(value.trailing_comment(), Some("# tr"));
        assert_eq!(
            nodes_with_leading_comments(&docs),
            expected_placement(&[("b", &["# c"][..])])
        );
    }

    #[test]
    fn no_comment_between_key_and_value_leaves_leading_comments_empty() {
        let docs = load("key:\n  inner: val\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_key, nested) = &entries[0];
        let Node::Mapping {
            entries: nested_entries,
            ..
        } = nested
        else {
            panic!("expected nested mapping");
        };
        let (inner_key, _) = &nested_entries[0];
        assert!(
            inner_key.leading_comments().is_empty(),
            "inner key should have no leading comments when there is no comment"
        );
    }

    // -----------------------------------------------------------------------
    // Long comment runs between a mapping key and its value
    // -----------------------------------------------------------------------

    const TWO_MIB: usize = 2 * 1024 * 1024;

    /// Runs `f` on a thread with a fixed stack size, so the stack budget does
    /// not depend on the test runner's thread or `RUST_MIN_STACK`.
    fn on_stack<T: Send + 'static>(bytes: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(bytes)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn input_with_comment_run(prefix: &str, count: usize, suffix: &str) -> String {
        use std::fmt::Write as _;
        let mut input = String::from(prefix);
        for i in 0..count {
            writeln!(input, "  # c{i}").unwrap();
        }
        input.push_str(suffix);
        input
    }

    fn comment_run(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("# c{i}")).collect()
    }

    #[test]
    fn long_comment_run_before_nested_mapping_loads_on_small_stack() {
        const COUNT: usize = 100_000;
        let (outer_key_comments, inner_key_comments) = on_stack(TWO_MIB, || {
            let input = input_with_comment_run("key:\n", COUNT, "  inner: val\n");
            let docs = load(&input).unwrap();
            let Node::Mapping { entries, .. } = &docs[0].root else {
                panic!("expected root mapping");
            };
            assert_eq!(entries.len(), 1);
            let (key, nested) = &entries[0];
            let Node::Mapping {
                entries: nested_entries,
                ..
            } = nested
            else {
                panic!("expected nested mapping");
            };
            assert_eq!(nested_entries.len(), 1);
            (
                key.leading_comments().to_vec(),
                nested_entries[0].0.leading_comments().to_vec(),
            )
        });
        assert!(outer_key_comments.is_empty());
        assert!(
            inner_key_comments == comment_run(COUNT),
            "inner key carries {} comments, expected {COUNT} in order",
            inner_key_comments.len()
        );
    }

    // Same placement as the short runs pinned above, for a run long enough to
    // overflow the stack when each comment costs a `begin_node` frame.
    #[rstest]
    #[case::nested_block_sequence("key:\n", "  - item\n", &["item"])]
    #[case::nested_flow_mapping("key:\n", "  {k: v}\nb: 1\n", &["k"])]
    #[case::nested_flow_sequence("key:\n", "  [1, 2]\n", &["1"])]
    #[case::scalar_value_goes_to_next_key("key:\n", "  v\nb: 1\n", &["b"])]
    #[case::empty_value_goes_to_next_key("key:\n", "b: 1\n", &["b"])]
    #[case::alias_value_goes_to_next_key("x: &x 1\na:\n", "  *x\nb: 2\n", &["b"])]
    #[case::mapping_end_reaches_next_item("- a:\n", "- z\n", &["z"])]
    #[case::last_entry_comments_reach_no_node("a:\n", "", &[])]
    #[case::document_end_comments_reach_no_node("a:\n", "---\nb: 1\n", &[])]
    fn long_comment_run_at_value_position_is_placed_like_short_run(
        #[case] prefix: &'static str,
        #[case] suffix: &'static str,
        #[case] expected: &[&str],
    ) {
        const COUNT: usize = 20_000;
        let placement = on_stack(TWO_MIB, move || {
            let input = input_with_comment_run(prefix, COUNT, suffix);
            nodes_with_leading_comments(&load(&input).unwrap())
        });
        let expected: Vec<(String, Vec<String>)> = expected
            .iter()
            .map(|label| ((*label).to_owned(), comment_run(COUNT)))
            .collect();
        assert!(
            placement == expected,
            "placement differs: got {:?}",
            placement
                .iter()
                .map(|(label, comments)| (label.as_str(), comments.len()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn comment_run_before_parse_error_returns_the_parse_error() {
        let err = load("a:\n  # c1\n  # c2\n  \"unterminated\n").unwrap_err();
        let LoadError::Parse { message, pos, .. } = err else {
            panic!("expected a parse error, got {err:?}");
        };
        assert!(message.contains("unterminated"), "message: {message}");
        assert_eq!(pos.line, 5);
    }

    #[test]
    fn comment_run_without_trailing_newline_at_end_of_input_reaches_no_node() {
        let docs = load("a:\n  # c1\n  # c2").unwrap();
        assert_eq!(nodes_with_leading_comments(&docs), []);
        assert_eq!(docs[0].end_comments, ["# c1", "# c2"]);
    }

    #[test]
    fn comment_run_before_value_with_crlf_line_endings_matches_lf() {
        let docs = load("a:\r\n  # c1\r\n  # c2\r\n  v\r\nb: 1\r\n").unwrap();
        assert_eq!(
            nodes_with_leading_comments(&docs),
            expected_placement(&[("b", &["# c1", "# c2"][..])])
        );
    }

    // -----------------------------------------------------------------------
    // Trailing comment of nested collection becomes leading comment on next sibling
    // -----------------------------------------------------------------------

    #[test]
    fn trailing_comment_of_sequence_preserved_as_leading_on_next_sibling() {
        let input =
            "Lists:\n  list-a:\n    - item1\n    - item2\n\n  # Style 2\n  list-b:\n    - item1\n";
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_lists_key, nested) = &entries[0];
        let Node::Mapping {
            entries: nested_entries,
            ..
        } = nested
        else {
            panic!("expected nested mapping");
        };
        assert_eq!(nested_entries.len(), 2);
        let (list_b_key, _) = &nested_entries[1];
        assert_eq!(
            list_b_key.leading_comments(),
            &["# Style 2"],
            "# Style 2 should be leading comment on list-b key"
        );
    }

    #[test]
    fn overflow_comments_from_nested_sequence_end_reach_next_mapping_entry() {
        let input = "outer:\n  a:\n    - x\n    # between\n  b: y\n";
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_outer_key, outer_val) = &entries[0];
        let Node::Mapping {
            entries: nested, ..
        } = outer_val
        else {
            panic!("expected nested mapping");
        };
        assert_eq!(nested.len(), 2);
        let (b_key, _) = &nested[1];
        assert_eq!(
            b_key.leading_comments(),
            &["# between"],
            "# between should be leading comment on b key"
        );
    }

    #[test]
    fn overflow_comments_from_nested_mapping_end_reach_next_sibling() {
        let input = "parent:\n  child1:\n    k: v\n    # end-of-child1\n  child2: val\n";
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_parent_key, parent_val) = &entries[0];
        let Node::Mapping {
            entries: siblings, ..
        } = parent_val
        else {
            panic!("expected parent mapping value");
        };
        assert_eq!(siblings.len(), 2);
        let (child2_key, _) = &siblings[1];
        assert_eq!(
            child2_key.leading_comments(),
            &["# end-of-child1"],
            "# end-of-child1 should be leading comment on child2 key"
        );
    }

    #[test]
    fn overflow_comments_at_top_level_sequence_end_are_not_lost() {
        let input = "items:\n  - a\n  - b\n  # tail\n";
        let docs = load(input).unwrap();
        // The document must parse successfully (no panic, no error).
        assert!(!docs.is_empty(), "document should parse without error");
        // The # tail comment must not cause data loss — the sequence items are intact.
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_items_key, seq_val) = &entries[0];
        let Node::Sequence { items, .. } = seq_val else {
            panic!("expected sequence value");
        };
        assert_eq!(items.len(), 2, "sequence items must not be lost");
    }

    #[test]
    fn no_overflow_comments_when_collection_ends_cleanly() {
        let docs = load("key:\n  - item1\n  - item2\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_key, seq_val) = &entries[0];
        let Node::Sequence { items, .. } = seq_val else {
            panic!("expected sequence value");
        };
        for item in items {
            assert!(
                item.leading_comments().is_empty(),
                "items should have no leading comments"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Combined scenarios
    // -----------------------------------------------------------------------

    #[test]
    fn original_bug_report_input_preserves_both_comments() {
        let input = "Lists:\n  # Style 1\n  list-a:\n    - item1\n    - item2\n\n  # Style 2\n  list-b:\n  - item1\n  - item2\n";
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_lists_key, nested) = &entries[0];
        let Node::Mapping {
            entries: nested_entries,
            ..
        } = nested
        else {
            panic!("expected nested mapping");
        };
        assert_eq!(nested_entries.len(), 2);
        let (first_key, _) = &nested_entries[0];
        let (second_key, _) = &nested_entries[1];
        assert_eq!(
            first_key.leading_comments(),
            &["# Style 1"],
            "list-a should have # Style 1 as leading comment"
        );
        assert_eq!(
            second_key.leading_comments(),
            &["# Style 2"],
            "list-b should have # Style 2 as leading comment"
        );
    }

    #[test]
    fn leading_and_trailing_comments_both_preserved_on_sibling_entries() {
        let input = "map:\n  # leading\n  key: value  # trailing\n  # next-leading\n  key2: v2\n";
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_map_key, map_val) = &entries[0];
        let Node::Mapping {
            entries: siblings, ..
        } = map_val
        else {
            panic!("expected mapping value");
        };
        assert_eq!(siblings.len(), 2);
        let (key1, val1) = &siblings[0];
        let (key2, _) = &siblings[1];
        assert_eq!(key1.leading_comments(), &["# leading"]);
        assert_eq!(val1.trailing_comment(), Some("# trailing"));
        assert_eq!(key2.leading_comments(), &["# next-leading"]);
    }

    #[test]
    fn deeply_nested_overflow_comments_reach_correct_sibling() {
        let input = "top:\n  mid:\n    - x\n    # deep-overflow\n  next: y\n";
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected root mapping");
        };
        let (_top_key, top_val) = &entries[0];
        let Node::Mapping {
            entries: top_entries,
            ..
        } = top_val
        else {
            panic!("expected top-level mapping");
        };
        assert_eq!(top_entries.len(), 2);
        let (next_key, _) = &top_entries[1];
        assert_eq!(
            next_key.leading_comments(),
            &["# deep-overflow"],
            "# deep-overflow should propagate from nested sequence to next sibling"
        );
    }

    // -----------------------------------------------------------------------
    // Document marker flags (explicit_start / explicit_end)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::bare_document("key: value\n", false, false)]
    #[case::start_marker_only("---\nkey: value\n", true, false)]
    #[case::end_marker_only("key: value\n...\n", false, true)]
    #[case::both_markers("---\nkey: value\n...\n", true, true)]
    #[case::empty_with_both_markers("---\n...\n", true, true)]
    fn document_marker_flags_match_input(
        #[case] input: &str,
        #[case] expected_start: bool,
        #[case] expected_end: bool,
    ) {
        let docs = load(input).expect("load failed");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].explicit_start, expected_start, "explicit_start");
        assert_eq!(docs[0].explicit_end, expected_end, "explicit_end");
    }

    #[test]
    fn multi_document_flags_are_independent() {
        let docs = load("doc1: a\n---\ndoc2: b\n...\n---\ndoc3: c\n").expect("load failed");
        assert_eq!(docs.len(), 3);
        assert!(!docs[0].explicit_start, "doc1 explicit_start");
        assert!(!docs[0].explicit_end, "doc1 explicit_end");
        assert!(docs[1].explicit_start, "doc2 explicit_start");
        assert!(docs[1].explicit_end, "doc2 explicit_end");
        assert!(docs[2].explicit_start, "doc3 explicit_start");
        assert!(!docs[2].explicit_end, "doc3 explicit_end");
    }

    // -----------------------------------------------------------------------
    // sanitize_scalar_for_error unit tests
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::newline("foo\nbar", '\n', "\\u000A", "foo\\u000Abar")]
    #[case::carriage_return("foo\rbar", '\r', "\\u000D", "foo\\u000Dbar")]
    #[case::null_byte("foo\0bar", '\0', "\\u0000", "foo\\u0000bar")]
    fn sanitize_replaces_control_char_with_escape(
        #[case] input: &str,
        #[case] raw_char: char,
        #[case] escape_seq: &str,
        #[case] expected: &str,
    ) {
        let result = sanitize_scalar_for_error(input);
        assert!(
            !result.contains(raw_char),
            "output must not contain the raw control character"
        );
        assert!(
            result.contains(escape_seq),
            "output must contain {escape_seq} escape, got: {result:?}"
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn sanitize_short_value_stored_verbatim() {
        let input = "hello";
        let result = sanitize_scalar_for_error(input);
        assert_eq!(result, "hello");
        assert!(
            !result.ends_with("..."),
            "short value must not be truncated"
        );
    }

    #[test]
    fn sanitize_value_at_exact_limit_not_truncated() {
        let input = "a".repeat(128);
        let result = sanitize_scalar_for_error(&input);
        assert_eq!(
            result.len(),
            128,
            "128-char input must produce 128-char output"
        );
        assert!(
            !result.ends_with("..."),
            "value at exact limit must not be truncated"
        );
    }

    #[test]
    fn sanitize_value_over_limit_truncated() {
        let input = "a".repeat(129);
        let result = sanitize_scalar_for_error(&input);
        assert!(
            result.ends_with("..."),
            "value over limit must end with '...'"
        );
        assert_eq!(
            result.len(),
            128 + 3,
            "truncated output must be 128 chars + 3 ellipsis chars"
        );
    }

    #[test]
    fn sanitize_multibyte_char_boundary_not_split() {
        let input: String = "中".repeat(127) + "ab"; // 129 chars total
        let result = sanitize_scalar_for_error(&input);
        assert!(
            result.ends_with("..."),
            "129-char multibyte input should be truncated"
        );
        let char_count = result.trim_end_matches("...").chars().count();
        assert_eq!(
            char_count, 128,
            "truncated portion must be exactly 128 chars"
        );
    }

    // -----------------------------------------------------------------------
    // Cow variant identity for resolver-injected vs user-authored tags
    // -----------------------------------------------------------------------

    fn load_root(input: &str) -> Node<Span> {
        load(input).expect("load failed").remove(0).root
    }

    fn node_tag(node: Node<Span>) -> Option<Cow<'static, str>> {
        match node {
            Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
                tag
            }
            Node::Alias { .. } => None,
        }
    }

    #[rstest]
    #[case::str_tag("hello\n")]
    #[case::int_tag("42\n")]
    #[case::null_tag("null\n")]
    #[case::map_tag("a: 1\n")]
    #[case::seq_tag("- a\n")]
    #[case::bare_excl_tag("! hello\n")]
    fn resolver_emitted_tag_is_borrowed(#[case] input: &str) {
        let tag = node_tag(load_root(input));
        assert!(
            matches!(tag, Some(Cow::Borrowed(_))),
            "resolver-emitted tag must be Borrowed, got: {tag:?}"
        );
    }

    #[rstest]
    #[case::scalar("!!str hello\n")]
    #[case::mapping("!!map\na: 1\n")]
    #[case::sequence("!!seq\n- a\n")]
    fn user_authored_tag_is_owned(#[case] input: &str) {
        let tag = node_tag(load_root(input));
        assert!(
            matches!(tag, Some(Cow::Owned(_))),
            "user-authored tag must be Owned, got: {tag:?}"
        );
    }

    #[test]
    fn alias_node_has_no_tag_field() {
        let docs = LoaderBuilder::new()
            .build()
            .load("- &a x\n- *a\n")
            .expect("load failed");
        let Node::Sequence { items, .. } = &docs[0].root else {
            panic!("expected root sequence");
        };
        assert!(
            matches!(items[1], Node::Alias { .. }),
            "second item must be Alias in lossless mode"
        );
    }

    #[test]
    fn tag_value_content_preserved_across_cow_variants() {
        let Node::Scalar {
            tag: tag_resolver, ..
        } = load_root("hello\n")
        else {
            panic!("expected scalar");
        };
        assert_eq!(tag_resolver.as_deref(), Some("tag:yaml.org,2002:str"));

        let Node::Scalar { tag: tag_user, .. } = load_root("!custom hello\n") else {
            panic!("expected scalar");
        };
        assert_eq!(tag_user.as_deref(), Some("!custom"));
    }

    // -----------------------------------------------------------------------
    // Loader correctly gates NodeMeta construction
    // -----------------------------------------------------------------------

    fn node_meta_is_none(node: &Node<Span>) -> bool {
        matches!(
            node,
            Node::Scalar { meta: None, .. }
                | Node::Mapping { meta: None, .. }
                | Node::Sequence { meta: None, .. }
        )
    }

    #[rstest]
    #[case::plain_scalar("hello\n")]
    #[case::plain_mapping("a: 1\n")]
    #[case::plain_sequence("- a\n")]
    fn loaded_node_with_no_meta_fields_has_meta_none(#[case] input: &str) {
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        assert!(
            node_meta_is_none(root),
            "plain node must have meta: None, got: {root:?}"
        );
    }

    #[test]
    fn loaded_anchored_scalar_has_meta_some() {
        let docs = load("- &foo bar\n").unwrap();
        let Node::Sequence { items, .. } = &docs[0].root else {
            panic!("expected root Sequence");
        };
        let item = &items[0];
        assert!(
            matches!(item, Node::Scalar { meta: Some(_), .. }),
            "anchored scalar must have meta: Some, got: {item:?}"
        );
        assert_eq!(item.anchor(), Some("foo"));
    }

    #[test]
    fn loaded_scalar_with_anchor_has_meta_some_with_anchor_loc() {
        let docs = load("&tag hello\n").unwrap();
        let root = &docs[0].root;
        assert!(
            matches!(root, Node::Scalar { meta: Some(_), .. }),
            "anchored scalar must have meta: Some"
        );
        assert!(
            root.anchor_loc().is_some(),
            "anchor_loc() must be Some for anchored scalar"
        );
    }

    // -----------------------------------------------------------------------
    // Property displacement promotion — combined anchor+tag on block collections
    // -----------------------------------------------------------------------

    #[rstest]
    // Block mapping
    #[case::block_mapping_anchor_only("&a\nk: v\n", Some("a"), false)]
    #[case::block_mapping_tag_only("!mytag\nk: v\n", None, true)]
    #[case::block_mapping_anchor_then_tag("&a !mytag\nk: v\n", Some("a"), true)]
    #[case::block_mapping_tag_then_anchor("!mytag &a\nk: v\n", Some("a"), true)]
    // Block sequence
    #[case::block_sequence_anchor_only("&a\n- item\n", Some("a"), false)]
    #[case::block_sequence_tag_only("!mytag\n- item\n", None, true)]
    #[case::block_sequence_anchor_then_tag("&a !mytag\n- item\n", Some("a"), true)]
    #[case::block_sequence_tag_then_anchor("!mytag &a\n- item\n", Some("a"), true)]
    // Flow mapping
    #[case::flow_mapping_anchor_only("&a {k: v}\n", Some("a"), false)]
    #[case::flow_mapping_tag_only("!mytag {k: v}\n", None, true)]
    #[case::flow_mapping_anchor_then_tag("&a !mytag {k: v}\n", Some("a"), true)]
    #[case::flow_mapping_tag_then_anchor("!mytag &a {k: v}\n", Some("a"), true)]
    // Flow sequence
    #[case::flow_sequence_anchor_only("&a [item]\n", Some("a"), false)]
    #[case::flow_sequence_tag_only("!mytag [item]\n", None, true)]
    #[case::flow_sequence_anchor_then_tag("&a !mytag [item]\n", Some("a"), true)]
    #[case::flow_sequence_tag_then_anchor("!mytag &a [item]\n", Some("a"), true)]
    fn combined_properties_attach_to_root_collection(
        #[case] input: &str,
        #[case] expected_anchor: Option<&str>,
        #[case] expected_has_tag: bool,
    ) {
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        assert_eq!(root.anchor(), expected_anchor, "anchor on root collection");
        assert_eq!(
            root.tag_loc().is_some(),
            expected_has_tag,
            "tag_loc on root collection"
        );
    }

    // Block collections: first child must not inherit anchor or tag from the root
    #[rstest]
    // Block mapping
    #[case::block_mapping_anchor_only("&a\nk: v\n")]
    #[case::block_mapping_tag_only("!mytag\nk: v\n")]
    #[case::block_mapping_anchor_then_tag("&a !mytag\nk: v\n")]
    #[case::block_mapping_tag_then_anchor("!mytag &a\nk: v\n")]
    // Block sequence
    #[case::block_sequence_anchor_only("&a\n- item\n")]
    #[case::block_sequence_tag_only("!mytag\n- item\n")]
    #[case::block_sequence_anchor_then_tag("&a !mytag\n- item\n")]
    #[case::block_sequence_tag_then_anchor("!mytag &a\n- item\n")]
    fn first_child_of_block_collection_has_no_properties(#[case] input: &str) {
        let docs = load(input).unwrap();
        let root = &docs[0].root;
        let first_child: &Node<Span> = match root {
            Node::Mapping { entries, .. } => &entries[0].0,
            Node::Sequence { items, .. } => &items[0],
            Node::Scalar { .. } | Node::Alias { .. } => panic!("expected block collection"),
        };
        assert_eq!(
            first_child.anchor(),
            None,
            "anchor must not appear on first child"
        );
        assert!(
            first_child.tag_loc().is_none(),
            "tag_loc must not appear on first child"
        );
    }

    // --- Alias registration smoke test ---

    #[test]
    fn anchor_on_block_mapping_with_tag_is_resolvable_via_alias() {
        let input = "root:\n  tagged: &a !mytag\n    k: v\n  ref: *a\n";
        let result = LoaderBuilder::new().resolved().build().load(input);
        assert!(
            result.is_ok(),
            "alias *a must resolve — anchor must be on the mapping, not lost to first key: {result:?}"
        );
    }
}
