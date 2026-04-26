// SPDX-License-Identifier: MIT

use std::collections::HashMap;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{Pos, Span};
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

/// Prepare rename: validates cursor is on anchor/alias, returns name range.
///
/// Returns `None` if the cursor is not on an anchor or alias, or `docs` is empty.
#[must_use]
pub fn prepare_rename(docs: &[Document<Span>], position: Position) -> Option<Range> {
    let cursor = lsp_to_pos(position);
    let doc = containing_document(docs, cursor)?;
    let (anchors, aliases) = collect_anchor_alias_entries(doc);

    anchors
        .iter()
        .find(|(_, loc)| span_contains(*loc, cursor))
        .or_else(|| aliases.iter().find(|(_, loc)| span_contains(*loc, cursor)))
        .map(|(_, loc)| span_to_range(*loc))
}

/// Rename: returns edits for all occurrences of anchor and aliases.
///
/// Returns `None` if the cursor is not on an anchor or alias, the new name
/// is invalid, or `docs` is empty.
#[must_use]
pub fn rename(
    docs: &[Document<Span>],
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    if !is_valid_anchor_name(new_name) {
        return None;
    }

    let cursor = lsp_to_pos(position);
    let doc = containing_document(docs, cursor)?;
    let (anchors, aliases) = collect_anchor_alias_entries(doc);

    let name = anchors
        .iter()
        .find(|(_, loc)| span_contains(*loc, cursor))
        .map(|(n, _)| n.as_str())
        .or_else(|| {
            aliases
                .iter()
                .find(|(_, loc)| span_contains(*loc, cursor))
                .map(|(n, _)| n.as_str())
        })?;

    let anchor_edits = anchors
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, loc)| TextEdit {
            range: span_to_range(*loc),
            new_text: format!("&{new_name}"),
        });

    let alias_edits = aliases
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, loc)| TextEdit {
            range: span_to_range(*loc),
            new_text: format!("*{new_name}"),
        });

    let edits: Vec<TextEdit> = anchor_edits.chain(alias_edits).collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..WorkspaceEdit::default()
    })
}

/// Validate that a proposed new anchor name is valid.
fn is_valid_anchor_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 256 && name.chars().all(is_anchor_name_char)
}

/// Check if a character is valid in a YAML anchor/alias name.
const fn is_anchor_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.'
}

type NamedSpans = Vec<(String, Span)>;

/// Walk a single document and collect `(name, span)` pairs for every anchor
/// token and every alias token.
fn collect_anchor_alias_entries(doc: &Document<Span>) -> (NamedSpans, NamedSpans) {
    let mut anchors = Vec::new();
    let mut aliases = Vec::new();
    collect_node(&doc.root, &mut anchors, &mut aliases);
    (anchors, aliases)
}

