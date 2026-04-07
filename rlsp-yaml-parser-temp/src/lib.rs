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
    /// `StreamStart` emitted; scanning the body.
    InStream,
    /// `StreamEnd` emitted; done.
    Done,
}

/// Lazy iterator that yields events by walking a [`LineBuffer`].
struct EventIter<'input> {
    buf: LineBuffer<'input>,
    state: IterState,
}

impl<'input> EventIter<'input> {
    fn new(input: &'input str) -> Self {
        Self {
            buf: LineBuffer::new(input),
            state: IterState::BeforeStream,
        }
    }

    /// Drain remaining lines and return the position after the last byte.
    ///
    /// Called once when transitioning to `StreamEnd`.  For whitespace-only
    /// input the lines consumed here are the whitespace lines.  For empty
    /// input the buffer is already at EOF so the loop body never executes
    /// and `Pos::ORIGIN` is returned.
    fn drain_to_end(&mut self) -> Pos {
        let mut pos = Pos::ORIGIN;
        while let Some(line) = self.buf.consume_next() {
            for ch in line.content.chars() {
                pos = pos.advance(ch);
            }
            match line.break_type {
                BreakType::Lf => pos = pos.advance('\n'),
                BreakType::CrLf => {
                    pos.byte_offset += '\r'.len_utf8();
                    pos.char_offset += 1;
                    pos = pos.advance('\n');
                }
                BreakType::Cr => {
                    pos.byte_offset += '\r'.len_utf8();
                    pos.char_offset += 1;
                    pos.line += 1;
                    pos.column = 0;
                }
                BreakType::Eof => {}
            }
        }
        pos
    }
}

impl<'input> Iterator for EventIter<'input> {
    type Item = Result<(Event<'input>, Span), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            IterState::BeforeStream => {
                self.state = IterState::InStream;
                let span = Span {
                    start: Pos::ORIGIN,
                    end: Pos::ORIGIN,
                };
                Some(Ok((Event::StreamStart, span)))
            }
            IterState::InStream => {
                // For Tasks 5-18: grammar dispatch lives here.
                // For now, drain remaining whitespace/comment lines and emit
                // StreamEnd.
                let end = self.drain_to_end();
                self.state = IterState::Done;
                let span = Span { start: end, end };
                Some(Ok((Event::StreamEnd, span)))
            }
            IterState::Done => None,
        }
    }
}
