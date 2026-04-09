// SPDX-License-Identifier: MIT
#![deny(clippy::panic)]

mod chars;
mod error;
mod event;
mod lexer;
mod lines;
mod loader;
mod pos;
mod scanner;

pub use error::Error;
pub use event::{Chomp, CollectionStyle, Event, ScalarStyle};
pub use lines::{BreakType, Line, LineBuffer};
pub use pos::{Pos, Span};

use std::collections::{HashMap, VecDeque};

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
/// use rlsp_yaml_parser_temp::{parse_events, Event};
///
/// let events: Vec<_> = parse_events("").collect();
/// assert!(matches!(events.first(), Some(Ok((Event::StreamStart, _)))));
/// assert!(matches!(events.last(), Some(Ok((Event::StreamEnd, _)))));
/// ```
pub fn parse_events(input: &str) -> impl Iterator<Item = Result<(Event<'_>, Span), Error>> + '_ {
    EventIter::new(input)
}

// ---------------------------------------------------------------------------
// Depth limit (security: DoS via deeply nested collections)
// ---------------------------------------------------------------------------

/// Maximum combined block-collection nesting depth accepted from untrusted
/// input.
///
/// This limit covers all open [`Event::SequenceStart`] and
/// [`Event::MappingStart`] events combined.  Using a unified limit prevents
/// an attacker from nesting 512 sequences inside 512 mappings (total depth
/// 1024) by exploiting separate per-type limits.
///
/// 512 is generous for all real-world YAML (Kubernetes / Helm documents are
/// typically under 20 levels deep) and small enough that the explicit-stack
/// overhead stays within a few KB.
pub const MAX_COLLECTION_DEPTH: usize = 512;

/// Maximum byte length of an anchor name accepted from untrusted input.
///
/// Maximum byte length of an anchor or alias name.
///
/// The YAML spec places no upper limit on anchor names, but scanning a name
/// consisting of millions of valid `ns-anchor-char` bytes would exhaust CPU
/// time without any heap allocation.  This limit caps anchor and alias name
/// scanning at 1 KiB — generous for all real-world YAML (Kubernetes names are
/// typically under 64 bytes) while preventing degenerate-input stalls.
///
/// The limit is enforced by [`parse_events`] for both `&name` (anchors) and
/// `*name` (aliases).  Exceeding it returns an [`Error`], not a panic.
pub const MAX_ANCHOR_NAME_BYTES: usize = 1024;

/// Maximum byte length of a tag accepted from untrusted input.
///
/// The YAML spec places no upper limit on tag length, but scanning a tag
/// consisting of millions of valid bytes would exhaust CPU time without any
/// heap allocation.  This limit caps tag scanning at 4 KiB — generous for all
/// real-world YAML (standard tags like `tag:yaml.org,2002:str` are under 30
/// bytes; custom namespace URIs are rarely over 200 bytes) while preventing
/// degenerate-input stalls.
///
/// The limit applies to the raw scanned portion: the URI content between `<`
/// and `>` for verbatim tags, or the suffix portion for shorthand tags.
/// Exceeding it returns an [`Error`], not a panic.
pub const MAX_TAG_LEN: usize = 4096;

/// Maximum byte length of a comment body accepted from untrusted input.
///
/// The YAML spec places no upper limit on comment length.  With zero-copy
/// `&'input str` slices, comment scanning itself allocates nothing, but
/// character-by-character iteration over a very long comment line still burns
/// CPU proportional to the line length.  This limit matches `MAX_TAG_LEN` —
/// comment-only files produce one `Comment` event per line (O(input size),
/// acceptable) as long as individual lines are bounded.
///
/// Exceeding this limit returns an [`Error`], not a panic or truncation.
pub const MAX_COMMENT_LEN: usize = 4096;

/// Maximum number of directives (`%YAML` + `%TAG` combined) per document.
///
/// Without this cap, an attacker could supply thousands of distinct `%TAG`
/// directives, each allocating a `HashMap` entry, to exhaust heap memory.
/// 64 is generous for all real-world YAML (the typical document has 0–2
/// directives) while bounding per-document directive overhead.
///
/// Exceeding this limit returns an [`Error`], not a panic.
pub const MAX_DIRECTIVES_PER_DOC: usize = 64;

/// Maximum byte length of a `%TAG` handle (e.g. `!foo!`) accepted from
/// untrusted input.
///
/// Tag handles are short by design; a 256-byte cap is generous while
/// preventing `DoS` via scanning very long handle strings.
///
/// Exceeding this limit returns an [`Error`], not a panic.
pub const MAX_TAG_HANDLE_BYTES: usize = 256;

/// Maximum byte length of the fully-resolved tag string after prefix expansion.
///
/// When a shorthand tag `!foo!bar` is resolved against its `%TAG` prefix, the
/// result is `prefix + suffix`.  This cap prevents the resolved string from
/// exceeding a safe bound even when the prefix and suffix are both at their
/// individual limits.  Reuses [`MAX_TAG_LEN`] so the bound is consistent with
/// verbatim tag limits.
///
/// The check is performed before allocation; exceeding this limit returns an
/// [`Error`], not a panic.
pub const MAX_RESOLVED_TAG_LEN: usize = MAX_TAG_LEN;

// ---------------------------------------------------------------------------
// Directive scope
// ---------------------------------------------------------------------------

/// Per-document directive state accumulated from `%YAML` and `%TAG` directives.
///
/// Cleared at the start of each new document (on `---` in `BetweenDocs`, on
/// `...`, or at EOF).  The default handles (`!!` and `!`) are **not** stored
/// here — they are resolved directly in [`DirectiveScope::resolve_tag`].
#[derive(Debug, Default)]
struct DirectiveScope {
    /// Version from `%YAML`, if any.
    version: Option<(u8, u8)>,
    /// Custom tag handles declared via `%TAG` directives.
    ///
    /// Key: handle (e.g. `"!foo!"`).  Value: prefix (e.g. `"tag:example.com:"`).
    tag_handles: HashMap<String, String>,
    /// Total directive count (YAML + TAG combined) for the `DoS` limit check.
    directive_count: usize,
}

impl DirectiveScope {
    /// Resolve a raw tag slice (as stored in `pending_tag`) to its final form.
    ///
    /// Resolution rules:
    /// - Verbatim tag (no leading `!`, i.e. already a bare URI from `!<URI>` scanning) → returned as-is.
    /// - `!!suffix` → look up `"!!"` in custom handles; fall back to default `tag:yaml.org,2002:`.
    /// - `!suffix` (no inner `!`) → returned as-is (local tag, no expansion).
    /// - `!handle!suffix` → look up `"!handle!"` in custom handles; error if not found.
    /// - `!` (bare) → returned as-is.
    ///
    /// Returns `Ok(Cow::Borrowed(raw))` when no allocation is needed, or
    /// `Ok(Cow::Owned(resolved))` after prefix expansion.  Returns `Err` when
    /// a named handle has no registered prefix.
    fn resolve_tag<'a>(
        &self,
        raw: &'a str,
        indicator_pos: Pos,
    ) -> Result<std::borrow::Cow<'a, str>, Error> {
        use std::borrow::Cow;

        // Verbatim tags arrive as bare URIs (scan_tag strips the `!<` / `>` wrappers).
        // They do not start with `!`, so no resolution is needed.
        if !raw.starts_with('!') {
            return Ok(Cow::Borrowed(raw));
        }

        let after_first_bang = &raw[1..];

        // `!!suffix` — primary handle.
        if let Some(suffix) = after_first_bang.strip_prefix('!') {
            let prefix = self
                .tag_handles
                .get("!!")
                .map_or("tag:yaml.org,2002:", String::as_str);
            let resolved = format!("{prefix}{suffix}");
            if resolved.len() > MAX_RESOLVED_TAG_LEN {
                return Err(Error {
                    pos: indicator_pos,
                    message: format!(
                        "resolved tag exceeds maximum length of {MAX_RESOLVED_TAG_LEN} bytes"
                    ),
                });
            }
            return Ok(Cow::Owned(resolved));
        }

        // `!handle!suffix` — named handle.
        if let Some(inner_bang) = after_first_bang.find('!') {
            let handle = &raw[..inner_bang + 2]; // `!handle!`
            let suffix = &after_first_bang[inner_bang + 1..];
            if let Some(prefix) = self.tag_handles.get(handle) {
                let resolved = format!("{prefix}{suffix}");
                if resolved.len() > MAX_RESOLVED_TAG_LEN {
                    return Err(Error {
                        pos: indicator_pos,
                        message: format!(
                            "resolved tag exceeds maximum length of {MAX_RESOLVED_TAG_LEN} bytes"
                        ),
                    });
                }
                return Ok(Cow::Owned(resolved));
            }
            return Err(Error {
                pos: indicator_pos,
                message: format!("undefined tag handle: {handle}"),
            });
        }

        // `!suffix` (local tag) or bare `!` — no expansion.
        Ok(Cow::Borrowed(raw))
    }

    /// Collect the tag handle/prefix pairs for inclusion in `DocumentStart`.
    fn tag_directives(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .tag_handles
            .iter()
            .map(|(h, p)| (h.clone(), p.clone()))
            .collect();
        // Sort for deterministic ordering in tests and events.
        pairs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        pairs
    }
}

// ---------------------------------------------------------------------------
// Iterator implementation
// ---------------------------------------------------------------------------

/// Outcome of one state-machine step inside [`EventIter::next`].
enum StepResult<'input> {
    /// The step pushed to `queue` or changed state; loop again to drain.
    Continue,
    /// The step produced an event or error to return immediately.
    Yield(Result<(Event<'input>, Span), Error>),
}

/// State of the top-level event iterator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IterState {
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
enum MappingPhase {
    /// The next node is a key (first half of a pair).
    Key,
    /// The next node is a value (second half of a pair).
    Value,
}

/// An entry on the collection stack, tracking open block sequences and mappings.
///
/// Flow collections are fully parsed by [`EventIter::handle_flow_collection`]
/// before returning; they never leave an entry on this stack.  The combined
/// depth limit (block + flow) is enforced inside `handle_flow_collection` by
/// summing `coll_stack.len()` with the local flow-frame count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionEntry {
    /// An open block sequence.  Holds the column of its `-` indicator.
    Sequence(usize),
    /// An open block mapping.  Holds the column of its first key and the
    /// current phase (expecting key or value).
    Mapping(usize, MappingPhase),
}

/// Whether the next expected token in a flow mapping is a key or value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowMappingPhase {
    /// Expecting the next key (or the closing `}`).
    Key,
    /// Expecting the value after a key has been consumed.
    Value,
}

impl CollectionEntry {
    /// The indentation column of this collection's indicator/key.
    const fn indent(self) -> usize {
        match self {
            Self::Sequence(col) | Self::Mapping(col, _) => col,
        }
    }
}

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
    /// Set to `true` after an `Err` is yielded.
    ///
    /// Once set, `next()` immediately returns `None` to prevent infinite
    /// error loops (e.g. depth-limit firing on the same prepended synthetic
    /// line).
    failed: bool,
    /// A pending anchor name (`&name`) that has been scanned but not yet
    /// attached to a node event.
    ///
    /// Anchors in YAML precede the node they annotate.  After scanning
    /// `&name`, the parser stores the name here and attaches it to the next
    /// `Scalar`, `SequenceStart`, or `MappingStart` event.
    ///
    /// `pending_anchor_for_collection` distinguishes two cases:
    /// - `true`: anchor was on its own line (`&name\n- item`) — the anchor
    ///   annotates the next node regardless of type (collection or scalar).
    /// - `false`: anchor was inline with key content
    ///   (`&name key: value`) — the anchor annotates the key scalar, not
    ///   the enclosing mapping.
    pending_anchor: Option<&'input str>,
    /// True when `pending_anchor` was set from a standalone anchor line (no
    /// inline content after the name).  False when set from an inline anchor
    /// that precedes a key or scalar on the same line.
    pending_anchor_for_collection: bool,
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
    pending_tag: Option<std::borrow::Cow<'input, str>>,
    /// True when `pending_tag` was set from a standalone tag line (no inline
    /// content after the tag).  False when set inline.
    pending_tag_for_collection: bool,
    /// Directive scope for the current document.
    ///
    /// Accumulated from `%YAML` and `%TAG` directives seen in `BetweenDocs`
    /// state.  Reset at document boundaries.
    directive_scope: DirectiveScope,
}

impl<'input> EventIter<'input> {
    fn new(input: &'input str) -> Self {
        Self {
            lexer: Lexer::new(input),
            state: IterState::BeforeStream,
            queue: VecDeque::new(),
            coll_stack: Vec::new(),
            failed: false,
            pending_anchor: None,
            pending_anchor_for_collection: false,
            pending_tag: None,
            pending_tag_for_collection: false,
            directive_scope: DirectiveScope::default(),
        }
    }

    /// Current combined collection depth (sequences + mappings).
    const fn collection_depth(&self) -> usize {
        self.coll_stack.len()
    }

