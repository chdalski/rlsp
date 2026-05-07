**Repository:** root
**Status:** Completed (2026-04-22)
**Created:** 2026-04-22

# Reclassify hex-escape security hardening as deliberate divergence ([59], [60], [61])

## Goal

Update `rlsp-yaml-parser/docs/yaml-spec-conformance.md` so the
hex-escape strictness at `lexer/quoted.rs:594-618` is recorded as a
**deliberate security policy**, not an implementation bug. The three
Strict findings [59] `ns-esc-8-bit`, [60] `ns-esc-16-bit`, and [61]
`ns-esc-32-bit` are reclassified under a new `Strict (security-hardened)`
sub-class, and their Discrepancy text is expanded to cover both layers
of the rejection (non-printable + bidi-control). The parser's behavior
does not change; only the conformance document's framing of it does.
After this plan, the audit correctly distinguishes "strict because of
a bug" from "strict because the project chose to be."

## Context

- **Audit state:** The parser's conformance audit
  (`rlsp-yaml-parser/docs/yaml-spec-conformance.md`, completed at
  commit `4a2e197`) classified [59]/[60]/[61] as Strict — spec
  permits the codepoints, code rejects them, which is Strict under
  the audit's decision-rule table. That classification is spec-correct
  but doesn't distinguish the deliberate security policy from an
  accidental bug.
- **The two-layer rejection** in `rlsp-yaml-parser/src/lexer/quoted.rs`:
  - Lines 594-606: rejects hex escapes (`\x`, `\u`, `\U`) when the
    decoded character is not `c-printable`. The comment at line 594
    explicitly says: *"Security: for hex escapes (\x, \u, \U), the
    decoded character must be a YAML c-printable character. Named
    escapes (\0, \a, \b, …) produce well-known control chars and are
    exempt from this check."*
  - Lines 608-618: rejects hex escapes when the decoded character is a
    bidi-control character (U+202A–U+202E, U+2066–U+2069). The comment
    at line 608 says: *"Security: reject bidi override characters
    produced by numeric escapes (\u and \U can reach the bidi range;
    \x max is U+00FF)."*
- **Bidi range is inside `c-printable`.** `is_c_printable` at
  `chars.rs:15-26` allows `\u{A0}..=\u{D7FF}`, which includes the bidi
  override codepoints. So the bidi-control rejection is a distinct
  layer, not redundant with the c-printable check. The audit's current
  Discrepancy text mentions only the c-printable check, not the bidi
  check — the entries need both.
- **Named-vs-hex asymmetry.** `decode_escape` at `chars.rs:173-199`
  permits named escapes like `\0` (U+0000), `\a` (U+0007), `\e`
  (U+001B), `\N` (U+0085) that decode to non-printable characters. The
  quoted-scalar check explicitly exempts these. The rationale in the
  source comment: named escapes are an opt-in, named set; hex escapes
  are a generic numeric encoding that is more prone to encoding-smuggling
  attacks. This asymmetry is intentional and should be documented in the
  audit, not "fixed" by tightening the named escapes.
- **YAML 1.2.2 §5.7 (cached at `.ai/references/yaml-1.2.2-spec.md`):**
  the spec defines `ns-esc-8-bit`, `ns-esc-16-bit`, `ns-esc-32-bit` as
  producing any valid Unicode codepoint in the respective range.
  Rejecting spec-permitted codepoints is Strict; this plan doesn't
  dispute the Strict classification — it adds a sub-class to record
  *why*.
- **Current audit Summary headline:** "9 Lenient findings, 3 Strict
  findings, total 12 entries." After this plan, the three hex-escape
  entries remain Strict but carry a `(security-hardened)` marker. The
  headline should reflect the sub-classification so reviewers can tell
  at a glance which Strict findings are bugs and which are policy.
- **No parser behavior change in this plan.** The code stays exactly as
  it is today. Only the audit document and the feature-log gain
  clarifying prose.
- **No new security checks added or removed.** If the project wants to
  tighten named escapes too (a stricter, consistent policy), that's a
  separate decision for a future plan.

