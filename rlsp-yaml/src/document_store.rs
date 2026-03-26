// SPDX-License-Identifier: MIT

use std::collections::HashMap;

use saphyr::{LoadableYamlNode, MarkedYamlOwned, YamlOwned};
use tower_lsp::lsp_types::Url;

use crate::parser;

struct Document {
    text: String,
    yaml: Option<Vec<YamlOwned>>,
    marked_yaml: Option<Vec<MarkedYamlOwned>>,
}

#[derive(Default)]
pub struct DocumentStore {
    documents: HashMap<Url, Document>,
}

impl DocumentStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, uri: Url, text: String) {
        let parsed = parser::parse_yaml(&text);
        let marked = parse_marked(&text);
        self.documents.insert(
            uri,
            Document {
                text,
                yaml: if parsed.documents.is_empty() {
                    None
                } else {
                    Some(parsed.documents)
                },
                marked_yaml: marked,
            },
        );
    }

    pub fn change(&mut self, uri: &Url, text: String) {
        if let Some(doc) = self.documents.get_mut(uri) {
            let parsed = parser::parse_yaml(&text);
            doc.marked_yaml = parse_marked(&text);
            doc.text = text;
            doc.yaml = if parsed.documents.is_empty() {
                None
            } else {
                Some(parsed.documents)
            };
        }
    }

    pub fn close(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    #[must_use]
    pub fn get(&self, uri: &Url) -> Option<&str> {
        self.documents.get(uri).map(|doc| doc.text.as_str())
    }

    #[must_use]
    pub fn get_yaml(&self, uri: &Url) -> Option<&Vec<YamlOwned>> {
        self.documents.get(uri)?.yaml.as_ref()
    }

    #[must_use]
    pub fn get_marked_yaml(&self, uri: &Url) -> Option<&Vec<MarkedYamlOwned>> {
        self.documents.get(uri)?.marked_yaml.as_ref()
    }

    #[must_use]
    pub fn all_documents(&self) -> Vec<(Url, String)> {
        self.documents
            .iter()
            .map(|(uri, doc)| (uri.clone(), doc.text.clone()))
            .collect()
    }
}

/// Parse text into a `MarkedYamlOwned` AST. Returns `None` on parse failure.
fn parse_marked(text: &str) -> Option<Vec<MarkedYamlOwned>> {
    MarkedYamlOwned::load_from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uri(name: &str) -> Url {
        Url::parse(&format!("file:///test/{name}")).expect("valid test URI")
    }

    #[test]
    fn should_store_document_on_open() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: value".to_string());

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

        store.open(uri.clone(), "old text".to_string());
        store.change(&uri, "new text".to_string());

        assert_eq!(store.get(&uri), Some("new text"));
    }

    #[test]
    fn should_remove_document_on_close() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "content".to_string());
        store.close(&uri);

        assert_eq!(store.get(&uri), None);
    }

    #[test]
    fn should_handle_multiple_documents() {
        let mut store = DocumentStore::new();
        let uri_a = test_uri("a.yaml");
        let uri_b = test_uri("b.yaml");

        store.open(uri_a.clone(), "alpha".to_string());
        store.open(uri_b.clone(), "beta".to_string());

        assert_eq!(store.get(&uri_a), Some("alpha"));
        assert_eq!(store.get(&uri_b), Some("beta"));
    }

    #[test]
    fn should_close_only_specified_document() {
        let mut store = DocumentStore::new();
        let uri_a = test_uri("a.yaml");
        let uri_b = test_uri("b.yaml");

        store.open(uri_a.clone(), "alpha".to_string());
        store.open(uri_b.clone(), "beta".to_string());
        store.close(&uri_a);

        assert_eq!(store.get(&uri_a), None);
        assert_eq!(store.get(&uri_b), Some("beta"));
    }

    #[test]
    fn should_overwrite_document_if_opened_again() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "first".to_string());
        store.close(&uri);
        store.open(uri.clone(), "second".to_string());

        assert_eq!(store.get(&uri), Some("second"));
    }

    #[test]
    fn should_handle_empty_document_text() {
        let mut store = DocumentStore::new();
        let uri = test_uri("empty.yaml");

        store.open(uri.clone(), String::new());

        assert_eq!(store.get(&uri), Some(""));
    }

    #[test]
    fn should_not_panic_on_change_for_unknown_document() {
        let mut store = DocumentStore::new();
        let uri = test_uri("unknown.yaml");

        store.change(&uri, "new text".to_string());

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

        store.open(uri.clone(), "first".to_string());
        store.open(uri.clone(), "second".to_string());

        assert_eq!(store.get(&uri), Some("second"));
    }

    #[test]
    fn should_store_parsed_yaml_alongside_text() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: value\n".to_string());

        assert_eq!(store.get(&uri), Some("key: value\n"));
        let yaml = store.get_yaml(&uri);
        assert!(yaml.is_some());
        assert_eq!(yaml.expect("yaml present").len(), 1);
    }

    #[test]
    fn should_return_none_for_ast_of_unknown_document() {
        let store = DocumentStore::new();
        let uri = test_uri("unknown.yaml");

        assert!(store.get_yaml(&uri).is_none());
    }

    #[test]
    fn should_update_parsed_yaml_on_change() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: old\n".to_string());
        store.change(&uri, "key: new\n".to_string());

        let yaml = store.get_yaml(&uri).expect("yaml present");
        assert_eq!(yaml.len(), 1);
        let val = yaml.first().expect("yaml present")["key"].as_str();
        assert_eq!(val, Some("new"));
    }

    #[test]
    fn should_clear_ast_on_close() {
        let mut store = DocumentStore::new();
        let uri = test_uri("doc.yaml");

        store.open(uri.clone(), "key: value\n".to_string());
        store.close(&uri);

        assert!(store.get_yaml(&uri).is_none());
    }

    #[test]
    fn all_documents_returns_all_open_documents() {
        let mut store = DocumentStore::new();
        let uri_a = test_uri("a.yaml");
        let uri_b = test_uri("b.yaml");

        store.open(uri_a.clone(), "alpha".to_string());
        store.open(uri_b.clone(), "beta".to_string());

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

        store.open(uri.clone(), "key: [bad".to_string());

        assert_eq!(store.get(&uri), Some("key: [bad"));
        assert!(store.get_yaml(&uri).is_none());
    }
}
