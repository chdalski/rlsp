// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{CollectionStyle, Document, LineIndex, ScalarStyle, Span};

use crate::lsp_util::{offset_to_lsp, span_to_lsp};
use crate::navigation::collect_anchor_alias_entries;

use super::make_action;

/// The anchors of the documents a request is answered over, found by the range
/// of their `&name` token: what an unused-anchor diagnostic selects.
///
/// Built once for all diagnostics of a request, so each diagnostic costs one
/// lookup instead of a walk over the documents.
pub(super) struct AnchorIndex<'a> {
    docs: &'a [Document<Span>],
    anchors: HashMap<RangeKey, IndexedAnchor<'a>>,
    /// Per document, the names an alias refers to: the validator reports an
    /// anchor unused only when its name is not among them, and deleting an
    /// anchor with an alias would leave the alias without one.
    aliased: Vec<HashSet<&'a str>>,
}

/// A [`Range`] as a map key: the type has no hash.
type RangeKey = (u32, u32, u32, u32);

const fn range_key(range: Range) -> RangeKey {
    (
        range.start.line,
        range.start.character,
        range.end.line,
        range.end.character,
    )
}

/// An anchor of an [`AnchorIndex`].
struct IndexedAnchor<'a> {
    /// Position of the anchor's document in `AnchorIndex::docs`.
    doc: usize,
    name: &'a str,
    /// Span of the `&name` token.
    loc: Span,
    /// Whether the document stays valid without the token: a flow collection
    /// entry that is an empty node needs the anchor to exist (`[&a, b]`), and a
    /// document without `---` whose only content is the anchored empty node
    /// would vanish from the stream with its token.
    removable: bool,
}

/// What the walk of one document knows besides the node it is at.
struct Walk<'a, 'm> {
    doc: usize,
    idx: &'a LineIndex,
    /// The anchor token of the document's root when the document is nothing
    /// but that anchored empty node and has no `---`.
    bare_empty_root: Option<Span>,
    anchors: &'m mut HashMap<RangeKey, IndexedAnchor<'a>>,
}

impl<'a> AnchorIndex<'a> {
    /// Index every anchor of `docs`; the whole of each document is read: how
    /// deep the documents nest is bounded where they are loaded.
    pub(super) fn new(docs: &'a [Document<Span>]) -> Self {
        let mut anchors = HashMap::new();
        let mut aliased = Vec::with_capacity(docs.len());
        for (doc_pos, doc) in docs.iter().enumerate() {
            let bare_empty_root = if !doc.explicit_start && is_empty_node(&doc.root) {
                doc.root.anchor_loc()
            } else {
                None
            };
            let mut walk = Walk {
                doc: doc_pos,
                idx: doc.line_index(),
                bare_empty_root,
                anchors: &mut anchors,
            };
            index_node(&doc.root, &mut walk, false);
            aliased.push(
                collect_anchor_alias_entries(doc)
                    .1
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect(),
            );
        }
        Self {
            docs,
            anchors,
            aliased,
        }
    }
}

