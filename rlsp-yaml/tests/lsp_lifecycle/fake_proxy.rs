// SPDX-License-Identifier: MIT
//! A fake `CONNECT` proxy that serves canned schema responses.
//!
//! The schema URLs name a host on the reserved `.invalid` TLD (RFC 6761), which
//! passes the SSRF guard (not an IP literal, not `localhost`). ureq tunnels the
//! `http://` target through the `http://` proxy with `CONNECT` and leaves name
//! resolution to the proxy, so the production fetch path runs without network
//! access.
//!
//! Shared by the `lsp_lifecycle` tests and `tests/schema_memory.rs`.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const SCHEMA_HOST: &str = "schema.invalid";

/// The canned response served for the tunneled `GET`.
pub struct Reply {
    /// Status code and reason phrase, e.g. `404 Not Found`.
    pub status: &'static str,
    pub content_type: Option<&'static str>,
    pub body: String,
}

impl Reply {
    pub const fn ok(content_type: Option<&'static str>, body: String) -> Self {
        Self {
            status: "200 OK",
            content_type,
            body,
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: "404 Not Found",
            content_type: Some("text/plain"),
            body: "not found".to_string(),
        }
    }
}

/// A `CONNECT` proxy that serves a canned [`Reply`] per request path for
/// `schema.invalid:80` and answers every other tunnel target with 502.
pub struct FakeProxy {
    addr: SocketAddr,
    /// The `CONNECT` line, then the tunneled request line and `Host` header, of
    /// each served request.
    seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl FakeProxy {
    /// Serve `reply` for every path.
    pub fn start(reply: Reply) -> Self {
        Self::start_routes(vec![("*", reply)])
    }

    /// Serve the reply of the route whose path matches the request, or the one
    /// registered for `*`; any other path gets 404.
    pub fn start_routes(mut routes: Vec<(&'static str, Reply)>) -> Self {
        routes.push(("*", Reply::not_found()));
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
                        let _ = serve_connection(stream, &routes, &thread_seen);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });

        Self { addr, seen, stop }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn seen(&self) -> Vec<String> {
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
    routes: &[(&str, Reply)],
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
    let path = request_line.split_whitespace().nth(1).unwrap_or_default();
    let (_, reply) = routes
        .iter()
        .find(|(route, _)| *route == path || *route == "*")
        .expect("start_routes registers a fallback route");
    let Reply {
        status,
        content_type,
        body,
    } = reply;
    seen.lock().expect("seen lock").extend([request_line, host]);
    let content_type = content_type.map_or(String::new(), |ct| format!("Content-Type: {ct}\r\n"));
    write!(
        stream,
        "HTTP/1.1 {status}\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
