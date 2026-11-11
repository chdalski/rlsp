// SPDX-License-Identifier: MIT
use std::time::Duration;

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use serde_json::json;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tower::Service;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::{LogMessageParams, MessageType, PublishDiagnosticsParams};
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

/// Drain the client socket in the background and forward every
/// `textDocument/publishDiagnostics` notification, in order, to the returned
/// receiver.
pub fn collect_published_diagnostics(
    socket: ClientSocket,
) -> UnboundedReceiver<PublishDiagnosticsParams> {
    let (sender, receiver) = unbounded_channel();
    tokio::spawn(socket.for_each(move |request| {
        if request.method() == "textDocument/publishDiagnostics"
            && let Some(params) = request.params()
            && let Ok(published) =
                serde_json::from_value::<PublishDiagnosticsParams>(params.clone())
        {
            sender.send(published).ok();
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

/// Return every publication of diagnostics made before the publication of the
/// sentinel's own, leaving out the sentinels' publications.
///
/// The sentinel is a document opened as the last step of `round`; each round
/// has its own, since an open sentinel is loaded again whenever the nesting
/// limit changes. Client-bound messages arrive in order, so once the
/// sentinel's publication is seen, everything the earlier requests published
/// has been delivered. Without this anchor an absence assertion could pass
/// because the messages were read too early.
pub async fn publishes_before_sentinel(
    service: &mut LspService<Backend>,
    published: &mut UnboundedReceiver<PublishDiagnosticsParams>,
    round: usize,
) -> Vec<PublishDiagnosticsParams> {
    let sentinel = format!("file:///test/sentinel-{round}.yaml");
    send(
        service,
        did_open_notification(&sentinel, "sentinel: true\n"),
    )
    .await;

    let mut before = Vec::new();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), published.recv())
            .await
            .expect("sentinel publication never arrived")
            .expect("publication channel closed before the sentinel's");
        if message.uri.as_str() == sentinel {
            return before;
        }
        if !message.uri.as_str().contains("/sentinel-") {
            before.push(message);
        }
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

/// The byte offset of an LSP position in `text`, found by an own scan for the
/// breaks `\r\n`, `\r`, and `\n`, so the tests do not lean on the code they
/// check. The character counts UTF-16 units, and one past the end of its line
/// stands for that end.
pub fn offset_of(text: &str, position: &serde_json::Value) -> usize {
    let line = position["line"].as_u64().expect("line");
    let character = usize::try_from(position["character"].as_u64().expect("character"))
        .expect("character fits");
    let mut start = 0;
    for _ in 0..line {
        let Some(found) = text[start..].find(['\r', '\n']) else {
            return text.len();
        };
        start += found;
        start += if text[start..].starts_with("\r\n") {
            2
        } else {
            1
        };
    }
    let mut offset = start;
    let mut units = 0;
    for ch in text[start..].chars() {
        if ch == '\r' || ch == '\n' || units >= character {
            break;
        }
        units += ch.len_utf16();
        offset += ch.len_utf8();
    }
    offset
}

/// `text` with the edits of `action` applied.
pub fn apply_action(text: &str, uri: &str, action: &serde_json::Value) -> String {
    let edits = action
        .pointer("/edit/changes")
        .and_then(|changes| changes.get(uri))
        .and_then(serde_json::Value::as_array)
        .expect("edits for the document");
    assert_eq!(edits.len(), 1, "one edit: {action}");
    let edit = edits.first().expect("one edit");
    let position = |pointer: &str| edit.pointer(pointer).expect("a position");
    let start = offset_of(text, position("/range/start"));
    let end = offset_of(text, position("/range/end"));
    format!(
        "{}{}{}",
        &text[..start],
        edit.get("newText")
            .and_then(serde_json::Value::as_str)
            .expect("new text"),
        &text[end..]
    )
}

/// A server with `text` open at `uri`.
pub async fn open(text: &str, uri: &str) -> LspService<Backend> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;
    send(&mut service, did_open_notification(uri, text)).await;
    service
}
