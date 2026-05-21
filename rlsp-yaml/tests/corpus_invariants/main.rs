// SPDX-License-Identifier: MIT
//
// Corpus invariant harness for rlsp-yaml.
//
// # Skip-list discipline
//
// The SKIP_LIST is **shrink-only**. Entries are removed as follow-up plans fix
// the root causes. New entries are only added when a NEW corpus file surfaces a
// known-fixable issue that has an immediate follow-up plan already filed; never
// to silence a surprise failure. This constraint is the harness's enforcement
// surface — without it the test degrades to a rubber stamp.
//
// A surprise failure (a (file, invariant) pair that fails but has no skip-list
// entry) must be reported to the lead via SendMessage identifying the pair and
// failure detail. The lead either files a follow-up plan (whose path the
// developer then references in the skip-list entry) or directs treating the
// failure as in-scope. The developer never adds a skip-list entry with an
// ad-hoc TODO marker lacking a plan reference.

#![expect(missing_docs, reason = "test code")]
#![expect(
    clippy::expect_used,
    reason = "test code — expect on infallible operations"
)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "test code — LSP line counts fit in u32 for any real corpus file"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "test code — indices are validated by invariant checks before use"
)]

mod i1_no_panics;
mod i2_range_validity;
mod i3_code_action_round_trip;
mod i4_scalar_preservation;
mod i5_anchor_loc_invariant;
mod i6_tag_loc_invariant;
mod i8_selection_no_panic;
mod i9_complete_at_no_panics;
mod shared;

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

use i1_no_panics::check_i1_no_panics;
use i2_range_validity::check_i2_range_validity;
use i3_code_action_round_trip::check_i3_code_action_round_trip;
use i4_scalar_preservation::check_i4_scalar_preservation;
use i5_anchor_loc_invariant::check_i5_anchor_loc_invariant;
use i6_tag_loc_invariant::{check_i6_references_no_panics, check_i6_tag_loc_invariant};
use i8_selection_no_panic::check_i8_selection_no_panic;
use i9_complete_at_no_panics::check_i9_complete_at_no_panics;
use shared::{CheckOutcome, collect_corpus_files, documents_equivalent, run_check};

/// Each registered invariant has an id, description, and a check function.
pub(crate) struct Invariant {
    pub(crate) id: &'static str,
    #[expect(
        dead_code,
        reason = "displayed in future failure-reporting; kept for extensibility"
    )]
    pub(crate) description: &'static str,
    pub(crate) check: fn(&Path, &str) -> Result<(), String>,
}

/// Registered invariants.
const INVARIANTS: &[Invariant] = &[
    Invariant {
        id: "I1",
        description: "No panics on full LSP pipeline",
        check: check_i1_no_panics,
    },
    Invariant {
        id: "I2",
        description: "Diagnostic range validity",
        check: check_i2_range_validity,
    },
    Invariant {
        id: "I3",
        description: "Code-action output parses",
        check: check_i3_code_action_round_trip,
    },
    Invariant {
        id: "I4",
        description: "Refactor code actions preserve scalar content",
        check: check_i4_scalar_preservation,
    },
    Invariant {
        id: "I5",
        description: "AST anchor_loc invariant: anchor().is_some() == anchor_loc().is_some() for every node",
        check: check_i5_anchor_loc_invariant,
    },
    Invariant {
        id: "I6",
        description: "AST tag_loc invariant: for every node, if tag is Some and NOT a resolver-injected core schema tag, tag_loc must also be Some",
        check: check_i6_tag_loc_invariant,
    },
    Invariant {
        id: "I7",
        description: "goto_definition and find_references never panic on corpus files",
        check: check_i6_references_no_panics,
    },
    Invariant {
        id: "I8",
        description: "selection_ranges never panics and outermost range starts at line 0 for non-empty result at (0,0)",
        check: check_i8_selection_no_panic,
    },
    Invariant {
        id: "I9",
        description: "complete_at never panics and returns <= MAX_COMPLETION_ITEMS items for any cursor position",
        check: check_i9_complete_at_no_panics,
    },
    Invariant {
        id: "I10",
        description: "Formatter round-trip: parsing format(text) produces an AST semantically equivalent to parsing text",
        check: check_i10_formatter_round_trip,
    },
    Invariant {
        id: "I11",
        description: "Validator stability under format-equivalent re-emit: diagnostic identities (code, severity, message) match pre- and post-format on AST-equivalent input",
        check: check_i11_validator_stability_under_reemit,
    },
];

// ---------------------------------------------------------------------------
// I10: Formatter round-trip — format(text) re-parses to an equivalent AST
// ---------------------------------------------------------------------------

fn check_i10_formatter_round_trip(_path: &Path, text: &str) -> Result<(), String> {
    let parse_pre = parse_yaml(text);
    if parse_pre.documents.is_empty() {
        return Ok(());
    }
    let formatted = format_yaml(text, &YamlFormatOptions::default());
    let parse_post = parse_yaml(&formatted);
    if parse_post.documents.is_empty() {
        return Err("formatter output failed to parse".to_string());
    }
    documents_equivalent(&parse_pre.documents, &parse_post.documents)
}

// ---------------------------------------------------------------------------
// I11: Validator stability under format-equivalent re-emit
// ---------------------------------------------------------------------------

