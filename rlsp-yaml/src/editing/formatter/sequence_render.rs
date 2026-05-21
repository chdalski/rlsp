// SPDX-License-Identifier: MIT

use rlsp_fmt::{Doc, concat, flat_alt, group, hard_line, indent, join, line, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;

use super::options::YamlFormatOptions;
use super::scalar_render::{format_tag, is_core_schema_tag};

/// Convert a YAML sequence to Doc, branching on block vs flow style.
pub(super) fn sequence_to_doc(
    seq: &[Node<Span>],
    style: CollectionStyle,
    options: &YamlFormatOptions,
) -> Doc {
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
pub(super) fn flow_sequence_to_doc(seq: &[Node<Span>], options: &YamlFormatOptions) -> Doc {
    let items: Vec<Doc> = seq
        .iter()
        .map(|item| super::flow_item_to_doc(item, options, false))
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
pub(super) fn sequence_item_to_doc(item: &Node<Span>, options: &YamlFormatOptions) -> Doc {
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
            tag,
            ..
        } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
            let pairs: Vec<Doc> = entries
                .iter()
                .map(|(k, v)| super::mapping_render::key_value_to_doc(k, v, options))
                .collect();
            let inner = join(&hard_line(), pairs);
            let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
            let prefix = match (item.anchor(), user_tag) {
                (Some(name), Some(t)) => format!("&{name} {}", format_tag(t)),
                (Some(name), None) => format!("&{name}"),
                (None, Some(t)) => format_tag(t),
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
            items, style, tag, ..
        } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
            let user_tag = tag.as_ref().filter(|t| !is_core_schema_tag(t));
            let prefix_doc = match (item.anchor(), user_tag) {
                (Some(name), Some(t)) => text(format!("&{name} {}", format_tag(t))),
                (Some(name), None) => text(format!("&{name}")),
                (None, Some(t)) => text(format_tag(t)),
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
            concat(vec![text("- "), super::node_to_doc(item, options, false)])
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
