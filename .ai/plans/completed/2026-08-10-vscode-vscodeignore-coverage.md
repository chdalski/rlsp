**Repository:** root
**Status:** Completed (2026-08-10)
**Created:** 2026-08-10

# VS Code extension: exclude coverage/ from the packaged VSIX

## Goal

`.vscodeignore` doesn't exclude the `coverage/` directory, so when a
developer packages the extension right after running `pnpm run test:coverage`
(which writes to `./coverage`), the lcov report gets bundled into the `.vsix`.
Add a `coverage/` entry to `.vscodeignore` so packaging never ships coverage
artifacts, regardless of local state. Surfaced by the reviewer during the
vitest-config rename (`2026-08-10-vscode-test-config-hygiene.md`).

## Context

- `.vscodeignore` (in `rlsp-yaml/integrations/vscode/`) is read by
  `vsce package` (`pnpm run package`) to decide what goes into the `.vsix`.
  Its current entries: `src/`, `node_modules/`, `tsconfig.json`,
  `eslint.config.mjs`, `.prettierrc`, `.gitignore`, `CLAUDE.md`,
  `pnpm-lock.yaml`, `vitest.config.mts`, `*.vsix` — no `coverage/`.
- `vitest.config.mts` sets `coverage.reportsDirectory: './coverage'`, so the
  coverage report lands at `rlsp-yaml/integrations/vscode/coverage/`. When
  that directory exists at package time it is included in the VSIX; a clean/CI
  package (no `coverage/` present) is unaffected — so this only surfaces
  locally, but the ignore entry closes it deterministically.
- The failing/observed behavior was confirmed by the reviewer of the
  vitest-config task, who verified `pnpm run package` includes `coverage/`
  whenever the directory is present.
- `coverage/` is **already git-ignored** via
  `rlsp-yaml/integrations/vscode/.gitignore` (line 5, `coverage/`), confirmed
  with `git check-ignore -v` on a path inside the directory. (An earlier
  `git check-ignore` on the not-yet-created directory false-negatived, because
  a dir-only pattern can't match a path git cannot confirm is a directory.)
  So no `.gitignore` change is needed — only the `.vscodeignore` (VSIX) gap
  remains.

- **Landing model:** trunk-based — lands directly on `main` through the
  developer → reviewer pipeline.

## Steps

- [x] Confirm `.vscodeignore` lacks a `coverage/` entry and that the coverage
      report dir is `./coverage`
- [x] Task 1 — Add `coverage/` to `.vscodeignore` and verify the VSIX excludes it

## Tasks

### Task 1: Exclude coverage/ from the packaged VSIX

Add a `coverage/` entry to `.vscodeignore` so `vsce package` never bundles the
coverage report, and verify by packaging with a `coverage/` directory present.

- [x] `.vscodeignore` contains a `coverage/` entry; no other line is changed
- [x] With a `coverage/` directory present (e.g. after `pnpm run test:coverage`),
      `pnpm run package` (vsce) succeeds and the resulting `.vsix` does not
      contain any `coverage/` path (verify — e.g. `unzip -l *.vsix | grep -i
      coverage` shows nothing — then remove the generated `.vsix` so the tree
      stays clean)
- [x] No source, test, or other config file is changed (this is a
      `.vscodeignore`-only change)

## Decisions

- **`.vscodeignore`-only — `.gitignore` already covers `coverage/`.** The user
  approved also adding `coverage/` to `.gitignore`, but verification showed it
  is already ignored (`.gitignore:5`), so the only remaining change is the
  `.vscodeignore` entry `vsce package` reads. A `coverage/` entry (trailing
  slash) matches the directory.

## Non-Goals

- Changing `.gitignore`. `coverage/` is already git-ignored
  (`rlsp-yaml/integrations/vscode/.gitignore:5`), so no git change is needed.
- Any change to `vitest.config.mts`, the coverage reporter, or the build.
