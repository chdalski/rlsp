// SPDX-License-Identifier: MIT

//! Shared helpers for converting parser `Span` byte offsets to LSP positions.

use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{Position, Range};

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
}
