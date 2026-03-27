**Repository:** root
**Status:** InProgress
**Created:** 2026-03-27

## Goal

Allow users to configure an HTTP proxy for schema fetching,
supporting corporate environments behind firewalls where
direct HTTPS access to schema hosts is blocked.

## Context

- Two functions in `schema.rs` make HTTP requests:
  `fetch_schema` (line 275) and `fetch_schemastore_catalog`
  (line 331). Both construct a `ureq::Agent` inline.
- `ureq` v3 supports proxy configuration via
  `Agent::config_builder().proxy(...)`.
- The proxy URL needs to flow from `Settings` → `Backend` →
  the fetch functions. Currently the fetch functions take
  only a URL string; they'll need access to the proxy
  config.
- Key files: `server.rs` (settings, passing proxy to
  fetch), `schema.rs` (agent construction),
  `configuration.md`, `feature-log.md`.

## Steps

- [ ] Add `httpProxy` setting and proxy-aware agent
- [ ] Wire proxy into all fetch call sites
- [ ] Write tests
- [ ] Update documentation

## Tasks

### Task 1: Proxy-aware HTTP agent

Refactor HTTP fetching in `schema.rs` to support an
optional proxy.

Files: `rlsp-yaml/src/schema.rs`, `rlsp-yaml/src/server.rs`

- [ ] Add `http_proxy: Option<String>` to `Settings`
- [ ] Add `get_http_proxy()` helper on `Backend`
- [ ] Add a helper function in `schema.rs`:
      `fn build_agent(proxy: Option<&str>) -> ureq::Agent`
      that creates an agent with `max_redirects(0)` and
      optionally configures the proxy. Both `fetch_schema`
      and `fetch_schemastore_catalog` call this instead of
      constructing agents inline.
- [ ] Update `fetch_schema` signature to accept an optional
      proxy: `fetch_schema(url: &str, proxy: Option<&str>)`
- [ ] Update `fetch_schemastore_catalog` signature similarly
- [ ] Update all call sites in `server.rs`:
      `process_schema` and `get_or_fetch_schemastore_catalog`
      to pass the proxy from settings
- [ ] Unit test: `build_agent` with and without proxy
      (verify agent is constructed without panic)

### Task 2: Documentation

Files: `rlsp-yaml/docs/configuration.md`,
`rlsp-yaml/docs/feature-log.md`

- [ ] Add `httpProxy` setting to configuration.md:
      Type `string` (optional), default `null` (no proxy).
      Format: `http://host:port` or `https://host:port`
- [ ] Update example JSON and Neovim example
- [ ] Mark "Proxy Support for Schema Fetching" as
      `[completed]` in feature-log.md

## Decisions

- **Single proxy setting:** One `httpProxy` URL for all
  schema fetching (both individual schemas and the
  SchemaStore catalog). No separate HTTPS proxy setting —
  most corporate proxies handle both.
- **Passed to fetch functions:** The proxy URL is threaded
  from settings through to the agent builder. This avoids
  global state and keeps the fetch functions testable.
- **`ureq` proxy API:** `ureq` v3 supports
  `config_builder().proxy(Proxy::new(url))`. Check the
  exact API at implementation time.
