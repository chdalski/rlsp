// SPDX-License-Identifier: MIT
#![expect(clippy::expect_used, missing_docs, reason = "test code")]

mod code_actions;
mod code_lens;
mod completion;
mod configuration;
mod custom_tags;
mod document_links;
mod document_management;
mod folding_ranges;
mod helpers;
mod hover;
mod kubernetes_detection;
mod lifecycle;
mod navigation;
mod on_type_formatting;
mod rename;
mod schema_modelines;
mod selection_ranges;
mod semantic_tokens;
mod validators_integration;
mod watched_files;

use futures::StreamExt;
use helpers::*;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

use code_actions::code_action_request;
use completion::completion_request;
use configuration::did_change_configuration_notification;
use on_type_formatting::on_type_formatting_request;

// ---- flowStyle setting ----

#[tokio::test]
async fn flow_style_off_suppresses_flow_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable flowStyle: "off"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "flowStyle": "off" })),
    )
    .await;

    let uri = "file:///test/flow-off.yaml";
    // This YAML uses a flow map — normally produces a flowStyle diagnostic.
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_flow_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c)) if c == "flowMap" || c == "flowSeq"
        )
    });
    assert!(
        !has_flow_diag,
        "flowStyle=off should suppress flow diagnostics; got: {diags:?}"
    );
}

#[tokio::test]
async fn flow_style_default_emits_warning_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No explicit flowStyle setting — default is "warning".
    let uri = "file:///test/flow-warning.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let flow_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "flowMap" || c == "flowSeq"
            )
        })
        .collect();

    assert!(
        !flow_diags.is_empty(),
        "flowStyle default should produce flow diagnostics; got: {diags:?}"
    );
    assert!(
        flow_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "flowStyle default should emit WARNING severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn flow_style_error_emits_error_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable flowStyle: "error"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "flowStyle": "error" })),
    )
    .await;

    let uri = "file:///test/flow-error.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let flow_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "flowMap" || c == "flowSeq"
            )
        })
        .collect();

    assert!(
        !flow_diags.is_empty(),
        "flowStyle=error should produce flow diagnostics; got: {diags:?}"
    );
    assert!(
        flow_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "flowStyle=error should emit ERROR severity; got: {diags:?}"
    );
}

// ---- duplicateKeys setting ----

#[tokio::test]
async fn duplicate_keys_default_emits_error_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No explicit duplicateKeys setting — default is "error".
    let uri = "file:///test/dup-default.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let dup_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "duplicateKey"
            )
        })
        .collect();

    assert!(
        !dup_diags.is_empty(),
        "duplicateKeys default should produce duplicate-key diagnostics; got: {diags:?}"
    );
    assert!(
        dup_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "duplicateKeys default should emit ERROR severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn duplicate_keys_error_emits_error_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Explicit duplicateKeys: "error"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "duplicateKeys": "error" })),
    )
    .await;

    let uri = "file:///test/dup-error.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let dup_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "duplicateKey"
            )
        })
        .collect();

    assert!(
        !dup_diags.is_empty(),
        "duplicateKeys=error should produce duplicate-key diagnostics; got: {diags:?}"
    );
    assert!(
        dup_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "duplicateKeys=error should emit ERROR severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn duplicate_keys_warning_emits_warning_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable duplicateKeys: "warning"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "duplicateKeys": "warning" })),
    )
    .await;

    let uri = "file:///test/dup-warning.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let dup_diags: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(NumberOrString::String(c)) if c == "duplicateKey"
            )
        })
        .collect();

    assert!(
        !dup_diags.is_empty(),
        "duplicateKeys=warning should produce duplicate-key diagnostics; got: {diags:?}"
    );
    assert!(
        dup_diags
            .iter()
            .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "duplicateKeys=warning should emit WARNING severity; got: {diags:?}"
    );
}

#[tokio::test]
async fn duplicate_keys_off_suppresses_duplicate_key_diagnostics() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable duplicateKeys: "off"
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "duplicateKeys": "off" })),
    )
    .await;

    let uri = "file:///test/dup-off.yaml";
    send(&mut service, did_open_notification(uri, "key: a\nkey: b\n")).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");

    let has_dup_diag = diags.iter().any(|d| {
        matches!(
            d.code.as_ref(),
            Some(NumberOrString::String(c)) if c == "duplicateKey"
        )
    });
    assert!(
        !has_dup_diag,
        "duplicateKeys=off should suppress duplicate-key diagnostics; got: {diags:?}"
    );
}

// ---- formatEnforceBlockStyle setting ----

fn formatting_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/formatting")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "options": {
                "tabSize": 2,
                "insertSpaces": true
            }
        }))
        .finish()
}

