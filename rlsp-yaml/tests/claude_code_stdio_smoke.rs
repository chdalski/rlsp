// SPDX-License-Identifier: MIT
//
// Stdio smoke test for the compiled `rlsp-yaml` binary — the exact process
// Claude Code spawns via `command: "rlsp-yaml"` in
// `integrations/claude-code/.lsp.json`. Unlike `tests/lsp_lifecycle/`, which
// drives `tower_lsp::LspService` in-process through `tower::Service::call`
// and never touches wire framing, this test spawns the real binary and
// speaks real `Content-Length`-framed JSON-RPC over its stdio pipes. It
// proves the binary the Claude Code LSP plugin depends on actually publishes
// diagnostics over stdio.
//
// Fixtures are reused from already-verified in-process behavior rather than
// invented here: "key: [bad\n" is the same syntax-error input asserted in
// `tests/lsp_lifecycle/document_management.rs` and `src/parser.rs`
// (`code: "yamlSyntax"`, severity ERROR); "key: value\n" is the same
// valid-YAML input asserted to produce no diagnostics.

#![expect(clippy::expect_used, missing_docs, reason = "test code")]

use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const ABSENCE_TIMEOUT: Duration = Duration::from_secs(2);
const EXIT_TIMEOUT: Duration = Duration::from_secs(2);

// ---- Harness: spawn the binary and speak Content-Length-framed JSON-RPC ---

/// Owns the spawned child. Kills it on drop so a panicking assertion never
/// leaves an orphan `rlsp-yaml` process running.
struct ServerProcess {
    child: Child,
}

impl ServerProcess {
    /// Waits for the child to exit on its own (after `shutdown`/`exit`),
    /// falling back to a kill if it hasn't within `timeout`.
    fn wait_for_exit(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawns the compiled `rlsp-yaml` binary and a background reader thread
/// that decodes framed messages from its stdout onto a channel.
fn spawn_server() -> (ServerProcess, ChildStdin, mpsc::Receiver<Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rlsp-yaml"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn rlsp-yaml binary");

    let stdin = child.stdin.take().expect("child stdin");
    let stdout = child.stdout.take().expect("child stdout");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || read_messages(stdout, &tx));

    (ServerProcess { child }, stdin, rx)
}

/// Reads framed messages off `stdout` until the pipe closes or the receiver
/// is dropped.
fn read_messages(stdout: ChildStdout, tx: &mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    while let Some(message) = read_one_message(&mut reader) {
        if tx.send(message).is_err() {
            return;
        }
    }
}

/// Reads one `Content-Length`-framed JSON-RPC message. Length is a byte
/// count of the UTF-8-encoded body, not a line or char count.
fn read_one_message(reader: &mut impl Read) -> Option<Value> {
    let content_length = read_content_length(reader)?;
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn read_content_length(reader: &mut impl Read) -> Option<usize> {
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        reader.read_exact(&mut byte).ok()?;
        header.push(byte[0]);
    }
    String::from_utf8(header)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length:"))
        .and_then(|value| value.trim().parse().ok())
}

fn send(stdin: &mut ChildStdin, message: &Value) {
    let body = serde_json::to_vec(message).expect("serialize LSP message");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
    stdin.write_all(&body).expect("write body");
    stdin.flush().expect("flush stdin");
}

/// Reads messages off `rx`, discarding non-matching ones, until `matches`
/// accepts one or `timeout` elapses.
fn recv_matching(
    rx: &mpsc::Receiver<Value>,
    timeout: Duration,
    mut matches: impl FnMut(&Value) -> bool,
) -> Option<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let message = rx.recv_timeout(remaining).ok()?;
        if matches(&message) {
            return Some(message);
        }
    }
}

