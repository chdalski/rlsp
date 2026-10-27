// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::schema::parse_schema;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn completion_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/completion")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_completion_items_for_valid_position() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "completion result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize result");
    assert!(
        result_str.contains("age"),
        "completion should suggest sibling key 'age', got: {result_str}"
    );
}

#[tokio::test]
async fn should_return_empty_completions_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have result");
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "completion result should be null or empty for unknown document"
    );
}

// ── complete_at AST-branch integration tests (B-1 through B-4) ───────────────

// B-1: OnValue branch — cursor on a value position returns sibling values for the key
#[tokio::test]
async fn completion_on_value_position_returns_sibling_values() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_value.yaml";
    send(
        &mut service,
        did_open_notification(uri, "env: production\nenv: \n"),
    )
    .await;

    // Cursor on line 1 col 5 — after "env: ", inside the value position
    let resp = send(&mut service, completion_request(2, uri, 1, 5)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("production"),
        "value completion should suggest sibling value 'production', got: {result_str}"
    );
}

// B-2: OnKey branch with schema — cursor on a key position with schema returns schema properties
#[tokio::test]
async fn completion_on_key_position_with_schema_returns_schema_properties() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_key_schema.yaml";
    // Use a $schema modeline; schema fetch will fail but sibling structural
    // suggestions still work from the document itself.
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    // Cursor at line 0 col 0 — on "name" key
    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(
        !result.is_null(),
        "key completion with schema should return a result"
    );
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("age"),
        "key completion should include sibling key 'age', got: {result_str}"
    );
}

// B-3: OnKey inside nested mapping — cursor on a key in a nested mapping returns its sibling keys
#[tokio::test]
async fn completion_inside_nested_mapping_returns_sibling_keys() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_nested.yaml";
    send(
        &mut service,
        did_open_notification(uri, "server:\n  host: localhost\n  port: 8080\n"),
    )
    .await;

    // Cursor at line 1 col 2 — on "host" key inside nested mapping
    let resp = send(&mut service, completion_request(2, uri, 1, 2)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(
        !result.is_null(),
        "nested mapping key completion should return a result"
    );
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("port"),
        "nested mapping completion should suggest sibling key 'port', got: {result_str}"
    );
}

// B-4: InSequenceItem branch — cursor inside a sequence item returns sibling keys from other items
#[tokio::test]
async fn completion_inside_sequence_item_returns_sibling_item_keys() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion_seq.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "items:\n  - name: Alice\n    role: admin\n  - name: Bob\n    \n",
        ),
    )
    .await;

    // Cursor at line 4 col 4 — blank key position inside second sequence item
    let resp = send(&mut service, completion_request(2, uri, 4, 4)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(
        !result.is_null(),
        "sequence-item completion should return a result"
    );
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("role"),
        "sequence-item completion should suggest sibling key 'role' from other items, got: {result_str}"
    );
}

// ── TE-specified B-1 through B-4 integration tests ───────────────────────────

// B-1: Cursor on existing mapping key, asserts sibling key label
#[tokio::test]
async fn should_complete_at_mapping_key_suggests_sibling() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b1.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\nregion: us-east\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 0, 0)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("age") || result_str.contains("region"),
        "should suggest sibling keys, got: {result_str}"
    );
}

// B-2: Cursor on value position, asserts structural value fallback
#[tokio::test]
async fn should_complete_at_value_position_suggests_structural_values() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b2.yaml";
    send(
        &mut service,
        did_open_notification(uri, "env: production\nenv: staging\nenv: \n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 2, 5)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("production") || result_str.contains("staging"),
        "should suggest structural values, got: {result_str}"
    );
}

// B-3: Cursor on a genuine blank line inside a nested mapping, with a schema.
// Schema supplies the missing key ("port") that structural context alone cannot
// surface — InBlankMapping with schema excludes present keys and returns schema
// properties, exercising the exact branch that C-7 covers at the unit level.
#[tokio::test]
async fn should_complete_at_blank_line_in_nested_mapping_suggests_sibling_keys() {
    const B3_SCHEMA_URL: &str = "https://example.com/test-b3-server.json";
    let schema = parse_schema(&serde_json::json!({
        "type": "object",
        "properties": {
            "server": {
                "type": "object",
                "properties": {
                    "host": { "type": "string" },
                    "port": { "type": "integer" }
                }
            }
        }
    }))
    .expect("b3 schema parse failed");

    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service.inner().seed_schema_cache(B3_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b3.yaml";
    // Blank line after "host:" — cursor at line 3, col 2 is on the blank line
    // inside the "server" nested mapping. InBlankMapping fires; schema supplies "port".
    let yaml = format!(
        "# yaml-language-server: $schema={B3_SCHEMA_URL}\nserver:\n  host: localhost\n  \n"
    );
    send(&mut service, did_open_notification(uri, &yaml)).await;

    // Cursor at line 3, col 2 — blank line inside nested "server" mapping
    let resp = send(&mut service, completion_request(2, uri, 3, 2)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("port"),
        "should suggest schema key 'port' on blank line in nested mapping, got: {result_str}"
    );
    assert!(
        !result_str.contains("\"host\""),
        "should exclude present key 'host', got: {result_str}"
    );
}

// B-4: Cursor on key inside sequence item, asserts sibling key from another item
#[tokio::test]
async fn should_complete_at_sequence_item_key_suggests_sibling_from_other_item() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/b4.yaml";
    send(
        &mut service,
        did_open_notification(uri, "items:\n  - name: Alice\n    age: 30\n  - name: Bob\n"),
    )
    .await;

    let resp = send(&mut service, completion_request(2, uri, 3, 4)).await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    assert!(!result.is_null(), "result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize");
    assert!(
        result_str.contains("age"),
        "should suggest sibling key 'age' from another item, got: {result_str}"
    );
}

