// SPDX-License-Identifier: MIT
//
// Stream-API conformance: verifies that `parse_events` handles every case in
// the yaml-test-suite (351/351).
//
// rstest `#[files]` generates one independent test per matched file,
// giving per-file pass/fail visibility in test output.

use std::path::PathBuf;
use std::time::Duration;

use rlsp_yaml_parser::{Event, parse_events};
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

// ---- Balanced collections ---------------------------------------------------

/// The collection events of `yaml` up to its first error nest as a stack: every
/// end closes the start of its own kind, and an input that parses clean ends with
/// no collection open.  An unbalanced stream makes a consumer loop or over-read;
/// `[: :]` once gave two `MappingStart` and one `MappingEnd`.
fn unbalanced_collections(yaml: &str) -> Option<String> {
    let mut open: Vec<&'static str> = Vec::new();
    for event in parse_events(yaml) {
        match event {
            Ok((Event::MappingStart { .. }, _)) => open.push("mapping"),
            Ok((Event::SequenceStart { .. }, _)) => open.push("sequence"),
            Ok((Event::MappingEnd, _)) if open.pop() != Some("mapping") => {
                return Some("a MappingEnd closes no mapping".to_string());
            }
            Ok((Event::SequenceEnd, _)) if open.pop() != Some("sequence") => {
                return Some("a SequenceEnd closes no sequence".to_string());
            }
            Ok((Event::DocumentEnd { .. }, _)) if !open.is_empty() => {
                return Some(format!("a document ends with {} open", open.len()));
            }
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

#[rstest]
#[timeout(Duration::from_secs(5))]
pub fn collection_events_of_suite_case_nest(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
) {
    for case in load_cases_from_file(&path) {
        assert_eq!(
            unbalanced_collections(&case.yaml),
            None,
            "{}[{}] {}",
            case.file,
            case.index,
            case.name
        );
    }
}

/// The inputs once found to give an unbalanced stream, and variants with the
/// pair in other positions.
#[rstest]
#[case::empty_key_and_value("[: :]\n")]
#[case::spaced("[ : : ]\n")]
#[case::explicit_key("[? : :]\n")]
#[case::nested("[[: :]]\n")]
#[case::after_a_key("[a: :]\n")]
#[case::before_an_item("[: :, a]\n")]
#[case::as_a_mapping_value("k: [: :]\n")]
#[case::as_a_sequence_item("- [: :]\n")]
#[case::plain_second_pair("[a: b: c]\n")]
fn a_second_colon_in_a_flow_sequence_entry_is_an_error_with_balanced_events(#[case] yaml: &str) {
    assert!(has_parse_error(yaml), "{yaml:?}");
    assert_eq!(unbalanced_collections(yaml), None, "{yaml:?}");
    assert!(rlsp_yaml_parser::load(yaml).is_err(), "{yaml:?}");
}
