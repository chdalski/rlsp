**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-18

## Goal

Retrofit the four remaining text-surgery code actions in
`rlsp-yaml/src/editing/code_actions.rs` to the AST +
`format_subtree` pattern established by the prior
retrofit series: `yaml11_bool_actions`,
`schema_yaml11_bool_type_actions`, `yaml11_octal_actions`,
and `delete_unused_anchor`. After this plan lands, every
code action except `tab_to_spaces` (legitimately
text-only — whitespace normalization per spec §6.1) will
consume the AST, and the "one parser, one AST"
architectural program is complete for code actions.

## Context

### Why this plan bundles four retrofits

Each of these actions has a similar shape and low per-
action implementation complexity: find the target node by
diagnostic span, clone with the desired change, re-emit
via `format_subtree`, replace the node's `loc`-bounded
span. The prior four retrofits (`flow_map_to_block`,
`flow_seq_to_block`, `block_to_flow`,
`string_to_block_scalar`, `quoted_bool_to_unquoted`) have
established and validated the pattern. Each of those was
its own plan because the pattern was being discovered and
refined. The pattern is now stable; bundling four small
retrofits into one plan with three tasks reduces
plan-review overhead without sacrificing per-task
reviewability.

### The four current implementations

All four are in `rlsp-yaml/src/editing/code_actions.rs`
and share the same shape: take `lines: &[&str]` +
`Diagnostic` + `Url`, build a full-line `edit_range`
covering cols 0 → `line.len()`, construct replacement
strings from `before` + modified-value + `after` slices.

**`yaml11_bool_actions` (lines 852-907)** — Emitted when a
diagnostic flags a YAML 1.1 boolean literal (`yes`, `no`,
`on`, `off`, `y`, `n`, `true`, `false`, etc.) whose
interpretation differs from the 1.2 parser. Produces two
actions: "Quote value" (wraps the value with `"..."`) and
"Convert to boolean" (via `scalar_helpers::yaml11_bool_canonical`
to pick a canonical `true`/`false`).

**`schema_yaml11_bool_type_actions` (lines 971-1017)** —
Triggered by schema validation when a YAML 1.1 bool
appears where the schema expects a different type. Gated
by `scalar_helpers::is_yaml11_bool(value)`. Returns a
single action ("Convert to boolean") — unlike
`yaml11_bool_actions`, no "Quote value" alternative is
offered for the schema-type variant.

**`yaml11_octal_actions` (lines 911-967)** — Emitted when
a diagnostic flags a YAML 1.1 octal literal (`0755`-form).
Two actions: "Quote as string" and "Convert to YAML 1.2
octal" (`0755` → `0o755`).

**`delete_unused_anchor` (lines 588-627)** — Emitted when
an anchor definition has no aliases referencing it.
Removes the `&name` token plus trailing space. Single
action.

### Shared defects

Traced and shared across all four:

1. **Full-line replacement.** Every action builds
   `edit_range` over cols 0 → `line.len()` of the
   diagnostic's line, then computes the replacement as
   `before + modified + after`. Any content on the line
   outside the diagnostic span is reconstructed from
   the pre-edit slice — fine until the reconstruction
   mis-handles a trailing comment with special chars,
   multi-byte characters, or a tab-aligned continuation
   marker. Matches the defect class the
   `quoted_bool_to_unquoted` retrofit closed.
2. **Text-level quoting.** `yaml11_bool_actions` and
   `yaml11_octal_actions` produce "Quote value" actions
   that wrap the raw slice with double quotes. If the
   slice contains a double quote or backslash, the
   output is not a valid double-quoted YAML scalar. The
   AST-first form asks the formatter to emit a
   properly-escaped double-quoted scalar instead of
   string-concatenating quotes on raw text.
   (`schema_yaml11_bool_type_actions` doesn't produce a
   "Quote value" action, so this particular defect
   doesn't apply there — but the full-line replacement
   and no-AST-validation defects do.)
3. **No AST validation.** The diagnostic's range points
   at text the validator flagged — but the code action
   trusts the text between start_col and end_col is
   what the validator intended. A stale diagnostic
   against changed buffer text could point at
   different content; the AST would surface the
   mismatch.

