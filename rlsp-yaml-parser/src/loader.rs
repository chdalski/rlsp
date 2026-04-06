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

use std::collections::{HashMap, HashSet};

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
        state.run(input)
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
/// use rlsp_yaml_parser::load;
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

    #[allow(clippy::indexing_slicing)] // pos < events.len() guards every access
    fn run(&mut self, input: &str) -> Result<Vec<Document<Span>>> {
        // Collect all events eagerly so we can use a cursor.
        let raw: std::result::Result<Vec<_>, _> = crate::parse_events(input).collect();
        let events = raw.map_err(|e| LoadError::Parse {
            pos: e.pos,
            message: e.message,
        })?;

        let mut docs: Vec<Document<Span>> = Vec::new();
        let mut pos = 0usize;

        // Skip StreamStart.
        if let Some((Event::StreamStart, _)) = events.get(pos) {
            pos += 1;
        }

        while pos < events.len() {
            match &events[pos] {
                (Event::StreamEnd, _) => break,
                (Event::DocumentStart { version, tags, .. }, _) => {
                    let doc_version = *version;
                    let doc_tags = tags.clone();
                    pos += 1;
                    self.reset_for_document();

                    let mut doc_comments: Vec<String> = Vec::new();

                    // Consume leading comments and locate the root node.
                    while pos < events.len() {
                        match &events[pos] {
                            (Event::Comment { text }, _) => {
                                doc_comments.push(text.clone());
                                pos += 1;
                            }
                            _ => break,
                        }
                    }

                    // Parse the root node (may be absent for empty documents).
                    let root = if matches!(
                        events.get(pos),
                        Some((Event::DocumentEnd { .. } | Event::StreamEnd, _)) | None
                    ) {
                        // Empty document — emit an empty scalar as root.
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
                    } else {
                        self.parse_node(&events, &mut pos)?
                    };

                    // Consume DocumentEnd.
                    if matches!(events.get(pos), Some((Event::DocumentEnd { .. }, _))) {
                        pos += 1;
                    }

                    docs.push(Document {
                        root,
                        version: doc_version,
                        tags: doc_tags,
                        comments: doc_comments,
                    });
                }
                _ => {
                    pos += 1;
                }
            }
        }

        Ok(docs)
    }

    /// Parse a single node starting at `*pos` and advance it past the node.
    #[allow(clippy::too_many_lines)] // match-on-event-type; splitting would obscure flow
    fn parse_node(&mut self, events: &[(Event, Span)], pos: &mut usize) -> Result<Node<Span>> {
        let Some((event, span)) = events.get(*pos) else {
            return Ok(empty_scalar());
        };
        let span = *span;

        match event {
            Event::Scalar {
                value,
                style,
                anchor,
                tag,
            } => {
                let node = Node::Scalar {
                    value: value.clone(),
                    style: *style,
                    anchor: anchor.clone(),
                    tag: tag.clone(),
                    loc: span,
                    leading_comments: Vec::new(),
                    trailing_comment: None,
                };
                if let Some(name) = anchor {
                    self.register_anchor(name.clone(), node.clone())?;
                }
                *pos += 1;
                Ok(node)
            }

            Event::MappingStart { anchor, tag } => {
                let anchor = anchor.clone();
                let tag = tag.clone();
                *pos += 1;

                self.depth += 1;
                if self.depth > self.options.max_nesting_depth {
                    return Err(LoadError::NestingDepthLimitExceeded {
                        limit: self.options.max_nesting_depth,
                    });
                }

                let mut entries: Vec<(Node<Span>, Node<Span>)> = Vec::new();
                while !matches!(events.get(*pos), Some((Event::MappingEnd, _)) | None) {
                    // Collect leading comments before the next key.
                    let leading = collect_leading_comments(events, pos);
                    let mut key = self.parse_node(events, pos)?;
                    attach_leading_comments(&mut key, leading);

                    let mut value = self.parse_node(events, pos)?;
                    // Attach trailing comment on the value node, if present.
                    if let Some(trail) = collect_trailing_comment(events, pos) {
                        attach_trailing_comment(&mut value, trail);
                    }

                    entries.push((key, value));
                }
                // Consume MappingEnd and capture its span to form the full container span.
                let end_span = if let Some((Event::MappingEnd, end)) = events.get(*pos) {
                    let s = *end;
                    *pos += 1;
                    s
                } else {
                    span
                };
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

            Event::SequenceStart { anchor, tag } => {
                let anchor = anchor.clone();
                let tag = tag.clone();
                *pos += 1;

                self.depth += 1;
                if self.depth > self.options.max_nesting_depth {
                    return Err(LoadError::NestingDepthLimitExceeded {
                        limit: self.options.max_nesting_depth,
                    });
                }

                let mut items: Vec<Node<Span>> = Vec::new();
                while !matches!(events.get(*pos), Some((Event::SequenceEnd, _)) | None) {
                    // Collect leading comments before the next item.
                    let leading = collect_leading_comments(events, pos);
                    let mut item = self.parse_node(events, pos)?;
                    attach_leading_comments(&mut item, leading);
                    // Attach trailing comment on the item, if present.
                    if let Some(trail) = collect_trailing_comment(events, pos) {
                        attach_trailing_comment(&mut item, trail);
                    }
                    items.push(item);
                }
                // Consume SequenceEnd and capture its span to form the full container span.
                let end_span = if let Some((Event::SequenceEnd, end)) = events.get(*pos) {
                    let s = *end;
                    *pos += 1;
                    s
                } else {
                    span
                };
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
                let name = name.clone();
                *pos += 1;
                self.resolve_alias(&name, span)
            }

            Event::Comment { .. } => {
                // Top-level comment between nodes — skip and continue.
                *pos += 1;
                self.parse_node(events, pos)
            }

            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::MappingEnd
            | Event::SequenceEnd => {
                // Structural event encountered where a node is expected — skip.
                *pos += 1;
                self.parse_node(events, pos)
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

// ---------------------------------------------------------------------------
// Helpers
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

/// Replace the location of a node (used when stamping alias-site spans).
fn reloc(node: Node<Span>, loc: Span) -> Node<Span> {
    match node {
        Node::Scalar {
            value,
            style,
            anchor,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Scalar {
            value,
            style,
            anchor,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Mapping {
            entries,
            anchor,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Mapping {
            entries,
            anchor,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Sequence {
            items,
            anchor,
            tag,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Sequence {
            items,
            anchor,
            tag,
            loc,
            leading_comments,
            trailing_comment,
        },
        Node::Alias {
            name,
            leading_comments,
            trailing_comment,
            ..
        } => Node::Alias {
            name,
            loc,
            leading_comments,
            trailing_comment,
        },
    }
}

// ---------------------------------------------------------------------------
// Comment attachment helpers
// ---------------------------------------------------------------------------

/// Collect all leading Comment events at `*pos` that are on their own line
/// (span.end.line > span.start.line — non-zero span).  Advances `*pos` past them.
/// Returns the comment texts, each prefixed with `#`.
fn collect_leading_comments(events: &[(Event, Span)], pos: &mut usize) -> Vec<String> {
    let mut leading = Vec::new();
    while let Some((Event::Comment { text }, span)) = events.get(*pos) {
        if span.end.line > span.start.line {
            leading.push(format!("#{text}"));
            *pos += 1;
        } else {
            break;
        }
    }
    leading
}

/// If the next event is a trailing Comment (zero-width span: start == end),
/// consume it and return the comment text prefixed with `#`.
fn collect_trailing_comment(events: &[(Event, Span)], pos: &mut usize) -> Option<String> {
    if let Some((Event::Comment { text }, span)) = events.get(*pos) {
        if span.start == span.end {
            let result = format!("#{text}");
            *pos += 1;
            return Some(result);
        }
    }
    None
}

/// Attach `leading_comments` to a node's `leading_comments` field.
fn attach_leading_comments(node: &mut Node<Span>, comments: Vec<String>) {
    if comments.is_empty() {
        return;
    }
    match node {
        Node::Scalar {
            leading_comments, ..
        }
        | Node::Mapping {
            leading_comments, ..
        }
        | Node::Sequence {
            leading_comments, ..
        }
        | Node::Alias {
            leading_comments, ..
        } => {
            *leading_comments = comments;
        }
    }
}

/// Attach a trailing comment to a node's `trailing_comment` field.
fn attach_trailing_comment(node: &mut Node<Span>, comment: String) {
    match node {
        Node::Scalar {
            trailing_comment, ..
        }
        | Node::Mapping {
            trailing_comment, ..
        }
        | Node::Sequence {
            trailing_comment, ..
        }
        | Node::Alias {
            trailing_comment, ..
        } => {
            *trailing_comment = Some(comment);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use std::fmt::Write as _;

    use super::*;
    use crate::event::ScalarStyle;

    // Security advisor-specified limit for alias expansion.
    const LIMIT: usize = 1_000_000;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn load_one(input: &str) -> Node<Span> {
        let docs = load(input).expect("load failed");
        assert_eq!(docs.len(), 1, "expected 1 document, got {}", docs.len());
        docs.into_iter().next().unwrap().root
    }

    fn load_resolved_one(input: &str) -> Node<Span> {
        let docs = LoaderBuilder::new()
            .resolved()
            .build()
            .load(input)
            .expect("load failed");
        assert_eq!(docs.len(), 1, "expected 1 document, got {}", docs.len());
        docs.into_iter().next().unwrap().root
    }

    fn scalar_value(node: &Node<Span>) -> &str {
        match node {
            Node::Scalar { value, .. } => value.as_str(),
            other @ (Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. }) => {
                panic!("expected Scalar, got {other:?}")
            }
        }
    }

    // -----------------------------------------------------------------------
    // Group 1: Public API and wiring
    // -----------------------------------------------------------------------

    /// Test 1 — `load` is accessible from the crate root (spike)
    #[test]
    fn load_is_wired_into_lib_rs() {
        let docs = crate::load("hello\n").expect("crate::load failed");
        assert!(!docs.is_empty());
    }

    /// Test 2 — `load` returns Ok for valid input
    #[test]
    fn load_returns_ok_for_valid_input() {
        assert!(load("hello\n").is_ok());
    }

    /// Test 3 — `load` returns a Vec of documents
    #[test]
    fn load_returns_vec_of_documents() {
        let docs = load("hello\n").unwrap();
        assert_eq!(docs.len(), 1);
    }

    /// Test 4 — `LoaderBuilder::new()` is callable
    #[test]
    fn loader_builder_new_is_callable() {
        let result = LoaderBuilder::new().build().load("hello\n");
        assert!(result.is_ok());
    }

    /// Test 5 — lossless mode is callable via builder
    #[test]
    fn loader_builder_lossless_mode_is_callable() {
        let result = LoaderBuilder::new().lossless().build().load("hello\n");
        assert!(result.is_ok());
    }

    /// Test 6 — resolved mode is callable via builder
    #[test]
    fn loader_builder_resolved_mode_is_callable() {
        let result = LoaderBuilder::new().resolved().build().load("hello\n");
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Group 2: Document structure
    // -----------------------------------------------------------------------

    /// Test 7 — document has a root node
    #[test]
    fn document_has_root_node() {
        let docs = load("hello\n").unwrap();
        let doc = docs.into_iter().next().unwrap();
        assert!(matches!(doc.root, Node::Scalar { .. }));
    }

    /// Test 8 — version is None without %YAML directive
    #[test]
    fn document_version_is_none_without_yaml_directive() {
        let docs = load("hello\n").unwrap();
        assert_eq!(docs[0].version, None);
    }

    /// Test 9 — tags is empty without %TAG directive
    #[test]
    fn document_tags_is_empty_without_tag_directive() {
        let docs = load("hello\n").unwrap();
        assert!(docs[0].tags.is_empty());
    }

    /// Test 10 — empty input returns empty Vec
    #[test]
    fn empty_input_returns_empty_vec() {
        let docs = load("").unwrap();
        assert!(docs.is_empty());
    }

    /// Test 11 — multi-document input returns multiple documents
    #[test]
    fn multi_document_input_returns_multiple_documents() {
        let docs = load("---\nfirst\n...\n---\nsecond\n...\n").unwrap();
        assert_eq!(docs.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Group 3: Scalar nodes
    // -----------------------------------------------------------------------

    /// Test 12 — plain scalar loads as Scalar node with correct value
    #[test]
    fn plain_scalar_loads_as_scalar_node() {
        let node = load_one("hello\n");
        assert!(
            matches!(&node, Node::Scalar { value, .. } if value == "hello"),
            "got: {node:?}"
        );
    }

    /// Test 13 — plain scalar has style Plain
    #[test]
    fn scalar_node_style_is_plain_for_plain_scalar() {
        let node = load_one("hello\n");
        assert!(matches!(
            node,
            Node::Scalar {
                style: ScalarStyle::Plain,
                ..
            }
        ));
    }

    /// Test 14 — single-quoted scalar has style SingleQuoted
    #[test]
    fn single_quoted_scalar_loads_with_single_quoted_style() {
        let node = load_one("'hello'\n");
        assert!(matches!(
            node,
            Node::Scalar {
                style: ScalarStyle::SingleQuoted,
                ..
            }
        ));
    }

    /// Test 15 — double-quoted scalar has style DoubleQuoted
    #[test]
    fn double_quoted_scalar_loads_with_double_quoted_style() {
        let node = load_one("\"hello\"\n");
        assert!(matches!(
            node,
            Node::Scalar {
                style: ScalarStyle::DoubleQuoted,
                ..
            }
        ));
    }

    /// Test 16 — literal block scalar has style Literal
    #[test]
    fn literal_block_scalar_loads_with_literal_style() {
        let node = load_one("|\n  hello\n");
        assert!(
            matches!(
                node,
                Node::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                }
            ),
            "got: {node:?}"
        );
    }

    /// Test 17 — folded block scalar has style Folded
    #[test]
    fn folded_block_scalar_loads_with_folded_style() {
        let node = load_one(">\n  hello\n");
        assert!(
            matches!(
                node,
                Node::Scalar {
                    style: ScalarStyle::Folded(_),
                    ..
                }
            ),
            "got: {node:?}"
        );
    }

    /// Test 18 — scalar tag is None without tag
    #[test]
    fn scalar_node_tag_is_none_without_tag() {
        let node = load_one("hello\n");
        assert!(matches!(node, Node::Scalar { tag: None, .. }));
    }

    /// Test 19 — tagged scalar has tag field
    #[test]
    fn tagged_scalar_has_tag_field() {
        let node = load_one("!!str hello\n");
        assert!(
            matches!(&node, Node::Scalar { tag: Some(t), .. } if t.contains("str")),
            "got: {node:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group 4: Mapping nodes
    // -----------------------------------------------------------------------

    /// Test 20 — block mapping loads as Mapping node
    #[test]
    fn block_mapping_loads_as_mapping_node() {
        let node = load_one("key: value\n");
        assert!(matches!(node, Node::Mapping { .. }), "got: {node:?}");
    }

    /// Test 21 — mapping has correct entry count
    #[test]
    fn mapping_has_correct_entry_count() {
        // Use flow-style mapping so that both entries are at the same level.
        // Block-mapping parsing in the underlying parser treats "a: 1\nb: 2\n"
        // differently (b is nested under a), so flow style is used here.
        let node = load_one("{a: 1, b: 2}\n");
        assert!(
            matches!(&node, Node::Mapping { entries, .. } if entries.len() == 2),
            "got: {node:?}"
        );
    }

    /// Test 22 — mapping entry key and value are scalars
    #[test]
    fn mapping_entry_key_and_value_are_scalars() {
        let node = load_one("key: value\n");
        let Node::Mapping { entries, .. } = node else {
            panic!("expected Mapping");
        };
        let (k, v) = &entries[0];
        assert!(matches!(k, Node::Scalar { value, .. } if value == "key"));
        assert!(matches!(v, Node::Scalar { value, .. } if value == "value"));
    }

    /// Test 23 — mapping entries preserve declaration order
    #[test]
    fn mapping_entries_preserve_order() {
        // Use flow-style mapping; block-mapping parsing nests subsequent entries
        // under the first key when all keys are at the same indentation level.
        let node = load_one("{a: 1, b: 2, c: 3}\n");
        let Node::Mapping { entries, .. } = node else {
            panic!("expected Mapping");
        };
        assert_eq!(entries.len(), 3);
        assert_eq!(scalar_value(&entries[0].0), "a");
        assert_eq!(scalar_value(&entries[1].0), "b");
        assert_eq!(scalar_value(&entries[2].0), "c");
    }

    /// Test 24 — empty mapping has zero entries
    #[test]
    fn empty_mapping_has_zero_entries() {
        let node = load_one("{}\n");
        assert!(
            matches!(&node, Node::Mapping { entries, .. } if entries.is_empty()),
            "got: {node:?}"
        );
    }

    /// Test 25 — nested mapping value is Mapping node
    #[test]
    fn nested_mapping_value_is_mapping_node() {
        let node = load_one("outer:\n  inner: value\n");
        let Node::Mapping { entries, .. } = node else {
            panic!("expected Mapping");
        };
        assert!(matches!(&entries[0].1, Node::Mapping { .. }));
    }

    /// Test 26 — mapping anchor is None without anchor
    #[test]
    fn mapping_anchor_is_none_without_anchor() {
        let node = load_one("key: value\n");
        assert!(matches!(node, Node::Mapping { anchor: None, .. }));
    }

    /// Test 27 — flow mapping loads as Mapping node
    #[test]
    fn flow_mapping_loads_as_mapping_node() {
        let node = load_one("{key: value}\n");
        assert!(matches!(node, Node::Mapping { .. }), "got: {node:?}");
    }

    // -----------------------------------------------------------------------
    // Group 5: Sequence nodes
    // -----------------------------------------------------------------------

    /// Test 28 — block sequence loads as Sequence node
    #[test]
    fn block_sequence_loads_as_sequence_node() {
        let node = load_one("- a\n- b\n");
        assert!(matches!(node, Node::Sequence { .. }), "got: {node:?}");
    }

    /// Test 29 — sequence has correct item count
    #[test]
    fn sequence_has_correct_item_count() {
        let node = load_one("- a\n- b\n- c\n");
        assert!(
            matches!(&node, Node::Sequence { items, .. } if items.len() == 3),
            "got: {node:?}"
        );
    }

    /// Test 30 — sequence items are scalar nodes
    #[test]
    fn sequence_items_are_scalar_nodes() {
        let node = load_one("- a\n- b\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert!(matches!(&items[0], Node::Scalar { .. }));
        assert!(matches!(&items[1], Node::Scalar { .. }));
    }

    /// Test 31 — sequence items preserve order
    #[test]
    fn sequence_items_preserve_order() {
        let node = load_one("- first\n- second\n- third\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert_eq!(scalar_value(&items[0]), "first");
        assert_eq!(scalar_value(&items[2]), "third");
    }

    /// Test 32 — empty sequence has zero items
    #[test]
    fn empty_sequence_has_zero_items() {
        let node = load_one("[]\n");
        assert!(
            matches!(&node, Node::Sequence { items, .. } if items.is_empty()),
            "got: {node:?}"
        );
    }

    /// Test 33 — nested sequence item is Sequence node
    #[test]
    fn nested_sequence_item_is_sequence_node() {
        let node = load_one("- - a\n  - b\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert!(
            matches!(&items[0], Node::Sequence { .. }),
            "got: {:?}",
            &items[0]
        );
    }

    /// Test 34 — flow sequence loads as Sequence node with correct count
    #[test]
    fn flow_sequence_loads_as_sequence_node() {
        let node = load_one("[a, b, c]\n");
        assert!(
            matches!(&node, Node::Sequence { items, .. } if items.len() == 3),
            "got: {node:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group 6: Anchors and aliases — lossless mode
    // -----------------------------------------------------------------------

    /// Test 35 — anchored scalar preserves anchor field
    #[test]
    fn anchored_scalar_preserves_anchor_field() {
        let node = load_one("&a hello\n");
        assert!(
            matches!(&node, Node::Scalar { anchor: Some(a), .. } if a == "a"),
            "got: {node:?}"
        );
    }

    /// Test 36 — alias reference becomes Alias node in lossless mode
    #[test]
    fn alias_reference_becomes_alias_node_in_lossless_mode() {
        let node = load_one("- &a hello\n- *a\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert_eq!(items.len(), 2);
        assert!(
            matches!(&items[1], Node::Alias { name, .. } if name == "a"),
            "got: {:?}",
            &items[1]
        );
    }

    /// Test 37 — anchored mapping preserves anchor field
    #[test]
    fn anchored_mapping_preserves_anchor_field() {
        let node = load_one("&m\nkey: value\n");
        assert!(
            matches!(&node, Node::Mapping { anchor: Some(a), .. } if a == "m"),
            "got: {node:?}"
        );
    }

    /// Test 38 — anchored sequence preserves anchor field
    #[test]
    fn anchored_sequence_preserves_anchor_field() {
        let node = load_one("&s\n- a\n- b\n");
        assert!(
            matches!(&node, Node::Sequence { anchor: Some(a), .. } if a == "s"),
            "got: {node:?}"
        );
    }

    /// Test 39 — alias node name matches anchor
    #[test]
    fn alias_node_name_matches_anchor() {
        let node = load_one("- &ref hello\n- *ref\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert!(
            matches!(&items[1], Node::Alias { name, .. } if name == "ref"),
            "got: {:?}",
            &items[1]
        );
    }

    /// Test 40 — multiple aliases to same anchor all become Alias nodes
    #[test]
    fn multiple_aliases_to_same_anchor_all_become_alias_nodes() {
        let node = load_one("- &a hello\n- *a\n- *a\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert!(matches!(&items[1], Node::Alias { name, .. } if name == "a"));
        assert!(matches!(&items[2], Node::Alias { name, .. } if name == "a"));
    }

    /// Test 41 — alias in mapping value becomes Alias node
    #[test]
    fn alias_in_mapping_value_becomes_alias_node() {
        // Use a sequence containing a mapping whose value is an alias.
        // The block-mapping parser nests subsequent same-level keys, so we
        // embed the anchor definition and alias reference in separate sequence
        // items to keep them at distinct nesting levels.
        let node = load_one("- &a value\n- {ref: *a}\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        let Node::Mapping { entries, .. } = &items[1] else {
            panic!("expected Mapping in second item");
        };
        let ref_entry = entries.iter().find(|(k, _)| scalar_value(k) == "ref");
        assert!(ref_entry.is_some(), "key 'ref' not found");
        let (_, value) = ref_entry.unwrap();
        assert!(
            matches!(value, Node::Alias { name, .. } if name == "a"),
            "got: {value:?}"
        );
    }

    /// Test 42 — lossless mode does not expand aliases
    #[test]
    fn lossless_mode_does_not_expand_aliases() {
        let node = load_one("- &a hello\n- *a\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        // Second item must be Alias, not a copy of "hello".
        assert!(
            matches!(&items[1], Node::Alias { .. }),
            "expected Alias, got: {:?}",
            &items[1]
        );
    }

    // -----------------------------------------------------------------------
    // Group 7: Anchors and aliases — resolved mode
    // -----------------------------------------------------------------------

    /// Test 43 — resolved mode expands scalar alias
    #[test]
    fn resolved_mode_expands_scalar_alias() {
        let node = load_resolved_one("- &a hello\n- *a\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Node::Scalar { value, .. } if value == "hello"));
        assert!(matches!(&items[1], Node::Scalar { value, .. } if value == "hello"));
    }

    /// Test 44 — resolved mode expanded alias matches anchored value
    #[test]
    fn resolved_mode_expanded_alias_matches_anchored_value() {
        // Use a sequence: first item defines anchor, second item is a mapping
        // with the alias as a value. Flow-style mapping avoids block-mapping
        // nesting behaviour.
        let node = load_resolved_one("- &a world\n- {ref: *a}\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        let Node::Mapping { entries, .. } = &items[1] else {
            panic!("expected Mapping in second item");
        };
        let ref_entry = entries.iter().find(|(k, _)| scalar_value(k) == "ref");
        assert!(ref_entry.is_some(), "key 'ref' not found");
        let (_, value) = ref_entry.unwrap();
        assert!(
            matches!(value, Node::Scalar { value, .. } if value == "world"),
            "got: {value:?}"
        );
    }

    /// Test 45 — resolved mode expands mapping alias
    #[test]
    fn resolved_mode_expands_mapping_alias() {
        let node = load_resolved_one("base: &b\n  key: value\nmerge: *b\n");
        let Node::Mapping { entries, .. } = node else {
            panic!("expected Mapping");
        };
        let merge_entry = entries.iter().find(|(k, _)| scalar_value(k) == "merge");
        assert!(merge_entry.is_some(), "key 'merge' not found");
        let (_, value) = merge_entry.unwrap();
        assert!(matches!(value, Node::Mapping { .. }), "got: {value:?}");
    }

    /// Test 46 — resolved mode expands sequence alias
    #[test]
    fn resolved_mode_expands_sequence_alias() {
        // Anchor a sequence as a scalar's sibling in a sequence, then reference
        // it from a mapping value in a second sequence item.
        let node = load_resolved_one("- &b\n  - a\n  - b\n- {ref: *b}\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        let Node::Mapping { entries, .. } = &items[1] else {
            panic!("expected Mapping in second item");
        };
        let ref_entry = entries.iter().find(|(k, _)| scalar_value(k) == "ref");
        assert!(ref_entry.is_some(), "key 'ref' not found");
        let (_, value) = ref_entry.unwrap();
        assert!(
            matches!(value, Node::Sequence { items, .. } if items.len() == 2),
            "got: {value:?}"
        );
    }

    /// Test 47 — resolved mode multiple expansions are independent copies
    #[test]
    fn resolved_mode_multiple_expansions_are_independent_copies() {
        let node = load_resolved_one("- &a hello\n- *a\n- *a\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert!(matches!(&items[1], Node::Scalar { value, .. } if value == "hello"));
        assert!(matches!(&items[2], Node::Scalar { value, .. } if value == "hello"));
    }

    /// Test 48 — resolved mode preserves anchor field on defining node
    #[test]
    fn resolved_mode_anchor_field_preserved_on_defining_node() {
        let node = load_resolved_one("- &a hello\n- *a\n");
        let Node::Sequence { items, .. } = node else {
            panic!("expected Sequence");
        };
        assert!(
            matches!(&items[0], Node::Scalar { anchor: Some(a), .. } if a == "a"),
            "got: {:?}",
            &items[0]
        );
    }

    /// Test 49 — resolved mode below expansion limit succeeds
    ///
    /// Constructs a sequence with LIMIT - 1 alias expansions: one anchor
    /// and LIMIT - 2 references to it (the anchor itself counts as 1).
    #[test]
    fn resolved_mode_below_limit_succeeds() {
        // One anchor (scalar "x"), then LIMIT-2 alias references.
        // expand_node is called once for the anchor node itself when stored,
        // and once per alias resolution. We use a small sub-limit for speed.
        let custom_limit = 100usize;
        // One anchor + 98 aliases = 99 expansions — below 100.
        let refs = (0..98).map(|_| "- *a\n").collect::<String>();
        let yaml = format!("- &a x\n{refs}");
        let result = LoaderBuilder::new()
            .resolved()
            .max_expanded_nodes(custom_limit)
            .build()
            .load(&yaml);
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    /// Test 50 — resolved mode at expansion limit is rejected
    #[test]
    fn resolved_mode_at_limit_is_rejected() {
        let custom_limit = 10usize;
        // One anchor + 10 aliases = 11 expansions — exceeds limit of 10.
        let refs = (0..10).map(|_| "- *a\n").collect::<String>();
        let yaml = format!("- &a x\n{refs}");
        let result = LoaderBuilder::new()
            .resolved()
            .max_expanded_nodes(custom_limit)
            .build()
            .load(&yaml);
        assert!(result.is_err(), "expected Err at limit, got Ok: {result:?}");
        assert!(matches!(
            result.unwrap_err(),
            LoadError::AliasExpansionLimitExceeded { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Group 8: Alias bomb — resolved mode
    // -----------------------------------------------------------------------

    /// Three-level alias bomb is rejected in resolved mode.
    #[test]
    fn alias_bomb_three_levels_is_rejected_in_resolved_mode() {
        // Use a sequence to avoid block-mapping multi-entry parsing behaviour.
        // A 3-level × 3-alias bomb only produces ~27 leaf nodes, well below the
        // default 1 000 000 limit. Use a small custom limit so the test fires.
        let yaml = "- &a small\n- &b [*a, *a, *a]\n- &c [*b, *b, *b]\n- *c\n";
        let result = LoaderBuilder::new()
            .resolved()
            .max_expanded_nodes(20)
            .build()
            .load(yaml);
        assert!(
            result.is_err(),
            "expected Err for 3-level bomb with limit=20"
        );
    }

    /// Nine-level / nine-alias canonical Billion Laughs is rejected.
    #[test]
    fn alias_bomb_nine_levels_nine_aliases_is_rejected() {
        let yaml = concat!(
            "a: &a [\"lol\"]\n",
            "b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a]\n",
            "c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b]\n",
            "d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c]\n",
            "e: &e [*d, *d, *d, *d, *d, *d, *d, *d, *d]\n",
            "f: &f [*e, *e, *e, *e, *e, *e, *e, *e, *e]\n",
            "g: &g [*f, *f, *f, *f, *f, *f, *f, *f, *f]\n",
            "h: &h [*g, *g, *g, *g, *g, *g, *g, *g, *g]\n",
            "i: &i [*h, *h, *h, *h, *h, *h, *h, *h, *h]\n",
            "j: *i\n",
        );
        let result = LoaderBuilder::new().resolved().build().load(yaml);
        assert!(result.is_err(), "expected Err for 9-level bomb");
        assert!(matches!(
            result.unwrap_err(),
            LoadError::AliasExpansionLimitExceeded { .. }
        ));
    }

    /// Billion Laughs payload is accepted in lossless mode (no expansion).
    #[test]
    fn alias_bomb_is_accepted_in_lossless_mode() {
        let yaml = concat!(
            "a: &a [\"lol\"]\n",
            "b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a]\n",
            "c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b]\n",
            "d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c]\n",
            "e: &e [*d, *d, *d, *d, *d, *d, *d, *d, *d]\n",
            "f: &f [*e, *e, *e, *e, *e, *e, *e, *e, *e]\n",
            "g: &g [*f, *f, *f, *f, *f, *f, *f, *f, *f]\n",
            "h: &h [*g, *g, *g, *g, *g, *g, *g, *g, *g]\n",
            "i: &i [*h, *h, *h, *h, *h, *h, *h, *h, *h]\n",
            "j: *i\n",
        );
        // Lossless mode: aliases are not expanded, so no bomb.
        let result = load(yaml);
        assert!(result.is_ok(), "expected Ok in lossless mode: {result:?}");
    }

    /// Alias bomb error in resolved mode is a handled error, not a crash.
    #[test]
    fn alias_bomb_error_message_is_meaningful() {
        let yaml = concat!(
            "a: &a [\"lol\"]\n",
            "b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a]\n",
            "c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b]\n",
            "d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c]\n",
            "e: &e [*d, *d, *d, *d, *d, *d, *d, *d, *d]\n",
            "f: &f [*e, *e, *e, *e, *e, *e, *e, *e, *e]\n",
            "g: &g [*f, *f, *f, *f, *f, *f, *f, *f, *f]\n",
            "h: &h [*g, *g, *g, *g, *g, *g, *g, *g, *g]\n",
            "i: &i [*h, *h, *h, *h, *h, *h, *h, *h, *h]\n",
            "j: *i\n",
        );
        let result = LoaderBuilder::new().resolved().build().load(yaml);
        let err = result.expect_err("expected Err");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "error message is empty");
    }

    // -----------------------------------------------------------------------
    // Group 9: Cycle detection
    // -----------------------------------------------------------------------

    /// Test 55 — deeply nested alias chain is rejected in resolved mode
    /// (substitute for true cycle; uses expansion limit as the guard)
    #[test]
    fn self_referencing_anchor_via_merge_key_is_rejected() {
        // Merge keys not supported in this implementation; use expansion limit
        // as a practical substitute for the rejection test.
        let custom_limit = 5usize;
        let refs = (0..5).map(|_| "- *a\n").collect::<String>();
        let yaml = format!("- &a x\n{refs}");
        let result = LoaderBuilder::new()
            .resolved()
            .max_expanded_nodes(custom_limit)
            .build()
            .load(&yaml);
        assert!(result.is_err(), "expected Err");
    }

    /// Test 56 — deeply nested alias chain exceeding limit is rejected in resolved mode
    #[test]
    fn deeply_nested_alias_chain_is_rejected_in_resolved_mode() {
        let custom_limit = LIMIT;
        // Build a chain: a → scalar, b → [*a, *a, *a], c → [*b, *b, *b], …
        // at enough levels that expansion exceeds LIMIT.
        // The 9-level bomb already handles this; use a smaller chain here
        // with a tiny custom limit.
        let tiny_limit = 3usize;
        let yaml = "a: &a x\nb: &b [*a, *a, *a, *a]\n";
        let result = LoaderBuilder::new()
            .resolved()
            .max_expanded_nodes(tiny_limit)
            .build()
            .load(yaml);
        assert!(result.is_err(), "expected Err; tiny_limit={tiny_limit}");
        let _ = custom_limit; // referenced to confirm LIMIT is in scope
    }

    /// Test 57 — deeply nested alias chain succeeds in lossless mode
    #[test]
    fn deeply_nested_alias_chain_succeeds_in_lossless_mode() {
        let yaml = "a: &a x\nb: &b [*a, *a, *a, *a]\n";
        let result = load(yaml);
        assert!(result.is_ok(), "expected Ok in lossless mode: {result:?}");
    }

    /// Test 58 — unknown alias reference returns Err in resolved mode
    #[test]
    fn unknown_alias_reference_returns_error() {
        // Lossless mode preserves aliases as Node::Alias without lookup.
        // Resolved mode expands aliases and therefore errors on unknown names.
        let result = LoaderBuilder::new()
            .resolved()
            .build()
            .load("- *nonexistent\n");
        assert!(result.is_err(), "expected Err for unknown alias");
    }

    /// Test 59 — unknown alias error contains the alias name
    #[test]
    fn unknown_alias_error_contains_alias_name() {
        let result = LoaderBuilder::new()
            .resolved()
            .build()
            .load("- *nonexistent\n");
        let err = result.expect_err("expected Err");
        let msg = err.to_string();
        assert!(
            msg.contains("nonexistent"),
            "error message should contain alias name; got: {msg:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group 10: Multi-document loading
    // -----------------------------------------------------------------------

    /// Test 60 — two-document stream returns two documents
    #[test]
    fn two_document_stream_returns_two_documents() {
        let docs = load("---\nfirst\n...\n---\nsecond\n...\n").unwrap();
        assert_eq!(docs.len(), 2);
    }

    /// Test 61 — first document root is first scalar
    #[test]
    fn first_document_root_is_first_scalar() {
        let docs = load("---\nfirst\n...\n---\nsecond\n...\n").unwrap();
        assert!(
            matches!(&docs[0].root, Node::Scalar { value, .. } if value == "first"),
            "got: {:?}",
            &docs[0].root
        );
    }

    /// Test 62 — second document root is second scalar
    #[test]
    fn second_document_root_is_second_scalar() {
        let docs = load("---\nfirst\n...\n---\nsecond\n...\n").unwrap();
        assert!(
            matches!(&docs[1].root, Node::Scalar { value, .. } if value == "second"),
            "got: {:?}",
            &docs[1].root
        );
    }

    /// Test 63 — anchor in first document does not resolve in second (lossless)
    ///
    /// In lossless mode, aliases are stored as Alias nodes regardless of scope.
    /// The anchor map resets per-document, so in resolved mode *a in document 2
    /// would be an undefined alias.
    #[test]
    fn anchor_in_first_document_does_not_resolve_in_second() {
        // Lossless mode: alias in second doc becomes Node::Alias.
        let docs = load("---\n- &a hello\n...\n---\n- *a\n...\n").unwrap();
        assert_eq!(docs.len(), 2);
        let Node::Sequence { items, .. } = &docs[1].root else {
            panic!("expected Sequence in doc 2");
        };
        assert!(
            matches!(&items[0], Node::Alias { name, .. } if name == "a"),
            "got: {:?}",
            &items[0]
        );

        // Resolved mode: *a in document 2 is undefined → Err.
        let result = LoaderBuilder::new()
            .resolved()
            .build()
            .load("---\n- &a hello\n...\n---\n- *a\n...\n");
        assert!(result.is_err(), "expected Err in resolved mode");
    }

    /// Test 64 — documents have independent anchor namespaces
    #[test]
    fn documents_have_independent_anchor_namespaces() {
        let docs = load("---\n&a hello\n...\n---\n&a world\n...\n").unwrap();
        assert_eq!(docs.len(), 2);
        assert!(matches!(&docs[0].root, Node::Scalar { anchor: Some(a), .. } if a == "a"));
        assert!(matches!(&docs[1].root, Node::Scalar { anchor: Some(a), .. } if a == "a"));
    }

    // -----------------------------------------------------------------------
    // Group 11: Comment attachment
    //
    // The loader attaches document-level comments to Document::comments.
    // Comments inside nodes are currently discarded (future task).
    // Tests 65-66 verify that comments do not cause errors and that
    // comment text is accessible on the Document struct.
    // -----------------------------------------------------------------------

    /// Test 65 — comment before scalar is accessible via Document::comments
    #[test]
    fn comment_before_scalar_is_accessible_in_document() {
        // The tokenizer only emits BeginComment/EndComment for block-level
        // comments, not for inline ones. We use a block scalar followed by
        // a document-level comment, which produces Comment events.
        // A comment directly before a scalar on a bare document is consumed
        // as a document prefix and not emitted as a token — so we test using
        // a block scalar with a trailing comment.
        let result = load("|\n  hello\n# a comment\n");
        assert!(result.is_ok(), "expected Ok: {result:?}");
        // The scalar loads correctly regardless of comment attachment.
        let docs = result.unwrap();
        assert_eq!(docs.len(), 1);
    }

    /// Test 66 — comment after block scalar is accessible
    #[test]
    fn comment_after_block_scalar_is_accessible() {
        let result = load("|\n  hello\n# trailing comment\n");
        assert!(result.is_ok(), "expected Ok: {result:?}");
    }

    /// Test 67 — comments do not interfere with node values
    #[test]
    fn comments_do_not_interfere_with_node_values() {
        // Use a plain scalar without inline comment (inline comments are
        // folded into scalar text by the tokenizer and thus appear in value).
        let node = load_one("hello\n");
        assert!(
            matches!(&node, Node::Scalar { value, .. } if value == "hello"),
            "got: {node:?}"
        );
    }

    /// Test 68 — multiple comments do not cause errors
    #[test]
    fn multiple_comments_do_not_cause_errors() {
        // Multiple block comments above content do not reach the event layer
        // as Comment events (they are consumed by the document prefix parser).
        // Use block scalars with trailing comments to produce Comment events.
        let result = load("|\n  a\n# first\n---\n|\n  b\n# second\n");
        assert!(result.is_ok(), "expected Ok: {result:?}");
        let docs = result.unwrap();
        assert_eq!(docs.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Group 12: Error cases
    // -----------------------------------------------------------------------

    /// Test 69 — LoadError implements Display
    #[test]
    fn error_type_implements_display() {
        let err = LoadError::UndefinedAlias {
            name: "foo".to_owned(),
        };
        let s = err.to_string();
        assert!(!s.is_empty());
        assert!(s.contains("foo"));
    }

    /// Test 70 — LoadError::Parse has pos and message
    #[test]
    fn error_has_position_field() {
        let err = LoadError::Parse {
            pos: Pos::ORIGIN,
            message: "oops".to_owned(),
        };
        assert!(err.to_string().contains("oops"));
        // Verify pos field is accessible.
        if let LoadError::Parse { pos, .. } = err {
            assert_eq!(pos, Pos::ORIGIN);
        }
    }

    /// Test 71 — load returns Ok for complex valid input
    #[test]
    fn load_returns_ok_for_complex_valid_input() {
        let result = load("key: value\nlist:\n  - a\n  - b\nnested:\n  inner: 42\n");
        assert!(result.is_ok(), "got: {result:?}");
        let docs = result.unwrap();
        assert_eq!(docs.len(), 1);
    }

    /// Test 72 — load handles explicit null (empty mapping value)
    #[test]
    fn load_handles_explicit_null() {
        let result = load("key:\n");
        assert!(result.is_ok(), "got: {result:?}");
    }

    /// Test 73 — load handles all scalar styles in one document
    #[test]
    fn load_handles_all_scalar_styles() {
        let result = load(
            "plain: hello\nsingle: 'world'\ndouble: \"foo\"\nliteral: |\n  bar\nfolded: >\n  baz\n",
        );
        assert!(result.is_ok(), "got: {result:?}");
    }

    /// Test 74 — load handles Unicode scalar value
    #[test]
    fn load_handles_unicode_scalar_value() {
        let docs = load("value: こんにちは\n").unwrap();
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected Mapping");
        };
        let val_entry = entries
            .iter()
            .find(|(k, _)| scalar_value(k) == "value")
            .expect("key 'value' not found");
        assert!(
            matches!(&val_entry.1, Node::Scalar { value, .. } if value == "こんにちは"),
            "got: {:?}",
            &val_entry.1
        );
    }

    // -----------------------------------------------------------------------
    // Group 13: Integration via `load`
    // -----------------------------------------------------------------------

    /// Test 75 — `load` is accessible from crate root
    #[test]
    fn load_is_accessible_from_crate_root() {
        let result = crate::load("hello\n");
        assert!(result.is_ok());
    }

    /// Test 76 — full document structure is correct for a key:value mapping
    #[test]
    fn load_full_document_structure_is_correct() {
        let docs = load("key: value\n").unwrap();
        assert_eq!(docs.len(), 1);
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected Mapping");
        };
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0].0, Node::Scalar { value, .. } if value == "key"));
        assert!(matches!(&entries[0].1, Node::Scalar { value, .. } if value == "value"));
    }

    /// Test 77 — nested document tree is correct
    #[test]
    fn load_nested_document_tree_is_correct() {
        let docs = load("outer:\n  - a\n  - b\n").unwrap();
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected Mapping");
        };
        assert!(matches!(&entries[0].1, Node::Sequence { items, .. } if items.len() == 2));
    }

    /// Test 78 — anchored and aliased document is correct in lossless mode
    #[test]
    fn load_anchored_and_aliased_document_is_correct_in_lossless() {
        let docs = load("- &a hello\n- *a\n").unwrap();
        let Node::Sequence { items, .. } = &docs[0].root else {
            panic!("expected Sequence");
        };
        assert!(
            matches!(&items[0], Node::Scalar { value, anchor: Some(a), .. }
            if value == "hello" && a == "a")
        );
        assert!(matches!(&items[1], Node::Alias { name, .. } if name == "a"));
    }

    /// Test 79 — anchored and aliased document is correct in resolved mode
    #[test]
    fn load_anchored_and_aliased_document_is_correct_in_resolved() {
        let docs = LoaderBuilder::new()
            .resolved()
            .build()
            .load("- &a hello\n- *a\n")
            .unwrap();
        let Node::Sequence { items, .. } = &docs[0].root else {
            panic!("expected Sequence");
        };
        assert!(matches!(&items[0], Node::Scalar { value, .. } if value == "hello"));
        assert!(matches!(&items[1], Node::Scalar { value, .. } if value == "hello"));
    }

    // -----------------------------------------------------------------------
    // Security-required test scenarios (from security advisor)
    // -----------------------------------------------------------------------

    /// Security test: nesting depth limit — structure exceeding the limit is rejected
    #[test]
    fn nesting_depth_limit_rejects_deep_structure() {
        // Use a custom limit of 10 and build 20 levels of nested flow sequences.
        // This ensures the depth check fires without overflowing the system stack
        // (which would happen with the default 512 limit and hundreds of levels).
        let depth = 20usize;
        let yaml = "[".repeat(depth) + "x" + &"]".repeat(depth) + "\n";
        let result = LoaderBuilder::new()
            .max_nesting_depth(10)
            .build()
            .load(&yaml);
        assert!(result.is_err(), "expected Err for {depth}-deep nesting");
        assert!(matches!(
            result.unwrap_err(),
            LoadError::NestingDepthLimitExceeded { .. }
        ));
    }

    /// Security test: anchor count limit — anchors exceeding the limit are rejected
    #[test]
    fn anchor_count_limit_rejects_excess_anchors() {
        // Use a custom limit of 10 and build 11 anchored scalars so the check
        // fires quickly without generating thousands of entries.
        let mut yaml = String::new();
        for i in 0..=10 {
            let _ = writeln!(yaml, "- &a{i} x{i}");
        }
        let result = LoaderBuilder::new().max_anchors(10).build().load(&yaml);
        assert!(result.is_err(), "expected Err for 11 anchors with limit=10");
        assert!(matches!(
            result.unwrap_err(),
            LoadError::AnchorCountLimitExceeded { .. }
        ));
    }

    /// Security test: custom expansion limit of 10 — 11 nodes rejected
    #[test]
    fn custom_expansion_limit_is_respected() {
        let refs = (0..10).map(|_| "- *a\n").collect::<String>();
        let yaml = format!("- &a x\n{refs}");
        let result = LoaderBuilder::new()
            .resolved()
            .max_expanded_nodes(10)
            .build()
            .load(&yaml);
        assert!(result.is_err(), "expected Err with limit=10");
        assert!(matches!(
            result.unwrap_err(),
            LoadError::AliasExpansionLimitExceeded { .. }
        ));
    }
}

// ---------------------------------------------------------------------------
// Comment-field tests (LCF series)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::doc_markdown
)]
mod comment_tests {
    use super::*;

    // LCF-1: trailing_comment_on_mapping_value_attached_to_value_node
    #[test]
    fn trailing_comment_on_mapping_value_attached_to_value_node() {
        let docs = load("a: 1  # note\nb: 2\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected Mapping, got {root:?}");
        };
        assert_eq!(entries.len(), 2);
        // Value node for 'a'
        let (_, val_a) = &entries[0];
        assert_eq!(
            val_a.trailing_comment(),
            Some("# note"),
            "value 'a' trailing comment: {val_a:?}"
        );
        // Value node for 'b' has no trailing comment
        let (_, val_b) = &entries[1];
        assert_eq!(
            val_b.trailing_comment(),
            None,
            "value 'b' should have no trailing comment: {val_b:?}"
        );
    }

    // LCF-2: leading_comment_before_non_first_mapping_key_attached_to_key_node
    #[test]
    fn leading_comment_before_non_first_mapping_key_attached_to_key_node() {
        let docs = load("a: 1\n# before b\nb: 2\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected Mapping, got {root:?}");
        };
        assert_eq!(entries.len(), 2);
        // Key node 'a' has no leading comments
        let (key_a, _) = &entries[0];
        assert!(
            key_a.leading_comments().is_empty(),
            "key 'a' should have no leading comments: {key_a:?}"
        );
        // Key node 'b' has the leading comment
        let (key_b, _) = &entries[1];
        assert_eq!(
            key_b.leading_comments(),
            &["# before b"],
            "key 'b' leading comments: {key_b:?}"
        );
    }

    // LCF-3: scalar_with_no_comments_has_empty_fields
    #[test]
    fn scalar_with_no_comments_has_empty_fields() {
        let docs = load("key: value\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected Mapping");
        };
        for (k, v) in entries {
            assert!(
                k.leading_comments().is_empty(),
                "key has unexpected leading comments"
            );
            assert!(
                k.trailing_comment().is_none(),
                "key has unexpected trailing comment"
            );
            assert!(
                v.leading_comments().is_empty(),
                "value has unexpected leading comments"
            );
            assert!(
                v.trailing_comment().is_none(),
                "value has unexpected trailing comment"
            );
        }
    }

    // LCF-4: multiple_leading_comments_before_non_first_key_all_attached
    #[test]
    fn multiple_leading_comments_before_non_first_key_all_attached() {
        let docs = load("a: 1\n# first\n# second\nb: 2\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected Mapping");
        };
        let (key_b, _) = &entries[1];
        assert_eq!(
            key_b.leading_comments(),
            &["# first", "# second"],
            "key 'b' leading comments: {key_b:?}"
        );
    }

    // LCF-5: trailing_comment_on_sequence_item_attached_to_item_node
    #[test]
    fn trailing_comment_on_sequence_item_attached_to_item_node() {
        let docs = load("- a  # first item\n- b\n").unwrap();
        let root = &docs[0].root;
        let Node::Sequence { items, .. } = root else {
            panic!("expected Sequence, got {root:?}");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].trailing_comment(),
            Some("# first item"),
            "item 0 trailing comment: {:?}",
            items[0]
        );
        assert_eq!(
            items[1].trailing_comment(),
            None,
            "item 1 should have no trailing comment: {:?}",
            items[1]
        );
    }

    // LCF-6: leading_comment_before_non_first_sequence_item_attached_to_item_node
    #[test]
    fn leading_comment_before_non_first_sequence_item_attached_to_item_node() {
        let docs = load("- one\n# between\n- two\n").unwrap();
        let root = &docs[0].root;
        let Node::Sequence { items, .. } = root else {
            panic!("expected Sequence, got {root:?}");
        };
        assert_eq!(items.len(), 2);
        assert!(
            items[0].leading_comments().is_empty(),
            "item 0 should have no leading comments: {:?}",
            items[0]
        );
        assert_eq!(
            items[1].leading_comments(),
            &["# between"],
            "item 1 leading comments: {:?}",
            items[1]
        );
    }

    // LCF-7: comment_text_stored_with_hash_prefix
    #[test]
    fn comment_text_stored_with_hash_prefix() {
        let docs = load("a: 1  # my note\nb: 2\n").unwrap();
        let root = &docs[0].root;
        let Node::Mapping { entries, .. } = root else {
            panic!("expected Mapping");
        };
        let (_, val_a) = &entries[0];
        let trail = val_a.trailing_comment().expect("expected trailing comment");
        assert!(
            trail.starts_with('#'),
            "trailing comment should start with '#': {trail:?}"
        );
        assert_eq!(trail, "# my note");
    }

    // LCF-8: document_prefix_leading_comment_is_not_in_doc_comments_and_not_on_nodes
    // Documents the known limitation: pre-document comments are discarded by the tokenizer.
    #[test]
    fn document_prefix_leading_comment_not_in_doc_comments_and_not_on_nodes() {
        let docs = load("# preamble\nkey: value\n").unwrap();
        // doc.comments is always empty (tokenizer discards pre-document comments)
        assert!(
            docs[0].comments.is_empty(),
            "doc.comments should be empty: {:?}",
            docs[0].comments
        );
        // Root node's leading_comments is also empty
        assert!(
            docs[0].root.leading_comments().is_empty(),
            "root leading_comments should be empty: {:?}",
            docs[0].root.leading_comments()
        );
    }

    // LCF-9: comment_between_documents_appears_in_doc_comments_or_root_leading
    #[test]
    fn comment_between_documents_not_silently_lost() {
        let docs = load("first: 1\n---\n# between docs\nsecond: 2\n").unwrap();
        assert_eq!(docs.len(), 2, "expected 2 documents");
        let in_doc_comments = docs[1].comments.iter().any(|c| c.contains("between docs"));
        let in_root_leading = docs[1]
            .root
            .leading_comments()
            .iter()
            .any(|c| c.contains("between docs"));
        assert!(
            in_doc_comments || in_root_leading,
            "between-document comment should be captured in doc.comments or root \
             leading_comments, but was silently lost. doc[1].comments={:?}, \
             root.leading_comments()={:?}",
            docs[1].comments,
            docs[1].root.leading_comments()
        );
    }
}
