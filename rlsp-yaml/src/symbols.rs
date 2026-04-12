// SPDX-License-Identifier: MIT

use std::fmt::Write;

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::{Document, Node};
use tower_lsp::lsp_types::{DocumentSymbol, Position, Range, SymbolKind};

use crate::scalar_helpers;

/// Produce a hierarchical list of document symbols for the given YAML text.
///
/// Returns an empty vector if the text is empty, the AST is unavailable,
/// or the document contains only comments.
#[must_use]
pub fn document_symbols(
    text: &str,
    documents: Option<&Vec<Document<Span>>>,
) -> Vec<DocumentSymbol> {
    let Some(documents) = documents else {
        return Vec::new();
    };
    if documents.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();

    // Split the text into document regions by `---` separators
    let doc_regions = split_document_regions(&lines);

    doc_regions
        .iter()
        .enumerate()
        .filter_map(|(doc_idx, region)| documents.get(doc_idx).map(|doc| (doc, region)))
        .flat_map(|(doc, region)| yaml_to_symbols(&doc.root, &lines, region.start_line))
        .collect()
}

/// A region of lines belonging to a single YAML document.
struct DocRegion {
    start_line: usize,
}

/// Split the text into document regions based on `---` separators.
fn split_document_regions(lines: &[&str]) -> Vec<DocRegion> {
    let mut regions = Vec::new();
    let mut current_start = 0;
    let mut found_first = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if found_first {
                regions.push(DocRegion {
                    start_line: current_start,
                });
                current_start = i + 1;
            } else {
                // First separator — the first document starts at line 0
                if i > 0 {
                    regions.push(DocRegion {
                        start_line: current_start,
                    });
                }
                current_start = i + 1;
                found_first = true;
            }
        }
    }

    // Add the last (or only) region
    if regions.is_empty() || current_start <= lines.len() {
        regions.push(DocRegion {
            start_line: if regions.is_empty() { 0 } else { current_start },
        });
    }

    // If we never found a separator, there's one region starting at line 0
    if regions.is_empty() {
        regions.push(DocRegion { start_line: 0 });
    }

    regions
}

/// Convert a `Node<Span>` into `DocumentSymbol` objects using the text for ranges.
fn yaml_to_symbols(node: &Node<Span>, lines: &[&str], base_line: usize) -> Vec<DocumentSymbol> {
    match node {
        Node::Mapping { entries, .. } => entries
            .iter()
            .filter_map(|(key, value)| make_symbol(&node_to_string(key), value, lines, base_line))
            .collect(),
        Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => Vec::new(),
    }
}

