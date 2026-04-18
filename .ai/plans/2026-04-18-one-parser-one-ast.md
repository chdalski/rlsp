**Repository:** root
**Status:** InProgress
**Created:** 2026-04-18

## Goal

Establish "one parser, one AST" as an explicit architectural
rule in `rlsp-yaml/`, and prove it by retrofitting
`validate_flow_style` — the validator that currently
re-lexes YAML by hand from `&str` — to consume the parser
AST instead. This eliminates the false-positive class
where GitHub Actions expressions (`${{ … }}`) inside plain
scalars are mis-flagged as flow mappings, and sets the
pattern (plus a mechanical audit gate) for retrofitting
the remaining text-scan sites in follow-up plans.

## Context

### The bug being closed

`rlsp-yaml/src/validation/validators.rs:197-265`
(`validate_flow_style`) walks each line of raw text
looking for `{` / `[` outside of quotes. It has no
YAML-context awareness — `$` is not an indicator, so
`${{ secrets.GITHUB_TOKEN }}` is a valid plain scalar in
block context per YAML 1.2 §7.3.3, yet the scanner reports
its internal braces as flow-mapping warnings. Tracing the
function on `          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}`
shows the outer loop emits **two overlapping diagnostics**
per expression (one for each `{`), so the noise is 2N in a
file with N such expressions. The user's `.github/workflows/
release-plz.yml` contains roughly 20 such expressions.

### Why the AST fix is clean

`rlsp-yaml-parser` already carries exactly the signal we
need:

- `Node::Mapping { style: CollectionStyle, loc, .. }`
  and `Node::Sequence { style: CollectionStyle, loc, .. }`
  at `rlsp-yaml-parser/src/node.rs:56-89`. Added in
  commit 728d182 as part of Task 1 of
  `.ai/plans/2026-04-13-flow-style-preservation-and-enforcement.md`.
- `CollectionStyle::{Block, Flow}` is populated by the
  loader directly from the parser events.
- For flow collections, `loc` covers the `{...}` / `[...]`
  extent inclusive — `rlsp-yaml-parser/src/loader.rs:513-527`
  and the sequence equivalent at 609-611 construct
  `Span { start: open_indicator.start, end: close_indicator.end }`.

The call site `rlsp-yaml/src/server.rs:483` already has
`result.documents` available; passing the AST is mechanical.

### Pattern reference

Two validators already follow the target shape:

- `validate_duplicate_keys(docs: &[Document<Span>])` at
  `validators.rs:592`
- `validate_yaml11_compat(docs: &[Document<Span>])` at
  `validators.rs:656`

The retrofit matches this shape.

### Inventory of remaining violators

Retrofitted in follow-up plans (not this one). The audit
test in Task 4 carries them on an allow-list with
`TODO(follow-up-plan)` markers so the backlog is visible:

- `validate_unused_anchors(text: &str)` at `validators.rs:29`
  — pure text-scan
- `validate_custom_tags(text, docs, allowed_tags)` at
  `validators.rs:297` — hybrid (text + docs)
- `validate_key_ordering(text, docs)` at `validators.rs:475`
  — hybrid
- `flow_map_to_block` and `flow_seq_to_block` in
  `rlsp-yaml/src/editing/code_actions.rs` — text-surgery
  code actions; independent destructive bug traced in
  `flow_map_to_block` that ate the `GITHUB_TOKEN:` key
  on the user-reported input

### Behavior changes users will observe

1. **Spurious `flowMap`/`flowSeq` warnings on `${{ … }}`
   disappear.** Primary motivation.
2. **Multi-line flow collections start being flagged.**
   The current text scanner misses `foo: {\n  a: 1,\n}`
   because `find_closing_char` only scans within one line.
   The AST walk finds them. Confirmed intended — users who
   don't want the noise already have `flowStyle: off`.
3. **Two-overlapping-diagnostics-per-expression bug
   disappears** — the AST never double-reports.

### Prerequisite

`.ai/plans/2026-04-18-corpus-invariants-scaffold.md`
(Move 0) must land before this plan begins execution.
Move 0 builds the invariant harness and seed corpus,
and produces a skip-list of currently-failing
(file, invariant) pairs. Move 0 is a prerequisite
because it establishes the corpus and invariant harness
this retrofit will run against — any regression the
retrofit introduces on real files surfaces as a new
harness failure rather than going undetected. This plan
does not claim to remove any specific Move 0 skip-list
entries; all current entries are on I4 and point at the
destructive-code-action-fix plan, which is a separate
follow-up. Move 1's success is judged against its own
acceptance criteria (below), not against the skip-list
shape.

### References

- YAML 1.2 specification §7.3.3 (plain scalars in block
  context may contain `{` and `}`):
  https://yaml.org/spec/1.2.2/#733-plain-style
- YAML 1.2 §7.4 (flow collections — the legitimate
  `{key: value}` form this validator should be flagging):
  https://yaml.org/spec/1.2.2/#74-flow-collection-styles
- Prior plan that added `CollectionStyle` to the AST:
  `.ai/plans/2026-04-13-flow-style-preservation-and-enforcement.md`
- GitHub Actions expression syntax (the false-positive
  payload): https://docs.github.com/en/actions/learn-github-actions/expressions
- Root CLAUDE.md "Crate Boundaries" section — the new
  rule extends the existing "parser is the authority on
  valid YAML" principle and belongs in the same location.

## Steps

- [x] Add "One parser, one AST" rule to root CLAUDE.md
- [x] Retrofit `validate_flow_style` to consume the AST
- [x] Add regression coverage for GHA-style expressions
- [x] Add a boundary-audit `#[test]` that fails when new
      violators are introduced
