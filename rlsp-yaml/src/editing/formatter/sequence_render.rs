// SPDX-License-Identifier: MIT

use rlsp_fmt::{Doc, concat, flat_alt, group, hard_line, indent, join, line, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;

use super::mapping_render::{
    Entry, block_mapping_parts, join_entries, line_end_comment, with_comments_above,
    with_line_end_comment,
};
use super::node_to_doc::renders_empty;
use super::options::YamlFormatOptions;
use super::scalar_render::{format_tag, is_core_schema_tag};

/// Convert a YAML sequence to Doc, branching on block vs flow style.
pub(super) fn sequence_to_doc(
    seq: &[Node<Span>],
    style: CollectionStyle,
    options: &YamlFormatOptions,
) -> Doc {
    let Entry { above, doc } = sequence_to_parts(seq, style, options);
    with_comments_above(doc, above)
}

/// Convert a YAML sequence to its Doc and the comments above its first item, for
/// a caller that puts the sequence's properties before the first item (see
/// `mapping_render::mapping_to_parts`).
pub(super) fn sequence_to_parts<'a>(
    seq: &'a [Node<Span>],
    style: CollectionStyle,
    options: &YamlFormatOptions,
) -> Entry<'a> {
    if seq.is_empty() {
        return Entry {
            above: Vec::new(),
            doc: text("[]"),
        };
    }

    let effective_style = if options.format_enforce_block_style {
        CollectionStyle::Block
    } else {
        style
    };

    match effective_style {
        CollectionStyle::Flow => Entry {
            above: Vec::new(),
            doc: flow_sequence_to_doc(seq, options),
        },
        CollectionStyle::Block => block_sequence_parts(seq, options),
    }
}

/// Convert the items of a block sequence to their Doc and the comments above the
/// first one, for a caller that puts text before the first `- ` (see
/// `mapping_render::block_mapping_parts`).
pub(super) fn block_sequence_parts<'a>(
    seq: &'a [Node<Span>],
    options: &YamlFormatOptions,
) -> Entry<'a> {
    join_entries(
        seq.iter().map(|item| sequence_item_parts(item, options)),
        "[]",
    )
}

/// Render a flow sequence as `[item1, item2, item3]`. Uses `group()` so the
/// printer keeps it on one line when it fits within `print_width`, and breaks it
/// across lines (one item per line, indented) when it does not.
pub(super) fn flow_sequence_to_doc(seq: &[Node<Span>], options: &YamlFormatOptions) -> Doc {
    let items: Vec<Doc> = seq
        .iter()
        .map(|item| super::node_to_doc::flow_seq_item_to_doc(item, options))
        .collect();
    let sep = concat(vec![text(","), line()]);
    let inner = join(&sep, items);

    group(concat(vec![
        text("["),
        indent(concat(vec![
            flat_alt(text(""), line()),
            inner,
            flat_alt(text(""), line()),
            text("]"),
        ])),
    ]))
}

/// Render a single sequence item with its `- ` prefix and the comments above it.
///
/// A comment on the `- ` line of an item that is a block collection reads as the
/// collection's own comment, so every comment of such an item prints above the
/// `- `: the item's, and those of its first entry.
fn sequence_item_parts<'a>(item: &'a Node<Span>, options: &YamlFormatOptions) -> Entry<'a> {
    let effective_style = |style: CollectionStyle| {
        if options.format_enforce_block_style {
            CollectionStyle::Block
        } else {
            style
        }
    };

    let mut above: Vec<&str> = item.leading_comments().iter().map(String::as_str).collect();

    let (item_doc, line_end) = match item {
        // Block mapping item: `- key: val\n  key2: val2`.
        // With anchor: `- &anchor\n  key: val\n  key2: val2`.
        // With tag: `- !tag\n  key: val` (anchor before tag per formatter convention).
        Node::Mapping {
            entries,
            style,
            tag,
            ..
        } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
            above.extend(item.trailing_comment());
            let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
            let prefix = match (item.anchor(), user_tag) {
                (Some(name), Some(t)) => format!("&{name} {}", format_tag(t)),
                (Some(name), None) => format!("&{name}"),
                (None, Some(t)) => format_tag(t),
                (None, None) => String::new(),
            };
            let Entry {
                above: first_above,
                doc: inner,
            } = block_mapping_parts(entries, options, !prefix.is_empty());
            above.extend(first_above);
            let doc = if prefix.is_empty() {
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
            };
            (doc, None)
        }
        // Block sequence item: `- \n  - item`.
        // With anchor: `- &anchor\n  - item`.
        // With tag: `- !tag\n  - item` (anchor before tag per formatter convention).
        Node::Sequence {
            items, style, tag, ..
        } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
            above.extend(item.trailing_comment());
            let Entry {
                above: first_above,
                doc: inner,
            } = block_sequence_parts(items, options);
            above.extend(first_above);
            let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
            let prefix_doc = match (item.anchor(), user_tag) {
                (Some(name), Some(t)) => text(format!("&{name} {}", format_tag(t))),
                (Some(name), None) => text(format!("&{name}")),
                (None, Some(t)) => text(format_tag(t)),
                (None, None) => text(String::new()),
            };
            let doc = concat(vec![
                text("- "),
                prefix_doc,
                indent(concat(vec![hard_line(), inner])),
            ]);
            (doc, None)
        }
        // Flow collections, scalars, empty collections, aliases — inline under `- `.
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            let comment = line_end_comment(item);
            // An item that prints nothing leaves the dash's space at the end of
            // the line, so it is left out before a comment.
            let dash = if renders_empty(item) && comment.is_some() {
                "-"
            } else {
                "- "
            };
            (
                concat(vec![
                    text(dash),
                    super::node_to_doc::node_to_doc(item, options, false),
                ]),
                comment,
            )
        }
    };

    Entry {
        above,
        doc: with_line_end_comment(item_doc, line_end),
    }
}
