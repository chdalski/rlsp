# rlsp-yaml architectural program — session brief

Created: 2026-04-18. Purpose: preserve context for a
future session (or post-compaction continuation) to
pick up the work without re-deriving everything.

## The originating bug

The user reported that `rlsp-yaml` emits a "Flow mapping
style: use block style instead" warning on
`GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}` in their own
`.github/workflows/release-plz.yml`, and that applying
the "convert to block style" quick fix destroys the
mapping (replaces the `env:` value with a garbled line
like `                           { secrets.GITHUB_TOKEN }`,
losing the key entirely).

Diagnosis:

- **Bug 1 (false diagnostic).** `rlsp-yaml/src/validation/validators.rs:197-265`
  (`validate_flow_style`) does text-based scanning for
  `{`/`[` outside of quotes with no YAML-context
  awareness. Per YAML 1.2 §7.3.3, plain scalars in block
  context may contain `{`/`}` as content. `${{ … }}`
  starts with `$`, not `{`, so the whole expression is a
  single plain scalar. The text scanner misreads it. A
  trace showed the outer loop even emits TWO overlapping
  diagnostics per expression (one for each `{`), so noise
  is 2N in a file with N expressions.
- **Bug 2 (destructive quick fix).** `rlsp-yaml/src/editing/code_actions.rs:104-186`
  (`flow_map_to_block`) blindly trusts the diagnostic
  range. For `${{ … }}` the key-detection check
  (`prefix.trim_end().ends_with(':')`) fails because the
  prefix ends with `$`, so it falls into the standalone
  else-branch and replaces the entire line (cols 0 to
  end) without reconstructing the key. Independent of
  Bug 1, the function also has latent defects: full-line
  replace (breaks for flow maps sharing a line with
  other content), single-line scope only (multi-line
  flow maps unhandled), and comma-based item splitting
  (only OK because `split_flow_items` tracks depth and
  quotes — that piece is fine).
- **Parser is correct.** `rlsp-yaml-parser` produces
  correct events/AST for `${{ … }}` — a plain-scalar
  `Scalar` node. The loader at `loader.rs:513-527` sets
  `loc` on flow collections to cover `{...}` / `[...]`
  inclusive. The AST already carries the
  `style: CollectionStyle::{Block, Flow}` field added in
  commit 728d182 under plan
  `.ai/plans/2026-04-13-flow-style-preservation-and-enforcement.md`
  Task 1.

## The deeper framing the user pivoted to

The user escalated past "just fix the bug" to the
architectural concern: they want confidence that
rlsp-yaml as a whole follows YAML spec rules and is
conformant with yaml-test-suite as a minimum. Existing
safety nets (parser conformance against yaml-test-suite,
formatter fixture files, round-trip tests) protect the
parser and the formatter but do NOT cover validators,
code actions, or other LSP features. Those features can
reach around the parser and re-implement YAML lexing by
hand, which is exactly how Bugs 1 and 2 were born.

## The architectural program

Agreed with the user, sequenced 0 → 1 → 2 → 3, one
plan at a time. The user asked to re-sequence so the
invariant harness (a seed of Move 3) lands FIRST, giving
subsequent retrofits a TDD-style acceptance signal over
real YAML rather than only synthetic unit tests. This
prevents the "retrofit passes its own tests but breaks
something in a real file" failure mode that motivated
the original bug report.

- **Move 0 — invariant harness + seed corpus.** A
  TDD-style scaffold: build a small test harness that
  runs broad invariants (no panics, diagnostic range
  validity, code-action output parses) over a seed
  corpus of 3-5 real-world YAML files (our own
  release-plz workflow, a K8s Deployment, a compose
  file, a GHA matrix workflow). Currently-failing
  (file, invariant) pairs land on a shrink-only
  skip-list where every entry references a concrete
  filed follow-up plan by path (the Surprise Failure
  Protocol gates new entries; ad-hoc markers are
  forbidden). Produces a `WORKLIST.md` that drives the
  subsequent plans. No production-code changes.
- **Move 1 — AST-first as a crate invariant.** State in
  root CLAUDE.md: no code in `rlsp-yaml/` may re-parse
  YAML structure from `&str`; LSP features consume the
  parser AST. Retrofit `validate_flow_style` as the
  demonstration. Add a mechanical audit `#[test]` that
  fails when new `text: &str` validators/code actions
  are introduced. Gate release-plz on CI so the audit
  is a release gate. Acceptance now references specific
  Move 0 skip-list entries that must be removed.
- **Move 2 — fixture pattern for every LSP feature.**
  Extend the formatter's human-readable markdown
  fixture pattern to diagnostics, code actions, hover,
  completion, etc. Each bug becomes a fixture before a
  fix; fixtures are forever regression coverage.
