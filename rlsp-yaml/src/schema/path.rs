// SPDX-License-Identifier: MIT

//! Lookup of the sub-schema at a path through a parsed schema.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::ptr;

use super::pattern::{Compiled, MAX_PATTERN_LEN, WORK_PER_STEP, lookup_or_compile};
use super::{AdditionalProperties, JsonSchema, ParsedSchema};

/// Steps the second pass of [`resolve_schema_path`] may spend on one call.
///
/// A step is one sub-schema queued for the walk, `WORK_PER_STEP` bytes of a key
/// at each node the walk visits for it, one over-long pattern skipped, or one
/// use of a pattern at the price validation charges (see `COMPILE_TIERS` in
/// `pattern.rs`), with the first use of each pattern in a call also paying for
/// its compilation. The first pass spends nothing.
///
/// Patterns and the schema come from a remote URL, and hover and completion
/// resolve a path on every request, so the work a schema can cause must not
/// depend on the schema. The size is the work worth about 18 ms: a step takes
/// about 183 ns, and the prices are about twice the measured cost. It pays for
/// one compile that ends under the last size tier (43.5k steps for a pattern of
/// the maximum length, which is the price of all three attempts) and matching a
/// key of 100 bytes against it (51.2k steps), leaving about 5k for the walk, or
/// for 39 patterns of the maximum length under the first tier (2.6k steps
/// each). The patterns of real schemas
/// (`^[_a-zA-Z][a-zA-Z0-9_-]*$` for GitHub Actions job ids, `^x-` for compose
/// extensions) cost under 700 steps for the first use and about 2 steps for
/// each test of a key of up to 100 bytes, so a path that is a few hundred
/// segments deep still fits.
const KEY_FALLBACK_STEP_ALLOWANCE: usize = 100_000;

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
/// The walk has two passes. The first follows the schemas that declare the
/// path: a key segment goes to the `properties` entry of that name, an item
/// segment to `items`. A schema without that entry tries every composition
/// branch with the same path, so a schema that references itself can come back
/// to the same schema with the same rest of the path. The walk goes through each
/// shared target once per rest of the path: a target already tried at that
/// point cannot succeed, or the walk would have ended. It keeps an explicit
/// stack, so the depth of a chain of references does not grow the call stack.
/// The first pass does no pattern work and its results do not change with the
/// second pass.
///
/// Only when the first pass finds nothing, and it met a schema with
/// `patternProperties` or `additionalProperties` that no `properties` entry
/// shadowed, does the second pass walk the same way from scratch. A key segment
/// that no `properties` entry names then tries the composition branches as the
/// first pass does, and only when none of them resolves the path, the schema's
/// own `patternProperties` entries whose pattern matches the key, searched for
/// in the key, in the order of the pattern text: the first match is tried
/// first, and a later one when the path dead-ends below it. A key that matches a
/// pattern is not additional, so `additionalProperties` applies to a key that no
/// pattern matches, and a schema that is `false` offers nothing. A pattern that
/// is not a valid regex, or is longer than `MAX_PATTERN_LEN`, never matches. Two
/// passes keep a property that a later branch declares ahead of the patterns and
/// the `additionalProperties` of an earlier one, and the branches keep their
/// place ahead of the patterns of the schema they belong to.
///
/// The second pass works within `KEY_FALLBACK_STEP_ALLOWANCE` steps, paid for
/// as validation pays. A payment the allowance refuses ends the resolution with
/// `None`, however much it has found so far, as no answer can be told from a
/// wrong one then.
pub fn resolve_schema_path<'a>(
    parsed: &'a ParsedSchema,
    schema: &'a JsonSchema,
    path: &[SchemaPathSegment],
) -> Option<&'a JsonSchema> {
    resolve_within(parsed, schema, path, KEY_FALLBACK_STEP_ALLOWANCE)
}

/// [`resolve_schema_path`] with a second pass of `allowance` steps.
fn resolve_within<'a>(
    parsed: &'a ParsedSchema,
    schema: &'a JsonSchema,
    path: &[SchemaPathSegment],
    allowance: usize,
) -> Option<&'a JsonSchema> {
    match walk(parsed, schema, path, None) {
        Ok(found) => Some(found),
        Err(Missing {
            key_fallbacks: false,
        }) => None,
        Err(Missing {
            key_fallbacks: true,
        }) => walk(parsed, schema, path, Some(&mut Allowance::new(allowance))).ok(),
    }
}

/// A walk that found nothing.
struct Missing {
    /// Whether the walk met a schema that `patternProperties` or
    /// `additionalProperties` could answer for, which only the second pass asks.
    key_fallbacks: bool,
}

/// The end of a second pass that a payment was refused in. Only the first pass
/// reports whether the second one has a use, so this says nothing.
const GIVEN_UP: Missing = Missing {
    key_fallbacks: false,
};

/// The steps the second pass has left, and the patterns it has paid for.
struct Allowance<'a> {
    left: usize,
    paid: HashMap<&'a str, Compiled>,
}

/// Take `steps` from `left`. A payment `left` does not cover is refused and
/// empties it, so nothing is paid for after a refusal.
const fn spend(left: &mut usize, steps: usize) -> bool {
    if let Some(rest) = left.checked_sub(steps) {
        *left = rest;
        true
    } else {
        *left = 0;
        false
    }
}

impl<'a> Allowance<'a> {
    fn new(steps: usize) -> Self {
        Self {
            left: steps,
            paid: HashMap::new(),
        }
    }

    /// Whether `pattern` matches `key`, or `None` when a payment is refused.
    ///
    /// A pattern is compiled, or taken from the cache, once per call. The price
    /// of that depends only on the pattern, so what a call pays does not depend
    /// on what the cache holds.
    fn matches(&mut self, pattern: &'a str, key: &str) -> Option<bool> {
        let Self { left, paid } = self;
        if pattern.len() > MAX_PATTERN_LEN {
            return spend(left, 1).then_some(false);
        }
        let compiled = match paid.entry(pattern) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                entry.insert(lookup_or_compile(pattern, |steps, _| spend(left, steps))?)
            }
        };
        if !spend(left, compiled.use_steps(key.len())) {
            return None;
        }
        Some(compiled.regex.as_ref().is_some_and(|re| re.is_match(key)))
    }

    /// The schemas `schema` offers for the undeclared `key`, or `None` when a
    /// payment is refused. A key that matches a pattern is not additional.
    fn schemas_for_key(
        &mut self,
        schema: &'a JsonSchema,
        key: &str,
    ) -> Option<Vec<&'a JsonSchema>> {
        let mut matching = Vec::new();
        for (pattern, pattern_schema) in schema.pattern_properties.iter().flatten() {
            if self.matches(pattern, key)? {
                matching.push(pattern_schema);
            }
        }
        if matching.is_empty()
            && let Some(AdditionalProperties::Schema(additional)) = &schema.additional_properties
        {
            matching.push(additional);
        }
        Some(matching)
    }
}

