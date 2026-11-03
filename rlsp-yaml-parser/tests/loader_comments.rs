// SPDX-License-Identifier: MIT
//
// Integration tests for comment storage in the loader AST: every `Comment`
// event of a loaded stream is stored in exactly one place.  The places are the
// four comment fields of `Document` and the `leading_comments` /
// `trailing_comment` of nodes.  These tests exercise the public `load()` API.

#![expect(clippy::unwrap_used, missing_docs, reason = "test code")]

use std::fmt::Write as _;

use rstest::rstest;

use rlsp_yaml_parser::loader::{LoadError, LoaderBuilder, load};
use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{Event, Span, parse_events};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Every stored comment with its place, per document: the document fields
/// first (`d0.prefix`, `d0.comments`), then the node places in document order
/// (`d0.lead:<label>`, `d0.trail:<label>`), then `d0.end` and `d0.after_end`.
/// A scalar's label is its escaped value, a collection's is `<map>` / `<seq>`,
/// an alias's is `*name`.
fn comment_places(docs: &[Document<Span>]) -> Vec<(String, String)> {
    fn label(node: &Node<Span>) -> String {
        match node {
            Node::Scalar { value, .. } => value.escape_debug().to_string(),
            Node::Mapping { .. } => "<map>".to_owned(),
            Node::Sequence { .. } => "<seq>".to_owned(),
            Node::Alias { name, .. } => format!("*{name}"),
        }
    }

    fn walk(doc: usize, node: &Node<Span>, out: &mut Vec<(String, String)>) {
        for comment in node.leading_comments() {
            out.push((format!("d{doc}.lead:{}", label(node)), comment.clone()));
        }
        if let Some(comment) = node.trailing_comment() {
            out.push((format!("d{doc}.trail:{}", label(node)), comment.to_owned()));
        }
        match node {
            Node::Mapping { entries, .. } => {
                for (key, value) in entries {
                    walk(doc, key, out);
                    walk(doc, value, out);
                }
            }
            Node::Sequence { items, .. } => {
                for item in items {
                    walk(doc, item, out);
                }
            }
            Node::Scalar { .. } | Node::Alias { .. } => {}
        }
    }

    let field = |out: &mut Vec<(String, String)>, doc: usize, name: &str, comments: &[String]| {
        out.extend(
            comments
                .iter()
                .map(|comment| (format!("d{doc}.{name}"), comment.clone())),
        );
    };

    let mut out = Vec::new();
    for (index, doc) in docs.iter().enumerate() {
        field(&mut out, index, "prefix", &doc.prefix_comments);
        field(&mut out, index, "comments", &doc.comments);
        walk(index, &doc.root, &mut out);
        field(&mut out, index, "end", &doc.end_comments);
        field(&mut out, index, "after_end", &doc.after_end_comments);
    }
    out
}

fn places(input: &str) -> Vec<(String, String)> {
    comment_places(&load(input).unwrap())
}

fn expected(places: &[(&str, &str)]) -> Vec<(String, String)> {
    places
        .iter()
        .map(|(place, text)| ((*place).to_owned(), (*text).to_owned()))
        .collect()
}

/// `count` comment lines `# c0`, `# c1`, ... at column 0.
fn comment_lines(count: usize) -> String {
    let mut out = String::new();
    for i in 0..count {
        writeln!(out, "# c{i}").unwrap();
    }
    out
}

fn comment_texts(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("# c{i}")).collect()
}

/// Runs `f` on a thread with a fixed small stack, so the stack budget does not
/// depend on the test runner's thread or `RUST_MIN_STACK`.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Place of each comment
// ---------------------------------------------------------------------------

