// SPDX-License-Identifier: MIT

//! Completion follows reference nodes wherever it reads a sub-schema, and
//! ends on schemas that share targets or reference themselves.

use std::collections::HashSet;

use rstest::rstest;
use tower_lsp::lsp_types::{CompletionItemKind, Position};

use super::schema_completions::{resolve_schema_path, schema_key_completions};
use super::support::test_fixtures::{integer_schema, labels, object_schema, pos, string_schema};
use super::*;
use crate::schema::test_support::{
    Sharing, assert_references_followed, on_a_small_stack, reference_to, with_targets,
};
use crate::schema::{JsonSchema, SchemaType};
use crate::test_utils::parse_docs;

/// An object schema whose properties exercise everything key completion reads
/// from a property schema: its type, description, `deprecated` flag, and the
/// type that picks the default of a required key's snippet.
fn server(t: &Sharing) -> JsonSchema {
    let port = JsonSchema {
        schema_type: Some(SchemaType::Single("integer".to_string())),
        description: Some("The port".to_string()),
        deprecated: Some(true),
        ..JsonSchema::default()
    };
    JsonSchema {
        required: Some(vec!["port".to_string(), "extra".to_string()]),
        ..object_schema(vec![
            ("host", string_schema()),
            ("port", t.put(port)),
            ("extra", t.put(string_schema())),
        ])
    }
}

fn enum_schema() -> JsonSchema {
    JsonSchema {
        schema_type: Some(SchemaType::Single("string".to_string())),
        enum_values: Some(vec![serde_json::json!("a"), serde_json::json!("b")]),
        ..JsonSchema::default()
    }
}

fn all_of(branches: Vec<JsonSchema>) -> JsonSchema {
    JsonSchema {
        all_of: Some(branches),
        ..JsonSchema::default()
    }
}

fn array_of(items: JsonSchema) -> JsonSchema {
    JsonSchema {
        schema_type: Some(SchemaType::Single("array".to_string())),
        items: Some(Box::new(items)),
        ..JsonSchema::default()
    }
}

/// Where a reference sits relative to the schema that offers the completions,
/// the document, and the cursor.
#[rstest]
#[case::root_key(|t: &Sharing| t.put(server(t)), "x: 1\n", pos(0, 0))]
#[case::property_schemas(server, "x: 1\n", pos(0, 0))]
#[case::all_of_branch(|t: &Sharing| all_of(vec![t.put(server(t))]), "x: 1\n", pos(0, 0))]
#[case::any_of_branch(
    |t: &Sharing| JsonSchema { any_of: Some(vec![t.put(server(t))]), ..JsonSchema::default() },
    "x: 1\n",
    pos(0, 0)
)]
#[case::one_of_branch(
    |t: &Sharing| JsonSchema { one_of: Some(vec![t.put(server(t))]), ..JsonSchema::default() },
    "x: 1\n",
    pos(0, 0)
)]
#[case::branch_of_a_branch(
    |t: &Sharing| {
        let inner = JsonSchema { any_of: Some(vec![t.put(server(t))]), ..JsonSchema::default() };
        all_of(vec![t.put(inner)])
    },
    "x: 1\n",
    pos(0, 0)
)]
#[case::property_on_the_path(
    |t: &Sharing| object_schema(vec![("server", t.put(server(t)))]),
    "server:\n  host: x\n  \n",
    pos(2, 2)
)]
#[case::property_on_the_path_behind_a_branch(
    |t: &Sharing| all_of(vec![t.put(object_schema(vec![("server", t.put(server(t)))]))]),
    "server:\n  host: x\n  \n",
    pos(2, 2)
)]
#[case::items_on_the_path(
    |t: &Sharing| object_schema(vec![("servers", array_of(t.put(server(t))))]),
    "servers:\n  - host: x\n  \n",
    pos(2, 2)
)]
#[case::items_behind_a_property_reference(
    |t: &Sharing| object_schema(vec![("servers", t.put(array_of(t.put(server(t)))))]),
    "servers:\n  - host: x\n  \n",
    pos(2, 2)
)]
#[case::value_of_a_property(
    |t: &Sharing| object_schema(vec![("env", t.put(enum_schema()))]),
    "env: \n",
    pos(0, 5)
)]
#[case::value_behind_a_root_reference(
    |t: &Sharing| t.put(object_schema(vec![("env", enum_schema())])),
    "env: \n",
    pos(0, 5)
)]
fn a_reference_at_a_position_completes_like_the_schema_written_inline(
    #[case] build: fn(&Sharing) -> JsonSchema,
    #[case] text: &str,
    #[case] cursor: Position,
) {
    let docs = parse_docs(text);

    // The order of the keys of one `properties` map is unspecified.
    assert_references_followed(build, |schema| {
        let mut items = complete_at(&docs, cursor, Some(schema));
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items
    });
}

