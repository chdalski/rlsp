// SPDX-License-Identifier: MIT
//
// Loader conformance test: runs `load()` against every case in the
// yaml-test-suite and verifies the resulting AST against the expected event
// tree embedded in each test-suite file.
//
// Structural verification is full-fidelity: document count, node kinds,
// scalar values, scalar styles, anchors, tags, and alias names are all
// checked. For block scalars (`|` / `>`) only the style variant is checked;
// the `Chomp` sub-variant is not encoded in the event tree and is not
// asserted.
//
// All cases are expected to pass structural verification; a failure is a
// regression.

// These lints are expected in test code and suppressed module-wide.
#![expect(
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::wildcard_enum_match_arm,
    clippy::doc_markdown,
    reason = "test code"
)]

use std::path::PathBuf;
use std::time::Duration;

use rlsp_yaml_parser::loader::{LoadError, LoaderBuilder, load};
use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{CollectionStyle, ScalarStyle, Span};
use rstest::rstest;

use super::{ConformanceCase, load_cases_from_file};

// ---- Expected AST model (parsed from event tree) ----------------------------

/// A node expected by the event tree.
#[derive(Debug, Clone)]
enum ExpectedNode {
    Scalar {
        value: String,
        style: StyleVariant,
        anchor: Option<String>,
        tag: Option<String>,
    },
    Mapping {
        entries: Vec<(Self, Self)>,
        style: CollectionStyle,
        anchor: Option<String>,
        tag: Option<String>,
    },
    Sequence {
        items: Vec<Self>,
        style: CollectionStyle,
        anchor: Option<String>,
        tag: Option<String>,
    },
    Alias {
        name: String,
    },
}

/// Scalar style variant without the Chomp sub-variant — the event tree does
/// not encode chomp, so we only match the outer variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StyleVariant {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
    Folded,
}

impl StyleVariant {
    const fn matches(self, style: ScalarStyle) -> bool {
        matches!(
            (self, style),
            (Self::Plain, ScalarStyle::Plain)
                | (Self::SingleQuoted, ScalarStyle::SingleQuoted)
                | (Self::DoubleQuoted, ScalarStyle::DoubleQuoted)
                | (Self::Literal, ScalarStyle::Literal(_))
                | (Self::Folded, ScalarStyle::Folded(_))
        )
    }
}

#[derive(Debug)]
struct ExpectedDocument {
    root: ExpectedNode,
}

// ---- Event-tree parser ------------------------------------------------------
//
// Tokens from the yaml-test-suite event tree are parsed line by line.
// The leading whitespace (indent) is stripped; only the content matters.

