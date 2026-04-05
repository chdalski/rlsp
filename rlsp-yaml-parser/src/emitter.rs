// SPDX-License-Identifier: MIT

//! YAML emitter — converts AST `Document<Span>` values back to YAML text.
//!
//! The entry points are [`emit`] and [`emit_to_writer`].  Both accept a slice
//! of documents and an [`EmitConfig`] that controls indentation, line width,
//! and default scalar/collection styles.

use std::io::{self, Write};

use crate::event::{Chomp, ScalarStyle};
use crate::node::{Document, Node};
use crate::pos::Span;

// ---------------------------------------------------------------------------
// Public configuration
// ---------------------------------------------------------------------------

/// Whether a collection is emitted in block or flow style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionStyle {
    /// Multi-line block style (indented).
    Block,
    /// Inline flow style (`{key: val}` / `[a, b]`).
    Flow,
}

/// Configuration for the emitter.
#[derive(Debug, Clone)]
pub struct EmitConfig {
    /// Spaces per indentation level (default 2).
    pub indent_width: usize,
    /// Soft line-width hint (default 80). Not currently enforced strictly.
    pub line_width: usize,
    /// Default style for scalars that have no explicit style set.
    pub default_scalar_style: ScalarStyle,
    /// Default style for collections.
    pub default_collection_style: CollectionStyle,
}

impl Default for EmitConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            line_width: 80,
            default_scalar_style: ScalarStyle::Plain,
            default_collection_style: CollectionStyle::Block,
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Emit `documents` to a `String` using `config`.
///
/// # Panics
///
/// Never panics in practice — panics are unreachable because writing to an
/// in-memory `Vec<u8>` is infallible and the emitter only produces valid UTF-8.
#[must_use]
pub fn emit(documents: &[Document<Span>], config: &EmitConfig) -> String {
    let mut buf = Vec::new();
    // Writing to Vec<u8> is infallible.
    let _: io::Result<()> = emit_to_writer(documents, config, &mut buf);
    // SAFETY: the emitter only writes ASCII and existing String content.
    String::from_utf8(buf).unwrap_or_default()
}

