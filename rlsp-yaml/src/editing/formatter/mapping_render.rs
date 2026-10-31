// SPDX-License-Identifier: MIT

use rlsp_fmt::{Doc, concat, flat_alt, group, hard_line, indent, join, line, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{ScalarStyle, Span};

use super::options::YamlFormatOptions;
use super::scalar_render::{format_tag, is_core_schema_tag};

/// Join a collection's anchor and user-defined tag into `&anchor !tag`, or just
/// one of them. Core schema tags are dropped. `None` when neither remains.
fn collection_properties(anchor: Option<&str>, tag: Option<&str>) -> Option<String> {
    let tag_prefix = tag.and_then(|t| {
        if is_core_schema_tag(t) {
            None
        } else {
            Some(format_tag(t))
        }
    });

    match (anchor, tag_prefix.as_deref()) {
        (Some(name), Some(t)) => Some(format!("&{name} {t}")),
        (Some(name), None) => Some(format!("&{name}")),
        (None, Some(t)) => Some(t.to_string()),
        (None, None) => None,
    }
}

/// The colon that ends a key whose value is a block collection, followed by the
/// collection's properties on the same line: `: &anchor !tag`, or `bare_colon`
/// (`:` or ` :`) when the collection carries none.
fn collection_colon(bare_colon: &str, anchor: Option<&str>, tag: Option<&str>) -> Doc {
    collection_properties(anchor, tag)
        .map_or_else(|| text(bare_colon), |props| text(format!(": {props}")))
}

/// The trailing comment to print after a node's rendered Doc.
///
/// A block scalar's trailing comment sits on its header line (`| # c`), not
/// after its content; printing it after the content would put it inside the
/// scalar's value.  The formatter does not print the header comment yet.
pub(super) fn printed_trailing_comment(node: &Node<Span>) -> Option<&str> {
    let is_block_scalar = matches!(
        node,
        Node::Scalar {
            style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
            ..
        }
    );
    if is_block_scalar {
        None
    } else {
        node.trailing_comment()
    }
}

/// Attach a node's comments to its rendered Doc: the trailing comment two spaces
/// after the Doc, each leading comment on its own line above it.
pub(super) fn with_comments(doc: Doc, leading: &[String], trailing: Option<&str>) -> Doc {
    let doc = match trailing {
        Some(tc) => concat(vec![doc, text(format!("  {tc}"))]),
        None => doc,
    };
    if leading.is_empty() {
        return doc;
    }
    let mut parts: Vec<Doc> = leading
        .iter()
        .flat_map(|lc| [text(lc.clone()), hard_line()])
        .collect();
    parts.push(doc);
    concat(parts)
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
pub(super) fn prepend_collection_properties(
    doc: Doc,
    anchor: Option<&str>,
    tag: Option<&str>,
    style: CollectionStyle,
) -> Doc {
    let props = collection_properties(anchor, tag);

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

/// Convert a YAML mapping to Doc, branching on block vs flow style.
pub(super) fn mapping_to_doc(
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
pub(super) fn flow_mapping_to_doc(
    entries: &[(Node<Span>, Node<Span>)],
    options: &YamlFormatOptions,
) -> Doc {
    // The padding only exists on one line; a broken mapping has none, so no line
    // ends in a space and the closing brace sits at the entries' column.
    let pad = if options.bracket_spacing { " " } else { "" };

    let items: Vec<Doc> = entries
        .iter()
        .map(|(key, value)| {
            let key_doc = super::node_to_doc::flow_item_to_doc(key, options, true);
            let val_doc = super::node_to_doc::flow_item_to_doc(value, options, false);
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
        text("{"),
        indent(concat(vec![
            flat_alt(text(pad), line()),
            inner,
            flat_alt(text(pad), line()),
            text("}"),
        ])),
    ]))
}

/// Returns `true` when a mapping key requires the explicit `? key` form.
///
/// Explicit key syntax is required when the key cannot appear as a plain scalar
/// before `: ` — specifically when the key is:
/// - a non-empty collection (mapping or sequence) of any style
/// - a block scalar (literal `|` or folded `>`), whose multi-line representation
///   cannot fit before a `: ` on the same line
///
/// Empty flow collections (`[]`, `{}`) are the one exception: they always render
/// as single-character tokens and are safe as inline implicit keys.
///
/// Non-empty flow collections (`[a, b]`, `{k: v}`) require explicit key form even
/// though they are single-line, because using them as implicit keys in a block
/// mapping can cause re-parsing ambiguity (the YAML parser may confuse them with
/// sequence or mapping indicators in the surrounding context).
///
/// An empty scalar key is handled separately (emitted as `: value` with no `?`).
pub(super) const fn needs_explicit_key(key: &Node<Span>) -> bool {
    match key {
        // Empty flow collections are safe as inline implicit keys.
        Node::Mapping { entries, .. } if entries.is_empty() => false,
        Node::Sequence { items, .. } if items.is_empty() => false,
        // Plain/quoted scalars and aliases are safe as inline implicit keys.
        Node::Scalar {
            style: ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted,
            ..
        }
        | Node::Alias { .. } => false,
        // Non-empty collections and block scalars require explicit key form.
        Node::Mapping { .. }
        | Node::Sequence { .. }
        | Node::Scalar {
            style: ScalarStyle::Literal(_) | ScalarStyle::Folded(_),
            ..
        } => true,
    }
}

/// Returns `true` when a mapping key is an effectively-untagged empty scalar
/// (the implicit empty key `:`).
///
/// An empty scalar with a **resolver-injected** core schema tag (`tag_loc: None`)
/// is treated as an empty key because the tag will not be emitted — resolvers inject
/// these automatically and the formatter suppresses them to maintain idempotency.
///
/// An empty scalar with a **user-authored** explicit tag (`tag_loc: Some(_)`) is
/// **not** an empty key — the tag carries semantic meaning and must be emitted, so it
/// routes through the normal key path.
pub(super) fn is_empty_key(key: &Node<Span>) -> bool {
    match key {
        Node::Scalar {
            value, tag: None, ..
        } if value.is_empty() => true,
        Node::Scalar {
            value,
            tag: Some(t),
            ..
        } if value.is_empty() && is_core_schema_tag(t) && key.tag_loc().is_none() => true,
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            false
        }
    }
}

/// Returns `true` when a mapping key requires a space before the `:` separator.
///
/// Two key forms need ` : ` rather than `: `:
///
/// 1. **Tagged empty scalar** (`!!null`, `!mytag`, etc.) where the tag will actually
///    be emitted — the rendered key ends with a tag; `:` is a valid URI character,
///    so `!!null:` would be parsed as tag `tag:yaml.org,2002:null:` rather than key
///    `!!null` + separator.  Resolver-injected core tags are suppressed (not emitted),
///    so they do not need the extra space.
///
/// 2. **Alias** (`*name`) — `*name:` is parsed as alias name `name:`, breaking
///    idempotency. A space before `:` keeps the alias name and separator distinct.
pub(super) fn key_needs_space_before_colon(key: &Node<Span>) -> bool {
    match key {
        Node::Scalar {
            value,
            tag: Some(t),
            ..
        } if value.is_empty() => {
            // Only need a space if the tag will actually be emitted.
            // Resolver-injected core tags are suppressed → no space needed.
            !(is_core_schema_tag(t) && key.tag_loc().is_none())
        }
        Node::Alias { .. } => true,
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } => false,
    }
}

/// Render a mapping entry that uses explicit key form: `? key\n: value`.
///
/// This form is required when the key is a block scalar, block sequence, or
/// block mapping — types that cannot appear inline before `: `.
pub(super) fn explicit_key_to_doc(
    key: &Node<Span>,
    value: &Node<Span>,
    options: &YamlFormatOptions,
) -> Doc {
    let key_doc = super::node_to_doc::node_to_doc(key, options, true);
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
                tag,
                ..
            } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let colon_prefix = collection_colon(":", value.anchor(), tag.as_deref());
                concat(vec![
                    colon_prefix,
                    indent(concat(vec![
                        hard_line(),
                        mapping_to_doc(entries, *style, options),
                    ])),
                ])
            }
            // Block sequence value: `:\n  - item` (or `:\n- item` when indentless).
            Node::Sequence {
                items, style, tag, ..
            } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let colon_prefix = collection_colon(":", value.anchor(), tag.as_deref());
                let seq_doc = super::sequence_render::sequence_to_doc(items, *style, options);
                if options.format_indent_sequences {
                    concat(vec![
                        colon_prefix,
                        indent(concat(vec![hard_line(), seq_doc])),
                    ])
                } else {
                    concat(vec![colon_prefix, hard_line(), seq_doc])
                }
            }
            // Inline value (scalar, flow collection, empty collection, alias).
            Node::Scalar { .. }
            | Node::Mapping { .. }
            | Node::Sequence { .. }
            | Node::Alias { .. } => {
                let value_doc = super::node_to_doc::node_to_doc(value, options, false);
                concat(vec![text(": "), value_doc])
            }
        }
    };

    // Append trailing comment from the value node.
    let colon_line = with_comments(colon_line, &[], printed_trailing_comment(value));

    concat(vec![question_line, hard_line(), colon_line])
}

