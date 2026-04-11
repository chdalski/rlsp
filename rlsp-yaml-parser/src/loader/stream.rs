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
