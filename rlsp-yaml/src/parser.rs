use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use yaml_rust2::{Yaml, YamlLoader};

pub struct ParseResult {
    pub documents: Vec<Yaml>,
    pub diagnostics: Vec<Diagnostic>,
}

#[must_use]
pub fn parse_yaml(text: &str) -> ParseResult {
    match YamlLoader::load_from_str(text) {
        Ok(documents) => ParseResult {
            documents,
            diagnostics: Vec::new(),
        },
        Err(err) => {
            let marker = err.marker();
            #[allow(clippy::cast_possible_truncation)]
            let line = marker.line().saturating_sub(1) as u32;
            #[allow(clippy::cast_possible_truncation)]
            let col = marker.col() as u32;
            let start = Position::new(line, col);
            let end = Position::new(line, u32::MAX);
            let diagnostic = Diagnostic {
                range: Range::new(start, end),
                severity: Some(DiagnosticSeverity::ERROR),
                message: err.info().to_string(),
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
        // yaml-rust2 reports the error on the line after the unclosed bracket
        // (line 4 in 1-based = line 3 in 0-based), which is where it expects ']'
        assert_eq!(diag.range.start.line, 3);
    }

    #[test]
    fn should_return_correct_column_position_in_diagnostic() {
        // "a: :" has error at col 3 (0-based), line 1 (1-based) -> line 0 (0-based)
        let result = parse_yaml("a: :\n");

        assert!(!result.diagnostics.is_empty());
        let diag = &result.diagnostics[0];
        assert_eq!(diag.range.start.line, 0);
        assert_eq!(diag.range.start.character, 3);
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
        // yaml-rust2 returns Err for the whole parse, so no partial documents
        assert!(result.documents.is_empty());
    }
}
