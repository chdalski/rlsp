**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-08

## Goal

Lift the `has_nested_collection_child` early-exit in the
`block_to_flow` code action so it produces valid flow
output for nested block collections — most importantly the
common Kubernetes/Docker-Compose shape `containers: [{name:
web, image: nginx}, …]` (a block sequence whose items are
block mappings), which today is silently unsupported. The
`block_to_flow` retrofit (commit landing the plan at
`.ai/plans/archive/2026-04-18-retrofit-block-to-flow-code-action.md`)
preserved the narrow nested-collection guard intentionally
to keep retrofit scope minimal; with the AST + formatter
path now mature, the guard is no longer load-bearing for
correctness — it only suppresses a usable feature.

## Context

### Verified before planning

A scratch verification (deleted before this plan was
written) established three facts that determine
implementation scope:

1. The formatter dispatches collection emission on each
   node's own `style` flag with no awareness of parent
   context — see `mapping_to_doc` at
   `rlsp-yaml/src/editing/formatter.rs:1135-1144` and
   `sequence_to_doc` at `rlsp-yaml/src/editing/formatter.rs:1514-1523`.
2. Flipping only the outer node's style to Flow while
   leaving inner Block-style children unchanged produces
   invalid YAML — empirically confirmed for both nested
   mapping and nested sequence inputs (reparse errors:
   `"missing comma between flow mapping entries"` and
   `"block sequence entry '-' is not allowed inside a
   flow collection"`).
3. Recursive style-flip on the cloned subtree (every
   `Mapping`/`Sequence` flag set to `Flow`) produces
   valid, re-parseable flow output.

The asymmetry with the existing `flow_to_block.rs:32`
(which only flips the outer node and works correctly) is
because Block-with-Flow-children is valid YAML, while
Flow-with-Block-children is not.

### Pre-existing reference for the recursive-flip pattern

The unit test FS-13 at `rlsp-yaml/src/editing/formatter.rs:2515-2539`
already exercises the recursive-flip pattern for the
symmetric direction (nested-flow-in-flow-sequence flipped
to block). The comment at line 2517 — "mimics Task 2's
approach" — establishes recursive style mutation as the
production approach this plan extends to `block_to_flow`.

### Files involved

- **Primary target:** `rlsp-yaml/src/editing/code_actions/block_to_flow.rs`
  - Lines 27-29 (early-exit guard to remove)
  - Lines 31-37 (style mutation to replace with recursive helper call)
  - Lines 180-186 (`has_nested_collection_child` — becomes dead)
  - Lines 197-238 (existing `#[cfg(test)] mod tests` — add re-parseability tests for new cases)