/// Decode yaml-test-suite escape sequences in a tree scalar value.
///
/// The tree format uses `\n`, `\t`, `\r`, and `\\` to represent the
/// corresponding control characters. This mirrors the encoding used in the
/// yaml-test-suite repository's event-tree files.
fn unescape_tree_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('0') => out.push('\0'),
                Some('a') => out.push('\x07'),
                Some('b') => out.push('\x08'),
                Some('e') => out.push('\x1B'),
                Some('f') => out.push('\x0C'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('v') => out.push('\x0B'),
                Some('r') => out.push('\r'),
                Some('\\') | None => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// A single token parsed from one tree line.
#[derive(Debug)]
enum TreeToken {
    StreamStart,
    StreamEnd,
    DocStart,
    DocEnd,
    SeqStart {
        anchor: Option<String>,
        tag: Option<String>,
        style: CollectionStyle,
    },
    SeqEnd,
    MapStart {
        anchor: Option<String>,
        tag: Option<String>,
        style: CollectionStyle,
    },
    MapEnd,
    Scalar {
        anchor: Option<String>,
        tag: Option<String>,
        style: StyleVariant,
        value: String,
    },
    Alias {
        name: String,
    },
}

/// Parse one trimmed tree line into a `TreeToken`.
///
/// Returns `None` for unrecognized or empty lines.
fn parse_tree_line(line: &str) -> Option<TreeToken> {
    // Strip only leading whitespace so that trailing spaces in scalar values
    // (e.g. `=VAL 'foo ` where ` ` is part of the value) are preserved.
    let line = line.trim_start();
    if line.trim_end().is_empty() {
        return None;
    }
    // For token-type comparisons that have no trailing content, also trim the
    // right so that accidental trailing whitespace in the tree file is ignored.
    let line_trimmed = line.trim_end();

    if line_trimmed == "+STR" {
        return Some(TreeToken::StreamStart);
    }
    if line_trimmed == "-STR" {
        return Some(TreeToken::StreamEnd);
    }
    if line_trimmed.starts_with("+DOC") {
        return Some(TreeToken::DocStart);
    }
    if line_trimmed.starts_with("-DOC") {
        return Some(TreeToken::DocEnd);
    }
    if line_trimmed.starts_with("-SEQ") {
        return Some(TreeToken::SeqEnd);
    }
    if line_trimmed.starts_with("-MAP") {
        return Some(TreeToken::MapEnd);
    }

    if let Some(rest) = line_trimmed.strip_prefix("+SEQ") {
        let rest = rest.trim();
        let style = if rest.starts_with("[]") {
            CollectionStyle::Flow
        } else {
            CollectionStyle::Block
        };
        // Strip flow marker before parsing optional anchor/tag
        let rest = rest.trim_start_matches("[]").trim();
        let (anchor, tag, _) = parse_optional_anchor_tag(rest);
        return Some(TreeToken::SeqStart { anchor, tag, style });
    }

    if let Some(rest) = line_trimmed.strip_prefix("+MAP") {
        let rest = rest.trim();
        let style = if rest.starts_with("{}") {
            CollectionStyle::Flow
        } else {
            CollectionStyle::Block
        };
        let rest = rest.trim_start_matches("{}").trim();
        let (anchor, tag, _) = parse_optional_anchor_tag(rest);
        return Some(TreeToken::MapStart { anchor, tag, style });
    }

    if let Some(rest) = line_trimmed.strip_prefix("=ALI") {
        let name = rest
            .trim()
            .strip_prefix('*')
            .unwrap_or_else(|| rest.trim())
            .to_string();
        return Some(TreeToken::Alias { name });
    }

    // For =VAL, use the leading-space-trimmed line (not right-trimmed) so that
    // trailing spaces in the scalar value are preserved (e.g. `=VAL 'foo ` where
    // the trailing space is part of the value).
    if let Some(rest) = line.strip_prefix("=VAL") {
        let rest = rest.trim_start();
        let (anchor, tag, rest) = parse_optional_anchor_tag(rest);
        // First character of `rest` is the style prefix.
        // The value portion uses yaml-test-suite escape notation:
        // `\n` → newline, `\t` → tab, `\r` → CR, `\\` → backslash.
        let (style, raw_value) = match rest.chars().next() {
            Some(':') => (StyleVariant::Plain, &rest[1..]),
            Some('\'') => (StyleVariant::SingleQuoted, &rest[1..]),
            Some('"') => (StyleVariant::DoubleQuoted, &rest[1..]),
            Some('|') => (StyleVariant::Literal, &rest[1..]),
            Some('>') => (StyleVariant::Folded, &rest[1..]),
            _ => return None,
        };
        let value = unescape_tree_value(raw_value);
        return Some(TreeToken::Scalar {
            anchor,
            tag,
            style,
            value,
        });
    }

    None
}

/// Parse optional `&anchor` and `<tag>` prefix from a token suffix string.
///
/// Returns `(anchor, tag, remaining)`.  Each component may be absent.
/// Handles the formats:
///   `&anchor rest`
///   `<tag> rest`
///   `&anchor <tag> rest`
///   (none of the above) → all `None`
fn parse_optional_anchor_tag(s: &str) -> (Option<String>, Option<String>, &str) {
    let mut rest = s;
    let mut anchor: Option<String> = None;
    let mut tag: Option<String> = None;

    // Optional anchor: `&name` (everything up to the first space or end)
    if rest.starts_with('&') {
        let end = rest.find(' ').unwrap_or(rest.len());
        anchor = Some(rest[1..end].to_string());
        rest = rest[end..].trim_start();
    }

    // Optional tag: `<content>`
    if rest.starts_with('<') {
        if let Some(close) = rest.find('>') {
            tag = Some(rest[1..close].to_string());
            rest = rest[close + 1..].trim_start();
        }
    }

    (anchor, tag, rest)
}

/// Build `Vec<ExpectedDocument>` from the raw event-tree string.
///
/// Returns `None` if the tree is empty or cannot be parsed (e.g. partial tree
/// in error cases). The caller skips structural verification when `None`.
fn parse_expected_documents(tree: &str) -> Option<Vec<ExpectedDocument>> {
    let tokens: Vec<TreeToken> = tree.lines().filter_map(parse_tree_line).collect();

    let mut docs: Vec<ExpectedDocument> = Vec::new();
    let mut pos = 0;

    // Skip leading +STR
    if matches!(tokens.get(pos), Some(TreeToken::StreamStart)) {
        pos += 1;
    }

    while pos < tokens.len() {
        match &tokens[pos] {
            TreeToken::DocStart => {
                pos += 1;
                if let Some((node, next_pos)) = parse_expected_node(&tokens, pos) {
                    docs.push(ExpectedDocument { root: node });
                    pos = next_pos;
                } else {
                    return None;
                }
                // Consume -DOC
                if matches!(tokens.get(pos), Some(TreeToken::DocEnd)) {
                    pos += 1;
                }
            }
            TreeToken::StreamEnd => break,
            _ => {
                pos += 1;
            }
        }
    }

    Some(docs)
}

/// Recursively parse one expected node starting at `tokens[pos]`.
///
/// Returns `(node, next_pos)` where `next_pos` is the index after this node.
fn parse_expected_node(tokens: &[TreeToken], pos: usize) -> Option<(ExpectedNode, usize)> {
    let token = tokens.get(pos)?;
    match token {
        TreeToken::Scalar {
            anchor,
            tag,
            style,
            value,
        } => Some((
            ExpectedNode::Scalar {
                value: value.clone(),
                style: *style,
                anchor: anchor.clone(),
                tag: tag.clone(),
            },
            pos + 1,
        )),
        TreeToken::Alias { name } => Some((ExpectedNode::Alias { name: name.clone() }, pos + 1)),
        TreeToken::SeqStart { anchor, tag, style } => {
            let anchor = anchor.clone();
            let tag = tag.clone();
            let style = *style;
            let mut items: Vec<ExpectedNode> = Vec::new();
            let mut cur = pos + 1;
            loop {
                match tokens.get(cur) {
                    Some(TreeToken::SeqEnd) => {
                        cur += 1;
                        break;
                    }
                    None => return None,
                    _ => {
                        let (item, next) = parse_expected_node(tokens, cur)?;
                        items.push(item);
                        cur = next;
                    }
                }
            }
            Some((
                ExpectedNode::Sequence {
                    items,
                    style,
                    anchor,
                    tag,
                },
                cur,
            ))
        }
        TreeToken::MapStart { anchor, tag, style } => {
            let anchor = anchor.clone();
            let tag = tag.clone();
            let style = *style;
            let mut entries: Vec<(ExpectedNode, ExpectedNode)> = Vec::new();
            let mut cur = pos + 1;
            loop {
                match tokens.get(cur) {
                    Some(TreeToken::MapEnd) => {
                        cur += 1;
                        break;
                    }
                    None => return None,
                    _ => {
                        let (key, next_k) = parse_expected_node(tokens, cur)?;
                        let (val, next_v) = parse_expected_node(tokens, next_k)?;
                        entries.push((key, val));
                        cur = next_v;
                    }
                }
            }
            Some((
                ExpectedNode::Mapping {
                    entries,
                    style,
                    anchor,
                    tag,
                },
                cur,
            ))
        }
        _ => None,
    }
}

// ---- AST comparison ---------------------------------------------------------

/// Walk the loaded AST and the expected node in parallel, asserting each field.
fn assert_node(actual: &Node<Span>, expected: &ExpectedNode, path: &str) {
    match (actual, expected) {
        (
            Node::Scalar {
                value,
                style,
                anchor,
                tag,
                ..
            },
            ExpectedNode::Scalar {
                value: exp_value,
                style: exp_style,
                anchor: exp_anchor,
                tag: exp_tag,
            },
        ) => {
            assert_eq!(
                value, exp_value,
                "{path}: scalar value mismatch: got {value:?}, expected {exp_value:?}"
            );
            assert!(
                exp_style.matches(*style),
                "{path}: scalar style mismatch: got {style:?}, expected {exp_style:?}"
            );
            assert_eq!(
                anchor.as_deref(),
                exp_anchor.as_deref(),
                "{path}: scalar anchor mismatch: got {anchor:?}, expected {exp_anchor:?}"
            );
            assert_eq!(
                tag.as_deref(),
                exp_tag.as_deref(),
                "{path}: scalar tag mismatch: got {tag:?}, expected {exp_tag:?}"
            );
        }
        (Node::Alias { name, .. }, ExpectedNode::Alias { name: exp_name }) => {
            assert_eq!(
                name, exp_name,
                "{path}: alias name mismatch: got {name:?}, expected {exp_name:?}"
            );
        }
        (
            Node::Sequence {
                items,
                style,
                anchor,
                tag,
                ..
            },
            ExpectedNode::Sequence {
                items: exp_items,
                style: exp_style,
                anchor: exp_anchor,
                tag: exp_tag,
            },
        ) => {
            assert_eq!(
                *style, *exp_style,
                "{path}: sequence style mismatch: got {style:?}, expected {exp_style:?}"
            );
            assert_eq!(
                anchor.as_deref(),
                exp_anchor.as_deref(),
                "{path}: sequence anchor mismatch"
            );
            assert_eq!(
                tag.as_deref(),
                exp_tag.as_deref(),
                "{path}: sequence tag mismatch"
            );
            assert_eq!(
                items.len(),
                exp_items.len(),
                "{path}: sequence length mismatch: got {}, expected {}",
                items.len(),
                exp_items.len()
            );
            for (i, (item, exp_item)) in items.iter().zip(exp_items.iter()).enumerate() {
                assert_node(item, exp_item, &format!("{path}[{i}]"));
            }
        }
        (
            Node::Mapping {
                entries,
                style,
                anchor,
                tag,
                ..
            },
            ExpectedNode::Mapping {
                entries: exp_entries,
                style: exp_style,
                anchor: exp_anchor,
                tag: exp_tag,
            },
        ) => {
            assert_eq!(
                *style, *exp_style,
                "{path}: mapping style mismatch: got {style:?}, expected {exp_style:?}"
            );
            assert_eq!(
                anchor.as_deref(),
                exp_anchor.as_deref(),
                "{path}: mapping anchor mismatch"
            );
            assert_eq!(
                tag.as_deref(),
                exp_tag.as_deref(),
                "{path}: mapping tag mismatch"
            );
            assert_eq!(
                entries.len(),
                exp_entries.len(),
                "{path}: mapping entry count mismatch: got {}, expected {}",
                entries.len(),
                exp_entries.len()
            );
            for (i, ((k, v), (exp_k, exp_v))) in entries.iter().zip(exp_entries.iter()).enumerate()
            {
                assert_node(k, exp_k, &format!("{path}.key[{i}]"));
                assert_node(v, exp_v, &format!("{path}.val[{i}]"));
            }
        }
        _ => {
            panic!(
                "{path}: node kind mismatch: got {:?} but expected {:?}",
                node_kind_name(actual),
                expected_kind_name(expected)
            );
        }
    }
}

#[expect(clippy::missing_const_for_fn, reason = "test helper")]
fn node_kind_name(node: &Node<Span>) -> &'static str {
    match node {
        Node::Scalar { .. } => "Scalar",
        Node::Mapping { .. } => "Mapping",
        Node::Sequence { .. } => "Sequence",
        Node::Alias { .. } => "Alias",
    }
}

