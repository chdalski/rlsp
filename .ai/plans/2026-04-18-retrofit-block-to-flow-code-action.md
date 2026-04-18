**Repository:** root
**Status:** InProgress
**Created:** 2026-04-18

## Goal

Retrofit the `block_to_flow` code action in
`rlsp-yaml/src/editing/code_actions.rs` from its current
text-surgery implementation to an AST-first approach using
the existing `format_subtree` API. This preemptively
eliminates the class of structural-text-surgery bugs the
flow-to-block fix just addressed (heuristic key detection,
missing flow-unsafe character escaping on mapping values,
indentation math) before any of them surface as
user-reported destructive behavior.

## Context

### Why this is queued next

The plan at
`.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`
(Completed 2026-04-18) rewrote `flow_map_to_block` /
`flow_seq_to_block` from text surgery to AST +
`format_subtree`. `block_to_flow` is the inverse
conversion and belongs to the same risk class
(structural text surgery on YAML collections).

User preference, recorded during the previous plan's
wrap-up: exclude the possibility of these bugs
preemptively rather than wait for a user report.
Current Move 0 corpus harness (I4 scalar preservation)
passes on the existing seed corpus, but "passes on
current corpus" does not mean "free of bug class" —
the corpus is a subset of real-world YAML, and the
bugs that this plan closes exist on inputs the corpus
doesn't yet contain.

### Current implementation defects

`block_to_flow` at `code_actions.rs:376` takes
`lines: &[&str]`, `line_idx: usize`, and a URI. It is
cursor-driven (dispatched from the cursor-line context
actions at line 66), not diagnostic-driven. Traced
defects:

1. **Heuristic key detection.** Line 385 uses
   `trimmed.find(':')?` to locate the key/value
   separator. This matches the FIRST colon on the
   line. On inputs like `anchor_name: &anchor` (the
   anchor token contains `:` in some forms) or
   quoted keys containing literal colons (`"foo:bar":
   baz`) the detection produces the wrong key.
2. **No flow-unsafe escaping on mapping values.**
   Lines 441-443 wrap children as
   `{<children joined by ", ">}`. Each child is the
   trimmed block-mapping line (e.g. `url: http://foo`).
   Joining them in flow context produces
   `{url: http://foo, …}` which is invalid flow
   mapping syntax: the `:` inside the URL gets
   interpreted as a new key-value pair. Destructive
   to semantics.
3. **`quote_flow_item` applied only to sequence items.**
   Lines 432-438 apply the quote-escape helper to
   sequence items before joining. Mapping values get
   no such treatment — any flow-unsafe character in
   a value produces broken YAML.
4. **Heuristic indent-walking.** Lines 409-424 walk
   subsequent lines looking for children at
   `base_indent + 2`. Any deeper indent triggers
   "too complex" (`return None`). The
   indentation-as-proxy approach drifts from the
   parser's own view of structure — comments,
   blank lines, and unusual indent conventions
   could misclassify children.
5. **Early return on nested structures** (line 420).
   Not destructive, but narrow. The current action
   refuses to convert anything with nested children.
   An AST-based rewrite could lift this restriction
   cheaply — but scope-wise this plan preserves the
   current narrow behavior to keep the change surface
   minimal.

Defects (1), (2), (3), (4) are all destructive risk.
Not known to fire on the Move 0 corpus (I4 passes
clean as of 2026-04-18), but the bug class is real.

### Why the AST+formatter approach eliminates the bug class

Same reasoning as the flow-to-block retrofit:

- The parser already identified the block collection
  correctly — style, extent, scalar values — via the
  `rlsp-yaml-parser` AST. `block_to_flow` does NOT
  need to re-derive this from text.
- `rlsp-yaml/src/editing/formatter.rs`'s `format_subtree`
  public API (added 2026-04-18 in
  `8dfe0e0`) takes a `Node<Span>` and emits its
  current style. If the node is cloned with
  `CollectionStyle::Flow`, `format_subtree` produces
  the flow form with correct quoting, escaping, and
  indentation.
- The code action becomes: find the block Node at the
  cursor position, clone with style flipped to Flow,
  call `format_subtree`, emit a `TextEdit` covering
  the node's span.

### Cursor-based trigger (different from flow-to-block)

The flow-to-block code actions were diagnostic-driven:
each `flowMap` / `flowSeq` diagnostic offered a quick
fix. `block_to_flow` has no diagnostic trigger — it's
offered as a cursor-position refactor (the dispatch
at line 66 calls it unconditionally for the
cursor-line context actions). The AST retrofit needs
to locate the correct block Node by cursor position,
not by diagnostic range.

Approach: walk `docs` for a `Node::Mapping` or
`Node::Sequence` with `style: CollectionStyle::Block`
whose `loc` starts on the cursor line. Prefer the
innermost match so the action targets the block
closest to the cursor.

### Non-Goals