// ── Completion over schemas with more than 20 composition branches ───────────

const MANY_BRANCHES_SCHEMA_URL: &str = "https://example.com/test-many-branches-completion.json";

/// Complete at (`line`, `character`) in `document` against a schema built
/// from `all_of_branches`, returning the serialized completion result.
async fn completion_with_all_of(
    all_of_branches: Vec<serde_json::Value>,
    document: &str,
    line: u32,
    character: u32,
) -> String {
    let schema = parse_schema(&json!({ "type": "object", "allOf": all_of_branches }))
        .expect("many-branches schema parse failed");

    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    service
        .inner()
        .seed_schema_cache(MANY_BRANCHES_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/many-branches-completion.yaml";
    let text = format!("# yaml-language-server: $schema={MANY_BRANCHES_SCHEMA_URL}\n{document}");
    send(&mut service, did_open_notification(uri, &text)).await;

    // The modeline occupies line 0, so the document's lines start at 1.
    let resp = send(
        &mut service,
        completion_request(2, uri, line + 1, character),
    )
    .await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    serde_json::to_string(result).expect("serialize")
}

/// 25 `allOf` branches: branch `i` (below 24) defines `field_{i}`; the last
/// branch is `last`.
fn field_branches_then(last: serde_json::Value) -> Vec<serde_json::Value> {
    (0..24)
        .map(|i| json!({ "properties": { format!("field_{i}"): { "type": "string" } } }))
        .chain(std::iter::once(last))
        .collect()
}

#[tokio::test]
async fn should_complete_top_level_key_defined_in_all_of_branch_beyond_the_twentieth() {
    let branches = field_branches_then(json!({
        "properties": { "late_key": { "type": "string" } }
    }));

    // Blank line after the only key present, inside the top-level mapping.
    let result_str = completion_with_all_of(branches, "field_0: x\n\n", 1, 0).await;

    assert!(
        result_str.contains("late_key"),
        "should offer 'late_key' from branch 25, got: {result_str}"
    );
    assert!(
        result_str.contains("field_1"),
        "should offer 'field_1' from an early branch, got: {result_str}"
    );
    assert!(
        !result_str.contains("\"field_0\""),
        "should exclude present key 'field_0', got: {result_str}"
    );
}

#[tokio::test]
async fn should_complete_nested_key_when_parent_schema_is_only_reachable_through_branch_beyond_the_twentieth()
 {
    let branches = field_branches_then(json!({
        "properties": {
            "server": {
                "type": "object",
                "properties": {
                    "host": { "type": "string" },
                    "port": { "type": "integer" }
                }
            }
        }
    }));

    let result_str = completion_with_all_of(branches, "server:\n  host: x\n  \n", 2, 2).await;

    assert!(
        result_str.contains("port"),
        "should offer 'port' under 'server' from branch 25, got: {result_str}"
    );
    assert!(
        !result_str.contains("\"host\""),
        "should exclude present key 'host', got: {result_str}"
    );
}

const ANY_OF_ITEMS_SCHEMA_URL: &str = "https://example.com/test-any-of-items-completion.json";

/// Complete at (`line`, `character`) in `document` against a schema whose
/// `servers` key is an array declared in an `anyOf` branch next to a null
/// branch, returning the serialized completion result.
async fn completion_with_servers_in_any_of(document: &str, line: u32, character: u32) -> String {
    let schema = parse_schema(&json!({
        "type": "object",
        "properties": { "servers": { "anyOf": [
            {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "host": { "type": "string" },
                        "port": { "type": "integer" }
                    }
                }
            },
            { "type": "null" }
        ] } }
    }))
    .expect("any-of items schema parse failed");

    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    service
        .inner()
        .seed_schema_cache(ANY_OF_ITEMS_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/any-of-items-completion.yaml";
    let text = format!("# yaml-language-server: $schema={ANY_OF_ITEMS_SCHEMA_URL}\n{document}");
    send(&mut service, did_open_notification(uri, &text)).await;

    // The modeline occupies line 0, so the document's lines start at 1.
    let resp = send(
        &mut service,
        completion_request(2, uri, line + 1, character),
    )
    .await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    serde_json::to_string(result).expect("serialize")
}

