**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-18

## Goal

Retrofit the four text-scanning validator functions —
`validate_unused_anchors`, `validate_custom_tags`,
`validate_key_ordering`, and `validate_schema` — to source
all positional information from the parser AST's `Span`
fields instead of re-scanning raw text. After this plan
lands, validators operate purely on AST nodes for both
structure and position, advancing the "one parser, one
AST" program toward its final state (code-action retrofits
complete via the sibling plan; only feature-level
retrofits remain after this).

## Context

### Why these four and why now

These are the four `pub fn` validator entry points
currently allow-listed as `TodoRetrofit` markers in
`rlsp-yaml/tests/parser_boundary_audit.rs`
(lines 96-123). The code-action retrofit program is
concluding (sibling plan
`2026-04-18-retrofit-remaining-code-actions.md` in
flight); validators are the next layer of the same
architectural program. The AST-consumption discipline
that's now well-established in code actions extends
naturally to validators — the pattern differs (validators
emit diagnostics, code actions emit text edits) but the
replacement shape is the same: replace text scanning with
AST node + `Span` access.

### Current state per target

#### `validate_unused_anchors` — `rlsp-yaml/src/validation/validators.rs:29`

```rust
pub fn validate_unused_anchors(text: &str) -> Vec<Diagnostic>
```

The heaviest retrofit of the four. Current implementation:

- Takes only `text`, no `docs`. Manually rediscovers
  document boundaries by scanning for `---` markers.
- Splits `text` into lines and delegates to
  `scan_tokens(lines, start_line, end_line)` to find
  `&name` and `*name` tokens via character-boundary
  search. Returns a private `Token` struct with
  `name`, `line`, `col`, `is_anchor` fields.
- Per-document registry of anchor definitions vs alias
  references; emits diagnostics for anchors with zero
  references.

Private helpers retired by this retrofit:

- `scan_tokens` (line 125) — allow-listed at audit
  line 257, `HelperOf { root: "validate_unused_anchors" }`.
- `struct Token` (line 13) — not audit-listed (struct,
  not function); disappears with `scan_tokens`.

The AST already exposes every anchor via each node's
`anchor: Option<String>` and every alias via
`Node::Alias`. Document boundaries are already split
at parse time — the `Vec<Document<Span>>` slice is the
authoritative boundary source.

#### `validate_custom_tags` — `rlsp-yaml/src/validation/validators.rs:311`

```rust
pub fn validate_custom_tags<S: std::hash::BuildHasher>(
    text: &str,
    docs: &[Document<Span>],
    allowed_tags: &HashSet<String, S>,
) -> Vec<Diagnostic>
```

Lightest retrofit. Already walks AST for tag discovery
(via `collect_tag_diagnostics` recursive walker). The
text scanning is confined to position resolution:

- `find_tag_occurrence(lines, tag_str, occurrence)` at
  line 421 — scans raw text for the Nth occurrence of a
  tag string to construct the diagnostic `Range`.
  Deduplicates across repeated occurrences of the same
  tag string.
- `is_inside_quotes(line, pos)` at line 467 — helper of
  `find_tag_occurrence`, character-by-character
  quote-state tracking to exclude matches inside quoted
  strings.

The AST already carries each node's `tag` field and the
tag's position is captured in the node's `loc: Span`.
Replacing `find_tag_occurrence` with direct `Span` access
on the tag-carrying node removes the dedup-counter
complication entirely.

Private helpers retired by this retrofit:

- `find_tag_occurrence` — allow-listed at audit line
  264, `HelperOf { root: "validate_custom_tags" }`.
- `is_inside_quotes` (validators.rs variant) —
  allow-listed at audit line 271, `HelperOf { root:
  "validate_custom_tags" }`. Verified to have no other
  callers in `validators.rs` (grep confirms only
  `find_tag_occurrence` calls it; `collect_flow_style_diagnostics`
  and `flow_diagnostic` do not).

Note: `is_inside_quotes` appears independently in
`decorators/document_links.rs` with its own `HelperOf`
mapping to `find_document_links` — that instance is a
separate function and survives this plan untouched.

