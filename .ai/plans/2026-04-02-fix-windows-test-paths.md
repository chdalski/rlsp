**Repository:** root
**Status:** InProgress
**Created:** 2026-04-02

## Goal

Fix 3 test failures in `server.test.ts` on Windows CI — tests use
Unix-style hardcoded paths in assertions but `path.resolve()` and
`path.join()` produce Windows paths on Windows runners.

## Context

- Tests pass on Linux/macOS, fail on Windows
- `findServerBinary` uses `path.resolve()` for custom paths and
  `path.join()` for bundled paths — both are platform-dependent
- Tests hardcode Unix paths like `/usr/local/bin/rlsp-yaml` in
  expected values
- Fix: use `path.resolve()` / `path.join()` in assertions so
  expected values match the platform

## Steps

- [x] Fix platform-dependent test assertions

## Tasks

### Task 1: Fix Windows path assertions in server.test.ts

Three failing tests in `rlsp-yaml/integrations/vscode/src/server.test.ts`:

- [x] Line 11: `path.resolve('/usr/local/bin/rlsp-yaml')` (774d849)
- [x] Line 85: `path.join(EXT, '')` — normalizes separators without drive letter (774d849)
- [x] Line 134: `path.resolve('/usr/local/../../bin/rlsp-yaml')` (774d849)

## Decisions

- Use `path.resolve()` in expected values — matches what the
  implementation does, platform-correct on all OSes
