**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-11

## Goal

Suppress the "Convert block to flow style" code action when
`formatEnforceBlockStyle: true` — the formatter will
immediately revert the conversion on save, making the action
pointless and confusing.

## Context

- `block_to_flow` in
  `rlsp-yaml/src/editing/code_actions/block_to_flow.rs:12`
  receives `options: &YamlFormatOptions` but never checks
  `format_enforce_block_style`. The action is always
  offered regardless of the setting.
- The formatter checks `format_enforce_block_style` at 7
  locations in `formatter.rs` and forces all collections to
  block style when `true`.
- The action only targets mapping values (not root
  mappings) — see `find_innermost_block_in_node()` at
  line 132. So the conflict is: user applies the action on
  a mapping value, gets flow output, saves, formatter
  reverts it.
- `docs/configuration.md:126` documents
  `formatEnforceBlockStyle` and cross-references it at
  line 278 in the `flowStyle` section.
- This conflict was recorded as an open policy question in
  the project follow-up queue. This plan resolves it with
  the "suppress" answer.

## Steps

- [ ] Add early return in `block_to_flow()`
- [ ] Add test fixture
- [ ] Update documentation
- [ ] Remove follow-up tracking item
- [ ] Verify all tests pass

## Tasks

### Task 1: Suppress block_to_flow under enforce-block policy

Add a one-line early return, a test fixture, and a docs
note.

- [ ] In `block_to_flow.rs`, add an early return at the
  top of `block_to_flow()` (after line 17, before
  `find_innermost_block_collection`): if
  `options.format_enforce_block_style` is `true`, return
  `None`.
- [ ] Add code-action fixture
  `block-to-flow-suppressed-by-enforce-block-style.md` —
  sets `format-options:` with
  `format_enforce_block_style: true`, cursor on a mapping
  key with a block value, `omits-action: flow style`.
  Verifies the action is not offered when the policy is
  active.
- [ ] Verify that the existing `block-to-flow` applies-
  action fixtures (which use default options where
  `format_enforce_block_style` is `false`) continue to
  pass — the action is still offered under the default.
- [ ] Add a `format_enforce_block_style` branch to
  `apply_format_option` in `code_action_fixtures.rs` — it
  is not currently handled. Without this, the fixture's
  `format-options:` key is silently ignored and the early
  return is never exercised.
- [ ] Add a note to `docs/configuration.md` in the
  `formatEnforceBlockStyle` section (around line 133):
  "When enabled, the 'Convert block to flow style' code
  action is suppressed — the formatter would revert the
  conversion on save."
- [ ] Remove the `block_to_flow` policy enforcement
  follow-up item from
  `.ai/memory/project_followup_plans.md`.
- [ ] `cargo fmt` produces no diff
- [ ] `cargo clippy --all-targets` reports zero warnings
- [ ] `cargo test -p rlsp-yaml` passes with zero failures

## Non-Goals

- Replacing `formatEnforceBlockStyle` with an enum — the
  boolean is sufficient for the current use case.
- Adding a `Flow` enforcement mode.
- Suppressing `flow_to_block` — it is diagnostic-driven
  and aligns with the block policy.

## Decisions

- **One-line suppression** — the simplest fix. The action
  is pointless when the formatter will undo it.
- **Single task** — ~5 lines of production code, 1
  fixture, 1 doc update. One commit.
