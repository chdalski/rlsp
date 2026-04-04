**Repository:** root
**Status:** InProgress
**Created:** 2026-04-04

## Goal

Fix the `quote_flow_item` double-quoting edge case in the block-to-flow code
action. Conformance triage (99 failing cases) deferred to a later session.

## Context

- The reviewer flagged that `quote_flow_item` in `code_actions.rs` wraps items
  starting with `"` in extra quotes, producing `""true""` for `- "true"` in
  block-to-flow conversion (commit 3737c50 review note)
- The conformance suite (commit 1446819) has 402 test cases: 303 pass, 99 fail.
  The failures are assumed to be saphyr limitations but haven't been triaged.

### Key files

- `rlsp-yaml/src/code_actions.rs` — `quote_flow_item` function
- `rlsp-yaml/tests/conformance.rs` — conformance test module

## Steps

- [x] Fix quote_flow_item double-quoting (44514c1)

## Tasks

### Task 1: Fix quote_flow_item double-quoting (44514c1)

Fix `quote_flow_item` in `code_actions.rs` to detect already-quoted items
and not double-wrap them.

- [x] Check if item starts and ends with matching quotes (`"..."` or `'...'`)
- [x] If already quoted, return as-is
- [x] If not quoted but needs quoting, wrap in double quotes
- [x] Add test: `- "true"` block → `["true"]` flow (not `[""true""]`)
- [x] Add test: `- 'hello'` block → `['hello']` flow
- [x] Verify existing code action tests still pass

## Decisions

- **Already-quoted detection is simple string check** — if item starts with
  `"` and ends with `"` (or `'`/`'`), treat as already quoted. Edge cases
  (unbalanced quotes, escaped quotes) are rare in block YAML items.
