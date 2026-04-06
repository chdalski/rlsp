**Repository:** root
**Status:** InProgress
**Created:** 2026-04-05

## Goal

Remove the 6 saphyr workarounds that remain in `rlsp-yaml`
after the mechanical type migration. The migration swapped
types but kept the same logic — the workarounds are now
unnecessary because `rlsp-yaml-parser` provides the
capabilities that saphyr lacked. Removing them simplifies
the code and improves performance.

## Context

- Migration from saphyr to rlsp-yaml-parser completed in
  plan `2026-04-05-saphyr-to-rlsp-yaml-parser-migration`
- The migration was mechanical — same logic, new types
- 6 documented saphyr workarounds may still be in the code:
  1. **Comment extract/reattach** in formatter.rs — saphyr
     discarded comments, so the formatter extracted them
     from raw text and reattached after formatting.
     rlsp-yaml-parser preserves comments in the AST.
  2. **Recursive span computation** in selection.rs —
     saphyr had zero spans on container nodes, so spans
     were computed from children. rlsp-yaml-parser provides
     spans on all nodes via `node.loc`.
  3. **Text-based duplicate key scanning** in validators.rs
     — saphyr silently deduplicated keys, so duplicate
     detection scanned raw text. rlsp-yaml-parser preserves
     all keys in the AST.
  4. **Eager alias resolution workaround** — saphyr resolved
     aliases inline. rlsp-yaml-parser has lossless mode
     preserving `Alias` nodes for navigation.
  5. **Lost chomping indicators** — saphyr didn't preserve
     `|-`, `|+`, `>-`, `>+`. rlsp-yaml-parser preserves
     `ScalarStyle` with `Chomp` variants.
  6. **Document boundary ambiguity** — saphyr couldn't
     distinguish document separators from content.
     rlsp-yaml-parser has explicit `Document` types with
     directives metadata.
- Key files to check:
  - `rlsp-yaml/src/formatter.rs` — comment handling
  - `rlsp-yaml/src/selection.rs` — span computation
  - `rlsp-yaml/src/validators.rs` — duplicate key detection
  - `rlsp-yaml/src/server.rs` — document handling

## Steps

- [x] Audit each workaround — verify it still exists and
      is removable
- [x] Simplify comment handling in formatter (c640283)
- [x] Simplify span access in selection (4694575)
- [ ] Simplify duplicate key detection in validators
- [ ] Leverage lossless alias mode where beneficial
- [ ] Leverage chomping preservation in formatter
- [ ] Verify all tests pass after each simplification

## Tasks

### Task 1: Audit workarounds (DONE — no code commit, read-only audit)

Read the relevant source files and determine which of the
6 workarounds still exist after the migration. Some may
have been partially or fully removed during the type
migration. Report findings before proceeding.

- [x] Check formatter.rs for comment extract/reattach logic
- [x] Check selection.rs for recursive span computation
- [x] Check validators.rs for text-based duplicate key scan
- [x] Check for eager alias resolution patterns
- [x] Check formatter.rs for chomping indicator handling
- [x] Check document boundary handling
- [x] Report which workarounds remain and their locations

**Result:** 3 of 6 workarounds remain: #1 (comment
extract/reattach), #2 (recursive span computation),
#3 (text-based duplicate key scan). Workarounds #4
(alias resolution), #5 (chomping), #6 (document
boundaries) are already gone.

**Files:** all `rlsp-yaml/src/*.rs`

### Task 2: Remove comment workaround in formatter (DONE — c640283)

The formatter currently works around missing inline
comments by raw-text scanning (`extract_comments`) and
signature-matching reattachment (`attach_comments`) —
~300 lines of workaround code.

The parser already emits Comment events with correct
spans. The loader discards them inside collections
(`*pos += 1; continue` at loader.rs:378-382 and
419-423). Fix the **loader** to attach comments to
adjacent nodes instead of discarding them, then replace
the formatter's raw-text workaround with AST-based
comment access.

- [x] Add comment storage to the node model in
      `rlsp-yaml-parser/src/node.rs`
- [x] Update the loader (`rlsp-yaml-parser/src/loader.rs`)
      to store comments on adjacent nodes instead of
      discarding them inside mappings/sequences
- [x] Replace `extract_comments`/`attach_comments` in
      `formatter.rs` with AST-based comment access
- [x] Verify formatter comment tests pass
- [x] Verify comment preservation in round-trip tests

**Files:** `rlsp-yaml-parser/src/node.rs`,
`rlsp-yaml-parser/src/loader.rs`, `formatter.rs`

### Task 3: Fix container spans and simplify selection (DONE — 4694575)

Container nodes (Mapping, Sequence) currently have
incomplete spans — the loader uses only the
MappingStart/SequenceStart event span, discarding the
MappingEnd/SequenceEnd span (loader.rs:387-390 and
427-430). Fix the **loader** to combine start and end
event spans, then remove the recursive `effective_start`/
`effective_end` workaround in `selection.rs`.

- [x] Update the loader (`rlsp-yaml-parser/src/loader.rs`)
      to read MappingEnd/SequenceEnd spans and construct
      full container spans: `Span { start: start.start,
      end: end.end }`
- [x] Remove `effective_start`, `effective_end`, and
      `is_zero_span` from `selection.rs`
- [x] Replace recursive span computation in
      `collect_ancestor_spans` with direct `node.loc`
      access
- [x] Verify selection range tests pass

**Files:** `rlsp-yaml-parser/src/loader.rs`, `selection.rs`

### Task 4: Simplify duplicate key detection

If text-based duplicate key scanning still exists, replace
with AST-based key comparison using preserved keys.

- [ ] Identify the text-scanning duplicate key detection
- [ ] Replace with AST-based comparison
- [ ] Verify validator tests pass

**Files:** `validators.rs`

### Task 5: Leverage remaining new capabilities

Address any remaining workarounds: alias mode, chomping
preservation, document boundaries.

- [ ] Switch to lossless alias mode if beneficial for
      anchor/alias navigation features
- [ ] Verify chomping indicators are correctly used in
      formatter output
- [ ] Clean up any document boundary workarounds
- [ ] Final verification: all tests pass, clippy clean

**Files:** various

## Decisions

- **Audit first, then remove.** The migration may have
  already removed some workarounds. Don't assume all 6
  still exist — check first.
- **One workaround per task.** Each removal is independently
  testable and committable. If one workaround turns out to
  be deeply entangled, it doesn't block the others.
- **Preserve behavior.** The goal is code simplification,
  not behavior change. All existing tests must continue to
  pass. If a workaround removal changes observable behavior,
  that's a separate feature decision.
- **Verify parser capabilities before claiming limitations.**
  A previous execution of this plan incorrectly marked two
  workarounds as "needs parser enhancements" when the parser
  already provided the required data — Comment events and
  MappingEnd/SequenceEnd spans. The issue was in the loader
  layer, not the parser. Before concluding that a workaround
  cannot be removed due to parser limitations: (1) check
  what the tokenizer/event layer actually emits, (2) check
  whether the loader consumes or discards that data, and
  (3) identify the specific layer that needs the change.
  "The parser doesn't support X" is not acceptable without
  citing the specific production or event that is missing.
