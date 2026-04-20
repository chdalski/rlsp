**Repository:** root
**Status:** Completed (2026-04-20)
**Created:** 2026-04-20

## Goal

Retrofit the four anchor/alias navigation features in `rlsp-yaml`
— `goto_definition`, `find_references`, `prepare_rename`, `rename`
— to consume the parser AST directly instead of re-scanning raw
text. Retire the text-scanning helpers these features share
(`scan_tokens`, `document_range_for_line`, `is_anchor_name_char`,
`Token`), shrink the `parser_boundary_audit` allow-list, and land
this as the last bundle of "feature-level" retrofits in the
navigation area.

## Context

### Current violations

Four `pub fn` entry points in `rlsp-yaml/src/navigation/` split
`text: &str` into lines and walk the lines with hand-rolled
pattern matching to locate `&name` and `*name` tokens:

- `navigation/references.rs`:
  - `goto_definition(text: &str, uri: &Url, position: Position) -> Option<Location>`
  - `find_references(text: &str, uri: &Url, position: Position, include_declaration: bool) -> Vec<Location>`
- `navigation/rename.rs`:
  - `prepare_rename(text: &str, position: Position) -> Option<Range>`
  - `rename(text: &str, uri: &Url, position: Position, new_name: &str) -> Option<WorkspaceEdit>`

Shared private helpers in each file: `scan_tokens`,
`document_range_for_line`, `is_anchor_name_char`, `Token` struct.
`rename.rs` also has `is_valid_anchor_name` for validating the
user-supplied `new_name` — this is user-input validation, not
parsing, and stays.

### Prerequisite landed state

This plan assumes `anchor_loc: Option<Span>` has landed on
`Node::Scalar`, `Node::Mapping`, `Node::Sequence` per plan
`2026-04-20-parser-anchor-span.md`. `Node::Alias.loc` already
covers the `*name` token exactly (verified by direct probe).
Record the prerequisite's final commit SHA in Decisions at
execution time.

### Specifications and consumers

- YAML 1.2 §6.9 (Node Properties) — anchor and alias syntax
- LSP spec: `GotoDefinitionResponse`, `Location`, `ReferenceParams`,
  `PrepareRenameResponse`, `WorkspaceEdit`, `TextEdit`. The
  `prepare_rename` range must be the exact text the editor will
  replace; `rename` `TextEdit.range` values define the exact
  replacement regions.

### Involved files

- `rlsp-yaml/src/navigation/references.rs` — retrofit targets
- `rlsp-yaml/src/navigation/rename.rs` — retrofit targets
- `rlsp-yaml/src/server.rs` — call sites at lines ~843, ~864,
  ~1290, ~1311; each currently reads the document text from the
  store and passes it to the nav function
- `rlsp-yaml/src/document_store.rs` — provides access to parsed
  `Document<Span>` values in addition to text (used today by
  `hover_at` and several validators that are already AST-first)
- `rlsp-yaml/tests/parser_boundary_audit.rs` — allow-list entries
  for the four retrofit targets plus the four shared helpers

### Behavior preserved

- **Document scoping.** Today's text implementation scopes anchor
  lookups to the current YAML document (anchors in document 2
  are not visible from document 1). The AST already partitions
  by `Document` — walking only the containing document preserves
  this.
- **Exact `&name`/`*name` ranges.** `Node::Alias.loc` gives exact
  alias spans; `Node::*.anchor_loc` gives exact anchor spans
  (prerequisite plan). No widening.
- **`rename` validation of `new_name`.** `is_valid_anchor_name`
  stays — validates user-supplied identifier against YAML anchor
  name rules. Not parsing; not a retrofit candidate.
- **Out-of-bounds and not-on-token returns.** `None` / empty-`Vec`
  returns for cursors outside all anchor/alias spans — same shape
  as today.

### Allow-list target

`rlsp-yaml/tests/parser_boundary_audit.rs` currently lists the
four retrofit targets plus the four per-file shared helpers
(two copies each of `scan_tokens` and `document_range_for_line`).
`is_anchor_name_char` and `is_valid_anchor_name` are NOT in the
allow-list (they take `ch: char` / `name: &str`, not
`text: &str`).

**Allow-list baseline (start of plan):** `const ALLOW_LIST` has
**82 entries**. Task 1 removes 4 entries (`goto_definition`,
`find_references`, plus `scan_tokens` and `document_range_for_line`
helpers scoped to `navigation/references.rs`). Task 2 removes 4
entries (`prepare_rename`, `rename`, plus `scan_tokens` and
`document_range_for_line` helpers scoped to `navigation/rename.rs`).
After Task 1: **78**. After Task 2: **74**. The audit regression
test fails fast if the measured count drifts from these targets,
catching both "forgot to remove" and "accidentally removed
something else" regressions in one check.

## Steps

- [x] Task 1: retrofit references.rs (goto_definition + find_references)
- [x] Task 2: retrofit rename.rs (prepare_rename + rename)

## Tasks

