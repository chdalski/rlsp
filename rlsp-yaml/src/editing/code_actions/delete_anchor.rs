// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Range, TextEdit};

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{CollectionStyle, Document, LineIndex, ScalarStyle, Span};

use crate::lsp_util::{offset_to_lsp, span_to_lsp};
use crate::navigation::collect_anchor_alias_entries;

use super::make_action;

/// Offer to delete the `&name` token an unused-anchor diagnostic ranges over.
///
/// The diagnostic only selects the anchor: the edit is computed from the
/// anchor's span in the AST and the text around it, and nothing is offered when
/// the diagnostic matches no anchor of `docs` (it is stale), when the anchor has
/// an alias in the meantime, or when deleting it would break the document.
///
/// The whole of each document is read: how deep the documents nest is bounded
/// where they are loaded.
pub(super) fn delete_unused_anchor(
    docs: &[Document<Span>],
    text: &str,
    diag: &Diagnostic,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<CodeAction> {
    let (doc, anchor) = docs.iter().find_map(|doc| {
        find_anchor(&doc.root, diag.range, doc.line_index(), false).map(|found| (doc, found))
    })?;
    // A document without `---` whose only content is the anchored empty node
    // would vanish from the stream with its token.
    let bare_empty_root = !doc.explicit_start
        && is_empty_node(&doc.root)
        && doc.root.anchor_loc() == Some(anchor.loc);
    if !anchor.removable || bare_empty_root || has_alias(doc, anchor.name) {
        return None;
    }

    let start = usize::try_from(anchor.loc.start).ok()?;
    let end = usize::try_from(anchor.loc.end).ok()?;
    if text.get(start..end)?.strip_prefix('&')? != anchor.name {
        return None;
    }
    let (delete_start, delete_end) = deletion_range(text, start, end)?;

    let idx = doc.line_index();
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

/// The anchor an unused-anchor diagnostic ranges over.
struct FoundAnchor<'a> {
    name: &'a str,
    /// Span of the `&name` token.
    loc: Span,
    /// Whether the document stays valid without the token: a flow collection
    /// entry that is an empty node needs the anchor to exist (`[&a, b]`).
    removable: bool,
}

/// Find the node whose anchor token spans exactly `range`.
///
/// `needs_node` is whether `node` is the whole of an entry of a flow
/// collection: a flow sequence item, or the key of a flow mapping entry whose
/// value is empty too.
fn find_anchor<'a>(
    node: &'a Node<Span>,
    range: Range,
    idx: &LineIndex,
    needs_node: bool,
) -> Option<FoundAnchor<'a>> {
    if let (Some(name), Some(loc)) = (node.anchor(), node.anchor_loc())
        && span_to_lsp(loc, idx) == range
    {
        return Some(FoundAnchor {
            name,
            loc,
            removable: !(needs_node && is_empty_node(node)),
        });
    }
    match node {
        Node::Mapping { entries, style, .. } => {
            let in_flow = *style == CollectionStyle::Flow;
            entries.iter().find_map(|(key, value)| {
                // `{&a : v}` keeps its entry without the anchor, `{&a, b}` does not.
                let key_needs_node = in_flow && is_empty_node(value);
                find_anchor(key, range, idx, key_needs_node)
                    .or_else(|| find_anchor(value, range, idx, false))
            })
        }
        Node::Sequence { items, style, .. } => {
            let in_flow = *style == CollectionStyle::Flow;
            items
                .iter()
                .find_map(|item| find_anchor(item, range, idx, in_flow))
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    }
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

/// Whether `doc` holds an alias to `name`: the validator reports an anchor
/// unused only when it has none, and deleting an anchor with an alias would
/// leave the alias without one.
fn has_alias(doc: &Document<Span>, name: &str) -> bool {
    collect_anchor_alias_entries(doc)
        .1
        .iter()
        .any(|(alias, _)| *alias == name)
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
}
