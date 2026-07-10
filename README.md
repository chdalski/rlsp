# The Rust Language Server Project

Small, fast language server implementations written in Rust with minimal memory footprint.

![CI](https://github.com/chdalski/rlsp/actions/workflows/ci.yml/badge.svg) [![codecov](https://codecov.io/gh/chdalski/rlsp/graph/badge.svg)](https://codecov.io/gh/chdalski/rlsp) [![crates.io](https://img.shields.io/crates/v/rlsp-yaml.svg)](https://crates.io/crates/rlsp-yaml) ![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)

## Crates

| Crate | Description |
|-------|-------------|
| [rlsp-yaml](rlsp-yaml/README.md) | YAML language server |
| [rlsp-yaml-parser](rlsp-yaml-parser/README.md) | Spec-faithful streaming YAML 1.2 parser |
| [rlsp-fmt](rlsp-fmt/README.md) | Generic Wadler-Lindig pretty-printing engine |

## Editor Extensions

**VS Code** — a dedicated extension is included at [`rlsp-yaml/integrations/vscode/`](rlsp-yaml/integrations/vscode/). It bundles the compiled server binary and provides full YAML language support — hover, completion, validation, formatting, and more — without any manual configuration. Platform-specific VSIX packages are built for Linux (x64, arm64), macOS (x64, arm64), and Windows (x64).

**Zed** — a Zed extension is available in the [Zed marketplace](https://zed.dev/extensions?query=rlsp-yaml). Search for `rlsp-yaml` and install — no manual server configuration required. The extension is at [`rlsp-yaml/integrations/zed/`](rlsp-yaml/integrations/zed/).

**Claude Code** — a plugin registers `rlsp-yaml` as a native LSP server, so diagnostics and code navigation flow into Claude's context after every edit. Works on every platform this project publishes a release binary for (Linux, macOS, Windows); you install the `rlsp-yaml` binary yourself — see the plugin README for per-platform instructions. Install the plugin with `/plugin marketplace add chdalski/rlsp` then `/plugin install rlsp-yaml@rlsp`. The plugin is at [`rlsp-yaml/integrations/claude-code/`](rlsp-yaml/integrations/claude-code/).

## Contributing

This project accepts bug reports and feature requests via [GitHub Issues](https://github.com/chdalski/rlsp/issues). External code contributions are not currently accepted. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## AI Note

Every line of source in this crate was authored, reviewed, and committed by AI agents
working through a multi-agent pipeline (planning, implementation, independent review,
and test/security advisors for high-risk tasks). The human role is designing the
architecture, rules, and review process; agents execute them. Conformance against the
YAML Test Suite is a measured acceptance criterion — not an aspiration — and any change
touching parser behaviour or untrusted input passes through formal test and security
advisor review before being merged.

## License

[MIT](LICENSE) — Christoph Dalski
