// SPDX-License-Identifier: MIT
//
// Formatter round-trip conformance test using yaml-test-suite data at
// `tests/yaml-test-suite/src/` (workspace root, relative path `../tests/`).
//
// For each non-fail case in the suite:
//   1. `format_yaml(input, &default_opts)` must produce output that parses
//      cleanly (no diagnostics).
//   2. Formatting must be idempotent: `format(format(input)) == format(input)`.

#![expect(clippy::unwrap_used, missing_docs, reason = "test code")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;
use rstest::rstest;

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
    index: usize,
    name: String,
    yaml: String,
    fail: bool,
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
            // Match `|` or `|N` (explicit indentation indicator, e.g. `|2`).
            let is_block_scalar = value == "|"
                || (value.starts_with('|')
                    && value.len() == 2
                    && value.as_bytes().get(1).is_some_and(u8::is_ascii_digit));
            if is_block_scalar {
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
        } else if block_buf.is_some() {
            if let Some(indented) = line.strip_prefix("    ") {
                block_buf.as_mut().unwrap().push_str(indented);
                block_buf.as_mut().unwrap().push('\n');
            } else if line.trim_matches([' ', '\t']).is_empty() {
                // Blank line within the block scalar — preserve as an empty line.
                block_buf.as_mut().unwrap().push('\n');
            } else {
                // Non-blank, non-indented line ends the block.
                flush_block(&mut current, &mut block_key, &mut block_buf);
                if line.starts_with("  ") && !line.starts_with("    ") {
                    parse_field(line.trim(), &mut current, &mut block_key, &mut block_buf);
                }
            }
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

        // Intentionally-invalid YAML is not formatted.
        if case.fail {
            continue;
        }

        let input = &case.yaml;
        let output = format_yaml(input, &opts);

        // Parse-clean assertion.
        // Empty output is valid for comment-only or doc-end-only streams.
        let diagnostics = parse_yaml(&output).diagnostics;
        let parse_failed = !diagnostics.is_empty();

        // Idempotency assertion.
        let second = format_yaml(&output, &opts);
        let idempotent_failed = output != second;

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
