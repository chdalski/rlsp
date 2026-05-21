use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::validation::ValidationSettings;
use rlsp_yaml::validation::validators::{
    validate_custom_tags, validate_duplicate_keys, validate_flow_style, validate_key_ordering,
    validate_unused_anchors, validate_yaml11_compat,
};
use rlsp_yaml_parser::{Document, Node, Span};
use tower_lsp::lsp_types::{DiagnosticSeverity, Range};

use crate::Invariant;

const CORPUS_DIR: &str = "tests/corpus";

/// Skip-list entries: `(corpus_file_name, invariant_id, followup_plan_reference_and_justification)`.
///
/// Shrink-only — see module-level doc comment for the discipline.
pub const SKIP_LIST: &[(&str, &str, &str)] = &[];

pub fn collect_corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(CORPUS_DIR);
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "yml" || ext == "yaml" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

pub fn is_skipped(file_name: &str, invariant_id: &str) -> bool {
    SKIP_LIST
        .iter()
        .any(|(f, id, _)| *f == file_name && *id == invariant_id)
}

pub enum CheckOutcome {
    Passed,
    FailedExpected,
    FailedUnexpected(String),
    PassedUnexpected,
}

pub fn run_check(path: &Path, content: &str, invariant: &Invariant) -> CheckOutcome {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let skipped = is_skipped(file_name, invariant.id);
    match (invariant.check)(path, content) {
        Ok(()) => {
            if skipped {
                CheckOutcome::PassedUnexpected
            } else {
                CheckOutcome::Passed
            }
        }
        Err(msg) => {
            if skipped {
                CheckOutcome::FailedExpected
            } else {
                CheckOutcome::FailedUnexpected(msg)
            }
        }
    }
}

/// Returns `Ok(())` if `a` and `b` are structurally and data-equivalent ASTs,
/// or `Err(path_description)` identifying the first mismatch location.
///
/// Equivalence rule: same document count; for each document pair, root nodes
/// recursively equivalent. Style, spans, and `NodeMeta` comments are ignored.
pub fn documents_equivalent(a: &[Document<Span>], b: &[Document<Span>]) -> Result<(), String> {
    if a.len() != b.len() {
        return Err(format!(
            "document count differs: {} vs {}",
            a.len(),
            b.len()
        ));
    }
    for (i, (da, db)) in a.iter().zip(b.iter()).enumerate() {
        nodes_equivalent(&da.root, &db.root, &format!("documents[{i}]"))?;
    }
    Ok(())
}

pub fn nodes_equivalent(a: &Node<Span>, b: &Node<Span>, path: &str) -> Result<(), String> {
    let kind_a = node_kind_name(a);
    let kind_b = node_kind_name(b);
    if kind_a != kind_b {
        return Err(format!("{path}: kind differs: {kind_a} vs {kind_b}"));
    }

    if let (Node::Alias { name: na, .. }, Node::Alias { name: nb, .. }) = (a, b) {
        if na != nb {
            return Err(format!("{path}: alias name differs: {na:?} vs {nb:?}"));
        }
    } else {
        let anchor_a = a.anchor();
        let anchor_b = b.anchor();
        if anchor_a != anchor_b {
            return Err(format!(
                "{path}: anchor differs: {anchor_a:?} vs {anchor_b:?}"
            ));
        }
        let tag_a = node_tag_str(a);
        let tag_b = node_tag_str(b);
        if tag_a != tag_b {
            return Err(format!("{path}: tag differs: {tag_a:?} vs {tag_b:?}"));
        }
        match (a, b) {
            (Node::Scalar { value: va, .. }, Node::Scalar { value: vb, .. }) => {
                if va != vb {
                    return Err(format!("{path}: scalar value differs: '{va}' vs '{vb}'"));
                }
            }
            (Node::Mapping { entries: ea, .. }, Node::Mapping { entries: eb, .. }) => {
                if ea.len() != eb.len() {
                    return Err(format!(
                        "{path}: entry count differs: {} vs {}",
                        ea.len(),
                        eb.len()
                    ));
                }
                for (i, ((ka, va), (kb, vb))) in ea.iter().zip(eb.iter()).enumerate() {
                    nodes_equivalent(ka, kb, &format!("{path}/mapping/entries[{i}]/key"))?;
                    nodes_equivalent(va, vb, &format!("{path}/mapping/entries[{i}]/value"))?;
                }
            }
            (Node::Sequence { items: ia, .. }, Node::Sequence { items: ib, .. }) => {
                if ia.len() != ib.len() {
                    return Err(format!(
                        "{path}: item count differs: {} vs {}",
                        ia.len(),
                        ib.len()
                    ));
                }
                for (i, (na, nb)) in ia.iter().zip(ib.iter()).enumerate() {
                    nodes_equivalent(na, nb, &format!("{path}/sequence/items[{i}]"))?;
                }
            }
            _ => unreachable!("variant mismatch already handled above"),
        }
    }
    Ok(())
}

