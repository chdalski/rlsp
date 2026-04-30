**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-30

## Goal

Port the rename feature's transformation-shaped inline
tests in `rlsp-yaml/src/navigation/rename.rs` to a new
fixture harness at `rlsp-yaml/tests/rename_fixtures.rs`,
mirroring the established cursor-driven code-action
fixture pattern. Concurrent with the port, extract the
duplicated integration-test helpers (`cursor_range`,
`docs_for`, `test_uri`, `apply_text_edit`,
`codepoint_to_byte`) from
`tests/code_action_fixtures.rs` and
`tests/code_action_property_preservation.rs` into a
shared `tests/common/mod.rs` module, then make the new
rename harness consume from there too. After this plan,
the rename feature has visually browsable fixtures for
its happy-path and negation behaviors, and three
integration test crates share helpers from one place
instead of three.

## Context

### Why the port

`rlsp-yaml/src/navigation/rename.rs` is the LSP
`textDocument/rename` implementation: it edits anchor
and alias names within a YAML document. Today it has
~68 inline `#[test]` and `#[rstest]` cases (19 attribute
markers + 49 `#[case]` discriminants) covering happy
paths, boundary conditions, validation, and edge cases.
Most of the happy-path tests have shape *input + cursor +
new_name → expected output document* — exactly the
shape the existing fixture format expresses cleanly.
A reader who opens a `.md` fixture sees the YAML before
the rename, the rename inputs, and the YAML after,
without reading Rust.

### Pattern taxonomy (carried over from the predecessor)

The completed code-action fixture port plan
(`2026-04-27-code-action-fixture-tests.md`, commit:
the port landed in late April) classified inline tests
into three patterns. The same taxonomy applies here:

- **Pattern A — clean transformation:** input doc +
  cursor + new_name → expected output document. Single
  positive transformation. Ports cleanly to a fixture.
- **Pattern B — negation:** input doc + cursor +
  new_name → no rename produced (rename returns `None`,
  invalid new-name rejected, cursor not on a renameable
  symbol). Ports cleanly to a fixture with an
  `omits-rename` discriminator.
- **Pattern C — does not fit:** range-structure
  assertions (e.g.
  `should_produce_correct_edit_ranges` asserts specific
  line/column ranges of the WorkspaceEdit's `TextEdit`s),
  count assertions (e.g. `rename_returns_edits_len`
  asserts a numeric edit count without validating the
  resulting document), existence-only assertions (e.g.
  `rename_accepts_valid_new_name` asserts `rename`
  returned `Some` without comparing edits), and **all
  prepare_rename tests** (the API returns a `Range`, not
  a transformation — Pattern C by construction).

### Pre-scan is mandatory

Per the followup queue's stated criterion, the
classification of every inline test must happen
**before** porting begins. A surprise Pattern C found
mid-port forces backtracking — fixture files for tests
that should have stayed inline get drafted, then
abandoned. Task 2 below makes the pre-scan its first
sub-task; the rest of the porting work depends on it.

### Approximate classification (to be verified by
pre-scan)

The plan uses the following rough counts to size the
fixture set; the developer/test-engineer pre-scan
finalizes the exact mapping before fixture files are
authored.

| Region | Likely Pattern | Approx. cases |
|---|---|---|
| `prepare_rename` happy-path tests | C (Range structure) | 6 single + 1 rstest case set |
| `prepare_rename_returns_none` rstest | B (omits) | 7 cases |
| `rename_returns_edits_len` rstest | A (with reformulation) | 6 cases |
| `should_produce_correct_edit_ranges` | C (Range structure) | 1 |
| `rename_anchor_on_*_collection_edits_token_span_not_body` | C (Range structure) | 2 |
| `rename_does_not_cross_document_boundary_to_*` | A | 2 |
| `rename_utf8_anchor_name_produces_correct_column_ranges` | C (column ranges) | 1 |
| `rename_returns_none_invalid_position` rstest | B (omits) | varies |
| `rename_rejects_invalid_new_name` rstest | B (omits) | varies |
| `rename_accepts_valid_new_name` rstest | C (existence-only) | varies |

