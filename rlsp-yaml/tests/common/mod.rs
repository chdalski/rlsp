// SPDX-License-Identifier: MIT
//
// Shared helpers for integration test crates.
//
// Imported by sibling integration tests via `mod common; use common::*;`.

use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};

pub fn docs_for(text: &str) -> Vec<rlsp_yaml_parser::node::Document<rlsp_yaml_parser::Span>> {
    rlsp_yaml_parser::load(text).unwrap_or_default()
}

#[expect(clippy::expect_used, reason = "literal URL is always valid")]
pub fn test_uri() -> Url {
    Url::parse("file:///test.yaml").expect("valid test URI")
}

pub fn cursor_range(line: u32, col: u32) -> Range {
    Range::new(Position::new(line, col), Position::new(line, col))
}

pub fn codepoint_to_byte(s: &str, codepoint_idx: usize) -> usize {
    s.char_indices()
        .nth(codepoint_idx)
        .map_or(s.len(), |(b, _)| b)
}

pub fn apply_text_edit(source: &str, edit: &TextEdit) -> String {
    let start_line = edit.range.start.line as usize;
    let end_line = edit.range.end.line as usize;
    let start_col = edit.range.start.character as usize;
    let end_col = edit.range.end.character as usize;

    let source_lines: Vec<&str> = source.lines().collect();
    let mut result = String::new();

    for (i, src_line) in source_lines.iter().enumerate() {
        if i < start_line || i > end_line {
            result.push_str(src_line);
            result.push('\n');
        } else if i == start_line && i == end_line {
            let start_byte = codepoint_to_byte(src_line, start_col);
            let end_byte = codepoint_to_byte(src_line, end_col);
            result.push_str(&src_line[..start_byte]);
            result.push_str(&edit.new_text);
            result.push_str(&src_line[end_byte..]);
            result.push('\n');
        } else if i == start_line {
            let start_byte = codepoint_to_byte(src_line, start_col);
            result.push_str(&src_line[..start_byte]);
            result.push_str(&edit.new_text);
        } else if i == end_line {
            let end_byte = codepoint_to_byte(src_line, end_col);
            result.push_str(&src_line[end_byte..]);
            result.push('\n');
        }
        // Lines strictly between start and end are absorbed by the edit — skip them.
    }
    result
}
