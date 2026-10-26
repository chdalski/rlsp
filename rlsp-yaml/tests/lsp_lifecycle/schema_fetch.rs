// SPDX-License-Identifier: MIT
//! End-to-end schema loading and failure logging over HTTP, served by a fake
//! `CONNECT` proxy.
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

use rstest::rstest;
use serde_json::json;
use tower_lsp::lsp_types::NumberOrString;

use super::helpers::*;

const SCHEMA_HOST: &str = "schema.invalid";
const SCHEMA_URL: &str = "http://schema.invalid/schema.json";
const OBJECT_SCHEMA: &str = r#"{"type":"object","properties":{"count":{"type":"integer"}}}"#;

/// The canned response served for the tunneled `GET`.
struct Reply {
    /// Status code and reason phrase, e.g. `404 Not Found`.
    status: &'static str,
    content_type: Option<&'static str>,
    body: String,
}

impl Reply {
    const fn ok(content_type: Option<&'static str>, body: String) -> Self {
        Self {
            status: "200 OK",
            content_type,
            body,
        }
    }

    fn not_found() -> Self {
        Self {
            status: "404 Not Found",
            content_type: Some("text/plain"),
            body: "not found".to_string(),
        }
    }
}

/// A `CONNECT` proxy that serves one canned [`Reply`] for `schema.invalid:80`
/// and answers every other tunnel target with 502.
struct FakeProxy {
    addr: SocketAddr,
    /// The `CONNECT` line, then the tunneled request line and `Host` header, of
    /// each served request.
    seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl FakeProxy {
    fn start(reply: Reply) -> Self {
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
                        let _ = serve_connection(stream, &reply, &thread_seen);
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
    reply: &Reply,
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

    let Reply {
        status,
        content_type,
        body,
    } = reply;
    let content_type = content_type.map_or(String::new(), |ct| format!("Content-Type: {ct}\r\n"));
    write!(
        stream,
        "HTTP/1.1 {status}\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn proxy_options(proxy_url: &str) -> serde_json::Value {
    json!({ "httpProxy": proxy_url })
}

/// Open a document whose `$schema` modeline names [`SCHEMA_URL`], with the
/// server configured to use `proxy_url`; return the schema-validation
/// diagnostic codes published for it.
async fn schema_diagnostic_codes(proxy_url: &str) -> Vec<NumberOrString> {
    let (mut service, _log) = start_server(proxy_options(proxy_url)).await;

    let uri = "file:///test/schema-fetch.yaml";
    open_with_modeline(&mut service, uri, SCHEMA_URL).await;

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
    let proxy = FakeProxy::start(Reply::ok(content_type, OBJECT_SCHEMA.to_string()));

    let codes = schema_diagnostic_codes(&proxy.url()).await;

    assert_eq!(codes, [schema_type()], "expected one schemaType diagnostic");
    let seen = proxy.seen();
    assert_eq!(seen[0], format!("CONNECT {SCHEMA_HOST}:80 HTTP/1.1"));
    assert_eq!(seen[1], "GET /schema.json HTTP/1.1");
    assert!(seen[2].eq_ignore_ascii_case(&format!("host: {SCHEMA_HOST}")));
}

#[tokio::test]
async fn should_publish_no_schema_diagnostics_when_proxy_serves_non_json_body() {
    let proxy = FakeProxy::start(Reply::ok(
        Some("text/html"),
        "<html>not a schema</html>".to_string(),
    ));

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
    let proxy = FakeProxy::start(Reply::ok(Some("text/plain; charset=utf-8"), schema));

    let codes = schema_diagnostic_codes(&proxy.url()).await;

    assert!(codes.is_empty(), "got: {codes:?}");
    assert_eq!(
        proxy.seen().len(),
        3,
        "the schema fetch must have been tried"
    );
}

// ---- failure logging ----

#[rstest]
#[case::loopback_host("http://localhost/schema.json")]
#[case::disallowed_scheme("ftp://schema.invalid/schema.json")]
#[tokio::test]
async fn should_warn_once_when_modeline_url_is_rejected(#[case] schema_url: &str) {
    let proxy = FakeProxy::start(Reply::not_found());
    let (mut service, mut log) = start_server(proxy_options(&proxy.url())).await;

    open_with_modeline(&mut service, "file:///test/rejected.yaml", schema_url).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "got: {warnings:?}");
    assert!(warnings[0].contains(schema_url), "got: {warnings:?}");
    assert!(warnings[0].contains("not permitted"), "got: {warnings:?}");
    assert!(
        proxy.seen().is_empty(),
        "a rejected URL must not be fetched"
    );
}

#[tokio::test]
async fn should_warn_on_every_failed_modeline_fetch() {
    let proxy = FakeProxy::start(Reply::not_found());
    let (mut service, mut log) = start_server(proxy_options(&proxy.url())).await;

    open_with_modeline(&mut service, "file:///test/first.yaml", SCHEMA_URL).await;
    open_with_modeline(&mut service, "file:///test/second.yaml", SCHEMA_URL).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 2, "got: {warnings:?}");
    for warning in &warnings {
        assert!(warning.contains(SCHEMA_URL), "got: {warning}");
        assert!(warning.contains("Fetch failed"), "got: {warning}");
    }
    let connects = proxy
        .seen()
        .iter()
        .filter(|line| line.starts_with("CONNECT "))
        .count();
    assert_eq!(connects, 2, "a failed fetch must be retried, not cached");
}

#[tokio::test]
async fn should_warn_when_schemas_setting_url_is_rejected() {
    let schema_url = "http://localhost/schema.json";
    let (mut service, mut log) =
        start_server(json!({ "schemas": { schema_url: "**/*.yaml" } })).await;

    send(
        &mut service,
        did_open_notification("file:///test/settings.yaml", "key: value\n"),
    )
    .await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(warnings.len(), 1, "got: {warnings:?}");
    assert!(warnings[0].contains(schema_url), "got: {warnings:?}");
    assert!(warnings[0].contains("not permitted"), "got: {warnings:?}");
}

#[tokio::test]
async fn should_not_warn_when_kubernetes_schema_fetch_fails() {
    // The proxy answers every tunnel target but `schema.invalid:80` with 502,
    // so the fetch of the Kubernetes schema URL fails without network access.
    let proxy = FakeProxy::start(Reply::not_found());
    let (mut service, mut log) = start_server(proxy_options(&proxy.url())).await;

    let custom_resource = "apiVersion: example.com/v1\nkind: Widget\nmetadata:\n  name: w\n";
    send(
        &mut service,
        did_open_notification("file:///test/widget.yaml", custom_resource),
    )
    .await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert!(
        proxy
            .seen()
            .contains(&"CONNECT raw.githubusercontent.com:443 HTTP/1.1".to_string()),
        "the Kubernetes schema fetch must have been tried, saw: {:?}",
        proxy.seen()
    );
    assert!(warnings.is_empty(), "got: {warnings:?}");
}

#[tokio::test]
async fn should_not_warn_when_schema_fetch_succeeds_or_is_cached() {
    let proxy = FakeProxy::start(Reply::ok(
        Some("application/json"),
        OBJECT_SCHEMA.to_string(),
    ));
    let (mut service, mut log) = start_server(proxy_options(&proxy.url())).await;

    open_with_modeline(&mut service, "file:///test/first.yaml", SCHEMA_URL).await;
    assert_eq!(proxy.seen().len(), 3, "the first document must fetch");
    open_with_modeline(&mut service, "file:///test/second.yaml", SCHEMA_URL).await;
    let warnings = warnings_before_sentinel(&mut service, &mut log).await;

    assert_eq!(
        proxy.seen().len(),
        3,
        "the second document must hit the cache"
    );
    assert!(warnings.is_empty(), "got: {warnings:?}");
}