The pre-scan finalizes per-test classification; counts
above guide planning, not deliverables.

### prepare_rename stays inline (in full)

`prepare_rename` returns `Option<Range>`, not a document
transformation. Even the happy-path tests assert specific
`Range` field values — there is no "expected output
document" to compare against. Forcing prepare_rename into
the fixture format would either (a) abandon its real
assertion (range fields) in favor of a weaker existence
check, or (b) bolt range-structure fields onto fixture
frontmatter, polluting the format for future LSP
features. Either option degrades the fixture format. All
prepare_rename tests stay inline as Pattern C.

### Helper consolidation: the third caller arrives

Two integration test crates currently duplicate
`cursor_range`, `docs_for`, `test_uri`, `apply_text_edit`,
and `codepoint_to_byte`:

- `rlsp-yaml/tests/code_action_fixtures.rs` (introduced
  by the predecessor code-action fixture port)
- `rlsp-yaml/tests/code_action_property_preservation.rs`
  (introduced by the property-preservation invariant in
  commit `4aa19e9`)

The followup entry tracking this duplication
(`.ai/memory/project_followup_plans.md`) defined the
trigger as *"third caller arrives."* This plan creates
that third caller — `tests/rename_fixtures.rs` — so the
extraction lands inside this plan rather than as a
separate refactor. The shared module lives at
`tests/common/mod.rs` per the standard Rust integration-
test convention: Cargo treats `tests/common/mod.rs` as a
module (not a separate test binary) and sibling
integration tests import it via `mod common; use
common::*;`.

### Frontmatter shape for rename fixtures

Code-action fixtures use:

```
---
test-name: ...
category: ...
cursor: line:char
applies-action: <title-substring>   # OR omits-action
format-options:
  ...
---
```

Rename fixtures use a parallel shape with rename-
specific fields:

```
---
test-name: rename-anchor-on-scalar
category: rename
cursor: 0:6
new-name: replacement
applies-rename: true                 # OR omits-rename: true
---
```

`applies-rename: true` and `omits-rename: true` are
mutually exclusive. The harness parses frontmatter, calls
`rlsp_yaml::navigation::rename::rename(...)` with the
parsed cursor and new-name, and either applies all
`TextEdit`s from the resulting `WorkspaceEdit` and
compares against `Expected-Document` (Pattern A), or
asserts the rename returned `None` (Pattern B). When
multiple `TextEdit`s land in the same `changes` map,
they apply in source-order from latest to earliest by
range start so earlier edits don't shift later edit
ranges.

### Key files

| File | Role |
|---|---|
| `rlsp-yaml/tests/common/mod.rs` | New shared helper module |
| `rlsp-yaml/tests/code_action_fixtures.rs` | Migrated to consume from `common` |
| `rlsp-yaml/tests/code_action_property_preservation.rs` | Migrated to consume from `common` |
| `rlsp-yaml/tests/rename_fixtures.rs` | New rename fixture harness |
| `rlsp-yaml/tests/fixtures/rename/*.md` | New fixture directory |
| `rlsp-yaml/tests/fixtures/rename/CLAUDE.md` | New per-directory README documenting frontmatter fields |
| `rlsp-yaml/src/navigation/rename.rs` | Pattern A and B inline tests removed; Pattern C inline tests preserved |
| `rlsp-yaml/tests/fixtures/CLAUDE.md` | Updated: rename added to fixture-vs-inline list AND new `## Rename Fixtures` section added at the same depth as the existing `## Code-Action Fixtures` section |
| `.ai/memory/project_followup_plans.md` | Both `Consolidate duplicated integration-test helpers` and `Port \`rename\` to fixtures` entries removed |

### References

