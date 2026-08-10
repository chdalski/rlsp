**Repository:** root
**Status:** NotStarted
**Created:** 2026-08-10

# VS Code extension: trim redundant CRLF guard tests + fix vitest ESM config warning

## Goal

Two config/test-hygiene follow-ups surfaced by the dep-bumps work
(`2026-08-10-vscode-dep-bumps-minus-ts7-and-ci-gates.md`), now being
addressed. (1) `src/overrides.test.ts` carries CRLF-scenario tests that
exercise a case `.gitattributes` already prevents, so they add surface
without value. (2) `vitest@4.1.10` / `vite@8.2.1` emits a deprecation warning
that `vitest.config.ts` is loaded as CommonJS while using ESM syntax; a
future Vite major will flip the default loader and break it. Trim the
redundant tests and fix the config at its root, without weakening the
security guard's actual assertions or the extension's build.

## Context

- **CRLF tests (`src/overrides.test.ts`, ~224 lines).** The file is the
  brace-expansion / fast-uri security regression guard. `.gitattributes` in
  the extension dir pins the lockfile to LF at checkout
  (`pnpm-lock.yaml text eol=lf`), which overrides `core.autocrlf` — so the
  real lockfile is always LF. Three tests exercise the CRLF path anyway:
  - `describe('CRLF-agnostic lockfile parsing')` — 2 tests over synthetic
    CRLF fixtures.
  - `describe('CRLF parity against the real lockfile')` — 1 test that
    transforms the real lockfile to CRLF and checks parity.
  These test a scenario `.gitattributes` prevents. The `normalizeLineEndings`
  helper is still used by the guard to read the lockfile and is cheap
  insurance against a stray non-LF checkout silently disabling the guard —
  it and its `describe('normalizeLineEndings')` unit tests (2 tests) stay.
  The security-guard assertions (brace-expansion ≥ 2.1.4 / overridden 5.x;
  fast-uri ≥ 3.1.5) and the `pnpm.overrides` block are out of scope.
  `resolvedDependencyVersion` and `allResolvedVersions` remain exercised by
  the guard's own assertions on the real (LF) lockfile after the CRLF direct
  tests are removed — no helper becomes orphaned/untested.

- **vitest config warning.** `vitest.config.ts` uses ESM
  (`import` / `export default`); `package.json` has no `"type"` field, so
  Node treats it as CommonJS and Vite's native config loader warns. The file
  is **not** in `tsconfig`'s `include` (`["src"]`) and is not covered by
  `eslint src/` or `prettier 'src/**/*.ts'`, so renaming it does not affect
  typecheck/lint/format; vitest auto-discovers config by extension, including
  `.mts`. One place references it by name: `.vscodeignore` (line 9,
  `vitest.config.ts`), which `vsce package` reads to decide VSIX contents — so
  Task 2 updates that entry to `vitest.config.mts`, leaving no dead ignore
  rule and keeping the config out of the packaged VSIX. Renaming
  `vitest.config.ts` → `vitest.config.mts` makes the module type match the ESM
  syntax and silences the warning at its root. The integration-test config
  `.vscode-test.mjs` is already ESM and untouched.

- **Landing model:** trunk-based — both tasks land directly on `main` through
  the developer → reviewer pipeline (each squashed and committed to `main`).

- **References:** `.claude/rules/simplicity.md` (KISS / YAGNI / Fewest
  Elements), `.claude/rules/root-cause-discipline.md` (fix the config, do not
  suppress the warning with `VITE_CONFIG_NATIVE_IGNORE_WARNING`).

## Steps

- [x] Investigate both items and confirm the fixes are safe (gitattributes,
      tsconfig include, config references)
- [x] Confirm CRLF trim scope with user (Moderate: keep `normalizeLineEndings`
      + unit tests; drop the 2 CRLF-scenario blocks)
