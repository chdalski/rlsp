**Repository:** root
**Status:** Completed (2026-07-08)
**Created:** 2026-07-08

# VS Code Extension — Patch brace-expansion 5.x ReDoS (scoped override)

## Goal

Remediate the `brace-expansion` ReDoS advisory (GHSA-jxxr-4gwj-5jf2,
moderate) affecting the `brace-expansion@5.0.5` transitive **devDependency**
of the VS Code extension in `rlsp-yaml/integrations/vscode`. Fix by pinning
**only the 5.x line** to its patched version (`>= 5.0.6`) via a
**version-scoped** `pnpm.overrides` entry, so the non-vulnerable
`brace-expansion@2.1.0` instances — which sit on the extension's runtime
`vscode-languageclient` path — are left untouched. Regenerate
`pnpm-lock.yaml` and confirm the 5.x line is patched while the 2.x line is
unchanged.

Success is measured concretely: after the fix, every 5.x `brace-expansion`
occurrence in `pnpm-lock.yaml` is ≥ 5.0.6, every `brace-expansion@2.1.0`
occurrence is still 2.1.0, `pnpm audit` reports no advisory for
`brace-expansion`, and the extension still builds, lints, formats, and its
unit tests pass.

## Context

### User direction

This work is a follow-up to the js-yaml DoS patch
(`2026-07-08-vscode-js-yaml-dos-patch.md`, Completed). During that review,
`pnpm audit` surfaced two residual dev-only findings. The user was
presented all three options (brace-expansion only / both / neither) and
**explicitly chose brace-expansion only**. The second finding
(`diff@7.0.0`, GHSA-73rr-hh4g-fpgx, low) is deliberately **excluded** —
its only patched line is a major bump (`>= 8.0.3`) outside mocha's declared
`^7.0.0` range, a breakage risk with poor risk/reward for a low-severity,
already-auto-dismissed, dev-only finding.

### The advisory

| Field | Value |
|-------|-------|
| Package | `brace-expansion@5.0.5` (npm) |
| Advisory | GHSA-jxxr-4gwj-5jf2 (moderate, CWE-400) |
| Summary | Large numeric range defeats documented `max` DoS protection (ReDoS) |
| Vulnerable range | `>= 5.0.0 < 5.0.6` |
| Patched | `>= 5.0.6` (published 5.x: 5.0.2–5.0.7) |
| GitHub Dependabot | alert **#17, `auto_dismissed`** (dev-only) — not an open alert |

Because the alert is already auto-dismissed on GitHub, this is **optional
hygiene**, undertaken at user direction — not clearing an open alert.

### Dependency provenance (verified via `pnpm why`)

Two distinct major lines of `brace-expansion` coexist in the tree:

- **Vulnerable (5.x):** `brace-expansion@5.0.5` ← `minimatch@10.2.5` ←
  `@eslint/config-array` / `@typescript-eslint/*` ← `eslint@10.2.1` /
  `typescript-eslint@8.59.0` (all **devDependencies**). `minimatch@10.2.5`
  declares `brace-expansion: ^5.0.5`, so `5.0.6`/`5.0.7` is in range.
- **Not vulnerable (2.x):** `brace-expansion@2.1.0` ← `minimatch@5.1.9` ←
  **`vscode-languageclient@9.0.1` (runtime dependency)**, and also ←
  `minimatch@9.0.9` ← `@vscode/test-cli` / `mocha`. The 2.x line is
  **outside** the vulnerable range and must not be disturbed.

The vulnerable code path is therefore entirely within the dev
lint-toolchain; the runtime path uses the non-vulnerable 2.x line.

### Why the override must be version-scoped

A blanket `"brace-expansion": ">= 5.0.6"` override would force the 2.1.0
instances up to the 5.x major — a major bump for `minimatch@5.1.9`
(runtime, via `vscode-languageclient`) and `minimatch@9.0.9`. That is the
"breaking something" risk. The fix must target **only the 5.x line** so it
patches the vulnerable instances and leaves 2.x exactly as-is.

pnpm supports version-scoped override keys (`"<pkg>@<range>": "<target>"`).
The recommended form is `"brace-expansion@5": "^5.0.6"` — it matches only
resolutions already in the 5.x major and rewrites them to the patched line,
leaving 2.x untouched. The developer must **empirically confirm** after
`pnpm install` that (a) the 5.x line moved to ≥ 5.0.6 and (b) the 2.1.0
instances are unchanged; if the `@5` selector does not scope as intended,
fall back to a parent-scoped key (e.g. `"minimatch@10>brace-expansion"`).
The mechanism is an implementation detail — the invariant is: **5.x
patched, 2.x untouched.**

### Established pattern

`package.json` already carries a `pnpm.overrides` block used for transitive
security pins (currently: lodash, fast-uri, serialize-javascript, qs,
undici, form-data, markdown-it, js-yaml). This plan adds one more entry,
following that convention — with the version-scoping refinement above.

## Decisions

- **Version-scoped override, not a blanket one.** Target only the 5.x line
  (`brace-expansion@5` → `^5.0.6`). A blanket override would major-bump the
  runtime 2.x line — the specific breakage this plan exists to avoid.
- **Stay within 5.x** (`^5.0.6`, resolves to 5.0.7). Do not touch the 2.x
  instances or `vscode-languageclient`.
