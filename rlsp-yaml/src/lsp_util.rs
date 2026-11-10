// SPDX-License-Identifier: MIT

//! Shared helpers for converting parser `Span` byte offsets to LSP positions.

use rlsp_yaml_parser::{LineBuffer, LineIndex, Span};
use tower_lsp::lsp_types::{Position, Range};

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