// Each input has comments `# c`, `# d`, ... that are stored in exactly one
// place, which the table pins.
#[rstest]
// Comments on the line of a node's text.
#[case::double_quoted_value("a: \"q\" # c\n", &[("d0.trail:q", "# c")])]
#[case::single_quoted_value("a: 'q' # c\n", &[("d0.trail:q", "# c")])]
#[case::quoted_sequence_item("- \"q\" # c\n", &[("d0.trail:q", "# c")])]
#[case::plain_value("a: 1 # c\nb: 2\n", &[("d0.trail:1", "# c")])]
#[case::plain_item("- a # c\n- b\n", &[("d0.trail:a", "# c")])]
#[case::empty_comment_text("a: 1 #\n", &[("d0.trail:1", "#")])]
#[case::spaces_after_hash_are_kept("a: 1 #   spaced  \n", &[("d0.trail:1", "#   spaced  ")])]
#[case::non_ascii_text("a: 1 # ü\n", &[("d0.trail:1", "# ü")])]
#[case::no_newline_at_end("a: 1 # c", &[("d0.trail:1", "# c")])]
#[case::crlf("a: 1 # c\r\nb: 2\r\n", &[("d0.trail:1", "# c")])]
#[case::multi_line_plain_last_line("a:\n  v\n  w # c\n", &[("d0.trail:v w", "# c")])]
#[case::multi_line_flow_sequence("a: [1,\n 2] # c\nb: 1\n", &[("d0.trail:<seq>", "# c")])]
#[case::multi_line_flow_mapping("a: {x: 1,\n y: 2} # c\nb: 1\n", &[("d0.trail:<map>", "# c")])]
#[case::alias_value("x: &x 1\na: *x # c\n", &[("d0.trail:*x", "# c")])]
#[case::root_flow_sequence("[a, b] # c\n", &[("d0.trail:<seq>", "# c")])]
#[case::explicit_key_value("? k\n: v # c\n", &[("d0.trail:v", "# c")])]
#[case::explicit_key_on_key_line("? k # c\n: v\n", &[("d0.trail:k", "# c")])]
// A comment after a key on the key's line is the key's trailing comment.
#[case::key_without_value("k: # c\n", &[("d0.trail:k", "# c")])]
#[case::key_without_value_before_next_key("k: # c\nj: 1\n", &[("d0.trail:k", "# c")])]
#[case::anchor_before_nested_mapping("a: &x # c\n  b: 1\n", &[("d0.trail:a", "# c")])]
#[case::tag_before_nested_mapping("a: !t # c\n  b: 1\n", &[("d0.trail:a", "# c")])]
#[case::anchor_and_tag_before_nested_sequence("a: &x !t # c\n  - 1\n", &[("d0.trail:a", "# c")])]
// A comment on a block scalar's header line is the scalar's trailing comment.
#[case::literal_header("a: | # c\n  t\n", &[("d0.trail:t\\n", "# c")])]
#[case::folded_header("a: > # c\n  t\n", &[("d0.trail:t\\n", "# c")])]
#[case::literal_header_with_chomping("a: |- # c\n  t\n", &[("d0.trail:t", "# c")])]
#[case::literal_item_header("- | # c\n  t\n- x\n", &[("d0.trail:t\\n", "# c")])]
#[case::root_literal_header("--- | # c\n  t\n", &[("d0.trail:t\\n", "# c")])]
#[case::header_comment_and_own_line_comment_below(
    "a: | # h\n  t\n# d\nb: 1\n",
    &[("d0.trail:t\\n", "# h"), ("d0.lead:b", "# d")]
)]
#[case::hash_in_block_scalar_content_is_no_comment("a: |\n  t # x\n", &[])]
// A comment on the line of the next item's dash, or of the next explicit key's
// `?`, belongs to that item or key, not to the collection or the empty node
// that ends before it.
#[case::dash_of_second_item_after_mapping_item(
    "- a: 1\n- # d\n  b: 2\n",
    &[("d0.lead:<map>", "# d")]
)]
#[case::dash_of_second_item_after_sequence_item(
    "- - a\n- # d\n  b: 2\n",
    &[("d0.lead:<map>", "# d")]
)]
#[case::dash_of_second_item_in_nested_sequence(
    "a:\n  - b: 1\n  - # d\n    c: 2\nd: 3\n",
    &[("d0.lead:<map>", "# d")]
)]
#[case::dash_of_second_inner_item_after_mapping_item(
    "- - a: 1\n  - # d\n    b: 1\n",
    &[("d0.lead:<map>", "# d")]
)]
#[case::dash_after_empty_item("- # c\n- # d\n", &[("d0.lead:", "# c"), ("d0.lead:", "# d")])]
#[case::dash_after_tagged_empty_item(
    "- !!str # c\n- # d\n- x\n",
    &[("d0.lead:", "# c"), ("d0.lead:", "# d")]
)]
#[case::dash_after_anchored_empty_item(
    "- &a # c\n- # d\n",
    &[("d0.lead:", "# c"), ("d0.lead:", "# d")]
)]
#[case::question_mark_after_nested_mapping(
    "a:\n  y: 1\n? # d\n  z\n: 1\n",
    &[("d0.lead:z", "# d")]
)]
// A tag or an anchor alone is a node that ends with its properties: the comment
// lines below them belong to what follows, while a comment on their own line, or
// one before a node that continues on a later line, stays with the node.
#[case::comment_line_below_tag_only_item(
    "- !!str\n# d\n- x\n",
    &[("d0.lead:x", "# d")]
)]
#[case::comment_line_below_anchor_only_item(
    "- &x\n  # d\n- y\n",
    &[("d0.lead:y", "# d")]
)]
#[case::comment_lines_and_blank_lines_below_tag_only_item(
    "- !!str\n\n# d\n\n# e\n- x\n",
    &[("d0.lead:x", "# d"), ("d0.lead:x", "# e")]
)]
#[case::comment_on_the_properties_line_and_below(
    "- !!str # c\n# d\n- x\n",
    &[("d0.lead:", "# c"), ("d0.lead:x", "# d")]
)]
#[case::comment_below_properties_before_dash_comment(
    "- !!str\n# d\n- # e\n  a: 1\n",
    &[("d0.lead:<map>", "# d"), ("d0.lead:<map>", "# e")]
)]
#[case::comment_below_tag_only_item_in_nested_sequence(
    "a:\n  - &x\n  # d\n  - y\nz: 1\n",
    &[("d0.lead:y", "# d")]
)]
#[case::comment_below_last_tag_only_item("- !!str\n# d\n", &[("d0.end", "# d")])]
#[case::comment_below_last_tag_only_item_goes_to_next_key(
    "a:\n  - !!str\n  # d\nz: 1\n",
    &[("d0.lead:z", "# d")]
)]
// A bare dash is an empty item like a tag alone: the comment lines below it
// belong to what follows, or to a node that continues on a later line.
#[case::comment_below_bare_dash_before_value("-\n# c\n  b\n", &[("d0.lead:b", "# c")])]
#[case::comment_below_bare_dash_before_next_item("-\n  # c\n- x\n", &[("d0.lead:x", "# c")])]
#[case::comment_at_column_zero_below_bare_dash_before_next_item(
    "-\n# about x\n- x\n",
    &[("d0.lead:x", "# about x")]
)]
#[case::comment_lines_and_blank_lines_below_bare_dash(
    "-\n\n# c\n\n# d\n- x\n",
    &[("d0.lead:x", "# c"), ("d0.lead:x", "# d")]
)]
#[case::comment_below_last_bare_dash("-\n  # d\n", &[("d0.end", "# d")])]
#[case::comment_below_last_bare_dash_goes_to_next_key(
    "a:\n  -\n  # d\nz: 1\n",
    &[("d0.lead:z", "# d")]
)]
#[case::comment_on_the_properties_line_of_a_tag_only_item("- !!str # c\n- x\n", &[("d0.lead:", "# c")])]
#[case::comment_before_a_node_on_a_later_line(
    "- !!str\n# d\n  v\n- x\n",
    &[("d0.lead:v", "# d")]
)]
#[case::comment_below_tag_only_key("? !!str\n# d\n: 1\nb: 2\n", &[("d0.lead:b", "# d")])]
#[case::comment_below_tag_only_value("a: !!str\n# d\nb: 1\n", &[("d0.lead:b", "# d")])]
// A comment after an empty key on its line is the key's.
#[case::empty_key_before_nested_mapping(": # c\n  a: 1\n", &[("d0.trail:", "# c")])]
// The text of a multi-line quoted scalar ends on the line of its closing quote.
#[case::multi_line_double_quoted_value("a: \"x\n  y\" # c\nb: 1\n", &[("d0.trail:x y", "# c")])]
#[case::multi_line_single_quoted_item("- 'x\n  y' # c\n- b\n", &[("d0.trail:x y", "# c")])]
#[case::own_line_after_multi_line_quoted_value(
    "a: \"x\n  y\"\n# d\nb: 1\n",
    &[("d0.lead:b", "# d")]
)]
// Own-line comments and comments after an indicator belong to the next node.
#[case::dash_before_nested_mapping("- # c\n  a: 1\n", &[("d0.lead:<map>", "# c")])]
#[case::dash_of_second_item("- a\n- # c\n  b: 1\n", &[("d0.lead:<map>", "# c")])]
#[case::question_mark_before_value("? # c\n: v\n", &[("d0.lead:", "# c")])]
#[case::own_line_before_item("- a\n  # c\n- b\n", &[("d0.lead:b", "# c")])]
#[case::own_line_at_column_zero_before_item("- a\n# c\n- b\n", &[("d0.lead:b", "# c")])]
// A comment on the `:` line of an explicit key cannot be told from an own-line
// comment between the key and the value, so it follows that machinery.
#[case::colon_line_before_scalar_without_next_entry("? k\n: # c\n  v\n", &[("d0.end", "# c")])]
#[case::own_line_before_colon("? k\n# c\n: v\n", &[("d0.end", "# c")])]
#[case::own_line_before_colon_goes_to_next_key("? k\n# c\n: v\nb: 1\n", &[("d0.lead:b", "# c")])]
#[case::colon_line_before_scalar_goes_to_next_key("? k\n: # c\n  v\nb: 1\n", &[("d0.lead:b", "# c")])]
#[case::colon_line_before_nested_mapping("? k\n: # c\n  x: 1\n", &[("d0.lead:x", "# c")])]
// Comments around the document, in source order.
#[case::implicit_document_prefix("# c\na: 1\n", &[("d0.prefix", "# c")])]
#[case::prefix_before_marker("# c\n---\na: 1\n", &[("d0.prefix", "# c")])]
#[case::yaml_directive_line("%YAML 1.2 # c\n---\na: 1\n", &[("d0.prefix", "# c")])]
#[case::tag_directive_line("%TAG ! tag:x: # c\n---\na: 1\n", &[("d0.prefix", "# c")])]
#[case::directives_and_prefix_in_source_order(
    "# a\n%YAML 1.2 # b\n%TAG ! tag:x: # c\n---\na: 1\n",
    &[("d0.prefix", "# a"), ("d0.prefix", "# b"), ("d0.prefix", "# c")]
)]
#[case::own_line_after_marker("---\n# c\na: 1\n", &[("d0.comments", "# c")])]
#[case::marker_line_before_mapping("--- # c\na: 1\n", &[("d0.comments", "# c")])]
#[case::marker_line_of_empty_document("--- # c\n", &[("d0.comments", "# c")])]
#[case::own_line_in_empty_document("---\n# c\n", &[("d0.comments", "# c")])]
#[case::marker_line_before_scalar("--- v # c\n", &[("d0.trail:v", "# c")])]
#[case::own_line_between_marker_comments_in_order(
    "--- # a\n# b\na: 1\n",
    &[("d0.comments", "# a"), ("d0.comments", "# b")]
)]
#[case::end_before_marker("a: v\n# d\n---\nb: 1\n", &[("d0.end", "# d")])]
#[case::end_before_document_end("a: 1\n# d\n...\n", &[("d0.end", "# d")])]
#[case::end_after_blank_line_keeps_both(
    "a: 1\n# x\n\n# y\n...\n",
    &[("d0.end", "# x"), ("d0.end", "# y")]
)]
#[case::end_after_root_scalar("v\n# c\n", &[("d0.end", "# c")])]
#[case::root_scalar_before_marker("v # c\n---\nw\n", &[("d0.trail:v", "# c")])]
#[case::end_and_next_prefix(
    "a: 1\n# d\n...\n# e\n---\nb: 2\n",
    &[("d0.end", "# d"), ("d1.prefix", "# e")]
)]
#[case::prefix_of_implicit_second_document("a: 1\n...\n# e\nb: 2\n", &[("d1.prefix", "# e")])]
#[case::document_end_line_before_next_document(
    "a: 1\n... # c\n---\nb: 2\n",
    &[("d1.prefix", "# c")]
)]
#[case::directive_of_second_document("a: 1\n...\n%YAML 1.2 # c\n---\nb: 1\n", &[("d1.prefix", "# c")])]
#[case::after_end_on_marker_line("a: 1\n... # c\n", &[("d0.after_end", "# c")])]
#[case::after_end_own_line("a: 1\n...\n# e\n", &[("d0.after_end", "# e")])]
#[case::after_end_several(
    "a: 1\n...\n# e\n# f\n",
    &[("d0.after_end", "# e"), ("d0.after_end", "# f")]
)]
#[case::after_end_of_empty_document("---\n...\n# e\n", &[("d0.after_end", "# e")])]
#[case::marker_comment_of_second_document("---\n# c\n---\nb\n", &[("d0.comments", "# c")])]
// Comments that no node inherits end up in `end_comments`.
#[case::last_entry_scalar_value("a:\n  # c\n  v\n", &[("d0.end", "# c")])]
#[case::last_entry_empty_value("a:\n  # c\n", &[("d0.end", "# c")])]
#[case::nested_mapping_tail("a:\n  b: 1\n  # tail\n", &[("d0.end", "# tail")])]
#[case::sequence_tail("- a\n- b\n# tail\n", &[("d0.end", "# tail")])]
#[case::two_levels_tail_in_order(
    "a:\n  - x\n  # t1\n# t2\n",
    &[("d0.end", "# t1"), ("d0.end", "# t2")]
)]
#[case::leftover_before_comments_after_root(
    "a:\n  b: 1\n  # t1\n\n# t2\n...\n",
    &[("d0.end", "# t1"), ("d0.end", "# t2")]
)]
#[case::leftover_does_not_reach_next_document(
    "a:\n  # c\n---\nb: 1\n",
    &[("d0.end", "# c")]
)]
#[case::alias_as_last_value("x: &x 1\na:\n  # c\n  *x\n", &[("d0.end", "# c")])]
#[case::next_line_comment_is_no_trailing("key: value\n# next\n", &[("d0.end", "# next")])]
#[case::no_newline_at_end_of_input("a:\n  # c1\n  # c2", &[("d0.end", "# c1"), ("d0.end", "# c2")])]
// Comments in flow collections.
#[case::flow_mapping_first_key("{ # c\n a: 1}\n", &[("d0.lead:a", "# c")])]
// A flow collection or an alias on the `---` line is the root like any other.
#[case::marker_line_flow_sequence_then_own_line("--- [a]\n# c\n", &[("d0.end", "# c")])]
#[case::marker_line_flow_sequence_then_comment("--- [a] # c\n", &[("d0.trail:<seq>", "# c")])]
#[case::marker_line_flow_sequence_comment_inside("--- [ # c\n 1]\n", &[("d0.lead:1", "# c")])]
#[case::marker_line_flow_mapping_comment_inside("--- { # c\n a: 1}\n", &[("d0.lead:a", "# c")])]
#[case::marker_line_alias_then_comments(
    "--- *a # c\n# d\n",
    &[("d0.trail:*a", "# c"), ("d0.end", "# d")]
)]
fn every_comment_is_stored_exactly_once_at_the_pinned_place(
    #[case] input: &str,
    #[case] expected_places: &[(&str, &str)],
) {
    assert_eq!(places(input), expected(expected_places), "input: {input:?}");
}

