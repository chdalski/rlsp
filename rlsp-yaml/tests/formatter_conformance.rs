// SPDX-License-Identifier: MIT
//
// Formatter round-trip conformance test using yaml-test-suite data at
// `tests/yaml-test-suite/src/` (workspace root, relative path `../tests/`).
//
// For each non-fail case in the suite:
//   1. `format_yaml(input, &default_opts)` must produce output that parses
//      cleanly (no diagnostics).
//   2. Formatting must be idempotent: `format(format(input)) == format(input)`.
//
// Known failures are listed in `KNOWN_FAILURES` (keyed by `"STEM[index]"`, e.g.
// `"G4RS[0]"`). The allowlist enforces its own shrinkage:
//   - Allowlisted entry that fails  → test passes (expected failure).
//   - Allowlisted entry that passes → test FAILS (remove it from the list).
//   - Non-allowlisted entry that fails → test FAILS (regression).

#![expect(clippy::unwrap_used, missing_docs, reason = "test code")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;
use rstest::rstest;

// ---- Known failures (keyed as "STEM[index]") --------------------------------
//
// Each entry represents one case in a yaml-test-suite `.yaml` file that is
// expected to fail the round-trip assertions. When a case is fixed (assertions
// start passing), the test will fail with an "unexpected pass" error — remove
// the entry from the list.
//
// Keep sorted and duplicate-free.
const KNOWN_FAILURES: &[&str] = &[
    "26DV[0]", "2G84[2]", "2G84[3]", "2XXW[0]", "35KP[0]", "6CA3[0]", "6M2F[0]", "6PBE[0]",
    "6WLZ[0]", "6XDY[0]", "8KB6[0]", "98YD[0]", "9BXH[0]", "C4HZ[0]", "DK95[7]", "E76Z[0]",
    "FH7J[0]", "FTA2[0]", "HWV9[0]", "J7PZ[0]", "JEF9[1]", "JEF9[2]", "JHB9[0]", "KK5P[0]",
    "L383[0]", "M2N8[0]", "MUS6[0]", "MUS6[2]", "MUS6[3]", "MUS6[4]", "MUS6[5]", "MUS6[6]",
    "NB6Z[0]", "NKF9[0]", "PW8X[0]", "Q5MG[0]", "QT73[0]", "RZP5[0]", "S3PD[0]", "T26H[0]",
    "UGM3[0]", "UKK6[2]", "W4TN[0]", "WZ62[0]", "XW4D[0]", "Y79Y[1]",
];

// ---- Allowlist helper -------------------------------------------------------

fn is_known_failure(key: &str) -> bool {
    KNOWN_FAILURES.binary_search(&key).is_ok()
}

// ---- Visual-representation helpers (from yaml-test-suite convention) --------

/// Convert the yaml-test-suite "visual" representation to real YAML text.
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

#[derive(Debug)]
struct ConformanceCase {
    file: String,
    stem: String,
    index: usize,
    name: String,
    yaml: String,
    fail: bool,
}

impl ConformanceCase {
    /// The allowlist key for this case: `"STEM[index]"`.
    fn allowlist_key(&self) -> String {
        format!("{}[{}]", self.stem, self.index)
    }
}

#[derive(Default)]
struct EntryFields {
    name: Option<String>,
    yaml: Option<String>,
    fail: Option<bool>,
    skip: bool,
}