- [ ] Gate release-plz on successful CI via `workflow_run`

## Tasks

### Task 1: Document the "One parser, one AST" rule

Extend the existing "Crate Boundaries" section of root
`/workspace/CLAUDE.md` with the explicit rule, carve-outs,
and the interpretation-vs-parse settings distinction. No
code changes in this task — documentation only.

- [x] Add the rule to the Crate Boundaries section,
      immediately after the existing "The parser is the
      authority on valid YAML" paragraph so the two read
      coherently
- [x] Rule text to include:
  - No code in `rlsp-yaml/` may re-parse YAML structure
    from raw text; LSP features consume the
    `rlsp-yaml-parser` AST
  - Carve-outs: byte-range arithmetic on parser-provided
    spans; pre-parse lexical concerns (modeline extraction,
    BOM detection); whitespace-preserving edits that don't
    touch structure
  - Settings that change *interpretation* (severity,
    enable/disable, allowed alphabets) are fine;
    settings that change *parsing* belong as
    `rlsp-yaml-parser` options
- [x] Verify the new text reads cleanly alongside the
      existing crate-boundary table

Acceptance: root `CLAUDE.md` contains the rule; reading
the Crate Boundaries section end-to-end gives a clear
statement of the boundary.

**Completed:** commit `d06fff8` — rule added to
CLAUDE.md immediately after the "parser is the
authority" paragraph, as planned.

### Task 2: Retrofit `validate_flow_style` to consume the AST

Change the signature to
`validate_flow_style(docs: &[Document<Span>]) -> Vec<Diagnostic>`.
Walk the AST recursively, descending into mapping values
and sequence items. Emit one `flowMap` diagnostic for
every `Node::Mapping { style: CollectionStyle::Flow, .. }`
with non-empty `entries`, and one `flowSeq` diagnostic for
every `Node::Sequence { style: CollectionStyle::Flow, .. }`
with non-empty `items`. Use the node's `loc` span as the
diagnostic range.

- [x] Rewrite the function body as an AST walker; extract
      a small helper for "walk node, collect flow
      diagnostics" since mappings and sequences both
      contain children that need the same treatment
- [x] Preserve the "skip empty collections" behavior —
      check `entries.is_empty()` / `items.is_empty()`
      before emitting
- [x] Convert `Span` to LSP `Range` using the existing
      span-to-range helpers already used in
      `validate_duplicate_keys` / `validate_yaml11_compat`
