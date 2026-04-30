**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-30

## Goal

Bring per-action regression coverage for the four code
actions whose property-doubling bug was fixed in commit
`daf3d21` (`flow_to_block`, `quoted_bool`, `yaml11_bool`,
`yaml11_octal`) to parity with the coverage added for
`string_to_block_scalar` in commit `6e3abf3` — inline
unit tests in each action's source-file test module
asserting `new_text` contains zero property occurrences
and the spliced result contains exactly one, plus
property-preservation fixtures for the one cursor-driven
action of the four (`quoted_bool`). The cross-action
`code_action_property_preservation.rs` invariant
remains the structural cross-cutting gate; this plan
adds the placement/shape-level layer it cannot provide.

## Context

### Why the gap exists

The predecessor plan
(`2026-04-30-code-action-property-preservation-and-doubling-fix.md`,
commits `6e3abf3` and `daf3d21`) handled
`string_to_block_scalar` and the four other affected
actions asymmetrically:

- **`string_to_block_scalar` (Task 1, `6e3abf3`):** got
  three inline unit tests in `block_scalar.rs::tests`
  (one per property shape) and three fixtures
  (`block-scalar-preserve-anchor.md` corrected,
  `block-scalar-preserve-user-tag.md` and
  `block-scalar-preserve-anchor-and-user-tag.md` added).
- **`flow_to_block`, `quoted_bool`, `yaml11_bool`,
  `yaml11_octal` (Task 2, `daf3d21`):** the surprise-
  failure protocol fixed the bug, but the dispatch
  message instructed only the "strip-on-clone pattern
  matching Task 1" — the *test* pattern (fixtures + unit
  tests) was not carried over. The cross-action
  invariant in `code_action_property_preservation.rs`
  was deemed sufficient.

### Why the invariant alone is not sufficient

The three test layers are complementary, not
substitutable:

| Layer | Catches | Misses |
|---|---|---|
| **Inline unit test** in the action's source file | Internal contract: `new_text` itself contains zero property occurrences before splicing. Lives next to the buggy code. | Production-entry-point wiring is not exercised. |
| **Fixture** (`tests/fixtures/code_actions/*.md`) | Exact output shape — placement, ordering, surrounding whitespace. Visually self-explanatory regression artifact. Hits the production entry point. | Per-fixture maintenance; one input shape per file. |
| **Property-preservation invariant** (`tests/code_action_property_preservation.rs`) | Cross-action structural property: `count(input, prop) == count(output, prop)`. Pluggable when new actions are added. | Doesn't constrain placement — if `quoted_bool` started emitting `&anchor` *before* the value instead of after, the count would still match. |

The invariant catches count regressions; fixtures catch
shape regressions; unit tests catch internal-contract
regressions. The four newly-fixed actions currently have
only the count layer.

### Diagnostic-driven vs cursor-driven actions

Per `rlsp-yaml/tests/fixtures/CLAUDE.md`, the fixture
format is reserved for cursor-driven actions —
diagnostic-driven actions stay 100% inline because their
trigger state cannot be expressed in the fixture
frontmatter. Of the four actions in scope:

| Action | Driver | Fixture coverage applicable? |
|---|---|---|
| `flow_to_block` | Diagnostic (`flowMap`, `flowSeq`) | No — inline unit tests only |
| `quoted_bool` | Cursor | Yes — fixtures + inline unit tests |
| `yaml11_bool` | Diagnostic (`yaml11Bool`) | No — inline unit tests only |
| `yaml11_octal` | Diagnostic (`yaml11Octal`) | No — inline unit tests only |

This is not a scope reduction; it is a project-convention
constraint. Adding fixtures for diagnostic-driven actions
would violate the documented rule and produce fixtures
the harness cannot exercise.

### Coverage to add

| Action | Inline unit tests | Fixtures |
|---|---|---|
| `flow_to_block` (map dispatch) | 3 (anchor / user-tag / combined) | — |
| `flow_to_block` (seq dispatch) | 3 (anchor / user-tag / combined) | — |
| `quoted_bool` | 3 (anchor / user-tag / combined) | 3 (`quoted-bool-preserve-anchor.md`, `quoted-bool-preserve-user-tag.md`, `quoted-bool-preserve-anchor-and-user-tag.md`) |
| `yaml11_bool` | 3 (anchor / user-tag / combined) | — |
| `yaml11_octal` | 3 (anchor / user-tag / combined) | — |

