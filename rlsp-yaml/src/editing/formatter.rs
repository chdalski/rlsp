// SPDX-License-Identifier: MIT

use std::fmt::Write as _;

use rlsp_fmt::{Doc, FormatOptions, concat, format as fmt_format, hard_line, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{ScalarStyle, Span};

use crate::editing::editor_config::LineEnding;

mod dedup;
mod mapping_render;
/// YAML formatting options and their defaults.
pub mod options;
mod scalar_render;
mod sequence_render;

pub use options::YamlFormatOptions;
use scalar_render::{
    escape_double_quoted, format_tag, is_core_schema_tag, needs_flow_quoting, needs_quoting,
    repr_block_to_doc, requires_double_quoting, string_to_doc,
};

/// A document-prefix leading comment extracted from raw YAML text.
///
/// These are comments that appear before the first content node in a document.
/// The YAML tokenizer (`l_document_prefix`) discards them before producing
/// events, so they cannot be recovered from the AST.  This struct is used
/// only to preserve them during formatting.
#[derive(Debug, Clone)]
struct Comment {
    /// 0-based line number in the original text.
    line: usize,
    /// The comment text including `#` (e.g. `# this is a comment`).
    text: String,
}

/// Find the comment portion of a single line, respecting quoted strings.
///
/// Returns `(byte_offset_of_hash, comment_text)` or `None` if the line has no comment.
fn find_comment_on_line(line: &str) -> Option<(usize, String)> {
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

/// Extract the content signature from a line: the trimmed non-comment portion.
fn content_signature(line: &str) -> String {
    if let Some((byte_pos, _)) = find_comment_on_line(line) {
        line[..byte_pos].trim().to_string()
    } else {
        line.trim().to_string()
    }
}

/// A content line from the original text, with its blank-line and doc-prefix comment context.
struct ContentEntry {
    signature: String,
    /// Number of blank lines that preceded this content line in the original.
    /// Capped at 1 — multiple consecutive blank lines collapse to one.
    blank_lines_before: usize,
    /// Document-prefix leading comment lines that precede this content line.
    leading: Vec<String>,
}

/// Compute the last "YAML content" line index from the AST.
///
/// Block scalar values (literal `|` or folded `>`) can contain lines that start with
/// `#`, which the text-scanning heuristic in `last_content_line_idx` misidentifies as
/// standalone comments.  This function derives the last content line from AST span
/// information, which is authoritative.
///
/// For each scalar node: `last_line = (idx.line_column(loc.start).0 as usize) + lines_in_value - 1`.
/// The `(idx.line_column(loc.start).0 as usize)` is the first content line (not the header line for block scalars).
/// `lines_in_value` counts the lines in the decoded value — for block scalars this
/// includes any trailing blank lines consumed by the chomp indicator.
fn last_content_line_from_ast(docs: &[Document<Span>]) -> Option<usize> {
    fn node_last_content_line(
        node: &Node<Span>,
        idx: &rlsp_yaml_parser::LineIndex,
    ) -> Option<usize> {
        match node {
            // Block scalars occupy multiple source lines.  The decoded value
            // includes trailing blank lines consumed by the chomp indicator,
            // so `lines().count()` gives the exact number of occupied source
            // lines (content + any trailing blanks consumed by keep/clip/strip).
            Node::Scalar {
                style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
                loc,
                value,
                ..
            } => {
                // line_column returns 1-based line; convert to 0-based first.
                let start_0 = idx.line_column(loc.start).0.saturating_sub(1) as usize;
                let line_count = value.lines().count();
                Some(start_0 + line_count.saturating_sub(1))
            }
            // Non-block scalars (plain, single-quoted, double-quoted) always
            // occupy a single source line regardless of how many newlines are
            // embedded in the decoded value.
            Node::Scalar { loc, .. } | Node::Alias { loc, .. } => {
                Some(idx.line_column(loc.start).0.saturating_sub(1) as usize)
            }
            Node::Mapping { entries, .. } => entries
                .iter()
                .flat_map(|(k, v)| {
                    [
                        node_last_content_line(k, idx),
                        node_last_content_line(v, idx),
                    ]
                })
                .flatten()
                .max(),
            Node::Sequence { items, .. } => items
                .iter()
                .filter_map(|n| node_last_content_line(n, idx))
                .max(),
        }
    }
    docs.iter()
        .filter_map(|doc| node_last_content_line(&doc.root, doc.line_index()))
        .max()
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
fn last_content_line_idx(
    original: &str,
    line_to_comment: &std::collections::HashMap<usize, &Comment>,
) -> Option<usize> {
    original
        .lines()
        .enumerate()
        .filter(|(idx, line)| {
            !line.trim().is_empty()
                && (!line.trim_start().starts_with('#') || line_to_comment.contains_key(idx))
        })
        .map(|(idx, _)| idx)
        .last()
}

fn attach_comments(
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
                if let Some(e) = next_entry {
                    if e.blank_lines_before > 0 {
                        result_lines.push(String::new());
                    }
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

        if let Some(entry) = next_entry {
            if entry.signature == fmt_sig {
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

/// Format a single AST node to text.
///
/// Renders `node` using the same `rlsp-fmt` path as `format_yaml`.
/// The first output line starts at column 0; the caller is responsible for
/// positioning it within the larger document.  Every continuation line
/// (lines 2 and beyond) receives `base_indent` leading spaces so the output
/// aligns with the surrounding structure.
///
/// Empty collections (`{}`, `[]`) are emitted inline regardless of their
/// `CollectionStyle`, matching `node_to_doc`'s short-circuit behavior.
#[must_use]
pub fn format_subtree(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    base_indent: usize,
) -> String {
    let doc = node_to_doc(node, options, false);
    let fmt_options = FormatOptions {
        print_width: options.print_width,
        tab_width: options.tab_width,
        use_tabs: false,
    };
    let rendered = fmt_format(&doc, &fmt_options);
    // Strip the trailing newline that fmt_format appends, then re-join lines
    // with base_indent prepended to every continuation line.
    let text = rendered.trim_end_matches('\n');
    if base_indent == 0 {
        return text.to_string();
    }
    let indent_str = " ".repeat(base_indent);
    let mut lines = text.lines();
    lines.next().map_or_else(String::new, |first| {
        let rest = lines.fold(String::new(), |mut acc, l| {
            let _ = write!(acc, "\n{indent_str}{l}");
            acc
        });
        format!("{first}{rest}")
    })
}

/// Format a YAML document string.
///
/// Returns the formatted text. If the input fails to parse, returns the
/// original text unchanged so the caller never loses content.
///
/// Inter-node comments (between mapping entries and sequence items) are read
/// directly from the AST node fields populated by the loader.  Document-prefix
/// leading comments (before the first content node) are discarded by the
/// tokenizer and recovered via a raw-text scan of the preamble only.
#[must_use]
pub fn format_yaml(text_input: &str, options: &YamlFormatOptions) -> String {
    // The parser preserves scalar styles (plain, quoted, block) and tags natively.
    // No special configuration needed — every scalar carries its original style.
    let documents: Vec<Document<Span>> = match rlsp_yaml_parser::load(text_input) {
        Ok(docs) => docs,
        Err(_) => return text_input.to_string(),
    };

    if documents.is_empty() {
        return String::new();
    }

    // Extract only document-prefix comments (lines before the first content line).
    // Inter-node comments are embedded directly by node_to_doc via AST fields.
    let prefix_comments = extract_doc_prefix_comments(text_input);

    let fmt_options = FormatOptions {
        print_width: options.print_width,
        tab_width: options.tab_width,
        use_tabs: false,
    };

    // Apply duplicate-key removal pre-pass when enabled.
    let documents: Vec<Document<Span>> = if options.format_remove_duplicate_keys {
        documents
            .into_iter()
            .map(|mut doc| {
                dedup::dedup_mapping_keys(&mut doc.root);
                doc
            })
            .collect()
    } else {
        documents
    };

    // Build document parts respecting explicit_start and explicit_end markers.
    //
    // Rules:
    // - Emit `---` before a document when it has `explicit_start: true` (first
    //   doc) or when it is not the first document (separator always required).
    // - Emit `...` after a document when it has `explicit_end: true`.
    let doc_marker = text("---");
    let end_marker = text("...");
    let mut parts: Vec<Doc> = Vec::new();
    for (i, doc) in documents.iter().enumerate() {
        let is_first = i == 0;
        let needs_start_marker = !is_first || doc.explicit_start;
        if needs_start_marker {
            if !parts.is_empty() {
                parts.push(hard_line());
            }
            parts.push(doc_marker.clone());
            parts.push(hard_line());
        }
        parts.push(node_to_doc(&doc.root, options, false));
        if doc.explicit_end {
            parts.push(hard_line());
            parts.push(end_marker.clone());
        }
    }
    let joined = concat(parts);

    let mut result = fmt_format(&joined, &fmt_options);

    // Ensure output ends with a single newline before attach_comments.
    // attach_comments also guarantees a trailing newline, but the guard here
    // keeps the contract clear for readers.
    if !result.ends_with('\n') {
        result.push('\n');
    }

    // Reattach document-prefix comments and blank lines to the formatted output.
    // Always runs — blank line preservation requires a pass even when there are no comments.
    // attach_comments always produces LF output with a trailing newline.
    let last_content_hint = last_content_line_from_ast(&documents);
    result = attach_comments(text_input, &result, &prefix_comments, last_content_hint);

    // Apply line-ending substitution: replace all LF with the requested terminator.
    // attach_comments produces only LF, so this replace is safe (no CR complications).
    match options.line_ending {
        LineEnding::Lf => {}
        LineEnding::Crlf => {
            result = result.replace('\n', "\r\n");
        }
        LineEnding::Cr => {
            result = result.replace('\n', "\r");
        }
    }

    // Apply insert_final_newline policy: strip exactly one trailing terminator.
    if !options.insert_final_newline {
        match options.line_ending {
            LineEnding::Lf => {
                if result.ends_with('\n') {
                    result.pop();
                }
            }
            LineEnding::Crlf => {
                if result.ends_with("\r\n") {
                    result.truncate(result.len() - 2);
                }
            }
            LineEnding::Cr => {
                if result.ends_with('\r') {
                    result.pop();
                }
            }
        }
    }

    result
}

/// Extract only the leading comments that appear before the first content line
/// in the input.  These are comments the YAML tokenizer discards at the
/// `l_document_prefix` level and that therefore do not appear in the AST.
///
/// Stops at the first non-blank, non-comment line so inter-node comments
/// (which the loader now attaches to AST nodes) are not returned here.
fn extract_doc_prefix_comments(text: &str) -> Vec<Comment> {
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

/// Convert a `Node<Span>` to a `Doc` IR node.
///
/// When `in_key` is `true`, the `single_quote` style option is suppressed for
/// scalar strings — keys are never single-quoted by style preference alone.
#[expect(
    clippy::too_many_lines,
    reason = "comprehensive match over all node variants"
)]
fn node_to_doc(node: &Node<Span>, options: &YamlFormatOptions, in_key: bool) -> Doc {
    match node {
        Node::Scalar {
            value, style, tag, ..
        } => {
            // Prefix with a tag if present.
            //
            // Core schema tags (`tag:yaml.org,2002:*`) are handled as follows:
            //
            // - **Resolver-injected** (`tag_loc: None`): always stripped — the resolver
            //   injects these automatically and re-emitting them breaks idempotency.
            //
            // - **User-authored on a non-empty scalar** (`tag_loc: Some`, `value` non-empty):
            //   stripped — the type can be inferred from the value, so the tag adds
            //   no information and round-trips without it.
            //
            // - **User-authored on an empty scalar** (`tag_loc: Some`, `value` empty):
            //   emitted in short form (`!!str`, `!!null`, etc.) — the tag carries
            //   semantic meaning that cannot be inferred from an absent value.
            //
            // Non-core tags (user tags) are always emitted as-is.
            let tag_loc_is_some = node.tag_loc().is_some();
            let tag_prefix = tag.as_ref().and_then(|t| {
                if is_core_schema_tag(t) {
                    if tag_loc_is_some && value.is_empty() {
                        // User-authored explicit core tag on empty scalar: emit in short form.
                        let suffix = t.trim_start_matches("tag:yaml.org,2002:");
                        Some(format!("!!{suffix}"))
                    } else {
                        // Resolver-injected, or user-authored on non-empty scalar: suppress.
                        None
                    }
                } else {
                    // Non-empty scalar with user tag: include trailing space for separation.
                    // Empty scalar with user tag: no trailing space (value is absent).
                    let formatted = format_tag(t);
                    if value.is_empty() {
                        Some(formatted)
                    } else {
                        Some(format!("{formatted} "))
                    }
                }
            });

            let scalar_doc = match style {
                ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => {
                    // YAML treats a content line as a "blank line" when it consists
                    // solely of whitespace characters.  A blank line in a block scalar
                    // cannot carry more indentation than the declared indent level — if
                    // it does, re-parsers reject the output with "blank line has more
                    // indentation than the content".
                    //
                    // When the formatter emits a block scalar the indent() call adds the
                    // mapping/sequence indent to every line, including content lines that
                    // are entirely whitespace.  This pushes those lines beyond the
                    // declared indent, triggering the re-parse error.
                    //
                    // A line starting with a space character is problematic: after the
                    // indent strip the remaining content still starts with a space, so
                    // some parsers count it as a blank line.  A line starting with a tab
                    // is safe: the tab is treated as a non-blank content character even
                    // when the rest of the line is whitespace (e.g. `\t  ` round-trips
                    // correctly).
                    //
                    // Fall back to double-quoted output when any non-empty decoded line
                    // is entirely whitespace and starts with a space.  Such lines become
                    // over-indented blank lines after the formatter's indent() call and
                    // the re-parser rejects them.  A tab-first whitespace-only line (e.g.
                    // `\t  `) is safe and must not trigger the fallback.
                    let has_problematic_whitespace_line = !value.is_empty()
                        && value.lines().filter(|l| !l.is_empty()).any(|l| {
                            l.starts_with(' ') && l.chars().all(|c| c == ' ' || c == '\t')
                        });
                    if has_problematic_whitespace_line {
                        text(format!("\"{}\"", escape_double_quoted(value)))
                    } else {
                        repr_block_to_doc(value, *style, options.tab_width)
                    }
                }
                ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                    if requires_double_quoting(value) {
                        // Decoded value contains chars that cannot appear unquoted
                        // or in single-quoted scalars (control chars, backslash,
                        // etc.) — always re-emit as double-quoted with proper
                        // escaping regardless of original style.
                        text(format!("\"{}\"", escape_double_quoted(value)))
                    } else if needs_quoting(value, options.yaml_version) {
                        if matches!(style, ScalarStyle::DoubleQuoted) {
                            text(format!("\"{}\"", escape_double_quoted(value)))
                        } else {
                            // Single-quoted: escape embedded single quotes as ''.
                            text(format!("'{}'", value.replace('\'', "''")))
                        }
                    } else if options.preserve_quotes {
                        // Safe-plain scalar: reproduce the source quote style
                        // instead of stripping to plain.
                        if matches!(style, ScalarStyle::DoubleQuoted) {
                            text(format!("\"{}\"", escape_double_quoted(value)))
                        } else {
                            text(format!("'{}'", value.replace('\'', "''")))
                        }
                    } else {
                        string_to_doc(value, options, in_key)
                    }
                }
                ScalarStyle::Plain => {
                    // Values that contain characters which cannot appear in a plain scalar
                    // at all — control characters, backslashes, or embedded newlines —
                    // must be emitted as double-quoted with proper escaping.
                    if requires_double_quoting(value) {
                        text(format!("\"{}\"", escape_double_quoted(value)))
                    } else if needs_quoting(value, options.yaml_version) {
                        // Value needs quoting (reserved keyword, special char, etc.) but
                        // was originally plain — preserve plain style so round-trip matches.
                        text(value.clone())
                    } else {
                        string_to_doc(value, options, in_key)
                    }
                }
            };

            // `tag_present_on_empty` is true when a tag is being preserved for
            // an empty scalar — the tag text itself is the entire output, so any
            // anchor prefix must be separated from it by a space.
            let tag_present_on_empty = tag_prefix.is_some() && value.is_empty();

            let doc = if let Some(ref prefix) = tag_prefix {
                // For non-empty scalars the prefix already ends with a space.
                // For empty scalars the prefix has no trailing space (value is absent).
                if value.is_empty() {
                    text(prefix.clone())
                } else {
                    concat(vec![text(prefix.clone()), scalar_doc])
                }
            } else {
                scalar_doc
            };

            if let Some(name) = node.anchor() {
                // When the scalar is empty we still need a space between the
                // anchor name and whatever follows (a tag or nothing).
                if value.is_empty() {
                    if tag_present_on_empty {
                        // `&anchor !!tag` — space required between anchor and tag.
                        concat(vec![text(format!("&{name} ")), doc])
                    } else {
                        // `&anchor` alone — no trailing space.
                        concat(vec![text(format!("&{name}")), doc])
                    }
                } else {
                    concat(vec![text(format!("&{name} ")), doc])
                }
            } else {
                doc
            }
        }

        Node::Mapping {
            entries,
            style,
            tag,
            ..
        } => {
            let doc = mapping_render::mapping_to_doc(entries, *style, options);
            let effective_style = if options.format_enforce_block_style {
                CollectionStyle::Block
            } else {
                *style
            };
            mapping_render::prepend_collection_properties(
                doc,
                node.anchor(),
                tag.as_deref(),
                effective_style,
            )
        }

        Node::Sequence {
            items, style, tag, ..
        } => {
            let doc = sequence_render::sequence_to_doc(items, *style, options);
            let effective_style = if options.format_enforce_block_style {
                CollectionStyle::Block
            } else {
                *style
            };
            mapping_render::prepend_collection_properties(
                doc,
                node.anchor(),
                tag.as_deref(),
                effective_style,
            )
        }

        Node::Alias { name, .. } => text(format!("*{name}")),
    }
}

/// Emit a node for use inside a flow collection (flow sequence or flow mapping).
///
/// For plain scalars that contain flow-unsafe characters, wraps in double quotes
/// so they are not misread as separators or delimiters by a YAML parser.
fn flow_item_to_doc(node: &Node<Span>, options: &YamlFormatOptions, in_key: bool) -> Doc {
    match node {
        Node::Scalar {
            value,
            style: ScalarStyle::Plain,
            ..
        } if node.anchor().is_none() && needs_flow_quoting(value) => {
            text(format!("\"{}\"", escape_double_quoted(value)))
        }
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            node_to_doc(node, options, in_key)
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn default_opts() -> YamlFormatOptions {
        YamlFormatOptions::default()
    }

    // ---- Group: Multi-contains checks (basic formatting) ----

    #[rstest]
    #[case::boolean_values("enabled: true\ndisabled: false\n", &["true", "false"] as &[&str])]
    #[case::numeric_values("port: 8080\nratio: 0.5\n", &["8080", "0.5"])]
    #[case::mapping_block_style("a: 1\nb: 2\n", &["a: 1", "b: 2"])]
    #[case::flow_sequence_items("items:\n  - a\n  - b\n  - c\n", &["a", "b", "c"])]
    #[case::multi_document(
        "key1: value1\n---\nkey2: value2\n",
        &["key1: value1", "---", "key2: value2"]
    )]
    #[case::float_special_values(
        "nan_val: .nan\ninf_val: .inf\nneg_inf_val: -.inf\n",
        &[".nan", ".inf", "-.inf"]
    )]
    #[case::tagged_node("tagged: !mytag some_value\n", &["!mytag", "some_value"])]
    #[case::literal_block_scalar(
        "body: |\n  line one\n  line two\n",
        &["|", "line one", "line two"]
    )]
    #[case::folded_block_scalar("body: >\n  folded line\n", &[">", "folded line"])]
    #[case::single_quoted_scalar_content("key: 'quoted value'\n", &["quoted value", "key:"])]
    #[case::double_quoted_scalar_content("key: \"quoted value\"\n", &["quoted value", "key:"])]
    fn format_yaml_multi_contains(#[case] input: &str, #[case] expected: &[&str]) {
        let result = format_yaml(input, &default_opts());
        for &s in expected {
            assert!(result.contains(s), "{s:?} missing: {result:?}");
        }
    }

    // ---- Group K: Anchor preservation ----

    // K1: Anchor on a scalar value is preserved.
    #[test]
    fn anchor_scalar_preserved() {
        let result = format_yaml("key: &anchor value\n", &default_opts());
        assert_eq!(result, "key: &anchor value\n");
    }

    // K2: Anchor on a block mapping value is preserved.
    #[test]
    fn anchor_block_mapping_preserved() {
        let result = format_yaml("defaults: &defaults\n  timeout: 30\n", &default_opts());
        assert_eq!(result, "defaults: &defaults\n  timeout: 30\n");
    }

    // K3: Anchor on a block sequence value is preserved.
    #[test]
    fn anchor_block_sequence_preserved() {
        let result = format_yaml("items: &mylist\n  - a\n  - b\n", &default_opts());
        assert_eq!(result, "items: &mylist\n  - a\n  - b\n");
    }

    // K4: Anchor on a flow mapping value is preserved.
    #[test]
    fn anchor_flow_mapping_preserved() {
        let result = format_yaml("key: &anchor {a: 1}\n", &default_opts());
        assert!(result.contains("&anchor"), "anchor missing: {result:?}");
    }

    // K5: Anchor on a flow sequence value is preserved.
    #[test]
    fn anchor_flow_sequence_preserved() {
        let result = format_yaml("key: &anchor [a, b]\n", &default_opts());
        assert_eq!(result, "key: &anchor [a, b]\n");
    }

    // K6: Anchor on a block-mapping sequence item is preserved.
    #[test]
    fn anchor_sequence_item_block_mapping_preserved() {
        let result = format_yaml("items:\n  - &item\n    key: val\n", &default_opts());
        assert_eq!(result, "items:\n  - &item\n    key: val\n");
    }

    // K7: Alias reference (`*name`) is preserved (regression guard).
    #[test]
    fn alias_reference_preserved() {
        let result = format_yaml(
            "defaults: &defaults\n  timeout: 30\nservice:\n  <<: *defaults\n",
            &default_opts(),
        );
        assert!(result.contains("&defaults"), "anchor missing: {result:?}");
        assert!(result.contains("*defaults"), "alias missing: {result:?}");
    }

    // K8: Anchor+alias round-trip is idempotent.
    #[test]
    fn anchor_alias_idempotent() {
        let input = "defaults: &defaults\n  timeout: 30\nservice:\n  <<: *defaults\n";
        let first = format_yaml(input, &default_opts());
        let second = format_yaml(&first, &default_opts());
        assert_eq!(first, second, "anchor/alias not idempotent: {first:?}");
    }

    // AP-2: Anchor on a top-level plain scalar is preserved.
    #[test]
    fn anchor_on_top_level_scalar_preserved() {
        let result = format_yaml("&doc hello\n", &default_opts());
        assert_eq!(result, "&doc hello\n");
    }

    // AP-10: Anchor and alias round-trip on a sequence value.
    #[test]
    fn anchor_and_alias_round_trip_sequence() {
        let input = "base: &base\n  - x\n  - y\nextended:\n  - *base\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("&base"), "anchor missing: {result:?}");
        assert!(result.contains("- x"), "sequence item missing: {result:?}");
        assert!(result.contains("*base"), "alias missing: {result:?}");
    }

    // AP-12: Anchor and user tag on same scalar — anchor precedes tag (YAML spec §6.8.1).
    #[test]
    fn anchor_before_tag_on_scalar() {
        let result = format_yaml("item: &myanchor !mytag value\n", &default_opts());
        assert!(result.contains("&myanchor"), "anchor missing: {result:?}");
        assert!(result.contains("!mytag"), "tag missing: {result:?}");
        assert!(result.contains("value"), "value missing: {result:?}");
        // Anchor must precede tag in the output string (YAML spec §6.8.1).
        // Split on the tag: the prefix must contain the anchor.
        let before_tag = result.split("!mytag").next().unwrap_or("");
        assert!(
            before_tag.contains("&myanchor"),
            "anchor must precede tag per YAML spec §6.8.1: {result:?}"
        );
    }

    // AP-13: Anchor coexists with trailing inline comment.
    #[test]
    fn anchor_with_trailing_comment_preserved() {
        let result = format_yaml("key: &anchor value  # inline comment\n", &default_opts());
        assert!(
            result.contains("&anchor value"),
            "anchor+value missing: {result:?}"
        );
        assert!(
            result.contains("# inline comment"),
            "comment missing: {result:?}"
        );
    }

    // AP-14: Anchor on an empty flow mapping is preserved.
    #[test]
    fn anchor_on_empty_flow_mapping_preserved() {
        let result = format_yaml("empty: &empty {}\n", &default_opts());
        assert_eq!(result, "empty: &empty {}\n");
    }

    // AP-15: Anchor on an empty flow sequence is preserved.
    #[test]
    fn anchor_on_empty_flow_sequence_preserved() {
        let result = format_yaml("empty: &empty []\n", &default_opts());
        assert_eq!(result, "empty: &empty []\n");
    }

    // AP-16: No spurious `&` sigil is injected when no anchor is defined.
    #[test]
    fn no_spurious_anchor_when_none() {
        let result = format_yaml("key: value\n", &default_opts());
        assert!(
            !result.contains('&'),
            "spurious anchor in output: {result:?}"
        );
    }

    // ---- Group L: Document marker flag emission ----

    // FM-1 (spike): Single bare document — no `---` or `...` emitted
    #[test]
    fn bare_document_emits_no_markers() {
        let result = format_yaml("key: value\n", &default_opts());
        assert!(result.contains("key: value"), "content missing: {result:?}");
        assert!(
            !result.contains("---"),
            "unexpected `---` in output: {result:?}"
        );
        assert!(
            !result.contains("..."),
            "unexpected `...` in output: {result:?}"
        );
    }

    // FM-2: Single document with explicit `---` start → `---` preserved in output
    #[test]
    fn explicit_start_marker_preserved() {
        let result = format_yaml("---\nkey: value\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` missing from output: {result:?}"
        );
    }

    // FM-3: Single document with explicit `...` end → `...` preserved in output
    #[test]
    fn explicit_end_marker_preserved() {
        let result = format_yaml("key: value\n...\n", &default_opts());
        assert!(
            result.contains("..."),
            "`...` missing from output: {result:?}"
        );
    }

    // FM-4: Single document with both `---` and `...` → both preserved
    #[test]
    fn both_markers_preserved() {
        let result = format_yaml("---\nkey: value\n...\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` missing from output: {result:?}"
        );
        assert!(
            result.contains("..."),
            "`...` missing from output: {result:?}"
        );
    }

    // FM-5: Multi-document — `---` separator always emitted between docs
    #[test]
    fn multi_document_separator_always_emitted() {
        let result = format_yaml("doc1: a\n---\ndoc2: b\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` separator missing: {result:?}"
        );
        assert!(
            result.contains("doc1: a"),
            "doc1 content missing: {result:?}"
        );
        assert!(
            result.contains("doc2: b"),
            "doc2 content missing: {result:?}"
        );
    }

    // FM-6: Multi-document — `...` terminator on first doc only, not second
    #[test]
    fn explicit_end_only_on_first_document() {
        let result = format_yaml("doc1: a\n...\n---\ndoc2: b\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` separator missing: {result:?}"
        );
        // The full output should be exactly: doc1 content, ..., ---, doc2 content.
        // The `...` appears before `doc2: b`, not after it.
        assert!(
            result.contains("..."),
            "`...` missing from output: {result:?}"
        );
        assert!(
            result.find("...") < result.find("doc2: b"),
            "`...` should appear before doc2, got: {result:?}"
        );
        // The portion after `doc2: b` must not contain `...`.
        let after_doc2 = result.find("doc2: b").map_or("", |pos| &result[pos..]);
        assert!(
            !after_doc2.contains("..."),
            "unexpected `...` after doc2: {result:?}"
        );
    }

    // FM-7: Multi-document — `...` on all documents → all preserved
    #[test]
    fn explicit_end_on_all_documents_preserved() {
        let result = format_yaml("doc1: a\n...\n---\ndoc2: b\n...\n", &default_opts());
        // Both `...` markers should be present
        let count = result.matches("...").count();
        assert_eq!(
            count, 2,
            "expected 2 `...` markers, got {count}: {result:?}"
        );
    }

    // ---- Group FS: format_subtree unit tests ----

    fn parse_root(src: &str) -> Node<Span> {
        rlsp_yaml_parser::load(src)
            .expect("test input must parse")
            .remove(0)
            .root
    }

    // FS-1: scalar node, base_indent 0 — single line, no indent applied
    #[test]
    fn format_subtree_scalar_base_indent_zero() {
        let node = parse_root("hello");
        let result = format_subtree(&node, &default_opts(), 0);
        assert_eq!(result, "hello");
    }

    // FS-2: scalar node, base_indent 4 — first line never indented
    #[test]
    fn format_subtree_scalar_base_indent_never_indents_first_line() {
        let node = parse_root("hello");
        let result = format_subtree(&node, &default_opts(), 4);
        assert_eq!(result, "hello");
    }

    // FS-3: empty mapping emits `{}` — records mapping_to_doc short-circuit
    #[test]
    fn format_subtree_empty_mapping_emits_inline() {
        let node = parse_root("{}");
        let result = format_subtree(&node, &default_opts(), 0);
        assert_eq!(result, "{}");
    }

    // FS-4: empty sequence emits `[]` — records sequence_to_doc short-circuit
    #[test]
    fn format_subtree_empty_sequence_emits_inline() {
        let node = parse_root("[]");
        let result = format_subtree(&node, &default_opts(), 0);
        assert_eq!(result, "[]");
    }

    // FS-5 through FS-7: block mapping with various base_indent values
    #[rstest]
    #[case::indent_zero(0, "a: 1", "b: 2")]
    #[case::indent_two(2, "a: 1", "  b: 2")]
    #[case::indent_eight(8, "a: 1", "        b: 2")]
    fn format_subtree_block_mapping_base_indent(
        #[case] base_indent: usize,
        #[case] expected_line0: &str,
        #[case] expected_line1: &str,
    ) {
        let node = parse_root("a: 1\nb: 2\n");
        let result = format_subtree(&node, &default_opts(), base_indent);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, expected_line0, "line 0 mismatch: {result:?}");
                assert_eq!(*line1, expected_line1, "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-8: block sequence, base_indent 2
    #[test]
    fn format_subtree_block_sequence_continuation_indented() {
        let node = parse_root("- x\n- y\n");
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "- x", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  - y", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-9: nested mapping inside sequence, base_indent 2
    #[test]
    fn format_subtree_nested_mapping_in_sequence_base_indent() {
        let node = parse_root("- a: 1\n  b: 2\n- c: 3\n");
        let result = format_subtree(&node, &default_opts(), 2);
        let lines: Vec<&str> = result.lines().collect();
        // First line has no leading spaces regardless of base_indent
        let first = lines.first().expect("output must have at least one line");
        assert!(
            first.starts_with("- a: 1"),
            "first line should start with `- a: 1`: {result:?}"
        );
        // The `- c: 3` item must have two leading spaces from base_indent
        let c_line = lines
            .iter()
            .find(|l| l.contains("c: 3"))
            .copied()
            .expect("output must contain `c: 3`");
        assert!(
            c_line.starts_with("  - c: 3"),
            "`- c: 3` line should have two leading spaces: {result:?}"
        );
    }

    // FS-10: enforce_block_style option converts flow mapping to block (tests the options-flag path)
    #[test]
    fn format_subtree_enforce_block_style_option_converts_flow_to_block() {
        let node = parse_root("{a: 1, b: 2}");
        let opts = YamlFormatOptions {
            format_enforce_block_style: true,
            ..YamlFormatOptions::default()
        };
        let result = format_subtree(&node, &opts, 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "a: 1", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  b: 2", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-11: flow mapping node → block via direct style mutation (Task 2 mechanism), base_indent 2
    #[test]
    fn format_subtree_flow_mapping_style_mutation_to_block() {
        let mut node = parse_root("{a: 1, b: 2}");
        if let Node::Mapping { style, .. } = &mut node {
            *style = CollectionStyle::Block;
        }
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "a: 1", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  b: 2", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-12: flow sequence node → block via direct style mutation, base_indent 2
    #[test]
    fn format_subtree_flow_sequence_style_mutation_to_block() {
        let mut node = parse_root("[a, b, c]");
        if let Node::Sequence { style, .. } = &mut node {
            *style = CollectionStyle::Block;
        }
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, line2, ..] => {
                assert_eq!(*line0, "- a", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  - b", "line 1 mismatch: {result:?}");
                assert_eq!(*line2, "  - c", "line 2 mismatch: {result:?}");
            }
            other => panic!("expected at least 3 lines, got: {other:?}"),
        }
    }

    // FS-13: nested flow mappings inside a flow sequence, both flipped to block via style mutation
    #[test]
    fn format_subtree_nested_flow_in_flow_sequence_to_block() {
        let mut node = parse_root("[{a: 1}, {b: 2}]");
        // Flip outer sequence and each inner mapping to Block — mimics Task 2's approach
        if let Node::Sequence { style, items, .. } = &mut node {
            *style = CollectionStyle::Block;
            for item in items.iter_mut() {
                if let Node::Mapping { style: ms, .. } = item {
                    *ms = CollectionStyle::Block;
                }
            }
        }
        let result = format_subtree(&node, &default_opts(), 2);
        // Each sequence item becomes a `- key: val` block entry
        assert!(result.contains("a: 1"), "a: 1 missing: {result:?}");
        assert!(result.contains("b: 2"), "b: 2 missing: {result:?}");
        // Continuation lines must have 2 leading spaces
        let second_item_line = result
            .lines()
            .find(|l| l.contains("b: 2"))
            .expect("line with b: 2 must exist");
        assert!(
            second_item_line.starts_with("  "),
            "second item line must be indented by 2: {result:?}"
        );
    }

    // FS-14: multi-line flow mapping input converted to block via style mutation
    #[test]
    fn format_subtree_multiline_flow_mapping_to_block() {
        let mut node = parse_root("{\n  a: 1,\n  b: 2,\n}");
        if let Node::Mapping { style, .. } = &mut node {
            *style = CollectionStyle::Block;
        }
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "a: 1", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  b: 2", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // ---- line_ending and insert_final_newline fields ------------------------

    // F1: LineEnding::Crlf replaces all LF with CRLF.
    #[test]
    fn line_ending_crlf_replaces_all_newlines() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Crlf,
            ..default_opts()
        };
        let output = format_yaml("a: 1\nb: 2\n", &opts);
        assert!(output.contains("\r\n"), "output should contain CRLF");
        for (i, ch) in output.char_indices() {
            if ch == '\n' {
                assert!(
                    i > 0 && output.as_bytes()[i - 1] == b'\r',
                    "bare LF at byte {i}"
                );
            }
        }
    }

    // F2: LineEnding::Cr replaces all LF with CR.
    #[test]
    fn line_ending_cr_replaces_all_newlines() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Cr,
            ..default_opts()
        };
        let output = format_yaml("a: 1\nb: 2\n", &opts);
        assert!(!output.contains('\n'), "output should have no LF");
        assert!(output.contains('\r'), "output should have at least one CR");
        assert!(!output.contains("\r\n"), "output should have no CRLF");
    }

    // F3: LineEnding::Lf leaves output unchanged (LF in, LF out).
    #[test]
    fn line_ending_lf_leaves_output_unchanged() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Lf,
            ..default_opts()
        };
        let output = format_yaml("a: 1\nb: 2\n", &opts);
        assert!(!output.contains('\r'), "LF mode should produce no CR");
        assert!(output.ends_with('\n'), "LF mode should end with LF");
    }

    // F4: insert_final_newline = false strips the trailing LF.
    #[test]
    fn insert_final_newline_false_strips_trailing_newline() {
        let opts = YamlFormatOptions {
            insert_final_newline: false,
            ..default_opts()
        };
        let output = format_yaml("key: value\n", &opts);
        assert_eq!(
            output, "key: value",
            "trailing newline should be stripped; got: {output:?}"
        );
    }

    // F5: insert_final_newline = true leaves the trailing LF in place.
    #[test]
    fn insert_final_newline_true_leaves_trailing_newline() {
        let opts = YamlFormatOptions {
            insert_final_newline: true,
            ..default_opts()
        };
        let output = format_yaml("key: value\n", &opts);
        assert!(
            output.ends_with('\n'),
            "trailing newline should be preserved; got: {output:?}"
        );
    }

    // F6: insert_final_newline = false with Crlf strips the trailing CRLF.
    #[test]
    fn insert_final_newline_false_with_crlf_strips_crlf_terminator() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Crlf,
            insert_final_newline: false,
            ..default_opts()
        };
        let output = format_yaml("key: value\n", &opts);
        assert!(
            !output.ends_with("\r\n") && !output.ends_with('\n') && !output.ends_with('\r'),
            "trailing CRLF terminator should be stripped; got: {output:?}"
        );
        assert!(
            output.ends_with("value"),
            "content should be intact; got: {output:?}"
        );
    }

    // F7: Default options still end with newline (regression guard).
    #[test]
    fn format_yaml_default_options_still_ends_with_newline() {
        let output = format_yaml("key: value\n", &default_opts());
        assert!(
            output.ends_with('\n'),
            "default options should preserve trailing newline; got: {output:?}"
        );
    }
}
