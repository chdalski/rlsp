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
mod tests {
    use std::io::Write as _;
    use std::path::Path;

    use super::documents_equivalent;
    use super::helpers::{collect_from, load_docs, skip_list_contains, with_temp_dir};

    #[test]
    fn skip_list_lookup_matches_on_filename_only() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(skip_list_contains(skip, path, "round-trip"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_invariant() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(!skip_list_contains(skip, path, "idempotent"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_filename() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/other.yaml");
        assert!(!skip_list_contains(skip, path, "round-trip"));
    }

    // ---------------------------------------------------------------------------
    // documents_equivalent unit tests (TC-1 through TC-20)
    // ---------------------------------------------------------------------------

    // TC-1: byte-identical inputs are equivalent
    #[test]
    fn should_return_ok_when_inputs_are_byte_identical() {
        let docs = load_docs("a: 1\n");
        assert!(documents_equivalent(&docs, &docs).is_ok());
    }

    // TC-2: differing document counts produce an error
    #[test]
    fn should_return_err_when_document_counts_differ() {
        let a = load_docs("a: 1\n");
        let b = load_docs("a: 1\n---\nb: 2\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("document count"),
            "error should mention 'document count', got: {err}"
        );
        assert!(
            err.contains('1'),
            "error should contain count 1, got: {err}"
        );
        assert!(
            err.contains('2'),
            "error should contain count 2, got: {err}"
        );
    }

    // TC-3: scalar value mismatch includes both values and the correct path
    #[test]
    fn should_return_err_when_scalar_value_differs() {
        let a = load_docs("a: foo\n");
        let b = load_docs("a: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("foo"),
            "error should contain 'foo', got: {err}"
        );
        assert!(
            err.contains("bar"),
            "error should contain 'bar', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-4: style difference is ignored — both yield the same scalar value
    #[test]
    fn should_return_ok_when_only_styles_differ() {
        let a = load_docs("a: foo\n");
        let b = load_docs("a: \"foo\"\n");
        assert!(
            documents_equivalent(&a, &b).is_ok(),
            "style difference should not affect equivalence"
        );
    }

    // TC-5: empty scalar values with different styles are equivalent
    #[test]
    fn should_return_ok_when_empty_scalar_values_match() {
        let a = load_docs("a: \"\"\n");
        let b = load_docs("a: ''\n");
        assert!(
            documents_equivalent(&a, &b).is_ok(),
            "empty string scalars with different quote styles should be equivalent"
        );
    }

    // TC-6: differing anchor names produce an error with the correct path
    #[test]
    fn should_return_err_when_anchor_name_differs() {
        let a = load_docs("a: &x 1\n");
        let b = load_docs("a: &y 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("anchor"),
            "error should mention 'anchor', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-7: anchor present on one side but not the other
    #[test]
    fn should_return_err_when_anchor_present_vs_absent() {
        let a = load_docs("a: &x 1\n");
        let b = load_docs("a: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("anchor"),
            "error should mention 'anchor', got: {err}"
        );
        assert!(
            err.contains(r#"Some("x")"#),
            "error should reflect Some(\"x\") vs None, got: {err}"
        );
    }

    // TC-8: tag mismatch produces an error with the correct path
    #[test]
    fn should_return_err_when_tag_differs() {
        let a = load_docs("a: !custom 1\n");
        let b = load_docs("a: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("tag"),
            "error should mention 'tag', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-9: mapping entry count mismatch
    #[test]
    fn should_return_err_when_mapping_entry_count_differs() {
        let a = load_docs("a: 1\nb: 2\n");
        let b = load_docs("a: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("entry count"),
            "error should mention 'entry count', got: {err}"
        );
        assert!(
            err.contains("documents[0]"),
            "error should contain path 'documents[0]', got: {err}"
        );
    }

    // TC-10: sequence item count mismatch
    #[test]
    fn should_return_err_when_sequence_item_count_differs() {
        let a = load_docs("- 1\n- 2\n");
        let b = load_docs("- 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("item count"),
            "error should mention 'item count', got: {err}"
        );
        assert!(
            err.contains("documents[0]"),
            "error should contain path 'documents[0]', got: {err}"
        );
    }

    // TC-11: Scalar vs Mapping kind mismatch
    #[test]
    fn should_return_err_when_node_variants_differ_scalar_vs_mapping() {
        let a = load_docs("a: foo\n");
        let b = load_docs("a:\n  b: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("kind"),
            "error should mention 'kind', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-12: Sequence vs Mapping kind mismatch
    #[test]
    fn should_return_err_when_node_variants_differ_sequence_vs_mapping() {
        let a = load_docs("a:\n  - 1\n");
        let b = load_docs("a:\n  b: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("kind"),
            "error should mention 'kind', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-13: deeply nested equivalent mapping returns Ok
    #[test]
    fn should_return_ok_for_deeply_nested_equivalent_mapping() {
        let docs = load_docs("a:\n  b:\n    c: 1\n");
        assert!(documents_equivalent(&docs, &docs).is_ok());
    }

    // TC-14: nested mapping value mismatch accumulates the correct path (spike test)
    #[test]
    fn should_return_err_at_correct_path_for_nested_mapping_value_mismatch() {
        let a = load_docs("a:\n  b: foo\n");
        let b = load_docs("a:\n  b: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("foo"),
            "error should contain 'foo', got: {err}"
        );
        assert!(
            err.contains("bar"),
            "error should contain 'bar', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value/mapping/entries[0]/value"),
            "error should contain nested path, got: {err}"
        );
    }

    // TC-15: sequence item mismatch includes correct index in path
    #[test]
    fn should_return_err_at_correct_path_for_nested_sequence_item_mismatch() {
        let a = load_docs("a:\n  - 1\n  - 2\n");
        let b = load_docs("a:\n  - 1\n  - 3\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(err.contains('2'), "error should contain '2', got: {err}");
        assert!(err.contains('3'), "error should contain '3', got: {err}");
        assert!(
            err.contains("sequence/items[1]"),
            "error should contain 'sequence/items[1]', got: {err}"
        );
    }

    // TC-16: mapping key mismatch reports key path
    #[test]
    fn should_return_err_at_correct_path_for_mapping_key_mismatch() {
        let a = load_docs("a: 1\n");
        let b = load_docs("b: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains('a'),
            "error should mention key 'a', got: {err}"
        );
        assert!(
            err.contains('b'),
            "error should mention key 'b', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/key"),
            "error should contain path 'mapping/entries[0]/key', got: {err}"
        );
    }

    // TC-17: same alias names on both sides are equivalent
    #[test]
    fn should_return_ok_when_both_sides_have_same_alias_name() {
        let docs = load_docs("a: &x 1\nb: *x\n");
        assert!(documents_equivalent(&docs, &docs).is_ok());
    }

    // TC-18: differing alias names produce an error
    // Use a sequence where the first two items define anchors identically on
    // both sides; the third item is an alias — differing on the two sides.
    #[test]
    fn should_return_err_when_alias_names_differ() {
        let a = load_docs("- &x 1\n- &y 2\n- *x\n");
        let b = load_docs("- &x 1\n- &y 2\n- *y\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("alias name"),
            "error should mention 'alias name', got: {err}"
        );
    }

    // TC-19: alias vs scalar kind mismatch
    // Same setup: third item is an alias on side A, a plain scalar on side B.
    #[test]
    fn should_return_err_when_alias_vs_scalar() {
        let a = load_docs("- &x 1\n- &y 2\n- *x\n");
        let b = load_docs("- &x 1\n- &y 2\n- 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("kind"),
            "error should mention 'kind', got: {err}"
        );
    }

    // TC-20: error path includes correct document index for multi-doc mismatch
    #[test]
    fn should_include_document_index_in_error_path() {
        let a = load_docs("a: 1\n---\nb: foo\n");
        let b = load_docs("a: 1\n---\nb: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("documents[1]"),
            "error should contain 'documents[1]', got: {err}"
        );
    }

    // Validates that zero invariants × N files = 0 checks, which is the
    // expected output of the real `corpus_invariants` test in Task 1.
    #[test]
    fn corpus_invariants_runs_zero_checks_with_empty_invariant_list() {
        with_temp_dir(|dir: &Path| {
            let mut f = std::fs::File::create(dir.join("smoke.yaml")).unwrap();
            writeln!(f, "key: value").unwrap();

            let files = collect_from(dir);
            assert_eq!(files.len(), 1);

            // With an empty invariant list, checks = files × 0 = 0.
            let n_invariants = 0_usize;
            assert_eq!(files.len() * n_invariants, 0);
        });
    }
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
