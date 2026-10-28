// SPDX-License-Identifier: MIT

use std::collections::HashSet;
use std::ops::ControlFlow;
use std::ptr;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemTag, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind,
};

use crate::schema::path::composition_branches;
use crate::schema::{JsonSchema, ParsedSchema, SchemaType};

use super::formatting::{
    json_value_to_yaml_label, truncate_description, truncate_enum_label, type_label,
};
use super::support::MAX_COMPLETION_ITEMS;

/// Visit every property `schema` offers, directly and through `allOf`,
/// `anyOf` and `oneOf` branches, until `visit` breaks. `visit` gets each
/// property's schema resolved. A property whose schema is `false` allows no
/// value, so it is not visited.
///
/// Own properties come first, then each branch list in that order, depth
/// first. A shared target is entered once, wherever the branches reach it, so
/// the walk is linear in the schema tree plus its targets and ends on a schema
/// that references itself. It keeps an explicit stack, so the depth of a chain
/// of references does not grow the call stack.
fn walk_properties<'a>(
    parsed: &'a ParsedSchema,
    schema: &'a JsonSchema,
    visit: &mut impl FnMut(&'a str, &'a JsonSchema) -> ControlFlow<()>,
) -> ControlFlow<()> {
    // Targets are told apart by address: `schema` may already be a resolved
    // target that a branch refers to again.
    let mut entered: HashSet<*const JsonSchema> = HashSet::from([ptr::from_ref(schema)]);
    let mut stack = vec![schema];
    while let Some(node) = stack.pop() {
        let schema = parsed.resolve(node);
        if node.reference.is_some() && !entered.insert(ptr::from_ref(schema)) {
            continue;
        }
        if let Some(props) = &schema.properties {
            for (key, prop_schema) in props {
                let prop_schema = parsed.resolve(prop_schema);
                // No value is allowed for the key, so it is no suggestion.
                if !prop_schema.always_fails {
                    visit(key, prop_schema)?;
                }
            }
        }
        stack.extend(composition_branches(schema).rev());
    }
    ControlFlow::Continue(())
}

/// Return true if the schema has any properties to suggest (direct or via composition).
pub(super) fn schema_has_properties(parsed: &ParsedSchema, schema: &JsonSchema) -> bool {
    walk_properties(parsed, schema, &mut |_, _| ControlFlow::Break(())).is_break()
}

/// Produce key completion items from a resolved schema, excluding already-present keys.
pub(super) fn schema_key_completions(
    parsed: &ParsedSchema,
    schema: &JsonSchema,
    present: &HashSet<String>,
) -> Vec<CompletionItem> {
    let schema = parsed.resolve(schema);
    let mut items: Vec<CompletionItem> = Vec::new();
    collect_schema_properties(parsed, schema, present, &mut items);

    // If 2+ required properties are missing, offer a snippet that inserts them all at once.
    if let Some(required) = &schema.required {
        let missing: Vec<&String> = required
            .iter()
            .filter(|r| !present.contains(r.as_str()))
            .filter(|r| {
                !schema
                    .properties
                    .as_ref()
                    .and_then(|props| props.get(r.as_str()))
                    .is_some_and(|prop| parsed.resolve(prop).always_fails)
            })
            .collect();
        if missing.len() >= 2 {
            let snippet_body: String = missing
                .iter()
                .enumerate()
                .map(|(idx, key)| {
                    let n = idx + 1;
                    let default = schema
                        .properties
                        .as_ref()
                        .and_then(|props| props.get(*key))
                        .map_or("", |prop| snippet_default(parsed.resolve(prop)));
                    if default.is_empty() {
                        format!("{key}: ${{{n}:}}")
                    } else {
                        format!("{key}: ${{{n}:{default}}}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            items.push(CompletionItem {
                label: "(all required)".to_string(),
                kind: Some(CompletionItemKind::SNIPPET),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text: Some(snippet_body),
                sort_text: Some("!".to_string()),
                detail: Some(format!("{} required properties", missing.len())),
                ..CompletionItem::default()
            });
        }
    }

    items
}

/// Return the snippet placeholder default for a schema based on its type.
fn snippet_default(schema: &JsonSchema) -> &'static str {
    match schema.schema_type.as_ref() {
        Some(SchemaType::Single(t)) => match t.as_str() {
            "string" => "\"\"",
            "integer" | "number" => "0",
            "boolean" => "false",
            "object" => "{}",
            "array" => "[]",
            _ => "",
        },
        _ => "",
    }
}

/// Collect completion items for the properties of a schema and its composition
/// branches, stopping once `MAX_COMPLETION_ITEMS` items exist.
pub(super) fn collect_schema_properties(
    parsed: &ParsedSchema,
    schema: &JsonSchema,
    present: &HashSet<String>,
    items: &mut Vec<CompletionItem>,
) {
    let _ = walk_properties(parsed, schema, &mut |key, prop_schema| {
        if present.contains(key) {
            return ControlFlow::Continue(());
        }
        let detail = type_label(prop_schema);
        let documentation = prop_schema.description.as_deref().map(|d| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: truncate_description(d),
            })
        });
        let (tags, sort_text) = if prop_schema.deprecated == Some(true) {
            (
                Some(vec![CompletionItemTag::DEPRECATED]),
                Some(format!("~{key}")),
            )
        } else {
            (None, None)
        };
        items.push(CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail,
            documentation,
            tags,
            sort_text,
            ..CompletionItem::default()
        });
        if items.len() >= MAX_COMPLETION_ITEMS {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
}

