// SPDX-License-Identifier: MIT

mod chars;
mod error;
mod event;
mod lexer;
mod lines;
mod loader;
mod pos;
mod scanner;

pub use error::Error;
pub use event::Event;
pub use lines::{BreakType, Line, LineBuffer};
pub use pos::{Pos, Span};

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
// Iterator implementation
// ---------------------------------------------------------------------------

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
    /// A single pending event to emit before resuming normal state dispatch.
    ///
    /// Used when a boundary transition must produce two consecutive events
    /// across two calls to `next()` — e.g. implicit `DocumentEnd` followed
    /// by a new `DocumentStart` when `---` is seen inside `InDocument`.
    pending: Option<(Event<'input>, Span)>,
}

impl<'input> EventIter<'input> {
    fn new(input: &'input str) -> Self {
        Self {
            lexer: Lexer::new(input),
            state: IterState::BeforeStream,
            pending: None,
        }
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

impl<'input> Iterator for EventIter<'input> {
    type Item = Result<(Event<'input>, Span), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // Drain the pending slot first.
        if let Some(event) = self.pending.take() {
            return Some(Ok(event));
        }

        // Iterative dispatch — avoids unbounded recursion on large bare docs.
        loop {
            match self.state {
                // --------------------------------------------------------------
                IterState::BeforeStream => {
                    self.state = IterState::BetweenDocs;
                    return Some(Ok((Event::StreamStart, zero_span(Pos::ORIGIN))));
                }

                // --------------------------------------------------------------
                IterState::BetweenDocs => {
                    // Skip blank, comment, and directive lines.
                    self.lexer.skip_directives_and_blank_lines();

                    if self.lexer.at_eof() {
                        // No more documents — emit StreamEnd.
                        let end = self.lexer.current_pos();
                        self.state = IterState::Done;
                        return Some(Ok((Event::StreamEnd, zero_span(end))));
                    } else if self.lexer.is_directives_end() {
                        // Explicit document start (`---`).
                        let (marker_pos, _) = self.lexer.consume_marker_line();
                        self.state = IterState::InDocument;
                        return Some(Ok((
                            Event::DocumentStart { explicit: true },
                            marker_span(marker_pos),
                        )));
                    } else if self.lexer.is_document_end() {
                        // Orphan `...` before any open document — consume and
                        // loop back to BetweenDocs.  No event is emitted.
                        self.lexer.consume_marker_line();
                        // continue loop
                    } else {
                        // Non-blank, non-marker content: bare document begins.
                        debug_assert!(
                            self.lexer.has_content(),
                            "expected content after skipping blank/comment/directive lines"
                        );
                        // Emit DocumentStart{explicit:false} at the current
                        // content position; transition to InDocument.
                        let content_pos = self.lexer.current_pos();
                        self.state = IterState::InDocument;
                        return Some(Ok((
                            Event::DocumentStart { explicit: false },
                            zero_span(content_pos),
                        )));
                    }
                }

                // --------------------------------------------------------------
                IterState::InDocument => {
                    // Skip blank/comment lines inside the document body.
                    // (Directive lines inside a document are content, not
                    // directives — they are consumed here as regular lines.)
                    self.lexer.skip_empty_lines();

                    if self.lexer.at_eof() {
                        // Implicit document end at EOF.  `drain_to_end` is a
                        // no-op here (buffer is already empty) but confirms the
                        // final position and keeps the method reachable for
                        // future callers.
                        let end = self.lexer.drain_to_end();
                        self.state = IterState::Done;
                        self.pending = Some((Event::StreamEnd, zero_span(end)));
                        return Some(Ok((Event::DocumentEnd { explicit: false }, zero_span(end))));
                    } else if self.lexer.is_document_end() {
                        // Explicit document end via `...`.
                        let (marker_pos, _) = self.lexer.consume_marker_line();
                        self.state = IterState::BetweenDocs;
                        return Some(Ok((
                            Event::DocumentEnd { explicit: true },
                            marker_span(marker_pos),
                        )));
                    } else if self.lexer.is_directives_end() {
                        // New `---` inside a document: implicit end of the
                        // current document, then start of the new one.
                        let (marker_pos, _) = self.lexer.consume_marker_line();
                        // Queue the new DocumentStart for the next call.
                        self.pending = Some((
                            Event::DocumentStart { explicit: true },
                            marker_span(marker_pos),
                        ));
                        self.state = IterState::InDocument;
                        return Some(Ok((
                            Event::DocumentEnd { explicit: false },
                            zero_span(marker_pos),
                        )));
                    }
                    // Regular content line — consume and loop to process
                    // the next line in InDocument.  Scalar/mapping/sequence
                    // parsing is deferred to Tasks 6+.
                    self.lexer.consume_line();
                    // continue loop (no recursion)
                }

                // --------------------------------------------------------------
                IterState::Done => return None,
            }
        }
    }
}
