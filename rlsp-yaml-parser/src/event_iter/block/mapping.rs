// SPDX-License-Identifier: MIT

//! Block-mapping handlers.
//!
//! Contains `peek_mapping_entry`, `consume_mapping_entry`,
//! `handle_mapping_entry`, `advance_mapping_to_value`, `advance_mapping_to_key`,
//! `tick_mapping_phase_after_scalar`, `is_value_indicator_line`, and
//! `consume_explicit_value_line`.

use crate::error::Error;
use crate::event::{CollectionStyle, Event, ScalarStyle};
use crate::event_iter::line_mapping::{
    find_value_indicator_offset, is_implicit_mapping_line, is_tab_indented_block_indicator,
};
use crate::event_iter::state::{
    CollectionEntry, ConsumedMapping, IterState, MappingPhase, PendingAnchor, PendingTag,
    StepResult,
};
use crate::limits::MAX_COLLECTION_DEPTH;
use crate::lines::Line;
use crate::pos::{Pos, Span};
use crate::{EventIter, zero_span};

impl<'input> EventIter<'input> {
    /// Check whether the next available line looks like an implicit mapping
    /// key: a non-empty line whose plain-scalar content is followed by `: `
    /// (colon + space) or `:\n` (colon at end-of-line) or `:\t`.
    ///
    /// Also recognises the explicit key indicator `? ` at the start of a line.
    ///
    /// Returns `(key_indent, key_pos)` on success, where `key_indent` is the
    /// document column of the first character of the key (or `?` indicator),
    /// and `key_pos` is its absolute [`Pos`].
    pub(in crate::event_iter) fn peek_mapping_entry(&self) -> Option<(usize, Pos)> {
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
    #[expect(
        clippy::too_many_lines,
        reason = "match-on-event-type; splitting would obscure flow"
    )]
    pub(in crate::event_iter) fn consume_mapping_entry(
        &mut self,
        key_indent: usize,
    ) -> ConsumedMapping<'input> {
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
        let key_end_pos = crate::pos::advance_within_line(key_start_pos, key_content);
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
    pub(in crate::event_iter) fn advance_mapping_to_value(&mut self) {
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
    pub(in crate::event_iter) fn advance_mapping_to_key(&mut self) {
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

    /// Handle a block-mapping key entry.
    #[expect(
        clippy::too_many_lines,
        reason = "match-on-event-type; splitting would obscure flow"
    )]
    pub(in crate::event_iter) fn handle_mapping_entry(
        &mut self,
        key_indent: usize,
        key_pos: Pos,
    ) -> StepResult<'input> {
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
    pub(in crate::event_iter) fn is_value_indicator_line(&self) -> bool {
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
    pub(in crate::event_iter) fn consume_explicit_value_line(&mut self, key_indent: usize) {
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
    pub(in crate::event_iter) fn tick_mapping_phase_after_scalar(&mut self) {
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
