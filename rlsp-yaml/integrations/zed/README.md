# rlsp-yaml for Zed

A [Zed](https://zed.dev) extension that provides YAML language server support via [rlsp-yaml](https://github.com/chdalski/rlsp).

## Installation

Search for `rlsp-yaml` in the Zed extension marketplace (`zed: extensions` command) and install it. The extension downloads the `rlsp-yaml` binary automatically at startup. If you already have `rlsp-yaml` on your PATH, that binary is used instead.

## Configuration

LSP settings are passed through to the server without modification. Configure them in your Zed `settings.json`:

```json
{
  "lsp": {
    "rlsp-yaml": {
      "initialization_options": {
        "validate": true,
        "yamlVersion": "1.2"
      }
    }
  }
}
```

See [docs/configuration.md](https://github.com/chdalski/rlsp/blob/main/rlsp-yaml/docs/configuration.md) for the full settings reference.

## Supported Platforms

| Platform | Architecture |
|----------|-------------|
| Linux    | x86_64, aarch64 |
| macOS    | x86_64, aarch64 |
| Windows  | x86_64 |