- **Exclude the `diff@7.0.0` finding** (user direction) — major-bump-only
  patch, breakage risk, low severity, already auto-dismissed.
- **No direct-dependency bumps.** The fix is the override alone.
- **No `version = "..."` edits to any Cargo.toml** — npm-only change.
- **Not a feature change** — no `docs/feature-log.md` entry. Plan file +
  commit history carry the record.

## Non-Goals

- The `diff@7.0.0` advisory (GHSA-73rr-hh4g-fpgx) — explicitly excluded.
- Any change to the `brace-expansion@2.1.0` / 2.x line, `minimatch@5.1.9`,
  `minimatch@9.0.9`, or `vscode-languageclient`.
- Runtime dependency changes of any kind.
- Rust/Cargo or GitHub Actions dependency changes.
- Bumping eslint, typescript-eslint, or any direct devDependency.
- release-plz-owned version fields.

## Acceptance Criteria (plan-level)

1. Every 5.x `brace-expansion` occurrence in the regenerated
   `pnpm-lock.yaml` resolves to ≥ 5.0.6 (no remaining `5.0.5`).
2. Every `brace-expansion@2.1.0` occurrence in `pnpm-lock.yaml` is
   **unchanged** (still 2.1.0) — the runtime path is untouched.
3. `pnpm audit` (or `pnpm audit --audit-level low`) reports **no** advisory
   for `brace-expansion`.
4. A version-scoped `brace-expansion` entry (targeting the 5.x line only)
   is present in `pnpm.overrides` in `package.json`.
5. Extension still passes its quality gates: `pnpm run build`,
   `pnpm run lint`, `pnpm run format`, `pnpm run test`.
6. No unrelated dependency downgrades or churn introduced in the lockfile
   beyond the scoped `brace-expansion` bump (spot-check the diff).

## Steps

- [x] Add a version-scoped `brace-expansion` 5.x override, regenerate the
      lockfile, and verify the 5.x line is patched while 2.x is untouched,
      plus audit + quality gates (Task 1).

## Tasks

### Task 1 — Add scoped brace-expansion 5.x override, regenerate lockfile

Single vertical slice — one coherent unit (manifest edit + lockfile
regeneration + verification); splitting would produce a non-buildable
intermediate state.

- [x] In `rlsp-yaml/integrations/vscode/package.json`, add a version-scoped
      entry to the existing `pnpm.overrides` block targeting the 5.x line
      only — recommended `"brace-expansion@5": "^5.0.6"`.
- [x] Run `pnpm install` in `rlsp-yaml/integrations/vscode` to regenerate
      `pnpm-lock.yaml`.
- [x] Verify the 5.x line: confirm no remaining `brace-expansion@5.0.5`;
      the 5.x occurrence(s) resolve to ≥ 5.0.6.
- [x] Verify the 2.x line is untouched: confirm `brace-expansion@2.1.0`
      still appears unchanged (runtime `vscode-languageclient` path). If
      the override disturbed 2.x, switch to a narrower selector and
      re-verify.
- [x] Run `pnpm audit --audit-level low`; confirm no advisory remains for
      `brace-expansion`. Note residual advisories for other packages
      (expected: `diff@7.0.0`, out of scope by decision) and confirm they
      are pre-existing, not introduced here.
- [x] Run the extension quality gates and confirm all pass:
      `pnpm run build`, `pnpm run lint`, `pnpm run format`,
      `pnpm run test`.
- [x] Spot-check the `pnpm-lock.yaml` diff for unexpected churn beyond the
      scoped `brace-expansion` bump.

**Acceptance:** all plan-level acceptance criteria (1–6) met and reported
in the handoff, including the actual `pnpm audit` result for
`brace-expansion`, the resolved 5.x version, and confirmation the 2.x line
is unchanged.

**Result (2026-07-08):** scoped override `"brace-expansion@5": "^5.0.6"`
added; 5.x line resolves to `5.0.7` at both lockfile sites (zero remaining
`5.0.5`); both `brace-expansion@2.1.0` sites unchanged (integrity hash
byte-identical to baseline, absent from the diff) — runtime
`vscode-languageclient` path untouched; `pnpm audit` reports zero
brace-expansion findings; all four quality gates pass (build clean, lint 0
warnings, prettier clean, vitest 38/38); no unrelated churn. The residual
`diff@7.0.0` low finding is unchanged and out of scope by user decision.

**Files:**
- `rlsp-yaml/integrations/vscode/package.json`
- `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`

**Advisors:** security-engineer — this is a security remediation; consult
for a risk assessment on the scoped-override approach (input gate:
does `^5.0.6` cover the vulnerable range `< 5.0.6`, and is the scoping
correct so the vulnerable 5.x line is fully patched?) and sign-off (output
gate: the 5.x line is remediated and the untouched 2.x line carries no
residual exposure). Test-engineer not required: no production or test code
changes; verification is via the existing quality gates plus `pnpm audit`
and lockfile inspection — objective commands, not new test design.

## Verification

After Task 1 is committed and lands on the default branch, the
`brace-expansion` advisory should not reappear in `pnpm audit`. GitHub
alert #17 is already `auto_dismissed`, so no dashboard change is expected;
the plan-level acceptance is the local `pnpm audit` clean result for
`brace-expansion`, the patched 5.x lockfile version, and the unchanged 2.x
line.
