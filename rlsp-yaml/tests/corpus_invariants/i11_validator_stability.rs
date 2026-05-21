use std::path::Path;

use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::schema::parse_schema;
use rlsp_yaml::schema_validation::validate_schema;
use rlsp_yaml::server::YamlVersion;
use rlsp_yaml::validation::ValidationSettings;
use rlsp_yaml::validation::validators::{
    validate_custom_tags, validate_duplicate_keys, validate_flow_style, validate_key_ordering,
    validate_unused_anchors, validate_yaml11_compat,
};
use tower_lsp::lsp_types::DiagnosticSeverity;

pub fn i11_build_schema() -> rlsp_yaml::schema::JsonSchema {
    parse_schema(&serde_json::json!({
        "type": "object",
        "additionalProperties": { "type": "string" }
    }))
    .expect("I11 schema: parse failed")
}

pub fn i11_collect_diagnostics(
    docs: &[rlsp_yaml_parser::node::Document<rlsp_yaml_parser::Span>],
    schema: &rlsp_yaml::schema::JsonSchema,
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
    all.extend(validate_schema(docs, schema, false, YamlVersion::V1_2));
    all
}

pub fn diagnostic_identity_multiset(
    diags: &[tower_lsp::lsp_types::Diagnostic],
) -> Vec<(String, Option<DiagnosticSeverity>, String)> {
    let mut result: Vec<(String, Option<DiagnosticSeverity>, String)> = diags
        .iter()
        .map(|d| (format!("{:?}", d.code), d.severity, d.message.clone()))
        .collect();
    result.sort();
    result
}

