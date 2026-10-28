---
name: project_followup_plans
description: "Open work queue: schema plan order, rlsp-yaml follow-ups, parser bindings, CI/integration items"
metadata:
  type: project
  originSessionId: 1d09bc1e-8453-49ff-a3b4-6f7a66d0a9d7
  modified: 2026-10-28T15:40:00.000Z
---

<!-- Only track open items here. Completed work lives in its plan file
     and git history — duplicating it here adds noise and stale state.
     Remove items when their plan is marked Completed. -->

## Open: rlsp-yaml

- **Schema plan queue (one plan at a time, in this order).** Each needs its own clarification + plan. Order the user set 2026-10-28: 5a (next), then the VS Code settings-schema plan below; 3c (iterative loader) is discussed with the user after those, not planned yet:
  3c. Iterative loader in `rlsp-yaml-parser` (user chose 2026-10-28 to queue it): `LoadState::parse_node` in `rlsp-yaml-parser/src/loader.rs` recurses per nesting level, about 6 KiB per level in unoptimized/coverage builds (depth 100 ≈ 1 MiB, 510 ≈ 6 MiB; release ≈ 0.5 MiB at 510). A finished WIP rewrite (heap stack of open collections, byte-identical `load()` Debug output on 2644 inputs in 3 modes, debug stack ≈ 95 KiB at depth 510, release `load` ≈ 5–8% slower) is on the local-only branch `wip/iterative-loader` (adff1074). Needs its own plan: published crate, yaml-test-suite conformance for `load()`, the test-engineer's budget tests (512/513 levels per collection kind, depth balance across documents, `max_nesting_depth(0)`), and resolved-mode recursion in `register_anchor`/`expand_node` measured.
  4. Bound the thread-local `REGEX_CACHE` and run `validate_schema` off the async executor (`spawn_blocking`). The cache now lives in `src/schema/pattern.rs` (shared by validation and the path resolver). Moving validation to `spawn_blocking` spreads thread-local caches over the blocking pool, so a bounded process-wide cache may fit better.
  4a. Price the path resolver's first pass (security Medium, raised 2026-10-27 during the pattern-resolver plan): the first pass of `resolve_schema_path` (`src/schema/path.rs`) is unpriced and hashes the whole key at every visited node, so a very long document key against a large untrusted schema costs schema nodes × path length × key length per hover/completion request. Only the second pass pays from `KEY_FALLBACK_STEP_ALLOWANCE`. The security engineer proposes its own plan, ideally together with item 4; the user must decide whether a path that resolves today may return nothing once an allowance runs out.
  5a. Fold `AdditionalProperties::Denied` into the always-failing schema (`JsonSchema::always_fails`, added 2026-10-28): `additionalProperties`/`additionalItems`/`unevaluatedProperties` keep their own `Denied` representation, so two representations of `false` exist and a `$ref` to `false` under those keywords gets the generic `schemaNotAllowed` wording instead of the keyword wording. User chose 2026-10-28: separate plan after the false-sub-schemas plan. Pure restructuring, second breaking change of `rlsp_yaml::schema` (removes `AdditionalProperties`); about 60 test references.
  VS Code settings schema (user chose 2026-10-28: separate plan after the tolerant-settings plan): `rlsp-yaml/integrations/vscode/package.json` should mark the values the server now rejects with a Warning — `formatPrintWidth` (and `maxItemsComputed`) as non-negative integers, `schemas` values as strings — so the settings editor flags them before the server does.
  6. `httpProxy` hardening — VS Code setting has no `scope`/`restrictedConfigurations`; `configuration.md` wrongly says "requests are made directly" when unset (ureq reads proxy env vars).
  7. SSRF guard via DNS resolution — hostname-string check only (`foo.localhost`, `localtest.me`, `127.0.0.1.nip.io` pass; DNS rebinding).
  8. Relative/local schemas (`$schema=./schema.json`, `file://`).
  Follow-ups surfaced during validate-kubernetes-schema-url, not yet put to the user: three existing tests in `tests/lsp_lifecycle/kubernetes_detection.rs` (`should_record_schema_association_for_kubernetes_manifest`, `should_use_configured_kubernetes_version_in_schema_url`, `should_record_kubernetes_schema_association_for_builtin_dotted_group`) start without the FakeProxy and reach raw.githubusercontent.com over the real network; new Kubernetes API groups need an entry in `BUILTIN_DOTTED_GROUPS` (association.rs) before auto-detection resolves them.
  Follow-ups from the 2026-10-28 tolerant-settings plan (reviewer/security Lows, not yet put to the user): `configuration.md` "Wrong settings" says a number "must be a non-negative whole number", yet `80.0` is rejected (say "integer without a decimal point"); `MAX_LOGGED_SETTING_CHARS` lives in `server.rs` but `settings.rs` imports it while `server.rs` imports from `settings.rs` (move the constant into `settings.rs`); `tests/lsp_lifecycle/configuration.rs` repeats the `unknownTag` match inline instead of the `has_code` helper; security Low (pre-existing): `loggable_url` (`server.rs`) cannot strip userinfo from cannot-be-a-base URLs such as a `schemas` key or modeline `user:pw@host`.
  Test consolidation candidate from the 2026-10-28 sibling-keys plan: `complete_at_in_sequence_item_suggests_missing_sibling_keys` (C-6, `src/completion.rs`) duplicates `complete_in_sequence_item_suggests_missing_sibling_keys` (`completion_drivers.rs`) with weaker assertions; kept because that plan forbade deleting tests.
  Follow-ups surfaced during share-ref-targets, not yet put to the user: schema-cache eviction across URLs (documents cache grows unbounded); negative cache for rejected/failed schemas (re-fetched + re-parsed on every edit); test consolidation proposals the test-engineer raised in Tasks 2/4/5/6 (duplicate overwrite tests, vacuous `let _ =` tests, duplicate deep-nesting fixtures); `cargo doc -p rlsp-yaml --no-deps` fails (public doc of `max_line_length` links private `MAX_LINE_LENGTH_CAP`, pre-existing).

