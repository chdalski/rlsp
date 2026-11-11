// SPDX-License-Identifier: MIT

//! Reads the source text of a scalar, one logical line at a time.
//!
//! Each character comes with the position it has in the document. The event
//! stream holds no map from a scalar's value to its source, so the reader
//! decodes the scalar's source inside the parser-provided span: the arithmetic
//! on parser spans that the crate boundaries allow. Positions count code
//! points, like every position inside `rlsp-yaml`.

use rlsp_yaml_parser::{ScalarStyle, Span};
use tower_lsp::lsp_types::Position;

/// One character of a scalar's value, with the source it was written as.
///
/// A scalar can hold millions of characters, so one takes 12 bytes: the
/// decoded character and its flags share one word, as a `char` needs 21 bits
/// and leaves the rest free.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourceChar {
    /// The decoded character, the width in source code points (bits 21 to 24),
    /// and whether an escape wrote it (bit 25).
    packed: u32,
    // A character lies on one source line, so one line number serves both ends.
    line: u32,
    start_column: u32,
}

const CHAR_BITS: u32 = 21;
const MAX_WIDTH: u32 = 0xF;
const ESCAPED_BIT: u32 = 1 << 25;

impl SourceChar {
    /// A character that `width` code points of source wrote, starting at
    /// `start_column` of `line`. The width of an escape is at most the ten
    /// code points of `\U` and its eight digits.
    fn new(ch: char, escaped: bool, width: u8, line: u32, start_column: u32) -> Self {
        let flags = if escaped { ESCAPED_BIT } else { 0 };
        Self {
            packed: u32::from(ch) | (u32::from(width).min(MAX_WIDTH) << CHAR_BITS) | flags,
            line,
            start_column,
        }
    }

    /// The decoded character.
    #[must_use]
    pub fn ch(&self) -> char {
        char::from_u32(self.packed & ((1 << CHAR_BITS) - 1)).unwrap_or(char::REPLACEMENT_CHARACTER)
    }

    /// Whether an escape wrote the character: a double-quoted escape sequence,
    /// or `''` in a single-quoted scalar.
    #[must_use]
    pub const fn escaped(&self) -> bool {
        self.packed & ESCAPED_BIT != 0
    }

    /// The position of the first source character that wrote this character.
    #[must_use]
    pub const fn start(&self) -> Position {
        Position {
            line: self.line,
            character: self.start_column,
        }
    }

    /// The position after the last source character that wrote this character.
    #[must_use]
    pub const fn end(&self) -> Position {
        let width = (self.packed >> CHAR_BITS) & MAX_WIDTH;
        Position {
            line: self.line,
            character: self.start_column.saturating_add(width),
        }
    }
}

impl std::fmt::Debug for SourceChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceChar")
            .field("ch", &self.ch())
            .field("escaped", &self.escaped())
            .field("start", &self.start())
            .field("end", &self.end())
            .finish()
    }
}

/// Resolves byte offsets of one text to positions.
///
/// It counts from the offset it resolved last, so asking for the offsets of a
/// document in order costs one pass over the text in all, however many offsets
/// share a long line. An offset before the last one is counted again from the
/// start of the line that holds the last one, or from the start of the text.
pub struct Positions<'a> {
    text: &'a str,
    /// Byte offset of the last resolved position.
    at: usize,
    line: u32,
    column: u32,
    /// Byte offset of the start of the line that holds `at`.
    line_start: usize,
}

impl<'a> Positions<'a> {
    /// Positions of the offsets of `text`; the first line is line 0.
    #[must_use]
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            at: 0,
            line: 0,
            column: 0,
            line_start: 0,
        }
    }

    /// The position of byte `offset`, as `LineIndex::line_column` gives it.
    /// `None` if `offset` is not on a character boundary of the text.
    pub fn at(&mut self, offset: usize) -> Option<Position> {
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        if offset < self.at {
            if offset >= self.line_start {
                self.at = self.line_start;
                self.column = 0;
            } else {
                *self = Self::new(self.text);
            }
        }
        let skip_lf = self.at > 0
            && self.text.as_bytes().get(self.at - 1) == Some(&b'\r')
            && self.text.as_bytes().get(self.at) == Some(&b'\n');
        let mut chars = self.text.get(self.at..offset)?.char_indices().peekable();
        let base = self.at;
        if skip_lf && chars.next_if(|&(_, ch)| ch == '\n').is_some() {
            self.line_start = base + 1;
        }
        while let Some((i, ch)) = chars.next() {
            match ch {
                '\r' | '\n' => {
                    let mut next = base + i + 1;
                    if ch == '\r' && chars.next_if(|&(_, c)| c == '\n').is_some() {
                        next += 1;
                    }
                    self.line = self.line.saturating_add(1);
                    self.column = 0;
                    self.line_start = next;
                }
                _ => self.column = self.column.saturating_add(1),
            }
        }
        self.at = offset;
        Some(Position::new(self.line, self.column))
    }
}

