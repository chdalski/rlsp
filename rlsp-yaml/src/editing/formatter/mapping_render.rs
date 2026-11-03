// SPDX-License-Identifier: MIT

use std::borrow::Cow;
use std::iter;

use rlsp_fmt::{Doc, concat, flat_alt, group, hard_line, indent, join, line, text};
use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{ScalarStyle, Span};

use super::node_to_doc::{prints_block_header, prints_no_text, renders_empty, value_to_doc};
use super::options::YamlFormatOptions;
use super::scalar_render::{CORE_TAG_PREFIX, format_tag, is_core_schema_tag};

/// Join a collection's anchor and user-defined tag into `&anchor !tag`, or just
/// one of them. Core schema tags are dropped. `None` when neither remains.
pub(super) fn collection_properties(anchor: Option<&str>, tag: Option<&str>) -> Option<String> {
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

/// Spell out the tag of a collection that the renderers would drop, so that they
/// print it.  They drop `!!map` on a mapping and `!!seq` on a sequence because
/// they say nothing the kind does not.  Two tags say something:
///
/// - a core tag of another type (`!!set` on a mapping, `!!omap` on a sequence,
///   `!!str` on either) changes the data;
/// - the bare `!` is the non-specific tag, which the loader resolves to the kind's
///   own tag.  Only its text tells it from a collection written without a tag, so
///   it is kept; the loader leaves `tag_loc` on such a collection, and a `!` is the
///   one tag whose span is one byte long (`!!seq` is not).
///
/// The tag becomes its short form (`!!set`, `!`), which is not a core URI, so
/// `collection_properties` keeps it.
pub(super) fn spell_collection_tags(node: &mut Node<Span>) {
    fn spell(tag: &mut Option<Cow<'static, str>>, natural: &str, non_specific: bool) {
        if non_specific {
            *tag = Some(Cow::Borrowed("!"));
        } else if let Some(suffix) = tag
            .as_deref()
            .filter(|t| is_core_schema_tag(t) && *t != natural)
            .map(|t| t.trim_start_matches(CORE_TAG_PREFIX).to_owned())
        {
            *tag = Some(Cow::Owned(format!("!!{suffix}")));
        }
    }
    let non_specific = node
        .tag_loc()
        .is_some_and(|span| span.end - span.start == 1);
    match node {
        Node::Mapping { entries, tag, .. } => {
            spell(tag, "tag:yaml.org,2002:map", non_specific);
            for (key, value) in entries.iter_mut() {
                spell_collection_tags(key);
                spell_collection_tags(value);
            }
        }
        Node::Sequence { items, tag, .. } => {
            spell(tag, "tag:yaml.org,2002:seq", non_specific);
            items.iter_mut().for_each(spell_collection_tags);
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// The colon that ends a key whose value is a block collection, followed by the
/// collection's properties on the same line: `: &anchor !tag`, or `bare_colon`
/// (`:` or ` :`) when the collection carries none.  A key that needs the space
/// before its colon (`*a : &x`) keeps it.
fn collection_colon(bare_colon: &str, anchor: Option<&str>, tag: Option<&str>) -> Doc {
    collection_properties(anchor, tag).map_or_else(
        || text(bare_colon),
        |props| text(format!("{bare_colon} {props}")),
    )
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

/// What the first entry of a block mapping follows on the line above it.  A key
/// can read as something else there (see `forced_explicit_key`).
#[derive(Clone, Copy)]
pub(super) struct Lead {
    /// The mapping's anchor or tag is printed there.
    pub(super) properties: bool,
}

/// Convert a YAML mapping to Doc, branching on block vs flow style.
///
/// `lead` tells what the first entry follows (see `block_mapping_parts`).
pub(super) fn mapping_to_doc(
    entries: &[(Node<Span>, Node<Span>)],
    style: CollectionStyle,
    options: &YamlFormatOptions,
    lead: Lead,
) -> Doc {
    let Entry { above, doc } = mapping_to_parts(entries, style, options, lead);
    with_comments_above(doc, above)
}

/// Convert a YAML mapping to its Doc and the comments above its first entry, for
/// a caller that puts the mapping's properties before the first entry: the
/// comments print above them, as a reader of the output attaches a comment under
/// the properties to the first entry or the document, not to the collection.
pub(super) fn mapping_to_parts<'a>(
    entries: &'a [(Node<Span>, Node<Span>)],
    style: CollectionStyle,
    options: &YamlFormatOptions,
    lead: Lead,
) -> Entry<'a> {
    if entries.is_empty() {
        return Entry {
            above: Vec::new(),
            doc: text("{}"),
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
            doc: flow_mapping_to_doc(entries, options),
        },
        CollectionStyle::Block => block_mapping_parts(entries, options, lead),
    }
}

/// Convert the entries of a block mapping to their Doc and the comments above
/// the first one.  A caller that puts text before the first entry (`- ` or `? `)
/// prints these above that text: between the text and the entry, they would be
/// read as the comments of the collection.
///
/// `lead` tells what the first entry follows: the mapping's properties, where a
/// first key that is a collection would take them (see `forced_explicit_key`).
pub(super) fn block_mapping_parts<'a>(
    entries: &'a [(Node<Span>, Node<Span>)],
    options: &YamlFormatOptions,
    lead: Lead,
) -> Entry<'a> {
    let previous_values = iter::once(None).chain(entries.iter().map(|(_, value)| Some(value)));
    join_entries(
        entries
            .iter()
            .zip(previous_values)
            .map(|((key, value), previous)| {
                let explicit_key = forced_explicit_key(previous, key, lead);
                entry_to_parts(key, value, options, explicit_key)
            }),
        "{}",
    )
}

/// Whether an entry prints with `? ` although its key does not need it
/// (see `block_mapping_parts`): `previous` is the value of the entry before it,
/// `None` for the first entry, whose key may sit below its mapping's properties.
/// Only a collection key is forced.  Below the properties of its mapping, the
/// parser attaches the properties on the line above `[b]: 1` to the key (`!m`
/// above it is the key's tag, not the mapping's).  After a value with no text, a
/// collection key with properties keeps `? ` so that its properties stay apart
/// from the value's; the shorter form reads the same.
fn forced_explicit_key(previous: Option<&Node<Span>>, key: &Node<Span>, lead: Lead) -> bool {
    previous.map_or_else(
        || lead.properties && is_collection(key),
        |previous| prints_no_text(previous) && is_collection_with_properties(key),
    )
}

/// Whether `node` is a collection that prints an anchor or a tag.
fn is_collection_with_properties(node: &Node<Span>) -> bool {
    match node {
        Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
            collection_properties(node.anchor(), tag.as_deref()).is_some()
        }
        Node::Scalar { .. } | Node::Alias { .. } => false,
    }
}

