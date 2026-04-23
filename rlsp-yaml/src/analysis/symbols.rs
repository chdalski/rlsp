// SPDX-License-Identifier: MIT

use std::fmt::Write;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{Pos, Span};
use tower_lsp::lsp_types::{DocumentSymbol, Position, Range, SymbolKind};

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
        .flat_map(|doc| yaml_to_symbols(&doc.root))
        .collect()
}

/// Convert a `Node<Span>` into `DocumentSymbol` objects.
fn yaml_to_symbols(node: &Node<Span>) -> Vec<DocumentSymbol> {
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .map(|(key, value)| make_symbol(&node_to_string(key), key, value))
            .collect(),
        Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => Vec::new(),
    }
}

/// Create a `DocumentSymbol` for a key-value pair.
#[expect(
    deprecated,
    reason = "DocumentSymbol.deprecated field is required by LSP spec"
)]
fn make_symbol(key_name: &str, key_node: &Node<Span>, value: &Node<Span>) -> DocumentSymbol {
    let key_loc = node_loc(key_node);
    let value_loc = node_loc(value);

    let selection_range = span_to_lsp_range(key_loc);
    let range = Range::new(pos_to_lsp(key_loc.start), pos_to_lsp(value_loc.end));

    let children = match value {
        Node::Mapping { entries, .. } => {
            let child_symbols: Vec<DocumentSymbol> = entries
                .iter()
                .map(|(k, v)| make_symbol(&node_to_string(k), k, v))
                .collect();
            if child_symbols.is_empty() {
                None
            } else {
                Some(child_symbols)
            }
        }
        Node::Sequence { items, .. } => {
            let child_symbols = make_sequence_children(items);
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
        detail: None,
        kind: node_symbol_kind(value),
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children,
    }
}

/// Create child symbols for sequence items.
#[expect(
    deprecated,
    reason = "DocumentSymbol.deprecated field is required by LSP spec"
)]
fn make_sequence_children(items: &[Node<Span>]) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        let item_loc = node_loc(item);

        let mut name = String::new();
        let _ = write!(name, "[{idx}]");

        let range = span_to_lsp_range(item_loc);
        // selection_range is a single character at the item content start (AST-derived).
        let start = pos_to_lsp(item_loc.start);
        let sel_end = Position::new(start.line, start.character + 1);
        let selection_range = Range::new(start, sel_end);

        let item_children = match item {
            Node::Mapping { entries, .. } => {
                let cs: Vec<DocumentSymbol> = entries
                    .iter()
                    .map(|(k, v)| make_symbol(&node_to_string(k), k, v))
                    .collect();
                if cs.is_empty() { None } else { Some(cs) }
            }
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
        };

        children.push(DocumentSymbol {
            name,
            detail: None,
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

/// Convert a parser `Span` (1-based line, 0-based column) to an LSP `Range` (both 0-based).
fn span_to_lsp_range(span: Span) -> Range {
    Range::new(pos_to_lsp(span.start), pos_to_lsp(span.end))
}

/// Convert a parser `Pos` (1-based line, 0-based column) to an LSP `Position` (both 0-based).
#[expect(
    clippy::cast_possible_truncation,
    reason = "LSP line/col are u32; always fits"
)]
fn pos_to_lsp(pos: Pos) -> Position {
    Position::new(pos.line.saturating_sub(1) as u32, pos.column as u32)
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

    // UT-NEW-C: Sequence-of-mappings produces [0], [1] children with mapping-key grand-children
    // and selection_range is a single character at item content start (AST-derived)
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
        assert_eq!(item0.name, "[0]");
        let item1 = &children[1];
        assert_eq!(item1.name, "[1]");

        // [0] should have name and age as grand-children
        let gc0 = item0.children.as_ref().expect("[0] should have children");
        assert!(gc0.iter().any(|c| c.name == "name"), "[0] missing 'name'");
        assert!(gc0.iter().any(|c| c.name == "age"), "[0] missing 'age'");

        // [1] should have name as grand-child
        let gc1 = item1.children.as_ref().expect("[1] should have children");
        assert!(gc1.iter().any(|c| c.name == "name"), "[1] missing 'name'");

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
}
