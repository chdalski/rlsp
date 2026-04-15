// SPDX-License-Identifier: MIT
//
// Stream-API conformance: verifies that `parse_events` handles every case in
// the yaml-test-suite (351/351).
//
// rstest `#[files]` generates one independent test per matched file,
// giving per-file pass/fail visibility in test output.

use std::path::PathBuf;
use std::time::Duration;

use rlsp_yaml_parser::parse_events;
use rstest::rstest;

use super::{ConformanceCase, load_cases_from_file};

// ---- Helpers ----------------------------------------------------------------

/// Returns `true` if `parse_events` produces at least one `Err` for `input`.
fn has_parse_error(input: &str) -> bool {
    parse_events(input).any(|r| r.is_err())
}

/// Returns `true` if `parse_events` produces no `Err` for `input`.
fn parses_clean(input: &str) -> bool {
    parse_events(input).all(|r| r.is_ok())
}

// ---- Parameterized conformance test -----------------------------------------

#[rstest]
#[timeout(Duration::from_secs(5))]
pub fn yaml_test_suite(#[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf) {
    let cases = load_cases_from_file(&path);
    if cases.is_empty() {
        // All entries are skipped (e.g., ZYU8). Nothing to test.
        return;
    }

    for case in &cases {
        assert_case(case);
    }
}

fn assert_case(case: &ConformanceCase) {
    let tag = format!("{}[{}] {}", case.file, case.index, case.name);

    if case.fail {
        assert!(
            has_parse_error(&case.yaml),
            "expected parse error but got clean parse: {tag}\n  yaml: {:?}",
            &case.yaml[..case.yaml.len().min(120)]
        );
    } else {
        let first_err = parse_events(&case.yaml)
            .find_map(std::result::Result::err)
            .map(|e| e.to_string());
        assert!(
            parses_clean(&case.yaml),
            "unexpected parse error: {tag}\n  error: {}\n  yaml: {:?}",
            first_err.unwrap_or_default(),
            &case.yaml[..case.yaml.len().min(120)]
        );
    }
}
