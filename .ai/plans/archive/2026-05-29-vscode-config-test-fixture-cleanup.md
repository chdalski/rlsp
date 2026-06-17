**Repository:** root
**Status:** Completed (2026-05-29)
**Created:** 2026-05-29

## Goal

Restore `pnpm run test:integration` to exit zero in
`rlsp-yaml/integrations/vscode/` by removing explicit
`undefined` property assignments from the `makeConfigStub`
fixtures in `src/config.test.ts`. The fixtures currently
fail `tsc`'s `exactOptionalPropertyTypes` check, which
blocks the `tsc && vscode-test` pipeline before any
integration test runs. The fix is test-only — no
production code, type declaration, or tsconfig change.

## Context

### Current state (verified at HEAD `633dab55`)

`pnpm run test:integration` fails at the `tsc` step with
seven `TS2345` errors in `src/config.test.ts` (lines 34,
53, 66, 78, 90, 114, 128). Each error has the form:

```text
Argument of type '{ ..., globalValue: undefined, ... }' is not
assignable to parameter of type 'InspectResult'.
  Types of property 'globalValue' are incompatible.
    Type 'undefined' is not assignable to type 'number'.
```

`pnpm run test` (vitest) passes 38/38; vitest transpiles
through esbuild, which does not enforce
`exactOptionalPropertyTypes`. The strictness is enforced
only by the standalone `tsc` invocation in the
integration pipeline.

### Why this exists

