use std::path::Path;

use rlsp_yaml_parser::{Node, Span};

pub fn check_i5_anchor_loc_invariant(_path: &Path, text: &str) -> Result<(), String> {
    let Ok(docs) = rlsp_yaml_parser::loader::load(text) else {
        return Ok(()); // invalid YAML has no AST to check
    };
    for doc in &docs {
        check_i5_node(&doc.root)?;
    }
    Ok(())
}

pub fn check_i5_node(node: &Node<Span>) -> Result<(), String> {
    match node {
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } => {
            let anchor = node.anchor();
            let anchor_loc = node.anchor_loc();
            if anchor.is_some() != anchor_loc.is_some() {
                return Err(format!(
                    "I5 invariant violated: anchor={anchor:?} but anchor_loc={anchor_loc:?}"
                ));
            }
        }
        Node::Alias { .. } => {}
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                check_i5_node(k)?;
                check_i5_node(v)?;
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_i5_node(item)?;
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
    Ok(())
}
