**Repository:** root
**Status:** NotStarted
**Created:** 2026-07-02

# VS Code Extension — Clear Dependabot Transitive Alerts

## Goal

Clear all 8 open Dependabot security alerts on the repository, every one
of which traces to a transitive dependency of `@vscode/vsce` (the VS Code
extension packaging CLI, a devDependency) in
`rlsp-yaml/integrations/vscode`. Fix by pinning the three vulnerable
transitive packages to their patched versions via the existing
`pnpm.overrides` block in `package.json`, and bump the direct
`@vscode/vsce` devDependency 3.9.1 → 3.9.2. Regenerate `pnpm-lock.yaml`
and confirm the vulnerable versions are gone.

Success is measured concretely: after the fix, `pnpm audit` reports no
advisories for the three packages, the locked versions of `undici`,
`form-data`, and `markdown-it` are all at or above their patched
versions, and the extension still builds, lints, formats, and its unit
tests pass.

## Context

### The alerts

All 8 open alerts (GitHub Dependabot) are npm, all in
`rlsp-yaml/integrations/vscode/pnpm-lock.yaml`:

| # | Package | Severity | Vulnerable range | Patched |
|---|---------|----------|------------------|---------|
| 26 | undici | high | >=7.23.0, <7.28.0 | 7.28.0 |
| 31 | undici | high | >=7.23.0, <7.28.0 | 7.28.0 |
| 27 | undici | medium | >=7.0.0, <7.28.0 | 7.28.0 |
| 33 | undici | medium | >=7.0.0, <7.28.0 | 7.28.0 |
| 30 | undici | low | >=7.0.0, <7.28.0 | 7.28.0 |
| 34 | undici | low | >=7.0.0, <7.28.0 | 7.28.0 |
| 28 | form-data | high | >=4.0.0, <4.0.6 | 4.0.6 |
| 25 | markdown-it | medium | <=14.1.1 | 14.2.0 |

### Dependency provenance (verified via `pnpm why`)

- `undici@7.24.7` ← `cheerio@1.2.0` ← `@vscode/vsce@3.9.1` (devDep)
- `form-data@4.0.5` ← `@vscode/vsce@3.9.1` (devDep)
- `markdown-it@14.1.1` ← `@vscode/vsce@3.9.1` (devDep)

All three are **transitive dev-only** dependencies. None are in the
extension's runtime dependency tree — the sole runtime dependency is
`vscode-languageclient@^9.0.1`. Real-world exposure is therefore limited
to the local/CI packaging toolchain, not shipped extension code. The fix
is still worthwhile to clear the alert dashboard and keep the build
toolchain current.

### Established pattern

`package.json` already carries a `pnpm.overrides` block used for exactly
this purpose:

```json
"pnpm": {
  "overrides": {
    "lodash": ">=4.18.0",
    "fast-uri": "^3.1.2",
    "serialize-javascript": "^7.0.5",
    "qs": "^6.15.2"
  }
}
```

The archived plan `2026-05-29-vscode-tmp-qs-uuid-security-patches.md`
established this override-based remediation pattern. This plan follows it.

### Version choices (verified available on npm)

- `undici`: pin `^7.28.0`. Latest 7.x resolves ≥7.28.0. Do **not** jump to
  8.x — `cheerio` expects the 7.x line; a major bump risks resolution or
  behavior changes in the packaging tool.
- `form-data`: pin `^4.0.6` (latest is 4.0.6).
- `markdown-it`: pin `^14.2.0` (latest 14.x is 14.3.0; caret allows it).
- `@vscode/vsce`: bump devDependency `^3.9.1` → `^3.9.2` (latest 3.9.x).

## Decisions

- **Fix via `pnpm.overrides`, not by removing/replacing `@vscode/vsce`.**
  vsce is the standard VS Code packaging tool; the vulnerable code is in
  its transitive deps, which overrides pin directly. This matches the
  existing convention in the file.
- **Bump `@vscode/vsce` to 3.9.2 in the same change** (per user
  direction). It is a patch bump that may independently refresh some
  transitives; combined with the overrides it keeps the direct dep
  current. Overrides remain the authoritative pin regardless of what vsce
  pulls.
- **Stay within `undici` 7.x** — caret `^7.28.0`, not `8.x`.
- **No `version = "..."` edits to any Cargo.toml** — not applicable here
  (npm-only change), noted for completeness.
