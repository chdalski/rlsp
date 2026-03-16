use regex::Regex;
use tower_lsp::lsp_types::{DocumentLink, Position, Range, Url};

/// Maximum allowed URL length to prevent `DoS` attacks.
const MAX_URL_LENGTH: usize = 2048;

/// Regex pattern for detecting URLs with http://, https://, or file:// schemes.
/// Excludes common delimiters and whitespace to avoid capturing surrounding text.
static URL_REGEX: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"(https?|file)://[^\s<>"{}|\\^`\[\]()]+"#).unwrap());

/// Find all document links (URLs and `!include` paths) in the given YAML text.
///
/// Detects URLs with schemes http://, https://, and file:// in:
/// - YAML string values (both quoted and unquoted)
/// - Comment lines
///
/// Also detects `!include <path>` tags and resolves the path against `base_uri`
/// when provided.
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
/// let links = find_document_links(text, None);
/// assert_eq!(links.len(), 2);
/// ```
#[must_use]
pub fn find_document_links(text: &str, base_uri: Option<&Url>) -> Vec<DocumentLink> {
    text.lines()
        .enumerate()
        .flat_map(|(line_idx, line)| {
            let mut links = url_links(line, line_idx);
            links.extend(include_links(line, line_idx, base_uri));
            links
        })
        .collect()
}

/// Return URL links (http/https/file scheme) found on a single line.
fn url_links(line: &str, line_idx: usize) -> Vec<DocumentLink> {
    URL_REGEX
        .find_iter(line)
        .filter_map(|mat| {
            let matched_text = trim_trailing_punctuation(mat.as_str());
            let byte_end = mat.start() + matched_text.len();

            if matched_text.len() > MAX_URL_LENGTH {
                return None;
            }

            let url = Url::parse(matched_text).ok()?;
            let range = calculate_range(line_idx, mat.start(), byte_end, line);
            Some(DocumentLink {
                range,
                target: Some(url),
                tooltip: None,
                data: None,
            })
        })
        .collect()
}

/// Return `!include <path>` links found on a single line.
///
/// Skips occurrences inside quoted strings. Resolves relative paths against
/// `base_uri` when provided; skips them when `base_uri` is `None`.
fn include_links(line: &str, line_idx: usize, base_uri: Option<&Url>) -> Vec<DocumentLink> {
    const TAG: &str = "!include ";

    let mut links = Vec::new();
    let mut search_from = 0;
    while let Some(rel_pos) = line[search_from..].find(TAG) {
        let tag_start = search_from + rel_pos;
        let path_start = tag_start + TAG.len();
        search_from = path_start;

        // Skip if inside a quoted string
        if is_inside_quotes(line, tag_start) {
            continue;
        }

        // Extract path: take until first whitespace or '#' comment marker
        let path = take_until_whitespace_or_comment(&line[path_start..]);

        if path.is_empty() {
            continue;
        }

        let Some(target_url) = resolve_include_path(path, base_uri) else {
            continue;
        };

        let path_byte_end = path_start + path.len();
        let range = calculate_range(line_idx, path_start, path_byte_end, line);

        links.push(DocumentLink {
            range,
            target: Some(target_url),
            tooltip: Some("Open included file".to_string()),
            data: None,
        });
    }
    links
}

/// Extract the path token from the rest of a line after `!include `.
///
/// Stops at the first whitespace character or a `#` comment marker.
fn take_until_whitespace_or_comment(s: &str) -> &str {
    let end = s
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || *c == '#')
        .map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// Return `true` if `pos` (byte offset) falls inside a quoted string on `line`.
///
/// Tracks quote state as a simple three-state machine so that an apostrophe
/// inside a double-quoted string is not mistaken for an opening single-quote,
/// and vice versa.
fn is_inside_quotes(line: &str, pos: usize) -> bool {
    #[derive(PartialEq)]
    enum State {
        Outside,
        InSingle,
        InDouble,
    }

    let mut state = State::Outside;
    let mut chars = line[..pos].chars();
    while let Some(c) = chars.next() {
        match (&state, c) {
            (State::Outside, '"') => state = State::InDouble,
            (State::Outside, '\'') => state = State::InSingle,
            (State::InDouble, '\\') => {
                chars.next(); // skip escaped character
            }
            (State::InDouble, '"') | (State::InSingle, '\'') => state = State::Outside,
            _ => {}
        }
    }
    state != State::Outside
}