#[expect(clippy::missing_const_for_fn, reason = "test helper")]
fn expected_kind_name(node: &ExpectedNode) -> &'static str {
    match node {
        ExpectedNode::Scalar { .. } => "Scalar",
        ExpectedNode::Mapping { .. } => "Mapping",
        ExpectedNode::Sequence { .. } => "Sequence",
        ExpectedNode::Alias { .. } => "Alias",
    }
}

fn assert_documents(docs: &[Document<Span>], expected: &[ExpectedDocument], tag: &str) {
    assert_eq!(
        docs.len(),
        expected.len(),
        "{tag}: document count mismatch: got {}, expected {}",
        docs.len(),
        expected.len()
    );
    for (i, (doc, exp_doc)) in docs.iter().zip(expected.iter()).enumerate() {
        assert_node(&doc.root, &exp_doc.root, &format!("{tag}[doc {i}]"));
    }
}

// ---- Harness spike ----------------------------------------------------------

/// LC-0: validates harness before the parameterized suite runs.
#[test]
fn spike_sequence_of_mappings_loads_correctly() {
    // 229Q.yaml index 0 — Spec Example 2.4, sequence of mappings
    let yaml = "\
-
  name: Mark McGwire
  hr:   65
  avg:  0.278
-
  name: Sammy Sosa
  hr:   63
  avg:  0.288
";
    let docs = load(yaml).expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document");
    assert!(
        matches!(&docs[0].root, Node::Sequence { .. }),
        "expected Sequence root, got: {:?}",
        node_kind_name(&docs[0].root)
    );
}