- [x] Keep the diagnostic code strings (`"flowMap"`,
      `"flowSeq"`), severity (`WARNING`), source
      (`"rlsp-yaml"`), and message text identical so the
      `flowStyle` setting's severity override in
      `server.rs:484-488` continues to work without changes
- [x] Update `rlsp-yaml/src/server.rs:483` to pass
      `&result.documents`
- [x] Update `rlsp-yaml/benches/hot_path.rs:43` — parse
      inputs once in bench setup, pass docs
- [x] Update `rlsp-yaml/benches/insight.rs:43` — same
      (line 33 already parses docs; only the bench closure
      needs the new signature)
- [x] Update `rlsp-yaml/tests/ecosystem_fixtures.rs:26,
      247, 275` — parse, pass docs
- [x] Rewrite the unit-test block in
      `validators.rs:1028-1113` to build docs in each
      test. Preserve existing test names and intents
      (empty collections skipped, real flow mappings
      detected, nested detection, quoted content ignored,
      multi-document behavior)
- [x] Check whether `find_closing_char` at
      `validators.rs:268` has any remaining callers after
      the retrofit. If only `validate_flow_style` used it,
      delete it. If `validate_unused_anchors` still uses
      it (that function stays text-based in this plan),
      leave it in place.
- [x] Update `rlsp-yaml/docs/configuration.md` under the
      `flowStyle` entry to note that multi-line flow
      collections (the form where `{` or `[` opens on one
      line and closes on another) are also detected — the
      current text-based implementation misses them, and
      users relying on `flowStyle: warning` will start
      seeing new warnings after this change
- [x] Add an entry to `rlsp-yaml/docs/feature-log.md`
      recording the two user-visible behavior changes:
      (a) `${{ … }}` GitHub Actions expressions and any
      other plain scalar containing `{`/`[` no longer
      trip `flowMap`/`flowSeq`; (b) multi-line flow
      collections are now detected
- [x] Run `cargo fmt`, `cargo clippy --all-targets`,
      `cargo test` — all clean

Acceptance: the retrofitted function uses AST only; the
test suite passes; the GHA-expression input produces
zero diagnostics when passed through the full pipeline;
multi-line flow collections now emit warnings; the two
user-facing docs reflect the new behavior.

**Completed:** commit `9c5a6e1` — retrofit
lands via AST walker; 14 new tests for field identity,
GHA-style expressions, multi-line detection, and
no-double-report; existing standalone intents preserved.
`find_closing_char` deleted (no remaining callers).
Range contract preserved via `end_col + 1` adjustment
to match the parser's zero-width `MappingEnd`/`SequenceEnd`
span — documented in-source. Corpus harness's I4 skip-list
entries for `release-plz-workflow.yml` and
`github-actions-matrix.yml` stay active: the range
correctness fix makes `flow_map_to_block` reachable on
`- { ... }` sequence-entry flow maps, which triggers the
destructive-code-action bug tracked in the
destructive-code-action-fix stub plan.

### Task 3: Add regression coverage for GHA-style expressions

Write tests that would have caught the original bug
class. Both a narrow unit test and an ecosystem-style
fixture using a representative GitHub Actions workflow.

- [x] Add unit test
      `flow_style_ignores_github_actions_expressions` in
      `validators.rs` covering:
  - Single `${{ foo }}` as a plain-scalar value
  - Nested `${{ fromJSON(needs.x.outputs.y) }}` with
    function call syntax inside braces
  - A line with multiple expressions
    (`${{ x }} and ${{ y }}` concatenated)
  - A real flow mapping in the same document
    (`matrix: { target: linux, os: ubuntu }`) to confirm
    positive detection still works
- [x] Add an ecosystem fixture exercising the pattern.
      Either a new module/case in
      `tests/ecosystem_fixtures.rs` using a GitHub Actions
      workflow snippet, or a fixture file under
      `tests/fixtures/` if the fixtures directory
      convention extends there. Use content representative
      of `.github/workflows/release-plz.yml` — a
      `strategy.matrix` with a real flow mapping plus
      multiple `${{ … }}` expressions in env/if/run —
      and assert zero `flowMap`/`flowSeq` diagnostics
      on the expression lines, plus positive detection
      on the matrix line
