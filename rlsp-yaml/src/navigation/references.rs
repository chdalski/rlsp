// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Pos, Span};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

type NamedSpans = Vec<(String, Span)>;

/// Go-to-definition: cursor on `*alias` returns the location of the matching `&anchor`.
///
/// Returns `None` if the cursor is not on an alias, the anchor is not found,
/// or `docs` is empty.
#[must_use]
pub fn goto_definition(docs: &[Document<Span>], uri: &Url, position: Position) -> Option<Location> {
    let cursor = lsp_to_pos(position);
    let (doc, idx) = containing_document(docs, cursor)?;

    let (anchors, aliases) = collect_anchor_alias_entries(doc);

    let alias_entry = aliases
        .iter()
        .find(|(_, loc)| span_contains(*loc, cursor, idx))?;
    let alias_name = &alias_entry.0;

    let anchor_entry = anchors.iter().find(|(name, _)| name == alias_name)?;

    Some(Location {
        uri: uri.clone(),
        range: span_to_range(anchor_entry.1, idx),
    })
}

/// Find references: cursor on `&anchor` or `*alias` returns all `*alias` usage locations.
///
/// When `include_declaration` is true, the `&anchor` definition is also included.
/// Returns an empty list if the cursor is not on an anchor or alias, or `docs` is empty.
#[must_use]
pub fn find_references(
    docs: &[Document<Span>],
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Vec<Location> {
    let cursor = lsp_to_pos(position);
    let Some((doc, idx)) = containing_document(docs, cursor) else {
        return Vec::new();
    };

    let (anchors, aliases) = collect_anchor_alias_entries(doc);

    let name = anchors
        .iter()
        .find(|(_, loc)| span_contains(*loc, cursor, idx))
        .map(|(n, _)| n.as_str())
        .or_else(|| {
            aliases
                .iter()
                .find(|(_, loc)| span_contains(*loc, cursor, idx))
                .map(|(n, _)| n.as_str())
        });

    let Some(name) = name else {
        return Vec::new();
    };

    let declaration = if include_declaration {
        anchors
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, loc)| Location {
                uri: uri.clone(),
                range: span_to_range(*loc, idx),
            })
    } else {
        None
    };

    let alias_locations = aliases
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, loc)| Location {
            uri: uri.clone(),
            range: span_to_range(*loc, idx),
        });

    declaration.into_iter().chain(alias_locations).collect()
}

/// Walk a single document and collect `(name, span)` pairs for every anchor
/// token and every alias token.
///
/// Anchor spans come from `Node::*.anchor_loc` (the `&name` token span).
/// Alias spans come from `Node::Alias.loc` (the `*name` token span).
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

/// Find the document whose root span contains the cursor (per-document scoping).
fn containing_document(
    docs: &[Document<Span>],
    cursor: Pos,
) -> Option<(&Document<Span>, &LineIndex)> {
    docs.iter().find_map(|doc| {
        let idx = doc.line_index();
        if span_contains(node_loc(&doc.root), cursor, idx) {
            Some((doc, idx))
        } else {
            None
        }
    })
}

