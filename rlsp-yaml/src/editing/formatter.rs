// SPDX-License-Identifier: MIT

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
                if entry.blank_lines_before > 0 || carried_blanks > 0 {
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
    let mut iter = documents.iter().map(|doc| node_to_doc(&doc.root, options));
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
fn node_to_doc(node: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    match node {
        Node::Scalar {
            value, style, tag, ..
        } => {
            // Prefix with a user-defined tag if present (e.g. `!mytag`).
            // Core Schema tags (tag:yaml.org,2002:*) are not preserved — only user tags.
            let tag_prefix = tag.as_ref().and_then(|t| {
                if is_core_schema_tag(t) {
                    None
                } else {
                    Some(format!("{t} "))
                }
            });

            let scalar_doc = match style {
                ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => {
                    repr_block_to_doc(value, *style)
                }
                ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                    if needs_quoting(value, options.yaml_version) {
                        if matches!(style, ScalarStyle::DoubleQuoted) {
                            text(format!("\"{}\"", escape_double_quoted(value)))
                        } else {
                            text(format!("'{value}'"))
                        }
                    } else {
                        string_to_doc(value, options)
                    }
                }
                ScalarStyle::Plain => {
                    if needs_quoting(value, options.yaml_version) {
                        text(value.clone())
                    } else {
                        string_to_doc(value, options)
                    }
                }
            };

            if let Some(prefix) = tag_prefix {
                concat(vec![text(prefix), scalar_doc])
            } else {
                scalar_doc
            }
        }

        Node::Mapping { entries, style, .. } => mapping_to_doc(entries, *style, options),

        Node::Sequence { items, style, .. } => sequence_to_doc(items, *style, options),

        Node::Alias { name, .. } => text(format!("*{name}")),
    }
}

/// Returns `true` if the tag string is a YAML Core Schema tag.
fn is_core_schema_tag(tag: &str) -> bool {
    tag.starts_with("tag:yaml.org,2002:")
}

/// Convert a string scalar to a Doc, quoting as necessary.
fn string_to_doc(s: &str, options: &YamlFormatOptions) -> Doc {
    if needs_quoting(s, options.yaml_version) {
        // Must quote — use the preferred style.
        if options.single_quote && !s.contains('\'') {
            text(format!("'{s}'"))
        } else {
            // Double-quote and escape.
            text(format!("\"{}\"", escape_double_quoted(s)))
        }
    } else if options.single_quote {
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

/// Escape a string for use in a double-quoted YAML scalar.
fn escape_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
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
/// Blank lines (empty strings from `str::lines()`) are omitted from the Doc
/// entirely — `attach_comments` re-inserts them from the original input when
/// it matches content signatures. This avoids double-blanks that would result
/// from both the Doc IR and `attach_comments` each contributing a blank line.
fn repr_block_to_doc(s: &str, style: ScalarStyle) -> Doc {
    let header = match style {
        ScalarStyle::Literal(Chomp::Clip) => "|",
        ScalarStyle::Literal(Chomp::Strip) => "|-",
        ScalarStyle::Literal(Chomp::Keep) => "|+",
        ScalarStyle::Folded(Chomp::Clip) => ">",
        ScalarStyle::Folded(Chomp::Strip) => ">-",
        ScalarStyle::Folded(Chomp::Keep) => ">+",
        ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => "",
    };
    let mut parts = vec![text(header)];
    for line_str in s.lines() {
        if !line_str.is_empty() {
            // Non-empty line: indent one level relative to the parent key.
            parts.push(indent(concat(vec![
                hard_line(),
                text(line_str.to_string()),
            ])));
        }
        // Blank lines are skipped here; attach_comments re-inserts them from
        // the original source, preserving blank-line semantics without
        // producing trailing-whitespace lines or double blanks.
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
            let key_doc = node_to_doc(key, options);
            let val_doc = node_to_doc(value, options);
            concat(vec![key_doc, text(": "), val_doc])
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

/// Convert a single key-value pair to Doc, including any AST-attached comments.
fn key_value_to_doc(key: &Node<Span>, value: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    let key_doc = node_to_doc(key, options);

    let effective_style = |style: CollectionStyle| {
        if options.format_enforce_block_style {
            CollectionStyle::Block
        } else {
            style
        }
    };

    let pair_doc = match value {
        // Block mappings: `key:\n  child: val` — hard_line inside indent.
        Node::Mapping { entries, style, .. }
            if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block =>
        {
            concat(vec![
                key_doc,
                text(":"),
                indent(concat(vec![
                    hard_line(),
                    mapping_to_doc(entries, *style, options),
                ])),
            ])
        }
        // Block sequences: indented block items under key.
        Node::Sequence { items, style, .. }
            if !items.is_empty() && effective_style(*style) == CollectionStyle::Block =>
        {
            concat(vec![
                key_doc,
                text(":"),
                indent(concat(vec![
                    hard_line(),
                    sequence_to_doc(items, *style, options),
                ])),
            ])
        }
        // Flow collections, scalars, empty collections, aliases — all inline.
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            let value_doc = node_to_doc(value, options);
            concat(vec![key_doc, text(": "), value_doc])
        }
    };

    // Append trailing comment from the value node.
    let pair_doc = if let Some(tc) = value.trailing_comment() {
        concat(vec![pair_doc, text(format!("  {tc}"))])
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
    let items: Vec<Doc> = seq.iter().map(|item| node_to_doc(item, options)).collect();
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
        Node::Mapping { entries, style, .. }
            if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block =>
        {
            // `- key: val\n  key2: val2` — first pair on the dash line, remaining
            // pairs indented one level so they align under the first key.
            let pairs: Vec<Doc> = entries
                .iter()
                .map(|(k, v)| key_value_to_doc(k, v, options))
                .collect();
            let inner = join(&hard_line(), pairs);
            // indent() shifts all hard_line breaks inside `inner` by one level,
            // placing continuation pairs 2 spaces right of `- `.
            concat(vec![text("- "), indent(inner)])
        }
        Node::Sequence { items, style, .. }
            if !items.is_empty() && effective_style(*style) == CollectionStyle::Block =>
        {
            concat(vec![
                text("- "),
                indent(concat(vec![
                    hard_line(),
                    sequence_to_doc(items, *style, options),
                ])),
            ])
        }
        // Flow collections, scalars, empty collections, aliases — inline under `- `.
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            concat(vec![text("- "), node_to_doc(item, options)])
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
}
