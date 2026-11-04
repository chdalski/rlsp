**Repository:** root
**Status:** NotStarted
**Created:** 2026-11-04

# VS Code Extension: Override minimatch and glob onto Their Patched Majors

## Goal

The "VS Code Extension" workflow fails in `pnpm run audit` on two moderate advisories, minimatch
below 11.0.1 (GHSA-g3hv-75wg-m249) and glob from 9.0.0 to 14.0.1 (GHSA-3g3m-2fjr-6pw4), and no
release of their consumers declares a patched range. pnpm overrides lift every resolved minimatch to
major 11 and every glob to major 14, the overrides they make redundant go, and the regression guard
in `src/overrides.test.ts` covers the new pins, so the audit passes with the gate as strict as today.

## Context

- All paths are under `rlsp-yaml/integrations/vscode/`; the commands are those of the root
  `CLAUDE.md`, VS Code Extension section.
- `src/overrides.test.ts` guards the overrides: `patchedFloors` holds a floor per package and major,
  `isPatchedVersion` rejects any major without a floor, `allPatched` aggregates with an `onEmpty`
  policy, and tests read `pnpm-lock.yaml` to assert the exact overrides block and the resolved
  versions of each guarded package.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [ ] Task 1: The override guard's lockfile tests are table-driven
- [ ] Task 2: minimatch and glob resolve only on their patched majors
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: The override guard's lockfile tests are table-driven

The lockfile tests in `src/overrides.test.ts` hold one near-identical block per guarded package, and
comments list the packages again. One table gets an entry per package, so a new package is one
entry; the guard fails for the same lockfile states as before.

- [ ] One table holds, per guarded package, whether it must resolve and, where set, the majors it
      may resolve on; one table-driven test checks every resolved version of each package against
      its floor and, where set, its majors, and the per-package lockfile tests are gone
- [ ] The majors in the table are literals, not derived from `patchedFloors`: brace-expansion has 2
      and 5, serialize-javascript 7, source-map-js 1, and fast-uri none
- [ ] The tests of `normalizeLineEndings`, `isAtLeast`, `parseVersion`, `isPatchedVersion`,
      `allPatched`, and `allResolvedVersions`, and the test of the exact overrides block, pass with
      their assertions unchanged
- [ ] Each of three temporary edits to `pnpm-lock.yaml` makes the lockfile tests fail both at the
      commit before this task and after it: an overridden package removed, a version below its
      floor, and a major the guard does not list; the handoff names each edit with both results
- [ ] The comments and test titles in `src/overrides.test.ts` describe the guard after this task:
      only the table and `patchedFloors` name the guarded packages, each package's advisories and
      floors stand at its `patchedFloors` row, and no text says that a package lacks a major check
      the table gives it; the comment in `src/engine-compat.test.ts` no longer points to a fast-uri
      test
