**Repository:** root
**Status:** InProgress
**Created:** 2026-04-18

## Goal

Eliminate the destructive `flow_map_to_block` and
`flow_seq_to_block` quick-fix code actions by replacing
their text-surgery implementations with an AST-first
approach: add a `format_subtree` public API to the
formatter, and rewrite both code actions to find the
target AST node, flip its `CollectionStyle` to `Block`,
and re-emit the subtree via the formatter. The
user-facing destructive bug (applying the quickfix to
legitimate flow mappings like `- { target: linux, os:
ubuntu }` drops scalar content) disappears; the audit
allow-list shrinks by one (`code_actions`) since the
public entry point can now take the AST instead of
re-parsing from `&str`; the text-surgery helpers
(`split_flow_items`, `quote_flow_item`, per-line range
math) retire.

## Context

### Architectural program placement

This plan is a follow-up to Move 1 (the "one parser,
one AST" rule and the `validate_flow_style` retrofit at
`.ai/plans/2026-04-18-one-parser-one-ast.md`, Completed
2026-04-18). Move 1 applied the rule to validators;
this plan applies it to the destructive code-action
pair. Task 4 of Move 1 added
`rlsp-yaml/tests/parser_boundary_audit.rs` with an
allow-list entry for `code_actions` in
`rlsp-yaml/src/editing/code_actions.rs` — that entry
will be removed by this plan.

### The defects in the current implementation

Traced during the originating bug investigation. Both
`flow_map_to_block` (lines 104-186 of
`rlsp-yaml/src/editing/code_actions.rs`) and
`flow_seq_to_block` (lines 190-270) take `lines:
&[&str]` plus a `Diagnostic` and do text surgery:

1. **Full-line replace.** Both functions build an
   `edit_range` covering `col 0 → line.len()` of the
   diagnostic's line. Anything on that line outside
   the flow-map span is destroyed. Common breakage:
   sequence-item flow mappings like `- { target: x, os:
   y }` where the `-` and surrounding whitespace get
   overwritten.
2. **Single-line scope.** The internal `find_closing_char`
   helper (deleted from validators in Move 1 Task 2 but
   the same pattern lives in code_actions via
   `flow_content = &line[start_col..end_col]`) only
   looks at the diagnostic's line. Multi-line flow
   mappings are never correctly extracted.
3. **Key-reconstruction fragility.** Lines 125-132 of
   `flow_map_to_block` detect "this flow map is a
   mapping value" via
   `prefix.trim_end().ends_with(':')`. When the prefix
   is `- ` (sequence item) or anything else, the
   else-branch at line 163 discards the key structure
   entirely — the user's `GITHUB_TOKEN: ${{ … }}` case
   fell into this branch.
4. **Comma splitter is the only correct part.**
   `split_flow_items` (line 767) tracks quotes and
   nesting depth correctly. That helper is retired
   nonetheless — the formatter already knows how to
   emit a block mapping from an AST node; no manual
   splitting needed.

### Why the AST-first approach is clean

`rlsp-yaml-parser::Node` carries `style: CollectionStyle`
on both `Mapping` and `Sequence` variants
(`rlsp-yaml-parser/src/node.rs:56-89`), and the
existing formatter already dispatches on that style via
`mapping_to_doc` (line 1041) / `flow_mapping_to_doc`
(line 1071) and `sequence_to_doc` (line 1399) /
`flow_sequence_to_doc` (line 1425). Converting a flow
collection to block style is semantically "change the
node's `style` to `Block` and re-emit" — the formatter
handles indentation, key/value spacing, scalar
quoting, and everything else automatically.

The formatter's current public API
(`format_yaml(text, options) -> String` at line 364)
only operates on whole documents. The missing piece
is a subtree-level entry point:

```rust
pub fn format_subtree(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    base_indent: usize,
) -> String
```

The internal `node_to_doc` function (line 480) already
produces a `Doc` from any `Node<Span>`; `format_subtree`
wraps it with the `Doc` → text rendering step (via
`rlsp-fmt`) and handles base indentation.

### Why `code_actions` changes signature