- **`autoWrapFlowStyle` opt-out for `block_to_flow`** — The code-action user-format-config plan (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`) sets `block_to_flow`'s default to auto-wrap output to fit `formatPrintWidth`, removing the misleading `(long line)` title suffix. If users later report wanting single-line flow output regardless of length (e.g., for tooling that prefers compact JSON-like flow), add a setting like `autoWrapFlowStyle: false` (default `true`) so the action emits unwrapped flow when the user opts out. Defer until evidence shows demand — pre-emptively shipping the setting risks configuration sprawl across all code actions. Bias: do nothing unless a user explicitly asks for it.

- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.

- **Replace `WORKLIST.md` + `SKIP_LIST` with `#[ignore]`** (user preference, 2026-04-18; never planned): the shrink-only skip-list machinery is heavier than needed. Delete `rlsp-yaml/tests/corpus/WORKLIST.md`, remove `SKIP_LIST` and its matching logic from `tests/corpus_invariants/shared.rs` (empty since April), rewrite the top-of-file comment in `tests/corpus_invariants/main.rs` to "no known failures; fix a new one or `#[ignore]` it with a plan reference". The harness is already split into one test per invariant (2026-05-20 plan), so per-test `#[ignore]` works. If the corpus expansion above lands first, its new failures go through `#[ignore]` instead of the Surprise Failure Protocol.
## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)

## Open: CI & integrations

- **`configuration.md` diagnostic-code table is incomplete** (found 2026-10-28): emitted codes such as `schemaAdditionalProperties`, `schemaUnevaluatedProperty`, `schemaNot`, `schemaContains`, `schemaMinItems` are missing from the table in `rlsp-yaml/docs/configuration.md`. User queued it as a follow-up.
- **Security Low: "(all required)" completion snippet does not escape names** (found 2026-10-28 by the security engineer, pre-existing): `schema_key_completions` in `src/completion/schema_completions.rs` inserts the schema's `required` names into LSP snippet text without escaping `$`, `}`, `\`, or newlines, so a hostile schema can inject snippet syntax. Needs its own task.

- **vsce is unpinned in CI and the local `package` script.** `vscode-extension.yml` (package and publish jobs, the latter with `VSCE_PAT`) and the extension's `package` script run `pnpx @vscode/vsce`, which fetches the latest vsce each run. Pinning was put to the user during the 2026-10-27 Dependabot plan and declined for now ("Nicht jetzt"); a pin inside a script string is not updated by Dependabot. Revisit only if the user raises it.

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