/// True for a JSON-RPC response (not a request or notification) with the
/// given `id`. Distinguishing on `method` absence avoids matching a
/// server-initiated request (e.g. `client/registerCapability`) that happens
/// to reuse the same id value.
fn is_response_to(message: &Value, id: i64) -> bool {
    message.get("method").is_none() && message.get("id") == Some(&json!(id))
}

fn is_diagnostics_for(message: &Value, uri: &str) -> bool {
    message.get("method") == Some(&json!("textDocument/publishDiagnostics"))
        && message
            .get("params")
            .and_then(|params| params.get("uri"))
            .is_some_and(|value| value == uri)
}

// ---- LSP message builders ---------------------------------------------

fn initialize_request() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "processId": null,
            "rootUri": null
        }
    })
}

fn initialized_notification() -> Value {
    json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} })
}

fn did_open_notification(uri: &str, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "yaml",
                "version": 1,
                "text": text
            }
        }
    })
}

fn shutdown_request() -> Value {
    json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null })
}

fn exit_notification() -> Value {
    json!({ "jsonrpc": "2.0", "method": "exit" })
}

/// Runs the `initialize` -> `initialized` -> `didOpen` handshake and asserts
/// the `initialize` response carries no error.
fn initialize_and_open(stdin: &mut ChildStdin, rx: &mpsc::Receiver<Value>, uri: &str, text: &str) {
    send(stdin, &initialize_request());
    let response = recv_matching(rx, HANDSHAKE_TIMEOUT, |m| is_response_to(m, 1))
        .expect("timed out waiting for initialize response");
    assert!(
        response.get("error").is_none(),
        "initialize returned an error: {response:?}"
    );

    send(stdin, &initialized_notification());
    send(stdin, &did_open_notification(uri, text));
}

/// Runs the `shutdown` -> `exit` teardown and waits for the process to exit.
fn shutdown_and_exit(
    server: &mut ServerProcess,
    stdin: &mut ChildStdin,
    rx: &mpsc::Receiver<Value>,
) {
    send(stdin, &shutdown_request());
    recv_matching(rx, HANDSHAKE_TIMEOUT, |m| is_response_to(m, 2))
        .expect("timed out waiting for shutdown response");
    send(stdin, &exit_notification());
    server.wait_for_exit(EXIT_TIMEOUT);
}

// ---- Tests --------------------------------------------------------------

#[test]
fn stdio_smoke_reports_syntax_error_diagnostic() {
    let (mut server, mut stdin, rx) = spawn_server();
    let uri = "file:///smoke/bad.yaml";
    initialize_and_open(&mut stdin, &rx, uri, "key: [bad\n");

    let notification = recv_matching(&rx, HANDSHAKE_TIMEOUT, |m| is_diagnostics_for(m, uri))
        .expect("timed out waiting for publishDiagnostics");
    let diagnostics = notification["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");

    assert!(!diagnostics.is_empty(), "expected at least one diagnostic");
    assert!(
        diagnostics
            .iter()
            .any(|d| d["severity"] == 1 && d["code"] == "yamlSyntax"),
        "expected a yamlSyntax error diagnostic, got: {diagnostics:?}"
    );

    shutdown_and_exit(&mut server, &mut stdin, &rx);
}

#[test]
fn stdio_smoke_reports_no_diagnostics_for_valid_yaml() {
    let (mut server, mut stdin, rx) = spawn_server();
    let uri = "file:///smoke/good.yaml";
    initialize_and_open(&mut stdin, &rx, uri, "key: value\n");

    // Either no publishDiagnostics arrives for this URI, or one arrives with
    // an empty diagnostics array — both mean "no error reported". Which one
    // the server does is an implementation detail this smoke test does not
    // pin down.
    if let Some(notification) = recv_matching(&rx, ABSENCE_TIMEOUT, |m| is_diagnostics_for(m, uri))
    {
        let diagnostics = notification["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array");
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for valid YAML, got: {diagnostics:?}"
        );
    }

    shutdown_and_exit(&mut server, &mut stdin, &rx);
}