- [x] Tests must fail against current (pre-Task-2)
      behavior and pass after Task 2

Acceptance: running the new tests before Task 2's
implementation lands shows them failing with specific
false-positive diagnostics; after Task 2 they pass.

**Completed:** commit `9f40e88` — added
`flow_style_ignores_github_actions_expressions` unit
test and the `GHA_RELEASE_PLZ_STYLE` ecosystem fixture
with `gha_release_plz_style_no_false_positives` and
`gha_release_plz_style_expression_lines_zero_flow_diagnostics`
tests. Regression-catch demonstrated: reverting Task 2
locally surfaced 9 false positives on the unit test
and 16 on the ecosystem test, consistent with the 2N-
per-expression bug class documented in plan context.

### Task 4: Add boundary-audit test

Write a `#[test]` that walks `rlsp-yaml/src/**/*.rs` and
fails if any validator or code-action function signature
takes `text: &str` outside an allow-list. The test encodes
the CLAUDE.md rule mechanically and carries the inventoried
violators as a visible worklist.

- [x] Create `rlsp-yaml/tests/parser_boundary_audit.rs`
      that:
  - Walks `src/**/*.rs` using `std::fs` (no new deps)
  - Detects pub-fn signatures matching `pub fn validate_\w+\(`
    and `pub fn \w+_to_block\(` / similar code-action
    patterns, flagging those whose first `&str` parameter
    is named `text`
  - Compares each detected match against an allow-list of
    `(file, function)` pairs that documents each exemption
  - Allow-list entries for confirmed carve-outs
    (whitespace-preserving edits, modeline extraction,
    BOM detection) carry a `// carve-out:` justification
    comment
  - Allow-list entries for remaining violators inventoried
    in Context carry a `// TODO(follow-up-plan):`
    justification referencing the function name so
    follow-up plans have a visible worklist
- [x] Add a top-of-file comment to
      `parser_boundary_audit.rs` stating explicitly: *the
      allow-list is shrink-only. Entries are removed as
      violators are retrofitted in follow-up plans. New
      entries are never added for new violations; the
      only exception is a genuine carve-out (modeline
      extraction, BOM detection, whitespace-preserving
      edit) which must include a `// carve-out:`
      justification referencing the exception category*.
      This constraint is the audit's enforcement surface
      — without it the test degrades to a rubber stamp.
- [x] Verify per-entry coverage before trusting the
      allow-list: for each inventoried violator placed
      on the list, temporarily remove its entry and run
      the audit. The test must fail citing that specific
      function. Restore the entry after verification.
      Without this step an allow-list entry can be dead
      weight — the regex may not match the function's
      actual signature, so the entry protects nothing.
      The commit message for Task 4 records that each
      inventoried entry was verified.
- [x] Test fails cleanly when a synthetic new violator
      is added to the crate (manual spot-check during
      development)
- [x] Test passes on the retrofitted codebase

Acceptance: `cargo test --test parser_boundary_audit`
passes. The top-of-file comment states the shrink-only
constraint. Adding a test-only `pub fn validate_foo(text: &str)`
stub locally (and reverting) confirms the audit fails on
new violators. The per-entry verification has been
performed for every allow-listed violator and recorded
in the commit message.

**Completed:** commit `84d6ece` — boundary
audit lands in `rlsp-yaml/tests/parser_boundary_audit.rs`.
5 allow-list entries (the 4 inventoried violators plus
`validate_schema` in `schema_validation.rs`, discovered
during implementation). Per-entry verification performed
and reviewer-side re-verified. Also adds 10 unit tests
covering regex detection (Group A) and allow-list
mechanics (Group B). `flow_map_to_block` /
`flow_seq_to_block` are private helpers taking
`lines: &[&str]`, so they fall outside the audit's
public-function scope by design — tracked in the
destructive-code-action-fix stub plan.

### Task 5: Gate release-plz on successful CI

