---
name: claude-plugin-update-needs-version-bump
description: "`claude plugin update` only fetches when plugin.json `version` changes; a plugin-root CLAUDE.md fails `claude plugin validate --strict`"
metadata: 
  node_type: memory
  type: project
  originSessionId: ecd903e1-51ea-4549-ad69-68bb702531bd
  modified: 2026-09-16T10:36:38.368Z
---

`claude plugin update` decides based on the version string alone. Claude Code works out the version in this order: `plugin.json` `version`, then the marketplace entry `version`, then the git SHA. If that version equals the installed one, the command prints "already at the latest version" and fetches nothing. The gitCommitSha in `installed_plugins.json` plays no part. Measured on 2026-09-16 with Claude Code 2.1.273 in an isolated `CLAUDE_CONFIG_DIR` with a git-subdir plugin: a content change with the same version was not picked up, and a version bump was. Removing `version` from `plugin.json` fails `claude plugin validate --strict` ("No version specified").

A `CLAUDE.md` placed at the plugin root (`rlsp-yaml/integrations/claude-code/CLAUDE.md`) also fails `validate --strict`, with the warning "CLAUDE.md at the plugin root is not loaded as project context".

**Why:** the rlsp-yaml plugin stayed at `0.1.0` while its content changed (the auto-provision hook was removed in `a7a937c9`), so existing installs never received the change.

**How to apply:** any change under `rlsp-yaml/integrations/claude-code/` must come with a `plugin.json` version bump, or existing installs never see it. Don't put instruction files inside the plugin folder while `validate --strict` is a gate.
