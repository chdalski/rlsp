// SPDX-License-Identifier: MIT
use std::time::Duration;

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tower::Service;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{LogMessageParams, MessageType};
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

/// Start a server whose client-bound log messages are collected.
pub async fn start_server(
    options: serde_json::Value,
) -> (LspService<Backend>, UnboundedReceiver<LogMessageParams>) {
    let (mut service, socket) = LspService::new(Backend::new);
    let log = collect_log_messages(socket);

    send(&mut service, initialize_request_with_options(1, &options)).await;
    send(&mut service, initialized_notification()).await;
    (service, log)
}

/// Open a document whose `$schema` modeline names `schema_url`.
pub async fn open_with_modeline(service: &mut LspService<Backend>, uri: &str, schema_url: &str) {
    let text = format!("# yaml-language-server: $schema={schema_url}\ncount: not-a-number\n");
    send(service, did_open_notification(uri, &text)).await;
}

/// Return every Warning log message sent before the sentinel's own.
///
/// The sentinel is a document whose modeline names a loopback URL: the SSRF
/// guard rejects it without any network access, which logs exactly one
/// Warning. Client-bound messages arrive in order, so once the sentinel's
/// Warning is seen, every Warning the earlier requests caused has been
/// delivered. Without this anchor an absence assertion could pass because the
/// messages were read too early.
pub async fn warnings_before_sentinel(
    service: &mut LspService<Backend>,
    log: &mut UnboundedReceiver<LogMessageParams>,
) -> Vec<String> {
    const SENTINEL_URL: &str = "http://127.0.0.1/sentinel.json";
    open_with_modeline(service, "file:///test/sentinel.yaml", SENTINEL_URL).await;

    let mut warnings = Vec::new();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), log.recv())
            .await
            .expect("sentinel Warning never arrived")
            .expect("log channel closed before the sentinel Warning");
        if message.typ != MessageType::WARNING {
            continue;
        }
        if message.message.contains(SENTINEL_URL) {
            return warnings;
        }
        warnings.push(message.message);
    }
}

/// A GitHub-workflow-like schema: `jobs` names its jobs by a pattern, a job is
/// a `oneOf` of two objects behind a `$ref`, and `steps` is an array of a `$ref`.
pub fn workflow_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": { "jobs": {
            "type": "object",
            "patternProperties": { "^[_a-zA-Z][a-zA-Z0-9_-]*$": { "$ref": "#/definitions/job" } },
            "additionalProperties": false
        } },
        "definitions": {
            "job": {
                "description": "Job docs",
                "oneOf": [
                    { "properties": {
                        "runs-on": {
                            "description": "Runner docs",
                            "enum": ["ubuntu-latest", "windows-latest"]
                        },
                        "steps": { "type": "array", "items": { "$ref": "#/definitions/step" } }
                    } },
                    { "properties": { "uses": { "description": "Reusable docs" }, "with": {} } }
                ]
            },
            "step": { "properties": {
                "uses": { "description": "Step uses docs" },
                "run": { "description": "Step run docs" },
                "name": { "description": "Step name docs" }
            } }
        }
    })
}

/// A compose-like schema: `services` names its services by a pattern, and the
/// root allows `x-` extensions beside `additionalProperties: false`.
pub fn compose_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": { "services": {
            "type": "object",
            "patternProperties": { "^[a-zA-Z0-9._-]+$": { "$ref": "#/definitions/service" } },
            "additionalProperties": false
        } },
        "patternProperties": { "^x-": { "description": "Extension docs" } },
        "additionalProperties": false,
        "definitions": {
            "service": {
                "description": "Service docs",
                "properties": {
                    "image": { "description": "Image docs" },
                    "restart": { "enum": ["no", "always"] }
                }
            }
        }
    })
}