    /// Push close events for all collections whose indent is `>= threshold`,
    /// from innermost to outermost.
    ///
    /// After each close, if the new top of the stack is a mapping in Value
    /// phase, flips it to Key phase — the closed collection was that
    /// mapping's value.
    fn close_collections_at_or_above(&mut self, threshold: usize, pos: Pos) {
        while let Some(&top) = self.coll_stack.last() {
            if top.indent() >= threshold {
                self.coll_stack.pop();
                let ev = match top {
                    CollectionEntry::Sequence(_) => Event::SequenceEnd,
                    CollectionEntry::Mapping(_, _) => Event::MappingEnd,
                };
                self.queue.push_back((ev, zero_span(pos)));
                // After closing a collection, the parent mapping (if any)
                // transitions from Value phase to Key phase.
                if let Some(CollectionEntry::Mapping(_, phase)) = self.coll_stack.last_mut() {
                    if *phase == MappingPhase::Value {
                        *phase = MappingPhase::Key;
                    }
                }
            } else {
                break;
            }
        }
    }

    /// Push close events for all open collections (document-end).
    ///
    /// If a mapping is in Value phase when it closes, an empty plain scalar is
    /// emitted first to satisfy the pending key that had no inline value —
    /// **unless** the previous closed item was a collection (sequence or
    /// mapping), which was itself the value.  After each closed collection,
    /// the parent mapping (if any) is advanced from Value to Key phase.
    fn close_all_collections(&mut self, pos: Pos) {
        while let Some(top) = self.coll_stack.pop() {
            let ev = match top {
                CollectionEntry::Sequence(_) => Event::SequenceEnd,
                CollectionEntry::Mapping(_, MappingPhase::Value) => {
                    // Mapping closed while waiting for a value — emit empty value.
                    // Consume any pending anchor so `&anchor\n` at end of doc
                    // is properly attached to the empty value.
                    self.queue.push_back((
                        Event::Scalar {
                            value: std::borrow::Cow::Borrowed(""),
                            style: ScalarStyle::Plain,
                            anchor: self.pending_anchor.take(),
                            tag: None,
                        },
                        zero_span(pos),
                    ));
                    Event::MappingEnd
                }
                CollectionEntry::Mapping(_, MappingPhase::Key) => Event::MappingEnd,
            };
            self.queue.push_back((ev, zero_span(pos)));
            // After closing any collection, advance the parent mapping (if in
            // Value phase) to Key phase — the just-closed collection was its value.
            if let Some(CollectionEntry::Mapping(_, phase)) = self.coll_stack.last_mut() {
                if *phase == MappingPhase::Value {
                    *phase = MappingPhase::Key;
                }
            }
        }
    }

    /// Check whether the next available line is a block-sequence entry
    /// indicator (`-` followed by space, tab, or end-of-content).
    ///
    /// Returns `(dash_indent, dash_pos)` where:
    /// - `dash_indent` is the effective document column of the `-`.
    /// - `dash_pos` is the absolute [`Pos`] of the `-` character.
    fn peek_sequence_entry(&self) -> Option<(usize, Pos)> {
        let line = self.lexer.peek_next_line()?;
        let dash_indent = line.indent;
        let trimmed = line.content.trim_start_matches(' ');

        if !trimmed.starts_with('-') {
            return None;
        }
        let after_dash = &trimmed[1..];
        let is_entry =
            after_dash.is_empty() || after_dash.starts_with(' ') || after_dash.starts_with('\t');
        if !is_entry {
            return None;
        }

        let leading_spaces = line.content.len() - trimmed.len();
        let dash_pos = Pos {
            byte_offset: line.pos.byte_offset + leading_spaces,
            char_offset: line.pos.char_offset + leading_spaces,
            line: line.pos.line,
            column: line.pos.column + leading_spaces,
        };
        Some((dash_indent, dash_pos))
    }

    /// Check whether the next available line looks like an implicit mapping
    /// key: a non-empty line whose plain-scalar content is followed by `: `
    /// (colon + space) or `:\n` (colon at end-of-line) or `:\t`.
    ///
    /// Also recognises the explicit key indicator `? ` at the start of a line.
    ///
    /// Returns `(key_indent, key_pos)` on success, where `key_indent` is the
    /// document column of the first character of the key (or `?` indicator),
    /// and `key_pos` is its absolute [`Pos`].
    fn peek_mapping_entry(&self) -> Option<(usize, Pos)> {
        let line = self.lexer.peek_next_line()?;
        let key_indent = line.indent;

        let leading_spaces = line.content.len() - line.content.trim_start_matches(' ').len();
        let trimmed = &line.content[leading_spaces..];

        if trimmed.is_empty() {
            return None;
        }

        let key_pos = Pos {
            byte_offset: line.pos.byte_offset + leading_spaces,
            char_offset: line.pos.char_offset + leading_spaces,
            line: line.pos.line,
            column: line.pos.column + leading_spaces,
        };

        // Explicit key indicator: `? ` or `?` at EOL.
        if let Some(after_q) = trimmed.strip_prefix('?') {
            if after_q.is_empty()
                || after_q.starts_with(' ')
                || after_q.starts_with('\t')
                || after_q.starts_with('\n')
                || after_q.starts_with('\r')
            {
                return Some((key_indent, key_pos));
            }
        }

        // Implicit key: line contains `: ` or ends with `:`.
        // We scan the plain-scalar portion of the line for the value indicator.
        if is_implicit_mapping_line(trimmed) {
            return Some((key_indent, key_pos));
        }

        None
    }

    /// Try to consume a scalar from the current lexer position.
    ///
    /// `plain_parent_indent` — the indent of the current line; plain scalar
    /// continuation stops when the next line is less-indented than this.
    ///
    /// `block_parent_indent` — the indent of the enclosing block context;
    /// block scalars collect content that is more indented than this value.
    ///
    /// Consumes `self.pending_anchor` and attaches it to the emitted scalar.
    fn try_consume_scalar(
        &mut self,
        plain_parent_indent: usize,
        block_parent_indent: usize,
    ) -> Result<Option<(Event<'input>, Span)>, Error> {
        if let Some(result) = self
            .lexer
            .try_consume_literal_block_scalar(block_parent_indent)
        {
            let (value, chomp, span) = result?;
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Literal(chomp),
                    anchor: self.pending_anchor.take(),
                    tag: self.pending_tag.take(),
                },
                span,
            )));
        }
        if let Some(result) = self
            .lexer
            .try_consume_folded_block_scalar(block_parent_indent)
        {
            let (value, chomp, span) = result?;
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Folded(chomp),
                    anchor: self.pending_anchor.take(),
                    tag: self.pending_tag.take(),
                },
                span,
            )));
        }
        if let Some((value, span)) = self.lexer.try_consume_single_quoted(plain_parent_indent)? {
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::SingleQuoted,
                    anchor: self.pending_anchor.take(),
                    tag: self.pending_tag.take(),
                },
                span,
            )));
        }
        if let Some((value, span)) = self.lexer.try_consume_double_quoted(plain_parent_indent)? {
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::DoubleQuoted,
                    anchor: self.pending_anchor.take(),
                    tag: self.pending_tag.take(),
                },
                span,
            )));
        }
        if let Some((value, span)) = self.lexer.try_consume_plain_scalar(plain_parent_indent) {
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Plain,
                    anchor: self.pending_anchor.take(),
                    tag: self.pending_tag.take(),
                },
                span,
            )));
        }
        Ok(None)
    }

    /// Consume the leading `-` indicator from the current line and (if
    /// present) prepend a synthetic line for the inline content.
    ///
    /// Returns `true` if inline content was found and prepended.
    fn consume_sequence_dash(&mut self, dash_indent: usize) -> bool {
        // SAFETY: caller verified via peek_sequence_entry — the line exists.
        let Some(line) = self.lexer.peek_next_line() else {
            unreachable!("consume_sequence_dash called without a pending line")
        };

        let content = line.content;
        let after_spaces = content.trim_start_matches(' ');
        debug_assert!(
            after_spaces.starts_with('-'),
            "sequence dash not at expected position"
        );
        let rest_of_line = &after_spaces[1..];
        let inline = rest_of_line.trim_start_matches([' ', '\t']);
        let had_inline = !inline.is_empty();

        if had_inline {
            let leading_spaces = content.len() - after_spaces.len();
            let spaces_after_dash = rest_of_line.len() - inline.len();
            let offset_from_dash = 1 + spaces_after_dash;
            let total_offset = leading_spaces + offset_from_dash;
            let inline_col = dash_indent + offset_from_dash;
            let inline_pos = Pos {
                byte_offset: line.pos.byte_offset + total_offset,
                char_offset: line.pos.char_offset + total_offset,
                line: line.pos.line,
                column: line.pos.column + total_offset,
            };
            let synthetic = Line {
                content: inline,
                offset: inline_pos.byte_offset,
                indent: inline_col,
                break_type: line.break_type,
                pos: inline_pos,
            };
            self.lexer.consume_line();
            self.lexer.prepend_inline_line(synthetic);
        } else {
            self.lexer.consume_line();
        }

        had_inline
    }

    /// Consume the current mapping-entry line.
    ///
    /// Handles both forms:
    /// - **Explicit key** (`? key`): consume the `?` indicator line, extract
    ///   any inline key content and prepend a synthetic line for it.
    /// - **Implicit key** (`key: value`): split the line at the `: ` / `:\n`
    ///   boundary.  Return the key as a pre-extracted slice so the caller can
    ///   emit it as a `Scalar` event directly (bypassing the plain-scalar
    ///   continuation logic).  Prepend the value portion (if non-empty) as a
    ///   synthetic line.
    ///
    /// Returns a `ConsumedMapping` describing what was found.
    fn consume_mapping_entry(&mut self, key_indent: usize) -> ConsumedMapping<'input> {
        // SAFETY: caller verified via peek_mapping_entry — the line exists.
        let Some(line) = self.lexer.peek_next_line() else {
            unreachable!("consume_mapping_entry called without a pending line")
        };

        // Extract all data from the borrowed line before any mutable lexer calls.
        // `content` is `'input`-lived (borrows the original input string, not
        // the lexer's internal buffer), so it remains valid after consume_line().
        let content: &'input str = line.content;
        let line_pos = line.pos;
        let line_break_type = line.break_type;

        let leading_spaces = content.len() - content.trim_start_matches(' ').len();
        let trimmed = &content[leading_spaces..];

        // --- Explicit key: `? ...` ---
        if let Some(after_q) = trimmed.strip_prefix('?') {
            let inline = after_q.trim_start_matches([' ', '\t']);
            let had_key_inline = !inline.is_empty();

            if had_key_inline {
                // Offset from line start to inline key content.
                let spaces_after_q = after_q.len() - inline.len();
                let total_offset = leading_spaces + 1 + spaces_after_q;
                let inline_col = key_indent + 1 + spaces_after_q;
                let inline_pos = Pos {
                    byte_offset: line_pos.byte_offset + total_offset,
                    char_offset: line_pos.char_offset + total_offset,
                    line: line_pos.line,
                    column: line_pos.column + total_offset,
                };
                let synthetic = Line {
                    content: inline,
                    offset: inline_pos.byte_offset,
                    indent: inline_col,
                    break_type: line_break_type,
                    pos: inline_pos,
                };
                self.lexer.consume_line();
                self.lexer.prepend_inline_line(synthetic);
            } else {
                self.lexer.consume_line();
            }
            return ConsumedMapping::ExplicitKey { had_key_inline };
        }

        // --- Implicit key: `key: value` or `key:` ---
        // Find the `: ` (or `:\t` or `:\n` or `:` at EOL) boundary.
        // SAFETY: peek_mapping_entry already confirmed this line is a mapping
        // entry, so find_value_indicator_offset will return Some.
        let Some(colon_offset) = find_value_indicator_offset(trimmed) else {
            unreachable!("consume_mapping_entry: implicit key line has no value indicator")
        };

        let key_content = trimmed[..colon_offset].trim_end_matches([' ', '\t']);
        let after_colon = &trimmed[colon_offset + 1..]; // skip ':'
        let value_content = after_colon.trim_start_matches([' ', '\t']);

        // Key span: starts at the first non-space character.
        let key_start_pos = Pos {
            byte_offset: line_pos.byte_offset + leading_spaces,
            char_offset: line_pos.char_offset + leading_spaces,
            line: line_pos.line,
            column: line_pos.column + leading_spaces,
        };
        let key_end_pos = {
            let mut p = key_start_pos;
            for ch in key_content.chars() {
                p = p.advance(ch);
            }
            p
        };
        let key_span = Span {
            start: key_start_pos,
            end: key_end_pos,
        };

        // Compute position of value content (after `: ` / `:\t`).
        let spaces_after_colon = after_colon.len() - value_content.len();
        let value_offset_in_trimmed = colon_offset + 1 + spaces_after_colon;
        let value_col = key_indent + value_offset_in_trimmed;
        let value_pos = Pos {
            byte_offset: line_pos.byte_offset + leading_spaces + value_offset_in_trimmed,
            char_offset: line_pos.char_offset + leading_spaces + value_offset_in_trimmed,
            line: line_pos.line,
            column: line_pos.column + leading_spaces + value_offset_in_trimmed,
        };

        // Consume the physical line, then (if there is inline value content)
        // prepend one synthetic line for the value.  The key is returned
        // directly in the ConsumedMapping variant — not via a synthetic line —
        // so that the caller can push a Scalar event without routing through
        // try_consume_plain_scalar (which would incorrectly treat the value
        // synthetic line as a plain-scalar continuation).
        self.lexer.consume_line();

        if !value_content.is_empty() {
            let value_synthetic = Line {
                content: value_content,
                offset: value_pos.byte_offset,
                indent: value_col,
                break_type: line_break_type,
                pos: value_pos,
            };
            self.lexer.prepend_inline_line(value_synthetic);
        }

        ConsumedMapping::ImplicitKey {
            key_value: key_content,
            key_span,
        }
    }

    /// After emitting a key scalar, flip the innermost mapping to `Value` phase.
    ///
    /// **Call-site invariant:** the top of `coll_stack` must be a
    /// `CollectionEntry::Mapping`.  This function is only called from
    /// mapping-emission paths (`handle_mapping_entry`, explicit-key handling)
    /// where the caller has already verified that a mapping is the active
    /// collection.  Do **not** call this after emitting a scalar that may be a
    /// sequence item — use `tick_mapping_phase_after_scalar` instead, which
    /// stops at a Sequence entry and handles the ambiguity correctly.
    fn advance_mapping_to_value(&mut self) {
        debug_assert!(
            matches!(self.coll_stack.last(), Some(CollectionEntry::Mapping(..))),
            "advance_mapping_to_value called but top of coll_stack is not a Mapping"
        );
        for entry in self.coll_stack.iter_mut().rev() {
            if let CollectionEntry::Mapping(_, phase) = entry {
                *phase = MappingPhase::Value;
                return;
            }
        }
    }

    /// Drain any pending trailing comment from the lexer into the event queue.
    ///
    /// Called after emitting a scalar event.  If a trailing comment was
    /// detected on the scalar's line (e.g. `foo # comment`), it is pushed to
    /// `self.queue` as `Event::Comment`.
    ///
    /// Trailing comments are bounded by the physical line length, which is
    /// itself bounded by the total input size.  No separate length limit is
    /// applied here; the security constraint (`MAX_COMMENT_LEN`) applies to
    /// standalone comment lines (scanned in [`Self::skip_and_collect_comments_in_doc`]
    /// and [`Self::skip_and_collect_comments_between_docs`]).
    fn drain_trailing_comment(&mut self) {
        if let Some((text, span)) = self.lexer.trailing_comment.take() {
            self.queue.push_back((Event::Comment { text }, span));
        }
    }

    /// After emitting a value scalar/collection, flip the innermost mapping
    /// back to `Key` phase.
    ///
    /// **Call-site invariant:** the top of `coll_stack` must be a
    /// `CollectionEntry::Mapping`.  This function is only called from
    /// mapping-emission paths where the caller has already verified that a
    /// mapping is the active collection.  Do **not** call this after emitting a
    /// scalar that may be a sequence item — use `tick_mapping_phase_after_scalar`
    /// instead.
    fn advance_mapping_to_key(&mut self) {
        debug_assert!(
            matches!(self.coll_stack.last(), Some(CollectionEntry::Mapping(..))),
            "advance_mapping_to_key called but top of coll_stack is not a Mapping"
        );
        for entry in self.coll_stack.iter_mut().rev() {
            if let CollectionEntry::Mapping(_, phase) = entry {
                *phase = MappingPhase::Key;
                return;
            }
        }
    }
}

