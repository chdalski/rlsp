**Repository:** root
**Status:** InProgress
**Created:** 2026-04-18

## Goal

Retrofit the `string_to_block_scalar` code action in
`rlsp-yaml/src/editing/code_actions.rs` from its current
text-surgery implementation to an AST-first approach using
the existing `format_subtree` API. The parser provides the
exact scalar value, style, and span; the formatter knows
how to emit block-scalar form with correct indentation and
escape handling. This preemptively eliminates a class of
data-loss and invalid-output bugs the current
text-parse-then-splice approach is vulnerable to: naïve
escape handling, heuristic key detection, full-line
replacement that destroys trailing comments, and inability
to distinguish already-block scalars from plain/quoted
ones.

## Context

### Why this is queued now

This plan is a direct follow-up to the just-completed
`block_to_flow` retrofit
(`.ai/plans/2026-04-18-retrofit-block-to-flow-code-action.md`,
Completed 2026-04-18). The `block_to_flow` rewrite
established the AST+formatter pattern for structural
code-action transforms. `string_to_block_scalar` is the
next structural-text-surgery code action in the queue
per the user-preferred "exclude the possibility of bugs
preemptively" discipline established during the
flow-to-block fix wrap-up.

The Move 0 corpus harness (I4 scalar preservation)
passes on the seed corpus for `string_to_block_scalar`
— either the action doesn't fire (no mapping value in
the seed corpus exceeds 40 chars) or it fires without
dropping scalars. "Not destructive on current corpus"
does not mean "free of the bug class" — the defects
documented below exist for inputs the corpus does not
yet contain.

### Current implementation defects

`string_to_block_scalar` at `code_actions.rs:676-741`
takes a single line (`line: &str`), a line index, and a
URI. Cursor-driven, not diagnostic-driven. Traced
defects:

1. **Heuristic key detection.** Line 682 uses
   `line.find(':')?` to locate the key/value
   separator. This is the same class of bug
   `block_to_flow` had before retrofit: it matches
   the FIRST colon on the line. Breaks on:
   - Quoted keys with embedded colons (`"foo:bar":
     long value`) — the action cuts at `"foo`
   - Values containing URLs or timestamps
     (`homepage: http://example.com/very-long-path`)
     — the action cuts at the first colon, not the
     key/value separator
2. **Naïve escape handling.** Line 715 does
   `value.replace("\\n", &format!("\n{indent_str}"))`.
   Only handles the literal two-char sequence `\n` in
   double-quoted strings. Doesn't handle `\t`, `\\`,
   `\"`, `\/`, `\b`, `\f`, `\r`, `\uXXXX`, `\xXX`,
   or any of the other YAML 1.2 double-quoted escape
   sequences (§5.7). Single-quoted strings have
   their own escape convention (`''` for literal
   `'`) that isn't handled either. A string like
   `"line one\nline\ttwo"` would convert to
   `| line one\nline<literal-tab>two` — mangled.
3. **Full-line replacement.** Lines 726-729 build
   `edit_range` covering cols 0 → `line.len()`.
   Anything trailing the value gets destroyed —
   comments (`key: "..." # note`), whitespace,
   tab-aligned continuation markers.
4. **No detection of already-block scalars.** The
   heuristics at lines 688-694 distinguish double-
   quoted, single-quoted, and plain, but not block
   (`|` / `>`). A block scalar whose first line
   happens to contain `:` could be mis-interpreted
   and re-converted. The AST would report the
   actual `ScalarStyle` directly.
5. **Threshold is on text chars, not parsed value.**
   Line 696 checks `value.len() < min_length`. The
   "value" here is the slice after the colon,
   including any quote-escape noise or trailing
   whitespace — not the actual parsed value the
   user sees. With the AST, the threshold applies
   to the parsed `Scalar.value`, which matches the
   user's mental model.
6. **Exclusion heuristics on value start.** Lines
   701-707 reject values starting with `{`, `[`,
   `&`, `*` — approximations for "this looks like
   a flow collection, anchor, or alias, don't
   touch." With the AST, the node type is known
   exactly: offer conversion only for
   `Node::Scalar` with plain/quoted style.

Defects (1), (2), (3) are data-loss or invalid-output
risks. (4), (5), (6) are detection-accuracy issues
that could cause the action to fire when it shouldn't
(or skip when it should).

### Why the AST+formatter approach eliminates the bug class

- The parser already identified the scalar — its
  value, style, span, anchor, tag — in the AST. No
  text re-parsing needed.
