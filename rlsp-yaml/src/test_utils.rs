// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Document;
use tower_lsp::lsp_types::Url;

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
