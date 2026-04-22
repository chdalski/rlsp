**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-21

# rlsp-yaml-parser YAML 1.2.2 Conformance Audit

## Goal

Produce an uncompromisable, section-by-section conformance
audit of `rlsp-yaml-parser` against YAML 1.2.2 and commit
it as the parser's own conformance documentation at
`rlsp-yaml-parser/docs/yaml-spec-conformance.md`. Every
numbered production in the spec is classified using a
strict, uniform entry format that makes each classification
**derivable from artifacts the reader can independently
re-verify**: a verbatim spec quote, a file+line
implementation citation, and a test-coverage reference.

Classifications are outputs of applying a fixed decision
rule to the spec quote and the implementation fact — they
are not opinions. An auditor cannot classify a production
without producing the artifacts, and a second-phase
verification pass re-checks each entry's artifacts against
the cached spec and source before the chapter commits.
This closes the interpretation-error class that an earlier
attempt at this audit exposed (a §9 [202] BOM
classification relied on a plausible but incorrect
paraphrase of §5.2; under the strict format, the required
verbatim quote would have directly contradicted the
classification).

The audit is a pure documentation deliverable. No source
code is changed by this plan. Remediation of any finding
surfaced by the audit — including any downstream
ramifications to `rlsp-yaml` or `rlsp-fmt` — is scoped to
separate follow-up plans the user initiates after reading
the conformance document. Committing to any specific
remediation inside this plan would bias the auditor.

## Context

- **Why the strict methodology.** A prior attempt at this
  audit permitted free-form "brief note" prose for
  Conformant entries. That allowed a Conformant
  classification on §9 [202] `l-document-prefix` to be
  justified with *"§5.2 restricts BOM to stream-start
  only"* — a paraphrase contradicted by the spec's actual
  text, which permits BOM at the start of any document.
  The reviewer's gate was structural (every production
  classified? every Lenient/Strict entry cited?) and did
  not verify the spec claim. One such error surfaced by
  chance; many more likely hid in the ~200
  interpretation-heavy Conformant entries. The user reset
  the work and asked for an audit that cannot hide
  interpretation errors.

- **Parser-only scope.** The audit covers
  `rlsp-yaml-parser` exclusively. The parser is the
  project's authority on valid YAML (root `CLAUDE.md`,
  Crate Boundaries). Auditing it first gives every
  downstream audit (`rlsp-yaml`, `rlsp-fmt`) a
  known-conformant oracle to classify against. Mixing
  input-side and output-side classifications in one pass
  entangles findings and undermines the clean derivation
  the methodology relies on.

- **Reference specification.** YAML 1.2.2 at
  <https://yaml.org/spec/1.2.2/>. A cached local copy is
  produced by Task 1; all subsequent tasks read from the
  cache to avoid WebFetch truncation (observed during the
  prior attempt — the live URL returned content truncated
  at production [86]).

- **Spec-section map already in the code.**
  `rlsp-yaml-parser/src/chars.rs` is organized by §5
  production numbers. Each §5 production should map to a
  predicate in `chars.rs` or to a sequence-level check in
  the lexer; any missing mapping is a finding.

- **Conformance baseline.** Parser passes 351/351 on the
  yaml-test-suite stream API; loader conformance is also
  in place. A high pass rate is not proof of conformance —
  lenient acceptance passes the suite without flagging
  non-conformant input. The audit is the tool that
  surfaces those cases.

- **Source files to cross-reference during the audit:**
  - `rlsp-yaml-parser/src/chars.rs` — §5 character
    predicates.
  - `rlsp-yaml-parser/src/lexer/` — tokenization
    (`plain.rs`, `quoted.rs`, `block.rs`, `comment.rs`).
  - `rlsp-yaml-parser/src/event_iter/` — event
    generation (`step.rs`, `flow.rs`, `block/`,
    `directives.rs`, `directive_scope.rs`,
    `properties.rs`).
  - `rlsp-yaml-parser/src/loader.rs` — AST construction.
  - `rlsp-yaml-parser/src/lines.rs` — indent handling.
  - `rlsp-yaml-parser/src/encoding.rs` — encoding and BOM.
  - `rlsp-yaml-parser/docs/architecture.md` — design doc
    that may already describe specific choices.

