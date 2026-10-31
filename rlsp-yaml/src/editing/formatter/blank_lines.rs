// SPDX-License-Identifier: MIT

//! Blank-line restoration for formatted output.
//!
//! The formatter builds its output from the AST, which carries every comment
//! but no blank lines.  This module copies the input's blank lines back into
//! the output.  It never creates, moves, or removes a comment line: every
//! non-blank line of the output stays exactly as the formatter printed it.
//!
//! Lines are paired by their text, so a blank line could land inside a block
//! scalar, where it would change the value.  The restoration therefore leaves
//! block scalars alone: their lines, found through the parser's spans, take no
//! part on either side.  A block scalar prints the blank lines of its own value.

use std::iter::Peekable;
use std::ops::Range;
use std::slice::Iter;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, ScalarStyle, Span};

/// A non-blank line of the original text, with the blank line that preceded it.
struct SourceLine {
    /// The line without its comment, trimmed.  Empty for a comment-only line.
    signature: String,
    /// Whether at least one blank line preceded this line.  Several blank
    /// lines collapse to one.
    blank_before: bool,
}

/// Find the comment portion of a single line, respecting quoted strings.
///
/// Returns the byte offset of the `#` that starts the comment, or `None` if the
/// line has no comment.
fn find_comment_on_line(line: &str) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = line.char_indices();
    while let Some((byte_pos, c)) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            '\\' if in_double => {
                // Skip the next character (escape sequence).
                chars.next();
            }
            '#' if !in_single && !in_double => {
                // Must be preceded by whitespace or be the first non-whitespace char.
                let before = &line[..byte_pos];
                if before.trim_end().is_empty() || before.ends_with(|c: char| c.is_whitespace()) {
                    return Some(byte_pos);
                }
            }
            _ => {}
        }
    }
    None
}

/// The signature of a line: its trimmed non-comment portion.  Two spellings of
/// one line that differ only in whitespace before a comment share a signature.
fn content_signature(line: &str) -> String {
    let code = find_comment_on_line(line).map_or(line, |byte_pos| &line[..byte_pos]);
    code.trim().to_string()
}