/// The labels of the key items `schema` offers, sorted: the order of the keys
/// of one `properties` map is unspecified.
///
/// Goes around `complete_at` because that merges items of the same label.
fn key_labels(schema: &ParsedSchema) -> Vec<String> {
    let items = schema_key_completions(schema, &schema.root, &HashSet::new());
    let mut labels: Vec<String> = items
        .into_iter()
        .filter(|i| i.kind == Some(CompletionItemKind::FIELD))
        .map(|i| i.label)
        .collect();
    labels.sort();
    labels
}

fn declaring(key: &str) -> JsonSchema {
    object_schema(vec![(key, string_schema())])
}

// ══════════════════════════════════════════════════════════════════════════
// A shared target contributes its keys once
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn a_target_reached_through_two_composition_lists_contributes_its_keys_once() {
    let schema = with_targets(
        JsonSchema {
            all_of: Some(vec![reference_to(0)]),
            any_of: Some(vec![reference_to(0)]),
            one_of: Some(vec![reference_to(0)]),
            ..JsonSchema::default()
        },
        vec![declaring("a")],
    );

    assert_eq!(key_labels(&schema), ["a"]);
}

#[test]
fn a_target_reached_through_two_branches_of_different_targets_contributes_its_keys_once() {
    // root -> allOf -> 0 -> anyOf -> 2 and root -> allOf -> 1 -> oneOf -> 2
    let schema = with_targets(
        all_of(vec![reference_to(0), reference_to(1)]),
        vec![
            JsonSchema {
                any_of: Some(vec![reference_to(2)]),
                ..declaring("first")
            },
            JsonSchema {
                one_of: Some(vec![reference_to(2)]),
                ..declaring("second")
            },
            declaring("shared"),
        ],
    );

    assert_eq!(key_labels(&schema), ["first", "second", "shared"]);
}

#[test]
fn a_schema_that_references_itself_contributes_its_keys_once() {
    let target = JsonSchema {
        all_of: Some(vec![reference_to(0)]),
        any_of: Some(vec![reference_to(0), reference_to(0)]),
        ..declaring("a")
    };
    // The root reference and the self references lead to the same target.
    let schema = with_targets(reference_to(0), vec![target]);

    assert_eq!(key_labels(&schema), ["a"]);
}

#[test]
fn a_target_that_the_root_is_a_reference_to_is_not_entered_again() {
    let target = JsonSchema {
        all_of: Some(vec![reference_to(0)]),
        ..declaring("a")
    };
    let schema = with_targets(
        JsonSchema {
            all_of: Some(vec![reference_to(0)]),
            ..JsonSchema::default()
        },
        vec![target],
    );

    assert_eq!(key_labels(&schema), ["a"]);
}

#[test]
fn keys_from_two_distinct_branches_still_appear_twice() {
    let schema = with_targets(
        all_of(vec![reference_to(0), reference_to(1)]),
        vec![declaring("dup"), declaring("dup")],
    );

    assert_eq!(key_labels(&schema), ["dup", "dup"]);
}

// ══════════════════════════════════════════════════════════════════════════
// Termination on cyclic and deep reference graphs
// ══════════════════════════════════════════════════════════════════════════

/// A schema that offers `a` and `next`, where `next` is the schema again, and
/// that composes itself.
fn cyclic() -> ParsedSchema {
    let target = JsonSchema {
        all_of: Some(vec![reference_to(0)]),
        one_of: Some(vec![reference_to(0)]),
        ..object_schema(vec![("a", string_schema()), ("next", reference_to(0))])
    };
    with_targets(reference_to(0), vec![target])
}

#[rstest]
#[case::a_key_further_down(&["next", "next", "a"][..], true)]
#[case::the_schema_itself(&["next", "next"][..], true)]
#[case::a_missing_key(&["missing"][..], false)]
#[case::a_missing_key_further_down(&["next", "next", "missing"][..], false)]
#[case::items_where_there_are_none(&["next", "[]"][..], false)]
fn path_resolution_ends_on_a_cyclic_schema(#[case] path: &[&str], #[case] found: bool) {
    let schema = cyclic();
    let path: Vec<String> = path.iter().map(ToString::to_string).collect();

    let resolved = resolve_schema_path(&schema, &schema.root, &path);

    assert_eq!(resolved.is_some(), found);
}

