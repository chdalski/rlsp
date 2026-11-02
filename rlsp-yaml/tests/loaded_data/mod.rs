// SPDX-License-Identifier: MIT
//
// The data a document loads to, for the tests that compare the output of the
// formatter with its input.  Imported by the test crates that need it via
// `mod loaded_data;`; not part of `common`, which every test crate compiles.

use std::fmt::Write as _;

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Span, load};

/// The data of `node`: structure, scalar values, tags, anchors and aliases; not
/// spans, styles or comments.
pub fn node_signature(node: &Node<Span>) -> String {
    fn walk(node: &Node<Span>, out: &mut String) {
        let props = format!("&{:?} !{:?}", node.anchor(), tag_of(node));
        match node {
            Node::Scalar { value, .. } => {
                let _ = write!(out, "S{props}{value:?};");
            }
            Node::Alias { name, .. } => {
                let _ = write!(out, "A{name};");
            }
            Node::Mapping { entries, .. } => {
                let _ = write!(out, "M{props}(");
                for (key, value) in entries {
                    walk(key, out);
                    walk(value, out);
                }
                out.push_str(");");
            }
            Node::Sequence { items, .. } => {
                let _ = write!(out, "Q{props}[");
                for item in items {
                    walk(item, out);
                }
                out.push_str("];");
            }
        }
    }
    fn tag_of(node: &Node<Span>) -> Option<&str> {
        match node {
            Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
                tag.as_deref()
            }
            Node::Alias { .. } => None,
        }
    }
    let mut out = String::new();
    walk(node, &mut out);
    out
}

/// The data of `yaml` (see `node_signature`).  `None` when `yaml` does not load.
pub fn data_signature(yaml: &str) -> Option<String> {
    let mut out = String::new();
    for doc in load(yaml).ok()? {
        out.push_str(&node_signature(&doc.root));
        out.push('\n');
    }
    Some(out)
}