// A comment-only stream has no document to keep its comments.
#[rstest]
#[case::empty("")]
#[case::one_comment("# c\n")]
#[case::two_comments("# c\n# d\n")]
#[case::comment_before_document_end("# c\n...\n")]
fn comment_only_stream_loads_as_zero_documents_and_loses_its_comments_by_design(
    #[case] input: &str,
) {
    assert!(load(input).unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Conservation: every comment event is stored once
// ---------------------------------------------------------------------------

fn comment_event_count(input: &str) -> usize {
    parse_events(input)
        .filter(|item| matches!(item, Ok((Event::Comment { .. }, _))))
        .count()
}

// Inputs that mix the positions: whatever the placement, no comment is lost
// or stored twice.
#[rstest]
#[case::mixed("%YAML 1.2 # a\n---  # b\nk: &x # c\n  - \"q\" # d\n  - | # e\n    t\n... # f\n")]
#[case::flow_in_block("a: [x, # c\n  y] # d\n")]
#[case::empty_key_and_value("? # a\n: # b\n")]
#[case::comments_between_documents("a: 1 # a\n# b\n...\n# c\n---\n# d\nb: 2 # e\n# f\n")]
#[case::nested_tails("a:\n  b:\n    - x # a\n    # b\n  # c\n# d\n")]
#[case::multi_line_quoted("a: \"x\n  y\" # c\nb: 1\n")]
fn comment_places_hold_every_comment_event_once(#[case] input: &str) {
    assert_eq!(places(input).len(), comment_event_count(input), "{input:?}");
}

// ---------------------------------------------------------------------------
// Multi-document streams
// ---------------------------------------------------------------------------

#[test]
fn leftover_comments_of_one_document_are_not_stored_again_in_the_next() {
    let docs = load("a:\n  # c\n---\nb:\n  # d\n").unwrap();
    assert_eq!(docs[0].end_comments, ["# c"]);
    assert_eq!(docs[1].end_comments, ["# d"]);
    assert!(docs.iter().all(|doc| doc.prefix_comments.is_empty()));
    assert!(docs.iter().all(|doc| doc.after_end_comments.is_empty()));
}

#[test]
fn only_the_last_document_has_after_end_comments() {
    let docs = load("a: 1\n...\n---\nb: 2\n...\n# e\n").unwrap();
    assert!(docs[0].after_end_comments.is_empty());
    assert_eq!(docs[1].after_end_comments, ["# e"]);
}

// ---------------------------------------------------------------------------
// Resolved mode
// ---------------------------------------------------------------------------

fn resolved_places(input: &str) -> Vec<(String, String)> {
    comment_places(&LoaderBuilder::new().resolved().build().load(input).unwrap())
}

// The document comment fields do not depend on the loader mode.
#[rstest]
#[case::prefix("# c\n---\na: 1\n")]
#[case::marker_line("--- # c\na: 1\n")]
#[case::end("a: 1\n# d\n...\n")]
#[case::next_prefix("a: 1\n# d\n...\n# e\n---\nb: 2\n")]
#[case::after_end("a: 1\n... # c\n")]
fn document_comment_fields_are_the_same_in_resolved_mode(#[case] input: &str) {
    assert_eq!(resolved_places(input), places(input), "input: {input:?}");
}

// A node produced by an alias carries none of the anchor's comments: the
// definition site keeps them, and the comments of the alias site stay.
#[rstest]
#[case::trailing_of_anchored_scalar("a: &x 1 # t\nb: *x\n", &[("d0.trail:1", "# t")])]
#[case::key_line_comment_of_anchored_mapping(
    "a: &x # c\n  b: 1\nd: *x\n",
    &[("d0.trail:a", "# c")]
)]
#[case::leading_comment_inside_anchored_mapping(
    "a: &x\n  # inner\n  b: 1\nc: *x\n",
    &[("d0.lead:b", "# inner")]
)]
#[case::comments_inside_anchored_sequence_and_alias_site_comment(
    "a: &x\n  - 1 # t\n  # l\n  - 2\nc: *x # s\n",
    &[("d0.trail:1", "# t"), ("d0.lead:2", "# l"), ("d0.trail:<seq>", "# s")]
)]
#[case::alias_site_comment_of_scalar("x: &x 1\na: *x # c\n", &[("d0.trail:1", "# c")])]
#[case::alias_site_comment_with_anchor_comment(
    "x: &x 1 # t\na: *x # c\n",
    &[("d0.trail:1", "# t"), ("d0.trail:1", "# c")]
)]
#[case::nested_alias_inside_expansion(
    "x: &x 1 # t\ny: &y\n  - *x # n\nz: *y\n",
    &[("d0.trail:1", "# t"), ("d0.trail:1", "# n")]
)]
fn resolved_expansion_carries_no_comments_of_the_anchor(
    #[case] input: &str,
    #[case] expected_places: &[(&str, &str)],
) {
    assert_eq!(resolved_places(input), expected(expected_places));
}

