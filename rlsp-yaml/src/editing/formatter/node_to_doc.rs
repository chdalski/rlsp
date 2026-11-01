// SPDX-License-Identifier: MIT

use rlsp_fmt::{Doc, concat, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{ResolvedTag, ScalarStyle, Span};

use super::mapping_render::{self, Entry, Lead, with_comments_above};
use super::options::YamlFormatOptions;
use super::scalar_render::{
    CORE_TAG_PREFIX, block_value_needs_quoting, escape_double_quoted, flow_quoted_to_doc,
    format_tag, is_core_schema_tag, needs_flow_quoting, needs_quoting, plain_core_tag,
    reads_as_another_node_on_its_own_line, repr_block_to_doc, requires_double_quoting,
    string_to_doc,
};
use super::sequence_render;

/// Convert a `Node<Span>` to a `Doc` IR node.
///
/// When `in_key` is `true`, the `single_quote` style option is suppressed for
/// scalar strings — keys are never single-quoted by style preference alone.
///
/// A block scalar prints its own trailing comment on its header line.
pub(super) fn node_to_doc(node: &Node<Span>, options: &YamlFormatOptions, in_key: bool) -> Doc {
    render_node(
        node,
        options,
        in_key,
        Context::Block,
        node.trailing_comment(),
    )
}

/// Convert a mapping value to a `Doc`.  A block scalar value prints
/// `header_comment` on its header line, which is the slot the entry chose for
/// its comment (see `mapping_render::place_entry_comments`).
pub(super) fn value_to_doc(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    header_comment: Option<&str>,
) -> Doc {
    render_node(node, options, false, Context::Block, header_comment)
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

/// Where a node is printed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Outside a flow collection.
    Block,
    /// In a flow collection: a key, or the value of a flow mapping.
    Flow,
    /// An item of a flow sequence.
    FlowSeqItem,
}

/// A printed scalar and how it reads back: a plain scalar takes its tag from
/// the Core schema, any quoted or block scalar is a string.
enum Printed {
    Plain(Doc),
    Quoted(Doc),
}

impl Printed {
    /// A string value printed by `string_to_doc`: plain unless `single_quote`
    /// quotes it (a key is never quoted by that preference alone).
    fn of_string(value: &str, options: &YamlFormatOptions, in_key: bool) -> Self {
        let doc = string_to_doc(value, options, in_key);
        if options.single_quote && !in_key {
            Self::Quoted(doc)
        } else {
            Self::Plain(doc)
        }
    }

    /// The tag the printed scalar resolves to without a tag of its own.
    fn resolved_tag(&self, value: &str) -> &'static str {
        match self {
            Self::Plain(_) => plain_core_tag(value),
            Self::Quoted(_) => Some(ResolvedTag::Str),
        }
        .map_or("", ResolvedTag::as_str)
    }

    fn into_doc(self) -> Doc {
        match self {
            Self::Plain(doc) | Self::Quoted(doc) => doc,
        }
    }
}

