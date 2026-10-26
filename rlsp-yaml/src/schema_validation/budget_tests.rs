// SPDX-License-Identifier: MIT

use rstest::rstest;
use serde_json::json;

use super::context::{MAX_DIAGNOSTICS, STEP_CEILING, STEP_FLOOR, STEPS_PER_NODE};
use super::support::{
    collect_evaluated_item_count, collect_evaluated_properties, compile_attempts,
    is_valid_regex_value,
};
use super::*;
use crate::schema::{SchemaType, parse_schema};
use crate::test_utils::parse_docs;

const CLOUD_CONFIG: &str = "#cloud-config
package_update: true
package_upgrade: false
packages:
  - git
  - curl
users:
  - name: deploy
    groups: [sudo, docker]
    shell: /bin/bash
    ssh_authorized_keys:
      - ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample deploy@example
write_files:
  - path: /etc/motd
    content: |
      Welcome
runcmd:
  - [ ls, -l, / ]
  - echo done
timezone: UTC
";

const CLOUD_INIT: &str = include_str!("../../tests/fixtures/schemas/cloud-init.json");
const GITHUB_WORKFLOW: &str = include_str!("../../tests/fixtures/schemas/github-workflow.json");
const COMPOSE_SPEC: &str = include_str!("../../tests/fixtures/schemas/compose-spec.json");
const KUBERNETES_DEPLOYMENT: &str =
    include_str!("../../tests/fixtures/schemas/kubernetes-deployment-apps-v1.json");

fn fixture_schema(raw: &str) -> JsonSchema {
    let value: serde_json::Value = serde_json::from_str(raw).expect("fixture is JSON");
    parse_schema(&value).expect("fixture is a schema").root
}

fn schema_of(raw: &serde_json::Value) -> JsonSchema {
    parse_schema(raw).expect("valid schema").root
}

/// Validate `text` against `raw_schema` within a budget of `steps`.
fn run(raw_schema: &serde_json::Value, text: &str, steps: usize) -> Validation {
    let schema = schema_of(raw_schema);
    validate_documents(
        &parse_docs(text),
        &schema,
        true,
        YamlVersion::V1_2,
        &Budget::new(steps),
    )
}

/// Steps an unbounded run of `text` against `raw_schema` spends.
fn steps_needed(raw_schema: &serde_json::Value, text: &str) -> usize {
    let schema = schema_of(raw_schema);
    let budget = Budget::new(usize::MAX);
    let _ = validate_documents(&parse_docs(text), &schema, true, YamlVersion::V1_2, &budget);
    budget.spent()
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter_map(|d| match &d.code {
            Some(tower_lsp::lsp_types::NumberOrString::String(code)) => Some(code.clone()),
            _ => None,
        })
        .collect()
}

// ══════════════════════════════════════════════════════════════════════════
// Budget and step_budget
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn budget_grants_spends_until_the_limit_then_refuses() {
    let budget = Budget::new(2);

    assert!(budget.spend(1));
    assert!(budget.spend(1));
    assert!(
        !budget.is_exhausted(),
        "reaching the limit is not exhaustion"
    );
    assert!(!budget.spend(1));
    assert!(budget.is_exhausted());
}

#[test]
fn budget_refuses_a_spend_larger_than_what_remains() {
    let budget = Budget::new(5);

    assert!(!budget.spend(6));
    assert!(budget.is_exhausted());
    assert_eq!(budget.spent(), 5);
}

#[test]
fn budget_of_zero_refuses_the_first_spend_without_underflow() {
    let budget = Budget::new(0);

    assert!(budget.spend(0), "a zero-step spend is free");
    assert!(!budget.spend(1));
    assert!(budget.is_exhausted());
}

#[test]
fn budget_stays_exhausted_after_a_refusal() {
    let budget = Budget::new(1);
    assert!(!budget.spend(2));

    assert!(!budget.spend(1));
    assert!(budget.is_exhausted());
}

#[test]
fn budget_survives_a_spend_that_would_overflow() {
    let budget = Budget::new(usize::MAX);
    assert!(budget.spend(usize::MAX - 1));

    assert!(!budget.spend(usize::MAX));
    assert!(budget.is_exhausted());
}

#[rstest]
#[case::no_documents("", 0)]
#[case::scalar("hello", 1)]
#[case::long_scalar_weighs_more(
    "\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"",
    2
)]
#[case::mapping_counts_keys_and_values("a: 1\nb: 2", 5)]
#[case::sequence("[1, 2, 3]", 4)]
#[case::nested("a:\n  - b\n  - c: d", 7)]
#[case::alias_is_one_node("a: &x 1\nb: *x", 5)]
#[case::two_documents_add_up("a: 1\n---\nb: 2", 6)]
fn step_budget_counts_every_node_once(#[case] text: &str, #[case] nodes: usize) {
    assert_eq!(
        step_budget(&parse_docs(text)),
        STEP_FLOOR + nodes * STEPS_PER_NODE
    );
}

#[test]
fn step_budget_is_capped_at_the_ceiling() {
    let text = "- 1\n".repeat(STEP_CEILING / STEPS_PER_NODE + 10);

    assert_eq!(step_budget(&parse_docs(&text)), STEP_CEILING);
}

// ══════════════════════════════════════════════════════════════════════════
// collect_evaluated_* report what they could not finish
// ══════════════════════════════════════════════════════════════════════════

fn all_of_declaring(key: &str, position: usize, width: usize) -> JsonSchema {
    let branches = (0..width).map(|i| {
        if i == position {
            schema_of(&json!({"properties": {key: {}}}))
        } else {
            schema_of(&json!({"required": ["other"]}))
        }
    });
    JsonSchema {
        all_of: Some(branches.collect()),
        ..JsonSchema::default()
    }
}

#[test]
fn evaluated_properties_is_unknown_without_budget() {
    let schema = schema_of(&json!({"properties": {"a": {}}}));

    assert_eq!(
        collect_evaluated_properties(&schema, "a", &Budget::new(0), 0),
        None
    );
}