/// Result of consuming a mapping-entry line.
enum ConsumedMapping<'input> {
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
        /// The key text slice (borrows input).
        key_value: &'input str,
        /// Span covering the key text.
        key_span: Span,
    },
}

/// True when `trimmed` (content after stripping leading spaces) represents
/// an implicit mapping key: it contains `: `, `:\t`, or ends with `:`.
fn is_implicit_mapping_line(trimmed: &str) -> bool {
    find_value_indicator_offset(trimmed).is_some()
}

/// Return the byte offset of the `:` value indicator within `trimmed`, or
/// `None` if the line is not a mapping entry.
///
/// The `:` must be followed by a space, tab, newline/CR, or end-of-string to
/// count as a value indicator (YAML 1.2 §7.4).  A `:` immediately followed by
/// a non-space `ns-char` is part of a plain scalar.
///
/// Double-quoted and single-quoted spans are skipped correctly: a `:` inside
/// quotes is not a value indicator.
///
/// Lines that begin with YAML indicator characters that cannot start a plain
/// scalar (e.g. `%`, `@`, `` ` ``, `,`, `[`, `]`, `{`, `}`, `#`, `&`, `*`,
/// `!`, `|`, `>`) are rejected immediately — they are not implicit mapping
/// keys.  Quoted-scalar starts (`"`, `'`) and bare-indicator starts (`?`, `-`,
/// `:`) are handled specially.
fn find_value_indicator_offset(trimmed: &str) -> Option<usize> {
    // Reject lines that start with indicator characters that cannot begin a
    // plain scalar (and are thus not valid implicit mapping keys).
    if matches!(
        trimmed.as_bytes().first().copied(),
        Some(
            b'%' | b'@'
                | b'`'
                | b','
                | b'['
                | b']'
                | b'{'
                | b'}'
                | b'#'
                | b'&'
                | b'*'
                | b'!'
                | b'|'
                | b'>'
        )
    ) {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut i = 0;
    let mut prev_was_space = false; // tracks whether the previous byte was whitespace
    while let Some(&ch) = bytes.get(i) {
        // Stop at an unquoted `#` preceded by whitespace (or at position 0):
        // YAML 1.2 §6.6 — a `#` after whitespace begins a comment; any `:` that
        // follows is inside the comment and cannot be a value indicator.
        if ch == b'#' && (i == 0 || prev_was_space) {
            return None;
        }

        // Skip double-quoted span (handles `\"` escapes).
        // After a quoted span, `prev_was_space` is false — a closing `"` is
        // not whitespace.
        if ch == b'"' {
            i += 1; // skip opening `"`
            while let Some(&inner) = bytes.get(i) {
                match inner {
                    b'\\' => i += 2, // skip escape sequence (two bytes)
                    b'"' => {
                        i += 1; // skip closing `"`
                        break;
                    }
                    _ => i += 1,
                }
            }
            prev_was_space = false;
            continue;
        }

        // Skip single-quoted span (handles `''` escape).
        // After a quoted span, `prev_was_space` is false — a closing `'` is
        // not whitespace.
        if ch == b'\'' {
            i += 1; // skip opening `'`
            while let Some(&inner) = bytes.get(i) {
                i += 1;
                if inner == b'\'' {
                    // `''` is an escaped single-quote; a lone `'` ends the span.
                    if bytes.get(i).copied() == Some(b'\'') {
                        i += 1; // consume the second `'` of the `''` escape
                    } else {
                        break; // lone `'` — end of quoted span
                    }
                }
            }
            prev_was_space = false;
            continue;
        }

        if ch == b':' {
            match bytes.get(i + 1).copied() {
                None | Some(b' ' | b'\t' | b'\n' | b'\r') => return Some(i),
                _ => {}
            }
        }

        prev_was_space = ch == b' ' || ch == b'\t';

        // Multi-byte char: advance by UTF-8 lead-byte length.
        i += if ch < 0x80 {
            1
        } else if ch & 0xE0 == 0xC0 {
            2
        } else if ch & 0xF0 == 0xE0 {
            3
        } else {
            4
        };
    }
    None
}

/// Scan an anchor name from `content`, returning the name slice.
///
/// `content` must begin immediately after the `&` or `*` indicator — the first
/// character is the first character of the name.  The name continues until
/// a character that is not `ns-anchor-char` (i.e., whitespace, flow indicator,
/// or end of content).
///
/// Returns `Ok(name)` where `name` is a non-empty borrowed slice of `content`.
/// Returns `Err` if:
/// - The name would be empty (first character is not `ns-anchor-char`).
/// - The name exceeds [`MAX_ANCHOR_NAME_BYTES`] bytes.
///
/// The caller is responsible for providing the correct [`Pos`] for error
/// reporting.
fn scan_anchor_name(content: &str, indicator_pos: Pos) -> Result<&str, Error> {
    use crate::chars::is_ns_anchor_char;
    let end = content
        .char_indices()
        .take_while(|&(_, ch)| is_ns_anchor_char(ch))
        .last()
        .map_or(0, |(i, ch)| i + ch.len_utf8());
    if end == 0 {
        return Err(Error {
            pos: indicator_pos,
            message: "anchor name must not be empty".into(),
        });
    }
    if end > MAX_ANCHOR_NAME_BYTES {
        return Err(Error {
            pos: indicator_pos,
            message: format!("anchor name exceeds maximum length of {MAX_ANCHOR_NAME_BYTES} bytes"),
        });
    }
    Ok(&content[..end])
}

/// Scan a tag from `content`, returning the tag slice and its byte length in `content`.
///
/// `content` must begin immediately after the `!` indicator.  The function
/// handles all four YAML 1.2 §6.8.1 tag forms:
///
/// - **Verbatim** `!<URI>` → `content` starts with `<`; returns the URI
///   (between the angle brackets) and its length including the `<` and `>`.
/// - **Primary shorthand** `!!suffix` → `content` starts with `!`; returns
///   the full `!!suffix` slice (including the leading `!` that is part of
///   `content`).
/// - **Named-handle shorthand** `!handle!suffix` → returns the full slice
///   `!handle!suffix` (the leading `!` of `handle` is in `content`).
/// - **Secondary shorthand** `!suffix` → `content` starts with a tag-char;
///   returns `!suffix` via a slice that includes one byte before `content`
///   (the caller provides `full_tag_start` for this).
/// - **Non-specific** `!` alone → `content` is empty or starts with a
///   separator; returns `"!"` as a one-byte slice of the `!` indicator.
///
/// # Parameters
///
/// - `content`: the input slice immediately after the `!` indicator character.
/// - `tag_start`: the input slice starting at the `!` (one byte before `content`).
/// - `indicator_pos`: the [`Pos`] of the `!` indicator (for error reporting).
///
/// # Returns
///
/// `Ok((tag_slice, advance_past_exclamation))` where:
/// - `tag_slice` is the borrowed slice to store in `pending_tag`.
/// - `advance_past_exclamation` is the number of bytes to advance past the
///   `!` indicator (i.e. the advance for the entire tag token, not counting
///   the `!` itself).
///
/// Returns `Err` on invalid verbatim tags (unmatched `<`, empty URI, control
/// character in URI) or when the tag length exceeds [`MAX_TAG_LEN`].
fn scan_tag<'i>(
    content: &'i str,
    tag_start: &'i str,
    indicator_pos: Pos,
) -> Result<(&'i str, usize), Error> {
    // ---- Verbatim tag: `!<URI>` ----
    if let Some(after_open) = content.strip_prefix('<') {
        // Find the closing `>`.
        let close = after_open.find('>').ok_or_else(|| Error {
            pos: indicator_pos,
            message: "verbatim tag missing closing '>'".into(),
        })?;
        let uri = &after_open[..close];
        if uri.is_empty() {
            return Err(Error {
                pos: indicator_pos,
                message: "verbatim tag URI must not be empty".into(),
            });
        }
        if uri.len() > MAX_TAG_LEN {
            return Err(Error {
                pos: indicator_pos,
                message: format!("verbatim tag URI exceeds maximum length of {MAX_TAG_LEN} bytes"),
            });
        }
        // Reject control characters in the URI.
        for ch in uri.chars() {
            if ch < '\x20' || ch == '\x7F' {
                return Err(Error {
                    pos: indicator_pos,
                    message: format!("verbatim tag URI contains invalid character {ch:?}"),
                });
            }
        }
        // advance = 1 (for '<') + uri.len() + 1 (for '>') bytes past the `!`
        let advance = 1 + uri.len() + 1;
        return Ok((uri, advance));
    }

    // ---- Primary handle: `!!suffix` ----
    if let Some(suffix) = content.strip_prefix('!') {
        // suffix starts after the second `!`
        let suffix_bytes = scan_tag_suffix(suffix);
        // `!!` alone with no suffix is valid (empty suffix shorthand).
        if suffix_bytes > MAX_TAG_LEN {
            return Err(Error {
                pos: indicator_pos,
                message: format!("tag exceeds maximum length of {MAX_TAG_LEN} bytes"),
            });
        }
        // tag_slice = `!!suffix` — one byte back for the first `!` (in `tag_start`)
        // plus `!` in content plus suffix.
        let tag_slice = &tag_start[..2 + suffix_bytes]; // `!` + `!` + suffix
        let advance = 1 + suffix_bytes; // past the `!` in content and suffix
        return Ok((tag_slice, advance));
    }

    // ---- Non-specific tag: bare `!` (content is empty or starts with non-tag-char) ----
    // A `%` alone (without two following hex digits) also falls here via scan_tag_suffix.
    if scan_tag_suffix(content) == 0 {
        // The tag is just `!` — a one-byte slice from `tag_start`.
        let tag_slice = &tag_start[..1];
        return Ok((tag_slice, 0)); // 0 bytes advance past `!` (nothing follows the `!`)
    }

    // ---- Named handle `!handle!suffix` or secondary handle `!suffix` ----
    // Scan tag chars until we hit a `!` (named handle delimiter) or non-tag-char.
    let mut end = 0;
    let mut found_inner_bang = false;
    for (i, ch) in content.char_indices() {
        if ch == '!' {
            // Named handle: `!handle!suffix` — scan the suffix after the inner `!`.
            found_inner_bang = true;
            end = i + 1; // include the `!`
            // Scan suffix chars (and %HH sequences) after the inner `!`.
            end += scan_tag_suffix(&content[i + 1..]);
            break;
        } else if is_tag_char(ch) {
            end = i + ch.len_utf8();
        } else if ch == '%' {
            // Percent-encoded sequence: %HH.
            let pct_len = scan_tag_suffix(&content[i..]);
            if pct_len == 0 {
                break; // bare `%` without two hex digits — stop
            }
            end = i + pct_len;
        } else {
            break;
        }
    }

    if end == 0 && !found_inner_bang {
        // No tag chars at all (covered by non-specific check above, but defensive).
        let tag_slice = &tag_start[..1];
        return Ok((tag_slice, 0));
    }

    if end > MAX_TAG_LEN {
        return Err(Error {
            pos: indicator_pos,
            message: format!("tag exceeds maximum length of {MAX_TAG_LEN} bytes"),
        });
    }

    // tag_slice = `!` + content[..end] — includes the leading `!` from tag_start.
    let tag_slice = &tag_start[..=end];
    Ok((tag_slice, end))
}

