**Repository:** root
**Status:** NotStarted
**Created:** 2026-08-07

# VS Code Extension: Patch brace-expansion@5 DoS Advisories

## Goal

The VS Code extension's lockfile resolves `brace-expansion@5.0.7`
(pinned via `brace-expansion@5: ^5.0.6` in
`rlsp-yaml/integrations/vscode/package.json`), which is
vulnerable to two high-severity DoS advisories that `pnpm audit`
flags but GitHub Dependabot does not currently surface as an
alert:

- **GHSA-mh99-v99m-4gvg** — DoS via unbounded expansion length
  causing an OOM crash. Vulnerable `>= 4.0.0, < 5.0.8`.
- **GHSA-rgw5-rvv9-x895** — DoS via unbounded intermediate
  arrays, bypassing the GHSA-mh99 mitigation. Vulnerable
  `>= 4.0.0, < 5.0.9`.

Resolve both by advancing the `brace-expansion@5` override to
`^5.0.9` (the first version outside both ranges; `5.0.9` is
published) and regenerating the lockfile so no `brace-expansion`
5.x version below 5.0.9 resolves. Build, lint, format, and tests
must stay green. The deterministic, commit-time proof is that the
regenerated lockfile resolves `brace-expansion` on the 5.x line
to ≥ 5.0.9 and `pnpm audit` no longer reports either advisory.

## Context

- **Same advisories, different major line.** These are the same
  two advisories already fixed for the `brace-expansion` **2.x**
  line (Dependabot alerts #40/#41, patched to 2.1.4 in commit
  `bb11c34b`). This is the **5.x** line instance, which GitHub
  Dependabot did not separately alert on (open Dependabot alert
  count is 0), but which `pnpm audit` catches as a live exposure
  against the resolved `5.0.7`.
- **Dev-only surface (Medium).** The security-engineer rated
  this Medium: `brace-expansion@5` reaches only the dev/build
  toolchain (eslint / typescript-eslint via `minimatch@10.2.5`),
  not the shipped `.vsix`, so there is no end-user exposure —
  but real exposure to CI and contributor machines via crafted
  glob input.
- **Regression guard must be updated (expected third file).**
  `rlsp-yaml/integrations/vscode/src/overrides.test.ts` guards
  the `brace-expansion@5` pin with two literals: an
  override-block-presence assertion
  (`expect(overridesBlock).toContain('brace-expansion@5: ^5.0.6')`)
  and a resolution assertion on the `minimatch@10.2.5` path
  (`expect(resolvedDependencyVersion('minimatch@10.2.5:', 'brace-expansion')).toBe('5.0.7')`).
  Bumping the pin makes both literals stale, so this plan updates
  them to the new pinned floor (`^5.0.9`) and the new resolved
  version. This is a literal-tracking update — the guard's design
  and its non-vulnerable-resolution intent are preserved, no
  assertion is weakened. So this task's working tree changes
  three files, not two.
- **Established mechanism.** The `pnpm.overrides` block is this
  project's transitive-security-pin mechanism; this is the same
  single-pin-bump shape as the prior `brace-expansion@5` patch
  (commit `6608bbf8`, plan
  `archive/2026-07-08-vscode-brace-expansion-redos-patch.md`) and
  the recent js-yaml #50 patch (`d0f059e0`).
- **Version fields off-limits.** Do not touch the extension's own
  `version` field or any `Cargo.toml`.
- **Tooling:** pnpm 10.33.2 (matches declared `packageManager`),
  Node v24.14.1. Commands in the root `/workspace/CLAUDE.md`
  under "VS Code Extension".

## Steps

- [x] Confirm the two advisories, their vulnerable ranges, the
      resolved `5.0.7`, and that `5.0.9` is published
- [ ] Advance the `brace-expansion@5` override to `^5.0.9` in
      `package.json`
- [ ] Regenerate `pnpm-lock.yaml` via `pnpm install`
- [ ] Update the `brace-expansion@5` literals in
      `overrides.test.ts` (presence `^5.0.6` → `^5.0.9`;
      resolution `5.0.7` → the new resolved version)
- [ ] Verify build, lint, format, and tests pass
- [ ] Verify the lockfile resolves `brace-expansion` 5.x ≥ 5.0.9
      with no 5.x version in `>= 4.0.0, < 5.0.9`; `pnpm audit`
      no longer reports either advisory
- [ ] Confirm the extension `version` field is unchanged
- [ ] At plan completion, remove the now-superseded
      `brace-expansion@5` backlog entry from
      `.ai/memory/project_followup_plans.md` (lead) — this plan
      advances the pin past the `^5.0.7` that note contemplated,
      fully resolving the concern it tracked

## Tasks

### Task 1: Advance the brace-expansion@5 override to ^5.0.9 and regenerate

Bump the `brace-expansion@5` entry in the `pnpm.overrides` block
to `^5.0.9`, regenerate the lockfile, and update the
`overrides.test.ts` literals that track this pin. The manifest
edit and lockfile regeneration land together (editing the
manifest without regenerating the lockfile breaks CI with
`ERR_PNPM_OUTDATED_LOCKFILE`).

Files: `rlsp-yaml/integrations/vscode/package.json` (the
`brace-expansion@5` override), `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`
(regenerated), and `rlsp-yaml/integrations/vscode/src/overrides.test.ts`
(literal update).

- [ ] The `brace-expansion@5` override targets `^5.0.9` (or a
      later patched version); no other override entry changes
- [ ] `pnpm-lock.yaml` is regenerated so `brace-expansion` on the
      5.x line resolves ≥ 5.0.9 and no 5.x version remains in
      `>= 4.0.0, < 5.0.9`. This lockfile check is the
      deterministic, commit-time proof
- [ ] `overrides.test.ts` `brace-expansion@5` literals are updated
      to the new pinned floor and the new resolved version; no
      assertion is deleted or loosened in a way that would let a
      vulnerable 5.x version pass. The `brace-expansion@2` and
      `fast-uri` assertions are untouched
- [ ] Local `pnpm audit` no longer reports GHSA-mh99-v99m-4gvg or
      GHSA-rgw5-rvv9-x895 (supporting check)
- [ ] `pnpm run build`, `pnpm run lint`, `pnpm run format`, and
      `pnpm run test` all pass
- [ ] The extension's own `version` field is unchanged; no
      `Cargo.toml` is modified
- [ ] Working-tree changes are limited to `package.json`,
      `pnpm-lock.yaml`, and `src/overrides.test.ts`; no scratch
      files remain

## Decisions

- **Own plan, not folded into the consolidation:** this is an
  advisory *fix* (advancing a pin), which the consolidation
  plan's Non-Goals explicitly exclude; it was surfaced by the
  consolidation audit and scheduled first at the user's
  direction.
- **Bump to ^5.0.9, not ^5.0.8:** GHSA-rgw5-rvv9-x895's range is
  `< 5.0.9`, so 5.0.8 is still vulnerable; 5.0.9 clears both
  advisories.
- **Regression guard updated, not bypassed:** the
  `overrides.test.ts` literal update tracks the new proven-safe
  state; the guard still asserts a non-vulnerable resolution.

## Non-Goals

- Removing or auditing any other override pin — that is the
  separate consolidation plan
  (`2026-08-07-vscode-overrides-consolidation-audit.md`).
- Adding the CI `pnpm audit` / Dependabot `npm` compensating
  control — that is its own separate plan.
- Any change to the Rust crates, `Cargo.toml`, or `Cargo.lock`.
- CI workflow changes.
