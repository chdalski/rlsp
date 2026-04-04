**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-04

## Goal

Fix the `quote_flow_item` double-quoting edge case in the block-to-flow code
action, and triage the 99 failing yaml-test-suite conformance cases to confirm
they are saphyr parser limitations (not rlsp-yaml bugs).

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

- [ ] Fix quote_flow_item double-quoting
- [ ] Triage conformance failures

## Tasks

### Task 1: Fix quote_flow_item double-quoting

Fix `quote_flow_item` in `code_actions.rs` to detect already-quoted items
and not double-wrap them.

- [ ] Check if item starts and ends with matching quotes (`"..."` or `'...'`)
- [ ] If already quoted, return as-is
- [ ] If not quoted but needs quoting, wrap in double quotes
- [ ] Add test: `- "true"` block → `["true"]` flow (not `[""true""]`)
- [ ] Add test: `- 'hello'` block → `['hello']` flow
- [ ] Verify existing code action tests still pass

### Task 2: Triage conformance failures

Review the 99 failing conformance cases and categorize each as:
- **saphyr parser limitation** — saphyr can't parse the input correctly
- **rlsp-yaml formatter bug** — our formatter changes meaning
- **rlsp-yaml validator issue** — our diagnostics are wrong

For each failure, add a brief comment in the test output or a tracking note.
If any failures are rlsp-yaml bugs (not saphyr), file them as issues or note
them for fixing.

- [ ] Run conformance suite and capture failure details
- [ ] Categorize each failure
- [ ] Document findings (comment in conformance.rs or separate file)
- [ ] If rlsp-yaml bugs found, report to lead

## Decisions

- **Already-quoted detection is simple string check** — if item starts with
  `"` and ends with `"` (or `'`/`'`), treat as already quoted. Edge cases
  (unbalanced quotes, escaped quotes) are rare in block YAML items.
