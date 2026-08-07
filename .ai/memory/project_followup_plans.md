---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

<!-- Only track open items here. Completed work lives in its plan file
     and git history — duplicating it here adds noise and stale state.
     Remove items when their plan is marked Completed. -->

## Open: rlsp-fmt

## Open: rlsp-yaml

- **`autoWrapFlowStyle` opt-out for `block_to_flow`** — The code-action user-format-config plan (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`) sets `block_to_flow`'s default to auto-wrap output to fit `formatPrintWidth`, removing the misleading `(long line)` title suffix. If users later report wanting single-line flow output regardless of length (e.g., for tooling that prefers compact JSON-like flow), add a setting like `autoWrapFlowStyle: false` (default `true`) so the action emits unwrapped flow when the user opts out. Defer until evidence shows demand — pre-emptively shipping the setting risks configuration sprawl across all code actions. Bias: do nothing unless a user explicitly asks for it.

- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.
## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)

## Open: CI & integrations

<!-- Surfaced during the 2026-07-23 dependabot-followup plan
     (`.ai/plans/2026-07-23-dependabot-followup-ci-and-security-maintenance.md`).
     All three were verified out-of-scope for that overrides/CI-config plan. -->

- **`dtolnay/rust-toolchain` refs pin mutable branches, not tags or SHAs.**
  All 8 workflow refs use `@1.97.1`, which resolves a *branch* the action
  pre-creates ahead of the actual Rust release (this is the same
  branches-as-versions behavior that produced the fictional-`1.100.0` PR #50
  and prompted the dependabot `ignore` rule in that plan). A mutable branch
  ref is re-resolvable and violates the project convention in
  `.claude/rules/github-workflows.md` (pin to version tags, not SHAs — but
  these are neither). Consider pinning to commit SHAs (with a comment naming
  the version) or accept the branch pin deliberately. Low urgency; the pin is
  bumped manually in lockstep, so drift is bounded. Verify the current
  pinning posture before acting — this note reflects 2026-07-23 state.

- **`packageManager` field may warrant a `+<integrity>` hash.** Task 4 made CI
  resolve pnpm from `rlsp-yaml/integrations/vscode/package.json`'s
  `"packageManager": "pnpm@10.33.2"`. `pnpm/action-setup` supports (and
  strips) an optional `+<sha512...>` integrity suffix. The security advisor
  rated adding one **low-severity / non-blocking**. If pursued, confirm the
  action's current handling and that the hash matches the intended pnpm
  release.

- **`brace-expansion@5` override floor `^5.0.6` sits below the resolved
  `5.0.7`.** No active exposure — the lockfile resolves `@5` to `5.0.7`
  today — but the override string does not pin that floor, so a future
  re-resolution could permit `5.0.6`. If tightening to `^5.0.7`, verify the
  specific advisory coverage of `5.0.6` at scoping time rather than taking
  the claim on faith (flagged by the Task 5 security advisor).

- **`pnpm.overrides` block consolidation audit.** The VS Code extension's
  `pnpm.overrides` block (`rlsp-yaml/integrations/vscode/package.json`) has
  accumulated across ~7 sequential security plans and now pins 11 packages
  (`lodash`, `serialize-javascript`, `qs`, `form-data`, `markdown-it`,
  `js-yaml`, `undici`, `fast-uri`, `postcss`, `brace-expansion@2`,
  `brace-expansion@5`). No plan has audited whether each pin is still
  load-bearing — a transitive chain may have moved past the vulnerable
  version on its own, making an override redundant and safe to drop.
  Deferred from the 2026-08-07 advisory-patch plan
  (`.ai/plans/2026-08-07-vscode-dependabot-advisory-patches.md`) to keep
  that fix tight and independently reviewable. When acting: for each
  override, determine whether removing it still resolves a non-vulnerable
  version (`pnpm why <pkg>` against the relevant advisory ranges); remove
  only pins proven redundant, and verify each removal with `pnpm install`
  + build/lint/test. Do not remove a pin without confirming the underlying
  advisory is no longer reachable — a redundant-looking pin can become
  load-bearing again after an unrelated dependency bump.
  **Now scheduled as a plan:** `.ai/plans/2026-08-07-vscode-overrides-consolidation-audit.md`
  (InProgress — audit done, removal pending) owns this work — remove this
  backlog note when that plan is marked Completed.

- **Compensating control for npm advisories (CI `pnpm audit` + Dependabot
  coverage).** The VS Code extension has no CI `pnpm audit` gate and no
  Dependabot npm-ecosystem entry (verified 2026-08-07: no `audit` in any
  `.github/workflows/*.yml`; `.github/dependabot.yml` declares only `cargo`
  and `github-actions`). GitHub Dependabot *alerts* still fire post-merge,
  but there is no pre-merge/local signal. This matters because the
  2026-08-07 overrides consolidation removes 9 manifest-level pins that were
  the only proactive guardrail against these subtrees regressing into a
  vulnerable range on a future dependency bump. Deferred by the user
  ("leave 2 out for now", 2026-08-07). When acting: add a `pnpm audit` step
  to `vscode-extension.yml` and/or a `package-ecosystem: "npm"` entry (which
  natively handles `pnpm-lock.yaml`, no npm involved) scoped to
  `directory: /rlsp-yaml/integrations/vscode` in `.github/dependabot.yml`.
  Security-engineer rated the gap Medium.
