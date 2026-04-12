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

mod comments;
mod reloc;
mod stream;

use comments::{attach_leading_comments, attach_trailing_comment};
use reloc::reloc;
use stream::{
    consume_leading_comments, consume_leading_doc_comments, next_from, peek_trailing_comment,
};

use std::collections::{HashMap, HashSet};
use std::iter::Peekable;

use crate::error::Error;
use crate::event::{Event, ScalarStyle};
use crate::node::{Document, Node};
use crate::pos::{Pos, Span};

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Errors produced by the loader.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// The event stream contained a parse error.
    #[error("parse error at {pos:?}: {message}")]
    Parse { pos: Pos, message: String },

    /// The event stream ended unexpectedly mid-document.
    #[error("unexpected end of event stream")]
    UnexpectedEndOfStream,

    /// Nesting depth exceeded the configured limit.
    #[error("nesting depth limit exceeded (max: {limit})")]
    NestingDepthLimitExceeded { limit: usize },

    /// Too many distinct anchor names were defined.
    #[error("anchor count limit exceeded (max: {limit})")]
    AnchorCountLimitExceeded { limit: usize },

    /// Alias expansion produced more nodes than the configured limit.
    #[error("alias expansion node limit exceeded (max: {limit})")]
    AliasExpansionLimitExceeded { limit: usize },

    /// A circular alias reference was detected.
    #[error("circular alias reference: '{name}'")]
    CircularAlias { name: String },

    /// An alias referred to an anchor that was never defined.
    #[error("undefined alias: '{name}'")]
    UndefinedAlias { name: String },
}

// Convenience alias used inside the module.
type Result<T> = std::result::Result<T, LoadError>;

// Type alias for the peekable event stream used throughout the loader.
type EventStream<'a> =
    Peekable<Box<dyn Iterator<Item = std::result::Result<(Event<'a>, Span), Error>> + 'a>>;

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
    /// Maximum mapping/sequence nesting depth (default: 512).
    pub max_nesting_depth: usize,
    /// Maximum number of distinct anchor names per document (default: 10 000).
    pub max_anchors: usize,
    /// Maximum total nodes produced by alias expansion, resolved mode only
    /// (default: 1 000 000).
    pub max_expanded_nodes: usize,
    /// Loader mode.
    pub mode: LoadMode,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            max_nesting_depth: 512,
            max_anchors: 10_000,
            max_expanded_nodes: 1_000_000,
            mode: LoadMode::Lossless,
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
        let mut state = LoadState::new(&self.options);
        let iter: Box<dyn Iterator<Item = std::result::Result<(Event<'_>, Span), Error>> + '_> =
            Box::new(crate::parse_events(input));
        state.run(iter.peekable())
    }
}

// ---------------------------------------------------------------------------
// Convenience entry point
// ---------------------------------------------------------------------------

/// Load YAML text using lossless mode and default security limits.
///
/// Returns one `Document<Span>` per YAML document in the stream.
///
/// # Errors
///
/// Returns `Err` if the input contains a parse error or exceeds a security
/// limit (nesting depth or anchor count).
///
/// ```
/// use rlsp_yaml_parser::loader::load;
///
/// let docs = load("hello\n").unwrap();
/// assert_eq!(docs.len(), 1);
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
}

impl<'opt> LoadState<'opt> {
    fn new(options: &'opt LoaderOptions) -> Self {
        Self {
            options,
            anchor_map: HashMap::new(),
            anchor_count: 0,
            depth: 0,
            expanded_nodes: 0,
        }
    }

