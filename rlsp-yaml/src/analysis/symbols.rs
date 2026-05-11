// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{DocumentSymbol, Position, Range, SymbolKind};

use crate::lsp_util::{offset_to_lsp, span_to_lsp};

/// Produce a hierarchical list of document symbols for the given YAML documents.
///
/// Returns an empty vector if `docs` is empty or every document root is a
/// non-mapping node (scalar, sequence, alias).
#[must_use]
pub fn document_symbols(docs: &[Document<Span>]) -> Vec<DocumentSymbol> {
    if docs.is_empty() {
        return Vec::new();
    }

    docs.iter()
        .flat_map(|doc| yaml_to_symbols(&doc.root, doc.line_index()))
        .collect()
}

/// Convert a `Node<Span>` into `DocumentSymbol` objects.
fn yaml_to_symbols(node: &Node<Span>, idx: &LineIndex) -> Vec<DocumentSymbol> {
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .map(|(key, value)| make_symbol(&node_to_string(key), key, value, idx))
            .collect(),
        Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => Vec::new(),
    }
}

/// Truncate a string to at most 60 Unicode scalar values, appending `…` if truncated.
fn truncate_detail(s: &str) -> String {
    const LIMIT: usize = 60;
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(LIMIT).collect();
    if chars.next().is_some() {
        // There are more chars beyond the limit
        format!("{collected}…")
    } else {
        collected
    }
}

/// Compute the `detail` string for a value node.
fn value_detail(value: &Node<Span>) -> Option<String> {
    match value {
        Node::Scalar { value, .. } => Some(truncate_detail(value)),
        Node::Mapping { entries, .. } => {
            let n = entries.len();
            if n == 1 {
                Some("1 key".to_string())
            } else {
                Some(format!("{n} keys"))
            }
        }
        Node::Sequence { items, .. } => {
            let n = items.len();
            if n == 1 {
                Some("1 item".to_string())
            } else {
                Some(format!("{n} items"))
            }
        }
        Node::Alias { .. } => None,
    }
}

/// Create a `DocumentSymbol` for a key-value pair.
#[expect(
    deprecated,
    reason = "DocumentSymbol.deprecated field is required by LSP spec"
)]
fn make_symbol(
    key_name: &str,
    key_node: &Node<Span>,
    value: &Node<Span>,
    idx: &LineIndex,
) -> DocumentSymbol {
    let key_loc = node_loc(key_node);
    let value_loc = node_loc(value);

    let selection_range = span_to_lsp(key_loc, idx);
    let range = Range::new(
        offset_to_lsp(key_loc.start, idx),
        offset_to_lsp(value_loc.end, idx),
    );

    let children = match value {
        Node::Mapping { entries, .. } => {
            let child_symbols: Vec<DocumentSymbol> = entries
                .iter()
                .map(|(k, v)| make_symbol(&node_to_string(k), k, v, idx))
                .collect();
            if child_symbols.is_empty() {
                None
            } else {
                Some(child_symbols)
            }
        }
        Node::Sequence { items, .. } => {
            let child_symbols = make_sequence_children(items, idx);
            if child_symbols.is_empty() {
                None
            } else {
                Some(child_symbols)
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    };

    DocumentSymbol {
        name: key_name.to_string(),
        detail: value_detail(value),
        kind: node_symbol_kind(value),
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children,
    }
}

/// Label keys checked (in order) against the first mapping entry's key.
const LABEL_KEYS: &[&str] = &["name", "id", "key"];

/// Try to extract a label from a mapping item's first entry.
///
/// Returns `Some(label_value)` when the first key matches one of `LABEL_KEYS`
/// and its value is a scalar. Returns `None` otherwise.
fn label_from_mapping(entries: &[(Node<Span>, Node<Span>)]) -> Option<&str> {
    let (first_key, first_value) = entries.first()?;
    let key_str = match first_key {
        Node::Scalar { value, .. } => value.as_str(),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => return None,
    };
    if !LABEL_KEYS.contains(&key_str) {
        return None;
    }
    match first_value {
        Node::Scalar { value, .. } => Some(value.as_str()),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
    }
}

/// Create child symbols for sequence items.
#[expect(
    deprecated,
    reason = "DocumentSymbol.deprecated field is required by LSP spec"
)]
fn make_sequence_children(items: &[Node<Span>], idx: &LineIndex) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let item_loc = node_loc(item);

        let index_name = format!("[{i}]");

        // Determine name and detail using label-key heuristic.
        let (name, detail) = match item {
            Node::Mapping { entries, .. } => label_from_mapping(entries).map_or_else(
                || (index_name.clone(), None),
                |label| (label.to_string(), Some(index_name.clone())),
            ),
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
                (index_name.clone(), None)
            }
        };

        let range = span_to_lsp(item_loc, idx);
        // selection_range is a single character at the item content start (AST-derived).
        let start = offset_to_lsp(item_loc.start, idx);
        let sel_end = Position::new(start.line, start.character + 1);
        let selection_range = Range::new(start, sel_end);

        let item_children = match item {
            Node::Mapping { entries, .. } => {
                let cs: Vec<DocumentSymbol> = entries
                    .iter()
                    .map(|(k, v)| make_symbol(&node_to_string(k), k, v, idx))
                    .collect();
                if cs.is_empty() { None } else { Some(cs) }
            }
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
        };

        children.push(DocumentSymbol {
            name,
            detail,
            kind: node_symbol_kind(item),
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: item_children,
        });
    }

    children
}

