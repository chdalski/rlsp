**Repository:** root
**Status:** InProgress
**Created:** 2026-08-07

# VS Code Extension: Fix Red CI in overrides.test.ts (type errors + Windows CRLF)

## Goal

The `vscode-extension.yml` CI workflow is red across the
matrix, from two distinct bugs in
`rlsp-yaml/integrations/vscode/src/overrides.test.ts`:

1. **Type errors (all platforms, new):** the consolidation
   audit's floor-based guard rework does not compile under
   the project's strictest TypeScript. `test:integration`
   runs `tsc && vscode-test`, so it fails at `tsc`:
   - `overrides.test.ts(47,18)` TS2345 — `match[1]` is
     `string | undefined` under `noUncheckedIndexedAccess`,
     not assignable to `Set<string>.add`.
   - `overrides.test.ts(68,31)` / `(68,38)` TS2532 — tuple
     elements `a[i]` / `f[i]` are possibly `undefined` when
     indexed by a variable.
2. **CRLF line-ending bug (Windows only, pre-existing since
   2026-07-24):** the lockfile-parsing helpers match
   `\n`-delimited patterns (e.g.
   `lockfile.lastIndexOf(\`\n  ${blockHeader}\n\`)`), so on
   the Windows runner — where `pnpm-lock.yaml` is checked out
   with CRLF — they return `undefined`, producing
   `AssertionError: expected undefined to be '…'`. There is
   no `.gitattributes` forcing LF.

Restore the "VS Code Extension" workflow to green across the
full matrix (Linux, Windows, macOS), and close the local
gate gap that let the type errors through (`pnpm run test`
is vitest, transpile-only — it does not typecheck).

## Context

- CI (`.github/workflows/vscode-extension.yml`) runs, per
  matrix job: `pnpm run build`, `pnpm run test` (vitest),
  and `xvfb-run -a pnpm run test:integration`
  (`tsc && vscode-test`). The `tsc` there is what catches
  the type errors; vitest does not typecheck, which is why
  the local gate missed them.
- Extension scripts today: `test` = `vitest run`,
  `test:integration` = `tsc && vscode-test`. There is **no**
  standalone typecheck script.
- Bug 2 is **not reproducible locally** (Linux/LF); its fix
  is verified via the Windows CI job after the change lands.
- The guard is a **security regression guard** — it asserts
  the lockfile resolves non-vulnerable versions for
  brace-expansion and fast-uri. Fixes must preserve that
  detection (the guard must still fail if a version regresses
  into an advisory range); do not weaken assertions to make
  CI pass.
- Workspace TS rules forbid escape hatches: no `as`/`!`
  casts, no `eslint-disable`, no `@ts-expect-error` to
  silence the type errors — fix them with real narrowing.
- `.gitattributes` scope: keep it **targeted** (the lockfile,
  or lockfiles) rather than a repo-wide `* text=auto eol=lf`,
  which would renormalize every file including the Rust tree
  and produce a large unrelated diff.

Key files:
- `rlsp-yaml/integrations/vscode/src/overrides.test.ts`
  (type fixes + line-ending-agnostic parsing)
- `rlsp-yaml/integrations/vscode/package.json` (add a
  `typecheck` script)
- a new `.gitattributes` (targeted LF pin for the lockfile)
- root `/workspace/CLAUDE.md` — its "VS Code Extension"
  command block documents the local gate; adding `typecheck`
  to `package.json` without listing it there leaves the next
  session following the old list, reopening the gap

## Steps

- [x] Reproduce the type errors locally with `tsc --noEmit`
      and confirm the CRLF cause from the Windows CI logs
- [x] Fix the type errors with proper narrowing (no
      casts/escape hatches)
- [x] Make the lockfile parsing line-ending-agnostic
      (normalize CRLF→LF on read) and add a targeted
      `.gitattributes` LF pin for `pnpm-lock.yaml`
- [x] Add a `typecheck` script (`tsc --noEmit`) and run it
      as part of the gate; document it in the root
      `CLAUDE.md` VS Code Extension command block
- [x] Verify `build`, `lint`, `format`, `test`, `typecheck`
      all pass locally
- [ ] After the change lands, confirm the "VS Code Extension"
      workflow succeeds across the full matrix incl. Windows
      (lead, via `gh run`, at plan completion)

## Tasks

### Task 1: Fix the type errors and the CRLF bug, add a typecheck gate

Fix both bugs in `overrides.test.ts`, add a standalone
`typecheck` script so this class of error is caught locally,
and pin the lockfile to LF so it checks out consistently on
Windows.

Files: `src/overrides.test.ts`, `package.json` (new
`typecheck` script), a new `.gitattributes`, and root
`CLAUDE.md` (document the `typecheck` command).

- [x] The three `tsc` errors are resolved with real type
      narrowing/guards — no `as`, `!`, `@ts-expect-error`, or
      `eslint-disable`. `pnpm exec tsc --noEmit` reports 0
      errors.
- [x] The lockfile-parsing helpers are line-ending-agnostic
      (the lockfile content is normalized CRLF→LF before
      parsing, or the patterns tolerate `\r\n`), so
      `resolvedDependencyVersion`, `allResolvedVersions`, and
      the `overrides:` block slice work whether the file has
      LF or CRLF.
- [x] A targeted `.gitattributes` forces `pnpm-lock.yaml` to
      LF (`pnpm-lock.yaml text eol=lf` or equivalent), so the
      file checks out LF on Windows. It does NOT introduce a
      repo-wide renormalization.
- [x] A `typecheck` script (`tsc --noEmit`) exists in
      `package.json` and passes.
- [x] The root `/workspace/CLAUDE.md` "VS Code Extension"
      command block lists `pnpm run typecheck`, so the closed
      gate is documented where the project's build/test
      reference lives.
- [x] The guard's security-detection semantics are unchanged:
      it still fails if brace-expansion (@2/@5 paths) or
      fast-uri resolves to a version inside its advisory
      range. Confirm the floor comparisons and the retained
      brace-expansion@5 exact-match assertion still hold.
- [x] `pnpm run build`, `pnpm run lint`, `pnpm run format`,
      `pnpm run test`, and `pnpm run typecheck` all pass.
- [x] Extension `version` unchanged; no `Cargo.toml`
      modified. Only `src/overrides.test.ts`, `package.json`,
      `.gitattributes`, and root `CLAUDE.md` change.

## Decisions

- **Fix the guard, do not disable it:** the CRLF and type
  errors are fixed so the guard runs correctly on all
  platforms; the guard's assertions are preserved, not
  weakened, per root-cause discipline (the security check
  must keep working).
- **In-test normalization + targeted `.gitattributes`:** the
  in-test CRLF normalization makes parsing robust regardless
  of how the file is checked out; the targeted
  `.gitattributes` prevents the CRLF checkout at the source
  for everyone. Both, because either alone leaves a gap
  (normalization doesn't fix other CRLF-sensitive tooling;
  `.gitattributes` doesn't retroactively fix an already-CRLF
  working copy).
- **Add a `typecheck` gate:** the local gate ran vitest
  (transpile-only) and missed `tsc` errors; a standalone
  `typecheck` script closes that gap without needing the
  display-dependent `test:integration`.

## Non-Goals

- The deferred compensating control (CI `pnpm audit` +
  Dependabot npm coverage) — separate plan, runs next.
- Changing the guard's security thresholds or which packages
  it checks.
- A repo-wide line-ending renormalization or changes to
  other CI workflows.
- Any change to the Rust crates, `Cargo.toml`, or
  `Cargo.lock`.
