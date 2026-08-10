**Repository:** root
**Status:** NotStarted
**Created:** 2026-08-07

# VS Code Extension: npm-Advisory Compensating Control (CI audit + Dependabot)

## Goal

The 2026-08-07 overrides consolidation removed 9 manifest-level
`pnpm.overrides` pins that were the only *proactive* guardrail
against the extension's transitive npm dependencies regressing
into a vulnerable range. Today the only remaining signal is
GitHub Dependabot *alerts*, which fire **reactively** (after a
change lands on the default branch); there is no pre-merge or
local audit gate, and `.github/dependabot.yml` has no npm
ecosystem entry, so Dependabot opens no version-update PRs for
the extension. Add a compensating control that restores
*proactive* coverage:

1. A CI **`pnpm audit` gate** on the VS Code extension so a
   newly-introduced advisory (at or above a chosen severity)
   fails the build before merge.
2. A Dependabot **npm ecosystem entry** for the extension so
   Dependabot proposes dependency-update PRs (it natively
   handles `pnpm-lock.yaml`; the ecosystem key is `"npm"`).

The control must pass on the current tree — which has one
pre-existing dev-only `diff` low advisory
(GHSA-73rr-hh4g-fpgx via `@vscode/test-cli > mocha > diff`) —
without that low spuriously breaking CI.

## Context

- **Why now:** deferred by the user during the consolidation
  ("leave 2 out for now"), then reprioritized after the
  extension CI was returned to green. This is the last item of
  the 2026-08-07 security program.
- **CI shape** (`.github/workflows/vscode-extension.yml`): a
  matrix `Build VSIX` job runs, per target, Install → Build →
  Run tests → integration tests (gated to
  `x86_64-unknown-linux-gnu`) → package. The workflow is
  path-filtered to extension changes and also runs on
  `workflow_dispatch`. A `pnpm audit` step should run **once**,
  not once per matrix target — gate it to the Linux target
  (same pattern as the integration-test step) or add a small
  dedicated job.
- **Severity threshold is a security-control decision.** The
  gate must catch the class of advisory that surfaced this
  session (the brace-expansion@5 highs were **dev-only**, so a
  `--prod`-only scope would have missed them), while not
  breaking on the pre-existing dev-only `diff` **low**. The
  exact threshold/scope (`--audit-level`, `--prod` vs full
  tree, allowlist vs threshold) is for the security advisor to
  specify — this plan routes that decision to them rather than
  prescribing it.
- **Dependabot npm entry:** `package-ecosystem: "npm"`,
  `directory: /rlsp-yaml/integrations/vscode`. "npm" is
  Dependabot's identifier for all JS package managers incl.
  pnpm; it reads `pnpm-lock.yaml`. Mirror the existing entries'
  `schedule`/`groups` style.
- **The `diff` low is out of scope to *fix*** — it is a
  separate dev-only advisory; the control must simply not be
  broken by it. If the security advisor wants it fixed rather
  than tolerated, that becomes its own follow-up, not this
  plan.
- **Version fields off-limits:** no `Cargo.toml` / extension
  `version` changes.

Key files:
- `.github/workflows/vscode-extension.yml` (add the audit gate)
- `.github/dependabot.yml` (add the npm ecosystem entry)
- `rlsp-yaml/integrations/vscode/package.json` (add an `audit`
  script encoding the gate's threshold, so CI and local devs
  run the identical check)
- root `/workspace/CLAUDE.md` — document the new audit gate in
  the "VS Code Extension" command block (mirroring the
  `typecheck` precedent from the CI-fix plan) so contributors
  know it is enforced

## Steps

- [ ] Consult the security advisor on the audit gate's scope
      and severity threshold (must catch dev-tree moderate/high
      advisories; must not break on the pre-existing `diff` low)
- [ ] Add an `audit` script encoding the threshold and wire
      the gate into `vscode-extension.yml` (`pnpm run audit`),
      running once (Linux-gated or a dedicated job)
- [ ] Add the npm ecosystem entry to `.github/dependabot.yml`
- [ ] Document the audit gate (`pnpm run audit`) in the root
      `CLAUDE.md` "VS Code Extension" command block
- [ ] Verify the gate passes on the current tree and
      demonstrate (command + output in the handoff) that it
      fails on a moderate/high advisory; validate the
      dependabot.yml change
- [ ] After the change lands, confirm the extension workflow
      still succeeds and the audit step ran (lead, via
      `gh run`, at plan completion)
- [ ] At plan completion, remove the "Compensating control for
      npm advisories" entry from
      `.ai/memory/project_followup_plans.md` (lead) — this plan
      resolves it

## Tasks

### Task 1: Add the CI pnpm-audit gate and the Dependabot npm entry

Add both halves of the compensating control. They are one
cohesive feature (proactive npm-advisory coverage) though they
touch two files.

Files: `.github/workflows/vscode-extension.yml`,
`.github/dependabot.yml`,
`rlsp-yaml/integrations/vscode/package.json` (new `audit`
script), and root `CLAUDE.md`.

- [ ] An `audit` script in `package.json` encodes the
      scope/threshold the security advisor specified, and
      `vscode-extension.yml` invokes it (`pnpm run audit`)
      exactly once per workflow run (not once per matrix
      target), so CI and local devs run the identical check.
- [ ] The gate **passes on the current tree** (the pre-existing
      dev-only `diff` low does not fail it). The developer's
      handoff includes the actual command and output showing
      the gate **failing** at a lowered threshold (or against a
      known-vulnerable version), so the reviewer can verify the
      would-fail behavior from the handoff alone — the
      committed workflow is left in a passing state.
- [ ] The root `/workspace/CLAUDE.md` "VS Code Extension"
      command block lists `pnpm run audit`, so the enforced
      gate is documented where the build/test reference lives.
- [ ] `.github/dependabot.yml` has an npm ecosystem entry
      scoped to `/rlsp-yaml/integrations/vscode`, consistent
      with the existing entries' schedule/grouping style; the
      file remains valid Dependabot config.
- [ ] No `Cargo.toml` or extension `version` change; only
      `.github/workflows/vscode-extension.yml`,
      `.github/dependabot.yml`,
      `rlsp-yaml/integrations/vscode/package.json`, and root
      `CLAUDE.md` change; no scratch files.
- [ ] The security advisor has signed off (output gate) that
      the gate's scope/threshold meaningfully catches the
      dev-tree advisory class without spurious failures.

## Decisions

- **Threshold routed to the security advisor:** the lead
  identifies the need (proactive gate that covers the dev tree
  where this session's advisories lived, without tripping on
  the known `diff` low); the advisor specifies the concrete
  control, per the risk-assessment rule.
- **Gate on the extension workflow, run once:** the extension
  workflow is path-filtered, so the gate fires when the
  extension is touched (not on unrelated Rust PRs); running it
  once (Linux-gated) avoids 5× redundant audits.
- **Layered, not either/or:** the CI audit (pre-merge, when the
  extension changes) complements Dependabot alerts (post-merge)
  and the new Dependabot npm PRs (routine updates) — together
  they restore the proactive coverage the pin removal spent.

## Non-Goals

- Fixing the pre-existing dev-only `diff` low advisory — the
  gate must tolerate it; fixing it is a separate item if
  desired.
- Re-adding any removed `pnpm.overrides` pin.
- Changes to other workflows or the Rust CI.
- Any `Cargo.toml` / `Cargo.lock` change.
