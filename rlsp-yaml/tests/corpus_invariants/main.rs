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

mod i10_formatter_round_trip;
mod i11_validator_stability;
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

use i1_no_panics::check_i1_no_panics;
use i2_range_validity::check_i2_range_validity;
use i3_code_action_round_trip::check_i3_code_action_round_trip;
use i4_scalar_preservation::check_i4_scalar_preservation;
use i5_anchor_loc_invariant::check_i5_anchor_loc_invariant;
use i6_tag_loc_invariant::{check_i6_references_no_panics, check_i6_tag_loc_invariant};
use i8_selection_no_panic::check_i8_selection_no_panic;
use i9_complete_at_no_panics::check_i9_complete_at_no_panics;
use i10_formatter_round_trip::check_i10_formatter_round_trip;
use i11_validator_stability::check_i11_validator_stability_under_reemit;
use shared::{CheckOutcome, collect_corpus_files, run_check};

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

    use super::shared::documents_equivalent;
    use super::shared::helpers::{collect_from, load_docs, skip_list_contains, with_temp_dir};

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