#[test]
fn evaluated_properties_finds_a_key_declared_at_the_root_with_one_step() {
    let schema = all_of_declaring("a", 3, 100);
    let root = JsonSchema {
        properties: Some([("a".to_string(), JsonSchema::default())].into()),
        ..schema
    };

    assert_eq!(
        collect_evaluated_properties(&root, "a", &Budget::new(1), 0),
        Some(true)
    );
}

#[rstest]
#[case::one_step_short(2, None)]
#[case::enough_to_find_it(3, Some(true))]
fn evaluated_properties_cut_short_before_the_declaring_branch_is_unknown(
    #[case] steps: usize,
    #[case] expected: Option<bool>,
) {
    // Root plus branches 0 and 1; the key is declared in branch 1.
    let schema = all_of_declaring("a", 1, 100);

    assert_eq!(
        collect_evaluated_properties(&schema, "a", &Budget::new(steps), 0),
        expected
    );
}

#[rstest]
#[case::exact_budget_finishes(4, Some(false))]
#[case::one_step_short_is_unknown(3, None)]
fn evaluated_properties_needs_every_schema_node_to_rule_a_key_out(
    #[case] steps: usize,
    #[case] expected: Option<bool>,
) {
    // Root plus three branches, none declaring the key.
    let schema = all_of_declaring("a", 99, 3);
    let budget = Budget::new(steps);

    assert_eq!(
        collect_evaluated_properties(&schema, "a", &budget, 0),
        expected
    );
    assert_eq!(budget.is_exhausted(), expected.is_none());
}

#[rstest]
#[case::all_of("allOf")]
#[case::any_of("anyOf")]
#[case::one_of("oneOf")]
fn evaluated_properties_looks_past_the_twentieth_branch(#[case] keyword: &str) {
    let mut branches = vec![json!({"required": ["other"]}); 24];
    branches.push(json!({"properties": {"a": {}}}));
    let schema = schema_of(&json!({ keyword: branches }));

    assert_eq!(
        collect_evaluated_properties(&schema, "a", &Budget::new(usize::MAX), 0),
        Some(true)
    );
}

#[test]
fn evaluated_properties_is_unknown_past_the_depth_limit() {
    let schema = schema_of(&json!({"properties": {"a": {}}}));

    assert_eq!(
        collect_evaluated_properties(
            &schema,
            "a",
            &Budget::new(usize::MAX),
            support::MAX_VALIDATION_DEPTH + 1
        ),
        None
    );
}

#[rstest]
#[case::items_cover_everything(json!({"items": {}}), Some(usize::MAX))]
#[case::prefix_items_cover_their_length(json!({"prefixItems": [{}, {}]}), Some(2))]
#[case::nothing_evaluated(json!({}), Some(0))]
#[case::items_in_the_twenty_fifth_all_of_branch(
    json!({"allOf": twenty_four_failing_branches_then(json!({"items": {}}))}),
    Some(usize::MAX)
)]
fn evaluated_item_count_covers_every_all_of_branch(
    #[case] raw: serde_json::Value,
    #[case] expected: Option<usize>,
) {
    let schema = schema_of(&raw);

    assert_eq!(
        collect_evaluated_item_count(&schema, &Budget::new(usize::MAX), 0),
        expected
    );
}