// Lossless mode keeps the alias node and its own comments.
#[test]
fn lossless_alias_keeps_its_own_comments() {
    assert_eq!(
        places("x: &x 1\na: *x # c\n"),
        expected(&[("d0.trail:*x", "# c")])
    );
}

// An anchor with many comments, expanded many times, stays in the AST once.
#[test]
fn resolved_expansions_do_not_multiply_the_comments_of_the_anchor() {
    const COMMENTS: usize = 1_000;
    const ALIASES: usize = 1_000;
    let mut input = String::from("base: &a\n");
    for i in 0..COMMENTS {
        writeln!(input, "  # c{i}").unwrap();
    }
    input.push_str("  - x\n");
    for i in 0..ALIASES {
        writeln!(input, "r{i}: *a").unwrap();
    }
    assert_eq!(resolved_places(&input).len(), COMMENTS);
}

// ---------------------------------------------------------------------------
// Long comment runs at every document slot, on a small stack
// ---------------------------------------------------------------------------

const RUN: usize = 100_000;

#[rstest]
#[case::prefix(
    format!("{}---\na: 1\n", comment_lines(RUN)),
    |doc: &Document<Span>| doc.prefix_comments.clone()
)]
#[case::prefix_of_implicit_document(
    format!("{}a: 1\n", comment_lines(RUN)),
    |doc: &Document<Span>| doc.prefix_comments.clone()
)]
#[case::after_marker(
    format!("---\n{}a: 1\n", comment_lines(RUN)),
    |doc: &Document<Span>| doc.comments.clone()
)]
#[case::end_after_root_scalar(
    format!("v\n{}", comment_lines(RUN)),
    |doc: &Document<Span>| doc.end_comments.clone()
)]
#[case::end_inside_block_mapping(
    format!("a: 1\n{}", comment_lines(RUN)),
    |doc: &Document<Span>| doc.end_comments.clone()
)]
#[case::end_before_document_end(
    format!("a: 1\n{}...\n", comment_lines(RUN)),
    |doc: &Document<Span>| doc.end_comments.clone()
)]
#[case::after_end(
    format!("a: 1\n...\n{}", comment_lines(RUN)),
    |doc: &Document<Span>| doc.after_end_comments.clone()
)]
fn long_comment_run_at_a_document_slot_is_stored_in_order_on_a_small_stack(
    #[case] input: String,
    #[case] slot: fn(&Document<Span>) -> Vec<String>,
) {
    let stored = on_small_stack(move || {
        let docs = load(&input).unwrap();
        assert_eq!(docs.len(), 1);
        slot(&docs[0])
    });
    assert!(
        stored == comment_texts(RUN),
        "stored {} comments",
        stored.len()
    );
}

