**Repository:** root
**Status:** InProgress
**Created:** 2026-09-16

## Goal

The rlsp-yaml Claude Code plugin changed after release without its
`plugin.json` version changing, so `claude plugin update` reports "already at
the latest version" and existing installs never receive the changes. Fix the
stale version once (bump to 0.2.0), and make sure it never goes stale again:
every change to this repo is made by Claude, so a project convention in the
root `CLAUDE.md` requires every change to the plugin folder to bump the plugin
version in the same commit. A live check confirms that Claude follows the
rule. Users also get documentation on how plugin updates reach them.

## Context

- **How Claude Code decides whether to update** (verified 2026-09-16 against
  Claude Code 2.1.273, both by reading its code and by an isolated experiment
  with a separate `CLAUDE_CONFIG_DIR` and a git-subdir plugin). Claude Code
  resolves a plugin's version in this order: `plugin.json` `version`, then
  the marketplace entry's `version`, then the git commit SHA. `plugin update`
  compares that version with the installed one; if they are equal it prints
  "already at the latest version" and fetches nothing. A content change with
  an unchanged version was not picked up; a version bump was.
- **Content is served straight from `main`.** The repo-root
  `.claude-plugin/marketplace.json` declares the plugin with a `git-subdir`
  source (`chdalski/rlsp`, path `rlsp-yaml/integrations/claude-code`) and no
  `ref`. Every file in that folder ships to users, and a change is live for
  fresh installs as soon as it lands on `main`. The version only decides
  whether `plugin update` fetches for an existing install.
- **Why the current version is wrong.** `plugin.json` has said `0.1.0` since
  the plugin was created in `ba828c36` (2026-07-08). The plugin changed twice
  after that: `653b91c9` added a SessionStart hook that downloaded the binary
  automatically, and `a7a937c9` (2026-07-10) removed that hook in favour of
  "bring your own binary". Installs from before 2026-07-10 still run the
  removed hook and cannot update.
- **Constraints measured on 2026-09-16 with `claude plugin validate --strict`**
  (a documented gate in the root `CLAUDE.md`):
  - Removing `version` from `plugin.json` fails it ("No version specified").
  - A `CLAUDE.md` at the plugin root fails it ("CLAUDE.md at the plugin root
    is not loaded as project context").
- **Every change here is made by Claude.** The project is AI-written, and the
  root `CLAUDE.md` is loaded at startup by every agent: the lead, the
  developer, the reviewer and the advisors. Its Conventions already say that
  agents must not edit `version` in any `Cargo.toml`, because release-plz owns
  those. `plugin.json` is not managed by release-plz.
- **Documentation facts verified 2026-09-16 against Claude Code 2.1.273** (by
  a developer and a reviewer in earlier, discarded work; the handoff must
  verify them again):
  - In a session: `/plugin marketplace update rlsp`, then
    `/plugin update rlsp-yaml@rlsp`. From a shell: `claude plugin marketplace
    update rlsp`, then `claude plugin update rlsp-yaml@rlsp`.
  - Update outcomes are "✔ rlsp-yaml is already at the latest version (…)."
    or "✔ Plugin "rlsp-yaml" updated from … to … for scope user. Restart to
    apply changes." `/reload-plugins` loads the new version without a restart.
  - Auto-update is off by default for third-party marketplaces such as
    `rlsp`. It can be turned on via `/plugin` → Marketplaces → `rlsp` →
    Enable auto-update, or with `extraKnownMarketplaces.rlsp.autoUpdate: true`
    in `settings.json`. Claude Code reads and applies that setting; it does
    not merely parse it.
  - `claude plugin marketplace` has no `--auto-update` flag in 2.1.273, even
    though a summary of the docs claimed one.
  - Docs:
    https://code.claude.com/docs/en/discover-plugins#configure-auto-updates
- **Pitfall for the live check.** A Claude Code session started inside a
  clone of this repo loads `.claude/CLAUDE.md`, which casts the main session
  as a lead that does not implement code. The check has to reflect how edits
  are actually made here: by the `developer` agent, whose instructions include
  the root `CLAUDE.md`. `claude --agent <name>` and `claude -p` exist in
  2.1.273.
- **Key files:**
  - `rlsp-yaml/integrations/claude-code/.claude-plugin/plugin.json`: the
    version.
  - `CLAUDE.md` (root): Conventions.
  - `rlsp-yaml/integrations/claude-code/README.md`: user docs. Its "Staying
    up to date" subsection covers the binary only.
  - `rlsp-yaml/docs/feature-log.md`: the "Claude Code Plugin" entry.
- **Specifications:**
  - Semantic Versioning 2.0.0: https://semver.org/
  - Conventional Commits 1.0.0: https://www.conventionalcommits.org/en/v1.0.0/
  - Claude Code plugin reference: https://code.claude.com/docs/en/plugins-reference
  - Discover plugins / auto-updates:
    https://code.claude.com/docs/en/discover-plugins

### Bump rules

Every commit that changes a file under `rlsp-yaml/integrations/claude-code/`
also changes `plugin.json`'s `version` in that same commit, by exactly one step:

| The commit's plugin change | Current major is 0 | Current major ≥ 1 |
|---|---|---|
| breaks existing installs (e.g. removes or renames something users rely on) | minor | major |
| adds a feature | patch | minor |
| anything else (fixes, docs, README, metadata) | patch | patch |

These match release-plz's defaults for 0.x crates. Milestone bumps (e.g. to
1.0) are user-directed.

