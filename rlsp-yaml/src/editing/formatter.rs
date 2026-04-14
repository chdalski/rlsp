// SPDX-License-Identifier: MIT

use std::fmt::Write as _;

use rlsp_fmt::{
    Doc, FormatOptions, concat, flat_alt, format as fmt_format, group, hard_line, indent, join,
    line, text,
};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{Chomp, ScalarStyle, Span};

use crate::server::YamlVersion;

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
/// Returns the index of the last line in `original` that is actual YAML
/// content — neither blank nor a standalone comment outside `line_to_comment`.
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

fn attach_comments(original: &str, formatted: &str, comments: &[Comment]) -> String {
    // Build a quick lookup: line index -> comment.
    let line_to_comment: std::collections::HashMap<usize, &Comment> =
        comments.iter().map(|c| (c.line, c)).collect();

    // Standalone `#` lines after this index are EOF-trailing comments not
    // embedded in the AST; collect them to append verbatim.  Lines before or
    // at this index that start with `#` are inter-node comments already
    // emitted by the AST formatter — skip them or they will be duplicated.
    let last_content_idx = last_content_line_idx(original, &line_to_comment);

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
        } else if line.trim().is_empty() {
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

/// YAML-specific formatting options.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is a distinct, well-named formatting option; a flags enum would add complexity for no benefit"
)]
// When adding or changing settings, check fixture coverage for setting
// interactions — see rlsp-yaml/tests/fixtures/CLAUDE.md.
pub struct YamlFormatOptions {
    /// Maximum line width. Default: 80.
    pub print_width: usize,
    /// Spaces per indent level. Default: 2.
    pub tab_width: usize,
    /// Use tabs instead of spaces. Default: false.
    pub use_tabs: bool,
    /// Prefer single-quoted strings. Default: false (double quotes).
    pub single_quote: bool,
    /// Add spaces inside flow braces: `{ a: 1 }` vs `{a: 1}`. Default: true.
    pub bracket_spacing: bool,
    /// YAML specification version for quoting decisions. Default: `V1_2`.
    pub yaml_version: YamlVersion,
    /// Override all collection styles to block. When `true`, flow sequences and
    /// flow mappings are emitted in block style regardless of the source style.
    /// Default: false.
    pub format_enforce_block_style: bool,
    /// Remove duplicate mapping keys before formatting, keeping the last
    /// occurrence (YAML spec: last value wins). Default: false.
    pub format_remove_duplicate_keys: bool,
}

