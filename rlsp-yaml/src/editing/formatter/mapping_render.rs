// SPDX-License-Identifier: MIT

use std::iter;

use rlsp_fmt::{Doc, concat, flat_alt, group, hard_line, indent, join, line, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{ScalarStyle, Span};

use super::node_to_doc::{prints_block_header, renders_empty, value_to_doc};
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

/// The comment to print at the end of the line a node is printed on: its
/// trailing comment, unless the node is a block scalar whose header line carries
/// it.  After a block scalar's content it would be part of the value.
pub(super) fn line_end_comment(node: &Node<Span>) -> Option<&str> {
    if prints_block_header(node) {
        None
    } else {
        node.trailing_comment()
    }
}

/// A comment at the end of a line: two spaces, then the comment.  Nothing when
/// there is none.
pub(super) fn trailing_comment_doc(comment: Option<&str>) -> Doc {
    text(comment.map_or_else(String::new, |c| format!("  {c}")))
}

/// Append a comment to the end of the last line of `doc`.
pub(super) fn with_line_end_comment(doc: Doc, comment: Option<&str>) -> Doc {
    concat(vec![doc, trailing_comment_doc(comment)])
}

/// Print each comment on its own line above `doc`.
pub(super) fn with_comments_above<'a>(
    doc: Doc,
    comments: impl IntoIterator<Item = &'a str>,
) -> Doc {
    let mut parts: Vec<Doc> = comments
        .into_iter()
        .flat_map(|c| [text(c.to_string()), hard_line()])
        .collect();
    if parts.is_empty() {
        return doc;
    }
    parts.push(doc);
    concat(parts)
}

/// Where the comments of a mapping entry print.  The entry's value line has one
/// slot for a comment.  A comment that stays on its line takes it; the other
/// moves to its own line above the entry.
struct EntryComments<'a> {
    above: Option<&'a str>,
    slot: Option<&'a str>,
}

/// Place the comment on the key's line and the comment on the value's line.
/// Each stays on its line when the output has that line and a slot on it: the
/// value line of the entry.  A key comment shares that line with the value; when
/// the value has a comment of its own, the key comment moves above the entry.
fn place_entry_comments<'a>(
    key_comment: Option<&'a str>,
    value_comment: Option<&'a str>,
) -> EntryComments<'a> {
    match (key_comment, value_comment) {
        (Some(key), Some(_)) => EntryComments {
            above: Some(key),
            slot: value_comment,
        },
        (key, value) => EntryComments {
            above: None,
            slot: key.or(value),
        },
    }
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
            let Entry { above, doc } = block_mapping_parts(entries, options);
            with_comments_above(doc, above)
        }
    }
}

/// Convert the entries of a block mapping to their Doc and the comments above
/// the first one.  A caller that puts text before the first entry (`- ` or `? `)
/// prints these above that text: between the text and the entry, they would be
/// read as the comments of the collection.
pub(super) fn block_mapping_parts<'a>(
    entries: &'a [(Node<Span>, Node<Span>)],
    options: &YamlFormatOptions,
) -> Entry<'a> {
    join_entries(
        entries
            .iter()
            .map(|(key, value)| entry_to_parts(key, value, options)),
        "{}",
    )
}

/// Join the Docs of the entries of a block collection, one per line, and take
/// the comments above the first one apart.  `empty` is printed for no entry.
pub(super) fn join_entries<'a>(
    mut entries: impl Iterator<Item = Entry<'a>>,
    empty: &str,
) -> Entry<'a> {
    let Some(Entry { above, doc: first }) = entries.next() else {
        return Entry {
            above: Vec::new(),
            doc: text(empty),
        };
    };
    let rest = entries.map(|entry| with_comments_above(entry.doc, entry.above));
    Entry {
        above,
        doc: join(&hard_line(), iter::once(first).chain(rest).collect()),
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
/// block mapping — types that cannot appear inline before `: `.  The comment on
/// the key's line goes after the key, the comment on the value's line after the
/// value: each line has its own slot.
///
/// The comments above the key's first entry, when the key is a block collection,
/// are returned with the entry's Doc to print above the `?`.
pub(super) fn explicit_key_to_doc<'a>(
    key: &'a Node<Span>,
    value: &Node<Span>,
    options: &YamlFormatOptions,
) -> Entry<'a> {
    let Entry {
        above,
        doc: key_doc,
    } = explicit_key_parts(key, options);
    let value_is_empty = matches!(value, Node::Scalar { value, .. } if value.is_empty());

    // `? key_doc` — the key part.
    // For block scalars/collections as keys, the key_doc spans multiple lines.
    // We render `?` + space + key indented by 2 spaces.
    let question_line = with_line_end_comment(
        concat(vec![text("? "), indent(key_doc)]),
        line_end_comment(key),
    );

    // `: value_doc` — the value part.
    let value_comment = line_end_comment(value);
    let colon_line = if value_is_empty {
        // Set-like entry or empty value: emit bare `:` with no trailing space.
        concat(vec![text(":"), trailing_comment_doc(value_comment)])
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
                    trailing_comment_doc(value_comment),
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
                let colon_line = concat(vec![colon_prefix, trailing_comment_doc(value_comment)]);
                if options.format_indent_sequences {
                    concat(vec![colon_line, indent(concat(vec![hard_line(), seq_doc]))])
                } else {
                    concat(vec![colon_line, hard_line(), seq_doc])
                }
            }
            // Inline value (scalar, flow collection, empty collection, alias).
            Node::Scalar { .. }
            | Node::Mapping { .. }
            | Node::Sequence { .. }
            | Node::Alias { .. } => {
                let value_doc = super::node_to_doc::node_to_doc(value, options, false);
                concat(vec![
                    text(": "),
                    value_doc,
                    trailing_comment_doc(value_comment),
                ])
            }
        }
    };

    Entry {
        above,
        doc: concat(vec![question_line, hard_line(), colon_line]),
    }
}