- `format_subtree` (added 2026-04-18 under
  `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`,
  commit `8dfe0e0`) takes a `Node<Span>` and emits it
  in its current style. For a block scalar, the
  formatter's existing `repr_block_to_doc`
  (`formatter.rs:919`) handles block-scalar
  indentation headers, chomping indicators, and
  content emission correctly.
- The code action becomes: find the target
  `Node::Scalar` at the cursor line, verify it
  qualifies (style ∈ {Plain, DoubleQuoted,
  SingleQuoted}, parsed value length ≥ threshold,
  parent is a mapping entry — i.e., the scalar is
  a mapping value), clone with `style:
  ScalarStyle::Literal`, call `format_subtree`,
  emit a `TextEdit` covering the scalar's span.

### Cursor-based trigger

Like `block_to_flow`, `string_to_block_scalar` is
offered from the cursor-line context actions at
`code_actions.rs:65`, not from a diagnostic. The AST
retrofit locates the target scalar by walking the AST
for a `Node::Scalar` whose span starts on the cursor
line AND is the value of a mapping entry (not a key,
not a sequence item — matching current behavior).

### Preserved behavior

- The 40-char length threshold is preserved as a
  UX decision (not a correctness concern). The
  value measured is the parsed scalar value.
- Only mapping values are eligible (not keys, not
  sequence items). Matches current narrow dispatch.
- Only plain, double-quoted, and single-quoted
  scalars are converted. Already-block scalars
  (literal `|` or folded `>`) and flow collections
  are excluded by the AST's style field — no
  heuristic needed.
- Target style: `ScalarStyle::Literal` (the
  vertical-bar block form). The current
  implementation targets `|`; preserve that. Folded
  (`>`) scalars are a user-configurable preference
  that could be a future enhancement but is out of
  scope here.

### Non-Goals

- **Folded block scalar (`>`) as the target** —
  current implementation uses literal (`|`) only;
  preserve that. Offering folded as an alternative
  output form is a future enhancement plan.
- **Sequence items** — current implementation only
  works on `key: value` pairs (mapping values).
  Preserve that narrow dispatch. AST version
  naturally filters via parent-node check.
- **Nested mapping values** — if the target
  scalar is a value inside a deeply nested
  mapping, it's still a scalar and still
  convertible. Not a non-goal; this "just works"
  under the AST walk since scalars have no
  children to worry about.
- **Other code actions** — `tab_to_spaces`,
  `quoted_bool_to_unquoted`, `yaml11_*`,
  `delete_unused_anchor`. Each is a separate
  future plan.
- **Move 2's fixture pattern** — plan uses
  existing test infrastructure (unit tests + Move
  0 corpus invariants).
- **Introducing new formatter behavior** —
  `format_subtree` already handles block-scalar
  emission via `repr_block_to_doc`. This plan
  reuses existing infrastructure; no new emission
  logic.

### References

- Prior code-action retrofit plans establishing
  the AST+format_subtree pattern:
  - `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`
    (Completed 2026-04-18 — added `format_subtree`
    and retrofitted `flow_map_to_block` /
    `flow_seq_to_block`)
  - `.ai/plans/2026-04-18-retrofit-block-to-flow-code-action.md`
    (Completed 2026-04-18 — retrofitted
    `block_to_flow`; established the
    cursor-based-dispatch AST-walk pattern)
- `format_subtree` public API:
  `rlsp-yaml/src/editing/formatter.rs` (added in
  commit `8dfe0e0`)
- Formatter's block-scalar emission:
  `rlsp-yaml/src/editing/formatter.rs:919`
  (`repr_block_to_doc`)
- Current `string_to_block_scalar`:
  `rlsp-yaml/src/editing/code_actions.rs:676-741`
- Current cursor dispatch:
  `rlsp-yaml/src/editing/code_actions.rs:65`
- YAML 1.2 §5.7 (double-quoted escape sequences):
  https://yaml.org/spec/1.2.2/#57-escaped-characters
- YAML 1.2 §8.1 (block scalar styles):
  https://yaml.org/spec/1.2.2/#81-block-scalar-styles
- `rlsp-yaml-parser::event::ScalarStyle`:
  `rlsp-yaml-parser/src/event.rs` (enum variants
  Plain, SingleQuoted, DoubleQuoted, Literal,
  Folded)
- Root CLAUDE.md "One parser, one AST" rule.

## Steps

