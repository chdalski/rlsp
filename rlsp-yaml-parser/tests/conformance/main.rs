// SPDX-License-Identifier: MIT
//
// Conformance test suite using yaml-test-suite data symlinked at
// `tests/yaml-test-suite/src/` (commit recorded in `src/.commit`).
//
// Sub-modules:
//   stream  — event-stream API (`parse_events`) conformance, 351/351
//   loader  — loader AST (`load`) conformance (Task 2)

#![expect(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    missing_docs,
    reason = "test code"
)]

pub mod loader;
pub mod stream;

// ---- Test-case data model ---------------------------------------------------

#[derive(Debug)]
pub struct ConformanceCase {
    pub file: String,
    pub index: usize,
    pub name: String,
    pub yaml: String,
    pub fail: bool,
    /// Raw event-tree string from the `tree:` field, after `visual_to_raw`.
    /// `None` when the test-suite entry has no `tree:` field.
    pub tree: Option<String>,
}

// ---- Visual-representation helpers (from yaml-test-suite convention) --------

/// Convert the yaml-test-suite "visual" representation to real YAML text.
///
/// The test suite uses Unicode markers to represent whitespace characters that
/// would be invisible in plain text:
///   `␣` → space, `»` → tab, `←` → CR, `⇔` → BOM, `↵` → (nothing), `∎\n` → (nothing)
#[must_use]
pub fn visual_to_raw(s: &str) -> String {
    s.replace('␣', " ")
        .replace('»', "\t")
        .replace('—', "")
        .replace('←', "\r")
        .replace('\u{21D4}', "\u{FEFF}")
        .replace('↵', "")
        .replace("∎\n", "")
}

// ---- Loading test cases from the vendored YAML files ------------------------

/// Accumulated fields for one metadata entry. Fields not set in the current
/// entry are inherited from the previous one (except `fail`, which resets).
#[derive(Default)]
struct EntryFields {
    name: Option<String>,
    yaml: Option<String>,
    tree: Option<String>,
    fail: Option<bool>,
    skip: bool,
}

impl EntryFields {
    /// Apply a parsed `key: value` pair (inline scalar).
    fn set(&mut self, key: &str, value: &str) {
        match key {
            "name" => self.name = Some(value.to_string()),
            "yaml" => self.yaml = Some(value.to_string()),
            "fail" => self.fail = Some(value == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }

    /// Apply a completed block scalar for the given key.
    fn set_block(&mut self, key: &str, block: String) {
        match key {
            "name" => self.name = Some(block),
            "yaml" => self.yaml = Some(block),
            "tree" => self.tree = Some(block),
            "fail" => self.fail = Some(block.trim() == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }
}

/// One parsed entry from `parse_test_metadata`.
pub struct TestEntry {
    pub name: String,
    pub yaml: String,
    pub tree: Option<String>,
    pub fail: bool,
}

/// Parse the yaml-test-suite metadata format without a YAML library.
///
/// Returns [`TestEntry`] values. Entries with `skip` are omitted.
#[must_use]
pub fn parse_test_metadata(content: &str) -> Vec<TestEntry> {
    let mut results: Vec<TestEntry> = Vec::new();
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
            // Merge current into inherited
            if let Some(n) = current.name.take() {
                inherited.name = Some(n);
            }
            if let Some(y) = current.yaml.take() {
                inherited.yaml = Some(y);
            }
            if let Some(t) = current.tree.take() {
                inherited.tree = Some(t);
            }
            if current.skip {
                inherited.skip = true;
            }
            let fail = current.fail.take().unwrap_or(false);
            if !inherited.skip {
                if let Some(ref yaml) = inherited.yaml {
                    let name = inherited.name.clone().unwrap_or_default();
                    results.push(TestEntry {
                        name,
                        yaml: yaml.clone(),
                        tree: inherited.tree.clone(),
                        fail,
                    });
                }
            }
            // Reset tree per-entry (it is not inherited like yaml/name).
            inherited.tree = None;
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

#[must_use]
pub fn load_cases_from_file(path: &std::path::Path) -> Vec<ConformanceCase> {
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
        .map(|(idx, entry)| {
            let name = if entry.name.is_empty() {
                format!("{file_name}-{idx:02}")
            } else {
                entry.name
            };
            ConformanceCase {
                file: file_name.clone(),
                index: idx,
                name,
                yaml: visual_to_raw(&entry.yaml),
                tree: entry.tree.map(|t| visual_to_raw(&t)),
                fail: entry.fail,
            }
        })
        .collect()
}
