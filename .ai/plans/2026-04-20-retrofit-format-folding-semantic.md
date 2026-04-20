**Repository:** root
**Status:** InProgress
**Created:** 2026-04-20

## Goal

Retrofit the remaining three small-to-medium feature-level entry
points in `rlsp-yaml` to consume the parser AST (and, for comment
tokens, the parser's `Event::Comment` stream) instead of
reconstructing YAML structure from raw text: `format_on_type`,
`folding_ranges`, and `semantic_tokens`. Retire text-scanning
helpers, shrink the `parser_boundary_audit` allow-list, and remove
the corresponding follow-up-queue entries. `complete_at` is the
only remaining retrofit after this plan and gets a dedicated plan
due to its scope (2701 LOC, 16 allow-listed helpers).

## Context

### Current violations

Three `pub fn` entry points still split `text: &str` into lines
and walk the lines:

- `editing/on_type_formatting.rs::format_on_type(text: &str, position: Position, ch: &str, tab_size: u32) -> Vec<TextEdit>`
  — scans the trigger-character line and previous non-empty line
  to determine indentation and mapping-colon position.
  Helpers: `leading_spaces`, `find_mapping_colon`,
  `find_prev_non_empty_line`, `needs_extra_indent`,
  `is_block_scalar_indicator`. Only `leading_spaces` and
  `find_mapping_colon` are allow-listed (they take `line: &str`).
- `analysis/folding.rs::folding_ranges(text: &str) -> Vec<FoldingRange>`
  — iterates lines, reconstructs indentation-based folds
  (`collect_indentation_folds`), document-section folds
  (`collect_document_section_folds`), and comment-block folds
  (`collect_comment_block_folds`). 7 allow-list entries: root +
  6 HelperOf.
- `analysis/semantic_tokens.rs::semantic_tokens(text: &str) -> Vec<SemanticToken>`
  — iterates lines, finds comments, mapping keys, anchors,
  aliases, and tags via text scanning. 4 allow-list entries:
  root + 3 HelperOf.

### Prerequisite landed state

This plan depends on `2026-04-20-parser-tag-span.md` landing
first. That plan adds `tag_loc: Option<Span>` to
`Node::Scalar/Mapping/Sequence` (mirror of `anchor_loc`). With
`tag_loc` present, `semantic_tokens` emits tag tokens at the
exact `!name` / `!!suffix` span.

After the prerequisite: `Node::Scalar/Mapping/Sequence.loc` gives
all structural spans; `Node::*.anchor` + `.anchor_loc` give the
anchor token span; `Node::*.tag` + `.tag_loc` give the tag
token span; `Node::Alias.loc` gives the alias token span.
Comments are NOT in the AST, but the parser's event stream
exposes `Event::Comment` — consumed directly by folding_ranges
and semantic_tokens.

Record the prerequisite's final commit SHA in Decisions at
execution time.

### Text-handling carve-outs

Each retrofit has a distinct choice to make about comment
handling:

- **`format_on_type`** — does not read comments. No carve-out.
- **`folding_ranges`** — currently supports folding contiguous
  comment blocks (`# line 1\n# line 2\n# line 3\n` collapses into
  one fold). Comments are not in the AST. Options: (a) drop
  comment-block folding — user-visible regression but consistent
  with the prior "drop comment scanning" decision; (b) consume
  `Event::Comment` to enumerate comment spans and group adjacent
  comments — preserves behavior and uses the parser, not text.
  **Decision: option (b) for `folding_ranges`.** Comment folding
  is a user-visible feature common across YAML editors; dropping
  it would regress a feature users rely on. `semantic_tokens`
  (below) makes the same choice for the same reason, so the
  `Event::Comment` consumption pattern is reused.
- **`semantic_tokens`** — produces syntax-highlighting tokens
  including comment tokens. Dropping comment tokens means YAML
  comments lose their distinct editor color — an unacceptable
  regression. Comments MUST be preserved. Consume `Event::Comment`
  from the parser's event iterator to produce comment tokens.
  Anchors, aliases, tags, and keys come from the AST.

The `Event::Comment` consumption is NOT a text-handling carve-out
— it consumes the parser's authoritative output. It does not
require an allow-list entry.

### Specifications and consumers

- LSP spec: `FoldingRange`, `SemanticToken` (delta-encoded by
  LSP protocol, but this function returns a flat vector of
  absolute-position `SemanticToken` entries and the server does
  the delta encoding), `TextEdit`.
- `rlsp_yaml_parser` public API: `load(text) -> Vec<Document<Span>>`
  is already used throughout. The event iterator must also be
  public for this plan's consumption of `Event::Comment` — verify
  at task start; if it is not public, this plan has a blocker
  requiring a parser change (a small one — just exposing an
  existing iterator). Document the resolution in Decisions.

### Involved files

- `rlsp-yaml/src/editing/on_type_formatting.rs` — retrofit target
- `rlsp-yaml/src/analysis/folding.rs` — retrofit target
- `rlsp-yaml/src/analysis/semantic_tokens.rs` — retrofit target
- `rlsp-yaml/src/server.rs` — three call sites
- `rlsp-yaml/tests/parser_boundary_audit.rs` — allow-list
- `.ai/memory/project_followup_plans.md` — queue entry removal

### Allow-list target

**Baseline (start of plan):** **58 entries** (post-Plan-B state).
Verified scoped entries by direct inspection:

- `editing/on_type_formatting.rs`: `format_on_type` (root) +
  `leading_spaces`, `find_mapping_colon` (2 HelperOf). Total: 3.
- `analysis/folding.rs`: `folding_ranges` (root) + 6 HelperOf
  (per queue entry: `collect_indentation_folds`,
  `collect_document_section_folds`, `collect_comment_block_folds`,
  `find_last_content_line`, `find_last_content_line_in_range`,
  `find_mapping_colon`). Total: 7.
- `analysis/semantic_tokens.rs`: `semantic_tokens` (root) + 3
  HelperOf (`collect_inline_markers`, `char_col_of`,
  `find_mapping_colon`). Total: 4.

Each task removes all scoped entries for its target.

- **Task 1 (`format_on_type`):** removes 3 → **55**.
- **Task 2 (`folding_ranges`):** removes 7 → **48**.
- **Task 3 (`semantic_tokens`):** removes 4 → **44**.

Shrink-only discipline. No new entries in any task.

## Steps

- [x] Task 1: retrofit editing/on_type_formatting.rs
- [ ] Task 2: retrofit analysis/folding.rs
- [ ] Task 3: retrofit analysis/semantic_tokens.rs

## Tasks

### Task 1: Retrofit editing/on_type_formatting.rs to AST-only

Committed as `0d7c06d7935ec743829a9974055d4971d24f2cfd` (may be
superseded by follow-up amend for SHA recording). Smallest
retrofit. `format_on_type` produces a `TextEdit` based on
the trigger character's position — indentation and colon
detection. AST gives the node spans for context (mapping/sequence,
current indentation level from parent node spans). No comment
handling needed.

- [x] New signature:
      `pub fn format_on_type(docs: &[Document<Span>], position: Position, ch: &str, tab_size: u32) -> Vec<TextEdit>`.
      Returns empty `Vec` when `docs` is empty (no-op formatting,
      matches current behavior when text is empty).
- [x] Implement using AST containment: locate the node that
      contains the trigger position. Use the node's context
      (mapping key, mapping value, sequence item, scalar) to
      decide indentation. Use the parent node's `loc.start.column`
      (or equivalent) to compute the current indentation level
      instead of counting leading spaces of the previous line.
- [x] Keep behavior for triggers after `:` (newline in a
      mapping value position) and after `-` (newline in a
      sequence-item position). For block-scalar indicators
      (`|`, `>`), preserve the existing extra-indent logic —
      `is_block_scalar_indicator` may still be a useful pure
      helper on `&str` for checking scalar-value syntax; if so,
      retain it (it is not allow-listed). `needs_extra_indent`
      and `find_prev_non_empty_line` may also be retained as
      pure helpers on strings as long as they don't perform
      structural YAML parsing (they are not allow-listed today).
- [x] Delete `leading_spaces` and `find_mapping_colon` from
      `editing/on_type_formatting.rs` — the AST supplies
      indentation via `loc.start.column` and colon positions via
      mapping-entry structure.
- [x] Update `rlsp-yaml/src/server.rs` at the `format_on_type`
      call site (search for `on_type_formatting::format_on_type`
      — approximately in the handler registered for
      `textDocument/onTypeFormatting`). Pass
      `docs.as_deref().unwrap_or(&[])` instead of `&text`.
- [x] Update every existing unit test in
      `editing/on_type_formatting.rs` to use the new signature.
      Parse inputs via `rlsp_yaml_parser::load`. No test case may
      be deleted unless supplanted by an equivalent case.
- [x] Add 3 rstest regression cases (named): (a) newline after `:`
      in a mapping value position produces correct indentation
      derived from parent node's column; (b) newline after `-` in
      a sequence item produces correct indentation; (c) block
      scalar indicator trigger (`|` or `>`) produces the expected
      extra indent.
- [x] **Before editing:** confirm `ALLOW_LIST` length = 58 and
      the `editing/on_type_formatting.rs` scoped entries match
      exactly: `format_on_type`, `leading_spaces`,
      `find_mapping_colon` (3 total).
- [x] **After editing:** remove exactly those 3 entries.
      `ALLOW_LIST` length = 55.
- [x] Remove the `format_on_type` entry from
      `.ai/memory/project_followup_plans.md`.
- [x] `cargo test` passes; `cargo clippy --all-targets` zero
      warnings; `cargo fmt --check` clean.

### Task 2: Retrofit analysis/folding.rs to AST + Event::Comment

`folding_ranges` produces fold regions for mappings, sequences,
document sections, and comment blocks. The first three come from
the AST (`Node::Mapping.loc`, `Node::Sequence.loc`, `Document.root.loc`);
comments come from `Event::Comment` in the parser's event stream.

- [ ] New signature:
      `pub fn folding_ranges(docs: &[Document<Span>], text: &str) -> Vec<FoldingRange>`.
      The `text` parameter is retained ONLY to re-parse for
      `Event::Comment` extraction — it is not used for any
      structural decision. Document this in the implementation
      with a comment at the signature site explaining the
      narrow purpose. If the event iterator can be obtained
      without re-parsing (e.g. a cached events vector already
      lives in the parse pipeline), prefer that and drop `text`.
- [ ] Implement: walk every `Document<Span>`; for each
      `Node::Mapping` and `Node::Sequence`, produce a fold spanning
      `loc.start.line..loc.end.line` (0-based after conversion).
      For document-section folds (inputs with multiple
      `---`-separated documents), produce a fold per
      `Document.root.loc`. For comment-block folds, invoke the
      parser's event iterator on `text` (or reuse the cached
      iterator), filter `Event::Comment`, group contiguous
      comments (line N ends where line N+1's comment starts),
      and produce a fold per group of ≥2 lines.
- [ ] Delete all 6 text-walking helpers from
      `analysis/folding.rs`: `collect_indentation_folds`,
      `collect_document_section_folds`, `collect_comment_block_folds`,
      `find_last_content_line`, `find_last_content_line_in_range`,
      `find_mapping_colon`.
- [ ] Update the `server.rs` call site for `folding_range` (LSP
      handler for `textDocument/foldingRange`, around line 905
      pre-retrofit — verify at task start) to pass `docs` and
      `&text`. Preserve the `truncate(limit)` and empty-check
      `Ok(None)` short-circuit.
- [ ] Update every existing unit test in `analysis/folding.rs`
      to use the new signature. Parse via
      `rlsp_yaml_parser::load`. Tests that assert specific line
      numbers for mapping/sequence folds must match AST-derived
      line numbers, which may differ from text-walk derivation
      at exact edge cases — update assertions accordingly.
- [ ] Verify comment-block fold tests still pass — if the
      `Event::Comment` grouping produces different fold boundaries
      from the prior text-walk, update tests with new values and
      document the drift in commit message.
- [ ] Add 4 rstest regression cases (named): (a) mapping at top
      level produces a fold whose range matches its AST `loc`;
      (b) nested mapping produces nested folds; (c) multi-document
      YAML produces one fold per document (plus nested folds);
      (d) contiguous block of ≥2 comments produces exactly one
      comment fold covering all consecutive comment lines.
- [ ] **Before editing:** confirm `ALLOW_LIST` length = 55 (post
      Task 1) and the `analysis/folding.rs` scoped entries match
      exactly: `folding_ranges` + 6 HelperOf (enumerated above).
- [ ] **After editing:** remove exactly those 7 entries.
      `ALLOW_LIST` length = 48.
- [ ] Remove the `folding_ranges` entry from
      `.ai/memory/project_followup_plans.md`.
- [ ] `cargo test` passes; `cargo clippy --all-targets` zero
      warnings; `cargo fmt --check` clean.

### Task 3: Retrofit analysis/semantic_tokens.rs to AST + Event::Comment

`semantic_tokens` classifies every token in the document for
syntax highlighting: comments, mapping keys, anchors, aliases,
tags. Comments come from `Event::Comment`; everything else from
the AST.

- [ ] New signature:
      `pub fn semantic_tokens(docs: &[Document<Span>], text: &str) -> Vec<SemanticToken>`.
      Same `text` caveat as Task 2: retained only for
      `Event::Comment` extraction. Document the narrow purpose.
- [ ] Implement:
      - **Mapping keys.** Walk `Node::Mapping.entries`; for each
        `(key, value)` pair where `key` is a `Node::Scalar`, emit
        a key token with `key.loc` as the span. Classification
        uses the existing key-token logic.
      - **Anchors.** For each `Node::Scalar/Mapping/Sequence`
        with `anchor: Some(_)` and `anchor_loc: Some(span)`, emit
        an anchor token with `anchor_loc` as the span.
      - **Aliases.** For each `Node::Alias`, emit an alias token
        with `alias.loc` as the span.
      - **Tags.** For each node with `tag: Some(_)` and
        `tag_loc: Some(span)` (both available via the prerequisite
        parser plan), emit a tag token with `tag_loc` as the span.
        Symmetric with anchor-token emission. No behavior change
        vs. the pre-retrofit text-walk implementation — the span
        is parser-authoritative and covers `!name` / `!!suffix` /
        `!<URI>` / `!handle!suffix` uniformly.
      - **Comments.** Invoke the parser's event iterator;
        filter `Event::Comment` entries; emit a comment token per
        comment, using the event's span.
- [ ] Delete all 3 text-walking helpers from
      `analysis/semantic_tokens.rs`: `collect_inline_markers`,
      `char_col_of`, `find_mapping_colon`.
- [ ] Update `rlsp-yaml/src/server.rs` at the `semantic_tokens`
      call site (LSP handler for `textDocument/semanticTokens/full`,
      around lines somewhere in server.rs — verify at task
      start). Pass `docs` and `&text`.
- [ ] Update every existing unit test in `analysis/semantic_tokens.rs`
      to use the new signature. Parse via `rlsp_yaml_parser::load`.
      Tests asserting comment token positions should still pass
      (`Event::Comment` produces the same spans as the text walk).
      Tests asserting tag-token highlighting: update to assert the
      token span equals `node.tag_loc` (parser-authoritative).
      The span values may drift from the old text-walk values at
      edge cases — adopt the AST-derived values as authoritative.
- [ ] Add 5 rstest regression cases (named): (a) mapping key
      produces a key token at key.loc; (b) scalar with anchor
      produces an anchor token at anchor_loc (NOT at the scalar's
      loc); (c) alias produces an alias token at alias.loc; (d)
      comment produces a comment token from `Event::Comment`; (e)
      tagged scalar produces a tag token at tag_loc (NOT at the
      scalar's loc) — verify with `!!int 42`: tag token spans
      `!!int` exactly and the scalar token starts after.
- [ ] **Before editing:** confirm `ALLOW_LIST` length = 48 (post
      Task 2) and the `analysis/semantic_tokens.rs` scoped
      entries match exactly: `semantic_tokens` + 3 HelperOf
      (enumerated above).
- [ ] **After editing:** remove exactly those 4 entries.
      `ALLOW_LIST` length = 44.
- [ ] Remove the `semantic_tokens` entry from
      `.ai/memory/project_followup_plans.md`.
- [ ] No change to `rlsp-yaml/docs/feature-log.md` — tag tokens
      remain in the highlighting set (delivered by the
      prerequisite parser plan). This retrofit is internal-only
      and per project convention (feature-log is user-facing
      feature decisions only) no entry is added.
- [ ] `cargo test` passes; `cargo clippy --all-targets` zero
      warnings; `cargo fmt --check` clean.

## Decisions

- **Comment handling via `Event::Comment`, not text.** Both
  folding_ranges and semantic_tokens consume the parser's
  `Event::Comment` to produce comment-related output. This keeps
  the project within "one parser, one AST" spirit: comments come
  from the parser's event stream, which is its authoritative
  view. No new text-scanning helpers or carve-outs.
- **Tag tokens preserved via `tag_loc`.** The prerequisite plan
  `2026-04-20-parser-tag-span.md` adds `tag_loc: Option<Span>` to
  `Node::Scalar/Mapping/Sequence` — mirror of the earlier
  `anchor_loc` plan. `semantic_tokens` emits tag tokens at the
  exact `tag_loc` span, preserving behavior.
- **Event iterator public API verification.** Task 2 and Task 3
  assume the parser's event iterator is publicly reachable. If
  not, Task 2 is blocked pending a parser change. Developer
  verifies at task start and reports. Resolution: minimal — expose
  the iterator that already exists internally.
- **`text` parameter retained where needed.** Tasks 2 and 3 keep
  `text` in the signature solely for `Event::Comment` extraction.
  The `text` is NOT consulted for structural decisions. An
  explanatory comment at the signature site documents this. If a
  future change exposes events alongside docs in the call site
  (pre-parsed once), drop `text`.
- **`format_on_type` keeps some string helpers.**
  `needs_extra_indent`, `is_block_scalar_indicator`, and
  `find_prev_non_empty_line` can remain as pure utilities on
  `&str` as long as they do not reconstruct YAML structure —
  they are not on the allow-list today, so retention is fine.
- **Shrink-only allow-list discipline.** Entries may be removed,
  never added.

## Non-Goals

- `complete_at` retrofit (2701-LOC file, 16 allow-listed
  helpers, completion context reconstruction). Dedicated plan
  after this one lands.
- Exposing tag token spans in the parser AST — scoped to the
  prerequisite plan `2026-04-20-parser-tag-span.md`, not this
  plan.
- Test dedup / file split for `analysis/*.rs` or
  `editing/on_type_formatting.rs`. Tracked as a pending-to-draft
  plan in `.ai/memory/project_followup_plans.md` under the
  "Post-AST+formatter-program cleanup" bullet; a dedicated plan
  file is filed after `complete_at` (the last remaining retrofit)
  lands.
- Any formatter, validator, code-action, or hover changes.
