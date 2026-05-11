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

- **`block_to_flow` policy enforcement against `formatEnforceBlockStyle`** — When the user sets `formatEnforceBlockStyle: true`, the workspace formatter rewrites all flow-style collections back to block style on save. The `block_to_flow` code action contradicts this policy: applying it produces flow output that the next save reverts. Open question: should `block_to_flow` be suppressed (not offered) when `formatEnforceBlockStyle: true`, or should it remain offered with a UI hint, or is the conflict acceptable as user-driven? Surfaced during the code-action user-format-config plan audit (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`, in Decisions/Non-Goals). Separate plan needed — the question is policy/UX, not a correctness bug. Related but distinct: are there other code actions whose existence violates user formatter policy when settings are non-default?

- **`autoWrapFlowStyle` opt-out for `block_to_flow`** — The code-action user-format-config plan (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`) sets `block_to_flow`'s default to auto-wrap output to fit `formatPrintWidth`, removing the misleading `(long line)` title suffix. If users later report wanting single-line flow output regardless of length (e.g., for tooling that prefers compact JSON-like flow), add a setting like `autoWrapFlowStyle: false` (default `true`) so the action emits unwrapped flow when the user opts out. Defer until evidence shows demand — pre-emptively shipping the setting risks configuration sprawl across all code actions. Bias: do nothing unless a user explicitly asks for it.

- **Document rlsp-yaml ↔ prettier (and other formatters) interop** — The code-action user-format-config plan (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`) made code-action output use rlsp-yaml's `formatPrintWidth`. Many users run rlsp-yaml as their LSP server but prettier (or a different formatter) as their format-on-save formatter. In that setup the user maintains two parallel print-width settings: rlsp-yaml's `formatPrintWidth` (controls code-action output shape) and prettier's `printWidth` (controls save-time reformatting). Both default to 80, so most users won't notice — but a customized one without the other produces mid-edit visual jitter (code action wraps for 80; save reformats to 100 or vice versa). Documentation-only follow-up: add a section to `rlsp-yaml/docs/configuration.md` explaining the interaction, listing which rlsp-yaml settings have prettier equivalents (`formatPrintWidth`/`printWidth`, `formatSingleQuote`/`singleQuote`, `formatBracketSpacing`/`bracketSpacing`), and recommending users keep them aligned when both formatters are active. **Out of scope:** automatic cross-formatter config awareness (e.g., reading `.prettierrc` for fallback values) — that's a much heavier change and invites a "which other prettier knobs do we honor?" rabbit hole. Start with documentation; consider richer interop only if user demand emerges.

- **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.

- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.
## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)