- **Methodology additions this plan introduces.** The conformance
  doc's Methodology section will gain a new sub-class `Strict
  (security-hardened)` and a new `Rationale` field. Concrete shape:
  - Classification enum extended to:
    `Conformant | Lenient | Strict | Strict (security-hardened) | Not Implemented | Not Applicable (...)`
  - Decision-rule table gains one row:
    *permits X | rejects X as part of a documented security policy →*
    `Strict (security-hardened)`
  - `Rationale` is a new entry field, required only for
    `Strict (security-hardened)` entries and optional elsewhere. It
    cites the source-code comment, feature-log entry, or design doc
    that marks the divergence as deliberate.
  - Without a Rationale citation a `Strict (security-hardened)`
    classification is not justified — this prevents the sub-class
    from being used as a laundering tag for accidental bugs.

## Non-Goals

- Changing parser behavior. No code edits in `chars.rs`, `lexer/`, or
  any other source file.
- Tightening the named-escape side of the asymmetry. The asymmetry is
  intentional per the source comment; a future plan can revisit it
  if desired.
- Re-auditing the parser for other deliberate-security checks that may
  have been classified as generic Strict. The plan covers only the
  three hex-escape entries [59]/[60]/[61]. If a later pass finds more,
  they follow the same reclassification pattern in a separate plan.
- Adding or expanding tests for the hex-escape rejection. The existing
  tests in `rlsp-yaml-parser/src/chars.rs:391-394` and
  `tests/yaml-test-suite/src/G4RS.yaml` already cover it.
- Introducing a configurable strict-vs-secure mode on the parser.

## Steps

- [x] Task 1 — extend the conformance doc's Methodology with the
      `Strict (security-hardened)` sub-class and Rationale field
- [x] Task 2 — reclassify [59], [60], [61] and update their Discrepancy
      text to cover both rejection layers; update Summary table and
      headline; update feature-log

## Tasks

### Task 1: Add `Strict (security-hardened)` sub-class to Methodology

Extend the conformance document's Methodology section to define the new
sub-class, its required Rationale line, and the decision-rule row. This
task adds the vocabulary; Task 2 uses it.

- [x] In `rlsp-yaml-parser/docs/yaml-spec-conformance.md`, update the
      Strict Entry Format classification list to include `Strict
      (security-hardened)` as a valid value.
- [x] Add a new field `Rationale` to the entry format, required for
      `Strict (security-hardened)` entries: a one-sentence reference to
      the source comment, feature-log entry, or design doc that marks
      the divergence as deliberate. Document that `Rationale` is
      optional for other classifications and mandatory for
      security-hardened ones.
- [x] Extend the decision-rule table with a new row:
      `permits X | rejects X as part of a documented security policy`
      → `Strict (security-hardened)`.
- [x] Add a short prose note immediately under the decision-rule
      table explaining that `Strict (security-hardened)` is a
      sub-class of Strict, not a separate top-level class — it still
      means the parser rejects spec-permitted input, but the rejection
      is deliberate and the code or documentation explains why. Without
      a Rationale citation, a classifier cannot use this sub-class.
- [x] The existing `Strict` classification (without the sub-class
      marker) continues to mean "rejects spec-permitted input,
      unintentional or undecided" — the default Strict is a bug until
      proven otherwise.
- [x] No other entries in the doc are modified in this task — only
      the Methodology section.
- [x] `cargo test --workspace` passes (sanity — nothing changed).
- [x] `cargo fmt --check` and `cargo clippy --all-targets` run clean.

Commit: `985127e907e2176057a7762dd408c43633553acb`

### Task 2: Reclassify [59], [60], [61] and update Summary

Apply the new sub-class to the three hex-escape entries, expand their
Discrepancy text to cover the bidi-control layer, add Rationale lines
citing the source comments, update the Summary table, and note the
project convention in the feature-log.

- [x] Update the §5 [59] `ns-esc-8-bit` entry:
      - Change Classification from `Strict` to `Strict
        (security-hardened)`.
      - Expand the Discrepancy line to cover BOTH rejection layers:
        the c-printable check (`quoted.rs:594-606`) AND the
        bidi-control check (`quoted.rs:608-618`). One sentence each,
        joined — e.g., "the implementation rejects hex escapes whose
        decoded character falls outside `c-printable`
        (`quoted.rs:594-606`); it additionally rejects hex escapes
        whose decoded character is in the bidi-override range
        (U+202A–U+202E, U+2066–U+2069) via the bidi-control check at
        `quoted.rs:608-618`."
      - Add a `Rationale` field citing the source comments at
        `quoted.rs:594` ("Security: for hex escapes ...") and
        `quoted.rs:608` ("Security: reject bidi override characters ...").
      - Also note the named-vs-hex asymmetry: named escapes like `\0`,
        `\a`, `\e`, `\N` are exempt from the c-printable check by
        design; this is documented in the source comment at
        `quoted.rs:594`.
- [x] Update the §5 [60] `ns-esc-16-bit` entry: same reclassification,
      same Discrepancy expansion, same Rationale addition.
- [x] Update the §5 [61] `ns-esc-32-bit` entry: same reclassification,
      same Discrepancy expansion, same Rationale addition.
- [x] Update the `## Summary` table:
      - The three entries stay in the table (they are still
        divergences from the spec, just deliberate ones).
      - **Rewrite the Classification cell of rows [59], [60], [61]
        in the Summary table from `Strict` to `Strict
        (security-hardened)`.** Do not merely add a column or suffix
        as cosmetic decoration while leaving the existing `Strict`
        cell text unchanged — the cell value itself must be the new
        sub-class name.
      - Update the headline count. Since all three Strict findings
        after the BOM fix are security-hardened, the new headline
        reads: "9 Lenient findings, 0 Strict findings (bug-class), 3
        Strict (security-hardened) findings, total 12 entries." The
        intent is that the "still to fix" Strict count and the
        "deliberate divergence" Strict count are visibly separated.
