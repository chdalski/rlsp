**Repository:** root
**Status:** InProgress
**Created:** 2026-07-08

# Claude Code LSP Plugin Integration for rlsp-yaml

## Goal

Give Claude Code users the same YAML intelligence that the existing VS Code
and Zed integrations provide, by shipping a Claude Code **plugin** that
registers `rlsp-yaml` as a native LSP server. After Claude edits a `.yaml`/
`.yml` file, the server's diagnostics and code-navigation flow back into
Claude's context automatically. On **Linux and macOS** the plugin provisions
the `rlsp-yaml` binary itself — preferring a copy already on the user's
`PATH`, otherwise downloading the matching release from GitHub — so a user
with neither Rust nor a prebuilt binary can install the plugin and get a
working YAML language server without manual setup. The plugin is distributed
via a repo-hosted `.claude-plugin/marketplace.json`, so users install it with
`/plugin marketplace add chdalski/rlsp` + `/plugin install` — not only by
pointing `--plugin-dir` at a local checkout. The plugin is also made
**submission-ready** for Claude Code's community marketplace — the self-serve
path by which a stranger who doesn't already know the repo can discover it —
via `claude plugin validate`-clean structure, catalog metadata, and documented
submission steps; performing the actual submission and any official-marketplace
listing are out of scope (see Non-Goals). **Windows is not covered
by this plan** (see Non-Goals): its provisioning and `.lsp.json` command
resolution need a separate, Windows-verified design.

This is the first ("Tier 1") of a possible series of Claude Code
integrations. It covers the native LSP path only. On-demand MCP tools and
edit-time enforcement hooks are explicitly out of scope (see Non-Goals).

## Context

### What Claude Code offers for LSP integration (verified against docs)

- Claude Code acts as an **LSP client via plugins**. A plugin declares LSP
  servers in a `.lsp.json` file; Claude Code launches the binary over stdio
  and, after each edit Claude makes, injects the server's diagnostics into
  Claude's context.
- `.lsp.json` per-language fields (documented): `command` (required),
  `extensionToLanguage` (required), `args`, `env`, `transport` (`stdio`
  default / `socket`), `initializationOptions` (object passed at LSP init),
  `settings` (object pushed via `workspace/didChangeConfiguration`),
  `workspaceFolder`, `startupTimeout`, `maxRestarts`, `diagnostics` (bool,
  default `true` — controls whether diagnostics are injected into Claude's
  context).
- Variable substitution is applied inside LSP configs, hook commands, and
  monitor commands: `${CLAUDE_PLUGIN_ROOT}` (plugin install dir — **ephemeral**,
  moves on every plugin update), `${CLAUDE_PLUGIN_DATA}` (per-plugin state dir
  — **persistent** across updates, located at `~/.claude/plugins/data/<id>/`,
  relocatable via `CLAUDE_CONFIG_DIR`), `${CLAUDE_PROJECT_DIR}`, and
  `${ENV_VAR}`.
- Plugin layout: `.claude-plugin/plugin.json` (manifest), plus optional
  `.lsp.json`, `hooks/hooks.json`, `.mcp.json`, `skills/`, `agents/`.
- `plugin.json` manifest fields (documented): `name` (required, kebab-case) plus
  optional `displayName`, `version`, `description`, `author` (object: `name`
  required, `email`/`url` optional), `homepage`, `repository`, `license`,
  `keywords` (string array), and component-path fields (`lspServers`, `hooks`,
  `mcpServers`, `skills`, …). The community catalog surfaces `displayName`,
  `description`, `keywords`, `author`, `license`, `homepage`, and `repository`
  for browse/search. `claude plugin validate` is the documented CLI check for a
  plugin's structure (the community-submission pipeline runs the same check).
- A `SessionStart` hook runs a shell command when a session begins/resumes and
  can run arbitrary shell (curl/tar/chmod). **Exit-code semantics matter here:**
  for `SessionStart`, the hook's **stdout is injected into Claude's context only
  on exit 0**; on a non-zero exit, only **stderr** is shown (to the user, not
  Claude) and stdout is discarded. The hook re-runs every session regardless of
  exit code. So a provisioning hook that wants its status/guidance to reach
  Claude must exit 0 and write to stdout.