/// Emit `documents` to `writer` using `config`.
///
/// # Errors
///
/// Returns an error if writing to `writer` fails.
pub fn emit_to_writer(
    documents: &[Document<Span>],
    config: &EmitConfig,
    writer: &mut dyn Write,
) -> io::Result<()> {
    let mut emitter = Emitter { config, writer };
    for (i, doc) in documents.iter().enumerate() {
        emitter.emit_document(doc, i > 0)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal emitter state
// ---------------------------------------------------------------------------

struct Emitter<'a> {
    config: &'a EmitConfig,
    writer: &'a mut dyn Write,
}

impl Emitter<'_> {
    fn emit_document(&mut self, doc: &Document<Span>, is_multi: bool) -> io::Result<()> {
        // Emit document-start marker when there are multiple documents or
        // a version directive is present.
        if is_multi || doc.version.is_some() {
            writeln!(self.writer, "---")?;
        }

        // Emit comments before the root node.
        for comment in &doc.comments {
            writeln!(self.writer, "# {comment}")?;
        }

        self.emit_node(&doc.root, 0, false)?;

        // Ensure the document ends with a newline.
        writeln!(self.writer)?;
        Ok(())
    }

    fn emit_node(&mut self, node: &Node<Span>, indent: usize, in_flow: bool) -> io::Result<()> {
        match node {
            Node::Scalar {
                value,
                style,
                anchor,
                tag,
                ..
            } => self.emit_scalar(value, *style, anchor.as_deref(), tag.as_deref()),
            Node::Mapping {
                entries,
                anchor,
                tag,
                ..
            } => self.emit_mapping(entries, anchor.as_deref(), tag.as_deref(), indent, in_flow),
            Node::Sequence {
                items, anchor, tag, ..
            } => self.emit_sequence(items, anchor.as_deref(), tag.as_deref(), indent, in_flow),
            Node::Alias { name, .. } => write!(self.writer, "*{name}"),
        }
    }

    // -----------------------------------------------------------------------
    // Scalar
    // -----------------------------------------------------------------------

    fn emit_scalar(
        &mut self,
        value: &str,
        style: ScalarStyle,
        anchor: Option<&str>,
        tag: Option<&str>,
    ) -> io::Result<()> {
        if let Some(name) = anchor {
            write!(self.writer, "&{name} ")?;
        }
        if let Some(t) = tag {
            write!(self.writer, "{} ", format_tag(t))?;
        }
        match style {
            ScalarStyle::Plain => {
                let s = if needs_quoting(value) {
                    format!("'{}'", value.replace('\'', "''"))
                } else {
                    value.to_owned()
                };
                write!(self.writer, "{s}")
            }
            ScalarStyle::SingleQuoted => {
                write!(self.writer, "'{}'", value.replace('\'', "''"))
            }
            ScalarStyle::DoubleQuoted => {
                write!(self.writer, "\"{}\"", escape_double(value))
            }
            ScalarStyle::Literal(chomp) => {
                let indicator = chomp_indicator(chomp);
                writeln!(self.writer, "|{indicator}")?;
                for line in value.split('\n') {
                    writeln!(self.writer, "  {line}")?;
                }
                Ok(())
            }
            ScalarStyle::Folded(chomp) => {
                let indicator = chomp_indicator(chomp);
                writeln!(self.writer, ">{indicator}")?;
                for line in value.split('\n') {
                    writeln!(self.writer, "  {line}")?;
                }
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Mapping
    // -----------------------------------------------------------------------

    fn emit_mapping(
        &mut self,
        entries: &[(Node<Span>, Node<Span>)],
        anchor: Option<&str>,
        tag: Option<&str>,
        indent: usize,
        in_flow: bool,
    ) -> io::Result<()> {
        // Determine effective style.
        let style = if in_flow {
            CollectionStyle::Flow
        } else {
            self.config.default_collection_style
        };

        // Prefix.
        if let Some(name) = anchor {
            write!(self.writer, "&{name} ")?;
        }
        if let Some(t) = tag {
            write!(self.writer, "{} ", format_tag(t))?;
        }

        if entries.is_empty() {
            return write!(self.writer, "{{}}");
        }

        match style {
            CollectionStyle::Flow => {
                write!(self.writer, "{{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(self.writer, ", ")?;
                    }
                    self.emit_node(key, indent, true)?;
                    write!(self.writer, ": ")?;
                    self.emit_node(value, indent, true)?;
                }
                write!(self.writer, "}}")
            }
            CollectionStyle::Block => {
                let child_indent = indent + self.config.indent_width;
                let pad: String = " ".repeat(indent);
                for (key, value) in entries {
                    write!(self.writer, "{pad}")?;
                    self.emit_node(key, child_indent, false)?;
                    write!(self.writer, ": ")?;
                    // If value is a block collection, put it on the next line.
                    if is_block_collection(value, self.config.default_collection_style) {
                        writeln!(self.writer)?;
                        write!(self.writer, "{}", " ".repeat(child_indent))?;
                    }
                    self.emit_node(value, child_indent, false)?;
                    writeln!(self.writer)?;
                }
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Sequence
    // -----------------------------------------------------------------------

    fn emit_sequence(
        &mut self,
        items: &[Node<Span>],
        anchor: Option<&str>,
        tag: Option<&str>,
        indent: usize,
        in_flow: bool,
    ) -> io::Result<()> {
        let style = if in_flow {
            CollectionStyle::Flow
        } else {
            self.config.default_collection_style
        };

        if let Some(name) = anchor {
            write!(self.writer, "&{name} ")?;
        }
        if let Some(t) = tag {
            write!(self.writer, "{} ", format_tag(t))?;
        }

        if items.is_empty() {
            return write!(self.writer, "[]");
        }

        match style {
            CollectionStyle::Flow => {
                write!(self.writer, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(self.writer, ", ")?;
                    }
                    self.emit_node(item, indent, true)?;
                }
                write!(self.writer, "]")
            }
            CollectionStyle::Block => {
                let pad: String = " ".repeat(indent);
                let child_indent = indent + self.config.indent_width;
                for item in items {
                    write!(self.writer, "{pad}- ")?;
                    self.emit_node(item, child_indent, false)?;
                    writeln!(self.writer)?;
                }
                Ok(())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true if a plain scalar value must be quoted to round-trip safely.
fn needs_quoting(value: &str) -> bool {
    // Empty string must be quoted.
    if value.is_empty() {
        return true;
    }
    // Reserved words that would be interpreted as non-string types.
    matches!(
        value,
        "null"
            | "Null"
            | "NULL"
            | "~"
            | "true"
            | "True"
            | "TRUE"
            | "false"
            | "False"
            | "FALSE"
            | ".inf"
            | ".Inf"
            | ".INF"
            | "-.inf"
            | "-.Inf"
            | "-.INF"
            | ".nan"
            | ".NaN"
            | ".NAN"
    )
}

/// Escape special characters for double-quoted scalars.
fn escape_double(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
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

/// Returns the chomping indicator character for block scalars.
const fn chomp_indicator(chomp: Chomp) -> &'static str {
    match chomp {
        Chomp::Strip => "-",
        Chomp::Clip => "",
        Chomp::Keep => "+",
    }
}

/// Format a tag handle for emission.
fn format_tag(tag: &str) -> String {
    tag.strip_prefix("tag:yaml.org,2002:").map_or_else(
        || {
            if tag.starts_with('!') {
                tag.to_owned()
            } else {
                format!("!<{tag}>")
            }
        },
        |suffix| format!("!!{suffix}"),
    )
}

/// Returns true when a node is a block-style collection (needs its own line).
const fn is_block_collection(node: &Node<Span>, default_style: CollectionStyle) -> bool {
    match (node, default_style) {
        (Node::Mapping { entries, .. }, CollectionStyle::Block) if !entries.is_empty() => true,
        (Node::Sequence { items, .. }, CollectionStyle::Block) if !items.is_empty() => true,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;
    use crate::loader::load;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn null_span() -> Span {
        use crate::pos::Pos;
        let p = Pos {
            byte_offset: 0,
            char_offset: 0,
            line: 1,
            column: 0,
        };
        Span { start: p, end: p }
    }

    fn scalar(value: &str) -> Node<Span> {
        Node::Scalar {
            value: value.to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        }
    }

    fn doc(root: Node<Span>) -> Document<Span> {
        Document {
            root,
            version: None,
            tags: vec![],
            comments: vec![],
        }
    }

    fn default_config() -> EmitConfig {
        EmitConfig::default()
    }

    fn reload_one(yaml: &str) -> Node<Span> {
        load(yaml)
            .expect("reload failed")
            .into_iter()
            .next()
            .unwrap()
            .root
    }

    // -----------------------------------------------------------------------
    // Group 1: Spike test
    // -----------------------------------------------------------------------

    #[test]
    fn emit_plain_scalar_round_trips() {
        let docs = load("hello\n").expect("parse failed");
        let config = default_config();

        let result = emit(&docs, &config);

        assert!(result.contains("hello"), "result: {result:?}");
        assert!(load(&result).is_ok(), "reload failed: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 2: EmitConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn config_default_indent_width_is_2() {
        assert_eq!(EmitConfig::default().indent_width, 2);
    }

    #[test]
    fn config_default_line_width_is_80() {
        assert_eq!(EmitConfig::default().line_width, 80);
    }

    #[test]
    fn config_default_scalar_style_is_plain() {
        assert!(matches!(
            EmitConfig::default().default_scalar_style,
            ScalarStyle::Plain
        ));
    }

    #[test]
    fn config_default_collection_style_is_block() {
        assert!(matches!(
            EmitConfig::default().default_collection_style,
            CollectionStyle::Block
        ));
    }

    #[test]
    fn config_indent_width_4_used_in_block_mapping() {
        let config = EmitConfig {
            indent_width: 4,
            ..EmitConfig::default()
        };
        let root = Node::Mapping {
            entries: vec![(scalar("key"), scalar("value"))],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &config);
        // key appears at indent 0; "value" is on same line after ": "
        assert!(result.contains("key: value"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 3: Plain scalar styles
    // -----------------------------------------------------------------------

    #[test]
    fn plain_scalar_emits_unquoted() {
        let result = emit(&[doc(scalar("hello"))], &default_config());
        assert!(result.contains("hello"), "result: {result:?}");
        assert!(!result.contains('"'), "result: {result:?}");
        assert!(!result.contains('\''), "result: {result:?}");
    }

    #[test]
    fn plain_scalar_empty_string_emits_quoted() {
        let node = Node::Scalar {
            value: String::new(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        // Empty plain scalar must be quoted to distinguish from null.
        assert!(result.contains("''"), "result: {result:?}");
    }

    #[test]
    fn plain_scalar_null_word_emits_single_quoted() {
        let node = Node::Scalar {
            value: "null".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("'null'"), "result: {result:?}");
    }

    #[test]
    fn plain_scalar_true_emits_single_quoted() {
        let node = Node::Scalar {
            value: "true".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("'true'"), "result: {result:?}");
    }

    #[test]
    fn plain_scalar_false_emits_single_quoted() {
        let node = Node::Scalar {
            value: "false".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("'false'"), "result: {result:?}");
    }

    #[test]
    fn plain_scalar_tilde_emits_single_quoted() {
        let node = Node::Scalar {
            value: "~".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("'~'"), "result: {result:?}");
    }

    #[test]
    fn plain_scalar_inf_emits_single_quoted() {
        let node = Node::Scalar {
            value: ".inf".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("'.inf'"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 4: Single-quoted scalar
    // -----------------------------------------------------------------------

    #[test]
    fn single_quoted_scalar_wraps_in_single_quotes() {
        let node = Node::Scalar {
            value: "hello world".to_owned(),
            style: ScalarStyle::SingleQuoted,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("'hello world'"), "result: {result:?}");
    }

    #[test]
    fn single_quoted_scalar_escapes_embedded_single_quote() {
        let node = Node::Scalar {
            value: "it's".to_owned(),
            style: ScalarStyle::SingleQuoted,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("'it''s'"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 5: Double-quoted scalar
    // -----------------------------------------------------------------------

    #[test]
    fn double_quoted_scalar_wraps_in_double_quotes() {
        let node = Node::Scalar {
            value: "hello world".to_owned(),
            style: ScalarStyle::DoubleQuoted,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("\"hello world\""), "result: {result:?}");
    }

    #[test]
    fn double_quoted_scalar_escapes_embedded_quote() {
        let node = Node::Scalar {
            value: "say \"hi\"".to_owned(),
            style: ScalarStyle::DoubleQuoted,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains(r#""say \"hi\"""#), "result: {result:?}");
    }

    #[test]
    fn double_quoted_scalar_escapes_newline() {
        let node = Node::Scalar {
            value: "line1\nline2".to_owned(),
            style: ScalarStyle::DoubleQuoted,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("\"line1\\nline2\""), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 6: Block scalar (literal)
    // -----------------------------------------------------------------------

    #[test]
    fn literal_block_scalar_clip_emits_pipe() {
        let node = Node::Scalar {
            value: "line1\nline2".to_owned(),
            style: ScalarStyle::Literal(Chomp::Clip),
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("|\n"), "result: {result:?}");
        assert!(result.contains("line1"), "result: {result:?}");
    }

    #[test]
    fn literal_block_scalar_strip_emits_pipe_minus() {
        let node = Node::Scalar {
            value: "line1\nline2".to_owned(),
            style: ScalarStyle::Literal(Chomp::Strip),
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("|-\n"), "result: {result:?}");
    }

    #[test]
    fn literal_block_scalar_keep_emits_pipe_plus() {
        let node = Node::Scalar {
            value: "line1\nline2".to_owned(),
            style: ScalarStyle::Literal(Chomp::Keep),
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("|+\n"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 7: Block scalar (folded)
    // -----------------------------------------------------------------------

    #[test]
    fn folded_block_scalar_clip_emits_gt() {
        let node = Node::Scalar {
            value: "line1\nline2".to_owned(),
            style: ScalarStyle::Folded(Chomp::Clip),
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains(">\n"), "result: {result:?}");
        assert!(result.contains("line1"), "result: {result:?}");
    }

    #[test]
    fn folded_block_scalar_strip_emits_gt_minus() {
        let node = Node::Scalar {
            value: "line1\nline2".to_owned(),
            style: ScalarStyle::Folded(Chomp::Strip),
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains(">-\n"), "result: {result:?}");
    }

    #[test]
    fn folded_block_scalar_keep_emits_gt_plus() {
        let node = Node::Scalar {
            value: "line1\nline2".to_owned(),
            style: ScalarStyle::Folded(Chomp::Keep),
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains(">+\n"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 8: Block collections
    // -----------------------------------------------------------------------

    #[test]
    fn block_mapping_emits_key_colon_value() {
        let root = Node::Mapping {
            entries: vec![(scalar("name"), scalar("Alice"))],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("name: Alice"), "result: {result:?}");
    }

    #[test]
    fn block_mapping_multiple_entries() {
        let root = Node::Mapping {
            entries: vec![(scalar("a"), scalar("1")), (scalar("b"), scalar("2"))],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("a: 1"), "result: {result:?}");
        assert!(result.contains("b: 2"), "result: {result:?}");
    }

    #[test]
    fn block_sequence_emits_dash_items() {
        let root = Node::Sequence {
            items: vec![scalar("a"), scalar("b"), scalar("c")],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("- a"), "result: {result:?}");
        assert!(result.contains("- b"), "result: {result:?}");
        assert!(result.contains("- c"), "result: {result:?}");
    }

    #[test]
    fn block_mapping_empty_emits_braces() {
        let root = Node::Mapping {
            entries: vec![],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("{}"), "result: {result:?}");
    }

    #[test]
    fn block_sequence_empty_emits_brackets() {
        let root = Node::Sequence {
            items: vec![],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("[]"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 9: Flow collections
    // -----------------------------------------------------------------------

    #[test]
    fn flow_mapping_emits_braces() {
        let config = EmitConfig {
            default_collection_style: CollectionStyle::Flow,
            ..EmitConfig::default()
        };
        let root = Node::Mapping {
            entries: vec![(scalar("key"), scalar("val"))],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &config);
        assert!(result.contains("{key: val}"), "result: {result:?}");
    }

    #[test]
    fn flow_sequence_emits_brackets() {
        let config = EmitConfig {
            default_collection_style: CollectionStyle::Flow,
            ..EmitConfig::default()
        };
        let root = Node::Sequence {
            items: vec![scalar("a"), scalar("b")],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &config);
        assert!(result.contains("[a, b]"), "result: {result:?}");
    }

    #[test]
    fn flow_mapping_empty_emits_braces() {
        let config = EmitConfig {
            default_collection_style: CollectionStyle::Flow,
            ..EmitConfig::default()
        };
        let root = Node::Mapping {
            entries: vec![],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &config);
        assert!(result.contains("{}"), "result: {result:?}");
    }

    #[test]
    fn flow_sequence_empty_emits_brackets() {
        let config = EmitConfig {
            default_collection_style: CollectionStyle::Flow,
            ..EmitConfig::default()
        };
        let root = Node::Sequence {
            items: vec![],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &config);
        assert!(result.contains("[]"), "result: {result:?}");
    }

    #[test]
    fn flow_mapping_multiple_entries_comma_separated() {
        let config = EmitConfig {
            default_collection_style: CollectionStyle::Flow,
            ..EmitConfig::default()
        };
        let root = Node::Mapping {
            entries: vec![(scalar("a"), scalar("1")), (scalar("b"), scalar("2"))],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &config);
        assert!(result.contains("{a: 1, b: 2}"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 10: Anchors and aliases
    // -----------------------------------------------------------------------

    #[test]
    fn anchor_emits_ampersand_prefix() {
        let node = Node::Scalar {
            value: "shared".to_owned(),
            style: ScalarStyle::Plain,
            anchor: Some("ref".to_owned()),
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("&ref shared"), "result: {result:?}");
    }

    #[test]
    fn alias_emits_asterisk_prefix() {
        let root = Node::Sequence {
            items: vec![
                Node::Scalar {
                    value: "val".to_owned(),
                    style: ScalarStyle::Plain,
                    anchor: Some("a".to_owned()),
                    tag: None,
                    loc: null_span(),
                    leading_comments: Vec::new(),
                    trailing_comment: None,
                },
                Node::Alias {
                    name: "a".to_owned(),
                    loc: null_span(),
                    leading_comments: Vec::new(),
                    trailing_comment: None,
                },
            ],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("&a val"), "result: {result:?}");
        assert!(result.contains("*a"), "result: {result:?}");
    }

    #[test]
    fn anchor_on_mapping_emits_before_entries() {
        let root = Node::Mapping {
            entries: vec![(scalar("k"), scalar("v"))],
            anchor: Some("m".to_owned()),
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("&m"), "result: {result:?}");
    }

    #[test]
    fn anchor_on_sequence_emits_before_items() {
        let root = Node::Sequence {
            items: vec![scalar("x")],
            anchor: Some("s".to_owned()),
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("&s"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 11: Tags
    // -----------------------------------------------------------------------

    #[test]
    fn yaml_core_tag_emits_double_bang_shorthand() {
        let node = Node::Scalar {
            value: "42".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: Some("tag:yaml.org,2002:int".to_owned()),
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("!!int"), "result: {result:?}");
    }

    #[test]
    fn local_tag_emits_exclamation_prefix() {
        let node = Node::Scalar {
            value: "val".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: Some("!local".to_owned()),
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(result.contains("!local"), "result: {result:?}");
    }

    #[test]
    fn unknown_uri_tag_emits_angle_bracket_form() {
        let node = Node::Scalar {
            value: "val".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: Some("http://example.com/tag".to_owned()),
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        assert!(
            result.contains("!<http://example.com/tag>"),
            "result: {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Group 12: Multi-document emission
    // -----------------------------------------------------------------------

    #[test]
    fn single_document_no_separator() {
        let result = emit(&[doc(scalar("only"))], &default_config());
        assert!(!result.starts_with("---"), "result: {result:?}");
    }

    #[test]
    fn two_documents_second_has_separator() {
        let docs = vec![doc(scalar("first")), doc(scalar("second"))];
        let result = emit(&docs, &default_config());
        // The second document should be preceded by "---".
        let separator_count = result.matches("---").count();
        assert_eq!(separator_count, 1, "result: {result:?}");
        assert!(result.contains("first"), "result: {result:?}");
        assert!(result.contains("second"), "result: {result:?}");
    }

    #[test]
    fn three_documents_two_separators() {
        let docs = vec![doc(scalar("a")), doc(scalar("b")), doc(scalar("c"))];
        let result = emit(&docs, &default_config());
        let separator_count = result.matches("---").count();
        assert_eq!(separator_count, 2, "result: {result:?}");
    }

    #[test]
    fn empty_document_list_emits_empty_string() {
        let result = emit(&[], &default_config());
        assert!(result.is_empty(), "result: {result:?}");
    }

    #[test]
    fn document_with_version_emits_separator() {
        let mut d = doc(scalar("val"));
        d.version = Some((1, 2));
        let result = emit(&[d], &default_config());
        assert!(result.starts_with("---"), "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 13: Comments
    // -----------------------------------------------------------------------

    #[test]
    fn document_comment_emits_hash_prefix() {
        let mut d = doc(scalar("val"));
        d.comments = vec!["a comment".to_owned()];
        let result = emit(&[d], &default_config());
        assert!(result.contains("# a comment"), "result: {result:?}");
    }

    #[test]
    fn multiple_comments_all_emitted() {
        let mut d = doc(scalar("val"));
        d.comments = vec!["first".to_owned(), "second".to_owned()];
        let result = emit(&[d], &default_config());
        assert!(result.contains("# first"), "result: {result:?}");
        assert!(result.contains("# second"), "result: {result:?}");
    }

    #[test]
    fn comments_appear_before_root_node() {
        let mut d = doc(scalar("val"));
        d.comments = vec!["note".to_owned()];
        let result = emit(&[d], &default_config());
        let comment_pos = result.find("# note").expect("comment missing");
        let val_pos = result.find("val").expect("value missing");
        assert!(comment_pos < val_pos, "result: {result:?}");
    }

    // -----------------------------------------------------------------------
    // Group 14: Edge cases and integration
    // -----------------------------------------------------------------------

    #[test]
    fn nested_mapping_indents_child() {
        let inner = Node::Mapping {
            entries: vec![(scalar("x"), scalar("1"))],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let root = Node::Mapping {
            entries: vec![(scalar("outer"), inner)],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("outer:"), "result: {result:?}");
        assert!(result.contains("x: 1"), "result: {result:?}");
    }

    #[test]
    fn nested_sequence_indents_child() {
        let inner = Node::Sequence {
            items: vec![scalar("a"), scalar("b")],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let root = Node::Sequence {
            items: vec![inner],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &default_config());
        assert!(result.contains("- "), "result: {result:?}");
        assert!(result.contains('a'), "result: {result:?}");
    }

    #[test]
    fn emit_output_is_valid_utf8() {
        let docs = load("key: value\n").expect("parse failed");
        let result = emit(&docs, &default_config());
        // String::from_utf8 was already called inside emit; if we reach here it's valid.
        assert!(!result.is_empty());
    }

    #[test]
    fn emit_to_writer_matches_emit() {
        let docs = load("a: b\n").expect("parse failed");
        let config = default_config();

        let expected = emit(&docs, &config);
        let mut buf = Vec::new();
        emit_to_writer(&docs, &config, &mut buf).expect("write failed");
        let actual = String::from_utf8(buf).expect("utf-8");

        assert_eq!(actual, expected);
    }

    #[test]
    fn scalar_round_trip_plain() {
        let yaml = "greeting: hello\n";
        let docs = load(yaml).expect("parse failed");
        let result = emit(&docs, &default_config());
        let reloaded = reload_one(&result);
        let original = load(yaml).unwrap().into_iter().next().unwrap().root;
        // Value-level equality (ignore location spans).
        assert_eq!(format!("{reloaded:?}"), format!("{original:?}"));
    }

    #[test]
    fn flow_style_sequence_round_trips() {
        let config = EmitConfig {
            default_collection_style: CollectionStyle::Flow,
            ..EmitConfig::default()
        };
        let root = Node::Sequence {
            items: vec![scalar("1"), scalar("2"), scalar("3")],
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(root)], &config);
        assert!(load(&result).is_ok(), "reload failed: {result:?}");
    }

    #[test]
    fn reserved_word_round_trips_as_string() {
        let node = Node::Scalar {
            value: "null".to_owned(),
            style: ScalarStyle::Plain,
            anchor: None,
            tag: None,
            loc: null_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let result = emit(&[doc(node)], &default_config());
        // After reload, the value should still be the string "null" (quoted),
        // not a null node.
        assert!(load(&result).is_ok(), "reload failed: {result:?}");
    }
}
