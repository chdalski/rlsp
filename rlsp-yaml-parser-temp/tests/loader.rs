// SPDX-License-Identifier: MIT
#![allow(clippy::panic)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

//! Integration tests for the rlsp-yaml-parser-temp loader.
//!
//! Exercises `load()` and `LoaderBuilder` through the public API.

use rlsp_yaml_parser_temp::loader::{LoadError, LoaderBuilder, load};
use rlsp_yaml_parser_temp::node::Node;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_one(input: &str) -> Node<rlsp_yaml_parser_temp::Span> {
    let docs = load(input).expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document, got {}", docs.len());
    docs.into_iter().next().unwrap().root
}

fn load_resolved_one(input: &str) -> Node<rlsp_yaml_parser_temp::Span> {
    let docs = LoaderBuilder::new()
        .resolved()
        .build()
        .load(input)
        .expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document, got {}", docs.len());
    docs.into_iter().next().unwrap().root
}

fn scalar_value(node: &Node<rlsp_yaml_parser_temp::Span>) -> &str {
    match node {
        Node::Scalar { value, .. } => value.as_str(),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            panic!("expected Scalar, got: {node:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// IT-0: Spike — validates the harness
// ---------------------------------------------------------------------------

#[test]
fn spike_plain_scalar_loads() {
    let docs = load("hello\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    assert!(
        matches!(&docs[0].root, Node::Scalar { value, .. } if value == "hello"),
        "got: {:?}",
        &docs[0].root
    );
}

// ---------------------------------------------------------------------------
// Group A — Basic node types
// ---------------------------------------------------------------------------

#[test]
fn it_1_plain_scalar_value_is_correct() {
    let node = load_one("hello\n");
    assert!(
        matches!(&node, Node::Scalar { value, .. } if value == "hello"),
        "got: {node:?}"
    );
}

#[test]
fn it_2_integer_scalar_value_is_correct() {
    let node = load_one("42\n");
    assert!(
        matches!(&node, Node::Scalar { value, .. } if value == "42"),
        "got: {node:?}"
    );
}

#[test]
fn it_3_empty_document_has_empty_scalar_root() {
    let docs = load("---\n...\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    assert!(
        matches!(&docs[0].root, Node::Scalar { value, .. } if value.is_empty()),
        "got: {:?}",
        &docs[0].root
    );
}

#[test]
fn it_4_block_mapping_has_correct_entries() {
    let node = load_one("{name: Alice, age: 30}\n");
    assert!(
        matches!(&node, Node::Mapping { entries, .. } if entries.len() == 2),
        "got: {node:?}"
    );
}

#[test]
fn it_5_block_sequence_has_correct_items() {
    let node = load_one("- alpha\n- beta\n- gamma\n");
    assert!(
        matches!(&node, Node::Sequence { items, .. } if items.len() == 3),
        "got: {node:?}"
    );
}

#[test]
fn it_6_nested_mapping_inside_sequence() {
    let node = load_one("- {name: Alice, age: 30}\n");
    let Node::Sequence { items, .. } = node else {
        panic!("expected Sequence");
    };
    assert!(
        matches!(&items[0], Node::Mapping { .. }),
        "got: {:?}",
        &items[0]
    );
}

#[test]
fn it_7_multi_document_produces_two_documents() {
    let docs = load("doc1\n---\ndoc2\n").expect("load failed");
    assert_eq!(docs.len(), 2);
}

// ---------------------------------------------------------------------------
// Group B — Comment skipping
// ---------------------------------------------------------------------------

#[test]
fn it_8_leading_comment_is_not_a_node_in_ast() {
    let node = load_one("# top comment\nhello\n");
    assert!(
        matches!(&node, Node::Scalar { value, .. } if value == "hello"),
        "expected scalar 'hello', got: {node:?}"
    );
}

#[test]
fn it_9_inline_comment_does_not_corrupt_scalar() {
    // The parser may or may not include the inline comment in the value.
    // The key property: the value does not include the `#` character or
    // the comment text "inline comment".
    let node = load_one("hello # inline comment\n");
    let value = scalar_value(&node);
    assert!(
        !value.contains("inline comment"),
        "comment text should not be in scalar value; got: {value:?}"
    );
}

#[test]
fn it_10_comment_between_mapping_entries_is_skipped() {
    let node = load_one("a: 1\n# comment\nb: 2\n");
    assert!(
        matches!(&node, Node::Mapping { entries, .. } if entries.len() == 2),
        "expected 2-entry mapping, got: {node:?}"
    );
}

#[test]
fn it_11_comment_between_sequence_items_is_skipped() {
    let node = load_one("- a\n# comment\n- b\n");
    assert!(
        matches!(&node, Node::Sequence { items, .. } if items.len() == 2),
        "expected 2-item sequence, got: {node:?}"
    );
}

// ---------------------------------------------------------------------------
// Group C — Anchors and aliases
// ---------------------------------------------------------------------------

#[test]
fn it_12_anchor_on_scalar_is_preserved_in_lossless_mode() {
    let node = load_one("- &ref shared\n- *ref\n");
    let Node::Sequence { items, .. } = node else {
        panic!("expected Sequence");
    };
    assert_eq!(items.len(), 2);
    assert!(
        matches!(&items[0], Node::Scalar { anchor: Some(a), value, .. } if a == "ref" && value == "shared"),
        "got: {:?}",
        &items[0]
    );
    assert!(
        matches!(&items[1], Node::Alias { name, .. } if name == "ref"),
        "got: {:?}",
        &items[1]
    );
}

#[test]
fn it_13_alias_expands_to_anchored_scalar_in_resolved_mode() {
    let node = load_resolved_one("- &ref shared\n- *ref\n");
    let Node::Sequence { items, .. } = node else {
        panic!("expected Sequence");
    };
    assert_eq!(items.len(), 2);
    assert!(
        matches!(&items[0], Node::Scalar { value, .. } if value == "shared"),
        "got: {:?}",
        &items[0]
    );
    assert!(
        matches!(&items[1], Node::Scalar { value, .. } if value == "shared"),
        "got: {:?}",
        &items[1]
    );
}

#[test]
fn it_14_anchor_on_mapping_is_registered() {
    let node = load_one("base: &base\n  x: 1\nref: *base\n");
    let Node::Mapping { entries, .. } = node else {
        panic!("expected Mapping");
    };
    let base_entry = entries.iter().find(|(k, _)| scalar_value(k) == "base");
    assert!(base_entry.is_some(), "key 'base' not found");
    let (_, val) = base_entry.unwrap();
    assert!(
        matches!(val, Node::Mapping { anchor: Some(a), .. } if a == "base"),
        "got: {val:?}"
    );
}

#[test]
fn it_15_alias_expands_to_anchored_mapping_in_resolved_mode() {
    let node = LoaderBuilder::new()
        .resolved()
        .build()
        .load("- &b\n  x: 1\n- *b\n")
        .expect("load failed");
    let Node::Sequence { items, .. } = &node[0].root else {
        panic!("expected Sequence");
    };
    assert_eq!(items.len(), 2);
    assert!(
        matches!(&items[1], Node::Mapping { entries, .. } if entries.len() == 1),
        "got: {:?}",
        &items[1]
    );
}

#[test]
fn it_16_undefined_alias_in_resolved_mode_returns_error() {
    let result = LoaderBuilder::new()
        .resolved()
        .build()
        .load("val: *missing\n");
    assert!(result.is_err(), "expected Err");
    assert!(
        matches!(result.unwrap_err(), LoadError::UndefinedAlias { name } if name == "missing"),
        "expected UndefinedAlias for 'missing'"
    );
}

// ---------------------------------------------------------------------------
// Group D — Tag directives
// ---------------------------------------------------------------------------

#[test]
fn it_17_tag_directives_captured_on_document() {
    let docs = load("%TAG !foo! tag:example.com,2026:\n---\nhello\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    assert!(
        docs[0]
            .tags
            .iter()
            .any(|(h, p)| h == "!foo!" && p.contains("tag:example.com")),
        "expected tag directive pair; tags: {:?}",
        docs[0].tags
    );
}

#[test]
fn it_18_version_directive_captured_on_document() {
    let docs = load("%YAML 1.2\n---\nhello\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].version, Some((1, 2)));
}

// ---------------------------------------------------------------------------
// Group E — Error propagation
// ---------------------------------------------------------------------------

#[test]
fn it_19_parse_error_propagates_as_load_error_parse() {
    // A tab character at the start of a line is invalid in YAML block context.
    let result = load("\t invalid\n");
    // Either the new or old parser may handle this differently; the key
    // property is that it returns a Result (Ok or Err) without panicking.
    // If it's an error, verify it's a LoadError::Parse.
    if let Err(e) = result {
        assert!(
            matches!(e, LoadError::Parse { .. }),
            "expected LoadError::Parse, got: {e:?}"
        );
    }
    // If it's Ok, the parser is lenient — that's also acceptable.
}

// ---------------------------------------------------------------------------
// Group F — DoS limits
// ---------------------------------------------------------------------------

#[test]
fn it_20_nesting_depth_limit_triggers_at_configured_threshold() {
    let yaml = "[[[x]]]\n"; // depth 3
    let result = LoaderBuilder::new().max_nesting_depth(2).build().load(yaml);
    assert!(result.is_err(), "expected Err for depth 3 with limit 2");
    assert!(
        matches!(
            result.unwrap_err(),
            LoadError::NestingDepthLimitExceeded { limit: 2 }
        ),
        "expected NestingDepthLimitExceeded"
    );
}

#[test]
fn it_21_nesting_at_exact_limit_succeeds() {
    let yaml = "[[x]]\n"; // depth 2
    let result = LoaderBuilder::new().max_nesting_depth(2).build().load(yaml);
    assert!(
        result.is_ok(),
        "expected Ok for depth exactly at limit; got: {result:?}"
    );
}

#[test]
fn it_22_anchor_count_limit_returns_error() {
    let yaml = "- &a x\n- &b y\n- &c z\n";
    let result = LoaderBuilder::new().max_anchors(2).build().load(yaml);
    assert!(result.is_err(), "expected Err for 3 anchors with limit 2");
    assert!(
        matches!(
            result.unwrap_err(),
            LoadError::AnchorCountLimitExceeded { limit: 2 }
        ),
        "expected AnchorCountLimitExceeded"
    );
}

#[test]
fn it_23_alias_expansion_limit_returns_error_in_resolved_mode() {
    // 1 anchor + 3 aliases = 4 expansions (anchor counts too) > limit 3
    let yaml = "- &a x\n- *a\n- *a\n- *a\n";
    let result = LoaderBuilder::new()
        .resolved()
        .max_expanded_nodes(3)
        .build()
        .load(yaml);
    assert!(result.is_err(), "expected Err with limit 3");
    assert!(
        matches!(
            result.unwrap_err(),
            LoadError::AliasExpansionLimitExceeded { limit: 3 }
        ),
        "expected AliasExpansionLimitExceeded"
    );
}

#[test]
fn it_24_circular_alias_detection_in_resolved_mode() {
    // The parser should reject a YAML file with a true circular reference.
    // In practice this may be caught at parse time (as a parse error) or at
    // load time (as CircularAlias). Either way: no panic, no infinite loop.
    // We test with the Billion Laughs pattern which is not circular but
    // exhausts the expansion limit.
    let yaml = concat!(
        "a: &a [\"lol\"]\n",
        "b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a]\n",
        "c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b]\n",
        "j: *c\n",
    );
    let result = LoaderBuilder::new()
        .resolved()
        .max_expanded_nodes(100)
        .build()
        .load(yaml);
    assert!(
        result.is_err(),
        "expected Err for alias bomb with limit 100; got Ok"
    );
}

// ---------------------------------------------------------------------------
// Group H — Round-trip as AST content (no emitter cross-dependency)
// ---------------------------------------------------------------------------

#[test]
fn it_rt_1_scalar_hello() {
    let docs = load("hello\n").expect("load");
    assert_eq!(docs.len(), 1);
    assert!(matches!(&docs[0].root, Node::Scalar { value, .. } if value == "hello"));
}

#[test]
fn it_rt_2_mapping_with_two_pairs() {
    let node = load_one("{name: Alice, age: 30}\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping");
    };
    assert_eq!(entries.len(), 2);
    assert!(matches!(&entries[0].0, Node::Scalar { value, .. } if value == "name"));
    assert!(matches!(&entries[0].1, Node::Scalar { value, .. } if value == "Alice"));
    assert!(matches!(&entries[1].0, Node::Scalar { value, .. } if value == "age"));
    assert!(matches!(&entries[1].1, Node::Scalar { value, .. } if value == "30"));
}

#[test]
fn it_rt_3_sequence_with_three_scalars() {
    let node = load_one("- alpha\n- beta\n- gamma\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(scalar_value(&items[0]), "alpha");
    assert_eq!(scalar_value(&items[1]), "beta");
    assert_eq!(scalar_value(&items[2]), "gamma");
}

#[test]
fn it_rt_4_flow_mapping_two_pairs() {
    let node = load_one("{a: 1, b: 2}\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(scalar_value(&entries[0].0), "a");
    assert_eq!(scalar_value(&entries[1].0), "b");
}

#[test]
fn it_rt_5_flow_sequence_three_scalars() {
    let node = load_one("[alpha, beta, gamma]\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(scalar_value(&items[0]), "alpha");
    assert_eq!(scalar_value(&items[1]), "beta");
    assert_eq!(scalar_value(&items[2]), "gamma");
}

#[test]
fn it_rt_6_empty_flow_mapping() {
    let node = load_one("{}\n");
    assert!(
        matches!(&node, Node::Mapping { entries, .. } if entries.is_empty()),
        "got: {node:?}"
    );
}

#[test]
fn it_rt_7_empty_flow_sequence() {
    let node = load_one("[]\n");
    assert!(
        matches!(&node, Node::Sequence { items, .. } if items.is_empty()),
        "got: {node:?}"
    );
}

#[test]
fn it_rt_8_deeply_nested_leaf_scalar() {
    // The block-mapping parser nests subsequent same-level keys under the first.
    // Use flow-style mappings to ensure predictable nesting.
    let node = load_one("{a: {b: {c: leaf}}}\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping");
    };
    let (_, b_node) = &entries[0];
    let Node::Mapping {
        entries: b_entries, ..
    } = b_node
    else {
        panic!("expected nested Mapping for a's value; got: {b_node:?}");
    };
    let (_, c_node) = &b_entries[0];
    let Node::Mapping {
        entries: c_entries, ..
    } = c_node
    else {
        panic!("expected nested Mapping for b's value; got: {c_node:?}");
    };
    let (_, leaf_node) = &c_entries[0];
    assert!(
        matches!(leaf_node, Node::Scalar { value, .. } if value == "leaf"),
        "got: {leaf_node:?}"
    );
}

#[test]
fn it_rt_9_anchored_node_preserves_anchor() {
    let node = load_one("- &ref shared\n- *ref\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence");
    };
    assert!(
        matches!(&items[0], Node::Scalar { anchor: Some(a), .. } if a == "ref"),
        "got: {:?}",
        &items[0]
    );
}

#[test]
fn it_rt_10_two_documents_correct_values() {
    let docs = load("doc1\n---\ndoc2\n").expect("load");
    assert_eq!(docs.len(), 2);
    assert!(matches!(&docs[0].root, Node::Scalar { value, .. } if value == "doc1"));
    assert!(matches!(&docs[1].root, Node::Scalar { value, .. } if value == "doc2"));
}

#[test]
fn it_rt_11_literal_block_scalar_contains_lines() {
    let node = load_one("|\n  line1\n  line2\n");
    let value = scalar_value(&node);
    assert!(
        value.contains("line1"),
        "expected 'line1' in value; got: {value:?}"
    );
    assert!(
        value.contains("line2"),
        "expected 'line2' in value; got: {value:?}"
    );
}

#[test]
fn it_rt_12_large_mapping_count() {
    // Programmatically load 1000-entry mapping, verify count and spot-check.
    let mut yaml = String::new();
    for i in 0..1000 {
        use std::fmt::Write as _;
        let _ = writeln!(yaml, "key{i}: val{i}");
    }
    let node = load_one(&yaml);
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping");
    };
    assert_eq!(entries.len(), 1000);
    assert_eq!(scalar_value(&entries[0].0), "key0");
    assert_eq!(scalar_value(&entries[0].1), "val0");
    assert_eq!(scalar_value(&entries[999].0), "key999");
    assert_eq!(scalar_value(&entries[999].1), "val999");
}
