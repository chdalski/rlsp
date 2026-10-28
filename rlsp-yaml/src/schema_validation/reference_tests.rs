// SPDX-License-Identifier: MIT

//! Validation follows reference nodes wherever it reads a sub-schema.

use rstest::rstest;

use super::support::test_fixtures::integer_schema;
use super::*;
use crate::schema::test_support::{on_a_small_stack, reference_to, with_targets};
use crate::schema::{AdditionalProperties, SchemaType};
use crate::test_utils::parse_docs;

fn diagnostics_of(schema: &ParsedSchema, text: &str) -> Vec<Diagnostic> {
    validate_schema(&parse_docs(text), schema, true, YamlVersion::V1_2).diagnostics
}

/// The diagnostics and the number of steps a run against `schema` spends.
fn outcome_of(schema: &ParsedSchema, text: &str) -> (Vec<Diagnostic>, usize) {
    let budget = Budget::new(usize::MAX);
    let result = validate_documents(&parse_docs(text), schema, true, YamlVersion::V1_2, &budget);
    (result.diagnostics, budget.spent())
}

fn object() -> SchemaType {
    SchemaType::Single("object".to_string())
}

/// The parent schemas that read a sub-schema, each with the document that
/// reaches it, and what `diagnostics_of` must make of it.
///
/// Every case puts `{type: integer}` (or, for `if`, a schema that picks the
/// `else` branch) at one position of `JsonSchema`. Adding a child-schema field
/// to `JsonSchema` fails `every_child_schema_position_has_a_case` until the
/// field has a case here.
#[rstest]
#[case::properties(
    |c| JsonSchema { properties: Some([("a".to_string(), c)].into()), ..JsonSchema::default() },
    "a: x"
)]
#[case::pattern_properties(
    |c| JsonSchema { pattern_properties: Some(vec![("^a".to_string(), c)]), ..JsonSchema::default() },
    "a: x"
)]
#[case::additional_properties(
    |c| JsonSchema {
        additional_properties: Some(AdditionalProperties::Schema(Box::new(c))),
        ..JsonSchema::default()
    },
    "a: x"
)]
#[case::property_names(
    |c| JsonSchema { property_names: Some(Box::new(c)), ..JsonSchema::default() },
    "a: 1"
)]
#[case::dependent_schemas(
    |c| JsonSchema { dependent_schemas: Some([("a".to_string(), c)].into()), ..JsonSchema::default() },
    "a: 1"
)]
#[case::unevaluated_properties(
    |c| JsonSchema {
        unevaluated_properties: Some(AdditionalProperties::Schema(Box::new(c))),
        ..JsonSchema::default()
    },
    "a: x"
)]
#[case::items(
    |c| JsonSchema { items: Some(Box::new(c)), ..JsonSchema::default() },
    "- x"
)]
#[case::prefix_items(
    |c| JsonSchema { prefix_items: Some(vec![c]), ..JsonSchema::default() },
    "- x"
)]
#[case::additional_items(
    |c| JsonSchema {
        prefix_items: Some(vec![JsonSchema::default()]),
        additional_items: Some(AdditionalProperties::Schema(Box::new(c))),
        ..JsonSchema::default()
    },
    "- 1\n- x"
)]
#[case::contains(
    |c| JsonSchema { contains: Some(Box::new(c)), ..JsonSchema::default() },
    "- x"
)]
#[case::unevaluated_items(
    |c| JsonSchema { unevaluated_items: Some(Box::new(c)), ..JsonSchema::default() },
    "- x"
)]
#[case::all_of(
    |c| JsonSchema { all_of: Some(vec![c]), ..JsonSchema::default() },
    "x"
)]
#[case::any_of(
    |c| JsonSchema { any_of: Some(vec![c]), ..JsonSchema::default() },
    "x"
)]
#[case::one_of(
    |c| JsonSchema { one_of: Some(vec![c]), ..JsonSchema::default() },
    "x"
)]
#[case::not(
    |c| JsonSchema { not: Some(Box::new(c)), ..JsonSchema::default() },
    "x"
)]
#[case::if_schema(
    |c| JsonSchema {
        if_schema: Some(Box::new(c)),
        else_schema: Some(Box::new(JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            ..JsonSchema::default()
        })),
        ..JsonSchema::default()
    },
    "x"
)]
#[case::then_schema(
    |c| JsonSchema {
        if_schema: Some(Box::new(JsonSchema::default())),
        then_schema: Some(Box::new(c)),
        ..JsonSchema::default()
    },
    "x"
)]
#[case::else_schema(
    |c| JsonSchema {
        if_schema: Some(Box::new(integer_schema())),
        else_schema: Some(Box::new(c)),
        ..JsonSchema::default()
    },
    "x"
)]
#[case::content_schema(
    |c| JsonSchema { content_schema: Some(Box::new(c)), ..JsonSchema::default() },
    "'a: 1'"
)]
#[case::root(|c| c, "x")]
fn a_reference_at_a_position_validates_like_the_schema_written_inline(
    #[case] at_position: fn(JsonSchema) -> JsonSchema,
    #[case] text: &str,
) {
    let inline = ParsedSchema::new(at_position(integer_schema()));
    let shared = with_targets(at_position(reference_to(0)), vec![integer_schema()]);
    // The reference node alone reads as an empty schema.
    let dangling = ParsedSchema::new(at_position(reference_to(0)));

    // Following a reference spends no step, so the step counts match as well.
    let expected = outcome_of(&inline, text);
    assert_eq!(
        outcome_of(&shared, text),
        expected,
        "the reference is not followed"
    );
    assert_ne!(
        outcome_of(&dangling, text).0,
        expected.0,
        "the case does not depend on the position"
    );
}

