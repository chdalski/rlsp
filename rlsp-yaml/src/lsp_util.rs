// SPDX-License-Identifier: MIT

//! Shared helpers for converting parser `Span` byte offsets to LSP positions.

use rlsp_yaml_parser::{LineBuffer, LineIndex, Span};
use tower_lsp::lsp_types::{Position, Range, TextEdit};

/// The source lines of `text`, without their line breaks, in order.
///
/// A line ends at `\n`, `\r\n`, or a lone `\r` (YAML 1.2 production [28]
/// `b-break`), the way [`LineIndex`] numbers lines: the `n`th item is the line
/// that LSP line `n` starts. A break at the end of `text` adds no empty line.
/// The iterator is lazy and each item borrows from `text`.
pub fn source_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut lines = LineBuffer::new(text);
    std::iter::from_fn(move || lines.consume_next().map(|line| line.content))
}

/// The source lines of `text` with their own line breaks, in order.
///
/// The lines are cut at the offsets [`LineBuffer`] reports, so they follow one
/// another without a gap and their concatenation is `text`. `None` only if an
/// offset does not fall on a character boundary, which the buffer does not
/// produce.
fn lines_with_breaks(text: &str) -> Option<Vec<&str>> {
    let mut buffer = LineBuffer::new(text);
    let mut starts = Vec::new();
    while let Some(line) = buffer.consume_next() {
        starts.push(line.offset);
    }
    let ends = starts.iter().skip(1).copied().chain([text.len()]);
    starts
        .iter()
        .zip(ends)
        .map(|(&start, end)| text.get(start..end))
        .collect()
}

/// The LSP position of the end of `text`.
///
/// After a line break, or in an empty text, that is column 0 of the line
/// below the last. Otherwise it is the end of the last line, with the column
/// in UTF-16 units, as the server negotiates no other `positionEncoding` and
/// clients read columns that way. A column counted in code points would end
/// the edit before the tail of a last line that holds a character outside
/// the BMP.
fn end_position(text: &str) -> Option<Position> {
    let (count, last_line) =
        source_lines(text).fold((0_usize, ""), |(count, _), line| (count + 1, line));
    if text.is_empty() || text.ends_with(['\n', '\r']) {
        Some(Position::new(u32::try_from(count).ok()?, 0))
    } else {
        Some(Position::new(
            u32::try_from(count - 1).ok()?,
            u32::try_from(last_line.encode_utf16().count()).ok()?,
        ))
    }
}

/// The lines in which a document and its formatted text differ, as line
/// indices: `start..end` of the document is replaced by `start..formatted_end`
/// of the formatted text. Everything before `start` and from `end` on is
/// identical in both, line breaks included.
struct ChangedRegion {
    start: usize,
    end: usize,
    formatted_end: usize,
}

impl ChangedRegion {
    /// The region between the leading and the trailing lines the two texts
    /// share. The leading run is taken first, so the two runs never overlap.
    fn between(original: &[&str], formatted: &[&str]) -> Self {
        let start = original
            .iter()
            .zip(formatted)
            .take_while(|(line, formatted_line)| line == formatted_line)
            .count();
        let tail = original
            .iter()
            .skip(start)
            .rev()
            .zip(formatted.iter().skip(start).rev())
            .take_while(|(line, formatted_line)| line == formatted_line)
            .count();
        Self {
            start,
            end: original.len() - tail,
            formatted_end: formatted.len() - tail,
        }
    }

    /// Whether the lines `first..=last` hold a line of the region or, for a
    /// region without a line of the document, a line next to it.
    const fn touched_by(&self, first: usize, last: usize) -> bool {
        if self.start < self.end {
            first < self.end && last >= self.start
        } else {
            first <= self.start && last >= self.start.saturating_sub(1)
        }
    }
}

