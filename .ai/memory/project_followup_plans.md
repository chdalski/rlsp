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

- **`.editorconfig` support for the formatter** — Counterpart to the 2026-05-20 formatter disable switch plan (`/workspace/.ai/plans/2026-05-20-formatter-disable-switch-and-interop-docs.md`). The interop doc explicitly states rlsp-yaml does not read `.editorconfig` today and that support is "a separate, planned feature." Scope: respect `indent_style`, `indent_size`, `end_of_line`, `insert_final_newline`, `trim_trailing_whitespace`, and `max_line_length` (mapped to `formatPrintWidth`) from the nearest `.editorconfig` file. Walk up from the file until `root = true` or filesystem root per the spec. Precedence: explicit LSP setting > `.editorconfig` > defaults. Needs its own plan — discovery, precedence rules, watcher hookup for live reload, and per-pattern matching are non-trivial. User agreed (2026-05-20) that this lands AFTER the disable switch.

- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.
## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)