impl Default for YamlFormatOptions {
    fn default() -> Self {
        Self {
            print_width: 80,
            tab_width: 2,
            use_tabs: false,
            single_quote: false,
            bracket_spacing: true,
            yaml_version: YamlVersion::V1_2,
            format_enforce_block_style: false,
            format_remove_duplicate_keys: false,
        }
    }
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
        use_tabs: options.use_tabs,
    };

    // Apply duplicate-key removal pre-pass when enabled.
    let documents: Vec<Document<Span>> = if options.format_remove_duplicate_keys {
        documents
            .into_iter()
            .map(|mut doc| {
                dedup_mapping_keys(&mut doc.root);
                doc
            })
            .collect()
    } else {
        documents
    };

    // Join multiple documents with `---` separators.
    let sep = text("---");
    let mut parts: Vec<Doc> = Vec::new();
    let mut iter = documents
        .iter()
        .map(|doc| node_to_doc(&doc.root, options, false));
    if let Some(first) = iter.next() {
        parts.push(first);
    }
    for doc in iter {
        parts.push(hard_line());
        parts.push(sep.clone());
        parts.push(hard_line());
        parts.push(doc);
    }
    let joined = concat(parts);

    let mut result = fmt_format(&joined, &fmt_options);

    // Ensure output ends with a single newline.
    if !result.ends_with('\n') {
        result.push('\n');
    }

    // Reattach document-prefix comments and blank lines to the formatted output.
    // Always runs — blank line preservation requires a pass even when there are no comments.
    result = attach_comments(text_input, &result, &prefix_comments);

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
            value,
            style,
            anchor,
            tag,
            ..
        } => {
            // Prefix with a tag if present.
            //
            // Core schema tags (`tag:yaml.org,2002:*`) are normally stripped —
            // the type can be inferred from the value.  The exception: when the
            // scalar is **empty**, the tag carries semantic meaning that cannot
            // be inferred (`!!str` → empty string; `!!null` → null; etc.), so
            // it must be preserved as its short form (e.g. `!!str`).
            let tag_prefix = tag.as_ref().and_then(|t| {
                if is_core_schema_tag(t) {
                    if value.is_empty() {
                        // Convert full URI to short form: `tag:yaml.org,2002:str` → `!!str`
                        let suffix = t.trim_start_matches("tag:yaml.org,2002:");
                        Some(format!("!!{suffix}"))
                    } else {
                        None
                    }
                } else {
                    // Non-empty scalar with user tag: include trailing space for separation.
                    // Empty scalar with user tag: no trailing space (value is absent).
                    if value.is_empty() {
                        Some(t.clone())
                    } else {
                        Some(format!("{t} "))
                    }
                }
            });

            let scalar_doc = match style {
                ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => {
                    repr_block_to_doc(value, *style, options.tab_width)
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
                    } else {
                        string_to_doc(value, options, in_key)
                    }
                }
                ScalarStyle::Plain => {
                    // Values starting with a quote character cannot be emitted as plain —
                    // they would look like an unterminated quoted scalar to a re-parser.
                    // Emit these with proper escaping via string_to_doc instead.
                    if value.starts_with('"') || value.starts_with('\'') {
                        string_to_doc(value, options, in_key)
                    } else if needs_quoting(value, options.yaml_version) {
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

            if let Some(name) = anchor {
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
            anchor,
            tag,
            ..
        } => {
            let doc = mapping_to_doc(entries, *style, options);
            let effective_style = if options.format_enforce_block_style {
                CollectionStyle::Block
            } else {
                *style
            };
            prepend_collection_properties(doc, anchor.as_deref(), tag.as_deref(), effective_style)
        }

        Node::Sequence {
            items,
            style,
            anchor,
            tag,
            ..
        } => {
            let doc = sequence_to_doc(items, *style, options);
            let effective_style = if options.format_enforce_block_style {
                CollectionStyle::Block
            } else {
                *style
            };
            prepend_collection_properties(doc, anchor.as_deref(), tag.as_deref(), effective_style)
        }

        Node::Alias { name, .. } => text(format!("*{name}")),
    }
}

/// Returns `true` if the tag string is a YAML Core Schema tag.
fn is_core_schema_tag(tag: &str) -> bool {
    tag.starts_with("tag:yaml.org,2002:")
}

/// Prepend anchor and user-defined tag node properties to a collection Doc.
///
/// For **block** collections the properties must appear on their own line — emitting
/// `&anchor ` as inline text before the first block indicator (`-` or `key:`) produces
/// invalid YAML such as `&anchor - item`.  A `hard_line()` separates the properties
/// from the collection content.
///
/// For **flow** collections the properties stay inline: `&anchor {key: val}`.
///
/// Order: tag first (inner), then anchor (outer) — producing `&anchor !tag content`.
/// Core schema tags (`tag:yaml.org,2002:*`) are silently dropped for collections.
fn prepend_collection_properties(
    doc: Doc,
    anchor: Option<&str>,
    tag: Option<&str>,
    style: CollectionStyle,
) -> Doc {
    let tag_prefix = tag.and_then(|t| {
        if is_core_schema_tag(t) {
            None
        } else {
            Some(t.to_string())
        }
    });

    // Build the properties string: `&anchor !tag` or just one of them.
    let props = match (anchor, tag_prefix.as_deref()) {
        (Some(name), Some(t)) => Some(format!("&{name} {t}")),
        (Some(name), None) => Some(format!("&{name}")),
        (None, Some(t)) => Some(t.to_string()),
        (None, None) => None,
    };

    let Some(props_str) = props else {
        return doc;
    };

    match style {
        CollectionStyle::Block => {
            // Block collections: properties on own line, then hard-break to content.
            concat(vec![text(props_str), hard_line(), doc])
        }
        CollectionStyle::Flow => {
            // Flow collections: properties inline before the opening bracket.
            concat(vec![text(format!("{props_str} ")), doc])
        }
    }
}

/// Convert a string scalar to a Doc, quoting as necessary.
///
/// When `in_key` is `true`, the `single_quote` option is ignored — keys are
/// never wrapped in single quotes by style preference alone.
fn string_to_doc(s: &str, options: &YamlFormatOptions, in_key: bool) -> Doc {
    if needs_quoting(s, options.yaml_version) {
        // Must quote — use the preferred style.
        if options.single_quote && !s.contains('\'') {
            text(format!("'{s}'"))
        } else {
            // Double-quote and escape.
            text(format!("\"{}\"", escape_double_quoted(s)))
        }
    } else if options.single_quote && !in_key {
        text(format!("'{s}'"))
    } else {
        // Plain — no quotes needed.
        text(s.to_string())
    }
}

/// Returns true if a string value requires quoting to avoid YAML ambiguity.
///
/// The `version` parameter controls whether YAML 1.1-only boolean keywords
/// (`yes`, `no`, `on`, `off` and their capitalised variants) count as reserved.
/// In YAML 1.2 those words are plain strings and do not need quoting.
fn needs_quoting(s: &str, version: YamlVersion) -> bool {
    if s.is_empty() {
        return true;
    }

    // All-whitespace values would be trimmed to nothing by YAML's flow-scalar
    // trimming rules, so they must be quoted.
    if s.chars().all(char::is_whitespace) {
        return true;
    }

    // Values with leading or trailing whitespace lose those spaces when emitted as
    // a plain scalar and re-parsed — YAML trims leading and trailing whitespace from
    // plain scalars, so the formatter output would not be idempotent.
    if s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) {
        return true;
    }

    // Values that are reserved YAML keywords in all versions.
    let always_reserved = matches!(
        s,
        "null" | "~" | "true" | "false" | "Null" | "NULL" | "True" | "TRUE" | "False" | "FALSE"
    );

    // Values that are reserved only under YAML 1.1.
    let v1_1_reserved = version == YamlVersion::V1_1
        && matches!(
            s,
            "yes" | "no" | "on" | "off" | "Yes" | "No" | "On" | "Off" | "YES" | "NO" | "ON" | "OFF"
        );

    always_reserved
        || v1_1_reserved
        || looks_like_number(s)
        || s.starts_with(|c: char| {
            matches!(
                c,
                ':' | '#'
                    | '&'
                    | '*'
                    | '?'
                    | '|'
                    | '-'
                    | '<'
                    | '>'
                    | '='
                    | '!'
                    | '%'
                    | '@'
                    | '`'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '"'
                    | '\''
            )
        })
        || s.contains(": ")
        || s.contains(" #")
        || s.starts_with("- ")
        || s.starts_with("--- ")
        || s == "---"
        || s == "..."
}

/// Returns true if the string looks like a YAML number (integer or float).
fn looks_like_number(s: &str) -> bool {
    s.parse::<i64>().is_ok()
        || s.parse::<f64>().is_ok()
        || matches!(
            s,
            ".inf" | ".Inf" | ".INF" | "+.inf" | "-.inf" | ".nan" | ".NaN" | ".NAN"
        )
}

/// Returns `true` if the decoded string value contains characters that require
/// double-quoting to represent in YAML — control characters, backslash, or any
/// other C0 character (U+0000–U+001F).
///
/// This check must happen *before* `needs_quoting` in the `DoubleQuoted` branch
/// so that decoded values with raw control bytes are never emitted as plain
/// scalars (which would produce unparseable YAML).
fn requires_double_quoting(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c, '\\')
            || (c as u32) <= 0x1F
            || c == '\u{0085}' // NEL
            || c == '\u{2028}' // line separator
            || c == '\u{2029}' // paragraph separator
    })
}