// The leftover run bubbles up through every open collection.
#[test]
fn long_comment_run_after_deeply_nested_collections_is_stored_once_in_order() {
    const DEPTH: usize = 100;
    let mut input = String::new();
    for level in 0..DEPTH {
        writeln!(input, "{}k:", "  ".repeat(level)).unwrap();
    }
    writeln!(input, "{}x: 1", "  ".repeat(DEPTH)).unwrap();
    input.push_str(&comment_lines(RUN));
    let stored = on_small_stack(move || load(&input).unwrap().remove(0).end_comments);
    assert!(
        stored == comment_texts(RUN),
        "stored {} comments",
        stored.len()
    );
}

/// The key `a:` with `levels` mappings nested in it, each holding one key, and a
/// scalar at the bottom.
fn mappings_nested_below_a_key(levels: usize) -> String {
    let mut input = String::from("a:\n");
    for level in 1..=levels {
        writeln!(input, "{}k:", "  ".repeat(level)).unwrap();
    }
    writeln!(input, "{}v", "  ".repeat(levels + 1)).unwrap();
    input
}

// A comment below a deep chain of collections leaves every one of them and
// reaches the next key of the root, or the end of the document.
#[rstest]
#[case::block_mappings_then_key(
    format!("{}# bottom\nz: 1\n", mappings_nested_below_a_key(299)),
    vec![("d0.lead:z", "# bottom")],
)]
#[case::block_sequences_then_key(
    format!("a:\n{}v\n# bottom\nz: 1\n", "- ".repeat(299)),
    vec![("d0.lead:z", "# bottom")],
)]
#[case::flow_sequences_then_key(
    format!("a: {}v{}\n# bottom\nz: 1\n", "[".repeat(299), "]".repeat(299)),
    vec![("d0.lead:z", "# bottom")],
)]
#[case::block_mappings_then_end_of_document(
    format!("{}# bottom\n", mappings_nested_below_a_key(299)),
    vec![("d0.end", "# bottom")],
)]
#[case::flow_sequences_then_end_of_document(
    format!("a: {}v{}\n# bottom\n", "[".repeat(299), "]".repeat(299)),
    vec![("d0.end", "# bottom")],
)]
fn a_comment_below_a_deep_chain_of_collections_reaches_what_follows(
    #[case] input: String,
    #[case] expected_places: Vec<(&'static str, &'static str)>,
) {
    let found = on_small_stack(move || {
        comment_places(
            &LoaderBuilder::new()
                .max_nesting_depth(512)
                .build()
                .load(&input)
                .unwrap(),
        )
    });
    assert_eq!(found, expected(&expected_places));
}

