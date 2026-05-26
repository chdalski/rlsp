// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Document;
use tower_lsp::lsp_types::{CompletionItem, Position};

use crate::schema::JsonSchema;

mod completion_drivers;
mod completion_items;
mod cursor_location;
mod formatting;
mod navigation;
mod schema_completions;
mod support;

use completion_drivers::{complete_in_sequence_item, complete_on_key, complete_on_value};
use completion_items::keys_to_items;
use cursor_location::{CursorLocation, locate_cursor};
use navigation::{collect_sequence_sibling_keys, collect_sibling_keys_ast, present_keys};
use schema_completions::{resolve_schema_path, schema_has_properties, schema_key_completions};
use support::MAX_COMPLETION_ITEMS;

/// Compute completion items for the given cursor position within the AST.
///
/// When `schema` is provided, schema-defined properties and enum values are
/// merged with structural (document-based) suggestions. Falls back to structural
/// completion when `schema` is `None` or has no relevant properties.
///
/// Returns an empty list when `docs` is empty, the cursor is outside any node,
/// or the cursor is on a comment or document separator.
#[must_use]
pub fn complete_at(
    docs: &[Document<Span>],
    position: Position,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem> {
    let cursor_line = position.line as usize;
    let mut items = match locate_cursor(docs, position) {
        CursorLocation::OutsideAny => Vec::new(),
        CursorLocation::OnKey {
            key,
            enclosing_path,
            mapping,
        } => complete_on_key(docs, cursor_line, key, &enclosing_path, mapping, schema),
        CursorLocation::OnValue {
            key,
            enclosing_path,
            ..
        } => complete_on_value(docs, cursor_line, &key, enclosing_path, schema),
        CursorLocation::InSequenceItem {
            enclosing_path,
            sequence,
            current_item,
        } => complete_in_sequence_item(enclosing_path, sequence, current_item, schema),
        CursorLocation::InBlankSequence {
            enclosing_path,
            sequence,
        } => {
            if let Some(s) = schema {
                let mut items_path = enclosing_path;
                items_path.push("[]".to_string());
                if let Some(items_schema) = resolve_schema_path(s, &items_path)
                    && schema_has_properties(items_schema)
                {
                    return schema_key_completions(items_schema, &HashSet::new());
                }
            }
            keys_to_items(
                collect_sequence_sibling_keys(sequence)
                    .into_iter()
                    .collect(),
            )
        }
        CursorLocation::InBlankMapping {
            enclosing_path,
            mapping,
        } => {
            let present = docs.first().map_or_else(HashSet::new, |d| {
                present_keys(mapping, cursor_line, d.line_index())
            });
            if let Some(s) = schema {
                if let Some(resolved_schema) = resolve_schema_path(s, &enclosing_path)
                    && schema_has_properties(resolved_schema)
                {
                    return schema_key_completions(resolved_schema, &present);
                }
            }
            keys_to_items(
                collect_sibling_keys_ast(mapping)
                    .into_iter()
                    .filter(|k| !present.contains(k.as_str()))
                    .collect(),
            )
        }
    };
    items.truncate(MAX_COMPLETION_ITEMS);
    items
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;
    use tower_lsp::lsp_types::CompletionItemKind;

    use super::support::test_fixtures::{
        integer_schema, labels, object_schema, pos, string_schema,
    };
    use super::*;
    use crate::schema::{JsonSchema, SchemaType};
    use crate::test_utils::parse_docs;

    // ══════════════════════════════════════════════════════════════════════════
    // Backward-Compatibility Tests (Tests 1–15): None schema
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 1, 3, 4, 5 — sibling key suggestions and exclusions
    #[rstest]
    #[case::sibling_keys(
        "name: Alice\nage: 30\n",
        pos(0, 0),
        &["age"][..],
        &["name"][..]
    )]
    #[case::nested_sibling_keys(
        "server:\n  host: localhost\n  port: 8080\n",
        pos(1, 2),
        &["port"][..],
        &["server", "host"][..]
    )]
    #[case::deeply_nested_keys(
        "a:\n  b:\n    c: 1\n    d: 2\n",
        pos(2, 4),
        &["d"][..],
        &["a", "b", "c"][..]
    )]
    #[case::sequence_item_sibling(
        "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n",
        pos(3, 4),
        &["age"][..],
        &[][..]
    )]
    fn sibling_key_suggests_and_excludes(
        #[case] text: &str,
        #[case] cursor: tower_lsp::lsp_types::Position,
        #[case] expected: &[&str],
        #[case] absent: &[&str],
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, None);
        let ls = labels(&result);
        for key in expected {
            assert!(ls.contains(key), "should suggest {key:?}, got: {ls:?}");
        }
        for key in absent {
            assert!(!ls.contains(key), "should not suggest {key:?}, got: {ls:?}");
        }
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD)),
            "all no-schema key completions should have FIELD kind"
        );
    }

    #[test]
    fn should_not_suggest_keys_already_present_in_mapping() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), None);

        let ls = labels(&result);
        assert!(
            !ls.contains(&"name"),
            "should not suggest 'name' which is at the cursor line"
        );
    }

    #[test]
    fn should_not_suggest_keys_already_in_current_sequence_item() {
        let text = "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 4), None);

        let ls = labels(&result);
        assert!(
            !ls.contains(&"name"),
            "should not suggest 'name' already present in current sequence item"
        );
    }

    #[test]
    fn should_suggest_values_seen_for_same_key_name() {
        let text = "items:\n  - env: production\n  - env: staging\n  - env: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 10), None);

        let ls = labels(&result);
        assert!(
            ls.contains(&"production"),
            "should suggest value 'production', got: {ls:?}"
        );
        assert!(
            ls.contains(&"staging"),
            "should suggest value 'staging', got: {ls:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "value completions should have VALUE kind"
        );
    }

    #[test]
    fn should_not_suggest_duplicate_values() {
        let text = "items:\n  - env: production\n  - env: production\n  - env: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 10), None);

        let ls = labels(&result);
        let production_count = ls.iter().filter(|&&l| l == "production").count();
        assert_eq!(
            production_count, 1,
            "should deduplicate: 'production' should appear only once, got: {ls:?}"
        );
    }

    #[test]
    fn should_return_empty_when_ast_is_none() {
        let result = complete_at(&[], pos(0, 0), None);

        assert!(
            result.is_empty(),
            "should return empty when AST is None (failed parse)"
        );
    }

    // Tests 9, 11, 12, 13, 14 — empty result for various degenerate inputs (no schema)
    #[rstest]
    #[case::empty_document("", pos(0, 0))]
    #[case::comment_line("# comment\nkey: value\n", pos(0, 0))]
    #[case::document_separator("key1: v1\n---\nkey2: v2\n", pos(1, 0))]
    #[case::position_beyond_lines("key: value\n", pos(10, 0))]
    #[case::position_beyond_line_length("key: value\n", pos(0, 100))]
    fn returns_empty_for_structural_no_schema(
        #[case] text: &str,
        #[case] cursor: tower_lsp::lsp_types::Position,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, None);
        assert!(result.is_empty(), "should return empty, got: {result:?}");
    }

    #[test]
    fn should_return_empty_for_no_documents() {
        use rlsp_yaml_parser::Span;
        use rlsp_yaml_parser::node::Document;
        let empty: Vec<Document<Span>> = Vec::new();
        let result = complete_at(&empty, pos(0, 0), None);

        assert!(
            result.is_empty(),
            "should return empty for empty documents vector"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group F — Fallback (structural-only subset)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_fall_back_to_structural_completion_when_schema_is_none() {
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), None);

        let ls = labels(&result);
        assert!(
            ls.contains(&"age"),
            "structural sibling 'age' should appear when schema is None"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group H — Multi-Document Boundary Tests
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 51, 52, 54 — cross-document label contamination prevention
    #[rstest]
    #[case::sibling_not_cross_dash("alpha: 1\n---\nbeta: 2\n", pos(2, 0), None, "alpha")]
    #[case::sibling_not_cross_ellipsis("alpha: 1\n...\nbeta: 2\n", pos(2, 0), None, "alpha")]
    #[case::values_not_from_other_doc(
        "env: production\n---\nenv: \n",
        pos(2, 5),
        None,
        "production"
    )]
    fn cross_document_label_not_contaminated(
        #[case] text: &str,
        #[case] cursor: tower_lsp::lsp_types::Position,
        #[case] schema: Option<&JsonSchema>,
        #[case] absent_label: &str,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, schema);
        let ls = labels(&result);
        assert!(
            !ls.contains(&absent_label),
            "should not suggest {absent_label:?} from other document, got: {ls:?}"
        );
    }

    #[test]
    fn should_not_suppress_schema_key_present_only_in_other_document() {
        let schema = object_schema(vec![("name", string_schema()), ("age", integer_schema())]);
        let text = "name: Alice\n---\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(2, 0), Some(&schema));

        let ls = labels(&result);
        assert!(
            ls.contains(&"name"),
            "should suggest 'name' because it is absent from document 2, got: {ls:?}"
        );
    }

    #[test]
    fn should_not_detect_sequence_context_from_other_document() {
        let text = "items:\n  - name: Alice\n---\nhost: local\nport: 8080\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 0), None);

        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest sibling key 'port' in document 2, got: {ls:?}"
        );
        assert!(
            !ls.contains(&"name"),
            "should not suggest 'name' from the sequence in document 1, got: {ls:?}"
        );
    }

    #[test]
    fn should_handle_cursor_on_first_line_of_multi_doc_file() {
        let text = "alpha: 1\n---\nbeta: 2\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), None);

        let ls = labels(&result);
        assert!(
            !ls.contains(&"beta"),
            "should not suggest 'beta' from document 2 when cursor is on line 0, got: {ls:?}"
        );
    }

    #[test]
    fn should_handle_cursor_on_last_line_of_multi_doc_file() {
        let text = "alpha: 1\n---\nbeta: 2\ngamma: 3\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 0), None);

        let ls = labels(&result);
        assert!(
            ls.contains(&"beta"),
            "should suggest sibling 'beta' from the same document, got: {ls:?}"
        );
        assert!(
            !ls.contains(&"alpha"),
            "should not suggest 'alpha' from document 1, got: {ls:?}"
        );
    }

    #[test]
    fn should_handle_consecutive_document_separators() {
        let text = "alpha: 1\n---\n---\nbeta: 2\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(3, 0), None);

        let ls = labels(&result);
        assert!(
            !ls.contains(&"alpha"),
            "should not suggest 'alpha' from document 1 through empty middle document, got: {ls:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group J — structural/blank-line subset
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::no_schema("key: value\n\n", pos(1, 0), None)]
    #[case::schema_no_properties("\n", pos(0, 0), Some(JsonSchema::default()))]
    fn blank_line_returns_empty(
        #[case] text: &str,
        #[case] cursor: tower_lsp::lsp_types::Position,
        #[case] schema: Option<JsonSchema>,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, schema.as_ref());
        assert!(
            result.is_empty(),
            "blank line should return empty, got: {result:?}"
        );
    }

    // ── C tests: complete_at branch coverage ─────────────────────────────────

    // C-1: OutsideAny returns empty
    #[test]
    fn complete_at_outside_any_returns_empty() {
        let docs = parse_docs("name: Alice\n");
        let result = complete_at(&docs, pos(5, 0), None);
        assert!(
            result.is_empty(),
            "OutsideAny should return empty, got: {result:?}"
        );
    }

    // C-2: OnKey no schema, no siblings
    #[test]
    fn complete_at_on_key_with_no_siblings_returns_empty() {
        let docs = parse_docs("only: val\n");
        let result = complete_at(&docs, pos(0, 0), None);
        assert!(
            result.is_empty(),
            "single key with no siblings should return empty, got: {result:?}"
        );
    }

    // C-3: OnKey with schema, present key excluded
    #[test]
    fn complete_at_on_key_schema_excludes_present_keys() {
        let docs = parse_docs("name: Alice\nage: 30\n");
        let schema = object_schema(vec![
            ("name", string_schema()),
            ("age", integer_schema()),
            ("city", string_schema()),
        ]);
        let result = complete_at(&docs, pos(0, 0), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"city"), "should suggest 'city', got: {ls:?}");
        assert!(
            !ls.contains(&"name"),
            "should exclude cursor key 'name', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"age"),
            "should exclude present key 'age', got: {ls:?}"
        );
    }

    // C-4: OnValue with schema enum
    #[test]
    fn complete_at_on_value_schema_enum() {
        let docs = parse_docs("env: \n");
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("prod"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let result = complete_at(&docs, pos(0, 5), Some(&schema));
        let ls = labels(&result);
        assert!(ls.contains(&"prod"), "should suggest 'prod', got: {ls:?}");
        assert!(
            ls.contains(&"staging"),
            "should suggest 'staging', got: {ls:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "enum suggestions should have VALUE kind"
        );
    }

    // C-5: OnValue no schema, structural fallback
    #[test]
    fn complete_at_on_value_no_schema_structural_fallback() {
        let docs = parse_docs("kind: app\nkind: \n");
        let result = complete_at(&docs, pos(1, 6), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"app"),
            "should suggest existing value 'app', got: {ls:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "structural value suggestions should have VALUE kind"
        );
    }

    // C-6: InSequenceItem, sibling keys minus current-item keys
    #[test]
    fn complete_at_in_sequence_item_suggests_missing_sibling_keys() {
        let docs = parse_docs("items:\n  - name: Alice\n    age: 30\n  - name: Bob\n");
        let result = complete_at(&docs, pos(3, 4), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"age"),
            "should suggest sibling key 'age', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"name"),
            "should exclude current item key 'name', got: {ls:?}"
        );
    }

    // C-7: InBlankMapping with schema suggests missing schema key
    #[test]
    fn complete_at_in_blank_mapping_with_schema_suggests_keys() {
        let docs = parse_docs("server:\n  host: localhost\n  \n");
        let schema = object_schema(vec![(
            "server",
            object_schema(vec![("host", string_schema()), ("port", integer_schema())]),
        )]);
        let result = complete_at(&docs, pos(2, 2), Some(&schema));
        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest schema key 'port', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"host"),
            "should exclude present key 'host', got: {ls:?}"
        );
    }

    // C-8: InBlankMapping without schema — suggests nothing when all keys present.
    #[test]
    fn complete_at_in_blank_mapping_no_schema_structural_keys() {
        let docs = parse_docs("name: Alice\nage: 30\n\n");
        let result = complete_at(&docs, pos(2, 0), None);
        assert!(
            result.is_empty(),
            "all keys already present — blank-line no-schema should return empty, got: {result:?}"
        );
    }

    // C-9: InBlankSequence with schema descends via [] sentinel
    #[test]
    fn complete_at_in_blank_sequence_with_schema_descends_items() {
        let docs = parse_docs("servers:\n  - host: localhost\n  \n");
        let schema = object_schema(vec![(
            "servers",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(object_schema(vec![
                    ("host", string_schema()),
                    ("port", integer_schema()),
                ]))),
                ..JsonSchema::default()
            },
        )]);
        let result = complete_at(&docs, pos(2, 2), Some(&schema));
        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest schema item key 'port', got: {ls:?}"
        );
    }

    // C-10: InSequenceItem — cursor on blank within a sequence item returns keys
    // from sibling items that are absent from the current item.
    #[test]
    fn complete_at_in_blank_sequence_no_schema_union_of_sibling_keys() {
        let docs =
            parse_docs("items:\n  - name: Alice\n    age: 30\n  \n  - name: Bob\n    city: NY\n");
        let result = complete_at(&docs, pos(3, 2), None);
        let ls = labels(&result);
        assert!(
            ls.contains(&"city"),
            "should suggest 'city' from sibling item, got: {ls:?}"
        );
        assert!(
            result
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::FIELD)),
            "structural key suggestions should have FIELD kind"
        );
    }
}
