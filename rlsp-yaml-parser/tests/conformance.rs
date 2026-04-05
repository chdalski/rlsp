// SPDX-License-Identifier: MIT
//
// Conformance test suite using yaml-test-suite data symlinked at
// `tests/yaml-test-suite/src/` (commit recorded in `src/.commit`).
//
// Each `.yaml` file in that directory contains one or more test cases.
// For each case:
//   - If `fail: true` — the YAML is intentionally invalid.  We verify that
//     `parse_events` produces at least one `Err` item.
//   - Otherwise — we verify that `parse_events` produces no `Err` items
//     (the entire event stream is successfully parsed).
//
// rstest `#[files]` generates one independent test per matched file,
// giving per-file pass/fail visibility in test output.

#![allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use rlsp_yaml_parser::parse_events;
use rstest::rstest;
use saphyr::{LoadableYamlNode, YamlOwned};

// ---- Test-case data model ---------------------------------------------------

#[derive(Debug)]
struct ConformanceCase {
    file: String,
    index: usize,
    name: String,
    yaml: String,
    fail: bool,
}

// ---- Visual-representation helpers (from yaml-test-suite convention) --------

/// Convert the yaml-test-suite "visual" representation to real YAML text.
///
/// The test suite uses Unicode markers to represent whitespace characters that
/// would be invisible in plain text:
///   `␣` → space, `»` → tab, `←` → CR, `⇔` → BOM, `↵` → (nothing), `∎\n` → (nothing)
fn visual_to_raw(s: &str) -> String {
    s.replace('␣', " ")
        .replace('»', "\t")
        .replace('—', "")
        .replace('←', "\r")
        .replace('\u{21D4}', "\u{FEFF}")
        .replace('↵', "")
        .replace("∎\n", "")
}

// ---- Loading test cases from the vendored YAML files ------------------------

fn load_cases_from_file(path: &Path) -> Vec<ConformanceCase> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let Ok(docs) = YamlOwned::load_from_str(&content) else {
        return Vec::new();
    };

    let Some(YamlOwned::Sequence(seq)) = docs.first() else {
        return Vec::new();
    };

    let mut cases = Vec::new();
    let mut inherited: std::collections::HashMap<String, YamlOwned> =
        std::collections::HashMap::new();

    for (idx, item) in seq.iter().enumerate() {
        let YamlOwned::Mapping(map) = item else {
            continue;
        };

        // Fields except `fail` are inherited from the previous entry in the file.
        inherited.remove("fail");
        for (k, v) in map {
            if let YamlOwned::Value(saphyr::ScalarOwned::String(key)) = k {
                inherited.insert(key.clone(), v.clone());
            }
        }

        // Skip entries explicitly marked with `skip`.
        if inherited.contains_key("skip") {
            continue;
        }

        let yaml_raw = match inherited.get("yaml") {
            Some(YamlOwned::Value(saphyr::ScalarOwned::String(s))) => visual_to_raw(s),
            _ => continue,
        };

        let name = match inherited.get("name") {
            Some(YamlOwned::Value(saphyr::ScalarOwned::String(s))) => s.clone(),
            _ => format!("{file_name}-{idx:02}"),
        };

        let fail = match inherited.get("fail") {
            Some(YamlOwned::Value(saphyr::ScalarOwned::Boolean(b))) => *b,
            _ => false,
        };

        cases.push(ConformanceCase {
            file: file_name.clone(),
            index: idx,
            name,
            yaml: yaml_raw,
            fail,
        });
    }

    cases
}

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
fn yaml_test_suite(#[files("tests/yaml-test-suite/src/*.yaml")] path: PathBuf) {
    let cases = load_cases_from_file(&path);
    assert!(!cases.is_empty(), "no cases loaded from {path:?}");

    for case in &cases {
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
}
