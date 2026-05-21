// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{CollectionStyle, LineIndex, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::validation::{DiagnosticCategory, ValidationSettings};

/// Validate flow style usage in YAML documents.
///
/// Returns diagnostics for:
/// - Flow mappings (`{...}`) with code `flowMap`
/// - Flow sequences (`[...]`) with code `flowSeq`
///
/// Severity is taken from `settings`; returns an empty vec when
/// `settings.severity_for(FlowStyle)` is `None` (disabled). Empty collections
/// (`{}`, `[]`) produce no diagnostic. Uses the parser AST so plain scalars
/// containing `{`/`[` (e.g. `${{ env.VAR }}`) are never false-flagged.
#[must_use]
pub fn validate_flow_style(
    docs: &[Document<Span>],
    settings: &ValidationSettings,
) -> Vec<Diagnostic> {
    let Some(severity) = settings.severity_for(DiagnosticCategory::FlowStyle) else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    for doc in docs {
        let idx = doc.line_index();
        collect_flow_style_diagnostics(&doc.root, &mut diagnostics, severity, 0, idx);
    }
    diagnostics
}

/// Recursively walk a node and emit diagnostics for non-empty flow collections.
fn collect_flow_style_diagnostics(
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
        Node::Mapping {
            style: CollectionStyle::Flow,
            entries,
            loc,
            ..
        } if !entries.is_empty() => {
            diagnostics.push(flow_diagnostic(
                "flowMap",
                "Flow mapping style: use block style instead",
                severity,
                *loc,
                idx,
            ));
            for (key, value) in entries {
                collect_flow_style_diagnostics(key, diagnostics, severity, depth + 1, idx);
                collect_flow_style_diagnostics(value, diagnostics, severity, depth + 1, idx);
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_flow_style_diagnostics(key, diagnostics, severity, depth + 1, idx);
                collect_flow_style_diagnostics(value, diagnostics, severity, depth + 1, idx);
            }
        }
        Node::Sequence {
            style: CollectionStyle::Flow,
            items,
            loc,
            ..
        } if !items.is_empty() => {
            diagnostics.push(flow_diagnostic(
                "flowSeq",
                "Flow sequence style: use block style instead",
                severity,
                *loc,
                idx,
            ));
            for item in items {
                collect_flow_style_diagnostics(item, diagnostics, severity, depth + 1, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_flow_style_diagnostics(item, diagnostics, severity, depth + 1, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

fn flow_diagnostic(
    code: &str,
    message: &str,
    severity: DiagnosticSeverity,
    loc: Span,
    idx: &LineIndex,
) -> Diagnostic {
    let (start_line_1based, start_col) = idx.line_column(loc.start);
    // The AST end span is at the closing `}` or `]` character (zero-width span).
    // Add 1 so the LSP range end is exclusive — past the delimiter — which
    // lets flow_map_to_block/flow_seq_to_block extract the full `{...}` slice.
    let (end_line_1based, end_col) = idx.line_column(loc.end);
    Diagnostic {
        range: Range::new(
            Position::new(start_line_1based.saturating_sub(1), start_col),
            Position::new(end_line_1based.saturating_sub(1), end_col + 1),
        ),
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        message: message.to_string(),
        source: Some("rlsp-yaml".to_string()),
        ..Diagnostic::default()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    use super::*;
    use crate::test_utils::parse_docs;
    use crate::validation::ValidationSettings;

    // ---- Flow Style Validator: Happy Paths / Edge Cases / Empty Collections ----

    #[rstest]
    #[case::block_only("key:\n  nested: value\n")]
    #[case::empty_document("")]
    #[case::brackets_in_double_quotes("message: \"array is [1,2,3]\"\n")]
    #[case::braces_in_single_quotes("message: 'object is {a: 1}'\n")]
    #[case::empty_flow_mapping("status: {}\n")]
    #[case::empty_flow_sequence("items: []\n")]
    #[case::flow_mapping_spaces_only("status: { }\n")]
    #[case::flow_mapping_multiple_spaces("status: {  }\n")]
    #[case::flow_sequence_spaces_only("items: [  ]\n")]
    #[case::multiple_empty_collections_one_line("a: {}\nb: []\n")]
    #[case::braces_inside_single_quoted_string("msg: 'value with {braces}'\n")]
    fn flow_style_returns_empty(#[case] input: &str) {
        let docs = parse_docs(input);
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::flow_mapping("config: {key: value}\n", 1)]
    #[case::flow_sequence("items: [one, two, three]\n", 1)]
    #[case::both_types_on_two_lines("config: {key: value}\nitems: [a, b]\n", 2)]
    #[case::nested_flow_styles("data: {outer: [inner]}\n", 2)]
    #[case::multi_document("doc1: {a: 1}\n---\ndoc2: [x]\n", 2)]
    #[case::outer_nonempty_inner_empty("data: {a: {}}\n", 1)]
    #[case::mixed_empty_nonempty("a: {}\nb: {x: 1}\n", 1)]
    #[case::flow_detected_after_single_quote_ends("msg: 'quoted' \nreal: {a: 1}\n", 1)]
    fn flow_style_count(#[case] input: &str, #[case] expected: usize) {
        let docs = parse_docs(input);
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), expected);
    }

    #[rstest]
    #[case::flow_mapping("config: {key: value}\n")]
    #[case::flow_sequence("items: [a, b]\n")]
    fn flow_style_range_start_line_zero(#[case] input: &str) {
        let docs = parse_docs(input);
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 0);
    }

    // ---- Flow Style Validator: standalone ----

    #[test]
    fn should_detect_flow_mapping() {
        let docs = parse_docs("config: {key: value}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn should_detect_flow_sequence() {
        let docs = parse_docs("items: [one, two, three]\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowSeq")
        );
    }

    #[test]
    fn should_detect_both_flow_mapping_and_sequence() {
        let docs = parse_docs("config: {key: value}\nitems: [a, b]\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 2);
        let has_flow_map = result
            .iter()
            .any(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap"));
        let has_flow_seq = result
            .iter()
            .any(|d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "flowSeq"));
        assert!(has_flow_map);
        assert!(has_flow_seq);
    }

    #[test]
    fn should_warn_on_outer_but_not_inner_empty_flow_mapping() {
        // Outer `{a: {}}` is non-empty → warns; inner `{}` is empty → no extra warn.
        let docs = parse_docs("data: {a: {}}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn should_warn_only_on_non_empty_when_mixed_with_empty() {
        let docs = parse_docs("a: {}\nb: {x: 1}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1);
    }

    // ---- Flow Style Validator: API contract — diagnostic field identity ----

    #[test]
    fn flow_map_diagnostic_message_text() {
        let docs = parse_docs("config: {key: value}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(
            result[0].message,
            "Flow mapping style: use block style instead"
        );
    }

    #[test]
    fn flow_seq_diagnostic_message_text() {
        let docs = parse_docs("items: [a, b]\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(
            result[0].message,
            "Flow sequence style: use block style instead"
        );
    }

    #[test]
    fn flow_map_diagnostic_source() {
        let docs = parse_docs("config: {key: value}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    #[test]
    fn flow_seq_diagnostic_source() {
        let docs = parse_docs("items: [a, b]\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    // ---- Flow Style Validator: GHA-style plain scalar expressions ----

    #[test]
    fn gha_expression_in_plain_scalar_no_diagnostic() {
        // `${{ … }}` is a plain scalar in block context — AST does not see a flow mapping.
        let docs = parse_docs("token: ${{ secrets.GITHUB_TOKEN }}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert!(result.is_empty());
    }

    #[test]
    fn gha_expression_double_brace_no_diagnostic() {
        let docs = parse_docs("run: echo ${{ env.MY_VAR }}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert!(result.is_empty());
    }

    #[test]
    fn gha_expression_nested_no_diagnostic() {
        let docs = parse_docs("env:\n  TOKEN: ${{ secrets.TOKEN }}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert!(result.is_empty());
    }

    #[test]
    fn gha_expression_alongside_real_flow_map() {
        // GHA expression line: zero diagnostics; real flow map line: one diagnostic.
        let docs = parse_docs("token: ${{ secrets.TOKEN }}\nconfig: {key: value}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    // ---- Flow Style Validator: multi-line flow collections ----

    #[test]
    fn multiline_flow_map_detected() {
        // Current text scanner misses multi-line flow maps; AST walk finds them.
        // Closing `}` must be indented >= the key column per YAML 1.2 flow rules.
        let docs = parse_docs("foo: {\n       a: 1,\n     }\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    #[test]
    fn multiline_flow_seq_detected() {
        let docs = parse_docs("items: [\n         a,\n         b,\n       ]\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowSeq")
        );
    }

    #[test]
    fn multiline_flow_map_range_starts_on_opening_line() {
        let docs = parse_docs("foo: {\n       a: 1,\n     }\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result[0].range.start.line, 0);
    }

    // ---- Flow Style Validator: no double-reporting ----

    #[test]
    fn nested_nonempty_flow_maps_no_double_report() {
        // outer {outer: {inner: 1}} → 2 diagnostics (one each), not more.
        let docs = parse_docs("data: {outer: {inner: 1}}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 2);
        assert!(
            result.iter().all(
                |d| matches!(d.code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
            )
        );
    }

    #[test]
    fn deeply_nested_flow_seq_count() {
        // [[1, 2], [3, 4]] → 3 diagnostics: outer seq + two inner seqs.
        let docs = parse_docs("data: [[1, 2], [3, 4]]\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 3);
    }

    // ---- Flow Style Validator: empty-collection edge cases ----

    #[test]
    fn empty_nested_seq_inside_nonempty_map_no_extra_diagnostic() {
        // {a: []} → 1 diagnostic for the outer map; inner empty seq: none.
        let docs = parse_docs("data: {a: []}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap")
        );
    }

    // ---- Flow Style Validator: GHA expression regression (Task 3) ----

    #[test]
    fn flow_style_ignores_github_actions_expressions() {
        // Regression guard: all four expression forms must produce zero diagnostics;
        // the real flow mapping in the same document must still be detected.
        let yaml = "\
jobs:
  build:
    env:
      TOKEN: ${{ secrets.GITHUB_TOKEN }}
      MATRIX_JSON: ${{ fromJSON(needs.x.outputs.y) }}
      COMBINED: ${{ x }} and ${{ y }}
    strategy:
      matrix: { target: linux, os: ubuntu }
";
        let docs = parse_docs(yaml);
        let result = validate_flow_style(&docs, &ValidationSettings::default());

        // Only the real flow mapping on the `matrix:` line should be reported.
        assert_eq!(
            result.len(),
            1,
            "expected exactly 1 diagnostic (matrix line), got: {result:?}"
        );
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "flowMap"),
            "expected flowMap diagnostic on matrix line, got: {:?}",
            result[0].code,
        );
    }

    // ---- Flow Style Validator: severity propagation ----

    #[test]
    fn validate_flow_style_none_returns_empty_on_triggering_input() {
        let docs = parse_docs("config: {key: value}\n");
        let settings = ValidationSettings {
            flow_style: None,
            duplicate_keys: None,
        };
        let result = validate_flow_style(&docs, &settings);
        assert!(
            result.is_empty(),
            "disabled flow_style must suppress all diagnostics"
        );
    }

    #[test]
    fn validate_flow_style_error_severity_produces_error_diagnostics() {
        let docs = parse_docs("config: {key: value}\n");
        let settings = ValidationSettings {
            flow_style: Some(DiagnosticSeverity::ERROR),
            duplicate_keys: None,
        };
        let result = validate_flow_style(&docs, &settings);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn validate_flow_style_default_settings_produces_warning() {
        let docs = parse_docs("config: {key: value}\n");
        let result = validate_flow_style(&docs, &ValidationSettings::default());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
    }
}
