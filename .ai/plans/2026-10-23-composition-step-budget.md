**Repository:** root
**Status:** InProgress
**Created:** 2026-10-23

# Evaluate Every Composition Branch Within a Step Budget

## Goal

Schema validation and completion look at only the first 20
`allOf`/`anyOf`/`oneOf` branches of a schema node, so real
schemas lose diagnostics (cloud-init has 57 `allOf`
branches; `package_update: 5` in branch 25 goes unflagged),
`anyOf`/`oneOf` report false errors when the matching branch
comes later, and completion misses keys. Evaluate every
branch, bound validation by a shared step
budget proportional to the document's size, and tell the
user in the LSP log when validation stopped at that budget.

## Context

- `MAX_BRANCH_COUNT = 20` exists twice:
  `rlsp-yaml/src/schema_validation/support.rs` (validation,
  including the `unevaluatedProperties`/`unevaluatedItems`
  helpers `collect_evaluated_properties` and
  `collect_evaluated_item_count`) and
  `rlsp-yaml/src/completion/support.rs` (completion walks in
  `rlsp-yaml/src/completion/schema_completions.rs`, where it
  also serves as a recursion depth cap).
- Composition is validated by probes: `anyOf`, `oneOf`,
  `not`, and `if` in
  `rlsp-yaml/src/schema_validation/composition.rs` and
  `contains` in `array_constraints.rs` validate into a
  scratch context and read only whether it stayed empty.
  Nested probes multiply — the per-node cap does not bound
  that work; only `MAX_VALIDATION_DEPTH = 64` does.
- `validate_schema` in `rlsp-yaml/src/schema_validation.rs`
  is the validation entry point. Its production caller is
  `Backend::process_schema` in `rlsp-yaml/src/server.rs`,
  which already logs schema failures through
  `schema_failure_message` (control/bidi character
  replacement, `loggable_url`, length caps).
- Test fixtures — vendored copies of real, self-contained
  schemas (no remote `$ref`), each paired with real-world
  documents:

  | Schema | Source (pinned) | Licence | Documents |
  |---|---|---|---|
  | cloud-init | https://raw.githubusercontent.com/canonical/cloud-init/d5873d2e693cb3e83d7e50c97da3b8586faf8c2e/cloudinit/config/schemas/schema-cloud-config-v1.json | GPL-3.0 or Apache-2.0 (used under Apache-2.0) | a cloud-config document written for the test |
  | GitHub workflow | https://raw.githubusercontent.com/SchemaStore/schemastore/57967c6f6d744228bdcd0500703d345bb8334574/src/schemas/json/github-workflow.json | Apache-2.0 | `tests/corpus/github-actions-matrix.yml`, `tests/corpus/release-plz-workflow.yml` |
  | Compose spec | https://raw.githubusercontent.com/compose-spec/compose-go/52eea9f323cbdb2bf9c2092513615771faec4690/schema/compose-spec.json | Apache-2.0 | `tests/corpus/docker-compose.yml` |
  | Kubernetes Deployment | https://raw.githubusercontent.com/yannh/kubernetes-json-schema/f7d855e49c34b039c9ef2196444910275e31e09e/master-standalone-strict/deployment-apps-v1.json | Apache-2.0 | `tests/corpus/kubernetes-deployment.yaml` |

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: one probe helper that stops at the first
      diagnostic
- [x] Task 2: validate every branch within a step budget
- [x] Task 3: log when validation stopped at the budget
- [ ] Task 4: complete keys from every branch through one
      property walk
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: One probe helper that stops at the first diagnostic

The five probe sites build their scratch context the same
way; one helper replaces them, so the budget and the
undecided outcome of later tasks land in one place. A probe
is read only as empty or non-empty, so it can stop at its
first diagnostic — that cuts the work of failing branches,
which grows once every branch is evaluated. Behavior is
unchanged.

- [x] The five probe sites use one shared probe helper; none
      builds its own scratch context
- [x] A probe stops validating once it has recorded a
      diagnostic; validation outside probes still records
      every diagnostic
- [x] These tests pass with their assertions unchanged: the
      `anyOf`, `oneOf`, `not`, and `if`/`then`/`else` tests
      in `composition.rs`, the `contains` tests in
      `array_constraints.rs`, and
      `content_schema_nested_sub_schema_uses_full_validation`
      in `scalar_constraints.rs`
- [x] Build, tests, clippy, and fmt pass

### Task 2: Validate every branch within a step budget

Validation evaluates every composition branch. One step is
one entry into `validate_node`, plus one per schema node the
`collect_evaluated_*` helpers visit; operations whose work
grows with their input — `uniqueItems`, the
`dependentSchemas` clone, `contentSchema`/`contentEncoding`,
`enum` and `required`/`dependentRequired` scans, regex
compilation and matching, mapping and sequence width, and
the length of scalar values — charge steps in proportion to
that work. The
budget is shared by everything a single `validate_schema`
run does, so nested probes cannot multiply past it.

