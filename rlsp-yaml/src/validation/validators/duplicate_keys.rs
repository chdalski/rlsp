// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

use super::truncate_name;
use crate::lsp_util::span_to_lsp;
use crate::validation::ValidationSettings;

/// Validate duplicate mapping keys in YAML documents.
///
/// Returns error diagnostics for any key that appears more than once within
/// the same mapping. Operates on the parsed AST, which preserves all keys
/// even when duplicate.
///
/// Each document and each nested mapping is scoped independently.
///
/// Walks the whole tree, so its stack use follows the nesting depth, which the
/// loader bounds. Pass documents from a nesting-limited, non-`Resolved` load.
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
        check_node_for_duplicate_keys(&doc.root, &mut diagnostics, severity, idx);
    }
    diagnostics
}

/// Recursively walk a node and emit diagnostics for duplicate keys in each mapping.
fn check_node_for_duplicate_keys(
    node: &Node<Span>,
    diagnostics: &mut Vec<Diagnostic>,
    severity: DiagnosticSeverity,
    idx: &LineIndex,
) {
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
                check_node_for_duplicate_keys(key, diagnostics, severity, idx);
                check_node_for_duplicate_keys(value, diagnostics, severity, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_node_for_duplicate_keys(item, diagnostics, severity, idx);
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
    let display_key = truncate_name(key);
    diagnostics.push(Diagnostic {
        range: span_to_lsp(loc, idx),
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
    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString, Position, Range};

    use super::*;
    use crate::test_utils::{assert_loads, nested_flow, staircase, text_of};
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

    #[rstest]
    #[case::ascii_100("k".repeat(100), "k".repeat(100))]
    #[case::ascii_101("k".repeat(101), format!("{}...", "k".repeat(100)))]
    #[case::ascii_110("k".repeat(110), format!("{}...", "k".repeat(100)))]
    #[case::multibyte_within_100_chars("é".repeat(60), "é".repeat(60))]
    #[case::multibyte_beyond_100_chars("é".repeat(150), format!("{}...", "é".repeat(100)))]
    fn duplicate_key_message_truncation(#[case] key: String, #[case] shown: String) {
        let result = parse_duplicate(&format!("{key}: 1\n{key}: 2\n"));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, format!("Duplicate key: '{shown}'"));
    }

    #[rstest]
    #[case::plain_single_line("abc: 1\nabc: 2\n", (1, 0), (1, 3))]
    #[case::indented_single_line("outer:\n  inner:\n    dup: 1\n    dup: 2\n", (3, 4), (3, 7))]
    #[case::flow_mapping("{a: 1, a: 2}\n", (0, 7), (0, 8))]
    #[case::double_quoted_single_line("k: 1\n\"k\": 2\n", (1, 0), (1, 3))]
    #[case::empty_quoted_key("\"\": 1\n\"\": 2\n", (1, 0), (1, 2))]
    #[case::alias_key("x: &an 1\n? *an\n: 1\n? *an\n: 2\n", (3, 2), (3, 5))]
    #[case::double_quoted_multiline("\"a b\": 1\n? \"a\n  b\"\n: 2\n", (1, 2), (2, 4))]
    #[case::plain_multiline("? a\n  b\n: 1\n? a\n  b\n: 2\n", (3, 2), (4, 3))]
    #[case::single_quoted_multiline_blank_line(
        "? 'a\n\n  b'\n: 1\n? 'a\n\n  b'\n: 2\n",
        (4, 2),
        (6, 4)
    )]
    #[case::double_quoted_escaped_line_break("k: 1\n? \"k\\\n   \"\n: 2\n", (1, 2), (2, 4))]
    #[case::literal_block_scalar_key("? |\n  a\n: 1\n? |\n  a\n: 2\n", (3, 2), (5, 0))]
    #[case::crlf_multiline("\"a b\": 1\r\n? \"a\r\n  b\"\r\n: 2\r\n", (1, 2), (2, 4))]
    #[case::cr_multiline("? a\r  b\r: 1\r? a\r  b\r: 2\r", (3, 2), (4, 3))]
    #[case::astral_single_line("\"\u{1F600}\": 1\n\"\u{1F600}\": 2\n", (1, 0), (1, 3))]
    #[case::astral_multiline(
        "\"\u{1F600} \u{1F600}\": 1\n? \"\u{1F600}\n  \u{1F600}\"\n: 2\n",
        (1, 2),
        (2, 4)
    )]
    fn duplicate_key_range(#[case] text: &str, #[case] start: (u32, u32), #[case] end: (u32, u32)) {
        let result = parse_duplicate(text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].range,
            Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1))
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

    // ---- Duplicate Key Validator: Nesting Depth ----
    //
    // The validator reads whole documents: a mapping is checked wherever it
    // sits.  `d` is the number of collections around the mapping.

    fn duplicate_in_flow(d: usize) -> String {
        format!("a: {}\n", nested_flow(d, "{x: 1, x: 2}"))
    }

    fn duplicate_in_block_mapping(d: usize) -> String {
        let indent = "  ".repeat(d);
        format!("{}{indent}x: 1\n{indent}x: 2\n", staircase(d, "k:"))
    }

    fn duplicate_in_block_sequence(d: usize) -> String {
        let indent = "  ".repeat(d);
        format!("{}{indent}- x: 1\n{indent}  x: 2\n", staircase(d, "-"))
    }

    #[rstest]
    #[case::flow(duplicate_in_flow, 0)]
    #[case::block_mappings(duplicate_in_block_mapping, 1)]
    #[case::block_sequences(duplicate_in_block_sequence, 1)]
    fn a_duplicate_key_at_any_depth_is_reported(
        #[case] build: fn(usize) -> String,
        #[case] line_offset: usize,
        #[values(0, 99, 100, 101, 254)] d: usize,
    ) {
        let yaml = build(d);
        assert_loads(&yaml);
        let result = parse_duplicate(&yaml);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Duplicate key: 'x'");
        let line = if line_offset == 0 { 0 } else { d + 1 };
        assert_eq!(result[0].range.start.line, u32::try_from(line).unwrap());
        assert_eq!(text_of(&yaml, result[0].range), "x");
    }

    #[test]
    fn a_duplicate_key_inside_a_complex_key_at_depth_is_reported() {
        let indent = "  ".repeat(101);
        let yaml = format!(
            "{}{indent}? {{x: 1, x: 2}}\n{indent}: v\n",
            staircase(101, "k:")
        );
        assert_loads(&yaml);
        let result = parse_duplicate(&yaml);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Duplicate key: 'x'");
    }
}