#[test]
fn every_child_schema_position_has_a_case() {
    // Exhaustive on purpose: a new field of `JsonSchema` must be classified
    // here, and a new child-schema field needs a case above.
    #[expect(
        clippy::unneeded_field_pattern,
        reason = "listing every field is the point: a new field fails to compile here"
    )]
    let JsonSchema {
        id: _,
        schema_type: _,
        title: _,
        description: _,
        format: _,
        content_encoding: _,
        content_media_type: _,
        content_schema: _, // content_schema
        properties: _,     // properties
        required: _,
        enum_values: _,
        default: _,
        examples: _,
        items: _,        // items
        prefix_items: _, // prefix_items
        contains: _,     // contains
        min_items: _,
        max_items: _,
        max_contains: _,
        min_contains: _,
        unique_items: _,
        additional_properties: _, // additional_properties
        additional_items: _,      // additional_items
        min_properties: _,
        max_properties: _,
        pattern_properties: _, // pattern_properties
        property_names: _,     // property_names
        all_of: _,             // all_of
        any_of: _,             // any_of
        one_of: _,             // one_of
        not: _,                // not
        if_schema: _,          // if_schema
        then_schema: _,        // then_schema
        else_schema: _,        // else_schema
        reference: _,
        anchor: _,
        dynamic_anchor: _,
        pattern: _,
        minimum: _,
        maximum: _,
        min_length: _,
        max_length: _,
        exclusive_minimum: _,
        exclusive_maximum: _,
        exclusive_minimum_draft04: _,
        exclusive_maximum_draft04: _,
        multiple_of: _,
        const_value: _,
        dependent_required: _,
        dependent_schemas: _, // dependent_schemas
        deprecated: _,
        unevaluated_properties: _, // unevaluated_properties
        unevaluated_items: _,      // unevaluated_items
    } = JsonSchema::default();
}