#[test]
fn key_completion_ends_on_a_cyclic_schema() {
    assert_eq!(key_labels(&cyclic()), ["a", "next"]);
}

#[test]
fn key_completion_below_a_cyclic_property_ends() {
    let docs = parse_docs("next:\n  next:\n    a: x\n    \n");

    let items = complete_at(&docs, pos(3, 4), Some(&cyclic()));

    assert_eq!(labels(&items), ["next"]);
}

/// `length` targets where each composes the next, and the last one declares `leaf`.
fn chain(length: usize) -> ParsedSchema {
    let mut targets: Vec<JsonSchema> = (1..length)
        .map(|next| all_of(vec![reference_to(next)]))
        .collect();
    targets.push(declaring("leaf"));
    with_targets(reference_to(0), targets)
}

const CHAIN_LENGTH: usize = 10_000;

#[test]
fn key_completion_ends_on_a_chain_of_ten_thousand_targets() {
    assert_eq!(key_labels(&chain(CHAIN_LENGTH)), ["leaf"]);
}

#[test]
fn path_resolution_ends_on_a_chain_of_ten_thousand_targets() {
    let schema = chain(CHAIN_LENGTH);

    let found = resolve_schema_path(&schema, &schema.root, &["leaf".to_string()]);
    let missing = resolve_schema_path(&schema, &schema.root, &["missing".to_string()]);

    assert!(found.is_some());
    assert!(missing.is_none());
}

/// `layers` layers of two targets that compose both targets of the next layer,
/// ending in one target that declares `leaf`: `2^layers` paths, `2 * layers`
/// targets.
fn ladder(layers: usize) -> ParsedSchema {
    let mut targets = Vec::new();
    for layer in 0..layers {
        let next = 2 * layer + 2;
        let below = if layer + 1 == layers {
            vec![reference_to(next)]
        } else {
            vec![reference_to(next), reference_to(next + 1)]
        };
        targets.push(all_of(below.clone()));
        targets.push(all_of(below));
    }
    targets.push(declaring("leaf"));
    with_targets(all_of(vec![reference_to(0), reference_to(1)]), targets)
}

#[test]
fn walks_visit_each_target_of_a_branching_graph_once() {
    let schema = ladder(64);

    assert_eq!(key_labels(&schema), ["leaf"]);
    assert!(resolve_schema_path(&schema, &schema.root, &["missing".to_string()]).is_none());
    assert!(resolve_schema_path(&schema, &schema.root, &["leaf".to_string()]).is_some());
}

#[test]
fn walks_end_when_a_reference_points_past_the_target_table() {
    let schema = with_targets(
        all_of(vec![reference_to(9), reference_to(0)]),
        vec![declaring("a")],
    );

    assert_eq!(key_labels(&schema), ["a"]);
    assert!(resolve_schema_path(&schema, &schema.root, &["missing".to_string()]).is_none());
}

#[test]
fn a_resolved_path_is_the_target_not_the_reference() {
    let described = JsonSchema {
        description: Some("the target".to_string()),
        ..integer_schema()
    };
    let schema = with_targets(
        object_schema(vec![("port", reference_to(0))]),
        vec![described],
    );

    let resolved = resolve_schema_path(&schema, &schema.root, &["port".to_string()]);

    assert_eq!(
        resolved.and_then(|s| s.description.as_deref()),
        Some("the target")
    );
}

/// A chain of `length` targets that compose the next one; the last declares
/// `server`, which declares `leaf`.
fn chain_declaring_server_last(length: usize) -> ParsedSchema {
    let mut targets: Vec<JsonSchema> = (1..length)
        .map(|next| all_of(vec![reference_to(next)]))
        .collect();
    targets.push(object_schema(vec![("server", declaring("leaf"))]));
    with_targets(reference_to(0), targets)
}

#[test]
fn completion_below_a_property_ends_on_a_chain_of_ten_thousand_targets() {
    on_a_small_stack(|| {
        let schema = chain_declaring_server_last(CHAIN_LENGTH);
        let docs = parse_docs("server:\n  other: x\n  \n");

        let items = complete_at(&docs, pos(2, 2), Some(&schema));

        assert_eq!(labels(&items), ["leaf"]);
    });
}

