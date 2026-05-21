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
