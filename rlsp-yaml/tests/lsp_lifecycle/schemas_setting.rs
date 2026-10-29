// SPDX-License-Identifier: MIT
use rstest::rstest;
use serde_json::json;
use tokio::sync::mpsc::UnboundedReceiver;
use tower_lsp::LspService;
use tower_lsp::lsp_types::LogMessageParams;

use rlsp_yaml::server::Backend;

use super::configuration::did_change_configuration_notification;
use super::fake_proxy::{FakeProxy, Reply, SCHEMA_HOST};
use super::helpers::*;

const SCHEMA_URL: &str = "http://schema.invalid/s.json";
const KUBERNETES_CONNECT: &str = "CONNECT raw.githubusercontent.com:443 HTTP/1.1";
const POD: &str = "apiVersion: v1\nkind: Pod\n";

/// How the settings reach the server.
#[derive(Debug, Clone, Copy)]
enum Delivery {
    Initialize,
    DidChangeConfiguration,
}

/// A server started with `settings`, delivered the way `delivery` says.
async fn server_with_settings(
    delivery: Delivery,
    settings: serde_json::Value,
) -> (LspService<Backend>, UnboundedReceiver<LogMessageParams>) {
    match delivery {
        Delivery::Initialize => start_server(settings).await,
        Delivery::DidChangeConfiguration => {
            let (mut service, log) = start_server(json!({})).await;
            send(
                &mut service,
                did_change_configuration_notification(&settings),
            )
            .await;
            (service, log)
        }
    }
}

/// A server with `SchemaStore` off, so only `schemas` and Kubernetes detection
/// choose a schema, with the settings delivered the way `delivery` says.
async fn server_with_schemas(
    delivery: Delivery,
    proxy: &FakeProxy,
    schemas: &serde_json::Value,
) -> (LspService<Backend>, UnboundedReceiver<LogMessageParams>) {
    server_with_settings(
        delivery,
        json!({
            "httpProxy": proxy.url(),
            "schemaStore": false,
            "schemas": schemas,
        }),
    )
    .await
}

/// Open `text` as `uri` and return the `CONNECT` lines the fake proxy saw and
/// the Warnings the run caused.
async fn open_and_collect(
    service: &mut LspService<Backend>,
    log: &mut UnboundedReceiver<LogMessageParams>,
    proxy: &FakeProxy,
    uri: &str,
    text: &str,
) -> (Vec<String>, Vec<String>) {
    send(service, did_open_notification(uri, text)).await;
    let warnings = warnings_before_sentinel(service, log).await;
    let connects = proxy
        .seen()
        .into_iter()
        .filter(|line| line.starts_with("CONNECT "))
        .collect();
    (connects, warnings)
}

fn schema_fetched(connects: &[String]) -> bool {
    connects
        .iter()
        .any(|line| line.starts_with(&format!("CONNECT {SCHEMA_HOST}")))
}

fn proxy() -> FakeProxy {
    FakeProxy::start(Reply::ok(Some("application/json"), "{}".to_string()))
}

