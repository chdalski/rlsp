**Repository:** root
**Status:** Completed (2026-04-12)
**Created:** 2026-04-12

## Goal

Fix the CI-breaking `engines.vscode` / `@types/vscode`
mismatch and update all outdated VS Code extension dev
dependencies to their current major versions, adapting
code and configuration as needed.

## Context

- The prior dependency update (commit 40d4678) bumped
  `@types/vscode` to `^1.115.0` but left `engines.vscode`
  at `^1.93.0`. The `vsce` packager rejects this mismatch,
  breaking the VSIX build pipeline.
- `pnpm outdated` shows 4 packages with major version
  jumps available:

  | Package    | Current | Latest | Risk     |
  |------------|---------|--------|----------|
  | esbuild    | 0.25.12 | 0.28.0 | Low      |
  | ESLint     | 9.39.4  | 10.2.0 | Low      |
  | TypeScript | 5.9.3   | 6.0.2  | Moderate |
  | vitest     | 3.2.4   | 4.1.4  | Low-mod  |

- **esbuild 0.28** — only breaking change is integrity
  checks for fallback downloads; no bundling behavior
  changes for CJS/Node/external-vscode.
- **ESLint 10** — removes ESLintrc (we already use flat
  config), requires config naming, bumps Node minimum.
  `typescript-eslint` 8.58.1 already supports ESLint 10.
- **TypeScript 6** — many default changes (`strict` on by
  default, `module` defaults to `esnext`, `types` defaults
  to `[]`, `esModuleInterop: false` no longer allowed,
  `target: es5` deprecated). Our `tsconfig.json` explicitly
  sets `target`, `module`, `moduleResolution`, `rootDir` —
  so most defaults don't affect us. Main risk: new
  strictness errors and the `types: []` default.
- **vitest 4** — drops Node 18, requires Vite 6+, removes
  `basic` reporter, renames `workspace` → `projects` config.
  Our tests are simple `describe`/`it`/`expect` — low risk.
- These upgrades interact: vitest 4 pulls in vite 6,
  ESLint 10 lints with TypeScript 6. Updating them
  individually creates intermediate broken states.

### Files involved

- `rlsp-yaml/integrations/vscode/package.json` — version
  ranges and engine
- `rlsp-yaml/integrations/vscode/pnpm-lock.yaml` — lockfile
- `rlsp-yaml/integrations/vscode/tsconfig.json` — may need
  adaptation for TypeScript 6
- `rlsp-yaml/integrations/vscode/eslint.config.mjs` — may
  need adaptation for ESLint 10
- `rlsp-yaml/integrations/vscode/src/**/*.ts` — may need
  fixes for new TypeScript/ESLint errors

### References

- [ESLint 10 release](https://github.com/eslint/eslint/releases/tag/v10.0.0)
- [TypeScript 6 announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-6-0/)
- [Vitest 4 release](https://github.com/vitest-dev/vitest/releases/tag/v4.0.0)
- [esbuild changelog](https://github.com/evanw/esbuild/blob/main/CHANGELOG.md)

## Steps

- [x] Research breaking changes for each major version
- [x] Fix engines.vscode mismatch (CI fix) — 4032830
- [x] Update all 4 major-version packages — dcaecc7
- [x] Adapt tsconfig.json for TypeScript 6 if needed — dcaecc7
- [x] Adapt eslint.config.mjs for ESLint 10 if needed — dcaecc7
- [x] Fix any new TypeScript compilation errors — none needed
- [x] Fix any new ESLint errors — none needed
- [x] Verify build, lint, test, and vsce package all pass — dcaecc7

## Tasks

### Task 1: Fix engines.vscode mismatch

Bump `engines.vscode` from `^1.93.0` to `^1.115.0` in
`package.json` to match the already-updated
`@types/vscode: ^1.115.0`. This is a one-line change that
unblocks CI.

- [x] Update `engines.vscode` to `^1.115.0`
- [x] Verify `pnpm run build` succeeds
- [x] Verify `pnpx @vscode/vsce package --no-dependencies`
      succeeds (the specific check that broke CI)

### Task 2: Update major-version dev dependencies

Update all 4 outdated packages to their latest major
versions and adapt code/configuration for breaking changes.

1. Update `package.json` version ranges:
   - `esbuild`: `^0.25.12` → `^0.28.0`
   - `eslint`: `^9.39.4` → `^10.0.0`
   - `typescript`: `^5.9.3` → `^6.0.0`
   - `vitest`: `^3.2.4` → `^4.0.0`
2. Run `pnpm install` to update lockfile
3. Adapt configuration files as needed:
   - `tsconfig.json` — check if TypeScript 6 defaults
     require explicit overrides (especially `types`)
   - `eslint.config.mjs` — add config `name` property if
     ESLint 10 requires it; check for new rule changes
4. Fix any compilation or lint errors in `src/**/*.ts`
5. Verify full quality pipeline:
   - [x] `pnpm run build` succeeds
   - [x] `pnpm run lint` passes
   - [x] `pnpm run test` passes (28 tests)
   - [x] `pnpm run format` passes
   - [x] `pnpx @vscode/vsce package --no-dependencies`
         succeeds

## Decisions

- **Bump engine to ^1.115.0, not a middle ground** — VS Code
  1.115 is the current stable release (2026-04-08). The
  `@types/vscode` package already targets it. No reason to
  pick an intermediate version.
- **All 4 major updates in one task** — these packages
  interact (vitest 4 → vite 6, ESLint 10 lints with TS 6).
  Updating individually creates intermediate broken states
  that are harder to debug than doing them together.
- **Keep serialize-javascript alerts open** — mocha pins
  `^6`, fix requires `>=7.0.3`. Dev-only transitive dep
  with no practical risk. Will resolve when mocha bumps.