#[test]
fn evaluated_item_count_is_unknown_when_the_budget_cuts_the_scan() {
    let schema = schema_of(&json!({"allOf": [{}, {}, {}]}));

    assert_eq!(
        collect_evaluated_item_count(&schema, &Budget::new(3), 0),
        None
    );
    assert_eq!(
        collect_evaluated_item_count(&schema, &Budget::new(4), 0),
        Some(0)
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Completeness
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn run_with_no_documents_is_complete() {
    let schema = schema_of(&json!({"type": "object"}));

    let result = validate_schema(&[], &ParsedSchema::new(schema), true, YamlVersion::V1_2);

    assert_eq!(result.completeness, Completeness::Complete);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn invalid_document_is_still_complete() {
    let result = run(&json!({"type": "integer"}), "hello", 1_000);

    assert_eq!(result.completeness, Completeness::Complete);
    assert_eq!(codes(&result.diagnostics), ["schemaType"]);
}

#[test]
fn zero_steps_is_exhausted_with_no_diagnostics() {
    let result = run(&json!({"type": "integer"}), "hello", 0);

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn one_step_fewer_than_needed_is_exhausted() {
    let schema = json!({"properties": {"a": {"type": "integer"}}, "anyOf": [{"required": ["z"]}]});
    let text = "a: 1";
    let needed = steps_needed(&schema, text);

    assert_eq!(
        run(&schema, text, needed).completeness,
        Completeness::Complete
    );
    assert_eq!(
        run(&schema, text, needed - 1).completeness,
        Completeness::StoppedEarly
    );
}

#[test]
fn documents_share_one_budget() {
    let schema = json!({"properties": {"a": {"type": "integer"}}});
    let one = "a: x";
    let two = "a: x\n---\na: y";
    let needed = steps_needed(&schema, one);

    let result = run(&schema, two, needed);

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert_eq!(
        codes(&result.diagnostics),
        ["schemaType"],
        "the first document's diagnostic stands, the second is cut off"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Soundness under every budget
// ══════════════════════════════════════════════════════════════════════════

/// Run `text` against `raw_schema` with every budget from 0 to what the full run
/// needs. A run that stops early may drop diagnostics but must never invent one,
/// so each partial result is a subset of the full result.
fn assert_sound_under_every_budget(
    raw_schema: &serde_json::Value,
    text: &str,
    expected_codes: &[&str],
) {
    let needed = steps_needed(raw_schema, text);
    let full = run(raw_schema, text, needed);
    assert_eq!(full.completeness, Completeness::Complete);
    assert_eq!(codes(&full.diagnostics), expected_codes, "full run");

    for steps in 0..needed {
        let partial = run(raw_schema, text, steps);
        assert_eq!(
            partial.completeness,
            Completeness::StoppedEarly,
            "{steps} of {needed} steps"
        );
        for diagnostic in &partial.diagnostics {
            assert!(
                full.diagnostics.contains(diagnostic),
                "{steps} of {needed} steps invented {diagnostic:?}"
            );
        }
    }
}

fn twenty_four_failing_branches_then(last: serde_json::Value) -> Vec<serde_json::Value> {
    let mut branches: Vec<_> = (0..24)
        .map(|i| json!({"required": [format!("field_{i}")]}))
        .collect();
    branches.push(last);
    branches
}

#[rstest]
#[case::any_of_later_branch_matches(
    json!({"anyOf": [{"required": ["z"]}, {"type": "object"}]}),
    "a: 1",
    &[]
)]
#[case::any_of_none_match(
    json!({"anyOf": [{"required": ["z"]}, {"type": "array"}, {"properties": {"a": {"type": "string"}}}]}),
    "a: 1",
    &["schemaType"]
)]
#[case::any_of_only_the_twenty_fifth_branch_matches(
    json!({"anyOf": twenty_four_failing_branches_then(json!({"required": ["a"]}))}),
    "a: 1",
    &[]
)]
#[case::one_of_later_branch_matches(
    json!({"oneOf": [{"required": ["z"]}, {"type": "object"}]}),
    "a: 1",
    &[]
)]
#[case::one_of_only_the_twenty_fifth_branch_matches(
    json!({"oneOf": twenty_four_failing_branches_then(json!({"required": ["a"]}))}),
    "a: 1",
    &[]
)]
#[case::one_of_zero_match(
    json!({"oneOf": [{"required": ["z"]}, {"type": "array"}]}),
    "a: 1",
    &["schemaType"]
)]
#[case::one_of_two_match_and_a_failing_third(
    json!({"oneOf": [{"type": "object"}, {"required": ["a"]}, {"required": ["z"]}]}),
    "a: 1",
    &["schemaType"]
)]
#[case::not_matches(json!({"not": {"type": "object"}}), "a: 1", &["schemaNot"])]
#[case::not_does_not_match(json!({"not": {"type": "array"}}), "a: 1", &[])]
#[case::not_around_any_of(
    json!({"not": {"anyOf": [{"required": ["z"]}, {"type": "object"}]}}),
    "a: 1",
    &["schemaNot"]
)]
#[case::not_around_failing_any_of(
    json!({"not": {"anyOf": [{"required": ["z"]}, {"type": "array"}]}}),
    "a: 1",
    &[]
)]
#[case::if_selects_then(
    json!({"if": {"required": ["a"]}, "then": {"required": ["z"]}, "else": {"required": ["w"]}}),
    "a: 1",
    &["schemaRequired"]
)]
#[case::if_selects_else(
    json!({"if": {"required": ["q"]}, "then": {"required": ["z"]}, "else": {"required": ["w"]}}),
    "a: 1",
    &["schemaRequired"]
)]
#[case::contains_below_min(
    json!({"contains": {"type": "integer"}, "minContains": 3}),
    "[1, x, 2]",
    &["schemaContains"]
)]
#[case::contains_above_max(
    json!({"contains": {"type": "integer"}, "maxContains": 1}),
    "[1, 2, x]",
    &["schemaContains"]
)]
#[case::contains_satisfied(json!({"contains": {"type": "integer"}}), "[x, y, 1]", &[])]
#[case::unevaluated_properties_denied(
    json!({
        "allOf": [{"properties": {"a": {}}}, {"properties": {"b": {}}}],
        "unevaluatedProperties": false
    }),
    "a: 1\nb: 2\nc: 3",
    &["schemaUnevaluatedProperty"]
)]
#[case::unevaluated_properties_schema(
    json!({
        "allOf": [{"properties": {"a": {}}}, {"properties": {"b": {}}}],
        "unevaluatedProperties": {"type": "string"}
    }),
    "a: 1\nb: 2\nc: 3",
    &["schemaType"]
)]
#[case::unevaluated_items_with_all_of(
    json!({
        "allOf": [{"prefixItems": [{}]}, {"prefixItems": [{}, {}]}],
        "unevaluatedItems": {"type": "string"}
    }),
    "[a, b, 3, 4]",
    &["schemaType", "schemaType"]
)]
#[case::content_schema_inside_any_of(
    json!({"anyOf": [
        {"type": "string", "contentEncoding": "base64",
         "contentSchema": {"properties": {"a": {"type": "string"}}}},
        {"type": "integer"}
    ]}),
    "\"YTogMQpiOiAyCg==\"",
    &["schemaType"]
)]
#[case::pattern_property_unmatched_key_is_additional(
    json!({"patternProperties": {"^a": {"type": "integer"}}, "additionalProperties": false}),
    "ab: 1\nc: 2",
    &["schemaAdditionalProperty"]
)]
#[case::pattern_keyword(
    json!({"properties": {"a": {"pattern": "^x"}}}),
    "a: y",
    &["schemaPattern"]
)]
#[case::format_regex(
    json!({"properties": {"a": {"format": "regex"}}}),
    "a: \"(\"",
    &["schemaFormat"]
)]
#[case::pattern_in_unevaluated_scan(
    json!({
        "allOf": [{"patternProperties": {"^a": {}}}],
        "unevaluatedProperties": false
    }),
    "ab: 1\nc: 2",
    &["schemaUnevaluatedProperty"]
)]
#[case::diagnostics_before_a_composition(
    json!({"properties": {"a": {"type": "string"}}, "anyOf": [{"required": ["z"]}]}),
    "a: 1",
    &["schemaType", "schemaType"]
)]
fn diagnostics_are_sound_under_every_budget(
    #[case] schema: serde_json::Value,
    #[case] text: &str,
    #[case] expected_codes: &[&str],
) {
    assert_sound_under_every_budget(&schema, text, expected_codes);
}

