// SPDX-License-Identifier: MIT

use super::content_tracking::{ContentEntry, content_signature, last_content_line_idx};

/// A document-prefix leading comment extracted from raw YAML text.
///
/// These are comments that appear before the first content node in a document.
/// The YAML tokenizer (`l_document_prefix`) discards them before producing
/// events, so they cannot be recovered from the AST.  This struct is used
/// only to preserve them during formatting.
#[derive(Debug, Clone)]
pub(super) struct Comment {
    /// 0-based line number in the original text.
    pub(super) line: usize,
    /// The comment text including `#` (e.g. `# this is a comment`).
    pub(super) text: String,
}

/// Find the comment portion of a single line, respecting quoted strings.
///
/// Returns `(byte_offset_of_hash, comment_text)` or `None` if the line has no comment.
pub(super) fn find_comment_on_line(line: &str) -> Option<(usize, String)> {
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
                    return Some((byte_pos, line[byte_pos..].to_string()));
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract only the leading comments that appear before the first content line
/// in the input.  These are comments the YAML tokenizer discards at the
/// `l_document_prefix` level and that therefore do not appear in the AST.
///
/// Stops at the first non-blank, non-comment line so inter-node comments
/// (which the loader now attaches to AST nodes) are not returned here.
pub(super) fn extract_doc_prefix_comments(text: &str) -> Vec<Comment> {
    let mut comments = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Skip blank lines in the prefix region.
            continue;
        }
        if let Some((byte_pos, comment_text)) = find_comment_on_line(line) {
            let before = &line[..byte_pos];
            if before.trim().is_empty() {
                // Leading comment — still in prefix region.
                comments.push(Comment {
                    line: line_idx,
                    text: comment_text,
                });
                continue;
            }
        }
        // First non-blank, non-comment line — stop scanning.
        break;
    }
    comments
}

/// Attach document-prefix leading comments and blank lines back to the formatted output.
///
/// Strategy:
/// - Build a list of content entries from the original (one per non-blank,
///   non-doc-prefix-comment line).  Each entry records blank lines before it and any
///   doc-prefix leading comments attached to it.
/// - Walk the formatted output; when a line's signature matches the next entry, emit the
///   blank line (if any), then leading comments (indented to match the content line), then
///   the line.  Entries with empty signatures (comment-only lines now embedded by the
///   AST-based formatter) are skipped; their blank-lines-before count is carried forward.
/// - Any unmatched leading comments (e.g. at end of file) are appended at the end.
///
/// Always runs (even with no comments) so blank line reattachment is never skipped.
///
/// `last_content_hint` is an AST-derived 0-based line index used to prevent
/// block scalar `#`-prefixed content lines from being mistaken for EOF comments.
pub(super) fn attach_comments(
    original: &str,
    formatted: &str,
    comments: &[Comment],
    last_content_hint: Option<usize>,
) -> String {
    // Build a quick lookup: line index -> comment.
    let line_to_comment: std::collections::HashMap<usize, &Comment> =
        comments.iter().map(|c| (c.line, c)).collect();

    // `#` lines after `last_content_idx` are EOF-trailing comments; before it
    // they are inter-node comments already emitted by the AST formatter.
    // Combine text-scan and AST-derived hints to get the correct boundary.
    let last_content_idx = last_content_line_idx(original, &line_to_comment)
        .map(|t| last_content_hint.map_or(t, |h| t.max(h)))
        .or(last_content_hint);

    let mut entries: Vec<ContentEntry> = Vec::new();
    let mut pending_leading: Vec<String> = Vec::new();
    let mut pending_blanks: usize = 0;
    let mut first_entry = true;

    for (idx, line) in original.lines().enumerate() {
        if let Some(comment) = line_to_comment.get(&idx) {
            // All comments from extract_doc_prefix_comments are Leading.
            // Insert a blank separator if there was a gap before this comment group.
            if pending_blanks > 0 {
                pending_leading.push(String::new());
            }
            pending_blanks = 0;
            pending_leading.push(comment.text.clone());
        } else if line.is_empty() {
            pending_blanks += 1;
        } else if line.trim_start().starts_with('#')
            && last_content_idx.is_some_and(|last| idx > last)
        {
            // A standalone comment line after all content — not in line_to_comment
            // and not handled by the AST formatter.  Collect as an EOF-trailing
            // comment to be appended verbatim after the formatted body.
            if pending_blanks > 0 {
                pending_leading.push(String::new());
            }
            pending_blanks = 0;
            pending_leading.push(line.trim().to_string());
        } else {
            entries.push(ContentEntry {
                signature: content_signature(line),
                blank_lines_before: if first_entry {
                    0
                } else {
                    pending_blanks.min(1)
                },
                leading: std::mem::take(&mut pending_leading),
            });
            first_entry = false;
            pending_blanks = 0;
        }
    }

    // Any remaining leading comments (after all content) go at the end.
    let trailing_leading = pending_leading;

    let mut result_lines: Vec<String> = Vec::new();
    let mut entry_iter = entries.iter();
    let mut next_entry = entry_iter.next();

    for fmt_line in formatted.lines() {
        let fmt_sig = content_signature(fmt_line);

        if fmt_sig.is_empty() {
            // This is a blank or comment-only line already emitted by the AST-based
            // formatter.  Check whether the corresponding entry in the original text
            // had blank lines before it (indicating a blank section separator) and
            // emit that blank before the comment line.
            if matches!(next_entry, Some(e) if e.signature.is_empty()) {
                if let Some(e) = next_entry
                    && e.blank_lines_before > 0
                {
                    result_lines.push(String::new());
                }
                next_entry = entry_iter.next();
            }
            result_lines.push(fmt_line.to_string());
            continue;
        }

        // Non-empty signature line: match against the next entry.
        // Skip any remaining empty-sig entries (e.g. if there were multiple
        // comment lines for this section) and carry any unmatched blank count.
        let mut carried_blanks = 0usize;
        while matches!(next_entry, Some(e) if e.signature.is_empty()) {
            if let Some(e) = next_entry {
                carried_blanks = carried_blanks.max(e.blank_lines_before);
            }
            next_entry = entry_iter.next();
        }

        if let Some(entry) = next_entry
            && entry.signature == fmt_sig
        {
            let indent_len = fmt_line.len() - fmt_line.trim_start().len();
            let indent_str = " ".repeat(indent_len);

            // Emit blank line before this entry if the original had one,
            // or if a skipped empty-sig entry carried a blank count.
            // Skip if the formatted output already ended with a blank line
            // (e.g. emitted by repr_block_to_doc for folded scalar content)
            // to prevent double blanks.
            let last_is_blank = result_lines.last().is_some_and(String::is_empty);
            if (entry.blank_lines_before > 0 || carried_blanks > 0) && !last_is_blank {
                result_lines.push(String::new());
            }

            for lc in &entry.leading {
                if lc.is_empty() {
                    result_lines.push(String::new());
                } else {
                    result_lines.push(format!("{indent_str}{lc}"));
                }
            }

            result_lines.push(fmt_line.to_string());

            next_entry = entry_iter.next();
            continue;
        }

        result_lines.push(fmt_line.to_string());
    }

    // Append trailing leading comments (e.g. at end of file after all content).
    for lc in &trailing_leading {
        if lc.is_empty() {
            result_lines.push(String::new());
        } else {
            result_lines.push(lc.clone());
        }
    }

    let mut out = result_lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