Change `.github/workflows/release-plz.yml` from
`push: branches: [main]` to `workflow_run` of the CI
workflow, with a job-level conditional that releases
only run when CI concluded successfully. The audit test
runs as part of CI, so CI passing implies the audit
passed.

- [ ] Identify the CI workflow's `name:` field exactly
      (must match the `workflows` filter of the trigger)
- [ ] Replace `release-plz.yml`'s trigger:
  - Remove `push: branches: [main]`
  - Add `workflow_run: workflows: ["<CI name>"],
    types: [completed], branches: [main]`
- [ ] Add
      `if: github.event.workflow_run.conclusion == 'success'`
      on every job (release PR, release, trigger-vscode,
      filter-binaries, build-binaries) so a failed CI
      skips the whole workflow cleanly
- [ ] Ensure release-plz checks out the exact commit CI
      validated: use
      `ref: ${{ github.event.workflow_run.head_sha }}`
      on the `actions/checkout` steps (the default
      behavior under `workflow_run` is to use the default
      branch, not the triggering commit, which is wrong
      for us)
- [ ] Per the repository's github-workflows rule, update
      action versions to latest stable since we're
      touching the file (spot-check `actions/checkout`,
      `dtolnay/rust-toolchain`, `MarcoIeni/release-plz-action`,
      `Swatinem/rust-cache`)
- [ ] Document the gate in a short comment at the top of
      `release-plz.yml`
- [ ] Post-merge manual verification (supplementary):
  - Push a branch with a failing clippy warning → CI
    fails → release-plz does not trigger
  - Push a passing commit → CI succeeds → release-plz
    triggers and operates on the expected SHA

Acceptance (diff-verifiable at review time):
- `release-plz.yml` has no `push` trigger; its trigger
  is `workflow_run: workflows: ["<CI name>"], types:
  [completed], branches: [main]`
- The `workflows:` filter value matches the `name:` field
  in `ci.yml` exactly (reviewer cross-references both
  files)
- Every release job has
  `if: github.event.workflow_run.conclusion == 'success'`
- Every `actions/checkout` step in `release-plz.yml` has
  `ref: ${{ github.event.workflow_run.head_sha }}`
- Action version tags at `@vN` are the current latest
  stable (spot-checked against upstream)
- The top-of-file comment documents the gate

Supplementary acceptance (post-merge manual, not
reviewable from diff):
- Known-bad CI run does not trigger release-plz
- Known-good CI run triggers release-plz against the
  validated commit SHA

## Decisions

- **Scope — option 1a.** Only `validate_flow_style` is
  retrofitted here. Remaining text-scan sites
  (`validate_unused_anchors`, two hybrid validators, two
  text-surgery code actions) are inventoried for
  follow-up plans and carried on the audit test's
  allow-list so the backlog is visible in code.
- **Multi-line flow collections are flagged.** Natural
  consequence of the AST walk. `flowStyle: off` remains
  the user opt-out.
- **Audit implementation:** `#[test]` in `rlsp-yaml/tests/`.
  Runs with `cargo test`; no new CI surface.
- **CI gate:** `workflow_run` trigger on
  `release-plz.yml`. Gate is reviewable in code; SHA is
  pinned to the validated commit.
- **Signature change has no deprecation shim.**
  `validate_flow_style` is crate-internal by convention
  (no external callers).
- **Diagnostic semantics unchanged.** Codes
  (`flowMap`/`flowSeq`), severity (WARNING default, ERROR
  override via `flowStyle: "error"`), source, and message
  text stay identical so the existing `flowStyle` setting
  and severity-override logic at `server.rs:484-488`
  continue to work without modification.
- **Span coverage.** The AST's `loc` on flow collections
  already covers `{...}` / `[...]` inclusive per
  `loader.rs:513-527`, so diagnostic ranges will be
  precise without extra scanning.
- **The destructive `flow_map_to_block` quick fix is not
  fixed in this plan.** After Task 2 it is no longer
  reachable on `${{ … }}` input, so the user-visible
  destruction stops. Its latent defects (full-line
  replace, single-line scope, key-reconstruction
  fragility) become a Move-1 follow-up plan before Move 2.
