**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-12

## Goal

Declare the VS Code extension as a workspace extension and
configure its untrusted-workspace capabilities so it works
correctly in remote development scenarios (SSH, WSL,
Codespaces, Dev Containers).

## Context

- The extension spawns a Rust language server binary and
  communicates over LSP via `vscode-languageclient/node` —
  it must run on the same machine as the workspace files.
- Without an explicit `extensionKind`, VS Code uses
  heuristics that may place the extension on the wrong
  host in remote configurations.
- The extension already handles untrusted workspaces at
  runtime (`server.ts:39-40` ignores custom `server.path`
  when untrusted, `main.ts:64-75` restarts on trust
  grant), but the `package.json` never declares this
  capability — so VS Code may disable the extension
  entirely in untrusted workspaces, making the runtime
  handling dead code.
- rust-analyzer uses the same pattern: `extensionKind:
  ["workspace"]` plus a `capabilities.untrustedWorkspaces`
  declaration.
- Reference: https://code.visualstudio.com/api/advanced-topics/remote-extensions

### Files involved

- `rlsp-yaml/integrations/vscode/package.json` — the only
  file that needs changes

## Steps

- [x] Research remote extension requirements (VS Code docs)
- [x] Audit extension source for remote compatibility
- [x] Compare with rust-analyzer reference implementation
- [ ] Add `extensionKind` and `capabilities` to package.json

## Tasks

### Task 1: Add extensionKind and capabilities to package.json

Add two top-level properties to
`rlsp-yaml/integrations/vscode/package.json`:

1. `"extensionKind": ["workspace"]` — declares the
   extension must run on the workspace host
2. `"capabilities": { "untrustedWorkspaces": { ... } }` —
   declares limited support with `rlsp-yaml.server.path`
   as a restricted configuration

Placement: `extensionKind` after the `main` field (groups
it with other extension metadata), `capabilities` after
`activationEvents` (mirrors rust-analyzer's layout).

- [ ] Add `extensionKind: ["workspace"]` after `main`
- [ ] Add `capabilities.untrustedWorkspaces` with
      `"supported": "limited"`,
      `"restrictedConfigurations": ["rlsp-yaml.server.path"]`,
      and a description
- [ ] Verify `pnpm run build` succeeds
- [ ] Verify `pnpm run lint` passes
- [ ] Verify `pnpm run test` passes

## Decisions

- **`extensionKind: ["workspace"]` not `["workspace", "ui"]`**
  — the extension spawns a native binary and accesses
  workspace files. It cannot function as a UI extension.
  No fallback needed.
- **`"supported": "limited"` not `false`** — unlike
  rust-analyzer which invokes arbitrary toolchain binaries,
  our extension only runs its own bundled binary by default.
  The only risk in untrusted workspaces is the custom
  `server.path` setting, which the code already ignores
  when untrusted.
- **No source code changes** — the extension's TypeScript
  source already uses correct APIs for remote development
  (context.extensionPath, workspace.getConfiguration,
  vscode-languageclient/node, no local-only APIs).
