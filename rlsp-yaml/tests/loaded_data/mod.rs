// SPDX-License-Identifier: MIT
//
// The data a document loads to, for the tests that compare the output of the
// formatter with its input.  Imported by the test crates that need it via
// `mod loaded_data;`; not part of `common`, which every test crate compiles.

use std::fmt::Write as _;

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Span, load};

/// The data of `yaml`: structure, scalar values, tags, anchors and aliases; not
/// spans, styles or comments.  `None` when `yaml` does not load.
pub fn data_signature(yaml: &str) -> Option<String> {
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
            // The loader leaves the value that an entry of an explicit key lacks
            // (`? ? a`, the inner entry has no value) without the null tag that
            // an empty value written out gets (`? a: `); both are null.
            Node::Scalar {
                tag: None, value, ..
            } if value.is_empty() => Some("tag:yaml.org,2002:null"),
            Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
                tag.as_deref()
            }
            Node::Alias { .. } => None,
        }
    }
    let mut out = String::new();
    for doc in load(yaml).ok()? {
        walk(&doc.root, &mut out);
        out.push('\n');
    }
    Some(out)
}
