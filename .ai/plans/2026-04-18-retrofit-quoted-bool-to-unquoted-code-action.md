**Repository:** root
**Status:** Completed (2026-04-18)
**Created:** 2026-04-18

## Goal

Retrofit the `quoted_bool_to_unquoted` code action in
`rlsp-yaml/src/editing/code_actions.rs` from text surgery
to an AST + `format_subtree` approach. The current
implementation scans the cursor's line for the literal
byte sequences `"true"`, `"false"`, `'true'`, `'false'`
and replaces the full line when one is found near the
cursor — a pattern vulnerable to matching inside other
quoted strings, replacing trailing comments, and firing
on non-scalar contexts. The AST form locates the
quoted-bool scalar by span containment, clones it with
`style: ScalarStyle::Plain`, and re-emits via
`format_subtree` — eliminating the bug class the way the
completed `flow_map_to_block`, `block_to_flow`, and
`string_to_block_scalar` retrofits did. Also shrinks the
`parser_boundary_audit` allow-list by one entry
(`editing/code_actions.rs::quoted_bool_to_unquoted`,
currently `TodoRetrofit`).

## Context

### Current implementation defects

`quoted_bool_to_unquoted` at
`rlsp-yaml/src/editing/code_actions.rs:630-672` takes
`line: &str`, `line_idx: usize`, `range: Range`, `uri:
&Url`. Cursor-driven dispatch from
`code_actions.rs:64`. Traced defects:

1. **Literal-byte pattern scan.** Line 639 iterates
   over `["\"true\"", "\"false\"", "'true'", "'false'"]`
   and uses `line.find(pattern)` to locate the FIRST
   byte-sequence match on the line. Consequences:
   - A double-quoted scalar like
     `msg: "status reported \"true\" today"` contains the
     literal byte sequence `"true"` (the escaped quotes
     in source become unescaped bytes at certain points
     of analysis — but even without escape interpretation,
     a line like `msg: "a '"true"' message"` contains the
     byte sequence `"true"`). The pattern scan matches
     inside the outer quoted scalar and offers to
     "convert" the inner substring.
   - A flow mapping like `{x: "true", y: 1}` is caught,
     but the full-line replacement (next defect) then
     destroys the rest of the mapping.
2. **Full-line replacement.** Lines 653-656 build
   `edit_range` covering `col 0 → line.len()`. Anything
   before the quoted bool (e.g. a key with whitespace
   alignment), anything after (trailing comment, another
   entry in a flow collection), and indentation are all
   replaced. The replacement text is the line with just
   the 6-char pattern rewritten; surrounding content is
   preserved by luck (because the new_text is built from
   `before` + `unquoted` + `after`). Still, any edit
   over the full line's range is wrong on principle — if
   an LSP client re-parses after applying, the edit
   covers positions it should not.
3. **Cursor-proximity check is too loose.** Line 643's
   `col <= pattern_end` accepts the cursor anywhere from
   column 0 through the end of the pattern. Two quoted
   bools on the same line (`key1: "true", key2: "false"`)
   will always offer the first one regardless of which
   the cursor is on.
4. **No scalar-context verification.** The pattern scan
   does not confirm that the `"true"` / `"false"` is
   actually a YAML scalar value, not a substring of a
   longer scalar. A double-quoted string whose decoded
   value is `status "true" reported` source-encodes as
   `"status \"true\" reported"` — the literal bytes
   `"true"` do not appear there (backslash-quote is the
   escape). But a single-quoted string whose decoded
   value contains the literal 6-char sequence `'true'`
   source-encodes as
   `'status ''true'' reported'` — once the string opens
   with `'`, `''` is the escape, so the decoded value
   contains `'true'`. Depending on exact source
   encoding, false positives are possible.
