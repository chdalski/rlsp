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

/// The `apiVersion` and `kind` of a Kubernetes manifest, checked against
/// Kubernetes naming rules.
///
/// The only way to obtain a value is [`KubernetesResource::new`], so
/// [`kubernetes_schema_url`] never receives raw document text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesResource {
    api_version: String,
    kind: String,
}

impl KubernetesResource {
    /// Validate `api_version` and `kind`; `None` if either breaks the rules.
    ///
    /// - `kind` starts with an ASCII letter, continues with ASCII letters and
    ///   digits, and has at most 63 characters.
    /// - `api_version` is a version (`v1`, `v1beta2`, `v2alpha1`), optionally
    ///   preceded by `group/`, where the group is a DNS-1123 subdomain.
    #[must_use]
    pub fn new(api_version: &str, kind: &str) -> Option<Self> {
        let valid_api_version = match api_version.split_once('/') {
            Some((group, version)) => is_dns_subdomain(group) && is_api_version(version),
            None => is_api_version(api_version),
        };
        (valid_api_version && is_kind(kind)).then(|| Self {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
        })
    }

    /// The validated `apiVersion`, e.g. `v1` or `apps/v1`.
    #[must_use]
    pub fn api_version(&self) -> &str {
        &self.api_version
    }

    /// The validated `kind`, e.g. `Pod`.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

const MAX_VERSION_DIGITS: usize = 9;

fn is_kind(kind: &str) -> bool {
    let mut chars = kind.chars();
    kind.len() <= 63
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric())
}

/// A DNS-1123 subdomain: dot-separated labels of 1–63 lowercase alphanumerics
/// and inner `-`, at most 253 characters in total.
fn is_dns_subdomain(group: &str) -> bool {
    group.len() <= 253
        && group.split('.').all(|label| {
            let bytes = label.as_bytes();
            let alphanumeric = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
            (1..=63).contains(&bytes.len())
                && bytes.first().is_some_and(alphanumeric)
                && bytes.last().is_some_and(alphanumeric)
                && bytes.iter().all(|b| alphanumeric(b) || *b == b'-')
        })
}

/// `v` followed by digits, optionally followed by `alpha` or `beta` and digits.
///
/// Each digit run is capped at [`MAX_VERSION_DIGITS`]: real versions use one or
/// two digits, and the cap keeps a hostile value from lengthening the URL.
fn is_api_version(version: &str) -> bool {
    let is_number = |digits: &str| {
        (1..=MAX_VERSION_DIGITS).contains(&digits.len())
            && digits.bytes().all(|b| b.is_ascii_digit())
    };
    let Some(rest) = version.strip_prefix('v') else {
        return false;
    };
    let digits_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (major, stability) = rest.split_at(digits_end);
    is_number(major)
        && (stability.is_empty()
            || stability
                .strip_prefix("alpha")
                .or_else(|| stability.strip_prefix("beta"))
                .is_some_and(is_number))
}

