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
//!   guards against alias bombs (Billion Laughs attack).
//!
//! Security controls (all active in both modes unless noted):
//! - `max_nesting_depth` — caps sequence/mapping nesting to prevent stack
//!   exhaustion (default 512).
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
use crate::node::{Document, Node, NodeMeta};
use crate::pos::{LineIndex, Pos, Span};
use crate::schema::{CollectionKind, Schema, resolve_collection, resolve_scalar};

use comments::{attach_leading_comments, attach_trailing_comment};
use reloc::reloc;
use stream::{
    consume_leading_comments, consume_leading_doc_comments, next_from, peek_trailing_comment,
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
    Resolved,
}

/// Security and behaviour options for the loader.
#[derive(Debug, Clone)]
pub struct LoaderOptions {
    /// Maximum mapping/sequence nesting depth before returning
    /// [`LoadError::NestingDepthLimitExceeded`] (default: 512).
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

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            max_nesting_depth: 512,
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
    /// Leading comments accumulated by `parse_node` when it encounters a
    /// `Comment` event between a mapping key and its value's collection start,
    /// or by a sequence/mapping loop when it hits End with leftover leading
    /// comments.  The next mapping/sequence loop iteration picks these up and
    /// prepends them to the next entry's leading comments.
    pending_leading: Vec<String>,
    /// Line index for the current document source; shared across all documents
    /// produced from the same input via `Arc` to avoid N full copies.
    line_index: Arc<LineIndex>,
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