After this plan, the public `code_actions` entry at
`rlsp-yaml/src/editing/code_actions.rs:21` takes
`&[Document<Span>]` in addition to (or instead of)
`text: &str`. The flow-to-block code actions find
their target node by matching the diagnostic's `range`
against AST node spans. Without the AST, the public
entry would have to re-parse the text — violating the
"one parser, one AST" rule. Following the validator
precedent (Move 1 Task 2's
`validate_flow_style(docs: &[Document<Span>])`), we
take the AST as a parameter. The call site at
`server.rs` already has `result.documents` in scope.

This removes `code_actions` from the audit allow-list
at `rlsp-yaml/tests/parser_boundary_audit.rs`.

### Current `flow_map_to_block` / `flow_seq_to_block`
### behavior post-Move-1

Both are **private** `fn`s taking `lines: &[&str]`.
They don't appear on the audit allow-list directly
(the audit only targets `pub fn`). The public
`code_actions` entry dispatches to them.

After Move 1 Task 2's `end_col + 1` range adjustment,
the code action is now reachable on legitimate
sequence-item flow maps (`- { ... }`). Those cases
produce destructive output. This is the remaining
user-visible defect this plan closes.

### Non-Goals

- **`block_to_flow`** (line 272 of `code_actions.rs`).
  The inverse conversion. Might benefit from the same
  AST+formatter approach but is not known to be
  destructive. Out of scope; file a follow-up plan if
  investigation finds defects.
- **Other code actions** — `tab_to_spaces`,
  `delete_unused_anchor`, `quoted_bool_to_unquoted`,
  `string_to_block_scalar`, the YAML 1.1 bool/octal
  actions. Each has its own shape; this plan does not
  touch them.
- **Move 2's fixture pattern.** This plan uses the
  existing test infrastructure (unit tests + Move 0
  corpus invariants). Fixtures for code actions are
  Move 2's responsibility.
- **Introducing new formatter behavior.** The
  `format_subtree` API reuses `node_to_doc` unchanged;
  this plan adds only the wrapping entry point, not
  new emission logic.

### References

- Move 1 plan (the validator precedent for
  AST-first retrofits):
  `.ai/plans/2026-04-18-one-parser-one-ast.md`
- Move 0 plan (corpus invariants harness that will
  exercise this fix):
  `.ai/plans/2026-04-18-corpus-invariants-scaffold.md`
- Current `flow_map_to_block`:
  `rlsp-yaml/src/editing/code_actions.rs:104-186`
- Current `flow_seq_to_block`:
  `rlsp-yaml/src/editing/code_actions.rs:190-270`
- Formatter's `node_to_doc` (reusable internal API):
  `rlsp-yaml/src/editing/formatter.rs:480`
- Formatter's current public entry point:
  `rlsp-yaml/src/editing/formatter.rs:364`
- Audit allow-list that will shrink:
  `rlsp-yaml/tests/parser_boundary_audit.rs`
- Root CLAUDE.md "One parser, one AST" rule — the
  principle this plan applies to code actions.
- YAML 1.2 flow and block collection styles:
  https://yaml.org/spec/1.2.2/

## Steps

- [x] Add `format_subtree` public API to the formatter
- [ ] Rewrite `flow_map_to_block` and
      `flow_seq_to_block` to use AST + `format_subtree`;
      change `code_actions` public signature to take
      AST
- [ ] Cleanup — retire text-surgery helpers, shrink
      audit allow-list, add regression coverage

## Tasks

### Task 1: Add `format_subtree` public API to the formatter

Expose a subtree-level emission entry point that any
caller can use to render a single AST node to text.
This is the enabling API for Task 2's code-action
rewrite. Independent and unit-testable in isolation.

- [x] Add `pub fn format_subtree(node: &Node<Span>,
      options: &YamlFormatOptions, base_indent: usize)
      -> String` to `rlsp-yaml/src/editing/formatter.rs`.
      Implementation calls the existing `node_to_doc`
      to get a `Doc`, then renders the `Doc` via the
      same `rlsp-fmt` path `format_yaml` uses, with
      `base_indent` applied so the output's lines
      (except the first) carry `base_indent` leading
      spaces.