const fn is_collection(node: &Node<Span>) -> bool {
    matches!(node, Node::Mapping { .. } | Node::Sequence { .. })
}

/// Whether `node` is a collection with entries that prints as a block collection:
/// with the style `Block`, or with any style when `enforce_block` is set (the
/// option that prints every collection as block).
pub(super) const fn is_block_collection(node: &Node<Span>, enforce_block: bool) -> bool {
    match node {
        Node::Mapping { entries, style, .. } => {
            !entries.is_empty() && (enforce_block || matches!(style, CollectionStyle::Block))
        }
        Node::Sequence { items, style, .. } => {
            !items.is_empty() && (enforce_block || matches!(style, CollectionStyle::Block))
        }
        Node::Scalar { .. } | Node::Alias { .. } => false,
    }
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
            // An implicit key is one line of at most 1024 characters (YAML
            // 1.2.2 §7.4.2), and a collection key can be longer than that, or
            // break over lines, whose indentation counts; an explicit key has no
            // limit.  The group around the prefix breaks when the key's own group
            // does, since both hold the same text.
            let key_doc = if needs_explicit_key(key) {
                let prefix = if may_exceed_implicit_key_limit(key) {
                    text("? ")
                } else {
                    flat_alt(text(""), text("? "))
                };
                group(concat(vec![prefix, key_doc]))
            } else {
                key_doc
            };
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

/// The number of characters an implicit key may have (YAML 1.2.2 §7.4.2,
/// §8.2.2); the parser rejects a longer one.
const IMPLICIT_KEY_LIMIT: usize = 1024;

/// The most characters one byte of a scalar prints as: a character without a
/// printable form comes out as an escape such as ` `.
const MAX_ESCAPED_WIDTH: usize = 6;

/// The characters a node prints besides its scalars' text and its properties:
/// brackets, separators, quotes, and the `&`, `!` and spaces of the properties.
const NODE_OVERHEAD: usize = 8;

/// The most characters a tag prints beyond its length: `!<` and `>`.
const TAG_DELIMITERS: usize = 4;

/// Whether the flow form of `node` may be longer than an implicit key may be.
/// An upper bound, so a key that fits is never taken for one that does not, and
/// the walk stops once the bound passes the limit, after
/// `IMPLICIT_KEY_LIMIT / NODE_OVERHEAD` nodes at most.
pub(super) fn may_exceed_implicit_key_limit(node: &Node<Span>) -> bool {
    fn spend(node: &Node<Span>, budget: &mut usize) -> bool {
        let content = match node {
            Node::Scalar { value, .. } => value.len().saturating_mul(MAX_ESCAPED_WIDTH),
            Node::Alias { name, .. } => name.len(),
            Node::Mapping { .. } | Node::Sequence { .. } => 0,
        };
        let tag = match node {
            Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
                tag.as_deref().map_or(0, |tag| tag.len() + TAG_DELIMITERS)
            }
            Node::Alias { .. } => 0,
        };
        let anchor = node.anchor().map_or(0, str::len);
        let cost = content
            .saturating_add(tag)
            .saturating_add(anchor)
            .saturating_add(NODE_OVERHEAD);
        let Some(left) = budget.checked_sub(cost) else {
            return true;
        };
        *budget = left;
        match node {
            Node::Mapping { entries, .. } => entries
                .iter()
                .any(|(key, value)| spend(key, budget) || spend(value, budget)),
            Node::Sequence { items, .. } => items.iter().any(|item| spend(item, budget)),
            Node::Scalar { .. } | Node::Alias { .. } => false,
        }
    }
    let mut budget = IMPLICIT_KEY_LIMIT;
    spend(node, &mut budget)
}

