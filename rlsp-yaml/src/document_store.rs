// SPDX-License-Identifier: MIT

use std::collections::HashMap;

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Document;
use tower_lsp::lsp_types::Url;

use crate::parser;

struct StoredDocument {
    text: String,
    documents: Option<Vec<Document<Span>>>,
}

impl StoredDocument {
    /// Store `text` with its AST, loaded with `max_nesting_depth`; the AST is
    /// absent when the text does not load.
    fn load(text: String, max_nesting_depth: usize) -> Self {
        let parsed = parser::parse_yaml_with_limit(&text, max_nesting_depth);
        let documents = if parsed.documents.is_empty() {
            None
        } else {
            Some(parsed.documents)
        };
        Self { text, documents }
    }
}

/// In-memory store for open YAML documents and their parsed ASTs.
///
/// The store holds no nesting limit: every call that parses takes the
/// `maxNestingDepth` setting's current value, so the caller decides when a
/// stored AST is out of date.
#[derive(Default)]
pub struct DocumentStore {
    documents: HashMap<Url, StoredDocument>,
}

impl DocumentStore {
    /// Create a new empty document store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a document, parsing it immediately with `max_nesting_depth`.
    pub fn open(&mut self, uri: Url, text: String, max_nesting_depth: usize) {
        self.documents
            .insert(uri, StoredDocument::load(text, max_nesting_depth));
    }

    /// Update an open document's text and reparse it with `max_nesting_depth`.
    pub fn change(&mut self, uri: &Url, text: String, max_nesting_depth: usize) {
        if let Some(doc) = self.documents.get_mut(uri) {
            *doc = StoredDocument::load(text, max_nesting_depth);
        }
    }

    /// Parse the stored text of every open document again with
    /// `max_nesting_depth`, leaving the texts as they are, and return each
    /// document's URI and text.
    ///
    /// For a change of the nesting limit: one call under one lock, so a text
    /// the client changed in the meantime is never replaced by an older one.
    pub fn reparse_all(&mut self, max_nesting_depth: usize) -> Vec<(Url, String)> {
        self.documents
            .iter_mut()
            .map(|(uri, doc)| {
                let text = std::mem::take(&mut doc.text);
                *doc = StoredDocument::load(text, max_nesting_depth);
                (uri.clone(), doc.text.clone())
            })
            .collect()
    }

    /// Remove a document from the store when it is closed.
    pub fn close(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// Return the raw text of an open document.
    #[must_use]
    pub fn get(&self, uri: &Url) -> Option<&str> {
        self.documents.get(uri).map(|doc| doc.text.as_str())
    }

    /// Returns the parsed AST (`Document<Span>`) for the given URI.
    #[must_use]
    pub fn get_documents(&self, uri: &Url) -> Option<&Vec<Document<Span>>> {
        self.documents.get(uri)?.documents.as_ref()
    }

    /// Return all open document URIs and their raw text.
    #[must_use]
    pub fn all_documents(&self) -> Vec<(Url, String)> {
        self.documents
            .iter()
            .map(|(uri, doc)| (uri.clone(), doc.text.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rlsp_yaml_parser::DEFAULT_MAX_NESTING_DEPTH;
    use rlsp_yaml_parser::node::Node;
    use rstest::rstest;

    use super::*;

    /// The limit of every test that is not about the limit.
    const LIMIT: usize = DEFAULT_MAX_NESTING_DEPTH;

    /// Three nested block mappings: loads with a limit of 3, not with 2.
    const THREE_LEVELS: &str = "a:\n  b:\n    c: 1\n";

    fn test_uri(name: &str) -> Url {
        Url::parse(&format!("file:///test/{name}")).expect("valid test URI")
    }

    #[test]
    fn should_store_document_on_open() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: value".to_string(), LIMIT);

        assert_eq!(store.get(&uri), Some("key: value"));
    }

    #[test]
    fn should_return_none_for_unknown_uri() {
        let store = DocumentStore::new();
        let uri = test_uri("unknown.yaml");

        assert_eq!(store.get(&uri), None);
    }

    #[test]
    fn should_update_document_on_change() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "old text".to_string(), LIMIT);
        store.change(&uri, "new text".to_string(), LIMIT);

        assert_eq!(store.get(&uri), Some("new text"));
    }

    #[test]
    fn should_remove_document_on_close() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "content".to_string(), LIMIT);
        store.close(&uri);