/// Returns the location span of a node.
const fn node_loc(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

/// Returns `true` when `cursor` is within `span` using half-open `[start, end)`.
fn span_contains(span: Span, cursor: Pos, idx: &LineIndex) -> bool {
    let start = (
        idx.line_column(span.start).0 as usize,
        idx.line_column(span.start).1 as usize,
    );
    let end = (
        idx.line_column(span.end).0 as usize,
        idx.line_column(span.end).1 as usize,
    );
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
fn span_to_range(loc: Span, idx: &LineIndex) -> Range {
    Range::new(
        Position::new(
            idx.line_column(loc.start).0.saturating_sub(1),
            idx.line_column(loc.start).1,
        ),
        Position::new(
            idx.line_column(loc.end).0.saturating_sub(1),
            idx.line_column(loc.end).1,
        ),
    )
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::expect_used, reason = "test code")]
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

    // ---- Go-to-Definition: Returns anchor line (GT-1) ----

    #[rstest]
    #[case::jumps_to_anchor_definition(
        "defaults: &defaults\n  adapter: postgres\nproduction:\n  <<: *defaults\n",
        3,
        6,
        0
    )]
    #[case::multiple_anchors_jumps_to_correct_one(
        "a: &first\n  key: val\nb: &second\n  key: val\nc:\n  ref: *second\n",
        5,
        7,
        2
    )]
    #[case::jump_within_same_document(
        "---\ndefaults: &defaults\n  key: val\nproduction:\n  <<: *defaults\n",
        4,
        6,
        1
    )]
    #[case::ignores_anchor_in_comment("# &fake\nreal: &real val\nref: *real\n", 2, 5, 1)]
    fn goto_definition_returns_anchor_line(
        #[case] text: &str,
        #[case] line: u32,
        #[case] character: u32,
        #[case] expected_anchor_line: u32,
    ) {
        let uri = test_uri();
        let result = goto_definition(&parse(text), &uri, pos(line, character));
        let loc = result.expect("should return a location");
        assert_eq!(loc.range.start.line, expected_anchor_line);
    }

    // GT-2: exact range assertion
    #[test]
    fn goto_definition_returns_correct_range_exact() {
        let text = "defaults: &defaults\n  adapter: postgres\nproduction:\n  <<: *defaults\n";
        let uri = test_uri();
        let result = goto_definition(&parse(text), &uri, pos(3, 6));

        let loc = result.expect("should return a location");
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(
            loc.range.start.character, 10,
            "anchor '&defaults' starts at column 10"
        );
        assert_eq!(
            loc.range.end.character, 19,
            "anchor '&defaults' ends at column 19"
        );
    }

    // ---- Go-to-Definition: Edge Cases (returns None) — GT-3 ----

    #[rstest]
    #[case::cursor_not_on_alias("key: value\n", 0, 0)]
    #[case::cursor_on_anchor_not_alias("defaults: &defaults\n  key: value\n", 0, 10)]
    #[case::alias_has_no_matching_anchor("production:\n  <<: *undefined\n", 1, 6)]
    #[case::empty_document("", 0, 0)]
    #[case::beyond_document_lines("key: &anchor value\n", 10, 0)]
    #[case::beyond_line_length("key: &anchor value\n", 0, 100)]
    #[case::not_across_document_boundaries(
        "doc1: &shared\n  key: val\n---\ndoc2:\n  ref: *shared\n",
        4,
        7
    )]
    #[case::ampersand_in_non_anchor_context("formula: a & b\nref: *undefined\n", 0, 11)]
    fn goto_definition_returns_none(#[case] text: &str, #[case] line: u32, #[case] character: u32) {
        let uri = test_uri();
        let result = goto_definition(&parse(text), &uri, pos(line, character));
        assert!(result.is_none());
    }

    // GT-4: anchor on a collection value — span is anchor token, not collection body
    #[test]
    fn goto_definition_anchor_on_mapping_value() {
        let text = "base: &base\n  key: val\nchild:\n  <<: *base\n";
        let uri = test_uri();
        let result = goto_definition(&parse(text), &uri, pos(3, 6));

        let loc = result.expect("should return a location");
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(loc.range.start.character, 6, "&base starts at col 6");
        assert_eq!(loc.range.end.character, 11, "&base ends at col 11");
    }

    // GT-5: cursor at first character of alias (on `*`)
    #[test]
    fn goto_definition_cursor_at_first_char_of_alias_name() {
        let text = "x: &anchor\nref: *anchor\n";
        let uri = test_uri();
        let result = goto_definition(&parse(text), &uri, pos(1, 5));
        let loc = result.expect("should return a location");
        assert_eq!(loc.range.start.line, 0);
    }

    // GT-6: cursor at last character inside alias token span
    #[test]
    fn goto_definition_cursor_at_last_char_of_alias_name() {
        let text = "x: &anchor\nref: *anchor\n";
        let uri = test_uri();
        // *anchor alias span is [5, 12) — col 11 is the last char inside the span
        let result = goto_definition(&parse(text), &uri, pos(1, 11));
        assert!(result.is_some());
    }

    // GT-7: cursor one past end of alias token — half-open span [start, end)
    #[test]
    fn goto_definition_cursor_one_past_alias_name_returns_none() {
        let text = "x: &anchor\nref: *anchor\n";
        let uri = test_uri();
        // *anchor alias span is [5, 12) — col 12 is past the end (half-open)
        let result = goto_definition(&parse(text), &uri, pos(1, 12));
        assert!(result.is_none());
    }

    // GT-8: UTF-8 anchor name
    #[test]
    fn goto_definition_utf8_anchor_name() {
        // "résumé" = 6 chars, anchor is `&résumé` at col 3 on "x: &résumé"
        // col 3 = `&`, cols 4-9 = r,é,s,u,m,é → end col = 10
        let text = "x: &résumé\nref: *résumé\n";
        let uri = test_uri();
        // alias `*résumé` is on line 1; cursor anywhere inside it
        let result = goto_definition(&parse(text), &uri, pos(1, 5));
        let loc = result.expect("should return a location");
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(loc.range.start.character, 3, "&résumé starts at col 3");
        assert_eq!(
            loc.range.end.character, 10,
            "&résumé ends at col 10 (3 + 7 codepoints)"
        );
    }

    // ---- Find References: Happy Path ----

    #[test]
    fn should_find_all_alias_references_for_anchor() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(0, 10), false);

        assert_eq!(result.len(), 2, "should find 2 alias references");
        let lines: Vec<u32> = result.iter().map(|l| l.range.start.line).collect();
        assert!(lines.contains(&3), "should include *shared on line 3");
        assert!(lines.contains(&5), "should include *shared on line 5");
    }

    // Cursor-on-alias with include_declaration=false — exercises the aliases.find branch
    // combined with the include_declaration=false path; no other test covers this combination.
    #[test]
    fn find_references_cursor_on_alias_excludes_declaration() {
        let text = "defaults: &shared\n  key: val\ndev:\n  <<: *shared\nprod:\n  <<: *shared\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(3, 6), false);

        assert_eq!(result.len(), 2);
        let lines: Vec<u32> = result.iter().map(|l| l.range.start.line).collect();
        assert!(lines.contains(&3));
        assert!(lines.contains(&5));
        assert!(
            !lines.contains(&0),
            "anchor excluded when include_declaration=false"
        );
    }

    // ---- Find References: Edge Cases ----

    #[rstest]
    #[case::cursor_not_on_anchor_or_alias("key: value\n", 0, 0, false)]
    #[case::anchor_has_no_alias_usages("defaults: &lonely\n  key: val\n", 0, 10, false)]
    #[case::empty_document("", 0, 0, false)]
    #[case::beyond_document_lines("key: &anchor value\n", 10, 0, false)]
    #[case::beyond_line_length("key: &anchor value\n", 0, 100, false)]
    fn find_references_returns_empty(
        #[case] text: &str,
        #[case] line: u32,
        #[case] character: u32,
        #[case] include_declaration: bool,
    ) {
        let uri = test_uri();
        let result = find_references(
            &parse(text),
            &uri,
            pos(line, character),
            include_declaration,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn should_return_only_declaration_when_anchor_has_no_usages_and_include_declaration_true() {
        let text = "defaults: &lonely\n  key: val\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(0, 10), true);

        assert_eq!(
            result.len(),
            1,
            "should return exactly 1 location (the anchor itself)"
        );
        assert_eq!(result[0].range.start.line, 0);
    }

    // ---- Find References: Multi-Document Scoping ----

    #[test]
    fn should_scope_references_to_same_document() {
        let text = "doc1: &name\n  ref: *name\n---\ndoc2: &name\n  ref: *name\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(0, 6), false);

        assert_eq!(
            result.len(),
            1,
            "should find only 1 alias reference in document 1"
        );
        assert_eq!(
            result[0].range.start.line, 1,
            "the reference should be on line 1 (document 1)"
        );
    }

    // FR-9: include_declaration: true with cursor on alias
    #[test]
    fn find_references_include_declaration_cursor_on_alias() {
        let text = "x: &alias\nref1: *alias\nref2: *alias\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(1, 6), true);

        assert_eq!(result.len(), 3, "anchor + 2 aliases");
        let lines: Vec<u32> = result.iter().map(|l| l.range.start.line).collect();
        assert!(lines.contains(&0), "anchor at line 0");
        assert!(lines.contains(&1), "alias at line 1");
        assert!(lines.contains(&2), "alias at line 2");
    }

    // FR-10: anchor on collection — anchor_loc span, not collection body
    #[test]
    fn find_references_anchor_on_collection_not_expanded() {
        let text = "base: &base\n  key: val\nchild:\n  <<: *base\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(0, 6), true);

        assert_eq!(result.len(), 2, "anchor + 1 alias");
        let anchor_loc = result
            .iter()
            .find(|l| l.range.start.line == 0)
            .expect("anchor location");
        assert_eq!(anchor_loc.range.start.character, 6, "&base starts at col 6");
        assert_eq!(anchor_loc.range.end.character, 11, "&base ends at col 11");
    }

    // FR-11: cursor at first character of anchor token
    #[test]
    fn find_references_cursor_at_first_char_of_anchor_token() {
        let text = "x: &anchor\nref: *anchor\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(0, 3), false);
        assert!(
            !result.is_empty(),
            "cursor on & of &anchor should find references"
        );
    }

    // FR-12: cursor one past end of anchor token — should return empty
    #[test]
    fn find_references_cursor_one_past_anchor_token_returns_empty() {
        let text = "x: &anchor\nref: *anchor\n";
        let uri = test_uri();
        // &anchor is at cols [3,10), col 10 is past the end
        let result = find_references(&parse(text), &uri, pos(0, 10), false);
        assert!(result.is_empty());
    }

    // FR-13: UTF-8 anchor name lookup resolves correctly
    #[test]
    fn find_references_utf8_anchor_lookup_resolves() {
        let text = "x: &résumé\nref1: *résumé\nref2: *résumé\n";
        let uri = test_uri();
        let result = find_references(&parse(text), &uri, pos(0, 3), false);

        assert_eq!(result.len(), 2, "both aliases found");
        // Verify alias range columns use codepoint counts
        for loc in &result {
            // *résumé on line 1 starts at col 6 ("ref1: " = 6 chars), ends at col 13 (6 + 7)
            // *résumé on line 2 starts at col 6 ("ref2: " = 6 chars), ends at col 13 (6 + 7)
            assert_eq!(loc.range.start.character, 6, "alias starts at col 6");
            assert_eq!(loc.range.end.character, 13, "alias ends at col 13");
        }
    }
}
