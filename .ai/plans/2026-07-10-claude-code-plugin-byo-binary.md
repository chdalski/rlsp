**Repository:** root
**Status:** Completed (2026-07-10)
**Created:** 2026-07-10

# Claude Code Plugin — Bring-Your-Own-Binary

## Goal

The Claude Code plugin currently auto-provisions the `rlsp-yaml`
binary through a `SessionStart` hook that downloads and integrity-checks
a release binary. Convert it to the ecosystem-standard "assume installed,
document how to install" model: a two-file plugin (`plugin.json` +
`.lsp.json`) whose `.lsp.json` invokes a bare `command: "rlsp-yaml"`,
delegating binary resolution to Claude Code / the OS. This makes the
plugin work on **every platform we publish a release binary for
(including Windows)** and removes the download-and-execute trust boundary
plus its ongoing maintenance (per-release sha256 pinning, `curl`/`tar`
hardening, cross-platform provisioning scripts). The cost — users install
the binary themselves — is offset by per-platform install documentation.

## Context

- **User decision.** The user chose the bring-your-own-binary approach
  (over extending auto-provisioning to Windows) and additionally directed
  removing `MANUAL_VERIFICATION.md` and updating the READMEs. Reference for
  the pattern: `Piebald-AI/claude-code-lsps` — 30 LSP plugins, each just
  `plugin.json` + `.lsp.json` with a bare command, install responsibility
  documented per language.
- **Plugin location.** `rlsp-yaml/integrations/claude-code/`. Current files:
  - `.claude-plugin/plugin.json` — unchanged by this work.
  - `.lsp.json` — `yaml.command` is `"${CLAUDE_PLUGIN_DATA}/rlsp-yaml"`.
  - `hooks/hooks.json` — `SessionStart` hook → `scripts/provision.sh`.
  - `scripts/provision.sh` (~236 lines) — hardened downloader: PATH-first,
    else download pinned release, verify hardcoded per-target sha256,
    `curl --proto '=https'`, tar entry validation, `umask 077`, atomic `mv`.
  - `scripts/provision.test.sh` — hermetic subprocess test suite for
    `provision.sh`.
  - `MANUAL_VERIFICATION.md` — interactive verification procedures scoped to
    the old provisioning tasks.
  - `README.md` — has a `## Provisioning` section describing the hook.
- **Published release binaries** (the targets install docs must cover),
  from tag `rlsp-yaml-v0.13.0`:
  `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `riscv64gc-unknown-linux-gnu`, `x86_64-apple-darwin`,
  `aarch64-apple-darwin` (all `.tar.gz`), and `x86_64-pc-windows-msvc`
  (`.zip`). Docs should be **version-agnostic** — point at the latest
  release page and the asset-name pattern, not a hardcoded version/sha.
- **Unaffected by the `.lsp.json` change:**
  - `rlsp-yaml/tests/claude_code_stdio_smoke.rs` spawns
    `env!("CARGO_BIN_EXE_rlsp-yaml")` directly; it does not read `.lsp.json`.
    Its header comment already says `command: "rlsp-yaml"` (becomes accurate
    after this change). Keep the test.
  - Root `.claude-plugin/marketplace.json` references only the plugin
    directory (git-subdir); no hook/provisioning reference.
  - VS Code and Zed integrations manage the binary differently; out of scope.
- **Files that reference the old model and must be updated for currency:**
  - `rlsp-yaml/integrations/claude-code/README.md` (Provisioning section,
    config example command, troubleshooting, intro platform line).
  - Root `README.md` (~line 21) — "automatic binary provisioning
    (PATH-first, else auto-download)", "Supports Linux and macOS".
  - `rlsp-yaml/README.md` (~line 52) — same blurb.
  - `rlsp-yaml/docs/feature-log.md` — "Claude Code Plugin" entry (provisioning
    + "Windows is not yet supported").
  - Root `CLAUDE.md` build/test section — the `claude plugin validate`
    comment says "hooks.json"; a line runs `provision.test.sh`.
- **Reference — Claude Code plugin mechanics.** Hooks are optional; a plugin
  of only `plugin.json` + `.lsp.json` validates. Plugin reference:
  https://code.claude.com/docs/en/plugins-reference
- **Open technical item (does not block this plan).** Whether Claude Code
  resolves a bare `rlsp-yaml` command to `rlsp-yaml.exe` on native Windows
  (PATHEXT) is unverified in this sandbox. Bare-command is the standard
  cross-platform pattern and we do publish the Windows binary, but Windows
  end-to-end can only be confirmed out-of-band by the user. If CC does not
  append `.exe`, a per-OS command form is a follow-up.

## Non-Goals

- **Adding musl (or any new) release targets.** With a bare command the
  plugin runs whatever binary the user supplies, but publishing new release
  assets is a separate release-CI change — a possible follow-up, not this
  plan. Install docs cover only the targets we publish today.
- **Changing the VS Code or Zed integrations** — they bundle/manage the
  binary differently and are unaffected.
- **Any change to the `rlsp-yaml` server binary itself.**
- **Replacing `MANUAL_VERIFICATION.md`** — it is removed with no successor
  doc (user-directed).

## Steps

- [x] Change `.lsp.json` `yaml.command` to bare `"rlsp-yaml"`
- [x] Delete `hooks/hooks.json` (and the now-empty `hooks/` dir)
- [x] Delete `scripts/provision.sh` and `scripts/provision.test.sh` (and the
      now-empty `scripts/` dir)
- [x] Delete `MANUAL_VERIFICATION.md`
- [x] Rewrite the plugin `README.md`: replace `## Provisioning` with
      per-platform install instructions; fix intro platform line, config
      example command, and troubleshooting
