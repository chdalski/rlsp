**Repository:** root
**Status:** InProgress
**Created:** 2026-09-23

## Goal

Two Dependabot pull requests against the VS Code extension are stuck: the
`@vscode/vsce` 4.0.0 bump and the `@types/vscode` bump both fail CI's
`VS Code Extension Coverage` job. Neither upgrade is unsafe — each trips a
regression guard in the extension's own test suite that Dependabot cannot
satisfy on its own, because the guard needs a companion change in the same
commit. Land both upgrades with their companion changes so the guards pass on
their real intent. Separately, audit every entry in the extension's pnpm
override and audit-allowlist configuration against the upgraded dependency
graph. That audit found the opposite of what it set out to find: no override
is redundant, one that was retired earlier needs restoring, and the guard
accepts any `brace-expansion` major line it has never vetted. Bound every
`brace-expansion` major line by an override and by a major-aware guard, and
correct the comments that describe the retired overrides as redundant.

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

- **How to tell whether an override is redundant.** An override on
  `pkg@major` is redundant only when every direct dependent's own declared
  range for `pkg` is bounded at or above that major line's patched floor. A
  scratch resolution landing above the floor does not establish this: pnpm
  picks the newest version satisfying a range when nothing else constrains
  it, so the observed version reflects resolver preference, which a
  version-limited registry mirror, an unindexed store, or an incremental
  lockfile update can change. This plan's first draft used the resolution
  test and reached the wrong conclusion; the declared-range test is the
  correct one.

- **`GHSA-rgw5-rvv9-x895` (HIGH) vulnerable bands**, from the GitHub
  Advisory API: `< 1.1.18`, `>= 2.0.0, < 2.1.4`, `>= 3.0.0, < 3.0.6`, and
  `>= 4.0.0, < 5.0.9`. The advisory covers no major line above 5.

- **Both `brace-expansion` overrides are load-bearing.** Applying the
  declared-range test to the two direct dependents in this graph:
  - `minimatch@10.2.6` declares `"brace-expansion": "^5.0.8"`. `5.0.8`
    exists and sits inside the `>= 4.0.0, < 5.0.9` band, so without the
    `brace-expansion@5` override a vulnerable version is a semver-legal
    resolution. This reaches the extension's runtime path through
    `vscode-languageclient`.
  - `minimatch@9.0.9` declares `"brace-expansion": "^2.0.2"`. Versions
    `2.0.2` through `2.1.3` all sit inside the `>= 2.0.0, < 2.1.4` band, so
    the `brace-expansion@2` override — retired earlier on the resolution
    test — needs restoring. This reaches the graph through `mocha`, a
    dev-only path.

- **Measured override evidence (lead, 2026-09-23).** Resolving the manifest
  from scratch in a scratch directory, with the agreed upgrades applied:
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
- [x] Land the `@vscode/vsce` upgrade and repair the two stale guards
- [x] Establish which overrides are genuinely redundant, by declared range
- [ ] Bound every `brace-expansion` major line by an override and a
      major-aware guard
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

- [x] `devDependencies["@vscode/vsce"]` accepts 4.0.0, and `pnpm-lock.yaml`
      resolves `@vscode/vsce@4.0.0`
- [x] The `brace-expansion` major-5 guard passes against the resolved version
      by requiring it to be at or above the patched floor 5.0.9, and fails
      for a 5.x version below that floor
- [x] The `fast-uri` guard passes while `fast-uri` is absent from the
      lockfile, and fails for a resolved `fast-uri` version outside the
      patched ranges. `isPatchedFastUri` and its unit suite still exist and
      still reject every version they reject today
- [x] Every comment in `overrides.test.ts` describes what the file now
      asserts; no comment claims the major-5 branch is held to an exact
      version or that `fast-uri` is expected in the graph
