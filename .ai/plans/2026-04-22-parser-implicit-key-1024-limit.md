**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-22

# Enforce 1024-character implicit key limit ([154], [155], [192], [193])

## Goal

Make `rlsp-yaml-parser` enforce YAML 1.2.2's 1024-Unicode-character
limit on implicit mapping keys in both flow and block contexts. The
spec at §7.4.3 states: "the ':' indicator must appear at most 1024
Unicode characters beyond the start of the key." §8.2.2 extends the
same limit to block-context implicit keys. The parser currently
enforces the single-line restriction but not the character limit,
producing four Lenient findings in the conformance audit: [154]
`ns-s-implicit-yaml-key`, [155] `c-s-implicit-json-key`, [192]
`ns-l-block-map-implicit-entry`, and [193]
`ns-s-block-map-implicit-key`. After this plan, all four
classifications flip from Lenient to Conformant.

## Context

- **Conformance audit** (completed at commit `4a2e197`): entries [154]
  and [155] in §7, entries [192] and [193] in §8 classified Lenient,
  sharing one root cause — no length counter in `event_iter/flow.rs`
  or `event_iter/block/mapping.rs`.
- **Spec text (cached at `.ai/references/yaml-1.2.2-spec.md`):**
  - Line 4650-4654 (§7.4.3): "If the '?' indicator is omitted,
    parsing needs to see past the implicit key to recognize it as
    such. To limit the amount of lookahead required, the ':'
    indicator must appear at most 1024 Unicode characters beyond the
    start of the key. In addition, the key is restricted to a single
    line."
  - Line 4684, 4691: productions [154] and [155] carry the comment
    `/* At most 1024 characters altogether */`.
  - Line 5719 (§8.2.2): "they are limited to a single line and must
    not span more than 1024 Unicode characters."
- **Unicode characters, not bytes.** The spec measures "Unicode
  characters" — the parser must count Rust `char`s (via
  `.chars().count()`), not UTF-8 bytes. A single-character input
  like `"é"` is one character but two bytes; a 1024-char limit
  based on bytes would falsely reject 1024-character keys that
  happen to contain multibyte codepoints.
- **Spec scope: implicit keys only.** The 1024-char limit applies
  ONLY to implicit keys (no `?` indicator). Explicit keys introduced
  by `?` are not restricted. The parser must preserve this
  asymmetry: explicit keys larger than 1024 characters stay valid.
- **Block-context entry points:**
  - `rlsp-yaml-parser/src/event_iter/block/mapping.rs:88-300` —
    `consume_mapping_entry` handles implicit keys via the
    `find_value_indicator_offset` helper from `line_mapping.rs:68`.
    That helper returns the byte offset of the `:` indicator;
    characters between start-of-line and that offset constitute the
    implicit key.
  - `rlsp-yaml-parser/src/event_iter/line_mapping.rs:68` —
    `find_value_indicator_offset(trimmed: &str) -> Option<usize>` is
    the shared lookup. It is the natural place to surface the
    character count, either by returning `(offset, char_count)` or by
    adding a dedicated helper that both block and flow can call.
- **Flow-context entry points:**
  - `rlsp-yaml-parser/src/event_iter/flow.rs:1077-1093` — the flow
    context's `:`-separator detection for single-pair implicit
    mapping keys. The existing comment cites §7.4.1 and §7.4.2 and
    currently restricts flow-sequence implicit keys to one line; the
    1024-char check must be added alongside the single-line check.
    The comparable location for flow-mapping implicit keys lives in
    the same function's case-match around the same block.
  - `rlsp-yaml-parser/src/event_iter/flow.rs:1359-1620` — JSON-key
    implicit pair handling (quoted scalars as keys). The length
    check applies here too.
- **Error-message format:** the parser's existing errors use
  concise messages with the spec section in parentheses — e.g.
  `"implicit flow mapping key must be on a single line"` at
  `flow.rs:1089`. The new error should follow the same style, for
  example `"implicit key exceeds 1024 Unicode characters (YAML 1.2
  §7.4.3)"` for flow and `"implicit block key exceeds 1024 Unicode
  characters (YAML 1.2 §8.2.2)"` for block.
- **Production coverage:** four audit entries, two contexts, one
  spec rule. Block and flow share the *rule* but not the *code
  path*; the plan decomposes accordingly into separate tasks so
  each is independently reviewable.

## Non-Goals

- Changing how implicit keys are scanned or tokenized. The fix
  adds a length check on the already-scanned key span; it does
  not reorganize the scan loop.