/// The content of a scalar, one logical line at a time.
///
/// A logical line is a line of the scalar's value: the source lines that an
/// escaped line break joins make up one logical line. The quotes of a quoted
/// scalar and the header line of a block scalar are no content. A scalar with
/// no content yields no line.
pub struct ScalarSource<'a> {
    /// The source text still to read, without quotes or header.
    rest: &'a str,
    line: u32,
    column: u32,
    double_quoted: bool,
    single_quoted: bool,
}

impl<'a> ScalarSource<'a> {
    /// The reader of the scalar that `loc` spans in `text`.
    ///
    /// It starts at the position `positions` gives for the span. `None` if the
    /// span does not fit `text`, or does not start and end on character
    /// boundaries.
    pub fn new(
        style: ScalarStyle,
        loc: Span,
        text: &'a str,
        positions: &mut Positions<'_>,
    ) -> Option<Self> {
        let source = text.get(usize::try_from(loc.start).ok()?..usize::try_from(loc.end).ok()?)?;
        let start = positions.at(usize::try_from(loc.start).ok()?)?;
        let (line, column) = (start.line, start.character);
        let unquoted = |quote: char| {
            source.strip_prefix(quote).map_or((source, column), |body| {
                (
                    body.strip_suffix(quote).unwrap_or(body),
                    column.saturating_add(1),
                )
            })
        };
        let (rest, line, column) = match style {
            ScalarStyle::DoubleQuoted => {
                let (rest, column) = unquoted('"');
                (rest, line, column)
            }
            ScalarStyle::SingleQuoted => {
                let (rest, column) = unquoted('\'');
                (rest, line, column)
            }
            // The span starts at the `|` or `>`; the rest of that line is the
            // header and a comment.
            ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => {
                (skip_line(source), line.saturating_add(1), 0)
            }
            ScalarStyle::Plain => (source, line, column),
        };
        Some(Self {
            rest,
            line,
            column,
            double_quoted: style == ScalarStyle::DoubleQuoted,
            single_quoted: style == ScalarStyle::SingleQuoted,
        })
    }

    /// Reads one character of the content, or an escape sequence that stands
    /// for one, and returns it.
    fn read_char(&mut self) -> Option<SourceChar> {
        let first = self.rest.chars().next()?;
        let (ch, escaped, code_points, bytes) = if self.double_quoted && first == '\\' {
            let after = self.rest.get(1..).unwrap_or("");
            // An escape that is not valid stands for U+FFFD and covers the
            // backslash only; what follows is read as characters.
            let (ch, code_points, bytes) =
                decode_escape(after).unwrap_or((char::REPLACEMENT_CHARACTER, 0, 0));
            (ch, true, 1 + code_points, 1 + bytes)
        } else if self.single_quoted && self.rest.starts_with("''") {
            ('\'', true, 2, 2)
        } else {
            (first, false, 1, first.len_utf8())
        };
        let start_column = self.column;
        // The width saturates, so that reading a character always advances.
        let width = u8::try_from(code_points).unwrap_or(u8::MAX);
        let read = SourceChar::new(ch, escaped, width, self.line, start_column);
        self.column = read.end().character;
        self.rest = self.rest.get(bytes..).unwrap_or("");
        Some(read)
    }

    /// Consumes the line break at the start of the rest, if there is one.
    fn skip_break(&mut self) -> bool {
        let Some(after) = strip_break(self.rest) else {
            return false;
        };
        self.rest = after;
        self.line = self.line.saturating_add(1);
        self.column = 0;
        true
    }

    /// Consumes a backslash and the line break after it, which join the line
    /// to the next, and the blanks that start the next line.
    fn skip_escaped_break(&mut self) -> bool {
        if !self.double_quoted {
            return false;
        }
        let Some(after_break) = self.rest.strip_prefix('\\').and_then(strip_break) else {
            return false;
        };
        let blanks = after_break
            .find(|c| !matches!(c, ' ' | '\t'))
            .unwrap_or(after_break.len());
        self.rest = after_break.get(blanks..).unwrap_or("");
        self.line = self.line.saturating_add(1);
        self.column = u32::try_from(blanks).unwrap_or(u32::MAX);
        true
    }
}

impl Iterator for ScalarSource<'_> {
    type Item = Vec<SourceChar>;

    fn next(&mut self) -> Option<Vec<SourceChar>> {
        if self.rest.is_empty() {
            return None;
        }
        let mut line = Vec::new();
        while !self.rest.is_empty() {
            if self.skip_break() {
                return Some(line);
            }
            if self.skip_escaped_break() {
                continue;
            }
            line.extend(self.read_char());
        }
        Some(line)
    }
}

/// `text` without the line break it starts with: `\n`, `\r\n`, or a lone `\r`.
/// `None` if `text` does not start with a line break.
fn strip_break(text: &str) -> Option<&str> {
    text.strip_prefix("\r\n")
        .or_else(|| text.strip_prefix(['\n', '\r']))
}

/// `text` without its first line and the break that ends it.
fn skip_line(text: &str) -> &str {
    text.find(['\n', '\r'])
        .and_then(|end| text.get(end..))
        .and_then(strip_break)
        .unwrap_or("")
}

