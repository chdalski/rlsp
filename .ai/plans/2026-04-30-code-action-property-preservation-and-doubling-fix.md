**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-30

## Goal

Fix the `string_to_block_scalar` code action's bug where
node properties (anchors and user-authored tags) are
emitted twice in the replacement text; add a
parameterized invariant test that asserts every
cursor-driven mutating code action preserves property
literals from input to output; and add a load-bearing
comment at `block_to_flow.rs:51-52` documenting why the
edit-start computation uses `key_end_col + 1` rather
than `loc.start`, so a future simplification cannot
silently re-introduce the doubling bug. The invariant
locks in the seven non-buggy actions' current
correctness, catches the known doubling bug, and
surfaces any latent defects of the same shape in
actions whose property handling has not been verified
end-to-end.

## Context

### The bug

`string_to_block_scalar` (`rlsp-yaml/src/editing/code_actions/block_scalar.rs:12-55`)
clones the source scalar, mutates its `style` to
`ScalarStyle::Literal`, and calls `format_subtree(...)`
to render the replacement text. The clone retains the
node's `tag` field and its `meta.anchor` /
`meta.anchor_loc` / `meta.tag_loc` fields, so
`format_subtree` re-emits `&anchor` and/or `!tag` as a
prefix in `new_text`. Meanwhile, the edit's
`scalar_loc` (`block_scalar.rs:34-43`) covers only the
quoted scalar — `anchor_loc` and `tag_loc` are excluded
per the parser's span model
(`rlsp-yaml-parser/src/node.rs:103-119`). Result: the
source `&anchor `/`!tag ` prefix is preserved in the
buffer *and* the formatter re-emits it, producing
doubling.

Concrete examples:

| Input | Buggy output |
|---|---|
| `description: &myanchor "long string"` | `description: &myanchor &myanchor \|\n  long string` |
| `description: !mytag "long string"` | `description: !mytag !mytag \|\n  long string` |
| `description: &a !!str "long string"` | `description: &a &a \|\n  long string` (anchor doubled; core tag stripped — formatter idempotency rule strips `!!str` from non-empty scalars at `formatter.rs:540-573`) |

The bug went undetected because the inline test that
preceded the fixture port (`tests/fixtures/code_actions/block-scalar-preserve-anchor.md`)
asserted only `result.contains("&myanchor")`, which holds
for both single and double occurrences. The current
fixture's `Expected-Document` codifies the doubled output
and must be corrected as part of the fix.

### Why the doubling does not affect `block_to_flow`

`block_to_flow` (`rlsp-yaml/src/editing/code_actions/block_to_flow.rs:51-52`)
sets the edit start at `key_end_col + 1` — the column
immediately after the `key:` — rather than at
`loc.start`. The wider edit range *includes* the source
`&anchor`/`!tag` prefix, so when the formatter re-emits
the property in `new_text`, the source prefix is replaced
rather than preserved. This is correct but non-obvious;
a future cleanup that "simplifies" the start to
`loc.start` would silently regress to the same
doubling bug.

### The invariant

A parameterized property-preservation test that, for each
non-`delete_anchor` cursor-driven mutating action, feeds
inputs containing `&anchor`, `!mytag`, and the combined
`&a !mytag` form, runs the action, and asserts each
property literal appears in the output exactly as often
as in the input. `delete_anchor` is excluded — its
purpose is to remove an anchor, so the property count is
expected to decrease by one for the targeted anchor;
treating it identically would test the wrong thing.

The actions covered by the invariant:

| File | Function | Mutates how? |
|---|---|---|
| `block_scalar.rs` | `string_to_block_scalar` | Re-renders scalar in literal block form |
| `block_to_flow.rs` | `block_to_flow` | Re-renders block collection in flow form |
| `flow_to_block.rs` | (entry function) | Re-renders flow collection in block form |
| `quoted_bool.rs` | (entry function) | Strips quotes from boolean scalar |
| `tab_to_spaces.rs` | (entry function) | Replaces leading tabs with spaces |
| `yaml11_bool.rs` | (entry function) | Quotes YAML 1.1 boolean keywords |
| `yaml11_octal.rs` | (entry function) | Updates legacy octal literals |

`delete_anchor.rs` is intentionally not in the invariant.

### Surprise-failure protocol

