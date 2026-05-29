**Repository:** root
**Status:** Completed (2026-05-11)
**Created:** 2026-05-11

## Goal

Add a second code action "Convert to folded block scalar"
alongside the existing "Convert to block scalar" (literal).
When a qualifying scalar is on the cursor line, the user
sees two quick-fix options: literal (`|`) and folded (`>`).
The folded form is better for prose where line breaks should
be collapsed into spaces.

## Context

- The `string_to_block_scalar` code action in
  `rlsp-yaml/src/editing/code_actions/block_scalar.rs`
  currently emits a single `CodeAction` titled "Convert to
  block scalar" that converts to `ScalarStyle::Literal(Chomp::Clip)`.
- The function `string_to_block_scalar()` (line 12–68)
  finds a candidate scalar, clones it, sets the style to
  `Literal(Clip)`, formats via `format_subtree()`, and
  returns a single `CodeAction`.
- The qualifying conditions (line 96–110): block mapping
  value (or sequence item, if the sequence-item plan lands
  first), `Plain`/`SingleQuoted`/`DoubleQuoted` style,
  cursor on the scalar's line, >= 40 characters.
- The formatter already handles `ScalarStyle::Folded(_)`
  through the same `repr_block_to_doc()` path as literal
  (line 576–608 in `formatter.rs`). No formatter changes
  are needed.
- The dispatcher in `code_actions.rs` (line 89) calls
  `string_to_block_scalar()` which returns
  `Option<CodeAction>`. To return two actions, the return
  type must change to `Vec<CodeAction>` (or the dispatcher
  calls two functions).
- The `code_actions()` function (the dispatcher) collects
  actions into a `Vec<CodeAction>` via
  `.into_iter().flatten()` on each action source. Changing
  the block scalar function to return a `Vec<CodeAction>`
  and flattening it matches the existing collection
  pattern.

### Readers of changed code paths

- `code_actions()` in `code_actions.rs` — the dispatcher
  that calls `string_to_block_scalar()`. Must adapt to the
  new return type.
- `string_to_block_scalar()` — the function being modified
  to produce two actions instead of one.
- `find_block_scalar_candidate()` and
  `find_block_scalar_in_node()` — unchanged; they find the
  candidate scalar, which is shared by both actions.
- Fixture harness `tests/code_action_fixtures.rs` — reads
  fixtures from `tests/fixtures/code_actions/`. The
  harness is unchanged.
- Inline tests in `block_scalar.rs` (line 138–243) — all
  use `apply_block_scalar_edit()` helper from
  `test_helpers`. That helper calls
  `string_to_block_scalar()` and expects a single action.
  It must be updated to select the literal action from the
  returned vector (or a new helper added for folded).
- The existing fixture `block-scalar-title-is-exact.md`
  asserts the action title is exactly "Convert to block
  scalar" via `applies-action: Convert to block scalar`.
  After this change, two actions match the substring
  "block scalar". The fixture must be updated to use the
  new literal-specific title.
- All existing `applies-action: block scalar` fixtures
  will match both actions. The harness picks the first
  match. We need to update fixture `applies-action` values
  to use the specific title for literal, or verify that
  the harness consistently picks the right one.

### Fixture title disambiguation

The harness in `code_action_fixtures.rs` finds the first
action whose title contains the `applies-action` substring.
With two actions ("Convert to block scalar (literal)" and
"Convert to block scalar (folded)"), a fixture with
`applies-action: block scalar` would match whichever comes
first. To avoid ambiguity:
- Existing fixtures keep `applies-action: literal` (matches
  only the literal action title)
- New folded fixtures use `applies-action: folded`

## Steps

- [x] Refactor `string_to_block_scalar()` to return two
  actions (literal and folded)
- [x] Update the dispatcher to handle the new return type
- [x] Update existing fixtures and inline tests for the
  new title
- [x] Add folded-specific test fixtures
- [x] Verify all tests pass

## Tasks

### Task 1: Add folded block scalar code action

**Commit:** `55efe504b2f6278f9e751de336e8a2247b8b17d2`

Modify `string_to_block_scalar()` to return both literal
and folded actions. Update the dispatcher, existing tests,
and add new fixtures.

