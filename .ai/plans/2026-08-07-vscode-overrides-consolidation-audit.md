**Repository:** root
**Status:** InProgress
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
  guards `brace-expansion` and `fast-uri` with two kinds of
  assertion: (1) that the `pnpm.overrides` block literally
  contains their pin entries (e.g. `brace-expansion@2: ^2.1.4`,
  `fast-uri: ^3.1.5`), and (2) that the lockfile resolves
  them to non-vulnerable versions on their dependency paths.
  Both kinds are coupled to the current pinned state, so
  removing or changing either guarded pin will break them.
  The safety property the guard protects is "`brace-expansion`
  and `fast-uri` resolve to non-vulnerable versions" — that
  property must never be weakened or deleted to force a pass.
  But if the audit *proves* a guarded pin redundant (its
  dependency chain resolves a non-vulnerable version without
  the override), updating the guard's assertions to the new
  proven state is legitimate stale-artifact cleanup, not
  weakening. Task 2 states the exact rule.
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

- [x] Enumerate the current `pnpm.overrides` entries and,
      for each, the advisory/advisories it was added for
- [x] For each pin, determine whether it is still
      load-bearing — whether removing it still resolves a
      non-vulnerable version — with `pnpm why` evidence and
      the relevant advisory range
- [x] Record a per-pin verdict (keep / remove) with evidence
      (see Audit Verdicts section)
- [ ] Remove the pins proven redundant; regenerate the
      lockfile
- [ ] Verify build, lint, format, and tests pass, and that
      the lockfile still resolves non-vulnerable versions for
      every previously-pinned package
- [ ] After the change lands, confirm no prior-closed
      Dependabot alert has returned to `open`
- [ ] At plan completion, remove the override-consolidation
      backlog entry from `.ai/memory/project_followup_plans.md`
      (lead) — that note names this plan as its owner and
      instructs its own removal once this plan is Completed

## Tasks

### Task 1: Audit each override pin and record a per-pin verdict

Produce an evidence-backed verdict for every entry in the
`pnpm.overrides` block: is the pin still load-bearing, or has
the dependency graph moved past the vulnerable range so the
pin can be removed safely? This is investigation — its output
is the documented verdict that Task 2 acts on.

- [x] Every current override entry has a recorded verdict
      (keep or remove) with `pnpm why <pkg>` evidence and the
      advisory range it addresses
- [x] Each "remove" verdict states the non-vulnerable version
      the dependency resolves to without the override
- [x] Each "keep" verdict states which consumer still pulls a
      version inside the advisory range absent the pin
- [x] The verdict set is recorded in this plan (Decisions or
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
      and `pnpm run test` all pass. For the `overrides.test.ts`
      guard: if `brace-expansion@2` and/or `fast-uri` are
      marked "remove", the override-block-presence assertion(s)
      for the removed pin are deleted, and any resolution
      assertion for that package is updated to assert the new
      resolved (still non-vulnerable) version it lands on
      without the override — never deleted or loosened in a
      way that would let a version inside the advisory range
      pass. Every guarded pin that is KEPT has its assertions
      left exactly as-is. If both guarded pins are kept,
      `overrides.test.ts` is unchanged
- [ ] The extension `version` field is unchanged; no
      `Cargo.toml` is modified
- [ ] After the change lands on the scanned branch, no
      Dependabot alert closed by a prior security plan has
      returned to `open` (lead, at plan completion)

## Audit Verdicts (Task 1 — recorded 2026-08-07)

Empirical redundancy test (does the chain resolve a
non-vulnerable version without the override?), reviewer-verified
with the faithful Task-2 model (real lockfile, remove the
override, full `pnpm install`, then `pnpm why` + `pnpm audit`).

- **REMOVE (9)** — each resolves its patched version absent the
  override, zero advisories: `lodash`, `fast-uri`, `qs`,
  `undici`, `postcss`, `form-data`, `markdown-it`, `js-yaml`,
  `brace-expansion@2`.
- **KEEP (1)** — genuinely load-bearing: `serialize-javascript`
  — `mocha@11.7.5` declares `^6.0.2` whose `< 7.0.0` ceiling
  excludes the 7.0.5 patch, so it resolves the vulnerable 6.0.2
  without the pin.
- **DEFERRED (1)** — `brace-expansion@5`: the `^5.0.6` override
  gives no protection (resolves the vulnerable 5.0.7 with or
  without it; live GHSA-mh99-v99m-4gvg / GHSA-rgw5-rvv9-x895).
  Handled by the `2026-08-07-vscode-brace-expansion-5-dos-patch.md`
  fix (bump to `^5.0.9`); its keep/remove status is re-evaluated
  empirically at Task 2 after that fix lands.

Task 2 removes the 9 REMOVE pins, keeps `serialize-javascript`,
and re-checks `brace-expansion@5`.

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
- **Sequencing and the deferred compensating control (user
  decision 2026-08-07):** the user directed doing the
  `brace-expansion@5` fix first, then this removal (Task 2),
  and deferring the compensating control (a CI `pnpm audit`
  step + a pnpm/`npm` Dependabot entry) "for now." So Task 2
  runs after the `brace-expansion@5` fix WITHOUT that control
  in place. The 9 removals are safe today (empirically
  verified); the trade-off is that a future regression on one
  of these subtrees would be caught reactively by a Dependabot
  alert (post-merge) rather than blocked pre-merge. The
  deferred control is tracked in
  `.ai/memory/project_followup_plans.md` so it is not lost.

## Non-Goals

- Adding, advancing, or re-scoping any pin — this plan only
  removes provably-redundant pins and leaves load-bearing
  ones exactly as they are.
- Any change to the Rust crates, `Cargo.toml`, or
  `Cargo.lock`.
- CI workflow changes.
- Fixing any newly-surfaced advisory — a new alert is its own
  security plan, not part of this cleanup.
