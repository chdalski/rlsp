// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use super::code_lens::code_lens_request;
use super::configuration::did_change_configuration_notification;
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
    let lens_str = schema_lens_after_opening(
        &mut service,
        uri,
        "apiVersion: v1\nkind: Pod\nmetadata:\n  name: test\n",
    )
    .await;
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
    let lens_str = schema_lens_after_opening(
        &mut service,
        uri,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: test\n",
    )
    .await;
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
    let (seen, _) = open_manifest_through_proxy("apiVersion: apps/v1\nkind: Deployment\n").await;

    assert!(
        seen.iter().any(|line| line == KUBERNETES_CONNECT),
        "a valid manifest must reach the Kubernetes schema repository: {seen:?}"
    );
}

/// Open `text` through the fake proxy and assert that it triggers no
/// Kubernetes fetch, no matching warning, and goes on to `SchemaStore` matching.
async fn assert_falls_through_to_schemastore(text: &str) {
    let (seen, warnings) = open_manifest_through_proxy(text).await;

    assert!(
        !seen
            .iter()
            .any(|line| line.contains("raw.githubusercontent.com")),
        "this manifest must not trigger a Kubernetes fetch: {seen:?}"
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

#[rstest]
#[case::traversal_kind("apiVersion: v1\nkind: ../../../../attacker/repo/main/evil\n")]
#[case::traversal_api_version("apiVersion: apps/../../x\nkind: Pod\n")]
#[case::uppercase_group("apiVersion: Apps/v1\nkind: Pod\n")]
#[case::escaped_slashes_in_kind("apiVersion: v1\nkind: \"..\\x2f..\\x2fx\"\n")]
#[tokio::test]
async fn should_not_fetch_kubernetes_schema_for_invalid_names_and_fall_through_to_schemastore(
    #[case] text: &str,
) {
    assert_falls_through_to_schemastore(text).await;
}

#[rstest]
#[case::cert_manager("apiVersion: cert-manager.io/v1\nkind: Certificate\n")]
#[case::kustomize("apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\n")]
#[tokio::test]
async fn should_not_fetch_kubernetes_schema_for_custom_resources_and_fall_through_to_schemastore(
    #[case] text: &str,
) {
    assert_falls_through_to_schemastore(text).await;
}

#[tokio::test]
async fn should_not_touch_the_network_for_a_custom_resource_when_schemastore_is_disabled() {
    let proxy = FakeProxy::start(Reply::not_found());
    let (mut service, mut log) = start_server(json!({
        "httpProxy": proxy.url(),
        "schemaStore": false
    }))
    .await;

    send(
        &mut service,
        did_open_notification(
            "file:///test/manifest.yaml",
            "apiVersion: cert-manager.io/v1\nkind: Certificate\n",
        ),
    )
    .await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(proxy.seen().is_empty(), "{:?}", proxy.seen());
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[tokio::test]
async fn should_record_kubernetes_schema_association_for_builtin_dotted_group() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/ingress.yaml";
    let lens_str = schema_lens_after_opening(
        &mut service,
        uri,
        "apiVersion: networking.k8s.io/v1\nkind: Ingress\nmetadata:\n  name: test\n",
    )
    .await;
    assert!(
        lens_str.contains("ingress-networking-v1.json"),
        "lens should reference ingress-networking-v1.json: {lens_str}"
    );
}

// ---- kubernetesVersion validation ----

const DEPLOYMENT: &str = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: test\n";
const MASTER_DEPLOYMENT_FILE: &str = "master-standalone-strict/deployment-apps-v1.json";

/// The complete settings of a server that routes outbound requests through
/// `proxy`, so no test reaches the network. `kubernetesVersion` is set only
/// when given.
///
/// A configuration change replaces all settings, so every change must be
/// built from this too, or it drops the proxy.
fn k8s_settings(proxy: &FakeProxy, version: Option<serde_json::Value>) -> serde_json::Value {
    let mut settings = json!({ "httpProxy": proxy.url(), "schemaStore": false });
    if let Some(version) = version {
        settings
            .as_object_mut()
            .expect("settings is an object")
            .insert("kubernetesVersion".to_string(), version);
    }
    settings
}

/// Start a server with [`k8s_settings`] as its initialization options.
async fn start_server_with_k8s_version(
    version: Option<serde_json::Value>,
) -> (
    LspService<Backend>,
    tokio::sync::mpsc::UnboundedReceiver<tower_lsp::lsp_types::LogMessageParams>,
    FakeProxy,
) {
    let proxy = FakeProxy::start(Reply::not_found());
    let (service, log) = start_server(k8s_settings(&proxy, version)).await;
    (service, log, proxy)
}

/// The number of Kubernetes schema repository connections the proxy served.
fn kubernetes_connects(proxy: &FakeProxy) -> usize {
    proxy
        .seen()
        .iter()
        .filter(|line| *line == KUBERNETES_CONNECT)
        .count()
}

/// Open `text` as `uri` and return the code lens, which carries the schema URL.
async fn schema_lens_after_opening(
    service: &mut LspService<Backend>,
    uri: &str,
    text: &str,
) -> String {
    send(service, did_open_notification(uri, text)).await;
    let resp = send(service, code_lens_request(50, uri)).await;
    let resp = resp.expect("codeLens should return a response");
    let result = resp.result().expect("codeLens should have result");
    let lens = result
        .as_array()
        .and_then(|lenses| lenses.first())
        .expect("codeLens should return a lens for the K8s schema");
    serde_json::to_string(lens).expect("serialize lens")
}

#[tokio::test]
async fn should_log_exactly_one_warning_for_an_invalid_version_in_initialization_options() {
    let (mut service, mut log, _proxy) = start_server_with_k8s_version(Some(json!("1.29"))).await;

    send(
        &mut service,
        did_open_notification("file:///test/one.yaml", DEPLOYMENT),
    )
    .await;
    send(
        &mut service,
        did_open_notification("file:///test/two.yaml", "apiVersion: v1\nkind: Pod\n"),
    )
    .await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("kubernetesVersion"), "{warnings:?}");
    assert!(warnings[0].contains("\"1.29\""), "{warnings:?}");
}

#[rstest]
#[case::numeric(Some(json!("1.29.0")))]
#[case::master(Some(json!("master")))]
#[case::absent(None)]
#[case::null(Some(json!(null)))]
#[tokio::test]
async fn should_log_no_warning_for_a_valid_or_absent_version(
    #[case] version: Option<serde_json::Value>,
) {
    let (mut service, mut log, _proxy) = start_server_with_k8s_version(version).await;

    send(
        &mut service,
        did_open_notification("file:///test/one.yaml", DEPLOYMENT),
    )
    .await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[case::garbage("bad")]
#[case::empty("")]
#[case::capitalised_master("Master")]
#[case::leading_v("v1.29.0")]
#[case::two_groups("1.29")]
#[case::traversal("1.29.0/../../attacker/repo/main/x")]
#[tokio::test]
async fn should_select_master_for_an_invalid_version(#[case] version: &str) {
    let (mut service, _log, _proxy) = start_server_with_k8s_version(Some(json!(version))).await;

    let lens = schema_lens_after_opening(&mut service, "file:///test/one.yaml", DEPLOYMENT).await;

    assert!(lens.contains(MASTER_DEPLOYMENT_FILE), "{lens}");
    assert!(!lens.contains("attacker"), "{lens}");
}

#[tokio::test]
async fn should_keep_other_initialization_options_when_the_version_is_invalid() {
    let proxy = FakeProxy::start(Reply::not_found());
    let (mut service, _log) = start_server(json!({
        "httpProxy": proxy.url(),
        "kubernetesVersion": "bad"
    }))
    .await;

    send(
        &mut service,
        did_open_notification("file:///test/one.yaml", DEPLOYMENT),
    )
    .await;

    assert!(
        proxy.seen().iter().any(|line| line == KUBERNETES_CONNECT),
        "the proxy setting must survive an invalid kubernetesVersion: {:?}",
        proxy.seen()
    );
}

#[tokio::test]
async fn should_warn_and_select_master_when_a_configuration_change_sets_an_invalid_version() {
    let (mut service, mut log, proxy) = start_server_with_k8s_version(Some(json!("1.29.0"))).await;
    let lens = schema_lens_after_opening(&mut service, "file:///test/one.yaml", DEPLOYMENT).await;
    assert!(lens.contains("v1.29.0-standalone-strict"), "{lens}");
    assert_eq!(kubernetes_connects(&proxy), 1, "{:?}", proxy.seen());

    send(
        &mut service,
        did_change_configuration_notification(&k8s_settings(&proxy, Some(json!("bad")))),
    )
    .await;
    let lens = schema_lens_after_opening(&mut service, "file:///test/two.yaml", DEPLOYMENT).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(lens.contains(MASTER_DEPLOYMENT_FILE), "{lens}");
    assert_eq!(
        kubernetes_connects(&proxy),
        2,
        "the fetch after the change must go through the proxy: {:?}",
        proxy.seen()
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("\"bad\""), "{warnings:?}");
}

#[tokio::test]
async fn should_warn_again_for_each_configuration_change_with_an_invalid_version() {
    let (mut service, mut log, proxy) = start_server_with_k8s_version(None).await;

    for _ in 0..2 {
        send(
            &mut service,
            did_change_configuration_notification(&k8s_settings(&proxy, Some(json!("bad")))),
        )
        .await;
    }
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 2, "{warnings:?}");
}

#[tokio::test]
async fn should_stop_warning_and_use_the_version_once_a_configuration_change_makes_it_valid() {
    let (mut service, mut log, proxy) = start_server_with_k8s_version(Some(json!("bad"))).await;

    send(
        &mut service,
        did_change_configuration_notification(&k8s_settings(&proxy, Some(json!("1.30.1")))),
    )
    .await;
    let lens = schema_lens_after_opening(&mut service, "file:///test/one.yaml", DEPLOYMENT).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(lens.contains("v1.30.1-standalone-strict"), "{lens}");
    assert_eq!(
        kubernetes_connects(&proxy),
        1,
        "the fetch after the change must go through the proxy: {:?}",
        proxy.seen()
    );
    assert_eq!(
        warnings.len(),
        1,
        "only the initialization Warning: {warnings:?}"
    );
}

#[tokio::test]
async fn should_select_master_without_warning_when_a_configuration_change_drops_the_version() {
    let (mut service, mut log, proxy) = start_server_with_k8s_version(Some(json!("1.29.0"))).await;

    send(
        &mut service,
        did_change_configuration_notification(&k8s_settings(&proxy, None)),
    )
    .await;
    let lens = schema_lens_after_opening(&mut service, "file:///test/one.yaml", DEPLOYMENT).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(lens.contains(MASTER_DEPLOYMENT_FILE), "{lens}");
    assert_eq!(
        kubernetes_connects(&proxy),
        1,
        "the fetch after the change must go through the proxy: {:?}",
        proxy.seen()
    );
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[case::control_and_bidi(json!("1.2\n[ERROR] forged\r\u{1b}[31m\u{202e}\u{2028}"))]
#[case::very_long(json!("9".repeat(10_000)))]
#[tokio::test]
async fn should_sanitize_and_cap_an_invalid_version_in_the_client_log(
    #[case] version: serde_json::Value,
) {
    let (mut service, mut log, _proxy) = start_server_with_k8s_version(Some(version)).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let message = &warnings[0];
    assert!(message.contains("kubernetesVersion"), "{message}");
    assert!(message.chars().count() <= 512, "{message}");
    assert!(
        !message
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{202e}' | '\u{2028}')),
        "{message:?}"
    );
}

#[rstest]
#[case::fractional(json!(1.29))]
#[case::integer(json!(129))]
#[case::boolean(json!(true))]
#[case::array(json!(["1.29.0"]))]
#[tokio::test]
async fn should_log_one_warning_and_select_master_for_a_wrong_typed_version_at_initialization(
    #[case] version: serde_json::Value,
) {
    let (mut service, mut log, proxy) = start_server_with_k8s_version(Some(version)).await;

    let lens = schema_lens_after_opening(&mut service, "file:///test/one.yaml", DEPLOYMENT).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(lens.contains(MASTER_DEPLOYMENT_FILE), "{lens}");
    assert_eq!(kubernetes_connects(&proxy), 1, "{:?}", proxy.seen());
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("Setting kubernetesVersion must be a string"),
        "{warnings:?}"
    );
}

#[tokio::test]
async fn should_log_one_warning_and_select_master_for_a_wrong_typed_version_on_configuration_change()
 {
    let (mut service, mut log, proxy) = start_server_with_k8s_version(Some(json!("1.29.0"))).await;

    send(
        &mut service,
        did_change_configuration_notification(&k8s_settings(&proxy, Some(json!(1.29)))),
    )
    .await;
    let lens = schema_lens_after_opening(&mut service, "file:///test/one.yaml", DEPLOYMENT).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(lens.contains(MASTER_DEPLOYMENT_FILE), "{lens}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("kubernetesVersion"), "{warnings:?}");
}
