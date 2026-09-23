**Repository:** root
**Status:** NotStarted
**Created:** 2026-09-23

## Goal

Two Dependabot pull requests against the VS Code extension are stuck: the
`@vscode/vsce` 4.0.0 bump and the `@types/vscode` bump both fail CI's
`VS Code Extension Coverage` job. Neither upgrade is unsafe — each trips a
regression guard in the extension's own test suite that Dependabot cannot
satisfy on its own, because the guard needs a companion change in the same
commit. Land both upgrades with their companion changes so the guards pass on
their real intent. Separately, measure every entry in the extension's pnpm
override and audit-allowlist configuration against the upgraded dependency
graph, and drop the one the upgrades make redundant — `brace-expansion@5` —
while keeping `serialize-javascript` and the audit allowlist, which still
hold back advisories no package update clears. The overrides block then
carries only pins that still do work.

## Context

- **The two blocked pull requests.** Dependabot PR #75 bumps `@vscode/vsce`
  3.9.2 to 4.0.0; PR #73 bumps `@types/vscode` 1.125.0 to 1.137.0. Both fail
  the same CI job. Once the equivalent changes land on `main`, Dependabot
  closes both automatically.

- **Key files.** Everything in scope lives under
  `rlsp-yaml/integrations/vscode/`: `package.json` (the `engines.vscode`
  field, `devDependencies`, and the `pnpm.overrides` and `pnpm.auditConfig`
  blocks), `pnpm-lock.yaml`, and two guard test files in `src/` —
  `overrides.test.ts` and `engine-compat.test.ts`. Neither guard has a
  production counterpart module; each test file is itself the guard, and
  both carry long header comments explaining what they defend and why.

- **Why PR #73 fails.** `engine-compat.test.ts` enforces that
  `devDependencies["@types/vscode"]` and `engines.vscode` agree on
  `major.minor`, in both directions. Dependabot raised only the types
  package, leaving `engines.vscode` at `^1.125.0`. The same guard separately
  requires `@types/vscode` to be an exact pin rather than a caret range, and
  requires `pnpm-lock.yaml` to resolve that exact version.

- **Why PR #75 fails.** `overrides.test.ts` trips on two assertions, both
  stale guard expressions rather than security regressions:
  - It asserts `brace-expansion@5` resolves to exactly `5.0.9`. The override
    is the range `^5.0.9`, so a fresh resolution legitimately produces
    `5.0.12`. The test's header comment justifies the exact match by claiming
    an override change would be a visible, intentional edit — but a caret
    range permits exactly the silent drift the comment assumes cannot happen.
  - It asserts `fast-uri` resolves to at least one version in the lockfile.
    `@vscode/vsce` 4.0.0 removed the dependencies that pulled `fast-uri` in,
    so the package is absent from the graph entirely and the presence
    assertion fails on absence, not on a vulnerable version.

- **Measured override evidence (lead, 2026-09-23).** Resolving the manifest
  from scratch in a scratch directory, with the agreed upgrades applied,
  gave these results:
  - Dropping `brace-expansion@5: ^5.0.9` changes no resolution:
    `brace-expansion` resolves to `2.1.7` and `5.0.12` with the override and
    without it. `pnpm audit --audit-level=low` reports only the one
    allowlisted low finding either way.
  - Dropping `serialize-javascript: ^7.0.5` regresses
    `serialize-javascript` from `7.1.1` to `6.0.2` and adds one high
    (GHSA-5c6j-r48x-rmvq) and one moderate (GHSA-qj8w-gfj5-8c6v) finding,
    failing the audit gate.
  - Dropping `pnpm.auditConfig.ignoreCves` re-exposes GHSA-73rr-hh4g-fpgx
    (jsdiff denial of service, low) and fails the audit gate.