/// Whether the printed form of `key`, with its properties and the space before
/// its colon, is longer than an implicit key may be.  Escaping makes a scalar
/// longer than its value (`\L` for each U+2028), so the value does not tell.
fn exceeds_implicit_key_limit(key: &Node<Span>, options: &YamlFormatOptions) -> bool {
    may_exceed_implicit_key_limit(key) && {
        let printed = rlsp_fmt::format(
            &super::node_to_doc::node_to_doc(key, options, true),
            &rlsp_fmt::FormatOptions {
                print_width: usize::MAX,
                tab_width: options.indent_width(),
                use_tabs: false,
            },
        );
        printed.chars().count() + usize::from(key_needs_space_before_colon(key))
            > IMPLICIT_KEY_LIMIT
    }
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
const fn needs_explicit_key(key: &Node<Span>) -> bool {
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

/// Returns `true` when a mapping key is an effectively-untagged empty plain
/// scalar without an anchor (the implicit empty key `:`).
///
/// A quoted empty scalar (`""`) is a string, not the empty key, and an anchor is
/// part of the key: both are printed.
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
            value,
            style: ScalarStyle::Plain,
            tag: None,
            ..
        } if value.is_empty() && key.anchor().is_none() => true,
        Node::Scalar {
            value,
            style: ScalarStyle::Plain,
            tag: Some(t),
            ..
        } if value.is_empty()
            && key.anchor().is_none()
            && is_core_schema_tag(t)
            && key.tag_loc().is_none() =>
        {
            true
        }
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
///
/// 3. **Anchored empty plain scalar** (`&a`) — `&a:` is parsed as the anchor name
///    `a:`.
pub(super) fn key_needs_space_before_colon(key: &Node<Span>) -> bool {
    match key {
        Node::Scalar {
            value,
            style: ScalarStyle::Plain,
            ..
        } if value.is_empty() && key.anchor().is_some() => true,
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
        mut above,
        doc: key_doc,
    } = explicit_key_parts(key, options);
    // A value with an anchor or a tag prints it after the `:`.
    let value_is_empty = renders_empty(value);

    // `? key_doc` — the key part.
    // For block scalars/collections as keys, the key_doc spans multiple lines.
    // We render `?` + space + key indented by 2 spaces.
    // A key that prints nothing leaves the space after the `?` at the end of
    // the line, so it is left out.
    let question = if renders_empty(key) {
        text("?")
    } else if is_block_collection(key, false) {
        // The first entry shares the line with the `?`; the entries after it
        // continue in its column.
        concat(vec![text(options.compact_marker('?')), indent(key_doc)])
    } else {
        concat(vec![text("? "), indent(key_doc)])
    };
    // A comment on the line of a `?` that has no key text reads back as a comment
    // above the key, so it is printed there.
    let question_comment = if prints_no_text(key) {
        above.extend(line_end_comment(key));
        None
    } else {
        line_end_comment(key)
    };
    let question_line = with_line_end_comment(question, question_comment);

    Entry {
        above,
        doc: concat(vec![
            question_line,
            hard_line(),
            explicit_value_line(value, value_is_empty, options),
        ]),
    }
}

/// The `: value` line of an explicit key entry.  A function of its own, apart
/// from `explicit_key_to_doc`: that function recurses into the key before it
/// builds this line, and a debug build keeps the temporaries of this line in the
/// frame of every level of nesting in a key.
fn explicit_value_line(
    value: &Node<Span>,
    value_is_empty: bool,
    options: &YamlFormatOptions,
) -> Doc {
    // `: value_doc` — the value part.
    let value_comment = line_end_comment(value);
    if value_is_empty {
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
                let lead = Lead {
                    properties: collection_properties(value.anchor(), tag.as_deref()).is_some(),
                };
                let Entry { above, doc } = mapping_to_parts(entries, *style, options, lead);
                // A comment after the colon reads back as a comment of the first
                // entry, so it is printed as one.
                concat(vec![
                    colon_prefix,
                    indent(concat(vec![
                        hard_line(),
                        with_comments_above(doc, value_comment.into_iter().chain(above)),
                    ])),
                ])
            }
            // Block sequence value: `:\n  - item` (or `:\n- item` when indentless).
            Node::Sequence {
                items, style, tag, ..
            } if !items.is_empty() && effective_style(*style) == CollectionStyle::Block => {
                let colon_prefix = collection_colon(":", value.anchor(), tag.as_deref());
                let Entry { above, doc } =
                    super::sequence_render::sequence_to_parts(items, *style, options);
                let seq_doc = with_comments_above(doc, value_comment.into_iter().chain(above));
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
                concat(vec![
                    text(": "),
                    value_doc,
                    trailing_comment_doc(value_comment),
                ])
            }
        }
    }
}

