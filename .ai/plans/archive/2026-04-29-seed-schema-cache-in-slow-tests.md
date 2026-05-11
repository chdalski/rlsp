**Repository:** root
**Status:** Completed (2026-04-30)
**Created:** 2026-04-29

## Goal

Eliminate the 15-second-per-test timeout penalty in two
`lsp_lifecycle.rs` tests that trigger a `$schema` modeline
fetch to `json.schemastore.org` in a no-network
environment. Pre-seed the schema cache with a minimal
`{"type": "object"}` schema so `process_schema` hits the
cache and skips the HTTP fetch entirely. Expected
improvement: ~30 seconds total saved from the test suite.

## Context

Analysis in the current session identified two tests as
the sole source of slow `lsp_lifecycle.rs` execution
(16.3s total for 107 tests, where two tests account for
~30s of blocking):

- `should_return_hover_when_schema_modeline_is_present`
  (15.5s)
- `should_exercise_schema_lookup_in_completion` (15.3s)

Both open a document with modeline
`# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json`.
The `did_open` handler calls `process_schema` →
`spawn_blocking(fetch_schema_raw)` → `ureq` with
`timeout_global = 15s`. In Docker without outbound
network, the request hangs for the full timeout. The
`#[tokio::test]` runtime blocks on drop waiting for the
pending task.

The other 105 tests complete in ~0.8s total. The
remaining schema tests are fast because they use URLs
that fail immediately or don't trigger `process_schema`.

The established pattern for avoiding network fetches in
tests is `service.inner().seed_schema_cache(url, schema)`
before `did_open`. Four other tests already use this
pattern (`should_emit_schema_yaml11_boolean_*` and the
`B3_SCHEMA_URL` completion test).

Both slow tests only assert that the response has a
`result` field — they don't inspect schema-derived
content. A minimal `{"type": "object"}` schema is
sufficient for the cache hit.

### Key files

| File | Role |
|---|---|
| `rlsp-yaml/tests/lsp_lifecycle.rs` | Test file — the only file that changes |

### References

- `seed_schema_cache` usage precedent: `lsp_lifecycle.rs`
  lines 2577, 2615, 2656, 3601
- `build_agent` timeout configuration:
  `rlsp-yaml/src/schema.rs:427-430`

## Steps

- [x] Pre-seed schema cache in two slow tests

## Tasks

### Task 1: Pre-seed schema cache in slow tests

**Commit:** `90197c9`

- [x] In `should_return_hover_when_schema_modeline_is_present`
      (~line 1559), add before `send(&mut service,
      initialize_request(1))`:
      ```
      let stub = serde_json::json!({"type": "object"});
      let schema = rlsp_yaml::schema::parse_schema(&stub)
          .expect("stub schema");
      service.inner().seed_schema_cache(
          "https://json.schemastore.org/github-workflow.json",
          schema,
      );
      ```
- [x] Same change in
      `should_exercise_schema_lookup_in_completion`
      (~line 1600).
- [x] Verify both tests still pass: `cargo test --test
      lsp_lifecycle -- should_return_hover_when_schema
      should_exercise_schema_lookup_in_completion`
- [x] Verify the full suite still passes and is faster:
      `cargo test --test lsp_lifecycle` — measured **1.29s**
      (from ~16s baseline; target was <3s).
- [x] `cargo clippy --all-targets` clean.
- [x] `cargo fmt` applied.

Acceptance: both tests pass, full `lsp_lifecycle` suite
time drops below 3s (from ~16s), no regressions. **Met:
1.29s, 107/107 passing.**

**Scope addition (accepted by reviewer):** developer
introduced a `GITHUB_WORKFLOW_SCHEMA_URL` constant
alongside the existing `CONFIGMAP_SCHEMA_URL` and
converted the modeline strings to `format!`. This
follows the existing idiom in the file and removes
duplication between the seed call and the modeline.

## Decisions

- **Minimal stub schema `{"type": "object"}`.** Both
  tests only assert response structure, not
  schema-derived content. A richer schema is unnecessary
  and would couple the tests to schema details they
  don't verify.
- **No production code changes.** The timeout
  configuration in `build_agent` is correct for real
  users. The fix is test-only.
- **No advisors needed.** Test-only change following an
  established pattern (4 other tests use the same
  `seed_schema_cache` approach). Low risk, low
  uncertainty.