- **Known gaps (documented as silent):** when a plugin LSP `command` is not
  found, the error appears only in the human-facing `/plugin` Errors tab and
  is *not* injected into the model's context; there is no documented
  auto-restart of an LSP server after its binary later appears mid-session
  (only `/reload-plugins` on plugin update). The practical consequence: the
  binary should be present at session start, and if a fresh download only
  completes partway through the first session, the server becomes available
  on a subsequent session. The provisioning design must degrade gracefully
  rather than assume the binary is ready the instant the session opens.
- There is **no** documented mechanism for Claude Code to discover or reuse a
  language-server binary installed by another editor (VS Code / JetBrains /
  Zed). Reusing those binaries would mean hard-coding another tool's private,
  version-suffixed install path — rejected as brittle.

### Plugin distribution / marketplace (verified against docs)

- **Local testing needs no catalog:** `claude --plugin-dir <plugin-dir>` loads
  a bare plugin (`.claude-plugin/plugin.json` + `.lsp.json`) directly.
- **Installing via `/plugin install` requires a marketplace catalog:** a
  `.claude-plugin/marketplace.json` at the **marketplace root** (the repo root).
  Required fields: `name` (marketplace id), `owner` (object with a `name`
  string), and `plugins` (array; each entry has at least `name` and `source`).
  A top-level `description` is recommended — `claude plugin validate --strict`
  warns without it (confirmed against the CLI, v2.1.204).
- **Referencing a plugin in a subdirectory of a git repo** uses the plugin
  entry's `source` with the `git-subdir` type:
  `{ "source": "git-subdir", "url": "<owner/repo>", "path": "<subdir>" }`.
  Claude Code does a sparse partial clone of just that subdirectory. Catalog
  paths resolve relative to the marketplace root (the directory containing
  `.claude-plugin/`).
- **No central registry submission is required for installation** — a repo that
  hosts its own `marketplace.json` is itself a marketplace, added by users via
  `/plugin marketplace add <owner/repo>`. Getting listed in Claude Code's
  built-in community/official marketplace (discoverability without knowing the
  repo) is a separate, optional step.
- **Discoverability by strangers** comes only from a marketplace that is
  pre-added or widely added. The **official** marketplace
  (`claude-plugins-official`, pre-added on every install, also at the public
  web catalog `claude.com/plugins`) is **curated by Anthropic — no self-serve
  application**. The **community** marketplace
  (`anthropics/claude-plugins-community`, added manually with
  `/plugin marketplace add anthropics/claude-plugins-community`) is the only
  **self-serve** route: authors submit via an in-app form —
  `claude.ai/admin-settings/directory/submissions/plugins/new` (Team/Enterprise
  orgs) or `platform.claude.com/plugins/submit` (individual authors) — and
  approved plugins are pinned to a repo commit SHA (CI bumps the pin on new
  commits). A repo-root `marketplace.json` is **not** required for community
  submission (that catalog maintains its own); the submission needs a public
  GitHub repo and a `plugin.json`, and `claude plugin validate` is the
  pre-submit check the review pipeline also runs (plus automated safety
  screening). Authoritative submission docs:
  `code.claude.com/docs/en/plugins#submit-your-plugin-to-the-community-marketplace`.

### The rlsp-yaml server

- `rlsp-yaml` is a stdio `tower-lsp` server (`rlsp-yaml/src/main.rs`) — no CLI
  args, no TCP; all configuration flows through LSP `initializationOptions`
  and `workspace/didChangeConfiguration` as a single camelCase settings
  object. Full settings reference: `rlsp-yaml/docs/configuration.md`.
- The server also reads per-document modelines and `.editorconfig`, so useful
  defaults apply even when no `initializationOptions` are supplied.

### Release / binary distribution (source of the download URL)

- release-plz tags releases `rlsp-yaml-v<version>` (`release-plz.toml`).
- `.github/workflows/release-plz.yml` (`build-binaries` job) builds and
  attaches, per release, `rlsp-yaml-<target>.tar.gz` (Linux/macOS) and
  `rlsp-yaml-<target>.zip` (Windows). Each archive contains the bare
  `rlsp-yaml` (`.exe` on Windows) binary.
