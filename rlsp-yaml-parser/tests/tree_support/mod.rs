// SPDX-License-Identifier: MIT
//
// Helpers for tests of the loaded tree that two test crates of rlsp-yaml-parser
// use.  Imported via `mod tree_support;`.  They stay apart from `support`
// because a test crate that imports a module and leaves one of its functions
// unused fails the dead-code lint.

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;

/// How many collections enclose the deepest node of `root`.  Walks with a list
/// on the heap, so a deep tree costs the test no stack.
pub fn tree_depth(root: &Node<Span>) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(root, 0)];
    while let Some((node, depth)) = pending.pop() {
        match node {
            Node::Mapping { entries, .. } => {
                deepest = deepest.max(depth + 1);
                for (key, value) in entries {
                    pending.push((key, depth + 1));
                    pending.push((value, depth + 1));
                }
            }
            Node::Sequence { items, .. } => {
                deepest = deepest.max(depth + 1);
                pending.extend(items.iter().map(|item| (item, depth + 1)));
            }
            Node::Scalar { .. } | Node::Alias { .. } => {}
        }
    }
    deepest
}

/// Position of the byte `offset` of `input`: the offset, the line (from 1) and
/// the column (from 0).
pub fn position_at(input: &str, offset: usize) -> (usize, usize, usize) {
    let before = &input[..offset];
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    (
        offset,
        before.matches('\n').count() + 1,
        offset - line_start,
    )
}