- **The `serialize-javascript` pin is blocked upstream, not by our code.**
  The chain is `@vscode/test-cli > mocha > serialize-javascript`.
  `@vscode/test-cli` is already at its newest release, 0.0.15, and it
  constrains `mocha` to `^11.7.6`. mocha 11.x depends on
  `serialize-javascript: ^6.0.2`. mocha 12 moved to `^7.1.1`, but
  `@vscode/test-cli`'s range excludes mocha 12. The same chain and the same
  upstream constraint produce the allowlisted jsdiff finding.

- **CI already runs vsce 4.0.0.** `.github/workflows/vscode-extension.yml`
  packages and publishes with unpinned `pnpx @vscode/vsce`, so release jobs
  resolve 4.0.0 today. The `devDependencies` entry governs local development
  and the lockfile, so this upgrade restores parity rather than introducing
  4.0.0 to the pipeline.

- **Node baseline.** `@vscode/vsce` 4.0.0 declares `engines: { node: '>= 22' }`.
  Every `setup-node` step in `.github/workflows/` pins Node 24, and the
  devcontainer runs Node 24.

- **No documentation states a minimum VS Code version.** `engines.vscode` in
  `package.json` is the only place the floor is recorded, so raising it needs
  no documentation change.

- **The audit gate.** `pnpm run audit` runs
  `pnpm audit --audit-level=low --ignore-registry-errors`, so any finding
  that is not allowlisted fails it.

## Steps

- [x] Read both pull requests and the failing CI job logs
- [x] Reproduce each guard failure's cause against the guard source
- [x] Measure each override's effect on resolution and on the audit gate
- [x] Confirm with the user which upgrades to land and how the guards change
- [ ] Land the `@vscode/vsce` upgrade and repair the two stale guards
- [ ] Remove the redundant `brace-expansion@5` override
- [ ] Land the `@types/vscode` upgrade in lockstep with `engines.vscode`
- [ ] Confirm Dependabot closed both pull requests

## Tasks

### Task 1: `@vscode/vsce` 4.0.0 lands and the two stale lockfile guards match their intent

Upgrade `@vscode/vsce` to 4.0.0 and repair the two `overrides.test.ts`
assertions the upgrade exposes as stale. The `brace-expansion@5` assertion
becomes a patched-version floor so it tolerates the drift its own `^5.0.9`
override permits, and the `fast-uri` assertion tolerates the package being
absent from the graph while still rejecting a vulnerable version if a future
dependency reintroduces it. This is the slice that unblocks PR #75.

- [ ] `devDependencies["@vscode/vsce"]` accepts 4.0.0, and `pnpm-lock.yaml`
      resolves `@vscode/vsce@4.0.0`
- [ ] The `brace-expansion` major-5 guard passes against the resolved version
      by requiring it to be at or above the patched floor 5.0.9, and fails
      for a 5.x version below that floor
- [ ] The `fast-uri` guard passes while `fast-uri` is absent from the
      lockfile, and fails for a resolved `fast-uri` version outside the
      patched ranges. `isPatchedFastUri` and its unit suite still exist and
      still reject every version they reject today
- [ ] Every comment in `overrides.test.ts` describes what the file now
      asserts; no comment claims the major-5 branch is held to an exact
      version or that `fast-uri` is expected in the graph
- [ ] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass
- [ ] `pnpm run audit` reports no finding beyond the allowlisted low
- [ ] `pnpm run build` and `pnpm run test:integration` pass

### Task 2: The redundant `brace-expansion@5` override is removed and the overrides guard reflects the retained pins

Remove `brace-expansion@5: ^5.0.9` from `pnpm.overrides`. Measurement shows
the dependency graph resolves a patched `brace-expansion` on its own, so the
pin no longer changes any resolution — the same reasoning that retired the
`brace-expansion@2` and `fast-uri` overrides. The guard keeps asserting the
resolved version, so a future regression into the advisory range still fails.

- [ ] `pnpm.overrides` in `package.json` declares `serialize-javascript`
      and nothing else
- [ ] `pnpm-lock.yaml`, resolved with no override for `brace-expansion`,
      carries a major-5 `brace-expansion` at or above 5.0.9
- [ ] The guard over the overrides block expects only the retained pins, and
      the guard over removed pins covers `brace-expansion@5` alongside the
      pins already removed
