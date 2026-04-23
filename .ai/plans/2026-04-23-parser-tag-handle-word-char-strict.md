**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-23

# Reject underscore in named tag handle names ([92])

## Goal

Make `rlsp-yaml-parser` reject `_` in named tag handle names per
YAML 1.2.2 production [38] `ns-word-char ::= ns-dec-digit |
ns-ascii-letter | '-'` (referenced by [92]
`c-named-tag-handle ::= c-tag ns-word-char+ c-tag`). Today
`is_valid_tag_handle` accepts `_`, producing the single remaining
bug-class Lenient finding in the §6 portion of the conformance
audit. After this plan, [92] flips from Lenient to Conformant
and the audit Summary drops from 5 Lenient entries to 4.

## Context

- **Conformance audit:** `rlsp-yaml-parser/docs/yaml-spec-conformance.md`
  — §6 [92] `c-named-tag-handle` is classified Lenient with
  Discrepancy "The spec's `ns-word-char` production excludes
  `'_'`, but `is_valid_tag_handle` accepts `'_'` as a valid
  character in named tag handle names." §5 [38] `ns-word-char` is
  classified Conformant but its Implementation line quotes
  `.is_ascii_alphanumeric() || c == '-' || c == '_'` — the quoted
  code is internally inconsistent with the Conformant label and
  must be tidied once the code matches the spec.
- **Spec text** (cached at `.ai/references/yaml-1.2.2-spec.md`):
  - Production [38] (§5.6): `ns-word-char ::= ns-dec-digit |
    ns-ascii-letter | '-'`. Underscore is not in the alternation.
  - Production [92] (§6.8.1): `c-named-tag-handle ::= c-tag
    ns-word-char+ c-tag`. Named-handle body is one-or-more
    `ns-word-char`.
- **Root cause (single production, single call site):**
  - `rlsp-yaml-parser/src/event_iter/properties.rs:281-295` —
    `is_valid_tag_handle`. The inner-word check at line 290 is
    `|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'`. The
    `|| c == '_'` disjunct is the bug.
  - `is_valid_tag_handle` is consumed only by
    `rlsp-yaml-parser/src/event_iter/directives.rs:182` (the
    `%TAG` directive handler). Inline tag references like
    `!handle!suffix` go through `scan_tag` in
    `rlsp-yaml-parser/src/event_iter/properties.rs:85` and rely
    on `is_ns_tag_char_single` for suffix characters — a separate
    and correct code path (`ns-tag-char` legitimately includes
    `_`, so suffix validation stays unchanged).
- **Stale paraphrases of the grammar in source comments:**
  - `rlsp-yaml-parser/src/event_iter/properties.rs:280` — doc
    line says "word chars are `[a-zA-Z0-9_-]`".
  - `rlsp-yaml-parser/src/event_iter/directives.rs:179-180` —
    comment says "word chars are ASCII alphanumeric, `-`, or
    `_`".
  - Both must be updated alongside the code fix so the source
    remains its own documentation.
- **Existing unit test asserts the Lenient behavior:**
  - `rlsp-yaml-parser/src/event_iter/properties.rs:482-485` —
    `is_valid_tag_handle_named_with_hyphen_and_underscore`
    currently asserts `is_valid_tag_handle("!my-handle_1!")` is
    `true`. The test bakes the bug in and must flip.
- **Feature-log convention:** `rlsp-yaml-parser/docs/feature-log.md`
  records user-facing behavioral decisions. Rejecting
  previously-accepted input is a user-observable behavior change
  and warrants an entry.
- **Follow-up queue entry to retire:** `.ai/memory/project_followup_plans.md`
  contains a bullet `[Lenient] Named tag handle underscore ([92])`
  under `## Open: rlsp-yaml-parser`. The bullet closes when this
  plan lands.
- **yaml-test-suite baseline:** the conformance test at
  `rlsp-yaml-parser/tests/conformance.rs` exercises the full
  test-suite. A test case that uses `%TAG !x_y! prefix` and
  currently passes under the lenient acceptance will now error.
  Any flips must be recorded in the commit message and, if the
  flip is correct (the lenient acceptance was wrong), the
  baseline updated to match.

## Non-Goals

