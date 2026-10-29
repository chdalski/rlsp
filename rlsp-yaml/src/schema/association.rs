// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};

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

/// A Kubernetes version for the schema repository's directory names: `master`
/// or `MAJOR.MINOR.PATCH`.
///
/// The only ways to obtain a value are [`KubernetesVersion::new`] and
/// [`KubernetesVersion::master`], so [`kubernetes_schema_url`] never receives
/// raw setting text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesVersion(String);

impl KubernetesVersion {
    /// Validate `version`; `None` unless it is exactly `master` or three
    /// dot-separated groups of 1–9 ASCII digits such as `1.29.0`.
    ///
    /// The digit cap keeps a hostile value from lengthening the URL.
    #[must_use]
    pub fn new(version: &str) -> Option<Self> {
        let mut groups = version.split('.');
        let is_group = |group: Option<&str>| {
            group.is_some_and(|digits| {
                (1..=MAX_VERSION_DIGITS).contains(&digits.len())
                    && digits.bytes().all(|b| b.is_ascii_digit())
            })
        };
        let numeric = is_group(groups.next())
            && is_group(groups.next())
            && is_group(groups.next())
            && groups.next().is_none();
        (version == "master" || numeric).then(|| Self(version.to_string()))
    }

    /// The `master` version, which tracks the latest schemas.
    #[must_use]
    pub fn master() -> Self {
        Self("master".to_string())
    }

    /// The validated text, e.g. `master` or `1.29.0`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
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

/// Kubernetes's own API groups that contain a dot, each with the segment the
/// schema repository uses in file names.
///
/// The repository holds no custom-resource schemas, and neither a first-label
/// rule nor a `.k8s.io` suffix rule tells them apart: `apps.kruise.io` would
/// hit a built-in schema and `kustomize.config.k8s.io` would fail a fetch
/// instead of reaching `SchemaStore`.  A new Kubernetes API group therefore
/// needs an entry here before it is auto-detected.
///
/// The segment is the group's first label, except for `internal.apiserver.k8s.io`:
/// the repository names that group's files after the API's Go package.
const BUILTIN_DOTTED_GROUPS: &[(&str, &str)] = &[
    ("admissionregistration.k8s.io", "admissionregistration"),
    ("apiextensions.k8s.io", "apiextensions"),
    ("apiregistration.k8s.io", "apiregistration"),
    ("authentication.k8s.io", "authentication"),
    ("authorization.k8s.io", "authorization"),
    ("certificates.k8s.io", "certificates"),
    ("coordination.k8s.io", "coordination"),
    ("discovery.k8s.io", "discovery"),
    ("events.k8s.io", "events"),
    ("flowcontrol.apiserver.k8s.io", "flowcontrol"),
    ("internal.apiserver.k8s.io", "apiserverinternal"),
    ("lifecycle.k8s.io", "lifecycle"),
    ("networking.k8s.io", "networking"),
    ("node.k8s.io", "node"),
    ("rbac.authorization.k8s.io", "rbac"),
    ("resource.k8s.io", "resource"),
    ("scheduling.k8s.io", "scheduling"),
    ("storage.k8s.io", "storage"),
    ("storagemigration.k8s.io", "storagemigration"),
];

/// The file-name segment of `group`, or `None` for a custom-resource group.
///
/// A group without a dot is one of Kubernetes's own (`apps`, `batch`, ...) and
/// is its own segment; a custom-resource group always contains a dot.
fn schema_file_segment(group: &str) -> Option<&str> {
    if !group.contains('.') {
        return Some(group);
    }
    BUILTIN_DOTTED_GROUPS
        .iter()
        .find(|(builtin, _)| *builtin == group)
        .map(|&(_, segment)| segment)
}

/// Construct a Kubernetes JSON Schema URL for the given resource and version.
///
/// Uses the schema repository at
/// `https://raw.githubusercontent.com/yannh/kubernetes-json-schema`, which
/// holds schemas only for Kubernetes's own API groups.  Returns `None` for a
/// custom resource, so the caller can go on to another schema source instead
/// of fetching a file that does not exist.
///
/// Filename rules:
/// - `kind` is lowercased.
/// - For grouped API versions (e.g. `apps/v1`) the filename is
///   `{kind}-{segment}-{version}.json`, where the segment comes from the
///   group (e.g. `networking` for `networking.k8s.io`).
/// - For core API versions (e.g. `v1`) the filename is
///   `{kind}-{api_version}.json`.
#[must_use]
pub fn kubernetes_schema_url(
    resource: &KubernetesResource,
    k8s_version: &KubernetesVersion,
) -> Option<String> {
    let kind_lower = resource.kind().to_ascii_lowercase();
    let filename = if let Some((group, version)) = resource.api_version().split_once('/') {
        let segment = schema_file_segment(group)?;
        format!("{kind_lower}-{segment}-{version}.json")
    } else {
        format!("{kind_lower}-{}.json", resource.api_version())
    };
    let dir_prefix = if k8s_version.as_str() == "master" {
        "master-standalone-strict".to_string()
    } else {
        format!("v{}-standalone-strict", k8s_version.as_str())
    };
    Some(format!(
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/{dir_prefix}/{filename}"
    ))
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema association — file pattern matching
// ──────────────────────────────────────────────────────────────────────────────

/// Longest glob in a `schemas` entry, in bytes after the leading `!` and `/`
/// are removed.
///
/// With [`MAX_PATH_BYTES`] it bounds one match of a `schemas` entry at
/// 256 × 4096 steps.
pub const MAX_PATTERN_BYTES: usize = 256;

/// States of the glob simulation that are kept on the stack: a `schemas`
/// pattern compiles to at most [`MAX_PATTERN_BYTES`] tokens and the leading
/// `**/`, and the simulation has one more state for the end of the glob.
/// A longer glob takes its states from the heap.
const STACK_STATES: usize = MAX_PATTERN_BYTES + 2;

/// Most globs one `schemas` entry may list.
pub const MAX_PATTERNS_PER_ENTRY: usize = 32;

/// Longest document path that [`match_schema_by_filename`] matches; a longer
/// one matches no `schemas` entry.
pub const MAX_PATH_BYTES: usize = 4096;

/// The globs of one entry: the files its schema applies to, minus the files it
/// excludes.
///
/// Follows yaml-language-server. A leading `!` marks a glob as an exclusion
/// and is removed once, then one leading `/` is removed, and the rest matches
/// at any directory level, as if it began with `**/`. A glob that is empty
/// after that is ignored. No value lacks a glob that includes files.
///
/// [`FilePatterns::new`] builds the entries of the `schemas` setting, which
/// can come from a cloned repository, and enforces the limits for them.
/// [`FilePatterns::without_limits`] builds the entries of the `SchemaStore`
/// catalog with the same rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePatterns {
    include: Vec<Glob>,
    exclude: Vec<Glob>,
}

/// Split `pattern` into whether it is an exclusion and the glob that remains
/// once one leading `!` and then one leading `/` are removed.
fn split(pattern: &str) -> (bool, &str) {
    let (excluded, rest) = pattern
        .strip_prefix('!')
        .map_or((false, pattern), |rest| (true, rest));
    (excluded, rest.strip_prefix('/').unwrap_or(rest))
}

impl FilePatterns {
    /// Sort `patterns` into inclusions and exclusions; `None` when there are
    /// more than [`MAX_PATTERNS_PER_ENTRY`], when one is longer than
    /// [`MAX_PATTERN_BYTES`], or when none includes files.
    ///
    /// The entry is refused as a whole rather than without the faulty glob: a
    /// dropped exclusion would apply the schema to more files than intended.
    /// The limits are checked before any glob is compiled.
    #[must_use]
    pub fn new(patterns: &[String]) -> Option<Self> {
        let within_limits = patterns.len() <= MAX_PATTERNS_PER_ENTRY
            && patterns
                .iter()
                .all(|pattern| split(pattern).1.len() <= MAX_PATTERN_BYTES);
        if !within_limits {
            return None;
        }
        Self::without_limits(patterns.iter().map(String::as_str))
    }

    /// Sort `patterns` into inclusions and exclusions like [`Self::new`], but
    /// without its limits on the number and the length of the globs; `None`
    /// when none includes files.
    ///
    /// For sources whose total size is bounded already, such as the
    /// `SchemaStore` catalog. Never use it for the `schemas` setting: its
    /// globs are untrusted, and only the limits of [`Self::new`] bound the
    /// cost of a match.
    #[must_use]
    pub(crate) fn without_limits<'a>(patterns: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let (mut include, mut exclude) = (Vec::new(), Vec::new());
        for pattern in patterns {
            let (excluded, rest) = split(pattern);
            if rest.is_empty() {
                continue;
            }
            let glob = Glob::anywhere(rest);
            if excluded {
                exclude.push(glob);
            } else {
                include.push(glob);
            }
        }
        (!include.is_empty()).then_some(Self { include, exclude })
    }