/// Decodes the escape sequence of a double-quoted scalar (YAML 1.2.2, section
/// 5.7) that follows a backslash.
///
/// Returns the character, the number of code points of `after` it uses, and
/// the number of bytes. `None` if the sequence is not an escape: an unknown
/// code, hex digits that are missing or not ASCII hex digits, or a code point
/// that is no character.
fn decode_escape(after: &str) -> Option<(char, usize, usize)> {
    let code = after.chars().next()?;
    let digits = match code {
        'x' => 2,
        'u' => 4,
        'U' => 8,
        _ => {
            let ch = match code {
                '0' => '\0',
                'a' => '\x07',
                'b' => '\x08',
                't' | '\t' => '\t',
                'n' => '\n',
                'v' => '\x0B',
                'f' => '\x0C',
                'r' => '\r',
                'e' => '\x1B',
                ' ' | '"' | '/' | '\\' => code,
                'N' => '\u{85}',
                '_' => '\u{A0}',
                'L' => '\u{2028}',
                'P' => '\u{2029}',
                _ => return None,
            };
            return Some((ch, 1, 1));
        }
    };
    let value = after
        .get(1..=digits)?
        .chars()
        .try_fold(0_u32, |acc, c| Some((acc << 4) | c.to_digit(16)?))?;
    Some((char::from_u32(value)?, 1 + digits, 1 + digits))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rlsp_yaml_parser::node::Node;
    use rlsp_yaml_parser::{Chomp, LineIndex};
    use rstest::rstest;
    use tower_lsp::lsp_types::Range;

    use super::*;
    use crate::test_utils::{parse_docs, text_under};

    /// The style, span, and value of every scalar of `text` in document order,
    /// mapping keys included.
    fn scalars(yaml: &str) -> Vec<(ScalarStyle, Span, String)> {
        fn walk(node: &Node<Span>, out: &mut Vec<(ScalarStyle, Span, String)>) {
            match node {
                Node::Scalar {
                    style, loc, value, ..
                } => out.push((*style, *loc, value.clone())),
                Node::Mapping { entries, .. } => {
                    for (key, value) in entries {
                        walk(key, out);
                        walk(value, out);
                    }
                }
                Node::Sequence { items, .. } => items.iter().for_each(|item| walk(item, out)),
                Node::Alias { .. } => {}
            }
        }
        let mut out = Vec::new();
        for doc in parse_docs(yaml) {
            walk(&doc.root, &mut out);
        }
        out
    }

    /// The lines of the last scalar of `text`, the value of `k`.
    fn lines_of(yaml: &str) -> Vec<Vec<SourceChar>> {
        let (style, loc, _) = scalars(yaml).pop().expect("a scalar");
        ScalarSource::new(style, loc, yaml, &mut Positions::new(yaml))
            .expect("the span fits the text")
            .collect()
    }

    /// The value the parser loads for the last scalar of `text`.
    fn parser_value(yaml: &str) -> String {
        scalars(yaml).pop().expect("a scalar").2
    }

    /// The lines of the scalar that is all of `source`, read as `style`.
    fn lines_of_raw(style: ScalarStyle, source: &str) -> Vec<Vec<SourceChar>> {
        let loc = Span {
            start: 0,
            end: u32::try_from(source.len()).expect("a short source"),
        };
        ScalarSource::new(style, loc, source, &mut Positions::new(source))
            .expect("the span fits the text")
            .collect()
    }

    fn chars(lines: &[Vec<SourceChar>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.iter().map(SourceChar::ch).collect())
            .collect()
    }

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    // A scalar can hold millions of characters; the plan accepts no growth of
    // what the reader holds per character.
    #[test]
    fn a_source_char_takes_no_more_than_the_cell_it_replaced() {
        assert_eq!(size_of::<SourceChar>(), 12);
    }

    // ── Escapes ──────────────────────────────────────────────────────────────

    #[rstest]
    #[case::null("\\0", '\0')]
    #[case::bell("\\a", '\x07')]
    #[case::backspace("\\b", '\x08')]
    #[case::tab("\\t", '\t')]
    #[case::literal_tab("\\\t", '\t')]
    #[case::line_feed("\\n", '\n')]
    #[case::vertical_tab("\\v", '\x0B')]
    #[case::form_feed("\\f", '\x0C')]
    #[case::carriage_return("\\r", '\r')]
    #[case::escape("\\e", '\x1B')]
    #[case::space("\\ ", ' ')]
    #[case::double_quote("\\\"", '"')]
    #[case::slash("\\/", '/')]
    #[case::backslash("\\\\", '\\')]
    #[case::next_line("\\N", '\u{85}')]
    #[case::no_break_space("\\_", '\u{A0}')]
    #[case::line_separator("\\L", '\u{2028}')]
    #[case::paragraph_separator("\\P", '\u{2029}')]
    #[case::hex_ascii("\\x41", 'A')]
    #[case::hex_latin("\\xe9", 'é')]
    #[case::hex_upper_case("\\xE9", 'é')]
    #[case::unicode_ascii("\\u0041", 'A')]
    #[case::unicode_euro("\\u20AC", '€')]
    #[case::unicode_line_separator("\\u2028", '\u{2028}')]
    #[case::long_ascii("\\U00000041", 'A')]
    #[case::long_astral("\\U0001F600", '\u{1F600}')]
    #[case::long_last_code_point("\\U0010FFFF", '\u{10FFFF}')]
    #[case::hex_line_feed("\\x0A", '\n')]
    #[case::unicode_last_before_the_surrogates("\\uD7FF", '\u{D7FF}')]
    #[case::unicode_first_after_the_surrogates("\\uE000", '\u{E000}')]
    #[case::unicode_mixed_case("\\u20aC", '€')]
    #[case::long_mixed_case("\\U0001f60A", '\u{1F60A}')]
    fn a_double_quoted_escape_decodes_to_the_value_the_parser_loads(
        #[case] escape: &str,
        #[case] expected: char,
    ) {
        let text = format!("k: \"{escape}\"\n");

        let lines = lines_of(&text);

        assert_eq!(parser_value(&text), expected.to_string());
        assert_eq!(lines.len(), 1);
        let [only] = lines[0].as_slice() else {
            panic!("one character, got {:?}", lines[0]);
        };
        assert_eq!(only.ch(), expected);
        assert!(only.escaped());
        assert_eq!(only.start(), pos(0, 4));
        let width = u32::try_from(escape.chars().count()).expect("a short escape");
        assert_eq!(only.end(), pos(0, 4 + width));
    }

    #[rstest]
    #[case::plain("k: a\\x41\\n\n", "a\\x41\\n")]
    #[case::single_quoted("k: 'a\\x41\\n'\n", "a\\x41\\n")]
    #[case::literal("k: |\n  a\\x41\\n\n", "  a\\x41\\n")]
    #[case::folded("k: >\n  a\\x41\\n\n", "  a\\x41\\n")]
    fn a_backslash_is_a_literal_character_outside_double_quotes(
        #[case] text: &str,
        #[case] content: &str,
    ) {
        let lines = lines_of(text);

        assert_eq!(chars(&lines), [content]);
        assert!(lines[0].iter().all(|c| !c.escaped()));
    }

    // Every escape sequence of the single-quoted and block styles is a
    // character of its own; only `''` is an escape.
    #[rstest]
    #[case::inside("k: 'it''s'\n", "it's", &[(2, 6, 8)])]
    #[case::only("k: ''''\n", "'", &[(0, 4, 6)])]
    #[case::before_the_closing_quote("k: 'a'''\n", "a'", &[(1, 5, 7)])]
    #[case::after_the_opening_quote("k: '''a'\n", "'a", &[(0, 4, 6)])]
    fn a_doubled_single_quote_is_one_escaped_quote(
        #[case] text: &str,
        #[case] value: &str,
        #[case] quotes: &[(usize, u32, u32)],
    ) {
        let lines = lines_of(text);

        assert_eq!(chars(&lines), [value]);
        assert_eq!(parser_value(text), value);
        for (index, start, end) in quotes {
            let c = lines[0][*index];
            assert!(c.escaped());
            assert_eq!((c.start(), c.end()), (pos(0, *start), pos(0, *end)));
        }
        let escaped = lines[0].iter().filter(|c| c.escaped()).count();
        assert_eq!(escaped, quotes.len());
    }

    // Only a backslash that no other backslash escapes joins the line to the
    // next; an escaped blank or tab at the end of a line is an escape of its
    // own, and the line break after it folds.
    #[rstest]
    #[case::even_run("k: \"a\\\\\n  b\"\n", &["a\\", "  b"], "a\\ b")]
    #[case::odd_run("k: \"a\\\\\\\n  b\"\n", &["a\\b"], "a\\b")]
    #[case::escaped_blank("k: \"a\\ \n  b\"\n", &["a ", "  b"], "a  b")]
    #[case::escaped_tab("k: \"a\\\t\n  b\"\n", &["a\t", "  b"], "a\t b")]
    fn backslash_runs_before_a_line_break(
        #[case] text: &str,
        #[case] expected: &[&str],
        #[case] value: &str,
    ) {
        assert_eq!(chars(&lines_of(text)), expected);
        assert_eq!(parser_value(text), value);
    }

    // The `''` of the single-quoted style is no escape in any other style.
    #[test]
    fn a_doubled_single_quote_is_two_characters_in_a_double_quoted_scalar() {
        let text = "k: \"it''s\"\n";

        let lines = lines_of(text);

        assert_eq!(chars(&lines), ["it''s"]);
        assert_eq!(parser_value(text), "it''s");
        assert!(lines[0].iter().all(|c| !c.escaped()));
    }

    #[rstest]
    #[case::two_pairs("k: 'a''''b'\n", "a''b", 2)]
    #[case::pair_before_a_line_break("k: 'x''\n  y'\n", "x' y", 1)]
    fn a_run_of_doubled_single_quotes_is_one_escaped_quote_per_pair(
        #[case] text: &str,
        #[case] value: &str,
        #[case] pairs: usize,
    ) {
        let lines = lines_of(text);

        let escaped: Vec<_> = lines.iter().flatten().filter(|c| c.escaped()).collect();
        assert_eq!(escaped.len(), pairs);
        for c in escaped {
            assert_eq!((c.ch(), c.end().character - c.start().character), ('\'', 2));
        }
        assert_eq!(parser_value(text), value);
    }

    // ── Line breaks and joins ────────────────────────────────────────────────

    #[rstest]
    #[case::lf("\n")]
    #[case::crlf("\r\n")]
    #[case::cr("\r")]
    fn an_escaped_line_break_joins_the_lines_and_drops_the_blanks_after_it(#[case] br: &str) {
        let text = format!("k: \"a\\{br}  b\"{br}");

        let lines = lines_of(&text);

        assert_eq!(chars(&lines), ["ab"]);
        assert_eq!(parser_value(&text), "ab");
        assert_eq!(
            (lines[0][0].start(), lines[0][0].end()),
            (pos(0, 4), pos(0, 5))
        );
        assert_eq!(
            (lines[0][1].start(), lines[0][1].end()),
            (pos(1, 2), pos(1, 3))
        );
    }

    #[rstest]
    #[case::spaces("k: \"a\\\n  b\"\n", "ab")]
    #[case::tab_and_spaces("k: \"a\\\n \t b\"\n", "ab")]
    #[case::blanks_before_the_backslash_stay("k: \"a \\\n  b\"\n", "a b")]
    #[case::chain("k: \"a\\\n  \\\n  b\"\n", "ab")]
    #[case::chain_to_the_closing_quote("k: \"a\\\n  \"\n", "a")]
    fn the_blanks_around_an_escaped_break_follow_the_value_the_parser_loads(
        #[case] text: &str,
        #[case] value: &str,
    ) {
        assert_eq!(chars(&lines_of(text)), [value]);
        assert_eq!(parser_value(text), value);
    }

    #[test]
    fn an_empty_line_after_an_escaped_break_starts_a_new_logical_line() {
        let text = "k: \"a\\\n\n  b\"\n";

        assert_eq!(chars(&lines_of(text)), ["a", "  b"]);
    }

    #[rstest]
    #[case::double_quoted("k: \"x\n  red\n  blue\"\n")]
    #[case::single_quoted("k: 'x\n  red\n  blue'\n")]
    #[case::plain("k: x\n  red\n  blue\n")]
    fn unescaped_line_breaks_yield_separate_lines(#[case] text: &str) {
        let lines = lines_of(text);

        assert_eq!(chars(&lines), ["x", "  red", "  blue"]);
        assert_eq!(lines[1][2].start(), pos(1, 2));
        assert_eq!(lines[1][4].end(), pos(1, 5));
    }

    #[test]
    fn a_quoted_line_with_only_the_closing_quote_has_no_quote() {
        assert_eq!(chars(&lines_of("k: \"x\n  \"\n")), ["x", "  "]);
    }

    #[rstest]
    #[case::literal("a: |\n  red\n")]
    #[case::folded_strip("a: >-\n  red\n")]
    #[case::indentation_indicator("a: |2+\n    red\n")]
    #[case::header_comment("a: | # x\n  red\n")]
    fn a_block_scalars_header_line_is_not_content(#[case] text: &str) {
        let lines = lines_of(text);

        assert_eq!(lines.len(), 1);
        assert!(chars(&lines)[0].ends_with("red"));
        assert_eq!(lines[0][0].start(), pos(1, 0));
        assert!(!chars(&lines)[0].contains(['|', '>', '#']));
    }

    #[rstest]
    #[case::no_content("a: |\n")]
    #[case::next_entry("a: |\nb: x\n")]
    #[case::header_without_break("a: |")]
    fn a_block_scalar_without_content_yields_no_line(#[case] text: &str) {
        let (style, loc, _) = scalars(text).remove(1);

        let source = ScalarSource::new(style, loc, text, &mut Positions::new(text)).unwrap();

        assert_eq!(source.count(), 0);
    }

    #[test]
    fn a_block_scalar_keeps_quotes_and_backslashes_as_characters() {
        let lines = lines_of("a: |\n  \"x\\\"y\n");

        assert_eq!(chars(&lines), ["  \"x\\\"y"]);
        assert!(lines[0].iter().all(|c| !c.escaped()));
    }

    #[rstest]
    #[case::double_quoted("k: \"\"\n")]
    #[case::single_quoted("k: ''\n")]
    fn an_empty_quoted_scalar_yields_no_line(#[case] text: &str) {
        assert!(lines_of(text).is_empty());
    }

    #[rstest]
    #[case::double_quoted("k: \"red\"\n")]
    #[case::single_quoted("k: 'red'\n")]
    fn quotes_are_not_content(#[case] text: &str) {
        let lines = lines_of(text);

        assert_eq!(chars(&lines), ["red"]);
        assert_eq!(lines[0][0].start(), pos(0, 4));
        assert_eq!(lines[0][2].end(), pos(0, 7));
    }

    // ── Invalid and truncated input ──────────────────────────────────────────

    #[rstest]
    #[case::unknown_code("\"\\q\"", "\u{FFFD}q")]
    #[case::short_hex("\"\\x4\"", "\u{FFFD}x4")]
    #[case::not_hex("\"\\xZZ\"", "\u{FFFD}xZZ")]
    #[case::short_unicode("\"\\u12\"", "\u{FFFD}u12")]
    #[case::surrogate("\"\\uD800\"", "\u{FFFD}uD800")]
    #[case::low_surrogate("\"\\uDFFF\"", "\u{FFFD}uDFFF")]
    #[case::beyond_unicode("\"\\U00110000\"", "\u{FFFD}U00110000")]
    #[case::sign_in_the_digits("\"\\u+041\"", "\u{FFFD}u+041")]
    #[case::minus_in_the_digits("\"\\u-041\"", "\u{FFFD}u-041")]
    #[case::blank_in_the_digits("\"\\u 041\"", "\u{FFFD}u 041")]
    #[case::multibyte_in_hex("\"\\xé9\"", "\u{FFFD}xé9")]
    #[case::multibyte_inside_unicode("\"\\u00é9\"", "\u{FFFD}u00é9")]
    #[case::multibyte_code("\"\\é\"", "\u{FFFD}é")]
    #[case::astral_in_hex("\"\\x😀\"", "\u{FFFD}x😀")]
    #[case::astral_inside_long("\"\\U0001F60😀\"", "\u{FFFD}U0001F60😀")]
    fn an_invalid_escape_is_a_replacement_character_for_the_backslash_only(
        #[case] source: &str,
        #[case] expected: &str,
    ) {
        // The parser rejects these documents, so no color scan reaches them; the
        // reader's replacement of the backslash is its own decision (`read_char`).
        assert!(rlsp_yaml_parser::load(&format!("k: {source}\n")).is_err());

        let lines = lines_of_raw(ScalarStyle::DoubleQuoted, source);

        assert_eq!(chars(&lines), [expected]);
        let first = lines[0].iter().find(|c| c.escaped()).expect("an escape");
        assert_eq!((first.start().character, first.end().character), (1, 2));
        assert_eq!(lines[0].iter().filter(|c| c.escaped()).count(), 1);
    }

    #[rstest]
    #[case::opening_double_quote(ScalarStyle::DoubleQuoted, "\"", &[])]
    #[case::opening_single_quote(ScalarStyle::SingleQuoted, "'", &[])]
    #[case::empty_plain(ScalarStyle::Plain, "", &[])]
    #[case::empty_double_quoted(ScalarStyle::DoubleQuoted, "", &[])]
    #[case::literal_header_only(ScalarStyle::Literal(Chomp::Clip), "|", &[])]
    #[case::backslash_at_the_end(ScalarStyle::DoubleQuoted, "\"abc\\", &["abc\u{FFFD}"])]
    #[case::unclosed_double_quoted(ScalarStyle::DoubleQuoted, "\"abc", &["abc"])]
    #[case::escaped_closing_quote(ScalarStyle::DoubleQuoted, "\"abc\\\"", &["abc\u{FFFD}"])]
    #[case::quote_at_the_end_of_a_plain(ScalarStyle::Plain, "a\"", &["a\""])]
    #[case::lone_escaped_break(ScalarStyle::DoubleQuoted, "\"a\\\n", &["a"])]
    fn a_truncated_span_does_not_panic(
        #[case] style: ScalarStyle,
        #[case] source: &str,
        #[case] expected: &[&str],
    ) {
        let lines = lines_of_raw(style, source);

        assert_eq!(chars(&lines), expected);
    }

    #[rstest]
    #[case::end_past_the_text("ab", 0, 3)]
    #[case::end_before_the_start("ab", 2, 1)]
    #[case::start_inside_a_character("éé", 1, 4)]
    #[case::end_inside_a_character("éé", 0, 1)]
    #[case::both_past_the_text("éé", 5, 9)]
    #[case::unrepresentable_end("ab", 0, u32::MAX)]
    fn a_span_that_does_not_fit_the_text_has_no_reader(
        #[case] text: &str,
        #[case] start: u32,
        #[case] end: u32,
    ) {
        let loc = Span { start, end };

        let source = ScalarSource::new(ScalarStyle::Plain, loc, text, &mut Positions::new(text));

        assert!(source.is_none());
    }

    // ── Positions ────────────────────────────────────────────────────────────

    /// The position `LineIndex` gives the byte offset of `marker` in `text`.
    fn indexed(yaml: &str, marker: char) -> Position {
        let (line, column) =
            LineIndex::new(yaml).line_column(u32::try_from(yaml.find(marker).unwrap()).unwrap());
        Position::new(line - 1, column)
    }

    #[rstest]
    #[case::plain("k: aZ<BR>")]
    #[case::plain_over_lines("k: x<BR>  Z<BR>")]
    #[case::single_quoted("k: 'a''Z'<BR>")]
    #[case::double_quoted("k: \"😀\\x41Z\"<BR>")]
    #[case::double_quoted_over_lines("k: \"a<BR>  Z\"<BR>")]
    #[case::escaped_break("k: \"a\\<BR>  Z\"<BR>")]
    #[case::literal("k: |<BR>  é😀Z<BR>")]
    #[case::folded("k: ><BR>  é😀Z<BR>")]
    #[case::literal_after_entries("a: 1<BR>b: 😀<BR>k: |<BR>  x<BR>  Z<BR>")]
    #[case::literal_with_a_blank_line("k: |<BR>  x<BR><BR>  Z<BR>")]
    #[case::folded_with_a_blank_line("k: ><BR>  x<BR><BR>  Z<BR>")]
    #[case::plain_over_lines_with_an_astral("k: 😀<BR>  Z<BR>")]
    #[case::escaped_break_then_a_blank_line("k: \"a\\<BR><BR>  Z\"<BR>")]
    #[case::chain_of_escaped_breaks("k: \"a\\<BR>  \\<BR>  Z\"<BR>")]
    #[case::tab_and_spaces_after_an_escaped_break("k: \"a\\<BR> \t Z\"<BR>")]
    #[case::astral_before_an_escaped_break("k: \"😀\\<BR>  é Z\"<BR>")]
    fn a_characters_position_is_the_position_the_line_index_gives_its_source_offset(
        #[case] template: &str,
        #[values("\n", "\r\n", "\r")] br: &str,
        #[values("", "\u{feff}")] bom: &str,
    ) {
        let text = format!("{bom}{}", template.replace("<BR>", br));

        let lines = lines_of(&text);

        let found = lines
            .iter()
            .flatten()
            .find(|c| c.ch() == 'Z')
            .expect("the marker");
        let expected = indexed(&text, 'Z');
        assert_eq!(found.start(), expected);
        assert_eq!(
            found.end(),
            Position::new(expected.line, expected.character + 1)
        );
        assert!(!found.escaped());
    }

    #[rstest]
    #[case::plain("k: aé😀b<BR>")]
    #[case::single_quoted("k: 'a''é😀b'<BR>")]
    #[case::double_quoted("k: \"a\\x41é😀\\nb\"<BR>")]
    #[case::escaped_break("k: \"a\\<BR>  é😀b\"<BR>")]
    #[case::literal("k: |<BR>  é😀b<BR>")]
    #[case::folded("k: ><BR>  é😀b<BR>")]
    #[case::double_quoted_over_lines("k: \"a<BR>  é😀b\"<BR>")]
    #[case::plain_over_lines("k: a<BR>  é😀b<BR>")]
    fn the_text_under_a_characters_range_is_its_source(
        #[case] template: &str,
        #[values("\n", "\r\n", "\r")] br: &str,
        #[values("", "\u{feff}")] bom: &str,
    ) {
        let text = format!("{bom}{}", template.replace("<BR>", br));

        for c in lines_of(&text).iter().flatten() {
            let range = Range::new(c.start(), c.end());
            let source = text_under(&text, range);
            if c.escaped() {
                assert!(
                    source.starts_with('\\') || source == "''",
                    "{c:?} {source:?}"
                );
            } else {
                assert_eq!(source, c.ch().to_string(), "{c:?}");
            }
        }
    }

    const MANY_SCALARS: &str = "\
a: x
---
b:
  - 'p'
  - \"q\"
  - [r, 's', \"t\"]
  - { u: v, \"w\": 😀 }
  - |
    block
  - >-
    é
---
- é😀 plain
- k: \"é😀\"
";

    #[rstest]
    #[case::forward(false, 1)]
    #[case::backward(true, 1)]
    #[case::each_twice(false, 2)]
    #[case::backward_each_twice(true, 2)]
    fn start_positions_equal_the_line_index_in_any_order(
        #[case] backward: bool,
        #[case] repeat: usize,
    ) {
        let docs = parse_docs(MANY_SCALARS);
        assert_eq!(docs.len(), 3);
        let mut starts: Vec<u32> = scalars(MANY_SCALARS).iter().map(|s| s.1.start).collect();
        if backward {
            starts.reverse();
        }
        let index = LineIndex::new(MANY_SCALARS);
        let mut positions = Positions::new(MANY_SCALARS);

        for start in starts.iter().flat_map(|s| std::iter::repeat_n(s, repeat)) {
            let (line, column) = index.line_column(*start);
            assert_eq!(
                positions.at(*start as usize),
                Some(Position::new(line - 1, column)),
                "offset {start}"
            );
        }
    }

    #[test]
    fn a_position_inside_a_multibyte_character_or_past_the_text_is_none() {
        let mut positions = Positions::new("é\n");

        assert_eq!(positions.at(1), None);
        assert_eq!(positions.at(9), None);
        assert_eq!(positions.at(2), Some(Position::new(0, 1)));
    }

    #[test]
    fn a_position_between_the_cr_and_the_lf_of_a_line_break_stays_on_the_line() {
        let text = "a\r\nb";
        let index = LineIndex::new(text);
        let mut positions = Positions::new(text);

        // Offsets 2 and 3 split the break and follow it.
        for offset in [2_u32, 3, 4] {
            let (line, column) = index.line_column(offset);
            assert_eq!(
                positions.at(offset as usize),
                Some(Position::new(line - 1, column)),
                "offset {offset}"
            );
        }
    }

    #[rstest]
    #[timeout(std::time::Duration::from_secs(5))]
    fn the_positions_of_many_offsets_on_one_line_are_found_in_linear_time() {
        let text = "😀 red ".repeat(200_000);
        let mut positions = Positions::new(&text);

        let last = (0..200_000)
            .map(|n| positions.at(n * "😀 red ".len() + 5))
            .last();

        assert_eq!(last, Some(Some(Position::new(0, 199_999 * 6 + 2))));
    }

    // U+0085, U+2028, and U+2029 are no line breaks in YAML 1.2 (section 5.4).
    #[rstest]
    #[case::plain("k: a\u{2028}b\u{85}Z\u{2029}\n")]
    #[case::single_quoted("k: 'a\u{2028}b\u{85}Z\u{2029}'\n")]
    #[case::double_quoted("k: \"a\u{2028}b\u{85}Z\u{2029}\"\n")]
    #[case::literal("k: |\n  a\u{2028}b\u{85}Z\u{2029}\n")]
    #[case::folded("k: >\n  a\u{2028}b\u{85}Z\u{2029}\n")]
    fn next_line_and_the_separators_are_ordinary_characters(#[case] text: &str) {
        let lines = lines_of(text);

        assert_eq!(lines.len(), 1);
        assert!(lines[0].iter().all(|c| !c.escaped()));
        let z = lines[0].iter().find(|c| c.ch() == 'Z').expect("the marker");
        assert_eq!(z.start(), indexed(text, 'Z'));
    }

    // ── Properties ───────────────────────────────────────────────────────────

    fn escape_or_character() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec![
            "a",
            "é",
            "😀",
            " ",
            "\\x41",
            "\\xe9",
            "\\U0001F600",
            "\\n",
            "\\\\",
            "\\\"",
            "\\t",
            "\\/",
            "\\u20AC",
            "\\_",
        ])
    }

    fn text_token() -> impl Strategy<Value = &'static str> {
        // U+0085, U+2028, and U+2029 are no line breaks in YAML 1.2.
        prop::sample::select(vec![
            "a", "é", "😀", " ", "\n", "\r", "\r\n", "\u{85}", "\u{2028}", "\u{2029}",
        ])
    }

    fn source_token() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec![
            "\\", "\"", "'", "''", "\n", "\r", "\r\n", "x", "u", "U", "4", "F", "é", "😀", " ",
            "\t", "|", ">", "\u{2028}",
        ])
    }

    proptest! {
        #[test]
        fn the_characters_of_a_double_quoted_scalar_are_the_value_the_parser_loads(
            parts in prop::collection::vec(escape_or_character(), 0..40),
        ) {
            let text = format!("k: \"{}\"\n", parts.concat());

            let lines = lines_of(&text);

            let decoded: String = lines.iter().flatten().map(SourceChar::ch).collect();
            prop_assert_eq!(decoded, parser_value(&text));
            for c in lines.iter().flatten() {
                let source = text_under(&text, Range::new(c.start(), c.end()));
                if c.escaped() {
                    prop_assert!(source.starts_with('\\'));
                } else {
                    prop_assert_eq!(source, c.ch().to_string());
                }
            }
        }

        #[test]
        fn the_positions_of_any_offset_sequence_equal_the_line_index(
            tokens in prop::collection::vec(text_token(), 0..30),
            bom in any::<bool>(),
            offsets in prop::collection::vec(0_usize..140, 0..40),
        ) {
            let text = format!("{}{}", if bom { "\u{feff}" } else { "" }, tokens.concat());
            let index = LineIndex::new(&text);
            let mut positions = Positions::new(&text);

            // One reader answers forward, backward, repeated, and invalid
            // offsets in turn; an invalid offset must not spoil the next answer.
            for offset in offsets {
                let expected = text.is_char_boundary(offset).then(|| {
                    let (line, column) = index.line_column(u32::try_from(offset).unwrap());
                    Position::new(line - 1, column)
                });
                prop_assert_eq!(positions.at(offset), expected, "offset {}", offset);
            }
        }

        #[test]
        fn an_arbitrary_source_never_panics_and_reads_ordered_positions(
            parts in prop::collection::vec(source_token(), 0..40),
            prefix in prop::sample::select(vec!["", "a: ", "é\n", "\r\n😀 "]),
        ) {
            let source = parts.concat();
            for (style, scalar) in [
                (ScalarStyle::DoubleQuoted, format!("\"{source}\"")),
                (ScalarStyle::SingleQuoted, format!("'{source}'")),
                (ScalarStyle::Plain, source.clone()),
                (ScalarStyle::Literal(Chomp::Clip), format!("|\n{source}")),
                (ScalarStyle::Folded(Chomp::Strip), format!(">\r\n{source}")),
            ] {
                let text = format!("{prefix}{scalar}");
                let loc = Span {
                    start: u32::try_from(prefix.len()).unwrap(),
                    end: u32::try_from(text.len()).unwrap(),
                };
                let lines: Vec<_> = ScalarSource::new(style, loc, &text, &mut Positions::new(&text))
                    .expect("the span fits the text")
                    .collect();

                let mut previous_end = Position::new(0, 0);
                for c in lines.iter().flatten() {
                    prop_assert!(previous_end <= c.start(), "{:?} {:?}", style, text);
                    prop_assert!(c.start() <= c.end());
                    previous_end = c.end();
                    let under = text_under(&text, Range::new(c.start(), c.end()));
                    if c.escaped() {
                        prop_assert!(under.starts_with('\\') || under == "''", "{:?} {:?}", style, text);
                    } else {
                        prop_assert_eq!(c.end().character - c.start().character, 1);
                        prop_assert_eq!(under, c.ch().to_string(), "{:?} {:?}", style, text);
                    }
                }
            }
        }
    }
}