- **Test oracles.** Two sources anchor the audit to
  empirical behaviour:
  - yaml-test-suite at
    <https://github.com/yaml/yaml-test-suite>.
  - The project's own parser tests at
    `rlsp-yaml-parser/tests/` (including
    `conformance/`).

## Strict Entry Format

Every production, regardless of classification, uses this
format:

```
### [NNN] production-name

BNF: <exact BNF from the spec>

- Classification: Conformant | Lenient | Strict | Not Implemented | Not Applicable (descriptive) | Not Applicable (meta-notation)
- Spec (§X.Y): "<verbatim quote of the normative text>"
- Implementation: <crate>/<path>:<line-range>
- Test coverage: <yaml-test-suite case ID(s)> | <project test path> | no direct test
- Discrepancy: <one-sentence gap — Lenient/Strict only; omit for other classes>
```

### Classification decision rules (methodology, not opinion)

| Spec says | Code does | Classification |
|-----------|-----------|----------------|
| requires X | does X | **Conformant** |
| requires X | does X **and also** Y (Y not permitted) | **Lenient** |
| permits X | rejects X | **Strict** |
| requires X | does not implement X | **Not Implemented** |
| entry has no normative obligation on the implementation (purely descriptive or meta-notation) | — | **Not Applicable (descriptive)** or **Not Applicable (meta-notation)** |

The classification is the output of applying these rules
to the spec quote and the implementation fact recorded in
the entry. A classification that does not follow from the
recorded evidence is a reviewer-rejectable defect.

For `Not Applicable` entries: the Spec quote is still
required (it's what establishes that the entry is
descriptive / meta-notation); the Implementation and
Test coverage fields may be empty with the explicit text
`(no implementation obligation)`.

### Test-coverage conventions

- **yaml-test-suite case ID** — four-character identifier
  (e.g., `6CA3`) when a test case exercises the production.
  Multiple IDs allowed.
- **project test path** — `rlsp-yaml-parser/tests/<file>.rs`
  plus test function name, if the production is exercised
  by a project test.
- **no direct test** — valid only when neither of the
  above applies. An explicit "no direct test" is itself a
  data point (coverage gap); silent omission is not
  permitted.

## Non-Goals

- Remediating any finding inside this plan. Remediation
  is a follow-up decision.
- Auditing `rlsp-yaml` (language server + formatter) or
  `rlsp-fmt` (generic pretty-printer). Output-side
  conformance is out of scope here; each is its own
  future audit if needed.
- Surfacing downstream ramifications of hypothetical
  parser fixes. Ramifications are fix-specific and
  belong in each future remediation plan's Context, not
  in this audit.
- Rewriting `rlsp-yaml-parser/src/chars.rs` structure.
  The audit consumes the existing layout; any
  restructuring would be a separate concern.
- Changing the yaml-test-suite baseline.
- Expanding the audit beyond YAML 1.2.2. YAML 1.1
  compatibility diagnostics are out of scope.
- Producing per-production remediation recipes.
- Auditing productions defined outside §3–§10 (informative
  appendices, typographic conventions in §2).

## Steps

- [x] Task 1 — cache the YAML 1.2.2 spec locally at
      `.ai/references/yaml-1.2.2-spec.md`, verify
      completeness, write conformance-document scaffold
      with Methodology and entry-format reference
- [x] Task 2 — draft §3 + §4 + §5 chapter entries under
      the strict format
- [x] Task 3 — verify §3 + §4 + §5 entries against
      cached spec and cited source (independent pass)
- [x] Task 4 — draft §6 chapter entries
- [x] Task 5 — verify §6 entries
- [x] Task 6 — draft §7 chapter entries
- [x] Task 7 — verify §7 entries
- [x] Task 8 — draft §8 chapter entries
- [ ] Task 9 — verify §8 entries
- [ ] Task 10 — draft §9 chapter entries
- [ ] Task 11 — verify §9 entries
- [ ] Task 12 — draft §10 chapter entries
- [ ] Task 13 — verify §10 entries + append consolidated
      Summary table

## Tasks

### Task 1: Cache spec and write scaffold

Download the YAML 1.2.2 specification into
`.ai/references/yaml-1.2.2-spec.md` so every subsequent
task has deterministic access to the normative text.
Create `rlsp-yaml-parser/docs/yaml-spec-conformance.md`
with a Methodology section and the strict entry-format
reference reproduced inline.

