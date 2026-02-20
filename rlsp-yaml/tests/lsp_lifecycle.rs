use rlsp_yaml::server::Backend;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{
    HoverProviderCapability, InitializeResult, OneOf, TextDocumentSyncCapability,
    TextDocumentSyncKind,
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
    let (mut service, _socket) = LspService::new(Backend::new);

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
    let (mut service, _socket) = LspService::new(Backend::new);

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/doc.yaml";
    send(&mut service, did_open_notification(uri, "content")).await;
    send(&mut service, did_close_notification(uri)).await;

    let backend = service.inner();
    let stored = backend.get_document_text(uri);
    assert_eq!(stored, None);
}