When the invariant runs against the seven actions, two
outcomes are expected for the six actions other than
`string_to_block_scalar`:

1. **Pass** — the action already preserves properties.
   No further work; the invariant locks in correctness.
2. **Fail** — the invariant surfaces a defect not
   previously known.

If a Task 2 failure has the **same root cause** as the
`string_to_block_scalar` bug (clone-then-format
re-emission with a too-narrow edit range), fix it
in-plan during Task 2 with a short follow-up commit —
this is a known-shape fix and stays inside the task.

If the root cause **differs**, the developer must not
self-authorize an `#[ignore]` or any other suppression.
Instead, halt Task 2 and notify the lead with a status
update naming the failing action, the observed
mismatch, and a one-sentence hypothesis for the
different root cause. The lead decides plan-level
disposition: add a follow-up task to this plan,
restructure Task 2's scope, or pause the plan and
consult the user. This honors
`claim-verification.md` (per-failure verification, no
batch categorization) and `no-silent-target-weakening.md`
(no quiet acceptance of a partial result).

Task 2 cannot be marked complete while any action's case
is `#[ignore]`d, suppressed, or otherwise excluded from
the invariant's effective coverage.

### Key files

| File | Role |
|---|---|
| `rlsp-yaml/src/editing/code_actions/block_scalar.rs` | Fix site for the doubling bug |
| `rlsp-yaml/src/editing/code_actions/block_to_flow.rs` | Receives the load-bearing comment about `key_end_col + 1` |
| `rlsp-yaml/src/editing/code_actions/{flow_to_block,quoted_bool,tab_to_spaces,yaml11_bool,yaml11_octal}.rs` | Subjects of the invariant; not modified unless surprise-failure protocol fires |
| `rlsp-yaml/tests/fixtures/code_actions/block-scalar-preserve-anchor.md` | `Expected-Document` updates from doubled to single anchor |
| `rlsp-yaml/tests/fixtures/code_actions/block-scalar-preserve-user-tag.md` | New fixture (added in Task 1) |
| `rlsp-yaml/tests/fixtures/code_actions/block-scalar-preserve-anchor-and-user-tag.md` | New fixture (added in Task 1) |
| `rlsp-yaml/tests/code_action_property_preservation.rs` | New integration test crate housing the invariant |
| `rlsp-yaml-parser/src/node.rs:103-119` | `NodeMeta` field reference (read-only — informs the fix) |

### References

- Project follow-up note: `.ai/memory/project_followup_plans.md`
  — entry "Code-action property-preservation invariant + fix
  `string_to_block_scalar` doubling + load-bearing comment for
  `block_to_flow`".
- YAML 1.2 §6.9 (Node Properties) — anchor and tag syntax.
- The rule against fixture-format use for non-visual
  assertions: `rlsp-yaml/tests/fixtures/CLAUDE.md` "When to
  Write a Fixture vs an Inline Test" — invariants over
  multiple action outputs are structural assertions and
  belong as inline integration tests, not fixtures.

## Steps

- [x] Task 1 — fix `string_to_block_scalar` doubling +
      update/add fixtures
- [ ] Task 2 — add property-preservation invariant test +
      load-bearing comment in `block_to_flow`

## Tasks

### Task 1: Fix `string_to_block_scalar` property doubling

`string_to_block_scalar` currently emits anchors and
user-authored tags twice in its replacement text. Fix the
fix site so the cloned scalar passed to `format_subtree`
no longer carries property metadata that the source range
already preserves. Update the existing anchor fixture to
codify the corrected single-occurrence output, and add
two new fixtures covering user tags alone and the
combined anchor+user-tag form so future regressions are
caught at the fixture layer.

- [x] In `block_scalar.rs:23-28`, before the
      `format_subtree(&block_scalar, ...)` call, mutate
      the cloned scalar's property fields so the
      formatter no longer re-emits anchor or user tag.
      The exact field set is for the developer to
      determine from `NodeMeta` (`node.rs:103-119`) and
      the formatter's emission logic. The acceptance
      criterion is that running the action on each of the
      three example inputs in the Context section
      produces a single occurrence of each property
      literal in the output.
- [x] Update
      `rlsp-yaml/tests/fixtures/code_actions/block-scalar-preserve-anchor.md`
      so `Expected-Document` is:
      ```yaml
      description: &myanchor |
        this is a long string that exceeds forty characters
      ```
      (single `&myanchor`).
