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
// At the end the test prints a summary:
//   N passed, M skipped, K failed
//
// Failures are printed but the test DOES panic with a non-zero count so that
// CI fails on regressions.

#![allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::path::Path;

use rlsp_yaml_parser::parse_events;
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

// ---- Main test function -----------------------------------------------------

#[test]
fn yaml_test_suite_conformance() {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/yaml-test-suite/src");

    if !data_dir.exists() {
        eprintln!("[conformance] yaml-test-suite data not found at {data_dir:?}, skipping test.");
        return;
    }

    let mut entries: Vec<_> = std::fs::read_dir(&data_dir)
        .expect("read yaml-test-suite/src dir")
        .filter_map(std::result::Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    let mut all_cases: Vec<ConformanceCase> = Vec::new();
    for entry in &entries {
        all_cases.extend(load_cases_from_file(&entry.path()));
    }

    let total = all_cases.len();
    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for case in &all_cases {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);

        if case.fail {
            // Error case: our parser must produce at least one error.
            if has_parse_error(&case.yaml) {
                passed += 1;
            } else {
                failures.push(format!(
                    "FAIL (expected parse error, got clean parse) {tag}\n  yaml: {:?}",
                    &case.yaml[..case.yaml.len().min(120)]
                ));
            }
        } else {
            // Valid case: our parser must produce a clean event stream.
            if parses_clean(&case.yaml) {
                passed += 1;
            } else {
                // Collect the first error for the failure message.
                let first_err = parse_events(&case.yaml)
                    .find_map(std::result::Result::err)
                    .map(|e| e.to_string())
                    .unwrap_or_default();
                failures.push(format!(
                    "FAIL (unexpected parse error) {tag}\n  error: {first_err}\n  yaml: {:?}",
                    &case.yaml[..case.yaml.len().min(120)]
                ));
            }
        }
    }

    let failed = failures.len();
    let skipped = total - passed - failed;

    eprintln!(
        "\n[conformance] {total} cases: {passed} passed, {skipped} skipped, {failed} failed\n"
    );

    if !failures.is_empty() {
        for f in &failures {
            eprintln!("{f}");
        }
        eprintln!();
    }

    assert_eq!(
        failed, 0,
        "{failed} conformance failure(s) — see stderr for details"
    );
}