        assert_eq!(store.get(&uri), None);
    }

    #[test]
    fn should_handle_multiple_documents() {
        let mut store = DocumentStore::new();
        let uri_a = test_uri("a.yaml");
        let uri_b = test_uri("b.yaml");

        store.open(uri_a.clone(), "alpha".to_string(), LIMIT);
        store.open(uri_b.clone(), "beta".to_string(), LIMIT);

        assert_eq!(store.get(&uri_a), Some("alpha"));
        assert_eq!(store.get(&uri_b), Some("beta"));
    }

    #[test]
    fn should_close_only_specified_document() {
        let mut store = DocumentStore::new();
        let uri_a = test_uri("a.yaml");
        let uri_b = test_uri("b.yaml");

        store.open(uri_a.clone(), "alpha".to_string(), LIMIT);
        store.open(uri_b.clone(), "beta".to_string(), LIMIT);
        store.close(&uri_a);

        assert_eq!(store.get(&uri_a), None);
        assert_eq!(store.get(&uri_b), Some("beta"));
    }

    #[test]
    fn should_overwrite_document_if_opened_again() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "first".to_string(), LIMIT);
        store.close(&uri);
        store.open(uri.clone(), "second".to_string(), LIMIT);

        assert_eq!(store.get(&uri), Some("second"));
    }

    #[test]
    fn should_handle_empty_document_text() {
        let mut store = DocumentStore::new();
        let uri = test_uri("empty.yaml");

        store.open(uri.clone(), String::new(), LIMIT);

        assert_eq!(store.get(&uri), Some(""));
    }

    #[test]
    fn should_not_panic_on_change_for_unknown_document() {
        let mut store = DocumentStore::new();
        let uri = test_uri("unknown.yaml");

        store.change(&uri, "new text".to_string(), LIMIT);

        assert_eq!(store.get(&uri), None);
    }

    #[test]
    fn should_not_panic_on_close_for_unknown_document() {
        let mut store = DocumentStore::new();
        let uri = test_uri("unknown.yaml");

        store.close(&uri);

        assert_eq!(store.get(&uri), None);
    }

    #[test]
    fn should_overwrite_when_opening_already_open_document() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "first".to_string(), LIMIT);
        store.open(uri.clone(), "second".to_string(), LIMIT);

        assert_eq!(store.get(&uri), Some("second"));
    }

    #[test]
    fn should_store_parsed_documents_alongside_text() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: value\n".to_string(), LIMIT);

        assert_eq!(store.get(&uri), Some("key: value\n"));
        let docs = store.get_documents(&uri);
        assert!(docs.is_some());
        assert_eq!(docs.expect("documents present").len(), 1);
    }

    #[test]
    fn should_return_none_for_ast_of_unknown_document() {
        let store = DocumentStore::new();
        let uri = test_uri("unknown.yaml");

        assert!(store.get_documents(&uri).is_none());
    }

    #[test]
    fn should_update_parsed_documents_on_change() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: old\n".to_string(), LIMIT);
        store.change(&uri, "key: new\n".to_string(), LIMIT);

        let docs = store.get_documents(&uri).expect("documents present");
        assert_eq!(docs.len(), 1);
        // Verify the root mapping contains the updated value.
        match &docs[0].root {
            Node::Mapping { entries, .. } => {
                let (_, val) = &entries[0];
                match val {
                    Node::Scalar { value, .. } => assert_eq!(value, "new"),
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                        panic!("expected Scalar value")
                    }
                }
            }
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                panic!("expected Mapping root")
            }
        }
    }

    #[test]
    fn should_clear_ast_on_close() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: value\n".to_string(), LIMIT);
        store.close(&uri);

        assert!(store.get_documents(&uri).is_none());
    }

    #[test]
    fn all_documents_returns_all_open_documents() {
        let mut store = DocumentStore::new();
        let uri_a = test_uri("a.yaml");
        let uri_b = test_uri("b.yaml");

        store.open(uri_a, "alpha".to_string(), LIMIT);
        store.open(uri_b, "beta".to_string(), LIMIT);

        let mut docs = store.all_documents();
        docs.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(docs.len(), 2);
        let texts: Vec<&str> = docs.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texts.contains(&"alpha"));
        assert!(texts.contains(&"beta"));
    }

    #[test]
    fn all_documents_returns_empty_when_store_is_empty() {
        let store = DocumentStore::new();
        assert!(store.all_documents().is_empty());
    }

    #[test]
    fn should_store_no_ast_when_parsing_fails() {
        let mut store = DocumentStore::new();
        let uri = test_uri("bad.yaml");

        store.open(uri.clone(), "key: [bad".to_string(), LIMIT);

        assert_eq!(store.get(&uri), Some("key: [bad"));
        assert!(store.get_documents(&uri).is_none());
    }

    #[test]
    fn should_store_multi_document_yaml() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "a: 1\n---\nb: 2\n".to_string(), LIMIT);

        let docs = store.get_documents(&uri).expect("documents present");
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn should_return_document_span_vec_from_get_documents() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: value\n".to_string(), LIMIT);

        let docs = store.get_documents(&uri).expect("documents present");
        // Confirm type is Document<Span> by accessing the root field.
        let _ = &docs[0].root;
    }

    #[test]
    fn should_update_document_span_root_on_change() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "hello\n".to_string(), LIMIT);
        store.change(&uri, "world\n".to_string(), LIMIT);

        let docs = store.get_documents(&uri).expect("documents present");
        match &docs[0].root {
            Node::Scalar { value, .. } => assert_eq!(value, "world"),
            Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                panic!("expected Scalar")
            }
        }
    }

    #[rstest]
    #[case::fits_the_limit(3, true)]
    #[case::over_the_limit(2, false)]
    fn open_stores_the_ast_only_when_the_document_fits_the_limit(
        #[case] limit: usize,
        #[case] has_ast: bool,
    ) {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), THREE_LEVELS.to_string(), limit);

        assert_eq!(store.get_documents(&uri).is_some(), has_ast);
        assert_eq!(store.get(&uri), Some(THREE_LEVELS));
    }

    #[rstest]
    #[case::lowered_below_the_depth(3, 2, false)]
    #[case::raised_to_the_depth(2, 3, true)]
    fn change_reparses_with_the_given_limit(
        #[case] open_limit: usize,
        #[case] change_limit: usize,
        #[case] has_ast: bool,
    ) {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");
        store.open(uri.clone(), THREE_LEVELS.to_string(), open_limit);

        store.change(&uri, THREE_LEVELS.to_string(), change_limit);

        assert_eq!(store.get_documents(&uri).is_some(), has_ast);
        assert_eq!(store.get(&uri), Some(THREE_LEVELS));
    }

    #[rstest]
    #[case::raised_to_the_depth(2, 3, true)]
    #[case::lowered_below_the_depth(3, 2, false)]
    fn reparse_all_loads_every_stored_text_with_the_given_limit(
        #[case] open_limit: usize,
        #[case] reparse_limit: usize,
        #[case] has_ast: bool,
    ) {
        let mut store = DocumentStore::new();
        let uri_a = test_uri("a.yaml");
        let uri_b = test_uri("b.yaml");
        store.open(uri_a.clone(), THREE_LEVELS.to_string(), open_limit);
        store.open(uri_b.clone(), THREE_LEVELS.to_string(), open_limit);

        let mut returned = store.reparse_all(reparse_limit);

        returned.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            returned,
            vec![
                (uri_a.clone(), THREE_LEVELS.to_string()),
                (uri_b.clone(), THREE_LEVELS.to_string())
            ]
        );
        assert_eq!(store.get_documents(&uri_a).is_some(), has_ast);
        assert_eq!(store.get_documents(&uri_b).is_some(), has_ast);
        assert_eq!(store.get(&uri_a), Some(THREE_LEVELS));
        assert_eq!(store.get(&uri_b), Some(THREE_LEVELS));
    }

    #[test]
    fn reparse_all_returns_nothing_for_an_empty_store() {
        let mut store = DocumentStore::new();

        assert!(store.reparse_all(LIMIT).is_empty());
    }
}