- Predecessor code-action fixture port:
  `.ai/plans/2026-04-27-code-action-fixture-tests.md`
  (Completed) — defines the Pattern A/B/C taxonomy, the
  fixture format conventions, and the harness shape.
- Existing harness:
  `rlsp-yaml/tests/code_action_fixtures.rs` — pattern
  reference for fixture parsing, frontmatter handling,
  and edit-application logic.
- Fixture-vs-inline rule:
  `rlsp-yaml/tests/fixtures/CLAUDE.md` — Pattern C
  scenarios stay inline.
- LSP rename spec:
  https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_rename

## Steps

- [x] Task 1 — extract shared helpers to
      `tests/common/mod.rs`; migrate the two existing
      integration test crates
- [ ] Task 2 — pre-scan rename inline tests; create
      rename fixture harness; port Pattern A and B
      tests; remove ported inline tests; remove the
      helper-consolidation follow-up entry

## Tasks

### Task 1: Extract shared helpers to `tests/common/mod.rs`

Move the duplicated helpers
(`cursor_range`, `docs_for`, `test_uri`,
`apply_text_edit`, `codepoint_to_byte`) from
`tests/code_action_fixtures.rs` and
`tests/code_action_property_preservation.rs` into a
shared module at `tests/common/mod.rs`. Both existing
test crates declare `mod common;` and import the helpers
via `use common::*;`. The harness-specific logic
(frontmatter parsing, dispatch, assertion) stays in each
crate's main file.

- [x] Create `rlsp-yaml/tests/common/mod.rs` with the
      five helpers: `cursor_range`, `docs_for`,
      `test_uri`, `apply_text_edit`, `codepoint_to_byte`.
      Each helper's signature and behavior matches the
      existing implementations exactly — this is a pure
      relocation, not a redesign.
- [x] Update `rlsp-yaml/tests/code_action_fixtures.rs`:
      add `mod common;` and `use common::*;`; delete the
      now-duplicated helper definitions.
- [x] Update
      `rlsp-yaml/tests/code_action_property_preservation.rs`:
      add `mod common;` and `use common::*;`; delete the
      duplicated helper definitions.
- [x] `cargo test --test code_action_fixtures` passes
      (no regressions; the harness still finds and runs
      every fixture under `tests/fixtures/code_actions/`).
- [x] `cargo test --test code_action_property_preservation`
      passes (24/24 invariant cases).
- [x] `cargo test --workspace` passes.
- [x] `cargo clippy --all-targets` clean.
- [x] `cargo fmt` applied.

Acceptance: the five helpers exist exactly once across
`tests/`; both existing integration test crates compile
and run with no behavioral change; `cargo test
--workspace` is green.

**Commit:** `4a86295`

### Task 2: Create rename fixture harness; port Pattern A and B inline tests

Pre-scan the inline tests in `rlsp-yaml/src/navigation/rename.rs`,
classify each into Pattern A / B / C, document the
classification in this task's completion notes. Create
the rename fixture harness at
`rlsp-yaml/tests/rename_fixtures.rs` consuming
`tests/common/mod.rs`. Port every Pattern A test to a
fixture file under `rlsp-yaml/tests/fixtures/rename/`
with `applies-rename: true`, every Pattern B test with
`omits-rename: true`. Remove the ported inline tests
from `rename.rs`. Pattern C tests (all `prepare_rename`
tests, range-structure assertions, count assertions,
existence-only assertions) stay inline. Remove the
helper-consolidation follow-up entry from
`.ai/memory/project_followup_plans.md` since this plan
fulfills it.

- [ ] **Pre-scan:** read every test in `rename.rs::tests`
      (lines 211-615). For each test (including each
      `#[case::...]` discriminant inside an `rstest`),
      assign Pattern A, B, or C. Record the
      classification as a markdown table in this task's
      completion notes — test name, pattern, brief
      reason. The classification governs every porting
      decision below.
