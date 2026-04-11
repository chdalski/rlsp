// SPDX-License-Identifier: MIT

use std::iter::Peekable;

use crate::error::Error;
use crate::event::Event;
use crate::pos::Span;

use super::{LoadError, Result};

type EventStream<'a> =
    Peekable<Box<dyn Iterator<Item = std::result::Result<(Event<'a>, Span), Error>> + 'a>>;

/// Pull the next event from the stream, converting parse errors to `LoadError`.
pub(super) fn next_from<'a>(stream: &mut EventStream<'a>) -> Result<Option<(Event<'a>, Span)>> {
    match stream.next() {
        None => Ok(None),
        Some(Ok(item)) => Ok(Some(item)),
        Some(Err(e)) => Err(LoadError::Parse {
            pos: e.pos,
            message: e.message,
        }),
    }
}

/// Consume leading block-level Comment events at document level, appending
/// them to `doc_comments`.  Stops at the first non-Comment event.
///
/// Block-level comments have `span.end.line > span.start.line`.
pub(super) fn consume_leading_doc_comments(
    stream: &mut EventStream<'_>,
    doc_comments: &mut Vec<String>,
) -> Result<()> {
    while matches!(stream.peek(), Some(Ok((Event::Comment { .. }, _)))) {
        if let Some((Event::Comment { text }, span)) = next_from(stream)? {
            if span.end.line > span.start.line {
                doc_comments.push(format!("#{text}"));
            }
        }
    }
    Ok(())
}

/// Consume leading block-level Comment events before a collection item or
/// mapping key.  Returns the captured comment texts.
///
/// A "leading" comment is any `Comment` event that appears before the next
/// non-comment structural event.  By the time this function is called,
/// `peek_trailing_comment` has already consumed any trailing comment that was
/// on the same line as the preceding value — so every remaining `Comment` here
/// is on its own line and belongs to the upcoming key/item as a leading comment.
pub(super) fn consume_leading_comments(stream: &mut EventStream<'_>) -> Result<Vec<String>> {
    let mut leading = Vec::new();
    while matches!(stream.peek(), Some(Ok((Event::Comment { .. }, _)))) {
        if let Some((Event::Comment { text }, _)) = next_from(stream)? {
            leading.push(format!("#{text}"));
        }
    }
    Ok(leading)
}