- [ ] Task 1 — Trim the redundant CRLF-scenario tests in `overrides.test.ts`
- [ ] Task 2 — Rename `vitest.config.ts` → `vitest.config.mts`
- [ ] (Lead) After Task 1 lands, remove the "`overrides.test.ts` accumulated
      test surface — consolidation candidate" entry from
      `.ai/memory/project_followup_plans.md` (Open: CI & integrations) — this
      plan resolves it, so it must not linger as an open follow-up

## Tasks

### Task 1: Trim the redundant CRLF-scenario tests in overrides.test.ts

Remove the CRLF tests that exercise the LF-pinned-lockfile scenario
`.gitattributes` already prevents, keeping the cheap `normalizeLineEndings`
robustness and leaving the security guard's assertions untouched.

- [ ] The `describe('CRLF-agnostic lockfile parsing')` and
      `describe('CRLF parity against the real lockfile')` blocks are removed
- [ ] `normalizeLineEndings` and its `describe('normalizeLineEndings')` unit
      tests remain; it is still used to read the lockfile in the guard
- [ ] The security-guard assertions (brace-expansion / fast-uri) and the
      `pnpm.overrides` regression-guard `describe` are unchanged
- [ ] No helper is left orphaned/untested: `resolvedDependencyVersion` and
      `allResolvedVersions` remain exercised by the guard's assertions
- [ ] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` all pass

### Task 2: Rename vitest.config.ts to vitest.config.mts

Make the config an explicit ESM module so Vite stops loading ESM-syntax as
CommonJS, fixing the deprecation warning at its root rather than suppressing
it or changing the whole package's module system. Update the matching
`.vscodeignore` entry so the rename leaves no dead ignore rule and the config
stays out of the packaged VSIX.

- [ ] `vitest.config.ts` is renamed to `vitest.config.mts` with its contents
      unchanged; no `"type": "module"` is added to `package.json`
- [ ] `.vscodeignore`'s entry matches the new filename (`vitest.config.mts`),
      leaving no dead rule; `pnpm run package` (vsce) succeeds and the
      resulting `.vsix` does not contain the vitest config (verify, then
      remove the generated `.vsix` so the tree stays clean)
- [ ] `pnpm run test` and `pnpm run test:coverage` still discover and use the
      config — tests run and coverage is generated
- [ ] The "ESM syntax in a file loaded as CommonJS" deprecation warning no
      longer appears in the vitest/vite output (cite the before/after)
- [ ] `pnpm run typecheck`, `pnpm run lint`, `pnpm run format`, and
      `pnpm run build` are unaffected

## Decisions

- **Moderate CRLF trim (user choice).** Keep `normalizeLineEndings` + its unit
  tests as cheap defense so the security guard cannot silently misfire on a
  stray non-LF lockfile; remove only the CRLF-scenario blocks that
  `.gitattributes` makes unreachable. (User picked "Moderate" over removing
  all CRLF machinery.)
- **Rename to `.mts` over alternatives.** `"type": "module"` on
  `package.json` would change the whole package's module resolution and risks
  the esbuild CJS build (`--format=cjs`, `main: ./out/main.js`) and the
  vscode-test runner; `VITE_CONFIG_NATIVE_IGNORE_WARNING=true` only hides the
  warning (a symptom mask, per root-cause-discipline). Renaming the single
  config file (and its `.vscodeignore` entry) is the targeted root-cause fix.
- **Two tasks in one plan.** Task 1 (test hygiene) and Task 2 (vitest config)
  touch disjoint files and are unrelated except as sibling follow-ups from the
  dep-bumps plan; each is independently committable (one commit each). Kept in
  one plan deliberately — both are small, already scoped, and the user asked
  for them together — rather than split into two plans.

## Non-Goals

- Changing the security-guard assertions, the `normalizeLineEndings` helper,
  or the `pnpm.overrides` block.
- Adding `"type": "module"` to `package.json` or otherwise changing the
  extension's module system / esbuild build.
- Touching `.vscode-test.mjs` or the integration-test setup.