- **Move 3 — grow Move 0's corpus + invariants over
  time.** Add more real YAML (Ansible, exotic 1.2
  documents), more invariants (refactor AST equivalence,
  validator stability under reformat, formatter
  round-trip), and widen the coverage. Ongoing, not a
  single-plan deliverable.

Each move is its own plan (or multiple plans). The
four moves answer four distinct questions:

- *Does the LSP survive real-world YAML at all?* →
  Move 0
- *Are we using the parser?* → Move 1
- *Does this specific input produce the right output?* →
  Move 2
- *Does the feature hold up across a growing real-world
  corpus?* → Move 3

## Move 0 status — Completed 2026-04-18

Plan file: `.ai/plans/2026-04-18-corpus-invariants-scaffold.md`

All five tasks landed. Key outcomes:

- `rlsp-yaml/tests/corpus_invariants.rs` — test harness
  with I1 (no panics), I2 (diagnostic range validity),
  I3 (code-action round-trip: post-edit still parses
  without new Error diagnostics), I4 (refactor code
  actions preserve scalar content — added after Task 3
  exposed that I3 couldn't catch
  semantically-destructive-but-parseable edits).
- `rlsp-yaml/tests/corpus/` — 4 real-world seed files
  (release-plz-workflow.yml, kubernetes-deployment.yaml,
  docker-compose.yml, github-actions-matrix.yml).
- `rlsp-yaml/tests/corpus/WORKLIST.md` — human-readable
  mirror of the `SKIP_LIST` constant.
- Current skip-list has two entries, both on I4, both
  citing `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`:
  `(github-actions-matrix.yml, I4)` and
  `(release-plz-workflow.yml, I4)` — both surface the
  `flow_map_to_block` destructive edit that drops
  scalar content.
- 47 corpus_invariants tests total (harness-internal,
  range-validity helpers, scalar-collection helpers,
  integration probes). Full workspace suite green.

Commits: `678b16e`, `9074b51`, `f9e28ae`, `c9210a1`,
`1b9ffbb`, plus status/SHA-correction doc commits.