#[tokio::test]
async fn format_enforce_block_style_converts_flow_to_block() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Enable formatEnforceBlockStyle
    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatEnforceBlockStyle": true })),
    )
    .await;

    let uri = "file:///test/enforce-block.yaml";
    let flow_yaml = "key: {a: 1, b: 2}\n";
    send(&mut service, did_open_notification(uri, flow_yaml)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    assert!(
        !result.is_null(),
        "formatEnforceBlockStyle=true should produce a formatting edit for flow YAML; got null"
    );

    let edits = result
        .as_array()
        .expect("formatting result should be array");
    assert!(
        !edits.is_empty(),
        "formatEnforceBlockStyle=true should produce at least one edit"
    );

    // The formatted text should not contain flow-style braces.
    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");
    assert!(
        !new_text.contains('{'),
        "formatEnforceBlockStyle=true should remove flow maps; got: {new_text:?}"
    );
}

#[tokio::test]
async fn format_enforce_block_style_off_by_default_preserves_flow() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No formatEnforceBlockStyle setting — default is false.
    let uri = "file:///test/no-enforce-block.yaml";
    // This YAML is already well-formatted so the formatter won't change it.
    // Use a simple flow map that the formatter would normally leave as-is.
    let flow_yaml = "key:\n  a: 1\n  b: 2\n";
    send(&mut service, did_open_notification(uri, flow_yaml)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    // Well-formatted block YAML with no changes expected — formatter returns null or empty.
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "well-formatted block YAML should produce no edits when formatEnforceBlockStyle is false; got: {result:?}"
    );
}

// ---- formatPreserveQuotes setting ----

const KUBERNETES_YAML: &str = "\
apiVersion: v1
kind: Namespace
metadata:
  name: finance
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: payment-api
  namespace: finance
spec:
  replicas: 2
  selector:
    matchLabels:
      app: payment
  template:
    metadata:
      labels:
        app: payment
    spec:
      containers:
      - name: payment
        image: python:3.12-slim
        command: [\"python\", \"-m\", \"http.server\", \"5000\"]
        ports:
        - containerPort: 5000
        readinessProbe:
          httpGet:
            path: /
            port: 5000
          initialDelaySeconds: 5
          periodSeconds: 5
";

#[tokio::test]
async fn preserve_quotes_retains_double_quoted_command_array() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatPreserveQuotes": true })),
    )
    .await;

    let uri = "file:///test/k8s-preserve-command.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    assert!(
        !result.is_null(),
        "formatPreserveQuotes=true should produce a formatting edit; got null"
    );
    let edits = result.as_array().expect("result should be array");
    assert!(
        !edits.is_empty(),
        "formatting should produce at least one edit"
    );

    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");

    assert!(
        new_text.contains(r#"["python", "-m", "http.server", "5000"]"#),
        "all four command elements must remain double-quoted; got: {new_text:?}"
    );
}

#[tokio::test]
async fn preserve_quotes_plain_scalars_remain_plain() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatPreserveQuotes": true })),
    )
    .await;

    let uri = "file:///test/k8s-preserve-plain.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    let new_text = if result.is_null() || result.as_array().is_some_and(Vec::is_empty) {
        KUBERNETES_YAML.to_owned()
    } else {
        let edits = result.as_array().expect("result should be array");
        edits[0]["newText"]
            .as_str()
            .expect("newText should be a string")
            .to_owned()
    };

    for plain in ["payment-api", "finance", "python:3.12-slim"] {
        assert!(
            new_text.contains(plain),
            "plain scalar {plain:?} should appear unquoted in output; got: {new_text:?}"
        );
        assert!(
            !new_text.contains(&format!("\"{plain}\"")),
            "plain scalar {plain:?} must not be double-quoted in output; got: {new_text:?}"
        );
        assert!(
            !new_text.contains(&format!("'{plain}'")),
            "plain scalar {plain:?} must not be single-quoted in output; got: {new_text:?}"
        );
    }
}

#[tokio::test]
async fn preserve_quotes_idempotent_on_second_format() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatPreserveQuotes": true })),
    )
    .await;

    let uri = "file:///test/k8s-preserve-idempotent.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    // First format pass
    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("first formatting should return a response");
    let result = resp
        .result()
        .expect("first formatting should have a result");

    let first_output = if result.is_null() || result.as_array().is_some_and(Vec::is_empty) {
        KUBERNETES_YAML.to_owned()
    } else {
        let edits = result.as_array().expect("result should be array");
        edits[0]["newText"]
            .as_str()
            .expect("newText should be a string")
            .to_owned()
    };

    // Update document to first_output, then format again
    send(&mut service, did_change_notification(uri, &first_output, 2)).await;

    let resp2 = send(&mut service, formatting_request(3, uri)).await;
    let resp2 = resp2.expect("second formatting should return a response");
    let result2 = resp2
        .result()
        .expect("second formatting should have a result");

    assert!(
        result2.is_null() || result2.as_array().is_some_and(Vec::is_empty),
        "second format pass should produce no edits (idempotent); got: {result2:?}"
    );
}

