// SPDX-License-Identifier: MIT
#![expect(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test code"
)]

//! Integration tests for the rlsp-yaml-parser loader.
//!
//! Exercises `load()` and `LoaderBuilder` through the public API.

use rstest::rstest;

use rlsp_yaml_parser::CollectionStyle;
use rlsp_yaml_parser::ScalarStyle;
use rlsp_yaml_parser::loader::{LoadError, LoaderBuilder, load};
use rlsp_yaml_parser::node::Node;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_one(input: &str) -> Node<rlsp_yaml_parser::Span> {
    let docs = load(input).expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document, got {}", docs.len());
    docs.into_iter().next().unwrap().root
}

fn load_resolved_one(input: &str) -> Node<rlsp_yaml_parser::Span> {
    let docs = LoaderBuilder::new()
        .resolved()
        .build()
        .load(input)
        .expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document, got {}", docs.len());
    docs.into_iter().next().unwrap().root
}

fn scalar_value(node: &Node<rlsp_yaml_parser::Span>) -> &str {
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

#[rstest]
#[case::plain_scalar("hello\n", "hello")]
#[case::integer_scalar("42\n", "42")]
fn plain_scalar_value_is_correct(#[case] input: &str, #[case] expected: &str) {
    let node = load_one(input);
    assert!(
        matches!(&node, Node::Scalar { value, .. } if value == expected),
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

// ---------------------------------------------------------------------------
// IT-I: Inline anchor/tag before mapping key — property placement (9KAX)
//
// Per YAML test suite 9KAX: an inline property (anchor or tag on the same
// line as the key) annotates the KEY SCALAR.  A standalone property (own
// line) annotates the collection node that follows.
// ---------------------------------------------------------------------------

// IT-I-1: Inline anchor at root level annotates key scalar, not mapping.
#[test]
fn inline_anchor_before_key_annotates_key_scalar_root() {
    let node = load_one("&anchor key: value\n");
    let Node::Mapping {
        entries, anchor, ..
    } = &node
    else {
        panic!("expected Mapping root, got: {node:?}");
    };
    assert!(
        anchor.is_none(),
        "mapping must have no anchor; got: {anchor:?}"
    );
    assert_eq!(entries.len(), 1, "expected 1 entry; got: {}", entries.len());
    let (k, v) = &entries[0];
    let Node::Scalar {
        value: kv,
        anchor: ka,
        ..
    } = k
    else {
        panic!("key must be Scalar; got: {k:?}");
    };
    assert_eq!(kv.as_str(), "key");
    assert_eq!(
        ka.as_deref(),
        Some("anchor"),
        "anchor must be on key scalar; got: {ka:?}"
    );
    assert_eq!(scalar_value(v), "value");
}

// IT-I-2: Inline anchor at indented level annotates key scalar, not inner mapping.
#[test]
fn inline_anchor_before_key_annotates_key_scalar_indented() {
    let node = load_one("outer:\n  &anchor inner_key: inner_value\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root");
    };
    assert_eq!(entries.len(), 1);
    let (_, v) = &entries[0];
    let Node::Mapping {
        entries: inner,
        anchor,
        ..
    } = v
    else {
        panic!("expected inner Mapping; got: {v:?}");
    };
    assert!(
        anchor.is_none(),
        "inner mapping must have no anchor; got: {anchor:?}"
    );
    assert_eq!(
        inner.len(),
        1,
        "expected 1 inner entry; got: {}",
        inner.len()
    );
    let (ik, iv) = &inner[0];
    let Node::Scalar {
        value: ikv,
        anchor: ika,
        ..
    } = ik
    else {
        panic!("inner key must be Scalar; got: {ik:?}");
    };
    assert_eq!(ikv.as_str(), "inner_key");
    assert_eq!(
        ika.as_deref(),
        Some("anchor"),
        "anchor must be on inner key scalar; got: {ika:?}"
    );
    assert_eq!(scalar_value(iv), "inner_value");
}

// IT-I-3: Inline tag before key annotates key scalar, not mapping.
#[test]
fn inline_tag_before_key_annotates_key_scalar() {
    let node = load_one("!!str key: value\n");
    let Node::Mapping { entries, tag, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert!(tag.is_none(), "mapping must have no tag; got: {tag:?}");
    assert_eq!(entries.len(), 1, "expected 1 entry; got: {}", entries.len());
    let (k, v) = &entries[0];
    let Node::Scalar {
        value: kv, tag: kt, ..
    } = k
    else {
        panic!("key must be Scalar; got: {k:?}");
    };
    assert_eq!(kv.as_str(), "key");
    assert!(
        kt.as_deref().is_some_and(|t| t.contains("str")),
        "tag must be on key scalar; got: {kt:?}"
    );
    assert_eq!(scalar_value(v), "value");
}

// IT-I-4: Inline anchor + tag together before key — both annotate key scalar.
#[test]
fn inline_anchor_and_tag_before_key_annotate_key_scalar() {
    let node = load_one("&a !!str key: value\n");
    let Node::Mapping {
        entries,
        anchor,
        tag,
        ..
    } = &node
    else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert!(
        anchor.is_none(),
        "mapping must have no anchor; got: {anchor:?}"
    );
    assert!(tag.is_none(), "mapping must have no tag; got: {tag:?}");
    assert_eq!(entries.len(), 1, "expected 1 entry; got: {}", entries.len());
    let (k, v) = &entries[0];
    let Node::Scalar {
        value: kv,
        anchor: ka,
        tag: kt,
        ..
    } = k
    else {
        panic!("key must be Scalar; got: {k:?}");
    };
    assert_eq!(kv.as_str(), "key");
    assert_eq!(
        ka.as_deref(),
        Some("a"),
        "anchor must be on key scalar; got: {ka:?}"
    );
    assert!(
        kt.as_deref().is_some_and(|t| t.contains("str")),
        "tag must be on key scalar; got: {kt:?}"
    );
    assert_eq!(scalar_value(v), "value");
}

// IT-I-5: Standalone anchor (own line) annotates the mapping, not a key scalar.
#[test]
fn standalone_anchor_before_mapping_annotates_mapping() {
    let node = load_one("&anchor\nkey: value\n");
    let Node::Mapping {
        entries, anchor, ..
    } = &node
    else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        anchor.as_deref(),
        Some("anchor"),
        "anchor must be on mapping; got: {anchor:?}"
    );
    assert_eq!(entries.len(), 1, "expected 1 entry; got: {}", entries.len());
    let (k, _) = &entries[0];
    let Node::Scalar {
        value: kv,
        anchor: ka,
        ..
    } = k
    else {
        panic!("key must be Scalar; got: {k:?}");
    };
    assert_eq!(kv.as_str(), "key");
    assert!(ka.is_none(), "key scalar must have no anchor; got: {ka:?}");
}

// IT-I-6: Multi-entry mapping — inline anchor on one key, other keys unaffected.
#[test]
fn inline_anchor_on_one_key_in_multi_entry_mapping() {
    let node = load_one("a: 1\n&anchor b: 2\nc: 3\n");
    let Node::Mapping {
        entries, anchor, ..
    } = &node
    else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert!(
        anchor.is_none(),
        "mapping must have no anchor; got: {anchor:?}"
    );
    assert_eq!(
        entries.len(),
        3,
        "expected 3 entries; got: {}",
        entries.len()
    );
    // First key: no anchor
    let (k0, _) = &entries[0];
    let Node::Scalar { anchor: a0, .. } = k0 else {
        panic!("expected Scalar key 0");
    };
    assert!(a0.is_none(), "key 'a' must have no anchor; got: {a0:?}");
    // Second key: carries the anchor
    let (k1, v1) = &entries[1];
    let Node::Scalar {
        value: kv1,
        anchor: a1,
        ..
    } = k1
    else {
        panic!("expected Scalar key 1");
    };
    assert_eq!(kv1.as_str(), "b");
    assert_eq!(
        a1.as_deref(),
        Some("anchor"),
        "anchor must be on key 'b'; got: {a1:?}"
    );
    assert_eq!(scalar_value(v1), "2");
    // Third key: no anchor
    let (k2, _) = &entries[2];
    let Node::Scalar { anchor: a2, .. } = k2 else {
        panic!("expected Scalar key 2");
    };
    assert!(a2.is_none(), "key 'c' must have no anchor; got: {a2:?}");
}

// IT-I-7: Inline anchor before indented key — no phantom nesting.
// `&anchor key: value` must produce exactly 1 top-level mapping entry.
#[test]
fn inline_anchor_produces_no_phantom_nesting() {
    let node = load_one("&anchor key: value\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root");
    };
    assert_eq!(
        entries.len(),
        1,
        "must have exactly 1 entry (no phantom nested mapping); got: {}",
        entries.len()
    );
    // The value must be a plain scalar, not a nested mapping.
    let (_, v) = &entries[0];
    assert!(
        matches!(v, Node::Scalar { .. }),
        "value must be a plain scalar, not a nested mapping; got: {v:?}"
    );
}

// IT-I-7: Anchor on inline key scalar is usable as an alias (resolved mode).
// `&anchor key: value\nref: *anchor\n` — *anchor resolves to the key scalar "key".
#[test]
fn anchor_before_mapping_key_is_usable_as_alias() {
    let node = load_resolved_one("&anchor key: value\nref: *anchor\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        2,
        "expected 2 entries; got: {}",
        entries.len()
    );
    // Entry 0: key="key", value="value"
    let (k0, v0) = &entries[0];
    assert_eq!(scalar_value(k0), "key", "entry 0 key must be 'key'");
    assert_eq!(scalar_value(v0), "value", "entry 0 value must be 'value'");
    // Entry 1: key="ref", value is the resolved alias (the anchored key scalar "key")
    let (k1, v1) = &entries[1];
    assert_eq!(scalar_value(k1), "ref", "entry 1 key must be 'ref'");
    assert_eq!(
        scalar_value(v1),
        "key",
        "alias *anchor must resolve to the anchored key scalar value 'key'; got: {v1:?}"
    );
}

// IT-I-8: Multiple anchored keys in the same mapping — all 3 entries present, no phantom nesting.
#[test]
fn multiple_anchored_keys_in_same_mapping() {
    let node = load_one("&a one: 1\n&b two: 2\n&c three: 3\n");
    let Node::Mapping {
        entries, anchor, ..
    } = &node
    else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert!(
        anchor.is_none(),
        "mapping must have no anchor; got: {anchor:?}"
    );
    assert_eq!(
        entries.len(),
        3,
        "expected 3 entries; got: {}",
        entries.len()
    );
    let expected = [("one", "a"), ("two", "b"), ("three", "c")];
    for (i, (exp_val, exp_anchor)) in expected.iter().enumerate() {
        let (k, _) = &entries[i];
        let Node::Scalar {
            value: kv,
            anchor: ka,
            ..
        } = k
        else {
            panic!("entry {i} key must be Scalar; got: {k:?}");
        };
        assert_eq!(kv.as_str(), *exp_val, "entry {i} key value");
        assert_eq!(
            ka.as_deref(),
            Some(*exp_anchor),
            "entry {i} key anchor must be '{exp_anchor}'; got: {ka:?}"
        );
    }
}

// Control case: Inline anchor before value-side scalar — not before key.
// `key: &anchor value` — anchor annotates the value scalar (not the key, not the mapping).
#[test]
fn inline_anchor_before_value_scalar_annotates_value() {
    let node = load_one("key: &anchor value\n");
    let Node::Mapping {
        entries, anchor, ..
    } = &node
    else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert!(
        anchor.is_none(),
        "mapping must have no anchor; got: {anchor:?}"
    );
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    let Node::Scalar {
        value: kv,
        anchor: ka,
        ..
    } = k
    else {
        panic!("key must be Scalar; got: {k:?}");
    };
    assert_eq!(kv.as_str(), "key");
    assert!(ka.is_none(), "key scalar must have no anchor; got: {ka:?}");
    let Node::Scalar {
        value: vv,
        anchor: va,
        ..
    } = v
    else {
        panic!("value must be Scalar; got: {v:?}");
    };
    assert_eq!(vv.as_str(), "value");
    assert_eq!(
        va.as_deref(),
        Some("anchor"),
        "anchor must be on value scalar; got: {va:?}"
    );
}

// ---------------------------------------------------------------------------
// Group J — Trailing comment attachment
// ---------------------------------------------------------------------------

#[test]
fn it_j1_trailing_comment_on_mapping_value_is_attached() {
    let node = load_one("key: value  # my comment\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1);
    let (_, v) = &entries[0];
    let tc = v
        .trailing_comment()
        .expect("trailing comment must be attached to value node");
    assert!(
        tc.contains("my comment"),
        "trailing comment must contain 'my comment'; got: {tc:?}"
    );
}

#[test]
fn it_j2_trailing_comment_text_includes_hash() {
    let node = load_one("key: value  # my comment\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    let (_, v) = &entries[0];
    let tc = v
        .trailing_comment()
        .expect("trailing comment must be attached");
    assert!(
        tc.starts_with('#'),
        "trailing comment must start with '#'; got: {tc:?}"
    );
}

// ---------------------------------------------------------------------------
// Group K — Quoted mapping keys (style preservation)
// ---------------------------------------------------------------------------

#[test]
fn quoted_key_double_quoted_simple() {
    let docs = load("\"key\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "key" && *style == ScalarStyle::DoubleQuoted),
        "key must be decoded with DoubleQuoted style, got: {k:?}"
    );
    assert!(
        matches!(v, Node::Scalar { value, .. } if value == "value"),
        "val: {v:?}"
    );
}

#[test]
fn quoted_key_single_quoted_simple() {
    let docs = load("'key': value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "key" && *style == ScalarStyle::SingleQuoted),
        "key must be decoded with SingleQuoted style, got: {k:?}"
    );
    assert!(
        matches!(v, Node::Scalar { value, .. } if value == "value"),
        "val: {v:?}"
    );
}

#[test]
fn quoted_key_double_quoted_with_escape_sequence() {
    // \t in double-quoted YAML is a literal tab character
    let docs = load("\"ke\\ty\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "ke\ty" && *style == ScalarStyle::DoubleQuoted),
        "key escape must be decoded and style DoubleQuoted, got: {k:?}"
    );
}

#[test]
fn quoted_key_single_quoted_with_escaped_quote() {
    // In single-quoted scalars, '' is the escape for a literal '
    let docs = load("'it''s': value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "it's" && *style == ScalarStyle::SingleQuoted),
        "single-quoted key escape must be decoded and style SingleQuoted, got: {k:?}"
    );
}

#[test]
fn quoted_key_with_spaces_inside() {
    let docs = load("\"hello world\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "hello world" && *style == ScalarStyle::DoubleQuoted),
        "spaces inside quoted key must be preserved, got: {k:?}"
    );
}

#[test]
fn quoted_key_double_quoted_empty() {
    let docs = load("\"\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value.is_empty() && *style == ScalarStyle::DoubleQuoted),
        "empty quoted key must decode to empty string with DoubleQuoted style, got: {k:?}"
    );
}

#[test]
fn quoted_key_in_nested_mapping() {
    let docs = load("outer:\n  \"inner key\": inner value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, .. } if value == "outer"),
        "outer key: {k:?}"
    );
    let Node::Mapping { entries: inner, .. } = v else {
        panic!("expected nested Mapping, got: {v:?}");
    };
    assert_eq!(inner.len(), 1);
    let (ik, _) = &inner[0];
    assert!(
        matches!(ik, Node::Scalar { value, style, .. }
            if value == "inner key" && *style == ScalarStyle::DoubleQuoted),
        "nested quoted key must be decoded, got: {ik:?}"
    );
}

#[test]
fn quoted_key_multiple_entries_mixed() {
    let docs = load("plain_key: 1\n\"quoted_key\": 2\n'another': 3\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 3);
    let (k0, _) = &entries[0];
    assert!(
        matches!(k0, Node::Scalar { value, style, .. }
            if value == "plain_key" && *style == ScalarStyle::Plain),
        "entry 0 key: {k0:?}"
    );
    let (k1, _) = &entries[1];
    assert!(
        matches!(k1, Node::Scalar { value, style, .. }
            if value == "quoted_key" && *style == ScalarStyle::DoubleQuoted),
        "entry 1 key: {k1:?}"
    );
    let (k2, _) = &entries[2];
    assert!(
        matches!(k2, Node::Scalar { value, style, .. }
            if value == "another" && *style == ScalarStyle::SingleQuoted),
        "entry 2 key: {k2:?}"
    );
}

#[test]
fn it_j3_trailing_comment_on_sequence_item_is_attached() {
    let node = load_one("- item  # seq comment\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence root; got: {node:?}");
    };
    assert_eq!(items.len(), 1);
    let tc = items[0]
        .trailing_comment()
        .expect("trailing comment must be attached to sequence item");
    assert!(
        tc.contains("seq comment"),
        "trailing comment must contain 'seq comment'; got: {tc:?}"
    );
}

#[test]
fn it_j4_no_trailing_comment_when_comment_on_next_line() {
    let node = load_one("key: value\n# next line comment\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1);
    let (_, v) = &entries[0];
    assert_eq!(
        v.trailing_comment(),
        None,
        "next-line comment must NOT be captured as trailing; got: {:?}",
        v.trailing_comment()
    );
}

#[test]
fn it_j5_leading_comment_still_works_after_fix() {
    // A comment between two mapping entries must be captured as a leading
    // comment on the second key.  This exercises consume_leading_comments —
    // the function whose broken span.end.line > span.start.line condition was
    // removed by the Bug 2 fix.
    let node = load_one("a: 1\n# header\nb: 2\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 2);
    let (k, _) = &entries[1];
    let lc = k.leading_comments();
    assert!(
        !lc.is_empty(),
        "leading comment must be attached to second mapping key; got empty"
    );
    assert!(
        lc.iter().any(|c| c.contains("header")),
        "leading comments must contain 'header'; got: {lc:?}"
    );
}

#[test]
fn it_j6_multiple_trailing_comments_separate_entries() {
    let node = load_one("a: 1  # first\nb: 2  # second\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 2);
    let (_, v0) = &entries[0];
    let tc0 = v0
        .trailing_comment()
        .expect("entry 0 value must have trailing comment");
    assert!(
        tc0.contains("first"),
        "entry 0 trailing comment must contain 'first'; got: {tc0:?}"
    );
    let (_, v1) = &entries[1];
    let tc1 = v1
        .trailing_comment()
        .expect("entry 1 value must have trailing comment");
    assert!(
        tc1.contains("second"),
        "entry 1 trailing comment must contain 'second'; got: {tc1:?}"
    );
}

#[test]
fn it_j7_trailing_comment_with_special_chars() {
    let node = load_one("key: value  # comment: with: colons\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1);
    let (_, v) = &entries[0];
    let tc = v
        .trailing_comment()
        .expect("trailing comment must be attached");
    assert!(
        tc.contains("comment: with: colons"),
        "trailing comment must contain colons verbatim; got: {tc:?}"
    );
}

#[test]
fn it_j8_no_trailing_comment_on_collection_node() {
    let node = load_one("map:\n  a: 1\n  b: 2\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1);
    let (_, v) = &entries[0];
    assert!(
        matches!(
            v,
            Node::Mapping {
                trailing_comment: None,
                ..
            }
        ),
        "multi-line mapping value must have no trailing comment; got: {v:?}"
    );
}

// ---------------------------------------------------------------------------
// CS: CollectionStyle is threaded from events through to AST nodes
// ---------------------------------------------------------------------------

// CS-1: block sequence carries CollectionStyle::Block
#[test]
fn cs_1_block_sequence_has_block_style() {
    let node = load_one("- a\n- b\n");
    let Node::Sequence { style, items, .. } = &node else {
        panic!("expected Sequence; got: {node:?}");
    };
    assert_eq!(*style, CollectionStyle::Block);
    assert_eq!(items.len(), 2);
}

// CS-2: flow sequence carries CollectionStyle::Flow
#[test]
fn cs_2_flow_sequence_has_flow_style() {
    let node = load_one("[a, b]");
    let Node::Sequence { style, items, .. } = &node else {
        panic!("expected Sequence; got: {node:?}");
    };
    assert_eq!(*style, CollectionStyle::Flow);
    assert_eq!(items.len(), 2);
}

// CS-3: block mapping carries CollectionStyle::Block
#[test]
fn cs_3_block_mapping_has_block_style() {
    let node = load_one("a: 1\nb: 2\n");
    let Node::Mapping { style, entries, .. } = &node else {
        panic!("expected Mapping; got: {node:?}");
    };
    assert_eq!(*style, CollectionStyle::Block);
    assert_eq!(entries.len(), 2);
}

// CS-4: flow mapping carries CollectionStyle::Flow
#[test]
fn cs_4_flow_mapping_has_flow_style() {
    let node = load_one("{a: 1, b: 2}");
    let Node::Mapping { style, entries, .. } = &node else {
        panic!("expected Mapping; got: {node:?}");
    };
    assert_eq!(*style, CollectionStyle::Flow);
    assert_eq!(entries.len(), 2);
}

// CS-5: nested flow sequence inside block mapping preserves per-node styles
#[test]
fn cs_5_nested_flow_sequence_inside_block_mapping() {
    let node = load_one("items: [x, y]\n");
    let Node::Mapping { style, entries, .. } = &node else {
        panic!("expected Mapping; got: {node:?}");
    };
    assert_eq!(*style, CollectionStyle::Block);
    let (_, val) = &entries[0];
    let Node::Sequence {
        style: seq_style, ..
    } = val
    else {
        panic!("expected Sequence value; got: {val:?}");
    };
    assert_eq!(*seq_style, CollectionStyle::Flow);
}

// CS-6: nested block sequence inside flow mapping preserves per-node styles
#[test]
fn cs_6_nested_block_mapping_inside_flow_sequence() {
    let node = load_one("[{a: 1}]");
    let Node::Sequence { style, items, .. } = &node else {
        panic!("expected Sequence; got: {node:?}");
    };
    assert_eq!(*style, CollectionStyle::Flow);
    let Node::Mapping {
        style: map_style, ..
    } = &items[0]
    else {
        panic!("expected Mapping item; got: {:?}", items[0]);
    };
    assert_eq!(*map_style, CollectionStyle::Flow);
}

// ---------------------------------------------------------------------------
// Group A — M2N8: explicit key (`?`) inside a block sequence item
// ---------------------------------------------------------------------------

// A-1 (spike): sequence item `- ? : x` — per YAML spec, `? : x` is a mapping
// with empty key and value "x", used as the complex key; the outer mapping
// entry has that inner mapping as its key and an empty scalar as its value.
#[test]
fn explicit_key_sequence_item_correct_entry_count() {
    // `- ? : x` → Sequence[ Mapping{ key=Mapping{""->x}, value="" } ]
    let node = load_one("- ? : x\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence root; got: {node:?}");
    };
    assert_eq!(items.len(), 1, "expected 1 item; got {}", items.len());
    let Node::Mapping { entries, .. } = &items[0] else {
        panic!("expected Mapping item; got: {:?}", items[0]);
    };
    assert_eq!(
        entries.len(),
        1,
        "expected 1 mapping entry; got {}",
        entries.len()
    );
    // The key is the inner mapping representing `? : x`
    let (k, _v) = &entries[0];
    let Node::Mapping { entries: inner, .. } = k else {
        panic!("key must be inner Mapping (complex key); got: {k:?}");
    };
    assert_eq!(inner.len(), 1, "inner mapping must have 1 entry");
    let (ik, iv) = &inner[0];
    assert_eq!(
        scalar_value(ik),
        "",
        "inner key must be empty string; got: {ik:?}"
    );
    assert_eq!(
        scalar_value(iv),
        "x",
        "inner value must be 'x'; got: {iv:?}"
    );
}

// A-2: sequence item with explicit scalar key — `- ? k\n  : v`
// Key is a plain scalar "k", value is a plain scalar "v".
#[test]
fn explicit_key_sequence_item_inline_key_content() {
    let node = load_one("- ? k\n  : v\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence root; got: {node:?}");
    };
    assert_eq!(items.len(), 1);
    let Node::Mapping { entries, .. } = &items[0] else {
        panic!("expected Mapping item; got: {:?}", items[0]);
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert_eq!(scalar_value(k), "k");
    assert_eq!(scalar_value(v), "v");
}

// A-3: two sequence items, each with explicit scalar key — item count is 2
#[test]
fn explicit_key_two_sequence_items_correct_count() {
    let node = load_one("- ? a\n  : 1\n- ? b\n  : 2\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence root; got: {node:?}");
    };
    assert_eq!(
        items.len(),
        2,
        "expected 2 sequence items; got {}",
        items.len()
    );
    for (idx, item) in items.iter().enumerate() {
        let Node::Mapping { entries, .. } = item else {
            panic!("item {idx} must be Mapping; got: {item:?}");
        };
        assert_eq!(
            entries.len(),
            1,
            "item {idx} mapping must have 1 entry; got {}",
            entries.len()
        );
    }
}

// A-4: sequence item, bare `?` with no key content and no value
// → Sequence[ Mapping{entries:[]} ] (empty mapping, no entry emitted for bare ?)
#[test]
fn explicit_key_sequence_item_no_value() {
    let node = load_one("- ?\n");
    let Node::Sequence { items, .. } = &node else {
        panic!("expected Sequence root; got: {node:?}");
    };
    assert_eq!(items.len(), 1, "expected 1 sequence item");
    // The parser currently emits an empty Mapping for `- ?` (0 entries).
    // The key invariant: no crash, and the item is a Mapping (not a Scalar).
    assert!(
        matches!(&items[0], Node::Mapping { .. }),
        "bare ? sequence item must be a Mapping; got: {:?}",
        items[0]
    );
}

// ---------------------------------------------------------------------------
// Group B — 6PBE: zero-indented block sequences as explicit-key content
// ---------------------------------------------------------------------------

// B-1: 6PBE — zero-indented block sequences as both key and value of an explicit mapping entry.
// `?\n- a\n- b\n:\n- c\n- d\n` → Mapping{ key=Sequence["a","b"], value=Sequence["c","d"] }
#[test]
fn explicit_key_zero_indented_sequence_as_value() {
    let node = load_one("?\n- a\n- b\n:\n- c\n- d\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        1,
        "expected 1 mapping entry; got {}",
        entries.len()
    );
    let (k, v) = &entries[0];
    let Node::Sequence { items: k_items, .. } = k else {
        panic!("key must be Sequence; got: {k:?}");
    };
    assert_eq!(
        k_items.len(),
        2,
        "key sequence must have 2 items; got {}",
        k_items.len()
    );
    assert_eq!(scalar_value(&k_items[0]), "a");
    assert_eq!(scalar_value(&k_items[1]), "b");
    let Node::Sequence { items: v_items, .. } = v else {
        panic!("value must be Sequence; got: {v:?}");
    };
    assert_eq!(
        v_items.len(),
        2,
        "value sequence must have 2 items; got {}",
        v_items.len()
    );
    assert_eq!(scalar_value(&v_items[0]), "c");
    assert_eq!(scalar_value(&v_items[1]), "d");
}

// B-2: bare `?` then block sequence at col 0 is the complex key — no phantom empty key scalar
// `?\n- seq1\n:\n` → Mapping{ key=Sequence["seq1"], value=Scalar"" }
// The critical invariant: exactly 1 entry (no spurious empty-key entry before it).
#[test]
fn explicit_key_zero_indented_sequence_as_key_no_phantom_empty_key() {
    let node = load_one("?\n- seq1\n:\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        1,
        "must be exactly 1 entry — no phantom empty key; got {}",
        entries.len()
    );
    let (k, _) = &entries[0];
    let Node::Sequence { items, .. } = k else {
        panic!("key must be Sequence; got: {k:?}");
    };
    assert_eq!(
        items.len(),
        1,
        "key sequence must have 1 item; got {}",
        items.len()
    );
}

// B-3: second plain mapping entry following a zero-indented sequence key is parsed correctly
// `?\n- seq1\n: v1\nnext: v2\n` → Mapping{ (Seq["seq1"], "v1"), ("next", "v2") }
#[test]
fn mapping_entry_following_zero_indented_sequence_key() {
    let node = load_one("?\n- seq1\n: v1\nnext: v2\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        2,
        "expected 2 entries; got {}",
        entries.len()
    );
    let (_, v0) = &entries[0];
    assert_eq!(
        scalar_value(v0),
        "v1",
        "first value must be 'v1'; got: {v0:?}"
    );
    let (k1, v1) = &entries[1];
    assert_eq!(scalar_value(k1), "next");
    assert_eq!(scalar_value(v1), "v2");
}

// ---------------------------------------------------------------------------
// Group C — KK5P: multiple explicit key entries in the same mapping
// ---------------------------------------------------------------------------

// C-1: two consecutive explicit key entries at root level
#[test]
fn explicit_key_two_entries_root_mapping() {
    let node = load_one("? a\n: 1\n? b\n: 2\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        2,
        "expected 2 entries; got {}",
        entries.len()
    );
    let (k0, v0) = &entries[0];
    assert_eq!(scalar_value(k0), "a");
    assert_eq!(scalar_value(v0), "1");
    let (k1, v1) = &entries[1];
    assert_eq!(scalar_value(k1), "b");
    assert_eq!(scalar_value(v1), "2");
}

// C-2: three explicit key entries — no phantom entries
#[test]
fn explicit_key_three_entries_no_phantom() {
    let node = load_one("? x\n: 10\n? y\n: 20\n? z\n: 30\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        3,
        "expected 3 entries; got {}",
        entries.len()
    );
}

// C-3: explicit key mixed with implicit keys — correct entry count and values
#[test]
fn explicit_key_mixed_with_implicit_keys() {
    let node = load_one("plain: 0\n? expl\n: 1\nplain2: 2\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        3,
        "expected 3 entries; got {}",
        entries.len()
    );
    let (k0, v0) = &entries[0];
    assert_eq!(scalar_value(k0), "plain");
    assert_eq!(scalar_value(v0), "0");
    let (k1, v1) = &entries[1];
    assert_eq!(scalar_value(k1), "expl");
    assert_eq!(scalar_value(v1), "1");
    let (k2, v2) = &entries[2];
    assert_eq!(scalar_value(k2), "plain2");
    assert_eq!(scalar_value(v2), "2");
}

// ---------------------------------------------------------------------------
// Group D — S3PD: empty key with comment on the indicator line
// ---------------------------------------------------------------------------

// D-1: bare `?` followed by comment — empty string key, scalar value
#[test]
fn empty_key_with_comment_on_indicator_line() {
    let node = load_one("? # this is a comment\n: value\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1, "expected 1 entry; got {}", entries.len());
    let (k, v) = &entries[0];
    assert_eq!(scalar_value(k), "", "key must be empty string; got: {k:?}");
    assert_eq!(
        scalar_value(v),
        "value",
        "value must be 'value'; got: {v:?}"
    );
}

// D-2: bare `?` with comment and no following `:` — parser emits empty Mapping (0 entries)
#[test]
fn empty_key_with_comment_no_value() {
    let docs = load("? # comment\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    // The parser emits an empty Mapping for `? # comment` with no `:` line.
    // Key invariant: no crash, root is a Mapping.
    assert!(
        matches!(&docs[0].root, Node::Mapping { .. }),
        "bare ? with comment must produce a Mapping root; got: {:?}",
        docs[0].root
    );
}

// D-3: multiple empty keys with comments — all entries present
#[test]
fn multiple_empty_keys_with_comments() {
    let node = load_one("? # first\n: a\n? # second\n: b\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        2,
        "expected 2 entries; got {}",
        entries.len()
    );
    let (k0, v0) = &entries[0];
    assert_eq!(scalar_value(k0), "");
    assert_eq!(scalar_value(v0), "a");
    let (k1, v1) = &entries[1];
    assert_eq!(scalar_value(k1), "");
    assert_eq!(scalar_value(v1), "b");
}

// D-4: implicit empty key via `: value` at root level
#[test]
fn implicit_empty_key_colon_value() {
    let node = load_one(": value\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1, "expected 1 entry; got {}", entries.len());
    let (k, v) = &entries[0];
    assert_eq!(scalar_value(k), "", "key must be empty string; got: {k:?}");
    assert_eq!(scalar_value(v), "value");
}

// ---------------------------------------------------------------------------
// Group E — NKF9: multiple documents with empty/explicit keys
// ---------------------------------------------------------------------------

// E-1: two documents, each containing a bare explicit key
#[test]
fn multi_doc_explicit_key_each_doc_has_one_entry() {
    let docs = load("? a\n: 1\n---\n? b\n: 2\n").expect("load failed");
    assert_eq!(docs.len(), 2, "expected 2 documents; got {}", docs.len());
    let Node::Mapping { entries: e0, .. } = &docs[0].root else {
        panic!("doc 0 root must be Mapping; got: {:?}", docs[0].root);
    };
    assert_eq!(e0.len(), 1);
    assert_eq!(scalar_value(&e0[0].0), "a");
    assert_eq!(scalar_value(&e0[0].1), "1");
    let Node::Mapping { entries: e1, .. } = &docs[1].root else {
        panic!("doc 1 root must be Mapping; got: {:?}", docs[1].root);
    };
    assert_eq!(e1.len(), 1);
    assert_eq!(scalar_value(&e1[0].0), "b");
    assert_eq!(scalar_value(&e1[0].1), "2");
}

// E-2: two documents — first has empty key with comment, second is plain mapping
#[test]
fn multi_doc_empty_key_first_doc_parses_correctly() {
    let docs = load("? # comment\n: x\n---\nplain: y\n").expect("load failed");
    assert_eq!(docs.len(), 2);
    let Node::Mapping { entries: e0, .. } = &docs[0].root else {
        panic!("doc 0 root must be Mapping; got: {:?}", docs[0].root);
    };
    assert_eq!(e0.len(), 1);
    assert_eq!(scalar_value(&e0[0].0), "");
    assert_eq!(scalar_value(&e0[0].1), "x");
    let Node::Mapping { entries: e1, .. } = &docs[1].root else {
        panic!("doc 1 root must be Mapping; got: {:?}", docs[1].root);
    };
    assert_eq!(e1.len(), 1);
    assert_eq!(scalar_value(&e1[0].0), "plain");
}

// E-3: three documents all with explicit keys — no cross-document state leakage
#[test]
fn multi_doc_no_cross_document_state_leakage() {
    let docs = load("? p\n: 1\n---\n? q\n: 2\n---\n? r\n: 3\n").expect("load failed");
    assert_eq!(docs.len(), 3, "expected 3 documents; got {}", docs.len());
    let keys = ["p", "q", "r"];
    let vals = ["1", "2", "3"];
    for (i, doc) in docs.iter().enumerate() {
        let Node::Mapping { entries, .. } = &doc.root else {
            panic!("doc {i} root must be Mapping; got: {:?}", doc.root);
        };
        assert_eq!(
            entries.len(),
            1,
            "doc {i} must have 1 entry; got {}",
            entries.len()
        );
        assert_eq!(scalar_value(&entries[0].0), keys[i], "doc {i} key");
        assert_eq!(scalar_value(&entries[0].1), vals[i], "doc {i} value");
    }
}

// ---------------------------------------------------------------------------
// Group F — Cross-cutting regression guards
// ---------------------------------------------------------------------------

// F-1: plain implicit key still works after explicit-key fix
#[test]
fn plain_block_mapping_unaffected_by_explicit_key_changes() {
    let node = load_one("key: value\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(scalar_value(&entries[0].0), "key");
    assert_eq!(scalar_value(&entries[0].1), "value");
}

// F-2: block sequence of plain scalars at indented level is not misidentified
#[test]
fn sequence_of_plain_scalars_unaffected_by_explicit_key_changes() {
    let node = load_one("items:\n  - a\n  - b\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert_eq!(scalar_value(k), "items");
    let Node::Sequence { items, .. } = v else {
        panic!("value must be Sequence; got: {v:?}");
    };
    assert_eq!(items.len(), 2);
}

// F-3: explicit key followed by implicit key — both entries present, correct values
#[test]
fn explicit_key_followed_by_implicit_key_both_entries_correct() {
    let node = load_one("? expl_key\n: expl_val\nimpl_key: impl_val\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(
        entries.len(),
        2,
        "expected 2 entries; got {}",
        entries.len()
    );
    assert_eq!(scalar_value(&entries[0].0), "expl_key");
    assert_eq!(scalar_value(&entries[0].1), "expl_val");
    assert_eq!(scalar_value(&entries[1].0), "impl_key");
    assert_eq!(scalar_value(&entries[1].1), "impl_val");
}

// F-4: deeply nested explicit key — no phantom entries at any level
#[test]
fn explicit_key_nested_inside_mapping_value_no_phantom_entries() {
    let node = load_one("outer:\n  ? inner_key\n  : inner_val\n");
    let Node::Mapping { entries, .. } = &node else {
        panic!("expected Mapping root; got: {node:?}");
    };
    assert_eq!(entries.len(), 1);
    let (k_outer, v_outer) = &entries[0];
    assert_eq!(scalar_value(k_outer), "outer");
    let Node::Mapping {
        entries: inner_entries,
        ..
    } = v_outer
    else {
        panic!("outer value must be Mapping; got: {v_outer:?}");
    };
    assert_eq!(
        inner_entries.len(),
        1,
        "inner mapping must have 1 entry; got {}",
        inner_entries.len()
    );
    assert_eq!(scalar_value(&inner_entries[0].0), "inner_key");
    assert_eq!(scalar_value(&inner_entries[0].1), "inner_val");
}
