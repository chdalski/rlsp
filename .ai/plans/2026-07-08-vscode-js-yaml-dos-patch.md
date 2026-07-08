**Repository:** root
**Status:** NotStarted
**Created:** 2026-07-08

# VS Code Extension — Clear js-yaml DoS Dependabot Alert

## Goal

Clear the single open Dependabot security alert (#36) on the repository:
`js-yaml@4.1.1` (CVE-2026-53550 / GHSA-h67p-54hq-rp68, medium), a
quadratic-complexity DoS in merge-key (`<<`) handling. The package is a
transitive **devDependency** of the VS Code extension in
`rlsp-yaml/integrations/vscode`. Fix by pinning `js-yaml` to its patched
line (`^4.2.0`) via the existing `pnpm.overrides` block in `package.json`,
regenerate `pnpm-lock.yaml`, and confirm the vulnerable version is gone.

Success is measured concretely: after the fix, the locked version of
`js-yaml` is ≥ 4.2.0 everywhere it appears in `pnpm-lock.yaml`, `pnpm audit`
reports no advisory for `js-yaml`, and the extension still builds, lints,
formats, and its unit tests pass.

## Context

### Full security-surface investigation (scope basis)

The user's request — "check the open security issues on GitHub if they can
be fixed" — spans all of GitHub's security surfaces, so every surface was
checked before scoping this plan:

| Surface | Result |
|---------|--------|
| Dependabot alerts | **1 open** (#36, this plan); 27 fixed, 2 auto-dismissed |
| Security-labeled issues | 0 open |
| Code-scanning alerts | 0 open (none configured/reported) |
| Secret-scanning alerts | 0 open (feature enabled; API returned empty) |

Alert #36 is therefore the **complete set** of currently fixable security
work — this plan's scope is not a narrowing of the request; it is the whole
of it.

### The alert

One open alert (GitHub Dependabot), npm ecosystem, in
`rlsp-yaml/integrations/vscode/pnpm-lock.yaml`:

| # | Package | Severity | CVE / GHSA | Vulnerable range | Patched |
|---|---------|----------|------------|------------------|---------|
| 36 | js-yaml | medium | CVE-2026-53550 / GHSA-h67p-54hq-rp68 | >= 4.0.0, <= 4.1.1 | 4.2.0 |

The vulnerability: when a YAML merge value is a sequence
(`<<: [*a, *a, ...]`), `js-yaml` re-processes every aliased source without
deduplication, giving O(K·M) work for O(K+M) input — a small crafted
document (tens of KB) can pin a Node worker for seconds (DoS).

### Dependency provenance (verified via `pnpm why`)

`js-yaml@4.1.1` is pulled in through two dev-only chains:

- `@vscode/vsce@3.9.2` (devDep) → `@secretlint/*` → `js-yaml`
- `@vscode/vsce@3.9.2` (devDep) → `secretlint` → `js-yaml`
- `@vscode/test-cli@0.0.12` (devDep) → `mocha@11.7.5` → `js-yaml`
- `@secretlint/config-loader` → `rc-config-loader@4.1.4` → `js-yaml`

All chains are **transitive dev-only** dependencies (packaging + test
tooling). `js-yaml` is not in the extension's runtime dependency tree —
the sole runtime dependency is `vscode-languageclient@^9.0.1`. Real-world
exposure is therefore limited to the local/CI packaging and test
toolchain, not shipped extension code. The fix is still worthwhile to
clear the alert dashboard and keep the build toolchain current.

### Established pattern

`package.json` already carries a `pnpm.overrides` block used for exactly
this purpose (transitive-security pins):

```json
"pnpm": {
  "overrides": {
    "lodash": ">=4.18.0",
    "fast-uri": "^3.1.2",
    "serialize-javascript": "^7.0.5",
    "qs": "^6.15.2",
    "undici": "^7.28.0",
    "form-data": "^4.0.6",
    "markdown-it": "^14.2.0"
  }
}
```

The completed plan `2026-07-02-vscode-dependabot-transitive-patches.md`
(and the archived `2026-05-29-vscode-tmp-qs-uuid-security-patches.md`
before it) established this override-based remediation pattern. This plan
follows it.

### Version choice (verified available on npm)

- `js-yaml`: pin `^4.2.0`. Published 4.x versions are 4.0.0, 4.1.0, 4.1.1,
  4.2.0, 4.3.0; caret resolves to the latest 4.x (4.3.0), which is ≥ the
  patched 4.2.0. Do **not** jump to 5.x — all three consumers declare
  `^4.1.0` / `^4.1.1`, so a 5.x major would fall outside their ranges and
  risk resolution or behavior changes in the packaging/test tools. `^4.2.0`
  stays within every consumer's declared range while clearing the alert.

## Decisions

- **Fix via `pnpm.overrides`, not by removing/replacing the consuming
  devDependencies.** `@vscode/vsce`, `@vscode/test-cli`, and `mocha` are
  standard packaging/test tools; the vulnerable code is in their
  transitive `js-yaml`, which an override pins directly. This matches the
  existing convention in the file.
- **Stay within `js-yaml` 4.x** — caret `^4.2.0`, not `5.x`. Consumers
  declare `^4.1.0` / `^4.1.1`; a major bump is out of range.
- **No direct-dependency version bumps this time.** Unlike the 2026-07-02
  plan, no direct devDependency (e.g. `@vscode/vsce`) needs bumping —
  `@vscode/vsce` is already at `^3.9.2`, and the override alone remediates
  the alert. (YAGNI: do not bump unrelated deps.)
- **No `version = "..."` edits to any Cargo.toml** — not applicable here
  (npm-only change), noted for completeness.
- **This is not a feature change** — no `docs/feature-log.md` entry (the
  feature-log is user-facing feature decisions only; dependency security
  patches are not features). The plan file plus commit history carry the
  record.

## Non-Goals

- Rust/Cargo dependency changes — the only open alert is npm-ecosystem.
- GitHub Actions dependency changes — none open in that ecosystem.
- Refactoring the VS Code extension, its build, or its runtime deps.
- Touching `vscode-languageclient` or any runtime dependency.
- Bumping `@vscode/vsce`, `@vscode/test-cli`, `mocha`, or any other direct
  devDependency — the override is the authoritative pin.
- Any change to release/version fields owned by release-plz.
- Changes to the `rlsp-yaml-parser` Rust YAML parser — the alert is about
  the npm `js-yaml` library in the extension's dev toolchain, unrelated to
  this project's own parser.

## Acceptance Criteria (plan-level)

1. `js-yaml` resolves to a patched version (≥ 4.2.0) at **every**
   occurrence in the regenerated `pnpm-lock.yaml` (there are currently
   three `js-yaml@4.1.1` resolution sites).
2. `pnpm audit` (or `pnpm audit --audit-level low`) reports **no** advisory
   for `js-yaml`.
3. `js-yaml` appears as an entry in the `pnpm.overrides` block of
   `package.json` set to `^4.2.0`.
4. Extension still passes its existing quality gates:
   - `pnpm run build` succeeds
   - `pnpm run lint` passes
   - `pnpm run format` (prettier check) passes
   - `pnpm run test` (vitest unit tests) passes
5. No unrelated dependency downgrades or churn introduced in the lockfile
   beyond what the `js-yaml` override requires (spot-check the diff).

## Steps

- [x] Pin patched `js-yaml` via `pnpm.overrides`, regenerate the lockfile,
      and verify audit + quality gates (Task 1).

## Tasks

### Task 1 — Pin patched js-yaml, regenerate lockfile

Single vertical slice — the change is one coherent unit (manifest edit +
lockfile regeneration + verification); splitting it would produce a
non-buildable intermediate state.

- [x] In `rlsp-yaml/integrations/vscode/package.json`, add one entry to the
      existing `pnpm.overrides` block: `"js-yaml": "^4.2.0"`.
- [x] Run `pnpm install` in `rlsp-yaml/integrations/vscode` to regenerate
      `pnpm-lock.yaml` with the pinned version.
- [x] Verify locked versions: confirm `pnpm-lock.yaml` resolves `js-yaml`
      to ≥ 4.2.0 at every occurrence (no remaining `js-yaml@4.1.1`).
- [x] Run `pnpm audit --audit-level low` and confirm no advisory remains
      for `js-yaml`. Note any residual advisories for other packages and
      confirm they are pre-existing / out of scope (not introduced by this
      change).
- [x] Run the extension quality gates and confirm all pass:
      `pnpm run build`, `pnpm run lint`, `pnpm run format`,
      `pnpm run test`.
- [x] Spot-check the `pnpm-lock.yaml` diff for unexpected churn beyond the
      `js-yaml` override.

**Acceptance:** all plan-level acceptance criteria (1–5) met and reported
in the handoff, including the actual `pnpm audit` result for `js-yaml` and
the resolved version(s) of `js-yaml` in the lockfile.

**Result (2026-07-08):** js-yaml resolves to `4.3.0` at all 5 lockfile
occurrences (zero remaining `4.1.1`); `pnpm audit` reports no js-yaml
finding (advisory cleared); override present at `package.json`
`pnpm.overrides`; all four quality gates pass (build clean, lint 0
warnings, prettier clean, vitest 38/38); no unrelated lockfile churn (the
two residual audit findings — `brace-expansion@5.0.5`, `diff@7.0.0` — are
verified pre-existing, identical at baseline and HEAD, and are separate
out-of-scope alerts).

**Files:**
- `rlsp-yaml/integrations/vscode/package.json`
- `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`

**Advisors:** security-engineer — the task exists specifically to remediate
a security advisory; consult for a risk assessment on the version-pin
choice (input gate) and sign-off that `^4.2.0` fully covers the advisory
range (`<= 4.1.1`) without under-pinning, and that staying on 4.x rather
than 5.x is the correct call (output gate). Test-engineer not required: no
production code or test changes; verification is via the existing extension
quality gates plus `pnpm audit`, which are objective commands, not new test
design.

## Verification

After Task 1 is committed and the patched lockfile lands on the default
branch, re-check the Dependabot alert page — GitHub should auto-resolve
alert #36. (Auto-resolution is asynchronous and outside this plan's direct
control; the plan-level acceptance is the local `pnpm audit` clean result
for `js-yaml` and the resolved lockfile version.)
