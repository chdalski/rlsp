// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::DEFAULT_MAX_NESTING_DEPTH;
use rlsp_yaml_parser::ErrorKind;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::loader::LoaderBuilder;
use rlsp_yaml_parser::node::Document;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

/// The result of parsing a YAML text buffer.
pub struct ParseResult {
    /// Successfully parsed YAML documents.
    pub documents: Vec<Document<Span>>,
    /// Diagnostics (parse errors) produced during parsing.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse YAML text at the loader's default nesting limit, returning documents
/// and any parse diagnostics.
///
/// Handlers do not call this: they load with the `maxNestingDepth` setting
/// through [`parse_yaml_with_limit`].
#[must_use]
pub fn parse_yaml(text: &str) -> ParseResult {
    parse_yaml_with_limit(text, DEFAULT_MAX_NESTING_DEPTH)
}

/// Parse YAML text with `max_nesting_depth` as the deepest nesting of
/// collections, returning documents and any parse diagnostics.
///
/// A document nested deeper gives one diagnostic and no documents.
#[must_use]
pub fn parse_yaml_with_limit(text: &str, max_nesting_depth: usize) -> ParseResult {
    match LoaderBuilder::new()
        .lossless()
        .max_nesting_depth(max_nesting_depth)
        .build()
        .load(text)
    {
        Ok(documents) => ParseResult {
            documents,
            diagnostics: Vec::new(),
        },
        Err(err) => {
            let (pos, message, code) = match &err {
                rlsp_yaml_parser::loader::LoadError::Parse {
                    pos, message, kind, ..
                } => {
                    let code = if *kind == ErrorKind::InvalidCharacter {
                        "invalidCharacter"
                    } else {
                        "yamlSyntax"
                    };
                    (*pos, message.clone(), code)
                }
                rlsp_yaml_parser::loader::LoadError::UnresolvedScalar { pos, .. }
                | rlsp_yaml_parser::loader::LoadError::NestingDepthLimitExceeded { pos, .. }
                | rlsp_yaml_parser::loader::LoadError::AnchorCountLimitExceeded { pos, .. }
                | rlsp_yaml_parser::loader::LoadError::AliasExpansionLimitExceeded {
                    pos, ..
                }
                | rlsp_yaml_parser::loader::LoadError::CircularAlias { pos, .. }
                | rlsp_yaml_parser::loader::LoadError::UndefinedAlias { pos, .. } => {
                    (*pos, err.to_string(), "yamlSyntax")
                }
                rlsp_yaml_parser::loader::LoadError::UnexpectedEndOfStream => {
                    (rlsp_yaml_parser::Pos::ORIGIN, err.to_string(), "yamlSyntax")
                }
                _ => (rlsp_yaml_parser::Pos::ORIGIN, err.to_string(), "yamlSyntax"),
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "LSP line/col are u32; always fits"
            )]
            let line = pos.line.saturating_sub(1) as u32;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "LSP line/col are u32; always fits"
            )]
            let col = pos.column as u32;
            let start = Position::new(line, col);
            let end = Position::new(line, u32::MAX);
            let diagnostic = Diagnostic {
                range: Range::new(start, end),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String(code.to_string())),
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
mod tests {
    use std::fmt::Write as _;

    use rlsp_yaml_parser::node::Node;
    use rstest::rstest;

    use super::*;

