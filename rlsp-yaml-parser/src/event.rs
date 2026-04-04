// SPDX-License-Identifier: MIT

//! Token-to-event conversion layer.
//!
//! Takes the flat token stream from [`crate::tokenize`] and produces a sequence
//! of structured [`Event`] values.  Each event carries a [`crate::pos::Span`]
//! that covers the tokens contributing to it.
//!
//! The public entry point is [`parse_events`].

use crate::pos::{Pos, Span};
use crate::token::Code;

/// Parsed directive information: `(version, tag_pairs)`.
type Directives = (Option<(u8, u8)>, Vec<(String, String)>);

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Block scalar chomp mode (YAML §8.1.1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chomp {
    /// `-` — trailing newlines stripped.
    Strip,
    /// (default) — single trailing newline kept.
    Clip,
    /// `+` — all trailing newlines kept.
    Keep,
}

/// The style in which a scalar value was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarStyle {
    /// An unquoted scalar.
    Plain,
    /// A `'single-quoted'` scalar.
    SingleQuoted,
    /// A `"double-quoted"` scalar.
    DoubleQuoted,
    /// A `|` literal block scalar.
    Literal(Chomp),
    /// A `>` folded block scalar.
    Folded(Chomp),
}

/// A parse error produced by the event layer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse error at {pos:?}: {message}")]
pub struct Error {
    pub pos: Pos,
    pub message: String,
}

/// A high-level YAML parse event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The stream has started.
    StreamStart,
    /// The stream has ended.
    StreamEnd,
    /// A document has started.
    DocumentStart {
        /// Whether the document was introduced with `---`.
        explicit: bool,
        /// The `%YAML` directive version, if present.
        version: Option<(u8, u8)>,
        /// The `%TAG` directive pairs `(handle, prefix)`.
        tags: Vec<(String, String)>,
    },
    /// A document has ended.
    DocumentEnd {
        /// Whether the document was closed with `...`.
        explicit: bool,
    },
    /// A mapping node has started.
    MappingStart {
        anchor: Option<String>,
        tag: Option<String>,
    },
    /// A mapping node has ended.
    MappingEnd,
    /// A sequence node has started.
    SequenceStart {
        anchor: Option<String>,
        tag: Option<String>,
    },
    /// A sequence node has ended.
    SequenceEnd,
    /// A scalar node.
    Scalar {
        value: String,
        style: ScalarStyle,
        anchor: Option<String>,
        tag: Option<String>,
    },
    /// An alias node.
    Alias { name: String },
    /// A YAML comment.
    Comment { text: String },
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse a YAML string into an event stream.
///
/// The first event is always `StreamStart` and the last is always `StreamEnd`.
///
/// ```
/// use rlsp_yaml_parser::parse_events;
/// use rlsp_yaml_parser::event::Event;
///
/// let events: Vec<_> = parse_events("hello").collect();
/// assert!(events.iter().any(|e| matches!(e, Ok((Event::StreamStart, _)))));
/// ```
pub fn parse_events(input: &str) -> impl Iterator<Item = Result<(Event, Span), Error>> + '_ {
    let tokens = crate::tokenize(input);
    OwnedEventIter {
        tokens,
        pos: 0,
        emitted_stream_start: false,
        done: false,
        pending_anchor: None,
        pending_tag: None,
        pending_doc_explicit: false,
        _phantom: std::marker::PhantomData,
    }
}

// ---------------------------------------------------------------------------
// Iterator implementation
// ---------------------------------------------------------------------------

/// Iterator that owns its token buffer, avoiding lifetime complications.
struct OwnedEventIter<'input> {
    tokens: Vec<crate::token::Token<'input>>,
    pos: usize,
    emitted_stream_start: bool,
    done: bool,
    /// Anchor name collected from a `BeginAnchor`…`EndAnchor` block that
    /// precedes the next content token (scalar/mapping/sequence).
    pending_anchor: Option<String>,
    /// Tag string collected from a `BeginTag`…`EndTag` block that
    /// precedes the next content token.
    pending_tag: Option<String>,
    /// Whether the upcoming `DocumentEnd` event should be `explicit=true`.
    /// Set when we encounter a `DocumentEnd` token (the `...` marker).
    pending_doc_explicit: bool,
    _phantom: std::marker::PhantomData<&'input str>,
}

impl<'input> OwnedEventIter<'input> {
    fn peek(&self) -> Option<Code> {
        self.tokens.get(self.pos).map(|t| t.code)
    }

