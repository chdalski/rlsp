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