- [x] `.ai/references/yaml-1.2.2-spec.md` exists and
      contains the full YAML 1.2.2 specification through
      the end of §10. Completeness is verified by two
      checks: (1) extract every production number `[N]`
      from the cached file and confirm the sequence has
      no gaps (1 through whatever N the cache's last
      production is); (2) confirm the cache contains a
      `## 10.` (or equivalent) chapter header AND ends
      with content from that chapter (not truncated
      mid-production). Both checks are recorded in the
      commit message.
- [x] The cached file records its source URL and fetch
      date in a comment at the top so future readers can
      re-fetch if needed.
- [x] `rlsp-yaml-parser/docs/yaml-spec-conformance.md` is
      created and opens with a Methodology section
      containing:
      - The spec reference (URL + cached-copy path).
      - The strict entry format block (identical to the
        "Strict Entry Format" section of this plan).
      - The classification decision-rule table.
      - The test-coverage conventions.
      - A statement that the audit is documentation-only,
        parser-scoped, and that remediation and
        downstream ramifications are out of scope.
- [x] `rlsp-yaml-parser/docs/yaml-spec-conformance.md`
      contains placeholder chapter headers `## §3`,
      `## §4`, `## §5`, `## §6`, `## §7`, `## §8`, `## §9`,
      `## §10`, `## Summary` — in that order, empty
      bodies, so subsequent draft tasks know exactly
      where their entries go.
- [x] `rlsp-yaml-parser/README.md` is updated under its
      `## Documentation` section to add an entry for
      the new conformance doc, e.g.:
      `- [YAML 1.2.2 Conformance](docs/yaml-spec-conformance.md) — per-production classification against the spec, with source citations and test coverage`.
      Placement: immediately after the existing
      `Architecture` entry. Without this entry the doc
      is orphaned from the crate's documentation index.
- [x] No source code is modified.
- [x] `cargo test --workspace` passes (sanity — nothing
      changed).
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `97bb19adea36225009da408d9ba57d3f02d226b2`

### Task 2: Draft §3 + §4 + §5 entries

Fill the `## §3`, `## §4`, `## §5` chapter bodies in
`rlsp-yaml-parser/docs/yaml-spec-conformance.md` with
one entry per numbered production, using the strict
format. §3 and §4 are short and descriptive; §5 covers
the character productions (character set, encodings,
indicators, line breaks, whitespace, misc, escaped
characters) and contains tens of numbered productions.

- [x] Every numbered production in §3 is recorded. A
      production with no normative obligation on the
      implementation (purely descriptive) is classified
      `Not Applicable (descriptive)` with a one-sentence
      rationale and an empty Implementation / Test
      coverage pair.
- [x] §4 is classified per the same rule. If §4 is
      entirely meta-notation with no implementation
      obligation, record a single `Not Applicable
      (meta-notation)` entry with a one-sentence
      rationale.
- [x] Every numbered production in §5 has an entry in
      the strict format, with: BNF, classification,
      verbatim spec quote from
      `.ai/references/yaml-1.2.2-spec.md`, implementation
      file+line, test coverage reference, and (for
      Lenient/Strict) a discrepancy sentence.
- [x] The classification of every §5 entry follows from
      the spec quote + implementation fact per the
      decision-rule table; no entry classifies without
      those artifacts.
- [x] Within this task's chapter block (text between
      the `## §N` header and the next `## ` header of
      the same level), every `### [` entry has a
      `- Test coverage:` line. Verify by extracting
      the chapter block and comparing counts:
      ```
      awk '/^## §N/{f=1; next} /^## /{f=0} f' \
        rlsp-yaml-parser/docs/yaml-spec-conformance.md \
        > /tmp/chapter.md
      [ "$(grep -c '^### \[' /tmp/chapter.md)" \
        = "$(grep -c '^- Test coverage:' /tmp/chapter.md)" ]
      ```
      Substitute the actual chapter number for `§N`.
      Equal counts is a pass; any mismatch is a defect.
      "No direct test" is a permitted value for the
      field.
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `93c5a46bf28e00bedf7619bb8acb8d8ad2462526`

### Task 3: Verify §3 + §4 + §5 entries (independent pass)