/// Return the `Span` for a node.
const fn node_loc(node: &Node<Span>) -> Span {
    match node {
        Node::Scalar { loc, .. }
        | Node::Mapping { loc, .. }
        | Node::Sequence { loc, .. }
        | Node::Alias { loc, .. } => *loc,
    }
}

/// Map a `Node<Span>` value to the appropriate `SymbolKind`.
fn node_symbol_kind(node: &Node<Span>) -> SymbolKind {
    match node {
        Node::Mapping { .. } => SymbolKind::OBJECT,
        Node::Sequence { .. } => SymbolKind::ARRAY,
        Node::Scalar { tag, .. } => match tag.as_deref() {
            Some("tag:yaml.org,2002:null") => SymbolKind::NULL,
            Some("tag:yaml.org,2002:bool") => SymbolKind::BOOLEAN,
            Some("tag:yaml.org,2002:int" | "tag:yaml.org,2002:float") => SymbolKind::NUMBER,
            _ => SymbolKind::STRING,
        },
        Node::Alias { .. } => SymbolKind::STRING,
    }
}

/// Convert a YAML node to a string representation for use as a key name.
fn node_to_string(node: &Node<Span>) -> String {
    match node {
        Node::Scalar { value, .. } => value.clone(),
        Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => {
            format!("{node:?}")
        }
    }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::expect_used, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_utils::parse_docs as parse_docs_inner;
    use tower_lsp::lsp_types::SymbolKind;

    #[expect(clippy::unnecessary_wraps, reason = "callers use Option API")]
    fn parse_docs(text: &str) -> Option<Vec<Document<Span>>> {
        Some(parse_docs_inner(text))
    }

    fn find_symbol<'a>(symbols: &'a [DocumentSymbol], name: &str) -> Option<&'a DocumentSymbol> {
        symbols.iter().find(|s| s.name == name)
    }

    // Test 1
    #[test]
    fn should_return_symbols_for_flat_mapping() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 2);
        let name_sym = find_symbol(&symbols, "name").expect("should have 'name' symbol");
        assert_eq!(name_sym.kind, SymbolKind::STRING);
        let age_sym = find_symbol(&symbols, "age").expect("should have 'age' symbol");
        assert_eq!(age_sym.kind, SymbolKind::NUMBER);
    }

    // Test 2
    #[test]
    fn should_return_symbol_with_object_kind_for_mapping() {
        let text = "server:\n  port: 8080\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        let server = &symbols[0];
        assert_eq!(server.name, "server");
        assert_eq!(server.kind, SymbolKind::OBJECT);
        let children = server.children.as_ref().expect("should have children");
        assert!(
            find_symbol(children, "port").is_some(),
            "should have 'port' child"
        );
    }

    // Test 3
    #[test]
    fn should_return_symbol_with_array_kind_for_sequence() {
        let text = "items:\n  - one\n  - two\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "items");
        assert_eq!(symbols[0].kind, SymbolKind::ARRAY);
    }

    // Test 4
    #[test]
    fn should_return_nested_symbols() {
        let text = "server:\n  host: localhost\n  port: 8080\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        let server = &symbols[0];
        assert_eq!(server.kind, SymbolKind::OBJECT);
        let children = server.children.as_ref().expect("should have children");
        assert_eq!(children.len(), 2);
        assert!(find_symbol(children, "host").is_some());
        assert!(find_symbol(children, "port").is_some());
    }

    // Test 5
    #[test]
    fn should_return_deeply_nested_symbols() {
        let text = "a:\n  b:\n    c: deep\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        let a = &symbols[0];
        assert_eq!(a.name, "a");
        let b_children = a.children.as_ref().expect("a should have children");
        let b = find_symbol(b_children, "b").expect("should have 'b'");
        let c_children = b.children.as_ref().expect("b should have children");
        assert!(find_symbol(c_children, "c").is_some(), "should have 'c'");
    }

    // Test 6
    #[test]
    fn should_return_correct_symbol_kinds_for_scalar_types() {
        let text = "str_val: hello\nint_val: 42\nbool_val: true\nnull_val: ~\nfloat_val: 3.14\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 5);
        assert_eq!(
            find_symbol(&symbols, "str_val").expect("str_val").kind,
            SymbolKind::STRING
        );
        assert_eq!(
            find_symbol(&symbols, "int_val").expect("int_val").kind,
            SymbolKind::NUMBER
        );
        assert_eq!(
            find_symbol(&symbols, "bool_val").expect("bool_val").kind,
            SymbolKind::BOOLEAN
        );
        assert_eq!(
            find_symbol(&symbols, "null_val").expect("null_val").kind,
            SymbolKind::NULL
        );
        assert_eq!(
            find_symbol(&symbols, "float_val").expect("float_val").kind,
            SymbolKind::NUMBER
        );
    }

    // Test 7
    #[test]
    fn should_return_symbols_for_mixed_types() {
        let text = "name: Alice\naddress:\n  city: Wonderland\nhobbies:\n  - reading\n  - chess\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 3);
        let name = find_symbol(&symbols, "name").expect("name");
        assert_eq!(name.kind, SymbolKind::STRING);
        let address = find_symbol(&symbols, "address").expect("address");
        assert_eq!(address.kind, SymbolKind::OBJECT);
        let addr_children = address.children.as_ref().expect("address children");
        assert!(find_symbol(addr_children, "city").is_some());
        let hobbies = find_symbol(&symbols, "hobbies").expect("hobbies");
        assert_eq!(hobbies.kind, SymbolKind::ARRAY);
    }

    // Test 8
    #[test]
    fn should_return_empty_for_empty_document() {
        let text = "";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert!(symbols.is_empty());
    }

    // Test 9 — renamed from should_return_empty_when_ast_is_none
    // There is no longer an "AST is None" state; the caller passes an empty slice.
    #[test]
    fn should_return_empty_for_empty_slice() {
        let symbols = document_symbols(&[]);
        assert!(symbols.is_empty());
    }

    // Test 10
    #[test]
    fn should_return_symbols_for_multi_document_yaml() {
        let text = "doc1key: value1\n---\ndoc2key: value2\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        let has_doc1 = symbols.iter().any(|s| s.name == "doc1key")
            || symbols.iter().any(|s| {
                s.children
                    .as_ref()
                    .is_some_and(|c| c.iter().any(|ch| ch.name == "doc1key"))
            });
        let has_doc2 = symbols.iter().any(|s| s.name == "doc2key")
            || symbols.iter().any(|s| {
                s.children
                    .as_ref()
                    .is_some_and(|c| c.iter().any(|ch| ch.name == "doc2key"))
            });
        assert!(has_doc1, "should contain doc1key");
        assert!(has_doc2, "should contain doc2key");
    }

    // Test 11
    #[test]
    fn should_set_ranges_on_symbols() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        let sym = &symbols[0];
        assert_eq!(sym.range.start.line, 0);
        assert_eq!(sym.selection_range.start.line, 0);
    }

    // Test 12
    #[test]
    fn should_set_selection_range_to_key_span() {
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        let sym = &symbols[0];
        assert_eq!(sym.selection_range.start.character, 0);
        assert_eq!(sym.selection_range.end.character, 4); // "name" is 4 chars
    }

    // Test 13
    #[test]
    fn should_handle_sequence_of_mappings() {
        let text = "users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        let users = &symbols[0];
        assert_eq!(users.name, "users");
        assert_eq!(users.kind, SymbolKind::ARRAY);
        let children = users.children.as_ref().expect("users should have children");
        assert!(
            children.len() >= 2,
            "should have at least 2 sequence item children"
        );
        // Each sequence item should have name and age as children
        let first = &children[0];
        let first_children = first
            .children
            .as_ref()
            .expect("first item should have children");
        assert!(
            first_children.iter().any(|c| c.name == "name"),
            "first item should have 'name'"
        );
        assert!(
            first_children.iter().any(|c| c.name == "age"),
            "first item should have 'age'"
        );
    }

    // Test 14
    #[test]
    fn should_return_empty_for_comment_only_document() {
        let text = "# just a comment\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert!(symbols.is_empty());
    }

    // Test 15
    #[test]
    fn should_handle_document_with_only_separator() {
        let text = "---\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        // Should not panic; may return empty or minimal symbols
        let _ = symbols;
    }

    // Tests 16-17 — yaml_to_symbols: non-mapping root returns empty
    #[rstest]
    #[case::sequence_root("- one\n- two\n- three\n")]
    #[case::scalar_root("just a scalar\n")]
    fn returns_empty_for_non_mapping_root(#[case] text: &str) {
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        assert!(
            symbols.is_empty(),
            "non-mapping root should produce no symbols, got: {symbols:?}"
        );
    }

    // Test 18 — bare dash items produce sequence children without panic
    #[test]
    fn should_produce_sequence_children_for_bare_dash_items() {
        let text = "items:\n  -\n  - two\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        // Should not panic; items symbol should still be produced
        let items = find_symbol(&symbols, "items");
        assert!(items.is_some(), "should have 'items' symbol");
    }

    // Test 19 — integer-keyed mappings do not panic
    #[test]
    fn should_handle_integer_keyed_mapping() {
        let text = "1: one\n2: two\n";
        let docs = parse_docs(text);
        let _symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
    }

    // Test 20 — renamed from should_include_pre_separator_region_when_content_precedes_first_separator
    // Validates correct multi-doc handling through AST path (no longer tests split_document_regions).
    #[test]
    fn should_return_symbols_from_both_docs_when_content_precedes_separator() {
        let text = "before: separator\n---\nafter: separator\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        let has_before = symbols.iter().any(|s| s.name == "before");
        let has_after = symbols.iter().any(|s| s.name == "after");
        assert!(has_before, "should have 'before' symbol");
        assert!(has_after, "should have 'after' symbol");
    }

    // Test 21 — sequence item with Mapping value
    #[test]
    fn should_produce_symbols_for_sequence_of_mappings_with_multiple_keys() {
        let text = "list:\n  - a: 1\n    b: 2\n  - a: 3\n    b: 4\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        let list = find_symbol(&symbols, "list").expect("list symbol");
        assert_eq!(list.kind, SymbolKind::ARRAY);
        let children = list.children.as_ref().expect("list children");
        assert!(children.len() >= 2, "should have at least 2 items");
        // Each item should itself have children (a, b)
        let first = &children[0];
        let first_children = first.children.as_ref().expect("first item children");
        assert!(first_children.iter().any(|c| c.name == "a"));
        assert!(first_children.iter().any(|c| c.name == "b"));
    }

    // Test 22 — value that spans multiple lines
    #[test]
    fn should_extend_symbol_range_to_last_child_line() {
        let text = "root:\n  child1: a\n  child2: b\n  child3: c\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        let root = find_symbol(&symbols, "root").expect("root symbol");
        assert!(
            root.range.end.line >= 3,
            "root range should extend to last child line, got: {:?}",
            root.range.end.line
        );
    }

    // Test 23 — empty documents slice
    #[test]
    fn should_return_empty_for_empty_documents_vec() {
        let symbols = document_symbols(&[]);
        assert!(
            symbols.is_empty(),
            "empty documents should produce no symbols"
        );
    }

    // -----------------------------------------------------------------------
    // New rstest regression cases (UT-NEW-A through UT-NEW-D)
    // -----------------------------------------------------------------------

    // UT-NEW-A: UTF-8 key selection_range covers the full key (codepoint-accurate)
    #[test]
    fn utf8_key_selection_range_covers_full_key() {
        let text = "名前: Alice\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1, "should have exactly 1 symbol");
        let sym = &symbols[0];
        assert_eq!(sym.selection_range.start.character, 0);
        assert_eq!(
            sym.selection_range.end.character, 2,
            "selection_range.end.character should be 2 codepoints (名前 = 2 chars)"
        );
    }

    // UT-NEW-B: Deeply nested mapping satisfies range-enclosure invariant at every level
    #[test]
    fn deeply_nested_range_enclosure() {
        let text = "a:\n  b:\n    c:\n      d: leaf\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        assert_eq!(symbols.len(), 1);
        let a = &symbols[0];
        assert!(
            a.range.end.line >= 3,
            "a.range.end.line should reach last line, got {}",
            a.range.end.line
        );

        let b_children = a.children.as_ref().expect("a should have children");
        let b = find_symbol(b_children, "b").expect("should have 'b'");
        assert!(
            b.range.end.line >= 3,
            "b.range.end.line should reach last line, got {}",
            b.range.end.line
        );
        // Enclosure: a contains b
        assert!(
            a.range.end >= b.range.end,
            "a.range.end {:?} should >= b.range.end {:?}",
            a.range.end,
            b.range.end
        );

        let c_children = b.children.as_ref().expect("b should have children");
        let c = find_symbol(c_children, "c").expect("should have 'c'");
        assert!(
            b.range.end >= c.range.end,
            "b.range.end {:?} should >= c.range.end {:?}",
            b.range.end,
            c.range.end
        );
    }

    // UT-NEW-C: Sequence-of-mappings uses label-key heuristic to name children.
    // YAML has `name` as first key, so items are named by the value, not [0]/[1].
    // detail shows the original index when label-key is used.
    #[test]
    fn sequence_of_mappings_indexed_children_with_grandchildren() {
        let text = "users:\n  - name: Alice\n    age: 30\n  - name: Bob\n    age: 25\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        let users = find_symbol(&symbols, "users").expect("users symbol");
        assert_eq!(users.kind, SymbolKind::ARRAY);
        let children = users.children.as_ref().expect("users children");
        assert_eq!(children.len(), 2, "should have exactly 2 sequence items");

        let item0 = &children[0];
        assert_eq!(item0.name, "Alice", "first item should use label-key value");
        assert_eq!(
            item0.detail.as_deref(),
            Some("[0]"),
            "detail shows original index when label-key used"
        );
        let item1 = &children[1];
        assert_eq!(item1.name, "Bob", "second item should use label-key value");
        assert_eq!(
            item1.detail.as_deref(),
            Some("[1]"),
            "detail shows original index when label-key used"
        );

        // Items should have name and age as grand-children
        let gc0 = item0.children.as_ref().expect("Alice should have children");
        assert!(gc0.iter().any(|c| c.name == "name"), "Alice missing 'name'");
        assert!(gc0.iter().any(|c| c.name == "age"), "Alice missing 'age'");

        let gc1 = item1.children.as_ref().expect("Bob should have children");
        assert!(gc1.iter().any(|c| c.name == "name"), "Bob missing 'name'");

        // selection_range is exactly 1 character wide (item content start, AST-derived)
        assert_eq!(
            item0.selection_range.start.line, item0.selection_range.end.line,
            "selection_range is single-line"
        );
        assert_eq!(
            item0.selection_range.end.character,
            item0.selection_range.start.character + 1,
            "selection_range spans exactly 1 character"
        );
    }

    // UT-NEW-D: Multi-document YAML produces symbols from every doc, each scoped to its root
    #[test]
    fn multi_document_symbols_scoped_per_doc() {
        let text = "doc1: v1\n---\ndoc2: v2\n---\ndoc3: v3\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));

        let doc1_sym = find_symbol(&symbols, "doc1").expect("should have doc1");
        let doc3_sym = find_symbol(&symbols, "doc3").expect("should have doc3");
        assert!(find_symbol(&symbols, "doc2").is_some(), "should have doc2");

        assert_eq!(doc1_sym.range.start.line, 0, "doc1 should start at line 0");
        assert_eq!(doc3_sym.range.start.line, 4, "doc3 should start at line 4");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group 4: node_symbol_kind — tag-URI-driven SymbolKind mapping
    // ══════════════════════════════════════════════════════════════════════════

    // T4.1 — quoted integer-looking value gets STRING kind, not NUMBER
    #[test]
    fn tag_driven_quoted_integer_gets_string_symbol_kind() {
        let text = "count: \"42\"\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let sym = find_symbol(&symbols, "count").expect("should have 'count'");
        assert_eq!(
            sym.kind,
            SymbolKind::STRING,
            "quoted '42' has str tag — must be STRING, not NUMBER"
        );
    }

    // T4.2 — explicit !!null on a non-null-looking value gets NULL kind
    #[test]
    fn tag_driven_explicit_null_tag_gives_null_symbol_kind() {
        let text = "key: !!null foo\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let sym = find_symbol(&symbols, "key").expect("should have 'key'");
        assert_eq!(
            sym.kind,
            SymbolKind::NULL,
            "!!null tag must produce NULL symbol kind regardless of value content"
        );
    }

    // LSP-2: multibyte key — character offsets are codepoint-based, not byte-based
    //
    // selection_range covers the key span only ("日本語"), so end.character
    // must be 3 (codepoints) rather than 9 (bytes) or any byte-based count.
    #[test]
    fn symbols_multibyte_key_lsp_character_correct() {
        let text = "日本語: val\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let sym = find_symbol(&symbols, "日本語").expect("should have '日本語' symbol");
        assert_eq!(
            sym.selection_range.start.character, 0,
            "selection_range start.character should be 0, got {}",
            sym.selection_range.start.character
        );
        assert_eq!(
            sym.selection_range.end.character, 3,
            "selection_range end.character should be 3 (codepoints), not 9 (bytes), got {}",
            sym.selection_range.end.character
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // U-NEW: detail text and label-key heuristic tests
    // ══════════════════════════════════════════════════════════════════════════

    // U-NEW-1: short scalar value appears verbatim in detail
    #[test]
    fn scalar_detail_short_value_appears_verbatim() {
        let text = "key: hello\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        assert_eq!(symbols[0].detail.as_deref(), Some("hello"));
    }

    // U-NEW-2: scalar value of exactly 60 chars appears verbatim (no truncation at boundary)
    #[test]
    fn scalar_detail_exact_60_chars_appears_verbatim() {
        let value: String = "x".repeat(60);
        let text = format!("key: {value}\n");
        let docs = parse_docs(&text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        assert_eq!(
            symbols[0].detail.as_deref(),
            Some(value.as_str()),
            "exactly 60 chars should not be truncated"
        );
    }

    // U-NEW-3: scalar value of 61 chars is truncated with ellipsis suffix
    #[test]
    fn scalar_detail_over_60_chars_is_truncated_with_ellipsis() {
        let value: String = "x".repeat(61);
        let text = format!("key: {value}\n");
        let docs = parse_docs(&text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let detail = symbols[0].detail.as_deref().expect("should have detail");
        assert!(
            detail.ends_with('…'),
            "detail should end with ellipsis, got: {detail}"
        );
        let without_ellipsis: String = detail.chars().filter(|&c| c != '…').collect();
        assert_eq!(
            without_ellipsis.chars().count(),
            60,
            "content before ellipsis should be exactly 60 chars"
        );
    }

    // U-NEW-4: mapping value detail shows "N keys"
    #[test]
    fn mapping_value_detail_shows_n_keys() {
        let text = "server:\n  host: localhost\n  port: 8080\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let server = find_symbol(&symbols, "server").expect("should have 'server'");
        assert_eq!(server.detail.as_deref(), Some("2 keys"));
    }

    // U-NEW-5: mapping value detail singular and plural
    #[rstest]
    #[case::one_key("cfg:\n  debug: true\n", "1 key")]
    #[case::two_keys("cfg:\n  a: 1\n  b: 2\n", "2 keys")]
    fn mapping_value_detail_singular_and_plural(#[case] text: &str, #[case] expected: &str) {
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let cfg = find_symbol(&symbols, "cfg").expect("should have 'cfg'");
        assert_eq!(cfg.detail.as_deref(), Some(expected));
    }

    // U-NEW-6: sequence value detail shows "N items"
    #[test]
    fn sequence_value_detail_shows_n_items() {
        let text = "tags:\n  - alpha\n  - beta\n  - gamma\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let tags = find_symbol(&symbols, "tags").expect("should have 'tags'");
        assert_eq!(tags.detail.as_deref(), Some("3 items"));
    }

    // U-NEW-7: sequence value detail singular and plural
    #[rstest]
    #[case::one_item("tags:\n  - only\n", "1 item")]
    #[case::two_items("tags:\n  - a\n  - b\n", "2 items")]
    fn sequence_value_detail_singular_and_plural(#[case] text: &str, #[case] expected: &str) {
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let tags = find_symbol(&symbols, "tags").expect("should have 'tags'");
        assert_eq!(tags.detail.as_deref(), Some(expected));
    }

    // U-NEW-8: label key "name" used as sequence item name
    #[test]
    fn label_key_name_used_as_sequence_item_name() {
        let text = "items:\n  - name: nginx\n    image: nginx:latest\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let items = find_symbol(&symbols, "items").expect("should have 'items'");
        let children = items.children.as_ref().expect("items should have children");
        assert_eq!(children[0].name, "nginx");
        assert_eq!(children[0].detail.as_deref(), Some("[0]"));
    }

    // U-NEW-9: label key "id" used as sequence item name
    #[test]
    fn label_key_id_used_as_sequence_item_name() {
        let text = "rules:\n  - id: rule-001\n    action: allow\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let rules = find_symbol(&symbols, "rules").expect("should have 'rules'");
        let children = rules.children.as_ref().expect("rules should have children");
        assert_eq!(children[0].name, "rule-001");
        assert_eq!(children[0].detail.as_deref(), Some("[0]"));
    }

    // U-NEW-10: label key "key" used as sequence item name
    #[test]
    fn label_key_key_used_as_sequence_item_name() {
        let text = "entries:\n  - key: primary\n    value: 1\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let entries = find_symbol(&symbols, "entries").expect("should have 'entries'");
        let children = entries
            .children
            .as_ref()
            .expect("entries should have children");
        assert_eq!(children[0].name, "primary");
        assert_eq!(children[0].detail.as_deref(), Some("[0]"));
    }

    // U-NEW-11: non-label first key falls back to index
    #[test]
    fn non_label_first_key_falls_back_to_index() {
        let text = "list:\n  - host: db.local\n    port: 5432\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let list = find_symbol(&symbols, "list").expect("should have 'list'");
        let children = list.children.as_ref().expect("list should have children");
        assert_eq!(children[0].name, "[0]");
        assert!(children[0].detail.is_none());
    }

    // U-NEW-12: label key present but value is not a scalar falls back to index
    #[test]
    fn label_key_present_but_value_not_scalar_falls_back_to_index() {
        let text = "items:\n  - name:\n      nested: true\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let items = find_symbol(&symbols, "items").expect("should have 'items'");
        let children = items.children.as_ref().expect("items should have children");
        assert_eq!(children[0].name, "[0]");
        assert!(children[0].detail.is_none());
    }

    // U-NEW-13: sequence item that is a scalar falls back to index
    #[test]
    fn sequence_item_not_a_mapping_falls_back_to_index() {
        let text = "tags:\n  - alpha\n  - beta\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let tags = find_symbol(&symbols, "tags").expect("should have 'tags'");
        let children = tags.children.as_ref().expect("tags should have children");
        assert_eq!(children[0].name, "[0]");
        assert!(children[0].detail.is_none());
        assert_eq!(children[1].name, "[1]");
        assert!(children[1].detail.is_none());
    }

    // U-NEW-14: label key check uses first entry only — "name" as second key does not match
    #[test]
    fn label_key_check_uses_first_entry_only() {
        let text = "items:\n  - value: first\n    name: alice\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let items = find_symbol(&symbols, "items").expect("should have 'items'");
        let children = items.children.as_ref().expect("items should have children");
        assert_eq!(
            children[0].name, "[0]",
            "should fall back because 'value' (first key) is not in label list"
        );
        assert!(children[0].detail.is_none());
    }

    // U-NEW-15: mixed sequence — some items labeled, some fallback
    #[test]
    fn mixed_sequence_items_some_labeled_some_fallback() {
        let text = "items:\n  - name: web\n    port: 80\n  - host: db\n    port: 5432\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let items = find_symbol(&symbols, "items").expect("should have 'items'");
        let children = items.children.as_ref().expect("items should have children");
        assert_eq!(children[0].name, "web");
        assert_eq!(children[0].detail.as_deref(), Some("[0]"));
        assert_eq!(children[1].name, "[1]");
        assert!(children[1].detail.is_none());
    }

    // U-NEW-16: empty mapping sequence item falls back to index
    #[test]
    fn empty_mapping_sequence_item_falls_back_to_index() {
        let text = "items:\n  - {}\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(docs.as_deref().unwrap_or(&[]));
        let items = find_symbol(&symbols, "items").expect("should have 'items'");
        let children = items.children.as_ref().expect("items should have children");
        assert_eq!(children[0].name, "[0]");
        assert!(children[0].detail.is_none());
    }
}
