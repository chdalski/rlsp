// SPDX-License-Identifier: MIT

use rlsp_fmt::{Doc, concat, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{ScalarStyle, Span};

use super::mapping_render::{self, Entry, with_comments_above};
use super::options::YamlFormatOptions;
use super::scalar_render::{
    block_value_needs_quoting, escape_double_quoted, flow_quoted_to_doc, format_tag,
    is_core_schema_tag, needs_flow_quoting, needs_quoting, repr_block_to_doc,
    requires_double_quoting, string_to_doc,
};
use super::sequence_render;

/// Convert a `Node<Span>` to a `Doc` IR node.
///
/// When `in_key` is `true`, the `single_quote` style option is suppressed for
/// scalar strings — keys are never single-quoted by style preference alone.
///
/// A block scalar prints its own trailing comment on its header line.
pub(super) fn node_to_doc(node: &Node<Span>, options: &YamlFormatOptions, in_key: bool) -> Doc {
    render_node(node, options, in_key, false, node.trailing_comment())
}

/// Convert a mapping value to a `Doc`.  A block scalar value prints
/// `header_comment` on its header line, which is the slot the entry chose for
/// its comment (see `mapping_render::place_entry_comments`).
pub(super) fn value_to_doc(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    header_comment: Option<&str>,
) -> Doc {
    render_node(node, options, false, false, header_comment)
}

/// Whether the formatter prints `node` as a block scalar, so that its header
/// line is a slot for a comment.  A block scalar whose value cannot be printed
/// as one (see `block_value_needs_quoting`) comes out double-quoted and has no
/// header.
pub fn prints_block_header(node: &Node<Span>) -> bool {
    matches!(
        node,
        Node::Scalar {
            style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
            value,
            ..
        } if !block_value_needs_quoting(value)
    )
}

/// Whether the formatter prints nothing for `node` itself: an empty plain scalar
/// with no anchor and no tag that is printed.  A comment may still follow it.
pub(super) fn renders_empty(node: &Node<Span>) -> bool {
    let Node::Scalar {
        value,
        style: ScalarStyle::Plain,
        tag,
        ..
    } = node
    else {
        return false;
    };
    // A core tag is printed, in its short form, only on an empty scalar that
    // carries it in the source; any other tag is always printed.
    let tag_printed = tag
        .as_ref()
        .is_some_and(|t| !is_core_schema_tag(t) || node.tag_loc().is_some());
    value.is_empty() && !tag_printed && node.anchor().is_none()
}

/// Whether the formatter prints no text for a scalar apart from its properties:
/// an empty plain scalar, or an empty scalar of any style whose tag is printed
/// in its place.  An anchor or tag may still be printed.
pub(super) fn prints_no_text(node: &Node<Span>) -> bool {
    let Node::Scalar {
        value, style, tag, ..
    } = node
    else {
        return false;
    };
    let tag_printed = tag
        .as_ref()
        .is_some_and(|t| !is_core_schema_tag(t) || node.tag_loc().is_some());
    value.is_empty() && (matches!(style, ScalarStyle::Plain) || tag_printed)
}

/// Render `node`; `in_flow` marks a scalar that sits directly inside a flow
/// collection, where `,` `[` `]` `{` `}` end a plain scalar (`ns-plain-char(c)`
/// [130] admits only `ns-plain-safe(c)` [127], which is `ns-plain-safe-in` [129]
/// = `ns-char - c-flow-indicator` in flow context), so a scalar holding one of
/// them is emitted quoted.
#[expect(
    clippy::too_many_lines,
    reason = "comprehensive match over all node variants"
)]
fn render_node(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    in_key: bool,
    in_flow: bool,
    header_comment: Option<&str>,
) -> Doc {
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
                    // A value with an over-indented blank line cannot be printed as a
                    // block scalar (see `block_value_needs_quoting`).
                    if block_value_needs_quoting(value) {
                        text(format!("\"{}\"", escape_double_quoted(value)))
                    } else {
                        repr_block_to_doc(value, *style, options.tab_width, header_comment)
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
                    } else if in_flow && needs_flow_quoting(value) {
                        // Stripping the quotes would leave a flow indicator in a plain scalar.
                        flow_quoted_to_doc(value, options, in_key)
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
                    } else if in_flow && needs_flow_quoting(value) {
                        flow_quoted_to_doc(value, options, in_key)
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
                    if tag_present_on_empty || !matches!(style, ScalarStyle::Plain) {
                        // `&anchor !!tag`, or `&anchor ""` — space required between
                        // anchor and what follows.
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
            let has_properties =
                mapping_render::collection_properties(node.anchor(), tag.as_deref()).is_some();
            let Entry { above, doc } =
                mapping_render::mapping_to_parts(entries, *style, options, has_properties);
            let effective_style = if options.format_enforce_block_style {
                CollectionStyle::Block
            } else {
                *style
            };
            with_comments_above(
                mapping_render::prepend_collection_properties(
                    doc,
                    node.anchor(),
                    tag.as_deref(),
                    effective_style,
                ),
                above,
            )
        }

        Node::Sequence {
            items, style, tag, ..
        } => {
            let Entry { above, doc } = sequence_render::sequence_to_parts(items, *style, options);
            let effective_style = if options.format_enforce_block_style {
                CollectionStyle::Block
            } else {
                *style
            };
            with_comments_above(
                mapping_render::prepend_collection_properties(
                    doc,
                    node.anchor(),
                    tag.as_deref(),
                    effective_style,
                ),
                above,
            )
        }

        Node::Alias { name, .. } => text(format!("*{name}")),
    }
}

/// Emit a node for use inside a flow collection (flow sequence or flow mapping).
///
/// A scalar that contains flow indicators is quoted whatever its source
/// style, so it is not misread as a separator or delimiter by a YAML parser.
pub(super) fn flow_item_to_doc(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    in_key: bool,
) -> Doc {
    render_node(node, options, in_key, true, None)
}