- [ ] Rewrite `string_to_block_scalar` as AST +
      `format_subtree` with cursor-based scalar-node
      matching
- [ ] Cleanup — retire any helpers that become
      unused, add regression tests for the defect
      classes, update `feature-log.md`

## Tasks

### Task 1: Rewrite `string_to_block_scalar` via AST + `format_subtree`

Replace the text-parsing implementation with an
AST-first approach. Cursor-based dispatch; preserves
the current narrow behavior (mapping values only,
plain/quoted styles only, 40-char length threshold,
literal `|` target style).

- [ ] Change `string_to_block_scalar` signature to
      `fn string_to_block_scalar(docs:
      &[Document<Span>], text: &str, line_idx:
      usize, uri: &Url) -> Option<CodeAction>`. The
      call site at `code_actions.rs:65` already
      passes `docs` and `text` in scope.
- [ ] Walk `docs` for a mapping entry `(key, value)`
      where:
  - `value` is a `Node::Scalar`
  - `value`'s span starts on line `line_idx + 1`
    (LSP line 0-based, parser line 1-based — cross-
    check convention against `block_to_flow`'s
    `find_innermost_block_in_node` and
    `node_loc(k).start.line` usage at
    `code_actions.rs:501`)
  - `value.style` is `ScalarStyle::Plain`,
    `ScalarStyle::DoubleQuoted`, or
    `ScalarStyle::SingleQuoted`
  - `value.value.chars().count() >= 40` (use
    char count, not byte length, so multi-byte
    characters don't inflate the threshold)
- [ ] If no qualifying scalar is found, return
      `None`. This replaces the current heuristic
      early returns (lines 696, 701-707).
- [ ] Clone the scalar node with
      `style: ScalarStyle::Literal`. Anchor, tag,
      and other fields preserved unchanged.
- [ ] Compute `base_indent` as the mapping key's
      column plus 2 (`key_loc.start.column + 2`,
      using the parser's 0-based column convention).
      This matches the pattern established in
      `block_to_flow` Task 1 (commit `173f838`) and
      produces correctly-indented continuation
      lines for the literal block scalar: the
      key lives at column K, the block scalar
      header `|` lands after `key: ` on the same
      line, and the scalar's continuation content
      is indented to K+2.
- [ ] Call `format_subtree(&cloned,
      &YamlFormatOptions::default(), base_indent)`.
- [ ] Emit a `TextEdit` whose range is the scalar
      node's `loc` span (NOT the full line). The
      edit starts at the scalar's starting position
      and ends at its end position. Trailing
      comments and whitespace after the scalar on
      the same line are preserved.
- [ ] Preserve the title `"Convert to block
      scalar"`.
- [ ] Update call site at `code_actions.rs:65` to
      pass `docs` and `text`.
- [ ] Update any existing unit tests that invoke
      `string_to_block_scalar` directly (grep for
      the function name under `#[cfg(test)]` to
      enumerate). Preserve test intents.
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full workspace passes
  - `cargo test --test corpus_invariants` passes
    with empty SKIP_LIST (no regression; current
    behavior is clean — must stay that way)
  - `cargo test --test parser_boundary_audit`
    passes (4 allow-list entries unchanged;
    `string_to_block_scalar` is private, no audit
    interaction)
  - Surprise Failure Protocol: any unexpected
    corpus-harness failure → developer messages
    lead, no skip-list entries added without
    direction.

Acceptance: `string_to_block_scalar` consumes the
AST; finds the target scalar by cursor line +
mapping-value parent + qualifying style + length
threshold; produces correct literal-block-scalar
output with proper escape handling, indentation,
and no collateral edits to surrounding content;
existing unit-test intents preserved; corpus
SKIP_LIST stays empty; audit allow-list unchanged.

### Task 2: Cleanup — regression tests, docs

Finalize the change: add regression tests targeting
the specific defect classes the old text-surgery
implementation failed on, and update user-facing
docs.

- [ ] Grep `code_actions.rs` for remaining callers
      of any helper that was used solely by the
      old `string_to_block_scalar`. The current
      implementation inlines all logic; there's
      likely no helper to retire. If any is
      discovered, apply the same conditional-delete
      protocol used in the block_to_flow cleanup
      (Task 2 of
      `.ai/plans/2026-04-18-retrofit-block-to-flow-code-action.md`).
- [ ] Verify `parser_boundary_audit` allow-list
      remains at exactly 4 entries. No code-action
      signature changes this task; this is a
      confirmation check.
- [ ] Add NEW regression unit tests covering the
      defect classes from Context:
  - **Embedded escape sequences** — a
    double-quoted value containing `\n`, `\t`,
    `\\`, `\"`, and `\uXXXX`. After conversion,
    the block scalar must contain the resolved
    characters (actual newline, tab, backslash,
    quote, unicode character), not the literal
    two-character escape sequences.
  - **Single-quoted escapes** — a single-quoted
    value containing `''` (the single-quote
    escape). After conversion, the block scalar
    must contain a literal `'`.
  - **URL-style value** — a mapping value like
    `homepage: http://example.com/very-long-path-that-exceeds-40-chars`.
    Key detection must not be fooled by the `:`
    inside the URL; conversion must preserve the
    full URL.
  - **Trailing comment** — a mapping entry with a
    trailing comment on the same line
    (`key: "long value"  # note`). The edit must
    preserve the comment. The scalar's `loc` span
    covers the scalar content only, so the edit
    (ranged over `loc`) does not touch the
    trailing whitespace or comment. If during
    implementation the parser's `loc` is observed
    to include trailing whitespace or extend past
    the scalar content, that is a blocker — the
    developer messages the lead before proceeding.
  - **Quoted key with embedded colon** — a
    mapping entry where the key is `"foo:bar":
    long value`. The new implementation must
    target the value, not be fooled by the colon
    inside the quoted key.
  - **Already-block scalar** — a mapping value
    whose scalar style is already
    `Literal`/`Folded`. The code action must not
    be offered (return `None`).
  - **Short value** — a mapping value under the
    40-char threshold. The action must not be
    offered.
  - **Sequence item scalar** — a sequence item
    like `- "long string"`. The action must not
    be offered (not a mapping value). Preserves
    current narrow behavior.
  - **Flow collection start** — a mapping value
    like `key: [long, list, of, items]`. The
    value is a `Node::Sequence`, not a scalar.
    The action must not be offered.
  - **Anchor-only exclusion is replaced by AST
    checks** — a mapping value with an anchor
    (e.g., `key: &anchor "long string"`). The
    scalar's `anchor` field is preserved under
    `format_subtree`; conversion emits the
    anchor correctly. Verify this is the case
    (and add the test), since the old code's
    `starts_with('&')` heuristic refused such
    values.
- [ ] Update `rlsp-yaml/docs/feature-log.md` with
      a new entry recording the AST-based
      `string_to_block_scalar` rewrite. Match the
      shape of the `block_to_flow` entry.
- [ ] Build/test gates (same as Task 1).

Acceptance: regression tests cover all listed
defect classes; audit allow-list unchanged at 4
entries; workspace suite green;
`feature-log.md` records the change.

## Decisions

- **Approach: AST + `format_subtree`.** Mirrors
  the flow-to-block and block-to-flow retrofits.
  Rejected "tighten text-parsing in place" —
  eliminates the bug class (not just specific
  bugs) and aligns with the "one parser, one
  AST" rule.
- **Target style: `ScalarStyle::Literal` (`|`)
  only.** Preserves current implementation's
  choice. Folded (`>`) as an alternative is a
  future enhancement.
- **40-char length threshold preserved.** UX
  decision, not a correctness concern. Measured
  against the parsed scalar value in chars
  (`chars().count()`), not bytes, so multi-byte
  characters behave as users expect.
- **Mapping-value-only dispatch preserved.** AST
  walk filters by parent type; sequence items
  and other positions are excluded naturally.
- **Anchor preservation is automatic.** Anchors
  live on the AST node; `format_subtree` emits
  them. The old code's `starts_with('&')`
  heuristic exclusion is dropped.
- **Trailing-comment preservation.** The edit
  range covers only the scalar's `loc` span, not
  the full line. Trailing content on the same
  line (comments, whitespace) is NOT replaced by
  the edit. If during implementation the
  scalar's `loc` is observed to extend past the
  scalar content (parser convention dependent),
  that is a blocker — the developer messages the
  lead before proceeding, not a silent fallback.
- **No formatter changes.** `format_subtree`
  already handles `ScalarStyle::Literal`
  emission via `repr_block_to_doc` (already in
  use for whole-document formatting). This
  plan reuses existing infrastructure
  unchanged.
- **Corpus harness as acceptance gate.**
  `corpus_invariants` I4 currently passes
  clean for this action; must stay clean.
  Surprise Failure Protocol applies.
