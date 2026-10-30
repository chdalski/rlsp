---
name: project_followup_plans
description: "Open work queue: schema plan order, rlsp-yaml follow-ups, parser bindings, CI/integration items"
metadata:
  type: project
  originSessionId: 1d09bc1e-8453-49ff-a3b4-6f7a66d0a9d7
  modified: 2026-10-29T16:05:37.851Z
---

<!-- Only track open items here. Completed work lives in its plan file
     and git history — duplicating it here adds noise and stale state.
     Remove items when their plan is marked Completed. -->

## Open: rlsp-yaml

- **Alias expansion depth in resolved mode (verified 2026-10-29; own plan, queued after the two parser plans under "Open: rlsp-yaml-parser").** `max_nesting_depth` does not bound the expanded tree (library only; the server uses lossless mode). Anchor chains (`aK: &aK [[…[*aK-1]…]]`, 500 brackets each) yield depth 1501 from 4 anchors (returned OK at limit 512). 20 anchors (≈20 KB, far below `max_expanded_nodes`) overflow a release build even on an 8 MiB stack. `expand_node`, derived `Clone` (`register_anchor`, `resolve_alias`) and `Drop` all recurse over the expanded depth, so the fix must bound the expanded depth. Code reading only, not verified: the `in_progress` cycle check and the "Accepted risks" note in `loader.rs` look unreachable from `load()`, since `anchor_map` stores already-expanded trees registered after the node completes.
  Reviewer Low from the comment-run fix (not yet put to the user): the arm comment in `parse_node` and the `docs/architecture.md` paragraph say a nested collection hands a comment run to its first entry; an empty flow collection (`key:\n  # c\n  {}`) has none, so the run goes on to the next key (as the tests pin).
