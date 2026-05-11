**Repository:** root
**Status:** InProgress
**Created:** 2026-05-11

## Goal

Extend the `string_to_block_scalar` code action to offer
conversion on long scalar values that are direct items of
block-style sequences, not only mapping values. A user with
`- "this is a very long sequence item value that exceeds
forty characters"` currently gets no block-scalar action;
after this change, the action is offered and produces
correct output.

## Context

- The `string_to_block_scalar` code action lives in
  `rlsp-yaml/src/editing/code_actions/block_scalar.rs`.
- `find_block_scalar_in_node()` (line 86–136) walks the
  AST. The `Node::Mapping` arm (line 92–124) qualifies
  direct scalar values of block mappings when:
  1. The mapping is `CollectionStyle::Block`
  2. The scalar is `Plain`, `SingleQuoted`, or
     `DoubleQuoted`
  3. The scalar starts on the cursor's line
  4. The decoded value has >= 40 characters
- The `Node::Sequence` arm (line 126–132) only recurses
  into items — it never qualifies direct sequence-item
  scalars.
- The existing fixture
  `block-scalar-sequence-item-omits.md` asserts the
  action is NOT offered for sequence items. This fixture
  must be replaced with an applies-action fixture.
- The formatter already handles block scalars inside
  sequences correctly — the fixture
  `tests/fixtures/formatter/block-scalar-nested-in-sequence.md`
  confirms idempotent output for `- |\n  content`.
- `format_subtree()` takes a `base_indent` parameter that
  controls continuation-line indentation. For mapping
  values, `base_indent` is the key's column. For sequence
  items, the equivalent is the item's indentation level
  (column of the `-` dash).
- The function returns
  `(scalar_node, key_col, scalar_loc, line_index)` where
  `key_col` is currently always a mapping key's column.
  For sequence items, this becomes the sequence item's
  column (the dash position).
- `node_loc()` (imported from `block_to_flow`) extracts a
  node's location span. For a sequence item, the item IS
  the scalar node itself (`Node::Sequence.items` is
  `Vec<Node>` — no wrapper node). The scalar's `loc.start`
  gives the scalar's start position (after `- `). To get
  the dash column (needed for `base_indent`), use
  `idx.line_column(loc.start).1` to get the scalar's
  column, then subtract 2 (for `- ` prefix). Alternatively,
  since the sequence node itself is available in the match
  arm, the sequence's own `loc.start` column can serve as
  the base indent for items at the top level — but for
  nested sequences, the item's own line start is more
  reliable. The simplest correct approach: for a sequence
  item scalar, `base_indent` = the scalar's start column
  minus 2 (the `- ` prefix width). This mirrors how
  mapping keys work: `key_col` is the key's column, and
  the block scalar indentation follows the key's column.

### Readers of changed code paths

- `string_to_block_scalar()` (line 12–68) — consumes the
  candidate tuple. The `key_col` field becomes
  `base_indent` at line 22. No change needed here; the
  tuple shape is unchanged.
- `find_block_scalar_candidate()` (line 73–84) — iterates
  documents, delegates to `find_block_scalar_in_node()`.
  No change needed.
- `find_block_scalar_in_node()` (line 86–136) — **the
  function being modified**. Its callers (above two, plus
  the recursive calls within itself) are all internal to
  this file.
- Fixture harness `tests/code_action_fixtures.rs` — reads
  fixtures from `tests/fixtures/code_actions/`. The
  harness is unchanged; only fixture files change.
- Inline tests in `block_scalar.rs` (line 138–243) — all
  use `apply_block_scalar_edit()` which calls
  `string_to_block_scalar()` with mapping-value YAML.
  Unchanged.

## Steps

- [x] Extend `find_block_scalar_in_node()` to qualify
  sequence-item scalars
- [x] Replace the omit fixture with an applies-action
  fixture
- [x] Add additional sequence-item test fixtures
- [x] Verify all tests pass

## Tasks

### Task 1: Extend string_to_block_scalar to sequence items

**Commit:** `e3b46f24b419539054f22da4f58efb4edf482939`

Modify `find_block_scalar_in_node()` in
`rlsp-yaml/src/editing/code_actions/block_scalar.rs` to
also qualify scalars that are direct items of block-style
sequences. Add test fixtures covering the new behavior.

- [x] In `find_block_scalar_in_node()`, modify the
  `Node::Sequence` match arm (line 126–132) to check
  whether each item is a qualifying scalar before
  recursing. The qualifying conditions are the same as
  for mapping values: `Plain`/`SingleQuoted`/`DoubleQuoted`
  style, on the cursor's line, >= 40 characters. The
  sequence must be `CollectionStyle::Block`.
- [x] For sequence-item scalars, compute `base_indent` as
  `idx.line_column(loc.start).1 as usize - 2` — the
  scalar's start column minus 2 for the `- ` prefix. This
  gives the dash column, which `format_subtree()` uses to
  indent continuation lines of the block scalar. This
  mirrors the mapping-value pattern where `base_indent` is
  the key's column (line 112).
- [x] Delete the existing
  `block-scalar-sequence-item-omits.md` fixture (it
  asserted the action was NOT offered).
- [x] Add fixture
  `block-scalar-sequence-item-converts.md` — a plain
  sequence with a long double-quoted item. `cursor` on
  the item line, `applies-action: Convert to block
  scalar`. Verify the Expected-Document shows correct
  `- |\n  content` output.
- [x] Add fixture
  `block-scalar-sequence-item-plain-converts.md` — a
  plain scalar sequence item (unquoted) >= 40 chars.
- [x] Add fixture
  `block-scalar-sequence-item-nested-in-mapping-converts.md`
  — a mapping value that is a block sequence, with a long
  scalar item inside. Verifies the recursive walk finds
  it.
- [x] Add fixture
  `block-scalar-sequence-item-in-flow-sequence-omits.md`
  — a long scalar inside a flow-style sequence `[...]`.
  Asserts the action is NOT offered (flow sequences
  excluded, matching the flow-mapping exclusion).
- [x] Add fixture
  `block-scalar-sequence-item-short-omits.md` — a
  sequence item scalar below 40 characters. Asserts
  the action is NOT offered.
- [x] `cargo fmt` produces no diff
- [x] `cargo clippy --all-targets` reports zero warnings
- [x] `cargo test -p rlsp-yaml` passes with zero failures
- [x] The existing block-scalar fixtures (14 applies + 11
  remaining omits after deleting the sequence-item-omits
  fixture) continue to pass unchanged

## Non-Goals

- Offering a folded block scalar (`>`) alternative — that
  is a separate plan.
- Modifying the formatter — the formatter already handles
  block scalars in sequence items correctly.
- Changing the 40-character threshold — the threshold is
  shared across all contexts and is not being revisited.

## Decisions

- **Same qualifying conditions as mapping values** — the
  40-char threshold, style filter, and block-collection
  requirement apply identically. No special casing for
  sequence items.
- **`base_indent` = scalar column − 2** — for sequence
  items, the scalar starts after `- ` (2 chars). Subtracting
  2 gives the dash column, which is the natural indent base
  — matching how mapping-key columns work for mapping values.
- **Executes before the folded-scalar plan** — this plan
  adds new fixtures with the current action title "Convert
  to block scalar". The folded-scalar plan will rename the
  title to "Convert to block scalar (literal)" and update
  all fixtures including these new ones. This ordering is
  simpler than writing fixtures with a title that doesn't
  exist yet.
- **Single task** — the change is ~15 lines of production
  code and 5 fixtures. Splitting into multiple tasks would
  add coordination overhead disproportionate to the work.