- [ ] `pnpm run build`, `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass

### Task 2: minimatch and glob resolve only on their patched majors

Overrides lift minimatch and glob onto their patched majors. minimatch 11 is then the only consumer
of brace-expansion and declares `^5.0.12`, its patched floor, so both brace-expansion overrides go.

- The lockfile resolves minimatch 9.0.9 (through glob 10.5.0 and mocha 11.8.0) and 10.2.6 (through
  `@vscode/test-cli`, eslint, typescript-eslint, and `vscode-languageclient`), and glob 10.5.0
  (through mocha) and 13.0.6 (through `@vscode/test-cli` and c8's test-exclude).
- minimatch is also a runtime dependency: esbuild bundles `vscode-languageclient` and its minimatch
  into `out/main.js`, which ships in the VSIX. The client builds a `Minimatch` with the options
  `dot` and `nocase` in `lib/common/fileOperations.js`, and with `noext` in
  `lib/common/utils/globPattern.js`, and calls `makeRe()` and `match()` on it.
- `pnpm run lint`, `test:integration`, and `pnpm run build` run the new majors: eslint's glob
  matching, `@vscode/test-cli`'s test file gathering, and the bundled client.

- [ ] The overrides in `package.json` are exactly `serialize-javascript: ^7.1.3`,
      `source-map-js: ^1.2.2`, `minimatch: ^11.0.1`, and `glob: ^14.0.2`, and `pnpm-lock.yaml` is
      regenerated from them
- [ ] The lockfile resolves minimatch only on major 11 at 11.0.1 or later, glob only on major 14 at
      14.0.2 or later, and brace-expansion only on major 5 at 5.0.12 or later; the handoff quotes
      the brace-expansion range that each of its dependents declares in the regenerated lockfile
- [ ] `pnpm run audit` passes, and `pnpm.auditConfig.ignoreCves` is unchanged
- [ ] The guard's table and `patchedFloors` cover minimatch (floor 11.0.1, major 11) and glob (floor
      14.0.2, major 14), both required to resolve; brace-expansion must still resolve, on major 5
      only, and its major 2 is rejected like any major without a floor
- [ ] The cases of `isPatchedVersion` and `allPatched` cover for minimatch and glob the versions the
      lockfile resolved before this task, one patch below the floor, the floor, and a later major,
      plus `glob` 11.0.1 and `minimatch` 14.0.2 as cross-package traps, and the brace-expansion
      major 2 cases expect rejection
- [ ] The comments and test titles in `src/overrides.test.ts` describe the guard after this task:
      the `patchedFloors` rows of minimatch and glob carry their advisories and floors, no text
      describes a brace-expansion override or a vetted brace-expansion major 2, and the rule for
      retiring an override stands without the history of the `brace-expansion@2` override
- [ ] The handoff shows from minimatch 11's typings that `require('minimatch')` provides a
      `Minimatch` class whose constructor takes the options `dot`, `nocase`, and `noext` and whose
      instances have `makeRe()` and `match(string)`, as `vscode-languageclient` uses them
- [ ] The handoff names the Node version of VS Code 1.140.0, the lowest that `engines.vscode`
      allows, with the source it was read from, and minimatch 11's `engines.node` admits it
- [ ] `pnpm run build`, `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, `pnpm run format`,
      and `xvfb-run -a pnpm run test:integration` pass

## Non-Goals

- Retiring the `serialize-javascript` or `source-map-js` override
- The allowlisted CVE-2026-24001 and its comment in `.github/workflows/vscode-extension.yml`

## Decisions

- **One unconditional key per package:** `minimatch: ^11.0.1` and `glob: ^14.0.2`, the form of
  `serialize-javascript`; patched releases exist only on these majors. Declined: version-range
  selectors such as `minimatch@<11.0.1`.
- **Both brace-expansion overrides go:** after the override, minimatch 11 is the only consumer of
  brace-expansion and declares `^5.0.12`, the patched floor. Declined: keeping `brace-expansion@5`.
- **The minimatch API that the shipped extension uses is checked from its typings:** the tests do
  not show that every call site of the bundled client works. Declined: an extra
  `vscode-test --coverage` run.
- **The lockfile tests become table-driven first:** a preparatory task, so the new packages are
  table entries instead of copied blocks; serialize-javascript and source-map-js gain a major list
  there. Declined: copying the per-package blocks, and a separate plan after this one.
- **New guard cases grow linearly:** each new package gets its own boundary cases and only the two
  cross-package traps that a table keyed by major alone would get wrong. Declined: traps for every
  pair of packages.
- **The header keeps the rule for retiring an override, not the `brace-expansion@2` history:** that
  override is gone, and the commit history keeps its story.
- **No allowlist:** the CVEs do not go into `pnpm.auditConfig.ignoreCves`, which holds only
  pre-existing dev-only lows. If build, lint, or a test breaks under the new majors, the task stops
  and the decision goes to the user. Declined: allowlisting, and an automatic fallback to it.
