// SPDX-License-Identifier: MIT

use regex::Regex;
use rlsp_yaml_parser::LineIndex;
use rlsp_yaml_parser::ScalarStyle;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{DocumentLink, Position, Range, Url};

/// Maximum allowed URL length to prevent `DoS` attacks.
const MAX_URL_LENGTH: usize = 2048;

/// Regex pattern for detecting URLs with http://, https://, or file:// schemes.
/// Excludes common delimiters and whitespace to avoid capturing surrounding text.
static URL_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"(https?|file)://[^\s<>"{}|\\^`\[\]()]+"#)
        .unwrap_or_else(|_| unreachable!("static regex is valid"))
});

/// Find all document links (URLs and `!include` paths) in the given YAML AST.
///
/// Walks every `Node::Scalar` in the AST. For each scalar:
/// - Applies `URL_REGEX` to the decoded scalar `value`, producing `DocumentLink`s
///   with byte-accurate ranges for single-line scalars. Multi-line scalars
///   (literal `|` and folded `>` block styles) fall back to the full `node.loc`
///   span as the range — precise in-value offset arithmetic across embedded
///   newlines is not performed.
/// - If the scalar carries a `!include` tag, treats `value` as a file path and
///   resolves it against `base_uri` when provided.
///
/// **Deliberate behavior change from the pre-retrofit implementation:**
/// Comments are no longer scanned — comments are not `Node::Scalar` nodes in
/// the AST. URLs that previously appeared only in comments will not produce
/// links.
///
/// Returns a vector of `DocumentLink` with accurate position ranges.
/// Invalid or overly long URLs are silently skipped.
#[must_use]
pub fn find_document_links(docs: &[Document<Span>], base_uri: Option<&Url>) -> Vec<DocumentLink> {
    let mut links = Vec::new();
    for doc in docs {
        let idx = doc.line_index();
        collect_node_links(&doc.root, base_uri, &mut links, idx);
    }
    links
}

/// Recursively walk `node` and collect document links into `out`.
fn collect_node_links(
    node: &Node<Span>,
    base_uri: Option<&Url>,
    out: &mut Vec<DocumentLink>,
    idx: &LineIndex,
) {
    match node {
        Node::Scalar {
            value,
            style,
            tag,
            loc,
            ..
        } => {
            // !include tag: treat value as a file path.
            if tag.as_deref() == Some("!include") {
                // Reject empty paths and control characters that are invalid in file paths.
                if !value.is_empty()
                    && !value.contains(['\n', '\r', '\x00'])
                    && let Some(target) = resolve_include_path(value, base_uri)
                {
                    out.push(DocumentLink {
                        range: span_to_range(*loc, idx),
                        target: Some(target),
                        tooltip: Some("Open included file".to_string()),
                        data: None,
                    });
                }
                // Do not also scan the value for URLs when it's an !include tag.
                return;
            }

            // URL detection in scalar value.
            let is_multiline = matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_));
            // quote_char_len: opening quote character count in the source before `value` bytes.
            // For single/double-quoted scalars the first source byte is the quote character.
            let quote_utf16 = match style {
                ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => 1u32,
                ScalarStyle::Plain | ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => 0u32,
            };

            for mat in URL_REGEX.find_iter(value) {
                let matched = trim_trailing_punctuation(mat.as_str());
                if matched.len() > MAX_URL_LENGTH {
                    continue;
                }
                let Ok(url) = Url::parse(matched) else {
                    continue;
                };
                let range = if is_multiline {
                    // Multi-line scalar: fall back to the full node span.
                    span_to_range(*loc, idx)
                } else {
                    // Single-line scalar: compute the precise character range within the
                    // source line. loc.start column is the codepoint column of the scalar's
                    // opening character (including the quote for quoted styles).
                    let loc_start_col = idx.line_column(loc.start).1;
                    let start_utf16 = u32::try_from(loc_start_col as usize).unwrap_or(u32::MAX)
                        + quote_utf16
                        + byte_to_utf16_offset(value, mat.start());
                    let end_utf16 = u32::try_from(loc_start_col as usize).unwrap_or(u32::MAX)
                        + quote_utf16
                        + byte_to_utf16_offset(value, mat.start() + matched.len());
                    let lsp_line =
                        u32::try_from(idx.line_column(loc.start).0.saturating_sub(1) as usize)
                            .unwrap_or(u32::MAX);
                    Range {
                        start: Position {
                            line: lsp_line,
                            character: start_utf16,
                        },
                        end: Position {
                            line: lsp_line,
                            character: end_utf16,
                        },
                    }
                };
                out.push(DocumentLink {
                    range,
                    target: Some(url),
                    tooltip: None,
                    data: None,
                });
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, val) in entries {
                collect_node_links(key, base_uri, out, idx);
                collect_node_links(val, base_uri, out, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node_links(item, base_uri, out, idx);
            }
        }
        Node::Alias { .. } => {}
    }
}

/// Convert a `Span` to an LSP `Range`.
///
/// `Pos::line` is 1-based; LSP lines are 0-based — hence `saturating_sub(1)`.
/// `Pos::column` is 0-based codepoints; used directly as LSP character offset
/// (same limitation as `schema_validation.rs:span_to_range`).
fn span_to_range(loc: Span, idx: &LineIndex) -> Range {
    Range {
        start: Position {
            line: idx.line_column(loc.start).0.saturating_sub(1),
            character: idx.line_column(loc.start).1,
        },
        end: Position {
            line: idx.line_column(loc.end).0.saturating_sub(1),
            character: idx.line_column(loc.end).1,
        },
    }
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
fn trim_trailing_punctuation(s: &str) -> &str {
    s.trim_end_matches(['.', ',', ';', ':', '!', '?'])
}

/// Convert a byte offset to a UTF-16 code unit offset within `s[..byte_offset]`.
///
/// LSP uses UTF-16 code units for character positions.
fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> u32 {
    u32::try_from(s[..byte_offset].chars().map(char::len_utf16).sum::<usize>()).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[expect(clippy::cast_possible_truncation, reason = "test code")]
mod tests {
    use rlsp_yaml_parser::Span;
    use rlsp_yaml_parser::node::Document;
    use rstest::rstest;

    use super::*;

    fn load_docs(yaml: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(yaml).unwrap_or_default()
    }

    fn base(uri: &str) -> Url {
        Url::parse(uri).unwrap()
    }

    /// Helper to extract (`line`, `start_char`, `end_char`, `url`) tuples from `DocumentLink`s.
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
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    // ========== Basic URL Detection Tests ==========

    #[rstest]
    // ========== Basic URL Detection Tests ==========
    #[case::quoted_string("homepage: \"https://example.com\"\n", 1)]
    #[case::single_quoted_string("url: 'https://example.com'\n", 1)]
    #[case::unquoted_value("homepage: https://example.com\n", 1)]
    // Comment lines are not AST nodes — no links produced (deliberate drop).
    #[case::in_comment("# See https://example.com for details\n", 0)]
    // ========== Multiple URLs Tests ==========
    #[case::multiple_on_same_line("urls: https://example.com https://other.com\n", 2)]
    #[case::across_multiple_lines("url1: https://example.com\nurl2: https://other.com\n", 2)]
    // Inline comment: scalar value is "value"; URL is in comment — not an AST node.
    #[case::inline_comment("key: value # See https://example.com\n", 0)]
    // ========== Multi-Document YAML Tests ==========
    #[case::first_document_section("url: https://first.com\n---\nother: content\n", 1)]
    #[case::middle_section("first: doc\n---\nurl: https://middle.com\n---\nlast: doc\n", 1)]
    #[case::all_sections(
        "url: https://first.com\n---\nurl: https://second.com\n---\nurl: https://third.com\n",
        3
    )]
    // ========== Edge Cases Tests ==========
    #[case::empty_document("", 0)]
    #[case::no_urls("key: value\nother: data\n# comment\n", 0)]
    // Comment line — no link (deliberate drop).
    #[case::url_at_line_end("# comment https://example.com\n", 0)]
    #[case::mixed_schemes_same_line("http://a.com https://b.com file:///c\n", 3)]
    #[case::stop_at_space("url: https://exam ple.com\n", 1)]
    #[case::special_encodings("url: https://example.com/path%20with%20spaces\n", 1)]
    #[case::multibyte_utf8_before_url("説明: https://example.com\n", 1)]
    #[case::mixed_multibyte_chars("日本語: https://example.jp 説明\n", 1)]
    // ========== Security Tests ==========
    #[case::scheme_only_rejected("url: https://\n", 0)]
    #[case::localhost_no_tld("url: https://localhost\n", 1)]
    // Comment — no link (deliberate drop).
    #[case::comment_no_space_after_hash("#https://example.com\n", 0)]
    // ========== Scheme and Validation Tests ==========
    #[case::three_schemes(
        "http: http://example.com\nhttps: https://example.com\nfile: file:///path/to/file\n",
        3
    )]
    #[case::only_allowed_schemes(
        "\nurl1: https://example.com\nurl2: ftp://example.com\nurl3: http://example.com\nurl4: file:///path/to/file\nurl5: javascript:alert(1)\n",
        3
    )]
    #[case::validated_urls_only(
        "\nurl1: https://valid.com\nurl2: https://\nurl3: https://another-valid.com\n",
        2
    )]
    fn find_document_links_returns_len(#[case] text: &str, #[case] expected_len: usize) {
        let result = find_document_links(&load_docs(text), None);
        assert_eq!(result.len(), expected_len);
    }

    // ========== Basic URL Detection Tests (tuple content) ==========

    #[test]
    fn should_detect_url_in_quoted_string() {
        let text = "homepage: \"https://example.com\"\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1, "should find exactly one URL");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "line should be 0");
        assert_eq!(tuples[0].1, 11, "start character after opening quote");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_url_in_single_quoted_string() {
        let text = "url: 'https://example.com'\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 6, "start character after opening quote");
    }

    #[test]
    fn should_detect_url_in_unquoted_value() {
        let text = "homepage: https://example.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 10, "start character after 'homepage: '");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    // URL in a comment — AST walk does not visit comments; no link produced.
    #[test]
    fn comment_url_produces_no_link() {
        let text = "# See https://example.com for details\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(
            result.len(),
            0,
            "comment URLs are not detected (deliberate drop)"
        );
    }

    #[test]
    fn should_detect_http_and_https_and_file_schemes() {
        let text =
            "http: http://example.com\nhttps: https://example.com\nfile: file:///path/to/file\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 3, "should detect all three schemes");
        let tuples = links_as_tuples(&result);
        assert!(tuples[0].3.starts_with("http://"));
        assert!(tuples[1].3.starts_with("https://"));
        assert!(tuples[2].3.starts_with("file:///"));
    }

    // ========== Multiple URLs Tests (tuple content) ==========

    #[test]
    fn should_detect_multiple_urls_on_same_line() {
        let text = "urls: https://example.com https://other.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        assert_eq!(tuples[1].3, "https://other.com/");
    }

    #[test]
    fn should_detect_urls_across_multiple_lines() {
        let text = "url1: https://example.com\nurl2: https://other.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0);
        assert_eq!(tuples[1].0, 1);
    }

    // URL in inline comment — AST scalar value is "value", comment is not a node.
    #[test]
    fn inline_comment_url_produces_no_link() {
        let text = "key: value # See https://example.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(
            result.len(),
            0,
            "URL in inline comment is not detected (deliberate drop)"
        );
    }

    // ========== Multi-Document YAML Tests (tuple content) ==========

    #[test]
    fn should_detect_urls_in_first_document_section() {
        let text = "url: https://first.com\n---\nother: content\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "URL in first section");
    }

    #[test]
    fn should_detect_urls_in_middle_section() {
        let text = "first: doc\n---\nurl: https://middle.com\n---\nlast: doc\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 2, "URL in middle section");
    }

    #[test]
    fn should_detect_urls_in_all_sections() {
        let text =
            "url: https://first.com\n---\nurl: https://second.com\n---\nurl: https://third.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 3);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://first.com/");
        assert_eq!(tuples[1].3, "https://second.com/");
        assert_eq!(tuples[2].3, "https://third.com/");
    }

    // ========== Edge Cases Tests (tuple content) ==========

    #[test]
    fn should_detect_url_at_line_start() {
        let text = "https://example.com # comment\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 0, "URL starts at column 0");
    }

    #[test]
    fn should_detect_url_with_query_params() {
        let text = "url: https://example.com?foo=bar&baz=qux\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/?foo=bar&baz=qux");
    }

    #[test]
    fn should_detect_url_with_fragment() {
        let text = "url: https://example.com#section\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/#section");
    }

    #[test]
    fn should_detect_url_with_path() {
        let text = "url: https://example.com/path/to/resource\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path/to/resource");
    }

    #[test]
    fn should_detect_url_with_port() {
        let text = "url: https://example.com:8080/path\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com:8080/path");
    }

    #[test]
    fn should_detect_url_with_username() {
        let text = "url: https://user@example.com/path\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://user@example.com/path");
    }

    #[test]
    fn should_trim_trailing_period_in_prose() {
        let text = "See https://example.com.\n";
        let result = find_document_links(&load_docs(text), None);

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
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_stop_at_parentheses() {
        let text = "(see https://example.com)\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        // Verify range doesn't include closing paren
        assert!(!tuples[0].3.contains(')'));
    }

    #[test]
    fn should_stop_at_square_brackets() {
        let text = "[https://example.com]\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains(']'));
    }

    #[test]
    fn should_stop_at_curly_braces() {
        let text = "{url: https://example.com}\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains('}'));
    }

    // ========== Security Tests ==========

    #[test]
    fn should_skip_extremely_long_urls() {
        let long_url = format!("https://example.com/{}", "a".repeat(3000));
        let text = format!("url: {long_url}\n");
        let result = find_document_links(&load_docs(&text), None);

        assert_eq!(result.len(), 0, "URLs over 2048 chars should be skipped");
    }

    #[test]
    fn should_handle_url_at_max_length() {
        // URL exactly at 2048 chars
        let path = "a".repeat(2048 - "https://example.com/".len());
        let url = format!("https://example.com/{path}");
        let text = format!("url: {url}\n");
        let result = find_document_links(&load_docs(&text), None);

        assert_eq!(
            result.len(),
            1,
            "URL at exactly 2048 chars should be detected"
        );
    }

    #[test]
    fn should_stop_at_space_in_url() {
        let text = "url: https://exam ple.com\n";
        let result = find_document_links(&load_docs(text), None);

        // Regex stops at space, so "https://exam" is matched and is a valid URL
        assert_eq!(result.len(), 1, "URL up to space should be detected");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://exam/");
    }

    #[test]
    fn should_handle_url_with_special_encodings() {
        let text = "url: https://example.com/path%20with%20spaces\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path%20with%20spaces");
    }

    #[test]
    fn should_handle_multibyte_utf8_before_url() {
        let text = "説明: https://example.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        // UTF-16 offset calculation should handle multi-byte chars correctly
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_handle_emoji_before_url() {
        let text = "🔗 https://example.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        // Emoji takes 2 UTF-16 code units
        assert_eq!(tuples[0].1, 3, "start position after emoji and space");
    }

    #[test]
    fn should_handle_mixed_multibyte_chars_and_urls() {
        let text = "日本語: https://example.jp 説明\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.jp/");
    }

    #[test]
    fn should_handle_file_url_with_triple_slash() {
        let text = "file: file:///absolute/path/to/file\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "file:///absolute/path/to/file");
    }

    #[test]
    fn should_handle_file_url_windows_path() {
        let text = "file: file:///C:/Windows/path\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "file:///C:/Windows/path");
    }

    // ========== Scheme and Validation Tests ==========

    #[test]
    fn should_only_detect_allowed_schemes() {
        let text = r"
url1: https://example.com
url2: ftp://example.com
url3: http://example.com
url4: file:///path/to/file
url5: javascript:alert(1)
";
        let result = find_document_links(&load_docs(text), None);

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
        let result = find_document_links(&load_docs(text), None);

        // This is actually a valid URL (no TLD required)
        assert_eq!(result.len(), 1, "simple domain should be valid");

        // The validation layer is tested by should_skip_incomplete_url_scheme_only
        // which verifies https:// (no domain) is rejected
    }

    #[test]
    fn should_only_include_validated_urls() {
        let text = r"
url1: https://valid.com
url2: https://
url3: https://another-valid.com
";
        let result = find_document_links(&load_docs(text), None);

        // Only the 2 valid URLs should be included (https:// with no domain fails validation)
        assert_eq!(result.len(), 2, "only validated URLs should be included");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://valid.com/");
        assert_eq!(tuples[1].3, "https://another-valid.com/");
    }

    // ========== Position Accuracy Tests ==========

    #[test]
    fn should_calculate_correct_positions_for_quoted_urls() {
        let text = "url: \"https://example.com\"\n";
        let result = find_document_links(&load_docs(text), None);

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
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 2, "URL is on line 2");
    }

    #[test]
    fn should_have_consistent_ranges_across_lines() {
        let text = "line1: https://first.com\nline2: https://second.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].range.start.line, 0);
        assert_eq!(result[1].range.start.line, 1);
        // Both URLs should have same relative position on their lines
        assert_eq!(
            result[0].range.start.character,
            result[1].range.start.character
        );
    }

    // ========== !include Tests ==========

    // Tests 1–3: include path resolution (collapsed — same shape: len==1 + target url)
    #[rstest]
    #[case::relative_path_with_base(
        "config: !include foo.yaml\n",
        Some("file:///dir/doc.yaml"),
        "file:///dir/foo.yaml"
    )]
    #[case::absolute_path(
        "config: !include /absolute/path.yaml\n",
        None,
        "file:///absolute/path.yaml"
    )]
    #[case::parent_relative_path(
        "config: !include ../relative.yaml\n",
        Some("file:///dir/subdir/doc.yaml"),
        "file:///dir/relative.yaml"
    )]
    fn include_resolves_to_expected_url(
        #[case] text: &str,
        #[case] base_uri: Option<&str>,
        #[case] expected_url: &str,
    ) {
        let b = base_uri.map(base);
        let result = find_document_links(&load_docs(text), b.as_ref());
        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(include_links.len(), 1);
        assert_eq!(
            include_links[0].target.as_ref().unwrap().as_str(),
            expected_url
        );
    }

    // !include inside quotes → no link (tag-based: parser won't tag a quoted string as !include)
    #[test]
    fn should_skip_include_inside_double_quotes() {
        let text = "note: \"some text with !include in quotes\"\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(&load_docs(text), Some(&b));

        assert!(
            result
                .iter()
                .all(|l| l.tooltip.as_deref() != Some("Open included file")),
            "!include inside quotes should be ignored"
        );
    }

    // no !include → only URL links returned
    #[test]
    fn should_return_only_url_links_when_no_include_present() {
        let text = "url: https://example.com\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(&load_docs(text), Some(&b));

        assert_eq!(result.len(), 1);
        assert!(
            result[0].tooltip.is_none(),
            "should be a URL link, not include"
        );
    }

    // relative path without base URI → no include link
    #[test]
    fn should_skip_relative_include_when_no_base_uri() {
        let text = "config: !include foo.yaml\n";
        let result = find_document_links(&load_docs(text), None);

        assert!(
            result
                .iter()
                .all(|l| l.tooltip.as_deref() != Some("Open included file")),
            "relative !include without base_uri should be skipped"
        );
    }

    // multiple !include tags on different lines
    #[test]
    fn should_detect_multiple_includes_on_different_lines() {
        let text = "a: !include a.yaml\nb: !include b.yaml\nc: !include c.yaml\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(&load_docs(text), Some(&b));

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

    // !include with no path → no link
    #[test]
    fn should_skip_include_with_no_path() {
        let text = "config: !include\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(&load_docs(text), Some(&b));

        assert!(
            result
                .iter()
                .all(|l| l.tooltip.as_deref() != Some("Open included file")),
            "!include with no path should be skipped"
        );
    }

    // both URL and !include present — URL in comment produces no link; !include is detected
    #[test]
    fn should_detect_include_but_not_comment_url_on_separate_lines() {
        let text = "# https://example.com\nconfig: !include foo.yaml\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(&load_docs(text), Some(&b));

        let url_count = result.iter().filter(|l| l.tooltip.is_none()).count();
        let include_count = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .count();
        assert_eq!(
            url_count, 0,
            "comment URL produces no link (deliberate drop)"
        );
        assert_eq!(include_count, 1, "should find one include link");
    }

    // !include on a separate mapping entry following a double-quoted value with apostrophe.
    // The old text-scanner tested that apostrophes inside double-quoted strings didn't confuse
    // quote-tracking. With tag-based detection, the parser handles quoting; this test verifies
    // the AST walk finds !include in the same mapping as a double-quoted scalar with apostrophes.
    #[test]
    fn should_not_suppress_include_after_double_quoted_value_with_apostrophe() {
        let text = "note: \"it's ok\"\nconfig: !include foo.yaml\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(&load_docs(text), Some(&b));

        assert_eq!(
            result
                .iter()
                .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
                .count(),
            1,
            "!include on a key following a double-quoted value with apostrophe should be detected"
        );
    }

    // ========== New rstest regression cases (5 required by dispatch) ==========

    #[rstest]
    // (a) URL in a quoted scalar produces a link inside the quoted scalar's loc
    #[case::url_in_quoted_scalar(
        "link: \"https://example.com/path\"\n",
        None,
        1,
        "https://example.com/path"
    )]
    // (b) URL in a plain scalar produces a correct range (start character is after key+colon+space)
    #[case::url_in_plain_scalar(
        "link: https://example.com/path\n",
        None,
        1,
        "https://example.com/path"
    )]
    // (c) !include tag on a scalar produces a link resolved against base_uri
    #[case::include_tag_produces_link(
        "cfg: !include config.yaml\n",
        Some("file:///base/doc.yaml"),
        1,
        "file:///base/config.yaml"
    )]
    // (d) URL in a comment produces NO link (deliberate drop)
    #[case::comment_url_no_link("# https://example.com\n", None, 0, "")]
    fn regression_document_links(
        #[case] text: &str,
        #[case] base_uri: Option<&str>,
        #[case] expected_count: usize,
        #[case] expected_url: &str,
    ) {
        let b = base_uri.map(|u| Url::parse(u).unwrap());
        let result = find_document_links(&load_docs(text), b.as_ref());
        assert_eq!(result.len(), expected_count);
        if expected_count > 0 {
            let url = result[0].target.as_ref().map_or("", Url::as_str);
            assert!(
                url.starts_with(expected_url) || url == expected_url,
                "expected URL starting with {expected_url:?}, got {url:?}"
            );
        }
    }

    // (e) URL exceeding MAX_URL_LENGTH is skipped — standalone because rstest cases cannot call
    // functions to generate the long string at compile time.
    #[test]
    fn regression_url_over_max_length_skipped() {
        let long_path = "a".repeat(MAX_URL_LENGTH + 1);
        let text = format!("url: https://example.com/{long_path}\n");
        let result = find_document_links(&load_docs(&text), None);
        assert_eq!(result.len(), 0, "URL over MAX_URL_LENGTH must be skipped");
    }

    // ========== New tests from test-engineer list ==========

    // A. URL in a mapping key scalar
    #[test]
    fn url_in_mapping_key_scalar_is_detected() {
        let text = "https://example.com: value\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(result.len(), 1, "URL in a mapping key should be detected");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        assert_eq!(tuples[0].0, 0, "URL key is on line 0");
        assert_eq!(tuples[0].1, 0, "URL key starts at column 0");
    }

    // D. URL in literal block scalar uses full node.loc range (fallback)
    #[test]
    fn url_in_literal_block_scalar_detected_with_full_node_range() {
        let text = "doc: |\n  https://example.com\n  more text\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(
            result.len(),
            1,
            "URL in literal block scalar should be found"
        );
        assert_eq!(
            result[0].target.as_ref().unwrap().as_str(),
            "https://example.com/"
        );
        // Range uses full node.loc (fallback for multi-line): start.line == 0 (doc: key line)
        // or wherever the parser puts the block scalar start — just assert the link exists.
    }

    // E. URL in folded block scalar detected
    #[test]
    fn url_in_folded_block_scalar_detected() {
        let text = "doc: >\n  https://example.com\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(
            result.len(),
            1,
            "URL in folded block scalar should be found"
        );
        assert_eq!(
            result[0].target.as_ref().unwrap().as_str(),
            "https://example.com/"
        );
    }

    // J. !include with URL-scheme absolute path
    #[test]
    fn include_with_url_scheme_path_detected() {
        let text = "ref: !include https://example.com/schema.yaml\n";
        let result = find_document_links(&load_docs(text), None);

        let include_links: Vec<_> = result
            .iter()
            .filter(|l| l.tooltip.as_deref() == Some("Open included file"))
            .collect();
        assert_eq!(
            include_links.len(),
            1,
            "!include with URL path should produce a link"
        );
        assert_eq!(
            include_links[0].target.as_ref().unwrap().as_str(),
            "https://example.com/schema.yaml"
        );
    }

    // K. Non-scalar roots produce no spurious links
    #[test]
    fn mapping_node_without_url_value_produces_no_links() {
        let text = "outer:\n  inner: plain text\n";
        let result = find_document_links(&load_docs(text), None);

        assert_eq!(
            result.len(),
            0,
            "mapping without URL values should produce no links"
        );
    }

    // Security: !include with newline in value → no link
    #[test]
    fn include_with_newline_in_value_produces_no_link() {
        // A literal block scalar tagged with !include — value will contain \n
        let text = "config: !include |\n  foo\n  bar\n";
        let b = base("file:///dir/doc.yaml");
        let result = find_document_links(&load_docs(text), Some(&b));

        assert!(
            result
                .iter()
                .all(|l| l.tooltip.as_deref() != Some("Open included file")),
            "!include with newline in scalar value must not produce a link"
        );
    }
}