- [x] `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass
- [x] `pnpm run audit` reports no finding beyond the allowlisted low
- [x] `pnpm run build` and `pnpm run test:integration` pass

### Task 2: Every `brace-expansion` major line is bounded by an override and by a major-aware guard

Restore the `brace-expansion@2` override, keep the `brace-expansion@5`
override, and replace the major-agnostic floor check with a predicate that
dispatches on major version the way `isPatchedFastUri` already does. Both
overrides are load-bearing: their direct dependents' own declared ranges
admit versions inside GHSA-rgw5-rvv9-x895's vulnerable bands, so the
overrides are what keep the advisory out of the graph by construction rather
than by resolver preference. Correct the comments that state the opposite.

- [ ] `pnpm.overrides` in `package.json` bounds both `brace-expansion` major
      lines present in the graph — the major-2 line at or above 2.1.4 and
      the major-5 line at or above 5.0.9 — and retains `serialize-javascript`
- [ ] `pnpm-lock.yaml` resolves every `brace-expansion` version at or above
      its own major line's patched floor, and its `overrides:` block
      declares the same pins as `package.json`
- [ ] A single predicate decides whether a `brace-expansion` version is
      patched, dispatching on major line against that line's floor from
      GHSA-rgw5-rvv9-x895 (1.1.18, 2.1.4, 3.0.6, 5.0.9), and returns false
      for any major line the advisory data does not cover
- [ ] The predicate rejects every version on the major-4 line, which the
      advisory covers as vulnerable with no patched release of its own, and
      rejects `6.0.0` and every other version on a major line the advisory
      does not cover at all. Both cases are proven by literals that do not
      depend on what the lockfile currently resolves
- [ ] No lockfile-driven assertion accepts a `brace-expansion` version by a
      major-agnostic comparison
- [ ] A `brace-expansion` major line that carries an override fails the
      guard if it disappears from the lockfile entirely. This holds for both
      the major-2 and the major-5 line, so an override going dead is caught
      rather than passing vacuously on an empty set
- [ ] The file states the test for whether an override is redundant — every
      direct dependent's own declared range bounded at or above that major
      line's patched floor — so a future removal candidate is evaluated
      against declared ranges rather than against a resolution snapshot
- [ ] The guard over the overrides block expects every retained pin,
      including both `brace-expansion` pins, and the guard over removed pins
      covers only `fast-uri`
- [ ] Every comment in `overrides.test.ts` describes the real reason each
      override is retained or was removed. No comment claims a
      `brace-expansion` override is redundant, and no comment justifies a
      retirement by a scratch resolution landing above a floor
- [ ] `pnpm.auditConfig.ignoreCves` is unchanged
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
- [ ] `pnpm run audit` reports no finding beyond the allowlisted low
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

- **No `brace-expansion` override is removed; the `brace-expansion@2`
  override is restored** (user choice, after a security review found the
  original premise wrong). Both direct dependents' declared ranges admit
  versions inside the advisory's vulnerable bands, so both overrides keep
  the advisory out of the graph by construction. The guard detects a
  vulnerable resolution; the override prevents one. Keeping both is defense
  in depth, and nothing shipped was ever exposed.

- **The guard dispatches on major line rather than comparing against a
  single floor** (user choice). A major-agnostic comparison accepts any
  version on a higher major line — `isAtLeast('6.0.0', '5.0.9')` is true —
  so a future `brace-expansion@6` would pass unvetted. `isPatchedFastUri`
  already dispatches on major for this reason; the `brace-expansion` branch
  now does the same and fails closed on lines the advisory does not cover.

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
  `brace-expansion` override hardening, and the `@types/vscode` upgrade have
  independent rationales and independent failure modes; separate commits
  keep each reviewable and revertable on its own.

## Non-Goals

- Overriding `mocha` to a major line that carries a patched
  `serialize-javascript`. That would force `@vscode/test-cli` outside its
  declared dependency range.
- Removing `serialize-javascript` from `pnpm.overrides`, or removing
  `pnpm.auditConfig.ignoreCves`. Measurement shows both still hold back
  audit findings.
- Re-auditing the rest of the dependency graph against the declared-range
  test. This plan applies it to `brace-expansion`, the package whose
  override the audit set out to remove. A sweep of every transitive
  dependency for the same class of exposure is separate work.
- Restoring a `fast-uri` override. `fast-uri` is absent from the graph
  entirely, so no dependent declares a range for it and an override would
  bind nothing.
- Consolidating `overrides.test.ts`. The file has grown across many plans
  and a pass over its aggregate structure is worth doing, but it is a
  refactor with no security content and does not belong in a commit that
  restores an override. It gets its own plan after this one.
- Adding an override for the allowlisted jsdiff advisory. The allowlist is
  the existing deliberate treatment for that dev-only low finding.
- Upgrading any other dependency in `package.json`, or any Rust dependency.
- Changing the CI workflows, including the unpinned `pnpx @vscode/vsce`
  invocations.