### Task 1: Retrofit navigation/references.rs to consume the AST

Committed as `c515ef478cc24479d053304bbbe60b5cb95b700c` (may be
superseded by follow-up amend for SHA recording). Replace the
text-scanning implementations of `goto_definition` and
`find_references` with AST walks. The new signatures accept
`docs: &[Document<Span>]` instead of `text: &str`. Introduce a
private helper that collects `(anchor_name, anchor_loc)` pairs and
`(alias_name, alias_loc)` pairs scoped to the document containing
the cursor, modeled on the existing `collect_anchors_and_aliases`
in `validation/validators.rs` but extended to also emit anchor
`Span`s (now available via `anchor_loc`). Update server call sites
to pass the AST. Update tests to match the new signatures.
Remove `scan_tokens`, `document_range_for_line`, `is_anchor_name_char`,
and the private `Token` struct from this file.

- [x] New signature for `goto_definition`:
      `pub fn goto_definition(docs: &[Document<Span>], uri: &Url, position: Position) -> Option<Location>`.
- [x] New signature for `find_references`:
      `pub fn find_references(docs: &[Document<Span>], uri: &Url, position: Position, include_declaration: bool) -> Vec<Location>`.
- [x] Add a private helper that walks a single `Document<Span>`
      and returns vectors of anchor entries `(name: String, loc: Span)`
      and alias entries `(name: String, loc: Span)`. Scope the walk
      to the single document containing the cursor — find it by
      checking which document's root `loc` contains the cursor's
      `Pos` (1-based line, 0-based column — same coordinate
      conversion as `hover_at`).
- [x] Implement `goto_definition` by locating the alias token
      containing the cursor, then finding the first anchor in the
      same document with the matching name. Convert the anchor's
      `Span` to an LSP `Range` using the existing
      `loc.start.line.saturating_sub(1)` / `loc.start.column`
      pattern (see `validation/validators.rs:97`).
- [x] Implement `find_references` by locating the anchor OR alias
      token containing the cursor (either position), collecting
      all aliases in the same document with that name, optionally
      prepending the anchor definition when `include_declaration`
      is true. Return empty `Vec<Location>` when the cursor is
      not on any anchor or alias.
- [x] Delete `scan_tokens`, `document_range_for_line`,
      `is_anchor_name_char`, and the `Token` struct from
      `navigation/references.rs`.
- [x] Update call sites in `server.rs` at lines ~843 and ~864 to
      read `docs` from the document store (same pattern used by
      `hover_at` call site at ~1228 post-hover-retrofit) and pass
      the slice into the retrofit. Preserve the existing
      short-circuit behavior: return `Ok(None)` when no parsed
      docs are available.
- [x] Update every existing unit test in
      `navigation/references.rs` to use the new signatures. Parse
      test-input strings via `rlsp_yaml_parser::load` and pass the
      resulting docs. No test case may be deleted unless it is
      supplanted by an equivalent case with the new signature.
      Assertions on `Location.range` start/end line and character
      values stay exact — the AST must produce identical ranges.
- [x] Add regression tests for behavior that only the AST can
      distinguish (one rstest case per, named): anchor on a
      collection value (`defaults: &d\n  k: v\nref: *d\n`) —
      cursor on `*d` jumps to the `&d` token, not the collection
      body; `include_declaration: true` with a cursor on `*alias`
      returns the anchor definition plus all aliases; UTF-8
      anchor name lookup.
- [x] Before making any edit, confirm the current `const
      ALLOW_LIST` length in `tests/parser_boundary_audit.rs` is
      **82**. Record this baseline in the task handoff.
- [x] Remove exactly 4 allow-list entries from
      `tests/parser_boundary_audit.rs`: the two roots
      `goto_definition` and `find_references`, plus the two
      HelperOf entries scoped to `navigation/references.rs`
      (`scan_tokens`, `document_range_for_line`). Note that
      `is_anchor_name_char` is not in the allow-list (it takes
      `ch: char`, not `text: &str`). After removal the const
      length must be **78**. Allow-list entries may only be
      removed, never added.
- [x] Remove the follow-up-queue entries for `goto_definition`
      and `find_references` from `.ai/memory/project_followup_plans.md`.
      The file convention (stated at the top of the file) is
      that only open items stay; completed retrofits must be
      deleted from the queue.
- [x] `cargo test` passes with zero failures.
- [x] `cargo clippy --all-targets` passes with zero warnings.
- [x] `cargo fmt --check` passes.

### Task 2: Retrofit navigation/rename.rs to consume the AST

Committed as `37c6c41be361009c4daa1d2a28397c0e5c87acea` (may be
superseded by follow-up amend for SHA recording). Replace the
text-scanning implementations of `prepare_rename` and
`rename` with AST walks. New signatures accept
`docs: &[Document<Span>]` instead of `text: &str`. Reuse the same
anchor/alias collector approach as Task 1 — if Task 1 placed the
helper in a shared location (e.g. `navigation/anchors.rs`), this
task calls it; otherwise each file has its own private copy.
Retire the rename-specific copies of `scan_tokens`,
`document_range_for_line`, `is_anchor_name_char`, and `Token`.
Keep `is_valid_anchor_name` — it validates user-supplied `new_name`
against YAML anchor rules and is not parser-related.