/// If the next event is a trailing Comment on the same line as `preceding_end_line`,
/// consume it and return the text.  Otherwise return `None`.
///
/// libfyaml (`fy_attach_comments_if_any` in `fy-parse.c`) uses the same
/// criterion: a comment is "trailing" when its line equals the preceding
/// token's end line (`fym.line == fyt->handle.end_mark.line`).  The new
/// parser emits trailing comments with real spans (not zero-width), so the
/// old `span.start == span.end` sentinel from the original parser does not
/// apply here.
pub(super) fn peek_trailing_comment(
    stream: &mut EventStream<'_>,
    preceding_end_line: usize,
) -> Result<Option<String>> {
    if matches!(
        stream.peek(),
        Some(Ok((Event::Comment { .. }, span))) if span.start.line == preceding_end_line
    ) {
        if let Some((Event::Comment { text }, _)) = next_from(stream)? {
            return Ok(Some(format!("#{text}")));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::event::Event;
    use crate::pos::{Pos, Span};

    fn make_stream<'a>(
        items: Vec<std::result::Result<(Event<'a>, Span), Error>>,
    ) -> EventStream<'a> {
        let boxed: Box<dyn Iterator<Item = _> + 'a> = Box::new(items.into_iter());
        boxed.peekable()
    }

    fn span(start_line: usize, end_line: usize) -> Span {
        Span {
            start: Pos {
                byte_offset: 0,
                line: start_line,
                column: 0,
            },
            end: Pos {
                byte_offset: 0,
                line: end_line,
                column: 0,
            },
        }
    }

    fn pos(line: usize) -> Pos {
        Pos {
            byte_offset: 0,
            line,
            column: 0,
        }
    }

    // -----------------------------------------------------------------------
    // next_from
    // -----------------------------------------------------------------------

    #[test]
    fn next_from_on_empty_stream_returns_ok_none() {
        let mut stream = make_stream(vec![]);
        assert_eq!(next_from(&mut stream), Ok(None));
    }

    #[test]
    fn next_from_forwards_ok_event() {
        let sp = span(1, 1);
        let mut stream = make_stream(vec![Ok((Event::StreamStart, sp))]);
        let result = next_from(&mut stream);
        assert_eq!(result, Ok(Some((Event::StreamStart, sp))));
    }

    #[test]
    fn next_from_propagates_parse_error_as_load_error() {
        let p = pos(3);
        let err = Error {
            pos: p,
            message: "unexpected token".to_string(),
        };
        let mut stream = make_stream(vec![Err(err)]);
        let result = next_from(&mut stream);
        assert_eq!(
            result,
            Err(LoadError::Parse {
                pos: p,
                message: "unexpected token".to_string(),
            })
        );
    }

    // -----------------------------------------------------------------------
    // consume_leading_doc_comments
    // -----------------------------------------------------------------------

    #[test]
    fn consume_leading_doc_comments_empty_when_first_event_is_not_comment() {
        let sp = span(1, 1);
        let mut stream = make_stream(vec![Ok((Event::StreamStart, sp))]);
        let mut doc_comments: Vec<String> = Vec::new();
        let result = consume_leading_doc_comments(&mut stream, &mut doc_comments);
        assert_eq!(result, Ok(()));
        assert!(doc_comments.is_empty());
        // stream not consumed — StreamStart still peeked
        assert!(stream.peek().is_some());
    }

    #[test]
    fn consume_leading_doc_comments_accumulates_block_comments() {
        // block-level: end.line > start.line
        let sp = span(1, 2);
        let trailing_sp = span(3, 3);
        let mut stream = make_stream(vec![
            Ok((Event::Comment { text: " note" }, sp)),
            Ok((Event::Comment { text: " note" }, sp)),
            Ok((Event::StreamStart, trailing_sp)),
        ]);
        let mut doc_comments: Vec<String> = Vec::new();
        assert_eq!(
            consume_leading_doc_comments(&mut stream, &mut doc_comments),
            Ok(())
        );
        assert_eq!(doc_comments, vec!["# note", "# note"]);
        // non-comment event still peeked
        assert!(matches!(stream.peek(), Some(Ok((Event::StreamStart, _)))));
    }

    #[test]
    fn consume_leading_doc_comments_skips_single_line_comment() {
        // inline: end.line == start.line
        let inline_sp = span(1, 1);
        let trailing_sp = span(2, 2);
        let mut stream = make_stream(vec![
            Ok((Event::Comment { text: " inline" }, inline_sp)),
            Ok((Event::StreamStart, trailing_sp)),
        ]);
        let mut doc_comments: Vec<String> = Vec::new();
        assert_eq!(
            consume_leading_doc_comments(&mut stream, &mut doc_comments),
            Ok(())
        );
        assert!(doc_comments.is_empty());
        // non-comment still peeked
        assert!(matches!(stream.peek(), Some(Ok((Event::StreamStart, _)))));
    }

    // -----------------------------------------------------------------------
    // consume_leading_comments
    // -----------------------------------------------------------------------

    #[test]
    fn consume_leading_comments_returns_empty_vec_when_first_event_is_not_comment() {
        let sp = span(1, 1);
        let mut stream = make_stream(vec![Ok((Event::StreamStart, sp))]);
        let result = consume_leading_comments(&mut stream);
        assert_eq!(result, Ok(vec![]));
        // stream untouched
        assert!(stream.peek().is_some());
    }

    #[test]
    fn consume_leading_comments_accumulates_all_comment_events() {
        let sp = span(1, 1);
        let trailing_sp = span(2, 2);
        let mut stream = make_stream(vec![
            Ok((Event::Comment { text: " a" }, sp)),
            Ok((Event::Comment { text: " b" }, sp)),
            Ok((Event::StreamStart, trailing_sp)),
        ]);
        let result = consume_leading_comments(&mut stream);
        assert_eq!(result, Ok(vec!["# a".to_string(), "# b".to_string()]));
        // non-comment still peeked
        assert!(matches!(stream.peek(), Some(Ok((Event::StreamStart, _)))));
    }

    #[test]
    fn consume_leading_comments_drains_all_consecutive_comments() {
        let sp = span(1, 1);
        let mut stream = make_stream(vec![
            Ok((Event::Comment { text: " x" }, sp)),
            Ok((Event::Comment { text: " y" }, sp)),
            Ok((Event::Comment { text: " z" }, sp)),
        ]);
        let result = consume_leading_comments(&mut stream);
        assert_eq!(
            result,
            Ok(vec![
                "# x".to_string(),
                "# y".to_string(),
                "# z".to_string()
            ])
        );
        // stream now empty
        assert!(stream.peek().is_none());
    }

    // -----------------------------------------------------------------------
    // peek_trailing_comment
    // -----------------------------------------------------------------------

    #[test]
    fn peek_trailing_comment_returns_none_when_next_is_not_comment() {
        let sp = span(1, 1);
        let mut stream = make_stream(vec![Ok((Event::StreamStart, sp))]);
        let result = peek_trailing_comment(&mut stream, 1);
        assert_eq!(result, Ok(None));
        // stream untouched
        assert!(stream.peek().is_some());
    }

    #[test]
    fn peek_trailing_comment_returns_none_when_stream_empty() {
        let mut stream = make_stream(vec![]);
        let result = peek_trailing_comment(&mut stream, 1);
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn peek_trailing_comment_returns_some_when_comment_on_same_line() {
        // comment span.start.line == preceding_end_line
        let sp = Span {
            start: Pos {
                byte_offset: 10,
                line: 5,
                column: 20,
            },
            end: Pos {
                byte_offset: 15,
                line: 5,
                column: 25,
            },
        };
        let mut stream = make_stream(vec![Ok((Event::Comment { text: " text" }, sp))]);
        let result = peek_trailing_comment(&mut stream, 5);
        assert_eq!(result, Ok(Some("# text".to_string())));
        // stream advanced — comment consumed
        assert!(stream.peek().is_none());
    }

    #[test]
    fn peek_trailing_comment_returns_none_when_comment_on_later_line() {
        // comment span.start.line > preceding_end_line
        let sp = Span {
            start: Pos {
                byte_offset: 0,
                line: 7,
                column: 0,
            },
            end: Pos {
                byte_offset: 5,
                line: 7,
                column: 5,
            },
        };
        let mut stream = make_stream(vec![Ok((Event::Comment { text: " later" }, sp))]);
        let result = peek_trailing_comment(&mut stream, 5);
        assert_eq!(result, Ok(None));
        // stream NOT consumed
        assert!(stream.peek().is_some());
    }
}
