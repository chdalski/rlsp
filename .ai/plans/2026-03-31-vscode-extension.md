**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-31

## Goal

Create a full-featured VS Code extension for rlsp-yaml that bundles the
language server binary, exposes all server settings, provides status bar
integration, output channel logging, and custom commands. The extension
lives in `editors/code/` following the rust-analyzer convention, uses pnpm
as the package manager, and enforces maximum TypeScript strictness to
match the project's Cargo.toml lint standards.

## Context

- **Reference:** rust-analyzer's `editors/code/` extension — similar
  architecture (LSP client wrapping a Rust binary, esbuild bundling,
  `@tsconfig/strictest`)
- **Server capabilities:** hover, completion, document symbols, folding,
  selection ranges, rename, go-to-definition, references, document links,
  code actions, code lens, formatting (full + range), on-type formatting,
  semantic tokens, color provider
- **Server settings** (from `docs/configuration.md`): `customTags`,
  `keyOrdering`, `kubernetesVersion`, `schemaStore`, `formatValidation`,
  `formatPrintWidth`, `formatSingleQuote`, `httpProxy`, `schemas`,
  `colorDecorators`
- **Binary targets** (from release workflow): `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`, `riscv64gc-unknown-linux-gnu`,
  `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`
- **Package manager:** pnpm (user preference, analogous to strict Rust
  tooling choices)
- **Publisher:** `chdalski`
- **TypeScript strictness:** `@tsconfig/strictest` base, matching the
  project's `clippy::pedantic + nursery + deny` stance
- **Bundled binary:** the extension ships the rlsp-yaml binary inside the
  VSIX for each platform (platform-specific builds)

## Steps

- [x] Clarify requirements with user
- [x] Review server capabilities and settings
- [ ] Scaffold extension project (`editors/code/`)
- [ ] Implement LSP client with binary discovery
- [ ] Expose all server settings in VS Code configuration
- [ ] Add status bar, output channel, and commands
- [ ] Add CI packaging workflow
- [ ] Update project documentation

## Tasks

### Task 1: Scaffold extension project

Set up the `editors/code/` directory with all configuration and
infrastructure files. No TypeScript source yet — just the project
skeleton that subsequent tasks build on.

- [ ] `editors/code/package.json` — extension manifest with metadata
  (name: `rlsp-yaml`, publisher: `chdalski`, engines, activation events
  for YAML files), `contributes.configuration` section exposing all
  server settings with types/defaults/descriptions, build scripts
  (`build`, `watch`, `package`, `lint`, `format`), dependencies
  (`vscode-languageclient`), dev dependencies (`typescript`, `esbuild`,
  `@tsconfig/strictest`, `eslint`, `prettier`, `@vscode/vsce`)
- [ ] `editors/code/tsconfig.json` — extends `@tsconfig/strictest`,
  target ES2022, module NodeNext, outDir `out`, sourceMap enabled
- [ ] `editors/code/.eslintrc.json` — strict ESLint config for
  TypeScript
- [ ] `editors/code/.prettierrc` — consistent formatting
- [ ] `editors/code/.vscodeignore` — exclude source/config from VSIX,
  include only `out/` and `package.json`
- [ ] `editors/code/.gitignore` — `node_modules/`, `out/`, `*.vsix`
- [ ] `editors/code/language-configuration.json` — YAML language
  configuration (comments, brackets, auto-closing pairs)
- [ ] Move `rlsp-yaml-logo.png` from repo root to `editors/code/`
  and reference it as `"icon"` in `package.json`
- [ ] Root `.gitignore` update if needed for `node_modules`

### Task 2: Implement LSP client and extension activation

The core TypeScript source — extension entry point, LSP client
initialization, and bundled binary discovery.

- [ ] `editors/code/src/main.ts` — `activate()` / `deactivate()`
  functions, creates and starts the language client, registers
  disposables
- [ ] `editors/code/src/client.ts` — creates `LanguageClient` with
  server options (bundled binary path resolution with platform-specific
  lookup), client options (document selector for YAML, configuration
  section sync), initialization options from VS Code settings
- [ ] `editors/code/src/config.ts` — typed configuration reader that
  extracts all `rlsp-yaml.*` settings and maps them to the server's
  `initializationOptions` format
- [ ] Binary discovery: check extension directory for
  platform-specific binary (`server/${platform}-${arch}/rlsp-yaml`),
  fall back to `rlsp-yaml.server.path` setting for custom override
- [ ] Handle configuration changes — listen for
  `workspace.onDidChangeConfiguration` and push updates via
  `workspace/didChangeConfiguration`

### Task 3: Status bar, output channel, and commands

Full-featured UI integration — server status indicator, dedicated
log output, and user-facing commands.

- [ ] `editors/code/src/status.ts` — status bar item showing server
  state (starting, running, stopped, error), click to show output
  channel
- [ ] Output channel — dedicated "rlsp-yaml" output channel for
  server traces, connected via `LanguageClient` `outputChannel` option
- [ ] Commands registered in `package.json` `contributes.commands`
  and implemented in `src/commands.ts`:
  - `rlsp-yaml.restartServer` — stop and restart the language server
  - `rlsp-yaml.showOutput` — focus the output channel
  - `rlsp-yaml.showVersion` — show server version in notification
- [ ] Keybindings or command palette entries for commands

### Task 4: CI packaging and platform-specific VSIX builds

Extend CI to build platform-specific VSIX packages that bundle the
compiled rlsp-yaml binary for each target.

- [ ] `.github/workflows/vscode-extension.yml` — workflow that:
  - Triggers on pushes to `editors/code/**` and on release tags
  - Builds rlsp-yaml for each platform target
  - Copies the binary into the extension's `server/` directory
  - Runs `pnpm install && pnpm build && pnpm package` with
    `--target` flag for platform-specific VSIX
  - Uploads VSIX artifacts
- [ ] `editors/code/package.json` — add `package` script using
  `@vscode/vsce package`
- [ ] Extension `README.md` for marketplace listing (features,
  screenshots placeholder, configuration reference)

### Task 5: Update project documentation

Update root and crate documentation to reference the VS Code extension.

- [ ] Root `README.md` — add VS Code extension section with install
  instructions
- [ ] `rlsp-yaml/README.md` — update VS Code editor setup section to
  reference the extension instead of generic LSP client instructions
- [ ] `CLAUDE.md` — add `editors/code/` to project structure

## Decisions

- **Directory:** `editors/code/` — matches rust-analyzer convention,
  leaves room for other editor extensions under `editors/`
- **Package manager:** pnpm — user preference for strictness, analogous
  to the project's Rust tooling stance
- **TypeScript config:** `@tsconfig/strictest` base — mirrors the
  `clippy::pedantic + nursery` + `deny` approach in `Cargo.toml`
- **Bundler:** esbuild — fast, proven in rust-analyzer's extension,
  produces single-file output for clean VSIX packaging
- **Binary bundling:** platform-specific VSIX builds with the binary
  in `server/<platform>-<arch>/rlsp-yaml` — no runtime download needed
- **Publisher:** `chdalski` — matches GitHub username
- **Extension name:** `rlsp-yaml` — matches the crate name directly