        loop {
            // Skip any leading comments or unknown events before a document.
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

                    let mut doc_comments: Vec<String> = Vec::new();

                    // Consume leading comments at document level.
                    consume_leading_doc_comments(&mut stream, &mut doc_comments, &self.line_index)?;

                    // Parse root node (may be absent for empty documents).
                    let root = if is_document_end(stream.peek()) {
                        // Empty document — emit an empty scalar as root.
                        let mut node = empty_scalar();
                        apply_schema_to_node(&mut node, self.options.schema, &self.line_index)?;
                        node
                    } else {
                        self.parse_node(&mut stream)?
                    };

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
                        comments: doc_comments,
                        explicit_start: doc_explicit_start,
                        explicit_end: doc_explicit_end,
                        line_index: Some(self.line_index.clone()),
                    });
                }
                Some(_) => {
                    // Comment or any other stray event outside a document — skip.
                }
            }
        }

        Ok(docs)
    }

    /// Parse a single node from the stream.
    ///
    /// Advances the stream past the node (including end-of-container events).
    #[expect(
        clippy::too_many_lines,
        reason = "match-on-event-type; splitting would obscure flow"
    )]
    fn parse_node(&mut self, stream: &mut EventStream<'_>) -> Result<Node<Span>> {
        // Structural end events close the caller's collection loop — do NOT
        // consume them here.  Return an empty scalar and leave the event in
        // the stream so the outer mapping/sequence loop can see and consume it.
        if matches!(
            stream.peek(),
            Some(Ok((
                Event::MappingEnd | Event::SequenceEnd | Event::DocumentEnd { .. },
                _
            )))
        ) {
            return Ok(empty_scalar());
        }

        let Some((event, span)) = next_from(stream)? else {
            return Ok(empty_scalar());
        };

        match event {
            Event::Scalar { value, style, meta } => {
                let (anchor, anchor_loc, tag, tag_loc) = unpack_meta(meta);
                let anchor = anchor.map(str::to_owned);
                // Capture the anchor span before it moves into NodeMeta.
                let anchor_span = anchor_loc.unwrap_or(span);
                let mut node = Node::Scalar {
                    value: value.into_owned(),
                    style,
                    tag: tag.map(|t| Cow::Owned(t.into_owned())),
                    loc: span,
                    meta: NodeMeta {
                        anchor,
                        anchor_loc,
                        tag_loc,
                        leading_comments: None,
                        trailing_comment: None,
                    }
                    .into_option(),
                };
                apply_schema_to_node(&mut node, self.options.schema, &self.line_index)?;
                if let Some(name) = node.anchor() {
                    self.register_anchor(name.to_owned(), &node, anchor_span)?;
                }
                Ok(node)
            }

            Event::MappingStart { style, meta } => {
                let (event_anchor, anchor_loc, event_tag, tag_loc) = unpack_meta(meta);
                let anchor = event_anchor.map(str::to_owned);
                let tag = event_tag.map(|t| Cow::Owned(t.into_owned()));
                let anchor_for_registration = anchor.clone();
                // Capture the anchor span before it moves into NodeMeta.
                let anchor_span = anchor_loc.unwrap_or(span);

                self.depth += 1;
                if self.depth > self.options.max_nesting_depth {
                    return Err(LoadError::NestingDepthLimitExceeded {
                        limit: self.options.max_nesting_depth,
                        pos: span_start_to_pos(span.start, &self.line_index),
                    });
                }

                let mut entries: Vec<(Node<Span>, Node<Span>)> = Vec::new();
                let mut end_span = span;

                loop {
                    // Consume leading comments before the next key.  Also
                    // collect any comments that spilled over from a sibling
                    // value's collection end (stored in `pending_leading`).
                    let raw_leading = consume_leading_comments(stream)?;
                    let leading = if self.pending_leading.is_empty() {
                        raw_leading
                    } else {
                        let mut combined = std::mem::take(&mut self.pending_leading);
                        combined.extend(raw_leading);
                        combined
                    };

                    match stream.peek() {
                        None | Some(Ok((Event::MappingEnd | Event::StreamEnd, _))) => {
                            // Save any collected leading comments so the next
                            // sibling entry in the parent collection can inherit
                            // them (e.g. a comment just before MappingEnd that
                            // belongs to the following mapping entry).
                            if !leading.is_empty() {
                                self.pending_leading = leading;
                            }
                            break;
                        }
                        Some(Err(_)) => {
                            // Consume the error.
                            return Err(match stream.next() {
                                Some(Err(e)) => LoadError::Parse {
                                    pos: e.pos,
                                    message: e.message,
                                    kind: e.kind,
                                },
                                _ => LoadError::UnexpectedEndOfStream,
                            });
                        }
                        Some(Ok(_)) => {}
                    }

                    let mut key = self.parse_node(stream)?;
                    attach_leading_comments(&mut key, leading);

                    let mut value = self.parse_node(stream)?;

                    // Trailing comment on the value — peek for inline comment.
                    // Block scalars (literal `|` and folded `>`) consume trailing
                    // blank lines as part of chomping; their span.end falls on the
                    // first line after the scalar, which can coincide with the
                    // next comment's line number.  That would falsely attach a
                    // leading inter-node comment as a trailing inline comment.
                    // Block scalars never have an inline comment on their content
                    // lines, so skip trailing-comment detection for them.
                    if !is_block_scalar(&value)
                        && matches!(stream.peek(), Some(Ok((Event::Comment { .. }, _))))
                    {
                        let value_end_line = node_end_line(&value, &self.line_index);
                        if let Some(trail) =
                            peek_trailing_comment(stream, value_end_line, &self.line_index)?
                        {
                            attach_trailing_comment(&mut value, trail);
                        }
                    }

                    entries.push((key, value));
                }

                // Consume MappingEnd and capture its span.
                if let Some(Ok((Event::MappingEnd, end))) = stream.peek() {
                    end_span = *end;
                    let _ = stream.next();
                }
                self.depth -= 1;

                let mut node = Node::Mapping {
                    entries,
                    style,
                    tag,
                    loc: Span {
                        start: span.start,
                        end: end_span.end,
                    },
                    meta: NodeMeta {
                        anchor,
                        anchor_loc,
                        tag_loc,
                        leading_comments: None,
                        trailing_comment: None,
                    }
                    .into_option(),
                };
                apply_schema_to_node(&mut node, self.options.schema, &self.line_index)?;
                if let Some(name) = anchor_for_registration {
                    self.register_anchor(name, &node, anchor_span)?;
                }
                Ok(node)
            }

            Event::SequenceStart { style, meta } => {
                let (event_anchor, anchor_loc, event_tag, tag_loc) = unpack_meta(meta);
                let anchor = event_anchor.map(str::to_owned);
                let tag = event_tag.map(|t| Cow::Owned(t.into_owned()));
                let anchor_for_registration = anchor.clone();
                // Capture the anchor span before it moves into NodeMeta.
                let anchor_span = anchor_loc.unwrap_or(span);

                self.depth += 1;
                if self.depth > self.options.max_nesting_depth {
                    return Err(LoadError::NestingDepthLimitExceeded {
                        limit: self.options.max_nesting_depth,
                        pos: span_start_to_pos(span.start, &self.line_index),
                    });
                }

                let mut items: Vec<Node<Span>> = Vec::new();
                let mut end_span = span;

                loop {
                    // Collect leading comments before the next item.  Also
                    // collect any comments that spilled over from a sibling
                    // value's collection end (stored in `pending_leading`).
                    let raw_leading = consume_leading_comments(stream)?;
                    let leading = if self.pending_leading.is_empty() {
                        raw_leading
                    } else {
                        let mut combined = std::mem::take(&mut self.pending_leading);
                        combined.extend(raw_leading);
                        combined
                    };

                    match stream.peek() {
                        None | Some(Ok((Event::SequenceEnd | Event::StreamEnd, _))) => {
                            // Save any collected leading comments so the next
                            // sibling entry in the parent collection can inherit
                            // them (e.g. a comment just before SequenceEnd that
                            // belongs to the following sequence item or mapping
                            // entry in the parent).
                            if !leading.is_empty() {
                                self.pending_leading = leading;
                            }
                            break;
                        }
                        Some(Err(_)) => {
                            // Consume the error.
                            return Err(match stream.next() {
                                Some(Err(e)) => LoadError::Parse {
                                    pos: e.pos,
                                    message: e.message,
                                    kind: e.kind,
                                },
                                _ => LoadError::UnexpectedEndOfStream,
                            });
                        }
                        Some(Ok(_)) => {}
                    }

                    let mut item = self.parse_node(stream)?;
                    attach_leading_comments(&mut item, leading);

                    // Trailing comment on the item — peek for inline comment.
                    // Block scalars are excluded for the same reason as in the
                    // mapping path: their span.end can coincide with the next
                    // comment's line, falsely turning a leading comment into a
                    // trailing one.
                    if !is_block_scalar(&item)
                        && matches!(stream.peek(), Some(Ok((Event::Comment { .. }, _))))
                    {
                        let item_end_line = node_end_line(&item, &self.line_index);
                        if let Some(trail) =
                            peek_trailing_comment(stream, item_end_line, &self.line_index)?
                        {
                            attach_trailing_comment(&mut item, trail);
                        }
                    }

                    items.push(item);
                }

                // Consume SequenceEnd and capture its span.
                if let Some(Ok((Event::SequenceEnd, end))) = stream.peek() {
                    end_span = *end;
                    let _ = stream.next();
                }
                self.depth -= 1;

                let mut node = Node::Sequence {
                    items,
                    style,
                    tag,
                    loc: Span {
                        start: span.start,
                        end: end_span.end,
                    },
                    meta: NodeMeta {
                        anchor,
                        anchor_loc,
                        tag_loc,
                        leading_comments: None,
                        trailing_comment: None,
                    }
                    .into_option(),
                };
                apply_schema_to_node(&mut node, self.options.schema, &self.line_index)?;
                if let Some(name) = anchor_for_registration {
                    self.register_anchor(name, &node, anchor_span)?;
                }
                Ok(node)
            }

            Event::Alias { name } => {
                let name = name.to_owned();
                self.resolve_alias(&name, span)
            }

            Event::Comment { text } => {
                // Comment between a mapping key and its collection value (e.g.
                // `key:\n  # comment\n  subkey: val`).  The comment appears
                // after the key Scalar and before the MappingStart/SequenceStart
                // that begins the value.  Save it in `pending_leading` so the
                // first entry of the upcoming collection can inherit it.
                self.pending_leading.push(with_hash_prefix(text));
                self.parse_node(stream)
            }

            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::MappingEnd
            | Event::SequenceEnd => {
                // Structural event where a node is expected — return empty scalar.
                Ok(empty_scalar())
            }
        }
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
            self.anchor_map.insert(name, node.clone());
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
                self.expand_node(anchored, &mut in_progress, loc)
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

/// Return the line number of a node's span end position.
///
/// Used to determine whether the next `Comment` event is trailing (same line)
/// or leading (different line).
#[inline]
fn node_end_line(node: &Node<Span>, line_index: &LineIndex) -> u32 {
    let end_offset = match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => loc.end,
    };
    line_index.line_column(end_offset).0
}

/// Return `true` if the node is a block scalar (literal `|` or folded `>`).
///
/// Block scalars consume trailing blank lines as part of chomping, so their
/// `span.end` falls on the line *after* the last consumed line.  This means a
/// comment on the immediately following line has the same line number as
/// `span.end.line`, which would cause `peek_trailing_comment` to falsely
/// classify it as an inline trailing comment.  The caller uses this predicate
/// to skip trailing-comment detection for block scalars.
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
            let effective_tag = tag.as_deref().filter(|t| *t != "!");
            if let Some(resolved) =
                resolve_collection(schema, CollectionKind::Mapping, effective_tag)
            {
                *tag = Some(Cow::Borrowed(resolved.as_str()));
                if let Some(m) = meta.as_mut() {
                    m.tag_loc = None;
                    if m.is_all_none() {
                        *meta = None;
                    }
                }
            }
        }
        Node::Sequence { tag, meta, .. } => {
            let effective_tag = tag.as_deref().filter(|t| *t != "!");
            if let Some(resolved) =
                resolve_collection(schema, CollectionKind::Sequence, effective_tag)
            {
                *tag = Some(Cow::Borrowed(resolved.as_str()));
                if let Some(m) = meta.as_mut() {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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
