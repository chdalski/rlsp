**Repository:** root
**Status:** InProgress
**Created:** 2026-08-10

# VS Code extension: land Dependabot bumps (minus TypeScript 7) + close the CI blind spot

## Goal

Dependabot PR #58 groups 12 VS Code extension dependency bumps but cannot
merge as-is: `typescript@7.0.2` is unsupported by `typescript-eslint`
(lint aborts entirely), and `vscode-languageclient@9→10` breaks the
typecheck (its client now requires a `LogOutputChannel`). Land the 11
non-TypeScript bumps directly on `main` (trunk-based) — including the
`vscode-languageclient@10` code migration and the refreshed dependency
graph — and close the CI gap that let the typecheck and lint failures
reach a "mostly green" PR. Keep the extension's npm security posture
(brace-expansion / fast-uri advisories) intact.

## Context

- **Source of the bumps — PR #58** (Dependabot group
  `vscode-extension-dependencies`, all under
  `rlsp-yaml/integrations/vscode/`). The 12 proposed updates:

  | Package | From | To | Notes |
  |---------|------|----|-------|
  | vscode-languageclient | 9.0.1 | 10.1.0 | **runtime dep**; major — LogOutputChannel API change |
  | @vscode/test-electron | 2.5.2 | 3.1.0 | dev; major (integration-test runner) |
  | @types/node | 25.6.0 | 26.1.2 | dev; major |
  | @types/vscode | 1.116.0 | 1.125.0 | dev |
  | @vitest/coverage-v8 | 4.1.5 | 4.1.10 | dev |
  | @vscode/test-cli | 0.0.12 | 0.0.15 | dev |
  | eslint | 10.2.1 | 10.8.0 | dev |
  | prettier | 3.8.3 | 3.9.6 | dev |
  | typescript-eslint | 8.59.0 | 8.66.0 | dev |
  | vite | 8.0.16 | 8.2.1 | dev |
  | vitest | 4.1.5 | 4.1.10 | dev |
  | typescript | 6.0.3 | 7.0.2 | dev — **EXCLUDED** (see Decisions) |

- **Why TypeScript 7 is excluded:** `pnpm run lint` on the PR branch aborts
  with *"typescript-eslint does not support TS 7.0"* (upstream tracks TS
  ≥7.1 support in typescript-eslint issue #10940). ESLint is a required
  project gate (`strictTypeChecked` + `stylisticTypeChecked`), so TS 7
  cannot land until typescript-eslint supports it. `typescript` stays at
  `6.0.3`.

- **The vscode-languageclient@10 API change (the typecheck failure).**
  v10's `LanguageClientOptions.outputChannel` requires a `LogOutputChannel`
  (adds `logLevel`, `trace`, `debug`, …), not a plain `OutputChannel`. The
  channel is created in `src/main.ts` (`window.createOutputChannel(...)`)
  and flows as `OutputChannel` through `src/client.ts` and `src/commands.ts`.
  `window.createOutputChannel(name, { log: true })` returns a
  `LogOutputChannel`, which extends `OutputChannel` (so `.show()` and
  disposal still work). Only the one typecheck error was observed under the
  full bump set; `@types/node@26` / `@types/vscode@1.125` did not introduce
  others. Confirm `engines.vscode` in `package.json` is not outpaced by the
  `@types/vscode` bump (do not use APIs newer than the declared engine).

- **The stale regression-guard test (the unit-test / only red PR check).**
  `src/overrides.test.ts` is a security guard for the brace-expansion
  (GHSA-3jxr-9vmj-r5cp) and fast-uri advisories. It probes
  `minimatch@5.1.9` — the version `vscode-languageclient@9` pulled — to
  assert brace-expansion resolves non-vulnerable. `vscode-languageclient@10`
  pulls `minimatch@10.2.6` instead, so that exact-version probe now resolves
  to `undefined` and the assertion fails. Verified in the PR lockfile the
  posture is still safe: brace-expansion resolves to `2.1.4` / `5.0.9`
  (overridden major) and fast-uri to `3.1.5` — all non-vulnerable; the
  `pnpm.overrides` block (`brace-expansion@5`, `serialize-javascript`) is
  unchanged. So this is a stale test, not a real regression — but hardcoding
  the next exact minimatch version reproduces the same brittleness on the
  following bump.

- **The CI blind spot.** On pull requests the only extension gate is
  `coverage.yml`'s `coverage-vscode` job (`pnpm run test:coverage` — vitest
  unit tests). **No workflow runs `typecheck`, `lint`, or `format` anywhere**
  (verified across all of `.github/workflows/`), and `test:integration` runs
  only on push-to-`main` via `vscode-extension.yml` (`on: push: branches:
  [main]` + `workflow_dispatch`). That is why the typecheck and lint
  failures never surfaced as red PR checks. `coverage.yml` already runs on
  both `pull_request:` and `push:` to `main` and already sets up pnpm for the
  extension — it is the natural home for fast static gates.

