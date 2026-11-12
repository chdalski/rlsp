**Repository:** root
**Status:** InProgress
**Created:** 2026-11-12

## Goal

Folding a nested block in the editor hides the line after it: the language server ends every
structural fold on the line where the next sibling starts, so `a:\n  b: 1\nc: 2\n` gets the fold
(1,2) and folding it hides `c: 2`. A fold must end on the last line of its node's content, so that
folding never hides a line outside the node.

## Context

- **Specification:** LSP 3.17, `FoldingRange`
  (<https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#foldingRange>):
  `endLine` is "the zero-based end line of the range to fold. The folded area ends with the line's
  last character."
- **Structural folds** are the folds `rlsp-yaml/src/analysis/folding.rs` emits for a block or flow
  mapping or sequence, and the Region fold of each document root in a multi-document stream.
  Comment folds (runs of own-line comments) are the other kind; they already end on their last
  line.
- **Spans** (`rlsp_yaml_parser::load`): a block collection's
  span ends where its end event ends, which is the start of the next sibling; on the way it takes
  in the collection's trailing line break, blank lines, and comments, even a comment at column 0.
  A flow collection's span ends before its closing bracket. A scalar with text ends its span at the
  end of its text, and a block scalar's span takes in its trailing blank lines. An empty value or
  item (`b:`, `b: !!str`, `? k` without a value, a bare `-`) loads as a scalar with a zero-width
  span at the start of a later line, in some layouts after the blank lines and comments that
  follow it.
- **Last leaf** of a collection: the node found by walking the collection backwards — its entries
  from the last one, each value before its key, and its items from the last one — and down into
  nested collections, up to the first scalar or alias with a non-empty span, or the first
  collection without entries or items. A zero-width scalar is skipped.
- **Order of folds is visible:** the `textDocument/foldingRange` handler in
  `rlsp-yaml/src/server.rs` truncates the list to `maxItemsComputed`, so outer folds come before
  inner ones.

## Steps

- [x] Task 1: One fold computation and one last-line rule
- [ ] Task 2: A structural fold ends on the last line of its last leaf
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: One fold computation and one last-line rule

`folding.rs` computes a structural fold's lines at three sites (mapping, sequence, document
root), each with a double guard, and `rlsp-yaml/src/editing/formatter/blank_lines.rs` holds the
rule that a span end at column 0 belongs to the line before. Each gets one home, so that Task 2
changes one function. Behavior is unchanged.

- [x] `folding.rs` computes the lines of every structural fold in one function that all three
      fold sites use, and folds come out in the same order as before
- [x] One function in `rlsp-yaml/src/lsp_util.rs` gives the 0-based last line a span covers, and
      `blank_lines.rs` holds no copy of its column-0 rule
- [x] A unit table pins that function: a span that ends at the start of line n, after LF, after
      CRLF, or after a bare CR, gives n − 1; a span that ends inside line n, at the end of input
      without a final line break or after a multi-byte character, gives n; an empty span at the
      start of line n gives n − 1, and an empty span at offset 0 gives 0
- [x] The tests in `folding.rs` and `blank_lines.rs`, `tests/lsp_lifecycle/folding_ranges.rs`,
      and `folding_ranges_respects_max_items_computed_limit`
      (`tests/lsp_lifecycle/validators_integration.rs`) pass with their assertions unchanged
- [x] `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass

### Task 2: A structural fold ends on the last line of its last leaf

A structural fold ends on the last line its collection's last leaf covers (Decisions), through the
last-line function of Task 1; this follows `endLine` of LSP 3.17 `FoldingRange` (Context). Today's
structural tests in `folding.rs` pin the long ends or check them with `contains` and count
bounds: `folding_ranges_contains_range`, `folding_ranges_returns_empty`,
`should_fold_document_sections`, `should_fold_document_sections_with_nested_content`,
`should_fold_three_document_sections`, `should_fold_last_section_after_final_separator`,
`nested_mapping_produces_nested_folds`, `multi_doc_produces_fold_per_document`,
`sequence_fold_basic`, and `flow_mapping_single_line_no_fold`. Two tests outside `folding.rs`
count on folds of one-line collections, which this task removes:
`folding_ranges_respects_max_items_computed_limit` (`tests/lsp_lifecycle/validators_integration.rs`)
and the folding check of `tests/lsp_lifecycle/stack_depth.rs`, whose innermost collections hold one
line in the `BlockMappings`, `BlockSequences`, and `QuestionKeys` shapes.

- [ ] `a:\n  b: 1\nc: 2\n` gets exactly the folds `[(0, 2)]`, `a:\n  b: 1\n  c: 3\nd: 4\n` gets
      `[(0, 3), (1, 2)]`, and `key1: val1\nkey2: val2\n---\nkey3: val3\nkey4: val4\n` gets the
      Region folds `[(0, 1), (3, 4)]`
- [ ] An empty last value or item ends no fold past the text before it:
      `a:\n  b: 1\n  c:\n\n# x\nd: 1\n` gets `[(0, 5), (1, 2)]`,
      `a:\n  b: 1\n  ? k\n\nc: 1\n` gets `[(0, 4), (1, 2)]`,
      `a:\n  b: !!str\n\n# x\nc: 1\n` gets `[(0, 4)]`, `a:\n  - x\n  -\n\n# x\nd: 1\n` gets
      `[(0, 5)]`, `a:\n  -\n  -\nb: 1\n` gets `[(0, 3)]`, and the stream
      `a: 1\nb: 2\n---\n---\nc: 3\nd: 4\n`, whose second document is empty, gets the Region folds
      `[(0, 1), (4, 5)]`
