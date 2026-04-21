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
#[expect(clippy::expect_used, reason = "literal URL is always valid")]
pub fn test_uri() -> Url {
    Url::parse("file:///test.yaml").expect("valid test URI")
}