/// Escape a string for use in a double-quoted YAML scalar.
///
/// Handles all YAML 1.2 §5.7 named escapes and falls back to `\xNN` hex
/// notation for remaining C0 control characters.
fn escape_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x00' => out.push_str("\\0"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\x0B' => out.push_str("\\v"),
            '\x0C' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\x1B' => out.push_str("\\e"),
            '\u{0085}' => out.push_str("\\N"),
            '\u{00A0}' => out.push_str("\\_"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            c if (c as u32) <= 0x1F => {
                // Remaining C0 controls as \xNN
                let _ = write!(out, "\\x{:02X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Convert a block scalar to Doc using hard lines.
///
/// The parser preserves the original chomping indicator, so we emit it
/// faithfully (`|`, `|-`, `|+`, `>`, `>-`, `>+`).
///
/// Content lines are wrapped in `indent()` so the Wadler-Lindig printer
/// indents them one level relative to the surrounding context.
///
/// When any content line begins with a space or tab, an explicit indentation
/// indicator digit equal to `tab_width` is appended to the header (e.g. `|2`
/// or `>2`).  Without it the YAML parser would auto-detect indentation from the
/// first content line and misparse any line whose content starts with a leading
/// space.
///
/// For literal scalars, blank lines (empty strings from `str::lines()`) are
/// omitted here — `attach_comments` re-inserts them from the original input
/// when it matches content signatures.
///
/// For folded scalars, blank lines must be emitted explicitly because folding
/// semantics require them to represent embedded newlines in the decoded value.
/// N blank lines between two content lines produces N newlines in the value;
/// without them, adjacent content lines would fold into a single space on
/// re-parse, making the output non-idempotent.
fn repr_block_to_doc(s: &str, style: ScalarStyle, tab_width: usize) -> Doc {
    // Detect whether any non-empty content line starts with a space character.
    // A leading space in the decoded value means the YAML parser would
    // auto-detect a higher indentation level than intended (treating the extra
    // space as indentation rather than content) — emitting an explicit indicator
    // digit equal to tab_width prevents this misparse.
    //
    // Tabs at the start of content are NOT checked: in YAML, tabs cannot be
    // used for indentation (YAML 1.2 §8.1.1), so a leading tab in the decoded
    // value is content that sits at the base indentation level and does not
    // cause the parser to auto-detect a higher indent level.
    // Only the first non-empty content line matters for auto-detection:
    // the YAML parser uses that line to determine the block scalar's
    // indentation level.  Subsequent content lines with leading spaces are
    // fine because the indent level is already fixed by the first line.
    let needs_indent_indicator = s
        .lines()
        .find(|l| !l.is_empty())
        .is_some_and(|l| l.starts_with(' '));

    let base_header = match style {
        ScalarStyle::Literal(Chomp::Clip) => "|",
        ScalarStyle::Literal(Chomp::Strip) => "|-",
        ScalarStyle::Literal(Chomp::Keep) => "|+",
        ScalarStyle::Folded(Chomp::Clip) => ">",
        ScalarStyle::Folded(Chomp::Strip) => ">-",
        ScalarStyle::Folded(Chomp::Keep) => ">+",
        ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => "",
    };

    let header = if needs_indent_indicator && !base_header.is_empty() {
        // Insert the digit between the block indicator character and any chomp
        // indicator: `|` → `|2`, `|-` → `|2-`, `>+` → `>2+`.
        let (block_char, chomp_char) = base_header.split_at(1);
        format!("{block_char}{tab_width}{chomp_char}")
    } else {
        base_header.to_string()
    };

    let mut parts = vec![text(header)];

    if matches!(style, ScalarStyle::Folded(_)) {
        // For folded scalars, blank lines encode the newline structure of the
        // decoded value.  We split on `\n` (not `.lines()`) to count the empty
        // segments that represent extra newlines in the decoded value.
        //
        // The number of blank lines to emit between two consecutive content
        // segments depends on whether either segment is "more-indented" (starts
        // with a space or tab, meaning it was at a greater indentation level in
        // the original YAML):
        //
        //   - Both at base level (no leading whitespace): the line break between
        //     them would be folded to a space on re-parse, so we need one blank
        //     line per `\n` between them.  K empty segments → K+1 `\n`s → K+1
        //     blanks.
        //
        //   - Either side is more-indented: the more-indented line's own line
        //     break is "free" (the parser preserves it without needing a blank).
        //     One blank is therefore "absorbed" by the free line break, so we
        //     emit max(0, K) blanks for K empty segments (= K+1 `\n`s → K blanks).
        //
        // Trailing `\n` from Clip chomp is implicit — strip the trailing empty
        // segment so it is not counted as an extra blank.
        let mut segments: Vec<&str> = s.split('\n').collect();
        if segments.last() == Some(&"") {
            segments.pop();
        }

        let mut pending_empty: usize = 0;
        let mut prev_content: Option<&str> = None;

        for seg in &segments {
            if seg.is_empty() {
                pending_empty += 1;
            } else {
                if let Some(prev) = prev_content {
                    // Determine whether either the previous or the current
                    // content line is "more-indented" (has a leading space
                    // beyond the block scalar's base indentation level).
                    // Only space characters count; a leading tab is content
                    // at the base indent level, not extra indentation.
                    let prev_more = prev.starts_with(' ');
                    let curr_more = seg.starts_with(' ');
                    let either_more = prev_more || curr_more;

                    let blank_count = if either_more {
                        // Free line-break absorbed; only emit extra blanks.
                        pending_empty
                    } else {
                        // Both at base level: each `\n` needs a blank.
                        pending_empty + 1
                    };
                    for _ in 0..blank_count {
                        parts.push(hard_line());
                    }
                }
                pending_empty = 0;
                parts.push(indent(concat(vec![hard_line(), text(seg.to_string())])));
                prev_content = Some(seg);
            }
        }
    } else {
        // For literal scalars, blank lines are omitted here; attach_comments
        // re-inserts them from the original source, preserving blank-line
        // semantics without producing trailing-whitespace lines or double blanks.
        for line_str in s.lines() {
            if !line_str.is_empty() {
                parts.push(indent(concat(vec![
                    hard_line(),
                    text(line_str.to_string()),
                ])));
            }
        }
    }

    concat(parts)
}

/// Convert a YAML mapping to Doc, branching on block vs flow style.
fn mapping_to_doc(
    entries: &[(Node<Span>, Node<Span>)],
    style: CollectionStyle,
    options: &YamlFormatOptions,
) -> Doc {
    if entries.is_empty() {
        return text("{}");
    }

    let effective_style = if options.format_enforce_block_style {
        CollectionStyle::Block
    } else {
        style
    };

    match effective_style {
        CollectionStyle::Flow => flow_mapping_to_doc(entries, options),
        CollectionStyle::Block => {
            let pairs: Vec<Doc> = entries
                .iter()
                .map(|(key, value)| key_value_to_doc(key, value, options))
                .collect();
            join(&hard_line(), pairs)
        }
    }
}

/// Render a flow mapping as `{ key: val, key2: val2 }` or `{key: val}` depending
/// on `bracket_spacing`. Uses `group()` so the printer keeps it on one line when
/// it fits within `print_width`, and breaks it across lines when it does not.
fn flow_mapping_to_doc(entries: &[(Node<Span>, Node<Span>)], options: &YamlFormatOptions) -> Doc {
    let (open, close) = if options.bracket_spacing {
        ("{ ", " }")
    } else {
        ("{", "}")
    };

    let items: Vec<Doc> = entries
        .iter()
        .map(|(key, value)| {
            let key_doc = node_to_doc(key, options, true);
            let val_doc = node_to_doc(value, options, false);
            // Alias keys and tagged empty scalar keys require a space before `:`
            // to prevent ambiguous re-parsing:
            //   - `*a: v` → alias name `a:` (alias consumes the colon)
            //   - `!!str: v` → tag `tag:yaml.org,2002:str:` (`:` is a valid URI char)
            // Use ` : ` (with leading space) for both to produce `*a : v` / `!!str : v`.
            let sep = if key_needs_space_before_colon(key) {
                text(" : ")
            } else {
                text(": ")
            };
            concat(vec![key_doc, sep, val_doc])
        })
        .collect();

    let sep = concat(vec![text(","), line()]);
    let inner = join(&sep, items);

    group(concat(vec![
        text(open),
        indent(concat(vec![flat_alt(text(""), line()), inner])),
        flat_alt(text(""), line()),
        text(close),
    ]))
}

/// Returns `true` when a mapping key requires the explicit `? key` form.
///
/// Explicit key syntax is required when the key cannot appear as a plain scalar
/// before `: ` — specifically when the key is:
/// - a collection (mapping or sequence) of any style
/// - a block scalar (literal `|` or folded `>`), whose multi-line representation
///   cannot fit before a `: ` on the same line
///
/// An empty scalar key is handled separately (emitted as `: value` with no `?`).
const fn needs_explicit_key(key: &Node<Span>) -> bool {
    match key {
        // Any collection as a key (empty or non-empty) requires explicit key form.
        // Flow collections like `[]` or `{}` used as keys need `? [] : value` to
        // avoid ambiguity with flow sequence/mapping indicators.
        // Block scalar keys (literal or folded) span multiple lines and cannot
        // appear before `: ` — they always require explicit key form.
        Node::Mapping { .. }
        | Node::Sequence { .. }
        | Node::Scalar {
            style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
            ..
        } => true,
        Node::Scalar { .. } | Node::Alias { .. } => false,
    }
}

/// Returns `true` when a mapping key is an untagged empty scalar (the implicit empty key `:`).
///
/// A tagged empty scalar (e.g. `!!null :`) is **not** an empty key — the tag carries
/// semantic meaning and must be emitted, so it routes through the normal key path.
const fn is_empty_key(key: &Node<Span>) -> bool {
    matches!(key, Node::Scalar { value, tag: None, .. } if value.is_empty())
}

/// Returns `true` when a mapping key requires a space before the `:` separator.
///
/// Two key forms need ` : ` rather than `: `:
///
/// 1. **Tagged empty scalar** (`!!null`, `!mytag`, etc.) — the rendered key ends
///    with a tag; `:` is a valid URI character, so `!!null:` would be parsed as
///    tag `tag:yaml.org,2002:null:` rather than key `!!null` + separator.
///
/// 2. **Alias** (`*name`) — `*name:` is parsed as alias name `name:`, breaking
///    idempotency. A space before `:` keeps the alias name and separator distinct.
const fn key_needs_space_before_colon(key: &Node<Span>) -> bool {
    matches!(key, Node::Scalar { value, tag: Some(_), .. } if value.is_empty())
        || matches!(key, Node::Alias { .. })
}

/// Render a mapping entry that uses explicit key form: `? key\n: value`.
///
/// This form is required when the key is a block scalar, block sequence, or
/// block mapping — types that cannot appear inline before `: `.
fn explicit_key_to_doc(key: &Node<Span>, value: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    let key_doc = node_to_doc(key, options, true);
    let value_is_empty = matches!(value, Node::Scalar { value, .. } if value.is_empty());

    // `? key_doc` — the key part.
    // For block scalars/collections as keys, the key_doc spans multiple lines.
    // We render `?` + space + key indented by 2 spaces.
    let question_line = concat(vec![text("? "), indent(key_doc)]);

    // `: value_doc` — the value part.
    let colon_line = if value_is_empty {
        // Set-like entry or empty value: emit bare `:` with no trailing space.
        text(":")
    } else {
        let effective_style = |style: CollectionStyle| {
            if options.format_enforce_block_style {
                CollectionStyle::Block
            } else {
                style
            }
        };
        match value {
            // Block mapping value: `: \n  child: val` — indent the mapping.
            Node::Mapping {
                entries,
                style,
                anchor,
                tag,
                ..
            } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
                let colon_prefix = match (anchor.as_ref(), user_tag) {
                    (Some(name), Some(t)) => format!(": &{name} {t}"),
                    (Some(name), None) => format!(": &{name}"),
                    (None, Some(t)) => format!(": {t}"),
                    (None, None) => ":".to_string(),
                };
                concat(vec![
                    text(colon_prefix),
                    indent(concat(vec![
                        hard_line(),
                        mapping_to_doc(entries, *style, options),
                    ])),
                ])
            }
            // Block sequence value: `:\n  - item`.
            Node::Sequence {
                items,
                style,
                anchor,
                tag,
                ..
            } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
                let colon_prefix = match (anchor.as_ref(), user_tag) {
                    (Some(name), Some(t)) => format!(": &{name} {t}"),
                    (Some(name), None) => format!(": &{name}"),
                    (None, Some(t)) => format!(": {t}"),
                    (None, None) => ":".to_string(),
                };
                concat(vec![
                    text(colon_prefix),
                    indent(concat(vec![
                        hard_line(),
                        sequence_to_doc(items, *style, options),
                    ])),
                ])
            }
            // Inline value (scalar, flow collection, empty collection, alias).
            Node::Scalar { .. }
            | Node::Mapping { .. }
            | Node::Sequence { .. }
            | Node::Alias { .. } => {
                let value_doc = node_to_doc(value, options, false);
                concat(vec![text(": "), value_doc])
            }
        }
    };

    // Append trailing comment from the value node.
    let colon_line = if let Some(tc) = value.trailing_comment() {
        concat(vec![colon_line, text(format!("  {tc}"))])
    } else {
        colon_line
    };

    concat(vec![question_line, hard_line(), colon_line])
}

/// Convert a single key-value pair to Doc, including any AST-attached comments.
#[expect(
    clippy::too_many_lines,
    reason = "comprehensive match over all value variants"
)]
fn key_value_to_doc(key: &Node<Span>, value: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    let effective_style = |style: CollectionStyle| {
        if options.format_enforce_block_style {
            CollectionStyle::Block
        } else {
            style
        }
    };

    // Dispatch to explicit key form when the key type requires it.
    // Empty-key entries (`: value`) bypass both explicit-key and normal paths.
    let pair_doc = if needs_explicit_key(key) {
        explicit_key_to_doc(key, value, options)
    } else if is_empty_key(key) {
        // Empty key: emit `: value` (no `?` prefix).
        let value_doc = node_to_doc(value, options, false);
        if matches!(value, Node::Scalar { value, .. } if value.is_empty()) {
            text(":")
        } else {
            concat(vec![text(": "), value_doc])
        }
    } else {
        let key_doc = node_to_doc(key, options, true);
        match value {
            // Block mappings: `key:\n  child: val` — hard_line inside indent.
            // With anchor: `key: &anchor\n  child: val`.
            // With tag: `key: !tag\n  child: val` (anchor before tag per formatter convention).
            Node::Mapping {
                entries,
                style,
                anchor,
                tag,
                ..
            } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
                let bare_colon = if key_needs_space_before_colon(key) {
                    " :"
                } else {
                    ":"
                };
                let colon = match (anchor.as_ref(), user_tag) {
                    (Some(name), Some(t)) => text(format!(": &{name} {t}")),
                    (Some(name), None) => text(format!(": &{name}")),
                    (None, Some(t)) => text(format!(": {t}")),
                    (None, None) => text(bare_colon),
                };
                concat(vec![
                    key_doc,
                    colon,
                    indent(concat(vec![
                        hard_line(),
                        mapping_to_doc(entries, *style, options),
                    ])),
                ])
            }
            // Block sequences: indented block items under key.
            // With anchor: `key: &anchor\n  - item`.
            // With tag: `key: !tag\n  - item` (anchor before tag per formatter convention).
            Node::Sequence {
                items,
                style,
                anchor,
                tag,
                ..
            } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
                let bare_colon = if key_needs_space_before_colon(key) {
                    " :"
                } else {
                    ":"
                };
                let colon = match (anchor.as_ref(), user_tag) {
                    (Some(name), Some(t)) => text(format!(": &{name} {t}")),
                    (Some(name), None) => text(format!(": &{name}")),
                    (None, Some(t)) => text(format!(": {t}")),
                    (None, None) => text(bare_colon),
                };
                concat(vec![
                    key_doc,
                    colon,
                    indent(concat(vec![
                        hard_line(),
                        sequence_to_doc(items, *style, options),
                    ])),
                ])
            }
            // Flow collections, scalars, empty collections, aliases — all inline.
            Node::Scalar { .. }
            | Node::Mapping { .. }
            | Node::Sequence { .. }
            | Node::Alias { .. } => {
                let value_doc = node_to_doc(value, options, false);
                // When the key's rendered form ends with a tag, a space before `:` is
                // required to prevent the colon from being parsed as part of the tag URI.
                let sep = if key_needs_space_before_colon(key) {
                    text(" : ")
                } else {
                    text(": ")
                };
                concat(vec![key_doc, sep, value_doc])
            }
        }
    };

    // Append trailing comment from the value node (only for non-explicit-key paths —
    // explicit_key_to_doc handles its own trailing comment).
    let pair_doc = if !needs_explicit_key(key) && !is_empty_key(key) {
        if let Some(tc) = value.trailing_comment() {
            concat(vec![pair_doc, text(format!("  {tc}"))])
        } else {
            pair_doc
        }
    } else {
        pair_doc
    };

    // Prepend leading comments from the key node.
    let leading = key.leading_comments();
    if leading.is_empty() {
        pair_doc
    } else {
        let mut parts: Vec<Doc> = Vec::new();
        for lc in leading {
            parts.push(text(lc.clone()));
            parts.push(hard_line());
        }
        parts.push(pair_doc);
        concat(parts)
    }
}