- Targets published: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `riscv64gc-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`,
  `x86_64-pc-windows-msvc`. There is **no** `aarch64-pc-windows-msvc` asset.
- Download URL shape:
  `https://github.com/chdalski/rlsp/releases/download/rlsp-yaml-v<version>/rlsp-yaml-<target>.tar.gz`.

### Reference integrations to mirror

- Zed extension (`rlsp-yaml/integrations/zed/src/lib.rs`) is the closest prior
  art for binary provisioning: it prefers `worktree.which("rlsp-yaml")`, else
  downloads the release asset for the detected platform, verifies the download
  URL begins with the GitHub-releases prefix, extracts, marks executable, and
  cleans up stale version dirs. Its `platform_target()` os/arch → target-triple
  mapping and its case-tests are a direct template. The provisioning here
  reimplements that *strategy* in a shell hook (Claude Code has no imperative
  extension API), not the code.
- VS Code extension (`rlsp-yaml/integrations/vscode/`) is the template for the
  integration's README structure and for the crate README's Editor Setup entry.

### Verification environment constraint

Agents run in a container without a live Claude Code instance. Split
verification accordingly:
- **Developer-verifiable in-sandbox:** the `claude` CLI binary is present in the
  developer container (independent of any live/interactive Claude Code session),
  so `claude plugin validate --strict` — the documented manifest validator that
  flags missing metadata and unrecognized fields — runs against the plugin
  (`plugin.json` / `.lsp.json` / `hooks/hooks.json`) and the repo
  `marketplace.json` in-sandbox; the provision script's
  download/integrity/extract behavior run directly against the real GitHub
  release; a minimal LSP `initialize`→`didOpen` handshake piped to the
  provisioned binary to confirm it emits `publishDiagnostics`; os/arch →
  asset-name mapping cases.
- **User-verified out-of-band (live Claude Code):** that the plugin loads with
  no `/plugin` errors, that an invalid-YAML edit surfaces an rlsp-yaml
  diagnostic into Claude's context, and the actual `SessionStart`-before-LSP
  ordering. Each such criterion below is labelled *(user-verified)* and the
  task must ship a written manual procedure the user follows.

## Steps

- [x] Clarify scope (Tier 1 LSP plugin) and provisioning (PATH + auto-download)
- [x] Confirm Claude Code `.lsp.json`, hooks, plugin, and variable-expansion facts
- [x] Confirm release asset naming, targets, and download URL scheme
- [x] Task 1 — Plugin skeleton + LSP wiring against a `PATH` binary
- [x] Task 2 — Auto-provisioning `SessionStart` hook + switch to data-dir binary
- [x] Task 3 — Docs + distribution: READMEs, CLAUDE.md, feature-log, CONTRIBUTING + issue template, marketplace.json, plugin.json metadata + submission docs
- [x] Task 4 — Correct the data-dir id in `MANUAL_VERIFICATION.md` (found during user verification)
- [ ] Task 5 — Document the first-install session-restart in the integration README (found during user verification)
- [ ] Final: user runs the live-verification procedures; mark plan Completed

## Tasks

### Task 1: Plugin skeleton and LSP wiring against a PATH binary

Create the Claude Code plugin under
`rlsp-yaml/integrations/claude-code/` that registers `rlsp-yaml` as an LSP
server, wired to a binary resolved from `PATH`. This task isolates "does the
LSP integration work at all" from the provisioning complexity that Task 2
adds.

Files:
- `rlsp-yaml/integrations/claude-code/.claude-plugin/plugin.json` — manifest
  with the metadata the community catalog surfaces: `name` (kebab-case),
  `displayName`, `description`, `version`, `author` (`name`, optional `url`),
  `homepage`, `repository`, `license` (`MIT`), and `keywords`
  (e.g. `["yaml", "lsp", "language-server"]`). Do not set a `version` that
  collides with the crate's release-plz-owned version; the plugin is versioned
  independently (see Decisions).
- `rlsp-yaml/integrations/claude-code/.lsp.json` — one language-server entry:
  `command: "rlsp-yaml"` (PATH-resolved for this task), `extensionToLanguage`
  mapping `".yaml"` and `".yml"` to `"yaml"`, `diagnostics: true`. Do **not**
  enumerate a per-setting schema; `initializationOptions` support is documented
  in Task 3, not hard-coded here (see Decisions).