/// Take `steps` from `allowance` when there is one. `false` when it refuses.
fn charge(allowance: &mut Option<&mut Allowance<'_>>, steps: usize) -> bool {
    allowance
        .as_deref_mut()
        .is_none_or(|allowance| spend(&mut allowance.left, steps))
}

/// What the walk has left to do.
enum Step<'a, 'p> {
    /// Resolve `path` from the schema.
    Visit(&'a JsonSchema, &'p [SchemaPathSegment]),
    /// The second pass offers the schemas `schema` has for `key` once everything
    /// its composition branches offer is tried, and then resolves `rest` from them.
    KeyFallback(&'a JsonSchema, &'p str, &'p [SchemaPathSegment]),
}

/// One walk of the schema tree, the first pass without an `allowance` and the
/// second with one. See [`resolve_schema_path`].
fn walk<'a>(
    parsed: &'a ParsedSchema,
    schema: &'a JsonSchema,
    path: &[SchemaPathSegment],
    mut allowance: Option<&mut Allowance<'a>>,
) -> Result<&'a JsonSchema, Missing> {
    let mut key_fallbacks = false;
    let mut tried: HashSet<(*const JsonSchema, usize)> = HashSet::new();
    let mut stack = vec![Step::Visit(schema, path)];
    while let Some(step) = stack.pop() {
        let (node, path) = match step {
            Step::Visit(node, path) => (node, path),
            Step::KeyFallback(schema, key, rest) => {
                let Some(allowance) = allowance.as_deref_mut() else {
                    continue;
                };
                let Some(offered) = allowance.schemas_for_key(schema, key) else {
                    return Err(GIVEN_UP);
                };
                if !spend(&mut allowance.left, offered.len()) {
                    return Err(GIVEN_UP);
                }
                stack.extend(offered.into_iter().rev().map(|s| Step::Visit(s, rest)));
                continue;
            }
        };
        let schema = parsed.resolve(node);
        if node.reference.is_some() && !tried.insert((ptr::from_ref(schema), path.len())) {
            continue;
        }
        let [segment, rest @ ..] = path else {
            return Ok(schema);
        };

        let key = match segment {
            // Array item descent: own `items` first, then the composition branches.
            SchemaPathSegment::Item(_) => {
                if let Some(items) = schema.items.as_deref() {
                    if !charge(&mut allowance, 1) {
                        return Err(GIVEN_UP);
                    }
                    stack.push(Step::Visit(items, rest));
                } else {
                    if let Some(allowance) = allowance.as_deref_mut()
                        && !spend(&mut allowance.left, composition_branches(schema).count())
                    {
                        return Err(GIVEN_UP);
                    }
                    stack.extend(
                        composition_branches(schema)
                            .rev()
                            .map(|b| Step::Visit(b, path)),
                    );
                }
                continue;
            }
            SchemaPathSegment::Key(key) => key,
        };
        if !charge(&mut allowance, key.len() / WORK_PER_STEP) {
            return Err(GIVEN_UP);
        }

        // Direct property lookup.
        if let Some(prop_schema) = schema.properties.as_ref().and_then(|p| p.get(key.as_str())) {
            if !charge(&mut allowance, 1) {
                return Err(GIVEN_UP);
            }
            stack.push(Step::Visit(prop_schema, rest));
            continue;
        }

        // Walk composition branches; the first branch is tried first. In the
        // second pass the patterns and `additionalProperties` of the schema
        // itself come after all of them.
        if let Some(allowance) = allowance.as_deref_mut() {
            if !spend(&mut allowance.left, composition_branches(schema).count()) {
                return Err(GIVEN_UP);
            }
            stack.push(Step::KeyFallback(schema, key, rest));
        } else {
            key_fallbacks |=
                schema.pattern_properties.is_some() || schema.additional_properties.is_some();
        }
        stack.extend(
            composition_branches(schema)
                .rev()
                .map(|b| Step::Visit(b, path)),
        );
    }
    Err(Missing { key_fallbacks })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::schema::SchemaType;
    use crate::schema::pattern::{
        MAX_PATTERN_LEN, REGEX_SIZE_LIMIT, compile_attempts, compile_priced,
    };
    use crate::schema::test_support::{
        CHAIN_LENGTH, all_of, any_of, array_of, boolean_schema, chain, composes_itself, cyclic,
        declaring, denying_additional, described, field_branches, integer_schema, key, ladder,
        nest_in_all_of, no_targets, null_schema, object_schema, on_a_small_stack, one_of,
        reference_to, string_schema, with_additional, with_patterns, with_targets,
    };
    use regex::RegexBuilder;

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

    // ══════════════════════════════════════════════════════════════════════════
    // Patterns and additional properties
    // ══════════════════════════════════════════════════════════════════════════

    /// A schema with only `patternProperties`.
    fn patterned(patterns: Vec<(&str, JsonSchema)>) -> JsonSchema {
        with_patterns(JsonSchema::default(), patterns)
    }

    /// A schema with only `additionalProperties` set to `additional`.
    fn additional(additional: JsonSchema) -> JsonSchema {
        with_additional(JsonSchema::default(), additional)
    }

    /// A schema that declares the property `name` as a boolean.
    fn declaring_boolean(name: &str) -> JsonSchema {
        object_schema(vec![(name, boolean_schema())])
    }

    /// The `description` of the schema `path` resolves to.
    fn description_at(parsed: &ParsedSchema, path: &[SchemaPathSegment]) -> Option<String> {
        resolve_schema_path(parsed, &parsed.root, path)?
            .description
            .clone()
    }

    /// The `type` of the schema `path` resolves to when the second pass has
    /// `allowance` steps.
    fn type_within(
        schema: &JsonSchema,
        path: &[SchemaPathSegment],
        allowance: usize,
    ) -> Option<String> {
        match resolve_within(&no_targets(), schema, path, allowance)?
            .schema_type
            .as_ref()?
        {
            SchemaType::Single(t) => Some(t.clone()),
            SchemaType::Multiple(_) => None,
        }
    }

    /// The fewest steps of allowance with which `path` resolves, or `None` when
    /// it does not resolve even with a million.
    fn cheapest_allowance(schema: &JsonSchema, path: &[SchemaPathSegment]) -> Option<usize> {
        let parsed = no_targets();
        let (mut low, mut high) = (0, 1_000_000);
        resolve_within(&parsed, schema, path, high)?;
        while low < high {
            let middle = low + (high - low) / 2;
            if resolve_within(&parsed, schema, path, middle).is_some() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        Some(low)
    }

    /// Steps the first use of `pattern` in a call pays for compiling it. Does not
    /// touch the cache, but counts as compile attempts.
    fn compile_price(pattern: &str) -> usize {
        compile_priced(pattern, |_| true)
            .expect("the price is paid")
            .steps
    }

    /// Steps one test of `pattern` against a key of `key_len` bytes costs.
    fn use_price(pattern: &str, key_len: usize) -> usize {
        compile_priced(pattern, |_| true)
            .expect("the price is paid")
            .use_steps(key_len)
    }

    fn keys(name: &str, count: usize) -> Vec<SchemaPathSegment> {
        vec![key(name); count]
    }

    // ── A key, a pattern, and additionalProperties ───────────────────────────

    #[test]
    fn resolve_schema_path_resolves_a_key_that_a_pattern_matches() {
        let schema = patterned(vec![("^x-", integer_schema())]);

        assert_eq!(resolved_type(&schema, &["x-a"]).as_deref(), Some("integer"));
    }

    #[rstest]
    #[case::matches_inside("box", Some("integer"))]
    #[case::no_match("abc", None)]
    fn resolve_schema_path_searches_a_pattern_in_the_key(
        #[case] name: &str,
        #[case] expected: Option<&str>,
    ) {
        let schema = patterned(vec![("x", integer_schema())]);

        assert_eq!(resolved_type(&schema, &[name]).as_deref(), expected);
    }

    #[test]
    fn resolve_schema_path_is_none_for_a_key_no_pattern_matches() {
        let schema = patterned(vec![("^x-", integer_schema())]);

        assert!(resolved_type(&schema, &["y"]).is_none());
    }

    #[rstest]
    #[case::empty_pattern_matches_empty_key("^$", Some("integer"))]
    #[case::non_empty_pattern_does_not(".+", None)]
    fn resolve_schema_path_matches_an_empty_key_like_any_other(
        #[case] pattern: &str,
        #[case] expected: Option<&str>,
    ) {
        let schema = patterned(vec![(pattern, integer_schema())]);

        assert_eq!(resolved_type(&schema, &[""]).as_deref(), expected);
    }

    #[test]
    fn resolve_schema_path_matches_a_multibyte_key_as_text() {
        let schema = patterned(vec![("^ä", integer_schema())]);

        assert_eq!(resolved_type(&schema, &["äöü"]).as_deref(), Some("integer"));
        assert!(resolved_type(&schema, &["öü"]).is_none());
    }

    #[test]
    fn resolve_schema_path_resolves_any_key_through_additional_properties() {
        let schema = additional(object_schema(vec![("b", integer_schema())]));

        assert_eq!(
            resolved_type(&schema, &["whatever"]).as_deref(),
            Some("object")
        );
        assert_eq!(
            resolved_type(&schema, &["whatever", "b"]).as_deref(),
            Some("integer")
        );
        assert!(resolved_type(&schema, &["whatever", "c"]).is_none());
    }

    #[test]
    fn resolve_schema_path_resolves_nothing_through_additional_properties_false() {
        let schema = denying_additional(object_schema(vec![("a", string_schema())]));

        assert!(resolved_type(&schema, &["other"]).is_none());
        assert_eq!(resolved_type(&schema, &["a"]).as_deref(), Some("string"));
    }

    #[test]
    fn resolve_schema_path_resolves_a_key_through_additional_properties_true() {
        let parsed = ParsedSchema::new(
            crate::schema::parse_schema(&serde_json::json!({ "additionalProperties": true }))
                .expect("schema parses")
                .root,
        );

        assert!(resolve_schema_path(&parsed, &parsed.root, &[key("k")]).is_some());
        assert!(resolve_schema_path(&parsed, &parsed.root, &[key("k"), key("z")]).is_none());
    }

    #[test]
    fn resolve_schema_path_does_not_apply_additional_properties_to_a_key_a_pattern_matches() {
        let schema = with_additional(
            patterned(vec![("^a", object_schema(vec![("c", string_schema())]))]),
            object_schema(vec![("b", boolean_schema())]),
        );

        assert!(resolved_type(&schema, &["a1", "b"]).is_none());
        assert_eq!(
            resolved_type(&schema, &["a1", "c"]).as_deref(),
            Some("string")
        );
        assert_eq!(
            resolved_type(&schema, &["zzz", "b"]).as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn resolve_schema_path_applies_additional_properties_to_a_key_no_pattern_matches() {
        let schema = with_additional(patterned(vec![("^x-", integer_schema())]), boolean_schema());

        assert_eq!(resolved_type(&schema, &["y"]).as_deref(), Some("boolean"));
        assert_eq!(resolved_type(&schema, &["x-1"]).as_deref(), Some("integer"));
    }

    #[test]
    fn resolve_schema_path_own_property_shadows_a_pattern_and_additional_properties() {
        let schema = with_additional(
            with_patterns(
                object_schema(vec![("a", object_schema(vec![("x", integer_schema())]))]),
                vec![("^a", object_schema(vec![("y", integer_schema())]))],
            ),
            object_schema(vec![("z", integer_schema())]),
        );

        assert_eq!(resolved_type(&schema, &["a"]).as_deref(), Some("object"));
        assert_eq!(
            resolved_type(&schema, &["a", "x"]).as_deref(),
            Some("integer")
        );
        assert!(resolved_type(&schema, &["a", "y"]).is_none());
        assert!(resolved_type(&schema, &["a", "z"]).is_none());
    }

    #[test]
    fn resolve_schema_path_never_gives_an_item_a_pattern_or_additional_properties() {
        let schema = with_additional(patterned(vec![(".*", string_schema())]), string_schema());

        assert!(resolve_schema_path(&no_targets(), &schema, &[ITEM]).is_none());
    }

    #[test]
    fn resolve_schema_path_matches_a_brackets_key_with_a_pattern_but_not_an_item() {
        let schema = patterned(vec![(r"^\[\]$", integer_schema())]);

        assert_eq!(resolved_type(&schema, &["[]"]).as_deref(), Some("integer"));
        assert!(resolve_schema_path(&no_targets(), &schema, &[ITEM]).is_none());
    }

    #[rstest]
    #[case::pattern(patterned(vec![(".*", reference_to(0))]))]
    #[case::additional_properties(additional(reference_to(0)))]
    fn resolve_schema_path_resolves_a_reference_behind_a_key_to_its_target(
        #[case] root: JsonSchema,
    ) {
        let schema = with_targets(root, vec![described("the target")]);

        assert_eq!(
            description_at(&schema, &[key("k")]).as_deref(),
            Some("the target")
        );
    }

    #[test]
    fn resolve_schema_path_follows_keys_items_patterns_and_additional_properties_in_one_path() {
        let named = object_schema(vec![("name", string_schema())]);
        let schema = patterned(vec![(
            "^svc",
            object_schema(vec![("list", array_of(additional(named)))]),
        )]);
        let path = [key("svc1"), key("list"), ITEM, key("any"), key("name")];

        assert_eq!(resolved_type_at(&schema, &path).as_deref(), Some("string"));
    }

    // ── Several patterns ─────────────────────────────────────────────────────

    #[test]
    fn resolve_schema_path_takes_the_first_matching_pattern_in_the_order_stored() {
        let schema = patterned(vec![("^a", integer_schema()), ("^ab", string_schema())]);

        assert_eq!(resolved_type(&schema, &["abc"]).as_deref(), Some("integer"));
    }

    #[test]
    fn resolve_schema_path_takes_the_first_pattern_in_pattern_text_order_of_a_parsed_schema() {
        let parsed = crate::schema::parse_schema(&serde_json::json!({
            "patternProperties": {
                "^b": { "description": "b" },
                "^a": { "description": "a" }
            }
        }))
        .expect("schema parses");

        // The key matches both; "^a" sorts before "^b" whatever order the JSON lists.
        let key_matching_both = [key("ab")];

        assert_eq!(
            description_at(&parsed, &key_matching_both).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn resolve_schema_path_skips_a_pattern_the_key_does_not_match() {
        let schema = patterned(vec![("^a", integer_schema()), ("^b", string_schema())]);

        assert_eq!(resolved_type(&schema, &["bz"]).as_deref(), Some("string"));
    }

    #[test]
    fn resolve_schema_path_uses_a_later_matching_pattern_when_the_path_dead_ends_below_the_first() {
        let schema = patterned(vec![
            ("^k", declaring("x")),
            ("^k", declaring_boolean("z")),
            ("^k", object_schema(vec![("z", integer_schema())])),
        ]);

        assert_eq!(
            resolved_type(&schema, &["k1", "z"]).as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn resolve_schema_path_is_none_when_every_matching_pattern_dead_ends_even_with_additional_properties()
     {
        let schema = with_additional(
            patterned(vec![("^k", declaring("x")), ("^k", declaring("y"))]),
            declaring_boolean("z"),
        );

        assert!(resolved_type(&schema, &["k1", "z"]).is_none());
    }

    // ── Two passes ───────────────────────────────────────────────────────────

    #[rstest]
    #[case::own_property(
        with_patterns(object_schema(vec![("k", integer_schema())]), vec![("^k", string_schema())]),
        "k",
    )]
    #[case::property_of_a_later_branch(
        all_of(vec![
            patterned(vec![("(\\w{20}){20}", string_schema())]),
            object_schema(vec![("k", integer_schema())]),
        ]),
        "k",
    )]
    #[case::property_of_a_later_branch_beside_additional(
        all_of(vec![
            additional(string_schema()),
            object_schema(vec![("k", integer_schema())]),
        ]),
        "k",
    )]
    fn resolve_schema_path_compiles_no_pattern_for_a_path_the_first_pass_resolves(
        #[case] schema: JsonSchema,
        #[case] name: &str,
    ) {
        let compiled_before = compile_attempts();

        assert_eq!(resolved_type(&schema, &[name]).as_deref(), Some("integer"));
        assert_eq!(compile_attempts(), compiled_before);
    }

    #[test]
    fn resolve_schema_path_compiles_no_pattern_for_a_path_without_key_segments() {
        let schema = JsonSchema {
            items: Some(Box::new(integer_schema())),
            ..patterned(vec![("(\\w{20}){20}", string_schema())])
        };
        let compiled_before = compile_attempts();

        assert_eq!(
            resolved_type_at(&schema, &[ITEM]).as_deref(),
            Some("integer")
        );
        assert_eq!(compile_attempts(), compiled_before);
    }

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn resolve_schema_path_prefers_a_property_of_a_later_branch_to_a_pattern_of_an_earlier_one(
        #[case] compose: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let schema = compose(vec![
            patterned(vec![("^k", integer_schema())]),
            object_schema(vec![("k", string_schema())]),
        ]);

        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some("string"));
    }

    #[test]
    fn resolve_schema_path_prefers_a_one_of_property_to_an_all_of_pattern() {
        let schema = JsonSchema {
            all_of: Some(vec![patterned(vec![("^k", integer_schema())])]),
            one_of: Some(vec![object_schema(vec![("k", string_schema())])]),
            ..JsonSchema::default()
        };

        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some("string"));
    }

    #[rstest]
    #[case::schema(additional(integer_schema()))]
    #[case::permissive(additional(JsonSchema::default()))]
    fn resolve_schema_path_prefers_a_property_of_a_later_branch_to_additional_properties_of_an_earlier_one(
        #[case] earlier: JsonSchema,
    ) {
        let schema = all_of(vec![earlier, object_schema(vec![("k", string_schema())])]);

        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some("string"));
    }

    #[test]
    fn resolve_schema_path_prefers_a_branch_property_to_the_patterns_and_additional_properties_of_the_node_itself()
     {
        let schema = JsonSchema {
            all_of: Some(vec![object_schema(vec![("k", string_schema())])]),
            ..with_additional(patterned(vec![("^k", integer_schema())]), boolean_schema())
        };

        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some("string"));
    }

    #[test]
    fn resolve_schema_path_prefers_a_property_of_a_late_branch_to_permissive_additional_properties_in_a_nested_branch()
     {
        let schema = all_of(vec![
            any_of(vec![additional(JsonSchema::default())]),
            object_schema(vec![("k", string_schema())]),
        ]);

        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some("string"));
    }

    #[test]
    fn resolve_schema_path_uses_the_pattern_of_an_earlier_branch_when_nothing_declares_the_rest_of_the_path()
     {
        let schema = all_of(vec![
            patterned(vec![("^k", declaring_boolean("z"))]),
            object_schema(vec![("k", declaring("x"))]),
        ]);

        assert_eq!(
            resolved_type(&schema, &["k", "z"]).as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn resolve_schema_path_falls_to_the_next_branch_when_the_pattern_of_the_first_dead_ends() {
        let schema = all_of(vec![
            patterned(vec![("^k", declaring("x"))]),
            patterned(vec![("^k", declaring_boolean("z"))]),
        ]);

        assert_eq!(
            resolved_type(&schema, &["k", "z"]).as_deref(),
            Some("boolean")
        );
    }

    #[rstest]
    #[case::own_pattern(patterned(vec![("^k", declaring("x"))]))]
    #[case::own_additional_properties(additional(declaring("x")))]
    #[case::additional_properties_false(denying_additional(JsonSchema::default()))]
    #[case::pattern_that_does_not_match(patterned(vec![("^other", declaring("x"))]))]
    fn resolve_schema_path_falls_back_to_the_branches_when_what_the_node_offers_dead_ends(
        #[case] node: JsonSchema,
    ) {
        let schema = JsonSchema {
            all_of: Some(vec![patterned(vec![("^k", declaring_boolean("z"))])]),
            ..node
        };

        assert_eq!(
            resolved_type(&schema, &["k", "z"]).as_deref(),
            Some("boolean")
        );
    }

    #[rstest]
    #[case::additional_properties(with_targets(
        reference_to(0),
        vec![additional(described("X"))],
    ))]
    #[case::pattern(with_targets(
        reference_to(0),
        vec![patterned(vec![(".*", described("X"))])],
    ))]
    #[case::behind_a_branch(with_targets(
        all_of(vec![reference_to(0)]),
        vec![additional(described("X"))],
    ))]
    #[case::referenced_twice(with_targets(
        all_of(vec![reference_to(0), reference_to(0)]),
        vec![additional(described("X"))],
    ))]
    fn resolve_schema_path_walks_in_the_second_pass_a_target_the_first_pass_tried(
        #[case] schema: ParsedSchema,
    ) {
        assert_eq!(description_at(&schema, &[key("k")]).as_deref(), Some("X"));
    }

    #[test]
    fn resolve_schema_path_compiles_the_pattern_of_a_shared_target_once() {
        let target = patterned(vec![("^k", declaring("x"))]);
        let schema = with_targets(all_of(vec![reference_to(0), reference_to(0)]), vec![target]);
        let compiled_before = compile_attempts();

        assert!(resolve_schema_path(&schema, &schema.root, &[key("k"), key("z")]).is_none());
        assert_eq!(compile_attempts(), compiled_before + 1);
    }

    #[test]
    fn resolve_schema_path_follows_patterns_through_composition_and_references() {
        let schema = with_targets(
            all_of(vec![any_of(vec![reference_to(0)])]),
            vec![patterned(vec![("^j", reference_to(1))]), described("deep")],
        );

        assert_eq!(
            description_at(&schema, &[key("job")]).as_deref(),
            Some("deep")
        );
    }

    // ── Invalid and over-long patterns ───────────────────────────────────────

    #[test]
    fn resolve_schema_path_counts_an_invalid_pattern_as_not_matching() {
        let invalid_first = with_additional(
            patterned(vec![
                ("(unclosed", integer_schema()),
                ("^k", string_schema()),
            ]),
            boolean_schema(),
        );
        let invalid_only = patterned(vec![("(unclosed", integer_schema())]);
        let invalid_with_additional = with_additional(invalid_only.clone(), boolean_schema());

        assert_eq!(
            resolved_type(&invalid_first, &["k"]).as_deref(),
            Some("string")
        );
        assert!(resolved_type(&invalid_only, &["(unclosed"]).is_none());
        assert_eq!(
            resolved_type(&invalid_with_additional, &["(unclosed"]).as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn resolve_schema_path_counts_an_over_long_pattern_as_not_matching_and_never_compiles_it() {
        let long = "a".repeat(MAX_PATTERN_LEN + 1);
        let schema = with_additional(
            patterned(vec![(long.as_str(), integer_schema())]),
            boolean_schema(),
        );
        let compiled_before = compile_attempts();

        assert_eq!(
            resolved_type(&schema, &[long.as_str()]).as_deref(),
            Some("boolean")
        );
        assert_eq!(compile_attempts(), compiled_before);
    }

    #[test]
    fn resolve_schema_path_compiles_a_pattern_of_the_maximum_length() {
        let longest = "a".repeat(MAX_PATTERN_LEN);
        let schema = patterned(vec![(longest.as_str(), integer_schema())]);

        assert_eq!(
            resolved_type(&schema, &[longest.as_str()]).as_deref(),
            Some("integer")
        );
    }

    #[rstest]
    #[case::plain("^a")]
    #[case::syntax_error("(unclosed")]
    #[case::too_big_for_the_first_sizes("[a-z]{2000}")]
    #[case::too_big_for_every_size("(\\w{20}){20}")]
    #[case::small("[a-z]{100}")]
    fn resolve_schema_path_counts_a_pattern_as_valid_when_it_compiles_once_under_the_size_limit(
        #[case] pattern: &str,
    ) {
        let key_matching_when_valid = "a".repeat(2000);
        let schema = with_additional(
            patterned(vec![(pattern, integer_schema())]),
            string_schema(),
        );
        let compiles_alone = RegexBuilder::new(pattern)
            .size_limit(REGEX_SIZE_LIMIT)
            .build()
            .is_ok_and(|re| re.is_match(&key_matching_when_valid));
        let expected = if compiles_alone { "integer" } else { "string" };

        assert_eq!(
            type_within(&schema, &[key(&key_matching_when_valid)], usize::MAX).as_deref(),
            Some(expected)
        );
    }

    // ── Termination ──────────────────────────────────────────────────────────

    #[rstest]
    #[case::the_schema_itself(0)]
    #[case::one_key(1)]
    #[case::three_keys(3)]
    #[case::fifty_keys(50)]
    #[case::as_deep_as_a_document_goes(512)]
    fn path_resolution_ends_on_a_schema_that_applies_itself_through_a_pattern(
        #[case] depth: usize,
    ) {
        let target = with_patterns(
            object_schema(vec![("leaf", string_schema())]),
            vec![(".*", reference_to(0))],
        );
        let schema = with_targets(reference_to(0), vec![target]);
        let to_leaf = [keys("k", depth), vec![key("leaf")]].concat();
        let to_item = [keys("k", depth), vec![ITEM]].concat();

        assert!(resolve_schema_path(&schema, &schema.root, &keys("k", depth)).is_some());
        assert!(resolve_schema_path(&schema, &schema.root, &to_leaf).is_some());
        assert!(resolve_schema_path(&schema, &schema.root, &to_item).is_none());
    }

    #[rstest]
    #[case::the_schema_itself(0)]
    #[case::one_key(1)]
    #[case::fifty_keys(50)]
    #[case::as_deep_as_a_document_goes(512)]
    fn path_resolution_ends_on_a_schema_that_applies_itself_through_additional_properties(
        #[case] depth: usize,
    ) {
        let target = with_additional(
            object_schema(vec![("leaf", string_schema())]),
            reference_to(0),
        );
        let schema = with_targets(reference_to(0), vec![target]);
        let to_leaf = [keys("k", depth), vec![key("leaf")]].concat();
        let to_item = [keys("k", depth), vec![ITEM]].concat();

        assert!(resolve_schema_path(&schema, &schema.root, &keys("k", depth)).is_some());
        assert!(resolve_schema_path(&schema, &schema.root, &to_leaf).is_some());
        assert!(resolve_schema_path(&schema, &schema.root, &to_item).is_none());
    }

    #[rstest]
    #[case::all_of(all_of)]
    #[case::any_of(any_of)]
    #[case::one_of(one_of)]
    fn path_resolution_ends_on_a_schema_that_composes_itself_and_has_a_pattern(
        #[case] composing: fn(Vec<JsonSchema>) -> JsonSchema,
    ) {
        let target = JsonSchema {
            pattern_properties: Some(vec![("^a$".to_string(), string_schema())]),
            ..composing(vec![reference_to(0), reference_to(0)])
        };
        let schema = with_targets(object_schema(vec![("here", reference_to(0))]), vec![target]);

        assert!(resolve_schema_path(&schema, &schema.root, &[key("here"), key("a")]).is_some());
        assert!(resolve_schema_path(&schema, &schema.root, &[key("here"), key("b")]).is_none());
    }

    #[test]
    fn path_resolution_ends_on_a_chain_of_ten_thousand_targets_that_ends_in_a_pattern() {
        on_a_small_stack(|| {
            let mut targets: Vec<JsonSchema> = (1..CHAIN_LENGTH)
                .map(|next| all_of(vec![reference_to(next)]))
                .collect();
            targets.push(patterned(vec![("^x$", string_schema())]));
            let schema = with_targets(reference_to(0), targets);

            assert!(resolve_schema_path(&schema, &schema.root, &[key("x")]).is_some());
            assert!(resolve_schema_path(&schema, &schema.root, &[key("y")]).is_none());
        });
    }

    #[test]
    fn path_resolution_visits_each_target_of_a_ladder_through_patterns_once() {
        let layers = 64;
        let mut targets = Vec::new();
        for layer in 0..layers {
            let next = 2 * (layer + 1);
            for _ in 0..2 {
                targets.push(patterned(vec![
                    (".*", reference_to(next)),
                    (".+", reference_to(next + 1)),
                ]));
            }
        }
        targets.push(declaring("leaf"));
        let schema = with_targets(all_of(vec![reference_to(0), reference_to(1)]), targets);
        let to_leaf = [keys("k", layers), vec![key("leaf")]].concat();
        let to_missing = [keys("k", layers), vec![key("missing")]].concat();

        assert!(resolve_schema_path(&schema, &schema.root, &to_leaf).is_some());
        assert!(resolve_schema_path(&schema, &schema.root, &to_missing).is_none());
    }

    #[test]
    fn path_resolution_visits_each_target_of_a_ladder_through_additional_properties_once() {
        let layers = 64;
        let mut targets = Vec::new();
        for layer in 0..layers {
            let next = 2 * (layer + 1);
            for _ in 0..2 {
                targets.push(all_of(vec![
                    additional(reference_to(next)),
                    additional(reference_to(next + 1)),
                ]));
            }
        }
        targets.push(declaring("leaf"));
        let schema = with_targets(all_of(vec![reference_to(0), reference_to(1)]), targets);
        let to_leaf = [keys("k", layers), vec![key("leaf")]].concat();
        let to_missing = [keys("k", layers), vec![key("missing")]].concat();

        assert!(resolve_schema_path(&schema, &schema.root, &to_leaf).is_some());
        assert!(resolve_schema_path(&schema, &schema.root, &to_missing).is_none());
    }

    #[rstest]
    #[case::distinct_patterns(true)]
    #[case::one_pattern(false)]
    fn path_resolution_bounds_the_pattern_work_on_a_chain_of_ten_thousand_targets(
        #[case] distinct: bool,
    ) {
        on_a_small_stack(move || {
            let targets: Vec<JsonSchema> = (0..CHAIN_LENGTH)
                .map(|index| {
                    let pattern = if distinct {
                        format!("^never{index}$")
                    } else {
                        "^never$".to_string()
                    };
                    JsonSchema {
                        all_of: Some(vec![reference_to(index + 1)]),
                        ..patterned(vec![(pattern.as_str(), string_schema())])
                    }
                })
                .chain([JsonSchema::default()])
                .collect();
            let schema = with_targets(reference_to(0), targets);
            let compiled_before = compile_attempts();

            assert!(resolve_schema_path(&schema, &schema.root, &[key("k")]).is_none());
            // The cheapest compile attempt costs 512 steps.
            assert!(
                compile_attempts() - compiled_before <= KEY_FALLBACK_STEP_ALLOWANCE / 512,
                "the allowance bounds the compiles"
            );
        });
    }

    // ── The allowance ────────────────────────────────────────────────────────

    #[test]
    fn resolve_schema_path_does_not_make_a_compile_the_allowance_cannot_pay_for() {
        let pattern = "^refused-compile";
        let price = compile_price(pattern);
        let schema = patterned(vec![(pattern, integer_schema())]);
        let compiled_before = compile_attempts();

        assert!(type_within(&schema, &[key("refused-compile-1")], price - 1).is_none());
        assert_eq!(compile_attempts(), compiled_before);
    }

    #[test]
    fn resolve_schema_path_stops_at_the_compile_attempt_the_allowance_cannot_pay_for() {
        let pattern = "[a-z]{2000}";
        let mut attempt_prices = Vec::new();
        compile_priced(pattern, |price| {
            attempt_prices.push(price);
            true
        });
        assert!(
            attempt_prices.len() > 1,
            "the pattern needs several attempts"
        );
        let paid_attempts = attempt_prices.len() - 1;
        let long_key = "a".repeat(2000);
        // The node's visit comes first, then one payment for each attempt.
        let affordable: usize = long_key.len() / WORK_PER_STEP
            + attempt_prices.iter().take(paid_attempts).sum::<usize>();
        let schema = patterned(vec![(pattern, integer_schema())]);
        let compiled_before = compile_attempts();

        assert!(type_within(&schema, &[key(&long_key)], affordable).is_none());
        assert_eq!(compile_attempts() - compiled_before, paid_attempts);

        let compiled_before = compile_attempts();
        assert_eq!(
            type_within(&schema, &[key(&long_key)], usize::MAX).as_deref(),
            Some("integer")
        );
        assert_eq!(
            compile_attempts() - compiled_before,
            attempt_prices.len(),
            "the refused attempts left nothing half done in the cache"
        );
    }

    #[rstest]
    #[case::cold(false)]
    #[case::warm(true)]
    fn resolve_schema_path_charges_the_compile_price_whether_or_not_the_cache_holds_the_pattern(
        #[case] warm: bool,
    ) {
        let pattern = "^cached-price-[a-z]+$";
        let name = "cached-price-x";
        let schema = patterned(vec![(pattern, integer_schema())]);
        // The compile, one test of the key, and queueing the schema it points to.
        let price = compile_price(pattern) + use_price(pattern, name.len()) + 1;
        if warm {
            assert!(type_within(&schema, &[key(name)], usize::MAX).is_some());
        }
        let compiled_before = compile_attempts();

        assert!(type_within(&schema, &[key(name)], price - 1).is_none());
        assert_eq!(
            type_within(&schema, &[key(name)], price).as_deref(),
            Some("integer")
        );
        if warm {
            assert_eq!(compile_attempts(), compiled_before, "no compile when warm");
        }
        // After the first call the cache is warm in both cases.
        assert!(type_within(&schema, &[key(name)], price - 1).is_none());
        assert_eq!(
            type_within(&schema, &[key(name)], price).as_deref(),
            Some("integer")
        );
    }

    #[test]
    fn resolve_schema_path_charges_the_compile_price_once_per_call_and_each_test() {
        let pattern = "^[ab]$";
        let schema = patterned(vec![(
            pattern,
            patterned(vec![(pattern, integer_schema())]),
        )]);
        let path = [key("a"), key("b")];
        // One compile, a test at each of the two keys, two schemas queued.
        let price = compile_price(pattern) + 2 * use_price(pattern, 1) + 2;

        assert_eq!(cheapest_allowance(&schema, &path), Some(price));
        assert_eq!(cheapest_allowance(&schema, &path), Some(price), "per call");
    }

    #[rstest]
    #[case::short_key("^a", 63)]
    #[case::one_block_of_work("^a", 64)]
    #[case::two_blocks_of_work("^a", 128)]
    #[case::last_size_tier("(?:[a-z]{2000})?", 1)]
    #[case::last_size_tier_longer_key("(?:[a-z]{2000})?", 100)]
    fn resolve_schema_path_charges_a_test_one_step_and_the_matching_work_of_the_key(
        #[case] pattern: &str,
        #[case] key_len: usize,
    ) {
        let name = "a".repeat(key_len);
        let schema = patterned(vec![(pattern, integer_schema())]);
        let compiled = compile_priced(pattern, |_| true).expect("the price is paid");
        // The compile, the test, the work of the node on the key, and one schema queued.
        let price = compiled.steps
            + 1
            + key_len * compiled.matching_work_per_byte / WORK_PER_STEP
            + key_len / WORK_PER_STEP
            + 1;

        assert_eq!(cheapest_allowance(&schema, &[key(&name)]), Some(price));
    }

    #[test]
    fn resolve_schema_path_prices_the_last_size_tier_per_byte() {
        let compiled = compile_priced("(?:[a-z]{2000})?", |_| true).expect("the price is paid");

        assert_eq!(compiled.matching_work_per_byte, 512 * WORK_PER_STEP);
    }

    #[test]
    fn resolve_schema_path_charges_an_invalid_pattern_its_compile_and_one_step_without_matching_work()
     {
        let pattern = "(unclosed";
        let schema = with_additional(
            patterned(vec![(pattern, integer_schema())]),
            string_schema(),
        );
        let long_key = "k".repeat(10_000);
        // The compile, one step for the test, and the additional schema queued.
        let price = compile_price(pattern) + 1 + 1;

        assert_eq!(cheapest_allowance(&schema, &[key("k")]), Some(price));
        assert_eq!(
            cheapest_allowance(&schema, &[key(&long_key)]),
            Some(price + long_key.len() / WORK_PER_STEP),
            "only the visit of the node costs more for a long key"
        );
    }

    #[test]
    fn resolve_schema_path_charges_a_pattern_that_does_not_match_as_well() {
        let (first, second) = ("^zzz", "^b");
        let schema = patterned(vec![(first, integer_schema()), (second, string_schema())]);
        let price = compile_price(first)
            + use_price(first, 2)
            + compile_price(second)
            + use_price(second, 2)
            + 1;

        assert_eq!(cheapest_allowance(&schema, &[key("bz")]), Some(price));
    }

    #[rstest]
    #[case::alone(None)]
    #[case::with_additional_properties(Some(boolean_schema()))]
    fn resolve_schema_path_offers_nothing_from_a_node_when_the_allowance_refuses_one_of_its_patterns(
        #[case] additional_schema: Option<JsonSchema>,
    ) {
        let (first, second) = ("^k", "^zz-refused");
        let node = patterned(vec![(first, integer_schema()), (second, string_schema())]);
        let schema = additional_schema.map_or_else(
            || node.clone(),
            |extra| with_additional(node.clone(), extra),
        );
        let first_alone = compile_price(first) + use_price(first, 2);
        let both = first_alone + compile_price(second) + use_price(second, 2);

        // The first pattern matches and would resolve the path, but the second
        // cannot be tested, so the node offers neither pattern nor additional.
        assert!(type_within(&schema, &[key("k1")], both - 1).is_none());
        assert!(type_within(&schema, &[key("k1")], first_alone + 1).is_none());
        assert_eq!(
            type_within(&schema, &[key("k1")], both + 1).as_deref(),
            Some("integer")
        );
    }

    #[test]
    fn resolve_schema_path_offers_nothing_from_a_node_when_its_first_pattern_is_refused_though_the_second_is_cheap()
     {
        let (expensive, cheap) = ("[a-z]{2000}", "^k");
        // Patterns are tried in text order: "[a-z]{2000}" sorts before "^k".
        let schema = patterned(vec![
            (expensive, integer_schema()),
            (cheap, string_schema()),
        ]);
        let cheap_total = compile_price(cheap) + use_price(cheap, 2) + 1;

        assert!(type_within(&schema, &[key("k1")], cheap_total).is_none());
    }

    #[test]
    fn resolve_schema_path_ends_with_none_at_a_refusal_although_a_later_branch_costs_little() {
        let pattern = "^zz-expensive";
        let schema = all_of(vec![
            patterned(vec![(pattern, integer_schema())]),
            additional(string_schema()),
        ]);
        let compile = compile_price(pattern);

        assert!(type_within(&schema, &[key("k")], compile - 1).is_none());
        assert_eq!(
            type_within(&schema, &[key("k")], usize::MAX).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_charges_a_node_with_only_additional_properties_the_queueing_alone() {
        let schema = additional(string_schema());

        assert_eq!(cheapest_allowance(&schema, &[key("k")]), Some(1));
    }

    #[test]
    fn resolve_schema_path_charges_an_over_long_pattern_one_step_and_no_compile() {
        let long = "a".repeat(MAX_PATTERN_LEN + 1);
        let schema = with_additional(
            patterned(vec![(long.as_str(), integer_schema())]),
            string_schema(),
        );
        let compiled_before = compile_attempts();

        // One step for the pattern, one for queueing the additional schema.
        assert_eq!(cheapest_allowance(&schema, &[key("k")]), Some(2));
        assert_eq!(compile_attempts(), compiled_before);
    }

    #[test]
    fn resolve_schema_path_spends_nothing_in_the_first_pass() {
        let pattern = "^first-pass-pattern$";
        let schema = all_of(vec![
            patterned(vec![(pattern, string_schema())]),
            object_schema(vec![("k", integer_schema())]),
        ]);
        let mut hit = None;

        assert_eq!(
            type_within(&schema, &[key("k")], 0).as_deref(),
            Some("integer")
        );
        lookup_or_compile(pattern, |_, cache_hit| {
            hit = Some(cache_hit);
            true
        });
        assert_eq!(hit, Some(false), "the pattern was never compiled");
    }

    #[test]
    fn resolve_schema_path_resolves_a_deep_path_through_realistic_patterns_within_its_allowance() {
        let job_id = "^[_a-zA-Z][a-zA-Z0-9_-]*$";
        let name = "a".repeat(64);
        let schema = (0..8).fold(string_schema(), |inner, _| patterned(vec![(job_id, inner)]));

        assert_eq!(
            resolved_type_at(&schema, &keys(&name, 8)).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_offers_nothing_from_a_node_whose_patterns_cost_more_than_the_allowance()
    {
        let count = KEY_FALLBACK_STEP_ALLOWANCE / compile_price("^p00000") + 1;
        let patterns: Vec<(String, JsonSchema)> = (0..count)
            .map(|index| (format!("^p{index:05}"), integer_schema()))
            .collect();
        let schema = JsonSchema {
            pattern_properties: Some(patterns),
            ..with_additional(JsonSchema::default(), string_schema())
        };
        let last = format!("p{:05}", count - 1);
        let compiled_before = compile_attempts();

        assert!(resolve_schema_path(&no_targets(), &schema, &[key(&last)]).is_none());
        assert!(compile_attempts() - compiled_before < count);
    }

    #[test]
    fn resolve_schema_path_charges_the_visit_of_a_long_key_at_a_node() {
        let name = "k".repeat(64 * 1024);
        let schema = additional(string_schema());
        let visit = name.len() / WORK_PER_STEP;

        // The visit of the node, then one step for queueing the additional schema.
        assert!(type_within(&schema, &[key(&name)], visit).is_none());
        assert_eq!(
            type_within(&schema, &[key(&name)], visit + 1).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_charges_each_entry_of_a_node_with_many_over_long_patterns() {
        let long = "a".repeat(MAX_PATTERN_LEN + 1);
        let patterns: Vec<(&str, JsonSchema)> = (0..1_000)
            .map(|_| (long.as_str(), integer_schema()))
            .collect();
        let schema = with_additional(patterned(patterns), string_schema());

        assert!(type_within(&schema, &[key("k")], 500).is_none());
        assert_eq!(
            type_within(&schema, &[key("k")], 1_001).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn resolve_schema_path_charges_each_branch_a_node_queues() {
        let schema = all_of(vec![additional(string_schema()); 5_000]);

        assert!(type_within(&schema, &[key("k")], 1_000).is_none());
        assert_eq!(
            type_within(&schema, &[key("k")], 100_000).as_deref(),
            Some("string")
        );
    }

    // ── The patterns of a schema come after its branches ─────────────────────

    #[test]
    fn resolve_schema_path_prefers_what_a_branch_resolves_to_the_own_pattern_of_the_node() {
        let schema = JsonSchema {
            all_of: Some(vec![patterned(vec![("^k", declaring_boolean("z"))])]),
            ..patterned(vec![("^k", object_schema(vec![("z", integer_schema())]))])
        };

        assert_eq!(
            resolved_type(&schema, &["k", "z"]).as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn resolve_schema_path_falls_back_to_the_own_pattern_when_the_branches_dead_end() {
        let schema = JsonSchema {
            all_of: Some(vec![patterned(vec![("^k", declaring("x"))])]),
            ..patterned(vec![("^k", object_schema(vec![("z", integer_schema())]))])
        };

        assert_eq!(
            resolved_type(&schema, &["k", "z"]).as_deref(),
            Some("integer")
        );
    }

    #[rstest]
    #[case::a_branch_resolves("^k", "integer")]
    #[case::the_branches_dead_end("^other", "string")]
    fn resolve_schema_path_applies_own_additional_properties_only_after_the_branches(
        #[case] branch_pattern: &str,
        #[case] expected: &str,
    ) {
        let schema = JsonSchema {
            all_of: Some(vec![patterned(vec![(branch_pattern, integer_schema())])]),
            ..additional(string_schema())
        };

        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some(expected));
    }

    #[test]
    fn resolve_schema_path_keeps_own_additional_properties_from_a_key_the_own_pattern_matches_after_the_branches()
     {
        let schema = JsonSchema {
            all_of: Some(vec![patterned(vec![("^other", declaring("z"))])]),
            ..with_additional(
                patterned(vec![("^k", declaring("x"))]),
                object_schema(vec![("z", integer_schema())]),
            )
        };

        assert!(resolved_type(&schema, &["k", "z"]).is_none());
    }

    #[test]
    fn resolve_schema_path_walks_the_branches_depth_first_before_the_own_pattern() {
        let schema = JsonSchema {
            all_of: Some(vec![
                any_of(vec![patterned(vec![("^k", string_schema())])]),
                patterned(vec![("^k", boolean_schema())]),
            ]),
            ..patterned(vec![("^k", integer_schema())])
        };

        assert_eq!(resolved_type(&schema, &["k"]).as_deref(), Some("string"));
    }

    #[test]
    fn resolve_schema_path_ends_with_none_when_a_branch_is_refused_though_the_own_pattern_is_paid()
    {
        let own = "^k";
        let expensive = "[a-z]{2000}";
        let node = patterned(vec![(own, string_schema())]);
        let alone = compile_price(own) + use_price(own, 1) + 1;
        let schema = JsonSchema {
            all_of: Some(vec![patterned(vec![(expensive, integer_schema())])]),
            ..node.clone()
        };

        assert_eq!(
            type_within(&node, &[key("k")], alone).as_deref(),
            Some("string")
        );
        // One more step for queueing the branch.
        assert!(type_within(&schema, &[key("k")], alone + 1).is_none());
    }

    #[test]
    fn resolve_schema_path_resolves_a_pattern_of_the_maximum_length_in_the_last_size_tier_within_its_allowance()
     {
        let padding = "(?:)".repeat((MAX_PATTERN_LEN - "(?:[a-z]{2000})?".len()) / 4);
        let pattern = format!("(?:[a-z]{{2000}})?{padding}");
        assert_eq!(pattern.len(), MAX_PATTERN_LEN);
        let mut attempts = 0;
        let compiled = compile_priced(&pattern, |_| {
            attempts += 1;
            true
        })
        .expect("the price is paid");
        assert_eq!(attempts, 3, "the pattern ends under the last size tier");
        assert_eq!(compiled.matching_work_per_byte, 512 * WORK_PER_STEP);
        let schema = patterned(vec![(pattern.as_str(), integer_schema())]);
        let name = "a".repeat(100);

        assert_eq!(
            resolved_type(&schema, &[name.as_str()]).as_deref(),
            Some("integer")
        );
    }
}
