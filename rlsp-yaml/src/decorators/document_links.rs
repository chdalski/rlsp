// SPDX-License-Identifier: MIT

use regex::Regex;
use rlsp_yaml_parser::LineIndex;
use rlsp_yaml_parser::ScalarStyle;
use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{DocumentLink, Position, Range, Url};

use super::scalar_source::{Positions, ScalarSource, SourceChar};

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
/// `text` is the source the `docs` were loaded from. Walks every
/// `Node::Scalar` in the AST. For each scalar:
/// - Applies `URL_REGEX` to the decoded scalar value. In a plain or quoted
///   scalar, the link's range covers the source text of its URL, on the lines
///   that hold it: an escape or a `''` inside or before the URL counts with
///   the columns it takes in the source, a URL on a continuation line is
///   placed on that line, and a URL across an escaped line break gets a range
///   over both lines. The scalar is read one logical line at a time through
///   [`ScalarSource`], so a URL never reaches across a line fold.
/// - Literal `|` and folded `>` block scalars fall back to the full `node.loc`
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
/// A scalar whose span does not fit `text` yields no links: nothing is guessed
/// about where its URLs lie.
///
/// Returns a vector of `DocumentLink` with accurate position ranges.
/// Invalid or overly long URLs are silently skipped.
#[must_use]
pub fn find_document_links(
    docs: &[Document<Span>],
    text: &str,
    base_uri: Option<&Url>,
) -> Vec<DocumentLink> {
    let mut links = Vec::new();
    let mut positions = Positions::new(text);
    for doc in docs {
        let idx = doc.line_index();
        collect_node_links(&doc.root, text, base_uri, &mut links, idx, &mut positions);
    }
    links
}

/// Recursively walk `node` and collect document links into `out`.
fn collect_node_links(
    node: &Node<Span>,
    text: &str,
    base_uri: Option<&Url>,
    out: &mut Vec<DocumentLink>,
    idx: &LineIndex,
    positions: &mut Positions<'_>,
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

            // Every URL holds `://`: a scalar without it needs no source reading.
            if !value.contains("://") {
                return;
            }
            if matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_)) {
                // Block scalar: fall back to the full node span.
                let range = span_to_range(*loc, idx);
                out.extend(find_urls(value).map(|(_, _, url)| DocumentLink {
                    range,
                    target: Some(url),
                    tooltip: None,
                    data: None,
                }));
            } else if let Some(source) = ScalarSource::new(*style, *loc, text, positions) {
                for line in source {
                    collect_line_links(&line, out);
                }
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, val) in entries {
                collect_node_links(key, text, base_uri, out, idx, positions);
                collect_node_links(val, text, base_uri, out, idx, positions);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node_links(item, text, base_uri, out, idx, positions);
            }
        }
        Node::Alias { .. } => {}
    }
}

/// Collect the links of one logical line of a scalar. The range of a link
/// runs from the start of the first character that wrote its URL to the end of
/// the last one; it reaches over two source lines when an escaped line break
/// joins them inside the URL.
fn collect_line_links(cells: &[SourceChar], out: &mut Vec<DocumentLink>) {
    let line: String = cells.iter().map(SourceChar::ch).collect();
    // The matches come in ascending order: count the characters between them
    // once, not from the start of the line for each.
    let mut counted = CountedPrefix::default();
    for (byte_start, matched, url) in find_urls(&line) {
        let first = counted.chars_before(&line, byte_start);
        let Some(url_cells) = cells.get(first..first.saturating_add(matched.chars().count()))
        else {
            continue;
        };
        if let (Some(start), Some(end)) = (url_cells.first(), url_cells.last()) {
            out.push(DocumentLink {
                range: Range::new(start.start(), end.end()),
                target: Some(url),
                tooltip: None,
                data: None,
            });
        }
    }
}

