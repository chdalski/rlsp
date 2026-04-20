// SPDX-License-Identifier: MIT
//
// Boundary audit: enforce the "One parser, one AST" rule from CLAUDE.md.
//
// Scans `rlsp-yaml/src/**/*.rs` for any `(pub )?fn` whose first positional
// parameter is named `text`, `line`, `lines`, `content`, `source`, or `input`
// with type `&str` or `&[&str]`. Any match not on the allow-list is a new
// violation of the rule and causes this test to fail.
//
// # Marker taxonomy
//
// Every allow-list entry carries an `AllowMarker` that classifies it:
//
// - `TodoRetrofit { plan }` — violator that must consume the parser AST
//   instead of raw text; "plan" identifies the follow-up retrofit plan.
// - `HelperOf { root }` — private helper whose root entry point is already on
//   the allow-list; disappears when the root is retrofitted (no independent
//   retrofit needed).
// - `CarveOut { reason }` — exempt from the rule: either the canonical parser
//   entry point, a pre-parse lexical concern (modeline/BOM/comment extraction),
//   whitespace-only edit, or a test fixture.
//
// # Allow-list discipline (SHRINK-ONLY)
//
// Entries are REMOVED as violators are retrofitted in follow-up plans.
// NEW entries are NEVER added for new violations introduced after this audit.
// The only acceptable additions are:
//   a) a genuine `CarveOut` with a written justification, or
//   b) a newly introduced feature root that requires a retrofit plan to be filed.
//
// If you find yourself wanting to add an entry for a new function, that is a
// signal the function should consume the parser AST instead of raw text.

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

/// Classification of an allow-list entry.
#[derive(Debug, PartialEq, Eq, Hash)]
enum AllowMarker {
    /// Violator that must be retrofitted to consume the parser AST.
    TodoRetrofit { plan: &'static str },
    /// Private helper of a `TodoRetrofit` root entry; disappears with root.
    HelperOf { root: &'static str },
    /// Exempt: canonical parser entry point, pre-parse lexical concern,
    /// whitespace-only edit, or test fixture.
    CarveOut { reason: &'static str },
}

impl fmt::Display for AllowMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TodoRetrofit { plan } => write!(f, "TodoRetrofit(plan={plan})"),
            Self::HelperOf { root } => write!(f, "HelperOf(root={root})"),
            Self::CarveOut { reason } => write!(f, "CarveOut(reason={reason})"),
        }
    }
}

/// A known entry — either a violator pending retrofit, a helper-of, or a carve-out.
///
/// `file` is relative to `rlsp-yaml/src/` (e.g., `"validation/validators.rs"`).
/// `func` is the function name without generics or parameter list.
#[derive(Debug, PartialEq, Eq, Hash)]
struct AllowEntry {
    file: &'static str,
    func: &'static str,
    marker: AllowMarker,
}

impl fmt::Display for AllowEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{} [{}]", self.file, self.func, self.marker)
    }
}

