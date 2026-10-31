// SPDX-License-Identifier: MIT
//
// Every `Comment` event of a stream that loads is stored in the AST, once:
// the comment count of the event stream equals the number of comments in the
// `Document` fields and on the nodes of the lossless AST.  Checked on every
// yaml-test-suite input that loads and has a document; a stream that holds
// only comments has no document to keep them.

use std::path::PathBuf;
use std::time::Duration;

use rlsp_yaml_parser::{Event, Node, Span, load, parse_events};
use rstest::rstest;

use super::load_cases_from_file;

fn comments_on_nodes(node: &Node<Span>) -> usize {
    let own = node.leading_comments().len() + usize::from(node.trailing_comment().is_some());
    let nested = match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .map(|(key, value)| comments_on_nodes(key) + comments_on_nodes(value))
            .sum(),
        Node::Sequence { items, .. } => items.iter().map(comments_on_nodes).sum(),
        Node::Scalar { .. } | Node::Alias { .. } => 0,
    };
    own + nested
}

/// `None` when `yaml` does not load or has no document; otherwise the number
/// of comment events and the number of stored comments.
fn event_and_stored_comments(yaml: &str) -> Option<(usize, usize)> {
    let docs = load(yaml).ok().filter(|docs| !docs.is_empty())?;
    let events = parse_events(yaml)
        .filter(|item| matches!(item, Ok((Event::Comment { .. }, _))))
        .count();
    let stored = docs
        .iter()
        .map(|doc| {
            doc.prefix_comments.len()
                + doc.comments.len()
                + doc.end_comments.len()
                + doc.after_end_comments.len()
                + comments_on_nodes(&doc.root)
        })
        .sum();
    Some((events, stored))
}

/// Returns the number of comments compared.
fn assert_every_comment_is_stored(yaml: &str, tag: &str) -> usize {
    let Some((events, stored)) = event_and_stored_comments(yaml) else {
        return 0;
    };
    assert_eq!(stored, events, "comments stored vs comment events: {tag}");
    events
}

#[rstest]
#[timeout(Duration::from_secs(5))]
pub fn every_comment_in_suite_is_stored_in_the_ast(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
) {
    for case in &load_cases_from_file(&path) {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);
        assert_every_comment_is_stored(&case.yaml, &tag);
    }
}

// The sweep must not pass by comparing nothing.
#[test]
fn comment_storage_sweep_compares_many_comments_in_the_suite() {
    let dir = std::path::Path::new("../tests/yaml-test-suite/src");
    let mut compared = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        for case in &load_cases_from_file(&entry.unwrap().path()) {
            compared += assert_every_comment_is_stored(&case.yaml, &case.name);
        }
    }
    assert!(compared > 50, "only {compared} comments were compared");
}

// Inputs for the positions the suite does not cover.
#[rstest]
#[case::double_quoted_value("a: \"q\" # c\n")]
#[case::key_without_value("k: # c\n")]
#[case::block_header("a: | # c\n  t\n")]
#[case::directives("%YAML 1.2 # c\n---\na: 1\n")]
#[case::document_end("a: 1\n... # c\n")]
#[case::between_documents("a: 1\n# d\n...\n# e\n---\nb: 2\n")]
#[case::mixed("%YAML 1.2 # a\n---  # b\nk: &x # c\n  - \"q\" # d\n  - | # e\n    t\n... # f\n")]
fn every_comment_in_explicit_inputs_is_stored(#[case] yaml: &str) {
    assert!(assert_every_comment_is_stored(yaml, yaml) > 0);
}
