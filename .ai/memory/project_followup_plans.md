---
name: follow-up-task-queue
description: "Remaining items after parser implementation, conformance hardening, migration, and workaround removal"
metadata:
  node_type: memory
  type: project
  originSessionId: 1d09bc1e-8453-49ff-a3b4-6f7a66d0a9d7
  modified: 2026-10-24T08:43:33.526Z
---

<!-- Only track open items here. Completed work lives in its plan file
     and git history — duplicating it here adds noise and stale state.
     Remove items when their plan is marked Completed. -->

## Open: rlsp-fmt

## Open: rlsp-yaml

- **Schema plan queue (user-approved 2026-10-23, one plan at a time, in this order).** Resume `.ai/plans/2026-10-23-composition-step-budget.md` at Task 2 first (Task 1 committed in 4702cb80, not yet pushed). Then, each needing its own clarification + plan:
  1. Bound schema size at parse time — `$ref` targets are copied inline, only depth is limited (Non-Goal of the step-budget plan).
  2. Hover docs for array items and nested composition (Non-Goal of the step-budget plan).
  2b. Bound the thread-local `REGEX_CACHE` (grows with every new pattern) and run `validate_schema` off the async executor (today inline at `server.rs` `process_schema`, not `spawn_blocking`) — Non-Goal of the step-budget plan, user-approved 2026-10-24.  2c. Bug (verified 2026-10-24): `unevaluatedItems: false` is dropped — it is parsed via `parse_schema_with_root`, which returns `None` for `Value::Bool(false)`, while `unevaluatedProperties` goes through `parse_additional_properties`, which handles `false`. So it never produces diagnostics. Check every other keyword parsed via `parse_schema_with_root` for the same boolean-`false` loss.
  3. `httpProxy` hardening — VS Code setting has no `scope` and is not in `restrictedConfigurations`, so an untrusted workspace can set it; `configuration.md` wrongly says "requests are made directly" when unset (ureq reads `ALL_PROXY`/`HTTPS_PROXY`/`HTTP_PROXY`).
  4. SSRF guard via DNS resolution — `is_ssrf_blocked_host` checks only the hostname string; `foo.localhost`, `localtest.me`, `127.0.0.1.nip.io` resolve to loopback and pass; fix idea: ureq `Agent::with_parts` + `unversioned::resolver::Resolver`; `configuration.md` claims "resolve to".
  5. Relative/local schemas (`$schema=./schema.json`, `file://`) — feature gap vs yaml-language-server.
  The user picked items 3–5 before items 1–2 surfaced; confirm the order with the user before starting.

- **`autoWrapFlowStyle` opt-out for `block_to_flow`** — The code-action user-format-config plan (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`) sets `block_to_flow`'s default to auto-wrap output to fit `formatPrintWidth`, removing the misleading `(long line)` title suffix. If users later report wanting single-line flow output regardless of length (e.g., for tooling that prefers compact JSON-like flow), add a setting like `autoWrapFlowStyle: false` (default `true`) so the action emits unwrapped flow when the user opts out. Defer until evidence shows demand — pre-emptively shipping the setting risks configuration sprawl across all code actions. Bias: do nothing unless a user explicitly asks for it.

- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.
## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)

## Open: CI & integrations

<!-- Surfaced during the 2026-07-23 dependabot-followup plan
     (`.ai/plans/2026-07-23-dependabot-followup-ci-and-security-maintenance.md`).
     All three were verified out-of-scope for that overrides/CI-config plan. -->

- **`dtolnay/rust-toolchain` refs pin mutable branches, not tags or SHAs.**
  Every `dtolnay/rust-toolchain` workflow ref uses `@1.98.1`, which resolves a *branch* the action
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