- **Test fixtures:** `rlsp-yaml/tests/fixtures/code_actions/block-to-flow-*.md`
  - Cursor-driven actions belong in fixtures per
    `tests/fixtures/CLAUDE.md` ("Use a fixture when the
    behavior is visually self-explanatory").
  - Two existing fixtures already cover non-nested cases:
    `block-to-flow-wraps-long-output.md`,
    `block-to-flow-respects-configured-print-width.md`.
- **Fixture harness:** `rlsp-yaml/tests/code_action_fixtures.rs`
  (the production LSP entry point — fixtures exercise
  `code_actions(...)` end-to-end).

### Specifications

- YAML 1.2.2 §7.4 (Flow Collection Styles) — flow
  collections (`{}`, `[]`) cannot contain block-style
  children; this is the correctness reason recursive
  style-flip is required.

## Steps

- [ ] Add `flip_to_flow` recursive helper in `block_to_flow.rs`
- [ ] Replace the early-exit guard with the recursive flip call
- [ ] Remove the now-dead `has_nested_collection_child` (after
      grepping for other call sites)
- [ ] Delete the three `omits-action` fixtures whose
      asserted absence is being inverted by this change:
      `block-to-flow-nested-structures-omits.md`,
      `block-to-flow-sequence-item-is-nested-sequence-omits.md`,
      `block-to-flow-sequence-items-are-mappings-omits.md`
- [ ] Add fixture: nested mapping in mapping value
- [ ] Add fixture: sequence-of-mappings (Kubernetes
      `containers:` shape)
- [ ] Add fixture: nested sequence in mapping value
- [ ] Add fixture: deep nesting (≥3 levels)
- [ ] Add fixture: anchors/tags on inner nested collection
- [ ] Add inline re-parseability test: nested mapping
- [ ] Add inline re-parseability test: nested sequence
- [ ] Add user-facing entry to `rlsp-yaml/docs/feature-log.md`
- [ ] Run `cargo fmt`, `cargo clippy --all-targets`,
      `cargo test`

## Tasks

### Task 1: Recursive style-flip for nested block_to_flow

Lift the `has_nested_collection_child` early-exit in
`block_to_flow.rs` and replace the outer-only style
mutation with a recursive helper that flips every
`Mapping`/`Sequence` in the cloned subtree to
`CollectionStyle::Flow`. This makes the action work on
nested block inputs while preserving its existing
behavior on non-nested inputs.

The change is structurally a single vertical slice: a
small production-side helper, the guard removal, dead-code
cleanup, and a test sweep through both the existing
fixture harness (the LSP production entry point) and the
existing inline re-parseability test pattern.

#### Production code

- [ ] Add `flip_to_flow(&mut Node<Span>)` private helper
      in `block_to_flow.rs` that walks the subtree and
      sets `*style = CollectionStyle::Flow` on every
      `Mapping` and `Sequence`, recursing into entries
      and items. Scalars and aliases are leaves.
- [ ] Remove the early-exit guard at
      `block_to_flow.rs:27-29` (the
      `has_nested_collection_child(node)` check).
- [ ] Replace the outer-only style mutation at
      `block_to_flow.rs:31-37` with a Mapping/Sequence
      type-check (returning `None` for Scalar/Alias)
      followed by a single `flip_to_flow(&mut flow_node)`
      call.
- [ ] Grep the workspace for `has_nested_collection_child`
      to confirm `block_to_flow.rs` is the only caller,
      then delete the function at
      `block_to_flow.rs:180-186`.

#### Test fixtures (cursor-driven; live in `tests/fixtures/code_actions/`)

Each fixture is one `.md` file with frontmatter
(`cursor: 0:0`, `applies-action: Convert block to flow style`),
a `## Test-Document` block, and a `## Expected-Document`
block. The fixture harness at
`tests/code_action_fixtures.rs` runs them through the
LSP code-actions entry point — this satisfies the
integration-test requirement in `integration-testing.md`.

Three existing `omits-action` fixtures assert the action
is absent for inputs this plan teaches it to handle —
they must be deleted as part of this task or the
`omits-action` assertions will fail when the action is
now offered:

- [ ] Delete `tests/fixtures/code_actions/block-to-flow-nested-structures-omits.md`
- [ ] Delete `tests/fixtures/code_actions/block-to-flow-sequence-item-is-nested-sequence-omits.md`
- [ ] Delete `tests/fixtures/code_actions/block-to-flow-sequence-items-are-mappings-omits.md`

New fixtures to add (each replaces one of the deleted
omits cases with a concrete `applies-action` assertion):

- [ ] `block-to-flow-nested-mapping.md` — input:
      `outer:\n  inner:\n    a: 1\n    b: 2`; expected:
      `outer: {inner: {a: 1, b: 2}}`.
- [ ] `block-to-flow-sequence-of-mappings.md` — input:
      Kubernetes-style `containers:` with two block
      mappings as items; expected: flow output preserving
      every key/value across both items.
- [ ] `block-to-flow-nested-sequence.md` — input: block
      sequence whose items are block sequences (e.g.
      `items:\n  - - a\n    - b\n  - - c\n    - d`);
      expected: `items: [[a, b], [c, d]]`.
- [ ] `block-to-flow-deep-nesting.md` — input with 3
      levels of block nesting; expected: a single flow
      output with all three levels flow-flattened.
- [ ] `block-to-flow-anchor-on-inner.md` — input where
      an inner nested collection carries an anchor or
      tag; expected: the inner anchor/tag re-emits in
      flow context exactly once.

#### Inline tests (re-parseability assertions; live in `block_to_flow.rs`'s test module)

The existing inline tests at `block_to_flow.rs:204-237`
follow Pattern C from `tests/fixtures/CLAUDE.md`
(re-parseability assertion on the applied edit). Two new
inline tests extend that pattern to nested cases:

- [ ] `should_produce_reparseable_yaml_when_nested_mapping_converts`
      — applies the action to a nested-mapping input and
      asserts `parse_yaml(&result).diagnostics.is_empty()`
      and `documents.len() == 1`.
- [ ] `should_produce_reparseable_yaml_when_nested_sequence_converts`
      — same shape, nested-sequence input.

#### User-facing documentation

The change is user-visible — the `Convert block to flow
style` action now appears for inputs it previously
silently skipped (any block collection with nested block
children, including the common Kubernetes `containers:`
shape). Per the project `CLAUDE.md`, user-facing behavior
changes belong in `feature-log.md`.

- [ ] Add a `feature-log.md` entry titled "Block-to-Flow
      Code Action Handles Nested Collections [completed]"
      describing: (a) the previously-unsupported input
      shapes the action now handles (mapping-in-mapping,
      sequence-of-mappings, sequence-of-sequences, deep
      nesting), (b) one concrete user-facing example
      (the Kubernetes `containers:` shape), (c) Tier 2.

#### Acceptance criteria

- [ ] `flip_to_flow` recursive helper added; outer-only
      style mutation replaced with a single call to it.
- [ ] `has_nested_collection_child` deleted (no remaining
      callers).
- [ ] Three obsolete `omits-action` fixtures deleted
      (`block-to-flow-nested-structures-omits.md`,
      `block-to-flow-sequence-item-is-nested-sequence-omits.md`,
      `block-to-flow-sequence-items-are-mappings-omits.md`).
- [ ] Five new fixtures present and passing under
      `cargo test --test code_action_fixtures`.
- [ ] Two new inline re-parseability tests passing under
      `cargo test --lib -p rlsp-yaml`.
- [ ] New entry present in `rlsp-yaml/docs/feature-log.md`
      describing the user-visible change.
- [ ] `cargo fmt --check` clean.
- [ ] `cargo clippy --all-targets` zero warnings.
- [ ] `cargo test` green across the workspace.

## Decisions

- **Recursive style-flip** (not an enforce-block-style-style
  formatter option) — the change is local to the action's
  cloned subtree; the formatter and AST are not modified.
  This keeps the action's behavior change scoped to
  itself and avoids a global formatter knob.
- **Comments inside flipped subtree are not preserved** —
  consistent with the symmetric `flow_to_block` action
  (no special comment handling). The action is a
  user-invoked transformation; the user opts into the
  trade-off when they invoke it. No warning UI.
- **Anchor/tag preservation on inner nested collections**
  passes through via the existing `flow_item_to_doc` →
  `node_to_doc` path; no dedicated logic. The
  `block-to-flow-anchor-on-inner.md` fixture verifies
  this empirically.
- **Tests use the existing fixture harness** for cursor-
  driven cases (preferred per
  `tests/fixtures/CLAUDE.md`) and the existing inline
  Pattern C for re-parseability assertions (which the
  fixture format does not support).

## Non-Goals

- **Auto-wrap of long flow output** — the existing
  print-width-aware wrapping in the formatter (already
  exercised by the `block-to-flow-wraps-long-output.md`
  fixture) covers nested cases too. No new wrapping
  logic in scope.
- **`autoWrapFlowStyle` user-configurable opt-out** —
  separate follow-up item, deferred until user demand.
- **`block_to_flow` policy interaction with
  `formatEnforceBlockStyle`** — separate follow-up item;
  this plan preserves current policy semantics.
- **Symmetric expansion of `string_to_block_scalar` to
  sequence-item scalars** — separate follow-up item.
- **Formatter changes** — the formatter is not modified
  by this plan. Only the code action is.