// ---- Parameterized conformance test (rstest #[files]) -----------------------

#[rstest]
#[timeout(Duration::from_secs(5))]
pub fn yaml_test_suite(#[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf) {
    let cases = load_cases_from_file(&path);
    if cases.is_empty() {
        return;
    }

    for case in &cases {
        assert_case(case);
    }
}

fn assert_case(case: &ConformanceCase) {
    let tag = format!("{}[{}] {}", case.file, case.index, case.name);

    // Fail cases: skip tree check (stream test is the authority on fail cases).
    if case.fail {
        return;
    }

    // LC-2: non-fail cases must load without error.
    let docs = load(&case.yaml).unwrap_or_else(|e| panic!("{tag}: load() returned error: {e}"));

    // LC-3 / LC-4 / LC-5..LC-14: verify against event tree if available.
    if let Some(tree_expected) = case.tree.as_deref().and_then(parse_expected_documents) {
        assert_documents(&docs, &tree_expected, &tag);
    }
}

// ---- Edge-case integration tests --------------------------------------------

/// LC-E1: empty YAML input produces zero documents.
#[test]
fn empty_yaml_loads_to_zero_documents() {
    let docs = load("").expect("load failed");
    assert_eq!(docs.len(), 0);
}

/// LC-E2: null scalar (bare `key:`) produces empty-string scalar value.
#[test]
fn null_scalar_produces_empty_string_value() {
    let docs = load("key:\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    match &docs[0].root {
        Node::Mapping { entries, .. } => {
            assert_eq!(entries.len(), 1);
            let (_, val) = &entries[0];
            assert!(
                matches!(val, Node::Scalar { value, style, .. }
                    if value.is_empty() && *style == ScalarStyle::Plain),
                "expected empty plain scalar, got: {val:?}"
            );
        }
        other => panic!("expected Mapping root, got: {other:?}"),
    }
}