- Enforcing the limit on explicit keys. The spec restricts only
  implicit keys, and YAML documents with >1024-char explicit keys
  must continue to parse successfully.
- Adding a configurable limit. The 1024 value is a spec constant,
  not a policy knob.
- Reclassifying any other audit entries. Only [154], [155], [192],
  [193] are in scope.
- Performance micro-optimization of the character-counting step.
  A straightforward `.chars().count()` on the pre-bounded key slice
  is adequate — the slice is at most the line length, and the
  check runs once per implicit key, not per character.

## Steps

- [ ] Task 1 — enforce 1024-char limit for block-context implicit
      keys ([192], [193])
- [ ] Task 2 — enforce 1024-char limit for flow-context implicit
      keys ([154], [155])
- [ ] Task 3 — update conformance doc, feature-log, follow-up queue

## Tasks

### Task 1: Enforce 1024-char limit for block-context implicit keys

Add a Unicode-character length check in the block-mapping entry
handler so implicit keys spanning more than 1024 characters yield
a parse error, while explicit (`?`-introduced) keys remain
unrestricted. Add regression tests covering both boundary and
multi-byte codepoint cases.

- [ ] In `rlsp-yaml-parser/src/event_iter/block/mapping.rs:88-300`
      (`consume_mapping_entry`, implicit-key branch), after
      `find_value_indicator_offset` returns the `:` byte offset,
      compute the Unicode-character count of the key slice
      (`trimmed[..offset].chars().count()`) and emit a parse error
      if it exceeds 1024. The error position is the `:` indicator
      (same as the single-line error at `flow.rs:1089`).
- [ ] The error message matches the project's style and cites the
      spec section: `"implicit block key exceeds 1024 Unicode
      characters (YAML 1.2 §8.2.2)"` (exact wording at implementor's
      discretion as long as it names the limit, the count unit, and
      the spec section).
- [ ] The check applies to BOTH plain (YAML-key) and quoted
      (JSON-key) implicit keys in block context, covering both
      [192] and [193].
- [ ] The check does NOT apply to explicit `?`-introduced keys.
      A `? ` key with >1024 characters continues to parse
      successfully.
- [ ] Regression tests in `rlsp-yaml-parser/tests/` (new file or
      appropriate existing file) cover:
      (a) 1024-ASCII-character implicit key → parses successfully
      (b) 1025-ASCII-character implicit key → parse error with the
          new message
      (c) 1024-character implicit key with multi-byte UTF-8
          codepoints (e.g. 1024 `é` characters = 2048 bytes) →
          parses successfully (proves the count is characters not
          bytes)
      (d) Explicit `? ` key with >1024 characters → parses
          successfully
      (e) Integration smoke test: `rlsp_yaml_parser::load()` on a
          multi-document or nested input where an over-length
          implicit key is nested several levels deep still produces
          the correct error at the correct position.
- [ ] The existing single-line restriction at the same code path is
      unchanged; both restrictions now fire at the same point but
      the single-line check fires first (a multi-line implicit key
      is malformed regardless of length).
- [ ] `cargo test -p rlsp-yaml-parser` passes.
- [ ] `cargo test -p rlsp-yaml-parser --test conformance` passes;
      if any yaml-test-suite cases flip status, record the
      before/after counts in the commit message and update the
      conformance-test baseline only if the flip is correct (a case
      that previously passed leniently now correctly errors).
- [ ] `cargo fmt --check` and `cargo clippy --all-targets` run
      clean.

### Task 2: Enforce 1024-char limit for flow-context implicit keys

Add the same Unicode-character length check to the flow-context
implicit-key handling so flow-context implicit keys ([154] and
[155]) that exceed 1024 characters produce a parse error, while
explicit `?`-introduced flow keys remain unrestricted.

- [ ] In `rlsp-yaml-parser/src/event_iter/flow.rs:1077-1093` (the
      `:`-separator branch for single-pair flow-sequence implicit
      keys), add a Unicode-character length check that counts from
      the start of the key to the `:` indicator. Emit a parse
      error if the count exceeds 1024. Error position is the `:`
      indicator.
- [ ] Extend the same check to the flow-mapping implicit-key
      handling elsewhere in `flow.rs` so both [154] (plain
      YAML-key) and [155] (quoted JSON-key) paths are covered.
      The implementor identifies the exact call-sites during
      implementation; the plan does not prescribe a single line
      range beyond 1077-1093 because the JSON-key path at lines
      1359-1620 may also need the check.