#### `validate_key_ordering` — `rlsp-yaml/src/validation/validators.rs:489`

```rust
pub fn validate_key_ordering(
    text: &str,
    docs: &[Document<Span>],
) -> Vec<Diagnostic>
```

Asymmetric hybrid. Already walks AST for ordering logic
(via `check_yaml_ordering`) but pre-scans raw text at
function entry to build a `HashMap<String, u32>` mapping
key name → first-occurrence line number. The AST walker
then looks up diagnostic positions in this pre-built
index. Mapping key nodes already carry `loc: Span`;
reading the span directly eliminates the pre-scan and
the `text` parameter entirely.

No private helpers retired — `check_yaml_ordering`
operates on AST nodes and is not audit-flagged. Only
the `text: &str` parameter disappears from the public
signature.

#### `validate_schema` — `rlsp-yaml/src/schema_validation.rs:215`

```rust
pub fn validate_schema(
    text: &str,
    docs: &[Document<Span>],
    schema: &JsonSchema,
    format_validation: bool,
    yaml_version: YamlVersion,
) -> Vec<Diagnostic>
```

Medium retrofit. Same asymmetric hybrid as
`validate_key_ordering` but with a larger helper tree.
Pre-scans `text` to build a `HashMap<String, Range>` via
`build_key_index` (line 250), passed through the shared
`Ctx` struct to every downstream validator helper
(~20 `validate_*` functions). The key index is consulted
whenever a diagnostic needs positioning.

The retrofit replaces `build_key_index` with direct
`Span` access from the mapping key nodes that the
validators already traverse. `Ctx` loses its `key_index`
field; all `validate_*` helpers continue to operate on
AST nodes unchanged.

Private helpers retired by this retrofit:

- `build_key_index` — allow-listed at audit line 278,
  `HelperOf { root: "validate_schema" }`.

The ~20 other `validate_*` helpers (`validate_mapping`,
`validate_sequence`, `validate_type`, etc.) are not
audit-listed — they already consume the AST — and are
not modified by this plan.

### AST-first retrofit scaffold per target

For each validator:

1. Remove the `text: &str` parameter (unless already
   absent; `validate_custom_tags` still takes it via its
   helpers but the signature loses it when helpers are
   rewritten).
2. For `validate_unused_anchors`: add `docs:
   &[Document<Span>]` to the signature; replace
   `scan_tokens` with an AST walk that collects anchors
   (from `anchor: Some(name)` on every node) and aliases
   (from `Node::Alias` nodes) per document; match by
   name; emit diagnostics for unreferenced anchors using
   the anchor-carrying node's `loc` directly.
3. For `validate_custom_tags`: replace
   `find_tag_occurrence` with a span-aware AST walker
   that yields `(tag_str, loc)` pairs directly from
   nodes carrying `tag: Some(...)`. Remove the
   `seen_counts` dedup logic (no longer needed — span
   identifies each occurrence uniquely).
4. For `validate_key_ordering`: remove the pre-scan
   block; in `check_yaml_ordering`, use the key node's
   `loc: Span` to construct diagnostic ranges directly.