/// Convert a YAML sequence to Doc, branching on block vs flow style.
fn sequence_to_doc(seq: &[Node<Span>], style: CollectionStyle, options: &YamlFormatOptions) -> Doc {
    if seq.is_empty() {
        return text("[]");
    }

    let effective_style = if options.format_enforce_block_style {
        CollectionStyle::Block
    } else {
        style
    };

    match effective_style {
        CollectionStyle::Flow => flow_sequence_to_doc(seq, options),
        CollectionStyle::Block => {
            let items: Vec<Doc> = seq
                .iter()
                .map(|item| sequence_item_to_doc(item, options))
                .collect();
            join(&hard_line(), items)
        }
    }
}

/// Render a flow sequence as `[item1, item2, item3]`. Uses `group()` so the
/// printer keeps it on one line when it fits within `print_width`, and breaks it
/// across lines (one item per line, indented) when it does not.
fn flow_sequence_to_doc(seq: &[Node<Span>], options: &YamlFormatOptions) -> Doc {
    let items: Vec<Doc> = seq
        .iter()
        .map(|item| node_to_doc(item, options, false))
        .collect();
    let sep = concat(vec![text(","), line()]);
    let inner = join(&sep, items);

    group(concat(vec![
        text("["),
        indent(concat(vec![flat_alt(text(""), line()), inner])),
        flat_alt(text(""), line()),
        text("]"),
    ]))
}

