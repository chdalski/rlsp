// SPDX-License-Identifier: MIT

use std::fmt::Write as _;

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Document;
use tower_lsp::lsp_types::{Range, Url};

/// Parse YAML text into documents. Returns an empty vec on parse error.
#[must_use]
pub fn parse_docs(text: &str) -> Vec<Document<Span>> {
    rlsp_yaml_parser::load(text).unwrap_or_default()
}

/// A stable test file URI for use in unit tests.
///
/// # Panics
///
/// Never panics — the literal `"file:///test.yaml"` is always a valid URL.
#[must_use]
pub fn test_uri() -> Url {
    Url::parse("file:///test.yaml").expect("valid test URI")
}

/// The VS Code extension manifest (`package.json`), so tests can pin that it
/// agrees with the server.
///
/// # Panics
///
/// Panics when the manifest is missing or is not JSON.
#[must_use]
pub fn vscode_manifest() -> serde_json::Value {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("integrations/vscode/package.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("package.json is readable"))
        .expect("package.json is JSON")
}

/// The `contributes.configuration.properties` object of [`vscode_manifest`].
///
/// # Panics
///
/// Panics when the manifest lacks the object.
#[must_use]
pub fn vscode_manifest_properties() -> serde_json::Map<String, serde_json::Value> {
    vscode_manifest()["contributes"]["configuration"]["properties"]
        .as_object()
        .expect("the manifest declares configuration properties")
        .clone()
}

/// `leaf` inside `depth` nested flow sequences: `[[leaf]]` for a depth of 2.
#[must_use]
pub fn nested_flow(depth: usize, leaf: &str) -> String {
    format!("{}{leaf}{}", "[".repeat(depth), "]".repeat(depth))
}

/// `depth` lines of `entry`, each indented one level deeper than the last.
#[must_use]
pub fn staircase(depth: usize, entry: &str) -> String {
    let mut lines = String::new();
    for level in 0..depth {
        writeln!(lines, "{}{entry}", "  ".repeat(level)).unwrap();
    }
    lines
}

/// Assert that `yaml` loads. `parse_docs` returns no documents on a load
/// error, so a test that expects no diagnostics from a deep document passes
/// vacuously unless it first shows that the document loads.
///
/// # Panics
///
/// Panics when `yaml` does not load.
pub fn assert_loads(yaml: &str) {
    assert!(rlsp_yaml_parser::load(yaml).is_ok(), "the text loads");
}

/// The characters of `range` (a range within one line) in `yaml`; lines end
/// at `\n`, `\r\n` or a lone `\r`.
///
/// # Panics
///
/// Panics when the range spans lines or its line does not exist.
#[must_use]
pub fn text_of(yaml: &str, range: Range) -> String {
    assert_eq!(range.start.line, range.end.line, "a one-line range");
    yaml.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .nth(range.start.line as usize)
        .expect("the line exists")
        .chars()
        .skip(range.start.character as usize)
        .take((range.end.character - range.start.character) as usize)
        .collect()
}