Commit `a558adf6` ("fix(vscode): omit unset
formatPrintWidth from LSP settings", 2026-05-20) added
`src/config.test.ts` with 10 vitest unit tests. The
fixtures in that file pass objects like:

```typescript
makeConfigStub({
  globalValue: 80,
  workspaceValue: undefined,
  workspaceFolderValue: undefined,
})
```

— with `undefined` literally written in. The local
`InspectResult` type declares `workspaceValue?: number`
(an optional `number`, not `number | undefined`). Under
`@tsconfig/strictest`'s `exactOptionalPropertyTypes: true`,
the literal `undefined` value is not assignable to an
optional `number` field. The author evidently ran only
`pnpm run test` after adding the file; the failure has
sat on `main` for nine days.

The vscode integration's `tsconfig.json` extends
`@tsconfig/strictest` and uses `"include": ["src"]`, so
every `.test.ts` file under `src/` is type-checked by
`tsc`. The other unit test files in `src/`
(`server.test.ts`, `status.test.ts`) do not use the
`{ field: undefined }` pattern and therefore pass.

### Why this fix and not Option B (widen the type)

A type-widening fix (adding `| undefined` to each
optional field on the local `InspectResult` type) would
also silence the `tsc` errors. It was rejected because:

- `exactOptionalPropertyTypes` was specifically added to
  flag `{ x: undefined }` as a code smell distinct from
  `{ x?: T }`. Widening the type embraces the pattern the
  strictness flag was designed to discourage.
- `rlsp-yaml/integrations/vscode/CLAUDE.md` mandates
  "Maximum TypeScript strictness." Type widening locally
  contradicts that mandate.
- Behavior of the production code is identical under
  both forms because `src/config.ts:30-33` uses optional
  chaining (`printWidthInspect?.workspaceFolderValue ??
  ...`). The literal `undefined` values in the fixtures
  add no test coverage that absence does not already
  cover.

### Scope

All edits are inside `rlsp-yaml/integrations/vscode/src/config.test.ts`.
No production code (`config.ts` or otherwise), no type
declarations, no tsconfig, no vitest config, no package
manifest changes.

### References

- `exactOptionalPropertyTypes`:
  <https://www.typescriptlang.org/tsconfig/#exactOptionalPropertyTypes>
- `@tsconfig/strictest` base config:
  <https://github.com/tsconfig/bases/blob/main/bases/strictest.json>
- Originating commit for the broken fixtures:
  `a558adf6` ("fix(vscode): omit unset formatPrintWidth
  from LSP settings", 2026-05-20).
- VS Code extension strictness mandate:
  `rlsp-yaml/integrations/vscode/CLAUDE.md` ("Maximum
  TypeScript strictness").

## Steps

- [x] Edit each `makeConfigStub({...})` call in
  `src/config.test.ts` to delete property assignments
  whose value is the literal `undefined`
- [x] Verify `tsc` passes (the first half of `pnpm run
  test:integration`)
- [x] Verify `pnpm run test:integration` exits zero
- [x] Verify `pnpm run test` still passes with the same
  baseline suite count
- [x] Verify `pnpm run lint`, `format`, and `build` exit
  zero

## Tasks

### Task 1: Remove explicit `undefined` from `config.test.ts` fixtures

**Commit:** `8106ca951549569a94257a92457929cc0b192811`

Edit `src/config.test.ts` so that every call to
`makeConfigStub({...})` contains only the properties the
test actually exercises. Each property whose value is
the literal `undefined` is deleted. The local
`InspectResult` type declaration is unchanged. No other
file is modified.

Behavior is preserved because the production code at
`src/config.ts:30-33` reads each scope through optional
chaining (`printWidthInspect?.workspaceFolderValue ??
printWidthInspect?.workspaceValue ??
printWidthInspect?.globalValue`), under which a missing
property and a present-with-undefined property are
indistinguishable. Each test asserts the same outcome
before and after the edit.

Files involved:

- `rlsp-yaml/integrations/vscode/src/config.test.ts` —
  delete `defaultValue: undefined`, `globalValue:
  undefined`, `workspaceValue: undefined`, and
  `workspaceFolderValue: undefined` lines from every
  `makeConfigStub({...})` call site. The seven call
  sites flagged by `tsc` are at lines 34, 53, 66, 78,
  90, 114, and 128 of the current file.

Sub-tasks (all must be true to pass):

- [x] `grep -nE "(default|global|workspace|workspaceFolder)Value:\s*undefined" src/config.test.ts` returns zero matches
- [x] `pnpm run lint` exits zero
- [x] `pnpm run format` exits zero
- [x] `pnpm run build` exits zero
- [x] `pnpm run test` exits zero — vitest 38/38 passing
- [x] `pnpm run test:integration` exits zero — `tsc`
  clean and vscode-test 19/19 passing
- [x] `git diff` shows changes confined to
  `rlsp-yaml/integrations/vscode/src/config.test.ts`

## Decisions

- **Fixture-edit over type-widening:** chose to delete
  the explicit `undefined` lines (Option A) rather than
  add `| undefined` to each optional field on the
  `InspectResult` type (Option B), because
  `exactOptionalPropertyTypes` exists to flag this
  exact code smell and the project's `CLAUDE.md`
  mandates maximum TypeScript strictness. Widening the
  type would silence the warning at the cost of the
  strictness contract.
- **Local type declaration unchanged:** the
  `InspectResult` type stays as
  `{ globalValue?: number; ... }`. The production code
  reads these fields through optional chaining, so the
  type semantics already match the production behavior
  — only the test fixtures were out of step.
- **Single atomic task:** all seven edits are
  mechanically identical and live in one file.
  Splitting per call site would produce seven commits
  with no intermediate value.
- **No production code changes:** the runtime behavior
  of `getConfig()` is not affected by this fix. The
  bug being fixed is the gap between vitest's
  esbuild-based type checking and the strict `tsc`
  invocation used by the integration pipeline — a
  test-author oversight, not a runtime defect.
- **No advisor consultations directed:** the change is
  test-only, has no trust-boundary or untrusted-input
  surface, follows an established codebase pattern
  (`server.test.ts` and `status.test.ts` already write
  fixtures without explicit `undefined` properties),
  and the verification suite is fully specified above.
  Low risk, low uncertainty.

## Non-Goals

- Changing the `InspectResult` type declaration in
  `config.test.ts`.
- Touching `src/config.ts` or any other production
  source file.
- Changing `tsconfig.json`, `vitest.config.ts`,
  `eslint.config.mjs`, or `package.json`.
- Refactoring the test structure, splitting the test
  file, or extracting helpers.
- Adding new test cases or removing existing ones.
- Repairing or investigating the
  `serde_json`-dependabot workflow failure from
  2026-05-26 (transient GitHub infrastructure error,
  out of scope).
- Adding CI to prevent recurrence (e.g., wiring
  `pnpm run test:integration` into the project's
  pre-commit or CI pipeline) — that is its own concern
  and a separate plan if pursued.