// ---------------------------------------------------------------------------
// Errors after a comment run
// ---------------------------------------------------------------------------

#[rstest]
#[case::before_marker(format!("# c\n#{}\n---\na: 1\n", "x".repeat(4097)))]
#[case::after_marker(format!("---\n#{}\na: 1\n", "x".repeat(4097)))]
#[case::after_root(format!("a: 1\n#{}\n", "x".repeat(4097)))]
#[case::after_document_end(format!("a: 1\n...\n#{}\n", "x".repeat(4097)))]
#[case::block_header(format!("a: | #{}\n  t\n", "x".repeat(4097)))]
#[case::key_line(format!("a: #{}\n  b: 1\n", "x".repeat(4097)))]
#[case::control_character_after_marker("---\n# c\x01\na: 1\n".to_owned())]
fn comment_error_is_a_parse_error_not_a_partial_ast(#[case] input: String) {
    assert!(
        matches!(load(&input), Err(LoadError::Parse { .. })),
        "input of {} bytes",
        input.len()
    );
}

#[test]
fn comment_of_the_maximum_length_is_stored() {
    let comment = format!("#{}", "x".repeat(4096));
    let docs = load(&format!("a: 1 {comment}\n")).unwrap();
    assert_eq!(docs[0].root.trailing_comment(), None);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected a mapping");
    };
    assert_eq!(entries[0].1.trailing_comment(), Some(comment.as_str()));
}