Re-read every entry added in Task 2. For each, perform
three independent checks: (1) the spec quote is a
verbatim match for the corresponding passage in
`.ai/references/yaml-1.2.2-spec.md`; (2) the
Implementation citation points at lines whose current
content plausibly implements what the entry claims;
(3) the classification follows from the quote + the
implementation fact under the decision-rule table.
Mismatches are corrected in-place during this task —
the verification phase is not a review; it's an editing
phase with an independent reviewer-style mindset.

- [x] Every §3, §4, §5 entry has its spec quote
      character-compared against the cached spec. Any
      discrepancy (missing text, paraphrase, added
      words) is corrected to a verbatim quote.
- [x] Every §3, §4, §5 entry has its Implementation
      citation opened and inspected. Any citation whose
      lines do not match the claim is corrected (new
      file+line) or the classification is adjusted.
- [x] Every §3, §4, §5 classification is re-derived
      from the verified quote + implementation under
      the decision-rule table. Mismatches are corrected.
- [x] Every §3, §4, §5 test-coverage claim is verified
      exhaustively: yaml-test-suite case IDs are opened
      in `rlsp-yaml-parser/tests/yaml-test-suite/` (or
      wherever the suite is symlinked in this repo)
      and confirmed to exercise the production; project
      test paths + function names are confirmed to
      exist; every "no direct test" claim is confirmed
      by a `grep` pass that finds no project test whose
      name or body references the production.
- [x] The task's commit message lists every entry that
      was corrected during the verification pass
      (production number + nature of the correction).
      An empty list is acceptable and must be stated
      explicitly.
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `4f932e675adebe99e1714154a32803b9f05c0c67`

### Task 4: Draft §6 entries

Fill the `## §6` chapter body with one entry per
numbered production in §6 (Structural Productions:
indentation, separation spaces, line prefixes, empty
lines, line folding, comments, separation lines,
directives, node properties), using the strict format.

- [x] Every numbered production in §6.1 through §6.9
      has an entry in the strict format, with all
      required fields plus (for Lenient/Strict) a
      discrepancy sentence.
- [x] Classifications follow from the spec quote +
      implementation fact under the decision-rule
      table.
- [x] Within this task's chapter block, every `### [`
      entry has a `- Test coverage:` line (same
      chapter-scoped awk+grep check as Task 2, with
      `§6` substituted for `§N`).
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `32131930bda98b84491a33724f06345390466e58`

### Task 5: Verify §6 entries (independent pass)

Same three-check protocol as Task 3, applied to every
§6 entry. Corrections in-place.

- [x] Every §6 entry's spec quote is character-compared
      against the cached spec; mismatches corrected.
- [x] Every §6 entry's Implementation citation is
      opened and inspected; mismatches corrected.
- [x] Every §6 classification is re-derived from the
      verified evidence; mismatches corrected.
- [x] Every §6 test-coverage claim is verified.
- [x] The task's commit message lists every §6
      correction made.
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `e3798fec7fa3a2c074c5a0af609efc15d960f358`

### Task 6: Draft §7 entries

Fill the `## §7` chapter body with one entry per
numbered production in §7 (Flow Style Productions:
alias nodes, empty nodes, flow scalar styles [plain,
single-quoted, double-quoted], flow collection styles
[flow sequence, flow mapping, pairs], flow nodes),
using the strict format.

- [x] Every numbered production in §7 has an entry in
      the strict format.
- [x] Classifications follow from the spec quote +
      implementation fact under the decision-rule
      table.
- [x] Within this task's chapter block, every `### [`
      entry has a `- Test coverage:` line (same
      chapter-scoped awk+grep check as Task 2, with
      `§7` substituted for `§N`).
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `83b0ff05d3b038c22b72b7da571cda613816d9a6`

### Task 7: Verify §7 entries (independent pass)

Same three-check protocol as Task 3, applied to every
§7 entry.

- [x] Every §7 entry's spec quote is character-compared
      against the cached spec; mismatches corrected.
- [x] Every §7 entry's Implementation citation is
      opened and inspected; mismatches corrected.
- [x] Every §7 classification is re-derived; mismatches
      corrected.
- [x] Every §7 test-coverage claim is verified.
- [x] The task's commit message lists every §7
      correction.
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `3e32c75fbba934da8d9fe7694f30e54762159346`