- [ ] The header comment groups `brace-expansion@5` with the packages that
      carry no override, alongside `brace-expansion@2` and `fast-uri`. No
      comment states that the major-5 branch is overridden, and no comment
      justifies an assertion by an override being a visible, intentional
      edit
- [ ] The `serialize-javascript` override and
      `pnpm.auditConfig.ignoreCves` are unchanged
- [ ] `pnpm run audit` reports no finding beyond the allowlisted low
- [ ] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass

### Task 3: `@types/vscode` and `engines.vscode` move to 1.138.0 together

Raise `@types/vscode` to 1.138.0 — the newest release, one minor above what
PR #73 proposes — and raise `engines.vscode` in the same commit so the
lockstep guard holds. This raises the minimum VS Code version required to
install the extension. This is the slice that unblocks PR #73.

- [ ] `engines.vscode` requires 1.138, and
      `devDependencies["@types/vscode"]` is the exact pin `1.138.0` with no
      range prefix
- [ ] `pnpm-lock.yaml` resolves `@types/vscode@1.138.0`
- [ ] Every assertion in `engine-compat.test.ts` passes against the real
      `package.json` and `pnpm-lock.yaml`
- [ ] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass
- [ ] `pnpm run build` and `pnpm run test:integration` pass
- [ ] `pnpm run package` produces a `.vsix` without a
      `@types/vscode` compatibility error from vsce

## Decisions

- **Both upgrades land as our own commits on `main` rather than by merging
  the Dependabot branches** (user choice). Task 3 supersedes PR #73 by going
  to 1.138.0 instead of the proposed 1.137.0, so that branch cannot be
  merged as-is; pushing onto Dependabot branches also risks being overwritten
  when Dependabot rebases. Dependabot closes both pull requests once it sees
  the dependencies updated.

- **`@types/vscode` goes to 1.138.0, not the 1.137.0 that PR #73 proposes**
  (user choice). 1.138.0 is the newest release, so landing it avoids an
  immediate follow-up bump.

- **The `brace-expansion@5` guard asserts a floor rather than pinning the
  override to an exact version** (user choice). The override's `^5.0.9` range
  is retained behaviour; a floor assertion tolerates safe patch drift inside
  that range while still failing on a regression below the patched version.
  The alternative — pinning the override to exactly `5.0.9` to make the
  existing exact match true again — would freeze out future 5.0.x patches.

- **The `fast-uri` guard tolerates absence rather than being deleted** (user
  choice). `fast-uri` left the graph through a transitive change, so a
  transitive change can bring it back. Keeping the guard and its helper costs
  nothing and leaves it armed; deleting it would leave a silent gap.

- **`serialize-javascript: ^7.0.5` stays.** Removing it reintroduces a high
  and a moderate advisory, and no package update clears it: the newest
  `@vscode/test-cli` constrains mocha to a major line whose
  `serialize-javascript` dependency is vulnerable.

- **`pnpm.auditConfig.ignoreCves` stays.** Removing it fails the audit gate
  on a low jsdiff advisory reaching the tree through the same upstream
  constraint.

- **Three slices rather than one commit.** The `@vscode/vsce` upgrade, the
  override removal, and the `@types/vscode` upgrade have independent
  rationales and independent failure modes; separate commits keep each
  reviewable and revertable on its own.

## Non-Goals

- Overriding `mocha` to a major line that carries a patched
  `serialize-javascript`. That would force `@vscode/test-cli` outside its
  declared dependency range.
- Removing `serialize-javascript` from `pnpm.overrides`, or removing
  `pnpm.auditConfig.ignoreCves`. Measurement shows both still hold back
  audit findings.
- Adding an override for the allowlisted jsdiff advisory. The allowlist is
  the existing deliberate treatment for that dev-only low finding.
- Upgrading any other dependency in `package.json`, or any Rust dependency.
- Changing the CI workflows, including the unpinned `pnpx @vscode/vsce`
  invocations.