- [x] Decide first-line-indent semantics: the caller
      passes `base_indent` meaning "every line but the
      first gets this many leading spaces; the first
      line starts at column 0 and the caller is
      responsible for positioning it in the larger
      document." Document this in the function's
      rustdoc. (This matches how the code action will
      use it — the first line of the block-style
      output is emitted in place of the flow `{`,
      with trailing lines indented to match.)
- [x] Before writing tests, inspect `node_to_doc`'s
      current handling of empty collections (`{}`,
      `[]`). Record the observed behavior — does it
      emit the empty collection inline as `{}` /
      `[]`, or does it produce block-style empty
      output? This is the expected behavior the test
      below asserts against.
- [x] Add unit tests for `format_subtree` covering:
  - Simple flow mapping node → block-style text
    with correct base indent
  - Flow sequence node → block-style text
  - Nested flow mapping inside a flow sequence →
    nested block output
  - Scalar node (trivial case, for API symmetry) →
    scalar text
  - Empty flow mapping `{}` → assert against the
    behavior recorded in the inspection sub-task
    above (a single specified outcome, not a
    branch-based criterion)
  - Multi-line flow mapping input (via the parser)
    converted to block form
  - Various `base_indent` values (0, 2, 8) produce
    correctly-indented output
- [x] `cargo fmt`, `cargo clippy --all-targets`,
      `cargo test` — all clean.

Acceptance: `format_subtree` is callable from outside
the formatter module; unit tests pass; rustdoc
documents the indent semantics; full workspace suite
stays green.

**Completed:** commit `8dfe0e0` — public
`format_subtree` added to `formatter.rs` reusing
`node_to_doc` + `fmt_format`, no new emission logic.
Rustdoc documents first-line-col-0 semantics and
continuation-line indentation. Empty collections
remain inline (`{}` / `[]`) per `node_to_doc`'s
existing short-circuit. 14 unit tests cover the
variants from the plan plus supplementary coverage
(parameterized base_indent, both style-conversion
paths via options flag and direct style mutation).

### Task 2: Rewrite `flow_map_to_block` and `flow_seq_to_block` via AST + `format_subtree`

Change the public `code_actions` entry signature to
accept the AST, find the target node by span, and
invoke `format_subtree` on a clone of the target node
with its `style` flipped to `Block`. Emit a `TextEdit`
replacing the AST node's span (not the diagnostic's
line) with the formatted output.

- [ ] Change `code_actions` signature from
      `pub fn code_actions(text: &str, cursor_range:
      Range, diagnostics: &[Diagnostic], uri: &Url)`
      to
      `pub fn code_actions(docs: &[Document<Span>],
      text: &str, cursor_range: Range, diagnostics:
      &[Diagnostic], uri: &Url)` — keep `text`
      because some other code actions in this module
      still need it (e.g. `tab_to_spaces`). The audit
      allow-list entry for `code_actions` goes away
      because the *first* parameter shape no longer
      matches the violator regex.
- [ ] Update call site at `rlsp-yaml/src/server.rs`
      to pass `&result.documents`.
- [ ] Update all test call sites that invoke
      `code_actions` directly — parse + pass docs.
      Includes `rlsp-yaml/tests/corpus_invariants.rs`
      (I3 and I4 invariants both call `code_actions`).
- [ ] Rewrite `flow_map_to_block`:
  - New signature:
    `fn flow_map_to_block(docs: &[Document<Span>],
    text: &str, diag: &Diagnostic, uri: &Url) ->
    Option<CodeAction>`
  - Walk the AST to find a `Node::Mapping { style:
    CollectionStyle::Flow, .. }` whose `loc` matches
    the diagnostic's `range` (after applying the same
    `end_col + 1` convention used in Move 1 Task 2 —
    the parser's `MappingEnd` is zero-width at the
    closing `}`)
  - Compute `base_indent` from the node's position:
    if the matched node's start column is at column
    C, the block form's continuation lines should be
    indented to at least C+2 (or whatever
    `YamlFormatOptions` specifies for mapping
    indent)
  - Clone the node with `style: CollectionStyle::Block`
  - Call `format_subtree(&cloned, &options,
    base_indent)` (options can be defaults for now;
    the code action doesn't read LSP client settings
    in this pass)
  - Produce a `TextEdit` with range = the node's
    original `loc` span, new_text = formatted output
  - Return the `CodeAction`
