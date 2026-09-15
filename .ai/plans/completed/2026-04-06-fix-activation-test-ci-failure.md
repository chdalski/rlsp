**Repository:** root
**Status:** Completed (2026-04-06)
**Created:** 2026-04-06

## Goal

Fix the VS Code integration test "activate() rejects when
no server binary is present" which fails in CI because the
workflow builds and copies the binary before running tests.
Also add fail-fast binary existence checking to the
extension so it gives a clear error instead of deferring
to LanguageClient process spawn failure.

## Context

- **CI failure:** The `vscode-extension.yml` workflow builds
  the Rust binary and copies it to `server/<target>/` before
  running integration tests. The test assumes no binary
  exists, but it does — so `activate()` succeeds.
- **Fragile error path:** `findServerBinary()` computes a
  path but never validates the file exists. When absent,
  the error only surfaces when `LanguageClient.start()`
  fails to spawn the process — not a clear error message.
- **Local passes:** No binary in `server/` locally, so the
  spawn failure propagates as a rejection that the test
  catches.

### Key files

- `rlsp-yaml/integrations/vscode/src/server.ts` — binary
  resolution logic (`findServerBinary`, `bundledBinaryPath`)
- `rlsp-yaml/integrations/vscode/src/test/integration/activation.test.ts`
  — the failing test
- `rlsp-yaml/integrations/vscode/src/test/unit/server.test.ts`
  — existing unit tests for server.ts (if they exist)

## Steps

- [x] Add file existence check to `bundledBinaryPath()` (bb1179b)
- [x] Skip "no binary" integration test when binary is present (bb1179b)
- [x] Verify tests pass locally and unit tests cover the new error path (bb1179b)

## Tasks

### Task 1: Add binary existence check and fix test

**main.ts changes (NOT server.ts):**
- Import `fs` (existsSync) in `main.ts`
- In `startClient()` (line 17-35), after
  `findServerBinary()` returns `binaryPath` on line 19,
  check `fs.existsSync(binaryPath)` before passing it to
  `createLanguageClient()`
- If the file doesn't exist, throw an Error with a clear
  message: `rlsp-yaml: server binary not found at
  "<path>". Install rlsp-yaml manually and set
  rlsp-yaml.server.path.`
- The check goes in `main.ts` not `server.ts` because
  `server.ts` unit tests use fake extensionPaths — adding
  existsSync there would break 14+ existing unit tests
  that assert on computed paths. The existence check is
  a runtime concern, not a path-resolution concern.

**activation.test.ts changes:**
- The "no binary" test should check whether the bundled
  binary actually exists at the expected path
- If the binary exists (CI environment), skip the test
  with `this.skip()` — the test is only meaningful when
  the binary is absent
- Use `findServerBinary` or compute the expected path to
  check for the binary's existence

**Unit test changes:**
- If `server.test.ts` exists, add a test that verifies
  `bundledBinaryPath` throws when the binary file doesn't
  exist at the computed path
- This covers the new error path with a unit test that
  doesn't depend on the CI environment

- [x] Add `fs.existsSync` check to `startClient()` in main.ts (bb1179b)
- [x] Skip integration test when binary is present (bb1179b)
- [x] Add/update unit test for missing binary error — N/A: existence check is in `main.ts` not `bundledBinaryPath`, so no new unit test in `server.test.ts`; integration test covers the error path (bb1179b)
- [x] All tests pass locally (`pnpm run test` and
      `xvfb-run -a pnpm run test:integration`) (bb1179b)

## Decisions

- **Check in main.ts, not server.ts:** `server.ts` has
  14+ unit tests that use fake extensionPaths. Adding
  `existsSync` there would break them all. The existence
  check is a runtime concern that belongs in `activate()`.
- **`existsSync` not async:** `existsSync` is appropriate
  for a one-time startup check. The surrounding function
  is async but the check is synchronous and fast.
- **Skip, not remove the test:** The "no binary" test is
  valid — it verifies the error path works. It just can't
  run when the binary is present. `this.skip()` preserves
  the test for environments where it's meaningful.
- **Check applies to all paths:** Both bundled and custom
  server paths benefit from a pre-flight existence check
  — a clear error message is better than a cryptic spawn
  failure from LanguageClient.