/// The edit that replaces the changed region of `text` with its counterpart in
/// `formatted`, the full-format output, when the lines `first_line..=last_line`
/// touch that region; the end character of a request is not read.
///
/// The shared leading and trailing lines stay in place, so applying the edit
/// gives exactly `formatted` whatever formatting does to the line count. The
/// edit may reach beyond the requested lines. There is no edit when the texts
/// are equal, when the lines do not touch the region, when `first_line` lies
/// past the last line of `text`, or when `last_line` is before `first_line`.
#[must_use]
pub fn range_format_edit(
    text: &str,
    formatted: &str,
    first_line: u32,
    last_line: u32,
) -> Option<TextEdit> {
    if text == formatted || first_line > last_line {
        return None;
    }
    let first = usize::try_from(first_line).ok()?;
    let last = usize::try_from(last_line).ok()?;
    let original_lines = lines_with_breaks(text)?;
    if first >= original_lines.len() {
        return None;
    }
    let formatted_lines = lines_with_breaks(formatted)?;

    let region = ChangedRegion::between(&original_lines, &formatted_lines);
    if !region.touched_by(first, last) {
        return None;
    }

    let end = if region.end < original_lines.len() {
        Position::new(u32::try_from(region.end).ok()?, 0)
    } else {
        end_position(text)?
    };
    Some(TextEdit {
        range: Range::new(Position::new(u32::try_from(region.start).ok()?, 0), end),
        new_text: formatted_lines
            .get(region.start..region.formatted_end)?
            .concat(),
    })
}

/// The edit that replaces the whole of `text` with `formatted`, or `None` when
/// they are equal.
#[must_use]
pub fn full_format_edit(text: &str, formatted: &str) -> Option<TextEdit> {
    if text == formatted {
        return None;
    }
    Some(TextEdit {
        range: Range::new(Position::new(0, 0), end_position(text)?),
        new_text: formatted.to_string(),
    })
}

/// Convert a `Span` byte offset to an LSP `Position` (0-based line and character).
///
/// The `LineIndex` is used to resolve the byte offset to `(line, column)`. The
/// parser uses 1-based line numbers; LSP uses 0-based, so line is decremented by 1.
#[must_use]
pub fn offset_to_lsp(offset: u32, idx: &LineIndex) -> Position {
    let (line, col) = idx.line_column(offset);
    Position::new(line.saturating_sub(1), col)
}

/// Convert a parser `Span` to an LSP `Range`.
#[must_use]
pub fn span_to_lsp(span: Span, idx: &LineIndex) -> Range {
    Range::new(offset_to_lsp(span.start, idx), offset_to_lsp(span.end, idx))
}

#[cfg(test)]
mod tests {
    use rlsp_yaml_parser::LineIndex;
    use rstest::rstest;

    use super::*;

    fn idx(source: &str) -> LineIndex {
        LineIndex::new(source)
    }

    // Saturating-sub contract: byte offset 0 on the first line (1-based line 1)
    // must produce LSP line 0, not underflowing to u32::MAX.
    #[test]
    fn offset_to_lsp_line_one_saturates_to_zero() {
        let source = "abc\n";
        let i = idx(source);
        let pos = offset_to_lsp(0, &i);
        assert_eq!(pos.line, 0, "line 1 (1-based) must map to line 0 (0-based)");
        assert_eq!(pos.character, 0);
    }

    // Second line: 1-based line 2 maps to 0-based line 1.
    #[test]
    fn offset_to_lsp_second_line_correct() {
        let source = "abc\ndef\n";
        let i = idx(source);
        // "def" starts at byte 4
        let pos = offset_to_lsp(4, &i);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    // Multibyte: character offsets are codepoint-based, not byte-based.
    #[test]
    fn offset_to_lsp_multibyte_character_is_codepoint_count() {
        // "日本語" is 9 bytes but 3 codepoints. Offset 9 is just past the last character.
        let source = "日本語\n";
        let i = idx(source);
        let pos = offset_to_lsp(9, &i);
        assert_eq!(pos.line, 0);
        assert_eq!(
            pos.character, 3,
            "character must be 3 codepoints, not 9 bytes"
        );
    }

    // span_to_lsp maps start and end independently.
    #[test]
    fn span_to_lsp_maps_start_and_end() {
        let source = "key: val\n";
        let i = idx(source);
        // "key" occupies bytes 0..3
        let range = span_to_lsp(Span { start: 0, end: 3 }, &i);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 3);
    }