#[test]
fn completion_below_a_missing_property_ends_on_a_chain_of_ten_thousand_targets() {
    on_a_small_stack(|| {
        let schema = chain_declaring_server_last(CHAIN_LENGTH);
        let docs = parse_docs("missing:\n  other: x\n  \n");

        let items = complete_at(&docs, pos(2, 2), Some(&schema));

        // The only key in sight is the one already present.
        assert!(items.is_empty(), "{:?}", labels(&items));
    });
}

#[test]
fn top_level_completion_ends_on_a_chain_of_ten_thousand_targets() {
    on_a_small_stack(|| {
        let schema = chain(CHAIN_LENGTH);
        let docs = parse_docs("x: 1\n");

        let items = complete_at(&docs, pos(0, 0), Some(&schema));

        assert!(labels(&items).contains(&"leaf"));
    });
}

#[rstest]
#[case::all_of(|b| JsonSchema { all_of: Some(b), ..JsonSchema::default() })]
#[case::any_of(|b| JsonSchema { any_of: Some(b), ..JsonSchema::default() })]
#[case::one_of(|b| JsonSchema { one_of: Some(b), ..JsonSchema::default() })]
fn a_schema_that_composes_itself_through_a_keyword_ends(
    #[case] composing: fn(Vec<JsonSchema>) -> JsonSchema,
) {
    let target = JsonSchema {
        properties: Some([("a".to_string(), string_schema())].into()),
        ..composing(vec![reference_to(0), reference_to(0)])
    };
    let schema = with_targets(
        object_schema(vec![("here", reference_to(0)), ("a", string_schema())]),
        vec![target],
    );
    let found = resolve_schema_path(
        &schema,
        &schema.root,
        &["here".to_string(), "a".to_string()],
    );
    let missing = resolve_schema_path(
        &schema,
        &schema.root,
        &["here".to_string(), "b".to_string()],
    );

    assert!(found.is_some());
    assert!(missing.is_none());
    assert_eq!(key_labels(&schema), ["a", "here"]);
}

#[test]
fn a_target_with_more_properties_than_the_item_limit_is_cut_at_the_limit() {
    let many = JsonSchema {
        all_of: Some(vec![reference_to(0)]),
        properties: Some(
            (0..2 * MAX_COMPLETION_ITEMS)
                .map(|i| (format!("key{i}"), string_schema()))
                .collect(),
        ),
        ..JsonSchema::default()
    };
    let schema = with_targets(reference_to(0), vec![many]);

    assert_eq!(key_labels(&schema).len(), MAX_COMPLETION_ITEMS);
}

#[test]
fn a_reference_without_a_target_offers_nothing() {
    let dangling = JsonSchema {
        reference: Some(crate::schema::SchemaRef::Unresolved("#/missing".into())),
        ..JsonSchema::default()
    };
    let schema = with_targets(
        all_of(vec![dangling, reference_to(5), declaring("a")]),
        vec![declaring("unused")],
    );

    assert_eq!(key_labels(&schema), ["a"]);
}

#[test]
fn a_reference_node_is_read_as_its_target_and_not_by_its_own_fields() {
    let node = JsonSchema {
        properties: Some([("own".to_string(), string_schema())].into()),
        ..reference_to(0)
    };
    let schema = with_targets(node, vec![declaring("target")]);

    assert_eq!(key_labels(&schema), ["target"]);
}

#[test]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "LSP snippet placeholders look like format arguments"
)]
fn the_required_snippet_of_a_reference_is_built_from_its_target() {
    let target = JsonSchema {
        required: Some(vec!["port".to_string(), "host".to_string()]),
        ..object_schema(vec![("port", reference_to(1)), ("host", string_schema())])
    };
    let schema = with_targets(reference_to(0), vec![target, integer_schema()]);

    let items = schema_key_completions(&schema, &schema.root, &HashSet::new());

    let snippet = items
        .iter()
        .find(|i| i.label == "(all required)")
        .and_then(|i| i.insert_text.as_deref());
    assert_eq!(snippet, Some("port: ${1:0}\nhost: ${2:\"\"}"));
}

#[test]
fn keys_are_offered_at_every_level_of_a_recursive_parsed_schema() {
    let schema = crate::schema::parse_schema(&serde_json::json!({
        "properties": { "name": { "type": "string" }, "child": { "$ref": "#" } }
    }))
    .expect("recursive schema parses");
    let docs = parse_docs("child:\n  child:\n    child:\n      other: 1\n");

    let items = complete_at(&docs, pos(3, 6), Some(&schema));

    let mut found = labels(&items);
    found.sort_unstable();
    assert_eq!(found, ["child", "name"], "once each, at the fourth level");
}
