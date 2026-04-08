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
// Depth limit (security: DoS via deeply nested sequences)
// ---------------------------------------------------------------------------

/// Maximum block-sequence nesting depth accepted from untrusted input.
///
/// Inputs that exceed this depth return an [`Error`] rather than consuming
/// unbounded memory.  512 is generous for all real-world YAML (Kubernetes /
/// Helm documents are typically under 20 levels deep) and small enough that
/// the explicit-stack overhead stays within a few KB.
pub const MAX_SEQUENCE_DEPTH: usize = 512;

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

/// Lazy iterator that yields events by walking a [`Lexer`].
struct EventIter<'input> {
    lexer: Lexer<'input>,
    state: IterState,
    /// Queued events to emit before resuming normal state dispatch.
    ///
    /// Used when a single parse step must produce multiple consecutive events —
    /// e.g. `SequenceStart` before the first item, or multiple `SequenceEnd`
    /// events when a dedent closes several nested sequences at once.
    queue: VecDeque<(Event<'input>, Span)>,
    /// Stack of block-sequence indent levels currently open.
    ///
    /// Each entry is the column of the `-` indicator that opened that sequence.
    /// When the next line's indent drops below an entry's value, that sequence
    /// is closed with a `SequenceEnd` event and the entry is popped.
    seq_stack: Vec<usize>,
    /// Set to `true` after an `Err` is yielded.
    ///
    /// Once set, `next()` immediately returns `None` to prevent infinite error
    /// loops (e.g. depth-limit firing on the same prepended synthetic line).
    failed: bool,
}

impl<'input> EventIter<'input> {
    fn new(input: &'input str) -> Self {
        Self {
            lexer: Lexer::new(input),
            state: IterState::BeforeStream,
            queue: VecDeque::new(),
            seq_stack: Vec::new(),
            failed: false,
        }
    }

    /// Push `SequenceEnd` events onto the queue for all open sequences whose
    /// dash-indent is `>= close_threshold`, from innermost to outermost.
    fn close_sequences_at_or_above(&mut self, close_threshold: usize, pos: Pos) {
        while let Some(&top) = self.seq_stack.last() {
            if top >= close_threshold {
                self.seq_stack.pop();
                self.queue.push_back((Event::SequenceEnd, zero_span(pos)));
            } else {
                break;
            }
        }
    }

    /// Push `SequenceEnd` events for all open sequences (document-end).
    fn close_all_sequences(&mut self, pos: Pos) {
        while self.seq_stack.pop().is_some() {
            self.queue.push_back((Event::SequenceEnd, zero_span(pos)));
        }
    }

    /// Check whether the next available line is a block-sequence entry indicator
    /// (`-` followed by space, tab, or end-of-content).
    ///
    /// Returns `(dash_indent, dash_pos)` where:
    /// - `dash_indent` is the effective document column of the `-` (from
    ///   `line.indent`, authoritative for both real and synthetic lines).
    /// - `dash_pos` is the absolute [`Pos`] of the `-` character — used to
    ///   attach correct span information to `SequenceStart` events.
    fn peek_sequence_entry(&self) -> Option<(usize, Pos)> {
        let line = self.lexer.peek_next_line()?;
        // Use line.indent as the authoritative column for the dash.
        let dash_indent = line.indent;
        // Strip leading spaces from content to reach the first non-space char.
        // For synthetic lines content already starts at `-` so trim is a no-op.
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

        // Compute the Pos of the `-` from `line.pos` (start of physical line)
        // plus the leading-spaces count.
        let leading_spaces = line.content.len() - trimmed.len();
        let dash_pos = Pos {
            byte_offset: line.pos.byte_offset + leading_spaces,
            char_offset: line.pos.char_offset + leading_spaces,
            line: line.pos.line,
            column: line.pos.column + leading_spaces,
        };
        Some((dash_indent, dash_pos))
    }

