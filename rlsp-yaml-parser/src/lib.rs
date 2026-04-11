// SPDX-License-Identifier: MIT
#![deny(clippy::panic)]

mod chars;
mod directive_scope;
pub mod encoding;
mod error;
mod event;
mod event_iter;
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
    CollectionEntry, ConsumedMapping, IterState, MappingPhase, PendingAnchor, PendingTag,
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
    /// When a tag or anchor appears inline on a physical line (e.g. `!!str &a key:`),
    /// the key content is prepended as a synthetic line with the key's column as its
    /// indent.  This field records the indent of the ORIGINAL physical line so that
    /// `handle_mapping_entry` can open the mapping at the correct (original) indent
    /// rather than the synthetic line's offset.
    property_origin_indent: Option<usize>,
}

impl<'input> EventIter<'input> {
    /// Current combined collection depth (sequences + mappings).
    const fn collection_depth(&self) -> usize {
        self.coll_stack.len()
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
}

use mapping::{
    find_value_indicator_offset, inline_contains_mapping_key, is_implicit_mapping_line,
    is_tab_indented_block_indicator,
};
use properties::{scan_anchor_name, scan_tag};

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

impl<'input> EventIter<'input> {
    /// Handle one iteration step in the `InDocument` state.
    #[allow(clippy::too_many_lines)]
    fn step_in_document(&mut self) -> StepResult<'input> {
        match self.skip_and_collect_comments_in_doc() {
            Ok(()) => {}
            Err(e) => {
                self.state = IterState::Done;
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
                    self.state = IterState::Done;
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
                self.state = IterState::Done;
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
                self.state = IterState::Done;
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
                        self.state = IterState::Done;
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
                    self.state = IterState::Done;
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
                self.state = IterState::Done;
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
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have a tag property".into(),
                    }));
                }
                // An Inline anchor preceding `*alias` is an error — it would annotate
                // the alias node, which is illegal.  A Standalone anchor belongs to
                // the surrounding collection, not the alias, so it is not an error here.
                if matches!(self.pending_anchor, Some(PendingAnchor::Inline(_))) {
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have an anchor property".into(),
                    }));
                }
                match scan_anchor_name(after_star, star_pos) {
                    Err(e) => {
                        self.state = IterState::Done;
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
                        self.state = IterState::Done;
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
                                self.state = IterState::Done;
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
                        // (Standalone variant) and the new tag has inline content
                        // that is (or contains) a mapping key line, they apply to
                        // different nodes (collection vs. key scalar).
                        if self.pending_tag.is_some() {
                            let is_different_node =
                                matches!(self.pending_tag, Some(PendingTag::Standalone(_)))
                                    && had_inline
                                    && inline_contains_mapping_key(inline);
                            if !is_different_node {
                                self.state = IterState::Done;
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
                                    self.state = IterState::Done;
                                    return StepResult::Yield(Err(e));
                                }
                            };
                        self.lexer.consume_line();
                        if had_inline {
                            self.pending_tag = Some(PendingTag::Inline(resolved_tag));
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
                                self.state = IterState::Done;
                                return StepResult::Yield(Err(Error {
                                    pos: bang_pos,
                                    message:
                                        "node property is not indented enough for this context"
                                            .into(),
                                }));
                            }
                            self.pending_tag = Some(PendingTag::Standalone(resolved_tag));
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
                        self.state = IterState::Done;
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
                        let has_standalone_tag =
                            matches!(self.pending_tag, Some(PendingTag::Standalone(_)));
                        let is_duplicate =
                            if matches!(self.pending_anchor, Some(PendingAnchor::Inline(_)))
                                && !has_standalone_tag
                            {
                                true
                            } else if matches!(
                                self.pending_anchor,
                                Some(PendingAnchor::Standalone(_))
                            ) && had_inline
                                && !has_standalone_tag
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
                            self.state = IterState::Done;
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
                                self.state = IterState::Done;
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
                                self.state = IterState::Done;
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
                self.state = IterState::Done;
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
                    self.state = IterState::Done;
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
                    self.state = IterState::Done;
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
                    self.state = IterState::Done;
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
                self.state = IterState::Done;
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
                    self.state = IterState::Done;
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
                self.state = IterState::Done;
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
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: dash_pos,
                        message: "block sequence entry at wrong indentation level".into(),
                    }));
                }
            }
            if self.collection_depth() >= MAX_COLLECTION_DEPTH {
                self.state = IterState::Done;
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
                    tag: self.pending_tag.take().map(PendingTag::into_cow),
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
            && (matches!(self.pending_tag, Some(PendingTag::Standalone(_)))
                || matches!(self.pending_anchor, Some(PendingAnchor::Standalone(_))))
            && (self.pending_tag.is_some() || self.pending_anchor.is_some())
        {
            let item_pos = self.lexer.current_pos();
            self.queue.push_back((
                Event::Scalar {
                    value: std::borrow::Cow::Borrowed(""),
                    style: ScalarStyle::Plain,
                    anchor: self.pending_anchor.take().map(PendingAnchor::name),
                    tag: self.pending_tag.take().map(PendingTag::into_cow),
                },
                zero_span(item_pos),
            ));
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
                    self.state = IterState::Done;
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
                    self.state = IterState::Done;
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
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: key_pos,
                        message: "wrong indentation: mapping key is more indented than the enclosing mapping".into(),
                    }));
                }
                _ => {}
            }
            if self.collection_depth() >= MAX_COLLECTION_DEPTH {
                self.state = IterState::Done;
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
            let mapping_tag = if matches!(self.pending_tag, Some(PendingTag::Standalone(_))) {
                self.pending_tag.take().map(PendingTag::into_cow)
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
                && !matches!(self.pending_tag, Some(PendingTag::Standalone(_)))
                && !matches!(self.pending_anchor, Some(PendingAnchor::Standalone(_)))
                && (self.pending_tag.is_some() || self.pending_anchor.is_some())
            {
                let pos = self.lexer.current_pos();
                self.queue.push_back((
                    Event::Scalar {
                        value: std::borrow::Cow::Borrowed(""),
                        style: ScalarStyle::Plain,
                        anchor: self.pending_anchor.take().map(PendingAnchor::name),
                        tag: self.pending_tag.take().map(PendingTag::into_cow),
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
                            self.state = IterState::Done;
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
                        self.state = IterState::Done;
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
                            tag: self.pending_tag.take().map(PendingTag::into_cow),
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
                        tag: self.pending_tag.take().map(PendingTag::into_cow),
                    },
                    key_span,
                ));
                self.advance_mapping_to_value();
            }
            ConsumedMapping::QuotedKeyError { pos, message } => {
                self.state = IterState::Done;
                return StepResult::Yield(Err(Error { pos, message }));
            }
            ConsumedMapping::InlineImplicitMappingError { pos } => {
                // The inline value is a block node (mapping or sequence indicator)
                // which cannot appear inline as a mapping value — block nodes must
                // start on a new line.
                self.state = IterState::Done;
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