    /// Whether the schema applies to the document at `path`: some inclusion
    /// matches and no exclusion does. The order of the globs does not matter.
    ///
    /// The cost of a match grows with the length of `path`, which this does
    /// not limit. For a path of unknown length use
    /// [`match_schema_by_filename`], which refuses a path longer than
    /// [`MAX_PATH_BYTES`].
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        self.include.iter().any(|glob| glob.is_match(path))
            && !self.exclude.iter().any(|glob| glob.is_match(path))
    }
}

/// Return the schema URL of the first entry of `schemas` whose patterns match
/// `filename`, or `None` if no entry matches.
///
/// `schemas` is ordered by URL, so the entry whose URL sorts first wins among
/// overlapping entries. A `filename` longer than [`MAX_PATH_BYTES`] matches no
/// entry. See [`FilePatterns`] for the pattern rules.
#[must_use]
pub fn match_schema_by_filename(
    filename: &str,
    schemas: &BTreeMap<String, FilePatterns>,
) -> Option<String> {
    if filename.len() > MAX_PATH_BYTES {
        return None;
    }
    schemas
        .iter()
        .find(|(_, patterns)| patterns.matches(filename))
        .map(|(url, _)| url.clone())
}

/// Return `true` if `pattern` matches `text`; compiles `pattern` on every call.
#[cfg(test)]
fn glob_matches(pattern: &str, text: &str) -> bool {
    Glob::new(pattern).is_match(text)
}

/// One element of a compiled glob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Literal(u8),
    /// `*`: any run of bytes other than `/`.
    Star,
    /// `**`, and the `/` after it if there is one: any run of bytes.
    /// With `at_segment_start` the run has to end where a path segment begins.
    DoubleStar {
        at_segment_start: bool,
    },
}

/// A glob compiled to tokens and matched without backtracking, so a match
/// costs at most the glob length times the text length however many
/// wildcards the glob has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Glob {
    tokens: Vec<Token>,
}

impl Glob {
    /// A glob in which `**/` may end only at the start of a path segment.
    fn anywhere(pattern: &str) -> Self {
        let leading = Token::DoubleStar {
            at_segment_start: true,
        };
        Self {
            tokens: std::iter::once(leading)
                .chain(tokenize(pattern, true))
                .collect(),
        }
    }

    /// A glob in which `**/` may end at any byte; the tests use it to pin the
    /// matcher on plain globs.
    ///
    /// Supported glob syntax:
    /// - `*` matches any sequence of characters that does not include `/`
    /// - `**` matches any sequence of characters including `/`
    /// - All other characters match literally
    #[cfg(test)]
    pub(super) fn new(pattern: &str) -> Self {
        Self {
            tokens: tokenize(pattern, false).collect(),
        }
    }

    /// Whether the text matches: the glob ends where the text does, and the
    /// tokens the text can have reached end at the end of the glob.
    ///
    /// The states of the simulation live on the stack for every glob that a
    /// `schemas` pattern can compile to; a longer glob, which only the
    /// `SchemaStore` catalog can bring, takes them from the heap.
    pub(super) fn is_match(&self, candidate: &str) -> bool {
        let text = candidate.as_bytes();
        if !self.ends_with_trailing_literals(text) {
            return false;
        }
        let states = self.tokens.len() + 1;
        let mut stack = ([false; STACK_STATES], [false; STACK_STATES]);
        if let (Some(reached), Some(next)) = (stack.0.get_mut(..states), stack.1.get_mut(..states))
        {
            return self.simulate(text, reached, next);
        }
        self.simulate(text, &mut vec![false; states], &mut vec![false; states])
    }

    /// The literal tokens at the end of the glob match exactly the last bytes
    /// of the text, whatever precedes them: a literal consumes one byte, and
    /// only wildcards can match nothing. A necessary condition for a match,
    /// checked in as many steps as the glob has trailing literal bytes or the
    /// text has bytes, whichever is fewer, before the simulation that costs
    /// glob length times text length.
    fn ends_with_trailing_literals(&self, text: &[u8]) -> bool {
        let mut text_end = text.iter().rev();
        self.tokens
            .iter()
            .rev()
            .map_while(|token| match token {
                Token::Literal(literal) => Some(literal),
                Token::Star | Token::DoubleStar { .. } => None,
            })
            .all(|literal| text_end.next() == Some(literal))
    }

    /// Track the tokens the text so far can have reached; the text matches
    /// when the end of the glob is among them at the end of the text.
    /// `reached` and `next` hold one state per token and one for the end.
    fn simulate<'a>(
        &self,
        text: &[u8],
        mut reached: &'a mut [bool],
        mut next: &'a mut [bool],
    ) -> bool {
        debug_assert_eq!(reached.len(), self.tokens.len() + 1);
        debug_assert_eq!(next.len(), reached.len());
        mark(reached, 0);
        self.skip_empty_runs(reached, text, 0);
        for (position, &byte) in text.iter().enumerate() {
            next.fill(false);
            for (index, token) in self.tokens.iter().enumerate() {
                if !reached.get(index).copied().unwrap_or(false) {
                    continue;
                }
                match *token {
                    Token::Literal(literal) if literal == byte => mark(next, index + 1),
                    Token::Star if byte != b'/' => mark(next, index),
                    Token::DoubleStar { .. } => mark(next, index),
                    Token::Literal(_) | Token::Star => {}
                }
            }
            self.skip_empty_runs(next, text, position + 1);
            std::mem::swap(&mut reached, &mut next);
            if !reached.contains(&true) {
                return false;
            }
        }
        reached.last().copied().unwrap_or(false)
    }

    /// A wildcard may match nothing: from each reached wildcard, also reach
    /// the token after it, where `position` in `text` allows it.
    fn skip_empty_runs(&self, reached: &mut [bool], text: &[u8], position: usize) {
        let at_segment_start = position == 0 || text.get(position - 1) == Some(&b'/');
        for (index, token) in self.tokens.iter().enumerate() {
            let can_end_here = match *token {
                Token::Literal(_) => false,
                Token::Star => true,
                Token::DoubleStar {
                    at_segment_start: required,
                } => at_segment_start || !required,
            };
            if can_end_here && reached.get(index).copied().unwrap_or(false) {
                mark(reached, index + 1);
            }
        }
    }
}

fn mark(states: &mut [bool], index: usize) {
    if let Some(state) = states.get_mut(index) {
        *state = true;
    }
}

