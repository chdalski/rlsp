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

- **Port `rename` to fixtures** — Symmetric companion to the code-action fixture port (plan `2026-04-27-code-action-fixture-tests.md`, completed). The user's scoping decision was: fixtures only for user-triggered document mutations where the input → expected-output shape is well-suited; rename qualifies. `rlsp-yaml/src/navigation/rename.rs` has ~68 inline tests; the same Pattern A/B/C taxonomy applies. The fixture format and harness in `tests/code_action_fixtures.rs` could be reused or duplicated (TBD during planning). Apply the same "visually self-explanatory" gate documented in `tests/fixtures/CLAUDE.md`. Per-test pre-scan for non-transformation assertions (kind, range structure, multi-edit count, exact title) is mandatory before porting — this surfaces Pattern C upfront.

<!-- helper-of: convention — an allow-list entry marked `HelperOf` exists because its root
     feature function is also allow-listed as a `TodoRetrofit`. When the root's retrofit plan
     lands, all `HelperOf` entries pointing at that root are removed from the allow-list at the
     same time; they are NOT independent retrofit items and do NOT need their own follow-up
     plans. -->

<!-- Audit-v2 feature-level retrofits — 12 public feature entry points that hand-roll YAML
     scanning instead of consuming the parser AST. Each item below has its signature, violation
     shape, replacement sketch, and the private helpers retired when the root retrofit lands.
     These were surfaced and allow-listed in commit c70f642 under
     .ai/plans/2026-04-18-parser-boundary-audit-v2.md Task 1. -->

- **`string_to_block_scalar` doubles anchors in output** — When converting `description: &myanchor "long string"` to a block scalar, `format_subtree` re-emits `&myanchor` in the edit's `new_text` while the edit's `range.start` lands after the source `&myanchor ` prefix (the parser's scalar `loc` excludes the anchor). Result: `description: &myanchor &myanchor |\n  long string`. Surfaced during the code-action fixture port (commit `94d5cfc`) — the original inline test asserted `result.contains("&myanchor")` which was satisfied by either one or two occurrences, so the doubling shipped under a loose assertion. The fixture `tests/fixtures/code_actions/block-scalar-preserve-anchor.md` currently codifies the doubled output as expected; once fixed, that fixture's `Expected-Document` updates to a single `&myanchor`. Fix the production code path; do not loosen the fixture format to mask this.

- **`block_to_flow` policy enforcement against `formatEnforceBlockStyle`** — When the user sets `formatEnforceBlockStyle: true`, the workspace formatter rewrites all flow-style collections back to block style on save. The `block_to_flow` code action contradicts this policy: applying it produces flow output that the next save reverts. Open question: should `block_to_flow` be suppressed (not offered) when `formatEnforceBlockStyle: true`, or should it remain offered with a UI hint, or is the conflict acceptable as user-driven? Surfaced during the code-action user-format-config plan audit (`.ai/plans/2026-04-27-code-action-respect-user-format-config.md`, in Decisions/Non-Goals). Separate plan needed — the question is policy/UX, not a correctness bug. Related but distinct: are there other code actions whose existence violates user formatter policy when settings are non-default?

- **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.
- **Non-printable unicode character diagnostic** — Parser's comment lexer (`lexer/comment.rs`) and content scanning don't validate characters against `is_c_printable` (YAML 1.2 §5.1). Non-printable/control characters pass through silently. Add LSP diagnostic (`invalidCharacter`, Warning severity) for non-printable characters in comments and content. Security concern: invisible/homoglyph characters could hide malicious content. Parser should preserve them (no data loss); diagnostics should flag them.
- **Expand `block_to_flow` code action to support nested block structures** — The action currently refuses nested inputs via `return None` in `code_actions.rs:420`. The `2026-04-18-retrofit-block-to-flow-code-action.md` plan preserved this narrow behavior to keep scope minimal (bug-class elimination, not feature expansion). After the retrofit lands, the AST+formatter path handles nesting automatically — lifting the restriction is cheap. Enhancement plan: remove the pre-check, add tests for nested block-to-flow conversions, confirm the formatter produces correct flow output (e.g., `{a: {b: 1}}`, `[[1, 2], [3, 4]]`).

- **Offer folded block scalar (`>`) as an alternative output form for the `string_to_block_scalar` code action** — The `2026-04-18-retrofit-string-to-block-scalar-code-action.md` plan converts strings to `ScalarStyle::Literal` (`|`) only, preserving the current behavior. Literal preserves newlines verbatim; folded collapses line breaks into spaces (better for prose). After the retrofit lands, offering `>` as a SECOND quick-fix alongside "Convert to block scalar (literal)" is a small UI enhancement: add a `Node::Scalar` clone path targeting `ScalarStyle::Folded`, emit a separate `CodeAction` titled "Convert to folded block scalar". User picks between literal and folded at apply time.
- **Expand `string_to_block_scalar` code action to sequence-item scalars** — The `2026-04-18-retrofit-string-to-block-scalar-code-action.md` plan preserved the current mapping-values-only dispatch (AST walk only looks at `Node::Mapping.entries` values, skipping `Node::Sequence.items`). Symmetric with the `block_to_flow` nested-support enhancement above. A long string like `- "this is a long sequence-item string"` could benefit from block-scalar form too. Enhancement plan: extend the AST walk to also match qualifying `Node::Scalar` values inside `Node::Sequence.items`, add regression tests for sequence-item conversion, verify the formatter produces correct `- |\n  content` output.
- **I5 corpus invariant: validator stability under whitespace re-emit** — Deferred in Move 0 (`.ai/plans/2026-04-18-corpus-invariants-scaffold.md`). For each corpus file and each validator, run the validator on the original text AND on a whitespace-only re-emit of the same document; assert the set of diagnostic codes is identical (ranges may shift). Catches validators whose output depends on whitespace quirks rather than structure.
- **I6 corpus invariant: formatter round-trip** — Also deferred in Move 0. For each corpus file: format the input, parse the formatted output, assert the resulting AST is semantically equivalent to the input's AST (same scalars at the same logical paths, same structure). Catches formatter bugs that produce non-round-tripping output. Already partially covered by the formatter's own fixture tests, but extending to the full corpus closes the "works on fixtures but fails on real files" gap.
- **Expand corpus beyond the 4 seed files** — Move 0 seeded the corpus with `release-plz-workflow.yml`, `kubernetes-deployment.yaml`, `docker-compose.yml`, `github-actions-matrix.yml`. Real-world YAML covers many more shapes: Ansible playbooks, Helm chart templates, GitLab CI pipelines, CloudFormation/CDK YAML, Prometheus alert rules, SOPS-encrypted files, Swagger/OpenAPI specs, Argo CD `Application` manifests, Flux CD `Kustomization`s, Tekton `Pipeline`/`Task` resources. Each adds new coverage. File as one plan per shape, or a batch-add plan for 3-5 at a time. Each new file may surface new I4 failures that flag latent bugs — treat those under the Surprise Failure Protocol.
## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)