/// Resolve an `!include` path to a `Url`.
///
/// - Paths starting with a known URL scheme are parsed directly.
/// - Absolute paths (`/...`) become `file:///...` URLs.
/// - Relative paths are resolved against the directory of `base_uri`.
///   Returns `None` if `base_uri` is absent for a relative path.
fn resolve_include_path(path: &str, base_uri: Option<&Url>) -> Option<Url> {
    // Already a URL
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("file://") {
        return Url::parse(path).ok();
    }

    // Absolute filesystem path
    if path.starts_with('/') {
        return Url::parse(&format!("file://{path}")).ok();
    }

    // Relative path — requires a base URI
    let base = base_uri?;
    // `base.join(".")` gives the directory (trailing slash preserved by the URL spec)
    let dir = base.join(".").ok()?;
    dir.join(path).ok()
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
            line: u32::try_from(line).expect("line index fits in u32"),
            character: start_char,
        },
        end: Position {
            line: u32::try_from(line).expect("line index fits in u32"),
            character: end_char,
        },
    }
}

/// Convert a byte offset to a UTF-16 code unit offset.
///
/// LSP uses UTF-16 code units for character positions, so we must convert
/// from Rust's UTF-8 byte indices to UTF-16 offsets.
fn byte_to_utf16_offset(line_text: &str, byte_offset: usize) -> u32 {
    u32::try_from(
        line_text[..byte_offset]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>(),
    )
    .expect("column offset fits in u32")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(uri: &str) -> Url {
        Url::parse(uri).unwrap()
    }

    /// Helper to extract (line, start_char, end_char, url) tuples from DocumentLinks.
    fn links_as_tuples(links: &[DocumentLink]) -> Vec<(u32, u32, u32, String)> {
        links
            .iter()
            .map(|l| {
                (
                    l.range.start.line,
                    l.range.start.character,
                    l.range.end.character,
                    l.target.as_ref().map(|u| u.to_string()).unwrap_or_default(),
                )
            })
            .collect()
    }

    // ========== Basic URL Detection Tests ==========

    #[test]
    fn should_detect_url_in_quoted_string() {
        let text = "homepage: \"https://example.com\"\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1, "should find exactly one URL");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "line should be 0");
        assert_eq!(tuples[0].1, 11, "start character after opening quote");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_url_in_single_quoted_string() {
        let text = "url: 'https://example.com'\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 6, "start character after opening quote");
    }

    #[test]
    fn should_detect_url_in_unquoted_value() {
        let text = "homepage: https://example.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 10, "start character after 'homepage: '");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_url_in_comment() {
        let text = "# See https://example.com for details\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_http_and_https_and_file_schemes() {
        let text =
            "http: http://example.com\nhttps: https://example.com\nfile: file:///path/to/file\n";
        let result = find_document_links(text, None);

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
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        assert_eq!(tuples[1].3, "https://other.com/");
    }

    #[test]
    fn should_detect_urls_across_multiple_lines() {
        let text = "url1: https://example.com\nurl2: https://other.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0);
        assert_eq!(tuples[1].0, 1);
    }

    #[test]
    fn should_detect_url_in_inline_comment() {
        let text = "key: value # See https://example.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    // ========== Multi-Document YAML Tests ==========

    #[test]
    fn should_detect_urls_in_first_document_section() {
        let text = "url: https://first.com\n---\nother: content\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "URL in first section");
    }

    #[test]
    fn should_detect_urls_in_middle_section() {
        let text = "first: doc\n---\nurl: https://middle.com\n---\nlast: doc\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 2, "URL in middle section");
    }

    #[test]
    fn should_detect_urls_in_all_sections() {
        let text =
            "url: https://first.com\n---\nurl: https://second.com\n---\nurl: https://third.com\n";
        let result = find_document_links(text, None);

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
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn should_return_empty_vec_for_document_with_no_urls() {
        let text = "key: value\nother: data\n# comment\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn should_detect_url_at_line_start() {
        let text = "https://example.com # comment\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 0, "URL starts at column 0");
    }

    #[test]
    fn should_detect_url_at_line_end() {
        let text = "# comment https://example.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn should_detect_url_with_query_params() {
        let text = "url: https://example.com?foo=bar&baz=qux\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/?foo=bar&baz=qux");
    }

    #[test]
    fn should_detect_url_with_fragment() {
        let text = "url: https://example.com#section\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/#section");
    }

    #[test]
    fn should_detect_url_with_path() {
        let text = "url: https://example.com/path/to/resource\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path/to/resource");
    }

    #[test]
    fn should_detect_url_with_port() {
        let text = "url: https://example.com:8080/path\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com:8080/path");
    }

    #[test]
    fn should_detect_url_with_username() {
        let text = "url: https://user@example.com/path\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://user@example.com/path");
    }

    #[test]
    fn should_detect_mixed_schemes_on_same_line() {
        let text = "http://a.com https://b.com file:///c\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 3);
    }

    #[test]
    fn should_trim_trailing_period_in_prose() {
        let text = "See https://example.com.\n";
        let result = find_document_links(text, None);

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
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_stop_at_parentheses() {
        let text = "(see https://example.com)\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        // Verify range doesn't include closing paren
        assert!(!tuples[0].3.contains(')'));
    }

    #[test]
    fn should_stop_at_square_brackets() {
        let text = "[https://example.com]\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains(']'));
    }

    #[test]
    fn should_stop_at_curly_braces() {
        let text = "{url: https://example.com}\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains('}'));
    }

    // ========== Security Tests ==========

    #[test]
    fn should_skip_extremely_long_urls() {
        let long_url = format!("https://example.com/{}", "a".repeat(3000));
        let text = format!("url: {}\n", long_url);
        let result = find_document_links(&text, None);

        assert_eq!(result.len(), 0, "URLs over 2048 chars should be skipped");
    }

    #[test]
    fn should_handle_url_at_max_length() {
        // URL exactly at 2048 chars
        let path = "a".repeat(2048 - "https://example.com/".len());
        let url = format!("https://example.com/{}", path);
        let text = format!("url: {}\n", url);
        let result = find_document_links(&text, None);

        assert_eq!(
            result.len(),
            1,
            "URL at exactly 2048 chars should be detected"
        );
    }

    #[test]
    fn should_stop_at_space_in_url() {
        let text = "url: https://exam ple.com\n";
        let result = find_document_links(text, None);

        // Regex stops at space, so "https://exam" is matched and is a valid URL
        assert_eq!(result.len(), 1, "URL up to space should be detected");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://exam/");
    }

    #[test]
    fn should_handle_url_with_special_encodings() {
        let text = "url: https://example.com/path%20with%20spaces\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path%20with%20spaces");
    }

    #[test]
    fn should_handle_multibyte_utf8_before_url() {
        let text = "説明: https://example.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        // UTF-16 offset calculation should handle multi-byte chars correctly
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_handle_emoji_before_url() {
        let text = "🔗 https://example.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        // Emoji takes 2 UTF-16 code units
        assert_eq!(tuples[0].1, 3, "start position after emoji and space");
    }

    #[test]
    fn should_handle_mixed_multibyte_chars_and_urls() {
        let text = "日本語: https://example.jp 説明\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.jp/");
    }

    #[test]
    fn should_skip_incomplete_url_scheme_only() {
        let text = "url: https://\n";
        let result = find_document_links(text, None);

        assert_eq!(
            result.len(),
            0,
            "URL with only scheme should fail validation"
        );
    }

    #[test]
    fn should_detect_url_without_tld() {
        let text = "url: https://localhost\n";
        let result = find_document_links(text, None);

        assert_eq!(
            result.len(),
            1,
            "URLs without TLD (like localhost) are valid"
        );
    }

    #[test]
    fn should_handle_file_url_with_triple_slash() {
        let text = "file: file:///absolute/path/to/file\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "file:///absolute/path/to/file");
    }

    #[test]
    fn should_handle_file_url_windows_path() {
        let text = "file: file:///C:/Windows/path\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "file:///C:/Windows/path");
    }

    #[test]
    fn should_handle_comment_without_space_after_hash() {
        let text = "#https://example.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
    }

    // ========== Position Accuracy Tests ==========

    #[test]
    fn should_calculate_correct_positions_for_quoted_urls() {
        let text = "url: \"https://example.com\"\n";
        let result = find_document_links(text, None);

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
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 2, "URL is on line 2");
    }

    #[test]
    fn should_have_consistent_ranges_across_lines() {
        let text = "line1: https://first.com\nline2: https://second.com\n";
        let result = find_document_links(text, None);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].range.start.line, 0);
        assert_eq!(result[1].range.start.line, 1);
        // Both URLs should have same relative position on their lines
        assert_eq!(
            result[0].range.start.character,
            result[1].range.start.character
        );
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
        let result = find_document_links(text, None);

        // Only http, https, file should be detected (ftp and javascript are not matched by regex)
        assert_eq!(
            result.len(),
            3,
            "only http, https, file schemes should be detected"
        );
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
        let result = find_document_links(text, None);

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
        let result = find_document_links(text, None);

        // Only the 2 valid URLs should be included (https:// with no domain fails validation)
        assert_eq!(result.len(), 2, "only validated URLs should be included");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://valid.com/");
        assert_eq!(tuples[1].3, "https://another-valid.com/");
    }

    // ========== !include Tests ==========

    // Test 1: relative path with base URI
    #[test]
    fn should_resolve_relative_include_path_against_base_uri() {
        let text = "config: !include foo.yaml\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(include_links.len(), 1);
        assert_eq!(
            include_links[0].target.as_ref().unwrap().as_str(),
            "file:///dir/foo.yaml"
        );
    }

    // Test 2: absolute path
    #[test]
    fn should_resolve_absolute_include_path() {
        let text = "config: !include /absolute/path.yaml\n";
        let result = find_document_links(text, None);

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(include_links.len(), 1);
        assert_eq!(
            include_links[0].target.as_ref().unwrap().as_str(),
            "file:///absolute/path.yaml"
        );
    }

    // Test 3: parent-relative path
    #[test]
    fn should_resolve_parent_relative_include_path() {
        let text = "config: !include ../relative.yaml\n";
        let b = base("file:///dir/subdir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(include_links.len(), 1);
        assert_eq!(
            include_links[0].target.as_ref().unwrap().as_str(),
            "file:///dir/relative.yaml"
        );
    }

    // Test 4: !include inside quotes → no link
    #[test]
    fn should_skip_include_inside_double_quotes() {
        let text = "note: \"some text with !include in quotes\"\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert!(
            include_links.is_empty(),
            "!include inside quotes should be ignored"
        );
    }

    // Test 5: no !include → only URL links returned
    #[test]
    fn should_return_only_url_links_when_no_include_present() {
        let text = "url: https://example.com\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        assert_eq!(result.len(), 1);
        assert!(
            result[0].tooltip.is_none(),
            "should be a URL link, not include"
        );
    }

    // Test 6: relative path without base URI → no include link
    #[test]
    fn should_skip_relative_include_when_no_base_uri() {
        let text = "config: !include foo.yaml\n";
        let result = find_document_links(text, None);

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert!(
            include_links.is_empty(),
            "relative !include without base_uri should be skipped"
        );
    }

    // Test 7: multiple !include tags on different lines
    #[test]
    fn should_detect_multiple_includes_on_different_lines() {
        let text = "a: !include a.yaml\nb: !include b.yaml\nc: !include c.yaml\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(include_links.len(), 3);
        assert_eq!(
            include_links[0].target.as_ref().unwrap().as_str(),
            "file:///dir/a.yaml"
        );
        assert_eq!(
            include_links[1].target.as_ref().unwrap().as_str(),
            "file:///dir/b.yaml"
        );
        assert_eq!(
            include_links[2].target.as_ref().unwrap().as_str(),
            "file:///dir/c.yaml"
        );
    }

    // Test 8: !include with no path → no link
    #[test]
    fn should_skip_include_with_no_path() {
        let text = "config: !include\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert!(
            include_links.is_empty(),
            "!include with no path should be skipped"
        );
    }

    // Test 9: line with both URL and !include → both links detected
    #[test]
    fn should_detect_both_url_and_include_on_same_line() {
        let text = "# https://example.com\nconfig: !include foo.yaml\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        let url_links: Vec<_> = result.iter().filter(|l| l.tooltip.is_none()).collect();
        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(url_links.len(), 1, "should find one URL link");
        assert_eq!(include_links.len(), 1, "should find one include link");
    }

    // Test 10: apostrophe inside double-quoted value must not suppress a following !include
    #[test]
    fn should_not_suppress_include_after_double_quoted_value_with_apostrophe() {
        let text = "note: \"it's ok\" !include foo.yaml\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(text, Some(&b));

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(
            include_links.len(),
            1,
            "!include after a double-quoted value containing an apostrophe should be detected"
        );
    }
}
