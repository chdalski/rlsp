// SPDX-License-Identifier: MIT

use std::collections::VecDeque;

use crate::directive_scope::DirectiveScope;
use crate::error::Error;
use crate::event::{Event, ScalarStyle};
use crate::lexer::Lexer;
use crate::pos::{Pos, Span};
use crate::state::{
    CollectionEntry, IterState, MappingPhase, PendingAnchor, PendingTag, StepResult,
};
use crate::{EventIter, zero_span};

impl<'input> EventIter<'input> {
    pub(crate) fn new(input: &'input str) -> Self {
        Self {
            lexer: Lexer::new(input),
            state: IterState::BeforeStream,
            queue: VecDeque::new(),
            coll_stack: Vec::new(),
            pending_anchor: None,
            pending_tag: None,
            directive_scope: DirectiveScope::default(),
            root_node_emitted: false,
            explicit_key_pending: false,
            property_origin_indent: None,
        }
    }

    /// Push close events for all collections whose indent is `>= threshold`,
    /// from innermost to outermost.
    ///
    /// After each close, if the new top of the stack is a mapping in Value
    /// phase, flips it to Key phase — the closed collection was that
    /// mapping's value.
    pub(crate) fn close_collections_at_or_above(&mut self, threshold: usize, pos: Pos) {
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
    pub(crate) fn close_all_collections(&mut self, pos: Pos) {
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

    /// Try to consume a scalar from the current lexer position.
    ///
    /// `plain_parent_indent` — the indent of the current line; plain scalar
    /// continuation stops when the next line is less-indented than this.
    ///
    /// `block_parent_indent` — the indent of the enclosing block context;
    /// block scalars collect content that is more indented than this value.
    ///
    /// Consumes `self.pending_anchor` and attaches it to the emitted scalar.
    pub(crate) fn try_consume_scalar(
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
                    tag: self.pending_tag.take().map(PendingTag::into_cow),
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
                    tag: self.pending_tag.take().map(PendingTag::into_cow),
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
                    tag: self.pending_tag.take().map(PendingTag::into_cow),
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
                        return Err(Error {
                            pos: tail_pos,
                            message: "comment requires at least one space before '#'".into(),
                        });
                    } else if !first_non_ws.starts_with('#') {
                        // Non-comment content after quoted scalar.
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
                    tag: self.pending_tag.take().map(PendingTag::into_cow),
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
                    tag: self.pending_tag.take().map(PendingTag::into_cow),
                },
                span,
            )));
        }
        Ok(None)
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
    pub(crate) fn drain_trailing_comment(&mut self) {
        if let Some((text, span)) = self.lexer.trailing_comment.take() {
            self.queue.push_back((Event::Comment { text }, span));
        }
    }

    /// Returns the minimum column at which a standalone block-node property
    /// (anchor or tag on its own line) is valid in the current context.
    ///
    /// - Mapping in Value phase at indent `n`: the value node must be at col > n.
    /// - Sequence at indent `n`: item content must be at col > n.
    /// - Mapping in Key phase at indent `n`: a key at col `n` is valid.
    /// - Root (empty stack): any column is valid.
    pub(crate) fn min_standalone_property_indent(&self) -> usize {
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

impl<'input> Iterator for EventIter<'input> {
    type Item = Result<(Event<'input>, Span), Error>;

    fn next(&mut self) -> Option<Self::Item> {
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