- [x] Validation, including the `collect_evaluated_*`
      helpers, evaluates every `allOf`/`anyOf`/`oneOf`
      branch; no branch-count limit remains in
      `schema_validation`
- [x] One step budget per `validate_schema` run is shared by
      all documents, probes, `contentSchema` validation, and
      the `collect_evaluated_*` helpers; its size is a fixed
      floor plus a fixed number of steps per YAML node and
      per byte of scalar content in the run's documents,
      capped at a fixed ceiling that does not depend on
      document size
- [x] The operations listed above charge steps in
      proportion to their work, and their results are
      unchanged
- [x] The steps a run uses depend only on the schema and the
      run's documents, never on what a cache holds — a
      schema pattern's compilation is charged once per run
      for each distinct pattern, whether or not it is
      cached, and its matching on every use
- [x] Each distinct `format: regex` value is compiled at
      most once per run; every use is still charged, so the
      step total is the same as without reuse, and results
      are unchanged
- [x] Each compile attempt for a `format: regex` value or a
      schema pattern (`pattern`, `patternProperties`) is
      charged no fewer steps than the worst-case cost of an
      attempt at that attempt's compile size limit, and the
      security-engineer's sign-off lists no regex
      compilation residual risk
- [x] A run stops once the diagnostics it returns, across
      all of its documents, reach a fixed maximum
- [x] `validate_schema` returns its diagnostics together
      with whether validation completed or stopped early
      (budget or diagnostic limit)
- [x] After the budget is exhausted, no diagnostic depends
      on a result the budget cut short: `not`, `anyOf`,
      `oneOf`, `contains`/`minContains`/`maxContains`
      report nothing from an undecided probe, an undecided
      `if` applies neither `then` nor `else`, and
      `unevaluatedProperties`/`unevaluatedItems` report
      nothing for a node whose evaluated properties or
      items were not fully collected; diagnostics recorded
      before exhaustion are kept
- [x] The four schemas in Context are vendored under
      `rlsp-yaml/tests/fixtures/schemas/` from their pinned
      sources, with each one's source URL and licence
      recorded next to it, and the directory is excluded
      from the published crate package
- [x] With one tenth of the production budget, each fixture
      schema validates each of its documents to completion,
      and at least one fixture document needs at least one
      hundredth of its production budget
- [x] A cloud-config document containing `package_update: 5`
      receives a type diagnostic for `package_update`
- [x] An `anyOf` and a `oneOf` whose only matching branch is
      the 25th produce no diagnostic
- [x] A deeply nested hostile composition schema built in a
      test, carrying `unevaluatedProperties: false` and
      `unevaluatedItems: false`, stops at the budget, and
      its result contains none of the diagnostics excluded
      above
- [x] A test schema that repeats an expensive operation
      (e.g. `uniqueItems` over a large array) across many
      branches stops at the budget, and a test document that
      produces more diagnostics than the limit stops at the
      diagnostic limit
- [x] `any_of_message_branch_count_capped_at_max_branch_count`
      in `composition.rs` asserts the full branch count, and
      `should_complete_for_all_of_with_many_branches` in
      `schema_validation.rs` asserts a diagnostic for every
      unsatisfied branch
- [x] An integration test in `rlsp-yaml/tests/lsp_lifecycle/`
      seeds a schema whose `allOf` has more than 20 branches
      (via `seed_schema_cache` and a modeline) and asserts
      that a diagnostic from a branch beyond the 20th is
      published
- [x] A Criterion bench case validates a document against a
      schema with a wide `anyOf`/`oneOf`
- [x] Build, tests, clippy, and fmt pass

### Task 3: Log when validation stopped at the budget

When a validation run stops early — at the budget or at the
diagnostic limit — the user learns from the LSP log that the
diagnostics are incomplete, for every schema source. Like
schema fetch failures, it is reported on every run that
stops.

- [x] When validation stops early, the client
      receives a `window/logMessage` of type Warning that
      names the schema URL, sanitized the same way as
      schema failure messages, and states that validation
      is incomplete
- [x] A run that completes sends no such Warning
- [x] An integration test in `rlsp-yaml/tests/lsp_lifecycle/`
      seeds a hostile composition schema, collects
      client-bound messages, and asserts the Warning
- [x] `rlsp-yaml/docs/configuration.md` tells the reader
      that schema validation work and the number of
      diagnostics are bounded and what they see when a bound
      is reached
- [x] `rlsp-yaml/docs/feature-log.md` records that every
      composition branch is evaluated and that a Warning
      reports validation stopped early (budget or diagnostic
      limit)
- [x] Build, tests, clippy, and fmt pass

