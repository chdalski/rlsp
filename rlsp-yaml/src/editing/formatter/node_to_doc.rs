// SPDX-License-Identifier: MIT

use rlsp_fmt::{Doc, concat, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{ScalarStyle, Span};

use super::mapping_render;
use super::options::YamlFormatOptions;
use super::scalar_render::{
    escape_double_quoted, format_tag, is_core_schema_tag, needs_flow_quoting, needs_quoting,
    repr_block_to_doc, requires_double_quoting, string_to_doc,
};
use super::sequence_render;

/// Convert a `Node<Span>` to a `Doc` IR node.
///
/// When `in_key` is `true`, the `single_quote` style option is suppressed for
/// scalar strings — keys are never single-quoted by style preference alone.
#[expect(
    clippy::too_many_lines,
    reason = "comprehensive match over all node variants"
)]
pub(super) fn node_to_doc(node: &Node<Span>, options: &YamlFormatOptions, in_key: bool) -> Doc {
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
pub(super) fn flow_item_to_doc(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    in_key: bool,
) -> Doc {
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