- **Schema plan queue (one plan at a time, in this order).** Each needs its own clarification + plan. Order: the alias-expansion depth plan above comes first; 3c (iterative loader) is to be discussed with the user afterwards, not planned yet:
  3c. Iterative loader in `rlsp-yaml-parser` (user chose 2026-10-28 to queue it): `LoadState::parse_node` in `rlsp-yaml-parser/src/loader.rs` recurses per nesting level, about 6 KiB per level in unoptimized/coverage builds (depth 100 ≈ 1 MiB, 510 ≈ 6 MiB; release ≈ 0.5 MiB at 510). A finished WIP rewrite (heap stack of open collections, byte-identical `load()` Debug output on 2644 inputs in 3 modes, debug stack ≈ 95 KiB at depth 510, release `load` ≈ 5–8% slower) is on the local-only branch `wip/iterative-loader` (adff1074). Needs its own plan: published crate, yaml-test-suite conformance for `load()`, the test-engineer's budget tests (512/513 levels per collection kind, depth balance across documents, `max_nesting_depth(0)`), and resolved-mode recursion in `register_anchor`/`expand_node` measured.
  4. Bound the thread-local `REGEX_CACHE` and run `validate_schema` off the async executor (`spawn_blocking`). The cache now lives in `src/schema/pattern.rs` (shared by validation and the path resolver). Moving validation to `spawn_blocking` spreads thread-local caches over the blocking pool, so a bounded process-wide cache may fit better.
  4a. Price the path resolver's first pass (security Medium, raised 2026-10-27 during the pattern-resolver plan): the first pass of `resolve_schema_path` (`src/schema/path.rs`) is unpriced and hashes the whole key at every visited node, so a very long document key against a large untrusted schema costs schema nodes × path length × key length per hover/completion request. Only the second pass pays from `KEY_FALLBACK_STEP_ALLOWANCE`. The security engineer proposes its own plan, ideally together with item 4; the user must decide whether a path that resolves today may return nothing once an allowance runs out.
  6. `httpProxy` and `schemas` hardening (security Medium, re-raised 2026-10-29) — neither VS Code setting has a `scope`/`restrictedConfigurations` entry, so an untrusted workspace (cloned repo `.vscode/settings.json`) sets them; for `httpProxy`, `configuration.md` wrongly says "requests are made directly" when unset (ureq reads proxy env vars).
  7. SSRF guard via DNS resolution — hostname-string check only (`foo.localhost`, `localtest.me`, `127.0.0.1.nip.io` pass; DNS rebinding).
  8. Relative/local schemas (`$schema=./schema.json`, `file://`).
  Follow-ups surfaced during validate-kubernetes-schema-url, not yet put to the user: three existing tests in `tests/lsp_lifecycle/kubernetes_detection.rs` (`should_record_schema_association_for_kubernetes_manifest`, `should_use_configured_kubernetes_version_in_schema_url`, `should_record_kubernetes_schema_association_for_builtin_dotted_group`) start without the FakeProxy and reach raw.githubusercontent.com over the real network; new Kubernetes API groups need an entry in `BUILTIN_DOTTED_GROUPS` (association.rs) before auto-detection resolves them.
  Follow-ups from the 2026-10-28 tolerant-settings plan (reviewer/security Lows, not yet put to the user): `MAX_LOGGED_SETTING_CHARS` lives in `server.rs` but `settings.rs` imports it while `server.rs` imports from `settings.rs` (move the constant into `settings.rs`); `tests/lsp_lifecycle/configuration.rs` repeats the `unknownTag` match inline instead of the `has_code` helper; security Low (pre-existing): `loggable_url` (`server.rs`) cannot strip userinfo from cannot-be-a-base URLs such as a `schemas` key or modeline `user:pw@host`.
  Follow-up from the 2026-10-29 VS Code settings-sync plan (reviewer Low, not yet put to the user): the "after Restart Server" integration test in `rlsp-yaml/integrations/vscode/src/test/integration/settings-sync.test.ts` does not assert the default `key: v` before enabling `formatSingleQuote`, so it cannot tell "reached the server" from "already in effect".
  Follow-ups from the 2026-10-29 SchemaStore upstream-rules plan (not yet put to the user): picomatch syntax beyond `*`/`**` stays literal, so 11 real-catalog patterns using `?` (winget `**/manifests/?/...`) never match; `allOf` over several matching catalog entries (upstream) was declined, first match wins. Test consolidation candidate: `file_patterns_match_what_follows_any_slash` (`src/schema/association.rs`) is a subset of the merged `anywhere_glob_agrees_with_the_reference_model` proptest (same reference model); kept because the plan allowed only one merge.
  Test consolidation candidate from the 2026-10-28 sibling-keys plan: `complete_at_in_sequence_item_suggests_missing_sibling_keys` (C-6, `src/completion.rs`) duplicates `complete_in_sequence_item_suggests_missing_sibling_keys` (`completion_drivers.rs`) with weaker assertions; kept because that plan forbade deleting tests.
  Follow-ups surfaced during share-ref-targets, not yet put to the user: schema-cache eviction across URLs (documents cache grows unbounded); negative cache for rejected/failed schemas (re-fetched + re-parsed on every edit); test consolidation proposals the test-engineer raised in Tasks 2/4/5/6 (duplicate overwrite tests, vacuous `let _ =` tests, duplicate deep-nesting fixtures); `cargo doc -p rlsp-yaml --no-deps` fails under `warnings = "deny"`: public docs link private items — `max_line_length` → `MAX_LINE_LENGTH_CAP` (pre-existing) and, since the 2026-10-28 tolerant-settings plan, `RejectedValue::Withheld` → `URL_SETTINGS` in `src/settings.rs`; CI does not run `cargo doc`.