/// Each entry must have an explicit marker. See allow-list discipline comment above.
const ALLOW_LIST: &[AllowEntry] = &[
    // -----------------------------------------------------------------------
    // Feature-level violators (original 5)
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "completion.rs",
        func: "complete_at",
        marker: AllowMarker::TodoRetrofit {
            plan: "retrofit-complete-at",
        },
    },
    AllowEntry {
        file: "editing/on_type_formatting.rs",
        func: "format_on_type",
        marker: AllowMarker::TodoRetrofit {
            plan: "retrofit-format-on-type",
        },
    },
    // -----------------------------------------------------------------------
    // Feature-level violators (surfaced during reconciliation)
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "analysis/folding.rs",
        func: "folding_ranges",
        marker: AllowMarker::TodoRetrofit {
            plan: "retrofit-folding-ranges",
        },
    },
    AllowEntry {
        file: "analysis/semantic_tokens.rs",
        func: "semantic_tokens",
        marker: AllowMarker::TodoRetrofit {
            plan: "retrofit-semantic-tokens",
        },
    },
    // -----------------------------------------------------------------------
    // HelperOf — private helpers of format_on_type
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "editing/on_type_formatting.rs",
        func: "leading_spaces",
        marker: AllowMarker::HelperOf {
            root: "format_on_type",
        },
    },
    AllowEntry {
        file: "editing/on_type_formatting.rs",
        func: "find_mapping_colon",
        marker: AllowMarker::HelperOf {
            root: "format_on_type",
        },
    },
    // -----------------------------------------------------------------------
    // HelperOf — private helpers of complete_at
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "completion.rs",
        func: "build_key_path",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "build_value_key_path",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "collect_present_keys_at_indent",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "classify_cursor",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "suggest_sibling_keys",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "is_in_sequence_item",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "suggest_keys_for_sequence_item",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "collect_current_sequence_item_keys",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "find_current_item_start",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "find_sequence_indent",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "collect_all_sequence_item_keys",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "collect_sibling_keys",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "find_mapping_colon",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "indentation_level",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "document_range",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "suggest_values_for_key",
        marker: AllowMarker::HelperOf {
            root: "complete_at",
        },
    },
    // -----------------------------------------------------------------------
    // HelperOf — private helpers of folding_ranges
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "analysis/folding.rs",
        func: "collect_indentation_folds",
        marker: AllowMarker::HelperOf {
            root: "folding_ranges",
        },
    },
    AllowEntry {
        file: "analysis/folding.rs",
        func: "collect_document_section_folds",
        marker: AllowMarker::HelperOf {
            root: "folding_ranges",
        },
    },
    AllowEntry {
        file: "analysis/folding.rs",
        func: "collect_comment_block_folds",
        marker: AllowMarker::HelperOf {
            root: "folding_ranges",
        },
    },
    AllowEntry {
        file: "analysis/folding.rs",
        func: "find_last_content_line",
        marker: AllowMarker::HelperOf {
            root: "folding_ranges",
        },
    },
    AllowEntry {
        file: "analysis/folding.rs",
        func: "find_last_content_line_in_range",
        marker: AllowMarker::HelperOf {
            root: "folding_ranges",
        },
    },
    AllowEntry {
        file: "analysis/folding.rs",
        func: "find_mapping_colon",
        marker: AllowMarker::HelperOf {
            root: "folding_ranges",
        },
    },
    // -----------------------------------------------------------------------
    // HelperOf — private helpers of semantic_tokens
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "analysis/semantic_tokens.rs",
        func: "collect_inline_markers",
        marker: AllowMarker::HelperOf {
            root: "semantic_tokens",
        },
    },
    AllowEntry {
        file: "analysis/semantic_tokens.rs",
        func: "char_col_of",
        marker: AllowMarker::HelperOf {
            root: "semantic_tokens",
        },
    },
    AllowEntry {
        file: "analysis/semantic_tokens.rs",
        func: "find_mapping_colon",
        marker: AllowMarker::HelperOf {
            root: "semantic_tokens",
        },
    },
    // -----------------------------------------------------------------------
    // CarveOut — pre-parse lexical concerns and whitespace
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "parser.rs",
        func: "parse_yaml",
        marker: AllowMarker::CarveOut {
            reason: "canonical parser entry point — this IS the one parser the rule references",
        },
    },
    AllowEntry {
        file: "validation/suppression.rs",
        func: "build_suppression_map",
        marker: AllowMarker::CarveOut {
            reason: "pre-parse lexical: scans rlsp-yaml-disable comments before YAML parsing",
        },
    },
    AllowEntry {
        file: "editing/formatter.rs",
        func: "extract_doc_prefix_comments",
        marker: AllowMarker::CarveOut {
            reason: "pre-parse lexical: document-prefix comment extraction",
        },
    },
    AllowEntry {
        file: "editing/formatter.rs",
        func: "find_comment_on_line",
        marker: AllowMarker::CarveOut {
            reason: "pre-parse lexical: comment-boundary scan helper for document-prefix extraction",
        },
    },
    AllowEntry {
        file: "editing/formatter.rs",
        func: "content_signature",
        marker: AllowMarker::CarveOut {
            reason: "pre-parse lexical: helper of find_comment_on_line",
        },
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "tab_to_spaces",
        marker: AllowMarker::CarveOut {
            reason: "whitespace normalization: tabs are YAML 1.2 §6.1 pre-parse lexical",
        },
    },
    // -----------------------------------------------------------------------
    // CarveOut — schema association modeline extraction
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "schema/association.rs",
        func: "extract_schema_url",
        marker: AllowMarker::CarveOut {
            reason: "pre-parse lexical: schema URL modeline extraction",
        },
    },
    AllowEntry {
        file: "schema/association.rs",
        func: "extract_yaml_version",
        marker: AllowMarker::CarveOut {
            reason: "pre-parse lexical: YAML version modeline extraction",
        },
    },
    AllowEntry {
        file: "schema/association.rs",
        func: "extract_custom_tags",
        marker: AllowMarker::CarveOut {
            reason: "pre-parse lexical: custom tag modeline extraction",
        },
    },
    // -----------------------------------------------------------------------
    // CarveOut — test fixtures inside #[cfg(test)] blocks
    // -----------------------------------------------------------------------
    AllowEntry {
        file: "analysis/selection.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "analysis/symbols.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "completion.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "flow_map_action",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "flow_seq_action",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "apply_block_to_flow_edit",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "apply_block_scalar_edit",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "docs_for",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "editing/code_actions.rs",
        func: "flow_diags_for",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "hover.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "schema/association.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "schema_validation.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "schema_validation.rs",
        func: "run_content",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "schema_validation/formats.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "schema_validation/formats.rs",
        func: "run_format",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "validation/validators.rs",
        func: "parse_docs",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "validation/validators.rs",
        func: "parse_duplicate",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
    AllowEntry {
        file: "validation/validators.rs",
        func: "parse_yaml11",
        marker: AllowMarker::CarveOut {
            reason: "test fixture",
        },
    },
];

