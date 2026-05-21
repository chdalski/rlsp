// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::validation::ValidationSettings;

/// Validate duplicate mapping keys in YAML documents.
///
/// Returns error diagnostics for any key that appears more than once within
/// the same mapping. Operates on the parsed AST, which preserves all keys
/// even when duplicate.
///
/// Each document and each nested mapping is scoped independently.
#[must_use]
pub fn validate_duplicate_keys(
    docs: &[Document<Span>],
    settings: &ValidationSettings,
) -> Vec<Diagnostic> {
    let Some(severity) = settings.severity_for(crate::validation::DiagnosticCategory::DuplicateKey)
    else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    for doc in docs {
        let idx = doc.line_index();
        check_node_for_duplicate_keys(&doc.root, &mut diagnostics, severity, 0, idx);
    }
    diagnostics
}

/// Recursively walk a node and emit diagnostics for duplicate keys in each mapping.
fn check_node_for_duplicate_keys(
    node: &Node<Span>,
    diagnostics: &mut Vec<Diagnostic>,
    severity: DiagnosticSeverity,
    depth: usize,
    idx: &LineIndex,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Mapping { entries, .. } => {
            let mut seen: HashSet<String> = HashSet::new();
            for (key, value) in entries {
                let key_str_and_loc: Option<(String, &Span)> = match key {
                    Node::Scalar {
                        value: key_str,
                        loc,
                        ..
                    } => Some((key_str.clone(), loc)),
                    Node::Alias { name, loc, .. } => Some((format!("*{name}"), loc)),
                    Node::Mapping { .. } | Node::Sequence { .. } => None,
                };
                if let Some((key_str, loc)) = key_str_and_loc {
                    if seen.contains(&key_str) {
                        push_duplicate_diagnostic(diagnostics, &key_str, *loc, severity, idx);
                    } else {
                        seen.insert(key_str);
                    }
                }
                // Recurse into the key (e.g. complex keys) and value
                check_node_for_duplicate_keys(key, diagnostics, severity, depth + 1, idx);
                check_node_for_duplicate_keys(value, diagnostics, severity, depth + 1, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_node_for_duplicate_keys(item, diagnostics, severity, depth + 1, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// Push an error diagnostic for a duplicate scalar key.
fn push_duplicate_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    key: &str,
    loc: Span,
    severity: DiagnosticSeverity,
    idx: &LineIndex,
) {
    let display_key = if key.len() > 100 {
        let end = key.char_indices().nth(100).map_or(key.len(), |(i, _)| i);
        format!("{}...", &key[..end])
    } else {
        key.to_string()
    };
    let (start_line_1based, start_col) = idx.line_column(loc.start);
    let (_, end_col) = idx.line_column(loc.end);
    let start_line = start_line_1based.saturating_sub(1);
    diagnostics.push(Diagnostic {
        range: Range::new(
            Position::new(start_line, start_col),
            Position::new(start_line, end_col),
        ),
        severity: Some(severity),
        code: Some(NumberOrString::String("duplicateKey".to_string())),
        message: format!("Duplicate key: '{display_key}'"),
        source: Some("rlsp-yaml".to_string()),
        ..Diagnostic::default()
    });
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

    use super::*;
    use crate::validation::ValidationSettings;

    fn parse_duplicate(text: &str) -> Vec<Diagnostic> {
        let docs = rlsp_yaml_parser::load(text).unwrap();
        validate_duplicate_keys(&docs, &ValidationSettings::default())
    }

    // ---- Duplicate Key Validator: Happy Paths / Edge Cases / All no-dup groups ----

    #[rstest]
    #[case::no_duplicates("a: 1\nb: 2\nc: 3\n")]
    #[case::same_key_different_nesting_levels("name: top\nnested:\n  name: inner\n")]
    #[case::scope_reset_on_doc_boundary("key: 1\n---\nkey: 2\n")]
    #[case::no_flow_mapping_duplicates("cfg: {a: 1, b: 2}\n")]
    #[case::anchor_key_appearing_once("&anchor key: 1\nother: 2\n")]
    #[case::non_scalar_key_skipped("{a: 1}: foo\n{a: 1}: bar\n")]
    #[case::single_alias_key("x: &anchor foo\n? *anchor\n: 1\nother: 2\n")]
    #[case::empty_document("")]
    #[case::comment_only("# just a comment\n")]
    #[case::same_key_different_sequence_items("items:\n  - name: alice\n  - name: bob\n")]
    #[case::sibling_mappings_under_common_parent(
        "parent:\n  child_a:\n    cpu: 100m\n    memory: 128Mi\n  child_b:\n    cpu: 200m\n    memory: 256Mi\n"
    )]
    #[case::deeply_nested_sibling_mappings(
        "level1:\n  level2:\n    sibling_a:\n      value: 1\n    sibling_b:\n      value: 2\n"
    )]
    #[case::empty_sibling_with_shared_key_in_later(
        "parent:\n  a: ~\n  b:\n    cpu: 1\n  c:\n    cpu: 2\n"
    )]
    #[case::mixed_indent_depth_siblings(
        "resources:\n  requests:\n    cpu: 100m\n  limits:\n    cpu: 500m\n"
    )]
    #[case::ellipsis_resets_scope("key: 1\n...\nkey: 2\n")]
    fn duplicate_keys_returns_empty(#[case] input: &str) {
        let result = parse_duplicate(input);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::simple_top_level("a: 1\na: 2\n", 1)]
    #[case::nested_mapping("outer:\n  x: 1\n  x: 2\n", 1)]
    #[case::within_same_doc_in_multi_doc("a: 1\na: 2\n---\nb: 3\n", 1)]
    #[case::flow_mapping_duplicate("cfg: {x: 1, x: 2}\n", 1)]
    #[case::double_quoted_and_unquoted("\"key\": 1\nkey: 2\n", 1)]
    #[case::two_double_quoted("\"key\": 1\n\"key\": 2\n", 1)]
    #[case::single_quoted_and_unquoted("'key': 1\nkey: 2\n", 1)]
    #[case::single_and_double_quoted("'key': 1\n\"key\": 2\n", 1)]
    #[case::second_key_has_anchor("key: 1\n&anchor key: 2\n", 1)]
    #[case::first_key_has_anchor("&anchor key: 1\nkey: 2\n", 1)]
    #[case::empty_string_keys("\"\": 1\n\"\": 2\n", 1)]
    #[case::unicode_keys("café: 1\ncafé: 2\n", 1)]
    #[case::within_same_sequence_item("items:\n  - name: alice\n    name: alice2\n", 1)]
    #[case::same_duplicate_within_one_sibling(
        "parent:\n  child:\n    cpu: 100m\n    cpu: 200m\n",
        1
    )]
    #[case::duplicate_before_ellipsis("a: 1\na: 2\n...\nb: 3\n", 1)]
    #[case::triple_duplicate_two_diags("parent:\n  child:\n    x: 1\n    x: 2\n    x: 3\n", 2)]
    fn duplicate_keys_count(#[case] input: &str, #[case] expected: usize) {
        let result = parse_duplicate(input);

        assert_eq!(result.len(), expected);
    }

    // ---- Duplicate Key Validator: Error severity ----

    #[rstest]
    #[case::simple_top_level("a: 1\na: 2\n")]
    #[case::flow_mapping_duplicate("cfg: {x: 1, x: 2}\n")]
    #[case::empty_string_keys("\"\": 1\n\"\": 2\n")]
    #[case::unicode_keys("café: 1\ncafé: 2\n")]
    fn duplicate_key_error_severity(#[case] input: &str) {
        let result = parse_duplicate(input);

        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    // ---- Duplicate Key Validator: standalone ----

    #[test]
    fn should_detect_simple_top_level_duplicate() {
        let text = "a: 1\na: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "duplicateKey")
        );
        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
        assert!(result[0].message.contains("'a'"));
        assert_eq!(result[0].range.start.line, 1, "duplicate is on line 1");
    }

    #[test]
    fn should_detect_duplicate_in_nested_mapping() {
        let text = "outer:\n  x: 1\n  x: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'x'"));
        assert_eq!(result[0].range.start.line, 2);
    }

    #[test]
    fn should_detect_duplicate_within_same_document_in_multi_doc_yaml() {
        let text = "a: 1\na: 2\n---\nb: 3\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'a'"));
    }

    #[test]
    fn should_detect_flow_mapping_duplicate() {
        let text = "cfg: {x: 1, x: 2}\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("'x'"));
    }

    #[test]
    fn should_detect_duplicate_alias_keys() {
        // *ref used as a mapping key twice
        let text = "x: &anchor foo\n? *anchor\n: 1\n? *anchor\n: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "duplicateKey")
        );
        assert!(result[0].message.contains("*anchor"));
    }

    #[test]
    fn should_detect_duplicate_empty_string_keys() {
        let text = "\"\": 1\n\"\": 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "duplicateKey")
        );
    }

    #[test]
    fn should_detect_duplicate_unicode_keys() {
        let text = "café: 1\ncafé: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(result[0].message.contains("café"));
    }

    #[test]
    fn should_detect_duplicate_within_same_sequence_item() {
        let text = "items:\n  - name: alice\n    name: alice2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'name'"));
    }

    #[test]
    fn should_not_flag_kubernetes_limitrange_sibling_pattern() {
        let text = "\
limits:
  max:
    cpu: \"2\"
    memory: 1Gi
  min:
    cpu: 100m
    memory: 128Mi
  default:
    cpu: 500m
    memory: 512Mi
  defaultRequest:
    cpu: 250m
    memory: 256Mi
";
        let result = parse_duplicate(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_not_flag_kubernetes_limitrange_inside_sequence_item() {
        let text = "\
spec:
  limits:
    - type: Container
      max:
        cpu: \"2\"
        memory: 1Gi
      min:
        cpu: 100m
        memory: 128Mi
      default:
        cpu: 500m
        memory: 512Mi
      defaultRequest:
        cpu: 250m
        memory: 256Mi
";
        let result = parse_duplicate(text);

        assert!(result.is_empty());
    }

    #[test]
    fn should_still_detect_duplicate_in_same_sibling_mapping() {
        let text = "parent:\n  child:\n    cpu: 100m\n    cpu: 200m\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'cpu'"));
        assert_eq!(result[0].range.start.line, 3);
    }

    #[test]
    fn should_detect_triple_duplicate_within_single_sibling_mapping() {
        let text = "parent:\n  child:\n    x: 1\n    x: 2\n    x: 3\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|d| d.message.contains("'x'")));
    }

    #[test]
    fn should_truncate_long_key_name_in_message() {
        let long_key = "k".repeat(110);
        let text = format!("{long_key}: 1\n{long_key}: 2\n");
        let result = parse_duplicate(&text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("..."));
        let display = &result[0].message;
        assert!(display.len() < long_key.len() + 20);
    }

    #[test]
    fn should_report_correct_column_for_indented_duplicate_key() {
        let text = "outer:\n  inner:\n    dup: 1\n    dup: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].range.start.line, 3,
            "duplicate is on line 3 (0-based)"
        );
        assert_eq!(
            result[0].range.start.character, 4,
            "exact column from AST loc, not indent approximation"
        );
    }

    #[test]
    fn should_report_correct_range_end_for_duplicate_key() {
        let text = "abc: 1\nabc: 2\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.character, 0);
        assert_eq!(
            result[0].range.end.character,
            result[0].range.start.character + 3,
            "end column = start + key length"
        );
    }

    #[test]
    fn duplicate_key_detected_before_ellipsis_terminator() {
        let text = "a: 1\na: 2\n...\nb: 3\n";
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("'a'"));
    }

    // ---- Duplicate Key Validator: severity propagation ----

    #[test]
    fn validate_duplicate_keys_returns_empty_when_disabled() {
        let settings = ValidationSettings {
            duplicate_keys: None,
            flow_style: None,
        };
        let docs = rlsp_yaml_parser::load("a: 1\na: 2\n").unwrap();
        assert!(validate_duplicate_keys(&docs, &settings).is_empty());
    }

    #[test]
    fn validate_duplicate_keys_emits_warning_severity_when_configured() {
        let settings = ValidationSettings {
            duplicate_keys: Some(DiagnosticSeverity::WARNING),
            flow_style: None,
        };
        let docs = rlsp_yaml_parser::load("a: 1\na: 2\n").unwrap();
        let result = validate_duplicate_keys(&docs, &settings);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn validate_duplicate_keys_emits_error_severity_by_default() {
        let docs = rlsp_yaml_parser::load("a: 1\na: 2\n").unwrap();
        let result = validate_duplicate_keys(&docs, &ValidationSettings::default());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn validate_duplicate_keys_propagates_warning_to_all_diagnostics() {
        let settings = ValidationSettings {
            duplicate_keys: Some(DiagnosticSeverity::WARNING),
            flow_style: None,
        };
        let docs =
            rlsp_yaml_parser::load("parent:\n  child:\n    x: 1\n    x: 2\n    x: 3\n").unwrap();
        let result = validate_duplicate_keys(&docs, &settings);
        assert_eq!(result.len(), 2);
        for diag in &result {
            assert_eq!(diag.severity, Some(DiagnosticSeverity::WARNING));
        }
    }
}