- [x] Add
      `rlsp-yaml/tests/fixtures/code_actions/block-scalar-preserve-user-tag.md`
      with input `description: !mytag "this is a long string that exceeds forty characters"`
      and expected output containing exactly one `!mytag`
      followed by the literal block form.
- [x] Add
      `rlsp-yaml/tests/fixtures/code_actions/block-scalar-preserve-anchor-and-user-tag.md`
      with input `description: &a !mytag "this is a long string that exceeds forty characters"`
      and expected output containing exactly one `&a` and
      one `!mytag` in the correct order, followed by the
      literal block form.
- [x] `cargo test --test code_action_fixtures` passes.
      The three preserve-* fixtures all pass.
- [x] `cargo test --workspace` passes (no regressions in
      other crates).
- [x] `cargo clippy --all-targets` clean.
- [x] `cargo fmt` applied.

Acceptance: all four assertions hold simultaneously —
`block-scalar-preserve-anchor.md` passes with the
single-occurrence Expected-Document, both new fixtures
pass, and `cargo test --workspace` is green.

**Commit:** `70f2caa`

### Task 2: Add property-preservation invariant + load-bearing comment in `block_to_flow`

Add a parameterized integration test that runs each of
the seven property-preserving cursor-driven mutating
actions (all of the actions in
`src/editing/code_actions/` except `delete_anchor`)
against three inputs (anchor-only, user-tag-only,
anchor+user-tag) and asserts the property-literal counts
in the output equal those in the input. Add a
load-bearing comment at `block_to_flow.rs:51-52`
documenting why the edit start is computed from
`key_end_col + 1` rather than `loc.start`.

- [ ] Create
      `rlsp-yaml/tests/code_action_property_preservation.rs`
      using `rstest` named cases. The test loads each
      input, dispatches `code_actions(...)` at a cursor
      position that triggers the action under test,
      applies the matching action's first `TextEdit` to
      the input, and asserts: for each property literal
      `prop` in the input, `count(input, prop) ==
      count(output, prop)`.
- [ ] Cover all seven non-`delete_anchor` actions by name
      in the parameterization. Use `#[case::action_name]`
      named cases per
      `lang-rust-testing.md` "Parameterized Tests". Inputs
      must be tailored per action so the action's trigger
      precondition holds (e.g., a long string for
      `string_to_block_scalar`, a tab-prefixed line for
      `tab_to_spaces`, a YAML 1.1 boolean keyword for
      `yaml11_bool`); the test-engineer advisory consult
      finalizes the test list before implementation.
- [ ] Add a comment immediately above
      `block_to_flow.rs:52` (the
      `let edit_start_col = key_end_col as usize + 1;`
      line) explaining that the `+ 1` past the colon
      is load-bearing for property preservation: the
      wider edit range covers the source `&anchor`/`!tag`
      prefix so the formatter's re-emission of those
      properties in `new_text` replaces rather than
      duplicates them. State that simplifying this to
      `loc.start` would re-introduce the doubling bug
      class.
- [ ] If the invariant fails for an action other than
      `string_to_block_scalar` (which is fixed in
      Task 1):
      - **Same root cause** as Task 1 (clone-then-format
        re-emission against a too-narrow edit range):
        fix in-plan within Task 2; record the additional
        fix in this task's completion notes.
      - **Different root cause:** halt Task 2 and notify
        the lead with the surprise-failure status update
        (failing action, observed mismatch, hypothesis).
        Do not write `#[ignore]`, do not file the
        follow-up unilaterally, do not submit the task
        for review. The lead decides plan-level
        disposition.
- [ ] File a follow-up entry in
      `.ai/memory/project_followup_plans.md` for the
      duplicated integration-test helpers (`cursor_range`,
      `docs_for`, `test_uri`) — currently re-implemented
      in `tests/code_action_fixtures.rs` and now
      additionally in
      `tests/code_action_property_preservation.rs`. The
      entry should name the duplication, cite both
      callsites, and propose a future consolidation plan
      to extract a shared module once a third caller
      arrives. This task does not consolidate (see
      Non-Goals).
- [ ] `cargo test --test code_action_property_preservation`
      passes for all seven actions with no `#[ignore]`d
      or suppressed cases.
