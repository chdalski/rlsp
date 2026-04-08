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

use std::collections::VecDeque;

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

/// An entry on the collection stack, tracking open sequences and mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionEntry {
    /// An open block sequence.  Holds the column of its `-` indicator.
    Sequence(usize),
    /// An open block mapping.  Holds the column of its first key and the
    /// current phase (expecting key or value).
    Mapping(usize, MappingPhase),
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
}

impl<'input> EventIter<'input> {
    fn new(input: &'input str) -> Self {
        Self {
            lexer: Lexer::new(input),
            state: IterState::BeforeStream,
            queue: VecDeque::new(),
            coll_stack: Vec::new(),
            failed: false,
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
                    self.queue.push_back((empty_scalar_event(), zero_span(pos)));
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
                    anchor: None,
                    tag: None,
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
                    anchor: None,
                    tag: None,
                },
                span,
            )));
        }
        if let Some((value, span)) = self.lexer.try_consume_single_quoted(plain_parent_indent)? {
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::SingleQuoted,
                    anchor: None,
                    tag: None,
                },
                span,
            )));
        }
        if let Some((value, span)) = self.lexer.try_consume_double_quoted(plain_parent_indent)? {
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::DoubleQuoted,
                    anchor: None,
                    tag: None,
                },
                span,
            )));
        }
        if let Some((value, span)) = self.lexer.try_consume_plain_scalar(plain_parent_indent) {
            return Ok(Some((
                Event::Scalar {
                    value,
                    style: ScalarStyle::Plain,
                    anchor: None,
                    tag: None,
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

impl<'input> EventIter<'input> {
    /// Handle one iteration step in the `BetweenDocs` state.
    fn step_between_docs(&mut self) -> StepResult<'input> {
        self.lexer.skip_directives_and_blank_lines();

        if self.lexer.at_eof() {
            let end = self.lexer.current_pos();
            self.state = IterState::Done;
            return StepResult::Yield(Ok((Event::StreamEnd, zero_span(end))));
        }
        if self.lexer.is_directives_end() {
            let (marker_pos, _) = self.lexer.consume_marker_line();
            self.state = IterState::InDocument;
            return StepResult::Yield(Ok((
                Event::DocumentStart { explicit: true },
                marker_span(marker_pos),
            )));
        }
        if self.lexer.is_document_end() {
            self.lexer.consume_marker_line();
            return StepResult::Continue; // orphan `...`, no event
        }
        debug_assert!(
            self.lexer.has_content(),
            "expected content after skipping blank/comment/directive lines"
        );
        let content_pos = self.lexer.current_pos();
        self.state = IterState::InDocument;
        StepResult::Yield(Ok((
            Event::DocumentStart { explicit: false },
            zero_span(content_pos),
        )))
    }

    /// Handle one iteration step in the `InDocument` state.
    fn step_in_document(&mut self) -> StepResult<'input> {
        self.lexer.skip_empty_lines();

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
            self.state = IterState::BetweenDocs;
            self.queue.push_back((
                Event::DocumentEnd { explicit: true },
                marker_span(marker_pos),
            ));
            return StepResult::Continue;
        }
        if self.lexer.is_directives_end() {
            let pos = self.lexer.current_pos();
            self.close_all_collections(pos);
            let (marker_pos, _) = self.lexer.consume_marker_line();
            self.state = IterState::InDocument;
            self.queue.push_back((
                Event::DocumentEnd { explicit: false },
                zero_span(marker_pos),
            ));
            self.queue.push_back((
                Event::DocumentStart { explicit: true },
                marker_span(marker_pos),
            ));
            return StepResult::Continue;
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
                    anchor: None,
                    tag: None,
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
                self.queue
                    .push_back((empty_scalar_event(), zero_span(item_pos)));
            }
        }
        StepResult::Continue
    }

    /// Handle a block-mapping key entry.
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
            self.queue.push_back((
                Event::MappingStart {
                    anchor: None,
                    tag: None,
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
            self.queue.push_back((empty_scalar_event(), zero_span(pos)));
            self.advance_mapping_to_key();
            return StepResult::Continue;
        }

        // Normal key line: consume and emit key scalar.
        let consumed = self.consume_mapping_entry(key_indent);
        match consumed {
            ConsumedMapping::ExplicitKey { had_key_inline } => {
                if !had_key_inline {
                    let pos = self.lexer.current_pos();
                    self.queue.push_back((empty_scalar_event(), zero_span(pos)));
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
                        anchor: None,
                        tag: None,
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
                    anchor: None,
                    tag: None,
                },
                zero_span(pos),
            ));
            self.advance_mapping_to_key();
        }
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