/// Returns true if `ch` is a valid YAML 1.2 `ns-tag-char` (§6.8.1) single character.
///
/// This is the *closed* set defined in the spec: `ns-uri-char` minus `!` and
/// the flow indicators.  `%` is NOT included here — percent-encoded sequences
/// (`%HH`) are handled separately via [`scan_tag_suffix`].
const fn is_tag_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '-' | '_'
                | '.'
                | '~'
                | '*'
                | '\''
                | '('
                | ')'
                | '#'
                | ';'
                | '/'
                | '?'
                | ':'
                | '@'
                | '&'
                | '='
                | '+'
                | '$'
        )
}

/// Returns the byte length of the valid tag suffix starting at `s`.
///
/// A tag suffix is a sequence of `ns-tag-char` characters and percent-encoded
/// `%HH` sequences (YAML 1.2 §6.8.1).  Scanning stops at the first character
/// that does not satisfy either condition.
fn scan_tag_suffix(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        // Percent-encoded sequence: `%` followed by exactly two hex digits.
        if bytes.get(pos) == Some(&b'%') {
            let h1 = bytes
                .get(pos + 1)
                .copied()
                .is_some_and(|b| b.is_ascii_hexdigit());
            let h2 = bytes
                .get(pos + 2)
                .copied()
                .is_some_and(|b| b.is_ascii_hexdigit());
            if h1 && h2 {
                pos += 3;
                continue;
            }
            break;
        }
        // Safe to decode the next char: all is_tag_char matches are ASCII,
        // so multi-byte UTF-8 chars will fail is_tag_char and stop the scan.
        let Some(ch) = s[pos..].chars().next() else {
            break;
        };
        if is_tag_char(ch) {
            pos += ch.len_utf8();
        } else {
            break;
        }
    }
    pos
}

/// Build an empty plain scalar event.
const fn empty_scalar_event<'input>() -> Event<'input> {
    Event::Scalar {
        value: std::borrow::Cow::Borrowed(""),
        style: ScalarStyle::Plain,
        anchor: None,
        tag: None,
    }
}

/// Build a span that covers exactly the 3-byte document marker at `marker_pos`.
const fn marker_span(marker_pos: Pos) -> Span {
    Span {
        start: marker_pos,
        end: Pos {
            byte_offset: marker_pos.byte_offset + 3,
            char_offset: marker_pos.char_offset + 3,
            line: marker_pos.line,
            column: marker_pos.column + 3,
        },
    }
}

/// Build a zero-width span at `pos`.
const fn zero_span(pos: Pos) -> Span {
    Span {
        start: pos,
        end: pos,
    }
}

/// Returns `true` if `handle` is a syntactically valid YAML tag handle.
///
/// Valid forms per YAML 1.2 §6.8.1 productions [89]–[92]:
/// - `!`   — primary tag handle
/// - `!!`  — secondary tag handle
/// - `!<word-chars>!` — named tag handle, where word chars are `[a-zA-Z0-9_-]`
fn is_valid_tag_handle(handle: &str) -> bool {
    match handle {
        "!" | "!!" => true,
        _ => {
            // Named handle: starts and ends with `!`, interior non-empty word chars.
            let inner = handle.strip_prefix('!').and_then(|s| s.strip_suffix('!'));
            match inner {
                Some(word) if !word.is_empty() => word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                _ => false,
            }
        }
    }
}

impl<'input> EventIter<'input> {
    /// Consume blank lines, comment lines, and directive lines in `BetweenDocs`
    /// context.
    ///
    /// - Blank lines: silently consumed.
    /// - Comment lines: emitted as `Event::Comment` items into `self.queue`.
    /// - Directive lines (`%`-prefixed): parsed and accumulated into
    ///   `self.directive_scope`.
    ///
    /// Returns `Err` on malformed directives, exceeded limits, or comment
    /// bodies exceeding `MAX_COMMENT_LEN`.  Stops at the first non-blank,
    /// non-comment, non-directive line (i.e. `---`, `...`, or content).
    ///
    /// The caller is responsible for resetting `self.directive_scope` before
    /// entering the `BetweenDocs` state (at each document boundary transition).
    /// This function does NOT reset it — `step_between_docs` re-enters it on
    /// every comment yield, so resetting here would clobber directives parsed
    /// on earlier re-entries for the same document.
    fn consume_preamble_between_docs(&mut self) -> Result<(), Error> {
        loop {
            // Skip blank lines first.
            self.lexer.skip_blank_lines_between_docs();

            // Collect comment lines.
            while self.lexer.is_comment_line() {
                match self.lexer.try_consume_comment(MAX_COMMENT_LEN) {
                    Ok(Some((text, span))) => {
                        self.queue.push_back((Event::Comment { text }, span));
                    }
                    Ok(None) => break,
                    Err(e) => return Err(e),
                }
                self.lexer.skip_blank_lines_between_docs();
            }

            // Parse directive lines.
            while self.lexer.is_directive_line() {
                let Some((content, dir_pos)) = self.lexer.try_consume_directive_line() else {
                    break;
                };
                self.parse_directive(content, dir_pos)?;
                self.lexer.skip_blank_lines_between_docs();
            }

            // After parsing directives, there may be more blank lines or comments.
            if !self.lexer.is_comment_line() && !self.lexer.is_directive_line() {
                return Ok(());
            }
        }
    }

    /// Parse a single directive line and update `self.directive_scope`.
    ///
    /// `content` is the full line content starting with `%` (e.g. `"%YAML 1.2"`).
    /// `dir_pos` is the position of the `%` character.
    fn parse_directive(&mut self, content: &'input str, dir_pos: Pos) -> Result<(), Error> {
        // Enforce per-document directive count limit.
        if self.directive_scope.directive_count >= MAX_DIRECTIVES_PER_DOC {
            return Err(Error {
                pos: dir_pos,
                message: format!(
                    "directive count exceeds maximum of {MAX_DIRECTIVES_PER_DOC} per document"
                ),
            });
        }

        // `content` starts with `%`; the rest is `NAME[ params...]`.
        let after_percent = &content[1..];

        // Determine directive name (up to first whitespace).
        let name_end = after_percent
            .find([' ', '\t'])
            .unwrap_or(after_percent.len());
        let name = &after_percent[..name_end];
        let rest = after_percent[name_end..].trim_start_matches([' ', '\t']);

        match name {
            "YAML" => self.parse_yaml_directive(rest, dir_pos),
            "TAG" => self.parse_tag_directive(rest, dir_pos),
            _ => {
                // Reserved directive — silently ignore per YAML 1.2 spec.
                self.directive_scope.directive_count += 1;
                Ok(())
            }
        }
    }

    /// Parse `%YAML major.minor` and store in directive scope.
    fn parse_yaml_directive(&mut self, params: &str, dir_pos: Pos) -> Result<(), Error> {
        if self.directive_scope.version.is_some() {
            return Err(Error {
                pos: dir_pos,
                message: "duplicate %YAML directive in the same document".into(),
            });
        }

        // Parse `major.minor`.
        let dot = params.find('.').ok_or_else(|| Error {
            pos: dir_pos,
            message: format!("malformed %YAML directive: expected 'major.minor', got {params:?}"),
        })?;
        let major_str = &params[..dot];
        let after_dot = &params[dot + 1..];
        // Minor version ends at first whitespace or end of string.
        let minor_end = after_dot.find([' ', '\t']).unwrap_or(after_dot.len());
        let minor_str = &after_dot[..minor_end];
        // Anything after the minor version must be empty or a comment (# ...).
        let trailing = after_dot[minor_end..].trim_start_matches([' ', '\t']);
        if !trailing.is_empty() && !trailing.starts_with('#') {
            return Err(Error {
                pos: dir_pos,
                message: format!(
                    "malformed %YAML directive: unexpected trailing content {trailing:?}"
                ),
            });
        }

        let major = major_str.parse::<u8>().map_err(|_| Error {
            pos: dir_pos,
            message: format!("malformed %YAML major version: {major_str:?}"),
        })?;
        let minor = minor_str.parse::<u8>().map_err(|_| Error {
            pos: dir_pos,
            message: format!("malformed %YAML minor version: {minor_str:?}"),
        })?;

        // Only major version 1 is accepted; 2+ is a hard error.
        if major != 1 {
            return Err(Error {
                pos: dir_pos,
                message: format!("unsupported YAML version {major}.{minor}: only 1.x is supported"),
            });
        }

        self.directive_scope.version = Some((major, minor));
        self.directive_scope.directive_count += 1;
        Ok(())
    }

    /// Parse `%TAG !handle! prefix` and store in directive scope.
    fn parse_tag_directive(&mut self, params: &'input str, dir_pos: Pos) -> Result<(), Error> {
        // Split on whitespace to get handle and prefix.
        let handle_end = params.find([' ', '\t']).ok_or_else(|| Error {
            pos: dir_pos,
            message: format!("malformed %TAG directive: expected 'handle prefix', got {params:?}"),
        })?;
        let handle = &params[..handle_end];
        let prefix = params[handle_end..].trim_start_matches([' ', '\t']);

        if prefix.is_empty() {
            return Err(Error {
                pos: dir_pos,
                message: "malformed %TAG directive: missing prefix".into(),
            });
        }

        // Validate handle shape: must be `!`, `!!`, or `!<word-chars>!`
        // where word chars are ASCII alphanumeric, `-`, or `_`
        // (YAML 1.2 §6.8.1 productions [89]–[92]).
        if !is_valid_tag_handle(handle) {
            return Err(Error {
                pos: dir_pos,
                message: format!("malformed %TAG handle: {handle:?} is not a valid tag handle"),
            });
        }

        // Validate handle length.
        if handle.len() > MAX_TAG_HANDLE_BYTES {
            return Err(Error {
                pos: dir_pos,
                message: format!(
                    "tag handle exceeds maximum length of {MAX_TAG_HANDLE_BYTES} bytes"
                ),
            });
        }

        // Validate prefix length.
        if prefix.len() > MAX_TAG_LEN {
            return Err(Error {
                pos: dir_pos,
                message: format!("tag prefix exceeds maximum length of {MAX_TAG_LEN} bytes"),
            });
        }

        // Reject control characters in prefix.
        for ch in prefix.chars() {
            if (ch as u32) < 0x20 || ch == '\x7F' {
                return Err(Error {
                    pos: dir_pos,
                    message: format!("tag prefix contains invalid control character {ch:?}"),
                });
            }
        }

        // Duplicate handle check.
        if self.directive_scope.tag_handles.contains_key(handle) {
            return Err(Error {
                pos: dir_pos,
                message: format!("duplicate %TAG directive for handle {handle:?}"),
            });
        }

        self.directive_scope
            .tag_handles
            .insert(handle.to_owned(), prefix.to_owned());
        self.directive_scope.directive_count += 1;
        Ok(())
    }

    /// Skip blank lines while collecting any comment lines encountered as
    /// `Event::Comment` items pushed to `self.queue`.
    ///
    /// Used in `InDocument` context.
    /// Returns `Err` if a comment body exceeds `MAX_COMMENT_LEN`.
    fn skip_and_collect_comments_in_doc(&mut self) -> Result<(), Error> {
        loop {
            // Skip truly blank lines (not comments).
            self.lexer.skip_empty_lines();
            // Collect any comment lines.
            if !self.lexer.is_comment_line() {
                return Ok(());
            }
            while self.lexer.is_comment_line() {
                match self.lexer.try_consume_comment(MAX_COMMENT_LEN) {
                    Ok(Some((text, span))) => {
                        self.queue.push_back((Event::Comment { text }, span));
                    }
                    Ok(None) => break,
                    Err(e) => return Err(e),
                }
            }
            // Loop to skip any blank lines that follow the comments.
        }
    }