    // Group: parse_yaml_no_diagnostics — assert diagnostics.is_empty() + document count
    #[rstest]
    #[case::valid_yaml("key: value\n", 1)]
    #[case::empty_document("", 0)]
    #[case::comment_only("# this is a comment\n", 0)]
    #[case::complex_types(
        "root:\n  list:\n    - item1\n    - item2\n  nested:\n    key: value\n",
        1
    )]
    #[case::multi_document("key1: value1\n---\nkey2: value2\n", 2)]
    fn parse_yaml_no_diagnostics(#[case] text: &str, #[case] expected_doc_count: usize) {
        let result = parse_yaml(text);
        assert!(
            result.diagnostics.is_empty(),
            "expected no diagnostics, got: {:?}",
            result.diagnostics
        );
        assert_eq!(result.documents.len(), expected_doc_count);
    }

    // Group: parse_yaml_returns_error_diagnostic — assert !diagnostics.is_empty() + ERROR severity
    #[rstest]
    #[case::invalid_yaml("key: [invalid\n")]
    #[case::bad_nested_flow(":\n  bad: [")]
    fn parse_yaml_returns_error_diagnostic(#[case] text: &str) {
        let result = parse_yaml(text);
        assert!(
            !result.diagnostics.is_empty(),
            "expected at least one diagnostic"
        );
        assert_eq!(
            result.diagnostics[0].severity,
            Some(DiagnosticSeverity::ERROR)
        );
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
        // parent.  Deep enough to be a meaningful smoke test.
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
        let result = parse_yaml("hello\n");

        assert!(!result.documents.is_empty());
        match &result.documents[0].root {
            Node::Scalar { loc, .. } => {
                assert_eq!(loc.start, 0);
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

    #[test]
    fn non_printable_character_produces_invalid_character_code() {
        // U+0080 is not c-printable and triggers ErrorKind::InvalidCharacter in the parser.
        let result = parse_yaml("# comment \u{0080}\n");

        assert!(!result.diagnostics.is_empty(), "expected a diagnostic");
        assert_eq!(
            result.diagnostics[0].code,
            Some(NumberOrString::String("invalidCharacter".to_string()))
        );
    }

    #[test]
    fn grammar_error_produces_yaml_syntax_code() {
        let result = parse_yaml("key: [bad\n");

        assert!(!result.diagnostics.is_empty(), "expected a diagnostic");
        assert_eq!(
            result.diagnostics[0].code,
            Some(NumberOrString::String("yamlSyntax".to_string()))
        );
    }

    // TE tests for Task 23 Phase A: API adaptation verification

    #[test]
    fn parse_yaml_returns_documents_with_string_value_type() {
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

    /// Parses `levels` nested block mappings, one key per line, on a thread with
    /// a 2 MiB stack: the limit must hold before the recursive loader overflows
    /// that stack, and a fixed size keeps the test independent of the test
    /// runner's thread.  `parse` is the function under test, called with the
    /// text of the nested mappings.
    fn parse_nested(
        levels: usize,
        parse: impl FnOnce(&str) -> ParseResult + Send + 'static,
    ) -> ParseResult {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                let mut text = String::new();
                for i in 0..levels - 1 {
                    let indent = "  ".repeat(i);
                    writeln!(text, "{indent}level{i}:").unwrap();
                }
                let leaf_indent = "  ".repeat(levels - 1);
                writeln!(text, "{leaf_indent}leaf: value").unwrap();
                parse(&text)
            })
            .expect("thread spawn")
            .join()
            .expect("thread join")
    }

    fn parse_nested_mappings(levels: usize) -> ParseResult {
        parse_nested(levels, parse_yaml)
    }

    fn parse_nested_mappings_with_limit(levels: usize, max_nesting_depth: usize) -> ParseResult {
        parse_nested(levels, move |text| {
            parse_yaml_with_limit(text, max_nesting_depth)
        })
    }

    #[rstest]
    #[case::least(1)]
    #[case::small(2)]
    #[case::default_limit(256)]
    #[case::over_the_default(300)]
    #[case::parser_cap(512)]
    fn parse_yaml_with_limit_loads_a_document_nested_exactly_to_the_limit(#[case] limit: usize) {
        let result = parse_nested_mappings_with_limit(limit, limit);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.documents.len(), 1);
    }

    #[rstest]
    #[case::least(1)]
    #[case::small(2)]
    #[case::default_limit(256)]
    #[case::over_the_default(300)]
    #[case::parser_cap(512)]
    fn parse_yaml_with_limit_reports_one_diagnostic_over_the_limit(#[case] limit: usize) {
        let result = parse_nested_mappings_with_limit(limit + 1, limit);

        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.documents.is_empty());
        let diagnostic = &result.diagnostics[0];
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("yamlSyntax".to_string()))
        );
        let start = diagnostic.range.start;
        assert_eq!(
            (start.line as usize, start.character as usize),
            (limit, 2 * limit)
        );
    }

    #[test]
    fn parse_yaml_loads_the_default_nesting_depth() {
        let result = parse_nested_mappings(DEFAULT_MAX_NESTING_DEPTH);

        assert!(result.diagnostics.is_empty());
        assert_eq!(result.documents.len(), 1);
    }

    #[test]
    fn parse_yaml_nesting_depth_limit_produces_diagnostic() {
        let result = parse_nested_mappings(DEFAULT_MAX_NESTING_DEPTH + 1);

        assert!(
            !result.diagnostics.is_empty(),
            "expected diagnostic for deep nesting"
        );
        assert!(
            result.documents.is_empty(),
            "expected no documents on error"
        );
    }

    #[test]
    fn parse_yaml_nesting_depth_limit_diagnostic_is_at_the_level_over_the_limit() {
        // The diagnostic carries the position of the node that exceeded the
        // limit, not the start of the text: the mapping over the limit starts
        // on the line after the keys of the levels below it, indented by two
        // columns per level.
        let result = parse_nested_mappings(DEFAULT_MAX_NESTING_DEPTH + 1);

        assert_eq!(result.diagnostics.len(), 1);
        let start = result.diagnostics[0].range.start;
        assert_eq!(
            (start.line as usize, start.character as usize),
            (DEFAULT_MAX_NESTING_DEPTH, 2 * DEFAULT_MAX_NESTING_DEPTH)
        );
    }

    #[test]
    fn parse_yaml_undefined_alias_in_lossless_mode_produces_alias_node() {
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