- [x] Change `string_to_block_scalar()` to return
  `Vec<CodeAction>` instead of `Option<CodeAction>`. Find
  the candidate once, then produce two actions: one with
  `ScalarStyle::Literal(Chomp::Clip)` titled "Convert to
  block scalar (literal)" and one with
  `ScalarStyle::Folded(Chomp::Clip)` titled "Convert to
  block scalar (folded)". If the candidate is `None` or
  either formatted text is empty, omit that action from
  the vector.
- [x] In `code_actions.rs`, update the dispatcher call at
  line 89 to handle `Vec<CodeAction>` — extend the
  collection with the returned vector instead of using
  `.into_iter().flatten()` on an `Option`.
- [x] In `apply_block_scalar_edit()` (in
  `code_actions.rs` test_helpers module), replace the
  `.find(|a| a.title.contains("block scalar"))` predicate
  with `.find(|a| a.title.contains("literal"))` so the
  helper selects the literal action from the two-element
  vector. Also update the helper's doc comment from
  "Convert to block scalar" to "Convert to block scalar
  (literal)". Existing inline tests continue to test the
  literal path.
- [x] Update the existing fixture
  `block-scalar-title-is-exact.md`: change the
  `applies-action` frontmatter field to match the new
  literal title, and update the body heading
  (`# Test: Action title is exactly "..."`) to reflect
  the new title "Convert to block scalar (literal)".
- [x] Update all existing `applies-action` fixtures that
  use `applies-action: Convert to block scalar` or
  `applies-action: block scalar` to use
  `applies-action: literal` so they unambiguously match
  the literal action.
- [x] Existing `omits-action: block scalar` fixtures
  require no changes — the substring "block scalar" appears
  in both new titles ("Convert to block scalar (literal)"
  and "Convert to block scalar (folded)"), so the harness's
  existing assertion (no action title contains the
  substring) correctly verifies both actions are absent.
- [x] Add fixture
  `block-scalar-folded-converts-long-string.md` — same
  input as `block-scalar-converts-long-string.md` but
  with `applies-action: folded`. Expected-Document shows
  `>` header with folded content.
- [x] Add fixture
  `block-scalar-folded-plain-scalar.md` — a plain scalar
  mapping value >= 40 chars with
  `applies-action: folded`.
- [x] Add fixture
  `block-scalar-folded-title-is-exact.md` — verifies
  the folded action title is exactly "Convert to block
  scalar (folded)".
- [x] `cargo fmt` produces no diff
- [x] `cargo clippy --all-targets` reports zero warnings
- [x] `cargo test -p rlsp-yaml` passes with zero failures
- [x] All existing block-scalar fixtures pass with
  updated `applies-action` values

## Non-Goals

- Changing the folded block scalar formatting behavior in
  the formatter — `repr_block_to_doc()` already handles
  folded scalars.
- Adding a user setting to choose a default block scalar
  style — the action offers both; the user picks at apply
  time.
- Offering folded for sequence items — if the
  sequence-item plan has not landed yet, both plans are
  independent. If it has landed, the folded action
  naturally covers sequence items too since it uses the
  same candidate-finding logic.

## Decisions

- **Two separate actions, not a setting** — offering both
  "literal" and "folded" as distinct quick-fix items lets
  the user choose per-instance without a global config.
  This matches how VS Code presents refactoring options.
- **Parenthetical title suffix** — "Convert to block
  scalar (literal)" and "Convert to block scalar (folded)"
  keep the actions visually grouped in the quick-fix menu
  while being distinguishable.
- **Single task** — the change is ~20 lines of production
  code, fixture updates, and 3 new fixtures. One commit.
- **Executes after the sequence-item plan** — the
  sequence-item plan adds new fixtures with the current
  title "Convert to block scalar". This plan renames all
  fixtures (including those new ones) to use
  `applies-action: literal`. Executing in this order
  avoids writing fixtures with a title that doesn't exist
  yet.
- **No separate consolidation plan needed** — both plans
  target `block_scalar.rs` but touch different parts: the
  sequence-item plan extends `find_block_scalar_in_node()`
  (the candidate finder), while this plan modifies
  `string_to_block_scalar()` (the action builder). After
  both land, the candidate-finding logic is shared by both
  literal and folded actions with no duplication. The
  folded fixtures test distinct output shapes (not
  duplicates of literal fixtures), so no pruning is needed.
