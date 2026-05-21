use std::path::Path;

use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;

use super::shared::documents_equivalent;

pub fn check_i10_formatter_round_trip(_path: &Path, text: &str) -> Result<(), String> {
    let parse_pre = parse_yaml(text);
    if parse_pre.documents.is_empty() {
        return Ok(());
    }
    let formatted = format_yaml(text, &YamlFormatOptions::default());
    let parse_post = parse_yaml(&formatted);
    if parse_post.documents.is_empty() {
        return Err("formatter output failed to parse".to_string());
    }
    documents_equivalent(&parse_pre.documents, &parse_post.documents)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::check_i10_formatter_round_trip;

    fn run_i10(text: &str) -> Result<(), String> {
        check_i10_formatter_round_trip(Path::new("test.yaml"), text)
    }

    // UT-I10-1: empty input returns Ok (empty pre-parse branch)
    #[test]
    fn i10_ut1_empty_input_returns_ok() {
        assert!(run_i10("").is_ok());
    }

    // UT-I10-2: invalid YAML returns Ok (empty pre-parse branch)
    #[test]
    fn i10_ut2_invalid_yaml_returns_ok() {
        assert!(run_i10("{{{invalid yaml").is_ok());
    }

    // UT-I10-3: idempotent valid YAML returns Ok (happy path)
    #[test]
    fn i10_ut3_idempotent_valid_yaml_returns_ok() {
        assert!(run_i10("key: value\n").is_ok());
    }

    // UT-I10-4: flow mapping → block conversion returns Ok (style changes, structure unchanged)
    #[test]
    fn i10_ut4_flow_to_block_conversion_returns_ok() {
        assert!(run_i10("{a: 1, b: 2}\n").is_ok());
    }

    // UT-I10-5: multi-document input returns Ok
    #[test]
    fn i10_ut5_multi_document_returns_ok() {
        assert!(run_i10("a: 1\n---\nb: 2\n").is_ok());
    }

    // UT-I10-6: defensive branch; not reachable by any valid formatter input — covered by inspection
}