/// Render `node`; `context` tells where it sits.  In a flow collection `,` `[`
/// `]` `{` `}` end a plain scalar (`ns-plain-char(c)` [130] admits only
/// `ns-plain-safe(c)` [127], which is `ns-plain-safe-in` [129] = `ns-char -
/// c-flow-indicator` in flow context), so a scalar holding one of them is
/// emitted quoted.
#[expect(
    clippy::too_many_lines,
    reason = "comprehensive match over all node variants"
)]
fn render_node(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    in_key: bool,
    context: Context,
    header_comment: Option<&str>,
) -> Doc {
    let in_flow = context != Context::Block;
    let quote_empty = context == Context::FlowSeqItem;
    match node {
        Node::Scalar {
            value, style, tag, ..
        } => {
            let scalar_doc = match style {
                ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => {
                    // A value with an over-indented blank line cannot be printed as a
                    // block scalar (see `block_value_needs_quoting`).
                    if block_value_needs_quoting(value) {
                        Printed::Quoted(text(format!("\"{}\"", escape_double_quoted(value))))
                    } else {
                        Printed::Quoted(repr_block_to_doc(
                            value,
                            *style,
                            options.indent_width(),
                            header_comment,
                        ))
                    }
                }
                ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                    if requires_double_quoting(value) {
                        // Decoded value contains chars that cannot appear unquoted
                        // or in single-quoted scalars (control chars, backslash,
                        // etc.) — always re-emit as double-quoted with proper
                        // escaping regardless of original style.
                        Printed::Quoted(text(format!("\"{}\"", escape_double_quoted(value))))
                    } else if needs_quoting(value, options.yaml_version) {
                        if matches!(style, ScalarStyle::DoubleQuoted) {
                            Printed::Quoted(text(format!("\"{}\"", escape_double_quoted(value))))
                        } else {
                            // Single-quoted: escape embedded single quotes as ''.
                            Printed::Quoted(text(format!("'{}'", value.replace('\'', "''"))))
                        }
                    } else if options.preserve_quotes {
                        // Safe-plain scalar: reproduce the source quote style
                        // instead of stripping to plain.
                        if matches!(style, ScalarStyle::DoubleQuoted) {
                            Printed::Quoted(text(format!("\"{}\"", escape_double_quoted(value))))
                        } else {
                            Printed::Quoted(text(format!("'{}'", value.replace('\'', "''"))))
                        }
                    } else if in_flow && needs_flow_quoting(value) {
                        // Stripping the quotes would leave a flow indicator in a plain scalar.
                        Printed::Quoted(flow_quoted_to_doc(value, options, in_key))
                    } else {
                        Printed::of_string(value, options, in_key)
                    }
                }
                ScalarStyle::Plain => {
                    // Values that contain characters which cannot appear in a plain scalar
                    // at all — control characters, backslashes, or embedded newlines —
                    // must be emitted as double-quoted with proper escaping.  So
                    // must a root scalar that the parser reads from the `---` line
                    // and that reads as something else on a line of its own
                    // (`--- ... x`, `--- - a`).
                    if quote_empty && value.is_empty() {
                        Printed::Quoted(text("\"\""))
                    } else if requires_double_quoting(value)
                        || reads_as_another_node_on_its_own_line(value)
                    {
                        Printed::Quoted(text(format!("\"{}\"", escape_double_quoted(value))))
                    } else if in_flow && (needs_flow_quoting(value) || value.starts_with(':')) {
                        // A plain scalar may start with `:` in a flow collection, but
                        // the parser reads `{ : :c }` as the value `c`.
                        Printed::Quoted(flow_quoted_to_doc(value, options, in_key))
                    } else if value.ends_with(':') {
                        // A flow collection may hold `v:`; on a line of its own it is
                        // a key (`? v:` reads as the key `v`).
                        Printed::Quoted(string_to_doc(value, options, in_key))
                    } else if needs_quoting(value, options.yaml_version) {
                        // Value needs quoting (reserved keyword, special char, etc.) but
                        // was originally plain — preserve plain style so round-trip matches.
                        Printed::Plain(text(value.clone()))
                    } else {
                        Printed::of_string(value, options, in_key)
                    }
                }
            };

            // Prefix with a tag if present.
            //
            // Core schema tags (`tag:yaml.org,2002:*`) are handled as follows:
            //
            // - **Resolver-injected** (`tag_loc: None`): always stripped — the resolver
            //   injects these automatically and re-emitting them breaks idempotency.
            //
            // - **User-authored on a non-empty scalar** (`tag_loc: Some`, `value` non-empty):
            //   stripped when the printed scalar resolves to the same tag without it
            //   (`!!str hello`), emitted in short form when it does not (`!!str 123`
            //   prints a plain scalar that is an int, `!!int "5"` prints a string).
            //
            // - **User-authored on an empty scalar** (`tag_loc: Some`, `value` empty):
            //   emitted in short form (`!!str`, `!!null`, etc.) — the tag carries
            //   semantic meaning that cannot be inferred from an absent value.
            //
            // Non-core tags (user tags) are always emitted as-is.
            let tag_loc_is_some = node.tag_loc().is_some();
            // The output has no text for the value of an empty scalar, except in
            // a flow sequence, where it is `""`.
            // A block scalar keeps its header, which holds the comment of its line,
            // and a quoted one its quotes: a comment after properties alone reads
            // back as a comment above the node.
            let value_absent =
                value.is_empty() && !quote_empty && matches!(style, ScalarStyle::Plain);
            // A tag follows `&anchor ` and precedes the value, so it ends in a
            // space unless the value is absent.
            let tag_end = if value_absent { "" } else { " " };
            let tag_prefix = tag.as_ref().and_then(|t| {
                if is_core_schema_tag(t) {
                    let suffix = t.trim_start_matches(CORE_TAG_PREFIX);
                    // `""` in a flow sequence reads as a string, so the null of an
                    // empty plain scalar needs its tag there.
                    let empty_null_item =
                        value.is_empty() && quote_empty && matches!(style, ScalarStyle::Plain);
                    if value.is_empty() && (tag_loc_is_some || empty_null_item) {
                        // Explicit core tag on empty scalar: emit in short form.
                        Some(format!("!!{suffix}{tag_end}"))
                    } else if tag_loc_is_some && scalar_doc.resolved_tag(value) != t.as_ref() {
                        // The printed scalar would not resolve to the tag: keep the tag.
                        Some(format!("!!{suffix} "))
                    } else {
                        // Resolver-injected, or user-authored and implied by the value.
                        None
                    }
                } else {
                    Some(format!("{}{tag_end}", format_tag(t)))
                }
            });
            let scalar_doc = scalar_doc.into_doc();

            // With an anchor alone, nothing follows the name.
            let anchor_alone =
                value_absent && tag_prefix.is_none() && matches!(style, ScalarStyle::Plain);
            let doc = match tag_prefix {
                // The prefix is all there is to print for an absent value.
                Some(prefix) if value_absent => text(prefix),
                Some(prefix) => concat(vec![text(prefix), scalar_doc]),
                None => scalar_doc,
            };

            match node.anchor() {
                // Anything after the anchor needs a space.
                Some(name) if anchor_alone => concat(vec![text(format!("&{name}")), doc]),
                Some(name) => concat(vec![text(format!("&{name} ")), doc]),
                None => doc,
            }
        }

        Node::Mapping {
            entries,
            style,
            tag,
            ..
        } => {
            let lead = Lead {
                properties: mapping_render::collection_properties(node.anchor(), tag.as_deref())
                    .is_some(),
                value_indicator: false,
            };
            let Entry { above, doc } =
                mapping_render::mapping_to_parts(entries, *style, options, lead);
            // An empty mapping prints `{}` on the line of its properties.
            let effective_style = if entries.is_empty() {
                CollectionStyle::Flow
            } else if options.format_enforce_block_style {
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
            // An empty sequence prints `[]` on the line of its properties.
            let effective_style = if items.is_empty() {
                CollectionStyle::Flow
            } else if options.format_enforce_block_style {
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
    render_node(node, options, in_key, Context::Flow, None)
}

/// Emit a node for use as an item of a flow sequence.  An item that has no text
/// of its own (an empty scalar) prints `""`: nothing may stand between two
/// commas (`[a, , b]` is an error), and properties without a node (`[&a ]`)
/// load as an empty scalar, so the explicit empty string keeps the item visible
/// and spells its null tag out (`[&a !!null ""]`).  The shorter `[&a]` would
/// load to the same data; the longer form is kept so that the output does not
/// depend on how a parser reads properties alone.
pub(super) fn flow_seq_item_to_doc(node: &Node<Span>, options: &YamlFormatOptions) -> Doc {
    render_node(node, options, false, Context::FlowSeqItem, None)
}