- [x] Update `rlsp-yaml-parser/docs/feature-log.md`: add an entry (or
      extend an existing encoding/security entry) documenting the
      hex-escape security hardening as a deliberate divergence from
      YAML 1.2.2 §5.7. Cite the two source locations
      (`quoted.rs:594-606` and `quoted.rs:608-618`) and note that
      named escapes are exempt by design.
- [x] Remove the stale follow-up queue entry. In
      `.ai/memory/project_followup_plans.md`, delete the
      `[Strict] Hex escape codepoint validation ([59], [60], [61])`
      bullet — the item was filed as remediation work but this plan
      records the strictness as deliberate policy, so the item is no
      longer open. The entry's cross-reference ("one root cause in
      `chars.rs`") is also inaccurate (actual rejection is at
      `lexer/quoted.rs:594-618`); removing the whole entry resolves
      both issues.
- [x] No source code is modified.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets` run clean.

## Decisions

- **Strict is split into `Strict` (bug) and `Strict (security-hardened)`
  (deliberate).** This lets future audits and remediation plans triage
  Strict findings without re-reading each Discrepancy to infer intent.
  The default `Strict` remains "bug or undecided."
- **`Rationale` field is mandatory for security-hardened entries only.**
  Making it optional for other classifications keeps the format tight
  while preventing the security-hardened sub-class from being used
  casually — every such entry must cite source comments or docs that
  demonstrate the policy is real, not after-the-fact rationalization.
- **No parser behavior change in this plan.** This is an audit-doc and
  feature-log update, not a code fix. The user explicitly chose "keep
  hardening, reclassify as deliberate divergence" when presented with
  the remediation options for Group 2 of the audit findings.
- **The bidi-control rejection is folded into the same three entries,
  not a separate audit finding.** It applies to the same productions
  ([59]/[60]/[61]) and is gated on the same escape-prefix check, so
  splitting it into its own entries would duplicate content. The
  Discrepancy text and Rationale cover both layers.
- **Named-vs-hex asymmetry is documented, not reconciled.** The source
  comment already notes the exemption. The audit entries reference that
  comment rather than proposing a reconciliation — tightening named
  escapes would break valid YAML (spec explicitly names `\0` etc. as
  valid escapes).
