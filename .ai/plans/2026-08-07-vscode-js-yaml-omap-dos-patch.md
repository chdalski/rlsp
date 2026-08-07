**Repository:** root
**Status:** InProgress
**Created:** 2026-08-07

# VS Code Extension: Patch js-yaml !!omap DoS Advisory (#50)

## Goal

GitHub Dependabot alert #50 (high severity) flags a
quadratic-CPU denial-of-service in `js-yaml`'s `!!omap`
resolution (CVE-2026-59870), affecting `>= 4.0.0, < 4.3.1`
and patched in 4.3.1. The VS Code extension's existing
`js-yaml: ^4.2.0` override resolves `js-yaml@4.3.0`, which
is still inside the vulnerable range. Resolve the alert by
advancing that `pnpm.overrides` pin to `^4.3.1` and
regenerating the lockfile so no `js-yaml` version below
4.3.1 resolves. Build, lint, format, and tests must stay
green. The authoritative closure signal is GitHub alert #50
leaving the `open` state after the change lands on the
scanned branch; the deterministic, commit-time proof is
that the regenerated `pnpm-lock.yaml` resolves `js-yaml`
≥ 4.3.1 with no version left in the vulnerable range.

## Context

Alert #50 surfaced during GitHub's re-scan after the
2026-08-07 ten-alert advisory-patch work landed on
`origin/main` (fix commit `bb11c34b`; that commit was
pushed as part of HEAD `1adba696`, which is why the sibling
plan's completion note records the push at `1adba696`). It
was not among those ten — it is a distinct, newly-raised
advisory against the `js-yaml@4.3.0` already present in the
lockfile.

- **Advisory:** `js-yaml` GHSA / CVE-2026-59870, high,
  vulnerable range `>= 4.0.0, < 4.3.1`, first patched
  4.3.1. Manifest: `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`.
- **Current state:** `pnpm.overrides` pins `js-yaml: ^4.2.0`
  in `rlsp-yaml/integrations/vscode/package.json`; the
  lockfile resolves `js-yaml@4.3.0` (single resolved
  version). Bumping the pin to `^4.3.1` moves it to the
  patched line.
- **Reach:** `js-yaml` is dev/build-tooling only — it is
  absent from the production tree (`pnpm why js-yaml --prod`
  returns nothing) and reaches the project via
  `@vscode/vsce` (publish), `@vscode/test-cli`/`mocha`
  (test), and `secretlint`. It is not shipped to the
  extension runtime. It is patched anyway because Dependabot
  flags it and the fix is a one-line pin bump.
- **No regression guard for js-yaml:** unlike the prior
  advisory patch (where `src/overrides.test.ts` pinned
  brace-expansion and fast-uri literals), no test or source
  file references a `js-yaml` version — verified during
  planning (`grep` of `src/` is empty; `overrides.test.ts`
  guards only brace-expansion and fast-uri). No
  regression-guard test update is expected.
- **Established mechanism:** the `pnpm.overrides` block is
  this project's transitive-security-pin mechanism; this is
  the same single-pin-bump shape as commit `bb11c34b`.
- **Version fields off-limits:** per the root CLAUDE.md,
  agents must not edit `version = "..."` fields in any
  `Cargo.toml`, and must not touch the extension's own
  `version` field. This task edits only the `pnpm.overrides`
  block and the regenerated lockfile.
- **Tooling:** pnpm 10.33.2 (matches declared
  `packageManager`), Node v24.14.1. Build/test commands are
  in the root `/workspace/CLAUDE.md` under "VS Code
  Extension".

## Steps

- [x] Confirm alert #50, its vulnerable range, and the
      patched version
- [x] Confirm the current `js-yaml` override and resolved
      version, and that no regression guard pins js-yaml
- [x] Advance the `js-yaml` override to `^4.3.1` in
      `package.json`
- [x] Regenerate `pnpm-lock.yaml` via `pnpm install`
- [x] Verify build, lint, format, and unit tests pass
- [x] Verify the regenerated lockfile resolves `js-yaml`
      ≥ 4.3.1 with no version in `>= 4.0.0, < 4.3.1`; local
      `pnpm audit` reports the js-yaml advisory cleared as a
      supporting check
- [x] Confirm the extension `version` field is unchanged
- [ ] After the change lands on the scanned branch, confirm
      via `gh api repos/chdalski/rlsp/dependabot/alerts/50`
      that alert #50 has left the `open` state, allowing for
      GitHub's re-scan latency (lead, at plan completion)

## Tasks

### Task 1: Advance the js-yaml override to ^4.3.1 and regenerate the lockfile

Bump the single `js-yaml` entry in the `pnpm.overrides`
block to `^4.3.1` and regenerate the lockfile so no
`js-yaml` version below 4.3.1 resolves. The manifest edit
and lockfile regeneration land together — editing
`package.json` without regenerating the lockfile leaves CI
in a broken (`ERR_PNPM_OUTDATED_LOCKFILE`) state.

Files: `rlsp-yaml/integrations/vscode/package.json` (the
`js-yaml` override) and `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`
(regenerated).

- [x] The `js-yaml` override targets `^4.3.1` (or a later
      patched version)
- [x] `pnpm-lock.yaml` is regenerated so `js-yaml` resolves
      ≥ 4.3.1 and no resolved `js-yaml` version remains in
      the vulnerable range `>= 4.0.0, < 4.3.1`. This lockfile
      check is the deterministic, commit-time proof; the
      GitHub alert re-query is the lead's post-merge
      confirmation (Steps), not a Task-1 criterion
- [x] Local `pnpm audit` no longer reports the js-yaml
      `!!omap` advisory (supporting check)
- [x] `pnpm run build`, `pnpm run lint`, `pnpm run format`,
      and `pnpm run test` all pass
- [x] The extension's own `version` field in `package.json`
      is unchanged; no `Cargo.toml` is modified
- [x] Working-tree changes are limited to `package.json` and
      `pnpm-lock.yaml`; no scratch or throwaway files remain

## Decisions

- **Separate plan from the ten-alert patch:** alert #50 is a
  distinct advisory that surfaced after that plan was
  completed and its plan file frozen; new work gets its own
  plan.
- **Single override bump:** only the `js-yaml` pin changes;
  same mechanism and verification shape as commit `bb11c34b`.
- **js-yaml patched despite being dev-only:** it does not
  reach the shipped runtime, but the fix is a trivial pin
  bump and Dependabot flags it, so patching it clears the
  alert at negligible cost.
- **Override-block consolidation is owned by a dedicated
  plan, not bundled here (user-endorsed 2026-08-07):** the
  `pnpm.overrides` audit-and-prune work is scheduled as its
  own plan, `2026-08-07-vscode-overrides-consolidation-audit.md`
  (NotStarted), to run after this security fix lands. It is
  kept separate from this single-package security patch for
  two reasons: (1) it closes no open advisory — every flagged
  alert, including #50, is patched by the security plans — so
  it is maintenance hygiene, not security work; and (2)
  consolidation means *removing* pins, which carries a small
  risk of re-opening a vulnerability if a pin is still
  load-bearing, so it is safer done deliberately in its own
  reviewed plan than bundled into a security patch.

## Non-Goals

- Auditing or pruning the other `pnpm.overrides` entries —
  owned by the dedicated consolidation plan
  `2026-08-07-vscode-overrides-consolidation-audit.md` (see
  Decisions for why it is not bundled here).
- Bumping any direct dependency or devDependency beyond the
  `js-yaml` override pin.
- Any change to the Rust crates, `Cargo.toml`, or
  `Cargo.lock`.
- CI workflow changes.
