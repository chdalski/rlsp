// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::code_lens::code_lens_request;
use super::helpers::*;

const GITHUB_WORKFLOW_SCHEMA_URL: &str = "https://json.schemastore.org/github-workflow.json";

pub fn initialize_request_with_schema_glob(id: i64, schema_url: &str, glob: &str) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": {
                "schemas": { schema_url: glob }
            }
        }))
        .finish()
}

// ---- $schema=none modeline ----

#[tokio::test]
async fn should_produce_no_diagnostics_for_valid_yaml_with_schema_none_modeline() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // $schema=none should disable schema processing; the non-schema validators
    // (anchors, flow style, key ordering, duplicate keys) still run, but valid
    // YAML should produce no diagnostics.
    let uri = "file:///test/schema-none.yaml";
    send(
        &mut service,
        did_open_notification(uri, "# yaml-language-server: $schema=none\nkey: value\n"),
    )
    .await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(
        diags.is_empty(),
        "valid YAML with $schema=none should have no diagnostics, got: {diags:?}"
    );
}

#[tokio::test]
async fn should_suppress_code_lens_after_schema_none_modeline() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-none-lens.yaml";
    // First open with a real schema modeline so an association is stored.
    send(
        &mut service,
        did_open_notification(
            uri,
            "# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json\nkey: value\n",
        ),
    )
    .await;

    // Verify lens is returned when association exists
    {
        let resp = send(&mut service, code_lens_request(2, uri)).await;
        let result = resp
            .expect("codeLens should return a response")
            .result()
            .expect("codeLens should have result")
            .clone();
        assert!(
            !result.is_null(),
            "codeLens should be present before $schema=none"
        );
    }

    // Now change to $schema=none, which should clear the association
    send(
        &mut service,
        did_change_notification(uri, "# yaml-language-server: $schema=none\nkey: value\n", 2),
    )
    .await;

    let resp = send(&mut service, code_lens_request(3, uri)).await;
    let result = resp
        .expect("codeLens should return a response")
        .result()
        .expect("codeLens should have result")
        .clone();
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "codeLens should be null after $schema=none clears the association"
    );
}

// ---- glob-based schema association fallback ----

#[tokio::test]
async fn should_attempt_schema_validation_via_glob_association() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    // Configure a schema glob that matches *.yaml; the fetch will fail (no
    // real network) but the path through match_schema_by_filename is exercised.
    send(
        &mut service,
        initialize_request_with_schema_glob(1, "https://example.com/schema.json", "*.yaml"),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/glob-doc.yaml";
    let resp = send(&mut service, did_open_notification(uri, "key: value\n")).await;
    assert!(
        resp.is_none(),
        "didOpen notification should not return a response"
    );

    // Document should be stored; schema fetch will fail silently, yielding no
    // schema-validation diagnostics but the code path is exercised.
    let text = service.inner().get_document_text(uri);
    assert_eq!(
        text.as_deref(),
        Some("key: value\n"),
        "document should be stored even when schema fetch fails"
    );
}

// ---- hover with schema association via modeline ----

#[tokio::test]
async fn should_return_hover_when_schema_modeline_is_present() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    let stub = serde_json::json!({"type": "object"});
    let schema = rlsp_yaml::schema::parse_schema(&stub).expect("stub schema");
    service
        .inner()
        .seed_schema_cache(GITHUB_WORKFLOW_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/hover-schema.yaml";
    // Pre-seeded schema cache means process_schema hits the cache and skips the
    // HTTP fetch entirely — no network required.
    send(
        &mut service,
        did_open_notification(
            uri,
            &format!("# yaml-language-server: $schema={GITHUB_WORKFLOW_SCHEMA_URL}\nname: Alice\n"),
        ),
    )
    .await;

    let hover_req = Request::build("textDocument/hover")
        .id(2)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 0 }
        }))
        .finish();

    let resp = send(&mut service, hover_req).await;
    let resp = resp.expect("hover should return a response");
    // Result may or may not be null depending on hover logic, but the
    // schema association lookup path in hover() is exercised regardless.
    assert!(
        resp.result().is_some(),
        "hover response should have a result field (even if null)"
    );
}