// ---------------------------------------------------------------------------
// Detection helpers (also exercised by unit tests below)
// ---------------------------------------------------------------------------

/// Returns `true` if `line` starts any function declaration: `(pub )?fn <name>`.
fn is_candidate_fn_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    Regex::new(r"^(?:pub\s+)?fn\s+\w")
        .unwrap()
        .is_match(trimmed)
}

/// Extracts the bare function name from a function declaration line.
///
/// Handles generics: `pub fn validate_foo<S>(` → `"validate_foo"`.
/// Works for both public and private functions.
fn extract_fn_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    // Strip optional `pub ` prefix
    let rest = trimmed
        .strip_prefix("pub ")
        .map_or(trimmed, |r| r.trim_start());
    let rest = rest.strip_prefix("fn ")?;
    // Name ends at `<`, `(`, or whitespace.
    let end = rest
        .find(|c: char| c == '<' || c == '(' || c.is_whitespace())
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() { None } else { Some(name) }
}

/// Returns `true` if the **first** named parameter of the function is one of
/// `{text, line, lines, content, source, input}` with type `&str` or `&[&str]`.
///
/// Uses an anchored regex on the extracted param block. An optional `&[mut ]self`
/// or `&'lt [mut ]self` method receiver is stripped first so that methods whose
/// first real parameter matches are still detected.
///
/// This preserves the first-parameter anchor: `code_actions(docs: &[…], text: &str, …)`
/// is NOT detected; `validate_foo(text: &str, …)` IS detected.
fn has_text_str_param(param_block: &str) -> bool {
    let re_receiver = Regex::new(r"^\s*&(?:'[a-z_]+\s+)?(?:mut\s+)?self\s*,\s*").unwrap();
    let stripped = re_receiver.replace(param_block, "");
    // Use &str\b for the scalar form to avoid matching &str_extra, and
    // &\[&str\] (no word boundary needed after ]) for the slice form.
    let re = Regex::new(r"^\s*(?:text|line|lines|content|source|input)\s*:\s*(?:&str\b|&\[&str\])")
        .unwrap();
    re.is_match(stripped.as_ref())
}