    fn reset_for_document(&mut self) {
        self.anchor_map.clear();
        self.anchor_count = 0;
        self.expanded_nodes = 0;
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
                });
            }
        }

        loop {
            // Skip any leading comments or unknown events before a document.
            match next_from(&mut stream)? {
                None | Some((Event::StreamEnd, _)) => break,
                Some((
                    Event::DocumentStart {
                        version,
                        tag_directives,
                        ..
                    },
                    _,
                )) => {
                    let doc_version = version;
                    let doc_tags = tag_directives;
                    self.reset_for_document();

                    let mut doc_comments: Vec<String> = Vec::new();

                    // Consume leading comments at document level.
                    consume_leading_doc_comments(&mut stream, &mut doc_comments)?;

                    // Parse root node (may be absent for empty documents).
                    let root = if is_document_end(stream.peek()) {
                        // Empty document — emit an empty scalar as root.
                        empty_scalar()
                    } else {
                        self.parse_node(&mut stream)?
                    };

                    // Consume DocumentEnd if present.
                    if matches!(stream.peek(), Some(Ok((Event::DocumentEnd { .. }, _)))) {
                        let _ = stream.next();
                    }

                    docs.push(Document {
                        root,
                        version: doc_version,
                        tags: doc_tags,
                        comments: doc_comments,
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
        let Some((event, span)) = next_from(stream)? else {
            return Ok(empty_scalar());
        };

        match event {
            Event::Scalar {
                value,
                style,
                anchor,
                tag,
            } => {
                let node = Node::Scalar {
                    value: value.into_owned(),
                    style,
                    anchor: anchor.map(str::to_owned),
                    tag: tag.map(std::borrow::Cow::into_owned),
                    loc: span,
                    leading_comments: Vec::new(),
                    trailing_comment: None,
                };
                if let Some(name) = node.anchor() {
                    self.register_anchor(name.to_owned(), node.clone())?;
                }
                Ok(node)
            }

            Event::MappingStart { anchor, tag, .. } => {
                let anchor = anchor.map(str::to_owned);
                let tag = tag.map(std::borrow::Cow::into_owned);

                self.depth += 1;
                if self.depth > self.options.max_nesting_depth {
                    return Err(LoadError::NestingDepthLimitExceeded {
                        limit: self.options.max_nesting_depth,
                    });
                }

                let mut entries: Vec<(Node<Span>, Node<Span>)> = Vec::new();
                let mut end_span = span;

                loop {
                    // Peek to detect MappingEnd or end of stream before
                    // consuming leading comments.
                    let leading = consume_leading_comments(stream)?;

                    match stream.peek() {
                        None | Some(Ok((Event::MappingEnd | Event::StreamEnd, _))) => break,
                        Some(Err(_)) => {
                            // Consume the error.
                            return Err(match stream.next() {
                                Some(Err(e)) => LoadError::Parse {
                                    pos: e.pos,
                                    message: e.message,
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
                    let value_end_line = node_end_line(&value);
                    if let Some(trail) = peek_trailing_comment(stream, value_end_line)? {
                        attach_trailing_comment(&mut value, trail);
                    }

                    entries.push((key, value));
                }

                // Consume MappingEnd and capture its span.
                if let Some(Ok((Event::MappingEnd, end))) = stream.peek() {
                    end_span = *end;
                    let _ = stream.next();
                }
                self.depth -= 1;

                let node = Node::Mapping {
                    entries,
                    anchor: anchor.clone(),
                    tag,
                    loc: Span {
                        start: span.start,
                        end: end_span.end,
                    },
                    leading_comments: Vec::new(),
                    trailing_comment: None,
                };
                if let Some(name) = anchor {
                    self.register_anchor(name, node.clone())?;
                }
                Ok(node)
            }

            Event::SequenceStart { anchor, tag, .. } => {
                let anchor = anchor.map(str::to_owned);
                let tag = tag.map(std::borrow::Cow::into_owned);

                self.depth += 1;
                if self.depth > self.options.max_nesting_depth {
                    return Err(LoadError::NestingDepthLimitExceeded {
                        limit: self.options.max_nesting_depth,
                    });
                }

                let mut items: Vec<Node<Span>> = Vec::new();
                let mut end_span = span;

                loop {
                    // Collect leading comments before the next item.
                    let leading = consume_leading_comments(stream)?;

                    match stream.peek() {
                        None | Some(Ok((Event::SequenceEnd | Event::StreamEnd, _))) => break,
                        Some(Err(_)) => {
                            // Consume the error.
                            return Err(match stream.next() {
                                Some(Err(e)) => LoadError::Parse {
                                    pos: e.pos,
                                    message: e.message,
                                },
                                _ => LoadError::UnexpectedEndOfStream,
                            });
                        }
                        Some(Ok(_)) => {}
                    }

                    let mut item = self.parse_node(stream)?;
                    attach_leading_comments(&mut item, leading);

                    // Trailing comment on the item — peek for inline comment.
                    let item_end_line = node_end_line(&item);
                    if let Some(trail) = peek_trailing_comment(stream, item_end_line)? {
                        attach_trailing_comment(&mut item, trail);
                    }

                    items.push(item);
                }

                // Consume SequenceEnd and capture its span.
                if let Some(Ok((Event::SequenceEnd, end))) = stream.peek() {
                    end_span = *end;
                    let _ = stream.next();
                }
                self.depth -= 1;

                let node = Node::Sequence {
                    items,
                    anchor: anchor.clone(),
                    tag,
                    loc: Span {
                        start: span.start,
                        end: end_span.end,
                    },
                    leading_comments: Vec::new(),
                    trailing_comment: None,
                };
                if let Some(name) = anchor {
                    self.register_anchor(name, node.clone())?;
                }
                Ok(node)
            }

            Event::Alias { name } => {
                let name = name.to_owned();
                self.resolve_alias(&name, span)
            }

            Event::Comment { .. } => {
                // Comment between nodes — skip and continue.
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

    fn register_anchor(&mut self, name: String, node: Node<Span>) -> Result<()> {
        if !self.anchor_map.contains_key(&name) {
            self.anchor_count += 1;
            if self.anchor_count > self.options.max_anchors {
                return Err(LoadError::AnchorCountLimitExceeded {
                    limit: self.options.max_anchors,
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
                });
            }
        }
        self.anchor_map.insert(name, node);
        Ok(())
    }

    fn resolve_alias(&mut self, name: &str, loc: Span) -> Result<Node<Span>> {
        match self.options.mode {
            LoadMode::Lossless => Ok(Node::Alias {
                name: name.to_owned(),
                loc,
                leading_comments: Vec::new(),
                trailing_comment: None,
            }),
            LoadMode::Resolved => {
                let anchored = self.anchor_map.get(name).cloned().ok_or_else(|| {
                    LoadError::UndefinedAlias {
                        name: name.to_owned(),
                    }
                })?;
                let mut in_progress: HashSet<String> = HashSet::new();
                self.expand_node(anchored, &mut in_progress)
            }
        }
    }

    /// Recursively expand a node, counting every node produced against the
    /// expansion limit and checking for cycles via `in_progress`.
    fn expand_node(
        &mut self,
        node: Node<Span>,
        in_progress: &mut HashSet<String>,
    ) -> Result<Node<Span>> {
        // Increment at the top — before child recursion — so every node
        // (including non-alias nodes inside expanded trees) counts against the
        // budget.
        self.expanded_nodes += 1;
        if self.expanded_nodes > self.options.max_expanded_nodes {
            return Err(LoadError::AliasExpansionLimitExceeded {
                limit: self.options.max_expanded_nodes,
            });
        }

        match node {
            Node::Alias { ref name, loc, .. } => {
                if in_progress.contains(name) {
                    return Err(LoadError::CircularAlias { name: name.clone() });
                }
                let target = self
                    .anchor_map
                    .get(name)
                    .cloned()
                    .ok_or_else(|| LoadError::UndefinedAlias { name: name.clone() })?;
                in_progress.insert(name.clone());
                let expanded = self.expand_node(target, in_progress)?;
                in_progress.remove(name);
                // Re-stamp with the alias site's location.
                Ok(reloc(expanded, loc))
            }
            Node::Mapping {
                entries,
                anchor,
                tag,
                loc,
                leading_comments,
                trailing_comment,
            } => {
                let mut expanded_entries = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    let ek = self.expand_node(k, in_progress)?;
                    let ev = self.expand_node(v, in_progress)?;
                    expanded_entries.push((ek, ev));
                }
                Ok(Node::Mapping {
                    entries: expanded_entries,
                    anchor,
                    tag,
                    loc,
                    leading_comments,
                    trailing_comment,
                })
            }
            Node::Sequence {
                items,
                anchor,
                tag,
                loc,
                leading_comments,
                trailing_comment,
            } => {
                let mut expanded_items = Vec::with_capacity(items.len());
                for item in items {
                    expanded_items.push(self.expand_node(item, in_progress)?);
                }
                Ok(Node::Sequence {
                    items: expanded_items,
                    anchor,
                    tag,
                    loc,
                    leading_comments,
                    trailing_comment,
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

/// Return the line number of a node's span end position.
///
/// Used to determine whether the next `Comment` event is trailing (same line)
/// or leading (different line).
const fn node_end_line(node: &Node<Span>) -> usize {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => loc.end.line,
    }
}

// ---------------------------------------------------------------------------
// Node helpers
// ---------------------------------------------------------------------------

const fn empty_scalar() -> Node<Span> {
    Node::Scalar {
        value: String::new(),
        style: ScalarStyle::Plain,
        anchor: None,
        tag: None,
        loc: Span {
            start: Pos::ORIGIN,
            end: Pos::ORIGIN,
        },
        leading_comments: Vec::new(),
        trailing_comment: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[expect(clippy::expect_used, clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    // UT-1: loader_state_resets_anchor_map_between_documents
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

    // UT-2: register_anchor_increments_count
    #[test]
    fn register_anchor_increments_count() {
        let options = LoaderOptions {
            max_anchors: 2,
            ..LoaderOptions::default()
        };
        let mut state = LoadState::new(&options);
        let node = Node::Scalar {
            value: "x".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: Span {
                start: Pos::ORIGIN,
                end: Pos::ORIGIN,
            },
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        assert!(state.register_anchor("a".to_owned(), node.clone()).is_ok());
        assert!(state.register_anchor("b".to_owned(), node.clone()).is_ok());
        let err = state
            .register_anchor("c".to_owned(), node)
            .expect_err("expected AnchorCountLimitExceeded");
        assert!(matches!(
            err,
            LoadError::AnchorCountLimitExceeded { limit: 2 }
        ));
    }

    // UT-3: expand_node_detects_circular_alias
    #[test]
    fn expand_node_detects_circular_alias() {
        let options = LoaderOptions {
            mode: LoadMode::Resolved,
            ..LoaderOptions::default()
        };
        let mut state = LoadState::new(&options);
        // Insert a self-referential alias node.
        let alias_node = Node::Alias {
            name: "a".to_owned(),
            loc: Span {
                start: Pos::ORIGIN,
                end: Pos::ORIGIN,
            },
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        state.anchor_map.insert("a".to_owned(), alias_node.clone());
        let mut in_progress = HashSet::new();
        let result = state.expand_node(alias_node, &mut in_progress);
        assert!(
            matches!(result, Err(LoadError::CircularAlias { .. })),
            "expected CircularAlias, got: {result:?}"
        );
    }
}