- [ ] `cargo test --workspace` passes (no regressions).
- [ ] `cargo clippy --all-targets` clean.
- [ ] `cargo fmt` applied.

Acceptance: the invariant test passes for all seven
actions with zero `#[ignore]` annotations on the
parameterized cases; the `block_to_flow` comment is
present and explains the load-bearing role of
`key_end_col + 1`; the follow-up entry for helper
consolidation is filed in
`.ai/memory/project_followup_plans.md`; `cargo test
--workspace` is green.

`#[ignore]` is not a permitted state for the invariant
at task close. If the surprise-failure protocol
escalates a different-root-cause failure to the lead
and the lead's disposition removes an action from the
invariant's coverage, Task 2 must be revised before it
can be marked complete — the change must be visible in
the plan, not silently embedded in source as an
ignored test.

## Decisions

- **Fix order: bug first, then invariant.** Task 1 fixes
  `string_to_block_scalar` and updates fixtures; Task 2
  adds the invariant. This keeps every commit's CI green.
  The proof that the invariant catches the doubling bug
  lives in the plan and Task 2's completion notes — the
  developer manually verifies (revert Task 1's fix
  locally, run invariant, confirm it fails, restore
  fix) before submitting Task 2 for review.
- **Invariant lives as inline `rstest`, not as fixtures.**
  `tests/fixtures/CLAUDE.md` reserves the fixture format
  for visually self-explanatory cases. A property-count
  assertion across many actions is structural, not
  visual; expressing it as 21 fixtures (3 inputs × 7
  actions) would obscure the shared assertion shape.
  Inline `rstest` named cases follow project convention
  per `lang-rust-testing.md`.
- **`delete_anchor` excluded from the invariant.** Its
  purpose is to remove a property, so a "count must
  match" assertion would test the wrong thing. A
  separate, narrower invariant for `delete_anchor`
  ("removes exactly the targeted anchor; preserves all
  others") is out of scope here.
- **Surprise-failure scope discipline.** If Task 2's
  invariant flags an action besides
  `string_to_block_scalar`, fix in-plan only when the
  root cause matches Task 1's. Different root causes
  trigger an escalation to the lead — the developer
  cannot self-authorize an `#[ignore]` or open a
  follow-up unilaterally. This honors
  `claim-verification.md` (per-failure verification, no
  batch categorization) and
  `no-silent-target-weakening.md` (no quiet acceptance
  of partial results).
- **Helper duplication accepted but tracked.** Task 2
  introduces a second integration-test crate
  (`code_action_property_preservation.rs`) that
  re-implements the same `cursor_range` / `docs_for` /
  `test_uri` helpers already present in
  `code_action_fixtures.rs`. Consolidating these into a
  shared module is out of scope for this plan (the
  pattern is two callsites, not yet structurally
  costly). A follow-up entry filed during Task 2
  records the duplication so a third caller — for
  example the rename-fixture port already on the
  follow-up queue — triggers the consolidation work
  rather than adding a third copy.
- **Three new/updated fixtures, not four.** The combined
  case `&a !!str "..."` (anchor + core tag) is excluded
  from the new fixture set because the formatter strips
  `!!str` from non-empty scalars by design, so a fixture
  for that case would only restate the existing
  idempotency rule rather than testing property
  preservation. The combined case in the invariant is
  `&a !mytag` (anchor + *user* tag).

## Non-Goals

- Auditing or fixing `delete_anchor`'s property handling.
  A future plan can add its own narrower invariant.
- Refactoring how the formatter emits anchors/tags. The
  fix is local to the code-action site; the formatter's
  emission logic is correct given the inputs it receives.
- Expanding the invariant beyond cursor-driven mutating
  actions. Diagnostic-driven actions and rename/format
  paths are scope-separated work tracked in their own
  follow-up entries.
- Replacing the existing inline `block_scalar.rs` /
  `block_to_flow.rs` test modules with a different test
  organization. Test consolidation is independent.
- Consolidating duplicated integration-test helpers
  (`cursor_range`, `docs_for`, `test_uri`) shared by
  `code_action_fixtures.rs` and the new
  `code_action_property_preservation.rs`. The
  duplication is tracked via a follow-up entry filed
  during Task 2; consolidation lands when a third
  caller arrives.