- `rlsp-yaml/integrations/claude-code/LICENSE` — MIT, matching the vscode/zed
  integration dirs.

Acceptance criteria:
- [x] `plugin.json` and `.lsp.json` parse as valid JSON, contain every field
      required by the Claude Code plugin/LSP schema (`command` and
      `extensionToLanguage` present; language id `"yaml"`), and the plugin
      passes `claude plugin validate --strict` (the same check the
      community-submission pipeline runs; `--strict` fails on missing metadata
      and unrecognized fields).
- [x] A smoke test drives the `PATH` `rlsp-yaml` binary directly (outside
      Claude Code) with an LSP `initialize` followed by `didOpen` of a document
      containing a YAML syntax error, and observes a `publishDiagnostics`
      notification reporting that error. This proves the exact binary the
      `.lsp.json` points at produces diagnostics over stdio.
- [x] *(user-verified)* A written procedure exists and the user confirms:
      installing the plugin locally via `claude --plugin-dir <dir>` (with
      `rlsp-yaml` on `PATH`) loads it with no
      `/plugin` Errors-tab entries, and editing a file with an invalid YAML
      value surfaces an rlsp-yaml diagnostic into Claude's context; hover and
      go-to-definition on an anchor/alias return results.

### Task 2: Auto-provisioning SessionStart hook and data-dir binary

Add self-provisioning so a user without `rlsp-yaml` on `PATH` gets a working
binary automatically, and repoint the LSP `command` at the provisioned copy.

Consult advisors before implementing (input gate) and obtain their sign-off on
the finished work (output gate):
- **security-engineer** — this task downloads an executable over the network
  and runs it. Risk categories to assess: authenticity/integrity of the
  downloaded binary before it is executed, safe archive extraction (path
  traversal in tar/zip), transport and URL trust (HTTPS; the URL must resolve
  to the project's own GitHub releases), and safe handling of the persistent
  data directory. Do not pre-specify the controls in this plan — the advisor
  specifies them.
- **test-engineer** — a shell provisioning script with platform branching and
  a network dependency is greenfield for this repo and has non-obvious testing
  strategy; get a test list before implementing.

Files:
- `rlsp-yaml/integrations/claude-code/hooks/hooks.json` — a `SessionStart` hook
  invoking the provision script via `${CLAUDE_PLUGIN_ROOT}`.
- `rlsp-yaml/integrations/claude-code/scripts/provision.sh` — POSIX (Linux/
  macOS) provisioning logic: (1) if `rlsp-yaml` is on `PATH`, make it available
  at `${CLAUDE_PLUGIN_DATA}/rlsp-yaml` and stop; (2) else if a previously
  provisioned binary already exists there, stop; (3) else detect os/arch, map
  to one of the published Linux/macOS release target triples, download the
  pinned release asset, verify its integrity before use, extract the binary
  into `${CLAUDE_PLUGIN_DATA}`, and mark it executable. On any host whose
  os/arch does not map to a published Linux/macOS asset, or on any failure,
  print actionable install guidance to stdout and **exit 0** so the guidance
  reaches Claude's context (per SessionStart semantics, stdout is surfaced to
  Claude only on exit 0). Provisioning re-runs every session regardless of exit
  code — the still-empty data dir triggers the retry on its own. Windows is out
  of scope — the hook is
  a POSIX shell script and is not expected to run on native Windows (see
  Decisions / Non-Goals).
- `rlsp-yaml/integrations/claude-code/.lsp.json` — change `command` to
  `"${CLAUDE_PLUGIN_DATA}/rlsp-yaml"`.

Acceptance criteria:
- [x] With `rlsp-yaml` **not** on `PATH` and an empty data dir, running
      `provision.sh` on the host downloads the asset matching the host's
      os/arch from the pinned `rlsp-yaml-v<version>` release, passes its
      integrity check, extracts a binary to `${CLAUDE_PLUGIN_DATA}`, and that
      binary answers an LSP `initialize` handshake over stdio.