### Why `delete_unused_anchor` is in the retrofit set

Span-local enough that text-edit is technically safe.
Retrofitted nonetheless for architectural consistency:
the action clones the node with `anchor: None` and
re-emits via `format_subtree`, which eliminates the
text-slice reconstruction and lets the formatter handle
any whitespace/anchor-position subtleties per spec.

### AST-first pattern per action

All four retrofits follow the same scaffold:

1. **Walk AST** for the node the diagnostic range
   identifies (match by `loc` overlapping the
   diagnostic's range — precedent from
   `flow_map_to_block` at `173f838`, which established
   the `end_col + 1` convention for diagnostic-to-loc
   matching).
2. **Clone the node** with the desired change:
   - `yaml11_bool_actions` — two clones: one with
     `style: ScalarStyle::DoubleQuoted` (for "Quote
     value"); one with `value =
     yaml11_bool_canonical(...)` and
     `style: ScalarStyle::Plain` (for "Convert to
     boolean").
   - `schema_yaml11_bool_type_actions` — one clone with
     `value = yaml11_bool_canonical(...)` and
     `style: ScalarStyle::Plain` (for "Convert to
     boolean"). Single action preserved.
   - `yaml11_octal_actions` — two clones: `style:
     ScalarStyle::DoubleQuoted` for "Quote as string";
     `value = format!("0o{}", ...)` for "Convert to
     YAML 1.2 octal".
   - `delete_unused_anchor` — one clone with `anchor:
     None`.
3. **Call `format_subtree`** on each clone with
   `base_indent` computed from the node's parent context
   (if the node is a mapping value, `base_indent =
   key_col`; if a sequence item, the sequence's
   indentation level; etc. — the `string_to_block_scalar`
   retrofit's correction from commit `370b8c4` confirmed
   that `base_indent = key_col` is right for scalars
   because the formatter's printer already applies
   `tab_width`).
4. **Emit `TextEdit`** over the node's `loc` span (NOT
   full line). Trailing comments and whitespace
   preserved. For `delete_unused_anchor`, the edit span
   must additionally include the anchor-token prefix
   since the anchor is a property of the node rather
   than part of its value — the diagnostic's range
   already covers `&name`, so the emit uses the
   union of diagnostic range + scalar `loc` (or
   equivalently, the diagnostic start to the scalar end).

### Program-level consolidation note

Per the test-engineer's scan-existing-tests protocol and
the plan-reviewer's program-level consolidation check,
`code_actions.rs` is currently the file with the highest
accumulated test-to-production ratio in the project.
This plan does NOT include an in-plan consolidation task
— the dedicated post-program cleanup plan, queued in
`.ai/memory/project_followup_plans.md`, owns that work.
Per-task TE consultation during this plan's execution
WILL produce per-task Consolidation sections (pruning
duplicates, merging over-granular tests) as part of the
test-engineer's new standard operating procedure. The
cross-module/file-splitting work waits for the
dedicated cleanup plan after all retrofits land.

### References

- Prior retrofit plans establishing the pattern:
  - `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`
    (added `format_subtree`, retrofitted
    `flow_map_to_block`/`flow_seq_to_block`)
  - `.ai/plans/2026-04-18-retrofit-block-to-flow-code-action.md`
    (cursor-based dispatch, refuse-nested preservation)
  - `.ai/plans/2026-04-18-retrofit-string-to-block-scalar-code-action.md`
    (scalar-node retrofit; settled `base_indent =
    key_col`)
  - `.ai/plans/2026-04-18-retrofit-quoted-bool-to-unquoted-code-action.md`
    (scalar style retrofit; most recent template)
- `format_subtree` public API:
  `rlsp-yaml/src/editing/formatter.rs` (commit
  `8dfe0e0`)
- Current implementations targeted by this plan:
  - `yaml11_bool_actions`:
    `rlsp-yaml/src/editing/code_actions.rs:852-907`
  - `schema_yaml11_bool_type_actions`:
    `rlsp-yaml/src/editing/code_actions.rs:971-1017`
  - `yaml11_octal_actions`:
    `rlsp-yaml/src/editing/code_actions.rs:911-967`
  - `delete_unused_anchor`:
    `rlsp-yaml/src/editing/code_actions.rs:588-627`
- Shared helpers the retrofits will preserve:
  `scalar_helpers::yaml11_bool_canonical`,
  `scalar_helpers::is_yaml11_bool`.
- Audit allow-list — current baseline is 108
  entries; this plan removes the four
  `TodoRetrofit` entries for `yaml11_bool_actions`
  (line 136), `yaml11_octal_actions` (line 143),
  `schema_yaml11_bool_type_actions` (line 150), and
  `delete_unused_anchor` (line 129), leaving 104
  entries after all three tasks complete. File:
  `rlsp-yaml/tests/parser_boundary_audit.rs`.
- Root CLAUDE.md "One parser, one AST" rule.
- YAML 1.1 boolean/octal interpretation differences
  vs YAML 1.2: `.claude/rules/` and the validator
  source in `rlsp-yaml/src/validation/validators.rs`.

## Non-Goals

- **`tab_to_spaces`.** Stays as text replacement.
  Documented in
  `.ai/memory/project_followup_plans.md` as NOT a
  retrofit candidate — tabs are pre-parse lexical, per
  YAML 1.2 §6.1; they're not represented in the AST.
- **Feature-level retrofits** (`hover_at`,
  `complete_at`, `format_on_type`,
  `find_document_links`, `find_colors`,
  `folding_ranges`, `selection_ranges`,
  `semantic_tokens`, `document_symbols`,
  `goto_definition`, `find_references`,
  `prepare_rename`, `rename`). Each is a separate
  plan per the queue.
- **Validator retrofits** (`validate_unused_anchors`,
  `validate_custom_tags`, `validate_key_ordering`,
  `validate_schema`). Each is a separate plan.
- **Audit v2** (broaden the regex to detect private
  helpers with `text`/`line`/`lines`/`content`/`source`/`input`
  params). Sequenced immediately after this plan per
  the queue. Not bundled here to keep the per-plan
  reviewable surface bounded.
- **Move 2's fixture pattern.** Plan uses existing
  test infrastructure (unit tests + Move 0 corpus
  invariants).
- **Introducing new formatter behavior.**
  `format_subtree` already handles every emission
  case these retrofits need (plain, quoted, block
  scalars; anchored nodes). No new formatter logic.
- **Post-program cleanup** (test dedup + code
  simplification + logical `code_actions.rs` split).
  Dedicated follow-up plan queued for after every
  retrofit in the program lands.
- **Behavior expansion.** Each retrofit preserves
  current dispatch shape and action titles. No new
  actions added, no currently-offered action removed.

## Steps

- [x] Retrofit `yaml11_bool_actions` and
      `schema_yaml11_bool_type_actions` (combined
      because they share structure)
- [x] Retrofit `yaml11_octal_actions`
- [x] Retrofit `delete_unused_anchor`

## Tasks

### Task 1: Retrofit `yaml11_bool_actions` and `schema_yaml11_bool_type_actions` via AST + `format_subtree`

These two retrofits share the "find the YAML 1.1 bool
scalar by diagnostic range" step but differ in the
actions they produce (`yaml11_bool_actions` returns
two actions — Quote + Convert; `schema_yaml11_bool_type_actions`
returns one — Convert). Retrofitting together captures
the shared finder once and keeps the action-shape
divergence explicit.

- [x] Change both signatures to
      `fn yaml11_bool_actions(docs: &[Document<Span>],
      text: &str, diag: &Diagnostic, uri: &Url) ->
      Vec<CodeAction>` and the analogous form for
      `schema_yaml11_bool_type_actions`. Pass `docs`
      and `text` from the call sites at
      `code_actions.rs:46-51` (`diag_actions` closure;
      `docs` and `text` already captured in scope since
      Move 1).
- [x] Extract a shared finder —
      `fn find_yaml11_bool_scalar<'a>(docs: &'a
      [Document<Span>], diag: &Diagnostic) ->
      Option<(&'a Node<Span>, &'a Span, usize)>` —
      that both callers use. The finder:
  1. Walks the AST for a `Node::Scalar` whose `loc`
     matches the diagnostic range (using the
     `end_col + 1` convention from `flow_map_to_block`).
  2. Returns `None` if the scalar's value isn't a
     YAML 1.1 bool per
     `scalar_helpers::is_yaml11_bool`.
  3. Returns `Some((node, loc, base_indent))` where
     `base_indent` is the parent-context column for
     `format_subtree` (same `key_col` convention from
     `string_to_block_scalar`).
- [x] `yaml11_bool_actions` calls the finder, then
      builds two clones (DoubleQuoted; canonical
      Plain) and emits two `TextEdit`s over the
      scalar's `loc` using `format_subtree`. Preserves
      the two current action titles ("Quote value",
      "Convert to boolean").
- [x] `schema_yaml11_bool_type_actions` calls the
      finder, then builds one clone (canonical Plain)
      and emits one `TextEdit`. Preserves the single
      current action title ("Convert to boolean").
      No "Quote value" action is added — current
      behavior shape is preserved per the Non-Goals.
- [x] Update call sites at
      `code_actions.rs:46-51` to pass `docs` and
      `text`. Both functions are currently called from
      the `diag_actions` closure.
- [x] Consult the test-engineer per the input gate.
      The TE's new standard operating procedure
      (agent file step 3) requires scanning the
      existing test block for duplicates and
      producing a Consolidation section listing
      tests to retire or merge alongside the
      regression tests to add. Follow that protocol.
- [x] Regression tests (cross-reference with TE's
      Consolidation section to avoid duplicating
      existing coverage):
  - Quote action produces a valid double-quoted
    scalar for a value containing a `"` character
    (exercises the escape-handling gap of the old
    text-wrap implementation)
  - Convert action normalizes `yes` → `true`,
    `off` → `false`, etc., for all 22 YAML 1.1
    bool tokens
  - Trailing comment on the same line is preserved
    by both actions (exercises the full-line-
    replacement defect)
  - Diagnostic range mid-line (value in a sequence
    item with cursor-column > 0) produces an edit
    starting at the scalar's span, not at column 0
    (same assertion pattern as
    `quoted_bool_to_unquoted`)
  - `schema_yaml11_bool_type_actions` gated out
    when the diagnostic's span is NOT a YAML 1.1
    bool (negative case for the shared finder's
    early return)
  - `schema_yaml11_bool_type_actions` returns exactly
    one action (regression against the current
    single-action shape — the shared finder must not
    leak a "Quote value" action into the schema-type
    variant)