- **This is not a feature change** — no `docs/feature-log.md` entry (the
  feature-log is user-facing feature decisions only; dependency security
  patches are not features). The plan file plus commit history carry the
  record.

## Non-Goals

- Rust/Cargo dependency changes — no open alerts are Cargo-ecosystem.
- GitHub Actions dependency changes — none open in that ecosystem.
- Refactoring the VS Code extension, its build, or its runtime deps.
- Touching `vscode-languageclient` or any runtime dependency.
- Any change to release/version fields owned by release-plz.

## Acceptance Criteria (plan-level)

1. All three packages resolve to patched versions in the regenerated
   `pnpm-lock.yaml`:
   - `undici` ≥ 7.28.0
   - `form-data` ≥ 4.0.6
   - `markdown-it` ≥ 14.2.0
2. `pnpm audit` (or `pnpm audit --audit-level low`) reports **no**
   advisories for undici, form-data, or markdown-it.
3. `@vscode/vsce` devDependency is `^3.9.2` in `package.json`.
4. Extension still passes its existing quality gates:
   - `pnpm run build` succeeds
   - `pnpm run lint` passes
   - `pnpm run format` (prettier check) passes
   - `pnpm run test` (vitest unit tests) passes
5. No unrelated dependency downgrades or churn introduced in the lockfile
   beyond what the overrides + vsce bump require (spot-check the diff).

## Steps

- [x] Pin patched transitives (undici, form-data, markdown-it) + bump
      `@vscode/vsce`, regenerate the lockfile, and verify audit + quality
      gates (Task 1).

## Tasks

### Task 1 — Pin patched transitives + bump vsce, regenerate lockfile

Single vertical slice — the change is one coherent unit (manifest edit +
lockfile regeneration + verification); splitting it would produce a
non-buildable intermediate state.

- [x] In `rlsp-yaml/integrations/vscode/package.json`, add three entries
      to the existing `pnpm.overrides` block:
      - `"undici": "^7.28.0"`
      - `"form-data": "^4.0.6"`
      - `"markdown-it": "^14.2.0"`
- [x] In the same file, bump the `@vscode/vsce` devDependency from
      `^3.9.1` to `^3.9.2`.
- [x] Run `pnpm install` in `rlsp-yaml/integrations/vscode` to regenerate
      `pnpm-lock.yaml` with the pinned versions.
- [x] Verify locked versions: confirm `pnpm-lock.yaml` resolves
      `undici` ≥ 7.28.0, `form-data` ≥ 4.0.6, `markdown-it` ≥ 14.2.0
      (resolved: undici 7.28.0, form-data 4.0.6, markdown-it 14.3.0).
- [x] Run `pnpm audit --audit-level low` and confirm no advisories remain
      for the three packages (clean for undici/form-data/markdown-it; 3
      residual moderate/low items are `@vscode/test-cli` test-infra only,
      not among the 8 in-scope alerts).
- [x] Run the extension quality gates and confirm all pass:
      `pnpm run build`, `pnpm run lint`, `pnpm run format`,
      `pnpm run test` (build PASS, eslint 0 warnings, prettier PASS,
      vitest 38/38).
- [x] Spot-check the `pnpm-lock.yaml` diff for unexpected churn beyond the
      three overrides + vsce bump (36 insertions / 72 deletions, no
      unrelated churn).

**Acceptance:** all plan-level acceptance criteria (1–5) met and reported
in the handoff, including the actual `pnpm audit` output and the resolved
versions of the three packages.

**Files:**
- `rlsp-yaml/integrations/vscode/package.json`
- `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`

**Advisors:** security-engineer — the task exists specifically to
remediate security advisories; consult for a risk assessment on the
version-pin choices (input gate) and sign-off that the pins fully cover
the advisory ranges without under-pinning (output gate). Test-engineer
not required: no production code or test changes; verification is via the
existing extension quality gates plus `pnpm audit`, which are objective
commands, not new test design.

## Verification

After Task 1 is committed, re-check the Dependabot alert page — GitHub
should auto-resolve alerts 25–34 once the patched lockfile lands on the
default branch. (Auto-resolution is asynchronous and outside this plan's
direct control; the plan-level acceptance is the local `pnpm audit` clean
result and the resolved lockfile versions.)