/// A detected violation: a function whose first parameter matches the text-handling shape.
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
            "BOUNDARY VIOLATION: {} new function(s) take a text-handling first parameter \
             outside the allow-list.\n\
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
// Unit tests for detection helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod detection_tests {
    use super::*;

    // --- Group A: is_candidate_fn_line broadening ---

    #[test]
    fn private_fn_any_name_detected() {
        let line = "fn validate_foo(text: &str) {";
        assert!(is_candidate_fn_line(line));
    }

    #[test]
    fn private_fn_arbitrary_name_detected() {
        let line = "fn apply_fix(text: &str) {";
        assert!(is_candidate_fn_line(line));
    }

    #[test]
    fn pub_fn_arbitrary_name_detected() {
        let line = "pub fn apply_fix(text: &str) {";
        assert!(is_candidate_fn_line(line));
    }

    #[test]
    fn generic_private_fn_detected() {
        let line = "fn foo<T>(";
        assert!(is_candidate_fn_line(line));
        assert_eq!(extract_fn_name(line), Some("foo"));
    }

    #[test]
    fn comment_line_not_detected() {
        let line = "// pub fn validate_foo(text: &str) {";
        assert!(!is_candidate_fn_line(line));
    }

    #[test]
    fn struct_def_not_detected() {
        let line = "pub struct Foo {";
        assert!(!is_candidate_fn_line(line));
    }

    #[test]
    fn let_binding_not_detected() {
        let line = r#"let fn_name = "validate_foo";"#;
        assert!(!is_candidate_fn_line(line));
    }

    // --- Group B: has_text_str_param — 6×2 positive matrix ---

    #[rstest::rstest]
    #[case::text_str("fn foo(text: &str, x: i32)")]
    #[case::line_str("fn foo(line: &str, x: i32)")]
    #[case::lines_str("fn foo(lines: &str, x: i32)")]
    #[case::content_str("fn foo(content: &str, x: i32)")]
    #[case::source_str("fn foo(source: &str, x: i32)")]
    #[case::input_str("fn foo(input: &str, x: i32)")]
    #[case::text_slice_str("fn foo(text: &[&str], x: i32)")]
    #[case::line_slice_str("fn foo(line: &[&str], x: i32)")]
    #[case::lines_slice_str("fn foo(lines: &[&str], x: i32)")]
    #[case::content_slice_str("fn foo(content: &[&str], x: i32)")]
    #[case::source_slice_str("fn foo(source: &[&str], x: i32)")]
    #[case::input_slice_str("fn foo(input: &[&str], x: i32)")]
    fn first_param_name_and_type_detected(#[case] sig: &str) {
        let params = extract_param_block(sig);
        assert!(has_text_str_param(&params), "expected detection for: {sig}");
    }

    // --- Group C: has_text_str_param — negative cases ---

    #[test]
    fn non_first_position_slice_str_not_detected() {
        let sig = "fn foo(docs: &[T], lines: &[&str])";
        let params = extract_param_block(sig);
        assert!(!has_text_str_param(&params));
    }

    #[test]
    fn differently_named_first_param_not_detected() {
        let sig = "fn foo(raw: &str, text: &str)";
        let params = extract_param_block(sig);
        assert!(!has_text_str_param(&params));
    }

    // After Task 2 the real code_actions signature starts with docs: &[Document<Span>] — not
    // text: &str — so the first-parameter check must NOT detect it as a violation.
    #[test]
    fn code_actions_new_signature_not_detected() {
        let line = "pub fn code_actions(docs: &[Document<Span>], text: &str, range: Range, diagnostics: &[Diagnostic], uri: &Url) -> Vec<CodeAction> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(
            !has_text_str_param(&params),
            "code_actions new signature must not be detected (text: &str is not the first param)"
        );
    }

    // --- Group D: receiver stripping for new param names ---

    #[rstest::rstest]
    #[case::self_then_line_str("fn foo(&self, line: &str)")]
    #[case::mut_self_then_lines_slice("fn foo(&mut self, lines: &[&str])")]
    #[case::lifetime_self_then_content_str("fn foo(&'a self, content: &str)")]
    #[case::self_then_source_str("fn foo(&self, source: &str)")]
    #[case::self_then_input_str("fn foo(&self, input: &str)")]
    fn method_receiver_stripped_for_new_names(#[case] sig: &str) {
        let params = extract_param_block(sig);
        assert!(
            has_text_str_param(&params),
            "expected detection after receiver strip for: {sig}"
        );
    }

    // --- Group E: AllowMarker display correctness ---

    #[test]
    fn allow_entry_todo_retrofit_display() {
        let entry = AllowEntry {
            file: "foo.rs",
            func: "bar",
            marker: AllowMarker::TodoRetrofit { plan: "plan-xyz" },
        };
        let s = format!("{entry}");
        assert!(s.contains("TodoRetrofit"), "missing TodoRetrofit in: {s}");
        assert!(s.contains("plan-xyz"), "missing plan-xyz in: {s}");
    }

    #[test]
    fn allow_entry_helper_of_display() {
        let entry = AllowEntry {
            file: "foo.rs",
            func: "helper",
            marker: AllowMarker::HelperOf {
                root: "validate_foo",
            },
        };
        let s = format!("{entry}");
        assert!(s.contains("HelperOf"), "missing HelperOf in: {s}");
        assert!(s.contains("validate_foo"), "missing validate_foo in: {s}");
    }

    #[test]
    fn allow_entry_carve_out_display() {
        let entry = AllowEntry {
            file: "foo.rs",
            func: "detect_bom",
            marker: AllowMarker::CarveOut {
                reason: "BOM detection",
            },
        };
        let s = format!("{entry}");
        assert!(s.contains("CarveOut"), "missing CarveOut in: {s}");
        assert!(s.contains("BOM detection"), "missing BOM detection in: {s}");
    }

    // --- Group F: allow-list suppression per marker kind ---

    #[test]
    fn todo_retrofit_entry_suppresses_violation() {
        let source = "pub fn validate_sentinel(text: &str) {}";
        let violations = scan_file("validation/validators.rs", source);
        assert_eq!(violations.len(), 1);

        let local_allow = AllowEntry {
            file: "validation/validators.rs",
            func: "validate_sentinel",
            marker: AllowMarker::TodoRetrofit { plan: "plan-test" },
        };
        let new_violations: Vec<&Violation> = violations
            .iter()
            .filter(|v| !(v.rel_path.ends_with(local_allow.file) && v.func == local_allow.func))
            .collect();
        assert!(
            new_violations.is_empty(),
            "TodoRetrofit entry should suppress violation: {new_violations:?}"
        );
    }

    #[test]
    fn helper_of_entry_suppresses_violation() {
        let source = "fn helper_for_validate(text: &str) {}";
        let violations = scan_file("validation/validators.rs", source);
        assert_eq!(violations.len(), 1);

        let local_allow = AllowEntry {
            file: "validation/validators.rs",
            func: "helper_for_validate",
            marker: AllowMarker::HelperOf {
                root: "validate_foo",
            },
        };
        let new_violations: Vec<&Violation> = violations
            .iter()
            .filter(|v| !(v.rel_path.ends_with(local_allow.file) && v.func == local_allow.func))
            .collect();
        assert!(
            new_violations.is_empty(),
            "HelperOf entry should suppress violation: {new_violations:?}"
        );
    }

    #[test]
    fn carve_out_entry_suppresses_violation() {
        let source = "pub fn detect_bom(input: &str) -> Option<usize> {}";
        let violations = scan_file("parser.rs", source);
        assert_eq!(violations.len(), 1);

        let local_allow = AllowEntry {
            file: "parser.rs",
            func: "detect_bom",
            marker: AllowMarker::CarveOut {
                reason: "BOM detection",
            },
        };
        let new_violations: Vec<&Violation> = violations
            .iter()
            .filter(|v| !(v.rel_path.ends_with(local_allow.file) && v.func == local_allow.func))
            .collect();
        assert!(
            new_violations.is_empty(),
            "CarveOut entry should suppress violation: {new_violations:?}"
        );
    }

    // --- Group G: dead entry detection with each marker kind ---

    #[test]
    fn dead_todo_retrofit_entry_flagged() {
        let source = "pub fn real_fn(text: &str) {}";
        let violations = scan_file("a.rs", source);

        let local_allow: &[(&str, &str)] = &[("a.rs", "real_fn"), ("a.rs", "validate_nonexistent")];
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

        assert!(
            dead.iter().any(|(_, func)| *func == "validate_nonexistent"),
            "dead TodoRetrofit entry should be flagged; got: {dead:?}"
        );
    }

    #[test]
    fn dead_helper_of_entry_flagged() {
        let source = "fn real_helper(lines: &[&str]) {}";
        let violations = scan_file("a.rs", source);

        let local_allow: &[(&str, &str)] = &[("a.rs", "real_helper"), ("a.rs", "helper_gone")];
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

        assert!(
            dead.iter().any(|(_, func)| *func == "helper_gone"),
            "dead HelperOf entry should be flagged; got: {dead:?}"
        );
    }

    #[test]
    fn dead_carve_out_entry_flagged() {
        let source = "fn real_fn(source: &str) {}";
        let violations = scan_file("a.rs", source);

        let local_allow: &[(&str, &str)] = &[("a.rs", "real_fn"), ("a.rs", "detect_bom_gone")];
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

        assert!(
            dead.iter().any(|(_, func)| *func == "detect_bom_gone"),
            "dead CarveOut entry should be flagged; got: {dead:?}"
        );
    }

    // --- Legacy tests (updated for new behavior) ---

    #[test]
    fn validate_foo_text_str_detected() {
        let line = "pub fn validate_foo(text: &str, docs: &[Document]) -> Vec<Diagnostic> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(has_text_str_param(&params));
    }

    #[test]
    fn second_param_text_str_not_detected() {
        let line = "pub fn validate_foo(docs: &[Document<Span>], text: &str) -> Vec<Diagnostic> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(!has_text_str_param(&params));
    }

    #[test]
    fn self_receiver_then_text_str_detected() {
        let line = "pub fn validate_foo(&self, text: &str) -> Vec<Diagnostic> {";
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

    // Old-style code_actions (text: &str as first param) IS still a violation.
    #[test]
    fn code_actions_old_signature_text_first_param_detected() {
        let line = "pub fn code_actions(text: &str, range: Range, diagnostics: &[Diagnostic]) -> Vec<CodeAction> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(
            has_text_str_param(&params),
            "old-style code_actions with text: &str as first param must be detected"
        );
    }

    #[test]
    fn non_text_named_str_param_not_detected() {
        let line = "pub fn validate_and_normalize_url(raw: &str) -> Option<String> {";
        assert!(is_candidate_fn_line(line));
        let params = extract_param_block(line);
        assert!(!has_text_str_param(&params));
    }

    #[test]
    fn generic_validate_fn_detected() {
        let line = "pub fn validate_custom_tags<S: std::hash::BuildHasher>(";
        let next = "    text: &str,";
        let combined = format!("{line} {next}");
        assert!(is_candidate_fn_line(line));
        let name = extract_fn_name(line).unwrap();
        assert_eq!(name, "validate_custom_tags");
        let params = extract_param_block(&combined);
        assert!(has_text_str_param(&params));
    }

    // --- Group B (allow-list mechanics) ---

    #[test]
    fn allowed_entry_suppresses_violation() {
        let source = "pub fn complete_at(text: &str) -> Vec<Diagnostic> {\n    vec![]\n}";
        let violations = scan_file("completion.rs", source);
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
        let source = "pub fn validate_real(docs: &[Document]) -> Vec<Diagnostic> {\n    vec![]\n}";
        let violations = scan_file("validation/validators.rs", source);

        let local_allow: &[(&str, &str)] = &[
            ("validation/validators.rs", "validate_real"),
            ("validation/validators.rs", "validate_nonexistent"),
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

        assert!(
            dead.iter().any(|(_, func)| *func == "validate_nonexistent"),
            "dead entry detection should flag validate_nonexistent; got: {dead:?}"
        );
    }
}
