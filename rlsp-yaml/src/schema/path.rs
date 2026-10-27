// SPDX-License-Identifier: MIT

//! Lookup of the sub-schema at a path through a parsed schema.

use std::collections::HashSet;
use std::ptr;

use super::{JsonSchema, ParsedSchema};

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
/// A segment that no `properties` entry names is tried against every
/// composition branch with the same path, so a schema that references itself
/// can come back to the same schema with the same rest of the path. The walk
/// goes through each shared target once per rest of the path: a target
/// already tried at that point cannot succeed, or the walk would have ended.
/// It keeps an explicit stack, so the depth of a chain of references does not
/// grow the call stack.
pub fn resolve_schema_path<'a>(
    parsed: &'a ParsedSchema,
    schema: &'a JsonSchema,
    path: &[String],
) -> Option<&'a JsonSchema> {
    let mut tried: HashSet<(*const JsonSchema, usize)> = HashSet::new();
    let mut stack = vec![(schema, path)];
    while let Some((node, path)) = stack.pop() {
        let schema = parsed.resolve(node);
        if node.reference.is_some() && !tried.insert((ptr::from_ref(schema), path.len())) {
            continue;
        }
        let [key, rest @ ..] = path else {
            return Some(schema);
        };

        // Array item descent.
        if key == "[]" {
            stack.extend(schema.items.as_deref().map(|items| (items, rest)));
            continue;
        }

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
        CHAIN_LENGTH, all_of, any_of, boolean_schema, chain, composes_itself, cyclic, declaring,
        field_branches, integer_schema, ladder, nest_in_all_of, no_targets, object_schema, one_of,
        reference_to, string_schema, with_targets,
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
        let path = vec!["name".to_string()];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_some(), "should find 'name' in schema");
    }

    #[test]
    fn resolve_schema_path_single_key_not_found() {
        let schema = object_schema(vec![("name", string_schema())]);
        let path = vec!["missing".to_string()];
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
        let path = vec!["[]".to_string()];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_some(), "[] sentinel should descend into items");
    }

    #[test]
    fn resolve_schema_path_array_sentinel_no_items_returns_none() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            ..JsonSchema::default()
        };
        let path = vec!["[]".to_string()];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(
            result.is_none(),
            "[] on schema without items should return None"
        );
    }

    #[test]
    fn resolve_schema_path_composition_branch_found() {
        let host_schema = string_schema();
        let schema = JsonSchema {
            any_of: Some(vec![object_schema(vec![("host", host_schema)])]),
            ..JsonSchema::default()
        };
        let path = vec!["host".to_string()];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_some(), "should find 'host' in anyOf branch");
    }

    #[test]
    fn resolve_schema_path_deep_path_found() {
        let schema = object_schema(vec![("a", object_schema(vec![("b", string_schema())]))]);
        let path = vec!["a".to_string(), "b".to_string()];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_some(), "deep path a.b should be found");
    }

    #[test]
    fn resolve_schema_path_deep_path_not_found() {
        let schema = object_schema(vec![("a", object_schema(vec![("b", string_schema())]))]);
        let path = vec!["a".to_string(), "c".to_string()];
        let parsed = no_targets();
        let result = resolve_schema_path(&parsed, &schema, &path);
        assert!(result.is_none(), "deep path a.c should not be found");
    }

    /// The `type` of the schema `path` resolves to, to tell resolved schemas apart.
    fn resolved_type(schema: &JsonSchema, path: &[&str]) -> Option<String> {
        let path: Vec<String> = path.iter().map(ToString::to_string).collect();
        match resolve_schema_path(&no_targets(), schema, &path)?
            .schema_type
            .as_ref()?
        {
            SchemaType::Single(t) => Some(t.clone()),
            SchemaType::Multiple(_) => None,
        }
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

        assert!(resolve_schema_path(&no_targets(), &schema, &["missing".to_string()]).is_none());
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
    fn path_resolution_ends_on_a_chain_of_ten_thousand_targets() {
        let schema = chain(CHAIN_LENGTH);

        let found = resolve_schema_path(&schema, &schema.root, &["leaf".to_string()]);
        let missing = resolve_schema_path(&schema, &schema.root, &["missing".to_string()]);

        assert!(found.is_some());
        assert!(missing.is_none());
    }

    #[test]
    fn path_resolution_visits_each_target_of_a_branching_graph_once() {
        let schema = ladder(64);

        assert!(resolve_schema_path(&schema, &schema.root, &["missing".to_string()]).is_none());
        assert!(resolve_schema_path(&schema, &schema.root, &["leaf".to_string()]).is_some());
    }

    #[test]
    fn path_resolution_ends_when_a_reference_points_past_the_target_table() {
        let schema = with_targets(
            all_of(vec![reference_to(9), reference_to(0)]),
            vec![declaring("a")],
        );

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

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn path_resolution_ends_on_a_schema_that_composes_itself_through_a_keyword(
        #[case] composing: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let schema = composes_itself(composing);
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
    }
}
