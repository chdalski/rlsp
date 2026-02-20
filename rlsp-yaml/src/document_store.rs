use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

#[derive(Default)]
pub struct DocumentStore {
    documents: HashMap<Url, String>,
}

impl DocumentStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, uri: Url, text: String) {
        self.documents.insert(uri, text);
    }

    pub fn change(&mut self, uri: &Url, text: String) {
        if let Some(doc) = self.documents.get_mut(uri) {
            *doc = text;
        }
    }

    pub fn close(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<&str> {
        self.documents.get(uri).map(String::as_str)
    }
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
}