- [x] A corrupted or tampered download is rejected: the integrity check fails,
      no executable is left in the data dir, and the failure is named on stdout
      with the hook exiting 0 (so Claude sees it, per SessionStart semantics).
      (The specific integrity mechanism is whatever the security advisor
      specifies; this criterion verifies the *behavior*.)
- [x] With `rlsp-yaml` present on `PATH`, `provision.sh` reuses it (the data-dir
      entry resolves to the PATH binary) and performs no network download.
- [x] On a Linux/macOS host whose os/arch has no published asset (e.g. 32-bit
      ARM Linux, `armv7`), the script does not attempt a download; it prints
      install guidance to stdout and exits 0 (so Claude sees the guidance).
- [x] os/arch → release-target mapping is covered by explicit cases for every
      published Linux/macOS target (`x86_64`/`aarch64`/`riscv64gc-unknown-linux-gnu`,
      `x86_64`/`aarch64-apple-darwin`) and at least one unsupported os/arch,
      mirroring the Zed extension's `platform_target` cases.
- [x] The security advisor's specified controls (integrity, extraction safety,
      URL trust) are implemented, and both the security-engineer and
      test-engineer have signed off on the finished implementation.
- [x] *(user-verified)* A written procedure exists and the user confirms: on a
      machine with no `rlsp-yaml` on `PATH` and an empty data dir, starting a
      Claude Code session results in the YAML LSP becoming active (documenting
      whether it activates in the same session or the next, per the timing gap
      in Context).

### Task 3: Documentation and crate integration

Document the integration for users and register it alongside the existing
editor integrations.

Files:
- `rlsp-yaml/integrations/claude-code/README.md` — install steps for both
  paths (`claude --plugin-dir <dir>` for local use; `/plugin marketplace add
  chdalski/rlsp` then `/plugin install rlsp-yaml@<marketplace-name>` for a
  normal install), how provisioning behaves (PATH-first, else download to
  the persistent data dir; what happens on unsupported platforms), how to
  supply server settings via the `.lsp.json` `initializationOptions` object
  with a pointer to `rlsp-yaml/docs/configuration.md` for the full settings
  reference, a troubleshooting note on the `/plugin` Errors tab, and a
  **Publishing / discoverability** section explaining that the repo
  `marketplace.json` covers direct `/plugin marketplace add chdalski/rlsp`
  install while community-marketplace *listing* covers stranger discoverability,
  with the exact submission steps and a link to the canonical docs page
  (`code.claude.com/docs/en/plugins#submit-your-plugin-to-the-community-marketplace`)
  as the authoritative source: run `claude plugin validate`; submit via
  `claude.ai/admin-settings/directory/submissions/plugins/new` or
  `platform.claude.com/plugins/submit`; approval pins the repo by commit SHA in
  `anthropics/claude-plugins-community`. Structure
  mirrors `rlsp-yaml/integrations/vscode/README.md`.
- `rlsp-yaml/README.md` — add a "Claude Code" entry to the Editor Setup
  section, alongside the existing VS Code / Zed / Neovim / Helix entries.
- `README.md` (repo root) — add a "Claude Code" entry to the "Editor
  Extensions" section, alongside the existing VS Code and Zed entries, noting
  Linux/macOS support.
- `.claude-plugin/marketplace.json` (repo root) — a marketplace catalog
  (`name`, `owner`, `description`, `plugins`) with a single plugin entry whose
  source is `git-subdir` (`url: chdalski/rlsp`,
  `path: rlsp-yaml/integrations/claude-code`), so users can
  `/plugin marketplace add chdalski/rlsp` and
  `/plugin install rlsp-yaml@<marketplace-name>`. This is the repo's own
  self-hosted marketplace — no central registry submission is required for
  installation (see Decisions).
- `CLAUDE.md` (repo root) — two updates: (a) add a row to the Components table
  for `rlsp-yaml/integrations/claude-code/`, alongside the vscode and zed rows;
  (b) add a `### Claude Code Plugin` subsection to the `## Build and Test`
  section (parallel to the existing `### VS Code Extension` / `### Zed
  Extension` subsections) documenting the commands to run this integration's
  checks — `claude plugin validate --strict` (which validates
  `plugin.json`/`.lsp.json`/`hooks.json`), the Task 1 LSP-handshake smoke test,
  and the Task 2 `provision.sh` test suite.