- **`autoWrapFlowStyle` opt-out for `block_to_flow`** — The code-action user-format-config plan (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`) sets `block_to_flow`'s default to auto-wrap output to fit `formatPrintWidth`, removing the misleading `(long line)` title suffix. If users later report wanting single-line flow output regardless of length (e.g., for tooling that prefers compact JSON-like flow), add a setting like `autoWrapFlowStyle: false` (default `true`) so the action emits unwrapped flow when the user opts out. Defer until evidence shows demand — pre-emptively shipping the setting risks configuration sprawl across all code actions. Bias: do nothing unless a user explicitly asks for it.

- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.

- **Replace `WORKLIST.md` + `SKIP_LIST` with `#[ignore]`** (user preference, 2026-04-18; never planned): the shrink-only skip-list machinery is heavier than needed. Delete `rlsp-yaml/tests/corpus/WORKLIST.md`, remove `SKIP_LIST` and its matching logic from `tests/corpus_invariants/shared.rs` (empty since April), rewrite the top-of-file comment in `tests/corpus_invariants/main.rs` to "no known failures; fix a new one or `#[ignore]` it with a plan reference". The harness is already split into one test per invariant (2026-05-20 plan), so per-test `#[ignore]` works. If the corpus expansion above lands first, its new failures go through `#[ignore]` instead of the Surprise Failure Protocol.
## Open: rlsp-yaml-parser

- **Next plan: comment preservation (user chose 2026-10-30, verified with `format_yaml`).** (L1) Comments before a later `---` or `...` have no place in the AST and the formatter deletes them: `a: v\n# d\n---\nb: 1\n` formats to `a: v\n---\nb: 1\n`. The comment-folding plan makes the indented form lose its comment the same way. Proposed shape: a field on `Document` that the formatter emits before the marker, without double output from the end-of-file text scan. (L2) A comment on the last continuation line is dropped by the lexer: `a:\n  v\n  w # c\n` formats to `a: v w\n`; proposed shape: one helper for the suffix-comment logic now copied in `lexer/plain.rs` (two sites) and `block/sequence.rs`.
- **Parser and span follow-ups from the comment-folding plan, Task 1 (2026-10-30, not yet put to the user).**
  - `{a: b\n  c\n}\n` and `{a: x\n  y, b: z}` report "missing comma"; they look like valid YAML, so a multi-line plain value in a flow mapping appears to fail.
  - `[a\n\n  b]` loads as `"a b"`; a blank line in a flow plain scalar should fold to a newline (`"a\nb"`). Not checked against the suite.
  - Multi-line quoted scalars end their span after the line break, and their value tokens in `classify_scalar_node` have a wrong length.
  - Block-scalar code action: on `d: aaa…\n  bbbb # c\nother: 1\n` the comment and trailing spaces become part of the literal value, which changes the data; the single-line path has the same defect.
  - Test consolidation candidate in `rlsp-yaml-parser/tests/loader_spans.rs`: `document_line_index_consistent_with_span_end_offset_multiline` duplicates `block_mapping_span_end_is_last_value_line`; `block_mapping_span_covers_full_extent` and `block_mapping_span_start_is_first_key_line` could merge.
- **Deviations from libfyaml 0.9.6, present in the last release (put to the user 2026-10-30 at the completion of the comment-folding plan; decision pending, per `rlsp-yaml-parser/CLAUDE.md`).**
  - Silent data change on valid input: `a:\n: c\n`, `a: 1\n[x]:\n: v\n`, `a: &y\n&k : v\n` make the value belong to the previous key (libfyaml: a new empty key).
  - `parse_events` omits the empty scalar in an empty document (`---\n---\n`, 14 yaml-test-suite cases expect `=VAL :`; the stream conformance test only checks ok/error), and a properties-only document before `---` loses its property and node.
  - `a: [x]: 2\n`, `a: *y : 2\n`, `a: &y\n[x]\nb: 1\n`, `a:\nw\n` (also `a:\n[x]\n`, which libfyaml accepts too), `&k\n*a\n`, `&a\n&b [x]\n`, `&x\n!!str\n&x v\n`, `? ? a\n[x] :\n`, `?\n!!str\nb: 1\n`, `[x]:2\n`, and `["q" "r"]` load; `a: 1\n? b: 2\n: c\n`, `a: !!str\n!!seq [x]: 2\n`, and `[x]\t: 2\n` error though valid.
  - `--- *a\n` emits the scalar `*a`; a property line before a flow key goes on the key, not the new mapping (`&y\n[x]: 2\n`); `{a: b\n  c\n}\n` and `[a\n\n  b]\n`; a closing `]` at the key's column loads after a quoted item and errors after a plain one; BOM in a plain scalar errors here and loads in libfyaml.
  - Settled, libfyaml is the wrong side; the user decided not to report them upstream: `- &x\n[x]\n` (too lenient, [197]), `k:\n-\n` ([201]), `a: !!str\n  |\n  t\n` ([199], a libfyaml 0.9.6 regression from 0.8).
  - Found at the last task, also present in the release: `-\n  # d\n` drops the empty item; `a: |\n  t\n  # d\n` formats with a second `# d`, not idempotent; `-\n# c\n` and `- -\n  # d\n` format non-idempotently; `-\n# d\nw\n` errors and `a: ?\n` loads (not settled against the spec).
  - Other: events after the `- a\0\n- b\n` error; `x:\n- &k\ny: 1\n` leaks the anchor to the key.
- **`--- v` inline scalar loses content (verified 2026-10-30, not yet put to the user as a plan).** `--- v\n# d\nw\n` emits the comment before `v`, then `w` is emitted and dropped; it formats to `---\nv\n`.
- **Flow sequence drops entries after a comment (verified 2026-10-29; user chose its own plan after the comment-preservation plan, before the alias-expansion depth plan).** `[a # c\n  b]` and `[a\n  # c\n  b]` load as `[a]`, and `[a, b\n  # c\n  c]` as `[a, b]`, with no error. A missing comma must be an error; flow mappings already report "missing comma between flow mapping entries".

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