impl EntryFields {
    fn set(&mut self, key: &str, value: &str) {
        match key {
            "name" => self.name = Some(value.to_string()),
            "yaml" => self.yaml = Some(value.to_string()),
            "fail" => self.fail = Some(value == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }

    fn set_block(&mut self, key: &str, block: String) {
        match key {
            "name" => self.name = Some(block),
            "yaml" => self.yaml = Some(block),
            "fail" => self.fail = Some(block.trim() == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }
}

fn parse_test_metadata(content: &str) -> Vec<(String, String, bool)> {
    let mut results: Vec<(String, String, bool)> = Vec::new();
    let mut inherited = EntryFields::default();
    let mut current = EntryFields::default();
    let mut block_key: Option<String> = None;
    let mut block_buf: Option<String> = None;
    let mut in_entry = false;

    let flush_block = |current: &mut EntryFields,
                       block_key: &mut Option<String>,
                       block_buf: &mut Option<String>| {
        if let (Some(k), Some(b)) = (block_key.take(), block_buf.take()) {
            current.set_block(&k, b);
        }
    };

    let flush_entry =
        |current: &mut EntryFields, inherited: &mut EntryFields, results: &mut Vec<_>| {
            if let Some(n) = current.name.take() {
                inherited.name = Some(n);
            }
            if let Some(y) = current.yaml.take() {
                inherited.yaml = Some(y);
            }
            if current.skip {
                inherited.skip = true;
            }
            let fail = current.fail.take().unwrap_or(false);
            if !inherited.skip {
                if let Some(ref yaml) = inherited.yaml {
                    let name = inherited.name.clone().unwrap_or_default();
                    results.push((name, yaml.clone(), fail));
                }
            }
        };

    let parse_field = |line: &str,
                       current: &mut EntryFields,
                       block_key: &mut Option<String>,
                       block_buf: &mut Option<String>| {
        if let Some((key, value)) = line.split_once(": ") {
            let key = key.trim();
            let value = value.trim();
            if value == "|" {
                *block_key = Some(key.to_string());
                *block_buf = Some(String::new());
            } else {
                current.set(key, value);
            }
        }
    };

    for line in content.lines() {
        if line == "---" {
            continue;
        }

        if let Some(rest) = line.strip_prefix("- ") {
            flush_block(&mut current, &mut block_key, &mut block_buf);
            if in_entry {
                flush_entry(&mut current, &mut inherited, &mut results);
            }
            in_entry = true;
            current = EntryFields::default();
            parse_field(rest, &mut current, &mut block_key, &mut block_buf);
        } else if let Some(indented) = block_buf
            .is_some()
            .then(|| line.strip_prefix("    "))
            .flatten()
        {
            block_buf.as_mut().unwrap().push_str(indented);
            block_buf.as_mut().unwrap().push('\n');
        } else if line.starts_with("  ") && !line.starts_with("    ") {
            flush_block(&mut current, &mut block_key, &mut block_buf);
            parse_field(line.trim(), &mut current, &mut block_key, &mut block_buf);
        }
    }

    flush_block(&mut current, &mut block_key, &mut block_buf);
    if in_entry {
        flush_entry(&mut current, &mut inherited, &mut results);
    }

    results
}

fn load_cases_from_file(path: &Path) -> Vec<ConformanceCase> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    parse_test_metadata(&content)
        .into_iter()
        .enumerate()
        .map(|(idx, (name, yaml, fail))| {
            let name = if name.is_empty() {
                format!("{file_name}-{idx:02}")
            } else {
                name
            };
            ConformanceCase {
                file: file_name.clone(),
                stem: stem.clone(),
                index: idx,
                name,
                yaml: visual_to_raw(&yaml),
                fail,
            }
        })
        .collect()
}

// ---- Panic helper -----------------------------------------------------------

/// Terminate the test with a clear message.
#[expect(clippy::panic, reason = "test harness reports failures via panic")]
fn fail(msg: &str) -> ! {
    panic!("{msg}")
}

// ---- rstest harness ---------------------------------------------------------

#[rstest]
#[timeout(Duration::from_secs(5))]
fn formatter_conformance(#[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf) {
    let cases = load_cases_from_file(&path);
    if cases.is_empty() {
        // All entries are skipped (e.g., ZYU8). Nothing to test.
        return;
    }

    let opts = YamlFormatOptions::default();

    for case in &cases {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);
        let key = case.allowlist_key();

        // Intentionally-invalid YAML is not formatted.
        if case.fail {
            continue;
        }

        let input = &case.yaml;
        let output = format_yaml(input, &opts);

        // Empty-output guard: non-empty input must produce non-empty output.
        let empty_output_failed = !input.trim().is_empty() && output.trim().is_empty();

        // Parse-clean assertion.
        let diagnostics = parse_yaml(&output).diagnostics;
        let parse_failed = !diagnostics.is_empty();

        // Idempotency assertion.
        let second = format_yaml(&output, &opts);
        let idempotent_failed = output != second;

        let any_failed = empty_output_failed || parse_failed || idempotent_failed;

        if is_known_failure(&key) {
            assert!(
                any_failed,
                "formatter_conformance: {tag} unexpectedly passed — \
                 remove \"{key}\" from KNOWN_FAILURES in \
                 rlsp-yaml/tests/formatter_conformance.rs",
            );
            // Expected failure: continue without further asserting.
            continue;
        }

        // Non-allowlisted case: all assertions must hold.
        assert!(
            !empty_output_failed,
            "formatter_conformance: {tag} — formatter returned empty output for non-empty input\n  \
             input: {:?}",
            &input[..input.len().min(120)]
        );

        if parse_failed {
            let diag_msgs: Vec<String> = diagnostics.iter().map(|d| d.message.clone()).collect();
            fail(&format!(
                "formatter_conformance: {tag} — formatted output does not parse cleanly\n  \
                 input:       {:?}\n  \
                 output:      {:?}\n  \
                 diagnostics: {:?}",
                &input[..input.len().min(120)],
                &output[..output.len().min(120)],
                diag_msgs,
            ));
        }

        assert!(
            !idempotent_failed,
            "formatter_conformance: {tag} — formatter is not idempotent\n  \
             input:  {:?}\n  \
             first:  {:?}\n  \
             second: {:?}",
            &input[..input.len().min(120)],
            &output[..output.len().min(200)],
            &second[..second.len().min(200)],
        );
    }
}

// ---- Unit tests for harness helpers -----------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. A key present in KNOWN_FAILURES is found.
    #[test]
    fn allowlist_known_failure_matches_by_exact_key() {
        // 26DV[0] is in KNOWN_FAILURES.
        assert!(is_known_failure("26DV[0]"));
    }

    // 2. A key absent from KNOWN_FAILURES is not found.
    #[test]
    fn allowlist_unknown_case_id_not_in_list() {
        assert!(!is_known_failure("YYYY[0]"));
    }

    // 3. KNOWN_FAILURES is sorted and contains no duplicates.
    #[test]
    fn allowlist_is_sorted_and_has_no_duplicates() {
        for pair in KNOWN_FAILURES.windows(2) {
            let (a, b) = (pair.first().unwrap(), pair.get(1).unwrap());
            assert!(
                a < b,
                "KNOWN_FAILURES is not strictly sorted or has duplicates: \
                 {a:?} >= {b:?}",
            );
        }
    }
}