- **Available scripts** (`package.json`): `typecheck` (`tsc --noEmit`),
  `lint` (`eslint src/`), `format` (`prettier --check`), `test`,
  `test:coverage`, `test:integration` (`tsc && vscode-test`), `audit`
  (`pnpm audit --audit-level=low --ignore-registry-errors`), `build`.

- **Dependabot config** (`.github/dependabot.yml`): the npm entry groups all
  patterns (`vscode-extension-dependencies`). It has precedent for a
  documented `ignore` (the `dtolnay/rust-toolchain` git-branch pin under the
  github-actions entry).

- **Landing model:** trunk-based — the work lands directly on `main` through
  the normal developer → reviewer pipeline (each task squashed and committed
  to `main`). No feature branch. PR #58 is superseded once the equivalent
  work lands; a `@dependabot rebase` comment then lets Dependabot re-evaluate
  the group against the new `main` and self-close its now-empty PR (lead
  action at completion).

- **References:** typescript-eslint TS 7 support tracking (issue #10940);
  VS Code `LogOutputChannel` API (`window.createOutputChannel(name, {log:
  true})`); GitHub Actions workflow conventions in
  `.claude/rules/github-workflows.md` (pin action majors, explicit
  least-privilege `permissions`, refresh action versions at touch-time).

## Steps

- [x] Diagnose PR #58 (three failures + CI blind spot) and confirm security
      posture in the PR lockfile
- [x] Confirm approach with user (fix & land most; add PR gates;
      trunk-based on `main`)
- [x] Task 1 — Add extension static-check CI gates + Dependabot TS-major ignore
- [x] Task 2 — Apply the 11 non-TS bumps + vscode-languageclient@10 migration
      + refresh the regression-guard test
- [ ] (Lead) Verify push-triggered CI on `main` is green across the matrix
      (incl. Windows) after the work lands — `gh run list`
- [ ] (Lead) Comment `@dependabot rebase` on PR #58 after both tasks land, so
      Dependabot re-evaluates the group against the new `main` — with the 11
      bumps already present and the TypeScript major ignored, Dependabot
      closes the now-empty PR itself
- [ ] (Lead) At plan completion, record the `overrides.test.ts`
      CRLF/lockfile-parity test-surface consolidation candidate in
      `.ai/memory/project_followup_plans.md` (under "Open: CI & integrations")
      so the deferred concern stays discoverable after this plan freezes

## Tasks

### Task 1: Add extension static-check CI gates and a Dependabot TypeScript-major ignore

Close the CI blind spot so future dependency bumps cannot merge with broken
types, lint, or formatting, and stop Dependabot from re-proposing the
un-landable TypeScript 7 major. This task changes CI/config only — no
extension source or test changes — and is landed first so the gates are in
place before the dependency change. `main` is currently green on typecheck,
lint, and format under the existing dependency set, so adding the gates does
not turn `main` red.

- [x] `typecheck`, `lint`, and `format` for the extension run in CI on both
      pull requests to `main` and pushes to `main`, and a failure of any of
      them fails the check
- [x] The new gates surface as clearly identifiable checks (a failing
      typecheck is distinguishable from a failing lint/format in the CI UI)
- [x] `.github/dependabot.yml` excludes `typescript` semver-major updates
      from the npm group while still allowing its minor/patch updates, with a
      comment stating the reason (typescript-eslint lacks TS 7 support;
      issue #10940) and the condition for removal
- [x] Any workflow file touched has explicit least-privilege `permissions`
      and its actions pinned to current major versions (per
      `.claude/rules/github-workflows.md`)
- [x] `.github/workflows/*.yml` and `.github/dependabot.yml` remain valid
      YAML; the changed workflow parses (e.g. `actionlint` if available, or a
      YAML parse check)

### Task 2: Apply the 11 non-TypeScript bumps, migrate to vscode-languageclient@10, and refresh the regression-guard test

Bring the extension's dependencies up to the PR #58 versions except
`typescript` (kept at 6.0.3), adapt the source to the `vscode-languageclient@10`
`LogOutputChannel` API, and refresh the brace-expansion / fast-uri regression
guard so it tracks the new dependency graph without silently going vacuous.
These are one atomic slice: the lockfile, the source migration, and the test
must land together or the typecheck/tests break.

- [x] `package.json` and `pnpm-lock.yaml` reflect the 11 target versions
      from the table above; `typescript` remains `6.0.3`; the `pnpm.overrides`
      block (`brace-expansion@5`, `serialize-javascript`) is unchanged
- [x] The extension source compiles against `vscode-languageclient@10`: the
      output channel supplied to the client is a `LogOutputChannel` and the
      types flowing through `client.ts` / `commands.ts` match, with no API
      used that exceeds the declared `engines.vscode`
- [x] The brace-expansion / fast-uri regression guard asserts the *current*
      dependency graph resolves non-vulnerable versions (brace-expansion
      ≥ 2.1.4 except the intentionally-overridden 5.x line ≥ 5.0.9; fast-uri
      ≥ 3.1.5) and fails if a future bump regresses into an advisory range —
      it must not vacuously pass because a probed transitive version is
      absent
- [x] `pnpm run audit` passes (low+ severity) with the new lockfile
- [x] `pnpm run typecheck`, `pnpm run lint`, `pnpm run format`,
      `pnpm run build`, and `pnpm run test` all pass locally
- [x] `pnpm run test:integration` passes under `xvfb`
      (`xvfb-run -a pnpm run test:integration`). A genuine inability to run
      the VS Code test harness in the sandbox is escalated as a blocker with
      the exact command and error — not a self-authorized skip; the
      Steps-level post-landing CI check is the backstop

## Decisions

- **Exclude TypeScript 7, land the other 11.** typescript-eslint has no TS 7
  support, and lint is a required gate — TS 7 is un-landable now. A
  Dependabot `ignore` (major-only) prevents re-proposal while keeping
  minor/patch TS updates flowing; remove it once typescript-eslint supports
  TS ≥7.1. (User: "Fix & land most.")
- **Trunk-based, land on `main`.** No feature branch; work flows through the
  developer → reviewer pipeline and is committed directly to `main`. Rather
  than closing PR #58 by hand, a `@dependabot rebase` comment (after the work
  lands) makes Dependabot re-evaluate the group against the new `main` — the
  11 bumps already present and the TypeScript major ignored — so it closes its
  own now-empty PR. (User: "Land it on main - trunk based"; "comment for
  dependabot to rebase.")
- **PR gates = fast static checks (typecheck + lint + format).**
  `test:integration` is heavy (downloads a VS Code build) and already runs on
  push-to-`main` via `vscode-extension.yml`; keeping it there (plus the
  post-landing CI verification) avoids slow/flaky PR runs while still closing
  the type/lint/format gap that caused this. (User: "add PR gates.")
- **`format` included alongside typecheck + lint.** Prettier's `format` check
  is ungated by the same omission; adding it now prevents the identical class
  of drift for one extra cheap step.
- **Regression guard tracks intent, not a hardcoded version.** Re-pinning
  the next exact minimatch version would break again on the following bump;
  the guard is refreshed to fail on real advisory regressions without going
  vacuous when a transitive version changes. Final form is the test-engineer's
  call at input-gate.
- **`overrides.test.ts` growth reviewed; test-surface consolidation kept out
  of scope.** The guard file has been touched by three prior 2026-08-07 plans
  and now carries CRLF-normalization helper tests and lockfile-parity tests
  alongside the security-guard assertions (~224 lines). Task 2's change to it
  is a minimal guard-refresh (track the new dependency graph) that adds no new
  test surface. Whether the CRLF fixture / parity tests still earn their keep
  now that `.gitattributes` pins the lockfile to LF is a test-refactor
  question, distinct from a dependency bump — bundling it here would break
  Task 2's atomicity and give the dep-bump slice latitude to delete
  security-adjacent tests. It is recorded as a Non-Goal and, at completion,
  logged in `.ai/memory/project_followup_plans.md` (the project's live
  backlog) so the aggregate concern stays discoverable after this plan
  freezes rather than being carried forward silently a fifth time.

## Non-Goals

- Bumping `typescript` to 7.x, or reworking the ESLint/typescript-eslint
  setup to run on the TS 7 API — deferred until typescript-eslint supports
  TS ≥7.1.
- Adding `test:integration` to pull-request CI — it stays on push-to-`main`.
- Changing the `pnpm.overrides` block or the extension's runtime behavior
  beyond what the `vscode-languageclient@10` API requires.
- Rust / other-ecosystem Dependabot groups — only the npm (VS Code
  extension) group is in scope.
- Consolidating or trimming `overrides.test.ts`'s CRLF-normalization and
  lockfile-parity test surface — a separate test-refactor concern (see
  Decisions). Task 2 only refreshes the security guard and adds no new test
  surface. The concern is logged in `.ai/memory/project_followup_plans.md` at
  completion (see Steps) so a dedicated follow-up plan can review the
  accumulated surface if the user wants it.