- [x] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full workspace green
  - `cargo test --test corpus_invariants` passes
    with empty SKIP_LIST
  - `cargo test --test parser_boundary_audit`
    passes with allow-list at exactly 106 entries
    (down from the 108-entry baseline — Task 1
    removes the `yaml11_bool_actions` and
    `schema_yaml11_bool_type_actions` entries)
- [x] Per the updated test-engineer protocol,
      obtain TE output-gate sign-off that both the
      regression additions AND the Consolidation
      deletions from the input gate's test list
      have landed in the diff.

Acceptance: both functions consume the AST; share a
finder helper; `yaml11_bool_actions` produces
properly-escaped quoted output via the formatter;
`schema_yaml11_bool_type_actions` continues to
return exactly one action; all defect-class
regressions from the Context section covered plus
any additional scenarios the TE surfaces; corpus
SKIP_LIST stays empty; audit allow-list at 106
(down from 108).

**Landed:** commit `64d5726` (see `git log` — SHA
may be superseded by follow-up amend). `const ALLOW_LIST` shrank
from 100 to 98 (−2). The plan's 108/106 figures
counted `AllowEntry {` occurrences including
test-helper constructors; the net delta matches
and the `cargo test --test parser_boundary_audit`
gate passed. Subsequent tasks should reference the
actual `ALLOW_LIST` count (now 98); post-Task-2
target is 97, post-Task-3 target is 96.