    /// Handle one iteration step in the `BetweenDocs` state.
    fn step_between_docs(&mut self) -> StepResult<'input> {
        match self.consume_preamble_between_docs() {
            Ok(()) => {}
            Err(e) => {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
        }
        // If comments were queued, drain them before checking document state.
        if !self.queue.is_empty() {
            return StepResult::Continue;
        }

        if self.lexer.at_eof() {
            let end = self.lexer.current_pos();
            self.state = IterState::Done;
            return StepResult::Yield(Ok((Event::StreamEnd, zero_span(end))));
        }
        if self.lexer.is_directives_end() {
            let (marker_pos, _) = self.lexer.consume_marker_line();
            self.state = IterState::InDocument;
            // Take the accumulated directives — scope stays active for document body tag resolution.
            let version = self.directive_scope.version;
            let tag_directives = self.directive_scope.tag_directives();
            self.queue.push_back((
                Event::DocumentStart {
                    explicit: true,
                    version,
                    tag_directives,
                },
                marker_span(marker_pos),
            ));
            self.drain_trailing_comment();
            return StepResult::Continue;
        }
        if self.lexer.is_document_end() {
            // Orphan `...` — if directives were parsed without a `---` marker,
            // that is a spec violation (YAML 1.2 §9.2: directives require `---`).
            if self.directive_scope.directive_count > 0 {
                let pos = self.lexer.current_pos();
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos,
                    message: "directives must be followed by a '---' document-start marker".into(),
                }));
            }
            self.lexer.consume_marker_line();
            return StepResult::Continue; // orphan `...`, no event
        }
        // Per YAML 1.2 §9.2, directives require a `---` marker.  If the next
        // line is not `---` and we have already parsed directives, that is a
        // spec violation — reject before emitting an implicit DocumentStart.
        if self.directive_scope.directive_count > 0 {
            let pos = self.lexer.current_pos();
            self.failed = true;
            return StepResult::Yield(Err(Error {
                pos,
                message: "directives must be followed by a '---' document-start marker".into(),
            }));
        }
        debug_assert!(
            self.lexer.has_content(),
            "expected content after consuming blank/comment/directive lines"
        );
        let content_pos = self.lexer.current_pos();
        self.state = IterState::InDocument;
        // Take the accumulated directives — scope stays active for document body tag resolution.
        let version = self.directive_scope.version;
        let tag_directives = self.directive_scope.tag_directives();
        StepResult::Yield(Ok((
            Event::DocumentStart {
                explicit: false,
                version,
                tag_directives,
            },
            zero_span(content_pos),
        )))
    }

    /// Handle one iteration step in the `InDocument` state.
    #[allow(clippy::too_many_lines)]
    fn step_in_document(&mut self) -> StepResult<'input> {
        match self.skip_and_collect_comments_in_doc() {
            Ok(()) => {}
            Err(e) => {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
        }
        // If comments were queued, drain them before checking document state.
        if !self.queue.is_empty() {
            return StepResult::Continue;
        }

        // ---- Document / stream boundaries ----

        if self.lexer.at_eof() && !self.lexer.has_inline_scalar() {
            let end = self.lexer.drain_to_end();
            self.close_all_collections(end);
            self.queue
                .push_back((Event::DocumentEnd { explicit: false }, zero_span(end)));
            self.queue.push_back((Event::StreamEnd, zero_span(end)));
            self.state = IterState::Done;
            return StepResult::Continue;
        }
        if self.lexer.is_document_end() {
            let pos = self.lexer.current_pos();
            self.close_all_collections(pos);
            let (marker_pos, _) = self.lexer.consume_marker_line();
            // Reset directive scope at the document boundary so directives from
            // this document do not leak into the next one.
            self.directive_scope = DirectiveScope::default();
            self.state = IterState::BetweenDocs;
            self.queue.push_back((
                Event::DocumentEnd { explicit: true },
                marker_span(marker_pos),
            ));
            self.drain_trailing_comment();
            return StepResult::Continue;
        }
        if self.lexer.is_directives_end() {
            let pos = self.lexer.current_pos();
            self.close_all_collections(pos);
            let (marker_pos, _) = self.lexer.consume_marker_line();
            // A bare `---` inside a document implicitly ends the current document
            // and starts a new one without a preamble.  Reset the directive scope
            // here since consume_preamble_between_docs will not be called for this
            // transition.
            self.directive_scope = DirectiveScope::default();
            self.state = IterState::InDocument;
            self.queue.push_back((
                Event::DocumentEnd { explicit: false },
                zero_span(marker_pos),
            ));
            self.queue.push_back((
                Event::DocumentStart {
                    explicit: true,
                    version: None,
                    tag_directives: Vec::new(),
                },
                marker_span(marker_pos),
            ));
            self.drain_trailing_comment();
            return StepResult::Continue;
        }

        // ---- Alias node: `*name` is a complete node ----

        if let Some(peek) = self.lexer.peek_next_line() {
            let content: &'input str = peek.content;
            let line_pos = peek.pos;
            let line_break_type = peek.break_type;
            let line_char_offset = line_pos.char_offset;
            let trimmed = content.trim_start_matches(' ');
            if let Some(after_star) = trimmed.strip_prefix('*') {
                let leading = content.len() - trimmed.len();
                let star_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
                    char_offset: line_char_offset + leading,
                    line: line_pos.line,
                    column: line_pos.column + leading,
                };
                // YAML 1.2 §7.1: alias nodes cannot have properties (anchor or tag).
                if self.pending_tag.is_some() {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have a tag property".into(),
                    }));
                }
                match scan_anchor_name(after_star, star_pos) {
                    Err(e) => {
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                    Ok(name) => {
                        let name_char_count = name.chars().count();
                        // Build alias span: from `*` through end of name.
                        let alias_end = Pos {
                            byte_offset: star_pos.byte_offset + 1 + name.len(),
                            char_offset: star_pos.char_offset + 1 + name_char_count,
                            line: star_pos.line,
                            column: star_pos.column + 1 + name_char_count,
                        };
                        let alias_span = Span {
                            start: star_pos,
                            end: alias_end,
                        };
                        // Compute remaining content after the alias name, before
                        // consuming the line (which would invalidate the borrow).
                        let after_name = &after_star[name.len()..];
                        let remaining: &'input str = after_name.trim_start_matches([' ', '\t']);
                        let spaces = after_name.len() - remaining.len();
                        let had_remaining = !remaining.is_empty();
                        let rem_byte_offset = star_pos.byte_offset + 1 + name.len() + spaces;
                        let rem_char_offset = line_char_offset + leading + 1 + name.len() + spaces;
                        let rem_col = star_pos.column + 1 + name_char_count + spaces;
                        self.lexer.consume_line();
                        if had_remaining {
                            let rem_pos = Pos {
                                byte_offset: rem_byte_offset,
                                char_offset: rem_char_offset,
                                line: star_pos.line,
                                column: rem_col,
                            };
                            let synthetic = crate::lines::Line {
                                content: remaining,
                                offset: rem_byte_offset,
                                indent: rem_col,
                                break_type: line_break_type,
                                pos: rem_pos,
                            };
                            self.lexer.prepend_inline_line(synthetic);
                        }
                        self.tick_mapping_phase_after_scalar();
                        return StepResult::Yield(Ok((Event::Alias { name }, alias_span)));
                    }
                }
            }
        }

        // ---- Tag: `!tag`, `!!tag`, `!<uri>`, or `!` — attach to next node ----

        if let Some(peek) = self.lexer.peek_next_line() {
            let content: &'input str = peek.content;
            let line_pos = peek.pos;
            let line_break_type = peek.break_type;
            let trimmed = content.trim_start_matches(' ');
            if trimmed.starts_with('!') {
                let leading = content.len() - trimmed.len();
                let bang_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
                    char_offset: line_pos.char_offset + leading,
                    line: line_pos.line,
                    column: line_pos.column + leading,
                };
                // `tag_start` starts at the `!`; `after_bang` is everything after it.
                let tag_start: &'input str = &content[leading..];
                let after_bang: &'input str = &content[leading + 1..];
                match scan_tag(after_bang, tag_start, bang_pos) {
                    Err(e) => {
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                    Ok((tag_slice, advance_past_bang)) => {
                        // Total bytes consumed for the tag token: 1 (`!`) + advance.
                        let tag_token_bytes = 1 + advance_past_bang;
                        let after_tag = &trimmed[tag_token_bytes..];
                        let inline: &'input str = after_tag.trim_start_matches([' ', '\t']);
                        let spaces = after_tag.len() - inline.len();
                        let had_inline = !inline.is_empty();
                        let inline_offset =
                            line_pos.byte_offset + leading + tag_token_bytes + spaces;
                        let inline_char_offset =
                            line_pos.char_offset + leading + tag_token_bytes + spaces;
                        let inline_col = line_pos.column + leading + tag_token_bytes + spaces;
                        // Duplicate tags on the same node are an error.
                        if self.pending_tag.is_some() {
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: bang_pos,
                                message: "a node may not have more than one tag".into(),
                            }));
                        }
                        // Resolve tag handle against directive scope at scan time.
                        let resolved_tag =
                            match self.directive_scope.resolve_tag(tag_slice, bang_pos) {
                                Ok(t) => t,
                                Err(e) => {
                                    self.failed = true;
                                    return StepResult::Yield(Err(e));
                                }
                            };
                        self.pending_tag = Some(resolved_tag);
                        self.lexer.consume_line();
                        if had_inline {
                            self.pending_tag_for_collection = false;
                            let inline_pos = Pos {
                                byte_offset: inline_offset,
                                char_offset: inline_char_offset,
                                line: line_pos.line,
                                column: inline_col,
                            };
                            let synthetic = crate::lines::Line {
                                content: inline,
                                offset: inline_offset,
                                indent: inline_col,
                                break_type: line_break_type,
                                pos: inline_pos,
                            };
                            self.lexer.prepend_inline_line(synthetic);
                        } else {
                            // Standalone tag line — applies to whatever node comes next.
                            self.pending_tag_for_collection = true;
                        }
                        return StepResult::Continue;
                    }
                }
            }
        }

        // ---- Anchor: `&name` — attach to the next node ----

        if let Some(peek) = self.lexer.peek_next_line() {
            let content: &'input str = peek.content;
            let line_pos = peek.pos;
            let line_break_type = peek.break_type;
            let trimmed = content.trim_start_matches(' ');
            if let Some(after_amp) = trimmed.strip_prefix('&') {
                // We only look for `&` at the start of the trimmed line.
                // Tags (`!`) before `&` are handled in Task 17.
                //
                // IMPORTANT for Task 17: when implementing tag-skip, the skip
                // logic must consume the *full* tag token (all `ns-anchor-char`
                // bytes after `!`), not just the `!` character alone.  The `!`
                // character is itself a valid `ns-anchor-char`, so skipping
                // only `!` and then re-entering anchor detection would silently
                // include the tag body in the anchor name.  Example: `!tag &a`
                // — skip must advance past `tag` before looking for `&a`.
                let leading = content.len() - trimmed.len();
                let amp_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
                    char_offset: line_pos.char_offset + leading,
                    line: line_pos.line,
                    column: line_pos.column + leading,
                };
                match scan_anchor_name(after_amp, amp_pos) {
                    Err(e) => {
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                    Ok(name) => {
                        // Determine what follows the anchor name on this line,
                        // before consuming the line (borrow ends here).
                        let after_name = &after_amp[name.len()..];
                        let inline: &'input str = after_name.trim_start_matches([' ', '\t']);
                        let spaces = after_name.len() - inline.len();
                        let had_inline = !inline.is_empty();
                        let inline_offset =
                            line_pos.byte_offset + leading + 1 + name.len() + spaces;
                        let inline_char_offset =
                            line_pos.char_offset + leading + 1 + name.len() + spaces;
                        let inline_col = line_pos.column + leading + 1 + name.len() + spaces;
                        // Duplicate anchors allowed — overwrite.
                        self.pending_anchor = Some(name);
                        self.lexer.consume_line();
                        if had_inline {
                            // Inline content after anchor — anchor applies to the
                            // inline node (scalar or key), not to any enclosing
                            // collection opened on this same line.
                            self.pending_anchor_for_collection = false;
                            let inline_pos = Pos {
                                byte_offset: inline_offset,
                                char_offset: inline_char_offset,
                                line: line_pos.line,
                                column: inline_col,
                            };
                            let synthetic = crate::lines::Line {
                                content: inline,
                                offset: inline_offset,
                                indent: inline_col,
                                break_type: line_break_type,
                                pos: inline_pos,
                            };
                            self.lexer.prepend_inline_line(synthetic);
                        } else {
                            // Standalone anchor line — anchor applies to whatever
                            // node comes next (collection or scalar).
                            self.pending_anchor_for_collection = true;
                        }
                        // Let the next iteration handle whatever follows.
                        return StepResult::Continue;
                    }
                }
            }
        }

        // ---- Flow collection detection: `[` or `{` starts a flow collection ----

        if let Some(line) = self.lexer.peek_next_line() {
            let trimmed = line.content.trim_start_matches(' ');
            if trimmed.starts_with('[') || trimmed.starts_with('{') {
                return self.handle_flow_collection();
            }
        }

        // ---- Block sequence / mapping entry detection ----

        if let Some((dash_indent, dash_pos)) = self.peek_sequence_entry() {
            return self.handle_sequence_entry(dash_indent, dash_pos);
        }
        if let Some((key_indent, key_pos)) = self.peek_mapping_entry() {
            return self.handle_mapping_entry(key_indent, key_pos);
        }

        // ---- Dedent: close collections more deeply nested than the current line ----

        if let Some(line) = self.lexer.peek_next_line() {
            let line_indent = line.indent;
            let close_pos = self.lexer.current_pos();
            self.close_collections_at_or_above(line_indent.saturating_add(1), close_pos);
            if !self.queue.is_empty() {
                return StepResult::Continue;
            }
        }

        // ---- Scalars ----

        // `plain_parent_indent` — the indent at which the current scalar starts;
        // used to stop plain-scalar continuation at a lesser-indented line.
        //
        // `block_parent_indent` — the indent of the enclosing block context;
        // block scalars (`|`, `>`) must have content lines more indented than
        // this value.  For a block scalar embedded as inline content after `? `
        // or `- `, the enclosing block's indent is the *collection's* indent,
        // not the column of the inline `|`/`>` token.
        let plain_parent_indent = self.lexer.peek_next_line().map_or(0, |l| l.indent);
        let block_parent_indent = self.coll_stack.last().map_or(0, |e| e.indent());
        match self.try_consume_scalar(plain_parent_indent, block_parent_indent) {
            Ok(Some(event)) => {
                self.tick_mapping_phase_after_scalar();
                // Drain any trailing comment detected on the scalar's line.
                self.drain_trailing_comment();
                return StepResult::Yield(Ok(event));
            }
            Err(e) => {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
            Ok(None) => {}
        }

        // Fallback: unrecognised content line — consume and loop.
        self.lexer.consume_line();
        StepResult::Continue
    }

    /// Handle a block-sequence dash entry (`-`).
    fn handle_sequence_entry(&mut self, dash_indent: usize, dash_pos: Pos) -> StepResult<'input> {
        let cur_pos = self.lexer.current_pos();
        self.close_collections_at_or_above(dash_indent.saturating_add(1), cur_pos);
        if !self.queue.is_empty() {
            return StepResult::Continue;
        }
        // YAML §8.2.1 seq-spaces rule: a block sequence used as a mapping
        // value in `block-out` context may start at the same column as its
        // parent key (seq-spaces(n, block-out) = n, not n+1).  We therefore
        // open a new sequence when:
        //   - the stack is empty, OR
        //   - dash_indent is greater than the current top's indent (normal
        //     case: sequence is nested deeper than its parent), OR
        //   - the top is a Mapping in Value phase at the same indent (the
        //     seq-spaces case: the sequence is the value of the current key).
        let opens_new = match self.coll_stack.last() {
            None => true,
            Some(
                &(CollectionEntry::Sequence(col)
                | CollectionEntry::Mapping(col, MappingPhase::Key)),
            ) => dash_indent > col,
            Some(&CollectionEntry::Mapping(col, MappingPhase::Value)) => dash_indent >= col,
        };
        if opens_new {
            if self.collection_depth() >= MAX_COLLECTION_DEPTH {
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos: dash_pos,
                    message: "collection nesting depth exceeds limit".into(),
                }));
            }
            self.coll_stack.push(CollectionEntry::Sequence(dash_indent));
            self.queue.push_back((
                Event::SequenceStart {
                    anchor: self.pending_anchor.take(),
                    tag: self.pending_tag.take(),
                    style: CollectionStyle::Block,
                },
                zero_span(dash_pos),
            ));
        }
        let had_inline = self.consume_sequence_dash(dash_indent);
        if !had_inline {
            // Only emit an empty scalar for a bare `-` when there is no
            // following indented content that could be the item's value.
            // If the next line is at an indent strictly greater than
            // `dash_indent`, it belongs to this sequence item — let the
            // main loop handle it.  Otherwise the item is truly empty.
            let next_indent = self.lexer.peek_next_line().map_or(0, |l| l.indent);
            if next_indent <= dash_indent {
                let item_pos = self.lexer.current_pos();
                self.queue.push_back((
                    Event::Scalar {
                        value: std::borrow::Cow::Borrowed(""),
                        style: ScalarStyle::Plain,
                        anchor: self.pending_anchor.take(),
                        tag: None,
                    },
                    zero_span(item_pos),
                ));
            }
        }
        StepResult::Continue
    }

    /// Handle a block-mapping key entry.
    #[allow(clippy::too_many_lines)]
    fn handle_mapping_entry(&mut self, key_indent: usize, key_pos: Pos) -> StepResult<'input> {
        let cur_pos = self.lexer.current_pos();
        self.close_collections_at_or_above(key_indent.saturating_add(1), cur_pos);
        if !self.queue.is_empty() {
            return StepResult::Continue;
        }

        // YAML §8.2.1 seq-spaces close: a block sequence opened as a mapping
        // value in `block-out` context may reside at the *same* column as its
        // parent key (seq-spaces(n, block-out) = n).  When a new mapping key
        // appears at column `n`, such a same-indent sequence must be closed —
        // the standard `close_collections_at_or_above(n+1)` above does not
        // reach it because its indent is exactly `n`, not `>= n+1`.
        //
        // Close the sequence only when the collection immediately beneath it
        // (the next item down the stack) is a Mapping at the same indent in
        // Value phase — that confirms it was opened by the seq-spaces rule,
        // not as an independent sequence at column 0.
        if let Some(&CollectionEntry::Sequence(seq_col)) = self.coll_stack.last() {
            if seq_col == key_indent {
                let parent_is_seq_spaces_mapping = self.coll_stack.iter().rev().nth(1).is_some_and(
                    |e| matches!(e, CollectionEntry::Mapping(col, _) if *col == key_indent),
                );
                if parent_is_seq_spaces_mapping {
                    self.coll_stack.pop();
                    self.queue
                        .push_back((Event::SequenceEnd, zero_span(cur_pos)));
                    // Advance parent mapping from Value to Key phase — the
                    // sequence was its value and is now fully closed.
                    if let Some(CollectionEntry::Mapping(_, phase)) = self.coll_stack.last_mut() {
                        *phase = MappingPhase::Key;
                    }
                    return StepResult::Continue;
                }
            }
        }

        let is_in_mapping_at_this_indent = self.coll_stack.last().is_some_and(
            |top| matches!(top, CollectionEntry::Mapping(col, _) if *col == key_indent),
        );

        if !is_in_mapping_at_this_indent {
            if self.collection_depth() >= MAX_COLLECTION_DEPTH {
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos: key_pos,
                    message: "collection nesting depth exceeds limit".into(),
                }));
            }
            self.coll_stack
                .push(CollectionEntry::Mapping(key_indent, MappingPhase::Key));
            // Consume pending anchor for the mapping only when the anchor was
            // on its own line (standalone). Inline anchors (e.g. `&a key: v`)
            // annotate the key scalar and must not be consumed here.
            let mapping_anchor = if self.pending_anchor_for_collection {
                self.pending_anchor.take()
            } else {
                None
            };
            let mapping_tag = if self.pending_tag_for_collection {
                self.pending_tag.take()
            } else {
                None
            };
            self.queue.push_back((
                Event::MappingStart {
                    anchor: mapping_anchor,
                    tag: mapping_tag,
                    style: CollectionStyle::Block,
                },
                zero_span(key_pos),
            ));
            return StepResult::Continue;
        }

        // Continuing an existing mapping.
        if self.is_value_indicator_line() {
            self.consume_explicit_value_line(key_indent);
            return StepResult::Continue;
        }

        // If the mapping is in Value phase and the next line is another key
        // (not a `: value` line), the previous key had no value — emit empty.
        if self.coll_stack.last().is_some_and(|top| {
            matches!(top, CollectionEntry::Mapping(col, MappingPhase::Value) if *col == key_indent)
        }) {
            let pos = self.lexer.current_pos();
            self.queue.push_back((
                Event::Scalar {
                    value: std::borrow::Cow::Borrowed(""),
                    style: ScalarStyle::Plain,
                    anchor: self.pending_anchor.take(),
                    tag: None,
                },
                zero_span(pos),
            ));
            self.advance_mapping_to_key();
            return StepResult::Continue;
        }

        // Normal key line: consume and emit key scalar.
        let consumed = self.consume_mapping_entry(key_indent);
        match consumed {
            ConsumedMapping::ExplicitKey { had_key_inline } => {
                if !had_key_inline {
                    let pos = self.lexer.current_pos();
                    self.queue.push_back((
                        Event::Scalar {
                            value: std::borrow::Cow::Borrowed(""),
                            style: ScalarStyle::Plain,
                            anchor: self.pending_anchor.take(),
                            tag: self.pending_tag.take(),
                        },
                        zero_span(pos),
                    ));
                    self.advance_mapping_to_value();
                }
            }
            ConsumedMapping::ImplicitKey {
                key_value,
                key_span,
            } => {
                self.queue.push_back((
                    Event::Scalar {
                        value: std::borrow::Cow::Borrowed(key_value),
                        style: ScalarStyle::Plain,
                        anchor: self.pending_anchor.take(),
                        tag: self.pending_tag.take(),
                    },
                    key_span,
                ));
                self.advance_mapping_to_value();
            }
        }
        StepResult::Continue
    }

    /// True when the next line is a bare value indicator (`: ` or `:`
    /// followed by space/EOL), used for the explicit-key form.
    fn is_value_indicator_line(&self) -> bool {
        let Some(line) = self.lexer.peek_next_line() else {
            return false;
        };
        let trimmed = line.content.trim_start_matches(' ');
        if !trimmed.starts_with(':') {
            return false;
        }
        let after_colon = &trimmed[1..];
        after_colon.is_empty()
            || after_colon.starts_with(' ')
            || after_colon.starts_with('\t')
            || after_colon.starts_with('\n')
            || after_colon.starts_with('\r')
    }

    /// Consume a `: value` line (explicit value indicator).
    ///
    /// If there is inline content after `: `, prepend a synthetic line for it
    /// so the next iteration emits it as the value scalar.
    fn consume_explicit_value_line(&mut self, key_indent: usize) {
        // SAFETY: caller checked is_value_indicator_line() — the line exists.
        let Some(line) = self.lexer.peek_next_line() else {
            unreachable!("consume_explicit_value_line called without a pending line")
        };

        // Extract all data from the borrowed line before any mutable lexer calls.
        let content: &'input str = line.content;
        let line_pos = line.pos;
        let line_break_type = line.break_type;

        let leading_spaces = content.len() - content.trim_start_matches(' ').len();
        let trimmed = &content[leading_spaces..];

        // Advance past `:` and any whitespace.
        let after_colon = &trimmed[1..]; // skip ':'
        let value_content = after_colon.trim_start_matches([' ', '\t']);
        let had_value_inline = !value_content.is_empty();

        if had_value_inline {
            let spaces_after_colon = after_colon.len() - value_content.len();
            let total_offset = leading_spaces + 1 + spaces_after_colon;
            let value_col = key_indent + 1 + spaces_after_colon;
            let value_pos = Pos {
                byte_offset: line_pos.byte_offset + total_offset,
                char_offset: line_pos.char_offset + total_offset,
                line: line_pos.line,
                column: line_pos.column + total_offset,
            };
            let synthetic = Line {
                content: value_content,
                offset: value_pos.byte_offset,
                indent: value_col,
                break_type: line_break_type,
                pos: value_pos,
            };
            self.lexer.consume_line();
            self.lexer.prepend_inline_line(synthetic);
        } else {
            // Bare `:` with no value content — the value is empty.
            self.lexer.consume_line();
            let pos = self.lexer.current_pos();
            self.queue.push_back((
                Event::Scalar {
                    value: std::borrow::Cow::Borrowed(""),
                    style: ScalarStyle::Plain,
                    anchor: self.pending_anchor.take(),
                    tag: None,
                },
                zero_span(pos),
            ));
            self.advance_mapping_to_key();
        }
    }

    /// Handle a flow collection (`[...]` or `{...}`) starting on the current line.
    ///
    /// This method reads the complete flow collection — potentially spanning
    /// multiple physical lines — and pushes all events (SequenceStart/End,
    /// MappingStart/End, Scalar) to `self.queue`.  It returns when the
    /// outermost closing delimiter (`]` or `}`) is consumed.
    ///
    /// ## Security invariants
    ///
    /// - **No recursion:** the parser uses an explicit `Vec<FlowFrame>` stack
    ///   rather than recursive function calls, preventing stack overflow on
    ///   deeply nested input.
    /// - **Unified depth limit:** each new nested collection checks against
    ///   `MAX_COLLECTION_DEPTH` using the same `coll_stack.len()` counter as
    ///   block collections, so flow and block nesting depths are additive.
    /// - **Incremental parsing:** content is processed line-by-line; no
    ///   `String` buffer holds the entire flow body.
    /// - **Unterminated collection:** reaching EOF without the matching closing
    ///   delimiter returns `Err`.
    #[allow(clippy::too_many_lines)]
    fn handle_flow_collection(&mut self) -> StepResult<'input> {
        use crate::lexer::scan_plain_line_flow;
        use std::borrow::Cow;

        // -----------------------------------------------------------------------
        // Local types for the explicit flow-parser stack.
        // -----------------------------------------------------------------------

        /// One frame on the explicit flow-parser stack.
        #[derive(Clone, Copy)]
        enum FlowFrame {
            /// An open `[...]` sequence.
            ///
            /// `has_value` is `false` immediately after opening and immediately
            /// after each comma; it becomes `true` when a scalar or nested
            /// collection is emitted.  A comma arriving when `has_value` is
            /// `false` is a leading comma error.
            Sequence { has_value: bool },
            /// An open `{...}` mapping.
            ///
            /// `has_value` tracks the same invariant as in `Sequence` but for
            /// the mapping as a whole (not per key/value pair).
            Mapping {
                phase: FlowMappingPhase,
                has_value: bool,
            },
        }

        // Design note — phase-advance pattern
        //
        // Four sites below repeat the same `if let Some(frame) = flow_stack.last_mut()
        // { match frame { Sequence { has_value } => ... Mapping { phase, has_value } =>
        // ... } }` shape.  Extracting a helper function would require moving `FlowFrame`
        // and `FlowMappingPhase` to module scope — adding module-level types whose sole
        // purpose is to enable this refactor adds more complexity than the duplication
        // costs.  Each site is 6–8 lines and clearly labelled by its comment; the
        // repetition is intentional and stable.

        // -----------------------------------------------------------------------
        // Buffer-management invariant
        // -----------------------------------------------------------------------
        //
        // The line buffer always holds the current line un-consumed.  We peek to
        // read content and only consume the line when we need to advance past it
        // (end-of-line or quoted-scalar delegation).
        //
        // `cur_content` / `cur_base_pos` always mirror what `peek_next_line()`
        // returns.  After any call that changes the buffer (consume_line /
        // prepend_inline_line), we immediately re-sync via peek.
        //
        // Helper: advance `pos` over `content[..byte_len]`, one char at a time.

        let abs_pos = |base: Pos, content: &str, i: usize| -> Pos {
            let mut p = base;
            for ch in content[..i].chars() {
                p = p.advance(ch);
            }
            p
        };

        // -----------------------------------------------------------------------
        // Initialise: read the current line, locate the opening delimiter.
        // -----------------------------------------------------------------------

        // SAFETY: caller verified via peek in step_in_document.
        let Some(first_line) = self.lexer.peek_next_line() else {
            unreachable!("handle_flow_collection called without a pending line")
        };

        let leading = first_line.content.len() - first_line.content.trim_start_matches(' ').len();

        // Stack for tracking open flow collections (nested via explicit iteration,
        // not recursion — security requirement).
        let mut flow_stack: Vec<FlowFrame> = Vec::new();
        // All events assembled during this call (pushed to self.queue at end).
        let mut events: Vec<(Event<'input>, Span)> = Vec::new();
        // Current byte offset within `cur_content`.
        let mut pos_in_line: usize = leading;
        // Pending anchor for the next node in this flow collection.
        // Seeded from any block-context anchor that was pending when this flow
        // collection was entered (e.g. `&seq [a, b]` sets pending_anchor before
        // the `[` is dispatched to handle_flow_collection).
        let mut pending_flow_anchor: Option<&'input str> = self.pending_anchor.take();
        // Pending tag for the next node in this flow collection.
        // Seeded from any block-context tag that was pending when this flow
        // collection was entered (e.g. `!!seq [a, b]` sets pending_tag before
        // the `[` is dispatched to handle_flow_collection).
        let mut pending_flow_tag: Option<std::borrow::Cow<'input, str>> = self.pending_tag.take();

        // Re-sync `cur_content` / `cur_base_pos` from the buffer.
        // Returns false when the buffer is empty (EOF mid-flow).
        // INVARIANT: called every time after consuming or prepending a line.
        macro_rules! resync {
            () => {{
                match self.lexer.peek_next_line() {
                    Some(l) => {
                        // Safe: we re-assign these immediately without holding
                        // a borrow on `self.lexer` at the same time.
                        (l.content, l.pos)
                    }
                    None => {
                        // EOF
                        ("", self.lexer.current_pos())
                    }
                }
            }};
        }

        let (mut cur_content, mut cur_base_pos) = resync!();

        // -----------------------------------------------------------------------
        // Main parse loop — iterates over characters in the current (and
        // subsequent) lines until the outermost closing delimiter is found.
        // -----------------------------------------------------------------------

        'outer: loop {
            // Skip leading spaces/tabs and comments.
            while pos_in_line < cur_content.len() {
                let Some(ch) = cur_content[pos_in_line..].chars().next() else {
                    break;
                };
                if ch == ' ' || ch == '\t' {
                    pos_in_line += 1;
                } else if ch == '#' {
                    // Emit a Comment event for this `# comment` to end of line.
                    // No MAX_COMMENT_LEN check here — this comment is bounded by the
                    // physical line length (itself bounded by total input size), the
                    // same reason drain_trailing_comment does not apply the limit.
                    let hash_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    // Comment text: everything after `#` (byte at pos_in_line is `#`,
                    // ASCII 1 byte, so text starts at pos_in_line + 1).
                    let text_start = pos_in_line + 1;
                    // SAFETY: text_start <= cur_content.len() because we found
                    // `#` at pos_in_line which is < cur_content.len().
                    let comment_text: &'input str = cur_content.get(text_start..).unwrap_or("");
                    let mut comment_end = hash_pos.advance('#');
                    for c in comment_text.chars() {
                        comment_end = comment_end.advance(c);
                    }
                    let comment_span = Span {
                        start: hash_pos,
                        end: comment_end,
                    };
                    events.push((Event::Comment { text: comment_text }, comment_span));
                    pos_in_line = cur_content.len();
                } else {
                    break;
                }
            }

            // ----------------------------------------------------------------
            // End of line — consume and advance.
            // ----------------------------------------------------------------
            if pos_in_line >= cur_content.len() {
                self.lexer.consume_line();

                if flow_stack.is_empty() {
                    // Outermost collection closed; done.
                    break 'outer;
                }

                (cur_content, cur_base_pos) = resync!();
                if cur_content.is_empty() && self.lexer.at_eof() {
                    let err_pos = self.lexer.current_pos();
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "unterminated flow collection: unexpected end of input".into(),
                    }));
                }
                pos_in_line = 0;
                continue 'outer;
            }

            let Some(ch) = cur_content[pos_in_line..].chars().next() else {
                continue 'outer;
            };

            // ----------------------------------------------------------------
            // Opening delimiters `[` and `{`
            // ----------------------------------------------------------------
            if ch == '[' || ch == '{' {
                // Check unified depth limit (flow + block combined).
                let total_depth = self.coll_stack.len() + flow_stack.len();
                if total_depth >= MAX_COLLECTION_DEPTH {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "collection nesting depth exceeds limit".into(),
                    }));
                }

                let open_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                let open_span = zero_span(open_pos);
                pos_in_line += 1;

                if ch == '[' {
                    flow_stack.push(FlowFrame::Sequence { has_value: false });
                    events.push((
                        Event::SequenceStart {
                            anchor: pending_flow_anchor.take(),
                            tag: pending_flow_tag.take(),
                            style: CollectionStyle::Flow,
                        },
                        open_span,
                    ));
                } else {
                    flow_stack.push(FlowFrame::Mapping {
                        phase: FlowMappingPhase::Key,
                        has_value: false,
                    });
                    events.push((
                        Event::MappingStart {
                            anchor: pending_flow_anchor.take(),
                            tag: pending_flow_tag.take(),
                            style: CollectionStyle::Flow,
                        },
                        open_span,
                    ));
                }
                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Closing delimiters `]` and `}`
            // ----------------------------------------------------------------
            if ch == ']' || ch == '}' {
                let close_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                let close_span = zero_span(close_pos);
                pos_in_line += 1;

                let Some(top) = flow_stack.pop() else {
                    // Closing delimiter with empty stack — mismatched.
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: close_pos,
                        message: format!("unexpected '{ch}' in flow context"),
                    }));
                };

                match (ch, top) {
                    (']', FlowFrame::Sequence { .. }) => {
                        events.push((Event::SequenceEnd, close_span));
                    }
                    ('}', FlowFrame::Mapping { phase, .. }) => {
                        // If mapping is in Value phase (key emitted, no value yet),
                        // emit empty value before closing.
                        if phase == FlowMappingPhase::Value {
                            events.push((empty_scalar_event(), close_span));
                        }
                        events.push((Event::MappingEnd, close_span));
                    }
                    (']', FlowFrame::Mapping { .. }) => {
                        self.failed = true;
                        return StepResult::Yield(Err(Error {
                            pos: close_pos,
                            message: "expected '}' to close flow mapping, found ']'".into(),
                        }));
                    }
                    ('}', FlowFrame::Sequence { .. }) => {
                        self.failed = true;
                        return StepResult::Yield(Err(Error {
                            pos: close_pos,
                            message: "expected ']' to close flow sequence, found '}'".into(),
                        }));
                    }
                    _ => unreachable!("all (ch, top) combinations covered above"),
                }

                // After a nested collection closes inside a parent frame,
                // mark the parent as having a value (the nested collection was it),
                // and if it's a mapping in Value phase, advance to Key phase.
                if let Some(parent) = flow_stack.last_mut() {
                    match parent {
                        FlowFrame::Sequence { has_value } => {
                            *has_value = true;
                        }
                        FlowFrame::Mapping { phase, has_value } => {
                            *has_value = true;
                            if *phase == FlowMappingPhase::Value {
                                *phase = FlowMappingPhase::Key;
                            }
                        }
                    }
                }

                if flow_stack.is_empty() {
                    // Outermost collection closed.
                    // Consume the current line; prepend any non-empty tail so the
                    // block state machine can process content after the `]`/`}`.
                    let tail_content = &cur_content[pos_in_line..];
                    self.lexer.consume_line();
                    if !tail_content.trim_start_matches([' ', '\t']).is_empty() {
                        let tail_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        let synthetic = crate::lines::Line {
                            content: tail_content,
                            offset: tail_pos.byte_offset,
                            indent: tail_pos.column,
                            break_type: crate::lines::BreakType::Eof,
                            pos: tail_pos,
                        };
                        self.lexer.prepend_inline_line(synthetic);
                    }
                    break 'outer;
                }
                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Comma separator
            // ----------------------------------------------------------------
            if ch == ',' {
                // Leading-comma check: if the current frame has not yet produced
                // any value since it was opened (or since the last comma), this
                // comma is invalid — e.g. `[,]` or `{,}`.
                let leading = match flow_stack.last() {
                    Some(
                        FlowFrame::Sequence { has_value } | FlowFrame::Mapping { has_value, .. },
                    ) => !has_value,
                    None => false,
                };
                if leading {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "invalid leading comma in flow collection".into(),
                    }));
                }

                pos_in_line += 1;

                // Skip whitespace after comma.
                while pos_in_line < cur_content.len() {
                    match cur_content[pos_in_line..].chars().next() {
                        Some(c) if c == ' ' || c == '\t' => pos_in_line += 1,
                        _ => break,
                    }
                }

                // Double-comma check: next char must not be another comma.
                if cur_content[pos_in_line..].starts_with(',') {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "invalid empty entry: consecutive commas in flow collection"
                            .into(),
                    }));
                }

                // Reset has_value and (for mappings) go back to Key phase.
                if let Some(frame) = flow_stack.last_mut() {
                    match frame {
                        FlowFrame::Sequence { has_value } => {
                            *has_value = false;
                        }
                        FlowFrame::Mapping { phase, has_value } => {
                            *has_value = false;
                            if *phase == FlowMappingPhase::Value {
                                *phase = FlowMappingPhase::Key;
                            }
                        }
                    }
                }

                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Block scalar indicators forbidden in flow context
            // ----------------------------------------------------------------
            if ch == '|' || ch == '>' {
                let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: format!(
                        "block scalar indicator '{ch}' is not allowed inside a flow collection"
                    ),
                }));
            }

            // ----------------------------------------------------------------
            // Block sequence entry indicator `-` forbidden in flow context.
            //
            // Per YAML 1.2 §7.4, block collections cannot appear inside flow
            // context.  A `-` followed by space, tab, or end-of-content is
            // the block-sequence entry indicator; a `-` followed by any other
            // non-separator character is a valid plain-scalar start (e.g. `-x`
            // or `-1` are legal plain scalars in flow context).
            // ----------------------------------------------------------------
            if ch == '-' {
                let after = &cur_content[pos_in_line + 1..];
                let next_c = after.chars().next();
                if next_c.is_none_or(|c| matches!(c, ' ' | '\t')) {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "block sequence entry '-' is not allowed inside a flow collection"
                            .into(),
                    }));
                }
            }

            // ----------------------------------------------------------------
            // Quoted scalars — delegate to existing lexer methods.
            //
            // Strategy: consume the current line, prepend a synthetic line
            // starting exactly at the quote character, call the method, then
            // re-sync `cur_content` / `cur_base_pos` from the buffer.
            // ----------------------------------------------------------------
            if ch == '\'' || ch == '"' {
                // `remaining` borrows from `cur_content` which borrows from `'input`.
                // We capture it before touching the lexer buffer.
                let remaining: &'input str = &cur_content[pos_in_line..];
                let cur_abs_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);

                // Consume the current line from the buffer and replace it with
                // a synthetic line that starts at the quote character.  The
                // quoted-scalar method will consume this synthetic line entirely,
                // including any content after the closing quote — so we must
                // reconstruct the tail from `remaining` and `span` below.
                self.lexer.consume_line();
                let synthetic = crate::lines::Line {
                    content: remaining,
                    offset: cur_abs_pos.byte_offset,
                    indent: cur_abs_pos.column,
                    break_type: crate::lines::BreakType::Eof,
                    pos: cur_abs_pos,
                };
                self.lexer.prepend_inline_line(synthetic);

                // Call the appropriate quoted-scalar method.
                let result = if ch == '\'' {
                    self.lexer.try_consume_single_quoted(0)
                } else {
                    self.lexer.try_consume_double_quoted(0)
                };

                let (value, span) = match result {
                    Ok(Some(vs)) => vs,
                    Ok(None) => {
                        self.failed = true;
                        return StepResult::Yield(Err(Error {
                            pos: cur_abs_pos,
                            message: "expected quoted scalar".into(),
                        }));
                    }
                    Err(e) => {
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                };

                let style = if ch == '\'' {
                    ScalarStyle::SingleQuoted
                } else {
                    ScalarStyle::DoubleQuoted
                };
                events.push((
                    Event::Scalar {
                        value,
                        style,
                        anchor: pending_flow_anchor.take(),
                        tag: pending_flow_tag.take(),
                    },
                    span,
                ));

                // The quoted-scalar method consumed its synthetic line entirely.
                // Any content after the closing quote is in `remaining` starting
                // at byte offset `span.end.byte_offset - cur_abs_pos.byte_offset`.
                // Prepend a synthetic line for that tail so the flow parser
                // continues processing `,`, `]`, `}`, etc. after the scalar.
                let consumed_bytes = span.end.byte_offset - cur_abs_pos.byte_offset;
                let tail_in_remaining = &remaining[consumed_bytes..];
                if !tail_in_remaining.is_empty() {
                    let tail_syn = crate::lines::Line {
                        content: tail_in_remaining,
                        offset: span.end.byte_offset,
                        indent: span.end.column,
                        break_type: crate::lines::BreakType::Eof,
                        pos: span.end,
                    };
                    self.lexer.prepend_inline_line(tail_syn);
                }

                // Re-sync from the buffer.
                (cur_content, cur_base_pos) = resync!();
                pos_in_line = 0;

                if cur_content.is_empty() && self.lexer.at_eof() && !flow_stack.is_empty() {
                    let err_pos = self.lexer.current_pos();
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "unterminated flow collection: unexpected end of input".into(),
                    }));
                }

                // Advance mapping phase for the emitted scalar; mark frame as having a value.
                if let Some(frame) = flow_stack.last_mut() {
                    match frame {
                        FlowFrame::Sequence { has_value } => {
                            *has_value = true;
                        }
                        FlowFrame::Mapping { phase, has_value } => {
                            *has_value = true;
                            *phase = match *phase {
                                FlowMappingPhase::Key => FlowMappingPhase::Value,
                                FlowMappingPhase::Value => FlowMappingPhase::Key,
                            };
                        }
                    }
                }

                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Explicit key indicator `?` in flow mappings
            // ----------------------------------------------------------------
            if ch == '?' {
                let next_ch = cur_content[pos_in_line + 1..].chars().next();
                if next_ch.is_none_or(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
                    // `?` followed by whitespace/EOL: explicit key indicator.
                    pos_in_line += 1;
                    continue 'outer;
                }
                // `?` not followed by whitespace — treat as plain scalar start.
            }

            // ----------------------------------------------------------------
            // `:` value separator in flow mappings
            // ----------------------------------------------------------------
            if ch == ':' {
                let next_ch = cur_content[pos_in_line + 1..].chars().next();
                let is_value_sep =
                    next_ch.is_none_or(|c| matches!(c, ' ' | '\t' | ',' | ']' | '}' | '\n' | '\r'));
                if is_value_sep {
                    if let Some(FlowFrame::Mapping { phase, .. }) = flow_stack.last_mut() {
                        if *phase == FlowMappingPhase::Key {
                            *phase = FlowMappingPhase::Value;
                        }
                    }
                    pos_in_line += 1;
                    continue 'outer;
                }
                // `:` not followed by separator — treat as plain scalar char.
            }

            // ----------------------------------------------------------------
            // Tag `!tag`, `!!tag`, `!<uri>`, or `!` in flow context
            // ----------------------------------------------------------------
            if ch == '!' {
                let bang_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                let after_bang = &cur_content[pos_in_line + 1..];
                let tag_start = &cur_content[pos_in_line..];
                match scan_tag(after_bang, tag_start, bang_pos) {
                    Err(e) => {
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                    Ok((tag_slice, advance_past_bang)) => {
                        // Total bytes: 1 (`!`) + advance_past_bang.
                        // `!<URI>`: advance_past_bang = 1 + uri.len() + 1
                        // `!!suffix`: advance_past_bang = 1 + suffix.len()
                        // `!suffix`: advance_past_bang = suffix.len()
                        // `!` alone: advance_past_bang = 0
                        if pending_flow_tag.is_some() {
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: bang_pos,
                                message: "a node may not have more than one tag".into(),
                            }));
                        }
                        // Resolve tag handle against directive scope at scan time.
                        let resolved_flow_tag =
                            match self.directive_scope.resolve_tag(tag_slice, bang_pos) {
                                Ok(t) => t,
                                Err(e) => {
                                    self.failed = true;
                                    return StepResult::Yield(Err(e));
                                }
                            };
                        pending_flow_tag = Some(resolved_flow_tag);
                        pos_in_line += 1 + advance_past_bang;
                        // Skip any whitespace after the tag.
                        while pos_in_line < cur_content.len() {
                            match cur_content[pos_in_line..].chars().next() {
                                Some(c) if c == ' ' || c == '\t' => pos_in_line += 1,
                                _ => break,
                            }
                        }
                        continue 'outer;
                    }
                }
            }

            // ----------------------------------------------------------------
            // Anchor `&name` in flow context
            // ----------------------------------------------------------------
            if ch == '&' {
                let after_amp = &cur_content[pos_in_line + 1..];
                let amp_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                match scan_anchor_name(after_amp, amp_pos) {
                    Err(e) => {
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                    Ok(name) => {
                        // Duplicate anchors allowed — overwrite.
                        pending_flow_anchor = Some(name);
                        pos_in_line += 1 + name.len();
                        // Skip any whitespace after the anchor name.
                        while pos_in_line < cur_content.len() {
                            match cur_content[pos_in_line..].chars().next() {
                                Some(c) if c == ' ' || c == '\t' => pos_in_line += 1,
                                _ => break,
                            }
                        }
                        continue 'outer;
                    }
                }
            }

            // ----------------------------------------------------------------
            // Alias `*name` in flow context
            // ----------------------------------------------------------------
            if ch == '*' {
                let after_star = &cur_content[pos_in_line + 1..];
                let star_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                // YAML 1.2 §7.1: alias nodes cannot have properties (anchor or tag).
                if pending_flow_tag.is_some() {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have a tag property".into(),
                    }));
                }
                match scan_anchor_name(after_star, star_pos) {
                    Err(e) => {
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                    Ok(name) => {
                        let alias_end = Pos {
                            byte_offset: star_pos.byte_offset + 1 + name.len(),
                            char_offset: star_pos.char_offset + 1 + name.chars().count(),
                            line: star_pos.line,
                            column: star_pos.column + 1 + name.chars().count(),
                        };
                        let alias_span = Span {
                            start: star_pos,
                            end: alias_end,
                        };
                        events.push((Event::Alias { name }, alias_span));
                        pos_in_line += 1 + name.len();
                        // Advance mapping phase; mark frame as having a value.
                        if let Some(frame) = flow_stack.last_mut() {
                            match frame {
                                FlowFrame::Sequence { has_value } => {
                                    *has_value = true;
                                }
                                FlowFrame::Mapping { phase, has_value } => {
                                    *has_value = true;
                                    *phase = match *phase {
                                        FlowMappingPhase::Key => FlowMappingPhase::Value,
                                        FlowMappingPhase::Value => FlowMappingPhase::Key,
                                    };
                                }
                            }
                        }
                        continue 'outer;
                    }
                }
            }

            // ----------------------------------------------------------------
            // Plain scalar in flow context
            // ----------------------------------------------------------------
            {
                // Indicator characters that cannot start a plain scalar in flow.
                let is_plain_first = if matches!(
                    ch,
                    ',' | '['
                        | ']'
                        | '{'
                        | '}'
                        | '#'
                        | '&'
                        | '*'
                        | '!'
                        | '|'
                        | '>'
                        | '\''
                        | '"'
                        | '%'
                        | '@'
                        | '`'
                ) {
                    false
                } else if matches!(ch, '?' | ':' | '-') {
                    // These start a plain scalar only if followed by a safe char.
                    let after = &cur_content[pos_in_line + ch.len_utf8()..];
                    let next_c = after.chars().next();
                    next_c.is_some_and(|nc| !matches!(nc, ' ' | '\t' | ',' | '[' | ']' | '{' | '}'))
                } else {
                    true
                };

                if is_plain_first {
                    let slice = &cur_content[pos_in_line..];
                    let scanned = scan_plain_line_flow(slice);
                    if !scanned.is_empty() {
                        let scalar_start = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        let scalar_end =
                            abs_pos(cur_base_pos, cur_content, pos_in_line + scanned.len());
                        let scalar_span = Span {
                            start: scalar_start,
                            end: scalar_end,
                        };

                        events.push((
                            Event::Scalar {
                                value: Cow::Borrowed(scanned),
                                style: ScalarStyle::Plain,
                                anchor: pending_flow_anchor.take(),
                                tag: pending_flow_tag.take(),
                            },
                            scalar_span,
                        ));
                        pos_in_line += scanned.len();

                        // Advance mapping phase; mark frame as having a value.
                        if let Some(frame) = flow_stack.last_mut() {
                            match frame {
                                FlowFrame::Sequence { has_value } => {
                                    *has_value = true;
                                }
                                FlowFrame::Mapping { phase, has_value } => {
                                    *has_value = true;
                                    *phase = match *phase {
                                        FlowMappingPhase::Key => FlowMappingPhase::Value,
                                        FlowMappingPhase::Value => FlowMappingPhase::Key,
                                    };
                                }
                            }
                        }
                        continue 'outer;
                    }
                }

                // Reserved indicators — task 19 will handle directives.
                // `!` (tags), `&`/`*` (anchors/aliases) are handled above.
                // Silently skipping remaining reserved indicators would mangle
                // YAML structure, so we error early here.
                if matches!(ch, '%' | '@' | '`') {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: format!(
                            "indicator '{ch}' inside flow collection is not yet supported"
                        ),
                    }));
                }

                // Any other character that is not a plain-scalar start and is
                // not an indicator handled above (e.g. C0 control characters,
                // DEL, C1 controls, surrogates) is invalid here. Error rather
                // than panicking — this is user-supplied input.
                let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: format!("invalid character {ch:?} inside flow collection"),
                }));
            }
        }

        // Tick the parent block mapping phase (if any) after completing a flow
        // collection that was a key or value in a block mapping.
        self.tick_mapping_phase_after_scalar();

        // Push all accumulated events to the queue.
        self.queue.extend(events);
        StepResult::Continue
    }

    /// Tick the key/value phase of the innermost open mapping after emitting a
    /// scalar event.
    ///
    /// - If the mapping was in `Key` phase, it flips to `Value`.
    /// - If the mapping was in `Value` phase (or there is no open mapping), it
    ///   flips back to `Key`.
    fn tick_mapping_phase_after_scalar(&mut self) {
        // Find the innermost mapping entry on the stack.
        for entry in self.coll_stack.iter_mut().rev() {
            if let CollectionEntry::Mapping(_, phase) = entry {
                *phase = match *phase {
                    MappingPhase::Key => MappingPhase::Value,
                    MappingPhase::Value => MappingPhase::Key,
                };
                return;
            }
            // Sequences between this mapping and the top don't count.
            if matches!(entry, CollectionEntry::Sequence(_)) {
                // A scalar here is an item in a sequence, not a mapping value.
                return;
            }
        }
    }
}

