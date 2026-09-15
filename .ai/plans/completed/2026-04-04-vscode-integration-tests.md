**Repository:** root
**Status:** Completed (2026-04-04)
**Created:** 2026-04-04

## Goal

Add VS Code integration tests for the extension using `@vscode/test-electron`
and `@vscode/test-cli`. Tests run inside a real VS Code instance and verify
extension activation, command registration, client lifecycle, and configuration
propagation. This complements the existing 28 vitest unit tests which cover
extracted pure logic.

## Context

- Existing unit tests: `server.test.ts` (21 tests), `status.test.ts` (7 tests)
  run via vitest — they test pure functions without the `vscode` module
- Integration tests need `@vscode/test-electron` which downloads and launches a
  headless VS Code instance
- The `@vscode/test-cli` provides the test runner configuration (`.vscode-test.mjs`)
- Integration tests use mocha (VS Code's test framework convention), not vitest
- The extension depends on an `rlsp-yaml` binary for full LSP client testing —
  some tests may need to handle the binary being absent gracefully
- Linux CI requires `xvfb` for the headless display (VS Code needs a display server)

## Steps

- [x] Add integration test infrastructure (b4c3f17)
- [x] Write integration test suite (f9fa0cc)
- [x] Add integration test script and update CI (11ddcc4)

## Tasks

### Task 1: Integration test infrastructure

Set up the test runner and configuration. No test cases yet.

- [x] Add dev dependencies: `@vscode/test-cli`, `@vscode/test-electron`, `mocha`,
  `@types/mocha`
- [x] Create `.vscode-test.mjs` config at `rlsp-yaml/integrations/vscode/`:
  ```javascript
  import { defineConfig } from '@vscode/test-cli';

  export default defineConfig({
    files: 'out/test/integration/**/*.test.js',
    mocha: { timeout: 20000 },
  });
  ```
- [x] Create `src/test/integration/` directory for integration tests
- [x] Create `src/test/integration/index.ts` — mocha test runner entry point
- [x] Add `"test:integration"` script to `package.json`:
  `"test:integration": "tsc && vscode-test"`
- [x] Update `tsconfig.json` `include` to cover `src/test/` if needed
- [x] Ensure `pnpm run test` (vitest) and `pnpm run test:integration`
  (vscode-test) are separate — unit tests stay fast, integration tests
  are opt-in

### Task 2: Write integration test suite

Write the actual test cases inside the VS Code extension host.

- [x] `src/test/integration/activation.test.ts` — extension lifecycle:
  - Extension activates when a YAML file is opened
  - Extension exports `activate` and `deactivate`
  - Extension is listed in active extensions after activation
- [x] `src/test/integration/commands.test.ts` — command registration:
  - Deferred per test engineer — commands register after `lc.start()`,
    which requires the binary. Testing absent commands tests the error
    path, not the feature.
- [x] `src/test/integration/configuration.test.ts` — settings:
  - All `rlsp-yaml.*` settings are defined in the configuration
  - Settings have correct default values
  - Settings are readable via `workspace.getConfiguration('rlsp-yaml')`
- [x] Handle missing binary gracefully — the integration tests run
  without a compiled rlsp-yaml binary (no server/ directory). Tests
  verify extension activation and command registration work
  even when the LSP client fails to start.

### Task 3: Add integration test script and update CI

Wire up the integration tests to run locally and in CI.

- [x] Add `xvfb-run` wrapper for Linux CI to provide a headless display
- [x] Add integration test step to `.github/workflows/vscode-extension.yml`
  (only on linux-x64 runner — no need to run in all matrix entries):
  ```yaml
  - name: Run integration tests (Linux)
    if: matrix.os == 'ubuntu-latest' && matrix.target == 'x86_64-unknown-linux-gnu'
    working-directory: rlsp-yaml/integrations/vscode
    run: xvfb-run -a pnpm run test:integration
  ```
- [x] Verify integration tests pass locally and document how to run them

## Decisions

- **Test framework:** mocha via `@vscode/test-cli` — this is the VS Code
  standard. Vitest cannot run inside the extension host.
- **Separate from unit tests:** `pnpm run test` stays vitest (fast, no VS Code
  needed). `pnpm run test:integration` runs the VS Code integration tests.
- **CI platform:** Linux-only for integration tests — macOS/Windows CI runners
  support it but add cost with minimal additional coverage. The extension host
  behavior is platform-independent.
- **Binary absence:** Integration tests must handle the server binary not being
  present — they test extension UI/lifecycle, not LSP features.