5. For `validate_schema`: remove `build_key_index` and
   the `key_index` field of `Ctx`; pass mapping-key
   `Span`s through the existing recursive validator path
   (most helpers already have access to the relevant
   node; the few that don't get the span as a parameter).

### Call sites

All four validators are called from
`rlsp-yaml/src/language_server.rs` (LSP handlers).
Signature changes require updating those call sites; the
parse result (`docs: &[Document<Span>]`) is already
available in every handler context per Move 1. The
developer must locate and update each call site; no
other consumer exists.

### References

- Sibling plan (in flight):
  `.ai/plans/2026-04-18-retrofit-remaining-code-actions.md`
  — establishes that the allow-list post-state is 104
  entries when this plan begins.
- Prior retrofit plans (validator-adjacent patterns):
  - `.ai/plans/2026-04-18-parser-boundary-audit-v2.md`
    (commit `c70f642`) — broadened audit to catch
    private helpers and these four validator roots.
  - `.ai/plans/2026-04-18-retrofit-quoted-bool-to-unquoted-code-action.md`
    (commit `f1b4338`) — template for AST-walk + span
    access (for code actions, but the walk pattern
    transfers).
- AST node types and fields:
  `rlsp-yaml-parser/src/loader.rs` — `Node<S>` variants,
  `anchor: Option<String>`, `tag: Option<String>`,
  `loc: Span`, `Node::Alias`.
- Current allow-list:
  `rlsp-yaml/tests/parser_boundary_audit.rs`.
- Root CLAUDE.md "One parser, one AST" rule.

### Program-level consolidation note

Per the plan-reviewer's program-level consolidation check
and the test-engineer's scan-existing-tests protocol,
`validators.rs` and `schema_validation.rs` share test
patterns with sibling validator tests. This plan does NOT
include a file-level consolidation task — the dedicated
post-program cleanup plan queued in
`.ai/memory/project_followup_plans.md` owns file-splitting
and cross-module test dedup. Per-task TE consultation
during execution WILL produce Consolidation sections
(pruning duplicates, merging over-granular tests) per the
codified TE standard operating procedure. Cross-module
cleanup waits for the post-program plan.

## Non-Goals

- **Other validators** (`validate_flow_style`,
  `validate_duplicate_keys`, `validate_yaml11_compat`).
  These already operate purely on AST and are not
  allow-listed — no retrofit needed.
- **Feature-level retrofits** (`hover_at`,
  `complete_at`, `format_on_type`,
  `find_document_links`, `find_colors`,
  `folding_ranges`, `selection_ranges`,
  `semantic_tokens`, `document_symbols`,
  `goto_definition`, `find_references`,
  `prepare_rename`, `rename`). Queued for individual
  plans after validators land.
- **New validator rules.** Each retrofit preserves
  current diagnostic codes, severity, and coverage. No
  new lint rules added, no existing rules removed.
- **Changing `Ctx` ergonomics** beyond removing
  `key_index`. Other `Ctx` fields
  (`diagnostics`, `format_validation`, `yaml_version`)
  stay.
- **Refactoring the schema validator internals.** The
  ~20 `validate_*` helper functions consume the AST
  already and are unchanged by this plan.
- **Post-program cleanup** (test dedup, code
  simplification, module splitting). Dedicated follow-up
  plan queued.
- **Introducing new parser APIs.** The AST already
  exposes every field needed — anchors, aliases, tags,
  mapping keys, and spans are all on existing node
  types.

## Steps

- [ ] Retrofit `validate_unused_anchors` (standalone —
      heaviest, changes signature and eliminates
      `scan_tokens` + document-boundary rescan)
- [ ] Retrofit `validate_custom_tags` and
      `validate_key_ordering` (combined — both are
      position-retrofits on already-AST-walking
      validators with similar shape)
- [ ] Retrofit `validate_schema` (standalone — largest
      helper tree; touches `Ctx` struct)

## Tasks

### Task 1: Retrofit `validate_unused_anchors` to AST-first

Standalone because this validator currently takes only
`text`, has its own `scan_tokens` helper, and manually
rediscovers document boundaries — the retrofit touches
all three and warrants one focused task.

- [ ] Change signature to
      `fn validate_unused_anchors(docs:
      &[Document<Span>]) -> Vec<Diagnostic>`. The `text`
      parameter is removed entirely.
- [ ] Implement the new body as a per-document AST
      walker that collects:
  1. Anchor definitions — any node with
     `anchor: Some(name)`. Record `(name, loc)`.
  2. Alias references — `Node::Alias` nodes. Record
     `name`.
  3. Anchors with zero matching aliases become
     diagnostics using the anchor-carrying node's
     `loc: Span` for the range.
- [ ] Remove `fn scan_tokens` and `struct Token` from
      `validators.rs`. Search the crate for any other
      usage — if none, delete outright; if any survive
      (unlikely, but verify), document in Decisions why
      and keep them in the allow-list.
- [ ] Remove the two allow-list entries
      (`validate_unused_anchors` at line 98,
      `scan_tokens` at line 257).
- [ ] Update the single call site in
      `rlsp-yaml/src/language_server.rs` to pass `docs`
      instead of `text`.
- [ ] TE input-gate consultation. Scan existing tests
      for `validate_unused_anchors` coverage; produce a
      Consolidation section listing duplicates to
      retire and test-coverage shape to add.
- [ ] Regression tests (augment with TE's Consolidation
      decisions):
  - Single-document anchor with no alias emits
    diagnostic pointing at the anchor's span (not the
    full line)
  - Multi-document YAML (`---` separators) — anchors
    are scoped per document (anchor in doc 1 does not
    satisfy alias in doc 2)
  - Anchor on a flow collection (`&a [1, 2, 3]`)
    produces correct diagnostic span
  - Anchor used via multiple aliases in the same doc —
    no diagnostic
  - Alias without corresponding anchor definition —
    this validator's responsibility ends at "anchor
    unused"; undefined-alias diagnostics are a
    different validator's concern; confirm no
    regression in that direction
  - Trailing comment on the anchor's line — diagnostic
    range does not include the comment
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full workspace green
  - `cargo test --test corpus_invariants` with empty
    SKIP_LIST
  - `cargo test --test parser_boundary_audit` passes
    with allow-list at exactly 102 entries (baseline
    104 after sibling plan lands, minus two entries
    for `validate_unused_anchors` root and
    `scan_tokens` helper)
