**Repository:** root
**Status:** InProgress
**Created:** 2026-10-29

# VS Code Settings Reach the Server, and `vscode-languageclient` 11

## Goal

Dependabot PR #94 updates the VS Code extension's dependencies,
`vscode-languageclient` 11 among them, and fails lint on the client's
deprecated `synchronize.configurationSection` — an option that sends
the settings nested under `rlsp-yaml`, which the server reads as all
defaults right after startup and on every change. Remove it so the
server keeps the user's settings, land the six updates on `main`, and
stop the extension's settings listener from outliving its client after
a restart.

## Context

- `rlsp-yaml/integrations/vscode/src/client.ts`: `createLanguageClient`
  sets `synchronize.configurationSection: 'rlsp-yaml'` and
  `initializationOptions: getConfig()` (the flat settings object built
  in `src/config.ts`), and registers its own
  `workspace.onDidChangeConfiguration` listener that sends
  `workspace/didChangeConfiguration` with `getConfig()`.
- `vscode-languageclient` 10.1.2 (`lib/common/configuration.js`): with
  `configurationSection` set, the library sends
  `{"settings": {"rlsp-yaml": {...}}}` once after initialization and on
  every change of the section; the server (`rlsp-yaml/src/settings.rs`)
  ignores the unknown key `rlsp-yaml`. This is read from the library
  source; no running editor has shown it yet.
- Tests: vitest unit tests in `src/*.test.ts` (`pnpm run test`);
  VS Code integration tests in `src/test/integration/`
  (`pnpm run test:integration`), which need the server binary under
  `server/<target>/` and a display; CI runs them on linux-x64.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: the server keeps the extension's settings
- [x] Task 2: the dependency updates of PR #94 are on `main`
- [x] Task 3: the settings listener lives exactly as long as its client
- [ ] Push to `main`, confirm the full CI matrix is green, and confirm
      PR #94 is closed

## Tasks

### Task 1: The server keeps the extension's settings

Remove the option that sends the nested payload, so the flat settings
from `initializationOptions` and from the extension's own listener are
the only settings the server receives. Runs on the current
dependencies.

- [x] The client options have no `synchronize` block, and a comment at
      the extension's settings listener says why the extension sends the
      flat settings itself
- [x] A unit test in `src/client.test.ts` pins that the client options
      have no `synchronize`, that the settings the client sends at
      initialize are the flat `getConfig()` result, that a change of an
      `rlsp-yaml` setting sends exactly one
      `workspace/didChangeConfiguration` whose `settings` is the flat
      object with no `rlsp-yaml` key, and that a change of another
      section sends nothing
- [x] An integration test changes `rlsp-yaml.formatSingleQuote` in a
      running editor, pins that the server then formats `key: "v"` as
      `key: 'v'`, and restores the setting; run against the code before
      this task it fails, and after it passes
- [x] `rlsp-yaml/docs/feature-log.md` has an entry that tells the reader
      that the VS Code extension's settings now stay in effect after
      startup and after a change
- [x] Typecheck, lint, format, unit tests, and integration tests pass

### Task 2: Land the dependency updates of PR #94

Bring `main` to the dependency versions of PR #94; with Task 1 in place
the deprecated option no longer fails lint.

- [x] `package.json` and `pnpm-lock.yaml` carry
      `vscode-languageclient` 11.0.0, `@vitest/coverage-v8` and `vitest`
      5.0.5, `eslint` 10.13.0, `typescript-eslint` 8.72.0, and `vite`
      8.3.5
- [x] The VS Code version that `vscode-languageclient` 11.0.0 requires —
      at runtime (`REQUIRED_VSCODE_VERSION` in its `lib/node/main.js`)
      and in its `engines.vscode` — is satisfied by every version the
      extension's `engines.vscode` (`^1.140.0`) admits
- [x] Typecheck, lint, format, audit, unit tests, and integration tests
      pass

### Task 3: The settings listener lives exactly as long as its client

`src/main.ts` creates a new client on "Restart Server" and on a
workspace trust grant, through `makeRestartServer` in
`src/commands.ts`. A client's settings listener is never disposed
today, so after a restart the old listener keeps sending to its stopped
client and each settings change raises an unhandled rejection.

- [x] A client's settings listener is disposed once that client's
      `stop()` has settled with the client in the Stopped state, whether
      `stop()` resolved or rejected (shutdown timeout, connection
      error); it stays registered when `stop()` rejects with the client
      Starting or Running, and across the library's restart of the
      server after a crash
- [x] A client's settings listener is disposed when the client enters
      the StartFailed state, from which the library does not restart it
- [x] The settings the client sends at initialize are read at each
      initialize, so a client that the library restarts after a crash
      sends the current settings
- [x] A settings notification that fails is written to the extension's
      output channel instead of becoming an unhandled rejection
- [x] Unit tests in `src/client.test.ts` pin each outcome above,
      including that the listener is not disposed without `stop()`, and
      that the initialization options return the current settings on
      each call; the test of Task 1 that pins the settings sent at
      initialize reads them through the new form and still pins the
      flat `getConfig()` result
- [x] The restart tests in `src/commands.test.ts` pass with their
      assertions unchanged
- [x] Typecheck, lint, format, unit tests, and integration tests pass

## Non-Goals

- Marking the values the server rejects in the VS Code settings schema
  of `package.json` (a queued plan of its own)
- Settings per workspace folder in a multi-root workspace — unsupported
  today, and still unsupported

## Decisions

- **Remove `configurationSection`:** the extension already sends the
  flat settings at startup and on every change. Declined: the pull model
  (the server requests `workspace/configuration` — a server change that
  reaches every integration) and suppressing the lint (keeps the nested
  payload).
- **Land on `main`:** the updates and fixes land as commits on `main`;
  Dependabot closes PR #94 once `main` carries its updates.
- **Order:** the fix lands on the current dependencies, so every commit
  passes lint; the listener lifecycle comes after the update, so it is
  written against `vscode-languageclient` 11.
- **If `vscode-languageclient` 11 requires a VS Code newer than any
  version `^1.140.0` admits,** raising `engines.vscode` is the user's
  decision, and Task 2 stops there until the user decides.
- **One listener per client, disposed inside that client's `stop()`:** a
  small `LanguageClient` subclass in `client.ts` disposes the listener
  once `stop()` has settled — resolved or rejected — and only when the
  client is then in the Stopped state, which also covers a shutdown
  that timed out. Not on a change to Stopped: a crash restart reports
  the client stopped without `stop()` and keeps running. A client whose
  start failed is the exception: no caller keeps it to call `stop()`,
  and in `vscode-languageclient` 11 StartFailed is a public state that
  the library never restarts from, so entering it disposes the
  listener. Not in the restart
  path or `deactivate`: that spreads disposal over three call sites.
  Declined: one listener in `activate` that sends to the current client
  — a change during the restart window is lost.
- **`initializationOptions` is `getConfig` itself:** the library calls it
  at every initialize, crash restarts included.
- **Behavior that changes besides the fix:** a settings change while a
  client is stopping, or after the library gave up restarting the
  server, is logged instead of raising an unhandled rejection (the
  change is dropped either way); a change between client creation and
  initialize now also reaches the server in the initialize request (the
  end state is unchanged).
