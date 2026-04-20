**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-20

## Goal

Retrofit `document_symbols` and `selection_ranges` in `rlsp-yaml`
to consume the parser AST directly, dropping the `text: &str`
parameter both functions still carry despite already accepting
`documents: Option<&Vec<Document<Span>>>`. Retire the text-scanning
private helpers, shrink the `parser_boundary_audit` allow-list, and
remove the follow-up-queue entries.

## Context

### Current violations

Two `pub fn` entry points in `rlsp-yaml/src/analysis/` accept a
`documents` slice AND a raw `text: &str`, then reconstruct
document boundaries and key positions from the text:

- `analysis/symbols.rs::document_symbols(text: &str, documents: Option<&Vec<Document<Span>>>) -> Vec<DocumentSymbol>`
  — uses `split_document_regions`, `find_key_in_lines`,
  `find_mapping_colon`, `find_value_end_line`,
  `find_sequence_item_line`, and `yaml_to_symbols` (which receives
  `lines` for deriving ranges).
- `analysis/selection.rs::selection_ranges(text: &str, documents: Option<&Vec<Document<Span>>>, positions: &[Position]) -> Vec<SelectionRange>`
  — uses `find_document_for_line`, `find_document_end`, and walks
  `lines` to decide which document contains each cursor.

All structural information (document boundaries, key spans,
mapping-entry ranges, sequence-item spans, end-of-value lines) is
already present on the AST via `Node::*.loc` and on each
`Document<Span>`.

### Prerequisite landed state

No parser changes required. The current AST is sufficient:
`Node::Scalar/Mapping/Sequence.loc` carries the span needed for
both symbol ranges and ancestor spans; each `Document<Span>.root`
has a `loc` that delimits the document.

### Specifications and consumers

- LSP spec: `DocumentSymbol` has `range` (full symbol extent) and
  `selection_range` (the identifier text to highlight).
  `SelectionRange` is a chain of nested ranges from innermost to
  outermost.
- Server call sites: `rlsp-yaml/src/server.rs` around line 967
  (`selection_range`) and line 1291 (`document_symbols`). Both
  already read `docs` from the document store; neither needs the
  text for the retrofit.

### Involved files

- `rlsp-yaml/src/analysis/symbols.rs` — retrofit target
- `rlsp-yaml/src/analysis/selection.rs` — retrofit target
- `rlsp-yaml/src/server.rs` — two call sites (drop `&text`
  argument; keep `docs` and `params`)
- `rlsp-yaml/tests/parser_boundary_audit.rs` — allow-list entries
- `.ai/memory/project_followup_plans.md` — queue entry removal

### Behavior preservation

- **Symbol `selection_range`.** Today's implementation places the
  selection range on the mapping key text (derived via text scan);
  for sequence items it places it on the `-` marker (1-character
  range). Post-retrofit: `selection_range` on mapping keys is the
  key node's `loc` — identical. For sequence items,
  `selection_range` becomes the item node's `loc.start..loc.start+1`
  when the item is in a block sequence (dash is 2 cols before
  item.loc.start per YAML block-sequence indentation). If
  computing the dash column reliably is non-trivial for edge
  cases, use `item.loc` (the full item range) and note the
  semantic widening in the plan's Decisions.
- **Symbol `range`.** Full range is `(key.loc.start, value.loc.end)`
  for a mapping entry; `item.loc` for a sequence item. This
  matches the current text-walk behavior except at exact end
  column — the AST end is one past the last content byte, the
  text walk ends at the last content line's full width.
  Regression tests must assert exact line/column values against
  the AST-derived ranges.
- **Document scoping.** Replace `split_document_regions` and
  `find_document_for_line` with `documents.iter()` and a
  loc-containment check on the cursor. One `Document<Span>` per
  parsed YAML document.

### Allow-list target

**Before start of plan:** `rlsp-yaml/tests/parser_boundary_audit.rs::ALLOW_LIST`
has **74 entries**. Verified by direct inspection:

- `analysis/symbols.rs` scoped entries: `document_symbols` (root
  `TodoRetrofit`) + 4 HelperOf (`split_document_regions`,
  `find_sequence_item_line`, `find_value_end_line`,
  `find_mapping_colon`) + 1 test-fixture entry (`parse_docs` in
  the `#[cfg(test)]` module). Note: `find_key_in_lines` and
  `yaml_to_symbols` are NOT allow-listed — they did not match
  the audit regex. `parse_docs` stays after retrofit (test helper).
- `analysis/selection.rs` scoped entries: `selection_ranges` (root
  `TodoRetrofit`) + 3 HelperOf (`selection_range_for_position`,
  `find_document_for_line`, `find_document_end`) + 1 test-fixture
  (`parse_docs`). `parse_docs` stays.