/// Render a single sequence item with its `- ` prefix, including AST-attached comments.
fn sequence_item_to_doc(item: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    let effective_style = |style: CollectionStyle| {
        if options.format_enforce_block_style {
            CollectionStyle::Block
        } else {
            style
        }
    };

    let item_doc = match item {
        // Block mapping item: `- key: val\n  key2: val2`.
        // With anchor: `- &anchor\n  key: val\n  key2: val2`.
        // With tag: `- !tag\n  key: val` (anchor before tag per formatter convention).
        Node::Mapping {
            entries,
            style,
            anchor,
            tag,
            ..
        } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
            let pairs: Vec<Doc> = entries
                .iter()
                .map(|(k, v)| key_value_to_doc(k, v, options))
                .collect();
            let inner = join(&hard_line(), pairs);
            let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
            let prefix = match (anchor.as_ref(), user_tag) {
                (Some(name), Some(t)) => format!("&{name} {t}"),
                (Some(name), None) => format!("&{name}"),
                (None, Some(t)) => t.clone(),
                (None, None) => String::new(),
            };
            if prefix.is_empty() {
                // `- key: val\n  key2: val2` — first pair on the dash line, remaining
                // pairs indented one level so they align under the first key.
                // indent() shifts all hard_line breaks inside `inner` by one level,
                // placing continuation pairs 2 spaces right of `- `.
                concat(vec![text("- "), indent(inner)])
            } else {
                // `- &anchor\n  key: val` or `- !tag\n  key: val` — prefix on the dash
                // line, content indented.
                concat(vec![
                    text("- "),
                    text(prefix),
                    indent(concat(vec![hard_line(), inner])),
                ])
            }
        }
        // Block sequence item: `- \n  - item`.
        // With anchor: `- &anchor\n  - item`.
        // With tag: `- !tag\n  - item` (anchor before tag per formatter convention).
        Node::Sequence {
            items,
            style,
            anchor,
            tag,
            ..
        } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
            let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
            let prefix_doc = match (anchor.as_ref(), user_tag) {
                (Some(name), Some(t)) => text(format!("&{name} {t}")),
                (Some(name), None) => text(format!("&{name}")),
                (None, Some(t)) => text(t.clone()),
                (None, None) => text(String::new()),
            };
            concat(vec![
                text("- "),
                prefix_doc,
                indent(concat(vec![
                    hard_line(),
                    sequence_to_doc(items, *style, options),
                ])),
            ])
        }
        // Flow collections, scalars, empty collections, aliases — inline under `- `.
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            concat(vec![text("- "), node_to_doc(item, options, false)])
        }
    };

    // Append trailing comment from the item node.
    let item_doc = if let Some(tc) = item.trailing_comment() {
        concat(vec![item_doc, text(format!("  {tc}"))])
    } else {
        item_doc
    };

    // Prepend leading comments from the item node.
    let leading = item.leading_comments();
    if leading.is_empty() {
        item_doc
    } else {
        let mut parts: Vec<Doc> = Vec::new();
        for lc in leading {
            parts.push(text(lc.clone()));
            parts.push(hard_line());
        }
        parts.push(item_doc);
        concat(parts)
    }
}