Notable insight from Task 3 → Task 4: I3 as originally
framed ("no new Error diagnostics after applying the
edit") is too narrow to catch destructive-but-parseable
code actions. The destructive `flow_map_to_block` output
on `${{ … }}` produces valid YAML (flow mapping under
`env:`), so I3 passes even though data is lost. I4
closes that gap via multiset-subset scalar preservation.
Future retrofit plans should be defensive about this:
"invariant passes" does not imply "no bug."

## Move 1 status — Completed 2026-04-18

Plan file: `.ai/plans/2026-04-18-one-parser-one-ast.md`

All five tasks landed. Commits:
- Task 1 (`d06fff8`): "one parser, one AST" rule added
  to root CLAUDE.md
- Task 2 (`9c5a6e1`): `validate_flow_style` retrofitted
  to `(docs: &[Document<Span>])`; 14 new unit tests;
  `find_closing_char` deleted; Move 0 skip-list I4
  entries became stale and were removed during the
  retrofit cleanup
- Task 3 (`9f40e88`): GHA-expression regression
  coverage (unit test + `GHA_RELEASE_PLZ_STYLE`
  ecosystem fixture); regression-catch verified
- Task 4 (`e58fdf0`): `parser_boundary_audit.rs`
  with shrink-only allow-list, 5 inventoried violators
  (the 4 from plan + `validate_schema` discovered
  during implementation), per-entry verification done
- Task 5 (`189e9eb`): `release-plz.yml` gated on CI
  via `workflow_run`; checks out validated `head_sha`;
  action versions refreshed; top-of-file comment
  documents the gate

User-reported bug closure: GHA `${{ … }}` expressions
no longer trip `flowMap`/`flowSeq` diagnostics; the
double-report bug is gone; multi-line flow collections
are now correctly detected.

Operational findings during execution:
1. I3 (code-action round-trip) passed on the current
   corpus after the retrofit because the destructive
   `flow_map_to_block` output happens to be
   syntactically valid YAML — I4 (added in Move 0
   mid-flight) is what catches the actual data-loss bug.
2. After the retrofit, I4 STILL flags
   `flow_map_to_block` on legitimate `- { ... }` flow
   mappings inside sequence items, because Task 2's
   `end_col + 1` range adjustment (needed to match the
   parser's zero-width `MappingEnd` span) makes the
   code action reach those flow maps. Skip-list entries
   correctly remain until the destructive-code-action
   fix lands.
3. Extra violator discovered during Task 4:
   `validate_schema` in `schema_validation.rs` — not
   in the original inventory. Added to allow-list with
   a TODO marker.

Scope decisions (confirmed with the user):

- **1a** — Only `validate_flow_style` retrofitted in
  this plan. Other violators become follow-up plans.
- **Multi-line flow collections are flagged** (natural
  consequence of the AST walk; fixes a latent false
  negative of the text scanner).
- **Audit as `#[test]`** in `rlsp-yaml/tests/parser_boundary_audit.rs`.
  No new CI surface; runs with `cargo test`.
- **CI gate via `workflow_run` trigger** on
  `release-plz.yml`. No push trigger; conditional
  `github.event.workflow_run.conclusion == 'success'` on
  every job; `ref: ${{ github.event.workflow_run.head_sha }}`
  on checkout so release operates on the validated SHA.
- **Allow-list is shrink-only.** Entries are removed as
  violators get retrofitted; new entries are only
  permitted for genuine carve-outs (modeline, BOM,
  whitespace-preserving edits) with `// carve-out:`
  justification.
- **Per-entry audit verification required.** For each
  allow-listed function, temp-remove the entry, run the
  audit, confirm it fails citing that function, restore.
  Recorded in commit message. Protects against
  dead-weight allow-list entries from regex misses.
- **Destructive `flow_map_to_block` quick fix NOT fixed
  in this plan.** After Task 2 it is unreachable on
  `${{ … }}` input, so the user-visible destruction
  stops. Its latent defects become a Move-1 follow-up
  plan before Move 2.

Move 1 tasks (summary):

1. Add "One parser, one AST" rule to root CLAUDE.md
   (doc-only).
2. Retrofit `validate_flow_style(docs: &[Document<Span>])`
   — AST walk, use `node.loc` for ranges, keep diagnostic
   codes/severity/message identical. Ripple: `server.rs:483`,
   `benches/hot_path.rs:43`, `benches/insight.rs:43`,
   `tests/ecosystem_fixtures.rs:26, 247, 275`, unit tests
   at `validators.rs:1028-1113`. Update
   `rlsp-yaml/docs/configuration.md` (flowStyle entry —
   note multi-line now detected) and `docs/feature-log.md`
   (record behavior changes).
3. Regression fixture: unit test `flow_style_ignores_github_actions_expressions`
   plus a GHA-workflow ecosystem fixture. Must fail
   pre-Task-2, pass after.
4. Boundary-audit test `rlsp-yaml/tests/parser_boundary_audit.rs`
   with shrink-only allow-list and per-entry coverage
   verification.
5. CI gate: `release-plz.yml` → `workflow_run` trigger,
   per-job success conditional, checkout at validated
   SHA, action versions refreshed. Acceptance is
   diff-verifiable; manual post-merge verification is
   supplementary.

## Follow-up queue (in dependency order)

Each is a separate plan, filed as needed. Order below
reflects sequencing as of the destructive-code-action
fix's completion (2026-04-18).

1. ✅ **Destructive `flow_map_to_block` /
   `flow_seq_to_block` fix.** Completed 2026-04-18
   under
   `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`.
   Three tasks landed: `format_subtree` public API
   (`8dfe0e0`), AST+formatter rewrite + SKIP_LIST
   cleanup (`957c80f`), audit regex tightened to
   first-parameter-only + helper retirement + docs
   (`76dbf5c`). Audit allow-list down to 4 entries
   (the validators). Corpus SKIP_LIST empty.
2. ✅ **Retrofit `block_to_flow` via AST + format_subtree.**
   Completed 2026-04-18 under
   `.ai/plans/2026-04-18-retrofit-block-to-flow-code-action.md`.
   Two tasks: AST rewrite (`173f838`) + cleanup
   (`b752319`). Bonus fixes: anchor-duplication bug
   in edit_start_col + missing end-line branch in
   apply_block_to_flow_edit. Retired `quote_flow_item`.
   Preserves refuse-nested behavior; nested-support
   lifting is queued in `project_followup_plans.md`.
3. **Retrofit remaining code actions to AST+formatter.**
   Queued as individual items in
   `project_followup_plans.md`:
   `quoted_bool_to_unquoted`, `yaml11_bool_actions`,
   `yaml11_octal_actions`,
   `schema_yaml11_bool_type_actions`,
   `delete_unused_anchor`, `string_to_block_scalar`.
   All are low-complexity scalar-style or
   node-property changes that the `format_subtree`
   API handles uniformly. `tab_to_spaces` explicitly
   stays as text (pre-parse lexical concern —
   whitespace normalization, not AST data).
   Rationale for one-plan-per-action: each has
   distinct edge cases (bool quoting semantics,
   octal numeric semantics, block-scalar
   indentation, anchor semantics). Incremental
   landing is lower per-plan risk than a bundled
   sweep.
4. **Retrofit `validate_unused_anchors(text: &str)`** at
   `validators.rs:29`. Pure text-scan; similar shape to
   flow-style retrofit. Low risk. Shrinks audit
   allow-list by one.
5. **Retrofit `validate_custom_tags(text, docs, …)`** at
   `validators.rs:297`. Hybrid → AST-only.
6. **Retrofit `validate_key_ordering(text, docs)`** at
   `validators.rs:475`. Hybrid → AST-only.
7. **Retrofit `validate_schema` (schema_validation.rs).**
   Discovered during Move 1 Task 4 — wasn't in original
   inventory. Unknown current signature shape; treat
   as "investigate + retrofit" scope.
8. **Move 2 — fixture pattern for every LSP feature**
   (see Architectural program). Per-feature narrow
   assertions complement the broad invariants from
   Move 0.
9. **E2E LSP test harness** (previously deferred; see
   "Deferred ideas" section below). Drives tower-lsp
   through JSON-RPC; catches settings and
   serialization bugs the in-process harness misses.
10. **Remove WORKLIST.md AND SKIP_LIST entirely —
   switch to `#[ignore]` for any deferred failures.**
   Queued after the E2E tests plan per user request.
   User preference (confirmed 2026-04-18): the
   WORKLIST.md + SKIP_LIST + Surprise Failure Protocol
   machinery is heavier than needed. `#[ignore]` is
   the natural idiomatic way to mark a test as
   temporarily deferred in Rust; we should use it
   instead of a custom skip-list construct.

   Cleanup scope:
   - Delete `rlsp-yaml/tests/corpus/WORKLIST.md`.
   - Remove the `SKIP_LIST` constant and its matching
     logic from `rlsp-yaml/tests/corpus_invariants.rs`.
     The harness becomes pass/fail only — no skip
     semantics.
   - To retain per-(file, invariant) deferral
     granularity, restructure the harness to emit
     one `#[test]` per (corpus file, invariant) pair
     rather than a single `corpus_invariants` test
     wrapping everything. That way `#[ignore]` can be
     applied to an individual failing pair with a
     descriptive reason comment. Alternatively:
     leave the harness as-is and accept that a single
     failure takes down the whole test — if we truly
     expect zero known failures at steady state, that's
     fine.
   - Update the top-of-file comment in
     `corpus_invariants.rs` — drop the shrink-only /
     Surprise Failure Protocol / mirror-of-WORKLIST
     references. Replace with a short note: "no
     known failures; if a new one appears, fix it or
     `#[ignore]` with a plan reference in the reason
     comment."
   - Move 0 plan's Decisions section: the entries
     "Empty-skip-list state is permanent
     infrastructure", "Corpus WORKLIST.md is a
     human-readable mirror", "Skip-list is shrink-only",
     "Per-entry skip-list verification required", and
     "Surprise Failure Protocol is the gate for any
     new skip-list entry" are all superseded by this
     cleanup plan. Add a single note citing the
     cleanup plan's path.
   - Update `rlsp-yaml/docs/feature-log.md` —
     Move 0's entry referenced WORKLIST.md; adjust.

   Discipline shift: from "shrink-only skip-list
   tracked in data" to "zero-tolerance; deferrals
   marked inline via `#[ignore]` with plan
   references." Matches idiomatic Rust testing.

Each validator retrofit shrinks the audit's
allow-list by one.

## Key technical anchors a resumer should know

- `rlsp-yaml-parser::node::Node::{Mapping, Sequence}`
  exposes `style: CollectionStyle` and `loc: Span`; the
  loader at `loader.rs:513-527` constructs `loc` to
  include the closing indicator for flow collections.
- `server.rs:483` is the validator call site; already
  has `result.documents` in scope.
- Pattern to copy: `validate_duplicate_keys` at
  `validators.rs:592` and `validate_yaml11_compat` at
  `validators.rs:656` both take
  `&[Document<Span>]`. Both use the span-to-range
  conversion we need.
- `flowStyle` severity override lives at
  `server.rs:484-488` and reads `DiagnosticSeverity` from
  the setting. The retrofit must not change diagnostic
  codes, message text, or severity defaults — otherwise
  the override breaks.
- The carve-outs named in the CLAUDE.md rule:
  (a) byte-range arithmetic on parser-provided spans,
  (b) pre-parse lexical concerns (modeline extraction,
  BOM detection),
  (c) whitespace-preserving edits that don't touch
  structure.

## CLAUDE.md rule — target wording (Task 1)

To be added under the existing "Crate Boundaries"
section, directly after the "parser is the authority on
valid YAML" paragraph:

> **One parser, one AST.** No code in `rlsp-yaml/` may
> re-parse YAML structure from raw text. Validators,
> code actions, hover providers, and any other LSP
> feature that reads YAML structure must consume the
> `rlsp-yaml-parser` AST. Settings that change
> *interpretation* (severity, enable/disable, allowed
> alphabets) are fine; settings that change *parsing*
> belong as `rlsp-yaml-parser` options. Text-handling
> carve-outs: byte-range arithmetic on parser-provided
> spans, pre-parse lexical concerns (modelines, BOM),
> whitespace-preserving edits that don't touch structure.

## Open at the moment of creating this memory

- The user asked to clarify something about the Move 1
  plan before approving — the current conversation
  state is "user is about to tell me what they want to
  clarify." No clarification content has arrived yet.
  Next session should check the ongoing conversation
  (or the user's next message) for what was raised.
- No code changes yet in this conversation. Plan file
  was created at `.ai/plans/2026-04-18-one-parser-one-ast.md`;
  it is uncommitted at the time of writing this memory.
- `/ensure-ai-dirs` was run; all three template files
  in `.ai/plans/` already matched the canonical
  templates — nothing to commit from that skill.

## Deferred ideas (post-Move-2 follow-ups)

### End-to-end LSP pipeline test harness

Discussed 2026-04-18. Current Move 0 invariants invoke
`parse_yaml`, validators, `format_yaml`, and
`code_actions` individually in-process. A true E2E
harness would drive the `tower-lsp` server through
JSON-RPC: send `didOpen` / `didChange`, wait for
`publishDiagnostics`, send `textDocument/codeAction`,
apply the returned `WorkspaceEdit` to the buffer, send
`didChange` again, wait for fresh diagnostics. Treats
the server as a black box.

**Gaps it would close that Move 0 does not:**

- **Settings wiring.** `parse_and_publish` at
  `rlsp-yaml/src/server.rs:476` reads configuration
  (e.g. `self.get_flow_style()` at ~line 481) and
  applies severity overrides (~lines 484–488). Direct
  validator calls bypass the settings merge, so
  setting-dependent severity bugs are invisible
  in-process.
- **JSON-RPC serialization.** Diagnostic ranges, code
  actions, and `WorkspaceEdit`s all cross the wire as
  JSON. Encoding bugs (UTF-16 offsets vs. byte offsets,
  URI escaping, null vs. missing fields) only surface
  at protocol boundaries.

**Where it fits:** after Move 0 + Move 2 land. Not a
replacement for either — in-process invariants are
cheaper and catch the core class of bugs; E2E catches
the serialization/settings-dependent ones. Scoped as
its own plan during Move 3's ongoing expansion.

**Production pipeline shape (for reference):**
1. Client sends `didOpen` / `didChange` → server stores
   the document
2. Server calls `parse_and_publish` → parse + all
   validators + severity overrides + publish
3. Client triggers Quick Fix → `textDocument/codeAction`
   → server returns `CodeAction`s with `WorkspaceEdit`s
4. Client applies the edit (server not involved); sends
   `didChange` → server re-parses and re-validates
5. The formatter is NOT invoked on the quickfix path —
   relevant for why `flow_map_to_block` produces ugly
   whitespace that never gets cleaned up

## How to resume

1. Read `.ai/plans/2026-04-18-one-parser-one-ast.md` for
   the Move 1 plan.
2. Check conversation state for the user's clarification
   after the plan was presented.
3. If plan is still awaiting approval: revise per user
   feedback, re-run plan-reviewer, present again.
4. If plan is approved but not committed: commit it
   with `docs(rlsp-yaml): add plan for one-parser-one-ast`
   before starting execution.
5. If plan is in progress: read the plan's Tasks section
   checkboxes and recorded commit SHAs to resume from
   the next incomplete task.
6. Team setup: `TeamCreate` with all four agents
   (developer, reviewer, test-engineer, security-engineer)
   per the blueprint. Send the plan file path to the
   reviewer for scope tracking. Spawn advisors
   task-agnostic (no upcoming-task hints at spawn time).
7. Risk assessment at dispatch: Task 2 has moderate
   uncertainty (API/behavior change; new test pattern
   not previously used); test-engineer consultation
   recommended. Task 5 touches release infrastructure
   (medium blast radius); worth extra care on
   verification; no security concerns. Tasks 1, 3, 4
   are lower risk.