- [x] Update root `README.md` and `rlsp-yaml/README.md` plugin blurbs
- [x] Update the `rlsp-yaml/docs/feature-log.md` "Claude Code Plugin" entry
- [x] Update root `CLAUDE.md` build/test section (drop `hooks.json` from the
      validate comment; remove the `provision.test.sh` line)
- [x] Verify: both `claude plugin validate --strict` invocations and the
      stdio smoke test pass; grep sweep shows no stale references

## Tasks

### Task 1: Convert plugin to bring-your-own-binary and update all docs

Switch the Claude Code plugin to a bare `command: "rlsp-yaml"`, remove the
provisioning hook/scripts and the manual-verification doc, and bring every
piece of documentation that described the old auto-provisioning model into
line with the new one — including per-platform binary-install instructions.
One task: the config change and the docs are a single logical change, and
any intermediate commit that changed the mechanism without the docs (or vice
versa) would leave the repository describing behavior it no longer has.

Mechanism:
- [x] `.lsp.json` `yaml.command` is `"rlsp-yaml"` (bare — no
      `${CLAUDE_PLUGIN_DATA}` prefix); `extensionToLanguage` and
      `diagnostics` unchanged
- [x] `hooks/hooks.json` removed; `hooks/` directory gone
- [x] `scripts/provision.sh` and `scripts/provision.test.sh` removed;
      `scripts/` directory gone
- [x] `MANUAL_VERIFICATION.md` removed

Documentation:
- [x] Plugin `README.md`: no `Provisioning`/hook/`${CLAUDE_PLUGIN_DATA}`
      references remain; a new "Installing the rlsp-yaml binary" section
      documents, for each published target, downloading the matching release
      asset, extracting it, and putting it on `PATH`, plus
      `cargo install rlsp-yaml` for users who have Rust; the config example
      command is bare `rlsp-yaml`; troubleshooting maps a missing binary to
      "install it / ensure it is on PATH"; the intro platform line no longer
      says Linux/macOS-only
- [x] Root `README.md` and `rlsp-yaml/README.md` plugin descriptions no
      longer claim automatic provisioning or a Linux/macOS-only limitation;
      they state the binary must be installed and point at the plugin README
      for instructions
- [x] `rlsp-yaml/docs/feature-log.md` "Claude Code Plugin" entry: the
      sentence "On Linux and macOS, the plugin provisions the `rlsp-yaml`
      binary itself — preferring one already on `PATH`, otherwise downloading
      the matching release — so no manual install is required" is replaced
      with one describing the binary as user-installed (with a pointer to the
      plugin README), and the trailing "Windows is not yet supported." claim
      is removed/corrected since Windows is now supported via
      bring-your-own-binary (see Decisions: update in place, not a new entry)