/// Create a `DocumentSymbol` for a key-value pair.
#[expect(
    deprecated,
    reason = "DocumentSymbol.deprecated field is required by LSP spec"
)]
fn make_symbol(
    key: &str,
    value: &Node<Span>,
    lines: &[&str],
    search_from: usize,
) -> Option<DocumentSymbol> {
    let kind = node_symbol_kind(value);
    let (key_line, key_col) = find_key_in_lines(key, lines, search_from)?;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let key_line_u32 = key_line as u32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let key_col_u32 = key_col as u32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let key_end_col = (key_col + key.len()) as u32;

    let selection_range = Range::new(
        Position::new(key_line_u32, key_col_u32),
        Position::new(key_line_u32, key_end_col),
    );

    // Determine the end of this symbol's range
    let end_line = find_value_end_line(lines, key_line);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let end_line_u32 = end_line as u32;
    let end_col = lines.get(end_line).map_or(0, |l| l.len());
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let end_col_u32 = end_col as u32;

    let range = Range::new(
        Position::new(key_line_u32, key_col_u32),
        Position::new(end_line_u32, end_col_u32),
    );

    let children = match value {
        Node::Mapping { entries, .. } => {
            let child_start = key_line + 1;
            let child_symbols: Vec<DocumentSymbol> = entries
                .iter()
                .filter_map(|(k, v)| make_symbol(&node_to_string(k), v, lines, child_start))
                .collect();
            if child_symbols.is_empty() {
                None
            } else {
                Some(child_symbols)
            }
        }
        Node::Sequence { items, .. } => {
            let child_start = key_line + 1;
            let child_symbols = make_sequence_children(items, lines, child_start);
            if child_symbols.is_empty() {
                None
            } else {
                Some(child_symbols)
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => None,
    };

    Some(DocumentSymbol {
        name: key.to_string(),
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children,
    })
}

/// Create child symbols for sequence items.
#[expect(
    deprecated,
    reason = "DocumentSymbol.deprecated field is required by LSP spec"
)]
fn make_sequence_children(
    items: &[Node<Span>],
    lines: &[&str],
    search_from: usize,
) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();
    let mut line_cursor = search_from;

    for (idx, item) in items.iter().enumerate() {
        // Find the next sequence item marker "- " starting from line_cursor
        let item_line = find_sequence_item_line(lines, line_cursor);
        let Some(item_line) = item_line else {
            continue;
        };

        let mut name = String::new();
        let _ = write!(name, "[{idx}]");
        let kind = node_symbol_kind(item);

        #[expect(
            clippy::cast_possible_truncation,
            reason = "LSP line/col are u32; always fits"
        )]
        let item_line_u32 = item_line as u32;
        let end_line = find_value_end_line(lines, item_line);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "LSP line/col are u32; always fits"
        )]
        let end_line_u32 = end_line as u32;
        let end_col = lines.get(end_line).map_or(0, |l| l.len());
        #[expect(
            clippy::cast_possible_truncation,
            reason = "LSP line/col are u32; always fits"
        )]
        let end_col_u32 = end_col as u32;

        let dash_col = lines.get(item_line).and_then(|l| l.find("- ")).unwrap_or(0);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "LSP line/col are u32; always fits"
        )]
        let dash_col_u32 = dash_col as u32;

        let range = Range::new(
            Position::new(item_line_u32, dash_col_u32),
            Position::new(end_line_u32, end_col_u32),
        );
        let selection_range = Range::new(
            Position::new(item_line_u32, dash_col_u32),
            Position::new(item_line_u32, dash_col_u32 + 1),
        );

        let item_children = match item {
            Node::Mapping { entries, .. } => {
                let cs: Vec<DocumentSymbol> = entries
                    .iter()
                    .filter_map(|(k, v)| make_symbol(&node_to_string(k), v, lines, item_line))
                    .collect();
                if cs.is_empty() { None } else { Some(cs) }
            }
            Node::Scalar { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
        };

        children.push(DocumentSymbol {
            name,
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: item_children,
        });

        line_cursor = end_line + 1;
    }

    children
}

/// Find the next line starting with a sequence item marker (`- `).
fn find_sequence_item_line(lines: &[&str], from: usize) -> Option<usize> {
    (from..lines.len()).find(|&i| {
        lines
            .get(i)
            .is_some_and(|l| l.trim().starts_with("- ") || l.trim() == "-")
    })
}

/// Find the line and column where a key appears in the text.
fn find_key_in_lines(key: &str, lines: &[&str], from: usize) -> Option<(usize, usize)> {
    for i in from..lines.len() {
        let line = lines.get(i)?;
        let trimmed = line.trim();

        // Skip comments, empty lines, separators
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" || trimmed == "..." {
            continue;
        }

        // Check for "key:" or "key: value" pattern
        // Also handle "- key: value" (sequence item with mapping)
        let search_line = trimmed.strip_prefix("- ").unwrap_or(trimmed);

        if let Some(colon_pos) = find_mapping_colon(search_line) {
            let found_key = search_line[..colon_pos].trim();
            if found_key == key {
                // Calculate the actual column in the original line
                let offset = line.len() - line.trim_start().len();
                let prefix = if trimmed.starts_with("- ") {
                    offset + 2
                } else {
                    offset
                };
                return Some((i, prefix));
            }
        }
    }
    None
}

