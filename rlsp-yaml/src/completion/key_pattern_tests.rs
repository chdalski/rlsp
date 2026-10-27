// SPDX-License-Identifier: MIT

//! Completion below keys that `patternProperties` or `additionalProperties`
//! describe: the keys and values of the schema reached through them.

use rstest::rstest;
use serde_json::json;

use super::support::test_fixtures::{labels, object_schema, pos, string_schema};
use super::*;
use crate::schema::test_support::{
    array_of, one_of, reference_to, with_additional, with_patterns, with_targets,
};
use crate::schema::{JsonSchema, ParsedSchema, SchemaType};
use crate::test_utils::parse_docs;

fn through_pattern(job: JsonSchema) -> JsonSchema {
    with_patterns(JsonSchema::default(), vec![("^[a-z]+$", job)])
}

fn through_additional_properties(job: JsonSchema) -> JsonSchema {
    with_additional(JsonSchema::default(), job)
}

/// A schema whose `jobs` map is `jobs`. A job is one of two objects behind a
/// reference, and the steps of the first are behind a second reference.
fn workflow(jobs: fn(JsonSchema) -> JsonSchema) -> ParsedSchema {
    let runner = JsonSchema {
        schema_type: Some(SchemaType::Single("string".to_string())),
        enum_values: Some(vec![json!("ubuntu-latest"), json!("windows-latest")]),
        ..JsonSchema::default()
    };
    let job = one_of(vec![
        object_schema(vec![
            ("runs-on", runner),
            ("steps", array_of(reference_to(1))),
        ]),
        object_schema(vec![("uses", string_schema()), ("with", string_schema())]),
    ]);
    let step = object_schema(vec![
        ("uses", string_schema()),
        ("run", string_schema()),
        ("name", string_schema()),
    ]);
    with_targets(
        object_schema(vec![("jobs", jobs(reference_to(0)))]),
        vec![job, step],
    )
}

fn completed(schema: &ParsedSchema, text: &str, line: u32, character: u32) -> Vec<String> {
    let docs = parse_docs(text);
    let items = complete_at(&docs, pos(line, character), Some(schema));
    labels(&items)
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

#[rstest]
fn complete_offers_the_keys_of_a_job_on_a_blank_line_below_it(
    #[values(through_pattern, through_additional_properties)] jobs: fn(JsonSchema) -> JsonSchema,
) {
    let schema = workflow(jobs);

    let offered = completed(&schema, "jobs:\n  build:\n    steps: []\n    \n", 3, 4);

    // Both branches of the job offer their keys; the present key is left out.
    assert!(offered.contains(&"runs-on".to_string()), "{offered:?}");
    assert!(offered.contains(&"uses".to_string()), "{offered:?}");
    assert!(offered.contains(&"with".to_string()), "{offered:?}");
    assert!(!offered.contains(&"steps".to_string()), "{offered:?}");
}

#[rstest]
fn complete_offers_the_other_keys_of_a_job_on_a_key_of_it(
    #[values(through_pattern, through_additional_properties)] jobs: fn(JsonSchema) -> JsonSchema,
) {
    let schema = workflow(jobs);

    let offered = completed(&schema, "jobs:\n  build:\n    runs-on: x\n", 2, 4);

    assert!(offered.contains(&"steps".to_string()), "{offered:?}");
}

#[rstest]
fn complete_offers_the_enum_values_of_a_key_of_a_job(
    #[values(through_pattern, through_additional_properties)] jobs: fn(JsonSchema) -> JsonSchema,
) {
    let schema = workflow(jobs);

    let offered = completed(&schema, "jobs:\n  build:\n    runs-on: \n", 2, 13);

    assert!(
        offered.contains(&"ubuntu-latest".to_string()),
        "{offered:?}"
    );
    assert!(
        offered.contains(&"windows-latest".to_string()),
        "{offered:?}"
    );
}

#[rstest]
fn complete_offers_the_keys_of_a_step_on_a_blank_line_in_it(
    #[values(through_pattern, through_additional_properties)] jobs: fn(JsonSchema) -> JsonSchema,
) {
    let schema = workflow(jobs);

    let offered = completed(
        &schema,
        "jobs:\n  build:\n    steps:\n      - uses: x\n        \n",
        4,
        8,
    );

    assert!(offered.contains(&"run".to_string()), "{offered:?}");
    assert!(offered.contains(&"name".to_string()), "{offered:?}");
    assert!(!offered.contains(&"uses".to_string()), "{offered:?}");
}

#[rstest]
fn complete_offers_the_keys_of_a_step_on_a_blank_item(
    #[values(through_pattern, through_additional_properties)] jobs: fn(JsonSchema) -> JsonSchema,
) {
    let schema = workflow(jobs);

    let offered = completed(&schema, "jobs:\n  build:\n    steps:\n      - \n", 3, 8);

    assert!(offered.contains(&"uses".to_string()), "{offered:?}");
    assert!(offered.contains(&"run".to_string()), "{offered:?}");
}

#[rstest]
fn complete_offers_no_job_key_on_a_blank_line_where_the_job_name_goes(
    #[values(through_pattern, through_additional_properties)] jobs: fn(JsonSchema) -> JsonSchema,
) {
    let schema = workflow(jobs);

    let offered = completed(&schema, "jobs:\n  \n", 1, 2);

    assert!(!offered.contains(&"runs-on".to_string()), "{offered:?}");
}