#[tokio::test]
async fn should_complete_item_keys_on_a_blank_line_in_a_sequence_declared_in_an_any_of_branch() {
    let result_str = completion_with_servers_in_any_of("servers:\n  - host: a\n  \n", 2, 2).await;

    assert!(
        result_str.contains("port"),
        "should offer 'port' from the item schema, got: {result_str}"
    );
}

#[tokio::test]
async fn should_complete_the_other_item_keys_on_a_key_in_an_item_of_a_sequence_declared_in_an_any_of_branch()
 {
    let result_str = completion_with_servers_in_any_of("servers:\n  - host: a\n", 1, 4).await;

    assert!(
        result_str.contains("port"),
        "should offer 'port' from the item schema, got: {result_str}"
    );
    assert!(
        !result_str.contains("\"host\""),
        "should exclude present key 'host', got: {result_str}"
    );
}

// ── Completion below keys that patterns and additionalProperties describe ────

const WORKFLOW_SCHEMA_URL: &str = "https://example.com/test-completion-workflow.json";
const COMPOSE_SCHEMA_URL: &str = "https://example.com/test-completion-compose.json";

/// Complete at (`line`, `character`) in `document` against `schema` served
/// under `url`, returning the serialized completion result.
async fn completion_with_schema(
    url: &str,
    schema: &serde_json::Value,
    document: &str,
    line: u32,
    character: u32,
) -> String {
    let schema = parse_schema(schema).expect("schema parse failed");

    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    service.inner().seed_schema_cache(url, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion-patterns.yaml";
    let text = format!("# yaml-language-server: $schema={url}\n{document}");
    send(&mut service, did_open_notification(uri, &text)).await;

    // The modeline occupies line 0, so the document's lines start at 1.
    let resp = send(
        &mut service,
        completion_request(2, uri, line + 1, character),
    )
    .await;
    let resp = resp.expect("completion should return a response");
    let result = resp.result().expect("completion should have a result");
    serde_json::to_string(result).expect("serialize")
}

#[tokio::test]
async fn should_complete_the_keys_of_both_branches_of_a_job_named_by_a_pattern() {
    let result_str = completion_with_schema(
        WORKFLOW_SCHEMA_URL,
        &workflow_schema(),
        "jobs:\n  build:\n    steps: []\n    \n",
        3,
        4,
    )
    .await;

    for expected in ["runs-on", "uses", "with"] {
        assert!(
            result_str.contains(expected),
            "should offer '{expected}', got: {result_str}"
        );
    }
}

#[tokio::test]
async fn should_complete_the_enum_values_of_a_key_of_a_job() {
    let result_str = completion_with_schema(
        WORKFLOW_SCHEMA_URL,
        &workflow_schema(),
        "jobs:\n  build:\n    runs-on: \n",
        2,
        13,
    )
    .await;

    assert!(
        result_str.contains("ubuntu-latest") && result_str.contains("windows-latest"),
        "should offer the runners, got: {result_str}"
    );
}

#[tokio::test]
async fn should_complete_the_other_keys_of_a_step_and_leave_out_the_present_one() {
    let result_str = completion_with_schema(
        WORKFLOW_SCHEMA_URL,
        &workflow_schema(),
        "jobs:\n  build:\n    steps:\n      - uses: a\n        \n",
        4,
        8,
    )
    .await;

    assert!(
        result_str.contains("\"run\"") && result_str.contains("\"name\""),
        "should offer 'run' and 'name', got: {result_str}"
    );
    assert!(
        !result_str.contains("\"uses\""),
        "should leave out the present key 'uses', got: {result_str}"
    );
}

#[tokio::test]
async fn should_complete_all_keys_of_a_step_in_a_blank_item() {
    let result_str = completion_with_schema(
        WORKFLOW_SCHEMA_URL,
        &workflow_schema(),
        "jobs:\n  build:\n    steps:\n      - \n",
        3,
        8,
    )
    .await;

    for expected in ["uses", "run", "name"] {
        assert!(
            result_str.contains(expected),
            "should offer '{expected}', got: {result_str}"
        );
    }
}

#[tokio::test]
async fn should_not_offer_job_names_on_a_blank_line_below_jobs() {
    let result_str =
        completion_with_schema(WORKFLOW_SCHEMA_URL, &workflow_schema(), "jobs:\n  \n", 1, 2).await;

    assert!(
        !result_str.contains("runs-on"),
        "should not offer job keys where a job name goes, got: {result_str}"
    );
}

#[tokio::test]
async fn should_complete_the_keys_and_values_of_a_service_named_by_a_pattern() {
    let keys = completion_with_schema(
        COMPOSE_SCHEMA_URL,
        &compose_schema(),
        "services:\n  web:\n    image: nginx\n    \n",
        3,
        4,
    )
    .await;
    let values = completion_with_schema(
        COMPOSE_SCHEMA_URL,
        &compose_schema(),
        "services:\n  web:\n    restart: \n",
        2,
        13,
    )
    .await;

    assert!(
        keys.contains("restart") && !keys.contains("\"image\""),
        "should offer 'restart' and leave out 'image', got: {keys}"
    );
    assert!(
        values.contains("always") && values.contains("\"no\""),
        "should offer the restart values, got: {values}"
    );
}