### Task 2: Retrofit `yaml11_octal_actions` via AST + `format_subtree`

Standalone retrofit — similar shape to Task 1's
functions but the convert step produces a YAML 1.2
octal literal (`0o` prefix) rather than a canonical
bool. Not combined with Task 1 because the canonical
transform differs and the diagnostic trigger is a
different validator rule.

- [x] Change signature to
      `fn yaml11_octal_actions(docs:
      &[Document<Span>], text: &str, diag:
      &Diagnostic, uri: &Url) -> Vec<CodeAction>`.
- [x] Walk AST to find the `Node::Scalar` whose
      `loc` matches the diagnostic range (same
      `end_col + 1` convention as Task 1). If the
      value isn't a YAML 1.1 octal (starts with
      `0` followed by octal digits, length ≥ 2),
      return `vec![]`.
- [x] Build two clones:
  1. Quote form: `style:
     ScalarStyle::DoubleQuoted`, value unchanged.
  2. Convert form: `value = format!("0o{}",
     &original[1..])` (drop the leading `0`,
     prepend `0o`), `style: ScalarStyle::Plain`.
- [x] Emit two `TextEdit`s over the scalar's
      `loc` span.
- [x] Update call site at
      `code_actions.rs:46-51` (`diag_actions`
      closure).