/// Render the key of an explicit key entry.  A block collection key starts on
/// the `? ` line, so the comments above its first entry come back apart from the
/// Doc.
fn explicit_key_parts<'a>(key: &'a Node<Span>, options: &YamlFormatOptions) -> Entry<'a> {
    let is_block = |style: CollectionStyle| {
        options.format_enforce_block_style || style == CollectionStyle::Block
    };
    match key {
        Node::Mapping {
            entries,
            style,
            tag,
            ..
        } if !entries.is_empty() && is_block(*style) => {
            let Entry { above, doc } = block_mapping_parts(entries, options);
            Entry {
                above,
                doc: prepend_collection_properties(
                    doc,
                    key.anchor(),
                    tag.as_deref(),
                    CollectionStyle::Block,
                ),
            }
        }
        Node::Sequence {
            items, style, tag, ..
        } if !items.is_empty() && is_block(*style) => {
            let Entry { above, doc } = super::sequence_render::block_sequence_parts(items, options);
            Entry {
                above,
                doc: prepend_collection_properties(
                    doc,
                    key.anchor(),
                    tag.as_deref(),
                    CollectionStyle::Block,
                ),
            }
        }
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            Entry {
                above: Vec::new(),
                doc: super::node_to_doc::node_to_doc(key, options, true),
            }
        }
    }
}

/// A mapping entry's Doc and the comments that print on their own lines above it.
pub(super) struct Entry<'a> {
    pub(super) above: Vec<&'a str>,
    pub(super) doc: Doc,
}

/// Convert a single key-value pair to its Doc and the comments above it: the
/// key's leading comments, then the key's own-line comment when the value's
/// line has a comment of its own.  A caller that puts text before the entry
/// (the `- ` of a sequence item) prints these above that text.
pub(super) fn entry_to_parts<'a>(
    key: &'a Node<Span>,
    value: &'a Node<Span>,
    options: &YamlFormatOptions,
) -> Entry<'a> {
    let effective_style = |style: CollectionStyle| {
        if options.format_enforce_block_style {
            CollectionStyle::Block
        } else {
            style
        }
    };

    let mut above: Vec<&str> = key.leading_comments().iter().map(String::as_str).collect();

    // Dispatch to explicit key form when the key type requires it.
    if needs_explicit_key(key) {
        let explicit = explicit_key_to_doc(key, value, options);
        above.extend(explicit.above);
        return Entry {
            above,
            doc: explicit.doc,
        };
    }

    let comments = place_entry_comments(key.trailing_comment(), value.trailing_comment());
    above.extend(comments.above);
    // A block scalar value carries the slot's comment on its header line, any
    // other value after the line the entry ends on.
    let (header_comment, line_end) = if prints_block_header(value) {
        (comments.slot, None)
    } else {
        (None, comments.slot)
    };

    // An empty key (`: value`) has no `?` prefix and no key text; the rest of the
    // entry is the same as with any other key.
    let empty_key = is_empty_key(key);
    let key_doc = if empty_key {
        text(String::new())
    } else {
        super::node_to_doc::node_to_doc(key, options, true)
    };
    let bare_colon = if key_needs_space_before_colon(key) {
        " :"
    } else {
        ":"
    };
    let doc = match value {
        // Block mappings: `key:\n  child: val` — hard_line inside indent.
        // With anchor: `key: &anchor\n  child: val`.
        // With tag: `key: !tag\n  child: val` (anchor before tag per formatter convention).
        Node::Mapping {
            entries,
            style,
            tag,
            ..
        } if !entries.is_empty() && effective_style(*style) == CollectionStyle::Block => {
            let colon = collection_colon(bare_colon, value.anchor(), tag.as_deref());
            concat(vec![
                key_doc,
                colon,
                trailing_comment_doc(line_end),
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
            let colon = collection_colon(bare_colon, value.anchor(), tag.as_deref());
            let seq_doc = super::sequence_render::sequence_to_doc(items, *style, options);
            let key_line = concat(vec![key_doc, colon, trailing_comment_doc(line_end)]);
            if options.format_indent_sequences {
                concat(vec![key_line, indent(concat(vec![hard_line(), seq_doc]))])
            } else {
                concat(vec![key_line, hard_line(), seq_doc])
            }
        }
        // Flow collections, scalars, empty collections, aliases — all inline.
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            let value_doc = value_to_doc(value, options, header_comment);
            // When the key's rendered form ends with a tag, a space before `:` is
            // required to prevent the colon from being parsed as part of the tag URI.
            // A value that prints nothing leaves the separator's space at the end
            // of the line, so it is left out before a comment and after an
            // empty key.
            let sep = match (
                key_needs_space_before_colon(key),
                renders_empty(value) && (line_end.is_some() || empty_key),
            ) {
                (true, true) => text(" :"),
                (true, false) => text(" : "),
                (false, true) => text(":"),
                (false, false) => text(": "),
            };
            concat(vec![
                key_doc,
                sep,
                value_doc,
                trailing_comment_doc(line_end),
            ])
        }
    };

    Entry { above, doc }
}