/// Index the anchors of `node` and below, in document order: the first anchor of
/// a range wins, as the first one found by a search would.
///
/// `needs_node` is whether `node` is the whole of an entry of a flow
/// collection: a flow sequence item, or the key of a flow mapping entry whose
/// value is empty too.
fn index_node<'a>(node: &'a Node<Span>, walk: &mut Walk<'a, '_>, needs_node: bool) {
    if let (Some(name), Some(loc)) = (node.anchor(), node.anchor_loc()) {
        walk.anchors
            .entry(range_key(span_to_lsp(loc, walk.idx)))
            .or_insert_with(|| IndexedAnchor {
                doc: walk.doc,
                name,
                loc,
                removable: !(needs_node && is_empty_node(node))
                    && walk.bare_empty_root != Some(loc),
            });
    }
    match node {
        Node::Mapping { entries, style, .. } => {
            let in_flow = *style == CollectionStyle::Flow;
            for (key, value) in entries {
                // `{&a : v}` keeps its entry without the anchor, `{&a, b}` does not.
                index_node(key, walk, in_flow && is_empty_node(value));
                index_node(value, walk, false);
            }
        }
        Node::Sequence { items, style, .. } => {
            let in_flow = *style == CollectionStyle::Flow;
            for item in items {
                index_node(item, walk, in_flow);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Offer to delete the `&name` token an unused-anchor diagnostic ranges over.
///
/// The diagnostic only selects the anchor: the edit is computed from the
/// anchor's span in the AST and the text around it, and nothing is offered when
/// the diagnostic matches no anchor of `index` (it is stale), when the anchor
/// has an alias in the meantime, or when deleting it would break the document.
pub(super) fn delete_unused_anchor(
    index: &AnchorIndex<'_>,
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let anchor = index.anchors.get(&range_key(diag.range))?;
    if !anchor.removable || index.aliased.get(anchor.doc)?.contains(anchor.name) {
        return None;
    }

    let start = usize::try_from(anchor.loc.start).ok()?;
    let end = usize::try_from(anchor.loc.end).ok()?;
    if text.get(start..end)?.strip_prefix('&')? != anchor.name {
        return None;
    }
    let (delete_start, delete_end) = deletion_range(text, start, end)?;

    let idx = index.docs.get(anchor.doc)?.line_index();
    Some(make_action(
        "Delete unused anchor".to_string(),
        uri,
        vec![TextEdit {
            range: Range::new(
                offset_to_lsp(u32::try_from(delete_start).ok()?, idx),
                offset_to_lsp(u32::try_from(delete_end).ok()?, idx),
            ),
            new_text: String::new(),
        }],
        CodeActionKind::QUICKFIX,
        Some(vec![diag.clone()]),
    ))
}

/// An empty plain scalar without a tag token: the node `[&a, b]` stands for,
/// which is nothing at all once the anchor is gone.  (The loader resolves the
/// tag of an untagged node, so `tag` cannot tell; the tag token can.)
fn is_empty_node(node: &Node<Span>) -> bool {
    node.tag_loc().is_none()
        && matches!(
            node,
            Node::Scalar { value, style: ScalarStyle::Plain, .. } if value.is_empty()
        )
}

/// The byte range to delete for the anchor token at `start..end` of `buffer`.
///
/// Only blanks (spaces and tabs) around the token go with it, so the rest of
/// the text stays byte-identical:
/// - content follows on the line: the token and the blanks after it;
/// - a line break or the end of input follows: the token and the blanks before
///   it, so no trailing blanks stay;
/// - a comment follows: the token and the blanks before it, so the comment
///   keeps its separator; the blanks after it when the token starts the line;
/// - the token is alone on its line (blanks before it, a line break or the end
///   of input after it): the whole line with its line break.
fn deletion_range(buffer: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    // A byte order mark is not text of the first line: nothing but blanks
    // before the token means the token starts the line.
    let line_start = buffer.get(..start)?.rfind(['\n', '\r']).map_or_else(
        || usize::from(buffer.starts_with('\u{FEFF}')) * '\u{FEFF}'.len_utf8(),
        |at| at + 1,
    );
    let before = buffer.get(line_start..start)?;
    let kept = before.trim_end_matches([' ', '\t']);
    let blanks_start = line_start + kept.len();
    let alone = kept.is_empty();

    let content = buffer.get(end..)?.trim_start_matches([' ', '\t']);
    let blanks_end = buffer.len() - content.len();

    let break_len = if content.starts_with("\r\n") {
        2
    } else {
        usize::from(content.starts_with(['\r', '\n']))
    };
    let at_line_end = break_len > 0 || content.is_empty();
    let comment = content.starts_with('#');

    Some(match (at_line_end, comment, alone) {
        (true, _, true) => (line_start, blanks_end + break_len),
        (true, _, false) => (blanks_start, blanks_end),
        (false, true, false) => (blanks_start, end),
        (false, _, _) => (start, blanks_end),
    })
}

#[cfg(test)]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "YAML flow mappings such as `{k: }` look like format arguments"
)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{Diagnostic, Position, Range};

    use super::super::code_actions;
    use super::super::test_helpers::{apply_text_edit, docs_for, make_diagnostic};
    use crate::editing::formatter::YamlFormatOptions;
    use crate::test_utils::{nested_flow, test_uri};
    use crate::validation::validators::validate_unused_anchors;

    const TITLE: &str = "Delete unused anchor";

    fn whole_document() -> Range {
        Range::new(Position::new(0, 0), Position::new(9999, 0))
    }

    /// The title-matching actions the server offers for `diag`.
    fn unused_anchor_actions(
        yaml: &str,
        docs: &[rlsp_yaml_parser::Document<rlsp_yaml_parser::Span>],
        range: Range,
        diag: &Diagnostic,
    ) -> Vec<tower_lsp::lsp_types::CodeAction> {
        code_actions(
            docs,
            yaml,
            range,
            std::slice::from_ref(diag),
            &test_uri(),
            &YamlFormatOptions::default(),
        )
        .into_iter()
        .filter(|a| a.title == TITLE)
        .collect()
    }

    /// For every unused-anchor diagnostic of `yaml`, in order: the document with
    /// the offered fix applied, or `None` when no fix is offered.  Each fix is a
    /// quick fix for its own diagnostic and a single edit that only deletes.
    fn applied_fixes(yaml: &str) -> Vec<Option<String>> {
        let docs = docs_for(yaml);
        let diagnostics = validate_unused_anchors(&docs);
        diagnostics
            .iter()
            .map(|diag| {
                let actions = unused_anchor_actions(yaml, &docs, whole_document(), diag);
                assert!(actions.len() <= 1, "{yaml:?}: {actions:?}");
                let action = actions.first()?;
                assert_eq!(
                    action.kind,
                    Some(tower_lsp::lsp_types::CodeActionKind::QUICKFIX)
                );
                assert_eq!(
                    action.diagnostics.as_deref(),
                    Some(std::slice::from_ref(diag))
                );
                let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
                assert_eq!(edits.len(), 1, "{yaml:?}");
                assert_eq!(edits[0].new_text, "", "{yaml:?}: the fix only deletes");
                Some(apply_text_edit(yaml, &edits[0]))
            })
            .collect()
    }

    /// The document `yaml` with its only unused anchor deleted.
    fn applied_fix(yaml: &str) -> String {
        let fixes = applied_fixes(yaml);
        assert_eq!(fixes.len(), 1, "one unused anchor in {yaml:?}");
        fixes
            .into_iter()
            .next()
            .flatten()
            .unwrap_or_else(|| panic!("a fix for {yaml:?}"))
    }

    // The fix deletes the token and the blanks around it; the rest of the
    // document stays byte-identical.
    #[rstest]
    #[case::plain_scalar("defaults: &unused value\n", "defaults: value\n")]
    #[case::trailing_comment("key: &a value  # keep me\n", "key: value  # keep me\n")]
    #[case::quoted_scalar("key: &a \"hello\"\n", "key: \"hello\"\n")]
    #[case::tag_after("key: &a !custom \"hello\"\n", "key: !custom \"hello\"\n")]
    #[case::tag_before("key: !custom &a \"hello\"\n", "key: !custom \"hello\"\n")]
    #[case::core_tag("k: &a !!str 1\n", "k: !!str 1\n")]
    #[case::flow_sequence("list: &nums [1, 2, 3]\n", "list: [1, 2, 3]\n")]
    #[case::several_blanks("k:   &a    v\n", "k:   v\n")]
    #[case::tab_after("k: &a\tv\n", "k: v\n")]
    #[case::block_mapping("base: &defaults\n  x: 1\n  y: 2\n", "base:\n  x: 1\n  y: 2\n")]
    #[case::block_sequence("list: &s\n  - 1\n", "list:\n  - 1\n")]
    #[case::literal_block_scalar("a: &x |\n  line one\nb: 1\n", "a: |\n  line one\nb: 1\n")]
    #[case::block_scalar_header_comment("a: &unused | # c\n  t\nb: 1\n", "a: | # c\n  t\nb: 1\n")]
    #[case::folded_block_scalar("a: &x >-\n  text\n", "a: >-\n  text\n")]
    #[case::block_scalar_keep("a: !t &x |+\n  t\n\n", "a: !t |+\n  t\n\n")]
    #[case::string_with_the_same_text("a: \"&x\"\nb: &x 1\n", "a: \"&x\"\nb: 1\n")]
    #[case::comment_with_the_same_text("# &x\nb: &x 1\n", "# &x\nb: 1\n")]
    #[case::block_scalar_body_with_the_same_text("a: &x |\n  &x text\n", "a: |\n  &x text\n")]
    #[case::mapping_key("&a k: v\n", "k: v\n")]
    #[case::key_in_sequence_item("- &a x: 1\n", "- x: 1\n")]
    #[case::mapping_in_sequence_item("- &a\n  x: 1\n", "-\n  x: 1\n")]
    #[case::explicit_key("? &k\n: v\n", "?\n: v\n")]
    #[case::multibyte_text("名前: &u v # ü\n", "名前: v # ü\n")]
    #[case::multibyte_name("k: &ü v\n", "k: v\n")]
    #[case::astral_key("😀: &a v\n", "😀: v\n")]
    #[case::astral_text_after("k: &a 😀 # 😀\n", "k: 😀 # 😀\n")]
    #[case::astral_name("k: &😀 v\n", "k: v\n")]
    #[case::astral_line_before("😀: 1\nk: &a v\n", "😀: 1\nk: v\n")]
    #[case::astral_twice_before("😀😀: &a v\n", "😀😀: v\n")]
    #[case::empty_value("a: &x\n", "a:\n")]
    #[case::empty_value_at_the_end("a: &x", "a:")]
    #[case::empty_value_trailing_blanks("a: &x   \nb: 1\n", "a:\nb: 1\n")]
    #[case::empty_value_comment("a: &y # c\nb: 1\n", "a: # c\nb: 1\n")]
    #[case::empty_sequence_item_tag("- &y !t\n- x\n", "- !t\n- x\n")]
    #[case::empty_sequence_item("- &y\n- x\n", "-\n- x\n")]
    #[case::empty_value_tag("k: !t &a\nj: 1\n", "k: !t\nj: 1\n")]
    #[case::empty_explicit_value("? k\n: &v\n", "? k\n:\n")]
    #[case::alone_on_the_line("&x\nk: v", "k: v")]
    #[case::alone_with_trailing_blanks("&x  \nk: v", "k: v")]
    #[case::alone_before_a_comment("&x # note\nk: v", "# note\nk: v")]
    #[case::alone_indented("k:\n  &x\n  a: 1\n", "k:\n  a: 1\n")]
    #[case::alone_indented_before_a_comment("k:\n  &x # n\n  a: 1\n", "k:\n  # n\n  a: 1\n")]
    #[case::after_document_start("--- &x\nk: v\n", "---\nk: v\n")]
    #[case::scalar_after_document_start("--- &x v\n", "--- v\n")]
    #[case::alone_on_the_last_line("k: v\n---\n&x", "k: v\n---\n")]
    #[case::crlf_alone("&x\r\nk: v\r\n", "k: v\r\n")]
    #[case::cr_alone("&x\rk: v\r", "k: v\r")]
    #[case::crlf_block("k: &a\r\n  x: 1\r\n", "k:\r\n  x: 1\r\n")]
    #[case::cr_block("k: &a\r  x: 1\r", "k:\r  x: 1\r")]
    #[case::crlf_comment("k: &a v # c\r\n", "k: v # c\r\n")]
    #[case::crlf_second_document("a: 1\r\n---\r\nb: &y 2\r\n", "a: 1\r\n---\r\nb: 2\r\n")]
    #[case::compact_mapping_keeps_alignment("- &a k: v\n  k2: v2\n", "- k: v\n  k2: v2\n")]
    #[case::flow_mapping_value("{k: &a}", "{k: }")]
    #[case::flow_mapping_value_then_entry("{k: &a, j: 1}", "{k: , j: 1}")]
    #[case::flow_sequence_item_with_content("[&a x]", "[x]")]
    #[case::flow_sequence_later_item("[a, &b x]", "[a, x]")]
    #[case::flow_mapping_key_with_content("{&a k: v}", "{k: v}")]
    #[case::empty_flow_mapping_key_with_a_value("{&a : v}", "{: v}")]
    #[case::empty_flow_pair_key_with_a_value("[&a : v]", "[: v]")]
    #[case::flow_mapping_value_on_its_own_line("{\n  k: &a\n}", "{\n  k:\n}")]
    fn the_fix_deletes_only_the_token(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(applied_fix(text), expected);
    }

    // Deleting the token would leave an entry of a flow collection without a
    // node; the diagnostic stays, the fix is not offered.
    #[rstest]
    #[case::first_sequence_item("[&a, b]")]
    #[case::last_sequence_item("[b, &a]")]
    #[case::mapping_key("{&a, b}")]
    #[case::only_sequence_item("[&a]")]
    #[case::only_mapping_key("{&a}")]
    #[case::blank_before_the_comma("[&a , b]")]
    #[case::item_on_its_own_line("[\n  &a,\n  b\n]")]
    #[case::only_item_on_its_own_line("[\n  &a\n]")]
    #[case::nested("{k: [&a, b]}")]
    #[case::in_block_sequence("- [&a]\n")]
    #[case::mapping_key_without_value("{&a : }")]
    fn no_fix_when_an_empty_flow_entry_would_lose_its_node(#[case] text: &str) {
        let docs = docs_for(text);
        assert_eq!(validate_unused_anchors(&docs).len(), 1, "{text:?}");
        assert_eq!(applied_fixes(text), [None], "{text:?}");
    }

    // Each anchor of a document gets its own fix, which touches only its token.
    #[rstest]
    #[case::key_and_value("&k key: &v val\n", &["key: &v val\n", "&k key: val\n"])]
    #[case::same_name_twice("a: &x 1\nb: &x 2\n", &["a: 1\nb: &x 2\n", "a: &x 1\nb: 2\n"])]
    #[case::one_of_two_used(
        "a: &used 1\nb: &unused 2\nc: *used\n",
        &["a: &used 1\nb: 2\nc: *used\n"]
    )]
    #[case::two_documents("a: &x 1\n---\nb: &y 2\n", &["a: 1\n---\nb: &y 2\n", "a: &x 1\n---\nb: 2\n"])]
    #[case::document_after_multibyte_documents(
        "名前: &p 1\n---\n名前: 2\n---\nc: &z 3\n",
        &["名前: 1\n---\n名前: 2\n---\nc: &z 3\n", "名前: &p 1\n---\n名前: 2\n---\nc: 3\n"]
    )]
    fn every_anchor_gets_a_fix_for_its_own_token(#[case] text: &str, #[case] expected: &[&str]) {
        let fixes: Vec<Option<String>> = applied_fixes(text);
        let expected: Vec<Option<String>> =
            expected.iter().map(|s| Some((*s).to_owned())).collect();
        assert_eq!(fixes, expected);
    }

    // Applying a fix and parsing again leaves one anchor less, and applying
    // the fixes one after the other ends without an anchor.
    #[test]
    fn applying_the_first_fix_again_and_again_removes_every_anchor() {
        let mut text = "&k key: &v val\nl: &s\n  - &t 1\n".to_owned();
        for expected_left in [4, 3, 2, 1] {
            let diagnostics = validate_unused_anchors(&docs_for(&text));
            assert_eq!(diagnostics.len(), expected_left, "{text:?}");
            text = applied_fixes(&text).into_iter().next().flatten().unwrap();
        }
        assert!(!text.contains('&'), "{text:?}");
    }

    // The anchored empty node is the only content of a document without `---`:
    // without the token the document would vanish from the stream, so no fix.
    #[rstest]
    #[case::with_break("&x\n")]
    #[case::without_break("&x")]
    #[case::before_a_comment("&x # c\n")]
    #[case::before_document_end("&x\n...\n")]
    #[case::before_the_next_document("&x\n---\nk: v\n")]
    #[case::before_the_next_document_crlf("&x\r\n---\r\nk: v\r\n")]
    fn no_fix_when_the_document_would_vanish(#[case] text: &str) {
        assert_eq!(applied_fixes(text), [None], "{text:?}");
    }

    // With `---` the document stays, and so does an anchored empty node that is
    // not the whole document.
    #[rstest]
    #[case::explicit_start("---\n&x\n---\nk: v\n", "---\n---\nk: v\n")]
    #[case::explicit_start_and_end("%YAML 1.2\n---\n&x\n...\n", "%YAML 1.2\n---\n...\n")]
    #[case::sequence_item("-\n  &x\n- b\n", "-\n- b\n")]
    #[case::before_a_collection("&x\n- a\n", "- a\n")]
    #[case::tag_stays("!t\n&x\nk: v\n", "!t\nk: v\n")]
    fn the_fix_is_offered_where_the_document_stays(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(applied_fix(text), expected);
    }

    // A byte order mark is not text of the line: the line of the token goes, the
    // mark stays.
    #[test]
    fn a_byte_order_mark_before_the_token_is_kept_and_the_line_deleted() {
        assert_eq!(applied_fix("\u{FEFF}&x\nk: v\n"), "\u{FEFF}k: v\n");
    }

    // A diagnostic that is not the range of an anchor token of the document
    // (stale, or sent for other text) offers nothing and does not panic.
    #[rstest]
    #[case::line_past_the_end("k: &a v\n", (99, 3, 99, 5))]
    #[case::column_past_the_line("k: &a v\n", (0, 100, 0, 200))]
    #[case::end_before_start("k: &a v\n", (0, 5, 0, 3))]
    #[case::empty_range("k: &a v\n", (0, 3, 0, 3))]
    #[case::multiline_range("k: &a v\nj: 1\n", (0, 3, 1, 2))]
    #[case::huge("k: &a v\n", (u32::MAX, u32::MAX, u32::MAX, u32::MAX))]
    #[case::sigil_only("k: &a v\n", (0, 3, 0, 4))]
    #[case::too_long("k: &a v\n", (0, 3, 0, 7))]
    #[case::shifted("k: &a v\n", (0, 4, 0, 6))]
    #[case::the_value("data: value\n", (0, 6, 0, 11))]
    #[case::stale_anchor_range("data: value\n", (0, 6, 0, 13))]
    #[case::inside_a_string("a: \"&x\"\n", (0, 4, 0, 6))]
    #[case::inside_a_comment("# &x\n", (0, 2, 0, 4))]
    #[case::inside_a_block_scalar("a: |\n  &x\n", (1, 2, 1, 4))]
    #[case::inside_a_line_break("k: &a\r\n", (0, 6, 0, 8))]
    fn a_diagnostic_that_is_no_anchor_token_offers_nothing(
        #[case] text: &str,
        #[case] range: (u32, u32, u32, u32),
    ) {
        let diag = Diagnostic {
            range: Range::new(
                Position::new(range.0, range.1),
                Position::new(range.2, range.3),
            ),
            ..make_diagnostic(0, 0, 0, "unusedAnchor")
        };
        assert!(unused_anchor_actions(text, &docs_for(text), whole_document(), &diag).is_empty());
    }

    #[test]
    fn a_diagnostic_without_documents_offers_nothing() {
        let text = "k: &a v\n";
        let diag = make_diagnostic(0, 3, 5, "unusedAnchor");
        assert!(unused_anchor_actions(text, &[], whole_document(), &diag).is_empty());
    }

    // The anchor has an alias by now, at any depth: deleting it would leave the
    // alias without an anchor.
    #[rstest]
    fn a_stale_diagnostic_of_an_anchor_with_an_alias_offers_nothing(
        #[values(0, 1, 99, 100, 101, 255)] depth: usize,
    ) {
        let text = format!("a: &x 1\nb: {}\n", nested_flow(depth, "*x"));
        assert!(rlsp_yaml_parser::load(&text).is_ok());
        let diag = make_diagnostic(0, 3, 5, "unusedAnchor");
        assert!(validate_unused_anchors(&docs_for(&text)).is_empty());
        assert!(unused_anchor_actions(&text, &docs_for(&text), whole_document(), &diag).is_empty());
    }

    // An alias of another name leaves the anchor unused, at any depth.
    #[rstest]
    fn an_alias_of_another_name_does_not_block_the_fix(#[values(101, 255)] depth: usize) {
        let text = format!("a: &x 1\nb: {}\n", nested_flow(depth, "*y"));
        assert!(rlsp_yaml_parser::load(&text).is_ok());
        let diag = make_diagnostic(0, 3, 5, "unusedAnchor");
        let actions = unused_anchor_actions(&text, &docs_for(&text), whole_document(), &diag);
        assert_eq!(actions.len(), 1);
    }

    // An alias of the same name in another document is no alias of the anchor.
    #[test]
    fn an_alias_in_another_document_does_not_block_the_fix() {
        let text = format!("a: &x 1\n---\nb: {}\n", nested_flow(101, "*x"));
        assert!(rlsp_yaml_parser::load(&text).is_ok());
        let diag = make_diagnostic(0, 3, 5, "unusedAnchor");
        let actions = unused_anchor_actions(&text, &docs_for(&text), whole_document(), &diag);
        assert_eq!(actions.len(), 1);
    }

    // An anchor is found and deleted at any depth.
    #[rstest]
    fn the_fix_deletes_an_anchor_at_any_depth(#[values(99, 100, 101, 255)] depth: usize) {
        let text = format!("a: {}\n", nested_flow(depth, "&x 1"));
        assert!(rlsp_yaml_parser::load(&text).is_ok());
        assert_eq!(
            applied_fix(&text),
            format!("a: {}\n", nested_flow(depth, "1"))
        );
    }

    // Whether a flow entry needs its anchor is told at any depth.
    #[test]
    fn the_flow_entry_guard_holds_at_depth() {
        let kept = format!("a: {}\n", nested_flow(101, "[&a, b]"));
        assert_eq!(applied_fixes(&kept), [None]);

        let deleted = format!("a: {}\n", nested_flow(101, "{&a : v}"));
        assert_eq!(
            applied_fix(&deleted),
            format!("a: {}\n", nested_flow(101, "{: v}"))
        );
    }

    // Two anchors of one name, one of them deep: each fix deletes its own token.
    #[test]
    fn a_deep_anchor_of_a_shared_name_gets_its_own_fix() {
        let text = format!("a: &x 1\nb: {}\n", nested_flow(101, "&x 2"));
        assert_eq!(
            applied_fixes(&text),
            [
                Some(format!("a: 1\nb: {}\n", nested_flow(101, "&x 2"))),
                Some(format!("a: &x 1\nb: {}\n", nested_flow(101, "2"))),
            ]
        );
    }

    // With two anchors of one name the diagnostic selects the one at its range.
    #[test]
    fn a_diagnostic_selects_the_anchor_at_its_range() {
        let text = "a: &x 1\nb: &x 2\n";
        let diag = make_diagnostic(1, 3, 5, "unusedAnchor");
        let actions = unused_anchor_actions(text, &docs_for(text), whole_document(), &diag);
        let edits = &actions[0].edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        assert_eq!(
            edits[0].range,
            Range::new(Position::new(1, 3), Position::new(1, 6))
        );
    }

    // The diagnostic is on the line of the token, not on the lines of the node.
    #[test]
    fn the_action_is_offered_for_the_anchor_line_only() {
        let text = "defaults: &d\n  key: val\n";
        let docs = docs_for(text);
        let diagnostics = validate_unused_anchors(&docs);
        let on_anchor = Range::new(Position::new(0, 0), Position::new(0, 0));
        let on_body = Range::new(Position::new(1, 0), Position::new(1, 0));
        assert_eq!(
            unused_anchor_actions(text, &docs, on_anchor, &diagnostics[0]).len(),
            1
        );
        assert!(unused_anchor_actions(text, &docs, on_body, &diagnostics[0]).is_empty());
    }

    // ---- one request, many diagnostics ---------------------------------------

    /// The "Delete unused anchor" actions of one `code_actions` call over all of
    /// `diagnostics`, which must equal those of one call per diagnostic, joined
    /// in the order of the diagnostics.
    fn batch_equals_singles(
        yaml: &str,
        diagnostics: &[Diagnostic],
        range: Range,
    ) -> Vec<tower_lsp::lsp_types::CodeAction> {
        let docs = docs_for(yaml);
        let batch: Vec<_> = code_actions(
            &docs,
            yaml,
            range,
            diagnostics,
            &test_uri(),
            &YamlFormatOptions::default(),
        )
        .into_iter()
        .filter(|a| a.title == TITLE)
        .collect();
        let singles: Vec<_> = diagnostics
            .iter()
            .flat_map(|diag| unused_anchor_actions(yaml, &docs, range, diag))
            .collect();
        assert_eq!(batch, singles, "{yaml:?}");
        batch
    }

    /// `yaml` with the edit of `action` applied.
    fn apply_action(yaml: &str, action: &tower_lsp::lsp_types::CodeAction) -> String {
        let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&test_uri()];
        apply_text_edit(yaml, &edits[0])
    }

    fn unused_anchor_diagnostics(yaml: &str) -> Vec<Diagnostic> {
        validate_unused_anchors(&docs_for(yaml))
    }

    #[test]
    fn one_call_over_all_diagnostics_equals_one_call_per_diagnostic() {
        let text = "&k key: &v val\nl: [&a, b]\nm: &x 1\nn: &x 2\n---\no: &y 1\np: *y\n";
        let mut diagnostics = unused_anchor_diagnostics(text);
        assert_eq!(diagnostics.len(), 5, "&k, &v, &a, &x, &x");
        // The anchor `&y` has an alias: its diagnostic is stale.
        diagnostics.push(make_diagnostic(5, 3, 5, "unusedAnchor"));

        let actions = batch_equals_singles(text, &diagnostics, whole_document());

        let applied: Vec<String> = actions.iter().map(|a| apply_action(text, a)).collect();
        assert_eq!(
            applied,
            [
                "key: &v val\nl: [&a, b]\nm: &x 1\nn: &x 2\n---\no: &y 1\np: *y\n",
                "&k key: val\nl: [&a, b]\nm: &x 1\nn: &x 2\n---\no: &y 1\np: *y\n",
                "&k key: &v val\nl: [&a, b]\nm: 1\nn: &x 2\n---\no: &y 1\np: *y\n",
                "&k key: &v val\nl: [&a, b]\nm: &x 1\nn: 2\n---\no: &y 1\np: *y\n",
            ]
        );
    }

    // One action per diagnostic, in the order of the diagnostics.
    #[test]
    fn the_actions_follow_the_order_of_the_diagnostics_and_repeat_with_them() {
        let text = "a: &x 1\nb: &y 2\nc: &z 3\n";
        let all = unused_anchor_diagnostics(text);
        let diagnostics = [
            all[2].clone(),
            all[0].clone(),
            all[1].clone(),
            all[0].clone(),
        ];

        let actions = batch_equals_singles(text, &diagnostics, whole_document());

        let applied: Vec<String> = actions.iter().map(|a| apply_action(text, a)).collect();
        assert_eq!(
            applied,
            [
                "a: &x 1\nb: &y 2\nc: 3\n",
                "a: 1\nb: &y 2\nc: &z 3\n",
                "a: &x 1\nb: 2\nc: &z 3\n",
                "a: 1\nb: &y 2\nc: &z 3\n",
            ]
        );
    }

    // A refusal for one anchor leaves the other anchors of the request alone.
    #[rstest]
    #[case::sequence_item("[&a, b]\n---\nk: &z v\n", 1)]
    #[case::mapping_key("{&a, b}\n---\nk: &z v\n", 1)]
    #[case::bare_empty_root("&x\n---\nk: &z v\n", 1)]
    #[case::removable_key("{&a : v}\n---\nk: &z v\n", 2)]
    #[case::explicit_start("---\n&x\n---\nk: &z v\n", 2)]
    #[case::alias_in_the_document("a: &x 1\nb: *x\n---\nk: &z v\n", 1)]
    fn a_refused_anchor_does_not_disturb_the_others(#[case] text: &str, #[case] offered: usize) {
        let mut diagnostics = unused_anchor_diagnostics(text);
        // The diagnostic of an anchor with an alias is stale.
        if text.contains("*x") {
            diagnostics.insert(0, make_diagnostic(0, 3, 5, "unusedAnchor"));
        }
        let actions = batch_equals_singles(text, &diagnostics, whole_document());
        assert_eq!(actions.len(), offered, "{text:?}");
        assert!(
            actions
                .last()
                .is_some_and(|a| apply_action(text, a).contains("k: v")),
            "{text:?}: the anchor `&z` is deleted"
        );
    }

    // Anchors of one name each get their own action; an alias blocks the
    // anchors of its name in its own document only.
    #[test]
    fn anchors_of_one_name_are_told_apart_by_their_range_and_document() {
        let text = "a: &x 1\nb: &x 2\nc: *x\n---\nd: &x 3\n---\ne: &x 4\nf: &x 5\n";
        let diagnostics = [
            make_diagnostic(0, 3, 5, "unusedAnchor"),
            make_diagnostic(1, 3, 5, "unusedAnchor"),
            make_diagnostic(4, 3, 5, "unusedAnchor"),
            make_diagnostic(6, 3, 5, "unusedAnchor"),
            make_diagnostic(7, 3, 5, "unusedAnchor"),
        ];

        let actions = batch_equals_singles(text, &diagnostics, whole_document());

        let applied: Vec<String> = actions.iter().map(|a| apply_action(text, a)).collect();
        assert_eq!(
            applied,
            [
                "a: &x 1\nb: &x 2\nc: *x\n---\nd: 3\n---\ne: &x 4\nf: &x 5\n",
                "a: &x 1\nb: &x 2\nc: *x\n---\nd: &x 3\n---\ne: 4\nf: &x 5\n",
                "a: &x 1\nb: &x 2\nc: *x\n---\nd: &x 3\n---\ne: &x 4\nf: 5\n",
            ]
        );
    }

    // An alias blocks the anchors of exactly its name.
    #[rstest]
    #[case::longer_alias("a: &a 1\nb: *ab\n")]
    #[case::shorter_alias("a: &ab 1\nb: *a\n")]
    #[case::other_case("a: &A 1\nb: *a\n")]
    fn an_alias_of_a_similar_name_does_not_block_the_fix(#[case] text: &str) {
        let diagnostics: Vec<Diagnostic> = unused_anchor_diagnostics(text)
            .into_iter()
            .filter(|d| super::super::diagnostic_code(d) == Some("unusedAnchor"))
            .collect();
        assert_eq!(diagnostics.len(), 1, "{text:?}");
        assert_eq!(
            batch_equals_singles(text, &diagnostics, whole_document()).len(),
            1
        );
    }

    #[test]
    fn many_documents_each_resolve_to_their_own_anchor() {
        let text: String = (0..50)
            .map(|i| match i {
                25 => "---\nk: v\n".to_string(),
                _ => format!("---\nk: &a{i} v\n"),
            })
            .collect();
        let mut diagnostics = unused_anchor_diagnostics(&text);
        assert_eq!(diagnostics.len(), 49);
        diagnostics.insert(10, make_diagnostic(51, 3, 7, "unusedAnchor"));

        let actions = batch_equals_singles(&text, &diagnostics, whole_document());

        let expected: Vec<String> = (0..50)
            .filter(|i| *i != 25)
            .map(|i| text.replacen(&format!("&a{i} "), "", 1))
            .collect();
        let applied: Vec<String> = actions.iter().map(|a| apply_action(&text, a)).collect();
        assert_eq!(applied, expected);
    }

    // Only the diagnostics on the lines of the requested range are answered.
    #[rstest]
    #[case::second_line(1, 1, &[1])]
    #[case::second_and_third_line(1, 2, &[1, 2])]
    #[case::before(0, 0, &[0])]
    #[case::past_the_end(5, 9, &[])]
    fn the_diagnostics_outside_the_requested_range_get_no_action(
        #[case] first: u32,
        #[case] last: u32,
        #[case] answered: &[usize],
    ) {
        let text = "a: &x 1\nb: &y 2\nc: &z 3\n";
        let range = Range::new(Position::new(first, 0), Position::new(last, 0));

        let actions = batch_equals_singles(text, &unused_anchor_diagnostics(text), range);

        let expected: Vec<String> = ["&x ", "&y ", "&z "]
            .iter()
            .enumerate()
            .filter(|(line, _)| answered.contains(line))
            .map(|(_, token)| text.replacen(token, "", 1))
            .collect();
        let applied: Vec<String> = actions.iter().map(|a| apply_action(text, a)).collect();
        assert_eq!(applied, expected);
    }

    // A stale diagnostic among valid ones offers nothing and disturbs nothing.
    #[test]
    fn a_stale_diagnostic_among_valid_ones_is_skipped() {
        let text = "a: &x 1\nb: &y 2\n";
        let all = unused_anchor_diagnostics(text);
        let diagnostics = [
            all[0].clone(),
            make_diagnostic(0, 6, 7, "unusedAnchor"),
            make_diagnostic(u32::MAX, 0, 1, "unusedAnchor"),
            all[1].clone(),
        ];
        let range = Range::new(Position::new(0, 0), Position::new(u32::MAX, 0));

        let actions = batch_equals_singles(text, &diagnostics, range);

        let applied: Vec<String> = actions.iter().map(|a| apply_action(text, a)).collect();
        assert_eq!(applied, ["a: 1\nb: &y 2\n", "a: &x 1\nb: 2\n"]);
    }
}