5. **No check that Plain form is safe.** The current
   code naively drops the quotes. For
   `{true,false,True,False,TRUE,FALSE,yes,no,on,off,...}`
   — plain YAML 1.2 bool — unquoted is fine. But if a
   user quoted a bool with LEADING or TRAILING
   whitespace that the quote preserved (`" true"` →
   decoded ` true`), the AST would report
   `value = " true"` which is NOT a YAML 1.2 bool in
   plain form. The current code doesn't see the decoded
   value so it cannot check.

Defects (1), (2), (4) are correctness issues. (3) is a
UX issue. (5) is a safety issue the AST form exposes.

### Why the AST+formatter approach eliminates the bug class

- The parser identifies the scalar — its exact value,
  style, span. No text re-parsing.
- The code action becomes: walk the AST to find a
  `Node::Scalar` whose `loc` span contains the cursor
  position AND whose `style` is `SingleQuoted` or
  `DoubleQuoted` AND whose decoded `value` is exactly
  `"true"` or `"false"` (the YAML 1.2 bool tokens).
- Clone the scalar with `style: ScalarStyle::Plain`.
  `format_subtree` emits it as plain. The edit range is
  the scalar's `loc` span — no collateral replacement.
- Trailing comments, preceding content on the same
  line, surrounding flow-collection entries are all
  preserved by construction (the edit does not touch
  them).

### Cursor-based trigger preserved

Like `block_to_flow` and `string_to_block_scalar`,
`quoted_bool_to_unquoted` is offered from the
cursor-line context-actions at `code_actions.rs:64`
rather than from a diagnostic. The AST retrofit locates
the target by walking the AST for a `Node::Scalar`
whose `loc.start.line == line_idx + 1` AND whose `loc`
column range contains the cursor column — matching the
user's intent that the cursor be "on" the quoted bool.

### Preserved behavior

- Title format `"Convert quoted string to {unquoted}"`
  where `{unquoted}` is literal `true` or `false`.
  Preserve that — the title communicates the specific
  conversion to the user.
- Only exact matches: decoded value must be exactly
  `"true"` or `"false"` (case-sensitive, no whitespace,
  no other bool-like tokens). YAML 1.1 bool variants
  (`yes`/`no`/`on`/`off`/`True`/`False`/`TRUE`/`FALSE`)
  are out of scope — they're handled by the separate
  `yaml11_bool_actions` retrofit (also queued).
- `CodeActionKind::QUICKFIX` preserved.
- Mapping-value-only dispatch is NOT a current
  constraint — the current text scanner fires on any
  line containing the pattern. The AST form naturally
  finds scalar nodes anywhere (mapping values, sequence
  items, even top-level document scalars). Preserve
  this broader-than-string_to_block_scalar scope since
  it matches current UX.

### References

- Prior code-action retrofit plans establishing
  the AST+`format_subtree` pattern:
  - `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`
    (Completed — added `format_subtree`, retrofitted
    `flow_map_to_block` / `flow_seq_to_block`)
  - `.ai/plans/2026-04-18-retrofit-block-to-flow-code-action.md`
    (Completed — retrofitted `block_to_flow`)
  - `.ai/plans/2026-04-18-retrofit-string-to-block-scalar-code-action.md`
    (Completed — retrofitted `string_to_block_scalar`;
    `base_indent = key_col` is the correct formula for
    block-scalar emission; for plain-scalar emission in
    this plan, the base indent concept is simpler —
    see Task 1)
- `format_subtree` public API:
  `rlsp-yaml/src/editing/formatter.rs` (added under
  commit `8dfe0e0`)
- Current `quoted_bool_to_unquoted`:
  `rlsp-yaml/src/editing/code_actions.rs:630-672`
- Current cursor dispatch:
  `rlsp-yaml/src/editing/code_actions.rs:64`
- `parser_boundary_audit` allow-list entry to remove:
  `rlsp-yaml/tests/parser_boundary_audit.rs` —
  `AllowEntry { file: "editing/code_actions.rs", func:
  "quoted_bool_to_unquoted", marker: TodoRetrofit {
  plan: "..." } }`
