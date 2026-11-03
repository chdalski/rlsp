// SPDX-License-Identifier: MIT
//
// Pins the `maxNestingDepth` setting through the server's entry points: the
// diagnostics of `didOpen` and `didChange`, the AST the store keeps for the
// handlers that read it, `formatting`, and the loading of every open document
// again when the effective limit changes.
use std::fmt::Write as _;

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::{Diagnostic, NumberOrString, PublishDiagnosticsParams};

use super::configuration::did_change_configuration_notification;
use super::formatting_integration::formatting_request_with_tab_size;
use super::helpers::*;
use super::watched_files::did_change_watched_files_notification;

/// Three nested block mappings: loads with a limit of 3, not with 2.
const THREE_LEVELS: &str = "a:\n  b:\n    c: 1\n";

/// `levels` nested block mappings; the leaf has a double space after its
/// colon, so the formatter has an edit to make.
fn nested(levels: usize) -> String {
    let mut text = String::new();
    for level in 0..levels - 1 {
        writeln!(text, "{}k:", "  ".repeat(level)).expect("writing to a String");
    }
    writeln!(text, "{}leaf:   1", "  ".repeat(levels - 1)).expect("writing to a String");
    text
}

fn document_symbol_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/documentSymbol")
        .id(id)
        .params(json!({ "textDocument": { "uri": uri } }))
        .finish()
}

/// Whether `documentSymbol` finds a symbol, which it does only for a document
/// whose AST the store holds.
async fn has_symbols(service: &mut LspService<Backend>, uri: &str) -> bool {
    let resp = send(service, document_symbol_request(9, uri))
        .await
        .expect("documentSymbol should return a response");
    resp.result()
        .expect("documentSymbol should have a result")
        .as_array()
        .is_some_and(|symbols| !symbols.is_empty())
}

fn diagnostics_of(service: &LspService<Backend>, uri: &str) -> Vec<Diagnostic> {
    service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist")
}

/// A server whose client-bound messages are dropped.
async fn server(options: &Value) -> LspService<Backend> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request_with_options(1, options)).await;
    send(&mut service, initialized_notification()).await;
    service
}

/// A server whose publications of diagnostics are collected.
async fn server_with_publishes(
    options: &Value,
) -> (
    LspService<Backend>,
    UnboundedReceiver<PublishDiagnosticsParams>,
) {
    let (mut service, socket) = LspService::new(Backend::new);
    let published = collect_published_diagnostics(socket);
    send(&mut service, initialize_request_with_options(1, options)).await;
    send(&mut service, initialized_notification()).await;
    (service, published)
}

fn publishes_of(published: &[PublishDiagnosticsParams], uri: &str) -> usize {
    published
        .iter()
        .filter(|publication| publication.uri.as_str() == uri)
        .count()
}

// ---- the limit reaches the loads of the handlers ----

#[rstest]
#[case::at_the_default(256, 0)]
#[case::over_the_default(257, 1)]
#[case::far_over_the_default(300, 1)]
#[tokio::test]
async fn default_limit_marks_a_document_over_256_levels(
    #[case] levels: usize,
    #[case] expected_diagnostics: usize,
) {
    let uri = "file:///test/default-limit.yaml";
    let mut service = server(&json!({})).await;

    send(&mut service, did_open_notification(uri, &nested(levels))).await;

    let diagnostics = diagnostics_of(&service, uri);
    assert_eq!(diagnostics.len(), expected_diagnostics, "{diagnostics:?}");
    if let Some(diagnostic) = diagnostics.first() {
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("yamlSyntax".to_string()))
        );
    }
}

#[tokio::test]
async fn setting_512_loads_a_document_of_300_levels() {
    let uri = "file:///test/limit-512.yaml";
    let mut service = server(&json!({ "maxNestingDepth": 512 })).await;

    send(&mut service, did_open_notification(uri, &nested(300))).await;

    assert!(diagnostics_of(&service, uri).is_empty());
    assert!(has_symbols(&mut service, uri).await);
}

#[rstest]
#[case::fits_the_limit(3, true)]
#[case::over_the_limit(2, false)]
#[tokio::test]
async fn limit_decides_whether_the_store_holds_the_ast(#[case] limit: u64, #[case] has_ast: bool) {
    let uri = "file:///test/ast-limit.yaml";
    let mut service = server(&json!({ "maxNestingDepth": limit })).await;

    send(&mut service, did_open_notification(uri, THREE_LEVELS)).await;

    assert_eq!(has_symbols(&mut service, uri).await, has_ast);
    assert_eq!(diagnostics_of(&service, uri).is_empty(), has_ast);
}