- [ ] TE output-gate sign-off covering regression adds
      + Consolidation deletes.

Acceptance: `validate_unused_anchors` consumes only the
AST; `scan_tokens` and `Token` deleted; single call site
updated; corpus SKIP_LIST stays empty; audit allow-list
at 102 (down from 104).

### Task 2: Retrofit `validate_custom_tags` and `validate_key_ordering` to AST-first

Bundled because both are position-retrofits on
already-AST-walking validators. Neither touches
signature parameters other than removing `text`; both
replace text-based position lookups with direct
`Span` access on nodes the walker already has in hand.

- [ ] Change signatures:
  - `fn validate_custom_tags<S: std::hash::BuildHasher>(docs:
    &[Document<Span>], allowed_tags: &HashSet<String,
    S>) -> Vec<Diagnostic>` — the `text` parameter
    removed.
  - `fn validate_key_ordering(docs:
    &[Document<Span>]) -> Vec<Diagnostic>` — the
    `text` parameter removed.
- [ ] For `validate_custom_tags`:
  - Update `collect_tag_diagnostics` to pass the
    tag-carrying node's `loc: Span` through the
    recursion instead of a `seen_counts` dedup
    accumulator.
  - Emit diagnostics with range = the tagged node's
    full `loc` (the node's span covers the tag plus
    the value it applies to; the parser does NOT
    track a separate tag-only span — verified
    against `rlsp-yaml-parser/src/loader.rs:415-434`
    where `loc: span` is assigned from the event's
    span covering the whole scalar). This is a
    deliberate range change from the current
    tag-only behavior — see Decisions. Any existing
    test that asserts the tag-only range must be
    updated as part of this task to assert the
    node-loc range.
  - Delete `find_tag_occurrence` and
    `is_inside_quotes` (validators.rs variant).
    Verify via `grep -n 'is_inside_quotes'
    /workspace/rlsp-yaml/src/validation/` that
    `find_tag_occurrence` was the sole caller (this
    was confirmed at plan time but re-verify after
    the function is removed).
- [ ] For `validate_key_ordering`:
  - Remove the pre-scan block that builds the key
    `HashMap<String, u32>`.
  - In `check_yaml_ordering`, use the mapping-key
    node's `loc: Span` to construct diagnostic
    ranges directly.
- [ ] Remove four allow-list entries:
  `validate_custom_tags` (line 105),
  `validate_key_ordering` (line 112),
  `find_tag_occurrence` (line 264),
  `is_inside_quotes` (validators.rs variant, line
  271).
- [ ] Update call sites in
      `rlsp-yaml/src/language_server.rs` for both
      validators.
- [ ] TE input-gate consultation for BOTH validators
      (one consult covering both; include
      Consolidation section).