- [ ] The error message follows the project style and cites the
      spec section: `"implicit flow key exceeds 1024 Unicode
      characters (YAML 1.2 §7.4.3)"` (exact wording at
      implementor's discretion as long as it names the limit, the
      count unit, and the spec section).
- [ ] The check does NOT apply to explicit `?`-introduced flow
      keys. A flow `{? long-key : value}` with an explicit-key
      indicator longer than 1024 characters continues to parse
      successfully.
- [ ] Regression tests mirror Task 1's cases (a)–(e) adapted to
      flow context: both `{key: value}` flow-mapping and `[key:
      value]` flow-sequence single-pair forms, plus one test each
      for the quoted-key (JSON-key) path.
- [ ] The existing single-line restriction (the `"implicit flow
      mapping key must be on a single line"` error at
      `flow.rs:1089`) is unchanged; both restrictions now fire at
      the same point but the single-line check fires first.
- [ ] `cargo test -p rlsp-yaml-parser` passes.
- [ ] `cargo test -p rlsp-yaml-parser --test conformance` passes;
      record flips in the commit message as in Task 1.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets` run
      clean.

### Task 3: Update conformance doc, feature-log, and follow-up queue

Reflect the enforcement in the conformance audit and user-facing
feature log, and close the follow-up queue item.

- [ ] Update the §7 [154] `ns-s-implicit-yaml-key` entry in
      `rlsp-yaml-parser/docs/yaml-spec-conformance.md`: change
      Classification from `Lenient` to `Conformant`, update the
      Implementation citation to reference the new 1024-char
      check's location, remove the Discrepancy line, and update
      the Test coverage field to cite the new tests added in
      Task 2.
- [ ] Update the §7 [155] `c-s-implicit-json-key` entry: same
      changes — Lenient → Conformant, updated Implementation
      citation, Discrepancy removed, Test coverage updated.
- [ ] Update the §8 [192] `ns-l-block-map-implicit-entry` entry:
      same changes, citing the Task 1 check location.
- [ ] Update the §8 [193] `ns-s-block-map-implicit-key` entry:
      same changes, citing the Task 1 check location.
- [ ] Update the `## Summary` table: remove the four entries for
      [154], [155], [192], [193]. Update the headline count. The
      current headline (after the hex-escape reclassification) is
      "9 Lenient findings, 0 Strict findings (bug-class), 3
      Strict (security-hardened) findings, total 12 entries."
      After this plan removes four Lenient entries, it becomes
      "5 Lenient findings, 0 Strict findings (bug-class), 3
      Strict (security-hardened) findings, total 8 entries."
- [ ] Update `rlsp-yaml-parser/docs/feature-log.md`: add a
      user-facing entry (or extend an existing mapping-related
      entry) documenting that implicit mapping keys are now
      capped at 1024 Unicode characters, citing §7.4.3 and §8.2.2
      and noting that explicit `?`-introduced keys are unaffected.
- [ ] Remove the stale follow-up queue entry. In
      `.ai/memory/project_followup_plans.md`, delete the
      `[Lenient] 1024-character implicit key limit ([154], [155],
      [192], [193])` bullet — the work is now complete.
- [ ] No source code is modified in this task (documentation
      only).
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets` run
      clean.

## Decisions

- **Two code-fix tasks plus one doc task, not one combined task.**
  Block and flow contexts share the spec rule but not the code
  path. Splitting into Task 1 (block) and Task 2 (flow) keeps each
  commit independently reviewable and bisectable, matches the
  audit's own breakdown (§7 entries vs §8 entries), and avoids a
  single commit that touches both hot paths.
- **Count characters, not bytes.** The spec measures Unicode
  characters. `.chars().count()` on the key's pre-bounded slice is
  correct and straightforward; a byte-length check would falsely
  reject 1024-char keys with multibyte codepoints. The test case
  with 1024 `é` characters verifies this.
- **Limit applies only to implicit keys.** Explicit keys (`?`
  indicator) have no length limit per spec; test case (d) verifies
  the parser preserves this.
- **Error message names the spec section.** Consistent with the
  existing implicit-key error at `flow.rs:1089`. Makes the
  conformance connection obvious in developer output.
- **Conformance doc is updated in the same plan as the fix.** Same
  pattern as the BOM plan — the audit is a living document, and a
  finding that is fixed in this plan must flip classification in
  this plan too.
- **yaml-test-suite baseline may shift.** Cases that previously
  passed leniently (over-length implicit keys parsed without
  error) will now correctly error. If any flips occur, Task 1 and
  Task 2 record them in commit messages and update the
  conformance-test baseline only for correct flips.