pub const fn node_kind_name(node: &Node<Span>) -> &'static str {
    match node {
        Node::Scalar { .. } => "Scalar",
        Node::Mapping { .. } => "Mapping",
        Node::Sequence { .. } => "Sequence",
        Node::Alias { .. } => "Alias",
    }
}

pub fn node_tag_str(node: &Node<Span>) -> Option<&str> {
    match node {
        Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
            tag.as_deref()
        }
        Node::Alias { .. } => None,
    }
}

/// Collect diagnostics from all validators for a given parsed documents set.
pub fn collect_all_diagnostics(
    docs: &[rlsp_yaml_parser::node::Document<rlsp_yaml_parser::Span>],
) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    let mut all = Vec::new();
    all.extend(validate_unused_anchors(docs));
    all.extend(validate_flow_style(docs, &ValidationSettings::default()));
    all.extend(validate_custom_tags(docs, &[]));
    all.extend(validate_key_ordering(docs));
    all.extend(validate_duplicate_keys(
        docs,
        &ValidationSettings::default(),
    ));
    all.extend(validate_yaml11_compat(docs));
    all
}

/// Collect all Error-severity diagnostics from parse + validators.
pub fn collect_error_diagnostics(text: &str) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    let parse_result = parse_yaml(text);
    let docs = parse_result.documents;
    collect_all_diagnostics(&docs)
        .into_iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect()
}

/// Build a `HashSet` of `"code|message|range_str"` keys for fast membership testing.
pub fn error_key_set(errors: &[tower_lsp::lsp_types::Diagnostic]) -> HashSet<String> {
    errors.iter().map(error_key).collect()
}

pub fn error_key(d: &tower_lsp::lsp_types::Diagnostic) -> String {
    let code = d
        .code
        .as_ref()
        .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
    format!("{}|{}|{}", code, d.message, fmt_range(d.range))
}

pub fn fmt_range(r: Range) -> String {
    format!(
        "L{}:{}-L{}:{}",
        r.start.line, r.start.character, r.end.line, r.end.character
    )
}

/// Extract a human-readable message from a panic payload.
pub fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload.downcast_ref::<&str>().map_or_else(
        || {
            payload
                .downcast_ref::<String>()
                .map_or_else(|| "<non-string panic>".to_string(), Clone::clone)
        },
        |s| (*s).to_string(),
    )
}

#[cfg(test)]
pub mod helpers {
    use std::path::{Path, PathBuf};

    use rlsp_yaml_parser::{CollectionStyle, Document, Node, ScalarStyle, Span as TestSpan};
    use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

    pub fn with_temp_dir<F: FnOnce(&Path)>(f: F) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!("corpus_test_{unique}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        f(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    pub fn make_diag(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Diagnostic {
        Diagnostic {
            range: Range::new(
                Position::new(start_line, start_char),
                Position::new(end_line, end_char),
            ),
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("test".to_string())),
            ..Default::default()
        }
    }

    pub fn collect_from(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return files;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yml" || ext == "yaml" {
                        files.push(path);
                    }
                }
            }
        }
        files.sort();
        files
    }

    pub fn skip_list_contains(
        skip: &[(&str, &str, &str)],
        path: &Path,
        invariant_id: &str,
    ) -> bool {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        skip.iter()
            .any(|(f, id, _)| *f == file_name && *id == invariant_id)
    }

    pub fn load_docs(text: &str) -> Vec<Document<TestSpan>> {
        rlsp_yaml_parser::loader::load(text).expect("valid YAML for test")
    }

    pub fn zero_span() -> TestSpan {
        TestSpan { start: 0, end: 0 }
    }

    pub fn make_scalar(value: &str) -> Node<TestSpan> {
        Node::Scalar {
            value: value.to_owned(),
            style: ScalarStyle::Plain,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    pub fn make_mapping(entries: Vec<(Node<TestSpan>, Node<TestSpan>)>) -> Node<TestSpan> {
        Node::Mapping {
            entries,
            style: CollectionStyle::Block,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    pub fn make_sequence(items: Vec<Node<TestSpan>>) -> Node<TestSpan> {
        Node::Sequence {
            items,
            style: CollectionStyle::Block,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    pub fn make_doc(root: Node<TestSpan>) -> Document<TestSpan> {
        Document::with_root(root)
    }
}