- [x] TE input-gate consultation per the standard
      protocol (scan existing tests, produce
      Consolidation section).
- [x] Regression tests (augment with TE's
      Consolidation decisions):
  - Quote action on `0777` produces a valid
    double-quoted scalar (the string `"0777"`),
    not a malformed `""0777""` or similar
  - Convert action on `0755` produces `0o755` and
    on `0777` produces `0o777`
  - Convert action rejects non-octal values
    (`08`, `09` contain non-octal digits) — the
    AST check catches this where the old
    text-surgery didn't
  - Trailing comment preservation
  - Mid-line cursor / sequence-item value
    (assertion that edit range starts at scalar,
    not col 0)
- [x] Build/test gates: fmt, clippy, full test,
      corpus invariants with empty SKIP_LIST,
      audit allow-list at 105 entries (one fewer
      than after Task 1).
- [x] TE output-gate sign-off covering
      regression adds + Consolidation deletes.

Acceptance: `yaml11_octal_actions` consumes the
AST; emits the converted octal form correctly;
rejects non-octal inputs via the AST check;
trailing-comment and mid-line-cursor regressions
pass; corpus SKIP_LIST stays empty; audit
allow-list at 105.

**Landed:** commit `485b89c` (see `git log` — SHA
may be superseded by follow-up amend). `const
ALLOW_LIST` at 97 (−1 from 98).

### Task 3: Retrofit `delete_unused_anchor` via AST + `format_subtree`

Different shape from Tasks 1 and 2 — removes an
anchor property from a node rather than changing a
scalar value or style. Diagnostic-based dispatch;
single action returned.

- [x] Change signature to
      `fn delete_unused_anchor(docs:
      &[Document<Span>], text: &str, diag:
      &Diagnostic, uri: &Url) -> Option<CodeAction>`.