- `rlsp-yaml/docs/feature-log.md` — add a "Claude Code Plugin [completed]"
  entry, consistent with the existing "Zed Editor Extension" entry (this is a
  user-facing new-editor-support feature, not an internal refactor).
- `.github/ISSUE_TEMPLATE/bug_claude_code_plugin.yml` — a bug-report template
  for the Claude Code plugin, mirroring the structure of the existing
  `bug_zed_extension.yml`.
- `CONTRIBUTING.md` — two updates: add the Claude Code plugin to the "Bug
  Reports" component enumeration (currently ends "…the VS Code extension, or the
  Zed extension"), and add a `claude-code-plugin` row (distinct color) to the
  "Recommended GitHub Labels" table, alongside `vscode-extension` and
  `zed-extension`.

Acceptance criteria:
- [x] The integration README documents installation, the PATH-first/download
      provisioning behavior, the Linux/macOS-only scope with the
      unsupported-platform message, and the `initializationOptions` passthrough
      with a link to `docs/configuration.md`.
- [x] Every command, path, and URL shown in the README matches what Tasks 1–2
      actually produce (plugin dir name, data-dir command path, release URL
      scheme).
- [x] `rlsp-yaml/README.md` Editor Setup lists Claude Code with a pointer to
      the integration directory, consistent in style with the adjacent entries.
- [x] Root `README.md` "Editor Extensions" section lists Claude Code alongside
      VS Code and Zed.
- [x] Root `CLAUDE.md` Components table has a `rlsp-yaml/integrations/claude-code/`
      row.
- [x] Root `CLAUDE.md` `## Build and Test` has a `### Claude Code Plugin`
      subsection documenting the runnable commands for this integration's
      `claude plugin validate --strict` check, Task 1 smoke test, and Task 2
      provisioning tests, parallel to the existing VS Code / Zed subsections.
- [x] `rlsp-yaml/docs/feature-log.md` has a Claude Code plugin entry matching
      the format of the existing "Zed Editor Extension" entry.
- [x] `.github/ISSUE_TEMPLATE/bug_claude_code_plugin.yml` exists, mirroring the
      `bug_zed_extension.yml` structure.
- [x] `CONTRIBUTING.md`'s "Bug Reports" enumeration lists the Claude Code plugin,
      and its "Recommended GitHub Labels" table has a `claude-code-plugin` row.
- [x] `.claude-plugin/marketplace.json` exists at the repo root, passes
      `claude plugin validate --strict` (which validates marketplace manifests),
      and has `name`, `owner`, `description`, and a `plugins` entry whose
      `git-subdir` source resolves to `rlsp-yaml/integrations/claude-code`; the
      integration README
      documents both the `--plugin-dir` local path and the
      `/plugin marketplace add` + `/plugin install` path.
- [ ] *(user-verified)* A written procedure exists and the user confirms that
      in a real Claude Code session, `/plugin marketplace add chdalski/rlsp`
      followed by `/plugin install rlsp-yaml@<marketplace-name>` installs and
      loads the plugin (LSP active, no `/plugin` Errors-tab entries).
- [x] The plugin passes `claude plugin validate --strict`, and `plugin.json`
      carries the community-catalog metadata fields (`displayName`,
      `description`, `keywords`, `author`, `license`, `homepage`,
      `repository`).
- [x] The integration README's Publishing section documents the community
      submission steps (run `claude plugin validate`; the two in-app form URLs;
      commit-SHA pinning by the `anthropics/claude-plugins-community` catalog),
      links the canonical submission docs page, and states that the repo
      `marketplace.json` covers direct install while community listing covers
      stranger discoverability.

### Task 4: Correct the data-dir id in the manual-verification doc