/// Render the key of an explicit key entry.  A block collection key starts on
/// the `? ` line, so the comments above its first entry come back apart from the
/// Doc.
fn explicit_key_parts<'a>(key: &'a Node<Span>, options: &YamlFormatOptions) -> Entry<'a> {
    // A flow collection key stays flow (see `flow_comments`), whatever the
    // option says about the other collections.
    let flow_key_options = YamlFormatOptions {
        format_enforce_block_style: false,
        ..options.clone()
    };
    let is_block = |style: CollectionStyle| style == CollectionStyle::Block;
    match key {
        Node::Mapping {
            entries,
            style,
            tag,
            ..
        } if !entries.is_empty() && is_block(*style) => {
            let lead = Lead {
                properties: collection_properties(key.anchor(), tag.as_deref()).is_some(),
            };
            let Entry { above, doc } = block_mapping_parts(entries, options, lead);
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
        Node::Scalar { .. } | Node::Alias { .. } => Entry {
            above: Vec::new(),
            doc: super::node_to_doc::node_to_doc(key, options, true),
        },
        Node::Mapping { .. } | Node::Sequence { .. } => Entry {
            above: Vec::new(),
            doc: super::node_to_doc::node_to_doc(key, &flow_key_options, true),
        },
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
///
/// `explicit_key` forces the explicit form `? key` where `forced_explicit_key`
/// says so: a collection key below the properties of its mapping (`[]: v` is
/// taken for the key's properties), and a collection key with properties after a
/// value with no text.
pub(super) fn entry_to_parts<'a>(
    key: &'a Node<Span>,
    value: &'a Node<Span>,
    options: &YamlFormatOptions,
    explicit_key: bool,
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
    if needs_explicit_key(key) || explicit_key || exceeds_implicit_key_limit(key, options) {
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
            let lead = Lead {
                properties: collection_properties(value.anchor(), tag.as_deref()).is_some(),
            };
            concat(vec![
                key_doc,
                colon,
                trailing_comment_doc(line_end),
                indent(concat(vec![
                    hard_line(),
                    mapping_to_doc(entries, *style, options, lead),
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
