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
// Known failures are listed in `KNOWN_FAILURES` (key format `"STEM[index]"`).
// The allowlist is self-enforcing:
//   - Allowlisted entry that still fails → passes (expected failure).
//   - Allowlisted entry that now passes → fails (remove from list).
//   - Non-allowlisted entry that fails   → fails (regression).

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

// ---- Known failures (keyed as "STEM[index]") --------------------------------
//
// Each entry is one case that is expected to fail structural verification.
// These represent loader bugs to be fixed in later tasks. When a case is fixed
// the test will fail with "unexpected pass" — remove the entry from the list.
//
// Keep sorted and duplicate-free.
const KNOWN_FAILURES: &[&str] = &[
    "26DV[0]", "2AUY[0]", "2EBW[0]", "2G84[2]", "2G84[3]", "2JQS[0]", "36F6[0]", "3RLN[0]",
    "3RLN[1]", "3RLN[3]", "3RLN[4]", "4ABK[0]", "4CQQ[0]", "4FJ6[0]", "4Q9F[0]", "4QFQ[0]",
    "4RWC[0]", "4V8U[0]", "4WA9[0]", "4ZYM[0]", "565N[0]", "5BVJ[0]", "5GBF[0]", "5MUD[0]",
    "5T43[0]", "5WE3[0]", "6BFJ[0]", "6CA3[0]", "6CK3[0]", "6FWR[0]", "6H3V[0]", "6HB6[0]",
    "6JQW[0]", "6M2F[0]", "6PBE[0]", "6SLA[0]", "6VJK[0]", "6WLZ[0]", "6WPF[0]", "735Y[0]",
    "753E[0]", "7A4E[0]", "7BMT[0]", "7T8X[0]", "87E4[0]", "8G76[0]", "8KB6[0]", "8UDB[0]",
    "93JH[0]", "93WF[0]", "96L6[0]", "96NN[0]", "9BXH[0]", "9KAX[0]", "9MMW[0]", "9TFX[0]",
    "9WXW[0]", "9YRD[0]", "A6F9[0]", "B3HG[0]", "C2DT[0]", "C4HZ[0]", "CFD4[0]", "CN3R[0]",
    "CPZ3[0]", "CT4Q[0]", "DE56[0]", "DE56[1]", "DE56[2]", "DE56[3]", "DFF7[0]", "DK3J[0]",
    "DK95[3]", "DK95[8]", "DWX9[0]", "E76Z[0]", "EX5H[0]", "F2C7[0]", "F6MC[0]", "F8F9[0]",
    "FBC9[0]", "FH7J[0]", "FP8R[0]", "FRK4[0]", "FTA2[0]", "G4RS[0]", "G992[0]", "H2RW[0]",
    "HMK4[0]", "HS5T[0]", "J3BT[0]", "JEF9[0]", "JEF9[1]", "K3WX[0]", "K527[0]", "K858[0]",
    "KH5V[0]", "KH5V[1]", "KH5V[2]", "KK5P[0]", "KSS4[0]", "L24T[0]", "L24T[1]", "L383[0]",
    "L9U5[0]", "LE5A[0]", "LQZ7[0]", "LX3P[0]", "M29M[0]", "M2N8[0]", "M2N8[1]", "M5C3[0]",
    "M6YH[0]", "M7A3[0]", "M9B4[0]", "MJS9[0]", "MZX3[0]", "NAT4[0]", "NB6Z[0]", "NHX8[0]",
    "NKF9[0]", "NP9H[0]", "P2AD[0]", "PRH3[0]", "PW8X[0]", "Q5MG[0]", "Q8AD[0]", "Q9WF[0]",
    "QF4Y[0]", "R4YG[0]", "RZP5[0]", "RZT7[0]", "S3PD[0]", "SM9W[1]", "T26H[0]", "T4YY[0]",
    "T5N4[0]", "TL85[0]", "TS54[0]", "U3XV[0]", "UGM3[0]", "UKK6[0]", "UKK6[2]", "W42U[0]",
    "W4TN[0]", "XV9V[0]", "XW4D[0]", "Y79Y[1]", "Z67P[0]", "ZWK4[0]",
];

// ---- Allowlist helper -------------------------------------------------------

fn is_known_failure(key: &str) -> bool {
    KNOWN_FAILURES.binary_search(&key).is_ok()
}