/// Extract a dedup key string for a mapping key node.
///
/// Returns `Some(key)` for scalar and alias keys, `None` for complex keys
/// (mapping or sequence as key), which are skipped in dedup.
fn dedup_key_str(key: &Node<Span>) -> Option<String> {
    match key {
        Node::Scalar { value, .. } => Some(value.clone()),
        Node::Alias { name, .. } => Some(format!("*{name}")),
        Node::Mapping { .. } | Node::Sequence { .. } => None,
    }
}

/// Remove duplicate mapping keys from the AST, keeping the last occurrence.
///
/// Iterates each `Node::Mapping` in reverse, tracking seen key strings.
/// Earlier duplicate entries are removed; the last occurrence is kept.
/// Recurses into values of remaining entries and into sequence items.
///
/// Key extraction rules:
/// - `Node::Scalar { value, .. }` → key string is `value`
/// - `Node::Alias { name, .. }` → key string is `*name`
/// - Complex keys (`Node::Mapping`, `Node::Sequence`) → skipped (not deduplicated)
fn dedup_mapping_keys(node: &mut Node<Span>) {
    use std::collections::HashSet;
    match node {
        Node::Mapping { entries, .. } => {
            // Determine which keys to keep by scanning in reverse: the last
            // occurrence of each key is encountered first in reverse order,
            // so it gets inserted into `seen`; earlier occurrences are dropped.
            let mut seen: HashSet<String> = HashSet::new();
            // Build a bitmask of which entries survive, working in reverse.
            let keep: Vec<bool> = entries
                .iter()
                .rev()
                .map(|(key, _)| {
                    // First time we see this key (in reverse) → keep it.
                    // Subsequent times → it's a duplicate earlier occurrence → drop.
                    // Complex keys (None) are always kept.
                    dedup_key_str(key).is_none_or(|k| seen.insert(k))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            // Drain all entries and retain only those flagged for keeping.
            let old = std::mem::take(entries);
            *entries = old
                .into_iter()
                .zip(keep)
                .filter_map(|(entry, k)| if k { Some(entry) } else { None })
                .collect();

            // Recurse into remaining values.
            for (_, value) in entries.iter_mut() {
                dedup_mapping_keys(value);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items.iter_mut() {
                dedup_mapping_keys(item);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
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

    // ---- Group: escape_double_quoted unit tests ----

    // EDQ1: Newline, carriage return, and tab are escaped.
    // EDQ2: Double-quote and backslash are escaped.
    #[rstest]
    #[case::newline_escaped("a\nb", "a\\nb")]
    #[case::carriage_return_escaped("a\rb", "a\\rb")]
    #[case::tab_escaped("a\tb", "a\\tb")]
    #[case::double_quote_escaped("say \"hi\"", "say \\\"hi\\\"")]
    #[case::backslash_escaped("a\\b", "a\\\\b")]
    fn escape_double_quoted_escapes(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(escape_double_quoted(input), expected);
    }

    // ---- Group: needs_quoting — returns true ----

    // NQ1 (empty string), NQ2 (numeric), and version-aware cases.
    #[rstest]
    #[case::on_v1_1("on", YamlVersion::V1_1)]
    #[case::yes_v1_1("yes", YamlVersion::V1_1)]
    #[case::off_v1_1("off", YamlVersion::V1_1)]
    #[case::no_v1_1("no", YamlVersion::V1_1)]
    #[case::true_v1_1("true", YamlVersion::V1_1)]
    #[case::true_v1_2("true", YamlVersion::V1_2)]
    #[case::null_v1_1("null", YamlVersion::V1_1)]
    #[case::null_v1_2("null", YamlVersion::V1_2)]
    #[case::uppercase_yes_v1_1("YES", YamlVersion::V1_1)]
    #[case::empty_string_v1_1("", YamlVersion::V1_1)]
    #[case::empty_string_v1_2("", YamlVersion::V1_2)]
    #[case::numeric_123_v1_1("123", YamlVersion::V1_1)]
    #[case::numeric_123_v1_2("123", YamlVersion::V1_2)]
    #[case::numeric_3_14_v1_2("3.14", YamlVersion::V1_2)]
    fn needs_quoting_returns_true(#[case] word: &str, #[case] version: YamlVersion) {
        assert!(
            needs_quoting(word, version),
            "{word:?} should require quoting in {version:?}"
        );
    }

    // ---- Group: needs_quoting — returns false ----

    #[rstest]
    #[case::on_v1_2("on", YamlVersion::V1_2)]
    #[case::yes_v1_2("yes", YamlVersion::V1_2)]
    #[case::off_v1_2("off", YamlVersion::V1_2)]
    #[case::no_v1_2("no", YamlVersion::V1_2)]
    #[case::uppercase_yes_v1_2("YES", YamlVersion::V1_2)]
    fn needs_quoting_returns_false(#[case] word: &str, #[case] version: YamlVersion) {
        assert!(
            !needs_quoting(word, version),
            "{word:?} should not require quoting in {version:?}"
        );
    }

    // ---- Group A: YamlFormatOptions field ----

    // A1: format_enforce_block_style defaults to false.
    #[test]
    fn format_enforce_block_style_defaults_to_false() {
        assert!(!YamlFormatOptions::default().format_enforce_block_style);
    }

    // ---- Group A (dedup): YamlFormatOptions field ----

    // A1: format_remove_duplicate_keys defaults to false.
    #[test]
    fn format_remove_duplicate_keys_defaults_to_false() {
        assert!(!YamlFormatOptions::default().format_remove_duplicate_keys);
    }

    // ---- Group B (dedup): Setting disabled — no-op ----

    // B1: duplicate keys are NOT removed when setting is false (default).
    #[test]
    fn dedup_disabled_does_not_remove_duplicate_keys() {
        let input = "key: 1\nkey: 2\n";
        let result = format_yaml(input, &default_opts());
        let count = result.matches("key:").count();
        assert!(
            count >= 2,
            "both keys should remain when dedup disabled: {result:?}"
        );
    }

    // ---- Group C (dedup): Basic dedup behavior ----

    fn dedup_opts() -> YamlFormatOptions {
        YamlFormatOptions {
            format_remove_duplicate_keys: true,
            ..default_opts()
        }
    }

    // C1: single duplicate key — last occurrence kept.
    #[test]
    fn dedup_single_duplicate_keeps_last() {
        let result = format_yaml("key: 1\nkey: 2\n", &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first occurrence should be removed: {result:?}"
        );
    }

    // C2: three occurrences of same key — only last kept.
    #[test]
    fn dedup_three_occurrences_keeps_only_last() {
        let result = format_yaml("key: a\nkey: b\nkey: c\n", &dedup_opts());
        assert!(
            result.contains("key: c"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: a"),
            "first occurrence should be removed: {result:?}"
        );
        assert!(
            !result.contains("key: b"),
            "middle occurrence should be removed: {result:?}"
        );
    }

    // C3: two unique keys — nothing removed.
    #[test]
    fn dedup_unique_keys_unchanged() {
        let result = format_yaml("a: 1\nb: 2\n", &dedup_opts());
        assert!(result.contains("a: 1"), "a:1 missing: {result:?}");
        assert!(result.contains("b: 2"), "b:2 missing: {result:?}");
    }

    // C4: mixed unique and duplicate — only duplicates removed.
    #[test]
    fn dedup_mixed_unique_and_duplicate() {
        let result = format_yaml("a: 1\nb: 2\na: 3\n", &dedup_opts());
        assert!(
            result.contains("a: 3"),
            "last a: should be present: {result:?}"
        );
        assert!(
            result.contains("b: 2"),
            "unique b: should be present: {result:?}"
        );
        assert!(
            !result.contains("a: 1"),
            "first a: should be removed: {result:?}"
        );
    }

    // ---- Group D (dedup): Edge cases — empty and single-entry mappings ----

    // D1: empty mapping — no change.
    #[test]
    fn dedup_empty_mapping_unchanged() {
        let result = format_yaml("map: {}\n", &dedup_opts());
        assert!(
            result.contains("{}"),
            "empty mapping should be preserved: {result:?}"
        );
    }

    // D2: single-entry mapping — no change.
    #[test]
    fn dedup_single_entry_mapping_unchanged() {
        let result = format_yaml("key: value\n", &dedup_opts());
        assert!(
            result.contains("key: value"),
            "single entry should be preserved: {result:?}"
        );
    }

    // ---- Group E (dedup): Key types ----

    // E1: alias key — duplicate alias keys, last kept.
    #[test]
    fn dedup_alias_key_duplicate_keeps_last() {
        // `? *ref` is explicit-key syntax that produces Node::Alias in key position.
        let input = "? *ref\n: value1\n? *ref\n: value2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("value2"),
            "last alias-keyed value missing: {result:?}"
        );
        assert!(
            !result.contains("value1"),
            "first alias-keyed value should be removed: {result:?}"
        );
    }

    // E2: complex key (mapping as key) — dedup skipped, no panic.
    #[test]
    fn dedup_complex_mapping_key_no_panic() {
        // Explicit complex key: `? {a: 1}: value`
        // Parser may or may not support this; if parsing fails, format_yaml returns input unchanged.
        let input = "? {a: 1}\n: value\n";
        let result = format_yaml(input, &dedup_opts());
        // Must not panic. Output may be unchanged (if unparseable) or formatted.
        let _ = result;
    }

    // E3: complex key (sequence as key) — dedup skipped, no panic.
    #[test]
    fn dedup_complex_sequence_key_no_panic() {
        let input = "? [1, 2]\n: value\n";
        let result = format_yaml(input, &dedup_opts());
        let _ = result;
    }

    // E4: case-sensitive key comparison — `Key` and `key` are distinct.
    #[test]
    fn dedup_case_sensitive_keys_both_kept() {
        let result = format_yaml("Key: 1\nkey: 2\n", &dedup_opts());
        assert!(result.contains("Key: 1"), "Key:1 missing: {result:?}");
        assert!(result.contains("key: 2"), "key:2 missing: {result:?}");
    }

    // ---- Group F (dedup): Recursion ----

    // F1: nested mapping — duplicate keys in nested mapping removed.
    #[test]
    fn dedup_nested_mapping_removes_inner_duplicates() {
        let input = "outer:\n  inner: 1\n  inner: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(result.contains("outer:"), "outer key missing: {result:?}");
        assert!(
            result.contains("inner: 2"),
            "last inner should be kept: {result:?}"
        );
        assert!(
            !result.contains("inner: 1"),
            "first inner should be removed: {result:?}"
        );
    }

    // F2: mapping inside sequence — dedup recurses through sequence items.
    #[test]
    fn dedup_recurses_into_sequence_items() {
        let input = "items:\n  - key: 1\n    key: 2\n  - key: 3\n    key: 4\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last key in first item missing: {result:?}"
        );
        assert!(
            result.contains("key: 4"),
            "last key in second item missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first key in first item should be removed: {result:?}"
        );
        assert!(
            !result.contains("key: 3"),
            "first key in second item should be removed: {result:?}"
        );
    }

    // F3: deeply nested — dedup recurses more than two levels.
    #[test]
    fn dedup_deeply_nested_removes_innermost_duplicates() {
        let input = "a:\n  b:\n    c: 1\n    c: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(result.contains("a:"), "a: missing: {result:?}");
        assert!(result.contains("b:"), "b: missing: {result:?}");
        assert!(
            result.contains("c: 2"),
            "last c: should be kept: {result:?}"
        );
        assert!(
            !result.contains("c: 1"),
            "first c: should be removed: {result:?}"
        );
    }

    // ---- Group G (dedup): Flow mappings ----

    // G1: flow mapping — dedup works for flow style.
    #[test]
    fn dedup_flow_mapping_removes_duplicate() {
        let result = format_yaml("{key: 1, key: 2}\n", &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first occurrence should be removed: {result:?}"
        );
    }

    // ---- Group H (dedup): Comments on removed entries ----

    // H1: comment on removed entry — no crash; surviving output is valid.
    #[test]
    fn dedup_removed_entry_with_trailing_comment_no_crash() {
        // The first `key` has an inline comment; it should be removed without panic.
        let input = "key: 1  # this gets removed\nkey: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first occurrence should be removed: {result:?}"
        );
    }

    // H2: leading comment on surviving (last) entry — comment preserved.
    #[test]
    fn dedup_surviving_entry_leading_comment_preserved() {
        // The last `key` has a leading comment; it should survive dedup.
        let input = "key: 1\n# keep this\nkey: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            result.contains("# keep this"),
            "leading comment should be preserved: {result:?}"
        );
    }

    // ---- Group I (dedup): Multiple documents ----

    // I1: duplicate keys in each document — dedup is per-document.
    #[test]
    fn dedup_multi_document_per_document() {
        let input = "key: 1\nkey: 2\n---\nkey: 3\nkey: 4\n";
        let result = format_yaml(input, &dedup_opts());
        // Each document should have had its first `key` removed.
        // The result should contain `key: 2` and `key: 4` (the last in each doc).
        assert!(
            result.contains("key: 2"),
            "last key in doc1 missing: {result:?}"
        );
        assert!(
            result.contains("key: 4"),
            "last key in doc2 missing: {result:?}"
        );
        assert!(
            result.contains("---"),
            "document separator missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first key in doc1 should be removed: {result:?}"
        );
        assert!(
            !result.contains("key: 3"),
            "first key in doc2 should be removed: {result:?}"
        );
    }

    // ---- Group J (dedup): Idempotency ----

    // J1: format_remove_duplicate_keys: true is idempotent.
    #[test]
    fn dedup_idempotent() {
        let input = "key: 1\nkey: 2\n";
        let first = format_yaml(input, &dedup_opts());
        let second = format_yaml(&first, &dedup_opts());
        assert_eq!(first, second, "dedup not idempotent: {first:?}");
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
}
