// SPDX-License-Identifier: MIT
//
// Boundary audit: enforce the "One parser, one AST" rule from CLAUDE.md.
//
// Scans `rlsp-yaml/src/**/*.rs` for `pub fn validate_*` and `pub fn code_actions`
// signatures whose first `&str` parameter is named `text`. Any match not on the
// allow-list is a new violation of the rule and causes this test to fail.
//
// # Allow-list discipline (SHRINK-ONLY)
//
// The allow-list carries the known remaining violators as a visible worklist.
// Entries are REMOVED as violators are retrofitted in follow-up plans.
// NEW entries are NEVER added for new violations.
// The only exception is a genuine carve-out (modeline extraction, BOM detection,
// whitespace-preserving edit that doesn't touch structure), which must include a
// `// carve-out:` justification comment referencing the exception category.
//
// If you find yourself wanting to add an entry here for a new function, that is
// a signal that the function should consume the parser AST instead of raw text.

#![expect(missing_docs, reason = "test code")]
#![expect(
    clippy::unwrap_used,
    reason = "test code — unwrap on infallible regex and Option values"
)]
#![expect(
    clippy::expect_used,
    reason = "test code — expect on infallible filesystem operations"
)]
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::{fmt, fs};

// ---------------------------------------------------------------------------
// Allow-list
// ---------------------------------------------------------------------------

/// A known remaining violator that has not yet been retrofitted.
///
/// `file` is relative to `rlsp-yaml/src/` (e.g., `"validation/validators.rs"`).
/// `func` is the function name without generics or parameter list.
#[derive(Debug, PartialEq, Eq, Hash)]
struct AllowEntry {
    file: &'static str,
    func: &'static str,
    note: &'static str,
}

impl fmt::Display for AllowEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{} ({})", self.file, self.func, self.note)
    }
}

/// Each entry must have a TODO or carve-out justification.
///
/// Entries here are removed as violators are retrofitted. See allow-list
/// discipline comment at the top of this file.
const ALLOW_LIST: &[AllowEntry] = &[
    AllowEntry {
        file: "validation/validators.rs",
        func: "validate_unused_anchors",
        note: "TODO(retrofit-validate-unused-anchors): pure text-scan, follow-up plan not yet filed",
    },
    AllowEntry {
        file: "validation/validators.rs",
        func: "validate_custom_tags",
        note: "TODO(retrofit-validate-custom-tags): hybrid text+docs, follow-up plan not yet filed",
    },
    AllowEntry {
        file: "validation/validators.rs",
        func: "validate_key_ordering",
        note: "TODO(retrofit-validate-key-ordering): hybrid text+docs, follow-up plan not yet filed",
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "code_actions",
        note: "TODO(.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md): top-level dispatcher passes raw text to internal helpers",
    },
    AllowEntry {
        file: "schema_validation.rs",
        func: "validate_schema",
        note: "TODO(retrofit-validate-schema): hybrid text+docs, follow-up plan not yet filed",
    },
];

// ---------------------------------------------------------------------------
// Detection helpers (also exercised by unit tests below)
// ---------------------------------------------------------------------------

/// Returns `true` if `line` starts a candidate function declaration:
/// `pub fn validate_<name>` or `pub fn code_actions`.
fn is_candidate_fn_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("pub fn ") {
        return false;
    }
    let after_pub_fn = &trimmed["pub fn ".len()..];
    after_pub_fn.starts_with("validate_") || after_pub_fn.starts_with("code_actions")
}

/// Extracts the bare function name from a candidate function declaration line.
///
/// Handles generics: `pub fn validate_foo<S>(` → `"validate_foo"`.
fn extract_fn_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("pub fn ")?;
    // Name ends at `<`, `(`, or whitespace.
    let end = rest
        .find(|c: char| c == '<' || c == '(' || c.is_whitespace())
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() { None } else { Some(name) }
}

/// Returns `true` if the collected parameter block for a function contains
/// `text: &str` as a parameter (not just any `&str` named something else).
fn has_text_str_param(param_block: &str) -> bool {
    let re = Regex::new(r"\btext\s*:\s*&str\b").unwrap();
    re.is_match(param_block)
}

/// A detected violation: a function whose signature contains `text: &str`.
#[derive(Debug, PartialEq, Eq)]
struct Violation {
    /// Relative path from `rlsp-yaml/src/`.
    rel_path: String,
    func: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.rel_path, self.func)
    }
}