- [x] Root `CLAUDE.md`: the `claude plugin validate` comment no longer lists
      `hooks.json`; the `provision.test.sh` line is removed
- [x] Install docs use version-agnostic language (latest-release page +
      asset-name pattern), no hardcoded version or sha256

Security guidance (from the security advisor — see Decisions):
- [x] Any install-doc security guidance the security-engineer specifies
      (e.g. checksum verification, trusted-source / PATH caveat) is included

Verification:
- [x] `claude plugin validate --strict rlsp-yaml/integrations/claude-code`
      passes
- [x] `claude plugin validate --strict .` passes
- [x] `cargo test -p rlsp-yaml --test claude_code_stdio_smoke` passes
- [x] Repo grep finds no remaining `provision` / `hooks.json` /
      `CLAUDE_PLUGIN_DATA` / `MANUAL_VERIFICATION` / `SessionStart`
      references outside `.ai/` archived/plan/memory files
- [x] No stale platform-limitation or auto-provisioning language remains in
      the plugin README, root `README.md`, `rlsp-yaml/README.md`, or
      `feature-log.md` (e.g. "Windows is not yet supported", "Linux and
      macOS", "auto-download", "provisions ... itself")
- [x] Advisor gates satisfied: test-engineer (input + output),
      security-engineer (input + output)

## Decisions

- **Approach — bring-your-own-binary (user-selected).** Bare
  `command: "rlsp-yaml"` delegates binary resolution to Claude Code / the
  OS, so the plugin is inherently cross-platform; users install the binary
  themselves. Chosen over extending auto-provisioning to Windows because it
  eliminates the download-and-execute trust boundary and all its
  per-release maintenance, and matches the ecosystem norm.
- **Single task, not split.** Mechanism (config + deletions) and docs are
  one logical change; splitting leaves an intermediate commit whose docs
  describe removed behavior. No structural reason to split.
- **`MANUAL_VERIFICATION.md` removed, not replaced (user-directed).** Its
  procedures were scoped to the old provisioning tasks; the new model
  (binary on PATH → plugin loads) matches every other LSP plugin, which
  ship no such doc.
- **feature-log entry updated in place, not superseded.** The existing
  "Claude Code Plugin" entry's provisioning/platform sentences are corrected
  to describe current behavior rather than adding a new "we changed our
  minds" entry — the log must not describe removed behavior (currency), and
  this is a factual correction to an existing feature. *Alternative the user
  may prefer: add a new superseding entry instead.*
- **Install docs are version-agnostic.** Point at the latest release page
  and the asset-name pattern, not a pinned version/sha, to avoid staleness.
- **Windows `.exe` resolution unverified.** Documented as a known open item;
  bare-command is standard and we publish the Windows binary, but CC's
  PATHEXT behavior can only be confirmed out-of-band by the user. If it
  fails, a per-OS command is a follow-up — not blocking.
- **Security controls deferred to the advisor.** The shift from
  integrity-verified download to a user-supplied binary is a trust-boundary
  change (untrusted binary resolution / provenance). The security-engineer
  specifies what install-doc guidance is warranted; the plan does not
  pre-prescribe it.

## Accepted Risks & Open Items (recorded at completion, 2026-07-10)

Inherent to the user-directed bring-your-own-binary architecture — not gaps
in the implementation (per the security-engineer, confirmed independently at
review):

- **Bare-`PATH` resolution is trusted with no code-level verification.**
  Mitigated only by doc-level `which`/`where` PATH-shadowing guidance in the
  install docs, consistent with ecosystem-standard LSP integrations.
- **No automated update mechanism.** Mitigated only by doc-level
  version-check guidance in the install docs.

Open item (non-blocking): Windows `.exe` PATHEXT resolution of the bare
`rlsp-yaml` command is unverified in-sandbox — confirm out-of-band. If it
fails, a per-OS command form is a follow-up.
