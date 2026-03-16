**Repository:** root
**Status:** InProgress
**Created:** 2026-03-16

## Goal

Refactor imperative `for` loops to idiomatic iterator chains
across the rlsp-yaml codebase, applying the 4-criteria test
from `functional-style.md`: readability, less code, no manual
index math, lower complexity. This is a pure refactoring —
no behavior changes, all existing tests must continue to pass.

## Context

- The codebase has ~120 `for` loops in production code and
  ~40 `let mut Vec::new()` accumulator patterns
- The updated `functional-style.md` defines when loops should
  stay (state machines, recursive walks, complex early-exit,
  test builders) and when they should be refactored
  (collect-and-push, linear search, reverse search, flat mapping)
- Detailed findings are in `/workspace/iterators-vs-loops.md`
- This is behavior-preserving refactoring — risk is low,
  advisors not needed
- Each file is an independent commit to keep diffs reviewable

### Patterns to refactor (from rules)

1. **Collect-and-push** → `.iter().filter().map().collect()`
2. **Linear search** → `.find()` / `.position()` / `.any()`
3. **Reverse search** → `.rev().find()` / `.rev().position()`
4. **Flat mapping** → `.flat_map().map().collect()`

### Patterns to keep as loops

- `char_indices()` state machines (quote/depth tracking)
- Recursive tree walks (`collect_tags`, `validate_node`, etc.)
- Complex early-exit with interleaved break/continue
- Test data builders

## Steps

- [x] Analyze codebase and categorize loops
- [x] Update rules files with nuanced criteria
- [x] Write findings document
- [x] Task 1: Refactor `document_links.rs` (7f34533)
- [x] Task 2: Refactor `rename.rs` (2de560b)
- [ ] Task 3: Refactor `references.rs`
- [ ] Task 4: Refactor `code_actions.rs`
- [ ] Task 5: Refactor `folding.rs`
- [ ] Task 6: Refactor `symbols.rs`
- [ ] Task 7: Refactor `hover.rs`
- [ ] Task 8: Refactor `validators.rs`
- [ ] Task 9: Refactor `schema_validation.rs`
- [ ] Task 10: Refactor `on_type_formatting.rs`
- [ ] Task 11: Refactor `completion.rs`

## Tasks

### Task 1: Refactor `document_links.rs`

Small file, ~1 refactorable loop. Good warmup.

- [ ] `find_document_links`: collect-and-push link accumulation
- [ ] Keep: none expected
- [ ] All existing tests pass

### Task 2: Refactor `rename.rs`

~3 refactorable loops.

- [ ] `rename_at`: collect-and-push edits accumulation
- [ ] `scan_tokens`: forward search, token scanning
- [ ] Keep: complex early-exit in document boundary scanning

### Task 3: Refactor `references.rs`

~3 refactorable loops, structurally similar to `rename.rs`.

- [ ] `find_references`: collect-and-push locations
- [ ] `scan_tokens`: forward search
- [ ] Keep: complex early-exit in document boundary scanning

### Task 4: Refactor `code_actions.rs`

~3 refactorable loops.

- [ ] `flow_map_to_block`: collect-and-push block_lines
- [ ] `flow_seq_to_block`: collect-and-push block_lines
- [ ] `block_to_flow`: collect-and-push children
- [ ] Keep: `split_flow_items` char state machine

### Task 5: Refactor `folding.rs`

~4 refactorable loops.

- [ ] `folding_ranges`: collect-and-push ranges from stack
- [ ] `find_multiline_value_end`: forward search
- [ ] `find_last_content_line`: reverse search
- [ ] Keep: `count_leading_whitespace` char scanning,
      main indentation-tracking loop (complex state)

### Task 6: Refactor `symbols.rs`

~7 refactorable loops.

- [ ] `document_symbols`: collect-and-push all_symbols
- [ ] `find_doc_regions`: collect-and-push regions
- [ ] `build_hash_symbols`: collect-and-push symbols from map
- [ ] `build_array_children`: collect-and-push children
- [ ] `find_sequence_item_line`: forward search → `.position()`
- [ ] `find_key_in_lines`: forward search → `.find_map()`
- [ ] Keep: `find_value_end_line` (complex indent logic)

### Task 7: Refactor `hover.rs`

~3 refactorable loops.

- [ ] `build_schema_key_path`: collect path segments
- [ ] `sequence_index`: reverse search
- [ ] `format_examples`: collect-and-push example strings
- [ ] Keep: `indentation_level` char scanning,
      `find_key_range` complex state, recursive schema walks

### Task 8: Refactor `validators.rs`

~6 refactorable loops.

- [ ] `validate_unused_anchors`: collect-and-push diagnostics
      from anchors/aliases, doc_ranges accumulation
- [ ] `validate_flow_style`: collect-and-push diagnostics
- [ ] `validate_key_ordering`: collect-and-push diagnostics
- [ ] Keep: `scan_tokens` char state machine,
      `find_tag_occurrence` char scanning with quote tracking,
      recursive tree walks (`collect_tags`, `check_key_ordering`)

### Task 9: Refactor `schema_validation.rs`

~2 refactorable loops.

- [ ] `validate_documents`: flat_map over docs
- [ ] `check_required`: collect-and-push missing keys
- [ ] Keep: recursive `validate_node` tree walk,
      `find_key_range` forward search with complex state

### Task 10: Refactor `on_type_formatting.rs`

~1 refactorable loop.

- [ ] `find_prev_non_empty_indent`: reverse search → `.rev().find()`
- [ ] Keep: `is_inside_quotes` char state machine

### Task 11: Refactor `completion.rs`

Largest file, ~10 refactorable loops. Do last since it's most
complex and benefits from patterns established in earlier tasks.

- [ ] `completions`: collect-and-push result items
- [ ] `build_key_path`: reverse search for path building
- [ ] `schema_completions_for_path`: collect-and-push items
- [ ] `is_in_sequence_item`: reverse search
- [ ] `find_sequence_start`: reverse search
- [ ] `find_parent_key_indent`: reverse search
- [ ] `existing_sibling_keys`: forward search collecting keys
- [ ] `collect_existing_items`: forward search
- [ ] Keep: `colon_position_outside_quotes` char state machine,
      complex early-exit searches with multiple break conditions

## Decisions

- **One commit per file** — keeps diffs small and reviewable,
  easy to bisect if a regression appears
- **Order by complexity** — start with simple files to establish
  patterns, end with `completion.rs` (most loops, most complex)
- **No advisors needed** — pure refactoring, no behavior change,
  no security surface, all tests already exist
- **Apply the 4-criteria test per loop** — don't blindly convert;
  if the iterator version is less readable, keep the loop