**Task 1 removes exactly 5 entries** (`document_symbols` root +
`split_document_regions`, `find_sequence_item_line`,
`find_value_end_line`, `find_mapping_colon`). After Task 1:
**69 entries**.

**Task 2 removes exactly 4 entries** (`selection_ranges` root +
`selection_range_for_position`, `find_document_for_line`,
`find_document_end`). After Task 2: **65 entries**.

The audit regression test fails fast if the measured count drifts
from these targets. Developer records the pre-edit baseline and
verifies the exact post-edit count in the task handoff.

## Steps

- [ ] Task 1: retrofit symbols.rs (document_symbols)
- [ ] Task 2: retrofit selection.rs (selection_ranges)

## Tasks

### Task 1: Retrofit analysis/symbols.rs to AST-only

Drop the `text: &str` parameter from `document_symbols`; derive
all symbol ranges from the AST. Retire the five text-scanning
private helpers. Update the server call site and all existing
tests.

- [ ] New signature:
      `pub fn document_symbols(docs: &[Document<Span>]) -> Vec<DocumentSymbol>`.
      Returns empty `Vec` when `docs` is empty. No `Option`
      wrapping — simplify to a slice.
- [ ] Implement using direct AST walk. For each `Document<Span>`,
      walk its root: mapping entries produce parent symbols where
      `selection_range` = key `Node::Scalar.loc` (converted to
      LSP `Range`) and `range` = `(key.loc.start, value.loc.end)`.
      Sequence items produce child symbols named `[idx]` where
      `range` = `item.loc` and `selection_range` = the 1-character
      position at `item.loc.start` (post-dash item content start
      for block sequences; use `item.loc.start` directly, which
      matches the current selection_range semantics on the item
      content rather than the dash — document this as a small
      behavior change in the plan's Decisions if any test
      enforces the dash-column choice).
- [ ] Delete `split_document_regions`, `find_sequence_item_line`,
      `find_key_in_lines`, `find_mapping_colon`,
      `find_value_end_line`, and the `DocRegion` struct from
      `analysis/symbols.rs`. Note: `find_key_in_lines` has no
      allow-list entry (the audit regex did not capture it); no
      allow-list row removal is required for it, but `cargo
      clippy` will flag it if any caller remains — that is the
      verification gate. `yaml_to_symbols`, `make_symbol`,
      `make_sequence_children`, `node_symbol_kind`, and
      `node_to_string` remain but no longer take `lines`.
- [ ] Update `rlsp-yaml/src/server.rs` at line ~1291 to pass
      `docs.as_deref().unwrap_or(&[])` (or equivalent slice
      conversion) without the `&text` argument. Preserve the
      `truncate(limit)` and empty-check short-circuit behavior.
- [ ] Update all 23 existing unit tests in `analysis/symbols.rs`
      to use the new signature. Parse test-input strings via
      `rlsp_yaml_parser::load` and pass the resulting slice. No
      test case may be deleted unless supplanted by an equivalent
      case with the new signature.
- [ ] Adjust tests whose assertions depend on the old text-walk
      range semantics (line 11, 12, 22, and any others that
      assert specific `range.end.line` / `selection_range.end.character`
      values). New exact values must come from the AST — update
      assertions to match AST-derived values and document the
      deltas in commit message.
- [ ] Add rstest regression cases (named per lang-rust-testing.md):
      (a) key with UTF-8 characters produces a symbol whose
      `selection_range` covers the full key; (b) deeply-nested
      mapping symbol chain has every level's `range` enclosing its
      child ranges; (c) sequence-of-mappings produces `[0]`, `[1]`
      children each with mapping-key grand-children; (d)
      multi-document YAML produces symbols from every document
      with ranges scoped to each document's root.
- [ ] **Before editing:** confirm `ALLOW_LIST` length is **74**
      and the set of entries with `file: "analysis/symbols.rs"`
      and `marker: TodoRetrofit` or `HelperOf { root:
      "document_symbols" }` matches: `document_symbols`,
      `split_document_regions`, `find_sequence_item_line`,
      `find_value_end_line`, `find_mapping_colon` (5 total).
      Remove exactly those 5 entries. Keep the
      `parse_docs` test-fixture entry. After removal,
      `ALLOW_LIST` length must be exactly **69**. Allow-list
      entries may only be removed, never added.
- [ ] Remove the follow-up-queue entry for `document_symbols` from
      `.ai/memory/project_followup_plans.md`.
- [ ] `cargo test` passes with zero failures.
- [ ] `cargo clippy --all-targets` passes with zero warnings.
- [ ] `cargo fmt --check` passes.