    /// Try to consume a scalar from the current lexer position.
    fn try_consume_scalar(
        &mut self,
        parent_indent: usize,
    ) -> Result<Option<(Event<'input>, Span)>, Error> {
        if let Some(result) = self.lexer.try_consume_literal_block_scalar(parent_indent) {
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
        if let Some(result) = self.lexer.try_consume_folded_block_scalar(parent_indent) {
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
        if let Some((value, span)) = self.lexer.try_consume_single_quoted(parent_indent)? {
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
        if let Some((value, span)) = self.lexer.try_consume_double_quoted(parent_indent)? {
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
        if let Some((value, span)) = self.lexer.try_consume_plain_scalar(parent_indent) {
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

    /// Consume the leading `-` indicator from the current line (which must be a
    /// sequence entry as verified by `peek_sequence_entry`).
    ///
    /// If the line has inline content after the dash (`- item`), a synthetic
    /// `Line` representing that inline content is prepended to the lexer buffer
    /// so the next iteration sees it as the next line to parse.
    ///
    /// Returns `true` if inline content was found and prepended, `false` if
    /// the item was empty (bare `-` with no following content).
    fn consume_sequence_dash(&mut self, dash_indent: usize) -> bool {
        // SAFETY: caller verified via peek_sequence_entry.
        let Some(line) = self.lexer.peek_next_line() else {
            unreachable!("consume_sequence_dash called at EOF")
        };

        let content = line.content;
        // Strip leading spaces to reach the `-` indicator.
        // For real lines this removes `dash_indent` spaces; for synthetic lines
        // the content already starts at `-` (no leading spaces) so trim is a no-op.
        let after_spaces = content.trim_start_matches(' ');
        debug_assert!(
            after_spaces.starts_with('-'),
            "sequence dash not at expected position"
        );
        // Content after the `-`.
        let rest_of_line = &after_spaces[1..]; // slice of input starting after '-'

        // Trim the leading space(s) after the dash to find the inline content.
        let inline = rest_of_line.trim_start_matches([' ', '\t']);
        let had_inline = !inline.is_empty();

        if had_inline {
            // Number of leading spaces we skipped to reach the `-`.
            // For real indented lines (e.g. `"  - foo"`) this equals `dash_indent`.
            // For synthetic lines `content` already starts at `-` so this is 0.
            let leading_spaces = content.len() - after_spaces.len();
            let spaces_after_dash = rest_of_line.len() - inline.len();
            let offset_from_dash = 1 + spaces_after_dash;
            // Total byte distance from `line.pos` (start of physical line) to
            // the inline content start.
            let total_offset = leading_spaces + offset_from_dash;
            // The effective document column of the inline content.
            let inline_col = dash_indent + offset_from_dash;
            let inline_pos = Pos {
                byte_offset: line.pos.byte_offset + total_offset,
                char_offset: line.pos.char_offset + total_offset,
                line: line.pos.line,
                column: line.pos.column + total_offset,
            };
            // `inline` has no leading spaces (they were trimmed above).
            // Set `indent` to `inline_col` so that dedent detection and
            // `peek_sequence_entry` use the correct document column.
            let synthetic = Line {
                content: inline,
                offset: inline_pos.byte_offset,
                indent: inline_col,
                break_type: line.break_type,
                pos: inline_pos,
            };
            // Consume the physical line first, then prepend the synthetic one.
            self.lexer.consume_line();
            self.lexer.prepend_inline_line(synthetic);
        } else {
            // No inline content — just consume the dash line.
            self.lexer.consume_line();
        }

        had_inline
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
            self.close_all_sequences(end);
            self.queue
                .push_back((Event::DocumentEnd { explicit: false }, zero_span(end)));
            self.queue.push_back((Event::StreamEnd, zero_span(end)));
            self.state = IterState::Done;
            return StepResult::Continue;
        }

        if self.lexer.is_document_end() {
            let pos = self.lexer.current_pos();
            self.close_all_sequences(pos);
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
            self.close_all_sequences(pos);
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

        // ---- Block sequence entry detection ----

        if let Some((dash_indent, dash_pos)) = self.peek_sequence_entry() {
            // Use current_pos (before consuming the line) for close-threshold
            // position — only needed for SequenceEnd spans on dedent.
            let cur_pos = self.lexer.current_pos();
            self.close_sequences_at_or_above(dash_indent.saturating_add(1), cur_pos);
            if !self.queue.is_empty() {
                return StepResult::Continue;
            }
            let opens_new = self.seq_stack.last().is_none_or(|&top| dash_indent > top);
            if opens_new {
                if self.seq_stack.len() >= MAX_SEQUENCE_DEPTH {
                    self.failed = true;
                    return StepResult::Yield(Err(Error {
                        pos: dash_pos,
                        message: "sequence nesting depth exceeds limit".into(),
                    }));
                }
                self.seq_stack.push(dash_indent);
                // Use the dash's actual position for the SequenceStart span.
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
                let item_pos = self.lexer.current_pos();
                self.queue.push_back((
                    Event::Scalar {
                        value: std::borrow::Cow::Borrowed(""),
                        style: ScalarStyle::Plain,
                        anchor: None,
                        tag: None,
                    },
                    zero_span(item_pos),
                ));
            }
            return StepResult::Continue;
        }

        // ---- Dedent: close sequences whose column >= current indent ----

        if let Some(line) = self.lexer.peek_next_line() {
            let line_indent = line.indent;
            let close_pos = self.lexer.current_pos();
            self.close_sequences_at_or_above(line_indent, close_pos);
            if !self.queue.is_empty() {
                return StepResult::Continue;
            }
        }

        // ---- Scalars ----

        // Use the current line's indent as parent_indent for plain scalar
        // continuation: continuation lines must be at strictly greater indent.
        // Synthetic inline lines have `indent` == the item's effective column,
        // so next-entry lines at indent 0 correctly fail the check.
        let parent_indent = self.lexer.peek_next_line().map_or(0, |l| l.indent);
        match self.try_consume_scalar(parent_indent) {
            Ok(Some(event)) => return StepResult::Yield(Ok(event)),
            Err(e) => {
                self.failed = true;
                return StepResult::Yield(Err(e));
            }
            Ok(None) => {}
        }

        // Fallback: unrecognised content line — consume and loop.
        // Task 12 will handle block mappings.
        self.lexer.consume_line();
        StepResult::Continue
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
