use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::lsp_types::{DocumentLink, Position, Range, Url};

/// Maximum allowed URL length to prevent DoS attacks.
const MAX_URL_LENGTH: usize = 2048;

/// Regex pattern for detecting URLs with http://, https://, or file:// schemes.
/// Excludes common delimiters and whitespace to avoid capturing surrounding text.
static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(https?|file)://[^\s<>"{}|\\^`\[\]()]+"#).unwrap()
});

/// Find all document links (URLs) in the given YAML text.
///
/// Detects URLs with schemes http://, https://, and file:// in:
/// - YAML string values (both quoted and unquoted)
/// - Comment lines
///
/// Returns a vector of `DocumentLink` with accurate position ranges.
/// Invalid or overly long URLs are silently skipped.
///
/// # Examples
///
/// ```
/// use rlsp_yaml::document_links::find_document_links;
///
/// let text = "homepage: https://example.com\n# See https://docs.example.com\n";
/// let links = find_document_links(text);
/// assert_eq!(links.len(), 2);
/// ```
#[must_use]
pub fn find_document_links(text: &str) -> Vec<DocumentLink> {
    let mut links = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        for mat in URL_REGEX.find_iter(line) {
            let mut matched_text = mat.as_str();

            // Trim trailing punctuation that's not part of the URL
            matched_text = trim_trailing_punctuation(matched_text);
            let byte_end = mat.start() + matched_text.len();

            // Skip URLs exceeding maximum length
            if matched_text.len() > MAX_URL_LENGTH {
                continue;
            }

            // Validate URL and create DocumentLink
            if let Ok(url) = Url::parse(matched_text) {
                let range = calculate_range(line_idx, mat.start(), byte_end, line);
                links.push(DocumentLink {
                    range,
                    target: Some(url),
                    tooltip: None,
                    data: None,
                });
            }
        }
    }

    links
}

/// Trim trailing punctuation characters that are commonly found after URLs in prose.
fn trim_trailing_punctuation(text: &str) -> &str {
    text.trim_end_matches(['.', ',', ';', ':', '!', '?'])
}

/// Calculate LSP Range for a URL match within a line.
///
/// Converts byte offsets to UTF-16 code unit offsets as required by LSP.
fn calculate_range(line: usize, byte_start: usize, byte_end: usize, line_text: &str) -> Range {
    let start_char = byte_to_utf16_offset(line_text, byte_start);
    let end_char = byte_to_utf16_offset(line_text, byte_end);

    Range {
        start: Position {
            line: line as u32,
            character: start_char,
        },
        end: Position {
            line: line as u32,
            character: end_char,
        },
    }
}