## Steps

- [x] Clarify requirements with user
- [x] Measure `claude plugin update` behaviour and `validate --strict`
      constraints
- [x] Task 1: Document how plugin updates reach users
- [x] Task 2: Version-bump rule and one-time bump to 0.2.0
- [ ] With the user's go-ahead, push `main`; verify the CI run for that push
      succeeds on every job
- [ ] On this machine, `claude plugin update rlsp-yaml@rlsp` moves the
      installed plugin from `0.1.0` to `0.2.0`

## Tasks

### Task 1: Document how plugin updates reach users

Users need to know that a plugin change reaches them only as a new version,
that auto-update for this marketplace is off unless they turn it on, and how
to update by hand.

- [x] The plugin README has a section on updating the plugin itself, separate
      from the binary "Staying up to date" subsection. It states that an
      update arrives only when the plugin's version changes and that every
      change to the plugin ships with a new version, gives the commands to
      update by hand (in a session and from a shell), and explains how to turn
      on auto-update for the `rlsp` marketplace
- [x] The review handoff shows each documented shell command run against
      Claude Code 2.1.273 in an isolated `CLAUDE_CONFIG_DIR`, with its output.
      It names the steps that can only be done interactively and the docs
      page they rely on, confirms that the real `~/.claude` plugin files are
      unchanged, and confirms that no scratch files remain in the repo
- [x] The feature-log "Claude Code Plugin" entry states that plugin changes
      are released as new versions and delivered through Claude Code's plugin
      update mechanism
- [x] The commit changes only the plugin README and the feature log;
      `plugin.json` is unchanged

### Task 2: Version-bump rule and one-time bump to 0.2.0

Add the project convention that keeps the plugin version current, and bump
the version to 0.2.0. That version covers the switch to "bring your own
binary" (`a7a937c9`) and the Task 1 docs, so both reach existing installs.

- [x] `plugin.json` `version` is `0.2.0`, and no other field changes
- [x] The root `CLAUDE.md` Conventions contain a rule requiring every commit
      that changes a file under `rlsp-yaml/integrations/claude-code/` to also
      change `plugin.json`'s `version` in that commit, following the Bump
      rules table. The rule states why (`claude plugin update` compares
      versions, not content), that milestone bumps are user-directed, and
      that the existing "must not edit `version` in any `Cargo.toml`" rule
      does not cover `plugin.json`
- [x] Live check: in a scratch clone outside `/workspace` containing this
      task's changes, a headless Claude Code session running as the
      `developer` agent is asked to make a small non-breaking change to a
      file in the plugin folder, with no mention of versions. The resulting
      diff changes `plugin.json` from `0.2.0` to `0.2.1`. The handoff shows the
      exact command, the prompt, and the resulting diff. The scratch clone is
      deleted afterwards
- [x] `claude plugin validate --strict` passes for the plugin directory and
      the repo root

## Decisions

- **A convention instead of CI automation** (user choice). Every change to
  this repo is made by Claude, so a rule in the root `CLAUDE.md` puts the
  version bump into the same commit as the change. There is no bot commit,
  no tool, no write token, and no tags. Trade-off accepted: enforcement relies
  on agents following the rule and the reviewer checking it; nothing checks it
  mechanically.
- **Root `CLAUDE.md`, not a `CLAUDE.md` in the plugin folder** (user choice
  after the measurement). A plugin-root `CLAUDE.md` fails
  `claude plugin validate --strict` and would ship to every user. The root
  file is loaded by every agent at startup.
- **Bump rules:** release-plz's 0.x defaults, applied per commit as in the
  Bump rules table (user agreed).
- **One-time version:** 0.2.0 rather than 0.1.1, because removing the
  download hook broke installs from before 2026-07-10 (user choice).
- **Docs before the rule.** Task 1 changes the plugin folder before the rule
  exists, and Task 2's 0.2.0 bump covers it. Both land in the same push, so
  users see a single new version, 0.2.0.
- **Live check** (user choice). The whole approach depends on agents following
  the rule, so it is demonstrated once with a real session.
- **Keep `version` in `plugin.json`; `marketplace.json` stays unchanged.**
  Removing `version` fails `validate --strict`, and `plugin.json` wins in
  Claude Code's version order.

## Non-Goals

- A CI workflow, bump tool, bot commits, or `claude-code-v*` tags
- A CI check that fails when the plugin folder changes without a version bump
- A `CLAUDE.md` inside the plugin folder
- Changing how the `rlsp-yaml` binary is installed or versioned
- A changelog, GitHub Releases, or release notes for the plugin
- Pinning the marketplace entry to a tag or `ref`
- Editing agent definitions or rules under `.claude/`
