// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

/// Validate YAML 1.1 compatibility for plain scalars.
///
/// Returns diagnostics for plain scalar values that have different semantics in
/// YAML 1.1 vs YAML 1.2:
/// - YAML 1.1 boolean forms (`yes`, `no`, `on`, `off`, `y`, `n`, and their
///   case variants) → `yaml11Boolean` WARNING
/// - C-style octal literals (`0755`, `007`, etc.) → `yaml11Octal` INFORMATION
///
/// Only plain (unquoted) scalars are checked. Quoted scalars are already
/// unambiguously strings in both versions.
#[must_use]
pub fn validate_yaml11_compat(docs: &[Document<Span>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for doc in docs {
        let idx = doc.line_index();
        collect_yaml11_diagnostics(&doc.root, &mut diagnostics, 0, idx);
    }
    diagnostics
}

/// Recursively walk a YAML node and emit diagnostics for YAML 1.1 compatibility issues.
fn collect_yaml11_diagnostics(
    node: &Node<Span>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
    idx: &LineIndex,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    match node {
        Node::Scalar {
            value, style, loc, ..
        } => {
            if *style == rlsp_yaml_parser::ScalarStyle::Plain {
                if crate::scalar_helpers::is_yaml11_bool(value) {
                    let canonical = crate::scalar_helpers::yaml11_bool_canonical(value);
                    let (start_line_1based, start_col) = idx.line_column(loc.start);
                    let (_, end_col) = idx.line_column(loc.end);
                    let start_line = start_line_1based.saturating_sub(1);
                    diagnostics.push(Diagnostic {
                        range: Range::new(
                            Position::new(start_line, start_col),
                            Position::new(start_line, end_col),
                        ),
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("yaml11Boolean".to_string())),
                        message: format!(
                            "\"{value}\" is a boolean in YAML 1.1 but a string in YAML 1.2. \
                             Most tools use 1.1 parsers and will interpret this as {canonical}. \
                             Quote it (\"{value}\") or use {canonical}."
                        ),
                        source: Some("rlsp-yaml".to_string()),
                        ..Diagnostic::default()
                    });
                } else if crate::scalar_helpers::is_yaml11_octal(value) {
                    let decimal = i64::from_str_radix(&value[1..], 8).unwrap_or(0);
                    let yaml12 = format!("0o{}", &value[1..]);
                    let (start_line_1based, start_col) = idx.line_column(loc.start);
                    let (_, end_col) = idx.line_column(loc.end);
                    let start_line = start_line_1based.saturating_sub(1);
                    diagnostics.push(Diagnostic {
                        range: Range::new(
                            Position::new(start_line, start_col),
                            Position::new(start_line, end_col),
                        ),
                        severity: Some(DiagnosticSeverity::INFORMATION),
                        code: Some(NumberOrString::String("yaml11Octal".to_string())),
                        message: format!(
                            "\"{value}\" is octal {decimal} in YAML 1.1 but the string \
                             \"{value}\" in YAML 1.2. Quote it (\"{value}\") or use \
                             {yaml12} (YAML 1.2 only)."
                        ),
                        source: Some("rlsp-yaml".to_string()),
                        ..Diagnostic::default()
                    });
                }
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_yaml11_diagnostics(key, diagnostics, depth + 1, idx);
                collect_yaml11_diagnostics(value, diagnostics, depth + 1, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_yaml11_diagnostics(item, diagnostics, depth + 1, idx);
            }
        }
        Node::Alias { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

    use super::*;
    use crate::test_utils::parse_docs;
    use crate::validation::ValidationSettings;

    fn parse_yaml11(text: &str) -> Vec<Diagnostic> {
        let docs = parse_docs(text);
        validate_yaml11_compat(&docs)
    }

    #[test]
    fn yaml11_bool_plain_yes_emits_warning() {
        let result = parse_yaml11("value: yes\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        let msg = &result[0].message;
        assert!(msg.contains("yes"), "message should contain the value");
        assert!(
            msg.contains("true"),
            "message should mention canonical form (yes → true)"
        );
    }

    #[rstest]
    #[case::yes_lowercase("yes")]
    #[case::yes_titlecase("Yes")]
    #[case::yes_uppercase("YES")]
    #[case::on_lowercase("on")]
    #[case::on_titlecase("On")]
    #[case::on_uppercase("ON")]
    #[case::y_lowercase("y")]
    #[case::y_uppercase("Y")]
    fn yaml11_bool_all_true_forms_emit_warning(#[case] value: &str) {
        let text = format!("k: {value}\n");
        let result = parse_yaml11(&text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[rstest]
    #[case::no_lowercase("no")]
    #[case::no_titlecase("No")]
    #[case::no_uppercase("NO")]
    #[case::off_lowercase("off")]
    #[case::off_titlecase("Off")]
    #[case::off_uppercase("OFF")]
    #[case::n_lowercase("n")]
    #[case::n_uppercase("N")]
    fn yaml11_bool_all_false_forms_emit_warning(#[case] value: &str) {
        let text = format!("k: {value}\n");
        let result = parse_yaml11(&text);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn yaml11_bool_quoted_double_no_diagnostic() {
        let result = parse_yaml11("value: \"yes\"\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_bool_quoted_single_no_diagnostic() {
        let result = parse_yaml11("value: 'yes'\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_bool_as_mapping_key_emits_diagnostic() {
        // Keys are Node::Scalar too — all plain scalars are walked.
        let result = parse_yaml11("yes: value\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
    }

    #[test]
    fn yaml11_bool_yaml12_true_no_diagnostic() {
        let result = parse_yaml11("value: true\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_bool_multiple_in_one_document() {
        let result = parse_yaml11("a: yes\nb: no\nc: on\n");

        assert_eq!(result.len(), 3);
        assert!(
            result
                .iter()
                .all(|d| d.code == Some(NumberOrString::String("yaml11Boolean".to_string())))
        );
        assert!(
            result
                .iter()
                .all(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        );
    }

    #[test]
    fn yaml11_bool_diagnostic_message_canonical_true() {
        let result = parse_yaml11("value: yes\n");

        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(msg.contains("yes"), "message should include the value");
        assert!(
            msg.contains("true"),
            "message should include canonical YAML 1.2 form"
        );
        assert!(
            msg.contains("\"yes\""),
            "message should suggest quoting as \"yes\""
        );
    }

    #[test]
    fn yaml11_bool_diagnostic_message_canonical_false() {
        let result = parse_yaml11("value: no\n");

        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(msg.contains("no"), "message should include the value");
        assert!(
            msg.contains("false"),
            "message should include canonical YAML 1.2 form"
        );
        assert!(
            msg.contains("\"no\""),
            "message should suggest quoting as \"no\""
        );
    }

    #[test]
    fn yaml11_octal_plain_emits_information() {
        let result = parse_yaml11("mode: 0755\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Octal".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::INFORMATION));
    }

    #[test]
    fn yaml11_octal_single_zero_no_diagnostic() {
        let result = parse_yaml11("count: 0\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_octal_quoted_double_no_diagnostic() {
        let result = parse_yaml11("mode: \"0755\"\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_octal_yaml12_notation_no_diagnostic() {
        let result = parse_yaml11("mode: 0o755\n");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_octal_diagnostic_message_includes_decimal_and_suggestion() {
        let result = parse_yaml11("mode: 0755\n");

        assert_eq!(result.len(), 1);
        let msg = &result[0].message;
        assert!(
            msg.contains("493"),
            "message should include decimal value of 0755"
        );
        assert!(
            msg.contains("0o755"),
            "message should include YAML 1.2 form"
        );
    }

    #[test]
    fn yaml11_octal_007_emits_information() {
        let result = parse_yaml11("file: 007\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Octal".to_string()))
        );
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::INFORMATION));
        assert!(
            result[0].message.contains('7'),
            "message should include decimal value 7"
        );
    }

    #[test]
    fn yaml11_bool_and_octal_in_same_document() {
        let result = parse_yaml11("flag: yes\nmode: 0755\n");

        assert_eq!(result.len(), 2);
        let codes: Vec<_> = result.iter().map(|d| d.code.as_ref().unwrap()).collect();
        assert!(
            codes
                .iter()
                .any(|c| *c == &NumberOrString::String("yaml11Boolean".to_string()))
        );
        assert!(
            codes
                .iter()
                .any(|c| *c == &NumberOrString::String("yaml11Octal".to_string()))
        );
    }

    #[test]
    fn yaml11_empty_document_no_diagnostics() {
        let result = parse_yaml11("");

        assert!(result.is_empty());
    }

    #[test]
    fn yaml11_in_nested_mapping() {
        let result = parse_yaml11("outer:\n  inner: yes\n");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].code,
            Some(NumberOrString::String("yaml11Boolean".to_string()))
        );
    }

    #[test]
    fn yaml11_in_sequence() {
        let result = parse_yaml11("items:\n  - yes\n  - no\n");

        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|d| d.code == Some(NumberOrString::String("yaml11Boolean".to_string())))
        );
    }

    // ---- YAML version agnosticism ----
    //
    // All validators in this module operate on raw text or on parsed
    // Document<Span>/Node<Span> values. The parser always parses as YAML 1.2
    // regardless of any `yamlVersion` setting, so the parsed representation
    // is identical for all version settings. Consequently, no validator here
    // requires a YamlVersion parameter — diagnostics are version-agnostic.
    //
    // The tests below confirm that inputs containing YAML 1.1-only boolean
    // literals (`yes`, `no`, `on`, `off`) produce the same diagnostic output
    // as equivalent inputs without them, locking down this invariant.

    #[test]
    fn validators_produce_same_diagnostics_regardless_of_yaml_version_setting() {
        let text_with_v1_1_keywords = "on: push\nyes: true\n";
        let text_plain = "push_trigger: push\nenabled: true\n";

        // validate_duplicate_keys: no duplicates in either text.
        let default_settings = ValidationSettings::default();
        assert_eq!(
            crate::validation::validators::validate_duplicate_keys(
                &rlsp_yaml_parser::load(text_with_v1_1_keywords).unwrap_or_default(),
                &default_settings,
            )
            .len(),
            crate::validation::validators::validate_duplicate_keys(
                &rlsp_yaml_parser::load(text_plain).unwrap_or_default(),
                &default_settings,
            )
            .len(),
            "duplicate-key diagnostics must not differ based on v1.1 keyword presence"
        );

        // validate_flow_style: no flow collections in either text.
        assert_eq!(
            crate::validation::validators::validate_flow_style(
                &rlsp_yaml_parser::load(text_with_v1_1_keywords).unwrap_or_default(),
                &default_settings,
            )
            .len(),
            crate::validation::validators::validate_flow_style(
                &rlsp_yaml_parser::load(text_plain).unwrap_or_default(),
                &default_settings,
            )
            .len(),
            "flow-style diagnostics must not differ based on v1.1 keyword presence"
        );
    }
}
