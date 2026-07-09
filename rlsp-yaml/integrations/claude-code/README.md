# rlsp-yaml — Claude Code Plugin

Registers `rlsp-yaml` as a native LSP server for Claude Code. After Claude
edits a `.yaml`/`.yml` file, the server's diagnostics and code-navigation
flow back into Claude's context automatically — no manual linting step, no
separate tool call. Supported on **Linux and macOS**; Windows is not
supported (see [Provisioning](#provisioning)).

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

## Provisioning

The plugin needs an `rlsp-yaml` binary to run. A `SessionStart` hook
provisions one automatically, with no action required:

1. **PATH-first.** If `rlsp-yaml` is already on `PATH` (e.g. installed via
   `cargo install rlsp-yaml` or a system package), the hook copies it into
   the plugin's persistent data directory and stops. No network access.
2. **Auto-download.** Otherwise, the hook detects your OS/architecture,
   downloads the matching binary from the project's
   [GitHub Releases](https://github.com/chdalski/rlsp/releases), verifies
   its integrity, and installs it into the persistent data directory.
3. **Unsupported platform.** If your OS/architecture has no published
   binary, the hook prints install guidance (a link to the release page)
   into Claude's context instead of failing silently.

The provisioned binary lives at `${CLAUDE_PLUGIN_DATA}/rlsp-yaml` — the
plugin's persistent data directory, which survives plugin updates (unlike
`${CLAUDE_PLUGIN_ROOT}`, which changes on every update). This means the
binary is downloaded once, not on every session.

The hook re-runs at the start of every session. If it already has a
working binary in place, this is a fast no-op; if a previous run failed or
was interrupted, it retries automatically.

**Windows is not covered by this plugin.** The provisioning hook is a POSIX
shell script and does not run under native Windows. Windows support is
planned as a separate, Windows-verified follow-up.

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
    "command": "${CLAUDE_PLUGIN_DATA}/rlsp-yaml",
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
  provisioning hook has not run yet or failed. Check the hook's guidance
  message in Claude's context at session start; it names the specific
  failure (unsupported platform, network error, integrity check failure).
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
