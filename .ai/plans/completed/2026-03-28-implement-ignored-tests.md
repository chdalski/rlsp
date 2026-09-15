# Implement Ignored Tests

**Repository:** root
**Status:** Completed (2026-03-28)
**Created:** 2026-03-28

## Goal

Replace four `#[ignore]` documentation-only tests with
real executable tests or appropriate alternatives. These
tests currently document architectural constraints but
don't assert anything — they occupy test namespace without
providing CI value.

## Context

- Test engineer assessment completed (2026-03-28):
  two tests should be implemented as real tests, one
  should be replaced by a clippy lint reference, and one
  should be deleted in favor of an improved source comment
- The redirect blocking test (Test 61) requires a local
  HTTP server — `tiny_http` as a dev-dependency
- The SSRF guard in `fetch_schema` blocks localhost, so
  the redirect test must call `build_agent` directly
- `clippy::await_holding_lock` is already active via the
  workspace pedantic lint config — Test 64 is redundant
- Lock poisoning (Test 65) is testable with stdlib only
- Lock ordering (Test 50) has no practical runtime test

## Steps

- [x] Implement redirect blocking test with `tiny_http` — d36e950
- [x] Replace mutex-across-await test with lint reference — dda1f1d
- [x] Implement lock poisoning test — dda1f1d
- [x] Delete lock ordering test, improve source comment — dda1f1d

## Tasks

### Task 1: Redirect blocking test

Replace the ignored Test 61 in `schema.rs` with a real
test using a local HTTP server.

**Files:** `rlsp-yaml/Cargo.toml`, `rlsp-yaml/src/schema.rs`

**Steps:**
- Add `tiny_http` to `[dev-dependencies]` in Cargo.toml
- Replace the ignored test body with:
  1. Start a `tiny_http::Server` on `127.0.0.1:0`
  2. Spawn a thread that accepts one request and responds
     with `302 Location: http://127.0.0.1:<port>/redirected`
  3. Call `build_agent(None)` to get the ureq agent
  4. Issue a GET to the server's address
  5. Assert the response is an error (redirect not followed)
- Remove the `#[ignore]` attribute
- Do NOT call `fetch_schema` — the SSRF guard blocks
  localhost. Call `build_agent` directly to test the
  agent's redirect configuration.

- [ ] Add `tiny_http` dev-dependency
- [ ] Implement redirect test with local server
- [ ] Remove `#[ignore]`
- [ ] Verify test passes with `cargo test`

### Task 2: Mutex-across-await + lock poisoning + lock ordering

Handle the remaining three tests in one commit.

**Files:** `rlsp-yaml/src/schema_validation.rs`,
`rlsp-yaml/src/completion.rs`, `rlsp-yaml/src/server.rs`

**Test 64 (mutex-across-await):** Delete the ignored test
body entirely. The `clippy::await_holding_lock` lint
(active via workspace pedantic config) already enforces
this at compile time — stronger than any runtime test.
Add a brief comment at the test's former location noting
the lint.

**Test 65 (lock poisoning):** Replace with a real test:
1. Create an `Arc<Mutex<SchemaCache>>` (or equivalent)
2. Spawn a thread that acquires the lock and panics,
   poisoning the mutex
3. Join the thread (expect error)
4. Verify that the production code's lock acquisition
   pattern (`.lock().ok()`) returns `None` without
   panicking
- Remove `#[ignore]`

**Test 50 (lock ordering):** Delete the ignored test body.
Improve the lock-ordering comment on the `Backend` struct
in `server.rs` (around line 538) to clearly document the
acquisition order. An ignored empty test provides no value
over a well-placed source comment.

- [ ] Delete Test 64, add lint reference comment
- [ ] Implement Test 65 lock poisoning test
- [ ] Delete Test 50, improve lock-ordering comment in server.rs
- [ ] Verify `cargo clippy` and `cargo test` pass

## Decisions

- **`tiny_http` over `mockito`:** Lighter weight, no async
  runtime needed, sufficient for a single redirect test.
  Added as dev-dependency only.

- **Delete rather than keep ignored tests:** An `#[ignore]`
  test with no assertions is worse than no test — it
  occupies test namespace, appears in `--ignored` runs,
  and creates false confidence. Delete and replace with
  the appropriate enforcement mechanism.

- **Clippy lint over runtime test for mutex-across-await:**
  The lint fires deterministically at compile time on
  every build. A runtime test would require injecting
  contention on async executor threads — non-deterministic
  and flaky. The lint is strictly better.

- **Comment over test for lock ordering:** No reliable
  non-flaky runtime test exists for lock ordering without
  custom lock instrumentation. A clear comment at the
  definition site is the right approach.