- Tightening `is_ns_tag_char_single` or `is_ns_uri_char_single`.
  The spec's `ns-uri-char` production explicitly lists `_`, so
  tag-suffix character validation is already correct — this plan
  does not touch those predicates.
- Reclassifying any audit entries beyond [92] and the internal
  cleanup of the [38] Implementation quote. Other Lenient
  findings (schema resolution §10) are out of scope.
- Adding a configurable strict-vs-lenient mode. The spec does
  not permit `_` in named handles; accepting it is a bug, not a
  policy knob.
- Changing how tag directives are scanned, tokenized, or
  resolved. The fix is a one-character tightening of an
  existing predicate.

## Steps

- [ ] Task 1 — remove `_` from `is_valid_tag_handle`; update
      tests and stale source comments
- [ ] Task 2 — update conformance doc, feature-log, follow-up
      queue

## Tasks

### Task 1: Reject underscore in named tag handle names

Remove the `|| c == '_'` disjunct from the named-handle check,
update the two source comments that paraphrase the wrong
grammar, flip the existing unit test that asserts the Lenient
behavior, and add regression tests covering the tightened
acceptance at both unit and integration levels.

- [ ] In `rlsp-yaml-parser/src/event_iter/properties.rs:288-290`
      (`is_valid_tag_handle`, named-handle branch), replace the
      closure body so the inner-word check becomes
      `|c| c.is_ascii_alphanumeric() || c == '-'`. No other
      changes to the function.