/// Find the position of the mapping colon in a YAML line.
/// Skips colons inside quoted strings.
fn find_mapping_colon(line: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ':' if !in_single_quote && !in_double_quote => {
                let rest = &line[i + 1..];
                if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Determine the end line for a value that starts on `key_line`.
/// Walks forward until indentation decreases or end of document region.
fn find_value_end_line(lines: &[&str], key_line: usize) -> usize {
    let key_indent = lines
        .get(key_line)
        .map_or(0, |l| l.len() - l.trim_start().len());

    let mut last_content_line = key_line;

    for i in (key_line + 1)..lines.len() {
        let Some(line) = lines.get(i) else {
            break;
        };
        let trimmed = line.trim();

        // Skip empty lines — they don't end a block
        if trimmed.is_empty() {
            continue;
        }

        // Document separator ends the block
        if trimmed == "---" || trimmed == "..." {
            break;
        }

        let indent = line.len() - line.trim_start().len();
        if indent <= key_indent {
            break;
        }

        last_content_line = i;
    }

    last_content_line
}

/// Map a `Node<Span>` value to the appropriate `SymbolKind`.
fn node_symbol_kind(node: &Node<Span>) -> SymbolKind {
    match node {
        Node::Mapping { .. } => SymbolKind::OBJECT,
        Node::Sequence { .. } => SymbolKind::ARRAY,
        Node::Scalar { value, .. } => {
            if scalar_helpers::is_null(value) {
                SymbolKind::NULL
            } else if scalar_helpers::is_bool(value) {
                SymbolKind::BOOLEAN
            } else if scalar_helpers::is_integer(value) || scalar_helpers::is_float(value) {
                SymbolKind::NUMBER
            } else {
                SymbolKind::STRING
            }
        }
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
    use tower_lsp::lsp_types::SymbolKind;

    fn parse_docs(text: &str) -> Option<Vec<Document<Span>>> {
        rlsp_yaml_parser::load(text).ok()
    }

    fn find_symbol<'a>(symbols: &'a [DocumentSymbol], name: &str) -> Option<&'a DocumentSymbol> {
        symbols.iter().find(|s| s.name == name)
    }

    // Test 1
    #[test]
    fn should_return_symbols_for_flat_mapping() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "items");
        assert_eq!(symbols[0].kind, SymbolKind::ARRAY);
    }

    // Test 4
    #[test]
    fn should_return_nested_symbols() {
        let text = "server:\n  host: localhost\n  port: 8080\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

        assert!(symbols.is_empty());
    }

    // Test 9
    #[test]
    fn should_return_empty_when_ast_is_none() {
        let text = "key: [bad";
        let symbols = document_symbols(text, None);

        assert!(symbols.is_empty());
    }

    // Test 10
    #[test]
    fn should_return_symbols_for_multi_document_yaml() {
        let text = "doc1key: value1\n---\ndoc2key: value2\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

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
        let symbols = document_symbols(text, docs.as_ref());

        assert!(symbols.is_empty());
    }

    // Test 15
    #[test]
    fn should_handle_document_with_only_separator() {
        let text = "---\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

        // Should not panic; may return empty or minimal symbols
        let _ = symbols;
    }

    // Tests 16-17 — yaml_to_symbols: non-mapping root returns empty
    #[rstest]
    #[case::sequence_root("- one\n- two\n- three\n")]
    #[case::scalar_root("just a scalar\n")]
    fn returns_empty_for_non_mapping_root(#[case] text: &str) {
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());
        assert!(
            symbols.is_empty(),
            "non-mapping root should produce no symbols, got: {symbols:?}"
        );
    }

    // Test 18 — find_sequence_item_line: bare dash (no space after)
    #[test]
    fn should_produce_sequence_children_for_bare_dash_items() {
        let text = "items:\n  -\n  - two\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

        // Should not panic; items symbol should still be produced
        let items = find_symbol(&symbols, "items");
        assert!(items.is_some(), "should have 'items' symbol");
    }

    // Test 19 — node_to_string: integer key
    #[test]
    fn should_handle_integer_keyed_mapping() {
        let text = "1: one\n2: two\n";
        let docs = parse_docs(text);
        // Should not panic even with integer keys
        let _symbols = document_symbols(text, docs.as_ref());
    }

    // Test 20 — split_document_regions: content before first ---
    #[test]
    fn should_include_pre_separator_region_when_content_precedes_first_separator() {
        let text = "before: separator\n---\nafter: separator\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

        let has_before = symbols.iter().any(|s| s.name == "before");
        let has_after = symbols.iter().any(|s| s.name == "after");
        assert!(has_before, "should have 'before' symbol");
        assert!(has_after, "should have 'after' symbol");
    }

    // Test 21 — make_sequence_children: sequence item with Mapping value
    #[test]
    fn should_produce_symbols_for_sequence_of_mappings_with_multiple_keys() {
        let text = "list:\n  - a: 1\n    b: 2\n  - a: 3\n    b: 4\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

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

    // Test 22 — find_value_end_line: value that spans multiple lines
    #[test]
    fn should_extend_symbol_range_to_last_child_line() {
        let text = "root:\n  child1: a\n  child2: b\n  child3: c\n";
        let docs = parse_docs(text);
        let symbols = document_symbols(text, docs.as_ref());

        let root = find_symbol(&symbols, "root").expect("root symbol");
        assert!(
            root.range.end.line >= 3,
            "root range should extend to last child line, got: {:?}",
            root.range.end.line
        );
    }

    // Test 23 — empty documents list
    #[test]
    fn should_return_empty_for_empty_documents_vec() {
        let text = "key: value\n";
        let empty: Vec<Document<Span>> = Vec::new();
        let symbols = document_symbols(text, Some(&empty));

        assert!(
            symbols.is_empty(),
            "empty documents should produce no symbols"
        );
    }
}