#[tokio::test]
async fn limit_applies_to_did_change() {
    let uri = "file:///test/change-limit.yaml";
    let mut service = server(&json!({ "maxNestingDepth": 3 })).await;
    send(&mut service, did_open_notification(uri, THREE_LEVELS)).await;
    assert!(diagnostics_of(&service, uri).is_empty());

    send(&mut service, did_change_notification(uri, &nested(4), 2)).await;
    assert_eq!(diagnostics_of(&service, uri).len(), 1);
    assert!(!has_symbols(&mut service, uri).await);

    send(&mut service, did_change_notification(uri, THREE_LEVELS, 3)).await;
    assert!(diagnostics_of(&service, uri).is_empty());
    assert!(has_symbols(&mut service, uri).await);
}

#[tokio::test]
async fn limit_applies_to_did_change_watched_files() {
    let uri = "file:///test/watched-limit.yaml";
    let mut service = server(&json!({ "maxNestingDepth": 2 })).await;
    send(&mut service, did_open_notification(uri, THREE_LEVELS)).await;
    assert_eq!(diagnostics_of(&service, uri).len(), 1);

    send(&mut service, did_change_watched_files_notification(uri)).await;

    assert_eq!(diagnostics_of(&service, uri).len(), 1);
}

// ---- formatting ----

#[rstest]
#[case::default_limit(json!({}), false)]
#[case::raised_limit(json!({ "maxNestingDepth": 512 }), true)]
#[tokio::test]
async fn formatting_a_document_of_300_levels_needs_the_raised_limit(
    #[case] options: Value,
    #[case] formats: bool,
) {
    let uri = "file:///test/format-limit.yaml";
    let mut service = server(&options).await;
    send(&mut service, did_open_notification(uri, &nested(300))).await;

    let resp = send(&mut service, formatting_request_with_tab_size(2, uri, 2))
        .await
        .expect("formatting should return a response");

    let result = resp.result().expect("formatting should have a result");
    assert_eq!(
        result.as_array().is_some_and(|edits| !edits.is_empty()),
        formats
    );
}

// ---- a rejected value takes the default ----

#[derive(Clone, Copy, Debug)]
enum Moment {
    Initialize,
    ChangeConfiguration,
}

#[rstest]
#[case::zero_is_not_clamped_to_one(json!(0), 2, false)]
#[case::over_the_cap_is_not_clamped_to_it(json!(513), 300, true)]
#[case::string_is_not_read_as_a_number(json!("256"), 257, true)]
#[tokio::test]
async fn rejected_limit_takes_the_default_and_is_never_clamped(
    #[values(Moment::Initialize, Moment::ChangeConfiguration)] moment: Moment,
    #[case] value: Value,
    #[case] levels: usize,
    #[case] marked: bool,
) {
    let uri = "file:///test/rejected-limit.yaml";
    let settings = json!({ "maxNestingDepth": value });
    let (mut service, mut log) = match moment {
        Moment::Initialize => start_server(settings).await,
        Moment::ChangeConfiguration => {
            let (mut service, log) = start_server(json!({})).await;
            send(
                &mut service,
                did_change_configuration_notification(&settings),
            )
            .await;
            (service, log)
        }
    };

    send(&mut service, did_open_notification(uri, &nested(levels))).await;

    assert_eq!(
        diagnostics_of(&service, uri).len(),
        usize::from(marked),
        "{moment:?}"
    );
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;
    assert_eq!(warnings.len(), 1, "{moment:?}: {warnings:?}");
    assert!(
        warnings[0].starts_with("Setting maxNestingDepth "),
        "{moment:?}: {warnings:?}"
    );
}

// ---- a change of the effective limit loads the open documents again ----