- [x] Walk AST to find the node carrying the
      anchor. The diagnostic's range points at the
      `&name` token. The anchored node's `loc`
      typically starts AT or AFTER the anchor
      token; different parser implementations
      differ. Strategy:
  1. Walk every node (scalar, mapping, sequence,
     alias).
  2. Match when the node's `anchor ==
     Some(name)` AND the diagnostic range is
     "just before" the node's `loc` (i.e., the
     diagnostic range ends within 1-2 columns of
     the node's `loc.start`). Verify the exact
     positional convention by reading the parser's
     loader code and cross-referencing with the
     current text-edit's range assumptions.
  3. If no match is found (stale diagnostic,
     already-removed anchor), return `None`.
- [x] Clone the matched node with `anchor: None`.
      All other fields preserved (including any
      tag, value/entries/items, and nested
      structure).
- [x] Call `format_subtree` on the clone with
      `base_indent` set appropriately for the
      node's position in its parent. For a mapping
      value, `base_indent = key_col` (pattern
      from `string_to_block_scalar`).
- [x] Emit `TextEdit` with range = union of
      diagnostic range (covering `&name`) and the
      node's `loc` — i.e., from the anchor's
      start column through the node's end column.
      This replaces the entire `&anchor_name
      <value>` region with the formatter's clean
      re-emission.
- [x] Update call site at
      `code_actions.rs:46-51` (`diag_actions`
      closure).
- [x] TE input-gate consultation (scan existing
      tests, Consolidation section).
- [x] Regression tests (augment with TE's
      Consolidation decisions):
  - Anchor on a scalar value (`key: &a "hello"`)
    — the action removes `&a ` and leaves
    `key: "hello"`
  - Anchor on a block mapping value (multi-line
    anchor case) — the action removes the anchor
    without disturbing the mapping's contents or
    indentation
  - Anchor on a flow collection (`list: &nums
    [1, 2, 3]`) — same
  - Anchor with a tag (`key: &a !!str "hello"`)
    — the tag is preserved after anchor removal
  - Trailing comment preservation
  - Stale diagnostic (anchor already removed
    from buffer text) — returns `None`, no edit
    emitted
- [x] Build/test gates: fmt, clippy, full test,
      corpus invariants with empty SKIP_LIST,
      audit allow-list at 104 entries (one fewer
      than after Task 2).
- [x] TE output-gate sign-off covering
      regression adds + Consolidation deletes.

Acceptance: `delete_unused_anchor` consumes the
AST; removes the anchor via AST cloning and
re-emission; preserves tags, values, and trailing
comments; returns `None` on stale diagnostics;
corpus SKIP_LIST stays empty; audit allow-list at
104.

**Landed:** commit `0bc9d38` (see `git log` — SHA
may be superseded by follow-up amend). `const
ALLOW_LIST` at 96 (−1 from 97).

## Decisions

- **Bundling three tasks into one plan.** The AST +
  `format_subtree` pattern is stable after five
  successful retrofits; per-plan overhead (plan
  review, team cycling, handoff ceremony) is no
  longer earning its cost for these low-complexity
  retrofits. Per-task reviewability is preserved —
  each task lands one commit and is independently
  scoped.
- **Combining `yaml11_bool_actions` with
  `schema_yaml11_bool_type_actions` in Task 1.**
  Queue note at
  `.ai/memory/project_followup_plans.md` flagged
  this possibility. Inspection of both functions
  confirmed they share the "find YAML 1.1 bool
  scalar from diagnostic range" step but diverge
  on action shape (two vs one). The shared finder
  helper captures the overlap; each caller builds
  its own action set. A shared two-action emitter
  would force expansion of
  `schema_yaml11_bool_type_actions`'s current
  single-action behavior, which violates the
  "no behavior expansion" Non-Goal.
- **`yaml11_octal_actions` stands alone in Task 2.**
  Shares scaffold with the bool retrofits but the
  canonical-value transform (`0NNN` → `0oNNN`) and
  the rejection criterion (octal digit validation)
  are different enough that a shared helper would
  carry more branching than coherence gained.
- **`delete_unused_anchor` stands alone in Task 3.**
  Fundamentally different shape: operates on a node
  property (`anchor`), not a scalar value. Its edit
  span must include the anchor token (which lives
  outside the node's `loc` in most parser
  implementations), unlike the other retrofits
  where the edit span IS the node's `loc`.
- **No in-plan consolidation task.** Per-task TE
  consultation covers test-level consolidation via
  the updated agent protocol. File-level
  consolidation (module split, helper merge) is
  the dedicated post-program cleanup plan's
  scope, already queued.
- **No new formatter behavior.** Every emission
  these retrofits need —
  `ScalarStyle::DoubleQuoted` with escape
  handling, `ScalarStyle::Plain`,
  anchor-stripped nodes — is already supported by
  `format_subtree` and the formatter's existing
  style dispatch. Reuse unchanged.
- **Call-site update is a cumulative delta.** All
  three tasks touch `code_actions.rs:46-51`
  (`diag_actions` closure) to change the
  dispatched function signatures and pass
  `docs`/`text`. The commit for each task includes
  its own call-site update; the final commit
  leaves the dispatch block consistent with the
  new signatures for all three retrofits.
- **Audit allow-list shrinks from 108 to 104.**
  All four targets are `TodoRetrofit` entries in
  the current allow-list (verified by reading
  `rlsp-yaml/tests/parser_boundary_audit.rs`).
  Each retrofit removes its entry as part of the
  same commit — Task 1 removes two entries
  (108 → 106), Task 2 removes one (106 → 105),
  Task 3 removes one (105 → 104). The remaining
  104 entries cover validator retrofits (4),
  feature-level retrofits (13), private helpers
  flagged for audit v2, and other items queued
  for later plans.