- [ ] No structural fold ends on a line after its last leaf: trailing blank lines and trailing
      comments of a block collection, blank lines and comments before `---`, and the closing bracket
      of a flow collection on a line of its own stay outside the fold
- [ ] A collection whose last leaf is a block scalar folds to the scalar's last line, its trailing
      blank lines included
- [ ] The same document with LF, CRLF, or bare CR line breaks, and with or without a final line
      break, gets the same fold lines
- [ ] Folds keep their order (outer before inner) and their kinds (Region for each document root
      of a multi-document stream, none for collections); comment folds are unchanged
- [ ] The structural tests named above are one table test with named cases that asserts the exact
      ordered list of (start line, end line, kind) per input; it covers the inputs of the tests it
      replaces and each case of the criteria above
- [ ] `should_return_folding_ranges_for_nested_yaml` (`tests/lsp_lifecycle/folding_ranges.rs`)
      asserts the exact folds `(0, 2)` and `(1, 2)` for its input through the server
- [ ] `folding_ranges_respects_max_items_computed_limit` opens nested collections of two or more
      lines each, which yield more folds than its limit of 2, and still asserts the truncation to 2;
      its comment describes that input
- [ ] The folding check of `stack_depth.rs` still shows, for every block shape, that the fold walk
      reaches the deepest collection that holds more than one line
- [ ] `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass

## Decisions

- **End line of a fold:** a structural fold ends on the last line its collection's last leaf
  covers, not on the line of the collection's span end, which lies past the collection's content
  (Context, Spans).
- **Empty values and items are skipped:** a zero-width scalar does not mark where its entry ends,
  so the last leaf skips it. A fold whose last entry has an empty value ends on that entry's key,
  and one whose last item is empty ends on the item before it, so the `-` line of an empty last
  item stays visible when the block is folded: the AST holds no position of the `-`. A
  collection or document root without a non-empty leaf gets no fold.
- **Preparatory refactor first:** one fold function in `folding.rs` and one last-line function in
  `lsp_util.rs` come in their own task before the fix, so that the fix changes one function and the
  last-line rule, with its edge cases (CRLF, bare CR, end of input), lives in one place instead of
  two.
- **Trailing comments and blank lines stay outside the fold:** a comment or blank line after a
  collection's last entry, indented or not, and before `---`, stays visible when the block is
  folded. Taking indented trailing comments into the fold would need comment positions beyond the
  structure the fold rule reads.
- **A block scalar's trailing blank lines stay inside the fold:** YAML 1.2.2 §8.1.1.2
  (`l-chomped-empty`) makes them part of the scalar, unlike the blank lines after a collection's
  last entry.
- **A collection whose content is one line gets no fold:** a fold from a line to the same line
  hides nothing. Today such a collection gets a fold only because its span runs into the next
  line, and the two tests outside `folding.rs` that Task 2 names count on such folds.
- **A closing bracket on its own line stays visible:** the fold of a flow collection ends on its
  last item's line. A flow collection without items is its own last leaf.
- **No change to the loader's spans:** the loader passes on the end of the collection's end event,
  YAML defines no node spans, and other features rely on today's end (`sibling_order` in
  `analysis/selection.rs`, the edit range of `editing/code_actions/block_to_flow.rs`), so the
  parser shows no bug to fix (Crate Boundaries in `CLAUDE.md`).
- **Folds keep their start line:** a nested collection's fold starts on its first entry's line, as
  today, and the exact-list tests pin those start lines. Starting it on its key's line changes
  which line stays visible for every nested fold, a behavior change that needs its own design
  (Non-Goals).

## Non-Goals

- Starting the fold of a nested collection on its key's line, so that a key's block can be folded
  on its own (separate plan after this one)
- The ranges of document symbols and of schema diagnostics on collections, which also run to the
  collection's span end (separate plan after this one)
- Consolidating the comment-fold tests of `folding.rs` (`.ai/backlog.md`)