#[test]
fn a_reference_is_followed_at_every_nesting_level() {
    // root -> properties.a -> properties.b, each through its own target.
    let b = JsonSchema {
        properties: Some([("b".to_string(), reference_to(1))].into()),
        ..JsonSchema::default()
    };
    let schema = with_targets(
        JsonSchema {
            properties: Some([("a".to_string(), reference_to(0))].into()),
            ..JsonSchema::default()
        },
        vec![b, integer_schema()],
    );

    let diagnostics = diagnostics_of(&schema, "a:\n  b: x");

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("a.b"), "{diagnostics:?}");
}

#[test]
fn a_target_shared_by_two_positions_validates_each_use() {
    let schema = with_targets(
        JsonSchema {
            properties: Some(
                [
                    ("a".to_string(), reference_to(0)),
                    ("b".to_string(), reference_to(0)),
                ]
                .into(),
            ),
            ..JsonSchema::default()
        },
        vec![integer_schema()],
    );

    assert_eq!(diagnostics_of(&schema, "a: x\nb: y").len(), 2);
}

#[test]
fn an_unresolved_reference_validates_permissively() {
    let node = JsonSchema {
        reference: Some(crate::schema::SchemaRef::Unresolved("#/missing".into())),
        ..JsonSchema::default()
    };
    let schema = ParsedSchema::new(JsonSchema {
        properties: Some([("a".to_string(), node)].into()),
        ..JsonSchema::default()
    });

    assert!(diagnostics_of(&schema, "a: anything").is_empty());
}

#[test]
fn a_reference_to_a_missing_target_validates_permissively() {
    let schema = with_targets(
        JsonSchema {
            properties: Some([("a".to_string(), reference_to(7))].into()),
            ..JsonSchema::default()
        },
        vec![integer_schema()],
    );

    assert!(diagnostics_of(&schema, "a: anything").is_empty());
}

#[test]
fn unevaluated_properties_sees_properties_declared_behind_a_reference() {
    let declares_a = JsonSchema {
        properties: Some([("a".to_string(), JsonSchema::default())].into()),
        ..JsonSchema::default()
    };
    let schema = with_targets(
        JsonSchema {
            all_of: Some(vec![reference_to(0)]),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        },
        vec![declares_a],
    );

    let diagnostics = diagnostics_of(&schema, "a: 1\nb: 2");

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("'b'"), "{diagnostics:?}");
}