Total: 15 inline unit tests + 3 fixtures.

The unit-test pattern mirrors `block_scalar.rs:182-242`
exactly:

```rust
#[test]
fn new_text_does_not_duplicate_anchor() {
    let text = "<input with &myanchor>";
    let (result, edit) = apply_<action>_edit(text, <line>);
    assert_eq!(count(&edit.new_text, "&myanchor"), 0);
    assert_eq!(count(&result, "&myanchor"), 1);
}
```

For `flow_to_block`, the existing `apply_flow_to_block_*`
helpers (or equivalents) in
`code_actions.rs::test_helpers` are the precedent. For
`quoted_bool`/`yaml11_bool`/`yaml11_octal`, the developer
adds one new helper per action mirroring
`apply_block_scalar_edit` shape if no equivalent already
exists.

### Key files

| File | Role |
|---|---|
| `rlsp-yaml/src/editing/code_actions/flow_to_block.rs` | Add 6 inline unit tests |
| `rlsp-yaml/src/editing/code_actions/quoted_bool.rs` | Add 3 inline unit tests |
| `rlsp-yaml/src/editing/code_actions/yaml11_bool.rs` | Add 3 inline unit tests |
| `rlsp-yaml/src/editing/code_actions/yaml11_octal.rs` | Add 3 inline unit tests |
| `rlsp-yaml/src/editing/code_actions.rs` | Add per-action `apply_<action>_edit` helpers if not already present (test_helpers module starts at line 150) |
| `rlsp-yaml/tests/fixtures/code_actions/quoted-bool-preserve-anchor.md` | New fixture |
| `rlsp-yaml/tests/fixtures/code_actions/quoted-bool-preserve-user-tag.md` | New fixture |
| `rlsp-yaml/tests/fixtures/code_actions/quoted-bool-preserve-anchor-and-user-tag.md` | New fixture |
| `rlsp-yaml/src/editing/code_actions/block_scalar.rs:182-242` | Pattern reference (read-only) |
| `rlsp-yaml/tests/fixtures/code_actions/block-scalar-preserve-*.md` | Pattern reference (read-only) |
| `rlsp-yaml/tests/fixtures/CLAUDE.md` | Cursor-driven vs diagnostic-driven rule (read-only) |

### References

- Predecessor plan:
  `.ai/plans/2026-04-30-code-action-property-preservation-and-doubling-fix.md`
  (Completed 2026-04-30) — origin of the bug fixes whose
  test coverage this plan backfills.
- Pattern source: `block_scalar.rs:182-242` for unit
  tests; `block-scalar-preserve-*.md` for fixtures.
- Fixture-vs-inline rule:
  `rlsp-yaml/tests/fixtures/CLAUDE.md` "When to Write a
  Fixture vs an Inline Test".

## Steps

- [ ] Task 1 — backfill inline unit tests + fixtures for
      the four actions

## Tasks

### Task 1: Backfill property-preservation unit tests + `quoted_bool` fixtures

For each of the four actions fixed in commit `daf3d21`,
add inline unit tests in the action's source-file
`#[cfg(test)] mod tests` module asserting that
`new_text` contains zero property occurrences and the
final spliced result contains exactly one, mirroring
`block_scalar.rs:182-242`. Add three property-preservation
fixtures for the one cursor-driven action
(`quoted_bool`), mirroring the
`block-scalar-preserve-*.md` set.

- [ ] In `rlsp-yaml/src/editing/code_actions/flow_to_block.rs`
      `mod tests`, add 6 unit tests covering both the
      map and sequence dispatch sites (3 property shapes
      × 2 sites). Each test asserts
      `count(edit.new_text, prop) == 0` and
      `count(result, prop) == 1` for the relevant
      property literal(s).
- [ ] In `rlsp-yaml/src/editing/code_actions/quoted_bool.rs`
      `mod tests`, add 3 unit tests (anchor / user-tag /
      combined) with the same shape.
- [ ] In `rlsp-yaml/src/editing/code_actions/yaml11_bool.rs`
      `mod tests`, add 3 unit tests with the same shape.
- [ ] In `rlsp-yaml/src/editing/code_actions/yaml11_octal.rs`
      `mod tests`, add 3 unit tests with the same shape.