- **Expanding the action to nested block structures.**
  The current implementation refuses nested children
  via `return None`. The AST+formatter approach can
  handle nesting automatically, but expanding scope
  here would be a behavior change beyond bug
  prevention. Preserve the current narrow
  refuse-nested behavior: pre-check the candidate
  block for nested collection children and return
  `None` if found. Lifting this restriction is a
  future enhancement plan.
- **`string_to_block_scalar`.** Separate queue
  item (#3). Scalar-to-scalar conversion with
  different shape; audited separately.
- **Other code actions.** `tab_to_spaces`,
  `quoted_bool_to_unquoted`, `yaml11_*`,
  `delete_unused_anchor` — span-local text
  replacements, not structural. Not in scope.
- **Retiring `quote_flow_item`.** It is still
  referenced by `block_to_flow` pre-retrofit. After
  this plan it may become unused; deletion is part
  of Task 2's cleanup, not a goal in itself.
- **Move 2's fixture pattern.** Plan uses existing
  test infrastructure (unit tests + Move 0 corpus
  invariants).

### References

- Prior plan (the flow-to-block retrofit that
  established the AST+formatter pattern):
  `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`
- `format_subtree` public API definition and tests:
  `rlsp-yaml/src/editing/formatter.rs` (added
  2026-04-18 in commit `8dfe0e0`)
- Current `block_to_flow`:
  `rlsp-yaml/src/editing/code_actions.rs:376-490`
- Current cursor dispatch that invokes it:
  `rlsp-yaml/src/editing/code_actions.rs:66`
- Move 0 corpus harness that exercises the code
  action on every seed file:
  `rlsp-yaml/tests/corpus_invariants.rs`
- Root CLAUDE.md "One parser, one AST" rule.
- YAML 1.2 flow collection syntax (the target form):
  https://yaml.org/spec/1.2.2/#74-flow-collection-styles

## Steps

- [ ] Rewrite `block_to_flow` as AST + `format_subtree`
      with cursor-based block-node matching
- [ ] Cleanup — retire `quote_flow_item` if unused,
      add regression tests for the defect classes,
      update `feature-log.md`

## Tasks

### Task 1: Rewrite `block_to_flow` via AST + `format_subtree`

Replace the text-surgery implementation with an
AST-first approach. Keep the cursor-based trigger
(unlike the flow-to-block actions, this one is not
offered from diagnostics). Preserve the narrow
refuse-nested behavior.

- [ ] Change `block_to_flow` signature to
      `fn block_to_flow(docs: &[Document<Span>],
      text: &str, line_idx: usize, uri: &Url) ->
      Option<CodeAction>`. The call site at
      `code_actions.rs:66` passes `docs` (already
      available — the Move-1-follow-up `code_actions`
      public signature takes `docs` as its first
      parameter).
- [ ] Walk `docs` to find the innermost block
      collection Node whose `loc.start.line` equals
      `line_idx + 1` (LSP line 0-based; parser line
      1-based — verify the convention used elsewhere
      in this module):
  - `Node::Mapping { style: CollectionStyle::Block,
    entries, loc, .. }` — candidate mapping
  - `Node::Sequence { style: CollectionStyle::Block,
    items, loc, .. }` — candidate sequence
  - Prefer the innermost match so nested structures
    target the closest block to the cursor (but see
    next bullet for the nested-rejection check)
- [ ] Pre-check the candidate for nested collection
      children. Walk its direct children
      (mapping entries' values, sequence items): if
      any is a `Node::Mapping` or `Node::Sequence`
      (regardless of style), return `None`. This
      preserves the current "refuse nested" behavior
      and keeps the plan scope minimal.
- [ ] Clone the candidate node with
      `style: CollectionStyle::Flow`.
- [ ] Compute `base_indent` from the parent context
      (the same strategy the flow-to-block retrofit
      uses). For a mapping value, `base_indent` is
      `key_indent + 2`; for a standalone block at
      column C, `base_indent = C`.
- [ ] Call `format_subtree(&cloned,
      &YamlFormatOptions::default(), base_indent)`.
- [ ] Emit a `TextEdit` whose range is the AST
      node's `loc` (NOT full lines). Use the same
      `block_text_and_start_col` helper pattern
      established in Task 2 of the flow-to-block
      retrofit — for cursor-based dispatch the
      helper may need a tweak or a sibling function;
      reuse where possible.
- [ ] Preserve the existing title selection:
      "Convert block to flow style" vs
      "Convert block to flow style (long line)"
      based on a length heuristic. Keep the
      threshold at 80 chars unless the formatter's
      output makes a different threshold obvious.
- [ ] Update call site at `code_actions.rs:66` to
      pass `docs` and `text`.
- [ ] Update unit tests at `code_actions.rs:1120,
      1128, 1136, 1160, 1184` to the new
      signature. The intent of each test stays:
  - `should_not_offer_block_to_flow_for_inline_value`
    — an already-inline `key: value` stays a no-op
  - `should_not_offer_block_to_flow_for_nested_structures`
    — nested children → `None` (the new pre-check
    enforces this)
  - `should_quote_bracket_containing_item_when_converting_block_to_flow`
    — after retrofit, the formatter handles quoting;
    assert the formatted output is well-formed (and
    matches the expected flow-quoted form)
  - `should_quote_item_containing_comma_when_converting_block_to_flow`
    — same
  - `should_not_quote_safe_items_when_converting_block_to_flow`
    — formatter doesn't over-quote safe items
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full suite passes
  - `cargo test --test corpus_invariants` passes
    (empty SKIP_LIST preserved; no regression —
    `block_to_flow` on the corpus continues to
    preserve scalars under I4)
  - Surprise Failure Protocol applies: any
    unexpected corpus-harness failure → developer
    messages the lead, no skip-list entry added
    without direction.

Acceptance: `block_to_flow` consumes the AST;
produces correct flow output including proper
quoting/escaping of flow-unsafe characters in
mapping values; refuses nested structures (matches
prior behavior); existing unit-test intents
preserved; workspace suite green; corpus SKIP_LIST
still empty.

### Task 2: Cleanup — retire helpers, regression tests, docs

Finalize the change: delete now-unused text-surgery
helpers, add regression tests targeting the specific
defect classes the old implementation failed on, and
update user-facing docs.

- [ ] Check `rlsp-yaml/src/editing/code_actions.rs`
      for remaining callers of `quote_flow_item`. If
      no callers remain after Task 1, delete it.
      Otherwise leave in place with a one-line
      comment noting the remaining caller.
- [ ] Audit allow-list status: verify
      `rlsp-yaml/tests/parser_boundary_audit.rs`
      allow-list remains at exactly the 4 validator
      entries. The `block_to_flow` rewrite doesn't
      touch public signatures (it's a private
      function), so no audit regex interaction.
      Confirm nothing shifts by running
      `cargo test --test parser_boundary_audit`.
- [ ] Add unit tests for the specific defect
      classes the old implementation failed on —
      these are NEW tests, not replacements for the
      existing ones:
  - Mapping value containing a colon
    (`key: url: http://foo\n  name: bar`) — the
    old implementation would produce invalid flow
    YAML; the new one must produce correct output
    (either quoted or escaped appropriately — let
    the formatter decide)
  - Mapping value containing a comma
    (`key: foo, bar` in the block form) — flow
    output must quote/escape correctly
  - Mapping value containing a brace or bracket —
    same
  - Mapping key that is quoted with an embedded
    colon (`"foo:bar": value`) — edge case; the
    AST preserves key identity, so the converted
    flow form must retain it
  - Anchored block mapping
    (`&anchor_name key: value`) — the parser
    records the anchor; the flow output must
    preserve it (verify against whatever the
    formatter emits for anchored nodes)
- [ ] Update `rlsp-yaml/docs/feature-log.md`: add a
      new entry recording the AST-based block-to-flow
      rewrite. Short form matching the flow-to-block
      entry's shape.
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full suite passes
  - `cargo test --test corpus_invariants` passes
    (empty SKIP_LIST)

Acceptance: if `quote_flow_item` has no remaining
callers after Task 1, it is deleted; if callers
remain, it is left in place with a one-line comment
identifying the remaining caller. New regression
tests cover each defect class from Context; audit
allow-list unchanged at 4 entries; workspace suite
green; `feature-log.md` records the change.

## Decisions

- **Approach: AST + `format_subtree`.** Mirrors the
  flow-to-block retrofit just completed. Same
  architectural rationale (one parser, one AST; the
  parser already identified the block collection
  correctly; the formatter already knows how to emit
  flow form with correct quoting). Rejected
  "tighten text-surgery in place" because the bug
  class — not just the specific bugs — is what we
  want to eliminate.
- **Preserve the refuse-nested behavior.** Current
  implementation returns `None` for nested
  structures (`block_to_flow` at line 420). AST
  approach could handle nesting automatically, but
  expanding scope would convert this from a
  bug-prevention plan into a feature plan. Keep
  narrow. If nesting support is wanted later, file
  a separate enhancement plan.
- **Cursor-based dispatch preserved.** Unlike
  flow-to-block which responds to diagnostics,
  `block_to_flow` is offered at every cursor line
  that matches a block collection start. The AST
  retrofit preserves that — cursor-driven, not
  diagnostic-driven.
- **`quote_flow_item` retirement is contingent.**
  Deleted only if no other caller remains after
  Task 1. Task 2 checks.
- **Audit allow-list unaffected.** `block_to_flow`
  is a private function (the audit regex targets
  `pub fn`). The public `code_actions` entry
  already takes `docs` as first parameter
  (landed in the flow-to-block plan Task 2); no
  signature shift here. Allow-list stays at 4.
- **No new formatter behavior introduced.**
  `format_subtree` already handles flow emission
  via the existing `flow_mapping_to_doc` /
  `flow_sequence_to_doc` dispatch. This plan reuses
  that infrastructure unchanged.
- **Move 0 corpus harness as the acceptance gate.**
  I4 (scalar preservation) on the seed corpus
  already passes for `block_to_flow`; it must
  continue to pass after the retrofit. Any surprise
  failure triggers the Surprise Failure Protocol.
