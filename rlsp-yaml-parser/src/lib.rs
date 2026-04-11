// SPDX-License-Identifier: MIT
#![deny(clippy::panic)]

mod chars;
mod directive_scope;
pub mod encoding;
mod error;
mod event;
mod lexer;
pub mod limits;
mod lines;
pub mod loader;
mod mapping;
pub mod node;
mod pos;
mod properties;
mod state;

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

use directive_scope::DirectiveScope;
use state::{
    CollectionEntry, ConsumedMapping, FlowMappingPhase, IterState, MappingPhase, PendingAnchor,
    StepResult,
};

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
#[allow(clippy::struct_excessive_bools)]
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
    pending_tag: Option<std::borrow::Cow<'input, str>>,
    /// True when `pending_tag` was set from a standalone tag line (no inline
    /// content after the tag).  False when set inline.
    pending_tag_for_collection: bool,
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
    /// When a tag or anchor appears inline on a physical line (e.g. `!!str &a key:`),
    /// the key content is prepended as a synthetic line with the key's column as its
    /// indent.  This field records the indent of the ORIGINAL physical line so that
    /// `handle_mapping_entry` can open the mapping at the correct (original) indent
    /// rather than the synthetic line's offset.
    property_origin_indent: Option<usize>,
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
            pending_tag: None,
            pending_tag_for_collection: false,
            directive_scope: DirectiveScope::default(),
            root_node_emitted: false,
            explicit_key_pending: false,
            property_origin_indent: None,
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
                    CollectionEntry::Sequence(_, _) => Event::SequenceEnd,
                    CollectionEntry::Mapping(_, _, _) => Event::MappingEnd,
                };
                self.queue.push_back((ev, zero_span(pos)));
                // After closing a collection, the parent mapping (if any)
                // transitions from Value phase to Key phase.  The parent
                // sequence (if any) marks its current item as completed.
                match self.coll_stack.last_mut() {
                    Some(CollectionEntry::Mapping(_, phase, _)) => {
                        if *phase == MappingPhase::Value {
                            *phase = MappingPhase::Key;
                        }
                    }
                    Some(CollectionEntry::Sequence(_, has_had_item)) => {
                        *has_had_item = true;
                    }
                    None => {}
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
                CollectionEntry::Sequence(_, _) => Event::SequenceEnd,
                CollectionEntry::Mapping(_, MappingPhase::Value, _) => {
                    // Mapping closed while waiting for a value — emit empty value.
                    // Consume any pending anchor so `&anchor\n` at end of doc
                    // is properly attached to the empty value.
                    self.queue.push_back((
                        Event::Scalar {
                            value: std::borrow::Cow::Borrowed(""),
                            style: ScalarStyle::Plain,
                            anchor: self.pending_anchor.take().map(PendingAnchor::name),
                            tag: None,
                        },
                        zero_span(pos),
                    ));
                    Event::MappingEnd
                }
                CollectionEntry::Mapping(_, MappingPhase::Key, _) => Event::MappingEnd,
            };
            self.queue.push_back((ev, zero_span(pos)));
            // After closing any collection, advance the parent mapping (if in
            // Value phase) to Key phase — the just-closed collection was its value.
            if let Some(CollectionEntry::Mapping(_, phase, _)) = self.coll_stack.last_mut() {
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
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
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
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
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
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
                    tag: self.pending_tag.take(),
                },
                span,
            )));
        }
        // Pass Some(parent_indent) when inside a block collection so
        // collect_double_quoted_continuations can validate continuation-line
        // indentation (YAML 1.2 §7.3.1).  At document root (coll_stack empty)
        // there is no enclosing block, so no indent constraint: pass None.
        let dq_block_indent = if self.coll_stack.is_empty() {
            None
        } else {
            Some(plain_parent_indent)
        };
        if let Some((value, span)) = self.lexer.try_consume_double_quoted(dq_block_indent)? {
            // In block context, after a double-quoted scalar closes, the only
            // valid trailing content is optional whitespace followed by an
            // optional comment (with mandatory preceding whitespace before `#`).
            // Non-comment, non-whitespace content is an error.
            if let Some((tail, tail_pos)) = self.lexer.pending_multiline_tail.take() {
                let first_non_ws = tail.trim_start_matches([' ', '\t']);
                if !first_non_ws.is_empty() {
                    let ws_len = tail.len() - first_non_ws.len();
                    if first_non_ws.starts_with('#') && ws_len == 0 {
                        // `#` immediately after closing quote — not a comment.
                        self.failed = true;
                        return Err(Error {
                            pos: tail_pos,
                            message: "comment requires at least one space before '#'".into(),
                        });
                    } else if !first_non_ws.starts_with('#') {
                        // Non-comment content after quoted scalar.
                        self.failed = true;
                        return Err(Error {
                            pos: tail_pos,
                            message: "unexpected content after quoted scalar".into(),
                        });
                    }
                    // Valid comment: discard (the comment event is not emitted
                    // in block context here; it will be picked up by drain_trailing_comment
                    // in the normal flow).
                }
            }
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::DoubleQuoted,
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
                    tag: self.pending_tag.take(),
                },
                span,
            )));
        }
        if let Some((value, span)) = self.lexer.try_consume_plain_scalar(plain_parent_indent) {
            // Check for invalid content in the suffix (e.g. NUL or mid-stream
            // BOM that stopped the scanner but is not valid at this position).
            if let Some(e) = self.lexer.plain_scalar_suffix_error.take() {
                return Err(e);
            }
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Plain,
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
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
    #[allow(clippy::too_many_lines)]
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

        // --- Explicit key: `? ` or `?` at EOL ---
        //
        // The explicit key indicator is `?` followed by whitespace or end of
        // line (YAML 1.2 §8.2.2).  A `?` followed by a non-whitespace character
        // (e.g. `?foo: val`) is NOT an explicit key — `?foo` is an implicit key
        // that starts with `?`, just like `?foo: val` being a mapping entry where
        // the key is the plain scalar `?foo`.  This check must mirror the
        // condition in peek_mapping_entry to keep consume and peek consistent.
        if let Some(after_q) = trimmed.strip_prefix('?') {
            let is_explicit_key = after_q.is_empty()
                || after_q.starts_with(' ')
                || after_q.starts_with('\t')
                || after_q.starts_with('\n')
                || after_q.starts_with('\r');
            if is_explicit_key {
                let inline = after_q.trim_start_matches([' ', '\t']);
                // A trailing comment (`# ...`) is not key content — treat as
                // if nothing followed the `?` indicator.
                let had_key_inline = !inline.is_empty() && !inline.starts_with('#');

                if had_key_inline {
                    // Offset from line start to inline key content.
                    let spaces_after_q = after_q.len() - inline.len();
                    let total_offset = leading_spaces + 1 + spaces_after_q;
                    let inline_col = key_indent + 1 + spaces_after_q;
                    let inline_pos = Pos {
                        byte_offset: line_pos.byte_offset + total_offset,
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
        let value_byte_offset_in_trimmed = colon_offset + 1 + spaces_after_colon;
        // `colon_offset` is a byte offset into `trimmed`; key text can contain
        // multi-byte UTF-8.  Convert the prefix bytes to char count for column.
        let key_chars = trimmed[..colon_offset].chars().count();
        let value_col_in_trimmed = key_chars + 1 + spaces_after_colon;
        let value_col = key_indent + value_col_in_trimmed;
        let value_pos = Pos {
            byte_offset: line_pos.byte_offset + leading_spaces + value_byte_offset_in_trimmed,
            line: line_pos.line,
            column: line_pos.column + leading_spaces + value_col_in_trimmed,
        };

        // Detect whether the key is a quoted scalar.  `key_content` already
        // has its outer whitespace stripped; if it starts with `'` or `"` the
        // key is quoted and must be decoded rather than emitted as Plain.
        let key_is_quoted = matches!(key_content.as_bytes().first(), Some(b'"' | b'\''));

        // Consume the physical line, then (if inline value content exists)
        // prepend one synthetic line for the value.  The key is returned
        // directly in the ConsumedMapping variant — not via a synthetic line —
        // so that the caller can push a Scalar event without routing through
        // try_consume_plain_scalar (which would incorrectly treat the value
        // synthetic line as a plain-scalar continuation).
        self.lexer.consume_line();

        // If the key is quoted, decode it now using the lexer's existing
        // quoted-scalar methods.  We prepend a synthetic line containing only
        // the key text (including the surrounding quote characters) so the
        // method can parse it normally, then discard the synthetic line.
        //
        // libfyaml (fy-parse.c, fy_attach_comments_if_any / token scanner):
        // all scalar tokens — quoted or plain — flow through the same token
        // queue; the *scanner* decodes the scalar at the token level before
        // the parser ever sees it.  We replicate that by decoding quoted keys
        // here, at the point where we know the key is quoted.
        let (decoded_key, key_style) = if key_is_quoted {
            let key_synthetic = Line {
                content: key_content,
                offset: key_start_pos.byte_offset,
                indent: leading_spaces,
                break_type: line_break_type,
                pos: key_start_pos,
            };
            self.lexer.prepend_inline_line(key_synthetic);

            if key_content.starts_with('\'') {
                match self.lexer.try_consume_single_quoted(0) {
                    Ok(Some((value, _))) => (value, ScalarStyle::SingleQuoted),
                    Ok(None) => {
                        return ConsumedMapping::QuotedKeyError {
                            pos: key_start_pos,
                            message: "single-quoted key could not be parsed".into(),
                        };
                    }
                    Err(e) => {
                        return ConsumedMapping::QuotedKeyError {
                            pos: e.pos,
                            message: e.message,
                        };
                    }
                }
            } else {
                match self.lexer.try_consume_double_quoted(None) {
                    Ok(Some((value, _))) => (value, ScalarStyle::DoubleQuoted),
                    Ok(None) => {
                        return ConsumedMapping::QuotedKeyError {
                            pos: key_start_pos,
                            message: "double-quoted key could not be parsed".into(),
                        };
                    }
                    Err(e) => {
                        return ConsumedMapping::QuotedKeyError {
                            pos: e.pos,
                            message: e.message,
                        };
                    }
                }
            }
        } else {
            (std::borrow::Cow::Borrowed(key_content), ScalarStyle::Plain)
        };

        if !value_content.is_empty() {
            // Detect illegal inline implicit mapping: if the inline value itself
            // contains a value indicator (`:` followed by space/EOL), this is an
            // attempt to start a block mapping inline (e.g. `a: b: c: d` or
            // `a: 'b': c`).  Block mappings cannot appear inline — their entries
            // must start on new lines.  Return an error before prepending the value.
            if find_value_indicator_offset(value_content).is_some() {
                return ConsumedMapping::InlineImplicitMappingError { pos: value_pos };
            }

            // Detect illegal inline block sequence: `key: - item` is invalid
            // because a block sequence indicator (`-`) cannot appear as an
            // inline value of a block mapping entry — the sequence must start
            // on a new line.  Only `- `, `-\t`, or bare `-` (at EOL) qualify
            // as sequence indicators.
            {
                let after_dash = value_content.strip_prefix('-');
                let is_seq_indicator = after_dash.is_some_and(|rest| {
                    rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')
                });
                if is_seq_indicator {
                    return ConsumedMapping::InlineImplicitMappingError { pos: value_pos };
                }
            }

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
            key_value: decoded_key,
            key_style,
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
        // The explicit key's content has been processed; clear the pending flag.
        self.explicit_key_pending = false;
        for entry in self.coll_stack.iter_mut().rev() {
            if let CollectionEntry::Mapping(_, phase, has_had_value) = entry {
                *phase = MappingPhase::Value;
                *has_had_value = true;
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
            if let CollectionEntry::Mapping(_, phase, _) = entry {
                *phase = MappingPhase::Key;
                return;
            }
        }
    }

    /// Returns the minimum column at which a standalone block-node property
    /// (anchor or tag on its own line) is valid in the current context.
    ///
    /// - Mapping in Value phase at indent `n`: the value node must be at col > n.
    /// - Sequence at indent `n`: item content must be at col > n.
    /// - Mapping in Key phase at indent `n`: a key at col `n` is valid.
    /// - Root (empty stack): any column is valid.
    fn min_standalone_property_indent(&self) -> usize {
        match self.coll_stack.last() {
            Some(
                CollectionEntry::Mapping(n, MappingPhase::Value, _)
                | CollectionEntry::Sequence(n, _),
            ) => n + 1,
            Some(CollectionEntry::Mapping(n, MappingPhase::Key, _)) => *n,
            None => 0,
        }
    }
}

use mapping::{
    find_value_indicator_offset, inline_contains_mapping_key, is_implicit_mapping_line,
    is_tab_indented_block_indicator,
};
use properties::{is_valid_tag_handle, scan_anchor_name, scan_tag};

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
            // Per YAML 1.2 §9.2, directives require a `---` marker.
            // A directive followed by EOF (no `---`) is a spec violation.
            if self.directive_scope.directive_count > 0 {
                let pos = self.lexer.current_pos();
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos,
                    message: "directives must be followed by a '---' document-start marker".into(),
                }));
            }
            let end = self.lexer.current_pos();
            self.state = IterState::Done;
            return StepResult::Yield(Ok((Event::StreamEnd, zero_span(end))));
        }
        if self.lexer.is_directives_end() {
            let (marker_pos, _) = self.lexer.consume_marker_line(false);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
            self.state = IterState::InDocument;
            self.root_node_emitted = false;
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
            self.lexer.consume_marker_line(true);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
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
        self.root_node_emitted = false;
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

        // ---- Tab indentation check ----
        //
        // YAML 1.2 §6.1: tabs cannot be used for indentation in block context.
        // Only lines whose VERY FIRST character is `\t` (no leading spaces) are
        // using a tab as the indentation character and must be rejected.
        //
        // Exceptions: `\t[`, `\t{`, `\t]`, `\t}` are allowed because flow
        // collection delimiters can follow tabs (YAML test suite 6CA3, Q5MG).
        // Lines like `  \tx` have SPACES as indentation; the tab is content.
        if let Some(line) = self.lexer.peek_next_line() {
            if line.content.starts_with('\t') {
                // First char is a tab — check what the first non-tab character
                // is.  Flow collection delimiters are allowed after leading tabs.
                let first_non_tab = line.content.trim_start_matches('\t').chars().next();
                if !matches!(first_non_tab, Some('[' | '{' | ']' | '}')) {
                    let err_pos = line.pos;
                    self.failed = true;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "tabs are not allowed as indentation (YAML 1.2 §6.1)".into(),
                    }));
                }
            }
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
            let (marker_pos, _) = self.lexer.consume_marker_line(true);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
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
            let (marker_pos, _) = self.lexer.consume_marker_line(false);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
            // A bare `---` inside a document implicitly ends the current document
            // and starts a new one without a preamble.  Reset the directive scope
            // here since consume_preamble_between_docs will not be called for this
            // transition.
            self.directive_scope = DirectiveScope::default();
            // Validate any inline tag on this `---` line against the new
            // document's (empty) directive scope.  Tags defined in the previous
            // document do not carry over (YAML §9.2), so an undefined handle
            // must fail immediately.
            if let Some((tag_val, tag_pos)) = self.lexer.peek_inline_scalar() {
                if tag_val.starts_with('!') {
                    if let Err(e) = self.directive_scope.resolve_tag(tag_val, tag_pos) {
                        self.lexer.drain_inline_scalar();
                        self.failed = true;
                        return StepResult::Yield(Err(e));
                    }
                }
            }
            self.state = IterState::InDocument;
            self.root_node_emitted = false;
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

        // ---- Directive lines (`%YAML`/`%TAG`) inside document body ----
        //
        // YAML 1.2 §9.2: directives can only appear in the preamble (before
        // `---`).  A `%YAML` or `%TAG` line inside a document body, followed
        // by `---`, indicates the author forgot to close the previous document
        // with `...` before writing the next document's preamble.
        //
        // We only fire the error when:
        //   1. The current line starts with `%YAML ` or `%TAG ` (a genuine
        //      YAML directive keyword, not arbitrary content like `%!PS-Adobe`).
        //   2. The following line is a `---` document-start marker.
        //
        // This avoids false positives when `%` appears as content in plain
        // scalars (XLQ9) or inside block scalar bodies (M7A3, W4TN).
        if let Some(line) = self.lexer.peek_next_line() {
            let is_yaml_directive =
                line.content.starts_with("%YAML ") || line.content.starts_with("%TAG ");
            if is_yaml_directive {
                let next_is_doc_start = self.lexer.peek_second_line().is_some_and(|l| {
                    l.content == "---"
                        || l.content.starts_with("--- ")
                        || l.content.starts_with("---\t")
                });
                if next_is_doc_start {
                    let err_pos = line.pos;
                    self.failed = true;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message:
                            "directive '%' is only valid before the document-start marker '---'"
                                .into(),
                    }));
                }
            }
        }

        // ---- Root-node guard ----
        //
        // A YAML document contains exactly one root node.  Once the root has
        // been fully emitted (`root_node_emitted = true`) and the collection
        // stack is empty, any further non-comment, non-blank content is invalid.
        if self.root_node_emitted && self.coll_stack.is_empty() && !self.lexer.has_inline_scalar() {
            if let Some(line) = self.lexer.peek_next_line() {
                let err_pos = line.pos;
                self.failed = true;
                self.lexer.consume_line();
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: "unexpected content after document root node".into(),
                }));
            }
        }

        // ---- Alias node: `*name` is a complete node ----

        if let Some(peek) = self.lexer.peek_next_line() {
            let content: &'input str = peek.content;
            let line_pos = peek.pos;
            let line_break_type = peek.break_type;
            let trimmed = content.trim_start_matches(' ');
            if let Some(after_star) = trimmed.strip_prefix('*') {
                let leading = content.len() - trimmed.len();
                let star_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
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
                // An Inline anchor preceding `*alias` is an error — it would annotate
                // the alias node, which is illegal.  A Standalone anchor belongs to
                // the surrounding collection, not the alias, so it is not an error here.
                if matches!(self.pending_anchor, Some(PendingAnchor::Inline(_))) {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have an anchor property".into(),
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
                        let rem_col = star_pos.column + 1 + name_char_count + spaces;
                        self.lexer.consume_line();
                        if had_remaining {
                            let rem_pos = Pos {
                                byte_offset: rem_byte_offset,
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
            let line_indent = peek.indent;
            let line_break_type = peek.break_type;
            let trimmed = content.trim_start_matches(' ');
            if trimmed.starts_with('!') {
                let leading = content.len() - trimmed.len();
                let bang_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
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
                        // YAML 1.2 §6.8.1: a tag property must be separated from
                        // the following node content by `s-separate` when the first
                        // character after the tag could be confused with a tag
                        // continuation or creates structural ambiguity:
                        // - `!` starts another tag property
                        // - flow indicators (`,`, `[`, `]`, `{`, `}`) cause
                        //   structural confusion (e.g. `!!str,`)
                        // - `%` may be a valid percent-encoded continuation that
                        //   should have been part of the tag, or an invalid
                        //   percent-sequence that makes the input unparseable
                        // When the tag scanner stopped at a plain non-tag char like
                        // `<`, the tag ended naturally and the content is the value
                        // (e.g. `!foo<bar val` → tag=`!foo`, scalar=`<bar val`).
                        if had_inline && spaces == 0 {
                            let first = inline.chars().next().unwrap_or('\0');
                            if first == '!'
                                || first == '%'
                                || matches!(first, ',' | '[' | ']' | '{' | '}')
                            {
                                self.failed = true;
                                return StepResult::Yield(Err(Error {
                                    pos: bang_pos,
                                    message:
                                        "tag must be separated from node content by whitespace"
                                            .into(),
                                }));
                            }
                        }
                        let inline_offset =
                            line_pos.byte_offset + leading + tag_token_bytes + spaces;
                        let inline_col = line_pos.column + leading + tag_token_bytes + spaces;
                        // Duplicate tags on the same node are an error.
                        // Exception: if the existing tag is collection-level
                        // (pending_tag_for_collection=true) and the new tag has
                        // inline content that is (or contains) a mapping key line,
                        // they apply to different nodes (collection vs. key scalar).
                        if self.pending_tag.is_some() {
                            let is_different_node = self.pending_tag_for_collection
                                && had_inline
                                && inline_contains_mapping_key(inline);
                            if !is_different_node {
                                self.failed = true;
                                return StepResult::Yield(Err(Error {
                                    pos: bang_pos,
                                    message: "a node may not have more than one tag".into(),
                                }));
                            }
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
                            // Record the original physical line's indent so that
                            // handle_mapping_entry can open the mapping at the correct
                            // indent when the key is on a synthetic (offset) line.
                            // Only set when the inline content is (or leads to) a
                            // mapping key — if it's a plain value, there is no
                            // handle_mapping_entry call to consume this, and leaving
                            // it set would corrupt the next unrelated mapping entry.
                            if self.property_origin_indent.is_none()
                                && inline_contains_mapping_key(inline)
                            {
                                self.property_origin_indent = Some(line_indent);
                            }
                            let inline_pos = Pos {
                                byte_offset: inline_offset,
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
                            // Validate: the tag must be indented enough for this context.
                            let min = self.min_standalone_property_indent();
                            if line_indent < min {
                                self.pending_tag = None;
                                self.failed = true;
                                return StepResult::Yield(Err(Error {
                                    pos: bang_pos,
                                    message:
                                        "node property is not indented enough for this context"
                                            .into(),
                                }));
                            }
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
            let line_indent = peek.indent;
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
                        let name_char_count = name.chars().count();
                        let inline_offset =
                            line_pos.byte_offset + leading + 1 + name.len() + spaces;
                        let inline_col = line_pos.column + leading + 1 + name_char_count + spaces;
                        // Duplicate anchors on the same node are an error.
                        //
                        // Case 1: existing anchor is inline (Inline variant) and no
                        // collection tag is pending — both this and the existing anchor
                        // are for the same item-level node.
                        //
                        // Case 2: existing anchor is standalone (Standalone variant)
                        // and the new anchor has inline content that is NOT a collection
                        // opener ([, {) or property (!, &) — both anchors apply to the
                        // same scalar node.
                        let amp_pos2 = amp_pos;
                        let is_duplicate =
                            if matches!(self.pending_anchor, Some(PendingAnchor::Inline(_)))
                                && !self.pending_tag_for_collection
                            {
                                true
                            } else if matches!(
                                self.pending_anchor,
                                Some(PendingAnchor::Standalone(_))
                            ) && had_inline
                                && !self.pending_tag_for_collection
                            {
                                // The existing anchor is collection-level, but the new anchor
                                // has inline content.  If that content is a mapping key line
                                // (contains `: ` etc.), the new anchor is for the key and the
                                // existing anchor is for the mapping — different nodes, no error.
                                // If the inline is a plain scalar (no key indicator), both
                                // anchors apply to the same scalar node — error.
                                let first_ch = inline.chars().next();
                                // If inline starts with a collection/property opener, treat as
                                // different node — no error.
                                let starts_with_opener = matches!(
                                    first_ch,
                                    Some('[' | '{' | '!' | '&' | '*' | '|' | '>')
                                );
                                // If inline contains a mapping key indicator (`: `), the new
                                // anchor is for a key — different node from the collection.
                                let is_mapping_key = find_value_indicator_offset(inline).is_some();
                                !starts_with_opener && !is_mapping_key
                            } else {
                                false
                            };
                        if is_duplicate {
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: amp_pos2,
                                message: "a node may not have more than one anchor".into(),
                            }));
                        }
                        self.lexer.consume_line();
                        if had_inline {
                            // Detect illegal inline block sequence: `&anchor - item`
                            // is invalid — a block sequence indicator cannot appear
                            // inline after an anchor property in block context.
                            let is_seq = inline.strip_prefix('-').is_some_and(|rest| {
                                rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')
                            });
                            if is_seq {
                                self.failed = true;
                                let seq_pos = Pos {
                                    byte_offset: inline_offset,
                                    line: line_pos.line,
                                    column: inline_col,
                                };
                                return StepResult::Yield(Err(Error {
                                    pos: seq_pos,
                                    message:
                                        "block sequence indicator cannot appear inline after a node property"
                                            .into(),
                                }));
                            }
                            // Inline content after anchor — anchor applies to the
                            // inline node (scalar or key), not to any enclosing
                            // collection opened on this same line.
                            self.pending_anchor = Some(PendingAnchor::Inline(name));
                            // Record the original physical line's indent so that
                            // handle_mapping_entry can open the mapping at the correct
                            // indent when the key is on a synthetic (offset) line.
                            // Only set when the inline content leads to a mapping key;
                            // value-context anchors must not corrupt the next entry.
                            if self.property_origin_indent.is_none()
                                && inline_contains_mapping_key(inline)
                            {
                                self.property_origin_indent = Some(line_indent);
                            }
                            let inline_pos = Pos {
                                byte_offset: inline_offset,
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
                            // Validate: the anchor must be indented enough for this context.
                            let min = self.min_standalone_property_indent();
                            if line_indent < min {
                                self.failed = true;
                                let err_pos = amp_pos;
                                return StepResult::Yield(Err(Error {
                                    pos: err_pos,
                                    message:
                                        "node property is not indented enough for this context"
                                            .into(),
                                }));
                            }
                            self.pending_anchor = Some(PendingAnchor::Standalone(name));
                        }
                        // Let the next iteration handle whatever follows.
                        return StepResult::Continue;
                    }
                }
            }
        }

        // ---- Flow collection detection: `[` or `{` starts a flow collection ----
        // Stray closing flow indicators (`]`, `}`) in block context are errors.

        if let Some(line) = self.lexer.peek_next_line() {
            let trimmed = line.content.trim_start_matches(' ');
            if trimmed.starts_with('[') || trimmed.starts_with('{') {
                return self.handle_flow_collection();
            }
            if trimmed.starts_with(']') || trimmed.starts_with('}') {
                let err_pos = line.pos;
                let ch = trimmed.chars().next().unwrap_or(']');
                self.failed = true;
                self.lexer.consume_line();
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: format!("unexpected '{ch}' outside flow collection"),
                }));
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
            // Record the minimum indent across all open collections before
            // closing. A root collection has indent 0. If the minimum indent
            // before closure was 0 and the stack empties, the root node is
            // complete. When a tag-inline mapping opens at a column > 0 (a
            // pre-existing indent-tracking limitation), closing it must not
            // prematurely mark the root as emitted.
            let min_indent_before = self.coll_stack.iter().map(|e| e.indent()).min();
            self.close_collections_at_or_above(line_indent.saturating_add(1), close_pos);
            // If closing collections emptied the stack, the root node is
            // complete — but only if the outermost collection was at indent 0
            // (a true root collection, not a spuriously-indented inline tag).
            if self.coll_stack.is_empty() && !self.queue.is_empty() && min_indent_before == Some(0)
            {
                self.root_node_emitted = true;
            }
            if !self.queue.is_empty() {
                return StepResult::Continue;
            }
        }

        // ---- Block structure validity checks ----
        //
        // After closing deeper collections and before consuming a scalar,
        // validate that the current line's indentation is consistent with
        // the innermost open block collection.
        //
        // For block sequences: the only valid content at the sequence's own
        // indent level is `- ` (handled by peek_sequence_entry above).
        // Any other content at that indent level is invalid YAML.
        //
        // For block mappings in Key phase: the only valid content at the
        // mapping's indent level is a mapping entry (handled by
        // peek_mapping_entry above). A plain scalar without `: ` is not
        // a valid implicit mapping key.
        if let Some(line) = self.lexer.peek_next_line() {
            let line_indent = line.indent;
            match self.coll_stack.last() {
                Some(&CollectionEntry::Sequence(seq_indent, _)) if line_indent == seq_indent => {
                    // Content at the sequence indent level that is NOT `- ` is
                    // invalid. peek_sequence_entry already returned None, so this
                    // line is not a sequence entry.
                    let err_pos = line.pos;
                    self.failed = true;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "invalid content at block sequence indent level: expected '- '"
                            .into(),
                    }));
                }
                Some(&CollectionEntry::Mapping(map_indent, MappingPhase::Key, _))
                    if line_indent == map_indent =>
                {
                    let err_pos = line.pos;
                    self.failed = true;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message:
                            "invalid content at block mapping indent level: expected mapping key"
                                .into(),
                    }));
                }
                // Content more deeply indented than the mapping key level is only
                // valid as an explicit-key continuation (explicit_key_pending=true)
                // or as the very first key (has_had_value=false — the first key may
                // be at any indent >= map_indent).  After at least one key-value pair
                // has been processed (has_had_value=true) with no explicit-key pending,
                // deeper content that is not a valid mapping key is an error.
                Some(&CollectionEntry::Mapping(map_indent, MappingPhase::Key, true))
                    if line_indent > map_indent
                        && !self.explicit_key_pending
                        && !self.lexer.is_next_line_synthetic() =>
                {
                    let err_pos = line.pos;
                    self.failed = true;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "unexpected indented content after mapping value".into(),
                    }));
                }
                _ => {}
            }
        }

        // ---- Scalars ----

        // `block_parent_indent` — the indent of the enclosing block context;
        // block scalars (`|`, `>`) must have content lines more indented than
        // this value.  For a block scalar embedded as inline content after `? `
        // or `- `, the enclosing block's indent is the *collection's* indent,
        // not the column of the inline `|`/`>` token.
        //
        // `plain_parent_indent` — the enclosing block's indent level.
        // Plain scalar continuation lines must be indented strictly more than
        // `plain_parent_indent` (YAML 1.2), with a special exception for
        // tab-indented lines when `plain_parent_indent == 0` (the tab provides
        // the s-separate-in-line separator required by s-flow-folded(0)).
        // Use usize::MAX as a sentinel for "root level" — the root node has no
        // parent collection, so block scalar body lines may start at column 0
        // (equivalent to a parent indent of -1 in the YAML spec).
        let block_parent_indent = self.coll_stack.last().map_or(usize::MAX, |e| e.indent());
        let plain_parent_indent = self.coll_stack.last().map_or(0, |e| e.indent());
        // Capture whether an inline scalar (from `--- text`) was pending before
        // the scalar dispatch call.  If it was, the emitted plain scalar came
        // from the `---` marker line and is NOT necessarily the complete root
        // node — the lexer emits `--- >` / `--- |` / `--- "text` inline content
        // as a plain scalar, but the actual node body follows on subsequent
        // lines.  Marking root_node_emitted in those cases would incorrectly
        // reject the body lines as "content after root node".
        let had_inline_scalar = self.lexer.has_inline_scalar();
        match self.try_consume_scalar(plain_parent_indent, block_parent_indent) {
            Ok(Some(event)) => {
                self.tick_mapping_phase_after_scalar();
                // Drain any trailing comment detected on the scalar's line.
                self.drain_trailing_comment();
                // A scalar emitted at the document root (no open collection)
                // is the complete root node — unless it came from inline
                // content after `---` (had_inline_scalar), in which case the
                // body on subsequent lines is part of the same node.
                if self.coll_stack.is_empty() && !had_inline_scalar {
                    self.root_node_emitted = true;
                }
                return StepResult::Yield(Ok(event));
            }
            Err(e) => {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
            Ok(None) => {}
        }

        // Check for invalid characters at the start of an unrecognised line.
        // A line that starts with a character that is neither whitespace nor a
        // valid YAML ns-char (e.g. NUL U+0000 or mid-stream BOM U+FEFF) is a
        // parse error.
        if let Some(line) = self.lexer.peek_next_line() {
            let first_ch = line.content.chars().next();
            if let Some(ch) = first_ch {
                if ch != ' ' && ch != '\t' && !crate::lexer::is_ns_char(ch) {
                    let err_pos = line.pos;
                    self.failed = true;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: format!("invalid character U+{:04X} in document", ch as u32),
                    }));
                }
            }
        }

        // Fallback: unrecognised content line — consume and loop.
        self.lexer.consume_line();
        StepResult::Continue
    }

    /// Handle a block-sequence dash entry (`-`).
    #[allow(clippy::too_many_lines)]
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
                &(CollectionEntry::Sequence(col, _)
                | CollectionEntry::Mapping(col, MappingPhase::Key, _)),
            ) => dash_indent > col,
            Some(&CollectionEntry::Mapping(col, MappingPhase::Value, _)) => dash_indent >= col,
        };
        if opens_new {
            // A block sequence cannot be an implicit mapping key — only flow nodes
            // may appear as implicit keys.  If the parent is a mapping in Key phase
            // and we are about to open a new sequence, this is a block sequence
            // where a mapping key is expected: an error.
            // Exception: when explicit_key_pending is set, the sequence IS the
            // content of an explicit key (`? \n- seq_key`), which is valid.
            if matches!(
                self.coll_stack.last(),
                Some(&CollectionEntry::Mapping(_, MappingPhase::Key, true))
            ) && !self.explicit_key_pending
            {
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos: dash_pos,
                    message: "block sequence cannot appear as an implicit mapping key".into(),
                }));
            }
            // A block sequence item at a wrong indent level is invalid.  When the
            // parent is a sequence that has already completed at least one item
            // (`has_had_item = true`) and the new dash is NOT at the parent
            // sequence's column (not a new sibling item), this is a wrong-indent
            // sequence entry.
            if let Some(&CollectionEntry::Sequence(parent_col, true)) = self.coll_stack.last() {
                if dash_indent != parent_col {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: dash_pos,
                        message: "block sequence entry at wrong indentation level".into(),
                    }));
                }
            }
            if self.collection_depth() >= MAX_COLLECTION_DEPTH {
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos: dash_pos,
                    message: "collection nesting depth exceeds limit".into(),
                }));
            }
            // Sequence opening consumes any pending explicit-key context.
            self.explicit_key_pending = false;
            // Mark the parent sequence (if any) as having started an item.
            if let Some(CollectionEntry::Sequence(_, current_item_started)) =
                self.coll_stack.last_mut()
            {
                *current_item_started = true;
            }
            self.coll_stack
                .push(CollectionEntry::Sequence(dash_indent, false));
            self.queue.push_back((
                Event::SequenceStart {
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
                    tag: self.pending_tag.take(),
                    style: CollectionStyle::Block,
                },
                zero_span(dash_pos),
            ));
        }
        // When continuing an existing sequence (opens_new = false), reset
        // `current_item_started` so that the new item can receive content.
        if !opens_new {
            if let Some(CollectionEntry::Sequence(_, current_item_started)) =
                self.coll_stack.last_mut()
            {
                *current_item_started = false;
            }
        }
        // When continuing an existing sequence (opens_new = false) and there is
        // a pending tag/anchor from the previous item's content (e.g. `- !!str`
        // whose inline extraction left a standalone tag line), that tag/anchor
        // applies to an empty scalar for the previous item.  Emit it now before
        // processing the current `-`.
        if !opens_new
            && (self.pending_tag_for_collection
                || matches!(self.pending_anchor, Some(PendingAnchor::Standalone(_))))
            && (self.pending_tag.is_some() || self.pending_anchor.is_some())
        {
            let item_pos = self.lexer.current_pos();
            self.queue.push_back((
                Event::Scalar {
                    value: std::borrow::Cow::Borrowed(""),
                    style: ScalarStyle::Plain,
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
                    tag: self.pending_tag.take(),
                },
                zero_span(item_pos),
            ));
            self.pending_tag_for_collection = false;
        }
        // Check for tab-indented block structure before consuming the dash.
        // In YAML, tabs cannot be used for block-level indentation.  When the
        // separator between the dash and the inline content is (or contains) a
        // tab, and the inline content is a block structure indicator, the tab
        // is acting as indentation for a block node — which is invalid
        // (YAML 1.2 §6.1).
        if let Some(line) = self.lexer.peek_next_line() {
            let after_spaces = line.content.trim_start_matches(' ');
            if let Some(rest) = after_spaces.strip_prefix('-') {
                let inline = rest.trim_start_matches([' ', '\t']);
                let separator = &rest[..rest.len() - inline.len()];
                if separator.contains('\t') && is_tab_indented_block_indicator(inline) {
                    let err_pos = line.pos;
                    self.failed = true;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "tab character is not valid block indentation".into(),
                    }));
                }
            }
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
                        anchor: self.pending_anchor.take().map(PendingAnchor::name),
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

        // When an anchor or tag appeared inline on the physical line before
        // the key content (e.g. `&anchor key: value`), the key is prepended
        // as a synthetic line at the property's column (e.g. column 8).
        // All indent-relative decisions below must use the PHYSICAL line's
        // indent (column 0 in that example), not the synthetic line's column.
        let effective_key_indent = self.property_origin_indent.unwrap_or(key_indent);

        self.close_collections_at_or_above(effective_key_indent.saturating_add(1), cur_pos);
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
        if let Some(&CollectionEntry::Sequence(seq_col, _)) = self.coll_stack.last() {
            if seq_col == effective_key_indent {
                let parent_is_seq_spaces_mapping = self.coll_stack.iter().rev().nth(1).is_some_and(
                    |e| matches!(e, CollectionEntry::Mapping(col, _, _) if *col == effective_key_indent),
                );
                if parent_is_seq_spaces_mapping {
                    self.coll_stack.pop();
                    self.queue
                        .push_back((Event::SequenceEnd, zero_span(cur_pos)));
                    // Advance parent mapping from Value to Key phase — the
                    // sequence was its value and is now fully closed.
                    if let Some(CollectionEntry::Mapping(_, phase, _)) = self.coll_stack.last_mut()
                    {
                        *phase = MappingPhase::Key;
                    }
                    return StepResult::Continue;
                }
            }
        }

        let is_in_mapping_at_this_indent = self.coll_stack.last().is_some_and(
            |top| matches!(top, CollectionEntry::Mapping(col, _, _) if *col == effective_key_indent),
        );

        if !is_in_mapping_at_this_indent {
            // A mapping entry at `effective_key_indent` cannot be opened when:
            //
            // 1. The top of the stack is a block sequence at the same indent —
            //    this would nest a mapping inside the sequence without a `- `
            //    prefix (BD7L pattern).
            //
            // 2. The top of the stack is a block mapping in Key phase at a
            //    lesser indent that has already had at least one entry — this
            //    would open a nested mapping when no current key exists for it
            //    to be the value of (EW3V, DMG6, N4JP, U44R patterns: wrong
            //    indentation).  The `has_had_value` flag suppresses this check
            //    for fresh mappings whose first key node is nested deeper than
            //    the mapping indicator (e.g. V9D5 explicit-key content).
            //    Also skip when a value-indicator line (`: value`) is next
            //    because it is the value portion of an alias/anchor mapping key
            //    split across tokens (e.g. `*alias : scalar` in 26DV), or when
            //    a pending tag or anchor is present (tags prepend synthetic
            //    inlines at their column — 74H7).
            match self.coll_stack.last() {
                Some(&CollectionEntry::Sequence(seq_col, _)) if seq_col == effective_key_indent => {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: key_pos,
                        message:
                            "invalid mapping entry at block sequence indent level: expected '- '"
                                .into(),
                    }));
                }
                Some(&CollectionEntry::Mapping(map_col, MappingPhase::Key, true))
                    if map_col < effective_key_indent
                        && self.pending_tag.is_none()
                        && self.pending_anchor.is_none()
                        && !self.is_value_indicator_line() =>
                {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: key_pos,
                        message: "wrong indentation: mapping key is more indented than the enclosing mapping".into(),
                    }));
                }
                _ => {}
            }
            if self.collection_depth() >= MAX_COLLECTION_DEPTH {
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos: key_pos,
                    message: "collection nesting depth exceeds limit".into(),
                }));
            }
            // Mark the parent sequence (if any) as having started an item.
            if let Some(CollectionEntry::Sequence(_, current_item_started)) =
                self.coll_stack.last_mut()
            {
                *current_item_started = true;
            }
            // Note: property_origin_indent is NOT consumed here.  It remains set
            // so the next call (which processes the synthetic key line at the
            // synthetic column) can again compute effective_key_indent = origin
            // indent and recognize the already-open mapping.  It will be cleared
            // in the "continuing existing mapping" branch below.
            self.coll_stack.push(CollectionEntry::Mapping(
                effective_key_indent,
                MappingPhase::Key,
                false,
            ));
            // Consume pending anchor/tag for the mapping only for standalone
            // properties (e.g. `&a\nkey: v`) where `pending_*_for_collection`
            // is true.
            //
            // Inline properties (e.g. `&a key: v`) leave `pending_*_for_collection`
            // false — they annotate the key scalar, not the mapping (YAML test
            // suite 9KAX: inline property → key scalar).  The pending anchor/tag
            // is left on `self.pending_anchor`/`self.pending_tag` and will be
            // consumed by `consume_mapping_entry` when it emits the key scalar.
            let mapping_anchor =
                if matches!(self.pending_anchor, Some(PendingAnchor::Standalone(_))) {
                    self.pending_anchor.take().map(PendingAnchor::name)
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
            // If there is a pending tag/anchor that is not designated for the
            // mapping collection itself (i.e. it came from an inline `!!tag`
            // or `&anchor` before the `:` value indicator), it applies to the
            // empty implicit key scalar.  Emit that key scalar first so the
            // pending properties are not lost and the mapping phase advances
            // correctly before the value indicator is consumed.
            let in_key_phase = self.coll_stack.last().is_some_and(|top| {
                matches!(top, CollectionEntry::Mapping(col, MappingPhase::Key, _) if *col == effective_key_indent)
            });
            if in_key_phase
                && !self.pending_tag_for_collection
                && !matches!(self.pending_anchor, Some(PendingAnchor::Standalone(_)))
                && (self.pending_tag.is_some() || self.pending_anchor.is_some())
            {
                let pos = self.lexer.current_pos();
                self.queue.push_back((
                    Event::Scalar {
                        value: std::borrow::Cow::Borrowed(""),
                        style: ScalarStyle::Plain,
                        anchor: self.pending_anchor.take().map(PendingAnchor::name),
                        tag: self.pending_tag.take(),
                    },
                    zero_span(pos),
                ));
                self.advance_mapping_to_value();
                return StepResult::Continue;
            }
            // Check for tab-indented block structure after explicit value marker.
            // `: TAB -`, `: TAB ?`, or `: TAB key:` are invalid because the tab
            // makes the following block-structure-forming content block-indented
            // via a tab, which is forbidden (YAML 1.2 §6.1).
            if let Some(line) = self.lexer.peek_next_line() {
                let after_spaces = line.content.trim_start_matches(' ');
                if let Some(after_colon) = after_spaces.strip_prefix(':') {
                    if !after_colon.is_empty() {
                        let value = after_colon.trim_start_matches([' ', '\t']);
                        let separator = &after_colon[..after_colon.len() - value.len()];
                        if separator.contains('\t') && is_tab_indented_block_indicator(value) {
                            let err_pos = line.pos;
                            self.failed = true;
                            self.lexer.consume_line();
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "tab character is not valid block indentation".into(),
                            }));
                        }
                    }
                }
            }
            self.consume_explicit_value_line(key_indent);
            return StepResult::Continue;
        }

        // If the mapping is in Value phase and the next line is another key
        // (not a `: value` line), the previous key had no value — emit empty.
        if self.coll_stack.last().is_some_and(|top| {
            matches!(top, CollectionEntry::Mapping(col, MappingPhase::Value, _) if *col == effective_key_indent)
        }) {
            let pos = self.lexer.current_pos();
            self.queue.push_back((
                Event::Scalar {
                    value: std::borrow::Cow::Borrowed(""),
                    style: ScalarStyle::Plain,
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
                    tag: None,
                },
                zero_span(pos),
            ));
            self.advance_mapping_to_key();
            return StepResult::Continue;
        }

        // Check for tab-indented block structure after explicit key marker.
        // `? TAB -`, `? TAB ?`, or `? TAB key:` are invalid because the tab
        // makes the following block-structure-forming content block-indented
        // via a tab, which is forbidden (YAML 1.2 §6.1).
        if let Some(line) = self.lexer.peek_next_line() {
            let after_spaces = line.content.trim_start_matches(' ');
            if let Some(after_q) = after_spaces.strip_prefix('?') {
                if !after_q.is_empty() {
                    let inline = after_q.trim_start_matches([' ', '\t']);
                    let separator = &after_q[..after_q.len() - inline.len()];
                    if separator.contains('\t') && is_tab_indented_block_indicator(inline) {
                        let err_pos = line.pos;
                        self.failed = true;
                        self.lexer.consume_line();
                        return StepResult::Yield(Err(Error {
                            pos: err_pos,
                            message: "tab character is not valid block indentation".into(),
                        }));
                    }
                }
            }
        }
        // Normal key line: consume and emit key scalar.
        // property_origin_indent has served its purpose (selecting effective
        // indent for the mapping-open and for subsequent continues).  Clear it
        // so it does not affect unrelated subsequent entries.
        self.property_origin_indent = None;
        let consumed = self.consume_mapping_entry(key_indent);
        match consumed {
            ConsumedMapping::ExplicitKey { had_key_inline } => {
                if had_key_inline {
                    // The key content will appear inline (already prepended).
                    // No explicit-key-pending needed since the key content is
                    // already in the buffer.
                } else {
                    let pos = self.lexer.current_pos();
                    self.queue.push_back((
                        Event::Scalar {
                            value: std::borrow::Cow::Borrowed(""),
                            style: ScalarStyle::Plain,
                            anchor: self.pending_anchor.take().map(PendingAnchor::name),
                            tag: self.pending_tag.take(),
                        },
                        zero_span(pos),
                    ));
                    self.advance_mapping_to_value();
                    // The key content is on the NEXT line — mark that an explicit
                    // key is pending so block sequence entries are allowed
                    // (e.g. `?\n- seq_key`).
                    self.explicit_key_pending = true;
                }
            }
            ConsumedMapping::ImplicitKey {
                key_value,
                key_style,
                key_span,
            } => {
                self.queue.push_back((
                    Event::Scalar {
                        value: key_value,
                        style: key_style,
                        anchor: self.pending_anchor.take().map(PendingAnchor::name),
                        tag: self.pending_tag.take(),
                    },
                    key_span,
                ));
                self.advance_mapping_to_value();
            }
            ConsumedMapping::QuotedKeyError { pos, message } => {
                self.failed = true;
                return StepResult::Yield(Err(Error { pos, message }));
            }
            ConsumedMapping::InlineImplicitMappingError { pos } => {
                // The inline value is a block node (mapping or sequence indicator)
                // which cannot appear inline as a mapping value — block nodes must
                // start on a new line.
                self.failed = true;
                return StepResult::Yield(Err(Error {
                    pos,
                    message:
                        "block node cannot appear as inline value; use a new line or a flow node"
                            .into(),
                }));
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
        // A comment-only value (e.g. `: # lala`) is not a real inline value.
        let had_value_inline = !value_content.is_empty() && !value_content.starts_with('#');

        if had_value_inline {
            let spaces_after_colon = after_colon.len() - value_content.len();
            let total_offset = leading_spaces + 1 + spaces_after_colon;
            let value_col = key_indent + 1 + spaces_after_colon;
            let value_pos = Pos {
                byte_offset: line_pos.byte_offset + total_offset,
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
            // `:` with no real value content (either bare or comment-only).
            // Consume the indicator line and advance to Value phase — the next
            // line may be a block node (the actual value), or if the next line
            // is another key at the same indent, the main loop emits an empty
            // scalar at that point (see the Value-phase empty-scalar guard).
            self.lexer.consume_line();
            self.advance_mapping_to_value();
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
            ///
            /// `after_colon` is `true` when we have just consumed a `:` value
            /// separator in a single-pair implicit mapping context.  In this
            /// state a new scalar or collection is the value of the single-pair
            /// mapping — not a new entry — so the missing-comma check must not
            /// fire.
            ///
            /// `last_was_plain` is `true` when the most recent emitted item was
            /// a plain scalar.  Plain scalars may span multiple lines in flow
            /// context, so the missing-comma check must not fire after a plain
            /// scalar (the next line's content may be a continuation).
            Sequence {
                has_value: bool,
                after_colon: bool,
                last_was_plain: bool,
            },
            /// An open `{...}` mapping.
            ///
            /// `has_value` tracks the same invariant as in `Sequence` but for
            /// the mapping as a whole (not per key/value pair).
            ///
            /// `last_was_plain` mirrors the same concept as in `Sequence`: when
            /// the most recent emitted item was a plain scalar, the next line
            /// may be a multi-line continuation, so indicator-start validation
            /// must be deferred until we know whether it is a continuation.
            Mapping {
                phase: FlowMappingPhase,
                has_value: bool,
                last_was_plain: bool,
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
        // The physical line number where the outermost flow collection opened.
        // Used to detect multi-line flow keys (C2SP).
        let start_line = first_line.pos.line;
        // The physical line number of the most recent emitted value (scalar or
        // inner-collection close).  Used to detect multi-line implicit keys (DK4H):
        // a `:` value separator on a different line than the preceding key is invalid.
        let mut last_token_line = first_line.pos.line;
        // Set when a `?` explicit-key indicator is consumed inside a flow sequence.
        // Suppresses the DK4H single-line check for the corresponding `:` separator —
        // explicit keys in flow sequences may span multiple lines (YAML 1.2 §7.4.2).
        let mut explicit_key_in_seq = false;

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
        let mut pending_flow_anchor: Option<&'input str> =
            self.pending_anchor.take().map(PendingAnchor::name);
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

        // The minimum indent for continuation lines in this flow collection.
        // When the flow collection is inside an enclosing block collection,
        // continuation lines must be indented more than the enclosing block's
        // indent level (YAML 1.2: flow context lines must not regress to or
        // below the enclosing block indent level).
        // At document root (coll_stack empty), there is no enclosing block, so
        // no constraint — represented as None.
        let flow_min_indent: Option<usize> = self.coll_stack.last().map(|e| e.indent());

        // -----------------------------------------------------------------------
        // Main parse loop — iterates over characters in the current (and
        // subsequent) lines until the outermost closing delimiter is found.
        // -----------------------------------------------------------------------

        'outer: loop {
            // Document markers (`---` and `...`) are only valid at the document
            // level — they are illegal inside flow collections (YAML 1.2 §8.1).
            // A document marker must appear at the very beginning of a line
            // (column 0) and be followed by whitespace or end-of-line.
            if pos_in_line == 0
                && (cur_content.starts_with("---") || cur_content.starts_with("..."))
            {
                let rest = &cur_content[3..];
                if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                    let err_pos = abs_pos(cur_base_pos, cur_content, 0);
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "document marker is not allowed inside a flow collection".into(),
                    }));
                }
            }

            // Tabs as indentation on a new line in flow context are invalid
            // (YAML 1.2 §6.2 — indentation uses spaces only).  A tab at the
            // start of a continuation line (before the first non-whitespace
            // character) is a tab used as indentation.  Blank lines (tab only,
            // no content) are exempt — they are treated as empty separator lines.
            if pos_in_line == 0 {
                let has_tab_indent =
                    cur_content.starts_with('\t') && !cur_content.trim().is_empty();
                if has_tab_indent {
                    let err_pos = abs_pos(cur_base_pos, cur_content, 0);
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "tab character is not allowed as indentation in flow context"
                            .into(),
                    }));
                }
            }

            // Skip leading spaces/tabs and comments.
            // `#` is a comment start only when preceded by whitespace (or at
            // start of line, i.e. pos_in_line == 0 with all prior chars being
            // whitespace).  A `#` immediately after a token (e.g. `,#`) is not
            // a comment — it is an error character that will be caught below.
            let prev_was_ws_at_loop_entry = pos_in_line == 0
                || cur_content[..pos_in_line]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c == ' ' || c == '\t');
            let mut prev_was_ws = prev_was_ws_at_loop_entry;
            while pos_in_line < cur_content.len() {
                let Some(ch) = cur_content[pos_in_line..].chars().next() else {
                    break;
                };
                if ch == ' ' || ch == '\t' {
                    prev_was_ws = true;
                    pos_in_line += 1;
                } else if ch == '#' && prev_was_ws {
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

                // Flow continuation lines must be indented more than the
                // enclosing block context (YAML 1.2: flow lines must not
                // regress to the block indent level).  Blank/whitespace-only
                // lines are exempt — they act as line separators.
                // At document root (no enclosing block), there is no
                // indentation constraint.
                if let Some(min_indent) = flow_min_indent {
                    if let Some(next_line) = self.lexer.peek_next_line() {
                        let trimmed = next_line.content.trim();
                        if !trimmed.is_empty() && next_line.indent <= min_indent {
                            let err_pos = next_line.pos;
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "flow collection continuation line is not indented enough"
                                    .into(),
                            }));
                        }
                    }
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
                    flow_stack.push(FlowFrame::Sequence {
                        has_value: false,
                        after_colon: false,
                        last_was_plain: false,
                    });
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
                        last_was_plain: false,
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
                    // Update the last-token-line tracker so the multi-line implicit
                    // key check (DK4H) knows where the key (inner collection) ended.
                    last_token_line = cur_base_pos.line;
                    match parent {
                        FlowFrame::Sequence {
                            has_value,
                            after_colon,
                            last_was_plain,
                        } => {
                            *has_value = true;
                            *after_colon = false;
                            *last_was_plain = false;
                        }
                        FlowFrame::Mapping {
                            phase,
                            has_value,
                            last_was_plain,
                        } => {
                            *has_value = true;
                            *last_was_plain = false;
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
                    let tail_trimmed = tail_content.trim_start_matches([' ', '\t']);
                    // `#` is a comment only when preceded by whitespace.  If the
                    // closing bracket is immediately followed by `#` (no space),
                    // that is not a valid comment — it is a syntax error.
                    if tail_trimmed.starts_with('#') {
                        let prev_was_ws = pos_in_line == 0
                            || cur_content[..pos_in_line]
                                .chars()
                                .next_back()
                                .is_some_and(|c| c == ' ' || c == '\t');
                        if !prev_was_ws {
                            let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "comment requires at least one space before '#'".into(),
                            }));
                        }
                    }
                    // A flow collection used as an implicit mapping key must
                    // fit on a single line (YAML 1.2 §7.4.2).  If the tail
                    // begins with `:` (making this collection a mapping key) and
                    // the closing delimiter is on a different line than the
                    // opening delimiter, reject as a multi-line flow key.
                    if tail_trimmed.starts_with(':') && cur_base_pos.line != start_line {
                        let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        self.failed = true;
                        return StepResult::Yield(Err(Error {
                            pos: err_pos,
                            message: "multi-line flow collection cannot be used as an implicit mapping key".into(),
                        }));
                    }
                    // If the block collection stack is empty AND the tail does not
                    // start with `:` (which would indicate this flow collection is a
                    // mapping key), the flow collection is the document root node.
                    // Mark it so subsequent content on the NEXT LINE triggers the
                    // root-node guard in `step_in_document`.
                    if self.coll_stack.is_empty() && !tail_trimmed.starts_with(':') {
                        self.root_node_emitted = true;
                    }
                    self.lexer.consume_line();
                    if !tail_trimmed.is_empty() {
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
                        FlowFrame::Sequence { has_value, .. }
                        | FlowFrame::Mapping { has_value, .. },
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

                // If a tag or anchor is pending but no scalar was emitted yet,
                // the comma terminates an implicit empty-scalar node.  Emit it
                // so the pending properties are attached to the correct node
                // rather than carried forward to the next entry.
                if pending_flow_tag.is_some() || pending_flow_anchor.is_some() {
                    let empty_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    events.push((
                        Event::Scalar {
                            value: Cow::Borrowed(""),
                            style: ScalarStyle::Plain,
                            anchor: pending_flow_anchor.take(),
                            tag: pending_flow_tag.take(),
                        },
                        zero_span(empty_pos),
                    ));
                    // Advance phase: this scalar acts as a value (or key).
                    if let Some(frame) = flow_stack.last_mut() {
                        match frame {
                            FlowFrame::Sequence {
                                has_value,
                                after_colon,
                                last_was_plain,
                            } => {
                                *has_value = true;
                                *after_colon = false;
                                *last_was_plain = false;
                            }
                            FlowFrame::Mapping {
                                phase,
                                has_value,
                                last_was_plain,
                            } => {
                                *has_value = true;
                                *last_was_plain = false;
                                *phase = match *phase {
                                    FlowMappingPhase::Key => FlowMappingPhase::Value,
                                    FlowMappingPhase::Value => FlowMappingPhase::Key,
                                };
                            }
                        }
                    }
                }

                // Reset has_value and (for mappings) go back to Key phase.
                if let Some(frame) = flow_stack.last_mut() {
                    match frame {
                        FlowFrame::Sequence {
                            has_value,
                            after_colon,
                            last_was_plain,
                        } => {
                            *has_value = false;
                            *after_colon = false;
                            *last_was_plain = false;
                        }
                        FlowFrame::Mapping {
                            phase,
                            has_value,
                            last_was_plain,
                        } => {
                            *has_value = false;
                            *last_was_plain = false;
                            if *phase == FlowMappingPhase::Value {
                                *phase = FlowMappingPhase::Key;
                            }
                        }
                    }
                }
                // Reset last_token_line after a comma — the next key can start
                // on the same line as the comma (or any subsequent line) without
                // triggering the multi-line implicit key error.
                last_token_line = cur_base_pos.line;

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
                    // Flow context: no block-indentation constraint on
                    // continuation lines of double-quoted scalars.
                    self.lexer.try_consume_double_quoted(None)
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

                // Reconstruct the tail after the closing quote so the flow
                // parser can continue with `,`, `]`, `}`, etc.
                //
                // For single-line scalars, the tail is in `remaining` at byte
                // offset `span.end.byte_offset - cur_abs_pos.byte_offset`.
                //
                // For multiline scalars, the lexer's continuation loop consumed
                // additional input lines; the tail on the closing-quote line is
                // stored in `self.lexer.pending_multiline_tail`.  Drain it here.
                if let Some((tail, tail_pos)) = self.lexer.pending_multiline_tail.take() {
                    if !tail.is_empty() {
                        let tail_syn = crate::lines::Line {
                            content: tail,
                            offset: tail_pos.byte_offset,
                            indent: tail_pos.column,
                            break_type: crate::lines::BreakType::Eof,
                            pos: tail_pos,
                        };
                        self.lexer.prepend_inline_line(tail_syn);
                    }
                } else {
                    // Single-line scalar: derive tail from `remaining`.
                    let consumed_bytes = span.end.byte_offset - cur_abs_pos.byte_offset;
                    let tail_in_remaining = remaining.get(consumed_bytes..).unwrap_or("");
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
                }

                // Re-sync from the buffer.
                (cur_content, cur_base_pos) = resync!();
                pos_in_line = 0;
                // Track where this quoted scalar (potential key) ended.
                last_token_line = cur_base_pos.line;

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
                        FlowFrame::Sequence {
                            has_value,
                            after_colon,
                            last_was_plain,
                        } => {
                            *has_value = true;
                            *after_colon = false;
                            *last_was_plain = false;
                        }
                        FlowFrame::Mapping {
                            phase,
                            has_value,
                            last_was_plain,
                        } => {
                            *has_value = true;
                            *last_was_plain = false;
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
            // Explicit key indicator `?` in flow mappings and sequences
            // ----------------------------------------------------------------
            if ch == '?' {
                let next_ch = cur_content[pos_in_line + 1..].chars().next();
                if next_ch.is_none_or(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
                    // `?` followed by whitespace/EOL: explicit key indicator.
                    // In a flow sequence, remember this so the DK4H single-line
                    // check is suppressed for the corresponding `:` separator.
                    if matches!(flow_stack.last(), Some(FlowFrame::Sequence { .. })) {
                        explicit_key_in_seq = true;
                    }
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
                // `:` is a value separator when followed by whitespace/delimiter
                // (standard case) OR when in a flow sequence with a synthetic
                // current line (adjacent `:` from JSON-like key — YAML 1.2
                // §7.4.2).  A synthetic line means the `:` is on the same
                // physical line as the preceding quoted scalar / collection.
                let is_standard_sep =
                    next_ch.is_none_or(|c| matches!(c, ' ' | '\t' | ',' | ']' | '}' | '\n' | '\r'));
                let is_adjacent_json_sep = !is_standard_sep
                    && matches!(
                        flow_stack.last(),
                        Some(FlowFrame::Sequence {
                            has_value: true,
                            ..
                        })
                    )
                    && self.lexer.is_next_line_synthetic();
                let is_value_sep = is_standard_sep || is_adjacent_json_sep;
                if is_value_sep {
                    // Multi-line implicit single-pair mapping key check (YAML 1.2 §7.4.1):
                    // inside a flow sequence `[...]`, a single-pair mapping entry's key must
                    // be on the same line as the `:` separator.  (Flow mappings `{...}` allow
                    // multi-line implicit keys — see YAML 1.2 §7.4.2.)
                    // Exception: when a `?` explicit-key indicator was seen in this sequence
                    // (`explicit_key_in_seq`), the key may span multiple lines.
                    let in_sequence = matches!(flow_stack.last(), Some(FlowFrame::Sequence { .. }));
                    if in_sequence && cur_base_pos.line != last_token_line && !explicit_key_in_seq {
                        let colon_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        self.failed = true;
                        return StepResult::Yield(Err(Error {
                            pos: colon_pos,
                            message: "implicit flow mapping key must be on a single line".into(),
                        }));
                    }
                    explicit_key_in_seq = false;
                    if let Some(frame) = flow_stack.last_mut() {
                        match frame {
                            FlowFrame::Mapping {
                                phase,
                                has_value,
                                last_was_plain,
                            } => {
                                *last_was_plain = false;
                                if *phase == FlowMappingPhase::Key {
                                    // If a tag or anchor is pending but no key scalar was
                                    // emitted yet, the `:` terminates an implicit empty key.
                                    // Emit the empty key scalar now so the pending properties
                                    // are attached to the key, not carried to the value.
                                    if pending_flow_tag.is_some() || pending_flow_anchor.is_some() {
                                        let key_pos =
                                            abs_pos(cur_base_pos, cur_content, pos_in_line);
                                        events.push((
                                            Event::Scalar {
                                                value: Cow::Borrowed(""),
                                                style: ScalarStyle::Plain,
                                                anchor: pending_flow_anchor.take(),
                                                tag: pending_flow_tag.take(),
                                            },
                                            zero_span(key_pos),
                                        ));
                                        *has_value = true;
                                    }
                                    *phase = FlowMappingPhase::Value;
                                }
                            }
                            FlowFrame::Sequence {
                                after_colon,
                                last_was_plain,
                                ..
                            } => {
                                // `:` as value separator in a sequence means we are
                                // entering the value part of a single-pair implicit
                                // mapping.  Mark `after_colon` so the next scalar or
                                // collection is not rejected for missing a comma.
                                *after_colon = true;
                                // Reset last_was_plain so the value scalar on the next
                                // line is not appended to the key via multi-line
                                // plain-scalar continuation logic.
                                *last_was_plain = false;
                            }
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
                        // Two anchors on the same flow node are an error.
                        if pending_flow_anchor.is_some() {
                            let amp_pos2 = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: amp_pos2,
                                message: "a node may not have more than one anchor".into(),
                            }));
                        }
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
                if pending_flow_anchor.is_some() {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have an anchor property".into(),
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
                                FlowFrame::Sequence {
                                    has_value,
                                    after_colon,
                                    last_was_plain,
                                } => {
                                    *has_value = true;
                                    *after_colon = false;
                                    *last_was_plain = false;
                                }
                                FlowFrame::Mapping {
                                    phase,
                                    has_value,
                                    last_was_plain,
                                } => {
                                    *has_value = true;
                                    *last_was_plain = false;
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
            // Multi-line plain scalar continuation in flow context
            //
            // A plain scalar may span multiple lines (YAML §7.3.3).  When the
            // previous emitted token was a plain scalar (`last_was_plain`) and
            // the current character is a valid `ns-plain-char` (i.e. it can
            // appear within a plain scalar body, even if it cannot *start* one),
            // extend the in-progress scalar rather than treating the character
            // as the start of a new token.
            //
            // `ns-plain-char` in flow context: any `ns-char` that is not `:` or
            // `#`, plus `:` followed by ns-plain-safe, plus `#` not preceded by
            // whitespace.  At the start of a continuation line all leading
            // whitespace has been consumed, so `#` at position 0 here would be
            // `#` after whitespace — a comment start, not a continuation char.
            // ----------------------------------------------------------------
            {
                // For flow MAPPINGS: a plain scalar may continue a key only when
                // the phase is currently Value — meaning the previous scalar was
                // a KEY (Key→Value phase advance was done when emitting it).  A
                // VALUE scalar (phase Value→Key) must NOT continue: the next line
                // is a new key that requires a preceding comma.
                // For flow SEQUENCES: `last_was_plain` alone is enough (single-pair
                // implicit mapping keys can span lines, and regular sequence items
                // can also continue, though commas terminate them).
                let frame_last_was_plain = matches!(
                    flow_stack.last(),
                    Some(
                        FlowFrame::Mapping {
                            last_was_plain: true,
                            phase: FlowMappingPhase::Value,
                            ..
                        } | FlowFrame::Sequence {
                            last_was_plain: true,
                            ..
                        }
                    )
                );
                // `ns-plain-char` check: ch must not be a flow terminator, `:` (alone),
                // or `#` (comment start after whitespace, which is the only `#` we can
                // see here since whitespace was consumed).
                let is_ns_plain_char_continuation = frame_last_was_plain
                    && !matches!(ch, ',' | '[' | ']' | '{' | '}' | '#')
                    && (ch != ':' || {
                        let after = &cur_content[pos_in_line + 1..];
                        let next_c = after.chars().next();
                        // `:` is a valid continuation char only when NOT followed by
                        // a separator (space, tab, flow indicator, or end-of-line).
                        next_c.is_some_and(|nc| {
                            !matches!(nc, ' ' | '\t' | ',' | '[' | ']' | '{' | '}')
                        })
                    });

                if is_ns_plain_char_continuation {
                    let slice = &cur_content[pos_in_line..];
                    let scanned = scan_plain_line_flow(slice);
                    if !scanned.is_empty() {
                        // Extend the most-recently-emitted scalar event with a
                        // line-fold (space) and the continuation content.
                        if let Some((
                            Event::Scalar {
                                value,
                                style: ScalarStyle::Plain,
                                ..
                            },
                            _,
                        )) = events.last_mut()
                        {
                            let extended = format!("{value} {scanned}");
                            *value = Cow::Owned(extended);
                        }
                        pos_in_line += scanned.len();
                        // Update last_token_line to this line so the DK4H
                        // multi-line implicit-key check remains anchored to the
                        // last real token (the continuation content).
                        last_token_line = cur_base_pos.line;
                        // The continuation may itself end at EOL, leaving the scalar
                        // still incomplete.  Keep `last_was_plain` true and, for
                        // mappings, revert the phase back to Key so that the `: `
                        // separator is still recognised.
                        if let Some(frame) = flow_stack.last_mut() {
                            match frame {
                                FlowFrame::Mapping {
                                    phase,
                                    last_was_plain,
                                    ..
                                } => {
                                    // Undo the premature Key→Value advance: the key is not
                                    // yet complete until `: ` is seen.
                                    *phase = FlowMappingPhase::Key;
                                    *last_was_plain = true;
                                }
                                FlowFrame::Sequence { last_was_plain, .. } => {
                                    *last_was_plain = true;
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
                    // Missing-comma check: in a flow collection with has_value=true,
                    // a new plain scalar is starting without a preceding comma —
                    // YAML 1.2 §7.4 requires commas between entries.
                    match flow_stack.last() {
                        Some(FlowFrame::Mapping {
                            phase: FlowMappingPhase::Key,
                            has_value: true,
                            ..
                        }) => {
                            let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "missing comma between flow mapping entries".into(),
                            }));
                        }
                        Some(FlowFrame::Sequence {
                            has_value: true,
                            after_colon: false,
                            last_was_plain: false,
                        }) => {
                            let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.failed = true;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "missing comma between flow sequence entries".into(),
                            }));
                        }
                        _ => {}
                    }
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
                        // Track where this scalar (potential key) ended for the
                        // multi-line implicit key check (DK4H).
                        last_token_line = cur_base_pos.line;

                        // Advance mapping phase; mark frame as having a value.
                        if let Some(frame) = flow_stack.last_mut() {
                            match frame {
                                FlowFrame::Sequence {
                                    has_value,
                                    after_colon,
                                    last_was_plain,
                                } => {
                                    *has_value = true;
                                    *after_colon = false;
                                    *last_was_plain = true; // plain scalars may continue
                                }
                                FlowFrame::Mapping {
                                    phase,
                                    has_value,
                                    last_was_plain,
                                } => {
                                    *has_value = true;
                                    *last_was_plain = true; // plain scalars may continue on next line
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
        // A scalar was consumed — clear any pending explicit-key context.
        self.explicit_key_pending = false;
        // Find the innermost mapping entry on the stack.
        for entry in self.coll_stack.iter_mut().rev() {
            if let CollectionEntry::Mapping(_, phase, has_had_value) = entry {
                *phase = match *phase {
                    MappingPhase::Key => {
                        *has_had_value = true;
                        MappingPhase::Value
                    }
                    MappingPhase::Value => MappingPhase::Key,
                };
                return;
            }
            // Sequences between this mapping and the top don't count.
            if let CollectionEntry::Sequence(_, has_had_item) = entry {
                // A scalar here is an item in a sequence, not a mapping value.
                // Mark the sequence as having a completed item.
                *has_had_item = true;
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