- [ ] Create
      `rlsp-yaml/tests/fixtures/rename/CLAUDE.md`
      documenting the rename fixture format: required
      frontmatter fields (`test-name`, `category`,
      `cursor`, `new-name`, `applies-rename` /
      `omits-rename`), Test-Document and Expected-Document
      section conventions, and the multi-edit
      application rule (apply edits in reverse range-start
      order so earlier edits don't shift later ranges).
- [ ] Create
      `rlsp-yaml/tests/rename_fixtures.rs` modeled on
      `tests/code_action_fixtures.rs`. The harness:
      declares `mod common;` and `use common::*;`; parses
      rename frontmatter (`new-name`, `applies-rename`,
      `omits-rename`); calls
      `rlsp_yaml::navigation::rename::rename(...)`;
      applies all `TextEdit`s from the returned
      `WorkspaceEdit` for the test URI in reverse
      range-start order; asserts the result equals
      `Expected-Document` (for `applies-rename`) or that
      `rename(...)` returned `None` (for `omits-rename`).
      Reuse the rstest+`#[files(...)]` mechanism from
      `code_action_fixtures.rs`.
- [ ] For every Pattern A test identified in the
      pre-scan, author a fixture file at
      `rlsp-yaml/tests/fixtures/rename/<descriptive-slug>.md`.
      Filename slug describes the scenario (e.g.,
      `rename-anchor-on-scalar.md`,
      `rename-anchor-with-multiple-aliases.md`,
      `rename-does-not-cross-document-boundary.md`).
      Each fixture's `Test-Document` is the input YAML;
      `Expected-Document` is the post-rename YAML.
- [ ] For every Pattern B test, author a fixture file
      with `omits-rename: true` and no `Expected-Document`
      section. Filename slug describes the rejection
      reason (e.g.,
      `rename-rejects-empty-new-name.md`,
      `rename-rejects-cursor-not-on-anchor.md`).
- [ ] Remove the ported Pattern A and Pattern B inline
      tests from `rlsp-yaml/src/navigation/rename.rs`.
      Pattern C tests stay. The line ranges removed must
      match exactly the tests classified as A or B in
      the pre-scan — no Pattern C test is silently
      removed; no Pattern A or B test is left behind.
- [ ] Update `rlsp-yaml/tests/fixtures/CLAUDE.md` in
      two places: (a) add rename fixtures to the prose
      list in "When to Write a Fixture vs an Inline
      Test" alongside the existing code-action mention;
      (b) add a new top-level `## Rename Fixtures`
      section mirroring the depth of the existing
      `## Code-Action Fixtures` section — frontmatter
      fields table (`test-name`, `category`, `cursor`,
      `new-name`, `applies-rename`, `omits-rename`),
      assertion-modes description (applies-rename
      applies all `TextEdit`s and compares against
      `Expected-Document`; omits-rename asserts
      `rename(...)` returned `None`), cursor convention
      (zero-based line:character matching the LSP
      `Position` type), sections listing (`## Test-Document`
      always required; `## Expected-Document` required
      for `applies-rename`, omitted for `omits-rename`),
      and the multi-edit application rule (apply edits
      in reverse range-start order). The top-level file
      is the index developers consult; per-directory
      `tests/fixtures/rename/CLAUDE.md` is supplementary,
      not a replacement for the top-level documentation.
- [ ] Remove the
      `**Consolidate duplicated integration-test helpers**`
      entry from
      `.ai/memory/project_followup_plans.md`. This plan
      fulfills the criterion ("third caller arrives")
      and extracts the helpers in Task 1; the entry is
      no longer open work.
- [ ] Remove the `**Port \`rename\` to fixtures**`
      entry from `.ai/memory/project_followup_plans.md`.
      This plan delivers the work the entry tracks; the
      entry is no longer open work.
- [ ] `cargo test --test rename_fixtures` passes for
      every authored fixture, with zero ignored cases.
- [ ] `cargo test -p rlsp-yaml --lib` passes — the
      remaining inline `rename.rs` tests (Pattern C) are
      green.
- [ ] `cargo test --workspace` passes (no regressions
      across other crates).
- [ ] `cargo clippy --all-targets` clean.
- [ ] `cargo fmt` applied.
- [ ] Total test count check: the count of inline tests
      removed from `rename.rs` (counting each `#[case::...]`
      discriminant separately) equals the count of
      fixture files added under `tests/fixtures/rename/`.
      Record both counts in completion notes — they must
      be equal.

Acceptance: the rename fixture harness exists and runs
all authored Pattern A and B fixtures; `rename.rs` retains
only Pattern C inline tests; both follow-up entries
(`Consolidate duplicated integration-test helpers` and
`Port \`rename\` to fixtures`) are removed from
`.ai/memory/project_followup_plans.md`; the top-level
`rlsp-yaml/tests/fixtures/CLAUDE.md` documents rename
fixtures with the same depth as code-action fixtures;
the counts of removed inline tests and added fixture
files match exactly; `cargo test --workspace` is green.

## Decisions

- **Separate harness, not extension of `code_action_fixtures.rs`.**
  Code actions return `Vec<CodeActionOrCommand>` (action
  picked by title substring); rename returns
  `WorkspaceEdit` (potentially multiple `TextEdit`s
  applied together). Frontmatter shapes diverge
  (`applies-action` vs `applies-rename`, no `new-name`
  for code actions). Sharing the harness would force a
  discriminator field whose presence determines which
  other fields are valid — fragile. Truly shared logic
  (helpers, splice) lives in `tests/common/mod.rs` per
  Task 1; per-feature dispatch and assertion stay in
  per-feature harness files.
- **Helper consolidation lands in this plan.** The
  helper duplication tracked in
  `.ai/memory/project_followup_plans.md` was gated on a
  "third caller arrives" criterion. Task 2's new
  `tests/rename_fixtures.rs` is that third caller.
  Extracting in this plan is cheaper than (a) writing a
  third copy and consolidating later, or (b) a separate
  motivation-less refactor plan.
- **Pre-scan is mandatory before porting.** The
  followup queue entry for this work explicitly required
  the per-test pre-scan — surprise Pattern C found
  mid-port forces backtracking. The pre-scan sub-task is
  Task 2's first sub-task and gates every subsequent
  sub-task.
- **prepare_rename stays inline in full.** Its API
  returns `Option<Range>`, not a document transformation.
  All prepare_rename tests are Pattern C by construction.
- **Multi-edit application order.** When a `WorkspaceEdit`
  has multiple `TextEdit`s for the same URI (rename of
  an anchor with N aliases produces N+1 edits), apply
  them in reverse range-start order so earlier edits
  don't shift later edit ranges. Document this in the
  fixture-format CLAUDE.md so future readers understand
  the rule.
- **Fixture filename slugs are descriptive, not numbered.**
  Mirrors the code-action fixture convention
  (`block-scalar-preserve-anchor.md`, not
  `rename-test-001.md`). The slug is the entry point for
  someone scanning the fixture directory; numeric IDs
  add noise without information.

## Non-Goals

- Porting `prepare_rename` tests to fixtures. They are
  Pattern C by construction (return `Range`, not
  transformations) and stay inline.
- Porting Pattern C rename tests (range-structure,
  count, existence-only). They stay inline.
- Adding new rename test coverage beyond what is being
  ported. The plan moves existing tests; it does not
  extend behavioral coverage. Coverage gaps surfaced
  during the pre-scan are filed as follow-ups, not
  fixed in-plan.
- Refactoring `rename.rs` production code. The plan
  touches only the tests module of `rename.rs`.
- Generalizing the fixture harness to a
  feature-agnostic shared driver. Per-feature harnesses
  are the project's existing convention; sharing
  helpers (Task 1) is the right scope.