/// Convert a byte offset to a UTF-16 code unit offset.
///
/// LSP uses UTF-16 code units for character positions, so we must convert
/// from Rust's UTF-8 byte indices to UTF-16 offsets.
fn byte_to_utf16_offset(line_text: &str, byte_offset: usize) -> u32 {
    line_text[..byte_offset]
        .chars()
        .map(|c| c.len_utf16())
        .sum::<usize>() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to extract (line, start_char, end_char, url) tuples from DocumentLinks.
    fn links_as_tuples(links: &[DocumentLink]) -> Vec<(u32, u32, u32, String)> {
        links
            .iter()
            .map(|l| {
                (
                    l.range.start.line,
                    l.range.start.character,
                    l.range.end.character,
                    l.target
                        .as_ref()
                        .map(|u| u.to_string())
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    // ========== Basic URL Detection Tests ==========

    #[test]
    fn should_detect_url_in_quoted_string() {
        let text = "homepage: \"https://example.com\"\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1, "should find exactly one URL");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "line should be 0");
        assert_eq!(tuples[0].1, 11, "start character after opening quote");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_url_in_single_quoted_string() {
        let text = "url: 'https://example.com'\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 6, "start character after opening quote");
    }

    #[test]
    fn should_detect_url_in_unquoted_value() {
        let text = "homepage: https://example.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 10, "start character after 'homepage: '");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_url_in_comment() {
        let text = "# See https://example.com for details\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_http_and_https_and_file_schemes() {
        let text = "http: http://example.com\nhttps: https://example.com\nfile: file:///path/to/file\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 3, "should detect all three schemes");
        let tuples = links_as_tuples(&result);
        assert!(tuples[0].3.starts_with("http://"));
        assert!(tuples[1].3.starts_with("https://"));
        assert!(tuples[2].3.starts_with("file:///"));
    }

    // ========== Multiple URLs Tests ==========

    #[test]
    fn should_detect_multiple_urls_on_same_line() {
        let text = "urls: https://example.com https://other.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        assert_eq!(tuples[1].3, "https://other.com/");
    }

    #[test]
    fn should_detect_urls_across_multiple_lines() {
        let text = "url1: https://example.com\nurl2: https://other.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0);
        assert_eq!(tuples[1].0, 1);
    }

    #[test]
    fn should_detect_url_in_inline_comment() {
        let text = "key: value # See https://example.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    // ========== Multi-Document YAML Tests ==========

    #[test]
    fn should_detect_urls_in_first_document_section() {
        let text = "url: https://first.com\n---\nother: content\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "URL in first section");
    }

    #[test]
    fn should_detect_urls_in_middle_section() {
        let text = "first: doc\n---\nurl: https://middle.com\n---\nlast: doc\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 2, "URL in middle section");
    }

    #[test]
    fn should_detect_urls_in_all_sections() {
        let text = "url: https://first.com\n---\nurl: https://second.com\n---\nurl: https://third.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 3);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://first.com/");
        assert_eq!(tuples[1].3, "https://second.com/");
        assert_eq!(tuples[2].3, "https://third.com/");
    }

    // ========== Edge Cases Tests ==========

    #[test]
    fn should_return_empty_vec_for_empty_document() {
        let text = "";
        let result = find_document_links(text);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn should_return_empty_vec_for_document_with_no_urls() {
        let text = "key: value\nother: data\n# comment\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn should_detect_url_at_line_start() {
        let text = "https://example.com # comment\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 0, "URL starts at column 0");
    }

    #[test]
    fn should_detect_url_at_line_end() {
        let text = "# comment https://example.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn should_detect_url_with_query_params() {
        let text = "url: https://example.com?foo=bar&baz=qux\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/?foo=bar&baz=qux");
    }

    #[test]
    fn should_detect_url_with_fragment() {
        let text = "url: https://example.com#section\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/#section");
    }

    #[test]
    fn should_detect_url_with_path() {
        let text = "url: https://example.com/path/to/resource\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path/to/resource");
    }

    #[test]
    fn should_detect_url_with_port() {
        let text = "url: https://example.com:8080/path\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com:8080/path");
    }

    #[test]
    fn should_detect_url_with_username() {
        let text = "url: https://user@example.com/path\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://user@example.com/path");
    }

    #[test]
    fn should_detect_mixed_schemes_on_same_line() {
        let text = "http://a.com https://b.com file:///c\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 3);
    }

    #[test]
    fn should_trim_trailing_period_in_prose() {
        let text = "See https://example.com.\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        // Verify range doesn't include the period
        let url_text = "https://example.com";
        assert_eq!(
            tuples[0].2 - tuples[0].1,
            url_text.len() as u32,
            "range should exclude trailing period"
        );
    }

    #[test]
    fn should_trim_trailing_comma() {
        let text = "urls: https://example.com, https://other.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_stop_at_parentheses() {
        let text = "(see https://example.com)\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        // Verify range doesn't include closing paren
        assert!(!tuples[0].3.contains(')'));
    }

    #[test]
    fn should_stop_at_square_brackets() {
        let text = "[https://example.com]\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains(']'));
    }

    #[test]
    fn should_stop_at_curly_braces() {
        let text = "{url: https://example.com}\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains('}'));
    }

    // ========== Security Tests ==========

    #[test]
    fn should_skip_extremely_long_urls() {
        let long_url = format!("https://example.com/{}", "a".repeat(3000));
        let text = format!("url: {}\n", long_url);
        let result = find_document_links(&text);

        assert_eq!(result.len(), 0, "URLs over 2048 chars should be skipped");
    }

    #[test]
    fn should_handle_url_at_max_length() {
        // URL exactly at 2048 chars
        let path = "a".repeat(2048 - "https://example.com/".len());
        let url = format!("https://example.com/{}", path);
        let text = format!("url: {}\n", url);
        let result = find_document_links(&text);

        assert_eq!(result.len(), 1, "URL at exactly 2048 chars should be detected");
    }

    #[test]
    fn should_stop_at_space_in_url() {
        let text = "url: https://exam ple.com\n";
        let result = find_document_links(text);

        // Regex stops at space, so "https://exam" is matched and is a valid URL
        assert_eq!(result.len(), 1, "URL up to space should be detected");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://exam/");
    }

    #[test]
    fn should_handle_url_with_special_encodings() {
        let text = "url: https://example.com/path%20with%20spaces\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path%20with%20spaces");
    }

    #[test]
    fn should_handle_multibyte_utf8_before_url() {
        let text = "説明: https://example.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        // UTF-16 offset calculation should handle multi-byte chars correctly
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_handle_emoji_before_url() {
        let text = "🔗 https://example.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        // Emoji takes 2 UTF-16 code units
        assert_eq!(tuples[0].1, 3, "start position after emoji and space");
    }

    #[test]
    fn should_handle_mixed_multibyte_chars_and_urls() {
        let text = "日本語: https://example.jp 説明\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.jp/");
    }

    #[test]
    fn should_skip_incomplete_url_scheme_only() {
        let text = "url: https://\n";
        let result = find_document_links(text);

        assert_eq!(
            result.len(),
            0,
            "URL with only scheme should fail validation"
        );
    }

    #[test]
    fn should_detect_url_without_tld() {
        let text = "url: https://localhost\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1, "URLs without TLD (like localhost) are valid");
    }

    #[test]
    fn should_handle_file_url_with_triple_slash() {
        let text = "file: file:///absolute/path/to/file\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "file:///absolute/path/to/file");
    }

    #[test]
    fn should_handle_file_url_windows_path() {
        let text = "file: file:///C:/Windows/path\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "file:///C:/Windows/path");
    }

    #[test]
    fn should_handle_comment_without_space_after_hash() {
        let text = "#https://example.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
    }

    // ========== Position Accuracy Tests ==========

    #[test]
    fn should_calculate_correct_positions_for_quoted_urls() {
        let text = "url: \"https://example.com\"\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        let link = &result[0];
        // Should start after the opening quote (character 6)
        assert_eq!(link.range.start.character, 6);
        // Should end before the closing quote
        let url_text = "https://example.com";
        assert_eq!(
            link.range.end.character - link.range.start.character,
            url_text.len() as u32
        );
    }

    #[test]
    fn should_calculate_correct_line_numbers_in_multi_document() {
        let text = "first: doc\n---\nurl: https://example.com\n---\nlast: doc\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 2, "URL is on line 2");
    }

    #[test]
    fn should_have_consistent_ranges_across_lines() {
        let text = "line1: https://first.com\nline2: https://second.com\n";
        let result = find_document_links(text);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].range.start.line, 0);
        assert_eq!(result[1].range.start.line, 1);
        // Both URLs should have same relative position on their lines
        assert_eq!(result[0].range.start.character, result[1].range.start.character);
    }

    // ========== Scheme and Validation Tests ==========

    #[test]
    fn should_only_detect_allowed_schemes() {
        let text = r#"
url1: https://example.com
url2: ftp://example.com
url3: http://example.com
url4: file:///path/to/file
url5: javascript:alert(1)
"#;
        let result = find_document_links(text);

        // Only http, https, file should be detected (ftp and javascript are not matched by regex)
        assert_eq!(result.len(), 3, "only http, https, file schemes should be detected");
        let tuples = links_as_tuples(&result);
        assert!(tuples[0].3.starts_with("https://"));
        assert!(tuples[1].3.starts_with("http://"));
        assert!(tuples[2].3.starts_with("file:///"));
    }

    #[test]
    fn should_validate_urls_with_url_parse() {
        // This test verifies the Url::parse() validation layer exists.
        // Most invalid URLs are already filtered by regex, but we test edge cases.
        // Test URL with just a scheme and domain (should be valid)
        let text = "url: https://example\n";
        let result = find_document_links(text);

        // This is actually a valid URL (no TLD required)
        assert_eq!(result.len(), 1, "simple domain should be valid");

        // The validation layer is tested by should_skip_incomplete_url_scheme_only
        // which verifies https:// (no domain) is rejected
    }

    #[test]
    fn should_only_include_validated_urls() {
        let text = r#"
url1: https://valid.com
url2: https://
url3: https://another-valid.com
"#;
        let result = find_document_links(text);

        // Only the 2 valid URLs should be included (https:// with no domain fails validation)
        assert_eq!(result.len(), 2, "only validated URLs should be included");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://valid.com/");
        assert_eq!(tuples[1].3, "https://another-valid.com/");
    }
}
