**Repository:** root
**Status:** NotStarted
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

- [ ] Fix platform-dependent test assertions

## Tasks

### Task 1: Fix Windows path assertions in server.test.ts

Three failing tests in `rlsp-yaml/integrations/vscode/src/server.test.ts`:

- [ ] Line 11: `expect(result).toBe('/usr/local/bin/rlsp-yaml')` →
  use `path.resolve('/usr/local/bin/rlsp-yaml')`
- [ ] Line 85: `expect(result.startsWith(EXT)).toBe(true)` →
  use `path.join()` or `path.resolve()` for EXT so separators match
- [ ] Line 134: `expect(result).toBe('/bin/rlsp-yaml')` →
  use `path.resolve('/bin/rlsp-yaml')`

## Decisions

- Use `path.resolve()` in expected values — matches what the
  implementation does, platform-correct on all OSes