- [ ] Rewrite `flow_seq_to_block` symmetrically.
      Same shape — just target `Node::Sequence` with
      flow style.
- [ ] Remove the two existing `SKIP_LIST` entries
      from `rlsp-yaml/tests/corpus_invariants.rs`
      (currently at lines 70-85): the
      `(github-actions-matrix.yml, I4)` and
      `(release-plz-workflow.yml, I4)` entries that
      cite this very plan. After the code-action fix
      lands, I4 passes on those files; leaving the
      entries causes the harness's
      `PassedUnexpected` outcome → test failure. The
      `SKIP_LIST` must be empty after this task.
- [ ] Mirror the SKIP_LIST removal in
      `rlsp-yaml/tests/corpus/WORKLIST.md`: delete
      the two table rows so the "Current failures"
      table becomes empty (keep the skill-output
      prose, shrink-only discipline paragraph, and
      the empty-state note at the bottom).
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full suite passes
  - Move 0 corpus-invariants harness (I1-I4) passes
    with **empty SKIP_LIST** (zero entries — the
    two prior entries are removed, not replaced).
    No new entries may be added during this task.
    Any surprise failure (anything the harness flags
    that isn't accounted for by the code-action fix
    itself) triggers the Surprise Failure Protocol:
    developer messages the lead before adding any
    skip-list entry.

Acceptance: both code actions work AST-first; applied
to legitimate `- { target: linux, os: ubuntu }`
produces clean block-style output with no scalar
content loss; applied to multi-line flow mappings
produces correct output; Move 0 corpus I4 passes on
all 4 files; `SKIP_LIST` is empty (both removed
entries gone, no new entries added); `WORKLIST.md`'s
"Current failures" table is empty.

### Task 3: Cleanup — retire helpers, shrink audit, regression coverage, docs

Finalize the change: delete now-unused text-surgery
helpers, remove the allow-list entry that covered
`code_actions`, add regression coverage for the
specific defect classes, and update user-facing docs.

- [ ] Verify no remaining callers of `split_flow_items`
      or `quote_flow_item` (both in
      `rlsp-yaml/src/editing/code_actions.rs`).
      Delete if unused; leave in place with a comment
      if `block_to_flow` or another code action still
      uses them.
- [ ] Remove the `code_actions` entry from the
      allow-list in `rlsp-yaml/tests/parser_boundary_audit.rs`.
      Per-entry verification: confirm the audit
      still passes overall (the entry is no longer
      matched by the regex because `code_actions`'
      first parameter is now `docs: &[Document<Span>]`,
      not `text: &str`). Document the shrink in the
      commit message.
- [ ] Add unit tests in
      `rlsp-yaml/src/editing/code_actions.rs` (module
      tests) covering the specific defect classes the
      old implementation failed on:
  - Sequence-item flow mapping (`- { target: x, os:
    y }`) — applied quickfix produces clean block
    form, no data loss, original `-` preserved
  - Multi-line flow mapping — applied quickfix
    correctly spans all input lines and produces
    block output
  - Flow mapping as a mapping value (`key: { a: 1, b:
    2 }`) — applied quickfix produces `key:\n  a:
    1\n  b: 2\n`
  - Nested flow mapping inside flow sequence —
    applied quickfix handles the nesting
  - Empty flow mapping `{}` — quickfix not offered
    (or offered as no-op, matching current
    non-empty-filter behavior at line 218)
  - All-scalars flow sequence (`[a, b, c]`) —
    symmetric test for `flow_seq_to_block`
  - Flow mapping with nested flow sequence as a
    value (`{a: [1, 2]}`) — preserves both structures
    after the outer-to-block conversion
