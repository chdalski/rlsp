**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-13

## Goal

Clean up test categorization in `rlsp-yaml-parser/tests/`.
Tests that were added as ad-hoc regressions in
`conformance.rs` are duplicates of tests already in
`loader.rs`, and `loader.rs` contains DoS-limit tests that
duplicate more thorough coverage in `robustness.rs`. Remove
the duplicates and move misplaced tests to their correct
homes so each test file has a single, clear responsibility.

## Context

An audit of all test files in `rlsp-yaml-parser/tests/`
found three categories of issues:

### 1. `conformance.rs` contains non-conformance tests

`conformance.rs` should be exclusively the yaml-test-suite
data-driven runner. Lines 262–697 contain three groups of
hand-written regression tests that don't use yaml-test-suite
data:

- **Anchor/tag before mapping key** (lines 274–417) —
  4 tests asserting `Node` tree structure via `loader::load`.
  All 4 are duplicated in `loader.rs` Group I (IT-I-1
  through IT-I-4) with identical inputs and assertions.

- **Quoted-key regressions** (lines 419–638) — 10 tests.
  2 are event-layer tests (`quoted_key_parse_events_style_*`)
  and 8 are loader-layer tests. No exact duplicates exist
  elsewhere, but they belong in `smoke/quoted_scalars.rs`
  (event tests) and `loader.rs` (loader tests).

- **Comment attachment regressions** (lines 640–697) —
  3 tests asserting `Node::trailing_comment()` and
  `Node::leading_comments()`. All 3 are duplicated in
  `loader.rs` Group J (IT-J1, IT-J3, IT-J5).

### 2. `loader.rs` Group F duplicates `robustness.rs`

`loader.rs` tests IT-20 through IT-24 (Group F: DoS limits)
test the same `LoaderBuilder` security limits as
`robustness.rs` Groups A–C:

| `loader.rs` | `robustness.rs` |
|---|---|
| `it_20_nesting_depth_limit_triggers` | `nesting_depth_limit_triggers_at_configured_threshold` |
| `it_21_nesting_at_exact_limit_succeeds` | `nesting_depth_within_limit_succeeds` |
| `it_22_anchor_count_limit_returns_error` | `anchor_count_limit_triggers_at_configured_threshold` |
| `it_23_alias_expansion_limit_returns_error` | `alias_bomb_triggers_expansion_limit` |
| `it_24_circular_alias_detection` | `circular_alias_*` (3 tests) |

`robustness.rs` is the correct home — it has more thorough
stress-focused variants. `loader.rs` Group F is redundant.

### Correctly categorized files (no changes needed)

| File | Purpose |
|------|---------|
| `encoding.rs` | `decode()` + encoding edge cases |
| `error_reporting.rs` | Error detection, positions, recovery |
| `unicode_positions.rs` | Span positions with multi-byte UTF-8 |
| `loader_spans.rs` | Container node span coverage |
| `robustness.rs` | Security limits and stress tests |
| `smoke/*` (16 modules) | Event-level behavior by grammar area |

### Key files

