**Repository:** root
**Status:** NotStarted
**Created:** 2026-08-07

# VS Code Extension: Audit and Prune the pnpm.overrides Block

## Goal

The VS Code extension's `pnpm.overrides` block
(`rlsp-yaml/integrations/vscode/package.json`) has grown to
11 transitive-security pins added across roughly seven
sequential security plans, and no plan has ever checked
whether each pin is still doing anything. A pin becomes
redundant once the underlying dependency chain resolves a
non-vulnerable version on its own — at which point the
override is dead configuration that every future dependency
bump must still be reasoned around. Audit each pin, and
remove exactly those proven redundant, so the block carries
only pins that are still load-bearing. Removing a pin must
not re-open any Dependabot alert: every removal is verified
to still resolve a non-vulnerable version, and the full
build/lint/format/test suite must stay green.

This is maintenance hygiene, not a security fix — it closes
no open advisory. Its value is a smaller, comprehensible
override block and less standing maintenance surface.

## Context

- The block is this project's mechanism for pinning
  transitive dependencies to patched versions. As of this
  plan it holds 11 entries; the exact list and current
  values are enumerated at audit time from
  `rlsp-yaml/integrations/vscode/package.json` rather than
  hardcoded here (they shift as security plans land, e.g.
  the concurrent js-yaml #50 patch).
- **Directionally sensitive change.** Unlike the security
  plans, which *add or advance* pins (always safe — a
  higher floor cannot re-introduce a vulnerability), this
  plan *removes* pins. Removing a pin that is still
  load-bearing silently re-opens a vulnerability with no
  build or test failure. Each removal therefore requires
  explicit proof that the dependency still resolves to a
  non-vulnerable version without the override.
- **Regression guard.** `rlsp-yaml/integrations/vscode/src/overrides.test.ts`
  asserts the lockfile resolves patched versions for
  `brace-expansion` and `fast-uri`. If either of those pins
  is a removal candidate, that guard is the safety net —
  do not weaken or delete its assertions to make a removal
  pass.
- **Authoritative safety signal.** After the change lands
  on the branch GitHub scans, the Dependabot alert list
  (`gh api repos/chdalski/rlsp/dependabot/alerts`) must show
  no alert that was closed by a prior security plan return
  to `open`. The deterministic, commit-time proof is that
  the regenerated lockfile still resolves a non-vulnerable
  version for every audited package.
- **Establishing plans / prior context:** the pins were
  added by the security plans archived under `.ai/plans/`
  (overrides, transitive patches, js-yaml/brace-expansion
  DoS patches, the 2026-07-23 follow-up, the 2026-08-07
  ten-alert patch, and the 2026-08-07 js-yaml #50 patch).
  This plan graduates the consolidation backlog item
  recorded in `.ai/memory/project_followup_plans.md` into a
  scheduled plan.
- **Version fields off-limits:** do not touch the
  extension's own `version` field or any `Cargo.toml`.
- **Tooling:** pnpm 10.33.2, Node v24.14.1. Commands in the
  root `/workspace/CLAUDE.md` under "VS Code Extension".

## Steps

- [ ] Enumerate the current `pnpm.overrides` entries and,
      for each, the advisory/advisories it was added for
- [ ] For each pin, determine whether it is still
      load-bearing — whether removing it still resolves a
      non-vulnerable version — with `pnpm why` evidence and
      the relevant advisory range
- [ ] Record a per-pin verdict (keep / remove) with evidence
- [ ] Remove the pins proven redundant; regenerate the
      lockfile
- [ ] Verify build, lint, format, and tests pass, and that
      the lockfile still resolves non-vulnerable versions for
      every previously-pinned package
- [ ] After the change lands, confirm no prior-closed
      Dependabot alert has returned to `open`

## Tasks

### Task 1: Audit each override pin and record a per-pin verdict

Produce an evidence-backed verdict for every entry in the
`pnpm.overrides` block: is the pin still load-bearing, or has
the dependency graph moved past the vulnerable range so the
pin can be removed safely? This is investigation — its output
is the documented verdict that Task 2 acts on.

- [ ] Every current override entry has a recorded verdict
      (keep or remove) with `pnpm why <pkg>` evidence and the
      advisory range it addresses
- [ ] Each "remove" verdict states the non-vulnerable version
      the dependency resolves to without the override
- [ ] Each "keep" verdict states which consumer still pulls a
      version inside the advisory range absent the pin
- [ ] The verdict set is recorded in this plan (Decisions or
      an appended audit note) so Task 2's scope is
      unambiguous

### Task 2: Remove the redundant pins and verify no alert re-opens

Remove exactly the pins the audit marked "remove", regenerate
the lockfile, and prove nothing regressed. Only pins with a
recorded "remove" verdict are touched; "keep" pins are left
untouched.

- [ ] Only the pins marked "remove" in Task 1 are deleted
      from the `pnpm.overrides` block; no "keep" pin changes
- [ ] `pnpm-lock.yaml` is regenerated and still resolves a
      non-vulnerable version for every package that had a pin
      (removed or kept) — no version enters a known advisory
      range
- [ ] `pnpm run build`, `pnpm run lint`, `pnpm run format`,
      and `pnpm run test` all pass, including the
      `overrides.test.ts` regression guard with no assertion
      weakened
- [ ] The extension `version` field is unchanged; no
      `Cargo.toml` is modified
- [ ] After the change lands on the scanned branch, no
      Dependabot alert closed by a prior security plan has
      returned to `open` (lead, at plan completion)

## Decisions

- **Removal requires proof, not preference:** a pin is
  removed only when the audit proves the dependency resolves
  a non-vulnerable version without it. "Fewer pins is nicer"
  is the motivation, never the justification for a specific
  removal.
- **Split audit from removal:** Task 1 establishes the
  removal set with evidence before Task 2 changes anything,
  so the reviewer can check each removal against a recorded
  verdict rather than re-deriving it.
- **Runs after the security patches:** scheduled after the
  2026-08-07 js-yaml #50 patch so it audits the final pinned
  state, not a moving target.

## Non-Goals

- Adding, advancing, or re-scoping any pin — this plan only
  removes provably-redundant pins and leaves load-bearing
  ones exactly as they are.
- Any change to the Rust crates, `Cargo.toml`, or
  `Cargo.lock`.
- CI workflow changes.
- Fixing any newly-surfaced advisory — a new alert is its own
  security plan, not part of this cleanup.
