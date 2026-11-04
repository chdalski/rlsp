// SPDX-License-Identifier: MIT

//! Entry point for the `rlsp-yaml` language server binary.

use std::io;
use std::panic;
use std::thread;

use rlsp_yaml::server::Backend;
use tokio::io::{stdin, stdout};
use tokio::runtime::{Builder, Runtime};
use tower_lsp::{LspService, Server};

/// Stack size of the thread that drives the server and of the runtime's
/// blocking threads. The handlers run on the thread that calls `block_on`, and
/// the platform's main thread is too small on some targets (Windows MSVC), so
/// the server brings its own thread instead of relying on it. The blocking
/// threads run the recursive schema parsing.
const SERVER_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Upper bound for the runtime's blocking pool. Every blocking thread reserves
/// `SERVER_STACK_SIZE`. tower-lsp handles at most four messages at once, each
/// handler awaits its blocking task before it starts another, and stdio holds
/// one thread per direction; the rest is headroom for blocking tasks that keep
/// running after their request was canceled.
const MAX_BLOCKING_THREADS: usize = 16;

fn main() -> io::Result<()> {
    run_with_stack(SERVER_STACK_SIZE, || {
        build_runtime(SERVER_STACK_SIZE)?.block_on(serve());
        Ok(())
    })?
}

async fn serve() {
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin(), stdout(), socket).serve(service).await;
}

/// Runs `f` on a named thread with `stack_size` bytes of stack and returns its
/// value. A panic on that thread is re-raised with its original payload, so
/// the process ends as it would have if `f` had run on the calling thread.
fn run_with_stack<T: Send + 'static>(
    stack_size: usize,
    f: impl FnOnce() -> T + Send + 'static,
) -> io::Result<T> {
    let handle = thread::Builder::new()
        .name("rlsp-yaml-server".to_owned())
        .stack_size(stack_size)
        .spawn(f)
        .map_err(|e| with_context("failed to start the server thread", &e))?;
    Ok(handle
        .join()
        .unwrap_or_else(|payload| panic::resume_unwind(payload)))
}

/// Builds a runtime that drives its tasks on the calling thread. Its blocking
/// threads get `stack_size` bytes of stack.
fn build_runtime(stack_size: usize) -> io::Result<Runtime> {
    Builder::new_current_thread()
        .enable_all()
        .thread_stack_size(stack_size)
        .max_blocking_threads(MAX_BLOCKING_THREADS)
        .build()
        .map_err(|e| with_context("failed to build the async runtime", &e))
}

fn with_context(context: &str, error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;

    use tokio::runtime::{Handle, RuntimeFlavor};

    use super::*;

    /// Recurses until it has used about `bytes` of stack, so a thread that is
    /// smaller than that aborts the test binary.
    fn use_stack(bytes: usize) -> usize {
        const FRAME: usize = 4096;
        let frame = [0u8; FRAME];
        black_box(&frame);
        if bytes <= FRAME {
            0
        } else {
            1 + use_stack(bytes - FRAME)
        }
    }

    #[test]
    fn run_with_stack_returns_the_closure_value_on_a_named_thread() {
        let (value, name) = run_with_stack(SERVER_STACK_SIZE, || {
            (42, thread::current().name().map(str::to_owned))
        })
        .unwrap();

        assert_eq!(value, 42);
        assert!(name.is_some_and(|name| !name.is_empty()));
    }

    #[test]
    fn run_with_stack_resumes_a_panic_with_the_original_payload() {
        let result = panic::catch_unwind(|| run_with_stack(SERVER_STACK_SIZE, || panic!("boom")));

        let payload = result.unwrap_err();
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"boom"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn run_with_stack_reports_a_failed_thread_start_as_an_error() {
        let result = run_with_stack(usize::MAX, || ());

        assert!(result.is_err());
    }

    #[test]
    fn server_thread_stack_holds_more_than_the_default_two_mib() {
        let depth = run_with_stack(SERVER_STACK_SIZE, || use_stack(SERVER_STACK_SIZE / 2)).unwrap();

        assert!(depth > 0);
    }

    #[test]
    fn runtime_is_single_threaded() {
        let flavor = build_runtime(SERVER_STACK_SIZE)
            .unwrap()
            .block_on(async { Handle::current().runtime_flavor() });

        assert_eq!(flavor, RuntimeFlavor::CurrentThread);
    }

    #[test]
    fn runtime_blocking_threads_get_the_requested_stack_size() {
        let depth = build_runtime(SERVER_STACK_SIZE).unwrap().block_on(async {
            tokio::task::spawn_blocking(|| use_stack(SERVER_STACK_SIZE / 2))
                .await
                .unwrap()
        });

        assert!(depth > 0);
    }
}