- `rlsp-yaml-parser/tests/conformance.rs`
- `rlsp-yaml-parser/tests/loader.rs`
- `rlsp-yaml-parser/tests/robustness.rs`
- `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

## Steps

- [ ] Delete 7 duplicate tests from `conformance.rs`
- [ ] Move 10 quoted-key tests from `conformance.rs` to
      their correct homes
- [ ] Delete 5 duplicate DoS-limit tests from `loader.rs`
- [ ] Verify all tests pass after changes

## Tasks

### Task 1: Remove duplicates from `conformance.rs`

Delete the 7 tests that are exact duplicates of tests in
`loader.rs`:

**Anchor/tag duplicates (delete from `conformance.rs`):**
- `anchor_before_mapping_key_root_level` (line 279) —
  dup of `loader.rs::inline_anchor_before_key_annotates_key_scalar_root`
- `anchor_before_mapping_key_indented` (line 311) —
  dup of `loader.rs::inline_anchor_before_key_annotates_key_scalar_indented`
- `tag_before_mapping_key_root_level` (line 354) —
  dup of `loader.rs::inline_tag_before_key_annotates_key_scalar`
- `anchor_and_tag_before_mapping_key` (line 383) —
  dup of `loader.rs::inline_anchor_and_tag_before_key_annotate_key_scalar`

**Comment attachment duplicates (delete from `conformance.rs`):**
- `trailing_comment_on_mapping_value_attached` (line 645) —
  dup of `loader.rs::it_j1_trailing_comment_on_mapping_value_is_attached`
- `trailing_comment_on_sequence_item_attached` (line 663) —
  dup of `loader.rs::it_j3_trailing_comment_on_sequence_item_is_attached`
- `leading_comment_attached_to_second_mapping_key` (line 681) —
  dup of `loader.rs::it_j5_leading_comment_still_works_after_fix`

Also delete the section comment blocks that grouped these
tests. After deletion, `conformance.rs` should contain only
the yaml-test-suite driver (`yaml_test_suite` function) and
the quoted-key tests (moved in Task 2).

- [ ] Delete 4 anchor/tag duplicate tests and their section
      comment
- [ ] Delete 3 comment attachment duplicate tests and their
      section comment
- [ ] Run `cargo test` to verify nothing broke

### Task 2: Move quoted-key tests to correct homes

Move the 10 quoted-key regression tests from
`conformance.rs` (lines 419–638) to their correct files:

**Event-layer tests → `smoke/quoted_scalars.rs`:**
- `quoted_key_parse_events_style_double` (line 424)
- `quoted_key_parse_events_style_single` (line 444)

These test that `parse_events` emits the correct
`ScalarStyle` for quoted mapping keys. They use only
`parse_events` and `Event`/`ScalarStyle` — no loader. They
fit the smoke test pattern of asserting event-level behavior.

**Loader-layer tests → `loader.rs`:**
- `quoted_key_double_quoted_simple` (line 466)
- `quoted_key_single_quoted_simple` (line 488)
- `quoted_key_double_quoted_with_escape_sequence` (line 510)
- `quoted_key_single_quoted_with_escaped_quote` (line 529)
- `quoted_key_with_spaces_inside` (line 548)
- `quoted_key_double_quoted_empty` (line 566)
- `quoted_key_in_nested_mapping` (line 584)
- `quoted_key_multiple_entries_mixed` (line 611)

These test `loader::load` and assert on `Node::Scalar`
with specific `ScalarStyle` values. They belong in
`loader.rs` alongside the existing Group I tests that also
test property placement on mapping keys.

After this task, `conformance.rs` should contain only the
yaml-test-suite driver — no hand-written tests.

- [ ] Move 2 event-layer tests to `smoke/quoted_scalars.rs`
- [ ] Move 8 loader-layer tests to `loader.rs`
- [ ] Remove the section comment block from `conformance.rs`
- [ ] Remove stale `use` imports from `conformance.rs`
- [ ] Run `cargo test` to verify nothing broke

### Task 3: Remove duplicate DoS-limit tests from `loader.rs`

Delete `loader.rs` Group F (5 tests) that duplicate
`robustness.rs`:

- `it_20_nesting_depth_limit_triggers_at_configured_threshold`
- `it_21_nesting_at_exact_limit_succeeds`
- `it_22_anchor_count_limit_returns_error`
- `it_23_alias_expansion_limit_returns_error`
- `it_24_circular_alias_detection_in_resolved_mode`

Also delete the Group F section comment. After deletion,
`loader.rs` Groups A–E and G–J remain.

- [ ] Delete 5 DoS-limit tests and their section comment
- [ ] Remove stale `use` imports if any become unused
- [ ] Run `cargo test` to verify nothing broke

### Task 4: Final verification

- [ ] Run `cargo test` — all tests pass
- [ ] Run `cargo clippy --all-targets` — zero warnings
- [ ] Verify `conformance.rs` contains only the
      yaml-test-suite driver (no hand-written tests)
- [ ] Verify no test was lost (moved tests exist in new
      locations)

## Decisions

- **Delete duplicates rather than consolidate.** When two
  tests assert the same behavior with the same input, the
  one in the correct file stays and the misplaced one is
  deleted. No value in keeping both.
- **`robustness.rs` is the canonical home for DoS-limit
  tests.** It's more thorough (tests boundary conditions,
  has more stress variants) and its file name clearly
  communicates intent. `loader.rs` tests basic loader API
  functionality — security limits are a cross-cutting
  concern that belongs in the dedicated robustness file.
- **Quoted-key event tests go to `smoke/quoted_scalars.rs`,
  not a new file.** They test quoted scalar behavior in
  mapping key context — this is a specialization of quoted
  scalar handling, not a new category.