- YAML 1.2 Core Schema tag resolution (bool tokens
  `true` / `false`):
  https://yaml.org/spec/1.2.2/#tag-resolution
- `rlsp-yaml-parser::event::ScalarStyle`:
  `rlsp-yaml-parser/src/event.rs` (Plain, SingleQuoted,
  DoubleQuoted, Literal, Folded)
- Root CLAUDE.md "One parser, one AST" rule.

## Steps

- [x] Rewrite `quoted_bool_to_unquoted` as AST +
      `format_subtree` with cursor-based scalar-node
      matching; shrink audit allow-list by one entry
- [x] Cleanup — add regression tests for the defect
      classes, verify audit allow-list count, remove
      the retrofit bullet from `project_followup_plans.md`

## Tasks

### Task 1: Rewrite `quoted_bool_to_unquoted` via AST + `format_subtree` and shrink audit allow-list

Replace the text-parsing implementation with an AST-first
approach. Cursor-based dispatch; preserves the current
behavior envelope (quoted `true`/`false` only, plain-form
target, `QUICKFIX` kind). Remove the allow-list entry for
this function in the same commit.

- [x] Change `quoted_bool_to_unquoted` signature to
      `fn quoted_bool_to_unquoted(docs: &[Document<Span>],
      line_idx: usize, col: usize, uri: &Url) ->
      Option<CodeAction>`. The call site at
      `code_actions.rs:64` already has `docs` in scope
      (from the outer function's parameters) and can
      derive `col` from `range.start.character as usize`.
- [x] Walk `docs` for a `Node::Scalar` where:
  - `scalar.loc.start.line == line_idx + 1`
    (LSP 0-based → parser 1-based; cross-reference the
    same convention used in `string_to_block_scalar`
    at `code_actions.rs:682`)
  - `scalar.loc.start.column <= col <= scalar.loc.end.column`
    (cursor must be inside the scalar's column span on
    the cursor line; this replaces the loose
    "col <= pattern_end" heuristic)
  - `scalar.style` is `ScalarStyle::SingleQuoted` or
    `ScalarStyle::DoubleQuoted`
  - `scalar.value` is exactly `"true"` or `"false"`
    (case-sensitive, no leading/trailing whitespace —
    the decoded value from the parser is authoritative)
- [x] If no qualifying scalar is found, return `None`.
- [x] Clone the scalar node with
      `style: ScalarStyle::Plain`. Anchor, tag, and
      `value` preserved unchanged.
- [x] Compute `base_indent` as the scalar's start column
      (`scalar.loc.start.column`). Plain scalars emit
      inline — no continuation lines, no indentation
      semantics to worry about. `format_subtree` handles
      emission.
- [x] Call `format_subtree(&cloned_scalar,
      &YamlFormatOptions::default(), base_indent)`.
- [x] Emit a `TextEdit` whose range is the scalar node's
      `loc` span (NOT the full line). Start at
      `(loc.start.line - 1, loc.start.column)`, end at
      `(loc.end.line - 1, loc.end.column)`.
- [x] Preserve the title
      `"Convert quoted string to {unquoted}"` where
      `{unquoted}` is the literal `"true"` or `"false"`
      matching the decoded value.
- [x] Update the call site at `code_actions.rs:64` to
      pass `docs, line_idx, col, uri` instead of the
      current `line, line_idx, range, uri`.
- [x] Update any existing unit tests that invoke
      `quoted_bool_to_unquoted` directly (grep for the
      function name under `#[cfg(test)]`). Preserve test
      intents; adjust signatures to match the new
      parameter list.
- [x] Remove the `quoted_bool_to_unquoted` entry from
      `ALLOW_LIST` in `rlsp-yaml/tests/parser_boundary_audit.rs`.
      The audit test will fail if the function still
      matches the detection regex after retrofit —
      which it should not (the new signature takes
      `docs: &[Document<Span>]` first, then `line_idx:
      usize`, so the first-param anchor excludes it).
- [x] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full workspace passes
  - `cargo test --test corpus_invariants` passes
    (no regression on seed corpus)
  - `cargo test --test parser_boundary_audit` passes
    with exactly **100** allow-list entries (was 101;
    one entry removed for this retrofit).
- [x] Per-entry audit verification: temp-remove a
      different (unrelated) allow-list entry, confirm
      the audit fails citing THAT entry, restore.
      Protects against accidental regex breakage.

Acceptance: `quoted_bool_to_unquoted` consumes the AST;
finds the target scalar by cursor containment + qualifying
style + exact-value match; produces correct plain-style
output with the edit ranged over the scalar's `loc` span;
surrounding content (preceding indentation, trailing
comments, neighboring flow-collection entries) untouched;
corpus SKIP_LIST empty; audit allow-list at exactly 100
entries (the removed entry was `editing/code_actions.rs::quoted_bool_to_unquoted`
with `TodoRetrofit` marker); full workspace suite green.

**Completed:** commit `5fb6482` — AST-first rewrite
landed. `base_indent = scalar.loc.start.column` per
the plan (plain scalars emit inline; no block-style
indentation semantics). Allow-list shrunk from 101 to
100 entries. 22 tests cover style/value matching,
cursor containment, multi-doc walks, flow-context
preservation, unicode escapes, case variants refused,
QUICKFIX kind, title text, literal-block refused,
empty-docs safety.

### Task 2: Cleanup — regression tests, docs, audit verification

Finalize the change: add regression tests targeting the
defect classes the old text-surgery implementation
failed on, and update user-facing docs.

- [x] Add NEW regression unit tests covering the defect
      classes from Context:
  - **Pattern-inside-longer-scalar** — a double-quoted
    value whose decoded form contains the literal 6-char
    sequence `'true'` (e.g. source
    `msg: 'status ''true'' reported'`). The code action
    must NOT be offered on this line when the cursor is
    inside the outer single-quoted scalar, because the
    scalar is the whole `status 'true' reported` string,
    not a standalone `'true'` bool.
  - **Multiple bools same line** — source
    `{ a: "true", b: "false" }`. With cursor on `a`'s
    value, the action must offer "Convert … to true"
    and not match `b`'s value. With cursor on `b`'s
    value, must offer "Convert … to false" and not
    match `a`'s value.
  - **Trailing comment preserved** —
    `key: "true"  # explicit string`. The edit must
    cover the scalar's loc span only, leaving the
    comment intact after the edit.
    (Already covered by `quoted_bool_edit_range_is_scalar_span_not_full_line`.)
  - **Flow-context preservation** —
    `{ a: "true", b: 1 }`. Converting `"true"` to `true`
    must not destroy the rest of the flow mapping
    (current full-line replacement would).
  - **Sequence-item scalar** — `- "true"`. The AST walk
    finds it (no mapping-value constraint). Convert
    produces `- true` without touching siblings.
    (Already covered by `quoted_bool_action_offered_for_sequence_item`.)
  - **Value with whitespace not offered** —
    `key: " true"` (decoded value has leading space).
    The action must NOT be offered (decoded value
    check is exact).
  - **Non-bool quoted string not offered** —
    `key: "hello"`. Not offered.
  - **Unquoted true already** — `key: true`. Not offered
    (style is Plain, not quoted).
  - **Cursor outside scalar span** — source
    `key: "true"  # comment` with cursor on the
    comment. Not offered (span-containment check
    excludes it).
- [x] Verify `parser_boundary_audit` allow-list is at
      exactly 100 entries (was 101; lost the retrofit
      entry). No per-entry re-verification needed —
      Task 1 already removed the entry. Confirm by
      inspection of the ALLOW_LIST constant and a fresh
      `cargo test --test parser_boundary_audit` run.
- [x] Remove the `Retrofit quoted_bool_to_unquoted to
      AST+formatter` bullet from
      `.ai/memory/project_followup_plans.md` under
      "Open: rlsp-yaml". Memory convention mandates
      removal of follow-up items whose plan has
      completed — this plan's completion closes that
      bullet. Do not leave the bullet in place with a
      "Completed" annotation; the file is a queue of
      open items only.
- [x] Build/test gates (same as Task 1).

Acceptance: regression tests cover all listed defect
classes; audit allow-list at exactly 100 entries;
`project_followup_plans.md` no longer contains the
`quoted_bool_to_unquoted` retrofit bullet; full
workspace suite green. No `feature-log.md` entry is
added — the file is reserved for user-facing feature
decisions; internal refactors like this retrofit
belong in git history and this plan file only.

**Completed:** commit `e1e3a67` — 8 new defect-class
regression tests added (trailing-comment and
sequence-item already covered by Task 1). Audit
allow-list verified at 100 entries. Follow-up bullet
removed from `project_followup_plans.md`. No
`feature-log.md` entry per the plan amendment.

## Non-Goals

- **YAML 1.1 bool variants** (`yes`/`no`/`on`/`off`/
  `True`/`False`/`TRUE`/`FALSE`) — handled by the
  separate `yaml11_bool_actions` retrofit (queued in
  `project_followup_plans.md`).
- **Additional scalar-to-plain conversions** (e.g.
  quoted numeric strings → unquoted numbers) — out of
  scope.
- **Expanding the action to sequences or mappings** —
  the action only converts individual scalars.
- **Change to `format_subtree` itself** — this plan
  consumes the existing API unchanged.
- **Other code-action retrofits** (`yaml11_bool_actions`,
  `yaml11_octal_actions`,
  `schema_yaml11_bool_type_actions`,
  `delete_unused_anchor`) — each is a separate future
  plan.

## Decisions

- **Approach: AST + `format_subtree`.** Mirrors the
  flow-to-block, block-to-flow, and string-to-block
  retrofits. Rejected "tighten text-parsing in place" —
  eliminates the bug class and aligns with the "one
  parser, one AST" rule.
- **`base_indent = scalar.loc.start.column`.** Plain
  scalars emit inline; the formatter does not add
  continuation-line indentation. Unlike
  `string_to_block_scalar` which needed
  `base_indent = key_col` for the block-scalar printer's
  `tab_width` behavior, a plain scalar emission is
  trivial — pass the scalar's own start column and
  `format_subtree` returns just the plain text
  (`true` or `false`).
- **Exact value match only.** YAML 1.2 bool tokens are
  exactly `true` and `false` (other forms are
  non-standard or YAML 1.1). The action refuses to
  convert anything else — even `"True"` or `"TRUE"` —
  because converting to plain form `True` in YAML 1.2
  is a string, not a bool. The `yaml11_bool_actions`
  retrofit handles those cases with its own semantics.
- **Cursor containment via span, not pattern end.**
  The new check is `scalar.loc.start.column <= col <=
  scalar.loc.end.column`. A cursor at column X where X
  is inside the scalar's source span is "on" the
  scalar, whether X lands on the opening quote, a
  character of the bool, or the closing quote.
- **No edit range change for trailing content.** The
  edit ends at the scalar's `loc.end.column`. If the
  parser's `loc` span is observed to extend past the
  closing quote (parser convention-dependent), that is
  a blocker — the developer messages the lead, not a
  silent workaround.
- **Mapping-value-only dispatch NOT enforced.** Unlike
  `string_to_block_scalar`, this action currently
  fires on any line context — mapping values,
  sequence items, top-level scalars. Preserve this.
  The AST walk finds scalars anywhere; no parent-type
  filter is applied.
- **Audit allow-list shrinks by exactly 1.** Pre-plan
  count is 101; post-Task-1 is 100. No new carve-outs
  or helper-of entries introduced — the retrofit is
  pure scalar-style swap, no new helpers.