- [ ] Regression tests for `validate_custom_tags`
      (augment with TE decisions):
  - Tag on a mapping value (`key: !custom value`) —
    diagnostic range equals the scalar node's `loc`
    (the tag plus the value it applies to)
  - Tag on a sequence item (`- !custom value`) —
    same (range equals the scalar node's `loc`)
  - Repeated tag strings in different positions —
    each emits its own diagnostic; no dedup
    collision
  - Tag string inside a quoted scalar
    (`note: "use !custom here"`) — NOT flagged
    (quoted strings are `Node::Scalar` with
    `tag: None`, so the AST walker naturally skips
    them; verify explicitly)
  - Tag allowed via `allowed_tags` set — no
    diagnostic
- [ ] Regression tests for `validate_key_ordering`
      (augment with TE decisions):
  - Out-of-order keys in a block mapping —
    diagnostic range covers the offending key's span
    (not the full line)
  - Out-of-order keys in a flow mapping —
    diagnostic covers the key span; flow syntax
    correctly parsed
  - Null-valued keys participate in ordering per
    existing behavior; verify no regression
  - Multi-document YAML — each document's keys
    checked independently
  - Nested mappings — ordering checked at each
    nesting level
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full workspace green
  - `cargo test --test corpus_invariants` with
    empty SKIP_LIST
  - `cargo test --test parser_boundary_audit`
    passes with allow-list at exactly 98 entries
    (down from 102 after Task 1 — removes four
    entries: `validate_custom_tags`,
    `validate_key_ordering`, `find_tag_occurrence`,
    `is_inside_quotes`)
- [ ] TE output-gate sign-off covering regression adds
      + Consolidation deletes for both validators.

Acceptance: both validators consume only the AST;
`find_tag_occurrence` and `is_inside_quotes`
(validators.rs) deleted; key_ordering's pre-scan block
removed; all four allow-list entries gone; audit at 98
(down from 102).

### Task 3: Retrofit `validate_schema` to AST-first

Standalone because it touches a different file
(`schema_validation.rs`), a shared `Ctx` struct, and the
large `validate_*` helper tree. Though most helpers are
unchanged, the `Ctx` modification and `build_key_index`
removal ripple through enough call sites that
combining with Task 2 would inflate that task's review
surface.

- [ ] Change signature to
      `fn validate_schema(docs: &[Document<Span>],
      schema: &JsonSchema, format_validation: bool,
      yaml_version: YamlVersion) -> Vec<Diagnostic>`.
      The `text` parameter is removed.
- [ ] Remove `build_key_index` (line 250) and the
      `key_index` field on the `Ctx` struct.
- [ ] Remove the `key_index`-based position helpers
      that become dead: `node_range` (line 1695),
      `mapping_range` (line 1703), `key_range`
      (line 1711), `find_key_range` (line 1717). Each
      call site of these helpers rewires to use the
      relevant node's `loc: Span` directly — the
      `validate_*` helpers already have the node in
      scope during recursion, so no new parameters
      are needed at most call sites. Preserve all
      diagnostic codes, messages, and severities.
      Update the `Ctx` struct definition at line 183
      and its construction at line 191 to drop the
      `key_index` field.
- [ ] Remove two allow-list entries:
      `validate_schema` (line 119),
      `build_key_index` (line 278).
- [ ] Update the call site in
      `rlsp-yaml/src/language_server.rs`.
- [ ] TE input-gate consultation. The
      `schema_validation.rs` test file is large;
      scan carefully and produce a Consolidation
      section.
- [ ] Regression tests (augment with TE decisions):
  - Type-mismatch diagnostic — range equals the
    offending scalar node's `loc`
  - Missing required property — range equals the
    parent mapping node's `loc` (replacing the
    current `mapping_range(path, ctx.key_index)`
    lookup at `schema_validation.rs:1259`; the
    parent mapping node is already in scope during
    recursion)
  - Additional property not allowed — range equals
    the offending key scalar node's `loc`
    (replacing the current `key_range(&key_str,
    path, ctx.key_index)` lookup at
    `schema_validation.rs:1296`)
  - Format-validation diagnostics (when
    `format_validation: true`) — range equals the
    scalar node's `loc` being validated
  - YAML 1.1 string warning path (via
    `emit_yaml11_string_warnings`) — same
    diagnostic codes, messages, and severities as
    before; range equals the scalar node's `loc`
  - Schema composition (`anyOf`, `oneOf`, `allOf`)
    error reporting — range equals the offending
    node's `loc` (preserving which node the
    composition error attaches to)
  - Deeply nested violations — range equals the
    deepest offending node's `loc`; diagnostic
    codes and messages preserved from pre-retrofit
    behavior
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full workspace green
  - `cargo test --test corpus_invariants` with
    empty SKIP_LIST
  - `cargo test --test parser_boundary_audit`
    passes with allow-list at exactly 96 entries
    (down from 98 after Task 2 — removes two
    entries: `validate_schema`, `build_key_index`)