    // ── source_lines ─────────────────────────────────────────────────────────

    #[rstest]
    #[case::empty_input_yields_no_lines("", &[])]
    #[case::single_line_without_break("a", &["a"])]
    #[case::single_line_lf("a\n", &["a"])]
    #[case::single_line_crlf("a\r\n", &["a"])]
    #[case::single_line_lone_cr("a\r", &["a"])]
    #[case::two_lines_lf("a\nb", &["a", "b"])]
    #[case::two_lines_crlf("a\r\nb", &["a", "b"])]
    #[case::two_lines_lone_cr("a\rb", &["a", "b"])]
    #[case::trailing_lf_adds_no_line("a\nb\n", &["a", "b"])]
    #[case::trailing_crlf_adds_no_line("a\r\nb\r\n", &["a", "b"])]
    #[case::trailing_lone_cr_adds_no_line("a\rb\r", &["a", "b"])]
    #[case::mixed_breaks("a\nb\r\nc\rd", &["a", "b", "c", "d"])]
    #[case::consecutive_lf("\n\n", &["", ""])]
    #[case::consecutive_lone_cr("\r\r", &["", ""])]
    #[case::consecutive_crlf("\r\n\r\n", &["", ""])]
    #[case::crlf_then_lone_cr("\r\n\r", &["", ""])]
    #[case::lf_then_lone_cr("\n\r", &["", ""])]
    #[case::empty_line_between_lines("a\r\n\nb", &["a", "", "b"])]
    #[case::cr_before_lf_is_one_break("a\r\nb", &["a", "b"])]
    #[case::unicode_separators_are_not_breaks(
        "a\u{85}b\u{2028}c\u{2029}d\n",
        &["a\u{85}b\u{2028}c\u{2029}d"]
    )]
    fn source_lines_splits_at_yaml_line_breaks(#[case] text: &str, #[case] expected: &[&str]) {
        let lines: Vec<&str> = source_lines(text).collect();

        assert_eq!(lines, expected);
        assert!(
            lines.iter().all(|line| !line.contains(['\r', '\n'])),
            "no line keeps a break: {lines:?}"
        );
    }

    // ── lines_with_breaks ────────────────────────────────────────────────────

    #[rstest]
    #[case::empty_text("", &[])]
    #[case::single_line_without_break("a", &["a"])]
    #[case::terminated_lines("a\nb\n", &["a\n", "b\n"])]
    #[case::crlf("a\r\nb", &["a\r\n", "b"])]
    #[case::lone_cr("a\rb\r", &["a\r", "b\r"])]
    #[case::mixed_breaks("a\nb\r\nc\rd", &["a\n", "b\r\n", "c\r", "d"])]
    #[case::consecutive_lf("\n\n", &["\n", "\n"])]
    #[case::crlf_then_lone_cr("\r\n\r", &["\r\n", "\r"])]
    #[case::consecutive_lone_cr("\r\r", &["\r", "\r"])]
    #[case::unicode_separators_are_not_breaks("a\u{85}b\nc", &["a\u{85}b\n", "c"])]
    #[case::multibyte("é\nb", &["é\n", "b"])]
    #[case::a_byte_order_mark_stays_in_the_first_line("\u{feff}a\nb", &["\u{feff}a\n", "b"])]
    fn lines_with_breaks_cuts_after_each_break(#[case] text: &str, #[case] expected: &[&str]) {
        assert_eq!(lines_with_breaks(text).expect("lines"), expected);
    }

    // ── changed region ───────────────────────────────────────────────────────

    /// `(start, end, formatted_end)` of the region between `document` and `formatted`.
    fn region_of(document: &str, formatted: &str) -> (usize, usize, usize) {
        let region = ChangedRegion::between(
            &lines_with_breaks(document).expect("lines"),
            &lines_with_breaks(formatted).expect("lines"),
        );
        (region.start, region.end, region.formatted_end)
    }

    #[rstest]
    #[case::text_empty("", "a\n", (0, 0, 1))]
    #[case::formatted_empty("a\n", "", (0, 1, 0))]
    #[case::middle_line_changed("a\nb\nc\n", "a\nx\nc\n", (1, 2, 2))]
    #[case::first_line_changed("a\nb\nc\n", "x\nb\nc\n", (0, 1, 1))]
    #[case::last_line_changed("a\nb\nc\n", "a\nb\nx\n", (2, 3, 3))]
    #[case::inserted_inside("a\nc\n", "a\nb\nc\n", (1, 1, 2))]
    #[case::deleted_inside("a\nb\nc\n", "a\nc\n", (1, 2, 1))]
    #[case::inserted_at_the_start("b\n", "a\nb\n", (0, 0, 1))]
    #[case::inserted_at_the_end("a\n", "a\nb\n", (1, 1, 2))]
    #[case::repeated_line_removed("a\na\na\n", "a\na\n", (2, 3, 2))]
    #[case::repeated_line_added("a\na\n", "a\na\na\n", (2, 2, 3))]
    #[case::runs_do_not_overlap_when_deleting("a\nb\na\n", "a\n", (1, 3, 1))]
    #[case::runs_do_not_overlap_when_inserting("a\n", "a\nb\na\n", (1, 1, 3))]
    #[case::nothing_shared("a\nb\n", "x\ny\n", (0, 2, 2))]
    #[case::final_break_removed("a\nb\n", "a\nb", (1, 2, 2))]
    #[case::final_break_added("a\nb", "a\nb\n", (1, 2, 2))]
    #[case::only_line_loses_its_break("a\n", "a", (0, 1, 1))]
    #[case::open_lines_are_equal("x\nb", "y\nb", (0, 1, 1))]
    #[case::open_line_differs_from_a_terminated_one("a\nb", "a\nb\nb", (1, 1, 2))]
    #[case::crlf_to_lf("a\r\nb\r\n", "a\nb\n", (0, 2, 2))]
    #[case::one_crlf_line("a\r\nb\n", "a\nb\n", (0, 1, 1))]
    #[case::lone_cr_to_lf("a\rb\r", "a\nb\n", (0, 2, 2))]
    #[case::one_mixed_line("a\nb\r\nc\n", "a\nb\nc\n", (1, 2, 2))]
    #[case::blank_lines_removed("a\n\n\n\nb\n", "a\nb\n", (1, 4, 1))]
    fn the_changed_region_lies_between_the_shared_lines(
        #[case] text: &str,
        #[case] formatted: &str,
        #[case] region: (usize, usize, usize),
    ) {
        assert_eq!(region_of(text, formatted), region);
    }

    // ── range_format_edit and full_format_edit ───────────────────────────────

    /// `document` split after each of its breaks (`\r\n`, `\r`, `\n`), written
    /// without `lines_with_breaks` so the checks below do not rest on it.
    fn split_after_breaks(document: &str) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current = String::new();
        let mut chars = document.chars().peekable();
        while let Some(ch) = chars.next() {
            current.push(ch);
            if ch == '\r' && chars.peek() == Some(&'\n') {
                current.push('\n');
                chars.next();
            }
            if ch == '\r' || ch == '\n' {
                lines.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        lines
    }

    /// `document` with `edit` applied; a column past the end of a line stands
    /// for that end.
    fn apply(document: &str, edit: &TextEdit) -> String {
        let lines = split_after_breaks(document);
        let offset_of = |position: Position| -> usize {
            let line = position.line as usize;
            let Some(content) = lines.get(line) else {
                return document.len();
            };
            let before: usize = lines[..line].iter().map(String::len).sum();
            let mut units = 0;
            let mut within = 0;
            for ch in content.chars() {
                if matches!(ch, '\r' | '\n') || units >= position.character as usize {
                    break;
                }
                units += ch.len_utf16();
                within += ch.len_utf8();
            }
            before + within
        };
        let start = offset_of(edit.range.start);
        let end = offset_of(edit.range.end);
        assert!(start <= end, "the range runs forward: {edit:?}");
        format!(
            "{}{}{}",
            &document[..start],
            edit.new_text,
            &document[end..]
        )
    }

    #[rstest]
    #[case::equal_texts("a: 1\nb: 2\n", "a: 1\nb: 2\n", 0, 1)]
    #[case::equal_crlf_texts("a: 1\r\nb: 2\r\n", "a: 1\r\nb: 2\r\n", 0, u32::MAX)]
    #[case::start_at_the_line_count("a:  1\nb: 2\n", "a: 1\nb: 2\n", 2, 5)]
    #[case::start_far_past_the_last_line("a:  1\nb: 2\n", "a: 1\nb: 2\n", u32::MAX, u32::MAX)]
    #[case::start_after_end("a:  1\nb:  2\n", "a: 1\nb: 2\n", 1, 0)]
    #[case::empty_text("", "a: 1\n", 0, 0)]
    #[case::lines_above_the_region("a: 1\nb:  2\n", "a: 1\nb: 2\n", 0, 0)]
    #[case::lines_below_the_region("a:  1\nb: 2\n", "a: 1\nb: 2\n", 1, 9)]
    fn range_format_edit_gives_no_edit(
        #[case] text: &str,
        #[case] formatted: &str,
        #[case] first: u32,
        #[case] last: u32,
    ) {
        assert_eq!(range_format_edit(text, formatted, first, last), None);
    }

    #[rstest]
    #[case::break_only_difference(
        "a: 1\nb: 2\n",
        "a: 1\r\nb: 2\r\n",
        0,
        0,
        Range::new(Position::new(0, 0), Position::new(2, 0)),
        "a: 1\r\nb: 2\r\n"
    )]
    #[case::lone_cr_below_the_first_line(
        "a: 1\rb:  2\rc: 3\r",
        "a: 1\rb: 2\rc: 3\r",
        1,
        1,
        Range::new(Position::new(1, 0), Position::new(2, 0)),
        "b: 2\r"
    )]
    #[case::formatted_cr_breaks(
        "a:  1\nb: 2\n",
        "a: 1\rb: 2\r",
        0,
        1,
        Range::new(Position::new(0, 0), Position::new(2, 0)),
        "a: 1\rb: 2\r"
    )]
    #[case::final_break_removed(
        "a: 1\nb: 2\n",
        "a: 1\nb: 2",
        1,
        1,
        Range::new(Position::new(1, 0), Position::new(2, 0)),
        "b: 2"
    )]
    #[case::open_last_line_gets_its_break(
        "a: 1\nb:  2",
        "a: 1\nb: 2\n",
        1,
        1,
        Range::new(Position::new(1, 0), Position::new(1, 5)),
        "b: 2\n"
    )]
    #[case::open_last_line_with_a_bmp_character(
        "a: 1\nk:  é",
        "a: 1\nk: é\n",
        1,
        1,
        Range::new(Position::new(1, 0), Position::new(1, 5)),
        "k: é\n"
    )]
    #[case::open_last_line_with_an_astral_character(
        "a: 1\nk:  \u{1f600}",
        "a: 1\nk: \u{1f600}\n",
        1,
        1,
        Range::new(Position::new(1, 0), Position::new(1, 6)),
        "k: \u{1f600}\n"
    )]
    #[case::open_last_line_after_a_lone_cr(
        "a:  1\rb:  2",
        "a: 1\rb: 2",
        1,
        1,
        Range::new(Position::new(0, 0), Position::new(1, 5)),
        "a: 1\rb: 2"
    )]
    #[case::end_line_past_the_document_is_clamped(
        "a:  1\nb: 2\n",
        "a: 1\nb: 2\n",
        0,
        100,
        Range::new(Position::new(0, 0), Position::new(1, 0)),
        "a: 1\n"
    )]
    #[case::end_line_u32_max(
        "a:  1\nb: 2\n",
        "a: 1\nb: 2\n",
        0,
        u32::MAX,
        Range::new(Position::new(0, 0), Position::new(1, 0)),
        "a: 1\n"
    )]
    #[case::whole_text_shrinks_to_nothing(
        "\n\n",
        "",
        0,
        5,
        Range::new(Position::new(0, 0), Position::new(2, 0)),
        ""
    )]
    #[case::lines_added_inside_the_range(
        "a: 1\nk: [x, y]\nb: 2\n",
        "a: 1\nk:\n  - x\n  - y\nb: 2\n",
        1,
        1,
        Range::new(Position::new(1, 0), Position::new(2, 0)),
        "k:\n  - x\n  - y\n"
    )]
    #[case::lines_removed_inside_the_range(
        "a: 1\nk:\n  - x\n  - y\nb: 2\n",
        "a: 1\nk: [x, y]\nb: 2\n",
        1,
        3,
        Range::new(Position::new(1, 0), Position::new(4, 0)),
        "k: [x, y]\n"
    )]
    #[case::blank_lines_removed(
        "a: 1\n\n\n\nb: 2\n",
        "a: 1\nb: 2\n",
        0,
        3,
        Range::new(Position::new(1, 0), Position::new(4, 0)),
        ""
    )]
    #[case::insertion_at_the_end_is_placed_after_the_last_break(
        "a: 1\n",
        "a: 1\nb: 2\n",
        0,
        0,
        Range::new(Position::new(1, 0), Position::new(1, 0)),
        "b: 2\n"
    )]
    fn range_format_edit_replaces_the_changed_region(
        #[case] text: &str,
        #[case] formatted: &str,
        #[case] first: u32,
        #[case] last: u32,
        #[case] range: Range,
        #[case] new_text: &str,
    ) {
        let edit = range_format_edit(text, formatted, first, last).expect("an edit");

        assert_eq!(edit.range, range);
        assert_eq!(edit.new_text, new_text);
        assert_eq!(apply(text, &edit), formatted);
    }

    /// Whether `range_format_edit` gives an edit for each `(first, last)`;
    /// every edit it gives is the one of the whole text, whatever the range.
    #[rstest]
    #[case::replaced_lines(
        "a\nb\nc\nd\ne\nf\n",
        "a\nb\nX\nY\ne\nf\n",
        &[(0, 1, false), (1, 2, true), (2, 2, true), (3, 3, true), (3, 9, true), (4, 4, false), (5, 5, false), (0, u32::MAX, true), (6, 9, false), (3, 2, false)]
    )]
    #[case::inserted_inside(
        "a\nb\nd\ne\n",
        "a\nb\nc\nd\ne\n",
        &[(0, 0, false), (1, 1, true), (2, 2, true), (1, 2, true), (3, 3, false)]
    )]
    #[case::inserted_at_the_start("b\nc\n", "a\nb\nc\n", &[(0, 0, true), (1, 1, false)])]
    #[case::inserted_at_the_end("a\nb\n", "a\nb\nc\n", &[(0, 0, false), (1, 1, true), (2, 2, false), (2, 5, false)])]
    #[case::deleted_inside(
        "a\nb\nc\nd\n",
        "a\nd\n",
        &[(0, 0, false), (1, 1, true), (2, 2, true), (3, 3, false)]
    )]
    #[case::repeated_lines_delete_the_last("a\na\na\n", "a\na\n", &[(0, 1, false), (2, 2, true)])]
    #[case::repeated_lines_add_after_the_last("a\na\n", "a\na\na\n", &[(0, 0, false), (1, 1, true)])]
    #[case::blank_lines_removed(
        "a: 1\n\n\n\nb: 2\n",
        "a: 1\nb: 2\n",
        &[(0, 0, false), (0, 1, true), (1, 3, true), (4, 4, false)]
    )]
    #[case::final_break_removed("a: 1\nb: 2\n", "a: 1\nb: 2", &[(0, 0, false), (1, 1, true)])]
    fn range_format_edit_follows_the_touch_rule(
        #[case] text: &str,
        #[case] formatted: &str,
        #[case] ranges: &[(u32, u32, bool)],
    ) {
        let whole = range_format_edit(text, formatted, 0, u32::MAX)
            .expect("the whole text touches the region");
        assert_eq!(apply(text, &whole), formatted);

        for &(first, last, touches) in ranges {
            let edit = range_format_edit(text, formatted, first, last);

            if touches {
                assert_eq!(edit.as_ref(), Some(&whole), "lines {first}..={last}");
            } else {
                assert_eq!(edit, None, "lines {first}..={last}");
            }
        }
    }

    #[rstest]
    #[case::lf_terminated("a:  1\nb: 2\n", Position::new(2, 0))]
    #[case::crlf_terminated("a:  1\r\nb: 2\r\n", Position::new(2, 0))]
    #[case::lone_cr_terminated("a:  1\rb: 2\r", Position::new(2, 0))]
    #[case::empty_text("", Position::new(0, 0))]
    fn full_format_edit_covers_the_whole_text(#[case] text: &str, #[case] end: Position) {
        let formatted = "x: 1\n";

        let edit = full_format_edit(text, formatted).expect("an edit");

        assert_eq!(edit.range, Range::new(Position::new(0, 0), end));
        assert_eq!(apply(text, &edit), formatted);
    }

    #[rstest]
    #[case::unterminated("a:  1\nb: 2", Position::new(1, 4))]
    #[case::unterminated_lone_cr("a:  1\rb: 2", Position::new(1, 4))]
    #[case::unterminated_single_line("a:  1", Position::new(0, 5))]
    #[case::multibyte_last_line("k:  é", Position::new(0, 5))]
    #[case::astral_last_line_counts_two_units("k:  \u{1f600}", Position::new(0, 6))]
    fn full_format_edit_of_an_open_last_line_ends_at_its_utf16_length(
        #[case] text: &str,
        #[case] end: Position,
    ) {
        let formatted = "x: 1\n";

        let edit = full_format_edit(text, formatted).expect("an edit");

        assert_eq!(edit.range.end, end);
        assert_eq!(apply(text, &edit), formatted);
    }

    #[test]
    fn full_format_edit_gives_no_edit_for_equal_texts() {
        assert_eq!(full_format_edit("a: 1\r\n", "a: 1\r\n"), None);
    }

    #[test]
    fn full_format_edit_sees_a_break_only_difference() {
        assert!(full_format_edit("a: 1\n", "a: 1\r\n").is_some());
        assert!(full_format_edit("a: 1\n", "a: 1").is_some());
    }

    mod format_edit_properties {
        use proptest::prelude::*;

        use super::*;

        /// Whether `range_format_edit` should give an edit for the lines
        /// `first..=last`, decided with own loops over lines split without
        /// `lines_with_breaks`.
        fn should_touch(document: &str, formatted: &str, first: u32, last: u32) -> bool {
            let original = split_after_breaks(document);
            let wanted = split_after_breaks(formatted);
            let (first, last) = (first as usize, last as usize);
            if document == formatted || first > last || first >= original.len() {
                return false;
            }
            let mut start = 0;
            while start < original.len() && start < wanted.len() && original[start] == wanted[start]
            {
                start += 1;
            }
            let mut tail = 0;
            while tail < original.len() - start
                && tail < wanted.len() - start
                && original[original.len() - 1 - tail] == wanted[wanted.len() - 1 - tail]
            {
                tail += 1;
            }
            let end = original.len() - tail;
            if start < end {
                first < end && last >= start
            } else {
                first <= start && (start == 0 || last >= start - 1)
            }
        }

        /// What the edit for `first..=last` must do, checked against the
        /// independent oracle: no edit, or an edit that gives `formatted`.
        fn check(
            document: &str,
            formatted: &str,
            first: u32,
            last: u32,
        ) -> Result<(), TestCaseError> {
            let edit = range_format_edit(document, formatted, first, last);
            prop_assert_eq!(
                edit.is_some(),
                should_touch(document, formatted, first, last),
                "{:?} -> {:?}, lines {}..={}",
                document,
                formatted,
                first,
                last
            );
            if let Some(edit) = edit {
                prop_assert_eq!(apply(document, &edit), formatted);
                prop_assert_eq!(
                    Some(edit),
                    range_format_edit(document, formatted, 0, u32::MAX)
                );
            }
            Ok(())
        }

        /// Every text of up to four characters from `a`, `\n`, `\r`.
        fn small_texts() -> Vec<String> {
            let mut texts = vec![String::new()];
            let mut layer = vec![String::new()];
            for _ in 0..4 {
                layer = layer
                    .iter()
                    .flat_map(|text| ['a', '\n', '\r'].map(|ch| format!("{text}{ch}")))
                    .collect();
                texts.extend(layer.iter().cloned());
            }
            texts
        }

        #[test]
        fn every_small_text_pair_and_range_gives_no_edit_or_the_formatted_text() {
            let texts = small_texts();
            let lines = [0, 1, 2, 3, 4, u32::MAX];
            for text in &texts {
                for formatted in &texts {
                    for &first in &lines {
                        for &last in &lines {
                            check(text, formatted, first, last).expect("holds");
                        }
                    }
                }
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 2048,
                ..ProptestConfig::default()
            })]

            /// The edit for any range is none or gives `formatted`, and is the
            /// edit of the whole text.
            #[test]
            fn a_range_gives_no_edit_or_the_formatted_text(
                text in "[ab\n\r\u{1f600}]{0,12}",
                formatted in "[ab\n\r\u{1f600}]{0,12}",
                first in prop_oneof![0_u32..8, Just(u32::MAX)],
                last in prop_oneof![0_u32..8, Just(u32::MAX)],
            ) {
                check(&text, &formatted, first, last)?;
            }

            /// Widening a range that gets an edit keeps the edit.
            #[test]
            fn a_wider_range_keeps_the_edit(
                text in "[ab\n\r]{0,12}",
                formatted in "[ab\n\r]{0,12}",
                first in 0_u32..8,
                last in 0_u32..8,
            ) {
                if range_format_edit(&text, &formatted, first, last).is_some() {
                    let wider =
                        range_format_edit(&text, &formatted, first / 2, last.saturating_add(1));

                    prop_assert!(wider.is_some());
                }
            }

            /// Each part of a text cut at its line breaks follows the last.
            #[test]
            fn the_lines_with_breaks_make_up_the_text(text in "[a #\n\r\u{e9}]{0,40}") {
                let lines = lines_with_breaks(&text).expect("lines");

                prop_assert_eq!(lines.concat(), text.clone());
                prop_assert_eq!(
                    lines.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    split_after_breaks(&text)
                );
            }
        }
    }

    mod source_lines_properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 2048,
                ..ProptestConfig::default()
            })]

            /// `source_lines` agrees with `str::lines()` on a text whose breaks
            /// were first normalised to `\n`.
            #[test]
            fn matches_lines_of_the_break_normalised_text(
                text in "[a #\n\r\u{e9}]{0,40}",
            ) {
                let normalised = text.replace("\r\n", "\n").replace('\r', "\n");

                let lines: Vec<&str> = source_lines(&text).collect();

                prop_assert_eq!(lines, normalised.lines().collect::<Vec<_>>());
            }

            /// The `n`th line starts where `LineIndex` puts LSP line `n`.
            #[test]
            fn line_n_starts_at_line_index_line_n(
                text in "[a #\n\r\u{e9}]{0,40}",
            ) {
                let line_index = LineIndex::new(&text);

                for (n, line) in source_lines(&text).enumerate() {
                    let offset = line.as_ptr().addr() - text.as_ptr().addr();
                    let offset = u32::try_from(offset).expect("offset of a short text");

                    prop_assert_eq!(
                        offset_to_lsp(offset, &line_index),
                        Position::new(u32::try_from(n).expect("line of a short text"), 0)
                    );
                }
            }
        }
    }
}
