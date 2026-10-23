**Repository:** root
**Status:** NotStarted
**Created:** 2026-10-23

# Load Remote Schemas Regardless of Content-Type

## Goal

rlsp-yaml silently drops every JSON schema whose server
does not answer with a JSON `Content-Type` — which includes
`raw.githubusercontent.com` and GitLab raw URLs, so schema
validation, hover, and completion never work for those
schemas (GitHub issue #89). Accept any `Content-Type` and
let the existing size, parse, and depth checks decide
whether a response is a schema, and tell the user in the
LSP log why a schema named by a modeline, the `schemas`
setting, or SchemaStore could not be applied.

## Context

- Issue: https://github.com/chdalski/rlsp/issues/89
- `fetch_schema_raw` in `rlsp-yaml/src/schema.rs` rejects
  any response whose `Content-Type` contains neither
  `application/json` nor `application/schema`, before
  reading the body. `raw.githubusercontent.com` and GitLab
  `/-/raw/` URLs serve `text/plain; charset=utf-8`. Every
  schema source goes through this function: modelines,
  the `schemas` setting, SchemaStore catalog matches,
  Kubernetes auto-detection (all its schemas are on
  `raw.githubusercontent.com`), and remote `$ref`
  resolution.
- `validate_and_normalize_url` is the SSRF guard: it
  checks only the target URL and rejects loopback and
  private targets; the configured `httpProxy` is not
  checked.
- Test seam: ureq 3.4.2 tunnels an `http://` target
  through an `http://` proxy with `CONNECT` and leaves
  target-host resolution to the proxy. A fake proxy on
  `127.0.0.1` configured via `httpProxy`, with a target on
  the reserved `.invalid` TLD (RFC 6761), therefore
  exercises the production fetch path through the SSRF
  guard without network access.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: load schemas regardless of Content-Type
- [ ] Task 2: log why a schema could not be applied
- [ ] Push to `main` and confirm the full CI matrix is green
- [ ] Comment on issue #89 with what was fixed (including
      Kubernetes auto-detection) and that redirecting
      URLs such as GitHub release downloads still fail

## Tasks

### Task 1: Load schemas regardless of Content-Type

Schema fetches accept any `Content-Type`; a response is a
schema when it passes the existing bounds and parses.
`fetch_schemastore_catalog`, in the same file, already
performs the same size-bounded read and JSON parse without
a `Content-Type` check — the two copies drifting apart is
what caused this bug. `build_agent` disables redirect
following (`max_redirects(0)`), so ureq returns a 3xx
response as a success.

- [x] A schema served with `Content-Type: text/plain;
      charset=utf-8`, with `application/octet-stream`, and
      without a `Content-Type` header is loaded
- [x] A non-JSON body fails with `SchemaError::ParseFailed`;
      the SSRF guard, the `MAX_SCHEMA_BYTES` cap, and the
      JSON depth limit reject the same inputs as before
- [x] A 3xx response fails with `SchemaError::FetchFailed`
      whose message names the HTTP status and states that
      redirects are not followed
- [x] The schema fetch and the catalog fetch share one
      implementation of request, bounded body read, and
      JSON parse
- [x] `SchemaError::UnexpectedContentType`,
      `sanitize_content_type`, and every test and comment
      that describes `Content-Type` validation are gone;
      the fetch functions' doc comments name the errors
      they return
- [x] Unit tests exercise the production fetch code, not a
      re-implementation of it, for: the three
      `Content-Type` cases above, a non-JSON body, a body
      of exactly `MAX_SCHEMA_BYTES`, a body one byte over
      the cap, and a 3xx response
- [x] An integration test in `rlsp-yaml/tests/lsp_lifecycle/`
      opens a document whose `$schema` modeline points at
      a schema served as `text/plain; charset=utf-8`
      through the configured `httpProxy` (see the test
      seam in Context), asserts that the published
      diagnostics contain a schema-validation diagnostic
      for the document, and asserts that the fake proxy
      received the request
- [x] Build, tests, clippy, and fmt pass

### Task 2: Log why a schema could not be applied

When a schema named by a modeline, the `schemas` setting,
or a SchemaStore catalog match cannot be applied, the user
sees the reason in the LSP log, on every failed attempt.
Today `Backend::process_schema` in `rlsp-yaml/src/server.rs`
drops both URL-validation and fetch errors without a
trace, while `get_or_fetch_schemastore_catalog` already
logs catalog fetch failures as a `window/logMessage`
Warning. Kubernetes auto-detection builds a schema URL for
every document with a root `apiVersion` and `kind`,
including custom resources, for which no such schema
exists and the fetch fails with HTTP 404.

- [ ] When a schema URL from a modeline, the `schemas`
      setting, or a SchemaStore match is rejected by URL
      validation (e.g. `http://localhost/...`,
      `file://...`), the client receives a
      `window/logMessage` of type Warning whose message
      contains the URL and the rejection reason
- [ ] When a schema fetch for a URL from one of these
      sources fails, the client receives a
      `window/logMessage` of type Warning whose message
      contains the URL and the error
- [ ] A failed fetch of a Kubernetes auto-detected schema
      URL, a successful fetch, and a cache hit send no
      Warning
- [ ] Integration tests in `rlsp-yaml/tests/lsp_lifecycle/`
      collect client-bound messages and assert: the
      Warning for a rejected modeline URL, the Warning for
      a failed modeline fetch served through the
      configured `httpProxy` (e.g. an HTTP 404), and no
      Warning for a custom-resource manifest whose
      Kubernetes schema fetch fails
- [ ] Build, tests, clippy, and fmt pass

## Non-Goals

- Following HTTP redirects — redirect following stays
  disabled as an SSRF control, so URLs that redirect (e.g.
  GitHub release downloads, which answer with a 302) keep
  failing
- Logging failures of remote `$ref` fetches made during
  schema resolution
- Caching or backing off failed schema or catalog fetches
- Moving the network-dependent tests in
  `kubernetes_detection.rs` onto the proxy test seam

## Decisions

- **Remove the check, do not widen it:** an allowlist that
  adds `text/plain` and `application/octet-stream` stays
  brittle for other hosts, and rejecting only `text/html`
  saves nothing because the JSON parse rejects HTML at the
  first byte. Neither guards against a named threat.
- **Warn on every failed attempt, no per-URL dedup:**
  matches the catalog fetch and adds no state; a dedup set
  would not help while a `$schema` URL is being typed,
  since each keystroke yields a new URL. Once schemas load,
  repeats remain only for real failures.
- **No Warning for Kubernetes auto-detection:** its URL is
  a guess; for custom resources no schema exists, which is
  expected and not something the user can fix, and with
  per-attempt logging every keystroke in such a manifest
  would add a log line. Real Kubernetes fetch failures
  (e.g. offline) stay silent as the trade-off.
- **Test through `httpProxy`:** a test-only SSRF bypass
  would weaken a security control, and an injectable
  fetcher would change `Backend`'s construction for test
  reach alone; the proxy seam needs no production hook.
- **Breaking change:** `SchemaError` is public and not
  `#[non_exhaustive]`; removing `UnexpectedContentType`
  breaks its API, so Task 1's commit is marked breaking.
- **Issue link:** Task 1's commit message carries
  `Closes #89`.