/// Return the set of all property names defined in a schema (direct + composition branches).
pub(super) fn collect_schema_properties_keys(
    parsed: &ParsedSchema,
    schema: &JsonSchema,
) -> HashSet<String> {
    let mut keys = HashSet::new();
    let _ = walk_properties(parsed, schema, &mut |key, _| {
        if !keys.contains(key) {
            keys.insert(key.to_string());
        }
        ControlFlow::Continue(())
    });
    keys
}

/// Produce value completion items from a schema (enum values or boolean type).
pub(super) fn schema_value_completions(schema: &JsonSchema) -> Vec<CompletionItem> {
    // Enum values take priority.
    if let Some(enum_vals) = &schema.enum_values {
        let detail = type_label(schema);
        return enum_vals
            .iter()
            .filter_map(|v| {
                let label = json_value_to_yaml_label(v)?;
                let label = truncate_enum_label(&label);
                Some(CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::VALUE),
                    detail: detail.clone(),
                    ..CompletionItem::default()
                })
            })
            .collect();
    }

    // Boolean type → suggest "true" and "false".
    if matches!(&schema.schema_type, Some(SchemaType::Single(t)) if t == "boolean") {
        return vec![
            CompletionItem {
                label: "true".to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "false".to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..CompletionItem::default()
            },
        ];
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;
    use tower_lsp::lsp_types::{CompletionItemKind, Documentation, InsertTextFormat};

    use super::super::support::test_fixtures::{
        boolean_schema, integer_schema, labels, object_schema, pos, string_schema,
    };
    use super::*;
    use crate::completion::complete_at;
    use crate::schema::test_support::{
        all_of, any_of, field_branches, nest_in_all_of, no_targets, one_of,
    };
    use crate::schema::{JsonSchema, ParsedSchema, SchemaType};
    use crate::test_utils::parse_docs;

    // ── test helper ──────────────────────────────────────────────────────────

    fn schema_with_required(props: Vec<(&str, JsonSchema)>, required: Vec<&str>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            required: Some(required.into_iter().map(str::to_string).collect()),
            ..JsonSchema::default()
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Direct unit tests: schema_has_properties
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn schema_has_properties_with_direct_properties_returns_true() {
        let schema = object_schema(vec![("x", string_schema())]);
        assert!(schema_has_properties(&no_targets(), &schema));
    }

    #[test]
    fn schema_has_properties_empty_properties_map_returns_false() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(std::collections::HashMap::new()),
            ..JsonSchema::default()
        };
        assert!(!schema_has_properties(&no_targets(), &schema));
    }

    #[test]
    fn schema_has_properties_no_properties_returns_false() {
        assert!(!schema_has_properties(
            &no_targets(),
            &JsonSchema::default()
        ));
    }

    #[test]
    fn schema_has_properties_via_allof_branch_returns_true() {
        let schema = JsonSchema {
            all_of: Some(vec![object_schema(vec![("x", string_schema())])]),
            ..JsonSchema::default()
        };
        assert!(schema_has_properties(&no_targets(), &schema));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Direct unit tests: collect_schema_properties_keys
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn collect_schema_properties_keys_direct_properties() {
        let schema = object_schema(vec![("a", string_schema()), ("b", integer_schema())]);
        let keys = collect_schema_properties_keys(&no_targets(), &schema);
        assert!(keys.contains("a"), "should contain 'a'");
        assert!(keys.contains("b"), "should contain 'b'");
    }

    #[test]
    fn collect_schema_properties_keys_via_composition() {
        let schema = JsonSchema {
            one_of: Some(vec![
                object_schema(vec![("x", string_schema())]),
                object_schema(vec![("y", integer_schema())]),
            ]),
            ..JsonSchema::default()
        };
        let keys = collect_schema_properties_keys(&no_targets(), &schema);
        assert!(keys.contains("x"), "should contain 'x' from first branch");
        assert!(keys.contains("y"), "should contain 'y' from second branch");
    }

    #[test]
    fn collect_schema_properties_keys_empty_schema() {
        let keys = collect_schema_properties_keys(&no_targets(), &JsonSchema::default());
        assert!(keys.is_empty(), "empty schema should produce empty key set");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Property walk over composition branches without a branch or depth limit
    // ══════════════════════════════════════════════════════════════════════════

    fn item_labels(items: &[CompletionItem]) -> Vec<&str> {
        items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::FIELD))
            .map(|i| i.label.as_str())
            .collect()
    }

    #[rstest]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn schema_has_properties_finds_property_in_branch_beyond_twenty(
        #[case] compose: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let mut branches = vec![JsonSchema::default(); 24];
        branches.push(object_schema(vec![("late", string_schema())]));

        assert!(schema_has_properties(&no_targets(), &compose(branches)));
    }

    #[test]
    fn schema_has_properties_is_false_when_no_branch_has_properties() {
        assert!(!schema_has_properties(
            &no_targets(),
            &any_of(vec![JsonSchema::default(); 25])
        ));
    }

    #[test]
    fn collect_schema_properties_keys_includes_branches_beyond_twenty() {
        let keys = collect_schema_properties_keys(&no_targets(), &all_of(field_branches(30)));

        assert_eq!(keys.len(), 30);
        assert!(keys.contains("field_0"));
        assert!(keys.contains("field_29"));
    }

    #[test]
    fn collect_schema_properties_keys_includes_nesting_beyond_twenty_levels() {
        let schema = nest_in_all_of(object_schema(vec![("leaf", string_schema())]), 30);

        assert!(collect_schema_properties_keys(&no_targets(), &schema).contains("leaf"));
    }

    #[test]
    fn schema_key_completions_walk_own_properties_then_each_composition_list_in_order() {
        let schema = JsonSchema {
            all_of: Some(vec![
                all_of(vec![object_schema(vec![("inner", string_schema())])]),
                object_schema(vec![("sibling", string_schema())]),
            ]),
            any_of: Some(vec![object_schema(vec![("from_any_of", string_schema())])]),
            one_of: Some(vec![object_schema(vec![("from_one_of", string_schema())])]),
            ..object_schema(vec![("own", string_schema())])
        };

        let items = schema_key_completions(&no_targets(), &schema, &HashSet::new());

        assert_eq!(
            item_labels(&items),
            ["own", "inner", "sibling", "from_any_of", "from_one_of"]
        );
    }

    #[test]
    fn schema_key_completions_stop_at_max_completion_items_across_branches() {
        let branch = |prefix: &str| {
            object_schema(
                (0..60)
                    .map(|i| (format!("{prefix}_{i}"), string_schema()))
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect(),
            )
        };
        let schema = all_of(vec![
            branch("first"),
            branch("second"),
            object_schema(vec![("never_reached", string_schema())]),
        ]);

        let items = schema_key_completions(&no_targets(), &schema, &HashSet::new());
        let labels = item_labels(&items);

        assert_eq!(labels.len(), MAX_COMPLETION_ITEMS);
        assert_eq!(
            labels.iter().filter(|l| l.starts_with("first_")).count(),
            60
        );
        assert_eq!(
            labels.iter().filter(|l| l.starts_with("second_")).count(),
            40
        );
        assert!(!labels.contains(&"never_reached"));
    }

    #[test]
    fn schema_key_completions_skipped_present_keys_do_not_count_toward_the_cut() {
        let present: HashSet<String> = (0..MAX_COMPLETION_ITEMS)
            .map(|i| format!("skipped_{i}"))
            .collect();
        let skipped: Vec<(String, JsonSchema)> = present
            .iter()
            .map(|k| (k.clone(), string_schema()))
            .collect();
        let schema = all_of(vec![
            object_schema(
                skipped
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect(),
            ),
            object_schema(vec![("tail", string_schema())]),
        ]);

        let items = schema_key_completions(&no_targets(), &schema, &present);

        assert_eq!(item_labels(&items), ["tail"]);
    }

    #[test]
    fn schema_key_completions_keep_a_key_defined_in_two_branches_twice() {
        let schema = all_of(vec![
            object_schema(vec![("dup", string_schema())]),
            object_schema(vec![("dup", string_schema())]),
        ]);

        let items = schema_key_completions(&no_targets(), &schema, &HashSet::new());

        assert_eq!(item_labels(&items), ["dup", "dup"]);
    }

    #[test]
    fn schema_key_completions_are_empty_when_every_branch_is_empty() {
        let schema = all_of(vec![JsonSchema::default(); 3]);

        assert!(schema_key_completions(&no_targets(), &schema, &HashSet::new()).is_empty());
    }

    #[test]
    fn walks_find_properties_in_a_very_deep_and_a_very_wide_schema() {
        let deep = nest_in_all_of(object_schema(vec![("leaf", string_schema())]), 500);
        let mut wide_branches = vec![JsonSchema::default(); 5_000];
        wide_branches.push(object_schema(vec![("leaf", string_schema())]));
        let wide = all_of(wide_branches);

        for schema in [&deep, &wide] {
            assert!(schema_has_properties(&no_targets(), schema));
            assert!(collect_schema_properties_keys(&no_targets(), schema).contains("leaf"));
            assert_eq!(
                item_labels(&schema_key_completions(
                    &no_targets(),
                    schema,
                    &HashSet::new()
                )),
                ["leaf"]
            );
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Direct unit tests: schema_value_completions
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn schema_value_completions_with_enum_returns_value_items() {
        let schema = JsonSchema {
            enum_values: Some(vec![json!("a"), json!("b")]),
            ..JsonSchema::default()
        };
        let items = schema_value_completions(&schema);
        assert_eq!(items.len(), 2);
        let ls: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(ls.contains(&"a"), "should contain 'a'");
        assert!(ls.contains(&"b"), "should contain 'b'");
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "all items should have VALUE kind"
        );
    }

    #[test]
    fn schema_value_completions_boolean_type_returns_true_false() {
        let items = schema_value_completions(&boolean_schema());
        let ls: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(ls.contains(&"true"), "should contain 'true'");
        assert!(ls.contains(&"false"), "should contain 'false'");
        assert!(
            items
                .iter()
                .all(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "boolean items should have VALUE kind"
        );
    }

    #[test]
    fn schema_value_completions_no_enum_no_boolean_returns_empty() {
        let items = schema_value_completions(&string_schema());
        assert!(
            items.is_empty(),
            "string schema with no enum should return empty"
        );
    }

    #[test]
    fn schema_value_completions_skips_array_and_object_enum_values() {
        let schema = JsonSchema {
            enum_values: Some(vec![json!("valid"), json!(["a", "b"]), json!({"k": "v"})]),
            ..JsonSchema::default()
        };
        let items = schema_value_completions(&schema);
        let ls: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(ls.contains(&"valid"), "string enum value should appear");
        assert_eq!(
            ls.len(),
            1,
            "array and object enum values should be skipped"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group B — Schema Key Completion at Key Positions (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_suggest_schema_properties_at_top_level_key_position() {
        let schema = object_schema(vec![("name", string_schema()), ("age", integer_schema())]);
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"age"),
            "should suggest schema property 'age', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"name"),
            "should not suggest 'name' which is already present"
        );
        assert!(
            result
                .iter()
                .any(|i| i.kind == Some(CompletionItemKind::FIELD)),
            "schema key completions should have FIELD kind"
        );
    }

    #[test]
    fn should_include_schema_detail_and_documentation_in_key_suggestion() {
        let schema = object_schema(vec![(
            "name",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                description: Some("The user's name".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let text = "age: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let item = result.iter().find(|i| i.label == "name");
        assert!(
            item.is_some(),
            "should suggest 'name', got: {:?}",
            labels(&result)
        );
        let item = item.unwrap();
        assert_eq!(
            item.detail.as_deref(),
            Some("string"),
            "detail should be the type 'string'"
        );
        let has_description = match &item.documentation {
            Some(Documentation::String(s)) => s.contains("The user's name"),
            Some(Documentation::MarkupContent(m)) => m.value.contains("The user's name"),
            None => false,
        };
        assert!(
            has_description,
            "documentation should contain 'The user's name'"
        );
    }

    #[test]
    fn should_suggest_all_schema_properties_when_mapping_is_empty() {
        let schema = object_schema(vec![
            ("host", JsonSchema::default()),
            ("port", JsonSchema::default()),
            ("timeout", JsonSchema::default()),
        ]);
        let text = "host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(ls.contains(&"port"), "should suggest 'port'");
        assert!(ls.contains(&"timeout"), "should suggest 'timeout'");
        assert!(
            !ls.contains(&"host"),
            "should not suggest 'host' (already present)"
        );
    }

    #[test]
    fn should_not_suggest_schema_properties_already_in_document() {
        let schema = object_schema(vec![
            ("a", JsonSchema::default()),
            ("b", JsonSchema::default()),
            ("c", JsonSchema::default()),
        ]);
        let text = "a: 1\nb: 2\nc: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(2, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            !ls.contains(&"a"),
            "should not suggest 'a' (already present)"
        );
        assert!(
            !ls.contains(&"b"),
            "should not suggest 'b' (already present)"
        );
        assert!(!ls.contains(&"c"), "should not suggest 'c' (current line)");
    }

    #[test]
    fn should_suggest_schema_properties_for_nested_key_position() {
        let schema = object_schema(vec![(
            "server",
            object_schema(vec![("host", string_schema()), ("port", integer_schema())]),
        )]);
        let text = "server:\n  host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 2), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(ls.contains(&"port"), "should suggest nested 'port'");
        assert!(
            !ls.contains(&"host"),
            "should not suggest 'host' (already present)"
        );
        assert!(
            !ls.contains(&"server"),
            "should not suggest parent 'server'"
        );
    }

    #[test]
    fn should_merge_schema_and_structural_suggestions() {
        let schema = object_schema(vec![("kind", string_schema())]);
        let text = "name: Alice\nkind: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(ls.contains(&"kind"), "schema property 'kind' should appear");
        assert!(
            !ls.contains(&"name"),
            "current key 'name' should not appear"
        );
    }

    #[test]
    fn should_deduplicate_when_schema_and_structure_both_suggest_same_key() {
        let schema = object_schema(vec![("env", string_schema())]);
        let text = "env: production\nregion: us-east\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        let env_count = ls.iter().filter(|&&l| l == "env").count();
        assert!(
            env_count <= 1,
            "'env' should appear at most once, got: {ls:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group C — Schema Enum Completion at Value Positions (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_suggest_enum_values_at_value_position() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![
                    json!("production"),
                    json!("staging"),
                    json!("development"),
                ]),
                ..JsonSchema::default()
            },
        )]);
        let text = "env: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 5), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(ls.contains(&"production"), "should suggest 'production'");
        assert!(ls.contains(&"staging"), "should suggest 'staging'");
        assert!(ls.contains(&"development"), "should suggest 'development'");
        assert!(
            result
                .iter()
                .any(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "enum completions should have VALUE kind"
        );
    }

    #[test]
    fn should_include_schema_detail_in_enum_suggestion() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                enum_values: Some(vec![json!("prod"), json!("dev")]),
                description: Some("Deployment target".to_string()),
                ..JsonSchema::default()
            },
        )]);
        let text = "env: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 5), Some(&ParsedSchema::new(schema)));

        assert!(!result.is_empty(), "should have enum suggestions");
        assert!(
            result
                .iter()
                .any(|i| i.detail.as_deref().is_some_and(|d| d.contains("string"))),
            "at least one suggestion should have detail containing 'string'"
        );
    }

    #[test]
    fn should_not_duplicate_enum_value_already_used_in_same_key() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("production"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let text = "env: production\nenv: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 5), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        let prod_count = ls.iter().filter(|&&l| l == "production").count();
        assert!(prod_count <= 1, "'production' should appear at most once");
    }

    #[test]
    fn should_fall_back_to_structural_value_suggestions_when_no_schema_enum() {
        let schema = object_schema(vec![("env", string_schema())]);
        let text = "env: production\nenv: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 5), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"production"),
            "structural value 'production' should still appear as fallback"
        );
    }

    #[test]
    fn should_suggest_boolean_values_for_boolean_schema_type() {
        let schema = object_schema(vec![("enabled", boolean_schema())]);
        let text = "enabled: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 9), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(ls.contains(&"true"), "should suggest 'true'");
        assert!(ls.contains(&"false"), "should suggest 'false'");
        assert!(
            result
                .iter()
                .any(|i| i.kind == Some(CompletionItemKind::VALUE)),
            "boolean completions should have VALUE kind"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group D — Path Resolution (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::nested_path(
        object_schema(vec![("database", object_schema(vec![("host", string_schema()), ("port", integer_schema())]))]),
        "database:\n  host: localhost\n",
        pos(1, 2),
        "port",
        "database"
    )]
    #[case::array_items_schema(
        object_schema(vec![("servers", JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            items: Some(Box::new(object_schema(vec![("host", string_schema()), ("port", integer_schema())]))),
            ..JsonSchema::default()
        })]),
        "servers:\n  - host: localhost\n",
        pos(1, 4),
        "port",
        "servers"
    )]
    #[case::third_level_nesting(
        object_schema(vec![("a", object_schema(vec![("b", object_schema(vec![("c", string_schema()), ("d", integer_schema())]))]))]),
        "a:\n  b:\n    c: v\n",
        pos(2, 4),
        "d",
        "a"
    )]
    fn schema_path_resolution_suggests_nested_property(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] cursor: tower_lsp::lsp_types::Position,
        #[case] expected: &str,
        #[case] absent: &str,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, Some(&ParsedSchema::new(schema)));
        let ls = labels(&result);
        assert!(
            ls.contains(&expected),
            "should suggest {expected:?}, got: {ls:?}"
        );
        assert!(
            !ls.contains(&absent),
            "should not suggest {absent:?}, got: {ls:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group E — Composition Schemas (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::allof_branches(
        JsonSchema { all_of: Some(vec![object_schema(vec![("name", string_schema())]), object_schema(vec![("age", integer_schema())])]), ..JsonSchema::default() },
        "name: Alice\n",
        pos(0, 0),
        "age"
    )]
    #[case::anyof_branches(
        JsonSchema { any_of: Some(vec![object_schema(vec![("host", string_schema())]), object_schema(vec![("socket", string_schema())])]), ..JsonSchema::default() },
        "host: localhost\n",
        pos(0, 0),
        "socket"
    )]
    #[case::oneof_branches(
        JsonSchema { one_of: Some(vec![object_schema(vec![("url", string_schema())]), object_schema(vec![("path", string_schema())])]), ..JsonSchema::default() },
        "url: http://example.com\n",
        pos(0, 0),
        "path"
    )]
    fn composition_schema_suggests_from_branches(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] cursor: tower_lsp::lsp_types::Position,
        #[case] expected: &str,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, Some(&ParsedSchema::new(schema)));
        let ls = labels(&result);
        assert!(
            ls.contains(&expected),
            "should suggest {expected:?} from composition branches, got: {ls:?}"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group F — Fallback Behavior (schema-focused subset)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_fall_back_to_structural_when_schema_has_no_properties() {
        let schema = JsonSchema::default();
        let text = "name: Alice\nage: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"age"),
            "structural sibling 'age' should appear when schema has no properties"
        );
    }

    #[test]
    fn should_offer_schema_property_when_structural_has_no_siblings() {
        let schema = object_schema(vec![("unrelated", JsonSchema::default())]);
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"unrelated"),
            "schema property 'unrelated' should appear even when no structural siblings"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group G — Edge Cases (schema-focused subset)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_return_empty_for_schema_completion_on_empty_document() {
        let schema = object_schema(vec![("name", string_schema())]);
        let result = complete_at(&[], pos(0, 0), Some(&ParsedSchema::new(schema)));

        assert!(result.is_empty(), "should return empty for empty document");
    }

    #[test]
    fn should_return_empty_for_schema_completion_on_comment_line() {
        let schema = object_schema(vec![("name", string_schema())]);
        let text = "# comment\nkey: value\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        assert!(result.is_empty(), "should return empty for comment line");
    }

    #[test]
    fn should_return_empty_for_schema_completion_on_document_separator() {
        let schema = object_schema(vec![("name", string_schema())]);
        let text = "key1: v1\n---\nkey2: v2\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 0), Some(&ParsedSchema::new(schema)));

        assert!(
            result.is_empty(),
            "should return empty for document separator"
        );
    }

    #[test]
    fn should_handle_schema_property_with_no_type_gracefully() {
        let schema = object_schema(vec![("data", JsonSchema::default())]);
        let text = "name: Alice\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let item = result.iter().find(|i| i.label == "data");
        assert!(item.is_some(), "should suggest 'data' without panicking");
        let item = item.unwrap();
        if let Some(detail) = &item.detail {
            assert!(
                detail.is_empty(),
                "detail should be empty when schema has no type, got: {detail:?}"
            );
        }
    }

    #[test]
    fn should_handle_enum_completion_with_partial_value_at_cursor() {
        let schema = object_schema(vec![(
            "env",
            JsonSchema {
                enum_values: Some(vec![json!("production"), json!("staging")]),
                ..JsonSchema::default()
            },
        )]);
        let text = "env: pro\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 7), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"production") || ls.contains(&"staging"),
            "should return enum suggestions even with partial value at cursor"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Security / bounds tests (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_truncate_description_at_200_chars_in_completion_documentation() {
        let long_desc = "x".repeat(500);
        let schema = object_schema(vec![(
            "name",
            JsonSchema {
                description: Some(long_desc),
                ..JsonSchema::default()
            },
        )]);
        let text = "age: 30\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let item = result.iter().find(|i| i.label == "name");
        assert!(item.is_some(), "should suggest 'name'");
        if let Some(item) = item {
            let doc_char_count = match &item.documentation {
                Some(Documentation::String(s)) => s.chars().count(),
                Some(Documentation::MarkupContent(m)) => m.value.chars().count(),
                None => 0,
            };
            assert!(
                doc_char_count <= 200,
                "documentation should be truncated to 200 chars, got {doc_char_count}"
            );
        }
    }

    #[test]
    fn should_cap_completion_items_at_100_when_schema_has_many_properties() {
        let properties: std::collections::HashMap<String, JsonSchema> = (0..150)
            .map(|i| (format!("prop_{i:03}"), JsonSchema::default()))
            .collect();
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(properties),
            ..JsonSchema::default()
        };
        let text = "prop_000: x\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        assert!(
            result.len() <= 100,
            "completion items should be capped at 100, got {}",
            result.len()
        );
    }

    #[test]
    fn should_offer_keys_from_every_allof_branch_without_a_branch_cap() {
        let schema = all_of(field_branches(30));
        let text = "irrelevant: x\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let labels = item_labels(&result);
        assert_eq!(
            labels.len(),
            30,
            "all 30 allOf branches should contribute a key, got {labels:?}"
        );
        assert!(labels.contains(&"field_29"));
    }

    #[test]
    fn should_truncate_long_enum_labels_at_50_chars() {
        let long_val = "a".repeat(60);
        let schema = object_schema(vec![(
            "key",
            JsonSchema {
                enum_values: Some(vec![json!(long_val)]),
                ..JsonSchema::default()
            },
        )]);
        let text = "key: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 5), Some(&ParsedSchema::new(schema)));

        assert!(!result.is_empty(), "should have enum suggestion");
        for item in &result {
            assert!(
                item.label.chars().count() <= 50,
                "enum label should be truncated to 50 chars, got {} chars: {}",
                item.label.chars().count(),
                item.label
            );
        }
    }

    #[test]
    fn should_convert_json_boolean_enum_to_yaml_scalar_true_false() {
        let schema = object_schema(vec![(
            "enabled",
            JsonSchema {
                enum_values: Some(vec![json!(true), json!(false)]),
                ..JsonSchema::default()
            },
        )]);
        let text = "enabled: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 9), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"true"),
            "JSON boolean true should produce label 'true', got: {ls:?}"
        );
        assert!(
            ls.contains(&"false"),
            "JSON boolean false should produce label 'false', got: {ls:?}"
        );
        assert!(
            !ls.contains(&"\"true\""),
            "should not produce JSON-quoted string '\"true\"'"
        );
        assert!(
            !ls.contains(&"\"false\""),
            "should not produce JSON-quoted string '\"false\"'"
        );
    }

    #[test]
    fn should_return_no_schema_context_when_yaml_path_exceeds_schema_depth() {
        let schema = object_schema(vec![("a", object_schema(vec![("b", string_schema())]))]);
        let text = "a:\n  b:\n    c:\n      d:\n        e: v\n";
        let docs = parse_docs(text);
        let _result = complete_at(&docs, pos(4, 8), Some(&ParsedSchema::new(schema)));
    }

    #[test]
    fn should_exclude_already_present_keys_from_schema_suggestions() {
        let schema = object_schema(vec![
            ("a", JsonSchema::default()),
            ("b", JsonSchema::default()),
            ("c", JsonSchema::default()),
        ]);
        let text = "a: 1\nb: 2\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            !ls.contains(&"a"),
            "'a' is already present, should not appear"
        );
        assert!(
            !ls.contains(&"b"),
            "'b' is on cursor line, should not appear"
        );
        assert!(ls.contains(&"c"), "'c' is not present, should appear");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Deprecated property tagging (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_tag_deprecated_property_with_deprecated_tag_and_tilde_sort_text() {
        let schema = object_schema(vec![(
            "old_field",
            JsonSchema {
                deprecated: Some(true),
                ..JsonSchema::default()
            },
        )]);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let item = result
            .iter()
            .find(|i| i.label == "old_field")
            .expect("should suggest old_field");
        assert_eq!(
            item.tags,
            Some(vec![CompletionItemTag::DEPRECATED]),
            "deprecated property should have DEPRECATED tag"
        );
        assert!(
            item.sort_text
                .as_deref()
                .is_some_and(|s| s.starts_with('~')),
            "deprecated property sort_text should start with '~', got: {:?}",
            item.sort_text
        );
    }

    #[test]
    fn should_not_tag_non_deprecated_property() {
        let schema = object_schema(vec![(
            "current_field",
            JsonSchema {
                deprecated: None,
                ..JsonSchema::default()
            },
        )]);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let item = result
            .iter()
            .find(|i| i.label == "current_field")
            .expect("should suggest current_field");
        assert_eq!(
            item.tags, None,
            "non-deprecated property should have no tags"
        );
        assert_eq!(
            item.sort_text, None,
            "non-deprecated property should have no sort_text"
        );
    }

    #[test]
    fn should_only_tag_deprecated_property_in_mixed_schema() {
        let schema = object_schema(vec![
            (
                "new_field",
                JsonSchema {
                    deprecated: None,
                    ..JsonSchema::default()
                },
            ),
            (
                "old_field",
                JsonSchema {
                    deprecated: Some(true),
                    ..JsonSchema::default()
                },
            ),
        ]);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let new_item = result
            .iter()
            .find(|i| i.label == "new_field")
            .expect("should suggest new_field");
        let old_item = result
            .iter()
            .find(|i| i.label == "old_field")
            .expect("should suggest old_field");

        assert_eq!(
            new_item.tags, None,
            "non-deprecated 'new_field' should have no tags"
        );
        assert_eq!(
            old_item.tags,
            Some(vec![CompletionItemTag::DEPRECATED]),
            "deprecated 'old_field' should have DEPRECATED tag"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group I — Multi-Required Snippet Completion (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_offer_all_required_snippet_when_three_required_props_missing() {
        let schema = schema_with_required(
            vec![
                ("name", string_schema()),
                ("age", integer_schema()),
                ("enabled", boolean_schema()),
            ],
            vec!["name", "age", "enabled"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer '(all required)' snippet item");

        let insert_text = snippet
            .insert_text
            .as_deref()
            .expect("snippet item must have insert_text");

        assert!(
            insert_text.contains("${1:"),
            "snippet must contain tab-stop ${{1:...}}, got: {insert_text}"
        );
        assert!(
            insert_text.contains("${2:"),
            "snippet must contain tab-stop ${{2:...}}, got: {insert_text}"
        );
        assert!(
            insert_text.contains("${3:"),
            "snippet must contain tab-stop ${{3:...}}, got: {insert_text}"
        );
        assert!(
            insert_text.contains("name:"),
            "snippet must mention 'name', got: {insert_text}"
        );
        assert!(
            insert_text.contains("age:"),
            "snippet must mention 'age', got: {insert_text}"
        );
        assert!(
            insert_text.contains("enabled:"),
            "snippet must mention 'enabled', got: {insert_text}"
        );
    }

    #[rstest]
    #[case::only_one_missing(
        schema_with_required(
            vec![("name", string_schema()), ("age", integer_schema()), ("enabled", boolean_schema())],
            vec!["name", "age", "enabled"],
        ),
        "name: Alice\nage: 30\n",
        pos(0, 0)
    )]
    #[case::no_required_props(
        object_schema(vec![("name", string_schema()), ("age", integer_schema())]),
        "\n",
        pos(0, 0)
    )]
    fn should_not_offer_snippet(
        #[case] schema: JsonSchema,
        #[case] text: &str,
        #[case] cursor: tower_lsp::lsp_types::Position,
    ) {
        let docs = parse_docs(text);
        let result = complete_at(&docs, cursor, Some(&ParsedSchema::new(schema)));
        let has_snippet = result.iter().any(|i| i.label == "(all required)");
        assert!(!has_snippet, "should not offer '(all required)' snippet");
    }

    #[test]
    #[expect(
        clippy::literal_string_with_formatting_args,
        reason = "snippet placeholders look like format args"
    )]
    fn should_use_type_aware_defaults_in_snippet() {
        let schema = schema_with_required(
            vec![
                ("title", string_schema()),
                ("count", integer_schema()),
                ("active", boolean_schema()),
            ],
            vec!["title", "count", "active"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer snippet");

        let insert_text = snippet
            .insert_text
            .as_deref()
            .expect("must have insert_text");

        assert!(
            insert_text.contains("\"\""),
            "string type should default to \"\", got: {insert_text}"
        );
        assert!(
            insert_text.contains(":0")
                || insert_text.contains(": 0")
                || insert_text.contains("{1:0}")
                || insert_text.contains("{2:0}")
                || insert_text.contains("{3:0}"),
            "integer type should default to 0, got: {insert_text}"
        );
        assert!(
            insert_text.contains("false"),
            "boolean type should default to false, got: {insert_text}"
        );
    }

    #[test]
    fn should_set_insert_text_format_to_snippet() {
        let schema = schema_with_required(
            vec![("name", string_schema()), ("age", integer_schema())],
            vec!["name", "age"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer snippet");

        assert_eq!(
            snippet.insert_text_format,
            Some(InsertTextFormat::SNIPPET),
            "snippet item must have InsertTextFormat::SNIPPET"
        );
    }

    #[test]
    fn should_set_snippet_sort_text_to_exclamation() {
        let schema = schema_with_required(
            vec![("name", string_schema()), ("age", integer_schema())],
            vec!["name", "age"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let snippet = result
            .iter()
            .find(|i| i.label == "(all required)")
            .expect("should offer snippet");

        assert_eq!(
            snippet.sort_text.as_deref(),
            Some("!"),
            "snippet sort_text should be '!' to sort to top"
        );
    }

    #[test]
    fn should_use_object_default_in_snippet_for_object_type_required_field() {
        let schema = schema_with_required(
            vec![
                (
                    "config",
                    JsonSchema {
                        schema_type: Some(SchemaType::Single("object".to_string())),
                        ..JsonSchema::default()
                    },
                ),
                ("name", string_schema()),
            ],
            vec!["config", "name"],
        );
        let schema2 = schema_with_required(
            vec![
                (
                    "tags",
                    JsonSchema {
                        schema_type: Some(SchemaType::Single("array".to_string())),
                        ..JsonSchema::default()
                    },
                ),
                ("name", string_schema()),
            ],
            vec!["tags", "name"],
        );

        let text = "placeholder: null\n";
        let docs = parse_docs(text);

        let result1 = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));
        let snippet1 = result1.iter().find(|i| i.label == "(all required)");
        assert!(
            snippet1.is_some(),
            "should offer snippet for object-typed field"
        );
        let insert1 = snippet1.unwrap().insert_text.as_deref().unwrap_or("");
        assert!(
            insert1.contains("{}"),
            "object type default should be '{{}}', got: {insert1}"
        );

        let result2 = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema2)));
        let snippet2 = result2.iter().find(|i| i.label == "(all required)");
        assert!(
            snippet2.is_some(),
            "should offer snippet for array-typed field"
        );
        let insert2 = snippet2.unwrap().insert_text.as_deref().unwrap_or("");
        assert!(
            insert2.contains("[]"),
            "array type default should be '[]', got: {insert2}"
        );
    }

    #[test]
    fn should_use_bare_tab_stop_in_snippet_for_field_with_no_type() {
        let schema = schema_with_required(
            vec![("data", JsonSchema::default()), ("name", string_schema())],
            vec!["data", "name"],
        );
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let snippet = result.iter().find(|i| i.label == "(all required)");
        assert!(snippet.is_some(), "should offer snippet");
        let insert = snippet.unwrap().insert_text.as_deref().unwrap_or("");
        assert!(
            insert.contains("data: ${"),
            "no-type field should have a tab-stop, got: {insert}"
        );
    }

    #[test]
    fn should_offer_leaf_when_allof_nesting_exceeds_former_depth_limit() {
        fn deep_schema(depth: usize) -> JsonSchema {
            if depth == 0 {
                return object_schema(vec![("leaf", JsonSchema::default())]);
            }
            JsonSchema {
                all_of: Some(vec![deep_schema(depth - 1)]),
                ..JsonSchema::default()
            }
        }
        let schema = deep_schema(25);
        let text = "placeholder: null\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        assert!(item_labels(&result).contains(&"leaf"));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Group J — schema-helper subset (via complete_at)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn should_build_path_with_sequence_sentinel_for_bare_sequence_parent() {
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
        let text = "servers:\n  -\n    host: localhost\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(2, 4), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest 'port' via the item path segment, got: {ls:?}"
        );
    }

    #[test]
    #[expect(
        clippy::approx_constant,
        reason = "3.14 is a test value, not an approximation of PI"
    )]
    fn should_render_number_and_null_enum_values_as_yaml_labels() {
        let schema = object_schema(vec![(
            "value",
            JsonSchema {
                enum_values: Some(vec![
                    serde_json::Value::Number(serde_json::Number::from(42)),
                    serde_json::Value::Null,
                    serde_json::Value::Number(serde_json::Number::from_f64(3.14).unwrap()),
                ]),
                ..JsonSchema::default()
            },
        )]);
        let text = "value: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 6), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(ls.contains(&"42"), "should render integer 42, got: {ls:?}");
        assert!(ls.contains(&"null"), "should render null, got: {ls:?}");
        assert!(
            ls.iter().any(|l| l.starts_with("3.14") || *l == "3.14"),
            "should render float 3.14, got: {ls:?}"
        );
    }

    #[test]
    fn should_skip_array_and_object_enum_values() {
        let schema = object_schema(vec![(
            "value",
            JsonSchema {
                enum_values: Some(vec![
                    serde_json::json!("valid"),
                    serde_json::json!(["a", "b"]),
                    serde_json::json!({"k": "v"}),
                ]),
                ..JsonSchema::default()
            },
        )]);
        let text = "value: \n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 6), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"valid"),
            "string enum value should appear, got: {ls:?}"
        );
        assert_eq!(
            ls.len(),
            1,
            "array and object enum values should be skipped, got: {ls:?}"
        );
    }

    #[test]
    fn should_render_multiple_type_label_as_pipe_separated_string() {
        let schema = object_schema(vec![(
            "value",
            JsonSchema {
                schema_type: Some(SchemaType::Multiple(vec![
                    "string".to_string(),
                    "null".to_string(),
                ])),
                ..JsonSchema::default()
            },
        )]);
        let text = "name: x\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(0, 0), Some(&ParsedSchema::new(schema)));

        let item = result.iter().find(|i| i.label == "value");
        assert!(item.is_some(), "should suggest 'value'");
        assert_eq!(
            item.unwrap().detail.as_deref(),
            Some("string | null"),
            "multiple types should be joined with ' | '"
        );
    }

    #[test]
    fn should_suggest_schema_keys_on_blank_line_when_schema_is_present() {
        let schema = object_schema(vec![("host", string_schema()), ("port", integer_schema())]);
        let text = "host: localhost\n\n";
        let docs = parse_docs(text);
        let result = complete_at(&docs, pos(1, 0), Some(&ParsedSchema::new(schema)));

        let ls = labels(&result);
        assert!(
            ls.contains(&"port"),
            "should suggest 'port' on blank line with schema, got: {ls:?}"
        );
    }
}