/// Scan a single source file for violations.
///
/// Returns all candidate functions whose parameter list includes `text: &str`.
fn scan_file(rel_path: &str, source: &str) -> Vec<Violation> {
    let lines: Vec<&str> = source.lines().collect();

    lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            if !is_candidate_fn_line(line) {
                return None;
            }
            let func_name = extract_fn_name(line)?;
            // Collect the parameter block spanning up to 10 lines to handle
            // multi-line signatures.
            let window: Vec<&str> = lines.iter().skip(i).take(10).copied().collect();
            let combined = window.join(" ");
            let param_block = extract_param_block(&combined);
            if has_text_str_param(&param_block) {
                Some(Violation {
                    rel_path: rel_path.to_owned(),
                    func: func_name.to_owned(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Extracts the content between the first `(` and its matching `)`.
/// Returns an empty string if the opening paren is not found.
fn extract_param_block(s: &str) -> String {
    let Some(start) = s.find('(') else {
        return String::new();
    };
    let after = &s[start + 1..];
    let mut depth = 1usize;
    let mut end = after.len();
    for (idx, ch) in after.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = idx;
                    break;
                }
            }
            _ => {}
        }
    }
    after[..end].to_owned()
}

// ---------------------------------------------------------------------------
// The audit test
// ---------------------------------------------------------------------------

#[test]
fn parser_boundary_audit() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut violations: Vec<Violation> = Vec::new();
    collect_violations(&src_dir, &src_dir, &mut violations);

    // --- Detect dead allow-list entries ---
    let detected_keys: HashSet<(&str, &str)> = violations
        .iter()
        .filter_map(|v| {
            let normalized = v.rel_path.replace(std::path::MAIN_SEPARATOR, "/");
            ALLOW_LIST
                .iter()
                .find(|e| normalized.ends_with(e.file) && e.func == v.func.as_str())
                .map(|e| (e.file, e.func))
        })
        .collect();

    let dead_entries: Vec<&AllowEntry> = ALLOW_LIST
        .iter()
        .filter(|e| !detected_keys.contains(&(e.file, e.func)))
        .collect();

    // --- Filter out allowed violations ---
    let new_violations: Vec<&Violation> = violations
        .iter()
        .filter(|v| {
            let normalized = v.rel_path.replace(std::path::MAIN_SEPARATOR, "/");
            !ALLOW_LIST
                .iter()
                .any(|e| normalized.ends_with(e.file) && e.func == v.func.as_str())
        })
        .collect();

    // --- Report ---
    let mut messages = Vec::new();

    if !new_violations.is_empty() {
        messages.push(format!(
            "BOUNDARY VIOLATION: {} new function(s) take `text: &str` outside the allow-list.\n\
             These functions must consume the parser AST instead of raw text (CLAUDE.md rule).\n\
             Violations:\n{}",
            new_violations.len(),
            new_violations
                .iter()
                .map(|v| format!("  - {v}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if !dead_entries.is_empty() {
        messages.push(format!(
            "DEAD ALLOW-LIST ENTRIES: {} entry/entries have no matching function in src/.\n\
             Remove them — the allow-list is shrink-only.\n\
             Dead entries:\n{}",
            dead_entries.len(),
            dead_entries
                .iter()
                .map(|e| format!("  - {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    assert!(messages.is_empty(), "{}", messages.join("\n\n"));
}

/// Recursively collect violations from all `.rs` files under `dir`.
fn collect_violations(base: &PathBuf, dir: &PathBuf, out: &mut Vec<Violation>) {
    for entry in fs::read_dir(dir).expect("read_dir failed") {
        let entry = entry.expect("DirEntry failed");
        let path = entry.path();
        if path.is_dir() {
            collect_violations(base, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let rel = path
                .strip_prefix(base)
                .expect("path is under base")
                .to_string_lossy()
                .into_owned();
            let source = fs::read_to_string(&path).expect("read_to_string failed");
            out.extend(scan_file(&rel, &source));
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests for detection helpers (Groups A and B)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod detection_tests {
    use super::*;

    // --- Group A: detection regex correctness ---

    #[test]
    fn validate_foo_text_str_detected() {
        let line = "pub fn validate_foo(text: &str, docs: &[Document]) -> Vec<Diagnostic> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(has_text_str_param(&params));
    }

    #[test]
    fn validate_foo_no_text_str_not_detected() {
        let line = "pub fn validate_foo(docs: &[Document<Span>]) -> Vec<Diagnostic> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(!has_text_str_param(&params));
    }

    #[test]
    fn private_validate_not_detected() {
        let line = "fn validate_foo(text: &str) -> Vec<Diagnostic> {";
        assert!(!is_candidate_fn_line(line));
    }

    #[test]
    fn code_actions_pub_fn_text_str_detected() {
        let line = "pub fn code_actions(text: &str, range: Range, diagnostics: &[Diagnostic]) -> Vec<CodeAction> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(has_text_str_param(&params));
    }

    #[test]
    fn flow_map_to_block_private_not_detected() {
        let line =
            "fn flow_map_to_block(lines: &[&str], diag: &Diagnostic) -> Option<CodeAction> {";
        assert!(!is_candidate_fn_line(line));
    }

    #[test]
    fn non_text_named_str_param_not_detected() {
        // Matches `validate_` prefix, but param is `raw: &str`, not `text: &str`.
        let line = "pub fn validate_and_normalize_url(raw: &str) -> Option<String> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(!has_text_str_param(&params));
    }

    #[test]
    fn generic_validate_fn_detected() {
        // Generics on the function name: `pub fn validate_custom_tags<S: ...>(text: &str, ...)`
        let line = "pub fn validate_custom_tags<S: std::hash::BuildHasher>(";
        let next = "    text: &str,";
        let combined = format!("{line} {next}");
        assert!(is_candidate_fn_line(line));
        let name = extract_fn_name(line).unwrap();
        assert_eq!(name, "validate_custom_tags");
        let params = extract_param_block(&combined);
        assert!(has_text_str_param(&params));
    }

    // --- Group B: allow-list mechanics ---

    #[test]
    fn allowed_entry_suppresses_violation() {
        let source =
            "pub fn validate_unused_anchors(text: &str) -> Vec<Diagnostic> {\n    vec![]\n}";
        let violations = scan_file("validation/validators.rs", source);
        assert_eq!(violations.len(), 1);

        let new_violations: Vec<&Violation> = violations
            .iter()
            .filter(|v| {
                !ALLOW_LIST
                    .iter()
                    .any(|e| e.file == v.rel_path.as_str() && e.func == v.func.as_str())
            })
            .collect();
        assert!(
            new_violations.is_empty(),
            "expected allow-list to suppress the violation, but got: {new_violations:?}"
        );
    }

    #[test]
    fn unlisted_match_causes_failure() {
        let source = "pub fn validate_sentinel(text: &str) -> Vec<Diagnostic> {\n    vec![]\n}";
        let violations = scan_file("validation/validators.rs", source);
        assert_eq!(violations.len(), 1);

        let new_violations: Vec<&Violation> = violations
            .iter()
            .filter(|v| {
                !ALLOW_LIST
                    .iter()
                    .any(|e| e.file == v.rel_path.as_str() && e.func == v.func.as_str())
            })
            .collect();
        assert_eq!(
            new_violations.len(),
            1,
            "expected one unallowed violation for validate_sentinel"
        );
        assert!(
            new_violations.iter().any(|v| v.func == "validate_sentinel"),
            "expected validate_sentinel in violations, got: {new_violations:?}"
        );
    }

    #[test]
    fn dead_allow_list_entry_for_nonexistent_fn_is_flagged() {
        // Simulate: scan a file that does NOT contain `validate_nonexistent`.
        let source = "pub fn validate_real(docs: &[Document]) -> Vec<Diagnostic> {\n    vec![]\n}";
        let violations = scan_file("validation/validators.rs", source);

        // Build a local allow-list with a dead entry.
        let local_allow: &[(&str, &str)] = &[
            ("validation/validators.rs", "validate_real"),
            ("validation/validators.rs", "validate_nonexistent"), // dead
        ];

        let detected_keys: HashSet<(&str, &str)> = violations
            .iter()
            .filter_map(|v| {
                local_allow
                    .iter()
                    .find(|(f, func)| v.rel_path == *f && v.func == *func)
                    .copied()
            })
            .collect();

        let dead: Vec<(&str, &str)> = local_allow
            .iter()
            .copied()
            .filter(|e| !detected_keys.contains(e))
            .collect();

        // `validate_real` has no `text: &str` param so it's not detected; only the
        // nonexistent entry is dead in this context. The point is that the mechanism works.
        assert!(
            dead.iter().any(|(_, func)| *func == "validate_nonexistent"),
            "dead entry detection should flag validate_nonexistent; got: {dead:?}"
        );
    }
}