#[rstest]
#[case::one_glob(json!("*.yaml"), "file:///test/dir/a.yaml")]
#[case::array_of_globs(json!(["*.json", "*.yaml"]), "file:///test/dir/a.yaml")]
#[case::second_glob_of_an_array(json!(["*.yml", "dir/*.yaml"]), "file:///test/dir/a.yaml")]
#[case::array_with_an_exclusion_that_misses(
    json!(["*.yaml", "!skip.yaml"]),
    "file:///test/dir/a.yaml"
)]
#[case::exclusion_listed_first(json!(["!skip.yaml", "*.yaml"]), "file:///test/dir/a.yaml")]
#[case::directory_glob_below_the_workspace(
    json!(".github/workflows/*.yml"),
    "file:///home/u/proj/.github/workflows/ci.yml"
)]
#[case::absolute_glob(json!("/test/dir/a.yaml"), "file:///test/dir/a.yaml")]
#[tokio::test]
async fn should_apply_a_schemas_entry_whose_globs_match_the_document(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
    #[case] globs: serde_json::Value,
    #[case] uri: &str,
) {
    let proxy = proxy();
    let (mut service, mut log) =
        server_with_schemas(delivery, &proxy, &json!({ SCHEMA_URL: globs })).await;

    let (connects, warnings) =
        open_and_collect(&mut service, &mut log, &proxy, uri, "key: value\n").await;

    assert!(schema_fetched(&connects), "{connects:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[case::other_extension(json!("*.json"), "file:///test/dir/a.yaml")]
#[case::directory_is_part_of_a_longer_name(
    json!(".github/workflows/*.yml"),
    "file:///home/u/proj/my.github/workflows/ci.yml"
)]
#[case::directory_is_missing(
    json!(".github/workflows/*.yml"),
    "file:///home/u/proj/other/ci.yml"
)]
#[case::directory_glob_does_not_reach_a_subdirectory(
    json!(".github/workflows/*.yml"),
    "file:///home/u/proj/.github/workflows/sub/ci.yml"
)]
#[case::second_glob_misses_too(json!(["*.yml", "sub/*.yaml"]), "file:///test/other/a.yaml")]
#[tokio::test]
async fn should_not_apply_a_schemas_entry_whose_globs_miss_the_document(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
    #[case] globs: serde_json::Value,
    #[case] uri: &str,
) {
    let proxy = proxy();
    let (mut service, mut log) =
        server_with_schemas(delivery, &proxy, &json!({ SCHEMA_URL: globs })).await;

    let (connects, warnings) =
        open_and_collect(&mut service, &mut log, &proxy, uri, "key: value\n").await;

    assert!(!schema_fetched(&connects), "{connects:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[case::exclusion_after_the_inclusion(json!(["*.yaml", "!skip.yaml"]))]
#[case::exclusion_before_the_inclusion(json!(["!skip.yaml", "*.yaml"]))]
#[case::exclusion_with_a_slash(json!(["**", "!/dir/skip.yaml"]))]
#[tokio::test]
async fn should_leave_an_excluded_document_to_the_next_schema_source(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
    #[case] globs: serde_json::Value,
) {
    let proxy = proxy();
    let (mut service, mut log) =
        server_with_schemas(delivery, &proxy, &json!({ SCHEMA_URL: globs })).await;

    let (connects, warnings) = open_and_collect(
        &mut service,
        &mut log,
        &proxy,
        "file:///test/dir/skip.yaml",
        POD,
    )
    .await;

    assert!(!schema_fetched(&connects), "{connects:?}");
    assert!(
        connects.iter().any(|line| line == KUBERNETES_CONNECT),
        "the excluded manifest must go on to Kubernetes detection: {connects:?}"
    );
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[case::only_an_exclusion(json!(["!skip.yaml"]))]
#[case::empty_array(json!([]))]
#[case::empty_string(json!(""))]
#[case::array_with_a_number(json!(["*.yaml", 5]))]
#[case::object(json!({}))]
#[case::exclusion_string(json!("!**/*.yaml"))]
#[case::exclusion_string_with_credentials(json!("!http://user:secret@h.invalid/x"))]
#[case::secret_glob_next_to_a_number(json!(["**/secret-marker-*.yaml", 5]))]
#[case::over_long_glob(json!([format!("**/{}", "a".repeat(257))]))]
#[case::too_many_globs(json!(vec!["*.yaml"; 33]))]
#[tokio::test]
async fn should_drop_an_entry_without_a_usable_glob_and_warn_once(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
    #[case] globs: serde_json::Value,
) {
    let proxy = proxy();
    let (mut service, mut log) =
        server_with_schemas(delivery, &proxy, &json!({ SCHEMA_URL: globs })).await;

    let (connects, warnings) = open_and_collect(
        &mut service,
        &mut log,
        &proxy,
        "file:///test/dir/a.yaml",
        "key: value\n",
    )
    .await;

    assert!(!schema_fetched(&connects), "{connects:?}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with(&format!(
            "Setting schemas, entry {SCHEMA_URL} must be a glob"
        )),
        "{warnings:?}"
    );
}

#[tokio::test]
async fn should_withhold_a_rejected_glob_array_from_the_warning() {
    let proxy = proxy();
    let (mut service, mut log) = server_with_schemas(
        Delivery::Initialize,
        &proxy,
        &json!({ SCHEMA_URL: ["http://user:secret@h.invalid/x", 5] }),
    )
    .await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(!warnings[0].contains("secret"), "{warnings:?}");
}

#[rstest]
#[tokio::test]
async fn should_apply_the_entry_whose_url_sorts_first_when_several_match(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
) {
    // The proxy answers for every host, so the host that is fetched tells
    // which entry won.
    let proxy = proxy();
    let (mut service, mut log) = server_with_schemas(
        delivery,
        &proxy,
        &json!({
            "http://z.invalid/s.json": "*.yaml",
            "http://a.invalid/s.json": ["*.yaml"],
            "http://m.invalid/s.json": "**",
        }),
    )
    .await;

    let (connects, _) = open_and_collect(
        &mut service,
        &mut log,
        &proxy,
        "file:///test/dir/a.yaml",
        "key: value\n",
    )
    .await;

    assert_eq!(
        connects,
        ["CONNECT a.invalid:80 HTTP/1.1".to_string()],
        "{connects:?}"
    );
}

#[rstest]
#[tokio::test]
async fn should_keep_the_warning_for_a_huge_glob_small(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
) {
    let proxy = proxy();
    let (mut service, mut log) = server_with_schemas(
        delivery,
        &proxy,
        &json!({ SCHEMA_URL: ["x".repeat(1 << 20)] }),
    )
    .await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].chars().count() < 1000, "{}", warnings[0].len());
}

