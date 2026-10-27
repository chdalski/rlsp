---
name: follow-up-task-queue
description: "Remaining items after parser implementation, conformance hardening, migration, and workaround removal"
metadata:
  node_type: memory
  type: project
  originSessionId: 1d09bc1e-8453-49ff-a3b4-6f7a66d0a9d7
  modified: 2026-10-26T08:51:17.157Z
---

<!-- Only track open items here. Completed work lives in its plan file
     and git history — duplicating it here adds noise and stale state.
     Remove items when their plan is marked Completed. -->

## Open: rlsp-fmt

## Open: rlsp-yaml

- **Schema plan queue (one plan at a time, in this order; share-ref-targets completed 2026-10-26; validate-kubernetes-schema-url has all tasks committed and pushed but stays InProgress until the VS Code workflow is green — it fails only at `pnpm run audit` on the two Dependabot alerts below, so close it after item 2).** Each needs its own clarification + plan:
  2. Dependabot alerts in the VS Code extension (user-requested 2026-10-26) — `@azure/msal-node` 5.6.0 (fixed in 7.1.0, major bump, likely blocked by a dependent's range) and `serialize-javascript` 7.1.2 (fixed in 7.1.3, may self-heal after Dependabot's 3-day cooldown).
  3. Hover docs for array items and nested composition — includes unifying hover's and completion's path resolvers (they duplicate "which sub-schema describes this path").
  4. Bound the thread-local `REGEX_CACHE` and run `validate_schema` off the async executor (`spawn_blocking`).
  5. Bug: `unevaluatedItems: false` is dropped (boolean `false` sub-schema loss in the parser); check every keyword parsed through the generic sub-schema path.
  6. `httpProxy` hardening — VS Code setting has no `scope`/`restrictedConfigurations`; `configuration.md` wrongly says "requests are made directly" when unset (ureq reads proxy env vars).
  7. SSRF guard via DNS resolution — hostname-string check only (`foo.localhost`, `localtest.me`, `127.0.0.1.nip.io` pass; DNS rebinding).
  8. Relative/local schemas (`$schema=./schema.json`, `file://`).
  Follow-ups surfaced during validate-kubernetes-schema-url, not yet put to the user: a non-string `kubernetesVersion` (e.g. the number 1.29) makes the whole `Settings` deserialization fail silently, resetting every setting; three existing tests in `tests/lsp_lifecycle/kubernetes_detection.rs` (`should_record_schema_association_for_kubernetes_manifest`, `should_use_configured_kubernetes_version_in_schema_url`, `should_record_kubernetes_schema_association_for_builtin_dotted_group`) start without the FakeProxy and reach raw.githubusercontent.com over the real network; new Kubernetes API groups need an entry in `BUILTIN_DOTTED_GROUPS` (association.rs) before auto-detection resolves them.
  Follow-ups surfaced during share-ref-targets, not yet put to the user: schema-cache eviction across URLs (documents cache grows unbounded); negative cache for rejected/failed schemas (re-fetched + re-parsed on every edit); test consolidation proposals the test-engineer raised in Tasks 2/4/5/6 (duplicate overwrite tests, vacuous `let _ =` tests, duplicate deep-nesting fixtures); `cargo doc -p rlsp-yaml --no-deps` fails (public doc of `max_line_length` links private `MAX_LINE_LENGTH_CAP`, pre-existing).

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