#[test]
fn diagnostics_found_before_the_budget_ran_out_are_kept() {
    let failing_branches: Vec<_> = (0..100)
        .map(|i| json!({"required": [format!("z{i}")]}))
        .collect();
    let schema = json!({"properties": {
        "a": {"type": "integer"},
        "b": {"anyOf": failing_branches}
    }});
    // Enough for the root, `a` and the start of `b`'s probes, not for all of them.
    let result = run(&schema, "a: x\nb: {k: 1}", 10);

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert_eq!(codes(&result.diagnostics), ["schemaType"]);
    assert!(
        result.diagnostics[0]
            .message
            .contains("a does not match type"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn probe_that_failed_before_the_budget_ran_out_still_fails() {
    // The branch's `required` fails before its `properties` would be visited, so
    // four steps decide the anyOf: root and branch, each followed by the visit of
    // its one entry.
    let schema = json!({"anyOf": [
        {"required": ["z"], "properties": {"a": {"type": "string"}}}
    ]});

    let result = run(&schema, "a: 1", 4);

    assert_eq!(result.completeness, Completeness::Complete);
    assert_eq!(codes(&result.diagnostics), ["schemaType"]);
    assert!(result.diagnostics[0].message.contains("(anyOf)"));
}

/// `{c: {c: ... leaf}}` nested `levels` deep, and a schema requiring a string
/// leaf at the same depth. The schema is built directly: parsing JSON stops
/// well short of the validation depth limit.
fn nested_leaf(levels: usize, leaf: &str) -> (JsonSchema, String) {
    let mut schema = JsonSchema {
        schema_type: Some(SchemaType::Single("string".to_string())),
        ..JsonSchema::default()
    };
    let mut text = leaf.to_string();
    for _ in 0..levels {
        schema = JsonSchema {
            properties: Some([("c".to_string(), schema)].into()),
            ..JsonSchema::default()
        };
        text = format!("{{c: {text}}}");
    }
    (schema, text)
}

// `not` fires when its sub-schema passes. A leaf the depth limit hid is not
// known to pass, so `not` must stay silent there.
#[rstest]
#[case::leaf_passes_within_the_limit(5, "x", &["schemaNot"][..])]
#[case::leaf_fails_within_the_limit(5, "5", &[][..])]
#[case::leaf_fails_beyond_the_limit(support::MAX_VALIDATION_DEPTH + 10, "5", &[][..])]
#[case::leaf_hidden_beyond_the_limit(support::MAX_VALIDATION_DEPTH + 10, "x", &[][..])]
fn depth_cut_inside_a_probe_is_undecided_not_a_pass(
    #[case] levels: usize,
    #[case] leaf: &str,
    #[case] expected_codes: &[&str],
) {
    let (inner, text) = nested_leaf(levels, leaf);
    let schema = JsonSchema {
        not: Some(Box::new(inner)),
        ..JsonSchema::default()
    };

    let result = validate_documents(
        &parse_docs(&text),
        &schema,
        true,
        YamlVersion::V1_2,
        &Budget::new(usize::MAX),
    );

    assert_eq!(codes(&result.diagnostics), expected_codes);
}

// ══════════════════════════════════════════════════════════════════════════
// unevaluatedProperties / unevaluatedItems under a tight budget
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn property_declared_in_the_twenty_fifth_branch_is_not_unevaluated() {
    let schema = json!({
        "allOf": twenty_four_failing_branches_then(json!({"properties": {"a": {}}})),
        "unevaluatedProperties": false
    });

    let result = run(&schema, "a: 1", usize::MAX);

    assert!(
        !codes(&result.diagnostics).contains(&"schemaUnevaluatedProperty".to_string()),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn undeclared_property_beside_twenty_five_branches_is_unevaluated() {
    let schema = json!({
        "allOf": twenty_four_failing_branches_then(json!({"properties": {"a": {}}})),
        "unevaluatedProperties": false
    });

    let result = run(&schema, "b: 1", usize::MAX);

    assert!(
        codes(&result.diagnostics).contains(&"schemaUnevaluatedProperty".to_string()),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn item_covered_by_the_twenty_fifth_all_of_branch_is_not_unevaluated() {
    let schema = json!({
        "allOf": twenty_four_failing_branches_then(json!({"items": {}})),
        "unevaluatedItems": {"type": "string"}
    });

    let result = run(&schema, "[1, 2]", usize::MAX);

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("does not match type")),
        "{:?}",
        result.diagnostics
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Hostile and wide schemas
// ══════════════════════════════════════════════════════════════════════════

/// `levels` nested `anyOf`s of `width` branches each, every leaf failing, with
/// `unevaluatedProperties` and `unevaluatedItems` at every level. Unbounded,
/// evaluating it takes `width.pow(levels)` leaf probes.
///
/// `unevaluatedItems` is `{"not": {}}` (rejects everything) rather than `false`:
/// `parse_schema` drops a boolean `false` there, which would leave it untested.
fn hostile_composition(levels: usize, width: usize) -> serde_json::Value {
    if levels == 0 {
        return json!({"required": ["z"]});
    }
    json!({
        "anyOf": vec![hostile_composition(levels - 1, width); width],
        "unevaluatedProperties": false,
        "unevaluatedItems": {"not": {}}
    })
}

#[test]
fn hostile_nested_composition_stops_at_the_budget_without_invented_diagnostics() {
    let schema = schema_of(&hostile_composition(5, 8));
    let docs = parse_docs("a: 1\nb: [1, 2]");
    let budget = Budget::new(step_budget(&docs));

    let result = validate_documents(&docs, &schema, true, YamlVersion::V1_2, &budget);

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert_eq!(
        budget.spent(),
        step_budget(&docs),
        "spent exactly the budget"
    );
    assert!(
        result.diagnostics.is_empty(),
        "every root diagnostic depends on unfinished work: {:?}",
        result.diagnostics
    );
}

#[test]
fn wide_any_of_with_a_hundred_failing_branches_completes_at_the_default_budget() {
    let branches: Vec<_> = (0..100)
        .map(|i| json!({"required": [format!("z{i}")]}))
        .collect();
    let schema = schema_of(&json!({"anyOf": branches}));

    let result = validate_schema(
        &parse_docs("a: 1"),
        &ParsedSchema::new(schema),
        true,
        YamlVersion::V1_2,
    );

    assert_eq!(result.completeness, Completeness::Complete);
    assert_eq!(result.diagnostics.len(), 1);
    assert!(result.diagnostics[0].message.contains("any of the 100"));
}

#[test]
fn wide_unevaluated_properties_document_completes_at_the_default_budget() {
    let branches: Vec<_> = (0..30)
        .map(|branch| {
            let properties: serde_json::Map<String, serde_json::Value> = (0..5)
                .map(|i| (format!("key_{branch}_{i}"), json!({})))
                .collect();
            json!({"properties": properties})
        })
        .collect();
    let schema = schema_of(&json!({"allOf": branches, "unevaluatedProperties": false}));
    let text: String = (0..30)
        .flat_map(|branch| (0..5).map(move |i| format!("key_{branch}_{i}: 1\n")))
        .collect();

    let result = validate_schema(
        &parse_docs(&text),
        &ParsedSchema::new(schema),
        true,
        YamlVersion::V1_2,
    );

    assert_eq!(result.completeness, Completeness::Complete);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

// ══════════════════════════════════════════════════════════════════════════
// contentSchema shares the budget
// ══════════════════════════════════════════════════════════════════════════

const CONTENT_SCHEMA: fn() -> serde_json::Value = || {
    json!({
        "type": "string",
        "contentEncoding": "base64",
        "contentSchema": {"items": {"type": "integer"}}
    })
};

#[test]
fn content_schema_steps_are_charged_to_the_run() {
    // base64("- 1\n- 2\n- 3\n")
    let text = "\"LSAxCi0gMgotIDMK\"";
    let needed = steps_needed(&CONTENT_SCHEMA(), text);

    assert_eq!(
        run(&CONTENT_SCHEMA(), text, needed).completeness,
        Completeness::Complete
    );
    assert_eq!(
        run(&CONTENT_SCHEMA(), text, needed - 1).completeness,
        Completeness::StoppedEarly
    );
    assert!(
        needed > 1 + 3,
        "the root, the sequence and its items all cost steps: {needed}"
    );
}

#[test]
fn embedded_content_larger_than_the_budget_exhausts_it() {
    // Twenty branches each walk the 20 000 embedded items, far more than the
    // budget, which grows with the length of the encoded string only.
    let embedded = "- 1\n".repeat(20_000);
    let text = format!("\"{}\"", data_encoding::BASE64.encode(embedded.as_bytes()));
    let schema = schema_of(&json!({
        "type": "string",
        "contentEncoding": "base64",
        "contentSchema": {"allOf": vec![json!({"items": {}}); 20]}
    }));
    let docs = parse_docs(&text);

    let result = validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2);

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn embedded_content_is_validated_from_depth_zero() {
    // The content is a document of its own: a string at the depth limit still
    // gets its content validated in full, and the outer nesting is not counted.
    let schema = schema_of(&json!({
        "contentSchema": {"properties": {"a": {"type": "string"}}}
    }));
    let docs = parse_docs("\"a: 1\"");
    let budget = Budget::new(usize::MAX);
    let mut diagnostics = Vec::new();
    let mut ctx = Ctx::new(
        &mut diagnostics,
        true,
        YamlVersion::V1_2,
        docs[0].line_index(),
        &budget,
    );

    validate_node(
        &docs[0].root,
        &schema,
        &[],
        &mut ctx,
        support::MAX_VALIDATION_DEPTH,
    );

    assert!(!ctx.undecided);
    assert_eq!(diagnostics.len(), 1);
}

// ══════════════════════════════════════════════════════════════════════════
// Real schemas
// ══════════════════════════════════════════════════════════════════════════

/// A fixture schema, real documents it accepts, and a document it rejects.
struct Fixture {
    name: &'static str,
    schema: &'static str,
    valid: &'static [&'static str],
    invalid: &'static str,
    invalid_message: &'static str,
}

const FIXTURES: [Fixture; 4] = [
    Fixture {
        name: "cloud-init",
        schema: CLOUD_INIT,
        valid: &[CLOUD_CONFIG],
        invalid: "#cloud-config\npackage_update: 5\n",
        invalid_message: "Value at package_update does not match type",
    },
    Fixture {
        name: "github-workflow",
        schema: GITHUB_WORKFLOW,
        valid: &[
            include_str!("../../tests/corpus/github-actions-matrix.yml"),
            include_str!("../../tests/corpus/release-plz-workflow.yml"),
        ],
        invalid: "on: push\njobs:\n  build:\n    runs-on: 5\n    steps: []\n",
        invalid_message: "Value at jobs.build does not match any of the 2 oneOf schemas",
    },
    Fixture {
        name: "compose-spec",
        schema: COMPOSE_SPEC,
        valid: &[include_str!("../../tests/corpus/docker-compose.yml")],
        invalid: "services:\n  web:\n    image: nginx\n    ports: 5\n",
        invalid_message: "Value at services.web.ports does not match type",
    },
    Fixture {
        name: "kubernetes-deployment",
        schema: KUBERNETES_DEPLOYMENT,
        valid: &[include_str!(
            "../../tests/corpus/kubernetes-deployment.yaml"
        )],
        invalid: "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: three\n",
        invalid_message: "Value at spec.replicas does not match type",
    },
];

fn validate_fixture_document(schema: &JsonSchema, text: &str, steps: Option<usize>) -> Validation {
    let docs = parse_docs(text);
    let budget = Budget::new(steps.unwrap_or_else(|| step_budget(&docs)));
    validate_documents(&docs, schema, true, YamlVersion::V1_2, &budget)
}

#[test]
fn fixture_schemas_flag_a_known_invalid_document() {
    // Guards against a schema that degenerated to "accept everything" (for
    // example through an unresolved `$ref`), which would make the tests below
    // trivially green.
    for fixture in &FIXTURES {
        let schema = fixture_schema(fixture.schema);

        let result = validate_fixture_document(&schema, fixture.invalid, None);

        assert_eq!(
            result.completeness,
            Completeness::Complete,
            "{}",
            fixture.name
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains(fixture.invalid_message)),
            "{}: {:?}",
            fixture.name,
            result.diagnostics
        );
    }
}

#[test]
fn fixture_valid_documents_complete_without_diagnostics_at_the_production_budget() {
    for fixture in &FIXTURES {
        let schema = fixture_schema(fixture.schema);
        for text in fixture.valid {
            let result = validate_fixture_document(&schema, text, None);

            assert_eq!(
                result.completeness,
                Completeness::Complete,
                "{}",
                fixture.name
            );
            assert!(
                result.diagnostics.is_empty(),
                "{}: {:?}",
                fixture.name,
                result.diagnostics
            );
        }
    }
}

#[test]
fn fixture_documents_complete_with_a_tenth_of_the_production_budget() {
    for fixture in &FIXTURES {
        let schema = fixture_schema(fixture.schema);
        for text in fixture.valid.iter().chain([&fixture.invalid]) {
            let production = validate_fixture_document(&schema, text, None);
            let tenth =
                validate_fixture_document(&schema, text, Some(step_budget(&parse_docs(text)) / 10));

            assert_eq!(
                tenth.completeness,
                Completeness::Complete,
                "{}",
                fixture.name
            );
            assert_eq!(
                tenth.diagnostics, production.diagnostics,
                "{}",
                fixture.name
            );
        }
    }
}

// Pins the other side of the calibration: the production budget must not be so
// generous that no real document comes near it. If a validation speed-up makes
// this fail, re-derive STEP_FLOOR and STEPS_PER_NODE on purpose — do not loosen
// the bound to make it pass.
#[test]
fn some_fixture_document_needs_a_hundredth_of_the_production_budget() {
    let exhausted = FIXTURES.iter().any(|fixture| {
        let schema = fixture_schema(fixture.schema);
        fixture.valid.iter().any(|text| {
            let hundredth = step_budget(&parse_docs(text)) / 100;
            validate_fixture_document(&schema, text, Some(hundredth)).completeness
                == Completeness::StoppedEarly
        })
    });

    assert!(exhausted);
}

#[test]
fn cloud_config_with_a_wrongly_typed_package_update_is_flagged() {
    let schema = fixture_schema(CLOUD_INIT);

    let result = validate_schema(
        &parse_docs("#cloud-config\npackage_update: 5\npackages:\n  - git\n"),
        &ParsedSchema::new(schema),
        true,
        YamlVersion::V1_2,
    );

    assert_eq!(codes(&result.diagnostics), ["schemaType"]);
    assert!(
        result.diagnostics[0].message.contains("package_update"),
        "{:?}",
        result.diagnostics
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Scans that grow with their input cost steps in proportion
// ══════════════════════════════════════════════════════════════════════════

/// Steps `with` costs beyond `without` on `text`.
fn extra_steps(with: &serde_json::Value, without: &serde_json::Value, text: &str) -> usize {
    steps_needed(with, text) - steps_needed(without, text)
}

fn numbered_keys(count: usize) -> String {
    (0..count)
        .map(|i| format!("k{i}: 1\n"))
        .collect::<Vec<_>>()
        .concat()
}

#[test]
fn unique_items_costs_a_step_per_block_of_pair_comparisons() {
    let items: Vec<_> = (0..1000).map(|i| i.to_string()).collect();
    let text = format!("[{}]", items.join(", "));

    // 1000 items visited, then 1000 * 999 / 2 comparisons
    assert_eq!(
        extra_steps(&json!({"uniqueItems": true}), &json!({}), &text),
        1000 + 499_500 / 64
    );
}

#[test]
fn unique_items_check_the_budget_cannot_pay_for_reports_nothing() {
    let items: Vec<_> = (0..20_000).map(|i| format!("- {i}\n")).collect();
    let text = format!("{}- 0\n", items.concat());
    let schema = schema_of(&json!({"uniqueItems": true}));

    let result = validate_schema(
        &parse_docs(&text),
        &ParsedSchema::new(schema),
        true,
        YamlVersion::V1_2,
    );

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn enum_costs_a_step_per_block_of_listed_values() {
    let values: Vec<_> = (0..640).map(|i| format!("v{i}")).collect();

    assert_eq!(
        extra_steps(&json!({"enum": values}), &json!({}), "v1"),
        640 / 64
    );
}

#[test]
fn required_costs_a_step_per_block_of_key_lookups() {
    let required: Vec<_> = (0..100).map(|i| format!("k{i}")).collect();

    assert_eq!(
        extra_steps(
            &json!({"required": required}),
            &json!({}),
            &numbered_keys(200)
        ),
        100 * 200 / 64
    );
}

#[test]
fn dependent_required_costs_a_step_per_block_of_key_lookups() {
    let schema = json!({"dependentRequired": {"k0": ["k1", "k2"], "k5": ["k6"]}});

    // (trigger + 2 listed) + (trigger + 1 listed) lookups over 200 entries
    assert_eq!(
        extra_steps(&schema, &json!({}), &numbered_keys(200)),
        (3 + 2) * 200 / 64
    );
}

#[test]
fn dependent_schemas_cost_the_trigger_scan_and_the_copied_nodes() {
    let schema = json!({"dependentSchemas": {"k0": {}}});

    // scan: 200 entries; copy: 400 nodes; the step that enters the copy; and its
    // 200 entries visited once more
    assert_eq!(
        extra_steps(&schema, &json!({}), &numbered_keys(200)),
        200 / 64 + 400 / 64 + 1 + 200
    );
}

#[test]
fn a_pattern_costs_the_same_steps_cold_or_cached() {
    let schema = json!({"patternProperties": {"^cache_independence_[0-9]+$": {}}});
    let text = numbered_keys(1);

    let cold = steps_needed(&schema, &text);
    let warm = steps_needed(&schema, &text);

    assert_eq!(
        cold, warm,
        "the regex cache must not change what a run costs"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Regex compilation is priced by the work it can cost
// ══════════════════════════════════════════════════════════════════════════

/// Base price of a compile attempt under each size limit (4 KiB, 64 KiB,
/// 512 KiB), in steps.
const TIER_BASES: [usize; 3] = [512, 4_096, 32_768];

/// Steps per byte of pattern an attempt costs for parsing it.
const PARSE_STEPS_PER_BYTE: usize = 2;

/// Steps the first `attempts` compile attempts at a `len`-byte pattern cost.
fn compile_price(attempts: usize, len: usize) -> usize {
    TIER_BASES[..attempts]
        .iter()
        .map(|base| base + len * PARSE_STEPS_PER_BYTE)
        .sum()
}

// Compiles under the 4 KiB limit.
const SMALL_REGEX: &str = "^[a-z]+$";
// Too big for 4 KiB, compiles under 64 KiB.
const SEMVER_REGEX: &str = r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?$";
// Too big for 64 KiB, compiles under 512 KiB.
const LARGE_REGEX: &str = "[a-z]{2000}";
// Too big even for 512 KiB.
const HUGE_REGEX: &str = r"(\w{20}){20}";
const SYNTAX_ERROR_REGEX: &str = "(unclosed";

/// The check this replaces, kept as the oracle for validity.
fn reference_is_valid_regex(s: &str) -> bool {
    s.len() <= 1024
        && regex::RegexBuilder::new(s)
            .size_limit(512 * 1024)
            .build()
            .is_ok()
}

#[rstest]
#[case::small_regex_pays_one_attempt(SMALL_REGEX, 1, true)]
#[case::syntax_error_pays_one_attempt(SYNTAX_ERROR_REGEX, 1, false)]
#[case::medium_regex_pays_two_attempts(SEMVER_REGEX, 2, true)]
#[case::large_regex_pays_three_attempts(LARGE_REGEX, 3, true)]
#[case::huge_regex_pays_three_attempts_and_is_invalid(HUGE_REGEX, 3, false)]
fn format_regex_value_is_priced_by_the_attempts_it_needs(
    #[case] value: &str,
    #[case] attempts: usize,
    #[case] valid: bool,
) {
    let budget = Budget::new(usize::MAX);
    let before = compile_attempts();

    let outcome = is_valid_regex_value(value, &budget);

    assert_eq!(outcome, Some(valid));
    assert_eq!(budget.spent(), compile_price(attempts, value.len()));
    assert_eq!(compile_attempts() - before, attempts);
}

#[rstest]
#[case::small(SMALL_REGEX)]
#[case::syntax_error(SYNTAX_ERROR_REGEX)]
#[case::medium(SEMVER_REGEX)]
#[case::large(LARGE_REGEX)]
#[case::huge(HUGE_REGEX)]
#[case::empty("")]
#[case::longer_than_a_pattern_may_be(&"a".repeat(1025))]
#[case::at_the_length_limit(&"a".repeat(1024))]
fn format_regex_validity_matches_a_single_512_kib_compile(#[case] value: &str) {
    assert_eq!(
        is_valid_regex_value(value, &Budget::new(usize::MAX)),
        Some(reference_is_valid_regex(value))
    );
}

#[test]
fn format_regex_attempt_the_budget_cannot_pay_for_is_not_made() {
    let all = compile_price(3, HUGE_REGEX.len());
    for (affordable, attempts_made) in [
        (compile_price(1, HUGE_REGEX.len()) - 1, 0),
        (compile_price(2, HUGE_REGEX.len()) - 1, 1),
        (all - 1, 2),
    ] {
        let budget = Budget::new(affordable);
        let before = compile_attempts();

        assert_eq!(is_valid_regex_value(HUGE_REGEX, &budget), None);

        assert!(budget.is_exhausted(), "{affordable} steps");
        assert_eq!(
            compile_attempts() - before,
            attempts_made,
            "{affordable} steps"
        );
    }
    assert_eq!(
        is_valid_regex_value(HUGE_REGEX, &Budget::new(all)),
        Some(false)
    );
}

#[test]
fn format_regex_value_the_budget_cannot_pay_for_reports_nothing() {
    let schema = schema_of(&json!({"properties": {"a": {"format": "regex"}}}));
    let text = format!("a: '{HUGE_REGEX}'");
    let docs = parse_docs(&text);
    let result = validate_documents(&docs, &schema, true, YamlVersion::V1_2, &Budget::new(1_000));

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn format_regex_value_is_compiled_once_per_run_and_charged_on_every_use() {
    let schema = json!({"items": {"type": "string", "format": "regex"}});
    let repeated = "- '^[a-z]+$'\n".repeat(5);
    let distinct = "- '^[a-z]+$'\n- '^[b-z]+$'\n- '^[c-z]+$'\n- '^[d-z]+$'\n- '^[e-z]+$'\n";

    let before = compile_attempts();
    let repeated_steps = steps_needed(&schema, &repeated);
    let repeated_compiles = compile_attempts() - before;
    let before = compile_attempts();
    let distinct_steps = steps_needed(&schema, distinct);
    let distinct_compiles = compile_attempts() - before;

    assert_eq!(repeated_compiles, 1);
    assert_eq!(distinct_compiles, 5);
    assert_eq!(
        repeated_steps, distinct_steps,
        "reuse must not change what the run costs"
    );
}

#[test]
fn format_regex_value_repeated_across_branches_is_compiled_once() {
    let schema = json!({"allOf": vec![json!({"format": "regex"}); 10]});

    let before = compile_attempts();
    let _ = steps_needed(&schema, "'^[a-z]+$'");

    assert_eq!(compile_attempts() - before, 1);
}

#[test]
fn schema_pattern_pays_its_compile_price_once_per_run() {
    let schema = json!({"patternProperties": {"^k[0-9]+$": {}}});

    // Per matching key: one use of the pattern and the step entering the value.
    // Once: the pattern's compile price.
    assert_eq!(
        extra_steps(&schema, &json!({}), &numbered_keys(3)),
        3 * 2 + compile_price(1, "^k[0-9]+$".len())
    );
}

// Matching a 1000-byte key: a block of 64 bytes per step against programs up to
// 64 KiB, 512 steps per byte against a program near the 512 KiB limit.
#[rstest]
#[case::tier_one(SMALL_REGEX, 1, 1000 / 64)]
#[case::tier_two(SEMVER_REGEX, 2, 1000 / 64)]
#[case::tier_three(LARGE_REGEX, 3, 1000 * 512)]
fn schema_pattern_match_is_priced_per_haystack_byte_by_tier(
    #[case] pattern: &str,
    #[case] attempts: usize,
    #[case] matching_steps: usize,
) {
    // None of the patterns matches this key, so no value is entered.
    let key = "!".repeat(1000);
    let text = format!("'{key}': 1\n");
    let schema = json!({"patternProperties": {pattern: {}}});

    // one use, the matching, and the pattern's compile price
    assert_eq!(
        extra_steps(&schema, &json!({}), &text),
        1 + matching_steps + compile_price(attempts, pattern.len())
    );
}

#[test]
fn invalid_schema_pattern_still_costs_a_step_per_use() {
    let schema = json!({"patternProperties": {SYNTAX_ERROR_REGEX: {}}});

    assert_eq!(
        extra_steps(&schema, &json!({}), &numbered_keys(4)),
        4 + compile_price(1, SYNTAX_ERROR_REGEX.len())
    );
}

#[test]
fn schema_pattern_too_big_to_compile_is_priced_at_every_attempt() {
    let schema = json!({"patternProperties": {HUGE_REGEX: {}}});

    // One use for the key; the pattern is invalid, so no value is entered.
    assert_eq!(
        extra_steps(&schema, &json!({}), &numbered_keys(1)),
        1 + compile_price(3, HUGE_REGEX.len())
    );
}

#[test]
fn pattern_properties_cost_steps_for_the_key_they_match() {
    let key = format!("k{}", "x".repeat(6399));
    // an explicit key: implicit keys are limited to 1024 characters
    let text = format!("? {key}\n: 1\n");
    let schema = json!({"patternProperties": {"^k": {}}});

    // One use plus scanning the 6400-byte key, compiling the 2-byte pattern, and
    // entering the matched value.
    assert_eq!(
        extra_steps(&schema, &json!({}), &text),
        1 + 6400 / 64 + compile_price(1, 2) + 1
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Diagnostic limit
// ══════════════════════════════════════════════════════════════════════════

fn unknown_keys_run(count: usize) -> Validation {
    let schema = schema_of(&json!({"additionalProperties": false}));
    validate_schema(
        &parse_docs(&numbered_keys(count)),
        &ParsedSchema::new(schema),
        true,
        YamlVersion::V1_2,
    )
}

#[rstest]
#[case::below_the_limit(MAX_DIAGNOSTICS - 1, MAX_DIAGNOSTICS - 1, Completeness::Complete)]
#[case::at_the_limit(MAX_DIAGNOSTICS, MAX_DIAGNOSTICS, Completeness::Complete)]
#[case::above_the_limit(MAX_DIAGNOSTICS + 1, MAX_DIAGNOSTICS, Completeness::StoppedEarly)]
fn diagnostics_are_capped_per_run(
    #[case] keys: usize,
    #[case] reported: usize,
    #[case] completeness: Completeness,
) {
    let result = unknown_keys_run(keys);

    assert_eq!(result.diagnostics.len(), reported);
    assert_eq!(result.completeness, completeness);
}

#[test]
fn reaching_the_diagnostic_limit_stops_the_remaining_documents() {
    let schema = schema_of(&json!({"additionalProperties": false}));
    let text = format!("{}---\nextra: 1\n", numbered_keys(MAX_DIAGNOSTICS));

    let result = validate_schema(
        &parse_docs(&text),
        &ParsedSchema::new(schema),
        true,
        YamlVersion::V1_2,
    );

    assert_eq!(result.diagnostics.len(), MAX_DIAGNOSTICS);
    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("'extra'")),
        "the second document is never validated"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// The width of a container costs steps at every schema that visits it
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn wide_mapping_under_many_branches_exhausts_the_budget() {
    // 400 branches each walk all 3000 entries of the root mapping.
    let branches = vec![json!({"properties": {"k0": {}}}); 400];
    let schema = schema_of(&json!({"allOf": branches}));
    let docs = parse_docs(&numbered_keys(3000));
    let budget = Budget::new(step_budget(&docs));

    let result = validate_documents(&docs, &schema, true, YamlVersion::V1_2, &budget);

    assert_eq!(result.completeness, Completeness::StoppedEarly);
    assert_eq!(budget.spent(), step_budget(&docs));
}

#[test]
fn wide_sequence_under_many_branches_exhausts_the_budget() {
    let schema = json!({"allOf": vec![json!({"items": {}}); 400]});
    let text = "- 1\n".repeat(3000);

    let result = run(&schema, &text, step_budget(&parse_docs(&text)));

    assert_eq!(result.completeness, Completeness::StoppedEarly);
}

#[test]
fn unique_items_repeated_across_branches_exhausts_the_budget() {
    let schema = json!({"allOf": vec![json!({"uniqueItems": true}); 200]});
    let text = (0..1000)
        .map(|i| format!("- {i}\n"))
        .collect::<Vec<_>>()
        .concat();

    let result = run(&schema, &text, step_budget(&parse_docs(&text)));

    assert_eq!(result.completeness, Completeness::StoppedEarly);
}

#[test]
fn string_checks_cost_steps_in_proportion_to_the_text() {
    let text = format!("\"{}\"", "x".repeat(6400));

    // `{}` neither constrains nor scans the string, yet entering the node scans
    // its 6400 bytes once for the checks that run on every string
    assert_eq!(
        extra_steps(&json!({"allOf": [{}]}), &json!({}), &text),
        1 + 6400 / 64
    );
}

#[test]
fn many_empty_branches_over_a_long_string_exhaust_the_budget() {
    let schema = json!({"allOf": vec![json!({}); 500]});
    let text = format!("\"{}\"", "x".repeat(64 * 1024));

    let result = run(&schema, &text, step_budget(&parse_docs(&text)));

    assert_eq!(result.completeness, Completeness::StoppedEarly);
}

#[test]
fn one_large_string_is_validated_in_full_at_the_production_budget() {
    let schema = schema_of(&json!({"type": "string", "minLength": 1, "pattern": "^x+$"}));
    let docs = parse_docs(&format!("\"{}\"", "x".repeat(100 * 1024)));

    let result = validate_schema(&docs, &ParsedSchema::new(schema), true, YamlVersion::V1_2);

    assert_eq!(result.completeness, Completeness::Complete);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn unevaluated_items_skip_the_scan_when_items_evaluate_everything() {
    let with = json!({"allOf": [{"items": {}}], "unevaluatedItems": {"type": "string"}});
    let without = json!({"allOf": [{"items": {}}]});
    let text = "- 1\n".repeat(3000);

    // only the two schema nodes the evaluated-items scan visits
    assert_eq!(extra_steps(&with, &without, &text), 2);
}
