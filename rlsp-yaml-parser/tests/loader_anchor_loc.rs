// SPDX-License-Identifier: MIT
//
// Integration tests for anchor_loc threading from events through the loader
// into AST nodes.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    missing_docs,
    reason = "test code"
)]

use rlsp_yaml_parser::loader::load;
use rlsp_yaml_parser::node::Node;

// ---------------------------------------------------------------------------
// Spike: harness validation
// ---------------------------------------------------------------------------

// spike_anchor_loc_accessible_on_scalar_node
#[test]
fn spike_anchor_loc_accessible_on_scalar_node() {
    let docs = load("&a val\n").unwrap();
    let root = &docs[0].root;
    assert!(
        root.anchor().is_some() && root.anchor_loc().is_some(),
        "expected anchored Scalar with anchor_loc set; got: {root:?}"
    );
}

// ---------------------------------------------------------------------------
// Group AL: Loader wires anchor_loc from event to node
// ---------------------------------------------------------------------------

// AL-1: scalar_with_inline_anchor_has_anchor_loc_some
#[test]
fn al_1_scalar_with_inline_anchor_has_anchor_loc_some() {
    // "key: &a value\n"
    //  0123456789...
    // '&' is at byte offset 5
    let input = "key: &a value\n";
    let docs = load(input).unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected root Mapping");
    };
    let (_, value) = &entries[0];
    assert_eq!(value.anchor(), Some("a"), "anchor name must be 'a'");
    let loc = value
        .anchor_loc()
        .expect("anchor_loc must be Some for anchored scalar");
    assert_eq!(
        loc.start.byte_offset, 5,
        "anchor span must start at byte 5 (the '&')"
    );
}

// AL-2: block_mapping_with_standalone_anchor_has_anchor_loc_some
#[test]
fn al_2_block_mapping_with_standalone_anchor_has_anchor_loc_some() {
    let docs = load("&m\nk: v\n").unwrap();
    let root = &docs[0].root;
    assert!(
        matches!(root, Node::Mapping { .. }),
        "expected root Mapping; got: {root:?}"
    );
    assert_eq!(root.anchor(), Some("m"));
    assert!(
        root.anchor_loc().is_some(),
        "anchor_loc must be Some for anchored mapping"
    );
}

// AL-3: block_sequence_with_standalone_anchor_has_anchor_loc_some
#[test]
fn al_3_block_sequence_with_standalone_anchor_has_anchor_loc_some() {
    let docs = load("&s\n- item\n").unwrap();
    let root = &docs[0].root;
    assert!(
        matches!(root, Node::Sequence { .. }),
        "expected root Sequence; got: {root:?}"
    );
    assert_eq!(root.anchor(), Some("s"));
    assert!(
        root.anchor_loc().is_some(),
        "anchor_loc must be Some for anchored sequence"
    );
}

// AL-4: nested_mapping_value_with_anchor_has_anchor_loc_some
#[test]
fn al_4_nested_mapping_value_with_anchor_has_anchor_loc_some() {
    let docs = load("outer:\n  &n\n  inner: val\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected root Mapping");
    };
    let (_, outer_val) = &entries[0];
    assert!(
        matches!(outer_val, Node::Mapping { .. }),
        "expected nested Mapping value; got: {outer_val:?}"
    );
    assert_eq!(outer_val.anchor(), Some("n"));
    assert!(
        outer_val.anchor_loc().is_some(),
        "anchor_loc must be Some for anchored nested mapping"
    );
}

// AL-5: scalar_without_anchor_has_anchor_loc_none
#[test]
fn al_5_scalar_without_anchor_has_anchor_loc_none() {
    let docs = load("plain value\n").unwrap();
    let root = &docs[0].root;
    assert_eq!(root.anchor(), None);
    assert_eq!(root.anchor_loc(), None);
}

// AL-6: mapping_without_anchor_has_anchor_loc_none
#[test]
fn al_6_mapping_without_anchor_has_anchor_loc_none() {
    let docs = load("k: v\n").unwrap();
    let root = &docs[0].root;
    assert_eq!(root.anchor(), None);
    assert_eq!(root.anchor_loc(), None);
}

// AL-7: sequence_without_anchor_has_anchor_loc_none
#[test]
fn al_7_sequence_without_anchor_has_anchor_loc_none() {
    let docs = load("- item\n").unwrap();
    let root = &docs[0].root;
    assert_eq!(root.anchor(), None);
    assert_eq!(root.anchor_loc(), None);
}

// ---------------------------------------------------------------------------
// Group AL-ALIAS: Alias nodes return None from anchor_loc()
// ---------------------------------------------------------------------------

// AL-8: alias_node_anchor_loc_accessor_returns_none
#[test]
fn al_8_alias_node_anchor_loc_accessor_returns_none() {
    // lossless mode (default): second doc's root is Node::Alias
    let docs = load("&ref val\n---\n*ref\n").unwrap();
    assert_eq!(docs.len(), 2);
    let alias = &docs[1].root;
    assert!(
        matches!(alias, Node::Alias { .. }),
        "expected Alias node; got: {alias:?}"
    );
    assert_eq!(
        alias.anchor_loc(),
        None,
        "anchor_loc() must return None for Alias nodes"
    );
}

// ---------------------------------------------------------------------------
// Group AL-INV: AST-level invariant on synthetic document
// ---------------------------------------------------------------------------

fn walk_nodes_check_invariant(node: &Node<rlsp_yaml_parser::Span>) -> Result<(), String> {
    match node {
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } => {
            let anchor = node.anchor();
            let anchor_loc = node.anchor_loc();
            if anchor.is_some() != anchor_loc.is_some() {
                return Err(format!(
                    "invariant violated: anchor={anchor:?} but anchor_loc={anchor_loc:?}"
                ));
            }
        }
        Node::Alias { .. } => {}
    }
    // Recurse into children
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                walk_nodes_check_invariant(k)?;
                walk_nodes_check_invariant(v)?;
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                walk_nodes_check_invariant(item)?;
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
    Ok(())
}

// AL-9: anchor_loc_invariant_holds_for_all_nodes_in_complex_document
#[test]
fn al_9_anchor_loc_invariant_holds_for_all_nodes_in_complex_document() {
    let input = "&a foo\n---\n&b\n- &c item\n- plain\n";
    let docs = load(input).unwrap();
    for doc in &docs {
        walk_nodes_check_invariant(&doc.root)
            .expect("anchor_loc invariant must hold for all nodes");
    }
}
