**Repository:** root
**Status:** Completed (2026-03-16)
**Created:** 2026-03-16

## Goal

Fix a bug where completion functions in `completion.rs`
leak suggestions across YAML document boundaries (`---`).
Four functions scan lines without stopping at document
separators, causing keys and values from other documents
to appear as suggestions or suppress valid completions.

Related upstream issue:
redhat-developer/yaml-language-server#869

## Context

The completion module uses text-based line scanning (not
AST document indices) to collect sibling keys, present
keys, sequence context, and value suggestions. None of
these scanners check for `---` or `...` document
separators, so in a multi-document file they cross
boundaries freely.

Affected functions in `rlsp-yaml/src/completion.rs`:

1. **`collect_sibling_keys`** (line 753) — walks backward
   and forward collecting keys at the same indent. Crosses
   `---` because `extract_key("---")` returns `None` but
   the loop continues past it.

2. **`collect_present_keys_at_indent`** (line 253) — scans
   ALL lines in the file at the target indent. Keys from
   other documents are marked "present", which suppresses
   valid schema completions in the current document.

3. **`suggest_values_for_key`** (line 807) — scans all
   lines for matching key names to suggest values. Values
   from other documents leak into suggestions.

4. **`is_in_sequence_item`** (line 513) — walks backwards
   to determine sequence context without stopping at `---`.

The fix is the same pattern for all four: treat `---` and
`...` as hard boundaries that stop or scope the scan.

## Steps

- [x] Add a helper `is_document_separator(trimmed: &str)`
- [x] Fix `collect_sibling_keys` to stop at separators
- [x] Fix `collect_present_keys_at_indent` to scope within
      the current document
- [x] Fix `suggest_values_for_key` to scope within the
      current document
- [x] Fix `is_in_sequence_item` to stop at separators
- [x] Add tests for each function with multi-document input
- [x] Run `cargo test` and `cargo clippy` to verify

## Tasks

### Task 1: Add document boundary helper and fix all four scanners

All four functions need the same fix pattern — a document
separator check. Since the changes are small and tightly
coupled, they belong in a single commit.

- [x] Add `is_document_separator` helper that checks for
      `---` and `...` (trimmed)
- [x] `collect_sibling_keys`: break backward/forward loops
      on separator
- [x] `collect_present_keys_at_indent`: determine current
      document range (find nearest `---`/`...` before and
      after cursor line) and restrict iteration to that
      range
- [x] `suggest_values_for_key`: same document-range
      scoping
- [x] `is_in_sequence_item`: break backward loop on
      separator
- [x] Tests: multi-document sibling key isolation
- [x] Tests: multi-document present-key scoping
- [x] Tests: multi-document value suggestion isolation
- [x] Tests: multi-document sequence item detection
- [x] `cargo test` passes
- [x] `cargo clippy` passes

## Decisions

- **Single task, not four** — the fix is the same pattern
  applied four times with shared infrastructure (the helper
  function). Splitting into four tasks would create four
  review cycles for what is effectively one logical change.
- **Document range vs per-line check** — for functions that
  scan all lines (`collect_present_keys_at_indent`,
  `suggest_values_for_key`), pre-computing the current
  document's line range is cleaner than adding a separator
  check inside the filter chain. For backward/forward
  walkers, a simple `break` on separator is sufficient.