/// LC-E3: alias node is not expanded in lossless mode.
#[test]
fn alias_not_expanded_in_lossless_mode() {
    let yaml = "a: &anchor foo\nb: *anchor\n";
    let docs = load(yaml).expect("load failed");
    assert_eq!(docs.len(), 1);
    match &docs[0].root {
        Node::Mapping { entries, .. } => {
            assert_eq!(entries.len(), 2);
            let (_, val_b) = &entries[1];
            assert!(
                matches!(val_b, Node::Alias { name, .. } if name == "anchor"),
                "expected Alias(anchor), got: {val_b:?}"
            );
        }
        other => panic!("expected Mapping root, got: {other:?}"),
    }
}

/// LC-E3b: lossless mode preserves undefined aliases as Alias nodes (no error).
/// LC-E4: undefined alias in resolved mode returns `LoadError::UndefinedAlias`.
#[test]
fn lossless_mode_preserves_undefined_alias() {
    // In lossless mode, alias resolution is deferred — undefined aliases are
    // kept as Node::Alias without error.
    let docs = load("key: *undefined\n").expect("load should succeed in lossless mode");
    assert_eq!(docs.len(), 1);
    match &docs[0].root {
        Node::Mapping { entries, .. } => {
            let (_, val) = &entries[0];
            assert!(
                matches!(val, Node::Alias { name, .. } if name == "undefined"),
                "expected Alias(undefined), got: {val:?}"
            );
        }
        other => panic!("expected Mapping, got: {other:?}"),
    }
}