- [x] New signature for `prepare_rename`:
      `pub fn prepare_rename(docs: &[Document<Span>], position: Position) -> Option<Range>`.
- [x] New signature for `rename`:
      `pub fn rename(docs: &[Document<Span>], uri: &Url, position: Position, new_name: &str) -> Option<WorkspaceEdit>`.
- [x] Implement `prepare_rename` by locating the anchor OR alias
      token at the cursor and returning its exact LSP `Range`.
      Returns `None` when cursor is not on any anchor/alias.
- [x] Implement `rename` by locating the anchor OR alias token at
      the cursor, deriving the anchor name, then producing
      `TextEdit`s for every anchor and alias in the same document
      with that name. Each `TextEdit.new_text` carries the
      appropriate `&` or `*` sigil prefix: `&new_name` for the
      anchor, `*new_name` for each alias.
- [x] Keep the existing `is_valid_anchor_name` check on `new_name`
      and its `None` return when the name is invalid. This is user
      input validation.
- [x] Delete `scan_tokens`, `document_range_for_line`,
      `is_anchor_name_char`, and the `Token` struct from
      `navigation/rename.rs`.
- [x] Update call sites in `server.rs` at lines ~1290 and ~1311
      to pass `docs` instead of `text`. Preserve existing
      short-circuit behavior for missing documents.
- [x] Update every existing unit test in `navigation/rename.rs`
      to use the new signatures; parse test inputs via
      `rlsp_yaml_parser::load`. No test case may be deleted
      unless supplanted by an equivalent case with the new
      signature.
- [x] Add regression tests (rstest cases, named): `rename` on an
      anchor that annotates a collection (`defaults: &d\n  k: v`)
      produces exactly one `&d`-to-`&new` edit at the anchor
      token span — not at the collection's `loc`; `prepare_rename`
      on a cursor in the middle of an anchor name returns the
      full `&name` range; UTF-8 anchor name rename.
- [x] Before making any edit, confirm the current `const
      ALLOW_LIST` length in `tests/parser_boundary_audit.rs` is
      **78** (the length that Task 1 left behind). Record this
      baseline in the task handoff.
- [x] Remove exactly 4 allow-list entries from
      `tests/parser_boundary_audit.rs`: the two roots
      `prepare_rename` and `rename`, plus the two HelperOf
      entries scoped to `navigation/rename.rs` (`scan_tokens`,
      `document_range_for_line`). Note that `is_anchor_name_char`
      and `is_valid_anchor_name` are not in the allow-list (they
      take `ch: char` / `name: &str`, not `text: &str`). After
      removal the const length must be **74**. Allow-list
      entries may only be removed, never added.
- [x] Remove the follow-up-queue entries for `prepare_rename`
      and `rename` from `.ai/memory/project_followup_plans.md`.
      The file convention (stated at the top of the file) is
      that only open items stay; completed retrofits must be
      deleted from the queue.
- [x] `cargo test` passes with zero failures.
- [x] `cargo clippy --all-targets` passes with zero warnings.
- [x] `cargo fmt --check` passes.

## Decisions

- **AST walk per document, scoped by `loc` containment.** Same
  pattern as the already-retrofitted `hover_at` — find the
  document containing the cursor by span containment, then walk
  only that document.
- **Anchor token spans come from `Node::*.anchor_loc`.** This
  field is delivered by the prerequisite parser plan. Without it,
  the retrofit cannot produce precise ranges — especially for
  `prepare_rename`/`rename`, where a widened range would cause
  the editor to replace the value, not the anchor name.
- **Helper placement left to the developer.** The anchor/alias
  collector may live as a private helper in each file (duplicated
  — matching the current shape) or hoisted to a shared
  `navigation/anchors.rs` module. Decide during implementation
  based on whether duplication is small and self-contained (keep
  local) or non-trivial (extract). The program-level cleanup plan
  in the follow-up queue will consolidate if needed.
- **Shrink-only allow-list discipline.** Entries may be removed
  when their backing function is retrofitted or retired. No new
  entries in this plan. The `no-silent-target-weakening` rule
  prohibits bypassing the audit by adding fresh allow-list
  exceptions at runtime.

## Non-Goals

- Any other audit-v2 feature-level retrofit (`complete_at`,
  `format_on_type`, `find_document_links`, `find_colors`,
  `folding_ranges`, `selection_ranges`, `semantic_tokens`,
  `document_symbols`). Those remain in the follow-up queue.
- Changes to `rlsp-yaml-parser`. All parser work is scoped to
  the prerequisite plan `2026-04-20-parser-anchor-span.md`.
- Consolidation/test-dedup/file-split for `navigation/*.rs`.
  Deferred to the post-program cleanup plan already in the queue.