#[test]
fn unevaluated_items_sees_items_declared_behind_a_reference() {
    let evaluates_first = JsonSchema {
        prefix_items: Some(vec![JsonSchema::default()]),
        ..JsonSchema::default()
    };
    let schema = with_targets(
        JsonSchema {
            all_of: Some(vec![reference_to(0)]),
            unevaluated_items: Some(Box::new(integer_schema())),
            ..JsonSchema::default()
        },
        vec![evaluates_first],
    );

    // The first item is evaluated by the prefix; only the second must be an integer.
    let diagnostics = diagnostics_of(&schema, "- x\n- y");

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn unevaluated_items_ends_the_scan_at_an_items_keyword_behind_a_reference() {
    let evaluates_all = JsonSchema {
        items: Some(Box::new(JsonSchema::default())),
        ..JsonSchema::default()
    };
    let schema = with_targets(
        JsonSchema {
            all_of: Some(vec![reference_to(0)]),
            unevaluated_items: Some(Box::new(integer_schema())),
            ..JsonSchema::default()
        },
        vec![evaluates_all],
    );

    assert!(diagnostics_of(&schema, "- x\n- y").is_empty());
}

// ══════════════════════════════════════════════════════════════════════════
// Cyclic schemas and the cost of following a reference
// ══════════════════════════════════════════════════════════════════════════

/// A linked-list schema: `next` refers to the schema itself, `value` is an integer.
fn linked_list() -> ParsedSchema {
    let node = JsonSchema {
        schema_type: Some(object()),
        properties: Some(
            [
                ("next".to_string(), reference_to(0)),
                ("value".to_string(), integer_schema()),
            ]
            .into(),
        ),
        ..JsonSchema::default()
    };
    with_targets(reference_to(0), vec![node])
}

#[test]
fn a_recursive_schema_validates_every_level_of_the_document() {
    let diagnostics = diagnostics_of(
        &linked_list(),
        "value: 1\nnext:\n  value: 2\n  next:\n    value: x\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("next.next.value"),
        "{diagnostics:?}"
    );
}

#[test]
fn a_recursive_schema_ends_on_a_document_nested_past_the_depth_limit() {
    let levels = support::MAX_VALIDATION_DEPTH * 2;
    let mut text = String::new();
    for level in 0..levels {
        text.push_str(&"  ".repeat(level));
        text.push_str("next:\n");
    }
    text.push_str(&"  ".repeat(levels));
    text.push_str("value: 1\n");

    let result = validate_schema(&parse_docs(&text), &linked_list(), true, YamlVersion::V1_2);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[rstest]
#[case::one_branch(1)]
#[case::two_branches(2)]
fn a_schema_that_composes_itself_ends_within_the_step_budget(#[case] branches: usize) {
    let target = JsonSchema {
        all_of: Some(vec![reference_to(0); branches]),
        ..JsonSchema::default()
    };
    let schema = with_targets(reference_to(0), vec![target]);
    let docs = parse_docs("a: 1");
    let budget = Budget::new(step_budget(&docs));

    let result = validate_documents(&docs, &schema, true, YamlVersion::V1_2, &budget);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    // One branch stops at the depth limit; two branches have 2^64 paths to it
    // and stop at the budget.
    assert_eq!(
        result.completeness,
        if branches == 1 {
            Completeness::Complete
        } else {
            Completeness::StoppedEarly
        }
    );
}

#[test]
fn following_a_reference_spends_no_step() {
    let inline = ParsedSchema::new(JsonSchema {
        properties: Some(
            [(
                "a".to_string(),
                JsonSchema {
                    schema_type: Some(object()),
                    properties: Some([("b".to_string(), integer_schema())].into()),
                    ..JsonSchema::default()
                },
            )]
            .into(),
        ),
        ..JsonSchema::default()
    });
    let shared = with_targets(
        JsonSchema {
            properties: Some([("a".to_string(), reference_to(0))].into()),
            ..JsonSchema::default()
        },
        vec![
            JsonSchema {
                schema_type: Some(object()),
                properties: Some([("b".to_string(), reference_to(1))].into()),
                ..JsonSchema::default()
            },
            integer_schema(),
        ],
    );
    let docs = parse_docs("a:\n  b: 1");

    let spent = |schema: &ParsedSchema| {
        let budget = Budget::new(usize::MAX);
        let _ = validate_documents(&docs, schema, true, YamlVersion::V1_2, &budget);
        budget.spent()
    };

    assert_eq!(spent(&shared), spent(&inline));
}

#[test]
fn the_evaluated_properties_scan_through_a_reference_spends_the_steps_of_the_inline_scan() {
    let declares_a = JsonSchema {
        properties: Some([("a".to_string(), JsonSchema::default())].into()),
        ..JsonSchema::default()
    };
    let inline = ParsedSchema::new(JsonSchema {
        all_of: Some(vec![declares_a.clone()]),
        unevaluated_properties: Some(AdditionalProperties::Denied),
        ..JsonSchema::default()
    });
    let shared = with_targets(
        JsonSchema {
            all_of: Some(vec![reference_to(0)]),
            unevaluated_properties: Some(AdditionalProperties::Denied),
            ..JsonSchema::default()
        },
        vec![declares_a],
    );
    let docs = parse_docs("a: 1\nb: 2");

    let spent = |schema: &ParsedSchema| {
        let budget = Budget::new(usize::MAX);
        let _ = validate_documents(&docs, schema, true, YamlVersion::V1_2, &budget);
        budget.spent()
    };

    assert_eq!(spent(&shared), spent(&inline));
}

/// A document that nests `levels` mappings (`mapping`) or sequences, so a
/// schema that descends into it through a cycle has something to follow.
fn nested_document(mapping: bool, levels: usize) -> String {
    if !mapping {
        return format!("{}x\n", "- ".repeat(levels));
    }
    let mut text = String::new();
    for level in 0..levels {
        text.push_str(&"  ".repeat(level));
        text.push_str("a:\n");
    }
    text.push_str(&"  ".repeat(levels));
    text.push_str("b: 1\n");
    text
}

/// `T0 = {<keyword>: ref T0}` for each keyword that applies a sub-schema.
#[rstest]
#[case::all_of(|c| JsonSchema { all_of: Some(vec![c]), ..JsonSchema::default() }, true)]
#[case::any_of(|c| JsonSchema { any_of: Some(vec![c]), ..JsonSchema::default() }, true)]
#[case::one_of(|c| JsonSchema { one_of: Some(vec![c]), ..JsonSchema::default() }, true)]
#[case::not(|c| JsonSchema { not: Some(Box::new(c)), ..JsonSchema::default() }, true)]
#[case::if_schema(|c| JsonSchema { if_schema: Some(Box::new(c)), ..JsonSchema::default() }, true)]
#[case::then_schema(
    |c| JsonSchema {
        if_schema: Some(Box::new(JsonSchema::default())),
        then_schema: Some(Box::new(c)),
        ..JsonSchema::default()
    },
    true
)]
#[case::else_schema(
    |c| JsonSchema {
        if_schema: Some(Box::new(integer_schema())),
        else_schema: Some(Box::new(c)),
        ..JsonSchema::default()
    },
    true
)]
#[case::properties(
    |c| JsonSchema { properties: Some([("a".to_string(), c)].into()), ..JsonSchema::default() },
    true
)]
#[case::additional_properties(
    |c| JsonSchema {
        additional_properties: Some(AdditionalProperties::Schema(Box::new(c))),
        ..JsonSchema::default()
    },
    true
)]
#[case::pattern_properties(
    |c| JsonSchema { pattern_properties: Some(vec![(".*".to_string(), c)]), ..JsonSchema::default() },
    true
)]
#[case::unevaluated_properties(
    |c| JsonSchema {
        unevaluated_properties: Some(AdditionalProperties::Schema(Box::new(c))),
        ..JsonSchema::default()
    },
    true
)]
#[case::dependent_schemas(
    |c| JsonSchema { dependent_schemas: Some([("a".to_string(), c)].into()), ..JsonSchema::default() },
    true
)]
#[case::items(|c| JsonSchema { items: Some(Box::new(c)), ..JsonSchema::default() }, false)]
#[case::prefix_items(|c| JsonSchema { prefix_items: Some(vec![c]), ..JsonSchema::default() }, false)]
#[case::contains(|c| JsonSchema { contains: Some(Box::new(c)), ..JsonSchema::default() }, false)]
#[case::unevaluated_items(
    |c| JsonSchema { unevaluated_items: Some(Box::new(c)), ..JsonSchema::default() },
    false
)]
fn a_schema_that_applies_itself_through_a_keyword_ends(
    #[case] applying_itself: fn(JsonSchema) -> JsonSchema,
    #[case] mapping_document: bool,
) {
    let schema = with_targets(reference_to(0), vec![applying_itself(reference_to(0))]);
    let text = nested_document(mapping_document, support::MAX_VALIDATION_DEPTH * 2);

    let result = validate_schema(&parse_docs(&text), &schema, true, YamlVersion::V1_2);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn the_evaluated_properties_scan_over_a_schema_that_composes_itself_ends() {
    let target = JsonSchema {
        all_of: Some(vec![reference_to(0)]),
        unevaluated_properties: Some(AdditionalProperties::Denied),
        ..JsonSchema::default()
    };
    let schema = with_targets(reference_to(0), vec![target]);

    let result = validate_schema(&parse_docs("a: 1"), &schema, true, YamlVersion::V1_2);

    // The scan never learns whether `a` is evaluated, so it reports nothing.
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn the_evaluated_item_scan_over_a_schema_that_composes_itself_ends() {
    let target = JsonSchema {
        all_of: Some(vec![reference_to(0)]),
        unevaluated_items: Some(Box::new(integer_schema())),
        ..JsonSchema::default()
    };
    let schema = with_targets(reference_to(0), vec![target]);

    let result = validate_schema(&parse_docs("- x"), &schema, true, YamlVersion::V1_2);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn a_chain_of_references_ends_at_the_depth_limit() {
    let length = support::MAX_VALIDATION_DEPTH * 3;
    let mut targets: Vec<JsonSchema> = (1..length)
        .map(|next| JsonSchema {
            all_of: Some(vec![reference_to(next)]),
            ..JsonSchema::default()
        })
        .collect();
    targets.push(integer_schema());
    let schema = with_targets(reference_to(0), targets);

    let result = validate_schema(&parse_docs("x"), &schema, true, YamlVersion::V1_2);

    // The integer at the end of the chain is out of reach, and the cut is not a pass.
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn a_reference_node_is_read_as_its_target_and_not_by_its_own_fields() {
    let node = JsonSchema {
        schema_type: Some(SchemaType::Single("boolean".to_string())),
        ..reference_to(0)
    };
    let schema = with_targets(
        JsonSchema {
            properties: Some([("a".to_string(), node)].into()),
            ..JsonSchema::default()
        },
        vec![integer_schema()],
    );

    // `x` is an integer-schema violation and not a boolean-schema one.
    let diagnostics = diagnostics_of(&schema, "a: x");

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("integer"),
        "{diagnostics:?}"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Cycles through contentSchema: every embedded document restarts the depth
// ══════════════════════════════════════════════════════════════════════════

fn content_schema_of(schema: JsonSchema) -> JsonSchema {
    JsonSchema {
        content_schema: Some(Box::new(schema)),
        ..JsonSchema::default()
    }
}

/// The result of validating the string `a`, which parses to the string `a`
/// again, against `schema`.
fn validate_self_embedding_string(schema: &ParsedSchema) -> Validation {
    validate_schema(&parse_docs("\"a\""), schema, true, YamlVersion::V1_2)
}

#[test]
fn a_schema_that_embeds_itself_through_content_schema_ends() {
    on_a_small_stack(|| {
        let schema = with_targets(reference_to(0), vec![content_schema_of(reference_to(0))]);

        let result = validate_self_embedding_string(&schema);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.completeness, Completeness::StoppedEarly);
    });
}

#[test]
fn two_schemas_that_embed_each_other_through_content_schema_end() {
    on_a_small_stack(|| {
        let schema = with_targets(
            reference_to(0),
            vec![
                content_schema_of(reference_to(1)),
                content_schema_of(reference_to(0)),
            ],
        );

        let result = validate_self_embedding_string(&schema);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    });
}

#[rstest]
#[case::not(|c| JsonSchema { not: Some(Box::new(c)), ..JsonSchema::default() })]
#[case::if_schema(|c| JsonSchema { if_schema: Some(Box::new(c)), ..JsonSchema::default() })]
#[case::any_of(|c| JsonSchema { any_of: Some(vec![c]), ..JsonSchema::default() })]
fn a_cycle_through_content_schema_and_a_probe_ends(#[case] probing: fn(JsonSchema) -> JsonSchema) {
    // The probe builds a context of its own: it must carry the embedding depth over.
    on_a_small_stack(move || {
        let schema = with_targets(
            reference_to(0),
            vec![probing(content_schema_of(reference_to(0)))],
        );

        let result = validate_self_embedding_string(&schema);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.completeness, Completeness::StoppedEarly);
    });
}

#[test]
fn the_cut_of_an_embedded_document_is_not_read_as_a_pass() {
    on_a_small_stack(|| {
        // `not` of a schema nobody can decide must not report the string as matching it.
        let schema = with_targets(
            JsonSchema {
                not: Some(Box::new(reference_to(0))),
                ..JsonSchema::default()
            },
            vec![content_schema_of(reference_to(0))],
        );

        let result = validate_self_embedding_string(&schema);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    });
}

#[test]
fn embedded_documents_nest_below_the_content_limit() {
    // 'x' is the YAML text `"abc"`, which parses to the string `abc`, whose
    // text parses again to the string `abc`: two levels of embedding.
    let schema = ParsedSchema::new(content_schema_of(content_schema_of(JsonSchema {
        schema_type: Some(SchemaType::Single("integer".to_string())),
        ..JsonSchema::default()
    })));

    let result = validate_schema(&parse_docs("'\"abc\"'"), &schema, true, YamlVersion::V1_2);

    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(
        result.diagnostics[0].message.contains("integer"),
        "{:?}",
        result.diagnostics
    );
}

/// `levels` nested `contentSchema` keywords around an integer schema. The
/// string `a` parses to the string `a`, so it reaches the integer schema at
/// any nesting.
fn content_schema_nested(levels: usize) -> ParsedSchema {
    let leaf = JsonSchema {
        schema_type: Some(SchemaType::Single("integer".to_string())),
        ..JsonSchema::default()
    };
    ParsedSchema::new((0..levels).fold(leaf, |inner, _| content_schema_of(inner)))
}

#[rstest]
#[case::at_the_limit(support::MAX_CONTENT_DEPTH, 1, Completeness::Complete)]
#[case::past_the_limit(support::MAX_CONTENT_DEPTH + 1, 0, Completeness::StoppedEarly)]
fn embedded_documents_nest_up_to_the_content_limit(
    #[case] levels: usize,
    #[case] diagnostics: usize,
    #[case] completeness: Completeness,
) {
    let result = validate_self_embedding_string(&content_schema_nested(levels));

    assert_eq!(
        result.diagnostics.len(),
        diagnostics,
        "{:?}",
        result.diagnostics
    );
    assert_eq!(result.completeness, completeness);
}

// ── Schemas the parser builds: recursion is followed at any depth ───────────

/// A document that nests `next` mappings `depth` levels and ends in `leaf`.
fn nested_next_document(depth: usize, leaf_value: &str) -> String {
    let parents = (0..depth).map(|level| "  ".repeat(level) + "next:");
    let leaf = "  ".repeat(depth) + "leaf: " + leaf_value;
    parents.chain([leaf]).collect::<Vec<_>>().join("\n") + "\n"
}

fn recursive_schema() -> ParsedSchema {
    crate::schema::parse_schema(&serde_json::json!({
        "properties": { "next": { "$ref": "#" }, "leaf": { "type": "integer" } }
    }))
    .expect("recursive schema parses")
}

// The schema used to be cut at 32 levels, so a leaf nested deeper was not
// checked at all.
#[rstest]
#[case::shallow(5)]
#[case::just_inside_the_old_cut(31)]
#[case::beyond_the_old_cut(33)]
#[case::well_beyond_the_old_cut(40)]
fn recursive_schema_flags_a_leaf_at_any_nesting_depth(#[case] depth: usize) {
    let diagnostics = diagnostics_of(&recursive_schema(), &nested_next_document(depth, "x"));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].range.start.line as usize, depth);
}

#[test]
fn recursive_schema_accepts_a_valid_deep_document() {
    let diagnostics = diagnostics_of(&recursive_schema(), &nested_next_document(40, "1"));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn recursive_schema_ends_on_a_document_past_the_validation_depth_limit() {
    // The parser's loader recurses once per nesting level, so loading the
    // fixture needs more than the small stack; only the validation is pinned.
    let docs = parse_docs(&nested_next_document(100, "x"));

    on_a_small_stack(move || {
        let _ = validate_schema(&docs, &recursive_schema(), true, YamlVersion::V1_2);
    });
}