- [ ] TE output-gate sign-off covering regression
      adds + Consolidation deletes.

Acceptance: `validate_schema` consumes only the AST;
`build_key_index` deleted; `Ctx.key_index` field
removed; all diagnostic codes, messages, severities
preserved; audit at 96 (down from 98).

## Decisions

- **Bundling three tasks into one plan.** Same
  rationale as the code-action retrofit plan — the
  AST-consumption pattern is stable; per-plan overhead
  is not earning its cost for small retrofits. Task 1
  is standalone (heaviest); Task 2 bundles two small
  retrofits with shared shape; Task 3 is standalone
  (different file, larger helper tree).
- **Task 2 bundles `validate_custom_tags` and
  `validate_key_ordering`.** Both are position-retrofits
  on validators that already walk the AST for structural
  logic. The bundling captures the "use node span
  instead of text lookup" pattern once.
- **Task 1 is standalone.** `validate_unused_anchors`
  differs: it takes only `text`, manually rediscovers
  document boundaries, and retires a distinct helper
  family (`scan_tokens` + `Token` struct). Combining
  with Task 2 would mix "add `docs` parameter" with
  "remove text pre-scan" in one task, making the diff
  harder to review.
- **Task 3 is standalone.** `validate_schema` lives in
  a different file and touches the shared `Ctx` struct
  used by ~20 helpers. Review surface is best bounded
  to its own task.
- **Behavior preserved: codes, messages, severities,
  coverage.** Each validator's diagnostic codes,
  messages, severities, and which conditions trigger
  a diagnostic are preserved.
- **Behavior change: `validate_custom_tags` range
  widens from tag-string-only to full-node.** Current
  behavior ranges a custom-tag diagnostic at the
  `!custom` token only (via `find_tag_occurrence`'s
  text scan). The retrofit emits the range at the
  tagged node's full `loc` (covering the tag plus the
  value) because the parser does not track a separate
  tag-only span — verified at
  `rlsp-yaml-parser/src/loader.rs:415-434`. The
  user-visible effect is a wider diagnostic underline.
  Alternatives considered: (a) compute a tag-only
  range from node-loc start + tag string length —
  rejected as byte arithmetic that re-introduces the
  text-pattern of the current implementation;
  (b) add a parser-side `tag_span` field — out of
  scope for this plan (would be its own parser
  plan). Any test asserting the tag-only range is
  updated as part of Task 2.
- **Audit allow-list shrinks from 104 to 96.** Baseline
  104 is the sibling plan's expected post-state (after
  the four code-action retrofits land). Task 1 removes
  2 entries (104 → 102), Task 2 removes 4 entries
  (102 → 98), Task 3 removes 2 entries (98 → 96).
- **`is_inside_quotes` in validators.rs can be fully
  deleted.** Verified via grep: its only caller is
  `find_tag_occurrence` (line 431). Once
  `find_tag_occurrence` is removed, `is_inside_quotes`
  has zero callers in the crate and the allow-list
  entry can go. The `is_inside_quotes` in
  `decorators/document_links.rs` is a separate function
  (different file, different allow-list entry) and is
  not touched by this plan.
- **Call-site updates live in
  `rlsp-yaml/src/language_server.rs`.** All four
  validators are invoked from LSP handlers there; each
  task's call-site update is localized to its own
  validator's invocation.
