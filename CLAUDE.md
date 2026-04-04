# The Rust Language Server Project

A collection of language server implementations written in Rust, built entirely by AI agents. No human-written application code — every line of source is authored, reviewed, and committed by AI. The purpose is to provide users with small, fast implementations with minimal memory footprint.

## Build and Test

### Rust

```sh
cargo fmt              # format
cargo clippy --all-targets  # lint (zero warnings enforced)
cargo build            # build
cargo test             # run all tests
cargo bench            # run benchmarks (Criterion)
cargo clean            # clean stale build artifacts
```

### VS Code Extension

```sh
cd rlsp-yaml/integrations/vscode
pnpm install       # install dependencies
pnpm run build     # bundle extension (esbuild)
pnpm run test      # run unit tests (vitest)
pnpm run test:integration  # run VS Code integration tests (requires display; use xvfb-run -a on Linux)
pnpm run lint      # lint TypeScript source
pnpm run format    # check formatting (prettier)
```

## Components

| Path | Purpose |
|------|---------|
| `rlsp-fmt/` | Generic Wadler-Lindig pretty-printing engine |
| `rlsp-yaml/` | YAML language server |
| `rlsp-yaml/integrations/vscode/` | VS Code extension for rlsp-yaml |

## Conventions

<!-- Agents: add non-obvious project conventions discovered during work — things a future agent would need to know to avoid mistakes. One line each. Remove when no longer true. -->

- Workspace lint inheritance — root `Cargo.toml` defines `[workspace.lints]`, crates inherit via `lints.workspace = true`
- Clippy pedantic + nursery at warn; selected lints at deny; `warnings = "deny"`
- Maximum TypeScript strictness — `tsconfig.json` extends `@tsconfig/strictest`, ESLint uses `strictTypeChecked` + `stylisticTypeChecked`
- Automated releases via release-plz; tag format: `<package>-v<version>`. VS Code extension uses CalVer tags: `vscode-v<YYYY.MM.NN>`
- Conventional commits required — changelogs auto-generated via git-cliff
- OIDC trusted publishing to crates.io — no `CARGO_REGISTRY_TOKEN` secret needed
- pnpm as Node.js package manager
- AI-written project — external contributions via GitHub issues only
- Each `rlsp-<language>` crate must have `README.md`, `docs/configuration.md`, `docs/feature-log.md`
- Root `README.md` is landing page; crate `README.md` is self-contained for users; `docs/configuration.md` is pure settings reference

## References

<!-- Agents: add authoritative sources used to make implementation decisions. One line each. -->

- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [release-plz](https://release-plz.ieni.dev/)
- [YAML 1.2 Specification](https://yaml.org/spec/1.2.2/)
- [YAML Test Suite](https://github.com/yaml/yaml-test-suite)
- [YAML Test Matrix](https://matrix.yaml.info/)
- [Kubernetes API Reference](https://kubernetes.io/docs/reference/)
- [KubeSpec](https://kubespec.dev/)