// ---- completion with schema association via modeline ----

#[tokio::test]
async fn should_exercise_schema_lookup_in_completion() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    let stub = serde_json::json!({"type": "object"});
    let schema = rlsp_yaml::schema::parse_schema(&stub).expect("stub schema");
    service
        .inner()
        .seed_schema_cache(GITHUB_WORKFLOW_SCHEMA_URL, schema);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/completion-schema.yaml";
    // Pre-seeded schema cache means process_schema hits the cache and skips the
    // HTTP fetch entirely — no network required.
    send(
        &mut service,
        did_open_notification(
            uri,
            &format!("# yaml-language-server: $schema={GITHUB_WORKFLOW_SCHEMA_URL}\nname: Alice\nage: 30\n"),
        ),
    )
    .await;

    let completion_req = Request::build("textDocument/completion")
        .id(2)
        .params(json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 0 }
        }))
        .finish();

    let resp = send(&mut service, completion_req).await;
    let resp = resp.expect("completion should return a response");
    // Result may be null or have items; the important thing is the schema
    // association lookup path in completion() is exercised.
    assert!(
        resp.result().is_some(),
        "completion response should have a result field"
    );
}

// ---- composition branches beyond the twentieth ----
//
// Validation once looked at only the first 20 `allOf`/`anyOf`/`oneOf` branches
// of a schema node. These tests seed a 25-branch schema and open a document
// through the full LSP pipeline.

const MANY_BRANCHES_SCHEMA_URL: &str = "https://example.com/many-branches.json";

/// A schema whose `keyword` holds 25 branches, branch `i` requiring `field_i`.
fn many_branches_schema(keyword: &str) -> rlsp_yaml::schema::ParsedSchema {
    let branches: Vec<_> = (0..25)
        .map(|i| json!({ "required": [format!("field_{i}")] }))
        .collect();
    rlsp_yaml::schema::parse_schema(&json!({ keyword: branches }))
        .expect("many_branches_schema: parse failed")
}

async fn diagnostic_messages(keyword: &str, document: &str) -> Vec<String> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    service
        .inner()
        .seed_schema_cache(MANY_BRANCHES_SCHEMA_URL, many_branches_schema(keyword));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/many-branches.yaml";
    let text = format!("# yaml-language-server: $schema={MANY_BRANCHES_SCHEMA_URL}\n{document}");
    send(&mut service, did_open_notification(uri, &text)).await;

    service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist")
        .into_iter()
        .map(|d| d.message)
        .collect()
}

#[tokio::test]
async fn should_report_diagnostic_from_all_of_branch_beyond_the_twentieth() {
    // field_0..field_23 are present; only branch 25 (field_24) is unsatisfied.
    let document = (0..24)
        .map(|i| format!("field_{i}: x\n"))
        .collect::<Vec<_>>()
        .concat();

    let messages = diagnostic_messages("allOf", &document).await;

    assert!(
        messages
            .iter()
            .any(|m| m.contains("missing required property 'field_24'")),
        "expected the 25th allOf branch to report field_24; got: {messages:?}"
    );
}

#[tokio::test]
async fn should_accept_document_matching_only_the_twenty_fifth_any_of_branch() {
    let messages = diagnostic_messages("anyOf", "field_24: x\n").await;

    assert!(
        !messages.iter().any(|m| m.contains("anyOf")),
        "the 25th anyOf branch matches; got: {messages:?}"
    );
}

#[tokio::test]
async fn should_accept_document_matching_only_the_twenty_fifth_one_of_branch() {
    let messages = diagnostic_messages("oneOf", "field_24: x\n").await;

    assert!(
        !messages.iter().any(|m| m.contains("oneOf")),
        "the 25th oneOf branch matches; got: {messages:?}"
    );
}
