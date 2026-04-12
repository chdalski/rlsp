// SPDX-License-Identifier: MIT

use rlsp_fmt::{Doc, FormatOptions, concat, format as fmt_format, hard_line, indent, join, text};
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

        Node::Mapping { entries, .. } => mapping_to_doc(entries, options),

        Node::Sequence { items, .. } => sequence_to_doc(items, options),

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
        parts.push(hard_line());
        parts.push(text(line_str.to_string()));
    }
    concat(parts)
}

/// Convert a YAML mapping to Doc in block style.
fn mapping_to_doc(entries: &[(Node<Span>, Node<Span>)], options: &YamlFormatOptions) -> Doc {
    if entries.is_empty() {
        return text("{}");
    }

    let pairs: Vec<Doc> = entries
        .iter()
        .map(|(key, value)| key_value_to_doc(key, value, options))
        .collect();

    let sep = hard_line();
    join(&sep, pairs)
}

/// Convert a single key-value pair to Doc, including any AST-attached comments.
fn key_value_to_doc(key: &Node<Span>, value: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    let key_doc = node_to_doc(key, options);

    let pair_doc = match value {
        // Block mappings: `key:\n  child: val` — hard_line inside indent.
        Node::Mapping { entries, .. } if !entries.is_empty() => concat(vec![
            key_doc,
            text(":"),
            indent(concat(vec![hard_line(), mapping_to_doc(entries, options)])),
        ]),
        // Non-empty sequences: always block, indented under key.
        Node::Sequence { items, .. } if !items.is_empty() => concat(vec![
            key_doc,
            text(":"),
            indent(concat(vec![hard_line(), sequence_to_doc(items, options)])),
        ]),
        // All other values (scalars, empty collections, aliases) inline.
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

/// Convert a YAML sequence to Doc (always block style).
fn sequence_to_doc(seq: &[Node<Span>], options: &YamlFormatOptions) -> Doc {
    if seq.is_empty() {
        return text("[]");
    }
    let items: Vec<Doc> = seq
        .iter()
        .map(|item| sequence_item_to_doc(item, options))
        .collect();
    let sep = hard_line();
    join(&sep, items)
}

/// Render a single sequence item with its `- ` prefix, including AST-attached comments.
fn sequence_item_to_doc(item: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    let item_doc = match item {
        Node::Mapping { entries, .. } if !entries.is_empty() => {
            // `- key: val\n  key2: val2` — first pair on the dash line, remaining
            // pairs indented one level so they align under the first key.
            let pairs: Vec<Doc> = entries
                .iter()
                .map(|(k, v)| key_value_to_doc(k, v, options))
                .collect();
            let sep = hard_line();
            let inner = join(&sep, pairs);
            // indent() shifts all hard_line breaks inside `inner` by one level,
            // placing continuation pairs 2 spaces right of `- `.
            concat(vec![text("- "), indent(inner)])
        }
        Node::Sequence { items, .. } if !items.is_empty() => concat(vec![
            text("- "),
            indent(concat(vec![hard_line(), sequence_to_doc(items, options)])),
        ]),
        // Scalars, empty collections, aliases.
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

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code"
)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn default_opts() -> YamlFormatOptions {
        YamlFormatOptions::default()
    }

    fn opts_with_version(v: YamlVersion) -> YamlFormatOptions {
        YamlFormatOptions {
            yaml_version: v,
            ..default_opts()
        }
    }

    // ---- Group: Exact-output tests ----

    #[rstest]
    #[case::simple_key_value("key: value\n", "key: value\n")]
    #[case::multiple_keys("a: 1\nb: 2\nc: 3\n", "a: 1\nb: 2\nc: 3\n")]
    #[case::empty_document("", "")]
    #[case::syntax_error_returns_original("key: [unclosed\n", "key: [unclosed\n")]
    #[case::no_comments_regression("a: 1\nb: 2\nc: 3\n", "a: 1\nb: 2\nc: 3\n")]
    #[case::blank_line_at_eof_stripped("a: 1\n\n", "a: 1\n")]
    #[case::no_blank_lines_not_added("a: 1\nb: 2\n", "a: 1\nb: 2\n")]
    #[case::invalid_input_unchanged("key: [bad\n", "key: [bad\n")]
    fn format_yaml_exact_output(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(format_yaml(input, &default_opts()), expected);
    }

    // ---- Group: Idempotency tests ----

    #[rstest]
    #[case::key_value("key: value\n")]
    #[case::multi_key("a: 1\nb: 2\n")]
    #[case::nested_mapping("parent:\n  child: value\n")]
    #[case::block_sequence("items:\n  - one\n  - two\n")]
    #[case::trailing_comment("key: value  # comment\n")]
    #[case::leading_comment("# header\nkey: value\n")]
    #[case::sections_with_blank("# section 1\nkey1: v1\n\n# section 2\nkey2: v2\n")]
    #[case::nested_sequence("outer:\n  - - inner1\n    - inner2\n  - simple\n")]
    #[case::blank_line_between_keys(
        "on: push\n\npermissions:\n  contents: read\n\njobs:\n  build: {}\n"
    )]
    #[case::quote_stripping("value: \"python\"\n")]
    #[case::flow_to_block(
        "spec:\n  containers:\n    - name: test\n      command: [\"python\", \"-m\", \"http.server\", \"5000\"]\n"
    )]
    fn format_yaml_is_idempotent(#[case] input: &str) {
        let first = format_yaml(input, &default_opts());
        let second = format_yaml(&first, &default_opts());
        assert_eq!(
            first, second,
            "idempotency failed for {input:?}:\nfirst:  {first:?}\nsecond: {second:?}"
        );
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

    // ---- Group: Single-contains checks ----

    #[rstest]
    #[case::null_value("key: null\n", "null")]
    #[case::whole_number_float("x: 42.0\n", "42.0")]
    #[case::integer_preserved("port: 8080\n", "8080")]
    fn format_yaml_single_contains(#[case] input: &str, #[case] expected: &str) {
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains(expected),
            "{expected:?} missing: {result:?}"
        );
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

    // ---- Group: Quote stripping — safe strings → plain ----

    // QS1 (double-quoted safe → plain), QS2 (single-quoted safe → plain),
    // and duplicates from "scalar style preserved" section.
    #[rstest]
    #[case::double_quoted_safe("value: \"python\"\n", "\"python\"", "python")]
    #[case::single_quoted_safe("value: 'hello'\n", "'hello'", "hello")]
    #[case::double_quoted_greeting("greeting: \"hello\"\n", "\"hello\"", "hello")]
    #[case::single_quoted_greeting("greeting: 'hello'\n", "'hello'", "hello")]
    fn format_yaml_quotes_stripped(#[case] input: &str, #[case] quoted: &str, #[case] plain: &str) {
        let result = format_yaml(input, &default_opts());
        assert!(
            !result.contains(quoted),
            "unnecessary {quoted:?} should be stripped: {result:?}"
        );
        assert!(
            result.contains(plain),
            "{plain:?} should be present as plain: {result:?}"
        );
    }

    // ---- Group: Quote stripping — special strings → kept quoted ----

    // QS3 (number-like), QS4 (boolean keyword), QS7 (starts with #).
    #[rstest]
    #[case::number_like("value: \"5000\"\n", "\"5000\"")]
    #[case::boolean_keyword("value: \"true\"\n", "\"true\"")]
    #[case::hash_start("value: \"#comment\"\n", "\"#comment\"")]
    fn format_yaml_quotes_preserved(#[case] input: &str, #[case] expected: &str) {
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains(expected),
            "{expected:?} must be preserved: {result:?}"
        );
    }

    // ---- Group: Plain scalars — value present, not re-quoted ----

    // Covers scalar style preservation for true/false/null/on (default opts).
    #[rstest]
    #[case::true_preserved("enabled: true\n", "true", "\"true\"")]
    #[case::false_preserved("active: false\n", "false", "\"false\"")]
    #[case::null_preserved("value: null\n", "null", "\"null\"")]
    #[case::on_key_unquoted("on: push\n", "on:", "\"on\"")]
    fn format_yaml_plain_scalar_not_quoted(
        #[case] input: &str,
        #[case] contains: &str,
        #[case] not_quoted: &str,
    ) {
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains(contains),
            "{contains:?} missing: {result:?}"
        );
        let not_sq = not_quoted.replace('"', "'");
        assert!(
            !result.contains(not_quoted) && !result.contains(&not_sq),
            "{contains:?} must not be quoted: {result:?}"
        );
    }

    // ---- Group: Version-aware quoting — keyword stays quoted ----

    // QS5 (on in V1.1) absorbed here alongside v1_1 on/yes and both-version true.
    #[rstest]
    #[case::on_stays_quoted_v1_1("value: \"on\"\n", YamlVersion::V1_1, "\"on\"")]
    #[case::yes_stays_quoted_v1_1("value: \"yes\"\n", YamlVersion::V1_1, "\"yes\"")]
    #[case::true_stays_quoted_v1_1("value: \"true\"\n", YamlVersion::V1_1, "\"true\"")]
    #[case::true_stays_quoted_v1_2("value: \"true\"\n", YamlVersion::V1_2, "\"true\"")]
    fn format_yaml_quoted_keyword_stays_quoted(
        #[case] input: &str,
        #[case] version: YamlVersion,
        #[case] expected: &str,
    ) {
        let result = format_yaml(input, &opts_with_version(version));
        assert!(
            result.contains(expected),
            "{expected:?} must stay quoted in {version:?}: {result:?}"
        );
    }

    // ---- Group: Version-aware quoting — V1.2 strips non-reserved keywords ----

    #[rstest]
    #[case::on_stripped_v1_2("value: \"on\"\n", YamlVersion::V1_2, "\"on\"", "'on'", "on")]
    #[case::yes_stripped_v1_2("value: \"yes\"\n", YamlVersion::V1_2, "\"yes\"", "'yes'", "yes")]
    fn format_yaml_v1_2_keyword_quotes_stripped(
        #[case] input: &str,
        #[case] version: YamlVersion,
        #[case] not_dq: &str,
        #[case] not_sq: &str,
        #[case] plain: &str,
    ) {
        let result = format_yaml(input, &opts_with_version(version));
        assert!(
            !result.contains(not_dq) && !result.contains(not_sq),
            "{plain:?} is not reserved in V1.2; quotes should be stripped: {result:?}"
        );
        assert!(
            result.contains(plain),
            "{plain:?} must appear as plain: {result:?}"
        );
    }

    // ---- Group: on plain key — never quoted regardless of version ----

    #[rstest]
    #[case::v1_2(YamlVersion::V1_2)]
    #[case::v1_1(YamlVersion::V1_1)]
    fn format_yaml_on_plain_key_never_quoted(#[case] version: YamlVersion) {
        let result = format_yaml("on: push\n", &opts_with_version(version));
        assert!(
            result.contains("on:"),
            "on: key should not be quoted in {version:?}: {result:?}"
        );
        assert!(
            !result.contains("\"on\"") && !result.contains("'on'"),
            "on: plain key must not gain quotes in {version:?}: {result:?}"
        );
    }

    // ---- Group: Scalar style — single-contains checks (Task 23 Phase A) ----

    #[rstest]
    #[case::plain_scalar("key: plain_value\n", "key: plain_value")]
    #[case::literal_block_chomp_clip("key: |\n  line one\n  line two\n", "|")]
    #[case::folded_block_chomp_strip("key: >-\n  content\n", ">-")]
    fn format_yaml_scalar_style_contains(#[case] input: &str, #[case] expected: &str) {
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains(expected),
            "{expected:?} missing: {result:?}"
        );
    }

    // ---- Standalone: Tests with unique assertion shapes or custom options ----

    // Test 3: Nested mapping — child indented under parent.
    #[test]
    fn nested_mapping() {
        let input = "parent:\n  child: value\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("parent:"), "missing parent key");
        assert!(
            result.contains("  child: value") || result.contains("\n  child:"),
            "child should be indented: {result:?}"
        );
    }

    // Test 4: Deeply nested — 3+ levels.
    #[test]
    fn deeply_nested() {
        let input = "a:\n  b:\n    c: deep\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("a:"), "missing a");
        assert!(result.contains("b:"), "missing b");
        assert!(
            result.contains("c: deep") || result.contains("c:"),
            "missing c"
        );
    }

    // Test 5: Block sequence — `- item` format.
    #[test]
    fn block_sequence() {
        let input = "items:\n  - one\n  - two\n  - three\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("items:"), "missing items key");
        assert!(result.contains("- one"), "missing - one");
        assert!(result.contains("- two"), "missing - two");
    }

    // Test 6: Sequence of mappings — common K8s pattern.
    // Verifies that continuation keys in a sequence item mapping are indented
    // under the first key, not at the `- ` column level.
    #[test]
    fn sequence_of_mappings() {
        let input = "users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("users:"), "missing users: {result:?}");
        assert!(
            result.contains("- name: Alice"),
            "first item first key missing: {result:?}"
        );
        assert!(
            result.contains("  age: 30"),
            "age should be indented under its sequence item: {result:?}"
        );
        assert!(
            result.contains("- name: Bob"),
            "second item first key missing: {result:?}"
        );
        assert!(
            result.contains("  age: 25"),
            "second item age should be indented: {result:?}"
        );
    }

    // Extra: string values that need quoting get quoted.
    #[test]
    fn string_quoting_ambiguous_values() {
        // "true" as a string value — after parse it becomes Boolean(true),
        // so it resolves to the integer type. A string that looks like a number needs quoting.
        let opts = YamlFormatOptions {
            single_quote: false,
            ..Default::default()
        };
        // A key whose value is the string "null" is a reserved YAML keyword.
        // Test that strings requiring quotes actually get them via round-trip.
        let input = "key: some value\n";
        let result = format_yaml(input, &opts);
        // "some value" is a valid plain string — no quotes needed.
        assert!(result.contains("some value"), "result: {result:?}");
    }

    // Extra: single_quote option wraps strings.
    #[test]
    fn single_quote_option() {
        let opts = YamlFormatOptions {
            single_quote: true,
            ..Default::default()
        };
        let input = "key: hello\n";
        let result = format_yaml(input, &opts);
        assert!(
            result.contains("'hello'"),
            "expected single-quoted: {result:?}"
        );
    }

    // Test C1: Trailing comment preserved.
    #[test]
    fn trailing_comment_preserved() {
        let input = "key: value  # comment\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("key: value"), "content missing: {result:?}");
        assert!(
            result.contains("# comment"),
            "trailing comment missing: {result:?}"
        );
        // Comment must appear on the same line as the content.
        for line in result.lines() {
            if line.contains("key: value") {
                assert!(
                    line.contains("# comment"),
                    "trailing comment not on same line: {line:?}"
                );
            }
        }
    }

    // Test C2: Leading comment preserved.
    #[test]
    fn leading_comment_preserved() {
        let input = "# header\nkey: value\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("# header"),
            "leading comment missing: {result:?}"
        );
        assert!(result.contains("key: value"), "content missing: {result:?}");
        // Comment must appear before the key line.
        let comment_pos = result.find("# header").unwrap();
        let key_pos = result.find("key: value").unwrap();
        assert!(
            comment_pos < key_pos,
            "leading comment should appear before key: {result:?}"
        );
    }

    // Test C3: Multiple consecutive leading comments stay together.
    #[test]
    fn multiple_leading_comments() {
        let input = "# line one\n# line two\nkey: value\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("# line one"),
            "first comment missing: {result:?}"
        );
        assert!(
            result.contains("# line two"),
            "second comment missing: {result:?}"
        );
        assert!(result.contains("key: value"), "content missing: {result:?}");
        // Both comments must precede the key line.
        let c1_pos = result.find("# line one").unwrap();
        let c2_pos = result.find("# line two").unwrap();
        let key_pos = result.find("key: value").unwrap();
        assert!(c1_pos < key_pos, "first comment should precede key");
        assert!(c2_pos < key_pos, "second comment should precede key");
        assert!(c1_pos < c2_pos, "comments should be in original order");
    }

    // Test C4: Blank line between sections preserved.
    #[test]
    fn blank_line_between_sections() {
        let input = "# section 1\nkey1: v1\n\n# section 2\nkey2: v2\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("# section 1"),
            "section 1 comment missing: {result:?}"
        );
        assert!(
            result.contains("# section 2"),
            "section 2 comment missing: {result:?}"
        );
        assert!(result.contains("key1: v1"), "key1 missing: {result:?}");
        assert!(result.contains("key2: v2"), "key2 missing: {result:?}");
        let s1_pos = result.find("# section 1").unwrap();
        let k1_pos = result.find("key1: v1").unwrap();
        let s2_pos = result.find("# section 2").unwrap();
        let k2_pos = result.find("key2: v2").unwrap();
        assert!(s1_pos < k1_pos, "section 1 comment should precede key1");
        assert!(s2_pos < k2_pos, "section 2 comment should precede key2");
    }

    // Test C5: Comment at document start.
    #[test]
    fn comment_at_document_start() {
        let input = "# top comment\nkey: value\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.starts_with("# top comment"),
            "top comment should be first: {result:?}"
        );
        assert!(result.contains("key: value"), "content missing: {result:?}");
    }

    // Test C6: Comment at document end.
    #[test]
    fn comment_at_document_end() {
        let input = "key: value\n# bottom comment\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("key: value"), "content missing: {result:?}");
        assert!(
            result.contains("# bottom comment"),
            "bottom comment missing: {result:?}"
        );
    }

    // Test C7: Comments between sequence items.
    #[test]
    fn comments_between_sequence_items() {
        let input = "items:\n  - item1\n  # between\n  - item2\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("- item1"), "item1 missing: {result:?}");
        assert!(result.contains("- item2"), "item2 missing: {result:?}");
        assert!(
            result.contains("# between"),
            "between comment missing: {result:?}"
        );
        let i1_pos = result.find("- item1").unwrap();
        let bet_pos = result.find("# between").unwrap();
        let i2_pos = result.find("- item2").unwrap();
        assert!(i1_pos < bet_pos, "comment should be after item1");
        assert!(bet_pos < i2_pos, "comment should be before item2");
    }

    // Test C10: Hash inside quoted string is NOT extracted as a comment.
    #[test]
    fn hash_inside_quoted_string_not_extracted() {
        let input = "key: \"value # not a comment\"\n";
        let result = format_yaml(input, &default_opts());
        for line in result.lines() {
            if line.contains("key:") {
                assert!(
                    !line.trim_end().ends_with("# not a comment"),
                    "hash inside quoted string wrongly extracted as comment: {line:?}"
                );
            }
        }
        assert!(
            result.contains("value") && result.contains("not a comment"),
            "quoted string content should be present: {result:?}"
        );
    }

    // ND4: Empty string value is quoted in output.
    // The parser preserves "" as a double-quoted scalar; needs_quoting("") returns true.
    #[test]
    fn empty_string_value_is_quoted() {
        let input = "key: \"\"\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("\"\"") || result.contains("''"),
            "empty string should be quoted: {result:?}"
        );
    }

    // ND5: Numeric-looking string stays quoted.
    // The parser preserves "123" (double-quoted in source) as a double-quoted scalar.
    // needs_quoting("123") is true (looks_like_number), so it is re-quoted on output.
    #[test]
    fn numeric_looking_string_stays_quoted() {
        let input = "version: \"123\"\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("\"123\"") || result.contains("'123'"),
            "numeric-looking string should be quoted: {result:?}"
        );
    }

    // ND6: Scalar nodes with preserved styles (Literal, Folded, SingleQuoted,
    // DoubleQuoted, Plain) are tested in the "Scalar Style Preservation" section below.

    // ND7: Alias nodes appear in lossless mode (which the formatter uses).
    // They are rendered as `*name`.

    // ND8: (Removed — no longer applicable.)

    // `"on"` is a V1.1 boolean keyword; quotes preserved only when V1.1 is active.
    #[test]
    fn format_yaml_quoted_on_key_stays_quoted() {
        let result = format_yaml("\"on\": push\n", &opts_with_version(YamlVersion::V1_1));
        assert!(
            result.contains("\"on\""),
            "explicitly quoted on: key should stay quoted in V1.1: {result:?}"
        );
    }

    // `yes` and `no` are YAML 1.1 boolean keywords; preserved as plain scalars.
    #[test]
    fn format_yaml_other_yaml11_booleans_unquoted() {
        let result = format_yaml("yes: no\n", &default_opts());
        assert!(
            result.contains("yes:"),
            "yes: key should not be quoted: {result:?}"
        );
        assert!(
            result.contains("no"),
            "no value should not be quoted: {result:?}"
        );
    }

    // ---- Formatter: Blank Line Preservation ----

    #[test]
    fn format_yaml_blank_line_between_top_level_keys_preserved() {
        let input = "on: push\n\npermissions:\n  contents: read\n\njobs:\n  build: {}\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("on: push\n\npermissions:"),
            "blank line between on: and permissions: missing: {result:?}"
        );
        assert!(result.contains("jobs:"), "jobs: key missing: {result:?}");
        let on_pos = result.find("on: push").unwrap();
        let jobs_pos = result.find("jobs:").unwrap();
        let between = &result[on_pos..jobs_pos];
        assert!(
            between.contains("\n\n"),
            "expected at least one blank line before jobs: {result:?}"
        );
    }

    #[test]
    fn format_yaml_blank_line_between_nested_keys_preserved() {
        let input = "parent:\n  a: 1\n\n  b: 2\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("a: 1\n\n") || result.contains("a: 1\n\n  b:"),
            "blank line between nested a and b missing: {result:?}"
        );
    }

    #[test]
    fn format_yaml_multiple_consecutive_blank_lines_collapsed_to_one() {
        let input = "a: 1\n\n\nb: 2\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("a: 1\n\nb: 2"),
            "expected exactly one blank line: {result:?}"
        );
        assert!(
            !result.contains("a: 1\n\n\nb: 2"),
            "two consecutive blank lines should collapse to one: {result:?}"
        );
    }

    #[test]
    fn format_yaml_blank_line_between_sequence_items_preserved() {
        let input = "items:\n  - a: 1\n\n  - b: 2\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("\n\n"),
            "blank line between sequence items missing: {result:?}"
        );
    }

    #[test]
    fn format_yaml_blank_lines_and_comments_coexist() {
        let input = "# section one\na: 1\n\n# section two\nb: 2\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("# section one"),
            "first comment missing: {result:?}"
        );
        assert!(
            result.contains("# section two"),
            "second comment missing: {result:?}"
        );
        let first_pos = result.find("a: 1").unwrap();
        let second_pos = result.find("# section two").unwrap();
        let between = &result[first_pos..second_pos];
        assert!(
            between.contains("\n\n"),
            "blank line between sections missing: {result:?}"
        );
    }

    #[test]
    fn format_yaml_blank_lines_inside_block_scalar_unaffected() {
        let input = "body: |\n  line one\n\n  line three\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("line one"),
            "block content missing: {result:?}"
        );
        assert!(
            result.contains("line three"),
            "block content missing: {result:?}"
        );
    }

    // AC8: Multiple trailing leading comments at EOF, including a blank-line separator.
    #[test]
    fn attach_comments_multiple_trailing_leading_comments_at_eof() {
        let input = "key: value\n# first EOF comment\n\n# second EOF comment\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("key: value"), "content missing: {result:?}");
        assert!(
            result.contains("# first EOF comment"),
            "first EOF comment missing: {result:?}"
        );
        assert!(
            result.contains("# second EOF comment"),
            "second EOF comment missing: {result:?}"
        );
        let content_pos = result.find("key: value").unwrap();
        let first_pos = result.find("# first EOF comment").unwrap();
        let second_pos = result.find("# second EOF comment").unwrap();
        assert!(
            first_pos > content_pos,
            "first EOF comment should follow content"
        );
        assert!(
            second_pos > first_pos,
            "second EOF comment should follow first"
        );
    }

    // Tests for sequence node paths in sequence_to_doc / sequence_item_to_doc.

    // SQ1: Empty sequence value formats as `[]` (line 536).
    // the parser parses `[]` as Sequence([]), triggering the seq.is_empty() early return.
    #[test]
    fn empty_sequence_formats_as_brackets() {
        let input = "empty_seq: []\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("[]"),
            "empty sequence should format as []: {result:?}"
        );
    }

    // SQ2: Empty mapping value formats as `{}` (line 490).
    // the parser parses `{}` as Mapping({}), triggering the map.is_empty() early return.
    #[test]
    fn empty_mapping_formats_as_braces() {
        let input = "empty_map: {}\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("{}"),
            "empty mapping should format as {{}}: {result:?}"
        );
    }

    // SQ3: Nested sequence-in-sequence (lines 562-564).
    // the parser produces Sequence([Sequence([...]), ...]) for `- - item` syntax.
    // The non-empty Sequence arm in sequence_item_to_doc fires for the inner sequence.
    #[test]
    fn nested_sequence_in_sequence() {
        let input = "outer:\n  - - inner1\n    - inner2\n  - simple\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("inner1"), "inner1 missing: {result:?}");
        assert!(result.contains("inner2"), "inner2 missing: {result:?}");
        assert!(result.contains("simple"), "simple missing: {result:?}");
        let outer_pos = result.find("outer:").unwrap();
        let inner1_pos = result.find("inner1").unwrap();
        assert!(
            inner1_pos > outer_pos,
            "inner1 should appear after outer key: {result:?}"
        );
    }

    // Tests for multi-document edge cases.

    // MD1: `...` document-end terminator.
    // the parser treats `...` as a document boundary (same role as `---`). The formatter
    // always emits `---` separators between documents, so `...` terminators are not
    // preserved in the output — this is a known limitation. Content is preserved.
    #[test]
    fn document_end_terminator_content_preserved() {
        let input = "key1: value1\n...\n---\nkey2: value2\n";
        let result = format_yaml(input, &default_opts());
        assert!(
            result.contains("key1: value1"),
            "doc1 content missing: {result:?}"
        );
        assert!(
            result.contains("key2: value2"),
            "doc2 content missing: {result:?}"
        );
        assert!(
            result.contains("---"),
            "document separator missing: {result:?}"
        );
    }

    // MD2: Three-document file using mixed `---` and `...` separators.
    // the parser parses all three as separate documents. The formatter joins them with `---`.
    #[test]
    fn three_document_mixed_separators() {
        let input = "key: value\n...\nkey2: value2\n---\nkey3: value3\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("key: value"), "doc1 missing: {result:?}");
        assert!(result.contains("key2: value2"), "doc2 missing: {result:?}");
        assert!(result.contains("key3: value3"), "doc3 missing: {result:?}");
    }

    // MD3: Document closed by `...` with no following document.
    // the parser produces one document; `...` is consumed as a terminator.
    // The formatter emits the single document without any `---`.
    #[test]
    fn single_document_with_dot_terminator() {
        let input = "key: value\n...\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("key: value"), "content missing: {result:?}");
        assert!(
            !result.contains("---"),
            "no separator expected for single doc: {result:?}"
        );
    }

    // ---- Formatter: Flow-to-Block Sequence Indentation ----

    fn leading_spaces(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    // FI1: K8s containers/command pattern — flow sequence inside a mapping that
    // is itself a sequence item.  Items must be indented two spaces deeper than
    // the `command:` key.
    #[test]
    fn format_yaml_flow_sequence_in_mapping_in_sequence_item() {
        let input = "spec:\n  containers:\n    - name: test\n      command: [\"python\", \"-m\", \"http.server\", \"5000\"]\n";
        let result = format_yaml(input, &default_opts());

        assert!(
            result.contains("command:"),
            "command: key missing: {result:?}"
        );

        let command_pos = result.find("command:").expect("command: not found");
        let command_line = result[..command_pos].lines().count().saturating_sub(1);
        let lines: Vec<&str> = result.lines().collect();
        let command_indent = leading_spaces(lines[command_line]);

        let item_lines: Vec<&str> = lines[command_line + 1..]
            .iter()
            .take_while(|l| l.trim_start().starts_with('-') || l.trim().is_empty())
            .filter(|l| l.trim_start().starts_with('-'))
            .copied()
            .collect();

        assert!(
            !item_lines.is_empty(),
            "no sequence items found after command: in {result:?}"
        );
        for item in &item_lines {
            assert!(
                leading_spaces(item) > command_indent,
                "item {item:?} not indented deeper than command: (indent {command_indent}): {result:?}"
            );
        }
    }

    // FI2: Flow sequence inside a nested mapping (not a sequence item).
    // Verify `command:` items are indented deeper than `command:` itself.
    #[test]
    fn format_yaml_flow_sequence_in_nested_mapping() {
        let input = "job:\n  run:\n    command: [\"echo\", \"hello\"]\n";
        let result = format_yaml(input, &default_opts());

        assert!(
            result.contains("command:"),
            "command: key missing: {result:?}"
        );

        let command_pos = result.find("command:").expect("command: not found");
        let command_line = result[..command_pos].lines().count().saturating_sub(1);
        let lines: Vec<&str> = result.lines().collect();
        let command_indent = leading_spaces(lines[command_line]);

        let item_lines: Vec<&str> = lines[command_line + 1..]
            .iter()
            .take_while(|l| l.trim_start().starts_with('-') || l.trim().is_empty())
            .filter(|l| l.trim_start().starts_with('-'))
            .copied()
            .collect();

        assert!(
            !item_lines.is_empty(),
            "no items found after command: in {result:?}"
        );
        for item in &item_lines {
            assert!(
                leading_spaces(item) > command_indent,
                "item {item:?} not deeper than command: (indent {command_indent}): {result:?}"
            );
        }
    }

    // FI3: Single-element flow sequence in a mapping value.
    #[test]
    fn format_yaml_single_element_flow_sequence() {
        let input = "args: [\"--verbose\"]\n";
        let result = format_yaml(input, &default_opts());

        assert!(result.contains("args:"), "args: key missing: {result:?}");

        let args_pos = result.find("args:").expect("args: not found");
        let args_line = result[..args_pos].lines().count().saturating_sub(1);
        let lines: Vec<&str> = result.lines().collect();
        let args_indent = leading_spaces(lines[args_line]);

        let item_lines: Vec<&str> = lines[args_line + 1..]
            .iter()
            .take_while(|l| l.trim_start().starts_with('-') || l.trim().is_empty())
            .filter(|l| l.trim_start().starts_with('-'))
            .copied()
            .collect();

        assert!(
            !item_lines.is_empty(),
            "no items found after args: in {result:?}"
        );
        for item in &item_lines {
            assert!(
                leading_spaces(item) > args_indent,
                "item {item:?} not deeper than args: (indent {args_indent}): {result:?}"
            );
        }
    }

    // FI4: Deeply nested — flow sequence three levels deep (sequence item inside
    // a mapping inside a sequence item inside a mapping).
    #[test]
    fn format_yaml_deeply_nested_flow_sequence() {
        let input = "jobs:\n  build:\n    steps:\n      - name: run\n        run: [\"bash\", \"-c\", \"echo hi\"]\n";
        let result = format_yaml(input, &default_opts());

        assert!(result.contains("run:"), "run: key missing: {result:?}");

        let run_pos = result.rfind("run:").expect("run: not found");
        let run_line = result[..run_pos].lines().count().saturating_sub(1);
        let lines: Vec<&str> = result.lines().collect();
        let run_indent = leading_spaces(lines[run_line]);

        let after_run: Vec<&str> = lines[run_line + 1..]
            .iter()
            .take_while(|l| l.trim_start().starts_with('-') || l.trim().is_empty())
            .filter(|l| l.trim_start().starts_with('-'))
            .copied()
            .collect();

        if !after_run.is_empty() {
            for item in &after_run {
                assert!(
                    leading_spaces(item) > run_indent,
                    "item {item:?} not deeper than run: (indent {run_indent}): {result:?}"
                );
            }
        }
        assert!(
            result.contains("bash") || result.contains("echo"),
            "sequence content missing: {result:?}"
        );
    }

    // FI5: Top-level regression — a simple top-level flow sequence must still
    // produce block items at the correct indent (2 spaces for top-level key).
    #[test]
    fn format_yaml_top_level_flow_sequence_correct_indent() {
        let input = "items: [\"a\", \"b\", \"c\"]\n";
        let result = format_yaml(input, &default_opts());

        assert!(result.contains("items:"), "items: key missing: {result:?}");

        let items_pos = result.find("items:").expect("items: not found");
        let items_line = result[..items_pos].lines().count().saturating_sub(1);
        let lines: Vec<&str> = result.lines().collect();
        let items_indent = leading_spaces(lines[items_line]);

        let item_lines: Vec<&str> = lines[items_line + 1..]
            .iter()
            .take_while(|l| l.trim_start().starts_with('-') || l.trim().is_empty())
            .filter(|l| l.trim_start().starts_with('-'))
            .copied()
            .collect();

        assert!(
            !item_lines.is_empty(),
            "no items found after items: in {result:?}"
        );
        for item in &item_lines {
            assert!(
                leading_spaces(item) > items_indent,
                "item {item:?} not indented deeper than items: (indent {items_indent}): {result:?}"
            );
        }
    }

    // ---- Formatter: Unnecessary Quote Stripping ----

    // QS6: Double-quoted string containing `: ` — parser limitation note.
    // Known parser limitation: rlsp-yaml-parser strips spaces after `:` in
    // double-quoted strings, so "key: value" becomes "key:value". Since the
    // space is lost, needs_quoting no longer triggers and the value is emitted
    // plain. This test verifies the formatter doesn't crash and produces output.
    #[test]
    fn format_yaml_double_quoted_string_with_colon_space_kept_quoted() {
        let result = format_yaml("value: \"key: value\"\n", &default_opts());
        assert!(
            result.contains("key:"),
            "value content should be present: {result:?}"
        );
    }

    // QS8: Quoted strings in a flow sequence stripped after block conversion.
    #[test]
    fn format_yaml_quoted_string_in_block_sequence_stripped() {
        let result = format_yaml(
            "args: [\"python\", \"-m\", \"http.server\"]\n",
            &default_opts(),
        );
        // Safe strings: quotes stripped.
        assert!(
            !result.contains("\"python\""),
            "\"python\" quotes should be stripped: {result:?}"
        );
        assert!(
            !result.contains("\"http.server\""),
            "\"http.server\" quotes should be stripped: {result:?}"
        );
        // `-m` starts with `-` which triggers needs_quoting → stays quoted.
        assert!(
            result.contains("\"-m\""),
            "\"-m\" quotes must be preserved (starts with '-'): {result:?}"
        );
    }

    // QS10: When `single_quote: true`, stripped value is re-quoted in single quotes.
    #[test]
    fn format_yaml_quote_stripping_respects_single_quote_option() {
        let opts = YamlFormatOptions {
            single_quote: true,
            ..default_opts()
        };
        let result = format_yaml("value: \"python\"\n", &opts);
        assert!(
            result.contains("'python'"),
            "single_quote option should apply single quotes: {result:?}"
        );
        assert!(
            !result.contains("\"python\""),
            "original double quotes should not be preserved: {result:?}"
        );
    }
}
