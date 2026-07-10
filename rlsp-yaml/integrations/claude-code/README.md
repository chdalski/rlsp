# rlsp-yaml — Claude Code Plugin

Registers `rlsp-yaml` as a native LSP server for Claude Code. After Claude
edits a `.yaml`/`.yml` file, the server's diagnostics and code-navigation
flow back into Claude's context automatically — no manual linting step, no
separate tool call. Works on every platform this project publishes a
release binary for (Linux, macOS, Windows) — you install the `rlsp-yaml`
binary yourself; see
[Installing the rlsp-yaml binary](#installing-the-rlsp-yaml-binary).

## Installation

### Local (development / testing)

Point Claude Code at a local checkout of this directory:

```sh
claude --plugin-dir rlsp-yaml/integrations/claude-code
```

### Marketplace install

Add this repository as a plugin marketplace, then install the plugin:

```
/plugin marketplace add chdalski/rlsp
/plugin install rlsp-yaml@rlsp
```

Either path loads the same plugin — `--plugin-dir` is for testing a local
checkout without a marketplace add; the marketplace install is the normal
path for end users.

## Installing the rlsp-yaml binary

This plugin does not bundle or provision a binary — `.lsp.json` spawns the
bare command `rlsp-yaml`, resolved from your `PATH` the same way Claude
Code resolves any other LSP server (`rust-analyzer`, `gopls`, etc.).
Install it once before loading the plugin.

### Option 1 — prebuilt binary (Linux, macOS, Windows)

1. Go to the [latest release](https://github.com/chdalski/rlsp/releases/latest)
   and download the asset matching your platform:

   | Platform | Asset |
   |----------|-------|
   | Linux x86_64 | `rlsp-yaml-x86_64-unknown-linux-gnu.tar.gz` |
   | Linux aarch64 | `rlsp-yaml-aarch64-unknown-linux-gnu.tar.gz` |
   | Linux riscv64 | `rlsp-yaml-riscv64gc-unknown-linux-gnu.tar.gz` |
   | macOS x86_64 | `rlsp-yaml-x86_64-apple-darwin.tar.gz` |
   | macOS aarch64 (Apple Silicon) | `rlsp-yaml-aarch64-apple-darwin.tar.gz` |
   | Windows x86_64 | `rlsp-yaml-x86_64-pc-windows-msvc.zip` |

2. Verify the download against the checksum GitHub publishes for that
   asset — this is the same check a previous version of this plugin ran
   automatically:

   ```sh
   gh release view <tag> --repo chdalski/rlsp --json assets --jq '.assets[] | {name, digest}'
   sha256sum rlsp-yaml-<target>.tar.gz     # Linux
   shasum -a 256 rlsp-yaml-<target>.tar.gz # macOS
   ```

   Replace `<tag>` with the release tag you downloaded and `<target>` with
   the platform triple from the table above. No `gh` CLI? The same
   per-asset digest is shown on the release page itself, next to each
   asset.

3. Extract the archive (`tar xzf <file>` on Linux/macOS; unzip on Windows)
   and put the `rlsp-yaml` binary on your `PATH`.

4. Confirm it resolves to the binary you just installed — especially on a
   machine where `PATH` isn't fully under your own control:

   ```sh
   which rlsp-yaml   # Linux/macOS
   where rlsp-yaml   # Windows
   ```

### Option 2 — `cargo install` (if you have Rust)

```sh
cargo install rlsp-yaml
```

cargo verifies the download against the crates.io index checksum as part
of the registry protocol — no extra verification steps needed. Pin
`--version X.Y.Z` for a reproducible install across machines.

### Staying up to date

The plugin does not auto-update the binary. Check what you have installed
against the latest release:

```sh
rlsp-yaml --version
```

and compare against the
[Releases page](https://github.com/chdalski/rlsp/releases) or
[CHANGELOG](../../CHANGELOG.md); reinstall (or
`cargo install rlsp-yaml --force`) to pick up fixes.

### `PATH` is resolved fresh, not cached

Unlike a provisioned binary copied into a data directory once, `command:
"rlsp-yaml"` is resolved from `PATH` each time Claude Code starts the
language server. If you change `PATH` (e.g. switching a version manager's
active version) while a session is running, restart the session for the
new binary to take effect.

## Configuration

The plugin does not re-declare `rlsp-yaml`'s settings as a separate schema —
it passes whatever is in `.lsp.json`'s `yaml.initializationOptions` straight
through to the server at LSP startup. As shipped, that field is unset, so
the server runs on its built-in defaults plus whatever modelines and
`.editorconfig` files it finds in your workspace — see
[`rlsp-yaml/docs/configuration.md`](../../docs/configuration.md) for what
that covers.

To pin specific settings, add an `initializationOptions` object to
`.lsp.json` in a local checkout and load it with `--plugin-dir` (see
[Installation](#installation)):

```json
{
  "yaml": {
    "command": "rlsp-yaml",
    "extensionToLanguage": { ".yaml": "yaml", ".yml": "yaml" },
    "diagnostics": true,
    "initializationOptions": {
      "kubernetesVersion": "master",
      "schemaStore": true,
      "formatPrintWidth": 80,
      "customTags": ["!include", "!ref"]
    }
  }
}
```

Every key accepted here is documented in
[`rlsp-yaml/docs/configuration.md`](../../docs/configuration.md).
Marketplace installs run with server defaults, since a marketplace-installed
plugin's files live in a version-managed cache rather than a location meant
for hand edits.

## Troubleshooting

Run `/plugin` and open the plugin's detail view (or check the Errors tab).
Common issues:

- **`Executable not found in $PATH`** or a missing-binary error — the
  `rlsp-yaml` binary is not installed, or not on `PATH`. Install it (see
  [Installing the rlsp-yaml binary](#installing-the-rlsp-yaml-binary));
  restart or resume the session if the LSP server was already running.
- **No diagnostics after editing a YAML file** — confirm the LSP server is
  listed with no Errors-tab entries first; a server that never started
  cannot produce diagnostics.

## Publishing / Discoverability

There are two distinct distribution mechanisms:

- **Direct install** — this repository's own
  [`.claude-plugin/marketplace.json`](../../../.claude-plugin/marketplace.json)
  makes the plugin installable by anyone who already knows the repo, via
  `/plugin marketplace add chdalski/rlsp`. No submission or approval is
  required for this path.
- **Discoverability by strangers** — being listed in Claude Code's
  self-serve **community marketplace**
  (`anthropics/claude-plugins-community`) is what lets someone who has
  never heard of this repo find the plugin. This is a separate,
  Anthropic-reviewed submission, documented at
  [code.claude.com/docs/en/plugins#submit-your-plugin-to-the-community-marketplace](https://code.claude.com/docs/en/plugins#submit-your-plugin-to-the-community-marketplace).

Submission steps (see the docs page above for the authoritative, current
process):

1. Run `claude plugin validate` against this directory to confirm the
   plugin passes the same structural check the submission pipeline runs.
2. Submit via the in-app form —
   [claude.ai/admin-settings/directory/submissions/plugins/new](https://claude.ai/admin-settings/directory/submissions/plugins/new)
   for Team/Enterprise orgs, or
   [platform.claude.com/plugins/submit](https://platform.claude.com/plugins/submit)
   for individual authors.
3. Once approved, the community catalog pins this repository by commit SHA
   in `anthropics/claude-plugins-community`; CI there bumps the pin on new
   commits.

The official Anthropic marketplace (`claude-plugins-official`, pre-added on
every install) has no self-serve application and is out of scope here.
