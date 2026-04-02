**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-02

## Goal

Rename `rlsp-yaml/editors/code/` to `rlsp-yaml/integrations/vscode/` to
broaden the directory scope beyond editor extensions — the `integrations/`
directory can hold VS Code, Claude Code plugins, and other tool integrations.
"vscode" is more explicit than "code".

## Context

- Current path: `rlsp-yaml/editors/code/`
- New path: `rlsp-yaml/integrations/vscode/`
- 11 files with 45+ references need updating
- No code logic changes — pure rename/refactoring
- Internal TypeScript paths (`server.ts`, `config.ts`) use relative paths
  within the extension and don't need updating

## Steps

- [ ] Move directory and update all references

## Tasks

### Task 1: Rename directory and update all references

- [ ] `git mv rlsp-yaml/editors/code rlsp-yaml/integrations/vscode`
  (may need `mkdir -p rlsp-yaml/integrations` first)
- [ ] Update `/README.md` — line 15: `editors/code/` → `integrations/vscode/`
- [ ] Update `/CLAUDE.md` — lines 33, 85: directory structure and build
  commands
- [ ] Update `/rlsp-yaml/README.md` — line 47: extension link
- [ ] Update `/.github/workflows/vscode-extension.yml` — 10 path references
  (trigger paths, working directories, binary copy paths, artifact paths)
- [ ] Update `/rlsp-yaml/integrations/vscode/CLAUDE.md` — line 9: example
  reference to `rlsp-toml/editors/code/` → `rlsp-toml/integrations/vscode/`
- [ ] Update `/rlsp-yaml/integrations/vscode/README.md` — line 52: build
  from source path
- [ ] Verify `cd rlsp-yaml/integrations/vscode && pnpm build && pnpm run test`
- [ ] Grep for any remaining `editors/code` references (excluding completed
  plans)

## Decisions

- **Directory name:** `integrations/vscode/` — broader than `editors/code/`,
  accommodates Claude Code plugins and other non-editor integrations
- **Completed plans:** don't update old plan files — they're historical records