/// Split `pattern` into tokens. With `anchored`, a `**/` ends only at the
/// start of a path segment.
fn tokenize(pattern: &str, anchored: bool) -> impl Iterator<Item = Token> + '_ {
    let mut bytes = pattern.bytes().peekable();
    std::iter::from_fn(move || {
        let byte = bytes.next()?;
        Some(match byte {
            b'*' if bytes.next_if_eq(&b'*').is_some() => Token::DoubleStar {
                at_segment_start: bytes.next_if_eq(&b'/').is_some() && anchored,
            },
            b'*' => Token::Star,
            literal => Token::Literal(literal),
        })
    })
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
    // FilePatterns + match_schema_by_filename
    // ══════════════════════════════════════════════════════════════════════════

    fn parsed_version(version: &str) -> KubernetesVersion {
        KubernetesVersion::new(version).expect("valid version")
    }

    fn file_patterns(globs: &[&str]) -> FilePatterns {
        let globs: Vec<String> = globs.iter().map(ToString::to_string).collect();
        FilePatterns::new(&globs).expect("globs include a file")
    }

    fn schemas(entries: &[(&str, &[&str])]) -> BTreeMap<String, FilePatterns> {
        entries
            .iter()
            .map(|(url, globs)| ((*url).to_string(), file_patterns(globs)))
            .collect()
    }

    #[rstest]
    #[case::exact_filename("config.yaml", "/work/config.yaml")]
    #[case::exact_filename_at_the_root("config.yaml", "/config.yaml")]
    #[case::extension_glob_below_directories("*.yaml", "/a/b/c.yaml")]
    #[case::workflow_glob_below_a_directory(
        ".github/workflows/*.yml",
        "/home/u/proj/.github/workflows/ci.yml"
    )]
    #[case::absolute_path_still_matches("/work/config.yaml", "/work/config.yaml")]
    #[case::leading_slash_is_removed("/config.yaml", "/a/config.yaml")]
    #[case::only_one_leading_slash_is_removed("//x", "/a//x")]
    #[case::double_star_directory("configs/**/*.yaml", "/p/configs/nested/file.yaml")]
    #[case::leading_double_star("**/*.yaml", "/a/b.yaml")]
    #[case::leading_double_star_at_the_root("**/b.yaml", "/b.yaml")]
    #[case::single_star_only("*", "/a/b")]
    #[case::percent_encoded_path_is_not_decoded("a%20b.yaml", "/dir/a%20b.yaml")]
    #[case::empty_segments_are_not_normalized("b/x.yaml", "/a//b/x.yaml")]
    #[case::bare_name_after_an_empty_segment("x.yaml", "/a//b/x.yaml")]
    #[case::star_matches_a_dot_file("*.yaml", "/a/.yaml")]
    #[case::double_star_matches_an_empty_last_segment("**", "/a/")]
    #[case::drive_letter_path("*.yaml", "/C:/Users/me/a.yaml")]
    #[case::drive_letter_path_with_a_directory_glob(
        ".github/workflows/*.yml",
        "/C:/repo/.github/workflows/ci.yml"
    )]
    #[case::exclamation_mark_after_a_slash_is_literal("/!x", "/d/!x")]
    #[case::double_star_with_slashes_on_both_sides("/**/x.yaml", "/r/s/x.yaml")]
    #[case::double_star_with_slashes_matches_the_root("/**/x.yaml", "/x.yaml")]
    #[case::directory_glob_at_the_root(".github/workflows/*.yml", "/.github/workflows/ci.yml")]
    #[case::multibyte_literal("é/*.yaml", "/dir/é/ü.yaml")]
    #[case::double_star_slash_before_a_one_byte_name("**/c", "c")]
    fn file_patterns_match(#[case] glob: &str, #[case] path: &str) {
        assert!(file_patterns(&[glob]).matches(path), "{glob} on {path}");
    }

    #[rstest]
    #[case::other_extension("*.json", "/a/myfile.yaml")]
    #[case::other_name_with_the_same_length("x.yaml", "/a/x.yml")]
    #[case::name_is_a_suffix_of_the_segment("config.yaml", "/a/my-config.yaml")]
    #[case::name_is_a_suffix_of_the_segment_after_a_double_star("**/foo.yaml", "/a/barfoo.yaml")]
    #[case::directory_is_a_suffix_of_the_segment("config/*.yaml", "/x/myconfig/a.yaml")]
    #[case::workflow_dir_is_a_suffix_of_the_segment(
        ".github/workflows/*.yml",
        "/home/my.github/workflows/ci.yml"
    )]
    #[case::second_slash_is_not_removed("//x", "/a/x")]
    #[case::single_star_stops_at_a_slash("a/*", "/a/b/c")]
    #[case::case_differs("*.YAML", "/a/b.yaml")]
    #[case::literal_does_not_match_a_percent_decoded_path("a b.yaml", "/dir/a%20b.yaml")]
    #[case::three_empty_segments_are_not_one("a/b/x.yaml", "/a//b/x.yaml")]
    #[case::trailing_slash("*.yaml", "/a/c.yaml/")]
    #[case::directory_glob_does_not_reach_a_subdirectory(
        ".github/workflows/*.yml",
        "/repo/.github/workflows/sub/ci.yml"
    )]
    #[case::backslashes_are_literal(".github\\workflows\\*.yml", "/r/.github/workflows/ci.yml")]
    #[case::exclamation_mark_after_a_slash_is_not_an_exclusion("/!x", "/d/x")]
    #[case::empty_path("*.yaml", "")]
    #[case::trailing_double_star_slash_needs_a_segment("**/", "/a/b")]
    fn file_patterns_do_not_match(#[case] glob: &str, #[case] path: &str) {
        assert!(!file_patterns(&[glob]).matches(path), "{glob} on {path}");
    }

    #[rstest]
    #[case::exclusion_hits(&["*.yaml", "!skip.yaml"], "/a/skip.yaml", false)]
    #[case::exclusion_misses(&["*.yaml", "!skip.yaml"], "/a/other.yaml", true)]
    #[case::exclusion_first(&["!skip.yaml", "*.yaml"], "/a/skip.yaml", false)]
    #[case::exclusion_first_other_file(&["!skip.yaml", "*.yaml"], "/a/other.yaml", true)]
    #[case::exclusion_is_anywhere(&["**", "!tests/*"], "/p/tests/a.yaml", false)]
    #[case::exclusion_slash_removed(&["**", "!/x"], "/a/x", false)]
    #[case::exclusion_slash_removed_other_file(&["**", "!/x"], "/a/y", true)]
    #[case::only_one_exclamation_mark_is_removed_hit(&["**", "!!x"], "/a/!x", false)]
    #[case::only_one_exclamation_mark_is_removed_miss(&["**", "!!x"], "/a/x", true)]
    #[case::bare_exclamation_mark_is_ignored(&["x", "!"], "/a/x", true)]
    #[case::bare_exclamation_mark_is_ignored_other_file(&["x", "!"], "/a/y", false)]
    #[case::bare_slash_is_ignored(&["x", "/"], "/a/x", true)]
    #[case::empty_glob_is_ignored(&["x", ""], "/a/x", true)]
    #[case::second_inclusion_applies(&["a.yaml", "b.yaml"], "/p/b.yaml", true)]
    #[case::no_inclusion_hits(&["a.yaml", "b.yaml"], "/p/c.yaml", false)]
    #[case::several_exclusions(&["**", "!a.yaml", "!b.yaml"], "/p/b.yaml", false)]
    #[case::exclusion_follows_the_segment_rules(&["*.yaml", "!config.yaml"], "/p/my-config.yaml", true)]
    #[case::exclusion_follows_the_segment_rules_on_the_bare_name(&["*.yaml", "!config.yaml"], "/p/config.yaml", false)]
    #[case::exclusion_wins_over_several_inclusions(&["a.yaml", "*.yaml", "!a.yaml"], "/c/a.yaml", false)]
    #[case::slash_then_bang_is_a_literal_inclusion(&["/!x"], "/d/!x", true)]
    #[case::slash_then_bang_does_not_include_the_bare_name(&["/!x"], "/d/x", false)]
    fn file_patterns_include_and_exclude(
        #[case] globs: &[&str],
        #[case] path: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(
            file_patterns(globs).matches(path),
            expected,
            "{globs:?} on {path}"
        );
    }

    #[test]
    fn file_patterns_ignore_the_order_of_their_globs() {
        let forward = file_patterns(&["*.yaml", "!a.yaml", "b/*.yaml"]);
        let reverse = file_patterns(&["b/*.yaml", "!a.yaml", "*.yaml"]);
        for path in [
            "/a.yaml",
            "/x/a.yaml",
            "/x/b/a.yaml",
            "/x/c.yaml",
            "/x.json",
        ] {
            assert_eq!(forward.matches(path), reverse.matches(path), "{path}");
        }
    }

    #[rstest]
    #[case::empty_array(&[])]
    #[case::empty_string(&[""])]
    #[case::only_a_slash(&["/"])]
    #[case::only_a_bare_exclamation_mark(&["!"])]
    #[case::only_a_slash_exclusion(&["!/"])]
    #[case::only_exclusions(&["!x", "!y"])]
    #[case::only_empty_globs(&["", "/", "!", "!/"])]
    fn file_patterns_refuse_globs_that_include_no_file(#[case] globs: &[&str]) {
        let owned: Vec<String> = globs.iter().map(ToString::to_string).collect();
        assert_eq!(FilePatterns::new(&owned), None);
        assert_eq!(FilePatterns::without_limits(globs.iter().copied()), None);
    }

    #[rstest]
    #[case::exclusion_first(&["!skip.yaml", "*.yaml"])]
    #[case::exclusion_last(&["*.yaml", "!skip.yaml"])]
    #[case::only_one_exclamation_mark_is_removed(&["**", "!!x"])]
    #[case::only_one_slash_is_removed(&["//x", "!//y"])]
    #[case::slash_then_bang_is_literal(&["/!x", "!/!y"])]
    #[case::bare_exclamation_mark_is_ignored(&["x", "!"])]
    #[case::bare_slash_is_ignored(&["x", "/"])]
    #[case::empty_glob_is_ignored(&["x", "", "!/"])]
    fn without_limits_builds_what_new_builds_within_the_limits(#[case] globs: &[&str]) {
        let owned: Vec<String> = globs.iter().map(ToString::to_string).collect();
        let patterns = FilePatterns::new(&owned);
        assert!(patterns.is_some());
        assert_eq!(
            FilePatterns::without_limits(globs.iter().copied()),
            patterns
        );
    }

    #[rstest]
    #[case::more_than_the_count_limit(vec!["x".to_string(); MAX_PATTERNS_PER_ENTRY + 1])]
    #[case::glob_over_the_byte_limit(vec!["x".repeat(MAX_PATTERN_BYTES + 1)])]
    #[case::exclusion_over_the_byte_limit(vec!["x".to_string(), "!".to_string() + &"y".repeat(MAX_PATTERN_BYTES + 1)])]
    #[case::multibyte_glob_over_the_byte_limit(vec!["\u{e9}".repeat(MAX_PATTERN_BYTES / 2 + 1)])]
    fn without_limits_accepts_what_new_refuses(#[case] globs: Vec<String>) {
        assert_eq!(FilePatterns::new(&globs), None);
        assert!(FilePatterns::without_limits(globs.iter().map(String::as_str)).is_some());
    }

    #[test]
    fn without_limits_matches_and_excludes_with_a_glob_over_the_byte_limit() {
        let long = "a".repeat(MAX_PATTERN_BYTES + 44) + "*b";
        let including = FilePatterns::without_limits([long.as_str()]).expect("includes a file");
        let excluding =
            FilePatterns::without_limits(["**", &format!("!{long}")]).expect("includes a file");
        let hit = "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES + 44) + "zzb";
        let simulated_miss = "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES + 43) + "b";
        let literal_miss = "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES + 44) + "zzc";
        for (path, expected) in [
            (&hit, true),
            (&simulated_miss, false),
            (&literal_miss, false),
        ] {
            assert_eq!(anywhere_reference(&long, path), expected, "the reference");
            assert_eq!(including.matches(path), expected, "{path}");
            assert_eq!(excluding.matches(path), !expected, "excluding {path}");
        }
    }

    #[test]
    fn without_limits_matches_a_hundred_thousand_globs_and_a_huge_glob() {
        let many = vec!["x"; 100_000];
        let patterns = FilePatterns::without_limits(many).expect("includes a file");
        assert!(patterns.matches("/a/x"));
        assert!(!patterns.matches("/a/y"));
        let huge = "a**".repeat(500) + "b";
        let patterns = FilePatterns::without_limits([huge.as_str()]).expect("includes a file");
        assert!(patterns.matches(&("/".to_string() + &"a".repeat(500) + "b")));
        assert!(!patterns.matches(&("/".to_string() + &"a".repeat(500) + "c")));
    }

    #[test]
    fn file_patterns_accept_the_limits_and_refuse_one_more() {
        let globs = |count: usize, length: usize| vec!["x".repeat(length); count];
        assert!(FilePatterns::new(&globs(MAX_PATTERNS_PER_ENTRY, MAX_PATTERN_BYTES)).is_some());
        assert_eq!(
            FilePatterns::new(&globs(MAX_PATTERNS_PER_ENTRY + 1, 1)),
            None
        );
        assert_eq!(FilePatterns::new(&globs(1, MAX_PATTERN_BYTES + 1)), None);
    }

    #[test]
    fn file_patterns_measure_the_length_of_a_glob_without_its_prefixes() {
        let at_the_limit = "x".repeat(MAX_PATTERN_BYTES);
        let over_the_limit = "x".repeat(MAX_PATTERN_BYTES + 1);
        assert!(FilePatterns::new(&["a".into(), format!("!/{at_the_limit}")]).is_some());
        assert!(FilePatterns::new(&["a".into(), format!("!/{over_the_limit}")]).is_none());
    }

    #[test]
    fn file_patterns_measure_the_length_of_a_glob_in_bytes() {
        let two_bytes = "\u{e9}";
        assert!(FilePatterns::new(&[two_bytes.repeat(MAX_PATTERN_BYTES / 2)]).is_some());
        assert_eq!(
            FilePatterns::new(&[two_bytes.repeat(MAX_PATTERN_BYTES / 2 + 1)]),
            None
        );
    }

    #[test]
    fn file_patterns_count_every_listed_glob_not_only_the_usable_ones() {
        let mut globs = vec![String::new(); MAX_PATTERNS_PER_ENTRY];
        globs.push("x".to_string());
        assert_eq!(FilePatterns::new(&globs), None);
        globs.remove(0);
        assert!(FilePatterns::new(&globs).is_some());
    }

    #[test]
    fn match_schema_by_filename_matches_a_huge_path_to_nothing_without_work() {
        let path = format!("/{}.yaml", "a".repeat(10 << 20));
        let entries = schemas(&[("https://a.test/s.json", &["**"])]);
        assert_eq!(match_schema_by_filename(&path, &entries), None);
    }

    #[test]
    fn the_shared_glob_matcher_has_no_path_limit() {
        let path = "a/".repeat(MAX_PATH_BYTES);
        assert!(glob_matches("**", &path));
        assert!(glob_matches("**/a/", &path));
    }

    #[test]
    fn file_patterns_refuse_a_whole_entry_for_one_glob_over_the_limit() {
        let globs = [
            "x".to_string(),
            "!".to_string() + &"y".repeat(MAX_PATTERN_BYTES + 1),
        ];
        assert_eq!(FilePatterns::new(&globs), None);
    }

    #[test]
    fn match_schema_by_filename_matches_a_path_up_to_the_limit_and_no_longer() {
        let entries = schemas(&[
            ("https://a.test/s.json", &["*.yaml"]),
            ("https://b.test/s.json", &["**"]),
        ]);
        let path = |length: usize| format!("/{}.yaml", "a".repeat(length - "/.yaml".len()));
        assert_eq!(
            match_schema_by_filename(&path(MAX_PATH_BYTES), &entries),
            Some("https://a.test/s.json".to_string())
        );
        assert_eq!(
            match_schema_by_filename(&path(MAX_PATH_BYTES + 1), &entries),
            None
        );
        assert_eq!(
            match_schema_by_filename(&path(MAX_PATH_BYTES + 1), &BTreeMap::new()),
            None
        );
    }

    #[test]
    fn file_patterns_matches_applies_no_path_limit() {
        let path = |extension: &str| format!("/{}{extension}", "a".repeat(MAX_PATH_BYTES));
        let patterns = file_patterns(&["**", "!*.json"]);
        assert!(patterns.matches(&path(".yaml")));
        assert!(!patterns.matches(&path(".json")));
    }

    /// Run `work` on a thread and fail, instead of hanging, if it does not
    /// finish: a backtracking matcher takes years on the globs below.
    fn finishes_within_thirty_seconds(work: impl FnOnce() + Send + 'static) {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            work();
            let _ = sender.send(());
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(30))
                .is_ok(),
            "matching did not finish"
        );
    }

    /// Each path ends in the glob's trailing literal, so the simulation runs
    /// over all of it; a path that does not is rejected before. Each path comes
    /// with its result for a `schemas` pattern (matched anywhere) and for the
    /// plain matcher: no path with the trailing literal of the first two globs
    /// is missed by the plain matcher, whose `**/` may end at any byte.
    #[rstest]
    #[case::stacked_double_stars(
        format!("{}x", "**/".repeat(80)),
        "x",
        vec![("a".repeat(MAX_PATH_BYTES - 1) + "x", false, true)],
    )]
    #[case::stacked_double_stars_without_slashes(
        format!("{}x", "**".repeat(120)),
        "x",
        vec![("a".repeat(MAX_PATH_BYTES - 1) + "x", true, true)],
    )]
    #[case::stars_between_letters(
        format!("{}b", "*a".repeat(120)),
        "ab",
        vec![
            ("a/".repeat(MAX_PATH_BYTES / 2 - 1) + "ab", false, false),
            ("a".repeat(MAX_PATH_BYTES - 2) + "ab", true, true),
        ],
    )]
    #[case::stars_between_slashes(
        format!("{}b", "*/".repeat(120)),
        "/b",
        vec![
            ("a".repeat(MAX_PATH_BYTES - 2) + "/b", false, false),
            ("a/".repeat(MAX_PATH_BYTES / 2 - 1) + "b", true, false),
        ],
    )]
    #[case::double_stars_between_letters(
        format!("{}b", "a**".repeat(80)),
        "b",
        vec![
            ("a".repeat(79) + &"/".repeat(MAX_PATH_BYTES - 80) + "b", false, false),
            ("a".repeat(MAX_PATH_BYTES - 1) + "b", true, true),
        ],
    )]
    fn matching_ends_for_a_glob_that_made_backtracking_explode(
        #[case] glob: String,
        #[case] trailing_literal: &'static str,
        #[case] paths: Vec<(String, bool, bool)>,
    ) {
        for (path, ..) in &paths {
            assert!(path.ends_with(trailing_literal), "{path}");
        }
        finishes_within_thirty_seconds(move || {
            let patterns = file_patterns(&[&glob]);
            for (path, anywhere, plain) in &paths {
                assert_eq!(patterns.matches(path), *anywhere, "anywhere: {path}");
                assert_eq!(glob_matches(&glob, path), *plain, "plain: {path}");
            }
        });
    }

    #[rstest]
    #[case::first_url_wins(
        "/a/test.yaml",
        &[("https://example.com/first.json", &["*.yaml"][..]), ("https://example.com/second.json", &["*.yaml"])],
        Some("https://example.com/first.json")
    )]
    #[case::only_the_matching_entry_applies(
        "/a/test.yaml",
        &[("https://a.test/json.json", &["*.json"][..]), ("https://b.test/yaml.json", &["*.yaml"])],
        Some("https://b.test/yaml.json")
    )]
    #[case::an_excluded_entry_yields_to_the_next(
        "/a/test.yaml",
        &[("https://a.test/s.json", &["*.yaml", "!test.yaml"][..]), ("https://b.test/s.json", &["*.yaml"])],
        Some("https://b.test/s.json")
    )]
    #[case::every_entry_excludes_the_file(
        "/a/test.yaml",
        &[("https://a.test/s.json", &["*.yaml", "!test.yaml"][..]), ("https://b.test/s.json", &["**", "!/a/test.yaml"])],
        None
    )]
    #[case::no_entry(
        "/a/test.yaml",
        &[],
        None
    )]
    #[case::no_entry_matches(
        "/a/my-config.yaml",
        &[("https://a.test/s.json", &["config.yaml"][..])],
        None
    )]
    fn match_schema_by_filename_picks_the_first_matching_entry(
        #[case] filename: &str,
        #[case] entries: &[(&str, &[&str])],
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(
            match_schema_by_filename(filename, &schemas(entries)).as_deref(),
            expected
        );
    }

    #[test]
    fn match_schema_by_filename_ignores_the_order_the_entries_were_added_in() {
        let urls = [
            "https://c.test/s.json",
            "https://a.test/s.json",
            "https://b.test/s.json",
        ];
        for rotation in 0..urls.len() {
            let mut rotated = urls;
            rotated.rotate_left(rotation);
            let schemas: BTreeMap<_, _> = rotated
                .iter()
                .map(|url| ((*url).to_string(), file_patterns(&["*.yaml"])))
                .collect();
            assert_eq!(
                match_schema_by_filename("/x.yaml", &schemas).as_deref(),
                Some("https://a.test/s.json")
            );
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // glob_matches
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::literal("a.yaml", "a.yaml", true)]
    #[case::literal_mismatch("a.yaml", "b.yaml", false)]
    #[case::star_in_a_segment("*.yaml", "a.yaml", true)]
    #[case::star_stops_at_a_slash("*.yaml", "a/b.yaml", false)]
    #[case::star_may_match_nothing("a*b", "ab", true)]
    #[case::double_star_crosses_slashes("**/c.yaml", "a/b/c.yaml", true)]
    #[case::double_star_may_match_nothing("**/c.yaml", "c.yaml", true)]
    #[case::double_star_mid_pattern("a/**/c", "a/b/b/c", true)]
    #[case::double_star_ends_inside_a_segment("**/c.yaml", "a/bc.yaml", true)]
    #[case::double_star_alone("**", "a/b/c", true)]
    #[case::double_star_matches_empty_text("**", "", true)]
    #[case::star_matches_empty_text("*", "", true)]
    #[case::empty_pattern_matches_empty_text("", "", true)]
    #[case::empty_pattern_rejects_text("", "a", false)]
    #[case::pattern_longer_than_text("abc", "ab", false)]
    #[case::text_longer_than_pattern("ab", "abc", false)]
    #[case::three_stars_are_a_double_star_and_a_star("***", "a/b", true)]
    #[case::the_plain_matcher_adds_no_implicit_double_star("c.yaml", "a/c.yaml", false)]
    #[case::leading_exclamation_mark_is_literal("!x", "!x", true)]
    #[case::leading_exclamation_mark_is_not_an_exclusion("!x", "y", false)]
    #[case::triple_star_between_letters("a***b", "a/x/b", true)]
    #[case::double_star_inside_a_name("a**b", "a/x/b", true)]
    #[case::trailing_double_star_matches_nothing_after_a_slash("a/**", "a/", true)]
    #[case::question_mark_is_literal("?.yaml", "a.yaml", false)]
    #[case::brackets_are_literal("[a].yaml", "a.yaml", false)]
    #[case::multibyte_literal("é.yaml", "é.yaml", true)]
    #[case::star_matches_multibyte("*", "é", true)]
    #[case::star_matches_a_dot_file("*.yaml", ".yaml", true)]
    #[case::star_matches_a_hidden_name("*", ".hidden", true)]
    #[case::one_slash_after_a_double_star_is_swallowed("**//c", "a/c", true)]
    #[case::a_second_slash_after_a_double_star_is_not("**//c", "ac", false)]
    #[case::trailing_literal_is_the_whole_text("*abc", "abc", true)]
    #[case::text_shorter_than_the_trailing_literal("*abc", "bc", false)]
    #[case::text_shorter_than_the_trailing_literal_after_a_double_star("**abc", "bc", false)]
    #[case::last_byte_differs("*ab", "xc", false)]
    #[case::first_byte_of_the_trailing_literal_differs("*ab", "cb", false)]
    #[case::middle_byte_of_the_trailing_literal_differs("*abc", "xaxc", false)]
    #[case::a_repeated_byte_needs_both_copies("*aa", "a", false)]
    #[case::a_repeated_byte_after_a_prefix("*aa", "baa", true)]
    #[case::a_repeated_byte_after_a_repeat("*aa", "aaa", true)]
    #[case::star_then_literal("*b", "ab", true)]
    #[case::star_then_literal_fails_in_the_simulation("*b", "a/b", false)]
    #[case::trailing_literal_fits_but_the_head_fails("a*c", "xbc", false)]
    #[case::trailing_literal_fits_and_the_head_matches("a*c", "abc", true)]
    #[case::a_trailing_star_rejects_a_slash("a*", "a/b", false)]
    #[case::a_trailing_star_matches_a_name("a*", "ab", true)]
    #[case::a_trailing_double_star_matches_a_slash("a**", "a/b", true)]
    #[case::a_double_star_slash_may_end_the_glob_on_nothing("a**/", "a", true)]
    #[case::a_double_star_slash_may_end_the_glob_on_a_slash("a**/", "a/", true)]
    #[case::a_double_star_slash_may_end_the_glob_on_a_name("a**/", "ax/", true)]
    #[case::a_glob_of_only_wildcards("**/", "a/b", true)]
    #[case::the_slash_folded_into_a_double_star_is_not_literal("a**/b", "ab", true)]
    #[case::the_slash_folded_into_a_double_star_may_match("a**/b", "a/b", true)]
    #[case::the_second_slash_is_literal_and_missing("a**//b", "ab", false)]
    #[case::the_second_slash_is_literal_and_present("a**//b", "a/b", true)]
    #[case::a_slash_in_the_trailing_literal_matches("*/c", "a/c", true)]
    #[case::a_slash_in_the_trailing_literal_is_missing("*/c", "ac", false)]
    #[case::multibyte_trailing_literal_matches("*é", "xé", true)]
    #[case::multibyte_trailing_literal_misses("*é", "xe", false)]
    #[case::multibyte_literal_with_only_the_last_byte_in_common("*é", "\u{2a9}", false)]
    #[case::multibyte_trailing_literal_longer_than_the_text("*éé", "é", false)]
    #[case::multibyte_trailing_literal_before_an_ascii_tail("*é.yaml", "é.yaml", true)]
    fn glob_matches_follows_the_documented_syntax(
        #[case] pattern: &str,
        #[case] text: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(glob_matches(pattern, text), expected, "{pattern} on {text}");
    }

    /// The matcher this module used before it stopped backtracking, kept to
    /// check that the new one agrees with it.
    fn backtracking_glob_matches(pattern: &[u8], text: &[u8]) -> bool {
        match (pattern.first(), text.first()) {
            (None, None) => true,
            (Some(&b'*'), _) if pattern.get(1) == Some(&b'*') => {
                let rest = pattern.get(2..).unwrap_or(&[]);
                let rest = rest.strip_prefix(b"/").unwrap_or(rest);
                (0..=text.len()).any(|i| backtracking_glob_matches(rest, &text[i..]))
            }
            (Some(&b'*'), _) => {
                let rest = &pattern[1..];
                (0..=text.len()).any(|i| {
                    !text[..i].contains(&b'/') && backtracking_glob_matches(rest, &text[i..])
                })
            }
            (Some(&p), Some(&t)) => p == t && backtracking_glob_matches(&pattern[1..], &text[1..]),
            (None, Some(_)) | (Some(_), None) => false,
        }
    }

    /// What a `schemas` pattern without `**` matches: the glob matches the
    /// whole path or what follows one of its slashes.
    fn anywhere_reference(glob: &str, text: &str) -> bool {
        let starts = std::iter::once(0).chain(text.match_indices('/').map(|(i, _)| i + 1));
        starts
            .into_iter()
            .any(|start| backtracking_glob_matches(glob.as_bytes(), &text.as_bytes()[start..]))
    }

    /// Globs of `tokens` tokens: `tokens - 2` letters, a star and a letter.
    /// `Glob::new` gives them `tokens + 1` states; the stack buffers of
    /// `Glob::is_match` hold [`STACK_STATES`].
    #[rstest]
    #[case::one_state_below_the_stack_bound(STACK_STATES - 2)]
    #[case::at_the_stack_bound(STACK_STATES - 1)]
    #[case::one_state_above_the_stack_bound(STACK_STATES)]
    fn glob_matches_agrees_with_the_backtracking_matcher_around_the_stack_bound(
        #[case] tokens: usize,
    ) {
        let glob = "a".repeat(tokens - 2) + "*b";
        let texts = [
            ("a".repeat(tokens - 2) + "b", true),
            ("a".repeat(tokens - 2) + "zzb", true),
            ("a".repeat(tokens - 3) + "b", false),
            ("a".repeat(tokens - 2) + "/b", false),
            ("a".repeat(tokens - 2) + "c", false),
        ];
        for (text, expected) in &texts {
            assert_eq!(
                backtracking_glob_matches(glob.as_bytes(), text.as_bytes()),
                *expected,
                "the reference on {text}"
            );
            assert_eq!(
                glob_matches(&glob, text),
                *expected,
                "{tokens} tokens on {text}"
            );
        }
    }

    #[test]
    fn one_glob_gives_the_same_results_on_every_call() {
        let glob = Glob::new("a*b");
        for _ in 0..3 {
            assert!(glob.is_match("axxb"));
            assert!(!glob.is_match("axxc"));
            assert!(!glob.is_match("a/b"));
            assert!(glob.is_match("ab"));
        }
    }

    /// A pattern of `MAX_PATTERN_BYTES` bytes has `MAX_PATTERN_BYTES` tokens
    /// at most, one more with the leading `**/`, and one more state: exactly
    /// the stack buffers of `Glob::is_match`.
    #[rstest]
    #[case::letters_and_a_star(
        "a".repeat(MAX_PATTERN_BYTES - 2) + "*b",
        vec![
            "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES - 2) + "zzb",
            "a".repeat(MAX_PATTERN_BYTES - 2) + "b",
        ],
        vec![
            "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES - 3) + "b",
            "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES - 2) + "/b",
            "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES - 2) + "c",
        ],
    )]
    #[case::multibyte_letters(
        "\u{e9}".repeat(MAX_PATTERN_BYTES / 2),
        vec!["/d/".to_string() + &"\u{e9}".repeat(MAX_PATTERN_BYTES / 2)],
        vec!["/d/".to_string() + &"\u{e9}".repeat(MAX_PATTERN_BYTES / 2 - 1) + "\u{e8}"],
    )]
    fn file_patterns_at_the_pattern_limit_agree_with_the_backtracking_matcher(
        #[case] pattern: String,
        #[case] matching: Vec<String>,
        #[case] missing: Vec<String>,
    ) {
        assert_eq!(pattern.len(), MAX_PATTERN_BYTES);
        let patterns = file_patterns(&[&pattern]);
        let excluding = file_patterns(&["**", &format!("!{pattern}")]);
        for (paths, expected) in [(&matching, true), (&missing, false)] {
            for path in paths {
                assert!(path.len() <= MAX_PATH_BYTES);
                assert_eq!(
                    anywhere_reference(&pattern, path),
                    expected,
                    "the reference"
                );
                assert_eq!(patterns.matches(path), expected, "{path}");
                assert_eq!(excluding.matches(path), !expected, "excluding {path}");
            }
        }
    }

    mod glob_properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 4096,
                ..ProptestConfig::default()
            })]

            /// `glob_matches` agrees with the backtracking matcher on every
            /// short glob and text, so SchemaStore `fileMatch` is unchanged.
            #[test]
            fn glob_matches_agrees_with_the_backtracking_matcher(
                pattern in "[ab\u{e9}*/!.]{0,9}",
                text in "[ab\u{e9}/!.]{0,12}",
            ) {
                prop_assert_eq!(
                    glob_matches(&pattern, &text),
                    backtracking_glob_matches(pattern.as_bytes(), text.as_bytes()),
                    "{} on {}", pattern, text
                );
            }

            /// With a literal suffix that ends the text too, the matcher still
            /// agrees with the backtracking matcher: the suffix never skips
            /// the simulation, and a `/` folded into a `**/` before it is not
            /// part of it.
            #[test]
            fn glob_matches_agrees_with_the_backtracking_matcher_when_a_literal_ends_the_text(
                body in "[ab*/]{0,6}",
                suffix in prop::sample::select(vec![".y", "b", "/b", "\u{e9}", "a.yaml"]),
                text_body in "[ab/.\u{e9}]{0,12}",
            ) {
                let pattern = format!("{body}{suffix}");
                let text = format!("{text_body}{suffix}");
                prop_assert_eq!(
                    glob_matches(&pattern, &text),
                    backtracking_glob_matches(pattern.as_bytes(), text.as_bytes()),
                    "{} on {}", pattern, text
                );
            }

            /// No glob and path make matching panic.
            #[test]
            fn file_patterns_never_panic(
                globs in prop::collection::vec("[ab\u{e9}*/!.]{0,9}", 0..4),
                path in "[ab\u{e9}/.]{0,12}",
            ) {
                if let Some(patterns) = FilePatterns::new(&globs) {
                    let _ = patterns.matches(&path);
                }
                if let Some(patterns) = FilePatterns::without_limits(globs.iter().map(String::as_str)) {
                    let _ = patterns.matches(&path);
                }
            }

            /// Within the limits, the limited and the unlimited constructor
            /// build the same patterns.
            #[test]
            fn new_and_without_limits_agree_within_the_limits(
                globs in prop::collection::vec("[ab\u{e9}*/!.]{0,9}", 0..4),
            ) {
                prop_assert_eq!(
                    FilePatterns::new(&globs),
                    FilePatterns::without_limits(globs.iter().map(String::as_str))
                );
            }

            /// A glob and its own exclusion together match nothing.
            #[test]
            fn a_glob_and_its_exclusion_match_nothing(
                glob in "[ab*.]{1,6}",
                path in "[ab/.]{0,10}",
            ) {
                let patterns = file_patterns(&[&glob, &format!("!{glob}")]);
                prop_assert!(!patterns.matches(&path), "{} on {}", glob, path);
            }

            /// Adding an exclusion never adds a match; adding an inclusion
            /// never removes one.
            #[test]
            fn exclusions_only_remove_and_inclusions_only_add(
                base in "[ab*.]{1,6}",
                extra in "[ab*.]{1,6}",
                path in "[ab/.]{0,10}",
            ) {
                let alone = file_patterns(&[&base]).matches(&path);
                let excluded = file_patterns(&[&base, &format!("!{extra}")]).matches(&path);
                let included = file_patterns(&[&base, &extra]).matches(&path);
                prop_assert!(alone || !excluded);
                prop_assert!(!alone || included);
            }

            /// Without `**`, a pattern matches anywhere exactly when the plain
            /// glob matches the whole path or what follows one of its slashes.
            #[test]
            fn file_patterns_match_what_follows_any_slash(
                pattern in "([ab./]|\\*[ab./]){1,5}\\*?",
                text in "[ab/.]{0,12}",
            ) {
                let glob = pattern.strip_prefix('/').unwrap_or(&pattern);
                prop_assume!(!glob.is_empty());
                prop_assert_eq!(
                    file_patterns(&[&pattern]).matches(&text),
                    anywhere_reference(glob, &text),
                    "{} on {}", pattern, text
                );
            }
        }
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
    #[case::ingress_networking(
        "networking.k8s.io/v1",
        "Ingress",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/ingress-networking-v1.json"
    )]
    #[case::networkpolicy_networking(
        "networking.k8s.io/v1",
        "NetworkPolicy",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/networkpolicy-networking-v1.json"
    )]
    #[case::clusterrole_rbac(
        "rbac.authorization.k8s.io/v1",
        "ClusterRole",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/clusterrole-rbac-v1.json"
    )]
    #[case::csr_certificates(
        "certificates.k8s.io/v1",
        "CertificateSigningRequest",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/certificatesigningrequest-certificates-v1.json"
    )]
    #[case::vwc_admissionregistration(
        "admissionregistration.k8s.io/v1",
        "ValidatingWebhookConfiguration",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/validatingwebhookconfiguration-admissionregistration-v1.json"
    )]
    #[case::apiservice_apiregistration(
        "apiregistration.k8s.io/v1",
        "APIService",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/apiservice-apiregistration-v1.json"
    )]
    #[case::flowschema_flowcontrol(
        "flowcontrol.apiserver.k8s.io/v1",
        "FlowSchema",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/flowschema-flowcontrol-v1.json"
    )]
    #[case::storageversion_apiserverinternal(
        "internal.apiserver.k8s.io/v1alpha1",
        "StorageVersion",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/storageversion-apiserverinternal-v1alpha1.json"
    )]
    #[case::dotless_group_with_beta(
        "autoscaling/v2beta1",
        "HorizontalPodAutoscaler",
        "1.29.0",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.29.0-standalone-strict/horizontalpodautoscaler-autoscaling-v2beta1.json"
    )]
    #[case::dotted_builtin_master(
        "networking.k8s.io/v1",
        "Ingress",
        "master",
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/master-standalone-strict/ingress-networking-v1.json"
    )]
    fn kubernetes_schema_url_returns_url(
        #[case] api_version: &str,
        #[case] kind: &str,
        #[case] k8s_version: &str,
        #[case] expected: &str,
    ) {
        let resource = KubernetesResource::new(api_version, kind).expect("valid resource");
        assert_eq!(
            kubernetes_schema_url(&resource, &parsed_version(k8s_version)).as_deref(),
            Some(expected)
        );
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

    /// Every dotted built-in group with the segment the schema repository uses
    /// in its file names, written out independently of the production list.
    const BUILTIN_DOTTED_GROUPS: [(&str, &str); 19] = [
        ("admissionregistration.k8s.io", "admissionregistration"),
        ("apiextensions.k8s.io", "apiextensions"),
        ("apiregistration.k8s.io", "apiregistration"),
        ("authentication.k8s.io", "authentication"),
        ("authorization.k8s.io", "authorization"),
        ("certificates.k8s.io", "certificates"),
        ("coordination.k8s.io", "coordination"),
        ("discovery.k8s.io", "discovery"),
        ("events.k8s.io", "events"),
        ("flowcontrol.apiserver.k8s.io", "flowcontrol"),
        ("internal.apiserver.k8s.io", "apiserverinternal"),
        ("lifecycle.k8s.io", "lifecycle"),
        ("networking.k8s.io", "networking"),
        ("node.k8s.io", "node"),
        ("rbac.authorization.k8s.io", "rbac"),
        ("resource.k8s.io", "resource"),
        ("scheduling.k8s.io", "scheduling"),
        ("storage.k8s.io", "storage"),
        ("storagemigration.k8s.io", "storagemigration"),
    ];

    #[rstest]
    #[case::apps("apps")]
    #[case::autoscaling("autoscaling")]
    #[case::batch("batch")]
    #[case::policy("policy")]
    #[case::extensions("extensions")]
    #[case::any_name_without_a_dot("whatever")]
    fn kubernetes_schema_url_uses_a_dotless_group_as_its_own_segment(#[case] group: &str) {
        let resource =
            KubernetesResource::new(&format!("{group}/v1"), "Widget").expect("valid resource");
        assert_eq!(
            kubernetes_schema_url(&resource, &parsed_version("master")),
            Some(format!(
                "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/master-standalone-strict/widget-{group}-v1.json"
            ))
        );
    }

    #[test]
    fn kubernetes_schema_url_maps_every_builtin_dotted_group_to_its_segment() {
        for (group, segment) in BUILTIN_DOTTED_GROUPS {
            let resource =
                KubernetesResource::new(&format!("{group}/v1"), "Thing").expect("valid resource");
            assert_eq!(
                kubernetes_schema_url(&resource, &parsed_version("master")),
                Some(format!(
                    "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/master-standalone-strict/thing-{segment}-v1.json"
                )),
                "group {group}"
            );
        }
        let mut segments: Vec<&str> = BUILTIN_DOTTED_GROUPS.iter().map(|&(_, s)| s).collect();
        segments.sort_unstable();
        segments.dedup();
        assert_eq!(segments.len(), BUILTIN_DOTTED_GROUPS.len());
    }

    #[test]
    fn kubernetes_schema_url_returns_none_for_near_misses_of_builtin_groups() {
        for (group, _) in BUILTIN_DOTTED_GROUPS {
            for near_miss in [format!("x.{group}"), format!("{group}.x")] {
                let resource = KubernetesResource::new(&format!("{near_miss}/v1"), "Widget")
                    .expect("valid resource");
                assert_eq!(
                    kubernetes_schema_url(&resource, &parsed_version("master")),
                    None,
                    "group {near_miss}"
                );
            }
        }
    }

    #[rstest]
    #[case::cert_manager("cert-manager.io")]
    #[case::kustomize_config("kustomize.config.k8s.io")]
    #[case::gateway_api("gateway.networking.k8s.io")]
    #[case::volume_snapshots("snapshot.storage.k8s.io")]
    #[case::bare_suffix("k8s.io")]
    #[case::dash_suffix("x-k8s.io")]
    #[case::builtin_as_suffix("foo.networking.k8s.io")]
    #[case::builtin_as_prefix("networking.k8s.io.evil.com")]
    #[case::partial_builtin("apiserver.k8s.io")]
    #[case::first_label_of_dotless_builtin_vpa("autoscaling.k8s.io")]
    #[case::first_label_of_dotless_builtin_kruise("apps.kruise.io")]
    #[case::first_label_of_dotless_builtin_volcano("batch.volcano.sh")]
    #[case::first_label_of_dotted_builtin("networking.example.com")]
    #[case::wrong_suffix_internal("internal.k8s.io")]
    #[case::wrong_suffix_rbac("rbac.k8s.io")]
    #[case::example("example.com")]
    #[case::hyphenated("my-group.example.com")]
    fn kubernetes_schema_url_returns_none_for_custom_resource_groups(#[case] group: &str) {
        let resource =
            KubernetesResource::new(&format!("{group}/v1"), "Widget").expect("valid resource");
        assert_eq!(
            kubernetes_schema_url(&resource, &parsed_version("master")),
            None
        );
    }

    /// Versions the server accepts; shared with the VS Code pattern parity test.
    const ACCEPTED_VERSIONS: &[&str] = &[
        "master",
        "1.29.0",
        "0.0.0",
        "1.29.10",
        "100.200.300",
        "01.02.03",
        "123456789.123456789.123456789",
    ];

    /// Versions the server rejects; shared with the VS Code pattern parity test.
    const REJECTED_VERSIONS: &[&str] = &[
        "",
        "1.29",
        "1",
        "1.29.0.1",
        "1.29.",
        ".1.2",
        "1..2",
        "1.2.3.",
        "v1.29.0",
        "Master",
        "MASTER",
        "master ",
        "master\n",
        " 1.29.0",
        "1.29.0 ",
        "1.29.0\n",
        "1.29.0\0",
        "1.29.0-rc1",
        "1.29.x",
        "latest",
        "..",
        "../..",
        "1.29.0/../x",
        "1.29.0/../../attacker/repo/main/x",
        "1/../../../../evil/repo/main/x",
        "1.2.3?x",
        "1.2.3#x",
        "1.2.3%2e%2e",
        "1.2.3@host",
        "1.2.3\\x",
        "\u{661}.\u{662}.\u{663}",
        "\u{ff11}.\u{ff12}.\u{ff13}",
        "\u{202e}1.2.3",
        "1.29.0\u{200b}",
        "1234567890.1.2",
        "1.1234567890.2",
        "1.2.1234567890",
    ];

    #[test]
    fn kubernetes_version_new_accepts_valid_forms() {
        for version in ACCEPTED_VERSIONS {
            assert_eq!(
                KubernetesVersion::new(version)
                    .as_ref()
                    .map(KubernetesVersion::as_str),
                Some(*version),
                "{version:?}"
            );
        }
    }

    #[test]
    fn kubernetes_version_new_rejects_invalid_forms() {
        for version in REJECTED_VERSIONS {
            assert_eq!(KubernetesVersion::new(version), None, "{version:?}");
        }
    }

    #[test]
    fn kubernetes_version_master_equals_parsed_master() {
        assert_eq!(
            Some(KubernetesVersion::master()),
            KubernetesVersion::new("master")
        );
    }

    #[test]
    fn kubernetes_schema_url_keeps_leading_zeros_of_a_version() {
        let resource = KubernetesResource::new("v1", "Pod").expect("valid resource");
        assert_eq!(
            kubernetes_schema_url(&resource, &parsed_version("01.02.03")).as_deref(),
            Some(
                "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v01.02.03-standalone-strict/pod-v1.json"
            )
        );
    }

    /// The `kubernetesVersion` pattern in the VS Code manifest and the server's
    /// validator accept the same values (the settings-sync rule).
    #[test]
    fn vscode_kubernetes_version_pattern_agrees_with_the_server() {
        let properties = crate::test_utils::vscode_manifest_properties();
        let setting = &properties["rlsp-yaml.kubernetesVersion"];
        let pattern = setting["pattern"]
            .as_str()
            .expect("kubernetesVersion declares a pattern");
        let pattern = regex::Regex::new(pattern).expect("pattern compiles");

        assert!(
            setting["patternErrorMessage"]
                .as_str()
                .is_some_and(|message| !message.is_empty()),
            "kubernetesVersion declares a patternErrorMessage"
        );
        let default = setting["default"].as_str().expect("default is a string");
        assert!(pattern.is_match(default), "default {default:?}");
        for version in ACCEPTED_VERSIONS.iter().chain(REJECTED_VERSIONS) {
            assert_eq!(
                pattern.is_match(version),
                KubernetesVersion::new(version).is_some(),
                "{version:?}"
            );
        }
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

        /// An accepted resource whose group has schemas names a file in the
        /// schema repository: the URL passes the SSRF guard, starts with the
        /// repository prefix, and adds one directory and one file that the
        /// document cannot widen.
        fn assert_url_inside_the_schema_repository(url: &str) -> Result<(), TestCaseError> {
            let normalized = crate::schema::validate_and_normalize_url(url)
                .expect("accepted resource must pass the SSRF guard");

            prop_assert_eq!(&normalized, url);
            prop_assert!(normalized.starts_with(REPOSITORY_PREFIX), "{normalized}");
            let parsed =
                tower_lsp::lsp_types::Url::parse(&normalized).expect("normalized URL parses");
            prop_assert_eq!(parsed.query(), None);
            prop_assert_eq!(parsed.fragment(), None);
            let segments: Vec<&str> = parsed.path_segments().expect("hierarchical URL").collect();
            prop_assert_eq!(segments.len(), 5, "{}", normalized);
            prop_assert!(
                segments
                    .iter()
                    .all(|s| !s.is_empty() && !s.starts_with('.'))
            );
            prop_assert_eq!(
                segments[4].rsplit('.').next(),
                Some("json"),
                "{}",
                normalized
            );
            prop_assert!(
                !normalized[REPOSITORY_PREFIX.len()..].contains(['%', '\\']),
                "{}",
                normalized
            );
            Ok(())
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 2048,
                ..ProptestConfig::default()
            })]

            /// Any accepted version yields a URL inside the schema repository
            /// whose directory is `master-standalone-strict` or
            /// `v{MAJOR.MINOR.PATCH}-standalone-strict`.
            #[test]
            fn accepted_version_yields_url_inside_the_schema_repository(
                text in prop_oneof![
                    "\\PC{0,40}",
                    "[0-9./%?#:@\\\\ \\-]{0,40}",
                    "[0-9]{1,9}\\.[0-9]{1,9}\\.[0-9]{1,9}",
                    Just("master".to_string()),
                ],
            ) {
                let Some(k8s_version) = KubernetesVersion::new(&text) else {
                    return Ok(());
                };
                let resource = KubernetesResource::new("v1", "Pod").expect("valid resource");
                let url = kubernetes_schema_url(&resource, &k8s_version)
                    .expect("core resource has a URL");
                assert_url_inside_the_schema_repository(&url)?;
                let directory = url[REPOSITORY_PREFIX.len()..]
                    .split('/')
                    .next()
                    .expect("directory segment");
                prop_assert!(
                    regex::Regex::new(r"^(master|v[0-9]{1,9}\.[0-9]{1,9}\.[0-9]{1,9})-standalone-strict$")
                        .expect("regex compiles")
                        .is_match(directory),
                    "{}",
                    directory
                );
            }

            /// Guards the property above against passing vacuously.
            #[test]
            fn numeric_versions_are_accepted(
                text in "[0-9]{1,9}\\.[0-9]{1,9}\\.[0-9]{1,9}",
            ) {
                prop_assert!(KubernetesVersion::new(&text).is_some());
            }

            #[test]
            fn versions_with_a_character_outside_digits_and_dots_are_rejected(
                text in "[0-9.]{0,10}[^0-9.][0-9.]{0,10}",
            ) {
                prop_assert_eq!(KubernetesVersion::new(&text), None);
            }

            #[test]
            fn accepted_resource_yields_url_inside_the_schema_repository(
                api_version in name_text(),
                kind in name_text(),
            ) {
                let Some(resource) = KubernetesResource::new(&api_version, &kind) else {
                    return Ok(());
                };

                for k8s_version in ["1.29.0", "master"] {
                    if let Some(url) = kubernetes_schema_url(&resource, &parsed_version(k8s_version)) {
                        assert_url_inside_the_schema_repository(&url)?;
                    }
                }
            }

            /// Guards the property above against passing vacuously: core and
            /// dotless-group resources always have a URL.
            #[test]
            fn core_and_dotless_resources_yield_a_url(
                api_version in "(v[0-9]{1,3}((alpha|beta)[0-9])?)|([a-z0-9]{1,10}/v[0-9]{1,3})",
                kind in "[A-Z][A-Za-z0-9]{0,62}",
            ) {
                let resource = KubernetesResource::new(&api_version, &kind).expect("valid resource");
                for k8s_version in ["1.29.0", "master"] {
                    let url = kubernetes_schema_url(&resource, &parsed_version(k8s_version));
                    prop_assert!(url.is_some(), "{api_version} {kind}");
                    assert_url_inside_the_schema_repository(&url.expect("checked above"))?;
                }
            }

            #[test]
            fn builtin_dotted_resource_yields_url_inside_the_schema_repository(
                group in prop::sample::select(BUILTIN_DOTTED_GROUPS.map(|(group, _)| group).to_vec()),
                version in "v[0-9]{1,3}((alpha|beta)[0-9])?",
                kind in "[A-Z][A-Za-z0-9]{0,62}",
            ) {
                let resource = KubernetesResource::new(&format!("{group}/{version}"), &kind)
                    .expect("valid resource");
                for k8s_version in ["1.29.0", "master"] {
                    let url = kubernetes_schema_url(&resource, &parsed_version(k8s_version));
                    prop_assert!(url.is_some(), "{group}");
                    assert_url_inside_the_schema_repository(&url.expect("checked above"))?;
                }
            }

            #[test]
            fn dotted_group_outside_the_builtin_list_yields_no_url(
                group in "[a-z0-9]{1,8}(\\.[a-z0-9]{1,8}){1,3}",
                kind in "[A-Z][A-Za-z0-9]{0,62}",
            ) {
                prop_assume!(BUILTIN_DOTTED_GROUPS.iter().all(|&(builtin, _)| builtin != group));
                let resource = KubernetesResource::new(&format!("{group}/v1"), &kind)
                    .expect("valid resource");
                for k8s_version in ["1.29.0", "master"] {
                    prop_assert_eq!(kubernetes_schema_url(&resource, &parsed_version(k8s_version)), None);
                }
            }

            /// Guards the first property against passing vacuously: the
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

    // K8s-11: "Master" (capital M) is not a valid version; the setting falls back to master.
    // Different assertion shape (contains check) — left standalone.
    #[test]
    fn should_treat_capitalised_master_as_invalid_and_select_master() {
        assert_eq!(KubernetesVersion::new("Master"), None);
        let pod = KubernetesResource::new("v1", "Pod").expect("valid resource");
        let selected = KubernetesVersion::new("Master").unwrap_or_else(KubernetesVersion::master);
        let url = kubernetes_schema_url(&pod, &selected).expect("core resource has a URL");
        assert!(
            url.contains("/master-standalone-strict/"),
            "expected /master-standalone-strict/ in URL, got: {url}"
        );
    }
}