- [ ] If a per-action `apply_<action>_edit` helper does
      not already exist in
      `rlsp-yaml/src/editing/code_actions.rs::test_helpers`
      (line 150 onward), add one mirroring the
      `apply_block_scalar_edit` shape (line 299). The
      helper accepts the source text and the trigger
      input (cursor line for cursor-driven; diagnostic
      shape for diagnostic-driven), runs the action
      through `code_actions(...)`, applies the first
      `TextEdit`, and returns `(full_result, edit)`.
- [ ] Add fixture
      `rlsp-yaml/tests/fixtures/code_actions/quoted-bool-preserve-anchor.md`
      with a quoted-boolean input carrying `&myanchor`
      (e.g., `enabled: &myanchor "true"`), `cursor:`
      positioned on the quoted scalar — determine the
      exact column from existing `quoted-bool-*.md`
      fixtures or from the trigger condition in
      `quoted_bool.rs` — `applies-action: unquoted`,
      and an `Expected-Document` containing exactly one
      `&myanchor` in front of the unquoted boolean.
- [ ] Add fixture
      `rlsp-yaml/tests/fixtures/code_actions/quoted-bool-preserve-user-tag.md`
      with a quoted-boolean input carrying `!mytag`
      (e.g., `enabled: !mytag "true"`), same shape, with
      exactly one `!mytag` in the expected output.
- [ ] Add fixture
      `rlsp-yaml/tests/fixtures/code_actions/quoted-bool-preserve-anchor-and-user-tag.md`
      with both `&a !mytag` properties present, with
      exactly one of each in the expected output in the
      correct order.
- [ ] `cargo test --workspace` passes (no regressions;
      the 15 new unit tests + 3 new fixtures all pass).
- [ ] `cargo clippy --all-targets` clean.
- [ ] `cargo fmt` applied.

Acceptance: `cargo test -p rlsp-yaml` shows the 15 new
unit tests passing across the four action source files;
`cargo test --test code_action_fixtures` shows the
three `quoted-bool-preserve-*.md` fixtures passing; the
existing
`tests/code_action_property_preservation.rs` invariant
remains green; `cargo test --workspace` is green.

## Decisions

- **Fixtures only for `quoted_bool`, not for the three
  diagnostic-driven actions.** Project convention in
  `rlsp-yaml/tests/fixtures/CLAUDE.md` reserves the
  fixture format for cursor-driven actions —
  diagnostic-driven actions cannot express their trigger
  state in fixture frontmatter. Adding fixtures for them
  would violate the rule and produce artifacts the
  harness cannot exercise. Inline unit tests are the
  documented home for diagnostic-driven coverage.
- **Inline unit tests for all four actions.** The
  invariant catches counts; fixtures catch placement
  but only for cursor-driven actions; unit tests
  catch internal contracts (`new_text` zero
  occurrences) regardless of action type, and live next
  to the buggy code so they are the most likely
  regression-detector after a future refactor. They are
  the smallest atomic check.
- **No helper consolidation in this plan.** The
  duplicated integration-test helpers tracked in
  `.ai/memory/project_followup_plans.md` (introduced
  when `tests/code_action_property_preservation.rs` was
  created in commit `4aa19e9`) wait for a third caller
  per the filed criterion. This plan adds inline unit tests
  inside source files (which use the existing
  `code_actions.rs::test_helpers` module) and fixtures
  (which consume the existing `code_action_fixtures.rs`
  harness) — neither adds a new integration-test crate,
  so the consolidation criterion is unmet by this work.
- **Single task, not split.** The four actions and the
  fixture additions are mechanical applications of the
  same pattern. Splitting per-action would multiply
  ceremony with no review-clarity benefit; the reviewer
  evaluates each test against the same template.

## Non-Goals

- Adding fixtures for the three diagnostic-driven
  actions (`flow_to_block`, `yaml11_bool`,
  `yaml11_octal`) — see Decisions.
- Auditing or fixing `delete_anchor`'s property
  handling. Same scope-out as the predecessor plan.
- Consolidating the duplicated integration-test helpers
  — separate concern, deferred to its own follow-up
  trigger.
- Extending the property-preservation invariant in
  `tests/code_action_property_preservation.rs`. The
  invariant is correct as it stands; this plan adds
  per-action layers, not cross-action ones.