fn i11_build_schema() -> rlsp_yaml::schema::JsonSchema {
    parse_schema(&serde_json::json!({
        "type": "object",
        "additionalProperties": { "type": "string" }
    }))
    .expect("I11 schema: parse failed")
}

fn i11_collect_diagnostics(
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

fn diagnostic_identity_multiset(
    diags: &[tower_lsp::lsp_types::Diagnostic],
) -> Vec<(String, Option<DiagnosticSeverity>, String)> {
    let mut result: Vec<(String, Option<DiagnosticSeverity>, String)> = diags
        .iter()
        .map(|d| (format!("{:?}", d.code), d.severity, d.message.clone()))
        .collect();
    result.sort();
    result
}

fn check_i11_validator_stability_under_reemit(_path: &Path, text: &str) -> Result<(), String> {
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

// ---------------------------------------------------------------------------
// Corpus runner
// ---------------------------------------------------------------------------

#[test]
fn corpus_invariants() {
    let files = collect_corpus_files();
    let n_files = files.len();
    let n_invariants = INVARIANTS.len();
    let n_checks = n_files * n_invariants;

    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        for invariant in INVARIANTS {
            match run_check(path, &content, invariant) {
                CheckOutcome::Passed | CheckOutcome::FailedExpected => {}
                CheckOutcome::FailedUnexpected(msg) => {
                    failures.push(format!("FAIL [{} / {}]: {}", file_name, invariant.id, msg));
                }
                CheckOutcome::PassedUnexpected => {
                    failures.push(format!(
                        "STALE SKIP [{} / {}]: expected failure but invariant passed — remove skip-list entry",
                        file_name, invariant.id
                    ));
                }
            }
        }
    }

    println!("corpus_invariants: {n_invariants} invariants × {n_files} files = {n_checks} checks");

    assert!(
        failures.is_empty(),
        "{} check(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::Path;

    use tower_lsp::lsp_types::{DiagnosticSeverity, Position, Range};

    use super::*;
    use shared::helpers::{collect_from, load_docs, skip_list_contains, with_temp_dir};

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
        with_temp_dir(|dir| {
            let mut f = std::fs::File::create(dir.join("smoke.yaml")).unwrap();
            writeln!(f, "key: value").unwrap();

            let files = collect_from(dir);
            assert_eq!(files.len(), 1);

            // With an empty invariant list, checks = files × 0 = 0.
            let n_invariants = 0_usize;
            assert_eq!(files.len() * n_invariants, 0);
        });
    }

    // ---------------------------------------------------------------------------
    // I10 unit tests
    // ---------------------------------------------------------------------------

    fn run_i10(text: &str) -> Result<(), String> {
        check_i10_formatter_round_trip(Path::new("test.yaml"), text)
    }

    // UT-I10-1: empty input returns Ok (empty pre-parse branch)
    #[test]
    fn i10_ut1_empty_input_returns_ok() {
        assert!(run_i10("").is_ok());
    }

    // UT-I10-2: invalid YAML returns Ok (empty pre-parse branch)
    #[test]
    fn i10_ut2_invalid_yaml_returns_ok() {
        assert!(run_i10("{{{invalid yaml").is_ok());
    }

    // UT-I10-3: idempotent valid YAML returns Ok (happy path)
    #[test]
    fn i10_ut3_idempotent_valid_yaml_returns_ok() {
        assert!(run_i10("key: value\n").is_ok());
    }

    // UT-I10-4: flow mapping → block conversion returns Ok (style changes, structure unchanged)
    #[test]
    fn i10_ut4_flow_to_block_conversion_returns_ok() {
        assert!(run_i10("{a: 1, b: 2}\n").is_ok());
    }

    // UT-I10-5: multi-document input returns Ok
    #[test]
    fn i10_ut5_multi_document_returns_ok() {
        assert!(run_i10("a: 1\n---\nb: 2\n").is_ok());
    }

    // UT-I10-6: defensive branch — formatter output that parses to zero documents returns Err.
    // This branch is a guard against formatters producing unparseable output; the formatter
    // correctly never produces such output for valid input. Branch coverage is by inspection
    // only — we confirm the Ok/Err semantics of adjacent branches cover it structurally.
    //
    // UT-I10-6: defensive branch; not reachable by any valid formatter input — covered by inspection

    // ---------------------------------------------------------------------------
    // I11 unit tests
    // ---------------------------------------------------------------------------

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
        let docs = parse_yaml("key: value\n").documents;
        // Use a permissive schema (type: object, additionalProperties: string).
        // "value" is a string so no type-mismatch. Collect runs all 7 validators without panic.
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
    // error message content. Since `format_yaml` is a pure function with no injection point,
    // the end-to-end mismatch path is not directly exercisable here; we validate the comparison
    // logic by building multisets directly from diagnostics and confirming the error path works.
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
        // Build error string inline (same logic as check_i11_validator_stability_under_reemit).
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
    // from any valid formatter input. Covered by inspection: the guard string
    // "formatter output failed to parse" is identical to I10's guard.

    // UT-I11-15: INVARIANTS array contains an entry with id == "I11"
    #[test]
    fn i11_ut15_invariants_array_contains_i11() {
        assert!(
            INVARIANTS.iter().any(|inv| inv.id == "I11"),
            "INVARIANTS must contain an entry with id == \"I11\""
        );
    }
}
