// SPDX-License-Identifier: MIT
//! End-to-end schema loading over HTTP, served by a fake `CONNECT` proxy.
//!
//! The schema URL names a host on the reserved `.invalid` TLD (RFC 6761), which
//! passes the SSRF guard (not an IP literal, not `localhost`). ureq tunnels the
//! `http://` target through the `http://` proxy with `CONNECT` and leaves name
//! resolution to the proxy, so the production fetch path runs without network
//! access.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::json;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::NumberOrString;

use super::helpers::*;

const SCHEMA_HOST: &str = "schema.invalid";
const SCHEMA_URL: &str = "http://schema.invalid/schema.json";
const OBJECT_SCHEMA: &str = r#"{"type":"object","properties":{"count":{"type":"integer"}}}"#;

/// A `CONNECT` proxy that serves one canned schema for `schema.invalid:80`
/// and answers every other tunnel target with 502.
struct FakeProxy {
    addr: SocketAddr,
    /// The `CONNECT` line, then the tunneled request line and `Host` header, of
    /// each served request.
    seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl FakeProxy {
    fn start(content_type: Option<&'static str>, body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake proxy");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("local addr");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let (thread_seen, thread_stop) = (seen.clone(), stop.clone());
        std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = serve_connection(stream, content_type, &body, &thread_seen);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });

        Self { addr, seen, stop }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("seen lock").clone()
    }
}

impl Drop for FakeProxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Read up to and including the blank line that ends an HTTP head.
fn read_head(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte)?;
        head.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&head).into_owned())
}

fn serve_connection(
    mut stream: TcpStream,
    content_type: Option<&str>,
    body: &str,
    seen: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let connect = read_head(&mut stream)?;
    let connect_line = connect.lines().next().unwrap_or_default().to_string();
    let allowed = connect_line.starts_with(&format!("CONNECT {SCHEMA_HOST}:80 "));
    seen.lock().expect("seen lock").push(connect_line);
    if !allowed {
        return stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
    }
    stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;

    let request = read_head(&mut stream)?;
    let mut lines = request.lines();
    let request_line = lines.next().unwrap_or_default().to_string();
    let host = lines
        .find(|l| l.to_ascii_lowercase().starts_with("host:"))
        .unwrap_or_default()
        .to_string();
    seen.lock().expect("seen lock").extend([request_line, host]);

    let content_type = content_type.map_or(String::new(), |ct| format!("Content-Type: {ct}\r\n"));
    write!(
        stream,
        "HTTP/1.1 200 OK\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn initialize_request_with_proxy(id: i64, proxy_url: &str) -> Request {
    Request::build("initialize")
        .id(id)
        .params(json!({
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "initializationOptions": { "httpProxy": proxy_url }
        }))
        .finish()
}

/// Open a document whose `$schema` modeline names [`SCHEMA_URL`], with the
/// server configured to use `proxy_url`; return the schema-validation
/// diagnostic codes published for it.
async fn schema_diagnostic_codes(proxy_url: &str) -> Vec<NumberOrString> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request_with_proxy(1, proxy_url)).await;
    send(&mut service, initialized_notification()).await;

    let uri = "file:///test/schema-fetch.yaml";
    let text = format!("# yaml-language-server: $schema={SCHEMA_URL}\ncount: not-a-number\n");
    send(&mut service, did_open_notification(uri, &text)).await;

    service
        .inner()
        .get_diagnostics(uri)
        .expect("diagnostics should exist")
        .into_iter()
        .filter_map(|d| d.code)
        .collect()
}

fn schema_type() -> NumberOrString {
    NumberOrString::String("schemaType".to_string())
}

#[rstest]
#[case::text_plain_charset_utf8(Some("text/plain; charset=utf-8"))]
#[case::application_octet_stream(Some("application/octet-stream"))]
#[case::no_content_type(None)]
#[tokio::test]
async fn should_publish_schema_diagnostic_when_schema_served_with_non_json_content_type(
    #[case] content_type: Option<&'static str>,
) {
    let proxy = FakeProxy::start(content_type, OBJECT_SCHEMA.to_string());

    let codes = schema_diagnostic_codes(&proxy.url()).await;

    assert_eq!(codes, [schema_type()], "expected one schemaType diagnostic");
    let seen = proxy.seen();
    assert_eq!(seen[0], format!("CONNECT {SCHEMA_HOST}:80 HTTP/1.1"));
    assert_eq!(seen[1], "GET /schema.json HTTP/1.1");
    assert!(seen[2].eq_ignore_ascii_case(&format!("host: {SCHEMA_HOST}")));
}

#[tokio::test]
async fn should_publish_no_schema_diagnostics_when_proxy_serves_non_json_body() {
    let proxy = FakeProxy::start(Some("text/html"), "<html>not a schema</html>".to_string());

    let codes = schema_diagnostic_codes(&proxy.url()).await;

    assert!(codes.is_empty(), "got: {codes:?}");
    assert_eq!(
        proxy.seen().len(),
        3,
        "the schema fetch must have been tried"
    );
}

#[tokio::test]
async fn should_publish_no_schema_diagnostics_when_schema_exceeds_depth_limit() {
    // 60 levels is past MAX_JSON_DEPTH (50) but inside serde_json's own
    // recursion limit (128), so only the depth check can reject the schema.
    let deep = format!("{}{}", "[".repeat(60), "]".repeat(60));
    let schema = format!(
        r#"{{"type":"object","properties":{{"count":{{"type":"integer"}}}},"x-deep":{deep}}}"#
    );
    let proxy = FakeProxy::start(Some("text/plain; charset=utf-8"), schema);

    let codes = schema_diagnostic_codes(&proxy.url()).await;

    assert!(codes.is_empty(), "got: {codes:?}");
    assert_eq!(
        proxy.seen().len(),
        3,
        "the schema fetch must have been tried"
    );
}