### Task 4: Complete keys from every branch through one property walk

Completion offers keys from every composition branch.
`schema_has_properties`, `collect_schema_properties`, and
`collect_schema_properties_keys_inner` answer the same
question — which properties a schema offers, directly or
through composition — and become one walk. Completion walks
only the schema, so its cost is linear in the parsed schema
tree and it needs no budget.

- [ ] The three walks share one property walk; no
      branch-count or depth limit remains in `completion`
- [ ] Key completion offers properties from composition
      branches beyond the 20th;
      `should_cap_allof_branch_walking_at_max_branch_count`
      asserts that, and
      `should_not_panic_when_allof_depth_exceeds_max_branch_count`
      carries a name that no longer refers to the removed
      limit
- [ ] Completion items keep their order, and the
      `MAX_COMPLETION_ITEMS` cut is unchanged
- [ ] Apart from the two tests above, the existing tests of
      these walks and of key completion pass with their
      assertions unchanged — among them the
      `schema_has_properties_*` and
      `collect_schema_properties_keys_*` tests and the
      `(all required)` snippet tests in
      `schema_completions.rs`
- [ ] An integration test in `rlsp-yaml/tests/lsp_lifecycle/`
      sends completion requests for a document whose seeded
      schema has more than 20 `allOf` branches and asserts
      that a key defined only beyond the 20th branch is
      offered, both at the top level and under a nested key
      whose schema is reachable only through a branch beyond
      the 20th
- [ ] The feature-log entry for this change also covers
      completion
- [ ] Build, tests, clippy, and fmt pass

## Non-Goals

- Bounding schema size at parse time — `$ref` targets are
  copied inline and only depth is limited (separate plan
  after this one)
- Hover documentation for array items and nested
  composition (separate plan after this one)
- Deduplicating completion items or changing which items
  survive the `MAX_COMPLETION_ITEMS` cut
- The `contentSchema` context's hard-coded format
  validation and YAML version
- Bounding the regex cache and running validation off the
  async executor (separate plan after this one)

## Decisions

- **Budget instead of a cap:** raising the cap still
  truncates large schemas silently and leaves nesting
  unbounded; removing it without replacement leaves hostile
  nested schemas unbounded.
- **Budget proportional to document size:** a fixed per-run
  budget can be used up by a large valid file (many
  manifests in one file); floor plus steps per node bounds
  schema-driven multiplication per node and keeps total
  work linear in document size.
- **One budget shared by probes:** a fresh budget per probe
  would leave nested `anyOf` unbounded.
- **Steps weighted by work:** with one step per node visit,
  a hostile schema could repeat an expensive operation
  across many branches and the budget would not bound CPU
  time; only the accounting changes, not the algorithms —
  except for regex compilation, whose cost a fixed charge
  cannot cover: `format: regex` results are reused within a
  run, and every compile attempt — `format: regex` values
  and schema patterns — is charged for the worst case at
  its size limit (Task 2), so a hostile schema cannot
  stretch one validation to tens of seconds. A flat
  worst-case charge per use would stop ordinary documents;
  a tighter compile limit would change results.
- **Budget ceiling and diagnostic limit:** the ceiling keeps
  a huge document from inflating its own budget; the
  diagnostic limit keeps a run from building an unbounded
  `publishDiagnostics` payload now that every branch can
  report.
- **No length or depth limit for embedded content:**
  `contentSchema` content stays validated at any length,
  with its own depth count; its work is bounded by the
  weighted steps.
- **An undecided probe produces no diagnostic:** an
  exhausted probe leaves its scratch empty and would read
  as "matches", inventing `not`, `oneOf`, `contains`, and
  `if`/`then` errors; exhaustion may only drop diagnostics.
- **Signal in the log, not the document:** a Warning
  matches the schema failure warnings; a document
  diagnostic would sit permanently in the problems panel,
  and silence would hide incomplete results.
- **Budget Warning also for Kubernetes auto-detection:**
  schema fetch failures stay silent for that source because
  its URL is a guess; here the schema loaded, so the URL
  was right.
- **Single entry point:** `validate_schema` returns the
  completeness together with the diagnostics; a wrapper
  returning only diagnostics would let a future caller lose
  the signal.
- **Completion without a budget:** completion and hover
  walk only the schema, never schema × document; their cost
  is bounded by the parsed schema, whose depth parsing
  already limits.
- **Unified completion walk lands with the cap removal,
  not as a task ahead of it:** the three walks carry
  different limits today (branch and depth caps, depth cap
  only, none), so one walk cannot keep all three behaviors
  before the limits go.
- **Fixtures excluded from the crate package:** about
  1.1 MB of test-only data.
- **No CloudFormation fixture:** its schema (7.6 MB)
  exceeds `MAX_SCHEMA_BYTES`, so the server never loads it.