/// The 0-based lines inside each block scalar of `documents` (below the header,
/// through the last line the scalar consumes), in source order.  Block scalars
/// do not nest, so the ranges are sorted and disjoint.  Trailing blank lines
/// that the scalar consumes lie inside its range.
pub(super) fn block_scalar_lines(documents: &[Document<Span>]) -> Vec<Range<usize>> {
    fn collect(node: &Node<Span>, index: &LineIndex, out: &mut Vec<Range<usize>>) {
        match node {
            Node::Scalar {
                style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
                loc,
                ..
            } => {
                // `loc` runs from the header to the start of the line after the
                // block, or to the end of its last line at the end of the input.
                // `line_column` counts lines from 1, so the 1-based number of the
                // header line is the 0-based number of the line below it.
                let (header_line_1based, _) = index.line_column(loc.start);
                let (end_line_1based, end_column) = index.line_column(loc.end);
                let first_content = header_line_1based as usize;
                let end_exclusive = if end_column == 0 {
                    // The 1-based number of the line after the block is the
                    // 0-based number of the block's last line plus 2.
                    (end_line_1based as usize).saturating_sub(1)
                } else {
                    end_line_1based as usize
                };
                out.push(first_content..end_exclusive.max(first_content));
            }
            Node::Scalar { .. } | Node::Alias { .. } => {}
            Node::Mapping { entries, .. } => {
                for (key, value) in entries {
                    collect(key, index, out);
                    collect(value, index, out);
                }
            }
            Node::Sequence { items, .. } => {
                for item in items {
                    collect(item, index, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    for document in documents {
        collect(&document.root, document.line_index(), &mut out);
    }
    out
}

/// The block scalar that holds `line`, as an index into `blocks`.
fn block_of(blocks: &[Range<usize>], line: usize) -> Option<usize> {
    let candidate = blocks.partition_point(|b| b.end <= line);
    blocks
        .get(candidate)
        .filter(|b| b.contains(&line))
        .map(|_| candidate)
}

/// The non-blank lines of `original` outside block scalars, split the way the
/// parser splits lines (`\n`, `\r\n`, and a lone `\r`).
fn source_lines(original: &str, blocks: &[Range<usize>]) -> Vec<SourceLine> {
    let normalized = original.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<SourceLine> = Vec::new();
    let mut blank_pending = false;
    let mut seen_content = false;
    for (idx, line) in normalized.lines().enumerate() {
        if line.trim().is_empty() {
            blank_pending = true;
            continue;
        }
        // A blank line before or between the lines of a block scalar belongs to
        // its value; one after the last of them still separates the scalar from
        // the next line.
        if block_of(blocks, idx).is_none() {
            lines.push(SourceLine {
                signature: content_signature(line),
                blank_before: blank_pending && seen_content,
            });
        }
        blank_pending = false;
        seen_content = true;
    }
    lines
}

/// Take the entry that pairs with a formatted line of signature `signature`;
/// whether the original had a blank line before it.
///
/// A comment-only line (empty signature) pairs with the next comment-only
/// entry.  Entries that are comment-only lines the formatter printed elsewhere
/// stay unpaired.  An entry that never matches blocks all that follow, so after
/// a line whose spelling changed no further blank line is restored.
fn pairs_with_blank_line(entries: &mut Peekable<Iter<'_, SourceLine>>, signature: &str) -> bool {
    if signature.is_empty() {
        entries
            .next_if(|e| e.signature.is_empty())
            .is_some_and(|e| e.blank_before)
    } else {
        while entries.next_if(|e| e.signature.is_empty()).is_some() {}
        entries
            .next_if(|e| e.signature == signature)
            .is_some_and(|e| e.blank_before)
    }
}

/// Restore the blank lines of `original` in `formatted`.
///
/// `source_blocks` are the block scalar lines of `original` (see
/// [`block_scalar_lines`]).  Walks the non-blank lines of both texts in step: a
/// formatted line takes the blank line its original line had when the two have
/// the same signature.  Lines inside block scalars take no part, on either side.
/// A formatted line without a counterpart is copied as is.
///
/// Every line of `formatted` appears in the result once and in the same order;
/// the only lines added are blank lines.  The result ends with a newline.  No
/// blank line is added when `formatted` does not load, or when the blank lines
/// would change the structure or a scalar value.
pub(super) fn restore_blank_lines(
    original: &str,
    source_blocks: &[Range<usize>],
    formatted: &str,
) -> String {
    let Ok(formatted_documents) = rlsp_yaml_parser::load(formatted) else {
        return with_final_newline(&formatted.lines().collect::<Vec<_>>());
    };
    let formatted_blocks = block_scalar_lines(&formatted_documents);

    let source = source_lines(original, source_blocks);
    let mut entries = source.iter().peekable();
    let mut result_lines: Vec<&str> = Vec::new();

    for (idx, fmt_line) in formatted.lines().enumerate() {
        // A blank line of the formatter has no counterpart among the non-blank
        // source lines, and a line of a block scalar is left as printed.
        let in_block = block_of(&formatted_blocks, idx).is_some();
        if !in_block
            && !fmt_line.trim().is_empty()
            && pairs_with_blank_line(&mut entries, &content_signature(fmt_line))
        {
            push_blank(&mut result_lines);
        }
        result_lines.push(fmt_line);
    }

    let restored = with_final_newline(&result_lines);
    if same_content(&restored, &formatted_documents) {
        restored
    } else {
        with_final_newline(&formatted.lines().collect::<Vec<_>>())
    }
}

/// Whether `restored` loads to the structure and scalar values of `documents`.
///
/// Pairing lines by their text can drift, and a blank line in the wrong place can
/// change a value (after an empty `|+`, say).  This check makes the restoration
/// safe whatever the drift: when it fails, the formatter's own output stands.
fn same_content(restored: &str, documents: &[Document<Span>]) -> bool {
    fn shape(node: &Node<Span>, out: &mut Vec<String>) {
        match node {
            Node::Scalar { value, .. } => out.push(format!("={value}")),
            Node::Alias { name, .. } => out.push(format!("*{name}")),
            Node::Mapping { entries, .. } => {
                out.push("{".to_string());
                for (key, value) in entries {
                    shape(key, out);
                    shape(value, out);
                }
                out.push("}".to_string());
            }
            Node::Sequence { items, .. } => {
                out.push("[".to_string());
                for item in items {
                    shape(item, out);
                }
                out.push("]".to_string());
            }
        }
    }
    let shape_of = |documents: &[Document<Span>]| {
        let mut out = Vec::new();
        for document in documents {
            out.push("---".to_string());
            shape(&document.root, &mut out);
        }
        out
    };
    rlsp_yaml_parser::load(restored).is_ok_and(|loaded| shape_of(&loaded) == shape_of(documents))
}

/// Join `output_lines` with `\n` and end the result with a newline.
fn with_final_newline(output_lines: &[&str]) -> String {
    let mut out = output_lines.join("\n");
    out.push('\n');
    out
}

/// Push a blank line unless the output is empty or already ends in one.
fn push_blank(result_lines: &mut Vec<&str>) {
    if result_lines.last().is_some_and(|l| !l.trim().is_empty()) {
        result_lines.push("");
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// `restore_blank_lines` for an `original` that loads.
    fn restore(original: &str, formatted: &str) -> String {
        let documents = rlsp_yaml_parser::load(original).expect("test input must parse");
        restore_blank_lines(original, &block_scalar_lines(&documents), formatted)
    }

    #[rstest]
    #[case::before_content_line("# a\n\nb: 1\n", "# a\nb: 1\n", "# a\n\nb: 1\n")]
    #[case::before_comment_line("a: 1\n\n# c\nb: 2\n", "a: 1\n# c\nb: 2\n", "a: 1\n\n# c\nb: 2\n")]
    #[case::several_blank_lines_collapse("a: 1\n\n\n\nb: 2\n", "a: 1\nb: 2\n", "a: 1\n\nb: 2\n")]
    #[case::leading_blank_lines_are_dropped("\n\na: 1\n", "a: 1\n", "a: 1\n")]
    #[case::crlf_source("a: 1\r\n\r\nb: 2\r\n", "a: 1\nb: 2\n", "a: 1\n\nb: 2\n")]
    #[case::lone_cr_source("a: 1\r\rb: 2\r", "a: 1\nb: 2\n", "a: 1\n\nb: 2\n")]
    #[case::comment_changed_whitespace(
        "a: 1 # c\n\nb: 2\n",
        "a: 1  # c\nb: 2\n",
        "a: 1  # c\n\nb: 2\n"
    )]
    #[case::hash_lines_in_block_scalar_stay(
        "a: |\n  t\n  # d\n",
        "a: |\n  t\n  # d\n",
        "a: |\n  t\n  # d\n"
    )]
    #[case::no_blank_line_between_comment_and_its_line(
        "# a\n# b\nc: 1\n",
        "# a\n# b\nc: 1\n",
        "# a\n# b\nc: 1\n"
    )]
    fn restores_the_blank_lines_of_the_original(
        #[case] original: &str,
        #[case] formatted: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(restore(original, formatted), expected);
    }

    #[test]
    fn never_adds_a_line_that_is_missing_from_formatted() {
        assert_eq!(restore("a: 1 # x\n# y\n", "a: 1\n"), "a: 1\n");
    }

    #[test]
    fn keeps_a_comment_line_that_the_original_lacks() {
        let formatted = "---\n# c\na: 1\n";
        assert_eq!(
            restore("--- # c\n\na: 1\n", formatted),
            "---\n# c\n\na: 1\n"
        );
    }

    #[test]
    fn keeps_a_comment_line_that_moved() {
        let formatted = "a: v\n# c\n";
        assert_eq!(restore("a:\n  # c\n  v\n", formatted), formatted);
    }

    #[rstest]
    #[case::moved_comment("--- # c\n\na: 1\n\nb: 2\n", "---\n# c\na: 1\nb: 2\n")]
    #[case::dropped_directive("%YAML 1.2 # c\n---\n\na: 1\n", "# c\n---\na: 1\n")]
    #[case::block_scalar("a: |\n  t\n\n  # d\n\nb: 1\n", "a: |\n  t\n  # d\nb: 1\n")]
    #[case::comment_only("# a\n\n\n# b\n", "# a\n# b\n")]
    fn only_blank_lines_are_added(#[case] original: &str, #[case] formatted: &str) {
        let restored = restore(original, formatted);
        let without_blank_lines = |s: &str| {
            s.lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            without_blank_lines(&restored),
            without_blank_lines(formatted)
        );
    }

    #[rstest]
    #[case::blank_line_inside_block_is_not_added(
        "a: |\n  x\n\n  y\nb: 1\n",
        "a: |\n  x\n  y\nb: 1\n",
        "a: |\n  x\n  y\nb: 1\n"
    )]
    #[case::block_line_does_not_pair_with_a_line_outside(
        "a: 1\n\nb: \"y\"\nc: |\n  b: \"y\"\n  z\n",
        "a: 1\nb: y\nc: |\n  b: \"y\"\n  z\n",
        "a: 1\nb: y\nc: |\n  b: \"y\"\n  z\n"
    )]
    #[case::blank_line_after_block_separates_it_from_the_next_line(
        "a: |\n  x\n\nb: 1\n",
        "a: |\n  x\nb: 1\n",
        "a: |\n  x\n\nb: 1\n"
    )]
    #[case::blank_line_before_block_is_restored(
        "a: 1\n\nb: |\n  x\n",
        "a: 1\nb: |\n  x\n",
        "a: 1\n\nb: |\n  x\n"
    )]
    #[case::blank_lines_of_the_value_are_not_doubled(
        "a: |+\n  x\n\nb: 1\n",
        "a: |+\n  x\n\nb: 1\n",
        "a: |+\n  x\n\nb: 1\n"
    )]
    fn leaves_block_scalars_alone(
        #[case] original: &str,
        #[case] formatted: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(restore(original, formatted), expected);
    }

    #[test]
    fn block_scalar_lines_cover_the_lines_below_the_header() {
        let documents = rlsp_yaml_parser::load("a: 1\nb: |\n  x\n\n  y\nc: >\n  z\n\nd: 2\n")
            .expect("test input must parse");
        assert_eq!(block_scalar_lines(&documents), [2..5, 6..8]);
    }

    #[test]
    fn block_scalar_lines_end_with_the_input_without_a_final_newline() {
        let documents = rlsp_yaml_parser::load("b: |\n  x").expect("test input must parse");
        assert_eq!(block_scalar_lines(&documents), vec![1..2]);
    }

    #[test]
    fn drops_the_restoration_when_a_blank_line_would_change_a_value() {
        // The formatter prints the first scalar double-quoted, which shifts the
        // text pairing by one line: the blank line of the second `c` would land
        // after the empty `|+` and become part of its value.
        let original = "c: |+\n  x\n      \nc: |+\nc: |+\n";
        let formatted = "c: \"x\\n    \\n\"\nc: |+\nc: |+\n";
        assert_eq!(restore(original, formatted), formatted);
    }
}