/// The URLs in `s`, each with its byte offset, its text without trailing
/// punctuation, and its parsed form. Overlong and invalid URLs are skipped.
fn find_urls(s: &str) -> impl Iterator<Item = (usize, &str, Url)> {
    URL_REGEX.find_iter(s).filter_map(|mat| {
        let matched = trim_trailing_punctuation(mat.as_str());
        if matched.len() > MAX_URL_LENGTH {
            return None;
        }
        Url::parse(matched)
            .ok()
            .map(|url| (mat.start(), matched, url))
    })
}

/// Convert a `Span` to an LSP `Range`.
///
/// `Pos::line` is 1-based; LSP lines are 0-based — hence `saturating_sub(1)`.
/// `Pos::column` is 0-based code points, as the columns of all features are.
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

/// The characters of a string before ascending byte offsets, counted
/// incrementally: each call counts only the text since the previous one.
#[derive(Default)]
struct CountedPrefix {
    byte: usize,
    chars: usize,
}

impl CountedPrefix {
    /// The number of characters in `s` before `byte_offset`, which must not be
    /// smaller than that of the previous call and must lie on a character
    /// boundary; otherwise it is the count up to the previous offset.
    fn chars_before(&mut self, s: &str, byte_offset: usize) -> usize {
        if let Some(between) = s.get(self.byte..byte_offset) {
            self.chars = self.chars.saturating_add(between.chars().count());
            self.byte = byte_offset;
        }
        self.chars
    }
}

#[cfg(test)]
#[expect(clippy::cast_possible_truncation, reason = "test code")]
mod tests {
    use rlsp_yaml_parser::Span;
    use rlsp_yaml_parser::node::Document;
    use rstest::rstest;

    use super::*;
    use crate::test_utils::text_under;

    fn load_docs(yaml: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(yaml).unwrap_or_default()
    }

    fn links_of(yaml: &str, base_uri: Option<&Url>) -> Vec<DocumentLink> {
        find_document_links(&load_docs(yaml), yaml, base_uri)
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
        let result = links_of(text, None);
        assert_eq!(result.len(), expected_len);
    }

    // ========== Basic URL Detection Tests (tuple content) ==========

