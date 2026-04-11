// SPDX-License-Identifier: MIT

//! Block-sequence handlers.
//!
//! Contains `peek_sequence_entry`, `consume_sequence_dash`, and
//! `handle_sequence_entry`.

use crate::error::Error;
use crate::event::{CollectionStyle, Event, ScalarStyle};
use crate::event_iter::state::{
    CollectionEntry, IterState, MappingPhase, PendingAnchor, PendingTag, StepResult,
};
use crate::limits::MAX_COLLECTION_DEPTH;
use crate::lines::Line;
use crate::mapping::is_tab_indented_block_indicator;
use crate::pos::Pos;
use crate::{EventIter, zero_span};

impl<'input> EventIter<'input> {
    /// Check whether the next available line is a block-sequence entry
    /// indicator (`-` followed by space, tab, or end-of-content).
    ///
    /// Returns `(dash_indent, dash_pos)` where:
    /// - `dash_indent` is the effective document column of the `-`.
    /// - `dash_pos` is the absolute [`Pos`] of the `-` character.
    pub(crate) fn peek_sequence_entry(&self) -> Option<(usize, Pos)> {
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

    /// Consume the leading `-` indicator from the current line and (if
    /// present) prepend a synthetic line for the inline content.
    ///
    /// Returns `true` if inline content was found and prepended.
    pub(crate) fn consume_sequence_dash(&mut self, dash_indent: usize) -> bool {
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

    /// Handle a block-sequence dash entry (`-`).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn handle_sequence_entry(
        &mut self,
        dash_indent: usize,
        dash_pos: Pos,
    ) -> StepResult<'input> {
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
}
