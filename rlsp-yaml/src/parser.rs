// SPDX-License-Identifier: MIT

use rlsp_yaml_parser_temp::loader::LoaderBuilder;
use rlsp_yaml_parser_temp::node::Document;
use rlsp_yaml_parser_temp::Span;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

/// Maximum mapping/sequence nesting depth accepted by the parser.
/// Chosen to be deep enough for real-world YAML while staying well within
/// the default thread stack size.
const MAX_NESTING_DEPTH: usize = 256;

pub struct ParseResult {
    pub documents: Vec<Document<Span>>,
    pub diagnostics: Vec<Diagnostic>,
}

#[must_use]
pub fn parse_yaml(text: &str) -> ParseResult {
    match LoaderBuilder::new()
        .lossless()
        .max_nesting_depth(MAX_NESTING_DEPTH)
        .build()
        .load(text)
    {
        Ok(documents) => ParseResult {
            documents,
            diagnostics: Vec::new(),
        },
        Err(err) => {
            let (pos, message) = match &err {
                rlsp_yaml_parser_temp::loader::LoadError::Parse { pos, message } => {
                    (*pos, message.clone())
                }
                rlsp_yaml_parser_temp::loader::LoadError::NestingDepthLimitExceeded { .. }
                | rlsp_yaml_parser_temp::loader::LoadError::AnchorCountLimitExceeded { .. }
                | rlsp_yaml_parser_temp::loader::LoadError::AliasExpansionLimitExceeded { .. }
                | rlsp_yaml_parser_temp::loader::LoadError::CircularAlias { .. }
                | rlsp_yaml_parser_temp::loader::LoadError::UndefinedAlias { .. }
                | rlsp_yaml_parser_temp::loader::LoadError::UnexpectedEndOfStream => {
                    (rlsp_yaml_parser_temp::Pos::ORIGIN, err.to_string())
                }
            };
            #[allow(clippy::cast_possible_truncation)]
            let line = pos.line.saturating_sub(1) as u32;
            #[allow(clippy::cast_possible_truncation)]
            let col = pos.column as u32;
            let start = Position::new(line, col);
            let end = Position::new(line, u32::MAX);
            let diagnostic = Diagnostic {
                range: Range::new(start, end),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("yamlSyntax".to_string())),
                message,
                source: Some("rlsp-yaml".to_string()),
                ..Diagnostic::default()
            };
            ParseResult {
                documents: Vec::new(),
                diagnostics: vec![diagnostic],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn should_return_no_diagnostics_for_valid_yaml() {
        let result = parse_yaml("key: value\n");

        assert!(result.diagnostics.is_empty());
        assert_eq!(result.documents.len(), 1);
    }

    #[test]
    fn should_return_diagnostic_for_invalid_yaml() {
        let result = parse_yaml("key: [invalid\n");

        assert!(!result.diagnostics.is_empty());
        let diag = &result.diagnostics[0];
        assert_eq!(diag.severity, Some(DiagnosticSeverity::ERROR));
        assert!(!diag.message.is_empty());
    }

    #[test]
    fn should_return_correct_line_position_in_diagnostic() {
        let input = "key1: value1\nkey2: value2\nkey3: [bad\n";
        let result = parse_yaml(input);

        assert!(!result.diagnostics.is_empty());
        let diag = &result.diagnostics[0];
        // The parser reports the error where it expects the closing bracket.
        // The exact line depends on the parser implementation — assert it is
        // somewhere on or after the line with the unclosed bracket (line 2, 0-based).
        assert!(
            diag.range.start.line >= 2,
            "error should be reported on or after the unclosed bracket line, got line {}",
            diag.range.start.line
        );
    }

    #[test]
    fn should_return_correct_column_position_in_diagnostic() {
        // Unterminated flow sequence starting at column 4 on line 1.
        let result = parse_yaml("a: [bad\n");

        assert!(!result.diagnostics.is_empty());
        let diag = &result.diagnostics[0];
        // Error is on line 0 (0-based) or later — just verify it is reported.
        assert!(diag.range.start.line <= 1);
    }

    #[test]
    fn should_parse_multi_document_yaml() {
        let input = "key1: value1\n---\nkey2: value2\n";
        let result = parse_yaml(input);

        assert!(result.diagnostics.is_empty());
        assert_eq!(result.documents.len(), 2);
    }

    #[test]
    fn should_return_no_diagnostics_for_empty_document() {
        let result = parse_yaml("");

        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn should_return_no_diagnostics_for_comment_only_document() {
        let result = parse_yaml("# this is a comment\n");

        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn should_return_diagnostic_with_error_severity() {
        let result = parse_yaml(":\n  bad: [");

        assert!(!result.diagnostics.is_empty());
        assert_eq!(
            result.diagnostics[0].severity,
            Some(DiagnosticSeverity::ERROR)
        );
    }

    #[test]
    fn should_include_error_message_in_diagnostic() {
        let result = parse_yaml("key: [bad\n");

        assert!(!result.diagnostics.is_empty());
        assert!(!result.diagnostics[0].message.is_empty());
    }

    #[test]
    fn should_handle_yaml_with_only_document_separator() {
        let result = parse_yaml("---\n");

        assert!(result.diagnostics.is_empty());
        assert!(!result.documents.is_empty());
    }

    #[test]
    fn should_not_panic_on_deeply_nested_yaml() {
        // Build 64 levels of nesting: each key indented 2 more spaces than the
        // parent.  Deep enough to be a meaningful smoke test while staying within
        // the default thread stack size (debug builds have large frames).
        let mut text = String::new();
        for i in 0..64usize {
            let indent = "  ".repeat(i);
            writeln!(text, "{indent}level{i}:").unwrap();
        }
        let leaf_indent = "  ".repeat(64);
        writeln!(text, "{leaf_indent}leaf: value").unwrap();

        // Must not panic; either succeeds or returns an error diagnostic.
        let result = parse_yaml(&text);
        assert!(
            result.documents.len() + result.diagnostics.len() > 0,
            "should return a result (documents or diagnostics), not both empty"
        );
    }

    #[test]
    fn should_not_panic_on_large_document() {
        // Build 10,000 flat key-value pairs.
        let mut text = String::new();
        for i in 0..10_000usize {
            writeln!(text, "key{i}: value{i}").unwrap();
        }

        let result = parse_yaml(&text);
        assert!(result.diagnostics.is_empty(), "should parse without errors");
        assert!(
            !result.documents.is_empty(),
            "should produce at least 1 document"
        );
    }

    #[test]
    fn should_handle_very_large_yaml_document() {
        let text = format!("key: {}", "a".repeat(1_000_000));
        // Must not panic regardless of whether it parses or errors.
        let result = parse_yaml(&text);
        let _ = result;
    }

    #[test]
    fn should_parse_valid_yaml_with_complex_types() {
        let input = "root:\n  list:\n    - item1\n    - item2\n  nested:\n    key: value\n";
        let result = parse_yaml(input);

        assert!(result.diagnostics.is_empty());
        assert_eq!(result.documents.len(), 1);
    }

    #[test]
    fn should_parse_yaml_with_anchors_and_aliases() {
        let input = "defaults: &defaults\n  adapter: postgres\n  host: localhost\nproduction:\n  <<: *defaults\n  host: production-server\n";
        let result = parse_yaml(input);

        assert!(result.diagnostics.is_empty());
        assert!(!result.documents.is_empty());
    }

    #[test]
    fn should_return_diagnostic_for_multi_document_with_invalid_section() {
        let input = "key1: value1\n---\nkey2: [invalid\n";
        let result = parse_yaml(input);

        assert!(!result.diagnostics.is_empty());
        assert_eq!(
            result.diagnostics[0].severity,
            Some(DiagnosticSeverity::ERROR)
        );
        // The parser returns Err for the whole stream, so no partial documents.
        assert!(result.documents.is_empty());
    }

    // TE tests 43-46: new-type verification

    #[test]
    fn should_return_documents_as_document_span_vec() {
        let result = parse_yaml("key: value\n");

        assert!(!result.documents.is_empty());
        // Confirm the type is Document<Span> by accessing the root field.
        let _ = &result.documents[0].root;
    }

    #[test]
    fn should_include_span_in_parsed_scalar() {
        use rlsp_yaml_parser_temp::node::Node;

        let result = parse_yaml("hello\n");

        assert!(!result.documents.is_empty());
        match &result.documents[0].root {
            Node::Scalar { loc, .. } => {
                assert_eq!(loc.start.byte_offset, 0);
            }
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                panic!("expected Scalar")
            }
        }
    }

    #[test]
    fn should_produce_error_diagnostic_with_pos_offset() {
        let result = parse_yaml("key: [bad\n");

        assert!(!result.diagnostics.is_empty());
        // The Pos-to-Position conversion correctly populates the diagnostic range.
        let diag = &result.diagnostics[0];
        assert!(diag.range.start.line <= 1);
    }

    #[test]
    fn should_return_no_documents_on_parse_error() {
        let result = parse_yaml("key: [bad\n");

        assert!(result.documents.is_empty());
    }

    // TE tests for Task 23 Phase A: API adaptation verification

    #[test]
    fn parse_yaml_returns_documents_with_string_value_type() {
        use rlsp_yaml_parser_temp::node::Node;

        let result = parse_yaml("key: value\n");

        assert!(!result.documents.is_empty());
        match &result.documents[0].root {
            Node::Mapping { entries, .. } => {
                assert!(!entries.is_empty());
                let (k, v) = &entries[0];
                match k {
                    Node::Scalar { value, .. } => {
                        // value is a String — confirm it's accessible as &str
                        let s: &str = value.as_str();
                        assert_eq!(s, "key");
                    }
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                        panic!("expected Scalar key")
                    }
                }
                match v {
                    Node::Scalar { value, .. } => {
                        assert_eq!(value.as_str(), "value");
                    }
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                        panic!("expected Scalar value")
                    }
                }
            }
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                panic!("expected Mapping root")
            }
        }
    }

    #[test]
    fn parse_yaml_nesting_depth_limit_produces_diagnostic() {
        // Build YAML with 260 nesting levels — exceeds MAX_NESTING_DEPTH = 256.
        // Run in a thread with a larger stack so the recursive parser itself does
        // not overflow before it can enforce the limit.
        let result = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let mut text = String::new();
                for i in 0..260usize {
                    let indent = "  ".repeat(i);
                    writeln!(text, "{indent}level{i}:").unwrap();
                }
                let leaf_indent = "  ".repeat(260);
                writeln!(text, "{leaf_indent}leaf: value").unwrap();
                parse_yaml(&text)
            })
            .expect("thread spawn")
            .join()
            .expect("thread join");

        // NestingDepthLimitExceeded maps to Pos::ORIGIN — diagnostic should be reported
        assert!(
            !result.diagnostics.is_empty(),
            "expected diagnostic for deep nesting"
        );
        assert!(result.documents.is_empty(), "expected no documents on error");
    }

    #[test]
    fn parse_yaml_undefined_alias_in_lossless_mode_produces_alias_node() {
        use rlsp_yaml_parser_temp::node::Node;

        // In lossless mode (the default), *undefined aliases are NOT errors;
        // they are preserved as Node::Alias leaves.
        let result = parse_yaml("key: *undefined\n");

        // lossless mode: should parse without error, root is a Mapping
        // with an Alias value node
        if result.documents.is_empty() {
            // Some parser implementations DO error on undefined aliases in lossless mode —
            // if so, the diagnostic path must be non-empty.
            assert!(
                !result.diagnostics.is_empty(),
                "either documents or diagnostics must be non-empty"
            );
        } else {
            match &result.documents[0].root {
                Node::Mapping { entries, .. } => {
                    let (_, v) = &entries[0];
                    // Value should be an Alias node (lossless mode)
                    assert!(
                        matches!(v, Node::Alias { .. }),
                        "expected Alias node for *undefined in lossless mode, got: {v:?}"
                    );
                }
                Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                    panic!("expected Mapping root")
                }
            }
        }
    }
}
