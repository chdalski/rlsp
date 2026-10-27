// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::schema::parse_schema;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::helpers::*;

pub fn hover_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
    Request::build("textDocument/hover")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_hover_response_for_valid_position() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have result");
    // result should not be null
    assert!(!result.is_null(), "hover result should not be null");
    let result_str = serde_json::to_string(result).expect("serialize result");
    assert!(
        result_str.contains("key"),
        "hover content should contain 'key'"
    );
}

#[tokio::test]
async fn should_return_null_hover_for_whitespace_position() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n\n")).await;

    let resp = send(&mut service, hover_request(2, uri, 1, 0)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have result");
    assert!(
        result.is_null(),
        "hover result should be null for whitespace"
    );
}

#[tokio::test]
async fn should_return_none_for_hover_on_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, hover_request(2, uri, 0, 0)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have result");
    assert!(
        result.is_null(),
        "hover result should be null for unknown document"
    );
}

// ── Hover schema docs through the shared resolver ────────────────────────────

const HOVER_SCHEMA_URL: &str = "https://example.com/test-hover-docs.json";

/// Hover at (`line`, `character`) in `document` against a schema that documents
/// an array item's key, a key behind nested composition, a key reached only
/// through a later branch, a key with dots, and arrays declared in an `anyOf`
/// branch.
/// Returns the serialized hover result.
async fn hover_with_docs_schema(document: &str, line: u32, character: u32) -> String {
    let schema = parse_schema(&json!({
        "type": "object",
        "allOf": [
            { "properties": { "server": { "properties": {
                "host": { "description": "Host docs" }
            } } } },
            { "properties": { "server": { "properties": {
                "port": { "description": "Port docs" }
            } } } }
        ],
        "properties": {
            "containers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "image": { "description": "Container image docs" } }
                }
            },
            "spec": {
                "allOf": [{
                    "anyOf": [{
                        "properties": { "replicas": { "description": "Replica count docs" } }
                    }]
                }]
            },
            "labels": {
                "properties": {
                    "app.kubernetes.io/name": { "description": "App name label docs" }
                }
            },
            "sidecars": {
                "anyOf": [
                    {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": { "name": { "description": "Sidecar name docs" } }
                        }
                    },
                    { "type": "null" }
                ]
            },
            "ports": {
                "anyOf": [
                    { "type": "array", "items": { "description": "Port item docs" } },
                    { "type": "null" }
                ]
            }
        }
    }))
    .expect("hover docs schema parse failed");

    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    service.inner().seed_schema_cache(HOVER_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover-docs.yaml";
    let text = format!("# yaml-language-server: $schema={HOVER_SCHEMA_URL}\n{document}");
    send(&mut service, did_open_notification(uri, &text)).await;

    // The modeline occupies line 0, so the document's lines start at 1.
    let resp = send(&mut service, hover_request(2, uri, line + 1, character)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have a result");
    serde_json::to_string(result).expect("serialize")
}

#[tokio::test]
async fn should_show_schema_docs_for_a_key_below_an_array_item() {
    let result_str = hover_with_docs_schema("containers:\n  - image: nginx\n", 1, 4).await;

    assert!(
        result_str.contains("Container image docs"),
        "hover should show the docs of 'image', got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_for_a_key_reached_through_nested_composition() {
    let result_str = hover_with_docs_schema("spec:\n  replicas: 3\n", 1, 2).await;

    assert!(
        result_str.contains("Replica count docs"),
        "hover should show the docs of 'replicas', got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_for_a_key_containing_dots() {
    let result_str = hover_with_docs_schema("labels:\n  app.kubernetes.io/name: web\n", 1, 2).await;

    assert!(
        result_str.contains("App name label docs"),
        "hover should show the docs of 'app.kubernetes.io/name', got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_when_the_first_matching_branch_dead_ends() {
    let result_str = hover_with_docs_schema("server:\n  port: 1\n", 1, 2).await;

    assert!(
        result_str.contains("Port docs"),
        "hover should show the docs of 'port' from the later branch, got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_for_a_key_below_an_item_of_an_array_declared_in_an_any_of_branch()
{
    let result_str = hover_with_docs_schema("sidecars:\n  - name: x\n", 1, 4).await;

    assert!(
        result_str.contains("Sidecar name docs"),
        "hover should show the docs of 'name', got: {result_str}"
    );
}

#[tokio::test]
async fn should_show_schema_docs_for_an_item_of_an_array_declared_in_an_any_of_branch() {
    let result_str = hover_with_docs_schema("ports:\n  - 8080\n", 1, 4).await;

    assert!(
        result_str.contains("Port item docs"),
        "hover should show the docs of the item, got: {result_str}"
    );
}

// ── Hover schema docs below keys that patterns and additionalProperties describe ──

const WORKFLOW_SCHEMA_URL: &str = "https://example.com/test-hover-workflow.json";
const COMPOSE_SCHEMA_URL: &str = "https://example.com/test-hover-compose.json";

/// Hover at (`line`, `character`) in `document` against `schema` served under
/// `url`. Returns the serialized hover result.
async fn hover_with_schema(
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

    let uri = "file:///test/hover-patterns.yaml";
    let text = format!("# yaml-language-server: $schema={url}\n{document}");
    send(&mut service, did_open_notification(uri, &text)).await;

    // The modeline occupies line 0, so the document's lines start at 1.
    let resp = send(&mut service, hover_request(2, uri, line + 1, character)).await;
    let resp = resp.expect("hover should return a response");
    let result = resp.result().expect("hover should have a result");
    serde_json::to_string(result).expect("serialize")
}

#[tokio::test]
async fn should_show_the_docs_of_a_key_below_a_job_named_by_a_pattern() {
    let result_str = hover_with_schema(
        WORKFLOW_SCHEMA_URL,
        &workflow_schema(),
        "jobs:\n  build:\n    runs-on: ubuntu-latest\n",
        2,
        4,
    )
    .await;

    assert!(result_str.contains("Runner docs"), "got: {result_str}");
}

#[tokio::test]
async fn should_show_the_docs_of_the_job_name_itself() {
    let result_str = hover_with_schema(
        WORKFLOW_SCHEMA_URL,
        &workflow_schema(),
        "jobs:\n  build:\n    runs-on: ubuntu-latest\n",
        1,
        2,
    )
    .await;

    assert!(result_str.contains("Job docs"), "got: {result_str}");
}

#[tokio::test]
async fn should_show_the_docs_of_a_key_in_the_second_branch_of_a_job() {
    let result_str = hover_with_schema(
        WORKFLOW_SCHEMA_URL,
        &workflow_schema(),
        "jobs:\n  deploy:\n    uses: ./reusable.yaml\n",
        2,
        4,
    )
    .await;

    assert!(result_str.contains("Reusable docs"), "got: {result_str}");
}

#[tokio::test]
async fn should_show_the_docs_of_the_keys_of_each_step_of_a_job() {
    let document = "jobs:\n  build:\n    steps:\n      - uses: a\n      - run: b\n";

    let first = hover_with_schema(WORKFLOW_SCHEMA_URL, &workflow_schema(), document, 3, 8).await;
    let second = hover_with_schema(WORKFLOW_SCHEMA_URL, &workflow_schema(), document, 4, 8).await;

    assert!(first.contains("Step uses docs"), "got: {first}");
    assert!(second.contains("Step run docs"), "got: {second}");
}

#[tokio::test]
async fn should_show_no_docs_below_a_job_name_the_pattern_does_not_match() {
    let document = "jobs:\n  1bad:\n    runs-on: ubuntu-latest\n";

    let name = hover_with_schema(WORKFLOW_SCHEMA_URL, &workflow_schema(), document, 1, 2).await;
    let below = hover_with_schema(WORKFLOW_SCHEMA_URL, &workflow_schema(), document, 2, 4).await;

    assert!(!name.contains("Job docs"), "got: {name}");
    assert!(!below.contains("Runner docs"), "got: {below}");
}

#[tokio::test]
async fn should_show_the_docs_of_a_service_and_of_its_keys() {
    let document = "services:\n  web:\n    image: nginx\n";

    let service = hover_with_schema(COMPOSE_SCHEMA_URL, &compose_schema(), document, 1, 2).await;
    let image = hover_with_schema(COMPOSE_SCHEMA_URL, &compose_schema(), document, 2, 4).await;

    assert!(service.contains("Service docs"), "got: {service}");
    assert!(image.contains("Image docs"), "got: {image}");
}

#[tokio::test]
async fn should_show_the_docs_of_an_extension_key_next_to_additional_properties_false() {
    let known = hover_with_schema(COMPOSE_SCHEMA_URL, &compose_schema(), "x-ext: 1\n", 0, 0).await;
    let unknown =
        hover_with_schema(COMPOSE_SCHEMA_URL, &compose_schema(), "unknown: 1\n", 0, 0).await;

    assert!(known.contains("Extension docs"), "got: {known}");
    assert!(!unknown.contains("Extension docs"), "got: {unknown}");
}

#[tokio::test]
async fn should_show_no_docs_below_a_service_name_the_pattern_does_not_match() {
    let document = "services:\n  bad name:\n    image: nginx\n";

    let image = hover_with_schema(COMPOSE_SCHEMA_URL, &compose_schema(), document, 2, 4).await;

    assert!(!image.contains("Image docs"), "got: {image}");
}

// ── Hover schema docs of array items described by their position ──

const TUPLE_SCHEMA_URL: &str = "https://example.com/test-hover-tuple.json";

#[tokio::test]
async fn should_show_the_docs_of_the_second_item_of_a_prefix_items_sequence() {
    let schema = serde_json::json!({
        "properties": {
            "pair": {
                "type": "array",
                "prefixItems": [
                    { "description": "First item docs" },
                    { "description": "Second item docs" }
                ]
            }
        }
    });

    let result_str =
        hover_with_schema(TUPLE_SCHEMA_URL, &schema, "pair:\n  - a\n  - b\n", 2, 4).await;

    assert!(result_str.contains("Second item docs"), "got: {result_str}");
    assert!(!result_str.contains("First item docs"), "got: {result_str}");
}

#[tokio::test]
async fn should_show_the_docs_of_an_item_beyond_a_draft_07_tuple() {
    let schema = serde_json::json!({
        "properties": {
            "row": {
                "type": "array",
                "items": [
                    { "description": "First item docs" },
                    { "description": "Second item docs" }
                ],
                "additionalItems": { "description": "Further item docs" }
            }
        }
    });

    let result_str = hover_with_schema(
        TUPLE_SCHEMA_URL,
        &schema,
        "row:\n  - a\n  - b\n  - c\n",
        3,
        4,
    )
    .await;

    assert!(
        result_str.contains("Further item docs"),
        "got: {result_str}"
    );
}
