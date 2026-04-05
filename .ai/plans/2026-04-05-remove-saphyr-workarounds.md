**Repository:** root
**Status:** NotStarted
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

- [ ] Audit each workaround — verify it still exists and
      is removable
- [ ] Simplify comment handling in formatter
- [ ] Simplify span access in selection
- [ ] Simplify duplicate key detection in validators
- [ ] Leverage lossless alias mode where beneficial
- [ ] Leverage chomping preservation in formatter
- [ ] Verify all tests pass after each simplification

## Tasks

### Task 1: Audit workarounds

Read the relevant source files and determine which of the
6 workarounds still exist after the migration. Some may
have been partially or fully removed during the type
migration. Report findings before proceeding.

- [ ] Check formatter.rs for comment extract/reattach logic
- [ ] Check selection.rs for recursive span computation
- [ ] Check validators.rs for text-based duplicate key scan
- [ ] Check for eager alias resolution patterns
- [ ] Check formatter.rs for chomping indicator handling
- [ ] Check document boundary handling
- [ ] Report which workarounds remain and their locations

**Files:** all `rlsp-yaml/src/*.rs`

### Task 2: Remove comment workaround in formatter

If the comment extract/reattach workaround still exists,
replace it with direct use of comments from the parsed
AST (`Document.comments` or comment nodes in the tree).

- [ ] Identify the comment extraction code path
- [ ] Replace with AST-based comment access
- [ ] Verify formatter tests pass
- [ ] Verify comment preservation in round-trip tests

**Files:** `formatter.rs`

### Task 3: Simplify span access in selection

If recursive span computation still exists, replace with
direct `node.loc` span access.

- [ ] Identify `effective_start`/`effective_end` or similar
      recursive helpers
- [ ] Replace with direct span field access where possible
- [ ] Verify selection range tests pass

**Files:** `selection.rs`

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