- [ ] Update `rlsp-yaml/docs/feature-log.md`:
      (a) add a new entry recording that the
      destructive flow-to-block quick fix has been
      replaced with an AST+formatter approach; specific
      inputs that previously lost content now produce
      correct block output.
      (b) update the existing Corpus Invariant Harness
      entry — after Task 2, the skip-list no longer
      tracks any failures and WORKLIST.md's failure
      worklist is empty. Revise sentences that claim
      "tracks currently-failing (file, invariant) pairs"
      and "See `tests/corpus/WORKLIST.md` for the
      current failure worklist" to reflect the
      now-empty steady state.
- [ ] Run the Move 0 corpus-invariants harness
      explicitly as a final check: `cargo test --test
      corpus_invariants`. Must exit successfully with
      zero `SKIP_LIST` entries (both prior entries
      removed in Task 2 of this plan; no new entries
      added).
- [ ] `cargo fmt`, `cargo clippy --all-targets`,
      `cargo test` — all clean.

Acceptance: audit allow-list retains exactly four
entries — `validate_unused_anchors`,
`validate_custom_tags`, `validate_key_ordering`
(in `validators.rs`) and `validate_schema` (in
`schema_validation.rs`) — with `code_actions`
removed; unused text-surgery helpers
(`split_flow_items` / `quote_flow_item`) are
deleted if no other callers remain; regression
tests cover each defect class (sequence-item,
multi-line, key-value, nested, empty,
flow-sequence, nested-mixed); Move 0 corpus I4
passes cleanly with zero `SKIP_LIST` entries;
`feature-log.md` records the change and updates
the Corpus Invariant Harness entry to reflect
the empty-steady-state.

## Decisions

- **Approach chosen: AST + formatter (option B from
  pre-plan discussion).** The alternative (tighten
  text-surgery in place) was rejected because it
  conflicts with the "one parser, one AST" rule
  established in Move 1 and leaves the code action as
  a text-scanner. The AST+formatter approach retires
  the text-surgery codepath entirely.
- **`format_subtree` semantics: first line at column
  0, continuation lines indented by `base_indent`.**
  Matches how the code action will use it — the first
  line of the block output replaces the flow `{`
  in-place, continuation lines are indented to match
  the parent structure. Other semantics (e.g., indent
  the first line too) could be added later if a
  different caller needs them; this pass covers only
  what the flow-to-block code actions need.
- **`code_actions` takes both `docs` and `text`.**
  Some other code actions in the module
  (`tab_to_spaces`, `quoted_bool_to_unquoted`,
  string-to-block-scalar, etc.) still read raw text
  directly. Retrofitting all of them is out of scope;
  keeping `text` as a second parameter allows this
  plan to land without touching the other actions.
  Future follow-ups can retrofit them one by one and
  eventually drop the `text` parameter when no caller
  needs it.
- **Audit allow-list shrinks by one entry.** Only
  `code_actions` comes off. The four validator
  entries (`validate_unused_anchors`,
  `validate_custom_tags`, `validate_key_ordering`,
  `validate_schema`) remain untouched — their
  retrofits are separate follow-up plans.
- **No formatter settings (LSP-client-configurable)
  in this pass.** The code action calls
  `format_subtree` with `YamlFormatOptions::default()`
  rather than reading the user's configured
  formatting options. If users want their quickfix
  output to match their formatter settings, that's a
  follow-up. Defaults match what the server's
  formatter produces on the same input.
- **No `block_to_flow` in scope.** Inverse action;
  not known to be destructive; retrofitting it is a
  separate plan if warranted.
- **Move 0 corpus harness as the acceptance gate.**
  Task 2's acceptance depends on I1-I4 passing with
  empty skip-list. If a surprise failure surfaces,
  Surprise Failure Protocol applies — block on lead
  direction. This keeps the corpus-invariants gate
  load-bearing rather than advisory.
- **Stale signature references in Completed plans
  are out of scope.** Move 0's plan file
  (`.ai/plans/2026-04-18-corpus-invariants-scaffold.md`)
  describes the old `code_actions(text, ...)` shape
  in its Context and test-harness sub-tasks. After
  this plan lands, those descriptions become stale.
  Completed plans are historical records — the live
  source of truth is the code itself. This plan does
  not update Move 0's plan file; readers cross-
  referencing a Completed plan should treat its
  signature references as snapshots of their
  execution time, not as current documentation.