fn collect_node(node: &Node<Span>, anchors: &mut NamedSpans, aliases: &mut NamedSpans) {
    match node {
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } => {
            if let (Some(name), Some(loc)) = (node.anchor(), node.anchor_loc()) {
                anchors.push((name.to_owned(), loc));
            }
        }
        Node::Alias { name, loc, .. } => {
            aliases.push((name.clone(), *loc));
        }
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                collect_node(k, anchors, aliases);
                collect_node(v, anchors, aliases);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node(item, anchors, aliases);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Find the document whose root span contains the cursor.
fn containing_document(docs: &[Document<Span>], cursor: Pos) -> Option<&Document<Span>> {
    docs.iter()
        .find(|doc| span_contains(node_loc(&doc.root), cursor))
}

const fn node_loc(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

fn span_contains(span: Span, cursor: Pos) -> bool {
    let start = (span.start.line, span.start.column);
    let end = (span.end.line, span.end.column);
    let pos = (cursor.line, cursor.column);
    pos >= start && pos < end
}

/// Convert an LSP `Position` (0-based line, 0-based character) to a parser `Pos`
/// (1-based line, 0-based column).
const fn lsp_to_pos(position: Position) -> Pos {
    Pos {
        byte_offset: 0,
        line: position.line as usize + 1,
        column: position.character as usize,
    }
}

/// Convert a parser `Span` to an LSP `Range`.
fn span_to_range(loc: Span) -> Range {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    Range::new(
        Position::new(
            loc.start.line.saturating_sub(1) as u32,
            loc.start.column as u32,
        ),
        Position::new(loc.end.line.saturating_sub(1) as u32, loc.end.column as u32),
    )
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_utils::{parse_docs, test_uri};

    fn parse(yaml: &str) -> Vec<Document<Span>> {
        parse_docs(yaml)
    }

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    // ---- prepare_rename: Happy Path ----

    // PR-1
    #[test]
    fn should_return_range_when_cursor_on_anchor() {
        let docs = parse("key: &myanchor value\n");
        let result = prepare_rename(&docs, pos(0, 6));

        let range = result.expect("should return a range");
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 5, "&myanchor starts at column 5");
        assert_eq!(range.end.character, 14, "&myanchor ends at column 14");
    }

    // PR-2
    #[test]
    fn should_return_range_when_cursor_on_alias() {
        let docs = parse("defaults: &defaults\n  key: val\nproduction:\n  <<: *defaults\n");
        let result = prepare_rename(&docs, pos(3, 7));

        let range = result.expect("should return a range");
        assert_eq!(range.start.line, 3);
        assert!(range.start.character <= 7);
        assert!(range.end.character > 7);
    }

    // PR-3
    #[test]
    fn should_return_range_when_cursor_at_end_of_anchor_name() {
        let docs = parse("key: &anchor value\n");
        // &anchor is at col [5,12) — col 11 is the last char inside the span
        let result = prepare_rename(&docs, pos(0, 11));

        let range = result.expect("should return a range");
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 5);
    }

    // PR-4
    #[test]
    fn prepare_rename_cursor_at_first_char_of_anchor() {
        let docs = parse("x: &anchor\n");
        // &anchor at col [3,10)
        let result = prepare_rename(&docs, pos(0, 3));
        assert!(result.is_some());
    }

    // PR-5
    #[test]
    fn prepare_rename_cursor_one_past_anchor_token_returns_none() {
        let docs = parse("x: &anchor\n");
        // &anchor is at cols [3,10) — col 10 is past the end (half-open)
        let result = prepare_rename(&docs, pos(0, 10));
        assert!(result.is_none());
    }

    // PR-6
    #[test]
    fn prepare_rename_cursor_in_middle_of_anchor_name_returns_full_range() {
        let docs = parse("key: &longname value\n");
        // &longname starts at col 5, ends at col 13 (8 chars: & + longname)
        let result = prepare_rename(&docs, pos(0, 8));

        let range = result.expect("should return a range");
        assert_eq!(range.start.character, 5, "&longname starts at col 5");
        assert_eq!(
            range.end.character, 14,
            "&longname ends at col 14 (& + 8 chars)"
        );
    }

    // PR-7 — Edge cases
    #[rstest]
    #[case::not_on_anchor_or_alias("key: value\n", 0, 0)]
    #[case::empty_document("", 0, 0)]
    #[case::beyond_document_lines("key: &anchor value\n", 10, 0)]
    #[case::beyond_line_length("key: &anchor value\n", 0, 100)]
    #[case::anchor_in_comment("# &fake\nkey: value\n", 0, 2)]
    #[case::cursor_at_exact_end_of_line_not_in_token("key: &anchor\n", 0, 12)]
    #[case::cursor_at_document_end_not_in_token("key: &anchor", 0, 12)]
    fn prepare_rename_returns_none(#[case] text: &str, #[case] line: u32, #[case] character: u32) {
        let docs = parse(text);
        let result = prepare_rename(&docs, pos(line, character));
        assert!(result.is_none());
    }

    // ---- rename: Happy Path ----

    // RN-1
    #[rstest]
    #[case::anchor_and_single_alias(
        "defaults: &old\n  key: val\nproduction:\n  <<: *old\n",
        0,
        10,
        "new",
        2
    )]
    #[case::anchor_and_multiple_aliases(
        "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n",
        0,
        10,
        "common",
        3
    )]
    #[case::cursor_on_alias(
        "defaults: &old\n  key: val\nproduction:\n  <<: *old\n",
        3,
        7,
        "new",
        2
    )]
    #[case::anchor_with_no_aliases("key: &lonely value\n", 0, 5, "orphan", 1)]
    #[case::not_across_document_boundaries(
        "doc1: &name\n  ref: *name\n---\ndoc2: &name\n  ref: *name\n",
        0,
        6,
        "renamed",
        2
    )]
    #[case::within_second_document(
        "doc1: &name\n---\ndoc2: &name\n  ref: *name\n",
        2,
        6,
        "other",
        2
    )]
    fn rename_returns_edits_len(
        #[case] text: &str,
        #[case] line: u32,
        #[case] character: u32,
        #[case] new_name: &str,
        #[case] expected_len: usize,
    ) {
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(line, character), new_name);
        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), expected_len);
    }

    // RN-2
    #[test]
    fn should_produce_correct_edit_ranges() {
        let text = "key: &old value\nref: *old\n";
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(0, 5), "new");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2);

        // anchor edit
        let anchor_edit = edits
            .iter()
            .find(|e| e.new_text == "&new")
            .expect("anchor edit");
        assert_eq!(anchor_edit.range.start.line, 0);
        assert_eq!(anchor_edit.range.start.character, 5);
        assert_eq!(anchor_edit.range.end.line, 0);
        assert_eq!(anchor_edit.range.end.character, 9);

        // alias edit
        let alias_edit = edits
            .iter()
            .find(|e| e.new_text == "*new")
            .expect("alias edit");
        assert_eq!(alias_edit.range.start.line, 1);
        assert_eq!(alias_edit.range.start.character, 5);
        assert_eq!(alias_edit.range.end.line, 1);
        assert_eq!(alias_edit.range.end.character, 9);
    }

    // RN-3
    #[test]
    fn rename_anchor_on_mapping_collection_edits_token_span_not_body() {
        // &d annotates a mapping; anchor_loc must be the &d token, not the mapping body
        let text = "defaults: &d\n  k: v\nref: *d\n";
        let uri = test_uri();
        let docs = parse(text);
        // anchor_loc for &d: col 10-12 on line 0
        let result = rename(&docs, &uri, pos(0, 10), "renamed");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2);

        let anchor_edit = edits
            .iter()
            .find(|e| e.new_text == "&renamed")
            .expect("anchor edit");
        // token &d is on line 0
        assert_eq!(
            anchor_edit.range.start.line, 0,
            "anchor edit must be on line 0 (token), not the mapping body"
        );
        assert_eq!(anchor_edit.range.start.character, 10, "&d starts at col 10");
        assert_eq!(anchor_edit.range.end.character, 12, "&d ends at col 12");
    }

    // RN-4
    #[test]
    fn rename_anchor_on_sequence_edits_token_span_not_body() {
        // &seq annotates a sequence; anchor_loc must be the &seq token, not the sequence body
        let text = "items: &seq\n  - a\n  - b\nref: *seq\n";
        let uri = test_uri();
        let docs = parse(text);
        // anchor_loc for &seq: col 7-11 on line 0
        let result = rename(&docs, &uri, pos(0, 7), "renamed");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2);

        let anchor_edit = edits
            .iter()
            .find(|e| e.new_text == "&renamed")
            .expect("anchor edit");
        assert_eq!(
            anchor_edit.range.start.line, 0,
            "anchor edit must be on line 0 (token)"
        );
        assert_eq!(anchor_edit.range.start.character, 7, "&seq starts at col 7");
        assert_eq!(anchor_edit.range.end.character, 11, "&seq ends at col 11");
    }

    // RN-5
    #[test]
    fn rename_does_not_cross_document_boundary_to_second_doc() {
        let text = "doc1: &name\n  ref: *name\n---\ndoc2: &name\n  ref: *name\n";
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(0, 6), "renamed");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2, "only doc 1 edits");
        for e in edits {
            assert!(
                e.range.start.line < 3,
                "no edit should be on doc 2 lines (line {})",
                e.range.start.line
            );
        }
    }

    // RN-6
    #[test]
    fn rename_does_not_cross_document_boundary_to_first_doc() {
        let text = "doc1: &name\n  ref: *name\n---\ndoc2: &name\n  ref: *name\n";
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(3, 6), "renamed");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2, "only doc 2 edits");
        for e in edits {
            assert!(
                e.range.start.line >= 3,
                "no edit should be on doc 1 lines (line {})",
                e.range.start.line
            );
        }
    }

    // RN-7
    #[test]
    fn rename_utf8_anchor_name_produces_correct_column_ranges() {
        // x: &résumé\nref: *résumé\n
        // anchor_loc: start col=3, end col=10 (& + résumé = 7 codepoints)
        // alias loc: start col=5, end col=12
        let text = "x: &résumé\nref: *résumé\n";
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(0, 3), "newname");

        let edit = result.expect("should return WorkspaceEdit");
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert_eq!(edits.len(), 2);

        let anchor_edit = edits
            .iter()
            .find(|e| e.new_text == "&newname")
            .expect("anchor edit");
        assert_eq!(
            anchor_edit.range.start.character, 3,
            "anchor starts at col 3"
        );
        assert_eq!(
            anchor_edit.range.end.character, 10,
            "anchor ends at col 10 (3 + 7 codepoints)"
        );

        let alias_edit = edits
            .iter()
            .find(|e| e.new_text == "*newname")
            .expect("alias edit");
        assert_eq!(alias_edit.range.start.character, 5, "alias starts at col 5");
        assert_eq!(
            alias_edit.range.end.character, 12,
            "alias ends at col 12 (5 + 7 codepoints)"
        );
    }

    // ---- rename: Invalid Position Cases ----

    // RN-8
    #[rstest]
    #[case::cursor_not_on_anchor_or_alias("key: value\n", 0, 0, "anything")]
    #[case::empty_document("", 0, 0, "anything")]
    #[case::beyond_document_lines("key: &anchor value\n", 10, 0, "anything")]
    #[case::beyond_line_length("key: &anchor value\n", 0, 100, "anything")]
    fn rename_returns_none_invalid_position(
        #[case] text: &str,
        #[case] line: u32,
        #[case] character: u32,
        #[case] new_name: &str,
    ) {
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(line, character), new_name);
        assert!(result.is_none());
    }

    // ---- rename: Invalid new_name Validation (Security Cases) ----

    // RN-9
    #[rstest]
    #[case::empty_name("")]
    #[case::spaces("has space")]
    #[case::open_bracket("bad[name")]
    #[case::close_bracket("bad]name")]
    #[case::open_brace("bad{name")]
    #[case::close_brace("bad}name")]
    #[case::colon("bad:name")]
    #[case::comma("bad,name")]
    #[case::whitespace_only("   ")]
    #[case::hash("name#comment")]
    #[case::newline("name\n")]
    #[case::tab("name\t")]
    #[case::carriage_return("name\r")]
    #[case::ampersand("name&other")]
    #[case::asterisk("name*other")]
    #[case::exclamation("name!tag")]
    fn rename_rejects_invalid_new_name(#[case] new_name: &str) {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(0, 5), new_name);
        assert!(result.is_none());
    }

    // ---- rename: Valid new_name Validation ----

    // RN-10
    #[rstest]
    #[case::hyphen("valid-name")]
    #[case::underscore("valid_name")]
    #[case::dot("valid.name")]
    #[case::starts_with_digit("123abc")]
    fn rename_accepts_valid_new_name(#[case] new_name: &str) {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let docs = parse(text);
        let result = rename(&docs, &uri, pos(0, 5), new_name);
        assert!(result.is_some());
    }

    // RN-11
    #[test]
    fn should_reject_new_name_exceeding_max_length() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let docs = parse(text);
        let long_name = "a".repeat(257);
        let result = rename(&docs, &uri, pos(0, 5), &long_name);
        assert!(
            result.is_none(),
            "name longer than 256 chars must be rejected"
        );
    }

    // RN-12
    #[test]
    fn should_accept_new_name_at_exactly_max_length() {
        let text = "key: &anchor value\n";
        let uri = test_uri();
        let docs = parse(text);
        let max_name = "a".repeat(256);
        let result = rename(&docs, &uri, pos(0, 5), &max_name);
        assert!(
            result.is_some(),
            "name of exactly 256 chars must be accepted"
        );
    }
}
