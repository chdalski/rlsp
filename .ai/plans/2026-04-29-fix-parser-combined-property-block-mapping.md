**Repository:** root
**Status:** Completed (2026-04-29)
**Created:** 2026-04-29

## Goal

Fix two parser bugs where a block mapping with both
`&anchor` and `!tag` properties loses one of them:
Bug A (`&anchor !tag` order — anchor migrates to first
key) and Bug B (`!tag &anchor` order — user tag dropped).
After the fix, `rlsp_yaml_parser::load(...)` produces an
AST where both properties are attached to the block
mapping node regardless of property order. The fix is
verified by a new AST-shape test matrix covering the
`(anchor only, tag only, anchor+tag anchor-first,
anchor+tag tag-first) × (block mapping, block sequence,
flow mapping, flow sequence)` cross product, locking down
the full matrix against regressions.

## Context

### Root cause (traced to exact code)

The parser classifies each property (`&anchor` or `!tag`)
as `Standalone` (nothing follows it on the line) or
`Inline` (content follows on the same line). When two
properties appear in sequence on one line (e.g. `&base
!mytag`), the first is classified as `Inline` (because the
second follows), and the second is classified as
`Standalone` (nothing follows it).

When the mapping opens, `handle_mapping_entry` in
`rlsp-yaml-parser/src/event_iter/block/mapping.rs:530-548`
extracts the mapping's anchor and tag. It checks:
1. `pending_collection_anchor` (a displacement buffer), then
2. `pending_anchor` — but only if `Standalone`.

Same for tags: `pending_collection_tag` first, then
`pending_tag` only if `Standalone`.

This is correct for the intended use case: a `Standalone`
property belongs to the upcoming collection, and an
`Inline` property belongs to the key scalar. But when
**two collection-level properties** appear on the same
line, the first one is classified as `Inline` (because the
second follows it) and neither the displacement logic nor
the mapping extraction code promotes it to a slot the
mapping can reach.

### Bug A: `&base !mytag\n  timeout: 30`

1. `&base` scanned → `Inline` (because `!mytag` follows)
   → stored in `pending_anchor`.
2. `!mytag` scanned → `Standalone` (nothing follows)
   → stored in `pending_tag`.