### Task 8: Draft §8 entries

Fill the `## §8` chapter body with one entry per
numbered production in §8 (Block Style Productions:
block scalar headers, literal/folded styles, chomping
indicators, block scalar content, block collection
styles [block sequence, block mapping in compact and
explicit forms], block nodes), using the strict format.

- [x] Every numbered production in §8 has an entry in
      the strict format.
- [x] Classifications follow from the spec quote +
      implementation fact under the decision-rule
      table.
- [x] Within this task's chapter block, every `### [`
      entry has a `- Test coverage:` line (same
      chapter-scoped awk+grep check as Task 2, with
      `§8` substituted for `§N`).
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

Commit: `27034c902135569e6db583d02602231debe791e3`

### Task 9: Verify §8 entries (independent pass)

Same three-check protocol, applied to every §8 entry.

- [ ] Every §8 entry's spec quote is character-compared
      against the cached spec; mismatches corrected.
- [ ] Every §8 entry's Implementation citation is
      opened and inspected; mismatches corrected.
- [ ] Every §8 classification is re-derived; mismatches
      corrected.
- [ ] Every §8 test-coverage claim is verified.
- [ ] The task's commit message lists every §8
      correction.
- [ ] No source code is modified.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

### Task 10: Draft §9 entries

Fill the `## §9` chapter body with one entry per
numbered production in §9 (Document Stream Productions:
bare documents, explicit documents with `---`/`...`
markers, directive documents, streams, stream-level
concatenation rules), using the strict format.

- [ ] Every numbered production in §9 has an entry in
      the strict format.
- [ ] Classifications follow from the spec quote +
      implementation fact under the decision-rule
      table.
- [ ] Within this task's chapter block, every `### [`
      entry has a `- Test coverage:` line (same
      chapter-scoped awk+grep check as Task 2, with
      `§9` substituted for `§N`).
- [ ] No source code is modified.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

### Task 11: Verify §9 entries (independent pass)

Same three-check protocol, applied to every §9 entry.
Note: the known-failed §9 [202] / §5 [3] BOM
classification from the prior attempt must be
re-derived from the verbatim §5.2 text — if this
verification cycle lands the same wrong classification
again, the plan itself is broken.

- [ ] Every §9 entry's spec quote is character-compared
      against the cached spec; mismatches corrected.
- [ ] Every §9 entry's Implementation citation is
      opened and inspected; mismatches corrected.
- [ ] Every §9 classification is re-derived; mismatches
      corrected.
- [ ] Every §9 test-coverage claim is verified.
- [ ] §5 [3] `c-byte-order-mark` and §9 [202]
      `l-document-prefix` classifications each carry a
      verbatim §5.2 quote drawn from the cached spec at
      `.ai/references/yaml-1.2.2-spec.md`. The
      classification is re-derived from that quote
      under the decision-rule table. If the cached spec
      does not contain the sentence "Byte order marks
      may appear at the start of any document" or
      equivalent normative wording about mid-stream
      BOMs, the developer records this as a finding in
      the commit message (the cache may be incomplete —
      a possible Task 1 defect that must be surfaced,
      not silently dismissed).
- [ ] The task's commit message lists every §9
      correction.
- [ ] No source code is modified.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

### Task 12: Draft §10 entries

Fill the `## §10` chapter body with one entry per
schema defined in §10 (Failsafe, JSON, Core, and any
other schemas the spec lists), using the strict format.
Each schema entry classifies parser tag-resolution and
plain-scalar type-inference.

- [ ] Every schema in §10 has an entry (or entries)
      covering tag-resolution and plain-scalar
      type-inference as the parser implements them, in
      the strict format.
- [ ] Classifications follow from the spec quote +
      implementation fact under the decision-rule
      table.
- [ ] Within this task's chapter block, every `### [`
      entry has a `- Test coverage:` line (same
      chapter-scoped awk+grep check as Task 2, with
      `§10` substituted for `§N`).
- [ ] No source code is modified.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

### Task 13: Verify §10 entries and append consolidated Summary

Apply the three-check protocol to every §10 entry, then
append a `## Summary` section listing every Lenient and
Strict finding from §3–§10 in a single table sorted by
spec section and production number.

