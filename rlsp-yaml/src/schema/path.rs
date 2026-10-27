// SPDX-License-Identifier: MIT

//! Lookup of the sub-schema at a path through a parsed schema.

use std::collections::HashSet;
use std::ptr;

use super::{JsonSchema, ParsedSchema};

/// One step of a path through a schema: a mapping key or an array item.
///
/// An array item is a segment of its own, so a mapping key spelled `[]` is
/// never taken for one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaPathSegment {
    /// The value of the mapping key with this name.
    Key(String),
    /// The element of an array at this zero-based index. The schema walk
    /// ignores the index: it is there for picking the schema by position.
    Item(usize),
}

/// The `allOf`, `anyOf` and `oneOf` branches of `schema`, in that order.
pub fn composition_branches(schema: &JsonSchema) -> impl DoubleEndedIterator<Item = &JsonSchema> {
    [&schema.all_of, &schema.any_of, &schema.one_of]
        .into_iter()
        .flatten()
        .flatten()
}

/// Walk the schema tree from `schema` following `path`, returning the
/// sub-schema at that path if it exists, resolved. Returns `None` when the path
/// exceeds the schema depth.
///
/// A key segment that no `properties` entry names, and an item segment on a
/// schema without `items`, is tried against every composition branch with the
/// same path, so a schema that references itself can come back to the same
/// schema with the same rest of the path. The walk goes through each shared
/// target once per rest of the path: a target already tried at that point
/// cannot succeed, or the walk would have ended. It keeps an explicit stack, so
/// the depth of a chain of references does not grow the call stack.
pub fn resolve_schema_path<'a>(
    parsed: &'a ParsedSchema,
    schema: &'a JsonSchema,
    path: &[SchemaPathSegment],
) -> Option<&'a JsonSchema> {
    let mut tried: HashSet<(*const JsonSchema, usize)> = HashSet::new();
    let mut stack = vec![(schema, path)];
    while let Some((node, path)) = stack.pop() {
        let schema = parsed.resolve(node);
        if node.reference.is_some() && !tried.insert((ptr::from_ref(schema), path.len())) {
            continue;
        }
        let [segment, rest @ ..] = path else {
            return Some(schema);
        };

        let key = match segment {
            // Array item descent: own `items` first, then the composition branches.
            SchemaPathSegment::Item(_) => {
                match schema.items.as_deref() {
                    Some(items) => stack.push((items, rest)),
                    None => stack.extend(composition_branches(schema).rev().map(|b| (b, path))),
                }
                continue;
            }
            SchemaPathSegment::Key(key) => key,
        };

        // Direct property lookup.
        if let Some(prop_schema) = schema.properties.as_ref().and_then(|p| p.get(key.as_str())) {
            stack.push((prop_schema, rest));
            continue;
        }

        // Walk composition branches; the first branch is tried first.
        stack.extend(composition_branches(schema).rev().map(|b| (b, path)));
    }
    None
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::schema::SchemaType;
    use crate::schema::test_support::{
        CHAIN_LENGTH, all_of, any_of, array_of, boolean_schema, chain, composes_itself, cyclic,
        declaring, field_branches, integer_schema, key, ladder, nest_in_all_of, no_targets,
        null_schema, object_schema, on_a_small_stack, one_of, reference_to, string_schema,
        with_targets,
    };

    // ══════════════════════════════════════════════════════════════════════════
    // Direct unit tests: resolve_schema_path
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn resolve_schema_path_empty_path_returns_schema() {
        let parsed = no_targets();
        let schema = string_schema();
        let result = resolve_schema_path(&parsed, &schema, &[]);
        assert!(result.is_some());
        assert!(std::ptr::eq(result.unwrap(), &raw const schema));
    }

    #[test]
    fn resolve_schema_path_single_key_found() {
        let name_schema = string_schema();
        let schema = object_schema(vec![("name", name_schema)]);
        let path = vec![key("name")];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_some(), "should find 'name' in schema");
    }

    #[test]
    fn resolve_schema_path_single_key_not_found() {
        let schema = object_schema(vec![("name", string_schema())]);
        let path = vec![key("missing")];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_none(), "missing key should return None");
    }

    #[test]
    fn resolve_schema_path_array_sentinel_descends() {
        let items_schema = object_schema(vec![("host", string_schema())]);
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            items: Some(Box::new(items_schema)),
            ..JsonSchema::default()
        };
        let path = vec![SchemaPathSegment::Item(0)];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(
            result.is_some(),
            "an item segment should descend into items"
        );
    }

    #[test]
    fn resolve_schema_path_array_sentinel_no_items_returns_none() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            ..JsonSchema::default()
        };
        let path = vec![SchemaPathSegment::Item(0)];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(
            result.is_none(),
            "an item segment on a schema without items should return None"
        );
    }

    #[rstest]
    #[case::key(vec![key("[]")], Some("integer"))]
    #[case::item(vec![SchemaPathSegment::Item(0)], Some("boolean"))]
    fn resolve_schema_path_tells_a_brackets_key_from_an_array_item(
        #[case] path: Vec<SchemaPathSegment>,
        #[case] expected: Option<&str>,
    ) {
        let schema = JsonSchema {
            items: Some(Box::new(boolean_schema())),
            ..object_schema(vec![("[]", integer_schema())])
        };

        assert_eq!(resolved_type_at(&schema, &path).as_deref(), expected);
    }

    #[test]
    fn resolve_schema_path_brackets_key_without_property_is_none() {
        let schema = array_of(string_schema());

        assert!(resolve_schema_path(&no_targets(), &schema, &[key("[]")]).is_none());
    }

    #[test]
    fn resolve_schema_path_item_without_items_is_none_beside_a_brackets_property() {
        let schema = object_schema(vec![("[]", string_schema())]);

        assert!(
            resolve_schema_path(&no_targets(), &schema, &[SchemaPathSegment::Item(0)]).is_none()
        );
    }

    #[test]
    fn resolve_schema_path_finds_brackets_key_in_composition_branch() {
        let schema = all_of(vec![object_schema(vec![("[]", integer_schema())])]);

        assert_eq!(
            resolved_type_at(&schema, &[key("[]")]).as_deref(),
            Some("integer")
        );
    }

    #[rstest]
    #[case::object_in_array_in_array(vec![SchemaPathSegment::Item(0), SchemaPathSegment::Item(0), key("b")], true)]
    #[case::one_item_too_many(vec![SchemaPathSegment::Item(0), SchemaPathSegment::Item(0), SchemaPathSegment::Item(0)], false)]
    fn resolve_schema_path_descends_through_nested_arrays(
        #[case] path: Vec<SchemaPathSegment>,
        #[case] found: bool,
    ) {
        let schema = array_of(array_of(object_schema(vec![("b", string_schema())])));

        assert_eq!(
            resolve_schema_path(&no_targets(), &schema, &path).is_some(),
            found
        );
    }

    #[test]
    fn resolve_schema_path_descends_into_items_of_a_property() {
        let schema = object_schema(vec![("tags", array_of(string_schema()))]);

        assert_eq!(
            resolved_type_at(&schema, &[key("tags"), SchemaPathSegment::Item(0)]).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_composition_branch_found() {
        let host_schema = string_schema();
        let schema = JsonSchema {
            any_of: Some(vec![object_schema(vec![("host", host_schema)])]),
            ..JsonSchema::default()
        };
        let path = vec![key("host")];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_some(), "should find 'host' in anyOf branch");
    }

    #[test]
    fn resolve_schema_path_deep_path_found() {
        let schema = object_schema(vec![("a", object_schema(vec![("b", string_schema())]))]);
        let path = vec![key("a"), key("b")];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_some(), "deep path a.b should be found");
    }

    #[test]
    fn resolve_schema_path_deep_path_not_found() {
        let schema = object_schema(vec![("a", object_schema(vec![("b", string_schema())]))]);
        let path = vec![key("a"), key("c")];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_none(), "deep path a.c should not be found");
    }

    /// The `type` of the schema `path` resolves to, to tell resolved schemas apart.
    fn resolved_type_at(schema: &JsonSchema, path: &[SchemaPathSegment]) -> Option<String> {
        match resolve_schema_path(&no_targets(), schema, path)?
            .schema_type
            .as_ref()?
        {
            SchemaType::Single(t) => Some(t.clone()),
            SchemaType::Multiple(_) => None,
        }
    }

    /// [`resolved_type_at`] for a path of keys.
    fn resolved_type(schema: &JsonSchema, path: &[&str]) -> Option<String> {
        let path: Vec<_> = path.iter().map(|name| key(name)).collect();
        resolved_type_at(schema, &path)
    }

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn resolve_schema_path_finds_key_in_branch_beyond_twenty(
        #[case] compose: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let mut branches = vec![JsonSchema::default(); 25];
        branches.push(object_schema(vec![("target", integer_schema())]));
        let schema = compose(branches);

        assert_eq!(
            resolved_type(&schema, &["target"]).as_deref(),
            Some("integer")
        );
    }

    #[test]
    fn resolve_schema_path_continues_into_a_late_branch_for_a_multi_segment_path() {
        let late = object_schema(vec![("a", object_schema(vec![("b", boolean_schema())]))]);
        let mut branches = vec![JsonSchema::default(); 25];
        branches.push(nest_in_all_of(late, 1));
        let schema = all_of(branches);

        assert_eq!(
            resolved_type(&schema, &["a", "b"]).as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn resolve_schema_path_continues_to_next_branch_when_earlier_branch_dead_ends() {
        let schema = all_of(vec![
            object_schema(vec![("a", object_schema(vec![("x", string_schema())]))]),
            object_schema(vec![("a", object_schema(vec![("b", boolean_schema())]))]),
        ]);

        assert_eq!(
            resolved_type(&schema, &["a", "b"]).as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn resolve_schema_path_prefers_direct_property_then_all_of_over_one_of() {
        let schema = JsonSchema {
            all_of: Some(vec![object_schema(vec![("k", integer_schema())])]),
            one_of: Some(vec![object_schema(vec![("k", boolean_schema())])]),
            ..object_schema(vec![("direct", string_schema()), ("k", string_schema())])
        };
        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some("string"));

        let without_direct = JsonSchema {
            properties: None,
            ..schema
        };
        assert_eq!(
            resolved_type(&without_direct, &["k"]).as_deref(),
            Some("integer")
        );
    }

    #[test]
    fn resolve_schema_path_is_none_for_a_missing_key_among_many_branches() {
        let schema = all_of(field_branches(30));

        assert!(resolve_schema_path(&no_targets(), &schema, &[key("missing")]).is_none());
    }

    #[test]
    fn resolve_schema_path_finds_key_in_a_very_deep_and_a_very_wide_schema() {
        let deep = nest_in_all_of(object_schema(vec![("leaf", string_schema())]), 500);
        let mut wide_branches = vec![JsonSchema::default(); 5_000];
        wide_branches.push(object_schema(vec![("leaf", string_schema())]));
        let wide = all_of(wide_branches);

        for schema in [&deep, &wide] {
            assert_eq!(resolved_type(schema, &["leaf"]).as_deref(), Some("string"));
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Shared targets and reference cycles
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::a_key_further_down(vec![key("next"), key("next"), key("a")], true)]
    #[case::the_schema_itself(vec![key("next"), key("next")], true)]
    #[case::a_missing_key(vec![key("missing")], false)]
    #[case::a_missing_key_further_down(vec![key("next"), key("next"), key("missing")], false)]
    #[case::items_where_there_are_none(vec![key("next"), SchemaPathSegment::Item(0)], false)]
    #[case::items_at_the_root(vec![SchemaPathSegment::Item(0)], false)]
    #[case::items_further_down(vec![key("next"), key("next"), SchemaPathSegment::Item(0)], false)]
    #[case::a_key_below_missing_items(vec![SchemaPathSegment::Item(0), key("a")], false)]
    #[case::a_brackets_key_where_there_are_none(vec![key("next"), key("[]")], false)]
    fn path_resolution_ends_on_a_cyclic_schema(
        #[case] path: Vec<SchemaPathSegment>,
        #[case] found: bool,
    ) {
        let schema = cyclic();

        let resolved = resolve_schema_path(&schema, &schema.root, &path);

        assert_eq!(resolved.is_some(), found);
    }

    #[test]
    fn path_resolution_ends_on_a_chain_of_ten_thousand_targets() {
        let schema = chain(CHAIN_LENGTH);

        let found = resolve_schema_path(&schema, &schema.root, &[key("leaf")]);
        let missing = resolve_schema_path(&schema, &schema.root, &[key("missing")]);

        assert!(found.is_some());
        assert!(missing.is_none());
    }

    #[test]
    fn path_resolution_visits_each_target_of_a_branching_graph_once() {
        let schema = ladder(64);

        assert!(resolve_schema_path(&schema, &schema.root, &[key("missing")]).is_none());
        assert!(resolve_schema_path(&schema, &schema.root, &[key("leaf")]).is_some());
    }

    #[test]
    fn path_resolution_ends_when_a_reference_points_past_the_target_table() {
        let schema = with_targets(
            all_of(vec![reference_to(9), reference_to(0)]),
            vec![declaring("a")],
        );

        assert!(resolve_schema_path(&schema, &schema.root, &[key("missing")]).is_none());
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

        let resolved = resolve_schema_path(&schema, &schema.root, &[key("port")]);

        assert_eq!(
            resolved.and_then(|s| s.description.as_deref()),
            Some("the target")
        );
    }

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn path_resolution_ends_on_a_schema_that_composes_itself_through_a_keyword(
        #[case] composing: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let schema = composes_itself(composing);
        let found = resolve_schema_path(&schema, &schema.root, &[key("here"), key("a")]);
        let missing = resolve_schema_path(&schema, &schema.root, &[key("here"), key("b")]);

        assert!(found.is_some());
        assert!(missing.is_none());
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Array items declared in composition branches
    // ══════════════════════════════════════════════════════════════════════════

    const ITEM: SchemaPathSegment = SchemaPathSegment::Item(0);

    #[rstest]
    #[case::all_of_first(all_of, false)]
    #[case::all_of_after_null(all_of, true)]
    #[case::any_of_first(any_of, false)]
    #[case::any_of_after_null(any_of, true)]
    #[case::one_of_first(one_of, false)]
    #[case::one_of_after_null(one_of, true)]
    fn resolve_schema_path_finds_items_in_a_composition_branch(
        #[case] compose: fn(Vec<JsonSchema>) -> JsonSchema,
        #[case] after_null_branch: bool,
    ) {
        let mut branches = vec![array_of(string_schema())];
        if after_null_branch {
            branches.insert(0, null_schema());
        }

        assert_eq!(
            resolved_type_at(&compose(branches), &[ITEM]).as_deref(),
            Some("string")
        );
    }

    #[rstest]
    #[case::all_of_wins(true, true, "integer")]
    #[case::any_of_over_one_of(false, true, "string")]
    #[case::one_of_last(false, false, "boolean")]
    fn resolve_schema_path_prefers_all_of_items_then_any_of_over_one_of(
        #[case] with_all_of: bool,
        #[case] with_any_of: bool,
        #[case] expected: &str,
    ) {
        let schema = JsonSchema {
            all_of: with_all_of.then(|| vec![array_of(integer_schema())]),
            any_of: with_any_of.then(|| vec![array_of(string_schema())]),
            one_of: Some(vec![array_of(boolean_schema())]),
            ..JsonSchema::default()
        };

        assert_eq!(
            resolved_type_at(&schema, &[ITEM]).as_deref(),
            Some(expected)
        );
    }

    #[test]
    fn resolve_schema_path_takes_items_depth_first_from_the_first_branch() {
        let schema = all_of(vec![
            any_of(vec![array_of(string_schema())]),
            array_of(integer_schema()),
        ]);

        assert_eq!(
            resolved_type_at(&schema, &[ITEM]).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_own_items_shadow_items_in_branches() {
        let schema = JsonSchema {
            items: Some(Box::new(boolean_schema())),
            all_of: Some(vec![array_of(integer_schema())]),
            ..JsonSchema::default()
        };

        assert_eq!(
            resolved_type_at(&schema, &[ITEM]).as_deref(),
            Some("boolean")
        );
    }

    #[rstest]
    #[case::key_in_own_items("x", Some("string"))]
    #[case::key_only_in_branch_items("b", None)]
    fn resolve_schema_path_does_not_fall_back_to_branch_items_when_the_path_dead_ends_below_own_items(
        #[case] name: &str,
        #[case] expected: Option<&str>,
    ) {
        let schema = JsonSchema {
            all_of: Some(vec![array_of(object_schema(vec![("b", boolean_schema())]))]),
            ..array_of(object_schema(vec![("x", string_schema())]))
        };

        assert_eq!(
            resolved_type_at(&schema, &[ITEM, key(name)]).as_deref(),
            expected
        );
    }

    #[test]
    fn resolve_schema_path_falls_back_to_the_next_branch_when_items_of_the_first_dead_end() {
        let schema = all_of(vec![
            array_of(object_schema(vec![("x", string_schema())])),
            array_of(object_schema(vec![("b", boolean_schema())])),
        ]);

        assert_eq!(
            resolved_type_at(&schema, &[ITEM, key("b")]).as_deref(),
            Some("boolean")
        );
    }

    #[rstest]
    #[case::key_below_item(
        any_of(vec![array_of(object_schema(vec![("host", string_schema())])), null_schema()]),
        vec![ITEM, key("host")],
    )]
    #[case::item_of_a_property(
        object_schema(vec![("tags", any_of(vec![array_of(string_schema()), null_schema()]))]),
        vec![key("tags"), ITEM],
    )]
    fn resolve_schema_path_continues_below_items_found_in_a_branch(
        #[case] schema: JsonSchema,
        #[case] path: Vec<SchemaPathSegment>,
    ) {
        assert_eq!(resolved_type_at(&schema, &path).as_deref(), Some("string"));
    }

    #[rstest]
    #[case::two_levels(vec![ITEM, ITEM], Some("string"))]
    #[case::one_level_too_many(vec![ITEM, ITEM, ITEM], None)]
    fn resolve_schema_path_finds_items_in_branches_at_every_array_level(
        #[case] path: Vec<SchemaPathSegment>,
        #[case] expected: Option<&str>,
    ) {
        let inner = any_of(vec![array_of(string_schema()), null_schema()]);
        let schema = any_of(vec![array_of(inner), null_schema()]);

        assert_eq!(resolved_type_at(&schema, &path).as_deref(), expected);
    }

    #[rstest]
    #[case::object_branch(all_of(vec![object_schema(vec![("a", string_schema())])]))]
    #[case::scalar_branches(any_of(vec![null_schema(), string_schema()]))]
    #[case::no_branches(all_of(vec![]))]
    fn resolve_schema_path_item_is_none_when_no_branch_has_items(#[case] schema: JsonSchema) {
        assert!(resolve_schema_path(&no_targets(), &schema, &[ITEM]).is_none());
    }

    #[rstest]
    #[case::brackets_key_does_not_take_branch_items(all_of(vec![array_of(string_schema())]), key("[]"))]
    #[case::item_does_not_take_a_branch_brackets_property(
        all_of(vec![object_schema(vec![("[]", string_schema())])]),
        ITEM,
    )]
    fn resolve_schema_path_a_brackets_key_never_means_branch_items(
        #[case] schema: JsonSchema,
        #[case] segment: SchemaPathSegment,
    ) {
        assert!(resolve_schema_path(&no_targets(), &schema, &[segment]).is_none());
    }

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn resolve_schema_path_finds_items_in_a_branch_beyond_twenty(
        #[case] compose: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let mut branches = vec![JsonSchema::default(); 25];
        branches.push(array_of(string_schema()));

        assert_eq!(
            resolved_type_at(&compose(branches), &[ITEM]).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_finds_items_in_a_very_deep_schema() {
        let schema = nest_in_all_of(array_of(string_schema()), 500);

        assert_eq!(
            resolved_type_at(&schema, &[ITEM]).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_finds_items_behind_a_reference_in_a_branch() {
        let described = JsonSchema {
            description: Some("the target".to_string()),
            ..string_schema()
        };
        let schema = with_targets(
            reference_to(0),
            vec![
                all_of(vec![reference_to(1)]),
                array_of(reference_to(2)),
                described,
            ],
        );

        let resolved = resolve_schema_path(&schema, &schema.root, &[ITEM]);

        assert_eq!(
            resolved.and_then(|s| s.description.as_deref()),
            Some("the target")
        );
    }

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn path_resolution_ends_on_a_schema_that_composes_itself_when_asked_for_items(
        #[case] composing: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let schema = composes_itself(composing);

        assert!(resolve_schema_path(&schema, &schema.root, &[key("here"), ITEM]).is_none());
        assert!(resolve_schema_path(&schema, &schema.root, &[ITEM]).is_none());
    }

    #[test]
    fn path_resolution_finds_items_after_skipping_a_self_reference() {
        let schema = with_targets(
            reference_to(0),
            vec![
                all_of(vec![reference_to(0), reference_to(1)]),
                array_of(string_schema()),
            ],
        );

        assert!(resolve_schema_path(&schema, &schema.root, &[ITEM]).is_some());
    }

    #[test]
    fn path_resolution_visits_each_target_of_a_branching_graph_once_for_items() {
        let schema = ladder(64);

        assert!(resolve_schema_path(&schema, &schema.root, &[ITEM]).is_none());
    }

    #[test]
    fn path_resolution_ends_on_a_chain_of_ten_thousand_targets_for_items() {
        on_a_small_stack(|| {
            let schema = chain(CHAIN_LENGTH);

            assert!(resolve_schema_path(&schema, &schema.root, &[ITEM]).is_none());
        });
    }

    // ══════════════════════════════════════════════════════════════════════════
    // The index of an item segment
    // ══════════════════════════════════════════════════════════════════════════

    #[rstest]
    #[case::first(0)]
    #[case::second(1)]
    #[case::far_out(7)]
    #[case::largest(usize::MAX)]
    fn resolve_schema_path_ignores_the_item_index(#[case] index: usize) {
        let schema = array_of(object_schema(vec![("b", string_schema())]));
        let parsed = no_targets();
        let at_first = resolve_schema_path(&parsed, &schema, &[ITEM, key("b")]);

        let at_index = resolve_schema_path(
            &parsed,
            &schema,
            &[SchemaPathSegment::Item(index), key("b")],
        );

        assert!(at_first.is_some_and(|first| at_index.is_some_and(|got| ptr::eq(first, got))));
    }

    #[test]
    fn resolve_schema_path_ignores_the_indices_of_nested_items() {
        let schema = array_of(array_of(object_schema(vec![("b", string_schema())])));
        let found = [
            SchemaPathSegment::Item(3),
            SchemaPathSegment::Item(9),
            key("b"),
        ];
        let one_too_many = [
            SchemaPathSegment::Item(1),
            SchemaPathSegment::Item(2),
            SchemaPathSegment::Item(3),
        ];

        assert_eq!(resolved_type_at(&schema, &found).as_deref(), Some("string"));
        assert!(resolved_type_at(&schema, &one_too_many).is_none());
    }

    #[test]
    fn resolve_schema_path_ignores_the_item_index_in_a_composition_branch() {
        let schema = any_of(vec![null_schema(), array_of(string_schema())]);
        let parsed = no_targets();
        let at_first = resolve_schema_path(&parsed, &schema, &[ITEM]);

        let at_fifth = resolve_schema_path(&parsed, &schema, &[SchemaPathSegment::Item(5)]);

        assert!(at_first.is_some_and(|first| at_fifth.is_some_and(|got| ptr::eq(first, got))));
    }

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn path_resolution_ends_on_a_schema_that_composes_itself_when_asked_for_different_items(
        #[case] composing: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let schema = composes_itself(composing);
        let path = [SchemaPathSegment::Item(1), SchemaPathSegment::Item(2)];

        assert!(resolve_schema_path(&schema, &schema.root, &path).is_none());
    }

    #[test]
    fn resolve_schema_path_an_indexed_item_never_means_a_brackets_property() {
        let schema = all_of(vec![object_schema(vec![("[]", string_schema())])]);

        assert!(resolved_type_at(&schema, &[SchemaPathSegment::Item(2)]).is_none());
    }
}
