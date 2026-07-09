# Contributing to RLSP

RLSP is a collection of small, fast language server implementations written in Rust.
The project is maintained by a single maintainer; external code contributions are not currently accepted.

## How to Contribute

External contributions are welcome as **GitHub issues only**. Pull requests
and patches from external contributors are not accepted.

### Bug Reports

Open the bug report template for the affected component — `rlsp-yaml`, `rlsp-yaml-parser`, `rlsp-fmt`, the VS Code extension, the Zed extension, or the Claude Code plugin. Include:

- A clear description of the problem
- Steps to reproduce
- Expected vs actual behavior
- Version of the affected component (required)
- Editor and OS — optional, but helpful for `rlsp-yaml` and editor extension bugs

### Feature Requests

Open an issue using the feature request template. Describe the use case —
what you are trying to accomplish — rather than a specific implementation.

### Issue Templates

Templates are in [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/).

## Recommended GitHub Labels

| Label | Color | Description |
|-------|-------|-------------|
| `bug` | `#d73a4a` | Something isn't working |
| `enhancement` | `#a2eeef` | New feature or request |
| `question` | `#d876e3` | Further information needed |
| `accepted` | `#0e8a16` | Issue accepted for implementation |
| `wontfix` | `#ffffff` | Will not be addressed |
| `duplicate` | `#cfd3d7` | Duplicate of another issue |
| `rlsp-yaml` | `#fbca04` | Affects the YAML language server |
| `rlsp-yaml-parser` | `#fef2c0` | Affects the YAML 1.2 parser library |
| `rlsp-fmt` | `#c5def5` | Affects the pretty-printing engine |
| `vscode-extension` | `#0052cc` | Affects the VS Code extension |
| `zed-extension` | `#5319e7` | Affects the Zed extension |
| `claude-code-plugin` | `#1d76db` | Affects the Claude Code plugin |
