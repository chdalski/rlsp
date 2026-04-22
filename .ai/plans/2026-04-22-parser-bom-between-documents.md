**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-22

# Fix BOM between documents ([3], [202], [211])

## Goal

Make `rlsp-yaml-parser` accept a byte-order mark (BOM,
U+FEFF) at the start of any document in a multi-document
stream, not just at the start of the stream. YAML 1.2.2
§5.2 states: "Byte order marks may appear at the start of
any document." The parser currently strips BOM only at
byte offset 0 (`lines.rs:116`) and rejects a BOM at the
start of a subsequent document prefix (after `...`),
causing three Strict findings in the conformance audit:
[3] `c-byte-order-mark`, [202] `l-document-prefix`, and
[211] `l-yaml-stream`. After this plan, all three
classifications flip from Strict to Conformant.

## Context

- **Conformance audit:** `rlsp-yaml-parser/docs/yaml-spec-conformance.md`
  — the audit document produced in the prior plan
  (`2026-04-21-yaml-spec-conformance-audit.md`). Findings
  [3], [202], [211] are all Strict due to the same root
  cause.
- **Spec:** YAML 1.2.2 §5.2: "Byte order marks may
  appear at the start of any document, however all
  documents in the same stream must use the same
  character encoding."
  Production [202] `l-document-prefix` =
  `c-byte-order-mark? l-comment*` — optional BOM at the
  start of each document prefix.
- **Current BOM stripping locations:**
  - `rlsp-yaml-parser/src/encoding.rs:88-96` — decoding
    layer strips BOM from the raw byte stream before
    parsing begins. This handles the stream-start BOM.
  - `rlsp-yaml-parser/src/lines.rs:110-127` —
    `scan_line()` strips BOM when `is_first` is true
    (first line of input only).
  - The `LineBuffer` tracks `is_first` and never resets
    it after the initial prime (`lines.rs:180`).
- **Tab-in-indent check at `step.rs:38-62`** — the event
  iterator rejects lines whose first character is `\t`.
  A BOM character `\u{FEFF}` is not `\t`, so this check
  does not cause the BOM rejection. The rejection comes
  from the lexer encountering BOM as an unexpected
  character in a position where it expects indentation
  or a document marker.
- **Existing tests:**
  - `tests/encoding.rs:162` —
    `parse_events_accepts_bom_at_stream_start()` (passes).
  - `tests/encoding.rs:174` —
    `parse_events_rejects_bom_mid_stream()` — this tests
    BOM mid-scalar, which should remain an error; it is
    NOT the inter-document BOM case.
  - `src/lines.rs:774` —
    `bom_only_stripped_from_first_line()` — documents the
    current behavior: BOM on a non-first line is preserved
    as data. This test's expectation will change.
- **yaml-test-suite:** the conformance suite (351/351
  stream, 724+ loader) may include cases that exercise
  inter-document BOM. Check for test-suite cases that use
  `⇔` (BOM visual marker) between documents.
- **Downstream.** `rlsp-yaml` (`format_yaml`) calls
  `rlsp_yaml_parser::load()` — if BOM-prefixed documents
  are now accepted, the formatter receives them without
  change. No formatter-side work is expected, but verify
  with an integration-level smoke test.

## Steps

- [x] Task 1 — extend BOM stripping to document prefixes
      and add tests
Commit: `b997fd959d44cc4006781d3079a0a1b8dfdceadb`

- [ ] Task 2 — update conformance doc and verify
      yaml-test-suite baseline

## Non-Goals

- Fixing the other audit findings (hex escapes, implicit
  key limit, tag handle underscore). Each has its own
  follow-up plan.
- Changing BOM handling in the `encoding.rs` decoding
  layer — stream-start BOM stripping at the byte level is
  correct and stays.
- Supporting different encodings per document. §5.2 says
  "all documents in the same stream must use the same
  character encoding" — the parser can ignore encoding
  changes at BOM boundaries.

## Tasks

### Task 1: Extend BOM stripping to document prefixes

Make the parser strip a BOM character at the start of any
document prefix (after `...` or at stream start), so
multi-document streams with inter-document BOMs parse
correctly.