### Task 2: Retrofit analysis/selection.rs to AST-only

Drop the `text: &str` parameter from `selection_ranges`; resolve
the cursor's document via root-span containment; retire the text-
scanning helpers.

- [ ] New signature:
      `pub fn selection_ranges(docs: &[Document<Span>], positions: &[Position]) -> Vec<SelectionRange>`.
      Returns empty `Vec` when `docs` or `positions` is empty.
- [ ] Implement: for each cursor position, find the `Document<Span>`
      whose `root.loc` contains the cursor (by 1-based line +
      0-based column conversion — see `hover_at` and
      `validation/validators.rs`). Walk that document's AST to
      collect ancestor spans via the existing
      `collect_ancestor_spans` function (already AST-based).
      Build the `SelectionRange` chain innermost-first; the
      outermost parent is the document root range derived from
      `doc.root.loc`.
- [ ] Skip positions on comment lines and document separators by
      checking whether any `Document.root.loc` contains the
      cursor — positions outside all document locs return `None`
      (consistent with current "skip comment/separator" behavior
      which also excluded out-of-AST positions).
- [ ] Delete `selection_range_for_position`, `find_document_for_line`,
      `find_document_end` from `analysis/selection.rs`.
      `collect_ancestor_spans`, `node_span`, `span_to_lsp_range`,
      and `make_line_range` remain.
- [ ] Update `rlsp-yaml/src/server.rs` at line ~968 to drop the
      `&text` argument. Call site becomes
      `selection_ranges(docs.as_deref().unwrap_or(&[]), &params.positions)`
      or equivalent. Preserve the empty-check and `Ok(None)`
      short-circuit.
- [ ] Update every existing unit test in `analysis/selection.rs`
      to use the new signature; parse test input via
      `rlsp_yaml_parser::load`. No test case may be deleted
      unless supplanted by an equivalent case. Adjust any
      assertion whose exact range depended on the old text-walk
      behavior (e.g. `make_line_range` using `u32::MAX` end
      columns vs. AST-derived end columns).
- [ ] Add rstest regression cases (named): (a) cursor on a key
      returns a chain (key → entry → mapping → doc root); (b)
      cursor inside a nested mapping's leaf scalar produces the
      full nested chain; (c) multi-document: cursor in doc 2
      does not include doc 1's ranges; (d) cursor on a comment
      line returns no `SelectionRange` for that position.
- [ ] **Before editing:** confirm `ALLOW_LIST` length is **69**
      (after Task 1) and the set of entries with
      `file: "analysis/selection.rs"` and `marker: TodoRetrofit`
      or `HelperOf { root: "selection_ranges" }` matches:
      `selection_ranges`, `selection_range_for_position`,
      `find_document_for_line`, `find_document_end` (4 total).
      Remove exactly those 4 entries. Keep the `parse_docs`
      test-fixture entry. After removal, `ALLOW_LIST` length
      must be exactly **65**. Allow-list may shrink only.
- [ ] Remove the follow-up-queue entry for `selection_ranges` from
      `.ai/memory/project_followup_plans.md`.
- [ ] `cargo test` passes with zero failures.
- [ ] `cargo clippy --all-targets` passes with zero warnings.
- [ ] `cargo fmt --check` passes.

## Decisions

- **Signature: `&[Document<Span>]` not `Option<&Vec<…>>`.** Both
  functions currently accept an `Option` wrapper; the AST-first
  version requires `docs`, so drop the wrapper. Empty slice
  replaces the `None` case. Call sites in `server.rs` coerce
  with `.as_deref().unwrap_or(&[])`.
- **Behavior drift tolerance.** The text walk computed symbol
  `range.end.character` using line widths (`lines.get(end_line).len()`);
  the AST computes it from the value node's `loc.end.column`.
  These may differ at line-trailing whitespace or block scalars.
  Assertions in tests must be updated to match AST-derived
  values — treat any divergence as the correct new behavior.
  Do NOT round to old values; the AST is authoritative.
- **Sequence-item `selection_range`.** The text walk used the `-`
  marker column (`dash_col..dash_col+1`). The AST places items'
  `loc.start` at the content column (post-dash). Use
  `item.loc.start..item.loc.start+1` as the new selection range;
  document this as a 2-column shift for block sequences.
- **Shrink-only allow-list discipline.** Entries may be removed,
  never added.

## Non-Goals

- Any other retrofit (`complete_at`, `find_document_links`,
  `find_colors`, `folding_ranges`, `semantic_tokens`,
  `format_on_type`). Those remain in the queue.
- Any parser changes.
- Test dedup / file split for `analysis/*.rs`. Deferred to the
  post-program cleanup plan already in the queue.
