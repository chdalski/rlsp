// SPDX-License-Identifier: MIT
use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tower::Service;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::LogMessageParams;
use tower_lsp::{ClientSocket, LspService};

pub fn initialize_request(id: i64) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null
        }))
        .finish()
}

/// An `initialize` request carrying `options` as `initializationOptions`.
pub fn initialize_request_with_options(id: i64, options: &serde_json::Value) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": options
        }))
        .finish()
}

/// Drain the client socket in the background and forward every
/// `window/logMessage` notification, in order, to the returned receiver.
///
/// The socket is drained even after the receiver is dropped, so the server
/// never blocks on client-bound messages.
pub fn collect_log_messages(socket: ClientSocket) -> UnboundedReceiver<LogMessageParams> {
    let (sender, receiver) = unbounded_channel();
    tokio::spawn(socket.for_each(move |request| {
        if request.method() == "window/logMessage"
            && let Some(params) = request.params()
            && let Ok(log) = serde_json::from_value::<LogMessageParams>(params.clone())
        {
            sender.send(log).ok();
        }
        async {}
    }));
    receiver
}

pub fn initialized_notification() -> Request {
    Request::build("initialized").params(json!({})).finish()
}

pub fn shutdown_request(id: i64) -> Request {
    Request::build("shutdown").id(id).finish()
}

pub fn did_open_notification(uri: &str, text: &str) -> Request {
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

pub fn did_change_notification(uri: &str, text: &str, version: i32) -> Request {
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

pub fn did_close_notification(uri: &str) -> Request {
    Request::build("textDocument/didClose")
        .params(json!({
            "textDocument": {
                "uri": uri
            }
        }))
        .finish()
}

pub async fn send(service: &mut LspService<Backend>, req: Request) -> Option<Response> {
    service.call(req).await.expect("service call failed")
}