/// Inspect the first YAML document's root mapping for `apiVersion` and `kind`.
///
/// Returns `Some(resource)` if both keys are present, both values are plain
/// string scalars, and both follow Kubernetes naming rules (see
/// [`KubernetesResource::new`]).  Returns `None` if the document slice is
/// empty, the root node is not a mapping, either key is absent / non-string,
/// or either value breaks the rules.
#[must_use]
pub fn detect_kubernetes_resource(docs: &[Document<Span>]) -> Option<KubernetesResource> {
    let root = &docs.first()?.root;
    let Node::Mapping { entries, .. } = root else {
        return None;
    };

    let mut api_version: Option<&str> = None;
    let mut kind: Option<&str> = None;

    for (k, v) in entries {
        let key = match k {
            Node::Scalar { value, .. } => value.as_str(),
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
        };
        let val = match v {
            Node::Scalar { value, .. } => value.as_str(),
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => continue,
        };
        match key {
            "apiVersion" => api_version = Some(val),
            "kind" => kind = Some(val),
            _ => {}
        }
    }

    KubernetesResource::new(api_version?, kind?)
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
pub fn kubernetes_schema_url(resource: &KubernetesResource, k8s_version: &str) -> String {
    let kind_lower = resource.kind().to_ascii_lowercase();
    let filename = if let Some((group, version)) = resource.api_version().split_once('/') {
        format!("{kind_lower}-{group}-{version}.json")
    } else {
        format!("{kind_lower}-{}.json", resource.api_version())
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
    use crate::test_utils::parse_docs;

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
            Some(KubernetesResource::new(expected.0, expected.1).expect("valid resource"))
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
        let resource = KubernetesResource::new(api_version, kind).expect("valid resource");
        assert_eq!(kubernetes_schema_url(&resource, k8s_version), expected);
    }

    #[rstest]
    #[case::core("v1", "Pod")]
    #[case::grouped("apps/v1", "Deployment")]
    #[case::dotted_group("rbac.authorization.k8s.io/v1", "ClusterRole")]
    #[case::beta("networking.k8s.io/v1beta1", "Ingress")]
    #[case::alpha("example.com/v2alpha3", "Widget")]
    #[case::hyphenated_group("my-group.example.com/v1", "Widget")]
    #[case::kind_with_digits("v1", "K8s2Thing")]
    #[case::crd("apiextensions.k8s.io/v1", "CustomResourceDefinition")]
    #[case::hpa_beta("autoscaling/v2beta1", "HorizontalPodAutoscaler")]
    #[case::cert_manager("cert-manager.io/v1", "Certificate")]
    #[case::argo("argoproj.io/v1alpha1", "Application")]
    #[case::vpa("autoscaling.k8s.io/v1", "VerticalPodAutoscaler")]
    #[case::gateway("gateway.networking.k8s.io/v1", "HTTPRoute")]
    #[case::digit_leading_group_label("1a.example.com/v1", "Widget")]
    #[case::lowercase_kind("v1", "pod")]
    #[case::version_zero("v0", "Pod")]
    #[case::multi_digit_alpha("example.com/v1alpha12", "Widget")]
    #[case::version_9_digits("v123456789", "Pod")]
    #[case::kind_digit_inside("v1", "Ec2NodeClass")]
    #[case::group_x_k8s_io("x-k8s.io/v1", "Widget")]
    #[case::version_two_digits("v10", "Pod")]
    #[case::version_beta_three("v2beta3", "Pod")]
    #[case::kind_single_letter("v1", "P")]
    #[case::kind_63_chars("v1", &"K".repeat(63))]
    #[case::group_label_63_chars(&format!("{}.io/v1", "a".repeat(63)), "Pod")]
    #[case::group_253_chars(&format!("{}/v1", ["a".repeat(63), "b".repeat(63), "c".repeat(63), "d".repeat(61)].join(".")), "Pod")]
    fn kubernetes_resource_accepts(#[case] api_version: &str, #[case] kind: &str) {
        let resource = KubernetesResource::new(api_version, kind).expect("must accept");
        assert_eq!(resource.api_version(), api_version);
        assert_eq!(resource.kind(), kind);
    }

    #[rstest]
    #[case::traversal_kind("v1", "../../../../attacker/repo/main/evil")]
    #[case::slash_in_kind("v1", "Pod/x")]
    #[case::dot_in_kind("v1", "Po.d")]
    #[case::dash_in_kind("v1", "Po-d")]
    #[case::kind_starts_with_digit("v1", "1Pod")]
    #[case::kind_empty("v1", "")]
    #[case::kind_64_chars("v1", &"K".repeat(64))]
    #[case::kind_non_ascii_letter("v1", "Pöd")]
    #[case::kind_turkish_dotted_i("v1", "İngress")]
    #[case::kind_trailing_newline("v1", "Pod\n")]
    #[case::kind_with_space("v1", "Pod x")]
    #[case::kind_percent("v1", "Po%64")]
    #[case::kind_query("v1", "Pod?x=1")]
    #[case::kind_fragment("v1", "Pod#x")]
    #[case::traversal_group("../../x/v1", "Pod")]
    #[case::traversal_version("apps/../v1", "Pod")]
    #[case::slash_in_version("apps/v1/extra", "Pod")]
    #[case::leading_slash("/v1", "Pod")]
    #[case::trailing_slash("apps/", "Pod")]
    #[case::double_slash("apps//v1", "Pod")]
    #[case::uppercase_group("Apps/v1", "Pod")]
    #[case::underscore_group("my_group/v1", "Pod")]
    #[case::group_leading_dot(".apps/v1", "Pod")]
    #[case::group_trailing_dot("apps./v1", "Pod")]
    #[case::group_double_dot("a..b/v1", "Pod")]
    #[case::group_leading_hyphen("-apps/v1", "Pod")]
    #[case::group_trailing_hyphen("apps-/v1", "Pod")]
    #[case::group_label_64_chars(&format!("{}.io/v1", "a".repeat(64)), "Pod")]
    #[case::group_254_chars(&format!("{}/v1", ["a".repeat(63), "b".repeat(63), "c".repeat(63), "d".repeat(62)].join(".")), "Pod")]
    #[case::group_non_ascii("äpps/v1", "Pod")]
    #[case::group_percent("ap%2Fps/v1", "Pod")]
    #[case::api_version_empty("", "Pod")]
    #[case::version_without_v("1", "Pod")]
    #[case::version_v_only("v", "Pod")]
    #[case::version_uppercase_v("V1", "Pod")]
    #[case::version_unknown_suffix("v1gamma1", "Pod")]
    #[case::version_beta_without_number("v1beta", "Pod")]
    #[case::version_alpha_non_digit("v1alphax", "Pod")]
    #[case::version_trailing_dot("v1.", "Pod")]
    #[case::version_dotted("v1.2", "Pod")]
    #[case::version_without_major_before_beta("vbeta1", "Pod")]
    #[case::version_trailing_newline("v1\n", "Pod")]
    #[case::version_with_space("v1 ", "Pod")]
    #[case::version_non_ascii_digit("v١", "Pod")]
    #[case::version_fullwidth_digit("v\u{ff11}", "Pod")]
    #[case::version_plus_sign("v+1", "Pod")]
    #[case::version_hyphenated_beta("v1beta-1", "Pod")]
    #[case::version_beta_trailing_letter("v1beta1x", "Pod")]
    #[case::version_double_v("vv1", "Pod")]
    #[case::version_10_digits("v1234567890", "Pod")]
    #[case::version_10_digit_beta_number("v1beta1234567890", "Pod")]
    #[case::version_5000_digits(&format!("v{}", "1".repeat(5000)), "Pod")]
    #[case::version_alpha_without_number("v1alpha", "Pod")]
    #[case::multiple_slashes("a/b/v1", "Pod")]
    #[case::trailing_slash_after_version("apps/v1/", "Pod")]
    #[case::traversal_after_version("apps/v1/../../x", "Pod")]
    #[case::version_with_slash("v1/x", "Pod")]
    #[case::version_with_query("v1?x", "Pod")]
    #[case::version_hyphen("v-1", "Pod")]
    #[case::version_letter_after_v("va1", "Pod")]
    #[case::mixed_case_group("Example.COM/v1", "Pod")]
    #[case::group_label_leading_hyphen("-a.io/v1", "Pod")]
    #[case::group_label_trailing_hyphen("a-.io/v1", "Pod")]
    #[case::group_leading_dot_label(".a.io/v1", "Pod")]
    #[case::group_trailing_dot_label("a.io./v1", "Pod")]
    #[case::group_percent_encoded_dots("%2e%2e/v1", "Pod")]
    #[case::group_without_version("apps", "Pod")]
    #[case::group_dot_dot_version("apps/..", "Pod")]
    #[case::core_version_with_traversal("v1/..", "Pod")]
    #[case::group_with_at("a@b/v1", "Pod")]
    #[case::group_with_port("a:80/v1", "Pod")]
    #[case::group_with_space("a b/v1", "Pod")]
    #[case::group_with_backslash("a\\b/v1", "Pod")]
    #[case::group_cyrillic_a("\u{430}pps/v1", "Pod")]
    #[case::group_trailing_newline("apps\n/v1", "Pod")]
    #[case::kind_backslash("v1", "Pod\\x")]
    #[case::kind_at("v1", "Pod@host")]
    #[case::kind_colon("v1", "Pod:80")]
    #[case::kind_underscore("v1", "Pod_x")]
    #[case::kind_leading_hyphen("v1", "-Pod")]
    #[case::kind_double_dot("v1", "..")]
    #[case::kind_single_dot("v1", ".")]
    #[case::kind_tab("v1", "Po\td")]
    #[case::kind_crlf("v1", "Pod\r\n")]
    #[case::kind_leading_space("v1", " Pod")]
    #[case::kind_only_spaces("v1", "   ")]
    #[case::kind_nul("v1", "Pod\0")]
    #[case::kind_escape("v1", "Pod\u{1b}[31m")]
    #[case::kind_kelvin_sign("v1", "\u{212a}ind")]
    #[case::kind_fullwidth_letter("v1", "\u{ff30}od")]
    #[case::kind_cyrillic_er("v1", "\u{420}od")]
    #[case::kind_sharp_s("v1", "Stra\u{df}e")]
    #[case::kind_combining_accent("v1", "Pod\u{301}")]
    #[case::kind_rtl_override("v1", "Pod\u{202e}")]
    #[case::kind_emoji("v1", "Pod\u{1f600}")]
    #[case::kind_1_mib("v1", &"K".repeat(1 << 20))]
    #[case::group_1_mib(&format!("{}/v1", "a".repeat(1 << 20)), "Pod")]
    fn kubernetes_resource_rejects(#[case] api_version: &str, #[case] kind: &str) {
        assert_eq!(KubernetesResource::new(api_version, kind), None);
    }

    #[rstest]
    #[case::traversal_kind("apiVersion: v1\nkind: ../../../../attacker/repo/main/evil\n")]
    #[case::traversal_api_version("apiVersion: ../../x/v1\nkind: Pod\n")]
    #[case::slash_in_version("apiVersion: apps/v1/extra\nkind: Pod\n")]
    #[case::uppercase_group("apiVersion: Apps/v1\nkind: Pod\n")]
    #[case::empty_kind("apiVersion: v1\nkind: ''\n")]
    #[case::empty_api_version("apiVersion: ''\nkind: Pod\n")]
    #[case::escaped_slashes_in_kind("apiVersion: v1\nkind: \"\\x2e\\x2e\\x2f\\x2e\\x2e\"\n")]
    #[case::escaped_newline_in_kind("apiVersion: v1\nkind: \"Pod\\n\"\n")]
    #[case::escaped_tab_in_kind("apiVersion: v1\nkind: \"Po\\td\"\n")]
    #[case::trailing_space_in_kind("apiVersion: v1\nkind: \"Pod \"\n")]
    #[case::escaped_slash_in_api_version("apiVersion: \"apps\\x2f..\\x2fv1\"\nkind: Pod\n")]
    #[case::later_invalid_kind_wins("apiVersion: v1\nkind: Pod\nkind: ../x\n")]
    #[case::later_invalid_api_version_wins("apiVersion: v1\nkind: Pod\napiVersion: ../x\n")]
    #[case::integer_kind("apiVersion: v1\nkind: 123\n")]
    #[case::alias_kind("a: &a Pod\napiVersion: v1\nkind: *a\n")]
    #[case::sequence_root("- apiVersion: v1\n  kind: Pod\n")]
    #[case::scalar_root("Pod\n")]
    fn detect_kubernetes_resource_returns_none_for_invalid_names(#[case] text: &str) {
        let docs = parse_docs(text);
        assert_eq!(detect_kubernetes_resource(&docs), None);
    }

    #[test]
    fn detect_kubernetes_resource_uses_the_last_of_duplicate_keys() {
        let docs = parse_docs("apiVersion: v1\nkind: ../x\nkind: Pod\n");
        assert_eq!(
            detect_kubernetes_resource(&docs),
            Some(KubernetesResource::new("v1", "Pod").expect("valid resource"))
        );
    }

    mod url_properties {
        use proptest::prelude::*;

        use super::*;

        const REPOSITORY_PREFIX: &str =
            "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/";

        /// Free-form text over the characters that matter for URL syntax, mixed
        /// with well-formed values so that both accepted and rejected inputs
        /// are generated.
        fn name_text() -> impl Strategy<Value = String> {
            prop_oneof![
                "[a-zA-Z0-9./%?#:@\\ _\\-\\\\\\n]{0,40}",
                any::<String>(),
                "[a-z0-9]{1,8}(\\.[a-z0-9-]{1,8}){0,3}/v[0-9]{1,2}((alpha|beta)[0-9])?",
                "v[0-9]{1,2}((alpha|beta)[0-9])?",
                "[A-Za-z][A-Za-z0-9]{0,70}",
            ]
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 2048,
                ..ProptestConfig::default()
            })]

            /// Whatever the document says, an accepted resource names a file in
            /// the schema repository: the URL passes the SSRF guard, starts
            /// with the repository prefix, and adds one directory and one file
            /// that the document cannot widen.
            #[test]
            fn accepted_resource_yields_url_inside_the_schema_repository(
                api_version in name_text(),
                kind in name_text(),
            ) {
                let Some(resource) = KubernetesResource::new(&api_version, &kind) else {
                    return Ok(());
                };

                for k8s_version in ["1.29.0", "master"] {
                    let url = kubernetes_schema_url(&resource, k8s_version);
                    let normalized = crate::schema::validate_and_normalize_url(&url)
                        .expect("accepted resource must pass the SSRF guard");

                    prop_assert!(normalized.starts_with(REPOSITORY_PREFIX), "{normalized}");
                    let parsed = tower_lsp::lsp_types::Url::parse(&normalized).expect("normalized URL parses");
                    prop_assert_eq!(parsed.query(), None);
                    prop_assert_eq!(parsed.fragment(), None);
                    let segments: Vec<&str> =
                        parsed.path_segments().expect("hierarchical URL").collect();
                    prop_assert_eq!(segments.len(), 5, "{}", normalized);
                    prop_assert!(segments.iter().all(|s| !s.is_empty() && !s.starts_with('.')));
                    prop_assert_eq!(segments[4].rsplit('.').next(), Some("json"), "{}", normalized);
                    prop_assert!(
                        !normalized[REPOSITORY_PREFIX.len()..].contains(['%', '\\']),
                        "{}",
                        normalized
                    );
                }
            }

            /// Guards the property above against passing vacuously: the
            /// structured strategies must produce resources the validator
            /// accepts.
            #[test]
            fn structured_values_are_accepted(
                api_version in "(([a-z0-9][a-z0-9.-]{0,30}[a-z0-9]|[a-z0-9])/)?v[0-9]{1,3}((alpha|beta)[0-9])?",
                kind in "[A-Z][A-Za-z0-9]{0,62}",
            ) {
                prop_assume!(!api_version.contains("..") && !api_version.contains(".-")
                    && !api_version.contains("-.") && !api_version.contains("/."));
                prop_assert!(KubernetesResource::new(&api_version, &kind).is_some());
            }
        }
    }

    // K8s-11: "Master" (capital M) falls through to versioned branch (case-sensitive match)
    // Different assertion shape (contains check) — left standalone.
    #[test]
    fn should_treat_capitalised_master_as_versioned_prefix() {
        let pod = KubernetesResource::new("v1", "Pod").expect("valid resource");
        let url = kubernetes_schema_url(&pod, "Master");
        assert!(
            url.contains("vMaster-standalone-strict/"),
            "expected vMaster-standalone-strict/ in URL, got: {url}"
        );
    }
}