    #[test]
    fn should_detect_url_in_quoted_string() {
        let text = "homepage: \"https://example.com\"\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1, "should find exactly one URL");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "line should be 0");
        assert_eq!(tuples[0].1, 11, "start character after opening quote");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_detect_url_in_single_quoted_string() {
        let text = "url: 'https://example.com'\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 6, "start character after opening quote");
    }

    #[test]
    fn should_detect_url_in_unquoted_value() {
        let text = "homepage: https://example.com\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 10, "start character after 'homepage: '");
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    // URL in a comment — AST walk does not visit comments; no link produced.
    #[test]
    fn comment_url_produces_no_link() {
        let text = "# See https://example.com for details\n";
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        assert_eq!(tuples[1].3, "https://other.com/");
    }

    #[test]
    fn should_detect_urls_across_multiple_lines() {
        let text = "url1: https://example.com\nurl2: https://other.com\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0);
        assert_eq!(tuples[1].0, 1);
    }

    // URL in inline comment — AST scalar value is "value", comment is not a node.
    #[test]
    fn inline_comment_url_produces_no_link() {
        let text = "key: value # See https://example.com\n";
        let result = links_of(text, None);

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
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 0, "URL in first section");
    }

    #[test]
    fn should_detect_urls_in_middle_section() {
        let text = "first: doc\n---\nurl: https://middle.com\n---\nlast: doc\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].0, 2, "URL in middle section");
    }

    #[test]
    fn should_detect_urls_in_all_sections() {
        let text =
            "url: https://first.com\n---\nurl: https://second.com\n---\nurl: https://third.com\n";
        let result = links_of(text, None);

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
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].1, 0, "URL starts at column 0");
    }

    #[test]
    fn should_detect_url_with_query_params() {
        let text = "url: https://example.com?foo=bar&baz=qux\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/?foo=bar&baz=qux");
    }

    #[test]
    fn should_detect_url_with_fragment() {
        let text = "url: https://example.com#section\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/#section");
    }

    #[test]
    fn should_detect_url_with_path() {
        let text = "url: https://example.com/path/to/resource\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path/to/resource");
    }

    #[test]
    fn should_detect_url_with_port() {
        let text = "url: https://example.com:8080/path\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com:8080/path");
    }

    #[test]
    fn should_detect_url_with_username() {
        let text = "url: https://user@example.com/path\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://user@example.com/path");
    }

    #[test]
    fn should_trim_trailing_period_in_prose() {
        let text = "See https://example.com.\n";
        let result = links_of(text, None);

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
        let result = links_of(text, None);

        assert_eq!(result.len(), 2);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
    }

    #[test]
    fn should_stop_at_parentheses() {
        let text = "(see https://example.com)\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/");
        // Verify range doesn't include closing paren
        assert!(!tuples[0].3.contains(')'));
    }

    #[test]
    fn should_stop_at_square_brackets() {
        let text = "[https://example.com]\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains(']'));
    }

    #[test]
    fn should_stop_at_curly_braces() {
        let text = "{url: https://example.com}\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert!(!tuples[0].3.contains('}'));
    }

    // ========== Security Tests ==========

    #[test]
    fn should_stop_at_space_in_url() {
        let text = "url: https://exam ple.com\n";
        let result = links_of(text, None);

        // Regex stops at space, so "https://exam" is matched and is a valid URL
        assert_eq!(result.len(), 1, "URL up to space should be detected");
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://exam/");
    }

    #[test]
    fn should_handle_url_with_special_encodings() {
        let text = "url: https://example.com/path%20with%20spaces\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "https://example.com/path%20with%20spaces");
    }

    // The features count columns in code points, whatever the width of the
    // characters before the link; the handler converts them for the client.
    #[rstest]
    #[case::cjk_key("説明: https://example.com\n", &[(4, 23, "https://example.com/")])]
    #[case::emoji_before("🔗 https://example.com\n", &[(2, 21, "https://example.com/")])]
    #[case::mixed_multibyte(
        "日本語: https://example.jp 説明\n",
        &[(5, 23, "https://example.jp/")]
    )]
    #[case::astral_in_the_quoted_scalar(
        "k: \"😀 https://example.com\"\n",
        &[(6, 25, "https://example.com/")]
    )]
    #[case::astral_in_the_url(
        "k: https://example.com/😀\n",
        &[(3, 24, "https://example.com/%F0%9F%98%80")]
    )]
    #[case::two_links_after_an_astral_character(
        "k: 😀 https://a.b/x https://c.d/y\n",
        &[(5, 18, "https://a.b/x"), (19, 32, "https://c.d/y")]
    )]
    fn link_columns_count_code_points(#[case] text: &str, #[case] expected: &[(u32, u32, &str)]) {
        let result = links_of(text, None);

        let found = links_as_tuples(&result);
        let expected: Vec<_> = expected
            .iter()
            .map(|&(start, end, target)| (0, start, end, target.to_string()))
            .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn an_include_after_an_astral_character_ranges_over_its_code_points() {
        let base = Url::parse("file:///dir/doc.yaml").unwrap();

        let result = links_of("😀: !include foo.yaml\n", Some(&base));

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].range,
            Range::new(Position::new(0, 12), Position::new(0, 20))
        );
    }

    #[rstest]
    #[timeout(std::time::Duration::from_secs(5))]
    fn the_links_of_a_long_scalar_are_found_in_linear_time() {
        let text = format!("k: \"{}\"\n", "\u{1f600} http://a.b ".repeat(20_000));

        let result = links_of(&text, None);

        assert_eq!(result.len(), 20_000);
        // 4 columns for `k: "`, then 13 characters per round (an emoji, a
        // space, the 10-character URL, a space) and 2 into the last round.
        assert_eq!(result[19_999].range.start.character, 4 + 19_999 * 13 + 2);
    }

    #[test]
    fn should_handle_file_url_with_triple_slash() {
        let text = "file: file:///absolute/path/to/file\n";
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        let tuples = links_as_tuples(&result);
        assert_eq!(tuples[0].3, "file:///absolute/path/to/file");
    }

    #[test]
    fn should_handle_file_url_windows_path() {
        let text = "file: file:///C:/Windows/path\n";
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 2, "URL is on line 2");
    }

    #[test]
    fn should_have_consistent_ranges_across_lines() {
        let text = "line1: https://first.com\nline2: https://second.com\n";
        let result = links_of(text, None);

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
        let result = links_of(text, b.as_ref());
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

    // A multi-line quoted scalar: a URL on its first line keeps its link, and an
    // `!include` link covers the scalar up to its closing quote.
    #[test]
    fn url_in_multi_line_quoted_scalar_keeps_its_first_line_range() {
        let text = "k: \"http://a.b/c\n d\"\nz: 1\n";
        let result = links_of(text, None);

        assert_eq!(
            links_as_tuples(&result),
            [(0, 4, 16, "http://a.b/c".to_owned())]
        );
    }

    #[test]
    fn include_link_of_multi_line_quoted_scalar_ends_at_the_closing_quote() {
        let text = "k: !include \"a\n  b.yaml\"\nz: 1\n";
        let b = base("file:///dir/doc.yaml");
        let result = links_of(text, Some(&b));

        assert_eq!(result.len(), 1);
        let range = result[0].range;
        assert_eq!(range.start, Position::new(0, 12));
        assert_eq!(range.end, Position::new(1, 9));
    }

    // !include inside quotes → no link (tag-based: parser won't tag a quoted string as !include)
    #[test]
    fn should_skip_include_inside_double_quotes() {
        let text = "note: \"some text with !include in quotes\"\n";
        let b = base("file:///dir/doc.yaml");
        let result = links_of(text, Some(&b));

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
        let result = links_of(text, Some(&b));

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
        let result = links_of(text, None);

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
        let result = links_of(text, Some(&b));

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
        let result = links_of(text, Some(&b));

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
        let result = links_of(text, Some(&b));

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
        let result = links_of(text, Some(&b));

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
        let result = links_of(text, b.as_ref());
        assert_eq!(result.len(), expected_count);
        if expected_count > 0 {
            let url = result[0].target.as_ref().map_or("", Url::as_str);
            assert!(
                url.starts_with(expected_url) || url == expected_url,
                "expected URL starting with {expected_url:?}, got {url:?}"
            );
        }
    }

    // ========== New tests from test-engineer list ==========

    // A. URL in a mapping key scalar
    #[test]
    fn url_in_mapping_key_scalar_is_detected() {
        let text = "https://example.com: value\n";
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, None);

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
        let result = links_of(text, Some(&b));

        assert!(
            result
                .iter()
                .all(|l| l.tooltip.as_deref() != Some("Open included file")),
            "!include with newline in scalar value must not produce a link"
        );
    }

    // ========== Ranges over the source text of a URL ==========

    /// `((start line, start column), (end line, end column), target)` of every
    /// link of `text`, in code points.
    type LinkRange = ((u32, u32), (u32, u32), String);

    fn link_ranges_of(yaml: &str) -> Vec<LinkRange> {
        links_of(yaml, None)
            .iter()
            .map(|l| {
                (
                    (l.range.start.line, l.range.start.character),
                    (l.range.end.line, l.range.end.character),
                    l.target
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    fn one(start: (u32, u32), end: (u32, u32), target: &str) -> Vec<LinkRange> {
        vec![(start, end, target.to_owned())]
    }

    #[rstest]
    #[case::tab_escape_before_url("k: \"\\t https://x.com\"", (0, 7), (0, 20), "https://x.com/")]
    #[case::hex_escape_before_url(
        "key: \"\\x41 https://example.com\"",
        (0, 11),
        (0, 30),
        "https://example.com/"
    )]
    #[case::nul_escape_directly_before_url(
        "k: \"a\\0https://x.com\"",
        (0, 7),
        (0, 20),
        "https://x.com/"
    )]
    #[case::doubled_quote_before_url(
        "k: 'it''s https://x.com'",
        (0, 10),
        (0, 23),
        "https://x.com/"
    )]
    #[case::escapes_then_astral_then_url(
        "k: \"\\x41\\x41😀https://e.com\"",
        (0, 13),
        (0, 26),
        "https://e.com/"
    )]
    #[case::wide_escapes_before_url(
        "k: \"é\\U0001F600 https://x.com\"",
        (0, 16),
        (0, 29),
        "https://x.com/"
    )]
    #[case::tag_before_escaped_quoted(
        "a: !!str \"\\x41 https://x.com\"",
        (0, 15),
        (0, 28),
        "https://x.com/"
    )]
    #[case::anchor_before_single_quoted(
        "a: &x 'it''s https://x.com'",
        (0, 13),
        (0, 26),
        "https://x.com/"
    )]
    #[case::escape_inside_url(
        "k: \"https://x.com/a\\x41b\"",
        (0, 4),
        (0, 24),
        "https://x.com/aAb"
    )]
    #[case::unicode_escape_inside_url(
        "k: \"https://x.com/\\u00E9\"",
        (0, 4),
        (0, 24),
        "https://x.com/%C3%A9"
    )]
    #[case::literal_non_ascii_inside_url(
        "k: \"https://x.com/é\"",
        (0, 4),
        (0, 19),
        "https://x.com/%C3%A9"
    )]
    #[case::escaped_slashes_inside_url(
        "k: \"https:\\/\\/x.com\\/a\"",
        (0, 4),
        (0, 22),
        "https://x.com/a"
    )]
    #[case::doubled_quote_inside_url(
        "k: 'https://x.com/it''s'",
        (0, 4),
        (0, 23),
        "https://x.com/it's"
    )]
    #[case::doubled_quote_at_url_end(
        "k: 'https://x.com/a'''",
        (0, 4),
        (0, 21),
        "https://x.com/a'"
    )]
    #[case::escaped_dot_is_trimmed(
        "k: \"https://x.com/a\\x2E\"",
        (0, 4),
        (0, 19),
        "https://x.com/a"
    )]
    #[case::two_escaped_dots_are_trimmed(
        "k: \"https://x.com/\\x2E\\x2E\"",
        (0, 4),
        (0, 18),
        "https://x.com/"
    )]
    #[case::dots_either_side_of_an_escaped_break_are_trimmed(
        "k: \"https://x.com/a.\\\n  .\"",
        (0, 4),
        (0, 19),
        "https://x.com/a"
    )]
    #[case::url_across_an_escaped_break(
        "k: \"https://x.com/a\\\n  b\"",
        (0, 4),
        (1, 3),
        "https://x.com/ab"
    )]
    #[case::url_across_an_escaped_break_crlf(
        "k: \"https://x.com/a\\\r\n  b\"",
        (0, 4),
        (1, 3),
        "https://x.com/ab"
    )]
    #[case::literal_backslash_in_plain("k: https://x.com/a\\b", (0, 3), (0, 18), "https://x.com/a")]
    #[case::plain_continuation("k: a\n  https://x.com", (1, 2), (1, 15), "https://x.com/")]
    #[case::double_quoted_continuation(
        "k: \"a\n  https://x.com\"",
        (1, 2),
        (1, 15),
        "https://x.com/"
    )]
    #[case::single_quoted_continuation(
        "k: 'a\n  https://x.com'",
        (1, 2),
        (1, 15),
        "https://x.com/"
    )]
    #[case::crlf_continuation(
        "k: \"a\r\n  https://x.com\"\r\n",
        (1, 2),
        (1, 15),
        "https://x.com/"
    )]
    #[case::plain_crlf_continuation(
        "k: a\r\n  https://x.com\r\n",
        (1, 2),
        (1, 15),
        "https://x.com/"
    )]
    #[case::cr_continuation("k: a\r  https://x.com\r", (1, 2), (1, 15), "https://x.com/")]
    #[case::escaped_break_continuation(
        "k: \"a\\\n  https://x.com\"",
        (1, 2),
        (1, 15),
        "https://x.com/"
    )]
    #[case::tab_indent_continuation(
        "k: 'a\n\t https://x.com'",
        (1, 2),
        (1, 15),
        "https://x.com/"
    )]
    #[case::continuation_after_crlf_lines(
        "a: 1\r\nk: \"x\r\n  https://y.com\"\r\n",
        (2, 2),
        (2, 15),
        "https://y.com/"
    )]
    #[case::after_an_empty_line_of_a_plain_scalar(
        "k: a\n\n  https://x.com\n  b",
        (2, 2),
        (2, 15),
        "https://x.com/"
    )]
    #[case::url_ends_at_its_folded_line("k: https://x.com/a\n  b", (0, 3), (0, 18), "https://x.com/a")]
    #[case::double_quoted_key(
        "\"\\x41 https://x.com\": v",
        (0, 6),
        (0, 19),
        "https://x.com/"
    )]
    #[case::sequence_item("- \"\\t https://x.com\"", (0, 6), (0, 19), "https://x.com/")]
    #[case::flow_sequence_item("[ \"\\x41 https://x.com\" ]", (0, 8), (0, 21), "https://x.com/")]
    #[case::flow_mapping_key("{ \"\\x41 https://x.com\": 1 }", (0, 8), (0, 21), "https://x.com/")]
    #[case::explicit_multi_line_key("? \"a\n  https://x.com\"\n: v\n", (1, 2), (1, 15), "https://x.com/")]
    fn a_link_covers_the_source_text_of_its_url(
        #[case] text: &str,
        #[case] start: (u32, u32),
        #[case] end: (u32, u32),
        #[case] target: &str,
    ) {
        assert_eq!(link_ranges_of(text), one(start, end, target));
    }

    #[rstest]
    #[case::escaped_space("\\x20")]
    #[case::escaped_tab("\\t")]
    #[case::escaped_quote("\\\"")]
    #[case::escaped_backslash("\\\\")]
    #[case::escaped_brace("\\x7B")]
    #[case::escaped_angle("\\x3C")]
    #[case::next_line("\\N")]
    #[case::no_break_space("\\_")]
    #[case::line_separator("\\L")]
    fn an_escape_that_decodes_to_a_delimiter_ends_the_url(#[case] escape: &str) {
        let text = format!("k: \"https://x.com/a{escape}b\"");

        let found = link_ranges_of(&text);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, (0, 4));
        assert_eq!(found[0].2, "https://x.com/a");
    }

    #[test]
    fn a_url_trimmed_to_nothing_is_skipped() {
        assert!(link_ranges_of("k: \"https://\\x2E\"").is_empty());
    }

    #[test]
    fn the_urls_on_several_lines_of_one_quoted_scalar_have_their_own_ranges() {
        assert_eq!(
            link_ranges_of("k: \"http://a.b/c\n d https://e.f/g\"\n"),
            [
                ((0, 4), (0, 16), "http://a.b/c".to_owned()),
                ((1, 3), (1, 16), "https://e.f/g".to_owned()),
            ]
        );
    }

    #[test]
    fn two_urls_with_escapes_between_and_inside_have_their_own_ranges() {
        assert_eq!(
            link_ranges_of("k: \"https://a.com/\\x41 \\x42 https://b.com/C\""),
            [
                ((0, 4), (0, 22), "https://a.com/A".to_owned()),
                ((0, 28), (0, 43), "https://b.com/C".to_owned()),
            ]
        );
    }

    #[test]
    fn two_urls_split_by_an_escaped_space_have_their_own_ranges() {
        assert_eq!(
            link_ranges_of("k: \"https://a.com\\x20https://b.com\""),
            [
                ((0, 4), (0, 17), "https://a.com/".to_owned()),
                ((0, 21), (0, 34), "https://b.com/".to_owned()),
            ]
        );
    }

    // YAML 1.2.2 section 7.3.1: an escaped line break is followed by the empty
    // lines as line feeds, so the empty line ends the first URL. The parser's
    // value drops that line feed, so a value-based search finds one URL.
    #[test]
    fn an_empty_line_after_an_escaped_break_separates_the_urls() {
        assert_eq!(
            link_ranges_of("k: \"https://a.b/x\\\n\n  https://a.b/y\"\n"),
            [
                ((0, 4), (0, 17), "https://a.b/x".to_owned()),
                ((2, 2), (2, 15), "https://a.b/y".to_owned()),
            ]
        );
    }

    #[test]
    fn a_skipped_overlong_url_does_not_shift_the_next_link() {
        let text = format!(
            "k: \"https://x.com/{}\\x41 https://y.com\"",
            "a".repeat(2100)
        );

        let links = links_of(&text, None);

        assert_eq!(links.len(), 1);
        assert_eq!(text_under(&text, links[0].range), "https://y.com");
    }

    #[rstest]
    #[case::one_below_the_limit(2047, 1)]
    #[case::at_the_limit(2048, 1)]
    #[case::one_above_the_limit(2049, 0)]
    fn the_length_limit_counts_the_decoded_url(#[case] length: usize, #[case] expected: usize) {
        let prefix = "https://x.com/";
        let tail = length - prefix.len();
        let plain = format!("k: {prefix}{}", "a".repeat(tail));
        let escaped = format!("k: \"{prefix}{}\"", "\\x61".repeat(tail));

        assert_eq!(links_of(&plain, None).len(), expected);
        let links = links_of(&escaped, None);
        assert_eq!(links.len(), expected);
        if let Some(link) = links.first() {
            assert_eq!(link.range.end.character, 4 + 14 + 4 * tail as u32);
        }
    }

    #[test]
    fn links_in_later_documents_are_placed_on_their_lines() {
        assert_eq!(
            link_ranges_of("---\nk: \"\\x41 https://a.com\"\n---\nk: 'it''s https://b.com'\n"),
            [
                ((1, 9), (1, 22), "https://a.com/".to_owned()),
                ((3, 10), (3, 23), "https://b.com/".to_owned()),
            ]
        );
    }

    #[rstest]
    #[case::literal("doc: |\n  https://example.com\n  more text\n")]
    #[case::folded("doc: >\n  https://example.com\n")]
    #[case::escape_like_text("doc: |\n  \\x41 https://x.com\n")]
    fn a_link_in_a_block_scalar_keeps_the_range_of_the_node(#[case] text: &str) {
        let docs = load_docs(text);
        let Node::Mapping { entries, .. } = &docs[0].root else {
            unreachable!("a mapping");
        };
        let Node::Scalar { loc, .. } = &entries[0].1 else {
            unreachable!("a scalar");
        };
        let expected = span_to_range(*loc, docs[0].line_index());

        let links = links_of(text, None);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].range, expected);
    }

    #[test]
    fn a_text_that_does_not_hold_the_span_yields_no_link_and_no_panic() {
        let text = "k: \"\\x41 https://x.com\"\n";
        let docs = load_docs(text);

        assert!(find_document_links(&docs, "", None).is_empty());
        assert!(find_document_links(&docs, "k: \"", None).is_empty());
    }

    #[test]
    fn block_scalar_and_include_links_do_not_depend_on_the_text() {
        let text = "a: |\n  https://x.com\nb: !include /foo.yaml\n";
        let docs = load_docs(text);

        assert_eq!(find_document_links(&docs, "", None).len(), 2);
    }

    // The parser takes the square of the length of one line of escapes, so the
    // document is loaded before the timed search.
    #[test]
    fn escapes_and_urls_on_one_long_line_are_found_in_linear_time() {
        let text = format!("k: \"{}\"\n", "\\x41 http://a.b ".repeat(20_000));
        let docs = load_docs(&text);
        let start = std::time::Instant::now();

        let result = find_document_links(&docs, &text, None);

        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(result.len(), 20_000);
        assert_eq!(result[19_999].range.start.character, 4 + 19_999 * 16 + 5);
        assert_eq!(result[19_999].range.end.character, 4 + 19_999 * 16 + 15);
    }

    #[rstest]
    #[timeout(std::time::Duration::from_secs(5))]
    fn many_logical_lines_of_one_scalar_are_found_in_linear_time() {
        let text = format!("k: \"{} \"\n", " http://a.b\n".repeat(20_000));

        let result = links_of(&text, None);

        assert_eq!(result.len(), 20_000);
        assert_eq!(result[19_999].range.start.line, 19_999);
    }

    // The parser takes the square of the length of one line of escapes, so the
    // document is loaded before the timed search.
    #[test]
    fn a_long_scalar_without_a_url_is_skipped_quickly() {
        let text = format!("k: \"{}\"\n", "\\x41".repeat(20_000));
        let docs = load_docs(&text);
        let start = std::time::Instant::now();

        let result = find_document_links(&docs, &text, None);

        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert!(result.is_empty());
    }

    // The targets are those the value-based search finds, and the source text
    // under a range is what the parser decodes to the link's URL: for random
    // scalars of each style.
    mod properties {
        use proptest::prelude::*;

        use super::*;

        fn token() -> impl Strategy<Value = &'static str> {
            prop::sample::select(vec![
                "a",
                "é",
                "😀",
                " ",
                ".",
                ",",
                "https://a.b/x",
                "http://c.d",
                "file:///p",
                "\\x41",
                "\\t",
                "\\\"",
                "\\\\",
                "\\/",
                "\\U0001F600",
                "\\x2E",
                "\\x20",
                "\\N",
                "''",
                "\n  ",
                "\\\n  ",
                "x",
            ])
        }

        /// The URLs of every scalar's decoded value, as the value-based search
        /// finds them.
        fn targets_by_value(docs: &[Document<Span>]) -> Vec<String> {
            fn walk(node: &Node<Span>, out: &mut Vec<String>) {
                match node {
                    Node::Scalar { value, .. } => {
                        out.extend(find_urls(value).map(|(_, _, url)| url.to_string()));
                    }
                    Node::Mapping { entries, .. } => entries.iter().for_each(|(k, v)| {
                        walk(k, out);
                        walk(v, out);
                    }),
                    Node::Sequence { items, .. } => items.iter().for_each(|i| walk(i, out)),
                    Node::Alias { .. } => {}
                }
            }
            let mut out = Vec::new();
            for doc in docs {
                walk(&doc.root, &mut out);
            }
            out
        }

        /// The value that the parser loads for the scalar `source` of `style`.
        fn decoded(style: &str, source: &str) -> Option<String> {
            let docs = rlsp_yaml_parser::load(&format!("k: {style}{source}{style}\n")).ok()?;
            let Node::Mapping { entries, .. } = &docs.first()?.root else {
                return None;
            };
            let Node::Scalar { value, .. } = &entries.first()?.1 else {
                return None;
            };
            Some(value.clone())
        }

        proptest! {
            #[test]
            fn targets_equal_those_found_in_the_values_and_ranges_decode_to_them(
                style in prop::sample::select(vec!["", "\"", "'"]),
                parts in prop::collection::vec(token(), 0..30),
            ) {
                let source: String = parts.concat();
                // The parser drops the line feed of an empty line that follows an
                // escaped line break; the reader keeps it, as YAML 1.2.2
                // section 7.3.1 has it.
                prop_assume!(!source
                    .split("\\\n")
                    .skip(1)
                    .any(|rest| rest.trim_start_matches(' ').starts_with('\n')));
                let text = format!("k: {style}{source}{style}\n");
                let Ok(docs) = rlsp_yaml_parser::load(&text) else {
                    return Ok(());
                };

                let links = find_document_links(&docs, &text, None);

                let found: Vec<String> = links
                    .iter()
                    .filter_map(|l| l.target.as_ref().map(ToString::to_string))
                    .collect();
                prop_assert_eq!(found, targets_by_value(&docs));
                let mut previous = Position::new(0, 0);
                for link in &links {
                    prop_assert!(previous <= link.range.start && link.range.start < link.range.end);
                    previous = link.range.end;
                    if link.range.start.line == link.range.end.line {
                        let under = text_under(&text, link.range);
                        let url = decoded(style, &under).and_then(|v| Url::parse(&v).ok());
                        prop_assert_eq!(url.as_ref(), link.target.as_ref(), "{:?}", text);
                    }
                }
            }
        }
    }
}
