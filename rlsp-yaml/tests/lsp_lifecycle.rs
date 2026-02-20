use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{
    DiagnosticSeverity, HoverProviderCapability, InitializeResult, OneOf,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};

fn initialize_request(id: i64) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null
        }))
        .finish()
}

fn initialized_notification() -> Request {
    Request::build("initialized").params(json!({})).finish()
}

fn shutdown_request(id: i64) -> Request {
    Request::build("shutdown").id(id).finish()
}

fn did_open_notification(uri: &str, text: &str) -> Request {
    Request::build("textDocument/didOpen")
        .params(json!({
            "textDocument": {
                "uri": uri,
                "languageId": "yaml",
                "version": 1,
                "text": text
            }
        }))
        .finish()
}

fn did_change_notification(uri: &str, text: &str, version: i32) -> Request {
    Request::build("textDocument/didChange")
        .params(json!({
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": [
                { "text": text }
            ]
        }))
        .finish()
}

fn did_close_notification(uri: &str) -> Request {
    Request::build("textDocument/didClose")
        .params(json!({
            "textDocument": {
                "uri": uri
            }
        }))
        .finish()
}

async fn send(service: &mut LspService<Backend>, req: Request) -> Option<Response> {
    use tower::Service;
    service.call(req).await.expect("service call failed")
}

#[tokio::test]
async fn should_complete_initialize_shutdown_lifecycle() {
    let (mut service, _socket) = LspService::new(Backend::new);

    let resp = send(&mut service, initialize_request(1)).await;
    let resp = resp.expect("initialize should return a response");
    let result: InitializeResult =
        serde_json::from_value(resp.result().expect("result missing").clone())
            .expect("failed to deserialize InitializeResult");

    assert_eq!(
        result.capabilities.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
    );
    assert_eq!(
        result.capabilities.hover_provider,
        Some(HoverProviderCapability::Simple(true))
    );
    assert!(matches!(
        result.capabilities.document_symbol_provider,
        Some(OneOf::Left(true))
    ));

    let notif_resp = send(&mut service, initialized_notification()).await;
    assert!(
        notif_resp.is_none(),
        "notifications should not return a response"
    );

    let shutdown_resp = send(&mut service, shutdown_request(2)).await;
    assert!(shutdown_resp.is_some(), "shutdown should return a response");
}

#[tokio::test]
async fn should_store_document_text_on_did_open() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    let text = "key: value";
    let resp = send(&mut service, did_open_notification(uri, text)).await;
    assert!(
        resp.is_none(),
        "didOpen notification should not return a response"
    );

    let backend = service.inner();
    let stored = backend.get_document_text(uri);
    assert_eq!(stored.as_deref(), Some(text));
}

#[tokio::test]
async fn should_remove_document_on_did_close() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    send(&mut service, did_open_notification(uri, "content")).await;
    send(&mut service, did_close_notification(uri)).await;

    let backend = service.inner();
    let stored = backend.get_document_text(uri);
    assert_eq!(stored, None);
}

#[tokio::test]
async fn should_publish_diagnostics_on_did_open_with_invalid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/bad.yaml";
    send(&mut service, did_open_notification(uri, "key: [bad\n")).await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[tokio::test]
async fn should_publish_empty_diagnostics_on_did_open_with_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/good.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    let backend = service.inner();
    let diags = backend
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(diags.is_empty());
}

#[tokio::test]
async fn should_update_diagnostics_on_did_change() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    send(&mut service, did_open_notification(uri, "key: value\n")).await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(diags.is_empty());
    }

    send(&mut service, did_change_notification(uri, "key: [bad\n", 2)).await;

    let diags = service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[tokio::test]
async fn should_clear_diagnostics_on_did_close() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    send(&mut service, did_open_notification(uri, "key: [bad\n")).await;

    {
        let diags = service
            .inner()
            .get_diagnostics(uri)
            .expect("diagnostics should exist");
        assert!(!diags.is_empty());
    }

    send(&mut service, did_close_notification(uri)).await;

    let diags = service.inner().get_diagnostics(uri);
    assert!(diags.is_none() || diags.as_ref().is_some_and(Vec::is_empty));
}

fn hover_request(id: i64, uri: &str, line: u32, character: u32) -> Request {
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

fn document_symbol_request(id: i64, uri: &str) -> Request {
    Request::build("textDocument/documentSymbol")
        .id(id)
        .params(json!({
            "textDocument": { "uri": uri }
        }))
        .finish()
}

#[tokio::test]
async fn should_return_document_symbols_for_valid_yaml() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/symbols.yaml";
    send(
        &mut service,
        did_open_notification(uri, "name: Alice\nage: 30\n"),
    )
    .await;

    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have result");
    assert!(
        !result.is_null(),
        "documentSymbol result should not be null"
    );
    let result_str = serde_json::to_string(result).expect("serialize result");
    assert!(result_str.contains("name"), "symbols should contain 'name'");
    assert!(result_str.contains("age"), "symbols should contain 'age'");
}

#[tokio::test]
async fn should_return_empty_symbols_for_unknown_document() {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    // Do NOT send didOpen for this URI
    let uri = "file:///test/unknown.yaml";
    let resp = send(&mut service, document_symbol_request(2, uri)).await;
    let resp = resp.expect("documentSymbol should return a response");
    let result = resp.result().expect("documentSymbol should have result");
    // Should be null (None) or empty array
    assert!(
        result.is_null() || result.as_array().is_some_and(Vec::is_empty),
        "documentSymbol result should be null or empty for unknown document"
    );
}