#[test]
fn undefined_alias_in_resolved_mode_returns_error() {
    let result = LoaderBuilder::new()
        .resolved()
        .build()
        .load("key: *undefined\n");
    assert!(
        matches!(result, Err(LoadError::UndefinedAlias { ref name }) if name == "undefined"),
        "expected UndefinedAlias, got: {result:?}"
    );
}

/// LC-E5: nesting depth limit returns `LoadError::NestingDepthLimitExceeded`.
#[test]
fn nesting_depth_limit_exceeded_returns_error() {
    // Three levels deep: a sequence containing a sequence containing a sequence.
    let yaml = "- - - value\n";
    let result = LoaderBuilder::new().max_nesting_depth(2).build().load(yaml);
    assert!(
        matches!(
            result,
            Err(LoadError::NestingDepthLimitExceeded { limit: 2 })
        ),
        "expected NestingDepthLimitExceeded(2), got: {result:?}"
    );
}

/// LC-E6: anchor count limit returns `LoadError::AnchorCountLimitExceeded`.
#[test]
fn anchor_count_limit_exceeded_returns_error() {
    let yaml = "&a foo: &b bar: &c baz\n";
    let result = LoaderBuilder::new().max_anchors(2).build().load(yaml);
    assert!(
        matches!(
            result,
            Err(LoadError::AnchorCountLimitExceeded { limit: 2 })
        ),
        "expected AnchorCountLimitExceeded(2), got: {result:?}"
    );
}

/// LC-E8: multi-document YAML with explicit markers produces correct document count.
#[test]
fn multi_document_yaml_with_explicit_markers() {
    let yaml = "---\nfoo\n---\nbar\n";
    let docs = load(yaml).expect("load failed");
    assert_eq!(docs.len(), 2);
    assert!(
        matches!(&docs[0].root, Node::Scalar { value, .. } if value == "foo"),
        "expected scalar 'foo', got: {:?}",
        &docs[0].root
    );
    assert!(
        matches!(&docs[1].root, Node::Scalar { value, .. } if value == "bar"),
        "expected scalar 'bar', got: {:?}",
        &docs[1].root
    );
}

// ---- Unit tests for tree-parser helpers -------------------------------------

#[cfg(test)]
mod tree_parser_tests {
    use super::*;