Found during user verification of Task 2: `MANUAL_VERIFICATION.md`'s Task 2
section hardcodes `~/.claude/plugins/data/rlsp-yaml` in two example commands
(the data-dir clear step and the binary-check step). The real id is
`<plugin-name>@<marketplace>` with `@` replaced by `-` (documented rule; the
docs' example `formatter@my-marketplace` → `formatter-my-marketplace`), so the
dir is `rlsp-yaml-inline` for a `--plugin-dir` load and `rlsp-yaml-rlsp` for
the `rlsp` marketplace install — never the bare `rlsp-yaml`. The plugin itself
is unaffected: `provision.sh` and `.lsp.json` use the `${CLAUDE_PLUGIN_DATA}`
variable, which Claude Code substitutes with the correct dir at runtime; only
these human-facing example paths are wrong.

Files:
- `rlsp-yaml/integrations/claude-code/MANUAL_VERIFICATION.md` — replace the two
  hardcoded `~/.claude/plugins/data/rlsp-yaml` example paths in the Task 2
  section (the `rm -rf` clear step and the `ls -l` check step) with a
  discovery-based approach: have the user run `ls ~/.claude/plugins/data/` and
  find the `rlsp-yaml-*` directory. Enrich the existing `<id>` note so the
  reader understands the id is `<name>@<marketplace>` with `@`→`-`
  (`rlsp-yaml-inline` for a `--plugin-dir` load, `rlsp-yaml-rlsp` for the `rlsp`
  marketplace).

Acceptance criteria:
- [x] No command in `MANUAL_VERIFICATION.md` hardcodes
      `~/.claude/plugins/data/rlsp-yaml` (or any single fixed id); the clear and
      check steps use a discovery command (`ls ~/.claude/plugins/data/`) or
      otherwise resolve the id at runtime.
- [x] The doc states the id convention — `<plugin-name>@<marketplace>` with
      `@`→`-` — giving `rlsp-yaml-inline` (`--plugin-dir`) and `rlsp-yaml-rlsp`
      (`rlsp` marketplace) as the concrete cases.
- [x] `claude plugin validate --strict rlsp-yaml/integrations/claude-code` still
      passes (sanity check — no structural change).
- [x] No other file is modified — the README's `${CLAUDE_PLUGIN_DATA}`
      references are already correct and must not change.

### Task 5: Document the first-install session-restart in the README

Found during user verification of Task 3: on a **fresh install with no
`rlsp-yaml` on `PATH`**, the binary is provisioned by the `SessionStart` hook,
which runs at session start — so it is not available in the session where
`/plugin install` runs; the LSP activates only after the user starts a new
session. This is inherent to Claude Code's SessionStart-then-restart
provisioning model (verified: no install-time hook exists, and `/reload-plugins`
does not re-run `SessionStart`). Document it so the one-restart is expected, not
confusing. This closes the user-facing side of the plan's deferred
SessionStart-timing item; engineering around the gap (a provision-on-spawn
wrapper) remains out of scope (relies on undocumented behavior; see Non-Goals).

Files:
- `rlsp-yaml/integrations/claude-code/README.md` — in the provisioning/
  installation section (and the `/plugin` Errors-tab troubleshooting note), add
  a concise note: after a fresh `/plugin install` on a machine with no
  `rlsp-yaml` on `PATH`, start one new session so the `SessionStart` hook
  downloads and installs the binary and the LSP activates; keeping `rlsp-yaml`
  on `PATH` avoids the wait entirely (the PATH copy is instant).

Acceptance criteria:
- [ ] The integration README documents the first-install session-restart
      behavior for the no-PATH-binary case, in the provisioning/installation
      and/or `/plugin` troubleshooting section.
- [ ] The note states the on-PATH mitigation (instant, no wait).
- [ ] `claude plugin validate --strict rlsp-yaml/integrations/claude-code` still
      passes (sanity check — no structural change).
- [ ] Only `rlsp-yaml/integrations/claude-code/README.md` changes (no code, no
      other docs).

## Decisions

- **Tier 1 = native LSP plugin only.** MCP tools and edit-time enforcement
  hooks are deferred (Non-Goals) so the first deliverable ships the core
  experience with no changes to the Rust server.
- **Provisioning: PATH-first, else auto-download into `${CLAUDE_PLUGIN_DATA}`**
  (user's choice). The single `.lsp.json` `command` points at
  `${CLAUDE_PLUGIN_DATA}/rlsp-yaml`, and the `SessionStart` hook guarantees that
  path resolves to either the user's own PATH binary or a downloaded copy. The
  persistent data dir is used (not `${CLAUDE_PLUGIN_ROOT}`) because it survives
  plugin updates, so the binary is downloaded once.
- **Editor-binary reuse rejected.** No discovery mechanism exists and the paths
  are private/version-suffixed; depending on them is brittle.
- **Mid-session auto-heal is not relied upon.** Because LSP-missing errors do
  not reach the model and there is no documented hot-restart, the design targets
  "binary present at session start" and degrades to "available next session" if
  a first-run download is slow — rather than assuming the LSP recovers within
  the same session.
- **Windows is out of scope for this plan (Linux/macOS only).** Two Windows
  problems are unverifiable in the agent's container and lack documentation: a
  `SessionStart` POSIX shell hook does not run on native Windows (so the
  provisioning — including the PATH-copy step — does not execute there), and it
  is unconfirmed whether `.lsp.json`'s single static `command`
  (`${CLAUDE_PLUGIN_DATA}/rlsp-yaml`, no `.exe`) resolves a Windows executable.
  Rather than half-claim a PATH fallback that may not work, this plan does not
  claim Windows support at all. Windows (its own provisioning path, `.exe`
  handling, and command resolution — verified on a real Windows machine) is a
  follow-up plan. *(If the user wants Windows in this plan, it needs a
  dedicated Windows design added to Task 2 before execution.)*
- **Download version is pinned to a specific `rlsp-yaml-v<version>` tag**, not
  "latest release." The repo interleaves tag schemes (`rlsp-yaml-v*`,
  `vscode-v*`, `zed-v*`), so "latest release" is ambiguous; pinning is also
  reproducible. Keeping the pin current on new server releases (e.g. a
  release-plz trigger mirroring `trigger-vscode`/`trigger-zed`) is a follow-up.
- **The plugin does not re-declare the server's settings schema.** Unlike the
  VS Code extension's `package.json`, which enumerates each setting, the plugin
  passes an opaque `initializationOptions` object and documents it by pointing
  at `docs/configuration.md`. This keeps the plugin out of the formatter
  Settings-Sync obligation (CLAUDE.md) — there is no per-setting list to drift.
- **Independent plugin versioning.** The plugin's `plugin.json` `version` is not
  a `Cargo.toml` version and is not release-plz-owned; agents may set and bump
  it. It does not track the server crate version.
- **Two distribution mechanisms, both addressed.** (1) *Direct install:* the
  repo-root `.claude-plugin/marketplace.json` makes the plugin installable via
  `/plugin marketplace add chdalski/rlsp` — no registry submission needed. (2)
  *Discoverability by strangers:* only the community marketplace provides this
  self-serve. It does **not** use our `marketplace.json` — the
  `anthropics/claude-plugins-community` catalog maintains its own and pins our
  repo by commit SHA. Task 3 makes the plugin submission-ready
  (`claude plugin validate`-clean + catalog metadata) and documents the steps;
  the actual submit is a user action (in-app form). The official Anthropic
  marketplace (best reach) is Anthropic-curated with no self-serve application,
  so it is not a plannable step.

## Non-Goals

- MCP server exposing callable YAML tools (`yaml_lint`, `yaml_format`,
  `yaml_hover`) — a separate Tier-2 plan.
- `PostToolUse`/edit-time enforcement hooks that block or auto-format on write —
  a separate plan; requires a one-shot CLI the server does not have.
- Windows support of any kind (auto-download provisioning, PATH fallback, and
  `.lsp.json` command resolution) — this plan targets Linux and macOS only;
  Windows is deferred to a follow-up with its own Windows-verified design (see
  Decisions).
- **Performing** the community-marketplace submission (the in-app form is a user
  action under the author's Anthropic account) and any **official** marketplace
  listing (Anthropic-curated, no self-serve application) are out of the build
  pipeline's scope. Task 3 makes the plugin *submission-ready* (catalog metadata
  + `claude plugin validate`-clean) and documents the exact submission steps; the
  user performs the submit. CI automation (a workflow to build/validate the
  plugin or auto-bump the pinned download version) remains a follow-up.
- Any change to the `rlsp-yaml` Rust server, its capabilities, or its settings.
- `socket` transport — the server speaks stdio; socket config is undocumented
  and unneeded.
