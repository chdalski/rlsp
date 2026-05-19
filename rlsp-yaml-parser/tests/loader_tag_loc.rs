// SPDX-License-Identifier: MIT
//
// Integration tests for tag_loc threading from events through the loader
// into AST nodes.

#![expect(missing_docs, reason = "test code")]

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
        matches!(root, Node::Scalar { tag: Some(_), .. }) && root.tag_loc().is_some(),
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
    let Node::Scalar { tag, .. } = value else {
        panic!("expected Scalar value; got: {value:?}");
    };
    assert_eq!(tag.as_deref(), Some("!tag"), "tag name must be '!tag'");
    let loc = value
        .tag_loc()
        .expect("tag_loc must be Some for tagged scalar");
    assert_eq!(loc.start, 5, "tag span must start at byte 5 (the '!')");
}

// TL-2: mapping_with_anchor_and_tag_has_both_locs_some
#[test]
fn tl_2_mapping_with_anchor_and_tag_has_both_locs_some() {
    let docs = load("&a\n!!map\nk: v\n").unwrap();
    let root = &docs[0].root;
    let Node::Mapping { tag, .. } = root else {
        panic!("expected root Mapping; got: {root:?}");
    };
    assert_eq!(root.anchor(), Some("a"));
    assert!(root.anchor_loc().is_some(), "anchor_loc must be Some");
    assert!(tag.is_some(), "tag must be Some");
    assert!(
        root.tag_loc().is_some(),
        "tag_loc must be Some for tagged mapping"
    );
}

// TL-3: sequence_with_tag_has_tag_loc_some
#[test]
fn tl_3_sequence_with_tag_has_tag_loc_some() {
    let docs = load("!!seq\n- item\n").unwrap();
    let root = &docs[0].root;
    let Node::Sequence { tag, .. } = root else {
        panic!("expected root Sequence; got: {root:?}");
    };
    assert!(tag.is_some(), "tag must be Some");
    assert!(
        root.tag_loc().is_some(),
        "tag_loc must be Some for tagged sequence"
    );
}

// TL-4: untagged scalar has schema-resolved tag but no tag_loc (no source position)
#[test]
fn tl_4_tagless_scalar_has_tag_loc_none() {
    let docs = load("plain value\n").unwrap();
    let root = &docs[0].root;
    let Node::Scalar { tag, .. } = root else {
        panic!("expected root Scalar; got: {root:?}");
    };
    // Core schema resolves untagged plain scalar to !!str; tag_loc stays None
    // because the tag was not present in the source.
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:str"),
        "untagged scalar must have Core-resolved !!str tag"
    );
    assert!(
        root.tag_loc().is_none(),
        "tag_loc must be None for schema-resolved (not source-tagged) scalar"
    );
}

// TL-5: untagged mapping has schema-resolved tag but no tag_loc
#[test]
fn tl_5_tagless_mapping_has_tag_loc_none() {
    let docs = load("k: v\n").unwrap();
    let root = &docs[0].root;
    let Node::Mapping { tag, .. } = root else {
        panic!("expected root Mapping; got: {root:?}");
    };
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:map"),
        "untagged mapping must have Core-resolved !!map tag"
    );
    assert!(
        root.tag_loc().is_none(),
        "tag_loc must be None for schema-resolved (not source-tagged) mapping"
    );
}

// TL-6: untagged sequence has schema-resolved tag but no tag_loc
#[test]
fn tl_6_tagless_sequence_has_tag_loc_none() {
    let docs = load("- item\n").unwrap();
    let root = &docs[0].root;
    let Node::Sequence { tag, .. } = root else {
        panic!("expected root Sequence; got: {root:?}");
    };
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:seq"),
        "untagged sequence must have Core-resolved !!seq tag"
    );
    assert!(
        root.tag_loc().is_none(),
        "tag_loc must be None for schema-resolved (not source-tagged) sequence"
    );
}

// TL-7: verbatim_tag_on_scalar_has_tag_loc_some
#[test]
fn tl_7_verbatim_tag_on_scalar_has_tag_loc_some() {
    let docs = load("!<!my:tag> value\n").unwrap();
    let root = &docs[0].root;
    let Node::Scalar { tag, .. } = root else {
        panic!("expected root Scalar; got: {root:?}");
    };
    assert!(tag.is_some(), "tag must be Some for verbatim tag");
    assert!(
        root.tag_loc().is_some(),
        "tag_loc must be Some for verbatim-tagged scalar"
    );
}

// TL-8: handle_based_tag_via_directive_has_tag_loc_some
#[test]
fn tl_8_handle_based_tag_via_directive_has_tag_loc_some() {
    let input = "%TAG !e! tag:example.com,2000:fields/\n---\n!e!foo value\n";
    let docs = load(input).unwrap();
    let root = &docs[0].root;
    let Node::Scalar { tag, .. } = root else {
        panic!("expected root Scalar; got: {root:?}");
    };
    assert!(tag.is_some(), "tag must be Some for handle-based tag");
    assert!(
        root.tag_loc().is_some(),
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
    let Node::Scalar { tag, .. } = value else {
        panic!("expected Scalar value; got: {value:?}");
    };
    assert!(tag.is_some(), "tag must be Some");
    assert!(
        value.tag_loc().is_some(),
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
        Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
            // When a source tag is present (tag_loc: Some), the resolved tag
            // must also be present.  Schema-resolved tags (tag_loc: None) may
            // have tag: Some — this is correct and expected with the Core
            // schema default.
            let tag_loc = node.tag_loc();
            if tag_loc.is_some() {
                assert!(
                    tag.is_some(),
                    "tag_loc invariant violated: tag_loc=Some but tag=None; tag={tag:?} tag_loc={tag_loc:?}"
                );
            }
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