impl<'input> Iterator for EventIter<'input> {
    type Item = Result<(Event<'input>, Span), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // After an error, stop immediately — prevent infinite loops on the
        // same problematic input (e.g. depth-limit on a prepended synthetic line).
        if self.failed {
            return None;
        }

        // Iterative dispatch — avoids unbounded recursion on large bare docs.
        loop {
            // Drain the event queue first.
            if let Some(event) = self.queue.pop_front() {
                return Some(Ok(event));
            }

            let step = match self.state {
                IterState::BeforeStream => {
                    self.state = IterState::BetweenDocs;
                    return Some(Ok((Event::StreamStart, zero_span(Pos::ORIGIN))));
                }
                IterState::BetweenDocs => self.step_between_docs(),
                IterState::InDocument => self.step_in_document(),
                IterState::Done => return None,
            };

            match step {
                StepResult::Continue => {}
                StepResult::Yield(result) => return Some(result),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests for private helpers (Gap 2: peek/consume divergence guard)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{find_value_indicator_offset, is_implicit_mapping_line};

    /// Every line that `is_implicit_mapping_line` accepts must also produce
    /// `Some` from `find_value_indicator_offset`.  This is the contract
    /// enforced by the `unreachable!` at the `consume_mapping_entry` call site —
    /// if the two ever diverge a future change will trigger a runtime panic
    /// under `#[deny(clippy::panic)]`.
    ///
    /// The table covers: trailing colon, colon-space, colon-tab, colon in
    /// quoted spans (must be accepted by peek but offset still returned),
    /// multi-byte characters before the colon, and lines that should not
    /// be accepted.
    #[test]
    fn find_value_indicator_agrees_with_is_implicit_mapping_line() {
        let accepted = [
            "key:",
            "key: value",
            "key:\t",
            "key:  multiple spaces",
            "\"quoted key\": val",
            "'single quoted': val",
            "key with spaces: val",
            "k:",
            "longer-key-with-dashes: v",
            "unicode_\u{00e9}: v",
        ];
        for line in accepted {
            assert!(
                is_implicit_mapping_line(line),
                "expected is_implicit_mapping_line to accept: {line:?}"
            );
            assert!(
                find_value_indicator_offset(line).is_some(),
                "find_value_indicator_offset must return Some for accepted line: {line:?}"
            );
        }

        let rejected = [
            "plain scalar",
            "http://example.com",
            "no colon here",
            "# comment: not a key",
            "",
        ];
        for line in rejected {
            assert!(
                !is_implicit_mapping_line(line),
                "expected is_implicit_mapping_line to reject: {line:?}"
            );
            assert!(
                find_value_indicator_offset(line).is_none(),
                "find_value_indicator_offset must return None for rejected line: {line:?}"
            );
        }
    }
}