- [ ] Identify the mechanism: either reset `is_first` in
      `LineBuffer` when the event iterator encounters a
      document-end marker (`...`), or add a BOM-check in
      the event iterator's document-prefix scanning (e.g.
      in `directive_scope.rs` or `step.rs`), or handle it
      in `scan_line` with a new flag. The developer
      chooses the approach that minimizes blast radius.
- [ ] The chosen approach correctly handles:
      (a) BOM at the very start of the stream (existing
          behavior, must not regress).
      (b) BOM immediately after a `...` document-end
          marker, before the next document's content.
      (c) BOM after `...` followed by blank lines or
          comments before the next document.
      (d) Multiple documents in a row, each with a BOM.
      (e) BOM mid-scalar (must still be an error — not a
          document prefix position).
- [ ] New unit tests in `rlsp-yaml-parser/tests/encoding.rs`
      (or a more appropriate test file) cover cases (a)–(e)
      above. Each test has a descriptive name and asserts
      either successful parse (a–d) or parse error (e).
- [ ] The existing test `bom_only_stripped_from_first_line`
      in `src/lines.rs` is updated or removed to reflect
      the new behavior (BOM may now be stripped at
      document-prefix positions, not just the first line).
- [ ] `parse_events_rejects_bom_mid_stream` in
      `tests/encoding.rs` is verified to still pass (BOM
      mid-scalar is a different case from BOM at document
      prefix).
- [ ] Integration smoke test: a multi-document YAML
      string with BOM between documents is passed through
      `rlsp_yaml_parser::load()` and produces the correct
      AST (correct number of documents, correct scalar
      values). This exercises the production entry point.
- [ ] `cargo test -p rlsp-yaml-parser` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

### Task 2: Update conformance doc and verify test-suite baseline

Update the conformance audit to reflect the fix and verify
the yaml-test-suite baseline is unaffected (or improved).

- [ ] In `rlsp-yaml-parser/docs/yaml-spec-conformance.md`,
      update the §5 [3] `c-byte-order-mark` entry:
      change Classification from Strict to Conformant,
      update the Implementation citation to reflect the
      new BOM-stripping location, remove the Discrepancy
      line (Conformant entries have no discrepancy), and
      update the Test coverage field to cite the new
      inter-document BOM tests added in Task 1.
- [ ] Update the §9 [202] `l-document-prefix` entry:
      same changes — Strict → Conformant, updated
      Implementation citation, discrepancy removed, Test
      coverage updated to cite the new tests.
- [ ] Update the §9 [211] `l-yaml-stream` entry: same
      changes — Strict → Conformant, updated
      Implementation citation, discrepancy removed, Test
      coverage updated to cite the new tests.
- [ ] Update the `## Summary` table: remove the three
      entries for [3], [202], [211]. Update both the
      headline Strict count (was: 6 Strict; now: 3
      Strict) and the total-entries count (was: 15;
      now: 12). The full headline becomes "9 Lenient
      findings, 3 Strict findings, total 12 entries."
- [ ] Update the "Encoding Detection and Decoding"
      entry in `rlsp-yaml-parser/docs/feature-log.md`
      to note that BOM is also accepted (stripped) at
      document-prefix positions, not only at stream
      start. This is a behavioral change (parser
      now accepts input it previously rejected), so the
      feature-log entry should reflect the expanded
      acceptance.
- [ ] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and record the result. If any
      yaml-test-suite cases flipped status (e.g., a case
      with inter-document BOM that was previously failing
      now passes), update the conformance expectations.
      Record the before/after counts in the commit message.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

## Decisions

- **One plan for all three productions.** [3], [202], and
  [211] share a single root cause (BOM stripped only at
  byte offset 0). Fixing the root cause resolves all three.
- **Implementation approach is developer's choice.** The
  plan describes the required behavior (BOM accepted at
  document-prefix positions) and test cases, not the
  implementation strategy. The developer picks the
  approach that best fits the parser's architecture.
- **BOM mid-scalar remains an error.** The fix applies
  only to document-prefix positions (where [202]
  `l-document-prefix` permits `c-byte-order-mark?`). BOM
  inside scalar content, comments, or other non-prefix
  positions is still invalid.
- **Conformance doc is updated in this plan.** The audit
  is a living document; when a finding is fixed, its
  classification is updated in the same plan that fixes
  it. This keeps the audit current and prevents stale
  Strict entries.