3. Mapping opens → checks `pending_collection_anchor`
   (None) → checks `pending_anchor` as `Standalone`
   (no, it's `Inline`) → mapping anchor = None.
4. Tag: checks `pending_collection_tag` (None) → checks
   `pending_tag` as `Standalone` (yes) → mapping tag =
   `!mytag`.
5. `Inline` anchor `"base"` lingers in `pending_anchor`
   → attaches to next scalar event → "timeout" gets
   the anchor.

### Bug B: `!mytag &base\n  timeout: 30`

1. `!mytag` scanned → `Inline` (because `&base` follows)
   → stored in `pending_tag`.
2. `&base` scanned → `Standalone` (nothing follows)
   → stored in `pending_anchor`.
3. Mapping opens → checks `pending_collection_anchor`
   (None) → checks `pending_anchor` as `Standalone`
   (yes) → mapping anchor = `"base"`.
4. Tag: checks `pending_collection_tag` (None) → checks
   `pending_tag` as `Standalone` (no, it's `Inline`)
   → mapping tag = None → resolver injects
   `tag:yaml.org,2002:map`.
5. `Inline` tag `"!mytag"` lingers in `pending_tag`
   → dropped.

### Why block sequences work

`handle_sequence_entry` in
`event_iter/block/sequence.rs:193-207` uses `.take()` on
both `pending_anchor` and `pending_tag` **regardless of
variant** — it accepts both `Standalone` and `Inline`. The
mapping code discriminates against `Inline` variants; the
sequence code does not. This is the asymmetry.

### The fix

Two symmetric additions in
`rlsp-yaml-parser/src/event_iter/step.rs`:

**In the tag scanner's Standalone branch (after line 614):**
After setting `pending_tag = Standalone(...)`, check if
`pending_anchor` is `Some(PendingAnchor::Inline(...))`.
If so, displace it to `pending_collection_anchor` (and
its loc). Reasoning: the anchor was classified as `Inline`
only because THIS tag followed it on the line. Now that
the tag is `Standalone` (nothing more follows), both
properties are collection-level. The `Inline` anchor must
be promoted to the collection buffer so the mapping
extraction finds it.

**In the anchor scanner's Standalone branch (after line
828):** Same, symmetrically: after setting
`pending_anchor = Standalone(...)`, check if `pending_tag`
is `Some(PendingTag::Inline(...))`. If so, displace it to
`pending_collection_tag` (and its loc).

This fix does NOT change:
- The mapping extraction code in `mapping.rs` (its
  `Standalone`-only check is correct for the key-level
  vs collection-level distinction).
- The sequence extraction code in `sequence.rs` (already
  correct).
- The `Inline` branch of either scanner (those are correct
  for the case where the property is followed by actual
  content, not another property).

### Verification by non-regression

The fix must not break:
- `cargo test` (all existing tests)
- yaml-test-suite conformance (`cargo test --test
  conformance` or equivalent)
- Corpus invariants (`cargo test --test
  corpus_invariants`) — I10 (formatter round-trip)
  should continue to pass

### Reader survey: callers of `.anchor()` and `.tag()`

The fix changes what `anchor()` and `tag()` return for
block mapping nodes that have combined properties
(previously one was `None`; after the fix both are
`Some`). All callers in `rlsp-yaml/src/` were checked:

| File | Usage | Safe? |
|---|---|---|
| `editing/formatter.rs:1413` | `match (value.anchor(), user_tag)` — the `(Some, Some)` arm at line 1414 emits `: &name !tag`. Already handles the combined case; just never reached before. | Yes |
| `editing/formatter.rs:669` | Scalar anchor emission — prepends `&name ` when `node.anchor()` is `Some`. Not reached for mapping values (handled at line 1413). | Yes |
| `editing/formatter.rs:700,712` | `prepend_collection_properties(doc, node.anchor(), tag.as_deref(), ...)` — correctly builds `&anchor !tag` prefix. | Yes |
| `validation/validators.rs` | `validate_unused_anchors` collects all `anchor()` values into a set. A newly-present anchor on a mapping is correctly tracked as "defined." | Yes |
| `navigation/rename.rs` | Anchor rename uses `node.anchor()` to find anchor definitions. Newly-present anchors would be correctly found and renamed. | Yes |
| `navigation/references.rs` | `goto_definition`/`find_references` use `node.anchor()` to resolve `*alias` → `&anchor`. Newly-present anchors are correctly resolvable. | Yes |
| `editing/code_actions/delete_anchor.rs` | Offers "delete anchor" action when `anchor()` is `Some`. Newly-present anchors would be correctly detected and deletable. | Yes |
| `analysis/semantic_tokens.rs` | Highlights anchors/tags based on presence. Newly-present anchors/tags get correctly highlighted. | Yes |
| `tests/corpus_invariants.rs` I5 | Checks `anchor().is_some() == anchor_loc().is_some()`. The fix makes both consistently `Some` (previously only `anchor_loc` was `Some` in some paths). Still consistent. | Yes |
| `tests/corpus_invariants.rs` I6 | Checks tag consistency with `tag_loc`. Same reasoning as I5. | Yes |

All callers use `if let Some(...)`, `match`, or
`.is_some()` patterns that handle the
previously-`None`-now-`Some` case correctly. No caller
has exclusion logic that would break if an anchor/tag
is present on a mapping where it previously wasn't.

### Key files

| File | Role |
|---|---|
| `rlsp-yaml-parser/src/event_iter/step.rs` | Fix site: displacement logic in tag scanner (~line 614) and anchor scanner (~line 828) |
| `rlsp-yaml-parser/src/event_iter/block/mapping.rs` | Unchanged — mapping extraction at lines 530-548 |
| `rlsp-yaml-parser/src/event_iter/block/sequence.rs` | Unchanged — reference for correct behavior |
| `rlsp-yaml-parser/src/loader.rs` | Test site: loader builds AST from events; new AST-shape tests use `load()` |

### References

- YAML 1.2 §6.9 Node Properties:
  https://yaml.org/spec/1.2.2/#69-node-properties
- Follow-up entry being resolved:
  `.ai/memory/project_followup_plans.md` — "Parser
  mis-attaches or drops properties on block mapping
  with combined `&anchor` and user `!tag`"
- Downstream follow-up now unblocked:
  `.ai/memory/project_followup_plans.md` — "Code-action
  property-preservation invariant + fix
  `string_to_block_scalar` doubling"

## Steps

- [x] Fix displacement logic in step.rs + add AST-shape
      test matrix + verify all existing tests pass

## Tasks

### Task 1: Fix combined-property displacement + AST-shape test matrix

Fix the two displacement bugs in
`rlsp-yaml-parser/src/event_iter/step.rs` and add a
comprehensive AST-shape test matrix in the parser's
loader tests.

**Parser fix (two symmetric additions in step.rs):**

- [x] In the tag scanner's `else` (Standalone) branch,
      after `self.pending_tag = Some(PendingTag::
      Standalone(resolved_tag, tag_span))` at ~line 614:
      add a check — if `self.pending_anchor` is
      `Some(PendingAnchor::Inline(..))`, take it and
      move the name to `self.pending_collection_anchor`
      and the loc to
      `self.pending_collection_anchor_loc`. Then clear
      `self.pending_anchor` (it was taken). ~5 lines.
- [x] In the anchor scanner's `else` (Standalone) branch,
      after `self.pending_anchor = Some(PendingAnchor::
      Standalone(name, anchor_span))` at ~line 828:
      add a symmetric check — if `self.pending_tag` is
      `Some(PendingTag::Inline(..))`, take it and move
      the cow to `self.pending_collection_tag` and the
      loc to `self.pending_collection_tag_loc`. Then
      clear `self.pending_tag` (it was taken). ~5 lines.

**AST-shape test matrix (new tests in loader.rs):**

- [x] Add tests to `rlsp-yaml-parser/src/loader.rs`
      (existing `#[cfg(test)] mod tests` block) that
      verify correct anchor and tag attachment across
      the full property × shape matrix. Each test calls
      `load(yaml_input)` and asserts the outer collection
      node's `anchor()` and `tag()` values.

      The matrix is 4 property configurations × 4
      collection shapes = 16 test cases:

      **Property configurations:**
      - Anchor only: `&myanchor`
      - Tag only: `!mytag`
      - Anchor-first: `&myanchor !mytag`
      - Tag-first: `!mytag &myanchor`

      **Collection shapes (all as mapping values):**
      - Block mapping: `key: <props>\n  child: val\n`
      - Block sequence: `key: <props>\n  - item\n`
      - Flow mapping: `key: <props> {child: val}\n`
      - Flow sequence: `key: <props> [item]\n`

      For each case, assert:
      - `outer_value.anchor()` matches expected
        (`Some("myanchor")` when anchor present, `None`
        otherwise)
      - `outer_value.tag()` includes `"!mytag"` when
        user tag present (exact string depends on
        resolver behavior — check what the parser
        produces for `!mytag` and assert that)
      - First child key/item does NOT have the anchor
        or tag that belongs to the parent collection

      Use `rstest` `#[case::name]` syntax if available
      (check if `rstest` is a dev-dependency of
      `rlsp-yaml-parser`; if not, use individual
      `#[test]` functions with descriptive names
      following the `should_*` or `property_shape_`
      naming convention).

      Of the 16 cases, 14 should already pass (only the
      two `anchor+tag on block mapping` cases are
      currently buggy). Verify all 16 pass after the
      fix.

**Architecture doc update:**

- [x] Update the "Pending anchor and tag" section of
      `rlsp-yaml-parser/docs/architecture.md` (around
      lines 280-284) to describe the new displacement
      promotion path: when two collection-level
      properties appear on the same line, the first is
      classified `Inline` (because the second follows
      it), but the second property's `Standalone`
      classification triggers promotion of the first to
      `pending_collection_anchor`/`pending_collection_tag`
      so the mapping extraction can find it. Keep the
      existing description of `Standalone`/`Inline` and
      add a note about this exception — do not rewrite
      the section.

**Regression verification:**

- [x] `cargo test` (full workspace) passes with zero
      failures.
- [x] `cargo test --test corpus_invariants` passes
      (I10 formatter round-trip still green).
- [x] `cargo clippy --all-targets` exits with zero
      warnings.
- [x] `cargo fmt` applied.
- [x] If any yaml-test-suite conformance test changes
      status (previously passing now fails, or vice
      versa), report the test IDs to the lead via
      SendMessage before submitting to the reviewer.
      The expected outcome is: no conformance status
      changes (the fix makes our parser MORE conformant,
      not less).

**Files changed:**

- `rlsp-yaml-parser/src/event_iter/step.rs` — two
  displacement additions (~5 lines each)
- `rlsp-yaml-parser/src/loader.rs` — 16 new AST-shape
  tests in the existing test module
- `rlsp-yaml-parser/docs/architecture.md` — update
  "Pending anchor and tag" section to document the
  displacement promotion path

**No other files should change.** The formatter, code
actions, and rlsp-yaml crate do not need modifications
— all callers of `.anchor()` and `.tag()` handle the
previously-`None`-now-`Some` transition correctly (see
Reader survey in Context). The formatter's
`key_value_to_doc` already has the `(Some(name),
Some(t))` match arm at line 1414 that correctly emits
`: &anchor !tag` when both are present; it just never
received both from the parser before this fix.

The follow-up queue entry "Parser mis-attaches or drops
properties on block mapping with combined `&anchor` and
user `!tag`" is removed by the lead during the
plan-completion commit, not as part of this task (same
convention as the I10 plan's cleanup of its follow-up
entry).

**Completed:** commit `8f7cb0a`. 726/726
conformance tests pass, 83 corpus invariants pass,
full workspace tests green, clippy clean, fmt clean.

Acceptance: Both bugs are fixed — `load("key: &a !t\n
v: 1\n")` and `load("key: !t &a\n  v: 1\n")` produce
ASTs where the outer mapping value has both
`anchor()=Some("a")` and `tag()` containing `"!t"`. All
16 AST-shape test matrix cases pass. All existing tests
pass (zero regressions). `cargo clippy --all-targets`
and `cargo fmt` clean.

## Decisions

- **Fix displacement, not extraction.** The mapping
  extraction code's `Standalone`-only check is correct
  for distinguishing collection-level from key-level
  properties; changing it would break the legitimate
  `&key_anchor key: value` case. The fix belongs in
  the displacement logic where the classification
  decision is made — promoting an `Inline` first
  property to the collection buffer when the second
  property on the same line turns out to be
  `Standalone`.
- **One task, not split.** The fix is two ~5-line
  additions. The 16-case test matrix is the fix's
  verification. Both are inherently coupled — you
  can't land tests without the fix (they'd fail) and
  you can't land the fix without tests (no
  verification). Splitting would be artificial.
- **No formatter or code-action changes.** The
  formatter already has the correct match arm for
  combined anchor+tag on block mapping values
  (formatter.rs:1414). Code actions that call
  `format_subtree` on block collections will
  automatically produce correct output once the
  parser feeds them the right AST. The follow-up
  entry for the code-action property-preservation
  invariant is now unblocked by this fix but is NOT
  in scope here.
- **Test matrix uses `load()`, not `parse_events()`.**
  The bug is in the event stream's property
  classification, but the user-facing API is the
  loader's AST. Testing at the `load()` level catches
  both the event-level fix and any loader-level
  mishandling. Event-level tests are lower-priority
  and not included here — they can be added in a
  separate conformance pass if needed.

## Non-Goals

- Fixing the code-action `string_to_block_scalar`
  doubling or the `block_to_flow` fragile safety.
  Those are downstream consumers that have their own
  follow-up plan entry.
- Adding corpus files with combined-property block
  mappings. The AST-shape test matrix is sufficient for
  targeted verification; corpus expansion is a separate
  follow-up.
- Modifying the mapping extraction code in `mapping.rs`.
  The extraction code is correct; the classification/
  displacement code is wrong.