/// Convert a single key-value pair to Doc, including any AST-attached comments.
pub(super) fn key_value_to_doc(
    key: &Node<Span>,
    value: &Node<Span>,
    options: &YamlFormatOptions,
) -> Doc {
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
        let value_doc = super::node_to_doc::node_to_doc(value, options, false);
        if matches!(value, Node::Scalar { value, .. } if value.is_empty()) {
            text(":")
        } else {
            concat(vec![text(": "), value_doc])
        }
    } else {
        let key_doc = super::node_to_doc::node_to_doc(key, options, true);
        match value {
            // Block mappings: `key:\n  child: val` — hard_line inside indent.
            // With anchor: `key: &anchor\n  child: val`.
            // With tag: `key: !tag\n  child: val` (anchor before tag per formatter convention).
            Node::Mapping {
                entries,
                style,
                tag,
                ..
            } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let bare_colon = if key_needs_space_before_colon(key) {
                    " :"
                } else {
                    ":"
                };
                let colon = collection_colon(bare_colon, value.anchor(), tag.as_deref());
                concat(vec![
                    key_doc,
                    colon,
                    indent(concat(vec![
                        hard_line(),
                        mapping_to_doc(entries, *style, options),
                    ])),
                ])
            }
            // Block sequences: block items under key, indented or indentless.
            // With anchor: `key: &anchor\n  - item` (or `key: &anchor\n- item`).
            // With tag: `key: !tag\n  - item` (anchor before tag per formatter convention).
            Node::Sequence {
                items, style, tag, ..
            } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let bare_colon = if key_needs_space_before_colon(key) {
                    " :"
                } else {
                    ":"
                };
                let colon = collection_colon(bare_colon, value.anchor(), tag.as_deref());
                let seq_doc = super::sequence_render::sequence_to_doc(items, *style, options);
                if options.format_indent_sequences {
                    concat(vec![
                        key_doc,
                        colon,
                        indent(concat(vec![hard_line(), seq_doc])),
                    ])
                } else {
                    concat(vec![key_doc, colon, hard_line(), seq_doc])
                }
            }
            // Flow collections, scalars, empty collections, aliases — all inline.
            Node::Scalar { .. }
            | Node::Mapping { .. }
            | Node::Sequence { .. }
            | Node::Alias { .. } => {
                let value_doc = super::node_to_doc::node_to_doc(value, options, false);
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

    // The value's trailing comment is appended only on the implicit-key path —
    // explicit_key_to_doc handles its own.
    let trailing = if !needs_explicit_key(key) && !is_empty_key(key) {
        printed_trailing_comment(value)
    } else {
        None
    };
    with_comments(pair_doc, key.leading_comments(), trailing)
}
