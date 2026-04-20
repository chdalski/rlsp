// SPDX-License-Identifier: MIT
//
// Integration tests for tag_loc threading from events through the loader
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

// spike_tag_loc_accessible_on_scalar_node
#[test]
fn spike_tag_loc_accessible_on_scalar_node() {
    let docs = load("!str val\n").unwrap();
    let root = &docs[0].root;
    assert!(
        matches!(
            root,
            Node::Scalar {
                tag: Some(_),
                tag_loc: Some(_),
                ..
            }
        ),
        "expected tagged Scalar with tag_loc set; got: {root:?}"
    );
}

// ---------------------------------------------------------------------------
// Group TL: Loader wires tag_loc from event to AST node
// ---------------------------------------------------------------------------

// TL-1: scalar_with_primary_tag_has_tag_loc_some_starting_at_bang
#[test]
fn tl_1_scalar_with_primary_tag_has_tag_loc_some_starting_at_bang() {
    // "key: !tag value\n"
    //  01234567...
    // '!' is at byte offset 5
    let input = "key: !tag value\n";
    let docs = load(input).unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected root Mapping");
    };
    let (_, value) = &entries[0];
    let Node::Scalar { tag, tag_loc, .. } = value else {
        panic!("expected Scalar value; got: {value:?}");
    };
    assert_eq!(tag.as_deref(), Some("!tag"), "tag name must be '!tag'");
    let loc = tag_loc.expect("tag_loc must be Some for tagged scalar");
    assert_eq!(
        loc.start.byte_offset, 5,
        "tag span must start at byte 5 (the '!')"
    );
}

// TL-2: mapping_with_anchor_and_tag_has_both_locs_some
#[test]
fn tl_2_mapping_with_anchor_and_tag_has_both_locs_some() {
    let docs = load("&a\n!!map\nk: v\n").unwrap();
    let root = &docs[0].root;
    let Node::Mapping {
        anchor,
        anchor_loc,
        tag,
        tag_loc,
        ..
    } = root
    else {
        panic!("expected root Mapping; got: {root:?}");
    };
    assert_eq!(anchor.as_deref(), Some("a"));
    assert!(anchor_loc.is_some(), "anchor_loc must be Some");
    assert!(tag.is_some(), "tag must be Some");
    assert!(tag_loc.is_some(), "tag_loc must be Some for tagged mapping");
}

// TL-3: sequence_with_tag_has_tag_loc_some
#[test]
fn tl_3_sequence_with_tag_has_tag_loc_some() {
    let docs = load("!!seq\n- item\n").unwrap();
    let root = &docs[0].root;
    let Node::Sequence { tag, tag_loc, .. } = root else {
        panic!("expected root Sequence; got: {root:?}");
    };
    assert!(tag.is_some(), "tag must be Some");
    assert!(
        tag_loc.is_some(),
        "tag_loc must be Some for tagged sequence"
    );
}

// TL-4: tagless_scalar_has_tag_loc_none
#[test]
fn tl_4_tagless_scalar_has_tag_loc_none() {
    let docs = load("plain value\n").unwrap();
    let root = &docs[0].root;
    let Node::Scalar { tag, tag_loc, .. } = root else {
        panic!("expected root Scalar; got: {root:?}");
    };
    assert!(tag.is_none(), "tag must be None");
    assert!(
        tag_loc.is_none(),
        "tag_loc must be None for untagged scalar"
    );
}

// TL-5: tagless_mapping_has_tag_loc_none
#[test]
fn tl_5_tagless_mapping_has_tag_loc_none() {
    let docs = load("k: v\n").unwrap();
    let root = &docs[0].root;
    let Node::Mapping { tag, tag_loc, .. } = root else {
        panic!("expected root Mapping; got: {root:?}");
    };
    assert!(tag.is_none(), "tag must be None");
    assert!(
        tag_loc.is_none(),
        "tag_loc must be None for untagged mapping"
    );
}

// TL-6: tagless_sequence_has_tag_loc_none
#[test]
fn tl_6_tagless_sequence_has_tag_loc_none() {
    let docs = load("- item\n").unwrap();
    let root = &docs[0].root;
    let Node::Sequence { tag, tag_loc, .. } = root else {
        panic!("expected root Sequence; got: {root:?}");
    };
    assert!(tag.is_none(), "tag must be None");
    assert!(
        tag_loc.is_none(),
        "tag_loc must be None for untagged sequence"
    );
}

// TL-7: verbatim_tag_on_scalar_has_tag_loc_some
#[test]
fn tl_7_verbatim_tag_on_scalar_has_tag_loc_some() {
    let docs = load("!<!my:tag> value\n").unwrap();
    let root = &docs[0].root;
    let Node::Scalar { tag, tag_loc, .. } = root else {
        panic!("expected root Scalar; got: {root:?}");
    };
    assert!(tag.is_some(), "tag must be Some for verbatim tag");
    assert!(
        tag_loc.is_some(),
        "tag_loc must be Some for verbatim-tagged scalar"
    );
}

// TL-8: handle_based_tag_via_directive_has_tag_loc_some
#[test]
fn tl_8_handle_based_tag_via_directive_has_tag_loc_some() {
    let input = "%TAG !e! tag:example.com,2000:fields/\n---\n!e!foo value\n";
    let docs = load(input).unwrap();
    let root = &docs[0].root;
    let Node::Scalar { tag, tag_loc, .. } = root else {
        panic!("expected root Scalar; got: {root:?}");
    };
    assert!(tag.is_some(), "tag must be Some for handle-based tag");
    assert!(
        tag_loc.is_some(),
        "tag_loc must be Some for handle-based tagged scalar"
    );
}

// TL-9: nested_mapping_value_with_tag_has_tag_loc_some
#[test]
fn tl_9_nested_mapping_value_with_tag_has_tag_loc_some() {
    let docs = load("outer:\n  !!str inner\n").unwrap();
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected root Mapping");
    };
    let (_, value) = &entries[0];
    let Node::Scalar { tag, tag_loc, .. } = value else {
        panic!("expected Scalar value; got: {value:?}");
    };
    assert!(tag.is_some(), "tag must be Some");
    assert!(
        tag_loc.is_some(),
        "tag_loc must be Some for nested tagged scalar"
    );
}

// TL-10: tag_loc_invariant_holds_for_all_nodes_in_complex_document
#[test]
fn tl_10_tag_loc_invariant_holds_for_all_nodes_in_complex_document() {
    let input = "!s foo\n---\n!!map\nk: !!str val\n---\n- !!seq\n  - x\n";
    let docs = load(input).unwrap();
    for doc in &docs {
        check_tag_loc_invariant(&doc.root);
    }
}

fn check_tag_loc_invariant(node: &Node) {
    match node {
        Node::Scalar { tag, tag_loc, .. }
        | Node::Mapping { tag, tag_loc, .. }
        | Node::Sequence { tag, tag_loc, .. } => {
            assert_eq!(
                tag.is_some(),
                tag_loc.is_some(),
                "tag_loc invariant violated: tag={tag:?} tag_loc={tag_loc:?}"
            );
        }
        Node::Alias { .. } => {}
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                check_tag_loc_invariant(k);
                check_tag_loc_invariant(v);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_tag_loc_invariant(item);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}