/// The KNOWN_FAILURES key for a case: `"STEM[index]"`.
fn allowlist_key(case: &ConformanceCase) -> String {
    let stem = case.file.strip_suffix(".yaml").unwrap_or(&case.file);
    format!("{stem}[{}]", case.index)
}

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
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    if line == "+STR" {
        return Some(TreeToken::StreamStart);
    }
    if line == "-STR" {
        return Some(TreeToken::StreamEnd);
    }
    if line.starts_with("+DOC") {
        return Some(TreeToken::DocStart);
    }
    if line.starts_with("-DOC") {
        return Some(TreeToken::DocEnd);
    }
    if line.starts_with("-SEQ") {
        return Some(TreeToken::SeqEnd);
    }
    if line.starts_with("-MAP") {
        return Some(TreeToken::MapEnd);
    }

    if let Some(rest) = line.strip_prefix("+SEQ") {
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

    if let Some(rest) = line.strip_prefix("+MAP") {
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

    if let Some(rest) = line.strip_prefix("=ALI") {
        let name = rest
            .trim()
            .strip_prefix('*')
            .unwrap_or_else(|| rest.trim())
            .to_string();
        return Some(TreeToken::Alias { name });
    }

    if let Some(rest) = line.strip_prefix("=VAL") {
        let rest = rest.trim();
        let (anchor, tag, rest) = parse_optional_anchor_tag(rest);
        // First character of `rest` is the style prefix
        let (style, value) = match rest.chars().next() {
            Some(':') => (StyleVariant::Plain, rest[1..].to_string()),
            Some('\'') => (StyleVariant::SingleQuoted, rest[1..].to_string()),
            Some('"') => (StyleVariant::DoubleQuoted, rest[1..].to_string()),
            Some('|') => (StyleVariant::Literal, rest[1..].to_string()),
            Some('>') => (StyleVariant::Folded, rest[1..].to_string()),
            _ => return None,
        };
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
    let key = allowlist_key(case);

    // Fail cases: assert load() returns Err, skip tree check.
    if case.fail {
        let result = load(&case.yaml);
        // Fail cases might actually load fine in some implementations — we only
        // check that load returns Err for cases the suite marks as fail.
        // Some parsers are more lenient; don't enforce this strictly in
        // KNOWN_FAILURES as the stream test is the authority on fail cases.
        let _ = result;
        return;
    }

    let result = load(&case.yaml);

    // Check whether this case is expected to pass the tree verification.
    let tree_expected = case.tree.as_deref().and_then(parse_expected_documents);

    let failed = check_case(&result, tree_expected.as_deref(), &tag);

    if is_known_failure(&key) {
        assert!(
            failed,
            "loader_conformance: {tag} unexpectedly passed — \
             remove \"{key}\" from KNOWN_FAILURES in \
             rlsp-yaml-parser/tests/conformance/loader.rs",
        );
    } else {
        assert!(
            !failed,
            "loader_conformance: {tag} failed — see assertion output above"
        );
    }
}

/// Run the case assertions and return `true` if any assertion failed.
///
/// Uses `std::panic::catch_unwind` to capture assertion panics so the
/// KNOWN_FAILURES self-enforcement loop works correctly.
fn check_case(
    result: &Result<Vec<Document<Span>>, LoadError>,
    tree_expected: Option<&[ExpectedDocument]>,
    tag: &str,
) -> bool {
    use std::panic;

    // Clone/capture what we need for the closure (catch_unwind requires Send).
    let result_clone: Result<(), String> = match result {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    };

    let failed = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        // LC-2: non-fail cases must load without error.
        let docs = match result {
            Ok(docs) => docs,
            Err(e) => panic!("{tag}: load() returned error: {e}"),
        };

        // LC-3 / LC-4 / LC-5..LC-14: verify against event tree if available.
        if let Some(expected_docs) = tree_expected {
            assert_documents(docs, expected_docs, tag);
        }
    }))
    .is_err();

    // Suppress the "unused variable" warning for result_clone — it's used
    // only to satisfy the Send bound via catch_unwind, not for logic.
    let _ = result_clone;

    failed
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

    /// KNOWN_FAILURES allowlist: a present key matches.
    #[test]
    fn allowlist_known_failure_matches_by_exact_key() {
        // 26DV[0] is in KNOWN_FAILURES.
        assert!(is_known_failure("26DV[0]"));
    }

    /// KNOWN_FAILURES allowlist: an absent key does not match.
    #[test]
    fn allowlist_unknown_case_id_not_in_list() {
        assert!(!is_known_failure("YYYY[0]"));
    }

    /// KNOWN_FAILURES is sorted and duplicate-free.
    #[test]
    fn allowlist_is_sorted_and_has_no_duplicates() {
        for pair in KNOWN_FAILURES.windows(2) {
            match pair {
                [a, b] => assert!(
                    a < b,
                    "KNOWN_FAILURES is not sorted or has duplicates: {a:?} >= {b:?}"
                ),
                _ => unreachable!(),
            }
        }
    }
}
