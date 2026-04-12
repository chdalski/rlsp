// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};

use super::SchemaAssociation;

// ──────────────────────────────────────────────────────────────────────────────
// Schema association — modeline
// ──────────────────────────────────────────────────────────────────────────────

/// Extract a schema URL from a `yaml-language-server` modeline comment.
///
/// Searches the first 10 lines of `text` for a line of the form:
/// ```text
/// # yaml-language-server: $schema=<url>
/// ```
/// Leading and trailing whitespace around `<url>` is stripped.
/// Returns `None` if no such line is found within the first 10 lines.
#[must_use]
pub fn extract_schema_url(text: &str) -> Option<String> {
    const PREFIX: &str = "# yaml-language-server: $schema=";

    for line in text.lines().take(10) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(PREFIX) {
            let url = rest.trim();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Extract custom tag names from a `yaml-language-server` modeline comment.
///
/// Searches the first 10 lines of `text` for a line of the form:
/// ```text
/// # yaml-language-server: $tags=!include,!ref
/// ```
/// Each tag is trimmed of whitespace. Empty strings after splitting are dropped.
/// Returns an empty `Vec` if no such line is found within the first 10 lines.
#[must_use]
pub fn extract_custom_tags(text: &str) -> Vec<String> {
    const PREFIX: &str = "# yaml-language-server: $tags=";

    for line in text.lines().take(10) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(PREFIX) {
            return rest
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    Vec::new()
}

/// Extract a YAML version from a `yaml-language-server` modeline comment.
///
/// Searches the first 10 lines of `text` for a line of the form:
/// ```text
/// # yaml-language-server: $yamlVersion=1.1
/// ```
/// Only `"1.1"` and `"1.2"` are accepted; any other value is ignored.
/// Leading and trailing whitespace around the value is stripped before
/// validation. Returns `None` if no valid modeline is found within the first
/// 10 lines.
#[must_use]
pub fn extract_yaml_version(text: &str) -> Option<String> {
    const PREFIX: &str = "# yaml-language-server: $yamlVersion=";

    for line in text.lines().take(10) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(PREFIX) {
            let value = rest.trim();
            if value == "1.1" || value == "1.2" {
                return Some(value.to_string());
            }
        }
    }
    None
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema association — Kubernetes auto-detection
// ──────────────────────────────────────────────────────────────────────────────

/// Inspect the first YAML document's root mapping for `apiVersion` and `kind`.
///
/// Returns `Some((api_version, kind))` if both keys are present and both values
/// are plain string scalars.  Returns `None` if the document slice is empty,
/// the root node is not a mapping, or either key is absent / non-string.
#[must_use]
pub fn detect_kubernetes_resource(docs: &[Document<Span>]) -> Option<(String, String)> {
    let root = &docs.first()?.root;
    let Node::Mapping { entries, .. } = root else {
        return None;
    };

    let mut api_version: Option<String> = None;
    let mut kind: Option<String> = None;

    for (k, v) in entries {
        let key = match k {
            Node::Scalar { value, .. } => value.as_str(),
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
        };
        let val = match v {
            Node::Scalar { value, .. } => value.clone(),
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
        };
        match key {
            "apiVersion" => api_version = Some(val),
            "kind" => kind = Some(val),
            _ => {}
        }
    }

    Some((api_version?, kind?))
}

/// Construct a Kubernetes JSON Schema URL for the given resource.
///
/// Uses the schema repository at
/// `https://raw.githubusercontent.com/yannh/kubernetes-json-schema`.
///
/// Filename rules:
/// - `kind` is lowercased.
/// - For grouped API versions (e.g. `apps/v1`) the filename is
///   `{kind}-{group}-{version}.json`.
/// - For core API versions (e.g. `v1`) the filename is
///   `{kind}-{api_version}.json`.
#[must_use]
pub fn kubernetes_schema_url(api_version: &str, kind: &str, k8s_version: &str) -> String {
    let kind_lower = kind.to_lowercase();
    let filename = if let Some((group, version)) = api_version.split_once('/') {
        format!("{kind_lower}-{group}-{version}.json")
    } else {
        format!("{kind_lower}-{api_version}.json")
    };
    let dir_prefix = if k8s_version == "master" {
        "master-standalone-strict".to_string()
    } else {
        format!("v{k8s_version}-standalone-strict")
    };
    format!(
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/{dir_prefix}/{filename}"
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema association — file pattern matching
// ──────────────────────────────────────────────────────────────────────────────

/// Return the schema URL for the first association whose glob pattern matches
/// `filename`, or `None` if no association matches.
///
/// Supported glob syntax:
/// - `*` matches any sequence of characters that does not include `/`
/// - `**` matches any sequence of characters including `/`
/// - All other characters match literally
#[must_use]
pub fn match_schema_by_filename(
    filename: &str,
    associations: &[SchemaAssociation],
) -> Option<String> {
    associations
        .iter()
        .find(|a| glob_matches(&a.pattern, filename))
        .map(|a| a.url.clone())
}

/// Return `true` if `pattern` matches `text` using simple glob rules.
pub(super) fn glob_matches(pattern: &str, text: &str) -> bool {
    glob_matches_inner(pattern.as_bytes(), text.as_bytes())
}

fn glob_matches_inner(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        // Both exhausted — full match
        (None, None) => true,

        // Double-star: matches zero or more path segments
        (Some(&b'*'), _) if pattern.get(1) == Some(&b'*') => {
            let rest_pattern = pattern.get(2..).unwrap_or(&[]);
            // Skip any leading slash after **
            let rest_pattern = rest_pattern.strip_prefix(b"/").unwrap_or(rest_pattern);
            // Try matching rest_pattern against every suffix of text
            for i in 0..=text.len() {
                if glob_matches_inner(rest_pattern, text.get(i..).unwrap_or(&[])) {
                    return true;
                }
            }
            false
        }

        // Single-star: matches any sequence of non-slash characters
        (Some(&b'*'), _) => {
            let rest_pattern = pattern.get(1..).unwrap_or(&[]);
            for i in 0..=text.len() {
                if text.get(..i).is_some_and(|prefix| !prefix.contains(&b'/'))
                    && glob_matches_inner(rest_pattern, text.get(i..).unwrap_or(&[]))
                {
                    return true;
                }
            }
            false
        }

        // Literal character match
        (Some(&pc), Some(&tc)) => {
            if pc == tc {
                glob_matches_inner(
                    pattern.get(1..).unwrap_or(&[]),
                    text.get(1..).unwrap_or(&[]),
                )
            } else {
                false
            }
        }

        // One side exhausted but not the other — no match
        (None, Some(_)) | (Some(_), None) => false,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn n_lines(n: usize) -> String {
        "key: value\n".repeat(n)
    }

    // ══════════════════════════════════════════════════════════════════════════
    // extract_schema_url
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::first_line(
        "# yaml-language-server: $schema=https://example.com/schema.json\nkey: value\n",
        "https://example.com/schema.json"
    )]
    #[case::second_line(
        "key: value\n# yaml-language-server: $schema=https://example.com/schema.json\n",
        "https://example.com/schema.json"
    )]
    #[case::leading_whitespace_in_url(
        "# yaml-language-server: $schema=  https://example.com/schema.json\n",
        "https://example.com/schema.json"
    )]
    #[case::http_url(
        "# yaml-language-server: $schema=http://example.com/schema.json\n",
        "http://example.com/schema.json"
    )]
    #[case::file_url(
        "# yaml-language-server: $schema=file:///path/to/schema.json\n",
        "file:///path/to/schema.json"
    )]
    #[case::none_sentinel_lowercase("# yaml-language-server: $schema=none\nkey: value\n", "none")]
    #[case::none_sentinel_mixed_case("# yaml-language-server: $schema=None\nkey: value\n", "None")]
    #[case::none_sentinel_uppercase("# yaml-language-server: $schema=NONE\nkey: value\n", "NONE")]
    fn extract_schema_url_returns_some(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(extract_schema_url(text), Some(expected.to_string()));
    }

    #[test]
    fn extract_schema_url_returns_some_on_tenth_line() {
        let text = n_lines(9) + "# yaml-language-server: $schema=https://example.com/schema.json\n";
        assert_eq!(
            extract_schema_url(&text),
            Some("https://example.com/schema.json".to_string())
        );
    }

    #[rstest]
    #[case::no_modeline("key: value\nother: stuff\n")]
    #[case::missing_equals("# yaml-language-server: $schema https://example.com/schema.json\n")]
    #[case::wrong_prefix("# yaml-ls: $schema=https://example.com/schema.json\n")]
    #[case::empty_input("")]
    fn extract_schema_url_returns_none(#[case] text: &str) {
        assert_eq!(extract_schema_url(text), None);
    }

    #[test]
    fn extract_schema_url_returns_none_beyond_tenth_line() {
        let text =
            n_lines(10) + "# yaml-language-server: $schema=https://example.com/schema.json\n";
        assert_eq!(extract_schema_url(&text), None);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // extract_custom_tags
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::single_tag(
        "# yaml-language-server: $tags=!include\nkey: value\n",
        vec!["!include"]
    )]
    #[case::multiple_tags(
        "# yaml-language-server: $tags=!include,!ref,!Ref\nkey: value\n",
        vec!["!include", "!ref", "!Ref"]
    )]
    #[case::whitespace_trimmed(
        "# yaml-language-server: $tags= !include , !ref \nkey: value\n",
        vec!["!include", "!ref"]
    )]
    #[case::second_line(
        "key: value\n# yaml-language-server: $tags=!include,!ref\n",
        vec!["!include", "!ref"]
    )]
    fn extract_custom_tags_returns_tags(#[case] text: &str, #[case] expected: Vec<&str>) {
        assert_eq!(
            extract_custom_tags(text),
            expected.into_iter().map(str::to_string).collect::<Vec<_>>()
        );
    }

    #[rstest]
    #[case::no_tags_modeline("key: value\nother: stuff\n")]
    #[case::empty_input("")]
    fn extract_custom_tags_returns_empty(#[case] text: &str) {
        assert_eq!(extract_custom_tags(text), Vec::<String>::new());
    }

    #[test]
    fn extract_custom_tags_returns_empty_beyond_line_10() {
        let text = n_lines(10) + "# yaml-language-server: $tags=!include\n";
        assert_eq!(extract_custom_tags(&text), Vec::<String>::new());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // extract_yaml_version
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::version_1_1_first_line("# yaml-language-server: $yamlVersion=1.1\nkey: value\n", "1.1")]
    #[case::version_1_2_first_line("# yaml-language-server: $yamlVersion=1.2\nkey: value\n", "1.2")]
    #[case::whitespace_stripped(
        "# yaml-language-server: $yamlVersion=  1.2  \nkey: value\n",
        "1.2"
    )]
    #[case::second_line("key: value\n# yaml-language-server: $yamlVersion=1.2\n", "1.2")]
    fn extract_yaml_version_returns_some(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(extract_yaml_version(text), Some(expected.to_string()));
    }

    #[test]
    fn extract_yaml_version_returns_some_on_tenth_line() {
        let text = n_lines(9) + "# yaml-language-server: $yamlVersion=1.1\n";
        assert_eq!(extract_yaml_version(&text), Some("1.1".to_string()));
    }

    #[rstest]
    #[case::invalid_version_2_0("# yaml-language-server: $yamlVersion=2.0\nkey: value\n")]
    #[case::invalid_version_1_0("# yaml-language-server: $yamlVersion=1.0\nkey: value\n")]
    #[case::no_modeline("key: value\n")]
    #[case::empty_input("")]
    #[case::empty_version_value("# yaml-language-server: $yamlVersion=\nkey: value\n")]
    #[case::wrong_prefix("# yaml-ls: $yamlVersion=1.1\nkey: value\n")]
    fn extract_yaml_version_returns_none(#[case] text: &str) {
        assert_eq!(extract_yaml_version(text), None);
    }

    #[test]
    fn extract_yaml_version_returns_none_beyond_tenth_line() {
        let text = n_lines(10) + "# yaml-language-server: $yamlVersion=1.1\n";
        assert_eq!(extract_yaml_version(&text), None);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // match_schema_by_filename
    // ══════════════════════════════════════════════════════════════════════════

    fn assoc(pattern: &str, url: &str) -> SchemaAssociation {
        SchemaAssociation {
            pattern: pattern.to_string(),
            url: url.to_string(),
        }
    }

    #[rstest]
    #[case::exact_filename_match(
        "config.yaml",
        vec![assoc("config.yaml", "https://example.com/config-schema.json")],
        "https://example.com/config-schema.json"
    )]
    #[case::single_star_glob(
        "myfile.yaml",
        vec![assoc("*.yaml", "https://example.com/generic.json")],
        "https://example.com/generic.json"
    )]
    #[case::double_star_glob(
        "configs/nested/file.yaml",
        vec![assoc("configs/**/*.yaml", "https://example.com/schema.json")],
        "https://example.com/schema.json"
    )]
    #[case::first_matching_wins(
        "test.yaml",
        vec![
            assoc("*.yaml", "https://example.com/first.json"),
            assoc("*.yaml", "https://example.com/second.json"),
        ],
        "https://example.com/first.json"
    )]
    fn match_schema_by_filename_returns_url(
        #[case] filename: &str,
        #[case] associations: Vec<SchemaAssociation>,
        #[case] expected: &str,
    ) {
        assert_eq!(
            match_schema_by_filename(filename, &associations),
            Some(expected.to_string())
        );
    }

    #[rstest]
    #[case::extension_mismatch(
        "myfile.yaml",
        vec![assoc("*.json", "https://example.com/schema.json")]
    )]
    #[case::empty_associations("myfile.yaml", vec![])]
    #[case::partial_filename_no_match(
        "my-config.yaml",
        vec![assoc("config.yaml", "https://example.com/schema.json")]
    )]
    fn match_schema_by_filename_returns_none(
        #[case] filename: &str,
        #[case] associations: Vec<SchemaAssociation>,
    ) {
        assert_eq!(match_schema_by_filename(filename, &associations), None);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // detect_kubernetes_resource + kubernetes_schema_url
    // ══════════════════════════════════════════════════════════════════════════

    fn parse_docs(text: &str) -> Vec<rlsp_yaml_parser::node::Document<rlsp_yaml_parser::Span>> {
        rlsp_yaml_parser::load(text).unwrap_or_default()
    }

    #[rstest]
    #[case::core_api_pod("apiVersion: v1\nkind: Pod\n", ("v1", "Pod"))]
    #[case::grouped_api_deployment("apiVersion: apps/v1\nkind: Deployment\n", ("apps/v1", "Deployment"))]
    #[case::hpa_autoscaling(
        "apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\n",
        ("autoscaling/v2", "HorizontalPodAutoscaler")
    )]
    fn detect_kubernetes_resource_returns_some(#[case] text: &str, #[case] expected: (&str, &str)) {
        let docs = parse_docs(text);
        assert_eq!(
            detect_kubernetes_resource(&docs),
            Some((expected.0.to_string(), expected.1.to_string()))
        );
    }

    #[rstest]
    #[case::missing_api_version("kind: Pod\nmetadata:\n  name: test\n")]
    #[case::missing_kind("apiVersion: v1\nmetadata:\n  name: test\n")]
    #[case::first_doc_has_no_fields("other: value\n---\napiVersion: v1\nkind: Pod\n")]
    #[case::non_string_api_version_and_kind("apiVersion:\n  nested: true\nkind:\n  - item\n")]
    fn detect_kubernetes_resource_returns_none(#[case] text: &str) {
        let docs = parse_docs(text);
        assert_eq!(detect_kubernetes_resource(&docs), None);
    }

    #[test]
    fn detect_kubernetes_resource_returns_none_for_empty_docs() {
        assert_eq!(detect_kubernetes_resource(&[]), None);
    }

    #[rstest]
    #[case::core_api_versioned(
        "v1",
        "Pod",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/pod-v1.json"
    )]
    #[case::grouped_api_versioned(
        "apps/v1",
        "Deployment",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/deployment-apps-v1.json"
    )]
    #[case::hpa_autoscaling_versioned(
        "autoscaling/v2",
        "HorizontalPodAutoscaler",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/horizontalpodautoscaler-autoscaling-v2.json"
    )]
    #[case::core_api_master(
        "v1",
        "Pod",
        "master",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/master-standalone-strict/pod-v1.json"
    )]
    #[case::grouped_api_master(
        "apps/v1",
        "Deployment",
        "master",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/master-standalone-strict/deployment-apps-v1.json"
    )]
    fn kubernetes_schema_url_returns_url(
        #[case] api_version: &str,
        #[case] kind: &str,
        #[case] k8s_version: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(
            kubernetes_schema_url(api_version, kind, k8s_version),
            expected
        );
    }

    // K8s-11: "Master" (capital M) falls through to versioned branch (case-sensitive match)
    // Different assertion shape (contains check) — left standalone.
    #[test]
    fn should_treat_capitalised_master_as_versioned_prefix() {
        let url = kubernetes_schema_url("v1", "Pod", "Master");
        assert!(
            url.contains("vMaster-standalone-strict/"),
            "expected vMaster-standalone-strict/ in URL, got: {url}"
        );
    }
}
