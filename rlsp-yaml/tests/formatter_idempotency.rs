// SPDX-License-Identifier: MIT
//
// Property-based idempotency test for the YAML formatter with varied options.
//
// For each non-fail yaml-test-suite case combined with randomly generated
// YamlFormatOptions (varying print_width, tab_width, single_quote), asserts:
//
//   format(format(input, opts), opts) == format(input, opts)
//
// This verifies that the formatter is stable under different configurations,
// not only with the default options tested in formatter_conformance.rs.

#![expect(missing_docs, reason = "test code")]

use std::path::Path;

use proptest::prelude::*;
use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};

// ---- yaml-test-suite loader (mirrors formatter_conformance.rs) --------------

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

/// Returns `true` if `value` (the text after `: ` on a field line) introduces a
/// YAML block scalar.  Matches `|` or `|N` (explicit indentation indicator).
fn is_block_scalar_value(value: &str) -> bool {
    value == "|"
        || (value.starts_with('|')
            && value.len() == 2
            && value.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
}

/// Apply a non-`skip` parsed field to the current entry state.
fn apply_field(
    key: &str,
    value: &str,
    yaml: &mut Option<String>,
    fail: &mut bool,
    skip: &mut bool,
) {
    match key {
        "yaml" => *yaml = Some(value.to_string()),
        "fail" => *fail = value == "true",
        "skip" => *skip = true,
        _ => {}
    }
}

/// Apply a block-scalar field (key + accumulated buffer) to the current entry state.
fn apply_block_field(key: &str, buf: &str, yaml: &mut Option<String>, fail: &mut bool) {
    match key {
        "yaml" => *yaml = Some(buf.to_string()),
        "fail" => *fail = buf.trim() == "true",
        _ => {}
    }
}

/// Commit the current entry to `results` if not skipped, then reset mutable state.
fn commit_entry(
    yaml: &mut Option<String>,
    fail: &mut bool,
    skip: bool,
    results: &mut Vec<(String, bool)>,
) {
    if skip {
        yaml.take();
    } else if let Some(y) = yaml.take() {
        results.push((y, *fail));
    }
    *fail = false;
}

/// Parse a yaml-test-suite `.yaml` metadata file and return `(yaml, fail)` pairs.
///
/// The format is a simplified YAML list where each entry has at least a `name`,
/// optional `yaml` (the test input), and optional `fail: true` flag.  Block
/// scalars (`|`) are supported for `yaml` and `fail` fields.
fn parse_test_metadata(content: &str) -> Vec<(String, bool)> {
    let mut results = Vec::new();
    let mut current_yaml: Option<String> = None;
    let mut current_fail = false;
    let mut in_block_scalar = false;
    let mut block_key = String::new();
    let mut block_buf = String::new();
    let mut skip = false;
    let mut in_entry = false;

    for line in content.lines() {
        if line == "---" {
            continue;
        }

        if let Some(rest) = line.strip_prefix("- ") {
            if in_block_scalar {
                apply_block_field(&block_key, &block_buf, &mut current_yaml, &mut current_fail);
                in_block_scalar = false;
                block_buf.clear();
                block_key.clear();
            }
            if in_entry {
                commit_entry(&mut current_yaml, &mut current_fail, skip, &mut results);
            }
            in_entry = true;
            skip = false;

            if let Some((key, value)) = rest.split_once(": ") {
                let key = key.trim();
                let value = value.trim();
                if is_block_scalar_value(value) {
                    in_block_scalar = true;
                    block_key = key.to_string();
                    block_buf.clear();
                } else {
                    apply_field(key, value, &mut current_yaml, &mut current_fail, &mut skip);
                }
            }
        } else if in_block_scalar {
            if let Some(indented) = line.strip_prefix("    ") {
                block_buf.push_str(indented);
                block_buf.push('\n');
            } else if line.trim_matches([' ', '\t']).is_empty() {
                block_buf.push('\n');
            } else {
                apply_block_field(&block_key, &block_buf, &mut current_yaml, &mut current_fail);
                in_block_scalar = false;
                block_buf.clear();
                block_key.clear();
                if line.starts_with("  ")
                    && !line.starts_with("    ")
                    && let Some((key, value)) = line.trim().split_once(": ")
                {
                    apply_field(key, value, &mut current_yaml, &mut current_fail, &mut skip);
                }
            }
        } else if line.starts_with("  ")
            && !line.starts_with("    ")
            && let Some((key, value)) = line.trim().split_once(": ")
        {
            apply_field(key, value, &mut current_yaml, &mut current_fail, &mut skip);
        }
    }

    if in_block_scalar {
        apply_block_field(&block_key, &block_buf, &mut current_yaml, &mut current_fail);
    }
    if in_entry {
        commit_entry(&mut current_yaml, &mut current_fail, skip, &mut results);
    }

    results
}

/// Load all valid (non-fail) YAML inputs from the yaml-test-suite corpus.
///
/// Relative path is resolved from the workspace root so this test can be run
/// from any working directory that cargo uses.
fn load_valid_test_suite_inputs() -> Vec<String> {
    let suite_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("tests/yaml-test-suite/src");

    let mut inputs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&suite_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (yaml, fail) in parse_test_metadata(&content) {
                if !fail {
                    inputs.push(visual_to_raw(&yaml));
                }
            }
        }
    }
    inputs
}

/// Cached valid inputs loaded once per process.
fn valid_inputs() -> &'static Vec<String> {
    static INPUTS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    INPUTS.get_or_init(load_valid_test_suite_inputs)
}

// ---- YamlFormatOptions strategy ---------------------------------------------

fn format_options_strategy() -> impl Strategy<Value = YamlFormatOptions> {
    (
        // print_width: 20–160
        20usize..=160usize,
        // tab_width: 1–8
        1usize..=8usize,
        // single_quote: bool
        any::<bool>(),
    )
        .prop_map(|(print_width, tab_width, single_quote)| YamlFormatOptions {
            print_width,
            tab_width,
            single_quote,
            ..YamlFormatOptions::default()
        })
}

// ---- Property test ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    /// For any valid yaml-test-suite input and any combination of `print_width`,
    /// `tab_width`, and `single_quote`, the formatter is idempotent:
    ///
    ///   `format(format(input, opts), opts) == format(input, opts)`
    #[test]
    fn formatter_idempotent_with_random_options(
        idx in any::<prop::sample::Index>(),
        opts in format_options_strategy(),
    ) {
        let inputs = valid_inputs();
        prop_assume!(!inputs.is_empty());

        let input = idx.get(inputs);

        let first = format_yaml(input, &opts);
        let second = format_yaml(&first, &opts);

        prop_assert_eq!(
            &first,
            &second,
            "formatter not idempotent for input {:?} with options {:?}\n  \
             first:  {:?}\n  second: {:?}",
            &input[..input.len().min(200)],
            opts.print_width,
            &first[..first.len().min(300)],
            &second[..second.len().min(300)],
        );
    }
}