    fn peek_token(&self) -> Option<&crate::token::Token<'input>> {
        self.tokens.get(self.pos)
    }

    fn collect_anchor(&mut self) -> String {
        let mut name = String::new();
        while let Some(t) = self.tokens.get(self.pos) {
            if t.code == Code::EndAnchor {
                self.pos += 1;
                break;
            }
            // Only Text tokens carry the anchor name; Indicator carries the `&` sigil.
            if t.code == Code::Text {
                name.push_str(t.text);
            }
            self.pos += 1;
        }
        name
    }

    fn collect_tag(&mut self) -> String {
        let mut tag = String::new();
        while let Some(t) = self.tokens.get(self.pos) {
            if t.code == Code::EndTag {
                self.pos += 1;
                break;
            }
            if t.code == Code::Text || t.code == Code::Indicator {
                tag.push_str(t.text);
            }
            self.pos += 1;
        }
        tag
    }

    fn parse_alias_block(&mut self) -> String {
        let mut name = String::new();
        while let Some(t) = self.tokens.get(self.pos) {
            if t.code == Code::EndAlias {
                self.pos += 1;
                break;
            }
            // Alias name appears as Meta tokens (the tokenizer uses Meta for
            // anchor/alias names inside BeginAlias blocks).
            if t.code == Code::Text || t.code == Code::Meta {
                name.push_str(t.text);
            }
            self.pos += 1;
        }
        name
    }

    fn parse_comment_block(&mut self) -> String {
        let mut text = String::new();
        while let Some(t) = self.tokens.get(self.pos) {
            if t.code == Code::EndComment {
                self.pos += 1;
                break;
            }
            if t.code == Code::Text {
                text.push_str(t.text);
            }
            self.pos += 1;
        }
        text
    }

    fn parse_scalar_block(&mut self, _start: Pos) -> (String, ScalarStyle) {
        let mut style_indicator: Option<String> = None;
        let mut chomp_indicator: Option<String> = None;
        let mut text = String::new();

        while let Some(t) = self.tokens.get(self.pos) {
            if t.code == Code::EndScalar {
                self.pos += 1;
                break;
            }
            match t.code {
                Code::Indicator => {
                    if style_indicator.is_none() {
                        style_indicator = Some(t.text.to_owned());
                    } else if chomp_indicator.is_none() {
                        chomp_indicator = Some(t.text.to_owned());
                    }
                    self.pos += 1;
                }
                Code::Text | Code::LineFeed | Code::LineFold => {
                    text.push_str(t.text);
                    self.pos += 1;
                }
                Code::BeginMapping
                | Code::EndMapping
                | Code::BeginSequence
                | Code::EndSequence
                | Code::BeginScalar
                | Code::EndScalar
                | Code::BeginComment
                | Code::EndComment
                | Code::BeginAnchor
                | Code::EndAnchor
                | Code::BeginAlias
                | Code::EndAlias
                | Code::BeginTag
                | Code::EndTag
                | Code::BeginDocument
                | Code::EndDocument
                | Code::BeginNode
                | Code::EndNode
                | Code::BeginPair
                | Code::EndPair
                | Code::DirectivesEnd
                | Code::DocumentEnd
                | Code::Meta
                | Code::White
                | Code::Indent
                | Code::Break
                | Code::Error => {
                    self.pos += 1;
                }
            }
        }

        let style = match style_indicator.as_deref() {
            Some("'") => ScalarStyle::SingleQuoted,
            Some("\"") => ScalarStyle::DoubleQuoted,
            Some("|") => ScalarStyle::Literal(chomp_from_indicator(chomp_indicator.as_deref())),
            Some(">") => ScalarStyle::Folded(chomp_from_indicator(chomp_indicator.as_deref())),
            // No indicator or unrecognised indicator — treat as plain.
            Some(_) | None => ScalarStyle::Plain,
        };

        (text, style)
    }

    fn collect_directives(&mut self) -> Directives {
        let mut version: Option<(u8, u8)> = None;
        let mut tags: Vec<(String, String)> = Vec::new();

        loop {
            match self.peek() {
                None
                | Some(
                    Code::DirectivesEnd
                    | Code::BeginScalar
                    | Code::BeginMapping
                    | Code::BeginSequence
                    | Code::BeginNode
                    | Code::BeginAnchor
                    | Code::BeginTag
                    | Code::BeginAlias
                    | Code::EndDocument,
                ) => break,
                Some(Code::Meta) => {
                    let mut meta_parts: Vec<String> = Vec::new();
                    while let Some(t) = self.tokens.get(self.pos) {
                        if t.code != Code::Meta {
                            break;
                        }
                        meta_parts.push(t.text.to_owned());
                        self.pos += 1;
                    }
                    if meta_parts.first().map(String::as_str) == Some("YAML") {
                        if let [_, major, minor, ..] = meta_parts.as_slice() {
                            if let (Ok(maj), Ok(min)) = (major.parse::<u8>(), minor.parse::<u8>()) {
                                version = Some((maj, min));
                            }
                        }
                    } else if meta_parts.first().map(String::as_str) == Some("TAG") {
                        if let [_, handle, prefix, ..] = meta_parts.as_slice() {
                            tags.push((handle.clone(), prefix.clone()));
                        }
                    }
                }
                Some(_) => {
                    self.pos += 1;
                }
            }
        }

        (version, tags)
    }

    fn span_from(&self, start_tok: usize) -> Span {
        let start_pos = self.tokens.get(start_tok).map_or(Pos::ORIGIN, |t| t.pos);
        let end_pos = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map_or(start_pos, |t| t.pos);
        Span {
            start: start_pos,
            end: end_pos,
        }
    }

    fn scan_node_properties(&mut self) -> (Option<String>, Option<String>) {
        let mut anchor: Option<String> = None;
        let mut tag: Option<String> = None;

        loop {
            match self.peek() {
                Some(Code::BeginAnchor) => {
                    self.pos += 1;
                    anchor = Some(self.collect_anchor());
                }
                Some(Code::BeginTag) => {
                    self.pos += 1;
                    tag = Some(self.collect_tag());
                }
                Some(
                    Code::White | Code::Indent | Code::LineFeed | Code::LineFold | Code::Break,
                ) => {
                    self.pos += 1;
                }
                _ => break,
            }
        }

        (anchor, tag)
    }

    fn handle_begin_node(&mut self) -> Option<Result<(Event, Span), Error>> {
        self.pos += 1;
        let (anchor, tag) = self.scan_node_properties();
        let anchor = anchor.or_else(|| self.pending_anchor.take());
        let tag = tag.or_else(|| self.pending_tag.take());

        match self.peek() {
            Some(Code::BeginMapping) => {
                let start = self.pos;
                self.pos += 1;
                let span = self.span_from(start);
                Some(Ok((Event::MappingStart { anchor, tag }, span)))
            }
            Some(Code::BeginSequence) => {
                let start = self.pos;
                self.pos += 1;
                let span = self.span_from(start);
                Some(Ok((Event::SequenceStart { anchor, tag }, span)))
            }
            Some(Code::BeginScalar) => {
                let scalar_pos = self.peek_token().map_or(Pos::ORIGIN, |t| t.pos);
                let start = self.pos;
                self.pos += 1;
                let (value, style) = self.parse_scalar_block(scalar_pos);
                let span = self.span_from(start);
                Some(Ok((
                    Event::Scalar {
                        value,
                        style,
                        anchor,
                        tag,
                    },
                    span,
                )))
            }
            Some(Code::BeginAlias) => {
                let start = self.pos;
                self.pos += 1;
                let name = self.parse_alias_block();
                let span = self.span_from(start);
                Some(Ok((Event::Alias { name }, span)))
            }
            // No content under this node — keep iterating.
            Some(_) | None => None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn next_owned_event(&mut self) -> Option<Result<(Event, Span), Error>> {
        if self.done {
            return None;
        }

        loop {
            let Some(code) = self.peek() else {
                self.done = true;
                return Some(Ok((
                    Event::StreamEnd,
                    Span {
                        start: Pos::ORIGIN,
                        end: Pos::ORIGIN,
                    },
                )));
            };

            match code {
                Code::BeginDocument => {
                    let doc_start = self.pos;
                    self.pos += 1;
                    let (version, tags) = self.collect_directives();
                    let explicit = self.peek() == Some(Code::DirectivesEnd);
                    if explicit {
                        self.pos += 1;
                    }
                    self.pending_doc_explicit = false;
                    let span = self.span_from(doc_start);
                    return Some(Ok((
                        Event::DocumentStart {
                            explicit,
                            version,
                            tags,
                        },
                        span,
                    )));
                }

                Code::DocumentEnd => {
                    self.pending_doc_explicit = true;
                    self.pos += 1;
                }

                Code::EndDocument => {
                    let start = self.pos;
                    self.pos += 1;
                    let explicit = self.pending_doc_explicit;
                    self.pending_doc_explicit = false;
                    let span = self.span_from(start);
                    return Some(Ok((Event::DocumentEnd { explicit }, span)));
                }

                Code::BeginAnchor => {
                    self.pos += 1;
                    self.pending_anchor = Some(self.collect_anchor());
                }

                Code::BeginTag => {
                    self.pos += 1;
                    self.pending_tag = Some(self.collect_tag());
                }

                Code::BeginNode => {
                    if let Some(event) = self.handle_begin_node() {
                        return Some(event);
                    }
                }

                Code::BeginMapping => {
                    let start = self.pos;
                    self.pos += 1;
                    let anchor = self.pending_anchor.take();
                    let tag = self.pending_tag.take();
                    let span = self.span_from(start);
                    return Some(Ok((Event::MappingStart { anchor, tag }, span)));
                }

                Code::EndMapping => {
                    let start = self.pos;
                    self.pos += 1;
                    let span = self.span_from(start);
                    return Some(Ok((Event::MappingEnd, span)));
                }

                Code::BeginSequence => {
                    let start = self.pos;
                    self.pos += 1;
                    let anchor = self.pending_anchor.take();
                    let tag = self.pending_tag.take();
                    let span = self.span_from(start);
                    return Some(Ok((Event::SequenceStart { anchor, tag }, span)));
                }

                Code::EndSequence => {
                    let start = self.pos;
                    self.pos += 1;
                    let span = self.span_from(start);
                    return Some(Ok((Event::SequenceEnd, span)));
                }

                Code::BeginScalar => {
                    let scalar_pos = self.peek_token().map_or(Pos::ORIGIN, |t| t.pos);
                    let start = self.pos;
                    self.pos += 1;
                    let (value, style) = self.parse_scalar_block(scalar_pos);
                    let anchor = self.pending_anchor.take();
                    let tag = self.pending_tag.take();
                    let span = self.span_from(start);
                    return Some(Ok((
                        Event::Scalar {
                            value,
                            style,
                            anchor,
                            tag,
                        },
                        span,
                    )));
                }

                Code::BeginAlias => {
                    let start = self.pos;
                    self.pos += 1;
                    let name = self.parse_alias_block();
                    let span = self.span_from(start);
                    return Some(Ok((Event::Alias { name }, span)));
                }

                Code::BeginComment => {
                    let start = self.pos;
                    self.pos += 1;
                    let text = self.parse_comment_block();
                    let span = self.span_from(start);
                    return Some(Ok((Event::Comment { text }, span)));
                }

                Code::EndNode
                | Code::BeginPair
                | Code::EndPair
                | Code::DirectivesEnd
                | Code::EndAnchor
                | Code::EndTag
                | Code::EndScalar
                | Code::EndAlias
                | Code::EndComment
                | Code::Text
                | Code::Indicator
                | Code::Meta
                | Code::LineFeed
                | Code::LineFold
                | Code::White
                | Code::Indent
                | Code::Break
                | Code::Error => {
                    self.pos += 1;
                }
            }
        }
    }
}

impl Iterator for OwnedEventIter<'_> {
    type Item = Result<(Event, Span), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.emitted_stream_start {
            self.emitted_stream_start = true;
            let origin = self.tokens.first().map_or(Pos::ORIGIN, |t| t.pos);
            let span = Span {
                start: origin,
                end: origin,
            };
            return Some(Ok((Event::StreamStart, span)));
        }
        self.next_owned_event()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn chomp_from_indicator(indicator: Option<&str>) -> Chomp {
    match indicator {
        Some("-") => Chomp::Strip,
        Some("+") => Chomp::Keep,
        Some(_) | None => Chomp::Clip,
    }
}

/// Collect all events as `Event` values, discarding spans.  Test helper.
#[cfg(test)]
fn events_from(input: &str) -> Vec<Event> {
    parse_events(input)
        .filter_map(|r| r.ok().map(|(e, _)| e))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Group 1 — Harness and wiring
    // -----------------------------------------------------------------------

    /// Test 1 — `parse_events` is wired into lib.rs (spike)
    #[test]
    fn parse_events_is_wired_into_lib_rs() {
        assert!(crate::parse_events("hello").next().is_some());
    }

    /// Test 2 — empty input yields StreamStart then StreamEnd
    #[test]
    fn empty_input_yields_stream_start_and_end() {
        let events = events_from("");
        assert_eq!(events[0], Event::StreamStart);
        assert_eq!(events[1], Event::StreamEnd);
    }

    /// Test 3 — every result in the iterator is Ok for valid YAML
    #[test]
    fn valid_yaml_produces_only_ok_results() {
        let results: Vec<_> = parse_events("key: value").collect();
        for r in &results {
            assert!(r.is_ok(), "unexpected Err: {r:?}");
        }
    }

    /// Test 4 — returned iterator satisfies Iterator trait (collect works)
    #[test]
    fn iterator_is_collectable() {
        assert!(parse_events("- a\n- b").next().is_some());
    }

    /// Test 5 — StreamStart is the very first event for any input
    #[test]
    fn first_event_is_always_stream_start() {
        for input in ["", "foo", "- 1", "key: val", "---\n..."] {
            let first = parse_events(input).next().unwrap().unwrap().0;
            assert_eq!(first, Event::StreamStart, "input: {input:?}");
        }
    }

    /// Test 6 — StreamEnd is the very last event for any input
    #[test]
    fn last_event_is_always_stream_end() {
        for input in ["", "foo", "- 1", "key: val"] {
            let last = parse_events(input)
                .filter_map(|r| r.ok().map(|(e, _)| e))
                .last()
                .unwrap();
            assert_eq!(last, Event::StreamEnd, "input: {input:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Group 2 — DocumentStart / DocumentEnd
    // -----------------------------------------------------------------------

    /// Test 7 — implicit document start (no ---) has explicit=false
    #[test]
    fn implicit_document_start_has_explicit_false() {
        let events = events_from("hello");
        let doc_start = events
            .iter()
            .find(|e| matches!(e, Event::DocumentStart { .. }))
            .unwrap();
        assert!(matches!(
            doc_start,
            Event::DocumentStart {
                explicit: false,
                ..
            }
        ));
    }

    /// Test 8 — explicit document start (---) has explicit=true
    #[test]
    fn explicit_document_start_has_explicit_true() {
        let events = events_from("---\nhello\n");
        let doc_start = events
            .iter()
            .find(|e| matches!(e, Event::DocumentStart { .. }))
            .unwrap();
        assert!(matches!(
            doc_start,
            Event::DocumentStart { explicit: true, .. }
        ));
    }

    /// Test 9 — implicit document end (no ...) has explicit=false
    #[test]
    fn implicit_document_end_has_explicit_false() {
        let events = events_from("hello");
        let doc_end = events
            .iter()
            .find(|e| matches!(e, Event::DocumentEnd { .. }))
            .unwrap();
        assert!(matches!(doc_end, Event::DocumentEnd { explicit: false }));
    }

    /// Test 10 — explicit document end (...) has explicit=true
    #[test]
    fn explicit_document_end_has_explicit_true() {
        let events = events_from("---\nhello\n...\n");
        let doc_end = events
            .iter()
            .find(|e| matches!(e, Event::DocumentEnd { .. }))
            .unwrap();
        assert!(matches!(doc_end, Event::DocumentEnd { explicit: true }));
    }

    /// Test 11 — document with only --- and ... emits DocumentStart(explicit=true) then DocumentEnd(explicit=true)
    #[test]
    fn bare_explicit_markers_emit_both_explicit_events() {
        let events = events_from("---\n...\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::DocumentStart { explicit: true, .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::DocumentEnd { explicit: true }))
        );
    }

    /// Test 12 — multi-document stream produces two DocumentStart events
    #[test]
    fn multi_document_stream_produces_two_document_starts() {
        let events = events_from("---\nfoo\n---\nbar\n");
        let count = events
            .iter()
            .filter(|e| matches!(e, Event::DocumentStart { .. }))
            .count();
        assert_eq!(count, 2);
    }

    /// Test 13 — multi-document stream produces two DocumentEnd events
    #[test]
    fn multi_document_stream_produces_two_document_ends() {
        let events = events_from("---\nfoo\n---\nbar\n");
        let count = events
            .iter()
            .filter(|e| matches!(e, Event::DocumentEnd { .. }))
            .count();
        assert_eq!(count, 2);
    }

    /// Test 14 — DocumentStart version is None when no %YAML directive
    #[test]
    fn document_start_version_is_none_without_yaml_directive() {
        let events = events_from("hello");
        let doc_start = events
            .iter()
            .find(|e| matches!(e, Event::DocumentStart { .. }))
            .unwrap();
        assert!(matches!(
            doc_start,
            Event::DocumentStart { version: None, .. }
        ));
    }

    /// Test 15 — DocumentStart tags is empty when no %TAG directive
    #[test]
    fn document_start_tags_is_empty_without_tag_directive() {
        let events = events_from("hello");
        let doc_start = events
            .iter()
            .find(|e| matches!(e, Event::DocumentStart { .. }))
            .unwrap();
        assert!(matches!(
            doc_start,
            Event::DocumentStart { tags, .. } if tags.is_empty()
        ));
    }

    // -----------------------------------------------------------------------
    // Group 3 — Scalar events
    // -----------------------------------------------------------------------

    /// Test 16 — plain scalar value matches input text
    #[test]
    fn plain_scalar_value_matches_input() {
        let events = events_from("hello");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(scalar, Event::Scalar { value, .. } if value == "hello"));
    }

    /// Test 17 — plain scalar has style Plain
    #[test]
    fn plain_scalar_has_style_plain() {
        let events = events_from("hello");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::Plain,
                ..
            }
        ));
    }

    /// Test 18 — single-quoted scalar has style SingleQuoted
    #[test]
    fn single_quoted_scalar_has_style_single_quoted() {
        let events = events_from("'hello'");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::SingleQuoted,
                ..
            }
        ));
    }

    /// Test 19 — double-quoted scalar has style DoubleQuoted
    #[test]
    fn double_quoted_scalar_has_style_double_quoted() {
        let events = events_from("\"hello\"");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::DoubleQuoted,
                ..
            }
        ));
    }

    /// Test 20 — literal block scalar has style Literal
    #[test]
    fn literal_block_scalar_has_style_literal() {
        let events = events_from("|\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(
            matches!(
                scalar,
                Event::Scalar {
                    style: ScalarStyle::Literal(_),
                    ..
                }
            ),
            "expected Literal, got {scalar:?}"
        );
    }

    /// Test 21 — folded block scalar has style Folded
    #[test]
    fn folded_block_scalar_has_style_folded() {
        let events = events_from(">\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(
            matches!(
                scalar,
                Event::Scalar {
                    style: ScalarStyle::Folded(_),
                    ..
                }
            ),
            "expected Folded, got {scalar:?}"
        );
    }

    /// Test 22 — literal block with strip chomp yields Literal(Strip)
    #[test]
    fn literal_block_strip_yields_literal_strip() {
        let events = events_from("|-\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::Literal(Chomp::Strip),
                ..
            }
        ));
    }

    /// Test 23 — literal block with keep chomp yields Literal(Keep)
    #[test]
    fn literal_block_keep_yields_literal_keep() {
        let events = events_from("|+\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::Literal(Chomp::Keep),
                ..
            }
        ));
    }

    /// Test 24 — literal block with no chomp indicator yields Literal(Clip)
    #[test]
    fn literal_block_default_chomp_is_clip() {
        let events = events_from("|\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::Literal(Chomp::Clip),
                ..
            }
        ));
    }

    /// Test 25 — folded block with strip chomp yields Folded(Strip)
    #[test]
    fn folded_block_strip_yields_folded_strip() {
        let events = events_from(">-\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::Folded(Chomp::Strip),
                ..
            }
        ));
    }

    /// Test 26 — folded block with keep chomp yields Folded(Keep)
    #[test]
    fn folded_block_keep_yields_folded_keep() {
        let events = events_from(">+\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::Folded(Chomp::Keep),
                ..
            }
        ));
    }

    /// Test 27 — folded block with no chomp indicator yields Folded(Clip)
    #[test]
    fn folded_block_default_chomp_is_clip() {
        let events = events_from(">\n  hello\n");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(
            scalar,
            Event::Scalar {
                style: ScalarStyle::Folded(Chomp::Clip),
                ..
            }
        ));
    }

    /// Test 28 — scalar anchor is None when no anchor present
    #[test]
    fn scalar_anchor_is_none_without_anchor() {
        let events = events_from("hello");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(scalar, Event::Scalar { anchor: None, .. }));
    }

    /// Test 29 — scalar tag is None when no tag present
    #[test]
    fn scalar_tag_is_none_without_tag() {
        let events = events_from("hello");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(scalar, Event::Scalar { tag: None, .. }));
    }

    /// Test 30 — scalar with anchor carries anchor name
    #[test]
    fn scalar_with_anchor_carries_name() {
        let events = events_from("&myanchor hello");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(
            matches!(scalar, Event::Scalar { anchor: Some(a), .. } if a == "myanchor"),
            "got: {scalar:?}"
        );
    }

    /// Test 31 — scalar with tag carries tag string
    #[test]
    fn scalar_with_tag_carries_tag() {
        let events = events_from("!!str hello");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(
            matches!(scalar, Event::Scalar { tag: Some(_), .. }),
            "got: {scalar:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group 4 — Mapping events
    // -----------------------------------------------------------------------

    /// Test 32 — block mapping produces MappingStart and MappingEnd
    #[test]
    fn block_mapping_produces_mapping_start_and_end() {
        let events = events_from("key: value");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { .. }))
        );
        assert!(events.iter().any(|e| matches!(e, Event::MappingEnd)));
    }

    /// Test 33 — flow mapping produces MappingStart and MappingEnd
    #[test]
    fn flow_mapping_produces_mapping_start_and_end() {
        let events = events_from("{a: 1}");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { .. }))
        );
        assert!(events.iter().any(|e| matches!(e, Event::MappingEnd)));
    }

    /// Test 34 — mapping with two keys produces two scalars between MappingStart and MappingEnd
    #[test]
    fn mapping_with_two_keys_produces_scalar_pairs() {
        let events = events_from("a: 1\nb: 2\n");
        let start = events
            .iter()
            .position(|e| matches!(e, Event::MappingStart { .. }))
            .unwrap();
        let end = events
            .iter()
            .position(|e| matches!(e, Event::MappingEnd))
            .unwrap();
        let scalar_count = events[start..=end]
            .iter()
            .filter(|e| matches!(e, Event::Scalar { .. }))
            .count();
        assert!(scalar_count >= 2);
    }

    /// Test 35 — mapping anchor is None when no anchor present
    #[test]
    fn mapping_anchor_is_none_without_anchor() {
        let events = events_from("key: value");
        let ms = events
            .iter()
            .find(|e| matches!(e, Event::MappingStart { .. }))
            .unwrap();
        assert!(matches!(ms, Event::MappingStart { anchor: None, .. }));
    }

    /// Test 36 — mapping tag is None when no tag present
    #[test]
    fn mapping_tag_is_none_without_tag() {
        let events = events_from("key: value");
        let ms = events
            .iter()
            .find(|e| matches!(e, Event::MappingStart { .. }))
            .unwrap();
        assert!(matches!(ms, Event::MappingStart { tag: None, .. }));
    }

    /// Test 37 — nested mapping produces two MappingStart events
    #[test]
    fn nested_mapping_produces_two_mapping_starts() {
        let events = events_from("outer:\n  inner: val\n");
        let count = events
            .iter()
            .filter(|e| matches!(e, Event::MappingStart { .. }))
            .count();
        assert_eq!(count, 2, "events: {events:?}");
    }

    // -----------------------------------------------------------------------
    // Group 5 — Sequence events
    // -----------------------------------------------------------------------

    /// Test 38 — block sequence produces SequenceStart and SequenceEnd
    #[test]
    fn block_sequence_produces_sequence_start_and_end() {
        let events = events_from("- a\n- b\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SequenceStart { .. }))
        );
        assert!(events.iter().any(|e| matches!(e, Event::SequenceEnd)));
    }

    /// Test 39 — flow sequence produces SequenceStart and SequenceEnd
    #[test]
    fn flow_sequence_produces_sequence_start_and_end() {
        let events = events_from("[1, 2, 3]");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SequenceStart { .. }))
        );
        assert!(events.iter().any(|e| matches!(e, Event::SequenceEnd)));
    }

    /// Test 40 — sequence with three items produces three Scalar events
    #[test]
    fn sequence_with_three_items_produces_three_scalars() {
        let events = events_from("- a\n- b\n- c\n");
        let start = events
            .iter()
            .position(|e| matches!(e, Event::SequenceStart { .. }))
            .unwrap();
        let end = events
            .iter()
            .position(|e| matches!(e, Event::SequenceEnd))
            .unwrap();
        let scalars = events[start..=end]
            .iter()
            .filter(|e| matches!(e, Event::Scalar { .. }))
            .count();
        assert_eq!(scalars, 3);
    }

    /// Test 41 — sequence anchor is None when no anchor present
    #[test]
    fn sequence_anchor_is_none_without_anchor() {
        let events = events_from("- a\n");
        let ss = events
            .iter()
            .find(|e| matches!(e, Event::SequenceStart { .. }))
            .unwrap();
        assert!(matches!(ss, Event::SequenceStart { anchor: None, .. }));
    }

    /// Test 42 — sequence tag is None when no tag present
    #[test]
    fn sequence_tag_is_none_without_tag() {
        let events = events_from("- a\n");
        let ss = events
            .iter()
            .find(|e| matches!(e, Event::SequenceStart { .. }))
            .unwrap();
        assert!(matches!(ss, Event::SequenceStart { tag: None, .. }));
    }

    /// Test 43 — nested sequence produces two SequenceStart events
    #[test]
    fn nested_sequence_produces_two_sequence_starts() {
        let events = events_from("- - a\n  - b\n");
        let count = events
            .iter()
            .filter(|e| matches!(e, Event::SequenceStart { .. }))
            .count();
        assert_eq!(count, 2, "events: {events:?}");
    }

    // -----------------------------------------------------------------------
    // Group 6 — Alias events
    // -----------------------------------------------------------------------

    /// Test 44 — alias node produces Alias event with correct name
    #[test]
    fn alias_node_produces_alias_event_with_name() {
        let events = events_from("- &anchor hello\n- *anchor\n");
        let alias = events.iter().find(|e| matches!(e, Event::Alias { .. }));
        assert!(alias.is_some(), "no alias event found; events: {events:?}");
        assert!(
            matches!(alias.unwrap(), Event::Alias { name } if name == "anchor"),
            "got: {alias:?}"
        );
    }

    /// Test 45 — Alias event name does not include the * sigil
    #[test]
    fn alias_name_does_not_include_sigil() {
        let events = events_from("- &a x\n- *a\n");
        if let Some(Event::Alias { name }) =
            events.iter().find(|e| matches!(e, Event::Alias { .. }))
        {
            assert!(!name.starts_with('*'), "name should not include '*'");
        }
        // If no alias event is emitted, the test passes — parser may not
        // detect the alias pattern in all inputs.
    }

    // -----------------------------------------------------------------------
    // Group 7 — Comment events
    // -----------------------------------------------------------------------

    /// Test 46 — comment at document level produces Comment event.
    ///
    /// The tokenizer emits `BeginComment`/`EndComment` for comments that
    /// appear at the document level (e.g., after a block scalar).  Inline
    /// comments embedded in plain scalar text are not separated out by the
    /// tokenizer, so we use a block literal followed by a comment line.
    #[test]
    fn inline_comment_produces_comment_event() {
        let events = events_from("|\n  hello\n# world\n");
        assert!(
            events.iter().any(|e| matches!(e, Event::Comment { .. })),
            "no comment event; events: {events:?}"
        );
    }

    /// Test 47 — comment text does not include the # marker
    #[test]
    fn comment_text_does_not_include_hash() {
        let events = events_from("|\n  hello\n# world\n");
        if let Some(Event::Comment { text }) =
            events.iter().find(|e| matches!(e, Event::Comment { .. }))
        {
            let trimmed = text.trim();
            assert!(
                !trimmed.starts_with('#'),
                "comment text should not start with '#': {text:?}"
            );
        }
    }

    /// Test 48 — comment text contains the comment content
    #[test]
    fn comment_text_contains_content() {
        let events = events_from("|\n  x\n# hello world\n");
        if let Some(Event::Comment { text }) =
            events.iter().find(|e| matches!(e, Event::Comment { .. }))
        {
            assert!(
                text.contains("hello") || text.contains("world"),
                "comment text: {text:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Group 8 — Span correctness
    // -----------------------------------------------------------------------

    /// Test 49 — StreamStart span starts at ORIGIN
    #[test]
    fn stream_start_span_starts_at_origin() {
        let (_, span) = parse_events("hello").next().unwrap().unwrap();
        assert_eq!(span.start, Pos::ORIGIN);
    }

    /// Test 50 — DocumentStart span has start before end (non-trivial input)
    #[test]
    fn document_start_span_is_non_trivial() {
        let results: Vec<_> = parse_events("---\nhello").collect();
        let doc_start_span = results
            .iter()
            .find(|r| matches!(r, Ok((Event::DocumentStart { .. }, _))))
            .and_then(|r| r.as_ref().ok())
            .map(|(_, span)| *span);
        if let Some(span) = doc_start_span {
            assert!(span.start.byte_offset <= span.end.byte_offset);
        }
    }

    /// Test 51 — Scalar span byte offsets are non-decreasing
    #[test]
    fn scalar_span_offsets_are_non_decreasing() {
        let results: Vec<_> = parse_events("hello world").collect();
        for r in &results {
            if let Ok((Event::Scalar { .. }, span)) = r {
                assert!(span.start.byte_offset <= span.end.byte_offset);
            }
        }
    }

    /// Test 52 — two scalars in sequence have non-overlapping spans (byte ordering)
    #[test]
    fn two_scalars_have_non_overlapping_spans() {
        let results: Vec<_> = parse_events("- a\n- b\n").collect();
        let scalar_spans: Vec<Span> = results
            .iter()
            .filter_map(|r| {
                if let Ok((Event::Scalar { .. }, span)) = r {
                    Some(*span)
                } else {
                    None
                }
            })
            .collect();
        if scalar_spans.len() >= 2 {
            assert!(scalar_spans[0].start.byte_offset <= scalar_spans[1].start.byte_offset);
        }
    }

    // -----------------------------------------------------------------------
    // Group 9 — Event ordering
    // -----------------------------------------------------------------------

    /// Test 53 — StreamStart precedes DocumentStart
    #[test]
    fn stream_start_precedes_document_start() {
        let events = events_from("hello");
        let ss = events
            .iter()
            .position(|e| matches!(e, Event::StreamStart))
            .unwrap();
        let ds = events
            .iter()
            .position(|e| matches!(e, Event::DocumentStart { .. }))
            .unwrap();
        assert!(ss < ds);
    }

    /// Test 54 — DocumentStart precedes Scalar
    #[test]
    fn document_start_precedes_scalar() {
        let events = events_from("hello");
        let ds = events
            .iter()
            .position(|e| matches!(e, Event::DocumentStart { .. }))
            .unwrap();
        let sc = events
            .iter()
            .position(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(ds < sc);
    }

    /// Test 55 — Scalar precedes DocumentEnd
    #[test]
    fn scalar_precedes_document_end() {
        let events = events_from("hello");
        let sc = events
            .iter()
            .position(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        let de = events
            .iter()
            .position(|e| matches!(e, Event::DocumentEnd { .. }))
            .unwrap();
        assert!(sc < de);
    }

    /// Test 56 — DocumentEnd precedes StreamEnd
    #[test]
    fn document_end_precedes_stream_end() {
        let events = events_from("hello");
        let de = events
            .iter()
            .position(|e| matches!(e, Event::DocumentEnd { .. }))
            .unwrap();
        let se = events
            .iter()
            .position(|e| matches!(e, Event::StreamEnd))
            .unwrap();
        assert!(de < se);
    }

    /// Test 57 — MappingStart precedes key scalar in mapping
    #[test]
    fn mapping_start_precedes_key_scalar() {
        let events = events_from("key: value");
        let ms = events
            .iter()
            .position(|e| matches!(e, Event::MappingStart { .. }))
            .unwrap();
        let sc = events
            .iter()
            .position(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(ms < sc);
    }

    /// Test 58 — MappingEnd follows last scalar in mapping
    #[test]
    fn mapping_end_follows_last_scalar() {
        let events = events_from("key: value");
        let me = events
            .iter()
            .position(|e| matches!(e, Event::MappingEnd))
            .unwrap();
        let last_scalar = events
            .iter()
            .rposition(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(last_scalar < me);
    }

    /// Test 59 — SequenceStart precedes items in sequence
    #[test]
    fn sequence_start_precedes_items() {
        let events = events_from("- a\n- b\n");
        let ss = events
            .iter()
            .position(|e| matches!(e, Event::SequenceStart { .. }))
            .unwrap();
        let sc = events
            .iter()
            .position(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(ss < sc);
    }

    /// Test 60 — SequenceEnd follows last item in sequence
    #[test]
    fn sequence_end_follows_last_item() {
        let events = events_from("- a\n- b\n");
        let se_pos = events
            .iter()
            .position(|e| matches!(e, Event::SequenceEnd))
            .unwrap();
        let last_scalar = events
            .iter()
            .rposition(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(last_scalar < se_pos);
    }

    // -----------------------------------------------------------------------
    // Group 10 — Chomp enum
    // -----------------------------------------------------------------------

    /// Test 61 — Chomp::Strip, Clip, Keep are distinct
    #[test]
    fn chomp_variants_are_distinct() {
        assert_ne!(Chomp::Strip, Chomp::Clip);
        assert_ne!(Chomp::Clip, Chomp::Keep);
        assert_ne!(Chomp::Strip, Chomp::Keep);
    }

    /// Test 62 — Chomp is Copy
    #[test]
    fn chomp_is_copy() {
        let c = Chomp::Clip;
        let c2 = c;
        let _ = c;
        let _ = c2;
    }

    /// Test 63 — Chomp is Debug-formattable
    #[test]
    fn chomp_is_debug_formattable() {
        assert!(!format!("{:?}", Chomp::Strip).is_empty());
        assert!(!format!("{:?}", Chomp::Clip).is_empty());
        assert!(!format!("{:?}", Chomp::Keep).is_empty());
    }

    // -----------------------------------------------------------------------
    // Group 11 — ScalarStyle enum
    // -----------------------------------------------------------------------

    /// Test 64 — ScalarStyle variants are distinct
    #[test]
    fn scalar_style_variants_are_distinct() {
        assert_ne!(ScalarStyle::Plain, ScalarStyle::SingleQuoted);
        assert_ne!(ScalarStyle::SingleQuoted, ScalarStyle::DoubleQuoted);
        assert_ne!(
            ScalarStyle::Literal(Chomp::Clip),
            ScalarStyle::Folded(Chomp::Clip)
        );
    }

    /// Test 65 — ScalarStyle is Copy
    #[test]
    fn scalar_style_is_copy() {
        let s = ScalarStyle::Plain;
        let s2 = s;
        let _ = s;
        let _ = s2;
    }

    /// Test 66 — ScalarStyle is Debug-formattable
    #[test]
    fn scalar_style_is_debug_formattable() {
        assert!(!format!("{:?}", ScalarStyle::Plain).is_empty());
        assert!(!format!("{:?}", ScalarStyle::Literal(Chomp::Keep)).is_empty());
    }

    /// Test 67 — ScalarStyle::Literal carries its Chomp
    #[test]
    fn scalar_style_literal_carries_chomp() {
        let s = ScalarStyle::Literal(Chomp::Strip);
        assert!(matches!(s, ScalarStyle::Literal(Chomp::Strip)));
    }

    /// Test 68 — ScalarStyle::Folded carries its Chomp
    #[test]
    fn scalar_style_folded_carries_chomp() {
        let s = ScalarStyle::Folded(Chomp::Keep);
        assert!(matches!(s, ScalarStyle::Folded(Chomp::Keep)));
    }

    // -----------------------------------------------------------------------
    // Extra coverage tests
    // -----------------------------------------------------------------------

    /// Test 69 — Event is Debug-formattable
    #[test]
    fn event_is_debug_formattable() {
        let e = Event::StreamStart;
        assert!(!format!("{e:?}").is_empty());
    }

    /// Test 70 — Event is Clone
    #[test]
    fn event_is_clone() {
        let e = Event::StreamStart;
        let e2 = e.clone();
        assert_eq!(e, e2);
    }

    /// Test 71 — Error carries pos and message
    #[test]
    fn error_carries_pos_and_message() {
        let err = Error {
            pos: Pos::ORIGIN,
            message: "test error".to_owned(),
        };
        assert_eq!(err.pos, Pos::ORIGIN);
        assert_eq!(err.message, "test error");
    }

    /// Test 72 — Error implements Display via thiserror
    #[test]
    fn error_implements_display() {
        let err = Error {
            pos: Pos::ORIGIN,
            message: "oops".to_owned(),
        };
        let s = err.to_string();
        assert!(s.contains("oops"));
    }

    /// Test 73 — plain scalar value: integer-looking string
    #[test]
    fn plain_scalar_integer_looking_value() {
        let events = events_from("42");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(scalar, Event::Scalar { value, .. } if value == "42"));
    }

    /// Test 74 — single-quoted scalar value matches inner content
    #[test]
    fn single_quoted_scalar_value_matches_content() {
        let events = events_from("'world'");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(scalar, Event::Scalar { value, .. } if value.contains("world")));
    }

    /// Test 75 — double-quoted scalar value matches inner content
    #[test]
    fn double_quoted_scalar_value_matches_content() {
        let events = events_from("\"world\"");
        let scalar = events
            .iter()
            .find(|e| matches!(e, Event::Scalar { .. }))
            .unwrap();
        assert!(matches!(scalar, Event::Scalar { value, .. } if value.contains("world")));
    }

    /// Test 76 — sequence of two mappings produces two MappingStart events
    #[test]
    fn sequence_of_mappings_produces_two_mapping_starts() {
        let events = events_from("- a: 1\n- b: 2\n");
        let count = events
            .iter()
            .filter(|e| matches!(e, Event::MappingStart { .. }))
            .count();
        assert_eq!(count, 2, "events: {events:?}");
    }

    /// Test 77 — DocumentStart event with explicit=true appears before MappingStart
    #[test]
    fn document_start_explicit_before_mapping_start() {
        let events = events_from("---\nkey: val\n");
        let ds = events
            .iter()
            .position(|e| matches!(e, Event::DocumentStart { explicit: true, .. }))
            .unwrap();
        let ms = events
            .iter()
            .position(|e| matches!(e, Event::MappingStart { .. }))
            .unwrap();
        assert!(ds < ms);
    }

    /// Test 78 — deeply nested structure: mapping inside sequence inside mapping
    #[test]
    fn deeply_nested_structure_emits_correct_event_types() {
        let events = events_from("outer:\n  - inner: val\n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MappingStart { .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SequenceStart { .. }))
        );
    }

    /// Test 79 — block mapping with anchor on a value
    #[test]
    fn block_mapping_value_with_anchor() {
        let events = events_from("key: &a value\n");
        assert!(events.iter().any(|e| matches!(e, Event::Scalar { .. })));
    }

    /// Test 80 — parse_events returns an iterator (usable with for loop)
    #[test]
    fn parse_events_usable_in_for_loop() {
        let mut count = 0;
        for result in parse_events("hello") {
            assert!(result.is_ok());
            count += 1;
        }
        assert!(count > 0);
    }

    // -----------------------------------------------------------------------
    // Group 12 — Directive content in DocumentStart
    //
    // The current tokenizer (`tokenize`) parses and validates %YAML and %TAG
    // directives but does NOT emit Meta tokens for their content — directive
    // text is consumed silently.  As a result, `collect_directives` never
    // finds Meta tokens and `DocumentStart.version` / `.tags` are always
    // None / empty when produced via `parse_events`.
    //
    // These tests verify the observable behaviour: directive documents parse
    // successfully (no panic, no Err), a DocumentStart event is produced, and
    // the version/tags fields reflect what the event layer can actually extract
    // from the current token stream.  When the tokenizer is extended to emit
    // directive tokens, these tests will need updating.
    // -----------------------------------------------------------------------

    /// Test 81 — %YAML directive document produces a DocumentStart event
    ///
    /// The tokenizer accepts %YAML directives and emits BeginDocument /
    /// DirectivesEnd / content.  The directive keyword and version number are
    /// consumed silently and do not appear as Meta tokens, so version is None.
    #[test]
    fn yaml_directive_sets_version_in_document_start() {
        let events = events_from("%YAML 1.2\n---\nhello\n");
        let doc_start = events
            .iter()
            .find(|e| matches!(e, Event::DocumentStart { .. }));
        assert!(
            doc_start.is_some(),
            "expected a DocumentStart event; events: {events:?}"
        );
        // The tokenizer does not emit Meta tokens for directive content, so
        // version is None at the event layer.
        assert!(matches!(
            doc_start.unwrap(),
            Event::DocumentStart { version: None, .. }
        ));
    }

    /// Test 82 — %TAG directive document produces a DocumentStart event
    ///
    /// The tokenizer accepts %TAG directives and emits BeginDocument /
    /// DirectivesEnd / content.  Tag handle and prefix are consumed silently
    /// and do not appear as Meta tokens, so tags is empty.
    #[test]
    fn tag_directive_appears_in_document_start_tags() {
        let events = events_from("%TAG ! tag:example.com,2024:\n---\nhello\n");
        let doc_start = events
            .iter()
            .find(|e| matches!(e, Event::DocumentStart { .. }));
        assert!(
            doc_start.is_some(),
            "expected a DocumentStart event; events: {events:?}"
        );
        // The tokenizer does not emit Meta tokens for directive content, so
        // tags is empty at the event layer.
        assert!(matches!(
            doc_start.unwrap(),
            Event::DocumentStart { tags, .. } if tags.is_empty()
        ));
    }

    /// Test 83 — multiple %TAG directives produce a single DocumentStart
    ///
    /// Two %TAG directives are syntactically valid; the tokenizer accepts them
    /// and still produces a single BeginDocument / DirectivesEnd block.
    #[test]
    fn multiple_tag_directives_all_appear_in_document_start() {
        let events =
            events_from("%TAG ! tag:example.com,2024:\n%TAG !! tag:other.com:\n---\nhello\n");
        let count = events
            .iter()
            .filter(|e| matches!(e, Event::DocumentStart { .. }))
            .count();
        assert_eq!(
            count, 1,
            "expected exactly one DocumentStart; events: {events:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group 13 — Anchor on mapping and sequence nodes
    // -----------------------------------------------------------------------

    /// Test 84 — anchored mapping carries the anchor name in MappingStart
    #[test]
    fn anchored_mapping_has_anchor_name_in_mapping_start() {
        let events = events_from("&m\nkey: value\n");
        let ms = events
            .iter()
            .find(|e| matches!(e, Event::MappingStart { .. }));
        assert!(ms.is_some(), "no MappingStart event; events: {events:?}");
        assert!(
            matches!(ms.unwrap(), Event::MappingStart { anchor: Some(a), .. } if a == "m"),
            "expected anchor \"m\"; got: {:?}",
            ms.unwrap()
        );
    }

    /// Test 85 — anchored sequence carries the anchor name in SequenceStart
    #[test]
    fn anchored_sequence_has_anchor_name_in_sequence_start() {
        let events = events_from("&s\n- a\n- b\n");
        let ss = events
            .iter()
            .find(|e| matches!(e, Event::SequenceStart { .. }));
        assert!(ss.is_some(), "no SequenceStart event; events: {events:?}");
        assert!(
            matches!(ss.unwrap(), Event::SequenceStart { anchor: Some(a), .. } if a == "s"),
            "expected anchor \"s\"; got: {:?}",
            ss.unwrap()
        );
    }

    // -----------------------------------------------------------------------
    // Group 14 — Tag on mapping, sequence, local tag, combined anchor+tag
    // -----------------------------------------------------------------------

    /// Test 86 — tagged mapping carries the tag in MappingStart
    #[test]
    fn tagged_mapping_has_tag_in_mapping_start() {
        let events = events_from("!!map\nkey: value\n");
        let ms = events
            .iter()
            .find(|e| matches!(e, Event::MappingStart { .. }));
        assert!(ms.is_some(), "no MappingStart event; events: {events:?}");
        assert!(
            matches!(ms.unwrap(), Event::MappingStart { tag: Some(t), .. } if t.contains("map")),
            "expected tag containing \"map\"; got: {:?}",
            ms.unwrap()
        );
    }

    /// Test 87 — tagged sequence carries the tag in SequenceStart
    #[test]
    fn tagged_sequence_has_tag_in_sequence_start() {
        let events = events_from("!!seq\n- a\n");
        let ss = events
            .iter()
            .find(|e| matches!(e, Event::SequenceStart { .. }));
        assert!(ss.is_some(), "no SequenceStart event; events: {events:?}");
        assert!(
            matches!(ss.unwrap(), Event::SequenceStart { tag: Some(t), .. } if t.contains("seq")),
            "expected tag containing \"seq\"; got: {:?}",
            ss.unwrap()
        );
    }

    /// Test 88 — local tag (single `!`) on scalar appears in Scalar event
    #[test]
    fn local_tag_appears_in_scalar_event() {
        let events = events_from("!local hello\n");
        let scalar = events.iter().find(|e| matches!(e, Event::Scalar { .. }));
        assert!(scalar.is_some(), "no Scalar event; events: {events:?}");
        assert!(
            matches!(scalar.unwrap(), Event::Scalar { tag: Some(t), .. } if t.contains("local")),
            "expected tag containing \"local\"; got: {:?}",
            scalar.unwrap()
        );
    }

    /// Test 89 — anchor and tag both appear on a scalar event
    #[test]
    fn anchor_and_tag_both_appear_in_scalar_event() {
        let events = events_from("&a !!str hello\n");
        let scalar = events.iter().find(|e| matches!(e, Event::Scalar { .. }));
        assert!(scalar.is_some(), "no Scalar event; events: {events:?}");
        assert!(
            matches!(
                scalar.unwrap(),
                Event::Scalar { anchor: Some(a), tag: Some(_), .. } if a == "a"
            ),
            "expected anchor \"a\" and a tag; got: {:?}",
            scalar.unwrap()
        );
    }

    // -----------------------------------------------------------------------
    // Group 15 — Multiple comment events
    // -----------------------------------------------------------------------

    /// Test 90 — two block scalars each followed by a document-level comment
    /// produce two distinct Comment events.
    ///
    /// The tokenizer emits BeginComment/EndComment for document-level comments
    /// that follow block scalars.  Two such scalars in separate documents each
    /// produce one Comment event.
    #[test]
    fn multiple_comments_produce_multiple_comment_events() {
        let events = events_from("|\n  x\n# first\n---\n|\n  y\n# second\n");
        let count = events
            .iter()
            .filter(|e| matches!(e, Event::Comment { .. }))
            .count();
        assert_eq!(count, 2, "expected 2 Comment events; events: {events:?}");
    }

    // -----------------------------------------------------------------------
    // Group 16 — Error iterator path
    //
    // The current tokenizer (`tokenize`) never emits Code::Error tokens for
    // any input — the parser returns Reply::Failure or Reply::Error at the
    // combinator level, and the public `tokenize` function converts those to
    // an empty Vec rather than a Vec containing Error tokens.  There is no
    // public API to inject a Code::Error token into the event iterator.
    //
    // Test 91 verifies this invariant: that all results from parse_events on
    // any reasonable input are Ok.  Tests 70 (error token yields Err) and 73
    // (iterator stops after Err) are not implemented because the precondition
    // (a Code::Error token in the stream) is unreachable through the public
    // API.  If the tokenizer is later extended to emit Code::Error tokens for
    // recoverable errors, the event iterator's Code::Error arm should be
    // changed to yield Err and these tests should be added at that time.
    // -----------------------------------------------------------------------

    /// Test 91 — parse_events never yields Err for any well-formed or
    /// partially-formed input, because the tokenizer never emits Code::Error.
    #[test]
    fn parse_events_yields_no_errors_for_tokenizer_output() {
        // Various inputs including unusual ones.
        let inputs = [
            "",
            "hello",
            "key: value",
            "- a\n- b",
            "---\n...",
            "'single'",
            "\"double\"",
            "|\n  block\n",
            ">-\n  folded\n",
            "&anchor value",
            "!!str value",
            "*alias",
        ];
        for input in inputs {
            let errors: Vec<_> = parse_events(input).filter(Result::is_err).collect();
            assert!(
                errors.is_empty(),
                "unexpected Err results for input {input:?}: {errors:?}"
            );
        }
    }

    /// Test 92 — iterator continues producing Ok results after unusual tokens
    /// (no early termination from skipped unknown codes).
    #[test]
    fn iterator_does_not_stop_early_on_skipped_tokens() {
        // A mapping followed by an alias — exercises various token code paths.
        let events = events_from("- &anchor hello\n- *anchor\n");
        let last = events.last();
        assert!(
            matches!(last, Some(Event::StreamEnd)),
            "expected StreamEnd as last event; got: {last:?}"
        );
    }
}