    /// TP-1: plain scalar token parsed correctly.
    #[test]
    fn plain_scalar_token_parsed_correctly() {
        let token = parse_tree_line("=VAL :hello").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    style: StyleVariant::Plain,
                    value,
                    anchor: None,
                    tag: None,
                } if value == "hello"
            ),
            "got: {token:?}"
        );
    }

    /// TP-2: single-quoted scalar token parsed correctly.
    #[test]
    fn single_quoted_scalar_token_parsed_correctly() {
        let token = parse_tree_line("=VAL 'world").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    style: StyleVariant::SingleQuoted,
                    value,
                    anchor: None,
                    tag: None,
                } if value == "world"
            ),
            "got: {token:?}"
        );
    }

    /// TP-3: double-quoted scalar token parsed correctly.
    #[test]
    fn double_quoted_scalar_token_parsed_correctly() {
        let token = parse_tree_line("=VAL \"text").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    style: StyleVariant::DoubleQuoted,
                    value,
                    anchor: None,
                    tag: None,
                } if value == "text"
            ),
            "got: {token:?}"
        );
    }

    /// TP-4: literal block scalar token parsed correctly.
    #[test]
    fn literal_block_scalar_token_parsed_correctly() {
        let token = parse_tree_line("=VAL |content").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    style: StyleVariant::Literal,
                    value,
                    ..
                } if value == "content"
            ),
            "got: {token:?}"
        );
    }

    /// TP-5: folded block scalar token parsed correctly.
    #[test]
    fn folded_block_scalar_token_parsed_correctly() {
        let token = parse_tree_line("=VAL >content").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    style: StyleVariant::Folded,
                    value,
                    ..
                } if value == "content"
            ),
            "got: {token:?}"
        );
    }

    /// TP-6: scalar with anchor parsed correctly.
    #[test]
    fn scalar_with_anchor_parsed_correctly() {
        let token = parse_tree_line("=VAL &myanchor :value").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    anchor: Some(anchor),
                    style: StyleVariant::Plain,
                    value,
                    tag: None,
                } if anchor == "myanchor" && value == "value"
            ),
            "got: {token:?}"
        );
    }

    /// TP-7: scalar with tag parsed correctly.
    #[test]
    fn scalar_with_tag_parsed_correctly() {
        let token = parse_tree_line("=VAL <tag:str> :value").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    tag: Some(tag),
                    style: StyleVariant::Plain,
                    value,
                    anchor: None,
                } if tag == "tag:str" && value == "value"
            ),
            "got: {token:?}"
        );
    }

    /// TP-8: scalar with both anchor and tag parsed correctly.
    #[test]
    fn scalar_with_anchor_and_tag_parsed_correctly() {
        let token = parse_tree_line("=VAL &a <tag:str> :val").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    anchor: Some(anchor),
                    tag: Some(tag),
                    style: StyleVariant::Plain,
                    value,
                } if anchor == "a" && tag == "tag:str" && value == "val"
            ),
            "got: {token:?}"
        );
    }

    /// TP-9: alias token parsed correctly.
    #[test]
    fn alias_token_parsed_correctly() {
        let token = parse_tree_line("=ALI *myanchor").unwrap();
        assert!(
            matches!(&token, TreeToken::Alias { name } if name == "myanchor"),
            "got: {token:?}"
        );
    }

    /// TP-10: mapping start token parsed correctly (no anchor, no tag).
    #[test]
    fn mapping_start_token_parsed_correctly() {
        let token = parse_tree_line("+MAP").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::MapStart {
                    anchor: None,
                    tag: None,
                    style: CollectionStyle::Block,
                }
            ),
            "got: {token:?}"
        );
    }

    /// TP-11: mapping start with anchor parsed correctly.
    #[test]
    fn mapping_start_with_anchor_parsed_correctly() {
        let token = parse_tree_line("+MAP &mapanchor").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::MapStart {
                    anchor: Some(a),
                    tag: None,
                    style: CollectionStyle::Block,
                } if a == "mapanchor"
            ),
            "got: {token:?}"
        );
    }

    /// TP-12: sequence start with tag parsed correctly.
    #[test]
    fn sequence_start_with_tag_parsed_correctly() {
        let token = parse_tree_line("+SEQ <tag:seq>").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::SeqStart {
                    tag: Some(t),
                    anchor: None,
                    style: CollectionStyle::Block,
                } if t == "tag:seq"
            ),
            "got: {token:?}"
        );
    }

    /// TP-13: document start line recognized.
    #[test]
    fn document_start_with_explicit_marker_recognized() {
        let token = parse_tree_line("+DOC ---").unwrap();
        assert!(matches!(token, TreeToken::DocStart), "got: {token:?}");
    }

    /// TP-14: document start without explicit marker recognized.
    #[test]
    fn document_start_without_marker_recognized() {
        let token = parse_tree_line("+DOC").unwrap();
        assert!(matches!(token, TreeToken::DocStart), "got: {token:?}");
    }

    /// TP-15: empty scalar value (null scalar).
    #[test]
    fn empty_scalar_value_parsed_correctly() {
        let token = parse_tree_line("=VAL :").unwrap();
        assert!(
            matches!(
                &token,
                TreeToken::Scalar {
                    style: StyleVariant::Plain,
                    value,
                    anchor: None,
                    tag: None,
                } if value.is_empty()
            ),
            "got: {token:?}"
        );
    }
}