#[tokio::test]
async fn preserve_quotes_off_by_default_strips_safe_plain_scalars() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No formatPreserveQuotes — defaults to false
    let uri = "file:///test/k8s-default-strip.yaml";
    send(&mut service, did_open_notification(uri, KUBERNETES_YAML)).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");

    assert!(
        !result.is_null(),
        "default formatting should produce edits for the flow command array; got null"
    );
    let edits = result.as_array().expect("result should be array");
    assert!(
        !edits.is_empty(),
        "default formatting should produce at least one edit"
    );

    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");

    // Safe-plain scalars must have quotes stripped
    assert!(
        !new_text.contains("\"python\""),
        "default mode must strip quotes from safe-plain 'python'; got: {new_text:?}"
    );
    assert!(
        !new_text.contains("\"http.server\""),
        "default mode must strip quotes from safe-plain 'http.server'; got: {new_text:?}"
    );

    // Scalars that require quoting must stay quoted
    assert!(
        new_text.contains("\"-m\""),
        "default mode must keep quotes on '-m' (reserved leading dash); got: {new_text:?}"
    );
    assert!(
        new_text.contains("\"5000\""),
        "default mode must keep quotes on '5000' (looks like integer); got: {new_text:?}"
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
    let schema = rlsp_yaml::schema::parse_schema(&serde_json::json!({
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

// ---- formatEnable setting ----

fn range_formatting_request(id: i64, uri: &str, start_line: u32, end_line: u32) -> Request {
    Request::build("textDocument/rangeFormatting")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": start_line, "character": 0 },
                "end": { "line": end_line, "character": 0 }
            },
            "options": { "tabSize": 2, "insertSpaces": true }
        }))
        .finish()
}

#[tokio::test]
async fn format_enable_false_formatting_returns_null() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({
            "formatEnable": false,
            "formatEnforceBlockStyle": true
        })),
    )
    .await;

    let uri = "file:///test/fmt-enable-false.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");
    assert!(
        result.is_null(),
        "formatting should return null when formatEnable is false; got: {result:?}"
    );
}

#[tokio::test]
async fn format_enable_false_range_formatting_returns_null() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({
            "formatEnable": false,
            "formatEnforceBlockStyle": true
        })),
    )
    .await;

    let uri = "file:///test/range-fmt-enable-false.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let resp = send(&mut service, range_formatting_request(2, uri, 0, 1)).await;
    let resp = resp.expect("rangeFormatting should return a response");
    let result = resp.result().expect("rangeFormatting should have a result");
    assert!(
        result.is_null(),
        "rangeFormatting should return null when formatEnable is false; got: {result:?}"
    );
}

#[tokio::test]
async fn format_enable_false_on_type_formatting_returns_null() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatEnable": false })),
    )
    .await;

    let uri = "file:///test/on-type-fmt-enable-false.yaml";
    send(&mut service, did_open_notification(uri, "server:\n\n")).await;

    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp
        .result()
        .expect("onTypeFormatting should have a result");
    assert!(
        result.is_null(),
        "onTypeFormatting should return null when formatEnable is false; got: {result:?}"
    );
}

#[tokio::test]
async fn format_enable_explicit_true_formatting_returns_edits() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({
            "formatEnable": true,
            "formatEnforceBlockStyle": true
        })),
    )
    .await;

    let uri = "file:///test/fmt-enable-true.yaml";
    send(&mut service, did_open_notification(uri, "key: {a: 1}\n")).await;

    let resp = send(&mut service, formatting_request(2, uri)).await;
    let resp = resp.expect("formatting should return a response");
    let result = resp.result().expect("formatting should have a result");
    assert!(
        !result.is_null(),
        "formatting should return edits when formatEnable is true; got null"
    );
    let edits = result
        .as_array()
        .expect("formatting result should be an array");
    assert!(
        !edits.is_empty(),
        "formatting should return at least one edit when formatEnable is true"
    );
}

#[tokio::test]
async fn format_enable_default_on_type_formatting_works() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // No formatEnable setting — default is true; existing behavior must be preserved.
    let uri = "file:///test/on-type-fmt-default.yaml";
    send(&mut service, did_open_notification(uri, "server:\n\n")).await;

    let resp = send(&mut service, on_type_formatting_request(2, uri, 1, 0, "\n")).await;
    let resp = resp.expect("onTypeFormatting should return a response");
    let result = resp
        .result()
        .expect("onTypeFormatting should have a result");
    assert!(
        !result.is_null(),
        "onTypeFormatting should work when formatEnable is absent (default true); got null"
    );
}

#[tokio::test]
async fn format_enable_false_does_not_gate_code_action() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "formatEnable": false })),
    )
    .await;

    let uri = "file:///test/code-action-fmt-disabled.yaml";
    send(
        &mut service,
        did_open_notification(uri, "config: {key: value}\n"),
    )
    .await;

    let resp = send(&mut service, code_action_request(2, uri, 0, 1)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    assert!(
        !result.is_null(),
        "codeAction should not be gated by formatEnable; got null"
    );
    let actions = result
        .as_array()
        .expect("codeAction result should be an array");
    assert!(
        !actions.is_empty(),
        "codeAction should return at least one action when formatEnable is false"
    );
}
