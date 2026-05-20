// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::code_lens::code_lens_request;
use super::helpers::*;

pub fn initialize_request_with_k8s_version(id: i64, version: &str) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": { "kubernetesVersion": version }
        }))
        .finish()
}

#[tokio::test]
async fn should_record_schema_association_for_kubernetes_manifest() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // A Kubernetes manifest without a modeline or glob should trigger
    // auto-detection. The schema fetch will fail (no network in tests)
    // but the association is recorded before the fetch.
    let uri = "file:///test/pod.yaml";
    send(
        &mut service,
        did_open_notification(uri, "apiVersion: v1\nkind: Pod\nmetadata:\n  name: test\n"),
    )
    .await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    let arr = result.as_array().expect("codeLens result should be array");
    assert!(
        !arr.is_empty(),
        "codeLens should return a lens for the K8s schema"
    );
    let lens_str = serde_json::to_string(&arr[0]).expect("serialize lens");
    assert!(
        lens_str.contains("kubernetes-json-schema"),
        "lens should reference the kubernetes-json-schema repository"
    );
    assert!(
        lens_str.contains("pod-v1.json"),
        "lens should reference pod-v1.json"
    );
}

#[tokio::test]
async fn should_use_configured_kubernetes_version_in_schema_url() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(
        &mut service,
        initialize_request_with_k8s_version(1, "1.29.0"),
    )
    .await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/deployment.yaml";
    send(
        &mut service,
        did_open_notification(
            uri,
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: test\n",
        ),
    )
    .await;

    let resp = send(&mut service, code_lens_request(2, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    let arr = result.as_array().expect("codeLens result should be array");
    assert!(
        !arr.is_empty(),
        "codeLens should return a lens for the K8s schema"
    );
    let lens_str = serde_json::to_string(&arr[0]).expect("serialize lens");
    assert!(
        lens_str.contains("v1.29.0"),
        "lens should reference the configured Kubernetes version"
    );
    assert!(
        lens_str.contains("deployment-apps-v1.json"),
        "lens should reference deployment-apps-v1.json"
    );
}
