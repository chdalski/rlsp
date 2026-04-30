---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: Reconciliation
section: §9
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §9

10 entries reconciled. Both auditors agreed on all 10 verdicts; no disagreements to resolve.

## Final Verdict Tally

- `Strict-conformant`: 10
- `Stricter-than-spec`: 0
- `Lenient`: 0
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0

§9 is fully conformant. Both auditors converged independently on the same verdict for every production.

## Agreed Verdicts (10 entries)

| Production | Verdict |
|---|---|
| [202] l-document-prefix | Strict-conformant |
| [203] c-directives-end | Strict-conformant |
| [204] c-document-end | Strict-conformant |
| [205] l-document-suffix | Strict-conformant |
| [206] c-forbidden | Strict-conformant |
| [207] l-bare-document | Strict-conformant |
| [208] l-explicit-document | Strict-conformant |
| [209] l-directive-document | Strict-conformant |
| [210] l-any-document | Strict-conformant |
| [211] l-yaml-stream | Strict-conformant |

## Resolved Disagreements

None.

## Doc Errata (informational — propagates to final summary)

None. B explicitly noted "Conformance doc claims of 'Conformant' align with the code in every case; no doc disagreements found."

## Methodology Notes

- §9 is the smallest chapter (10 productions) and the only chapter with zero inter-auditor disagreements. Document/stream productions have well-defined boundaries and the parser's stream/document handling is consistent across both interpretations.
- The marker recogniser `is_marker` at `src/lexer.rs:544-565` enforces the [206] c-forbidden constraint directly (start-of-line + `---`/`...` + whitespace/EOL/EOF). The directive-without-marker rule is enforced at three sites in `directives.rs`. Empty explicit documents emit a borrowed-empty Plain scalar per [208]'s `e-node` alternative.
- Cumulative chapter result: §5 (62 of 64 SC), §6 (31 of 41 SC), §7 (57 of 58 SC, 3 [NEEDS USER REVIEW]), §8 (40 of 40 SC), §9 (10 of 10 SC). The total Strict-conformant rate is high; the Lenient findings are concentrated in §6 (8 of 41) plus 3 in §5 (carrying through as the c-printable family root cause).
