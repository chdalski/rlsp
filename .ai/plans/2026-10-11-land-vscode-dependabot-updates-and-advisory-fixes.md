**Repository:** root
**Status:** InProgress
**Created:** 2026-10-11

# Land the Open VS Code Dependabot Updates and Close the Open Security Advisories

## Goal

Two Dependabot pull requests and six Dependabot security alerts are open
against the VS Code extension. Instead of merging the pull requests by hand,
land equivalent changes directly on `main`: the 8-package group update
(PR #79), and the `@types/vscode` bump to 1.140.0 together with the
`engines.vscode` floor it requires (PR #82). Also close all six open
security alerts: patched versions of the affected packages must resolve, and
a `pnpm.overrides` floor must guarantee each one. The guarantee must not rest
on which version the resolver happened to pick. `pnpm audit` must come back
clean apart from the existing allowlisted finding. The work is done when both
pull requests and all six alerts are closed and CI on `main` is green across
the full matrix.

## Context

- **The open Dependabot items.**
  - PR #79 bumps the `vscode-extension-dependencies` group. Its head commit
    `6611f8c9` changes only `rlsp-yaml/integrations/vscode/package.json` and
    `pnpm-lock.yaml`:
    - `vscode-languageclient` 10.1.1 → 10.1.2. This is the only runtime
      dependency in the group; it is bundled into the shipped extension.
    - `@types/node` 26.5.1 → 26.6.4
    - `@vitest/coverage-v8` 5.0.0 → 5.0.3
    - `eslint` 10.10.0 → 10.12.0
    - `prettier` 3.9.6 → 3.9.9
    - `typescript-eslint` 8.70.0 → 8.71.1
    - `vite` 8.3.0 → 8.3.2
    - `vitest` 5.0.0 → 5.0.3

    `main` has not touched either file since the PR's base commit
    `ff4fe51a`.
  - PR #82 bumps `@types/vscode` 1.138.0 → 1.140.0. It fails the
    `VS Code Extension Coverage` CI job by design: `.github/dependabot.yml`
    keeps `@types/vscode` out of the group because it must move in lockstep
    with `engines.vscode`.
  - PR #44 is a release-plz release PR, not a Dependabot PR.

- **Verification of PR #79 already done (lead, 2026-10-11).** Run on the
  PR's head commit:
  - `pnpm install --frozen-lockfile`, `build`, `test` (123/123), `typecheck`,
    `lint`, and `format` all pass.
  - `xvfb-run -a pnpm run test:integration` passes: 19 passing, 1 pending by
    design. The pending test is the no-binary activation case, skipped
    because a server binary was present.
  - The lockfile adds no new package names. It removes `siginfo` and
    `stackback`.
  - `vscode-languageclient@10.1.2` declares `engines.vscode: ^1.91.0`.
  - `pnpm run audit` fails on exactly one finding, the
    `serialize-javascript` advisory described below. `main` fails the same
    gate today with 8 findings: 5 high, 2 moderate, 1 low.

- **The six open Dependabot alerts.** All six are in
  `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`. Vulnerable bands below
  come from the GitHub Advisory API. Each advisory also lists bands for
  major lines that do not appear in this dependency graph.
  - `brace-expansion`, alerts #59, #61, #63, #64. Three advisories published
    2026-09-29:
    - GHSA-6j4f-fj2g-mc7p, high. Vulnerable `>= 2.0.0, < 2.1.5` and
      `>= 4.0.0, < 5.0.10`.
    - GHSA-qhr7-859c-m2p7, high. Vulnerable `>= 2.0.0, < 2.1.6` and
      `>= 4.0.0, < 5.0.11`.
    - GHSA-q2hr-2g5m-vwhr, medium. Vulnerable `>= 2.0.0, < 2.1.7` and
      `>= 4.0.0, < 5.0.12`.

    The binding patched floors are therefore 2.1.7 for major 2 and 5.0.12
    for major 5. The current overrides `brace-expansion@2: ^2.1.4` and
    `brace-expansion@5: ^5.0.9` are below both floors. `main` resolves
    2.1.4 and 5.0.9; PR #79's lockfile resolves 2.1.7 and 5.0.12.
  - `serialize-javascript`, alert #65. GHSA-gfhx-hw2g-v5hg, low. Vulnerable
    `>= 7.1.1, < 7.1.2`, patched 7.1.2 (published 2026-09-23). The existing
    override `serialize-javascript: ^7.0.5` admits 7.1.1, and both `main`
    and PR #79 resolve 7.1.1. The package is reached through
    `@vscode/test-cli > mocha@11.8.0`, which declares `^6.0.2`. The override
    exists because the 6.x line carries older high and moderate advisories,
    and the newest `@vscode/test-cli` (0.0.15) holds `mocha` to `^11.7.6`.
  - `source-map-js`, alert #66. GHSA-68fv-2mgg-jv7q, high. Vulnerable
    `>= 1.0.0, < 1.2.2`, patched 1.2.2 (published 2026-09-30). It is reached
    through `@vitest/coverage-v8 > magicast@0.5.5` and through
    `vite > postcss@8.5.29`. `main` resolves 1.2.1; PR #79's lockfile
    resolves 1.2.2. No override exists for it today.

- **Declared ranges of the direct dependents, in PR #79's graph.** This is
  the project's test for whether an override is needed (root `CLAUDE.md`
  and the header comment of `overrides.test.ts`). A resolved version above
  the floor only reflects what the resolver happened to pick. The declared
  range is the guarantee.
  - `minimatch@9.0.9` declares `brace-expansion: ^2.0.2`, which admits
    2.0.2 through 2.1.6.
  - `minimatch@10.2.6` declares `brace-expansion: ^5.0.8`, which admits
    5.0.8 through 5.0.11.
  - `mocha@11.8.0` declares `serialize-javascript: ^6.0.2`; the existing
    override replaces this.
  - `magicast@0.5.5` declares `source-map-js: ^1.2.1`, which admits 1.2.1.
    `postcss@8.5.29` declares `^1.2.2`, which is already bounded.

  So the declared ranges admit vulnerable versions of all three packages.

- **Key files.** Everything in scope is under
  `rlsp-yaml/integrations/vscode/`:
  - `package.json`: `engines.vscode`, `dependencies`, `devDependencies`,
    and the `pnpm.overrides` and `pnpm.auditConfig` blocks.
  - `pnpm-lock.yaml`.
  - `src/overrides.test.ts`. This test file is itself the regression guard
    for advisory overrides; there is no production module behind it. It
    asserts which pins the lockfile's `overrides:` block declares. Through
    a package-keyed, major-aware `patchedFloors` table and
    `isPatchedVersion`, it checks every resolved `brace-expansion` and
    `fast-uri` version against its patched floor. Its header comment
    records the advisory bands and the rule for when an override is
    redundant. Today the table and the comments name GHSA-rgw5-rvv9-x895
    and its floors 2.1.4 and 5.0.9. `serialize-javascript` is checked only
    as a string in the overrides block, never as a resolved version.
  - `src/engine-compat.test.ts`. This guard enforces that
    `devDependencies["@types/vscode"]` is an exact pin, that it agrees with
    `engines.vscode` on major.minor in both directions, and that the
    lockfile resolves that exact version.

- **The audit gate.** `pnpm run audit` runs
  `pnpm audit --audit-level=low --ignore-registry-errors`. Any finding that
  is not allowlisted fails it. `pnpm.auditConfig.ignoreCves` allowlists only
  CVE-2026-24001, a dev-only jsdiff advisory.

- **What CI covers before and after the push.**
  - On pull requests, `ci.yml` and `coverage.yml` run unit tests with
    coverage, `typecheck`, `lint`, and `format`.
  - Only on push to `main` (path-filtered to the extension directory),
    `.github/workflows/vscode-extension.yml` also does the following.
    Because of the path filter, the push in this plan triggers it.
    - Builds VSIX packages on Linux x64/arm64, macOS x64/arm64, and
      Windows x64.
    - Runs `test:integration` and `pnpm run audit` on the Linux x64 leg.

    A push to `main` never publishes to the Marketplace. Publishing and
    tagging run only on `workflow_dispatch`.

- **VS Code versions.** `@types/vscode` 1.140.0 was published 2026-10-01
  and is the newest release. The current VS Code stable is 1.141.0. No
  documentation states a minimum VS Code version; `engines.vscode` in
  `package.json` is the only place it is recorded. The integration tests
  download the current stable VS Code.

- **Dependabot cooldown.** Dependabot resolves with a 72-hour minimum
  release age. Newer patch releases of three group members exist but are
  younger than that as of 2026-10-11:
  - `@types/node` 26.6.5 (2026-10-09)
  - `prettier` 3.9.10 (2026-10-10)
  - `vite` 8.3.4 (2026-10-08)

- **Closing the pull requests.** Once equivalent changes are on `main`, a
  `@dependabot rebase` comment makes Dependabot re-evaluate each pull
  request against `main` and close it itself. That is the established
  practice in this repository.

## Steps

- [x] Verify PR #79 locally, including the gates PR CI does not run
- [x] Inventory the open Dependabot pull requests and alerts, and the
      advisories' vulnerable bands
- [x] Check each advisory package's direct-dependent declared ranges
- [x] Confirm with the user that `engines.vscode` moves to 1.140
- [x] Land PR #79's group update on `main`
- [x] Raise or add the advisory override floors and extend the guard to
      cover them
- [x] Move `@types/vscode` and `engines.vscode` to 1.140.0 together
- [ ] Push to `main` and confirm every workflow the push triggers succeeds,
      across every matrix leg
- [ ] Confirm all six Dependabot alerts are closed as fixed
- [ ] Comment `@dependabot rebase` on PR #79 and PR #82, and confirm
      Dependabot closes both

## Tasks

### Task 1: PR #79's group update lands on `main`

Land the eight dependency updates PR #79 proposes as our own commit on
`main`, with the exact package manifest and lockfile Dependabot produced and
CI verified. This slice supersedes PR #79.

- [x] `package.json` and `pnpm-lock.yaml` under
      `rlsp-yaml/integrations/vscode/` are byte-identical to their content
      at commit `6611f8c9`
- [x] `pnpm install --frozen-lockfile` succeeds
- [x] `pnpm run build`, `pnpm run test`, `pnpm run typecheck`,
      `pnpm run lint`, and `pnpm run format` pass
- [x] `xvfb-run -a pnpm run test:integration` passes with a server binary
      present at
      `rlsp-yaml/integrations/vscode/server/x86_64-unknown-linux-gnu/rlsp-yaml`,
      so the activation-success test runs rather than being skipped
- [x] The commit contains no file outside `package.json` and
      `pnpm-lock.yaml`; the test server binary and other scratch output
      are not committed

### Task 2: Every open advisory is fixed by an override floor and guarded

Bound `brace-expansion` (both major lines), `serialize-javascript`, and
`source-map-js` by `pnpm.overrides` floors at their advisories' patched
versions. The guarantee then comes from the overrides rather than from
whatever the resolver picked. Extend `overrides.test.ts` so that a resolved
version of any of these packages below its patched floor fails the test
suite. This clears the last audit finding and closes all six alerts.

- [x] `pnpm.overrides` bounds `brace-expansion@2` at or above 2.1.7,
      `brace-expansion@5` at or above 5.0.12, `serialize-javascript` at or
      above 7.1.2, and `source-map-js` at or above 1.2.2. The lockfile's
      `overrides:` block declares the same pins as `package.json`
- [x] Compared with the lockfile at the end of Task 1, the only change in
      resolved package versions is `serialize-javascript` 7.1.1 → 7.1.2
- [x] `pnpm run audit` exits 0, and its report shows no finding other than
      the allowlisted CVE-2026-24001
- [x] `pnpm.auditConfig.ignoreCves` is unchanged
- [x] The lockfile guard in `overrides.test.ts` fails when any resolved
      version of `brace-expansion` (major 2 or 5), `serialize-javascript`,
      or `source-map-js` is below the floor named in the first criterion
      of this task. It also fails when an overridden package disappears
      from the lockfile entirely, so that an override that stops matching
      anything fails loudly instead of passing on an empty set
- [x] For `serialize-javascript` and `source-map-js`, the guard is closed
      to major lines it has not checked, the same way it already is for
      `brace-expansion` and `fast-uri`: a resolved version on any other
      major line fails. Literal cases that do not depend on the current
      lockfile show this for each package: a version one patch below the
      floor is rejected, the floor itself is accepted, and a version on a
      major line with no floor is rejected
- [x] The `brace-expansion` literal cases cover the new floors: 2.1.6 and
      5.0.11 are rejected, and 2.1.7 and 5.0.12 are accepted
- [x] In `overrides.test.ts`, no comment or test title names a superseded
      floor as current. In particular, 2.1.4 and 5.0.9 never appear as the
      current `brace-expansion` floors, and GHSA-rgw5-rvv9-x895 is never
      named as the advisory that sets them. Each floor in the floor table
      has a comment naming the advisory or advisories that set it. Every
      `describe` title that lists guarded packages lists all of them
      (`brace-expansion`, `fast-uri`, `serialize-javascript`,
      `source-map-js`)
- [x] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass

### Task 3: `@types/vscode` and `engines.vscode` move to 1.140.0 together

Raise `@types/vscode` to 1.140.0, the newest release, and raise
`engines.vscode` in the same commit so the lockstep guard holds. This raises
the minimum VS Code version needed to install the extension from 1.138 to
1.140. This slice supersedes PR #82.

- [x] `engines.vscode` is `^1.140.0`, and
      `devDependencies["@types/vscode"]` is the exact pin `1.140.0` with no
      range prefix
- [x] Compared with the lockfile at the end of Task 2, the only change in
      resolved package versions is `@types/vscode` 1.138.0 → 1.140.0
- [x] Every assertion in `engine-compat.test.ts` passes against the real
      `package.json` and `pnpm-lock.yaml`
- [x] `pnpm run build`, `pnpm run test`, `pnpm run typecheck`,
      `pnpm run lint`, `pnpm run format`, and `pnpm run audit` pass
- [x] `xvfb-run -a pnpm run test:integration` passes with a server binary
      present, as in Task 1
- [x] `pnpm run package` produces a `.vsix` with no `@types/vscode`
      compatibility error from vsce, and the `.vsix` is not committed

## Decisions

- **Land on `main` as our own commits rather than merging the Dependabot
  branches** (user choice). Dependabot closes the superseded pull requests
  after a `@dependabot rebase` comment.
- **`engines.vscode` moves to 1.140 together with `@types/vscode`** (user
  choice). Users on VS Code 1.138 or 1.139 stop receiving extension updates
  until they upgrade.
- **Task 1 takes PR #79's exact versions, not the newest releases.**
  `@types/node` 26.6.5, `prettier` 3.9.10, and `vite` 8.3.4 are younger than
  Dependabot's 72-hour cooldown. PR #79's versions are all past it, and CI
  and the lead's local run have already verified them. The newer patches
  arrive through the next scheduled Dependabot run.
- **Every advisory is fixed by an override floor, not only by the lockfile
  resolving a patched version.** The declared ranges of `minimatch@9.0.9`,
  `minimatch@10.2.6`, and `magicast@0.5.5` admit vulnerable versions. That
  makes PR #79's patched resolutions a matter of resolver preference, which
  the project's override rule does not accept as a guarantee. A new
  `source-map-js` override is added for the same reason, through
  `magicast`'s `^1.2.1`.
- **The guard covers every advisory override.** `serialize-javascript` and
  `source-map-js` join `brace-expansion` and `fast-uri` in the same
  package-keyed, major-aware floor check. The override prevents a vulnerable
  resolution and the guard detects one. Before this plan,
  `serialize-javascript` was checked only as a string in the overrides
  block, which cannot catch a resolved version below the floor.
- **The lead owns post-task closure, and it gates plan completion.** The
  push, CI verification, alert check, and `@dependabot rebase` steps act on
  the remote repository, not on code, so no task carries them. The plan
  stays open until all of these hold:
  - Every workflow the push triggers has succeeded on every matrix leg,
    including all five `vscode-extension.yml` build legs.
  - All six alerts are in state `fixed`.
  - Dependabot has closed PR #79 and PR #82.
- **Three slices.** The group update, the advisory overrides, and the
  `@types/vscode` floor each have their own reason and their own way of
  failing. Separate commits keep each one reviewable and revertable.

## Non-Goals

- The newer patch releases of `@types/node`, `prettier`, and `vite` that are
  still inside Dependabot's cooldown.
- A TypeScript 7 upgrade. `.github/dependabot.yml` deliberately ignores that
  major until `typescript-eslint` supports it.
- Moving `mocha` to major 12, which carries a patched `serialize-javascript`
  natively. That would force `@vscode/test-cli` outside its declared
  `mocha` range.
- Removing `pnpm.auditConfig.ignoreCves`, or the jsdiff advisory it
  allowlists.
- Release-plz's PR #44, and any Rust or GitHub Actions dependency.
- Changing `.github/dependabot.yml` or any CI workflow.
- Consolidating the structure of `overrides.test.ts` beyond what extending
  the guard requires.