- [ ] Update the doc comment at
      `rlsp-yaml-parser/src/event_iter/properties.rs:275-280` so
      the description of the named-handle word-char alphabet
      matches the fixed code (e.g., "word chars are
      `[a-zA-Z0-9-]`"). Do not introduce new wording beyond
      aligning the existing comment with the fix.
- [ ] Update the comment at
      `rlsp-yaml-parser/src/event_iter/directives.rs:179-180` so
      the paraphrased grammar matches the fixed code (e.g.,
      "word chars are ASCII alphanumeric or `-`"). Do not
      introduce new wording beyond aligning with the fix.
- [ ] Flip the existing unit test at
      `rlsp-yaml-parser/src/event_iter/properties.rs:482-485`.
      Rename the test function to
      `is_valid_tag_handle_named_with_hyphen` and assert that
      `is_valid_tag_handle("!my-handle-1!")` is `true`. The
      replacement preserves the hyphen case the original test
      was also covering; the underscore case moves to a new
      dedicated test (next sub-task).
- [ ] Add a new unit test
      `is_valid_tag_handle_rejects_named_with_underscore` in the
      same `tests` module that asserts
      `is_valid_tag_handle("!my_handle!")` is `false`. Add
      additional boundary cases in adjacent tests:
      `is_valid_tag_handle_rejects_underscore_only` for `!_!`,
      `is_valid_tag_handle_rejects_trailing_underscore` for
      `!abc_!`, and `is_valid_tag_handle_rejects_leading_underscore`
      for `!_abc!`. Each asserts `false`.
- [ ] Add integration-level regression tests (in the same test
      file used by existing `%TAG` directive coverage, or a new
      file if that is the closer fit — implementor's choice so
      long as the tests run under
      `cargo test -p rlsp-yaml-parser`) that exercise the
      production entry point (`rlsp_yaml_parser::load()` or
      `parse_events()`):
      (a) A document starting with `%TAG !my-handle! tag:example.org,2024:` and
          using `!my-handle!scalar` in the body parses
          successfully (hyphen remains accepted).
      (b) A document starting with `%TAG !my_handle! tag:example.org,2024:`
          produces a parse error at the directive position with
          a message that matches the existing malformed-handle
          error shape at `directives.rs:185`.
      (c) A document with an inline tag reference whose suffix
          contains `_` — for example `!!my_type scalar` or
          `!my-handle!my_suffix scalar` paired with a matching
          handle — still parses successfully. This verifies the
          fix did not leak into `scan_tag`.
- [ ] Run `cargo test -p rlsp-yaml-parser --test conformance` and
      record the before/after counts in the commit message. If
      yaml-test-suite cases flip status (previously Lenient
      acceptance that now errors), update the conformance-test
      baseline only for correct flips — a flip where the suite
      expects the parser to succeed on `%TAG !x_y! ...` would
      indicate the plan is wrong, not the baseline.
- [ ] `cargo test -p rlsp-yaml-parser` passes.
- [ ] `cargo test --workspace` passes (verifies no downstream
      consumer — `rlsp-yaml`, integration crates — relies on the
      previously-accepted underscore form).
- [ ] `cargo fmt --check` and `cargo clippy --all-targets` run
      clean.

### Task 2: Update conformance doc, feature-log, and follow-up queue

Reflect the tightened validation in the audit document and the
user-facing feature log, and close the follow-up queue item.

- [ ] Update the §6 [92] `c-named-tag-handle` entry in
      `rlsp-yaml-parser/docs/yaml-spec-conformance.md`: change
      Classification from `Lenient` to `Conformant`, update the
      Implementation citation to quote the fixed code
      (`.is_ascii_alphanumeric() || c == '-'`) and point at the
      post-fix line range, remove the Discrepancy line
      (Conformant entries have no discrepancy), and update the
      Test coverage field to cite the new regression tests from
      Task 1 (both unit and integration-level).
- [ ] Update the §5 [38] `ns-word-char` entry in the same file:
      Classification stays `Conformant`, but the Implementation
      text currently quotes `.is_ascii_alphanumeric() || c ==
      '-' || c == '_'` — replace that snippet with the fixed
      form `.is_ascii_alphanumeric() || c == '-'` so the audit
      text matches the post-fix code. No other changes to the
      [38] entry.
- [ ] Update the `## Summary` table in
      `rlsp-yaml-parser/docs/yaml-spec-conformance.md`: remove
      the row for `§6 [92] c-named-tag-handle`. Update the
      headline count. The current headline (after the 1024-char
      plan) is "5 Lenient findings, 0 Strict findings
      (bug-class), 3 Strict (security-hardened) findings, total
      8 entries." After this plan removes one Lenient entry, it
      becomes "4 Lenient findings, 0 Strict findings
      (bug-class), 3 Strict (security-hardened) findings, total
      7 entries."
- [ ] Update `rlsp-yaml-parser/docs/feature-log.md`: add a
      user-facing entry documenting that named tag handles now
      reject `_` per YAML 1.2.2 §5.6 (production [38]) and
      §6.8.1 (production [92]). Note that only `%TAG` directive
      handle names are affected — inline tag suffixes (e.g.,
      `!!my_type`) continue to accept `_` because the spec's
      `ns-uri-char` production explicitly permits it.
- [ ] Remove the stale follow-up queue entry. In
      `.ai/memory/project_followup_plans.md`, delete the
      `[Lenient] Named tag handle underscore ([92])` bullet
      under `## Open: rlsp-yaml-parser` — the work is complete.
- [ ] No source code is modified in this task (documentation
      only).
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets` run
      clean.

## Decisions

- **One code-fix task plus one doc task.** The code change is
  a one-character tightening in a single predicate with a
  handful of test edits. Splitting further would produce
  trivial commits; merging code and docs would make the code
  commit non-bisectable from the audit flip.
- **Keep [38]'s classification at Conformant; only fix its
  quoted Implementation text.** Before the code fix the [38]
  entry was Conformant-by-label but Lenient-by-quoted-code —
  an audit inconsistency, not a second classification. After
  the code fix, the label is correct and only the snippet
  needs to catch up. This is doc tidying, not a reclassification.
- **Flip the existing "hyphen and underscore" test rather than
  deleting it.** The test also covers the hyphen-accepted case
  that must keep passing. Splitting into
  `..._with_hyphen` + `..._rejects_...underscore` preserves
  coverage without losing the positive case.
- **Feature-log gets an entry.** Rejecting input the parser
  previously accepted is a user-observable change; the entry
  calls out that the tightening is scoped to `%TAG` directive
  handle names so users don't worry about inline tag suffixes
  that legitimately contain `_`.
- **Conformance doc is updated in the same plan as the fix.**
  Same pattern as the BOM, hex-escape, and 1024-char plans —
  the audit is a living document and a finding fixed here
  flips classification here.
- **yaml-test-suite baseline may shift.** Any flip must be a
  case where previous lenient acceptance was incorrect — the
  commit message records the before/after counts and the
  specific flipped cases (if any) so the shift is reviewable.