#[rstest]
#[case::most_globs(json!(vec!["*.yaml"; 32]), "file:///test/dir/a.yaml".to_string())]
#[case::longest_glob(
    json!([format!("**/{}", "a".repeat(253))]),
    format!("file:///test/dir/{}", "a".repeat(253))
)]
#[tokio::test]
async fn should_apply_an_entry_at_the_limits(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
    #[case] globs: serde_json::Value,
    #[case] uri: String,
) {
    let proxy = proxy();
    let (mut service, mut log) =
        server_with_schemas(delivery, &proxy, &json!({ SCHEMA_URL: globs })).await;

    let (connects, warnings) =
        open_and_collect(&mut service, &mut log, &proxy, &uri, "key: value\n").await;

    assert!(schema_fetched(&connects), "{connects:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[rstest]
#[tokio::test]
async fn should_let_the_next_entry_apply_when_the_first_excludes_the_document(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
) {
    let proxy = proxy();
    let (mut service, mut log) = server_with_schemas(
        delivery,
        &proxy,
        &json!({
            "http://a.invalid/s.json": ["**/*.yaml", "!skip/*.yaml"],
            "http://b.invalid/s.json": "**/*.yaml",
        }),
    )
    .await;

    let (connects, _) = open_and_collect(
        &mut service,
        &mut log,
        &proxy,
        "file:///test/skip/d.yaml",
        "key: value\n",
    )
    .await;

    assert_eq!(connects, ["CONNECT b.invalid:80 HTTP/1.1".to_string()]);
}

#[rstest]
#[tokio::test]
async fn should_leave_an_excluded_document_to_schemastore(
    #[values(Delivery::Initialize, Delivery::DidChangeConfiguration)] delivery: Delivery,
) {
    let proxy = proxy();
    let (mut service, mut log) = server_with_settings(
        delivery,
        json!({
            "httpProxy": proxy.url(),
            "schemas": { SCHEMA_URL: ["**/*.yaml", "!skip/*.yaml"] },
        }),
    )
    .await;

    let (connects, _) = open_and_collect(
        &mut service,
        &mut log,
        &proxy,
        "file:///test/skip/d.yaml",
        "key: value\n",
    )
    .await;

    assert!(!schema_fetched(&connects), "{connects:?}");
    assert!(
        connects
            .iter()
            .any(|line| line == "CONNECT www.schemastore.org:443 HTTP/1.1"),
        "the excluded document must go on to SchemaStore: {connects:?}"
    );
}
