// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::code_lens::code_lens_request;
use super::fake_proxy::{FakeProxy, Reply};
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

const KUBERNETES_CONNECT: &str = "CONNECT raw.githubusercontent.com:443 HTTP/1.1";
const SCHEMASTORE_CONNECT: &str = "CONNECT www.schemastore.org:443 HTTP/1.1";

/// Open `text` as a manifest with every outbound request routed through a
/// fake proxy; return the `CONNECT` and request lines the proxy saw and the
/// Warning log messages the run caused.
async fn open_manifest_through_proxy(text: &str) -> (Vec<String>, Vec<String>) {
    let proxy = FakeProxy::start(Reply::not_found());
    let (mut service, mut log) = start_server(json!({ "httpProxy": proxy.url() })).await;

    send(
        &mut service,
        did_open_notification("file:///test/manifest.yaml", text),
    )
    .await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    (proxy.seen(), warnings)
}

#[tokio::test]
async fn should_fetch_kubernetes_schema_for_valid_manifest() {
    let (seen, _) = open_manifest_through_proxy("apiVersion: example.com/v1\nkind: Widget\n").await;

    assert!(
        seen.iter().any(|line| line == KUBERNETES_CONNECT),
        "a valid manifest must reach the Kubernetes schema repository: {seen:?}"
    );
}

#[rstest]
#[case::traversal_kind("apiVersion: v1\nkind: ../../../../attacker/repo/main/evil\n")]
#[case::traversal_api_version("apiVersion: apps/../../x\nkind: Pod\n")]
#[case::uppercase_group("apiVersion: Apps/v1\nkind: Pod\n")]
#[case::escaped_slashes_in_kind("apiVersion: v1\nkind: \"..\\x2f..\\x2fx\"\n")]
#[tokio::test]
async fn should_not_fetch_kubernetes_schema_for_invalid_names_and_fall_through_to_schemastore(
    #[case] text: &str,
) {
    let (seen, warnings) = open_manifest_through_proxy(text).await;

    assert!(
        !seen
            .iter()
            .any(|line| line.contains("raw.githubusercontent.com")),
        "an invalid name must not trigger a Kubernetes fetch: {seen:?}"
    );
    assert!(
        seen.iter().any(|line| line == SCHEMASTORE_CONNECT),
        "the document must go on to SchemaStore matching: {seen:?}"
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("raw.githubusercontent.com")),
        "{warnings:?}"
    );
}