- [ ] Every §10 entry's spec quote is character-compared
      against the cached spec; mismatches corrected.
- [ ] Every §10 entry's Implementation citation is
      opened and inspected; mismatches corrected.
- [ ] Every §10 classification is re-derived;
      mismatches corrected.
- [ ] Every §10 test-coverage claim is verified.
- [ ] `rlsp-yaml-parser/docs/yaml-spec-conformance.md`
      ends with a `## Summary` section listing every
      `Lenient` and `Strict` finding from §3–§10 in a
      single table with columns: spec production,
      classification, source file+line, one-sentence
      discrepancy, and test coverage.
- [ ] The summary table is sorted by spec section
      (§3, §4, §5, §6, §7, §8, §9, §10), then by
      production number within each section.
- [ ] The Summary section opens with a headline count:
      "N Lenient findings, M Strict findings, total K
      entries."
- [ ] Every Lenient/Strict entry in the chapter bodies
      (§3–§10) is represented in the Summary; every
      Summary row has a corresponding chapter-body
      entry. Bidirectional consistency is verified.
- [ ] The task's commit message lists every §10
      correction made during the verification phase
      and notes whether any body-Summary consistency
      mismatch was caught and corrected.
- [ ] No source code is modified.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

## Decisions

- **Parser-scoped audit.** `rlsp-yaml` and `rlsp-fmt`
  conformance are out of scope. The parser is the
  project's authority on valid YAML; its audit becomes
  the oracle for any future output-side audits. Mixing
  input-side and output-side classifications in one
  pass entangles findings.
- **Downstream ramifications belong to remediation
  plans, not this audit.** When a fix is chosen, its
  blast radius into `rlsp-yaml` and `rlsp-fmt` is
  specific to the fix approach — audit-time speculation
  would either bias the fix or rot before the fix runs.
  Each remediation plan's Context documents its
  downstream impact.
- **The strict entry format applies to every class,
  not just Lenient/Strict.** Previous methodology gave
  Conformant entries a "brief note" exit that hid
  interpretation errors. The strict format removes the
  exit: every entry carries a verbatim spec quote, a
  file+line citation, and a test-coverage reference.
- **Classifications are derivations, not opinions.**
  The decision-rule table maps (spec-says, code-does)
  pairs to a classification. The auditor's output is
  the classification derived from the entry's recorded
  evidence; the reviewer's check is that the
  classification matches the derivation.
- **Two-phase tasks per chapter (draft + verify)** —
  draft produces entries, verify re-checks them
  against the cached spec and source. The independence
  is the safeguard against interpretation errors that
  a structural-only review cannot catch.
- **Spec is cached locally** because WebFetch against
  the live URL truncated at production [86] during the
  prior attempt. `.ai/references/yaml-1.2.2-spec.md`
  makes the audit deterministic and reproducible.
- **Conformance document lives in
  `rlsp-yaml-parser/docs/yaml-spec-conformance.md`.**
  It is the parser's own conformance documentation,
  not a repo-root reference. Placing it inside the
  audited crate keeps it close to the code it
  describes and matches the project's convention of
  co-locating crate docs.
- **Tag field is not in the entry format.** The audit
  is parser-only; every entry is implicitly parser-
  scoped. Removing the field eliminates a redundant
  `parser` value on every entry and prevents the
  temptation to file output-side observations that
  are out of scope.
- **Test-coverage reference is mandatory.** "No direct
  test" is a valid value but must be written
  explicitly — a missing Test coverage field is a
  defect. This prevents silent gaps in the empirical
  anchoring.
- **Plan is audit-only; no remediation is
  pre-committed.** The user reset this plan twice
  because earlier drafts either mixed audit with
  remediation (tab drop) or produced insufficiently
  rigorous classifications. This plan stays strictly
  in the audit domain.
- **Verification tasks are not reviews; they are
  edits.** A verification task's developer corrects
  mismatches in place during the task, not in a
  separate rejection cycle. This keeps the task count
  bounded and the audit document converging.
- **If the verification pass reaffirms an incorrect
  classification, the plan is broken.** This is an
  explicit failure criterion — if a known-failed
  classification from the prior attempt (e.g., §9
  [202] BOM) survives the new methodology, the
  methodology itself has failed and the plan must be
  re-examined before continuing.