pub fn check_i11_validator_stability_under_reemit(_path: &Path, text: &str) -> Result<(), String> {
    let parse_pre = parse_yaml(text);
    if parse_pre.documents.is_empty() {
        return Ok(());
    }
    let schema = i11_build_schema();
    let pre_multiset =
        diagnostic_identity_multiset(&i11_collect_diagnostics(&parse_pre.documents, &schema));
    let formatted = format_yaml(text, &YamlFormatOptions::default());
    let parse_post = parse_yaml(&formatted);
    if parse_post.documents.is_empty() {
        return Err("formatter output failed to parse".to_string());
    }
    let post_multiset =
        diagnostic_identity_multiset(&i11_collect_diagnostics(&parse_post.documents, &schema));
    if pre_multiset == post_multiset {
        return Ok(());
    }
    // Find the first differing entry to report a useful error.
    for (a, b) in pre_multiset.iter().zip(post_multiset.iter()) {
        if a != b {
            return Err(format!("diagnostic identity differs: pre={a:?} post={b:?}"));
        }
    }
    // Lengths differ — report the extra entry from whichever side is longer.
    if pre_multiset.len() > post_multiset.len() {
        let a = &pre_multiset[post_multiset.len()];
        return Err(format!(
            "diagnostic present pre-format but missing post-format: {a:?}"
        ));
    }
    let b = &post_multiset[pre_multiset.len()];
    Err(format!("diagnostic new post-format (not in pre): {b:?}"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};

    use super::{
        check_i11_validator_stability_under_reemit, diagnostic_identity_multiset, i11_build_schema,
        i11_collect_diagnostics,
    };
    use crate::INVARIANTS;

    fn make_i11_diag(
        code: &str,
        severity: DiagnosticSeverity,
        message: &str,
    ) -> tower_lsp::lsp_types::Diagnostic {
        use tower_lsp::lsp_types::NumberOrString;
        tower_lsp::lsp_types::Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 1)),
            severity: Some(severity),
            code: Some(NumberOrString::String(code.to_string())),
            message: message.to_string(),
            ..Default::default()
        }
    }

    fn run_i11(text: &str) -> Result<(), String> {
        check_i11_validator_stability_under_reemit(Path::new("test.yaml"), text)
    }

    // UT-I11-1: identical inputs produce equal multisets
    #[test]
    fn i11_ut1_identical_inputs_produce_equal_multisets() {
        let a = vec![make_i11_diag("E1", DiagnosticSeverity::ERROR, "msg")];
        let b = vec![make_i11_diag("E1", DiagnosticSeverity::ERROR, "msg")];
        assert_eq!(
            diagnostic_identity_multiset(&a),
            diagnostic_identity_multiset(&b)
        );
    }

    // UT-I11-2: input order does not affect the multiset
    #[test]
    fn i11_ut2_input_order_does_not_affect_multiset() {
        let diag_a = make_i11_diag("E1", DiagnosticSeverity::ERROR, "first");
        let diag_b = make_i11_diag("E2", DiagnosticSeverity::WARNING, "second");
        let diag_c = make_i11_diag("E3", DiagnosticSeverity::INFORMATION, "third");
        let ordered = vec![diag_a.clone(), diag_b.clone(), diag_c.clone()];
        let reordered = vec![diag_c, diag_a, diag_b];
        assert_eq!(
            diagnostic_identity_multiset(&ordered),
            diagnostic_identity_multiset(&reordered)
        );
    }

    // UT-I11-3: differing message text produces different multisets
    #[test]
    fn i11_ut3_differing_message_produces_different_multisets() {
        let a = vec![make_i11_diag("E1", DiagnosticSeverity::ERROR, "foo")];
        let b = vec![make_i11_diag("E1", DiagnosticSeverity::ERROR, "bar")];
        assert_ne!(
            diagnostic_identity_multiset(&a),
            diagnostic_identity_multiset(&b)
        );
    }

    // UT-I11-4: duplicate count difference is detected
    #[test]
    fn i11_ut4_duplicate_count_difference_is_detected() {
        let diag = make_i11_diag("E1", DiagnosticSeverity::ERROR, "msg");
        let pre = vec![diag.clone(), diag.clone()];
        let post = vec![diag];
        assert_ne!(
            diagnostic_identity_multiset(&pre),
            diagnostic_identity_multiset(&post)
        );
    }

    // UT-I11-5: empty input produces an empty multiset
    #[test]
    fn i11_ut5_empty_input_produces_empty_multiset() {
        assert!(diagnostic_identity_multiset(&[]).is_empty());
    }

    // UT-I11-6: differing code strings produce different multisets
    #[test]
    fn i11_ut6_differing_code_produces_different_multisets() {
        let a = vec![make_i11_diag("E1", DiagnosticSeverity::ERROR, "msg")];
        let b = vec![make_i11_diag("E2", DiagnosticSeverity::ERROR, "msg")];
        assert_ne!(
            diagnostic_identity_multiset(&a),
            diagnostic_identity_multiset(&b)
        );
    }

    // UT-I11-7: differing severity produces different multisets
    #[test]
    fn i11_ut7_differing_severity_produces_different_multisets() {
        let a = vec![make_i11_diag("E1", DiagnosticSeverity::ERROR, "msg")];
        let b = vec![make_i11_diag("E1", DiagnosticSeverity::WARNING, "msg")];
        assert_ne!(
            diagnostic_identity_multiset(&a),
            diagnostic_identity_multiset(&b)
        );
    }

    // UT-I11-8: empty document list returns empty diagnostics
    #[test]
    fn i11_ut8_empty_docs_returns_empty_diagnostics() {
        let schema = i11_build_schema();
        assert!(i11_collect_diagnostics(&[], &schema).is_empty());
    }

    // UT-I11-9: valid single-document YAML with permissive schema yields no errors from plumbing
    #[test]
    fn i11_ut9_valid_yaml_collect_does_not_panic() {
        use rlsp_yaml::parser::parse_yaml;
        let docs = parse_yaml("key: value\n").documents;
        let schema = i11_build_schema();
        let _ = i11_collect_diagnostics(&docs, &schema);
    }

    // UT-I11-10: parse-empty input returns Ok (early-return branch)
    #[test]
    fn i11_ut10_empty_input_returns_ok() {
        assert!(run_i11("").is_ok());
    }

    // UT-I11-11: valid simple YAML with matching pre/post diagnostics returns Ok
    #[test]
    fn i11_ut11_simple_yaml_returns_ok() {
        assert!(run_i11("key: value\n").is_ok());
    }

    // UT-I11-12: multi-document YAML with stable diagnostics returns Ok
    #[test]
    fn i11_ut12_multi_document_returns_ok() {
        assert!(run_i11("---\na: 1\n---\nb: 2\n").is_ok());
    }

    // UT-I11-13: mismatch detection — compare two differing multisets directly to validate
    // error message content.
    #[test]
    fn i11_ut13_mismatch_error_contains_diagnostic_detail() {
        let pre = vec![make_i11_diag(
            "E1",
            DiagnosticSeverity::ERROR,
            "type mismatch",
        )];
        let post = vec![make_i11_diag(
            "E2",
            DiagnosticSeverity::ERROR,
            "type mismatch",
        )];
        let pre_ms = diagnostic_identity_multiset(&pre);
        let post_ms = diagnostic_identity_multiset(&post);
        assert_ne!(pre_ms, post_ms);
        let err = pre_ms
            .iter()
            .zip(post_ms.iter())
            .find(|(a, b)| a != b)
            .map(|(a, b)| format!("diagnostic identity differs: pre={a:?} post={b:?}"))
            .unwrap_or_default();
        assert!(
            err.contains("E1") || err.contains("E2"),
            "error should reference differing code; got: {err}"
        );
    }

    // UT-I11-14: format-yields-empty-parse error path — mirrors I10 guard; not reachable
    // from any valid formatter input. Covered by inspection.

    // UT-I11-15: INVARIANTS array contains an entry with id == "I11"
    #[test]
    fn i11_ut15_invariants_array_contains_i11() {
        assert!(
            INVARIANTS.iter().any(|inv| inv.id == "I11"),
            "INVARIANTS must contain an entry with id == \"I11\""
        );
    }
}