#[tokio::test]
async fn raising_the_limit_reparses_every_open_document_once() {
    let (a, b, closed) = (
        "file:///test/a.yaml",
        "file:///test/b.yaml",
        "file:///test/closed.yaml",
    );
    let (mut service, mut published) =
        server_with_publishes(&json!({ "maxNestingDepth": 2 })).await;
    for uri in [a, b, closed] {
        send(&mut service, did_open_notification(uri, THREE_LEVELS)).await;
    }
    send(&mut service, did_close_notification(closed)).await;
    publishes_before_sentinel(&mut service, &mut published, 1).await;
    assert_eq!(diagnostics_of(&service, a).len(), 1);

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "maxNestingDepth": 3 })),
    )
    .await;

    let after = publishes_before_sentinel(&mut service, &mut published, 2).await;
    assert_eq!(publishes_of(&after, a), 1, "{after:?}");
    assert_eq!(publishes_of(&after, b), 1, "{after:?}");
    assert_eq!(publishes_of(&after, closed), 0, "{after:?}");
    for uri in [a, b] {
        assert!(diagnostics_of(&service, uri).is_empty(), "{uri}");
        assert!(has_symbols(&mut service, uri).await, "{uri}");
    }
}

#[tokio::test]
async fn raising_the_limit_to_512_loads_a_document_of_300_levels() {
    let uri = "file:///test/raised.yaml";
    let mut service = server(&json!({})).await;
    send(&mut service, did_open_notification(uri, &nested(300))).await;
    assert_eq!(diagnostics_of(&service, uri).len(), 1);

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "maxNestingDepth": 512 })),
    )
    .await;

    assert!(diagnostics_of(&service, uri).is_empty());
    assert!(has_symbols(&mut service, uri).await);
}

#[tokio::test]
async fn a_settings_change_without_the_field_returns_to_the_default() {
    let uri = "file:///test/lowered.yaml";
    let mut service = server(&json!({ "maxNestingDepth": 512 })).await;
    send(&mut service, did_open_notification(uri, &nested(300))).await;
    assert!(diagnostics_of(&service, uri).is_empty());

    send(
        &mut service,
        did_change_configuration_notification(&json!({})),
    )
    .await;

    assert_eq!(diagnostics_of(&service, uri).len(), 1);
    assert!(!has_symbols(&mut service, uri).await);
}

#[tokio::test]
async fn a_rejected_value_that_changes_the_effective_limit_reparses() {
    let uri = "file:///test/rejected-change.yaml";
    let (mut service, mut published) =
        server_with_publishes(&json!({ "maxNestingDepth": 512 })).await;
    send(&mut service, did_open_notification(uri, &nested(300))).await;
    publishes_before_sentinel(&mut service, &mut published, 1).await;
    assert!(diagnostics_of(&service, uri).is_empty());

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "maxNestingDepth": 513 })),
    )
    .await;

    let after = publishes_before_sentinel(&mut service, &mut published, 2).await;
    assert_eq!(publishes_of(&after, uri), 1, "{after:?}");
    assert_eq!(diagnostics_of(&service, uri).len(), 1);
}

#[tokio::test]
async fn reparse_loads_the_store_even_when_validation_is_off() {
    let uri = "file:///test/no-validation.yaml";
    let mut service = server(&json!({ "maxNestingDepth": 2 })).await;
    send(&mut service, did_open_notification(uri, THREE_LEVELS)).await;
    assert!(!has_symbols(&mut service, uri).await);

    send(
        &mut service,
        did_change_configuration_notification(&json!({ "maxNestingDepth": 3, "validate": false })),
    )
    .await;

    assert!(has_symbols(&mut service, uri).await);
}

#[rstest]
#[case::other_setting(json!({}), json!({ "keyOrdering": true }))]
#[case::validation_off(json!({}), json!({ "validate": false }))]
#[case::default_spelled_out(json!({}), json!({ "maxNestingDepth": 256 }))]
#[case::rejected_zero(json!({}), json!({ "maxNestingDepth": 0 }))]
#[case::rejected_string(json!({}), json!({ "maxNestingDepth": "x" }))]
#[case::empty_settings(json!({}), json!({}))]
#[case::same_value(json!({ "maxNestingDepth": 300 }), json!({ "maxNestingDepth": 300 }))]
#[tokio::test]
async fn no_reparse_when_the_effective_limit_is_unchanged(
    #[case] initial: Value,
    #[case] changed: Value,
) {
    let uri = "file:///test/unchanged.yaml";
    let (mut service, mut published) = server_with_publishes(&initial).await;
    send(&mut service, did_open_notification(uri, THREE_LEVELS)).await;
    publishes_before_sentinel(&mut service, &mut published, 1).await;

    send(
        &mut service,
        did_change_configuration_notification(&changed),
    )
    .await;

    let after = publishes_before_sentinel(&mut service, &mut published, 2).await;
    assert!(after.is_empty(), "{after:?}");
}
